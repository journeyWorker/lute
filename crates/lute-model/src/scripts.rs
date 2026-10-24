//! Play and test scripts under a project root: the walk that finds them and
//! the `choose:` grammar they share with `lute play`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// One scripted branch/hub answer. Timeout is kept typed all the way to the
/// play driver; it is never encoded as an option id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScriptedPick {
    Option(String),
    Timeout,
}

/// Every `*.play.yaml` / `*.test.yaml` under `dir`, sorted by path.
pub fn collect_scripts(dir: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(".play.yaml") || n.ends_with(".test.yaml"))
            {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, &mut out);
    out.sort();
    out
}

/// A step's, include's, or top-level `choose:`, parsed without the trace
/// mock grammar so a timeout remains a typed scripted answer.
///
/// # Errors
///
/// [`ChooseError`], whose text `lute play` reports for a `choose:` that is
/// not a mapping of branch/hub id to choice id(s) or `{ timeout: true }`.
pub fn parse_choose(
    v: &yaml_serde::Value,
) -> Result<BTreeMap<String, Vec<ScriptedPick>>, ChooseError> {
    let yaml_serde::Value::Mapping(entries) = v else {
        return Err(ChooseError::NotMapping);
    };
    let mut out = BTreeMap::new();
    for (key, value) in entries {
        let Some(id) = key.as_str() else {
            return Err(ChooseError::KeyNotId);
        };
        let values = match value {
            yaml_serde::Value::Sequence(items) => items
                .iter()
                .enumerate()
                .map(|(i, item)| parse_pick(id, item, Some(i + 1)))
                .collect::<Result<Vec<_>, _>>()?,
            _ => vec![parse_pick(id, value, None)?],
        };
        out.insert(id.to_string(), values);
    }
    Ok(out)
}

fn parse_pick(
    id: &str,
    value: &yaml_serde::Value,
    item: Option<usize>,
) -> Result<ScriptedPick, ChooseError> {
    let at = || PickAt {
        id: id.to_string(),
        item,
    };
    match value {
        yaml_serde::Value::String(option) if !option.trim().is_empty() => {
            Ok(ScriptedPick::Option(option.trim().to_string()))
        }
        yaml_serde::Value::Mapping(fields) => {
            let timeout = fields.get(yaml_serde::Value::String("timeout".into()));
            if fields.len() != 1 || timeout.is_none() {
                return Err(ChooseError::NotTimeoutMapping(at()));
            }
            if timeout.and_then(yaml_serde::Value::as_bool) != Some(true) {
                return Err(ChooseError::TimeoutNotTrue(at()));
            }
            Ok(ScriptedPick::Timeout)
        }
        _ => Err(ChooseError::NotPick(at())),
    }
}

/// A `choose:` answer's place: `choose.<id>`, and its 1-based list item
/// when the answers are a list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PickAt {
    pub id: String,
    pub item: Option<usize>,
}

impl std::fmt::Display for PickAt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let id = &self.id;
        match self.item {
            None => write!(f, "`choose.{id}`"),
            Some(i) => write!(f, "`choose.{id}` list item {i}"),
        }
    }
}

/// Why [`parse_choose`] refused a `choose:` value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChooseError {
    /// `choose:` is not a mapping.
    NotMapping,
    /// A `choose:` key that is not a string.
    KeyNotId,
    /// A mapping answer other than `{ timeout: <x> }`.
    NotTimeoutMapping(PickAt),
    /// `{ timeout: <x> }` with `<x>` not `true`.
    TimeoutNotTrue(PickAt),
    /// Neither a choice id nor a mapping.
    NotPick(PickAt),
}

impl std::fmt::Display for ChooseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChooseError::NotMapping => {
                f.write_str("`choose:` must be a mapping of branch/hub id to choice id(s)")
            }
            ChooseError::KeyNotId => f.write_str("`choose:` keys must be branch or hub ids"),
            ChooseError::NotTimeoutMapping(at) => {
                write!(f, "{at} mapping must be exactly `{{ timeout: true }}`")
            }
            ChooseError::TimeoutNotTrue(at) => {
                write!(f, "{at} timeout mapping must set `timeout` to `true`")
            }
            ChooseError::NotPick(at) => {
                write!(f, "{at} must be a choice id or `{{ timeout: true }}`")
            }
        }
    }
}

impl std::error::Error for ChooseError {}

/// The steps of one `include:` target (dsl 0.24.0 §1), read for `lute play`
/// and [`choose_keys`].
#[derive(Debug)]
pub struct IncludedSteps {
    /// The target as written, joined to the includer's directory.
    pub file: PathBuf,
    /// `file`, canonical: the identity a cycle check compares.
    pub canonical: PathBuf,
    pub text: String,
    pub steps: Vec<yaml_serde::Value>,
    /// The file is `steps: […]`, not a bare list of steps.
    pub under_steps: bool,
}

/// Why an `include:` target gave no steps.
#[derive(Debug)]
pub enum IncludeError {
    /// The file cannot be resolved or read (an [`lute_manifest::io_reason`]).
    Read { file: PathBuf, reason: String },
    /// The file is not YAML.
    Yaml { file: PathBuf, reason: String },
    /// The file is neither a list of steps nor a mapping whose only key is
    /// `steps:` holding one.
    Shape { file: PathBuf },
}

