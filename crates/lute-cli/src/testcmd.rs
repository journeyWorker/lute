//! `lute test` — scenario tests: declared mocks + declared expectations,
//! layered on `lute-trace`'s deterministic walk.
//!
//! A `*.test.yaml` file names a `.lute` document (`file:`, resolved relative
//! to the TEST file's own directory), carries the SAME five mock surfaces
//! `lute trace --mock` accepts (`state:`/`facts:`/`choose:`/`events:`/
//! `accepts:`, parsed by the SAME [`parse_mock_yaml`] — extra keys are
//! ignored, so `file:`/`expect:` coexist with them), and declares an
//! `expect:` block:
//!
//! ```yaml
//! file: ../scenes/confrontation.lute
//! state:            # mock seed (same as `lute trace --mock`)
//!   run.trueKiller: blake
//! choose:
//!   accuse: accuseBlake
//! expect:
//!   transcriptContains: ["Case closed."]
//!   state: { run.accused: blake }   # the trace's FINAL written state
//!   exit: complete                  # complete | incomplete
//! ```
//!
//! Each test traces its document once ([`trace_document`], no `--project`
//! gate — the same core-only resolution `lute trace` uses) and checks every
//! declared expectation, naming actual-vs-expected on any miss. Exit `0` when
//! all pass, `1` when any fails, `2` on an I/O failure or a malformed test
//! yaml. `--coverage` reports chosen-vs-never-chosen choices and
//! executed-vs-unexecuted match arms aggregated across every traced path
//! (honest: "over N traced paths", never a whole-space coverage claim).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lute_trace::{parse_mock_yaml, trace_document, Step, TraceExit, TraceReport};

/// One declared expectation's verdict, carrying enough to render both the
/// human miss line and the `--json` entry.
struct ExpectResult {
    kind: &'static str,
    /// A stable label for the checked thing (e.g. the state path, or empty).
    subject: String,
    expected: String,
    actual: String,
    passed: bool,
}

/// One test file's outcome.
struct TestResult {
    test_file: PathBuf,
    lute_file: String,
    exit: String,
    passed: bool,
    expectations: Vec<ExpectResult>,
    /// Populated only when a test cannot produce a report (a refused trace) —
    /// a single fatal reason rendered instead of per-expectation lines.
    refusal: Option<String>,
}

/// Coverage accumulated across every traced path in the run. Names come from
/// what the reports actually expose (decision outcomes, decision `eligible`
/// lists); the totals come from each report's own `coverage` counts. Nothing
/// here is presented as whole-space coverage — only "what these N paths
/// touched" (D1: trace explains, it never proves).
#[derive(Default)]
struct CoverageAccum {
    /// branch/hub id -> (chosen choice ids, choice ids seen eligible, total).
    choices: BTreeMap<String, (BTreeSet<String>, BTreeSet<String>, usize)>,
    /// match subject -> (chosen arm outcomes, total arms).
    arms: BTreeMap<String, (BTreeSet<String>, usize)>,
    /// Number of documents that produced a report (a non-refused trace).
    paths: usize,
}

/// Run every `*.test.yaml` scenario test under `dir`. See [`crate::Command::Test`].
pub fn run_test(dir: &Path, json: bool, providers: Option<&Path>, coverage: bool) -> ExitCode {
    let test_files = match find_test_files(dir) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("lute: cannot walk {}: {e}", dir.display());
            return ExitCode::from(2);
        }
    };

    let mut results = Vec::new();
    let mut cov = CoverageAccum::default();

    for test_file in &test_files {
        match run_one_test(test_file, providers, coverage.then_some(&mut cov)) {
            Ok(r) => results.push(r),
            // A malformed test yaml or an unreadable referenced document is a
            // usage/I-O failure (exit 2) — never a silent skip that would let
            // a broken suite report "all passed".
            Err(code) => return code,
        }
    }

    let passed = results.iter().filter(|r| r.passed).count();
    let failed = results.len() - passed;

    if json {
        print_json(&results, coverage.then_some(&cov));
    } else {
        print_human(dir, &results, coverage.then_some(&cov));
    }

    if failed > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// Trace one test file and evaluate its expectations. `Err(code)` is an I/O /
