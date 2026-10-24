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
/// The message `lute play` reports for a `choose:` that is not a mapping of
/// branch/hub id to choice id(s) or `{ timeout: true }`.
pub fn parse_choose(v: &yaml_serde::Value) -> Result<BTreeMap<String, Vec<ScriptedPick>>, String> {
    let yaml_serde::Value::Mapping(entries) = v else {
        return Err("`choose:` must be a mapping of branch/hub id to choice id(s)".into());
    };
    let mut out = BTreeMap::new();
    for (key, value) in entries {
        let Some(id) = key.as_str() else {
            return Err("`choose:` keys must be branch or hub ids".into());
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
) -> Result<ScriptedPick, String> {
    let where_ = item.map_or_else(
        || format!("`choose.{id}`"),
        |i| format!("`choose.{id}` list item {i}"),
    );
    match value {
        yaml_serde::Value::String(option) if !option.trim().is_empty() => {
            Ok(ScriptedPick::Option(option.trim().to_string()))
        }
        yaml_serde::Value::Mapping(fields) => {
            let timeout = fields.get(yaml_serde::Value::String("timeout".into()));
            if fields.len() != 1 || timeout.is_none() {
                return Err(format!(
                    "{where_} mapping must be exactly `{{ timeout: true }}`"
                ));
            }
            if timeout.and_then(yaml_serde::Value::as_bool) != Some(true) {
                return Err(format!(
                    "{where_} timeout mapping must set `timeout` to `true`"
                ));
            }
            Ok(ScriptedPick::Timeout)
        }
        _ => Err(format!(
            "{where_} must be a choice id or `{{ timeout: true }}`"
        )),
    }
}

/// dsl 0.49.0 §7.2 (#167): every branch/hub id a play or test script under
/// `root` names as a `choose:` key, at top level or in a step (an `include:`
/// step included). A script, or a `choose:`, that does not parse names
/// nothing here: `lute play` / `lute test` report it.
pub fn choose_keys(root: &Path) -> BTreeSet<String> {
    let key = |k: &str| yaml_serde::Value::String(k.to_string());
    let mut out = BTreeSet::new();
    for script in collect_scripts(root) {
        let Ok(text) = std::fs::read_to_string(&script) else {
            continue;
        };
        let Ok(yaml_serde::Value::Mapping(top)) = yaml_serde::from_str(&text) else {
            continue;
        };
        let steps = match top.get(key("steps")) {
            Some(yaml_serde::Value::Sequence(steps)) => steps.as_slice(),
            _ => &[],
        };
        let chooses = std::iter::once(top.get(key("choose")))
            .chain(steps.iter().map(|step| step.get("choose")))
            .flatten();
        for choose in chooses {
            if let Ok(keys) = parse_choose(choose) {
                out.extend(keys.into_keys());
            }
        }
    }
    out
}
