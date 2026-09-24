//! dsl 0.21.0 beats and occasions through the assembled `check()` and the
//! `check-project` passes: scene frontmatter beat keys and `<entry on=
//! priority=>` shape (`E-BEAT-ATTR`), the occasion vocabulary
//! (`E-OCCASION-UNKNOWN`, shape-only mode, untargeted occasions), the `when`
//! CEL slot (registry checks, the `scene.*` ban), `E-BEAT-UNREACHABLE` per
//! file and under the fact envelope, and `W-BEAT-SHADOWED`.

use std::path::{Path, PathBuf};

use lute_check::connectivity::{
    ambiguous_quest_ids, assemble_graph, check_reachability, live_assert_sites, quest_id_set,
    scene_key_set, unreachable_quest_ids,
};
use lute_check::{
    check, check_fact_guards, check_project_beats, compute_must, fold_env, BeatOnce, CheckInput,
    FactEnv, FoldedEnv, GroundFact, MaySet, Mode, RootVocab, SchemaImports,
};
use lute_core_span::{Diagnostic, Severity};
use lute_manifest::schema::{OccasionDecl, OccasionSelect};
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_syntax::ast::Document;

fn core() -> CapabilitySnapshot {
    lute_manifest::core::load_core_snapshot()
}

/// The core snapshot plus a plugin-declared occasion vocabulary (dsl 0.21.0
/// §2): `hubVisit` / `talk` (targeted) are `select: first`, `inbox` is
/// `select: all`.
fn with_occasions() -> CapabilitySnapshot {
    let mut snap = core();
    for (name, select, target) in [
        ("hubVisit", OccasionSelect::First, false),
        ("talk", OccasionSelect::First, true),
        ("inbox", OccasionSelect::All, false),
    ] {
        snap.occasions.insert(
            name.into(),
            OccasionDecl {
                name: name.into(),
                select,
                target: target.into(),
                description: None,
            },
        );
    }
    snap
}

fn input(text: &str, snapshot: CapabilitySnapshot) -> CheckInput {
    CheckInput {
        text: text.to_string(),
        uri: "beats".into(),
        snapshot,
        providers: Default::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
        defaults: Default::default(),
    }
}

fn diags_with(text: &str, snapshot: CapabilitySnapshot) -> Vec<Diagnostic> {
    check(&input(text, snapshot)).diagnostics
}

fn diags(text: &str) -> Vec<Diagnostic> {
    diags_with(text, core())
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
    ds.iter().filter(|d| d.severity == Severity::Error).collect()
}

/// The source text a diagnostic is anchored at.
fn anchored<'s>(src: &'s str, d: &Diagnostic) -> &'s str {
    &src[d.span.byte_start..d.span.byte_end]
}

/// A scene with authored `id:` and the beat frontmatter `fm` (each line
/// newline-terminated).
fn scene(id: &str, fm: &str) -> String {
    format!(
        "---\nkind: scene\nid: {id}\n{fm}state:\n  user.runs: {{ type: number, default: 0 }}\n  \
         run.mood: {{ type: number }}\n  scene.local: {{ type: bool, default: false }}\n---\n\
         ## Shot 1.\n@narrator: Hello.\n"
    )
}

fn lore(body: &str) -> String {
    format!("---\nkind: lore\ntitle: Barks\n---\n{body}")
}

fn typed_beat(text: &str, snapshot: CapabilitySnapshot) -> Option<lute_check::BeatMeta> {
    let input = input(text, snapshot);
    let (doc, _) = lute_syntax::parse(&input.text);
    fold_env(&doc, &input).0.typed.beat
}

// --- the scene beat --------------------------------------------------------

