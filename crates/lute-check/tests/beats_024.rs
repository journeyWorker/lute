//! dsl 0.24.0 project beat passes and project-wide references (round-3
//! triage): `W-BEAT-SHADOWED` over every target of a closed domain (T1-8),
//! `W-BEAT-PRIORITY-TIE` folding `once` and pure-schedule derived atoms
//! (T3-3), `W-BEAT-ONCE-RUN-USER` over user-tier relations and quests (T3-4),
//! scene frontmatter `when:` quest / entry references (T1-6), and an entry's
//! `target=` on an untargeted occasion as metadata (§6).

use std::path::PathBuf;

use lute_check::{
    check, check_project_beats, check_project_entry_refs, check_project_quest_refs, fold_env,
    CheckInput, FoldedEnv, Mode, SchemaImports,
};
use lute_core_span::{Diagnostic, Severity};
use lute_manifest::schema::{OccasionDecl, OccasionSelect, OccasionTarget};
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_syntax::ast::Document;

fn snapshot() -> CapabilitySnapshot {
    let mut snap = lute_manifest::core::load_core_snapshot();
    let domain = |prefix: &str, entity: &str| OccasionTarget::Domain {
        prefix: prefix.into(),
        entity: entity.into(),
        members: None,
    };
    for (name, target) in [
        ("hubVisit", OccasionTarget::Shape(false)),
        ("talk", domain("npc", "person")),
        ("visit", domain("place", "location")),
        ("roam", domain("area", "zone")),
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
    snap
}

fn input(text: &str) -> CheckInput {
    CheckInput {
        text: text.to_string(),
        uri: "beats024".into(),
        snapshot: snapshot(),
        providers: Default::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
        defaults: Default::default(),
    }
}

const VOCAB: &str = "entities:\n  person: { members: [maud, oskar] }\n  \
                     location: { members: [radio, deck] }\n  zone: { open: true }\n";

const STATE: &str = "state:\n  run.slot: { type: { enum: [morning, afternoon, night] }, default: morning }\n  \
                     user.bond: { type: number, default: 0 }\n";

fn scene(id: &str, fm: &str) -> String {
    format!("---\nkind: scene\nid: {id}\n{fm}{VOCAB}{STATE}---\n## Shot 1.\n@narrator: Hi.\n")
}

fn lore(fm: &str, body: &str) -> String {
    format!("---\nkind: lore\nid: lore.barks\ntitle: Barks\n{fm}{VOCAB}{STATE}---\n{body}")
}

fn quest_doc(body: &str) -> String {
    format!("---\nkind: quest\n---\n{body}")
}

fn parse_all(texts: &[&str]) -> (Vec<(PathBuf, Document)>, Vec<FoldedEnv>) {
    let mut docs = Vec::new();
    let mut foldeds = Vec::new();
    for (i, text) in texts.iter().enumerate() {
        let input = input(text);
        let (doc, _) = lute_syntax::parse(&input.text);
        foldeds.push(fold_env(&doc, &input).0);
        docs.push((PathBuf::from(format!("{i}.lute")), doc));
    }
    (docs, foldeds)
}

fn project_beats(texts: &[&str]) -> Vec<(PathBuf, Diagnostic)> {
    let (docs, foldeds) = parse_all(texts);
    let refs: Vec<&FoldedEnv> = foldeds.iter().collect();
    check_project_beats(&docs, &refs)
}

fn with_code<'a>(out: &'a [(PathBuf, Diagnostic)], code: &str) -> Vec<&'a Diagnostic> {
    out.iter().filter(|(_, d)| d.code == code).map(|(_, d)| d).collect()
}

fn entry(id: &str, attrs: &str) -> String {
    format!("<entry id=\"{id}\" {attrs}>\n@narrator: {id}.\n</entry>\n")
}

// --- T1-8: shadowed on every target -----------------------------------------

#[test]
fn an_untargeted_beat_beaten_on_every_target_of_a_closed_domain_is_shadowed() {
    let body = [
        entry("maudAgain", "on=\"talk\" target=\"npc.maud\""),
        entry("oskarAgain", "on=\"talk\" target=\"npc.oskar\""),
        entry("anyone", "on=\"talk\" priority=\"-5\""),
    ]
    .concat();
    let out = project_beats(&[&lore("", &body)]);
    let shadowed = with_code(&out, "W-BEAT-SHADOWED");
    assert_eq!(shadowed.len(), 1, "{out:?}");
    let m = &shadowed[0].message;
    assert!(m.contains("entry `anyone`"), "{m}");
    assert!(m.contains("on every target"), "{m}");
    assert!(m.contains("`npc.maud`: entry `maudAgain`"), "{m}");
    assert!(m.contains("`npc.oskar`: entry `oskarAgain`"), "{m}");
}

