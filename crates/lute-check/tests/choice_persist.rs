//! F1 — `<choice … into="run.<path>" [value="<lit>"]>` run-record sugar
//! validation (dsl 0.6.0 §2). `into=` ALONE records; the `persist=` attribute
//! was REMOVED in 0.6.0 (`E-PERSIST-REMOVED`, with a machine-applicable
//! deletion). The checker validates well-formedness of the run-fact promotion
//! (the engine materializes the `::set`). Fed through the assembled `check()`
//! over inline `state:` frontmatter (mirrors `ref_type.rs`'s
//! `codes()`-over-inline-schema harness).
use lute_check::{check, fix_document, CheckInput, CheckResult, Mode, SchemaImports};
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

/// Like [`codes`] but returns the full [`CheckResult`] — the `res.ok` (B4) and
/// per-diagnostic `severity`/`fixits` assertions need more than the code list.
fn diagnose(text: &str) -> CheckResult {
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
}

/// A clean record carries no record-sugar diagnostic (`E-PERSIST-REMOVED`,
/// `E-INTO-*`, `W-INTO-SET-DUP`) and no `E-UNDECLARED` (the `into` path
/// resolves).
fn assert_clean(cs: &[String]) {
    assert!(
        !cs.iter().any(|c| c.starts_with("E-INTO")
            || c == "E-PERSIST-REMOVED"
            || c == "W-INTO-SET-DUP"
            || c == "E-UNDECLARED"),
        "expected no record-sugar/undeclared diagnostics; got {cs:?}"
    );
}

#[test]
fn into_bool_default_records_clean() {
    // `run.helped: bool` — `value` is optional (defaults to `true`); a bare
    // `into=` alone records the fact and checks clean (0.6.0 §2.1: the ≤0.5.2
    // silent-no-op trap is gone — `into=` alone now records).
    let t = format!(
        "{HDR}state:\n  run.helped: {{ type: bool }}\n---\n## Shot 1.\n\
         <branch id=\"b\">\n\
         <choice id=\"c\" label=\"Help\" into=\"run.helped\">\n\
         </choice>\n\
         </branch>\n"
    );
    assert_clean(&codes(&t));
}

#[test]
fn into_bool_default_records_clean_on_hub_choice() {
    // Same, on a hub choice (§2 covers both `<branch>` and `<hub>` choices).
    let t = format!(
        "{HDR}state:\n  run.helped: {{ type: bool }}\n---\n## Shot 1.\n\
         <hub id=\"h\">\n\
         <choice id=\"c\" label=\"Help\" into=\"run.helped\" exit>\n\
         </choice>\n\
         </hub>\n"
    );
    assert_clean(&codes(&t));
}

#[test]
fn into_number_requires_value() {
    // `run.score: number` — `value` is REQUIRED for a number path; omitting it
    // is `E-INTO-VALUE`.
    let t = format!(
        "{HDR}state:\n  run.score: {{ type: number }}\n---\n## Shot 1.\n\
         <branch id=\"b\">\n\
         <choice id=\"c\" label=\"Score\" into=\"run.score\">\n\
         </choice>\n\
         </branch>\n"
    );
    assert!(
        codes(&t).contains(&"E-INTO-VALUE".to_string()),
        "a number record path without `value` must flag E-INTO-VALUE; got {:?}",
        codes(&t)
    );
}

#[test]
fn into_number_value_ok() {
    // `run.score: number` with `value="3"` — a numeric literal is compatible → clean.
    let t = format!(
        "{HDR}state:\n  run.score: {{ type: number }}\n---\n## Shot 1.\n\
         <branch id=\"b\">\n\
         <choice id=\"c\" label=\"Score\" into=\"run.score\" value=\"3\">\n\
         </choice>\n\
         </branch>\n"
    );
    assert_clean(&codes(&t));
}

#[test]
fn into_undeclared_errors() {
    // `into="run.ghost"` is not declared in the schema; state-by-typo must fail
    // with `E-INTO-UNDECLARED` (never silently create a run field).
    let t = format!(
        "{HDR}state:\n  run.helped: {{ type: bool }}\n---\n## Shot 1.\n\
         <branch id=\"b\">\n\
         <choice id=\"c\" label=\"Ghost\" into=\"run.ghost\">\n\
         </choice>\n\
         </branch>\n"
    );
    let cs = codes(&t);
    assert!(
        cs.contains(&"E-INTO-UNDECLARED".to_string()),
        "an undeclared `into` path must flag E-INTO-UNDECLARED; got {cs:?}"
    );
}

