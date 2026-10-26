//! dsl 0.52.0 §5.1–§5.3 (#216): a forward `::jump` out of a hub option body
//! leaves the hub. The issue's reproduction through `lute check` and
//! `lute play`, the compiled artifact through `lute run`, and a hand-edited
//! `leaves` the loader refuses. On v0.51.0 the same play presents the hub
//! again after `take` (`▷ hub desk: take [leave]`, then `Leaving.`).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value as Json;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

/// The issue's `hub.lute`.
const ISSUE_SCENE: &str = "---\nkind: scene\nid: hub\ntitle: hub\non: visit\npriority: 0\n---\n\
## Hub\n@narrator{code=\"0010\"}: Before the hub.\n<hub id=\"desk\">\n  <choice id=\"take\" \
text=\"Take it\">\n    @narrator{code=\"0020\"}: Taking it, then jumping past the hub.\n    \
::jump{to=\"after\"}\n  </choice>\n  <choice id=\"leave\" text=\"Leave\" exit>\n    \
@narrator{code=\"0030\"}: Leaving.\n  </choice>\n</hub>\n::label{name=\"after\"}\n\
@narrator{code=\"0040\"}: After the hub.\n";

/// The issue's `script.play.yaml`.
const ISSUE_SCRIPT: &str =
    "choose:\n  desk: [take, leave]\nsteps:\n  - occasion: visit\nexpect:\n  end: complete\n";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn lute(dir: &Path, args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .current_dir(dir)
        .env("LUTE_TELEMETRY", "0")
        .output()
        .unwrap()
}

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// `conformance/session/hub-once/project` with the issue's scene and script.
fn issue_project(tag: &str) -> lute_scratch::ScratchDir {
    let dir = lute_scratch::ScratchDir::named(tag);
    let base = root().join("conformance/session/hub-once/project");
    for rel in [
        "lute.project.yaml",
        "world.schema.yaml",
        "plugins/session.cases/plugin.yaml",
        "plugins/session.cases/occasions/session.yaml",
    ] {
        let to = dir.join(rel);
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::copy(base.join(rel), &to).unwrap();
    }
    std::fs::write(dir.join("hub.lute"), ISSUE_SCENE).unwrap();
    std::fs::write(dir.join("script.play.yaml"), ISSUE_SCRIPT).unwrap();
    dir
}

#[test]
fn issue_216_take_continues_after_the_hub() {
    let dir = issue_project("hub-leave-216");
    let check = lute(&dir, &["check", "--project", "."]);
    assert!(check.status.success(), "{}", text(&check));
    let play = lute(
        &dir,
        &["play", "--project", ".", "--script", "script.play.yaml"],
    );
    assert!(play.status.success(), "{}", text(&play));
    let transcript: Vec<String> = String::from_utf8_lossy(&play.stdout)
        .lines()
        .filter(|l| l.starts_with('@') || l.starts_with('▷'))
        .map(str::to_string)
        .collect();
    assert_eq!(
        transcript,
        [
            "@narrator: Before the hub.",
            "▷ hub desk: [take] leave        ← chosen: take",
            "@narrator: Taking it, then jumping past the hub.",
            "@narrator: After the hub.",
        ]
    );
}

/// `lute trace` follows the runtime: one presentation of `desk`, then the
/// line after the hub (v0.51.0 traced `<hub desk> … -> leave` again).
#[test]
fn issue_216_trace_leaves_the_hub() {
    let dir = issue_project("hub-leave-trace");
    std::fs::write(dir.join("mock.yaml"), "choose:\n  desk: [take, leave]\n").unwrap();
    let trace = lute(
        &dir,
        &["trace", "hub.lute", "--project", ".", "--mock", "mock.yaml"],
    );
    assert!(trace.status.success(), "{}", text(&trace));
    let stdout = String::from_utf8_lossy(&trace.stdout);
    assert_eq!(stdout.matches("<hub desk>").count(), 1, "{stdout}");
    assert!(!stdout.contains("Leaving."), "{stdout}");
    assert!(stdout.contains("After the hub."), "{stdout}");
}

