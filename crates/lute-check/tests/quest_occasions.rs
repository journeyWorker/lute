//! dsl 0.21.0 §7a — quests meet scenes and occasions, through the assembled
//! `check()` and the `check-project` passes: `visited('<scene id>')` in every
//! condition slot (§7a.1) and its project-level id resolution
//! (`E-CONN-UNKNOWN-NODE`), `<objective on=>` (§7a.2, `E-BEAT-ATTR` /
//! `E-OCCASION-UNKNOWN`), and `::accept{quest=}` (§7a.3, `E-ACCEPT-TARGET`).

use std::path::PathBuf;

use lute_check::connectivity::{quest_id_set, resolve_nodes, scene_key_set};
use lute_check::{check, check_project_accepts, CheckInput, Mode, SchemaImports};
use lute_core_span::{Diagnostic, Severity};
use lute_manifest::schema::{OccasionDecl, OccasionSelect};
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_syntax::ast::{Document, Node};

fn core() -> CapabilitySnapshot {
    lute_manifest::core::load_core_snapshot()
}

/// The core snapshot plus a plugin-declared occasion vocabulary.
fn with_occasions() -> CapabilitySnapshot {
    let mut snap = core();
    for name in ["runEnd", "hubVisit"] {
        snap.occasions.insert(
            name.into(),
            OccasionDecl {
                name: name.into(),
                select: OccasionSelect::First,
                target: false,
                description: None,
            },
        );
    }
    snap
}

fn diags_with(text: &str, snapshot: CapabilitySnapshot) -> Vec<Diagnostic> {
    check(&CheckInput {
        text: text.to_string(),
        uri: "quest_occasions".into(),
        snapshot,
        providers: Default::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
        defaults: Default::default(),
    })
    .diagnostics
}

fn diags(text: &str) -> Vec<Diagnostic> {
    diags_with(text, core())
}

fn errors(ds: &[Diagnostic]) -> Vec<&Diagnostic> {
    ds.iter().filter(|d| d.severity == Severity::Error).collect()
}

fn with_code<'a>(ds: &'a [Diagnostic], code: &str) -> Vec<&'a Diagnostic> {
    ds.iter().filter(|d| d.code == code).collect()
}

fn only<'a>(ds: &'a [Diagnostic], code: &str) -> &'a Diagnostic {
    let hits = with_code(ds, code);
    assert_eq!(hits.len(), 1, "want exactly one {code}: {ds:?}");
    hits[0]
}

fn anchored<'s>(src: &'s str, d: &Diagnostic) -> &'s str {
    &src[d.span.byte_start..d.span.byte_end]
}

fn quest_doc(body: &str) -> String {
    format!("---\nkind: quest\nstate:\n  run.d: {{ type: bool, default: false }}\n---\n{body}")
}

fn scene_doc(id: &str, fm: &str, body: &str) -> String {
    format!(
        "---\nkind: scene\nid: {id}\n{fm}state:\n  run.d: {{ type: bool, default: false }}\n---\n\
         ## Shot 1.\n{body}"
    )
}

fn docs(texts: &[(&str, &str)]) -> Vec<(PathBuf, Document)> {
    texts
        .iter()
        .map(|(path, text)| (PathBuf::from(path), lute_syntax::parse(text).0))
        .collect()
}

fn unknown_nodes(texts: &[(&str, &str)]) -> Vec<Diagnostic> {
    let docs = docs(texts);
    let key_set = scene_key_set(&docs);
    let quest_ids = quest_id_set(&docs);
    resolve_nodes(&docs, &key_set, &quest_ids)
        .into_iter()
        .map(|(_, d)| d)
        .collect()
}

// --- §7a.1 `visited()` in every condition slot -------------------------------

#[test]
fn visited_is_legal_in_quest_start_fail_and_objective_done() {
    let src = quest_doc(
        "<quest id=\"q\" start=\"visited('haven.s01ep01')\" fail=\"!visited('haven.s01ep01') && run.d\">\n\
         <objective id=\"heardVesna\" title=\"Hear Vesna out\" done=\"visited('haven.s01ep04')\"/>\n\
         </quest>\n",
    );
    let ds = diags(&src);
    assert!(errors(&ds).is_empty(), "{ds:?}");
}

