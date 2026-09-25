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
//! [`crate::decide::analyze_unset_sentinel_slot`] performs (re-deciding a
//! copy of the guard with every detected comparison replaced by an
//! undecided placeholder): `E-ARM-DEAD` survives when the guard is ALSO
//! independently dead for another reason. `E-MAYBE-UNSET` is NOT a
//! derivative — it stays independent (§4).

use std::collections::{BTreeMap, BTreeSet};

use cel_parser::ast::{operators as op, Expr};
use cel_parser::reference::Val;
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_manifest::types::Type;

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_syntax::ast::{
    Arm, AttrValue, CelSlot, Choice, ClipNode, Document, Match, Node, Objective, Quest,
};

use crate::cel_expand::DefTable;
use crate::check::FoldedEnv;
use crate::decide::{
    analyze_unset_sentinel_slot, decide_slot, DecideCtx, Decided, DollarBinding, UnsetSentinelHit,
};
use crate::match_check::{
    is_pattern_literals, literal_is_foreign, param_domain, quest_state_is_literal,
    subject_path, Domain, DomainInfo, DomainValue, Interval, NumCoverage,
};
use crate::solution::{disjoint, solution_set, SolutionSet};
use lute_syntax::is_pattern::{classify_is_literal, IsLiteral};

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

/// `E-OBJECTIVE-CONTRADICTION` (dsl 0.10.0 §5.2, D-G): two REQUIRED objectives
/// of one `<quest>` whose in-domain `done` predicates name the same declared
/// scalar path and whose solution sets over that path's declared type do not
/// intersect. Neither objective is individually dead — so neither draws
/// [`E_OBJECTIVE_UNSATISFIABLE`] — and the quest consequence rides as a note on
/// this diagnostic (C4, D-O), never as a second [`E_QUEST_UNREACHABLE`].
pub(crate) const E_OBJECTIVE_CONTRADICTION: &str = "E-OBJECTIVE-CONTRADICTION";

/// `W-OBJECTIVE-HIDDEN` (dsl 0.4.0 §5.3): a REQUIRED (`!optional`)
/// objective whose `when` visibility gate decides false — provably never
/// visible or tracked, yet still gates completion (the `0.2 §6.3` softlock
/// prose made checkable). A warning: `done` is evaluated independently of
/// visibility, so completion may still be reachable.
pub(crate) const W_OBJECTIVE_HIDDEN: &str = "W-OBJECTIVE-HIDDEN";

/// `E-ENTRY-UNREACHABLE` (dsl 0.20.0 §5): a lore entry whose `when`
/// eligibility guard provably never holds — the entry is never presented.
/// Decided per file when the guard is scalar-decidable (here), and by the
/// project pass (`crate::fact_check`) when only the fact envelope decides it.
pub const E_ENTRY_UNREACHABLE: &str = "E-ENTRY-UNREACHABLE";

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

/// Push one `E-UNSET-LITERAL` per hit (§2.1: every distinct comparison in
/// the slot, not just the first) at `span` — the enclosing guard slot's
/// location (CEL `Expr` nodes carry no per-node span of their own, only the
/// slot's, `cel_paths.rs`'s carry-forward note).
fn push_unset_literal_diags(diags: &mut Vec<Diagnostic>, hits: &[UnsetSentinelHit], span: Span) {
    for hit in hits {
        diags.push(diag(
            E_UNSET_LITERAL,
            Severity::Error,
            unset_literal_message(&hit.subject, hit.not_equals),
            span,
        ));
    }
}

/// `W-CODE-AFTER-END` (dsl 0.8.0): a record following `::end` in the SAME
/// straight-line body. `::end` terminates the walk at its own record
/// (`lute.core`'s `terminatesWalk`), so nothing after it in that body can
/// ever run.
///
/// Scoped to the IMMEDIATELY ENCLOSING sequence — a shot body, one
/// `<choice>` body, one `<when>`/`<otherwise>` arm, one `<on>`/
/// `<objective>` body. An `::end` inside one `<choice>` says nothing about
/// a sibling choice or about content after the enclosing `<branch>`: those
/// are DIFFERENT bodies, reached by a different route, and the walk that
/// terminated never entered them. Cross-body reachability is
/// `E-CONN-UNREACHABLE`'s job (a whole-project graph), not this local lint.
///
/// A WARNING, never an error: unreachable content is inert, not
/// ill-formed — the same call `W-OTHERWISE-DEAD` makes for a defensive
/// `<otherwise>`.
pub(crate) const W_CODE_AFTER_END: &str = "W-CODE-AFTER-END";

/// One `W-CODE-AFTER-END` for `nodes` (a single straight-line body) when a
/// `::end` is followed by anything, anchored at the FIRST such node — the
/// place an author would cut from. Exactly one per body: everything past
/// the first unreachable node is unreachable for the SAME reason, and N
/// warnings for one mistake is noise.
///
/// Dispatch is by TAG ([`lute_manifest::core::END_DIRECTIVE`]), the same
/// key `lower_directive` lowers on — see the `terminatesWalk` note in
/// `lute_manifest::validate::SEMANTICS_VOCAB` for why the flag declares
/// the semantics but never drives the dispatch.
fn check_code_after_end(nodes: &[Node], diags: &mut Vec<Diagnostic>) {
    let is_end =
        |n: &Node| matches!(n, Node::Directive(d) if d.tag == lute_manifest::core::END_DIRECTIVE);
    let Some(end_at) = nodes.iter().position(is_end) else {
        return;
    };
    let Some(dead) = nodes.get(end_at + 1) else {
        return;
    };
    diags.push(diag(
        W_CODE_AFTER_END,
        Severity::Warning,
        "unreachable content after `::end` (the walk terminates here)".to_string(),
        crate::admission::node_span(dead),
    ));
}

/// `W-CODE-AFTER-NEXT` (dsl 0.12.0): a record following an UNGUARDED
/// `::next` in the SAME straight-line body — mirrors [`W_CODE_AFTER_END`]
/// exactly: an unconditional forward jump leaves this body the same way
/// `::end` does, so nothing after it in that body can ever run. A GUARDED
/// `::next{when=}` does NOT qualify (fall-through exists; see the
/// `Node::Directive` arm in [`walk_reach`]).
pub(crate) const W_CODE_AFTER_NEXT: &str = "W-CODE-AFTER-NEXT";

