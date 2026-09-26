//! The project-level relational guard pass (dsl 0.20.0 §5, §7): every guard
//! slot is re-decided with the root's [`FactEnv`] in scope, and a verdict
//! that only the fact envelope makes decidable is reported through the code
//! that slot already owns —
//!
//! | slot | decides false | decides true |
//! | --- | --- | --- |
//! | `<when test>`, `<choice when>`, `::next{when}`, content line `when=` | `E-ARM-DEAD` | — |
//! | lore entry `when` | [`E_ENTRY_UNREACHABLE`] | — |
//! | scene beat `when` (dsl 0.21.0) | [`crate::beats::E_BEAT_UNREACHABLE`] | — |
//! | `<objective done>` | `E-OBJECTIVE-UNSATISFIABLE` | — |
//! | required `<objective when>` | `W-OBJECTIVE-HIDDEN` | — |
//! | `<quest start>` / `<quest fail>` | `E-QUEST-UNREACHABLE` | `fail` only |
//!
//! — plus [`W_FACT_GUARANTEED`] on a guard (line `when=`, `<choice when>`,
//! `<when test>`, entry `when`, scene beat `when`) that carries a guaranteed
//! relational query.
//!
//! **Never a duplicate of the per-file pass.** Single-file `check()` decides
//! the same slots with no fact envelope (`reachability.rs`). A slot that
//! already decides WITHOUT facts is that pass's verdict and is skipped here
//! (the relational cause is not load-bearing — `false && holds(x)` is dead
//! because of the literal), and a diagnostic the per-file pass already
//! reported under the same code at the same span is never repeated (an arm
//! subsumed by earlier `is` arms keeps its one `E-ARM-DEAD`). Every message
//! names the facts that decided it and why.
//!
//! **Slot identity.** A guard's must set is looked up by its document path
//! and its own `CelSlot` span ([`crate::fact_env::MustMap`]).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use cel_parser::ast::{operators as op, Expr};
use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_syntax::ast::{Arm, CelSlot, Document, Node, Objective, Quest};

use crate::cel_expand::{expand_cel, DefTable};
use crate::check::FoldedEnv;
use crate::decide::{
    analyze_unset_sentinel_slot, and_chain_holds, decide, exclusive_pairs, DecideCtx, Decided,
    DollarBinding,
};
use crate::fact_env::{
    CountInterval, FactEnv, FactScope, GroundFact, HoldsVerdict, MustFact, Provenance, QueryPattern,
};
use crate::match_check::{infer_domain, subject_path, DomainInfo};
use crate::meta::StateSchema;
pub use crate::reachability::E_ENTRY_UNREACHABLE;
use crate::reachability::{
    arm_has_foreign_literal, E_ARM_DEAD, E_OBJECTIVE_UNSATISFIABLE, E_QUEST_UNREACHABLE,
    REQUIRED_QUEST_NOTE, W_OBJECTIVE_HIDDEN,
};
use crate::rel_schema::RelVocab;

/// `W-FACT-GUARANTEED` (dsl 0.20.0 §5): a relational query inside a guard
/// (line `when=`, `<choice when>`, `<when test>`, entry `when`) that is
/// guaranteed on every route to the guard — the condition is redundant.
pub const W_FACT_GUARANTEED: &str = "W-FACT-GUARANTEED";

/// `E-FACT-EXCLUSIVE` (dsl 0.25.0 §1): an `::assert{A(x)}` on a path where a
/// fact of a relation `A` excludes holds on the same arguments.
pub const E_FACT_EXCLUSIVE: &str = "E-FACT-EXCLUSIVE";

