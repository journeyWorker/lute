//! dsl 0.22.0 language rules: `owner: engine` (`E-ENGINE-OWNED-WRITE`, §1.2),
//! `<quest tier>` / `<entry once>` / `entry.<id>.everRead` and
//! `W-QUEST-HANDLER-DEAD` (§7), occasion target domains (§8), and the project
//! beat advisories `W-BEAT-PRIORITY-TIE` / `W-BEAT-ONCE-RUN-USER` (§13).

use std::path::PathBuf;

use lute_check::{
    check, check_project_beats, check_project_entry_refs, check_project_quest_handlers, fold_env,
    CheckInput, FoldedEnv, Mode, SchemaImports,
};
use lute_core_span::{Diagnostic, Severity};
use lute_manifest::schema::{OccasionDecl, OccasionSelect, OccasionTarget};
use lute_manifest::snapshot::CapabilitySnapshot;

fn snapshot() -> CapabilitySnapshot {
    let mut snap = lute_manifest::core::load_core_snapshot();
    let domain = OccasionTarget::Domain {
        prefix: "npc".into(),
        entity: "person".into(),
        members: None,
    };
    let bosses = |members: &[&str]| OccasionTarget::Domain {
        prefix: "boss".into(),
        entity: "foe".into(),
        members: Some(members.iter().map(|m| m.to_string()).collect()),
    };
    for (name, select, target) in [
        ("hubVisit", OccasionSelect::First, OccasionTarget::Shape(false)),
        ("talk", OccasionSelect::First, domain),
        ("examine", OccasionSelect::First, OccasionTarget::Shape(true)),
        (
            "visit",
            OccasionSelect::First,
            OccasionTarget::Domain {
                prefix: "place".into(),
                entity: "location".into(),
                members: None,
            },
        ),
        ("board", OccasionSelect::All, OccasionTarget::Shape(false)),
        ("bossDefeated", OccasionSelect::First, bosses(&["gatekeeper", "warden"])),
        ("bossFled", OccasionSelect::First, bosses(&["gatekeeper", "wardne"])),
    ] {
        snap.occasions.insert(
            name.into(),
            OccasionDecl {
                name: name.into(),
                select,
                target,
                description: None,
            },
        );
    }
    snap
}

fn input(text: &str) -> CheckInput {
    CheckInput {
        text: text.to_string(),
        uri: "harness".into(),
        snapshot: snapshot(),
        providers: Default::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
        defaults: Default::default(),
    }
}

fn diags(text: &str) -> Vec<Diagnostic> {
    check(&input(text)).diagnostics
}

fn with_code<'a>(ds: &'a [Diagnostic], code: &str) -> Vec<&'a Diagnostic> {
    ds.iter().filter(|d| d.code == code).collect()
}

fn anchored<'s>(src: &'s str, d: &Diagnostic) -> &'s str {
    &src[d.span.byte_start..d.span.byte_end]
}

const VOCAB: &str = "entities:\n  person: { members: [maud, oskar] }\n  location: { open: true }\n  \
                     foe: { members: [gatekeeper, warden, cinderhound] }\n";

/// A scene document with frontmatter lines `fm` and body `body`.
fn scene(id: &str, fm: &str, body: &str) -> String {
    format!("---\nkind: scene\nid: {id}\n{fm}{VOCAB}---\n## Shot 1.\n{body}")
}

fn lore(body: &str) -> String {
    format!("---\nkind: lore\ntitle: Barks\n{VOCAB}---\n{body}")
}

fn quest_doc(body: &str) -> String {
    format!(
        "---\nkind: quest\nstate:\n  run.n: {{ type: number, default: 0 }}\n---\n{body}"
    )
}

// --- §1.2 owner: engine ----------------------------------------------------

