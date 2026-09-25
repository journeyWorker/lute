//! dsl 0.24.0 §3: entity sub-kinds (`subsetOf:`) and entity-indexed state
//! (`per:`), read by a rule variable in a rule `cel()` guard
//! (`run.approval[P]`).

use lute_check::{check, fold_env, CheckInput, Mode, SchemaImports};
use lute_manifest::provider::ProviderSet;

fn input(text: &str) -> CheckInput {
    CheckInput {
        text: text.to_string(),
        uri: "t".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
        defaults: Default::default(),
    }
}

fn diags(text: &str) -> Vec<(String, String)> {
    check(&input(text))
        .diagnostics
        .into_iter()
        .map(|d| (d.code, d.message))
        .collect()
}

fn with_code(all: &[(String, String)], code: &str) -> Vec<String> {
    all.iter().filter(|(c, _)| c == code).map(|(_, m)| m.clone()).collect()
}

/// A scene declaring `front` (frontmatter lines after the scene triad) with
/// `body` as its only shot.
fn scene(front: &str, body: &str) -> String {
    format!("---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n{front}---\n## Shot 1.\n{body}")
}

const PARTY: &str = "\
state:
  run.approval: { type: number, default: 0, per: companion }
entities:
  person: { members: [isolde, corvin, oda] }
  companion: { subsetOf: person, members: [isolde, corvin] }
relations:
  recruited: { args: [person], tier: run }
  sworn: { args: [companion], tier: run }
  inParty: { args: [companion], derive: true }
  loyal: { args: [companion], derive: true }
facts:
  - \"recruited(isolde)\"
";

fn party(rules: &str, body: &str) -> String {
    scene(&format!("{PARTY}rules:\n{rules}"), body)
}

const RULES: &str = "  - \"inParty(P) :- companion(P), recruited(P)\"\n  - \"loyal(P) :- inParty(P), cel(\\\"run.approval[P] >= 3\\\")\"\n";

#[test]
fn a_sub_kind_repeating_its_parents_members_is_not_a_kind_clash() {
    let all = diags(&party(RULES, "@narrator: hi\n"));
    assert!(with_code(&all, "E-ENTITY-KIND-CLASH").is_empty(), "{all:?}");
    assert!(with_code(&all, "E-ENTITY-KIND-SHAPE").is_empty(), "{all:?}");
}

#[test]
fn two_unrelated_kinds_sharing_an_id_still_clash_and_name_subset_of() {
    let all = diags(&scene(
        "entities:\n  person: { members: [ana] }\n  faction: { members: [ana] }\n",
        "@narrator: hi\n",
    ));
    let clash = with_code(&all, "E-ENTITY-KIND-CLASH");
    assert_eq!(clash.len(), 1, "{all:?}");
    assert!(clash[0].contains("subsetOf:"), "{}", clash[0]);
}

#[test]
fn a_sub_kind_member_outside_its_parent_is_named() {
    let all = diags(&scene(
        "entities:\n  person: { members: [isolde] }\n  companion: { subsetOf: person, members: [isolde, wren] }\n",
        "@narrator: hi\n",
    ));
    let shape = with_code(&all, "E-ENTITY-KIND-SHAPE");
    assert_eq!(shape.len(), 1, "{all:?}");
    assert!(shape[0].contains("`wren` is not a member of `person`"), "{}", shape[0]);
}

#[test]
fn a_sub_kind_of_an_undeclared_or_open_kind_is_a_shape_error() {
    let all = diags(&scene(
        "entities:\n  npc: { open: engine }\n  companion: { subsetOf: persn, members: [a] }\n  guard: { subsetOf: npc, members: [b] }\n",
        "@narrator: hi\n",
    ));
    let shape = with_code(&all, "E-ENTITY-KIND-SHAPE");
    assert_eq!(shape.len(), 2, "{all:?}");
    assert!(shape.iter().any(|m| m.contains("`persn` is not a declared entity kind")), "{shape:?}");
    assert!(shape.iter().any(|m| m.contains("`npc` is `open:`")), "{shape:?}");
}

#[test]
fn a_relation_over_a_sub_kind_rejects_a_parent_only_member() {
    // `oda` is a person: legal for a relation over `person`, not for one
    // over the sub-kind `companion`.
    let all = diags(&party(RULES, "::assert{recruited(oda)}\n::assert{sworn(isolde)}\n@narrator: hi\n"));
    assert!(with_code(&all, "E-FACT-DOMAIN").is_empty(), "{all:?}");
    let all = diags(&party(RULES, "::assert{sworn(oda)}\n@narrator: hi\n"));
    let dom = with_code(&all, "E-FACT-DOMAIN");
    assert_eq!(dom.len(), 1, "{all:?}");
    assert!(dom[0].contains("`oda` is not a declared member of entity kind `companion`"), "{}", dom[0]);
}

#[test]
fn per_declares_one_path_per_member() {
    let all = diags(&party(
        RULES,
        "@narrator{when=\"run.approval.isolde >= 1 && run.approval.corvin < 2\"}: ok\n@narrator{when=\"run.approval.oda >= 1\"}: not a companion\n",
    ));
    let undeclared = with_code(&all, "E-UNDECLARED");
    assert_eq!(undeclared.len(), 1, "{all:?}");
    assert!(undeclared[0].contains("run.approval.oda"), "{}", undeclared[0]);
}

