//! dsl 0.23.0 under `lute play`: `prev.run.*` is snapshotted at `newRun`
//! (§6) and a reward kind's `credits:` path receives the granted amount (§8).

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