#[test]
fn content_set_of_an_engine_owned_path_is_an_error_and_reads_are_free() {
    let src = scene(
        "a.one",
        "state:\n  run.day: { type: number, default: 1, owner: engine }\n  \
         run.mood: { type: number, default: 0 }\n",
        "::set{run.day += 1}\n::set{run.mood = run.day}\n@narrator: Day {{run.day}}.\n",
    );
    let ds = diags(&src);
    let hits = with_code(&ds, "E-ENGINE-OWNED-WRITE");
    assert_eq!(hits.len(), 1, "{ds:?}");
    assert_eq!(hits[0].severity, Severity::Error);
    assert_eq!(anchored(&src, hits[0]), "run.day");
    assert!(hits[0].message.contains("`engine:` step"), "{}", hits[0].message);
    // The write short-circuits: no op/type or undeclared twin.
    assert!(with_code(&ds, "E-UNDECLARED").is_empty(), "{ds:?}");
}

#[test]
fn owner_other_than_engine_is_a_state_decl_error() {
    let src = scene(
        "a.two",
        "state:\n  run.day: { type: number, default: 1, owner: content }\n",
        "@narrator: Hi.\n",
    );
    let ds = diags(&src);
    let d = with_code(&ds, "E-STATE-DECL");
    assert_eq!(d.len(), 1, "{ds:?}");
    assert!(d[0].message.contains("`owner:` is `content`"), "{}", d[0].message);
}

#[test]
fn engine_owned_decl_is_lifted_onto_the_schema() {
    let src = scene(
        "a.three",
        "state:\n  run.day: { type: number, default: 1, owner: engine }\n",
        "@narrator: Hi.\n",
    );
    let input = input(&src);
    let (doc, _) = lute_syntax::parse(&input.text);
    let folded = fold_env(&doc, &input).0;
    assert_eq!(
        folded.env.state.decls["run.day"].owner,
        Some(lute_manifest::types::Owner::Engine)
    );
}

// --- §8 occasion target domains --------------------------------------------

#[test]
fn a_beat_target_outside_its_domain_is_beat_attr_with_did_you_mean() {
    let src = scene("a.talk", "on: talk\ntarget: npc.mauda\n", "@narrator: Hi.\n");
    let ds = diags(&src);
    let d = with_code(&ds, "E-BEAT-ATTR");
    assert_eq!(d.len(), 1, "{ds:?}");
    assert_eq!(anchored(&src, d[0]), "npc.mauda");
    assert!(d[0].message.contains("did you mean `npc.maud`?"), "{}", d[0].message);

    // In-domain, and a wrong prefix.
    assert!(with_code(&diags(&scene("a.ok", "on: talk\ntarget: npc.oskar\n", "@narrator: Hi.\n")), "E-BEAT-ATTR").is_empty());
    let wrong = diags(&scene("a.bad", "on: talk\ntarget: place.maud\n", "@narrator: Hi.\n"));
    assert_eq!(with_code(&wrong, "E-BEAT-ATTR").len(), 1, "{wrong:?}");
}

#[test]
fn entry_beat_targets_and_open_kinds_and_shape_only_targets() {
    let src = lore(
        "<entry id=\"a\" on=\"talk\" target=\"npc.sable\">\n@narrator: hi\n</entry>\n\
         <entry id=\"b\" on=\"visit\" target=\"place.anywhere\">\n@narrator: hi\n</entry>\n\
         <entry id=\"c\" on=\"examine\" target=\"thing.whatever\">\n@narrator: hi\n</entry>\n",
    );
    let ds = diags(&src);
    let d = with_code(&ds, "E-BEAT-ATTR");
    assert_eq!(d.len(), 1, "only the closed-kind miss: {ds:?}");
    assert_eq!(anchored(&src, d[0]), "npc.sable");
}