#[test]
fn visited_is_legal_in_a_beat_when() {
    let src = scene_doc(
        "haven.s01ep05",
        "on: hubVisit\nwhen: \"visited('haven.s01ep04')\"\n",
        "@narrator: Back again.\n",
    );
    let ds = diags(&src);
    assert!(errors(&ds).is_empty(), "{ds:?}");
    // The slot IS profile-checked: a malformed call there is rejected.
    let bad = scene_doc(
        "haven.s01ep05",
        "on: hubVisit\nwhen: \"visited(run.d)\"\n",
        "@narrator: Back again.\n",
    );
    assert_eq!(with_code(&diags(&bad), "E-CEL-PROFILE").len(), 1);
}

#[test]
fn visited_is_legal_in_an_entry_when() {
    let src = "---\nkind: lore\ntitle: Barks\n---\n\
               <entry id=\"bark\" on=\"hubVisit\" when=\"visited('haven.s01ep04')\">\n\
               @vesna: You came back.\n</entry>\n";
    let ds = diags(src);
    assert!(errors(&ds).is_empty(), "{ds:?}");
}

#[test]
fn visited_is_legal_in_line_and_choice_when() {
    let src = scene_doc(
        "haven.s01ep06",
        "",
        "@vesna{when=\"visited('haven.s01ep04')\"}: We met before.\n\
         <branch id=\"b\">\n<choice id=\"c\" label=\"Remind her\" when=\"visited('haven.s01ep04')\">\n\
         @vesna: I remember.\n</choice>\n<choice id=\"d\" label=\"Leave\">\n@vesna: Bye.\n</choice>\n\
         </branch>\n",
    );
    let ds = diags(&src);
    assert!(errors(&ds).is_empty(), "{ds:?}");
}

#[test]
fn visited_outside_a_condition_slot_is_out_of_profile() {
    let src = scene_doc("haven.s01ep07", "", "::set{run.d = visited('haven.s01ep04')}\n");
    let ds = diags(&src);
    let d = only(&ds, "E-CEL-PROFILE");
    assert!(d.message.contains("condition slot"), "{}", d.message);
}

#[test]
fn malformed_visited_is_out_of_profile_and_the_message_lists_it() {
    let src = quest_doc(
        "<quest id=\"q\">\n<objective id=\"o\" done=\"visited(run.d)\"/>\n</quest>\n",
    );
    let ds = diags(&src);
    let d = only(&ds, "E-CEL-PROFILE");
    assert!(d.message.contains("`visited('<scene id>')`"), "{}", d.message);
}

#[test]
fn visited_is_never_decided_per_file() {
    // An undecided atom: neither `E-OBJECTIVE-UNREACHABLE`-style dead-guard
    // nor always-true verdicts may fire on it alone.
    let src = quest_doc(
        "<quest id=\"q\">\n<objective id=\"o\" done=\"visited('a.b') && !visited('a.b')\"/>\n\
         <objective id=\"p\" done=\"visited('a.b') || true\"/>\n</quest>\n",
    );
    let ds = diags(&src);
    assert!(errors(&ds).is_empty(), "{ds:?}");
}

#[test]
fn project_resolves_visited_ids_in_every_slot() {
    let scene = scene_doc("haven.s01ep04", "", "@vesna: Hello.\n");
    let quest = quest_doc(
        "<quest id=\"q\" start=\"visited('haven.s01ep04')\">\n\
         <objective id=\"o\" done=\"visited('haven.s01ep4')\"/>\n</quest>\n",
    );
    let ds = unknown_nodes(&[("scene.lute", &scene), ("quest.lute", &quest)]);
    let d = only(&ds, "E-CONN-UNKNOWN-NODE");
    assert!(
        d.message.contains("`haven.s01ep4`") && d.message.contains("did you mean `haven.s01ep04`"),
        "{}",
        d.message
    );
    assert_eq!(anchored(&quest, d), "visited('haven.s01ep4')");
}

