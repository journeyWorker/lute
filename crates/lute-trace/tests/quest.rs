//! The quest walk — events, monotonic objectives, fail precedence (dsl
//! 0.4.0 §4.4, Task 20). Fixtures: `docs/examples/quest-rescue-halsin.lute`
//! (the §4.6 worked example, adapted per its own header comment) for the
//! event/objective/completion/fail-precedence mechanics (a)-(c); a small
//! synthetic quest for the pre-event-snapshot mechanism (d), which
//! `quest-rescue-halsin.lute` doesn't exercise (it has only one `<on>` per
//! event).
//!
//! Harness mirrors `tests/walk.rs`'s `input_for`/`load_input` idiom (each
//! integration-test binary carries its own small copy — the house
//! precedent `tests/mock.rs`/`tests/walk.rs` already both follow).

use std::path::Path;

use lute_check::{CheckInput, Mode};
use lute_trace::{trace_document, Decision, MockSet, Step, TraceExit, UnresolvedEntry};

/// Assemble a [`CheckInput`] for `text` exactly as `lute check`/`lute
/// compile`/`lute trace` do (no `--project`; `base` resolves any `uses:`/
/// `components:` relative paths).
fn input_for(text: &str, uri: &str, base: &Path) -> CheckInput {
    let (doc, parse_diags) = lute_syntax::parse(text);
    assert!(parse_diags.is_empty(), "fixture must parse clean: {parse_diags:?}");
    let (meta0, _) = lute_check::parse_meta(&doc.meta, &lute_manifest::snapshot::CapabilitySnapshot::default());
    let (snapshot, _) =
        lute_manifest::project::resolve_document_snapshot(None, meta0.profile.as_deref(), &meta0.plugins);
    let imports = lute_check::resolve_imports(base, &meta0.uses, &meta0.extends, doc.meta.span);
    let components = lute_check::resolve_components(base, &meta0.components, doc.meta.span);
    CheckInput {
        text: text.to_string(),
        uri: uri.to_string(),
        snapshot,
        providers: lute_manifest::provider::ProviderSet::default(),
        mode: Mode::Ci,
        imports,
        components,
    }
}

/// Load and assemble a real `docs/examples/*.lute` fixture by path
/// (relative to this crate's `Cargo.toml`, the house `../../docs/examples/…`
/// idiom).
fn load_input(path: &str) -> CheckInput {
    let file = Path::new(path);
    let text = std::fs::read_to_string(file).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let base = file.parent().unwrap_or_else(|| Path::new("."));
    input_for(&text, path, base)
}

/// rescueHalsin activates DECLARATIVELY (`start="holds(inParty(shadowheart))"`,
/// dsl 0.4.0 §4.4) — `questActive` fires automatically from that ONE
/// transition, so no `--event questActive` is supplied (that lifecycle name
/// is now `E-TRACE-EVENT`-rejected pre-walk, §4.3).
fn quest_facts(facts: &[&str]) -> MockSet {
    MockSet {
        facts: facts.iter().map(|f| f.to_string()).collect(),
        ..Default::default()
    }
}

fn assert_complete(exit: &TraceExit) {
    assert!(matches!(exit, TraceExit::Complete), "expected Complete, got {exit:?}");
}

fn assert_incomplete(exit: &TraceExit) {
    assert!(matches!(exit, TraceExit::Incomplete), "expected Incomplete, got {exit:?}");
}

fn quest_decision<'a>(decisions: &'a [Decision], outcome: &str) -> Option<&'a Decision> {
    decisions.iter().find(|d| d.construct == "quest" && d.outcome == outcome)
}

fn has_step_assert(steps: &[Step], text: &str) -> bool {
    steps.iter().any(|s| matches!(s, Step::Assert { text: t } if t == text))
}

fn count_step_assert(steps: &[Step], text: &str) -> usize {
    steps.iter().filter(|s| matches!(s, Step::Assert { text: t } if t == text)).count()
}

fn count_quest_decisions(decisions: &[Decision], id: &str, outcome: &str) -> usize {
    decisions
        .iter()
        .filter(|d| d.construct == "quest" && d.id == id && d.outcome == outcome)
        .count()
}

fn has_step_retract(steps: &[Step], text: &str) -> bool {
    steps.iter().any(|s| matches!(s, Step::Retract { text: t } if t == text))
}

fn has_step_set(steps: &[Step], path: &str, value: &str) -> bool {
    steps
        .iter()
        .any(|s| matches!(s, Step::Set { path: p, value: v, .. } if p == path && v == value))
}

