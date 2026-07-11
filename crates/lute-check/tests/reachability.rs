//! B3 (0.4.0 T3): `E-WHEN-LITERAL-DOMAIN` — an `<when is="…">` literal outside
//! the subject's decided finite domain (dsl 0.4 §5.2, §6.3). Per-literal
//! domain membership inside the existing exhaustiveness engine
//! (`match_check.rs`'s `DomainInfo`/`infer_domain`) — driven through the
//! assembled `check()` over inline `state:` frontmatter, mirroring
//! `tests/hub.rs`'s `run()`/`codes()` harness.
use lute_check::{check, CheckInput, CheckResult, Mode, SchemaImports};
use lute_manifest::provider::ProviderSet;

fn run(text: &str) -> CheckResult {
    let input = CheckInput {
        text: text.to_string(),
        uri: "reachability".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
    };
    check(&input)
}

fn codes(text: &str) -> Vec<String> {
    run(text).diagnostics.into_iter().map(|d| d.code).collect()
}

// `run.rank` (enum, defaulted — never unset), `run.flag` (bool, defaulted),
// `run.n` (number, defaulted), `run.unbound` (number, NO default — maybe
// unset, dsl §9.4).
const HDR: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  \
    run.rank: { type: { enum: [fail, bronze, silver, gold] }, default: fail }\n  \
    run.flag: { type: bool, default: false }\n  \
    run.n: { type: number, default: 0 }\n  \
    run.unbound: { type: number }\n---\n## Shot 1.\n";

// (Appendix A) A foreign enum member — a typo (`platnum` against
// `[fail, bronze, silver, gold]`) — flags E-WHEN-LITERAL-DOMAIN.
#[test]
fn foreign_enum_member_is_literal_domain() {
    let out = codes(&format!(
        "{HDR}<match on=\"run.rank\">\n\
         <when is=\"platnum\">\n@narrator: x\n</when>\n\
         <otherwise>\n@narrator: o\n</otherwise>\n\
         </match>\n"
    ));
    assert!(
        out.contains(&"E-WHEN-LITERAL-DOMAIN".to_string()),
        "a foreign enum member typo must flag E-WHEN-LITERAL-DOMAIN: {out:?}"
    );
}

// The diagnostic's span slices the source to exactly the offending literal
// text, not the whole `is=` pattern.
#[test]
fn span_points_at_the_literal() {
    let text = format!(
        "{HDR}<match on=\"run.rank\">\n\
         <when is=\"platnum\">\n@narrator: x\n</when>\n\
         <otherwise>\n@narrator: o\n</otherwise>\n\
         </match>\n"
    );
    let result = run(&text);
    let diag = result
        .diagnostics
        .iter()
        .find(|d| d.code == "E-WHEN-LITERAL-DOMAIN")
        .unwrap_or_else(|| panic!("expected E-WHEN-LITERAL-DOMAIN: {:?}", result.diagnostics));
    assert_eq!(
        &text[diag.span.byte_start..diag.span.byte_end],
        "platnum",
        "span must bound exactly the literal, not the whole `is=` pattern"
    );
}

// `is="gold|platnum"`: only the foreign alternative (`platnum`) flags — the
// in-domain alternative (`gold`) contributes no diagnostic of its own, and
// (D4) the arm is never ALSO flagged E-ARM-DEAD.
#[test]
fn mixed_alternation_flags_only_foreign() {
    let out = codes(&format!(
        "{HDR}<match on=\"run.rank\">\n\
         <when is=\"gold|platnum\">\n@narrator: x\n</when>\n\
         <otherwise>\n@narrator: o\n</otherwise>\n\
         </match>\n"
    ));
    let flags = out
        .iter()
        .filter(|c| c.as_str() == "E-WHEN-LITERAL-DOMAIN")
        .count();
    assert_eq!(
        flags, 1,
        "exactly one alternative (`platnum`) is foreign: {out:?}"
    );
    assert!(
        !out.contains(&"E-ARM-DEAD".to_string()),
        "D4: this code owns the foreign-literal root, never piled with E-ARM-DEAD: {out:?}"
    );
}

// A bool literal against an enum domain is a domain-SHAPE mismatch (rule 2),
// not merely a missing member.
#[test]
fn bool_literal_against_enum_flags() {
    let out = codes(&format!(
        "{HDR}<match on=\"run.rank\">\n\
         <when is=\"true\">\n@narrator: x\n</when>\n\
         <otherwise>\n@narrator: o\n</otherwise>\n\
         </match>\n"
    ));
    assert!(
        out.contains(&"E-WHEN-LITERAL-DOMAIN".to_string()),
        "a bool literal against an enum domain is a shape mismatch: {out:?}"
    );
}