#[test]
fn scene_beat_lifts_every_key_and_checks_clean() {
    let src = scene(
        "hades.achilles.run12",
        "on: talk\ntarget: npc.achilles\nwhen: 'user.runs >= 10'\npriority: 50\nonce: user\n",
    );
    let ds = diags_with(&src, with_occasions());
    assert!(errors(&ds).is_empty(), "{ds:?}");
    let beat = typed_beat(&src, with_occasions()).expect("a beat");
    assert_eq!(beat.on, "talk");
    assert_eq!(beat.target.as_deref(), Some("npc.achilles"));
    let when = beat.when.expect("when slot");
    assert_eq!(when.raw, "user.runs >= 10");
    assert_eq!(
        &src[when.span.byte_start..when.span.byte_end],
        "user.runs >= 10",
        "the slot span is the value inside the quotes"
    );
    assert_eq!(when.span.line, 6, "the slot carries its real line");
    assert_eq!(beat.priority, 50);
    assert_eq!(beat.once, BeatOnce::User);
}

#[test]
fn scene_beat_defaults_are_priority_zero_and_once_run() {
    let beat = typed_beat(&scene("a.b", "on: hubVisit\n"), core()).expect("a beat");
    assert_eq!((beat.priority, beat.once), (0, BeatOnce::Run));
    assert!(beat.target.is_none() && beat.when.is_none());
    let beat = typed_beat(&scene("a.b", "on: hubVisit\nonce: false\n"), core()).unwrap();
    assert_eq!(beat.once, BeatOnce::None);
    assert_eq!(BeatOnce::None.as_str(), "none");
    assert!(typed_beat(&scene("a.b", ""), core()).is_none(), "no `on:`, no beat");
}

#[test]
fn non_identifier_on_is_beat_attr_at_the_value() {
    for (fm, value) in [("on: 'hub visit'\n", "hub visit"), ("on: 5\n", "5")] {
        let src = scene("a.b", fm);
        let ds = diags(&src);
        let d = only(&ds, "E-BEAT-ATTR");
        assert_eq!(anchored(&src, d), value, "{d:?}");
        assert!(typed_beat(&src, core()).is_none(), "an invalid `on` lifts no beat");
    }
}

#[test]
fn malformed_target_is_beat_attr() {
    for target in ["npc..achilles", "'1npc'", "npc.a b"] {
        let src = scene("a.b", &format!("on: talk\ntarget: {target}\n"));
        let ds = diags(&src);
        let d = only(&ds, "E-BEAT-ATTR");
        assert!(d.message.contains("`target:` must be a dotted id"), "{}", d.message);
    }
}

#[test]
fn non_integer_priority_is_beat_attr() {
    for p in ["high", "1.5", "'5'"] {
        let src = scene("a.b", &format!("on: talk\npriority: {p}\n"));
        let ds = diags(&src);
        let d = only(&ds, "E-BEAT-ATTR");
        assert!(d.message.contains("`priority:` must be an integer"), "{p}: {}", d.message);
    }
    let src = scene("a.b", "on: talk\npriority: -3\n");
    assert!(errors(&diags(&src)).is_empty(), "negative priority is an integer");
    assert_eq!(typed_beat(&src, core()).unwrap().priority, -3);
}

#[test]
fn once_outside_run_user_false_is_beat_attr() {
    for once in ["always", "true", "'false'", "2"] {
        let src = scene("a.b", &format!("on: talk\nonce: {once}\n"));
        let ds = diags(&src);
        let d = only(&ds, "E-BEAT-ATTR");
        assert!(d.message.contains("`once:` must be `run`"), "{once}: {}", d.message);
    }
    for once in ["run", "user", "false"] {
        let src = scene("a.b", &format!("on: talk\nonce: {once}\n"));
        assert!(errors(&diags(&src)).is_empty(), "{once}");
    }
}

#[test]
fn every_beat_key_without_on_is_beat_attr_at_its_key() {
    let src = scene(
        "a.b",
        "target: npc.a\nwhen: 'user.runs > 1'\npriority: 3\nonce: user\n",
    );
    let ds = diags(&src);
    let hits = with_code(&ds, "E-BEAT-ATTR");
    let keys: Vec<&str> = hits.iter().map(|d| anchored(&src, d)).collect();
    assert_eq!(keys, ["target", "when", "priority", "once"], "{ds:?}");
    assert!(hits.iter().all(|d| d.message.contains("without `on:`")));
    assert!(typed_beat(&src, core()).is_none());
}