#[test]
fn an_untargeted_beat_that_wins_some_target_or_an_open_domain_is_not_shadowed() {
    // `npc.oskar`'s only beat is spendable: `anyone` wins there.
    let body = [
        entry("maudAgain", "on=\"talk\" target=\"npc.maud\""),
        entry("oskarOnce", "on=\"talk\" target=\"npc.oskar\" once=\"run\""),
        entry("anyone", "on=\"talk\" priority=\"-5\""),
    ]
    .concat();
    let out = project_beats(&[&lore("", &body)]);
    assert!(with_code(&out, "W-BEAT-SHADOWED").is_empty(), "{out:?}");
    // An open kind's members are engine-populated: never "every target".
    let body = [
        entry("northAgain", "on=\"roam\" target=\"area.north\""),
        entry("anywhere", "on=\"roam\" priority=\"-5\""),
    ]
    .concat();
    let out = project_beats(&[&lore("", &body)]);
    assert!(with_code(&out, "W-BEAT-SHADOWED").is_empty(), "{out:?}");
}

// --- T3-3: tie exclusivity ---------------------------------------------------

#[test]
fn once_folds_into_eligibility_before_the_tie_check() {
    // `tName` is spent by its first read; `tDeath` needs that read.
    let body = [
        entry(
            "tName",
            "on=\"talk\" target=\"npc.oskar\" priority=\"20\" once=\"user\" when=\"user.bond >= 1\"",
        ),
        entry(
            "tDeath",
            "on=\"talk\" target=\"npc.oskar\" priority=\"20\" once=\"user\" \
             when=\"user.bond >= 1 &amp;&amp; entry.tName.everRead\"",
        )
        .replace("&amp;&amp;", "&&"),
    ]
    .concat();
    let out = project_beats(&[&lore("", &body)]);
    assert!(with_code(&out, "W-BEAT-PRIORITY-TIE").is_empty(), "{out:?}");
    // `once="run"` is spent by `read`: `everRead` does not exclude it.
    let body = body.replacen("once=\"user\"", "once=\"run\"", 1);
    let out = project_beats(&[&lore("", &body)]);
    assert_eq!(with_code(&out, "W-BEAT-PRIORITY-TIE").len(), 1, "{out:?}");
    // A scene's `once: user` is `!visited('<id>')`.
    let out = project_beats(&[
        &scene("a.first", "on: hubVisit\nonce: user\nwhen: 'user.bond >= 1'\n"),
        &scene("a.after", "on: hubVisit\nwhen: \"visited('a.first')\"\n"),
    ]);
    assert!(with_code(&out, "W-BEAT-PRIORITY-TIE").is_empty(), "{out:?}");
    let out = project_beats(&[
        &scene("a.first", "on: hubVisit\nonce: run\nwhen: 'user.bond >= 1'\n"),
        &scene("a.after", "on: hubVisit\nwhen: \"visited('a.first')\"\n"),
    ]);
    assert_eq!(with_code(&out, "W-BEAT-PRIORITY-TIE").len(), 1, "{out:?}");
}

const SCHEDULE: &str = "relations:\n  at: { args: [person, location], derive: true }\n  \
                        awake: { args: [person], tier: run }\n";

fn schedule(rules: &[&str]) -> String {
    let rules: String = rules.iter().map(|r| format!("  - \"{r}\"\n")).collect();
    format!("{SCHEDULE}rules:\n{rules}")
}

#[test]
fn a_pure_schedule_atom_contributes_its_rule_guards_to_exclusivity() {
    let beats = |rules: &[&str]| {
        let fm = schedule(rules);
        project_beats(&[
            &scene(
                "sol.first",
                &format!("on: visit\ntarget: place.radio\nwhen: 'holds(at(maud, radio))'\n{fm}"),
            ),
            &scene(
                "ines.forecast",
                &format!("on: visit\ntarget: place.radio\nwhen: 'holds(at(oskar, radio))'\n{fm}"),
            ),
        ])
    };
    let exclusive = beats(&[
        "at(maud, radio) :- cel(\\\"run.slot == 'morning'\\\")",
        "at(oskar, radio) :- cel(\\\"run.slot == 'afternoon'\\\")",
    ]);
    assert!(with_code(&exclusive, "W-BEAT-PRIORITY-TIE").is_empty(), "{exclusive:?}");
    // Every rule is a disjunct: an `oskar` rule that overlaps `maud`'s keeps the tie.
    let overlap = beats(&[
        "at(maud, radio) :- cel(\\\"run.slot == 'morning'\\\")",
        "at(oskar, radio) :- cel(\\\"run.slot == 'afternoon'\\\")",
        "at(oskar, radio) :- cel(\\\"run.slot == 'morning'\\\")",
    ]);
    assert_eq!(with_code(&overlap, "W-BEAT-PRIORITY-TIE").len(), 1, "{overlap:?}");
    // A body atom makes it no pure schedule.
    let gated = beats(&[
        "at(maud, radio) :- cel(\\\"run.slot == 'morning'\\\")",
        "at(oskar, radio) :- awake(oskar), cel(\\\"run.slot == 'afternoon'\\\")",
    ]);
    assert_eq!(with_code(&gated, "W-BEAT-PRIORITY-TIE").len(), 1, "{gated:?}");
}

