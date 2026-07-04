//! FEAT-3 — `<choice … persist="run" as="run.<path>" [value="<lit>"]>` sugar
//! validation (dsl §11.1.1). The checker validates well-formedness of the
//! run-fact promotion (the engine materializes the `::set`). Fed through the
//! assembled `check()` over inline `state:` frontmatter (mirrors `ref_type.rs`'s
//! `codes()`-over-inline-schema harness).
use lute_check::{check, CheckInput, Mode, SchemaImports};
use lute_manifest::provider::ProviderSet;

const HDR: &str = "---\ncharacter: x\nseason: 1\nepisode: 1\n";

fn codes(text: &str) -> Vec<String> {
    let input = CheckInput {
        text: text.to_string(),
        uri: "choice_persist".into(),
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

/// A clean result carries no persist-family diagnostic and no `E-UNDECLARED`
/// (the persist attrs are recognized and the `as` path resolves).
fn assert_clean(cs: &[String]) {
    assert!(
        !cs.iter()
            .any(|c| c.starts_with("E-PERSIST") || c == "E-UNDECLARED"),
        "expected no persist/undeclared diagnostics; got {cs:?}"
    );
}

#[test]
fn persist_bool_default_true_ok() {
    // `run.helped: bool` — `value` is optional (defaults to `true`) → clean.
    let t = format!(
        "{HDR}state:\n  run.helped: {{ type: bool }}\n---\n## Shot 1.\n\
         <branch id=\"b\">\n\
         <choice id=\"c\" label=\"Help\" persist=\"run\" as=\"run.helped\">\n\
         </choice>\n\
         </branch>\n"
    );
    assert_clean(&codes(&t));
}

#[test]
fn persist_number_requires_value() {
    // `run.score: number` — `value` is REQUIRED for a number path; omitting it
    // is `E-PERSIST-VALUE`.
    let t = format!(
        "{HDR}state:\n  run.score: {{ type: number }}\n---\n## Shot 1.\n\
         <branch id=\"b\">\n\
         <choice id=\"c\" label=\"Score\" persist=\"run\" as=\"run.score\">\n\
         </choice>\n\
         </branch>\n"
    );
    assert!(
        codes(&t).contains(&"E-PERSIST-VALUE".to_string()),
        "a number persist path without `value` must flag E-PERSIST-VALUE; got {:?}",
        codes(&t)
    );
}

#[test]
fn persist_number_value_ok() {
    // `run.score: number` with `value="3"` — a numeric literal is compatible → clean.
    let t = format!(
        "{HDR}state:\n  run.score: {{ type: number }}\n---\n## Shot 1.\n\
         <branch id=\"b\">\n\
         <choice id=\"c\" label=\"Score\" persist=\"run\" as=\"run.score\" value=\"3\">\n\
         </choice>\n\
         </branch>\n"
    );
    assert_clean(&codes(&t));
}

#[test]
fn persist_undeclared_as_errors() {
    // `as="run.ghost"` is not declared in the schema; state-by-typo must fail
    // with `E-UNDECLARED` (never silently create a run field).
    let t = format!(
        "{HDR}state:\n  run.helped: {{ type: bool }}\n---\n## Shot 1.\n\
         <branch id=\"b\">\n\
         <choice id=\"c\" label=\"Ghost\" persist=\"run\" as=\"run.ghost\">\n\
         </choice>\n\
         </branch>\n"
    );
    assert!(
        codes(&t).contains(&"E-UNDECLARED".to_string()),
        "an undeclared `as` path must flag E-UNDECLARED; got {:?}",
        codes(&t)
    );
}

#[test]
fn persist_non_run_target_errors() {
    // `persist="run"` but `as="scene.x"` is not a `run.*` path → `E-PERSIST-TARGET`.
    let t = format!(
        "{HDR}state:\n  scene.x: {{ type: bool }}\n---\n## Shot 1.\n\
         <branch id=\"b\">\n\
         <choice id=\"c\" label=\"Scene\" persist=\"run\" as=\"scene.x\">\n\
         </choice>\n\
         </branch>\n"
    );
    assert!(
        codes(&t).contains(&"E-PERSIST-TARGET".to_string()),
        "a non-run `as` target must flag E-PERSIST-TARGET; got {:?}",
        codes(&t)
    );
}

#[test]
fn persist_missing_as_errors() {
    // `persist="run"` with no `as` → `E-PERSIST-MISSING-AS`.
    let t = format!(
        "{HDR}state:\n  run.helped: {{ type: bool }}\n---\n## Shot 1.\n\
         <branch id=\"b\">\n\
         <choice id=\"c\" label=\"NoAs\" persist=\"run\">\n\
         </choice>\n\
         </branch>\n"
    );
    assert!(
        codes(&t).contains(&"E-PERSIST-MISSING-AS".to_string()),
        "persist without `as` must flag E-PERSIST-MISSING-AS; got {:?}",
        codes(&t)
    );
}

#[test]
fn persist_arm_conflict_errors() {
    // The arm already `::set`s the same path the sugar would write → the persist
    // write duplicates it: `E-PERSIST-CONFLICT`.
    let t = format!(
        "{HDR}state:\n  run.helped: {{ type: bool }}\n---\n## Shot 1.\n\
         <branch id=\"b\">\n\
         <choice id=\"c\" label=\"Help\" persist=\"run\" as=\"run.helped\">\n\
         ::set{{run.helped = false}}\n\
         </choice>\n\
         </branch>\n"
    );
    assert!(
        codes(&t).contains(&"E-PERSIST-CONFLICT".to_string()),
        "a persist write duplicating an arm `::set` must flag E-PERSIST-CONFLICT; got {:?}",
        codes(&t)
    );
}

#[test]
fn persist_wrong_value_type_errors() {
    // `run.helped: bool` with `value="7"` — a numeric literal is not a bool
    // literal → `E-PERSIST-VALUE`.
    let t = format!(
        "{HDR}state:\n  run.helped: {{ type: bool }}\n---\n## Shot 1.\n\
         <branch id=\"b\">\n\
         <choice id=\"c\" label=\"Help\" persist=\"run\" as=\"run.helped\" value=\"7\">\n\
         </choice>\n\
         </branch>\n"
    );
    assert!(
        codes(&t).contains(&"E-PERSIST-VALUE".to_string()),
        "a non-bool `value` for a bool persist path must flag E-PERSIST-VALUE; got {:?}",
        codes(&t)
    );
}
