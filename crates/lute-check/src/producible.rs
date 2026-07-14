//! `producible(R)` structural rule-dependency walk + the relational-
//! objective-liveness diagnostic (dsl 0.4.0 §4.2/§B, connectivity T7).
//!
//! D1 applies throughout, but INVERTED from `rel_schema.rs`'s usual
//! quarantine note: `producible()` is a boolean SATISFIABILITY walk over
//! declared rule STRUCTURE — it never runs the real Datalog fixpoint or
//! evaluates facts against runtime state, so it sits entirely OUTSIDE the
//! D1 quarantine (spec §4.2), unlike `decide()`'s CEL-evaluation fragment.
//!
//! ## The relational-objective-liveness gap (§1/§4.2)
//! An `<objective done="holds(R(...))">` gated by a relational fact query is
//! always `Undecided` under `decide()` (R5, `decide.rs`) — `decide()` never
//! reads the fact store. A genuinely unreachable relationally-gated
//! objective therefore passes `check` clean today. §4.2 closes this as a
//! SECOND-ORDER consequence of `producible()`: `producible(R) == false`
//! means every ground instance of `R` is structurally dead, so ANY
//! `<objective done>` querying `R` via `holds`/`count`/`validAt` is provably
//! dead too — relation-level, not argument-level (sound, deliberately
//! incomplete, the same tradeoff `W-OVERLAP-ARMS` already makes).
//!
//! ## Naive approach is unsound (why this isn't a plain assert-site search)
//! A `derive: true` relation can never be `::assert`ed at all
//! (`E-DERIVED-WRITE`), so tracing `::assert` sites for the gating relation
//! DIRECTLY finds an empty set for every derived relation by construction —
//! wrongly flagging any derived-relation-gated objective as dead. The fix is
//! walking the RULE-DEPENDENCY graph down to base relations instead (reusing
//! `datalog_check.rs`'s `predicate_edges` extraction pattern for rule/atom
//! structure, though not its Tarjan pass — this is a monotone least-fixpoint,
//! not an SCC search).
use std::collections::BTreeMap;
use std::collections::BTreeSet;

use cel_parser::ast::{CallExpr, Expr, IdedExpr, ListExpr, SelectExpr};
use cel_parser::reference::Val;
use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_syntax::ast::{Arm, Document, Node, Objective};
use lute_syntax::datalog::BodyLiteral;