/// The relational guard diagnostics of one document (see the module doc).
/// `reported` is what the per-file `check()` already reported for this
/// document; a verdict it already carries (same code, same span) is dropped.
pub fn check_fact_guards(
    path: &Path,
    doc: &Document,
    folded: &FoldedEnv,
    env: &FactEnv,
    reported: &[Diagnostic],
) -> Vec<Diagnostic> {
    let params = BTreeMap::new();
    let g = Guards::new(path, folded, env, &params);
    let mut out = Vec::new();
    // dsl 0.21.0 §5: a scene beat's `when` — evaluated when its occasion is
    // raised, before the scene runs; the must set there is the scene's entry
    // set (`crate::fact_must`).
    if let Some(when) = folded.typed.beat.as_ref().and_then(|b| b.when.as_ref()) {
        if let Some(v) = g.eval(when, None) {
            if v.newly_false() {
                out.push(v.grade(diag(
                    crate::beats::E_BEAT_UNREACHABLE,
                    Severity::Error,
                    crate::beats::beat_unreachable_message(
                        &crate::beats::scene_beat_name(folded),
                        when.raw.trim(),
                        Some(&v.dead_reasons()),
                    ),
                    when.span,
                )));
            } else {
                g.push_guaranteed(&v, "`when` guard", when, &mut out);
            }
        }
    }
    for shot in &doc.shots {
        g.walk(&shot.body, &mut out);
    }
    for quest in &doc.quests {
        g.quest(quest, &mut out);
        g.walk(&quest.body, &mut out);
    }
    for entry in &doc.entries {
        if let Some(when) = &entry.when {
            if let Some(v) = g.eval(when, None) {
                if v.newly_false() {
                    out.push(v.grade(diag(
                        E_ENTRY_UNREACHABLE,
                        Severity::Error,
                        format!(
                            "entry `{}` is never eligible: its `when` guard `{}` is provably false \
                             — {} (dsl 0.20.0 §5)",
                            entry.id,
                            when.raw.trim(),
                            v.dead_reasons()
                        ),
                        when.span,
                    )));
                } else {
                    g.push_guaranteed(&v, "`when` guard", when, &mut out);
                }
            }
        }
        g.walk(&entry.body, &mut out);
    }
    // dsl 0.23.0 §4: a bundle beat's `when` is a beat `when`.
    let doc_id = folded.typed.id.as_deref().unwrap_or("this document");
    for beat in &doc.beats {
        if let Some(when) = &beat.when {
            if let Some(v) = g.eval(when, None) {
                if v.newly_false() {
                    out.push(v.grade(diag(
                        crate::beats::E_BEAT_UNREACHABLE,
                        Severity::Error,
                        crate::beats::beat_unreachable_message(
                            &crate::bundles::bundle_beat_key(doc_id, &beat.id),
                            when.raw.trim(),
                            Some(&v.dead_reasons()),
                        ),
                        when.span,
                    )));
                } else {
                    g.push_guaranteed(&v, "`when` guard", when, &mut out);
                }
            }
        }
        g.walk(&beat.body, &mut out);
    }
    out.retain(|d| {
        !reported
            .iter()
            .any(|r| r.code == d.code && r.span == d.span)
    });
    out
}

/// Every `<quest id>` in `doc` with a REQUIRED (`!optional`) `<objective>`
/// whose `done` decides false under the fact envelope — the structured twin
/// of `E-OBJECTIVE-UNSATISFIABLE` (scalar and relational causes alike) that
/// connectivity's `completed(Q)` consumes as data (dsl 0.4.0 §8.2 C4 rides
/// the quest consequence as a note, never a standalone diagnostic). An
/// optional objective never marks its quest; an ambiguous or empty id is
/// skipped (provable-only: another declaration of the id may be alive).
pub fn dead_required_objective_quests(
    path: &Path,
    doc: &Document,
    folded: &FoldedEnv,
    env: &FactEnv,
    ambiguous_quest_ids: &BTreeSet<String>,
) -> BTreeSet<String> {
    let params = BTreeMap::new();
    let g = Guards::new(path, folded, env, &params);
    doc.quests
        .iter()
        .filter(|q| !q.id.is_empty() && !ambiguous_quest_ids.contains(&q.id))
        .filter(|q| g.has_dead_required_objective(&q.body))
        .map(|q| q.id.clone())
        .collect()
}