#[test]
fn empty_or_non_string_when_is_beat_attr() {
    for when in ["''", "true", "[a]"] {
        let src = scene("a.b", &format!("on: talk\nwhen: {when}\n"));
        let ds = diags(&src);
        only(&ds, "E-BEAT-ATTR");
        assert!(typed_beat(&src, core()).unwrap().when.is_none());
    }
}

#[test]
fn beat_keys_on_a_quest_or_lore_document_are_unknown_keys() {
    let quest = "---\nkind: quest\non: talk\npriority: 2\n---\n<quest id=\"q\">\n\
                 <objective id=\"o\" done=\"true\"/>\n</quest>\n";
    let ds = diags(quest);
    let unknown = with_code(&ds, "E-META-UNKNOWN-KEY");
    assert_eq!(unknown.len(), 2, "{ds:?}");
    assert!(unknown[0].message.contains("only a scene's frontmatter declares a beat"));
    assert!(with_code(&ds, "E-BEAT-ATTR").is_empty(), "never a beat off a scene: {ds:?}");

    let lore_doc = "---\nkind: lore\non: talk\n---\n<entry id=\"e\">\n@narrator: hi\n</entry>\n";
    let ds = diags(lore_doc);
    let d = only(&ds, "E-META-UNKNOWN-KEY");
    assert!(d.message.contains("`<entry on=…>`"), "{}", d.message);
}

#[test]
fn beat_keys_are_never_defaultable() {
    for key in lute_check::BEAT_KEYS {
        assert!(
            !lute_check::meta::default_key_legal_on(key, lute_check::MetaKind::Scene),
            "{key} must not be defaultable"
        );
    }
}

// --- occasion vocabulary ---------------------------------------------------

#[test]
fn shape_only_when_no_plugin_declares_occasions() {
    let src = scene("a.b", "on: anythingGoes\ntarget: npc.a\n");
    assert!(errors(&diags(&src)).is_empty(), "{:?}", diags(&src));
    let entry = lore("<entry id=\"e\" on=\"whatever\" target=\"npc.a\">\n@narrator: hi\n</entry>\n");
    assert!(errors(&diags(&entry)).is_empty(), "{:?}", diags(&entry));
}

#[test]
fn undeclared_occasion_is_occasion_unknown_with_a_suggestion() {
    let src = scene("a.b", "on: tlak\n");
    let ds = diags_with(&src, with_occasions());
    let d = only(&ds, "E-OCCASION-UNKNOWN");
    assert_eq!(anchored(&src, d), "tlak");
    assert!(d.message.contains("did you mean `talk`?"), "{}", d.message);
    assert!(d.message.contains("`hubVisit`, `inbox`, `talk`"), "{}", d.message);

    let entry = lore("<entry id=\"e\" on=\"shout\">\n@narrator: hi\n</entry>\n");
    let ds = diags_with(&entry, with_occasions());
    let d = only(&ds, "E-OCCASION-UNKNOWN");
    assert_eq!(anchored(&entry, d), "shout");

    let known = scene("a.b", "on: hubVisit\n");
    assert!(errors(&diags_with(&known, with_occasions())).is_empty());
}

#[test]
fn target_on_an_untargeted_occasion_is_beat_attr() {
    let src = scene("a.b", "on: hubVisit\ntarget: npc.a\n");
    let ds = diags_with(&src, with_occasions());
    let d = only(&ds, "E-BEAT-ATTR");
    assert_eq!(anchored(&src, d), "npc.a");
    assert!(d.message.contains("declared without `target: true`"), "{}", d.message);

    let entry = lore("<entry id=\"e\" on=\"inbox\" target=\"npc.a\">\n@narrator: hi\n</entry>\n");
    let ds = diags_with(&entry, with_occasions());
    assert_eq!(anchored(&entry, only(&ds, "E-BEAT-ATTR")), "npc.a");

    let targeted = scene("a.b", "on: talk\ntarget: npc.a\n");
    assert!(errors(&diags_with(&targeted, with_occasions())).is_empty());
}

// --- entry beats -----------------------------------------------------------

