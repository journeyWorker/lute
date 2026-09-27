//! A beat's `spentBy` judged like its `when` (HW27-03). A `spentBy` beat is
//! spent by its condition instead of by being presented: once the condition
//! has held, the beat stays spent for its `once` period (`run` unless
//! written). So
//!
//! - one whose `when && !spentBy` is provably false is never eligible —
//!   `E-BEAT-UNREACHABLE` (`E-ENTRY-UNREACHABLE` for an entry), the verdict
//!   `when: false` gets; and
//! - one whose `spentBy` already holds at the start of play — every state
//!   path at its declared default, the seeds the only facts, no scene
//!   presented, every quest `unset`, no entry read — is spent before it can
//!   play at all: `W-BEAT-SPENT-AT-START`. The common slip is reading
//!   `spentBy` as "repeat while" (`spentBy: "!run.balloonUp"`), or migrating
//!   `when="!holds(X)"` without dropping the `!`.
//!
//! A `spentBy` whose literal comparison is already reported (a member
//! outside the path's domain, a mistyped literal) draws neither verdict:
//! the literal is the cause.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use cel_parser::ast::{Expr, IdedExpr};
use cel_parser::reference::Val;
use lute_core_span::{Diagnostic, Severity, Span};
use lute_manifest::types::Literal;
use lute_syntax::ast::CelSlot;

use crate::cel_expand::{expand_cel, DefTable};
use crate::decide::{decide, decide_slot, DecideCtx, Decided};
use crate::meta::Namespace;

/// `W-BEAT-SPENT-AT-START`: a beat's `spentBy` already holds at the start
/// of play, so the beat is spent before it can ever be presented.
pub const W_BEAT_SPENT_AT_START: &str = "W-BEAT-SPENT-AT-START";

