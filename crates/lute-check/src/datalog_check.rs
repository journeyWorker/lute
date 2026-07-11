//! Per-rule Datalog checks (dsl 0.3.0 §7.1/§7.2): head legality, body-atom
//! closure, and safety — every check here is a syntactic/graph property of
//! ONE rule against the merged [`RelVocab`] (D1: no evaluator, no fixpoint
//! over facts). Runs on the MERGED post-composition rule set (`vocab.rules`,
//! spec §7.2) inside `check()`'s `fold_env`.
//!
//! Reuses [`check_atom`] (Task 7) for the "is this term-list a legal use of a
//! declared relation" closure shared with seed facts: a rule atom's `Var`
//! terms map to [`FactTerm::Wildcard`] with `wildcard_ok: true` (D6 domain-
//! checks only GROUND terms — a `Var`'s binding is a SAFETY concern, checked
//! separately by [`check_rule_safety`]), and ground `Const`/`Bool` terms map
//! straight across so `check_atom`'s existing per-arg domain closure applies
//! unchanged. The one shape `check_atom` cannot express — an entity KIND used
//! as §3.1's unary domain predicate `K(X)` in a rule body — gets its own
//! small closure ([`check_kind_predicate_arg`]), duplicating just the
//! `KindShape` match `check_atom` runs internally (its own copy is private to
//! `rel_schema.rs`).
//!
//! Stratification + guard-taint ([`check_stratification`], Task 9) are the
//! whole-rule-set graph analyses: Tarjan SCC over the predicate-dependency
//! graph (`E-DATALOG-UNSTRATIFIED` for a negation cycle) and the guard-taint
//! closure that fills [`RelVocab::guard_tainted`] for Task 11's
//! `E-VALIDAT-DERIVED`. Both are pure graph analyses over `vocab.rules` — no
//! fixpoint over facts (D1); everything above this stays purely per-rule.

use std::collections::{BTreeMap, BTreeSet};

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::relations::{EntityKindDecl, KindShape};
use lute_manifest::snapshot::Domain;
use lute_syntax::datalog::{BodyLiteral, FactArg, FactTerm, Rule, RuleAtom, RuleTerm};

use crate::rel_schema::{check_atom, RelVocab, E_FACT_DOMAIN, E_RELATION_ARITY, E_RELATION_UNKNOWN};

pub const E_DERIVE_UNDECLARED: &str = "E-DERIVE-UNDECLARED"; // §7.1
pub const W_DERIVE_NO_RULES: &str = "W-DERIVE-NO-RULES"; // §7.1 — Severity::Warning
pub const E_DATALOG_UNSAFE: &str = "E-DATALOG-UNSAFE"; // §7.2
pub const E_DATALOG_UNSTRATIFIED: &str = "E-DATALOG-UNSTRATIFIED"; // §7.2

/// Per-rule + per-derived-relation checks (§7.1/§7.2, D1): head legality,
/// body-atom closure, and safety for every rule in `vocab.rules`, plus
/// `W-DERIVE-NO-RULES` for a `derive: true` relation no rule ever heads.
/// Pure; safety's fixpoint runs over variable NAMES, never facts.
pub fn check_rules(vocab: &RelVocab, domains: &BTreeMap<String, Domain>) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for rule_decl in &vocab.rules {
        let rule = &rule_decl.rule;
        let span = rule_decl.span;
        check_head(vocab, domains, &rule.head, span, &mut out);
        for lit in &rule.body {
            if let BodyLiteral::Pos(atom) | BodyLiteral::Neg(atom) = lit {
                check_body_atom(vocab, domains, atom, span, &mut out);
            }
        }
        check_rule_safety(rule, span, &mut out);
    }
    for (name, decl) in &vocab.relations {
        if decl.derive && !vocab.rules.iter().any(|r| &r.rule.head.relation == name) {
            out.push(warn(
                W_DERIVE_NO_RULES,
                format!(
                    "relation `{name}` is declared `derive: true` but has no rules; the relation \
                     is legal but permanently empty — almost always a typo'd head name (dsl 0.3.0 §7.1)"
                ),
                zero_span(),
            ));
        }
    }
    out
}

