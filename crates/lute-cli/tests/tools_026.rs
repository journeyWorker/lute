//! dsl 0.26.0 §8 tools: `W-BEAT-PRIORITY-TIE` under the fact envelope's
//! must set (T3-2), `lute beats`' `covered by` verdict (T3-3), `lute scenario
//! --facts` (T3-11) and `lute doctor --strict` (T3-12).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lute-tools026-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, text).unwrap();
}

fn run(args: &[&str]) -> Output {
    Command::new(BIN).args(args).output().unwrap()
}

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

const SCHEMA: &str = "state:\n  run.day: { type: number, default: 0 }\n\
entities:\n  person: { members: [maud] }\n  item: { members: [lens] }\n\
relations:\n  met: { args: [person], tier: run }\n  hasItem: { args: [item], tier: run }\n  \
canPass: { args: [item], derive: true }\n\
rules:\n  - \"canPass(lens) :- hasItem(lens)\"\n";

fn scene(id: &str, fm: &str, body: &str) -> String {
    format!(
        "---\nkind: scene\nid: {id}\nuses: ../world.schema.yaml\n{fm}---\n\n## {id}\n\n@narrator: {id}.\n{body}"
    )
}

/// `intro` asserts `met(maud)` on every route; `hall` follows it.
fn project(tag: &str) -> PathBuf {
    let dir = temp_dir(tag);
    write(
        &dir,
        "lute.project.yaml",
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n",
    );
    write(&dir, "world.schema.yaml", SCHEMA);
    write(
        &dir,
        "scenes/intro.lute",
        &scene(
            "intro",
            "on: arrive\n",
            "::assert{met(maud)}\n::assert{hasItem(lens)}\n",
        ),
    );
    write(
        &dir,
        "scenes/hall.lute",
        &scene(
            "hall",
            "on: knock\nafter: \"visited('intro')\"\nwhen: 'run.day > 1'\n",
            "",
        ),
    );
    write(
        &dir,
        "scenes/tower.lute",
        &scene("tower", "on: climb\nwhen: 'holds(canPass(lens))'\n", ""),
    );
    dir
}

#[test]
fn the_tie_check_reads_the_must_set_at_a_beats_slot() {
    let dir = project("must");
    // At `hall`'s slot `met(maud)` holds on every route: the stranger's
    // `!holds(met(maud))` cannot hold with it.
    write(
        &dir,
        "lore/barks.lute",
        "---\nkind: lore\nid: barks\nuses: ../world.schema.yaml\n---\n\
         <entry id=\"stranger\" on=\"knock\" when=\"!holds(met(maud))\">\n@narrator: Who?\n</entry>\n",
    );
    let out = run(&["check-project", dir.to_str().unwrap()]);
    let t = text(&out);
    assert!(!t.contains("W-BEAT-PRIORITY-TIE"), "{t}");
}

#[test]
fn beats_shows_a_fallback_covered_by_an_earlier_beat() {
    let dir = project("covered");
    write(
        &dir,
        "lore/gates.lute",
        "---\nkind: lore\nid: gates\nuses: ../world.schema.yaml\n---\n\
         <entry id=\"areaGate\" on=\"gate\" when=\"!holds(canPass(lens))\">\n@narrator: Area.\n</entry>\n\
         <entry id=\"spineGate\" on=\"gate\" priority=\"-10\" when=\"!holds(canPass(lens))\">\n@narrator: Spine.\n</entry>\n\
         <entry id=\"liveGate\" on=\"gate\" priority=\"-20\" when=\"run.day > 3\">\n@narrator: Live.\n</entry>\n",
    );
    let out = run(&["beats", dir.to_str().unwrap(), "--occasion", "gate"]);
    let t = text(&out);
    let row = |id: &str| {
        t.lines()
            .find(|l| l.contains(id))
            .unwrap_or_default()
            .to_string()
    };
    assert!(row("spineGate").contains("covered by areaGate"), "{t}");
    assert!(!row("liveGate").contains("covered by"), "{t}");
    let json = run(&["beats", dir.to_str().unwrap(), "--json"]);
    assert!(
        text(&json).contains("\"coveredBy\": \"areaGate\""),
        "{}",
        text(&json)
    );
}

#[test]
fn scenario_facts_draws_producer_edges_and_layers_over_them() {
    let dir = project("facts");
    let plain = text(&run(&["scenario", dir.to_str().unwrap()]));
    assert!(!plain.contains("fact edges"), "{plain}");
    let out = run(&["scenario", dir.to_str().unwrap(), "--facts"]);
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(
        t.contains("scene(intro) -> scene(tower) [hasItem(lens), via canPass(lens)]"),
        "{t}"
    );
    // `tower` moves out of layer 0 behind its producer.
    let layer0 = t.lines().find(|l| l.contains("layer 0:")).unwrap();
    assert!(!layer0.contains("scene(tower)"), "{t}");
    let json = text(&run(&[
        "scenario",
        dir.to_str().unwrap(),
        "--facts",
        "--format",
        "json",
    ]));
    assert!(json.contains("\"factEdges\""), "{json}");
    assert!(json.contains("\"via\": \"canPass(lens)\""), "{json}");
    // A sub-view has no graph to draw edges on.
    let bad = run(&[
        "scenario",
        dir.to_str().unwrap(),
        "--facts",
        "reach",
        "intro",
    ]);
    assert_eq!(bad.status.code(), Some(2), "{}", text(&bad));
}

#[test]
fn doctor_strict_fails_on_any_failed_check() {
    // No `.lute` file and no manifest: `✗` checks.
    let dir = temp_dir("doctor");
    let lax = run(&["doctor", dir.to_str().unwrap()]);
    assert!(text(&lax).contains('✗'), "{}", text(&lax));
    assert_eq!(lax.status.code(), Some(0));
    let strict = run(&["doctor", dir.to_str().unwrap(), "--strict"]);
    assert_eq!(strict.status.code(), Some(1), "{}", text(&strict));
}
