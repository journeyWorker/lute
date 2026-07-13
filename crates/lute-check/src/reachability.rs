//! §5.2/§5.3 whole-document reachability pass (dsl 0.4.0 T4/T5): `E-ARM-DEAD`
//! (dead guard + subsumption), `W-OTHERWISE-DEAD` (§5.2), and the quest
//! lifecycle (§5.3) — `E-QUEST-UNREACHABLE`, `E-OBJECTIVE-UNSATISFIABLE`,
//! `W-OBJECTIVE-HIDDEN`. Modeled on `check_line_codes` (`match_check.rs`,
//! called `check.rs:711`) — a free function walking the whole
//! [`Document`], called once in `check()` step 8. All analysis is LOCAL to
//! one `<match>`/`<branch>`/`<hub>`/`<quest>` (§5.2/§5.3 — no
//! cross-construct graph). Every diagnostic here is [`Layer::Logic`].
//!
//! ## The PROVABLE-ONLY boundary (§5.1)
//! `E-ARM-DEAD` fires ONLY when [`crate::decide::decide_slot`] resolves a
//! guard to `Some(Decided::Bool(false))`, or an arm's `is` pattern is
//! provably subsumed by earlier UNGUARDED sibling arms' `is` sets. An
//! UNDECIDED guard (`decide_slot` returns `None` — a state-path read, a fact
//! query, `now()`, …) is NEVER flagged: `decide()` already implements
//! exactly R1–R5 (closed, `decide.rs`), so correctly consuming its `Option`
//! result (never treating `None` as `false`) is the whole soundness argument
//! here.
//!
//! ## D4 (foreign literals)
//! `E-WHEN-LITERAL-DOMAIN` (`match_check.rs`) owns the foreign-`is`-literal
//! root. A literal outside the subject's decided finite domain contributes
//! NOTHING here: it is excluded from the subsumption union `U` and from
//! `W-OTHERWISE-DEAD`'s coverage computation, and an arm whose ENTIRE `is`
//! set is foreign (its residual is empty) is skipped by the dead-arm pass —
//! already rooted by the other code. `literal_is_foreign` (imported from
//! `match_check`) is the SAME classification `E-WHEN-LITERAL-DOMAIN` itself
//! uses, so the two diagnostics can never disagree about what's foreign.
//!
//! ## dsl 0.5.2 (`E-UNSET-LITERAL`)
//! A CEL guard slot comparing a maybe-unset finite-domain subject to the
//! FOREIGN string `'unset'` (`S ==/!= 'unset'`, either operand order,
//! possibly nested) is the most common misspelling of the DSL's *unset*
//! sentinel (CEL `null`, `0.1 §11.2`) — `E-UNSET-LITERAL` catches it,
//! independent of `decide()`'s outcome (fires for `!=`, which decides
//! TRUE and never reaches the dead-arm path, exactly like `==`). It OWNS
//! (suppresses) the derivative `E-ARM-DEAD` a `==` form would otherwise
//! cause — mirrors D4 above via the SAME
//! [`crate::decide::find_unset_sentinel_cmp`] detector `decide.rs`'s R2
//! itself resolves through, so the lint and R2 can never disagree.
//! `E-MAYBE-UNSET` is NOT a derivative — it stays independent (§4).

use std::collections::BTreeMap;

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_syntax::ast::{Arm, CelSlot, Document, Match, Node, Objective, Quest};

use crate::cel_expand::DefTable;
use crate::check::FoldedEnv;
use crate::decide::{decide_slot, unset_sentinel_in_slot, DecideCtx, Decided, DollarBinding};
use crate::match_check::{
    classify_when_literal, infer_domain, is_pattern_literals, literal_is_foreign, param_domain,
    subject_path, Domain, DomainInfo, DomainValue, WhenLiteral,
};

/// `E-ARM-DEAD` (dsl 0.4.0 §5.2): a `<when>` arm or `<choice>` that can
/// provably never fire — a decided-false guard, or an `is` pattern subsumed
/// by earlier unguarded sibling arms (first-match-wins, dsl §11.2).
pub(crate) const E_ARM_DEAD: &str = "E-ARM-DEAD";

