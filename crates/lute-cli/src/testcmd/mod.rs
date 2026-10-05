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
//! [`lute_load::build_input`] call, same provider-catalog precedence (plugin
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
use lute_load::nearest_manifest_dir;
use lute_model::{ModelDocument, ModelError, ModelOptions, ProjectModel};
use lute_semantic::{NodeKey, NodeKind};

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
mod coverage;
mod discovery;
mod render;
mod runner;

use coverage::{accumulate_coverage, canonical_key, coverage_units, display_path};
use discovery::{closed_key_violations, find_files_with_suffix, fold_parent_dirs};
use render::{render_human, render_json};
use runner::{yaml_atom_hint, yaml_atom_hints, IMPLICIT_ELIGIBLE, IMPLICIT_EXIT};

pub(crate) use discovery::static_context_references;
pub(crate) use runner::{run_test_for_constraint, run_test_for_context};
pub use runner::run_test;

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

/// A play step's keys a `*.test.yaml` writes another way: `(key, how a test
/// says it)`. Both are seeds of the presented raise.
const TEST_SPELLING_OF_STEP_KEYS: &[(&str, &str)] = &[
    (
        "payload",
        "seeds the raise's payload as state, `state: { occasion.payload.<field>: … }`",
    ),
    (
        "target",
        "seeds the raised target as state, `state: { occasion.target: <member> }`",
    ),
];

/// One `E-TEST-KEY` line for an unrecognised key, with the same
/// edit-distance did-you-mean four checker codes already use (dsl 0.5.0
/// §2.2), over the workspace's ONE suggestion helper. `where_` names the
/// level so a top-level typo and an `expect:`-level typo are
/// distinguishable. A renamed key names its new spelling; a key a play
/// script reads at that level says so.

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
    forced_unknown: Vec<UnresolvedEntry>,
    /// Construct/id/outcome triples observed by the trace runner. This is
    /// consumed by task context so a static script reference is not mistaken
    /// for run evidence.
    visited: BTreeSet<String>,
    /// Quest ids observed transitioning to complete during this test trace.
    completed_quests: BTreeSet<String>,
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
            visited: BTreeSet::new(),
            completed_quests: BTreeSet::new(),
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
