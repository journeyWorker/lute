//! dsl 0.24.0 §4: a cast entry's `present:` (`W-CAST-ABSENT` on a line whose
//! enclosing guards do not imply it) and `emotions:` (`E-BAD-ENUM` for an
//! `emotion=` outside it), plus both keys' declaration checks.

use std::path::{Path, PathBuf};

use lute_check::connectivity::{
    ambiguous_quest_ids, assemble_graph, check_reachability, live_assert_sites, quest_id_set,
    scene_key_set, unreachable_quest_ids,
};
use lute_check::{
    check, compute_must, fold_env, stable_seeds, CheckInput, FactEnv, FoldedEnv, GroundFact,
    MaySet, MetaKind, Mode, RootVocab, SchemaImports,
};
use lute_core_span::{Diagnostic, Severity, Span};
use lute_manifest::schema::{CastMember, OccasionDecl, OccasionSelect};
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_syntax::ast::Document;

const ABSENT: &str = "W-CAST-ABSENT";

/// The inline vocabulary every fixture scene declares.
const VOCAB: &str = "entities:\n  companion: { members: [isolde, corvin] }\n\
    relations:\n  inParty: { args: [companion], tier: run }\n\
    enums:\n  emotion: [calm, fierce, sad]\n\
    state:\n  run.x: { type: number, default: 0 }\n  run.withUs: { type: bool, default: false }\n\
    defs:\n  isoldeHere: { type: bool, cel: \"holds(inParty(isolde))\" }\n";

fn member(id: &str, present: Option<&str>, emotions: Option<&[&str]>) -> CastMember {
    CastMember {
        id: id.into(),
        name: None,
        present: present.map(str::to_string),
        emotions: emotions.map(|es| es.iter().map(|e| e.to_string()).collect()),
        assume: None,
    }
}

/// The core snapshot plus a plugin cast — `isolde` present while in the
/// party, `corvin` while `run.withUs`, `maud` always — and one occasion.
fn snapshot() -> CapabilitySnapshot {
    let mut snap = lute_manifest::core::load_core_snapshot();
    for m in [
        member(
            "isolde",
            Some("holds(inParty(isolde))"),
            Some(&["calm", "fierce"]),
        ),
        member("corvin", Some("run.withUs == true"), None),
        member("maud", None, None),
    ] {
        snap.cast.insert(m.id.clone(), m);
    }
    snap.occasions.insert(
        "hubVisit".into(),
        OccasionDecl {
            name: "hubVisit".into(),
            select: OccasionSelect::First,
            ..Default::default()
        },
    );
    snap
}

fn input(text: &str, snapshot: CapabilitySnapshot) -> CheckInput {
    CheckInput {
        text: text.to_string(),
        uri: "cast".into(),
        snapshot,
        providers: Default::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
        defaults: Default::default(),
    }
}

/// A scene with id `id`, extra frontmatter `fm` (newline-terminated) and
/// one shot of `body`.
fn scene_as(id: &str, fm: &str, body: &str) -> String {
    format!("---\nkind: scene\nid: {id}\n{fm}{VOCAB}---\n## Shot 1.\n{body}\n")
}

fn scene(body: &str) -> String {
    scene_as("a.one", "", body)
}

fn diags(text: &str) -> Vec<Diagnostic> {
    check(&input(text, snapshot())).diagnostics
}

fn with_code<'a>(ds: &'a [Diagnostic], code: &str) -> Vec<&'a Diagnostic> {
    ds.iter().filter(|d| d.code == code).collect()
}

fn anchored<'s>(src: &'s str, d: &Diagnostic) -> &'s str {
    &src[d.span.byte_start..d.span.byte_end]
}

/// A silent verdict must come from the analysis, not from a fixture whose
/// vocabulary failed to resolve.
fn assert_clean_vocab(ds: &[Diagnostic]) {
    assert!(
        !ds.iter().any(|d| d.code.starts_with("E-RELATION")
            || d.code == "E-UNDECLARED"
            || d.code == "E-UNDECLARED-REF"
            || d.code == "E-CEL-PARSE"
            || d.code == "E-CAST-UNKNOWN"),
        "fixture must resolve: {ds:?}"
    );
}

// --- W-CAST-ABSENT, single-file ---------------------------------------------

#[test]
fn an_unguarded_line_by_a_present_speaker_warns_once() {
    let src = scene("@isolde: Onward.\n@maud: Tea?\n@narrator: Wind.");
    let ds = diags(&src);
    assert_clean_vocab(&ds);
    let hits = with_code(&ds, ABSENT);
    assert_eq!(hits.len(), 1, "{ds:?}");
    assert_eq!(hits[0].severity, Severity::Warning);
    assert_eq!(anchored(&src, hits[0]), "isolde");
    let msg = &hits[0].message;
    assert!(msg.contains("present: \"holds(inParty(isolde))\""), "{msg}");
    assert!(
        msg.contains("@isolde{when=\"holds(inParty(isolde))\"}"),
        "{msg}"
    );
}

