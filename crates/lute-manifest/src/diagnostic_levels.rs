//! `lute.project.yaml` `diagnostics:` (dsl 0.50.0 §6): the project's level
//! for a warning code, optionally scoped to project-relative path globs.
//!
//! ```yaml
//! diagnostics:
//!   W-DISPLAY-NAME-DUP: error
//!   W-STAGE-ABSENT: { level: off, paths: ["scenes/imported/**"] }
//! ```
//!
//! This module reads the entries' SHAPE (a level, `{ level, paths }`, the
//! globs). Whether a key is a diagnostic code the project may re-level is
//! `lute_check::diagnostics_config`, which knows the code table. An entry
//! for anything but a `W-*` code is inert here: [`DiagnosticLevels::level_for`]
//! answers only `W-*` codes, so a refused entry never applies.
//!
//! Levels apply at the reporting boundary only (the CLI's severity policy,
//! the LSP's publish); no analysis reads them.

use std::collections::BTreeMap;
use std::ops::Range;

use lute_core_span::Severity;

/// `E-DIAGNOSTICS-CONFIG`: a malformed or illegal `diagnostics:` entry. The
/// entry is dropped, the rest of the manifest loads, and the manifest is
/// invalid.
pub const E_DIAGNOSTICS_CONFIG: &str = "E-DIAGNOSTICS-CONFIG";

/// The manifest key that `diagnostics:` levels are reported under.
pub const MANIFEST_KEY: &str = "diagnostics";

/// The levels an entry may set, in the order messages list them.
pub const LEVEL_NAMES: [&str; 4] = ["off", "info", "warn", "error"];

/// The project's level for a code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticLevel {
    /// Not reported; counted as suppressed.
    Off,
    Info,
    Warn,
    Error,
}

impl DiagnosticLevel {
    /// The severity a diagnostic is reported at under this level; `None`
    /// for [`DiagnosticLevel::Off`].
    pub fn severity(self) -> Option<Severity> {
        match self {
            DiagnosticLevel::Off => None,
            DiagnosticLevel::Info => Some(Severity::Info),
            DiagnosticLevel::Warn => Some(Severity::Warning),
            DiagnosticLevel::Error => Some(Severity::Error),
        }
    }
}

impl std::str::FromStr for DiagnosticLevel {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "off" => Ok(DiagnosticLevel::Off),
            "info" => Ok(DiagnosticLevel::Info),
            "warn" => Ok(DiagnosticLevel::Warn),
            "error" => Ok(DiagnosticLevel::Error),
            _ => Err(()),
        }
    }
}

/// One `diagnostics:` entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LevelEntry {
    pub level: DiagnosticLevel,
    /// Project-relative globs (`lute.lint.yaml` `ignore:` syntax); empty
    /// means everywhere.
    pub paths: Vec<String>,
    /// The entry's key in `lute.project.yaml`, when located.
    pub span: Option<Range<usize>>,
}

/// A manifest's resolved `diagnostics:` block, keyed by code. Empty when the
/// manifest has none.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DiagnosticLevels {
    entries: BTreeMap<String, LevelEntry>,
}

impl DiagnosticLevels {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Every entry, by code.
    pub fn entries(&self) -> impl Iterator<Item = (&str, &LevelEntry)> {
        self.entries
            .iter()
            .map(|(code, entry)| (code.as_str(), entry))
    }

    /// Whether some entry for `code` is scoped by `paths`, i.e. whether
    /// [`Self::level_for`] needs the diagnostic's path at all.
    pub fn needs_path(&self, code: &str) -> bool {
        self.entries.get(code).is_some_and(|e| !e.paths.is_empty())
    }

    /// The project's level for a `code` diagnostic of the document at
    /// `rel_path` (project-root-relative, `/`-separated), or `None` when the
    /// code keeps its native severity there. Only `W-*` codes are
    /// configurable. An entry with `paths` applies only to a document whose
    /// path matches one of them, so a diagnostic without a path (`None`)
    /// matches only an entry without `paths`.
    pub fn level_for(&self, code: &str, rel_path: Option<&str>) -> Option<DiagnosticLevel> {
        if !code.starts_with("W-") {
            return None;
        }
        let entry = self.entries.get(code)?;
        if entry.paths.is_empty() {
            return Some(entry.level);
        }
        rel_path
            .filter(|path| crate::glob::matches_any(&entry.paths, path))
            .map(|_| entry.level)
    }
}