/// Every `<quest id>` in `doc` whose lifecycle is dead under the fact
/// envelope: `start` decides false (never activates) or `fail` decides true
/// (fails at its first evaluation) — the structured twin of this pass's
/// `E-QUEST-UNREACHABLE`, which lands in the project diagnostics that
/// `crate::connectivity::unreachable_quest_ids` (a per-file scan) cannot
/// see. Scalar-only causes are included too; they are already in the
/// per-file lifecycle set, so the union is unchanged. Same id skipping as
/// [`dead_required_objective_quests`].
pub fn dead_lifecycle_quests(
    path: &Path,
    doc: &Document,
    folded: &FoldedEnv,
    env: &FactEnv,
    ambiguous_quest_ids: &BTreeSet<String>,
) -> BTreeSet<String> {
    let params = BTreeMap::new();
    let g = Guards::new(path, folded, env, &params);
    doc.quests
        .iter()
        .filter(|q| !q.id.is_empty() && !ambiguous_quest_ids.contains(&q.id))
        .filter(|q| {
            q.start.as_ref().is_some_and(|s| g.decides_to(s, false))
                || q.fail.as_ref().is_some_and(|f| g.decides_to(f, true))
        })
        .map(|q| q.id.clone())
        .collect()
}

/// One document's resolution environment for the pass: its `@def` table,
/// state schema, and own relational vocabulary, plus the root's envelope.
/// Component params are empty (a project document is never a standalone
/// component self-check), so `params` is always an empty map.
struct Guards<'a> {
    path: &'a Path,
    defs: DefTable<'a>,
    schema: &'a StateSchema,
    vocab: &'a RelVocab,
    env: &'a FactEnv,
    params: &'a BTreeMap<String, DomainInfo>,
}

/// One relational query of a guard and its §5 verdict.
enum Atom {
    Holds {
        pattern: QueryPattern,
        verdict: HoldsOutcome,
        /// Under an odd number of `!` — `!holds(P)`.
        negated: bool,
    },
    /// A `count(P) ⋈ n` / `countDistinct(P, V) ⋈ n` comparison the interval
    /// decided; `column` is `countDistinct`'s counted position.
    Count {
        pattern: QueryPattern,
        column: Option<usize>,
        interval: CountInterval,
        value: bool,
    },
}

/// An owned [`HoldsVerdict`]; a guaranteed query carries its reason, an
/// impossible one the stable fact that defeats it, if any, an excluded one
/// (dsl 0.25.0 §1) why it cannot hold.
enum HoldsOutcome {
    Impossible(Option<String>),
    Guaranteed(String),
    Excluded(String),
    Possible,
}

/// A guard decided without and with the fact envelope, plus its relational
/// queries.
struct SlotVerdict {
    base: Option<Decided>,
    with: Option<Decided>,
    atoms: Vec<Atom>,
    /// dsl 0.23.0 §10 (`--wip`): decided under the envelope, but not the
    /// same way under its work-in-progress twin — only producers not written
    /// yet decide the guard.
    wip: bool,
    /// dsl 0.25.0 §1: why pairs of the guard's required `holds` conjuncts
    /// can never hold together (each makes the guard false).
    exclusive: Vec<String>,
    /// dsl 0.25.0 §1: why `!holds(B)` conjuncts follow from a required
    /// `holds(A)` of the same guard (each is redundant).
    redundant: Vec<String>,
}

impl SlotVerdict {
    /// Decides `value` only because of the facts (the per-file pass has not).
    fn newly(&self, value: bool) -> bool {
        self.with == Some(Decided::Bool(value)) && self.base != Some(Decided::Bool(value))
    }

    fn newly_false(&self) -> bool {
        self.newly(false)
    }

    /// dsl 0.23.0 §10: a dead-guard error downgraded to a warning when the
    /// guard is dead only for want of content not written yet.
    fn grade(&self, mut d: Diagnostic) -> Diagnostic {
        if self.wip {
            d.severity = Severity::Warning;
            d.message.push_str(
                " — a warning under `--wip`: it is dead only for want of producers not written \
                 yet (relations with no seed, assert, rule, or reserved declaration, or written \
                 only by a component `::assert` with an unbound `@param`) (dsl 0.23.0 §10, \
                 0.26.0 §2.6)",
            );
        }
        d
    }

    /// Why the guard decided: every decided relational query, in order.
    fn dead_reasons(&self) -> String {
        let reasons: Vec<String> = self
            .atoms
            .iter()
            .filter_map(|a| match a {
                Atom::Holds {
                    pattern,
                    verdict: HoldsOutcome::Impossible(defeat),
                    ..
                } => Some(defeat.clone().unwrap_or_else(|| impossible_reason(pattern))),
                Atom::Holds {
                    verdict: HoldsOutcome::Guaranteed(reason) | HoldsOutcome::Excluded(reason),
                    ..
                } => Some(reason.clone()),
                Atom::Count {
                    pattern,
                    column,
                    interval,
                    ..
                } => Some(count_reason(pattern, *column, interval)),
                Atom::Holds {
                    verdict: HoldsOutcome::Possible,
                    ..
                } => None,
            })
            .chain(self.exclusive.iter().cloned())
            .collect();
        reasons.join("; ")
    }

