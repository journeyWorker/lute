//! The scene walk + output contract (dsl 0.4.0 §4.4/§4.5, Task 19): the 8
//! named fixtures against `docs/examples/choice-persist.lute` (the §4.6
//! worked example) plus synthetic inline documents for the mechanisms
//! `choice-persist.lute` itself doesn't exercise (a guarded branch choice,
//! an unseeded match subject, a `<hub>`).
//!
//! Fixture assembly mirrors `tests/mock.rs`'s `folded_and_doc`/`load` idiom
//! (itself `lute-compile/tests/e2e.rs`'s `input_for`), stopping one step
//! earlier: `trace_document` owns `check`/parse/fold/normalize/expand
//! itself, so a test here only needs a [`CheckInput`].

use std::collections::BTreeMap;
use std::path::Path;

use lute_check::{CheckInput, Mode};
use lute_core_span::Span;
use lute_trace::{trace_document, MockSet, TraceExit};

fn zero_span() -> Span {
    Span { byte_start: 0, byte_end: 0, line: 0, column: 0, utf16_range: (0, 0) }
}

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
/// (relative to this crate's `Cargo.toml`, the house
/// `../../docs/examples/…` idiom).
fn load_input(path: &str) -> CheckInput {
    let file = Path::new(path);
    let text = std::fs::read_to_string(file).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let base = file.parent().unwrap_or_else(|| Path::new("."));
    input_for(&text, path, base)
}

fn choose(pairs: &[(&str, &[&str])]) -> MockSet {
    let mut choose = BTreeMap::new();
    for (id, cids) in pairs {
        choose.insert(id.to_string(), cids.iter().map(|s| s.to_string()).collect());
    }
    MockSet { choose, ..Default::default() }
}

fn assert_complete(exit: &TraceExit) {
    assert!(matches!(exit, TraceExit::Complete), "expected Complete, got {exit:?}");
}

fn assert_refused(exit: &TraceExit) -> &[lute_core_span::Diagnostic] {
    match exit {
        TraceExit::Refused(ds) => ds,
        other => panic!("expected Refused, got {other:?}"),
    }
}

// ---------------------------------------------------------------------
// 1. auto_pick_and_forced_pick — §4.6 worked example, both selection modes.
// ---------------------------------------------------------------------

#[test]
fn auto_pick_and_forced_pick() {
    let input = load_input("../../docs/examples/choice-persist.lute");

    // No --choose: `help` auto-picked (first choice, doc order — none of
    // the 3 choices carry a `when=` guard, so all are trivially eligible);
    // Shot 2's match fires arm 1 since the persist sugar wrote
    // `run.metHelpfully = true`.
    let (report, exit) = trace_document(&input, MockSet::default());
    assert_complete(&exit);
    let branch = report
        .decisions
        .iter()
        .find(|d| d.construct == "branch")
        .expect("a branch decision was recorded");
    assert_eq!(branch.outcome, "help");
    assert!(branch.auto, "no --choose -> auto pick: {branch:?}");
    assert!(!branch.forced);
    let m = report
        .decisions
        .iter()
        .find(|d| d.construct == "match")
        .expect("a match decision was recorded");
    assert_eq!(m.outcome, "arm 1");

    // `--choose sofaHelp=help`: every choice is eligible (none guarded);
    // Shot 2 still takes arm 1; coverage "choices 1/3, arms 1/2".
    let mocks = choose(&[("sofaHelp", &["help"])]);
    let (report, exit) = trace_document(&input, mocks);
    assert_complete(&exit);
    let branch = report.decisions.iter().find(|d| d.construct == "branch").expect("branch decision");
    assert_eq!(branch.outcome, "help");
    assert!(!branch.auto && !branch.forced);
    assert_eq!(branch.eligible, vec!["help".to_string(), "warmly".to_string(), "tip".to_string()]);
    let m = report.decisions.iter().find(|d| d.construct == "match").expect("match decision");
    assert_eq!(m.outcome, "arm 1");
    let choices_cov = report.coverage.choices.get("sofaHelp").expect("branch coverage entry");
    assert_eq!((choices_cov.visited, choices_cov.total), (1, 3));
    let arms_cov = report.coverage.arms.get("run.metHelpfully").expect("match coverage entry");
    assert_eq!((arms_cov.visited, arms_cov.total), (1, 2));
}

