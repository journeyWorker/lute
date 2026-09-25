//! `lute test` — scenario tests: declared mocks + declared expectations,
//! layered on `lute-trace`'s deterministic walk.
//!
//! A `*.test.yaml` file names a `.lute` document (`file:`, resolved relative
//! to the TEST file's own directory), carries the SAME mock surfaces
//! `lute trace --mock` accepts (`state:`/`facts:`/`choose:`/`events:`/
//! `accepts:`/`visited:`/`occasions:`/`quests:`/`entriesRead:`/`derive:`,
//! parsed by the SAME mock grammar), and declares an `expect:` block:
//!
//! ```yaml
//! file: ../scenes/confrontation.lute
//! state:            # mock seed (same as `lute trace --mock`)
//!   run.trueKiller: blake
//! choose:
//!   accuse: accuseBlake
//! expect:
//!   transcriptContains: ["Case closed."]
//!   transcriptLacks: ["Blake walks free."]
//!   offered: { accuse: [accuseBlake, accuseMoss] } # the options offered there
//!   state: { run.accused: blake }   # the FINAL effective state (write → seed → default)
//!   quests: { caseClosed: complete } # unset | active | complete | failed
//!   exit: complete                  # complete | incomplete
//! ```
//!
//! Each test traces its document once ([`trace_with_check`]) and checks every
//! declared expectation, naming actual-vs-expected on any miss. An
//! `incomplete` trace (an unknown guard halted the walk) FAILS unless the
//! test declares `expect: { exit: incomplete }` — a walk that stopped halfway
//! proves nothing about the expectations it never reached (0.21.1, T1-13).
//! A lore document is looked up, not played: its test names the entries to
//! present — `entry: <id>` or `entries: [ids]`, walked in order with the
//! read flags set between them ([`trace_entries_with_check`], dsl 0.22.0
//! §5) — and without one it is `E-TEST-LORE`.
//!
//! Every `*.play.yaml` under the directory that carries an `expect:` (on a
//! step or at the top) runs too, through `lute play`'s own walk and judge
//! ([`crate::play::run_play_for_test`], [`crate::play_expect`]); it passes
//! when every expectation holds and the play completed (or its top-level
//! `expect:` declares the exit). Exit `0` when all pass, `1` when any fails,
//! `2` on an I/O failure or a malformed test yaml. `--coverage` reports
//! chosen-vs-never-chosen choices and executed-vs-unexecuted match arms
//! aggregated across every traced path (honest: "over N traced paths", never
//! a whole-space coverage claim), and lists the untested documents of the
//! PROJECT — `--project <dir>`, else the nearest `lute.project.yaml` above
//! the test directory — not merely of the directory the tests live in. A
//! document a play presented is covered.
//!
//! `--project <dir>` resolves every traced document EXACTLY as `lute trace
//! <file> --project <dir>` does — same flag, same help text, same
//! [`crate::build_input`] call, same provider-catalog precedence (plugin
//! §10: an explicit `--providers <dir>` wins; otherwise auto-discover
//! through the project's pinned catalog). Before this flag existed, every
//! test traced with a hardcoded `project: None`, so a document whose schema
//! or directives depend on the manifest's `defaults: uses:` hoist or on a
//! `profile:`-activated plugin failed EVERY test with `E-DOMAIN-UNKNOWN` /
//! `E-UNDECLARED` / `E-UNKNOWN-DIRECTIVE` regardless of test content — the
//! harness could resolve mocks and choices, but never the project the
//! document was written against.
//!
//! `--project` here is unrelated to backlog #19 / T9.7 (0.9.0 improvement
//! backlog, T9.7): that item is about `lute test` walking the *artifact*
//! rather than the *source* and about the derived-relation (Datalog)
//! fixpoint — it changes WHAT the harness walks. This flag changes only
//! WHETHER the walk can see the manifest at all. The two are independent;
//! this fix neither requires nor precludes that one.
//!
//! No `lute.project.yaml` is auto-discovered for RESOLVING the document when
//! `--project` is omitted — matching every other command (`build_input`
//! resolves a project only from an explicit flag); a document whose schema
//! depends on a manifest the caller did not name still resolves core-only,
//! exactly as before. The nearest manifest is consulted only for what is a
//! property of the project rather than of the document: the coverage
//! denominator and the producer set `W-TRACE-MOCK-UNPRODUCIBLE` judges
//! mocked facts against (T1-14).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lute_trace::{
    parse_mock_surfaces, trace_beat_with_check, trace_entries_with_check, trace_with_check,
    TraceExit, TraceReport,
    UnresolvedEntry,
};

use crate::play_expect::ExpectMiss;

/// The complete legal top-level key set of a `*.test.yaml` (module docs).
/// [`HARNESS_KEYS`] are the harness's own; every other key is exactly
/// [`lute_trace::MOCK_TOP_KEYS`], the mock family's own CLOSED set.
/// CLOSED as of 0.10.0 (#2(a), D-B): the grammar being open is what let a
/// `chooses:` typo drop a selection and green a test against the arm the file
/// excluded.
///
/// **The two sets differ by exactly [`HARNESS_KEYS`]** — asserted by
/// [`the_test_key_set_is_the_mock_key_set_plus_the_harness_keys`], so the
/// two cannot drift when a surface is added on either side.
const TEST_TOP_KEYS: &[&str] = &[
    "accept",
    "accepts",
    "beat",
    "choose",
    "derive",
    "entries",
    "entriesRead",
    "entry",
    "events",
    "expect",
    "facts",
    "file",
    "occasions",
    "quests",
    "state",
    "visited",
];

/// The `*.test.yaml` keys that are not mock surfaces: the expectations, and
/// the lore entries (dsl 0.22.0 §5) or bundle beat (0.23.1) a test presents.
/// Only the drift test reads it — the runtime gate is the literal
/// [`TEST_TOP_KEYS`] — so it is test-only rather than a dead constant in the
/// binary.
#[cfg(test)]
const HARNESS_KEYS: &[&str] = &["beat", "entries", "entry", "expect"];

/// The complete legal key set inside `expect:`. Also CLOSED — a new
/// expectation kind is added HERE, which is the point of a closed set.
/// `quests:` (dsl 0.21.0 §7a.4) asserts the quest lifecycle the trace ran;
/// `offered:` / `transcriptLacks:` are dsl 0.22.0 §5; `facts:` / `notFacts:`
/// (every fact that holds at the end, after derivation) and `eligible:` (a
/// presented entry's or beat's `when`) are 0.23.1.
const TEST_EXPECT_KEYS: &[&str] = &[
    "eligible",
    "exit",
    "facts",
    "notFacts",
    "offered",
    "quests",
    "state",
    "transcriptContains",
    "transcriptLacks",
];