/// A rule's head (§7.1): the relation must exist AND be `derive: true` — a
/// base relation, a reserved relation, or an entity-kind name as head all
/// lack `derive: true` → `E-DERIVE-UNDECLARED`. Arity + ground-term domain
/// checks (D6) still run whenever the relation resolves, regardless of its
/// `derive`-ness — independent checks, same as `check_atom`'s own cascade.
fn check_head(
    vocab: &RelVocab,
    domains: &BTreeMap<String, Domain>,
    head: &RuleAtom,
    span: Span,
    out: &mut Vec<Diagnostic>,
) {
    match vocab.relations.get(&head.relation) {
        Some(decl) => {
            if !decl.derive {
                out.push(diag(
                    E_DERIVE_UNDECLARED,
                    format!(
                        "relation `{}` is not declared `derive: true`; only a derived relation may \
                         be a rule head (dsl 0.3.0 §7.1)",
                        head.relation
                    ),
                    span,
                ));
            }
            out.extend(check_atom(
                vocab,
                domains,
                &head.relation,
                &rule_terms_to_fact_args(&head.terms),
                /* wildcard_ok = */ true,
                span,
            ));
        }
        None if vocab.kinds.contains_key(&head.relation) => {
            out.push(diag(
                E_DERIVE_UNDECLARED,
                format!(
                    "entity kind `{}` cannot be a rule head; only a `derive: true` relation may be \
                     derived (dsl 0.3.0 §3.1/§7.1)",
                    head.relation
                ),
                span,
            ));
        }
        None => {
            out.push(diag(
                E_RELATION_UNKNOWN,
                format!("unknown relation `{}` (dsl 0.3.0 §7.1)", head.relation),
                span,
            ));
        }
    }
}

/// A `Pos`/`Neg` body atom (§7.2): its name must be a declared relation (ANY
/// class — a derived relation MAY feed another rule) OR an entity kind used
/// at arity 1 (§3.1's unary domain predicate `K(X)`, the one shape
/// `check_atom` cannot express since a kind is never itself a relation).
fn check_body_atom(
    vocab: &RelVocab,
    domains: &BTreeMap<String, Domain>,
    atom: &RuleAtom,
    span: Span,
    out: &mut Vec<Diagnostic>,
) {
    if !vocab.relations.contains_key(&atom.relation) {
        if let Some(kind) = vocab.kinds.get(&atom.relation) {
            if atom.terms.len() != 1 {
                out.push(diag(
                    E_RELATION_ARITY,
                    format!(
                        "entity kind `{0}` is a unary domain predicate `{0}(X)`; expected 1 \
                         argument, got {1} (dsl 0.3.0 §3.1)",
                        atom.relation,
                        atom.terms.len()
                    ),
                    span,
                ));
                return;
            }
            check_kind_predicate_arg(vocab, kind, &atom.relation, &atom.terms[0], span, out);
            return;
        }
    }
    out.extend(check_atom(
        vocab,
        domains,
        &atom.relation,
        &rule_terms_to_fact_args(&atom.terms),
        /* wildcard_ok = */ true,
        span,
    ));
}

/// `RuleTerm` → `FactArg` so `check_atom`'s arity + per-term domain closure
/// (D6) can be reused verbatim for a rule atom resolving to a genuine
/// relation: `Var` maps to `Wildcard` (skipped by `check_atom` when
/// `wildcard_ok: true` — D6 domain-checks only GROUND terms; a `Var`'s
/// binding is [`check_rule_safety`]'s concern), `Const`/`Bool` map straight
/// across. `check_atom` never reads a `FactArg`'s own `span` (it reports at
/// its `span` parameter), so the placeholder `(0, 0)` here is inert.
fn rule_terms_to_fact_args(terms: &[RuleTerm]) -> Vec<FactArg> {
    terms
        .iter()
        .map(|t| FactArg {
            term: match t {
                RuleTerm::Var(_) => FactTerm::Wildcard,
                RuleTerm::Const(s) => FactTerm::Ident(s.clone()),
                RuleTerm::Bool(b) => FactTerm::Bool(*b),
            },
            span: (0, 0),
        })
        .collect()
}

