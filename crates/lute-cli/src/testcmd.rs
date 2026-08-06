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

use lute_trace::{parse_mock_surfaces, trace_document, Step, TraceExit, TraceReport};

/// The complete legal top-level key set of a `*.test.yaml` (module docs).
/// `expect:` is the harness's own; the other seven are exactly
/// [`lute_trace::MOCK_TOP_KEYS`], the mock family's own CLOSED set.
/// CLOSED as of 0.10.0 (#2(a), D-B): the grammar being open is what let a
/// `chooses:` typo drop a selection and green a test against the arm the file
/// excluded.
///
/// **The two sets are not identical and differ by exactly `expect:`** —
/// asserted by [`the_test_key_set_is_the_mock_key_set_plus_expect`], so the
/// two cannot drift when a surface is added on either side.
const TEST_TOP_KEYS: &[&str] =
    &["accept", "accepts", "choose", "events", "expect", "facts", "file", "state"];

/// The complete legal key set inside `expect:`. Also CLOSED. (b)/(c)'s new
/// expectation kinds — endings, quest lifecycle, `facts:` as an output — are
/// deferred with #19 (D-B); when they land they are added HERE, which is the
/// point of a closed set.
const TEST_EXPECT_KEYS: &[&str] = &["exit", "state", "transcriptContains"];

/// One `E-TEST-KEY` line for an unrecognised key, with the same
/// edit-distance did-you-mean four checker codes already use (dsl 0.5.0
/// §2.2), over the workspace's ONE suggestion helper. `where_` names the
/// level so a top-level typo and an `expect:`-level typo are
/// distinguishable.
fn unknown_key_line(where_: &str, key: &str, allowed: &[&str]) -> String {
    let sugg = lute_manifest::suggest::nearest(key, allowed.iter().copied(), 2)
        .map(|k| format!(" — did you mean `{k}`?"))
        .unwrap_or_default();
    format!(
        "error [E-TEST-KEY] unknown {where_} key `{key}` in a `*.test.yaml`{sugg} (legal: {})",
        allowed.join(", ")
    )
}

/// Every closed-key violation in one test file, both levels, in document
/// order. Empty when the file is well-keyed.
fn closed_key_violations(map: &serde_yaml::Mapping) -> Vec<String> {
    let mut out = Vec::new();
    for (k, v) in map {
        let Some(key) = k.as_str() else {
            out.push(
                "error [E-TEST-KEY] a top-level key must be a string in a `*.test.yaml`".to_string(),
            );
            continue;
        };
        if !TEST_TOP_KEYS.contains(&key) {
            out.push(unknown_key_line("top-level", key, TEST_TOP_KEYS));
            continue;
        }
        if key == "expect" {
            if let Some(em) = v.as_mapping() {
                for (ek, _) in em {
                    match ek.as_str() {
                        Some(ekey) if TEST_EXPECT_KEYS.contains(&ekey) => {}
                        Some(ekey) => {
                            out.push(unknown_key_line("`expect:`", ekey, TEST_EXPECT_KEYS))
                        }
                        None => out
                            .push("error [E-TEST-KEY] an `expect:` key must be a string".to_string()),
                    }
                }
            }
        }
    }
    out
}

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
    /// Every branch/hub the walk auto-picked because no supplied selection
    /// named it, rendered `"<id> -> <arm>"`. Legal and deliberate (§4.4), but
    /// silent — and silence is what turned a `chooses:` typo into a green run
    /// against the arm the file excluded (#2(d), T9.8).
    autopicked: Vec<String>,
    /// Populated only when a test cannot produce a report (a refused trace):
    /// one rendered line per diagnostic the refusal is holding, in the order
    /// `lute trace` would print them. Never a canned summary — three
    /// different faults used to render the same four words (#25, T9.11).
    refusal: Option<Vec<String>>,
}

/// Coverage accumulated across every traced path in the run, keyed by the
/// construct's whole-project identity — `"{file}:{id}"` for a branch/hub,
/// `"{file}:{line}:{column}"` for a match (#24, T9.13). Before 0.10.0 the key
/// was the guard TEXT, so six `<match on="true">` blocks across four files
/// rendered as one row reading `3/3`. Nothing here is presented as
/// whole-space coverage — only "what these N paths touched" (D1: trace
/// explains, it never proves).
#[derive(Default)]
struct CoverageAccum {
    /// key -> (label, chosen choice ids, choice ids seen eligible, total).
    choices: BTreeMap<String, (String, BTreeSet<String>, BTreeSet<String>, usize)>,
    /// key -> (label, chosen arm outcomes, total arms).
    arms: BTreeMap<String, (String, BTreeSet<String>, usize)>,
    /// Number of documents that produced a report (a non-refused trace).
    paths: usize,
    /// Canonicalised path of every `.lute` that produced a report, so the
    /// untested set is `walk(dir) \ this \ components` (#24's second half).
    traced_files: BTreeSet<String>,
}

