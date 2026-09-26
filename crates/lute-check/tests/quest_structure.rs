//! dsl 0.24.0 §2 — quest structure, through the assembled `check()` and the
//! `check-project` passes: `<quest activate complete>` values, `<on
//! target>`, `::accept{at}`, accepting a subquest child
//! (`E-ACCEPT-TARGET`), `W-QUEST-NEVER-ACCEPTED`, and the reserved read-only
//! `quest.<id>.failedBy` / `quest.<id>.objectives.<o>.failed` paths; dsl
//! 0.25.0 §5 `<quest accept="external">`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use lute_check::{
    check, check_project_accepts, check_project_never_accepted, check_project_quest_refs,
    CheckInput, Mode, SchemaImports,
};
use lute_core_span::{Diagnostic, Severity};
use lute_manifest::schema::{EventDecl, OccasionDecl, OccasionSelect, OccasionTarget};
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_syntax::ast::Document;

/// The core snapshot plus an occasion vocabulary — `bossDefeated` raised
/// for one of two bosses, `examine` for any target, `hubVisit` for none —
/// and the world events a handler may name.
fn snapshot() -> CapabilitySnapshot {
    let mut snap = lute_manifest::core::load_core_snapshot();
    let bosses = OccasionTarget::Domain {
        prefix: "boss".into(),
        entity: "foe".into(),
        members: Some(vec!["gatekeeper".into(), "warden".into()]),
    };
    for (name, target) in [
        ("bossDefeated", bosses),
        ("examine", OccasionTarget::Shape(true)),
        ("hubVisit", OccasionTarget::Shape(false)),
    ] {
        snap.occasions.insert(
            name.into(),
            OccasionDecl {
                name: name.into(),
                select: OccasionSelect::First,
                target,
                description: None,
                ..Default::default()
            },
        );
    }
    for name in ["bossDefeated", "examine", "hubVisit", "combatEnd"] {
        snap.events
            .insert(name.into(), EventDecl { name: name.into() });
    }
    snap
}

fn diags(text: &str) -> Vec<Diagnostic> {
    check(&CheckInput {
        text: text.to_string(),
        uri: "quest_structure".into(),
        snapshot: snapshot(),
        providers: Default::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
        defaults: Default::default(),
    })
    .diagnostics
}

fn with_code<'a>(ds: &'a [Diagnostic], code: &str) -> Vec<&'a Diagnostic> {
    ds.iter().filter(|d| d.code == code).collect()
}

fn only<'a>(ds: &'a [Diagnostic], code: &str) -> &'a Diagnostic {
    let hits = with_code(ds, code);
    assert_eq!(hits.len(), 1, "want exactly one {code}: {ds:?}");
    hits[0]
}

fn errors(ds: &[Diagnostic]) -> Vec<&Diagnostic> {
    ds.iter()
        .filter(|d| d.severity == Severity::Error)
        .collect()
}

fn anchored<'s>(src: &'s str, d: &Diagnostic) -> &'s str {
    &src[d.span.byte_start..d.span.byte_end]
}

const VOCAB: &str = "entities:\n  foe: { members: [gatekeeper, warden, cinderhound] }\n";

fn quest_doc(body: &str) -> String {
    format!(
        "---\nkind: quest\n{VOCAB}state:\n  run.d: {{ type: bool, default: false }}\n---\n{body}"
    )
}

fn scene_doc(id: &str, body: &str) -> String {
    format!(
        "---\nkind: scene\nid: {id}\nstate:\n  run.d: {{ type: bool, default: false }}\n---\n\
         ## Shot 1.\n{body}"
    )
}

fn docs(texts: &[(&str, &str)]) -> Vec<(PathBuf, Document)> {
    texts
        .iter()
        .map(|(path, text)| (PathBuf::from(path), lute_syntax::parse(text).0))
        .collect()
}

// --- `<quest activate complete>` ---------------------------------------------