    /// The guaranteed relational queries (must-backed) — `W-FACT-GUARANTEED`.
    fn guaranteed_reasons(&self) -> Vec<String> {
        self.atoms
            .iter()
            .filter_map(|a| match a {
                Atom::Holds {
                    verdict: HoldsOutcome::Guaranteed(reason),
                    ..
                } => Some(reason.clone()),
                // dsl 0.25.0 §1: `!holds(B)` where an excluding fact holds.
                Atom::Holds {
                    verdict: HoldsOutcome::Excluded(reason),
                    negated: true,
                    ..
                } => Some(reason.clone()),
                Atom::Count {
                    pattern,
                    column,
                    interval,
                    value: true,
                } if interval.lo > 0 => Some(match column {
                    None => format!(
                        "at least {} fact(s) matching `{pattern}` hold on every route to here",
                        interval.lo
                    ),
                    Some(i) => format!(
                        "at least {} distinct value(s) at argument {} of `{pattern}` hold on \
                         every route to here",
                        interval.lo,
                        i + 1
                    ),
                }),
                _ => None,
            })
            .chain(self.redundant.iter().cloned())
            .collect()
    }
}

fn impossible_reason(pattern: &QueryPattern) -> String {
    format!(
        "no seed, assert, rule, or engine relation produces `{pattern}` under your declared routes"
    )
}

/// Why a guaranteed fact holds at the slot, naming where it is established.
fn guaranteed_reason(m: &MustFact) -> String {
    let fact = &m.fact;
    match &m.provenance {
        Provenance::Assert { .. } => {
            format!(
                "`{fact}` is asserted on every route to here ({})",
                m.provenance
            )
        }
        Provenance::Guard { .. } => format!(
            "`{fact}` already holds here: the enclosing guard at {} requires it",
            m.provenance
        ),
        Provenance::Seed => format!("`{fact}` is a `facts:` seed that nothing retracts"),
        Provenance::Derived(inner) => format!(
            "`{fact}` follows by rule from facts that hold on every route to here ({inner})"
        ),
    }
}

/// dsl 0.25.0 §1: why `pattern` cannot hold where the must fact `m` does.
fn excluded_reason(pattern: &QueryPattern, m: &MustFact) -> String {
    format!(
        "`{pattern}` cannot hold here: {}, and `{}` excludes `{}` (dsl 0.25.0 §1)",
        guaranteed_reason(m),
        m.fact.relation,
        pattern.relation
    )
}

fn count_reason(pattern: &QueryPattern, column: Option<usize>, iv: &CountInterval) -> String {
    let range = match iv.hi {
        Some(hi) if hi == iv.lo => format!("exactly {hi}"),
        Some(hi) if iv.lo == 0 => format!("at most {hi}"),
        Some(hi) => format!("between {} and {hi}", iv.lo),
        None => format!("at least {}", iv.lo),
    };
    match column {
        None => format!("`count({pattern})` is {range} under your declared routes"),
        Some(i) => format!(
            "the number of distinct values at argument {} of `{pattern}` is {range} under your \
             declared routes",
            i + 1
        ),
    }
}

impl<'a> Guards<'a> {
    fn new(
        path: &'a Path,
        folded: &'a FoldedEnv,
        env: &'a FactEnv,
        params: &'a BTreeMap<String, DomainInfo>,
    ) -> Self {
        Guards {
            path,
            defs: DefTable {
                bodies: &folded.def_bodies,
                params: &folded.env.def_params,
            },
            schema: &folded.env.state,
            vocab: &folded.env.rel_vocab,
            env,
            params,
        }
    }

    fn ctx<'c>(&'c self, dollar: Option<&'c DomainInfo>, span: Span, facts: bool) -> DecideCtx<'c> {
        DecideCtx {
            schema: self.schema,
            dollar: dollar.map(DollarBinding::Domain),
            params: self.params,
            facts: facts.then_some(FactScope {
                env: self.env,
                vocab: self.vocab,
                path: self.path,
                span,
                wip: false,
            }),
        }
    }