// ---------------------------------------------------------------------
// 2. presentation_point_eligibility — the §4.4 wrongly-refused guard.
// ---------------------------------------------------------------------

#[test]
fn presentation_point_eligibility() {
    // `run.flag` has NO default (genuinely unset until written); a choice
    // guarded `when="run.flag"` is enabled ONLY by the in-flow `::set` that
    // runs immediately before the `<branch>` in the SAME shot. Auto-pick
    // must land on `a`, proving eligibility was evaluated AFTER that write
    // — reading only the mock seed (unset, no default) would see `unknown`
    // and wrongly skip it.
    let text = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  run.flag: { type: bool }\n---\n\
                ## Shot 1.\n\
                ::set{run.flag = true}\n\
                <branch id=\"b\">\n\
                <choice id=\"a\" label=\"A\" when=\"run.flag\">\n@narrator: fired a\n</choice>\n\
                <choice id=\"other\" label=\"Other\">\n@narrator: fired other\n</choice>\n\
                </branch>\n";
    let input = input_for(text, "presentation-point", Path::new("."));
    let (report, exit) = trace_document(&input, MockSet::default());
    assert_complete(&exit);
    let branch = report.decisions.iter().find(|d| d.construct == "branch").expect("branch decision");
    assert_eq!(branch.outcome, "a", "the in-flow ::set must make `a` eligible: {report:#?}");
    assert!(branch.auto);
}

// ---------------------------------------------------------------------
// 3. forcing_false_guard_is_refused — E-TRACE-CHOICE at the presentation
//    point (§4.4), not merely the structural pre-walk pass (Task 18).
// ---------------------------------------------------------------------

#[test]
fn forcing_false_guard_is_refused() {
    // A `holds(...)` guard is state-DEPENDENT (never a `decide()`-provable
    // constant, §5.1 R5 — decide() reads no facts), so it never trips
    // `E-ARM-DEAD` at check time; with no `--fact` supplied, `claims` (a
    // non-`derive` relation) is DEFINITELY absent — `Bool(false)`, not
    // `unknown` (eval.rs: `non_derived_relation_absent_fact_is_definitely_false`).
    let text = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n\
                entities:\n  character: { members: [halsin] }\n\
                relations:\n  claims: { args: [character] }\n\
                ---\n\
                ## Shot 1.\n\
                <branch id=\"approach\">\n\
                <choice id=\"soft\" label=\"Soft\" when=\"holds(claims(halsin))\">\n@narrator: a\n</choice>\n\
                <choice id=\"blunt\" label=\"Blunt\">\n@narrator: b\n</choice>\n\
                </branch>\n";
    let input = input_for(text, "forced-false", Path::new("."));
    let mocks = choose(&[("approach", &["soft"])]);
    let (_report, exit) = trace_document(&input, mocks);
    let diags = assert_refused(&exit);
    assert!(
        diags.iter().any(|d| d.code == lute_trace::E_TRACE_CHOICE),
        "expected E-TRACE-CHOICE: {diags:?}"
    );
}

// ---------------------------------------------------------------------
// 4. forcing_unknown_guard_is_forced — permitted, annotated `forced`.
// ---------------------------------------------------------------------