#[test]
fn the_lines_own_when_implies_presence() {
    let src = scene(
        "@isolde{when=\"holds(inParty(isolde))\"}: Onward.\n\
         @isolde{when=\"holds(inParty(corvin))\"}: Wrong guard.",
    );
    let ds = diags(&src);
    assert_clean_vocab(&ds);
    let hits = with_code(&ds, ABSENT);
    assert_eq!(
        hits.len(),
        1,
        "only the line guarded by someone else's presence: {ds:?}"
    );
    assert!(src[hits[0].span.byte_start..].starts_with("isolde{when=\"holds(inParty(corvin))"));
}

#[test]
fn a_scene_beat_when_conjunction_implies_presence() {
    let src = scene_as(
        "a.one",
        "on: hubVisit\nwhen: \"holds(inParty(isolde)) && run.x >= 1\"\n",
        "@isolde: The fire is warm.",
    );
    let ds = diags(&src);
    assert_clean_vocab(&ds);
    assert!(with_code(&ds, ABSENT).is_empty(), "{ds:?}");
    // The same line without the beat guard warns.
    assert_eq!(
        with_code(&diags(&scene("@isolde: The fire is warm.")), ABSENT).len(),
        1
    );
}

#[test]
fn a_def_guard_expanding_to_presence_implies_it() {
    let src = scene(
        "@isolde{when=\"@isoldeHere\"}: Here.\n\
         <branch id=\"b\">\n<choice id=\"c\" label=\"Ask her\" when=\"@isoldeHere && run.x > 0\">\n\
         @isolde: Ask away.\n</choice>\n<choice id=\"d\" label=\"Leave\">\n@isolde: Fine.\n</choice>\n</branch>",
    );
    let ds = diags(&src);
    assert_clean_vocab(&ds);
    let hits = with_code(&ds, ABSENT);
    assert_eq!(hits.len(), 1, "only the unguarded choice: {ds:?}");
    assert!(src[hits[0].span.byte_start..].starts_with("isolde: Fine."));
}

#[test]
fn a_match_arm_on_the_presence_state_implies_it() {
    let src = scene(
        "<match on=\"run.withUs\">\n<when is=\"true\">\n@corvin: With you.\n</when>\n\
         <otherwise>\n@corvin: Alone.\n</otherwise>\n</match>",
    );
    let ds = diags(&src);
    assert_clean_vocab(&ds);
    let hits = with_code(&ds, ABSENT);
    assert_eq!(
        hits.len(),
        1,
        "the `<otherwise>` arm is where he is absent: {ds:?}"
    );
    assert!(src[hits[0].span.byte_start..].starts_with("corvin: Alone."));
    let msg = &hits[0].message;
    assert!(msg.contains("present: \"run.withUs == true\""), "{msg}");
    assert!(
        !msg.contains("check-project"),
        "no fact query, no project note: {msg}"
    );
}

#[test]
fn an_otherwise_arm_assumes_every_earlier_arm_failed() {
    let src = scene(
        "<match on=\"run.withUs\">\n<when is=\"false\">\n@narrator: Alone.\n</when>\n\
         <otherwise>\n@corvin: With you.\n</otherwise>\n</match>",
    );
    let ds = diags(&src);
    assert_clean_vocab(&ds);
    assert!(with_code(&ds, ABSENT).is_empty(), "{ds:?}");
}

#[test]
fn a_write_between_the_guard_and_the_line_voids_the_guard() {
    let set = scene(
        "<match on=\"run.withUs\">\n<when is=\"true\">\n::set{run.withUs = false}\n\
         @corvin: Goodbye.\n</when>\n<otherwise>\n@narrator: Quiet.\n</otherwise>\n</match>",
    );
    assert_eq!(
        with_code(&diags(&set), ABSENT).len(),
        1,
        "a `::set` of the guarded path"
    );
    let retract = scene(
        "<branch id=\"b\">\n<choice id=\"c\" label=\"Part\" when=\"holds(inParty(isolde))\">\n\
         ::retract{inParty(isolde)}\n@isolde: Farewell.\n</choice>\n\
         <choice id=\"d\" label=\"Stay\">\n@narrator: Nothing.\n</choice>\n</branch>",
    );
    let ds = diags(&retract);
    assert_clean_vocab(&ds);
    assert_eq!(
        with_code(&ds, ABSENT).len(),
        1,
        "a `::retract` of the guarded fact: {ds:?}"
    );
}