// --- T3-4: user-tier relations and quests -------------------------------------

#[test]
fn once_run_user_counts_user_tier_relations_and_quests() {
    let rels = "relations:\n  felled: { args: [person], tier: user }\n  \
                seen: { args: [person], tier: run }\n";
    let quests = quest_doc(
        "<quest id=\"chart\" title=\"Chart\">\n<objective id=\"a\" title=\"A\" done=\"user.bond >= 9\"/>\n</quest>\n\
         <quest id=\"errand\" title=\"Errand\" tier=\"run\">\n<objective id=\"a\" title=\"A\" done=\"user.bond >= 9\"/>\n</quest>\n",
    );
    let run = |when: &str| {
        let src = scene("a.one", &format!("on: hubVisit\nwhen: \"{when}\"\n{rels}"));
        project_beats(&[&src, &quests])
    };
    for when in ["holds(felled(maud))", "quest.chart.state == 'active'", "count(felled(maud)) >= 1"] {
        let out = run(when);
        assert_eq!(with_code(&out, "W-BEAT-ONCE-RUN-USER").len(), 1, "{when}: {out:?}");
    }
    for when in [
        "holds(seen(maud))",
        "quest.errand.state == 'active'",
        "holds(felled(maud)) && holds(seen(oskar))",
    ] {
        let out = run(when);
        assert!(with_code(&out, "W-BEAT-ONCE-RUN-USER").is_empty(), "{when}: {out:?}");
    }
}

// --- T1-6: scene frontmatter `when:` references ------------------------------

#[test]
fn frontmatter_when_quest_and_entry_typos_are_reported_at_the_id() {
    let quests = quest_doc(
        "<quest id=\"lampOut\" title=\"Lamp\">\n<objective id=\"a\" title=\"A\" done=\"user.bond >= 9\"/>\n</quest>\n",
    );
    let barks = lore("", &entry("tomasOil", "on=\"talk\" target=\"npc.oskar\""));
    let typo = scene(
        "mara.typo",
        "on: talk\ntarget: npc.maud\nwhen: \"entry.tomasOyl.everRead && quest.lampOot.state == 'active'\"\n",
    );
    let (docs, _) = parse_all(&[&quests, &barks, &typo]);
    let q = check_project_quest_refs(&docs);
    assert_eq!(q.len(), 1, "{q:?}");
    let (path, d) = &q[0];
    assert_eq!(path, &PathBuf::from("2.lute"));
    assert_eq!(d.code, "W-QUEST-REF-UNKNOWN");
    assert_eq!(&typo[d.span.byte_start..d.span.byte_end], "lampOot");
    assert!(d.message.contains("did you mean `lampOut`?"), "{}", d.message);
    let e = check_project_entry_refs(&docs);
    assert_eq!(e.len(), 1, "{e:?}");
    let (_, d) = &e[0];
    assert_eq!(&typo[d.span.byte_start..d.span.byte_end], "tomasOyl");
    assert!(d.message.contains("did you mean `tomasOil`?"), "{}", d.message);
}

// --- §6: entry target on an untargeted occasion --------------------------------

#[test]
fn an_entry_target_on_an_untargeted_occasion_is_metadata() {
    let src = lore("", &entry("keepsake", "on=\"hubVisit\" target=\"item.compass\""));
    let ds = check(&input(&src)).diagnostics;
    assert!(
        ds.iter().all(|d| d.severity != Severity::Error),
        "{ds:?}"
    );
    // It answers every raise: an always-eligible repeatable entry shadows a
    // later untargeted beat, exactly as an untargeted one would.
    let out = project_beats(&[
        &src,
        &scene("hub.later", "on: hubVisit\npriority: -1\n"),
    ]);
    assert_eq!(with_code(&out, "W-BEAT-SHADOWED").len(), 1, "{out:?}");
    // A scene's target there is still a restriction it cannot make.
    let scene_src = scene("hub.x", "on: hubVisit\ntarget: npc.maud\n");
    let ds = check(&input(&scene_src)).diagnostics;
    assert!(ds.iter().any(|d| d.code == "E-BEAT-ATTR"), "{ds:?}");
}

