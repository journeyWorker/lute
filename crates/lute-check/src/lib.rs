pub mod cel_paths;
pub mod cel_resolve;
pub mod ctx;
pub mod defassign;
pub mod directives;
pub mod match_check;
pub mod meta;
pub mod set_op;

pub use cel_resolve::check_cel_slot;
pub use ctx::{Ctx, Mode};
pub use defassign::check_definite_assignment;
pub use match_check::{check_branch, check_match, is_exhaustive, BranchRecord};
pub use meta::{parse_meta, Namespace, StateDecl, StateSchema, TypedMeta};
pub use set_op::check_set;