#[test]
fn project_resolves_visited_ids_in_a_beat_when() {
    let beat = scene_doc(
        "haven.s01ep05",
        "on: hubVisit\nwhen: \"visited('nowhere.s01')\"\n",
        "@narrator: Back.\n",
    );
    let ds = unknown_nodes(&[("beat.lute", &beat)]);
    let d = only(&ds, "E-CONN-UNKNOWN-NODE");
    assert!(d.message.contains("`nowhere.s01`"), "{}", d.message);
    assert_eq!(anchored(&beat, d), "visited('nowhere.s01')");
}

#[test]
fn project_accepts_known_visited_ids() {
    let scene = scene_doc("haven.s01ep04", "", "@vesna{when=\"visited('haven.s01ep04')\"}: Hi.\n");
    assert!(unknown_nodes(&[("scene.lute", &scene)]).is_empty());
}

// --- §7a.2 `<objective on=>` ---------------------------------------------------

#[test]
fn objective_on_parses_into_the_ast() {
    let src = quest_doc(
        "<quest id=\"q\">\n<objective id=\"low\" on=\"runEnd\" done=\"run.d\"/>\n</quest>\n",
    );
    let (doc, _) = lute_syntax::parse(&src);
    let Node::Objective(o) = &doc.quests[0].body[0] else {
        panic!("objective")
    };
    let (on, span) = o.on.as_ref().expect("on");
    assert_eq!(on, "runEnd");
    assert_eq!(&src[span.byte_start..span.byte_end], "runEnd");
    assert!(o.attrs.is_empty(), "`on` is extracted, not residual");
}

#[test]
fn objective_on_declared_occasion_is_clean() {
    let src = quest_doc(
        "<quest id=\"q\">\n<objective id=\"low\" on=\"runEnd\" done=\"run.d\"/>\n</quest>\n",
    );
    let ds = diags_with(&src, with_occasions());
    assert!(errors(&ds).is_empty(), "{ds:?}");
}

#[test]
fn objective_on_is_shape_only_without_a_vocabulary() {
    let src = quest_doc(
        "<quest id=\"q\">\n<objective id=\"low\" on=\"anythingGoes\" done=\"run.d\"/>\n</quest>\n",
    );
    let ds = diags(&src);
    assert!(errors(&ds).is_empty(), "{ds:?}");
}

#[test]
fn objective_on_non_identifier_is_beat_attr() {
    let src = quest_doc(
        "<quest id=\"q\">\n<objective id=\"low\" on=\"run end\" done=\"run.d\"/>\n</quest>\n",
    );
    let ds = diags_with(&src, with_occasions());
    let d = only(&ds, "E-BEAT-ATTR");
    assert_eq!(anchored(&src, d), "run end");
    assert!(with_code(&ds, "E-OCCASION-UNKNOWN").is_empty(), "{ds:?}");
}

#[test]
fn objective_on_unquoted_value_is_beat_attr() {
    let src = quest_doc(
        "<quest id=\"q\">\n<objective id=\"low\" on=@x done=\"run.d\"/>\n</quest>\n",
    );
    let ds = diags(&src);
    assert!(!with_code(&ds, "E-BEAT-ATTR").is_empty(), "{ds:?}");
}

#[test]
fn objective_on_unknown_occasion_with_a_vocabulary() {
    let src = quest_doc(
        "<quest id=\"q\">\n<objective id=\"low\" on=\"runEnds\" done=\"run.d\"/>\n</quest>\n",
    );
    let ds = diags_with(&src, with_occasions());
    let d = only(&ds, "E-OCCASION-UNKNOWN");
    assert_eq!(anchored(&src, d), "runEnds");
    assert!(d.message.contains("did you mean `runEnd`"), "{}", d.message);
}

// --- §7a.3 `::accept` --------------------------------------------------------

#[test]
fn accept_with_a_quest_id_is_clean_per_file() {
    let src = scene_doc("haven.s01ep04", "", "::accept{quest=\"helpVesna\"}\n");
    let ds = diags(&src);
    assert!(errors(&ds).is_empty(), "{ds:?}");
}

