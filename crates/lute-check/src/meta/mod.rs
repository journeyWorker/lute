use std::collections::{BTreeMap, BTreeSet};

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::schema::DefParam;
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_manifest::types::{lit_str, type_accepts, type_str, Literal, Type};
use lute_syntax::ast::Meta;

use crate::cel_paths::E_PATH_IDENT;
use lute_manifest::semantics::cel_paths::is_reserved_quest_path;

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

// Shared helpers the sibling submodules reach through `use super::*`.
use diagnostic_helpers::{err_at, scalar_span};
use diagnostics::*;
use kind::*;
use state::*;

pub use diagnostics::{
    ident_from_name, infer_meta_kind_from_shape, meta_key_span, meta_path_span, PendingPer,
};
pub use frontmatter::{frontmatter_parses, parse_meta, parse_meta_kind, parse_meta_kind_with_defaults};
pub use kind::{
    apply_quest_tier_default, authored_doc_kind, canonical_episode_id, canonical_episode_key, canonical_scene_key,
    default_key_legal_on, resolve_doc_kind, resolve_doc_kind_with_defaults, DocKind, MetaKind,
    E_KIND_MISSING, E_STATE_COLLECTION, E_UNKNOWN_KIND, SCENE_KEYS,
};
pub use types::{FactDecl, Namespace, RuleDecl, StateDecl, StateSchema, TypedMeta};

pub(crate) use diagnostic_helpers::yaml_key;
pub(crate) use diagnostics::{expand_per_pending, yaml_shape};
pub(crate) use state::{engine_namespace, namespace_of};