#[test]
fn into_non_run_target_errors() {
    // `into="scene.x"` is not a `run.*` path → `E-INTO-TARGET`.
    let t = format!(
        "{HDR}state:\n  scene.x: {{ type: bool }}\n---\n## Shot 1.\n\
         <branch id=\"b\">\n\
         <choice id=\"c\" label=\"Scene\" into=\"scene.x\">\n\
         </choice>\n\
         </branch>\n"
    );
    assert!(
        codes(&t).contains(&"E-INTO-TARGET".to_string()),
        "a non-run `into` target must flag E-INTO-TARGET; got {:?}",
        codes(&t)
    );
}

#[test]
fn into_bare_run_target_errors() {
    // `into="run"` (the bare namespace, no `.path`) names no run fact. It must
    // be rejected as an ill-formed target (`E-INTO-TARGET`) BEFORE the schema
    // lookup — never falling through to `E-INTO-UNDECLARED`.
    let t = format!(
        "{HDR}state:\n  run.helped: {{ type: bool }}\n---\n## Shot 1.\n\
         <branch id=\"b\">\n\
         <choice id=\"c\" label=\"Bare\" into=\"run\">\n\
         </choice>\n\
         </branch>\n"
    );
    let cs = codes(&t);
    assert!(
        cs.contains(&"E-INTO-TARGET".to_string()),
        "a bare `into=\"run\"` target must flag E-INTO-TARGET; got {cs:?}"
    );
    assert!(
        !cs.contains(&"E-INTO-UNDECLARED".to_string()),
        "a bare `into=\"run\"` must not fall through to E-INTO-UNDECLARED; got {cs:?}"
    );
}

#[test]
fn into_non_string_target_errors() {
    // A bare `into` flag (no `="run.<path>"`) is not a string literal target →
    // `E-INTO-TARGET`.
    let t = format!(
        "{HDR}state:\n  run.helped: {{ type: bool }}\n---\n## Shot 1.\n\
         <branch id=\"b\">\n\
         <choice id=\"c\" label=\"Flag\" into>\n\
         </choice>\n\
         </branch>\n"
    );
    assert!(
        codes(&t).contains(&"E-INTO-TARGET".to_string()),
        "a non-string `into` must flag E-INTO-TARGET; got {:?}",
        codes(&t)
    );
}

#[test]
fn into_wrong_value_type_errors() {
    // `run.helped: bool` with `value="7"` — a numeric literal is not a bool
    // literal → `E-INTO-VALUE`.
    let t = format!(
        "{HDR}state:\n  run.helped: {{ type: bool }}\n---\n## Shot 1.\n\
         <branch id=\"b\">\n\
         <choice id=\"c\" label=\"Help\" into=\"run.helped\" value=\"7\">\n\
         </choice>\n\
         </branch>\n"
    );
    assert!(
        codes(&t).contains(&"E-INTO-VALUE".to_string()),
        "a non-bool `value` for a bool record path must flag E-INTO-VALUE; got {:?}",
        codes(&t)
    );
}

#[test]
fn into_enum_member_spelled_like_bool_or_number_ok() {
    // `run.tier: enum["true", "3", "gold"]` — members happen to be spelled like
    // a bool / a number. The `value` must be judged against the RESOLVED enum
    // type (verbatim string membership), not eagerly coerced to Bool/Num, so
    // `value="3"` and `value="true"` are both valid members → clean.
    let t = format!(
        "{HDR}state:\n  run.tier: {{ type: {{ enum: [\"true\", \"3\", \"gold\"] }} }}\n---\n\
         ## Shot 1.\n\
         <branch id=\"b\">\n\
         <choice id=\"c1\" label=\"Three\" into=\"run.tier\" value=\"3\">\n\
         </choice>\n\
         <choice id=\"c2\" label=\"True\" into=\"run.tier\" value=\"true\">\n\
         </choice>\n\
         </branch>\n"
    );
    assert_clean(&codes(&t));
}