#[test]
fn a_plugin_present_that_does_not_parse_is_reported_once_and_not_decided() {
    let mut snap = snapshot();
    snap.cast.insert(
        "isolde".into(),
        member("isolde", Some("holds(inParty(isolde)"), None),
    );
    let src = scene("@isolde: One.\n@isolde: Two.");
    let ds = check(&input(&src, snap)).diagnostics;
    let parse = with_code(&ds, "E-CEL-PARSE");
    assert_eq!(parse.len(), 1, "{ds:?}");
    assert_eq!(anchored(&src, parse[0]), "isolde");
    assert!(
        parse[0].message.contains("cast `isolde` `present:`"),
        "{}",
        parse[0].message
    );
    assert!(with_code(&ds, ABSENT).is_empty(), "{ds:?}");
}

// --- W-CAST-ABSENT under the fact Must set (check-project) ------------------

/// One resolved root analyzed like `check-project`: per-file `check()`,
/// then the fact envelope, then the presence reconciliation.
fn project(texts: &[(&str, &str)]) -> Vec<(String, Vec<Diagnostic>)> {
    project_in(snapshot, texts)
}

fn project_in(
    snap: fn() -> CapabilitySnapshot,
    texts: &[(&str, &str)],
) -> Vec<(String, Vec<Diagnostic>)> {
    let mut docs: Vec<(PathBuf, Document)> = Vec::new();
    let mut foldeds: Vec<FoldedEnv> = Vec::new();
    let mut results = Vec::new();
    for (path, text) in texts {
        let input = input(text, snap());
        let (doc, _) = lute_syntax::parse(&input.text);
        let (folded, _, _) = fold_env(&doc, &input);
        results.push((PathBuf::from(path), check(&input)));
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
    let may = MaySet::build(&vocab, facts, &stable_seeds(&docs, &vocab));
    let folded_refs: Vec<&FoldedEnv> = foldeds.iter().collect();
    let must = compute_must(&docs, &folded_refs, &graph, &vocab, &may);
    let env = FactEnv::new(may, must.slots);
    let ladder = lute_check::beats::presence_ladder(&docs, &folded_refs);
    let producers = lute_check::cast::fact_producers(&docs);
    let after = lute_check::cast::occasions_before(&docs, &folded_refs, &graph);
    let no_ladder = std::collections::BTreeMap::new();
    let no_after = std::collections::BTreeMap::new();
    results
        .into_iter()
        .zip(docs.iter().zip(&foldeds))
        .map(|((path, mut result), ((_, doc), folded))| {
            let project = lute_check::cast::PresenceProject {
                env: &env,
                ladder: ladder.get(&path).unwrap_or(&no_ladder),
                producers: &producers,
                after: after.get(&path).unwrap_or(&no_after),
            };
            let added = lute_check::cast::reconcile_presence(
                &mut result.diagnostics,
                Path::new(&path),
                doc,
                folded,
                &project,
            );
            result.diagnostics.extend(added);
            result.diagnostics.sort_by_key(|d| d.span.byte_start);
            (path.display().to_string(), result.diagnostics)
        })
        .collect()
}

#[test]
fn a_fact_asserted_on_every_route_discharges_presence_in_check_project() {
    let src = scene("::assert{inParty(isolde)}\n@isolde: I'm in.");
    // Single-file: no fact decides, so the line warns and says why.
    let ds = diags(&src);
    assert_clean_vocab(&ds);
    let hits = with_code(&ds, ABSENT);
    assert_eq!(hits.len(), 1, "{ds:?}");
    assert!(
        hits[0].message.contains("`lute check-project` does"),
        "{}",
        hits[0].message
    );
    // The project's Must set holds `inParty(isolde)` at the line.
    let out = project(&[("a.lute", &src)]);
    assert!(with_code(&out[0].1, ABSENT).is_empty(), "{:?}", out[0].1);
}

#[test]
fn a_fact_asserted_on_only_some_routes_still_warns_in_check_project() {
    let src = scene(
        "<branch id=\"b\">\n<choice id=\"c\" label=\"Recruit\">\n::assert{inParty(isolde)}\n</choice>\n\
         <choice id=\"d\" label=\"Refuse\">\n@narrator: Alone.\n</choice>\n</branch>\n\
         @isolde: Maybe I'm here.",
    );
    let out = project(&[("a.lute", &src)]);
    let hits = with_code(&out[0].1, ABSENT);
    assert_eq!(hits.len(), 1, "{:?}", out[0].1);
    assert_eq!(anchored(&src, hits[0]), "isolde");
    assert!(
        !hits[0].message.contains("single-file"),
        "the project verdict keeps the project wording: {}",
        hits[0].message
    );
}

// --- W-CAST-ABSENT precision (0.24 prerelease) ---------------------------------

/// A vocabulary with a derived party (`inParty` over recruited, departed and
/// the engine-reserved `fell`) and a `cel()`-only schedule (`at`).
const REL_VOCAB: &str = "entities:\n  companion: { members: [isolde, mara, wren, tomas] }\n  \
    person: { members: [sol] }\n  place: { members: [radio, roof] }\n  part: { members: [wire] }\n\
    relations:\n  recruited: { args: [companion], tier: run }\n  departed: { args: [companion], tier: run }\n  \
    fell: { args: [companion], tier: run, reserved: true }\n  inParty: { args: [companion], derive: true }\n  \
    at: { args: [person, place], derive: true }\n  carrying: { args: [part], tier: run }\n\
    rules:\n  - 'inParty(P) :- recruited(P), not departed(P), not fell(P)'\n  \
    - 'at(sol, radio) :- cel(\"run.x == 1\")'\n\
    state:\n  run.x: { type: number, default: 0 }\n";

/// [`snapshot`] plus speakers present over [`REL_VOCAB`]: `mara` while not
/// departed, `wren` in the party or not yet recruited, `sol` on his
/// schedule, `tomas` in the party with `assume: true`, and `quill` once the
/// `meet` entry has been read.
fn rel_snapshot() -> CapabilitySnapshot {
    let mut snap = snapshot();
    for mut m in [
        member("mara", Some("!holds(departed(mara))"), None),
        member(
            "wren",
            Some("holds(inParty(wren)) || !holds(recruited(wren))"),
            None,
        ),
        member("sol", Some("holds(at(sol, radio))"), None),
        member("tomas", Some("holds(inParty(tomas))"), None),
        member("quill", Some("entry.meet.everRead"), None),
    ] {
        m.assume = (m.id == "tomas").then_some(true);
        snap.cast.insert(m.id.clone(), m);
    }
    snap
}

fn rel_scene(fm: &str, body: &str) -> String {
    format!("---\nkind: scene\nid: a.rel\n{fm}{REL_VOCAB}---\n## Shot 1.\n{body}\n")
}

fn rel_diags(text: &str) -> Vec<Diagnostic> {
    check(&input(text, rel_snapshot())).diagnostics
}

/// The source text each `W-CAST-ABSENT` of `ds` anchors at, up to its `:`.
fn absent_lines<'s>(src: &'s str, ds: &[Diagnostic]) -> Vec<&'s str> {
    with_code(ds, ABSENT)
        .iter()
        .map(|d| {
            let rest = &src[d.span.byte_start..];
            &rest[..rest.find('\n').unwrap_or(rest.len())]
        })
        .collect()
}