fn has_line_containing(steps: &[Step], needle: &str) -> bool {
    steps.iter().any(|s| matches!(s, Step::Line { text, .. } if text.contains(needle)))
}

fn objective_unresolved<'a>(unresolved: &'a [UnresolvedEntry], id: &str) -> Option<&'a UnresolvedEntry> {
    unresolved.iter().find(|u| u.construct == "objective" && u.id == id)
}

// ---------------------------------------------------------------------
// (a) The §4.6 quest transcript: start decides true -> active, the
//     `<on questActive>` handler's assert lands, `learn`'s `done` (an
//     unsupplied `derive:true` relation) goes unknown -> exit 3, naming
//     `believesLocation` + the mock hint.
// ---------------------------------------------------------------------

#[test]
fn quest_activates_and_halts_on_unresolved_derived_objective() {
    let input = load_input("../../docs/examples/quest-rescue-halsin.lute");
    let mocks = quest_facts(&["inParty(shadowheart)"]);

    let (report, exit) = trace_document(&input, mocks);
    assert_incomplete(&exit);

    // `start` decided true -> active, recorded EXACTLY once (the
    // declarative path activates at most one time per quest).
    let active = quest_decision(&report.decisions, "active").expect("quest must record an active decision");
    assert_eq!(active.id, "rescueHalsin");
    assert_eq!(
        count_quest_decisions(&report.decisions, "rescueHalsin", "active"),
        1,
        "activation must be recorded exactly once: {:?}",
        report.decisions
    );

    // The `<on event="questActive">` handler fired: its `::assert` landed
    // in the transcript — EXACTLY ONCE. Regression guard for the historical
    // double-fire (declarative auto-activation dispatching `questActive`,
    // then a `--event questActive` re-dispatching it a second time): that
    // event name is now `E-TRACE-EVENT`-rejected pre-walk (§4.3), so the
    // ONE call site in `walk_quest` is the only place it can ever fire.
    assert_eq!(
        count_step_assert(&report.steps, "heardLocation(player, halsin, grove)"),
        1,
        "questActive handler's assert must fire EXACTLY ONCE (double-fire regression): {:?}",
        report.steps
    );

    // `learn`'s `done` (`holds(believesLocation(...))`, `derive:true`,
    // never supplied) is unresolved, naming `believesLocation` and the
    // §4.6 "supply it as a mock" hint.
    let learn = objective_unresolved(&report.unresolved, "learn").expect("`learn` must be unresolved");
    assert!(
        learn.atoms.iter().any(|a| a.contains("believesLocation") && a.contains("--fact")),
        "unresolved atoms must name believesLocation + the mock hint: {:?}",
        learn.atoms
    );

    // Never reaches completion or failure.
    assert!(quest_decision(&report.decisions, "complete").is_none());
    assert!(quest_decision(&report.decisions, "failed").is_none());
}

// ---------------------------------------------------------------------
// (b) Supplying both derived facts completes the quest: exit 0, state
//     complete, `questComplete` handlers' writes visible.
// ---------------------------------------------------------------------

#[test]
fn supplying_derived_facts_completes_the_quest() {
    let input = load_input("../../docs/examples/quest-rescue-halsin.lute");
    let mocks = quest_facts(&[
        "inParty(shadowheart)",
        "believesLocation(player, halsin, grove)",
        "canReach(player, grove)",
    ]);

    let (report, exit) = trace_document(&input, mocks);
    assert_complete(&exit);
    assert!(report.unresolved.is_empty(), "a complete trace has nothing left unresolved: {:?}", report.unresolved);
    assert!(
        report.notes.is_empty(),
        "§3.1 note must not fire once `--fact` mocks are supplied: {:?}",
        report.notes
    );

    let complete = quest_decision(&report.decisions, "complete").expect("quest must record a complete decision");
    assert_eq!(complete.id, "rescueHalsin");

    // Both objectives recorded done.
    let reach_done = report
        .decisions
        .iter()
        .any(|d| d.construct == "objective" && d.id == "reach" && d.outcome == "done");
    let learn_done = report
        .decisions
        .iter()
        .any(|d| d.construct == "objective" && d.id == "learn" && d.outcome == "done");
    assert!(reach_done, "reach must be recorded done: {:?}", report.decisions);
    assert!(learn_done, "learn must be recorded done: {:?}", report.decisions);

    // `questComplete`'s writes are visible in the transcript.
    assert!(has_step_retract(&report.steps, "captive(halsin)"), "retract must be visible: {:?}", report.steps);
    assert!(has_step_assert(&report.steps, "inParty(halsin)"), "assert must be visible: {:?}", report.steps);
    assert!(has_step_set(&report.steps, "user.xp", "300"), "set must be visible: {:?}", report.steps);
}

