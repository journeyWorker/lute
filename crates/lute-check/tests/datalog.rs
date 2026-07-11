use lute_check::{check, CheckInput, Mode, SchemaImports};
use lute_manifest::provider::ProviderSet;

fn codes(text: &str) -> Vec<String> {
    let input = CheckInput {
        text: text.to_string(),
        uri: "t".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
    };
    check(&input).diagnostics.into_iter().map(|d| d.code).collect()
}

const HDR_TAIL: &str = "---\n## Shot 1.\n@narrator: hi\n";

fn scene_with(front_extra: &str) -> String {
    format!("---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n{front_extra}{HDR_TAIL}")
}

const VOCAB: &str = "entities:\n  faction: { members: [harpers, absolute] }\n  location: { members: [grove, moonrise] }\nrelations:\n  hostile: { args: [faction, faction] }\n  connected: { args: [location, location], tier: app }\n  ally: { args: [faction, faction], derive: true }\n  canReach: { args: [faction, location], derive: true }\n  orphan: { args: [faction], derive: true }\n";

#[test]
fn rule_head_must_be_derive_true() {
    let c = codes(&scene_with(&format!("{VOCAB}rules:\n  - \"hostile(A, B) :- ally(A, B)\"\n")));
    assert!(c.contains(&"E-DERIVE-UNDECLARED".to_string()), "{c:?}");
}

#[test]
fn derive_without_rules_is_warning() {
    let c = codes(&scene_with(&format!("{VOCAB}rules:\n  - \"ally(A, B) :- hostile(A, B)\"\n")));
    assert!(c.contains(&"W-DERIVE-NO-RULES".to_string()), "orphan + canReach have no rules: {c:?}");
}

#[test]
fn unbound_head_var_is_unsafe() {
    let c = codes(&scene_with(&format!("{VOCAB}rules:\n  - \"ally(A, B) :- hostile(A, A)\"\n")));
    assert!(c.contains(&"E-DATALOG-UNSAFE".to_string()), "B unbound: {c:?}");
}

#[test]
fn negated_var_needs_positive_binding() {
    let c = codes(&scene_with(&format!("{VOCAB}rules:\n  - \"ally(A, A) :- faction(A), not hostile(A, Z)\"\n")));
    assert!(c.contains(&"E-DATALOG-UNSAFE".to_string()), "Z only in negation: {c:?}");
}

#[test]
fn equality_binding_satisfies_safety() {
    let c = codes(&scene_with(&format!("{VOCAB}rules:\n  - \"ally(A, B) :- faction(A), B = A\"\n")));
    assert!(!c.contains(&"E-DATALOG-UNSAFE".to_string()), "B equality-bound (§7.1): {c:?}");
}

#[test]
fn kind_is_a_unary_domain_predicate() {
    let c = codes(&scene_with(&format!("{VOCAB}rules:\n  - \"ally(A, B) :- faction(A), faction(B), not hostile(A, B), not hostile(B, A), A != B\"\n")));
    assert!(!c.iter().any(|k| k.starts_with("E-")), "the spec §7 ally rule is clean: {c:?}");
}

#[test]
fn body_atom_unknown_and_arity() {
    let c = codes(&scene_with(&format!("{VOCAB}rules:\n  - \"ally(A, B) :- ghosts(A, B)\"\n  - \"canReach(A, L) :- hostile(A)\"\n")));
    assert!(c.contains(&"E-RELATION-UNKNOWN".to_string()), "{c:?}");
    assert!(c.contains(&"E-RELATION-ARITY".to_string()), "{c:?}");
}

#[test]
fn head_const_is_domain_checked() {
    let c = codes(&scene_with(&format!("{VOCAB}rules:\n  - \"ally(grove, B) :- hostile(B, B)\"\n")));
    assert!(c.contains(&"E-FACT-DOMAIN".to_string()), "grove is a location, not a faction (D6): {c:?}");
}

#[test]
fn guard_with_fact_query_is_guard_fact() {
    let front = format!("{VOCAB}state:\n  run.act: {{ type: number, default: 1 }}\nrules:\n  - \"ally(A, A) :- faction(A), cel(\\\"holds(hostile(harpers, absolute))\\\")\"\n");
    let c = codes(&scene_with(&front));
    assert!(c.contains(&"E-DATALOG-GUARD-FACT".to_string()), "{c:?}");
}

#[test]
fn now_in_guard_is_guard_fact_and_clean_scalar_guard_is_fine() {
    let now = codes(&scene_with(&format!("{VOCAB}rules:\n  - \"ally(A, A) :- faction(A), cel(\\\"now() < now()\\\")\"\n")));
    assert!(now.contains(&"E-DATALOG-GUARD-FACT".to_string()), "D7: {now:?}");
    let ok = codes(&scene_with(&format!("{VOCAB}state:\n  run.act: {{ type: number, default: 1 }}\nrules:\n  - \"ally(A, A) :- faction(A), cel(\\\"run.act == 1\\\")\"\n")));
    assert!(!ok.iter().any(|k| k == "E-DATALOG-GUARD-FACT" || k == "E-CEL-PROFILE"), "{ok:?}");
}

#[test]
fn guard_reading_undeclared_scalar_is_undeclared() {
    let c = codes(&scene_with(&format!("{VOCAB}rules:\n  - \"ally(A, A) :- faction(A), cel(\\\"run.ghost == 1\\\")\"\n")));
    assert!(c.contains(&"E-UNDECLARED".to_string()), "{c:?}");
}
