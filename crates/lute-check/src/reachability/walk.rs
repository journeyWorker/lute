use std::collections::BTreeMap;

use lute_core_span::{Diagnostic, Severity, Span};
use lute_syntax::ast::{Arm, AttrValue, CelSlot, Match, Node};

use crate::cel_expand::DefTable;
use crate::decide::{analyze_literal_comparisons, decide_slot, DecideCtx, Decided, DollarBinding};
use crate::match_check::{is_pattern_literals, literal_is_foreign, quest_state_is_literal, subject_path, CoverItem, Domain, DomainInfo, DomainValue, Interval, NumCoverage};
 
use lute_syntax::is_pattern::{classify_is_literal, IsLiteral};

use super::context::{Assumption, Reach};
use super::diagnostics::{
    assumed_dead_message, check_code_after_end, check_code_after_next, dead_guard_message, diag,
    push_literal_cmp_diags, subsumption_message,
};
use super::{E_ARM_DEAD, W_OTHERWISE_DEAD};
use super::objective::check_objective_reach;
use super::picks::Pick;

pub(super) fn walk_reach(
    nodes: &[Node],
    defs: &DefTable<'_>,
    rx: &Reach<'_>,
    ctx: &DecideCtx<'_>,
    diags: &mut Vec<Diagnostic>,
) {
    check_code_after_end(nodes, rx.targets, diags);
    check_code_after_next(nodes, rx.targets, diags);
    for node in nodes {
        match node {
            Node::Match(m) => {
                // dsl 0.4.0 §6.2/§6.3 (finding 3): a bare `@param` subject
                // resolves against `ctx.params` FIRST — the STANDALONE
                // component-file self-check path's own domain table
                // (`check_reachability`, seeded from `folded.typed.component`)
                // — mirroring the TRANSITIVE `::use` walk's
                // `walk_component_body` (`param_domains.get(&name)`). A
                // state/path subject, a `@def` subject (dsl 0.24.0), or an
                // `@param` name `ctx.params` doesn't carry (an ordinary
                // Scene/Quest walk, where `ctx.params` is always empty), is
                // resolved like the checker's own `<match>` pass does.
                let (subject, dom) = match crate::check::bare_param_ref(&m.subject.raw)
                    .and_then(|name| ctx.params.get(&name).cloned())
                {
                    Some(dom) => (subject_path(m), dom),
                    None => crate::match_check::resolve_subject(m, defs, rx.def_types, ctx.schema),
                };
                // dsl 0.5.2 §2.1: the `<match on>` SUBJECT is itself a
                // listed guard slot — checked against the OUTER `ctx` (the
                // subject's own comparison, if any, is evaluated BEFORE `$`
                // is bound to it below). No dead-arm derivative to own here
                // (a subject has no guarded body of its own), so no
                // suppression accompanies this one.
                let subject_analysis = analyze_literal_comparisons(&m.subject.raw, defs, ctx);
                push_literal_cmp_diags(
                    diags,
                    &subject_analysis.hits,
                    Some(&m.subject.raw),
                    m.subject.span,
                );
                let match_ctx = DecideCtx {
                    schema: ctx.schema,
                    dollar: Some(DollarBinding::Domain(&dom)),
                    params: ctx.params,
                    facts: ctx.facts,
                };
                diags.extend(check_match_reach(
                    m,
                    subject.as_deref(),
                    defs,
                    &match_ctx,
                    rx.assume,
                    rx.picks,
                ));
                for arm in &m.arms {
                    let body = match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => body,
                    };
                    walk_reach(body, defs, rx, ctx, diags);
                }
            }
            Node::Branch(b) => {
                diags.extend(super::check_choices_reach(
                    b.choices
                        .iter()
                        .filter_map(|c| c.when.as_ref().map(|w| (w, c.span))),
                    defs,
                    ctx,
                    rx.picks,
                ));
                // dsl 0.28.0 (T1-23): inside an option's arm its pick is recorded.
                for choice in &b.choices {
                    let picks = Pick::with(rx.picks, &b.id, choice, false, rx.targets);
                    let inner = Reach {
                        picks: &picks,
                        ..*rx
                    };
                    walk_reach(&choice.body, defs, &inner, ctx, diags);
                }
            }
            Node::Hub(h) => {
                diags.extend(super::check_choices_reach(
                    h.choices
                        .iter()
                        .filter_map(|c| c.when.as_ref().map(|w| (w, c.span))),
                    defs,
                    ctx,
                    rx.picks,
                ));
                let id = h
                    .attrs
                    .iter()
                    .find(|a| a.key == "id")
                    .and_then(|a| match &a.value {
                        AttrValue::Str(s) => Some(s.as_str()),
                        _ => None,
                    });
                // A hub's bodies run again on every visit.
                for choice in &h.choices {
                    let picks = Pick::with(rx.picks, id.unwrap_or(""), choice, true, rx.targets);
                    let inner = Reach {
                        picks: &picks,
                        once: false,
                        ..*rx
                    };
                    walk_reach(&choice.body, defs, &inner, ctx, diags);
                }
                // dsl 0.28.0 §5: the `<return>` body follows whichever option ran.
                if let Some(r) = &h.on_return {
                    let picks = Pick::on_return(rx.picks, id.unwrap_or(""), h, rx.targets, rx.once);
                    let inner = Reach {
                        picks: &picks,
                        once: false,
                        ..*rx
                    };
                    walk_reach(&r.body, defs, &inner, ctx, diags);
                }
            }
            Node::On(o) => {
                // dsl 0.5.2 §2.1: `<on when>` is a listed guard slot too
                // (0.2 §4.1 CelString gate, "any profile CEL slot"). No
                // dead-arm derivative to own here (an `<on>` handler has no
                // reachability code of its own), so no suppression
                // accompanies this — mirrors the `<match on>` subject and
                // quest/objective slots above.
                if let Some(when) = &o.when {
                    let analysis = analyze_literal_comparisons(&when.raw, defs, ctx);
                    push_literal_cmp_diags(diags, &analysis.hits, Some(&when.raw), when.span);
                }
                // A handler may run many times.
                let inner = Reach { once: false, ..*rx };
                walk_reach(&o.body, defs, &inner, ctx, diags);
            }
            Node::Objective(o) => {
                diags.extend(check_objective_reach(o, defs, ctx));
                walk_reach(&o.body, defs, rx, ctx, diags);
            }
            Node::Line(l) => {
                // dsl 0.4.0 §7.2: a gated line (`when=`) is a one-arm
                // construct — the SAME cause-1 rule a `<when test>` arm gets
                // (§5.2 rule 1): a guard that decides false makes the line
                // provably dead. No subsumption/`is` pattern applies (a line
                // guard has none), so only `decide_slot` matters here.
                if let Some(when) = &l.when {
                    if !when.raw.trim().is_empty() {
                        // dsl 0.5.2 §2.1: independent lint, regardless of
                        // `decide_slot`'s outcome.
                        let analysis = analyze_literal_comparisons(&when.raw, defs, ctx);
                        push_literal_cmp_diags(diags, &analysis.hits, Some(&when.raw), when.span);
                        // §2.3: suppress `E-ARM-DEAD` only when the literal
                        // comparison(s) are LOAD-BEARING for the
                        // decided-false — an independently-dead guard (a
                        // literal `false`, `@never`, …) must still flag it.
                        let suppress_arm_dead = analysis.owns_dead_guard();
                        if !suppress_arm_dead {
                            if let Some(Decided::Bool(false)) = decide_slot(&when.raw, defs, ctx) {
                                let mut message = "this gated line can never be shown: its \
                                                   `when` guard is provably false (dsl 0.4 \
                                                   §7.2, §5.2)"
                                    .to_string();
                                if let Some(why) = crate::clock::false_reason(&when.raw, defs, ctx)
                                {
                                    message.push_str(&format!(" — {why}"));
                                }
                                diags.push(diag(E_ARM_DEAD, Severity::Error, message, when.span));
                            } else if let Some(pick) =
                                Pick::deciding_false(rx.picks, &when.raw, defs, ctx)
                            {
                                diags.push(diag(
                                    E_ARM_DEAD,
                                    Severity::Error,
                                    format!(
                                        "this gated line can never be shown: its `when` guard is \
                                         provably false here — {}",
                                        pick.why()
                                    ),
                                    when.span,
                                ));
                            }
                        }
                    }
                }
            }
            // dsl 0.12.0: a guarded `::next{when=}` is a one-arm construct
            // exactly like a gated line above — a decided-false guard makes
            // the jump provably dead. An UNGUARDED `::next` needs no guard
            // analysis here (that is `check_code_after_next`'s job, above).
            // dsl 0.26.0 §4: any guarded directive likewise.
            Node::Directive(d) if d.when.is_some() => {
                let what = if d.tag == lute_manifest::core::NEXT_DIRECTIVE {
                    "this `::next` never fires: its `when` guard is provably false (dsl 0.12.0)"
                        .to_string()
                } else {
                    format!(
                        "this `::{}` never runs: its `when` guard is provably false (dsl 0.26.0 §4)",
                        d.tag
                    )
                };
                guard_reach(d.when.as_ref(), what, defs, ctx, rx.picks, diags);
            }
            // dsl 0.24.0 §1: a guarded `::set{… when=}` is the same one-arm
            // construct — a decided-false guard makes the write provably dead.
            Node::Set(s) if s.when.is_some() => guard_reach(
                s.when.as_ref(),
                "this `::set` never writes: its `when` guard is provably false (dsl 0.24.0 §1)"
                    .to_string(),
                defs,
                ctx,
                rx.picks,
                diags,
            ),
            // dsl 0.26.0 §4: so is a guarded `::assert` / `::retract`.
            Node::Assert(a) if a.when.is_some() => guard_reach(
                a.when.as_ref(),
                "this `::assert` never writes: its `when` guard is provably false (dsl 0.26.0 §4)"
                    .to_string(),
                defs,
                ctx,
                rx.picks,
                diags,
            ),
            Node::Retract(r) if r.when.is_some() => guard_reach(
                r.when.as_ref(),
                "this `::retract` never writes: its `when` guard is provably false (dsl 0.26.0 §4)"
                    .to_string(),
                defs,
                ctx,
                rx.picks,
                diags,
            ),
            Node::Directive(_)
            | Node::Set(_)
            | Node::Timeline(_)
            | Node::Assert(_)
            | Node::Retract(_) => {}
        }
    }
}