/// The quest lifecycle states an `expect.quests` entry may name (dsl 0.21.0
/// §7a.4) — `unset` is the pre-activation absence, the other three the
/// engine's `quest.<id>.state` domain.
const QUEST_STATES: &[&str] = &["unset", "active", "complete", "failed"];

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
                "error [E-TEST-KEY] a top-level key must be a string in a `*.test.yaml`"
                    .to_string(),
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
                        None => out.push(
                            "error [E-TEST-KEY] an `expect:` key must be a string".to_string(),
                        ),
                    }
                }
            }
        }
    }
    out
}

/// How this report DISPLAYS a state path the walk never wrote. It is a
/// RENDERING of "there is no value here", not a member of the value space —
/// which is exactly what T9.9 got wrong: the sentinel was substituted for the
/// absent value before the miss line was built, so a test expecting the
/// sentinel's own text printed `expected "<never written>", got "<never
/// written>"` while correctly failing. Two identical strings declared
/// different.
const NEVER_WRITTEN: &str = "<never written>";

/// One declared expectation's verdict, carrying enough to render both the
/// human miss line and the `--json` entry.
struct ExpectResult {
    kind: &'static str,
    /// A stable label for the checked thing (e.g. the state path, or empty).
    subject: String,
    expected: String,
    /// The observed value, or `None` when there is no value to observe — a
    /// `state:` path the walk never wrote. `None` is not a string and so can
    /// never compare equal to an expected literal, whatever that literal
    /// spells; [`NEVER_WRITTEN`] is applied at RENDER time only, and never on
    /// a line that also prints the expected side.
    actual: Option<String>,
    passed: bool,
}

/// One test file's — or expect-carrying play's — outcome.
struct TestResult {
    test_file: PathBuf,
    /// `"test"` for a `*.test.yaml`, `"play"` for a `*.play.yaml` (dsl
    /// 0.22.0 §4).
    kind: &'static str,
    /// The traced document; for a play, the project directory it played.
    lute_file: String,
    exit: String,
    passed: bool,
    expectations: Vec<ExpectResult>,
    /// A play's failed expectations, exactly as `lute play` reports them.
    /// Always empty for a `*.test.yaml`.
    misses: Vec<ExpectMiss>,
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
    /// Why the walk did not decide everything (T3-11): the guards that halted
    /// it or stayed unknown, each with the atoms a mock would decide. A
    /// failing test used to print only `exit: expected complete, got
    /// incomplete`, hiding the one thing the author needs next.
    unresolved: Vec<UnresolvedEntry>,
    /// Selections the test's `choose:` forced past an unknown guard — the
    /// walk continued, but that guard was never decided (T1-13).
    forced_unknown: Vec<UnresolvedEntry>,
    /// A test: the trace's beat-`when` note, when the scene's own eligibility
    /// does not hold under the test's mocks (T1-13) — shown on a PASS too,
    /// because a scene the selector would never present passing its test is
    /// the silence this note exists to break. A play: why it halted.
    notes: Vec<String>,
}

impl TestResult {
    /// A test that produced no report to assert against.
    fn refused(test_file: &Path, lute_file: String, exit: &str, lines: Vec<String>) -> Self {
        TestResult {
            test_file: test_file.to_path_buf(),
            kind: "test",
            lute_file,
            exit: exit.to_string(),
            passed: false,
            expectations: Vec::new(),
            misses: Vec::new(),
            autopicked: Vec::new(),
            refusal: Some(lines),
            unresolved: Vec::new(),
            forced_unknown: Vec::new(),
            notes: Vec::new(),
        }
    }
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
    /// Number of expect-carrying plays that ran (dsl 0.22.0 §4); every
    /// document they presented is in `traced_files`.
    plays: usize,
    /// Canonicalised path of every `.lute` that produced a report or that a
    /// play presented, so the untested set is `walk(dir) \ this \
    /// components` (#24's second half).
    traced_files: BTreeSet<String>,
}

/// A path in the one spelling both sides of the untested-set difference can
/// agree on. `TraceReport.file` comes from `base.join(&rel)` — for
/// `tests/../scenes/wake.lute` that is NOT what `find_lute_files` yields — so
/// the difference would report every document as untested without this.
/// Canonical paths are absolute and machine-specific and are used for the
/// comparison ONLY; the printed list keeps the walk's own display paths.
fn canonical_key(p: &std::path::Path) -> String {
    std::fs::canonicalize(p)
        .unwrap_or_else(|_| p.to_path_buf())
        .display()
        .to_string()
}

