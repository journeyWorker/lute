//! The reconciled single-root project analysis behind the `--project`
//! gate of `compile`/`trace`/`test`/`play` (connectivity design spec §5).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lute_core_span::{Diagnostic, Severity, Span};

use crate::cmd_scenario::assemble_root_scenario;
use crate::cmd_scenario::node_envelope::node_cycle_degraded;
use crate::project::reconcile::reconcile_collected;
use crate::project::{collect_project_docs, normalize_span_from_text};

/// The reconciled project analysis for the compile/trace project-aware gate
/// (connectivity design spec §5): per-document reconciled `CheckResult`s
/// (keyed by display path, [`BTreeMap`]-sorted for determinism) plus the
/// project-wide diagnostics. Produced by [`reconciled_project_results`] over a
/// SINGLE-ROOT collection (the whole `--project <dir>` is ONE root — §5's
/// single-root rule), reusing the SAME [`reconcile_collected`] analysis
pub(crate) struct ReconciledProject {
    pub(crate) per_doc: BTreeMap<PathBuf, lute_check::CheckResult>,
    pub(crate) project_diagnostics: Vec<(PathBuf, Diagnostic)>,
    /// Spec §5 gate signal: every node that is ON or DOWNSTREAM of a
    /// prerequisite cycle — absent from the sound topological order while its
    /// root has a cycle (`node_cycle_degraded`). A target hosting any of these
    /// blocks, even when the emitted `E-CONN-CYCLE` was anchored to a DIFFERENT
    /// node's file (see [`project_gate_result`]). Complete by Kahn's
    /// construction — no under-approximation of overlapping cycles.
    pub(crate) cycle_degraded: BTreeSet<lute_check::connectivity::NodeId>,
    /// Every graph node's `(id, span)` keyed by its declaring document — the
    /// gate's target-path -> hosted-nodes lookup.
    pub(crate) nodes_by_path: BTreeMap<PathBuf, Vec<(lute_check::connectivity::NodeId, Span)>>,
}

impl ReconciledProject {
    /// The spec §5 gate verdict of `file` ([`gate_for_doc`]), matched in
    /// the project by canonical identity; `None` when `file` is not one of
    /// its documents. The one envelope `lute trace --project`, `lute test`
    /// and the differential harness gate a document on.
    pub(crate) fn gate(&self, file: &Path) -> Option<lute_check::CheckResult> {
        let canon = std::fs::canonicalize(file).ok()?;
        let (key, base) = self
            .per_doc
            .iter()
            .find(|(path, _)| std::fs::canonicalize(path).is_ok_and(|c| c == canon))?;
        Some(gate_for_doc(self, key, base))
    }

    /// Round-5 G-17: every error whose cause lies in a schema or plugin
    /// file (not a walked document), folded as `check-project` folds it
    /// ([`crate::project::reconcile::relocate_imported_diags`]) — one line
    /// at the fault's own position counting its importers, instead of one
    /// copy per importing document. `dir` is the project directory the
    /// lines are shown under.
    pub(crate) fn schema_faults(&self, dir: &Path) -> Vec<String> {
        let mut per_doc: Vec<(PathBuf, lute_check::CheckResult)> = self
            .per_doc
            .iter()
            .map(|(p, r)| (p.clone(), r.clone()))
            .collect();
        let mut moved = Vec::new();
        crate::project::reconcile::relocate_imported_diags(&mut per_doc, &mut moved, dir);
        moved
            .into_iter()
            .filter(|(_, d)| d.severity == Severity::Error)
            .map(|(path, d)| {
                format!(
                    "{}:{}:{}: error [{}] {}",
                    path.display(),
                    d.span.line,
                    d.span.column,
                    d.code,
                    d.text()
                )
            })
            .collect()
    }
}

/// Collect + reconcile every `.lute` under `dir`, treating `dir` itself as THE
/// single project root for every file (connectivity spec §5: `--project <dir>`
/// resolves BOTH capabilities and connectivity against exactly that `<dir>`,
/// `load_project(dir)`, no nested nearest-root search — that directory-walk
/// discovery is `check-project`'s alone). The reusable seam the compile/trace
/// gate pulls the target document's reconciled `CheckResult` from.
/// `Err(ExitCode::from(2))` on the same I/O failures [`collect_project_docs`]
/// surfaces.
pub(crate) fn reconciled_project_results(
    dir: &Path,
    providers: Option<&Path>,
) -> Result<ReconciledProject, ExitCode> {
    let (file_results, by_root) = collect_project_docs(dir, providers, true)?;
    // Spec §5 gate: a target blocks when a node it hosts is ON or DOWNSTREAM
    // of a prerequisite cycle — i.e. absent from the sound topological order
    // while its root has a cycle. Decided by `node_cycle_degraded` over the
    // SAME `assemble_root_scenario` analysis `lute scenario` reports from, so a
    // target's gate verdict never disagrees with its `scenario reach` view.
    // This topological-order exclusion is COMPLETE (Kahn frees exactly the
    // cycle-INDEPENDENT nodes), where a DFS back-edge stack slice
    // under-approximates overlapping cycles. Built BEFORE `reconcile_collected`
    // consumes `file_results`.
    let mut cycle_degraded: BTreeSet<lute_check::connectivity::NodeId> = BTreeSet::new();
    for group_full in by_root.values() {
        let scenario = assemble_root_scenario(group_full, &file_results);
        for node in scenario.graph.nodes.keys() {
            if node_cycle_degraded(&scenario, node) {
                cycle_degraded.insert(node.clone());
            }
        }
    }
    let (file_results, project_diagnostics, nodes_by_path, _) =
        reconcile_collected(file_results, &by_root, false);
    Ok(ReconciledProject {
        per_doc: file_results.into_iter().collect(),
        project_diagnostics,
        cycle_degraded,
        nodes_by_path,
    })
}

