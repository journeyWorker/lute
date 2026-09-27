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
//!   options: { accuse: [accuseBlake, accuseMoss] } # the options offered there
//!   state: { run.accused: blake }   # the FINAL effective state (write → seed → default)
//!   quests: { caseClosed: complete } # unset | active | complete | failed
//!   end: complete                   # complete | terminal | incomplete | error
//! ```
//!
//! Each test traces its document once ([`trace_with_check`]) and checks every
//! declared expectation, naming actual-vs-expected on any miss. An
//! `incomplete` trace (an unknown guard halted the walk) FAILS unless the
//! test declares `expect: { end: incomplete }` — a walk that stopped halfway
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

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lute_trace::{
    parse_mock_surfaces, trace_beat_with_check, trace_entries_with_check, trace_with_check,
    TraceExit, TraceReport, UnresolvedEntry,
};
use rayon::prelude::*;

use crate::play_expect::ExpectMiss;

/// Every `eprintln!` of this module goes through [`stderr_line`]: `lute
/// test` runs its tests and plays in parallel (T2-1) and replays each one's
/// stderr lines in the order they are reported, so what a run prints is
/// what the sequential run printed. Every line is plain text
/// ([`lute_core_span::plain_message`]).
macro_rules! eprintln {
    ($($arg:tt)*) => {
        stderr_line(StderrLine::Text(
            lute_core_span::plain_message(&format!($($arg)*)).into_owned(),
        ))
    };
}

/// One stderr line of a test or play.
enum StderrLine {
    Text(String),
    /// The once-per-project-root note: printed for the first test, in
    /// report order, that reaches it.
    Note(PathBuf, String),
}

thread_local! {
    /// The capture buffers of the tests running on this thread, innermost
    /// last (a test waiting on nested parallel work may run another to
    /// completion on the same thread).
    static CAPTURE: RefCell<Vec<Vec<StderrLine>>> = const { RefCell::new(Vec::new()) };
}

/// Print `line` on stderr, or into the innermost capture when one is open.
fn stderr_line(line: StderrLine) {
    let line = CAPTURE.with(|c| match c.borrow_mut().last_mut() {
        Some(buffer) => {
            buffer.push(line);
            None
        }
        None => Some(line),
    });
    if let Some(StderrLine::Text(text) | StderrLine::Note(_, text)) = line {
        std::eprintln!("{text}");
    }
}

/// Run `f` with its stderr lines captured.
fn captured<R>(f: impl FnOnce() -> R) -> (R, Vec<StderrLine>) {
    CAPTURE.with(|c| c.borrow_mut().push(Vec::new()));
    let out = f();
    let lines = CAPTURE.with(|c| c.borrow_mut().pop()).unwrap_or_default();
    (out, lines)
}

