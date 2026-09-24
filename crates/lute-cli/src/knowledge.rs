//! `lute scenario <dir> knowledge [--for <node>]` (dsl 0.23.0 §1): the
//! knowledge map of a project. For every fact-guarded beat, entry and
//! objective — one whose condition queries a relation (`holds(…)`,
//! `count(…)`, `validAt(…)`) — the relations it reads, and for each relation
//! who makes it true: the documents that `::assert` it, the seed facts, the
//! engine (a `reserved` relation), or the rules that derive it, traced
//! through each rule's premises to their own producers. A relation nothing
//! produces is said so.
//!
//! Read-only, over the same per-root document collection `lute scenario`
//! and `check-project` use; documents need not check clean.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lute_check::{ProjectBeatKind, RelVocab};
use lute_syntax::ast::{CelSlot, Document, Node};
use lute_syntax::datalog::BodyLiteral;
use serde_json::{json, Map, Value as Json};

use crate::{ByRoot, DocGroup};

/// One fact-guarded element: what it is, where, its conditions (after
/// `@def` expansion) and the atoms they query.
struct Guarded {
    /// ``scene `k` `` / ``entry `id` `` / ``beat `k` `` / ``objective `q.o` ``.
    name: String,
    /// The `--for` handles that select it.
    handles: Vec<String>,
    document: String,
    /// `(slot, condition)`: `when`, `done`, `by`.
    slots: Vec<(&'static str, String)>,
    reads: BTreeSet<Pattern>,
}

/// An atom as queried, asserted, seeded or concluded: its relation and, per
/// argument, the constant it names (`None`: a variable or `_`, matching
/// anything). Tracing a query through a rule binds the head's variables, so
/// `holds(trusted(ada))` follows `trusted(P) :- met(P)` to `met(ada)`.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Pattern {
    rel: String,
    args: Vec<Option<String>>,
}

impl Pattern {
    fn text(&self) -> String {
        let args: Vec<&str> = self.args.iter().map(|a| a.as_deref().unwrap_or("_")).collect();
        format!("{}({})", self.rel, args.join(", "))
    }

    /// Some ground instance satisfies both: same relation and arity, and no
    /// argument where both name different constants.
    fn unifies(&self, rel: &str, args: &[Option<String>]) -> bool {
        self.rel == rel
            && self.args.len() == args.len()
            && self.args.iter().zip(args).all(|(a, b)| match (a, b) {
                (Some(a), Some(b)) => a == b,
                _ => true,
            })
    }
}

fn fact_args(p: &lute_syntax::datalog::FactPattern) -> Vec<Option<String>> {
    use lute_syntax::datalog::FactTerm;
    p.args
        .iter()
        .map(|a| match &a.term {
            FactTerm::Ident(s) => Some(s.clone()),
            FactTerm::Bool(b) => Some(b.to_string()),
            FactTerm::Wildcard => None,
        })
        .collect()
}

/// A rule atom's arguments under `bound` (head variable -> constant).
fn rule_args(
    terms: &[lute_syntax::datalog::RuleTerm],
    bound: &BTreeMap<&str, String>,
) -> Vec<Option<String>> {
    use lute_syntax::datalog::RuleTerm;
    terms
        .iter()
        .map(|t| match t {
            RuleTerm::Var(v) => bound.get(v.as_str()).cloned(),
            RuleTerm::Const(c) => Some(c.clone()),
            RuleTerm::Bool(b) => Some(b.to_string()),
        })
        .collect()
}

/// Who asserts what: a producer label per asserting site, with the pattern
/// its `::assert` writes.
type Asserters = Vec<(String, Pattern)>;

