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
use lute_manifest::project::{project_providers, ProjectConfig};
use lute_manifest::resolve::resolve_activation;

/// clap `value_parser` for `lute lint --deny <CODE>`.
///
/// Lint diagnostic codes are dynamic (`L-*` derived from plugin/custom rule
/// ids), so the code registry ([`crate::codes`]) `check`/`check-project`
/// use cannot enumerate them. Instead accept any code matching
/// `^(L-[A-Z0-9-]+|E-LINT-(CONFIG|EXPR|RULE))$`, plus the native
/// `W-DISPLAY-NAME-DUP`. Anything else is a clap usage error (exit 2),
/// matching the "a typo'd `--deny` MUST NOT silently protect nothing"
/// contract (spec §5).
pub fn parse_lint_deny_code(raw: &str) -> Result<String, String> {
    if is_lint_deniable(raw) {
        Ok(raw.to_string())
    } else {
        Err(format!(
            "unknown diagnostic code `{raw}` (expected `L-<CODE>` or \
             `E-LINT-CONFIG`/`E-LINT-EXPR`/`E-LINT-RULE`/`W-DISPLAY-NAME-DUP`); a typo'd `--deny` \
             would silently protect nothing"
        ))
    }
}

fn is_lint_deniable(code: &str) -> bool {
    match code {
        "E-LINT-CONFIG" | "E-LINT-EXPR" | "E-LINT-RULE" => true,
        // dsl 0.26.0 §2.8: the one native project-level lint.
        lute_check::display_names::W_DISPLAY_NAME_DUP => true,
        s if s.starts_with("L-") && s.len() > 2 => s[2..]
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-'),
        _ => false,
    }
}

/// Lint-scoped promotion policy — a private mirror of [`crate::DenyPolicy`]
/// with the same semantics (spec §5) over the lint code universe. Kept
/// separate so `lute check`'s registry-backed `--deny` is not perturbed by
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
            let e = lute_manifest::io_reason(&e);
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

/// Read `file` into a [`LintDocInput`] with its `path` set to the
/// root-relative display form. Returns `Err(exit 2)` on an unreadable file.
///
/// Under a manifest (`with_project` true) the document is assembled through
/// the shared model input path used by `check-project` — a scene a `chapters:`
/// chain lists carries the `on:` the chain derives, so lint classifies it as
/// the beat every other surface reads — returned with the model's
/// [`lute_model::BuiltInput`] for the root's project passes.
fn build_lint_input(
    file: &Path,
    root: &Path,
    with_project: bool,
) -> Result<(LintDocInput, Option<lute_model::BuiltInput>), ExitCode> {
    let (text, doc, built) = if with_project {
        let Some(built) = lute_model::build_input(file, None, Some(root), None) else {
            return Err(ExitCode::from(2));
        };
        let text = built.input.text.clone();
        let mut parsed = lute_syntax::parse(&text);
        lute_check::meta::apply_quest_tier_default(&mut parsed.0, &built.defaults);
        lute_check::chapters::apply_chapters(
            &mut parsed.0,
            &built.defaults,
            &built.input.snapshot.occasions,
        );
        (text, parsed.0, Some(built))
    } else {
        let text = lute_model::read_document(file).map_err(|message| {
            eprintln!("{message}");
            ExitCode::from(2)
        })?;
        let doc = lute_syntax::parse(&text).0;
        (text, doc, None)
    };
    Ok((
        LintDocInput {
            path: relative_display(file, root),
            doc,
            text,
        },
        built,
    ))
}

fn project_for_root(root: &Path) -> Result<Option<ProjectConfig>, ExitCode> {
    if !root.join("lute.project.yaml").is_file() {
        return Ok(None);
    }
    let opts = lute_model::ModelOptions {
        providers: None,
        permission_profile: None,
        mode: lute_check::Mode::Ci,
        compile: false,
        wip: false,
    };
    let model = lute_model::ProjectModel::build_single_root(root, &opts).map_err(|error| {
        eprintln!("lute: cannot load project {}: {error}", root.display());
        ExitCode::from(2)
    })?;
    model.manifest().cloned().ok_or_else(|| {
        eprintln!(
            "lute: cannot load project manifest {}",
            root.join("lute.project.yaml").display()
        );
        ExitCode::from(2)
    }).map(Some)
}