#[test]
fn accept_without_quest_is_accept_target() {
    let src = scene_doc("haven.s01ep04", "", "::accept\n");
    let ds = diags(&src);
    let d = only(&ds, "E-ACCEPT-TARGET");
    assert!(d.message.contains("names no quest"), "{}", d.message);
}

#[test]
fn accept_with_a_non_identifier_quest_is_accept_target() {
    let src = scene_doc("haven.s01ep04", "", "::accept{quest=\"help-vesna\"}\n");
    let ds = diags(&src);
    let d = only(&ds, "E-ACCEPT-TARGET");
    assert_eq!(anchored(&src, d), "help-vesna");
    assert!(with_code(&ds, "E-UNKNOWN-DIRECTIVE").is_empty(), "{ds:?}");
}

#[test]
fn accept_with_a_ref_quest_is_accept_target() {
    let src = scene_doc("haven.s01ep04", "", "::accept{quest=@q}\n");
    assert!(!with_code(&diags(&src), "E-ACCEPT-TARGET").is_empty());
}

#[test]
fn accept_with_another_attribute_is_unknown_attr() {
    let src = scene_doc("haven.s01ep04", "", "::accept{quest=\"q\" now=\"yes\"}\n");
    let ds = diags(&src);
    let d = only(&ds, "E-UNKNOWN-ATTR");
    assert!(d.message.contains("`::accept`"), "{}", d.message);
    assert!(with_code(&ds, "E-ACCEPT-TARGET").is_empty(), "{ds:?}");
}

fn accepts(texts: &[(&str, &str)]) -> Vec<Diagnostic> {
    check_project_accepts(&docs(texts))
        .into_iter()
        .map(|(_, d)| d)
        .collect()
}

#[test]
fn project_accept_of_an_accept_driven_quest_is_clean() {
    let scene = scene_doc(
        "haven.s01ep04",
        "",
        "<branch id=\"offer\">\n<choice id=\"yes\" label=\"Help\">\n::accept{quest=\"helpVesna\"}\n\
         </choice>\n<choice id=\"no\" label=\"Leave\">\n@vesna: Fine.\n</choice>\n</branch>\n",
    );
    let quest = quest_doc(
        "<quest id=\"helpVesna\">\n<objective id=\"o\" done=\"run.d\"/>\n</quest>\n",
    );
    let ds = accepts(&[("scene.lute", &scene), ("quest.lute", &quest)]);
    assert!(ds.is_empty(), "{ds:?}");
}

#[test]
fn project_accept_of_a_start_quest_is_accept_target() {
    let scene = scene_doc("haven.s01ep04", "", "::accept{quest=\"helpVesna\"}\n");
    let quest = quest_doc(
        "<quest id=\"helpVesna\" start=\"run.d\">\n<objective id=\"o\" done=\"run.d\"/>\n</quest>\n",
    );
    let ds = accepts(&[("scene.lute", &scene), ("quest.lute", &quest)]);
    let d = only(&ds, "E-ACCEPT-TARGET");
    assert!(d.message.contains("has a `start` condition"), "{}", d.message);
    assert_eq!(anchored(&scene, d), "helpVesna");
}

#[test]
fn project_accept_of_an_unknown_quest_is_accept_target() {
    let scene = scene_doc("haven.s01ep04", "", "::accept{quest=\"helpVesnaa\"}\n");
    let quest = quest_doc(
        "<quest id=\"helpVesna\">\n<objective id=\"o\" done=\"run.d\"/>\n</quest>\n",
    );
    let ds = accepts(&[("scene.lute", &scene), ("quest.lute", &quest)]);
    let d = only(&ds, "E-ACCEPT-TARGET");
    assert!(
        d.message.contains("no quest in the project") && d.message.contains("`helpVesna`"),
        "{}",
        d.message
    );
}

#[test]
fn project_accept_skips_a_malformed_target() {
    let scene = scene_doc("haven.s01ep04", "", "::accept{quest=\"help-vesna\"}\n");
    assert!(accepts(&[("scene.lute", &scene)]).is_empty());
}
