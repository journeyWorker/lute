//! Orchestrator — the crate's public entry point (spec §9).
//!
//! Ingests parsed documents, resolves rules, computes tables, evaluates
//! every enabled rule, and returns a deterministic `Vec<(PathBuf,
//! Diagnostic)>` sorted by `(path, byte_start, code)`. No side effects.
//!
//! Callers (`lute lint` CLI, LSP) are responsible for parsing/collecting
//! documents and reading `lute.lint.yaml`; both then feed this the same
//! shape so behaviour is identical across surfaces.

use std::path::{Path, PathBuf};

use lute_core_span::{Diagnostic, Layer, RelatedDiagnostic, Span};
use lute_manifest::provider::ProviderSet;
use lute_syntax::ast::Document;

use crate::config::LintConfig;
use crate::metrics::{compute_doc_tables, compute_project_row, AttrRow, DocTables};
use crate::model::{LintRuleDecl, LintTarget};
use crate::nodes::NodeWalkStats;
use crate::rules::{
    active_group_bys, evaluate_node_rules, evaluate_rule, resolve_rules, DocContext, ResolvedRule,
};

/// One parsed input document + the source text that produced it.
///
/// `path` is the CANONICAL display path used throughout diagnostics — the
/// caller normalizes (e.g. project-root-relative) before invoking. `doc` is
/// the [`lute_syntax::parse`] result (a parse-error document is legal —
/// lint runs over whatever AST syntax layer produced, matching the
/// `lute-check` policy) after the manifest's `chapters:` derivation
/// ([`lute_check::chapters::apply_chapters`]), so a scene a chain lists is
/// classified by the `on:` the chain gives it ([`crate::metrics::doc_kind`]).
#[derive(Debug)]
pub struct LintDocInput {
    pub path: PathBuf,
    pub doc: Document,
    /// The source text that produced `doc`: `node` rows read each node's
    /// authored attributes from it (dsl 0.50.0 §8.1), so it MUST be the
    /// exact text `doc` was parsed from (the LSP passes its buffer).
    pub text: String,
}

/// The engine's output.
///
/// - `diagnostics`: `(path, diag)` pairs sorted by `(path, byte_start,
///   code)` — spec §Engine "Deterministic ordering".
/// - `config_diagnostics`: `E-LINT-CONFIG` items, spec §3 — anchored to
///   `config_path` when provided (else to the first input document).
///   Kept separate so a caller may treat them differently (they never
///   originate from a document text).
/// - `node_stats`: the work of the node walks (`node` and `pair` rules),
///   summed over documents (all zero when neither kind ran).
#[derive(Default, Debug)]
pub struct LintOutcome {
    pub diagnostics: Vec<(PathBuf, Diagnostic)>,
    pub config_diagnostics: Vec<(PathBuf, Diagnostic)>,
    pub node_stats: NodeWalkStats,
}

/// Which rule targets a run evaluates (spec §2).
///
/// - `Full`: everything — the CLI's `lute lint`.
/// - `Document`: skips `project`-target rules. The LSP lints one open
///   document at a time, so project-wide aggregates (scene spread, custom
///   `project.*` assertions) would be computed over a single scene and
///   misfire; spec §2 makes them CLI-only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LintScope {
    Full,
    Document,
}

