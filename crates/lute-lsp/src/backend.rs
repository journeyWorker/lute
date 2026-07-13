//! The `LanguageServer` backend (Task 6.1).
//!
//! Holds the open-document map and runs the shared [`lute_check::check`] core on
//! every open/change, republishing the result via `publishDiagnostics`. It is a
//! pass-through to `check()` — no validation logic lives here.
//!
//! ## Sync model
//! We advertise `TextDocumentSyncKind::FULL`: the client resends the entire
//! document on each edit, so `did_change` simply takes the last content-change's
//! `text` as the new snapshot. Incremental sync (range-scoped edits) is out of
//! scope for 6.1.
//!
//! ## The divergence invariant
//! [`analyze`](Backend::analyze) builds a `TextIndex` from the *same* document
//! text `check()` saw and maps each diagnostic's byte offsets through it (via
//! [`crate::convert::to_lsp_diagnostic`]). Since `check()` already re-derived every
//! span from its bytes through one shared `TextIndex`, the positions the LSP
//! publishes match the headless CLI byte-for-byte — the property Task 6.2's
//! golden asserts.

use std::path::{Path, PathBuf};

use dashmap::DashMap;
use lute_check::{
    check, check_cel_slot, parse_meta_kind, resolve_imports, translate_cel_parse, CheckInput,
    MetaKind, Mode,
};
use lute_core_span::{Diagnostic, Layer, Severity, Span, TextIndex};
use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::{
    CodeActionOrCommand, CodeActionParams, CodeActionProviderCapability, CodeActionResponse,
    CompletionOptions, CompletionParams, CompletionResponse, Diagnostic as LspDiagnostic,
    DiagnosticSeverity, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DocumentSymbolParams, DocumentSymbolResponse, FoldingRange,
    FoldingRangeParams, FoldingRangeProviderCapability, GotoDefinitionParams,
    GotoDefinitionResponse, Hover, HoverParams, HoverProviderCapability, InitializeParams,
    InitializeResult, Location, MessageType, OneOf, Position, Range, ReferenceParams,
    SemanticTokens, SemanticTokensFullOptions, SemanticTokensOptions, SemanticTokensParams,
    SemanticTokensResult, SemanticTokensServerCapabilities, ServerCapabilities, ServerInfo,
    TextDocumentSyncCapability, TextDocumentSyncKind, Uri, WorkDoneProgressOptions,
};
use tower_lsp_server::{Client, LanguageServer};

use crate::code_action;
use crate::convert::to_lsp_diagnostic;
use crate::features::{completion, folding, hover, nav, semtok, symbols};

/// The last-known text of an open document plus the LSP version that produced it.
/// Republished diagnostics are stamped with `version` so the client can discard
/// results for a superseded edit.
#[derive(Clone, Debug)]
pub struct DocumentSnapshot {
    /// Full document text (FULL sync: the whole buffer on every change).
    pub text: String,
    /// LSP document version (monotonic per the client; from didOpen/didChange).
    pub version: i32,
}

/// The Lute language server: an LSP client handle plus the concurrent map of open
/// documents keyed by their [`Uri`].
#[derive(Debug)]
pub struct Backend {
    client: Client,
    docs: DashMap<Uri, DocumentSnapshot>,
    /// The ORIGINAL `Vec<Diagnostic>` (fixits + `covered` intact) the last
    /// `analyze`/`analyze_declaration` run produced for each open document —
    /// kept beside the published LSP `Diagnostic`s (which drop `fixits`
    /// entirely, `crate::convert`'s doc comment). `textDocument/codeAction`
    /// (Task 15) has no other way to recover a fixit: the wire-form
    /// `Diagnostic` the client echoes back in `CodeActionContext` never
    /// carried one. Cleared on `did_close` alongside `docs`.
    diagnostics: DashMap<Uri, Vec<Diagnostic>>,
}

impl Backend {
    /// Build a backend bound to `client` with empty document/diagnostic maps.
    pub fn new(client: Client) -> Self {
        Self {
            client,
            docs: DashMap::new(),
            diagnostics: DashMap::new(),
        }
    }

    /// Run `check()` over `snapshot`'s text and publish the converted diagnostics
    /// for `uri`, stamped with the snapshot version. Positions are derived exactly
    /// as the headless path derives them (see the divergence invariant above).
    ///
    /// Project-resolution diagnostics (a broken plugin graph above the document:
    /// load/cycle/unresolved-depends/assembly errors) are surfaced too, each as an
    /// Error at the document start — otherwise a scene that is itself clean would
    /// silently validate against a broken project (plugin §11).
    async fn analyze(&self, uri: Uri, snapshot: &DocumentSnapshot) {
        // B3 (data-catalog foundation 0.3.0): a project declaration `.yaml`
        // (under the project's `schema:`/`catalog:` dir) is a pure declaration
        // map, not a `.lute` scene — it has no body for `check()` to walk. Claim
        // it here and run the declaration-specific semantic pass instead; every
        // other document (every `.lute`, and any `.yaml` NOT under those dirs)
        // keeps today's `check()` walk below, unchanged.
        if let Some(path) = uri_to_path(&uri) {
            if let Some(root) = claimed_declaration_yaml(&path) {
                self.analyze_declaration(uri, snapshot, &path, &root).await;
                return;
            }
        }
        let (cap, providers, rdiags) = self.snapshot_for(&uri, &snapshot.text);
        // Resolve `uses:` schema imports (dsl §9.2) with the SAME resolver the
        // editor features use (`imports_for`), so diagnostics and features never
        // disagree on the imported schema. A non-file uri resolves to no imports.
        let imports = self.imports_for(&uri, &snapshot.text);
        // Resolve `components:` component imports (dsl §13) from the same scene
        // directory, mirroring `imports_for`, so `::use` validates identically on
        // both surfaces.
        let components = self.components_for(&uri, &snapshot.text);
        let input = CheckInput {
            text: snapshot.text.clone(),
            uri: uri.as_str().to_string(),
            snapshot: cap,
            providers,
            mode: Mode::Author,
            imports,
            components,
        };
        let result = check(&input);
        // Task 15: retain the ORIGINAL diagnostics (fixits/covered intact)
        // beside the published LSP form for `code_action` to read back later.
        self.diagnostics.insert(uri.clone(), result.diagnostics.clone());
        let idx = lute_core_span::TextIndex::new(&snapshot.text);
        let mut diags: Vec<LspDiagnostic> = result
            .diagnostics
            .iter()
            .map(|d| to_lsp_diagnostic(d, &idx, &uri))
            .collect();
        diags.extend(rdiags.iter().map(resolve_diag_to_lsp));
        self.client
            .publish_diagnostics(uri, diags, Some(snapshot.version))
            .await;
    }