/// `W-OTHERWISE-DEAD` (dsl 0.4.0 §5.2): an `<otherwise>` that is provably
/// unreachable because earlier unguarded `is` arms already cover the
/// subject's whole domain. A warning (not an error) — a defensive
/// `<otherwise>` is a legitimate hedge against schema evolution (0.3 §12).
pub(crate) const W_OTHERWISE_DEAD: &str = "W-OTHERWISE-DEAD";

/// `E-QUEST-UNREACHABLE` (dsl 0.4.0 §5.3): a `<quest>` that can provably
/// never complete — `start` decides false (never activates) or `fail`
/// decides true (fails at the first evaluation instant, precedence over
/// completion, `0.2 §6.3`). ONE diagnostic per quest naming whichever
/// standalone cause(s) hold (D21).
pub(crate) const E_QUEST_UNREACHABLE: &str = "E-QUEST-UNREACHABLE";

/// `E-OBJECTIVE-UNSATISFIABLE` (dsl 0.4.0 §5.3): an `<objective>` whose
/// `done` predicate decides false — it can never complete on any run. A
/// REQUIRED (`!optional`) objective additionally makes the enclosing quest
/// unreachable; that consequence rides as a NOTE on this diagnostic, never
/// as a second `E-QUEST-UNREACHABLE` (C4).
pub(crate) const E_OBJECTIVE_UNSATISFIABLE: &str = "E-OBJECTIVE-UNSATISFIABLE";

/// `W-OBJECTIVE-HIDDEN` (dsl 0.4.0 §5.3): a REQUIRED (`!optional`)
/// objective whose `when` visibility gate decides false — provably never
/// visible or tracked, yet still gates completion (the `0.2 §6.3` softlock
/// prose made checkable). A warning: `done` is evaluated independently of
/// visibility, so completion may still be reachable.
pub(crate) const W_OBJECTIVE_HIDDEN: &str = "W-OBJECTIVE-HIDDEN";

/// `E-UNSET-LITERAL` (dsl 0.5.2 §2): a CEL guard slot (`<when test>`,
/// `<choice when>`, `<match on>` subject, `<objective when/done>`, or
/// `<quest start/fail>`) comparing a maybe-unset finite-domain subject to
/// the FOREIGN string `'unset'` — the most common misspelling of the DSL's
/// *unset* sentinel (CEL `null`, `0.1 §11.2`), not the string `'unset'`. An
/// INDEPENDENT AST lint (§2.1): fires for BOTH `==` (decides false, R2) and
/// `!=` (decides true — never reaches the dead-arm path, yet the identical
/// mistake), regardless of `decide_slot`'s outcome, and possibly nested
/// inside a larger boolean expression. Owns (suppresses) the derivative
/// `E-ARM-DEAD`/`W-OTHERWISE-DEAD` it would otherwise produce (§2.3,
/// mirrors D4 above).
pub(crate) const E_UNSET_LITERAL: &str = "E-UNSET-LITERAL";

