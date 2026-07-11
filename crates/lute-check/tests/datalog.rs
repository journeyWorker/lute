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

fn guard_tainted(text: &str) -> std::collections::BTreeSet<String> {
    let (mut doc, _) = lute_syntax::parse(text);
    let mut arena = lute_cel::CelArena::default();
    lute_cel::fill_document(&mut arena, &mut doc);
    let input = CheckInput {
        text: text.to_string(),
        uri: "t".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
    };
    let (folded, _, _) = lute_check::fold_env(&doc, &input);
    folded.env.rel_vocab.guard_tainted.clone()
}

#[test]
fn positive_recursion_is_legal() {
    let front = "entities:\n  c: { members: [ana] }\n  loc: { members: [grove, moonrise] }\nrelations:\n  atLocation: { args: [c, loc], key: [0] }\n  connected: { args: [loc, loc], tier: app }\n  canReach: { args: [c, loc], derive: true }\nrules:\n  - \"canReach(C, L) :- atLocation(C, L)\"\n  - \"canReach(C, L2) :- canReach(C, L1), connected(L1, L2)\"\n";
    let c = codes(&scene_with(front));
    assert!(!c.contains(&"E-DATALOG-UNSTRATIFIED".to_string()), "§7.2 positive recursion: {c:?}");
}

#[test]
fn negation_cycle_is_unstratified() {
    let front = "entities:\n  f: { members: [a, b] }\nrelations:\n  p: { args: [f], derive: true }\n  q: { args: [f], derive: true }\nrules:\n  - \"p(X) :- f(X), not q(X)\"\n  - \"q(X) :- f(X), not p(X)\"\n";
    let c = codes(&scene_with(front));
    assert!(c.contains(&"E-DATALOG-UNSTRATIFIED".to_string()), "{c:?}");
}

#[test]
fn negation_cycle_spanning_two_files_is_caught_post_merge() {
    // §4.1/§7.2: checked on the MERGED rule set
    fn zero_span() -> lute_core_span::Span {
        lute_core_span::Span { byte_start: 0, byte_end: 0, line: 1, column: 1, utf16_range: (0, 0) }
    }
    let dir = {
        let d = std::env::temp_dir().join(format!("lute_strat_{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    };
    std::fs::write(dir.join("a.yaml"), "entities:\n  f: { members: [a] }\nrelations:\n  p: { args: [f], derive: true }\n  q: { args: [f], derive: true }\nrules:\n  - \"p(X) :- f(X), not q(X)\"\n").unwrap();
    std::fs::write(dir.join("b.yaml"), "uses: a.yaml\nrules:\n  - \"q(X) :- f(X), not p(X)\"\n").unwrap();
    let imports = lute_check::resolve_imports(&dir, &["b.yaml".into()], &[], zero_span());
    let input = lute_check::CheckInput {
        text: scene_with(""),
        uri: "t".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: lute_manifest::provider::ProviderSet::default(),
        mode: lute_check::Mode::Author,
        imports,
        components: Default::default(),
    };
    let c: Vec<String> = lute_check::check(&input).diagnostics.into_iter().map(|d| d.code).collect();
    assert!(c.contains(&"E-DATALOG-UNSTRATIFIED".to_string()), "{c:?}");
}

#[test]
fn guard_taint_propagates_to_downstream_readers() {
    let front = "entities:\n  faction: { members: [harpers, absolute] }\nrelations:\n  hostile: { args: [faction, faction] }\n  guarded: { args: [faction], derive: true }\n  downstream: { args: [faction], derive: true }\n  clean: { args: [faction], derive: true }\nstate:\n  run.act: { type: number, default: 1 }\nrules:\n  - \"guarded(A) :- faction(A), cel(\\\"run.act == 1\\\")\"\n  - \"downstream(A) :- guarded(A)\"\n  - \"clean(A) :- faction(A)\"\n";
    let tainted = guard_tainted(&scene_with(front));
    assert!(tainted.contains("guarded"), "directly guarded: {tainted:?}");
    assert!(tainted.contains("downstream"), "downstream reads guarded, so it is tainted too (§6): {tainted:?}");
    assert!(!tainted.contains("clean"), "clean never reads a guarded relation: {tainted:?}");
}
