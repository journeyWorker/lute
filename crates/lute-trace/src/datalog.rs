//! The stratified Datalog least-fixpoint (cel-and-facts.md) — the ONE
//! evaluator `lute run` / `lute play` (the reference runner) and `lute trace`
//! / `lute test` (dsl 0.22.0 §6, `derive: true`) share, so the toolchain can
//! never disagree with itself about what a project's rules conclude.
//!
//! A [`Program`] is the rule set, parsed from either surface it arrives on:
//! the compiled artifact's `rules` array ([`Program::from_ir`], the runner)
//! or the checker's merged relational vocabulary ([`Program::from_vocab`],
//! trace). [`Program::fixpoint`] evaluates it over a base fact set, stratum by
//! stratum; a rule-body CEL guard reads scalar state through the same
//! [`crate::eval::eval`] every other guard uses. [`Program::explain`] answers
//! "why (not)" for one ground atom (`lute play --explain`).
//!
//! Three-valued honesty: trace state may be unknown, so a rule guard can
//! decide neither way. Such a rule instance derives nothing, and its head
//! relation — with every relation that depends on it — is reported
//! [`Closure::undecided`], carrying the atoms that would decide it. The
//! runner, whose state is always ground, never produces one.

use std::collections::{BTreeMap, BTreeSet};

use lute_cel::CelArena;
use lute_check::RelVocab;
use serde_json::Value as Json;

use crate::eval::{eval, EffectiveState, EvalEnv, FactStore};
use crate::value::{UnresolvedAtom, Value};

/// A ground fact: relation and argument constants.
pub type Fact = (String, Vec<String>);

/// A variable binding produced by the body join.
pub type Binding = BTreeMap<String, String>;

/// Closed entity kinds, name → members. A rule-body atom naming a kind is a
/// membership test (`companion(P)` binds `P` to each member), never a fact
/// lookup: kinds and relations share one predicate namespace
/// (`E-KIND-NAME-CLASH`), and no fact is ever asserted under a kind's name.
pub type Kinds = BTreeMap<String, Vec<String>>;

/// A rule term: a variable (bound during the join) or a ground constant.
#[derive(Clone, Debug, PartialEq)]
pub enum Term {
    Var(String),
    Const(String),
}

/// A rule atom: a relation applied to terms.
#[derive(Clone, Debug, PartialEq)]
pub struct Atom {
    pub rel: String,
    pub terms: Vec<Term>,
}

/// One rule-body literal: atom / negated atom / comparison / scalar guard.
#[derive(Clone, Debug, PartialEq)]
pub enum Lit {
    Atom { atom: Atom, negated: bool },
    Cmp { lhs: Term, rhs: Term, negated: bool },
    Guard { cel: String },
}

/// A parsed rule `head :- body`, with its source text.
#[derive(Clone, Debug, PartialEq)]
pub struct Rule {
    pub head: Atom,
    pub body: Vec<Lit>,
    pub raw: String,
}

/// A stratified rule set.
#[derive(Clone, Debug, Default)]
pub struct Program {
    rules: Vec<Rule>,
    /// Least stratum per derived relation.
    strata: BTreeMap<String, usize>,
    /// Closed entity kinds the rule bodies may name as domain predicates.
    kinds: Kinds,
}

/// The least fixpoint over one base fact set.
#[derive(Clone, Debug, Default)]
pub struct Closure {
    /// `base ∪ derive(base)`.
    pub facts: BTreeSet<Fact>,
    /// Per derived fact, the fixpoint round that first produced it (base
    /// facts are absent = round 0). A proof only cites strictly earlier
    /// premises, so [`Program::explain`] is well-founded.
    rank: BTreeMap<Fact, usize>,
    /// Derived relations a rule guard could not decide for (trace only),
    /// with the atoms that would decide them. Propagated to every relation
    /// that depends on one, positively or negatively.
    pub undecided: BTreeMap<String, Vec<UnresolvedAtom>>,
}