#[test]
fn forcing_unknown_guard_is_forced() {
    // `believes` is `derive: true`: with zero supplied facts, `holds()`
    // over it is `unknown` (the rules are never run, §4.2 rule 3) — never
    // `E-MAYBE-UNSET` (that check is state-PATH-only, never fact queries).
    let text = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n\
                entities:\n  character: { members: [halsin] }\n\
                relations:\n  believes: { args: [character], derive: true }\n\
                ---\n\
                ## Shot 1.\n\
                <branch id=\"approach\">\n\
                <choice id=\"soft\" label=\"Soft\" when=\"holds(believes(halsin))\">\n@narrator: a\n</choice>\n\
                <choice id=\"blunt\" label=\"Blunt\">\n@narrator: b\n</choice>\n\
                </branch>\n";
    let input = input_for(text, "forced-unknown", Path::new("."));
    let mocks = choose(&[("approach", &["soft"])]);
    let (report, exit) = trace_document(&input, mocks);
    assert_complete(&exit);
    let branch = report.decisions.iter().find(|d| d.construct == "branch").expect("branch decision");
    assert_eq!(branch.outcome, "soft");
    assert!(branch.forced, "forcing past an unresolved guard must be annotated forced: {branch:?}");
    assert!(!branch.auto);
}

// ---------------------------------------------------------------------
// 5. unknown_match_guard_halts_exit3 — trace never guesses.
// ---------------------------------------------------------------------

#[test]
fn unknown_match_guard_halts_exit3() {
    let text = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  run.y: { type: string }\n---\n\
                ## Shot 1.\n\
                <match on=\"run.y\">\n\
                <when test=\"$ == 'x'\">\n@narrator: matched\n</when>\n\
                <otherwise>\n@narrator: nope\n</otherwise>\n\
                </match>\n";
    let input = input_for(text, "unknown-match", Path::new("."));
    let (report, exit) = trace_document(&input, MockSet::default());
    assert!(matches!(exit, TraceExit::Incomplete), "expected Incomplete (exit 3), got {exit:?}");
    assert_eq!(exit.code(), 3);
    assert!(!report.unresolved.is_empty(), "unresolved[] must name the halted path");
    let u = &report.unresolved[0];
    assert_eq!(u.id, "run.y");
    assert!(
        u.atoms.iter().any(|a| a.contains("run.y")),
        "the unresolved hint must name `run.y` so a mock can resolve it: {:?}",
        u.atoms
    );
    // The trace never guessed past the unknown guard: no match decision
    // (arm 1 fire/skip) was recorded.
    assert!(report.decisions.iter().all(|d| d.construct != "match"));
}

// ---------------------------------------------------------------------
// 6. no_arm_match_reports_and_continues — the §4.4 fourth outcome: every
//    arm's guard decides `false` (none `unknown`), no `<otherwise>` ->
//    "no arm" is annotated in the transcript, coverage is 0/total, and
//    the walk CONTINUES (Complete/exit 0) — contrast with section 5's
//    `unknown`-guard HALT (Incomplete/exit 3): same "nothing fired"
//    shape, opposite exit because one is knowledge and the other isn't.
// ---------------------------------------------------------------------

#[test]
fn no_arm_match_reports_and_continues() {
    // `run.flag`'s bool domain ({true, false}) is fully covered by the two
    // `is=` arms (check-clean: no `E-NONEXHAUSTIVE`/`E-UNSET-UNCOVERED`,
    // `default: false` means never `unset`). Each arm's `test` guard reads
    // a non-`derive` relation with zero supplied facts — DEFINITELY
    // `Bool(false)` (never `unknown`, same idiom as
    // `forcing_false_guard_is_refused` above), so `decide()` never trips
    // `E-ARM-DEAD` at check time (state/fact-dependent, not a constant)
    // yet both arms decide `false` at THIS walk: `is="true"` short-circuits
    // false against the `false` subject, `is="false" && test` is
    // `false && false = false`. No arm fires, no `<otherwise>` — the §4.4
    // fourth outcome.
    let text = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n\
                state:\n  run.flag: { type: bool, default: false }\n\
                entities:\n  character: { members: [halsin] }\n\
                relations:\n  claims: { args: [character] }\n\
                ---\n\
                ## Shot 1.\n\
                <match on=\"run.flag\">\n\
                <when is=\"true\" test=\"holds(claims(halsin))\">\n@narrator: true-gate\n</when>\n\
                <when is=\"false\" test=\"holds(claims(halsin))\">\n@narrator: false-gate\n</when>\n\
                </match>\n";
    let input = input_for(text, "no-arm-match", Path::new("."));
    let (report, exit) = trace_document(&input, MockSet::default());

    // (a) the no-arm match itself never forces an incomplete/refused exit.
    assert_complete(&exit);
    assert_eq!(exit.code(), 0);

    // (b) the transcript/report carries the "no arm" decision, at the
    // match's own span, for this exact subject.
    let m = report
        .decisions
        .iter()
        .find(|d| d.construct == "match")
        .expect("a \"no arm\" match decision must be recorded");
    assert_eq!(m.id, "run.flag");
    assert_eq!(m.outcome, "no arm");
    assert!(
        report.steps.iter().any(|s| matches!(s, lute_trace::Step::Decision(d) if d.outcome == "no arm")),
        "the \"no arm\" outcome must also appear inline in the transcript: {:?}",
        report.steps
    );

    // (c) coverage: no arm was visited out of the 2 declared.
    let arms_cov = report.coverage.arms.get("run.flag").expect("match coverage entry");
    assert_eq!((arms_cov.visited, arms_cov.total), (0, 2));
}

