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

// Finding 2 (D4 double-report): an arm carrying BOTH a foreign `is` literal
// AND a decided-false `test=` guard must flag ONLY E-WHEN-LITERAL-DOMAIN —
// the foreign-literal code owns the root; cause 1 (dead-guard) must never
// ALSO report E-ARM-DEAD on the same arm.
#[test]
fn foreign_literal_with_decided_false_guard_is_not_also_arm_dead() {
    let out = codes(&format!(
        "{HDR}<match on=\"run.rank\">\n\
         <when is=\"platnum\" test=\"1 > 2\">\n@narrator: x\n</when>\n\
         <otherwise>\n@narrator: o\n</otherwise>\n\
         </match>\n"
    ));
    assert!(
        out.contains(&"E-WHEN-LITERAL-DOMAIN".to_string()),
        "the foreign literal must still flag: {out:?}"
    );
    assert!(
        !out.contains(&"E-ARM-DEAD".to_string()),
        "D4: a foreign-literal arm must NEVER also be E-ARM-DEAD, even with \
         its own decided-false guard: {out:?}"
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

// ---------------------------------------------------------------------
// 0.4.0 T5: quest reachability (dsl 0.4 §5.3) — `E-QUEST-UNREACHABLE`
// (dead `start` / true `fail`, D21), `E-OBJECTIVE-UNSATISFIABLE` (dead
// `done`, + the required-quest note, C4), `W-OBJECTIVE-HIDDEN` (dead
// `when` on a required objective). Quest guards decide with `dollar: None`
// (no `$` in scope outside a `<match>` subject).
// ---------------------------------------------------------------------

// `run.rank` (enum, defaulted), `run.flag` (bool, defaulted) — mirrors HDR's
// schema shape but under `kind: quest` (no scene triad, no `## Shot`).
const QUEST_HDR: &str = "---\nkind: quest\nstate:\n  \
    run.rank: { type: { enum: [fail, bronze, silver, gold] }, default: fail }\n  \
    run.flag: { type: bool, default: false }\n---\n";

// Cause 1 of `E-QUEST-UNREACHABLE`: a literal-comparison `start` that
// decides false (R1/R3) — the quest never activates. The message names
// `start`.
#[test]
fn dead_start_is_quest_unreachable() {
    let text = format!(
        "{QUEST_HDR}<quest id=\"q\" start=\"1 > 2\">\n\
         <objective id=\"o\" done=\"true\"/>\n</quest>\n"
    );
    let res = run(&text);
    let hit = res
        .diagnostics
        .iter()
        .find(|d| d.code == "E-QUEST-UNREACHABLE")
        .unwrap_or_else(|| {
            panic!("dead start must flag E-QUEST-UNREACHABLE: {:?}", res.diagnostics)
        });
    assert!(hit.message.contains("start"), "message must name `start`: {}", hit.message);
}

// Cause 2: `fail` deciding TRUE (fail precedes completion, dsl 0.2 §6.3) is
// the SAME code, a distinct cause — the message names `fail`.
#[test]
fn true_fail_is_quest_unreachable() {
    let text = format!(
        "{QUEST_HDR}<quest id=\"q\" fail=\"2 > 1\">\n\
         <objective id=\"o\" done=\"true\"/>\n</quest>\n"
    );
    let res = run(&text);
    let hit = res
        .diagnostics
        .iter()
        .find(|d| d.code == "E-QUEST-UNREACHABLE")
        .unwrap_or_else(|| {
            panic!("true fail must flag E-QUEST-UNREACHABLE: {:?}", res.diagnostics)
        });
    assert!(hit.message.contains("fail"), "message must name `fail`: {}", hit.message);
}

// A `done` deciding false (here via R2: `platnum` is foreign to
// `run.rank`'s domain, the §5.4 worked example's own objective) is
// `E-OBJECTIVE-UNSATISFIABLE`, NEVER `E-QUEST-UNREACHABLE` (C4) — the
// required-quest consequence rides as a note on THIS diagnostic's message
// instead.
#[test]
fn foreign_done_is_objective_unsat() {
    let text = format!(
        "{QUEST_HDR}<quest id=\"q\">\n\
         <objective id=\"o\" done=\"run.rank == 'platnum'\"/>\n</quest>\n"
    );
    let res = run(&text);
    assert!(
        !res.diagnostics.iter().any(|d| d.code == "E-QUEST-UNREACHABLE"),
        "an objective-only defect must never pile on E-QUEST-UNREACHABLE (C4): {:?}",
        res.diagnostics
    );
    let hit = res
        .diagnostics
        .iter()
        .find(|d| d.code == "E-OBJECTIVE-UNSATISFIABLE")
        .unwrap_or_else(|| {
            panic!("dead done must flag E-OBJECTIVE-UNSATISFIABLE: {:?}", res.diagnostics)
        });
    assert!(
        hit.message.contains("run.rank == 'platnum'"),
        "message must quote the `done` expression: {}",
        hit.message
    );
    assert!(
        hit.message.contains("being required, the quest"),
        "a required objective's message must append the quest consequence note: {}",
        hit.message
    );
}

// The inverse: an `optional` objective's dead `done` still fires the code
// (it too can never complete) but WITHOUT the quest-consequence note — an
// optional objective never makes the quest unreachable.
#[test]
fn optional_dead_done_has_no_quest_note() {
    let text = format!(
        "{QUEST_HDR}<quest id=\"q\">\n\
         <objective id=\"o\" done=\"run.rank == 'platnum'\" optional/>\n</quest>\n"
    );
    let res = run(&text);
    let hit = res
        .diagnostics
        .iter()
        .find(|d| d.code == "E-OBJECTIVE-UNSATISFIABLE")
        .unwrap_or_else(|| {
            panic!("an optional dead done must still flag the code: {:?}", res.diagnostics)
        });
    assert!(
        hit.message.contains("run.rank == 'platnum'"),
        "message must quote the `done` expression even for optional objectives: {}",
        hit.message
    );
    assert!(
        !hit.message.contains("being required"),
        "an optional objective must carry no quest-consequence note: {}",
        hit.message
    );
}

// The §5.4 quest worked example verbatim: a dead `start` (`1 > 2`) AND a
// foreign `done` (`platnum`) are DISTINCT roots — both codes fire, each
// exactly once (dsl 0.4 §5.4's parenthetical).
#[test]
fn spec_54_quest_example_two_roots() {
    let text = format!(
        "{QUEST_HDR}<quest id=\"ghostHunt\" title=\"A quest nobody gets\" start=\"1 > 2\">\n\
         <objective id=\"catch\" done=\"run.rank == 'platnum'\"/>\n</quest>\n"
    );
    let out = codes(&text);
    assert_eq!(
        out.iter().filter(|c| c.as_str() == "E-QUEST-UNREACHABLE").count(),
        1,
        "{out:?}"
    );
    assert_eq!(
        out.iter().filter(|c| c.as_str() == "E-OBJECTIVE-UNSATISFIABLE").count(),
        1,
        "{out:?}"
    );
}

// `W-OBJECTIVE-HIDDEN`: a required objective's `when` decides false — the
// objective is provably never visible, yet still gates completion (dsl 0.2
// §6.3). A WARNING: `done` is evaluated independently of visibility, so
// `res.ok` stays true.
#[test]
fn hidden_required_objective_warns() {
    let text = format!(
        "{QUEST_HDR}<quest id=\"q\">\n\
         <objective id=\"o\" when=\"false\" done=\"run.flag\"/>\n</quest>\n"
    );
    let res = run(&text);
    assert!(res.ok, "a warning-only doc must stay res.ok: {:?}", res.diagnostics);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "W-OBJECTIVE-HIDDEN"),
        "{:?}",
        res.diagnostics
    );
}