/// A one-arm guard's §5.2 cause-1 check (`E-ARM-DEAD` when it decides
/// false, `what` the message) and its `unset`-literal lint. A diagnostic
/// already raised at the same guard is not raised again: a guarded
/// `::use`'s spliced writes carry its guard (dsl 0.26.0 §4), and the
/// `::use` itself speaks for them.
fn guard_reach(
    when: Option<&CelSlot>,
    what: String,
    defs: &DefTable<'_>,
    ctx: &DecideCtx<'_>,
    picks: &[Pick],
    diags: &mut Vec<Diagnostic>,
) {
    let Some(when) = when.filter(|w| !w.raw.trim().is_empty()) else {
        return;
    };
    let mut own = Vec::new();
    let analysis = analyze_literal_comparisons(&when.raw, defs, ctx);
    push_literal_cmp_diags(&mut own, &analysis.hits, Some(&when.raw), when.span);
    let suppress_arm_dead = analysis.owns_dead_guard();
    if !suppress_arm_dead {
        if let Some(Decided::Bool(false)) = decide_slot(&when.raw, defs, ctx) {
            own.push(diag(E_ARM_DEAD, Severity::Error, what, when.span));
        } else if let Some(pick) = Pick::deciding_false(picks, &when.raw, defs, ctx) {
            // dsl 0.28.0 (T1-23): false inside the option's own arm.
            let base = what.find(" (dsl").map_or(what.as_str(), |i| &what[..i]);
            let what = format!("{base} here — {}", pick.why());
            own.push(diag(E_ARM_DEAD, Severity::Error, what, when.span));
        }
    }
    for d in own {
        if !diags.iter().any(|x| x.code == d.code && x.span == d.span) {
            diags.push(d);
        }
    }
}

