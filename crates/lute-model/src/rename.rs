use std::collections::{BTreeMap, BTreeSet};

use lute_core_span::Span;
use lute_manifest::project::{IdentityRename, IdentityRenameDecl};

use crate::graph::{NodeKey, NodeKind, SemanticGraph};

/// A resolved, deterministic project identity migration sequence.
pub type ResolvedRenames = Vec<IdentityRename>;

/// One authored-ledger validation failure, retaining its manifest location.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenameError {
    pub code: &'static str,
    pub message: String,
    pub span: Option<Span>,
}

fn kind(name: &str) -> Option<NodeKind> {
    Some(match name {
        "project" => NodeKind::Project,
        "document" => NodeKind::Document,
        "scene" => NodeKind::Scene,
        "beat" => NodeKind::Beat,
        "shot" => NodeKind::Shot,
        "line" => NodeKind::Line,
        "choice" => NodeKind::Choice,
        "quest" => NodeKind::Quest,
        "objective" => NodeKind::Objective,
        "reward" => NodeKind::Reward,
        "entry" => NodeKind::Entry,
        "occasion" => NodeKind::Occasion,
        "relation" => NodeKind::Relation,
        "state" => NodeKind::State,
        "def" => NodeKind::Def,
        "component" => NodeKind::Component,
        "expanded" => NodeKind::Expanded,
        "fact" => NodeKind::Fact,
        "clock" => NodeKind::Clock,
        "engine" => NodeKind::Engine,
        _ => return None,
    })
}

fn parse_key(raw: &str) -> Option<NodeKey> {
    let (kind_name, key) = raw.split_once(':')?;
    if key.is_empty() {
        return None;
    }
    let parsed = NodeKey::new(kind(kind_name)?, key);
    (parsed.canonical() == raw).then_some(parsed)
}

fn error(code: &'static str, message: impl Into<String>, span: Option<Span>) -> RenameError {
    RenameError { code, message: message.into(), span }
}

/// Validate and resolve the manifest ledger after the project graph exists.
/// The output is sorted by source key and contains no source/destination aliases.
pub fn resolve_ledger(
    entries: &[IdentityRenameDecl],
    graph: &SemanticGraph,
) -> Result<ResolvedRenames, Vec<RenameError>> {
    let mut errors = Vec::new();
    let mut parsed = Vec::with_capacity(entries.len());
    let mut sources = BTreeMap::<String, usize>::new();
    let mut destinations = BTreeMap::<String, usize>::new();

    for (index, entry) in entries.iter().enumerate() {
        let Some(from) = parse_key(&entry.rename.from) else {
            errors.push(error(
                "E-RENAME-LEDGER",
                format!("rename source `{}` is not a canonical NodeKey", entry.rename.from),
                entry.span,
            ));
            continue;
        };
        let Some(to) = parse_key(&entry.rename.to) else {
            errors.push(error(
                "E-RENAME-LEDGER",
                format!("rename destination `{}` is not a canonical NodeKey", entry.rename.to),
                entry.span,
            ));
            continue;
        };
        if let Some(previous) = sources.insert(entry.rename.from.clone(), index) {
            errors.push(error(
                "E-RENAME-LEDGER",
                format!("rename source `{}` is declared more than once (entries {previous} and {index})", entry.rename.from),
                entry.span,
            ));
        }
        if let Some(previous) = destinations.insert(entry.rename.to.clone(), index) {
            errors.push(error(
                "E-RENAME-LEDGER",
                format!("rename destination `{}` is declared more than once (entries {previous} and {index})", entry.rename.to),
                entry.span,
            ));
        }
        parsed.push((entry.rename.clone(), from, to, entry.span));
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    let source_keys: BTreeSet<_> = parsed.iter().map(|(_, from, _, _)| from).collect();
    let destination_keys: BTreeSet<_> = parsed.iter().map(|(_, _, to, _)| to).collect();
    for (rename, from, to, span) in &parsed {
        if source_keys.contains(to) || from == to {
            errors.push(error(
                "E-RENAME-LEDGER-CYCLE",
                format!("rename `{}` → `{}` forms a chain, self-loop, or cycle", rename.from, rename.to),
                *span,
            ));
        }
        if graph.nodes.contains_key(from) || !graph.nodes.contains_key(to) {
            let mut reason = Vec::new();
            if graph.nodes.contains_key(from) {
                reason.push("source is still present");
            }
            if !graph.nodes.contains_key(to) {
                reason.push("destination is absent");
            }
            errors.push(error(
                "E-RENAME-LEDGER-STALE",
                format!("rename `{}` → `{}` is stale: {}", rename.from, rename.to, reason.join(" and ")),
                *span,
            ));
        }
    }
    // Keep this explicit: it documents the endpoint invariant and protects
    // against future changes that separate cycle detection from overlap.
    let _ = destination_keys;
    if !errors.is_empty() {
        return Err(errors);
    }

    parsed.sort_by(|a, b| a.0.from.cmp(&b.0.from));
    Ok(parsed.into_iter().map(|(rename, _, _, _)| rename).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_parser_keeps_colons_opaque_after_kind() {
        let key = parse_key("fact:owns(key:blue)").expect("canonical key");
        assert_eq!(key.kind, NodeKind::Fact);
        assert_eq!(key.key, "owns(key:blue)");
        assert!(parse_key("quest").is_none());
        assert!(parse_key("unknown:x").is_none());
    }
    #[test]
    fn resolves_sorted_entries_and_rejects_stale_endpoints() {
        let mut graph = SemanticGraph::default();
        graph.node(NodeKey::new(NodeKind::Quest, "new"), None, None);
        let entries = vec![
            IdentityRenameDecl {
                rename: IdentityRename { from: "quest:z".into(), to: "quest:new".into() },
                span: None,
            },
            IdentityRenameDecl {
                rename: IdentityRename { from: "quest:a".into(), to: "quest:missing".into() },
                span: None,
            },
        ];
        let errors = resolve_ledger(&entries, &graph).expect_err("stale endpoint");
        assert!(errors.iter().any(|e| e.code == "E-RENAME-LEDGER-STALE"));

        let mut graph = SemanticGraph::default();
        graph.node(NodeKey::new(NodeKind::Quest, "new-a"), None, None);
        graph.node(NodeKey::new(NodeKind::Quest, "new-z"), None, None);
        let entries = vec![
            IdentityRenameDecl {
                rename: IdentityRename { from: "quest:z".into(), to: "quest:new-z".into() },
                span: None,
            },
            IdentityRenameDecl {
                rename: IdentityRename { from: "quest:a".into(), to: "quest:new-a".into() },
                span: None,
            },
        ];
        let resolved = resolve_ledger(&entries, &graph).expect("valid ledger");
        assert_eq!(resolved[0].from, "quest:a");
        assert_eq!(resolved[1].from, "quest:z");
    }

    #[test]
    fn rejects_chains_and_cycles() {
        let mut graph = SemanticGraph::default();
        graph.node(NodeKey::new(NodeKind::Quest, "c"), None, None);
        let entries = vec![
            IdentityRenameDecl {
                rename: IdentityRename { from: "quest:a".into(), to: "quest:b".into() },
                span: None,
            },
            IdentityRenameDecl {
                rename: IdentityRename { from: "quest:b".into(), to: "quest:c".into() },
                span: None,
            },
        ];
        let errors = resolve_ledger(&entries, &graph).expect_err("chain");
        assert!(errors.iter().any(|e| e.code == "E-RENAME-LEDGER-CYCLE"));
    }
}