#[test]
fn quest_activate_and_complete_take_their_listed_values() {
    let good = quest_doc(
        "<quest id=\"main\" start=\"true\" complete=\"any\">\n\
         <objective id=\"a\" quest=\"alt\"/>\n<objective id=\"b\" done=\"run.d\"/>\n</quest>\n\
         <quest id=\"alt\" activate=\"accept\" complete=\"all\">\n\
         <objective id=\"o\" done=\"run.d\"/>\n</quest>\n",
    );
    let ds = diags(&good);
    assert!(with_code(&ds, "E-ATTR-TYPE").is_empty(), "{ds:?}");
    assert!(with_code(&ds, "E-UNKNOWN-ATTR").is_empty(), "{ds:?}");

    let bad = quest_doc(
        "<quest id=\"q\" activate=\"later\" complete=\"some\">\n\
         <objective id=\"o\" done=\"run.d\"/>\n</quest>\n",
    );
    let ds = diags(&bad);
    let hits = with_code(&ds, "E-ATTR-TYPE");
    assert_eq!(hits.len(), 2, "{ds:?}");
    let spots: BTreeSet<&str> = hits.iter().map(|d| anchored(&bad, d)).collect();
    assert_eq!(spots, BTreeSet::from(["later", "some"]));
    assert!(hits
        .iter()
        .any(|d| d.message.contains("`activate`") && d.message.contains("\"accept\"")));
    assert!(hits
        .iter()
        .any(|d| d.message.contains("`complete`") && d.message.contains("\"any\"")));
}

#[test]
fn activate_accept_beside_start_is_attr_type() {
    let src = quest_doc(
        "<quest id=\"q\" activate=\"accept\" start=\"run.d\">\n\
         <objective id=\"o\" done=\"run.d\"/>\n</quest>\n",
    );
    let ds = diags(&src);
    let d = only(&ds, "E-ATTR-TYPE");
    assert_eq!(anchored(&src, d), "accept");
    assert!(d.message.contains("`start`"), "{}", d.message);
}

#[test]
fn quest_accept_takes_only_external_and_never_beside_start() {
    let good = quest_doc(
        "<quest id=\"bounty\" accept=\"external\">\n<objective id=\"o\" done=\"run.d\"/>\n</quest>\n",
    );
    let ds = diags(&good);
    assert!(errors(&ds).is_empty(), "{ds:?}");

    let bad = quest_doc(
        "<quest id=\"bounty\" accept=\"board\">\n<objective id=\"o\" done=\"run.d\"/>\n</quest>\n",
    );
    let ds = diags(&bad);
    let d = only(&ds, "E-ATTR-TYPE");
    assert_eq!(anchored(&bad, d), "board");
    assert!(d.message.contains("\"external\""), "{}", d.message);

    let started = quest_doc(
        "<quest id=\"bounty\" accept=\"external\" start=\"run.d\">\n\
         <objective id=\"o\" done=\"run.d\"/>\n</quest>\n",
    );
    let ds = diags(&started);
    let d = only(&ds, "E-ATTR-TYPE");
    assert_eq!(anchored(&started, d), "external");
    assert!(d.message.contains("`start`"), "{}", d.message);
}

// --- `<on event target>` -----------------------------------------------------

fn on_quest(on: &str) -> String {
    quest_doc(&format!(
        "<quest id=\"q\" start=\"true\">\n<objective id=\"o\" done=\"run.d\"/>\n{on}\n\
         @narrator: Fired.\n</on>\n</quest>\n"
    ))
}

#[test]
fn on_target_on_a_targeted_occasion_is_checked_against_its_domain() {
    let ok = on_quest("<on event=\"bossDefeated\" target=\"boss.gatekeeper\">");
    let ds = diags(&ok);
    assert!(errors(&ds).is_empty(), "{ds:?}");
    let open = on_quest("<on event=\"examine\" target=\"item.lamp\">");
    assert!(errors(&diags(&open)).is_empty());

    let outside = on_quest("<on event=\"bossDefeated\" target=\"boss.cinderhound\">");
    let ds = diags(&outside);
    let d = only(&ds, "E-BEAT-ATTR");
    assert_eq!(anchored(&outside, d), "boss.cinderhound");
    assert!(d.message.contains("member list"), "{}", d.message);
}

#[test]
fn on_target_on_a_lifecycle_event_is_beat_attr() {
    let src = on_quest("<on event=\"questComplete\" target=\"boss.warden\">");
    let ds = diags(&src);
    let d = only(&ds, "E-BEAT-ATTR");
    assert_eq!(anchored(&src, d), "boss.warden");
    assert!(d.message.contains("lifecycle event"), "{}", d.message);
}

