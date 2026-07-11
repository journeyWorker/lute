//! `holds`/`count`/`validAt`/`now()` CEL profile admission + vocabulary-aware
//! fact-query validation (dsl 0.3.0 §6/§8, Task 11). Mirrors
//! `tests/fact_write.rs`'s harness helpers.

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

const VOCAB: &str = "entities:\n  c: { members: [ana, bo] }\n  loc: { members: [grove] }\nrelations:\n  inParty: { args: [c] }\n  atLoc: { args: [c, loc], key: [0] }\n  buddy: { args: [c, c], derive: true }\n  gated: { args: [c], derive: true }\nstate:\n  run.act: { type: number, default: 1 }\nrules:\n  - \"buddy(A, B) :- inParty(A), inParty(B), A != B\"\n  - \"gated(X) :- inParty(X), cel(\\\"run.act == 1\\\")\"\n";

fn scene_when(cond: &str) -> String {
    // A second, unguarded `<choice>` keeps the branch out of the unrelated
    // `E-BRANCH-ALL-GUARDED` diagnostic (dsl §11.1, S5: every choice in a
    // non-empty branch carrying a `when` guard, with no unguarded fallback,
    // is itself flagged) — orthogonal to this file's fact-query profile
    // checks on the FIRST choice's `when` slot.
    format!(
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n{VOCAB}---\n## Shot 1.\n<branch>\n<choice id=\"a\" label=\"a\" when=\"{cond}\">\n@narrator: hi\n</choice>\n<choice id=\"b\" label=\"b\">\n@narrator: bye\n</choice>\n</branch>\n",
    )
}

#[test]
fn holds_count_validat_now_are_in_profile() {
    for cond in [
        "holds(inParty(ana))",
        "holds(atLoc(ana, _))",
        "holds(atLoc(_, _))",
        "count(buddy(ana, _)) + 1 <= 3",
        "count(inParty(_)) > 0 && run.act == 1",
        "validAt(inParty(ana), now())",
        "validAt(buddy(ana, _), now())",
    ] {
        let c = codes(&scene_when(cond));
        assert!(!c.iter().any(|k| k.starts_with("E-")), "{cond}: {c:?}");
    }
}

#[test]
fn bare_relation_reference_stays_out_of_profile() {
    let c = codes(&scene_when("inParty(ana)"));
    assert!(c.contains(&"E-CEL-PROFILE".to_string()), "bare relation call (§8): {c:?}");
}

#[test]
fn malformed_query_shapes_are_cel_profile() {
    for cond in ["holds()", "holds(run.act)", "holds(inParty(ana), 2)", "now(1) == now(1)"] {
        let c = codes(&scene_when(cond));
        assert!(c.contains(&"E-CEL-PROFILE".to_string()), "{cond}: {c:?}");
    }
}

#[test]
fn query_pattern_closure_checks() {
    let c = codes(&scene_when("holds(ghost(ana))"));
    assert!(c.contains(&"E-RELATION-UNKNOWN".to_string()), "{c:?}");
    let c = codes(&scene_when("holds(inParty(ana, bo))"));
    assert!(c.contains(&"E-RELATION-ARITY".to_string()), "{c:?}");
    let c = codes(&scene_when("holds(inParty(grove))"));
    assert!(c.contains(&"E-FACT-DOMAIN".to_string()), "{c:?}");
    let c = codes(&scene_when("holds(inParty(run.act))"));
    assert!(c.contains(&"E-CEL-PROFILE".to_string()), "non-ground pattern arg: {c:?}");
    let c = codes(&scene_when("holds(c(ana))"));
    assert!(c.contains(&"E-RELATION-UNKNOWN".to_string()), "kind is not queryable: {c:?}");
}

#[test]
fn validat_over_guarded_derived_is_flagged() {
    let c = codes(&scene_when("validAt(gated(ana), now())"));
    assert!(c.contains(&"E-VALIDAT-DERIVED".to_string()), "{c:?}");
    let c = codes(&scene_when("holds(gated(ana))"));
    assert!(!c.contains(&"E-VALIDAT-DERIVED".to_string()), "holds is fine on guarded (§6): {c:?}");
}