#[test]
fn entry_beat_attrs_check_clean() {
    let src = lore(
        "<entry id=\"achillesBark3\" on=\"talk\" target=\"npc.achilles\" category=\"bark\" \
         priority=\"10\" when=\"entry.achillesBark3.read == false\">\n\
         @narrator: Back again, lad.\n</entry>\n",
    );
    let ds = diags_with(&src, with_occasions());
    assert!(errors(&ds).is_empty(), "{ds:?}");
}

#[test]
fn entry_beat_shape_faults_are_beat_attr() {
    let cases = [
        ("on=\"two words\"", "two words"),
        ("on=\"talk\" priority=\"ten\"", "ten"),
        ("priority=\"3\"", "3"),
    ];
    for (attrs, anchor) in cases {
        let src = lore(&format!("<entry id=\"e\" {attrs}>\n@narrator: hi\n</entry>\n"));
        let ds = diags(&src);
        let d = only(&ds, "E-BEAT-ATTR");
        assert_eq!(anchored(&src, d), anchor, "{attrs}: {ds:?}");
        assert!(with_code(&ds, "E-UNKNOWN-ATTR").is_empty(), "on/priority are in the closure");
    }
    let src = lore("<entry id=\"e\" on=\"talk\" priority>\n@narrator: hi\n</entry>\n");
    let ds = diags(&src);
    let d = only(&ds, "E-BEAT-ATTR");
    assert!(d.message.contains("must be a quoted string"), "{}", d.message);
    assert!(with_code(&ds, "E-ENTRY-ATTR").is_empty(), "the beat key owns its shape: {ds:?}");
}

#[test]
fn entry_beat_parse_priority_helper() {
    assert_eq!(lute_check::parse_beat_priority("10"), Some(10));
    assert_eq!(lute_check::parse_beat_priority("-2"), Some(-2));
    for bad in ["", "-", "+1", "1.0", "ten", "99999999999999999999"] {
        assert_eq!(lute_check::parse_beat_priority(bad), None, "{bad}");
    }
}

// --- the `when` slot -------------------------------------------------------

#[test]
fn when_joins_the_cel_slot_registry() {
    let undeclared = scene("a.b", "on: talk\nwhen: 'run.nope > 1'\n");
    assert!(with_code(&diags(&undeclared), "E-UNDECLARED").len() == 1);
    let maybe_unset = scene("a.b", "on: talk\nwhen: 'run.mood > 1'\n");
    assert_eq!(with_code(&diags(&maybe_unset), "E-MAYBE-UNSET").len(), 1);
    let guarded = scene("a.b", "on: talk\nwhen: 'isSet(run.mood) && run.mood > 1'\n");
    assert!(with_code(&diags(&guarded), "E-MAYBE-UNSET").is_empty());
    let broken = scene("a.b", "on: talk\nwhen: 'user.runs >'\n");
    assert_eq!(with_code(&diags(&broken), "E-CEL-PARSE").len(), 1, "{:?}", diags(&broken));
    let not_bool = scene("a.b", "on: talk\nwhen: '@nope'\n");
    assert_eq!(with_code(&diags(&not_bool), "E-UNDECLARED-REF").len(), 1);
    // 0.21.1 T1-1: `unset` is a member of the always-assigned quest state, so
    // comparing to it is an ordinary guard, not the E-UNSET-LITERAL sentinel
    // mistake (that rule keeps covering maybe-unset subjects, tests/reachability.rs).
    let quest_unset = scene("a.b", "on: talk\nwhen: \"quest.foo.state != 'unset'\"\n");
    let ds = diags(&quest_unset);
    assert!(with_code(&ds, "E-UNSET-LITERAL").is_empty(), "{ds:?}");
}

#[test]
fn when_reading_scene_state_is_beat_attr_without_a_cascade() {
    let src = scene("a.b", "on: talk\nwhen: 'scene.local && user.runs > 1'\n");
    let ds = diags(&src);
    let d = only(&ds, "E-BEAT-ATTR");
    assert!(d.message.contains("`scene.local`"), "{}", d.message);
    assert_eq!(anchored(&src, d), "scene.local && user.runs > 1");
    let undeclared = scene("a.b", "on: talk\nwhen: 'scene.missing'\n");
    let ds = diags(&undeclared);
    only(&ds, "E-BEAT-ATTR");
    assert!(with_code(&ds, "E-UNDECLARED").is_empty(), "the beat rule is the root: {ds:?}");
}

