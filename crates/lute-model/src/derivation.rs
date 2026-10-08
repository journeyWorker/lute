//! Datalog dependency closure for the semantic graph.
//!
//! This adapter deliberately keeps graph construction separate from the
//! checker.  A rule is a dependency even when its body is not currently true:
//! the graph describes possible impact, not a run-time fact store.  Closed
//! domains and asserted facts are nevertheless used to ground variables.  The
//! latter is important for rules whose schema argument is open but whose
//! producer is a concrete authored fact.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use lute_core_span::{Evidence, Span};
use lute_manifest::relations::KindShape;
use lute_manifest::fact::{FactPattern, FactTerm};
use lute_syntax::datalog::{BodyLiteral, RuleAtom, RuleTerm};

use lute_semantic::{fact_node, format_fact, NodeKey, NodeKind, SemanticGraph};
use crate::{ModelDocument, ProjectModel};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Value {
    Text(String),
    Bool(bool),
}

impl Value {
    fn display(&self) -> String {
        match self {
            Self::Text(s) => s.clone(),
            Self::Bool(v) => v.to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct GroundAtom {
    relation: String,
    args: Vec<Value>,
}

#[derive(Clone, Debug)]
struct RuleSource {
    rule: lute_syntax::datalog::Rule,
    raw: String,
    file: PathBuf,
    span: Span,
}

type Bindings = BTreeMap<String, Value>;

/// Add all Datalog dependency edges to `graph`.
///
/// The closure is finite whenever the declared relation argument domains are
/// finite.  Variables with no finite domain are retained as `@Variable` in
/// graph fact keys (rather than being discarded); concrete asserted facts can
/// still bind those variables and are propagated through subsequent rules.
pub fn add_derivations(graph: &mut SemanticGraph, model: &ProjectModel) {
    let mut rules = BTreeMap::<(PathBuf, usize, String), RuleSource>::new();
    let mut relation_domains = BTreeMap::<String, Vec<Option<Vec<Value>>>>::new();
    let mut finite_domains = BTreeMap::<String, Vec<Value>>::new();
    let mut producers = BTreeSet::<GroundAtom>::new();
    let mut asserted = Vec::<(FactPattern, PathBuf)>::new();

    // Build one deterministic project-wide vocabulary.  A relation can be
    // imported into many documents; the source rule, not the importer, owns
    // its edge origin.
    for doc in model.documents() {
        merge_domains(doc, &mut finite_domains, &mut relation_domains);
        for fact in &doc.folded.env.rel_vocab.facts {
            if let Some(g) = ground_pattern(&fact.fact) {
                producers.insert(g);
            }
        }
        let feed = lute_check::deps::collect_document_dependencies(&doc.doc);
        for write in feed.fact_writes {
            asserted.push((write.pattern, doc.path.clone()));
        }
        for decl in &doc.folded.env.rel_vocab.rules {
            let origin = doc
                .folded
                .env
                .rel_vocab
                .origins
                .rules
                .get(&decl.raw)
                .or_else(|| doc.folded.env.rel_vocab.origins.rules.get(decl.raw.trim()))
                .or_else(|| {
                    doc.folded
                        .env
                        .rel_vocab
                        .origins
                        .rules
                        .iter()
                        .find(|(raw, _)| raw.trim() == decl.raw.trim())
                        .map(|(_, origin)| origin)
                });
            let (file, span) = origin
                .map(|o| (o.file.clone(), o.span))
                .unwrap_or_else(|| (doc.path.clone(), decl.span));
            let key = (file.clone(), span.byte_start, decl.raw.trim().to_string());
            rules.entry(key).or_insert_with(|| RuleSource {
                rule: decl.rule.clone(),
                raw: decl.raw.clone(),
                file,
                span,
            });
        }
    }

    // An authored assert is a possible producer even when it is not known to
    // execute in the current scenario.  Expand finite wildcard/target slots;
    // a partially open pattern remains represented by the direct assert edge
    // emitted by graph.rs and does not invent a value here.
    for (pattern, _) in &asserted {
        for fact in expand_pattern(pattern, &relation_domains, &finite_domains) {
            producers.insert(fact);
        }
    }

    let rules: Vec<RuleSource> = rules.into_values().collect();
    // First compute the ground portion of the least fixpoint.  Cycles stop as
    // soon as no new ground head exists; symbolic instances are handled below
    // and therefore do not make this loop depend on an arbitrary depth bound.
    loop {
        let mut changed = false;
        for source in &rules {
            for bindings in
                rule_bindings(&source.rule, &producers, &relation_domains, &finite_domains)
            {
                if let Some(head) = ground_atom(&source.rule.head, &bindings) {
                    changed |= producers.insert(head);
                }
            }
        }
        if !changed {
            break;
        }
    }

    let mut seen = BTreeSet::<EdgeKey>::new();
    for source in &rules {
        let bindings = rule_bindings(&source.rule, &producers, &relation_domains, &finite_domains);
        for env in bindings {
            let Some(head) = symbolic_atom(&source.rule.head, &env) else {
                continue;
            };
            let head_node = fact_node(
                &head.relation,
                &head.args.iter().map(|a| a.term.clone()).collect::<Vec<_>>(),
            );
            graph.node(
                head_node.clone(),
                Some(source.file.clone()),
                Some(source.span),
            );
            let reason = rule_reason(source, &env);
            for literal in &source.rule.body {
                match literal {
                    BodyLiteral::Pos(atom) | BodyLiteral::Neg(atom) => {
                        add_atom_edge(
                            graph, &mut seen, source, atom, &head_node, &reason, false, &env,
                        );
                    }
                    BodyLiteral::Count { atom, .. } => {
                        add_atom_edge(
                            graph, &mut seen, source, atom, &head_node, &reason, true, &env,
                        );
                    }
                    BodyLiteral::Guard { cel, .. } => {
                        add_guard_edges(graph, &mut seen, source, cel, &head_node, &reason);
                    }
                    BodyLiteral::Cmp { .. } => {}
                }
            }
        }
    }
}

fn merge_domains(
    doc: &ModelDocument,
    finite: &mut BTreeMap<String, Vec<Value>>,
    relation_domains: &mut BTreeMap<String, Vec<Option<Vec<Value>>>>,
) {
    for (name, domain) in &doc.folded.env.domains {
        if !domain.open {
            let values = domain
                .members
                .iter()
                .map(|s| Value::Text(s.clone()))
                .collect::<Vec<_>>();
            merge_values(finite.entry(name.clone()).or_default(), values);
        }
    }
    for (name, shape) in &doc.folded.env.rel_vocab.kinds {
        if let KindShape::Members(members) = &shape.shape {
            merge_values(
                finite.entry(name.clone()).or_default(),
                members.iter().map(|s| Value::Text(s.clone())).collect(),
            );
        }
    }
    for (name, members) in &doc.folded.env.rel_vocab.enums {
        merge_values(
            finite.entry(name.clone()).or_default(),
            members.iter().map(|s| Value::Text(s.clone())).collect(),
        );
    }
    finite
        .entry("bool".to_string())
        .or_insert_with(|| vec![Value::Bool(false), Value::Bool(true)]);

    for (name, relation) in &doc.folded.env.rel_vocab.relations {
        let slots = relation_domains
            .entry(name.clone())
            .or_insert_with(|| vec![None; relation.args.len()]);
        if slots.len() < relation.args.len() {
            slots.resize(relation.args.len(), None);
        }
        for (slot, domain) in slots.iter_mut().zip(&relation.args) {
            let values = finite.get(domain).cloned();
            *slot = merge_optional_values(slot.take(), values);
        }
    }
}

fn merge_values(dst: &mut Vec<Value>, mut values: Vec<Value>) {
    dst.append(&mut values);
    dst.sort();
    dst.dedup();
}

fn merge_optional_values(old: Option<Vec<Value>>, new: Option<Vec<Value>>) -> Option<Vec<Value>> {
    match (old, new) {
        (Some(mut a), Some(b)) => {
            a.extend(b);
            a.sort();
            a.dedup();
            Some(a)
        }
        (None, Some(b)) => Some(b),
        (a, None) => a,
    }
}

fn rule_bindings(
    rule: &lute_syntax::datalog::Rule,
    producers: &BTreeSet<GroundAtom>,
    relation_domains: &BTreeMap<String, Vec<Option<Vec<Value>>>>,
    finite_domains: &BTreeMap<String, Vec<Value>>,
) -> Vec<Bindings> {
    let mut envs = vec![Bindings::new()];
    // Ground producers bind unknown/open-domain variables.  A missing producer
    // is intentionally not a reason to suppress the symbolic dependency.
    for literal in &rule.body {
        let atom = match literal {
            BodyLiteral::Pos(a) => a,
            _ => continue,
        };
        let matches: Vec<&GroundAtom> = producers
            .iter()
            .filter(|fact| fact.relation == atom.relation && fact.args.len() == atom.terms.len())
            .collect();
        if matches.is_empty() {
            continue;
        }
        let mut next = envs.clone();
        for env in &envs {
            for fact in &matches {
                if let Some(bound) = unify(atom, fact, env) {
                    next.push(bound);
                }
            }
        }
        next.sort();
        next.dedup();
        envs = next;
    }

    let variables = rule_variables(rule);
    for var in variables {
        let Some(domain) = variable_domain(rule, &var, relation_domains, finite_domains) else {
            continue;
        };
        let mut next = Vec::new();
        for env in &envs {
            if env.contains_key(&var) {
                next.push(env.clone());
            } else {
                for value in &domain {
                    let mut bound = env.clone();
                    bound.insert(var.clone(), value.clone());
                    next.push(bound);
                }
            }
        }
        if !next.is_empty() {
            envs = next;
        }
    }
    envs.retain(|env| {
        rule.body.iter().all(|literal| match literal {
            BodyLiteral::Cmp {
                lhs, rhs, negated, ..
            } => match (term_value(lhs, env), term_value(rhs, env)) {
                (Some(a), Some(b)) => {
                    if *negated {
                        a != b
                    } else {
                        a == b
                    }
                }
                _ => true,
            },
            _ => true,
        })
    });
    envs.sort();
    envs.dedup();
    envs
}

fn rule_variables(rule: &lute_syntax::datalog::Rule) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for a in
        std::iter::once(&rule.head).chain(rule.body.iter().filter_map(|literal| literal.atom()))
    {
        for term in &a.terms {
            if let RuleTerm::Var(name) = term {
                if !lute_manifest::fact::is_anonymous_var(name) {
                    out.insert(name.clone());
                }
            }
        }
    }
    for literal in &rule.body {
        if let BodyLiteral::Cmp { lhs, rhs, .. } = literal {
            for term in [lhs, rhs] {
                if let RuleTerm::Var(name) = term {
                    out.insert(name.clone());
                }
            }
        }
    }
    out
}

fn variable_domain(
    rule: &lute_syntax::datalog::Rule,
    variable: &str,
    relation_domains: &BTreeMap<String, Vec<Option<Vec<Value>>>>,
    finite_domains: &BTreeMap<String, Vec<Value>>,
) -> Option<Vec<Value>> {
    let mut found: Option<Vec<Value>> = None;
    let mut visit = |atom: &RuleAtom| {
        for (i, term) in atom.terms.iter().enumerate() {
            if !matches!(term, RuleTerm::Var(v) if v == variable) {
                continue;
            }
            let Some(Some(values)) = relation_domains.get(&atom.relation).and_then(|a| a.get(i))
            else {
                continue;
            };
            found = Some(match found.take() {
                None => values.clone(),
                Some(old) => old.into_iter().filter(|v| values.contains(v)).collect(),
            });
        }
    };
    visit(&rule.head);
    for literal in &rule.body {
        match literal {
            BodyLiteral::Pos(a) | BodyLiteral::Neg(a) => visit(a),
            BodyLiteral::Count { atom: a, .. } => visit(a),
            BodyLiteral::Guard { .. } | BodyLiteral::Cmp { .. } => {}
        }
    }
    found.or_else(|| finite_domains.get(variable).cloned())
}

fn unify(atom: &RuleAtom, fact: &GroundAtom, initial: &Bindings) -> Option<Bindings> {
    let mut env = initial.clone();
    for (term, value) in atom.terms.iter().zip(&fact.args) {
        match term {
            RuleTerm::Const(expected) if value != &Value::Text(expected.clone()) => return None,
            RuleTerm::Bool(expected) if value != &Value::Bool(*expected) => return None,
            RuleTerm::Var(name) if !lute_manifest::fact::is_anonymous_var(name) => {
                if let Some(old) = env.get(name) {
                    if old != value {
                        return None;
                    }
                } else {
                    env.insert(name.clone(), value.clone());
                }
            }
            RuleTerm::Var(_) => {}
            _ => {}
        }
    }
    Some(env)
}

fn term_value(term: &RuleTerm, env: &Bindings) -> Option<Value> {
    match term {
        RuleTerm::Const(s) => Some(Value::Text(s.clone())),
        RuleTerm::Bool(v) => Some(Value::Bool(*v)),
        RuleTerm::Var(name) => env.get(name).cloned(),
    }
}

fn ground_atom(atom: &RuleAtom, env: &Bindings) -> Option<GroundAtom> {
    let args = atom
        .terms
        .iter()
        .map(|term| match term {
            RuleTerm::Const(s) => Some(Value::Text(s.clone())),
            RuleTerm::Bool(v) => Some(Value::Bool(*v)),
            RuleTerm::Var(v) => env.get(v).cloned(),
        })
        .collect::<Option<Vec<_>>>()?;
    Some(GroundAtom {
        relation: atom.relation.clone(),
        args,
    })
}

fn symbolic_atom(atom: &RuleAtom, env: &Bindings) -> Option<FactPattern> {
    let args: Vec<FactTerm> = atom
        .terms
        .iter()
        .map(|term| match term {
            RuleTerm::Const(s) => FactTerm::Ident(s.clone()),
            RuleTerm::Bool(v) => FactTerm::Bool(*v),
            RuleTerm::Var(v) => env
                .get(v)
                .map(value_term)
                .unwrap_or_else(|| FactTerm::Param(v.clone())),
        })
        .collect();
    Some(FactPattern {
        relation: atom.relation.clone(),
        relation_span: (0, 0),
        args: args
            .into_iter()
            .map(|term| lute_manifest::fact::FactArg { term, span: (0, 0) })
            .collect(),
        span: (0, 0),
    })
}

fn value_term(value: &Value) -> FactTerm {
    match value {
        Value::Text(s) => FactTerm::Ident(s.clone()),
        Value::Bool(v) => FactTerm::Bool(*v),
    }
}

fn ground_pattern(pattern: &FactPattern) -> Option<GroundAtom> {
    let args = pattern
        .args
        .iter()
        .map(|arg| match &arg.term {
            FactTerm::Ident(s) => Some(Value::Text(s.clone())),
            FactTerm::Bool(v) => Some(Value::Bool(*v)),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    Some(GroundAtom {
        relation: pattern.relation.clone(),
        args,
    })
}

fn expand_pattern(
    pattern: &FactPattern,
    relation_domains: &BTreeMap<String, Vec<Option<Vec<Value>>>>,
    _finite_domains: &BTreeMap<String, Vec<Value>>,
) -> Vec<GroundAtom> {
    let mut out = vec![Vec::<Value>::new()];
    for (i, arg) in pattern.args.iter().enumerate() {
        let values = match &arg.term {
            FactTerm::Ident(s) => vec![Value::Text(s.clone())],
            FactTerm::Bool(v) => vec![Value::Bool(*v)],
            FactTerm::Wildcard | FactTerm::Param(_) | FactTerm::Target => relation_domains
                .get(&pattern.relation)
                .and_then(|a| a.get(i))
                .and_then(|v| v.clone())
                .unwrap_or_default(),
        };
        if values.is_empty() {
            return Vec::new();
        }
        let mut next = Vec::new();
        for prefix in &out {
            for value in &values {
                let mut p = prefix.clone();
                p.push(value.clone());
                next.push(p);
            }
        }
        out = next;
    }
    out.into_iter()
        .map(|args| GroundAtom {
            relation: pattern.relation.clone(),
            args,
        })
        .collect()
}

fn add_atom_edge(
    graph: &mut SemanticGraph,
    seen: &mut BTreeSet<EdgeKey>,
    source: &RuleSource,
    atom: &RuleAtom,
    head: &NodeKey,
    reason: &str,
    counted: bool,
    env: &Bindings,
) {
    let Some(pattern) = symbolic_atom(atom, env) else {
        return;
    };
    let terms: Vec<FactTerm> = pattern.args.iter().map(|a| a.term.clone()).collect();
    let source_node = fact_node(&pattern.relation, &terms);
    let reason = if counted {
        format!(
            "{reason}; dependency: count({})",
            format_fact(&pattern.relation, &terms)
        )
    } else {
        format!(
            "{reason}; dependency: {}",
            format_fact(&pattern.relation, &terms)
        )
    };
    let evidence = if rule_head_param_heuristic(&source.rule) {
        Evidence::Heuristic
    } else if terms.iter().any(|term| matches!(term, FactTerm::Param(_))) {
        Evidence::Heuristic
    } else {
        Evidence::Proven
    };
    add_edge(
        graph,
        seen,
        source_node,
        head.clone(),
        reason,
        source.file.clone(),
        source.span,
        evidence,
    );
}

fn rule_head_param_heuristic(rule: &lute_syntax::datalog::Rule) -> bool {
    let Some(first_body) = rule.body.iter().find_map(|literal| literal.atom()) else {
        return false;
    };
    rule.head.terms.iter().any(|term| {
        let RuleTerm::Var(name) = term else {
            return false;
        };
        let bound_by_first = first_body
            .terms
            .iter()
            .any(|candidate| matches!(candidate, RuleTerm::Var(candidate) if candidate == name));
        let bound_elsewhere = rule
            .body
            .iter()
            .skip(1)
            .filter_map(|literal| literal.atom())
            .any(|body| {
                body.terms.iter().any(
                    |candidate| matches!(candidate, RuleTerm::Var(candidate) if candidate == name),
                )
            });
        !bound_by_first && bound_elsewhere
    })
}

fn add_guard_edges(
    graph: &mut SemanticGraph,
    seen: &mut BTreeSet<EdgeKey>,
    source: &RuleSource,
    cel: &str,
    head: &NodeKey,
    reason: &str,
) {
    let feed = lute_check::deps::slot_dependencies(cel, source.span);
    for read in feed.reads {
        add_edge(
            graph,
            seen,
            NodeKey::new(NodeKind::State, read.path.clone()),
            head.clone(),
            format!("{reason}; guard: cel(\"{cel}\") reads {}", read.path),
            source.file.clone(),
            source.span,
            Evidence::Proven,
        );
    }
    for query in feed.queries {
        let node = fact_node(
            &query.pattern.relation,
            &query
                .pattern
                .args
                .iter()
                .map(|a| a.term.clone())
                .collect::<Vec<_>>(),
        );
        add_edge(
            graph,
            seen,
            node,
            head.clone(),
            format!("{reason}; guard: {cel}"),
            source.file.clone(),
            source.span,
            if query
                .pattern
                .args
                .iter()
                .any(|arg| matches!(arg.term, FactTerm::Param(_)))
            {
                Evidence::Heuristic
            } else {
                Evidence::Proven
            },
        );
    }
}

fn rule_reason(source: &RuleSource, env: &Bindings) -> String {
    let bindings = env
        .iter()
        .map(|(name, value)| format!("{name}={}", value.display()))
        .collect::<Vec<_>>()
        .join(", ");
    format!("rule: {}; bindings: {{{bindings}}}", source.raw.trim())
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct EdgeKey {
    source: String,
    target: String,
    kind: String,
    reason: String,
    file: PathBuf,
    start: usize,
    end: usize,
}

fn add_edge(
    graph: &mut SemanticGraph,
    seen: &mut BTreeSet<EdgeKey>,
    source: NodeKey,
    target: NodeKey,
    reason: String,
    file: PathBuf,
    span: Span,
    evidence: Evidence,
) {
    let key = EdgeKey {
        source: source.canonical(),
        target: target.canonical(),
        kind: "derives".to_string(),
        reason: reason.clone(),
        file: file.clone(),
        start: span.byte_start,
        end: span.byte_end,
    };
    if seen.insert(key) {
        graph.edge(
            source,
            target,
            "derives",
            reason,
            Some(file),
            Some(span),
            evidence,
        );
    }
}
