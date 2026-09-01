//! # lute-lint — content/metric advisory lints for Lute scenarios.
//!
//! Distinct from [`lute_check`]'s semantic validation, `lute-lint` runs
//! configurable, plugin-extensible advisories over content and metric tables
//! computed once from `lute_syntax::ast` documents. The engine is dogfooded:
//! every core, plugin, and project-local rule shares one shape (id + target +
//! CEL `when` + message) and one evaluator.
//!
//! See `docs/superpowers/specs/2026-08-26-lute-lint-system-design.md` for the
//! authoritative design (§3 config, §4 metric tables, §5 CEL fragment, §6 rule
//! model, §7 core rules & defaults, §8 diagnostics).
//!
//! ## Public surface
//! - [`engine::lint`] — orchestrator: takes parsed documents, resolved config,
//!   plugin rules, and provider snapshots; returns [`engine::LintOutcome`]
//!   (a deterministic `Vec<(PathBuf, Diagnostic)>`).
//! - [`config::LintConfig`] / [`config::parse_config`] — YAML config loader.
//! - [`model::LintRuleDecl`] and friends — declarative rule shape shared with
//!   plugin YAML and `custom:`; re-exported from `lute_manifest::lint` (the
//!   authoritative definitions).
//! - [`rules`] — the built-in rule registry (embedded YAML for data rules;
//!   Rust for `emotion-distribution` / `variant-composition` / `asset-exists`).
//!
//! Diagnostics are emitted with codes:
//! - `L-<UPPER-KEBAB>` for rule findings (spec §8).
//! - `E-LINT-CONFIG` for config-semantic errors (unknown id, bad option, …).
//! - `E-LINT-EXPR` for an unresolvable/mistyped `when` (rule skipped).
//! - `E-LINT-RULE` for a malformed plugin/custom rule declaration.
//!
//! No lint diagnostic ever enters `CapabilitySnapshot`'s hash — lints are
//! advisory and must never change artifact identity (spec §1).

pub mod config;
pub mod engine;
pub mod eval;
pub mod glob;
pub mod metrics;
pub mod model;
pub mod rules;

pub use config::{parse_config, LintConfig, RuleOverride};
pub use engine::{lint, LintDocInput, LintOutcome, LintScope};
pub use model::{LintLevel, LintRuleDecl, LintTarget};
