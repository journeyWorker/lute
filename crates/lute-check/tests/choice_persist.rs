//! FEAT-3 — `<choice … persist="run" into="run.<path>" [value="<lit>"]>` sugar
//! validation (dsl §11.1.1). The checker validates well-formedness of the
//! run-fact promotion (the engine materializes the `::set`). Fed through the
//! assembled `check()` over inline `state:` frontmatter (mirrors `ref_type.rs`'s
//! `codes()`-over-inline-schema harness).
use lute_check::{check, CheckInput, Mode, SchemaImports};
use lute_manifest::provider::ProviderSet;

const HDR: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n";

fn codes(text: &str) -> Vec<String> {
    let input = CheckInput {
        text: text.to_string(),
        uri: "choice_persist".into(),
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

/// A clean result carries no persist-family diagnostic and no `E-UNDECLARED`
/// (the persist attrs are recognized and the `into` path resolves).
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
         <choice id=\"c\" label=\"Help\" persist=\"run\" into=\"run.helped\">\n\
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
         <choice id=\"c\" label=\"Score\" persist=\"run\" into=\"run.score\">\n\
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
         <choice id=\"c\" label=\"Score\" persist=\"run\" into=\"run.score\" value=\"3\">\n\
         </choice>\n\
         </branch>\n"
    );
    assert_clean(&codes(&t));
}

#[test]
fn persist_undeclared_into_errors() {
    // `into="run.ghost"` is not declared in the schema; state-by-typo must fail
    // with `E-UNDECLARED` (never silently create a run field).
    let t = format!(
        "{HDR}state:\n  run.helped: {{ type: bool }}\n---\n## Shot 1.\n\
         <branch id=\"b\">\n\
         <choice id=\"c\" label=\"Ghost\" persist=\"run\" into=\"run.ghost\">\n\
         </choice>\n\
         </branch>\n"
    );
    assert!(
        codes(&t).contains(&"E-UNDECLARED".to_string()),
        "an undeclared `into` path must flag E-UNDECLARED; got {:?}",
        codes(&t)
    );
}

#[test]
fn persist_non_run_target_errors() {
    // `persist="run"` but `into="scene.x"` is not a `run.*` path → `E-PERSIST-TARGET`.
    let t = format!(
        "{HDR}state:\n  scene.x: {{ type: bool }}\n---\n## Shot 1.\n\
         <branch id=\"b\">\n\
         <choice id=\"c\" label=\"Scene\" persist=\"run\" into=\"scene.x\">\n\
         </choice>\n\
         </branch>\n"
    );
    assert!(
        codes(&t).contains(&"E-PERSIST-TARGET".to_string()),
        "a non-run `into` target must flag E-PERSIST-TARGET; got {:?}",
        codes(&t)
    );
}

#[test]
fn persist_missing_into_errors() {
    // `persist="run"` with no `into` → `E-PERSIST-MISSING-INTO`.
    let t = format!(
        "{HDR}state:\n  run.helped: {{ type: bool }}\n---\n## Shot 1.\n\
         <branch id=\"b\">\n\
         <choice id=\"c\" label=\"NoInto\" persist=\"run\">\n\
         </choice>\n\
         </branch>\n"
    );
    assert!(
        codes(&t).contains(&"E-PERSIST-MISSING-INTO".to_string()),
        "persist without `into` must flag E-PERSIST-MISSING-INTO; got {:?}",
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
         <choice id=\"c\" label=\"Help\" persist=\"run\" into=\"run.helped\">\n\
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
         <choice id=\"c\" label=\"Help\" persist=\"run\" into=\"run.helped\" value=\"7\">\n\
         </choice>\n\
         </branch>\n"
    );
    assert!(
        codes(&t).contains(&"E-PERSIST-VALUE".to_string()),
        "a non-bool `value` for a bool persist path must flag E-PERSIST-VALUE; got {:?}",
        codes(&t)
    );
}

#[test]
fn persist_enum_member_spelled_like_bool_or_number_ok() {
    // `run.tier: enum["true", "3", "gold"]` — members happen to be spelled like
    // a bool / a number. The `value` must be judged against the RESOLVED enum
    // type (verbatim string membership), not eagerly coerced to Bool/Num, so
    // `value="3"` and `value="true"` are both valid members → clean.
    let t = format!(
        "{HDR}state:\n  run.tier: {{ type: {{ enum: [\"true\", \"3\", \"gold\"] }} }}\n---\n\
         ## Shot 1.\n\
         <branch id=\"b\">\n\
         <choice id=\"c1\" label=\"Three\" persist=\"run\" into=\"run.tier\" value=\"3\">\n\
         </choice>\n\
         <choice id=\"c2\" label=\"True\" persist=\"run\" into=\"run.tier\" value=\"true\">\n\
         </choice>\n\
         </branch>\n"
    );
    assert_clean(&codes(&t));
}

#[test]
fn persist_bare_run_target_errors() {
    // `into="run"` (the bare namespace, no `.path`) names no run fact. It must be
    // rejected as an ill-formed target (`E-PERSIST-TARGET`) BEFORE the schema
    // lookup — never falling through to `E-UNDECLARED`.
    let t = format!(
        "{HDR}state:\n  run.helped: {{ type: bool }}\n---\n## Shot 1.\n\
         <branch id=\"b\">\n\
         <choice id=\"c\" label=\"Bare\" persist=\"run\" into=\"run\">\n\
         </choice>\n\
         </branch>\n"
    );
    let cs = codes(&t);
    assert!(
        cs.contains(&"E-PERSIST-TARGET".to_string()),
        "a bare `into=\"run\"` target must flag E-PERSIST-TARGET; got {cs:?}"
    );
    assert!(
        !cs.contains(&"E-UNDECLARED".to_string()),
        "a bare `into=\"run\"` must not fall through to E-UNDECLARED; got {cs:?}"
    );
}

#[test]
fn content_line_as_still_label_override() {
    // dsl §7.1: `as=` on a CONTENT LINE is the display-label override, NOT the
    // persist sugar. A `@speaker{as="???"}: text` line carries no `persist`, so
    // the persist family never fires — the rename of the persist target attr
    // (`as`→`into`, §11.1.1) leaves content-line `as` untouched.
    let t = format!(
        "{HDR}state:\n  run.helped: {{ type: bool }}\n---\n## Shot 1.\n\
         @bianca{{as=\"Hostess\"}}: Welcome in.\n"
    );
    assert_clean(&codes(&t));
}