use crate::cel_expand::{expand_cel, DefTable};
use crate::decide::{decide, decide_slot, DecideCtx, Decided};
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
    let seeded: BTreeSet<&str> = vocab.facts.iter().map(|f| f.fact.relation.as_str()).collect();
    let mut result: BTreeMap<String, bool> = BTreeMap::new();
    for (name, decl) in &vocab.relations {
        let value = if decl.derive {
            false // filled by the fixpoint below, monotone false -> true only.
        } else {
            seeded.contains(name.as_str())
                || decl.reserved
                || live_assert_relations.contains(name)
        };
        result.insert(name.clone(), value);
    }
    loop {
        let mut changed = false;
        for rule_decl in &vocab.rules {
            let head = &rule_decl.rule.head.relation;
            let Some(decl) = vocab.relations.get(head) else { continue };
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
                BodyLiteral::Neg(_) | BodyLiteral::Guard { .. } | BodyLiteral::Cmp { .. } => true,
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

/// Scan every `<objective done>` in `doc.quests` (recursively through
/// `<match>`/`<branch>`/`<hub>`/`<on>` bodies, mirroring `reachability.rs`'s
/// `walk_reach` shape) for a PROVABLY dead guard once every fact-query call
/// (`holds`/`count`/`validAt`) over a `producible() == false` relation is
/// constant-folded to its empty-result value — a SOUND PARTIAL EVALUATOR,
/// not a top-level-only or naive nested-call scan (both are unsound: a
/// top-level-only match misses `count(R(...)) > 0`, the ordinary boolean
/// use of `count`; a naive "any nested dead call" scan false-positives on
/// `holds(deadR(x)) || holds(liveR(y))`, which is very much alive).
///
/// The algorithm ([`substitute_dead_fact_queries`] + the EXISTING
/// `decide()` R1–R5 machinery, `decide.rs`):
/// 1. In `done`'s CEL AST, every fact-query call whose relation is
///    `producible() == false` is replaced by its empty-result constant:
///    `holds(...)`/`validAt(...)` → `false`, `count(...)` → `0`. A
///    producible OR undeclared-relation query is left untouched (still
///    `Undecided` per `decide()`'s existing R5 firewall).
/// 2. The substituted AST is handed to the UNMODIFIED `decide()` —
///    reusing R1–R5 wholesale means `&&`/`||`/comparisons/negation compose
///    exactly as they always have (Kleene short-circuit for `&&`/`||`, R2
///    domain reasoning for state-path comparisons, …), with zero new
///    boolean-composition logic to get wrong here.
/// 3. The objective is flagged ONLY when the WHOLE substituted guard
///    decides `Some(Decided::Bool(false))` — never on `Undecided` or
///    `true`. Worked cases: `count(deadR) > 0` → `0 > 0` → `false` → DEAD;
///    `count(deadR) >= 0` → `0 >= 0` → `true` → NOT dead (why constant
///    substitution beats scanning for a nested dead call — the SAME dead
///    relation is fine under `>=0`, fatal under `>0`); `holds(deadR) && x`
///    → `false && x` → `false` → DEAD (AND short-circuits regardless of
///    `x`); `holds(deadR) || holds(liveR)` → `false || Undecided` →
///    `Undecided` → NOT dead (OR never proves false from one dead arm).
///
/// Rides the existing `E-OBJECTIVE-UNSATISFIABLE` code as a THIRD standalone
/// cause (dsl 0.4.0 §5.3's established "name whichever cause holds"
/// precedent, alongside `E-QUEST-UNREACHABLE`'s dead-`start`/true-`fail` and
/// this same code's dead-`done`-literal cause) — never a new diagnostic
/// shape. The message carries the verbatim "under your declared routes"
/// hedge (§2.6/§4.2 rule 3): "dead" here means dead given the DECLARED
/// `after`/assert graph, never an unconditional "can never happen in play"
/// claim (posture A-hybrid — the engine is not bound to honor the graph).
///
/// `defs`/`ctx` mirror `reachability.rs`'s `check_objective_reach` exactly
/// (same `DefTable`/`DecideCtx` shape, `ctx.dollar` MUST be `None` — no `$`
/// is in scope at an `<objective>`'s attrs) so `@def`-wrapped guards and
/// state-path R2 composition resolve identically to the ordinary per-file
/// reachability pass, not a degraded copy.
pub fn scan_objective_liveness(
    doc: &Document,
    producible: &BTreeMap<String, bool>,
    defs: &DefTable<'_>,
    ctx: &DecideCtx<'_>,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for quest in &doc.quests {
        walk_objectives(&quest.body, producible, defs, ctx, &mut out);
    }
    out
}

/// Every `<quest id>` in `doc` with at least one PROVABLY dead REQUIRED
/// (`!optional`) `<objective>` — dsl 0.4.0 §5.3/§4.2's `E-OBJECTIVE-
/// UNSATISFIABLE` firing condition (the scalar `done`-literal cause,
/// identical to [`crate::reachability::check_objective_reach`]'s own
/// `decide_slot` check, OR the relational never-producible-relation cause,
/// this module's [`dead_guard`]) computed as STRUCTURED boolean data —
/// never derived by re-inspecting either cause's own emitted diagnostic
/// code or message text.
///
/// dsl 0.4.0 §8.2 rule C4 deliberately suppresses a standalone
/// `E-QUEST-UNREACHABLE` for this cause (it rides as a note on
/// `E-OBJECTIVE-UNSATISFIABLE` instead,
/// [`crate::reachability::REQUIRED_QUEST_NOTE`]) —
/// [`crate::connectivity::check_reachability`]'s `completed(Q)` needs the
/// boolean FACT itself, decoupled from that (intentionally suppressed)
/// diagnostic, so it consumes this set directly: connectivity design spec
/// §4.2's "relational-objective-liveness gap ... subsumed as a
/// second-order consequence" is closed for the SCALAR cause too, the exact
/// gap C4's suppression opened. This function emits NO diagnostic itself
/// and never causes C4's standalone `E-QUEST-UNREACHABLE` to fire — it is
/// pure data, consumed by a DIFFERENT engine (connectivity graph
/// reachability) than the one that emits `E-OBJECTIVE-UNSATISFIABLE`.
///
/// An OPTIONAL dead objective never marks its quest — C4's quest-level
/// consequence is REQUIRED-only; the quest can still complete via its
/// other objectives. Provable-only discipline: an UNDECIDED `done` (or one
/// this pass cannot resolve either way) never marks the quest — only a
/// PROVEN-dead required objective does.
///
/// `ambiguous_quest_ids` (Task 6 review-2's set, same convention
/// [`crate::connectivity::unreachable_quest_ids`] follows) is skipped
/// here too: an ambiguous id might carry one dead declaration and one
/// alive one, and [`crate::connectivity::check_reachability`]'s own
/// precedence already resolves an ambiguous `completed(Q)` to `Unknown`
/// before ever consulting the unreachable set — but skipping it here too
/// keeps this function's OWN returned set meaning exactly what its name
/// says, never silently including an id whose OTHER declaration is alive.
///
/// An empty quest id is skipped (that quest's own `E-QUEST-ID-MISSING`
/// problem, not this pass's).
pub fn dead_required_objective_quests(
    doc: &Document,
    producible: &BTreeMap<String, bool>,
    ambiguous_quest_ids: &BTreeSet<String>,
    defs: &DefTable<'_>,
    ctx: &DecideCtx<'_>,
) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for quest in &doc.quests {
        if quest.id.is_empty() || ambiguous_quest_ids.contains(&quest.id) {
            continue;
        }
        if has_dead_required_objective(&quest.body, producible, defs, ctx) {
            out.insert(quest.id.clone());
        }
    }
    out
}