    /// Analyze an open project declaration `.yaml`/`.yml` — state/defs/enums/
    /// entities under a project's `schema:`/`catalog:` dir ([`claimed_declaration_yaml`],
    /// data-catalog foundation B3) — and publish semantic diagnostics on the
    /// SAME file. A declaration has no body ([`schema_import`](lute_check::schema_import)'s
    /// module docs: "no `---` envelope, no body — the whole file IS the
    /// frontmatter"), so unlike [`analyze`](Self::analyze) this never calls
    /// `check()`; it drives the SAME building blocks `check()` drives for a
    /// scene's imported schema, applied to THIS file's own declaration:
    /// * [`parse_meta_kind`] (`MetaKind::Schema`) — the EXACT parse B2's
    ///   `schema_import::read_and_parse` uses for a `.yaml`/`.yml` import target
    ///   (a synthetic whole-file `Meta`, no `---` envelope) — reused verbatim so
    ///   a declaration parses identically whether opened directly or imported.
    /// * [`resolve_imports`] — this file's OWN `uses:`/`extends:` (dsl §9.2),
    ///   the SAME resolver [`imports_for`](Self::imports_for) runs for a scene,
    ///   so the state schema `defs:` CEL is checked against includes whatever
    ///   this declaration itself imports.
    /// * [`lute_check::schema_import::merge_domains`] — this file's own
    ///   `enums:`/`entities:` unioned with its imports, checked against the
    ///   project's active baseline (A4), catching an `E-DOMAIN-DUP` collision
    ///   exactly as `check()` does for a scene.
    /// * [`check_cel_slot`] — the checker's own CEL/path/`@ref` resolver (dsl
    ///   §8/§9), run once per `defs:` entry's `cel:` body against the merged
    ///   state/def tables above. `check()` itself does not yet drive this for
    ///   ANY document's `defs:` (`def_bodies` is still a D4 stub — see
    ///   `check.rs`'s `FoldedEnv::def_bodies` doc comment); the LSP is the
    ///   first caller, closing exactly the gap B3 exists to close.
    async fn analyze_declaration(
        &self,
        uri: Uri,
        snapshot: &DocumentSnapshot,
        file_path: &Path,
        project_root: &Path,
    ) {
        let idx = TextIndex::new(&snapshot.text);
        let whole = Span {
            byte_start: 0,
            byte_end: snapshot.text.len(),
            line: 1,
            column: 1,
            utf16_range: (0, 0),
        };
        let meta = lute_syntax::ast::Meta {
            raw_yaml: snapshot.text.clone(),
            span: whole,
        };
        let (typed, mut diags) = parse_meta_kind(
            &meta,
            &lute_manifest::snapshot::CapabilitySnapshot::default(),
            MetaKind::Schema,
        );

        let dir = file_path.parent().unwrap_or_else(|| Path::new("."));
        let imports = resolve_imports(dir, &typed.uses, &typed.extends, whole);
        diags.extend(imports.diags.clone());

        let project = match lute_manifest::project::load_project(project_root) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("lute-lsp: {e}");
                None
            }
        };
        let (baseline, rdiags) = lute_manifest::project::resolve_document_snapshot(
            project.as_ref(),
            typed.profile.as_deref(),
            &typed.plugins,
        );

        // Domain refs: this file's own `enums:`/`entities:` (not part of
        // `imports.domains`, which only covers files REACHED via `uses:`/
        // `extends:` — see `SchemaImports::domains`) unioned with its imports,
        // checked against the project's active baseline. On a same-name
        // collision the import-graph side wins (mirrors `resolve_imports`'s own
        // "shallower wins" convention: this file, depth 0, is the shallowest).
        let mut own_domains = imports.domains.clone();
        for (name, dom) in &typed.domains {
            own_domains.entry(name.clone()).or_insert_with(|| dom.clone());
        }
        let domain_imports = lute_check::SchemaImports {
            domains: own_domains,
            ..Default::default()
        };
        let (_domains, domain_diags) =
            lute_check::schema_import::merge_domains(&baseline, &domain_imports, whole);
        diags.extend(domain_diags);

        // The merged state schema `defs:` CEL paths resolve against: this
        // file's own inline `state:` overrides an imported decl of the same
        // path (mirrors `check.rs::fold_env`'s inline-over-imported precedence).
        let mut state = imports.state.clone();
        for (path, decl) in &typed.state.decls {
            state.decls.insert(path.clone(), decl.clone());
        }

        // The `@ref` existence/type tables `defs:` CEL bodies resolve `@name`
        // uses against: plugin/project baseline < imported < inline (same
        // precedence `check.rs::fold_env` uses for a scene's `defs`).
        let mut def_names: std::collections::BTreeSet<String> =
            baseline.defs.keys().cloned().collect();
        def_names.extend(imports.defs.keys().cloned());
        def_names.extend(typed.defs.keys().cloned());
        let mut def_types: std::collections::BTreeMap<String, lute_manifest::types::Type> =
            std::collections::BTreeMap::new();
        for (name, d) in &baseline.defs {
            def_types.insert(name.clone(), d.ty.clone());
        }
        for (name, v) in imports.defs.iter().chain(typed.defs.iter()) {
            if let Some(t) = v
                .get("type")
                .cloned()
                .and_then(|t| serde_yaml::from_value(t).ok())
            {
                def_types.insert(name.clone(), t);
            }
        }
        let env = lute_check::ctx::Env {
            mode: Mode::Author,
            state,
            defs: def_names,
            def_types,
            // Arity/arg-type checks (`E-REF-ARITY`/`E-REF-ARG-TYPE`) on a
            // `@ref(args)` USE inside a def's own `cel:` are conservatively
            // skipped (empty table => `check_cel_slot` silently omits them,
              // never a false positive) — parametrized-def bodies are rarer
            // than the path/undeclared-ref case B3's test targets, and B2's
            // `params_from_yaml` extractor is private to `check.rs`.
            def_params: std::collections::BTreeMap::new(),
            ..Default::default()
        };
        let ctx = lute_check::Ctx {
            env: &env,
            in_match: false,
            match_subject: None,
        };

        // Validate each `defs:` entry's own `cel:` body (dsl §8): CEL parse
        // validity, `@ref`/state-path resolution, and — when the def declares a
        // `type:` — that the body's produced type is compatible with it.
        let mut arena = lute_cel::CelArena::default();
        for (name, val) in &typed.defs {
            let Some(raw) = val.get("cel").and_then(|c| c.as_str()) else {
                continue;
            };
            let span = find_key_span(&snapshot.text, name).unwrap_or(whole);
            let mut slot = lute_syntax::ast::CelSlot::raw(
                lute_syntax::ast::CelKind::SetExpr,
                raw.to_string(),
                span,
            );
            match lute_cel::parse_slot(&mut arena, raw, span.byte_start) {
                Ok(handle) => slot.ast = Some(handle),
                Err(e) => {
                    // §8.1 (folded from Task 13): route through the SAME
                    // writer-voiced translation `check.rs`'s main E-CEL-PARSE
                    // site uses, instead of building the message from `e`'s
                    // raw backend text — the leak T13 flagged as out-of-scope
                    // for its own crate (`lute-check` has no declaration-path
                    // caller). `translate_cel_parse` is used ONLY for its
                    // `t.message` (the ANTLR-free win) — NEVER for `t.span`/
                    // `t.fixits`: both are offsets into the DECODED `raw`,
                    // which do not map back to source bytes for escaped or
                    // folded scalars (a decoded->source offset map is out of
                    // scope). The diagnostic's own span instead comes from
                    // `find_def_cel_value_span` — a YAML-aware locator that
                    // resolves the WHOLE `cel:` scalar VALUE span in source
                    // (finding 7: previously this anchored on `span`, the
                    // DEF-NAME key span, landing the squiggle on the wrong
                    // token) — falling back to the honest key `span` when
                    // the locator can't confidently resolve the value (e.g.
                    // a `|`/`>` block scalar). Fixits stay empty either way:
                    // no source-accurate fixit edit is recoverable from a
                    // decoded-offset translation.
                    let t = translate_cel_parse(raw, span, &e);
                    let cel_span = find_def_cel_value_span(&snapshot.text, name).unwrap_or(span);
                    diags.push(Diagnostic {
                        code: "E-CEL-PARSE".to_string(),
                        severity: Severity::Error,
                        message: t.message,
                        span: cel_span,
                        layer: Layer::Cel,
                        fixits: Vec::new(),
                        provenance: None,
                        covered: Vec::new(),
                    })
                }
            }
            let expected = val
                .get("type")
                .cloned()
                .and_then(|t| serde_yaml::from_value::<lute_manifest::types::Type>(t).ok())
                .map(lute_check::ctx::ExpectedType::Ty);
            diags.extend(check_cel_slot(&slot, &arena, &ctx, expected.as_ref()));
        }

        self.diagnostics.insert(uri.clone(), diags.clone());
        let mut lsp_diags: Vec<LspDiagnostic> =
            diags.iter().map(|d| to_lsp_diagnostic(d, &idx, &uri)).collect();
        lsp_diags.extend(rdiags.iter().map(resolve_diag_to_lsp));
        self.client
            .publish_diagnostics(uri, lsp_diags, Some(snapshot.version))
            .await;
    }

    /// Resolve a document's `uses:` schema imports (dsl §9.2) relative to its
    /// directory — the SINGLE resolver shared by [`analyze`](Self::analyze) and
    /// the four editor-feature handlers, so the diagnostics surface and the
    /// editor features never disagree on the imported state/defs. A non-file uri
    /// (no filesystem parent) resolves to no imports (`SchemaImports::default`);
    /// any I/O/parse/cycle failure degrades to a best-effort result, never panics.
    fn imports_for(&self, uri: &Uri, text: &str) -> lute_check::SchemaImports {
        let (doc, _) = lute_syntax::parse(text);
        let (meta0, _) = lute_check::parse_meta(
            &doc.meta,
            &lute_manifest::snapshot::CapabilitySnapshot::default(),
        );
        uri_to_path(uri)
            .and_then(|p| {
                p.parent().map(|d| {
                    lute_check::resolve_imports(d, &meta0.uses, &meta0.extends, doc.meta.span)
                })
            })
            .unwrap_or_default()
    }

    /// Resolve a document's `components:` component imports (dsl §13) relative to
    /// its directory — the analyze-side analog of [`imports_for`](Self::imports_for),
    /// so `::use` invocations validate against the SAME component table the CLI
    /// (`main.rs`) resolves. A non-file uri (no filesystem parent) resolves to no
    /// components (`ComponentSet::default`); any I/O/parse/cycle failure degrades
    /// to a best-effort result, never panics.
    fn components_for(&self, uri: &Uri, text: &str) -> lute_check::ComponentSet {
        let (doc, _) = lute_syntax::parse(text);
        let (meta0, _) = lute_check::parse_meta(
            &doc.meta,
            &lute_manifest::snapshot::CapabilitySnapshot::default(),
        );
        uri_to_path(uri)
            .and_then(|p| {
                p.parent()
                    .map(|d| lute_check::resolve_components(d, &meta0.components, doc.meta.span))
            })
            .unwrap_or_default()
    }

    /// The current full text of the open document `uri`, or `None` if it is not
    /// open. Cloned so the feature call runs without holding the `DashMap` guard.
    fn document_text(&self, uri: &Uri) -> Option<String> {
        self.docs.get(uri).map(|d| d.text.clone())
    }

    /// Build the capability snapshot for `uri`'s document by discovering a
    /// `lute.project.yaml` above the file and resolving through the SHARED
    /// [`resolve_document_snapshot`](lute_manifest::project::resolve_document_snapshot)
    /// — the *identical* resolution the CLI runs (plugin §11), so the two
    /// surfaces build byte-identical snapshots and cannot diverge.
    ///
    /// The scene's frontmatter `profile`/`plugins` are lifted with a default
    /// snapshot (both are built-in, not capability-gated). When no project is
    /// found above the document, `resolve_document_snapshot(None, ..)` yields the
    /// core-only baseline — today's behavior. A malformed project is logged and
    /// falls back to core-only rather than silently mis-validating (never panics).
    fn snapshot_for(
        &self,
        uri: &Uri,
        text: &str,
    ) -> (
        lute_manifest::snapshot::CapabilitySnapshot,
        lute_manifest::provider::ProviderSet,
        Vec<lute_manifest::project::ResolveDiag>,
    ) {
        let (doc, _) = lute_syntax::parse(text);
        let (meta0, _) = lute_check::parse_meta(
            &doc.meta,
            &lute_manifest::snapshot::CapabilitySnapshot::default(),
        );
        let project = uri_to_path(uri)
            .and_then(|p| find_project_root(&p))
            .and_then(|root| match lute_manifest::project::load_project(&root) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("lute-lsp: {e}");
                    None
                }
            });
        // Load the project's pinned provider catalog through the SAME shared
        // helper the CLI uses when `--providers` is absent, so the editor
        // resolves provider ids identically to the headless build (plugin §10).
        let providers = lute_manifest::project::project_providers(project.as_ref());
        let (snapshot, rdiags) = lute_manifest::project::resolve_document_snapshot(
            project.as_ref(),
            meta0.profile.as_deref(),
            &meta0.plugins,
        );
        (snapshot, providers, rdiags)
    }
}

