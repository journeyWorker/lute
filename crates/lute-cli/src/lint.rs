//! `lute lint` — the CLI surface over `lute_lint::lint` (spec §2).
//!
//! Grouping mirrors `check-project`: `.lute` files discovered under `PATH`
//! are grouped by nearest ancestor `lute.project.yaml` (roots without a
//! manifest fall back to `PATH` itself or, for a bare file, its parent
//! directory). Each root loads `lute.lint.yaml` (or `--config PATH`),
//! resolves its default profile's plugin activation for plugin-published
//! lint rules, loads the pinned provider catalog, and calls the engine
//! ONCE per root with [`LintScope::Full`].
//!
//! The engine's diagnostics carry `LintDocInput.path` back verbatim; we
//! feed it a project-root-relative display path so `ignore:` globs match
//! spec §3 (and the printed diagnostics stay short).
//!
//! Exit codes (spec §Surfaces): `0` clean / only sub-error findings, `1`
//! any Error-severity lint diagnostic (native or `--deny`-promoted,
//! including `E-LINT-CONFIG`/`E-LINT-EXPR`), `2` I/O, malformed YAML, or
//! usage.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lute_core_span::{Diagnostic, Severity, Span, TextIndex};
use lute_lint::{lint, parse_config, LintConfig, LintDocInput, LintOutcome, LintScope};
use lute_manifest::lint::namespace_active_lints;
use lute_manifest::loader::load_plugins_dir;
use lute_manifest::project::{load_project, project_providers, ProjectConfig};
use lute_manifest::resolve::resolve_activation;

/// clap `value_parser` for `lute lint --deny <CODE>`.
///
/// Lint diagnostic codes are dynamic (`L-*` derived from plugin/custom rule
/// ids), so the static `DENIABLE_CODES` registry `check`/`check-project` use
/// cannot enumerate them. Instead accept any code matching
/// `^(L-[A-Z0-9-]+|E-LINT-(CONFIG|EXPR|RULE))$`. Anything else is a clap
/// usage error (exit 2), matching the "a typo'd `--deny` MUST NOT silently
/// protect nothing" contract (spec §5).
pub fn parse_lint_deny_code(raw: &str) -> Result<String, String> {
    if is_lint_deniable(raw) {
        Ok(raw.to_string())
    } else {
        Err(format!(
            "unknown diagnostic code `{raw}` (expected `L-<CODE>` or \
             `E-LINT-CONFIG`/`E-LINT-EXPR`/`E-LINT-RULE`); a typo'd `--deny` \
             must not silently protect nothing (spec §5)"
        ))
    }
}

fn is_lint_deniable(code: &str) -> bool {
    match code {
        "E-LINT-CONFIG" | "E-LINT-EXPR" | "E-LINT-RULE" => true,
        s if s.starts_with("L-") && s.len() > 2 => s[2..]
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-'),
        _ => false,
    }
}

/// Lint-scoped promotion policy — a private mirror of [`crate::DenyPolicy`]
/// with the same semantics (spec §5) over the lint code universe. Kept
/// separate so `lute check`'s own `DENIABLE_CODES` guard is not perturbed by
/// dynamic `L-*` ids.
#[derive(Default, Clone)]
pub struct LintDenyPolicy {
    codes: BTreeSet<String>,
    warnings: bool,
}

impl LintDenyPolicy {
    pub fn new(codes: &[String], warnings: bool) -> Self {
        Self {
            codes: codes.iter().cloned().collect(),
            warnings,
        }
    }

    /// Same semantics as `check`'s: promote iff not already an error AND
    /// the code is named OR `--deny-warnings` is on and severity is Warning.
    pub fn denied(&self, d: &Diagnostic) -> bool {
        d.severity != Severity::Error
            && (self.codes.contains(&d.code) || (self.warnings && d.severity == Severity::Warning))
    }
}