#[test]
fn a_member_subset_narrows_the_domain_with_did_you_mean_over_the_subset() {
    // ashen N10: `bossDefeated` is raised only for the two guardians. A
    // hound is a `foe`, but not one of the occasion's listed members.
    let src = scene("a.boss", "on: bossDefeated\ntarget: boss.cinderhound\n", "@narrator: Hi.\n");
    let ds = diags(&src);
    let d = with_code(&ds, "E-BEAT-ATTR");
    assert_eq!(d.len(), 1, "{ds:?}");
    assert_eq!(anchored(&src, d[0]), "boss.cinderhound");
    assert!(
        d[0].message.contains("member list (`boss.gatekeeper`, `boss.warden`)"),
        "{}",
        d[0].message
    );
    // The did-you-mean runs over the subset.
    let near = diags(&scene("a.near", "on: bossDefeated\ntarget: boss.wardn\n", "@narrator: Hi.\n"));
    let d = with_code(&near, "E-BEAT-ATTR");
    assert_eq!(d.len(), 1, "{near:?}");
    assert!(d[0].message.contains("did you mean `boss.warden`?"), "{}", d[0].message);
    // A listed member is legal.
    let ok = diags(&scene("a.ok", "on: bossDefeated\ntarget: boss.gatekeeper\n", "@narrator: Hi.\n"));
    assert!(with_code(&ok, "E-BEAT-ATTR").is_empty(), "{ok:?}");
}

#[test]
fn a_listed_member_outside_the_entity_kind_refuses_every_target() {
    // `bossFled` lists `wardne`, which `foe` does not declare: even a
    // well-spelled target names the stray member, the kind, and the fix.
    let src = scene("a.fled", "on: bossFled\ntarget: boss.gatekeeper\n", "@narrator: Hi.\n");
    let ds = diags(&src);
    let d = with_code(&ds, "E-BEAT-ATTR");
    assert_eq!(d.len(), 1, "{ds:?}");
    let msg = &d[0].message;
    assert!(msg.contains("occasion `bossFled` lists `wardne`"), "{msg}");
    assert!(msg.contains("not a member of entity kind `foe`"), "{msg}");
    assert!(msg.contains("did you mean `warden`?"), "{msg}");
}

#[test]
fn occasion_target_ok_is_shared_with_play() {
    let snap = snapshot();
    let mut kinds = std::collections::BTreeMap::new();
    kinds.insert(
        "person".to_string(),
        lute_manifest::relations::EntityKindDecl {
            shape: lute_manifest::relations::KindShape::Members(vec!["maud".into()]),
        },
    );
    let talk = &snap.occasions["talk"];
    assert!(lute_check::occasion_target_ok(talk, "npc.maud", &kinds).is_ok());
    assert!(lute_check::occasion_target_ok(talk, "npc.mud", &kinds)
        .unwrap_err()
        .contains("did you mean `npc.maud`?"));
    // Unknown entity kind.
    let visit = &snap.occasions["visit"];
    assert!(lute_check::occasion_target_ok(visit, "place.x", &kinds)
        .unwrap_err()
        .contains("does not declare"));
    // Shape-only occasions accept anything.
    assert!(lute_check::occasion_target_ok(&snap.occasions["examine"], "x.y", &kinds).is_ok());
}

// --- §7 run boundaries ------------------------------------------------------

#[test]
fn quest_tier_is_run_or_user() {
    let ok = diags(&quest_doc(
        "<quest id=\"weekly\" tier=\"run\">\n<objective id=\"o\" done=\"run.n > 1\"/>\n</quest>\n",
    ));
    assert!(with_code(&ok, "E-ATTR-TYPE").is_empty(), "{ok:?}");
    assert!(with_code(&ok, "E-UNKNOWN-ATTR").is_empty(), "{ok:?}");
    let src = quest_doc(
        "<quest id=\"weekly\" tier=\"week\">\n<objective id=\"o\" done=\"run.n > 1\"/>\n</quest>\n",
    );
    let bad = diags(&src);
    let d = with_code(&bad, "E-ATTR-TYPE");
    assert_eq!(d.len(), 1, "{bad:?}");
    assert_eq!(anchored(&src, d[0]), "week");
}