#[test]
fn into_arm_conflict_warns() {
    // The arm already `::set`s the same path the record sugar would write — the
    // record write duplicates it: `W-INTO-SET-DUP` (a WARNING; never flips the
    // verdict, B4).
    let t = format!(
        "{HDR}state:\n  run.helped: {{ type: bool }}\n---\n## Shot 1.\n\
         <branch id=\"b\">\n\
         <choice id=\"c\" label=\"Help\" into=\"run.helped\">\n\
         ::set{{run.helped = false}}\n\
         </choice>\n\
         </branch>\n"
    );
    let res = diagnose(&t);
    let dups: Vec<_> = res
        .diagnostics
        .iter()
        .filter(|d| d.code == "W-INTO-SET-DUP")
        .collect();
    assert_eq!(
        dups.len(),
        1,
        "an arm `::set` duplicating the `into` path must warn exactly once; got {:?}",
        res.diagnostics
    );
    assert!(
        res.ok,
        "a warning must never flip the verdict (B4); got {:?}",
        res.diagnostics
    );
}

#[test]
fn content_line_as_still_label_override() {
    // dsl §7.1: `as=` on a CONTENT LINE is the display-label override, NOT the
    // record sugar. A `@speaker{as="???"}: text` line carries no `into`, so the
    // record family never fires — the rename of the record target attr
    // (`as`→`into`, §2) leaves content-line `as` untouched.
    let t = format!(
        "{HDR}state:\n  run.helped: {{ type: bool }}\n---\n## Shot 1.\n\
         @bianca{{as=\"Hostess\"}}: Welcome in.\n"
    );
    assert_clean(&codes(&t));
}

#[test]
fn no_into_no_record_diagnostics() {
    // A choice with neither `into=` nor `persist=` is ordinary — no record
    // sugar to validate, no diagnostics.
    let t = format!(
        "{HDR}---\n## Shot 1.\n\
         <branch id=\"b\">\n\
         <choice id=\"c\" label=\"Plain\">\n\
         </choice>\n\
         </branch>\n"
    );
    assert_clean(&codes(&t));
}

#[test]
fn into_alone_records_clean_then_persist_reports_removed() {
    // Acceptance: `<choice … into="run.x">` (declared bool run.x) checks with
    // ZERO diagnostics; adding `persist="run"` reports exactly one
    // `E-PERSIST-REMOVED` whose single `"migrate"` fixit deletes the attr.
    let clean = format!(
        "{HDR}state:\n  run.x: {{ type: bool }}\n---\n## Shot 1.\n\
         <branch id=\"b\">\n\
         <choice id=\"h\" label=\"L\" into=\"run.x\">\n\
         </choice>\n\
         </branch>\n"
    );
    let res = diagnose(&clean);
    assert!(
        res.diagnostics.is_empty(),
        "a bare declared-bool `into=` must record with ZERO diagnostics; got {:?}",
        res.diagnostics
    );
    assert!(res.ok);

    let with_persist = format!(
        "{HDR}state:\n  run.x: {{ type: bool }}\n---\n## Shot 1.\n\
         <branch id=\"b\">\n\
         <choice id=\"h\" label=\"L\" persist=\"run\" into=\"run.x\">\n\
         </choice>\n\
         </branch>\n"
    );
    let res = diagnose(&with_persist);
    assert_eq!(
        res.diagnostics.len(),
        1,
        "adding persist=\"run\" to a clean into= must report exactly one diagnostic; got {:?}",
        res.diagnostics
    );
    let d = &res.diagnostics[0];
    assert_eq!(d.code, "E-PERSIST-REMOVED");
    assert!(
        !res.ok,
        "E-PERSIST-REMOVED is an error and must flip the verdict; got {:?}",
        res.diagnostics
    );
    assert_eq!(d.fixits.len(), 1, "exactly one migrate fixit; got {:?}", d.fixits);
    let fx = &d.fixits[0];
    assert_eq!(
        fx.kind, "migrate",
        "the fixit must be `migrate` so `lute fix` applies it; got {:?}",
        fx
    );
    assert_eq!(fx.confidence, 100);
    let e = &fx.edit[0];
    let mut spliced = with_persist.clone();
    spliced.replace_range(e.span.byte_start..e.span.byte_end, &e.new_text);
    assert!(
        spliced.contains("<choice id=\"h\" label=\"L\" into=\"run.x\">"),
        "the fixit must delete `persist=\"run\"` cleanly; got:\n{spliced}"
    );
    assert!(!spliced.contains("persist="), "got:\n{spliced}");
}

