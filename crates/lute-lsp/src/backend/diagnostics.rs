use super::document::canonical_path_of;
use super::Backend;
use crate::convert::{to_lsp_diagnostic, to_lsp_range};
use lute_core_span::{Diagnostic, Severity, Span, TextIndex};
use lute_manifest::diagnostic_levels::DiagnosticLevel;
use lute_manifest::project::ProjectConfig;
use std::path::{Path, PathBuf};
use tower_lsp_server::ls_types::{
    Diagnostic as LspDiagnostic, DiagnosticRelatedInformation, DiagnosticSeverity, Location,
    Position, Range, Uri,
};

impl Backend {
    /// Round-6 T3-68: `d`'s sub-diagnostics located in another file (the
    /// schema import or plugin file its declaration sits in) become
    /// `relatedInformation` on `lsp` at that file's range, and — so the file
    /// shows its own fault while open — a diagnostic there pointing back at
    /// `uri`, pushed to `foreign` keyed by the file's canonical path. `texts`
    /// caches each file's text for one analysis; the check read the same
    /// files from disk.
    pub(crate) fn relate(
        &self,
        d: &Diagnostic,
        uri: &Uri,
        lsp: &mut LspDiagnostic,
        texts: &mut std::collections::HashMap<String, Option<String>>,
        foreign: &mut Vec<(PathBuf, LspDiagnostic)>,
    ) {
        for r in &d.related {
            let text = texts
                .entry(r.file.clone())
                .or_insert_with(|| std::fs::read_to_string(&r.file).ok());
            let Some(text) = text.as_deref() else {
                continue;
            };
            let Some(mut location) = crate::convert::related_location(r, text) else {
                continue;
            };
            let path = PathBuf::from(&r.file);
            if let Some(open) = self.open_uri_of(&path) {
                location.uri = open;
            }
            let mirror = (canonical_path_of(uri).as_ref() != Some(&path)).then(|| {
                let mut there =
                    to_lsp_diagnostic(&r.diagnostic, &TextIndex::new(text), &location.uri);
                there.related_information = Some(vec![DiagnosticRelatedInformation {
                    location: Location {
                        uri: uri.clone(),
                        range: lsp.range,
                    },
                    message: "reported here".to_string(),
                }]);
                there
            });
            lsp.related_information.get_or_insert_with(Vec::new).push(
                DiagnosticRelatedInformation {
                    location,
                    message: r.diagnostic.text().into_owned(),
                },
            );
            // A document importing itself shows the fault once, not mirrored.
            if let Some(there) = mirror {
                foreign.push((path, there));
            }
        }
    }
}

impl Backend {
    /// Publish `own` (diagnostics, version) for `uri` together with what other
    /// documents report in its file — `None` once `uri` is closed: an empty
    /// set — replace `uri`'s own reports in other files by `foreign`, and
    /// republish every open file whose set of those changed.
    pub(crate) async fn publish(
        &self,
        uri: Uri,
        own: Option<(Vec<LspDiagnostic>, Option<i32>)>,
        foreign: Vec<(PathBuf, LspDiagnostic)>,
    ) {
        let mut touched: Vec<PathBuf> = Vec::new();
        for mut entry in self.imported.iter_mut() {
            let before = entry.value().len();
            entry.value_mut().retain(|(from, _)| from != &uri);
            if entry.value().len() != before {
                touched.push(entry.key().clone());
            }
        }
        for (path, d) in foreign {
            if !touched.contains(&path) {
                touched.push(path.clone());
            }
            self.imported
                .entry(path)
                .or_default()
                .push((uri.clone(), d));
        }
        let own_path = canonical_path_of(&uri);
        match own {
            Some((own, version)) => {
                self.published.insert(uri.clone(), (own, version));
                self.republish(uri).await;
            }
            None => {
                // The buffer is gone: clear what the client shows for it.
                self.published.remove(&uri);
                self.client.publish_diagnostics(uri, Vec::new(), None).await;
            }
        }
        for path in touched {
            if Some(&path) == own_path.as_ref() {
                continue;
            }
            if let Some(open) = self.open_uri_of(&path) {
                self.republish(open).await;
            }
        }
    }

    /// Send `uri`'s last own diagnostics plus those other documents report
    /// in its file.
    pub(crate) async fn republish(&self, uri: Uri) {
        let Some((mut diags, version)) = self.published.get(&uri).map(|e| e.value().clone()) else {
            return;
        };
        if let Some(path) = canonical_path_of(&uri) {
            if let Some(entries) = self.imported.get(&path) {
                diags.extend(entries.iter().map(|(_, d)| d.clone()));
            }
        }
        self.client.publish_diagnostics(uri, diags, version).await;
    }
}

/// Convert a project-resolution diagnostic (a broken plugin graph above the
/// document) into an LSP diagnostic. Resolver diagnostics have no source span, so
/// they anchor at the document start (line 0, char 0) as an Error sourced "lute"
/// — matching the CLI, which already surfaces the same `ResolveDiag` messages to
/// stderr. This is the seam that keeps the LSP from silently dropping them.
pub(crate) fn resolve_diag_to_lsp(d: &lute_manifest::project::ResolveDiag) -> LspDiagnostic {
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
        message: lute_core_span::plain_message(&d.message).into_owned(),
        ..Default::default()
    }
}

/// dsl 0.50.0 §6.2: apply the project's `diagnostics:` levels to what is
/// about to be published for `file` (under project `root`), as the CLI's
/// severity policy reports them: a code set `off` is dropped, a re-levelled
/// one takes its new severity, and an error is never demoted. Reporting
/// only — `diags` was computed before this.
pub(crate) fn apply_project_levels(
    diags: &mut Vec<Diagnostic>,
    project: Option<&ProjectConfig>,
    root: &Path,
    file: &Path,
) {
    let Some(levels) = project.map(|p| &p.diagnostics).filter(|l| !l.is_empty()) else {
        return;
    };
    let rel = file
        .strip_prefix(root)
        .ok()
        .map(|r| r.to_string_lossy().replace('\\', "/"));
    diags.retain_mut(|d| {
        if d.severity == Severity::Error {
            return true;
        }
        match levels
            .level_for(&d.code, rel.as_deref())
            .map(DiagnosticLevel::severity)
        {
            None => true,
            Some(None) => false,
            Some(Some(severity)) => {
                d.severity = severity;
                true
            }
        }
    });
}

/// dsl 0.50.0 §6.1: the manifest's `E-DIAGNOSTICS-CONFIG` faults, located in
/// `text` (the manifest as it was loaded), published on the manifest itself
/// — once per manifest, as the CLI's manifest pass reports them.
pub(crate) fn diagnostics_config_lsp(project: &ProjectConfig, text: &str) -> Vec<LspDiagnostic> {
    let idx = TextIndex::new(text);
    lute_check::diagnostics_config::diagnostics_config_diags(project)
        .iter()
        .map(|d| {
            let mut lsp = resolve_diag_to_lsp(d);
            if let Some(r) = &d.span {
                lsp.range = to_lsp_range(&Span::from_bytes(&idx, r.start, r.end), &idx);
            }
            lsp
        })
        .collect()
}
