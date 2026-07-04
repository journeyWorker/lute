//! Group D — defassign×exhaustiveness regression tests (plan 2026-07-02, Phase 1).
use lute_check::{check, CheckInput, Mode, SchemaImports};
use lute_manifest::provider::ProviderSet;

const HDR: &str = "---\ncharacter: x\nseason: 1\nepisode: 1\n";

fn codes(text: &str) -> Vec<String> {
    let input = CheckInput {
        text: text.to_string(),
        uri: "group_d".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
    };
    check(&input)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn c5_nested_match_on_dollar_is_error() {
    let t = format!(
        "{HDR}state:\n  scene.g: {{ type: bool, default: false }}\n---\n## Shot 1.\n\
         <match on=\"scene.g\">\n\
         <when test=\"$ == true\">\n\
           <match on=\"$\">\n\
           <otherwise>:line[narrator]: a\n</otherwise>\n\
           </match>\n\
         </when>\n\
         <otherwise>:line[narrator]: b\n</otherwise>\n\
         </match>\n"
    );
    assert!(
        codes(&t).contains(&"E-DOLLAR-OUTSIDE-MATCH".to_string()),
        "nested `<match on=\"$\">` must report E-DOLLAR-OUTSIDE-MATCH (dsl §8.2); got {:?}",
        codes(&t)
    );
}

#[test]
fn c4_disjunctive_guard_does_not_prove_read() {
    // isSet(run.x) is under `||`, so it does NOT prove `run.x`; the `run.x > 0`
    // read of a non-defaulted run tier is E-MAYBE-UNSET (dsl §9.4).
    let t = format!(
        "{HDR}state:\n  run.x: {{ type: number }}\n  scene.y: {{ type: bool, default: false }}\n---\n## Shot 1.\n\
         <match on=\"scene.y\">\n\
         <when test=\"isSet(run.x) || run.x > 0\">:line[narrator]: a\n</when>\n\
         <otherwise>:line[narrator]: b\n</otherwise>\n\
         </match>\n"
    );
    assert!(
        codes(&t).contains(&"E-MAYBE-UNSET".to_string()),
        "disjunctive guard must NOT prove the read (C4); got {:?}",
        codes(&t)
    );
}

#[test]
fn c4_conjunctive_guard_still_proves_read() {
    // Regression guard: a top-level / conjunctive `isSet` MUST still prove.
    let t = format!(
        "{HDR}state:\n  run.x: {{ type: number }}\n  scene.y: {{ type: bool, default: false }}\n---\n## Shot 1.\n\
         <match on=\"scene.y\">\n\
         <when test=\"isSet(run.x) && run.x > 0\">:line[narrator]: a\n</when>\n\
         <otherwise>:line[narrator]: b\n</otherwise>\n\
         </match>\n"
    );
    assert!(
        !codes(&t).contains(&"E-MAYBE-UNSET".to_string()),
        "conjunctive isSet must still prove the read; got {:?}",
        codes(&t)
    );
}

#[test]
fn c1_scene_bool_unwritten_subject_is_maybe_unset() {
    // Non-default scene.bool, never written, match covers {true,false}, NO otherwise.
    // The unset subject read must NOT be suppressed (dsl §9.4).
    let t = format!(
        "{HDR}state:\n  scene.flag: {{ type: bool }}\n---\n## Shot 1.\n\
         <match on=\"scene.flag\">\n\
         <when test=\"$ == true\">:line[narrator]: a\n</when>\n\
         <when test=\"$ == false\">:line[narrator]: b\n</when>\n\
         </match>\n"
    );
    assert!(
        codes(&t).contains(&"E-MAYBE-UNSET".to_string()),
        "unwritten non-default scene subject must be E-MAYBE-UNSET (C1); got {:?}",
        codes(&t)
    );
}

#[test]
fn c1b_scene_bool_written_subject_is_clean() {
    // REGRESSION GUARD: the same subject, WRITTEN before the match, is clean —
    // no E-MAYBE-UNSET (proven) and no false-positive E-UNSET-UNCOVERED.
    let t = format!(
        "{HDR}state:\n  scene.flag: {{ type: bool }}\n---\n## Shot 1.\n\
         ::set{{scene.flag = true}}\n\
         <match on=\"scene.flag\">\n\
         <when test=\"$ == true\">:line[narrator]: a\n</when>\n\
         <when test=\"$ == false\">:line[narrator]: b\n</when>\n\
         </match>\n"
    );
    let c = codes(&t);
    assert!(
        !c.contains(&"E-MAYBE-UNSET".to_string()),
        "written subject must be proven; got {c:?}"
    );
    assert!(
        !c.contains(&"E-UNSET-UNCOVERED".to_string()),
        "written scene subject must not need an unset arm; got {c:?}"
    );
}

#[test]
fn c2_exhaustive_match_without_otherwise_folds_assignment() {
    // Domain-exhaustive bool match (default false so not maybe-unset), both arms
    // assign scene.x; the read AFTER the match must be proven (no E-MAYBE-UNSET).
    let t = format!(
        "{HDR}state:\n  scene.g: {{ type: bool, default: false }}\n  scene.x: {{ type: number }}\n---\n## Shot 1.\n\
         <match on=\"scene.g\">\n\
         <when test=\"$ == true\">\n::set{{scene.x = 1}}\n</when>\n\
         <when test=\"$ == false\">\n::set{{scene.x = 2}}\n</when>\n\
         </match>\n\
         ::set{{scene.x += 5}}\n"
    );
    assert!(
        !codes(&t).contains(&"E-MAYBE-UNSET".to_string()),
        "exhaustive-without-otherwise both-arms-assign must fold (C2); got {:?}",
        codes(&t)
    );
}

#[test]
fn c2b_nonexhaustive_match_does_not_fold_assignment() {
    // REGRESSION GUARD: a NON-exhaustive match (one arm only, no otherwise) must
    // NOT fold — the read after is genuinely maybe-unset.
    let t = format!(
        "{HDR}state:\n  scene.g: {{ type: bool, default: false }}\n  scene.x: {{ type: number }}\n---\n## Shot 1.\n\
         <match on=\"scene.g\">\n\
         <when test=\"$ == true\">\n::set{{scene.x = 1}}\n</when>\n\
         </match>\n\
         ::set{{scene.x += 5}}\n"
    );
    assert!(
        codes(&t).contains(&"E-MAYBE-UNSET".to_string()),
        "non-exhaustive match must NOT fold the assignment; got {:?}",
        codes(&t)
    );
}