/// One shape fault, located at the entry's key (or at `diagnostics:`).
fn fault(span: Option<Range<usize>>, message: String) -> crate::project::ResolveDiag {
    crate::project::ResolveDiag {
        code: E_DIAGNOSTICS_CONFIG.to_string(),
        message,
        span,
    }
}

/// The message a bad `paths:` value gets.
const PATHS_MESSAGE: &str = "`paths` takes project-relative globs, e.g. `scenes/imported/**`";

/// A level written as anything but one of [`LEVEL_NAMES`].
fn level_message(written: &str) -> String {
    format!(
        "level `{written}` is not one of {}",
        LEVEL_NAMES
            .iter()
            .map(|l| format!("`{l}`"))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

/// How a YAML scalar reads in a message.
fn scalar_text(v: &yaml_serde::Value) -> String {
    match v {
        yaml_serde::Value::String(s) => s.clone(),
        yaml_serde::Value::Null => "null".to_string(),
        yaml_serde::Value::Bool(b) => b.to_string(),
        yaml_serde::Value::Number(n) => n.to_string(),
        yaml_serde::Value::Sequence(_) => "a list".to_string(),
        yaml_serde::Value::Mapping(_) => "a mapping".to_string(),
        yaml_serde::Value::Tagged(t) => scalar_text(&t.value),
    }
}

fn parse_level(v: &yaml_serde::Value) -> Result<DiagnosticLevel, String> {
    v.as_str()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| level_message(&scalar_text(v)))
}

/// A project-relative glob: non-empty, not absolute, no `..` segment.
fn is_relative_glob(s: &str) -> bool {
    !s.is_empty()
        && !s.starts_with('/')
        && !s.contains('\\')
        && !std::path::Path::new(s).is_absolute()
        && !s.split('/').any(|seg| seg == "..")
}

fn parse_paths(v: &yaml_serde::Value) -> Result<Vec<String>, String> {
    let yaml_serde::Value::Sequence(items) = v else {
        return Err(PATHS_MESSAGE.to_string());
    };
    if items.is_empty() {
        return Err(PATHS_MESSAGE.to_string());
    }
    items
        .iter()
        .map(|item| {
            item.as_str()
                .filter(|s| is_relative_glob(s))
                .map(str::to_string)
                .ok_or_else(|| PATHS_MESSAGE.to_string())
        })
        .collect()
}

/// One entry's value: a level, or `{ level, paths? }`.
fn parse_entry(v: &yaml_serde::Value) -> Result<(DiagnosticLevel, Vec<String>), String> {
    let yaml_serde::Value::Mapping(map) = v else {
        return parse_level(v).map(|level| (level, Vec::new()));
    };
    for key in map.keys() {
        let name = key.as_str().unwrap_or_default();
        if name != "level" && name != "paths" {
            return Err(format!(
                "an entry is a level or `{{ level, paths }}`; unknown key `{}`{}",
                scalar_text(key),
                crate::suggest::did_you_mean(name, ["level", "paths"])
            ));
        }
    }
    let level = map.get("level").ok_or_else(|| {
        "an entry written as a mapping needs `level:` (one of `off`, `info`, `warn`, `error`)"
            .to_string()
    })?;
    let level = parse_level(level)?;
    let paths = map.get("paths").map(parse_paths).transpose()?;
    Ok((level, paths.unwrap_or_default()))
}

/// Resolve the raw `diagnostics:` value. Each malformed entry is reported
/// and dropped; the others apply.
pub(crate) fn resolve_diagnostics(
    raw: Option<&yaml_serde::Value>,
    locate: &dyn Fn(&[&str]) -> Option<Range<usize>>,
) -> (DiagnosticLevels, Vec<crate::project::ResolveDiag>) {
    let mut levels = DiagnosticLevels::default();
    let mut diags = Vec::new();
    let map = match raw {
        None | Some(yaml_serde::Value::Null) => return (levels, diags),
        Some(yaml_serde::Value::Mapping(map)) => map,
        Some(_) => {
            diags.push(fault(
                locate(&[MANIFEST_KEY]),
                "`diagnostics:` maps a diagnostic code to a level, e.g. `W-STAGE-ABSENT: off`"
                    .to_string(),
            ));
            return (levels, diags);
        }
    };
    for (key, value) in map {
        let Some(code) = key.as_str() else {
            diags.push(fault(
                locate(&[MANIFEST_KEY]),
                format!(
                    "`{}` is not a diagnostic code; a `diagnostics:` key is a code, e.g. \
                     `W-STAGE-ABSENT`",
                    scalar_text(key)
                ),
            ));
            continue;
        };
        let span = locate(&[MANIFEST_KEY, code]);
        match parse_entry(value) {
            Ok((level, paths)) => {
                levels
                    .entries
                    .insert(code.to_string(), LevelEntry { level, paths, span });
            }
            Err(message) => diags.push(fault(span, format!("`{code}`: {message}"))),
        }
    }
    (levels, diags)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolve(yaml: &str) -> (DiagnosticLevels, Vec<String>) {
        let raw: yaml_serde::Value = yaml_serde::from_str(yaml).unwrap();
        let (levels, diags) = resolve_diagnostics(Some(&raw), &|_| None);
        (levels, diags.into_iter().map(|d| d.message).collect())
    }

    #[test]
    fn levels_and_scoped_entries_resolve() {
        let (levels, diags) = resolve(
            "W-DISPLAY-NAME-DUP: error\nW-STAGE-ABSENT: { level: off, paths: [\"scenes/imported/**\"] }\nW-TEXT-GLUE: info\nW-WIP: warn\n",
        );
        assert!(diags.is_empty(), "{diags:?}");
        assert_eq!(
            levels.level_for("W-DISPLAY-NAME-DUP", None),
            Some(DiagnosticLevel::Error)
        );
        assert_eq!(
            levels.level_for("W-STAGE-ABSENT", Some("scenes/imported/a.lute")),
            Some(DiagnosticLevel::Off)
        );
        assert_eq!(
            levels.level_for("W-STAGE-ABSENT", Some("scenes/a.lute")),
            None
        );
        assert_eq!(levels.level_for("W-STAGE-ABSENT", None), None);
        assert_eq!(
            levels.level_for("W-TEXT-GLUE", None),
            Some(DiagnosticLevel::Info)
        );
        assert_eq!(levels.level_for("W-WIP", None), Some(DiagnosticLevel::Warn));
        assert!(levels.needs_path("W-STAGE-ABSENT"));
        assert!(!levels.needs_path("W-TEXT-GLUE"));
    }

    #[test]
    fn only_warning_codes_are_configurable() {
        let (levels, _) = resolve("E-UNKNOWN-ATTR: off\nL-LINE-LENGTH: off\n");
        assert_eq!(levels.level_for("E-UNKNOWN-ATTR", None), None);
        assert_eq!(levels.level_for("L-LINE-LENGTH", None), None);
    }

    #[test]
    fn shape_faults_are_reported_and_dropped() {
        let (levels, diags) = resolve(
            "W-A: silent\nW-B: { level: off, paths: [] }\nW-C: { level: off, paths: [\"/abs/**\"] }\nW-D: { level: off, paths: [\"../x/**\"] }\nW-E: { level: off, paths: [3] }\nW-F: { paths: [\"a/**\"] }\nW-G: { level: off, path: [\"a\"] }\nW-H: true\n",
        );
        assert!(levels.is_empty(), "{levels:?}");
        assert_eq!(
            diags,
            [
                "`W-A`: level `silent` is not one of `off`, `info`, `warn`, `error`",
                "`W-B`: `paths` takes project-relative globs, e.g. `scenes/imported/**`",
                "`W-C`: `paths` takes project-relative globs, e.g. `scenes/imported/**`",
                "`W-D`: `paths` takes project-relative globs, e.g. `scenes/imported/**`",
                "`W-E`: `paths` takes project-relative globs, e.g. `scenes/imported/**`",
                "`W-F`: an entry written as a mapping needs `level:` (one of `off`, `info`, `warn`, `error`)",
                "`W-G`: an entry is a level or `{ level, paths }`; unknown key `path` — did you mean `paths`?",
                "`W-H`: level `true` is not one of `off`, `info`, `warn`, `error`",
            ]
        );
    }

    #[test]
    fn a_non_mapping_block_is_one_fault() {
        let (levels, diags) = resolve("[W-A]\n");
        assert!(levels.is_empty());
        assert_eq!(
            diags,
            ["`diagnostics:` maps a diagnostic code to a level, e.g. `W-STAGE-ABSENT: off`"]
        );
    }
}
