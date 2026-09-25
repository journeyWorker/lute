//! dsl 0.24.0 checker fixes owned outside the feature slices: staging
//! directives checked against the cast (§4, round-3 T1-10) and a quest
//! handler whose `when` implies the quest's completion (round-3 T3-11).

use lute_check::{check, CheckInput, Mode, SchemaImports};
use lute_core_span::{Diagnostic, Span};
use lute_manifest::snapshot::CapabilitySnapshot;

fn run(text: &str, snapshot: CapabilitySnapshot, imports: SchemaImports) -> Vec<Diagnostic> {
    check(&CheckInput {
        text: text.to_string(),
        uri: "c024".into(),
        snapshot,
        providers: Default::default(),
        mode: Mode::Author,
        imports,
        components: Default::default(),
        defaults: Default::default(),
    })
    .diagnostics
}

fn core() -> CapabilitySnapshot {
    lute_manifest::core::load_core_snapshot()
}

fn with_code<'a>(ds: &'a [Diagnostic], code: &str) -> Vec<&'a Diagnostic> {
    ds.iter().filter(|d| d.code == code).collect()
}

/// Imports resolving a schema whose `cast:` declares `mara` and `tomas`.
fn cast_imports(tag: &str) -> SchemaImports {
    let dir = std::env::temp_dir().join(format!("lute_c024_{tag}_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("cast.schema.yaml"),
        "cast:\n  mara: { name: Mara }\n  tomas: { name: Tomas }\n",
    )
    .unwrap();
    let zero = Span {
        byte_start: 0,
        byte_end: 0,
        line: 1,
        column: 1,
        utf16_range: (0, 0),
    };
    let imports = lute_check::resolve_imports(&dir, &["cast.schema.yaml".into()], &[], zero);
    assert!(imports.diags.is_empty(), "{:?}", imports.diags);
    let _ = std::fs::remove_dir_all(&dir);
    imports
}

fn scene(body: &str) -> String {
    format!("---\nkind: scene\nid: a.one\n---\n## Shot 1.\n{body}")
}

// --- §4 staging directives against the cast (T1-10) -------------------------

#[test]
fn staged_character_outside_the_cast_is_unknown_with_a_suggestion() {
    let src = scene(
        "::auto{character=\"marra\" action=\"fade-in-up\"}\n\
         @mara: Hi.\n\
         <match on=\"run.x\">\n<otherwise>\n::camera{focus=\"tomsa\" zoom=\"1.1\"}\n</otherwise>\n</match>\n\
         <timeline>\n<track subject=\"camera\">\n::camera{focus=\"marq\" duration=\"0.4\"}\n</track>\n</timeline>\n",
    );
    let ds = run(&src, core(), cast_imports("bad"));
    let hits = with_code(&ds, "E-CAST-UNKNOWN");
    let anchored: Vec<&str> = hits.iter().map(|d| &src[d.span.byte_start..d.span.byte_end]).collect();
    assert_eq!(anchored, ["marra", "tomsa", "marq"], "{ds:?}");
    assert!(hits[0].message.contains("`::auto{character}` `marra`"), "{}", hits[0].message);
    assert!(hits[0].message.contains("did you mean `mara`"), "{}", hits[0].message);
    assert!(hits[1].message.contains("`::camera{focus}` `tomsa`"), "{}", hits[1].message);
    assert!(hits[1].message.contains("did you mean `tomas`"), "{}", hits[1].message);
}

#[test]
fn staged_cast_members_and_shape_only_projects_are_clean() {
    let src = scene(
        "::auto{character=\"mara\" action=\"fade-in-up\"}\n\
         ::camera{focus=\"tomas\" zoom=\"1.1\"}\n\
         @mara: Hi.\n",
    );
    assert!(with_code(&run(&src, core(), cast_imports("ok")), "E-CAST-UNKNOWN").is_empty());
    // Without a declared cast staging stays shape-only, like speakers.
    let loose = scene("::auto{character=\"anyone\" action=\"fade-in-up\"}\n@narrator: Hi.\n");
    assert!(with_code(&run(&loose, core(), SchemaImports::default()), "E-CAST-UNKNOWN").is_empty());
}

// --- T3-11 a handler that can only fire on a completed quest -----------------

fn quest(attrs: &str, objectives: &str, on: &str) -> String {
    format!(
        "---\nkind: quest\nstate:\n  run.down: {{ type: bool, default: false }}\n  \
         run.day: {{ type: number, default: 1 }}\n\
         defs:\n  slain: \"run.down == true\"\n---\n\
         <quest id=\"hunt\" title=\"Hunt\" start=\"true\"{attrs}>\n{objectives}\n{on}\n</quest>\n"
    )
}

fn handler_dead(src: &str) -> Vec<Diagnostic> {
    run(src, core(), SchemaImports::default())
        .into_iter()
        .filter(|d| d.code == "W-QUEST-HANDLER-DEAD")
        .collect()
}

const SLAY: &str = "<objective id=\"slay\" title=\"Slay\" done=\"run.down == true\"/>";

#[test]
fn a_handler_whose_when_implies_completion_never_runs() {
    let on = "<on event=\"bossDefeated\" when=\"run.day > 1 && (run.down == true)\">\n@narrator: Saw it.\n</on>";
    let src = quest("", SLAY, on);
    let hits = handler_dead(&src);
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(&src[hits[0].span.byte_start..hits[0].span.byte_end], "bossDefeated");
    assert!(hits[0].message.contains("quest `hunt`"), "{}", hits[0].message);
    // Through `@def`s on either side.
    let via_def = quest(
        "",
        "<objective id=\"slay\" title=\"Slay\" done=\"@slain\"/>",
        "<on event=\"bossDefeated\" when=\"@slain\">\n@narrator: Saw it.\n</on>",
    );
    assert_eq!(handler_dead(&via_def).len(), 1);
    // `complete="any"`: one required objective suffices.
    let two = format!("{SLAY}\n<objective id=\"flee\" title=\"Flee\" done=\"run.day > 5\"/>");
    let on = "<on event=\"bossDefeated\" when=\"run.down == true\">\n@narrator: Saw it.\n</on>";
    assert_eq!(handler_dead(&quest(" complete=\"any\"", &two, on)).len(), 1);
    assert!(handler_dead(&quest("", &two, on)).is_empty(), "`all` needs both objectives");
}

#[test]
fn a_handler_that_can_run_while_active_is_clean() {
    let on = |when: &str| format!("<on event=\"bossDefeated\" when=\"{when}\">\n@narrator: Saw it.\n</on>");
    // Not implied, or implied only under `||`.
    assert!(handler_dead(&quest("", SLAY, &on("run.day > 1"))).is_empty());
    assert!(handler_dead(&quest("", SLAY, &on("run.down == true || run.day > 1"))).is_empty());
    // The lifecycle events fire on the transition itself.
    let complete = "<on event=\"questComplete\" when=\"run.down == true\">\n@narrator: Done.\n</on>";
    assert!(handler_dead(&quest("", SLAY, complete)).is_empty());
    // An occasion objective is judged only when its occasion is raised.
    let occasion = "<objective id=\"slay\" title=\"Slay\" done=\"run.down == true\" on=\"bossDefeated\"/>";
    assert!(handler_dead(&quest("", occasion, &on("run.down == true"))).is_empty());
}