#[test]
fn a_write_in_one_choice_does_not_reach_a_sibling_choice() {
    let src = rel_scene(
        "on: hubVisit\nwhen: \"!holds(recruited(wren))\"\n",
        "<branch id=\"ask\">\n<choice id=\"yes\" label=\"Join us\">\n@wren: Gladly.\n::assert{recruited(wren)}\n</choice>\n\
         <choice id=\"no\" label=\"Stay\">\n@wren: I'll stay.\n</choice>\n</branch>\n@wren: After the branch.",
    );
    let ds = rel_diags(&src);
    assert_clean_vocab(&ds);
    // Only after the join: the `yes` path asserted what the guard denies.
    assert_eq!(
        absent_lines(&src, &ds),
        ["wren: After the branch."],
        "{ds:?}"
    );
}

#[test]
fn an_assert_invalidates_only_a_guard_atom_it_can_falsify() {
    let src = rel_scene(
        "on: hubVisit\nwhen: \"holds(inParty(isolde)) && !holds(carrying(wire))\"\n",
        "::assert{recruited(tomas)}\n@isolde: Another pair of hands.\n\
         ::assert{recruited(isolde)}\n@isolde: A positive premise only helps.\n\
         ::assert{carrying(wire)}\n@isolde: Another conjunct of the guard fell, not mine.\n\
         ::assert{departed(isolde)}\n@isolde: Now I may be gone.",
    );
    let ds = rel_diags(&src);
    assert_clean_vocab(&ds);
    assert_eq!(
        absent_lines(&src, &ds),
        ["isolde: Now I may be gone."],
        "{ds:?}"
    );
}

#[test]
fn a_derived_guard_implies_its_rule_premises() {
    let src = rel_scene(
        "on: hubVisit\nwhen: \"holds(inParty(mara))\"\n",
        "@mara: With you.\n@wren: Me too?",
    );
    let ds = rel_diags(&src);
    assert_clean_vocab(&ds);
    // `inParty(mara)` implies `!departed(mara)`; it says nothing of Wren.
    assert_eq!(absent_lines(&src, &ds), ["wren: Me too?"], "{ds:?}");
}