/// The compiled artifact through `lute run`: the jump carries `leaves: 1`,
/// the exit option's converge jump none, and `requiredSemantics` names
/// `lute.core.hubLeave/1`.
#[test]
fn the_compiled_issue_scene_runs_past_the_hub() {
    let dir = issue_project("hub-leave-run");
    let compile = lute(&dir, &["compile", "--project", ".", "-o", "out"]);
    assert!(compile.status.success(), "{}", text(&compile));
    let artifact: Json =
        serde_json::from_str(&std::fs::read_to_string(dir.join("out/hub.lute.json")).unwrap())
            .unwrap();
    let jumps: Vec<Option<&Json>> = artifact["commands"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["kind"] == "jump")
        .map(|c| c.get("leaves"))
        .collect();
    assert_eq!(jumps, [Some(&Json::from(1)), None]);
    let required: Vec<&str> = artifact["requiredSemantics"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Json::as_str)
        .collect();
    assert!(required.contains(&"lute.core.hubLeave/1"), "{required:?}");

    std::fs::write(dir.join("mock.yaml"), "choose:\n  desk: [take, leave]\n").unwrap();
    let run = lute(&dir, &["run", "out/hub.lute.json", "--mock", "mock.yaml"]);
    assert!(run.status.success(), "{}", text(&run));
    let lines: Vec<&str> = std::str::from_utf8(&run.stdout)
        .unwrap()
        .lines()
        .filter(|l| l.contains("narrator:") || l.contains("hub    ["))
        .map(str::trim)
        .collect();
    assert_eq!(
        lines,
        [
            "001-0100  narrator: Before the hub.",
            "001-0200  hub    [desk] -> take",
            "001-0300  narrator: Taking it, then jumping past the hub.",
            "001-0700  narrator: After the hub.",
        ]
    );
}

/// A quest segment never receives a leave: a quest admits no `<hub>`, so a
/// hand-edited `leaves` on a jump in an objective body is refused at load.
#[test]
fn a_leave_in_a_quest_body_is_refused_at_load() {
    let dir = lute_scratch::ScratchDir::named("hub-leave-quest");
    std::fs::write(
        dir.join("q.lute"),
        "---\nkind: quest\nstate:\n  run.d: { type: bool, default: true }\n---\n<quest id=\"q\" \
         title=\"Q\" tier=\"run\" start=\"true\">\n  <objective id=\"o\" title=\"O\" \
         done=\"run.d\">\n    @narrator: Done.\n    ::jump{to=\"end\"}\n    @narrator: Skipped.\n    \
         ::label{name=\"end\"}\n    @narrator: End.\n  </objective>\n</quest>\n",
    )
    .unwrap();
    let compile = lute(&dir, &["compile", "q.lute", "-o", "q.json"]);
    assert!(compile.status.success(), "{}", text(&compile));
    let mut artifact: Json =
        serde_json::from_str(&std::fs::read_to_string(dir.join("q.json")).unwrap()).unwrap();
    let jump = artifact["commands"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|c| c["kind"] == "jump")
        .expect("the objective body's jump");
    assert!(jump.get("leaves").is_none(), "{jump}");
    jump["leaves"] = Json::from(1);
    std::fs::write(dir.join("bad.json"), artifact.to_string()).unwrap();
    let run = lute(&dir, &["run", "bad.json"]);
    assert_eq!(run.status.code(), Some(2), "{}", text(&run));
    assert!(
        text(&run).contains(
            "malformed `jump` record at 001-0300: `leaves` is 1, but reaching its target \
             001-0500 leaves 0 hub option or `<return>` bodies"
        ),
        "{}",
        text(&run)
    );
}

/// `E-JUMP-INTO-BODY` through `lute check`: on v0.51.0 a jump from one hub
/// option into a sibling option's body checks clean.
#[test]
fn a_jump_into_a_sibling_option_is_refused() {
    let dir = issue_project("hub-leave-sibling");
    std::fs::write(
        dir.join("hub.lute"),
        ISSUE_SCENE
            .replace("::label{name=\"after\"}\n", "")
            .replace(
                "    @narrator{code=\"0030\"}: Leaving.\n",
                "    ::label{name=\"after\"}\n    @narrator{code=\"0030\"}: Leaving.\n",
            ),
    )
    .unwrap();
    let check = lute(&dir, &["check", "--project", "."]);
    assert!(!check.status.success());
    assert!(
        text(&check).contains(
            "error [E-JUMP-INTO-BODY] `::jump{to=\"after\"}` enters the body of hub `desk`'s \
             option `leave`, which runs only when that option is chosen — jump to a label in the \
             same body, or after the hub"
        ),
        "{}",
        text(&check)
    );
}