#[test]
fn on_target_needs_a_targeted_occasion_of_the_event_name() {
    // An occasion raised for no target.
    let src = on_quest("<on event=\"hubVisit\" target=\"boss.warden\">");
    let ds = diags(&src);
    let d = only(&ds, "E-BEAT-ATTR");
    assert!(
        d.message
            .contains("needs a targeted occasion named `hubVisit`"),
        "{}",
        d.message
    );
    // A world event no occasion shares a name with.
    let src = on_quest("<on event=\"combatEnd\" target=\"boss.warden\">");
    let ds = diags(&src);
    let d = only(&ds, "E-BEAT-ATTR");
    assert!(
        d.message
            .contains("needs a targeted occasion named `combatEnd`"),
        "{}",
        d.message
    );
    assert!(with_code(&ds, "E-UNKNOWN-ATTR").is_empty(), "{ds:?}");
}

#[test]
fn on_target_must_be_a_quoted_dotted_id() {
    let bare = on_quest("<on event=\"bossDefeated\" target=@t>");
    let ds = diags(&bare);
    assert!(
        only(&ds, "E-BEAT-ATTR").message.contains("quoted string"),
        "{ds:?}"
    );
    let bad = on_quest("<on event=\"bossDefeated\" target=\"boss gatekeeper\">");
    let ds = diags(&bad);
    assert!(
        only(&ds, "E-BEAT-ATTR").message.contains("dotted id"),
        "{ds:?}"
    );
}

// --- `::accept{at}` ----------------------------------------------------------

#[test]
fn accept_at_next_run_is_legal_and_any_other_value_is_accept_target() {
    let ok = scene_doc("hub.board", "::accept{quest=\"bounty\" at=\"nextRun\"}\n");
    let ds = diags(&ok);
    assert!(errors(&ds).is_empty(), "{ds:?}");

    let bad = scene_doc("hub.board", "::accept{quest=\"bounty\" at=\"later\"}\n");
    let ds = diags(&bad);
    let d = only(&ds, "E-ACCEPT-TARGET");
    assert_eq!(anchored(&bad, d), "later");
    assert!(d.message.contains("\"nextRun\""), "{}", d.message);
    assert!(with_code(&ds, "E-UNKNOWN-ATTR").is_empty(), "{ds:?}");
}

#[test]
fn accept_unknown_attribute_names_both_legal_ones() {
    let src = scene_doc("hub.board", "::accept{quest=\"bounty\" now=\"yes\"}\n");
    let ds = diags(&src);
    let d = only(&ds, "E-UNKNOWN-ATTR");
    assert!(d.message.contains("`quest` and `at`"), "{}", d.message);
}

// --- accepting a subquest child ---------------------------------------------

fn tree(child_attrs: &str) -> String {
    quest_doc(&format!(
        "<quest id=\"main\" start=\"true\">\n<objective id=\"h\" quest=\"hostages\"/>\n</quest>\n\
         <quest id=\"hostages\"{child_attrs}>\n<objective id=\"o\" done=\"run.d\"/>\n</quest>\n"
    ))
}

fn project_accepts(texts: &[(&str, &str)]) -> Vec<Diagnostic> {
    check_project_accepts(&docs(texts))
        .into_iter()
        .map(|(_, d)| d)
        .collect()
}

#[test]
fn accepting_a_child_that_activates_with_its_parent_is_accept_target() {
    let scene = scene_doc("camp.talk", "::accept{quest=\"hostages\"}\n");
    let quests = tree("");
    let ds = project_accepts(&[("scene.lute", &scene), ("quest.lute", &quests)]);
    let d = only(&ds, "E-ACCEPT-TARGET");
    assert_eq!(anchored(&scene, d), "hostages");
    assert!(
        d.message.contains("activates with its parent `main`")
            && d.message.contains("activate=\"accept\""),
        "{}",
        d.message
    );
}

#[test]
fn accepting_an_activate_accept_child_is_clean() {
    let scene = scene_doc("camp.talk", "::accept{quest=\"hostages\"}\n");
    let quests = tree(" activate=\"accept\"");
    let ds = project_accepts(&[("scene.lute", &scene), ("quest.lute", &quests)]);
    assert!(ds.is_empty(), "{ds:?}");
}