// The PROVABLE-ONLY boundary at the quest level: `holds(...)` is a fact
// query (R5, always undecided) — a quest whose `start` gates on one stays
// clean, exactly the quest-grove/rescue-halsin shape (dsl 0.4 §5.1).
#[test]
fn holds_guards_stay_undecided() {
    let vocab = "entities:\n  c: { members: [ana] }\nrelations:\n  inParty: { args: [c] }\n";
    let text = format!(
        "---\nkind: quest\n{vocab}---\n<quest id=\"q\" start=\"holds(inParty(ana))\">\n\
         <objective id=\"o\" done=\"true\"/>\n</quest>\n"
    );
    let out = codes(&text);
    assert!(
        !out.iter().any(|c| c == "E-QUEST-UNREACHABLE"
            || c == "E-OBJECTIVE-UNSATISFIABLE"
            || c == "W-OBJECTIVE-HIDDEN"),
        "an R5 fact-query guard must stay undecided, never flagged: {out:?}"
    );
}

// -- Finding 3: standalone component-file reachability param domains ------
//
// A component file checked STANDALONE (no `::use`/`resolve_components`)
// routes `<match on="@param">` through `check_param_match` (T7/T8
// exhaustiveness), but the SEPARATE reachability pass (`check_reachability`,
// this module) previously built `$`'s domain from schema paths + an EMPTY
// param map — so a bare-`@param` subject's domain was never seeded on the
// standalone path (only the TRANSITIVE `::use` import path, via
// `walk_component_body`'s own reachability call, diagnosed these).

