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
//! cause — mirrors D4 above via the SAME causality substitution
//! [`crate::decide::analyze_literal_comparisons`] performs (re-deciding a
//! copy of the guard with every detected comparison replaced by an
//! undecided placeholder): `E-ARM-DEAD` survives when the guard is ALSO
//! independently dead for another reason. `E-MAYBE-UNSET` is NOT a
//! derivative — it stays independent (§4).
//!
//! ## dsl 0.26.0 (compared literals)
//! The same guard-slot lint catches any other string literal compared
//! (`==`/`!=`, either order, or an `in [...]` element) with a subject whose
//! finite domain is a set of strings but has no such member — an enum path,
//! `occasion.target`, a quest's `state`/`failedBy`, `scene.choices.*`, an
//! enum param, `$` — as `E-WHEN-LITERAL-DOMAIN`, the code a foreign `<when
//! is>` literal gets, with a did-you-mean. It owns the dead guard exactly as
//! `E-UNSET-LITERAL` does.

use std::collections::BTreeMap;

use lute_manifest::snapshot::CapabilitySnapshot;

use lute_core_span::{Diagnostic, Severity, Span};
use lute_syntax::ast::{CelSlot, Document, Node};

use crate::cel_expand::DefTable;
use crate::check::FoldedEnv;
use crate::decide::{analyze_literal_comparisons, DecideCtx};
use crate::match_check::{param_domain, DomainInfo};
 
mod diagnostics;

 
pub(crate) use diagnostics::{
    diag, push_literal_cmp_diags, E_ARM_DEAD, E_OBJECTIVE_CONTRADICTION, E_OBJECTIVE_UNSATISFIABLE,
    E_QUEST_UNREACHABLE, W_DEADLINE_BEFORE_DONE, W_DEADLINE_NEVER, W_OBJECTIVE_HIDDEN,
    W_OTHERWISE_DEAD,
};
pub use diagnostics::E_ENTRY_UNREACHABLE;

mod dnf;
mod context;
mod picks;
mod walk;
mod objective;

use objective::{
    check_handler_after_completion, check_objective_contradiction, check_quest_reach,
};
pub(crate) use objective::REQUIRED_QUEST_NOTE;
use objective::{comparison_set, comparison_set_polar};
use walk::walk_reach;
pub(crate) use walk::{arm_has_foreign_literal, check_choices_reach};

pub(crate) use picks::{member_arm_verdicts, Pick};

pub(crate) use context::{dead_members, for_every_member, Assumption, ReachEnv};
use context::{never_holds, Reach};

pub(crate) use dnf::{
    implies, non_exclusive_witness, provably_exclusive, when_conjuncts, when_dnf, Disjunct, Dnf,
};


