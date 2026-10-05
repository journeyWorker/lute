use std::path::Path;
use lute_check::{check_cel_slot, parse_meta_kind, translate_cel_parse, MetaKind, Mode};
use lute_core_span::{Diagnostic, Layer, Severity, Span, TextIndex};
use lute_load::{assemble_input_with_mode, InputCache};
use tower_lsp_server::ls_types::{Diagnostic as LspDiagnostic, Uri};
use crate::convert::to_lsp_diagnostic;
use super::{Backend, DocumentSnapshot};
use super::diagnostics::resolve_diag_to_lsp;
use super::yaml_spans::{find_key_span, find_def_cel_value_span};

impl Backend {
    /// Analyze an open project declaration `.yaml`/`.yml` — state/defs/enums/
    /// entities under a project's `schema:`/`catalog:` dir ([`claimed_declaration_yaml`](super::document::claimed_declaration_yaml),
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
    /// * The model assembly's `imports` — this file's OWN `uses:`/`extends:`
    ///   (dsl §9.2), resolved by the same `InputCache` path used for scenes,
    ///   so the state schema `defs:` CEL is checked against its includes.
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
    pub(super) async fn analyze_declaration(
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

        let request_cache = InputCache::default();
        let (built, _) = assemble_input_with_mode(
            &request_cache,
            file_path,
            snapshot.text.clone(),
            None,
            Some(project_root),
            None,
            Mode::Author,
        );
        // Declaration imports and the active project baseline come from the
        // same model assembly used by scene analysis.
        let imports = built.input.imports.clone();
        diags.extend(imports.diags.clone());
        let baseline = built.input.snapshot.clone();
        let mut rdiags = built.resolve_diags;
        if let Err(message) = request_cache.project(project_root).as_ref() {
            rdiags.push(lute_manifest::project::ResolveDiag {
                span: None,
                code: "E-PROJECT-CONFIG".to_string(),
                message: message.to_string(),
            });
        }

        // Domain refs: this file's own `enums:`/`entities:` unioned with its
        // imports, checked against the project's active baseline. Both sources
        // go through `merge_domains`, which owns the fusion AND the inline-wins
        // precedence (`resolve_imports`'s shallowest-wins rule: this file is
        // depth 0 to any import's depth >= 1) — the SAME call `check()` makes
        // for a scene, so the two surfaces cannot disagree about which member
        // list is live.
        let (_domains, domain_diags) =
            lute_check::schema_import::merge_domains(&baseline, &imports, &typed, whole);
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
                    // token) — falling back to the honest key `span` only
                    // when the locator genuinely can't resolve the value
                    // (e.g. a quoted def-name key, or an anchor/alias in
                    // the entry's own value; `|`/`>` block scalars DO
                    // resolve — to their content-lines span, indicators
                    // aside). Fixits stay empty either way:
                    // no source-accurate fixit edit is recoverable from a
                    // decoded-offset translation.
                    // The `defs:` `cel:` declaration path is a condition body
                    // and never a `::set` (dsl 0.10.0 §12.1).
                    let t =
                        translate_cel_parse(raw, span, &e, lute_syntax::ast::CelKind::Condition);
                    let cel_span = find_def_cel_value_span(&snapshot.text, name).unwrap_or(span);
                    diags.push(Diagnostic {
                        code: t.code.to_string(),
                        severity: Severity::Error,
                        message: t.message,
                        span: cel_span,
                        layer: Layer::Cel,
                        evidence: None,
                        fixits: Vec::new(),
                        provenance: None,
                        covered: Vec::new(),
                        related: Vec::new(),
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
        let mut lsp_diags: Vec<LspDiagnostic> = diags
            .iter()
            .map(|d| to_lsp_diagnostic(d, &idx, &uri))
            .collect();
        lsp_diags.extend(rdiags.iter().map(resolve_diag_to_lsp));
        self.publish(uri, Some((lsp_diags, Some(snapshot.version))), Vec::new())
            .await;
    }
}