/// malformed-yaml failure (exit 2). `Ok` is a decided pass/fail verdict —
/// including a refused trace, which is a test FAILURE (semantic), not an I/O
/// error. When `cov` is `Some`, the produced report is folded into it.
fn run_one_test(
    test_file: &Path,
    providers: Option<&Path>,
    cov: Option<&mut CoverageAccum>,
) -> Result<TestResult, ExitCode> {
    let text = match std::fs::read_to_string(test_file) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("lute: cannot read {}: {e}", test_file.display());
            return Err(ExitCode::from(2));
        }
    };

    // The mock surfaces reuse `lute trace --mock`'s EXACT parser — `file:`/
    // `expect:` are simply unknown keys it ignores, so one parse serves both.
    let mocks = match parse_mock_yaml(&text) {
        Ok(m) => m,
        Err(d) => {
            eprintln!("lute: {}: [{}] {}", test_file.display(), d.code, d.message);
            return Err(ExitCode::from(2));
        }
    };

    // Parse `file:` and `expect:` from the same document as a YAML value,
    // mirroring `parse_mock_yaml`'s hand-rolled navigation (no serde derive
    // dependency added to this crate).
    let top: serde_yaml::Value = match serde_yaml::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("lute: {}: malformed test yaml: {e}", test_file.display());
            return Err(ExitCode::from(2));
        }
    };
    let map = match top.as_mapping() {
        Some(m) => m,
        None => {
            eprintln!(
                "lute: {}: a test file must be a YAML mapping with a `file:` key",
                test_file.display()
            );
            return Err(ExitCode::from(2));
        }
    };
    let rel = match map.get("file").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => {
            eprintln!(
                "lute: {}: missing required `file:` (path to the `.lute` under test)",
                test_file.display()
            );
            return Err(ExitCode::from(2));
        }
    };

    let base = test_file.parent().unwrap_or_else(|| Path::new("."));
    let lute_path = base.join(&rel);
    let lute_display = lute_path.display().to_string();

    let Some(input) = crate::build_input(&lute_path, providers, None) else {
        // build_input already printed the read error.
        return Err(ExitCode::from(2));
    };

    let (report, exit) = trace_document(&input, mocks);

    // A refused trace (document check errors or invalid mocks) cannot be
    // asserted against — mark the whole test failed with the reason.
    if let TraceExit::Refused(diags) = &exit {
        let reason = if diags.iter().any(|d| !d.code.starts_with("E-TRACE-")) {
            "trace refused: document has check error(s) — run `lute check` first".to_string()
        } else {
            "trace refused: invalid mock input".to_string()
        };
        return Ok(TestResult {
            test_file: test_file.to_path_buf(),
            lute_file: lute_display,
            exit: "refused".to_string(),
            passed: false,
            expectations: Vec::new(),
            refusal: Some(reason),
        });
    }

    let exit_str = match exit {
        TraceExit::Complete => "complete",
        TraceExit::Incomplete => "incomplete",
        TraceExit::Refused(_) => unreachable!("handled above"),
    };

    if let Some(cov) = cov {
        accumulate_coverage(cov, &report);
    }

    let expect = map.get("expect").and_then(|v| v.as_mapping());
    let mut expectations = Vec::new();

    if let Some(expect) = expect {
        // exit: complete | incomplete
        if let Some(want) = expect.get("exit").and_then(|v| v.as_str()) {
            expectations.push(ExpectResult {
                kind: "exit",
                subject: String::new(),
                expected: want.to_string(),
                actual: exit_str.to_string(),
                passed: want == exit_str,
            });
        }

        // transcriptContains: [substrings] — against the human transcript.
        if let Some(list) = expect.get("transcriptContains").and_then(|v| v.as_sequence()) {
            let transcript = report.render_human();
            for item in list {
                if let Some(sub) = item.as_str() {
                    expectations.push(ExpectResult {
                        kind: "transcriptContains",
                        subject: String::new(),
                        expected: sub.to_string(),
                        actual: if transcript.contains(sub) {
                            "present".to_string()
                        } else {
                            "absent".to_string()
                        },
                        passed: transcript.contains(sub),
                    });
                }
            }
        }

        // state: { path: literal } — against the FINAL written state.
        if let Some(state) = expect.get("state").and_then(|v| v.as_mapping()) {
            let final_state = final_state(&report);
            for (k, v) in state {
                let Some(path) = k.as_str() else { continue };
                let want = yaml_scalar_text(v).unwrap_or_default();
                let actual = final_state.get(path).cloned();
                expectations.push(ExpectResult {
                    kind: "state",
                    subject: path.to_string(),
                    expected: want.clone(),
                    actual: actual.clone().unwrap_or_else(|| "<never written>".to_string()),
                    passed: actual.as_deref() == Some(want.as_str()),
                });
            }
        }
    }

    let passed = expectations.iter().all(|e| e.passed);

    Ok(TestResult {
        test_file: test_file.to_path_buf(),
        lute_file: lute_display,
        exit: exit_str.to_string(),
        passed,
        expectations,
        refusal: None,
    })
}

