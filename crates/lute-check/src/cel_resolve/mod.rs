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

pub(super) mod core;
pub(super) mod rules;
pub(super) mod profile;
pub(super) mod facts;
pub(super) mod state;
pub use core::*;
pub use rules::*;
pub use profile::*;
pub(crate) use facts::*;
pub(crate) use state::*;

#[cfg(test)]
mod tests;
