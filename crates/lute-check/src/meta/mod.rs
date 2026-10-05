use std::collections::{BTreeMap, BTreeSet};

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::schema::DefParam;
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_manifest::types::{lit_str, type_accepts, type_str, Literal, Type};
use lute_syntax::ast::Meta;

use crate::cel_paths::{is_reserved_quest_path, E_PATH_IDENT};

mod types;
mod kind;
mod diagnostic_helpers;
mod frontmatter;
mod diagnostics;
mod datalog;
mod frontmatter_state;
mod state;
#[cfg(test)]
mod tests;

pub(crate) use diagnostic_helpers::yaml_key;
pub(super) use diagnostic_helpers::{err_at, scalar_span};
pub(super) use diagnostics::*;
pub(super) use kind::*;
pub(super) use state::*;

pub use types::*;
pub use kind::*;
pub use frontmatter::*;
pub use diagnostics::{ident_from_name, infer_meta_kind_from_shape, meta_key_span, meta_path_span};
pub(crate) use diagnostics::yaml_shape;
pub use diagnostics::PendingPer;
pub(crate) use diagnostics::expand_per_pending;
pub(crate) use state::{engine_namespace, namespace_of};