// `unset` on a subject that is never unset (a defaulted `bool` AND a
// defaulted `number` path) flags — rule 3, incl. rule 4's `Domain::Infinite`
// carve-out for the number subject (only the `unset` check applies there).
#[test]
fn unset_on_defaulted_path_flags() {
    let out = codes(&format!(
        "{HDR}<match on=\"run.flag\">\n\
         <when is=\"unset\">\n@narrator: a\n</when>\n\
         <otherwise>\n@narrator: b\n</otherwise>\n\
         </match>\n\
         <match on=\"run.n\">\n\
         <when is=\"unset\">\n@narrator: c\n</when>\n\
         <otherwise>\n@narrator: d\n</otherwise>\n\
         </match>\n"
    ));
    let flags = out
        .iter()
        .filter(|c| c.as_str() == "E-WHEN-LITERAL-DOMAIN")
        .count();
    assert_eq!(
        flags, 2,
        "`unset` on two never-unset (defaulted) subjects must flag twice: {out:?}"
    );
}

// `unset` on a genuinely maybe-unset subject (an un-defaulted `run.*` path)
// stays legal — no false positive.
#[test]
fn unset_on_maybe_unset_path_is_clean() {
    let out = codes(&format!(
        "{HDR}<match on=\"run.unbound\">\n\
         <when is=\"unset\">\n@narrator: x\n</when>\n\
         <otherwise>\n@narrator: o\n</otherwise>\n\
         </match>\n"
    ));
    assert!(
        !out.contains(&"E-WHEN-LITERAL-DOMAIN".to_string()),
        "`unset` on a maybe-unset (un-defaulted) path is legal: {out:?}"
    );
}

// An unresolved subject (an undeclared path) is silent here — it already
// gets its own E-UNDECLARED elsewhere, and this code never piles on with an
// unprovable domain claim.
#[test]
fn unresolved_subject_is_silent() {
    let out = codes(&format!(
        "{HDR}<match on=\"scene.nonsense.x\">\n\
         <when is=\"whatever\">\n@narrator: x\n</when>\n\
         <otherwise>\n@narrator: o\n</otherwise>\n\
         </match>\n"
    ));
    assert!(
        out.contains(&"E-UNDECLARED".to_string()),
        "sanity: `scene.nonsense.x` really is undeclared: {out:?}"
    );
    assert!(
        !out.contains(&"E-WHEN-LITERAL-DOMAIN".to_string()),
        "an unresolved (undeclared) subject must not flag: {out:?}"
    );
}

// Control (no false positives): a legitimate in-domain literal never flags.
#[test]
fn in_domain_literal_never_flags() {
    let out = codes(&format!(
        "{HDR}<match on=\"run.rank\">\n\
         <when is=\"gold\">\n@narrator: x\n</when>\n\
         <otherwise>\n@narrator: o\n</otherwise>\n\
         </match>\n"
    ));
    assert!(
        !out.contains(&"E-WHEN-LITERAL-DOMAIN".to_string()),
        "a legitimate in-domain literal must never flag: {out:?}"
    );
}

// ---------------------------------------------------------------------
// 0.4.0 T4: `E-ARM-DEAD` (dead guard + subsumption) + `W-OTHERWISE-DEAD`
// (dsl 0.4 §5.2). Cause 1 (dead guard) is driven by `decide.rs`'s
// `decide_slot`; cause 2 (subsumption) walks the arms' `is` patterns.
// ---------------------------------------------------------------------

// Cause 1: a literal-comparison guard that decides false (R1/R3) flags the
// arm, even with no `is` pattern at all.
#[test]
fn decided_false_test_is_arm_dead() {
    let out = codes(&format!(
        "{HDR}<match on=\"run.rank\">\n\
         <when test=\"1 > 2\">\n@narrator: x\n</when>\n\
         <otherwise>\n@narrator: o\n</otherwise>\n\
         </match>\n"
    ));
    assert!(
        out.contains(&"E-ARM-DEAD".to_string()),
        "a `test` that decides false must flag E-ARM-DEAD: {out:?}"
    );
}

