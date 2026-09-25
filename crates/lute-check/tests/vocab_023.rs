//! dsl 0.23.0 vocabularies: `prev.run.<path>` (§6), a declared cast
//! (`E-CAST-UNKNOWN`, §7), and reward kinds that credit state
//! (`W-REWARD-DOUBLE-CREDIT`, §8).

use lute_check::{check, CheckInput, Mode, SchemaImports};
use lute_core_span::{Diagnostic, Severity, Span};
use lute_manifest::schema::{CastMember, RewardKindDecl};
use lute_manifest::snapshot::CapabilitySnapshot;

fn run(text: &str, snapshot: CapabilitySnapshot, imports: SchemaImports) -> Vec<Diagnostic> {
    check(&CheckInput {
        text: text.to_string(),
        uri: "vocab".into(),
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

fn scene(state: &str, body: &str) -> String {
    format!("---\nkind: scene\nid: a.one\nstate:\n{state}---\n## Shot 1.\n{body}")
}

const RUN_STATE: &str = "  run.day: { type: number, default: 1 }\n  \
                         run.slot: { type: { enum: [morning, night] }, default: morning }\n";

// --- §6 prev.run -------------------------------------------------------------

#[test]
fn prev_run_mirrors_every_declared_run_path_and_is_maybe_unset() {
    // Guarded reads are clean; the mirror carries the run path's type.
    let clean = scene(
        RUN_STATE,
        "@narrator{when=\"isSet(prev.run.day) && prev.run.day > 2\"}: Back again.\n\
         <match on=\"prev.run.slot\">\n<when is=\"night\">\n@narrator: Late last time.\n</when>\n\
         <otherwise>\n@narrator: Otherwise.\n</otherwise>\n</match>\n",
    );
    let ds = run(&clean, core(), SchemaImports::default());
    assert!(ds.iter().all(|d| d.severity != Severity::Error), "{ds:?}");

    // An unguarded read is maybe-unset: nothing is snapshotted before the
    // first run ends.
    let bare = scene(RUN_STATE, "@narrator{when=\"prev.run.day > 2\"}: Back.\n");
    let ds = run(&bare, core(), SchemaImports::default());
    assert_eq!(with_code(&ds, "E-MAYBE-UNSET").len(), 1, "{ds:?}");
}

#[test]
fn prev_of_an_undeclared_run_path_is_undeclared() {
    let src = scene(RUN_STATE, "@narrator{when=\"isSet(prev.run.dya)\"}: Hm.\n");
    let ds = run(&src, core(), SchemaImports::default());
    let hits = with_code(&ds, "E-UNDECLARED");
    assert_eq!(hits.len(), 1, "{ds:?}");
    assert!(
        hits[0].message.contains("prev.run.day"),
        "{}",
        hits[0].message
    );
}

#[test]
fn prev_run_is_read_only() {
    let src = scene(RUN_STATE, "::set{prev.run.day = 3}\n@narrator: Hi.\n");
    let ds = run(&src, core(), SchemaImports::default());
    let hits = with_code(&ds, "E-QUEST-RESERVED-WRITE");
    assert_eq!(hits.len(), 1, "{ds:?}");
    assert!(hits[0].message.contains("prev.run"), "{}", hits[0].message);
    // An author cannot declare the mirror either: `prev` is no state tier.
    let decl = scene("  prev.run.day: { type: number }\n", "@narrator: Hi.\n");
    let ds = run(&decl, core(), SchemaImports::default());
    assert!(ds.iter().any(|d| d.code == "E-STATE-NAMESPACE"), "{ds:?}");
}

// --- §7 cast -----------------------------------------------------------------

fn member(id: &str, name: &str) -> CastMember {
    CastMember {
        id: id.into(),
        name: Some(name.into()),
        ..Default::default()
    }
}

fn cast_scene(body: &str) -> String {
    format!("---\nkind: scene\nid: a.one\n---\n## Shot 1.\n{body}")
}

#[test]
fn speakers_are_shape_only_without_a_cast() {
    let ds = run(
        &cast_scene("@anyone: Hello.\n"),
        core(),
        SchemaImports::default(),
    );
    assert!(with_code(&ds, "E-CAST-UNKNOWN").is_empty(), "{ds:?}");
}

#[test]
fn a_plugin_cast_rejects_an_unknown_speaker_with_a_suggestion() {
    let mut snap = core();
    snap.cast.insert("maud".into(), member("maud", "Maud"));
    let src = cast_scene("@maud: Tea?\n@maude: Tea.\n@narrator: Steam.\n");
    let ds = run(&src, snap, SchemaImports::default());
    let hits = with_code(&ds, "E-CAST-UNKNOWN");
    assert_eq!(hits.len(), 1, "{ds:?}");
    assert_eq!(
        &src[hits[0].span.byte_start..hits[0].span.byte_end],
        "maude"
    );
    assert!(
        hits[0].message.contains("did you mean `maud`"),
        "{}",
        hits[0].message
    );
}

#[test]
fn a_schema_cast_reaches_every_importer() {
    let dir = std::env::temp_dir().join(format!("lute_cast_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("cast.schema.yaml"),
        "cast:\n  oskar: { name: Oskar }\n  sable: { name: Sable }\n",
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
    assert_eq!(imports.cast["oskar"].name.as_deref(), Some("Oskar"));
    let src = cast_scene("@oskar: The forge.\n@sabel: Wind.\n");
    let ds = run(&src, core(), imports);
    let hits = with_code(&ds, "E-CAST-UNKNOWN");
    assert_eq!(hits.len(), 1, "{ds:?}");
    assert!(
        hits[0].message.contains("did you mean `sable`"),
        "{}",
        hits[0].message
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cast_is_a_schema_key_not_a_scene_key() {
    let src =
        "---\nkind: scene\nid: a.one\ncast:\n  maud: { name: Maud }\n---\n## Shot 1.\n@maud: Hi.\n";
    let ds = run(src, core(), SchemaImports::default());
    assert!(ds.iter().any(|d| d.code == "E-META-UNKNOWN-KEY"), "{ds:?}");
}

// --- §8 reward credits -------------------------------------------------------

fn with_embers(credits: Option<&str>) -> CapabilitySnapshot {
    let mut snap = core();
    snap.reward_kinds.insert(
        "EMBERS".into(),
        RewardKindDecl {
            name: "EMBERS".into(),
            credits: credits.map(str::to_string),
            ..Default::default()
        },
    );
    snap
}

fn quest(handler_set: &str) -> String {
    format!(
        "---\nkind: quest\nstate:\n  user.embers: {{ type: number, default: 0 }}\n  \
         user.bond: {{ type: number, default: 0 }}\n---\n\
         <quest id=\"climb\" start=\"true\">\n  <reward kind=\"EMBERS\" amount=\"100\"/>\n  \
         <objective id=\"out\" done=\"true\"/>\n  <on event=\"questComplete\">\n    \
         ::set{{{handler_set}}}\n    @narrator: The fire roars.\n  </on>\n</quest>\n"
    )
}

#[test]
fn setting_a_credited_path_in_a_handler_double_credits() {
    let src = quest("user.embers += 100");
    let ds = run(
        &src,
        with_embers(Some("user.embers")),
        SchemaImports::default(),
    );
    let hits = with_code(&ds, "W-REWARD-DOUBLE-CREDIT");
    assert_eq!(hits.len(), 1, "{ds:?}");
    assert_eq!(hits[0].severity, Severity::Warning);
    assert_eq!(
        &src[hits[0].span.byte_start..hits[0].span.byte_end],
        "user.embers"
    );
}

#[test]
fn no_double_credit_without_credits_or_for_another_path() {
    let ds = run(
        &quest("user.embers += 100"),
        with_embers(None),
        SchemaImports::default(),
    );
    assert!(
        with_code(&ds, "W-REWARD-DOUBLE-CREDIT").is_empty(),
        "{ds:?}"
    );
    let ds = run(
        &quest("user.bond += 1"),
        with_embers(Some("user.embers")),
        SchemaImports::default(),
    );
    assert!(
        with_code(&ds, "W-REWARD-DOUBLE-CREDIT").is_empty(),
        "{ds:?}"
    );
}