#[test]
fn a_cel_only_schedule_is_read_as_its_guard() {
    let on = rel_scene(
        "on: hubVisit\nwhen: \"run.x == 1\"\n",
        "@sol: Morning.\n::set{run.x = 2}\n@sol: Later.",
    );
    let ds = rel_diags(&on);
    assert_clean_vocab(&ds);
    // The `::set` rewrites what the schedule reads.
    assert_eq!(absent_lines(&on, &ds), ["sol: Later."], "{ds:?}");
    let off = rel_scene(
        "on: hubVisit\nwhen: \"run.x == 2\"\n",
        "@sol: Not my shift.",
    );
    assert_eq!(absent_lines(&off, &rel_diags(&off)), ["sol: Not my shift."]);
    let atom = rel_scene(
        "on: hubVisit\nwhen: \"holds(at(sol, radio))\"\n",
        "@sol: Here.\n::set{run.x = 3}\n@sol: The schedule moved.",
    );
    assert_eq!(
        absent_lines(&atom, &rel_diags(&atom)),
        ["sol: The schedule moved."]
    );
}

#[test]
fn a_voice_over_line_is_exempt_and_an_off_screen_line_is_not() {
    let src = scene("@isolde{vo}: A letter in her hand.\n@isolde{os}: From the next room.");
    let ds = diags(&src);
    assert_clean_vocab(&ds);
    assert_eq!(
        absent_lines(&src, &ds),
        ["isolde{os}: From the next room."],
        "{ds:?}"
    );
}

#[test]
fn a_quest_handler_assumes_its_stable_start_and_an_entry_its_own_read() {
    let quest = "---\nkind: quest\nid: q\ntitle: q\n---\n\
        <quest id=\"memory\" title=\"Memory\" start=\"entry.meet.everRead\">\n\
        <objective id=\"o\" title=\"o\" done=\"run.x >= 1\"/>\n\
        <on event=\"questActive\">\n@quill: I remember a little more.\n</on>\n</quest>\n\
        <quest id=\"other\" title=\"Other\" start=\"run.x >= 1\">\n\
        <objective id=\"o2\" title=\"o\" done=\"run.x >= 2\"/>\n\
        <on event=\"questActive\">\n@quill: Too soon.\n</on>\n</quest>\n";
    let ds = check(&input(quest, rel_snapshot())).diagnostics;
    assert_eq!(absent_lines(quest, &ds), ["quill: Too soon."], "{ds:?}");
    let lore = "---\nkind: lore\nid: l\n---\n\
        <entry id=\"meet\" on=\"hubVisit\" category=\"bark\" priority=\"30\" once=\"user\">\n\
        @quill: A ghost at the figurehead.\n</entry>\n";
    let ds = check(&input(lore, rel_snapshot())).diagnostics;
    assert!(with_code(&ds, ABSENT).is_empty(), "{ds:?}");
}

#[test]
fn a_beat_below_an_always_eligible_once_user_intro_assumes_it_was_read() {
    let lore = "---\nkind: lore\nid: l\n---\n\
        <entry id=\"meet\" on=\"hubVisit\" category=\"bark\" priority=\"30\" once=\"user\">\n\
        @narrator: A ghost at the figurehead.\n</entry>\n\
        <entry id=\"later\" on=\"hubVisit\" category=\"bark\" priority=\"20\" once=\"user\" when=\"run.x >= 2\">\n\
        @quill: My name was Quill.\n</entry>\n\
        <entry id=\"above\" on=\"hubVisit\" category=\"bark\" priority=\"40\" once=\"user\" when=\"run.x >= 2\">\n\
        @quill: Ranked above the meeting.\n</entry>\n";
    // Single-file: no ladder.
    let ds = check(&input(lore, rel_snapshot())).diagnostics;
    assert_eq!(absent_lines(lore, &ds).len(), 2, "{ds:?}");
    // The project ladder: `later` wins only once `meet` is spent.
    let out = project_in(rel_snapshot, &[("l.lute", lore)]);
    assert_eq!(
        absent_lines(lore, &out[0].1),
        ["quill: Ranked above the meeting."],
        "{:?}",
        out[0].1
    );
}

#[test]
fn assume_reads_a_negated_reserved_relation_as_holding() {
    let src = rel_scene(
        "",
        "::assert{recruited(tomas)}\n::assert{recruited(isolde)}\n@tomas: I'm in.\n@isolde: So am I.",
    );
    let out = project_in(rel_snapshot, &[("a.lute", &src)]);
    assert_clean_vocab(&out[0].1);
    // Both are recruited and nobody departs; only `tomas` (`assume: true`)
    // takes the engine-reserved `fell` as absent.
    assert_eq!(
        absent_lines(&src, &out[0].1),
        ["isolde: So am I."],
        "{:?}",
        out[0].1
    );
}

