//! Manifest discovery + validation for every command that GATES on a walked
//! tree of documents (0.10.0 §7, D-D).
//!
//! Before 0.10.0 a `lute.project.yaml` was only ever opened because some
//! DOCUMENT resolved it as its nearest root (`crate::project_root_for`). Two
//! holes followed. `compile --all --project <dir>` forces every document onto
//! the invoked root, so it opened no nested manifest at all — T1.10's finding:
//! `check-project` refused to proceed over an inner `identity:` block that
//! `compile --all` never read, while prefixing that project's `lineId`s with
//! the outer template. And a manifest in a directory holding no `.lute` file
//! was opened by nothing, in either command.
//!
//! This module is the one walk both use. Every manifest under the tree is
//! loaded EXACTLY ONCE and its diagnostics are anchored at its own path — not
//! replayed per inheriting document, which is also what D-Z requires of
//! `defaults:` (a bad path fails once, at the manifest).
//!
//! `lute scenario` also walks documents but is a report that never gates
//! (`main.rs::run_scenario` always returns `ExitCode::SUCCESS`); §7's "MUST
//! fail" has no meaning for it, so it is deliberately not wired here.

use std::path::{Path, PathBuf};

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::project::{load_project, ProjectConfig};

/// The manifest file name, in one place.
pub const MANIFEST_FILE: &str = "lute.project.yaml";

/// Every `lute.project.yaml` at or under `dir`, byte-sorted so a report is
/// deterministic regardless of directory-iteration order. Symlinked
/// DIRECTORIES are not followed — the same rule, for the same reason, as
/// [`crate::find_lute_files`]: a cyclic symlink would otherwise walk forever.
pub fn find_project_manifests(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d)? {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                stack.push(path);
            } else if path.file_name().and_then(|n| n.to_str()) == Some(MANIFEST_FILE) {
                out.push(path);
            }
        }
    }
    out.sort();
    Ok(out)
}

/// One manifest's verdict: where it is, whether it loaded, and every
/// diagnostic it raised.
pub struct ManifestVerdict {
    /// The manifest file itself — what every diagnostic below is anchored at.
    pub path: PathBuf,
    /// Its directory, i.e. the project root it defines.
    pub dir: PathBuf,
    /// `None` when the file could not be read or deserialized at all.
    pub config: Option<ProjectConfig>,
    /// The read/parse failure message, which already carries the path
    /// (`load_project` formats `invalid <path>: …`). Kept as a string rather
    /// than minting a diagnostic code the spec does not define.
    pub load_error: Option<String>,
    /// Manifest-scoped diagnostics: `E-IDENTITY-TEMPLATE` today, plus
    /// `E-DEFAULTS-KEY` from Task 3 and `W-PROJECT-INERT` from Task 2.
    pub diags: Vec<Diagnostic>,
}

impl ManifestVerdict {
    /// `true` when this manifest is invalid — §7's "MUST fail on an invalid
    /// one". A WARNING-severity diagnostic (`W-PROJECT-INERT`) does not.
    pub fn is_invalid(&self) -> bool {
        self.load_error.is_some() || self.diags.iter().any(|d| d.severity == Severity::Error)
    }
}

/// Lift a manifest-scoped [`lute_manifest::project::ResolveDiag`] — which has
/// no span and never will (D-Z: spanned YAML parsing is out of scope) — into
/// the `Diagnostic` shape the CLI's report/JSON/`--deny` machinery already
/// handles. The zeroed span is the signal to render without a line:column;
/// see [`spanless_line`].
pub fn as_diagnostic(code: &str, message: String) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity: if code.starts_with("E-") { Severity::Error } else { Severity::Warning },
        message,
        span: Span { byte_start: 0, byte_end: 0, line: 0, column: 0, utf16_range: (0, 0) },
        layer: Layer::Logic,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