/// The domain-valid contribution of one `is=` literal (D4): `None` when
/// `lit_raw` is foreign to `dom` — owned by `E-WHEN-LITERAL-DOMAIN`
/// (`match_check::literal_is_foreign`, the SAME classification that code
/// uses) — or a malformed/empty range, owned by `E-WHEN-RANGE` (dsl 0.18.0
/// §2: such a literal covers nothing). `subject` is the `<match on>` path:
/// on a `quest.<id>.state` subject `unset` is the lifecycle member
/// (`match_check::quest_state_is_literal`, 0.21.1 T1-1).
fn domain_valid_item(lit_raw: &str, dom: &DomainInfo, subject: Option<&str>) -> Option<CoverItem> {
    let lit = quest_state_is_literal(classify_is_literal(lit_raw).ok()?, subject);
    if literal_is_foreign(&lit, dom) {
        return None;
    }
    Some(match lit {
        IsLiteral::Bool(b) => CoverItem::Value(DomainValue::Bool(b)),
        IsLiteral::Str(s) => CoverItem::Value(DomainValue::Str(s)),
        IsLiteral::Unset => CoverItem::Unset,
        IsLiteral::Num(_) | IsLiteral::Range(_) => CoverItem::Num(Interval::of(&lit)?),
    })
}

