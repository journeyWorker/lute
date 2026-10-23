//! The project models one command builds, each once.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
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
///
/// A [`Self::selecting`] memo goes further (spec 0.49.0 §9.6): it serves
/// every request for a root from the one model its command selected, so a
/// witness-bearing command's analysis, plays and tests all read one build.
#[derive(Default)]
pub struct ModelMemo {
    built: Memo<(PathBuf, ModelOptions), Built>,
    /// The options every model of a selecting memo is built with.
    selected: Option<ModelOptions>,
    /// Underlying [`ProjectModel::build_single_root`] calls.
    builds: AtomicUsize,
}

impl std::fmt::Debug for ModelMemo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModelMemo")
            .field("selected", &self.selected)
            .field("builds", &self.builds())
            .finish_non_exhaustive()
    }
}

impl ModelMemo {
    /// A memo that builds each root once, with `opts`, whatever options a
    /// consumer asks for: a request is matched by the canonical identity of
    /// its root, and its providers directory and permission profile (the
    /// options that change what the project resolves to). The model is
    /// spelled from the first request's root.
    ///
    /// A consumer that asked for `compile: false` reads the model's
    /// analysis-only results ([`crate::ModelDocument::analysis_check`],
    /// [`ProjectModel::analysis_checks`],
    /// [`ProjectModel::analysis_diagnostics`]), which are what a
    /// `compile: false` model reports. The request's `mode` is not honoured:
    /// a selecting command runs under `opts.mode`, and its checks were shown
    /// not to branch on it for what they read (spec 0.49.0 §9.6).
    pub fn selecting(opts: ModelOptions) -> Self {
        Self {
            selected: Some(opts),
            ..Self::default()
        }
    }

    /// How many models this memo built ([`ProjectModel::build_single_root`]
    /// calls, failed ones included).
    pub fn builds(&self) -> usize {
        self.builds.load(Ordering::Relaxed)
    }

    /// [`ProjectModel::build_single_root`], once per `(root, opts)` — per
    /// selected root and project options on a [`Self::selecting`] memo.
    pub fn single_root(&self, root: &Path, opts: &ModelOptions) -> Built {
        let (key, build_opts) = match &self.selected {
            Some(selected) => {
                let opts = ModelOptions {
                    providers: opts.providers.clone(),
                    permission_profile: opts.permission_profile.clone(),
                    ..selected.clone()
                };
                let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
                ((root, opts.clone()), opts)
            }
            None => ((root.to_path_buf(), opts.clone()), opts.clone()),
        };
        let built = self.built.get_or_init(key, || {
            self.builds.fetch_add(1, Ordering::Relaxed);
            ProjectModel::build_single_root(root, &build_opts)
                .map(Arc::new)
                .map_err(Arc::new)
        });
        (*built).clone()
    }

    /// [`ProjectModel::roots_under`], each root's model built once per
    /// `opts`. Like it, the first root that fails to build is the error.
    pub fn roots_under(
        &self,
        dir: &Path,
        opts: &ModelOptions,
    ) -> Result<Vec<Arc<ProjectModel>>, Arc<ModelError>> {
        ProjectModel::project_roots(dir)
            .map_err(Arc::new)?
            .iter()
            .map(|root| self.single_root(root, opts))
            .collect()
    }
}
