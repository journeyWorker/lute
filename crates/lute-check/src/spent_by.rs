//! dsl 0.27.0 §5: a beat's `spentBy` judged like its `when` (HW27-03). A
//! beat is eligible only while its `spentBy` does not hold, so
//!
//! - one whose `when && !spentBy` is provably false is never eligible —
//!   `E-BEAT-UNREACHABLE` (`E-ENTRY-UNREACHABLE` for an entry), the verdict
//!   `when: false` gets; and
//! - one whose `spentBy` already holds at the start of a run (the seeds are
//!   the only facts, the state its declared domains) is not eligible until
//!   the condition stops holding — `W-BEAT-SPENT-AT-START`. The common
//!   slip is migrating `when="!holds(X)"` to `spentBy` without dropping
//!   the `!`.

use std::collections::BTreeSet;
use std::path::Path;

use lute_core_span::{Diagnostic, Severity};
use lute_syntax::ast::CelSlot;

use crate::cel_expand::DefTable;
use crate::decide::{decide_slot, DecideCtx, Decided};

/// `W-BEAT-SPENT-AT-START` (dsl 0.27.0 §5): a beat's `spentBy` already
/// holds in a run's initial state, so the beat is not eligible until it
/// stops holding.
pub const W_BEAT_SPENT_AT_START: &str = "W-BEAT-SPENT-AT-START";

/// The `spentBy` verdicts of every beat of `doc` (see the module doc).
pub(crate) fn check_spent_by(
    doc: &lute_syntax::ast::Document,
    folded: &crate::check::FoldedEnv,
    defs: &DefTable<'_>,
    ctx: &DecideCtx<'_>,
) -> Vec<Diagnostic> {
    let doc_id = folded.typed.id.as_deref().unwrap_or("this document");
    let mut beats: Vec<(String, &'static str, Option<&CelSlot>, &CelSlot)> = Vec::new();
    if let Some(b) = &folded.typed.beat {
        if let Some(s) = &b.spent_by {
            let name = format!("beat `{}`", crate::beats::scene_beat_name(folded));
            beats.push((name, crate::beats::E_BEAT_UNREACHABLE, b.when.as_ref(), s));
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
            ));
        }
    }
    for b in &doc.beats {
        if let Some(s) = &b.spent_by {
            let name = format!("beat `{}`", crate::bundles::bundle_beat_key(doc_id, &b.id));
            beats.push((name, crate::beats::E_BEAT_UNREACHABLE, b.when.as_ref(), s));
        }
    }
    let mut out = Vec::new();
    // The run's initial facts, built once and only when a beat asks.
    let mut start: Option<crate::FactEnv> = None;
    for (name, code, when, spent) in beats {
        let raw = spent.raw.trim();
        if raw.is_empty() {
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
                     is not eligible while its `spentBy` holds (dsl 0.27.0 §5)"
                ),
                spent.span,
            ));
            continue;
        }
        let env = start.get_or_insert_with(|| initial_facts(folded));
        let path = Path::new("");
        let mut must = crate::MustMap::default();
        must.insert(path, spent.span, seeds(folded));
        env.must = must;
        let env = &*env;
        let start_ctx = DecideCtx {
            schema: ctx.schema,
            dollar: None,
            params: ctx.params,
            facts: Some(crate::FactScope {
                env,
                vocab: &folded.env.rel_vocab,
                path,
                span: spent.span,
                wip: false,
            }),
        };
        if decides(raw, &start_ctx) == Some(Decided::Bool(true)) {
            let hint = raw
                .strip_prefix('!')
                .map(str::trim)
                .filter(|r| !r.is_empty())
                .map(|r| {
                    format!(
                        " — a beat spent once a result is reached reads the result itself: did \
                         you mean `spentBy: \"{r}\"`?"
                    )
                })
                .unwrap_or_default();
            out.push(crate::reachability::diag(
                W_BEAT_SPENT_AT_START,
                Severity::Warning,
                format!(
                    "{name} starts spent: its `spentBy: {raw}` already holds at the start of a \
                     run, so it is not eligible until that stops holding{hint} (dsl 0.27.0 §5)"
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