/// The atoms a condition queries: the pattern of every `holds(R(…))`,
/// `count(R(…))`, `validAt(R(…), t)`.
fn queried(raw: &str) -> BTreeSet<Pattern> {
    use cel_parser::ast::Expr;
    use cel_parser::reference::Val;
    fn walk(expr: &Expr, out: &mut BTreeSet<Pattern>) {
        match expr {
            Expr::Call(c) => {
                if c.target.is_none() && matches!(c.func_name.as_str(), "holds" | "count" | "validAt") {
                    if let Some(Expr::Call(pattern)) = c.args.first().map(|a| &a.expr) {
                        let args = pattern
                            .args
                            .iter()
                            .map(|a| match &a.expr {
                                Expr::Ident(s) if s != "_" => Some(s.clone()),
                                Expr::Literal(Val::String(s)) => Some(s.to_string()),
                                Expr::Literal(Val::Boolean(b)) => Some(b.to_string()),
                                _ => None,
                            })
                            .collect();
                        out.insert(Pattern {
                            rel: pattern.func_name.clone(),
                            args,
                        });
                    }
                }
                if let Some(t) = &c.target {
                    walk(&t.expr, out);
                }
                for a in &c.args {
                    walk(&a.expr, out);
                }
            }
            Expr::List(l) => l.elements.iter().for_each(|e| walk(&e.expr, out)),
            Expr::Select(s) => walk(&s.operand.expr, out),
            _ => {}
        }
    }
    let mut out = BTreeSet::new();
    let mut arena = lute_cel::CelArena::default();
    if let Some(root) = lute_cel::parse_slot_marked_refs(&mut arena, raw).and_then(|h| arena.get(h)) {
        walk(&root.expr, &mut out);
    }
    out
}

/// A condition after `@def` expansion in its own document.
fn expanded(slot: &CelSlot, folded: &lute_check::FoldedEnv) -> String {
    let defs = lute_check::DefTable {
        bodies: &folded.def_bodies,
        params: &folded.env.def_params,
    };
    let mut stack = Vec::new();
    lute_check::expand_cel(&slot.raw, &defs, None, &mut stack).unwrap_or_else(|_| slot.raw.clone())
}

fn rel_path(root: &Path, p: &Path) -> String {
    p.strip_prefix(root).unwrap_or(p).display().to_string()
}

/// Every fact-guarded element of one root, document order: scene and
/// bundle beats, entries (beat or not), then quest objectives.
fn guarded(root: &Path, group: &DocGroup) -> Vec<Guarded> {
    let docs: Vec<(PathBuf, Document)> = group.iter().map(|(p, d, _)| (p.clone(), d.clone())).collect();
    let foldeds: Vec<&lute_check::FoldedEnv> = group.iter().map(|(_, _, f)| f).collect();
    let beats = lute_check::project_beats(&docs, &foldeds);
    let mut out = Vec::new();
    let mut push = |name: String, handles: Vec<String>, document: String, slots: Vec<(&'static str, String)>| {
        let reads: BTreeSet<Pattern> = slots.iter().flat_map(|(_, c)| queried(c)).collect();
        if !reads.is_empty() {
            out.push(Guarded {
                name,
                handles,
                document,
                slots,
                reads,
            });
        }
    };
    for (path, doc, folded) in group {
        let document = rel_path(root, path);
        for b in beats.iter().filter(|b| b.path == path && b.kind != ProjectBeatKind::Entry) {
            if let Some(when) = &b.when {
                push(b.name(), vec![b.id.clone()], document.clone(), vec![("when", when.clone())]);
            }
        }
        for e in &doc.entries {
            if let Some(when) = &e.when {
                push(
                    format!("entry `{}`", e.id),
                    vec![e.id.clone()],
                    document.clone(),
                    vec![("when", expanded(when, folded))],
                );
            }
        }
        for q in &doc.quests {
            for node in &q.body {
                let Node::Objective(o) = node else { continue };
                let mut slots = vec![("done", expanded(&o.done, folded))];
                slots.extend(o.when.iter().map(|s| ("when", expanded(s, folded))));
                slots.extend(o.by.iter().map(|s| ("by", expanded(s, folded))));
                let id = format!("{}.{}", q.id, o.id);
                push(
                    format!("objective `{id}`"),
                    vec![id, format!("quest:{}", q.id)],
                    document.clone(),
                    slots,
                );
            }
        }
    }
    out
}