/// One `W-CODE-AFTER-NEXT` for `nodes`, mirroring [`check_code_after_end`]
/// verbatim except the terminator predicate (unguarded `::next` — dispatch
/// by TAG, [`lute_manifest::core::NEXT_DIRECTIVE`], AND `d.when.is_none()`).
fn check_code_after_next(nodes: &[Node], diags: &mut Vec<Diagnostic>) {
    let is_unguarded_next = |n: &Node| matches!(n, Node::Directive(d) if d.tag == lute_manifest::core::NEXT_DIRECTIVE && d.when.is_none());
    let Some(next_at) = nodes.iter().position(is_unguarded_next) else {
        return;
    };
    let Some(dead) = nodes.get(next_at + 1) else {
        return;
    };
    diags.push(diag(
        W_CODE_AFTER_NEXT,
        Severity::Warning,
        "unreachable content after `::next` (the walk jumps away here)".to_string(),
        crate::admission::node_span(dead),
    ));
}

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
            .map(|p| (p.name.clone(), param_domain(&p.ty)))
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
    };
    let mut diags = check_reachability_in(doc, &defs, &base_ctx, &env);
    // dsl 0.21.0 §5: a scene beat's `when` is a listed guard slot (the
    // `<quest start>` treatment: `E-UNSET-LITERAL` independently, no
    // suppression) and a `when` that decides false never lets the beat be
    // chosen. An entry beat's `when` is the entry's own eligibility guard and
    // keeps `E-ENTRY-UNREACHABLE` above.
    if let Some(when) = folded
        .typed
        .beat
        .as_ref()
        .and_then(|b| b.when.as_ref())
        .filter(|w| !w.raw.trim().is_empty())
    {
        let analysis = analyze_unset_sentinel_slot(&when.raw, &defs, &base_ctx);
        push_unset_literal_diags(&mut diags, &analysis.hits, when.span);
        if let Some(Decided::Bool(false)) = decide_slot(&when.raw, &defs, &base_ctx) {
            diags.push(diag(
                crate::beats::E_BEAT_UNREACHABLE,
                Severity::Error,
                crate::beats::beat_unreachable_message(
                    &crate::beats::scene_beat_name(folded),
                    when.raw.trim(),
                    None,
                ),
                when.span,
            ));
        }
    }
    // dsl 0.23.0 §4: a bundle beat's `when` gets the scene beat's treatment.
    let doc_id = folded.typed.id.as_deref().unwrap_or("this document");
    for beat in &doc.beats {
        let Some(when) = beat.when.as_ref().filter(|w| !w.raw.trim().is_empty()) else {
            continue;
        };
        let analysis = analyze_unset_sentinel_slot(&when.raw, &defs, &base_ctx);
        push_unset_literal_diags(&mut diags, &analysis.hits, when.span);
        if let Some(Decided::Bool(false)) = decide_slot(&when.raw, &defs, &base_ctx) {
            diags.push(diag(
                crate::beats::E_BEAT_UNREACHABLE,
                Severity::Error,
                crate::beats::beat_unreachable_message(
                    &crate::bundles::bundle_beat_key(doc_id, &beat.id),
                    when.raw.trim(),
                    None,
                ),
                when.span,
            ));
        }
    }
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
    // One body's walk under the `when` it runs behind (dsl 0.24.0).
    let walk_body =
        |bodies: &[&[Node]], when: Option<&CelSlot>, diags: &mut Vec<Diagnostic>| {
            let assumption = when.zip(env.snapshot).and_then(|(when, snapshot)| {
                Assumption::new(when, bodies, defs, env.def_types, base_ctx.schema, snapshot)
            });
            let rx = Reach {
                def_types: env.def_types,
                assume: assumption.as_ref(),
            };
            for body in bodies {
                walk_reach(body, defs, &rx, base_ctx, diags);
            }
        };
    // A scene's shots are one body: `scene.*`/`run.*` persist across shots.
    let shots: Vec<&[Node]> = doc.shots.iter().map(|s| s.body.as_slice()).collect();
    walk_body(&shots, env.beat_when, &mut diags);
    for quest in &doc.quests {
        diags.extend(check_quest_reach(quest, defs, base_ctx));
        diags.extend(check_objective_contradiction(quest, defs, base_ctx));
        diags.extend(check_handler_after_completion(quest, defs));
        walk_body(&[&quest.body], None, &mut diags);
    }
    // dsl 0.19.0 §4: an entry body is an ordinary node stream — its
    // `<match>` arms get the same dead-arm / dead-otherwise verdicts.
    // dsl 0.20.0 §5: a `when` that decides false never lets the entry show.
    for entry in &doc.entries {
        if let Some(when) = entry.when.as_ref().filter(|w| !w.raw.trim().is_empty()) {
            if let Some(Decided::Bool(false)) = decide_slot(&when.raw, defs, base_ctx) {
                diags.push(diag(
                    E_ENTRY_UNREACHABLE,
                    Severity::Error,
                    format!(
                        "entry `{}` is never eligible: its `when` guard `{}` is provably false \
                         (dsl 0.20.0 §5)",
                        entry.id,
                        when.raw.trim()
                    ),
                    when.span,
                ));
            }
        }
        walk_body(&[&entry.body], entry.when.as_ref(), &mut diags);
    }
    // dsl 0.23.0 §4: a bundle beat body is a scene body.
    for beat in &doc.beats {
        walk_body(&[&beat.body], beat.when.as_ref(), &mut diags);
    }
    diags
}

/// What [`check_reachability_in`] needs beyond the decide context (dsl
/// 0.24.0): the def result types a `<match on="@def">` subject takes its
/// domain from, the scene beat's `when` (a frontmatter slot, outside the
/// document tree), and the snapshot a body's directives are looked up in —
/// `None` (a component body) makes no [`Assumption`].
pub(crate) struct ReachEnv<'a> {
    pub(crate) def_types: &'a BTreeMap<String, Type>,
    pub(crate) beat_when: Option<&'a CelSlot>,
    pub(crate) snapshot: Option<&'a CapabilitySnapshot>,
}

/// One body's walk context: [`ReachEnv::def_types`] and the body's own
/// [`Assumption`].
struct Reach<'a> {
    def_types: &'a BTreeMap<String, Type>,
    assume: Option<&'a Assumption>,
}