impl LanguageServer for Backend {
    async fn initialize(&self, _params: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                // FULL document sync + publishDiagnostics (6.1) retained.
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                // 6.3 editor features. Trigger chars fire completion where the
                // resolver keys off punctuation: `::` (directive head), `@` (a
                // CEL `@ref`), `{` (a directive attr area), `.` (a state path).
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(
                        [":", "@", "{", "."].iter().map(|s| s.to_string()).collect(),
                    ),
                    ..Default::default()
                }),
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                // 6.4 structure features. Folding + document symbols are simple
                // providers; semantic tokens advertise the full-document legend
                // (the closed layer set) that the delta stream is decoded against.
                folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            work_done_progress_options: WorkDoneProgressOptions::default(),
                            legend: semtok::legend(),
                            range: Some(false),
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                        },
                    ),
                ),
                // Task 15 (D16): quick fixes over `Diagnostic.fixits` — the
                // ONLY surface an author can apply a `W-CHOICE-INTO-NO-PERSIST`
                // remedy or a §8.1 T2 CEL rewrite through (`lute fix` never
                // reads checker diagnostics, by construction). `Simple(true)`:
                // this server returns only plain `CodeAction`s, no `Command`s,
                // so it advertises no `code_action_kinds` allowlist.
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "lute-lsp".into(),
                version: Some(env!("CARGO_PKG_VERSION").into()),
            }),
            ..Default::default()
        })
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let pos = params.text_document_position_params;
        let Some(text) = self.document_text(&pos.text_document.uri) else {
            return Ok(None);
        };
        let (doc, _) = lute_syntax::parse(&text);
        let snapshot = self.snapshot_for(&pos.text_document.uri, &text).0;
        let imports = self.imports_for(&pos.text_document.uri, &text);
        let off = position_to_byte(&text, pos.position);
        Ok(hover::hover_at(&doc, &snapshot, &imports, off))
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let pos = params.text_document_position;
        let Some(text) = self.document_text(&pos.text_document.uri) else {
            return Ok(None);
        };
        let (doc, _) = lute_syntax::parse(&text);
        let (snapshot, providers, _) = self.snapshot_for(&pos.text_document.uri, &text);
        let imports = self.imports_for(&pos.text_document.uri, &text);
        let off = position_to_byte(&text, pos.position);
        let items = completion::complete_at(&doc, &snapshot, &providers, &imports, off);
        if items.is_empty() {
            return Ok(None);
        }
        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let pos = params.text_document_position_params;
        let uri = pos.text_document.uri;
        let Some(text) = self.document_text(&uri) else {
            return Ok(None);
        };
        let (doc, _) = lute_syntax::parse(&text);
        let snapshot = self.snapshot_for(&uri, &text).0;
        let imports = self.imports_for(&uri, &text);
        let idx = TextIndex::new(&text);
        let off = position_to_byte(&text, pos.position);
        Ok(
            nav::definition_at(&doc, &snapshot, &imports, off).map(|span| {
                GotoDefinitionResponse::Scalar(Location {
                    uri,
                    range: span_to_range(&span, &idx),
                })
            }),
        )
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let pos = params.text_document_position;
        let uri = pos.text_document.uri;
        let Some(text) = self.document_text(&uri) else {
            return Ok(None);
        };
        let (doc, _) = lute_syntax::parse(&text);
        let snapshot = self.snapshot_for(&uri, &text).0;
        let imports = self.imports_for(&uri, &text);
        let idx = TextIndex::new(&text);
        let off = position_to_byte(&text, pos.position);
        let locs: Vec<Location> = nav::references_at(
            &doc,
            &snapshot,
            &imports,
            off,
            params.context.include_declaration,
        )
        .into_iter()
        .map(|span| Location {
            uri: uri.clone(),
            range: span_to_range(&span, &idx),
        })
        .collect();
        if locs.is_empty() {
            return Ok(None);
        }
        Ok(Some(locs))
    }

    async fn folding_range(&self, params: FoldingRangeParams) -> Result<Option<Vec<FoldingRange>>> {
        let Some(text) = self.document_text(&params.text_document.uri) else {
            return Ok(None);
        };
        let (doc, _) = lute_syntax::parse(&text);
        let idx = TextIndex::new(&text);
        Ok(Some(folding::folding_ranges(&doc, &idx)))
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let Some(text) = self.document_text(&params.text_document.uri) else {
            return Ok(None);
        };
        let (doc, _) = lute_syntax::parse(&text);
        let idx = TextIndex::new(&text);
        let data = semtok::semantic_tokens(&doc, &idx);
        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data,
        })))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let Some(text) = self.document_text(&params.text_document.uri) else {
            return Ok(None);
        };
        let (doc, _) = lute_syntax::parse(&text);
        let idx = TextIndex::new(&text);
        Ok(Some(DocumentSymbolResponse::Nested(
            symbols::document_symbols(&doc, &idx),
        )))
    }

    /// `textDocument/codeAction` (Task 15, D16): map the cached ORIGINAL
    /// diagnostics' `fixits` (the published LSP diagnostics dropped them,
    /// `crate::convert`'s doc comment) that overlap `params.range` to
    /// `CodeAction`s, through [`code_action::code_actions_for_fixits`]. `None`
    /// when the document isn't open, has no cached diagnostics yet (never
    /// analyzed), or none overlap with a fixit — never an empty `Some(vec![])`.
    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = params.text_document.uri;
        let Some(text) = self.document_text(&uri) else {
            return Ok(None);
        };
        let Some(diags) = self.diagnostics.get(&uri) else {
            return Ok(None);
        };
        let idx = TextIndex::new(&text);
        let actions = code_action::code_actions_for_fixits(&diags, &uri, params.range, &idx);
        if actions.is_empty() {
            return Ok(None);
        }
        Ok(Some(
            actions
                .into_iter()
                .map(CodeActionOrCommand::CodeAction)
                .collect(),
        ))
    }

    async fn initialized(&self, _params: tower_lsp_server::ls_types::InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "lute-lsp initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let doc = params.text_document;
        let snapshot = DocumentSnapshot {
            text: doc.text,
            version: doc.version,
        };
        self.docs.insert(doc.uri.clone(), snapshot.clone());
        self.analyze(doc.uri, &snapshot).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        // FULL sync: the final content change carries the whole new document.
        let Some(change) = params.content_changes.into_iter().next_back() else {
            return;
        };
        let uri = params.text_document.uri;
        let snapshot = DocumentSnapshot {
            text: change.text,
            version: params.text_document.version,
        };
        self.docs.insert(uri.clone(), snapshot.clone());
        self.analyze(uri, &snapshot).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        self.docs.remove(&uri);
        self.diagnostics.remove(&uri);
        // LSP diagnostics are server-owned and persist in the client until the
        // server replaces them. The buffer is gone, so publish an empty set to
        // clear any squiggles the last analyze() left behind (no version stamp:
        // the document has no live version once closed).
        self.client.publish_diagnostics(uri, Vec::new(), None).await;
    }
}

/// Convert an LSP [`Position`] (0-based line, 0-based UTF-16 character) to a byte
/// offset into `text`. The inverse of [`TextIndex::position`]: locate the line by
/// counting `\n`s, then walk that line's chars summing `len_utf16` until the
/// UTF-16 column is reached — the exact per-line UTF-16 accounting `TextIndex`
/// does forward, so the LSP and headless surfaces never drift a code unit.
/// A `character` past the line end clamps to the line end (defensive; LSP clients
/// occasionally over-report a column at EOL).
fn position_to_byte(text: &str, pos: Position) -> usize {
    let bytes = text.as_bytes();
    // Byte offset of the start of line `pos.line` (0-based).
    let mut line = 0u32;
    let mut line_start = 0usize;
    if pos.line > 0 {
        for (i, b) in bytes.iter().enumerate() {
            if *b == b'\n' {
                line += 1;
                if line == pos.line {
                    line_start = i + 1;
                    break;
                }
            }
        }
        if line < pos.line {
            // Line beyond EOF: clamp to end of text.
            return text.len();
        }
    }
    // Walk the target line, summing UTF-16 units until `pos.character`.
    let line_end = text[line_start..]
        .find('\n')
        .map_or(text.len(), |n| line_start + n);
    let mut utf16 = 0u32;
    for (off, ch) in text[line_start..line_end].char_indices() {
        if utf16 >= pos.character {
            return line_start + off;
        }
        utf16 += ch.len_utf16() as u32;
    }
    line_end
}

