//! `check-project`'s pass over `mocks/*.yaml` (0.10.0 §8, D-E).
//!
//! `lute init` emits `mocks/playthrough.yaml`. The Haven drive test replaced
//! `world.schema.yaml` and deleted `scenes/opening.lute`, and `check-project`
//! stayed `ok`, exit 0, with a mock in the tree naming a state path that no
//! longer exists and a scene file that no longer exists (T1.9). The rule that
//! would have caught it has existed since 0.4.0 §4.3 — "state-by-typo MUST
//! fail in mocks exactly as in documents" — but fired only when a human ran
//! `trace` with the right two arguments, and then reported at
//! `scenes/wake.lute:0:0`: a source diagnostic against a file that is not at
//! fault, at an impossible position, for a defect in a YAML file.
//!
//! ## The glob is `mocks/*.yaml` and nothing else
//! Two neighbouring file sets look like they belong and do not. A
//! `*.test.yaml` already carries a required `file:` read by the same parser
//! and is already validated when `lute test` runs it, so sweeping it here
//! would be a second enforcement of a rule that has an owner. A
//! `conformance/*/mock.yaml` is a fixture whose whole purpose is to pin
//! behaviour, including behaviour this pass would call wrong.
//!
//! ## E-MOCK-SUBJECT suppresses the rest, and that is forced
//! With no resolvable subject there is no resolved schema and no parsed
//! document, so there is nothing for the remaining rules to decide. A
//! subject-less mock is reported ONCE. The author supplies `file:`, the
//! schema becomes knowable, and whatever else is wrong is caught next run.
//!
//! ## What it does NOT own
//! `E-TRACE-CHOICE` is two rules under one name. The STRUCTURAL half — a
//! `choose:` naming a branch/hub id, or a choice id beneath it, absent from
//! the subject document — is a pure id lookup over the parsed document,
//! statically decidable, and it is what runs here. The WALK half — a forced
//! choice whose `when=` guard decides false at its presentation point —
//! depends on in-flow writes the walk has not applied yet, which is why the
//! pre-walk validator deliberately does not evaluate guards. It already
//! anchors at the `<choice>` the diagnostic is about and it keeps that
//! anchor. Nothing here moves a correctly anchored document diagnostic into a
//! YAML file.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use lute_core_span::Diagnostic;