#[test]
fn entry_once_is_run_or_user_and_needs_on() {
    let ok = diags(&lore(
        "<entry id=\"a\" on=\"hubVisit\" once=\"user\">\n@narrator: hi\n</entry>\n",
    ));
    assert!(with_code(&ok, "E-BEAT-ATTR").is_empty(), "{ok:?}");
    assert!(with_code(&ok, "E-UNKNOWN-ATTR").is_empty(), "{ok:?}");
    let bad = diags(&lore(
        "<entry id=\"a\" on=\"hubVisit\" once=\"ever\">\n@narrator: hi\n</entry>\n\
         <entry id=\"b\" once=\"run\">\n@narrator: hi\n</entry>\n",
    ));
    let d = with_code(&bad, "E-BEAT-ATTR");
    assert_eq!(d.len(), 2, "{bad:?}");
    assert!(d.iter().any(|d| d.message.contains("`once=\"ever\"`")));
    assert!(d.iter().any(|d| d.message.contains("`once` requires `on`")));
}

#[test]
fn ever_read_is_a_readable_reserved_flag_and_unwritable() {
    let src = scene(
        "a.ever",
        "state:\n  run.x: { type: bool, default: false }\n",
        "<match on=\"entry.note.everRead\">\n<when is=\"true\">\n@narrator: again\n</when>\n\
         <otherwise>\n@narrator: first\n</otherwise>\n</match>\n::set{entry.note.everRead = true}\n",
    );
    let ds = diags(&src);
    assert!(with_code(&ds, "E-UNDECLARED").is_empty(), "{ds:?}");
    assert_eq!(with_code(&ds, "E-QUEST-RESERVED-WRITE").len(), 1, "{ds:?}");
    // check-project resolves the id like `entry.<id>.read`.
    let docs = vec![(PathBuf::from("s.lute"), lute_syntax::parse(&src).0)];
    let refs = check_project_entry_refs(&docs);
    assert_eq!(refs.len(), 1, "{refs:?}");
    assert_eq!(refs[0].1.code, "W-ENTRY-REF-UNKNOWN");
}

#[test]
fn quest_failed_handler_on_a_quest_that_cannot_fail_is_dead() {
    let handler = "<on event=\"questFailed\">\n@narrator: too late\n</on>\n";
    let dead = quest_doc(&format!(
        "<quest id=\"q\">\n<objective id=\"o\" done=\"run.n > 1\"/>\n{handler}</quest>\n"
    ));
    let live_fail = quest_doc(&format!(
        "<quest id=\"q\" fail=\"run.n > 5\">\n<objective id=\"o\" done=\"run.n > 1\"/>\n{handler}</quest>\n"
    ));
    // A required child that can fail fails the parent; the child itself has
    // a parent (cascade). A required child that cannot fail leaves the
    // parent's handler dead.
    let tree = quest_doc(&format!(
        "<quest id=\"p\">\n<objective id=\"o\" quest=\"c\"/>\n{handler}</quest>\n\
         <quest id=\"c\" fail=\"run.n > 5\">\n<objective id=\"o\" done=\"run.n > 1\"/>\n{handler}</quest>\n"
    ));
    let inert_child = quest_doc(&format!(
        "<quest id=\"p\">\n<objective id=\"o\" quest=\"c\"/>\n{handler}</quest>\n\
         <quest id=\"c\">\n<objective id=\"o\" done=\"run.n > 1\"/>\n{handler}</quest>\n"
    ));
    // dsl 0.23.0 §2: a missed REQUIRED `by=` deadline fails the quest; an
    // optional objective's miss spares it.
    let deadline = |optional: &str| {
        quest_doc(&format!(
            "<quest id=\"q\">\n<objective id=\"o\" done=\"run.n > 1\" by=\"run.n < 0\"{optional}/>\n{handler}</quest>\n"
        ))
    };
    let run = |src: &str| {
        let docs = vec![(PathBuf::from("q.lute"), lute_syntax::parse(src).0)];
        check_project_quest_handlers(&docs)
    };
    let out = run(&dead);
    assert_eq!(out.len(), 1, "{out:?}");
    assert_eq!(out[0].1.code, "W-QUEST-HANDLER-DEAD");
    assert_eq!(out[0].1.severity, Severity::Warning);
    assert_eq!(anchored(&dead, &out[0].1), "questFailed");
    assert!(run(&live_fail).is_empty());
    assert!(run(&tree).is_empty());
    let out = run(&inert_child);
    assert_eq!(out.len(), 1, "only the parent's handler: {out:?}");
    assert!(out[0].1.message.contains("quest `p`"), "{}", out[0].1.message);
    assert!(run(&deadline("")).is_empty());
    assert_eq!(run(&deadline(" optional")).len(), 1);
}

