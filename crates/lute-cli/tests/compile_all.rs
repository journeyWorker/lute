//! `lute compile --all` acceptance (dsl 0.8.0 adoption surface): project-wide
//! compile plus the `project.index.json` UNION every engine previously had to
//! re-implement by hand (`docs/runtime/execution-model.md`).
//!
//! Three groups: the happy path (both artifacts written, vocabulary unioned +
//! deduplicated + deterministically ordered, `documents` path-sorted), the
//! all-or-nothing failure path (one bad document ⇒ exit 1 and NOTHING written),
//! and the usage gate (exit 2 when `--all` is missing `--project` or `-o`).

use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

/// A fresh unique temp dir (matches `check_project.rs`'s own helper — each
/// integration test binary is compiled separately, so this is intentionally
/// duplicated rather than shared).
fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-compile-all-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &Path, rel: &str, text: &str) -> PathBuf {
    let path = dir.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&path, text).unwrap();
    path
}

fn run(args: &[&str]) -> std::process::Output {
    Command::new(BIN).args(args).output().unwrap()
}

/// A minimal core-only `lute.project.yaml` (mirrors `docs/examples`').
const PROJECT_YAML: &str = "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n";

/// Scene document: entity kind `npc`, relation `knows`, seed fact `knows(kai)`.
const SCENE: &str = "\
---
kind: scene
character: bianca
season: 1
episode: 1
entities:
  npc: { members: [kai, mira] }
relations:
  knows: { args: [npc], tier: run }
facts:
  - \"knows(kai)\"
---

## Opening.

