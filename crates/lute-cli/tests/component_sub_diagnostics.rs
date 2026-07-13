//! End-to-end CLI test for dsl 0.5.0 §2.2 "importer-visible component
//! sub-diagnostics": when a `components:` import fails to parse
//! (`E-COMPONENT-PARSE`), the IMPORTING document's output — human AND
//! `--json` — MUST surface the component's own child diagnostics (not just
//! an "(N issue(s))" count), spans relative to the component file. Mirrors
//! the temp-dir harness in `uses_import.rs`.

use std::path::PathBuf;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

/// A fresh unique temp dir (no `tempfile` dev-dep needed for these small tests).
fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-cli-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

// A genuinely broken component body (a garbage line before any heading, no
// sigil/tag shape at all) -> exactly one child E-UNCLASSIFIED parse
// diagnostic, deterministic and independent of any other 0.5.0 §2.1 split.
const COMPONENT: &str = "---\ncomponent: greet\n---\ngarbage line before any heading\n";

fn scene_importing_broken_component() -> &'static str {
    "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\ncomponents: [component.lute]\n---\n\
     ## Shot 1.\n@x: hi\n"
}

fn write_fixture(dir: &PathBuf) -> PathBuf {
    std::fs::write(dir.join("component.lute"), COMPONENT).unwrap();
    let scene = dir.join("scene.lute");
    std::fs::write(&scene, scene_importing_broken_component()).unwrap();
    scene
}

#[test]
fn human_output_surfaces_component_child_diagnostic_detail() {
    let dir = temp_dir("component-subdiag-human");
    let scene = write_fixture(&dir);
    let out = Command::new(BIN)
        .args(["check", scene.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!out.status.success(), "a broken component import must fail");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(combined.contains("E-COMPONENT-PARSE"), "got: {combined}");
    // The parent line still names the file + issue count...
    assert!(combined.contains("component.lute"), "got: {combined}");
    // ...but the author must NOT have to separately re-`check` the component
    // to learn what actually failed: the child diagnostic's own code and
    // message must be printed too, not just the "(N issue(s))" count.
    assert!(
        combined.contains("E-UNCLASSIFIED"),
        "human output must surface the component's own child diagnostic code; got: {combined}"
    );
    assert!(
        combined.contains("unrecognized line"),
        "human output must surface the component's own child diagnostic message, not just a \
         count; got: {combined}"
    );
}

#[test]
fn json_output_carries_structured_component_sub_diagnostics() {
    let dir = temp_dir("component-subdiag-json");
    let scene = write_fixture(&dir);
    let out = Command::new(BIN)
        .args(["check", scene.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "a broken component import must fail");
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let diags = v["diagnostics"].as_array().expect("diagnostics array");
    let parse_diag = diags
        .iter()
        .find(|d| d["code"] == "E-COMPONENT-PARSE" && d["message"].as_str().unwrap_or("").contains("issue"))
        .unwrap_or_else(|| panic!("expected an E-COMPONENT-PARSE parse-error diagnostic; got {diags:#?}"));
    let related = parse_diag["related"]
        .as_array()
        .filter(|r| !r.is_empty())
        .unwrap_or_else(|| panic!("E-COMPONENT-PARSE must carry structured `related` sub-diagnostics; got {parse_diag:#?}"));
    let child = &related[0];
    assert!(
        child["file"].as_str().unwrap_or("").contains("component.lute"),
        "related entry must name the component file; got {child:#?}"
    );
    let cd = &child["diagnostic"];
    assert_eq!(cd["code"], "E-UNCLASSIFIED");
    assert!(cd["message"].as_str().unwrap_or("").contains("unrecognized line"));
    // The span is relative to the COMPONENT file, not the importing scene.
    assert!(cd["span"]["line"].is_number(), "got {cd:#?}");
    assert!(cd["span"]["column"].is_number(), "got {cd:#?}");
}

// --- dsl 0.5.1 §4: the "(N issue(s))" count must equal every surfaced ------
// --- child, including a body diagnostic that reclassifies -----------------

// A component with TWO distinct problems: a genuine parse-level defect (a
// garbage line -> 1 `E-UNCLASSIFIED` child) AND, once the parser resyncs on
// the following `## Shot 2.` heading, a body-level semantic defect -- a
// content line's `when=` guard reading ambient state, which a component
// body may never do (dsl 0.4.0 §6.2) and so RECLASSIFIES to
// `E-COMPONENT-STATE` rather than an ordinary undeclared-state complaint.
const COMPONENT_WITH_TWO_ISSUES: &str = "---\ncomponent: greet\n---\n\
     garbage line before any heading\n## Shot 2.\n@x{when=\"run.flag\"}: hi\n";

fn write_two_issue_fixture(dir: &PathBuf) -> PathBuf {
    std::fs::write(dir.join("component.lute"), COMPONENT_WITH_TWO_ISSUES).unwrap();
    let scene = dir.join("scene.lute");
    std::fs::write(&scene, scene_importing_broken_component()).unwrap();
    scene
}

#[test]
fn issue_count_includes_a_reclassified_component_state_child() {
    let dir = temp_dir("component-subdiag-count");
    let scene = write_two_issue_fixture(&dir);
    let out = Command::new(BIN)
        .args(["check", scene.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "a broken component import must fail");
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let diags = v["diagnostics"].as_array().expect("diagnostics array");

    // The body diagnostic must STILL be surfaced as its own top-level
    // diagnostic too (additive, never a relocation -- an existing consumer
    // scanning the flat list for `E-COMPONENT-STATE` must keep finding it).
    assert!(
        diags.iter().any(|d| d["code"] == "E-COMPONENT-STATE"),
        "the reclassified body diagnostic must still be a top-level diagnostic; got {diags:#?}"
    );

    let parse_diag = diags
        .iter()
        .find(|d| d["code"] == "E-COMPONENT-PARSE" && d["message"].as_str().unwrap_or("").contains("issue"))
        .unwrap_or_else(|| panic!("expected an E-COMPONENT-PARSE parse-error diagnostic; got {diags:#?}"));
    let related = parse_diag["related"].as_array().expect("related array");
    assert_eq!(
        related.len(),
        2,
        "both the parse child AND the reclassified body child must be surfaced: {parse_diag:#?}"
    );
    assert!(
        related.iter().any(|r| r["diagnostic"]["code"] == "E-UNCLASSIFIED"),
        "{related:#?}"
    );
    assert!(
        related.iter().any(|r| r["diagnostic"]["code"] == "E-COMPONENT-STATE"),
        "{related:#?}"
    );
    assert!(
        parse_diag["message"]
            .as_str()
            .unwrap_or("")
            .contains("(2 issue(s))"),
        "the count must equal the 2 surfaced children, not just the 1 raw parse issue: {parse_diag:#?}"
    );
}