#[test]
fn per_state_folds_into_the_schema_with_defaults_and_prev_run_mirrors() {
    let text = party(RULES, "@narrator: hi\n");
    let (doc, _) = lute_syntax::parse(&text);
    let (folded, _, _) = fold_env(&doc, &input(&text));
    let decls = &folded.env.state.decls;
    for p in ["run.approval.isolde", "run.approval.corvin", "prev.run.approval.isolde"] {
        assert!(decls.contains_key(p), "{p} missing: {:?}", decls.keys().collect::<Vec<_>>());
    }
    assert!(!decls.contains_key("run.approval"), "the family itself is not a path");
    assert!(!decls.contains_key("run.approval.oda"));
    assert_eq!(
        folded.env.rel_vocab.indexed_state.get("run.approval").map(String::as_str),
        Some("companion")
    );
}

#[test]
fn per_over_an_open_or_unknown_kind_is_a_state_decl_error() {
    let all = diags(&scene(
        "state:\n  run.a: { type: number, default: 0, per: npc }\n  run.b: { type: number, default: 0, per: nobody }\nentities:\n  npc: { open: engine }\n",
        "@narrator: hi\n",
    ));
    let decl = with_code(&all, "E-STATE-DECL");
    assert_eq!(decl.len(), 2, "{all:?}");
    assert!(decl.iter().any(|m| m.contains("`per: npc` names an `open:` entity kind")), "{decl:?}");
    assert!(decl.iter().any(|m| m.contains("`per: nobody` names no entity kind")), "{decl:?}");
}

#[test]
fn a_rule_guard_reads_indexed_state_by_a_bound_variable() {
    let all = diags(&party(RULES, "@narrator{when=\"holds(loyal(isolde))\"}: loyal\n"));
    let errors: Vec<_> = all.iter().filter(|(c, _)| c.starts_with("E-")).collect();
    assert!(errors.is_empty(), "{all:?}");
}

#[test]
fn a_rule_guard_index_over_a_wider_kind_is_a_fact_domain_error() {
    let rules = "  - \"inParty(P) :- companion(P), recruited(P)\"\n  - \"loyal(P) :- recruited(P), cel(\\\"run.approval[P] >= 3\\\")\"\n";
    let all = diags(&party(rules, "@narrator: hi\n"));
    let dom = with_code(&all, "E-FACT-DOMAIN");
    assert_eq!(dom.len(), 1, "{all:?}");
    assert!(dom[0].contains("`recruited(P)` over `person`"), "{}", dom[0]);
    // No cascade onto the bare variable or the family path.
    assert!(with_code(&all, "E-CEL-PROFILE").is_empty(), "{all:?}");
    assert!(with_code(&all, "E-UNDECLARED").is_empty(), "{all:?}");
}

#[test]
fn a_rule_guard_index_needs_a_positive_binding_and_an_indexed_family() {
    let rules = "  - \"inParty(P) :- companion(P), recruited(P)\"\n  - \"loyal(P) :- inParty(P), cel(\\\"run.approval[Q] >= 3 && run.gold[P] > 1\\\")\"\n";
    let front = PARTY.replacen("state:\n", "state:\n  run.gold: { type: number, default: 0 }\n", 1);
    let all = diags(&scene(&format!("{front}rules:\n{rules}"), "@narrator: hi\n"));
    assert!(
        with_code(&all, "E-DATALOG-UNSAFE").iter().any(|m| m.contains("`run.approval[Q]`")),
        "{all:?}"
    );
    assert!(
        with_code(&all, "E-UNDECLARED").iter().any(|m| m.contains("`run.gold` is not entity-indexed state")),
        "{all:?}"
    );
}

#[test]
fn an_index_outside_a_rule_guard_says_to_name_the_member() {
    let all = diags(&party(RULES, "@narrator{when=\"run.approval >= 1\"}: family\n"));
    let undeclared = with_code(&all, "E-UNDECLARED");
    assert_eq!(undeclared.len(), 1, "{all:?}");
    assert!(undeclared[0].contains("entity-indexed (`per: companion`)"), "{}", undeclared[0]);
}

#[test]
fn indexed_rules_ground_once_per_member() {
    let text = party(RULES, "@narrator: hi\n");
    let (doc, _) = lute_syntax::parse(&text);
    let (folded, _, _) = fold_env(&doc, &input(&text));
    let rules = lute_check::evaluable_rules(&folded.env.rel_vocab);
    let raws: Vec<&str> = rules.iter().map(|r| r.raw.as_str()).collect();
    assert_eq!(rules.len(), 3, "{raws:?}");
    let loyal: Vec<String> = rules
        .iter()
        .filter(|r| r.rule.head.relation == "loyal")
        .map(|r| {
            let guard = r
                .rule
                .body
                .iter()
                .find_map(|l| match l {
                    lute_syntax::datalog::BodyLiteral::Guard { cel, .. } => Some(cel.clone()),
                    _ => None,
                })
                .unwrap();
            format!("{:?} {guard} | {}", r.rule.head.terms, r.raw)
        })
        .collect();
    assert_eq!(
        loyal,
        vec![
            "[Const(\"isolde\")] run.approval.isolde >= 3 | loyal(P) :- inParty(P), cel(\"run.approval[P] >= 3\") [P = isolde]".to_string(),
            "[Const(\"corvin\")] run.approval.corvin >= 3 | loyal(P) :- inParty(P), cel(\"run.approval[P] >= 3\") [P = corvin]".to_string(),
        ]
    );
}
