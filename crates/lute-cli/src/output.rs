//! Output helpers shared by every subcommand: the severity policy
//! (`--deny`, `--deny-warnings` and the project's `diagnostics:` levels), the
//! one diagnostic line format, and the EPIPE-safe stdout sink.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use lute_core_span::{Diagnostic, Severity};
use lute_manifest::diagnostic_levels::DiagnosticLevels;
use serde::Serialize;
use serde_json::{Map, Value};

/// The file a configured level comes from, as the JSON `configured` marker
/// and the human summaries name it (dsl 0.50.0 §6.3).
pub(crate) const CONFIG_FILE: &str = "lute.project.yaml";

/// Why a diagnostic is reported at another severity than its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Marker {
    /// `--deny` / `--deny-warnings` promoted it (JSON `denied`, `[denied]`).
    Denied,
    /// The project's `diagnostics:` re-levelled it (JSON `configured`,
    /// `[project]`).
    Project,
}

/// How one diagnostic is reported: its severity and why it differs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Reported {
    pub(crate) severity: Severity,
    pub(crate) marker: Option<Marker>,
}

impl Reported {
    /// `d` as it is, with no policy.
    pub(crate) fn native(d: &Diagnostic) -> Self {
        Reported {
            severity: d.severity,
            marker: None,
        }
    }
}

/// What severity each diagnostic is reported at (dsl 0.6.1 §5, 0.50.0 §6.2),
/// applied only where diagnostics are serialized, printed and counted. In
/// order:
/// 1. the project's level for (`code`, path), `W-*` codes only;
/// 2. `--deny CODE` promotes the code to an error, even when the project set
///    it `off` (the command line beats the config);
/// 3. `--deny-warnings` promotes what is still a warning after step 1: an
///    `off` code stays off and an `info` code stays info.
///
/// Errors are never demoted, and a native error is never marked.
#[derive(Clone, Default)]
pub(crate) struct SeverityPolicy {
    deny: BTreeSet<String>,
    deny_warnings: bool,
    /// The project's `diagnostics:`, when it has any.
    levels: Option<Arc<DiagnosticLevels>>,
    /// The canonical project root `paths` globs are relative to.
    root: PathBuf,
}

impl SeverityPolicy {
    /// The gate options alone, before a project is known.
    pub(crate) fn new(deny: &[String], deny_warnings: bool) -> Self {
        SeverityPolicy {
            deny: deny.iter().cloned().collect(),
            deny_warnings,
            levels: None,
            root: PathBuf::new(),
        }
    }

    /// These gate options plus the `diagnostics:` of the project at `root`
    /// (none for `None`, a project without the key, or one that does not
    /// load: its load error is reported by the command itself).
    pub(crate) fn for_project(&self, root: Option<&Path>) -> Self {
        let mut policy = SeverityPolicy::new(&[], self.deny_warnings);
        policy.deny.clone_from(&self.deny);
        let Some(root) = root else {
            return policy;
        };
        let Ok(Some(config)) = lute_manifest::project::load_project(root) else {
            return policy;
        };
        if config.diagnostics.is_empty() {
            return policy;
        }
        policy.root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        policy.levels = Some(Arc::new(config.diagnostics));
        policy
    }

    /// `at` relative to the project root, `/`-separated; `None` for a path
    /// outside it or one that names no file (a manifest-less label).
    fn rel_path(&self, at: &Path) -> Option<String> {
        let canonical = std::fs::canonicalize(at).ok()?;
        let rel = canonical.strip_prefix(&self.root).ok()?;
        Some(rel.to_string_lossy().replace('\\', "/"))
    }

    /// How `d`, reported in the file `at`, is shown; `None` when the project
    /// turned it `off` (it is counted as suppressed instead).
    pub(crate) fn report(&self, d: &Diagnostic, at: &Path) -> Option<Reported> {
        if d.severity == Severity::Error {
            return Some(Reported::native(d));
        }
        let promoted = Some(Reported {
            severity: Severity::Error,
            marker: Some(Marker::Denied),
        });
        if self.deny.contains(&d.code) {
            return promoted;
        }
        let level = self.levels.as_deref().and_then(|levels| {
            let rel = if levels.needs_path(&d.code) {
                self.rel_path(at)
            } else {
                None
            };
            levels.level_for(&d.code, rel.as_deref())
        });
        let severity = match level {
            Some(level) => level.severity()?,
            None => d.severity,
        };
        if self.deny_warnings && severity == Severity::Warning {
            return promoted;
        }
        Some(Reported {
            severity,
            marker: (severity != d.severity).then_some(Marker::Project),
        })
    }