/// Recurse a quest body for a REQUIRED `<objective>` whose `done` is
/// provably dead — mirrors [`walk_objectives`]'s traversal shape exactly
/// (same node kinds recursed, same leaf kinds skipped) so this structural
/// signal and the diagnostic-emitting walk can never disagree about which
/// objectives exist in a quest.
fn has_dead_required_objective(
    nodes: &[Node],
    producible: &BTreeMap<String, bool>,
    defs: &DefTable<'_>,
    ctx: &DecideCtx<'_>,
) -> bool {
    nodes.iter().any(|node| match node {
        Node::Objective(o) => {
            (!o.optional && objective_is_dead(o, producible, defs, ctx))
                || has_dead_required_objective(&o.body, producible, defs, ctx)
        }
        Node::Match(m) => m.arms.iter().any(|arm| {
            let body = match arm {
                Arm::When { body, .. } | Arm::Otherwise { body, .. } => body,
            };
            has_dead_required_objective(body, producible, defs, ctx)
        }),
        Node::Branch(b) => b
            .choices
            .iter()
            .any(|c| has_dead_required_objective(&c.body, producible, defs, ctx)),
        Node::Hub(h) => h
            .choices
            .iter()
            .any(|c| has_dead_required_objective(&c.body, producible, defs, ctx)),
        Node::On(o) => has_dead_required_objective(&o.body, producible, defs, ctx),
        Node::Line(_)
        | Node::Directive(_)
        | Node::Set(_)
        | Node::Timeline(_)
        | Node::Assert(_)
        | Node::Retract(_) => false,
    })
}

/// `true` iff `o.done` is provably dead under EITHER named cause dsl
/// 0.4.0 §5.3/§4.2 recognizes: the scalar `done`-literal cause
/// (`decide_slot` deciding false directly, identical to
/// [`crate::reachability::check_objective_reach`]'s own check) OR the
/// relational never-producible-relation cause ([`dead_guard`], whose own
/// gate (b) already excludes the scalar case) — the two causes are
/// disjoint by construction (§4.2/C4's own "whichever standalone cause
/// holds" precedent), so this is a plain OR, never double-counted.
fn objective_is_dead(
    o: &Objective,
    producible: &BTreeMap<String, bool>,
    defs: &DefTable<'_>,
    ctx: &DecideCtx<'_>,
) -> bool {
    if matches!(decide_slot(&o.done.raw, defs, ctx), Some(Decided::Bool(false))) {
        return true;
    }
    let mut dead_relations = BTreeSet::new();
    dead_guard(&o.done.raw, producible, defs, ctx, &mut dead_relations)
}