#[test]
fn persist_removed_flags_any_value() {
    // §2.2: a `<choice>` carrying `persist=` with ANY value is `E-PERSIST-
    // REMOVED` (the attribute is gone from the language, not merely constrained
    // to `"run"`).
    let t = format!(
        "{HDR}state:\n  run.x: {{ type: bool }}\n---\n## Shot 1.\n\
         <branch id=\"b\">\n\
         <choice id=\"h\" label=\"L\" persist=\"scene\" into=\"run.x\">\n\
         </choice>\n\
         </branch>\n"
    );
    assert!(
        codes(&t).contains(&"E-PERSIST-REMOVED".to_string()),
        "any `persist=` value must flag E-PERSIST-REMOVED; got {:?}",
        codes(&t)
    );
}

#[test]
fn persist_without_into_still_reports_removed() {
    // A `persist=` with no `into=` is still `E-PERSIST-REMOVED` — the attribute
    // was removed regardless of whether a record target is present.
    let t = format!(
        "{HDR}state:\n  run.x: {{ type: bool }}\n---\n## Shot 1.\n\
         <branch id=\"b\">\n\
         <choice id=\"h\" label=\"L\" persist=\"run\">\n\
         </choice>\n\
         </branch>\n"
    );
    assert!(
        codes(&t).contains(&"E-PERSIST-REMOVED".to_string()),
        "persist without into must still flag E-PERSIST-REMOVED; got {:?}",
        codes(&t)
    );
}

#[test]
fn fix_rule_round_trips_persist_into_to_bare_into() {
    // `lute fix`'s persist-removal rule (dsl 0.6.0 §2.3): a `persist="run"` +
    // `into=` pair → deletes persist → byte-expected bare-`into=` form → the
    // migrated document checks with ZERO diagnostics.
    let before = format!(
        "{HDR}state:\n  run.x: {{ type: bool }}\n---\n## Shot 1.\n\
         <branch id=\"b\">\n\
         <choice id=\"h\" label=\"L\" persist=\"run\" into=\"run.x\">\n\
         </choice>\n\
         </branch>\n"
    );
    let fixed = fix_document(&before);
    assert_eq!(
        fixed.changed, 1,
        "exactly the persist attr is deleted; got:\n{}",
        fixed.text
    );
    let expected = format!(
        "{HDR}state:\n  run.x: {{ type: bool }}\n---\n## Shot 1.\n\
         <branch id=\"b\">\n\
         <choice id=\"h\" label=\"L\" into=\"run.x\">\n\
         </choice>\n\
         </branch>\n"
    );
    assert_eq!(fixed.text, expected, "got:\n{}", fixed.text);
    let res = diagnose(&fixed.text);
    assert!(
        res.diagnostics.is_empty(),
        "the migrated document must check clean; got {:?}",
        res.diagnostics
    );
}

// --- dsl 0.10.0 §4 / D-L: `as=` is a RENAMED attribute, not an unknown one --

/// `as=` on a branch choice is `E-AS-REMOVED`, anchored column-exact at the
/// attribute's own key — mirroring `E-PERSIST-REMOVED` in severity, layer, span
/// behaviour and kind.
#[test]
fn as_on_branch_choice_is_as_removed() {
    let t = format!(
        "{HDR}state:\n  run.x: {{ type: bool }}\n---\n## Shot 1.\n\
         <branch id=\"b\">\n\
         <choice id=\"c\" label=\"L\" as=\"run.x\">\n\
         </choice>\n\
         </branch>\n"
    );
    let res = diagnose(&t);
    let d = res
        .diagnostics
        .iter()
        .find(|d| d.code == "E-AS-REMOVED")
        .unwrap_or_else(|| panic!("expected E-AS-REMOVED: {:?}", res.diagnostics));
    assert_eq!(d.severity, lute_core_span::Severity::Error);
    assert_eq!(d.layer, lute_core_span::Layer::Logic);
    assert_eq!(&t[d.span.byte_start..d.span.byte_end], "as=\"run.x\"");
    assert!(!res.ok, "an error flips the verdict: {:?}", res.diagnostics);
    // §4's fourth column: the closure rule must NOT also claim it.
    assert!(
        !res.diagnostics.iter().any(|x| x.code == "E-UNKNOWN-ATTR"),
        "each removal is reported once, by its own code: {:?}",
        res.diagnostics
    );
}