/// Map a byte [`Span`] to an LSP [`Range`] through `idx`. Mirrors
/// [`crate::convert`]'s private byte-span mapping (`TextIndex::position`,
/// de-1-indexing the line, 0-based UTF-16 column) so navigation results carry the
/// same UTF-16-correct positions as diagnostics. Byte-only spans synthesized by
/// the feature layer (zeroed line/col) resolve correctly here because the mapping
/// only reads `byte_start`/`byte_end`.
pub(crate) fn span_to_range(span: &Span, idx: &TextIndex) -> Range {
    Range {
        start: byte_to_position(span.byte_start, idx),
        end: byte_to_position(span.byte_end, idx),
    }
}

pub(crate) fn byte_to_position(byte: usize, idx: &TextIndex) -> Position {
    let p = idx.position(byte);
    Position {
        line: p.line - 1,
        character: p.utf16_col,
    }
}

/// Convert a project-resolution diagnostic (a broken plugin graph above the
/// document) into an LSP diagnostic. Resolver diagnostics have no source span, so
/// they anchor at the document start (line 0, char 0) as an Error sourced "lute"
/// — matching the CLI, which already surfaces the same `ResolveDiag` messages to
/// stderr. This is the seam that keeps the LSP from silently dropping them.
fn resolve_diag_to_lsp(d: &lute_manifest::project::ResolveDiag) -> LspDiagnostic {
    let start = Position {
        line: 0,
        character: 0,
    };
    LspDiagnostic {
        range: Range { start, end: start },
        severity: Some(DiagnosticSeverity::ERROR),
        source: Some("lute".into()),
        code: Some(tower_lsp_server::ls_types::NumberOrString::String(
            d.code.clone(),
        )),
        message: d.message.clone(),
        ..Default::default()
    }
}

/// Resolve a document [`Uri`] to a filesystem [`PathBuf`]. Only a `file` URI maps
/// to a real path: [`Uri::to_file_path`] does NOT check the scheme (verified — it
/// returns `Some` for `untitled:`/`vscode-vfs:` too), so guard it explicitly. A
/// non-file (virtual/unsaved) document returns `None`, and snapshot resolution
/// falls back to core-only. Schemes are case-insensitive (RFC 3986 §3.1). Ownership
/// is taken (`Cow` → `PathBuf`) so the path outlives the borrow of `uri`.
fn uri_to_path(uri: &Uri) -> Option<PathBuf> {
    if !uri.scheme().as_str().eq_ignore_ascii_case("file") {
        return None;
    }
    uri.to_file_path().map(|p| p.into_owned())
}

/// Walk up from the document at `file_path`, returning the first ancestor
/// directory that contains a `lute.project.yaml` (plugin §11 project discovery).
/// Starts at the file's parent directory and climbs to the filesystem root;
/// `None` when no project is found (a loose scene → core-only). Purely lexical
/// on the path components, so it works for buffers not yet written to disk.
fn find_project_root(file_path: &Path) -> Option<PathBuf> {
    let mut dir = file_path.parent();
    while let Some(d) = dir {
        if d.join("lute.project.yaml").is_file() {
            return Some(d.to_path_buf());
        }
        dir = d.parent();
    }
    None
}

/// Whether `file_path` is a project declaration `.yaml`/`.yml` the LSP claims
/// for semantic linting (data-catalog foundation B3): a YAML file under the
/// discovered project's `schema/` or `catalog/` subdirectory. Returns the
/// project root on a claim (the caller needs it again for baseline
/// resolution), `None` otherwise — so `lute.project.yaml` itself (sits at the
/// project root, not under either subdir) and any unrelated `.yaml` (CI
/// configs, ...) are never claimed; `.lute` handling is untouched (this gate
/// is checked ONLY when the extension is `.yaml`/`.yml`). Per the design
/// notes' "simplest robust rule" guidance: prefer the two conventional
/// declaration dirs over parsing every scene's `uses:`/`extends:` project-wide
/// to find which files are import-reachable.
fn claimed_declaration_yaml(file_path: &Path) -> Option<PathBuf> {
    let is_yaml = matches!(
        file_path.extension().and_then(|e| e.to_str()),
        Some("yaml") | Some("yml")
    );
    if !is_yaml {
        return None;
    }
    let root = find_project_root(file_path)?;
    let rel = file_path.strip_prefix(&root).ok()?;
    let first = rel.components().next()?;
    matches!(first.as_os_str().to_str(), Some("schema") | Some("catalog")).then_some(root)
}

/// Best-effort span of the mapping key `key` anywhere in whole-file YAML
/// `text` (a claimed declaration document — no `---` envelope, so byte 0 IS
/// the frontmatter start; unlike `features::find_yaml_key_span`'s `.lute`
/// `+4`-past-`"---\n"` convention). Matches only where `key` sits at a line
/// start (after indent) immediately followed by `:`, so a same-named
/// substring inside a value never steals the span — mirrors
/// `lute_check::meta`'s private `meta_key_span` (documented there as "kept in
/// sync" with `lute_lsp`'s own copy; this is a THIRD context — whole-file, no
/// envelope — so it gets its own copy rather than reusing either private
/// original). `None` when `key` never appears as a mapping key; callers fall
/// back to the whole-file span.
fn find_key_span(text: &str, key: &str) -> Option<Span> {
    let mut line_start = 0usize;
    for line in text.split_inclusive('\n') {
        let indent = line.len() - line.trim_start().len();
        if let Some(rest) = line.trim_start().strip_prefix(key) {
            if rest.trim_start().starts_with(':') {
                let start = line_start + indent;
                return Some(Span {
                    byte_start: start,
                    byte_end: start + key.len(),
                    line: 0,
                    column: 0,
                    utf16_range: (0, 0),
                });
            }
        }
        line_start += line.len();
    }
    None
}

/// Byte offset where the line containing `byte` begins.
fn line_start(text: &str, byte: usize) -> usize {
    text[..byte].rfind('\n').map(|p| p + 1).unwrap_or(0)
}

/// Byte offset one past the end of the line containing `byte` (i.e. right
/// after its trailing `\n`, or `text.len()` on the last line).
fn line_end(text: &str, byte: usize) -> usize {
    text[byte..].find('\n').map(|p| byte + p + 1).unwrap_or(text.len())
}

/// End of the indented block that opens right after `start` and is nested
/// under a parent key at `parent_indent`: the first byte offset, from
/// `start`, of a non-blank non-comment line whose OWN indent is `<=
/// parent_indent` (a sibling or dedented line), or `text.len()` if the
/// block runs to EOF. Blank/comment-only lines never end the block.
fn find_block_end(text: &str, start: usize, parent_indent: usize) -> usize {
    let mut pos = start;
    while pos < text.len() {
        let le = line_end(text, pos);
        let line = &text[pos..le];
        let content = line.trim_start_matches(' ');
        let indent = line.len() - content.len();
        let trimmed = content.trim_end();
        if !trimmed.is_empty() && !trimmed.starts_with('#') && indent <= parent_indent {
            return pos;
        }
        pos = le;
    }
    text.len()
}

/// Scoped, YAML-aware search for a DIRECT child mapping key inside
/// `[region_start, region_end)` — like [`find_key_span`], but bounded to
/// one block's own indent level so a same-named key belonging to an
/// unrelated mapping elsewhere in the file is never visited.
/// `region_start`/`region_end` MUST bound exactly one mapping's body (e.g.
/// via [`find_block_end`]). The block's own child indent is established
/// from its FIRST non-blank/non-comment line; only lines at that exact
/// indent are considered keys (deeper lines are a previous child's nested
/// content, skipped). Returns `(key_byte_start, colon_byte_offset,
/// child_indent)` for the first match.
fn find_scoped_child_key(
    text: &str,
    region_start: usize,
    region_end: usize,
    key: &str,
) -> Option<(usize, usize, usize)> {
    let mut pos = region_start;
    let mut child_indent: Option<usize> = None;
    while pos < region_end {
        let le = line_end(text, pos).min(region_end);
        let line = &text[pos..le];
        let content = line.trim_start_matches(' ');
        let line_indent = line.len() - content.len();
        let trimmed = content.trim_end();
        if !trimmed.is_empty() && !trimmed.starts_with('#') {
            let target_indent = *child_indent.get_or_insert(line_indent);
            if line_indent == target_indent {
                if let Some(rest) = trimmed.strip_prefix(key) {
                    let rest_trimmed = rest.trim_start();
                    if rest_trimmed.starts_with(':') {
                        let key_start = pos + line_indent;
                        let colon_off = key_start + key.len() + (rest.len() - rest_trimmed.len());
                        return Some((key_start, colon_off, line_indent));
                    }
                }
            }
        }
        pos = le;
    }
    None
}

/// Index right after the double-quoted YAML scalar starting at `text[start]
/// == '"'`, skipping `\`-escaped characters so an escaped inner quote
/// (`\"`) never ends the scan early. `None` if unterminated.
fn skip_double_quoted(text: &str, start: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'"' => return Some(i + 1),
            _ => i += 1,
        }
    }
    None
}

/// Index right after the single-quoted YAML scalar starting at `text[start]
/// == '\''`, treating a doubled `''` as an escaped literal quote (YAML's
/// single-quote escape — no backslash escapes in this style). `None` if
/// unterminated.
fn skip_single_quoted(text: &str, start: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut i = start + 1;
    while i < bytes.len() {
        if bytes[i] == b'\'' {
            if bytes.get(i + 1) == Some(&b'\'') {
                i += 2;
                continue;
            }
            return Some(i + 1);
        }
        i += 1;
    }
    None
}