/// The `spentBy` verdicts of every beat of `doc` (see the module doc).
pub(crate) fn check_spent_by(
    doc: &lute_syntax::ast::Document,
    folded: &crate::check::FoldedEnv,
    defs: &DefTable<'_>,
    ctx: &DecideCtx<'_>,
) -> Vec<Diagnostic> {
    let doc_id = folded.typed.id.as_deref().unwrap_or("this document");
    // Each beat with its element's span, where its environment is found.
    let mut beats: Vec<(String, &'static str, Option<&CelSlot>, &CelSlot, Span)> = Vec::new();
    if let Some(b) = &folded.typed.beat {
        if let Some(s) = &b.spent_by {
            let name = format!("beat `{}`", crate::beats::scene_beat_name(folded));
            beats.push((
                name,
                crate::beats::E_BEAT_UNREACHABLE,
                b.when.as_ref(),
                s,
                s.span,
            ));
        }
    }
    for e in &doc.entries {
        if let Some(s) = &e.spent_by {
            let name = format!("entry `{}`", e.id);
            beats.push((
                name,
                crate::reachability::E_ENTRY_UNREACHABLE,
                e.when.as_ref(),
                s,
                e.span,
            ));
        }
    }
    for b in &doc.beats {
        if let Some(s) = &b.spent_by {
            let name = format!("beat `{}`", crate::bundles::bundle_beat_key(doc_id, &b.id));
            beats.push((
                name,
                crate::beats::E_BEAT_UNREACHABLE,
                b.when.as_ref(),
                s,
                b.span,
            ));
        }
    }
    let mut out = Vec::new();
    // The start of play, built once and only when a beat asks.
    let mut start: Option<(crate::FactEnv, BTreeMap<String, Val>)> = None;
    for (name, code, when, spent, at) in beats {
        // dsl 0.28.0: judged over the beat's own environment.
        let own = DecideCtx {
            schema: &folded.env_at(at).state,
            dollar: None,
            params: ctx.params,
            facts: None,
        };
        let ctx = &own;
        let raw = spent.raw.trim();
        if raw.is_empty() {
            continue;
        }
        // One report per mistake: a reported literal is the cause.
        if !crate::decide::analyze_literal_comparisons(raw, defs, ctx)
            .hits
            .is_empty()
        {
            continue;
        }
        let when = when.map(|w| w.raw.trim()).filter(|w| !w.is_empty());
        let decides = |c: &str, ctx: &DecideCtx<'_>| decide_slot(c, defs, ctx);
        // A `when` already false alone is its own verdict.
        if when.is_some_and(|w| decides(w, ctx) == Some(Decided::Bool(false))) {
            continue;
        }
        let eligible = match when {
            Some(w) => format!("({w}) && !({raw})"),
            None => format!("!({raw})"),
        };
        if decides(&eligible, ctx) == Some(Decided::Bool(false)) {
            let whenever = match when {
                Some(w) => format!("whenever its `when` `{w}` does"),
                None => "always".to_string(),
            };
            out.push(crate::reachability::diag(
                code,
                Severity::Error,
                format!(
                    "{name} is never eligible: its `spentBy: {raw}` holds {whenever}, and a beat \
                     is spent once its `spentBy` has held"
                ),
                spent.span,
            ));
            continue;
        }
        let (env, pins) =
            start.get_or_insert_with(|| (initial_facts(folded), start_values(folded)));
        let path = Path::new("");
        let mut must = crate::MustMap::default();
        must.insert(path, spent.span, seeds(folded));
        env.must = must;
        let start_ctx = DecideCtx {
            schema: ctx.schema,
            dollar: None,
            params: ctx.params,
            facts: Some(crate::FactScope {
                env: &*env,
                vocab: &folded.env.rel_vocab,
                path,
                span: spent.span,
                wip: false,
            }),
        };
        if decide_at_start(raw, defs, pins, &start_ctx) == Some(Decided::Bool(true)) {
            let hint = raw
                .strip_prefix('!')
                .map(str::trim)
                .filter(|r| !r.is_empty())
                .map(|r| {
                    format!(
                        " — `spentBy` names what spends the beat, not what keeps it: did you \
                         mean `spentBy: \"{r}\"`, or `when: \"{raw}\"` for a beat that repeats \
                         while it holds?"
                    )
                })
                .unwrap_or_default();
            out.push(crate::reachability::diag(
                W_BEAT_SPENT_AT_START,
                Severity::Warning,
                format!(
                    "{name} is spent before it can play: its `spentBy: {raw}` already holds at \
                     the start (every state path at its default, only the seed facts), and a \
                     `spentBy` beat stays spent once its condition has held{hint}"
                ),
                spent.span,
            ));
        }
    }
    out
}

/// The fact envelope of a run's first moment in `folded`'s document: the
/// seeds (and what the rules derive from them) may hold, nothing any
/// content asserts does yet. A relation the engine writes (unbounded) stays
/// undecided. Its must set is the seeds, placed at the slot being judged.
fn initial_facts(folded: &crate::check::FoldedEnv) -> crate::FactEnv {
    let mut vocab = crate::RootVocab::default();
    vocab.add(&folded.env.rel_vocab, &folded.env.domains);
    let may = crate::MaySet::build(&vocab, std::iter::empty(), &BTreeSet::new());
    crate::FactEnv::new(may, crate::MustMap::default())
}

/// The document's seed facts, as must facts.
fn seeds(folded: &crate::check::FoldedEnv) -> Vec<crate::fact_env::MustFact> {
    folded
        .env
        .rel_vocab
        .facts
        .iter()
        .filter_map(|s| crate::GroundFact::from_pattern(&s.fact))
        .map(|fact| crate::fact_env::MustFact {
            fact,
            provenance: crate::fact_env::Provenance::Seed,
        })
        .collect()
}

/// The value every persistent state path holds at the start of play: its
/// declared scalar default (`run.*` / `user.*` / `app.*` / `season.*`). A path without
/// one stays a read with no value.
fn start_values(folded: &crate::check::FoldedEnv) -> BTreeMap<String, Val> {
    folded
        .env
        .state
        .decls
        .iter()
        .filter(|(_, d)| {
            matches!(
                d.namespace,
                Namespace::Run | Namespace::User | Namespace::App | Namespace::Season
            )
        })
        .filter_map(|(path, d)| {
            let v = match d.default.as_ref()? {
                Literal::Bool(b) => Val::Boolean(*b),
                Literal::Num(n) => Val::Double(*n),
                Literal::Str(s) => Val::String(s.clone()),
                Literal::List(_) | Literal::Map(_) => return None,
            };
            Some((path.clone(), v))
        })
        .collect()
}

/// Decide `raw` at the start of play: its `@def`s expanded, every state
/// path with a start value replaced by it, every quest `unset`, every entry
/// unread, no scene visited — then the ordinary decision.
fn decide_at_start(
    raw: &str,
    defs: &DefTable<'_>,
    pins: &BTreeMap<String, Val>,
    ctx: &DecideCtx<'_>,
) -> Option<Decided> {
    let mut stack = Vec::new();
    let expanded = expand_cel(raw, defs, Some("$"), &mut stack).unwrap_or_else(|_| raw.to_string());
    let mut arena = lute_cel::CelArena::default();
    let handle = lute_cel::parse_slot_marked_refs(&mut arena, &expanded)?;
    let mut ided = arena.get(handle)?.clone();
    pin_start(&mut ided, pins);
    decide(&ided.expr, ctx)
}

/// Replace, in place, every read [`decide_at_start`] knows the start value of.
fn pin_start(e: &mut IdedExpr, pins: &BTreeMap<String, Val>) {
    if let Some(path) = crate::cel_paths::select_path(&e.expr) {
        let start = if let Some(v) = pins.get(&path) {
            Some(v.clone())
        } else if crate::cel_paths::is_reserved_quest_state(&path) {
            Some(Val::String("unset".to_string()))
        } else if crate::cel_paths::is_reserved_entry_read(&path)
            || crate::cel_paths::is_entry_ever_read(&path)
            || crate::cel_paths::is_reserved_quest_objective_done(&path)
        {
            Some(Val::Boolean(false))
        } else {
            None
        };
        if let Some(v) = start {
            e.expr = Expr::Literal(v);
        }
        return;
    }
    match &mut e.expr {
        Expr::Call(c) => {
            if c.target.is_none() && c.func_name == crate::cel_resolve::VISITED_FN {
                e.expr = Expr::Literal(Val::Boolean(false));
                return;
            }
            if let Some(t) = c.target.as_deref_mut() {
                pin_start(t, pins);
            }
            for a in &mut c.args {
                pin_start(a, pins);
            }
        }
        Expr::List(l) => {
            for x in &mut l.elements {
                pin_start(x, pins);
            }
        }
        _ => {}
    }
}