/// Recursively collect every `*.lute` file under `dir`, sorted for
/// determinism. Duplicated logic with `crate::find_lute_files` (private) —
/// intentional: the lint walk does NOT canonicalize/dedupe symlinks
/// because lint diagnostics never depend on a single-physical-doc-once
/// invariant the way project-quest-id uniqueness does; keeping this local
/// avoids widening the public seam.
fn find_lute_files(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d)? {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|e| e.to_str()) == Some("lute") {
                out.push(path);
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Walk from `file`'s directory upward until a `lute.project.yaml` is
/// found; return that directory. When no ancestor carries one, return
/// `fallback` (the invocation root, or `file.parent()` for a bare file).
fn nearest_manifest_root(file: &Path, fallback: &Path) -> PathBuf {
    let mut dir = file.parent().unwrap_or(fallback).to_path_buf();
    loop {
        if dir.join("lute.project.yaml").is_file() {
            return dir;
        }
        match dir.parent() {
            Some(parent) if parent != dir => dir = parent.to_path_buf(),
            _ => return fallback.to_path_buf(),
        }
    }
}

/// Same nested-root grouping `check-project`/`collect_project_docs` use,
/// but bounded by `walk_root` (i.e. matches `crate::project_root_for`
/// semantics exactly).
fn project_root_for(file: &Path, walk_root: &Path) -> PathBuf {
    let mut dir = file.parent().unwrap_or(walk_root);
    loop {
        if dir.join("lute.project.yaml").is_file() {
            return dir.to_path_buf();
        }
        if dir == walk_root {
            return walk_root.to_path_buf();
        }
        dir = match dir.parent() {
            Some(parent) => parent,
            None => return walk_root.to_path_buf(),
        };
    }
}

/// Resolve `<root>/lute.lint.yaml` (or `explicit` when the user passed
/// `--config`). Returns `Ok(None)` on an absent file (defaults are fine);
/// `Err(exit 2)` on a read failure or malformed YAML; `Ok(Some(...))` on a
/// parsed config plus non-fatal `E-LINT-CONFIG` diagnostics.
#[allow(clippy::type_complexity)]
fn read_root_config(
    root: &Path,
    explicit: Option<&Path>,
) -> Result<Option<(PathBuf, LintConfig, Vec<Diagnostic>, Span)>, ExitCode> {
    let path = match explicit {
        Some(p) => p.to_path_buf(),
        None => root.join("lute.lint.yaml"),
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && explicit.is_none() => {
            return Ok(None);
        }
        Err(e) => {
            eprintln!("lute: cannot read {}: {e}", path.display());
            return Err(ExitCode::from(2));
        }
    };
    let idx = TextIndex::new(&text);
    let span = Span::from_bytes(&idx, 0, text.len());
    match parse_config(&text, span) {
        Ok((cfg, diags)) => Ok(Some((path, cfg, diags, span))),
        Err(e) => {
            eprintln!("lute: malformed {}: {e}", path.display());
            Err(ExitCode::from(2))
        }
    }
}

/// v1 simplification (spec §6 point 3): resolve the DEFAULT profile's
/// activation for the project root and use that ONE rule set for every
/// document. Per-document profile activation is a spec'd future refinement.
fn plugin_rules_for_root(
    project: Option<&ProjectConfig>,
) -> Vec<(String, lute_manifest::lint::LintRuleDecl)> {
    let Some(project) = project else {
        return Vec::new();
    };
    let (installed, _load_errs) = load_plugins_dir(&project.plugins_dir);
    // A load error surfaces through `lute check`'s project-diag channel;
    // suppress it here so `lute lint` never double-prints a `check` fault.
    let active = match resolve_activation(
        &project.graph,
        project.graph.default_profile.as_str(),
        &Default::default(),
        &installed,
    ) {
        Ok(a) => a,
        Err(_) => return Vec::new(),
    };
    namespace_active_lints(&active, &installed)
}

/// Project-root-relative display path (forward-slashed): what the engine
/// keys ignore globs on and what the printed diagnostics show.
fn relative_display(file: &Path, root: &Path) -> PathBuf {
    match file.strip_prefix(root) {
        Ok(rel) => {
            if rel.as_os_str().is_empty() {
                PathBuf::from(
                    file.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                )
            } else {
                // Normalize to forward slashes so `ignore:` globs written
                // as `drafts/**` match on Windows too.
                let s = rel.to_string_lossy().replace('\\', "/");
                PathBuf::from(s)
            }
        }
        Err(_) => file.to_path_buf(),
    }
}

/// Parse `file`'s text into a [`LintDocInput`] with its `path` set to the
/// root-relative display form. Returns `Err(exit 2)` on an unreadable file.
fn build_lint_input(file: &Path, root: &Path) -> Result<LintDocInput, ExitCode> {
    let text = match std::fs::read_to_string(file) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("lute: cannot read {}: {e}", file.display());
            return Err(ExitCode::from(2));
        }
    };
    let (doc, _diags) = lute_syntax::parse(&text);
    Ok(LintDocInput {
        path: relative_display(file, root),
        doc,
        text,
    })
}