/// §5.2/§5.3 whole-document pass. Walks `doc.shots` + `doc.quests` +
/// `doc.entries` (dsl 0.19.0 §4)
/// recursively (arm/choice/on/objective bodies, mirroring
/// `check_admission`'s walk, admission.rs:220-296); timeline clips carry no
/// arms and are skipped. `DefTable` is built from `folded.def_bodies` +
/// `folded.env.def_params` (D2) so a `test="@never"` guard hidden behind a
/// frontmatter `defs:` entry is caught exactly like an inline literal guard.
/// `snapshot` tells which directives write state (a body running one makes
/// no scalar [`Assumption`], dsl 0.24.0).
pub(crate) fn check_reachability(
    doc: &Document,
    folded: &FoldedEnv,
    snapshot: &CapabilitySnapshot,
) -> Vec<Diagnostic> {
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
            .map(|p| (p.name.clone(), param_domain(&p.ty, &folded.domains)))
            .collect()
    } else {
        BTreeMap::new()
    };
    let base_ctx = DecideCtx {
        schema: &folded.env.state,
        dollar: None,
        params: &param_domains,
        facts: None,
    };
    let env = ReachEnv {
        def_types: &folded.env.def_types,
        beat_when: folded.typed.beat.as_ref().and_then(|b| b.when.as_ref()),
        snapshot: Some(snapshot),
        folded: Some(folded),
    };
    // dsl 0.28.0: a kind or `for=` beat's slots are judged over its own
    // environment — `occasion.target` typed by its own members.
    let beat_ctx = |span: Span| DecideCtx {
        schema: &folded.env_at(span).state,
        dollar: None,
        params: &param_domains,
        facts: None,
    };
    let members_at = |span: Span| {
        folded
            .env
            .occasion_scopes
            .members_at(span.byte_start, span.byte_end)
    };
    let mut diags = check_reachability_in(doc, &defs, &base_ctx, &env);
    // dsl 0.21.0 §5: a scene beat's `when` is a listed guard slot and a
    // `when` that decides false never lets the beat be chosen. dsl 0.28.0
    // §1: when a literal or type fault is why it decides false, that fault
    // is the one report. An entry beat's `when` is the entry's own
    // eligibility guard and keeps `E-ENTRY-UNREACHABLE` above.
    if let Some(when) = folded
        .typed
        .beat
        .as_ref()
        .and_then(|b| b.when.as_ref())
        .filter(|w| !w.raw.trim().is_empty())
    {
        let analysis = analyze_literal_comparisons(&when.raw, &defs, &base_ctx);
        push_literal_cmp_diags(&mut diags, &analysis.hits, Some(&when.raw), when.span);
        if !analysis.owns_dead_guard() {
            if let Some(why) = never_holds(&when.raw, members_at(when.span), &defs, &base_ctx) {
                diags.push(diag(
                    crate::beats::E_BEAT_UNREACHABLE,
                    Severity::Error,
                    crate::beats::beat_unreachable_message(
                        &crate::beats::scene_beat_name(folded),
                        when.raw.trim(),
                        why.as_deref(),
                    ),
                    when.span,
                ));
            }
        }
    }
    // dsl 0.23.0 §4: a bundle beat's `when` gets the scene beat's treatment.
    let doc_id = folded.typed.id.as_deref().unwrap_or("this document");
    for beat in &doc.beats {
        let Some(when) = beat.when.as_ref().filter(|w| !w.raw.trim().is_empty()) else {
            continue;
        };
        let ctx = beat_ctx(beat.span);
        let analysis = analyze_literal_comparisons(&when.raw, &defs, &ctx);
        push_literal_cmp_diags(&mut diags, &analysis.hits, Some(&when.raw), when.span);
        if analysis.owns_dead_guard() {
            continue;
        }
        if let Some(why) = never_holds(&when.raw, members_at(beat.span), &defs, &ctx) {
            diags.push(diag(
                crate::beats::E_BEAT_UNREACHABLE,
                Severity::Error,
                crate::beats::beat_unreachable_message(
                    &crate::bundles::bundle_beat_key(doc_id, &beat.id),
                    when.raw.trim(),
                    why.as_deref(),
                ),
                when.span,
            ));
        }
    }
    // dsl 0.28.0 §1 (T1-5): a `spentBy` is a condition slot like `when` —
    // the same literal-domain checks.
    let scene_on = crate::beats::top_value_span(&doc.meta, "on");
    let spent_bys = folded
        .typed
        .beat
        .iter()
        .filter_map(|b| Some((b.spent_by.as_ref()?, scene_on)))
        .chain(
            doc.beats
                .iter()
                .filter_map(|b| Some((b.spent_by.as_ref()?, b.span))),
        )
        .chain(
            doc.entries
                .iter()
                .filter_map(|e| Some((e.spent_by.as_ref()?, e.span))),
        );
    for (slot, at) in spent_bys.filter(|(s, _)| !s.raw.trim().is_empty()) {
        let analysis = analyze_literal_comparisons(&slot.raw, &defs, &beat_ctx(at));
        push_literal_cmp_diags(&mut diags, &analysis.hits, Some(&slot.raw), slot.span);
    }
    // dsl 0.27.0 §4: beats judged under their occasion's gate and `!terminal`
    // — one already never eligible on its own `when` keeps that report.
    for d in crate::gates::seam_reachability(doc, folded, &defs, &base_ctx) {
        if !diags.iter().any(|x| x.code == d.code && x.span == d.span) {
            diags.push(d);
        }
    }
    // dsl 0.27.0 §5: a `spentBy` that always holds, or already holds at start.
    diags.extend(crate::spent_by::check_spent_by(
        doc, folded, &defs, &base_ctx,
    ));
    // A season-spent beat or season-tier quest not gated on the season.
    diags.extend(crate::season::check_ungated(doc, folded, &defs, &base_ctx));
    diags
}