/// Every asserting site in one root: `scene (path)`, ``quest `id` ``,
/// ``entry `id` ``, ``beat `id` ``, with the pattern it asserts.
fn asserters(root: &Path, group: &DocGroup) -> Asserters {
    let mut out = Asserters::new();
    let mut record = |nodes: &[Node], label: String| {
        let mut sites = Vec::new();
        lute_check::connectivity::collect_asserts(nodes, &mut sites);
        for a in sites {
            if !a.pattern.relation.is_empty() {
                let p = Pattern {
                    rel: a.pattern.relation.clone(),
                    args: fact_args(&a.pattern),
                };
                out.push((label.clone(), p));
            }
        }
    };
    for (path, doc, _) in group {
        let document = rel_path(root, path);
        for shot in &doc.shots {
            record(&shot.body, format!("scene ({document})"));
        }
        for q in &doc.quests {
            record(&q.body, format!("quest `{}` ({document})", q.id));
        }
        for e in &doc.entries {
            record(&e.body, format!("entry `{}` ({document})", e.id));
        }
        for b in &doc.beats {
            record(&b.body, format!("beat `{}` ({document})", b.id));
        }
    }
    out
}

/// The root's relational vocabulary: every document of one root folds the
/// same imported vocabulary (the `lute scenario` facts section's rule).
fn vocab(group: &DocGroup) -> RelVocab {
    group
        .iter()
        .map(|(_, _, f)| &*f.env.rel_vocab)
        .find(|v| !v.relations.is_empty())
        .cloned()
        .unwrap_or_default()
}

/// The immediate producers of `p` as one line — asserting sites, seed
/// facts, the engine, the rules whose head can conclude it — each
/// restricted to what can match `p`; the rules are traced by the caller.
fn producer_line(p: &Pattern, vocab: &RelVocab, asserted: &Asserters) -> String {
    let Some(decl) = vocab.relations.get(&p.rel) else {
        return "undeclared relation".to_string();
    };
    let mut parts = Vec::new();
    let sites: BTreeSet<&str> = asserted
        .iter()
        .filter(|(_, a)| p.unifies(&a.rel, &a.args))
        .map(|(label, _)| label.as_str())
        .collect();
    if !sites.is_empty() {
        parts.push(format!("asserted by {}", sites.into_iter().collect::<Vec<_>>().join(", ")));
    }
    let seeds: Vec<&str> = vocab
        .facts
        .iter()
        .filter(|f| p.unifies(&f.fact.relation, &fact_args(&f.fact)))
        .map(|f| f.raw.as_str())
        .collect();
    if !seeds.is_empty() {
        parts.push(format!("seed facts {}", seeds.join(", ")));
    }
    if decl.reserved {
        parts.push("reserved — the engine asserts it".to_string());
    }
    if decl.derive {
        let rules = vocab.rules.iter().filter(|r| concludes(p, &r.rule.head)).count();
        parts.push(match rules {
            0 => "derived, but no rule can conclude it".to_string(),
            1 => "derived by 1 rule".to_string(),
            n => format!("derived by {n} rules"),
        });
    }
    if parts.is_empty() {
        "NO PRODUCER — nothing asserts it, no seed fact, not reserved, no rule".to_string()
    } else {
        parts.join("; ")
    }
}

/// A rule with this head can conclude `p`.
fn concludes(p: &Pattern, head: &lute_syntax::datalog::RuleAtom) -> bool {
    p.unifies(&head.relation, &rule_args(&head.terms, &BTreeMap::new()))
}

