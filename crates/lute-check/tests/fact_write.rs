//! `::assert`/`::retract` write policy + pattern validation (dsl 0.3.0 §5,
//! Task 10). Mirrors `tests/rel_schema.rs`'s harness helpers.

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

const VOCAB: &str = "entities:\n  c: { members: [ana, bo] }\n  f: { members: [reds] }\nenums:\n  trust: [low, high]\nrelations:\n  inParty: { args: [c] }\n  topo: { args: [c, c], tier: app }\n  vibe: { args: [c, trust], derive: true }\n  sensed: { args: [c], reserved: true }\nrules:\n  - \"vibe(X, low) :- inParty(X)\"\n";

fn scene_body(body: &str) -> String {
    format!("---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n{VOCAB}---\n## Shot 1.\n{body}\n")
}

#[test]
fn derived_write_precedes_everything() {
    let c = codes(&scene_body("::assert{ vibe(ana, low) }"));
    assert!(c.contains(&"E-DERIVED-WRITE".to_string()), "{c:?}");
    let c = codes(&scene_body("::retract{ vibe(ana, _) }"));
    assert!(c.contains(&"E-DERIVED-WRITE".to_string()), "{c:?}");
}

#[test]
fn reserved_write_is_relation_reserved_write() {
    let c = codes(&scene_body("::assert{ sensed(ana) }"));
    assert!(c.contains(&"E-RELATION-RESERVED-WRITE".to_string()), "{c:?}");
}

#[test]
fn app_tier_write_is_fact_tier_write() {
    let c = codes(&scene_body("::retract{ topo(ana, bo) }"));
    assert!(c.contains(&"E-FACT-TIER-WRITE".to_string()), "{c:?}");
}

#[test]
fn wildcard_in_assert_is_flagged_retract_ok() {
    let c = codes(&scene_body("::assert{ inParty(_) }"));
    assert!(c.contains(&"E-RETRACT-WILDCARD-ASSERT".to_string()), "{c:?}");
    let c = codes(&scene_body("::retract{ inParty(_) }"));
    assert!(!c.iter().any(|k| k.starts_with("E-")), "{c:?}");
}

#[test]
fn unknown_arity_domain_mirror_assert_and_retract() {
    let c = codes(&scene_body("::assert{ ghost(ana) }"));
    assert!(c.contains(&"E-RELATION-UNKNOWN".to_string()), "{c:?}");
    let c = codes(&scene_body("::assert{ inParty(ana, bo) }"));
    assert!(c.contains(&"E-RELATION-ARITY".to_string()), "{c:?}");
    let c = codes(&scene_body("::retract{ inParty(reds) }"));
    assert!(c.contains(&"E-FACT-DOMAIN".to_string()), "reds is an f, not a c (§3.1): {c:?}");
}

#[test]
fn writable_base_relation_is_clean_and_quest_arms_admit_writes() {
    let c = codes(&scene_body("::assert{ inParty(ana) }"));
    assert!(!c.iter().any(|k| k.starts_with("E-")), "{c:?}");
    let quest = format!(
        "---\nkind: quest\n{VOCAB}---\n<quest id=\"q\" title=\"t\" start=\"true\">\n<on event=\"questComplete\">\n::assert{{ inParty(bo) }}\n</on>\n</quest>\n"
    );
    let c = codes(&quest);
    assert!(!c.iter().any(|k| k.starts_with("E-")), "{c:?}");
}

#[test]
fn derived_and_app_tier_precedence_reports_derived_write_first() {
    // A relation declared BOTH `derive: true` and `tier: app` also gets
    // `E-DERIVE-TIER` at the decl (Task 7's own conflict check); the
    // write-policy PRECEDENCE here (§5 policy 1 before policy 3) must still
    // report `E-DERIVED-WRITE` for an attempted write, never
    // `E-FACT-TIER-WRITE` — `tier_of` returns `None` for `derive: true`
    // regardless of a declared `tier:`.
    let front = "entities:\n  c: { members: [ana] }\nrelations:\n  derivedApp: { args: [c], derive: true, tier: app }\n";
    let c = codes(&format!(
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n{front}---\n## Shot 1.\n::assert{{ derivedApp(ana) }}\n"
    ));
    assert!(c.contains(&"E-DERIVED-WRITE".to_string()), "{c:?}");
    assert!(!c.contains(&"E-FACT-TIER-WRITE".to_string()), "{c:?}");
}
