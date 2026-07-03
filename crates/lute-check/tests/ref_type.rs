//! B2.2 — `E-REF-TYPE`: a declared `@ref` def used in a CEL slot whose
//! statically-known expected type is incompatible with the def's produced type
//! (dsl §8). Fed through the assembled `check()` over inline `state:`/`defs:`
//! frontmatter (mirrors `group_d.rs`'s harness).
use lute_check::{check, CheckInput, Mode};
use lute_manifest::provider::ProviderSet;

const HDR: &str = "---\ncharacter: x\nseason: 1\nepisode: 1\n";

fn codes(text: &str) -> Vec<String> {
    let input = CheckInput {
        text: text.to_string(),
        uri: "ref_type".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
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
