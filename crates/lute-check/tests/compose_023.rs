//! dsl 0.23.0 §2 (windows: `<objective by>`, `<objective on target>`) and §3
//! (composing occasions: `select: sequence`, scene beat `also`) through the
//! assembled `check()` and `check_project_beats` (`W-BEAT-SHADOWED`,
//! `W-BEAT-PRIORITY-TIE`).

use std::path::PathBuf;

use lute_check::{check, check_project_beats, fold_env, CheckInput, FoldedEnv, Mode, SchemaImports};
use lute_core_span::{Diagnostic, Severity};
use lute_manifest::schema::{OccasionDecl, OccasionSelect, OccasionTarget};
use lute_manifest::snapshot::CapabilitySnapshot;

fn snapshot() -> CapabilitySnapshot {
    let mut snap = lute_manifest::core::load_core_snapshot();
    let npc = OccasionTarget::Domain {
        prefix: "npc".into(),
        entity: "person".into(),
        members: None,
    };
    for (name, select, target) in [
        ("hubVisit", OccasionSelect::First, OccasionTarget::Shape(false)),
        ("talk", OccasionSelect::First, npc),
        ("examine", OccasionSelect::First, OccasionTarget::Shape(true)),
        ("board", OccasionSelect::All, OccasionTarget::Shape(false)),
        ("evening", OccasionSelect::Sequence, OccasionTarget::Shape(false)),
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
        uri: "compose".into(),
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

fn only<'a>(ds: &'a [Diagnostic], code: &str) -> &'a Diagnostic {
    let hits = with_code(ds, code);
    assert_eq!(hits.len(), 1, "want exactly one {code}: {ds:?}");
    hits[0]
}

fn errors(ds: &[Diagnostic]) -> Vec<&Diagnostic> {
    ds.iter().filter(|d| d.severity == Severity::Error).collect()
}

fn anchored<'s>(src: &'s str, d: &Diagnostic) -> &'s str {
    &src[d.span.byte_start..d.span.byte_end]
}

const VOCAB: &str = "state:\n  run.day: { type: number, default: 1 }\n  \
    run.done: { type: bool, default: false }\n  run.maybe: { type: bool }\n\
    entities:\n  person: { members: [maud, oskar] }\n";

fn scene(id: &str, fm: &str) -> String {
    format!("---\nkind: scene\nid: {id}\n{fm}{VOCAB}---\n## Shot 1.\n@narrator: Hi.\n")
}

fn quest(objective_attrs: &str) -> String {
    format!(
        "---\nkind: quest\n{VOCAB}---\n<quest id=\"q\" title=\"Q\">\n\
         <objective id=\"o\" title=\"O\" done=\"run.done\" {objective_attrs}/>\n</quest>\n"
    )
}

// --- §2 `<objective by>` ------------------------------------------------------

#[test]
fn by_is_a_bool_condition_slot_like_done() {
    let clean = quest("by=\"run.day >= 3\"");
    assert!(errors(&diags(&clean)).is_empty(), "{:?}", diags(&clean));
    // Every fault `done` reports for a slot, `by` reports with the same code:
    // an undeclared read, a maybe-unset read (definite assignment is local
    // to the slot).
    for bad in ["run.nope", "run.maybe"] {
        let as_done = format!(
            "---\nkind: quest\n{VOCAB}---\n<quest id=\"q\" title=\"Q\">\n\
             <objective id=\"o\" title=\"O\" done=\"{bad}\"/>\n</quest>\n"
        );
        let want: Vec<String> = errors(&diags(&as_done)).iter().map(|d| d.code.to_string()).collect();
        assert!(!want.is_empty(), "`{bad}` must be an error as `done`");
        let src = quest(&format!("by=\"{bad}\""));
        let ds = diags(&src);
        let got: Vec<String> = errors(&ds).iter().map(|d| d.code.to_string()).collect();
        assert_eq!(got, want, "`by=\"{bad}\"`: {ds:?}");
    }
}

// --- §2 `<objective on target>` ---------------------------------------------------

#[test]
fn objective_target_follows_the_beat_target_rule() {
    let clean = quest("on=\"talk\" target=\"npc.maud\"");
    assert!(errors(&diags(&clean)).is_empty(), "{:?}", diags(&clean));
    let open = quest("on=\"examine\" target=\"item.lamp\"");
    assert!(errors(&diags(&open)).is_empty(), "{:?}", diags(&open));

    for (attrs, anchor, needle) in [
        ("on=\"talk\" target=\"npc maud\"", "npc maud", "must be a dotted id"),
        ("target=\"npc.maud\"", "npc.maud", "`target` requires `on`"),
        ("on=\"hubVisit\" target=\"npc.maud\"", "npc.maud", "declared without `target: true`"),
    ] {
        let src = quest(attrs);
        let ds = diags(&src);
        let d = only(&ds, "E-BEAT-ATTR");
        assert_eq!(anchored(&src, d), anchor, "{attrs}: {ds:?}");
        assert!(d.message.contains(needle), "{attrs}: {}", d.message);
        assert!(with_code(&ds, "E-UNKNOWN-ATTR").is_empty(), "{attrs}: {ds:?}");
    }

    // Outside the occasion's domain, as for a beat.
    let src = quest("on=\"talk\" target=\"npc.vesna\"");
    let ds = diags(&src);
    assert_eq!(anchored(&src, only(&ds, "E-BEAT-ATTR")), "npc.vesna", "{ds:?}");

    // Not a quoted string.
    let src = quest("on=\"talk\" target=@x");
    let ds = diags(&src);
    let d = only(&ds, "E-BEAT-ATTR");
    assert!(d.message.contains("attribute `target` must be a quoted string"), "{}", d.message);
}