#[test]
fn standalone_component_dollar_eq_guard_foreign_to_param_is_arm_dead() {
    let cs = codes(
        "---\ncomponent: reaction\nparams:\n  tier: { enum: [cold, warm, fond] }\n---\n\
         ## Scene 1.\n\
         <match on=\"@tier\">\n\
         <when test=\"$ == 'gone'\">\n@narrator: x\n</when>\n\
         <otherwise>\n@narrator: o\n</otherwise>\n\
         </match>\n",
    );
    assert!(
        cs.contains(&"E-ARM-DEAD".to_string()),
        "a $ comparison foreign to the dispatched param's domain must decide \
         false in a STANDALONE component self-check: {cs:?}"
    );
}

#[test]
fn standalone_component_covered_param_otherwise_is_dead() {
    let cs = codes(
        "---\ncomponent: reaction\nparams:\n  tier: { enum: [cold, warm, fond] }\n---\n\
         ## Scene 1.\n\
         <match on=\"@tier\">\n\
         <when is=\"cold|warm|fond\">\n@narrator: x\n</when>\n\
         <otherwise>\n@narrator: o\n</otherwise>\n\
         </match>\n",
    );
    assert!(
        cs.contains(&"W-OTHERWISE-DEAD".to_string()),
        "an <otherwise> after a fully-covered param domain must flag \
         W-OTHERWISE-DEAD in a STANDALONE component self-check: {cs:?}"
    );
}

#[test]
fn scene_reachability_not_polluted_by_param_seeding() {
    // An ordinary SCENE doc must stay unaffected: `ctx.params` stays empty,
    // so a bare `@ghost`-shaped subject never resolves through it (guarded
    // to `MetaKind::Component` only, mirroring `fragment_kind.rs`'s
    // `scene_doc_not_polluted_by_component_param_seeding`).
    let out = codes(&format!(
        "{HDR}<match on=\"run.rank\">\n\
         <when test=\"$ == 'gone'\">\n@narrator: x\n</when>\n\
         <otherwise>\n@narrator: o\n</otherwise>\n\
         </match>\n"
    ));
    assert!(
        out.contains(&"E-ARM-DEAD".to_string()),
        "the ordinary state-path domain path must still decide: {out:?}"
    );
}

// ---------------------------------------------------------------------
// dsl 0.5.2: `E-UNSET-LITERAL` — a CEL guard slot comparing a maybe-unset
// finite-domain subject to the FOREIGN string `'unset'` (`S ==/!= 'unset'`,
// either operand order, possibly nested). An INDEPENDENT AST lint (fires for
// `==` AND `!=`, regardless of `decide()`'s outcome) that OWNS (suppresses)
// the derivative `E-ARM-DEAD` it would otherwise cause (§2.3, mirrors D4).
// `E-MAYBE-UNSET` is NOT a derivative and keeps firing.
// ---------------------------------------------------------------------

// The base case (spec §1's motivating example, `<choice>` form): a foreign
// quest's reserved `quest.<id>.state` (finite [active, complete, failed],
// always maybe-unset) compared to the string `'unset'` decides false (R2) —
// `E-UNSET-LITERAL` fires and OWNS the would-be `E-ARM-DEAD`; the raw read
// still independently flags `E-MAYBE-UNSET` (§4, unaffected by this lint).
#[test]
fn unset_sentinel_choice_flags_literal_owns_arm_dead_keeps_maybe_unset() {
    let out = codes(
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n\
         <branch id=\"b\">\n\
         <choice id=\"c\" label=\"C\" when=\"quest.foo.state == 'unset'\">\n@x: a\n</choice>\n\
         </branch>\n",
    );
    assert!(
        out.contains(&"E-UNSET-LITERAL".to_string()),
        "S == 'unset' against a maybe-unset finite domain must flag E-UNSET-LITERAL: {out:?}"
    );
    assert!(
        !out.contains(&"E-ARM-DEAD".to_string()),
        "E-UNSET-LITERAL owns the dead-arm root (§2.3, D4-style): {out:?}"
    );
    assert!(
        out.contains(&"E-MAYBE-UNSET".to_string()),
        "the raw quest.foo.state read is still an independent E-MAYBE-UNSET (§4): {out:?}"
    );
}