    /// `true` iff `d` is not an error but is reported as one — the signal
    /// that fails an otherwise-clean verdict (exit 1).
    pub(crate) fn promoted(&self, d: &Diagnostic, at: &Path) -> bool {
        d.severity != Severity::Error
            && self
                .report(d, at)
                .is_some_and(|r| r.severity == Severity::Error)
    }

    /// `true` iff any of `diags`, reported in `at`, is [`Self::promoted`].
    pub(crate) fn any_promoted(&self, at: &Path, diags: &[Diagnostic]) -> bool {
        diags.iter().any(|d| self.promoted(d, at))
    }

    /// How many of `items` the project turned `off`, by code.
    pub(crate) fn suppressed<'a>(
        &self,
        items: impl IntoIterator<Item = (&'a Path, &'a Diagnostic)>,
    ) -> BTreeMap<String, usize> {
        let mut out = BTreeMap::new();
        if self.levels.is_none() {
            return out;
        }
        for (at, d) in items {
            if self.report(d, at).is_none() {
                *out.entry(d.code.clone()).or_insert(0) += 1;
            }
        }
        out
    }
}

/// Overlay `reported` on `d`'s serialized JSON object: the reported
/// `severity` when it differs, and the additive `"denied": true` or
/// `"configured": "lute.project.yaml"` marker. lute-check's `Diagnostic` is
/// untouched; the CLI owns `--json` serialization.
fn apply_reported_json(d: &Diagnostic, reported: Reported, value: &mut Value) {
    let Value::Object(map) = value else {
        return;
    };
    if reported.severity != d.severity {
        map.insert("severity".into(), severity_str(reported.severity).into());
    }
    match reported.marker {
        Some(Marker::Denied) => {
            map.insert("denied".into(), true.into());
        }
        Some(Marker::Project) => {
            map.insert("configured".into(), CONFIG_FILE.into());
        }
        None => {}
    }
}

/// `diags`, reported in `at`, as JSON values under `policy` — a suppressed
/// one left out and counted by code instead.
pub(crate) fn diagnostics_json(
    diags: &[Diagnostic],
    at: &Path,
    policy: &SeverityPolicy,
) -> Result<(Vec<Value>, BTreeMap<String, usize>), serde_json::Error> {
    let mut values = Vec::with_capacity(diags.len());
    let mut suppressed = BTreeMap::new();
    for d in diags {
        match policy.report(d, at) {
            Some(reported) => {
                let mut value = serde_json::to_value(d)?;
                apply_reported_json(d, reported, &mut value);
                values.push(value);
            }
            None => *suppressed.entry(d.code.clone()).or_insert(0) += 1,
        }
    }
    Ok((values, suppressed))
}

/// Add `"suppressed": { "<code>": n, … }` to a JSON report object, only when
/// the project suppressed something (so a project without `diagnostics:`
/// reports byte-identically).
pub(crate) fn insert_suppressed(value: &mut Value, suppressed: &BTreeMap<String, usize>) {
    if suppressed.is_empty() {
        return;
    }
    if let Value::Object(map) = value {
        let counts: Map<String, Value> = suppressed
            .iter()
            .map(|(code, n)| (code.clone(), (*n).into()))
            .collect();
        map.insert("suppressed".into(), Value::Object(counts));
    }
}

/// Serialize a check result at the CLI boundary, applying `policy` without
/// changing `lute-check`'s wire type. Keeping this overlay here makes
/// single-file and project reports use exactly the same diagnostic serializer.
pub(crate) fn check_result_json(
    result: &lute_check::CheckResult,
    ok: bool,
    at: &Path,
    policy: &SeverityPolicy,
) -> Result<Value, serde_json::Error> {
    let mut value = serde_json::to_value(result)?;
    let (diagnostics, suppressed) = diagnostics_json(&result.diagnostics, at, policy)?;
    if let Value::Object(map) = &mut value {
        map.insert("diagnostics".into(), diagnostics.into());
        map.insert("ok".into(), serde_json::json!(ok));
    }
    insert_suppressed(&mut value, &suppressed);
    Ok(value)
}