/// §5.2/§5.3 whole-document pass. Walks `doc.shots` + `doc.quests`
/// recursively (arm/choice/on/objective bodies, mirroring
/// `check_admission`'s walk, admission.rs:220-296); timeline clips carry no
/// arms and are skipped. `DefTable` is built from `folded.def_bodies` +
/// `folded.env.def_params` (D2) so a `test="@never"` guard hidden behind a
/// frontmatter `defs:` entry is caught exactly like an inline literal guard.
pub(crate) fn check_reachability(doc: &Document, folded: &FoldedEnv) -> Vec<Diagnostic> {
    let defs = DefTable {
        bodies: &folded.def_bodies,
        params: &folded.env.def_params,
    };
    // dsl 0.4.0 §6.2/§6.3 (finding 3): a STANDALONE component-file
    // self-check's OWN `params:` domain table — mirrors check.rs's
    // `Walker` `param_domains` construction (`param_domain(ty)`) and
    // `validate_components`'s per-component table (T7/T8) — empty for an
    // ordinary Scene/Quest walk. Without this, a bare-`@param` `<match>`
    // subject in a STANDALONE component self-check degraded to an
    // unresolved (`infer_domain`) domain: a `$`-comparison guard foreign to
    // the param's domain never decided (no E-ARM-DEAD) and a covered
    // `<otherwise>` never flagged W-OTHERWISE-DEAD — only the TRANSITIVE
    // `::use` import path (`walk_component_body`'s own reachability call)
    // diagnosed them.
    let param_domains: BTreeMap<String, DomainInfo> = if folded.typed.component.is_some() {
        folded
            .typed
            .params
            .iter()
            .map(|p| (p.name.clone(), param_domain(&p.ty)))
            .collect()
    } else {
        BTreeMap::new()
    };
    let base_ctx = DecideCtx {
        schema: &folded.env.state,
        dollar: None,
        params: &param_domains,
    };
    let mut diags = Vec::new();
    for shot in &doc.shots {
        walk_reach(&shot.body, &defs, &base_ctx, &mut diags);
    }
    for quest in &doc.quests {
        diags.extend(check_quest_reach(quest, &defs, &base_ctx));
        walk_reach(&quest.body, &defs, &base_ctx, &mut diags);
    }
    diags
}

/// Recurse a node stream exactly like `check_admission`'s `walk`
/// (admission.rs:220-296): `<match>` arm bodies, `<branch>`/`<hub>` choice
/// bodies, `<on>`/`<objective>` bodies. A `<match>` gets a FRESH `$` binding
/// — its own subject's domain (dsl 0.4.0 §5.2 rule: "for match arms,
/// `ctx.dollar = Domain(&infer_domain(subject))`"); `<branch>`/`<hub>`
/// choices decide with the incoming `ctx` unchanged (`dollar: None` at the
/// top level — "for choices, `dollar = None`"). Timeline clips (`ClipNode`)
/// carry no arms — skipped, like every other leaf node.
fn walk_reach(nodes: &[Node], defs: &DefTable<'_>, ctx: &DecideCtx<'_>, diags: &mut Vec<Diagnostic>) {
    for node in nodes {
        match node {
            Node::Match(m) => {
                // dsl 0.4.0 §6.2/§6.3 (finding 3): a bare `@param` subject
                // resolves against `ctx.params` FIRST — the STANDALONE
                // component-file self-check path's own domain table
                // (`check_reachability`, seeded from `folded.typed.component`)
                // — mirroring the TRANSITIVE `::use` walk's
                // `walk_component_body` (`param_domains.get(&name)`). A
                // state/path subject, or an `@param` name `ctx.params`
                // doesn't carry (an ordinary Scene/Quest walk, where
                // `ctx.params` is always empty), falls back to the ordinary
                // state-path `infer_domain`.
                let dom = crate::check::bare_param_ref(&m.subject.raw)
                    .and_then(|name| ctx.params.get(&name).cloned())
                    .unwrap_or_else(|| infer_domain(subject_path(m).as_deref(), ctx.schema));
                // dsl 0.5.2 §2.1: the `<match on>` SUBJECT is itself a
                // listed guard slot — checked against the OUTER `ctx` (the
                // subject's own comparison, if any, is evaluated BEFORE `$`
                // is bound to it below). No dead-arm derivative to own here
                // (a subject has no guarded body of its own), so no
                // suppression accompanies this one.
                if let Some(hit) = unset_sentinel_in_slot(&m.subject.raw, defs, ctx) {
                    diags.push(diag(
                        E_UNSET_LITERAL,
                        Severity::Error,
                        unset_literal_message(&hit.subject, hit.not_equals),
                        m.subject.span,
                    ));
                }
                let match_ctx = DecideCtx {
                    schema: ctx.schema,
                    dollar: Some(DollarBinding::Domain(&dom)),
                    params: ctx.params,
                };
                diags.extend(check_match_reach(m, defs, &match_ctx));
                for arm in &m.arms {
                    let body = match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => body,
                    };
                    walk_reach(body, defs, ctx, diags);
                }
            }
            Node::Branch(b) => {
                diags.extend(check_choices_reach(
                    b.choices
                        .iter()
                        .filter_map(|c| c.when.as_ref().map(|w| (w, c.span))),
                    defs,
                    ctx,
                ));
                for choice in &b.choices {
                    walk_reach(&choice.body, defs, ctx, diags);
                }
            }
            Node::Hub(h) => {
                diags.extend(check_choices_reach(
                    h.choices
                        .iter()
                        .filter_map(|c| c.when.as_ref().map(|w| (w, c.span))),
                    defs,
                    ctx,
                ));
                for choice in &h.choices {
                    walk_reach(&choice.body, defs, ctx, diags);
                }
            }
            Node::On(o) => walk_reach(&o.body, defs, ctx, diags),
            Node::Objective(o) => {
                diags.extend(check_objective_reach(o, defs, ctx));
                walk_reach(&o.body, defs, ctx, diags);
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
                        let sentinel = unset_sentinel_in_slot(&when.raw, defs, ctx);
                        if let Some(hit) = &sentinel {
                            diags.push(diag(
                                E_UNSET_LITERAL,
                                Severity::Error,
                                unset_literal_message(&hit.subject, hit.not_equals),
                                when.span,
                            ));
                        }
                        // §2.3: an unset-sentinel guard already owns
                        // `E-ARM-DEAD` — mirrors the D4 foreign-literal
                        // exclusion above.
                        if sentinel.is_none() {
                            if let Some(Decided::Bool(false)) = decide_slot(&when.raw, defs, ctx) {
                                diags.push(diag(
                                    E_ARM_DEAD,
                                    Severity::Error,
                                    "this gated line can never be shown: its `when` guard is provably false (dsl 0.4 §7.2, §5.2)".to_string(),
                                    when.span,
                                ));
                            }
                        }
                    }
                }
            }
            Node::Directive(_)
            | Node::Set(_)
            | Node::Timeline(_)
            | Node::Assert(_)
            | Node::Retract(_) => {}
        }
    }
}

