//! `lute.lint.yaml` parser (spec §3).
//!
//! Two shapes:
//! - `rules: { <id>: level }` — a bare scalar level shorthand.
//! - `rules: { <id>: { level: …, options: {…} } }` — the full form.
//!
//! Both go through the same [`RuleOverride`] normalization. `custom:` entries
//! deserialize into [`crate::model::LintRuleDecl`] verbatim; unknown option
//! keys, colliding custom ids, and bad option types are surfaced as
//! `E-LINT-CONFIG` diagnostics by [`crate::rules::apply_config`] — the config
//! layer here parses whatever the YAML says and defers rule-aware
//! validation. A malformed YAML file returns a hard [`ConfigError`] (mapped
//! to exit 2 by the caller, spec §3).

use std::collections::BTreeMap;

use lute_core_span::{Diagnostic, Layer, Severity, Span};

use crate::model::{LintLevel, LintRuleDecl, E_LINT_CONFIG};

/// A resolved level + option override for one rule (spec §3).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RuleOverride {
    pub level: Option<LintLevel>,
    pub options: serde_yaml::Mapping,
}

/// The full parsed `lute.lint.yaml` (spec §3).
#[derive(Clone, Debug, Default)]
pub struct LintConfig {
    pub lsp: bool,
    pub ignore: Vec<String>,
    pub rules: BTreeMap<String, RuleOverride>,
    pub custom: Vec<LintRuleDecl>,
}