/// A path in the one spelling both sides of the untested-set difference can
/// agree on. `TraceReport.file` comes from `base.join(&rel)` — for
/// `tests/../scenes/wake.lute` that is NOT what `find_lute_files` yields — so
/// the difference would report every document as untested without this.
/// Canonical paths are absolute and machine-specific and are used for the
/// comparison ONLY; the printed list keeps the walk's own display paths.
fn canonical_key(p: &std::path::Path) -> String {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf()).display().to_string()
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

    // #24's denominator: every `.lute` under this root that no test traced,
    // MINUS the component documents. `find_lute_files` is the SAME
    // byte-sorted, symlink-deduped walk `check-project` uses, so the two
    // surfaces agree about what a document is. Compared on canonical paths,
    // printed as the walk spelled them.
    //
    // A component is filtered out because it is UNTESTABLE, not untested: it
    // is reached only by `::use` from an importer, it is never the `file:` of
    // a `*.test.yaml`, and it produces no artifact anyone can execute
    // (`compile_all::is_component_file`'s own doc makes the identical call for
    // `--all`). Listing it would print a line no author can ever discharge,
    // which is the exact shape of the wound this release exists to close. The
    // component case already has its own honest surface, and it is not this
    // one: `W-COMPONENT-UNVERIFIED` (dsl 0.10.0 §9 rule 4, D-W) says the
    // component's contract was not verified, and says who decides.
    let untested: Vec<String> = if coverage {
        match crate::find_lute_files(dir) {
            Ok(all) => all
                .iter()
                .filter(|p| !cov.traced_files.contains(&canonical_key(p)))
                .filter(|p| !crate::compile_all::is_component_file(p))
                .map(|p| p.display().to_string())
                .collect(),
            Err(e) => {
                eprintln!("lute: cannot walk {} for the untested set: {e}", dir.display());
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };

    if json {
        print_json(&results, coverage.then_some(&cov), &untested);
    } else {
        print_human(dir, &results, coverage.then_some(&cov), &untested);
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

    // The mock surfaces reuse `lute trace --mock`'s EXACT parser, in its
    // OPEN form: this family's legal set adds `expect:`, and a key violation
    // here is a per-test failure (exit 1, below) rather than the mock
    // family's parse error (exit 2). `parse_mock_surfaces` therefore skips
    // the closed-key gate and `closed_key_violations` supplies it.
    let mocks = match parse_mock_surfaces(&text) {
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

    // #2(a) / D-B: the key set is closed at BOTH levels. A violation is a
    // per-test FAILURE (exit 1), not an I/O error (exit 2) — every offending
    // file must be named in one run, and the suite must keep going. T9.8's
    // acceptance test asks for exit 1 by name.
    let key_violations = closed_key_violations(map);
    if !key_violations.is_empty() {
        return Ok(TestResult {
            test_file: test_file.to_path_buf(),
            lute_file: String::new(),
            exit: "invalid".to_string(),
            passed: false,
            expectations: Vec::new(),
            autopicked: Vec::new(),
            refusal: Some(key_violations),
        });
    }
    let rel = match lute_trace::mock_subject(&text) {
        Ok(Some(s)) => s,
        Ok(None) => {
            eprintln!(
                "lute: {}: missing required `file:` (path to the `.lute` under test)",
                test_file.display()
            );
            return Err(ExitCode::from(2));
        }
        Err(d) => {
            eprintln!("lute: {}: [{}] {}", test_file.display(), d.code, d.message);
            return Err(ExitCode::from(2));
        }
    };

    let base = test_file.parent().unwrap_or_else(|| Path::new("."));
    let lute_path = base.join(&rel);
    let lute_display = lute_path.display().to_string();

    let Some(built) = crate::build_input(&lute_path, providers, None) else {
        // build_input already printed the read error.
        return Err(ExitCode::from(2));
    };
    built.report_project_diags();
    let crate::BuiltInput { input, resolve_error, .. } = built;
    // plugin 0.0.2 §2: an `E-` capability-resolution diagnostic (bad plugin
    // option, missing active plugin, bad identity template) is a build-failing
    // error; it printed above, and it MUST gate here or it would pass silently.
    if resolve_error {
        return Err(ExitCode::from(1));
    }

    let (report, exit) = trace_document(&input, mocks);

    // A refused trace (document check errors or invalid mocks) cannot be
    // asserted against — mark the whole test failed and print every
    // diagnostic the refusal is holding. The harness used to inspect these
    // codes only to choose between two canned strings and then drop the
    // vector, so a stale `choose:` id, a stale branch id and a stale
    // `state:` path were indistinguishable (#25, T9.11).
    if let TraceExit::Refused(diags) = &exit {
        let lines: Vec<String> = diags
            .iter()
            .map(|d| {
                format!(
                    "{}:{}:{}: error [{}] {}",
                    lute_display,
                    d.span.line,
                    d.span.column,
                    d.code,
                    yaml_key_spelling(&d.message)
                )
            })
            .collect();
        return Ok(TestResult {
            test_file: test_file.to_path_buf(),
            lute_file: lute_display,
            exit: "refused".to_string(),
            passed: false,
            expectations: Vec::new(),
            autopicked: Vec::new(),
            refusal: Some(lines),
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

    // §4.4's auto-pick is legal and deliberate, but it was silent — and
    // silence is what let a dropped selection green a test against the arm
    // the file excluded (#2(d), T9.8).
    let autopicked: Vec<String> = report
        .decisions
        .iter()
        .filter(|d| d.auto && matches!(d.construct.as_str(), "branch" | "hub"))
        .map(|d| format!("{} -> {}", d.id, d.outcome))
        .collect();

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

    // #2(d) / D-B: `all()` over an empty vector is `true`, so a test that
    // recognised no expectation reported PASS. A test that asserts nothing is
    // not a passing test.
    if expectations.is_empty() {
        return Ok(TestResult {
            test_file: test_file.to_path_buf(),
            lute_file: lute_display,
            exit: exit_str.to_string(),
            passed: false,
            expectations: Vec::new(),
            autopicked,
            refusal: Some(vec![format!(
                "error [E-TEST-NO-EXPECT] this test declares no recognised expectation \
                 (legal `expect:` keys: {}); a test that asserts nothing cannot pass",
                TEST_EXPECT_KEYS.join(", ")
            )]),
        });
    }

    let passed = expectations.iter().all(|e| e.passed);

    Ok(TestResult {
        test_file: test_file.to_path_buf(),
        lute_file: lute_display,
        exit: exit_str.to_string(),
        passed,
        expectations,
        autopicked,
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

/// T9.11's second half. `lute-trace` composes its mock diagnostics for
/// `lute trace`'s command line (`--choose id=arm`, `--state path=value`,
/// `--fact`, `--event`, `--accept`), but in a `*.test.yaml` the same input
/// arrived as a YAML KEY. Printing the message verbatim names a syntax the
/// file cannot use. This rewrites the flag spelling to the key spelling and
/// nothing else — the codes, ids, values and clause citations are
/// `lute-trace`'s and stay exactly as written.
fn yaml_key_spelling(message: &str) -> String {
    let mut out = message.to_string();
    for (flag, key) in [
        ("--choose ", "choose: "),
        ("--state ", "state: "),
        ("--fact ", "facts: "),
        ("--event ", "events: "),
        ("--accept ", "accepts: "),
    ] {
        out = out.replace(flag, key);
    }
    out
}

/// Fold one report's decisions + coverage counts into the run accumulator.
fn accumulate_coverage(cov: &mut CoverageAccum, report: &TraceReport) {
    cov.paths += 1;
    cov.traced_files.insert(canonical_key(std::path::Path::new(&report.file)));
    for d in &report.decisions {
        match d.construct.as_str() {
            "branch" | "hub" => {
                let key = format!("{}:{}", report.file, d.id);
                let entry = cov
                    .choices
                    .entry(key)
                    .or_insert_with(|| (d.id.clone(), BTreeSet::new(), BTreeSet::new(), 0));
                entry.1.insert(d.outcome.clone());
                for e in &d.eligible {
                    entry.2.insert(e.clone());
                }
            }
            "match" => {
                let key = format!("{}:{}:{}", report.file, d.span.line, d.span.column);
                let entry =
                    cov.arms.entry(key).or_insert_with(|| (d.id.clone(), BTreeSet::new(), 0));
                entry.1.insert(d.outcome.clone());
            }
            _ => {}
        }
    }
    for c in report.coverage.choices.values() {
        let key = format!("{}:{}", report.file, c.label);
        let entry = cov
            .choices
            .entry(key)
            .or_insert_with(|| (c.label.clone(), BTreeSet::new(), BTreeSet::new(), 0));
        entry.3 = entry.3.max(c.total);
    }
    for (site, c) in &report.coverage.arms {
        let key = format!("{}:{site}", report.file);
        let entry = cov.arms.entry(key).or_insert_with(|| (c.label.clone(), BTreeSet::new(), 0));
        entry.2 = entry.2.max(c.total);
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
fn print_human(
    dir: &Path,
    results: &[TestResult],
    cov: Option<&CoverageAccum>,
    untested: &[String],
) {
    if results.is_empty() {
        println!("no *.test.yaml files under {}", dir.display());
    }
    for r in results {
        let mark = if r.passed { "PASS" } else { "FAIL" };
        println!("{mark}  {}  ({})", r.test_file.display(), r.lute_file);
        if let Some(lines) = &r.refusal {
            // The vector carries either the trace's own held diagnostics
            // (#25) or the harness's own `E-TEST-*` refusals (#2). Only the
            // first is a *trace* refusal, so only it gets that header.
            if r.exit == "refused" {
                println!("      trace refused:");
            }
            for line in lines {
                println!("        {line}");
            }
            for a in &r.autopicked {
                println!("      auto-picked (no selection supplied): {a}");
            }
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
        for a in &r.autopicked {
            println!("      auto-picked (no selection supplied): {a}");
        }
    }

    let passed = results.iter().filter(|r| r.passed).count();
    let failed = results.len() - passed;
    println!("\n{passed} passed, {failed} failed");

    if let Some(cov) = cov {
        print_coverage_human(cov, untested);
    }
}

/// Human coverage view — honest header, chosen/never-chosen names where the
/// reports expose them, counts where they do not. Every row names its
/// construct's own file and site; the guard text rides along as a label
/// (#24, T9.13). `untested` is already filtered to TESTABLE documents by the
/// caller — components are not in it, and the strings below say so.
fn print_coverage_human(cov: &CoverageAccum, untested: &[String]) {
    println!("\ncoverage over {} traced path(s):", cov.paths);
    if cov.choices.is_empty() && cov.arms.is_empty() {
        println!("  (no branch/hub or match constructs traced)");
    }
    for (key, (label, chosen, eligible_seen, total)) in &cov.choices {
        let never_named: Vec<&String> = eligible_seen.difference(chosen).collect();
        let mut line =
            format!("  branch/hub {label} ({key}): {}/{} chosen", chosen.len().min(*total), total);
        if !chosen.is_empty() {
            line.push_str(&format!(" [{}]", chosen.iter().cloned().collect::<Vec<_>>().join(", ")));
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
    for (key, (label, chosen, total)) in &cov.arms {
        let unexecuted = total.saturating_sub(chosen.len());
        let mut line = format!(
            "  match `{label}` ({key}): {}/{} arm(s) executed",
            chosen.len().min(*total),
            total
        );
        if !chosen.is_empty() {
            line.push_str(&format!(" [{}]", chosen.iter().cloned().collect::<Vec<_>>().join(", ")));
        }
        if unexecuted > 0 {
            line.push_str(&format!("; {unexecuted} unexecuted"));
        }
        println!("{line}");
    }
    // T9.13's real design hole: coverage accumulated only from reports that
    // RAN, so deleting a test made its scene invisible rather than untested.
    // Both strings say "testable", because component documents are out of the
    // denominator and claiming otherwise is the false-reassurance this whole
    // task is about.
    if untested.is_empty() {
        println!("  every testable document under this root is named by at least one test");
    } else {
        println!("  {} untested document(s) — no *.test.yaml names them:", untested.len());
        for f in untested {
            println!("    {f}");
        }
    }
}

/// Machine report: per-test verdicts + expectations, the summary, and
/// (optional) coverage — stable-keyed JSON.
fn print_json(results: &[TestResult], cov: Option<&CoverageAccum>, untested: &[String]) {
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
                "autopicked": r.autopicked.clone(),
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
            .map(|(key, (label, chosen, eligible_seen, total))| {
                let never_named: Vec<&String> = eligible_seen.difference(chosen).collect();
                let unseen = total.saturating_sub(chosen.len() + never_named.len());
                (
                    key.clone(),
                    json!({
                        "label": label,
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
            .map(|(key, (label, chosen, total))| {
                (
                    key.clone(),
                    json!({
                        "label": label,
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
            "untested": untested,
        });
    }

    println!("{}", serde_json::to_string_pretty(&root).expect("report is JSON-serializable"));
}

#[cfg(test)]
mod tests {
    use super::TEST_TOP_KEYS;

    /// The two closed key sets differ by **exactly** `expect:` — the claim
    /// `TEST_TOP_KEYS`' and `MOCK_TOP_KEYS`' doc comments both make. A new
    /// mock surface added on one side and not the other is the drift this
    /// catches: adding `seed:` to `MOCK_TOP_KEYS` alone would make a
    /// `*.test.yaml` reject a key its own mock parser reads, and adding it to
    /// `TEST_TOP_KEYS` alone would re-open the hole T3.10 filed.
    #[test]
    fn the_test_key_set_is_the_mock_key_set_plus_expect() {
        let mut want: Vec<&str> = lute_trace::MOCK_TOP_KEYS.to_vec();
        want.push("expect");
        want.sort_unstable();
        assert_eq!(TEST_TOP_KEYS.to_vec(), want);
        assert!(!lute_trace::MOCK_TOP_KEYS.contains(&"expect"));
    }
}
