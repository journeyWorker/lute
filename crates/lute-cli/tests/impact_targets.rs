use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_dir(tag: &str) -> PathBuf {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-impact-{tag}-{}-{id}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &Path, relative: &str, text: &str) {
    let path = dir.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn run(dir: &Path, target: &str) -> Output {
    Command::new(BIN)
        .args(["impact", dir.to_str().unwrap(), target, "--json"])
        .output()
        .unwrap()
}

fn fixture(tag: &str) -> PathBuf {
    let dir = temp_dir(tag);
    write(
        &dir,
        "lute.project.yaml",
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\ndefaults:\n  uses: [world.schema.yaml]\n  components: [components/greeting.component.lute]\n",
    );
    write(
        &dir,
        "world.schema.yaml",
        "state:\n  run.score: { type: int, default: 0 }\nentities:\n  npc: { members: [kai] }\nrelations:\n  knows: { args: [npc], tier: run }\nfacts:\n  - \"knows(kai)\"\ndefs:\n  ready: \"run.score >= 0\"\n",
    );
    write(
        &dir,
        "scenes/one.lute",
        "---\nkind: scene\nid: scene.one\n---\n\n## One\n\n@narrator{when=\"@ready\"}: Hello.\n::use{component=\"greeting\"}\n",
    );
    write(
        &dir,
        "quests/q.lute",
        "---\nkind: quest\nid: quest.doc\n---\n\n<quest id=\"q\" start=\"true\">\n  <objective id=\"o\" done=\"run.score >= 1\">\n    @narrator: Done.\n  </objective>\n</quest>\n",
    );
    write(
        &dir,
        "lore/l.lute",
        "---\nkind: lore\nid: lore\n---\n\n<entry id=\"e\" when=\"entry.e.read\">\n  @narrator: Entry.\n</entry>\n\n<beat id=\"b\" on=\"talk\">\n  @narrator: Beat.\n</beat>\n",
    );
    write(
        &dir,
        "components/greeting.component.lute",
        "---\ncomponent: greeting\n---\n\n@narrator: Hi.\n",
    );
    dir
}

fn assert_target(dir: &Path, target: &str, kind: &str) {
    let output = run(dir, target);
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(0), "{target}: {combined}");
    let report: Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("{target}: {error}: {combined}"));
    assert_eq!(report["target"]["kind"], kind, "{target}: {report}");
}

#[test]
fn impact_state_target() {
    assert_target(&fixture("state"), "state:run.score", "state");
}
#[test]
fn impact_line_items_include_declaration_metadata() {
    let dir = fixture("line-metadata");
    let output = run(&dir, "state:run.score");
    assert!(output.status.success(), "{:?}", output);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    let line = report["items"]["lines"]
        .as_array()
        .unwrap()
        .first()
        .expect("line impact");
    assert!(line["lineId"].is_string(), "{line}");
    assert!(line["speaker"].is_string(), "{line}");
    assert!(line["file"].is_string(), "{line}");
    assert!(line["line"].is_u64(), "{line}");
}

#[test]
fn impact_fact_target() {
    assert_target(&fixture("fact"), "fact:knows(kai)", "fact");
}

#[test]
fn impact_relation_target() {
    assert_target(&fixture("relation"), "relation:knows", "relation");
}

#[test]
fn impact_scene_target() {
    assert_target(&fixture("scene"), "scene:scene.one", "scene");
}

#[test]
fn impact_quest_target() {
    assert_target(&fixture("quest"), "quest:q", "quest");
}

#[test]
fn impact_objective_target() {
    assert_target(&fixture("objective"), "objective:q.o", "objective");
}

#[test]
fn impact_entry_target() {
    assert_target(&fixture("entry"), "entry:e", "entry");
}

#[test]
fn impact_beat_target() {
    assert_target(&fixture("beat"), "beat:lore.b", "beat");
}

#[test]
fn impact_occasion_target() {
    assert_target(&fixture("occasion"), "occasion:talk", "occasion");
}

#[test]
fn impact_definition_target() {
    assert_target(&fixture("def"), "def:ready", "def");
}

#[test]
fn impact_component_target() {
    assert_target(&fixture("component"), "component:greeting", "component");
}

#[test]
fn malformed_impact_targets_are_usage_errors() {
    let dir = fixture("malformed");
    for target in ["", "unknown:value", "state:", "state:run..score", "fact:knows(kai"] {
        let output = run(&dir, target);
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.status.code(), Some(2), "{target:?}: {combined}");
        assert!(combined.contains("lute impact"), "{target:?}: {combined}");
    }
}
