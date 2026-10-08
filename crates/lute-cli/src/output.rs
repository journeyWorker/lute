//! Output helpers shared by every subcommand: the `--deny` promotion
//! policy, the one diagnostic line format, and the EPIPE-safe stdout sink.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::io::Write;
use std::path::Path;

use lute_core_span::{Diagnostic, Severity};
use serde::Serialize;
use serde_json::{Map, Value};

/// The `--deny <CODE>` / `--deny-warnings` promotion policy (spec §5). Errors are
/// never demotable (spec §6), so promotion only ever turns a NON-error into an
/// error for the verdict, exit code, and reported severity.
#[derive(Default)]
pub(crate) struct DenyPolicy {
    pub(crate) codes: BTreeSet<String>,
    pub(crate) warnings: bool,
}

impl DenyPolicy {
    pub(crate) fn new(deny: &[String], deny_warnings: bool) -> Self {
        DenyPolicy {
            codes: deny.iter().cloned().collect(),
            warnings: deny_warnings,
        }
    }

    /// `true` iff `d` is PROMOTED to an error by this policy: it is not ALREADY
    /// an error (errors are never demotable, and re-marking a native error
    /// `denied` would misrepresent it as a promotion) AND its code is named by
    /// `--deny` OR it is a warning under `--deny-warnings`. The JSON
    /// `denied: true` marker and the human `[denied]` marker fire iff this is
    /// `true`, so a promotion is always distinguishable from a native error
    /// (spec §5).
    pub(crate) fn denied(&self, d: &Diagnostic) -> bool {
        d.severity != Severity::Error
            && (self.codes.contains(&d.code) || (self.warnings && d.severity == Severity::Warning))
    }

    /// `true` iff any diagnostic in `diags` is promoted — the signal that flips
    /// an otherwise-clean verdict to failure (exit 1).
    pub(crate) fn any_denied(&self, diags: &[Diagnostic]) -> bool {
        diags.iter().any(|d| self.denied(d))
    }
}

/// Apply the §5 deny promotion to one diagnostic's already-serialized JSON
/// object: when `policy.denied(d)`, override `severity` to `"error"` and add the
/// additive `"denied": true` marker. lute-check's `Diagnostic` struct is
/// UNTOUCHED — the CLI owns `--json` serialization and wraps the promotion at
/// this layer (spec §5). No-op when the diagnostic is not promoted.
pub(crate) fn apply_deny_json(d: &Diagnostic, policy: &DenyPolicy, value: &mut serde_json::Value) {
    if policy.denied(d) {
        if let serde_json::Value::Object(map) = value {
            map.insert("severity".into(), serde_json::json!("error"));
            map.insert("denied".into(), serde_json::json!(true));
        }
    }
}

/// Serialize a check result at the CLI boundary, applying the deny policy
/// without changing `lute-check`'s wire type. Keeping this overlay here makes
/// single-file and project reports use exactly the same diagnostic serializer.
pub(crate) fn check_result_json(
    result: &lute_check::CheckResult,
    ok: bool,
    policy: &DenyPolicy,
) -> Result<Value, serde_json::Error> {
    let mut value = serde_json::to_value(result)?;
    if let Some(diagnostics) = value.get_mut("diagnostics").and_then(Value::as_array_mut) {
        for (diagnostic, serialized) in result.diagnostics.iter().zip(diagnostics) {
            apply_deny_json(diagnostic, policy, serialized);
        }
    }
    if let Value::Object(map) = &mut value {
        map.insert("ok".into(), serde_json::json!(ok));
    }
    Ok(value)
}

