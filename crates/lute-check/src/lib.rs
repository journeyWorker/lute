pub mod cel_resolve;
pub mod ctx;
pub mod directives;
pub mod meta;

pub use cel_resolve::check_cel_slot;
pub use ctx::{Ctx, Mode};
pub use meta::{parse_meta, Namespace, StateDecl, StateSchema, TypedMeta};
