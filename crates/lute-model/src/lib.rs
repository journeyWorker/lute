//! Authoritative source/project semantic model.
//!
//! This crate owns input assembly and the one per-root document snapshot used
//! by project-oriented consumers. It deliberately has no dependency on the
//! CLI, LSP, or trace crates.

pub mod cache;
pub mod input;
pub mod manifest;
pub mod project;
pub mod reconcile;
pub mod scenario;
pub mod gate;

pub use cache::InputCache;
pub use input::{
    assemble_input, assemble_input_with_mode, build_input, build_input_with,
    build_input_with_mode, project_diag_line, read_document, BuiltInput,
};
pub use project::{
    discover_project, find_lute_files, nearest_manifest_dir, normalize_span_from_text,
    parse_project_docs, project_root_for, ByRoot, DocGroup, ModelDocument, ModelError,
    ModelOptions, ProjectModel, ReconciledOutputs,
};
pub use manifest::{manifest_context, resolve_snapshot, ManifestContext};
pub use reconcile::{
    compute_conn_fixpoint, reconcile_collected, relocate_imported_diags,
    rollup_component_body_diags,
};
pub use scenario::{assemble_root_scenario, node_cycle_degraded, RootScenario};
pub use gate::{
    gate_for_doc, project_gate_result, reconciled_project_results, ReconciledProject,
};
