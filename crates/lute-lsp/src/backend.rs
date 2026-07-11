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
use lute_check::{check, check_cel_slot, parse_meta_kind, resolve_imports, CheckInput, MetaKind, Mode};
use lute_core_span::{Diagnostic, Layer, Severity, Span, TextIndex};
use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::{
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
}

impl Backend {
    /// Build a backend bound to `client` with an empty document map.
    pub fn new(client: Client) -> Self {
        Self {
            client,
            docs: DashMap::new(),
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
        let idx = lute_core_span::TextIndex::new(&snapshot.text);
        let mut diags: Vec<LspDiagnostic> = result
            .diagnostics
            .iter()
            .map(|d| to_lsp_diagnostic(d, &idx))
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
                Err(e) => diags.push(Diagnostic {
                    code: "E-CEL-PARSE".to_string(),
                    severity: Severity::Error,
                    message: e.message,
                    span: e.span,
                    layer: Layer::Cel,
                    fixits: Vec::new(),
                    provenance: None,
                    covered: Vec::new(),
                }),
            }
            let expected = val
                .get("type")
                .cloned()
                .and_then(|t| serde_yaml::from_value::<lute_manifest::types::Type>(t).ok())
                .map(lute_check::ctx::ExpectedType::Ty);
            diags.extend(check_cel_slot(&slot, &arena, &ctx, expected.as_ref()));
        }

        let mut lsp_diags: Vec<LspDiagnostic> =
            diags.iter().map(|d| to_lsp_diagnostic(d, &idx)).collect();
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
}