// Cause 1 via R2: `$ == 'gone'` against a domain without `gone` decides
// false — the `$`-bound-to-subject-domain path (`ctx.dollar = Domain(&dom)`
// for match arms).
#[test]
fn foreign_dollar_eq_guard_is_arm_dead() {
    let out = codes(&format!(
        "{HDR}<match on=\"run.rank\">\n\
         <when test=\"$ == 'gone'\">\n@narrator: x\n</when>\n\
         <otherwise>\n@narrator: o\n</otherwise>\n\
         </match>\n"
    ));
    assert!(
        out.contains(&"E-ARM-DEAD".to_string()),
        "`$ == 'gone'` against a domain without `gone` decides false (R2): {out:?}"
    );
}

// A `test="@never"` guard hidden behind a frontmatter `defs:` entry whose
// body itself decides false is caught exactly like an inline literal guard
// — `DefTable` is built from `folded.def_bodies`/`folded.env.def_params`
// (D2 expand-then-decide).
#[test]
fn def_hidden_false_guard_is_arm_dead() {
    let text = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  \
                run.rank: { type: { enum: [fail, bronze, silver, gold] }, default: fail }\n\
                defs:\n  never: { type: bool, cel: \"1 > 2\" }\n---\n## Shot 1.\n\
                <match on=\"run.rank\">\n\
                <when test=\"@never\">\n@narrator: x\n</when>\n\
                <otherwise>\n@narrator: o\n</otherwise>\n\
                </match>\n";
    let out = codes(text);
    assert!(
        out.contains(&"E-ARM-DEAD".to_string()),
        "a `test=\"@never\"` def-hidden false guard must still flag E-ARM-DEAD: {out:?}"
    );
}

// The §5.4 worked example: `gold|silver` then `gold` subsumes (E-ARM-DEAD,
// citing arm 1); `platnum` is D4-rooted (E-WHEN-LITERAL-DOMAIN only, never
// piled with E-ARM-DEAD — its residual `is` set is fully foreign, so it's
// skipped by the dead-arm pass). Exactly those two codes fire, and C4 drops
// the dead arm's own `W-OVERLAP-ARMS`.
#[test]
fn spec_54_subsumption_example() {
    let text = format!(
        "{HDR}<match on=\"run.rank\">\n\
         <when is=\"gold | silver\">\n@narrator: a\n</when>\n\
         <when is=\"gold\">\n@narrator: b\n</when>\n\
         <when is=\"platnum\">\n@narrator: c\n</when>\n\
         <otherwise>\n@narrator: d\n</otherwise>\n\
         </match>\n"
    );
    let result = run(&text);
    let codes: Vec<&str> = result.diagnostics.iter().map(|d| d.code.as_str()).collect();
    assert_eq!(
        codes.iter().filter(|c| **c == "E-ARM-DEAD").count(),
        1,
        "exactly one E-ARM-DEAD (the subsumed `gold` arm): {codes:?}"
    );
    assert_eq!(
        codes.iter().filter(|c| **c == "E-WHEN-LITERAL-DOMAIN").count(),
        1,
        "exactly one E-WHEN-LITERAL-DOMAIN (`platnum`): {codes:?}"
    );
    assert!(
        !codes.contains(&"W-OVERLAP-ARMS"),
        "C4: the dead arm's own W-OVERLAP-ARMS must be suppressed: {codes:?}"
    );
    let dead = result
        .diagnostics
        .iter()
        .find(|d| d.code == "E-ARM-DEAD")
        .expect("E-ARM-DEAD present (checked above)");
    assert!(
        dead.message.contains("gold | silver") && dead.message.contains("first-match-wins"),
        "message must cite the covering arm's pattern (§5.4 message shape): {}",
        dead.message
    );
}

// A guard cannot resurrect a subsumed pattern, but the inverse also holds: a
// GUARDED earlier arm never counts toward the subsumption union U (its guard
// might be false at runtime) — the later identical `is="gold"` stays a plain
// (pre-existing) W-OVERLAP-ARMS, never elevated to E-ARM-DEAD.
#[test]
fn guarded_earlier_arm_never_subsumes() {
    let out = codes(&format!(
        "{HDR}<match on=\"run.rank\">\n\
         <when is=\"gold\" test=\"run.flag\">\n@narrator: a\n</when>\n\
         <when is=\"gold\">\n@narrator: b\n</when>\n\
         <otherwise>\n@narrator: c\n</otherwise>\n\
         </match>\n"
    ));
    assert!(
        !out.contains(&"E-ARM-DEAD".to_string()),
        "a guarded earlier arm's `is` set never counts toward subsumption: {out:?}"
    );
    assert!(
        out.contains(&"W-OVERLAP-ARMS".to_string()),
        "the pre-existing literal-overlap warning is unaffected by this task: {out:?}"
    );
}

// A dead `<choice when>` in both a `<branch>` and a `<hub>` flags E-ARM-DEAD
// (dollar: None — no `$` in scope at a choice guard); the pre-existing,
// purely-structural E-BRANCH-ALL-GUARDED / E-HUB-NO-EXIT are unrelaxed.
#[test]
fn dead_choice_when_in_branch_and_hub() {
    let branch_out = codes(&format!(
        "{HDR}<branch id=\"approach\">\n\
         <choice id=\"soft\" label=\"Soft\" when=\"1 > 2\">\n@narrator: a\n</choice>\n\
         </branch>\n"
    ));
    assert!(
        branch_out.contains(&"E-ARM-DEAD".to_string()),
        "a `<choice when>` that decides false must flag E-ARM-DEAD: {branch_out:?}"
    );
    assert!(
        branch_out.contains(&"E-BRANCH-ALL-GUARDED".to_string()),
        "E-ARM-DEAD must not relax the purely-structural E-BRANCH-ALL-GUARDED: {branch_out:?}"
    );

    let hub_out = codes(&format!(
        "{HDR}<hub id=\"h\">\n\
         <choice id=\"soft\" label=\"Soft\" when=\"1 > 2\">\n@narrator: a\n</choice>\n\
         </hub>\n"
    ));
    assert!(
        hub_out.contains(&"E-ARM-DEAD".to_string()),
        "a `<choice when>` in a hub that decides false must flag E-ARM-DEAD: {hub_out:?}"
    );
    assert!(
        hub_out.contains(&"E-HUB-NO-EXIT".to_string()),
        "E-ARM-DEAD must not relax the purely-structural E-HUB-NO-EXIT: {hub_out:?}"
    );
}

// `W-OTHERWISE-DEAD`: `is="true"` + `is="false"` on a DEFAULTED (never
// unset) bool subject cover the whole finite domain, so the trailing
// `<otherwise>` is provably dead — a warning (`res.ok` stays true).
#[test]
fn otherwise_dead_on_covered_bool() {
    let text = format!(
        "{HDR}<match on=\"run.flag\">\n\
         <when is=\"true\">\n@narrator: a\n</when>\n\
         <when is=\"false\">\n@narrator: b\n</when>\n\
         <otherwise>\n@narrator: c\n</otherwise>\n\
         </match>\n"
    );
    let result = run(&text);
    let out: Vec<String> = result.diagnostics.iter().map(|d| d.code.clone()).collect();
    assert!(
        out.contains(&"W-OTHERWISE-DEAD".to_string()),
        "a covered defaulted-bool domain makes the `<otherwise>` provably dead: {out:?}"
    );
    assert!(result.ok, "a warning must not flip `ok` false: {out:?}");
}

// Same arms, but the subject is genuinely maybe-unset (no `default`): `unset`
// stays uncovered, so the `<otherwise>` is still live — no false positive.
#[test]
fn otherwise_live_when_maybe_unset() {
    let text = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  \
                run.flag2: { type: bool }\n---\n## Shot 1.\n\
                <match on=\"run.flag2\">\n\
                <when is=\"true\">\n@narrator: a\n</when>\n\
                <when is=\"false\">\n@narrator: b\n</when>\n\
                <otherwise>\n@narrator: c\n</otherwise>\n\
                </match>\n";
    let out = codes(text);
    assert!(
        !out.contains(&"W-OTHERWISE-DEAD".to_string()),
        "an un-defaulted (maybe-unset) bool subject leaves `unset` uncovered — the \
         `<otherwise>` stays live: {out:?}"
    );
}

// The provable-only boundary (dsl 0.4 §5.1): a guard reading runtime state
// (`run.n > 1`, R5 — undecided) is NEVER flagged, no matter how it "looks"
// suspicious. `decide_slot` returning `None` must never be treated as false.
#[test]
fn undecided_guard_is_never_flagged() {
    let out = codes(&format!(
        "{HDR}<match on=\"run.rank\">\n\
         <when test=\"run.n > 1\">\n@narrator: x\n</when>\n\
         <otherwise>\n@narrator: o\n</otherwise>\n\
         </match>\n"
    ));
    assert!(
        !out.contains(&"E-ARM-DEAD".to_string()),
        "an undecided guard (a state-path read) must never be flagged — the provable-only \
         boundary (dsl 0.4 §5.1): {out:?}"
    );
}
