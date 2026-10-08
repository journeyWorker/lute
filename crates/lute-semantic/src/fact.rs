//! Stable graph keys and overlap primitives for Datalog facts.

use lute_core_span::Evidence;
use lute_manifest::fact::FactTerm;

use crate::{NodeKey, NodeKind};

/// Build the canonical graph key for a fact pattern.
pub fn fact_node(relation: &str, args: &[FactTerm]) -> NodeKey {
    NodeKey::new(NodeKind::Fact, format_fact(relation, args))
}

/// Render a fact pattern using the graph's stable argument spelling.
pub fn format_fact(relation: &str, args: &[FactTerm]) -> String {
    let rendered = args
        .iter()
        .map(|arg| match arg {
            FactTerm::Ident(value) => value.clone(),
            FactTerm::Bool(value) => value.to_string(),
            FactTerm::Wildcard => "_".to_string(),
            FactTerm::Param(value) => format!("@{value}"),
            FactTerm::Target => "occasion.target".to_string(),
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("{relation}({rendered})")
}

/// Determine whether two fact patterns overlap and classify the evidence.
pub fn fact_overlap(a: &str, b: &str) -> Option<Evidence> {
    let pa = lute_manifest::fact::parse_fact(a).ok()?;
    let pb = lute_manifest::fact::parse_fact(b).ok()?;
    if pa.relation != pb.relation || pa.args.len() != pb.args.len() {
        return None;
    }
    let mut heuristic = false;
    for (left, right) in pa.args.iter().zip(pb.args.iter()) {
        match (&left.term, &right.term) {
            (FactTerm::Ident(a), FactTerm::Ident(b)) if a != b => return None,
            (FactTerm::Bool(a), FactTerm::Bool(b)) if a != b => return None,
            (FactTerm::Param(_), _) | (_, FactTerm::Param(_)) => heuristic = true,
            _ => {}
        }
    }
    Some(if heuristic {
        Evidence::Heuristic
    } else {
        Evidence::Proven
    })
}