#[test]
fn external_acceptance_of_a_child_that_activates_with_its_parent_is_accept_target() {
    let quests = tree(" accept=\"external\"");
    let ds = project_accepts(&[("quest.lute", &quests)]);
    let d = only(&ds, "E-ACCEPT-TARGET");
    assert_eq!(anchored(&quests, d), "external");
    assert!(
        d.message.contains("parent `main`") && d.message.contains("activate=\"accept\""),
        "{}",
        d.message
    );
    let waiting = tree(" activate=\"accept\" accept=\"external\"");
    assert!(project_accepts(&[("quest.lute", &waiting)]).is_empty());
}

// --- W-QUEST-NEVER-ACCEPTED ---------------------------------------------------

fn never_accepted(texts: &[(&str, &str)], mocked: &[(&str, &str)]) -> Vec<Diagnostic> {
    let mut by_quest: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    for (id, file) in mocked {
        by_quest
            .entry(id.to_string())
            .or_default()
            .push(PathBuf::from(file));
    }
    check_project_never_accepted(&docs(texts), &by_quest)
        .into_iter()
        .map(|(_, d)| d)
        .collect()
}

const LONELY: &str = "<quest id=\"lonely\">\n<objective id=\"o\" done=\"run.d\"/>\n</quest>\n";

#[test]
fn an_accept_driven_quest_nothing_accepts_is_never_accepted() {
    let quest = quest_doc(LONELY);
    let scene = scene_doc("hub.board", "@narrator: Nothing to take.\n");
    let ds = never_accepted(&[("scene.lute", &scene), ("quest.lute", &quest)], &[]);
    let d = only(&ds, "W-QUEST-NEVER-ACCEPTED");
    assert_eq!(d.severity, Severity::Warning);
    assert_eq!(anchored(&quest, d), "lonely");
    assert!(
        d.message.contains("::accept{quest=\"lonely\"}")
            && d.message.contains("`start`")
            && d.message.contains("accept=\"external\""),
        "{}",
        d.message
    );
}

#[test]
fn an_accept_site_or_external_acceptance_reaches_the_quest() {
    let quest = quest_doc(LONELY);
    let later = scene_doc("hub.board", "::accept{quest=\"lonely\" at=\"nextRun\"}\n");
    assert!(never_accepted(&[("scene.lute", &later), ("quest.lute", &quest)], &[]).is_empty());
    let board = quest_doc(
        "<quest id=\"lonely\" accept=\"external\">\n<objective id=\"o\" done=\"run.d\"/>\n</quest>\n",
    );
    assert!(never_accepted(&[("quest.lute", &board)], &[]).is_empty());
    // An externally accepted child that waits for its acceptance.
    let child = tree(" activate=\"accept\" accept=\"external\"");
    assert!(never_accepted(&[("quest.lute", &child)], &[]).is_empty());
}

/// dsl 0.25.0 §5 (LH N5): a test's `accepts:` mock is a precondition of the
/// test, not a route in the game — the quest still warns, and the warning
/// names the mock and `accept="external"`.
#[test]
fn a_test_mock_accepting_the_quest_does_not_silence_the_warning() {
    let quest = quest_doc(LONELY);
    let quiet = scene_doc("hub.board", "@narrator: Nothing to take.\n");
    let ds = never_accepted(
        &[("scene.lute", &quiet), ("quest.lute", &quest)],
        &[("lonely", "tests/lonely.test.yaml")],
    );
    let d = only(&ds, "W-QUEST-NEVER-ACCEPTED");
    assert!(
        d.message.contains("`tests/lonely.test.yaml`") && d.message.contains("accept=\"external\""),
        "{}",
        d.message
    );
}

#[test]
fn only_accept_driven_quests_can_be_never_accepted() {
    // A `start` quest and a child that activates with it need no accept.
    assert!(never_accepted(&[("quest.lute", &tree(""))], &[]).is_empty());
    // An `activate="accept"` child waits for one.
    let quests = tree(" activate=\"accept\"");
    let ds = never_accepted(&[("quest.lute", &quests)], &[]);
    let d = only(&ds, "W-QUEST-NEVER-ACCEPTED");
    assert_eq!(anchored(&quests, d), "hostages");
    assert!(d.message.contains("parent `main`"), "{}", d.message);
}