// ---------------------------------------------------------------------
// 7. hub_reevaluates_between_picks — re-evaluation + once-drops-out.
// ---------------------------------------------------------------------

fn hub_fixture() -> String {
    "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  run.flag: { type: bool, default: false }\n---\n\
     ## Shot 1.\n\
     <hub id=\"h\">\n\
     <choice id=\"c1\" label=\"C1\" once>\n@narrator: c1 fires\n::set{run.flag = true}\n</choice>\n\
     <choice id=\"c2\" label=\"C2\" when=\"run.flag\">\n@narrator: c2 fires\n</choice>\n\
     <choice id=\"leave\" label=\"Leave\" exit>\n@narrator: bye\n</choice>\n\
     </hub>\n"
        .to_string()
}

#[test]
fn hub_reevaluates_between_picks() {
    let text = hub_fixture();

    // c1's ::set enables c2's `when="run.flag"` -> honored when re-checked
    // immediately before c2's presentation point.
    let input = input_for(&text, "hub-reeval", Path::new("."));
    let mocks = choose(&[("h", &["c1", "c2", "leave"])]);
    let (report, exit) = trace_document(&input, mocks);
    assert_complete(&exit);
    let picks: Vec<&str> = report
        .decisions
        .iter()
        .filter(|d| d.construct == "hub")
        .map(|d| d.outcome.as_str())
        .collect();
    assert_eq!(picks, vec!["c1", "c2", "leave"], "c2 must have been honored after c1's write: {report:#?}");

    // `c1` is `once`: forcing it a SECOND time (after it already fired) is
    // ineligible at that presentation point -> Refused. (once-arms drop
    // out of subsequent eligibility.)
    let input2 = input_for(&text, "hub-reeval-2", Path::new("."));
    let mocks2 = choose(&[("h", &["c1", "c2", "c1"])]);
    let (_report2, exit2) = trace_document(&input2, mocks2);
    let diags = assert_refused(&exit2);
    assert!(
        diags.iter().any(|d| d.code == lute_trace::E_TRACE_CHOICE),
        "re-forcing a visited `once` choice must be E-TRACE-CHOICE: {diags:?}"
    );
}

// ---------------------------------------------------------------------
// 8. writes_are_sequential — §4.6's second paragraph, verbatim.
// ---------------------------------------------------------------------

#[test]
fn writes_are_sequential() {
    let input = load_input("../../docs/examples/choice-persist.lute");
    let mocks = MockSet {
        state: vec![("run.metHelpfully".to_string(), "true".to_string(), zero_span())],
        choose: BTreeMap::from([("sofaHelp".to_string(), vec!["tip".to_string()])]),
        ..Default::default()
    };
    let (report, exit) = trace_document(&input, mocks);
    assert_complete(&exit);

    // The seeded `true` survives (`tip`'s persist writes run.tip, never
    // run.metHelpfully) — Shot 2 still takes arm 1.
    let m = report.decisions.iter().find(|d| d.construct == "match").expect("match decision");
    assert_eq!(m.outcome, "arm 1");

    // AND the `tip` arm's own persist ::set (`run.tip = 5`) is visible in
    // the transcript — both effects hold at once (sequential in-flow
    // visibility never overwrites the earlier seed).
    let tip_set = report.steps.iter().find_map(|s| match s {
        lute_trace::Step::Set { path, value, sugar } if path == "run.tip" => Some((value.clone(), *sugar)),
        _ => None,
    });
    let (value, sugar) = tip_set.expect("run.tip ::set must appear in the transcript");
    assert_eq!(value, "5");
    assert!(sugar, "the tip persist write must be annotated as sugar");
}