/// Group the discovered files by resolved project root and run the engine
/// once per root. Returns `Err(exit 2)` on any I/O / malformed-YAML failure.
fn lint_target(path: &Path, explicit_config: Option<&Path>) -> Result<LintOutcome, ExitCode> {
    let mut aggregated = LintOutcome::default();

    // Two shapes: a single .lute file or a directory tree.
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("lute: cannot read {}: {e}", path.display());
            return Err(ExitCode::from(2));
        }
    };

    let by_root: BTreeMap<PathBuf, Vec<PathBuf>> = if meta.is_file() {
        if path.extension().and_then(|e| e.to_str()) != Some("lute") {
            eprintln!("lute: {} is not a .lute file", path.display());
            return Err(ExitCode::from(2));
        }
        let fallback = path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        let root = nearest_manifest_root(path, &fallback);
        let mut m = BTreeMap::new();
        m.insert(root, vec![path.to_path_buf()]);
        m
    } else {
        let files = find_lute_files(path).map_err(|e| {
            eprintln!("lute: cannot walk {}: {e}", path.display());
            ExitCode::from(2)
        })?;
        let mut m: BTreeMap<PathBuf, Vec<PathBuf>> = BTreeMap::new();
        for f in files {
            let root = project_root_for(&f, path);
            m.entry(root).or_default().push(f);
        }
        m
    };

    for (root, files) in by_root {
        // Load config for this root (or the shared --config override).
        let (cfg_path, cfg, cfg_diags, cfg_span) = match read_root_config(&root, explicit_config)? {
            Some(v) => (Some(v.0), v.1, v.2, v.3),
            None => (
                None,
                LintConfig::default(),
                Vec::new(),
                Span {
                    byte_start: 0,
                    byte_end: 0,
                    line: 0,
                    column: 0,
                    utf16_range: (0, 0),
                },
            ),
        };

        // Load the project (for plugin lints + provider catalog). Absent
        // manifest ⇒ defaults-only, no plugin rules.
        let project = match load_project(&root) {
            Ok(p) => p,
            Err(e) => {
                // Malformed project manifests surface through `lute check`;
                // for `lute lint` treat it as a walk failure (exit 2).
                eprintln!("lute: {e}");
                return Err(ExitCode::from(2));
            }
        };
        let providers = project_providers(project.as_ref());
        let plugin_rules = plugin_rules_for_root(project.as_ref());

        // Build parsed inputs.
        let mut inputs: Vec<LintDocInput> = Vec::with_capacity(files.len());
        for f in &files {
            inputs.push(build_lint_input(f, &root)?);
        }

        let mut outcome = lint(
            &inputs,
            &cfg,
            &plugin_rules,
            &providers,
            cfg_path.as_deref(),
            cfg_span,
            LintScope::Full,
        );

        // Config-file `E-LINT-CONFIG` diagnostics from `parse_config`
        // (semantic YAML defects — unknown level, bad shape) are surfaced
        // through the same channel the engine uses.
        let anchor = cfg_path
            .clone()
            .unwrap_or_else(|| root.join("lute.lint.yaml"));
        for d in cfg_diags {
            outcome.config_diagnostics.push((anchor.clone(), d));
        }

        aggregated.diagnostics.extend(outcome.diagnostics);
        aggregated
            .config_diagnostics
            .extend(outcome.config_diagnostics);
    }

    // Deterministic order across roots.
    aggregated.diagnostics.sort_by(|(pa, da), (pb, db)| {
        pa.cmp(pb)
            .then_with(|| da.span.byte_start.cmp(&db.span.byte_start))
            .then_with(|| da.code.cmp(&db.code))
    });
    aggregated.config_diagnostics.sort_by(|(pa, da), (pb, db)| {
        pa.cmp(pb)
            .then_with(|| da.span.byte_start.cmp(&db.span.byte_start))
            .then_with(|| da.code.cmp(&db.code))
    });
    Ok(aggregated)
}

fn severity_str(s: Severity) -> &'static str {
    match s {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "info",
        Severity::Hint => "hint",
    }
}

fn diag_to_json(path: &Path, d: &Diagnostic, denied: bool) -> serde_json::Value {
    let mut v = serde_json::to_value(d).unwrap_or_else(|_| serde_json::json!({}));
    if let serde_json::Value::Object(map) = &mut v {
        map.insert("path".into(), path.display().to_string().into());
        if denied {
            map.insert("severity".into(), serde_json::json!("error"));
            map.insert("denied".into(), serde_json::json!(true));
        }
    }
    v
}