/// Index of the `}` matching the `{` at `text[open_abs]`, skipping quoted
/// scalar interiors (so a `}`/`{` inside a CEL string literal is never
/// mistaken for flow-mapping structure) and nested flow mappings. `None` if
/// unterminated.
fn find_matching_brace(text: &str, open_abs: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut i = open_abs + 1;
    let mut depth = 1;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => i = skip_double_quoted(text, i)?,
            b'\'' => i = skip_single_quoted(text, i)?,
            b'{' => {
                depth += 1;
                i += 1;
            }
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    None
}

/// The byte range, bounded by `region_end`, that holds def entry `name`'s
/// own body — everything belonging to THIS entry, never a sibling's. Flow
/// form (`name: { ... }`) resolves via brace matching (the returned range
/// sits strictly inside the braces); block form (`name:\n  cel: ...`)
/// resolves via indentation (every following line more-indented than
/// `entry_indent`). `None` when the value right after `name:`'s colon is
/// neither `{` nor blank/comment (e.g. a bare scalar) — that shape isn't
/// the `type:`/`cel:` mapping this locator expects, so the caller must fall
/// back rather than guess.
fn find_entry_extent(
    text: &str,
    colon_abs: usize,
    key_line_end: usize,
    entry_indent: usize,
    region_end: usize,
) -> Option<(usize, usize)> {
    let after_colon = colon_abs + 1;
    let rest_of_line = &text[after_colon..key_line_end];
    let trimmed = rest_of_line.trim_start_matches(' ');
    let value_col_start = after_colon + (rest_of_line.len() - trimmed.len());
    let trimmed_no_nl = trimmed.trim_end();
    if trimmed_no_nl.starts_with('{') {
        let close = find_matching_brace(text, value_col_start)?;
        if close >= region_end {
            return None;
        }
        Some((value_col_start + 1, close))
    } else if trimmed_no_nl.is_empty() || trimmed_no_nl.starts_with('#') {
        let mut pos = key_line_end;
        let mut end = key_line_end;
        while pos < region_end {
            let le = line_end(text, pos).min(region_end);
            let line = &text[pos..le];
            let content = line.trim_start_matches(' ');
            let line_indent = line.len() - content.len();
            if content.trim_end().is_empty() {
                pos = le;
                continue;
            }
            if line_indent <= entry_indent {
                break;
            }
            end = le;
            pos = le;
        }
        Some((key_line_end, end))
    } else {
        None
    }
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Locates `key`'s `:` inside `[region.0, region.1)`, skipping the interior
/// of any single/double-quoted scalar so CEL text that happens to contain a
/// `"cel:"`-shaped substring can never be mistaken for the real key.
/// Requires a whole-word match on both sides (`concel:` never matches
/// `cel`). Returns the absolute byte offset of the `:` character.
fn find_unquoted_key_colon(text: &str, region: (usize, usize), key: &str) -> Option<usize> {
    let (start, end) = region;
    let bytes = text.as_bytes();
    let mut i = start;
    while i < end {
        match bytes[i] {
            b'"' => i = skip_double_quoted(text, i)?,
            b'\'' => i = skip_single_quoted(text, i)?,
            b if is_word_byte(b) => {
                let word_start = i;
                let mut j = i;
                while j < end && is_word_byte(bytes[j]) {
                    j += 1;
                }
                if &text[word_start..j] == key {
                    let after = &text[j..end];
                    let after_trimmed = after.trim_start_matches(' ');
                    if after_trimmed.starts_with(':') {
                        return Some(j + (after.len() - after_trimmed.len()));
                    }
                }
                i = j;
            }
            _ => i += 1,
        }
    }
    None
}

fn mk_span(byte_start: usize, byte_end: usize) -> Span {
    Span { byte_start, byte_end, line: 0, column: 0, utf16_range: (0, 0) }
}

/// The whole scalar VALUE span (quotes included) for a `key:` whose `:`
/// sits at `colon_abs`, bounded by `region_end`. Handles double-quoted
/// (`\`-escaped), single-quoted (`''`-escaped), and bare/plain flow scalars
/// (terminated by `,`, `}`, or end of line). Declines (`None`) on a
/// literal/folded block scalar (`|`/`>`) — its DECODED content has no
/// simple byte-for-byte map back to source (dsl §8.1's decoded-vs-source
/// gap), so callers must fall back rather than guess at an un-mappable
/// multi-line span.
fn parse_scalar_value_span(text: &str, colon_abs: usize, region_end: usize) -> Option<Span> {
    let after = colon_abs + 1;
    if after > region_end {
        return None;
    }
    let rest = &text[after..region_end];
    let lead = rest.len() - rest.trim_start_matches(' ').len();
    let val_start = after + lead;
    if val_start >= region_end {
        return None;
    }
    let bytes = text.as_bytes();
    match bytes[val_start] {
        b'"' => {
            let end = skip_double_quoted(text, val_start)?;
            (end <= region_end).then(|| mk_span(val_start, end))
        }
        b'\'' => {
            let end = skip_single_quoted(text, val_start)?;
            (end <= region_end).then(|| mk_span(val_start, end))
        }
        b'|' | b'>' => None,
        _ => {
            let sub = &text[val_start..region_end];
            let end_rel = sub.find([',', '}', '\n']).unwrap_or(sub.len());
            let trimmed = sub[..end_rel].trim_end();
            (!trimmed.is_empty()).then(|| mk_span(val_start, val_start + trimmed.len()))
        }
    }
}

/// YAML-aware source-span locator scoped to `defs.<name>.cel` (dsl §8.1 —
/// closes the finding-7 gap `analyze_declaration`'s doc comment used to
/// flag): locates the entry for `name` under the top-level `defs:` mapping,
/// then the `cel:` key within THAT entry, then the whole scalar VALUE span
/// (quotes included) in source bytes. Handles inline/flow entries (`name: {
/// ..., cel: "..." }`), block entries (`name:\n  cel: "..."`), single/
/// double-quoted and `\`-escaped scalars, and a file where the same CEL
/// text is duplicated elsewhere (the search is scoped to `name`'s own
/// entry, so a duplicate can never steal the span). Returns `None` — never
/// a guessed or wrong span — on anything it can't confidently resolve (no
/// `defs:`, no entry for `name`, no `cel:` key in it, or a `cel:` value
/// shape it doesn't understand, e.g. a `|`/`>` block scalar): callers MUST
/// fall back to [`find_key_span`]'s key span in that case.
fn find_def_cel_value_span(text: &str, name: &str) -> Option<Span> {
    let defs_key = find_key_span(text, "defs")?;
    let defs_indent = defs_key.byte_start - line_start(text, defs_key.byte_start);
    let defs_body_start = line_end(text, defs_key.byte_start);
    let defs_body_end = find_block_end(text, defs_body_start, defs_indent);

    let (entry_key_start, entry_colon, entry_indent) =
        find_scoped_child_key(text, defs_body_start, defs_body_end, name)?;
    let entry_key_line_end = line_end(text, entry_key_start).min(defs_body_end);
    let (cel_region_start, cel_region_end) =
        find_entry_extent(text, entry_colon, entry_key_line_end, entry_indent, defs_body_end)?;

    let cel_colon = find_unquoted_key_colon(text, (cel_region_start, cel_region_end), "cel")?;
    parse_scalar_value_span(text, cel_colon, cel_region_end)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `position_to_byte` is the exact inverse of `TextIndex::position` on a
    /// multibyte document: every byte offset round-trips through its own Position.
    #[test]
    fn position_to_byte_round_trips_through_text_index() {
        let text = "## Shot 1.\n::café{x=\"π\"}\n:Ω: 世界\n";
        let idx = TextIndex::new(text);
        for byte in 0..=text.len() {
            // Only test char boundaries; a byte mid-char has no LSP position.
            if !text.is_char_boundary(byte) {
                continue;
            }
            let p = idx.position(byte);
            let pos = Position {
                line: p.line - 1,
                character: p.utf16_col,
            };
            assert_eq!(
                position_to_byte(text, pos),
                byte,
                "byte {byte} did not round-trip"
            );
        }
    }

    #[test]
    fn position_to_byte_clamps_out_of_range() {
        let text = "ab\ncd\n";
        // Character past line end clamps to the line end (before the `\n`).
        assert_eq!(
            position_to_byte(
                text,
                Position {
                    line: 0,
                    character: 99
                }
            ),
            2
        );
        // Line past EOF clamps to text end.
        assert_eq!(
            position_to_byte(
                text,
                Position {
                    line: 50,
                    character: 0
                }
            ),
            text.len()
        );
    }

    #[test]
    fn span_to_range_maps_utf16_columns() {
        let text = "π::x\n"; // `π` is 2 bytes, 1 UTF-16 unit.
        let idx = TextIndex::new(text);
        // Span over `::x` starts at byte 2 (after the 2-byte π) => UTF-16 col 1.
        let span = Span {
            byte_start: 2,
            byte_end: 5,
            line: 0,
            column: 0,
            utf16_range: (0, 0),
        };
        let range = span_to_range(&span, &idx);
        assert_eq!(
            range.start,
            Position {
                line: 0,
                character: 1
            }
        );
        assert_eq!(
            range.end,
            Position {
                line: 0,
                character: 4
            }
        );
    }

    /// S1: closing a document MUST publish an EMPTY diagnostics set for its URI.
    /// LSP diagnostics are server-owned and persist in the client until replaced,
    /// so without this the last analyze()'s squiggles linger after the buffer is
    /// gone. Drives a real `LspService<Backend>`: initialize -> didOpen (expect a
    /// non-empty publish) -> didClose (expect an empty publish for the same URI).
    #[tokio::test(flavor = "current_thread")]
    async fn did_close_publishes_empty_diagnostics() {
        use futures::StreamExt;
        use tower::{Service, ServiceExt};
        use tower_lsp_server::jsonrpc::Request as RpcRequest;
        use tower_lsp_server::LspService;

        let (mut service, mut socket) = LspService::new(Backend::new);
        let uri_str = "file:///t.lute";
        // A body line that matches no §4.3 rule → a guaranteed parse diagnostic.
        let text = "## Shot 1.\ngarbage prose line\n";

        let init = RpcRequest::build("initialize")
            .params(serde_json::json!({ "capabilities": {} }))
            .id(1)
            .finish();
        service.ready().await.unwrap().call(init).await.unwrap();

        let open = RpcRequest::build("textDocument/didOpen")
            .params(serde_json::json!({
                "textDocument": {
                    "uri": uri_str, "languageId": "lute", "version": 1, "text": text
                }
            }))
            .finish();
        service.ready().await.unwrap().call(open).await.unwrap();
        let opened = socket.next().await.expect("didOpen should publish");
        assert_eq!(opened.method(), "textDocument/publishDiagnostics");
        let odiags = opened
            .params()
            .and_then(|p| p.get("diagnostics"))
            .and_then(|d| d.as_array())
            .expect("publish carries a diagnostics array");
        assert!(
            !odiags.is_empty(),
            "an errored open doc should publish squiggles"
        );

        let close = RpcRequest::build("textDocument/didClose")
            .params(serde_json::json!({ "textDocument": { "uri": uri_str } }))
            .finish();
        service.ready().await.unwrap().call(close).await.unwrap();
        let closed = tokio::time::timeout(std::time::Duration::from_secs(2), socket.next())
            .await
            .expect("did_close must publish (empty) diagnostics; none arrived")
            .expect("socket closed without a close-publish");
        assert_eq!(closed.method(), "textDocument/publishDiagnostics");
        let cparams = closed.params().expect("close publish carries params");
        assert_eq!(
            cparams.get("uri").and_then(|u| u.as_str()),
            Some(uri_str),
            "close publish targets the closed URI"
        );
        let cdiags = cparams
            .get("diagnostics")
            .and_then(|d| d.as_array())
            .expect("close publish carries a diagnostics array");
        assert!(
            cdiags.is_empty(),
            "closing must clear diagnostics, got {cdiags:?}"
        );
    }

    #[test]
    fn uri_to_path_rejects_non_file_schemes() {
        use std::str::FromStr;
        // file: URI -> Some path
        let f = Uri::from_str("file:///tmp/x/doc.lute").unwrap();
        assert!(
            uri_to_path(&f).is_some(),
            "file: URI must resolve to a path"
        );
        // non-file (virtual/unsaved) schemes -> None (core-only fallback)
        for s in [
            "untitled:/repo/sub/doc.lute",
            "vscode-vfs://host/repo/doc.lute",
            "untitled:Untitled-1",
        ] {
            let u = Uri::from_str(s).unwrap();
            assert!(
                uri_to_path(&u).is_none(),
                "non-file URI {s:?} must NOT resolve to a filesystem path"
            );
        }
    }

    /// FINDING 1 guard: a document under a project whose plugin graph has a
    /// `DependsCycle` MUST publish that resolver diagnostic as an LSP diagnostic,
    /// even when the document itself is core-clean. Before the fix, `snapshot_for`
    /// discarded the resolver `Vec<ResolveDiag>`, so `analyze` never published it
    /// and the editor silently mis-validated against a broken project. Drives a
    /// real `LspService<Backend>` end to end: initialize -> didOpen a `file://`
    /// scene under a temp project with two mutually-depending plugins, then assert
    /// the published set carries a `DependsCycle` diagnostic sourced "lute" at the
    /// document start.
    #[tokio::test(flavor = "current_thread")]
    async fn analyze_publishes_project_resolver_diagnostics() {
        use futures::StreamExt;
        use std::fs;
        use tower::{Service, ServiceExt};
        use tower_lsp_server::jsonrpc::Request as RpcRequest;
        use tower_lsp_server::LspService;

        // Temp project with a plugin dependency cycle: a.x -> a.dep -> a.x.
        let root = std::env::temp_dir().join(format!("lute_lsp_cycle_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for (id, dep) in [("a.x", "a.dep"), ("a.dep", "a.x")] {
            let pdir = root.join("plugins").join(id);
            fs::create_dir_all(&pdir).unwrap();
            fs::write(
                pdir.join("plugin.yaml"),
                format!(
                    "id: {id}\nversion: 0.1.0\nkind: capability\ndepends: [ {{ id: {dep}, range: \"^0.1.0\" }} ]\nexports: {{}}\n"
                ),
            )
            .unwrap();
        }
        fs::write(
            root.join("lute.project.yaml"),
            "pluginsDir: plugins/\ndefaultProfile: s\nprofiles:\n  s:\n    plugins: { a.x: true, a.dep: true }\n",
        )
        .unwrap();
        let scene_path = root.join("scene.lute");
        let uri_str = format!("file://{}", scene_path.display());

        let (mut service, mut socket) = LspService::new(Backend::new);
        let init = RpcRequest::build("initialize")
            .params(serde_json::json!({ "capabilities": {} }))
            .id(1)
            .finish();
        service.ready().await.unwrap().call(init).await.unwrap();

        let open = RpcRequest::build("textDocument/didOpen")
            .params(serde_json::json!({
                "textDocument": {
                    "uri": uri_str, "languageId": "lute", "version": 1, "text": "## Shot 1.\n"
                }
            }))
            .finish();
        service.ready().await.unwrap().call(open).await.unwrap();
        let opened = socket.next().await.expect("didOpen should publish");
        assert_eq!(opened.method(), "textDocument/publishDiagnostics");
        let diags = opened
            .params()
            .and_then(|p| p.get("diagnostics").cloned())
            .and_then(|d| d.as_array().cloned())
            .expect("publish carries a diagnostics array");
        let resolver = diags.iter().find(|d| {
            d.get("message")
                .and_then(|m| m.as_str())
                .is_some_and(|m| m.contains("DependsCycle"))
        });
        let resolver = resolver.unwrap_or_else(|| {
            panic!("resolver DependsCycle diagnostic must be published, got {diags:?}")
        });
        assert_eq!(
            resolver.get("source").and_then(|s| s.as_str()),
            Some("lute"),
            "resolver diagnostic must be sourced \"lute\""
        );
        assert_eq!(
            resolver.get("severity").and_then(|s| s.as_u64()),
            Some(1),
            "resolver diagnostic must be Error severity"
        );
        assert_eq!(
            resolver.get("code").and_then(|c| c.as_str()),
            Some("E-DEPENDS-CYCLE"),
            "resolver diagnostic must carry the stable E-DEPENDS-CYCLE code"
        );
        let start = resolver
            .get("range")
            .and_then(|r| r.get("start"))
            .expect("range.start present");
        assert_eq!(start.get("line").and_then(|l| l.as_u64()), Some(0));
        assert_eq!(start.get("character").and_then(|c| c.as_u64()), Some(0));
        fs::remove_dir_all(&root).ok();
    }

    /// B3 claim rule: only a `.yaml`/`.yml` under a discovered project's
    /// `schema/` or `catalog/` subdirectory is claimed. `lute.project.yaml`
    /// itself (project root, not under either dir), a `.lute` file (wrong
    /// extension, even under `schema/`), and a `.yaml` with no project above
    /// it must all resolve to `None` — B3 must not claim more than the
    /// declaration dirs.
    #[test]
    fn claimed_declaration_yaml_claims_schema_and_catalog_dirs_only() {
        use std::fs;

        let root = std::env::temp_dir().join(format!("lute_lsp_claim_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("schema")).unwrap();
        fs::create_dir_all(root.join("catalog")).unwrap();
        fs::write(root.join("lute.project.yaml"), "defaultProfile: default\n").unwrap();

        assert_eq!(
            claimed_declaration_yaml(&root.join("schema/state.yaml")),
            Some(root.clone()),
            "a .yaml under schema/ must be claimed"
        );
        assert_eq!(
            claimed_declaration_yaml(&root.join("catalog/enums.yml")),
            Some(root.clone()),
            "a .yml under catalog/ must be claimed"
        );
        assert_eq!(
            claimed_declaration_yaml(&root.join("lute.project.yaml")),
            None,
            "the project manifest itself must NOT be claimed"
        );
        assert_eq!(
            claimed_declaration_yaml(&root.join("schema/scene.lute")),
            None,
            "a non-.yaml file under schema/ must NOT be claimed"
        );
        assert_eq!(
            claimed_declaration_yaml(&root.join("other/loose.yaml")),
            None,
            "a .yaml outside schema/catalog must NOT be claimed"
        );
        fs::remove_dir_all(&root).ok();
    }

    /// B3: opening a project declaration `.yaml` (under `schema/`) whose
    /// `defs:` entry's `cel:` reads an undeclared state path publishes an
    /// `E-UNDECLARED` diagnostic ON that same `.yaml` URI — today (pre-B3) the
    /// file is unclaimed and no semantic diagnostic is ever published for it.
    /// Drives a real `LspService<Backend>`: initialize -> didOpen the `.yaml`
    /// under a temp project -> assert the publish for THAT uri carries the
    /// undeclared-path diagnostic.
    #[tokio::test(flavor = "current_thread")]
    async fn analyze_declaration_yaml_flags_undeclared_cel_path() {
        use futures::StreamExt;
        use std::fs;
        use tower::{Service, ServiceExt};
        use tower_lsp_server::jsonrpc::Request as RpcRequest;
        use tower_lsp_server::LspService;

        let root = std::env::temp_dir().join(format!("lute_lsp_decl_dirty_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("schema")).unwrap();
        fs::write(root.join("lute.project.yaml"), "defaultProfile: default\nprofiles:\n  default: {}\n").unwrap();
        let decl_path = root.join("schema/state.yaml");
        // `run.nope` is never declared under `state:` — a bad/undeclared path.
        fs::write(
            &decl_path,
            "state:\n  run.trust: { type: number, default: 0 }\ndefs:\n  x: { type: bool, cel: \"run.nope\" }\n",
        )
        .unwrap();
        let uri_str = format!("file://{}", decl_path.display());

        let (mut service, mut socket) = LspService::new(Backend::new);
        let init = RpcRequest::build("initialize")
            .params(serde_json::json!({ "capabilities": {} }))
            .id(1)
            .finish();
        service.ready().await.unwrap().call(init).await.unwrap();

        let open = RpcRequest::build("textDocument/didOpen")
            .params(serde_json::json!({
                "textDocument": {
                    "uri": uri_str, "languageId": "yaml", "version": 1,
                    "text": fs::read_to_string(&decl_path).unwrap()
                }
            }))
            .finish();
        service.ready().await.unwrap().call(open).await.unwrap();
        let opened = socket.next().await.expect("didOpen should publish");
        assert_eq!(opened.method(), "textDocument/publishDiagnostics");
        let params = opened.params().expect("publish carries params");
        assert_eq!(
            params.get("uri").and_then(|u| u.as_str()),
            Some(uri_str.as_str()),
            "the diagnostic must publish ON the declaration .yaml's own URI"
        );
        let diags = params
            .get("diagnostics")
            .and_then(|d| d.as_array())
            .expect("publish carries a diagnostics array");
        let undeclared = diags.iter().find(|d| {
            d.get("code").and_then(|c| c.as_str()) == Some("E-UNDECLARED")
                && d.get("message")
                    .and_then(|m| m.as_str())
                    .is_some_and(|m| m.contains("run.nope"))
        });
        assert!(
            undeclared.is_some(),
            "expected an E-UNDECLARED diagnostic for `run.nope`, got {diags:?}"
        );
        assert_eq!(
            undeclared.unwrap().get("source").and_then(|s| s.as_str()),
            Some("lute")
        );
        fs::remove_dir_all(&root).ok();
    }

    /// B3: a clean declaration `.yaml` (every `defs:` CEL path declared)
    /// publishes NO diagnostics.
    #[tokio::test(flavor = "current_thread")]
    async fn analyze_declaration_yaml_clean_publishes_no_diagnostics() {
        use futures::StreamExt;
        use std::fs;
        use tower::{Service, ServiceExt};
        use tower_lsp_server::jsonrpc::Request as RpcRequest;
        use tower_lsp_server::LspService;

        let root = std::env::temp_dir().join(format!("lute_lsp_decl_clean_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("schema")).unwrap();
        fs::write(root.join("lute.project.yaml"), "defaultProfile: default\nprofiles:\n  default: {}\n").unwrap();
        let decl_path = root.join("schema/state.yaml");
        fs::write(
            &decl_path,
            "state:\n  run.trust: { type: number, default: 0 }\ndefs:\n  x: { type: bool, cel: \"run.trust > 0\" }\n",
        )
        .unwrap();
        let uri_str = format!("file://{}", decl_path.display());

        let (mut service, mut socket) = LspService::new(Backend::new);
        let init = RpcRequest::build("initialize")
            .params(serde_json::json!({ "capabilities": {} }))
            .id(1)
            .finish();
        service.ready().await.unwrap().call(init).await.unwrap();

        let open = RpcRequest::build("textDocument/didOpen")
            .params(serde_json::json!({
                "textDocument": {
                    "uri": uri_str, "languageId": "yaml", "version": 1,
                    "text": fs::read_to_string(&decl_path).unwrap()
                }
            }))
            .finish();
        service.ready().await.unwrap().call(open).await.unwrap();
        let opened = socket.next().await.expect("didOpen should publish");
        let diags = opened
            .params()
            .and_then(|p| p.get("diagnostics").cloned())
            .and_then(|d| d.as_array().cloned())
            .expect("publish carries a diagnostics array");
        assert!(
            diags.is_empty(),
            "a clean declaration .yaml must publish no diagnostics, got {diags:?}"
        );
        fs::remove_dir_all(&root).ok();
    }

    /// Declaration-path CEL-parse fixture helper (B3 + this locator's own
    /// tests): opens `text` as a claimed `schema/state.yaml`, returns every
    /// published `E-CEL-PARSE` diagnostic in PUBLISH order — the SAME order
    /// `analyze_declaration` pushed them (`typed.defs`'s `BTreeMap` iterates
    /// by name, ascending — never source-text order).
    async fn open_cel_parse_fixture(text: &str) -> (Vec<serde_json::Value>, std::path::PathBuf) {
        use futures::StreamExt;
        use std::fs;
        use tower::{Service, ServiceExt};
        use tower_lsp_server::jsonrpc::Request as RpcRequest;
        use tower_lsp_server::LspService;

        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("lute_lsp_decl_cel_parse_{}_{n}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("schema")).unwrap();
        fs::write(
            root.join("lute.project.yaml"),
            "defaultProfile: default\nprofiles:\n  default: {}\n",
        )
        .unwrap();
        let decl_path = root.join("schema/state.yaml");
        fs::write(&decl_path, text).unwrap();
        let uri_str = format!("file://{}", decl_path.display());

        let (mut service, mut socket) = LspService::new(Backend::new);
        let init = RpcRequest::build("initialize")
            .params(serde_json::json!({ "capabilities": {} }))
            .id(1)
            .finish();
        service.ready().await.unwrap().call(init).await.unwrap();

        let open = RpcRequest::build("textDocument/didOpen")
            .params(serde_json::json!({
                "textDocument": {
                    "uri": uri_str, "languageId": "yaml", "version": 1,
                    "text": text
                }
            }))
            .finish();
        service.ready().await.unwrap().call(open).await.unwrap();
        let opened = socket.next().await.expect("didOpen should publish");
        let diags = opened
            .params()
            .and_then(|p| p.get("diagnostics").cloned())
            .and_then(|d| d.as_array().cloned())
            .expect("publish carries a diagnostics array");
        let cel_parse: Vec<serde_json::Value> = diags
            .into_iter()
            .filter(|d| d.get("code").and_then(|c| c.as_str()) == Some("E-CEL-PARSE"))
            .collect();
        (cel_parse, root)
    }

    /// `(line, character)` start/end pair for byte range `[byte_start,
    /// byte_end)` of `text`, through the SAME `TextIndex`/`byte_to_position`
    /// conversion the backend itself uses for every published range.
    fn range_at(text: &str, byte_start: usize, byte_end: usize) -> ((u64, u64), (u64, u64)) {
        let idx = TextIndex::new(text);
        let s = byte_to_position(byte_start, &idx);
        let e = byte_to_position(byte_end, &idx);
        ((s.line as u64, s.character as u64), (e.line as u64, e.character as u64))
    }

    /// [`range_at`] for the FIRST occurrence of `needle` in `text`.
    fn expect_range(text: &str, needle: &str) -> ((u64, u64), (u64, u64)) {
        let start = text
            .find(needle)
            .unwrap_or_else(|| panic!("fixture must contain {needle:?}: {text}"));
        range_at(text, start, start + needle.len())
    }

    fn actual_range(diag: &serde_json::Value) -> ((u64, u64), (u64, u64)) {
        let range = diag.get("range").expect("diagnostic carries a range");
        (
            (
                range["start"]["line"].as_u64().unwrap(),
                range["start"]["character"].as_u64().unwrap(),
            ),
            (
                range["end"]["line"].as_u64().unwrap(),
                range["end"]["character"].as_u64().unwrap(),
            ),
        )
    }

    /// §8.1 leak-fix fold-in (flagged by Task 13 as out-of-scope for
    /// `lute-check`, closed here): a declaration `.yaml`'s `defs:` `cel:`
    /// body that fails to parse must publish an `E-CEL-PARSE` message with
    /// NONE of the embedded backend's own ANTLR vocabulary — the exact
    /// `no_backend_vocabulary_ever` contract `lute-check/tests/cel_message.rs`
    /// pins for the main `check()` path, now pinned for this SECOND
    /// construction site too. `run.act = 1` is T2's own bare-`=` fixture
    /// (`cel_message.rs` rule 4): a real parse failure `lute_cel::parse_slot`
    /// rejects, translated through the SAME `translate_cel_parse` (Task 15's
    /// fold-in of the T13 finding) instead of `e.message`.
    #[tokio::test(flavor = "current_thread")]
    async fn analyze_declaration_cel_parse_error_has_no_antlr_vocabulary() {
        // `run.act = 1` is a bare-`=` CEL parse failure (T2 rule 4) — CEL
        // wants `==`, so this never parses.
        let text = "state:\n  run.act: { type: number, default: 0 }\ndefs:\n  x: { type: bool, cel: \"run.act = 1\" }\n";
        let (cel_parse, root) = open_cel_parse_fixture(text).await;
        let hit = cel_parse
            .first()
            .unwrap_or_else(|| panic!("expected an E-CEL-PARSE diagnostic"));
        let message = hit
            .get("message")
            .and_then(|m| m.as_str())
            .expect("E-CEL-PARSE carries a message");
        for tok in [
            "viable alternative",
            "token recognition",
            "mismatched input",
            "extraneous input",
            "no viable",
        ] {
            assert!(
                !message.contains(tok),
                "declaration-path E-CEL-PARSE leaked backend vocabulary {tok:?}: {message}"
            );
        }
        assert!(
            message.contains("did you mean"),
            "expected the writer-voiced T2 bare-`=` suggestion, got: {message}"
        );
        // Finding 7 (flipped, this locator's own fix): the range now
        // anchors on the WHOLE `cel:` value scalar span (quotes included),
        // via `find_def_cel_value_span` — never the `defs:` key span, and
        // never a `translate_cel_parse`-rebased offset.
        assert_eq!(
            actual_range(hit),
            expect_range(text, "\"run.act = 1\""),
            "E-CEL-PARSE range must equal the whole `cel:` value scalar span"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    /// Block-style `cel:` (`x:\n  type: ...\n  cel: "..."`, no inline flow
    /// mapping) must anchor on the same whole-scalar span the inline flow
    /// form does — the locator is YAML-aware, not tied to one entry shape.
    #[tokio::test(flavor = "current_thread")]
    async fn analyze_declaration_cel_parse_error_anchors_block_style_cel_value() {
        let text = "state:\n  run.act: { type: number, default: 0 }\ndefs:\n  x:\n    type: bool\n    cel: \"run.act = 1\"\n";
        let (cel_parse, root) = open_cel_parse_fixture(text).await;
        let hit = cel_parse
            .first()
            .unwrap_or_else(|| panic!("expected an E-CEL-PARSE diagnostic"));
        assert_eq!(
            actual_range(hit),
            expect_range(text, "\"run.act = 1\""),
            "block-style `cel:` must anchor on its own scalar value span"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    /// Two defs sharing byte-identical `cel:` text: the locator is scoped to
    /// EACH def's own `defs.<name>.cel` entry, so it must anchor on the
    /// occurrence structurally inside `name`'s own entry — never the first
    /// textual occurrence in the file (a naive substring search would pick
    /// `b`'s span for BOTH diagnostics). `defs` iterates by name (`a` <
    /// `b`), so `a`'s diagnostic — even though `a` is defined SECOND in
    /// source — publishes first.
    #[tokio::test(flavor = "current_thread")]
    async fn analyze_declaration_cel_parse_error_picks_right_duplicate_occurrence() {
        let text = "state:\n  run.act: { type: number, default: 0 }\ndefs:\n  b: { type: bool, cel: \"run.act = 1\" }\n  a: { type: bool, cel: \"run.act = 1\" }\n";
        let (cel_parse, root) = open_cel_parse_fixture(text).await;
        assert_eq!(cel_parse.len(), 2, "both `a` and `b` must fail to parse: {cel_parse:?}");
        let needle = "\"run.act = 1\"";
        let first_occ = text.find(needle).expect("fixture contains the cel text");
        let second_occ = text[first_occ + needle.len()..]
            .find(needle)
            .map(|p| p + first_occ + needle.len())
            .expect("fixture contains the cel text twice");
        assert_ne!(first_occ, second_occ);
        assert_eq!(
            actual_range(&cel_parse[0]),
            range_at(text, second_occ, second_occ + needle.len()),
            "`a` (defined second in source) must anchor on its own occurrence, not `b`'s"
        );
        assert_eq!(
            actual_range(&cel_parse[1]),
            range_at(text, first_occ, first_occ + needle.len()),
            "`b` (defined first in source) must anchor on its own occurrence"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    /// A double-quoted `cel:` scalar containing an escaped inner quote
    /// (`cel: "run.act = \"x\""`) still fails the SAME bare-`=` rule; the
    /// locator must skip over the `\"` escapes rather than stopping at the
    /// first one, so the anchored span covers the WHOLE outer-quoted
    /// scalar (both escaped inner quotes included), the message stays
    /// ANTLR-free, and the diagnostic carries no fixit — `E-CEL-PARSE` here
    /// is message-only from `translate_cel_parse`, per the decoded-vs-source
    /// offset gap this locator exists to route around (never `t.span`/
    /// `t.fixits`, both offsets into the DECODED `raw`).
    #[tokio::test(flavor = "current_thread")]
    async fn analyze_declaration_cel_parse_error_escaped_scalar_whole_span_no_fixit() {
        use futures::StreamExt;
        use std::fs;
        use tower::{Service, ServiceExt};
        use tower_lsp_server::jsonrpc::Request as RpcRequest;
        use tower_lsp_server::LspService;

        let text = "state:\n  run.act: { type: number, default: 0 }\ndefs:\n  x: { type: bool, cel: \"run.act = \\\"x\\\"\" }\n";
        let needle = "\"run.act = \\\"x\\\"\"";
        assert!(
            text.contains(needle),
            "sanity: fixture must contain the escaped scalar literally: {text}"
        );

        let root = std::env::temp_dir()
            .join(format!("lute_lsp_decl_cel_parse_escaped_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("schema")).unwrap();
        fs::write(
            root.join("lute.project.yaml"),
            "defaultProfile: default\nprofiles:\n  default: {}\n",
        )
        .unwrap();
        let decl_path = root.join("schema/state.yaml");
        fs::write(&decl_path, text).unwrap();
        let uri_str = format!("file://{}", decl_path.display());

        let (mut service, mut socket) = LspService::new(Backend::new);
        let init = RpcRequest::build("initialize")
            .params(serde_json::json!({ "capabilities": {} }))
            .id(1)
            .finish();
        service.ready().await.unwrap().call(init).await.unwrap();

        let open = RpcRequest::build("textDocument/didOpen")
            .params(serde_json::json!({
                "textDocument": {
                    "uri": uri_str, "languageId": "yaml", "version": 1,
                    "text": text
                }
            }))
            .finish();
        service.ready().await.unwrap().call(open).await.unwrap();
        let opened = socket.next().await.expect("didOpen should publish");
        let diags = opened
            .params()
            .and_then(|p| p.get("diagnostics").cloned())
            .and_then(|d| d.as_array().cloned())
            .expect("publish carries a diagnostics array");
        let cel_parse = diags
            .iter()
            .find(|d| d.get("code").and_then(|c| c.as_str()) == Some("E-CEL-PARSE"))
            .unwrap_or_else(|| panic!("expected an E-CEL-PARSE diagnostic, got {diags:?}"));
        let message = cel_parse.get("message").and_then(|m| m.as_str()).expect("carries a message");
        for tok in [
            "viable alternative",
            "token recognition",
            "mismatched input",
            "extraneous input",
            "no viable",
        ] {
            assert!(!message.contains(tok), "leaked backend vocabulary {tok:?}: {message}");
        }
        assert!(message.contains("did you mean"), "expected the T2 bare-`=` suggestion, got: {message}");
        assert_eq!(
            actual_range(cel_parse),
            expect_range(text, needle),
            "must anchor on the WHOLE outer-quoted scalar, escapes included"
        );

        // Fixits stay empty: `code_action` maps ONLY cached `fixits` to
        // `CodeAction`s, so an overlapping request returning nothing proves
        // this `E-CEL-PARSE` carried none.
        let range = cel_parse.get("range").cloned().expect("range");
        let code_action_req = RpcRequest::build("textDocument/codeAction")
            .params(serde_json::json!({
                "textDocument": { "uri": uri_str },
                "range": range,
                "context": { "diagnostics": [] }
            }))
            .id(2)
            .finish();
        let resp = service.ready().await.unwrap().call(code_action_req).await.unwrap();
        let actions = resp.and_then(|r| r.result().cloned());
        assert!(
            matches!(actions, None | Some(serde_json::Value::Null))
                || actions.as_ref().and_then(|a| a.as_array()).map(|a| a.is_empty()).unwrap_or(false),
            "an E-CEL-PARSE diagnostic must carry no fixit-derived code action: {actions:?}"
        );

        fs::remove_dir_all(&root).ok();
    }

    /// Rock-solid fallback: a literal block scalar (`cel: |\n  ...`) is
    /// explicitly out of the locator's scope (dsl §8.1's decoded-vs-source
    /// offset gap — a multi-line folded/literal scalar's DECODED text has
    /// no simple byte-for-byte map back to its indented source lines), so
    /// `find_def_cel_value_span` declines rather than guessing, and the
    /// diagnostic falls back to the honest `defs:` KEY span exactly like
    /// the pre-fix behavior.
    #[tokio::test(flavor = "current_thread")]
    async fn analyze_declaration_cel_parse_error_falls_back_to_key_span_for_block_scalar() {
        let text = "state:\n  run.act: { type: number, default: 0 }\ndefs:\n  x:\n    type: bool\n    cel: |\n      run.act = 1\n";
        let (cel_parse, root) = open_cel_parse_fixture(text).await;
        let hit = cel_parse
            .first()
            .unwrap_or_else(|| panic!("expected an E-CEL-PARSE diagnostic"));
        let key_span = find_key_span(text, "x").expect("`x` is a `defs:` key in the fixture");
        assert_eq!(
            actual_range(hit),
            range_at(text, key_span.byte_start, key_span.byte_end),
            "a `|` literal block scalar must fall back to the `defs:` key span"
        );
        std::fs::remove_dir_all(&root).ok();
    }
}
