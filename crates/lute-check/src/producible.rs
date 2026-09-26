//! `producible(R)`: the relation-level structural rule-dependency walk (dsl
//! 0.4.0 §4.2/§B) — "can ANY fact of `R` ever exist". It backs `lute trace`'s
//! `W-TRACE-MOCK-UNPRODUCIBLE` note and `lute scenario`'s facts section. The
//! argument-level successor that decides guards (dsl 0.20.0 §3) is
//! [`crate::fact_env::MaySet`].
//!
//! D1 applies throughout, but INVERTED from `rel_schema.rs`'s usual
//! quarantine note: `producible()` is a boolean SATISFIABILITY walk over
//! declared rule STRUCTURE — it never runs the real Datalog fixpoint or
//! evaluates facts against runtime state, so it sits entirely OUTSIDE the
//! D1 quarantine (spec §4.2).
//!
//! ## Naive approach is unsound (why this isn't a plain assert-site search)
//! A `derive: true` relation can never be `::assert`ed at all
//! (`E-DERIVED-WRITE`), so tracing `::assert` sites for a relation DIRECTLY
//! finds an empty set for every derived relation by construction. The walk
//! instead follows the RULE-DEPENDENCY graph down to base relations
//! (reusing `datalog_check.rs`'s `predicate_edges` extraction pattern for
//! rule/atom structure, though not its Tarjan pass — this is a monotone
//! least-fixpoint, not an SCC search).
use std::collections::BTreeMap;
use std::collections::BTreeSet;

use lute_syntax::datalog::BodyLiteral;

use crate::rel_schema::RelVocab;

