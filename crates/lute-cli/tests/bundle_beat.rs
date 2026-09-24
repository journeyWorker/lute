//! dsl 0.23.0 §4: `lute trace <lore.lute> --beat <id>` and `lute run <lore
//! artifact> --beat <id>` present ONE bundle beat outside any selection — its
//! body runs like a scene's (a `choose:` mock picks), every effect applies,
//! and its `when` is shown on the head, not enforced. A lore artifact
//! interleaves `entry` and `beat` records, so an entry's body segment stops
//! at the next record of either kind.

use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");
const SCENE_ARTIFACT: &str = "../../conformance/end-reason/artifact.json";

const SOURCE: &str = r#"---
kind: lore
id: interviews
state:
  run.porterTrust: { type: number, default: 0 }
---

<entry id="porterNote" target="item.porter_note">
  @narrator: A note about the porter.
</entry>

<beat id="porter" on="talk" target="npc.porter" when="run.porterTrust >= 0">
  @porter: You again.
  <branch id="porterTalk">
    <choice id="ask" label="Ask about the night">
      @porter: I saw nothing.
      ::set{run.porterTrust += 1}
    </choice>
    <choice id="leave" label="Leave">
      @porter: Good.
    </choice>
  </branch>
</beat>
"#;

/// The lore artifact shape the contract fixes for `SOURCE` plus a trailing
/// entry (dsl 0.23.0 §4): one addressing unit per `<entry>`/`<beat>` in
/// source order, each head record followed by its body segment.
const ARTIFACT: &str = r#"{
  "kind": "lore",
  "lute": "0.22.0",
  "irVersion": "0.22.0",
  "meta": { "id": "interviews" },
  "state": [
    { "path": "entry.porterNote.read", "type": "bool", "default": false, "provenance": "entry:porterNote" },
    { "path": "entry.lastNote.read", "type": "bool", "default": false, "provenance": "entry:lastNote" },
    { "path": "run.porterTrust", "type": "number", "default": 0 }
  ],
  "commands": [
    { "kind": "entry", "addr": "001-0100", "id": "porterNote", "target": "item.porter_note", "body": "001-0200" },
    { "kind": "line", "addr": "001-0200", "role": "narration", "speaker": "narrator", "text": "A note about the porter." },
    { "kind": "beat", "addr": "002-0100", "id": "interviews.porter", "on": "talk", "target": "npc.porter",
      "when": { "raw": "run.porterTrust >= 0" }, "priority": 0, "once": "run", "body": "002-0200" },
    { "kind": "line", "addr": "002-0200", "role": "dialogue", "speaker": "porter", "text": "You again." },
    { "kind": "choice", "addr": "002-0300", "branchId": "porterTalk", "recordKey": "scene.choices.porterTalk",
      "options": [
        { "id": "ask", "label": "Ask about the night", "target": "002-0400" },
        { "id": "leave", "label": "Leave", "target": "002-0700" }
      ],
      "converge": "002-0900" },
    { "kind": "line", "addr": "002-0400", "role": "dialogue", "speaker": "porter", "text": "I saw nothing." },
    { "kind": "set", "addr": "002-0500", "path": "run.porterTrust", "op": "+=", "value": "1", "expr": { "lit": 1.0 } },
    { "kind": "jump", "addr": "002-0600", "target": "002-0900" },
    { "kind": "line", "addr": "002-0700", "role": "dialogue", "speaker": "porter", "text": "Good." },
    { "kind": "jump", "addr": "002-0800", "target": "002-0900" },
    { "kind": "entry", "addr": "003-0100", "id": "lastNote", "body": "003-0200" },
    { "kind": "line", "addr": "003-0200", "role": "narration", "speaker": "narrator", "text": "The last note." }
  ]
}"#;

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "lute-bundle-beat-{tag}-{}-{n}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &Path, rel: &str, text: &str) -> String {
    let p = dir.join(rel);
    std::fs::write(&p, text).unwrap();
    p.to_str().unwrap().to_string()
}

