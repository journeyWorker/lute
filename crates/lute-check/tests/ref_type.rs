//! B2.2 — `E-REF-TYPE`: a declared `@ref` def used in a CEL slot whose
//! statically-known expected type is incompatible with the def's produced type
//! (dsl §8). Fed through the assembled `check()` over inline `state:`/`defs:`
//! frontmatter (mirrors `group_d.rs`'s harness).
use lute_check::{check, CheckInput, Mode, SchemaImports};
use lute_manifest::provider::ProviderSet;
use lute_manifest::schema::DefDecl;
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_manifest::types::Type;

const HDR: &str = "---\ncharacter: x\nseason: 1\nepisode: 1\n";

fn codes(text: &str) -> Vec<String> {
    let input = CheckInput {
        text: text.to_string(),
        uri: "ref_type".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
    };
    check(&input)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn setexpr_nested_path_mismatch_flags_ref_type() {
    // NESTED-path resolver: `scene.player` is a Record with `hp: number`; a
    // `def flag: bool` assigned to `scene.player.hp` is a clear mismatch. This
    // exercises `set_op::resolve_type`'s descend-into-Record branch (the
    // reviewer's required correction over exact-key lookup).
    let t = format!(
        "{HDR}state:\n  \
         scene.player: {{ type: {{ record: [ {{ name: hp, type: number }} ] }} }}\n\
         defs:\n  flag: {{ type: bool, cel: \"true\" }}\n---\n## Shot 1.\n\
         ::set{{scene.player.hp = @flag}}\n"
    );
    assert!(
        codes(&t).contains(&"E-REF-TYPE".to_string()),
        "bool def assigned to a nested number field must flag E-REF-TYPE; got {:?}",
        codes(&t)
    );
}

#[test]
fn condition_number_def_flags_ref_type() {
    // A `def num: number` used as a `<when test>` guard (Condition ⇒ Bool) is a
    // clear mismatch: number is not bool.
    let t = format!(
        "{HDR}state:\n  scene.n: {{ type: number, default: 0 }}\n\
         defs:\n  num: {{ type: number, cel: \"scene.n\" }}\n---\n## Shot 1.\n\
         <match on=\"scene.n\">\n\
         <when test=\"@num\">:line[narrator]: a\n</when>\n\
         <otherwise>:line[narrator]: b\n</otherwise>\n\
         </match>\n"
    );
    assert!(
        codes(&t).contains(&"E-REF-TYPE".to_string()),
        "number def in a bool guard must flag E-REF-TYPE; got {:?}",
        codes(&t)
    );
}

#[test]
fn condition_bool_def_is_clean() {
    // A `def ok: bool` used as a `<when test>` guard is compatible ⇒ no flag.
    let t = format!(
        "{HDR}state:\n  scene.flag: {{ type: bool, default: false }}\n\
         defs:\n  ok: {{ type: bool, cel: \"scene.flag\" }}\n---\n## Shot 1.\n\
         <match on=\"scene.flag\">\n\
         <when test=\"@ok\">:line[narrator]: a\n</when>\n\
         <otherwise>:line[narrator]: b\n</otherwise>\n\
         </match>\n"
    );
    assert!(
        !codes(&t).contains(&"E-REF-TYPE".to_string()),
        "bool def in a bool guard must NOT flag E-REF-TYPE; got {:?}",
        codes(&t)
    );
}

#[test]
fn bianca_style_fond_bool_def_is_clean() {
    // Mirrors the real `docs/examples/bianca-s01ep02.lute` def
    // `fond: { type: bool, cel: "scene.affect.bianca >= 1" }`, used only in a
    // bool guard position ⇒ must NOT gain E-REF-TYPE.
    let t = format!(
        "{HDR}state:\n  \
         scene.affect.bianca: {{ type: number, default: 0 }}\n  \
         scene.choices.number: {{ type: string, default: \"\" }}\n\
         defs:\n  fond: {{ type: bool, cel: \"scene.affect.bianca >= 1\" }}\n---\n## Shot 1.\n\
         <match on=\"scene.choices.number\">\n\
         <when test=\"@fond\">:line[fixer]: a\n</when>\n\
         <otherwise>:line[fixer]: b\n</otherwise>\n\
         </match>\n"
    );
    assert!(
        !codes(&t).contains(&"E-REF-TYPE".to_string()),
        "bianca's `fond: bool` in a bool guard must NOT flag E-REF-TYPE; got {:?}",
        codes(&t)
    );
}

// --- B whole-branch fix: plugin-exported defs (`snapshot.defs`) are declared
// `@refs` (dsl §8.1). `ctx.defs` must union inline frontmatter defs with plugin
// def names; otherwise a whole-slot `@pluginDef` is falsely `E-UNDECLARED-REF`
// and can never reach the `E-REF-TYPE` branch. These cases drive a SYNTHETIC
// in-memory `snapshot.defs` for isolation/convenience (no fixture I/O); the
// on-disk loader path (load_plugins_dir -> assemble_snapshot -> snapshot.defs)
// is covered end-to-end by `crates/lute-check/tests/plugin_defs_disk.rs`.

/// Like `codes`, but drives `check()` over a caller-supplied snapshot so a
/// plugin-exported def can be injected into `snap.defs`.
fn check_codes(text: &str, snap: CapabilitySnapshot) -> Vec<String> {
    let input = CheckInput {
        text: text.to_string(),
        uri: "ref_type".into(),
        snapshot: snap,
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
    };
    check(&input)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

// Minimal valid scenes whose whole `<when test>` value is a bare plugin-def ref
// (a Condition slot ⇒ expected Bool). Only the `test=` ref differs.
const SCENE_WARMTH: &str = "---\ncharacter: x\nseason: 1\nepisode: 1\n\
    state:\n  scene.flag: { type: bool, default: false }\n---\n## Shot 1.\n\
    <match on=\"scene.flag\">\n\
    <when test=\"@warmth\">:line[narrator]: a\n</when>\n\
    <otherwise>:line[narrator]: b\n</otherwise>\n\
    </match>\n";
const SCENE_COUNT: &str = "---\ncharacter: x\nseason: 1\nepisode: 1\n\
    state:\n  scene.flag: { type: bool, default: false }\n---\n## Shot 1.\n\
    <match on=\"scene.flag\">\n\
    <when test=\"@count\">:line[narrator]: a\n</when>\n\
    <otherwise>:line[narrator]: b\n</otherwise>\n\
    </match>\n";

#[test]
fn plugin_def_ref_is_declared_and_type_checked() {
    // A plugin-exported BOOL def used whole-slot in a bool guard: NOT undeclared
    // (it is a declared def via `snapshot.defs`), NOT E-REF-TYPE (bool ~ bool).
    let mut snap = lute_manifest::core::load_core_snapshot();
    snap.defs.insert(
        "warmth".into(),
        DefDecl {
            name: "warmth".into(),
            ty: Type::Bool,
            params: Default::default(),
            cel: "true".into(),
            min: None,
            max: None,
            values: None,
        },
    );
    let codes = check_codes(SCENE_WARMTH, snap);
    assert!(
        !codes.contains(&"E-UNDECLARED-REF".to_string()),
        "plugin def must be declared; got {codes:?}"
    );
    assert!(
        !codes.contains(&"E-REF-TYPE".to_string()),
        "bool def in bool guard is compatible; got {codes:?}"
    );
}

#[test]
fn plugin_def_ref_type_mismatch_flags() {
    // A plugin-exported NUMBER def used whole-slot in a bool guard: NOT
    // undeclared, but reaches E-REF-TYPE (number is not bool).
    let mut snap = lute_manifest::core::load_core_snapshot();
    snap.defs.insert(
        "count".into(),
        DefDecl {
            name: "count".into(),
            ty: Type::Number,
            params: Default::default(),
            cel: "1".into(),
            min: None,
            max: None,
            values: None,
        },
    );
    let codes = check_codes(SCENE_COUNT, snap);
    assert!(
        !codes.contains(&"E-UNDECLARED-REF".to_string()),
        "got {codes:?}"
    );
    assert!(
        codes.contains(&"E-REF-TYPE".to_string()),
        "number def in bool guard must flag; got {codes:?}"
    );
}

#[test]
fn call_form_whole_slot_number_def_in_bool_guard_flags_ref_type() {
    // A parameterized-call def (`@name(args)`, dsl §8.1) whose produced type is
    // NUMBER, used whole-slot in a bool guard, must reach E-REF-TYPE exactly like
    // the bare `@name` form. `params` is empty here (DefParam lands in P2.3); the
    // whole-slot type check depends only on `ty` and the call being whole-slot.
    let mut snap = lute_manifest::core::load_core_snapshot();
    snap.defs.insert(
        "countAtLeast".into(),
        DefDecl {
            name: "countAtLeast".into(),
            ty: Type::Number,
            params: Default::default(),
            cel: "1".into(),
            min: None,
            max: None,
            values: None,
        },
    );
    let scene = "---\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  scene.flag: { type: bool, default: false }\n---\n## Shot 1.\n<match on=\"scene.flag\">\n<when test=\"@countAtLeast(2)\">:line[narrator]: a\n</when>\n<otherwise>:line[narrator]: b\n</otherwise>\n</match>\n";
    let codes = check_codes(scene, snap);
    assert!(
        codes.contains(&"E-REF-TYPE".to_string()),
        "number-producing @call in a bool guard must flag; got {codes:?}"
    );
}