/// Run every `*.test.yaml` scenario test under `dir`, then every
/// `*.play.yaml` under it that carries an `expect:` (dsl 0.22.0 §4). `dir`
/// may also be ONE `*.test.yaml` or `*.play.yaml` file, which runs alone.
/// See [`crate::Command::Test`].
pub fn run_test(
    dir: &Path,
    json: bool,
    providers: Option<&Path>,
    project: Option<&Path>,
    coverage: bool,
    no_derive: bool,
) -> ExitCode {
    let single = dir.is_file().then(|| {
        let name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
        (name.ends_with(".test.yaml"), name.ends_with(".play.yaml"))
    });
    let (test_files, play_files) = match single {
        Some((true, _)) => (vec![dir.to_path_buf()], Vec::new()),
        Some((_, true)) => (Vec::new(), vec![dir.to_path_buf()]),
        Some(_) => {
            eprintln!(
                "lute: {} is not a directory, a `*.test.yaml` or a `*.play.yaml`",
                dir.display()
            );
            return ExitCode::from(2);
        }
        None => match (
            find_files_with_suffix(dir, ".test.yaml"),
            find_files_with_suffix(dir, ".play.yaml"),
        ) {
            (Ok(t), Ok(p)) => (t, p),
            (Err(e), _) | (_, Err(e)) => {
                eprintln!("lute: cannot walk {}: {e}", dir.display());
                return ExitCode::from(2);
            }
        },
    };

    let mut results = Vec::new();
    let mut cov = CoverageAccum::default();
    let mut producers = ProducerCache::default();

    for test_file in &test_files {
        match run_one_test(
            test_file,
            providers,
            project,
            no_derive,
            &mut producers,
            coverage.then_some(&mut cov),
        ) {
            Ok(r) => results.push(r),
            // A malformed test yaml or an unreadable referenced document is a
            // usage/I-O failure (exit 2) — never a silent skip that would let
            // a broken suite report "all passed".
            Err(code) => return code,
        }
    }
    for play_file in &play_files {
        if let Some(r) = run_one_play(play_file, project, no_derive, coverage.then_some(&mut cov))
        {
            results.push(r);
        }
    }

    let passed = results.iter().filter(|r| r.passed).count();
    let failed = results.len() - passed;

    // #24's denominator: every `.lute` under the PROJECT root that no test
    // traced and no play presented, MINUS the component documents. The root
    // is `--project`, else the nearest `lute.project.yaml` above `dir`, else
    // `dir` itself — the
    // tests conventionally live in `tests/`, and measuring against the walk
    // root there made `every testable document … is named` vacuously true
    // (T1-13). `find_lute_files` is the SAME byte-sorted, symlink-deduped
    // walk `check-project` uses, so the two surfaces agree about what a
    // document is. Compared on canonical paths, printed as the walk spelled
    // them.
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
    let walk_root = if single.is_some() {
        dir.parent().unwrap_or_else(|| Path::new("."))
    } else {
        dir
    };
    let coverage_root: PathBuf = match project {
        Some(p) => p.to_path_buf(),
        None => match crate::nearest_manifest_dir(dir) {
            Some(root) if canonical_key(&root) != canonical_key(walk_root) => root,
            _ => walk_root.to_path_buf(),
        },
    };
    let untested: Vec<String> = if coverage {
        match crate::find_lute_files(&coverage_root) {
            Ok(all) => all
                .iter()
                .filter(|p| !cov.traced_files.contains(&canonical_key(p)))
                .filter(|p| !crate::compile_all::is_component_file(p))
                .map(|p| p.display().to_string())
                .collect(),
            Err(e) => {
                eprintln!(
                    "lute: cannot walk {} for the untested set: {e}",
                    coverage_root.display()
                );
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };

    // T3-15: the whole report is rendered first and written once, so a
    // closed pipe (`lute test … | head`) is an I/O exit rather than a panic.
    let text = if json {
        render_json(&results, coverage.then_some((&cov, coverage_root.as_path())), &untested)
    } else {
        render_human(
            dir,
            &results,
            coverage.then_some((&cov, coverage_root.as_path())),
            &untested,
        )
    };
    if crate::write_stdout(&text).is_err() {
        return ExitCode::from(2);
    }

    if failed > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// The project May producer set per project root (T1-14), computed once per
/// root for the whole run — every test of a project shares it, and it costs
/// a full project collection.
#[derive(Default)]
struct ProducerCache {
    by_root: BTreeMap<PathBuf, Option<BTreeSet<String>>>,
    /// Project roots a scenario test discovered (nearest `lute.project.yaml`,
    /// no `--project`) and already announced on stderr — one note per root.
    noted: BTreeSet<PathBuf>,
}

impl ProducerCache {
    /// The producer set of the project `lute_path` belongs to: `--project`
    /// when given (every file resolves against it, as the trace gate does),
    /// else the nearest `lute.project.yaml`. `None` when there is no project
    /// to consult or it could not be collected — the trace then judges the
    /// document alone and its note says so.
    fn for_document(
        &mut self,
        lute_path: &Path,
        project: Option<&Path>,
        providers: Option<&Path>,
    ) -> Option<&BTreeSet<String>> {
        let (root, single_root) = match project {
            Some(p) => (p.to_path_buf(), true),
            None => (crate::nearest_manifest_dir(lute_path)?, false),
        };
        self.by_root
            .entry(root)
            .or_insert_with_key(|root| {
                crate::project_assert_relations(root, single_root, providers)
            })
            .as_ref()
    }
}

/// Trace one test file and evaluate its expectations. `Err(code)` is an I/O /
/// malformed-yaml failure (exit 2). `Ok` is a decided pass/fail verdict —
/// including a refused trace, which is a test FAILURE (semantic), not an I/O
/// error. When `cov` is `Some`, the produced report is folded into it.
///
/// `project` resolves the traced document EXACTLY as `lute trace --project`
/// does (module docs): threaded straight into [`crate::build_input`], never
/// substituted for `None`. A project-resolution `E-` diagnostic
/// (`resolve_error`) is therefore a build-failing error here too — `Err(1)`,
/// never folded into a per-test `TestResult` where a caller filtering on
/// `passed` could mistake a broken manifest for a failing assertion.
fn run_one_test(
    test_file: &Path,
    providers: Option<&Path>,
    project: Option<&Path>,
    no_derive: bool,
    producers: &mut ProducerCache,
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
    // OPEN form: this family's legal set adds [`HARNESS_KEYS`], and a key
    // violation here is a per-test failure (exit 1, below) rather than the
    // mock family's parse error (exit 2). `parse_mock_surfaces` therefore
    // skips the closed-key gate and `closed_key_violations` supplies it.
    let mut mocks = match parse_mock_surfaces(&text) {
        Ok(m) => m,
        Err(d) => {
            eprintln!("lute: {}: [{}] {}", test_file.display(), d.code, d.message);
            return Err(ExitCode::from(2));
        }
    };
    // `--no-derive` wins over the test's own `derive:` key (dsl 0.22.0 §6).
    if no_derive {
        mocks.derive = Some(false);
    }

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
        return Ok(TestResult::refused(
            test_file,
            String::new(),
            "invalid",
            key_violations,
        ));
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
    // dsl 0.22.0 §5: `entry: <id>` / `entries: [ids]` — the lore entries to
    // present, in order. 0.23.1: `beat: <id>` — one bundle `<beat>` (local
    // or canonical `<document id>.<beat id>`), as `lute trace --beat`.
    let entries: Vec<String> = match (map.get("entry"), map.get("entries")) {
        (None, None) => Vec::new(),
        (Some(serde_yaml::Value::String(id)), None) => vec![id.clone()],
        (None, Some(serde_yaml::Value::Sequence(ids))) if ids.iter().all(|i| i.is_string()) => {
            ids.iter().filter_map(|i| i.as_str().map(str::to_string)).collect()
        }
        (Some(_), Some(_)) => {
            eprintln!(
                "lute: {}: name the entries to present with ONE of `entry: <id>` or \
                 `entries: [ids]`, not both",
                test_file.display()
            );
            return Err(ExitCode::from(2));
        }
        _ => {
            eprintln!(
                "lute: {}: `entry:` must be one entry id and `entries:` a list of entry ids",
                test_file.display()
            );
            return Err(ExitCode::from(2));
        }
    };
    let beat: Option<String> = match map.get("beat") {
        None => None,
        Some(serde_yaml::Value::String(id)) if !id.trim().is_empty() => Some(id.clone()),
        Some(_) => {
            eprintln!(
                "lute: {}: `beat:` must be one bundle beat id (`<beat id>` or \
                 `<document id>.<beat id>`)",
                test_file.display()
            );
            return Err(ExitCode::from(2));
        }
    };
    if beat.is_some() && !entries.is_empty() {
        eprintln!(
            "lute: {}: a test presents ONE of `beat:` or `entry:` / `entries:`, not both",
            test_file.display()
        );
        return Err(ExitCode::from(2));
    }

    let base = test_file.parent().unwrap_or_else(|| Path::new("."));
    let lute_path = base.join(&rel);
    let lute_display = lute_path.display().to_string();

    // One test naming a document that no longer exists is that test's
    // failure, never the whole suite's abort.
    if !lute_path.is_file() {
        return Ok(TestResult::refused(
            test_file,
            lute_display.clone(),
            "invalid",
            vec![format!(
                "error [E-TEST-FILE] `file: {rel}` names no document ({lute_display} does not \
                 exist)"
            )],
        ));
    }

    // 0.23.1: like `lute check <file>` and like the plays of this run, a
    // scenario test without `--project` resolves its document against the
    // nearest `lute.project.yaml` — announced once per project root.
    let discovered = match project {
        Some(_) => None,
        None => crate::nearest_manifest_dir(&lute_path),
    };
    if let Some(root) = &discovered {
        if producers.noted.insert(root.clone()) {
            let shown = crate::cwd_relative(&root.display().to_string());
            eprintln!(
                "lute: note: scenario tests use project {} (nearest lute.project.yaml); pass \
                 --project to choose another",
                if shown.is_empty() { "." } else { shown.as_str() }
            );
        }
    }
    let resolve_with = project.or(discovered.as_deref());

    let Some(built) = crate::build_input(&lute_path, providers, resolve_with, None) else {
        // build_input already printed the read error.
        return Err(ExitCode::from(2));
    };
    built.report_project_diags();
    let crate::BuiltInput {
        input,
        resolve_error,
        ..
    } = built;
    // plugin 0.0.2 §2: an `E-` capability-resolution diagnostic (bad plugin
    // option, missing active plugin, bad identity template) is a build-failing
    // error; it printed above, and it MUST gate here or it would pass silently.
    if resolve_error {
        return Err(ExitCode::from(1));
    }

    // T1-13: a lore document is looked up, not played — `lute trace` refuses
    // one without `--entry`/`--beat`, and a test naming it without `entry:`
    // / `entries:` / `beat:` would walk nothing and PASS `exit: complete`.
    // Say so instead of asserting against nothing.
    if entries.is_empty() && beat.is_none() {
        let (doc, _) = lute_syntax::parse(&input.text);
        let (folded, _, _) = lute_check::fold_env(&doc, &input);
        if folded.doc_kind == lute_check::DocKind::Lore {
            let ids: Vec<&str> = doc.entries.iter().map(|e| e.id.as_str()).collect();
            let beats: Vec<&str> = doc.beats.iter().map(|b| b.id.as_str()).collect();
            return Ok(TestResult::refused(
                test_file,
                lute_display,
                "invalid",
                vec![format!(
                    "error [E-TEST-LORE] `file: {rel}` is a lore document — a lore document is \
                     looked up, not played, so there is no walk to assert against until the \
                     test names what to present: `entry: <id>` or `entries: [ids]` (declared: \
                     {}), or `beat: <id>` (declared: {})",
                    if ids.is_empty() { "none".to_string() } else { ids.join(", ") },
                    if beats.is_empty() { "none".to_string() } else { beats.join(", ") }
                )],
            ));
        }
    }

    // T1-14: mocked facts are judged against the project's producers.
    let project_asserts = if mocks.facts.is_empty() {
        None
    } else {
        producers
            .for_document(&lute_path, project, providers)
            .cloned()
    };
    let checked = lute_check::check(&input);
    let (report, exit) = if let Some(beat) = &beat {
        trace_beat_with_check(&input, checked, mocks, beat, project_asserts.as_ref())
    } else if entries.is_empty() {
        trace_with_check(&input, checked, mocks, project_asserts.as_ref())
    } else {
        let ids: Vec<&str> = entries.iter().map(String::as_str).collect();
        trace_entries_with_check(&input, checked, mocks, &ids, project_asserts.as_ref())
    };

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
        return Ok(TestResult::refused(
            test_file,
            lute_display,
            "refused",
            lines,
        ));
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
                actual: Some(exit_str.to_string()),
                passed: want == exit_str,
            });
        }

        // transcriptContains / transcriptLacks: [substrings] — against the
        // human transcript (dsl 0.22.0 §5 adds the negative form).
        let transcript = if expect.contains_key("transcriptContains")
            || expect.contains_key("transcriptLacks")
        {
            report.render_human()
        } else {
            String::new()
        };
        for (kind, want_present) in [("transcriptContains", true), ("transcriptLacks", false)] {
            let Some(list) = expect.get(kind).and_then(|v| v.as_sequence()) else {
                continue;
            };
            for sub in list.iter().filter_map(|i| i.as_str()) {
                let present = transcript.contains(sub);
                expectations.push(ExpectResult {
                    kind,
                    subject: String::new(),
                    expected: sub.to_string(),
                    actual: Some(if present { "present" } else { "absent" }.to_string()),
                    passed: present == want_present,
                });
            }
        }

        // offered: { <choice id>: [options] } (dsl 0.22.0 §5) — the options
        // the walk actually offered at that branch/hub, as a set: every
        // presentation's eligible choices, unioned (a hub is offered once per
        // visit). `None` when the walk never presented it.
        if let Some(offered) = expect.get("offered").and_then(|v| v.as_mapping()) {
            for (k, v) in offered {
                let Some(id) = k.as_str() else { continue };
                let want: Option<BTreeSet<&str>> = v
                    .as_sequence()
                    .and_then(|s| s.iter().map(|i| i.as_str()).collect());
                let presented: Vec<&lute_trace::Decision> = report
                    .decisions
                    .iter()
                    .filter(|d| matches!(d.construct.as_str(), "branch" | "hub") && d.id == id)
                    .collect();
                let actual: Option<BTreeSet<&str>> = (!presented.is_empty()).then(|| {
                    presented
                        .iter()
                        .flat_map(|d| d.eligible.iter().map(String::as_str))
                        .collect()
                });
                let set_text = |s: &BTreeSet<&str>| {
                    format!("[{}]", s.iter().copied().collect::<Vec<_>>().join(", "))
                };
                expectations.push(ExpectResult {
                    kind: "offered",
                    subject: id.to_string(),
                    expected: match &want {
                        Some(w) => set_text(w),
                        None => "a list of choice ids".to_string(),
                    },
                    actual: actual.as_ref().map(set_text),
                    passed: want.is_some() && want == actual,
                });
            }
        }

        // state: { path: literal } — against the FINAL effective state (T2-5):
        // the last write, else the test's own seed, else the declared
        // `default:` — the same read order every guard in the walk used. A
        // path the walk never wrote used to report "never written" even when
        // its default (or the test's seed) was exactly the expected value.
        if let Some(state) = expect.get("state").and_then(|v| v.as_mapping()) {
            let final_state = &report.final_state;
            for (k, v) in state {
                let Some(path) = k.as_str() else { continue };
                let want = yaml_scalar_text(v).unwrap_or_default();
                let actual = final_state.get(path).cloned();
                expectations.push(ExpectResult {
                    kind: "state",
                    subject: path.to_string(),
                    expected: want.clone(),
                    // T9.9: the absent case stays absent all the way to the
                    // renderer. Substituting a display string here is what
                    // let a miss line print its two sides identically.
                    passed: actual.as_deref() == Some(want.as_str()),
                    actual,
                });
            }
        }

        // quests: { id: unset|active|complete|failed } — against the
        // lifecycle the trace ran (dsl 0.21.0 §7a.4).
        if let Some(quests) = expect.get("quests").and_then(|v| v.as_mapping()) {
            let final_quests = final_quests(&report, &input.text);
            for (k, v) in quests {
                let Some(id) = k.as_str() else { continue };
                let want = yaml_scalar_text(v).unwrap_or_default();
                // `None`: the traced document declares no such quest — there
                // is no lifecycle to observe (the T9.9 absent-value rule).
                let actual = final_quests.get(id).cloned();
                expectations.push(ExpectResult {
                    kind: "quests",
                    subject: id.to_string(),
                    passed: QUEST_STATES.contains(&want.as_str())
                        && actual.as_deref() == Some(want.as_str()),
                    expected: want,
                    actual,
                });
            }
        }

        // facts / notFacts: [atoms] — every fact that holds when the walk
        // ends, after derivation (0.23.1), judged as `lute play`'s
        // end-of-play expectation judges it. A fact of a relation whose
        // derivation read undecided state neither holds nor fails to hold.
        for (kind, want_held) in [("facts", true), ("notFacts", false)] {
            let Some(list) = expect.get(kind).and_then(|v| v.as_sequence()) else {
                continue;
            };
            for atom in list.iter().filter_map(yaml_scalar_text) {
                let Some((rel, _)) = crate::play_expect::parse_atom(&atom) else {
                    expectations.push(ExpectResult {
                        kind,
                        subject: atom.clone(),
                        expected: "a ground atom `rel(a, b)`".to_string(),
                        actual: Some("not a ground atom".to_string()),
                        passed: false,
                    });
                    continue;
                };
                let canonical = crate::play_expect::canonical_atom(&atom);
                let held = if report.final_facts.contains(&canonical) {
                    "holds"
                } else if report.final_undecided.contains(&rel) {
                    "unknown"
                } else {
                    "does not hold"
                };
                let want = if want_held { "holds" } else { "does not hold" };
                expectations.push(ExpectResult {
                    kind,
                    subject: atom.clone(),
                    expected: want.to_string(),
                    actual: Some(held.to_string()),
                    passed: held == want,
                });
            }
        }

        // eligible: bool | { <id>: bool } (0.23.1) — the `when` verdict of
        // the presented entry/beat. Trace presents it either way (the
        // engine's gate, shown, not enforced); this asserts the verdict.
        if let Some(want) = expect.get("eligible") {
            let presented = presented_eligibility(&report);
            let wants: Vec<(Option<String>, Option<bool>)> = match want {
                serde_yaml::Value::Mapping(m) => m
                    .iter()
                    .map(|(k, v)| (k.as_str().map(str::to_string), v.as_bool()))
                    .collect(),
                other => vec![(None, other.as_bool())],
            };
            for (id, want) in wants {
                let matched: Vec<&(String, Option<bool>)> = presented
                    .iter()
                    .filter(|(p, _)| {
                        id.as_deref()
                            .is_none_or(|id| p == id || p.ends_with(&format!(".{id}")))
                    })
                    .collect();
                let actual = (!matched.is_empty()).then(|| {
                    matched
                        .iter()
                        .map(|(_, e)| match e {
                            Some(true) => "true",
                            Some(false) => "false",
                            None => "unknown",
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                });
                let expected = match want {
                    Some(b) => b.to_string(),
                    None => "true or false".to_string(),
                };
                expectations.push(ExpectResult {
                    kind: "eligible",
                    subject: id.unwrap_or_default(),
                    passed: want.is_some()
                        && !matched.is_empty()
                        && matched.iter().all(|(_, e)| *e == want),
                    expected,
                    actual,
                });
            }
        }
    }

    // #2(d) / D-B: `all()` over an empty vector is `true`, so a test that
    // recognised no expectation reported PASS. A test that asserts nothing is
    // not a passing test.
    if expectations.is_empty() {
        let mut r = TestResult::refused(
            test_file,
            lute_display,
            exit_str,
            vec![format!(
                "error [E-TEST-NO-EXPECT] this test declares no recognised expectation \
                 (legal `expect:` keys: {}); a test that asserts nothing cannot pass",
                TEST_EXPECT_KEYS.join(", ")
            )],
        );
        r.autopicked = autopicked;
        return Ok(r);
    }

    // T1-13: an incomplete trace halted at an unknown guard, so everything
    // after it — including whatever the expectations above were written
    // about — was never walked. It passed whenever the test did not mention
    // `exit:`. Now it fails unless the test opts in with `exit: incomplete`.
    let declares_exit = expect.is_some_and(|e| e.contains_key("exit"));
    if exit_str == "incomplete" && !declares_exit {
        expectations.push(ExpectResult {
            kind: "exit",
            subject: IMPLICIT_EXIT.to_string(),
            expected: "complete".to_string(),
            actual: Some(exit_str.to_string()),
            passed: false,
        });
    }

    let passed = expectations.iter().all(|e| e.passed);

    Ok(TestResult {
        test_file: test_file.to_path_buf(),
        kind: "test",
        lute_file: lute_display,
        exit: exit_str.to_string(),
        passed,
        expectations,
        misses: Vec::new(),
        autopicked,
        refusal: None,
        unresolved: report.unresolved.clone(),
        forced_unknown: report.forced_unknown.clone(),
        notes: report
            .notes
            .iter()
            .filter(|n| n.starts_with(lute_trace::NOTE_BEAT_WHEN))
            .cloned()
            .chain(ineligible_notes(&report))
            .collect(),
    })
}

/// What `lute test` needs to know about a `*.play.yaml` before running it.
enum PlayScan {
    /// No `expect:` anywhere — not a test; `lute play` runs it, `lute test`
    /// does not.
    NoExpect,
    /// Carries an `expect:`; `declares_exit` when the top-level one names
    /// `exit:` (the opt-in for a halted play).
    Expect { declares_exit: bool },
    /// Unreadable or not YAML — reported, never skipped in silence.
    Broken(String),
}

/// Does this play carry any `expect:` — top-level or on any step?
fn scan_play(play_file: &Path) -> PlayScan {
    let text = match std::fs::read_to_string(play_file) {
        Ok(t) => t,
        Err(e) => return PlayScan::Broken(format!("cannot read the play: {e}")),
    };
    let top: serde_yaml::Value = match serde_yaml::from_str(&text) {
        Ok(v) => v,
        Err(e) => return PlayScan::Broken(format!("malformed play YAML: {e}")),
    };
    let top_expect = top.get("expect");
    let step_expect = top
        .get("steps")
        .and_then(|s| s.as_sequence())
        .is_some_and(|steps| steps.iter().any(|s| s.get("expect").is_some()));
    if top_expect.is_none() && !step_expect {
        return PlayScan::NoExpect;
    }
    PlayScan::Expect {
        declares_exit: top_expect.is_some_and(|e| e.get("exit").is_some()),
    }
}

/// Run one expect-carrying play (dsl 0.22.0 §4) through the SAME walk and
/// judge `lute play` uses. `None` for a play without `expect:`. The project
/// is `--project`, else the nearest `lute.project.yaml` above the play. A
/// play that could not run (usage error, no project) is a FAILURE naming
/// why; a play that halted fails unless its top-level `expect:` declares the
/// exit — the rule an incomplete trace follows (T1-13). When `cov` is `Some`,
/// every document the play presented counts as covered.
fn run_one_play(
    play_file: &Path,
    project: Option<&Path>,
    no_derive: bool,
    cov: Option<&mut CoverageAccum>,
) -> Option<TestResult> {
    let refused = |lute_file: String, line: String| {
        let mut r = TestResult::refused(play_file, lute_file, "invalid", vec![line]);
        r.kind = "play";
        Some(r)
    };
    let declares_exit = match scan_play(play_file) {
        PlayScan::NoExpect => return None,
        PlayScan::Expect { declares_exit } => declares_exit,
        PlayScan::Broken(why) => return refused(String::new(), format!("error: {why}")),
    };
    let Some(project_dir) = project
        .map(Path::to_path_buf)
        .or_else(|| crate::nearest_manifest_dir(play_file))
    else {
        return refused(
            String::new(),
            "error: no `lute.project.yaml` above this play — a play runs a project; pass \
             `--project <dir>`"
                .to_string(),
        );
    };
    let project_display = project_dir.display().to_string();
    let run = match crate::play::run_play_for_test(&project_dir, play_file, !no_derive) {
        Ok(run) => run,
        Err(why) => return refused(project_display, format!("error: {why}")),
    };
    if let Some(cov) = cov {
        cov.plays += 1;
        for doc in &run.presented_docs {
            cov.traced_files
                .insert(canonical_key(&project_dir.join(doc)));
        }
    }
    let mut misses = run.misses;
    if run.exit != "complete" && !declares_exit {
        misses.push(ExpectMiss {
            step: None,
            label: None,
            occasion: None,
            repetition: None,
            key: "exit".to_string(),
            expected: "complete (a halted play fails unless its top-level `expect:` declares \
                       the exit, e.g. `expect: { exit: incomplete }`)"
                .to_string(),
            actual: run.exit.to_string(),
        });
    }
    Some(TestResult {
        test_file: play_file.to_path_buf(),
        kind: "play",
        lute_file: project_display,
        exit: run.exit.to_string(),
        passed: misses.is_empty(),
        expectations: Vec::new(),
        misses,
        autopicked: Vec::new(),
        refusal: None,
        unresolved: Vec::new(),
        forced_unknown: Vec::new(),
        notes: run.notes,
    })
}

/// The `subject` of the `exit` expectation every test carries implicitly
/// when it declares none: an incomplete trace fails (T1-13).
const IMPLICIT_EXIT: &str = "implicit";

/// One unresolved atom's mock hint (`lute-trace` renders it as a `lute
/// trace` flag, `--state p=<value>` / `--fact "f"`) in the spelling a
/// `*.test.yaml` can actually use — the T9.11 rule `yaml_key_spelling`
/// applies to refusals, applied to the hint an author acts on next.
fn yaml_atom_hint(atom: &str) -> String {
    if let Some((path, value)) = atom
        .strip_prefix("--state ")
        .and_then(|rest| rest.split_once('='))
    {
        return format!("`state: {{ {path}: {value} }}`");
    }
    if let Some(fact) = atom.strip_prefix("--fact ") {
        return format!("`facts: [{fact}]`");
    }
    atom.to_string()
}

/// The hint list for one unresolved entry, `, `-joined.
fn yaml_atom_hints(u: &UnresolvedEntry) -> String {
    u.atoms
        .iter()
        .map(|a| yaml_atom_hint(a))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Every presented lore entry / bundle beat with its `when` verdict, in
/// presentation order: `Some(true)` eligible (or no `when`), `Some(false)`
/// not, `None` undecided under the mocks.
fn presented_eligibility(report: &TraceReport) -> Vec<(String, Option<bool>)> {
    report
        .steps
        .iter()
        .filter_map(|s| match s {
            lute_trace::Step::Entry { id, eligible, .. } | lute_trace::Step::Beat { id, eligible } => {
                Some((id.clone(), *eligible))
            }
            _ => None,
        })
        .collect()
}

/// A note for every presented entry / bundle beat whose `when` does not
/// hold under the test's mocks: trace presents it anyway (the engine's gate,
/// shown, not enforced), so a passing test says so instead of staying
/// silent (lamplight N6).
fn ineligible_notes(report: &TraceReport) -> Vec<String> {
    presented_eligibility(report)
        .into_iter()
        .filter_map(|(id, eligible)| {
            let why = match eligible {
                Some(false) => "its `when` is false",
                None => "its `when` is undecided",
                Some(true) => return None,
            };
            Some(format!(
                "`{id}` is not eligible under these mocks ({why}); the test presents it anyway \
                 — assert it with `expect: {{ eligible: false }}`"
            ))
        })
        .collect()
}

/// Every quest the traced document declares, mapped to where the walk left
/// it (dsl 0.21.0 §7a.4): the LAST `quest` decision's `active`/`complete`/
/// `failed` outcome, else `unset` — a quest whose `start` never held, that
/// awaited an accept, or that was never decided at all never left `unset`.
/// Read off the transcript's decisions, the same record the human report
/// prints, never a second lifecycle model.
fn final_quests(report: &TraceReport, text: &str) -> BTreeMap<String, String> {
    let (doc, _) = lute_syntax::parse(text);
    let mut out: BTreeMap<String, String> = doc
        .quests
        .iter()
        .filter(|q| !q.id.is_empty())
        .map(|q| (q.id.clone(), "unset".to_string()))
        .collect();
    for d in &report.decisions {
        if d.construct != "quest" {
            continue;
        }
        if let Some(slot) = out.get_mut(&d.id) {
            if matches!(d.outcome.as_str(), "active" | "complete" | "failed") {
                *slot = d.outcome.clone();
            }
        }
    }
    out
}

/// T9.11's second half. `lute-trace` composes its mock diagnostics for
/// `lute trace`'s command line (`--choose id=arm`, `--state path=value`,
/// `--fact`, `--event`, `--accept`, `--entry`), but in a `*.test.yaml` the same input
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
        ("--entry ", "entry: "),
        ("--beat ", "beat: "),
    ] {
        out = out.replace(flag, key);
    }
    out
}