/// A hard YAML-level failure. Malformed YAML never reaches the diagnostic
/// stream — the caller reports it separately and exits 2 (spec §3).
#[derive(Debug)]
pub struct ConfigError {
    pub message: String,
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ConfigError {}

/// Parse a `lute.lint.yaml` string, returning the [`LintConfig`] plus any
/// non-fatal semantic diagnostics (spec §3): shape-level defects that keep
/// the rest of the file usable. A truly-broken YAML tree — invalid syntax,
/// wrong root type, wrong `rules` sub-value shape — is a [`ConfigError`].
///
/// `config_file` names the on-disk path used to anchor `E-LINT-CONFIG`
/// diagnostics. `_config_span` is the full-file span the caller precomputed
/// from the file text via `TextIndex`; a nested-shape mishap anchors to the
/// file head (line 1, column 1) because YAML round-trip does not preserve
/// per-key byte offsets and pulling in a stateful parser would blow the
/// zero-new-dep constraint (spec §Constraints).
pub fn parse_config(
    yaml: &str,
    config_span: Span,
) -> Result<(LintConfig, Vec<Diagnostic>), ConfigError> {
    let root: serde_yaml::Value = serde_yaml::from_str(yaml)
        .map_err(|e| ConfigError { message: format!("malformed lute.lint.yaml: {e}") })?;
    // An empty file is legal — the caller falls back to defaults.
    if root.is_null() {
        return Ok((LintConfig::default(), Vec::new()));
    }
    let map = match root {
        serde_yaml::Value::Mapping(m) => m,
        _ => {
            return Err(ConfigError {
                message: "lute.lint.yaml must be a mapping at the root".into(),
            })
        }
    };
    let mut cfg = LintConfig::default();
    let mut diags = Vec::new();

    for (k, v) in map {
        let key = match k.as_str() {
            Some(s) => s.to_string(),
            None => {
                diags.push(diag(
                    format!("non-string key in lute.lint.yaml: {k:?}"),
                    config_span,
                ));
                continue;
            }
        };
        match key.as_str() {
            "lsp" => match v {
                serde_yaml::Value::Bool(b) => cfg.lsp = b,
                other => diags.push(diag(
                    format!("`lsp:` must be a bool, got {other:?}"),
                    config_span,
                )),
            },
            "ignore" => match v {
                serde_yaml::Value::Sequence(seq) => {
                    for item in seq {
                        match item.as_str() {
                            Some(s) => cfg.ignore.push(s.to_string()),
                            None => diags.push(diag(
                                format!("`ignore:` entries must be strings, got {item:?}"),
                                config_span,
                            )),
                        }
                    }
                }
                other => diags.push(diag(
                    format!("`ignore:` must be a list, got {other:?}"),
                    config_span,
                )),
            },
            "rules" => match v {
                serde_yaml::Value::Mapping(rules) => {
                    for (rk, rv) in rules {
                        let id = match rk.as_str() {
                            Some(s) => s.to_string(),
                            None => {
                                diags.push(diag(
                                    format!("rule key must be a string, got {rk:?}"),
                                    config_span,
                                ));
                                continue;
                            }
                        };
                        match parse_rule_override(&rv) {
                            Ok(ovr) => {
                                cfg.rules.insert(id, ovr);
                            }
                            Err(msg) => diags.push(diag(
                                format!("rule `{id}`: {msg}"),
                                config_span,
                            )),
                        }
                    }
                }
                other => diags.push(diag(
                    format!("`rules:` must be a mapping, got {other:?}"),
                    config_span,
                )),
            },
            "custom" => match v {
                serde_yaml::Value::Sequence(seq) => {
                    for item in seq {
                        match serde_yaml::from_value::<LintRuleDecl>(item.clone()) {
                            Ok(decl) => cfg.custom.push(decl),
                            Err(e) => diags.push(diag(
                                format!("bad custom rule: {e}"),
                                config_span,
                            )),
                        }
                    }
                }
                other => diags.push(diag(
                    format!("`custom:` must be a list, got {other:?}"),
                    config_span,
                )),
            },
            unknown => diags.push(diag(
                format!("unknown key `{unknown}:` in lute.lint.yaml"),
                config_span,
            )),
        }
    }

    Ok((cfg, diags))
}

fn parse_rule_override(v: &serde_yaml::Value) -> Result<RuleOverride, String> {
    match v {
        serde_yaml::Value::String(s) => {
            let lvl = level_from_str(s)
                .ok_or_else(|| format!("unknown level `{s}`; expected off|hint|info|warn|error"))?;
            Ok(RuleOverride { level: Some(lvl), options: Default::default() })
        }
        serde_yaml::Value::Mapping(m) => {
            let mut ovr = RuleOverride::default();
            for (k, v) in m {
                let key = k
                    .as_str()
                    .ok_or_else(|| format!("non-string key {k:?}"))?;
                match key {
                    "level" => {
                        let s = v
                            .as_str()
                            .ok_or_else(|| "`level:` must be a string".to_string())?;
                        ovr.level = Some(level_from_str(s).ok_or_else(|| {
                            format!("unknown level `{s}`; expected off|hint|info|warn|error")
                        })?);
                    }
                    "options" => match v {
                        serde_yaml::Value::Mapping(m) => ovr.options = m.clone(),
                        _ => return Err("`options:` must be a mapping".into()),
                    },
                    other => return Err(format!("unknown key `{other}`")),
                }
            }
            Ok(ovr)
        }
        other => Err(format!("must be a level string or mapping, got {other:?}")),
    }
}

fn level_from_str(s: &str) -> Option<LintLevel> {
    match s {
        "off" => Some(LintLevel::Off),
        "hint" => Some(LintLevel::Hint),
        "info" => Some(LintLevel::Info),
        "warn" => Some(LintLevel::Warn),
        "error" => Some(LintLevel::Error),
        _ => None,
    }
}

/// Deep-merge `override_map` OVER `defaults`. Scalars/lists at the same key
/// replace; nested mappings recurse (spec §3 "option maps deep-merge over a
/// rule's declared defaults; scalars override").
pub fn deep_merge(
    defaults: &serde_yaml::Mapping,
    override_map: &serde_yaml::Mapping,
) -> serde_yaml::Mapping {
    let mut out = defaults.clone();
    for (k, v) in override_map {
        match (out.get(k), v) {
            (Some(serde_yaml::Value::Mapping(a)), serde_yaml::Value::Mapping(b)) => {
                out.insert(k.clone(), serde_yaml::Value::Mapping(deep_merge(a, b)));
            }
            _ => {
                out.insert(k.clone(), v.clone());
            }
        }
    }
    out
}

fn diag(message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: E_LINT_CONFIG.to_string(),
        severity: Severity::Error,
        message,
        span,
        layer: Layer::Content,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zero_span() -> Span {
        Span {
            byte_start: 0,
            byte_end: 0,
            line: 1,
            column: 1,
            utf16_range: (0, 0),
        }
    }

    #[test]
    fn empty_file_defaults() {
        let (cfg, diags) = parse_config("", zero_span()).unwrap();
        assert!(diags.is_empty());
        assert!(!cfg.lsp);
        assert!(cfg.ignore.is_empty());
        assert!(cfg.rules.is_empty());
        assert!(cfg.custom.is_empty());
    }

    #[test]
    fn full_shape() {
        let src = r#"
lsp: true
ignore: ["drafts/**"]
rules:
  dialogue-length: warn
  dialogue-ratio: { level: error, options: { min: 0.35 } }
  my-plugin/x: off
custom:
  - id: too-many-choices
    target: scene
    when: "scene.choices > options.max"
    level: warn
    message: "too many"
    options: { max: 6 }
"#;
        let (cfg, diags) = parse_config(src, zero_span()).unwrap();
        assert!(diags.is_empty(), "{diags:?}");
        assert!(cfg.lsp);
        assert_eq!(cfg.ignore, vec!["drafts/**".to_string()]);
        assert_eq!(cfg.rules.get("dialogue-length").unwrap().level, Some(LintLevel::Warn));
        let dr = cfg.rules.get("dialogue-ratio").unwrap();
        assert_eq!(dr.level, Some(LintLevel::Error));
        assert!(dr.options.contains_key(serde_yaml::Value::from("min")));
        assert_eq!(cfg.rules.get("my-plugin/x").unwrap().level, Some(LintLevel::Off));
        assert_eq!(cfg.custom.len(), 1);
        assert_eq!(cfg.custom[0].id, "too-many-choices");
    }

    #[test]
    fn bad_level_becomes_diagnostic() {
        let src = "rules:\n  dialogue-length: shout\n";
        let (_, diags) = parse_config(src, zero_span()).unwrap();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, "E-LINT-CONFIG");
    }

    #[test]
    fn malformed_yaml_hard_errors() {
        let src = "not: valid: yaml: ][";
        assert!(parse_config(src, zero_span()).is_err());
    }

    #[test]
    fn deep_merge_replaces_scalars_and_recurses() {
        let defaults: serde_yaml::Mapping = serde_yaml::from_str(
            "maxWords: 40\nnested: { a: 1, b: 2 }\n",
        )
        .unwrap();
        let overrides: serde_yaml::Mapping =
            serde_yaml::from_str("maxWords: 50\nnested: { b: 20 }\n").unwrap();
        let merged = deep_merge(&defaults, &overrides);
        let expected: serde_yaml::Mapping =
            serde_yaml::from_str("maxWords: 50\nnested: { a: 1, b: 20 }\n").unwrap();
        assert_eq!(merged, expected);
    }
}
