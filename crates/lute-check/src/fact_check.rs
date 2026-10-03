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
    analyze_literal_comparisons, and_chain_holds, decide, exclusive_pairs, DecideCtx, Decided,
    DollarBinding,
};
use crate::fact_env::{
    CountInterval, FactEnv, FactScope, GroundFact, HoldsVerdict, MustFact, Provenance, QueryPattern,
};
use crate::match_check::{infer_domain, subject_path, CoverItem, DomainInfo, DomainValue};
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
    // dsl 0.28.0: the members a kind or `for=` beat written at `at` runs for.
    let members_at = |at: Span| {
        folded
            .env
            .occasion_scopes
            .members_at(at.byte_start, at.byte_end)
    };
    // dsl 0.28.0: the arms of a kind or `for=` beat's body that can match
    // only members its `when` never holds for — pushed after its body walk.
    let mut member_arms: Vec<Diagnostic> = Vec::new();
    // dsl 0.21.0 §5: a scene beat's `when` — evaluated when its occasion is
    // raised, before the scene runs; the must set there is the scene's entry
    // set (`crate::fact_must`).
    if let Some(when) = folded.typed.beat.as_ref().and_then(|b| b.when.as_ref()) {
        let members = members_at(when.span).or_else(|| {
            folded
                .typed
                .beat
                .as_ref()
                .and_then(|b| b.for_kind.as_ref())
                .and_then(|(_, span)| members_at(*span))
        });
        if let Some(v) = g.eval(when, None) {
            let per_member = g.members_never(when, members);
            if let Some((why, graded)) = per_member.unreachable() {
                out.push(graded.grade(diag(
                    crate::beats::E_BEAT_UNREACHABLE,
                    Severity::Error,
                    crate::beats::beat_unreachable_message(
                        &crate::beats::scene_beat_name(folded),
                        when.raw.trim(),
                        Some(&why),
                    ),
                    when.span,
                )));
            } else if v.newly_false() {
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
                if members.is_none() {
                    g.push_guaranteed(&v, "`when` guard", when, &mut out);
                }
                for shot in &doc.shots {
                    member_arms.extend(g.member_arms(doc, &shot.body, when, &per_member));
                }
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
        // dsl 0.28.0: over the entry's own environment.
        let g = g.over(&folded.env_at(entry.span).state);
        if let Some(when) = &entry.when {
            if let Some(v) = g.eval(when, None) {
                let per_member = g.members_never(when, members_at(entry.span));
                let dead = if v.newly_false() {
                    Some((v.dead_reasons(), &v))
                } else {
                    per_member.unreachable()
                };
                if let Some((why, graded)) = dead {
                    out.push(graded.grade(diag(
                        E_ENTRY_UNREACHABLE,
                        Severity::Error,
                        format!(
                            "entry `{}` is never eligible: its `when` guard `{}` is provably false \
                             — {why} (dsl 0.20.0 §5)",
                            entry.id,
                            when.raw.trim(),
                        ),
                        when.span,
                    )));
                } else {
                    g.push_guaranteed(&v, "`when` guard", when, &mut out);
                    member_arms.extend(g.member_arms(doc, &entry.body, when, &per_member));
                }
            }
        }
        g.walk(&entry.body, &mut out);
    }
    // dsl 0.23.0 §4: a bundle beat's `when` is a beat `when`.
    let doc_id = folded.typed.id.as_deref().unwrap_or("this document");
    for beat in &doc.beats {
        let g = g.over(&folded.env_at(beat.span).state);
        if let Some(when) = &beat.when {
            if let Some(v) = g.eval(when, None) {
            let per_member = g.members_never(when, members_at(beat.span));
                if let Some((why, graded)) = per_member.unreachable() {
                    out.push(graded.grade(diag(
                        crate::beats::E_BEAT_UNREACHABLE,
                        Severity::Error,
                        crate::beats::beat_unreachable_message(
                            &crate::bundles::bundle_beat_key(doc_id, &beat.id),
                            when.raw.trim(),
                            Some(&why),
                        ),
                        when.span,
                    )));
                } else if v.newly_false() {
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
                    if members_at(beat.span).is_none() {
                        g.push_guaranteed(&v, "`when` guard", when, &mut out);
                    }
                    member_arms.extend(g.member_arms(doc, &beat.body, when, &per_member));
                }
            }
        }
        g.walk(&beat.body, &mut out);
    }
    for d in member_arms {
        if !out.iter().any(|x| x.code == d.code && x.span == d.span) {
            out.push(d);
        }
    }
    // dsl 0.27.0 §4: each beat judged under its occasion's gate and the
    // project's `!terminal`, with the fact envelope in scope.
    for b in crate::gates::seam_beats(doc, folded) {
        let Some(seam) = b.seam(folded) else {
            continue;
        };
        // Already dead by its `when` alone — also when `--wip` graded that
        // report to `W-WIP`.
        let without_wip = format!("`{}` without `--wip`", b.code);
        if out.iter().any(|d| {
            d.span == b.span
                && (d.code == b.code || d.code == W_WIP && d.message.contains(&without_wip))
        }) {
            continue;
        }
        let judge = |conds: &[String]| -> Option<(Vec<SlotVerdict>, bool)> {
            let vs: Vec<SlotVerdict> = conds
                .iter()
                .map(|c| {
                    let slot =
                        CelSlot::raw(lute_syntax::ast::CelKind::Condition, c.clone(), b.span);
                    g.eval(&slot, None)
                })
                .collect::<Option<_>>()?;
            let dead = !vs.is_empty() && vs.iter().all(|v| v.with == Some(Decided::Bool(false)));
            Some((vs, dead))
        };
        let (verdict, vs) = match judge(&seam.gate_conds) {
            Some((vs, true)) => (crate::gates::SeamVerdict::GateDead, vs),
            _ => match judge(&seam.conds) {
                Some((vs, true)) => (crate::gates::SeamVerdict::Dead, vs),
                _ => continue,
            },
        };
        // Already dead without the facts: the per-file pass's verdict.
        if vs.iter().all(|v| v.base == Some(Decided::Bool(false))) {
            continue;
        }
        let mut reasons: Vec<String> = Vec::new();
        for v in &vs {
            let r = v.dead_reasons();
            if !r.is_empty() && !reasons.contains(&r) {
                reasons.push(r);
            }
        }
        let mut d = diag(
            b.code,
            Severity::Error,
            seam.message(
                &b.name,
                &b.on,
                b.when.map(|w| w.raw.as_str()),
                verdict,
                Some(&reasons.join("; ")),
            ),
            b.span,
        );
        if let Some(v) = vs.iter().find(|v| v.wip) {
            d = v.grade(d);
        }
        out.push(d);
    }
    out.retain(|d| {
        !reported
            .iter()
            .any(|r| r.code == d.code && r.span == d.span)
    });
    out
}

/// dsl 0.28.0: in a kind or `for=` beat, a `<match on="occasion.target">`
/// needs an arm only for the members the beat's `when` can hold for. With
/// the fact envelope in scope, members the per-file check could not rule
/// out may be: the match's `E-NONEXHAUSTIVE` in `diags` (the document's
/// per-file report) is dropped once every member still uncovered is one of
/// them, and names only the rest otherwise.
pub fn reconcile_member_matches(
    diags: &mut Vec<Diagnostic>,
    path: &Path,
    doc: &Document,
    folded: &FoldedEnv,
    env: &FactEnv,
) {
    const E_NONEXHAUSTIVE: &str = "E-NONEXHAUSTIVE";
    if !diags.iter().any(|d| d.code == E_NONEXHAUSTIVE) {
        return;
    }
    let params = BTreeMap::new();
    let base = Guards::new(path, folded, env, &params);
    // Each beat's `when`, element span and bodies.
    let mut beats: Vec<(Option<&CelSlot>, Span, Vec<&[Node]>)> = Vec::new();
    if let Some(b) = &folded.typed.beat {
        let shots = doc.shots.iter().map(|s| s.body.as_slice()).collect();
        beats.push((
            b.when.as_ref(),
            crate::beats::top_value_span(&doc.meta, "on"),
            shots,
        ));
    }
    beats.extend(
        doc.entries
            .iter()
            .map(|e| (e.when.as_ref(), e.span, vec![e.body.as_slice()])),
    );
    beats.extend(
        doc.beats
            .iter()
            .map(|b| (b.when.as_ref(), b.span, vec![b.body.as_slice()])),
    );
    for (when, at, bodies) in beats {
        let Some(when) = when else {
            continue;
        };
        let members = folded
            .env
            .occasion_scopes
            .members_at(at.byte_start, at.byte_end);
        let own = folded.env_at(at);
        let g = base.over(&own.state);
        let never = g.members_never(when, members).never();
        if never.is_empty() {
            continue;
        }
        let mut matches = Vec::new();
        for body in bodies {
            target_matches(body, &mut matches);
        }
        let ctx = crate::ctx::Ctx {
            env: own,
            in_match: false,
            match_subject: None,
        };
        let ruled_out = |item: &CoverItem| matches!(item, CoverItem::Value(DomainValue::Str(s)) if never.contains(s));
        for m in matches {
            let at_match = |d: &Diagnostic| {
                d.code == E_NONEXHAUSTIVE
                    && (d.span.byte_start, d.span.byte_end) == (m.span.byte_start, m.span.byte_end)
            };
            let Some(i) = diags.iter().position(at_match) else {
                continue;
            };
            let (subject, info) =
                crate::match_check::resolve_subject(m, &g.defs, &own.def_types, &own.state);
            let now = crate::match_check::check_match_with_domain(
                m,
                subject.as_deref(),
                info,
                &ctx,
                &ruled_out,
            )
            .into_iter()
            .find(|d| d.code == E_NONEXHAUSTIVE);
            match now {
                Some(d) => diags[i].message = d.message,
                None => {
                    diags.remove(i);
                }
            }
        }
    }
}

/// dsl 0.28.0: the targets a kind or `for=` beat never plays for — its
/// `when` decided once per member, `occasion.target` bound to it and the
/// root's fact envelope in scope: the per-member verdicts
/// `E-BEAT-UNREACHABLE` reports a beat dead by once they cover every member.
/// A kind beat's are its `<prefix>.<member>` targets
/// ([`crate::ProjectBeat::kind_targets`]), a `for=` beat's its bare members,
/// in member order. Empty for any other beat, or a `when` that does not read
/// `occasion.target`.
pub fn beat_never_for(beat: &crate::ProjectBeat<'_>, env: &FactEnv) -> Vec<String> {
    let Some(when) = beat.when_slot else {
        return Vec::new();
    };
    let folded = beat.folded;
    let at = (when.span.byte_start, when.span.byte_end);
    let members = folded.env.occasion_scopes.members_at(at.0, at.1);
    let params = BTreeMap::new();
    let g = Guards::new(beat.path, folded, env, &params);
    let never = g
        .over(&folded.env_at(when.span).state)
        .members_never(when, members)
        .never();
    match &beat.kind_targets {
        None => never,
        Some(targets) => never
            .iter()
            .filter_map(|m| {
                targets
                    .iter()
                    .find(|t| t.strip_suffix(m.as_str()).is_some_and(|p| p.ends_with('.')))
                    .cloned()
            })
            .collect(),
    }
}

/// Every `<match on="occasion.target">` in `nodes`, nested ones included.
fn target_matches<'n>(nodes: &'n [Node], out: &mut Vec<&'n lute_syntax::ast::Match>) {
    for node in nodes {
        match node {
            Node::Match(m) => {
                if subject_path(m).as_deref() == Some(crate::beats::OCCASION_TARGET) {
                    out.push(m);
                }
                for arm in &m.arms {
                    let (Arm::When { body, .. } | Arm::Otherwise { body, .. }) = arm;
                    target_matches(body, out);
                }
            }
            Node::Branch(b) => {
                for c in &b.choices {
                    target_matches(&c.body, out);
                }
            }
            Node::Hub(h) => {
                for body in h.bodies() {
                    target_matches(body, out);
                }
            }
            Node::On(o) => target_matches(&o.body, out),
            Node::Objective(o) => target_matches(&o.body, out),
            Node::Line(_)
            | Node::Directive(_)
            | Node::Set(_)
            | Node::Timeline(_)
            | Node::Assert(_)
            | Node::Retract(_) => {}
        }
    }
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
    def_types: &'a BTreeMap<String, lute_manifest::types::Type>,
    schema: &'a StateSchema,
    vocab: &'a RelVocab,
    env: &'a FactEnv,
    params: &'a BTreeMap<String, DomainInfo>,
}