/// Every `mocks/*.yaml` under `dir`: a `.yaml` file whose immediate parent
/// directory is named `mocks`, excluding `*.test.yaml` (owned by `lute
/// test`). Byte-sorted.
fn find_mocks(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d)? {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                stack.push(path);
                continue;
            }
            let in_mocks = path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                == Some("mocks");
            let is_yaml = path.extension().and_then(|e| e.to_str()) == Some("yaml");
            let is_test = path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(".test.yaml"));
            if in_mocks && is_yaml && !is_test {
                out.push(path);
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Validate every `mocks/*.yaml` under `dir` against the schema resolved for
/// its `file:` subject. Returns `(mock path, diagnostic)` pairs for
/// `run_check_project`'s project-wide list, which already renders a spanless
/// diagnostic as `path: severity [CODE] message` (D-AB).
///
/// Every subject is looked up in `by_root` — the documents `check-project`
/// already parsed and folded — so no document is parsed twice and every mock
/// is validated against exactly the schema its subject really resolves.
pub fn check_mocks_under(
    dir: &Path,
    by_root: &crate::ByRoot,
    inputs: &BTreeMap<PathBuf, (PathBuf, lute_check::CheckInput)>,
) -> std::io::Result<Vec<(PathBuf, Diagnostic)>> {
    // Canonical path -> the already-parsed document, its folded env, and the
    // check input it resolved: its capability snapshot (the plugin calls
    // `bridges:` answers) and its text (what content reads, dsl 0.25.0 §7).
    let mut docs: BTreeMap<
        PathBuf,
        (
            &lute_syntax::ast::Document,
            &lute_check::FoldedEnv,
            Option<&lute_check::CheckInput>,
        ),
    > = BTreeMap::new();
    for group in by_root.values() {
        for (path, doc, folded) in group {
            if let Ok(c) = std::fs::canonicalize(path) {
                let input = inputs.get(path).map(|(_, input)| input);
                docs.insert(c, (doc, folded, input));
            }
        }
    }

    let mut out = Vec::new();
    for mock in find_mocks(dir)? {
        let text = match std::fs::read_to_string(&mock) {
            Ok(t) => t,
            Err(e) => {
                out.push((
                    mock.clone(),
                    crate::manifests::as_diagnostic(
                        lute_trace::E_TRACE_MOCK_PARSE,
                        format!("cannot read mock: {e}"),
                    ),
                ));
                continue;
            }
        };

        // 1. The subject. With no resolvable subject there is no resolved
        //    schema, so this arm ALWAYS `continue`s: one report per mock.
        let rel = match lute_trace::mock_subject(&text) {
            Ok(Some(rel)) => rel,
            Ok(None) => {
                out.push((
                    mock.clone(),
                    crate::manifests::as_diagnostic(
                        lute_trace::E_MOCK_SUBJECT,
                        "mock declares no `file:` — name the document it previews, relative to \
                         this file (0.10.0 §8)"
                            .to_string(),
                    ),
                ));
                continue;
            }
            Err(d) => {
                out.push((mock.clone(), d));
                continue;
            }
        };
        let base = mock.parent().unwrap_or(dir);
        let Ok(subject) = std::fs::canonicalize(base.join(&rel)) else {
            out.push((
                mock.clone(),
                crate::manifests::as_diagnostic(
                    lute_trace::E_MOCK_SUBJECT,
                    format!("`file: {rel}` names a path that does not exist (0.10.0 §8)"),
                ),
            ));
            continue;
        };
        let Some((doc, folded, input)) = docs.get(&subject) else {
            out.push((
                mock.clone(),
                crate::manifests::as_diagnostic(
                    lute_trace::E_MOCK_SUBJECT,
                    format!(
                        "`file: {rel}` does not name a `.lute` document under {} (0.10.0 §8)",
                        dir.display()
                    ),
                ),
            ));
            continue;
        };

        // 2. The mock's own four surfaces, then the pre-walk validator.
        let mocks = match lute_trace::parse_mock_yaml(&text) {
            Ok(m) => m,
            Err(d) => {
                out.push((mock.clone(), d));
                continue;
            }
        };
        let mut diags = lute_trace::validate(&mocks, folded, doc);
        if let Some(input) = input {
            let reads = lute_trace::content_read_paths(&input.text, &folded.def_bodies);
            diags.extend(lute_trace::validate_bridges(
                &mocks,
                folded,
                &input.snapshot,
                &reads,
            ));
        }
        for mut d in diags {
            // Right file, offending key named, impossible position gone: the
            // span the validator produced is `synthetic_span()`'s all-zeros
            // (printed without a position) or, for a `bridges:` entry, the
            // entry's own line:column in THIS mock — either is correct only
            // paired with the MOCK's path.
            d.message = format!("{} (resolved for `{rel}`)", d.message);
            out.push((mock.clone(), d));
        }
    }
    Ok(out)
}

/// dsl 0.24.0 §2, 0.25.0 §5: per quest id, every trace mock
/// (`mocks/*.yaml`) or scenario test (`*.test.yaml`) under the project root
/// `root` whose top-level `accept:` / `accepts:` list names it, as
/// root-relative paths in path order. No acceptance source — a mock proves
/// a test, not the game — only what `W-QUEST-NEVER-ACCEPTED` names when it
/// warns anyway. Best effort: an unreadable or malformed file contributes
/// nothing (the mock pass above and `lute test` report it), and a nested
/// project root (a subdirectory with its own `lute.project.yaml`) is left
/// to its own pass.
pub fn mocked_accepts_under(root: &Path) -> BTreeMap<String, Vec<PathBuf>> {
    let mut out: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                if !path.join("lute.project.yaml").is_file() {
                    stack.push(path);
                }
                continue;
            }
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            let in_mocks = path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                == Some("mocks");
            if !(name.ends_with(".test.yaml") || (in_mocks && name.ends_with(".yaml"))) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(serde_yaml::Value::Mapping(top)) = serde_yaml::from_str(&text) else {
                continue;
            };
            let rel = path.strip_prefix(root).unwrap_or(&path);
            for key in ["accept", "accepts"] {
                if let Some(serde_yaml::Value::Sequence(items)) = top.get(key) {
                    for id in items.iter().filter_map(|i| i.as_str()) {
                        let files = out.entry(id.to_string()).or_default();
                        if !files.iter().any(|f| f == rel) {
                            files.push(rel.to_path_buf());
                        }
                    }
                }
            }
        }
    }
    for files in out.values_mut() {
        files.sort();
    }
    out
}