/// D4: true when the arm's `is=` pattern carries AT LEAST ONE literal
/// foreign to `dom` or a malformed/empty range (`domain_valid_item` returns
/// `None` exactly for those — the SAME classification
/// `E-WHEN-LITERAL-DOMAIN`/`E-WHEN-RANGE` use, `match_check.rs`). D4
/// (finding 2): the literal-level code OWNS the root for such an arm —
/// cause 1 (dead-guard) below MUST NOT also report `E-ARM-DEAD` on it, even
/// when the arm's guard independently decides false.
pub(crate) fn arm_has_foreign_literal(
    pat: &lute_syntax::ast::IsPattern,
    dom: &DomainInfo,
    subject: Option<&str>,
) -> bool {
    is_pattern_literals(&pat.raw, pat.span)
        .iter()
        .any(|(lit_raw, _)| domain_valid_item(lit_raw, dom, subject).is_none())
}

/// The accumulated subsumption union `U` (dsl 0.4.0 §5.2 rule 2): every
/// domain-valid literal (+ the `unset` case) contributed by an earlier
/// UNGUARDED `<when>` arm, each remembering the FIRST arm that contributed
/// it (span + its own `is` pattern text) for the citation in the
/// `E-ARM-DEAD` message (the §5.4 worked example's "the earlier unguarded
/// arm at 2:3 (`gold | silver`)"). Numeric literals (dsl 0.18.0 §4) fold
/// into the merged interval union `num`; `num_sources` keeps each
/// contribution in arm order for the citation.
#[derive(Default)]
struct Coverage {
    values: BTreeMap<DomainValue, (Span, String)>,
    num: NumCoverage,
    num_sources: Vec<(Interval, (Span, String))>,
    unset: Option<(Span, String)>,
}

impl Coverage {
    fn add(&mut self, item: CoverItem, span: Span, pattern: &str) {
        match item {
            CoverItem::Value(v) => {
                self.values
                    .entry(v)
                    .or_insert_with(|| (span, pattern.to_string()));
            }
            CoverItem::Num(iv) => {
                self.num.add(iv);
                self.num_sources.push((iv, (span, pattern.to_string())));
            }
            CoverItem::Unset => {
                if self.unset.is_none() {
                    self.unset = Some((span, pattern.to_string()));
                }
            }
        }
    }

    /// The (span, pattern) of the earlier arm that contributed `item` to
    /// `U`, plus whether `item` needed more than one earlier arm, or `None`
    /// when `item` isn't covered yet. An interval covered by one earlier arm
    /// alone cites that arm; one covered only by several arms together cites
    /// the earliest of them and reports `joint`.
    fn source(&self, item: &CoverItem) -> Option<(&(Span, String), bool)> {
        match item {
            CoverItem::Value(v) => self.values.get(v).map(|s| (s, false)),
            CoverItem::Num(iv) => {
                if !self.num.contains(*iv) {
                    return None;
                }
                if let Some((_, cite)) = self
                    .num_sources
                    .iter()
                    .find(|(src, _)| src.lo <= iv.lo && iv.hi <= src.hi)
                {
                    return Some((cite, false));
                }
                self.num_sources
                    .iter()
                    .find(|(src, _)| src.lo <= iv.hi && iv.lo <= src.hi)
                    .map(|(_, cite)| (cite, true))
            }
            CoverItem::Unset => self.unset.as_ref().map(|s| (s, false)),
        }
    }
}

