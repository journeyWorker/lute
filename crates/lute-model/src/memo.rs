//! The project models one command builds, each once.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use lute_check::Memo;

use crate::project::{ModelError, ModelOptions, ProjectModel};

/// A model build's outcome, shared by every consumer of one memo. The error
/// is shared too, so each consumer reports a failed build as it would have.
pub type Built = Result<Arc<ProjectModel>, Arc<ModelError>>;

/// Every [`ProjectModel`] one command builds, once per root and options.
///
/// A command that consults its project several ways builds the same model
/// once per consumer without it: `lute test` read one project's tests'
/// analysis, gate verdicts, quest ids, producer set and play compile from
/// five separate builds. The memo assumes the project's files do not change
/// while it lives, so a command that writes and then re-reads (`lute patch`)
/// must not share one across the write. Roots are keyed exactly as given:
/// a model spells its documents' paths from its root.
#[derive(Default)]
pub struct ModelMemo {
    built: Memo<(PathBuf, ModelOptions), Built>,
}

impl ModelMemo {
    /// [`ProjectModel::build_single_root`], once per `(root, opts)`.
    pub fn single_root(&self, root: &Path, opts: &ModelOptions) -> Built {
        let built = self.built.get_or_init((root.to_path_buf(), opts.clone()), || {
            ProjectModel::build_single_root(root, opts)
                .map(Arc::new)
                .map_err(Arc::new)
        });
        (*built).clone()
    }

    /// [`ProjectModel::roots_under`], each root's model built once per
    /// `opts`. Like it, the first root that fails to build is the error.
    pub fn roots_under(&self, dir: &Path, opts: &ModelOptions) -> Result<Vec<Arc<ProjectModel>>, Arc<ModelError>> {
        ProjectModel::project_roots(dir)
            .map_err(Arc::new)?
            .iter()
            .map(|root| self.single_root(root, opts))
            .collect()
    }
}