/// Serialize the `0.48.0.check` payload (both modes): one `files[]` entry per
/// document checked (`policy_of(i)` is the i-th file's policy: each file is
/// reported under its own project), and `project_diagnostics` exactly when
/// the whole project ran, under `project_policy`. File rows and project
/// diagnostics are both assembled through [`check_result_json`] and
/// [`diagnostics_json`], so the policy cannot drift between modes.
pub(crate) fn check_report_json<'p>(
    file_results: &[(PathBuf, lute_check::CheckResult)],
    project_diags: Option<&[(PathBuf, Diagnostic)]>,
    ok: bool,
    policy_of: impl Fn(usize) -> &'p SeverityPolicy,
    project_policy: &SeverityPolicy,
) -> Result<Value, serde_json::Error> {
    let files = file_results
        .iter()
        .enumerate()
        .map(|(i, (path, result))| {
            let policy = policy_of(i);
            let file_ok = result.ok && !policy.any_promoted(path, &result.diagnostics);
            let mut value = check_result_json(result, file_ok, path, policy)?;
            if let Value::Object(map) = &mut value {
                map.insert("path".into(), path.display().to_string().into());
            }
            Ok(value)
        })
        .collect::<Result<Vec<_>, serde_json::Error>>()?;
    let mut report = Value::Object(Map::new());
    if let Value::Object(map) = &mut report {
        map.insert("ok".into(), ok.into());
        map.insert("files".into(), files.into());
    }
    if let Some(project_diags) = project_diags {
        let mut project = Vec::with_capacity(project_diags.len());
        let mut suppressed: BTreeMap<String, usize> = BTreeMap::new();
        for (path, diagnostic) in project_diags {
            let (values, gone) =
                diagnostics_json(std::slice::from_ref(diagnostic), path, project_policy)?;
            for mut value in values {
                if let Value::Object(map) = &mut value {
                    map.insert("path".into(), path.display().to_string().into());
                }
                project.push(value);
            }
            for (code, n) in gone {
                *suppressed.entry(code).or_insert(0) += n;
            }
        }
        if let Value::Object(map) = &mut report {
            map.insert("project_diagnostics".into(), project.into());
        }
        insert_suppressed(&mut report, &suppressed);
    }
    Ok(report)
}

pub(crate) fn pretty_json<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(value)
}

/// Write `s` to stdout as raw bytes, returning any I/O error instead of
/// panicking the way `print!`/`println!` do when the pipe is closed (EPIPE,
/// e.g. `lute compile f.lute | head`). Callers map `Err` to exit `2`, matching
/// the `-o` file-write error path (compiler CLI spec: `2` on an I/O failure).
pub(crate) fn write_stdout(s: &str) -> std::io::Result<()> {
    let mut o = std::io::stdout().lock();
    o.write_all(s.as_bytes())?;
    o.flush()
}

/// One `file:line:col: severity [CODE] message` line per diagnostic. A
/// primary that collapsed same-root repeats (dsl 0.4.0 §8.2 C1/C5) appends a
/// trailing ` (+N more: 12:3, 47:9, …)` — line:column, comma-joined, document
/// order. Shared by [`print_human`] (the `check`/`compile` diagnostic list),
/// `run_trace`'s Refused rendering (dsl 0.4.0 §4.5: "the `E-TRACE-*` codes
/// render exactly as check diagnostics do"), and `compile --locale-bundle`'
/// `W-L10N-MISSING` stream — ONE line format, never a second convention.
///
/// Rendered to a `String` rather than printed so a caller whose STDOUT is
/// carrying an artifact can send the same bytes to stderr instead
/// ([`print_diagnostics`] is the stdout wrapper every prior caller uses).
pub(crate) fn render_diagnostics(
    file: &Path,
    diagnostics: &[Diagnostic],
    policy: &SeverityPolicy,
) -> String {
    let path = file.display();
    let mut out = String::new();
    for d in diagnostics {
        // dsl 0.50.0 §6.2: a code the project turned `off` is not printed;
        // the summary counts it as suppressed.
        let Some(reported) = policy.report(d, file) else {
            continue;
        };
        let more = if d.covered.is_empty() {
            String::new()
        } else {
            let locs: Vec<String> = d
                .covered
                .iter()
                .map(|s| format!("{}:{}", s.line, s.column))
                .collect();
            format!(" (+{} more: {})", locs.len(), locs.join(", "))
        };
        // A diagnostic with no place in the file (a mock flag's, D-AB)
        // names the file alone: `:0:0` claims a position that does not exist.
        let at = if d.span.line > 0 {
            format!(":{}:{}", d.span.line, d.span.column)
        } else {
            String::new()
        };
        let _ = writeln!(
            out,
            "{path}{at}: {} [{}]{} {}{more}",
            severity_str(reported.severity),
            d.code,
            marker_str(reported.marker),
            d.text(),
        );
        // dsl 0.5.0 §2.2: an `E-COMPONENT-PARSE` (or any diagnostic) carrying
        // `related` sub-diagnostics from ANOTHER file (e.g. a failed
        // component import's own parse errors) — print each indented under
        // the parent line, `related.file` in place of the importer's path,
        // so the author sees what actually failed without a separate
        // `check` of the component.
        for r in &d.related {
            let _ = writeln!(
                out,
                "    {}:{}:{}: {} [{}] {}",
                cwd_relative(&r.file),
                r.diagnostic.span.line,
                r.diagnostic.span.column,
                severity_str(r.diagnostic.severity),
                r.diagnostic.code,
                r.diagnostic.text(),
            );
        }
    }
    out
}