/// Fold one report's decisions + coverage counts into the run accumulator.
fn accumulate_coverage(cov: &mut CoverageAccum, report: &TraceReport) {
    cov.paths += 1;
    cov.traced_files
        .insert(canonical_key(std::path::Path::new(&report.file)));
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
                let entry = cov
                    .arms
                    .entry(key)
                    .or_insert_with(|| (d.id.clone(), BTreeSet::new(), 0));
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
        let entry = cov
            .arms
            .entry(key)
            .or_insert_with(|| (c.label.clone(), BTreeSet::new(), 0));
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

/// Recursively collect every file under `dir` whose name ends in `suffix`
/// (`.test.yaml`, `.play.yaml`), byte-sorted for deterministic order —
/// mirrors [`crate::find_lute_files`]'s walk (stack, symlinked dirs not
/// followed).
fn find_files_with_suffix(dir: &Path, suffix: &str) -> std::io::Result<Vec<PathBuf>> {
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
                .is_some_and(|n| n.ends_with(suffix))
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
fn render_human(
    dir: &Path,
    results: &[TestResult],
    cov: Option<(&CoverageAccum, &Path)>,
    untested: &[String],
) -> String {
    let mut out = String::new();
    let out = &mut out;
    if results.is_empty() {
        outln!(
            out,
            "no *.test.yaml files (or *.play.yaml files carrying `expect:`) under {}",
            dir.display()
        );
    }
    for r in results {
        let mark = if r.passed { "PASS" } else { "FAIL" };
        if r.kind == "play" {
            outln!(out, "{mark}  {}  (play of {})", r.test_file.display(), r.lute_file);
        } else {
            outln!(out, "{mark}  {}  ({})", r.test_file.display(), r.lute_file);
        }
        if let Some(lines) = &r.refusal {
            // The vector carries either the trace's own held diagnostics
            // (#25) or the harness's own `E-TEST-*` refusals (#2). Only the
            // first is a *trace* refusal, so only it gets that header.
            if r.exit == "refused" {
                outln!(out, "      trace refused:");
            }
            for line in lines {
                outln!(out, "        {line}");
            }
            for a in &r.autopicked {
                outln!(out, "      auto-picked (no selection supplied): {a}");
            }
            continue;
        }
        if !r.passed {
            for e in r.expectations.iter().filter(|e| !e.passed) {
                render_miss(out, e);
            }
            for m in &r.misses {
                outln!(out, "      {m}");
            }
        }
        // T3-11: WHY the walk stopped (or left a guard undecided), with the
        // test keys that would decide it — the failure used to say only
        // `expected complete, got incomplete`.
        for u in &r.unresolved {
            outln!(
                out,
                "      unresolved: {} `{}` ({}) — supply {} in this test",
                u.construct,
                u.expression,
                u.id,
                yaml_atom_hints(u)
            );
        }
        for u in &r.forced_unknown {
            let hints = yaml_atom_hints(u);
            outln!(
                out,
                "      unresolved (forced): {} `{}` was chosen past a guard that was unknown \
                 (`{}`){}",
                u.construct,
                u.id,
                u.expression,
                if hints.is_empty() {
                    String::new()
                } else {
                    format!(" — supply {hints} to decide it")
                }
            );
        }
        for n in &r.notes {
            outln!(out, "      note: {n}");
        }
        for a in &r.autopicked {
            outln!(out, "      auto-picked (no selection supplied): {a}");
        }
    }

    let passed = results.iter().filter(|r| r.passed).count();
    let failed = results.len() - passed;
    outln!(out, "\n{passed} passed, {failed} failed");

    if let Some((cov, root)) = cov {
        render_coverage_human(out, cov, root, untested);
    }
    std::mem::take(out)
}

/// One failed expectation's miss line(s).
fn render_miss(out: &mut String, e: &ExpectResult) {
    match (e.kind, e.actual.as_deref()) {
        ("transcriptContains", Some(actual)) => outln!(
            out,
            "      transcriptContains {:?}: {actual} (expected present)",
            e.expected
        ),
        ("transcriptLacks", Some(actual)) => outln!(
            out,
            "      transcriptLacks {:?}: {actual} (expected absent)",
            e.expected
        ),
        ("offered", Some(actual)) => outln!(
            out,
            "      offered {}: expected {}, got {actual}",
            e.subject,
            e.expected
        ),
        ("offered", None) => outln!(
            out,
            "      offered {}: expected {}, but the walk never presented a branch/hub `{}`",
            e.subject,
            e.expected,
            e.subject
        ),
        ("state", Some(actual)) => outln!(
            out,
            "      state {}: expected {:?}, got {:?}",
            e.subject,
            e.expected,
            actual
        ),
        ("quests", _) if !QUEST_STATES.contains(&e.expected.as_str()) => outln!(
            out,
            "      quests {}: {:?} is not a quest state (expected one of: {})",
            e.subject,
            e.expected,
            QUEST_STATES.join(", ")
        ),
        ("quests", Some(actual)) => outln!(
            out,
            "      quests {}: expected {:?}, got {:?}",
            e.subject,
            e.expected,
            actual
        ),
        ("quests", None) => outln!(
            out,
            "      quests {}: expected {:?}, but the traced document declares no quest `{}`",
            e.subject,
            e.expected,
            e.subject
        ),
        // T9.9: there is no observed value to print. The old line printed
        // the sentinel on the `got` side, so a test whose expected literal
        // happened to BE the sentinel's text rendered `expected "<never
        // written>", got "<never written>"` — a difference whose two sides
        // were byte identical. Since T2-5 "no value" means never written,
        // not seeded, and no declared `default:`.
        ("state", None) => {
            outln!(
                out,
                "      state {}: expected {:?}, but the path was never written and has no seed \
                 or declared default",
                e.subject,
                e.expected
            );
            if e.expected == NEVER_WRITTEN {
                // Do not invent grammar here: `expect:`'s key set is closed
                // and the new expectation kinds are deferred with #19 (D-B).
                // Name the gap instead.
                outln!(
                    out,
                    "      note: {NEVER_WRITTEN:?} is how this report DISPLAYS an absent value, \
                     not a literal an expectation can match; `expect:` has no \"never written\" \
                     form (legal keys: {}) — deferred with #19",
                    TEST_EXPECT_KEYS.join(", ")
                );
            }
        }
        ("exit", Some(actual)) if e.subject == IMPLICIT_EXIT => outln!(
            out,
            "      exit: {actual} — an unknown guard halted the walk before the end, so the \
             expectations after it were never walked; an incomplete trace fails unless the \
             test declares `expect: {{ exit: incomplete }}`"
        ),
        ("exit", Some(actual)) => outln!(out, "      exit: expected {}, got {actual}", e.expected),
        ("facts" | "notFacts", Some(actual)) => outln!(
            out,
            "      {} {}: expected {}, got {actual}",
            e.kind,
            e.subject,
            e.expected
        ),
        ("eligible", Some(actual)) => outln!(
            out,
            "      eligible{}: expected {}, got {actual}",
            if e.subject.is_empty() { String::new() } else { format!(" {}", e.subject) },
            e.expected
        ),
        ("eligible", None) => outln!(
            out,
            "      eligible{}: expected {}, but the test presented no such entry or beat",
            if e.subject.is_empty() { String::new() } else { format!(" {}", e.subject) },
            e.expected
        ),
        _ => {}
    }
}

/// Human coverage view — honest header, chosen/never-chosen names where the
/// reports expose them, counts where they do not. Every row names its
/// construct's own file and site; the guard text rides along as a label
/// (#24, T9.13). `untested` is already filtered to TESTABLE documents by the
/// caller — components are not in it, and the strings below say so.
fn render_coverage_human(out: &mut String, cov: &CoverageAccum, root: &Path, untested: &[String]) {
    if cov.plays == 0 {
        outln!(out, "\ncoverage over {} traced path(s):", cov.paths);
    } else {
        outln!(
            out,
            "\ncoverage over {} traced path(s) and {} play(s):",
            cov.paths,
            cov.plays
        );
    }
    if cov.choices.is_empty() && cov.arms.is_empty() {
        outln!(out, "  (no branch/hub or match constructs traced)");
    }
    for (key, (label, chosen, eligible_seen, total)) in &cov.choices {
        let never_named: Vec<&String> = eligible_seen.difference(chosen).collect();
        let mut line = format!(
            "  branch/hub {label} ({key}): {}/{} chosen",
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
                never_named
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        // Choices never seen eligible in ANY traced path: count only, honest.
        let unseen = total.saturating_sub(chosen.len() + never_named.len());
        if unseen > 0 {
            line.push_str(&format!(
                "; {unseen} never seen eligible in any traced path"
            ));
        }
        outln!(out, "{line}");
    }
    for (key, (label, chosen, total)) in &cov.arms {
        let unexecuted = total.saturating_sub(chosen.len());
        let mut line = format!(
            "  match `{label}` ({key}): {}/{} arm(s) executed",
            chosen.len().min(*total),
            total
        );
        if !chosen.is_empty() {
            line.push_str(&format!(
                " [{}]",
                chosen.iter().cloned().collect::<Vec<_>>().join(", ")
            ));
        }
        if unexecuted > 0 {
            line.push_str(&format!("; {unexecuted} unexecuted"));
        }
        outln!(out, "{line}");
    }
    // T9.13's real design hole: coverage accumulated only from reports that
    // RAN, so deleting a test made its scene invisible rather than untested.
    // Both strings say "testable", because component documents are out of the
    // denominator and claiming otherwise is the false-reassurance this whole
    // task is about.
    if untested.is_empty() {
        outln!(
            out,
            "  every testable document under {} is named by at least one test or presented by \
             a play",
            root.display()
        );
    } else {
        outln!(
            out,
            "  {} untested document(s) under {} — no *.test.yaml names them and no play \
             presents them:",
            untested.len(),
            root.display()
        );
        for f in untested {
            outln!(out, "    {f}");
        }
    }
}

/// One unresolved entry as JSON (T3-11): where it is, what was undecided,
/// the raw atoms, and the test keys that would decide it.
fn unresolved_json(u: &UnresolvedEntry) -> serde_json::Value {
    serde_json::json!({
        "construct": u.construct,
        "id": u.id,
        "line": u.span.line,
        "column": u.span.column,
        "expression": u.expression,
        "atoms": u.atoms,
        "supply": u.atoms.iter().map(|a| yaml_atom_hint(a)).collect::<Vec<_>>(),
    })
}

/// Machine report: per-test verdicts + expectations, the summary, and
/// (optional) coverage — stable-keyed JSON.
fn render_json(
    results: &[TestResult],
    cov: Option<(&CoverageAccum, &Path)>,
    untested: &[String],
) -> String {
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
                "kind": r.kind,
                "file": r.lute_file,
                "exit": r.exit,
                "passed": r.passed,
                "refusal": r.refusal,
                "autopicked": r.autopicked.clone(),
                "expectations": expectations,
                "misses": r.misses.iter().map(ExpectMiss::to_json).collect::<Vec<_>>(),
                "unresolved": r.unresolved.iter().map(unresolved_json).collect::<Vec<_>>(),
                "forcedUnknown": r.forced_unknown.iter().map(unresolved_json).collect::<Vec<_>>(),
                "notes": r.notes,
            })
        })
        .collect();

    let passed = results.iter().filter(|r| r.passed).count();
    let failed = results.len() - passed;

    let mut root = json!({
        "tests": tests,
        "summary": { "passed": passed, "failed": failed },
    });

    if let Some((cov, cov_root)) = cov {
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
            "plays": cov.plays,
            "choices": Value::Object(choices),
            "arms": Value::Object(arms),
            "root": cov_root.display().to_string(),
            "untested": untested,
        });
    }

    format!(
        "{}\n",
        serde_json::to_string_pretty(&root).expect("report is JSON-serializable")
    )
}

#[cfg(test)]
mod tests {
    use super::{HARNESS_KEYS, TEST_TOP_KEYS};

    /// The two closed key sets differ by **exactly** [`HARNESS_KEYS`] — the
    /// claim `TEST_TOP_KEYS`' and `MOCK_TOP_KEYS`' doc comments both make. A
    /// new mock surface added on one side and not the other is the drift this
    /// catches: adding `seed:` to `MOCK_TOP_KEYS` alone would make a
    /// `*.test.yaml` reject a key its own mock parser reads, and adding it to
    /// `TEST_TOP_KEYS` alone would re-open the hole T3.10 filed.
    #[test]
    fn the_test_key_set_is_the_mock_key_set_plus_the_harness_keys() {
        let mut want: Vec<&str> = lute_trace::MOCK_TOP_KEYS.to_vec();
        want.extend_from_slice(HARNESS_KEYS);
        want.sort_unstable();
        assert_eq!(TEST_TOP_KEYS.to_vec(), want);
        for k in HARNESS_KEYS {
            assert!(!lute_trace::MOCK_TOP_KEYS.contains(k), "{k}");
        }
    }
}