// `!= 'unset'` decides TRUE (R2) and therefore never reaches the dead-arm
// path at all — yet it is the identical sentinel mistake, so the lint MUST
// still fire (§2.1's final paragraph: this is why it can't be a swap inside
// `if decided == false`).
#[test]
fn unset_sentinel_not_equals_still_flags_literal() {
    let out = codes(
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n\
         <branch id=\"b\">\n\
         <choice id=\"c\" label=\"C\" when=\"quest.foo.state != 'unset'\">\n@x: a\n</choice>\n\
         </branch>\n",
    );
    assert!(
        out.contains(&"E-UNSET-LITERAL".to_string()),
        "S != 'unset' is the identical sentinel mistake and must flag E-UNSET-LITERAL: {out:?}"
    );
}

// Both reversed-operand forms (`'unset' == S`, `'unset' != S`) trigger the
// lint exactly like the canonical `S ==/!= 'unset'` order (§2.1).
#[test]
fn unset_sentinel_reversed_operands_flag_literal() {
    let eq = codes(
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n\
         <branch id=\"b\">\n\
         <choice id=\"c\" label=\"C\" when=\"'unset' == quest.foo.state\">\n@x: a\n</choice>\n\
         </branch>\n",
    );
    assert!(
        eq.contains(&"E-UNSET-LITERAL".to_string()),
        "'unset' == S must flag E-UNSET-LITERAL: {eq:?}"
    );
    let ne = codes(
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n\
         <branch id=\"b\">\n\
         <choice id=\"c\" label=\"C\" when=\"'unset' != quest.foo.state\">\n@x: a\n</choice>\n\
         </branch>\n",
    );
    assert!(
        ne.contains(&"E-UNSET-LITERAL".to_string()),
        "'unset' != S must flag E-UNSET-LITERAL: {ne:?}"
    );
}

// Nested inside a larger boolean expression (§2.1: "the lint scans every
// comparison sub-expression, not only a top-level guard").
#[test]
fn unset_sentinel_nested_in_boolean_expr_flags_literal() {
    let out = codes(&format!(
        "{HDR}<branch id=\"b\">\n\
         <choice id=\"c\" label=\"C\" when=\"run.flag && quest.foo.state == 'unset'\">\n@x: a\n</choice>\n\
         </branch>\n"
    ));
    assert!(
        out.contains(&"E-UNSET-LITERAL".to_string()),
        "a sentinel comparison nested under && must still flag E-UNSET-LITERAL: {out:?}"
    );
}

// A `<match on><when test>` arm form (not just `<choice when>`): `$` bound to
// the match subject's own domain resolves through the SAME `resolve_domain`
// R2 uses, so the lint fires there too and owns that arm's `E-ARM-DEAD`.
#[test]
fn unset_sentinel_match_arm_dollar_flags_literal_owns_arm_dead() {
    let out = codes(
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n\
         <match on=\"quest.foo.state\">\n\
         <when test=\"$ == 'unset'\">\n@x: a\n</when>\n\
         <otherwise>\n@x: o\n</otherwise>\n\
         </match>\n",
    );
    assert!(
        out.contains(&"E-UNSET-LITERAL".to_string()),
        "$ == 'unset' against the match subject's own domain must flag E-UNSET-LITERAL: {out:?}"
    );
    assert!(
        !out.contains(&"E-ARM-DEAD".to_string()),
        "E-UNSET-LITERAL owns the dead-arm root here too: {out:?}"
    );
}

// -- Controls: prove no over-suppression ----------------------------------