// ---------------------------------------------------------------------
// (c) Fail precedence (`0.2 §6.3`): a seeded `atLocation(halsin,
//     moonrise)` fails the quest EVEN THOUGH both objectives would
//     otherwise complete.
// ---------------------------------------------------------------------

#[test]
fn fail_takes_precedence_over_derived_completion() {
    let input = load_input("../../docs/examples/quest-rescue-halsin.lute");
    let mocks = quest_facts(&[
        "inParty(shadowheart)",
        "believesLocation(player, halsin, grove)",
        "canReach(player, grove)",
        "atLocation(halsin, moonrise)",
    ]);

    let (report, exit) = trace_document(&input, mocks);
    assert_complete(&exit); // every guard fully decided; the quest itself failed

    let failed = quest_decision(&report.decisions, "failed").expect("quest must record a failed decision");
    assert_eq!(failed.id, "rescueHalsin");

    // Completion never fires, even though both objectives are done.
    assert!(
        quest_decision(&report.decisions, "complete").is_none(),
        "fail must PREEMPT completion, never both: {:?}",
        report.decisions
    );
    assert!(!has_step_retract(&report.steps, "captive(halsin)"), "questComplete must never fire");
    assert!(!has_step_set(&report.steps, "user.xp", "300"), "questComplete's ::set must never fire");
}

// ---------------------------------------------------------------------
// (d) Pre-event snapshot: an `<on>` guard reading a path an EARLIER
//     `<on>` handler for the SAME event just wrote sees the OLD (pre-
//     event) value, never that in-flow write.
// ---------------------------------------------------------------------

fn pre_event_snapshot_fixture() -> String {
    "---\n\
     kind: quest\n\
     luteVersion: \"0.4.0\"\n\
     title: Pre-event snapshot\n\
     state:\n  \
       run.flag: { type: bool, default: true }\n\
     ---\n\n\
     <quest id=\"q\" title=\"Q\" start=\"true\">\n\
     <on event=\"questActive\">\n\
     ::set{ run.flag = false }\n\
     </on>\n\
     <on event=\"questActive\" when=\"run.flag\">\n\
     @narrator: saw flag\n\
     </on>\n\
     </quest>\n"
        .to_string()
}

#[test]
fn on_guard_reads_pre_event_snapshot_not_a_sibling_arms_write() {
    let text = pre_event_snapshot_fixture();
    let input = input_for(&text, "pre_event_snapshot.lute", Path::new("."));

    let (report, exit) = trace_document(&input, MockSet::default());
    assert_complete(&exit);

    // The first handler's write landed.
    assert!(has_step_set(&report.steps, "run.flag", "false"), "the first handler's ::set must land: {:?}", report.steps);

    // The second handler's guard (`when="run.flag"`) fired using the
    // PRE-EVENT value (schema default `true`) — NOT the sibling arm's
    // live write. If the walk wrongly used live state, this line would
    // never appear.
    assert!(
        has_line_containing(&report.steps, "saw flag"),
        "the second handler must fire on the OLD (pre-event) value: {:?}",
        report.steps
    );
    let on_decisions: Vec<&Decision> = report.decisions.iter().filter(|d| d.construct == "on").collect();
    assert!(
        on_decisions.iter().any(|d| d.outcome == "fires"),
        "at least one on-decision must record a fire: {on_decisions:?}"
    );
}

// ---------------------------------------------------------------------
// (e) Two-path activation (dsl 0.4.0 §4.4): a `start`-less quest is
//     ACCEPT-DRIVEN — it stays inactive ("awaiting accept", exit 0, NOT
//     unresolved/exit-3) until a matching `--accept <questId>` activates
//     it, firing `questActive` EXACTLY ONCE.
// ---------------------------------------------------------------------

fn accept_driven_fixture() -> String {
    "---\n\
     kind: quest\n\
     luteVersion: \"0.4.0\"\n\
     title: Accept-driven quest\n\
     ---\n\n\
     <quest id=\"sideQuest\" title=\"Side Quest\">\n\
     <on event=\"questActive\">\n\
     @narrator: accepted!\n\
     </on>\n\
     </quest>\n"
        .to_string()
}

