//! `<match>` exhaustiveness + first-match-wins lint, and `<branch>` recording
//! (dsl §11.1, §11.2).
//!
//! ## `<match>` (§11.2)
//! Arms run **top-to-bottom, first match wins**. An `<otherwise>` is REQUIRED
//! unless the subject's domain is *finite* and *fully covered* by the `<when>`
//! arms. Domain finiteness is inferred from the subject's declared `state:` type
//! ([`StateSchema`]): a `bool` (domain `{true,false}`), an `enum` (domain = its
//! members), or a `scene.choices.<branchId>` path (domain = the branch's choice
//! ids ∪ `unset`) is FINITE; a `number` is the real line ([`Domain::Number`],
//! dsl 0.18.0 §4 — covered only by a union of intervals spanning it);
//! anything else (string/record/…) is INFINITE and therefore requires
//! `<otherwise>`.
//!
//! Coverage is computed from the arms' **`is` literal patterns** (§7.3.1) — the
//! NORMATIVE path (§11.2) — unioned with the conservative values a `test` guard
//! provably matches (recognizing `test` coverage is downgraded to MAY). An `is`
//! pattern (`Literal ("|" Literal)*`, classified by the shared
//! [`lute_syntax::is_pattern`]) contributes each literal to `covered`: an
//! enum member / choice id → the string value, `true`/`false` → the bool value,
//! a decimal `Number` → the point interval `[n, n]`, a range `N..M`/`N..`/`..M`
//! (dsl 0.18.0 §2) → its closed interval, and `unset` → the unset case.
//! Diagnostics:
//!
//! - **`E-WHEN-RANGE`** — a malformed or empty range literal (dsl 0.18.0 §2);
//!   it covers nothing.
//! - **`E-NONEXHAUSTIVE`** — no `<otherwise>` and the domain is either infinite,
//!   or finite/number but not fully covered by the `<when>` arms (a finite
//!   domain's message names the uncovered members, a number domain's the first
//!   uncovered gap, dsl 0.18.0 §4).
//!
//! A whole-subject `@def` (`<match subject="@wd">`, dsl 0.24.0) takes its domain from
//! [`resolve_subject`]: the one state path its body is, else its declared or
//! inferred result type.
//! - **`E-WHEN-PATTERN`** — a `<when>` arm with neither an `is` pattern nor a
//!   `test` guard (§7.3.1); one of the two is REQUIRED.
//! - **`E-MATCH-NO-SUBJECT`** — a `<when is>` arm in a `<match>` with no `on`:
//!   there is no subject to compare against. An all-`test` subject-less match
//!   is legal.
//! - **`E-UNSET-UNCOVERED`** — the subject is *maybe-unset* (`scene.choices.*`, or
//!   a `run.*`/`user.*`/`app.*` decl with no schema `default`) and the `unset`
//!   case is not covered by an `unset`-matching arm nor an `<otherwise>`.
//! - **`E-AGE-GATE`** — an age-gated `<match subject="app.rating">` that covers neither
//!   a `teen` arm nor an `<otherwise>` (a release-build hard gate, §11.2).
//! - **`E-MATCH-DUP-OTHERWISE`** — more than one `<otherwise>`; §11.2 allows at
//!   most one. Flatten routes only the last, so earlier otherwise bodies would be
//!   unreachable. Flagged at every `<otherwise>` past the first.
//! - **`W-OVERLAP-ARMS`** (Warning) — two `<when>` arms that *provably* match the
//!   same value (kept conservative: identical literal equality tests only, never
//!   general SAT). First-match-wins means the later arm is dead. Numeric
//!   literals: only a point (or degenerate `n..n` range) already inside
//!   earlier coverage warns. A wider range never does — partial overlap is
//!   the descending-threshold cascade (`3..` then `1..`, first match wins),
//!   and full containment is `E-ARM-DEAD`'s (dsl 0.18.0 §4).
//!
//! ## `<branch>` (§11.1)
//! `<branch id>` MUST be unique within the episode (the `.lute` document);
//! selecting a choice records `scene.choices.<branchId> = <choiceId>`, an
//! implicitly-declared, episode-scoped path whose domain is the branch's choice
//! ids ∪ `unset`. [`check_branch`] emits **`E-DUP-BRANCH`** on a repeat id,
//! **`E-BRANCH-EMPTY`** on a branch with no `<choice>` (§7.3 requires `Choice+`;
//! an empty branch would flatten to an unroutable choice), **`E-CHOICE-DUP`** on
//! a repeated choice id, and returns the implicit [`StateDecl`] to fold into the
//! schema.
//!
//! ### Branch-dup threading (for T4.9 assembly)
//! Duplicate detection is *episode-wide*, but this module checks one branch at a
//! time. Rather than hide episode state in `Ctx` (which is per-check and cloned),
//! the caller threads a `&mut BTreeSet<String>` of seen branch ids in **document
//! order** and folds each returned [`BranchRecord::decl`] into the accumulating
//! `StateSchema`. This keeps `Ctx` immutable and the episode set explicit and
//! caller-owned — the T4.9 whole-document walk already iterates shots/nodes in
//! order, so it owns the set and the schema it grows.
//!
//! ## Spans / layer
//! All diagnostics are [`Layer::Logic`] (§9/§11 logic checks). Per the cel-parser
//! 0.10.1 carry-forward (T3.1/T4.3) arm-test byte offsets are unavailable, so
//! coverage is reconstructed from a throwaway re-parse of each slot's raw CEL and
//! diagnostics fall back to the enclosing match/arm/branch span.

