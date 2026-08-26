//! Opt-in lint diagnostics for open documents (lint-system design §2, §3).
//!
//! Discovery is per analysis pass: `<project root>/lute.lint.yaml` is
//! re-`stat`/re-read from disk each call. The LSP has no watched-files channel
//! today — [`Backend::snapshot_for`](crate::backend::Backend::snapshot_for)
//! already re-`load_project`s per analysis for the same reason (a config file
//! change is picked up on the next `didChange`/`didOpen` without any watch
//! plumbing). `lute.lint.yaml` piggybacks that policy: it is a small file,
//! parsing it per analysis is cheap, and it stays consistent with how every
//! other project-derived state is invalidated in this crate.
//!
//! Gating (spec §2):
//! - absent config file, malformed YAML, or `lsp: false` ⇒ zero lint
//!   diagnostics, zero errors. Silent no-op. Malformed-config reporting is the
//!   CLI's job (`lute lint` exit 2); the LSP has no natural cross-file publish
//!   channel to surface a diagnostic anchored at a file the editor never
//!   opened, so `LintOutcome::config_diagnostics` is deliberately dropped here.
//!
//! Scope: [`LintScope::Document`]. The engine already skips `project`-target
//! rules under that scope, so aggregates like `scene-length-spread` never
//! misfire over a single open buffer (spec §2 keeps them CLI-only).
//!
//! Plugin rules: resolved once for the project's DEFAULT profile and reused
//! for every document under it. Per-document profile activation is spec'd
//! (design §6, "For each document, active plugin rules = plugins resolved for
//! that document's profile") but v1-simplified here to the default profile —
//! the LSP already re-resolves the CAPABILITY snapshot per document, so
//! honoring per-document lint profiles too would double the resolver cost per
//! keystroke without a corresponding rule-set change for the overwhelming
//! majority of projects (one profile). Revisit if a multi-profile project
//! declares distinct lint sets per profile.

use std::path::{Path, PathBuf};

use lute_core_span::{Diagnostic, Span};
use lute_lint::{LintDocInput, LintRuleDecl, LintScope};
use lute_manifest::project::ProjectConfig;
use lute_manifest::provider::ProviderSet;
use lute_manifest::resolve::ActivationMap;
use lute_syntax::ast::Document;

/// Filename of the lint config, sibling to `lute.project.yaml`.
pub const CONFIG_FILENAME: &str = "lute.lint.yaml";

/// Compute lint diagnostics for `file_path`'s open document under
/// `project_root` — the opt-in editor surface (spec §2). Returns an empty
/// vector on every gating no-op (no config file, malformed YAML, `lsp:
/// false`), so the caller can unconditionally extend its published
/// diagnostics with the result. Never panics.
///
/// `project` is the already-loaded manifest ([`Backend::snapshot_for`] loaded
/// it for the capability snapshot; we reuse it rather than re-`load_project`
/// per call). `providers` is the same pinned catalog `check()` sees for this
/// project, so `asset-exists`-style rules resolve id status against exactly
/// the snapshot the semantic layer used.
///
/// `doc` and `text` are the parse/text pair `analyze()` already computed for
/// `check()`; passed in rather than re-parsed to keep the analyze pipeline
/// single-parse.
pub fn lint_document(
    file_path: &Path,
    project_root: &Path,
    project: Option<&ProjectConfig>,
    providers: &ProviderSet,
    doc: &Document,
    text: &str,
) -> Vec<Diagnostic> {
    let config_path = project_root.join(CONFIG_FILENAME);
    let Ok(yaml) = std::fs::read_to_string(&config_path) else {
        // No config file (or unreadable): lint is opt-in, silent no-op.
        return Vec::new();
    };
    let config_span = Span {
        byte_start: 0,
        byte_end: yaml.len(),
        line: 1,
        column: 1,
        utf16_range: (0, 0),
    };
    // Malformed YAML: the CLI (`lute lint`) is the authoritative reporter
    // for config errors (spec §3, exit 2). Silently no-op in the editor
    // rather than mis-anchor an `E-LINT-CONFIG` on the OPEN document (the
    // config file is not the open buffer here).
    let Ok((config, _semantic_config_diags)) = lute_lint::parse_config(&yaml, config_span) else {
        return Vec::new();
    };
    if !config.lsp {
        return Vec::new();
    }

    // Plugin rules: default-profile activation for this project, once.
    let plugin_rules = default_profile_lint_rules(project);

    // `LintDocInput.path` is the canonical display path in diagnostics
    // (spec §Diagnostics). Normalize to project-root-relative so a
    // published lint diagnostic reads the same as `lute lint`'s output.
    let rel_path: PathBuf = file_path
        .strip_prefix(project_root)
        .map(PathBuf::from)
        .unwrap_or_else(|_| file_path.to_path_buf());
    let input = LintDocInput {
        path: rel_path,
        doc: doc.clone(),
        text: text.to_string(),
    };

    let outcome = lute_lint::lint(
        std::slice::from_ref(&input),
        &config,
        &plugin_rules,
        providers,
        Some(&config_path),
        config_span,
        LintScope::Document,
    );
    // Drop `config_diagnostics`: no cross-file publish channel — see the
    // module docs.
    outcome.diagnostics.into_iter().map(|(_p, d)| d).collect()
}

/// Namespaced lint rules for the project's DEFAULT profile activation, or
/// an empty vec when no project (loose scene) or the activation fails. A
/// load/resolve failure is silently swallowed: the semantic layer
/// (`Backend::snapshot_for`) already surfaces those diagnostics through
/// `ResolveDiag`; duplicating them here would be redundant noise and lint
/// is advisory anyway (spec §1).
fn default_profile_lint_rules(project: Option<&ProjectConfig>) -> Vec<(String, LintRuleDecl)> {
    let Some(project) = project else {
        return Vec::new();
    };
    let (installed, _load_errs) = lute_manifest::loader::load_plugins_dir(&project.plugins_dir);
    let default_profile = project.graph.default_profile.as_str();
    let scene_local = ActivationMap::new();
    match lute_manifest::resolve::resolve_activation(
        &project.graph,
        default_profile,
        &scene_local,
        &installed,
    ) {
        Ok(active) => lute_manifest::lint::namespace_active_lints(&active, &installed),
        Err(_) => Vec::new(),
    }
}