#[test]
fn start_less_quest_stays_inactive_without_accept_exit_zero_not_incomplete() {
    let text = accept_driven_fixture();
    let input = input_for(&text, "accept_driven.lute", Path::new("."));

    let (report, exit) = trace_document(&input, MockSet::default());
    // Awaiting accept is NOT an unknown/halt condition — the walk
    // completes cleanly, exit 0, never exit 3.
    assert_complete(&exit);

    let awaiting = quest_decision(&report.decisions, "awaiting accept")
        .expect("a start-less, unaccepted quest must report awaiting-accept");
    assert_eq!(awaiting.id, "sideQuest");
    assert!(quest_decision(&report.decisions, "active").is_none(), "must never activate without --accept");
    assert!(!has_line_containing(&report.steps, "accepted!"), "questActive must never fire without activation");
}

#[test]
fn start_less_quest_activates_on_matching_accept_questactive_fires_once() {
    let text = accept_driven_fixture();
    let input = input_for(&text, "accept_driven.lute", Path::new("."));
    let mocks = MockSet { accepts: vec!["sideQuest".to_string()], ..Default::default() };

    let (report, exit) = trace_document(&input, mocks);
    assert_complete(&exit);

    // Activated exactly once via the accept-driven path.
    let active = quest_decision(&report.decisions, "active").expect("`--accept` must activate the quest");
    assert_eq!(active.id, "sideQuest");
    assert_eq!(
        count_quest_decisions(&report.decisions, "sideQuest", "active"),
        1,
        "accept-driven activation must be recorded exactly once: {:?}",
        report.decisions
    );
    assert!(quest_decision(&report.decisions, "awaiting accept").is_none());

    // `questActive` fired from the ONE activation, exactly once (double-fire
    // regression guard for the accept-driven path).
    let fire_count = report.steps.iter().filter(|s| matches!(s, Step::Line { text, .. } if text.contains("accepted!"))).count();
    assert_eq!(fire_count, 1, "questActive handler must fire exactly once: {:?}", report.steps);
}

// ---------------------------------------------------------------------
// (f) §3.2 terminal quests: a quest driven to FAILED (fail-precedence)
//     with objectives whose `done` stays UNKNOWN this same pass (recorded
//     by `reevaluate_objectives` BEFORE `fail` is checked, `0.2 §6.3`
//     ordering) must not keep the walk unresolved — once terminal, an
//     objective "can no longer affect any outcome" (§3.2).
// ---------------------------------------------------------------------

#[test]
fn terminal_failed_quest_purges_unresolved_objectives() {
    let input = load_input("../../docs/examples/quest-rescue-halsin.lute");
    // `atLocation(halsin, moonrise)` seeds `fail` true; neither `canReach`
    // nor `believesLocation` is supplied, so BOTH objectives' `done` guards
    // go unknown on this same settle pass, before `fail` decides.
    let mocks = quest_facts(&["inParty(shadowheart)", "atLocation(halsin, moonrise)"]);

    let (report, exit) = trace_document(&input, mocks);
    assert_complete(&exit); // terminal (failed) -> NOT incomplete/exit 3

    let failed = quest_decision(&report.decisions, "failed").expect("quest must record a failed decision");
    assert_eq!(failed.id, "rescueHalsin");

    assert!(
        report.unresolved.is_empty(),
        "a terminally-failed quest's unresolved objectives must not survive: {:?}",
        report.unresolved
    );
    assert!(objective_unresolved(&report.unresolved, "reach").is_none());
    assert!(objective_unresolved(&report.unresolved, "learn").is_none());
}

// ---------------------------------------------------------------------
// (g) §3.2 de-duplication: an objective whose `done` stays unknown across
//     MULTIPLE settle passes on a quest that never reaches a terminal
//     state is reported unresolved exactly ONCE, not once per pass.
// ---------------------------------------------------------------------

