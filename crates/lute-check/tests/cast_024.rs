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
    }
}

/// The core snapshot plus a plugin cast — `isolde` present while in the
/// party, `corvin` while `run.withUs`, `maud` always — and one occasion.
fn snapshot() -> CapabilitySnapshot {
    let mut snap = lute_manifest::core::load_core_snapshot();
    for m in [
        member("isolde", Some("holds(inParty(isolde))"), Some(&["calm", "fierce"])),
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
    assert!(msg.contains("@isolde{when=\"holds(inParty(isolde))\"}"), "{msg}");
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
    assert_eq!(hits.len(), 1, "only the line guarded by someone else's presence: {ds:?}");
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
    assert_eq!(with_code(&diags(&scene("@isolde: The fire is warm.")), ABSENT).len(), 1);
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
    assert_eq!(hits.len(), 1, "the `<otherwise>` arm is where he is absent: {ds:?}");
    assert!(src[hits[0].span.byte_start..].starts_with("corvin: Alone."));
    let msg = &hits[0].message;
    assert!(msg.contains("present: \"run.withUs == true\""), "{msg}");
    assert!(!msg.contains("check-project"), "no fact query, no project note: {msg}");
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
    assert_eq!(with_code(&diags(&set), ABSENT).len(), 1, "a `::set` of the guarded path");
    let retract = scene(
        "<branch id=\"b\">\n<choice id=\"c\" label=\"Part\" when=\"holds(inParty(isolde))\">\n\
         ::retract{inParty(isolde)}\n@isolde: Farewell.\n</choice>\n\
         <choice id=\"d\" label=\"Stay\">\n@narrator: Nothing.\n</choice>\n</branch>",
    );
    let ds = diags(&retract);
    assert_clean_vocab(&ds);
    assert_eq!(with_code(&ds, ABSENT).len(), 1, "a `::retract` of the guarded fact: {ds:?}");
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
    assert!(parse[0].message.contains("cast `isolde` `present:`"), "{}", parse[0].message);
    assert!(with_code(&ds, ABSENT).is_empty(), "{ds:?}");
}

// --- W-CAST-ABSENT under the fact Must set (check-project) ------------------

/// One resolved root analyzed like `check-project`: per-file `check()`,
/// then the fact envelope, then the presence reconciliation.
fn project(texts: &[(&str, &str)]) -> Vec<(String, Vec<Diagnostic>)> {
    let mut docs: Vec<(PathBuf, Document)> = Vec::new();
    let mut foldeds: Vec<FoldedEnv> = Vec::new();
    let mut results = Vec::new();
    for (path, text) in texts {
        let input = input(text, snapshot());
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
    results
        .into_iter()
        .zip(docs.iter().zip(&foldeds))
        .map(|((path, mut result), ((_, doc), folded))| {
            lute_check::cast::reconcile_presence(&mut result.diagnostics, Path::new(&path), doc, folded, &env);
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
    assert!(hits[0].message.contains("`lute check-project` does"), "{}", hits[0].message);
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
    assert!(msg.contains("`isolde`") && msg.contains("calm, fierce"), "{msg}");
}

#[test]
fn an_emotion_outside_the_emotion_enum_is_reported_once() {
    let src = scene("@isolde{when=\"@isoldeHere\" emotion=\"grumpy\"}: Hm.");
    let ds = diags(&src);
    let hits = with_code(&ds, "E-BAD-ENUM");
    assert_eq!(hits.len(), 1, "the enum's own check owns it: {hits:?}");
    assert!(!hits[0].message.contains("`isolde`'s emotions"), "{}", hits[0].message);
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
    let (typed, ds) = lute_check::parse_meta_kind(&meta, &CapabilitySnapshot::default(), MetaKind::Schema);
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
    assert_eq!(by_id("maud").emotions.as_deref(), Some(&["calm".to_string()][..]));
    assert_eq!(by_id("isolde").name.as_deref(), Some("Isolde"));

    let typo = "cast:\n  isolde: { presnt: \"run.x >= 1\" }\n";
    let (_, ds) = lute_check::parse_meta_kind(&schema_meta(typo), &CapabilitySnapshot::default(), MetaKind::Schema);
    let bad: Vec<&Diagnostic> = ds.iter().filter(|d| d.code == "E-META-VALUE").collect();
    assert_eq!(bad.len(), 1, "{ds:?}");
    assert!(bad[0].message.contains("presnt"), "{}", bad[0].message);
}