/// `file` as the author should read it on a `related` sub-line: a canonical
/// component path (the identity `lute-check` keeps in `related.file`) shown
/// relative to the current directory when it lies under it, the way the
/// primary path already reads (0.21.1 T3-7). Anything else prints unchanged.
pub(crate) fn cwd_relative(file: &str) -> String {
    let path = Path::new(file);
    std::env::current_dir()
        .and_then(std::fs::canonicalize)
        .ok()
        .filter(|_| path.is_absolute())
        .and_then(|cwd| path.strip_prefix(cwd).ok().map(|p| p.display().to_string()))
        .unwrap_or_else(|| file.to_string())
}

/// [`render_diagnostics`] to stdout — the sink every `check`/`trace` caller
/// has always used.
pub(crate) fn print_diagnostics(file: &Path, diagnostics: &[Diagnostic], policy: &SeverityPolicy) {
    print!("{}", render_diagnostics(file, diagnostics, policy));
}

/// A summary line per diagnostic (via [`print_diagnostics`]), then a
/// pass/fail count summary. Mirrors the sorted order `check()` already
/// applied.
pub(crate) fn print_human(file: &Path, result: &lute_check::CheckResult, policy: &SeverityPolicy) {
    let path = file.display();
    print_diagnostics(file, &result.diagnostics, policy);
    // §8.3: counting is by primaries — collapse (0.4.0 T14) already reduced
    // `result.diagnostics` to one entry per root cause, so a plain count needs
    // no change here. Five reads of one typo are ONE error. A promoted
    // warning counts as an error (spec §5), never also as a warning.
    let items = || result.diagnostics.iter().map(|d| (file, d));
    let errors = count_reported(items(), Severity::Error, policy);
    let warnings = count_reported(items(), Severity::Warning, policy);
    let infos = info_tail(count_reported(items(), Severity::Info, policy), "info");
    let suppressed = suppressed_tail(&policy.suppressed(items()));
    let ok = result.ok && !policy.any_promoted(file, &result.diagnostics);
    if ok {
        println!("ok: {path} ({warnings} warning(s){infos}{suppressed})");
    } else {
        println!("failed: {path} ({errors} error(s), {warnings} warning(s){infos}{suppressed})");
    }
}

/// How many of `items` (each with the file it is reported in) are reported
/// at `severity` under `policy`: a promoted one counts as an error only, a
/// suppressed one not at all. `hint` is never counted by callers.
pub(crate) fn count_reported<'a>(
    items: impl IntoIterator<Item = (&'a Path, &'a Diagnostic)>,
    severity: Severity,
    policy: &SeverityPolicy,
) -> usize {
    items
        .into_iter()
        .filter(|(at, d)| policy.report(d, at).is_some_and(|r| r.severity == severity))
        .count()
}

/// The human marker after `[CODE]`: ` [denied]` for a `--deny` promotion,
/// ` [project]` for a `lute.project.yaml` level, else nothing.
pub(crate) fn marker_str(marker: Option<Marker>) -> &'static str {
    match marker {
        Some(Marker::Denied) => " [denied]",
        Some(Marker::Project) => " [project]",
        None => "",
    }
}

/// The `, N suppressed by lute.project.yaml` tail of a summary line: empty
/// when the project suppressed nothing (dsl 0.50.0 §6.4).
pub(crate) fn suppressed_tail(suppressed: &BTreeMap<String, usize>) -> String {
    let n: usize = suppressed.values().sum();
    if n == 0 {
        String::new()
    } else {
        format!(", {n} suppressed by {CONFIG_FILE}")
    }
}

/// The `, I <noun>` tail of a summary line: empty when there is no info
/// diagnostic, so a run without one prints the summary it always printed
/// (0.41.0 §3.3).
pub(crate) fn info_tail(infos: usize, noun: &str) -> String {
    if infos == 0 {
        String::new()
    } else {
        format!(", {infos} {noun}")
    }
}

pub(crate) fn severity_str(s: Severity) -> &'static str {
    match s {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "info",
        Severity::Hint => "hint",
    }
}

/// A state path as an author writes it in a condition: a segment that is
/// not an identifier quoted (`run.visits["lab-b2"]`); text that is no plain
/// path (`holds(…)`, `run.x.<who>`) as given.
pub(crate) fn spelled_path(path: &str) -> String {
    match lute_cel::path::parse_path_text(path) {
        Some(segs) => lute_manifest::text::bracket_spelling(&segs),
        None => path.to_string(),
    }
}