// --- §13 project beat advisories -------------------------------------------

fn project_beats(texts: &[&str]) -> Vec<(PathBuf, Diagnostic)> {
    let mut docs = Vec::new();
    let mut foldeds: Vec<FoldedEnv> = Vec::new();
    for (i, text) in texts.iter().enumerate() {
        let input = input(text);
        let (doc, _) = lute_syntax::parse(&input.text);
        foldeds.push(fold_env(&doc, &input).0);
        docs.push((PathBuf::from(format!("{i}.lute")), doc));
    }
    let refs: Vec<&FoldedEnv> = foldeds.iter().collect();
    check_project_beats(&docs, &refs)
}

const STATE: &str = "state:\n  run.day: { type: number, default: 1 }\n  \
    run.slot: { type: { enum: [morning, night] }, default: morning }\n  \
    user.runs: { type: number, default: 0 }\n";

fn beat(id: &str, fm: &str) -> String {
    scene(id, &format!("{fm}{STATE}"), "@narrator: Hi.\n")
}

fn codes(out: &[(PathBuf, Diagnostic)], code: &str) -> usize {
    out.iter().filter(|(_, d)| d.code == code).count()
}

#[test]
fn equal_priority_beats_whose_whens_can_overlap_tie() {
    let out = project_beats(&[
        &beat("a.one", "on: hubVisit\nwhen: 'run.day >= 2'\n"),
        &beat("a.two", "on: hubVisit\nwhen: 'run.slot == \"night\"'\n"),
    ]);
    assert_eq!(codes(&out, "W-BEAT-PRIORITY-TIE"), 1, "{out:?}");
    let d = &out.iter().find(|(_, d)| d.code == "W-BEAT-PRIORITY-TIE").unwrap();
    assert_eq!(d.0, PathBuf::from("1.lute"), "anchored at the later beat");
    assert!(d.1.message.contains("scene `a.one`"), "{}", d.1.message);
}