    /// Decide `slot` without and with facts, `dollar` being the enclosing
    /// `<match>` subject's domain (arm tests only). `None` for an empty or
    /// unparseable guard (already reported elsewhere) — never a guess.
    fn eval(&self, slot: &CelSlot, dollar: Option<&DomainInfo>) -> Option<SlotVerdict> {
        if slot.raw.trim().is_empty() {
            return None;
        }
        let mut stack = Vec::new();
        let expanded = expand_cel(&slot.raw, &self.defs, Some("$"), &mut stack)
            .unwrap_or_else(|_| slot.raw.clone());
        let mut arena = lute_cel::CelArena::default();
        let handle = lute_cel::parse_slot_marked_refs(&mut arena, &expanded)?;
        let expr = &arena.get(handle)?.expr;
        let with_ctx = self.ctx(dollar, slot.span, true);
        let mut atoms = Vec::new();
        collect_atoms(expr, &with_ctx, false, &mut atoms);
        let (required, absent) = and_chain_holds(expr);
        let exclusive = exclusive_pairs(&required, self.vocab)
            .into_iter()
            .map(|(a, b)| {
                format!(
                    "`{a}` and `{b}` can never hold together (`{}` excludes `{}`, dsl 0.25.0 §1)",
                    a.relation, b.relation
                )
            })
            .collect();
        let redundant = absent
            .iter()
            .filter_map(|n| {
                let p = required.iter().find(|p| {
                    !exclusive_pairs(&[(*p).clone(), n.clone()], self.vocab).is_empty()
                })?;
                Some(format!(
                    "`!holds({n})` follows from this guard's `holds({p})`: `{}` excludes `{}` \
                     (dsl 0.25.0 §1)",
                    p.relation, n.relation
                ))
            })
            .collect();
        let with = decide(expr, &with_ctx);
        let wip = self.env.wip.is_some() && matches!(with, Some(Decided::Bool(_))) && {
            let mut wip_ctx = self.ctx(dollar, slot.span, true);
            if let Some(scope) = &mut wip_ctx.facts {
                scope.wip = true;
            }
            decide(expr, &wip_ctx) != with
        };
        Some(SlotVerdict {
            base: decide(expr, &self.ctx(dollar, slot.span, false)),
            with,
            atoms,
            wip,
            exclusive,
            redundant,
        })
    }

    /// `true` iff `slot` decides `value` with the fact envelope in scope —
    /// and, under `--wip` (dsl 0.26.0 §2.6), not only for want of producers
    /// not written yet.
    fn decides_to(&self, slot: &CelSlot, value: bool) -> bool {
        self.eval(slot, None)
            .is_some_and(|v| v.with == Some(Decided::Bool(value)) && !v.wip)
    }

    /// dsl 0.5.2 §2.3: a load-bearing `S == 'unset'` comparison already roots
    /// the dead guard as `E-UNSET-LITERAL` (per-file); the dead-arm
    /// derivative is owned by it here too.
    fn sentinel_owns(&self, slot: &CelSlot, dollar: Option<&DomainInfo>) -> bool {
        let a =
            analyze_unset_sentinel_slot(&slot.raw, &self.defs, &self.ctx(dollar, slot.span, true));
        !a.hits.is_empty() && a.load_bearing_for_false
    }

    /// A dead guard with its own dead-code verdict, else `W-FACT-GUARANTEED`.
    fn guard(
        &self,
        slot: &CelSlot,
        dollar: Option<&DomainInfo>,
        dead: impl FnOnce(&SlotVerdict) -> Diagnostic,
        warn_guaranteed: bool,
        out: &mut Vec<Diagnostic>,
    ) {
        let Some(v) = self.eval(slot, dollar) else {
            return;
        };
        if v.newly_false() {
            if !self.sentinel_owns(slot, dollar) {
                // dsl 0.23.0 §10: `--wip` grades a dead arm, choice, gated
                // line, or `::next` like every other dead guard.
                out.push(v.grade(dead(&v)));
            }
        } else if warn_guaranteed {
            self.push_guaranteed(&v, "guard", slot, out);
        }
    }

