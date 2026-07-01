pub mod cel_paths;
pub mod cel_resolve;
pub mod check;
pub mod ctx;
pub mod defassign;
pub mod directives;
pub mod inject;
pub mod match_check;
pub mod meta;
pub mod set_op;
pub mod timeline;

pub use cel_resolve::check_cel_slot;
pub use check::{check, CheckInput, CheckResult, Resolved};
pub use ctx::{Ctx, Mode};
pub use inject::{
    lower_node, InjectKind, InjectedCommand, Provenance, SpriteState, StageState, DEFAULT_ANCHOR,
};
pub use defassign::check_definite_assignment;
pub use match_check::{check_branch, check_match, is_exhaustive, BranchRecord};
pub use meta::{parse_meta, Namespace, StateDecl, StateSchema, TypedMeta};
pub use set_op::check_set;
pub use timeline::{resolve_timeline, ResolvedRow, ResolvedTimeline};