#[test]
fn when_that_decides_false_is_beat_unreachable_per_file() {
    let src = scene("hades.a", "on: talk\nwhen: 'user.runs > 1 && false'\n");
    let ds = diags(&src);
    let d = only(&ds, "E-BEAT-UNREACHABLE");
    assert_eq!(d.severity, Severity::Error);
    assert_eq!(anchored(&src, d), "user.runs > 1 && false");
    assert!(d.message.starts_with("beat `hades.a` is never eligible"), "{}", d.message);

    // An entry beat keeps its own code.
    let entry = lore("<entry id=\"bark\" on=\"talk\" when=\"false\">\n@narrator: hi\n</entry>\n");
    let ds = diags(&entry);
    only(&ds, "E-ENTRY-UNREACHABLE");
    assert!(with_code(&ds, "E-BEAT-UNREACHABLE").is_empty());
}

// --- check-project ---------------------------------------------------------

const VOCAB: &str = "entities:\n  crew: { members: [vesna, toma] }\n\
    relations:\n  awake: { args: [crew], tier: run }\n  found: { args: [crew], tier: run }\n\
    facts:\n  - \"awake(vesna)\"\n";

fn fact_scene(id: &str, fm: &str) -> String {
    format!("---\nkind: scene\nid: {id}\n{fm}{VOCAB}---\n## Shot 1.\n@narrator: Hello.\n")
}

struct Project {
    docs: Vec<(PathBuf, Document)>,
    foldeds: Vec<FoldedEnv>,
    per_file: Vec<Vec<Diagnostic>>,
    env: FactEnv,
}

fn project(texts: &[(&str, &str)], snapshot: CapabilitySnapshot) -> Project {
    let mut docs = Vec::new();
    let mut foldeds = Vec::new();
    let mut per_file = Vec::new();
    let mut results = Vec::new();
    for (path, text) in texts {
        let input = input(text, snapshot.clone());
        let (doc, _) = lute_syntax::parse(&input.text);
        let (folded, _, _) = fold_env(&doc, &input);
        let result = check(&input);
        per_file.push(result.diagnostics.clone());
        results.push((PathBuf::from(path), result));
        foldeds.push(folded);
        docs.push((PathBuf::from(path), doc));
    }
    let key_set = scene_key_set(&docs);
    let quest_ids = quest_id_set(&docs);
    let (graph, _) = assemble_graph(&docs, &key_set, &quest_ids);
    let lifecycle = unreachable_quest_ids(&docs, &results);
    let ambiguous = ambiguous_quest_ids(&docs);
    let (reach, _) = check_reachability(&graph, &quest_ids, &ambiguous, &lifecycle);
    let mut vocab = RootVocab::default();
    for folded in &foldeds {
        vocab.add(&folded.env.rel_vocab, &folded.env.domains);
    }
    let facts = live_assert_sites(&docs, &reach, &ambiguous, &lifecycle)
        .into_iter()
        .filter_map(|(_, a)| GroundFact::from_pattern(&a.pattern));
    let may = MaySet::build(&vocab, facts, &lute_check::stable_seeds(&docs, &vocab));
    let folded_refs: Vec<&FoldedEnv> = foldeds.iter().collect();
    let must = compute_must(&docs, &folded_refs, &graph, &vocab, &may);
    let env = FactEnv::new(may, must.slots);
    Project {
        docs,
        foldeds,
        per_file,
        env,
    }
}

impl Project {
    fn guards(&self, path: &str) -> Vec<Diagnostic> {
        let i = self
            .docs
            .iter()
            .position(|(p, _)| p == Path::new(path))
            .expect("fixture path");
        check_fact_guards(
            &self.docs[i].0,
            &self.docs[i].1,
            &self.foldeds[i],
            &self.env,
            &self.per_file[i],
        )
    }