// ---------------------------------------------------------------------
// 9. output_is_byte_deterministic — §4.5's own contract.
// ---------------------------------------------------------------------

#[test]
fn output_is_byte_deterministic() {
    let input = load_input("../../docs/examples/choice-persist.lute");
    let mocks = choose(&[("sofaHelp", &["help"])]);

    let (report1, exit1) = trace_document(&input, mocks.clone());
    let (report2, exit2) = trace_document(&input, mocks);
    assert_eq!(exit1, exit2);
    assert_eq!(exit1.code(), 0);

    let human1 = report1.render_human();
    let human2 = report2.render_human();
    assert_eq!(human1, human2, "render_human must be byte-identical across runs");

    let json1 = report1.render_json();
    let json2 = report2.render_json();
    assert_eq!(json1, json2, "render_json must be byte-identical across runs");

    // Top-level JSON key order is normative (§4.5): file, seeds, steps,
    // decisions, unresolved, coverage — checked as literal byte positions
    // in the SERIALIZED TEXT (not via a re-parsed `serde_json::Value`,
    // whose `Map` may reorder keys).
    let keys = ["\"file\"", "\"seeds\"", "\"steps\"", "\"decisions\"", "\"unresolved\"", "\"coverage\""];
    let mut last = 0usize;
    for k in keys {
        let pos = json1.find(k).unwrap_or_else(|| panic!("missing key {k} in:\n{json1}"));
        assert!(pos >= last, "key {k} out of order in:\n{json1}");
        last = pos;
    }

    // Sanity: the JSON round-trips as a well-formed document.
    let _: serde_json::Value = serde_json::from_str(&json1).expect("render_json must be valid JSON");
    assert!(!matches!(exit1, TraceExit::Refused(_)));
}

// ---------------------------------------------------------------------
// dsl 0.5.1 §1: trace preview of reserved quest-path reads.
// ---------------------------------------------------------------------

/// A scene reading `quest.foo.state` as a `<match>` subject, exhaustive
/// over the reserved domain (`active|complete|failed|unset`) with one arm
/// per literal — no `<otherwise>`, so which arm fires pins down exactly
/// what the subject decided.
fn quest_state_match_text() -> String {
    "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n\
     <match on=\"quest.foo.state\">\n\
     <when is=\"active\">\n@narrator: is-active\n</when>\n\
     <when is=\"complete\">\n@narrator: is-complete\n</when>\n\
     <when is=\"failed\">\n@narrator: is-failed\n</when>\n\
     <when is=\"unset\">\n@narrator: is-unset\n</when>\n\
     </match>\n"
        .to_string()
}

/// A scene reading `quest.foo.objectives.bar.done` as a `<match>` subject,
/// exhaustive over its bool domain.
fn quest_objective_done_match_text() -> String {
    "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n\
     <match on=\"quest.foo.objectives.bar.done\">\n\
     <when is=\"true\">\n@narrator: is-true\n</when>\n\
     <when is=\"false\">\n@narrator: is-false\n</when>\n\
     </match>\n"
        .to_string()
}

fn match_decision<'a>(report: &'a lute_trace::TraceReport, id: &str) -> &'a lute_trace::Decision {
    report
        .decisions
        .iter()
        .find(|d| d.construct == "match" && d.id == id)
        .unwrap_or_else(|| panic!("no match decision for {id}: {:?}", report.decisions))
}