// --- T3-16: W-RELATION-UNREAD / W-DEF-UNUSED -----------------------------------

fn usage(texts: &[&str]) -> Vec<(PathBuf, Diagnostic)> {
    let (docs, foldeds) = parse_all(texts);
    let paths: Vec<PathBuf> = docs.iter().map(|(p, _)| p.clone()).collect();
    let docs: Vec<lute_check::UsageDoc<'_>> = docs
        .iter()
        .zip(&foldeds)
        .zip(texts)
        .zip(&paths)
        .map(|((((_, doc), folded), text), path)| lute_check::UsageDoc {
            path,
            text,
            doc,
            folded,
        })
        .collect();
    lute_check::check_project_usage(&docs, &[])
}

/// A scene declaring `met` (asserted by its body) and `quiet`, with `read`
/// as the one reading condition / rule / def text.
fn usage_scene(decls: &str, when: &str, body: &str) -> String {
    format!(
        "---\nkind: scene\nid: a.one\n{decls}{VOCAB}{STATE}---\n## Shot 1.\n\
         @narrator{{when=\"{when}\"}}: Hi.\n{body}::assert{{ met(maud) }}\n"
    )
}

const MET: &str = "relations:\n  met: { args: [person], tier: run }\n  seen: { args: [person], tier: run }\n";

#[test]
fn a_relation_written_but_never_read_warns_once_at_its_declaration() {
    let src = usage_scene(MET, "user.bond >= 0", "");
    let out = usage(&[&src]);
    let hits: Vec<_> = out.iter().filter(|(_, d)| d.code == "W-RELATION-UNREAD").collect();
    assert_eq!(hits.len(), 1, "{out:?}");
    assert_eq!(&src[hits[0].1.span.byte_start..hits[0].1.span.byte_end], "met");
    assert!(hits[0].1.message.contains("relation `met`"), "{}", hits[0].1.message);
    // `seen` is never written: not this advisory's business.
    assert!(!out.iter().any(|(_, d)| d.message.contains("`seen`")), "{out:?}");
    // Read by a query, a rule body atom, or a rule guard: silent.
    for (decls, when) in [
        (MET.to_string(), "holds(met(maud))"),
        (MET.to_string(), "count(met(maud)) >= 1"),
        (MET.to_string(), "countDistinct(met(P), P) >= 1"),
        (format!("{MET}rules:\n  - \"seen(P) :- met(P)\"\n"), "user.bond >= 0"),
        (
            format!("{MET}rules:\n  - \"seen(maud) :- cel(\\\"holds(met(maud))\\\")\"\n"),
            "user.bond >= 0",
        ),
    ] {
        let out = usage(&[&usage_scene(&decls, when, "")]);
        assert!(
            !out.iter().any(|(_, d)| d.code == "W-RELATION-UNREAD" && d.message.contains("`met`")),
            "{when} / {decls}: {out:?}"
        );
    }
    // Reserved (engine-asserted) relations are the engine's to read.
    let reserved = "relations:\n  met: { args: [person], tier: run, reserved: true }\n";
    let out = usage(&[&usage_scene(reserved, "user.bond >= 0", "")]);
    assert!(!out.iter().any(|(_, d)| d.code == "W-RELATION-UNREAD"), "{out:?}");
}

#[test]
fn a_def_no_reference_uses_warns_once_at_its_declaration() {
    let defs = |extra: &str| format!("{MET}defs:\n  quiet: \"user.bond == 0\"\n{extra}");
    let src = usage_scene(&defs(""), "holds(met(maud))", "");
    let out = usage(&[&src]);
    let hits: Vec<_> = out.iter().filter(|(_, d)| d.code == "W-DEF-UNUSED").collect();
    assert_eq!(hits.len(), 1, "{out:?}");
    assert_eq!(&src[hits[0].1.span.byte_start..hits[0].1.span.byte_end], "quiet");
    // Used in content, by another def, or in a rule guard: silent.
    for (extra, when) in [
        ("", "@quiet && holds(met(maud))"),
        ("  calm: \"@quiet\"\n", "@calm && holds(met(maud))"),
        ("rules:\n  - \"seen(maud) :- cel(\\\"@quiet\\\")\"\n", "holds(met(maud))"),
    ] {
        let out = usage(&[&usage_scene(&defs(extra), when, "")]);
        assert!(!out.iter().any(|(_, d)| d.code == "W-DEF-UNUSED"), "{extra} / {when}: {out:?}");
    }
}
