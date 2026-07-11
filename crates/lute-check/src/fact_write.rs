//! `::assert`/`::retract` write policy + pattern validation (dsl 0.3.0 §5).
//!
//! The relational analogue of [`crate::set_op::check_set`] — same short-
//! circuit write-policy discipline, same module shape. Fills the Walker
//! arms `crate::check::Walker::walk` left as leaves (Task 2) now that the
//! merged [`crate::rel_schema::RelVocab`] (Task 7) is available on [`Ctx`].
//!
//! **Write policy** (§5, in precedence order — the first match short-
//! circuits and stops further checking, exactly like `check_set`'s
//! `app`/reserved-quest short-circuit):
//! 1. `derive: true` → [`E_DERIVED_WRITE`] — engine-computed by `rules:`.
//! 2. `reserved: true` → `E-RELATION-RESERVED-WRITE` (reused from
//!    [`crate::rel_schema`]) — engine-populated by non-Datalog means
//!    (mirrors `E-QUEST-RESERVED-WRITE`).
//! 3. `app`-tier (neither derived nor reserved, via
//!    [`crate::rel_schema::RelVocab::tier_of`]) → [`E_FACT_TIER_WRITE`]
//!    (mirrors `E-APP-READONLY`).
//! 4. Otherwise author-writable; the pattern itself is validated by the
//!    shared [`crate::rel_schema::check_atom`] closure checker — unknown-
//!    relation/arity/domain diagnostics, plus wildcard legality
//!    (`::assert` is fully ground, `wildcard_ok = false`; `::retract`
//!    admits `_`, `wildcard_ok = true`).
//!
//! An unknown relation is diagnosed BEFORE the write-policy checks (there is
//! no decl to classify) by delegating straight to `check_atom`, which owns
//! the one `E-RELATION-UNKNOWN` diagnostic (incl. its entity-kind hint) —
//! never duplicated here.
//!
//! D13: a sentinel pattern (`relation.is_empty()`, an already parse-
//! diagnosed malformed `::assert`/`::retract` payload) is skipped — never
//! double-reported.

use std::collections::BTreeMap;

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::snapshot::Domain;
use lute_syntax::ast::{Assert, Retract};
use lute_syntax::datalog::FactPattern;

use crate::rel_schema::{check_atom, RelVocab, E_RELATION_RESERVED_WRITE};
use crate::Ctx;

/// A `derive: true` relation asserted/retracted by content (§5 policy 1).
pub const E_DERIVED_WRITE: &str = "E-DERIVED-WRITE";
/// An `app`-tier base relation asserted/retracted by content (§5 policy 3).
pub const E_FACT_TIER_WRITE: &str = "E-FACT-TIER-WRITE";

/// Check an `::assert{ GroundFact }` directive (dsl 0.3.0 §5): write policy,
/// then full pattern validation with `wildcard_ok = false` — a `_` in an
/// assert pattern is `E-RETRACT-WILDCARD-ASSERT` (assert is a single
/// concrete delta, never a bulk operation).
pub fn check_assert(a: &Assert, domains: &BTreeMap<String, Domain>, ctx: &Ctx<'_>) -> Vec<Diagnostic> {
    check_write(&a.pattern, a.span, /* wildcard_ok = */ false, domains, ctx)
}

/// Check a `::retract{ RetractPattern }` directive (dsl 0.3.0 §5): write
/// policy, then full pattern validation with `wildcard_ok = true` — `_` is
/// legal in every ground position.
pub fn check_retract(r: &Retract, domains: &BTreeMap<String, Domain>, ctx: &Ctx<'_>) -> Vec<Diagnostic> {
    check_write(&r.pattern, r.span, /* wildcard_ok = */ true, domains, ctx)
}

/// Shared assert/retract logic (§5), short-circuiting exactly like
/// `set_op::check_set` (`set_op.rs:75-165`): D13 sentinel → unknown relation
/// → write policy (precedence order) → pattern validation.
fn check_write(
    pattern: &FactPattern,
    span: Span,
    wildcard_ok: bool,
    domains: &BTreeMap<String, Domain>,
    ctx: &Ctx<'_>,
) -> Vec<Diagnostic> {
    // D13 sentinel: an already parse-diagnosed malformed payload — never
    // double-report.
    if pattern.relation.is_empty() {
        return Vec::new();
    }

    let vocab: &RelVocab = &ctx.env.rel_vocab;
    let Some(decl) = vocab.relations.get(&pattern.relation) else {
        // Unknown relation: `check_atom` owns the ONE unknown-relation
        // diagnostic (incl. its entity-kind hint) — there is no decl to
        // classify against the write policy, so delegate straight to it
        // rather than re-implementing the lookup/message here.
        return check_atom(
            vocab,
            domains,
            &pattern.relation,
            &pattern.args,
            wildcard_ok,
            span,
        );
    };

    // Write policy (§5), in precedence order — each case short-circuits,
    // matching `check_set`'s `app`/reserved-quest short-circuit discipline.
    if decl.derive {
        return vec![diag(
            E_DERIVED_WRITE,
            format!(
                "relation `{}` is `derive: true`: it is computed by `rules:` and MUST NOT be \
                 asserted or retracted by content (dsl 0.3.0 §5)",
                pattern.relation
            ),
            span,
        )];
    }
    if decl.reserved {
        return vec![diag(
            E_RELATION_RESERVED_WRITE,
            format!(
                "relation `{}` is `reserved: true`: it is engine-populated by non-Datalog means \
                 and MUST NOT be asserted or retracted by content (dsl 0.3.0 §4/§5); populate it \
                 via the schema `facts:` seed block or an engine-side write",
                pattern.relation
            ),
            span,
        )];
    }
    if vocab.tier_of(decl) == Some("app") {
        return vec![diag(
            E_FACT_TIER_WRITE,
            format!(
                "relation `{}` is `app`-tier: it is engine-owned/read-only to content, exactly \
                 as `app.*` scalar state (dsl 0.3.0 §5/§9.5); populate it via the schema \
                 `facts:` seed block",
                pattern.relation
            ),
            span,
        )];
    }

    // Author-writable (`scene`/`run`/`user`/`quest`-scratch base relation):
    // arity/domain/wildcard-legality validation via the shared closure
    // checker (Task 7).
    check_atom(
        vocab,
        domains,
        &pattern.relation,
        &pattern.args,
        wildcard_ok,
        span,
    )
}

/// Build a `Layer::Staging` error diagnostic (`::assert`/`::retract` are
/// staging directives, dsl 0.3.0 §5 — Global Constraints' layer table).
fn diag(code: &str, message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity: Severity::Error,
        message,
        span,
        layer: Layer::Staging,
        fixits: Vec::new(),
        provenance: None,
    }
}