    fn push_guaranteed(
        &self,
        v: &SlotVerdict,
        what: &str,
        slot: &CelSlot,
        out: &mut Vec<Diagnostic>,
    ) {
        let reasons = v.guaranteed_reasons();
        if reasons.is_empty() {
            return;
        }
        out.push(diag(
            W_FACT_GUARANTEED,
            Severity::Warning,
            format!(
                "{what} `{}` is redundant: {} (dsl 0.20.0 §5)",
                slot.raw.trim(),
                reasons.join("; ")
            ),
            slot.span,
        ));
    }

    fn quest(&self, quest: &Quest, out: &mut Vec<Diagnostic>) {
        let start = quest
            .start
            .as_ref()
            .and_then(|s| Some((s, self.eval(s, None)?)))
            .filter(|(_, v)| v.newly_false());
        let fail = quest
            .fail
            .as_ref()
            .and_then(|f| Some((f, self.eval(f, None)?)))
            .filter(|(_, v)| v.newly(true));
        let mut causes = Vec::new();
        if let Some((s, v)) = &start {
            causes.push(format!(
                "`start=\"{}\"` is provably false — {}, so the quest never activates",
                s.raw.trim(),
                v.dead_reasons()
            ));
        }
        if let Some((f, v)) = &fail {
            causes.push(format!(
                "`fail=\"{}\"` is provably true — {}, so the quest fails at its first evaluation",
                f.raw.trim(),
                v.dead_reasons()
            ));
        }
        if causes.is_empty() {
            return;
        }
        out.push(diag(
            E_QUEST_UNREACHABLE,
            Severity::Error,
            format!(
                "quest can never complete: {} (dsl 0.20.0 §5)",
                causes.join("; and ")
            ),
            // The QUEST's span, as `check_quest_reach` anchors it: the anchor
            // `connectivity::unreachable_quest_ids` matches on.
            quest.span,
        ));
    }

    fn objective(&self, o: &Objective, out: &mut Vec<Diagnostic>) {
        if let Some(v) = self.eval(&o.done, None) {
            if v.newly_false() {
                let mut msg = format!(
                    "`done` predicate `{}` is provably false — {}: the objective can never \
                     complete on any run",
                    o.done.raw.trim(),
                    v.dead_reasons()
                );
                if o.optional {
                    msg.push_str(" (dsl 0.20.0 §5)");
                } else {
                    msg.push_str(REQUIRED_QUEST_NOTE);
                }
                out.push(v.grade(diag(
                    E_OBJECTIVE_UNSATISFIABLE,
                    Severity::Error,
                    msg,
                    o.span,
                )));
            }
        }
        if o.optional {
            return;
        }
        if let Some(when) = &o.when {
            if let Some(v) = self.eval(when, None) {
                if v.newly_false() {
                    out.push(diag(
                        W_OBJECTIVE_HIDDEN,
                        Severity::Warning,
                        format!(
                            "objective's `when` `{}` is provably false — {}: it is never visible \
                             or tracked, yet still gates completion (dsl 0.20.0 §5) — mark it \
                             `optional` or fix the gate (0.2 §6.3)",
                            when.raw.trim(),
                            v.dead_reasons()
                        ),
                        o.span,
                    ));
                }
            }
        }
    }

