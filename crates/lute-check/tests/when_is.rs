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
        defaults: Default::default(),
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

/// 0.21.1 T1-12 (lamplight F26): `is=` + `test=` is "pattern AND guard", so
/// an arm whose guard the checker cannot decide covers NOTHING. It used to
/// cover its whole `is` set (a union), which made the later `is="a"`
/// fallback "unreachable" (`W-OVERLAP-ARMS`) although trace/play run it
/// whenever the guard is false.
#[test]
fn undecidable_guard_on_is_arm_does_not_shadow_a_later_same_pattern_arm() {
    let hdr = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  \
        run.who: { type: { enum: [a, b] }, default: a }\n  \
        run.proof: { type: bool, default: false }\n---\n## Shot 1.\n";
    let out = codes(&format!(
        "{hdr}<match on=\"run.who\">\n\
         <when is=\"a\" test=\"run.proof\">\n@narrator: proof.\n</when>\n\
         <when is=\"a\">\n@narrator: fallback.\n</when>\n\
         <otherwise>\n@narrator: other.\n</otherwise>\n\
         </match>\n"
    ));
    assert!(
        !out.contains(&"W-OVERLAP-ARMS".to_string()),
        "a guarded arm never makes a later same-pattern arm unreachable: {out:?}"
    );
}

/// The missed-`E-NONEXHAUSTIVE` half of T1-12: with no `<otherwise>`, the
/// guarded `is="a"` arm does not cover `a` (the guard may be false), so the
/// match has no arm for `run.who == a && !run.proof` — the runtime's
/// `-> no arm` — and must be rejected.
#[test]
fn undecidable_guard_on_is_arm_leaves_the_member_uncovered() {
    let hdr = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  \
        run.who: { type: { enum: [a, b, nobody] }, default: a }\n  \
        run.proof: { type: bool, default: false }\n---\n## Shot 1.\n";
    let out = codes(&format!(
        "{hdr}<match on=\"run.who\">\n\
         <when is=\"a\" test=\"run.proof\">\n@narrator: proof.\n</when>\n\
         <when is=\"b | nobody\">\n@narrator: other.\n</when>\n\
         </match>\n"
    ));
    assert!(
        out.contains(&"E-NONEXHAUSTIVE".to_string()),
        "`a` with a false guard falls through every arm: {out:?}"
    );
}