#[test]
fn a_disjunct_proven_by_a_fact_only_the_unit_itself_asserts_counts() {
    // Wren is present "in the party, or not yet recruited". Only this
    // `once: run` scene recruits her, after its first line and in one
    // choice, so `!holds(recruited(wren))` holds before and beside it —
    // even though another scene can make her depart and `fell` is reserved.
    let meet = rel_scene(
        "on: hubVisit\n",
        "@wren: Before anyone asks.\n<branch id=\"ask\">\n<choice id=\"yes\" label=\"Join us\">\n\
         ::assert{recruited(wren)}\n</choice>\n<choice id=\"no\" label=\"Stay\">\n@wren: I'll stay.\n</choice>\n\
         </branch>\n@wren: After the branch.",
    );
    let leaves = rel_scene(
        "on: hubVisit\npriority: 5\nwhen: \"holds(inParty(wren))\"\n",
        "::assert{departed(wren)}",
    )
    .replace("id: a.rel", "id: a.leaves");
    let out = project_in(
        rel_snapshot,
        &[("meet.lute", &meet), ("leaves.lute", &leaves)],
    );
    assert_clean_vocab(&out[0].1);
    assert_eq!(
        absent_lines(&meet, &out[0].1),
        ["wren: After the branch."],
        "{:?}",
        out[0].1
    );
    // A second producer anywhere else voids the assumption.
    let recruits = leaves.replace("::assert{departed(wren)}", "::assert{recruited(wren)}");
    let out = project_in(
        rel_snapshot,
        &[("meet.lute", &meet), ("recruits.lute", &recruits)],
    );
    assert_eq!(absent_lines(&meet, &out[0].1).len(), 3, "{:?}", out[0].1);
}

// --- dsl 0.25.0 §6: `changedOn` ---------------------------------------------

/// [`rel_snapshot`] plus the `battleEnd` occasion and the world event its
/// raise fires.
fn battle_snapshot() -> CapabilitySnapshot {
    let mut snap = rel_snapshot();
    snap.occasions.insert(
        "battleEnd".into(),
        OccasionDecl {
            name: "battleEnd".into(),
            select: OccasionSelect::First,
            ..Default::default()
        },
    );
    snap.events.insert(
        "battleEnd".into(),
        lute_manifest::schema::EventDecl {
            name: "battleEnd".into(),
        },
    );
    snap
}

/// A [`REL_VOCAB`] scene `id` whose engine-reserved `fell` changes on `battleEnd`.
fn battle_scene(id: &str, fm: &str, body: &str) -> String {
    rel_scene(fm, body)
        .replace("id: a.rel", &format!("id: {id}"))
        .replace(
            "reserved: true }",
            "reserved: true, changedOn: [battleEnd] }",
        )
}

/// A `tomas` line guarded by every premise of his `present` but `fell`.
const TOMAS: &str = "@tomas{when=\"holds(recruited(tomas)) && !holds(departed(tomas))\"}";

/// The spoken text of every `W-CAST-ABSENT` line of `ds`.
fn absent_said<'s>(src: &'s str, ds: &[Diagnostic]) -> Vec<&'s str> {
    absent_lines(src, ds)
        .into_iter()
        .map(|l| l.rsplit(": ").next().unwrap_or(l))
        .collect()
}

#[test]
fn changed_on_takes_assume_away_where_its_occasion_is_presented() {
    let march = battle_scene(
        "a.march",
        "on: hubVisit\n",
        &format!("{TOMAS}: Before the battle."),
    );
    let ds = check(&input(&march, battle_snapshot())).diagnostics;
    assert_clean_vocab(&ds);
    assert!(with_code(&ds, ABSENT).is_empty(), "{ds:?}");
    let field = battle_scene(
        "a.field",
        "on: battleEnd\n",
        &format!("{TOMAS}: After the battle."),
    );
    let ds = check(&input(&field, battle_snapshot())).diagnostics;
    assert_clean_vocab(&ds);
    assert_eq!(absent_said(&field, &ds), ["After the battle."], "{ds:?}");
    let hit = with_code(&ds, ABSENT)[0];
    assert!(
        hit.message.contains("`assume: true` does not cover `fell`"),
        "{}",
        hit.message
    );
    // Without `changedOn`, 0.24: `assume` covers the battle scene as well.
    let plain = field.replace(", changedOn: [battleEnd]", "");
    assert!(with_code(
        &check(&input(&plain, battle_snapshot())).diagnostics,
        ABSENT
    )
    .is_empty());
    // A quest handler on the occasion's world event runs on the raise.
    let quest = format!(
        "---\nkind: quest\nid: q\ntitle: q\n{}---\n\
         <quest id=\"road\" title=\"Road\" start=\"run.x >= 0\">\n\
         <objective id=\"o\" title=\"o\" done=\"run.x >= 1\"/>\n\
         <on event=\"battleEnd\">\n{TOMAS}: The field is quiet.\n</on>\n\
         <on event=\"questActive\">\n{TOMAS}: On the road.\n</on>\n</quest>\n",
        REL_VOCAB.replace(
            "reserved: true }",
            "reserved: true, changedOn: [battleEnd] }"
        )
    );
    let ds = check(&input(&quest, battle_snapshot())).diagnostics;
    assert_clean_vocab(&ds);
    assert_eq!(absent_said(&quest, &ds), ["The field is quiet."], "{ds:?}");
}