/// A beat's / entry's `when` as an assumption over the body it guards (dsl
/// 0.24.0): the body runs only once the guard held, so a `<match>` literal
/// the guard rules out can never be the subject's value there. Kept to what
/// the body cannot change: the guard's top-level conjuncts over `run.*` /
/// `user.*` / `prev.*` paths no `::set` / `<choice into>` in the body writes
/// — none at all once the body runs a `::use` or a state-writing directive —
/// plus the paths its presence guards prove (nothing unsets a path).
struct Assumption {
    raw: String,
    conjuncts: Vec<(String, SolutionSet)>,
    present: crate::defassign::Assigned,
}

impl Assumption {
    fn new(
        when: &CelSlot,
        bodies: &[&[Node]],
        defs: &DefTable<'_>,
        def_types: &BTreeMap<String, Type>,
        schema: &crate::meta::StateSchema,
        snapshot: &CapabilitySnapshot,
    ) -> Option<Self> {
        let raw = when.raw.trim();
        if raw.is_empty() {
            return None;
        }
        let mut written = BTreeSet::new();
        let mut opaque = false;
        for body in bodies {
            opaque |= scan_writes(body, snapshot, &mut written);
        }
        let conjuncts = if opaque {
            Vec::new()
        } else {
            let overlaps = |p: &str, w: &str| {
                p == w || p.starts_with(&format!("{w}.")) || w.starts_with(&format!("{p}."))
            };
            when_conjuncts(raw, defs, schema, None)
                .0
                .into_iter()
                .filter(|(p, _)| {
                    matches!(p.split('.').next(), Some("run" | "user" | "prev"))
                        && !written.iter().any(|w| overlaps(p, w))
                })
                .collect()
        };
        let scope = crate::defassign::Scope {
            schema,
            defs: DefTable {
                bodies: defs.bodies,
                params: defs.params,
            },
            def_types,
        };
        let present = crate::defassign::assumed_present(Some(when), &scope);
        (!conjuncts.is_empty() || !present.is_empty()).then(|| Self {
            raw: raw.to_string(),
            conjuncts,
            present,
        })
    }

    /// Whether the guard rules out `item` as the value of `path`.
    fn rules_out(&self, path: &str, item: &CoverItem, schema: &crate::meta::StateSchema) -> bool {
        let mut on_path = self
            .conjuncts
            .iter()
            .filter(|(p, _)| p == path)
            .map(|(_, set)| set);
        match item {
            // A comparison, an ordering, or a bare bool read is true only on
            // a value (`!=` is true on `unset`, dsl 0.23.0 §9).
            CoverItem::Unset => {
                crate::defassign::is_present(path, &self.present, schema)
                    || on_path.any(|set| !matches!(set, SolutionSet::Except(_)))
            }
            CoverItem::Value(v) => {
                let lit = SolutionSet::Values(std::iter::once(v.clone()).collect());
                on_path.any(|set| disjoint(set, &lit))
            }
            CoverItem::Num(iv) => {
                let lit = SolutionSet::Interval {
                    lo: iv.lo,
                    lo_inc: iv.lo.is_finite(),
                    hi: iv.hi,
                    hi_inc: iv.hi.is_finite(),
                };
                on_path.any(|set| disjoint(set, &lit))
            }
        }
    }
}

/// Every state path a `::set` / `<choice into>` in `nodes` writes (at any
/// depth), into `out`; `true` when the body also runs something whose writes
/// are not spelled out here — a `::use` or a directive that writes state.
fn scan_writes(
    nodes: &[Node],
    snapshot: &CapabilitySnapshot,
    out: &mut BTreeSet<String>,
) -> bool {
    let opaque_directive = |tag: &str| {
        tag == "use" || crate::check::directive_writes_state(snapshot, tag)
    };
    let mut opaque = false;
    for node in nodes {
        match node {
            Node::Set(s) => {
                out.insert(s.path.clone());
            }
            Node::Directive(d) => opaque |= opaque_directive(&d.tag),
            Node::Branch(b) => {
                for c in &b.choices {
                    opaque |= scan_choice_writes(c, snapshot, out);
                }
            }
            Node::Hub(h) => {
                for c in &h.choices {
                    opaque |= scan_choice_writes(c, snapshot, out);
                }
            }
            Node::Match(m) => {
                for arm in &m.arms {
                    let (Arm::When { body, .. } | Arm::Otherwise { body, .. }) = arm;
                    opaque |= scan_writes(body, snapshot, out);
                }
            }
            Node::On(o) => opaque |= scan_writes(&o.body, snapshot, out),
            Node::Objective(o) => opaque |= scan_writes(&o.body, snapshot, out),
            Node::Timeline(tl) => {
                for clip in tl.tracks.iter().flat_map(|t| &t.clips) {
                    match &clip.node {
                        ClipNode::Set(s) => {
                            out.insert(s.path.clone());
                        }
                        ClipNode::Directive(d) => opaque |= opaque_directive(&d.tag),
                    }
                }
            }
            Node::Line(_) | Node::Assert(_) | Node::Retract(_) => {}
        }
    }
    opaque
}

fn scan_choice_writes(
    choice: &Choice,
    snapshot: &CapabilitySnapshot,
    out: &mut BTreeSet<String>,
) -> bool {
    if let Some(AttrValue::Str(into)) = choice.attrs.iter().find(|a| a.key == "into").map(|a| &a.value) {
        out.insert(into.clone());
    }
    scan_writes(&choice.body, snapshot, out)
}

