use lute_check::{check, Mode};
use lute_model::{assemble_input_with_mode, InputCache};
use tower_lsp_server::ls_types::{Diagnostic as LspDiagnostic, Uri};
use crate::convert::to_lsp_diagnostic;
use super::{Backend, DocumentSnapshot};
use super::diagnostics::resolve_diag_to_lsp;
use super::document::{uri_to_path, claimed_declaration_yaml, is_yaml, find_project_root};

impl Backend {
    /// Run `check()` over `snapshot`'s text and publish the converted diagnostics
    /// for `uri`, stamped with the snapshot version. Positions are derived exactly
    /// as the headless path derives them (see the divergence invariant above).
    ///
    /// Project-resolution diagnostics (a broken plugin graph above the document:
    /// load/cycle/unresolved-depends/assembly errors) are surfaced too, each as an
    /// Error at the document start — otherwise a scene that is itself clean would
    /// silently validate against a broken project (plugin §11).
    pub(super) async fn analyze(&self, uri: Uri, snapshot: &DocumentSnapshot) {
        if self.publish_if_stale(&uri, snapshot.version).await {
            return;
        }
        // B3 (data-catalog foundation 0.3.0): a project declaration `.yaml`
        // (under the project's `schema:`/`catalog:` dir, or `*.schema.yaml`)
        // is a pure declaration map, not a `.lute` scene — it has no body for
        // `check()` to walk. Claim it here and run the declaration-specific
        // semantic pass instead. Any other `.yaml`/`.yml` (the manifest, a
        // play script, a test) is no `.lute` document either (LF28-9: walking
        // it flagged every line E-UNCLASSIFIED): it publishes only what other
        // documents report in its file.
        if let Some(path) = uri_to_path(&uri) {
            if let Some(root) = claimed_declaration_yaml(&path) {
                self.analyze_declaration(uri, snapshot, &path, &root).await;
                return;
            }
            if is_yaml(&path) {
                self.publish(uri, Some((Vec::new(), Some(snapshot.version))), Vec::new())
                    .await;
                return;
            }
        }
        let Some(file_path) = uri_to_path(&uri) else {
            return;
        };
        let project_root = find_project_root(&file_path);
        let request_cache = InputCache::default();
        let loaded_project = project_root
            .as_deref()
            .map(|root| request_cache.project(root));
        let (built, (doc_ast, _)) = assemble_input_with_mode(
            &request_cache,
            &file_path,
            snapshot.text.clone(),
            None,
            project_root.as_deref(),
            None,
            Mode::Author,
        );
        let input = built.input;
        let providers = input.providers.clone();
        let result = check(&input);
        self.warn_if_older_than_stamp(&snapshot.text, &input.defaults)
            .await;
        // Opt-in lint (design §2, §3): publish alongside check diagnostics
        // when `<project root>/lute.lint.yaml` exists AND sets `lsp: true`.
        // Silent no-op on absent/malformed config or `lsp: false` — the CLI
        // (`lute lint`) owns config-error reporting; a diagnostic anchored at
        // a file the editor never opened has no natural publish channel here
        // (`crate::lint`'s module docs). Lint diagnostics MUST be cached
        // alongside `check()`'s so `textDocument/codeAction` can still walk
        // their fixits (spec §8 permits fixits on lint diagnostics; today
        // none of the v1 rules emit any, but the cache is the general seam).
        let mut all_diags = result.diagnostics;
        if let Some(project_root) = project_root.as_deref() {
            let project = loaded_project
                .as_deref()
                .and_then(|loaded| loaded.as_ref().ok())
                .and_then(Option::as_ref);
            // `assemble_input_with_mode` already applied project defaults and
            // chapter derivations to this parsed document.
            all_diags.extend(crate::lint::lint_document(
                &file_path,
                project_root,
                project,
                &providers,
                &doc_ast,
                &snapshot.text,
            ));
        }
        // Task 15: retain the ORIGINAL diagnostics (fixits/covered intact)
        // beside the published LSP form for `code_action` to read back later.
        self.diagnostics.insert(uri.clone(), all_diags.clone());
        let idx = lute_core_span::TextIndex::new(&snapshot.text);
        let mut foreign = Vec::new();
        let mut texts = std::collections::HashMap::new();
        let mut diags: Vec<LspDiagnostic> = all_diags
            .iter()
            .map(|d| {
                let mut lsp = to_lsp_diagnostic(d, &idx, &uri);
                self.relate(d, &uri, &mut lsp, &mut texts, &mut foreign);
                lsp
            })
            .collect();
        let mut rdiags = built.resolve_diags;
        if let Some(Err(error)) = loaded_project.as_deref() {
            rdiags.push(lute_manifest::project::ResolveDiag {
                span: None,
                code: "E-PROJECT-CONFIG".to_string(),
                message: error.to_string(),
            });
        }
        diags.extend(rdiags.iter().map(resolve_diag_to_lsp));
        self.publish(uri, Some((diags, Some(snapshot.version))), foreign)
            .await;
    }
}