@bianca{code=\"0010\"}: Hello there.

<branch id=\"pick\">
  <choice id=\"go\" label=\"Go on\">
    @bianca{code=\"0020\"}: Onward.
  </choice>
  <choice id=\"stay\" label=\"Stay here\">
    @bianca{code=\"0030\"}: Fine.
  </choice>
</branch>
";

/// Quest document: RE-declares `npc`/`knows` identically (must dedupe, not
/// conflict), repeats the same seed fact (must dedupe), and adds `seen` plus a
/// derived relation with a rule (so `relations`/`rules` have something to
/// union beyond the shared pair).
const QUEST: &str = "\
---
kind: quest
entities:
  npc: { members: [kai, mira] }
relations:
  knows: { args: [npc], tier: run }
  seen: { args: [npc], tier: run }
  anyone: { args: [npc], derive: true }
facts:
  - \"knows(kai)\"
rules:
  - \"anyone(N) :- knows(N)\"
---

<quest id=\"findKai\" title=\"Find Kai\" start=\"true\">
  <objective id=\"meet\" done=\"holds(knows(kai))\">
    @narrator{code=\"0010\"}: You found Kai.
  </objective>
</quest>
";

/// A two-document project, plus a component FRAGMENT that `--all` must skip
/// (it is inlined into its importers, never an artifact of its own).
fn two_doc_project(tag: &str) -> PathBuf {
    let dir = temp_dir(tag);
    write(&dir, "lute.project.yaml", PROJECT_YAML);
    write(&dir, "a.lute", SCENE);
    write(&dir, "quests/q.lute", QUEST);
    write(
        &dir,
        "parts/greet.component.lute",
        "---\ncomponent: greet\n---\n\n## Body.\n\n@narrator: hi\n",
    );
    dir
}

fn read_json(path: &Path) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn names(index: &serde_json::Value, key: &str) -> Vec<String> {
    index[key]
        .as_array()
        .unwrap_or_else(|| panic!("`{key}` must be an array: {index:#?}"))
        .iter()
        .map(|e| e["name"].as_str().unwrap_or_default().to_string())
        .collect()
}

#[test]
fn all_writes_every_artifact_and_a_unioned_index() {
    let dir = two_doc_project("happy");
    let out = temp_dir("happy-out");
    let result = run(&[
        "compile",
        "--all",
        "--project",
        dir.to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
    ]);
    assert_eq!(
        result.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    // Per-document artifacts, at `<outdir>/<path relative to project>.json`,
    // with nested directories created.
    assert!(out.join("a.lute.json").is_file(), "scene artifact");
    assert!(out.join("quests/q.lute.json").is_file(), "nested quest artifact");
    assert!(
        !out.join("parts/greet.component.lute.json").exists(),
        "a component fragment is not an addressable document"
    );

    let index = read_json(&out.join("project.index.json"));
    assert_eq!(index["irVersion"], "0.10.1");
    assert!(
        index["capabilityVersion"].as_str().is_some_and(|s| !s.is_empty()),
        "the index carries the project's one resolved snapshot stamp"
    );

    // `documents` is path-sorted, forward-slashed, and relative on both sides.
    let docs = index["documents"].as_array().unwrap();
    let paths: Vec<&str> = docs.iter().map(|d| d["path"].as_str().unwrap()).collect();
    assert_eq!(paths, vec!["a.lute", "quests/q.lute"], "sorted by path");
    assert_eq!(docs[0]["artifact"], "a.lute.json");
    assert_eq!(docs[1]["artifact"], "quests/q.lute.json");
    assert_eq!(docs[0]["kind"], "scene");
    assert_eq!(docs[0]["key"], "bianca.s01ep01", "canonical scene key");
    assert_eq!(docs[1]["kind"], "quest");
    assert_eq!(docs[1]["key"], "findKai", "the quest's declared id");
    for d in docs {
        let p = d["path"].as_str().unwrap();
        assert!(!p.starts_with('/') && !p.contains('\\'), "relative + forward-slashed: {p}");
    }

    // The union: `npc`/`knows` are declared by BOTH documents identically and
    // appear once; `seen`/`anyone` only by the quest. Name-sorted.
    assert_eq!(names(&index, "entities"), vec!["npc"], "shared entity kind dedupes");
    assert_eq!(
        names(&index, "relations"),
        vec!["anyone", "knows", "seen"],
        "relations union, dedupe, and sort by name"
    );
    assert_eq!(
        index["seedFacts"].as_array().unwrap().len(),
        1,
        "the same ground tuple from two documents is ONE seed fact"
    );
    assert_eq!(
        index["rules"].as_array().unwrap().len(),
        1,
        "the quest's lone rule reaches the index"
    );
    // Always emitted, empty included — an engine unions unconditionally.
    assert!(index["enums"].is_array());
    assert!(index["prereqEdges"].is_array());
}

#[test]
fn all_is_byte_deterministic_across_runs() {
    let dir = two_doc_project("determinism");
    let mut rendered = Vec::new();
    for i in 0..2 {
        let out = temp_dir(&format!("determinism-out{i}"));
        let result = run(&[
            "compile",
            "--all",
            "--project",
            dir.to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
        ]);
        assert_eq!(result.status.code(), Some(0));
        rendered.push((
            std::fs::read_to_string(out.join("project.index.json")).unwrap(),
            std::fs::read_to_string(out.join("a.lute.json")).unwrap(),
        ));
    }
    assert_eq!(rendered[0].0, rendered[1].0, "index must be byte-stable");
    assert_eq!(rendered[0].1, rendered[1].1, "artifacts must be byte-stable");
}

#[test]
fn all_with_a_failing_document_exits_one_and_writes_nothing() {
    let dir = two_doc_project("failing");
    // An undeclared state read: a plain `E-UNDECLARED`, error-grade.
    write(
        &dir,
        "bad.lute",
        "---\nkind: scene\ncharacter: broken\nseason: 1\nepisode: 9\n---\n\n\
         ## Bad.\n\n@x: {{ run.nope }}\n",
    );
    let out = temp_dir("failing-out");
    // The output directory must stay EMPTY — not "mostly written".
    let result = run(&[
        "compile",
        "--all",
        "--project",
        dir.to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
    ]);
    assert_eq!(result.status.code(), Some(1));
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(combined.contains("E-UNDECLARED"), "the diagnostic is printed: {combined}");
    assert!(combined.contains("no output written"), "got: {combined}");
    let written: Vec<_> = std::fs::read_dir(&out).unwrap().flatten().collect();
    assert!(
        written.is_empty(),
        "a failing gate must leave NO partial output: {:?}",
        written.iter().map(|e| e.path()).collect::<Vec<_>>()
    );
}

#[test]
fn all_without_project_is_a_usage_error() {
    let out = temp_dir("usage-noproject");
    let result = run(&["compile", "--all", "-o", out.to_str().unwrap()]);
    assert_eq!(result.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("--all requires --project"), "got: {stderr}");
    assert!(stderr.contains("Usage: lute compile --all"), "got: {stderr}");
}

#[test]
fn all_without_out_is_a_usage_error() {
    let dir = two_doc_project("usage-noout");
    let result = run(&["compile", "--all", "--project", dir.to_str().unwrap()]);
    assert_eq!(result.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("--all requires -o"), "got: {stderr}");
}

#[test]
fn all_with_a_file_argument_is_a_usage_error() {
    let dir = two_doc_project("usage-file");
    let out = temp_dir("usage-file-out");
    let result = run(&[
        "compile",
        "--all",
        "--project",
        dir.to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
        dir.join("a.lute").to_str().unwrap(),
    ]);
    assert_eq!(result.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("--all takes no <FILE>"),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn a_conflicting_cross_document_signature_exits_one_and_writes_nothing() {
    let dir = temp_dir("conflict");
    write(&dir, "lute.project.yaml", PROJECT_YAML);
    write(&dir, "a.lute", SCENE);
    // Same relation NAME, different arity. Neither document is wrong on its
    // own — nothing but the union can see this, which is the point.
    write(
        &dir,
        "b.lute",
        &SCENE
            .replace("character: bianca", "character: kai")
            .replace("knows: { args: [npc], tier: run }", "knows: { args: [npc, npc], tier: run }")
            .replace("\"knows(kai)\"", "\"knows(kai, mira)\""),
    );
    let out = temp_dir("conflict-out");
    let result = run(&[
        "compile",
        "--all",
        "--project",
        dir.to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
    ]);
    assert_eq!(result.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        stderr.contains("relation `knows` is declared with conflicting signatures"),
        "got: {stderr}"
    );
    assert!(stderr.contains("a.lute") && stderr.contains("b.lute"), "both sides named: {stderr}");
    assert!(
        std::fs::read_dir(&out).unwrap().next().is_none(),
        "a conflict writes nothing"
    );
}

/// 0.10.0 §7 / D-S: under a FORCED outer root a nested manifest does not
/// govern. Warn when it would have mattered — a different capability
/// snapshot or different `identity:` templates — and stay silent when it
/// would have resolved identically. Both disjuncts, in one fixture.
#[test]
fn compile_all_warns_only_for_a_nested_manifest_that_would_have_mattered() {
    let dir = temp_dir("project-inert");
    for sub in ["scenes", "same/scenes", "differs/scenes"] {
        std::fs::create_dir_all(dir.join(sub)).unwrap();
    }
    let core = "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n";
    std::fs::write(dir.join("lute.project.yaml"), core).unwrap();
    // Byte-identical resolution to the outer root: no warning.
    std::fs::write(dir.join("same/lute.project.yaml"), core).unwrap();
    // Different `identity:` templates: warns on the identity disjunct alone,
    // with no plugins on disk to install.
    std::fs::write(
        dir.join("differs/lute.project.yaml"),
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n\
         identity:\n  lineId: \"{prefix}-{speaker}-{code}\"\n",
    )
    .unwrap();
    let scene = |c: &str| {
        format!("---\nkind: scene\ncharacter: {c}\nseason: 1\nepisode: 1\n---\n\n## S\n\n@{c}: hi\n")
    };
    std::fs::write(dir.join("scenes/a.lute"), scene("a")).unwrap();
    std::fs::write(dir.join("same/scenes/b.lute"), scene("b")).unwrap();
    std::fs::write(dir.join("differs/scenes/c.lute"), scene("c")).unwrap();

    let out = std::process::Command::new(BIN)
        .args([
            "compile",
            "--all",
            "--project",
            dir.to_str().unwrap(),
            "-o",
            dir.join("out").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code(), Some(0), "a warning must not fail the build:\n{text}");
    assert!(
        text.contains("W-PROJECT-INERT") && text.contains("differs/lute.project.yaml"),
        "the manifest that would have resolved differently must warn, by path:\n{text}"
    );
    assert!(
        !text.contains("same/lute.project.yaml"),
        "a nested manifest that resolves identically is not a signal (D-S):\n{text}"
    );
}

/// D-S's other half: `check-project` resolves each file against its OWN
/// nearest root, so every nested manifest governs and the warning is
/// unreachable. Same fixture, different command, zero warnings.
#[test]
fn check_project_never_reports_project_inert() {
    let dir = temp_dir("project-inert-check");
    std::fs::create_dir_all(dir.join("differs/scenes")).unwrap();
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::write(
        dir.join("lute.project.yaml"),
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("differs/lute.project.yaml"),
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n\
         identity:\n  lineId: \"{prefix}-{speaker}-{code}\"\n",
    )
    .unwrap();
    let scene = |c: &str| {
        format!("---\nkind: scene\ncharacter: {c}\nseason: 1\nepisode: 1\n---\n\n## S\n\n@{c}: hi\n")
    };
    std::fs::write(dir.join("scenes/a.lute"), scene("a")).unwrap();
    std::fs::write(dir.join("differs/scenes/c.lute"), scene("c")).unwrap();

    let out = std::process::Command::new(BIN)
        .args(["check-project", dir.to_str().unwrap()])
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code(), Some(0), "{text}");
    assert!(!text.contains("W-PROJECT-INERT"), "the nearest manifest governs:\n{text}");
}

/// T1.10's headline sentence with a third cause, found while re-investigating
/// it: a nested manifest whose ONLY difference from the invoked root is its
/// `defaults:` block. D-S names the capability snapshot and `identity:`; §6
/// minted `defaults:` in the same release and none of the six manifests D-S
/// was measured against declares one, so it was never in the sample. It is
/// the surface where inertness costs the MOST — every `lineId` in the inner
/// subtree changes and both commands stay at exit 0.
#[test]
fn compile_all_warns_for_a_nested_manifest_that_differs_only_in_defaults() {
    let dir = temp_dir("project-inert-defaults");
    for sub in ["scenes", "same/scenes", "differs/scenes"] {
        std::fs::create_dir_all(dir.join(sub)).unwrap();
    }
    let outer = "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n\
                 defaults:\n  kind: scene\n  character: outerguy\n  season: 9\n  episode: 9\n";
    std::fs::write(dir.join("lute.project.yaml"), outer).unwrap();
    // Same capability, same (absent) identity, SAME defaults: still silent.
    std::fs::write(dir.join("same/lute.project.yaml"), outer).unwrap();
    // Same capability, same (absent) identity, DIFFERENT defaults.
    std::fs::write(
        dir.join("differs/lute.project.yaml"),
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n\
         defaults:\n  kind: scene\n  character: innerguy\n  season: 1\n  episode: 1\n",
    )
    .unwrap();
    // Frontmatter carrying NOTHING the manifest supplies, so the resolved
    // key is entirely the governing root's.
    let scene = |c: &str| format!("---\ntitle: T\n---\n\n## S\n\n@{c}{{code=\"0010\"}}: hi\n");
    std::fs::write(dir.join("scenes/a.lute"), scene("outerguy")).unwrap();
    std::fs::write(dir.join("same/scenes/b.lute"), scene("outerguy")).unwrap();
    std::fs::write(dir.join("differs/scenes/c.lute"), scene("innerguy")).unwrap();

    let out = std::process::Command::new(BIN)
        .args([
            "compile",
            "--all",
            "--project",
            dir.to_str().unwrap(),
            "-o",
            dir.join("out").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code(), Some(0), "{text}");
    assert!(
        text.contains("W-PROJECT-INERT") && text.contains("differs/lute.project.yaml"),
        "a `defaults:`-only difference is still a difference:\n{text}"
    );
    assert!(
        text.contains("`defaults:` block"),
        "the message must name WHICH surface differs, as the other two do:\n{text}"
    );
    assert!(
        !text.contains("same/lute.project.yaml"),
        "identical `defaults:` is not a signal (D-S's narrowing survives):\n{text}"
    );

    // What the warning is ABOUT, measured: the inner document's key is the
    // OUTER root's defaults, not its own root's. Without this the test would
    // pass against a warning that fired for no reason.
    let art = std::fs::read_to_string(dir.join("out/differs/scenes/c.lute.json")).unwrap();
    assert!(
        art.contains("outerguy.s09ep09.innerguy_0010"),
        "the inner subtree is keyed by the outer root: {art}"
    );
    assert!(
        !art.contains("innerguy.s01ep01."),
        "its own root's defaults are not applied — that is the inertness: {art}"
    );
}