// An UNDEFAULTED enum compared to a foreign literal that is NOT the string
// `'unset'` must behave exactly as before this revision: still E-ARM-DEAD,
// never E-UNSET-LITERAL (the detector only ever matches the literal string
// `'unset'`, dsl 0.5.2 §2.1 condition 3).
#[test]
fn control_undefaulted_foreign_enum_not_unset_stays_arm_dead() {
    let hdr = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  \
               run.grade: { type: { enum: [bronze, silver, gold] } }\n---\n## Shot 1.\n";
    let out = codes(&format!(
        "{hdr}<branch id=\"b\">\n\
         <choice id=\"c\" label=\"C\" when=\"run.grade == 'legendary'\">\n@x: a\n</choice>\n\
         </branch>\n"
    ));
    assert!(
        out.contains(&"E-ARM-DEAD".to_string()),
        "a foreign (non-'unset') literal against an undefaulted enum must still flag \
         E-ARM-DEAD, unaffected by this revision: {out:?}"
    );
    assert!(
        !out.contains(&"E-UNSET-LITERAL".to_string()),
        "a foreign literal that is not the string 'unset' must never flag E-UNSET-LITERAL: {out:?}"
    );
}

// A DEFAULTED enum (never maybe-unset) compared to an ordinary foreign typo
// also stays E-ARM-DEAD-only — this revision only ever touches the literal
// string `'unset'`.
#[test]
fn control_defaulted_enum_foreign_typo_stays_arm_dead() {
    let out = codes(&format!(
        "{HDR}<branch id=\"b\">\n\
         <choice id=\"c\" label=\"C\" when=\"run.rank == 'legendary'\">\n@x: a\n</choice>\n\
         </branch>\n"
    ));
    assert!(
        out.contains(&"E-ARM-DEAD".to_string()),
        "a defaulted-enum foreign typo must still flag E-ARM-DEAD: {out:?}"
    );
    assert!(
        !out.contains(&"E-UNSET-LITERAL".to_string()),
        "{out:?}"
    );
}

// An `<objective done>` foreign (non-'unset') literal is untouched by this
// revision: still `E-OBJECTIVE-UNSATISFIABLE`, never `E-UNSET-LITERAL`.
#[test]
fn control_objective_foreign_literal_stays_unsatisfiable() {
    let text = format!(
        "{QUEST_HDR}<quest id=\"q\">\n\
         <objective id=\"o\" done=\"run.rank == 'legendary'\"/>\n</quest>\n"
    );
    let out = codes(&text);
    assert!(
        out.contains(&"E-OBJECTIVE-UNSATISFIABLE".to_string()),
        "a foreign (non-'unset') objective done literal must still flag \
         E-OBJECTIVE-UNSATISFIABLE: {out:?}"
    );
    assert!(
        !out.contains(&"E-UNSET-LITERAL".to_string()),
        "{out:?}"
    );
}

// A LEGITIMATE enum member literally named `unset` (in-domain) is a normal
// comparison — R2 leaves it undecided (a domain member's actual value is
// still unknown), so no E-ARM-DEAD, and the literal is not FOREIGN so no
// E-UNSET-LITERAL either (dsl 0.5.2 §2.1 condition 3 / §3 non-goal).
#[test]
fn control_legit_unset_enum_member_is_normal_comparison() {
    let hdr = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  \
               run.phase: { type: { enum: [unset, active] } }\n---\n## Shot 1.\n";
    let out = codes(&format!(
        "{hdr}<branch id=\"b\">\n\
         <choice id=\"c\" label=\"C\" when=\"run.phase == 'unset'\">\n@x: a\n</choice>\n\
         </branch>\n"
    ));
    assert!(
        !out.contains(&"E-UNSET-LITERAL".to_string()),
        "an in-domain enum member literally named `unset` is not foreign: {out:?}"
    );
    assert!(
        !out.contains(&"E-ARM-DEAD".to_string()),
        "R2 leaves an in-domain literal comparison undecided: {out:?}"
    );
}

// `quest.<id>.state == 'active'` (an in-domain member) is unchanged by this
// revision: no E-ARM-DEAD (R2 undecided), no E-UNSET-LITERAL, and the raw
// read still independently flags E-MAYBE-UNSET (§4).
#[test]
fn control_quest_state_in_domain_comparison_unchanged() {
    let out = codes(
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n\
         <branch id=\"b\">\n\
         <choice id=\"c\" label=\"C\" when=\"quest.foo.state == 'active'\">\n@x: a\n</choice>\n\
         </branch>\n",
    );
    assert!(!out.contains(&"E-ARM-DEAD".to_string()), "{out:?}");
    assert!(!out.contains(&"E-UNSET-LITERAL".to_string()), "{out:?}");
    assert!(
        out.contains(&"E-MAYBE-UNSET".to_string()),
        "the raw quest.foo.state read stays independently maybe-unset: {out:?}"
    );
}