/// Print `p` and, when derived, each rule that can conclude it with its
/// premises — the head's variables bound by `p` — traced recursively. An
/// atom already traced in this element (a shared or recursive premise) is
/// referenced, not re-expanded.
fn trace(
    out: &mut String,
    p: &Pattern,
    negated: bool,
    depth: usize,
    vocab: &RelVocab,
    asserted: &Asserters,
    traced: &mut BTreeSet<Pattern>,
) {
    use lute_syntax::datalog::RuleTerm;
    let pad = "  ".repeat(depth + 2);
    let not = if negated { "not " } else { "" };
    if !traced.insert(p.clone()) {
        outln!(out, "{pad}{not}{} — (traced above)", p.text());
        return;
    }
    outln!(out, "{pad}{not}{} — {}", p.text(), producer_line(p, vocab, asserted));
    for r in vocab.rules.iter().filter(|r| concludes(p, &r.rule.head)) {
        outln!(out, "{pad}  rule: {}", r.raw.trim());
        let mut bound: BTreeMap<&str, String> = BTreeMap::new();
        for (term, arg) in r.rule.head.terms.iter().zip(&p.args) {
            if let (RuleTerm::Var(v), Some(c)) = (term, arg) {
                bound.insert(v.as_str(), c.clone());
            }
        }
        for lit in &r.rule.body {
            let (atom, negated) = match lit {
                BodyLiteral::Pos(a) => (a, false),
                BodyLiteral::Neg(a) => (a, true),
                _ => continue,
            };
            let premise = Pattern {
                rel: atom.relation.clone(),
                args: rule_args(&atom.terms, &bound),
            };
            trace(out, &premise, negated, depth + 2, vocab, asserted, traced);
        }
    }
}

/// The `--for` selection over every root's elements; `Err` names the miss.
fn select<'a>(
    roots: &'a [(PathBuf, Vec<Guarded>, RelVocab, Asserters)],
    for_node: Option<&str>,
) -> Result<Vec<(&'a PathBuf, Vec<&'a Guarded>, &'a RelVocab, &'a Asserters)>, String> {
    let picked: Vec<_> = roots
        .iter()
        .map(|(root, elements, vocab, asserted)| {
            let chosen = elements
                .iter()
                .filter(|g| for_node.is_none_or(|n| g.handles.iter().any(|h| h == n)))
                .collect::<Vec<_>>();
            (root, chosen, vocab, asserted)
        })
        .collect();
    if let Some(n) = for_node {
        if picked.iter().all(|(_, chosen, _, _)| chosen.is_empty()) {
            let handles: Vec<&str> = roots
                .iter()
                .flat_map(|(_, e, _, _)| e.iter().flat_map(|g| g.handles.iter().map(String::as_str)))
                .collect();
            let hint = lute_manifest::suggest::nearest(n, handles.iter().copied(), 3)
                .map(|s| format!(" — did you mean `{s}`?"))
                .unwrap_or_default();
            return Err(format!(
                "`--for {n}` names no fact-guarded beat, entry or objective{hint}"
            ));
        }
    }
    Ok(picked)
}

fn collect(by_root: &ByRoot) -> Vec<(PathBuf, Vec<Guarded>, RelVocab, Asserters)> {
    by_root
        .iter()
        .map(|(root, group)| (root.clone(), guarded(root, group), vocab(group), asserters(root, group)))
        .collect()
}