#[test]
fn changed_on_takes_assume_away_after_the_occasion_in_check_project() {
    // Tomas joins on the march (else the project knows his guard never holds).
    let march = battle_scene(
        "a.march",
        "on: hubVisit\n",
        &format!("::assert{{recruited(tomas)}}\n{TOMAS}: Before the battle."),
    );
    let field = battle_scene(
        "a.field",
        "on: battleEnd\nafter: 'visited(\"a.march\")'\n",
        &format!("{TOMAS}: After the battle."),
    );
    let camp = battle_scene(
        "a.camp",
        "on: hubVisit\nafter: 'visited(\"a.field\")'\n",
        &format!("{TOMAS}: Days later."),
    );
    let road = battle_scene(
        "a.road",
        "on: hubVisit\nafter: 'visited(\"a.march\")'\n",
        &format!("{TOMAS}: Not after the battle."),
    );
    // Single-file cannot order the camp after the battle.
    assert!(with_code(&check(&input(&camp, battle_snapshot())).diagnostics, ABSENT).is_empty());
    let out = project_in(
        battle_snapshot,
        &[
            ("march.lute", &march),
            ("field.lute", &field),
            ("camp.lute", &camp),
            ("road.lute", &road),
        ],
    );
    for (_, ds) in &out {
        assert_clean_vocab(ds);
    }
    assert!(with_code(&out[0].1, ABSENT).is_empty(), "{:?}", out[0].1);
    assert_eq!(
        absent_said(&field, &out[1].1),
        ["After the battle."],
        "{:?}",
        out[1].1
    );
    assert_eq!(
        absent_said(&camp, &out[2].1),
        ["Days later."],
        "{:?}",
        out[2].1
    );
    assert!(with_code(&out[3].1, ABSENT).is_empty(), "{:?}", out[3].1);
}

/// ER C2 (0.25 prerelease): a guard that needs a `fell` fact — which only
/// `battleEnd` writes — puts the code it guards after the battle, whatever
/// its occasion or `after` edges.
#[test]
fn a_guard_needing_a_changed_on_fact_takes_assume_away() {
    let said = |fm: &str, body: &str| {
        let src = battle_scene("a.grief", fm, body);
        let ds = check(&input(&src, battle_snapshot())).diagnostics;
        assert_clean_vocab(&ds);
        absent_said(&src, &ds)
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>()
    };
    // The unit's own `when`, directly and through `count`.
    let unit = said(
        "on: hubVisit\nwhen: \"holds(fell(isolde))\"\n",
        &format!("{TOMAS}: Two days quiet."),
    );
    assert_eq!(unit, ["Two days quiet."]);
    let counted = said(
        "on: hubVisit\nwhen: \"count(fell(_)) >= 1\"\n",
        &format!("{TOMAS}: Cairns."),
    );
    assert_eq!(counted, ["Cairns."]);
    // A line's own guard, and only that line.
    let line = said(
        "on: hubVisit\n",
        "@tomas{when=\"holds(recruited(tomas)) && !holds(departed(tomas)) && holds(fell(isolde))\"}: She's gone.\n\
         @tomas{when=\"holds(recruited(tomas)) && !holds(departed(tomas))\"}: Morning.",
    );
    assert_eq!(line, ["She's gone."]);
    // A guard that holds without a `fell` fact proves nothing.
    let either = said(
        "on: hubVisit\nwhen: \"holds(fell(isolde)) || run.x == 1\"\n",
        &format!("{TOMAS}: Maybe."),
    );
    assert!(either.is_empty(), "{either:?}");
}