/// The project-aware gate verdict for one `file` compiled/traced under
/// `--project <dir>` (connectivity design spec §5). Runs the SINGLE-ROOT
/// project reconciliation ([`reconciled_project_results`]) and returns the
/// TARGET document's own reconciled `CheckResult` MERGED with every
/// project-wide diagnostic anchored on that same file — so an
/// `E-STATE-MAYBE-UNAVAILABLE`/`E-CONN-*` fault on the target's OWN
/// `after`/reads blocks it, while a SIBLING document's project-only fault does
/// NOT (§5). Its `ok` is recomputed over the merged set.
///
/// **Out-of-tree (normative, §5):** if the canonicalized `file` is NOT within
/// `dir`'s recursively-collected `.lute` set, this errors EXPLICITLY
/// (`ExitCode::from(2)`) rather than silently falling back to a standalone
/// `check` — a silent fallback would mask a mistyped path or wrong `--project`.
pub(crate) fn project_gate_result(
    file: &Path,
    dir: &Path,
    providers: Option<&Path>,
) -> Result<lute_check::CheckResult, ExitCode> {
    let reconciled = reconciled_project_results(dir, providers)?;
    if let Err(e) = std::fs::canonicalize(file) {
        eprintln!("lute: cannot read {}: {e}", file.display());
        return Err(ExitCode::from(2));
    }
    reconciled.gate(file).ok_or_else(|| {
        eprintln!(
            "lute: {} is not within --project {} (the connectivity gate requires the target to be part of the project)",
            file.display(),
            dir.display()
        );
        ExitCode::from(2)
    })
}

/// The spec §5 gate verdict for ONE already-reconciled document: its own
/// reconciled `CheckResult`, MERGED with every project-wide diagnostic anchored
/// on that same file — so an `E-STATE-MAYBE-UNAVAILABLE`/`E-CONN-*` fault on
/// this document's OWN `after`/reads blocks it, while a SIBLING's project-only
/// fault does not. `ok` is recomputed over the merged set.
///
/// Split out of [`project_gate_result`] so `compile --all`
/// ([`compile_all::run`]) can gate EVERY document off ONE project
/// reconciliation instead of re-running the whole single-root collection once
/// per file (which would be quadratic in project size, and could in principle
/// observe a project mid-edit differently between passes).
///
/// [`compile_all::run`]: crate::compile_all::run
pub(crate) fn gate_for_doc(
    reconciled: &ReconciledProject,
    path: &PathBuf,
    base: &lute_check::CheckResult,
) -> lute_check::CheckResult {
    let mut result = base.clone();
    // §5: block on the TARGET's own reconciled diagnostics only — merge in
    // every project-wide diagnostic anchored on this same file (its own
    // `E-STATE-MAYBE-UNAVAILABLE`/`E-CONN-*`), never a sibling's.
    for (p, d) in &reconciled.project_diagnostics {
        if p == path {
            result.diagnostics.push(d.clone());
        }
    }
    // Spec §5 (topological-order exclusion): a target that HOSTS a node ON or
    // DOWNSTREAM of an `after`-precedence cycle must block even when the ONE
    // emitted `E-CONN-CYCLE` was anchored to a DIFFERENT node's file — the
    // target may be a non-anchored cyclic/downstream node with no own read
    // fault, which the merge above would let through. Synthesize a
    // TARGET-anchored `E-CONN-CYCLE` (reusing `lute_check`'s own
    // constructor/code) so `ok` recomputes false. Guarded on `E_CONN_CYCLE` NOT
    // already present so an already-anchored node (or one already carrying a
    // retained `E-MAYBE-UNSET`) never double-reports the cycle. Membership is
    // `cycle_degraded` — absence from the sound topological order given a cycle
    // (`node_cycle_degraded`), COMPLETE (no under-approximation of overlapping
    // cycles) and, per spec §5, covering BOTH on-cycle and strictly-downstream
    // targets.
    let already_cyclic = result
        .diagnostics
        .iter()
        .any(|d| d.code == lute_check::connectivity::E_CONN_CYCLE);
    if !already_cyclic {
        if let Some((id, span)) = reconciled.nodes_by_path.get(path).and_then(|hosted| {
            hosted
                .iter()
                .find(|(id, _)| reconciled.cycle_degraded.contains(id))
        }) {
            // Normalize line/col from the target's own text (mirrors
            // `reconcile_collected`'s project-diag normalization) — the raw
            // `character:`-key node span carries zeroed line/col otherwise.
            let text = std::fs::read_to_string(path).unwrap_or_default();
            let span = normalize_span_from_text(&text, *span);
            result
                .diagnostics
                .push(lute_check::connectivity::cycle_diag(
                    format!(
                    "prerequisite cycle: this document hosts `{id}`, which is on or downstream \
                     of an `after`-precedence cycle (E-CONN-CYCLE); no evaluation order can \
                     satisfy every `after` (dsl §2.4/§4.1 §A)"
                ),
                    span,
                ));
        }
    }
    result.ok = !result
        .diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error);
    result
}