/// The final scalar state the trace reports: the LAST `::set` write per path
/// across the walk (§4.5 steps). Seeds are inputs, not "reported" state, so
/// they are intentionally not folded in — an expectation asserts what the
/// walk PRODUCED.
fn final_state(report: &TraceReport) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for step in &report.steps {
        if let Step::Set { path, value, .. } = step {
            out.insert(path.clone(), value.clone());
        }
    }
    out
}

/// Fold one report's decisions + coverage counts into the run accumulator.
fn accumulate_coverage(cov: &mut CoverageAccum, report: &TraceReport) {
    cov.paths += 1;
    for d in &report.decisions {
        match d.construct.as_str() {
            "branch" | "hub" => {
                let entry = cov.choices.entry(d.id.clone()).or_default();
                entry.0.insert(d.outcome.clone());
                for e in &d.eligible {
                    entry.1.insert(e.clone());
                }
            }
            "match" => {
                cov.arms.entry(d.id.clone()).or_default().0.insert(d.outcome.clone());
            }
            _ => {}
        }
    }
    for (id, c) in &report.coverage.choices {
        let entry = cov.choices.entry(id.clone()).or_default();
        entry.2 = entry.2.max(c.total);
    }
    for (id, c) in &report.coverage.arms {
        let entry = cov.arms.entry(id.clone()).or_default();
        entry.1 = entry.1.max(c.total);
    }
}

/// Render a YAML scalar to its literal TEXT form, matching the shape
/// `lute-trace`'s mock parser coerces `state:` values through (bool/number/
/// string). A non-scalar yields `None`.
fn yaml_scalar_text(v: &serde_yaml::Value) -> Option<String> {
    match v {
        serde_yaml::Value::Bool(b) => Some(b.to_string()),
        serde_yaml::Value::Number(n) => Some(n.to_string()),
        serde_yaml::Value::String(s) => Some(s.clone()),
        _ => None,
    }
}

