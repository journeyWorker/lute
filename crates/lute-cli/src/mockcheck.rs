//! `check-project`'s pass over `mocks/*.yaml` (0.10.0 §8, D-E).
//!
//! `lute init` emits `mocks/playthrough.yaml`. The Anseo drive test replaced
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
            let in_mocks = path.parent().and_then(|p| p.file_name()).and_then(|n| n.to_str())
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
) -> std::io::Result<Vec<(PathBuf, Diagnostic)>> {
    // Canonical path -> the already-parsed document and its folded env.
    let mut docs: BTreeMap<PathBuf, (&lute_syntax::ast::Document, &lute_check::FoldedEnv)> =
        BTreeMap::new();
    for group in by_root.values() {
        for (path, doc, folded) in group {
            if let Ok(c) = std::fs::canonicalize(path) {
                docs.insert(c, (doc, folded));
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
        let Some((doc, folded)) = docs.get(&subject) else {
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
        for mut d in lute_trace::validate(&mocks, folded, doc) {
            // Right file, offending key named, impossible position gone: the
            // span the validator produced is `synthetic_span()`'s all-zeros,
            // and pairing it with the MOCK's path is what makes it correct.
            d.message = format!("{} (resolved for `{rel}`)", d.message);
            out.push((mock.clone(), d));
        }
    }
    Ok(out)
}