fn print_human_line(path: &Path, d: &Diagnostic, denied: bool) {
    let marker = if denied { " [denied]" } else { "" };
    let severity = if denied {
        "error"
    } else {
        severity_str(d.severity)
    };
    if d.span.line == 0 && d.span.column == 0 {
        // A config-file diagnostic anchored at the file head via a zero
        // span still prints with a real 1:1 position (config-file parse
        // errors always have a span computed against the file text); a
        // truly zeroed span is a rare edge — render without a position.
        println!(
            "{}: {severity} [{}]{marker} {}",
            path.display(),
            d.code,
            d.message
        );
    } else {
        println!(
            "{}:{}:{}: {severity} [{}]{marker} {}",
            path.display(),
            d.span.line,
            d.span.column,
            d.code,
            d.message,
        );
    }
}

/// Entry point: `lute lint <PATH> [--json] [--deny CODE] [--deny-warnings]
/// [--config PATH]`.
pub fn run_lint(
    path: &Path,
    json: bool,
    deny: &[String],
    deny_warnings: bool,
    config: Option<&Path>,
) -> ExitCode {
    let policy = LintDenyPolicy::new(deny, deny_warnings);
    let outcome = match lint_target(path, config) {
        Ok(v) => v,
        Err(code) => return code,
    };

    // Verdict: an Error diagnostic (native or promoted) — in either bucket
    // — fails the run. Config diagnostics (`E-LINT-CONFIG`) are already
    // Error severity, so they gate exit 1 naturally.
    let any_error = |diags: &[(PathBuf, Diagnostic)]| -> bool {
        diags
            .iter()
            .any(|(_, d)| d.severity == Severity::Error || policy.denied(d))
    };
    let ok = !any_error(&outcome.diagnostics) && !any_error(&outcome.config_diagnostics);

    if json {
        let diagnostics: Vec<serde_json::Value> = outcome
            .diagnostics
            .iter()
            .map(|(p, d)| diag_to_json(p, d, policy.denied(d)))
            .collect();
        let config_diagnostics: Vec<serde_json::Value> = outcome
            .config_diagnostics
            .iter()
            .map(|(p, d)| diag_to_json(p, d, policy.denied(d)))
            .collect();
        let report = serde_json::json!({
            "ok": ok,
            "diagnostics": diagnostics,
            "configDiagnostics": config_diagnostics,
        });
        match serde_json::to_string_pretty(&report) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("lute: failed to serialize result: {e}");
                return ExitCode::from(2);
            }
        }
    } else {
        for (p, d) in &outcome.diagnostics {
            print_human_line(p, d, policy.denied(d));
        }
        for (p, d) in &outcome.config_diagnostics {
            print_human_line(p, d, policy.denied(d));
        }
        let (errors, warnings) = count(&outcome, &policy);
        if ok {
            println!(
                "ok: {} ({errors} error(s), {warnings} warning(s))",
                path.display()
            );
        } else {
            println!(
                "failed: {} ({errors} error(s), {warnings} warning(s))",
                path.display(),
            );
        }
    }

    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn count(outcome: &LintOutcome, policy: &LintDenyPolicy) -> (usize, usize) {
    let all = outcome
        .diagnostics
        .iter()
        .chain(outcome.config_diagnostics.iter());
    let mut errors = 0;
    let mut warnings = 0;
    for (_, d) in all {
        if d.severity == Severity::Error || policy.denied(d) {
            errors += 1;
        } else if d.severity == Severity::Warning {
            warnings += 1;
        }
    }
    (errors, warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deniable_matches_expected_shapes() {
        assert!(is_lint_deniable("L-DIALOGUE-LENGTH"));
        assert!(is_lint_deniable("L-MY-PLUGIN-RULE"));
        assert!(is_lint_deniable("E-LINT-CONFIG"));
        assert!(is_lint_deniable("E-LINT-EXPR"));
        assert!(is_lint_deniable("E-LINT-RULE"));

        assert!(!is_lint_deniable("L-"));
        assert!(!is_lint_deniable("L-lowercase"));
        assert!(!is_lint_deniable("E-USES-PARSE"));
        assert!(!is_lint_deniable("W-LUTE-VERSION-STALE"));
        assert!(!is_lint_deniable(""));
    }
}