/// One value the subsumption union `U` (or an arm's residual) tracks: a
/// concrete finite-domain literal, or the `unset` case — kept distinct from
/// [`DomainValue`] since `unset` is a membership fact about `maybe_unset`,
/// never a domain member (mirrors `match_check::ArmCoverage`).
enum CoverItem {
    Value(DomainValue),
    Unset,
}

/// The domain-valid contribution of one classified `is=` literal (D4):
/// `None` when `lit` is foreign to `dom` — owned by `E-WHEN-LITERAL-DOMAIN`
/// (`match_check::literal_is_foreign`, the SAME classification that code
/// uses).
fn domain_valid_item(lit_raw: &str, dom: &DomainInfo) -> Option<CoverItem> {
    let lit = classify_when_literal(lit_raw);
    if literal_is_foreign(&lit, dom) {
        return None;
    }
    Some(match lit {
        WhenLiteral::Bool(b) => CoverItem::Value(DomainValue::Bool(b)),
        WhenLiteral::Num(n) => CoverItem::Value(DomainValue::Num(n)),
        WhenLiteral::Str(s) => CoverItem::Value(DomainValue::Str(s)),
        WhenLiteral::Unset => CoverItem::Unset,
    })
}

/// D4: true when the arm's `is=` pattern carries AT LEAST ONE literal
/// foreign to `dom` (`domain_valid_item` returns `None` exactly for a
/// foreign literal — the SAME `literal_is_foreign` classification
/// `E-WHEN-LITERAL-DOMAIN` uses, `match_check.rs`). D4 (finding 2): the
/// foreign-literal code OWNS the root for such an arm — cause 1
/// (dead-guard) below MUST NOT also report `E-ARM-DEAD` on it, even when
/// the arm's guard independently decides false.
fn arm_has_foreign_literal(pat: &lute_syntax::ast::IsPattern, dom: &DomainInfo) -> bool {
    is_pattern_literals(&pat.raw, pat.span)
        .iter()
        .any(|(lit_raw, _)| domain_valid_item(lit_raw, dom).is_none())
}