#[test]
fn match_subject_may_not_be_a_relation_query() {
    let doc = format!(
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n{VOCAB}---\n## Shot 1.\n<match on=\"holds(inParty(ana))\">\n<when test=\"$\">\n@narrator: hi\n</when>\n<otherwise>\n@narrator: bye\n</otherwise>\n</match>\n"
    );
    let c = codes(&doc);
    assert!(c.contains(&"E-MATCH-RELATION-SUBJECT".to_string()), "{c:?}");
}

#[test]
fn quest_lifecycle_guards_admit_fact_queries() {
    let quest = format!(
        "---\nkind: quest\n{VOCAB}---\n<quest id=\"q\" title=\"t\" start=\"holds(inParty(ana))\" fail=\"holds(atLoc(ana, grove))\">\n<objective id=\"o\" title=\"t\" done=\"count(buddy(ana, _)) >= 1\"/>\n</quest>\n"
    );
    let c = codes(&quest);
    assert!(!c.iter().any(|k| k.starts_with("E-")), "§11: {c:?}");
}

/// 0.3.0 T11 fix (soundness gap, f06c650): a relation arg MAY be typed
/// against a plugin/core/project merged *domain* — `build_rel_vocab`
/// (rel_schema.rs) accepts an arg name via `domains.contains_key(arg)`, not
/// only a RelVocab entity kind or `enums:` name. `check_atom`'s membership
/// check for such an arg (rel_schema.rs `domains.get(dname)`) is reachable
/// exactly the same way for a seeded `facts:`, an `::assert`/`::retract`
/// write, AND a `holds`/`count`/`validAt` query pattern — but
/// `check_fact_queries` used to hand `check_atom` an EMPTY domains map, so a
/// bad domain-member value in a query silently passed. `felt`'s sole arg
/// `emotion` is `lute.core`'s baseline domain (`snap.domains["emotion"]`,
/// closed, members `[neutral, surprised, delighted, shy, content, angry,
/// sad]`) — reachable with NO `entities:`/`enums:` declared at all, proving
/// the arg resolves via the merged `domains` map, not a RelVocab kind/enum.
const DOMAIN_VOCAB: &str = "relations:\n  felt: { args: [emotion] }\n";

fn scene_when_domain(cond: &str) -> String {
    format!(
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n{DOMAIN_VOCAB}---\n## Shot 1.\n<branch>\n<choice id=\"a\" label=\"a\" when=\"{cond}\">\n@narrator: hi\n</choice>\n<choice id=\"b\" label=\"b\">\n@narrator: bye\n</choice>\n</branch>\n",
    )
}

#[test]
fn query_pattern_domain_typed_arg_is_membership_checked() {
    // Non-member of the `emotion` domain in a query pattern must be caught —
    // exactly like a bad member already is for facts:/::assert/::retract.
    let c = codes(&scene_when_domain("holds(felt(zzz))"));
    assert!(
        c.contains(&"E-FACT-DOMAIN".to_string()),
        "domain-typed relation arg in a query pattern must be membership-checked \
         against the merged domains map (0.3.0 T11 gap): {c:?}"
    );
    let c = codes(&scene_when_domain("count(felt(zzz)) > 0"));
    assert!(c.contains(&"E-FACT-DOMAIN".to_string()), "count(): {c:?}");
    let c = codes(&scene_when_domain("validAt(felt(zzz), now())"));
    assert!(c.contains(&"E-FACT-DOMAIN".to_string()), "validAt(): {c:?}");

    // A genuine domain member checks clean.
    let c = codes(&scene_when_domain("holds(felt(neutral))"));
    assert!(
        !c.iter().any(|k| k.starts_with("E-")),
        "valid domain member must check clean: {c:?}"
    );
}