/// Why one ground atom holds: it is a base fact, or a rule instance whose
/// premises hold.
#[derive(Clone, Debug, PartialEq)]
pub enum Proof {
    Base(Fact),
    Derived {
        fact: Fact,
        rule: String,
        premises: Vec<Premise>,
    },
}

/// One premise of a proof or of a failed attempt.
#[derive(Clone, Debug, PartialEq)]
pub enum Premise {
    /// A positive atom that holds, with its own support.
    Holds(Box<Proof>),
    /// A positive atom that does not hold (`atom` may keep unbound
    /// variables when no binding reached it); `why` explains a ground
    /// derived atom's failure in turn.
    Missing {
        atom: String,
        why: Vec<Attempt>,
    },
    /// `not X` with `X` absent.
    Absent(Fact),
    /// `not X` with `X` present — the premise fails; `X`'s own support.
    Present(Box<Proof>),
    /// A comparison / guard, rendered with the binding substituted.
    Test { text: String, holds: Option<bool> },
    /// A literal after the first failing positive atom: never reached.
    Unreached(String),
}

/// One rule that could conclude the asked atom, and how far it got.
#[derive(Clone, Debug, PartialEq)]
pub struct Attempt {
    pub rule: String,
    pub premises: Vec<Premise>,
}

/// [`Program::explain`]'s answer.
#[derive(Clone, Debug, PartialEq)]
pub enum Explanation {
    Holds(Proof),
    /// Not derivable: every rule whose head matches, with its failing
    /// premises. Empty when `derived` is false and nothing concludes it.
    Fails { derived: bool, attempts: Vec<Attempt> },
}

impl Program {
    /// Parse the artifact / project-index `rules` array (IR `RuleEntry`).
    pub fn from_ir(rules: Option<&Json>) -> Self {
        let rules = rules
            .and_then(Json::as_array)
            .map(|arr| arr.iter().filter_map(ir_rule).collect())
            .unwrap_or_default();
        Self::new(rules)
    }

    /// The checker's merged rules (`RelVocab.rules`), lowered exactly as
    /// `lute-compile` lowers them to the IR (`RuleTerm::Bool` → a `"true"` /
    /// `"false"` constant; a rule reading entity-indexed state by a rule
    /// variable grounded per member, `lute_check::evaluable_rules`), with the
    /// vocabulary's closed entity kinds.
    pub fn from_vocab(vocab: &RelVocab) -> Self {
        let rules = lute_check::evaluable_rules(vocab)
            .iter()
            .map(|r| Rule {
                head: syntax_atom(&r.rule.head),
                body: r.rule.body.iter().map(syntax_lit).collect(),
                raw: r.raw.clone(),
            })
            .collect();
        Self::new(rules).with_kinds(closed_kinds(&vocab.kinds))
    }

    fn new(rules: Vec<Rule>) -> Self {
        let derived: BTreeSet<String> = rules.iter().map(|r| r.head.rel.clone()).collect();
        let strata = compute_strata(&rules, &derived);
        Self {
            rules,
            strata,
            kinds: Kinds::new(),
        }
    }