/// Recursively collect every `*.test.yaml` under `dir`, byte-sorted for
/// deterministic order — mirrors [`crate::find_lute_files`]'s walk (stack,
/// symlinked dirs not followed), filtered to the `.test.yaml` suffix.
fn find_test_files(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d)? {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                stack.push(path);
            } else if path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(".test.yaml"))
            {
                out.push(path);
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Human report: one block per test with per-expectation pass/fail lines on a
/// miss, then a `N passed, M failed` summary and (optional) coverage.
fn print_human(dir: &Path, results: &[TestResult], cov: Option<&CoverageAccum>) {
    if results.is_empty() {
        println!("no *.test.yaml files under {}", dir.display());
    }
    for r in results {
        let mark = if r.passed { "PASS" } else { "FAIL" };
        println!("{mark}  {}  ({})", r.test_file.display(), r.lute_file);
        if let Some(reason) = &r.refusal {
            println!("      {reason}");
            continue;
        }
        if !r.passed {
            for e in r.expectations.iter().filter(|e| !e.passed) {
                match e.kind {
                    "transcriptContains" => println!(
                        "      transcriptContains {:?}: {} (expected present)",
                        e.expected, e.actual
                    ),
                    "state" => println!(
                        "      state {}: expected {:?}, got {:?}",
                        e.subject, e.expected, e.actual
                    ),
                    "exit" => println!(
                        "      exit: expected {}, got {}",
                        e.expected, e.actual
                    ),
                    _ => {}
                }
            }
        }
    }

    let passed = results.iter().filter(|r| r.passed).count();
    let failed = results.len() - passed;
    println!("\n{passed} passed, {failed} failed");

    if let Some(cov) = cov {
        print_coverage_human(cov);
    }
}

/// Human coverage view — honest header, chosen/never-chosen names where the
/// reports expose them, counts where they do not.
fn print_coverage_human(cov: &CoverageAccum) {
    println!("\ncoverage over {} traced path(s):", cov.paths);
    if cov.choices.is_empty() && cov.arms.is_empty() {
        println!("  (no branch/hub or match constructs traced)");
        return;
    }
    for (id, (chosen, eligible_seen, total)) in &cov.choices {
        let never_named: Vec<&String> = eligible_seen.difference(chosen).collect();
        let mut line = format!(
            "  branch/hub {id}: {}/{} chosen",
            chosen.len().min(*total),
            total
        );
        if !chosen.is_empty() {
            line.push_str(&format!(
                " [{}]",
                chosen.iter().cloned().collect::<Vec<_>>().join(", ")
            ));
        }
        if !never_named.is_empty() {
            line.push_str(&format!(
                "; never chosen [{}]",
                never_named.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
            ));
        }
        // Choices never seen eligible in ANY traced path: count only, honest.
        let unseen = total.saturating_sub(chosen.len() + never_named.len());
        if unseen > 0 {
            line.push_str(&format!("; {unseen} never seen eligible in any traced path"));
        }
        println!("{line}");
    }
    for (id, (chosen, total)) in &cov.arms {
        let unexecuted = total.saturating_sub(chosen.len());
        let mut line = format!("  match `{id}`: {}/{} arm(s) executed", chosen.len().min(*total), total);
        if !chosen.is_empty() {
            line.push_str(&format!(
                " [{}]",
                chosen.iter().cloned().collect::<Vec<_>>().join(", ")
            ));
        }
        if unexecuted > 0 {
            line.push_str(&format!("; {unexecuted} unexecuted"));
        }
        println!("{line}");
    }
}

/// Machine report: per-test verdicts + expectations, the summary, and
/// (optional) coverage — stable-keyed JSON.
fn print_json(results: &[TestResult], cov: Option<&CoverageAccum>) {
    use serde_json::{json, Value};

    let tests: Vec<Value> = results
        .iter()
        .map(|r| {
            let expectations: Vec<Value> = r
                .expectations
                .iter()
                .map(|e| {
                    json!({
                        "kind": e.kind,
                        "subject": e.subject,
                        "expected": e.expected,
                        "actual": e.actual,
                        "passed": e.passed,
                    })
                })
                .collect();
            json!({
                "test": r.test_file.display().to_string(),
                "file": r.lute_file,
                "exit": r.exit,
                "passed": r.passed,
                "refusal": r.refusal,
                "expectations": expectations,
            })
        })
        .collect();

    let passed = results.iter().filter(|r| r.passed).count();
    let failed = results.len() - passed;

    let mut root = json!({
        "tests": tests,
        "summary": { "passed": passed, "failed": failed },
    });

    if let Some(cov) = cov {
        let choices: serde_json::Map<String, Value> = cov
            .choices
            .iter()
            .map(|(id, (chosen, eligible_seen, total))| {
                let never_named: Vec<&String> = eligible_seen.difference(chosen).collect();
                let unseen = total.saturating_sub(chosen.len() + never_named.len());
                (
                    id.clone(),
                    json!({
                        "total": total,
                        "chosen": chosen.iter().cloned().collect::<Vec<_>>(),
                        "neverChosen": never_named.iter().map(|s| (*s).clone()).collect::<Vec<_>>(),
                        "neverEligibleInAnyPath": unseen,
                    }),
                )
            })
            .collect();
        let arms: serde_json::Map<String, Value> = cov
            .arms
            .iter()
            .map(|(id, (chosen, total))| {
                (
                    id.clone(),
                    json!({
                        "total": total,
                        "executed": chosen.iter().cloned().collect::<Vec<_>>(),
                        "unexecuted": total.saturating_sub(chosen.len()),
                    }),
                )
            })
            .collect();
        root["coverage"] = json!({
            "tracedPaths": cov.paths,
            "choices": Value::Object(choices),
            "arms": Value::Object(arms),
        });
    }

    println!("{}", serde_json::to_string_pretty(&root).expect("report is JSON-serializable"));
}
