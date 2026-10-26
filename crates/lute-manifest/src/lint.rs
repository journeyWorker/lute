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
//! `capabilitySnapshot` (design §1 non-goals, §6): a project changing its
//! lint set MUST NOT change artifact identity. The loader carries the
//! declarations through; assembly reads none of it.
//!
//! Parse-only. Predicates and `message` are strings here; the lint engine
//! in `lute-lint` parses them via `lute_cel::parse_slot` and evaluates them
//! over metric tables and node rows (design §5). A rule's body is
//! [`LintRuleBody`]: `when` for every target but `pair`, `open`/`close`
//! (and optional `inside`/`scope`) for `pair` (dsl 0.51.0 §6.1). The shape
//! is enforced while deserializing ([`LintRuleShapeError`]), so a plugin's
//! malformed rule surfaces through the loader's per-file
//! [`crate::loader::LoadError::Parse`] channel (`E-PLUGIN-PARSE`) and a
//! `lute.lint.yaml` `custom:` one as `E-LINT-CONFIG`.

use serde::{Deserialize, Serialize};

/// One lint rule declared by a plugin's `lints/*.yaml` file — the loader's
/// on-disk shape for a plugin-published rule (design §6). Consumers
/// (lute-lint) resolve `level` against `lute.lint.yaml` overrides and
/// evaluate the [`LintRuleBody`] over the rows named by `target`.
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
#[serde(try_from = "RawLintRuleDecl", into = "RawLintRuleDecl")]
pub struct LintRuleDecl {
    /// Rule id, raw as authored in the plugin's YAML. The namespaced form
    /// `<plugin-id>/<id>` (design §3) is produced by
    /// [`namespace_active_lints`] when the loader hands rules to a
    /// consumer, NEVER stored here — the same plugin package can be reused
    /// under a different id and this decl still applies.
    pub id: String,
    /// One of `line | section | scene | speaker | group | project | node |
    /// pair` — the metric table row the body binds over (design §4, §6;
    /// `node`: dsl 0.50.0 §8.1, one row per body node; `pair`: dsl 0.51.0
    /// §6, begin/end balance over node rows).
    pub target: LintTarget,
    /// The rule's predicates, held as strings: parsing is deferred to
    /// `lute-lint`, so a malformed CEL text is reported by the engine, not
    /// the loader. [`LintRuleDecl::check_shape`] holds it against `target`.
    pub body: LintRuleBody,
    /// Default severity level. `None` means "no plugin-side default" — the
    /// consumer's config resolution (design §3) picks a level, falling
    /// back to whatever `lute-lint`'s registry declares for a rule with
    /// no author-declared default.
    pub level: Option<LintLevel>,
    /// Human-readable message template (design §5 "Message templates").
    /// `{path.to.field}` interpolation is applied by `lute-lint`; the raw
    /// text passes through here.
    pub message: String,
    /// Rule-specific option DEFAULTS. An opaque YAML mapping the rule's
    /// own logic interprets; the consumer's `lute.lint.yaml` deep-merges
    /// per-project overrides on top (design §3). Kept as
    /// [`yaml_serde::Mapping`] so the loader neither validates nor
    /// re-shapes the interior — `yaml_serde` is already the workspace's
    /// YAML crate, and using its ordered `Mapping` keeps declaration
    /// order stable for consumers that render options back.
    pub options: yaml_serde::Mapping,
}

impl LintRuleDecl {
    /// Whether `body` fits `target`: a [`LintRuleBody::Pair`] for
    /// [`LintTarget::Pair`], a [`LintRuleBody::Predicate`] for every other
    /// target (dsl 0.51.0 §6.1). Deserialization already enforces it; a
    /// rule built in Rust is checked by the lint engine with this.
    pub fn check_shape(&self) -> Result<(), LintRuleShapeError> {
        let fault = match (&self.body, self.target) {
            (LintRuleBody::Predicate { .. }, LintTarget::Pair) => LintShapeFault::WhenOnPair,
            (LintRuleBody::Pair(_), target) if target != LintTarget::Pair => {
                LintShapeFault::PairKeyOffPair {
                    key: "open",
                    target,
                }
            }
            _ => return Ok(()),
        };
        Err(LintRuleShapeError {
            id: self.id.clone(),
            fault,
        })
    }
}

/// What a rule asserts (dsl 0.51.0 §6.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LintRuleBody {
    /// `when`: a lint-CEL expression evaluated over the `target`'s row
    /// (design §5); a finding per row it holds for.
    Predicate { when: String },
    /// A `pair` rule's predicates over node rows.
    Pair(PairSpec),
}

/// A `pair` rule (dsl 0.51.0 §6.1): `open`, `close` and the optional
/// `inside` are lint-CEL predicates over the `node` row; `scope` says which
/// rows one pass balances.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PairSpec {
    pub open: String,
    pub close: String,
    /// Every row strictly between an open and its close must satisfy it.
    pub inside: Option<String>,
    pub scope: PairScope,
}

/// Which rows one `pair` pass balances (dsl 0.51.0 §6.2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PairScope {
    /// Each body's direct children, in order.
    #[default]
    Body,
    /// Every row of one section or top-level declaration, in document
    /// order, at every depth.
    Section,
}

