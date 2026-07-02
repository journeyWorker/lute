//! Group D — defassign×exhaustiveness regression tests (plan 2026-07-02, Phase 1).
use lute_check::{check, CheckInput, Mode};
use lute_manifest::provider::ProviderSet;

const HDR: &str = "---\ncharacter: x\nseason: 1\nepisode: 1\n";

fn codes(text: &str) -> Vec<String> {
    let input = CheckInput {
        text: text.to_string(),
        uri: "group_d".into(),
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
