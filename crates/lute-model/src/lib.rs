//! Authoritative project semantic model.
//!
//! This crate owns the checked, compiled, and analyzed project snapshot. Source
//! discovery and input assembly live in `lute-load`.


pub mod project;
pub mod revision;
pub mod diff;
pub mod reconcile;
pub mod scenario;
pub mod gate;
pub mod constraints;
pub mod graph;
pub mod derivation;
pub mod patch;
pub mod impact;
pub mod rename;
pub mod memo;

pub use project::{
    ByRoot, DocGroup, ModelDocument, ModelError, ModelOptions, ProjectModel, ReconciledOutputs,
};
pub use reconcile::{
    compute_conn_fixpoint, reconcile_collected, relocate_imported_diags,
    rollup_component_body_diags,
};
pub use scenario::{assemble_root_scenario, node_cycle_degraded, project_docs, RootScenario};
pub use gate::{
    gate_for_doc, project_gate_result, project_gate_result_in, reconciled_project_results,
    reconciled_project_results_in,
    ReconciledProject,
};

pub use memo::{Built, ModelMemo};
pub use rename::{resolve_ledger, ResolvedRenames};
pub use impact::{ImpactItem, ImpactReport, ImpactTarget};
pub use revision::{project_revision, FileRevision, ProjectRevision, RevisionError};
pub use diff::{diff_models, ChangeKind, DiffError, SemanticChange, SemanticDiff};
pub use patch::{apply_patch, apply_patch_to, PatchBase, PatchEdit, PatchRefusal, PatchReport, PatchRequest, Preserve};
