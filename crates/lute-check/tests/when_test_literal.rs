//! `W-WHEN-TEST-LITERAL` (dsl 0.18.0 §3): a `<when test>` that only compares
//! `$` to literals warns, carrying a `migrate` fixit that rewrites the
//! `test="…"` attribute to the equivalent `is="…"` pattern — the same rewrite
//! `lute fix` applies unprompted. Driven through the assembled `check()` and
//! `fix_document` over inline `state:` frontmatter.
use lute_check::{check, fix_document, CheckInput, CheckResult, Mode, SchemaImports};
use lute_core_span::{Diagnostic, Severity};
use lute_manifest::provider::ProviderSet;

const CODE: &str = "W-WHEN-TEST-LITERAL";

const HDR: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  \
    run.rank: { type: { enum: [fail, bronze, silver, gold] }, default: fail }\n  \
    run.n: { type: number, default: 0 }\n---\n## Shot 1.\n";

fn diagnose(text: &str) -> CheckResult {
    check(&CheckInput {
        text: text.to_string(),
        uri: "when_test_literal".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
        defaults: Default::default(),
    })
}

fn lint(text: &str) -> Vec<Diagnostic> {
    diagnose(text)
        .diagnostics
        .into_iter()
        .filter(|d| d.code == CODE)
        .collect()
}

#[test]
fn literal_test_warns_with_migrate_fixit() {
    let src = format!(
        "{HDR}<match on=\"run.rank\">\n\
         <when test=\"$ == 'gold'\">\n@x: a\n</when>\n\
         <otherwise>\n@x: b\n</otherwise>\n\
         </match>\n"
    );
    let ds = lint(&src);
    assert_eq!(ds.len(), 1, "{ds:?}");
    let d = &ds[0];
    assert_eq!(d.severity, Severity::Warning);
    assert_eq!(
        d.message,
        "`test=\"$ == 'gold'\"` compares the match subject to a literal; write `is=\"gold\"` \
         — the pattern form the checker can reason about; `lute fix` rewrites it (dsl 0.18.0 §3)"
    );
    assert_eq!(
        &src[d.span.byte_start..d.span.byte_end],
        "test=\"$ == 'gold'\"",
        "anchored at the whole test attribute"
    );
    assert_eq!(d.fixits.len(), 1);
    let fx = &d.fixits[0];
    assert_eq!(fx.kind, "migrate");
    assert_eq!(fx.edit.len(), 1);
    assert_eq!(
        &src[fx.edit[0].span.byte_start..fx.edit[0].span.byte_end],
        "test=\"$ == 'gold'\""
    );
    assert_eq!(fx.edit[0].new_text, "is=\"gold\"");
}

/// The fixit and `lute fix` read the same rewrite list, so they must agree
/// byte for byte; and the rewritten document no longer warns.
#[test]
fn fixit_and_lute_fix_agree_and_clear_the_warning() {
    let src = format!(
        "{HDR}<match on=\"run.rank\">\n\
         <when test=\"$ in ['silver', 'bronze']\">\n@x: a\n</when>\n\
         <when test=\"'gold' == $\">\n@x: b\n</when>\n\
         <otherwise>\n@x: c\n</otherwise>\n\
         </match>\n\
         <match on=\"run.n\">\n\
         <when test=\"$ >= 1 && $ <= 5\">\n@x: d\n</when>\n\
         <when test=\"$ <= 0\">\n@x: e\n</when>\n\
         <when test=\"$ >= 5.5\">\n@x: f\n</when>\n\
         <otherwise>\n@x: g\n</otherwise>\n\
         </match>\n"
    );
    let mut ds = lint(&src);
    assert_eq!(ds.len(), 5, "{ds:?}");
    ds.sort_by_key(|d| std::cmp::Reverse(d.span.byte_start));
    let mut via_lsp = src.clone();
    for d in &ds {
        let e = &d.fixits[0].edit[0];
        via_lsp.replace_range(e.span.byte_start..e.span.byte_end, &e.new_text);
    }
    let via_fix = fix_document(&src);
    assert_eq!(via_fix.text, via_lsp);
    assert_eq!(via_fix.changed, 5);
    for want in [
        "<when is=\"silver|bronze\">",
        "<when is=\"gold\">",
        "<when is=\"1..5\">",
        "<when is=\"..0\">",
        "<when is=\"5.5..\">",
    ] {
        assert!(
            via_fix.text.contains(want),
            "{want} missing:\n{}",
            via_fix.text
        );
    }
    assert!(lint(&via_fix.text).is_empty());
    assert_eq!(fix_document(&via_fix.text).changed, 0, "idempotent");
}

