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
    let cs = codes("---\nkind: quest\nstate:\n  run.d: { type: bool, default: false }\n---\n\
                    <quest id=\"q\">\n<objective id=\"o\" done=\"run.d\"/>\n\
                    <on event=\"questComplete\">\n<match on=\"quest.q.state\">\n\
                    <when is=\"complete\">\n:x: done\n</when>\n<otherwise>\n:x: -\n</otherwise>\n\
                    </match>\n</on>\n</quest>\n");
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
                    <hub id=\"h\">\n<choice id=\"c\" label=\"L\" exit>\n:x: bye\n</choice>\n</hub>\n</quest>\n");
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

// --- Review Finding 1: quest branch folding (fold_branches over doc.quests) -

#[test]
fn quest_branch_duplicate_choice_ids_errors() {
    // A <branch> living directly in a <quest> body was never folded by the
    // scene-only fold_branches pre-pass, so its E-CHOICE-DUP dup-detection
    // never ran (and its implicit scene.choices.<id> decl was never folded).
    let cs = codes(
        "---\nkind: quest\n---\n<quest id=\"q\">\n<objective id=\"o\" done=\"run.d\"/>\n\
         <branch id=\"b\">\n<choice id=\"c\" label=\"A\">\n:x: hi\n</choice>\n\
         <choice id=\"c\" label=\"B\">\n:x: bye\n</choice>\n</branch>\n</quest>\n",
    );
    assert!(cs.contains(&"E-CHOICE-DUP".to_string()), "{cs:?}");
}

// --- Review Finding 2: quest directive-slot expansion (fold_directive_slots
// over doc.quests) ---------------------------------------------------------

/// Same synthetic `::minigame` directive as `tests/directive_slots.rs` (the
/// core snapshot carries no directive with declared state slots).
fn snapshot_with_minigame() -> lute_manifest::snapshot::CapabilitySnapshot {
    use lute_manifest::schema::*;
    use lute_manifest::types::{Field, FromAttr, Literal as Lit, PathSegment, Type};
    let mut snap = lute_manifest::core::load_core_snapshot();
    snap.state_shapes.insert(
        "minigameResult".into(),
        StateShape {
            name: "minigameResult".into(),
            fields: vec![Field {
                name: "rank".into(),
                ty: Type::Enum(vec![
                    "fail".into(),
                    "bronze".into(),
                    "silver".into(),
                    "gold".into(),
                ]),
                default: Some(Lit::Str("fail".into())),
                required: false,
                shape: None,
            }],
        },
    );
    snap.directives.insert(
        "minigame".into(),
        DirectiveDecl {
            name: "minigame".into(),
            layer: Some("bridge".into()),
            attrs: vec![
                AttrDecl {
                    name: "kind".into(),
                    required: true,
                    ty: Type::Str,
                    default: None,
                },
                AttrDecl {
                    name: "id".into(),
                    required: true,
                    ty: Type::Str,
                    default: None,
                },
                AttrDecl {
                    name: "resultKey".into(),
                    required: true,
                    ty: Type::SlotId {
                        namespace: "scene.minigame".into(),
                    },
                    default: None,
                },
                AttrDecl {
                    name: "wait".into(),
                    required: false,
                    ty: Type::Bool,
                    default: Some(Lit::Bool(true)),
                },
            ],
            semantics: vec![],
            state: Some(DirectiveState {
                declares: vec![SlotDecl {
                    scope: "scene".into(),
                    path: vec![
                        PathSegment::Literal("minigame".into()),
                        PathSegment::FromAttr {
                            from_attr: FromAttr {
                                name: "resultKey".into(),
                                slot_type: Some("localId".into()),
                            },
                        },
                    ],
                    shape: "minigameResult".into(),
                }],
            }),
            effects: None,
            bridge: None,
            lower: Lowering::Builtin {
                kind: "builtin".into(),
                name: "bridgeMinigame".into(),
            },
        },
    );
    snap
}

fn codes_with(text: &str, snap: lute_manifest::snapshot::CapabilitySnapshot) -> Vec<String> {
    let input = CheckInput {
        text: text.into(),
        uri: "quest".into(),
        snapshot: snap,
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
    };
    check(&input).diagnostics.into_iter().map(|d| d.code).collect()
}

#[test]
fn quest_on_directive_slot_opens_scene_path() {
    // `fold_directive_slots` roots only in doc.shots; a quest `<on>` body's
    // `::minigame` never opened `scene.minigame.service01.rank`, so a valid
    // read of it wrongly reported E-UNDECLARED.
    let text = "---\nkind: quest\n---\n<quest id=\"q\">\n<objective id=\"o\" done=\"true\"/>\n\
                <on event=\"questActive\">\n\
                ::minigame{kind=\"rhythm\" id=\"x\" resultKey=\"service01\" wait=\"true\"}\n\
                <match on=\"scene.minigame.service01.rank\">\n\
                <when test=\"$ == 'gold'\">\n:x: hi\n</when>\n\
                <otherwise>\n:x: bye\n</otherwise>\n\
                </match>\n</on>\n</quest>\n";
    let cs = codes_with(text, snapshot_with_minigame());
    assert!(!cs.contains(&"E-UNDECLARED".to_string()), "{cs:?}");
}

// --- Review Finding 3: quest/objective id CelIdent validation (§8.4) -------

#[test]
fn quest_id_with_hyphen_errors() {
    let cs = codes(
        "---\nkind: quest\n---\n<quest id=\"bad-id\">\n<objective id=\"o\" done=\"run.d\"/>\n</quest>\n",
    );
    assert!(cs.contains(&"E-PATH-IDENT".to_string()), "{cs:?}");
}

#[test]
fn objective_id_with_hyphen_errors() {
    let cs = codes(
        "---\nkind: quest\n---\n<quest id=\"q\">\n<objective id=\"bad-oid\" done=\"run.d\"/>\n</quest>\n",
    );
    assert!(cs.contains(&"E-PATH-IDENT".to_string()), "{cs:?}");
}

// --- Review Finding 4: quest start/fail definite-assignment (§9.4) ---------

#[test]
fn quest_start_guard_maybe_unset_without_default() {
    let text = "---\nkind: quest\nstate:\n  run.ready: { type: bool }\n---\n\
                <quest id=\"q\" start=\"run.ready\">\n<objective id=\"o\" done=\"true\"/>\n</quest>\n";
    let cs = codes(text);
    assert!(cs.contains(&"E-MAYBE-UNSET".to_string()), "{cs:?}");
}

#[test]
fn quest_start_guard_clean_with_default() {
    let text = "---\nkind: quest\nstate:\n  run.ready: { type: bool, default: false }\n---\n\
                <quest id=\"q\" start=\"run.ready\">\n<objective id=\"o\" done=\"true\"/>\n</quest>\n";
    let cs = codes(text);
    assert!(!cs.contains(&"E-MAYBE-UNSET".to_string()), "{cs:?}");
}

// --- CheckFix F6/F7: `<quest id>`/`<objective id>` required (§6.3/§6.4) -----

#[test]
fn quest_missing_id_errors() {
    let cs = codes("---\nkind: quest\n---\n<quest>\n<objective id=\"o\" done=\"a\"/>\n</quest>\n");
    assert!(cs.contains(&"E-QUEST-ID-MISSING".to_string()), "{cs:?}");
}

#[test]
fn objective_missing_id_errors() {
    let cs = codes("---\nkind: quest\n---\n<quest id=\"q\">\n<objective done=\"x\"/>\n</quest>\n");
    assert!(cs.contains(&"E-OBJECTIVE-ID-MISSING".to_string()), "{cs:?}");
}