/// A rule whose keys do not fit its target (dsl 0.51.0 §6.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LintRuleShapeError {
    /// The rule's raw id.
    pub id: String,
    pub fault: LintShapeFault,
}

/// The shape faults of [`LintRuleShapeError`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LintShapeFault {
    /// A target other than `pair` without `when`.
    MissingWhen,
    /// `when` on a `pair` rule.
    WhenOnPair,
    /// A `pair` rule without `open` or `close`.
    MissingPairKey(&'static str),
    /// `open`, `close`, `inside` or `scope` on a target other than `pair`.
    PairKeyOffPair {
        key: &'static str,
        target: LintTarget,
    },
}

impl std::fmt::Display for LintRuleShapeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let id = &self.id;
        match self.fault {
            LintShapeFault::MissingWhen => write!(f, "rule `{id}`: missing field `when`"),
            LintShapeFault::WhenOnPair => write!(
                f,
                "rule `{id}`: `when` is refused on `target: pair`, which states `open` and \
                 `close` instead"
            ),
            LintShapeFault::MissingPairKey(key) => {
                write!(f, "rule `{id}`: a `target: pair` rule needs `{key}`")
            }
            LintShapeFault::PairKeyOffPair { key, target } => write!(
                f,
                "rule `{id}`: `{key}` belongs to `target: pair`, not `target: {}`; this \
                 target states `when`",
                target.name()
            ),
        }
    }
}

impl std::error::Error for LintRuleShapeError {}

/// The on-disk keys of a rule, before [`LintRuleDecl`]'s shape check.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLintRuleDecl {
    id: String,
    target: LintTarget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    when: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    open: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    close: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    inside: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    scope: Option<PairScope>,
    #[serde(default)]
    level: Option<LintLevel>,
    message: String,
    #[serde(default)]
    options: yaml_serde::Mapping,
}

impl TryFrom<RawLintRuleDecl> for LintRuleDecl {
    type Error = LintRuleShapeError;

    fn try_from(raw: RawLintRuleDecl) -> Result<Self, Self::Error> {
        let fail = |fault| {
            Err(LintRuleShapeError {
                id: raw.id.clone(),
                fault,
            })
        };
        let body = if raw.target == LintTarget::Pair {
            if raw.when.is_some() {
                return fail(LintShapeFault::WhenOnPair);
            }
            let (Some(open), Some(close)) = (raw.open.clone(), raw.close.clone()) else {
                let key = if raw.open.is_none() { "open" } else { "close" };
                return fail(LintShapeFault::MissingPairKey(key));
            };
            LintRuleBody::Pair(PairSpec {
                open,
                close,
                inside: raw.inside.clone(),
                scope: raw.scope.unwrap_or_default(),
            })
        } else {
            let pair_keys = [
                ("open", raw.open.is_some()),
                ("close", raw.close.is_some()),
                ("inside", raw.inside.is_some()),
                ("scope", raw.scope.is_some()),
            ];
            if let Some((key, _)) = pair_keys.into_iter().find(|(_, present)| *present) {
                return fail(LintShapeFault::PairKeyOffPair {
                    key,
                    target: raw.target,
                });
            }
            let Some(when) = raw.when.clone() else {
                return fail(LintShapeFault::MissingWhen);
            };
            LintRuleBody::Predicate { when }
        };
        Ok(Self {
            id: raw.id,
            target: raw.target,
            body,
            level: raw.level,
            message: raw.message,
            options: raw.options,
        })
    }
}

impl From<LintRuleDecl> for RawLintRuleDecl {
    fn from(decl: LintRuleDecl) -> Self {
        let (when, pair) = match decl.body {
            LintRuleBody::Predicate { when } => (Some(when), None),
            LintRuleBody::Pair(pair) => (None, Some(pair)),
        };
        Self {
            id: decl.id,
            target: decl.target,
            when,
            open: pair.as_ref().map(|p| p.open.clone()),
            close: pair.as_ref().map(|p| p.close.clone()),
            inside: pair.as_ref().and_then(|p| p.inside.clone()),
            scope: pair.map(|p| p.scope),
            level: decl.level,
            message: decl.message,
            options: decl.options,
        }
    }
}

/// Metric-table target a lint rule binds over (design §4). Serialized
/// lowercase to match the config file surface (`target: scene`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LintTarget {
    Line,
    Section,
    Scene,
    Speaker,
    Group,
    Project,
    /// Every body node — line, directive or element — with its authored
    /// attributes, siblings, parent and `preceding` scope (dsl 0.50.0 §8.1).
    Node,
    /// Begin/end balance over node rows: `open`, `close`, optional
    /// `inside` and `scope` (dsl 0.51.0 §6).
    Pair,
}

impl LintTarget {
    /// The target as written in a rule (`target: node`).
    pub fn name(self) -> &'static str {
        match self {
            LintTarget::Line => "line",
            LintTarget::Section => "section",
            LintTarget::Scene => "scene",
            LintTarget::Speaker => "speaker",
            LintTarget::Group => "group",
            LintTarget::Project => "project",
            LintTarget::Node => "node",
            LintTarget::Pair => "pair",
        }
    }
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
