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

#[test]
fn relations_facts_rules_are_known_meta_keys() {
    let c = codes(&scene_with(
        "entities:\n  character: { members: [ana] }\nrelations:\n  inParty: { args: [character] }\nfacts:\n  - \"inParty(ana)\"\nrules: []\n",
    ));
    assert!(!c.contains(&"E-META-UNKNOWN-KEY".to_string()), "{c:?}");
}

#[test]
fn unquoted_rule_mapping_is_datalog_parse() {
    // `head :- body` unquoted → YAML mapping, not a string (spec §4 Quoting).
    let c = codes(&scene_with("rules:\n  - canReach(C, L) : atLocation(C, L)\n"));
    assert!(c.contains(&"E-DATALOG-PARSE".to_string()), "{c:?}");
}

#[test]
fn malformed_fact_string_is_datalog_parse() {
    let c = codes(&scene_with("facts:\n  - \"hostile harpers\"\n"));
    assert!(c.contains(&"E-DATALOG-PARSE".to_string()), "{c:?}");
}

#[test]
fn function_term_in_rule_is_datalog_function() {
    let c = codes(&scene_with("rules:\n  - \"d(X) :- b(f(X))\"\n"));
    assert!(c.contains(&"E-DATALOG-FUNCTION".to_string()), "{c:?}");
}

#[test]
fn hyphenated_relation_name_is_path_ident() {
    let c = codes(&scene_with("relations:\n  in-party: { args: [character] }\nentities:\n  character: { members: [ana] }\n"));
    assert!(c.contains(&"E-PATH-IDENT".to_string()), "{c:?}");
}

#[test]
fn kind_shape_neither_or_both_is_entity_kind_shape() {
    let c = codes(&scene_with("entities:\n  ghost: {}\n  both: { members: [x], open: engine }\n"));
    assert_eq!(c.iter().filter(|k| *k == "E-ENTITY-KIND-SHAPE").count(), 2, "{c:?}");
}

#[test]
fn id_in_two_kinds_is_entity_kind_clash() {
    let c = codes(&scene_with("entities:\n  character: { members: [ana] }\n  faction: { members: [ana] }\n"));
    assert!(c.contains(&"E-ENTITY-KIND-CLASH".to_string()), "{c:?}");
}

#[test]
fn kind_colliding_with_relation_is_kind_name_clash() {
    let c = codes(&scene_with("entities:\n  inParty: { members: [x] }\nrelations:\n  inParty: { args: [inParty] }\n"));
    assert!(c.contains(&"E-KIND-NAME-CLASH".to_string()), "{c:?}");
}

#[test]
fn relation_shape_diagnostics() {
    let c = codes(&scene_with(
        "entities:\n  c: { members: [x] }\nrelations:\n  empty: {}\n  badArg: { args: [nowhere] }\n  badKey: { args: [c], key: [3] }\n  dupKey: { args: [c, c], key: [0, 0] }\n  derived: { args: [c], derive: true, tier: run }\n  conflicted: { args: [c], derive: true, reserved: true }\n",
    ));
    assert!(c.contains(&"E-RELATION-EMPTY".to_string()), "{c:?}");
    assert!(c.iter().filter(|k| *k == "E-RELATION-DOMAIN").count() >= 3, "badArg+badKey+dupKey: {c:?}");
    assert!(c.contains(&"E-DERIVE-TIER".to_string()), "{c:?}");
    assert!(c.contains(&"E-RELATION-RESERVED-WRITE".to_string()), "both flags: {c:?}");
}

#[test]
fn raw_duplicate_relation_key_is_relation_dup() {
    // serde_yaml collapses duplicate keys; the Task 5 raw-text scan preserves them.
    let c = codes(&scene_with(
        "entities:\n  c: { members: [x] }\nrelations:\n  inParty: { args: [c] }\n  inParty: { args: [c, c] }\n",
    ));
    assert!(c.contains(&"E-RELATION-DUP".to_string()), "{c:?}");
}

#[test]
fn seed_fact_validation() {
    let front = "entities:\n  c: { members: [ana] }\n  npc: { open: engine }\nenums:\n  trust: [low, high]\nrelations:\n  knows: { args: [c, trust] }\n  met: { args: [c, npc] }\n";
    // unknown relation
    let c = codes(&scene_with(&format!("{front}facts:\n  - \"ghost(ana)\"\n")));
    assert!(c.contains(&"E-RELATION-UNKNOWN".to_string()), "{c:?}");
    // wrong arity
    let c = codes(&scene_with(&format!("{front}facts:\n  - \"knows(ana)\"\n")));
    assert!(c.contains(&"E-RELATION-ARITY".to_string()), "{c:?}");
    // non-member enum arg
    let c = codes(&scene_with(&format!("{front}facts:\n  - \"knows(ana, sideways)\"\n")));
    assert!(c.contains(&"E-FACT-DOMAIN".to_string()), "{c:?}");
    // wildcard in a seed (D12)
    let c = codes(&scene_with(&format!("{front}facts:\n  - \"knows(ana, _)\"\n")));
    assert!(c.contains(&"E-RETRACT-WILDCARD-ASSERT".to_string()), "{c:?}");
    // open-kind arg: unknown id is FINE (D10)…
    let c = codes(&scene_with(&format!("{front}facts:\n  - \"met(ana, minted77)\"\n")));
    assert!(!c.contains(&"E-FACT-DOMAIN".to_string()), "open kind must not membership-check: {c:?}");
    // …but an id belonging to a DIFFERENT closed kind is not (one-id-one-kind)
    let c = codes(&scene_with(&format!("{front}facts:\n  - \"met(ana, ana)\"\n")));
    assert!(c.contains(&"E-FACT-DOMAIN".to_string()), "{c:?}");
    // clean seeds stay clean
    let c = codes(&scene_with(&format!("{front}facts:\n  - \"knows(ana, low)\"\n")));
    assert!(!c.iter().any(|k| k.starts_with("E-RELATION") || k == "E-FACT-DOMAIN"), "{c:?}");
}

#[test]
fn inline_redeclaring_imported_relation_needs_matching_sig() {
    // inline vs imported uses the SAME full-decl comparison as extends (Task 6 D5)
    use std::path::Path;
    fn write(dir: &Path, name: &str, body: &str) { std::fs::write(dir.join(name), body).unwrap(); }
    fn zero_span() -> lute_core_span::Span {
        lute_core_span::Span { byte_start: 0, byte_end: 0, line: 1, column: 1, utf16_range: (0, 0) }
    }
    let dir = std::env::temp_dir().join(format!("lute_rs_inline_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    write(&dir, "s.yaml", "entities:\n  c: { members: [x] }\nrelations:\n  r: { args: [c], tier: run }\n");
    let imports = lute_check::resolve_imports(&dir, &["s.yaml".into()], &[], zero_span());
    let input = lute_check::CheckInput {
        text: scene_with("relations:\n  r: { args: [c], tier: user }\n"),
        uri: "t".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: lute_manifest::provider::ProviderSet::default(),
        mode: lute_check::Mode::Author,
        imports,
        components: Default::default(),
    };
    let c: Vec<String> = lute_check::check(&input).diagnostics.into_iter().map(|d| d.code).collect();
    assert!(c.contains(&"E-EXTENDS-RELATION-SIG".to_string()), "{c:?}");
}