/// Per-`<match>` engine (dsl 0.4.0 §5.2). `ctx.dollar` MUST be
/// `Domain(&dom)` for the subject's resolved domain — [`walk_reach`], the sole
/// caller, builds it, for a root document and for a component body alike
/// (Task 7e: the arm-local call `walk_component_body` used to make was folded
/// into the whole-body [`check_reachability_in`] walk). An unexpected shape
/// (`None`/`Value`) degrades to an unresolved domain rather than panicking, so
/// no literal-domain claim is ever made without proof. `subject` is the
/// resolved subject path; `assume` the enclosing body's [`Assumption`] (dsl
/// 0.24.0): a literal it rules out is as dead as one an earlier arm covers.
/// `picks` are the pick records of the options enclosing the match (dsl
/// 0.28.0, [`Pick`]): a value other than the recorded one is dead too.
fn check_match_reach(
    m: &Match,
    subject: Option<&str>,
    defs: &DefTable<'_>,
    ctx: &DecideCtx<'_>,
    assume: Option<&Assumption>,
    picks: &[Pick],
) -> Vec<Diagnostic> {
    let dom = match &ctx.dollar {
        Some(DollarBinding::Domain(d)) => (*d).clone(),
        _ => DomainInfo {
            domain: Domain::Infinite,
            maybe_unset: false,
            resolved: false,
        },
    };
    let mut diags = Vec::new();
    // dsl 0.28.0 (T3-40): a slot a finite clock never reaches needs no arm.
    let unreached = |item: &CoverItem| match item {
        CoverItem::Value(DomainValue::Str(s)) => {
            subject.is_some_and(|p| ctx.schema.clock_excludes(p, s))
        }
        _ => false,
    };
    let picked = |item: &CoverItem| subject.and_then(|p| Pick::ruling_out(picks, p, item));
    let ruled_out = |item: &CoverItem| {
        unreached(item)
            || picked(item).is_some()
            || subject.is_some_and(|p| assume.is_some_and(|a| a.rules_out(p, item, ctx.schema)))
    };
    let clock_reason = subject.and_then(|p| crate::clock::slot_end_reason(ctx.schema, p));
    let mut u = Coverage::default();
    let mut otherwise_span: Option<Span> = None;

    for arm in &m.arms {
        match arm {
            Arm::Otherwise { span, .. } => otherwise_span = Some(*span),
            Arm::When { is, test, span, .. } => {
                let mut dead = false;

                // dsl 0.5.2 §2.1: independent lint over the arm's `test`,
                // regardless of `decide_slot`'s outcome.
                let analysis = if test.raw.trim().is_empty() {
                    None
                } else {
                    Some(analyze_literal_comparisons(&test.raw, defs, ctx))
                };
                if let Some(a) = &analysis {
                    push_literal_cmp_diags(&mut diags, &a.hits, Some(&test.raw), test.span);
                }

                // Cause 1: decided-false guard (dsl 0.4.0 §5.2 rule 1). A
                // guard present AND is-pattern present: the decided-false
                // guard alone kills the arm — same code, this cause named
                // (cause 2 is skipped once this fires). D4 (finding 2): an
                // arm whose `is=` pattern carries a foreign literal is
                // ALREADY rooted by `E-WHEN-LITERAL-DOMAIN` — that code
                // OWNS the root, so cause 1 MUST NOT also fire on it, even
                // when the guard independently decides false (avoids the
                // `is="platnum" test="1 > 2"` double-report). §2.3: a
                // LOAD-BEARING literal comparison (`'unset'` sentinel or a
                // foreign member) is likewise already rooted above — an
                // independently-dead guard (a literal `false`, `@never`, …)
                // still flags E-ARM-DEAD even when such a comparison is ALSO
                // present.
                let foreign_literal = is
                    .as_ref()
                    .is_some_and(|pat| arm_has_foreign_literal(pat, &dom, subject));
                let literal_cmp_owns = analysis.as_ref().is_some_and(|a| a.owns_dead_guard());
                if !foreign_literal && !literal_cmp_owns && !test.raw.trim().is_empty() {
                    if let Some(Decided::Bool(false)) = decide_slot(&test.raw, defs, ctx) {
                        let mut message = dead_guard_message("arm", &test.raw);
                        if let Some(why) = crate::clock::false_reason(&test.raw, defs, ctx) {
                            message.push_str(&format!(" — {why}"));
                        }
                        diags.push(diag(E_ARM_DEAD, Severity::Error, message, *span));
                        dead = true;
                    } else if let Some(pick) = Pick::deciding_false(picks, &test.raw, defs, ctx) {
                        let message = format!(
                            "arm can never fire: guard `{}` is provably false here — {}",
                            test.raw.trim(),
                            pick.why()
                        );
                        diags.push(diag(E_ARM_DEAD, Severity::Error, message, *span));
                        dead = true;
                    }
                }

                // Cause 2: subsumption. "A guard cannot resurrect a subsumed
                // pattern" — this runs even when the arm carries a (live or
                // undecided) `test`, only short-circuited once cause 1
                // already flagged this SAME arm (one E-ARM-DEAD per arm).
                if !dead {
                    if let Some(pat) = is {
                        let residual: Vec<CoverItem> = is_pattern_literals(&pat.raw, pat.span)
                            .into_iter()
                            .filter_map(|(lit, _)| domain_valid_item(&lit, &dom, subject))
                            .collect();
                        // A fully-foreign residual (D4-rooted) is skipped —
                        // `is_empty` covers both "no `is` literal survived
                        // the foreign filter" and (implicitly) "no `is` at
                        // all", since the `let Some(pat) = is` guard already
                        // excludes the latter.
                        if !residual.is_empty() {
                            let mut covering: Option<&(Span, String)> = None;
                            let mut joint = false;
                            let mut fully_covered = true;
                            let mut by_assumption = false;
                            let mut by_clock = false;
                            let mut by_pick: Option<&Pick> = None;
                            for item in &residual {
                                if unreached(item) {
                                    by_clock = true;
                                    continue;
                                }
                                if let Some(pk) = picked(item) {
                                    by_pick = Some(pk);
                                    continue;
                                }
                                if ruled_out(item) {
                                    by_assumption = true;
                                    continue;
                                }
                                match u.source(item) {
                                    Some((src, item_joint)) => {
                                        joint |=
                                            item_joint || covering.is_some_and(|c| c.0 != src.0);
                                        if covering
                                            .is_none_or(|c| src.0.byte_start < c.0.byte_start)
                                        {
                                            covering = Some(src);
                                        }
                                    }
                                    None => {
                                        fully_covered = false;
                                        break;
                                    }
                                }
                            }
                            let assumed = assume.filter(|_| by_assumption);
                            let clocked = clock_reason.as_deref().filter(|_| by_clock);
                            let message = match (fully_covered, covering, assumed) {
                                (false, _, _) => None,
                                (true, Some((cov_span, cov_pattern)), assumed) => {
                                    let mut msg = subsumption_message(
                                        pat.raw.trim(),
                                        *cov_span,
                                        cov_pattern,
                                        joint,
                                    );
                                    if let Some(a) = assumed {
                                        msg.push_str(&format!(
                                            "; the rest is ruled out by the body's `when` guard \
                                             `{}`",
                                            a.raw
                                        ));
                                    }
                                    if let Some(why) = clocked {
                                        msg.push_str(&format!("; the rest never comes: {why}"));
                                    }
                                    Some(msg)
                                }
                                (true, None, Some(a)) => {
                                    let mut msg = assumed_dead_message(pat.raw.trim(), &a.raw);
                                    if let Some(why) = clocked {
                                        msg.push_str(&format!("; the rest never comes: {why}"));
                                    }
                                    Some(msg)
                                }
                                (true, None, None) => clocked.map(|why| {
                                    format!(
                                        "arm can never fire: its pattern `{}` never comes — {why}",
                                        pat.raw.trim()
                                    )
                                }),
                            };
                            // dsl 0.28.0 (T1-23): the option's own pick record.
                            let message = match (fully_covered, message, by_pick) {
                                (true, Some(m), Some(pk)) => {
                                    Some(format!("{m}; the rest is ruled out: {}", pk.why()))
                                }
                                (true, None, Some(pk)) => Some(format!(
                                    "arm can never fire: its pattern `{}` never matches here — {}",
                                    pat.raw.trim(),
                                    pk.why()
                                )),
                                (_, m, _) => m,
                            };
                            if let Some(message) = message {
                                diags.push(diag(E_ARM_DEAD, Severity::Error, message, *span));
                            }
                        }
                    }
                }

                // Accumulate U from UNGUARDED arms only (dsl 0.4.0 §5.2 rule
                // 2: "earlier, unguarded (`test`-less) sibling arms") —
                // regardless of whether this arm was itself just flagged (a
                // subsumed arm's own domain-valid literals are already a
                // subset of U, so re-adding them changes nothing).
                if test.raw.trim().is_empty() {
                    if let Some(pat) = is {
                        for (lit, _) in is_pattern_literals(&pat.raw, pat.span) {
                            if let Some(item) = domain_valid_item(&lit, &dom, subject) {
                                u.add(item, *span, pat.raw.trim());
                            }
                        }
                    }
                }
            }
        }
    }

    // `W-OTHERWISE-DEAD` (dsl 0.4.0 §5.2 rule 3): requires a resolved FINITE
    // domain, or a `number` subject whose whole real line is covered (dsl
    // 0.18.0 §4) — an unresolved/infinite subject makes no "whole domain"
    // claim to violate. A member the body's `when` rules out (dsl 0.24.0)
    // needs no arm.
    let narrowed = std::cell::Cell::new(false);
    let covered_or_ruled_out = |covered: bool, item: CoverItem| {
        covered || {
            let out = ruled_out(&item);
            narrowed.set(narrowed.get() | out);
            out
        }
    };
    let domain_covered = dom.resolved
        && match &dom.domain {
            Domain::Finite(vals) => vals.iter().all(|v| {
                covered_or_ruled_out(u.values.contains_key(v), CoverItem::Value(v.clone()))
            }),
            Domain::IntRange { lo, hi } => (*lo..=*hi).all(|k| {
                let p = Interval {
                    lo: k as f64,
                    hi: k as f64,
                };
                covered_or_ruled_out(u.num.contains(p), CoverItem::Num(p))
            }),
            Domain::Number => u.num.covers_all(),
            Domain::IntNumber => u.num.covers_all(),
            Domain::Infinite => false,
        };
    if let (Some(span), true) = (otherwise_span, domain_covered) {
        if u.unset.is_some() || !dom.maybe_unset || covered_or_ruled_out(false, CoverItem::Unset) {
            let whole = match assume.filter(|_| narrowed.get()) {
                Some(a) => format!("the domain left by the body's `when` guard `{}`", a.raw),
                None if narrowed.get() => "the values the subject can hold here".to_string(),
                None => "the subject's whole domain".to_string(),
            };
            diags.push(diag(
                W_OTHERWISE_DEAD,
                Severity::Warning,
                format!(
                    "`<otherwise>` can never fire: earlier unguarded `is` arms already cover \
                     {whole} (dsl 0.4 §5.2)"
                ),
                span,
            ));
        }
    }

    diags
}

