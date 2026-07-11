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
