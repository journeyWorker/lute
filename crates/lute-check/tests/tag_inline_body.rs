//! dsl §2.3: an element with children uses the BLOCK form — the children live
//! on their **own** lines. A single-line `<tag …>body</tag>` is not supported,
//! and the toolchain must say so: ONE `E-TAG-INLINE-BODY` naming the real
//! mistake, never the three-way misdirection it used to produce —
//! `E-UNCLOSED-TAG` (the close IS there, on the opener's line),
//! `E-UNCLASSIFIED` (the following arm is perfectly well-formed), and worst of
//! all `E-NONEXHAUSTIVE`, a false claim about the author's `<match>` whose arms
//! merely failed to parse.
use lute_check::{check, CheckInput, Mode, SchemaImports};
use lute_manifest::provider::ProviderSet;

fn codes(text: &str) -> Vec<String> {
    let input = CheckInput {
        text: text.to_string(),
        uri: "tag_inline_body".into(),
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

const HDR: &str = "---\nkind: scene\ncharacter: fixer\nseason: 1\nepisode: 1\nstate:\n  \
    run.mood: { type: { enum: [calm, tense] }, default: calm }\n---\n## Shot 1.\n";

/// The measured case: the author's real mistake is "the body shares the
/// opener's line", so that is the only thing they may be told.
#[test]
fn inline_arm_bodies_report_only_the_inline_body_mistake() {
    let out = codes(&format!(
        "{HDR}<match on=\"run.mood\">\n\
         <when is=\"calm\"> @fixer{{mono}}: Steady. </when>\n\
         <otherwise> @fixer{{mono}}: Not steady. </otherwise>\n\
         </match>\n"
    ));
    assert_eq!(
        out,
        vec!["E-TAG-INLINE-BODY", "E-TAG-INLINE-BODY"],
        "exactly one code, one occurrence per offending line: {out:?}"
    );
    assert!(
        !out.contains(&"E-NONEXHAUSTIVE".to_string()),
        "the `<match>` IS exhaustive — never a false claim about the author's logic: {out:?}"
    );
}

/// Regression guard: the SUPPORTED block form of the same content stays clean.
#[test]
fn block_form_arms_check_clean() {
    let out = codes(&format!(
        "{HDR}<match on=\"run.mood\">\n\
         <when is=\"calm\">\n@fixer{{mono}}: Steady.\n</when>\n\
         <otherwise>\n@fixer{{mono}}: Not steady.\n</otherwise>\n\
         </match>\n"
    ));
    assert!(out.is_empty(), "block form must check clean: {out:?}");
}

/// The wrapped-OPENER case is a different mistake with its own established
/// code (dsl 0.5.0 §2.1/§2.3) and must be untouched by the inline-body work.
#[test]
fn wrapped_opener_still_reports_tag_not_one_line() {
    let out = codes(&format!(
        "{HDR}<on event=\"x\"\nwhen=\"run.mood == 'calm'\">\n</on>\n"
    ));
    assert!(
        out.contains(&"E-TAG-NOT-ONE-LINE".to_string()),
        "a wrapped opener stays E-TAG-NOT-ONE-LINE: {out:?}"
    );
    assert!(
        !out.contains(&"E-TAG-INLINE-BODY".to_string()),
        "a wrapped opener has no inline body — the two codes must not collide: {out:?}"
    );
}

/// A sibling block element sharing the `parse_open_tag` detection point: every
/// `<tag …>` opener in the language is scanned there, so `<choice>` reports the
/// mistake exactly as `<when>`/`<otherwise>` do, with no cascade of its own.
#[test]
fn inline_body_on_sibling_element_reports_the_same_mistake() {
    let out = codes(&format!(
        "{HDR}<branch id=\"b\">\n\
         <choice id=\"c\" label=\"L\"> @fixer: hi. </choice>\n\
         </branch>\n"
    ));
    assert_eq!(
        out,
        vec!["E-TAG-INLINE-BODY"],
        "`<choice>` opens through the same code path as `<when>`: {out:?}"
    );
}

/// The block form of that same sibling stays clean (regression guard).
#[test]
fn block_form_sibling_checks_clean() {
    let out = codes(&format!(
        "{HDR}<branch id=\"b\">\n\
         <choice id=\"c\" label=\"L\">\n@fixer: hi.\n</choice>\n\
         </branch>\n"
    ));
    assert!(out.is_empty(), "block form must check clean: {out:?}");
}

/// The same suppression covers the sibling containers whose verdicts are read
/// off a child list: a `<branch>` collapsed onto one line HAS a `<choice>`, so
/// "empty branch" would be as false a claim as "non-exhaustive match".
#[test]
fn whole_branch_on_one_line_never_claims_an_empty_branch() {
    let out = codes(&format!(
        "{HDR}<branch id=\"b\"><choice id=\"c\" label=\"L\">@fixer: hi.</choice></branch>\n"
    ));
    assert_eq!(
        out,
        vec!["E-TAG-INLINE-BODY"],
        "the branch's `<choice>` is right there — never `E-BRANCH-EMPTY`: {out:?}"
    );
}

#[test]
fn whole_hub_on_one_line_never_claims_a_missing_exit() {
    let out = codes(&format!(
        "{HDR}<hub><choice id=\"c\" label=\"L\" exit>@fixer: hi.</choice></hub>\n"
    ));
    assert_eq!(
        out,
        vec!["E-TAG-INLINE-BODY"],
        "the hub's `exit` choice is right there — never `E-HUB-NO-EXIT`: {out:?}"
    );
}

/// When the inline form really does cost the `<match>` its arms, exhaustiveness
/// has no basis to judge and must stay silent rather than fabricate a verdict.
#[test]
fn whole_match_on_one_line_never_judges_exhaustiveness() {
    let out = codes(&format!(
        "{HDR}<match on=\"run.mood\"><when is=\"calm\"></when></match>\n"
    ));
    assert!(
        out.contains(&"E-TAG-INLINE-BODY".to_string()),
        "the inline form is still named: {out:?}"
    );
    assert!(
        !out.contains(&"E-NONEXHAUSTIVE".to_string()),
        "arms that did not parse are no basis for an exhaustiveness verdict: {out:?}"
    );
}
