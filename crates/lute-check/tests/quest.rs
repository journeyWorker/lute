//! Real quest-kind checking semantics (dsl 0.2.0 §3.1, §5, §6): the `kind:`
//! discriminator, the `quest.*` state tier, `check_quest`/`check_objective`
//! folded reserved decls, grammar admission, and the reserved-write policy.
//!
//! Replaces Plan A's transitional `E-QUEST-UNSUPPORTED` gate
//! (`tests/quest_transitional.rs`, deleted here).

use lute_check::{check, CheckInput, CheckResult, Mode, SchemaImports};
use lute_manifest::provider::ProviderSet;

fn run(text: &str) -> CheckResult {
    let input = CheckInput {
        text: text.to_string(),
        uri: "quest".into(),
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

// --- Task 1: `kind:` discriminator (E-KIND-MISSING / E-UNKNOWN-KIND) -------

#[test]
fn scene_kind_missing_errors() {
    // a doc with the scene triad but no `kind:` -> E-KIND-MISSING.
    assert!(codes("---\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n:x: hi\n")
        .contains(&"E-KIND-MISSING".to_string()));
}

#[test]
fn unknown_kind_errors() {
    assert!(codes("---\nkind: reward\n---\n").contains(&"E-UNKNOWN-KIND".to_string()));
}

#[test]
fn kind_scene_is_clean_discriminator() {
    // explicit kind: scene + triad -> no E-KIND-MISSING/E-UNKNOWN-KIND.
    let cs = codes("---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n:x: hi\n");
    assert!(!cs.iter().any(|c| c == "E-KIND-MISSING" || c == "E-UNKNOWN-KIND"), "{cs:?}");
}

// --- Task 2: MetaKind::Quest + kind-scoped frontmatter keys ----------------

#[test]
fn quest_needs_no_scene_triad() {
    let cs = codes("---\nkind: quest\n---\n<quest id=\"q\">\n<objective id=\"o\" done=\"run.d\"/>\n</quest>\n");
    assert!(!cs.iter().any(|c| c == "E-META-MISSING"), "{cs:?}");
}

#[test]
fn quest_rejects_scene_triad_key() {
    let cs = codes("---\nkind: quest\ncharacter: x\n---\n<quest id=\"q\">\n<objective id=\"o\" done=\"a\"/>\n</quest>\n");
    assert!(cs.contains(&"E-META-UNKNOWN-KEY".to_string()), "{cs:?}");
}

// --- Task 3: quest.* state tier ---------------------------------------------

#[test]
fn quest_scratch_path_is_a_declared_tier() {
    // declaring quest.q.count in state: and reading it must NOT be E-STATE-NAMESPACE / E-UNDECLARED.
    let cs = codes("---\nkind: quest\nstate:\n  quest.q.count: { type: number, default: 0 }\n---\n\
                    <quest id=\"q\">\n<objective id=\"o\" done=\"quest.q.count >= 1\"/>\n</quest>\n");
    assert!(!cs.iter().any(|c| c == "E-STATE-NAMESPACE" || c == "E-UNDECLARED"), "{cs:?}");
}

// --- Task 4: check_quest/check_objective — folded reserved decls -----------

#[test]
fn duplicate_quest_id_errors() {
    let cs = codes("---\nkind: quest\n---\n<quest id=\"q\">\n<objective id=\"o\" done=\"a\"/>\n</quest>\n\
                    <quest id=\"q\">\n<objective id=\"o2\" done=\"b\"/>\n</quest>\n");
    assert!(cs.contains(&"E-QUEST-ID-DUP".to_string()), "{cs:?}");
}

#[test]
fn duplicate_objective_id_in_quest_errors() {
    let cs = codes("---\nkind: quest\n---\n<quest id=\"q\">\n<objective id=\"o\" done=\"a\"/>\n<objective id=\"o\" done=\"b\"/>\n</quest>\n");
    assert!(cs.contains(&"E-OBJECTIVE-ID-DUP".to_string()), "{cs:?}");
}

#[test]
fn objective_missing_done_errors() {
    let cs = codes("---\nkind: quest\n---\n<quest id=\"q\">\n<objective id=\"o\"/>\n</quest>\n");
    assert!(cs.contains(&"E-OBJECTIVE-MISSING-DONE".to_string()), "{cs:?}");
}

#[test]
fn quest_state_readable_in_match() {
    // quest.q.state is an implicitly-declared enum; a match over it covering unset is clean.
    let cs = codes("---\nkind: quest\n---\n<quest id=\"q\">\n<objective id=\"o\" done=\"run.d\"/>\n\
                    <on event=\"questComplete\">\n<match on=\"quest.q.state\">\n\
                    <when is=\"complete\">:x: done</when>\n<otherwise>:x: -</otherwise>\n</match>\n</on>\n</quest>\n");
    assert!(!cs.iter().any(|c| c == "E-UNDECLARED"), "{cs:?}");
}

// --- Task 5: table-driven grammar admission ---------------------------------

#[test]
fn scene_rejects_on_and_objective_and_quest() {
    let cs = codes("---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n\
                    <on event=\"questComplete\">\n:x: hi\n</on>\n");
    assert!(cs.contains(&"E-GRAMMAR-NOT-ADMITTED".to_string()), "{cs:?}");
}

#[test]
fn quest_rejects_hub_timeline_and_headings() {
    let cs = codes("---\nkind: quest\n---\n<quest id=\"q\">\n<objective id=\"o\" done=\"a\"/>\n\
                    <hub id=\"h\">\n<choice id=\"c\" label=\"L\" exit>:x: bye</choice>\n</hub>\n</quest>\n");
    assert!(cs.contains(&"E-GRAMMAR-NOT-ADMITTED".to_string()), "{cs:?}");
}

#[test]
fn quest_doc_with_shot_heading_is_not_admitted() {
    let cs = codes("---\nkind: quest\n---\n## Shot 1.\n:x: hi\n");
    assert!(cs.contains(&"E-GRAMMAR-NOT-ADMITTED".to_string()), "{cs:?}");
}

#[test]
fn nested_objective_inside_on_arm_not_admitted() {
    let cs = codes("---\nkind: quest\n---\n<quest id=\"q\">\n<objective id=\"o\" done=\"a\"/>\n\
                    <on event=\"questActive\">\n<objective id=\"bad\" done=\"b\"/>\n</on>\n</quest>\n");
    assert!(cs.contains(&"E-GRAMMAR-NOT-ADMITTED".to_string()), "{cs:?}");
}

#[test]
fn quest_body_admits_objective_on_match_branch_set_content() {
    let cs = codes("---\nkind: quest\n---\n<quest id=\"q\">\n<objective id=\"o\" done=\"run.d\"/>\n\
                    <on event=\"questComplete\">\n::set{run.x = 1}\n:x: hi\n</on>\n</quest>\n");
    assert!(!cs.contains(&"E-GRAMMAR-NOT-ADMITTED".to_string()), "{cs:?}");
}

// --- Task 6: <on> semantics + write-policy ----------------------------------

#[test]
fn set_to_reserved_quest_state_errors() {
    let cs = codes("---\nkind: quest\n---\n<quest id=\"q\">\n<objective id=\"o\" done=\"run.d\"/>\n\
                    <on event=\"questActive\">\n::set{quest.q.state = \"complete\"}\n</on>\n</quest>\n");
    assert!(cs.contains(&"E-QUEST-RESERVED-WRITE".to_string()), "{cs:?}");
}

#[test]
fn set_to_objective_done_errors() {
    let cs = codes("---\nkind: quest\n---\n<quest id=\"q\">\n<objective id=\"o\" done=\"run.d\"/>\n\
                    <on event=\"questActive\">\n::set{quest.q.objectives.o.done = true}\n</on>\n</quest>\n");
    assert!(cs.contains(&"E-QUEST-RESERVED-WRITE".to_string()), "{cs:?}");
}

#[test]
fn no_more_quest_unsupported() {
    let cs = codes("---\nkind: quest\n---\n<quest id=\"q\">\n<objective id=\"o\" done=\"run.d\"/>\n</quest>\n");
    assert!(!cs.contains(&"E-QUEST-UNSUPPORTED".to_string()), "{cs:?}");
}

// --- Task 7: quest lineId identity ------------------------------------------

#[test]
fn duplicate_line_code_within_a_quest_errors() {
    // two lines with the same speaker+code inside one quest -> E-DUP-LINE-CODE.
    let cs = codes("---\nkind: quest\n---\n<quest id=\"q\">\n<objective id=\"o\" done=\"a\"/>\n\
                    <on event=\"questActive\">\n:x{code=\"0010\"}: one\n:x{code=\"0010\"}: two\n</on>\n</quest>\n");
    assert!(cs.contains(&"E-DUP-LINE-CODE".to_string()), "{cs:?}");
}