fn state_mocks(pairs: &[(&str, &str)]) -> MockSet {
    MockSet {
        state: pairs.iter().map(|(p, v)| (p.to_string(), v.to_string(), zero_span())).collect(),
        ..Default::default()
    }
}

/// §1.1's own worked example: `--state quest.foo.state=complete` against a
/// scene that reads `quest.foo.state` is ADMITTED (was
/// `E-TRACE-MOCK-UNDECLARED` pre-0.5.1) and previews the `complete` arm.
/// §1.3: the mocked-admission case also gets the existence-unverified
/// note, WITHOUT a "defaults to" clause (nothing was defaulted).
#[test]
fn reserved_quest_state_mock_admitted_and_previews_referenced_arm() {
    let input = input_for(&quest_state_match_text(), "quest-state-match", Path::new("."));
    let mocks = state_mocks(&[("quest.foo.state", "complete")]);
    let (report, exit) = trace_document(&input, mocks);
    assert_complete(&exit);

    let d = match_decision(&report, "quest.foo.state");
    assert_eq!(d.outcome, "arm 2", "{d:?}"); // "complete" is the 2nd `<when>`
    assert_eq!(d.guard.as_deref(), Some("is=\"complete\""), "{d:?}");

    assert!(
        report.notes.iter().any(|n| n.contains("quest `foo`") && n.contains("unverified")),
        "{:?}",
        report.notes
    );
    assert!(
        !report.notes.iter().any(|n| n.contains("defaults to")),
        "a purely-mocked read must not claim a default: {:?}",
        report.notes
    );
}

/// §1.1: `--state` on a reserved path the document does NOT reference
/// stays `E-TRACE-MOCK-UNDECLARED` — Refused, end to end.
#[test]
fn reserved_quest_state_mock_on_unreferenced_path_stays_refused() {
    let input = input_for(&quest_state_match_text(), "quest-state-match", Path::new("."));
    let mocks = state_mocks(&[("quest.bar.state", "complete")]);
    let (_, exit) = trace_document(&input, mocks);
    let diags = assert_refused(&exit);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, "E-TRACE-MOCK-UNDECLARED", "{diags:?}");
}

/// §1.2: an un-mocked, exhaustive `quest.<id>.state` match fires the
/// `unset` arm (its reserved default) rather than halting/"no arm". §1.3:
/// the defaulted-read case gets the existence-unverified note WITH a
/// "defaults to `unset`" clause.
#[test]
fn unmocked_quest_state_match_fires_unset_arm_not_no_arm() {
    let input = input_for(&quest_state_match_text(), "quest-state-match", Path::new("."));
    let (report, exit) = trace_document(&input, MockSet::default());
    assert_complete(&exit);

    let d = match_decision(&report, "quest.foo.state");
    assert_eq!(d.outcome, "arm 4", "{d:?}"); // "unset" is the 4th `<when>`
    assert_eq!(d.guard.as_deref(), Some("is=\"unset\""), "{d:?}");

    let arms_cov = report.coverage.arms.get("quest.foo.state").expect("match coverage entry");
    assert_eq!((arms_cov.visited, arms_cov.total), (1, 4));

    assert!(
        report
            .notes
            .iter()
            .any(|n| n.contains("quest `foo`") && n.contains("unverified") && n.contains("defaults to `unset`")),
        "{:?}",
        report.notes
    );
}

/// §1.2: an un-mocked, exhaustive `quest.<id>.objectives.<oid>.done` match
/// fires the `false` arm (its reserved default) — the exact 0.5.0 "no arm"
/// defect this revision fixes. §1.3: defaulted note names `false`.
#[test]
fn unmocked_objective_done_match_fires_false_arm_not_no_arm() {
    let input = input_for(&quest_objective_done_match_text(), "quest-objective-match", Path::new("."));
    let (report, exit) = trace_document(&input, MockSet::default());
    assert_complete(&exit);

    let d = match_decision(&report, "quest.foo.objectives.bar.done");
    assert_eq!(d.outcome, "arm 2", "{d:?}"); // "false" is the 2nd `<when>`
    assert_eq!(d.guard.as_deref(), Some("is=\"false\""), "{d:?}");

    let arms_cov = report.coverage.arms.get("quest.foo.objectives.bar.done").expect("match coverage entry");
    assert_eq!((arms_cov.visited, arms_cov.total), (1, 2));

    assert!(
        report
            .notes
            .iter()
            .any(|n| n.contains("quest `foo`") && n.contains("unverified") && n.contains("defaults to `false`")),
        "{:?}",
        report.notes
    );
}