/// Domain-check the single argument of a rule body atom naming an entity
/// KIND used as §3.1's unary domain predicate `K(X)` (D6) — mirrors
/// `check_atom`'s own `KindShape` closure, duplicated because a kind is never
/// itself a relation, so `check_atom` cannot be called for this shape. A
/// `Var` argument is never ground and is skipped (safety, not domain
/// membership, governs it).
fn check_kind_predicate_arg(
    vocab: &RelVocab,
    kind: &EntityKindDecl,
    kind_name: &str,
    term: &RuleTerm,
    span: Span,
    out: &mut Vec<Diagnostic>,
) {
    if matches!(term, RuleTerm::Var(_)) {
        return;
    }
    match &kind.shape {
        KindShape::Members(members) => {
            let RuleTerm::Const(id) = term else {
                out.push(diag(
                    E_FACT_DOMAIN,
                    format!(
                        "`{kind_name}(…)` argument must be a member of entity kind `{kind_name}` \
                         (dsl 0.3.0 §3.1)"
                    ),
                    span,
                ));
                return;
            };
            if !members.contains(id) {
                out.push(diag(
                    E_FACT_DOMAIN,
                    format!(
                        "`{id}` is not a declared member of entity kind `{kind_name}` (dsl 0.3.0 §3.1)"
                    ),
                    span,
                ));
            }
        }
        KindShape::Open => {
            let RuleTerm::Const(id) = term else {
                out.push(diag(
                    E_FACT_DOMAIN,
                    format!(
                        "`{kind_name}(…)` argument must be an id (declared `open` entity kind \
                         `{kind_name}`, dsl 0.3.0 §3.1)"
                    ),
                    span,
                ));
                return;
            };
            if let Some(other) = closed_kind_owning(vocab, id) {
                out.push(diag(
                    E_FACT_DOMAIN,
                    format!(
                        "`{id}` already belongs to entity kind `{other}`; an id belongs to exactly \
                         one kind (dsl 0.3.0 §3.1)"
                    ),
                    span,
                ));
            }
        }
        KindShape::Invalid => {
            // The decl itself already got E-ENTITY-KIND-SHAPE; never cascade.
        }
    }
}

/// The name of a CLOSED entity kind that already claims `id` as a member, if
/// any (§3.1 one-id-one-kind, D10's cross-check for an open-kind arg). A
/// local copy of `rel_schema::closed_kind_owning` (private to its own module)
/// — this is the one other call site, reached only via
/// [`check_kind_predicate_arg`].
fn closed_kind_owning<'a>(vocab: &'a RelVocab, id: &str) -> Option<&'a str> {
    vocab
        .kinds
        .iter()
        .find_map(|(name, decl)| match &decl.shape {
            KindShape::Members(members) if members.iter().any(|m| m.as_str() == id) => {
                Some(name.as_str())
            }
            _ => None,
        })
}