/// The §5.2/§5.3 walk itself, over a caller-supplied resolution environment —
/// the seam [`check_reachability`] resolves a whole [`FoldedEnv`] into, and the
/// ONE entry point `validate_components` reaches for a component body (Task
/// 7e), which has no `FoldedEnv` of its own.
///
/// Splitting the pass here rather than manufacturing a second `FoldedEnv` is
/// the point: BOTH callers hand over exactly a [`DefTable`] and a base
/// [`DecideCtx`], so the imported-component path and the STANDALONE
/// component-file self-check above agree by CONSTRUCTION — the component branch
/// of `param_domains` above and `validate_components`'s own per-component table
/// are the same `param_domain(ty)` map over the same `params:` list, and a
/// component's `DefTable.bodies` is empty on both paths (a component file has
/// no frontmatter `defs:`; a bodiless `@ref` marker resolves via `params`, D3).
///
/// `base_ctx.dollar` MUST be `None`: every `$` binding this walk needs is a
/// FRESH one it derives per `<match>` subject (see [`walk_reach`]).
pub(crate) fn check_reachability_in(
    doc: &Document,
    defs: &DefTable<'_>,
    base_ctx: &DecideCtx<'_>,
    env: &ReachEnv<'_>,
) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    let targets = crate::next_labels::next_targets(doc);
    // One body's walk under the `when` it runs behind (dsl 0.24.0), over
    // `ctx` — its beat's own environment. `once`: a scene body, played at
    // most once per scene play (a component body may be used many times).
    let walk_body = |bodies: &[&[Node]],
                     when: Option<&CelSlot>,
                     ctx: &DecideCtx<'_>,
                     once: bool,
                     diags: &mut Vec<Diagnostic>| {
        let assumption = when.zip(env.snapshot).and_then(|(when, snapshot)| {
            Assumption::new(when, bodies, defs, env.def_types, ctx.schema, snapshot)
        });
        let rx = Reach {
            def_types: env.def_types,
            assume: assumption.as_ref(),
            targets: &targets,
            picks: &[],
            once,
        };
        for body in bodies {
            walk_reach(body, defs, &rx, ctx, diags);
        }
    };
    // A scene's shots are one body: `scene.*`/`run.*` persist across shots.
    let shots: Vec<&[Node]> = doc.shots.iter().map(|s| s.body.as_slice()).collect();
    let scene_body = env.folded.is_some();
    walk_body(&shots, env.beat_when, base_ctx, scene_body, &mut diags);
    for quest in &doc.quests {
        diags.extend(check_quest_reach(quest, defs, base_ctx));
        diags.extend(check_objective_contradiction(quest, defs, base_ctx));
        diags.extend(check_handler_after_completion(quest, defs));
        walk_body(&[&quest.body], None, base_ctx, false, &mut diags);
    }
    // dsl 0.19.0 §4: an entry body is an ordinary node stream — its
    // `<match>` arms get the same dead-arm / dead-otherwise verdicts.
    // dsl 0.20.0 §5: a `when` that decides false never lets the entry show.
    for entry in &doc.entries {
        let ctx = env.ctx_at(base_ctx, entry.span);
        if let Some(when) = entry.when.as_ref().filter(|w| !w.raw.trim().is_empty()) {
            // dsl 0.28.0 §1 (T1-5): the literal checks every `when` gets; a
            // literal or type fault that makes the guard false is the report.
            let analysis = analyze_literal_comparisons(&when.raw, defs, &ctx);
            push_literal_cmp_diags(&mut diags, &analysis.hits, Some(&when.raw), when.span);
            let never = (!analysis.owns_dead_guard())
                .then(|| never_holds(&when.raw, env.members_at(entry.span), defs, &ctx))
                .flatten();
            if let Some(why) = never {
                diags.push(diag(
                    E_ENTRY_UNREACHABLE,
                    Severity::Error,
                    match why {
                        Some(why) => format!(
                            "entry `{}` is never eligible: its `when` guard `{}` is provably \
                             false — {why} (dsl 0.20.0 §5, 0.27.0 §4)",
                            entry.id,
                            when.raw.trim()
                        ),
                        None => format!(
                            "entry `{}` is never eligible: its `when` guard `{}` is provably \
                             false (dsl 0.20.0 §5)",
                            entry.id,
                            when.raw.trim()
                        ),
                    },
                    when.span,
                ));
            }
        }
        walk_body(&[&entry.body], entry.when.as_ref(), &ctx, false, &mut diags);
    }
    // dsl 0.23.0 §4: a bundle beat body is a scene body.
    for beat in &doc.beats {
        let ctx = env.ctx_at(base_ctx, beat.span);
        walk_body(
            &[&beat.body],
            beat.when.as_ref(),
            &ctx,
            scene_body,
            &mut diags,
        );
    }
    diags
}


