//! Process-wide counts of the project-wide passes: what
//! `LUTE_STATS=project` prints (tooling only, spec 0.53.0 §3.7). A command
//! reconciles each project root once and assembles each root's scenario
//! once; a consumer reads the model's results instead of recomputing them.

use std::sync::atomic::{AtomicUsize, Ordering};

static RECONCILIATIONS: AtomicUsize = AtomicUsize::new(0);
static SCENARIOS: AtomicUsize = AtomicUsize::new(0);
static FIXPOINTS: AtomicUsize = AtomicUsize::new(0);

/// The project-wide passes this process ran so far.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProjectWorkStats {
    /// [`crate::reconcile_collected`] calls.
    pub reconciliations: usize,
    /// [`crate::assemble_root_scenario`] calls.
    pub scenarios: usize,
    /// [`crate::compute_conn_fixpoint`] calls (a reconciliation and a
    /// scenario assembly each run one per root).
    pub fixpoints: usize,
}

impl std::fmt::Display for ProjectWorkStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "reconciliations={} scenarios={} fixpoints={}",
            self.reconciliations, self.scenarios, self.fixpoints
        )
    }
}

/// The project-wide passes this process ran so far.
pub fn project_work_stats() -> ProjectWorkStats {
    ProjectWorkStats {
        reconciliations: RECONCILIATIONS.load(Ordering::Relaxed),
        scenarios: SCENARIOS.load(Ordering::Relaxed),
        fixpoints: FIXPOINTS.load(Ordering::Relaxed),
    }
}

pub(crate) fn count_reconciliation() {
    RECONCILIATIONS.fetch_add(1, Ordering::Relaxed);
}

pub(crate) fn count_fixpoint() {
    FIXPOINTS.fetch_add(1, Ordering::Relaxed);
}

pub(crate) fn count_scenario() {
    SCENARIOS.fetch_add(1, Ordering::Relaxed);
}