fn lute(args: &[&str]) -> (Option<i32>, String, String) {
    let out = Command::new(BIN).args(args).output().unwrap();
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

fn kinds(v: &serde_json::Value) -> Vec<String> {
    v["commands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["kind"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn trace_bundle_beat_walks_the_chosen_arm() {
    let dir = temp_dir("trace");
    let file = write(&dir, "interviews.lute", SOURCE);
    let (code, stdout, stderr) =
        lute(&["trace", &file, "--beat", "porter", "--choose", "porterTalk=ask"]);
    assert_eq!(code, Some(0), "stderr: {stderr}\nstdout: {stdout}");
    assert!(stdout.contains("<beat interviews.porter>\n"), "{stdout}");
    assert!(stdout.contains("You again."), "{stdout}");
    assert!(stdout.contains("I saw nothing."), "{stdout}");
    assert!(stdout.contains("::set  run.porterTrust = 1"), "{stdout}");
    assert!(!stdout.contains("A note about the porter."), "{stdout}");

    // The canonical id works too, and `--json` carries the beat head.
    let (code, stdout, stderr) = lute(&[
        "trace",
        &file,
        "--beat",
        "interviews.porter",
        "--choose",
        "porterTalk=leave",
        "--json",
    ]);
    assert_eq!(code, Some(0), "stderr: {stderr}\nstdout: {stdout}");
    let v: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(
        v["steps"][0],
        serde_json::json!({ "kind": "beat", "id": "interviews.porter", "eligible": true })
    );
    assert!(stdout.contains("Good."), "{stdout}");
}

#[test]
fn trace_bundle_beat_usage_and_refusals() {
    let dir = temp_dir("trace-usage");
    let file = write(&dir, "interviews.lute", SOURCE);

    // A lore document without `--entry`/`--beat`: usage error naming both.
    let (code, stdout, stderr) = lute(&["trace", &file]);
    assert_eq!(code, Some(2), "stdout: {stdout}");
    assert!(stderr.contains("pass `--entry <id>`"), "{stderr}");
    assert!(stderr.contains("porterNote"), "{stderr}");
    assert!(stderr.contains("`--beat <id>`"), "{stderr}");
    assert!(stderr.contains("interviews.porter"), "{stderr}");

    // An unknown beat id is refused with E-TRACE-BEAT.
    let (code, stdout, _) = lute(&["trace", &file, "--beat", "nope"]);
    assert_eq!(code, Some(1), "{stdout}");
    assert!(stdout.contains("E-TRACE-BEAT"), "{stdout}");
    assert!(stdout.contains("invalid `--beat`"), "{stdout}");

    // `--entry` and `--beat` are one presentation or the other.
    let (code, _, stderr) =
        lute(&["trace", &file, "--entry", "porterNote", "--beat", "porter"]);
    assert_eq!(code, Some(2), "{stderr}");
}

#[test]
fn run_bundle_beat_runs_its_segment_like_a_scene() {
    let dir = temp_dir("run");
    let art = write(&dir, "interviews.json", ARTIFACT);
    let mock = write(&dir, "ask.yaml", "choose:\n  porterTalk: [ask]\n");
    let (code, stdout, stderr) = lute(&[
        "run",
        &art,
        "--beat",
        "interviews.porter",
        "--mock",
        &mock,
        "--json",
    ]);
    assert_eq!(code, Some(0), "stderr: {stderr}\nstdout: {stdout}");
    let v: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(
        v["commands"][0],
        serde_json::json!({
            "addr": "002-0100",
            "kind": "beat",
            "id": "interviews.porter",
            "eligible": true
        })
    );
    // The body segment only — never into the next `entry` record.
    assert_eq!(kinds(&v), ["beat", "line", "choice", "line", "set"]);
    assert_eq!(v["state"]["run.porterTrust"], 1);

    // The bare beat id resolves; the human line mirrors an entry's.
    let (code, stdout, stderr) = lute(&["run", &art, "--beat", "porter", "--mock", &mock]);
    assert_eq!(code, Some(0), "stderr: {stderr}");
    assert!(stdout.contains("  002-0100  beat   interviews.porter\n"), "{stdout}");

    // `when` false is recorded, not enforced.
    let low = write(
        &dir,
        "low.yaml",
        "state:\n  run.porterTrust: -1\nchoose:\n  porterTalk: [leave]\n",
    );
    let (code, stdout, stderr) = lute(&["run", &art, "--beat", "porter", "--mock", &low, "--json"]);
    assert_eq!(code, Some(0), "stderr: {stderr}");
    let v: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(v["commands"][0]["eligible"], false);
    assert_eq!(kinds(&v), ["beat", "line", "choice", "line"]);
}

#[test]
fn run_entry_segment_stops_at_the_next_bundle_beat() {
    let dir = temp_dir("run-entry");
    let art = write(&dir, "interviews.json", ARTIFACT);
    let (code, stdout, stderr) = lute(&["run", &art, "--entry", "porterNote", "--json"]);
    assert_eq!(code, Some(0), "stderr: {stderr}");
    let v: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(kinds(&v), ["entry", "line"]);
    assert_eq!(v["state"]["entry.porterNote.read"], true);
}

#[test]
fn compiled_bundle_beat_runs_by_canonical_id() {
    let dir = temp_dir("compiled");
    let file = write(&dir, "interviews.lute", SOURCE);
    let art = dir.join("interviews.json");
    let art = art.to_str().unwrap();
    let (code, stdout, stderr) = lute(&["compile", &file, "-o", art]);
    assert_eq!(code, Some(0), "stderr: {stderr}\nstdout: {stdout}");
    let mock = write(&dir, "ask.yaml", "choose:\n  porterTalk: [ask]\n");

    let (code, stdout, stderr) = lute(&[
        "run",
        art,
        "--beat",
        "interviews.porter",
        "--mock",
        &mock,
        "--json",
    ]);
    assert_eq!(code, Some(0), "stderr: {stderr}\nstdout: {stdout}");
    let v: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(v["commands"][0]["kind"], "beat");
    assert_eq!(v["commands"][0]["id"], "interviews.porter");
    assert_eq!(v["commands"][0]["eligible"], true);
    assert_eq!(kinds(&v), ["beat", "line", "choice", "line", "set"]);
    assert_eq!(v["state"]["run.porterTrust"], 1);

    // The entry before the beat stops at the beat's head record.
    let (code, stdout, stderr) = lute(&["run", art, "--entry", "porterNote", "--json"]);
    assert_eq!(code, Some(0), "stderr: {stderr}");
    let v: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(kinds(&v), ["entry", "line"]);
}

#[test]
fn run_bundle_beat_usage_errors() {
    let dir = temp_dir("run-usage");
    let art = write(&dir, "interviews.json", ARTIFACT);

    let (code, stdout, stderr) = lute(&["run", &art]);
    assert_eq!(code, Some(2), "stdout: {stdout}");
    assert!(stderr.contains("is a lore artifact"), "{stderr}");
    assert!(stderr.contains("`--beat <id>`"), "{stderr}");

    let (code, _, stderr) = lute(&["run", &art, "--beat", "nope"]);
    assert_eq!(code, Some(2));
    assert!(stderr.contains("names no beat"), "{stderr}");
    assert!(stderr.contains("interviews.porter"), "{stderr}");

    let (code, _, stderr) = lute(&["run", SCENE_ARTIFACT, "--beat", "porter"]);
    assert_eq!(code, Some(2));
    assert!(stderr.contains("`--beat porter` needs a lore artifact"), "{stderr}");

    let (code, _, stderr) = lute(&["run", &art, "--entry", "porterNote", "--beat", "porter"]);
    assert_eq!(code, Some(2), "{stderr}");
}

/// dsl 0.23.0 §4 through `lute play`: a bundle beat is a candidate of its
/// occasion under its canonical id, is presented by running its body, is
/// spent by that presentation (`once: run` default), and `visited()` reads
/// its canonical id afterwards.
#[test]
fn play_presents_a_bundle_beat_and_spends_it() {
    let dir = temp_dir("play");
    write(
        &dir,
        "lute.project.yaml",
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n",
    );
    write(&dir, "interviews.lute", SOURCE);
    write(
        &dir,
        "after.lute",
        "---\nkind: lore\nid: after\n---\n\n\
         <beat id=\"recall\" on=\"talk\" target=\"npc.porter\" priority=\"-1\" \
         when=\"visited('interviews.porter')\">\n  @porter: As I said.\n</beat>\n",
    );
    let script = write(
        &dir,
        "p.play.yaml",
        "steps:\n  - occasion: talk\n    target: npc.porter\n  - occasion: talk\n    target: npc.porter\n\
         choose:\n  porterTalk: [ask]\n",
    );
    let root = dir.to_str().unwrap().to_string();
    let (code, stdout, stderr) = lute(&["check-project", &root]);
    assert_eq!(code, Some(0), "check-project clean:\n{stdout}\n{stderr}");
    let (code, stdout, stderr) = lute(&["play", &root, "--script", &script, "--json"]);
    assert_eq!(code, Some(0), "play:\n{stdout}\n{stderr}");
    let v: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let steps = v["steps"].as_array().expect("steps");
    let text = stdout.as_str();
    // Step 1: the bundle beat wins and its chosen arm runs.
    assert_eq!(steps[0]["winner"], "interviews.porter", "{text}");
    assert!(text.contains("I saw nothing."), "{text}");
    // Step 2: it is spent for the run; the beat gated on its visit wins.
    assert_eq!(steps[1]["winner"], "after.recall", "{text}");
    assert!(text.contains("As I said."), "{text}");
}