/// once per root. Returns `Err(exit 2)` on any I/O / malformed-YAML failure.
fn lint_target(path: &Path, explicit_config: Option<&Path>) -> Result<LintOutcome, ExitCode> {
    let mut aggregated = LintOutcome::default();

    // Two shapes: a single .lute file or a directory tree.
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) => {
            let e = lute_manifest::io_reason(&e);
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
        let root = lute_model::nearest_manifest_dir(path).unwrap_or(fallback);
        let mut m = BTreeMap::new();
        m.insert(root, vec![path.to_path_buf()]);
        m
    } else {
        let files = lute_model::find_lute_files(path).map_err(|e| {
            let e = lute_manifest::io_reason(&e);
            eprintln!("lute: cannot walk {}: {e}", path.display());
            ExitCode::from(2)
        })?;
        let mut m: BTreeMap<PathBuf, Vec<PathBuf>> = BTreeMap::new();
        for f in files {
            let root = lute_model::project_root_for(&f, path);
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

        // Load the project through the shared semantic model. Absent
        // manifest ⇒ defaults-only, no plugin rules.
        let project = project_for_root(&root)?;
        let providers = project_providers(project.as_ref());
        let plugin_rules = plugin_rules_for_root(project.as_ref());

        // Build parsed inputs — under a manifest, desugared as `check-project`
        // reads them ([`build_lint_input`]).
        let (inputs, builts): (Vec<LintDocInput>, Vec<_>) = {
            use rayon::prelude::*;
            files
                .par_iter()
                .map(|f| build_lint_input(f, &root, project.is_some()))
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .unzip()
        };

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

        // dsl 0.26.0 §2.8: `W-DISPLAY-NAME-DUP` over the root's documents,
        // each against the cast its own profile and imports declare.
        if let Some(project) = &project {
            let builts: Vec<_> = builts.iter().flatten().collect();
            outcome.diagnostics.extend(display_name_dups(
                &root,
                &project.plugins_dir,
                &builts,
                &inputs,
            ));
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

/// `W-DISPLAY-NAME-DUP` (dsl 0.26.0 §2.8) for one project root — the same
/// pass `check-project` runs, each document's cast resolved the way `check`
/// resolves it. `builts` is index-aligned with `inputs`; paths are the
/// root-relative display paths the other lint diagnostics carry.
fn display_name_dups(
    root: &Path,
    plugins_dir: &Path,
    builts: &[&lute_model::BuiltInput],
    inputs: &[LintDocInput],
) -> Vec<(PathBuf, Diagnostic)> {
    let per_doc: Vec<_> = builts
        .iter()
        .zip(inputs)
        .map(|(built, input)| {
            (
                lute_check::declared_cast(&built.input.snapshot, &built.input.imports, &[]),
                lute_check::check::use_speaker_lines(&input.doc, &built.input.components),
                built.input.imports.rel.origins.cast.clone(),
            )
        })
        .collect();
    let docs: Vec<(PathBuf, lute_syntax::ast::Document)> = inputs
        .iter()
        .map(|i| (i.path.clone(), i.doc.clone()))
        .collect();
    let casts: Vec<_> = per_doc.iter().map(|(c, _, _)| c).collect();
    let use_lines: Vec<_> = per_doc.iter().map(|(_, u, _)| u).collect();
    let origins: Vec<_> = per_doc.iter().map(|(_, _, o)| o).collect();
    let home = |id: &str| {
        let (path, span) = cast_home(root, Some(plugins_dir), &origins, id)?;
        // Root-relative, like every other lint path.
        let shown = path
            .strip_prefix(root)
            .map_or(path.clone(), Path::to_path_buf);
        Some((shown, span))
    };
    lute_check::display_names::check_display_names(&docs, &casts, &use_lines, &home)
}

/// dsl 0.26.0 §2.8: where cast entry `id` is written — an installed
/// plugin's `cast` export under `plugins_dir` (a plugin entry wins a same-id
/// clash, as in [`lute_check::declared_cast`]), else the schema `cast:` an
/// import resolves (`origins`, canonical paths). The path is `root`-joined;
/// the span is positioned. `None` when neither line scan finds it.
pub(crate) fn cast_home(
    root: &Path,
    plugins_dir: Option<&Path>,
    origins: &[&BTreeMap<String, lute_check::rel_schema::DeclOrigin>],
    id: &str,
) -> Option<(PathBuf, Span)> {
    if let Some(found) = plugins_dir.and_then(|d| plugin_cast_home(d, id)) {
        return Some(found);
    }
    let o = origins.iter().find_map(|o| o.get(id))?;
    let canon_root = std::fs::canonicalize(root).ok();
    let shown = canon_root
        .as_deref()
        .and_then(|c| o.file.strip_prefix(c).ok())
        .map_or_else(|| o.file.clone(), |rel| root.join(rel));
    Some((shown, o.span))
}

/// [`cast_home`] over every plugin package under `plugins_dir` (sorted, as
/// the loader scans them): the first `cast` export file declaring `id`.
fn plugin_cast_home(plugins_dir: &Path, id: &str) -> Option<(PathBuf, Span)> {
    crate::plugin_origins::export_files(plugins_dir, "cast")
        .into_iter()
        .find_map(|file| {
            let t = std::fs::read_to_string(&file).ok()?;
            let o = lute_check::rel_schema::cast_entry_offset(&t, id)?;
            Some((file, Span::from_bytes(&TextIndex::new(&t), o, o + id.len())))
        })
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
            d.text()
        );
    } else {
        println!(
            "{}:{}:{}: {severity} [{}]{marker} {}",
            path.display(),
            d.span.line,
            d.span.column,
            d.code,
            d.text(),
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
