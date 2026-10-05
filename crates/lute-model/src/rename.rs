use std::collections::{BTreeMap, BTreeSet};

use lute_core_span::Span;
use lute_manifest::project::{IdentityRename, IdentityRenameDecl};

use lute_semantic::{NodeKey, NodeKind, RenameError, SemanticGraph};

/// A resolved, deterministic project identity migration sequence.
pub type ResolvedRenames = Vec<IdentityRename>;


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
    RenameError {
        code,
        message: message.into(),
        span,
    }
}

fn descendants(graph: &SemanticGraph, root: &NodeKey) -> Vec<NodeKey> {
    let mut out = Vec::new();
    let mut queue = vec![root.clone()];
    let mut seen = BTreeSet::new();
    while let Some(current) = queue.pop() {
        for edge in &graph.edges {
            if !(edge.kind == "contains"
                || (edge.kind == "writes" && edge.target.kind == NodeKind::State))
                || edge.source != current
                || !seen.insert(edge.target.clone())
            {
                continue;
            }
            out.push(edge.target.clone());
            queue.push(edge.target.clone());
        }
    }
    out.sort();
    out
}
/// Rewrite an exact ownership token. Separators are part of the graph key
/// constructors (`.`, `#`, `:`); a raw substring replacement would migrate
/// unrelated identities such as `oldTown`.
fn rewrite_descendant(from: &NodeKey, to: &NodeKey, descendant: &NodeKey) -> Option<NodeKey> {
    if from.kind != to.kind {
        return None;
    }
    let key = &descendant.key;
    let mut position = None;
    for (start, _) in key.match_indices(&to.key) {
        let before = key[..start].chars().next_back();
        let end = start + to.key.len();
        let after = key[end..].chars().next();
        let boundary = |c: Option<char>| c.is_none_or(|c| matches!(c, '.' | '#' | ':'));
        if boundary(before) && boundary(after) {
            position = Some((start, end));
            break;
        }
    }
    let (start, end) = position?;
    let mut rewritten =
        String::with_capacity(key.len() + from.key.len().saturating_sub(to.key.len()));
    rewritten.push_str(&key[..start]);
    rewritten.push_str(&from.key);
    rewritten.push_str(&key[end..]);
    (rewritten != *key).then(|| NodeKey::new(descendant.kind, rewritten))
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
                format!(
                    "rename source `{}` is not a canonical NodeKey",
                    entry.rename.from
                ),
                entry.span,
            ));
            continue;
        };
        let Some(to) = parse_key(&entry.rename.to) else {
            errors.push(error(
                "E-RENAME-LEDGER",
                format!(
                    "rename destination `{}` is not a canonical NodeKey",
                    entry.rename.to
                ),
                entry.span,
            ));
            continue;
        };
        if let Some(previous) = sources.insert(entry.rename.from.clone(), index) {
            errors.push(error(
                "E-RENAME-LEDGER",
                format!(
                    "rename source `{}` is declared more than once (entries {previous} and {index})",
                    entry.rename.from
                ),
                entry.span,
            ));
        }
        if let Some(previous) = destinations.insert(entry.rename.to.clone(), index) {
            errors.push(error(
                "E-RENAME-LEDGER",
                format!(
                    "rename destination `{}` is declared more than once (entries {previous} and {index})",
                    entry.rename.to
                ),
                entry.span,
            ));
        }
        parsed.push((entry.rename.clone(), from, to, entry.span));
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    let source_keys: BTreeSet<_> = parsed.iter().map(|(_, from, _, _)| from).collect();
    for (rename, from, to, span) in &parsed {
        if source_keys.contains(to) || from == to {
            errors.push(error(
                "E-RENAME-LEDGER-CYCLE",
                format!(
                    "rename `{}` → `{}` forms a chain, self-loop, or cycle",
                    rename.from, rename.to
                ),
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
                format!(
                    "rename `{}` → `{}` is stale: {}",
                    rename.from,
                    rename.to,
                    reason.join(" and ")
                ),
                *span,
            ));
        }
    }
    // Expand each authored mapping over the destination subtree. The graph's
    // contains edges, rather than key prefixes, define ownership.
    let mut expanded = Vec::<(IdentityRename, Option<Span>)>::new();
    let mut covered_sources = BTreeMap::<String, Option<Span>>::new();
    for (rename, from, to, span) in &parsed {
        expanded.push((rename.clone(), *span));
        for descendant in descendants(graph, to) {
            let Some(source) = rewrite_descendant(from, to, &descendant) else {
                errors.push(error(
                    "E-RENAME-LEDGER-STALE",
                    format!(
                        "cannot expand `{}` → `{}` to contained descendant `{}`",
                        rename.from,
                        rename.to,
                        descendant.canonical()
                    ),
                    *span,
                ));
                continue;
            };
            covered_sources.insert(source.canonical(), *span);
            expanded.push((
                IdentityRename {
                    from: source.canonical(),
                    to: descendant.canonical(),
                },
                *span,
            ));
        }
    }
    for (rename, _, _, span) in &parsed {
        if covered_sources.contains_key(&rename.from) {
            errors.push(error(
                "E-RENAME-LEDGER",
                format!(
                    "rename `{}` is already covered by an ancestor expansion",
                    rename.from
                ),
                *span,
            ));
        }
    }
    let mut expanded_sources = BTreeMap::<String, Option<Span>>::new();
    let mut expanded_destinations = BTreeMap::<String, Option<Span>>::new();
    for (rename, span) in &expanded {
        let Some(from) = parse_key(&rename.from) else {
            continue;
        };
        let Some(to) = parse_key(&rename.to) else {
            continue;
        };
        if graph.nodes.contains_key(&from) || !graph.nodes.contains_key(&to) {
            errors.push(error(
                "E-RENAME-LEDGER-STALE",
                format!(
                    "expanded rename `{}` → `{}` is stale",
                    rename.from, rename.to
                ),
                *span,
            ));
        }
        if expanded_sources
            .insert(rename.from.clone(), *span)
            .is_some()
            || expanded_destinations
                .insert(rename.to.clone(), *span)
                .is_some()
        {
            errors.push(error(
                "E-RENAME-LEDGER",
                format!(
                    "expanded rename `{}` → `{}` duplicates an endpoint",
                    rename.from, rename.to
                ),
                *span,
            ));
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    expanded.sort_by(|a, b| a.0.from.cmp(&b.0.from));
    Ok(expanded.into_iter().map(|(rename, _)| rename).collect())
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
                rename: IdentityRename {
                    from: "quest:z".into(),
                    to: "quest:new".into(),
                },
                span: None,
            },
            IdentityRenameDecl {
                rename: IdentityRename {
                    from: "quest:a".into(),
                    to: "quest:missing".into(),
                },
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
                rename: IdentityRename {
                    from: "quest:z".into(),
                    to: "quest:new-z".into(),
                },
                span: None,
            },
            IdentityRenameDecl {
                rename: IdentityRename {
                    from: "quest:a".into(),
                    to: "quest:new-a".into(),
                },
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
                rename: IdentityRename {
                    from: "quest:a".into(),
                    to: "quest:b".into(),
                },
                span: None,
            },
            IdentityRenameDecl {
                rename: IdentityRename {
                    from: "quest:b".into(),
                    to: "quest:c".into(),
                },
                span: None,
            },
        ];
        let errors = resolve_ledger(&entries, &graph).expect_err("chain");
        assert!(errors.iter().any(|e| e.code == "E-RENAME-LEDGER-CYCLE"));
    }

    #[test]
    fn expands_containment_and_rejects_covered_descendant() {
        let mut graph = SemanticGraph::default();
        let quest = NodeKey::new(NodeKind::Quest, "new");
        let objective = NodeKey::new(NodeKind::Objective, "new.reach");
        let state = NodeKey::new(NodeKind::State, "quest.new.state");
        graph.node(quest.clone(), None, None);
        graph.node(objective.clone(), None, None);
        graph.node(state.clone(), None, None);
        graph.edge(
            quest.clone(),
            objective,
            "contains",
            "quest owns objective",
            None,
            None,
            lute_core_span::Evidence::Proven,
        );
        graph.edge(
            quest,
            state,
            "contains",
            "quest owns state",
            None,
            None,
            lute_core_span::Evidence::Proven,
        );
        let root = IdentityRenameDecl {
            rename: IdentityRename {
                from: "quest:old".into(),
                to: "quest:new".into(),
            },
            span: None,
        };
        let expanded =
            resolve_ledger(std::slice::from_ref(&root), &graph).expect("expanded ledger");
        assert_eq!(
            expanded.iter().map(|r| r.from.as_str()).collect::<Vec<_>>(),
            vec!["objective:old.reach", "quest:old", "state:quest.old.state"]
        );
        assert!(expanded.iter().any(|r| r.to == "objective:new.reach"));
        let duplicate = IdentityRenameDecl {
            rename: IdentityRename {
                from: "objective:old.reach".into(),
                to: "objective:new.reach".into(),
            },
            span: None,
        };
        let errors = resolve_ledger(&[root, duplicate], &graph).expect_err("covered descendant");
        assert!(errors.iter().any(|error| error.code == "E-RENAME-LEDGER"));
    }

    #[test]
    fn explicit_stale_descendant_is_reported_on_its_entry() {
        let mut graph = SemanticGraph::default();
        graph.node(NodeKey::new(NodeKind::Objective, "new.reach"), None, None);
        let entry = IdentityRenameDecl {
            rename: IdentityRename {
                from: "objective:old.missing".into(),
                to: "objective:new.missing".into(),
            },
            span: None,
        };
        let errors = resolve_ledger(&[entry], &graph).expect_err("stale descendant");
        assert!(errors
            .iter()
            .any(|error| error.code == "E-RENAME-LEDGER-STALE"));
    }
}
