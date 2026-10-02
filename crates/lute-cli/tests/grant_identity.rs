use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value as Json;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/grant-identity")
}

fn play(tag: &str, script: &str) -> Json {
    let dir = std::env::temp_dir().join(format!("lute-grant-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let script_path = dir.join("grant.play.yaml");
    std::fs::write(&script_path, script).unwrap();
    let out: Output = Command::new(BIN)
        .args([
            "play",
            fixture().to_str().unwrap(),
            "--script",
            script_path.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

fn grants<'a>(v: &'a Json, quest: &str) -> Vec<&'a Json> {
    let mut found = Vec::new();
    fn visit<'a>(value: &'a Json, quest: &str, found: &mut Vec<&'a Json>) {
        match value {
            Json::Array(items) => items.iter().for_each(|item| visit(item, quest, found)),
            Json::Object(object) => {
                if object.get("kind") == Some(&Json::String("grant".into()))
                    && object.get("quest") == Some(&Json::String(quest.into()))
                {
                    found.push(value);
                }
                object.values().for_each(|item| visit(item, quest, found));
            }
            _ => {}
        }
    }
    visit(v, quest, &mut found);
    found
}

fn keys(v: &Json, quest: &str) -> Vec<(u64, u64, String)> {
    grants(v, quest)
        .into_iter()
        .map(|grant| {
            (
                grant["instance"].as_u64().unwrap(),
                grant["index"].as_u64().unwrap(),
                grant["objective"].as_str().unwrap_or("").to_owned(),
            )
        })
        .collect()
}

#[test]
fn run_tier_grants_increment_instance_and_count_filtered_rewards() {
    let v = play(
        "run",
        "steps:\n  - engine: { state: { run.ready: true, run.finished: true } }\n  - newRun: { state: { run.ready: true, run.finished: true } }\n",
    );
    let repeat = grants(&v, "repeat");
    assert_eq!(
        repeat.iter().map(|g| g["instance"].as_u64().unwrap()).collect::<Vec<_>>(),
        [1, 1, 1, 2, 2, 2]
    );
    assert_eq!(
        repeat.iter().map(|g| g["index"].as_u64().unwrap()).collect::<Vec<_>>(),
        [0, 2, 0, 0, 2, 0]
    );
    assert!(repeat.iter().all(|g| g["reward"]["kind"] != "FILTERED"));
}

#[test]
fn rearm_increments_instance() {
    let v = play(
        "rearm",
        "steps:\n  - engine: { state: { run.ready: true, run.finished: true } }\n  - engine: { state: { run.rearm: true } }\n",
    );
    assert_eq!(
        grants(&v, "rearmable")
            .iter()
            .map(|g| g["instance"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        [1, 2]
    );
}

#[test]
fn season_reset_increments_instance() {
    let v = play(
        "season",
        "steps:\n  - engine: { state: { run.day: 2, run.finished: true } }\n  - engine: { state: { run.day: 3 } }\n  - engine: { state: { run.day: 4 } }\n",
    );
    assert_eq!(
        grants(&v, "seasonal")
            .iter()
            .map(|g| g["instance"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        [1, 2]
    );
}

#[test]
fn a_save_file_preserves_instance_counter_when_resuming() {
    let v = play(
        "save-file",
        "quests: { repeat: complete }\nquestInstances: { repeat: 1 }\n\
         state: { run.ready: true, run.finished: true }\n\
         steps:\n  - newRun: { state: { run.ready: true, run.finished: true } }\n",
    );
    assert_eq!(
        grants(&v, "repeat")
            .iter()
            .map(|g| g["instance"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        [2, 2, 2]
    );
}

#[test]
fn replaying_a_save_produces_identical_grant_keys() {
    let script = "quests: { repeat: complete }\nstate: { run.ready: true, run.finished: true }\nsteps:\n  - newRun: { state: { run.ready: true, run.finished: true } }\n";
    assert_eq!(keys(&play("replay-a", script), "repeat"), keys(&play("replay-b", script), "repeat"));
}

#[test]
fn reward_index_is_owner_relative_and_filtered_entries_still_count() {
    let v = play(
        "index",
        "steps:\n  - engine: { state: { run.ready: true, run.finished: true } }\n",
    );
    let repeat = grants(&v, "repeat");
    assert_eq!(repeat[0]["index"], 0);
    assert_eq!(repeat[1]["index"], 2);
    assert_eq!(repeat[0]["objective"], "finish");
}