/// `<branch>`/`<hub>` choice engine (dsl 0.4.0 §5.2). A `<choice when>` has
/// no `is` pattern (subsumption doesn't apply — only a `<when>` arm's `is`
/// set can be subsumed), so only cause 1 (decided-false guard) fires here.
/// `ctx.dollar` MUST be `None` — no `$` is in scope at a `<choice when>`.
pub(crate) fn check_choices_reach<'a>(
    whens: impl Iterator<Item = (&'a CelSlot, Span)>,
    defs: &DefTable<'_>,
    ctx: &DecideCtx<'_>,
    picks: &[Pick],
) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for (slot, span) in whens {
        if slot.raw.trim().is_empty() {
            continue;
        }
        // dsl 0.5.2 §2.1: independent lint, regardless of `decide_slot`'s
        // outcome.
        let analysis = analyze_literal_comparisons(&slot.raw, defs, ctx);
        push_literal_cmp_diags(&mut diags, &analysis.hits, Some(&slot.raw), slot.span);
        // §2.3: suppress `E-ARM-DEAD` only when the literal comparison(s)
        // are LOAD-BEARING for the decided-false (mirrors the arm-level
        // causality check above).
        let suppress_arm_dead = analysis.owns_dead_guard();
        if !suppress_arm_dead {
            if let Some(Decided::Bool(false)) = decide_slot(&slot.raw, defs, ctx) {
                diags.push(diag(
                    E_ARM_DEAD,
                    Severity::Error,
                    dead_guard_message("choice", &slot.raw),
                    span,
                ));
            } else if let Some(pick) = Pick::deciding_false(picks, &slot.raw, defs, ctx) {
                diags.push(diag(
                    E_ARM_DEAD,
                    Severity::Error,
                    format!(
                        "choice can never fire: guard `{}` is provably false here — {}",
                        slot.raw.trim(),
                        pick.why()
                    ),
                    span,
                ));
            }
        }
    }
    diags
}