/// Serialize the project-wide report envelope. File rows and project
/// diagnostics are both assembled through [`check_result_json`] and
/// [`diagnostic_json`], so deny promotion cannot drift between commands.
pub(crate) fn project_report_json(
    file_results: &[(std::path::PathBuf, lute_check::CheckResult)],
    project_diags: &[(std::path::PathBuf, Diagnostic)],
    ok: bool,
    policy: &DenyPolicy,
) -> Result<Value, serde_json::Error> {
    let files = file_results
        .iter()
        .map(|(path, result)| {
            let file_ok = result.ok && !policy.any_denied(&result.diagnostics);
            let mut value = check_result_json(result, file_ok, policy)?;
            if let Value::Object(map) = &mut value {
                map.insert("path".into(), path.display().to_string().into());
            }
            Ok(value)
        })
        .collect::<Result<Vec<_>, serde_json::Error>>()?;
    let project = project_diags
        .iter()
        .map(|(path, diagnostic)| {
            let mut value = diagnostic_json(diagnostic, policy)?;
            if let Value::Object(map) = &mut value {
                map.insert("path".into(), path.display().to_string().into());
            }
            Ok(value)
        })
        .collect::<Result<Vec<_>, serde_json::Error>>()?;
    let mut report = Map::new();
    report.insert("ok".into(), ok.into());
    report.insert("files".into(), files.into());
    report.insert("project_diagnostics".into(), project.into());
    Ok(Value::Object(report))
}

fn diagnostic_json(
    diagnostic: &Diagnostic,
    policy: &DenyPolicy,
) -> Result<Value, serde_json::Error> {
    let mut value = serde_json::to_value(diagnostic)?;
    apply_deny_json(diagnostic, policy, &mut value);
    Ok(value)
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
/// render exactly as check diagnostics do"), and `compile --locales`'
/// `W-L10N-MISSING` stream — ONE line format, never a second convention.
///
/// Rendered to a `String` rather than printed so a caller whose STDOUT is
/// carrying an artifact can send the same bytes to stderr instead
/// ([`print_diagnostics`] is the stdout wrapper every prior caller uses).
pub(crate) fn render_diagnostics(
    file: &Path,
    diagnostics: &[Diagnostic],
    policy: &DenyPolicy,
) -> String {
    let path = file.display();
    let mut out = String::new();
    for d in diagnostics {
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
        // §5 promotion: a denied diagnostic prints `error` with a `[denied]`
        // marker so it is distinguishable from a native error.
        let denied = policy.denied(d);
        let marker = if denied { " [denied]" } else { "" };
        // A diagnostic with no place in the file (a mock flag's, D-AB)
        // names the file alone: `:0:0` claims a position that does not exist.
        let at = if d.span.line > 0 {
            format!(":{}:{}", d.span.line, d.span.column)
        } else {
            String::new()
        };
        let _ = writeln!(
            out,
            "{path}{at}: {} [{}]{marker} {}{more}",
            if denied {
                "error"
            } else {
                severity_str(d.severity)
            },
            d.code,
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
pub(crate) fn print_diagnostics(file: &Path, diagnostics: &[Diagnostic], policy: &DenyPolicy) {
    print!("{}", render_diagnostics(file, diagnostics, policy));
}

/// A summary line per diagnostic (via [`print_diagnostics`]), then a
/// pass/fail count summary. Mirrors the sorted order `check()` already
/// applied.
pub(crate) fn print_human(file: &Path, result: &lute_check::CheckResult, policy: &DenyPolicy) {
    let path = file.display();
    print_diagnostics(file, &result.diagnostics, policy);
    // §8.3: counting is by primaries — collapse (0.4.0 T14) already reduced
    // `result.diagnostics` to one entry per root cause, so a plain count needs
    // no change here. Five reads of one typo are ONE error. A `--deny`-promoted
    // warning counts as an error (spec §5), never also as a warning.
    let errors = result
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error || policy.denied(d))
        .count();
    let warnings = result
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Warning && !policy.denied(d))
        .count();
    let ok = result.ok && !policy.any_denied(&result.diagnostics);
    if ok {
        println!("ok: {path} ({warnings} warning(s))");
    } else {
        println!("failed: {path} ({errors} error(s), {warnings} warning(s))");
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
