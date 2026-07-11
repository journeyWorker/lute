//! B4: `<when is="…">` literal-pattern coverage + `E-WHEN-PATTERN` end-to-end
//! (dsl §7.3.1, §11.2), driven through the assembled `check()` over inline
//! `state:` frontmatter (mirrors `ref_type.rs`/`interp.rs`'s harness). `is` is
//! the NORMATIVE exhaustiveness path: the literal arms must cover the subject's
//! domain with no `<otherwise>`, else `E-NONEXHAUSTIVE`.
use lute_check::{check, CheckInput, Mode, SchemaImports};
use lute_manifest::provider::ProviderSet;

fn codes(text: &str) -> Vec<String> {
    let input = CheckInput {
        text: text.to_string(),
        uri: "when_is".into(),
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

const HDR: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  \
    run.rank: { type: { enum: [fail, bronze, silver, gold] }, default: fail }\n---\n## Shot 1.\n";

#[test]
fn is_arms_cover_enum_no_otherwise_is_exhaustive() {
    // `is` arms cover the full enum with NO <otherwise> => no E-NONEXHAUSTIVE.
    let out = codes(&format!(
        "{HDR}<match on=\"run.rank\">\n\
         <when is=\"fail | bronze\">\n@narrator: a\n</when>\n\
         <when is=\"silver\">\n@narrator: b\n</when>\n\
         <when is=\"gold\">\n@narrator: c\n</when>\n\
         </match>\n"
    ));
    assert!(
        !out.contains(&"E-NONEXHAUSTIVE".to_string()),
        "is arms fully cover the enum: {out:?}"
    );
}

#[test]
fn is_arms_missing_member_is_nonexhaustive() {
    // omit `gold` => E-NONEXHAUSTIVE (is-derived coverage is normative, §11.2).
    let out = codes(&format!(
        "{HDR}<match on=\"run.rank\">\n\
         <when is=\"fail | bronze\">\n@narrator: a\n</when>\n\
         <when is=\"silver\">\n@narrator: b\n</when>\n\
         </match>\n"
    ));
    assert!(
        out.contains(&"E-NONEXHAUSTIVE".to_string()),
        "missing `gold` arm: {out:?}"
    );
}

#[test]
fn when_with_neither_is_nor_test_is_e_when_pattern() {
    // a `<when>` with neither `is` nor `test` => E-WHEN-PATTERN (§7.3.1, D-D).
    let out = codes(&format!(
        "{HDR}<match on=\"run.rank\">\n\
         <when>\n@narrator: a\n</when>\n\
         <otherwise>\n@narrator: b\n</otherwise>\n\
         </match>\n"
    ));
    assert!(
        out.contains(&"E-WHEN-PATTERN".to_string()),
        "empty <when> must be E-WHEN-PATTERN: {out:?}"
    );
}

#[test]
fn is_and_test_arm_parses_and_is_drives_coverage() {
    // `is="gold" test="$ != 'x'"` composes P.when; `is` still drives coverage, so
    // the enum is exhaustive with no <otherwise> and no E-WHEN-PATTERN.
    let out = codes(&format!(
        "{HDR}<match on=\"run.rank\">\n\
         <when is=\"fail | bronze\">\n@narrator: a\n</when>\n\
         <when is=\"silver\">\n@narrator: b\n</when>\n\
         <when is=\"gold\" test=\"$ != 'x'\">\n@narrator: c\n</when>\n\
         </match>\n"
    ));
    assert!(
        !out.contains(&"E-NONEXHAUSTIVE".to_string()),
        "is drives coverage even with a guard: {out:?}"
    );
    assert!(
        !out.contains(&"E-WHEN-PATTERN".to_string()),
        "an arm carrying `is` is never E-WHEN-PATTERN: {out:?}"
    );
}