/// Load and validate every manifest under `dir`, once each.
pub fn validate_manifests_under(dir: &Path) -> std::io::Result<Vec<ManifestVerdict>> {
    let mut out = Vec::new();
    for path in find_project_manifests(dir)? {
        let root = path.parent().unwrap_or(dir).to_path_buf();
        let (config, load_error) = match load_project(&root) {
            Ok(cfg) => (cfg, None),
            Err(e) => (None, Some(e)),
        };
        let diags = config
            .as_ref()
            .map(|c| {
                c.identity_diags
                    .iter()
                    .chain(c.defaults_diags.iter())
                    .map(|d| as_diagnostic(&d.code, d.message.clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        out.push(ManifestVerdict { path, dir: root, config, load_error, diags });
    }
    Ok(out)
}

/// The human line for a diagnostic with no position: `path: severity [CODE]
/// message`. A manifest diagnostic (D-Z) and a mock diagnostic (D-AB) both
/// have a right FILE and no right line — printing `:0:0` claims a position
/// that does not exist, which is the exact defect §8 opens with
/// (`scenes/wake.lute:0:0`).
pub fn spanless_line(path: &Path, d: &Diagnostic, denied: bool) -> String {
    let severity = if denied || d.severity == Severity::Error { "error" } else { "warning" };
    let marker = if denied { " [denied]" } else { "" };
    format!("{}: {severity} [{}]{marker} {}", path.display(), d.code, d.message)
}

/// Print every verdict's diagnostics and return `true` when the tree holds an
/// invalid manifest (the caller's exit-1 signal).
pub fn report_and_gate(verdicts: &[ManifestVerdict]) -> bool {
    let mut invalid = false;
    for v in verdicts {
        if let Some(e) = &v.load_error {
            println!("{e}");
        }
        for d in &v.diags {
            println!("{}", spanless_line(&v.path, d, false));
        }
        invalid |= v.is_invalid();
    }
    invalid
}

/// 0.10.0 §7 / D-S: a nested `lute.project.yaml` under the invoked root that
/// does NOT govern, **and** that would have resolved a different capability
/// snapshot or different `identity:` templates than the invoked root's.
///
/// Both disjuncts are evaluated and the capability one is the one that fires:
/// measured over `docs/examples`, three of six descendants resolve a
/// different snapshot (`showcase/`, `plugindef-project/`, `idola-project/` —
/// each declares a `pluginsDir` and activates a plugin the outer root's
/// `core` profile does not), while `anseo/`'s two `identity:` templates are
/// the defaults verbatim. The narrowing takes six candidates to three, and
/// that is what makes it a signal rather than noise.
pub const W_PROJECT_INERT: &str = "W-PROJECT-INERT";

/// What a manifest resolves ON ITS OWN — no document profile, no scene-local
/// plugins. That is the comparison D-S names: two MANIFESTS, not two
/// documents. Resolving with a document's frontmatter instead would fold in
/// scene-local plugin options and measure the wrong thing.
fn manifest_surface(
    cfg: Option<&ProjectConfig>,
) -> (String, lute_manifest::project::IdentityTemplates) {
    let (snapshot, _) = lute_manifest::project::resolve_document_snapshot(
        cfg,
        None,
        &std::collections::BTreeMap::new(),
    );
    let identity = cfg.map(|c| c.identity.clone()).unwrap_or_default();
    (snapshot.version, identity)
}

/// Push `W-PROJECT-INERT` onto every verdict whose manifest is not
/// `governing` and would have resolved differently. Call ONLY from a forced
/// single-root command; under nearest-root resolution every manifest governs
/// and the warning is unreachable by construction.
pub fn mark_inert_under(verdicts: &mut [ManifestVerdict], governing: &Path) {
    let Some(root) = verdicts.iter().find(|v| v.dir == governing) else {
        // The invoked root has no manifest of its own; nothing to compare to.
        return;
    };
    let (root_version, root_identity) = manifest_surface(root.config.as_ref());
    for v in verdicts.iter_mut() {
        if v.dir == governing || v.config.is_none() {
            continue;
        }
        let (version, identity) = manifest_surface(v.config.as_ref());
        let differs = version != root_version || identity != root_identity;
        if !differs {
            continue;
        }
        let why = if version != root_version {
            "a different capability snapshot"
        } else {
            "different `identity:` templates"
        };
        v.diags.push(as_diagnostic(
            W_PROJECT_INERT,
            format!(
                "this manifest does not govern under `--project {}` and would have resolved \
                 {why}; its settings are not applied to any document. Invoke this root \
                 directly to use them (0.10.0 §7)",
                governing.display()
            ),
        ));
    }
}