// --- reserved `failedBy` / `objectives.<o>.failed` ---------------------------

fn reader(when: &str) -> String {
    scene_doc(
        "camp.epilogue",
        &format!("@narrator{{when=\"{when}\"}}: The bridge story.\n"),
    )
}

#[test]
fn failed_by_and_objective_failed_read_as_always_assigned_typed_paths() {
    for when in [
        "quest.road.failedBy == 'superseded'",
        "quest.road.failedBy == 'unset'",
        "quest.road.objectives.bridge.failed",
        "!quest.road.objectives.bridge.failed && quest.road.failedBy != 'cascade'",
    ] {
        let ds = diags(&reader(when));
        assert!(errors(&ds).is_empty(), "{when}: {ds:?}");
    }
    let src = scene_doc(
        "camp.epilogue",
        "<match on=\"quest.road.failedBy\">\n<when is=\"unset\">\n@narrator: Still open.\n</when>\n\
         <when is=\"superseded|cascade\">\n@narrator: Overtaken.\n</when>\n\
         <otherwise>\n@narrator: Lost.\n</otherwise>\n</match>\n",
    );
    let ds = diags(&src);
    assert!(errors(&ds).is_empty(), "{ds:?}");
}

#[test]
fn a_failed_by_literal_outside_its_members_is_a_literal_domain_error() {
    let src = reader("quest.road.failedBy == 'supersed'");
    let ds = diags(&src);
    let d = only(&ds, "E-WHEN-LITERAL-DOMAIN");
    assert!(
        d.message.contains("did you mean `'superseded'`"),
        "{}",
        d.message
    );
    assert!(with_code(&ds, "E-ARM-DEAD").is_empty(), "{ds:?}");
    let src = scene_doc(
        "camp.epilogue",
        "<match on=\"quest.road.failedBy\">\n<when is=\"supersed\">\n@narrator: x\n</when>\n\
         <otherwise>\n@narrator: y\n</otherwise>\n</match>\n",
    );
    let ds = diags(&src);
    let d = only(&ds, "E-WHEN-LITERAL-DOMAIN");
    assert!(d.message.contains("superseded"), "{}", d.message);
}

#[test]
fn failed_by_and_objective_failed_are_unwritable_and_undeclarable() {
    for set in [
        "::set{quest.road.failedBy = 'fail'}",
        "::set{quest.road.objectives.bridge.failed = true}",
    ] {
        let ds = diags(&scene_doc("camp.epilogue", &format!("{set}\n")));
        assert_eq!(
            with_code(&ds, "E-QUEST-RESERVED-WRITE").len(),
            1,
            "{set}: {ds:?}"
        );
    }
    for path in ["quest.road.failedBy", "quest.road.objectives.bridge.failed"] {
        let src = format!(
            "---\nkind: scene\nid: camp.epilogue\nstate:\n  {path}: {{ type: string }}\n---\n\
             ## Shot 1.\n@narrator: x\n"
        );
        let ds = diags(&src);
        assert!(
            !with_code(&ds, "E-QUEST-RESERVED-DECL").is_empty(),
            "{path}: {ds:?}"
        );
    }
}

#[test]
fn project_resolves_failed_by_and_objective_failed_ids() {
    let quest = quest_doc(
        "<quest id=\"road\" start=\"true\">\n<objective id=\"bridge\" done=\"run.d\"/>\n</quest>\n",
    );
    let refs = |when: &str| {
        let scene = reader(when);
        check_project_quest_refs(&docs(&[("scene.lute", &scene), ("quest.lute", &quest)]))
            .into_iter()
            .map(|(_, d)| d)
            .collect::<Vec<_>>()
    };
    assert!(
        refs("quest.road.failedBy == 'fail' || quest.road.objectives.bridge.failed").is_empty()
    );
    let ds = refs("quest.raod.failedBy == 'fail'");
    assert!(
        only(&ds, "W-QUEST-REF-UNKNOWN")
            .message
            .contains("quest `raod`"),
        "{ds:?}"
    );
    let ds = refs("quest.road.objectives.brige.failed");
    assert!(
        only(&ds, "W-QUEST-REF-UNKNOWN")
            .message
            .contains("objective `brige`"),
        "{ds:?}"
    );
}
