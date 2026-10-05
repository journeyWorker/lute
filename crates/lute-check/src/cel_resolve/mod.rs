//! CEL slot resolution split by responsibility.

use cel_parser::ast::Expr;
use cel_parser::reference::Val;
use lute_cel::CelArena;
use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_syntax::ast::{CelKind, CelSlot};
use lute_syntax::datalog::{BodyLiteral, FactArg, FactTerm};

use crate::cel_paths::{collect_path_uses, is_reserved_entry_read, is_reserved_quest_path};
use crate::ctx::ExpectedType;
use crate::rel_schema::{check_atom, RelVocab};
use crate::Ctx;
use lute_manifest::types::Type;

mod core;
mod facts;
mod profile;
mod rules;
mod state;

pub use core::{
    check_cel_slot, E_CEL_PROFILE, E_CEL_TYPE, E_DATALOG_GUARD_FACT, E_MATCH_RELATION_SUBJECT,
    E_VALIDAT_DERIVED,
};
pub use profile::{visited_call_target, visited_targets, VISITED_FN};
pub use rules::{
    check_rule_guards, expand_rule_guard, expand_rule_guards, E_RULE_GUARD_DEF,
    W_QUEST_STATE_HAS,
};

pub(crate) use core::group_per_member;
pub(crate) use facts::{check_match_subject_defs, pattern_terms};
pub(crate) use profile::{count_distinct_column, is_profile_fact_query, query_relation};
pub(crate) use rules::check_def_body;
pub(crate) use state::{compatible, ty_desc};

#[cfg(test)]
mod tests;
