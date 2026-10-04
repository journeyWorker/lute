use super::*;

fn unknown_key_line(where_: &str, key: &str, allowed: &[&str]) -> String {
    if let Some((_, _, msg)) = RENAMED_TEST_KEYS
        .iter()
        .find(|(level, old, _)| *level == where_ && *old == key)
    {
        return format!("error [E-TEST-KEY] {msg}");
    }
    let sugg = lute_manifest::suggest::did_you_mean(key, allowed.iter().copied());
    let play = if where_ == "top-level" {
        if crate::play::STEP_KEYS.contains(&key) {
            Some("a play step")
        } else {
            crate::play::SCRIPT_KEYS
                .contains(&key)
                .then_some("a play script's top level")
        }
    } else {
        crate::play_expect::STEP_EXPECT_KEYS
            .contains(&key)
            .then_some("a play step's `expect:`")
    };
    let hint = play
        .map(|p| {
            let ours = TEST_SPELLING_OF_STEP_KEYS
                .iter()
                .find(|(k, _)| where_ == "top-level" && *k == key)
                .map(|(_, ours)| format!("; a test {ours}"))
                .unwrap_or_default();
            format!(" (`{key}:` belongs to {p}, in a `*.play.yaml`{ours})")
        })
        .unwrap_or_default();
    format!(
        "error [E-TEST-KEY] unknown {where_} key `{key}` in a `*.test.yaml`{sugg}{hint} (legal: {})",
        allowed.join(", ")
    )
}

/// Every closed-key violation in one test file, both levels, in document
/// order, each at `file:line:col` of the key (or value) it names. Empty
/// when the file is well-keyed.
pub(super) fn closed_key_violations(map: &serde_yaml::Mapping, text: &str, file: &Path) -> Vec<String> {
    use lute_trace::YamlStep::{Key, Value};
    let at = |path: &[lute_trace::YamlStep<'_>]| {
        let s = (1..=path.len())
            .rev()
            .find_map(|end| lute_trace::yaml_span(text, &path[..end]));
        match s {
            Some(s) => format!("{}:{}:{}: ", file.display(), s.line, s.column),
            None => format!("{}: ", file.display()),
        }
    };
    let mut out = Vec::new();
    for (k, v) in map {
        let Some(key) = k.as_str() else {
            out.push(format!(
                "{}error [E-TEST-KEY] a top-level key must be a string in a `*.test.yaml`",
                at(&[])
            ));
            continue;
        };
        if !TEST_TOP_KEYS.contains(&key) {
            out.push(format!(
                "{}{}",
                at(&[Key(key)]),
                unknown_key_line("top-level", key, TEST_TOP_KEYS)
            ));
            continue;
        }
        if key == "expect" {
            if let Some(em) = v.as_mapping() {
                for (ek, ev) in em {
                    match ek.as_str() {
                        // How the walk ended: one of play's `expect.end`
                        // values — a misspelt one could never hold.
                        Some("end") => {
                            let ends = crate::play_expect::ENDS;
                            match ev.as_str() {
                                Some(e) if ends.contains(&e) => {}
                                got => out.push(format!(
                                    "{}error [E-TEST-KEY] `expect.end: {}` names no way a walk \
                                     ends{} (one of: {})",
                                    at(&[Key("expect"), Key("end"), Value]),
                                    got.unwrap_or("?"),
                                    got.map(|g| lute_manifest::suggest::did_you_mean(
                                        g,
                                        ends.iter().copied()
                                    ))
                                    .unwrap_or_default(),
                                    ends.join(", ")
                                )),
                            }
                        }
                        Some(ekey) if TEST_EXPECT_KEYS.contains(&ekey) => {}
                        Some(ekey) => out.push(format!(
                            "{}{}",
                            at(&[Key("expect"), Key(ekey)]),
                            unknown_key_line("`expect:`", ekey, TEST_EXPECT_KEYS)
                        )),
                        None => out.push(format!(
                            "{}error [E-TEST-KEY] an `expect:` key must be a string",
                            at(&[Key("expect")])
                        )),
                    }
                }
            }
        }
    }
    out
}

/// Return authored node ids named by a validated test's typed fields. Context
/// discovery must not treat arbitrary YAML values as references.
pub(crate) fn static_context_references(path: &Path) -> Result<Vec<String>, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("cannot read test script {}: {error}", path.display()))?;
    let value: serde_yaml::Value =
        serde_yaml::from_str(&text).map_err(|error| format!("malformed test YAML: {error}"))?;
    let Some(map) = value.as_mapping() else {
        return Err("test script must be a YAML mapping".into());
    };
    if !closed_key_violations(map, &text, path).is_empty() {
        return Err("test script has unknown keys".into());
    }
    let mut refs = BTreeSet::new();
    if let Some(ids) = map.get("visited").and_then(serde_yaml::Value::as_sequence) {
        refs.extend(ids.iter().filter_map(|id| id.as_str().map(str::to_string)));
    }
    if let Some(ids) = map.get("accepts").and_then(serde_yaml::Value::as_sequence) {
        refs.extend(ids.iter().filter_map(|id| id.as_str().map(str::to_string)));
    }
    if let Some(quests) = map.get("quests").and_then(serde_yaml::Value::as_mapping) {
        refs.extend(quests.keys().filter_map(|id| id.as_str().map(str::to_string)));
    }
    if let Some(id) = map.get("beat").and_then(serde_yaml::Value::as_str) {
        refs.insert(id.to_string());
    }
    if let Some(id) = map.get("entry").and_then(serde_yaml::Value::as_str) {
        refs.insert(id.to_string());
    }
    for key in ["entries"] {
        if let Some(ids) = map.get(key).and_then(serde_yaml::Value::as_sequence) {
            refs.extend(ids.iter().filter_map(|id| id.as_str().map(str::to_string)));
        }
    }
    if let Some(expect) = map.get("expect") {
        collect_test_expect_context_references(expect, &mut refs);
    }
    Ok(refs.into_iter().collect())
}

fn collect_test_expect_context_references(value: &serde_yaml::Value, refs: &mut BTreeSet<String>) {
    let Some(map) = value.as_mapping() else { return };
    for (key, value) in map {
        let Some(key) = key.as_str() else { continue };
        match key {
            "offered" | "notOffered" | "presented" => {
                if let Some(ids) = value.as_sequence() {
                    refs.extend(ids.iter().filter_map(|id| id.as_str().map(str::to_string)));
                }
            }
            "quests" => {
                if let Some(quests) = value.as_mapping() {
                    refs.extend(quests.keys().filter_map(|id| id.as_str().map(str::to_string)));
                }
            }
            _ => {}
        }
    }
}
/// `path` with each `dir/..` pair dropped lexically, for display only
/// (`./tests/../scenes/a.lute` → `./scenes/a.lute`). A leading `..` stays.
pub(super) fn fold_parent_dirs(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            std::path::Component::ParentDir
                if matches!(
                    out.components().next_back(),
                    Some(std::path::Component::Normal(_))
                ) =>
            {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// Recursively collect every file under `dir` whose name ends in `suffix`
/// (`.test.yaml`, `.play.yaml`), byte-sorted for deterministic order —
/// mirrors [`crate::find_lute_files`]'s walk (stack, symlinked dirs not
/// followed).
pub(super) fn find_files_with_suffix(dir: &Path, suffix: &str) -> std::io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d)? {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                stack.push(path);
            } else if path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(suffix))
            {
                out.push(path);
            }
        }
    }
    out.sort();
    Ok(out)
}
