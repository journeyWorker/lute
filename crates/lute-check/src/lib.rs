pub mod cel_paths;
pub mod cel_resolve;
pub mod check;
pub mod component_import;
pub mod ctx;
pub mod defassign;
pub mod directives;
pub mod inject;
pub mod match_check;
pub mod meta;
pub mod schema_import;
pub mod set_op;
pub mod tag;
pub mod timeline;

pub use cel_paths::E_PATH_IDENT;
pub use cel_resolve::{check_cel_slot, E_CEL_PROFILE};
pub use check::{check, fold_env, CheckInput, CheckResult, FoldedEnv, Resolved, E_HUB_UNSUPPORTED};
pub use component_import::{resolve_components, ComponentDef, ComponentSet};
pub use ctx::{Ctx, Mode};
pub use defassign::check_definite_assignment;
pub use inject::{
    lower_node, InjectKind, InjectedCommand, Provenance, SpriteState, StageState, DEFAULT_ANCHOR,
};
pub use match_check::{check_branch, check_line_codes, check_match, is_exhaustive, BranchRecord};
pub use meta::{
    parse_meta, parse_meta_kind, MetaKind, Namespace, StateDecl, StateSchema, TypedMeta,
};
pub use schema_import::{resolve_imports, SchemaImports};
pub use set_op::check_set;
pub use tag::{tag_document, TagOutcome};
pub use timeline::{resolve_timeline, ResolvedRow, ResolvedTimeline};