use std::collections::{BTreeMap, BTreeSet};

use cel_parser::ast::Expr;
use cel_parser::reference::Val;
use lute_cel::CelArena;
use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::provider::{IdStatus, ProviderSet};
use lute_manifest::relations::{EntityKindDecl, KindShape};
use lute_manifest::schema::RewardTarget;
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_manifest::types::{Literal, Type};
use lute_syntax::ast::{
    Arm, Attr, AttrValue, Branch, Document, Hub, IsPattern, Line, Match, Node, Quest, Reward,
};
use lute_syntax::is_pattern::{classify_is_literal, is_alternatives, IsLiteral, IsLiteralError};

use crate::cel_paths::{is_reserved_quest_activated_at, E_PATH_IDENT};
use lute_manifest::semantics::cel_paths::{
    is_reserved_entry_read, is_reserved_quest_objective_done, QUEST_STATES,
};
use crate::meta::{Namespace, StateDecl, StateSchema};
use crate::Ctx;

mod domain;
mod exhaustive;
mod interval;
mod line_codes;
mod pattern;
mod records;
mod rewards;

pub(crate) use domain::{infer_domain, param_domain, resolve_subject, subject_path};
pub use domain::{Domain, DomainInfo, DomainValue};
pub use exhaustive::{
    check_match, is_exhaustive, E_MATCH_NO_SUBJECT, E_WHEN_LITERAL_DOMAIN, E_WHEN_PATTERN,
    E_WHEN_RANGE,
};
pub(crate) use exhaustive::{
    check_match_has_subject, check_match_with_domain, check_param_match, handles_unset_resolved,
    is_exhaustive_resolved, CoverItem,
};
pub(crate) use interval::{Interval, NumCoverage};
pub use line_codes::check_line_codes;
pub(crate) use line_codes::check_line_codes_with_policy;
pub(crate) use line_codes::collect_lines;
pub use pattern::is_pattern_literals;
use pattern::{
    arm_coverage, bad_range_message, domain_members_display, foreign_literal_message, parse_expr,
};
pub(crate) use pattern::{
    arm_takes_unset, is_pattern_proves_set, literal_is_foreign, quest_state_is_literal,
};
pub use records::{
    check_branch, check_hub, check_quest, BranchRecord, HubRecord, QuestRecord,
    E_BRANCH_ALL_GUARDED, E_HUB_NO_EXIT, E_OBJECTIVE_QUEST_DONE, E_QUEST_TREE_CYCLE,
};
pub use rewards::{
    check_quest_rewards, E_REWARD_ATTR, E_REWARD_DUP, E_REWARD_KIND, E_REWARD_TARGET,
    W_REWARD_DOUBLE_CREDIT,
};

/// Build a `Layer::Logic` diagnostic (a §9/§11 logic check).
fn diag(code: &str, severity: Severity, message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity,
        message,
        evidence: None,
        span,
        layer: Layer::Logic,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

#[cfg(test)]
mod tests;
