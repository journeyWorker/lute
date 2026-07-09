//! Real `<hub>` checking semantics (dsl §7.3.2, §11.1.3).
//!
//! A `<hub id>` is a revisit conversation. Its static contract:
//! - `E-HUB-NO-EXIT` unless it has an unguarded `exit` choice OR every choice is
//!   `once` (so the eligible set provably empties → auto-exit).
//! - hub ids share the per-episode uniqueness domain with `<branch>` ids
//!   (`E-DUP-BRANCH`); choice ids are unique within a hub (`E-CHOICE-DUP`).
//! - it implicitly declares `scene.choices.<hubId>` (enum of choice ids ∪
//!   `unset`) and, per choice, `scene.visited.<hubId>.<choiceId>: bool`
//!   (default `false`) — both readable in a later `<match>`/`<when>`.
//!
//! Replaces the transitional `E-HUB-UNSUPPORTED` gate (Plan A Task 8).
use lute_check::{check, CheckInput, CheckResult, Mode, SchemaImports};
use lute_manifest::provider::ProviderSet;

fn run(text: &str) -> CheckResult {
    let input = CheckInput {
        text: text.to_string(),
        uri: "hub".into(),
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

const FM: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n";
const FM_FLAG: &str =
    "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  scene.flag: { type: bool, default: false }\n---\n";

// (a) A hub with only sticky (`once`) / guarded (`when`) choices — no unguarded
// exit and not every choice `once` — cannot provably terminate: E-HUB-NO-EXIT.
#[test]
fn hub_no_exit_rejected() {
    let out = codes(&format!(
        "{FM_FLAG}## Shot 1.\n<hub id=\"h\">\n\
         <choice id=\"a\" label=\"A\" once>\n:narrator: a.\n</choice>\n\
         <choice id=\"b\" label=\"B\" when=\"scene.flag\">\n:narrator: b.\n</choice>\n</hub>\n",
    ));
    assert!(
        out.contains(&"E-HUB-NO-EXIT".to_string()),
        "a hub with no unguarded exit and not all-`once` must yield E-HUB-NO-EXIT; got {out:?}",
    );
}

// (b) A single unguarded `<choice … exit>` satisfies the exit obligation.
#[test]
fn hub_unguarded_exit_ok() {
    let out = codes(&format!(
        "{FM}## Shot 1.\n<hub id=\"h\">\n\
         <choice id=\"a\" label=\"A\" once>\n:narrator: a.\n</choice>\n\
         <choice id=\"leave\" label=\"Leave\" exit>\n:narrator: bye.\n</choice>\n</hub>\n",
    ));
    assert!(
        !out.contains(&"E-HUB-NO-EXIT".to_string()),
        "an unguarded `exit` choice must satisfy the exit obligation; got {out:?}",
    );
}

// (c) Every choice `once` → the eligible set provably empties (auto-exit), so no
// E-HUB-NO-EXIT even without an `exit` choice; the doc checks clean.
#[test]
fn hub_all_once_ok() {
    let text = format!(
        "{FM}## Shot 1.\n<hub id=\"h\">\n\
         <choice id=\"a\" label=\"A\" once>\n:narrator: a.\n</choice>\n\
         <choice id=\"b\" label=\"B\" once>\n:narrator: b.\n</choice>\n</hub>\n",
    );
    let res = run(&text);
    assert!(res.ok, "an all-`once` hub must check clean; got {:?}", res.diagnostics);
}

// (d) The implicit recording decls are folded: a later `<match on="scene.choices.h">`
// (enum of choice ids ∪ `unset`) and `<match on="scene.visited.h.a">` (bool,
// default false) type-check exhaustively — proving both namespaces were declared.
#[test]
fn hub_records_choices_and_visited() {
    let text = format!(
        "{FM}## Shot 1.\n<hub id=\"h\">\n\
         <choice id=\"a\" label=\"A\" once>\n:narrator: a.\n</choice>\n\
         <choice id=\"leave\" label=\"Leave\" exit>\n:narrator: bye.\n</choice>\n</hub>\n\
         <match on=\"scene.choices.h\">\n\
         <when is=\"a\">\n:narrator: pa.\n</when>\n\
         <when is=\"leave\">\n:narrator: pl.\n</when>\n\
         <when is=\"unset\">\n:narrator: pu.\n</when>\n</match>\n\
         <match on=\"scene.visited.h.a\">\n\
         <when is=\"true\">\n:narrator: yes.\n</when>\n\
         <when is=\"false\">\n:narrator: no.\n</when>\n</match>\n",
    );
    let res = run(&text);
    let out: Vec<String> = res.diagnostics.iter().map(|d| d.code.clone()).collect();
    assert!(
        !out.contains(&"E-NONEXHAUSTIVE".to_string()),
        "scene.choices.<hub>/scene.visited.<hub>.<choice> must be folded as finite domains so \
         the matches are exhaustive; got {out:?}",
    );
    assert!(res.ok, "the recording matches must check clean; got {out:?}");
}

// (e) Hub ids and branch ids share one per-episode uniqueness domain: a hub id
// colliding with an earlier branch id is E-DUP-BRANCH.
#[test]
fn hub_dup_id_with_branch() {
    let out = codes(&format!(
        "{FM}## Shot 1.\n<branch id=\"dup\">\n\
         <choice id=\"x\" label=\"X\">\n:narrator: x.\n</choice>\n</branch>\n\
         <hub id=\"dup\">\n\
         <choice id=\"a\" label=\"A\" exit>\n:narrator: a.\n</choice>\n</hub>\n",
    ));
    assert!(
        out.contains(&"E-DUP-BRANCH".to_string()),
        "a hub id colliding with a branch id (shared domain) must yield E-DUP-BRANCH; got {out:?}",
    );
}

// (f) Choice ids are unique within a hub: a repeat is E-CHOICE-DUP.
#[test]
fn hub_dup_choice_id() {
    let out = codes(&format!(
        "{FM}## Shot 1.\n<hub id=\"h\">\n\
         <choice id=\"a\" label=\"A\" exit>\n:narrator: a1.\n</choice>\n\
         <choice id=\"a\" label=\"A2\">\n:narrator: a2.\n</choice>\n</hub>\n",
    ));
    assert!(
        out.contains(&"E-CHOICE-DUP".to_string()),
        "a repeated `<choice id>` within a hub must yield E-CHOICE-DUP; got {out:?}",
    );
}

// (g) A full valid hub document passes clean — no residual `E-HUB-UNSUPPORTED`
// gate (the transitional const is removed by Plan B).
#[test]
fn hub_passes_clean_check_end_to_end() {
    let text = format!(
        "{FM}## Shot 1.\n<hub id=\"chatWithBianca\">\n\
         <choice id=\"askCoffee\" label=\"Ask about the coffee\" once>\n\
         :narrator: House blend. Bold, like the clientele.\n</choice>\n\
         <choice id=\"leave\" label=\"Head out\" exit>\n:narrator: I'd better get moving.\n</choice>\n</hub>\n",
    );
    let res = run(&text);
    let out: Vec<String> = res.diagnostics.iter().map(|d| d.code.clone()).collect();
    assert!(
        !out.contains(&"E-HUB-UNSUPPORTED".to_string()),
        "the transitional hub gate must be gone; got {out:?}",
    );
    assert!(res.ok, "a valid hub document must check ok=true; got {out:?}");
}

// Regression (Task-8 review fix): defassign still walks each hub choice's `when`
// guard. A maybe-unset DECLARED read inside a guard must flag E-MAYBE-UNSET —
// it must not escape definite-assignment (mirroring `walk_branch`).
#[test]
fn hub_choice_when_guard_is_checked_by_defassign() {
    let out = codes(
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  scene.n: { type: number }\n---\n## Shot 1.\n\
         <hub id=\"h\">\n<choice id=\"a\" label=\"A\" when=\"scene.n > 0\">\n:narrator: hi.\n</choice>\n\
         <choice id=\"leave\" label=\"Leave\" exit>\n:narrator: bye.\n</choice>\n</hub>\n",
    );
    assert!(
        out.contains(&"E-MAYBE-UNSET".to_string()),
        "a hub choice `when` reading a declared no-default path must flag E-MAYBE-UNSET \
         (guard must not escape defassign); got {out:?}",
    );
}
