//! Shared source discovery and input loading.
//!
//! This crate owns the non-compiling input boundary used by project, CLI, and
//! LSP consumers. It deliberately has no dependency on project compilation or
//! semantic analysis.

pub mod cache;
pub mod discovery;
pub mod input;
pub mod manifest;

pub use cache::InputCache;
pub use discovery::{
    find_lute_files, nearest_manifest_dir, normalize_span_from_text,
    parse_project_docs, project_root_for,
};
pub use input::{
    assemble_input, assemble_input_with_mode, build_input, build_input_with,
    build_input_with_mode, project_diag_line, read_document, BuiltInput,
};
pub use manifest::{manifest_context, resolve_snapshot, ManifestContext};