/// Recurse a node stream exactly like `check_admission`'s `walk`
/// (admission.rs:220-296): `<match>` arm bodies, `<branch>`/`<hub>` choice
/// bodies, `<on>`/`<objective>` bodies. A `<match>` gets a FRESH `$` binding
/// — its own subject's domain (dsl 0.4.0 §5.2 rule: "for match arms,
/// `ctx.dollar = Domain(&infer_domain(subject))`"); `<branch>`/`<hub>`
/// choices decide with the incoming `ctx` unchanged (`dollar: None` at the
/// top level — "for choices, `dollar = None`"). Timeline clips (`ClipNode`)
/// carry no arms — skipped, like every other leaf node.
///
/// `nodes` is by construction exactly ONE straight-line body at every call
/// site (a shot, an arm, a choice, an `<on>`/`<objective>` body), which is
/// precisely [`check_code_after_end`]'s unit of analysis — so the
/// `W-CODE-AFTER-END` scan rides this recursion instead of duplicating it.
fn walk_reach(
    nodes: &[Node],
    defs: &DefTable<'_>,
    rx: &Reach<'_>,
    ctx: &DecideCtx<'_>,
    diags: &mut Vec<Diagnostic>,
) {
    check_code_after_end(nodes, diags);
    check_code_after_next(nodes, diags);
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
                let subject_analysis = analyze_unset_sentinel_slot(&m.subject.raw, defs, ctx);
                push_unset_literal_diags(diags, &subject_analysis.hits, m.subject.span);
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
                ));
                for arm in &m.arms {
                    let body = match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => body,
                    };
                    walk_reach(body, defs, rx, ctx, diags);
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
                    walk_reach(&choice.body, defs, rx, ctx, diags);
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
                    walk_reach(&choice.body, defs, rx, ctx, diags);
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
                    let analysis = analyze_unset_sentinel_slot(&when.raw, defs, ctx);
                    push_unset_literal_diags(diags, &analysis.hits, when.span);
                }
                walk_reach(&o.body, defs, rx, ctx, diags);
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
                        let analysis = analyze_unset_sentinel_slot(&when.raw, defs, ctx);
                        push_unset_literal_diags(diags, &analysis.hits, when.span);
                        // §2.3: suppress `E-ARM-DEAD` only when the
                        // sentinel comparison(s) are LOAD-BEARING for the
                        // decided-false — an independently-dead guard (a
                        // literal `false`, an unrelated foreign-typo
                        // comparison, `@never`, …) must still flag it.
                        let suppress_arm_dead =
                            !analysis.hits.is_empty() && analysis.load_bearing_for_false;
                        if !suppress_arm_dead {
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
            // dsl 0.12.0: a guarded `::next{when=}` is a one-arm construct
            // exactly like a gated line above — a decided-false guard makes
            // the jump provably dead. An UNGUARDED `::next` needs no guard
            // analysis here (that is `check_code_after_next`'s job, above).
            Node::Directive(d) if d.when.is_some() => {
                let when = d.when.as_ref().expect("guarded above");
                if !when.raw.trim().is_empty() {
                    let analysis = analyze_unset_sentinel_slot(&when.raw, defs, ctx);
                    push_unset_literal_diags(diags, &analysis.hits, when.span);
                    let suppress_arm_dead =
                        !analysis.hits.is_empty() && analysis.load_bearing_for_false;
                    if !suppress_arm_dead {
                        if let Some(Decided::Bool(false)) = decide_slot(&when.raw, defs, ctx) {
                            diags.push(diag(
                                E_ARM_DEAD,
                                Severity::Error,
                                "this `::next` never fires: its `when` guard is provably false (dsl 0.12.0)".to_string(),
                                when.span,
                            ));
                        }
                    }
                }
            }
            // dsl 0.24.0 §1: a guarded `::set{… when=}` is the same one-arm
            // construct — a decided-false guard makes the write provably dead.
            Node::Set(s) if s.when.is_some() => {
                let when = s.when.as_ref().expect("guarded above");
                if !when.raw.trim().is_empty() {
                    let analysis = analyze_unset_sentinel_slot(&when.raw, defs, ctx);
                    push_unset_literal_diags(diags, &analysis.hits, when.span);
                    let suppress_arm_dead =
                        !analysis.hits.is_empty() && analysis.load_bearing_for_false;
                    if !suppress_arm_dead {
                        if let Some(Decided::Bool(false)) = decide_slot(&when.raw, defs, ctx) {
                            diags.push(diag(
                                E_ARM_DEAD,
                                Severity::Error,
                                "this `::set` never writes: its `when` guard is provably false (dsl 0.24.0 §1)".to_string(),
                                when.span,
                            ));
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
/// concrete finite-domain literal, a numeric point/range interval (dsl
/// 0.18.0 §4), or the `unset` case — kept distinct from [`DomainValue`] since
/// `unset` is a membership fact about `maybe_unset`, never a domain member
/// (mirrors `match_check::ArmCoverage`).
enum CoverItem {
    Value(DomainValue),
    Num(Interval),
    Unset,
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
fn check_match_reach(
    m: &Match,
    subject: Option<&str>,
    defs: &DefTable<'_>,
    ctx: &DecideCtx<'_>,
    assume: Option<&Assumption>,
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
    let ruled_out = |item: &CoverItem| {
        subject.is_some_and(|p| assume.is_some_and(|a| a.rules_out(p, item, ctx.schema)))
    };
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
                    Some(analyze_unset_sentinel_slot(&test.raw, defs, ctx))
                };
                if let Some(a) = &analysis {
                    push_unset_literal_diags(&mut diags, &a.hits, *span);
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
                // LOAD-BEARING unset-sentinel guard is likewise already
                // rooted by `E-UNSET-LITERAL` above — an independently-dead
                // guard (a literal `false`, an unrelated foreign typo,
                // `@never`, …) still flags E-ARM-DEAD even when a sentinel
                // comparison is ALSO present.
                let foreign_literal = is
                    .as_ref()
                    .is_some_and(|pat| arm_has_foreign_literal(pat, &dom, subject));
                let sentinel_load_bearing = analysis
                    .as_ref()
                    .is_some_and(|a| !a.hits.is_empty() && a.load_bearing_for_false);
                if !foreign_literal && !sentinel_load_bearing && !test.raw.trim().is_empty() {
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
                            for item in &residual {
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
                                    Some(msg)
                                }
                                (true, None, Some(a)) => {
                                    Some(assumed_dead_message(pat.raw.trim(), &a.raw))
                                }
                                (true, None, None) => None,
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
    let domain_covered = dom.resolved
        && match &dom.domain {
            Domain::Finite(vals) => vals
                .iter()
                .all(|v| u.values.contains_key(v) || ruled_out(&CoverItem::Value(v.clone()))),
            Domain::Number => u.num.covers_all(),
            Domain::Infinite => false,
        };
    if let (Some(span), true) = (otherwise_span, domain_covered) {
        if u.unset.is_some() || !dom.maybe_unset || ruled_out(&CoverItem::Unset) {
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
        let analysis = analyze_unset_sentinel_slot(&slot.raw, defs, ctx);
        push_unset_literal_diags(&mut diags, &analysis.hits, span);
        // §2.3: suppress `E-ARM-DEAD` only when the sentinel comparison(s)
        // are LOAD-BEARING for the decided-false (mirrors the arm-level
        // causality check above).
        let suppress_arm_dead = !analysis.hits.is_empty() && analysis.load_bearing_for_false;
        if !suppress_arm_dead {
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
        let analysis = analyze_unset_sentinel_slot(&slot.raw, defs, ctx);
        push_unset_literal_diags(&mut diags, &analysis.hits, slot.span);
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

/// dsl 0.24.0 (round-3 T3-11): `W-QUEST-HANDLER-DEAD` for an `<on event="E"
/// when="G">` whose `G` implies the quest's completion — every REQUIRED
/// objective's `done` (any one of them under `complete="any"`). Quests
/// settle after every write, and a world event reaches a quest's handlers
/// only while it is active, so whenever `G` holds the quest has already
/// completed and the body never runs. The lifecycle events (`questComplete`,
/// `questFailed`) are dispatched by the transition itself and are exempt.
/// Decided only where it is certain: the quest has a required objective and
/// every one counted is settle-judged (no `on=` occasion objective, no
/// `quest=` child), and implication is conjunct containment after `@def`
/// expansion — each top-level `&&` conjunct of `done` is literally a
/// conjunct of `G`.
fn check_handler_after_completion(quest: &Quest, defs: &DefTable<'_>) -> Vec<Diagnostic> {
    let required: Vec<&Objective> = quest
        .body
        .iter()
        .filter_map(|n| match n {
            Node::Objective(o) if !o.optional => Some(o),
            _ => None,
        })
        .collect();
    if required.is_empty() || required.iter().any(|o| o.on.is_some() || o.quest.is_some()) {
        return Vec::new();
    }
    let any = quest.completes_on_any();
    let mut diags = Vec::new();
    for node in &quest.body {
        let Node::On(on) = node else { continue };
        if matches!(on.event.as_str(), "questComplete" | "questFailed") {
            continue;
        }
        let Some(when) = on.when.as_ref().filter(|w| !w.raw.trim().is_empty()) else {
            continue;
        };
        let guard = text_conjuncts(&expand_text(&when.raw, defs));
        let implied = |o: &&Objective| {
            let done = text_conjuncts(&expand_text(&o.done.raw, defs));
            !done.is_empty() && done.iter().all(|c| guard.contains(c))
        };
        let completes = if any { required.iter().any(implied) } else { required.iter().all(implied) };
        if !completes {
            continue;
        }
        diags.push(diag(
            crate::project_check::W_QUEST_HANDLER_DEAD,
            Severity::Warning,
            format!(
                "`<on event=\"{}\">` never runs: its `when` implies {} of quest `{}` is done, so \
                 the quest has already completed when it holds, and an event reaches only an \
                 active quest's handlers; move the body to `<on event=\"questComplete\">` or \
                 the objective's own body (dsl 0.24.0)",
                on.event,
                if any { "a required objective" } else { "every required objective" },
                quest.id
            ),
            on.event_span,
        ));
    }
    diags
}

/// `raw` with its `@def`s expanded (the raw text when expansion fails).
fn expand_text(raw: &str, defs: &DefTable<'_>) -> String {
    let mut stack = Vec::new();
    crate::cel_expand::expand_cel(raw, defs, None, &mut stack).unwrap_or_else(|_| raw.to_string())
}

/// The top-level `&&` conjuncts of CEL text, each with whitespace outside
/// string literals removed and redundant outer parentheses stripped, a
/// parenthesized conjunction flattened. A text with a top-level `||` or
/// `?:` is one conjunct (`&&` binds tighter, so splitting it would be
/// wrong).
fn text_conjuncts(raw: &str) -> Vec<String> {
    fn normalize(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let mut quote: Option<char> = None;
        let mut esc = false;
        for c in s.chars() {
            match quote {
                Some(q) => {
                    out.push(c);
                    if esc {
                        esc = false;
                    } else if c == '\\' {
                        esc = true;
                    } else if c == q {
                        quote = None;
                    }
                }
                None if c.is_whitespace() => {}
                None => {
                    if c == '\'' || c == '"' {
                        quote = Some(c);
                    }
                    out.push(c);
                }
            }
        }
        out
    }
    /// Byte offsets of the depth-0 `&&`s, and whether a depth-0 `||`/`?`
    /// occurs, in normalized text; `None` when brackets are unbalanced.
    fn top_level(s: &str) -> Option<(Vec<usize>, bool)> {
        let b = s.as_bytes();
        let (mut depth, mut quote, mut esc) = (0i32, None::<u8>, false);
        let (mut ands, mut other) = (Vec::new(), false);
        let mut i = 0;
        while i < b.len() {
            let c = b[i];
            if let Some(q) = quote {
                if esc {
                    esc = false;
                } else if c == b'\\' {
                    esc = true;
                } else if c == q {
                    quote = None;
                }
                i += 1;
                continue;
            }
            match c {
                b'\'' | b'"' => quote = Some(c),
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => {
                    depth -= 1;
                    if depth < 0 {
                        return None;
                    }
                }
                b'&' if depth == 0 && b.get(i + 1) == Some(&b'&') => {
                    ands.push(i);
                    i += 1;
                }
                b'|' | b'?' if depth == 0 => other = true,
                _ => {}
            }
            i += 1;
        }
        (depth == 0 && quote.is_none()).then_some((ands, other))
    }
    /// `s` without one pair of parentheses enclosing all of it.
    fn unwrap(s: &str) -> Option<&str> {
        let inner = s.strip_prefix('(')?.strip_suffix(')')?;
        top_level(inner).map(|_| inner)
    }
    fn split(s: &str, out: &mut Vec<String>) {
        let mut s = s;
        while let Some(inner) = unwrap(s) {
            s = inner;
        }
        match top_level(s) {
            Some((ands, false)) if !ands.is_empty() => {
                let mut start = 0;
                for at in ands {
                    split(&s[start..at], out);
                    start = at + 2;
                }
                split(&s[start..], out);
            }
            _ if !s.is_empty() => out.push(s.to_string()),
            _ => {}
        }
    }
    let mut out = Vec::new();
    split(&normalize(raw), &mut out);
    out
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
fn check_objective_reach(
    o: &Objective,
    defs: &DefTable<'_>,
    ctx: &DecideCtx<'_>,
) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    // dsl 0.5.2 §2.1: `<objective when/done>` are listed guard slots too —
    // independent of `E-OBJECTIVE-UNSATISFIABLE`/`W-OBJECTIVE-HIDDEN`, which
    // §2.3's ownership clause does NOT scope (it names only
    // `E-ARM-DEAD`/`W-OTHERWISE-DEAD`), so no suppression accompanies this.
    if let Some(when) = &o.when {
        let analysis = analyze_unset_sentinel_slot(&when.raw, defs, ctx);
        push_unset_literal_diags(&mut diags, &analysis.hits, when.span);
    }
    let done_analysis = analyze_unset_sentinel_slot(&o.done.raw, defs, ctx);
    push_unset_literal_diags(&mut diags, &done_analysis.hits, o.done.span);
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

/// One objective that participates in pairing.
struct Gate<'a> {
    id: &'a str,
    path: String,
    raw: &'a str,
    set: SolutionSet,
    span: Span,
}

/// dsl 0.10.0 §5.2 (D-G): every pair of REQUIRED, in-domain, same-path `done`
/// predicates of one quest whose solution sets do not intersect.
///
/// **Direct children of `quest.body` only**, mirroring `E-OBJECTIVE-ID-DUP`'s
/// own scoping (`match_check.rs:650-651`). An objective nested in a `<match>`
/// arm or a `<branch>` choice cannot be shown to coexist with one in a sibling
/// arm, so pairing across them would manufacture a contradiction between
/// objectives that never meet — the one outcome §5.2 may not produce.
fn check_objective_contradiction(
    quest: &Quest,
    defs: &DefTable<'_>,
    ctx: &DecideCtx<'_>,
) -> Vec<Diagnostic> {
    let mut gates: Vec<Gate<'_>> = Vec::new();
    for node in &quest.body {
        let Node::Objective(o) = node else { continue };
        // An OPTIONAL objective never participates: the quest can still
        // complete without it, so a pair including one is not a contradiction
        // ABOUT THE QUEST. An id-less objective has its own diagnostic.
        if o.optional || o.id.is_empty() {
            continue;
        }
        // An individually dead `done` is `E-OBJECTIVE-UNSATISFIABLE`'s, and the
        // two codes MUST NOT both fire for one pair. A comparison over a
        // declared path is never `decide`-false today (a `number` path is
        // `Domain::Number`, which R2 leaves undecided like `Infinite`), but
        // stating the exclusion structurally is cheaper than relying on that
        // staying true.
        if matches!(
            decide_slot(&o.done.raw, defs, ctx),
            Some(Decided::Bool(false))
        ) {
            continue;
        }
        if let Some(g) = in_domain_gate(o, ctx) {
            gates.push(g);
        }
    }
    let mut diags = Vec::new();
    for j in 1..gates.len() {
        for i in 0..j {
            if gates[i].path != gates[j].path || !disjoint(&gates[i].set, &gates[j].set) {
                continue;
            }
            // Anchored at the SECOND objective, reported once per pair.
            diags.push(diag(
                E_OBJECTIVE_CONTRADICTION,
                Severity::Error,
                contradiction_message(&gates[i], &gates[j]),
                gates[j].span,
            ));
        }
    }
    diags
}

/// §5.2's in-domain test: the predicate is, IN ITS ENTIRETY, a single
/// comparison `<declared scalar state path> <op> <literal>` with the literal
/// well-typed for the path's declared type. An `&&`, an `||`, a `!`, a fact
/// query, a `@ref`, a second path, or anything else puts it out of domain.
///
/// Parsed through the SAME expand-then-marked-reparse pipeline `decide_slot`
/// and the project guard pass (`fact_check.rs`) use, so this analysis and
/// reachability can never see different trees for the same raw text.
fn in_domain_gate<'a>(o: &'a Objective, ctx: &DecideCtx<'_>) -> Option<Gate<'a>> {
    let raw = o.done.raw.trim();
    // A `@ref` anywhere puts it out of domain, and `parse_slot_marked_refs`
    // would hide that by turning the ref into a marker ident.
    if !lute_cel::scan_refs(raw).is_empty() {
        return None;
    }
    let mut arena = lute_cel::CelArena::default();
    let handle = lute_cel::parse_slot_marked_refs(&mut arena, raw)?;
    let ided = arena.get(handle)?;
    let (path, set) = comparison_set(&ided.expr, ctx.schema)?;
    Some(Gate {
        id: &o.id,
        path,
        raw: o.done.raw.trim(),
        set,
        span: o.span,
    })
}

/// `expr` as ONE in-domain comparison `<declared scalar state path> <op>
/// <literal>` (either operand order) with its solution set over the path's
/// declared type; `None` for anything else.
fn comparison_set(expr: &Expr, schema: &crate::meta::StateSchema) -> Option<(String, SolutionSet)> {
    let Expr::Call(c) = expr else {
        return None;
    };
    if c.target.is_some() || c.args.len() != 2 {
        return None;
    }
    // Normalise to `path <op> literal`; a `literal <op> path` form flips the
    // operator so the solution set is always computed path-side.
    let (path, opname, lit) = match (
        crate::cel_paths::select_path(&c.args[0].expr),
        &c.args[1].expr,
        crate::cel_paths::select_path(&c.args[1].expr),
        &c.args[0].expr,
    ) {
        (Some(p), Expr::Literal(v), _, _) => (p, c.func_name.as_str(), v),
        (_, _, Some(p), Expr::Literal(v)) => (p, flip(&c.func_name)?, v),
        _ => return None,
    };
    let declared = crate::set_op::resolve_type(&path, schema)?;
    let set = solution_set(declared, opname, lit)?;
    Some((path, set))
}

/// The top-level `&&` conjuncts of a condition that are in-domain comparisons
/// (dsl 0.22.0 §13, `W-BEAT-PRIORITY-TIE`): each `path op literal`, a bare
/// `bool` path / its `!` as `== true` / `== false` — the reserved
/// `entry.<id>.read` / `entry.<id>.everRead` flags and a ground `holds(…)` /
/// `visited('…')` query included, as pseudo-paths — and (dsl 0.24.0, T3-3)
/// what a positive `holds(A)` of a pure-schedule derived atom implies about
/// state ([`schedule_conjuncts`]). Every other conjunct constrains nothing
/// here — which only makes exclusivity harder to prove.
///
/// Pairwise disjoint TRUE sets mean "never both true" — weaker than the
/// conjunction deciding `false` (dsl 0.23.0 §9), which an erring read of an
/// unset path (`run.flag && !run.flag`) does not.
#[derive(Clone, Debug, Default)]
pub(crate) struct Conjuncts(Vec<(String, SolutionSet)>);

/// [`Conjuncts`] of `raw` after `@def` expansion, typed against `schema`;
/// `vocab` (the document's relational vocabulary) lets a `holds(A)` conjunct
/// contribute its schedule's state constraints.
pub(crate) fn when_conjuncts(
    raw: &str,
    defs: &DefTable<'_>,
    schema: &crate::meta::StateSchema,
    vocab: Option<&crate::rel_schema::RelVocab>,
) -> Conjuncts {
    let mut out = Vec::new();
    if let Some(expr) = parse_expanded(raw, defs) {
        let ctx = ConjunctCtx { defs, schema, vocab };
        collect_conjuncts(&expr, &ctx, &mut out);
    }
    Conjuncts(out)
}

fn parse_expanded(raw: &str, defs: &DefTable<'_>) -> Option<Expr> {
    let mut stack = Vec::new();
    let expanded = crate::cel_expand::expand_cel(raw, defs, None, &mut stack)
        .unwrap_or_else(|_| raw.to_string());
    let mut arena = lute_cel::CelArena::default();
    let handle = lute_cel::parse_slot_marked_refs(&mut arena, &expanded)?;
    arena.get(handle).map(|ided| ided.expr.clone())
}

struct ConjunctCtx<'a> {
    defs: &'a DefTable<'a>,
    schema: &'a crate::meta::StateSchema,
    vocab: Option<&'a crate::rel_schema::RelVocab>,
}

fn collect_conjuncts(expr: &Expr, ctx: &ConjunctCtx<'_>, out: &mut Vec<(String, SolutionSet)>) {
    if let Expr::Call(c) = expr {
        if c.target.is_none() && c.func_name == op::LOGICAL_AND && c.args.len() == 2 {
            collect_conjuncts(&c.args[0].expr, ctx, out);
            collect_conjuncts(&c.args[1].expr, ctx, out);
            return;
        }
    }
    let bool_path = |e: &Expr| {
        let path = crate::cel_paths::select_path(e)?;
        // The reserved entry flags are engine-written bools (dsl 0.19.0 §5,
        // 0.22.0 §7), undeclared in `state:`.
        (crate::cel_paths::reserved_entry_id(&path).is_some()
            || matches!(crate::set_op::resolve_type(&path, ctx.schema)?, Type::Bool))
        .then_some(path)
    };
    let flag = |path: String, value: bool| {
        (
            path,
            SolutionSet::Values(std::iter::once(DomainValue::Bool(value)).collect()),
        )
    };
    if let Some(hit) = comparison_set(expr, ctx.schema) {
        out.push(hit);
    } else if let Some(path) = bool_path(expr).or_else(|| holds_key(expr)) {
        out.push(flag(path, true));
        if let Some(vocab) = ctx.vocab {
            out.extend(schedule_conjuncts(expr, vocab, ctx));
        }
    } else if let Expr::Call(c) = expr {
        if c.target.is_none() && c.func_name == op::LOGICAL_NOT && c.args.len() == 1 {
            if let Some(path) = bool_path(&c.args[0].expr).or_else(|| holds_key(&c.args[0].expr)) {
                out.push(flag(path, false));
            }
        }
    }
}

/// A ground `holds(rel(args…))` or `visited('id')` query as a pseudo-path
/// (`holds(rel(a,b))`, `visited('id')`), so `holds(P)` and `!holds(P)` are
/// exclusive like `x` and `!x`.
fn holds_key(expr: &Expr) -> Option<String> {
    let Expr::Call(c) = expr else { return None };
    (c.target.is_none() && matches!(c.func_name.as_str(), "holds" | "visited") && c.args.len() == 1)
        .then(|| term_key(&c.args[0].expr).map(|t| format!("{}({t})", c.func_name)))
        .flatten()
}

fn term_key(e: &Expr) -> Option<String> {
    match e {
        Expr::Ident(name) => Some(name.clone()),
        Expr::Literal(Val::String(s)) => Some(format!("'{s}'")),
        Expr::Literal(Val::Int(i)) => Some(i.to_string()),
        Expr::Call(c) if c.target.is_none() => {
            let args: Option<Vec<String>> = c.args.iter().map(|a| term_key(&a.expr)).collect();
            Some(format!("{}({})", c.func_name, args?.join(",")))
        }
        _ => None,
    }
}

/// dsl 0.24.0 (T3-3): what a positive `holds(rel(c…))` implies about state
/// when the atom is a pure schedule — `rel` is `derive: true` and not
/// engine-`reserved`, no seed fact is the atom, and EVERY rule whose head
/// unifies with it is a ground head over `cel()` guards only (`at(sol,
/// radio) :- cel("run.slot == 'morning'")`). The atom then holds exactly when
/// one of those guards does, so a path every guard constrains is constrained
/// to the union of their sets. Anything else (a variable head, a body atom,
/// a comparison literal, no rule at all) implies nothing.
fn schedule_conjuncts(
    expr: &Expr,
    vocab: &crate::rel_schema::RelVocab,
    ctx: &ConjunctCtx<'_>,
) -> Vec<(String, SolutionSet)> {
    use lute_syntax::datalog::{BodyLiteral, FactTerm, RuleTerm};
    let Expr::Call(c) = expr else { return Vec::new() };
    if c.target.is_some() || c.func_name != "holds" || c.args.len() != 1 {
        return Vec::new();
    }
    let Expr::Call(atom) = &c.args[0].expr else { return Vec::new() };
    if atom.target.is_some() {
        return Vec::new();
    }
    let Some(consts) = atom
        .args
        .iter()
        .map(|a| match &a.expr {
            Expr::Ident(n) => Some(n.clone()),
            Expr::Literal(Val::String(s)) => Some(s.to_string()),
            Expr::Literal(Val::Boolean(b)) => Some(b.to_string()),
            _ => None,
        })
        .collect::<Option<Vec<String>>>()
    else {
        return Vec::new();
    };
    let rel = atom.func_name.as_str();
    if !vocab.relations.get(rel).is_some_and(|d| d.derive && !d.reserved) {
        return Vec::new();
    }
    let seeded = vocab.facts.iter().any(|f| {
        f.fact.relation == rel
            && f.fact.args.len() == consts.len()
            && f.fact.args.iter().zip(&consts).all(|(a, k)| match &a.term {
                FactTerm::Ident(i) => i == k,
                FactTerm::Bool(b) => b.to_string() == *k,
                FactTerm::Wildcard => true,
            })
    });
    if seeded {
        return Vec::new();
    }
    // One unifying rule is one disjunct: the conjunction of its guards.
    let inner = ConjunctCtx { vocab: None, ..*ctx };
    let mut per_rule: Vec<Vec<(String, SolutionSet)>> = Vec::new();
    for r in vocab.rules.iter().filter(|r| r.rule.head.relation == rel) {
        let head = &r.rule.head.terms;
        if head.len() != consts.len() {
            continue;
        }
        let mut unifies = true;
        for (t, k) in head.iter().zip(&consts) {
            match t {
                // A variable head (bound by a body atom) is not a schedule.
                RuleTerm::Var(_) => return Vec::new(),
                RuleTerm::Const(v) => unifies &= v == k,
                RuleTerm::Bool(b) => unifies &= b.to_string() == *k,
            }
        }
        if !unifies {
            continue;
        }
        if r.rule.body.is_empty() {
            return Vec::new();
        }
        let mut sets = Vec::new();
        for lit in &r.rule.body {
            let BodyLiteral::Guard { cel, .. } = lit else {
                return Vec::new();
            };
            if let Some(e) = parse_expanded(cel, ctx.defs) {
                collect_conjuncts(&e, &inner, &mut sets);
            }
        }
        per_rule.push(sets);
    }
    if per_rule.is_empty() {
        return Vec::new();
    }
    let mut acc: Vec<(String, SolutionSet)> = Vec::new();
    for (path, set) in &per_rule[0] {
        if acc.iter().any(|(p, _)| p == path) {
            continue;
        }
        let mut joined = Some(set.clone());
        for other in &per_rule[1..] {
            joined = match (joined, other.iter().find(|(p, _)| p == path)) {
                (Some(j), Some((_, s))) => join(&j, s),
                _ => None,
            };
        }
        if let Some(j) = joined {
            acc.push((path.clone(), j));
        }
    }
    acc
}

/// The union of two solution sets of one path, when representable: two
/// finite value sets, or two number sets. `None` otherwise (the path is then
/// not constrained).
fn join(a: &SolutionSet, b: &SolutionSet) -> Option<SolutionSet> {
    use crate::solution::number_spans;
    match (a, b) {
        (SolutionSet::Values(x), SolutionSet::Values(y)) => {
            Some(SolutionSet::Values(x.union(y).cloned().collect()))
        }
        (
            SolutionSet::Interval { .. } | SolutionSet::Union(_),
            SolutionSet::Interval { .. } | SolutionSet::Union(_),
        ) => {
            let mut spans = number_spans(Some(a));
            spans.extend(number_spans(Some(b)));
            Some(SolutionSet::Union(spans))
        }
        _ => None,
    }
}

/// Two conditions that cannot both hold: some pair of their conjuncts
/// constrains one path to disjoint solution sets. Sound, never complete.
pub(crate) fn provably_exclusive(a: &Conjuncts, b: &Conjuncts) -> bool {
    a.0.iter().any(|(pa, sa)| {
        b.0.iter()
            .any(|(pb, sb)| pa == pb && disjoint(sa, sb))
    })
}

/// The operator with its operands exchanged (`1 < x` is `x > 1`).
fn flip(func_name: &str) -> Option<&'static str> {
    Some(match func_name {
        op::EQUALS => op::EQUALS,
        op::NOT_EQUALS => op::NOT_EQUALS,
        op::LESS => op::GREATER,
        op::LESS_EQUALS => op::GREATER_EQUALS,
        op::GREATER => op::LESS,
        op::GREATER_EQUALS => op::LESS_EQUALS,
        _ => return None,
    })
}

/// The `E-OBJECTIVE-CONTRADICTION` message: names BOTH objective ids (it cannot
/// know which is wrong) and the path, quotes both predicates, and appends
/// [`REQUIRED_QUEST_NOTE`] VERBATIM per D-O — the quest consequence reads
/// identically whichever cause found it, and is never a second
/// `E-QUEST-UNREACHABLE` (0.4.0 §8.2 rule C4).
fn contradiction_message(first: &Gate<'_>, second: &Gate<'_>) -> String {
    format!(
        "required objectives `{}` and `{}` cannot both complete: `done=\"{}\"` and \
         `done=\"{}\"` have no common value of `{}` (dsl 0.10.0 §5.2){}",
        first.id, second.id, first.raw, second.raw, first.path, REQUIRED_QUEST_NOTE
    )
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
    format!(
        "quest can never complete: {} (dsl 0.4 §5.3)",
        causes.join("; ")
    )
}

/// The §5.3/C4 quest-consequence note (Task 5 rules, quoted verbatim):
/// appended to [`objective_unsat_message`]'s output when the objective is
/// required (`!optional`). `pub(crate)`: Task 7's `producible.rs` reuses it
/// verbatim for its own THIRD `E-OBJECTIVE-UNSATISFIABLE` cause (a
/// non-producible gated relation) so the required-quest consequence reads
/// identically regardless of which cause triggered the diagnostic.
pub(crate) const REQUIRED_QUEST_NOTE: &str =
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
/// first-match-wins (dsl 0.4 §5.2) ``. When no single earlier arm covers
/// the pattern but several together do (`joint`), the earliest of them is
/// cited as the first of several.
fn subsumption_message(pattern: &str, cov_span: Span, cov_pattern: &str, joint: bool) -> String {
    let by = if joint {
        "earlier unguarded arms together, the first at"
    } else {
        "earlier unguarded arm at"
    };
    format!(
        "arm can never fire: its pattern `{pattern}` is fully covered by the {by} {}:{} \
         (`{cov_pattern}`) — first-match-wins (dsl 0.4 §5.2)",
        cov_span.line, cov_span.column
    )
}

/// dsl 0.24.0: an arm every literal of which the enclosing body's `when`
/// rules out — the body runs only once that guard held.
fn assumed_dead_message(pattern: &str, when: &str) -> String {
    format!(
        "arm can never fire: its pattern `{pattern}` is ruled out by the body's `when` guard \
         `{when}`, which holds whenever this body runs (dsl 0.24.0)"
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