/// The text view (`lute scenario <dir> knowledge`).
pub(crate) fn run_text(
    out: &mut String,
    by_root: &ByRoot,
    _file_results: &[(PathBuf, lute_check::CheckResult)],
    for_node: Option<&str>,
) -> ExitCode {
    let roots = collect(by_root);
    let picked = match select(&roots, for_node) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("lute scenario knowledge: {e}");
            return ExitCode::from(2);
        }
    };
    if by_root.is_empty() {
        outln!(out, "lute: no .lute files found");
        return ExitCode::SUCCESS;
    }
    for (root, elements, vocab, asserted) in picked {
        outln!(out, "project root: {}", root.display());
        outln!(out, "  knowledge (fact-guarded condition -> relations read -> producers):");
        if elements.is_empty() {
            outln!(out, "    (no condition queries a relation)");
            continue;
        }
        for g in elements {
            outln!(out);
            outln!(out, "  {} ({})", g.name, g.document);
            for (slot, cond) in &g.slots {
                outln!(out, "    {slot}: {}", cond.split_whitespace().collect::<Vec<_>>().join(" "));
            }
            let mut traced = BTreeSet::new();
            for rel in &g.reads {
                trace(out, rel, false, 0, vocab, asserted, &mut traced);
            }
        }
    }
    ExitCode::SUCCESS
}

/// The `--format json` view: per root, the elements (with the relations
/// they read) and one producer record per relation reached from them.
pub(crate) fn json(by_root: &ByRoot, for_node: Option<&str>) -> Result<Json, String> {
    let roots = collect(by_root);
    let picked = select(&roots, for_node)?;
    let roots_json: Vec<Json> = picked
        .into_iter()
        .map(|(root, elements, vocab, asserted)| {
            let mut reached: BTreeSet<String> = BTreeSet::new();
            let mut stack: Vec<String> =
                elements.iter().flat_map(|g| g.reads.iter().map(|p| p.rel.clone())).collect();
            while let Some(rel) = stack.pop() {
                if !reached.insert(rel.clone()) {
                    continue;
                }
                for r in vocab.rules.iter().filter(|r| r.rule.head.relation == rel) {
                    for lit in &r.rule.body {
                        if let BodyLiteral::Pos(a) | BodyLiteral::Neg(a) = lit {
                            stack.push(a.relation.clone());
                        }
                    }
                }
            }
            let relations: Map<String, Json> = reached
                .iter()
                .map(|rel| {
                    let decl = vocab.relations.get(rel);
                    let rules: Vec<Json> = vocab
                        .rules
                        .iter()
                        .filter(|r| &r.rule.head.relation == rel)
                        .map(|r| {
                            let premises: Vec<Json> = r
                                .rule
                                .body
                                .iter()
                                .filter_map(|lit| match lit {
                                    BodyLiteral::Pos(a) => Some(json!({ "relation": a.relation })),
                                    BodyLiteral::Neg(a) => {
                                        Some(json!({ "relation": a.relation, "negated": true }))
                                    }
                                    _ => None,
                                })
                                .collect();
                            json!({ "rule": r.raw.trim(), "premises": premises })
                        })
                        .collect();
                    let seeds: Vec<&str> = vocab
                        .facts
                        .iter()
                        .filter(|f| &f.fact.relation == rel)
                        .map(|f| f.raw.as_str())
                        .collect();
                    (
                        rel.clone(),
                        json!({
                            "declared": decl.is_some(),
                            "derived": decl.is_some_and(|d| d.derive),
                            "reserved": decl.is_some_and(|d| d.reserved),
                            "assertedBy": asserted
                                .iter()
                                .filter(|(_, a)| &a.rel == rel)
                                .map(|(label, _)| label.as_str())
                                .collect::<BTreeSet<_>>(),
                            "seedFacts": seeds,
                            "rules": rules,
                        }),
                    )
                })
                .collect();
            let elements: Vec<Json> = elements
                .iter()
                .map(|g| {
                    let slots: Map<String, Json> =
                        g.slots.iter().map(|(s, c)| (s.to_string(), json!(c))).collect();
                    json!({
                        "node": g.name,
                        "document": g.document,
                        "conditions": slots,
                        "reads": g.reads.iter().map(Pattern::text).collect::<Vec<_>>(),
                    })
                })
                .collect();
            json!({
                "root": root.display().to_string(),
                "elements": elements,
                "relations": relations,
            })
        })
        .collect();
    Ok(json!({ "roots": roots_json }))
}