fn walk_objectives(
    nodes: &[Node],
    producible: &BTreeMap<String, bool>,
    defs: &DefTable<'_>,
    ctx: &DecideCtx<'_>,
    out: &mut Vec<Diagnostic>,
) {
    for node in nodes {
        match node {
            Node::Objective(o) => {
                let mut dead_relations = BTreeSet::new();
                if dead_guard(&o.done.raw, producible, defs, ctx, &mut dead_relations) {
                    out.push(diag(
                        crate::reachability::E_OBJECTIVE_UNSATISFIABLE,
                        dead_relation_message(!o.optional, &o.done.raw, &dead_relations),
                        o.span,
                    ));
                }
                walk_objectives(&o.body, producible, defs, ctx, out);
            }
            Node::Match(m) => {
                for arm in &m.arms {
                    let body = match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => body,
                    };
                    walk_objectives(body, producible, defs, ctx, out);
                }
            }
            Node::Branch(b) => {
                for choice in &b.choices {
                    walk_objectives(&choice.body, producible, defs, ctx, out);
                }
            }
            Node::Hub(h) => {
                for choice in &h.choices {
                    walk_objectives(&choice.body, producible, defs, ctx, out);
                }
            }
            Node::On(o) => walk_objectives(&o.body, producible, defs, ctx, out),
            Node::Line(_)
            | Node::Directive(_)
            | Node::Set(_)
            | Node::Timeline(_)
            | Node::Assert(_)
            | Node::Retract(_) => {}
        }
    }
}

/// `true` iff `raw` — after `@def`-expansion ([`expand_cel`], matching
/// `decide_slot`'s own pipeline) and every `producible() == false`
/// fact-query call constant-folded ([`substitute_dead_fact_queries`]) —
/// decides `Some(Decided::Bool(false))` under the UNMODIFIED `decide()`
/// (R1–R5), AND that dead-relation substitution is LOAD-BEARING for the
/// false result (dsl 0.4.0 §5.3/§4.2: the relational cause rides as a
/// THIRD named cause, one diagnostic per objective, naming whichever
/// standalone cause holds — never a duplicate of a non-relational cause
/// the ordinary reachability pass already owns). Two gates, both required:
/// (a) at least one dead-relation substitution actually happened
/// (`!dead_relations.is_empty()` post-substitution) — `done="false"` or
/// `done="0 > 1"` have NO fact-query at all, substitute nothing, and must
/// never contribute a bogus relational diagnostic naming an empty relation
/// set; (b) the ORIGINAL (pre-substitution) guard is not ALREADY `false`
/// under `decide()` — `done="false && holds(deadR)"` is dead because of
/// the literal `false`, not because `deadR` is dead (mirrors 0.5.2's
/// `E-UNSET-LITERAL` causality-ownership discipline: only the load-bearing
/// cause gets named). Malformed CEL (already reported elsewhere) yields
/// `false` (never a guess). `dead_relations` collects every relation the
/// substitution actually folded, for the diagnostic message; left empty on
/// a `false` return.
fn dead_guard(
    raw: &str,
    producible: &BTreeMap<String, bool>,
    defs: &DefTable<'_>,
    ctx: &DecideCtx<'_>,
    dead_relations: &mut BTreeSet<String>,
) -> bool {
    let mut stack = Vec::new();
    let expanded = expand_cel(raw, defs, Some("$"), &mut stack).unwrap_or_else(|_| raw.to_string());
    let mut arena = lute_cel::CelArena::default();
    let Some(handle) = lute_cel::parse_slot_marked_refs(&mut arena, &expanded) else {
        return false;
    };
    let Some(ided) = arena.get(handle) else { return false };
    // Gate (b): decide the ORIGINAL guard first, WITHOUT substitution. If
    // it is already provably false on its own, that non-relational cause
    // is load-bearing and owned by the ordinary reachability pass — the
    // relational cause must not also fire (no duplicate diagnostic).
    if matches!(decide(&ided.expr, ctx), Some(Decided::Bool(false))) {
        return false;
    }
    let substituted = substitute_dead_fact_queries(&ided.expr, producible, dead_relations);
    // Gate (a): a dead-relation substitution must have actually happened —
    // otherwise there is nothing relational to name.
    if dead_relations.is_empty() {
        return false;
    }
    if matches!(decide(&substituted, ctx), Some(Decided::Bool(false))) {
        true
    } else {
        dead_relations.clear();
        false
    }
}