#[test]
fn nonterminal_quest_dedupes_unresolved_objective_across_passes() {
    let input = load_input("../../docs/examples/quest-rescue-halsin.lute");
    // No `atLocation` fact -> `fail` stays unknown, never true -> the quest
    // stays Active across both custom (non-lifecycle) events, re-running
    // `settle_quest`/`reevaluate_objectives` on `reach`/`learn` each time.
    let mocks = MockSet {
        facts: vec!["inParty(shadowheart)".to_string()],
        events: vec!["poke1".to_string(), "poke2".to_string()],
        ..Default::default()
    };

    let (report, exit) = trace_document(&input, mocks);
    assert_incomplete(&exit); // still genuinely unresolved (not terminal) -> exit 3 preserved

    assert!(quest_decision(&report.decisions, "failed").is_none());
    assert!(quest_decision(&report.decisions, "complete").is_none());

    let reach_count = report.unresolved.iter().filter(|u| u.construct == "objective" && u.id == "reach").count();
    let learn_count = report.unresolved.iter().filter(|u| u.construct == "objective" && u.id == "learn").count();
    assert_eq!(reach_count, 1, "`reach` must be reported unresolved exactly once, not once per pass: {:?}", report.unresolved);
    assert_eq!(learn_count, 1, "`learn` must be reported unresolved exactly once, not once per pass: {:?}", report.unresolved);
}

// ---------------------------------------------------------------------
// (h) §3.1: the schema declares seed `facts:` (`act1.schema.yaml`, imported
//     via `uses:`) but NO `--fact` mock is supplied at all — trace signals
//     the explicit-world model with an informational note naming a
//     declared-but-un-supplied seeded relation. Nothing about the walk
//     itself changes: a non-`derive:true` relation with zero asserted
//     facts DECIDES `false` (`FactStore::holds`, closed-world over the
//     explicit set), so `start` (reading the unsupplied `inParty` seed)
//     decides `false` -> the quest never activates -> exit 0, EXACTLY as
//     it did before this note existed — the note is purely informational
//     signage explaining why a schema-seeded relation reads empty here.
// ---------------------------------------------------------------------

#[test]
fn declares_seed_facts_with_no_mocks_emits_not_auto_loaded_note() {
    let input = load_input("../../docs/examples/quest-rescue-halsin.lute");
    let mocks = MockSet::default();

    let (report, exit) = trace_document(&input, mocks);

    assert_eq!(report.notes.len(), 1, "expected exactly one §3.1 note: {:?}", report.notes);
    let note = &report.notes[0];
    assert!(
        note.to_lowercase().contains("not auto-load"),
        "note must state schema seed facts are not auto-loaded: {note}"
    );
    assert!(note.contains("--fact"), "note must say --fact supplies them: {note}");
    assert!(
        ["inParty", "captive", "atLocation", "connected"].iter().any(|r| note.contains(r)),
        "note must name a declared seed relation: {note}"
    );

    // Fact set / exit code are UNCHANGED vs before this note existed:
    // nothing was auto-loaded (0 seeded facts, same banner as always), so
    // `start` decides `false` off the genuinely-empty explicit set and the
    // quest never activates — informational only, never a reachability
    // claim, never touching the exit-code decision.
    assert_eq!(report.seeds.facts, 0, "the seeded-count banner reflects supplied mocks only, unaffected by the note");
    assert_complete(&exit);
    assert!(report.unresolved.is_empty(), "the note must never manufacture an unresolved atom: {:?}", report.unresolved);
    let never = quest_decision(&report.decisions, "never").expect("start decides false off the empty explicit set");
    assert_eq!(never.id, "rescueHalsin");
}

// ---------------------------------------------------------------------
// (i) §3.1 regression: an UNRELATED `--fact` (a real, declared relation
//     the schema just never seeded) must NOT silence the note — "none
//     were supplied" means none of the DECLARED SEED tuples, not merely
//     "the mock list happens to be non-empty". `heardLocation` is a
//     declared `act1.schema.yaml` relation but is NOT among its seeded
//     `facts:` (only `inParty`/`captive`/`atLocation`/`connected`×2 are).
// ---------------------------------------------------------------------

#[test]
fn unrelated_supplied_fact_does_not_silence_the_note() {
    let input = load_input("../../docs/examples/quest-rescue-halsin.lute");
    let mocks = quest_facts(&["heardLocation(player, halsin, grove)"]);

    let (report, _exit) = trace_document(&input, mocks);

    assert_eq!(
        report.notes.len(),
        1,
        "an unrelated supplied fact must not silence the §3.1 note (zero DECLARED SEEDS were supplied): {:?}",
        report.notes
    );
    let note = &report.notes[0];
    assert!(
        ["inParty", "captive", "atLocation", "connected"].iter().any(|r| note.contains(r)),
        "note must name a declared-but-unsupplied seed relation, not the unrelated one: {note}"
    );
}