/// Print captured lines; a note only for a root not `noted` yet.
fn replay(lines: Vec<StderrLine>, noted: &mut BTreeSet<PathBuf>) {
    for line in lines {
        match line {
            StderrLine::Text(text) => std::eprintln!("{text}"),
            StderrLine::Note(root, text) => {
                if noted.insert(root) {
                    std::eprintln!("{text}");
                }
            }
        }
    }
}

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
pub(crate) const TEST_TOP_KEYS: &[&str] = &[
    "accepts",
    "beat",
    "bridges",
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
/// `options:` / `transcriptLacks:` are dsl 0.22.0 §5; `facts:` / `notFacts:`
/// (every fact that holds at the end, after derivation) and `eligible:` (a
/// presented entry's or beat's `when`) are 0.23.1; `accepts:` (the quests a
/// scene's `::accept` took) is 0.24.0.
pub(crate) const TEST_EXPECT_KEYS: &[&str] = &[
    "accepts",
    "eligible",
    "end",
    "facts",
    "notFacts",
    "options",
    "quests",
    "state",
    "transcriptContains",
    "transcriptLacks",
];

/// The quest lifecycle states an `expect.quests` entry may name (dsl 0.21.0
/// §7a.4) — `unset` is the pre-activation absence, the other three the
/// engine's `quest.<id>.state` domain.
const QUEST_STATES: &[&str] = &["unset", "active", "complete", "failed"];

/// The spellings a `*.test.yaml` no longer reads, each with the error that
/// names its replacement: `(level, old, message)`.
const RENAMED_TEST_KEYS: &[(&str, &str, &str)] = &[
    (
        "top-level",
        "accept",
        "`accept:` is now `accepts:` in a `*.test.yaml`",
    ),
    (
        "`expect:`",
        "offered",
        "`expect.offered` is now `expect.options` — in a test and in a play, `options` are \
         the menu choices a branch/hub presents (a play's `offered` lists beat candidates)",
    ),
    (
        "`expect:`",
        "exit",
        "`expect.exit` is now `expect.end` — write `end: <how the walk ended>`, one of: \
         complete, terminal, incomplete, error",
    ),
];

/// One `E-TEST-KEY` line for an unrecognised key, with the same
/// edit-distance did-you-mean four checker codes already use (dsl 0.5.0
/// §2.2), over the workspace's ONE suggestion helper. `where_` names the
/// level so a top-level typo and an `expect:`-level typo are
/// distinguishable. A renamed key names its new spelling; a key a play
/// script reads at that level says so.
fn unknown_key_line(where_: &str, key: &str, allowed: &[&str]) -> String {
    if let Some((_, _, msg)) = RENAMED_TEST_KEYS
        .iter()
        .find(|(level, old, _)| *level == where_ && *old == key)
    {
        return format!("error [E-TEST-KEY] {msg}");
    }
    let sugg = lute_manifest::suggest::did_you_mean(key, allowed.iter().copied());
    let play = if where_ == "top-level" {
        if crate::play::STEP_KEYS.contains(&key) {
            Some("a play step")
        } else {
            crate::play::SCRIPT_KEYS
                .contains(&key)
                .then_some("a play script's top level")
        }
    } else {
        crate::play_expect::STEP_EXPECT_KEYS
            .contains(&key)
            .then_some("a play step's `expect:`")
    };
    let hint = play
        .map(|p| format!(" (`{key}:` belongs to {p}, in a `*.play.yaml`)"))
        .unwrap_or_default();
    format!(
        "error [E-TEST-KEY] unknown {where_} key `{key}` in a `*.test.yaml`{sugg}{hint} (legal: {})",
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
                for (ek, ev) in em {
                    match ek.as_str() {
                        // How the walk ended: one of play's `expect.end`
                        // values — a misspelt one could never hold.
                        Some("end") => {
                            let ends = crate::play_expect::ENDS;
                            match ev.as_str() {
                                Some(e) if ends.contains(&e) => {}
                                got => out.push(format!(
                                    "error [E-TEST-KEY] `expect.end: {}` names no way a walk \
                                     ends{} (one of: {})",
                                    got.unwrap_or("?"),
                                    got.map(|g| lute_manifest::suggest::did_you_mean(
                                        g,
                                        ends.iter().copied()
                                    ))
                                    .unwrap_or_default(),
                                    ends.join(", ")
                                )),
                            }
                        }
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
    /// Prerelease N3: for the implicit `eligible` expectation, the premise
    /// that makes the presentation ineligible (`its \`after: …\` is false
    /// — mock …`). Human report only.
    why: Option<String>,
    /// dsl 0.27.0 §4 (HW27-04): for an `eligible` miss, why the engine would
    /// not raise the beat's occasion under the mocks — `--json`'s
    /// `notRaised`, only when that is the cause.
    not_raised: Option<lute_trace::NotRaised>,
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
    /// How the walk ended — `complete | terminal | incomplete | error`, the
    /// values `expect.end` names. `None` when nothing was walked.
    end: Option<String>,
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
            end: None,
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

/// Coverage accumulated across every traced path and play in the run, keyed
/// by the construct's whole-project identity — `"{file}:{id}"` for a
/// branch/hub, `"{file}:{line}:{column}"` for a match (#24, T9.13). Before
/// 0.10.0 the key was the guard TEXT, so six `<match on="true">` blocks
/// across four files rendered as one row reading `3/3`. Nothing here is
/// presented as whole-space coverage — only "what these N paths and plays
/// touched" (D1: trace explains, it never proves).
#[derive(Default)]
struct CoverageAccum {
    /// Canonical `"{file}:{id}"` -> the branch/hub row. The canonical file
    /// ([`canonical_key`]) is what lets a play's pick and a traced path's
    /// land on one row (T3-20); the row prints the first spelling seen.
    choices: BTreeMap<String, ChoiceRow>,
    /// Printed site -> the `<match>` / `when=` guard row.
    arms: BTreeMap<String, ArmRow>,
    /// Number of documents that produced a report (a non-refused trace).
    paths: usize,
    /// Number of expect-carrying plays that ran (dsl 0.22.0 §4); every
    /// document they presented is in `traced_files`.
    plays: usize,
    /// Canonicalised path of every `.lute` that produced a report or that a
    /// play presented — what covers a scene or quest document, whose whole
    /// document is its coverage unit (#24's second half).
    traced_files: BTreeSet<String>,
    /// T3-20: `(canonical file, id)` of every bundle beat / lore entry a
    /// test presented (`beat:` / `entry:` / `entries:`) or a play presented
    /// — a lore document's units are its beats and entries, not the file.
    units: BTreeSet<(String, String)>,
    /// T3-20: canonical files a play presented a beat or ran a quest of.
    play_files: BTreeSet<String>,
    /// T3-20: the `(canonical file, id)` units a play presented.
    play_units: BTreeSet<(String, String)>,
}

/// One branch/hub row of [`CoverageAccum::choices`].
struct ChoiceRow {
    /// `"{file}:{id}"` as first seen (a traced path's spelling when a test
    /// traced it — tests fold in before plays).
    site: String,
    /// The branch/hub id.
    label: String,
    chosen: BTreeSet<String>,
    /// Choice ids seen eligible (offered) where the construct ran.
    eligible: BTreeSet<String>,
    total: usize,
}

/// One `<match>` or `when=` guard row of [`CoverageAccum::arms`].
struct ArmRow {
    /// The subject (a match) or the guard's text, as authored.
    label: String,
    /// Arm outcomes (`arm 1`, `otherwise`), or `taken` / `skipped`.
    chosen: BTreeSet<String>,
    total: usize,
    /// A `when=` guard, not an authored `<match>` (T3-22).
    guard: bool,
}

impl ArmRow {
    fn new(label: String, guard: bool) -> Self {
        ArmRow {
            label,
            chosen: BTreeSet::new(),
            total: 0,
            guard,
        }
    }
}

impl CoverageAccum {
    /// Fold a later accumulation in — what accumulating its reports after
    /// this one's would have produced (a label stays the first one seen).
    fn merge(&mut self, later: CoverageAccum) {
        for (key, row) in later.choices {
            match self.choices.get_mut(&key) {
                Some(entry) => {
                    entry.chosen.extend(row.chosen);
                    entry.eligible.extend(row.eligible);
                    entry.total = entry.total.max(row.total);
                }
                None => {
                    self.choices.insert(key, row);
                }
            }
        }
        for (key, row) in later.arms {
            match self.arms.get_mut(&key) {
                Some(entry) => {
                    entry.chosen.extend(row.chosen);
                    entry.total = entry.total.max(row.total);
                }
                None => {
                    self.arms.insert(key, row);
                }
            }
        }
        self.paths += later.paths;
        self.plays += later.plays;
        self.traced_files.extend(later.traced_files);
        self.units.extend(later.units);
        self.play_files.extend(later.play_files);
        self.play_units.extend(later.play_units);
    }

    /// The branch/hub row at `file`'s `id`, created with `site` as its
    /// printed spelling.
    fn choice_row(&mut self, canonical_file: &str, file: &str, id: &str) -> &mut ChoiceRow {
        self.choices
            .entry(format!("{canonical_file}:{id}"))
            .or_insert_with(|| ChoiceRow {
                site: format!("{file}:{id}"),
                label: id.to_string(),
                chosen: BTreeSet::new(),
                eligible: BTreeSet::new(),
                total: 0,
            })
    }
}

/// T3-20: one coverage unit of the project — a scene or quest document as a
/// whole, or one bundle beat / lore entry of a lore document, named by the
/// id the project index gives it.
struct CoverageUnit {
    /// The walk's display path of its document.
    file: String,
    canonical: String,
    /// `None`: the whole document is the unit.
    id: Option<String>,
    /// The id as its document writes it (a bundle beat's own `id`).
    local: String,
    /// The id of the document the unit is in (its frontmatter `id`, or a
    /// scene's canonical key), when it has one.
    doc: Option<String>,
    /// `scene` / `quest` / `document` / `beat` / `entry`.
    kind: &'static str,
    /// It answers an occasion, so a play can present it.
    beat: bool,
}

impl CoverageUnit {
    fn covered(&self, cov: &CoverageAccum) -> bool {
        match &self.id {
            None => cov.traced_files.contains(&self.canonical),
            Some(id) => cov.units.contains(&(self.canonical.clone(), id.clone())),
        }
    }

    fn presented_by_play(&self, cov: &CoverageAccum) -> bool {
        match &self.id {
            None => cov.play_files.contains(&self.canonical),
            Some(id) => cov
                .play_units
                .contains(&(self.canonical.clone(), id.clone())),
        }
    }
}

/// The coverage units of the testable documents under `root` (T3-20):
/// components are out (untestable, see [`run_test`]); a lore document's
/// units are its bundle beats (`<document id>.<beat id>`) and entries; any
/// other document is one unit.
fn coverage_units(root: &Path) -> std::io::Result<Vec<CoverageUnit>> {
    let mut out = Vec::new();
    let paths: Vec<PathBuf> = crate::find_lute_files(root)?
        .into_iter()
        .filter(|p| !crate::compile_all::is_component_file(p))
        .collect();
    // Desugared as `check` sees them: template- and sequence-derived beats
    // are units too (dsl 0.27.0 §6).
    let docs = crate::parse_project_docs(root, &paths);
    for (path, parsed) in paths.iter().zip(docs) {
        let file = display_path(path);
        let canonical = canonical_key(path);
        let Ok((doc, _)) = parsed else {
            continue;
        };
        if doc.entries.is_empty() && doc.beats.is_empty() {
            let meta = serde_yaml::from_str::<serde_yaml::Mapping>(&doc.meta.raw_yaml).ok();
            let scene = lute_check::connectivity::scene_key(&doc);
            let doc_id = scene.clone().or_else(|| {
                meta.as_ref()
                    .and_then(|m| m.get("id"))
                    .and_then(serde_yaml::Value::as_str)
                    .map(str::to_string)
            });
            out.push(CoverageUnit {
                file,
                canonical,
                id: None,
                local: scene.clone().unwrap_or_default(),
                doc: doc_id,
                kind: if !doc.quests.is_empty() {
                    "quest"
                } else if scene.is_some() {
                    "scene"
                } else {
                    "document"
                },
                beat: meta.is_some_and(|m| m.contains_key("on")),
            });
            continue;
        }
        // Entries and bundle beats in source order, as the index lists them.
        let bundle = lute_check::connectivity::bundle_id(&doc);
        let mut units: Vec<(usize, CoverageUnit)> = doc
            .entries
            .iter()
            .map(|e| {
                (
                    e.span.byte_start,
                    CoverageUnit {
                        file: file.clone(),
                        canonical: canonical.clone(),
                        id: Some(e.id.clone()),
                        local: e.id.clone(),
                        doc: bundle.clone(),
                        kind: "entry",
                        beat: e.on.is_some(),
                    },
                )
            })
            .chain(doc.beats.iter().map(|b| {
                let id = match &bundle {
                    Some(d) => lute_check::bundle_beat_key(d, &b.id),
                    None => b.id.clone(),
                };
                (
                    b.span.byte_start,
                    CoverageUnit {
                        file: file.clone(),
                        canonical: canonical.clone(),
                        id: Some(id),
                        local: b.id.clone(),
                        doc: bundle.clone(),
                        kind: "beat",
                        beat: b.on.is_some(),
                    },
                )
            }))
            .collect();
        units.sort_by_key(|(at, _)| *at);
        out.extend(units.into_iter().map(|(_, u)| u));
    }
    Ok(out)
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

/// A path as the report prints it (T3-23): relative to the directory
/// `lute test` runs in — `scenes/a.lute`, `.` for that directory itself —
/// whatever spelling reached it (a test's `file: ../scenes/a.lute`, a
/// play's project root, the nearest manifest's absolute directory); a path
/// outside that directory keeps its `..`-folded spelling.
fn display_path(p: &Path) -> String {
    static CWD: std::sync::LazyLock<Option<PathBuf>> =
        std::sync::LazyLock::new(|| std::env::current_dir().and_then(std::fs::canonicalize).ok());
    let rel = CWD.as_deref().and_then(|cwd| {
        let abs = std::fs::canonicalize(p).ok()?;
        abs.strip_prefix(cwd).ok().map(Path::to_path_buf)
    });
    match rel {
        Some(r) if r.as_os_str().is_empty() => ".".to_string(),
        Some(r) => r.display().to_string(),
        None => fold_parent_dirs(p).display().to_string(),
    }
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

    // T2-1: the project is loaded and analysed once for the whole run —
    // every test's project producer set / quest ids before the tests, the
    // plays' compiled project before the plays — and the tests, then the
    // plays, run in parallel (`RAYON_NUM_THREADS` respected). Each one's
    // stderr is captured and replayed, and its result folded, in the order
    // the sequential run reported them; the first usage/I-O failure still
    // stops the run with nothing after it printed.
    let mut results = Vec::new();
    let mut cov = CoverageAccum::default();
    let mut noted: BTreeSet<PathBuf> = BTreeSet::new();
    let mut shared = Shared::for_tests(&test_files, project, providers);

    // Round-5 G-17: a schema or plugin fault every importing document
    // shares is reported once, at its own line, as `check-project` folds it
    // — then the run is refused, instead of one failed test (and one
    // `<doc>:1:1` copy) per document that imports it.
    let play_roots: BTreeSet<PathBuf> = play_files
        .iter()
        .filter_map(|p| project_dir_of(p, project))
        .collect();
    for root in play_roots {
        if !shared.gates.contains_key(&root) {
            let rec = crate::reconciled_project_results(&root, providers).ok();
            shared.gates.insert(root, rec);
        }
    }
    let faults: Vec<String> = shared
        .gates
        .iter()
        .filter_map(|(root, rec)| Some(rec.as_ref()?.schema_faults(root)))
        .flatten()
        .collect();
    if !faults.is_empty() {
        for line in &faults {
            println!("{line}");
        }
        eprintln!(
            "lute test: {} schema or plugin error(s) every importing document shares; \
             refusing to run the tests",
            faults.len()
        );
        return ExitCode::from(1);
    }

    let runs: Vec<_> = test_files
        .par_iter()
        .map(|test_file| {
            let mut local = CoverageAccum::default();
            let (r, lines) = captured(|| {
                run_one_test(
                    test_file,
                    providers,
                    project,
                    no_derive,
                    &shared,
                    coverage.then_some(&mut local),
                )
            });
            (r, local, lines)
        })
        .collect();
    for (r, local, lines) in runs {
        replay(lines, &mut noted);
        match r {
            Ok(r) => {
                results.push(r);
                cov.merge(local);
            }
            // A malformed test yaml or an unreadable referenced document is a
            // usage/I-O failure (exit 2) — never a silent skip that would let
            // a broken suite report "all passed".
            Err(code) => return code,
        }
    }

    shared.compile_plays(&play_files, project);
    let runs: Vec<_> = play_files
        .par_iter()
        .map(|play_file| {
            let mut local = CoverageAccum::default();
            let (r, lines) = captured(|| {
                run_one_play(
                    play_file,
                    project,
                    no_derive,
                    &shared,
                    coverage.then_some(&mut local),
                )
            });
            (r, local, lines)
        })
        .collect();
    for (r, local, lines) in runs {
        replay(lines, &mut noted);
        if let Some(r) = r {
            results.push(r);
            cov.merge(local);
        }
    }

    let passed = results.iter().filter(|r| r.passed).count();
    let failed = results.len() - passed;

    // #24's denominator, at beat granularity since T3-20: every coverage
    // unit ([`coverage_units`]) under the PROJECT root that no test
    // presented and no play presented, MINUS the component documents. The
    // root is `--project`, else the nearest `lute.project.yaml` above `dir`,
    // else `dir` itself — the
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
    let units: Vec<CoverageUnit> = if coverage {
        match coverage_units(&coverage_root) {
            Ok(units) => units,
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
        render_json(
            &results,
            coverage.then_some((&cov, coverage_root.as_path())),
            &units,
        )
    } else {
        render_human(
            dir,
            &results,
            coverage.then_some((&cov, coverage_root.as_path())),
            &units,
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

/// The project state every test and play of one run shares (T2-1),
/// computed before they run so the parallel tests and plays only read it.
#[derive(Default)]
struct Shared {
    /// Per-run memo of the shared document inputs every traced document
    /// resolves.
    inputs: crate::InputCache,
    /// The project May producer set per project root (T1-14), for every
    /// root a test mocking `facts:` resolves against — it costs a full
    /// project collection.
    producers: BTreeMap<PathBuf, Option<BTreeSet<String>>>,
    /// Every quest id of the project per root (dsl 0.26.0 §7, T3-5), for
    /// every root a test with `accepts:` resolves against.
    quests: BTreeMap<PathBuf, Option<BTreeSet<String>>>,
    /// The compiled project per play project directory.
    plays: BTreeMap<PathBuf, crate::play::PlayProject>,
    /// The reconciled project analysis per project root a test resolves
    /// its document against (`None` when it failed): every test gates its
    /// document on its project's verdict, as `lute trace --project` and
    /// `lute play` do — one envelope (round-5 `test-project-envelope`).
    gates: BTreeMap<PathBuf, Option<crate::ReconciledProject>>,
}

/// The root whose producer set judges `lute_path`'s mocked facts:
/// `--project` when given (every file resolves against it, as the trace
/// gate does), else the nearest `lute.project.yaml` — with whether it is a
/// single root ([`crate::project_assert_relations`]).
fn producer_root(lute_path: &Path, project: Option<&Path>) -> Option<(PathBuf, bool)> {
    match project {
        Some(p) => Some((p.to_path_buf(), true)),
        None => crate::nearest_manifest_dir(lute_path).map(|root| (root, false)),
    }
}

/// The project `file` belongs to: `--project`, else the nearest
/// `lute.project.yaml` above it.
fn project_dir_of(file: &Path, project: Option<&Path>) -> Option<PathBuf> {
    project
        .map(Path::to_path_buf)
        .or_else(|| crate::nearest_manifest_dir(file))
}

impl Shared {
    /// Collect, once per root, what `test_files` read of their projects.
    fn for_tests(test_files: &[PathBuf], project: Option<&Path>, providers: Option<&Path>) -> Self {
        let mut producer_roots: BTreeSet<(PathBuf, bool)> = BTreeSet::new();
        let mut quest_roots: BTreeSet<PathBuf> = BTreeSet::new();
        let mut gate_roots: BTreeSet<PathBuf> = BTreeSet::new();
        for test_file in test_files {
            let Ok(text) = std::fs::read_to_string(test_file) else {
                continue;
            };
            let (Ok(mocks), Ok(Some(rel))) =
                (parse_mock_surfaces(&text), lute_trace::mock_subject(&text))
            else {
                continue;
            };
            let lute_path = test_file
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(&rel);
            if !mocks.facts.is_empty() {
                producer_roots.extend(producer_root(&lute_path, project));
            }
            if !mocks.accepts.is_empty() {
                quest_roots.extend(project_dir_of(&lute_path, project));
            }
            gate_roots.extend(project_dir_of(&lute_path, project));
        }
        let mut shared = Shared::default();
        for (root, single_root) in producer_roots {
            let set = crate::project_assert_relations(&root, single_root, providers);
            shared.producers.insert(root, set);
        }
        for root in quest_roots {
            let ids = crate::project_quest_ids(&root, providers);
            shared.quests.insert(root, ids);
        }
        for root in gate_roots {
            let rec = crate::reconciled_project_results(&root, providers).ok();
            shared.gates.insert(root, rec);
        }
        shared
    }

    /// Compile, once per project, what the expect-carrying `play_files`
    /// run over.
    fn compile_plays(&mut self, play_files: &[PathBuf], project: Option<&Path>) {
        for play_file in play_files {
            if !matches!(scan_play(play_file), PlayScan::Expect { .. }) {
                continue;
            }
            if let Some(dir) = project_dir_of(play_file, project) {
                if !self.plays.contains_key(&dir) {
                    let compiled = crate::play::PlayProject::compile(&dir);
                    self.plays.insert(dir, compiled);
                }
            }
        }
    }

    /// The producer set of the project `lute_path` belongs to
    /// ([`producer_root`]). `None` when there is no project to consult or it
    /// could not be collected — the trace then judges the document alone
    /// and its note says so.
    fn for_document(
        &self,
        lute_path: &Path,
        project: Option<&Path>,
        providers: Option<&Path>,
    ) -> Option<BTreeSet<String>> {
        let (root, single_root) = producer_root(lute_path, project)?;
        match self.producers.get(&root) {
            Some(set) => set.clone(),
            None => crate::project_assert_relations(&root, single_root, providers),
        }
    }

    /// Every quest id of the project at `root`.
    fn project_quests(&self, root: &Path, providers: Option<&Path>) -> Option<BTreeSet<String>> {
        match self.quests.get(root) {
            Some(ids) => ids.clone(),
            None => crate::project_quest_ids(root, providers),
        }
    }

    /// `lute_path`'s gate verdict in the project at `root` (the spec §5
    /// gate `lute trace --project` applies); `None` when the project could
    /// not be analysed or does not hold the document — the standalone
    /// check then decides, as `lute trace` without a project does.
    fn gate(&self, root: &Path, lute_path: &Path) -> Option<lute_check::CheckResult> {
        self.gates.get(root)?.as_ref()?.gate(lute_path)
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
    shared: &Shared,
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
            eprintln!("lute: {}: [{}] {}", test_file.display(), d.code, d.text());
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
    // 0.27 prerelease OT-F-3: every string `expect.state` value and where it
    // was written — member-checked once the document's schema is folded.
    let expect_state: Vec<(String, String, String)> = map
        .get("expect")
        .and_then(|e| e.get("state"))
        .and_then(|v| v.as_mapping())
        .into_iter()
        .flatten()
        .filter_map(|(k, v)| {
            use lute_trace::YamlStep::Key;
            let (path, want) = (k.as_str()?, v.as_str()?);
            let at = lute_trace::yaml_span(&text, &[Key("expect"), Key("state"), Key(path)])
                .map_or_else(
                    || test_file.display().to_string(),
                    |s| format!("{}:{}:{}", test_file.display(), s.line, s.column),
                );
            Some((path.to_string(), want.to_string(), at))
        })
        .collect();
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
            eprintln!("lute: {}: [{}] {}", test_file.display(), d.code, d.text());
            return Err(ExitCode::from(2));
        }
    };
    // dsl 0.22.0 §5: `entry: <id>` / `entries: [ids]` — the lore entries to
    // present, in order. 0.23.1: `beat: <id>` — one bundle `<beat>` (local
    // or canonical `<document id>.<beat id>`), as `lute trace --beat`.
    let entries: Vec<String> = match (map.get("entry"), map.get("entries")) {
        (None, None) => Vec::new(),
        (Some(serde_yaml::Value::String(id)), None) => vec![id.clone()],
        (None, Some(serde_yaml::Value::Sequence(ids))) if ids.iter().all(|i| i.is_string()) => ids
            .iter()
            .filter_map(|i| i.as_str().map(str::to_string))
            .collect(),
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
    // Shown folded (`tests/spine/../../lore/a.lute` → `lore/a.lute`), so a
    // report names one document one way wherever the test sits (ML-F9).
    let lute_display = fold_parent_dirs(&lute_path).display().to_string();

    // One test naming a document that no longer exists is that test's
    // failure, never the whole suite's abort.
    /// A test whose `file:` names no document (0.23.1).
    const E_TEST_FILE: &str = "E-TEST-FILE";
    /// A transcript needle naming what no presented line can carry (0.27).
    const E_TEST_NEEDLE: &str = "E-TEST-NEEDLE";
    if !lute_path.is_file() {
        // `file:` is relative to the test file: name the `../` spelling
        // when the path resolves from a directory above it.
        let hint = base
            .ancestors()
            .skip(1)
            .take(4)
            .enumerate()
            .find(|(_, dir)| dir.join(&rel).is_file())
            .map(|(up, _)| {
                format!(
                    " — `file:` is relative to the test file; did you mean `{}{rel}`?",
                    "../".repeat(up + 1)
                )
            })
            .unwrap_or_default();
        return Ok(TestResult::refused(
            test_file,
            lute_display.clone(),
            "invalid",
            vec![format!(
                "error [{E_TEST_FILE}] `file: {rel}` names no document ({lute_display} does not \
                 exist){hint}"
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
        let shown = crate::cwd_relative(&root.display().to_string());
        stderr_line(StderrLine::Note(
            root.clone(),
            format!(
                "lute: note: scenario tests use project {} (nearest lute.project.yaml); pass \
                 --project to choose another",
                if shown.is_empty() {
                    "."
                } else {
                    shown.as_str()
                }
            ),
        ));
    }
    let resolve_with = project.or(discovered.as_deref());

    let doc_text = match crate::read_document(&lute_path) {
        Ok(doc_text) => doc_text,
        Err(message) => {
            eprintln!("{message}");
            return Err(ExitCode::from(2));
        }
    };
    let (built, _) = crate::assemble_input(
        &shared.inputs,
        &lute_path,
        doc_text,
        providers,
        resolve_with,
        None,
    );
    for m in &built.project_diags {
        eprintln!("lute: {m}");
    }
    let crate::BuiltInput {
        input,
        resolve_error,
        meta,
        ..
    } = built;
    // plugin 0.0.2 §2: an `E-` capability-resolution diagnostic (bad plugin
    // option, missing active plugin, bad identity template) is a build-failing
    // error; it printed above, and it MUST gate here or it would pass silently.
    if resolve_error {
        return Err(ExitCode::from(1));
    }

    // 0.27 prerelease OT-F-2 / OT N-2: a needle speaker or attribute no
    // presented line can carry makes `transcriptContains` a sure miss and
    // `transcriptLacks` a vacuous pass — refused like a misspelt key, before
    // anything is walked, located at the needle.
    let item_at = |key: &str, i: usize| {
        use lute_trace::YamlStep::{Item, Key};
        lute_trace::yaml_span(&text, &[Key("expect"), Key(key), Item(i)]).map_or_else(
            || test_file.display().to_string(),
            |s| format!("{}:{}:{}", test_file.display(), s.line, s.column),
        )
    };
    let expect_items = |key: &'static str| {
        map.get("expect")
            .and_then(|e| e.get(key))
            .and_then(|v| v.as_sequence())
            .into_iter()
            .flatten()
            .enumerate()
            .filter_map(move |(i, v)| Some((key, i, v.as_str()?)))
    };
    let needles = lute_trace::exec::record::NeedleVocab::of(&input, &meta);
    let needle_problems: Vec<String> = expect_items("transcriptContains")
        .chain(expect_items("transcriptLacks"))
        .filter_map(|(key, i, n)| {
            let why = lute_trace::exec::record::needle_problem(n, &needles)?;
            Some(format!(
                "{}: error [{E_TEST_NEEDLE}] `expect.{key}` {why}",
                item_at(key, i)
            ))
        })
        .collect();
    if !needle_problems.is_empty() {
        return Ok(TestResult::refused(
            test_file,
            lute_display,
            "invalid",
            needle_problems,
        ));
    }

    // 0.27 prerelease OT N-2: an `expect.facts` / `expect.notFacts` atom
    // naming an undeclared relation, the wrong arity or a foreign argument
    // names no fact that can hold — a `notFacts` one would pass vacuously.
    // Refused as a seed `facts:` entry is ([`lute_check::check_atom`], a
    // derived relation included), located at the atom.
    let expect_atoms: Vec<(&str, usize, &str)> = expect_items("facts")
        .chain(expect_items("notFacts"))
        .collect();
    if !expect_atoms.is_empty() {
        let doc = desugared(&input);
        let (folded, _, _) = lute_check::fold_env(&doc, &input);
        let problems: Vec<String> = expect_atoms
            .iter()
            .flat_map(|&(key, i, atom)| {
                let whys: Vec<String> = match lute_syntax::datalog::parse_fact(
                    &crate::play_expect::canonical_atom(atom),
                ) {
                    Err(_) => Vec::new(),
                    Ok(pattern) => lute_check::check_atom(
                        &folded.env.rel_vocab,
                        &folded.env.domains,
                        &pattern.relation,
                        &pattern.args,
                        false,
                        lute_core_span::Span {
                            byte_start: 0,
                            byte_end: 0,
                            line: 0,
                            column: 0,
                            utf16_range: (0, 0),
                        },
                    )
                    .into_iter()
                    .map(|d| d.message)
                    .collect(),
                };
                whys.into_iter().map(move |why| {
                    format!(
                        "{}: error [{}] `expect.{key}` entry `{atom}` names no fact that can \
                         hold: {why}",
                        item_at(key, i),
                        lute_trace::E_TRACE_MOCK_FACT
                    )
                })
            })
            .collect();
        if !problems.is_empty() {
            return Ok(TestResult::refused(
                test_file,
                lute_display,
                "invalid",
                problems,
            ));
        }
    }

    // 0.27 prerelease OT-F-3: an `expect.state` value outside its path's
    // closed domain (`{ domain: K }` / `{ entity: K }`, an inline enum) can
    // never hold — refused like a seed outside it, with the members and the
    // nearest one, not reported as a miss. `unset` is the spelling of "no
    // value" (an implicit choice slot, a quest), never a typo.
    if !expect_state.is_empty() {
        let doc = desugared(&input);
        let (folded, _, _) = lute_check::fold_env(&doc, &input);
        let problems: Vec<String> = expect_state
            .iter()
            .filter(|(_, want, _)| want != "unset")
            .filter_map(|(path, want, at)| {
                let why = lute_trace::mock::state_member_problem(&folded.env.state, path, want)?;
                Some(format!(
                    "{at}: error [E-TRACE-MOCK-TYPE] `expect.state.{path}: {want}` can never \
                     hold: {why}"
                ))
            })
            .collect();
        if !problems.is_empty() {
            return Ok(TestResult::refused(
                test_file,
                lute_display,
                "invalid",
                problems,
            ));
        }
    }

    // `expect.eligible`, a map key naming an entry by its `<document
    // id>.<entry id>` alias resolved to the entry id (dsl 0.26.0 §7, T3-10).
    let eligible_want: Option<serde_yaml::Value> = map
        .get("expect")
        .and_then(|e| e.get("eligible"))
        .map(|want| match want {
            serde_yaml::Value::Mapping(m) => {
                let doc = desugared(&input);
                let doc_id = serde_yaml::from_str::<serde_yaml::Value>(&doc.meta.raw_yaml)
                    .ok()
                    .and_then(|v| v.get("id")?.as_str().map(str::to_string));
                serde_yaml::Value::Mapping(
                    m.iter()
                        .map(|(k, v)| {
                            let k = match k.as_str() {
                                Some(id) => serde_yaml::Value::String(
                                    lute_trace::entry_local_id(&doc, doc_id.as_deref(), id)
                                        .to_string(),
                                ),
                                None => k.clone(),
                            };
                            (k, v.clone())
                        })
                        .collect(),
                )
            }
            other => other.clone(),
        });
    // A map-form `expect.eligible` judges every entry / bundle beat of the
    // file it names under the test's mocks, presented or not (dsl 0.24.0,
    // T3-5) — so a lore test may carry it alone, presenting nothing.
    let judges_eligibility_by_id = eligible_want
        .as_ref()
        .is_some_and(serde_yaml::Value::is_mapping);
    let mut lore_lookup_only = false;
    // dsl 0.26.0 §7 (T1-7): a test asserting `eligible:` judges the
    // presentation, not a walk the engine would never make — an entry,
    // bundle beat or scene whose `when` is false under the mocks is shown on
    // its head and its body is not walked (no bridge answer it would need).
    mocks.gate_eligibility = eligible_want.is_some();

    // T1-13: a lore document is looked up, not played — `lute trace` refuses
    // one without `--entry`/`--beat`, and a test naming it without `entry:`
    // / `entries:` / `beat:` would walk nothing and PASS `end: complete`.
    // Say so instead of asserting against nothing — unless it judges the
    // entries by id (map-form `eligible:`) or the seeded world (`facts:` /
    // `notFacts:`), which presenting nothing answers.
    let judges_world = map
        .get("expect")
        .is_some_and(|e| e.get("facts").is_some() || e.get("notFacts").is_some());
    if entries.is_empty() && beat.is_none() {
        let doc = desugared(&input);
        let (folded, _, _) = lute_check::fold_env(&doc, &input);
        if folded.doc_kind == lute_check::DocKind::Lore
            && (judges_eligibility_by_id || judges_world)
        {
            lore_lookup_only = true;
        } else if folded.doc_kind == lute_check::DocKind::Lore {
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
                     {}), or `beat: <id>` (declared: {}), or judges them with `expect: \
                     {{ eligible: {{ <id>: true|false }} }}` or the seeded world with `expect: \
                     {{ facts: [...] }}`",
                    if ids.is_empty() {
                        "none".to_string()
                    } else {
                        ids.join(", ")
                    },
                    if beats.is_empty() {
                        "none".to_string()
                    } else {
                        beats.join(", ")
                    }
                )],
            ));
        }
    }

    // T1-14: mocked facts are judged against the project's producers.
    let project_asserts = if mocks.facts.is_empty() {
        None
    } else {
        shared.for_document(&lute_path, project, providers)
    };
    // dsl 0.26.0 §7 (T3-5), round-5 T3-24: an `accepts:`, a `quests:` seed or
    // an `expect.quests` naming a quest this document does not declare
    // resolves against every quest of the project.
    let expected_quests: Vec<&str> = map
        .get("expect")
        .and_then(|v| v.get("quests"))
        .and_then(|v| v.as_mapping())
        .map(|m| m.keys().filter_map(|k| k.as_str()).collect())
        .unwrap_or_default();
    let named_quests: Vec<&str> = mocks
        .accepts
        .iter()
        .map(String::as_str)
        .chain(mocks.state.iter().filter_map(|(p, _, _)| seeded_quest(p)))
        .chain(expected_quests.iter().copied())
        .collect();
    if !named_quests.is_empty() {
        let doc = desugared(&input);
        let foreign = named_quests
            .iter()
            .any(|id| !doc.quests.iter().any(|q| q.id == *id));
        if foreign {
            mocks.project_quests =
                resolve_with.and_then(|root| shared.project_quests(root, providers));
        }
    }
    // What a quest another document declares starts the walk as (T3-24).
    let foreign_start = ForeignQuestStart {
        project: mocks.project_quests.clone(),
        seeds: mocks
            .state
            .iter()
            .filter_map(|(p, v, _)| seeded_quest(p).map(|id| (id.to_string(), v.clone())))
            .collect(),
        accepted: mocks.accepts.clone(),
    };
    // One envelope: the document is gated on its project's verdict (what
    // `lute trace --project` and `lute play` gate on), else the standalone
    // check. An unpresented entry / beat is judged under the same mocks and
    // verdict (T3-5).
    let checked = resolve_with
        .and_then(|root| shared.gate(root, &lute_path))
        .unwrap_or_else(|| lute_check::check(&input));
    let eligibility_mocks = judges_eligibility_by_id.then(|| (mocks.clone(), checked.clone()));
    let (report, exit) = if let Some(beat) = &beat {
        trace_beat_with_check(&input, checked, mocks, beat, project_asserts.as_ref())
    } else if lore_lookup_only {
        trace_entries_with_check(&input, checked, mocks, &[], project_asserts.as_ref())
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
        // A `bridges:` answer's diagnostic is anchored in THIS file's text
        // (dsl 0.24.0 §5), every other one in the traced document.
        let test_display = test_file.display().to_string();
        let lines: Vec<String> = diags
            .iter()
            .map(|d| {
                let at = if d.provenance.as_deref() == Some(lute_trace::MOCK_TEXT) {
                    &test_display
                } else {
                    &lute_display
                };
                let severity = crate::output::severity_str(d.severity);
                format!(
                    "{}:{}:{}: {severity} [{}] {}",
                    at,
                    d.span.line,
                    d.span.column,
                    d.code,
                    yaml_key_spelling(&d.text())
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
    // How the walk ended: `terminal` when it completed with the project's
    // `terminal:` holding.
    let ended = match exit_str {
        "complete" if report.terminal == Some(true) => "terminal",
        e => e,
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
        // end: complete | terminal | incomplete | error — how the walk
        // ended.
        if let Some(want) = expect.get("end").and_then(|v| v.as_str()) {
            expectations.push(ExpectResult {
                not_raised: None,
                why: None,
                kind: "end",
                subject: String::new(),
                expected: want.to_string(),
                actual: Some(ended.to_string()),
                passed: want == ended,
            });
        }

        // transcriptContains / transcriptLacks: [substrings] — against the
        // presented content lines in the one canonical transcript form
        // (`lute_trace::exec::said_line`, dsl 0.24.0 T1-2), the form `lute
        // play` matches too; never the trace's human rendering (headers,
        // decisions, staging). A needle's attribute block binds the line it
        // lands on (0.27, T1-11).
        let transcript = if expect.contains_key("transcriptContains")
            || expect.contains_key("transcriptLacks")
        {
            report.said()
        } else {
            String::new()
        };
        for (kind, want_present) in [("transcriptContains", true), ("transcriptLacks", false)] {
            let Some(list) = expect.get(kind).and_then(|v| v.as_sequence()) else {
                continue;
            };
            for sub in list.iter().filter_map(|i| i.as_str()) {
                // A scene walk is one step.
                let miss = lute_trace::exec::record::judge(&transcript, &[], sub, want_present);
                expectations.push(ExpectResult {
                    not_raised: None,
                    why: None,
                    kind,
                    subject: String::new(),
                    expected: sub.to_string(),
                    passed: miss.is_none(),
                    actual: Some(miss.unwrap_or_else(|| {
                        if want_present { "present" } else { "absent" }.to_string()
                    })),
                });
            }
        }

        // options: { <choice id>: [option ids] } (dsl 0.22.0 §5) — the
        // options the walk actually offered at that branch/hub, as a set:
        // every presentation's eligible choices, unioned (a hub is offered
        // once per visit). `None` when the walk never presented it.
        if let Some(options) = expect.get("options").and_then(|v| v.as_mapping()) {
            for (k, v) in options {
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
                    not_raised: None,
                    why: None,
                    kind: "options",
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
        // A reserved path of the document's own quests reads its engine
        // default before any write, as in a play (T3-65).
        if let Some(state) = expect.get("state").and_then(|v| v.as_mapping()) {
            let final_state = &report.final_state;
            let doc = desugared(&input);
            for (k, v) in state {
                let Some(path) = k.as_str() else { continue };
                let want = yaml_scalar_text(v).unwrap_or_default();
                let actual = final_state
                    .get(path)
                    .cloned()
                    .or_else(|| reserved_quest_default(&doc, path).map(str::to_string));
                expectations.push(ExpectResult {
                    not_raised: None,
                    why: None,
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
        // lifecycle the trace ran (dsl 0.21.0 §7a.4); a quest another
        // document of the project declares, against where the walk left it
        // (round-5 T3-24, [`ForeignQuestStart::judge`]).
        if let Some(quests) = expect.get("quests").and_then(|v| v.as_mapping()) {
            let final_quests = final_quests(&report, &input);
            for (k, v) in quests {
                let Some(id) = k.as_str() else { continue };
                let want = yaml_scalar_text(v).unwrap_or_default();
                let (actual, why) = match final_quests.get(id) {
                    Some(state) => (Some(state.clone()), None),
                    None => foreign_start.judge(&report, id, &final_quests),
                };
                expectations.push(ExpectResult {
                    not_raised: None,
                    why,
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
                        not_raised: None,
                        why: None,
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
                    not_raised: None,
                    why: None,
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
        // engine's gate, shown, not enforced); this asserts the verdict. A
        // map key naming an entry / bundle beat of the file this test did
        // NOT present is judged on its own under the same mocks (dsl 0.24.0,
        // T3-5) — `eligible:` covers the whole file, not only what played.
        if let Some(want) = &eligible_want {
            let presented = presented_eligibility(&report);
            // dsl 0.28.0 (T1-25): `{ <id>: { false: <reason> } }` also
            // names the premise that must close it (`spentBy`, `when`, …) —
            // so a beat closed by something else does not pass vacuously.
            let parse = |v: &serde_yaml::Value| -> (Option<bool>, Option<String>) {
                match v {
                    serde_yaml::Value::Mapping(m) if m.len() == 1 => {
                        match m.get(serde_yaml::Value::Bool(false)).map(yaml_scalar_text) {
                            Some(reason) => (Some(false), Some(reason.unwrap_or_default())),
                            None => (None, None),
                        }
                    }
                    other => (other.as_bool(), None),
                }
            };
            let wants: Vec<(Option<String>, Option<bool>, Option<String>)> = match want {
                serde_yaml::Value::Mapping(m) => m
                    .iter()
                    .map(|(k, v)| {
                        let (b, reason) = parse(v);
                        (k.as_str().map(str::to_string), b, reason)
                    })
                    .collect(),
                other => vec![(None, other.as_bool(), None)],
            };
            for (id, want, reason) in wants {
                let mut matched: Vec<(String, Option<bool>)> = presented
                    .iter()
                    .filter(|(p, _)| id.as_deref().is_none_or(|id| names_presented(p, id)))
                    .cloned()
                    .collect();
                let mut alone = None;
                if matched.is_empty() {
                    if let (Some(id), Some((mocks, checked))) = (id.as_deref(), &eligibility_mocks)
                    {
                        alone =
                            eligibility_alone(&input, checked, mocks, id, project_asserts.as_ref());
                        matched = alone
                            .as_ref()
                            .map(presented_eligibility)
                            .unwrap_or_default();
                    }
                }
                // Round-5 T3-12: an `eligible: true` miss names the false
                // premise, as the implicit miss does. OT-F-10: a key naming
                // nothing the document declares gets a did-you-mean.
                let why = if matched.is_empty() {
                    id.as_deref().and_then(|id| {
                        let doc = desugared(&input);
                        let names = doc
                            .entries
                            .iter()
                            .map(|e| e.id.as_str())
                            .chain(doc.beats.iter().map(|b| b.id.as_str()));
                        lute_manifest::suggest::nearest(id, names, 3)
                            .map(|k| format!("did you mean `{k}`?"))
                    })
                } else {
                    (want == Some(true))
                        .then(|| matched.iter().find(|(_, e)| *e == Some(false)))
                        .flatten()
                        .map(|(p, _)| ineligible_why(alone.as_ref().unwrap_or(&report), p))
                };
                let judged = alone.as_ref().unwrap_or(&report);
                let by = |p: &str| judged.ineligible_by.get(p).copied();
                let actual = (!matched.is_empty()).then(|| {
                    matched
                        .iter()
                        .map(|(p, e)| match (e, by(p).filter(|_| reason.is_some())) {
                            (Some(true), _) => "true".to_string(),
                            (Some(false), Some(kind)) => format!("false ({kind})"),
                            (Some(false), None) => "false".to_string(),
                            (None, _) => "unknown".to_string(),
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                });
                let known = |r: &str| lute_trace::exec::session::PREMISE_KINDS.contains(&r);
                let expected = match (want, &reason) {
                    (Some(_), Some(r)) if known(r) => format!("false ({r})"),
                    (Some(_), Some(r)) => format!(
                        "false for a reason, and `{r}` is none (one of: {})",
                        lute_trace::exec::session::PREMISE_KINDS.join(", ")
                    ),
                    (Some(b), None) => b.to_string(),
                    (None, _) => "true, false or `{ false: <reason> }`".to_string(),
                };
                let reason_holds = reason.as_deref().is_none_or(|r| {
                    known(r)
                        && matched
                            .iter()
                            .all(|(p, e)| *e != Some(false) || by(p) == Some(r))
                });
                expectations.push(ExpectResult {
                    not_raised: (want == Some(true))
                        .then(|| matched.iter().find(|(_, e)| *e == Some(false)))
                        .flatten()
                        .and_then(|(p, _)| {
                            alone.as_ref().unwrap_or(&report).not_raised.get(p).cloned()
                        }),
                    why,
                    kind: "eligible",
                    subject: id.unwrap_or_default(),
                    passed: want.is_some()
                        && !matched.is_empty()
                        && matched.iter().all(|(_, e)| *e == want)
                        && reason_holds,
                    expected,
                    actual,
                });
            }
        }

        // accepts: [quest ids] (dsl 0.24.0, T3-5) — the quests the walk's
        // `::accept{quest=…}` accepted, as a set. A scene never holds the
        // quest, so its lifecycle (`quests:`) cannot show the accept.
        if let Some(v) = expect.get("accepts") {
            let want: Option<BTreeSet<&str>> = v
                .as_sequence()
                .and_then(|s| s.iter().map(|i| i.as_str()).collect());
            let actual: BTreeSet<&str> = report
                .steps
                .iter()
                .filter_map(|s| match s {
                    lute_trace::Step::Accept { quest, .. } => Some(quest.as_str()),
                    _ => None,
                })
                .collect();
            let set_text = |s: &BTreeSet<&str>| {
                format!("[{}]", s.iter().copied().collect::<Vec<_>>().join(", "))
            };
            expectations.push(ExpectResult {
                not_raised: None,
                why: None,
                kind: "accepts",
                subject: String::new(),
                expected: match &want {
                    Some(w) => set_text(w),
                    None => "a list of quest ids".to_string(),
                },
                actual: Some(set_text(&actual)),
                passed: want.as_ref() == Some(&actual),
            });
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
        r.end = Some(ended.to_string());
        return Ok(r);
    }

    // T1-13: an incomplete trace halted at an unknown guard, so everything
    // after it — including whatever the expectations above were written
    // about — was never walked. It passed whenever the test did not mention
    // `end:`. Now it fails unless the test opts in with `end: incomplete`.
    let declares_exit = expect.is_some_and(|e| e.contains_key("end"));
    if exit_str == "incomplete" && !declares_exit {
        expectations.push(ExpectResult {
            not_raised: None,
            why: None,
            kind: "end",
            subject: IMPLICIT_EXIT.to_string(),
            expected: "complete".to_string(),
            actual: Some(exit_str.to_string()),
            passed: false,
        });
    }

    // dsl 0.26.0 §7 (T1-7): a walk of an entry, bundle beat or scene the
    // engine would never present — its `when` false under these mocks —
    // proves nothing about play (a contract test of another author's file
    // silently went hollow). It fails unless the test asserts `eligible:`.
    let asserted = eligible_want.as_ref();
    for (id, eligible) in presented_eligibility(&report) {
        if eligible == Some(false) && !eligibility_asserted(&id, asserted) {
            expectations.push(ExpectResult {
                not_raised: report.not_raised.get(&id).cloned(),
                why: Some(ineligible_why(&report, &id)),
                kind: "eligible",
                subject: id,
                expected: IMPLICIT_ELIGIBLE.to_string(),
                actual: Some("false".to_string()),
                passed: false,
            });
        }
    }

    let passed = expectations.iter().all(|e| e.passed);
    // Prerelease N3: a scene decided ineligible is judged by `eligible:` (or
    // fails on it above) — the trace's "shows it as if it had been presented"
    // note would contradict both.
    let scene_ineligible = report
        .scene_eligible
        .as_ref()
        .is_some_and(|(_, e)| *e == Some(false));

    Ok(TestResult {
        test_file: test_file.to_path_buf(),
        kind: "test",
        lute_file: lute_display,
        exit: exit_str.to_string(),
        end: Some(ended.to_string()),
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
            .filter(|n| {
                (n.starts_with(lute_trace::NOTE_BEAT_WHEN) && !scene_ineligible)
                    || n.starts_with(lute_trace::NOTE_ACCEPT_SPENT)
            })
            .cloned()
            .chain(ineligible_notes(&report, asserted))
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
        declares_exit: top_expect.is_some_and(|e| e.get("end").is_some()),
    }
}

/// Run one expect-carrying play (dsl 0.22.0 §4) through the SAME walk and
/// judge `lute play` uses. `None` for a play without `expect:`. The project
/// is `--project`, else the nearest `lute.project.yaml` above the play. A
/// play that could not run (usage error, no project) is a FAILURE naming
/// why; a play that halted fails unless its top-level `expect:` declares the
/// exit — the rule an incomplete trace follows (T1-13). When `cov` is `Some`,
/// every document, beat and entry the play presented counts as covered, and
/// every branch/hub it answered joins the choice rows (T3-20).
fn run_one_play(
    play_file: &Path,
    project: Option<&Path>,
    no_derive: bool,
    shared: &Shared,
    cov: Option<&mut CoverageAccum>,
) -> Option<TestResult> {
    // One `error:` line per usage error, each plain text.
    let refused = |lute_file: String, why: &str| {
        let lines = why
            .lines()
            .map(|l| format!("error: {}", lute_core_span::plain_message(l)))
            .collect();
        let mut r = TestResult::refused(play_file, lute_file, "invalid", lines);
        r.kind = "play";
        Some(r)
    };
    let declares_exit = match scan_play(play_file) {
        PlayScan::NoExpect => return None,
        PlayScan::Expect { declares_exit } => declares_exit,
        PlayScan::Broken(why) => return refused(String::new(), &why),
    };
    let Some(project_dir) = project_dir_of(play_file, project) else {
        return refused(
            String::new(),
            "no `lute.project.yaml` above this play — a play runs a project; pass \
             `--project <dir>`",
        );
    };
    let project_display = display_path(&project_dir);
    let compiled;
    let play_project = match shared.plays.get(&project_dir) {
        Some(p) => p,
        None => {
            compiled = crate::play::PlayProject::compile(&project_dir);
            &compiled
        }
    };
    let run = match crate::play::run_play_for_test(play_project, play_file, !no_derive) {
        Ok(run) => run,
        Err(why) => return refused(project_display, &why),
    };
    if let Some(cov) = cov {
        cov.plays += 1;
        let mut canonical: BTreeMap<String, String> = BTreeMap::new();
        let mut canon = |doc: &str| -> String {
            canonical
                .entry(doc.to_string())
                .or_insert_with(|| canonical_key(&project_dir.join(doc)))
                .clone()
        };
        for doc in &run.presented_docs {
            let c = canon(doc);
            cov.traced_files.insert(c.clone());
            cov.play_files.insert(c);
        }
        for (doc, id) in &run.presented {
            let unit = (canon(doc), id.clone());
            cov.units.insert(unit.clone());
            cov.play_units.insert(unit);
        }
        for c in &run.choices {
            let file = display_path(&project_dir.join(&c.document));
            let row = cov.choice_row(&canon(&c.document), &file, &c.id);
            row.chosen.extend(c.chose.iter().cloned());
            row.eligible.extend(c.offered.iter().cloned());
            row.total = row.total.max(c.total);
        }
    }
    let mut misses = run.misses;
    if run.exit != "complete" && !declares_exit {
        misses.push(ExpectMiss {
            step: None,
            label: None,
            occasion: None,
            repetition: None,
            key: "end".to_string(),
            expected: "complete (a halted play fails unless its top-level `expect:` declares \
                       how it ends, e.g. `expect: { end: incomplete }`)"
                .to_string(),
            actual: run.exit.to_string(),
        });
    }
    Some(TestResult {
        test_file: play_file.to_path_buf(),
        kind: "play",
        lute_file: project_display,
        exit: run.exit.to_string(),
        end: Some(run.end.to_string()),
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
    if let Some((id, list)) = atom
        .strip_prefix("--choose ")
        .and_then(|rest| rest.split_once('='))
    {
        let list = list.split(',').collect::<Vec<_>>().join(", ");
        return format!("`choose: {{ {id}: [{list}] }}`");
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

/// The `expected` text of the `eligible` expectation a test carries
/// implicitly for a presented entry / beat / scene whose `when` is false
/// (dsl 0.26.0 §7, T1-7).
const IMPLICIT_ELIGIBLE: &str = "an eligible presentation, or an `eligible:` assertion";

/// Prerelease N3, round-5 T3-12: the premise that makes presented `id`
/// ineligible, named for the miss line — an entry an earlier read spent
/// (which read), else the verdict the session's eligibility rule gave it
/// ([`TraceReport::premises`]: its `when`, its `after:` / `after=` with the
/// mocks it needs, a spent `once`, a holding `spentBy`), else its `when`.
fn ineligible_why(report: &TraceReport, id: &str) -> String {
    let entry_heads: Vec<Option<&String>> = report
        .steps
        .iter()
        .filter_map(|s| match s {
            lute_trace::Step::Entry { id: e, spent, .. } if e == id => Some(spent.as_ref()),
            _ => None,
        })
        .collect();
    if let Some(at) = entry_heads.iter().position(Option::is_some) {
        let once = entry_heads[at].map_or("", String::as_str);
        let flag = if once == "run" { "read" } else { "everRead" };
        let by = if at > 0 {
            "an earlier presentation in this test read it".to_string()
        } else if once == "run" {
            format!("the mocked `entriesRead: {{ run: [{id}] }}` read it")
        } else {
            format!("the mocked `entriesRead:` (`{{ user: [{id}] }}` or `run:`) read it")
        };
        return format!("it is `once=\"{once}\"` and already spent — {by} (`entry.{id}.{flag}`)");
    }
    match report.premises.get(id) {
        Some(why) => why.clone(),
        None => "its `when` is false".to_string(),
    }
}

/// Every presented lore entry / bundle beat — and the traced scene itself
/// (dsl 0.26.0 §7, T1-7) — with its eligibility verdict, in presentation
/// order: `Some(true)` eligible (or no `when`), `Some(false)` not, `None`
/// undecided under the mocks.
fn presented_eligibility(report: &TraceReport) -> Vec<(String, Option<bool>)> {
    report
        .scene_eligible
        .iter()
        .cloned()
        .chain(report.steps.iter().filter_map(|s| match s {
            lute_trace::Step::Entry { id, eligible, .. }
            | lute_trace::Step::Beat { id, eligible, .. } => Some((id.clone(), *eligible)),
            _ => None,
        }))
        .collect()
}

/// Does the test's `expect.eligible` assert the verdict of the presented
/// `id`? The scalar form covers every presented one, the map form the ids
/// it names (dsl 0.24.0, T3-5).
fn eligibility_asserted(id: &str, asserted: Option<&serde_yaml::Value>) -> bool {
    match asserted {
        None => false,
        Some(serde_yaml::Value::Mapping(m)) => m
            .keys()
            .filter_map(serde_yaml::Value::as_str)
            .any(|k| names_presented(id, k)),
        Some(_) => true,
    }
}

/// Does the `expect.eligible` key `id` name the presented entry / beat `p`
/// (a local id matches its canonical `<document id>.<id>`)?
fn names_presented(p: &str, id: &str) -> bool {
    p == id || p.ends_with(&format!(".{id}"))
}

/// `input`'s document as `check` sees it: parsed, then desugared — the beats
/// a `beat:` template or a `chapters:` chain derives included (dsl 0.27.0 §6), so a
/// test names and judges them like authored ones.
fn desugared(input: &lute_check::CheckInput) -> lute_syntax::ast::Document {
    let (mut doc, _) = lute_syntax::parse(&input.text);
    let _ = lute_check::desugar_document(&mut doc, input);
    doc
}

/// `id`'s eligibility judged on its own under `mocks` and the gate verdict
/// `checked` (dsl 0.24.0, T3-5): the entry / bundle beat of `input`'s
/// document is presented alone, from the mocked start, and its head's
/// verdict read back. Empty when the document declares no such entry or
/// beat.
fn eligibility_alone(
    input: &lute_check::CheckInput,
    checked: &lute_check::CheckResult,
    mocks: &lute_trace::MockSet,
    id: &str,
    project_asserts: Option<&BTreeSet<String>>,
) -> Option<TraceReport> {
    let doc = desugared(input);
    let checked = checked.clone();
    let (report, _) = if doc.entries.iter().any(|e| e.id == id) {
        trace_entries_with_check(input, checked, mocks.clone(), &[id], project_asserts)
    } else if doc
        .beats
        .iter()
        .any(|b| names_presented(id, &b.id) || b.id == id)
    {
        trace_beat_with_check(input, checked, mocks.clone(), id, project_asserts)
    } else {
        return None;
    };
    Some(report)
}

/// A note for every presented entry / bundle beat / scene whose `when` is
/// undecided under the test's mocks — trace presents it anyway — unless the
/// test's `expect.eligible` asserts that verdict. A `when` that is false is
/// no note: the test fails on it (dsl 0.26.0 §7, T1-7).
fn ineligible_notes(report: &TraceReport, asserted: Option<&serde_yaml::Value>) -> Vec<String> {
    presented_eligibility(report)
        .into_iter()
        // A scene's undecided `when` is the trace's own beat-`when` note.
        .filter(|(id, _)| report.scene_eligible.as_ref().is_none_or(|(s, _)| s != id))
        .filter(|(id, eligible)| eligible.is_none() && !eligibility_asserted(id, asserted))
        .map(|(id, _)| {
            let local = id.rsplit('.').next().unwrap_or(&id);
            format!(
                "`{id}` may not be eligible under these mocks (its `when` is undecided); the \
                 test presents it anyway — assert it with `expect: {{ eligible: {{ {local}: \
                 true|false }} }}` or supply what its `when` reads"
            )
        })
        .collect()
}

/// Every quest the traced document declares, mapped to where the walk left
/// it (dsl 0.21.0 §7a.4): the LAST `quest` decision's `active`/`complete`/
/// `failed` outcome, else `unset` — a quest whose `start` never held, that
/// awaited an accept, or that was never decided at all never left `unset`.
/// Read off the transcript's decisions, the same record the human report
/// prints, never a second lifecycle model.
fn final_quests(report: &TraceReport, input: &lute_check::CheckInput) -> BTreeMap<String, String> {
    let doc = desugared(input);
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

/// T3-65: what a reserved path of one of `doc`'s own quests reads before the
/// engine writes it, as a play reads it — `quest.<q>.state` / `.failedBy`
/// `unset`, `quest.<q>.objectives.<o>.done` / `.failed` `false`. `None` for
/// any other path.
fn reserved_quest_default(doc: &lute_syntax::ast::Document, path: &str) -> Option<&'static str> {
    let segs: Vec<&str> = path.split('.').collect();
    let quest = |id: &str| doc.quests.iter().find(|q| q.id == id);
    match segs.as_slice() {
        ["quest", q, "state" | "failedBy"] => quest(q).map(|_| "unset"),
        ["quest", q, "objectives", o, "done" | "failed"] => quest(q)?
            .body
            .iter()
            .any(|n| matches!(n, lute_syntax::ast::Node::Objective(obj) if obj.id == *o))
            .then_some("false"),
        _ => None,
    }
}

/// The quest id a `quest.<id>.state` seed (a `quests:` entry, or a `state:`
/// seed of the reserved path) names.
fn seeded_quest(path: &str) -> Option<&str> {
    path.strip_prefix("quest.")?.strip_suffix(".state")
}

/// Round-5 T3-24: what `expect.quests` needs to judge a quest another
/// document of the project declares. The walk holds no such quest — it
/// never judges its `start` or objectives — so the quest ends the walk as
/// it began (its `quests:` / `state:` seed, else `unset`), except that an
/// accept — `accepts:`, or a `::accept` the walk ran that is not queued for
/// the next run — activates it while `unset`, as the engine's settle does.
struct ForeignQuestStart {
    /// Every quest id of the resolved project (`None`: no project resolved,
    /// or the test names no quest outside the traced document).
    project: Option<BTreeSet<String>>,
    seeds: BTreeMap<String, String>,
    accepted: Vec<String>,
}

impl ForeignQuestStart {
    /// `(observed state, why)` for `id`, which the traced document does not
    /// declare (`own` is [`final_quests`]). The state is `None` when no
    /// quest of the project — or, with no project, of the document —
    /// declares `id`; `why` then says so, with the nearest id.
    fn judge(
        &self,
        report: &TraceReport,
        id: &str,
        own: &BTreeMap<String, String>,
    ) -> (Option<String>, Option<String>) {
        if !self.project.as_ref().is_some_and(|p| p.contains(id)) {
            let (unknown, candidates): (String, Vec<&str>) = match &self.project {
                Some(p) => (
                    format!("no document of the project declares quest `{id}`"),
                    p.iter().map(String::as_str).collect(),
                ),
                None => (
                    format!("the traced document declares no quest `{id}`"),
                    own.keys().map(String::as_str).collect(),
                ),
            };
            let hint = lute_manifest::suggest::nearest(id, candidates, 2)
                .map(|c| format!(" — did you mean `{c}`?"))
                .unwrap_or_default();
            return (None, Some(format!("{unknown}{hint}")));
        }
        let seeded = self
            .seeds
            .get(id)
            .map(|s| s.trim_matches(['\'', '"']).to_string());
        let accepted = self.accepted.iter().any(|a| a == id)
            || report.steps.iter().any(
                |s| matches!(s, lute_trace::Step::Accept { quest, next_run: false } if quest == id),
            );
        let state = match seeded {
            Some(s) if s != "unset" => s,
            _ if accepted => "active".to_string(),
            _ => "unset".to_string(),
        };
        let why = format!(
            "quest `{id}` is declared in another document of the project, so the walk does not \
             judge its `start` or objectives: it is its state at the start of the walk (a \
             `quests:` seed, else `unset`), made `active` by an accept"
        );
        (Some(state), Some(why))
    }
}

/// T9.11's second half. `lute-trace` composes its mock diagnostics for
/// `lute trace`'s command line (`--choose id=arm`, `--state path=value`,
/// `--fact`, `--event`, `--accept`, `--entry`), but in a `*.test.yaml` the same input
/// arrived as a YAML KEY. Printing the message verbatim names a syntax the
/// file cannot use. This rewrites the flag spelling to the key spelling and
/// nothing else — the codes, ids, values and clause citations are
/// `lute-trace`'s and stay exactly as written.
fn yaml_key_spelling(message: &str) -> String {
    // Round-5 T3-12: a backticked mock hint (`` `--fact "f"` ``, `` `--state
    // p=<value>` ``, a refused `--choose`'s premise) becomes the YAML entry
    // a test writes, as an unresolved atom's hint does ([`yaml_atom_hint`]).
    let mut out = String::with_capacity(message.len());
    let mut rest = message;
    while let Some(start) = rest.find("`--") {
        let after = &rest[start + 1..];
        let Some(end) = after.find('`') else { break };
        let inner = &after[..end];
        out.push_str(&rest[..start]);
        if inner.starts_with("--state ") || inner.starts_with("--fact ") {
            out.push_str(&yaml_atom_hint(inner));
        } else {
            out.push('`');
            out.push_str(inner);
            out.push('`');
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
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

/// Fold one report's decisions + coverage counts into the run accumulator:
/// its document, the bundle beats / entries it presented (T3-20), its
/// branch/hub picks under the canonical site a play's picks share.
fn accumulate_coverage(cov: &mut CoverageAccum, report: &TraceReport) {
    cov.paths += 1;
    let canonical = canonical_key(std::path::Path::new(&report.file));
    // OT-F-14: one spelling per file, whichever test directory traced it.
    let file = display_path(Path::new(&report.file));
    cov.traced_files.insert(canonical.clone());
    for s in &report.steps {
        if let lute_trace::Step::Entry { id, .. } | lute_trace::Step::Beat { id, .. } = s {
            cov.units.insert((canonical.clone(), id.clone()));
        }
    }
    for d in &report.decisions {
        match d.construct.as_str() {
            "branch" | "hub" => {
                let row = cov.choice_row(&canonical, &file, &d.id);
                row.chosen.insert(d.outcome.clone());
                row.eligible.extend(d.eligible.iter().cloned());
            }
            "match" | "guard" => {
                let site = lute_trace::report::site_key_in(&d.span, d.component.as_ref());
                let key = arm_row_key(&file, &site, d.component.as_ref());
                cov.arms
                    .entry(key)
                    // Ember N14: the author's `@def` spelling, not its
                    // expansion (as `lute trace` prints it, T3-12).
                    .or_insert_with(|| {
                        ArmRow::new(
                            d.authored_id.clone().unwrap_or_else(|| d.id.clone()),
                            d.construct == "guard",
                        )
                    })
                    .chosen
                    .insert(d.outcome.clone());
            }
            _ => {}
        }
    }
    for c in report.coverage.choices.values() {
        let row = cov.choice_row(&canonical, &file, &c.label);
        row.total = row.total.max(c.total);
    }
    for (site, c) in &report.coverage.arms {
        let key = arm_row_key(&file, site, c.component.as_ref());
        let row = cov
            .arms
            .entry(key)
            // N14: the author's `@def` spelling, not its expansion (T3-12).
            .or_insert_with(|| {
                ArmRow::new(
                    c.authored_label.clone().unwrap_or_else(|| c.label.clone()),
                    c.guard,
                )
            });
        row.total = row.total.max(c.total);
    }
}

/// The printed coverage site of a `<match>` / guard at trace `site` of the
/// document `file`: `file:line:column`, or — expanded from a component
/// `::use` (SD-15) — the component's own `file:line:column` and which use
/// in which document, so a component's match never merges into its host's
/// construct at the same position, nor into another use's.
fn arm_row_key(file: &str, site: &str, component: Option<&lute_trace::ComponentSite>) -> String {
    match component {
        None => format!("{file}:{site}"),
        Some(c) => {
            // `site_key_in`: `"{c.file}:{line}:{column} ({use})"`.
            let at = site
                .strip_prefix(c.file.as_str())
                .and_then(|rest| rest.split_once(" ("))
                .map_or("", |(at, _)| at);
            format!(
                "{}{at} ({} in {file})",
                display_path(Path::new(&c.file)),
                c.scope
            )
        }
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

/// `path` with each `dir/..` pair dropped lexically, for display only
/// (`./tests/../scenes/a.lute` → `./scenes/a.lute`). A leading `..` stays.
fn fold_parent_dirs(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            std::path::Component::ParentDir
                if matches!(
                    out.components().next_back(),
                    Some(std::path::Component::Normal(_))
                ) =>
            {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
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
    units: &[CoverageUnit],
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
        // T3-23: a test that never resolved a document names none — no `()`.
        let what = match (r.kind == "play", r.lute_file.is_empty()) {
            (_, true) => String::new(),
            (true, false) => format!("  (play of {})", r.lute_file),
            (false, false) => format!("  ({})", r.lute_file),
        };
        outln!(out, "{mark}  {}{what}", r.test_file.display());
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
        // A halted play: the halt is the root cause, so it comes first, and
        // the end-of-play expectations it left unmet fold into one line.
        let halted = r.kind == "play" && !r.notes.is_empty();
        if halted {
            for n in &r.notes {
                outln!(out, "      halted: {n}");
            }
        }
        if !r.passed {
            // OT-F-10: a false eligibility is the cause of the state and
            // fact misses its unwalked body leaves, so it comes first.
            let (causes, rest): (Vec<&ExpectResult>, Vec<&ExpectResult>) = r
                .expectations
                .iter()
                .filter(|e| !e.passed)
                .partition(|e| e.kind == "eligible");
            for e in causes.into_iter().chain(rest) {
                render_miss(out, e);
            }
            let (at_end, misses): (Vec<&ExpectMiss>, Vec<&ExpectMiss>) = r
                .misses
                .iter()
                .partition(|m| halted && m.step.is_none() && m.key != "end");
            for m in misses {
                outln!(out, "      {m}");
            }
            match at_end.as_slice() {
                [] => {}
                [m] => outln!(out, "      {m}"),
                ms => {
                    outln!(
                    out,
                    "      end of play: {} expectations missed on the world the halt left ({}) \
                     — fix the halt first",
                    ms.len(),
                    ms.iter().map(|m| m.key.as_str()).collect::<Vec<_>>().join(", ")
                )
                }
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
        if !halted {
            for n in &r.notes {
                outln!(out, "      note: {n}");
            }
        }
        for a in &r.autopicked {
            outln!(out, "      auto-picked (no selection supplied): {a}");
        }
    }

    let passed = results.iter().filter(|r| r.passed).count();
    let failed = results.len() - passed;
    outln!(out, "\n{passed} passed, {failed} failed");

    if let Some((cov, root)) = cov {
        render_coverage_human(out, cov, root, units);
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
        ("options", Some(actual)) => outln!(
            out,
            "      options {}: expected {}, got {actual}",
            e.subject,
            e.expected
        ),
        ("options", None) => outln!(
            out,
            "      options {}: expected {}, but the walk never presented a branch/hub `{}`",
            e.subject,
            e.expected,
            e.subject
        ),
        ("state", Some(actual)) => outln!(
            out,
            "      state {}: expected {}, got {}",
            e.subject,
            state_literal(&e.expected),
            state_literal(actual)
        ),
        ("quests", _) if !QUEST_STATES.contains(&e.expected.as_str()) => outln!(
            out,
            "      quests {}: {:?} is not a quest state (expected one of: {})",
            e.subject,
            e.expected,
            QUEST_STATES.join(", ")
        ),
        ("quests", Some(actual)) => {
            outln!(
                out,
                "      quests {}: expected {:?}, got {:?}",
                e.subject,
                e.expected,
                actual
            );
            if let Some(why) = &e.why {
                outln!(out, "      note: {why}");
            }
        }
        ("quests", None) => outln!(
            out,
            "      quests {}: expected {:?}, but {}",
            e.subject,
            e.expected,
            e.why
                .as_deref()
                .unwrap_or("the traced document declares no such quest")
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
                "      state {}: expected {}, but the path was never written and has no seed \
                 or declared default",
                e.subject,
                state_literal(&e.expected)
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
        ("end", Some(actual)) if e.subject == IMPLICIT_EXIT => outln!(
            out,
            "      end: {actual} — an unknown guard halted the walk before the end, so the \
             expectations after it were never walked; an incomplete trace fails unless the \
             test declares `expect: {{ end: incomplete }}`"
        ),
        ("end", Some(actual)) => outln!(out, "      end: expected {}, got {actual}", e.expected),
        ("facts" | "notFacts", Some(actual)) => outln!(
            out,
            "      {} {}: expected {}, got {actual}",
            e.kind,
            e.subject,
            e.expected
        ),
        ("eligible", Some(_)) if e.expected == IMPLICIT_ELIGIBLE => outln!(
            out,
            "      eligible {id}: not eligible under these mocks ({}) — the engine would never \
             present it, so the walk proves nothing about play; fix the mocks, or assert \
             `expect: {{ eligible: {{ {id}: false }} }}` (the body is then not walked)",
            e.why.as_deref().unwrap_or("its `when` is false"),
            id = e.subject,
        ),
        ("eligible", Some(actual)) => outln!(
            out,
            "      eligible{}: expected {}, got {actual}{}",
            if e.subject.is_empty() {
                String::new()
            } else {
                format!(" {}", e.subject)
            },
            e.expected,
            e.why
                .as_deref()
                .map(|why| format!(" — {why}"))
                .unwrap_or_default()
        ),
        ("eligible", None) if e.subject.is_empty() => outln!(
            out,
            "      eligible: expected {}, but the test presented no entry, beat or scene",
            e.expected
        ),
        ("eligible", None) => outln!(
            out,
            "      eligible {}: expected {}, but the document declares no such entry or beat{}",
            e.subject,
            e.expected,
            e.why
                .as_deref()
                .map(|hint| format!(" — {hint}"))
                .unwrap_or_default()
        ),
        ("accepts", Some(actual)) => {
            outln!(out, "      accepts: expected {}, got {actual}", e.expected)
        }
        _ => {}
    }
}

/// A state value as a miss line shows it: numbers and booleans bare (`3`,
/// `true`), anything else quoted (`"open"`) — the text form carries no type,
/// so the shape decides.
fn state_literal(text: &str) -> String {
    if text == "true" || text == "false" || text.parse::<f64>().is_ok_and(f64::is_finite) {
        text.to_string()
    } else {
        format!("{text:?}")
    }
}

/// Human coverage view — honest header, chosen/never-chosen names where the
/// reports expose them, counts where they do not. Every row names its
/// construct's own file and site; the guard text rides along as a label
/// (#24, T9.13). `units` are the TESTABLE units ([`coverage_units`]) —
/// components are not in it, and the strings below say so.
fn render_coverage_human(
    out: &mut String,
    cov: &CoverageAccum,
    root: &Path,
    units: &[CoverageUnit],
) {
    let root = display_path(root);
    if cov.plays == 0 {
        outln!(out, "\ncoverage over {} traced path(s):", cov.paths);
    } else {
        // T3-20: a play feeds the units it presented and the branch/hub
        // picks it made; the arm rows below are the traced paths' alone
        // (lighthouse N3).
        outln!(
            out,
            "\ncoverage over {} traced path(s) and {} play(s) (plays count toward what they \
             presented and the choices they picked, not match arms):",
            cov.paths,
            cov.plays
        );
    }
    if cov.choices.is_empty() && cov.arms.is_empty() {
        outln!(out, "  (no branch/hub or match constructs traced)");
    }
    for row in cov.choices.values() {
        let (chosen, total) = (&row.chosen, row.total);
        let never_named: Vec<&String> = row.eligible.difference(chosen).collect();
        let mut line = format!(
            "  branch/hub {} ({}): {}/{} chosen",
            row.label,
            row.site,
            chosen.len().min(total),
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
        // Choices never seen eligible anywhere: count only, honest.
        let unseen = total.saturating_sub(chosen.len() + never_named.len());
        if unseen > 0 {
            let anywhere = if cov.plays == 0 {
                "any traced path"
            } else {
                "any traced path or play"
            };
            line.push_str(&format!("; {unseen} never seen eligible in {anywhere}"));
        }
        outln!(out, "{line}");
    }
    for (key, row) in &cov.arms {
        let (label, chosen, total) = (&row.label, &row.chosen, &row.total);
        if row.guard {
            // T3-22: a `when=` guard is taken or skipped, not a match.
            let (taken, skipped) = (
                chosen.contains(lute_trace::report::GUARD_TAKEN),
                chosen.contains(lute_trace::report::GUARD_SKIPPED),
            );
            let seen = match (taken, skipped) {
                (true, true) => "taken and skipped",
                (true, false) => "taken; never skipped",
                (false, true) => "skipped; never taken",
                (false, false) => "never decided",
            };
            outln!(out, "  guard `{label}` ({key}): {seen}");
            continue;
        }
        let unexecuted = total.saturating_sub(chosen.len());
        // A `<match>` with no `on` has no subject to quote (ML-F4).
        let what = if label.trim().is_empty() {
            "match with no subject".to_string()
        } else {
            format!("match `{label}`")
        };
        let mut line = format!(
            "  {what} ({key}): {}/{} arm(s) executed",
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
    // T3-20: and a lore document holding two endings stayed "named" while
    // one of them lost its every proof — the unit is the beat, not the file.
    // Both strings say "testable", because component documents are out of the
    // denominator and claiming otherwise is the false-reassurance this whole
    // task is about.
    let untested: Vec<&CoverageUnit> = units.iter().filter(|u| !u.covered(cov)).collect();
    if untested.is_empty() {
        outln!(
            out,
            "  every testable document, beat and entry under {} is presented by at least one \
             test or play",
            root
        );
    } else {
        outln!(
            out,
            "  {} untested unit(s) under {} — no *.test.yaml presents them and no play presents \
             them:",
            untested.len(),
            root
        );
        for line in grouped(&untested) {
            outln!(out, "    {line}");
        }
    }
    // T3-20: the ending-proof view. A scene test traces a beat from mocked
    // state; only a play reaches it from the start, so a beat no play
    // presented has no proof it is reachable in play.
    let beats: Vec<&CoverageUnit> = units.iter().filter(|u| u.beat).collect();
    if beats.is_empty() {
        return;
    }
    if cov.plays == 0 {
        outln!(
            out,
            "  no play ran, so none of the {} beat(s) under {} is proven presented in play",
            beats.len(),
            root
        );
        return;
    }
    // OT-F-14: an untested beat is unplayed too; it is listed once, above.
    let unplayed: Vec<&CoverageUnit> = beats
        .into_iter()
        .filter(|u| !u.presented_by_play(cov))
        .collect();
    let traced_only: Vec<&CoverageUnit> = unplayed
        .iter()
        .copied()
        .filter(|u| u.covered(cov))
        .collect();
    if unplayed.is_empty() {
        outln!(out, "  every beat under {} is presented by a play", root);
    } else if traced_only.is_empty() {
        outln!(
            out,
            "  every other beat under {} is presented by a play",
            root
        );
    } else {
        outln!(
            out,
            "  {} {}beat(s) no play presents (a test traces them; only a play proves they are \
             reached in play):",
            traced_only.len(),
            if traced_only.len() < unplayed.len() {
                "more "
            } else {
                ""
            }
        );
        for line in grouped(&traced_only) {
            outln!(out, "    {line}");
        }
    }
}

/// Units one line per document, in walk order: `scenes/x.lute` for a whole
/// document, `lore/endings/ren.lute: ember, lantern` for its beats/entries.
fn grouped(units: &[&CoverageUnit]) -> Vec<String> {
    let mut lines: Vec<(String, Vec<&str>)> = Vec::new();
    for u in units {
        if lines.last().is_none_or(|(f, _)| *f != u.file) {
            lines.push((u.file.clone(), Vec::new()));
        }
        if u.id.is_some() {
            lines.last_mut().expect("pushed").1.push(&u.local);
        }
    }
    lines
        .into_iter()
        .map(|(f, ids)| {
            if ids.is_empty() {
                f
            } else {
                format!("{f}: {}", ids.join(", "))
            }
        })
        .collect()
}

/// One unit as JSON: its file, its project id (an entry's global id, a
/// bundle beat's `<doc>.<beat>`, a scene's key), the document it is in
/// (`doc`), its id there (`local`, `null` for a whole document), and its
/// kind.
fn unit_json(u: &CoverageUnit) -> serde_json::Value {
    serde_json::json!({
        "file": u.file,
        "id": u.id.clone().or_else(|| (!u.local.is_empty()).then(|| u.local.clone())),
        "doc": u.doc,
        "local": u.id.as_ref().map(|_| u.local.clone()),
        "kind": u.kind,
    })
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
    units: &[CoverageUnit],
) -> String {
    use serde_json::{json, Value};

    let tests: Vec<Value> = results
        .iter()
        .map(|r| {
            let expectations: Vec<Value> = r
                .expectations
                .iter()
                .map(|e| {
                    let mut v = json!({
                        "kind": e.kind,
                        "subject": e.subject,
                        "expected": e.expected,
                        "actual": e.actual,
                        "passed": e.passed,
                    });
                    // dsl 0.27.0 §4 (HW27-04): an `eligible` miss because
                    // the engine would not raise the beat's occasion.
                    if let Some(nr) = e.not_raised.as_ref().filter(|_| !e.passed) {
                        v["notRaised"] = json!(nr);
                    }
                    v
                })
                .collect();
            json!({
                "test": r.test_file.display().to_string(),
                "kind": r.kind,
                "file": r.lute_file,
                "exit": r.exit,
                "end": r.end,
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
        // Keyed by the printed site (the first spelling seen), as before.
        let choices: serde_json::Map<String, Value> = cov
            .choices
            .values()
            .map(|row| {
                let (chosen, total) = (&row.chosen, row.total);
                let never_named: Vec<&String> = row.eligible.difference(chosen).collect();
                let unseen = total.saturating_sub(chosen.len() + never_named.len());
                (
                    row.site.clone(),
                    json!({
                        "label": row.label,
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
            .map(|(key, row)| {
                (
                    key.clone(),
                    json!({
                        "kind": if row.guard { "guard" } else { "match" },
                        "label": row.label,
                        "total": row.total,
                        "executed": row.chosen.iter().cloned().collect::<Vec<_>>(),
                        "unexecuted": row.total.saturating_sub(row.chosen.len()),
                    }),
                )
            })
            .collect();
        root["coverage"] = json!({
            "tracedPaths": cov.paths,
            "plays": cov.plays,
            "choices": Value::Object(choices),
            "arms": Value::Object(arms),
            "root": display_path(cov_root),
            // T3-20: the units (whole documents, bundle beats, entries) no
            // test and no play presented.
            "untested": units
                .iter()
                .filter(|u| !u.covered(cov))
                .map(unit_json)
                .collect::<Vec<_>>(),
            // T3-20: the beats (anything answering an occasion) a test
            // traces but no play presented — an untested beat is listed
            // once, under `untested`, as the text does (OT-F-14).
            "notPresentedByPlay": units
                .iter()
                .filter(|u| u.beat && u.covered(cov) && !u.presented_by_play(cov))
                .map(unit_json)
                .collect::<Vec<_>>(),
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