#[test]
fn changed_on_needs_a_reserved_relation_and_a_declared_occasion() {
    let decl_errors = |fell: &str| -> Vec<String> {
        let src = rel_scene("", "@narrator: Hm.").replace(
            "fell: { args: [companion], tier: run, reserved: true }",
            fell,
        );
        check(&input(&src, battle_snapshot()))
            .diagnostics
            .into_iter()
            .filter(|d| d.code == "E-RELATION-DECL")
            .map(|d| d.message)
            .collect()
    };
    let ok = decl_errors(
        "fell: { args: [companion], tier: run, reserved: true, changedOn: [battleEnd] }",
    );
    assert!(ok.is_empty(), "{ok:?}");
    let unreserved = decl_errors("fell: { args: [companion], tier: run, changedOn: [battleEnd] }");
    assert!(
        unreserved.len() == 1 && unreserved[0].contains("not `reserved: true`"),
        "{unreserved:?}"
    );
    let typo = decl_errors(
        "fell: { args: [companion], tier: run, reserved: true, changedOn: [batleEnd] }",
    );
    assert!(
        typo.len() == 1
            && typo[0].contains(
                "`changedOn: batleEnd` is not a declared occasion — did you mean `battleEnd`?"
            ),
        "{typo:?}"
    );
}

// --- emotions -----------------------------------------------------------------

#[test]
fn an_emotion_outside_the_speakers_emotions_is_bad_enum() {
    let src = scene(
        "@isolde{when=\"@isoldeHere\" emotion=\"sad\"}: Hm.\n\
         @isolde{when=\"@isoldeHere\" emotion=\"fierce\"}: Ha.\n\
         @maud{emotion=\"sad\"}: Oh.",
    );
    let ds = diags(&src);
    assert_clean_vocab(&ds);
    let hits = with_code(&ds, "E-BAD-ENUM");
    assert_eq!(hits.len(), 1, "{ds:?}");
    assert_eq!(anchored(&src, hits[0]), "sad");
    assert!(hits[0].span.byte_start < src.find("emotion=\"fierce\"").unwrap());
    let msg = &hits[0].message;
    assert!(
        msg.contains("`isolde`") && msg.contains("calm, fierce"),
        "{msg}"
    );
}

#[test]
fn an_emotion_outside_the_emotion_enum_is_reported_once() {
    let src = scene("@isolde{when=\"@isoldeHere\" emotion=\"grumpy\"}: Hm.");
    let ds = diags(&src);
    let hits = with_code(&ds, "E-BAD-ENUM");
    assert_eq!(hits.len(), 1, "the enum's own check owns it: {hits:?}");
    assert!(
        !hits[0].message.contains("`isolde`'s emotions"),
        "{}",
        hits[0].message
    );
}

// --- declarations -------------------------------------------------------------

fn schema_meta(yaml: &str) -> lute_syntax::ast::Meta {
    lute_syntax::ast::Meta {
        raw_yaml: yaml.to_string(),
        span: Span {
            byte_start: 0,
            byte_end: yaml.len(),
            line: 1,
            column: 1,
            utf16_range: (0, 0),
        },
    }
}

#[test]
fn a_schema_cast_entry_checks_present_and_emotions_at_its_key() {
    let yaml = "enums:\n  emotion: [calm, fierce]\n\
                cast:\n  isolde: { name: Isolde, present: \"holds(inParty(isolde)) &&\", emotions: [calm, grumpy] }\n  \
                corvin: { present: \"size(run.party) > 1\" }\n  \
                maud: { name: Maud, present: \"run.x >= 1\", emotions: [calm] }\n";
    let meta = schema_meta(yaml);
    let (typed, ds) =
        lute_check::parse_meta_kind(&meta, &CapabilitySnapshot::default(), MetaKind::Schema);
    let at = |code: &str| -> Vec<&str> {
        ds.iter()
            .filter(|d| d.code == code)
            .map(|d| &yaml[d.span.byte_start..d.span.byte_end])
            .collect()
    };
    assert_eq!(at("E-CEL-PARSE"), ["isolde"], "{ds:?}");
    assert_eq!(at("E-BAD-ENUM"), ["isolde"], "{ds:?}");
    assert_eq!(at("E-CEL-PROFILE"), ["corvin"], "{ds:?}");
    let by_id = |id: &str| typed.cast.iter().find(|c| c.id == id).expect(id);
    // A faulty condition is dropped, never decided against.
    assert_eq!(by_id("isolde").present, None);
    assert_eq!(by_id("corvin").present, None);
    assert_eq!(by_id("maud").present.as_deref(), Some("run.x >= 1"));
    assert_eq!(
        by_id("maud").emotions.as_deref(),
        Some(&["calm".to_string()][..])
    );
    assert_eq!(by_id("isolde").name.as_deref(), Some("Isolde"));

    let typo = "cast:\n  isolde: { presnt: \"run.x >= 1\" }\n";
    let (_, ds) = lute_check::parse_meta_kind(
        &schema_meta(typo),
        &CapabilitySnapshot::default(),
        MetaKind::Schema,
    );
    let bad: Vec<&Diagnostic> = ds.iter().filter(|d| d.code == "E-META-VALUE").collect();
    assert_eq!(bad.len(), 1, "{ds:?}");
    assert!(bad[0].message.contains("presnt"), "{}", bad[0].message);
}