#[test]
fn exclusive_whens_different_priorities_targets_or_select_all_do_not_tie() {
    // Disjoint values of one path.
    let exclusive = project_beats(&[
        &beat("a.one", "on: hubVisit\nwhen: 'run.day == 1 && run.slot == \"night\"'\n"),
        &beat("a.two", "on: hubVisit\nwhen: 'run.slot == \"morning\"'\n"),
    ]);
    assert_eq!(codes(&exclusive, "W-BEAT-PRIORITY-TIE"), 0, "{exclusive:?}");
    let ranked = project_beats(&[
        &beat("a.one", "on: hubVisit\npriority: 1\nwhen: 'run.day >= 2'\n"),
        &beat("a.two", "on: hubVisit\nwhen: 'run.day >= 2'\n"),
    ]);
    assert_eq!(codes(&ranked, "W-BEAT-PRIORITY-TIE"), 0, "{ranked:?}");
    let targets = project_beats(&[
        &beat("a.one", "on: talk\ntarget: npc.maud\nwhen: 'run.day >= 2'\n"),
        &beat("a.two", "on: talk\ntarget: npc.oskar\nwhen: 'run.day >= 2'\n"),
    ]);
    assert_eq!(codes(&targets, "W-BEAT-PRIORITY-TIE"), 0, "{targets:?}");
    let offered = project_beats(&[
        &beat("a.one", "on: board\nwhen: 'run.day >= 2'\n"),
        &beat("a.two", "on: board\nwhen: 'run.day >= 2'\n"),
    ]);
    assert_eq!(codes(&offered, "W-BEAT-PRIORITY-TIE"), 0, "{offered:?}");
    // `holds(P)` against `!holds(P)` is exclusive too.
    let rel = "relations:\n  here: { args: [person] }\n";
    let facts = project_beats(&[
        &beat("a.one", &format!("on: hubVisit\nwhen: 'holds(here(maud))'\n{rel}")),
        &beat("a.two", &format!("on: hubVisit\nwhen: '!holds(here(maud))'\n{rel}")),
    ]);
    assert_eq!(codes(&facts, "W-BEAT-PRIORITY-TIE"), 0, "{facts:?}");
    let other = project_beats(&[
        &beat("a.one", &format!("on: hubVisit\nwhen: 'holds(here(maud))'\n{rel}")),
        &beat("a.two", &format!("on: hubVisit\nwhen: '!holds(here(oskar))'\n{rel}")),
    ]);
    assert_eq!(codes(&other, "W-BEAT-PRIORITY-TIE"), 1, "{other:?}");
    // An untargeted beat is a candidate for every target: it ties.
    let wide = project_beats(&[
        &beat("a.one", "on: talk\nwhen: 'run.day >= 2'\n"),
        &beat("a.two", "on: talk\ntarget: npc.oskar\nwhen: 'run.day >= 3'\n"),
    ]);
    assert_eq!(codes(&wide, "W-BEAT-PRIORITY-TIE"), 1, "{wide:?}");
}

#[test]
fn a_shadowed_beat_reports_shadowing_not_a_tie() {
    let out = project_beats(&[
        &beat("a.one", "on: hubVisit\nonce: false\n"),
        &beat("a.two", "on: hubVisit\nwhen: 'run.day >= 2'\n"),
    ]);
    assert_eq!(codes(&out, "W-BEAT-SHADOWED"), 1, "{out:?}");
    assert_eq!(codes(&out, "W-BEAT-PRIORITY-TIE"), 0, "{out:?}");
}

#[test]
fn once_run_beat_gated_only_on_user_state_is_advised() {
    let out = project_beats(&[&beat("a.one", "on: hubVisit\nwhen: 'user.runs >= 3'\n")]);
    assert_eq!(codes(&out, "W-BEAT-ONCE-RUN-USER"), 1, "{out:?}");
    let (_, d) = out.iter().find(|(_, d)| d.code == "W-BEAT-ONCE-RUN-USER").unwrap();
    assert!(d.message.contains("write `once: run` if it should replay every run"), "{}", d.message);
    // `once: user`, a run-tier read, a fact query, or no `when`: silent.
    // dsl 0.23.1 (ashen N1): so is an AUTHORED `once: run` — the author's
    // acknowledgement — and a `prev.run.*` read (run history, not user).
    for fm in [
        "on: hubVisit\nonce: user\nwhen: 'user.runs >= 3'\n",
        "on: hubVisit\nwhen: 'user.runs >= 3 && run.day == 1'\n",
        "on: hubVisit\nwhen: 'user.runs >= 3 && visited(\"a.two\")'\n",
        "on: hubVisit\n",
        "on: hubVisit\nonce: run\nwhen: 'user.runs >= 3'\n",
        "on: hubVisit\nwhen: 'user.runs >= 3 && isSet(prev.run.day)'\n",
    ] {
        let out = project_beats(&[&beat("a.one", fm)]);
        assert_eq!(codes(&out, "W-BEAT-ONCE-RUN-USER"), 0, "{fm}: {out:?}");
    }
    // An entry's `once="run"` is always written: silent.
    let src = lore(
        "<entry id=\"bark\" on=\"hubVisit\" once=\"run\" when=\"entry.bark.everRead\">\n\
         @narrator: hi\n</entry>\n",
    );
    let out = project_beats(&[&src]);
    assert_eq!(codes(&out, "W-BEAT-ONCE-RUN-USER"), 0, "{out:?}");
}