/// Build a STRUCTURAL copy of `expr`, replacing every well-shaped fact-query
/// call ([`crate::cel_resolve::is_profile_fact_query`]) over a
/// `producible() == false` relation with its empty-result constant:
/// `holds`/`validAt` → `Val::Boolean(false)`, `count` → `Val::Int(0)` — the
/// value EVERY ground instance of a never-producible relation's query
/// returns, by definition (`holds`/`count`: no fact ever asserted;
/// `validAt`: no fact ever holds at any instant). A fact query over a
/// `now()` (no relation pattern), an UNDECLARED relation (`producible` has
/// no entry — already `E-RELATION-UNKNOWN`'d), or a relation that IS
/// producible is left UNTOUCHED (still `Undecided` per `decide()`'s R5
/// firewall). Every other `Call` (operators — `&&`/`||`/comparisons/`!`/
/// `in`/`?:` are ALL synthetic `Call`s here, same as `Ident`/`Literal`
/// elsewhere) recurses into its target/args, mirroring
/// `cel_resolve.rs`'s `check_fact_queries` recursion shape exactly so a
/// fact query nested inside an operator, a `List` element, or a `Select`
/// operand is still found and substituted.
fn substitute_dead_fact_queries(
    expr: &Expr,
    producible: &BTreeMap<String, bool>,
    dead_relations: &mut BTreeSet<String>,
) -> Expr {
    match expr {
        Expr::Call(c) => {
            if crate::cel_resolve::is_profile_fact_query(c) {
                if let Some(relation) = pattern_relation(c) {
                    if producible.get(&relation) == Some(&false) {
                        dead_relations.insert(relation);
                        let empty = if c.func_name == "count" {
                            Val::Int(0)
                        } else {
                            Val::Boolean(false)
                        };
                        return Expr::Literal(empty);
                    }
                }
                return expr.clone(); // now(), unresolved, or a live/unknown relation.
            }
            Expr::Call(CallExpr {
                func_name: c.func_name.clone(),
                target: c.target.as_ref().map(|t| {
                    Box::new(IdedExpr {
                        id: t.id,
                        expr: substitute_dead_fact_queries(&t.expr, producible, dead_relations),
                    })
                }),
                args: c
                    .args
                    .iter()
                    .map(|a| IdedExpr {
                        id: a.id,
                        expr: substitute_dead_fact_queries(&a.expr, producible, dead_relations),
                    })
                    .collect(),
            })
        }
        Expr::List(list) => Expr::List(ListExpr {
            elements: list
                .elements
                .iter()
                .map(|e| IdedExpr {
                    id: e.id,
                    expr: substitute_dead_fact_queries(&e.expr, producible, dead_relations),
                })
                .collect(),
        }),
        Expr::Select(sel) => Expr::Select(SelectExpr {
            operand: Box::new(IdedExpr {
                id: sel.operand.id,
                expr: substitute_dead_fact_queries(&sel.operand.expr, producible, dead_relations),
            }),
            field: sel.field.clone(),
            test: sel.test,
        }),
        other => other.clone(),
    }
}

/// The relation name a well-shaped fact-query call's pattern arg names —
/// `holds(R(...))`/`count(R(...))`/`validAt(R(...), ...)`'s `args[0]` is
/// guaranteed `Expr::Call` by [`crate::cel_resolve::is_profile_fact_query`];
/// `now()` carries no pattern and yields `None`.
fn pattern_relation(c: &CallExpr) -> Option<String> {
    match &c.args.first()?.expr {
        Expr::Call(pattern) => Some(pattern.func_name.clone()),
        _ => None,
    }
}

/// `E-OBJECTIVE-UNSATISFIABLE` message for the non-producible-relation cause
/// (spec §4.2 rule 2/3): quotes the `done` predicate's raw text (matching
/// `objective_unsat_message`'s style), names every relation the
/// substitution actually folded (deterministic order, `BTreeSet`), and
/// carries the verbatim "under your declared routes" hedge (§2.6).
/// `required` appends [`crate::reachability::REQUIRED_QUEST_NOTE`]
/// verbatim, mirroring the existing dead-`done`-literal cause's C4
/// treatment.
fn dead_relation_message(required: bool, raw: &str, dead_relations: &BTreeSet<String>) -> String {
    let relations: Vec<&str> = dead_relations.iter().map(String::as_str).collect();
    let mut msg = format!(
        "`done` predicate `{}` queries relation(s) `{}`, which is unreachable under your \
         declared routes: no `facts:` seed, no `reserved` tier, and no rule closure over \
         already-producible relations can ever populate it, so the objective can never \
         complete on any run",
        raw.trim(),
        relations.join("`, `")
    );
    if required {
        msg.push_str(crate::reachability::REQUIRED_QUEST_NOTE);
    } else {
        msg.push_str(" (dsl 0.4.0 §4.2/§5.3)");
    }
    msg
}

