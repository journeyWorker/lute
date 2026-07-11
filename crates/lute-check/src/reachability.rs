//! §5.2/§5.3 whole-document reachability pass (dsl 0.4.0 T4/T5): `E-ARM-DEAD`
//! (dead guard + subsumption) and `W-OTHERWISE-DEAD`. Modeled on
//! `check_line_codes` (`match_check.rs`, called `check.rs:711`) — a free
//! function walking the whole [`Document`], called once in `check()` step 8.
//! All analysis is LOCAL to one `<match>`/`<branch>`/`<hub>` (§5.2 — no
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

use std::collections::BTreeMap;

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_syntax::ast::{Arm, CelSlot, Document, Match, Node};

use crate::cel_expand::DefTable;
use crate::check::FoldedEnv;
use crate::decide::{decide_slot, DecideCtx, Decided, DollarBinding};
use crate::match_check::{
    classify_when_literal, infer_domain, is_pattern_literals, literal_is_foreign, subject_path,
    Domain, DomainInfo, DomainValue, WhenLiteral,
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
    let empty_params: BTreeMap<String, DomainInfo> = BTreeMap::new();
    let base_ctx = DecideCtx {
        schema: &folded.env.state,
        dollar: None,
        params: &empty_params,
    };
    let mut diags = Vec::new();
    for shot in &doc.shots {
        walk_reach(&shot.body, &defs, &base_ctx, &mut diags);
    }
    for quest in &doc.quests {
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
                let dom = infer_domain(subject_path(m).as_deref(), ctx.schema);
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
            Node::Objective(o) => walk_reach(&o.body, defs, ctx, diags),
            Node::Line(_)
            | Node::Directive(_)
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

                // Cause 1: decided-false guard (dsl 0.4.0 §5.2 rule 1). A
                // guard present AND is-pattern present: the decided-false
                // guard alone kills the arm — same code, this cause named
                // (cause 2 is skipped once this fires).
                if !test.raw.trim().is_empty() {
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
        if let Some(Decided::Bool(false)) = decide_slot(&slot.raw, defs, ctx) {
            diags.push(diag(
                E_ARM_DEAD,
                Severity::Error,
                dead_guard_message("choice", &slot.raw),
                span,
            ));
        }
    }
    diags
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
    }
}