// --- §3 `also` ----------------------------------------------------------------

#[test]
fn also_on_a_select_first_occasion_is_clean_and_lifted() {
    for fm in ["on: hubVisit\nalso: true\n", "on: talk\ntarget: npc.maud\nalso: false\n"] {
        let src = scene("a.b", fm);
        assert!(errors(&diags(&src)).is_empty(), "{fm}: {:?}", diags(&src));
    }
    let input = input(&scene("a.b", "on: hubVisit\nalso: true\n"));
    let (doc, _) = lute_syntax::parse(&input.text);
    assert!(fold_env(&doc, &input).0.typed.beat.expect("a beat").also);
}

#[test]
fn also_shape_faults_are_beat_attr() {
    for value in ["maybe", "'true'", "1"] {
        let src = scene("a.b", &format!("on: hubVisit\nalso: {value}\n"));
        let ds = diags(&src);
        let d = only(&ds, "E-BEAT-ATTR");
        assert!(d.message.contains("`also:` must be `true`"), "{value}: {}", d.message);
    }
    for occasion in ["board", "evening"] {
        let src = scene("a.b", &format!("on: {occasion}\nalso: true\n"));
        let ds = diags(&src);
        let d = only(&ds, "E-BEAT-ATTR");
        assert!(d.message.contains("applies only to a `select: first` occasion"), "{}", d.message);
        assert_eq!(anchored(&src, d), "true", "anchored at the value: {ds:?}");
    }
    // `also: false` on those occasions says nothing wrong.
    assert!(errors(&diags(&scene("a.b", "on: evening\nalso: false\n"))).is_empty());
}

#[test]
fn also_on_an_entry_is_beat_attr_only() {
    let src = format!(
        "---\nkind: lore\ntitle: Barks\n{VOCAB}---\n\
         <entry id=\"bark\" on=\"hubVisit\" also=\"true\">\n@narrator: Hi.\n</entry>\n"
    );
    let ds = diags(&src);
    let d = only(&ds, "E-BEAT-ATTR");
    assert!(d.message.contains("an `<entry>` beat cannot ride along"), "{}", d.message);
    assert!(with_code(&ds, "E-UNKNOWN-ATTR").is_empty(), "{ds:?}");
}

// --- §3 project advisories ignore `also` ----------------------------------------------

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

fn codes(out: &[(PathBuf, Diagnostic)], code: &str) -> usize {
    out.iter().filter(|(_, d)| d.code == code).count()
}

#[test]
fn an_also_beat_neither_shadows_nor_is_shadowed() {
    // The control: without `also`, `a.one` shadows `a.two`.
    let out = project_beats(&[
        &scene("a.one", "on: hubVisit\npriority: 5\nonce: false\n"),
        &scene("a.two", "on: hubVisit\n"),
    ]);
    assert_eq!(codes(&out, "W-BEAT-SHADOWED"), 1, "{out:?}");
    // An always-eligible, never-spent `also` beat ranked first shadows nothing.
    let out = project_beats(&[
        &scene("a.one", "on: hubVisit\npriority: 5\nonce: false\nalso: true\n"),
        &scene("a.two", "on: hubVisit\n"),
    ]);
    assert_eq!(codes(&out, "W-BEAT-SHADOWED"), 0, "{out:?}");
    // An `also` beat ranked below such a beat still rides along.
    let out = project_beats(&[
        &scene("a.one", "on: hubVisit\npriority: 5\nonce: false\n"),
        &scene("a.two", "on: hubVisit\nalso: true\n"),
    ]);
    assert_eq!(codes(&out, "W-BEAT-SHADOWED"), 0, "{out:?}");
}

#[test]
fn an_also_beat_never_ties() {
    let overlapping = |a: &str, b: &str| {
        project_beats(&[
            &scene("a.one", &format!("on: hubVisit\nwhen: 'run.day >= 2'\n{a}")),
            &scene("a.two", &format!("on: hubVisit\nwhen: 'run.day >= 3'\n{b}")),
        ])
    };
    assert_eq!(codes(&overlapping("", ""), "W-BEAT-PRIORITY-TIE"), 1, "the control ties");
    for (a, b) in [("also: true\n", ""), ("", "also: true\n"), ("also: true\n", "also: true\n")] {
        let out = overlapping(a, b);
        assert_eq!(codes(&out, "W-BEAT-PRIORITY-TIE"), 0, "{a:?}/{b:?}: {out:?}");
    }
}