/// Full lint pass — everything the CLI and LSP call.
///
/// Steps:
/// 1. Apply `config.ignore` globs to filter `inputs` (project-root-relative
///    matching, spec §3).
/// 2. Resolve rules — core + plugin + custom — with `config` overrides.
/// 3. Compute per-document tables and project-wide row (spec §4).
/// 4. Evaluate every enabled rule against every eligible document.
/// 5. Convert findings into anchored diagnostics (spec §8 span/related
///    conventions) and sort.
///
/// `config_path` is used to anchor `E-LINT-CONFIG` diagnostics; if `None`,
/// they attach to the first input document (or the empty path when
/// `inputs` itself is empty).
pub fn lint(
    inputs: &[LintDocInput],
    config: &LintConfig,
    plugin_rules: &[(String, LintRuleDecl)],
    providers: &ProviderSet,
    config_path: Option<&Path>,
    config_span: Span,
    scope: LintScope,
) -> LintOutcome {
    let mut outcome = LintOutcome::default();

    // Filter by ignore globs. Paths are matched as strings using
    // forward-slash normalization; callers should pass project-root-
    // relative paths but a naive absolute path still matches globs
    // matching the exact string.
    let active: Vec<&LintDocInput> = inputs
        .iter()
        .filter(|i| !ignored(&i.path, &config.ignore))
        .collect();

    // Resolve rules once for the whole run (spec §6).
    let (rules, config_diags) = resolve_rules(config, plugin_rules, config_span);
    // Attach config diagnostics.
    let cfg_anchor = config_path
        .map(PathBuf::from)
        .or_else(|| active.first().map(|i| i.path.clone()))
        .unwrap_or_default();
    for d in config_diags {
        outcome.config_diagnostics.push((cfg_anchor.clone(), d));
    }

    if rules.is_empty() || active.is_empty() {
        outcome.diagnostics.sort_by(diag_order);
        return outcome;
    }

    let group_bys = active_group_bys(&rules);

    // Compute per-doc tables and attribute rows.
    let per_doc: Vec<DocEntry<'_>> = active
        .iter()
        .map(|&input| {
            let (tables, attrs) = compute_doc_tables(&input.doc, &group_bys);
            DocEntry {
                input,
                tables,
                attrs,
            }
        })
        .collect();
    // Project row: aggregate over per-doc SceneRow.words — LINEAR scene
    // documents only (`SceneRow.kind == "scene"`, [`crate::metrics::doc_kind`]).
    // A component, quest, or lore document is not a scene, and a beat (a
    // scene answering an occasion, dsl 0.21.0 §3) is a short overlay: folding
    // a 10-word bark into the spread would report a meaningless min against a
    // full episode (dsl 0.22.0 §13).
    let scene_mask: Vec<bool> = per_doc
        .iter()
        .map(|d| d.tables.scene.as_ref().is_some_and(|s| s.kind == "scene"))
        .collect();
    let scene_words: Vec<u32> = per_doc
        .iter()
        .zip(scene_mask.iter())
        .filter(|(_, &is_scene)| is_scene)
        .map(|(d, _)| d.tables.scene.as_ref().map_or(0, |s| s.words))
        .collect();
    let project = compute_project_row(&scene_words);

    // `node` and `pair` rules: every one in a single walk of each
    // document's nodes (dsl 0.50.0 §8.1, 0.51.0 §6.2), below.
    let walks = |r: &&ResolvedRule| matches!(r.target, LintTarget::Node | LintTarget::Pair);
    let node_rules: Vec<&ResolvedRule> = rules.iter().filter(walks).collect();

    // Track project-target rules: a finding is emitted on the FIRST
    // contributing SCENE document (a component/quest never anchors a
    // scene-aggregate finding), with RelatedDiagnostic entries pointing at
    // each subsequent contributing scene (spec §8).
    for rule in &rules {
        if walks(&rule) {
            continue;
        }
        if rule.target == LintTarget::Project {
            if scope == LintScope::Document {
                continue;
            }
            let anchor_idx = scene_mask.iter().position(|&s| s).unwrap_or(0);
            let first = &per_doc[anchor_idx];
            let ctx = first.context(providers);
            let decl_span = decl_span(rule, config_path.unwrap_or(&cfg_anchor), config_span);
            let findings = evaluate_rule(rule, &ctx, &project, decl_span);
            for mut f in findings {
                // Attach related for every subsequent document.
                for (idx, entry) in per_doc.iter().enumerate() {
                    if idx == anchor_idx || !scene_mask[idx] {
                        continue;
                    }
                    if let Some(scene) = &entry.tables.scene {
                        f.related.push(RelatedDiagnostic {
                            file: entry.input.path.display().to_string(),
                            diagnostic: Diagnostic {
                                code: f.code.clone(),
                                severity: f.severity,
                                message: format!("contributes to project rule `{}`", rule.id),
                                evidence: None,
                                span: scene.span,
                                layer: f.layer.unwrap_or(Layer::Content),
                                fixits: Vec::new(),
                                provenance: None,
                                covered: Vec::new(),
                                related: Vec::new(),
                            },
                        });
                    }
                }
                outcome
                    .diagnostics
                    .push((first.input.path.clone(), finding_to_diagnostic(f)));
            }
        } else {
            for doc in &per_doc {
                let ctx = doc.context(providers);
                let decl_span = decl_span(rule, config_path.unwrap_or(&cfg_anchor), config_span);
                let findings = evaluate_rule(rule, &ctx, &project, decl_span);
                for f in findings {
                    outcome
                        .diagnostics
                        .push((doc.input.path.clone(), finding_to_diagnostic(f)));
                }
            }
        }
    }
    if !node_rules.is_empty() {
        for doc in &per_doc {
            let (findings, stats) =
                evaluate_node_rules(&node_rules, &doc.context(providers), config_span);
            outcome.node_stats.add(&stats);
            for f in findings {
                outcome
                    .diagnostics
                    .push((doc.input.path.clone(), finding_to_diagnostic(f)));
            }
        }
    }

    outcome.diagnostics.sort_by(diag_order);
    outcome.config_diagnostics.sort_by(diag_order);
    outcome
}