    /// Mirrors `reachability.rs`'s `walk_reach` recursion.
    fn walk(&self, nodes: &[Node], out: &mut Vec<Diagnostic>) {
        for node in nodes {
            match node {
                Node::Match(m) => {
                    let subject = subject_path(m);
                    let dom = infer_domain(subject.as_deref(), self.schema);
                    for arm in &m.arms {
                        match arm {
                            Arm::When {
                                is,
                                test,
                                span,
                                body,
                                ..
                            } => {
                                // D4: an arm whose `is` carries a foreign
                                // literal is rooted by `E-WHEN-LITERAL-DOMAIN`.
                                if !is.as_ref().is_some_and(|p| {
                                    arm_has_foreign_literal(p, &dom, subject.as_deref())
                                }) {
                                    self.guard(
                                        test,
                                        Some(&dom),
                                        |v| dead_arm(*span, "arm", test, v),
                                        true,
                                        out,
                                    );
                                }
                                self.walk(body, out);
                            }
                            Arm::Otherwise { body, .. } => self.walk(body, out),
                        }
                    }
                }
                Node::Branch(b) => self.choices(&b.choices, out),
                Node::Hub(h) => self.choices(&h.choices, out),
                Node::On(o) => self.walk(&o.body, out),
                Node::Objective(o) => {
                    self.objective(o, out);
                    self.walk(&o.body, out);
                }
                Node::Line(l) => {
                    if let Some(when) = &l.when {
                        self.guard(
                            when,
                            None,
                            |v| {
                                diag(
                                    E_ARM_DEAD,
                                    Severity::Error,
                                    format!(
                                        "this gated line can never be shown: its `when` guard \
                                         `{}` is provably false — {} (dsl 0.20.0 §5)",
                                        when.raw.trim(),
                                        v.dead_reasons()
                                    ),
                                    when.span,
                                )
                            },
                            true,
                            out,
                        );
                    }
                }
                Node::Directive(d) => {
                    if let Some(when) = &d.when {
                        self.guard(
                            when,
                            None,
                            |v| {
                                diag(
                                    E_ARM_DEAD,
                                    Severity::Error,
                                    format!(
                                        "this `::next` never fires: its `when` guard `{}` is \
                                         provably false — {} (dsl 0.20.0 §5)",
                                        when.raw.trim(),
                                        v.dead_reasons()
                                    ),
                                    when.span,
                                )
                            },
                            false,
                            out,
                        );
                    }
                }
                Node::Assert(a) => self.exclusive_assert(a, out),
                Node::Set(_) | Node::Timeline(_) | Node::Retract(_) => {}
            }
        }
    }

    /// dsl 0.25.0 §1: an `::assert{A(x)}` where a fact `B(x)` of a relation
    /// excluding `A` holds on every route to it would make both hold —
    /// [`E_FACT_EXCLUSIVE`].
    fn exclusive_assert(&self, a: &lute_syntax::ast::Assert, out: &mut Vec<Diagnostic>) {
        let Some(fact) = GroundFact::from_pattern(&a.pattern) else {
            return;
        };
        let Some(m) = self
            .env
            .must
            .at(self.path, a.span)
            .iter()
            .find(|m| !exclusive_pairs(&[fact.clone(), m.fact.clone()], self.vocab).is_empty())
        else {
            return;
        };
        out.push(diag(
            E_FACT_EXCLUSIVE,
            Severity::Error,
            format!(
                "`::assert{{{fact}}}` would make `{fact}` and `{}` both hold: {}, and `{}` \
                 excludes `{}` (dsl 0.25.0 §1) — retract `{}` first, or assert only where it \
                 does not hold",
                m.fact,
                guaranteed_reason(m),
                fact.relation,
                m.fact.relation,
                m.fact
            ),
            a.span,
        ));
    }

    fn choices(&self, choices: &[lute_syntax::ast::Choice], out: &mut Vec<Diagnostic>) {
        for c in choices {
            if let Some(when) = &c.when {
                self.guard(
                    when,
                    None,
                    |v| dead_arm(c.span, "choice", when, v),
                    true,
                    out,
                );
            }
            self.walk(&c.body, out);
        }
    }

    /// Mirrors `walk`'s traversal exactly, so the structured signal and the
    /// diagnostics can never disagree about which objectives exist.
    fn has_dead_required_objective(&self, nodes: &[Node]) -> bool {
        nodes.iter().any(|node| match node {
            Node::Objective(o) => {
                (!o.optional && self.decides_to(&o.done, false))
                    || self.has_dead_required_objective(&o.body)
            }
            Node::Match(m) => m.arms.iter().any(|arm| {
                let (Arm::When { body, .. } | Arm::Otherwise { body, .. }) = arm;
                self.has_dead_required_objective(body)
            }),
            Node::Branch(b) => b
                .choices
                .iter()
                .any(|c| self.has_dead_required_objective(&c.body)),
            Node::Hub(h) => h
                .choices
                .iter()
                .any(|c| self.has_dead_required_objective(&c.body)),
            Node::On(o) => self.has_dead_required_objective(&o.body),
            Node::Line(_)
            | Node::Directive(_)
            | Node::Set(_)
            | Node::Timeline(_)
            | Node::Assert(_)
            | Node::Retract(_) => false,
        })
    }
}