/// Safety (§7.2): a fixpoint over the SET OF VARIABLE NAMES a rule binds —
/// never over facts (D1). `bound` starts as every `Var` appearing in a `Pos`
/// body atom, then grows via equality-binding (`V = c` binds `V`; `V = W`
/// binds whichever side is still unbound once the other is) until no more
/// literals bind anything new. Every head `Var` and every `Var` inside a
/// `Neg` atom or a `!=` comparison MUST end up in `bound`, else
/// `E-DATALOG-UNSAFE`. `!=` never binds (both its sides must already be
/// bound); a `Guard` literal binds nothing and is skipped — it contains no
/// rule variables by construction (bare uppercase idents are out of the
/// closed CEL profile).
fn check_rule_safety(rule: &Rule, span: Span, out: &mut Vec<Diagnostic>) {
    let mut bound: BTreeSet<&str> = BTreeSet::new();
    for lit in &rule.body {
        if let BodyLiteral::Pos(atom) = lit {
            for term in &atom.terms {
                if let RuleTerm::Var(v) = term {
                    bound.insert(v.as_str());
                }
            }
        }
    }

    loop {
        let mut changed = false;
        for lit in &rule.body {
            if let BodyLiteral::Cmp {
                lhs,
                rhs,
                negated: false,
                ..
            } = lit
            {
                changed |= bind_via_equality(lhs, rhs, &mut bound);
            }
        }
        if !changed {
            break;
        }
    }

    for term in &rule.head.terms {
        if let RuleTerm::Var(v) = term {
            if !bound.contains(v.as_str()) {
                out.push(diag(
                    E_DATALOG_UNSAFE,
                    format!(
                        "variable `{v}` in the head of rule `{}` is unsafe: it is not bound by a \
                         positive body atom or equality (dsl 0.3.0 §7.2)",
                        rule.head.relation
                    ),
                    span,
                ));
            }
        }
    }

    for lit in &rule.body {
        match lit {
            BodyLiteral::Neg(atom) => {
                for term in &atom.terms {
                    if let RuleTerm::Var(v) = term {
                        if !bound.contains(v.as_str()) {
                            out.push(diag(
                                E_DATALOG_UNSAFE,
                                format!(
                                    "variable `{v}` in negated atom `{}` is unsafe: negation binds \
                                     nothing, and the variable is not bound by any positive body \
                                     atom or equality (dsl 0.3.0 §7.2)",
                                    atom.relation
                                ),
                                span,
                            ));
                        }
                    }
                }
            }
            BodyLiteral::Cmp {
                lhs,
                rhs,
                negated: true,
                ..
            } => {
                for t in [lhs, rhs] {
                    if let RuleTerm::Var(v) = t {
                        if !bound.contains(v.as_str()) {
                            out.push(diag(
                                E_DATALOG_UNSAFE,
                                format!(
                                    "variable `{v}` in a `!=` comparison is unsafe: `!=` never \
                                     binds, and the variable is not bound by any positive body \
                                     atom or equality (dsl 0.3.0 §7.2)"
                                ),
                                span,
                            ));
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// One equality-binding step (`Cmp { negated: false, .. }`, §7.2): `V = c`
/// (the other side ground) binds `V` unconditionally; `V = W` binds whichever
/// side is still unbound once the other is already `bound`. Returns whether
/// `bound` grew, so the caller's fixpoint loop can detect convergence.
fn bind_via_equality<'r>(lhs: &'r RuleTerm, rhs: &'r RuleTerm, bound: &mut BTreeSet<&'r str>) -> bool {
    let lhs_var = match lhs {
        RuleTerm::Var(v) => Some(v.as_str()),
        _ => None,
    };
    let rhs_var = match rhs {
        RuleTerm::Var(v) => Some(v.as_str()),
        _ => None,
    };
    match (lhs_var, rhs_var) {
        (Some(v), Some(w)) => {
            let v_bound = bound.contains(v);
            let w_bound = bound.contains(w);
            if v_bound && !w_bound {
                bound.insert(w);
                true
            } else if w_bound && !v_bound {
                bound.insert(v);
                true
            } else {
                false
            }
        }
        (Some(v), None) | (None, Some(v)) => {
            if bound.contains(v) {
                false
            } else {
                bound.insert(v);
                true
            }
        }
        (None, None) => false,
    }
}

/// `RelVocab`'s merged `relations:` map carries no per-decl span
/// (`RelationDecl` has none) and `check_rules`'s signature — deliberately
/// just `vocab` + `domains` — has no `Meta` to recover one from.
/// `W-DERIVE-NO-RULES` is a whole-vocabulary property (a relation with zero
/// rules), not a per-rule one; its diagnostic anchors at byte 0, and
/// `crate::check`'s `normalize_spans` fills in line/column from that offset —
/// same zero-then-normalize convention every other producer here follows.
fn zero_span() -> Span {
    Span {
        byte_start: 0,
        byte_end: 0,
        line: 0,
        column: 0,
        utf16_range: (0, 0),
    }
}

/// Build a `Layer::Logic` ERROR diagnostic (0.3.0 Global Constraints: every
/// new module's own small `diag` helper, exactly like `set_op.rs`'s).
fn diag(code: &str, message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity: Severity::Error,
        message,
        span,
        layer: Layer::Logic,
        fixits: Vec::new(),
        provenance: None,
    }
}

/// Build a `Layer::Logic` WARNING diagnostic — this module's one `W-*` code
/// ([`W_DERIVE_NO_RULES`]) needs `Severity::Warning`, so it gets its own tiny
/// constructor rather than widening [`diag`]'s signature away from the
/// `set_op.rs` house shape.
fn warn(code: &str, message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity: Severity::Warning,
        message,
        span,
        layer: Layer::Logic,
        fixits: Vec::new(),
        provenance: None,
    }
}

/// Stratification + guard-taint (§7.2/§6, Task 9, D1): whole-rule-set graph
/// analyses over the MERGED `vocab.rules` — no fixpoint over facts, ever.
///
/// **Stratification.** Nodes = declared relation names (`vocab.relations`'
/// keys; entity kinds are excluded — they are never rule heads, so an edge
/// FROM a kind can never close a cycle). For every rule `H :- …L…` whose
/// head `H` resolves to a declared relation (an unknown head is skipped —
/// [`check_head`] already reported `E-RELATION-UNKNOWN`/`E-DERIVE-UNDECLARED`
/// for it), and for every `Pos`/`Neg` body atom `L` that ALSO resolves to a
/// declared relation (a kind-as-`K(X)` or an undeclared name contributes no
/// edge — the former can never cycle back, the latter is [`check_body_atom`]'s
/// concern), add a structural edge `pred(L) → H`, tagged negative when `L` is
/// `Neg`. Tarjan's algorithm ([`tarjan_sccs`], iterative — an explicit stack
/// simulates the call frames so the depth-first walk never recurses) assigns
/// every node its strongly-connected-component id; any NEGATIVE edge whose
/// two endpoints share an SCC — including a direct self-edge, `p :- not p`,
/// which is trivially its own single-node "cycle" — is a negation cycle
/// (`E-DATALOG-UNSTRATIFIED`, at the rule that carries the offending literal,
/// naming every relation in that SCC, sorted). A purely POSITIVE cycle (e.g.
/// `canReach`'s self-recursion) shares an SCC too but is never flagged — only
/// a negated edge closing the cycle is unstratifiable (§7.2).
///
/// **Guard taint.** Seed = every relation that is some rule's head where that
/// rule's body carries a `Guard` literal. Propagate forward along the SAME
/// structural edges (irrespective of polarity — reading a tainted relation
/// through `not` still inherits the taint, spec §6) until the reachable set
/// stops growing (a fixpoint over the NAME set, bounded by `vocab.relations`'
/// size). `vocab.guard_tainted` becomes the reachable set intersected with
/// `derive: true` relations (the field's contract — a base relation is never
/// a member; D1 gives it no "rule closure" to taint).
pub fn check_stratification(vocab: &mut RelVocab) -> Vec<Diagnostic> {
    let nodes: BTreeSet<String> = vocab.relations.keys().cloned().collect();
    let (adjacency, edges) = predicate_edges(vocab, &nodes);
    let scc_of = tarjan_sccs(&nodes, &adjacency);

    let mut out = Vec::new();
    for edge in &edges {
        if !edge.negated {
            continue;
        }
        if scc_of[&edge.from] != scc_of[&edge.to] {
            continue;
        }
        let scc_id = scc_of[&edge.from];
        let mut members: Vec<&str> = scc_of
            .iter()
            .filter(|&(_, &id)| id == scc_id)
            .map(|(name, _)| name.as_str())
            .collect();
        members.sort_unstable();
        out.push(diag(
            E_DATALOG_UNSTRATIFIED,
            format!(
                "relation(s) `{}` form a negation cycle: a rule's negated body literal may never \
                 depend on itself, even indirectly through other rules (dsl 0.3.0 §7.2) — \
                 stratify the rules so recursion never crosses `not`",
                members.join("`, `")
            ),
            edge.span,
        ));
    }

    vocab.guard_tainted = compute_guard_taint(vocab, &adjacency);
    out
}

/// One structural predicate-dependency edge (§7.2): `from` is a body atom's
/// relation, `to` is the rule's head relation ("`to`'s rule reads `from`"),
/// `negated` is true iff the body atom is `Neg`, and `span` is the owning
/// rule's span (where a stratification diagnostic for this edge is anchored).
struct PredEdge {
    from: String,
    to: String,
    negated: bool,
    span: Span,
}

/// Build the predicate-dependency graph (§7.2) over `vocab.rules`: an
/// adjacency map (`from` → every `to` it reaches, for [`tarjan_sccs`] and taint
/// propagation) plus the flat edge list (for the per-edge negation-cycle
/// check). Only atoms naming a `node` (a declared relation) contribute an
/// edge — an entity-kind atom (`K(X)`) or an atom naming an undeclared
/// relation is invisible to this graph (see [`check_stratification`]'s doc).
fn predicate_edges(
    vocab: &RelVocab,
    nodes: &BTreeSet<String>,
) -> (BTreeMap<String, Vec<String>>, Vec<PredEdge>) {
    let mut adjacency: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut edges = Vec::new();
    for rule_decl in &vocab.rules {
        let head = &rule_decl.rule.head.relation;
        if !nodes.contains(head) {
            continue;
        }
        for lit in &rule_decl.rule.body {
            let (atom, negated) = match lit {
                BodyLiteral::Pos(a) => (a, false),
                BodyLiteral::Neg(a) => (a, true),
                BodyLiteral::Guard { .. } | BodyLiteral::Cmp { .. } => continue,
            };
            if !nodes.contains(&atom.relation) {
                continue;
            }
            adjacency
                .entry(atom.relation.clone())
                .or_default()
                .push(head.clone());
            edges.push(PredEdge {
                from: atom.relation.clone(),
                to: head.clone(),
                negated,
                span: rule_decl.span,
            });
        }
    }
    (adjacency, edges)
}

/// Tarjan's strongly-connected-components algorithm (§7.2), iterative: an
/// explicit `work` stack of `(node, next-child-index)` frames simulates the
/// recursive DFS call stack (a rule set is always small, but never recurse
/// unboundedly on user input). Every node in `nodes` gets an SCC id, even an
/// isolated one with no edges (its own singleton component) — the caller
/// only cares whether two DISTINCT nodes (or a node and itself via a direct
/// self-edge) share an id.
fn tarjan_sccs(
    nodes: &BTreeSet<String>,
    adjacency: &BTreeMap<String, Vec<String>>,
) -> BTreeMap<String, usize> {
    let empty: Vec<String> = Vec::new();
    let mut index_of: BTreeMap<String, usize> = BTreeMap::new();
    let mut lowlink: BTreeMap<String, usize> = BTreeMap::new();
    let mut on_stack: BTreeSet<String> = BTreeSet::new();
    let mut tarjan_stack: Vec<String> = Vec::new();
    let mut scc_of: BTreeMap<String, usize> = BTreeMap::new();
    let mut next_index = 0usize;
    let mut next_scc = 0usize;

    for start in nodes {
        if index_of.contains_key(start) {
            continue;
        }
        let mut work: Vec<(String, usize)> = vec![(start.clone(), 0)];
        index_of.insert(start.clone(), next_index);
        lowlink.insert(start.clone(), next_index);
        next_index += 1;
        tarjan_stack.push(start.clone());
        on_stack.insert(start.clone());

        while !work.is_empty() {
            let top = work.len() - 1;
            let node = work[top].0.clone();
            let child_i = work[top].1;
            let neighbors = adjacency.get(&node).unwrap_or(&empty);
            if child_i < neighbors.len() {
                work[top].1 += 1;
                let next = neighbors[child_i].clone();
                if !index_of.contains_key(&next) {
                    index_of.insert(next.clone(), next_index);
                    lowlink.insert(next.clone(), next_index);
                    next_index += 1;
                    tarjan_stack.push(next.clone());
                    on_stack.insert(next.clone());
                    work.push((next, 0));
                } else if on_stack.contains(&next) {
                    let next_index_val = index_of[&next];
                    if next_index_val < lowlink[&node] {
                        lowlink.insert(node, next_index_val);
                    }
                }
            } else {
                work.pop();
                let node_low = lowlink[&node];
                if let Some((parent, _)) = work.last() {
                    let parent = parent.clone();
                    if node_low < lowlink[&parent] {
                        lowlink.insert(parent, node_low);
                    }
                }
                if node_low == index_of[&node] {
                    loop {
                        let w = tarjan_stack.pop().expect("SCC root must be on the stack");
                        on_stack.remove(&w);
                        let done = w == node;
                        scc_of.insert(w, next_scc);
                        if done {
                            break;
                        }
                    }
                    next_scc += 1;
                }
            }
        }
    }
    scc_of
}

/// The guard-taint closure (§6, D1: no fixpoint over facts — this is a
/// fixpoint over the NAME set, ≤ `vocab.relations.len()` passes): seed with
/// every relation headed by a rule whose body carries a `Guard` literal, then
/// repeatedly walk `adjacency` forward (any polarity — §6 draws no
/// distinction) until no pass adds a new name. The result is filtered to
/// `derive: true` relations, matching [`RelVocab::guard_tainted`]'s contract.
fn compute_guard_taint(
    vocab: &RelVocab,
    adjacency: &BTreeMap<String, Vec<String>>,
) -> BTreeSet<String> {
    let mut tainted: BTreeSet<String> = vocab
        .rules
        .iter()
        .filter(|r| {
            r.rule
                .body
                .iter()
                .any(|lit| matches!(lit, BodyLiteral::Guard { .. }))
        })
        .map(|r| r.rule.head.relation.clone())
        .collect();

    loop {
        let mut grew = false;
        for (from, reached) in adjacency {
            if !tainted.contains(from) {
                continue;
            }
            for to in reached {
                if tainted.insert(to.clone()) {
                    grew = true;
                }
            }
        }
        if !grew {
            break;
        }
    }

    tainted
        .into_iter()
        .filter(|name| vocab.relations.get(name).is_some_and(|decl| decl.derive))
        .collect()
}
