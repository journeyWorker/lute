//! dsl 0.23.0 under `lute play`: `prev.run.*` is snapshotted at `newRun`
//! (§6) and a reward kind's `credits:` path receives the granted amount (§8).
//! dsl 0.24.0 §1: an enum member's display label renders in `{{…}}`.

use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn write(dir: &Path, rel: &str, text: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

fn project() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lute-vocab-play-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    write(
        &dir,
        "lute.project.yaml",
        "pluginsDir: plugins/\ndefaultProfile: game\nprofiles:\n  game:\n    plugins: { game.v: true }\n\
         defaults:\n  uses: [world.schema.yaml]\n",
    );
    write(
        &dir,
        "plugins/game.v/plugin.yaml",
        "id: game.v\nversion: 0.1.0\nkind: capability\ndepends: [ { id: lute.core, range: \"^0.0.1\" } ]\n\
         exports:\n  occasions: occasions/\n  rewardkinds: rewardkinds/\n",
    );
    write(
        &dir,
        "plugins/game.v/occasions/o.yaml",
        "occasions:\n  hubVisit: {}\n  runEnd: {}\n",
    );
    write(
        &dir,
        "plugins/game.v/rewardkinds/k.yaml",
        "rewardKinds:\n  EMBERS: { credits: user.embers }\n",
    );
    write(
        &dir,
        "world.schema.yaml",
        "state:\n  run.floor: { type: number, default: 0 }\n  user.embers: { type: number, default: 0 }\n",
    );
    write(
        &dir,
        "quests/climb.lute",
        "---\nkind: quest\n---\n<quest id=\"climb\" start=\"true\">\n  <reward kind=\"EMBERS\" amount=\"100\"/>\n  \
         <objective id=\"out\" on=\"runEnd\" done=\"run.floor >= 3\"/>\n</quest>\n",
    );
    write(
        &dir,
        "scenes/hub.lute",
        "---\nkind: scene\nid: hub\non: hubVisit\nonce: false\n---\n## Shot 1.\n\
         @narrator{when=\"isSet(prev.run.floor) && prev.run.floor >= 3\"}: High last time.\n\
         @narrator: The hearth.\n",
    );
    dir
}

#[test]
fn new_run_snapshots_prev_run_and_a_grant_credits_its_path() {
    let dir = project();
    write(
        &dir,
        "s.play.yaml",
        "steps:\n  - engine: { state: { run.floor: 4 } }\n  - occasion: runEnd\n  - newRun: true\n  \
         - occasion: hubVisit\nexpect:\n  state: { prev.run.floor: 4, run.floor: 0, user.embers: 100 }\n",
    );
    let out = Command::new(BIN)
        .args(["play", "--json", "--script"])
        .arg(dir.join("s.play.yaml"))
        .arg(&dir)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{text}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // The grant record names where the amount landed.
    assert!(
        text.contains("\"credited\"") && text.contains("\"path\": \"user.embers\""),
        "{text}"
    );
    // The guarded `prev.run.floor` line played in the new run.
    assert!(text.contains("High last time."), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// dsl 0.24.0 §1: a state path typed against a named enum renders the
/// member's declared label in `{{…}}` — in `lute play` (the reference runner,
/// reading the artifact's `state[].labels`) and in `lute trace`/`lute test`
/// (reading the checker's merged vocabulary) alike; a member without a label
/// renders its id. The typed path reads the domain, so it is not unread.
#[test]
fn an_enum_member_label_renders_in_play_trace_and_test() {
    let dir = std::env::temp_dir().join(format!("lute-vocab-labels-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    write(
        &dir,
        "lute.project.yaml",
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n\
         defaults:\n  uses: [world.schema.yaml]\n",
    );
    write(
        &dir,
        "world.schema.yaml",
        "enums:\n  weekday:\n    members: [mon, sun]\n    labels: { sun: Sunday }\n\
         state:\n  run.wd: { type: { domain: weekday }, default: mon }\n",
    );
    write(
        &dir,
        "scenes/day.lute",
        "---\nkind: scene\nid: town.day\non: dawn\n---\n\n## Day\n\n\
         @narrator: Today is {{run.wd}}.\n::set{run.wd = \"sun\"}\n@narrator: Now it is {{run.wd}}.\n",
    );
    write(&dir, "s.play.yaml", "steps:\n  - occasion: dawn\n");
    write(
        &dir,
        "tests/day.test.yaml",
        "file: ../scenes/day.lute\nexpect:\n  transcriptContains:\n    \
         - \"Today is mon.\"\n    - \"Now it is Sunday.\"\n",
    );
    let lute = |args: &[&str]| {
        let out = Command::new(BIN).args(args).output().unwrap();
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        (out.status.code(), text)
    };
    let d = dir.to_str().unwrap();
    let scene = dir.join("scenes/day.lute");
    let scene = scene.to_str().unwrap();

    let script = dir.join("s.play.yaml");
    let (code, text) = lute(&["play", d, "--script", script.to_str().unwrap()]);
    assert_eq!(code, Some(0), "{text}");
    assert!(text.contains("Today is mon.") && text.contains("Now it is Sunday."), "{text}");

    let (code, text) = lute(&["trace", scene, "--project", d]);
    assert_eq!(code, Some(0), "{text}");
    assert!(text.contains("Today is mon.") && text.contains("Now it is Sunday."), "{text}");

    let tests = dir.join("tests");
    let (code, text) = lute(&["test", tests.to_str().unwrap(), "--project", d]);
    assert_eq!(code, Some(0), "{text}");
    assert!(text.contains("1 passed, 0 failed"), "{text}");

    // The engine contract: the path's state entry carries the labels.
    let out = Command::new(BIN).args(["compile", scene, "--project", d]).output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    let art: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let entry = art["state"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["path"] == "run.wd")
        .cloned()
        .unwrap();
    assert_eq!(entry["labels"], serde_json::json!({ "sun": "Sunday" }), "{entry}");

    let (code, text) = lute(&["check-project", d]);
    assert_eq!(code, Some(0), "{text}");
    assert!(!text.contains("W-DOMAIN-UNREAD"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}