// ---------------------------------------------------------------------
// dsl 0.5.1 §4: unmatched `--event` note.
// ---------------------------------------------------------------------

/// A never-completing quest doc (`done` reads a non-derive relation with
/// zero supplied facts — DEFINITELY `false`, state/fact-dependent, so
/// `check` never constant-folds it `E-OBJECTIVE-UNSATISFIABLE`, mirroring
/// `no_arm_match_reports_and_continues`'s idiom) with ONE
/// `<on event="questActive">` handler.
fn never_completing_quest_text() -> String {
    "---\nkind: quest\nrelations:\n  claims: { args: [character] }\n\
     entities:\n  character: { members: [halsin] }\n---\n\
     <quest id=\"q\" start=\"true\">\n\
     <objective id=\"o\" done=\"holds(claims(halsin))\"/>\n\
     <on event=\"questActive\">\n@narrator: hi\n</on>\n\
     </quest>\n"
        .to_string()
}

/// A quest with ONE `<on event="questActive">` handler; `--event
/// nonexistentHandler` matches no `<on>` handler anywhere in the document
/// -> an informational note, exit UNCHANGED (still Complete).
#[test]
fn unmatched_event_emits_note_and_leaves_exit_unchanged() {
    let input = input_for(&never_completing_quest_text(), "unmatched-event", Path::new("."));
    let mocks = MockSet { events: vec!["nonexistentHandler".to_string()], ..Default::default() };
    let (report, exit) = trace_document(&input, mocks);
    assert_complete(&exit);
    assert!(
        report.notes.iter().any(|n| n.contains("event `nonexistentHandler` matched no `<on>` handler")),
        "{:?}",
        report.notes
    );
}

/// A `--event` naming a defined handler (a capability-declared world event,
/// so it is legal both as an `<on>` name AND a `--event` flag — a
/// lifecycle name can never be, `E-TRACE-EVENT`) produces NO
/// unmatched-event note.
#[test]
fn matched_event_emits_no_unmatched_note() {
    let text = "---\nkind: quest\nrelations:\n  claims: { args: [character] }\n\
                entities:\n  character: { members: [halsin] }\n---\n\
                <quest id=\"q\" start=\"true\">\n\
                <objective id=\"o\" done=\"holds(claims(halsin))\"/>\n\
                <on event=\"questActive\">\n@narrator: hi\n</on>\n\
                <on event=\"npcSpoke\">\n@narrator: heard\n</on>\n\
                </quest>\n";
    let mut input = input_for(text, "matched-event", Path::new("."));
    input
        .snapshot
        .events
        .insert("npcSpoke".to_string(), lute_manifest::schema::EventDecl { name: "npcSpoke".to_string() });
    let mocks = MockSet { events: vec!["npcSpoke".to_string()], ..Default::default() };
    let (report, exit) = trace_document(&input, mocks);
    assert_complete(&exit);
    assert!(!report.notes.iter().any(|n| n.contains("matched no")), "{:?}", report.notes);
}

/// Preserved behavior (§4's own text): a `--event` naming a built-in
/// lifecycle event is STILL `E-TRACE-EVENT`, Refused, end to end.
#[test]
fn builtin_lifecycle_event_still_refused_end_to_end() {
    let input = input_for(&never_completing_quest_text(), "lifecycle-event", Path::new("."));
    let mocks = MockSet { events: vec!["questComplete".to_string()], ..Default::default() };
    let (_, exit) = trace_document(&input, mocks);
    let diags = assert_refused(&exit);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, "E-TRACE-EVENT", "{diags:?}");
}