/// `as=` on a HUB choice is the same error — §4's table lists it in both
/// positions, and `check_choice_record` is reached from both walkers.
#[test]
fn as_on_hub_choice_is_as_removed() {
    let t = format!(
        "{HDR}state:\n  run.x: {{ type: bool }}\n---\n## Shot 1.\n\
         <hub id=\"h\">\n\
         <choice id=\"c\" label=\"L\" as=\"run.x\" exit>\n\
         </choice>\n\
         </hub>\n"
    );
    assert!(codes(&t).contains(&"E-AS-REMOVED".to_string()), "{:?}", codes(&t));
}

/// The fixit is a 2→4 byte KEY REPLACEMENT with NO widening. This is where it
/// differs from `E-PERSIST-REMOVED`, whose edit is a deletion widened by
/// `widen_attr_delete` to swallow one adjacent separating space; reusing that
/// helper here would eat the space before `as` and produce `label="L"into=…`.
#[test]
fn as_removed_fixit_replaces_only_the_key() {
    let t = format!(
        "{HDR}state:\n  run.x: {{ type: bool }}\n---\n## Shot 1.\n\
         <branch id=\"b\">\n\
         <choice id=\"c\" label=\"L\" as=\"run.x\">\n\
         </choice>\n\
         </branch>\n"
    );
    let res = diagnose(&t);
    let d = res.diagnostics.iter().find(|d| d.code == "E-AS-REMOVED").unwrap();
    assert_eq!(d.fixits.len(), 1, "exactly one migrate fixit; got {:?}", d.fixits);
    let fx = &d.fixits[0];
    assert_eq!(fx.kind, "migrate");
    assert_eq!(fx.confidence, 100);
    assert_eq!(fx.edit.len(), 1);
    let e = &fx.edit[0];
    assert_eq!(
        e.span.byte_end - e.span.byte_start,
        2,
        "the edit spans exactly the two bytes of `as`, never a widened region"
    );
    assert_eq!(e.new_text, "into");
    let mut spliced = t.clone();
    spliced.replace_range(e.span.byte_start..e.span.byte_end, &e.new_text);
    assert!(
        spliced.contains("<choice id=\"c\" label=\"L\" into=\"run.x\">"),
        "got:\n{spliced}"
    );
}

/// The LSP fixit and `lute fix` are TWO INDEPENDENT implementations —
/// `lute fix` never reads `Diagnostic.fixits` (`lute-lsp/src/code_action.rs:7`),
/// so `fix.rs:134-135` and the fixit above can drift silently. This test is the
/// only thing holding them byte-identical, and it is the hazard
/// `check.rs:2758-2759` already records for `persist`.
#[test]
fn lsp_fixit_and_lute_fix_agree_byte_for_byte() {
    let before = format!(
        "{HDR}state:\n  run.x: {{ type: bool }}\n---\n## Shot 1.\n\
         <branch id=\"b\">\n\
         <choice id=\"c\" label=\"L\" as=\"run.x\">\n\
         </choice>\n\
         </branch>\n"
    );
    let d = diagnose(&before)
        .diagnostics
        .into_iter()
        .find(|d| d.code == "E-AS-REMOVED")
        .expect("expected E-AS-REMOVED");
    let e = &d.fixits[0].edit[0];
    let mut via_lsp = before.clone();
    via_lsp.replace_range(e.span.byte_start..e.span.byte_end, &e.new_text);

    let via_fix = fix_document(&before);
    assert_eq!(
        via_fix.text, via_lsp,
        "`lute fix` and the LSP fixit MUST produce identical bytes"
    );
    let res = diagnose(&via_lsp);
    assert!(
        !res.diagnostics.iter().any(|x| x.code == "E-AS-REMOVED"),
        "the migrated document must be clean of the code: {:?}",
        res.diagnostics
    );
}

/// The migration is what restores the `set` record the document was silently
/// losing: with `into=`, the record sugar validates; with `as=`, nothing did.
#[test]
fn migrated_as_records_the_run_fact() {
    let after = format!(
        "{HDR}state:\n  run.x: {{ type: bool }}\n---\n## Shot 1.\n\
         <branch id=\"b\">\n\
         <choice id=\"c\" label=\"L\" into=\"run.x\">\n\
         </choice>\n\
         </branch>\n"
    );
    let res = diagnose(&after);
    assert!(res.diagnostics.is_empty(), "got {:?}", res.diagnostics);
}