    /// `W-BEAT-SHADOWED` only (the 0.22 tie / once advisories have their own
    /// tests in `harness_022.rs`).
    fn shadowed(&self) -> Vec<(PathBuf, Diagnostic)> {
        let refs: Vec<&FoldedEnv> = self.foldeds.iter().collect();
        check_project_beats(&self.docs, &refs)
            .into_iter()
            .filter(|(_, d)| d.code == "W-BEAT-SHADOWED")
            .collect()
    }
}

#[test]
fn when_over_an_impossible_fact_is_beat_unreachable_in_the_project() {
    let src = fact_scene("haven.a", "on: talk\nwhen: 'holds(found(toma))'\n");
    let p = project(&[("a.lute", &src)], core());
    assert!(
        with_code(&p.per_file[0], "E-BEAT-UNREACHABLE").is_empty(),
        "undecided without facts: {:?}",
        p.per_file[0]
    );
    let ds = p.guards("a.lute");
    let d = only(&ds, "E-BEAT-UNREACHABLE");
    assert_eq!(anchored(&src, d), "holds(found(toma))");
    assert!(d.message.contains("no seed, assert, rule, or engine relation produces"), "{}", d.message);
}

#[test]
fn when_over_a_guaranteed_fact_is_fact_guaranteed() {
    let src = fact_scene("haven.a", "on: talk\nwhen: 'holds(awake(vesna))'\n");
    let p = project(&[("a.lute", &src)], core());
    let ds = p.guards("a.lute");
    let w = only(&ds, "W-FACT-GUARANTEED");
    assert!(w.message.contains("`awake(vesna)` is a `facts:` seed"), "{}", w.message);
}

#[test]
fn a_scalar_dead_when_is_reported_once_per_file() {
    let src = fact_scene("haven.a", "on: talk\nwhen: 'false && holds(found(toma))'\n");
    let p = project(&[("a.lute", &src)], core());
    only(&p.per_file[0], "E-BEAT-UNREACHABLE");
    assert!(p.guards("a.lute").is_empty(), "{:?}", p.guards("a.lute"));
}

fn shadow_codes(p: &Project) -> Vec<(String, String)> {
    p.shadowed()
        .into_iter()
        .map(|(path, d)| {
            assert_eq!(d.severity, Severity::Warning);
            (path.display().to_string(), d.message)
        })
        .collect()
}

#[test]
fn an_always_eligible_repeatable_beat_shadows_a_lower_priority_one() {
    let a = scene("hades.a", "on: talk\ntarget: npc.achilles\npriority: 10\nonce: false\n");
    let b = scene("hades.b", "on: talk\ntarget: npc.achilles\npriority: 5\nwhen: 'user.runs > 3'\n");
    let p = project(&[("a.lute", &a), ("b.lute", &b)], with_occasions());
    let shadowed = p.shadowed();
    assert_eq!(shadowed.len(), 1, "{shadowed:?}");
    let (path, d) = &shadowed[0];
    assert_eq!(path, Path::new("b.lute"));
    assert_eq!(anchored(&b, d), "on", "anchored at the shadowed beat's `on`");
    assert!(
        d.message.starts_with(
            "scene `hades.b` can never win occasion `talk` for `npc.achilles`: scene `hades.a` \
             (priority 10) is ordered before it"
        ),
        "{}",
        d.message
    );
}

#[test]
fn an_untargeted_always_beat_shadows_a_targeted_one_but_not_the_reverse() {
    let a = scene("hades.a", "on: talk\nonce: false\n");
    let b = scene("hades.b", "on: talk\ntarget: npc.achilles\n");
    let p = project(&[("a.lute", &a), ("b.lute", &b)], with_occasions());
    assert_eq!(shadow_codes(&p).len(), 1);

    let a = scene("hades.a", "on: talk\ntarget: npc.achilles\nonce: false\n");
    let b = scene("hades.b", "on: talk\n");
    let p = project(&[("a.lute", &a), ("b.lute", &b)], with_occasions());
    assert!(shadow_codes(&p).is_empty(), "B still wins for other targets");
}

