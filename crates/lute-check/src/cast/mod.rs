//! Cast (dsl 0.23.0 §7): a plugin (`cast/*.yaml`) or a schema document
//! (`cast:`) MAY declare the speaker ids a project uses. Once any cast is
//! declared, a content line whose speaker is outside it is
//! [`E_CAST_UNKNOWN`] with a did-you-mean; without one, speakers stay
//! shape-only. `narrator` is always a speaker. Since dsl 0.24.0 §4 the
//! character a staging directive names — `::actor{character}` and
//! `::camera{focus}` — is held to the same cast.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use cel_parser::ast::{operators as op, CallExpr, Expr, IdedExpr};
use cel_parser::reference::Val;
use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::relations::KindShape;
use lute_syntax::ast::{Arm, CelSlot, ClipNode, Directive, Document, Line, Match, Node};
use lute_syntax::datalog::{is_anonymous_var, BodyLiteral, FactPattern, FactTerm, RuleTerm};
use lute_syntax::is_pattern::{classify_is_literal, is_alternatives, IsLiteral};
use crate::beats::BeatOnce;
use crate::cel_expand::{expand_cel, subject_text, DefTable};
use crate::check::FoldedEnv;
use crate::decide::{decide, DecideCtx, Decided};
use crate::fact_env::{FactEnv, FactScope};
use crate::match_check::DomainInfo;
use crate::rel_schema::RelVocab;
mod emotion;
mod facts;
mod presence;
mod speaker;

/// A content line's speaker is not in the declared cast (dsl 0.23.0 §7).
pub const E_CAST_UNKNOWN: &str = "E-CAST-UNKNOWN";

pub use emotion::{check_emotions, decide_guard_implication, W_CAST_ABSENT};
pub use facts::{fact_producers, occasions_before, FactProducers};
pub use presence::{check_presence, reconcile_presence, PresenceProject};
pub use speaker::{check_speakers, declared_cast};

pub(crate) use emotion::{cast_diag, doc_bodies, literal_attr, present_faults, validate_member, visit};
pub(crate) use facts::{fact_writes, pattern_args, unit_facts, FactWrite};
pub(crate) use presence::{write_effects, Atom, Effect, Pol};
pub(crate) use speaker::unknown;