/// One linted document with its tables.
struct DocEntry<'i> {
    input: &'i LintDocInput,
    tables: DocTables,
    attrs: Vec<AttrRow>,
}

impl DocEntry<'_> {
    fn context<'a>(&'a self, providers: &'a ProviderSet) -> DocContext<'a> {
        DocContext {
            path: &self.input.path,
            doc: &self.input.doc,
            source: &self.input.text,
            tables: &self.tables,
            attrs: &self.attrs,
            providers,
        }
    }
}

fn finding_to_diagnostic(f: crate::rules::Finding) -> Diagnostic {
    Diagnostic {
        code: f.code,
        severity: f.severity,
        message: f.message,
        evidence: None,
        span: f.span,
        layer: f.layer.unwrap_or(Layer::Content),
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: f.related,
    }
}

/// Where a rule's declaration lives — used to anchor `E-LINT-EXPR` (spec
/// §5). Core rules point at the config file (their declaration is
/// embedded in this crate, not a file the user edits); a plugin rule
/// would point at its plugin.yaml but that path isn't threaded through
/// this crate yet, so it also falls back to the config file to keep the
/// anchor predictable.
fn decl_span(_rule: &crate::rules::ResolvedRule, _config_path: &Path, config_span: Span) -> Span {
    config_span
}

fn diag_order(a: &(PathBuf, Diagnostic), b: &(PathBuf, Diagnostic)) -> std::cmp::Ordering {
    a.0.cmp(&b.0)
        .then(a.1.span.byte_start.cmp(&b.1.span.byte_start))
        .then(a.1.code.cmp(&b.1.code))
}

fn ignored(path: &Path, patterns: &[String]) -> bool {
    if patterns.is_empty() {
        return false;
    }
    // Normalize to forward slashes for glob matching (the matcher in
    // `lute_manifest::glob` treats `/` as the segment separator).
    let s = path.to_string_lossy().replace('\\', "/");
    lute_manifest::glob::matches_any(patterns, &s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lute_syntax::parse;

    fn parse_input(path: &str, text: &str) -> LintDocInput {
        let (doc, _diags) = parse(text);
        LintDocInput {
            path: PathBuf::from(path),
            doc,
            text: text.to_string(),
        }
    }

    fn empty_span() -> Span {
        Span {
            byte_start: 0,
            byte_end: 0,
            line: 1,
            column: 1,
            utf16_range: (0, 0),
        }
    }

    #[test]
    fn ignore_glob_filters_docs() {
        let doc = parse_input(
            "drafts/prologue.lute",
            "---\n---\n## intro\n@alice: hello\n",
        );
        let cfg = LintConfig {
            ignore: vec!["drafts/**".into()],
            ..Default::default()
        };
        let out = lint(
            &[doc],
            &cfg,
            &[],
            &ProviderSet::default(),
            None,
            empty_span(),
            LintScope::Full,
        );
        assert!(out.diagnostics.is_empty());
    }
}
