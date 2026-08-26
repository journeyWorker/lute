//! plugin §? / lint-system §6: the `lints` plugin export.
//!
//! One `LintRuleDecl` per rule; every plugin that publishes lints ships them
//! as `lints/*.yaml` files whose top key is `lints:` (see [`LintsFile`] in
//! [`crate::schema`]). Rules are stored on [`crate::loader::LoadedPlugin`]
//! with their RAW ids; a downstream consumer namespaces each rule as
//! `<plugin-id>/<id>` — the id shape the lint config (`lute.lint.yaml`)
//! addresses plugin rules by (see the approved lint-system design,
//! `docs/superpowers/specs/2026-08-26-lute-lint-system-design.md` §3 and
//! §6).
//!
//! Lints are ADVISORY. They are DELIBERATELY NOT folded into the
//! [`crate::snapshot::CapabilitySnapshot`] and never touch
//! `capabilityVersion` (design §1 non-goals, §6): a project changing its
//! lint set MUST NOT change artifact identity. The loader carries the
//! declarations through; assembly reads none of it.
//!
//! Parse-only. `when` and `message` are strings here; the lint engine in
//! `lute-lint` parses `when` via `lute_cel::parse_slot` and evaluates it
//! over metric tables (design §5). Load-time YAML failures for a lint file
//! surface through the loader's existing per-file
//! [`crate::loader::LoadError::Parse`] channel — the same anchor every
//! other export uses, and the load-time flavor of the design's `E-LINT-RULE`.

use serde::{Deserialize, Serialize};

/// One lint rule declared by a plugin's `lints/*.yaml` file — the loader's
/// on-disk shape for a plugin-published rule (design §6). Consumers
/// (lute-lint) resolve `level` against `lute.lint.yaml` overrides and
/// evaluate `when` over the metric row named by `target`.
///
/// Deserialization is deliberately permissive on `options` (an arbitrary
/// YAML mapping the rule's own logic interprets) and on `level` (a plugin
/// MAY omit it to let the consumer's config choose; the design's default
/// resolution table lives in `lute-lint`, not here).
///
/// Derives mirror [`crate::schema::AttrDecl`]'s `Clone/Debug/Serialize/
/// Deserialize` PLUS `PartialEq` — the batch contract asks for equality so
/// consumers (and tests) can compare rule sets without hand-rolling one.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LintRuleDecl {
    /// Rule id, raw as authored in the plugin's YAML. The namespaced form
    /// `<plugin-id>/<id>` (design §3) is produced by
    /// [`namespace_active_lints`] when the loader hands rules to a
    /// consumer, NEVER stored here — the same plugin package can be reused
    /// under a different id and this decl still applies.
    pub id: String,
    /// One of `line | shot | scene | speaker | group | project` — the
    /// metric table row `when` binds over (design §4, §6).
    pub target: LintTarget,
    /// A lint-CEL expression evaluated over the `target`'s row (design §5).
    /// Held as a string here; parsing is deferred to `lute-lint`, so a
    /// malformed CEL text is reported by the engine, not the loader.
    pub when: String,
    /// Default severity level. `None` means "no plugin-side default" — the
    /// consumer's config resolution (design §3) picks a level, falling
    /// back to whatever `lute-lint`'s registry declares for a rule with
    /// no author-declared default.
    #[serde(default)]
    pub level: Option<LintLevel>,
    /// Human-readable message template (design §5 "Message templates").
    /// `{path.to.field}` interpolation is applied by `lute-lint`; the raw
    /// text passes through here.
    pub message: String,
    /// Rule-specific option DEFAULTS. An opaque YAML mapping the rule's
    /// own logic interprets; the consumer's `lute.lint.yaml` deep-merges
    /// per-project overrides on top (design §3). Kept as
    /// [`serde_yaml::Mapping`] so the loader neither validates nor
    /// re-shapes the interior — `serde_yaml` is already the workspace's
    /// YAML crate, and using its ordered `Mapping` keeps declaration
    /// order stable for consumers that render options back.
    #[serde(default)]
    pub options: serde_yaml::Mapping,
}

/// Metric-table target a lint rule binds over (design §4). Serialized
/// lowercase to match the config file surface (`target: scene`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LintTarget {
    Line,
    Shot,
    Scene,
    Speaker,
    Group,
    Project,
}

/// Severity level a lint fires at (design §3). Serialized lowercase to
/// match `lute.lint.yaml`'s `{ level: warn }` shorthand. `Off` disables
/// the rule; the other four map onto `lute_core_span::Severity` in the
/// lint engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LintLevel {
    Off,
    Hint,
    Info,
    Warn,
    Error,
}

/// Given the resolved active plugins for a document/profile (as
/// [`crate::resolve::resolve_activation`] returns them) and the on-disk
/// [`crate::resolve::InstalledPlugins`] the loader produced, return every
/// active plugin's declared lint rule keyed by its namespaced id
/// `<plugin-id>/<id>` (design §3, §6).
///
/// Iteration order follows the caller's `active` order (which is
/// resolution order — plugin §11.1), so an outer consumer can render or
/// deduplicate deterministically. A rule from a plugin NOT present in
/// `installed` is skipped: activation validity is not this helper's
/// concern (assembly's `MissingActivePlugin` owns that).
///
/// The lint helper lives HERE rather than on `InstalledPlugins` /
/// `ActivePlugin` because it is the least-invasive seam: it needs both
/// halves (an activation's ordered id list PLUS the loaded packages) but
/// belongs to neither type's contract — the lint layer is advisory and
/// added on top of an already-frozen resolver surface. `ActivePlugin`
/// deliberately does NOT retain a `LoadedPlugin` reference (it is
/// consumed by assembly, which looks packages back up by id), so the
/// helper takes both arguments explicitly rather than growing that type.
pub fn namespace_active_lints(
    active: &[crate::resolve::ActivePlugin],
    installed: &crate::resolve::InstalledPlugins,
) -> Vec<(String, LintRuleDecl)> {
    let mut out = Vec::new();
    for ap in active {
        let Some(inst) = installed.get(&ap.id) else {
            continue;
        };
        for rule in &inst.loaded.lints {
            out.push((format!("{}/{}", ap.id, rule.id), rule.clone()));
        }
    }
    out
}