/// `lute play` adds the `include:` text as written to a [`IncludeError::Read`].
impl std::fmt::Display for IncludeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IncludeError::Read { file, reason } => {
                write!(f, "cannot read {}: {reason}", file.display())
            }
            IncludeError::Yaml { file, reason } => {
                write!(f, "{}: malformed YAML: {reason}", file.display())
            }
            IncludeError::Shape { file } => write!(
                f,
                "{}: an included file is a list of steps or `steps: […]`",
                file.display()
            ),
        }
    }
}

impl std::error::Error for IncludeError {}

/// `include: <rel>` written in `includer`: resolved against the includer's
/// directory and read as a list of steps or `steps: […]`.
///
/// # Errors
///
/// [`IncludeError`]: the file does not resolve or read, is not YAML, or has
/// another shape.
pub fn read_include(includer: &Path, rel: &str) -> Result<IncludedSteps, IncludeError> {
    let file = includer.parent().unwrap_or(Path::new(".")).join(rel);
    let read_err = |e: &std::io::Error, file: &Path| IncludeError::Read {
        file: file.to_path_buf(),
        reason: lute_manifest::io_reason(e),
    };
    let canonical = file.canonicalize().map_err(|e| read_err(&e, &file))?;
    let text = std::fs::read_to_string(&canonical).map_err(|e| read_err(&e, &file))?;
    let value: yaml_serde::Value = match yaml_serde::from_str(&text) {
        Ok(value) => value,
        Err(e) => {
            return Err(IncludeError::Yaml {
                file,
                reason: e.to_string(),
            })
        }
    };
    let (steps, under_steps) = match value {
        yaml_serde::Value::Sequence(items) => (items, false),
        yaml_serde::Value::Mapping(mut m) if m.len() == 1 => match m.remove("steps") {
            Some(yaml_serde::Value::Sequence(items)) => (items, true),
            _ => return Err(IncludeError::Shape { file }),
        },
        _ => return Err(IncludeError::Shape { file }),
    };
    Ok(IncludedSteps {
        file,
        canonical,
        text,
        steps,
        under_steps,
    })
}

/// dsl 0.49.0 §7.2 (#167): every branch/hub id a play or test script under
/// `root` names as a `choose:` key, at top level or in a step, the steps an
/// `include:` splices in included, recursively. A script, a `choose:`, or an
/// include that does not parse names nothing here: `lute play` / `lute
/// test` report it.
pub fn choose_keys(root: &Path) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for script in collect_scripts(root) {
        let Ok(text) = std::fs::read_to_string(&script) else {
            continue;
        };
        let Ok(yaml_serde::Value::Mapping(top)) = yaml_serde::from_str(&text) else {
            continue;
        };
        if let Some(choose) = top.get("choose") {
            extend_choose_keys(choose, &mut out);
        }
        if let Some(yaml_serde::Value::Sequence(steps)) = top.get("steps") {
            let mut stack = vec![script.canonicalize().unwrap_or_else(|_| script.clone())];
            step_choose_keys(steps, &script, &mut stack, &mut out);
        }
    }
    out
}

fn extend_choose_keys(choose: &yaml_serde::Value, out: &mut BTreeSet<String>) {
    if let Ok(keys) = parse_choose(choose) {
        out.extend(keys.into_keys());
    }
}

/// The `choose:` keys of `steps`, written in `from`, and of every file an
/// `include:` step among them splices in. `stack`: the canonical files
/// being expanded; naming one again is a cycle and adds nothing.
fn step_choose_keys(
    steps: &[yaml_serde::Value],
    from: &Path,
    stack: &mut Vec<PathBuf>,
    out: &mut BTreeSet<String>,
) {
    for step in steps {
        if let Some(choose) = step.get("choose") {
            extend_choose_keys(choose, out);
        }
        let Some(rel) = step
            .get("include")
            .and_then(yaml_serde::Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let Ok(included) = read_include(from, rel) else {
            continue;
        };
        if stack.contains(&included.canonical) {
            continue;
        }
        stack.push(included.canonical);
        step_choose_keys(&included.steps, &included.file, stack, out);
        stack.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 0.49.1: the typed refusal renders the 0.49.0 `lute play` text.
    #[test]
    fn choose_error_text_is_the_0_49_0_message() {
        let text = |yaml: &str| {
            parse_choose(&yaml_serde::from_str(yaml).unwrap())
                .unwrap_err()
                .to_string()
        };
        assert_eq!(
            text("[a]"),
            "`choose:` must be a mapping of branch/hub id to choice id(s)"
        );
        assert_eq!(text("{1: a}"), "`choose:` keys must be branch or hub ids");
        assert_eq!(
            text("{m: {timeout: true, x: 1}}"),
            "`choose.m` mapping must be exactly `{ timeout: true }`"
        );
        assert_eq!(
            text("{m: [a, {timeout: false}]}"),
            "`choose.m` list item 2 timeout mapping must set `timeout` to `true`"
        );
        assert_eq!(
            text("{m: 3}"),
            "`choose.m` must be a choice id or `{ timeout: true }`"
        );
    }

    #[test]
    fn include_error_text() {
        let file = PathBuf::from("steps/a.yaml");
        let read = IncludeError::Read {
            file: file.clone(),
            reason: "No such file or directory".into(),
        };
        assert_eq!(
            read.to_string(),
            "cannot read steps/a.yaml: No such file or directory"
        );
        let yaml = IncludeError::Yaml {
            file: file.clone(),
            reason: "bad".into(),
        };
        assert_eq!(yaml.to_string(), "steps/a.yaml: malformed YAML: bad");
        assert_eq!(
            IncludeError::Shape { file }.to_string(),
            "steps/a.yaml: an included file is a list of steps or `steps: […]`"
        );
    }
}