    /// The closed entity kinds rule bodies read as membership tests.
    pub fn with_kinds(mut self, kinds: Kinds) -> Self {
        self.kinds = kinds;
        self
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// `true` when some rule concludes `rel`.
    pub fn derives(&self, rel: &str) -> bool {
        self.strata.contains_key(rel)
    }

    /// The stratified least fixpoint `base ∪ derive(base)`. `state` is what
    /// rule-body guards read.
    pub fn fixpoint(&self, base: &BTreeSet<Fact>, state: &EffectiveState<'_>) -> Closure {
        let mut out = Closure {
            facts: base.clone(),
            ..Closure::default()
        };
        if self.rules.is_empty() {
            return out;
        }
        let max = self.strata.values().copied().max().unwrap_or(0);
        let mut round = 0;
        for s in 0..=max {
            loop {
                round += 1;
                let mut new: BTreeSet<Fact> = BTreeSet::new();
                for rule in &self.rules {
                    if self.strata.get(&rule.head.rel).copied().unwrap_or(0) != s {
                        continue;
                    }
                    let mut unknown = Vec::new();
                    for binding in solve_body(&rule.body, &out.facts, &self.kinds, state, &mut unknown) {
                        if let Some(args) = ground_atom(&rule.head, &binding) {
                            let fact = (rule.head.rel.clone(), args);
                            if !out.facts.contains(&fact) {
                                new.insert(fact);
                            }
                        }
                    }
                    if !unknown.is_empty() {
                        let atoms = out.undecided.entry(rule.head.rel.clone()).or_default();
                        for a in unknown {
                            if !atoms.contains(&a) {
                                atoms.push(a);
                            }
                        }
                    }
                }
                if new.is_empty() {
                    break;
                }
                for f in &new {
                    out.rank.insert(f.clone(), round);
                }
                out.facts.extend(new);
            }
        }
        self.propagate_undecided(&mut out.undecided);
        out
    }

    /// A relation fed (positively or negatively) by an undecided one is
    /// undecided too.
    fn propagate_undecided(&self, undecided: &mut BTreeMap<String, Vec<UnresolvedAtom>>) {
        if undecided.is_empty() {
            return;
        }
        loop {
            let mut changed = false;
            for rule in &self.rules {
                for lit in &rule.body {
                    let Lit::Atom { atom, .. } = lit else { continue };
                    let Some(atoms) = undecided.get(&atom.rel).cloned() else {
                        continue;
                    };
                    let head = undecided.entry(rule.head.rel.clone()).or_default();
                    for a in atoms {
                        if !head.contains(&a) {
                            head.push(a);
                            changed = true;
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }
    }

    /// Why `goal` holds (a proof whose every premise is supported in turn,
    /// negations shown absent) or, when it does not, every rule that could
    /// conclude it with its failing premises — a ground failing derived
    /// premise is explained in turn, `depth` levels deep.
    pub fn explain(&self, closure: &Closure, goal: &Fact, state: &EffectiveState<'_>) -> Explanation {
        if closure.facts.contains(goal) {
            return Explanation::Holds(self.prove(closure, goal, state));
        }
        let mut visiting = BTreeSet::new();
        Explanation::Fails {
            derived: self.derives(&goal.0),
            attempts: self.attempts(closure, goal, state, &mut visiting, 4),
        }
    }

    fn prove(&self, closure: &Closure, fact: &Fact, state: &EffectiveState<'_>) -> Proof {
        let Some(&rank) = closure.rank.get(fact) else {
            return Proof::Base(fact.clone());
        };
        let earlier = |f: &Fact| closure.rank.get(f).is_none_or(|&r| r < rank);
        for rule in self.rules.iter().filter(|r| r.head.rel == fact.0) {
            let Some(seed) = unify(&rule.head.terms, &fact.1, &Binding::new()) else {
                continue;
            };
            let mut unknown = Vec::new();
            let sols = solve_from(&rule.body, &closure.facts, &self.kinds, state, &mut unknown, seed);
            let Some(b) = sols.into_iter().find(|b| {
                positive_atoms(&rule.body)
                    .filter_map(|a| ground_atom(a, b).map(|args| (a.rel.clone(), args)))
                    .all(|f| earlier(&f))
            }) else {
                continue;
            };
            let premises = rule
                .body
                .iter()
                .map(|lit| match lit {
                    Lit::Atom { atom, negated } => {
                        let f = (atom.rel.clone(), ground_atom(atom, &b).unwrap_or_default());
                        if *negated && f.1.len() != atom.terms.len() {
                            // `not rel(…, _)` (dsl 0.24 T3-9): no tuple matched.
                            Premise::Test {
                                text: render_test(lit, &b),
                                holds: Some(true),
                            }
                        } else if *negated {
                            Premise::Absent(f)
                        } else if self.kinds.contains_key(&atom.rel) {
                            kind_premise(atom, &b, true)
                        } else {
                            Premise::Holds(Box::new(self.prove(closure, &f, state)))
                        }
                    }
                    other => Premise::Test {
                        text: render_test(other, &b),
                        holds: Some(true),
                    },
                })
                .collect();
            return Proof::Derived {
                fact: fact.clone(),
                rule: rule.raw.clone(),
                premises,
            };
        }
        // Unreachable for a fact the fixpoint ranked; degrade honestly.
        Proof::Base(fact.clone())
    }

    fn attempts(
        &self,
        closure: &Closure,
        goal: &Fact,
        state: &EffectiveState<'_>,
        visiting: &mut BTreeSet<Fact>,
        depth: usize,
    ) -> Vec<Attempt> {
        if depth == 0 || !visiting.insert(goal.clone()) {
            return Vec::new();
        }
        let mut out = Vec::new();
        for rule in self.rules.iter().filter(|r| r.head.rel == goal.0) {
            let Some(seed) = unify(&rule.head.terms, &goal.1, &Binding::new()) else {
                continue;
            };
            out.push(Attempt {
                rule: rule.raw.clone(),
                premises: self.attempt_premises(rule, seed, closure, state, visiting, depth),
            });
        }
        visiting.remove(goal);
        out
    }

    /// Join the positive atoms left to right, keeping every binding; the
    /// first atom no binding extends is the missing premise (its
    /// predecessors shown under one surviving binding). When the join
    /// completes, the binding failing the fewest filters is judged — every
    /// premise rendered under that ONE binding, so the attempt reads as a
    /// single consistent instance of the rule.
    fn attempt_premises(
        &self,
        rule: &Rule,
        seed: Binding,
        closure: &Closure,
        state: &EffectiveState<'_>,
        visiting: &mut BTreeSet<Fact>,
        depth: usize,
    ) -> Vec<Premise> {
        let mut bindings = vec![seed];
        let mut failed_at = None;
        for (i, lit) in rule.body.iter().enumerate() {
            if let Lit::Atom {
                atom,
                negated: false,
            } = lit
            {
                let next = extend(atom, &bindings, &closure.facts, &self.kinds);
                if next.is_empty() {
                    failed_at = Some(i);
                    break;
                }
                bindings = next;
            }
        }
        let b = match failed_at {
            Some(_) => bindings.swap_remove(0),
            None => {
                let failing = |b: &Binding| {
                    rule.body
                        .iter()
                        .filter(|lit| {
                            test_holds(lit, b, &closure.facts, &self.kinds, state, &mut Vec::new())
                                != Some(true)
                        })
                        .count()
                };
                bindings
                    .into_iter()
                    .min_by_key(|b| failing(b))
                    .unwrap_or_default()
            }
        };
        let reached = |i: usize| failed_at.is_none_or(|f| i < f);
        let mut premises = Vec::with_capacity(rule.body.len());
        for (i, lit) in rule.body.iter().enumerate() {
            let premise = match lit {
                Lit::Atom {
                    atom,
                    negated: false,
                } if self.kinds.contains_key(&atom.rel) && (failed_at == Some(i) || reached(i)) => {
                    kind_premise(atom, &b, failed_at != Some(i))
                }
                Lit::Atom {
                    atom,
                    negated: false,
                } if failed_at == Some(i) => {
                    let why = match ground_atom(atom, &b) {
                        Some(args) if self.derives(&atom.rel) => self.attempts(
                            closure,
                            &(atom.rel.clone(), args),
                            state,
                            visiting,
                            depth - 1,
                        ),
                        _ => Vec::new(),
                    };
                    Premise::Missing {
                        atom: render_atom(atom, &b),
                        why,
                    }
                }
                Lit::Atom {
                    atom,
                    negated: false,
                } if reached(i) => {
                    let args = ground_atom(atom, &b).unwrap_or_default();
                    Premise::Holds(Box::new(self.prove(closure, &(atom.rel.clone(), args), state)))
                }
                // Filters are judged only once the join completed.
                other if failed_at.is_some() => Premise::Unreached(render_test(other, &b)),
                Lit::Atom { atom, .. } if ground_atom(atom, &b).is_none() => Premise::Test {
                    text: render_test(lit, &b),
                    holds: test_holds(lit, &b, &closure.facts, &self.kinds, state, &mut Vec::new()),
                },
                Lit::Atom { atom, .. } => {
                    let f = (atom.rel.clone(), ground_atom(atom, &b).unwrap_or_default());
                    if self.kinds.contains_key(&atom.rel) {
                        if atom_holds(&f, &closure.facts, &self.kinds) {
                            Premise::Test {
                                text: format!("not {}", render_atom(atom, &b)),
                                holds: Some(false),
                            }
                        } else {
                            Premise::Absent(f)
                        }
                    } else if closure.facts.contains(&f) {
                        Premise::Present(Box::new(self.prove(closure, &f, state)))
                    } else {
                        Premise::Absent(f)
                    }
                }
                other => Premise::Test {
                    text: render_test(other, &b),
                    holds: test_holds(other, &b, &closure.facts, &self.kinds, state, &mut Vec::new()),
                },
            };
            premises.push(premise);
        }
        premises
    }
}

impl Proof {
    pub fn fact(&self) -> &Fact {
        match self {
            Proof::Base(f) | Proof::Derived { fact: f, .. } => f,
        }
    }
}

/// Render a ground fact `rel(a, b)`.
pub fn render_fact(f: &Fact) -> String {
    format!("{}({})", f.0, f.1.join(", "))
}

/// Parse a ground `rel(a, b)` into a [`Fact`]; `None` when it is not one —
/// a `_` wildcard or a leading-uppercase rule variable (`prime(X)`) is not
/// ground.
pub fn parse_ground(s: &str) -> Option<Fact> {
    use lute_syntax::datalog::FactTerm;
    let pat = lute_syntax::datalog::parse_fact(s).ok()?;
    let mut args = Vec::with_capacity(pat.args.len());
    for a in &pat.args {
        match &a.term {
            FactTerm::Ident(s) if s.starts_with(|c: char| c.is_ascii_uppercase()) => return None,
            FactTerm::Ident(s) => args.push(s.clone()),
            FactTerm::Bool(b) => args.push(b.to_string()),
            FactTerm::Wildcard | FactTerm::Param(_) => return None,
        }
    }
    Some((pat.relation, args))
}

// ---------------------------------------------------------------------------
// Parsing.
// ---------------------------------------------------------------------------

fn ir_rule(r: &Json) -> Option<Rule> {
    let head = ir_atom(r.get("head")?)?;
    let body = r
        .get("body")
        .and_then(Json::as_array)
        .map(|b| b.iter().filter_map(ir_lit).collect())
        .unwrap_or_default();
    let raw = r.get("raw").and_then(Json::as_str).unwrap_or("").to_string();
    Some(Rule { head, body, raw })
}

fn ir_atom(a: &Json) -> Option<Atom> {
    let rel = a.get("relation").and_then(Json::as_str)?.to_string();
    let terms = a
        .get("terms")
        .and_then(Json::as_array)
        .map(|ts| ts.iter().filter_map(ir_term).collect())
        .unwrap_or_default();
    Some(Atom { rel, terms })
}

fn ir_term(t: &Json) -> Option<Term> {
    match t.get("kind").and_then(Json::as_str)? {
        "var" => Some(Term::Var(t.get("name").and_then(Json::as_str)?.to_string())),
        "const" => Some(Term::Const(t.get("value").and_then(Json::as_str)?.to_string())),
        _ => None,
    }
}

fn ir_lit(l: &Json) -> Option<Lit> {
    let negated = l.get("negated").and_then(Json::as_bool).unwrap_or(false);
    match l.get("kind").and_then(Json::as_str)? {
        "atom" => Some(Lit::Atom {
            atom: ir_atom(l.get("atom")?)?,
            negated,
        }),
        "cmp" => Some(Lit::Cmp {
            lhs: ir_term(l.get("lhs")?)?,
            rhs: ir_term(l.get("rhs")?)?,
            negated,
        }),
        "guard" => Some(Lit::Guard {
            cel: l.get("cel").and_then(Json::as_str)?.to_string(),
        }),
        _ => None,
    }
}

fn syntax_term(t: &lute_syntax::datalog::RuleTerm) -> Term {
    use lute_syntax::datalog::RuleTerm;
    match t {
        RuleTerm::Var(v) => Term::Var(v.clone()),
        RuleTerm::Const(c) => Term::Const(c.clone()),
        RuleTerm::Bool(b) => Term::Const(b.to_string()),
    }
}

fn syntax_atom(a: &lute_syntax::datalog::RuleAtom) -> Atom {
    Atom {
        rel: a.relation.clone(),
        terms: a.terms.iter().map(syntax_term).collect(),
    }
}

fn syntax_lit(l: &lute_syntax::datalog::BodyLiteral) -> Lit {
    use lute_syntax::datalog::BodyLiteral;
    match l {
        BodyLiteral::Pos(a) => Lit::Atom {
            atom: syntax_atom(a),
            negated: false,
        },
        BodyLiteral::Neg(a) => Lit::Atom {
            atom: syntax_atom(a),
            negated: true,
        },
        BodyLiteral::Guard { cel, .. } => Lit::Guard { cel: cel.clone() },
        BodyLiteral::Cmp {
            lhs, rhs, negated, ..
        } => Lit::Cmp {
            lhs: syntax_term(lhs),
            rhs: syntax_term(rhs),
            negated: *negated,
        },
    }
}

// ---------------------------------------------------------------------------
// Evaluation.
// ---------------------------------------------------------------------------

/// Least stratum per derived relation: a positive body atom keeps the head
/// at-or-above its stratum, a negated one pushes it strictly above.
/// Stratification (checker-guaranteed) makes this converge; the cap defends
/// against a malformed artifact.
fn compute_strata(rules: &[Rule], derived: &BTreeSet<String>) -> BTreeMap<String, usize> {
    let mut strata: BTreeMap<String, usize> = derived.iter().map(|r| (r.clone(), 0)).collect();
    let cap = derived.len() + 2;
    for _ in 0..cap {
        let mut changed = false;
        for rule in rules {
            let h = &rule.head.rel;
            for lit in &rule.body {
                if let Lit::Atom { atom, negated } = lit {
                    if let Some(&s) = strata.get(&atom.rel) {
                        let want = s + usize::from(*negated);
                        if strata[h] < want {
                            strata.insert(h.clone(), want);
                            changed = true;
                        }
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
    strata
}

fn positive_atoms(body: &[Lit]) -> impl Iterator<Item = &Atom> {
    body.iter().filter_map(|l| match l {
        Lit::Atom {
            atom,
            negated: false,
        } => Some(atom),
        _ => None,
    })
}

/// Every binding satisfying `body` over `facts`: join the positive atoms,
/// then filter by negated atoms, comparisons and guards. A guard that
/// decides neither way rejects the binding and records why into `unknown`.
fn solve_body(
    body: &[Lit],
    facts: &BTreeSet<Fact>,
    kinds: &Kinds,
    state: &EffectiveState<'_>,
    unknown: &mut Vec<UnresolvedAtom>,
) -> Vec<Binding> {
    solve_from(body, facts, kinds, state, unknown, Binding::new())
}

fn solve_from(
    body: &[Lit],
    facts: &BTreeSet<Fact>,
    kinds: &Kinds,
    state: &EffectiveState<'_>,
    unknown: &mut Vec<UnresolvedAtom>,
    seed: Binding,
) -> Vec<Binding> {
    let mut bindings = vec![seed];
    for atom in positive_atoms(body) {
        bindings = extend(atom, &bindings, facts, kinds);
        if bindings.is_empty() {
            return bindings;
        }
    }
    bindings.retain(|b| {
        body.iter().all(|lit| match lit {
            Lit::Atom { negated: false, .. } => true,
            other => test_holds(other, b, facts, kinds, state, unknown) == Some(true),
        })
    });
    bindings
}

/// Extend every binding through one positive atom: each matching fact, or —
/// for an entity kind — each member.
fn extend(atom: &Atom, bindings: &[Binding], facts: &BTreeSet<Fact>, kinds: &Kinds) -> Vec<Binding> {
    let mut next = Vec::new();
    if let Some(members) = kinds.get(&atom.rel) {
        for b in bindings {
            for m in members {
                if let Some(ext) = unify(&atom.terms, std::slice::from_ref(m), b) {
                    next.push(ext);
                }
            }
        }
        return next;
    }
    for b in bindings {
        for (rel, args) in facts {
            if rel != &atom.rel || args.len() != atom.terms.len() {
                continue;
            }
            if let Some(ext) = unify(&atom.terms, args, b) {
                next.push(ext);
            }
        }
    }
    next
}

/// A ground atom holds: a member of the entity kind it names, else a fact.
fn atom_holds(f: &Fact, facts: &BTreeSet<Fact>, kinds: &Kinds) -> bool {
    match kinds.get(&f.0) {
        Some(members) => f.1.len() == 1 && members.contains(&f.1[0]),
        None => facts.contains(f),
    }
}

/// An entity-kind atom as a proof premise: membership, not a fact.
fn kind_premise(atom: &Atom, b: &Binding, holds: bool) -> Premise {
    Premise::Test {
        text: format!("{} — entity kind `{}`", render_atom(atom, b), atom.rel),
        holds: Some(holds),
    }
}

/// The closed kinds of a checker vocabulary (an `open:` kind's members are
/// engine-registered, never enumerable here).
pub fn closed_kinds(kinds: &BTreeMap<String, lute_manifest::relations::EntityKindDecl>) -> Kinds {
    kinds
        .iter()
        .filter_map(|(name, d)| match &d.shape {
            lute_manifest::relations::KindShape::Members(ms) => Some((name.clone(), ms.clone())),
            _ => None,
        })
        .collect()
}

/// The closed kinds of an artifact / project-index `entities` array (IR
/// `EntityKindEntry`: `{name, members?, open}`).
pub fn ir_kinds(entities: Option<&Json>) -> Kinds {
    entities
        .and_then(Json::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|e| {
                    let name = e.get("name").and_then(Json::as_str)?;
                    let members = e.get("members").and_then(Json::as_array)?;
                    Some((
                        name.to_string(),
                        members.iter().filter_map(|m| m.as_str().map(str::to_string)).collect(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A filter literal under `b`: a negated atom (absent from `facts`), a
/// comparison or a guard. `None` only for a guard that decided neither way.
fn test_holds(
    lit: &Lit,
    b: &Binding,
    facts: &BTreeSet<Fact>,
    kinds: &Kinds,
    state: &EffectiveState<'_>,
    unknown: &mut Vec<UnresolvedAtom>,
) -> Option<bool> {
    match lit {
        Lit::Atom { negated: false, .. } => Some(true),
        Lit::Atom { atom, .. } => Some(match ground_atom(atom, b) {
            Some(args) => !atom_holds(&(atom.rel.clone(), args), facts, kinds),
            // An anonymous `_` (dsl 0.24 T3-9) — the only variable safety
            // leaves unbound here — is existential: no matching tuple at all.
            None => extend(atom, std::slice::from_ref(b), facts, kinds).is_empty(),
        }),
        Lit::Cmp { lhs, rhs, negated } => match (ground_term(lhs, b), ground_term(rhs, b)) {
            (Some(l), Some(r)) => Some((l == r) != *negated),
            _ => Some(false),
        },
        Lit::Guard { cel } => match eval_rule_guard(cel, b, state, unknown) {
            Value::Bool(v) => Some(v),
            _ => None,
        },
    }
}

fn ground_term(t: &Term, b: &Binding) -> Option<String> {
    match t {
        Term::Const(c) => Some(c.clone()),
        Term::Var(v) => b.get(v).cloned(),
    }
}

fn ground_atom(a: &Atom, b: &Binding) -> Option<Vec<String>> {
    a.terms.iter().map(|t| ground_term(t, b)).collect()
}

/// Extend `binding` so `terms` matches `args`, or `None` on a conflict.
fn unify(terms: &[Term], args: &[String], binding: &Binding) -> Option<Binding> {
    if terms.len() != args.len() {
        return None;
    }
    let mut b = binding.clone();
    for (t, a) in terms.iter().zip(args) {
        match t {
            Term::Const(c) => {
                if c != a {
                    return None;
                }
            }
            Term::Var(v) => match b.get(v) {
                Some(existing) if existing != a => return None,
                Some(_) => {}
                None => {
                    b.insert(v.clone(), a.clone());
                }
            },
        }
    }
    Some(b)
}

/// A rule-body CEL guard reads only scalar state and the ground terms the
/// join bound — never facts (a fact query in a rule guard is rejected by the
/// checker). Each bound rule variable is substituted by its ground value,
/// then the fragment is evaluated over `state` with an empty fact store.
fn eval_rule_guard(
    cel: &str,
    binding: &Binding,
    state: &EffectiveState<'_>,
    unknown: &mut Vec<UnresolvedAtom>,
) -> Value {
    let substituted = substitute_vars(cel, binding);
    let mut arena = CelArena::default();
    let Ok(handle) = lute_cel::parse_slot(&mut arena, &substituted, 0) else {
        return Value::Bool(false);
    };
    let Some(ided) = arena.get(handle) else {
        return Value::Bool(false);
    };
    let vocab = RelVocab::default();
    let fs = FactStore::new(&vocab);
    let env = EvalEnv { state, facts: &fs };
    let mut atoms = Vec::new();
    let v = eval(&ided.expr, &env, &mut atoms);
    if !matches!(v, Value::Bool(_)) {
        for a in atoms {
            if !unknown.contains(&a) {
                unknown.push(a);
            }
        }
    }
    v
}

/// Substitute each bound rule variable in a guard fragment with its ground
/// value — a numeric value inlined bare, any other quoted as a CEL string
/// literal. String-literal regions are left untouched
/// (`lute_cel::cel_string_mask`), so a `'@gold'`-style value is never
/// rewritten.
fn substitute_vars(cel: &str, binding: &Binding) -> String {
    if binding.is_empty() {
        return cel.to_string();
    }
    let mask = lute_cel::cel_string_mask(cel);
    let bytes = cel.as_bytes();
    let mut out = String::with_capacity(cel.len());
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        let in_string = mask.get(i).copied().unwrap_or(false);
        if !in_string && (c.is_ascii_alphabetic() || c == '_') {
            let start = i;
            while i < bytes.len() && ((bytes[i] as char).is_ascii_alphanumeric() || bytes[i] == b'_')
            {
                i += 1;
            }
            let ident = &cel[start..i];
            match binding.get(ident) {
                Some(val) if val.parse::<f64>().is_ok() => out.push_str(val),
                Some(val) => {
                    out.push('\'');
                    out.push_str(&val.replace('\'', "\\'"));
                    out.push('\'');
                }
                None => out.push_str(ident),
            }
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

fn render_term(t: &Term, b: &Binding) -> String {
    match t {
        Term::Const(c) => c.clone(),
        Term::Var(v) => b.get(v).cloned().unwrap_or_else(|| {
            if lute_syntax::datalog::is_anonymous_var(v) {
                "_".to_string()
            } else {
                v.clone()
            }
        }),
    }
}

fn render_atom(a: &Atom, b: &Binding) -> String {
    let args: Vec<String> = a.terms.iter().map(|t| render_term(t, b)).collect();
    format!("{}({})", a.rel, args.join(", "))
}

fn render_test(lit: &Lit, b: &Binding) -> String {
    match lit {
        Lit::Atom { atom, negated } => {
            format!("{}{}", if *negated { "not " } else { "" }, render_atom(atom, b))
        }
        Lit::Cmp { lhs, rhs, negated } => format!(
            "{} {} {}",
            render_term(lhs, b),
            if *negated { "!=" } else { "=" },
            render_term(rhs, b)
        ),
        Lit::Guard { cel } => format!("cel(\"{}\")", substitute_vars(cel, b)),
    }
}