fn dead_arm(span: Span, kind: &str, slot: &CelSlot, v: &SlotVerdict) -> Diagnostic {
    diag(
        E_ARM_DEAD,
        Severity::Error,
        format!(
            "{kind} can never fire: guard `{}` is provably false — {} (dsl 0.20.0 §5)",
            slot.raw.trim(),
            v.dead_reasons()
        ),
        span,
    )
}

/// Collect every relational query of a guard, in source order: each
/// well-shaped `holds(P)` with its verdict, and each `count(P) ⋈ n`
/// comparison the interval decides. Recurses like
/// `cel_resolve::check_fact_queries` (operator args, list elements, select
/// operands, `validAt`'s time argument).
fn collect_atoms(expr: &Expr, ctx: &DecideCtx<'_>, negated: bool, out: &mut Vec<Atom>) {
    match expr {
        Expr::Call(c) => {
            if crate::cel_resolve::is_profile_fact_query(c) {
                if c.func_name == "holds" {
                    if let (Some(scope), Expr::Call(p)) = (&ctx.facts, &c.args[0].expr) {
                        if let Some(pattern) = QueryPattern::from_call(p) {
                            let verdict = match scope.holds(&pattern) {
                                HoldsVerdict::Impossible => HoldsOutcome::Impossible(
                                    scope.defeat(&pattern).map(|(head, denied)| {
                                        format!(
                                            "`{head}` can only come from a rule that needs \
                                                 `not {denied}`, and `{denied}` holds throughout \
                                                 every run (a seed nothing removes, or derived \
                                                 from such seeds)"
                                        )
                                    }),
                                ),
                                HoldsVerdict::Guaranteed(m) => {
                                    HoldsOutcome::Guaranteed(guaranteed_reason(m))
                                }
                                HoldsVerdict::Excluded(m) => {
                                    HoldsOutcome::Excluded(excluded_reason(&pattern, m))
                                }
                                HoldsVerdict::Possible => HoldsOutcome::Possible,
                            };
                            out.push(Atom::Holds {
                                pattern,
                                verdict,
                                negated,
                            });
                        }
                    }
                } else if c.func_name == "validAt" {
                    if let Some(t) = c.args.get(1) {
                        collect_atoms(&t.expr, ctx, negated, out);
                    }
                }
                return;
            }
            let is_cmp = [
                op::GREATER,
                op::GREATER_EQUALS,
                op::LESS,
                op::LESS_EQUALS,
                op::EQUALS,
                op::NOT_EQUALS,
            ]
            .contains(&c.func_name.as_str());
            if is_cmp && c.args.len() == 2 {
                if let Some(atom) = count_atom(expr, &c.args[0].expr, &c.args[1].expr, ctx) {
                    out.push(atom);
                    return;
                }
            }
            if let Some(t) = &c.target {
                collect_atoms(&t.expr, ctx, negated, out);
            }
            let flip = negated != (c.func_name == op::LOGICAL_NOT);
            for a in &c.args {
                collect_atoms(&a.expr, ctx, flip, out);
            }
        }
        Expr::List(list) => {
            for el in &list.elements {
                collect_atoms(&el.expr, ctx, negated, out);
            }
        }
        Expr::Select(sel) => collect_atoms(&sel.operand.expr, ctx, negated, out),
        _ => {}
    }
}

/// A `count(P) ⋈ n` / `countDistinct(P, V) ⋈ n` comparison (either operand
/// order) the interval decides.
fn count_atom(cmp: &Expr, a: &Expr, b: &Expr, ctx: &DecideCtx<'_>) -> Option<Atom> {
    let scope = ctx.facts.as_ref()?;
    let (pattern, column) = [a, b].into_iter().find_map(|side| {
        let Expr::Call(c) = side else { return None };
        crate::fact_env::count_query(c)
    })?;
    let interval = scope.count_in(&pattern, column)?;
    let Some(Decided::Bool(value)) = decide(cmp, ctx) else {
        return None;
    };
    Some(Atom::Count {
        pattern,
        column,
        interval,
        value,
    })
}

fn diag(code: &str, severity: Severity, message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity,
        message,
        span,
        layer: Layer::Logic,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}