/// Build a `Layer::Logic` error diagnostic (a §4.2 project-wide reachability
/// consequence, same layer `reachability.rs`'s own `E-OBJECTIVE-UNSATISFIABLE`
/// emission uses).
fn diag(code: &str, message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity: Severity::Error,
        message,
        span,
        layer: Layer::Logic,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lute_manifest::relations::RelationDecl;

    fn base_relation(reserved: bool) -> RelationDecl {
        RelationDecl {
            args: vec!["c".to_string()],
            tier: Some("run".to_string()),
            derive: false,
            reserved,
            key: vec![],
            malformed_fields: vec![],
        }
    }

    fn derive_relation() -> RelationDecl {
        RelationDecl {
            args: vec!["c".to_string()],
            tier: None,
            derive: true,
            reserved: false,
            key: vec![],
            malformed_fields: vec![],
        }
    }

    fn dummy_span() -> Span {
        Span { byte_start: 0, byte_end: 0, line: 1, column: 1, utf16_range: (0, 0) }
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
        crate::meta::RuleDecl { rule, raw: text.to_string(), span: dummy_span() }
    }

    #[test]
    fn base_relation_with_facts_seed_is_producible() {
        let mut vocab = RelVocab::default();
        vocab.relations.insert("seeded".to_string(), base_relation(false));
        vocab.facts.push(fact("seeded"));
        let result = producible(&vocab, &BTreeSet::new());
        assert_eq!(result.get("seeded"), Some(&true));
    }

    #[test]
    fn base_relation_reserved_is_producible_without_facts() {
        let mut vocab = RelVocab::default();
        vocab.relations.insert("engineOwned".to_string(), base_relation(true));
        let result = producible(&vocab, &BTreeSet::new());
        assert_eq!(result.get("engineOwned"), Some(&true));
    }

    #[test]
    fn base_relation_with_live_assert_is_producible() {
        let mut vocab = RelVocab::default();
        vocab.relations.insert("asserted".to_string(), base_relation(false));
        let live: BTreeSet<String> = ["asserted".to_string()].into_iter().collect();
        let result = producible(&vocab, &live);
        assert_eq!(result.get("asserted"), Some(&true));
    }

    #[test]
    fn base_relation_with_no_producer_is_not_producible() {
        let mut vocab = RelVocab::default();
        vocab.relations.insert("orphan".to_string(), base_relation(false));
        let result = producible(&vocab, &BTreeSet::new());
        assert_eq!(result.get("orphan"), Some(&false));
    }

    #[test]
    fn derived_relation_producible_via_seeded_base() {
        let mut vocab = RelVocab::default();
        vocab.relations.insert("base".to_string(), base_relation(false));
        vocab.relations.insert("derived".to_string(), derive_relation());
        vocab.facts.push(fact("base"));
        vocab.rules.push(rule("derived(X) :- base(X)"));
        let result = producible(&vocab, &BTreeSet::new());
        assert_eq!(result.get("derived"), Some(&true));
    }

    #[test]
    fn derived_relation_never_producible_when_base_has_no_producer() {
        let mut vocab = RelVocab::default();
        vocab.relations.insert("base".to_string(), base_relation(false));
        vocab.relations.insert("derived".to_string(), derive_relation());
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
        vocab.relations.insert("notSeeded".to_string(), base_relation(false));
        vocab.relations.insert("neverSeeded".to_string(), base_relation(false));
        vocab.relations.insert("derived".to_string(), derive_relation());
        vocab.facts.push(fact("notSeeded"));
        vocab.rules.push(rule("derived(X) :- notSeeded(X), not neverSeeded(X)"));
        let result = producible(&vocab, &BTreeSet::new());
        assert_eq!(result.get("derived"), Some(&true));
    }

    #[test]
    fn guard_only_body_never_blocks_producibility() {
        // `derived(X) :- cel("true")` -- no positive atom at all; vacuously
        // satisfiable (the spec's "every positive atom producible" holds
        // trivially over an empty positive set).
        let mut vocab = RelVocab::default();
        vocab.relations.insert("derived".to_string(), derive_relation());
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
        vocab.relations.insert("edge".to_string(), base_relation(false));
        vocab.relations.insert("path".to_string(), derive_relation());
        vocab.facts.push(fact("edge"));
        vocab.rules.push(rule("path(X, Y) :- edge(X, Y)"));
        vocab.rules.push(rule("path(X, Y) :- path(X, Z), edge(Z, Y)"));
        let result = producible(&vocab, &BTreeSet::new());
        assert_eq!(result.get("path"), Some(&true));
    }

    // --- `dead_required_objective_quests` (Defect B: structured
    // dead-required-objective liveness, decoupled from any diagnostic
    // code/message -- consumed directly by
    // `crate::connectivity::check_reachability`'s `completed(Q)`) -------

    fn cel(raw: &str) -> lute_syntax::ast::CelSlot {
        lute_syntax::ast::CelSlot::raw(lute_syntax::ast::CelKind::Condition, raw.to_string(), dummy_span())
    }

    fn objective_node(id: &str, done: &str, optional: bool) -> Node {
        Node::Objective(Objective {
            id: id.to_string(),
            id_span: dummy_span(),
            done: cel(done),
            when: None,
            title: None,
            optional,
            attrs: Vec::new(),
            body: Vec::new(),
            span: dummy_span(),
        })
    }

    fn quest_doc(quest_id: &str, body: Vec<Node>) -> Document {
        Document {
            meta: lute_syntax::ast::Meta { raw_yaml: String::new(), span: dummy_span() },
            title: None,
            shots: Vec::new(),
            quests: vec![lute_syntax::ast::Quest {
                id: quest_id.to_string(),
                id_span: dummy_span(),
                title: None,
                start: None,
                fail: None,
                after: None,
                after_span: dummy_span(),
                attrs: Vec::new(),
                body,
                span: dummy_span(),
            }],
            span: dummy_span(),
        }
    }

    #[test]
    fn scalar_dead_required_objective_marks_quest() {
        let doc = quest_doc("deadQuest", vec![objective_node("o", "false", false)]);
        let (bodies, params) = (BTreeMap::new(), BTreeMap::new());
        let defs = DefTable { bodies: &bodies, params: &params };
        let schema = crate::meta::StateSchema::default();
        let ctx_params = BTreeMap::new();
        let ctx = DecideCtx { schema: &schema, dollar: None, params: &ctx_params };
        let out = dead_required_objective_quests(&doc, &BTreeMap::new(), &BTreeSet::new(), &defs, &ctx);
        assert!(out.contains("deadQuest"), "scalar-dead REQUIRED objective must mark the quest");
    }

    /// C4: an OPTIONAL dead objective never marks its quest -- the quest
    /// can still complete via its other objectives.
    #[test]
    fn scalar_dead_optional_objective_never_marks_quest() {
        let doc = quest_doc("q", vec![objective_node("o", "false", true)]);
        let (bodies, params) = (BTreeMap::new(), BTreeMap::new());
        let defs = DefTable { bodies: &bodies, params: &params };
        let schema = crate::meta::StateSchema::default();
        let ctx_params = BTreeMap::new();
        let ctx = DecideCtx { schema: &schema, dollar: None, params: &ctx_params };
        let out = dead_required_objective_quests(&doc, &BTreeMap::new(), &BTreeSet::new(), &defs, &ctx);
        assert!(out.is_empty(), "an OPTIONAL dead objective must never mark its quest");
    }

    /// The relational never-producible-relation flavor (spec §4.2): a
    /// REQUIRED objective gated on `holds(R(...))` where `producible(R)`
    /// is proven `false` must mark the quest too -- the SAME signal that
    /// fires `E-OBJECTIVE-UNSATISFIABLE`'s relational cause
    /// ([`dead_guard`]), consumed here as data instead of a diagnostic.
    #[test]
    fn relational_dead_required_objective_marks_quest() {
        let doc = quest_doc(
            "deadQuest",
            vec![objective_node("o", "holds(deadRel(\"x\"))", false)],
        );
        let mut producible_map = BTreeMap::new();
        producible_map.insert("deadRel".to_string(), false);
        let (bodies, params) = (BTreeMap::new(), BTreeMap::new());
        let defs = DefTable { bodies: &bodies, params: &params };
        let schema = crate::meta::StateSchema::default();
        let ctx_params = BTreeMap::new();
        let ctx = DecideCtx { schema: &schema, dollar: None, params: &ctx_params };
        let out = dead_required_objective_quests(&doc, &producible_map, &BTreeSet::new(), &defs, &ctx);
        assert!(
            out.contains("deadQuest"),
            "a REQUIRED objective gated on a never-producible relation must mark the quest"
        );
    }

    /// A live objective (or one gated on a producible/undeclared relation)
    /// never marks its quest -- provable-only discipline, never a guess.
    #[test]
    fn live_objective_never_marks_quest() {
        let doc = quest_doc("q", vec![objective_node("o", "true", false)]);
        let (bodies, params) = (BTreeMap::new(), BTreeMap::new());
        let defs = DefTable { bodies: &bodies, params: &params };
        let schema = crate::meta::StateSchema::default();
        let ctx_params = BTreeMap::new();
        let ctx = DecideCtx { schema: &schema, dollar: None, params: &ctx_params };
        let out = dead_required_objective_quests(&doc, &BTreeMap::new(), &BTreeSet::new(), &defs, &ctx);
        assert!(out.is_empty());
    }

    /// Provable-only discipline: an objective gated on a relation this
    /// pass has NO entry for at all (never resolved either way) must stay
    /// `Unknown`, never guessed dead.
    #[test]
    fn undecided_relational_objective_never_marks_quest() {
        let doc = quest_doc(
            "q",
            vec![objective_node("o", "holds(unknownRel(\"x\"))", false)],
        );
        let (bodies, params) = (BTreeMap::new(), BTreeMap::new());
        let defs = DefTable { bodies: &bodies, params: &params };
        let schema = crate::meta::StateSchema::default();
        let ctx_params = BTreeMap::new();
        let ctx = DecideCtx { schema: &schema, dollar: None, params: &ctx_params };
        let out = dead_required_objective_quests(&doc, &BTreeMap::new(), &BTreeSet::new(), &defs, &ctx);
        assert!(out.is_empty());
    }

    /// A dead required objective nested inside a `<branch>` choice must
    /// still be found -- the traversal mirrors [`walk_objectives`]'s own
    /// recursion shape exactly, so the two can never disagree.
    #[test]
    fn dead_objective_nested_in_branch_choice_is_found() {
        let branch = Node::Branch(lute_syntax::ast::Branch {
            id: "b".to_string(),
            attrs: Vec::new(),
            choices: vec![lute_syntax::ast::Choice {
                id: "c".to_string(),
                label: "c".to_string(),
                when: None,
                attrs: Vec::new(),
                body: vec![objective_node("o", "false", false)],
                span: dummy_span(),
            }],
            span: dummy_span(),
        });
        let doc = quest_doc("deadQuest", vec![branch]);
        let (bodies, params) = (BTreeMap::new(), BTreeMap::new());
        let defs = DefTable { bodies: &bodies, params: &params };
        let schema = crate::meta::StateSchema::default();
        let ctx_params = BTreeMap::new();
        let ctx = DecideCtx { schema: &schema, dollar: None, params: &ctx_params };
        let out = dead_required_objective_quests(&doc, &BTreeMap::new(), &BTreeSet::new(), &defs, &ctx);
        assert!(out.contains("deadQuest"));
    }

    /// Task 6 review-2 provable-only precedent, mirrored here: an
    /// ambiguous quest id (2+ declarations) is never marked, even when
    /// THIS declaration's own objective is provably dead -- a DIFFERENT
    /// declaration of the same id might be alive.
    #[test]
    fn ambiguous_quest_id_never_marked() {
        let doc = quest_doc("q", vec![objective_node("o", "false", false)]);
        let ambiguous: BTreeSet<String> = ["q".to_string()].into_iter().collect();
        let (bodies, params) = (BTreeMap::new(), BTreeMap::new());
        let defs = DefTable { bodies: &bodies, params: &params };
        let schema = crate::meta::StateSchema::default();
        let ctx_params = BTreeMap::new();
        let ctx = DecideCtx { schema: &schema, dollar: None, params: &ctx_params };
        let out = dead_required_objective_quests(&doc, &BTreeMap::new(), &ambiguous, &defs, &ctx);
        assert!(out.is_empty());
    }
}