#[test]
fn non_literal_and_unsafe_tests_do_not_warn() {
    let src = format!(
        "{HDR}<match on=\"run.rank\">\n\
         <when test=\"$ != 'gold'\">\n@x: a\n</when>\n\
         <when test=\"$ == 'true'\">\n@x: b\n</when>\n\
         <when test=\"$ == null\">\n@x: c\n</when>\n\
         <otherwise>\n@x: d\n</otherwise>\n\
         </match>\n\
         <match on=\"run.n\">\n\
         <when test=\"$ > 2\">\n@x: e\n</when>\n\
         <when test=\"$ >= 3 && $ <= 1\">\n@x: f\n</when>\n\
         <otherwise>\n@x: g\n</otherwise>\n\
         </match>\n"
    );
    assert!(lint(&src).is_empty(), "{:?}", lint(&src));
    assert_eq!(fix_document(&src).changed, 0);
}

/// An arm that already has `is=` is the pattern form; its `test` is a guard
/// that refines it, never rewritten.
#[test]
fn arm_with_is_is_left_alone() {
    let src = format!(
        "{HDR}<match on=\"run.rank\">\n\
         <when is=\"gold\" test=\"$ == 'gold'\">\n@x: a\n</when>\n\
         <otherwise>\n@x: b\n</otherwise>\n\
         </match>\n"
    );
    assert!(lint(&src).is_empty());
    assert_eq!(fix_document(&src).text, src);
}

/// Only `<when>` arms of a `<match>`: a `$ == …` guard on a choice inside an
/// arm (where `$` is in scope) is not a match arm and never warns.
#[test]
fn choice_guard_inside_an_arm_does_not_warn() {
    let src = format!(
        "{HDR}<match on=\"run.rank\">\n\
         <when is=\"gold\">\n\
         <branch id=\"b\">\n\
         <choice id=\"c\" label=\"L\" when=\"$ == 'gold'\">\n@x: a\n</choice>\n\
         </branch>\n\
         </when>\n\
         <otherwise>\n@x: b\n</otherwise>\n\
         </match>\n"
    );
    assert!(lint(&src).is_empty(), "{:?}", lint(&src));
    assert_eq!(fix_document(&src).changed, 0);
}

/// `lute fix` rewrites ONLY the `test="…"` attribute, whatever sits around it
/// (unknown attrs are `E-UNKNOWN-ATTR`, not parse errors, so phase 2 runs).
#[test]
fn fix_rewrites_only_the_test_attribute() {
    let src = format!(
        "{HDR}<match on=\"run.rank\">\n\
         <when foo=\"1\" test=\"$ == 'gold'\" bar>\n@x: a\n</when>\n\
         <otherwise>\n@x: b\n</otherwise>\n\
         </match>\n"
    );
    let out = fix_document(&src);
    assert_eq!(out.changed, 1);
    assert_eq!(
        out.text,
        src.replace("test=\"$ == 'gold'\"", "is=\"gold\""),
        "got:\n{}",
        out.text
    );
    assert!(out.text.contains("<when foo=\"1\" is=\"gold\" bar>"));
}

#[test]
fn fix_reaches_arms_nested_in_branches_hubs_and_arms() {
    let src = format!(
        "{HDR}<branch id=\"b\">\n\
         <choice id=\"c\" label=\"L\">\n\
         <match on=\"run.rank\">\n\
         <when test=\"$ == 'gold'\">\n\
         <match on=\"run.n\">\n\
         <when test=\"$ == 2\">\n@x: a\n</when>\n\
         <otherwise>\n@x: b\n</otherwise>\n\
         </match>\n\
         </when>\n\
         <otherwise>\n@x: c\n</otherwise>\n\
         </match>\n\
         </choice>\n\
         </branch>\n\
         <hub id=\"h\">\n\
         <choice id=\"d\" label=\"M\" exit>\n\
         <match on=\"run.rank\">\n\
         <when test=\"$ in ['fail']\">\n@x: d\n</when>\n\
         <otherwise>\n@x: e\n</otherwise>\n\
         </match>\n\
         </choice>\n\
         </hub>\n"
    );
    assert_eq!(lint(&src).len(), 3, "{:?}", lint(&src));
    let out = fix_document(&src);
    assert_eq!(out.changed, 3, "got:\n{}", out.text);
    for want in [
        "<when is=\"gold\">",
        "<when is=\"2\">",
        "<when is=\"fail\">",
    ] {
        assert!(out.text.contains(want), "{want} missing:\n{}", out.text);
    }
    assert!(!out.text.contains("test="), "got:\n{}", out.text);
}

#[test]
fn fix_reaches_quest_bodies() {
    let src = "---\nkind: quest\nstate:\n  \
               run.rank: { type: { enum: [fail, gold] }, default: fail }\n---\n\
               <quest id=\"q\">\n<objective id=\"o\" done=\"true\"/>\n\
               <on event=\"questActive\">\n\
               <match on=\"run.rank\">\n\
               <when test=\"$ == 'gold'\">\n@x: hi\n</when>\n\
               <otherwise>\n@x: bye\n</otherwise>\n\
               </match>\n</on>\n</quest>\n";
    assert_eq!(lint(src).len(), 1, "{:?}", lint(src));
    let out = fix_document(src);
    assert_eq!(out.changed, 1);
    assert!(
        out.text.contains("<when is=\"gold\">"),
        "got:\n{}",
        out.text
    );
}
