//! dsl 0.24.0 definite assignment through `@def`s and guard narrowing
//! (round-3 T1-4, T1-5, T1-7): a read inside a def body is checked at the use
//! site; a beat's / entry's `when` is an assumption for its body; a presence
//! guard proves the reads its own short-circuit protects; `prev.run.*` is one
//! snapshot; a `<match on="@def">` takes the def's domain.

use lute_check::{check, CheckInput, Mode, SchemaImports};
use lute_core_span::Diagnostic;
use lute_manifest::schema::{OccasionDecl, OccasionSelect};
use lute_manifest::snapshot::CapabilitySnapshot;

fn snapshot() -> CapabilitySnapshot {
    let mut snap = lute_manifest::core::load_core_snapshot();
    snap.occasions.insert(
        "hubVisit".into(),
        OccasionDecl {
            name: "hubVisit".into(),
            select: OccasionSelect::First,
            target: false.into(),
            description: None,
            ..Default::default()
        },
    );
    snap
}

fn run(text: &str) -> Vec<Diagnostic> {
    check(&CheckInput {
        text: text.to_string(),
        uri: "narrowing".into(),
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

const STATE: &str = "state:\n  \
    run.depth: { type: number, default: 0 }\n  \
    run.outcome: { type: { enum: [diving, died, surfaced] }, default: diving }\n  \
    run.o: { type: { enum: [won, lost] } }\n  \
    run.a: { type: number, default: 0 }\n  \
    run.wd: { type: { enum: [a, b, c] }, default: a }\n  \
    run.odd: { type: number }\n";

fn scene(meta: &str, body: &str) -> String {
    format!("---\nkind: scene\nid: probe.s\n{STATE}{meta}---\n## Shot 1.\n{body}")
}

fn beat(when: &str, body: &str) -> String {
    scene(&format!("on: hubVisit\nonce: false\nwhen: \"{when}\"\n"), body)
}

// --- T1-4: reads through `@def` ------------------------------------------------

#[test]
fn a_def_body_read_is_checked_at_every_use_site() {
    let defs = "defs:\n  lastF: \"prev.run.depth * 10\"\n";
    for body in [
        "@narrator: Deep: {{@lastF}}.\n",
        "@narrator{when=\"@lastF > 30\"}: Deep.\n",
        "<branch id=\"b\">\n<choice id=\"c\" label=\"Dive {{@lastF}}\">\n@n: x\n</choice>\n</branch>\n",
        "::set{run.a = @lastF}\n",
        "::use{component=\"gauge\" fathoms=@lastF}\n",
    ] {
        let src = scene(defs, body);
        let ds = run(&src);
        let hits = with_code(&ds, "E-MAYBE-UNSET");
        assert_eq!(hits.len(), 1, "{body}: {ds:#?}");
        let d = hits[0];
        assert!(
            d.message.contains("`prev.run.depth`") && d.message.contains("`@lastF`"),
            "names the path and the def: {}",
            d.message
        );
        assert!(
            src[d.span.byte_start..d.span.byte_end].contains("@lastF"),
            "anchored at the use: {body}"
        );
    }
    for body in [
        "@narrator{when=\"isSet(prev.run.depth)\"}: Deep: {{@lastF}}.\n",
        "@narrator{when=\"isSet(prev.run.depth) && @lastF > 30\"}: Deep.\n",
    ] {
        let ds = run(&scene(defs, body));
        assert!(with_code(&ds, "E-MAYBE-UNSET").is_empty(), "{body}: {ds:#?}");
    }
}

#[test]
fn a_def_subject_reads_and_narrows_like_its_path() {
    let defs = "defs:\n  lastO: \"prev.run.outcome\"\n";
    let open = "<match on=\"@lastO\">\n<when is=\"diving\">\n@n: a\n</when>\n\
                <when is=\"died|surfaced\">\n@n: b\n</when>\n";
    let ds = run(&scene(defs, &format!("{open}</match>\n")));
    assert_eq!(with_code(&ds, "E-UNSET-UNCOVERED").len(), 1, "{ds:#?}");
    let reads = with_code(&ds, "E-MAYBE-UNSET");
    assert_eq!(reads.len(), 1, "{ds:#?}");
    assert!(reads[0].message.contains("`@lastO`"), "{}", reads[0].message);
    let covered = format!("{open}<when is=\"unset\">\n@n: c\n</when>\n</match>\n");
    let ds = run(&scene(defs, &covered));
    assert!(ds.is_empty(), "{ds:#?}");
}

// --- T1-5: `<match on="@def">` domains -------------------------------------------

#[test]
fn a_def_subject_takes_the_def_domain() {
    let defs = "defs:\n  \
        wd: { type: { enum: [a, b, c] }, cel: \"run.a == 1 ? 'a' : run.a == 2 ? 'b' : 'c'\" }\n  \
        wd2: \"run.wd\"\n";
    let exhaustive = "<match on=\"@wd\">\n<when is=\"a\">\n@n: a\n</when>\n\
                      <when is=\"b|c\">\n@n: bc\n</when>\n</match>\n";
    let ds = run(&scene(defs, exhaustive));
    assert!(ds.is_empty(), "{ds:#?}");

    let partial = "<match on=\"@wd\">\n<when is=\"a\">\n@n: a\n</when>\n</match>\n";
    let ds = run(&scene(defs, partial));
    let d = with_code(&ds, "E-NONEXHAUSTIVE");
    assert_eq!(d.len(), 1, "{ds:#?}");
    assert!(d[0].message.contains("`b`, `c` are not covered"), "{}", d[0].message);

    let typo = "<match on=\"@wd2\">\n<when is=\"zz\">\n@n: zz\n</when>\n\
                <otherwise>\n@n: other\n</otherwise>\n</match>\n";
    let ds = run(&scene(defs, typo));
    assert_eq!(with_code(&ds, "E-WHEN-LITERAL-DOMAIN").len(), 1, "{ds:#?}");
}

#[test]
fn nonexhaustive_names_the_missing_member() {
    let ds = run(&scene(
        "",
        "<match on=\"run.wd\">\n<when is=\"a|b\">\n@n: ab\n</when>\n</match>\n",
    ));
    let d = with_code(&ds, "E-NONEXHAUSTIVE");
    assert_eq!(d.len(), 1, "{ds:#?}");
    assert!(d[0].message.contains("`c` is not covered"), "{}", d[0].message);
}

// --- T1-7 (a): a beat's / entry's `when` is an assumption --------------------------

const DIVE_BODY: &str = "<match on=\"prev.run.outcome\">\n\
    <when is=\"died\">\n@narrator: Died at {{prev.run.depth}}.\n</when>\n\
    <when is=\"surfaced\">\n@narrator: Surfaced.\n</when>\n\
    <when is=\"diving\">\n@narrator: ?\n</when>\n</match>\n\
    <match on=\"run.outcome\">\n<when is=\"diving\">\n@narrator: diving.\n</when>\n\
    <when is=\"died\">\n@narrator: dead arm.\n</when>\n\
    <otherwise>\n@narrator: other.\n</otherwise>\n</match>\n";

#[test]
fn a_beat_when_is_an_assumption_for_the_body() {
    let when = "isSet(prev.run.outcome) && run.outcome == 'diving'";
    let ds = run(&beat(when, DIVE_BODY));
    assert!(with_code(&ds, "E-UNSET-UNCOVERED").is_empty(), "{ds:#?}");
    assert!(with_code(&ds, "E-MAYBE-UNSET").is_empty(), "{ds:#?}");
    let dead = with_code(&ds, "E-ARM-DEAD");
    assert_eq!(dead.len(), 1, "{ds:#?}");
    assert!(
        dead[0].message.contains("`died`") && dead[0].message.contains(when),
        "{}",
        dead[0].message
    );
    // Under the assumption only `diving` is left, and an arm covers it.
    assert_eq!(with_code(&ds, "W-OTHERWISE-DEAD").len(), 1, "{ds:#?}");

    // Without the assumption the same body reads maybe-unset paths.
    let ds = run(&scene("", DIVE_BODY));
    assert_eq!(with_code(&ds, "E-UNSET-UNCOVERED").len(), 1, "{ds:#?}");
    assert!(!with_code(&ds, "E-MAYBE-UNSET").is_empty(), "{ds:#?}");
    assert!(with_code(&ds, "E-ARM-DEAD").is_empty(), "{ds:#?}");
}

#[test]
fn a_write_in_the_body_retracts_the_value_assumption() {
    let body = "::set{run.outcome = 'died'}\n<match on=\"run.outcome\">\n\
                <when is=\"diving\">\n@n: a\n</when>\n<when is=\"died\">\n@n: b\n</when>\n\
                <otherwise>\n@n: c\n</otherwise>\n</match>\n";
    let ds = run(&beat("run.outcome == 'diving'", body));
    assert!(with_code(&ds, "E-ARM-DEAD").is_empty(), "{ds:#?}");
}

#[test]
fn entry_and_bundle_beat_when_are_assumptions() {
    let src = format!(
        "---\nkind: lore\nid: notes\n{STATE}---\n\
         <entry id=\"e\" when=\"isSet(run.o)\">\n@n: {{{{run.o}}}}\n</entry>\n\
         <beat id=\"b\" on=\"hubVisit\" when=\"isSet(run.o) && run.o == 'won'\">\n\
         <match on=\"run.o\">\n<when is=\"won\">\n@n: w\n</when>\n\
         <when is=\"lost\">\n@n: l\n</when>\n</match>\n</beat>\n"
    );
    let ds = run(&src);
    assert!(with_code(&ds, "E-MAYBE-UNSET").is_empty(), "{ds:#?}");
    assert!(with_code(&ds, "E-UNSET-UNCOVERED").is_empty(), "{ds:#?}");
    let dead = with_code(&ds, "E-ARM-DEAD");
    assert_eq!(dead.len(), 1, "{ds:#?}");
    assert!(dead[0].message.contains("`lost`"), "{}", dead[0].message);
}

// --- T1-7 (b): short-circuit narrowing ---------------------------------------------

fn line_reads(when: &str) -> usize {
    let defs = "defs:\n  won: \"isSet(run.o) && run.o == 'won'\"\n";
    let ds = run(&scene(defs, &format!("@narrator{{when=\"{when}\"}}: x.\n")));
    with_code(&ds, "E-MAYBE-UNSET").len()
}

#[test]
fn a_presence_guard_proves_the_reads_its_short_circuit_protects() {
    for when in [
        "isSet(run.o) && run.o == 'won'",
        "(isSet(run.o) && run.o == 'won')",
        "run.a == 1 || (isSet(run.o) && run.o == 'won')",
        "(isSet(run.o) && run.o == 'won') || run.a == 1",
        "isSet(run.o) ? run.o == 'won' : false",
        "run.a == 1 || @won",
        "!isSet(run.o) || run.o == 'won'",
        "!isSet(run.o) ? false : run.o == 'won'",
        "has(run.o) && run.o == 'won'",
    ] {
        assert_eq!(line_reads(when), 0, "{when}");
    }
    for when in [
        "isSet(run.o) || run.o == 'won'",
        "run.o == 'won' && isSet(run.o)",
        "isSet(run.o) ? false : run.o == 'won'",
    ] {
        assert_eq!(line_reads(when), 1, "{when}");
    }
}

#[test]
fn a_short_circuit_proof_does_not_leak_into_the_body() {
    let ds = run(&scene(
        "",
        "@narrator{when=\"run.a == 2 || isSet(run.o)\"}: {{run.o}}.\n",
    ));
    assert_eq!(with_code(&ds, "E-MAYBE-UNSET").len(), 1, "{ds:#?}");
}

// --- T1-7 (c): the `prev.run` snapshot is atomic -----------------------------------

#[test]
fn one_present_prev_run_path_proves_every_defaulted_mirror() {
    let ok = run(&scene(
        "",
        "@narrator{when=\"isSet(prev.run.outcome) && prev.run.depth > 3\"}: x.\n\
         <match on=\"prev.run.outcome\">\n<when is=\"died\">\n@n: {{prev.run.depth}}\n</when>\n\
         <otherwise>\n@n: o\n</otherwise>\n</match>\n",
    ));
    assert!(with_code(&ok, "E-MAYBE-UNSET").is_empty(), "{ok:#?}");
    // `run.odd` has no default: it may be unset when the run ends.
    let ds = run(&scene(
        "",
        "@narrator{when=\"isSet(prev.run.outcome) && prev.run.odd > 3\"}: x.\n",
    ));
    let hits = with_code(&ds, "E-MAYBE-UNSET");
    assert_eq!(hits.len(), 1, "{ds:#?}");
    assert!(hits[0].message.contains("`prev.run.odd`"), "{}", hits[0].message);
}