/// dsl 0.28.0: a kind or `for=` beat's `when`, decided with the fact
/// envelope once per member it runs for (`occasion.target` bound to each).
/// Empty for any other beat, or a `when` that does not read the member.
struct MemberVerdicts<'m> {
    members: &'m [String],
    verdicts: Vec<SlotVerdict>,
}

impl MemberVerdicts<'_> {
    /// The members the `when` never holds for.
    fn never(&self) -> Vec<String> {
        self.members
            .iter()
            .zip(&self.verdicts)
            .filter(|(_, v)| v.with == Some(Decided::Bool(false)))
            .map(|(m, _)| m.clone())
            .collect()
    }

    /// Why the beat is never eligible — its `when` holds for no member, and
    /// only the facts make that so — with the verdict it is graded by.
    fn unreachable(&self) -> Option<(String, &SlotVerdict)> {
        let vs = &self.verdicts;
        if vs.is_empty()
            || !vs.iter().all(|v| v.with == Some(Decided::Bool(false)))
            || vs.iter().all(|v| v.base == Some(Decided::Bool(false)))
        {
            return None;
        }
        // One reason per member, members sharing a reason named together; a
        // member the `when` is false for as written needs no fact to say so.
        let mut reasons: Vec<(Vec<&str>, String)> = Vec::new();
        for (m, v) in self.members.iter().zip(vs) {
            let r = match v.dead_reasons() {
                r if !r.is_empty() => r,
                _ if v.base == Some(Decided::Bool(false)) => {
                    "false as written, whatever the facts".to_string()
                }
                _ => "the facts make its `when` false".to_string(),
            };
            match reasons.iter_mut().find(|(_, x)| *x == r) {
                Some((ms, _)) => ms.push(m.as_str()),
                None => reasons.push((vec![m.as_str()], r)),
            }
        }
        let head = crate::reachability::for_every_member(self.members);
        let each: Vec<String> = reasons
            .iter()
            .map(|(ms, r)| {
                let ms: Vec<String> = ms.iter().map(|m| format!("`{m}`")).collect();
                format!("{} — {r}", ms.join(", "))
            })
            .collect();
        let why = format!("{head}: {}", each.join("; "));
        Some((why, vs.iter().find(|v| v.wip).unwrap_or(&vs[0])))
    }
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

    /// dsl 0.23.0 §10: a dead-guard error reported as [`W_WIP`] when the
    /// guard is dead only for want of content not written yet.
    fn grade(&self, d: Diagnostic) -> Diagnostic {
        if !self.wip {
            return d;
        }
        wip_warning(
            d,
            "it is dead only for want of producers not written yet (relations with no seed, \
             assert, rule, or reserved declaration, or written only by a component `::assert` \
             with an unbound `@param`) (dsl 0.23.0 §10, 0.26.0 §2.6)",
        )
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
                        "at least {} fact(s) matching `holds{pattern}` hold on every route to here",
                        interval.lo,
                    ),
                    Some(i) => format!(
                        "at least {} distinct value(s) at argument {} of `holds{pattern}` hold on \
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

/// `check-project --wip`: a dead-guard error (`E-ARM-DEAD`,
/// `E-BEAT-UNREACHABLE`, `E-ENTRY-UNREACHABLE`, `E-OBJECTIVE-UNSATISFIABLE`)
/// reported as a warning because only content not written yet kills it.
pub const W_WIP: &str = "W-WIP";

/// dsl 0.28.0 (T3-66): the error `d` as `check-project --wip` reports it —
/// [`W_WIP`], a warning, never an `E-` code at warning severity; the message
/// keeps its sentence and names the code it has without the flag, then
/// `why` the flag spares it.
pub fn wip_warning(mut d: Diagnostic, why: &str) -> Diagnostic {
    d.message = format!("{} — `{}` without `--wip`: {why}", d.message, d.code);
    d.code = W_WIP.to_string();
    d.severity = Severity::Warning;
    d
}

fn impossible_reason(pattern: &QueryPattern) -> String {
    format!(
        "no seed, assert, rule, or engine relation produces `holds{pattern}` under your declared routes"
    )
}

/// HW27-10: an impossible derived fact named by the rule premise nothing
/// produces — `` `canEnter(office)` can only come from `canEnter(R) :-
/// holding(R)`, which needs `holding(office)`, and no seed, … produces it
/// `` — `None` when no rule concludes it.
fn starved_reason(pattern: &QueryPattern, starved: &[(&str, String)]) -> Option<String> {
    if starved.is_empty() {
        return None;
    }
    let rules: Vec<String> = starved
        .iter()
        .map(|(rule, premise)| format!("`{}`, which needs `{premise}`", rule.trim()))
        .collect();
    let premises: Vec<String> = starved.iter().map(|(_, p)| format!("`{p}`")).collect();
    Some(format!(
        "`holds{pattern}` can only come from {}, and no seed, assert, rule, or engine relation \
         produces {} under your declared routes",
        rules.join(" or "),
        premises.join(" or ")
    ))
}

/// Why a guaranteed fact holds at the slot, naming where it is established.
fn guaranteed_reason(m: &MustFact) -> String {
    let fact = &m.fact;
    let query = QueryPattern {
        relation: fact.relation.clone(),
        args: fact.args.iter().cloned().map(Some).collect(),
    };
    let shown = format!("holds{query}");
    match &m.provenance {
        Provenance::Assert { .. } => {
            format!("`{shown}` is asserted on every route to here ({})", m.provenance)
        }
        Provenance::Guard { .. } => format!(
            "`{shown}` already holds here: the enclosing guard at {} requires it",
            m.provenance
        ),
        Provenance::Seed => format!("`{shown}` is a `facts:` seed that nothing retracts"),
        Provenance::Derived(inner) => format!(
            "`{shown}` follows by rule from facts that hold on every route to here ({inner})"
        ),
    }
}

/// dsl 0.25.0 §1: why `pattern` cannot hold where the must fact `m` does.
fn excluded_reason(pattern: &QueryPattern, m: &MustFact) -> String {
    format!(
        "`holds{pattern}` cannot hold here: {}, and relation `{}` excludes relation `{}` \
         (dsl 0.25.0 §1)",
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
        None => format!("`count{pattern}` is {range} under your declared routes"),
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
            def_types: &folded.env.def_types,
            schema: &folded.env.state,
            vocab: &folded.env.rel_vocab,
            env,
            params,
        }
    }

    /// This environment over `schema` — a kind or `for=` beat's own
    /// ([`FoldedEnv::env_at`]).
    fn over(&self, schema: &'a StateSchema) -> Guards<'a> {
        Guards {
            path: self.path,
            defs: DefTable {
                bodies: self.defs.bodies,
                params: self.defs.params,
            },
            def_types: self.def_types,
            schema,
            vocab: self.vocab,
            env: self.env,
            params: self.params,
        }
    }

    /// dsl 0.28.0: `when` decided once per member of `members` (a kind or
    /// `for=` beat's), each with `occasion.target` bound to it.
    fn members_never<'m>(
        &self,
        when: &CelSlot,
        members: Option<&'m [String]>,
    ) -> MemberVerdicts<'m> {
        let members = members
            .filter(|_| crate::occasion_bind::mentions_target(&when.raw))
            .unwrap_or_default();
        let verdicts: Option<Vec<SlotVerdict>> = members
            .iter()
            .map(|m| {
                let raw = crate::occasion_bind::instantiate(&when.raw, m);
                self.eval(&CelSlot::raw(when.kind, raw, when.span), None)
            })
            .collect();
        match verdicts {
            Some(verdicts) => MemberVerdicts { members, verdicts },
            None => MemberVerdicts {
                members: &[],
                verdicts: Vec::new(),
            },
        }
    }

    /// dsl 0.28.0: the arms of `body` (a kind or `for=` beat's, guarded by
    /// `when`) that can match only members `when` never holds for.
    fn member_arms(
        &self,
        doc: &Document,
        body: &[Node],
        when: &CelSlot,
        per_member: &MemberVerdicts<'_>,
    ) -> Vec<Diagnostic> {
        let never = per_member.never();
        if never.is_empty() || never.len() == per_member.members.len() {
            return Vec::new();
        }
        crate::reachability::member_arm_verdicts(
            doc,
            body,
            &when.raw,
            per_member.members,
            &never,
            &self.defs,
            self.def_types,
            self.schema,
        )
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
        // A bare root read here is a member named like a state root, already
        // refused where it is declared: the guard means nothing to judge.
        if crate::cel_paths::collect_path_uses(expr)
            .iter()
            .any(|u| !u.path.contains('.') && self.schema.is_faulty(&u.path))
        {
            return None;
        }
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
                    "`!holds{n}` follows from this guard's `holds{p}`: `{}` excludes `{}` \
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

    /// dsl 0.5.2 §2.3 / 0.26.0: a load-bearing literal comparison — `S ==
    /// 'unset'` or a string outside `S`'s domain — already roots the dead
    /// guard per file (`E-UNSET-LITERAL` / `E-WHEN-LITERAL-DOMAIN`); the
    /// dead-arm derivative is owned by it here too.
    fn literal_owns(&self, slot: &CelSlot, dollar: Option<&DomainInfo>) -> bool {
        let a =
            analyze_literal_comparisons(&slot.raw, &self.defs, &self.ctx(dollar, slot.span, true));
        a.owns_dead_guard()
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
            if !self.literal_owns(slot, dollar) {
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
        if let Some(when) = &o.visible_when {
            if let Some(v) = self.eval(when, None) {
                if v.newly_false() {
                    out.push(diag(
                        W_OBJECTIVE_HIDDEN,
                        Severity::Warning,
                        format!(
                            "objective's `visibleWhen` `{}` is provably false — {}: it is never visible \
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
                Node::Hub(h) => {
                    self.choices(&h.choices, out);
                    if let Some(r) = &h.on_return {
                        self.walk(&r.body, out);
                    }
                }
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
                    self.exclusive_call(d, out);
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

    /// dsl 0.27.0 §4: the same judgment for the facts a directive call's
    /// declared `effects.asserts` write, against what holds at the call
    /// minus what its declared `retracts` drop first.
    fn exclusive_call(&self, d: &lute_syntax::ast::Directive, out: &mut Vec<Diagnostic>) {
        let Some(facts) = self.vocab.call_facts(d) else {
            return;
        };
        let dropped: Vec<crate::fact_env::QueryPattern> = facts
            .retracts
            .iter()
            .filter_map(crate::fact_env::QueryPattern::from_fact_pattern)
            .collect();
        let held = self.env.must.at(self.path, d.span);
        for p in &facts.asserts {
            let Some(fact) = GroundFact::from_pattern(p) else {
                continue;
            };
            let Some(m) = held.iter().find(|m| {
                !dropped.iter().any(|q| q.matches(&m.fact))
                    && !exclusive_pairs(&[fact.clone(), m.fact.clone()], self.vocab).is_empty()
            }) else {
                continue;
            };
            out.push(diag(
                E_FACT_EXCLUSIVE,
                Severity::Error,
                format!(
                    "`::{}` asserts `{fact}` (its declared `effects.asserts`), which would make \
                     `{fact}` and `{}` both hold: {}, and `{}` excludes `{}` (dsl 0.25.0 §1, \
                     dsl 0.27.0 §4) — declare `retracts: [\"{}\"]` on the directive, or call it \
                     only where `{}` does not hold",
                    d.tag,
                    m.fact,
                    guaranteed_reason(m),
                    fact.relation,
                    m.fact.relation,
                    m.fact,
                    m.fact
                ),
                d.span,
            ));
        }
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
            Node::Hub(h) => h.bodies().any(|b| self.has_dead_required_objective(b)),
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

/// Collect every relational query of a guard, in source order: each well-shaped
/// list-form `holds('rel', [args])` with its verdict, and each
/// `count('rel', [args]) ⋈ n` comparison the interval decides. Recurses like
/// `cel_resolve::check_fact_queries` (operator args, list elements, select
/// operands, `validAt`'s time argument).
fn collect_atoms(expr: &Expr, ctx: &DecideCtx<'_>, negated: bool, out: &mut Vec<Atom>) {
    match expr {
        Expr::Call(c) => {
            if crate::cel_resolve::is_profile_fact_query(c) {
                if c.func_name == "holds" {
                    if let Some(scope) = &ctx.facts {
                        if let Some(pattern) = QueryPattern::from_call(c) {
                            let verdict = match scope.holds(&pattern) {
                                HoldsVerdict::Impossible => HoldsOutcome::Impossible(
                                    scope
                                        .defeat(&pattern)
                                        .map(|(head, denied)| {
                                            format!(
                                                "`{head}` can only come from a rule that needs \
                                                 `not {denied}`, and `{denied}` holds throughout \
                                                 every run (a seed nothing removes, or derived \
                                                 from such seeds)"
                                            )
                                        })
                                        .or_else(|| {
                                            starved_reason(&pattern, &scope.starved(&pattern))
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
                    if let Some(t) = c.args.get(2) {
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
    let evidence = match crate::evidence::classification(code) {
        Some(crate::evidence::DiagnosticClass::Analysis { evidence }) => Some(evidence),
        _ => None,
    };
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
        evidence,
    }
}