#[test]
fn priority_orders_before_document_order() {
    // `a` is first in the project but has the LOWER priority: `b` wins, and
    // `b` is spent (`once: run`), so nothing is shadowed.
    let a = scene("hades.a", "on: hubVisit\npriority: 1\nonce: false\n");
    let b = scene("hades.b", "on: hubVisit\npriority: 2\n");
    let p = project(&[("a.lute", &a), ("b.lute", &b)], with_occasions());
    assert!(shadow_codes(&p).is_empty(), "{:?}", shadow_codes(&p));

    // Same priorities: document order breaks the tie.
    let a = scene("hades.a", "on: hubVisit\nonce: false\n");
    let b = scene("hades.b", "on: hubVisit\n");
    let p = project(&[("b.lute", &b), ("a.lute", &a)], with_occasions());
    assert!(shadow_codes(&p).is_empty(), "b is first and spendable");
    let p = project(&[("a.lute", &a), ("b.lute", &b)], with_occasions());
    assert_eq!(shadow_codes(&p).len(), 1);
}

#[test]
fn a_spendable_conditional_or_gated_beat_never_shadows() {
    let b = scene("hades.b", "on: hubVisit\n");
    for a_fm in [
        "on: hubVisit\npriority: 9\n",                               // once: run
        "on: hubVisit\npriority: 9\nonce: user\n",                   // once: user
        "on: hubVisit\npriority: 9\nonce: false\nwhen: 'user.runs > 1'\n", // undecided
        "on: hubVisit\npriority: 9\nonce: false\nafter: 'visited(\"hades.b\")'\n", // gated
    ] {
        let a = scene("hades.a", a_fm);
        let p = project(&[("a.lute", &a), ("b.lute", &b)], with_occasions());
        assert!(shadow_codes(&p).is_empty(), "{a_fm}: {:?}", shadow_codes(&p));
    }
    let a = scene("hades.a", "on: hubVisit\npriority: 9\nonce: false\nwhen: 'true'\n");
    let p = project(&[("a.lute", &a), ("b.lute", &b)], with_occasions());
    assert_eq!(shadow_codes(&p).len(), 1, "a `when` deciding true is always eligible");
}

#[test]
fn select_all_occasions_are_never_shadowed() {
    let a = scene("hades.a", "on: inbox\npriority: 9\nonce: false\n");
    let b = scene("hades.b", "on: inbox\n");
    let p = project(&[("a.lute", &a), ("b.lute", &b)], with_occasions());
    assert!(shadow_codes(&p).is_empty(), "the player picks from the whole list");
}

#[test]
fn an_unconditional_entry_beat_shadows_because_entries_never_spend() {
    let barks = lore(
        "<entry id=\"bark\" on=\"talk\" target=\"npc.achilles\" priority=\"20\">\n\
         @narrator: Lad.\n</entry>\n\
         <entry id=\"later\" on=\"talk\" target=\"npc.achilles\">\n@narrator: Hm.\n</entry>\n",
    );
    let b = scene("hades.b", "on: talk\ntarget: npc.achilles\npriority: 5\n");
    let p = project(&[("barks.lute", &barks), ("b.lute", &b)], with_occasions());
    // Reported in selection order: priority 5 before priority 0.
    let got = shadow_codes(&p);
    assert_eq!(got.len(), 2, "{got:?}");
    assert_eq!(got[0].0, "b.lute");
    assert!(got[0].1.starts_with("scene `hades.b` can never win"), "{}", got[0].1);
    assert!(got[0].1.contains("entry `bark` (priority 20)"), "{}", got[0].1);
    assert!(got[0].1.contains("(an entry without `once`)"), "{}", got[0].1);
    assert_eq!(got[1].0, "barks.lute");
    assert!(got[1].1.starts_with("entry `later` can never win"), "{}", got[1].1);

    let guarded = lore(
        "<entry id=\"bark\" on=\"talk\" priority=\"20\" when=\"entry.bark.read == false\">\n\
         @narrator: Lad.\n</entry>\n",
    );
    let p = project(&[("barks.lute", &guarded), ("b.lute", &b)], with_occasions());
    assert!(shadow_codes(&p).is_empty(), "an entry heard once guards on its own read");
}