/// The accumulated subsumption union `U` (dsl 0.4.0 §5.2 rule 2): every
/// domain-valid literal (+ the `unset` case) contributed by an earlier
/// UNGUARDED `<when>` arm, each remembering the FIRST arm that contributed
/// it (span + its own `is` pattern text) for the citation in the
/// `E-ARM-DEAD` message (the §5.4 worked example's "the earlier unguarded
/// arm at 2:3 (`gold | silver`)").
#[derive(Default)]
struct Coverage {
    values: BTreeMap<DomainValue, (Span, String)>,
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
            CoverItem::Unset => {
                if self.unset.is_none() {
                    self.unset = Some((span, pattern.to_string()));
                }
            }
        }
    }

    /// The (span, pattern) of the FIRST earlier arm that contributed `item`
    /// to `U`, or `None` when `item` isn't covered yet.
    fn source(&self, item: &CoverItem) -> Option<&(Span, String)> {
        match item {
            CoverItem::Value(v) => self.values.get(v),
            CoverItem::Unset => self.unset.as_ref(),
        }
    }
}

/// Per-`<match>` engine (dsl 0.4.0 §5.2), reused inside component bodies
/// (Task 7). `ctx.dollar` MUST be `Domain(&infer_domain(subject))` — the
/// caller (`walk_reach` here; Task 7's `walk_component_body`) builds it. An
/// unexpected shape (`None`/`Value`) degrades to an unresolved domain rather
/// than panicking, so no literal-domain claim is ever made without proof.
pub(crate) fn check_match_reach(m: &Match, defs: &DefTable<'_>, ctx: &DecideCtx<'_>) -> Vec<Diagnostic> {
    let dom = match &ctx.dollar {
        Some(DollarBinding::Domain(d)) => (*d).clone(),
        _ => DomainInfo {
            domain: Domain::Infinite,
            maybe_unset: false,
            resolved: false,
        },
    };
    let mut diags = Vec::new();
    let mut u = Coverage::default();
    let mut otherwise_span: Option<Span> = None;

    for arm in &m.arms {
        match arm {
            Arm::Otherwise { span, .. } => otherwise_span = Some(*span),
            Arm::When { is, test, span, .. } => {
                let mut dead = false;

                // dsl 0.5.2 §2.1: independent lint over the arm's `test`,
                // regardless of `decide_slot`'s outcome.
                let sentinel = if test.raw.trim().is_empty() {
                    None
                } else {
                    unset_sentinel_in_slot(&test.raw, defs, ctx)
                };
                if let Some(hit) = &sentinel {
                    diags.push(diag(
                        E_UNSET_LITERAL,
                        Severity::Error,
                        unset_literal_message(&hit.subject, hit.not_equals),
                        *span,
                    ));
                }

                // Cause 1: decided-false guard (dsl 0.4.0 §5.2 rule 1). A
                // guard present AND is-pattern present: the decided-false
                // guard alone kills the arm — same code, this cause named
                // (cause 2 is skipped once this fires). D4 (finding 2): an
                // arm whose `is=` pattern carries a foreign literal is
                // ALREADY rooted by `E-WHEN-LITERAL-DOMAIN` — that code
                // OWNS the root, so cause 1 MUST NOT also fire on it, even
                // when the guard independently decides false (avoids the
                // `is="platnum" test="1 > 2"` double-report). §2.3: an
                // unset-sentinel guard (`sentinel.is_some()`) is likewise
                // already rooted by `E-UNSET-LITERAL` above.
                let foreign_literal = is.as_ref().is_some_and(|pat| arm_has_foreign_literal(pat, &dom));
                if !foreign_literal && sentinel.is_none() && !test.raw.trim().is_empty() {
                    if let Some(Decided::Bool(false)) = decide_slot(&test.raw, defs, ctx) {
                        diags.push(diag(
                            E_ARM_DEAD,
                            Severity::Error,
                            dead_guard_message("arm", &test.raw),
                            *span,
                        ));
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
                            .filter_map(|(lit, _)| domain_valid_item(&lit, &dom))
                            .collect();
                        // A fully-foreign residual (D4-rooted) is skipped —
                        // `is_empty` covers both "no `is` literal survived
                        // the foreign filter" and (implicitly) "no `is` at
                        // all", since the `let Some(pat) = is` guard already
                        // excludes the latter.
                        if !residual.is_empty() {
                            let mut covering: Option<&(Span, String)> = None;
                            let mut fully_covered = true;
                            for item in &residual {
                                match u.source(item) {
                                    Some(src) => {
                                        if covering.is_none_or(|c| src.0.byte_start < c.0.byte_start)
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
                            if fully_covered {
                                if let Some((cov_span, cov_pattern)) = covering {
                                    diags.push(diag(
                                        E_ARM_DEAD,
                                        Severity::Error,
                                        subsumption_message(pat.raw.trim(), *cov_span, cov_pattern),
                                        *span,
                                    ));
                                }
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
                            if let Some(item) = domain_valid_item(&lit, &dom) {
                                u.add(item, *span, pat.raw.trim());
                            }
                        }
                    }
                }
            }
        }
    }

    // `W-OTHERWISE-DEAD` (dsl 0.4.0 §5.2 rule 3): requires a resolved FINITE
    // domain — an unresolved/infinite subject makes no "whole domain" claim
    // to violate.
    if let (Some(span), true, Domain::Finite(vals)) = (otherwise_span, dom.resolved, &dom.domain) {
        let fully_covered = vals.iter().all(|v| u.values.contains_key(v));
        if fully_covered && (u.unset.is_some() || !dom.maybe_unset) {
            diags.push(diag(
                W_OTHERWISE_DEAD,
                Severity::Warning,
                "`<otherwise>` can never fire: earlier unguarded `is` arms already cover the \
                 subject's whole domain (dsl 0.4 §5.2)"
                    .to_string(),
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
) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for (slot, span) in whens {
        if slot.raw.trim().is_empty() {
            continue;
        }
        // dsl 0.5.2 §2.1: independent lint, regardless of `decide_slot`'s
        // outcome.
        let sentinel = unset_sentinel_in_slot(&slot.raw, defs, ctx);
        if let Some(hit) = &sentinel {
            diags.push(diag(
                E_UNSET_LITERAL,
                Severity::Error,
                unset_literal_message(&hit.subject, hit.not_equals),
                span,
            ));
        }
        // §2.3: an unset-sentinel guard already owns `E-ARM-DEAD`.
        if sentinel.is_none() {
            if let Some(Decided::Bool(false)) = decide_slot(&slot.raw, defs, ctx) {
                diags.push(diag(
                    E_ARM_DEAD,
                    Severity::Error,
                    dead_guard_message("choice", &slot.raw),
                    span,
                ));
            }
        }
    }
    diags
}

/// Per-`<quest>` engine (dsl 0.4.0 §5.3 rule 2, D21): `start` deciding
/// false or `fail` deciding true each root `E-QUEST-UNREACHABLE` — ONE
/// diagnostic per quest naming whichever standalone cause(s) hold (a dead
/// `start` AND a true `fail` are DISTINCT roots, both named).
/// `start: None`/`fail: None` never fire — only an EXPLICIT guard can be
/// provably decided. `ctx.dollar` MUST be `None` — no `$` is in scope at a
/// quest's `start`/`fail` attrs (the base ctx `check_reachability` already
/// builds).
fn check_quest_reach(quest: &Quest, defs: &DefTable<'_>, ctx: &DecideCtx<'_>) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    // dsl 0.5.2 §2.1: `<quest start/fail>` are listed guard slots too — the
    // lint fires independently of `E-QUEST-UNREACHABLE`, which this spec
    // revision's §2.3 ownership clause does NOT scope (it names only
    // `E-ARM-DEAD`/`W-OTHERWISE-DEAD`), so no suppression accompanies this.
    for slot in [&quest.start, &quest.fail].into_iter().flatten() {
        if let Some(hit) = unset_sentinel_in_slot(&slot.raw, defs, ctx) {
            diags.push(diag(
                E_UNSET_LITERAL,
                Severity::Error,
                unset_literal_message(&hit.subject, hit.not_equals),
                slot.span,
            ));
        }
    }
    let dead_start = quest
        .start
        .as_ref()
        .is_some_and(|s| matches!(decide_slot(&s.raw, defs, ctx), Some(Decided::Bool(false))));
    let true_fail = quest
        .fail
        .as_ref()
        .is_some_and(|f| matches!(decide_slot(&f.raw, defs, ctx), Some(Decided::Bool(true))));
    if !dead_start && !true_fail {
        return diags;
    }
    diags.push(diag(
        E_QUEST_UNREACHABLE,
        Severity::Error,
        quest_unreachable_message(dead_start, true_fail),
        quest.span,
    ));
    diags
}

/// Per-`<objective>` engine (dsl 0.4.0 §5.3 rules 1 and 3). `done` deciding
/// false is `E-OBJECTIVE-UNSATISFIABLE` — appending the required-quest note
/// when `!optional` (C4: NEVER a second `E-QUEST-UNREACHABLE`; enforced
/// here by construction, since `check_quest_reach` never looks at
/// objectives at all). A REQUIRED objective (`!optional`) whose `when`
/// decides false is separately `W-OBJECTIVE-HIDDEN` — independent of
/// whether `done` is itself decided, since visibility and completion are
/// evaluated independently (§5.3). `ctx.dollar` MUST be `None` — no `$` is
/// in scope at an `<objective>`'s attrs.
fn check_objective_reach(o: &Objective, defs: &DefTable<'_>, ctx: &DecideCtx<'_>) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    // dsl 0.5.2 §2.1: `<objective when/done>` are listed guard slots too —
    // independent of `E-OBJECTIVE-UNSATISFIABLE`/`W-OBJECTIVE-HIDDEN`, which
    // §2.3's ownership clause does NOT scope (it names only
    // `E-ARM-DEAD`/`W-OTHERWISE-DEAD`), so no suppression accompanies this.
    if let Some(hit) = unset_sentinel_in_slot(&o.done.raw, defs, ctx) {
        diags.push(diag(
            E_UNSET_LITERAL,
            Severity::Error,
            unset_literal_message(&hit.subject, hit.not_equals),
            o.done.span,
        ));
    }
    if let Some(when) = &o.when {
        if let Some(hit) = unset_sentinel_in_slot(&when.raw, defs, ctx) {
            diags.push(diag(
                E_UNSET_LITERAL,
                Severity::Error,
                unset_literal_message(&hit.subject, hit.not_equals),
                when.span,
            ));
        }
    }
    if let Some(Decided::Bool(false)) = decide_slot(&o.done.raw, defs, ctx) {
        diags.push(diag(
            E_OBJECTIVE_UNSATISFIABLE,
            Severity::Error,
            objective_unsat_message(!o.optional, &o.done.raw),
            o.span,
        ));
    }
    if !o.optional {
        if let Some(when) = &o.when {
            if let Some(Decided::Bool(false)) = decide_slot(&when.raw, defs, ctx) {
                diags.push(diag(
                    W_OBJECTIVE_HIDDEN,
                    Severity::Warning,
                    objective_hidden_message(),
                    o.span,
                ));
            }
        }
    }
    diags
}

/// `E-QUEST-UNREACHABLE` message (dsl 0.4.0 §5.3 rule 2, D21): joins
/// whichever standalone cause(s) hold — never a `start`-only phrase when
/// `fail` also holds, or vice versa (the §5.4 worked example's
/// parenthetical: "distinct roots ... so both appear").
fn quest_unreachable_message(dead_start: bool, true_fail: bool) -> String {
    let mut causes = Vec::new();
    if dead_start {
        causes.push("`start` decides false — the quest never activates");
    }
    if true_fail {
        causes.push(
            "`fail` decides true — fail precedes completion (0.2 §6.3), so an activated \
             instance fails at the first evaluation instant",
        );
    }
    format!("quest can never complete: {} (dsl 0.4 §5.3)", causes.join("; "))
}

/// The §5.3/C4 quest-consequence note (Task 5 rules, quoted verbatim):
/// appended to [`objective_unsat_message`]'s output when the objective is
/// required (`!optional`).
const REQUIRED_QUEST_NOTE: &str =
    "; the objective — and, being required, the quest — can never complete (dsl 0.4 §5.3)";

/// `E-OBJECTIVE-UNSATISFIABLE` message (dsl 0.4.0 §5.3 rule 1): quotes the
/// `done` predicate's raw text (matching [`dead_guard_message`]'s style).
/// `required` appends [`REQUIRED_QUEST_NOTE`] verbatim; an `optional`
/// objective's dead `done` still fires the code (it too can never
/// complete), just without the quest-level consequence (C4).
fn objective_unsat_message(required: bool, raw: &str) -> String {
    let mut msg = format!(
        "`done` predicate `{}` is provably false: the objective can never complete on any run",
        raw.trim()
    );
    if required {
        msg.push_str(REQUIRED_QUEST_NOTE);
    } else {
        msg.push_str(" (dsl 0.4 §5.3)");
    }
    msg
}

/// `W-OBJECTIVE-HIDDEN` message (dsl 0.4.0 §5.3 rule 3): carries `0.2
/// §6.3`'s own advice — mark the objective `optional` or fix the gate.
fn objective_hidden_message() -> String {
    "objective's `when` is provably false: it is never visible or tracked, yet still gates \
     completion (dsl 0.4 §5.3) — mark it `optional` or fix the gate (0.2 §6.3)"
        .to_string()
}

/// Cause-1 message (dsl 0.4.0 §5.2 rule 1): names the guard text and states
/// it is provably false. `kind` is `"arm"` (a `<when test>`) or `"choice"`
/// (a `<choice when>`).
fn dead_guard_message(kind: &str, raw: &str) -> String {
    format!(
        "{kind} can never fire: guard `{}` is provably false (dsl 0.4 §5.2)",
        raw.trim()
    )
}

/// Cause-2 message (dsl 0.4.0 §5.2 rule 2), matching the §5.4 worked
/// example's shape: `` arm can never fire: its pattern `gold` is fully
/// covered by the earlier unguarded arm at 2:3 (`gold | silver`) —
/// first-match-wins (dsl 0.4 §5.2) ``.
fn subsumption_message(pattern: &str, cov_span: Span, cov_pattern: &str) -> String {
    format!(
        "arm can never fire: its pattern `{pattern}` is fully covered by the earlier \
         unguarded arm at {}:{} (`{cov_pattern}`) — first-match-wins (dsl 0.4 §5.2)",
        cov_span.line, cov_span.column
    )
}

/// `E-UNSET-LITERAL` message (dsl 0.5.2 §2.2): names the subject and BOTH
/// supported forms — `!isSet(path)` first, then the `<match on><when
/// is="unset">` literal arm — so the message doubles as copy-paste-able fix
/// guidance.
fn unset_literal_message(subject: &str, not_equals: bool) -> String {
    let cmp = if not_equals { "!=" } else { "==" };
    format!(
        "comparing `{subject}` {cmp} the string `'unset'`, which is never equal to the DSL's \
         unset sentinel (the CEL `null` literal, dsl 0.1 §11.2). Test for unset with \
         `!isSet({subject})`, or in a `<match on=\"{subject}\">` use `<when is=\"unset\">` \
         (dsl 0.2 §5.2)"
    )
}

/// Build a `Layer::Logic` diagnostic (a §5.2 reachability check).
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