/// Boolean least-fixpoint over the declared rule DAG (spec §4.2): iterating
/// to a fixed point over `vocab.rules`/`vocab.relations`/`vocab.facts` —
/// finite and terminating by the same finite-Herbrand-base argument the real
/// Datalog fixpoint relies on, just cheaper (boolean domain, not fact sets).
///
/// A **base** (`derive: false`) relation `R` is producible iff (a) it has a
/// `facts:` seed (`vocab.facts`, unconditional), OR (b) `R.reserved == true`
/// (engine-populated out-of-band — no author-side producer is NOT a sound
/// impossibility signal), OR (c) `live_assert_relations` names it — the
/// caller's reachability-GATED `::assert{R(…)}` base case: every relation
/// with an assert site inside a node the project's T6 reachability pass did
/// NOT prove `Reachability::Unreachable` (see
/// [`crate::connectivity::live_assert_relations`], which computes this set;
/// `Reachable` AND `Unknown` both count — provable-only, only a PROVEN
/// `Unreachable` excludes).
///
/// A **derived** (`derive: true`) relation `R` is producible iff ANY rule
/// clause `R(...) :- B1,…,Bn` has EVERY POSITIVE (`BodyLiteral::Pos`) atom's
/// relation producible. `BodyLiteral::Neg`/`Guard{cel}`/`Cmp` are
/// conservatively treated as ALWAYS-satisfiable (0.3.0 §7.3) — provable-only,
/// never guess: they can never make a clause LESS satisfiable, so they never
/// cause a false-positive "unreachable" claim.
///
/// A rule head naming an undeclared relation or a non-`derive` relation is
/// already `E-DERIVE-UNDECLARED`'s problem (`datalog_check.rs`) — silently
/// skipped here, contributing nothing (never fabricates a value for a
/// relation this walk cannot make sense of).
pub fn producible(
    vocab: &RelVocab,
    live_assert_relations: &BTreeSet<String>,
) -> BTreeMap<String, bool> {
    let seeded: BTreeSet<&str> = vocab
        .facts
        .iter()
        .map(|f| f.fact.relation.as_str())
        .collect();
    let mut result: BTreeMap<String, bool> = BTreeMap::new();
    for (name, decl) in &vocab.relations {
        let value = if decl.derive {
            false // filled by the fixpoint below, monotone false -> true only.
        } else {
            seeded.contains(name.as_str()) || decl.reserved || live_assert_relations.contains(name)
        };
        result.insert(name.clone(), value);
    }
    loop {
        let mut changed = false;
        for rule_decl in &vocab.rules {
            let head = &rule_decl.rule.head.relation;
            let Some(decl) = vocab.relations.get(head) else {
                continue;
            };
            if !decl.derive {
                continue;
            }
            if result.get(head).copied().unwrap_or(false) {
                continue; // already producible; no clause can un-prove it.
            }
            let clause_satisfiable = rule_decl.rule.body.iter().all(|lit| match lit {
                BodyLiteral::Pos(atom) => {
                    if vocab.relations.contains_key(&atom.relation) {
                        result.get(&atom.relation).copied().unwrap_or(false)
                    } else {
                        // An entity-kind atom (`K(X)`) or an atom naming an
                        // undeclared relation — `predicate_edges` (spec §7.2,
                        // `datalog_check.rs`) deliberately excludes both from
                        // the rule-dependency graph. A kind may have runtime
                        // members with no author-side "producer" signal, and
                        // an undeclared predicate is already diagnosed
                        // elsewhere (`E-DERIVE-UNDECLARED` et al.) — neither
                        // is a sound impossibility signal here. Conservatively
                        // satisfiable, same discipline as `Neg`/`Guard`/`Cmp`
                        // below: never make a clause LESS satisfiable, never
                        // a false-positive "unreachable" claim.
                        true
                    }
                }
                BodyLiteral::Neg(_)
                | BodyLiteral::Guard { .. }
                | BodyLiteral::Cmp { .. }
                | BodyLiteral::Count { .. } => true,
            });
            if clause_satisfiable {
                result.insert(head.clone(), true);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use lute_core_span::Span;
    use lute_manifest::relations::RelationDecl;

    fn base_relation(reserved: bool) -> RelationDecl {
        RelationDecl {
            args: vec!["c".to_string()],
            tier: Some("run".to_string()),
            derive: false,
            reserved,
            ..Default::default()
        }
    }

    fn derive_relation() -> RelationDecl {
        RelationDecl {
            args: vec!["c".to_string()],
            tier: None,
            derive: true,
            reserved: false,
            ..Default::default()
        }
    }

    fn dummy_span() -> Span {
        Span {
            byte_start: 0,
            byte_end: 0,
            line: 1,
            column: 1,
            utf16_range: (0, 0),
        }
    }

    fn fact(relation: &str) -> crate::meta::FactDecl {
        let pattern = lute_syntax::datalog::parse_fact(&format!("{relation}(a)")).unwrap();
        crate::meta::FactDecl {
            fact: pattern,
            raw: format!("{relation}(a)"),
            span: dummy_span(),
        }
    }

    fn rule(text: &str) -> crate::meta::RuleDecl {
        let rule = lute_syntax::datalog::parse_rule(text).unwrap();
        crate::meta::RuleDecl {
            rule,
            raw: text.to_string(),
            span: dummy_span(),
        }
    }

    #[test]
    fn base_relation_with_facts_seed_is_producible() {
        let mut vocab = RelVocab::default();
        vocab
            .relations
            .insert("seeded".to_string(), base_relation(false));
        vocab.facts.push(fact("seeded"));
        let result = producible(&vocab, &BTreeSet::new());
        assert_eq!(result.get("seeded"), Some(&true));
    }

    #[test]
    fn base_relation_reserved_is_producible_without_facts() {
        let mut vocab = RelVocab::default();
        vocab
            .relations
            .insert("engineOwned".to_string(), base_relation(true));
        let result = producible(&vocab, &BTreeSet::new());
        assert_eq!(result.get("engineOwned"), Some(&true));
    }

    #[test]
    fn base_relation_with_live_assert_is_producible() {
        let mut vocab = RelVocab::default();
        vocab
            .relations
            .insert("asserted".to_string(), base_relation(false));
        let live: BTreeSet<String> = ["asserted".to_string()].into_iter().collect();
        let result = producible(&vocab, &live);
        assert_eq!(result.get("asserted"), Some(&true));
    }

    #[test]
    fn base_relation_with_no_producer_is_not_producible() {
        let mut vocab = RelVocab::default();
        vocab
            .relations
            .insert("orphan".to_string(), base_relation(false));
        let result = producible(&vocab, &BTreeSet::new());
        assert_eq!(result.get("orphan"), Some(&false));
    }

    #[test]
    fn derived_relation_producible_via_seeded_base() {
        let mut vocab = RelVocab::default();
        vocab
            .relations
            .insert("base".to_string(), base_relation(false));
        vocab
            .relations
            .insert("derived".to_string(), derive_relation());
        vocab.facts.push(fact("base"));
        vocab.rules.push(rule("derived(X) :- base(X)"));
        let result = producible(&vocab, &BTreeSet::new());
        assert_eq!(result.get("derived"), Some(&true));
    }

    #[test]
    fn derived_relation_never_producible_when_base_has_no_producer() {
        let mut vocab = RelVocab::default();
        vocab
            .relations
            .insert("base".to_string(), base_relation(false));
        vocab
            .relations
            .insert("derived".to_string(), derive_relation());
        vocab.rules.push(rule("derived(X) :- base(X)"));
        let result = producible(&vocab, &BTreeSet::new());
        assert_eq!(result.get("derived"), Some(&false));
    }

    #[test]
    fn negated_body_atom_never_blocks_producibility() {
        // `derived(X) :- notSeeded(X), not neverSeeded(X)` -- the `Neg` atom
        // is always-satisfiable, so `derived` is producible purely off
        // `notSeeded`'s facts seed, regardless of `neverSeeded`'s state.
        let mut vocab = RelVocab::default();
        vocab
            .relations
            .insert("notSeeded".to_string(), base_relation(false));
        vocab
            .relations
            .insert("neverSeeded".to_string(), base_relation(false));
        vocab
            .relations
            .insert("derived".to_string(), derive_relation());
        vocab.facts.push(fact("notSeeded"));
        vocab
            .rules
            .push(rule("derived(X) :- notSeeded(X), not neverSeeded(X)"));
        let result = producible(&vocab, &BTreeSet::new());
        assert_eq!(result.get("derived"), Some(&true));
    }

    #[test]
    fn guard_only_body_never_blocks_producibility() {
        // `derived(X) :- cel("true")` -- no positive atom at all; vacuously
        // satisfiable (the spec's "every positive atom producible" holds
        // trivially over an empty positive set).
        let mut vocab = RelVocab::default();
        vocab
            .relations
            .insert("derived".to_string(), derive_relation());
        vocab.rules.push(rule("derived(X) :- cel(\"true\")"));
        let result = producible(&vocab, &BTreeSet::new());
        assert_eq!(result.get("derived"), Some(&true));
    }

    #[test]
    fn recursive_derived_relation_reaches_fixpoint() {
        // `path(X,Y) :- edge(X,Y)`; `path(X,Y) :- path(X,Z), edge(Z,Y)` --
        // self-referencing, must not infinite-loop; terminates true off the
        // base case.
        let mut vocab = RelVocab::default();
        vocab
            .relations
            .insert("edge".to_string(), base_relation(false));
        vocab
            .relations
            .insert("path".to_string(), derive_relation());
        vocab.facts.push(fact("edge"));
        vocab.rules.push(rule("path(X, Y) :- edge(X, Y)"));
        vocab
            .rules
            .push(rule("path(X, Y) :- path(X, Z), edge(Z, Y)"));
        let result = producible(&vocab, &BTreeSet::new());
        assert_eq!(result.get("path"), Some(&true));
    }
}
