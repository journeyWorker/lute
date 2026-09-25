//! End-to-end contract tests for `lute compile-stream`'s flushed NDJSON wire.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "lute-compile-stream-{tag}-{}-{n}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_at(dir: &Path, rel: &str, content: &str) -> PathBuf {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, content).unwrap();
    path
}

fn scene_prefix(extra_meta: &str) -> String {
    format!(
        "---\nkind: scene\ncharacter: hero\nseason: 1\nepisode: 1\n{extra_meta}---\n\n## Opening\n\n"
    )
}

fn stream_to_end(args: &[&str], body: &[u8]) -> std::process::Output {
    let mut child = Command::new(BIN)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(body).unwrap();
    child.wait_with_output().unwrap()
}

fn events(output: &[u8]) -> Vec<serde_json::Value> {
    std::str::from_utf8(output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn next_event(rx: &Receiver<String>, child: &mut Child) -> serde_json::Value {
    match rx.recv_timeout(Duration::from_secs(3)) {
        Ok(line) => serde_json::from_str(&line).unwrap(),
        Err(error) => {
            let _ = child.kill();
            panic!("timed out waiting for flushed NDJSON event: {error}");
        }
    }
}

#[test]
fn first_update_is_flushed_before_stdin_closes() {
    let dir = temp_dir("flush");
    let prefix = write_at(&dir, "prefix.lute", &scene_prefix(""));
    let mut child = Command::new(BIN)
        .args(["compile-stream", prefix.to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                break;
            }
        }
    });

    let start = next_event(&rx, &mut child);
    assert_eq!(start["kind"], "start");
    assert_eq!(start["sequence"], 0);
    assert_eq!(start["appendFrom"], 0);

    stdin
        .write_all(b"@narrator{code=\"0010\"}: before EOF\n")
        .unwrap();
    stdin.flush().unwrap();
    let update = next_event(&rx, &mut child);
    assert_eq!(update["kind"], "update", "{update}");
    assert_eq!(update["sequence"], 1);
    assert_eq!(update["appendFrom"], 0);
    assert_eq!(update["artifact"]["commands"][0]["text"], "before EOF");
    assert!(
        child.try_wait().unwrap().is_none(),
        "the update must not depend on transport EOF"
    );

    drop(stdin);
    let finish = next_event(&rx, &mut child);
    assert_eq!(finish["kind"], "finish", "{finish}");
    assert_eq!(finish["sequence"], 1);
    assert_eq!(child.wait().unwrap().code(), Some(0));
    reader.join().unwrap();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn final_branch_snapshot_runs_through_the_existing_runner() {
    let dir = temp_dir("runner");
    let prefix = write_at(
        &dir,
        "prefix.lute",
        &scene_prefix("state:\n  run.result: { type: string, default: pending }\n"),
    );
    let body = b"<branch id=\"pick\">\n\
<choice id=\"left\" label=\"Left\">\n\
@narrator: chose left\n\
::set{ run.result = \"left\" }\n\
</choice>\n\
<choice id=\"right\" label=\"Right\">\n\
@narrator: chose right\n\
::set{ run.result = \"right\" }\n\
</choice>\n\
</branch>\n";
    let output = stream_to_end(&["compile-stream", prefix.to_str().unwrap()], body);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let wire = events(&output.stdout);
    assert_eq!(wire.last().unwrap()["kind"], "finish", "{wire:#?}");
    let artifact = wire
        .iter()
        .rev()
        .find(|event| event["kind"] == "update")
        .unwrap()["artifact"]
        .clone();
    let artifact_path = dir.join("artifact.json");
    std::fs::write(&artifact_path, serde_json::to_vec(&artifact).unwrap()).unwrap();
    let mock = write_at(&dir, "mock.yaml", "choose:\n  pick: left\n");
    let run = Command::new(BIN)
        .args([
            "run",
            artifact_path.to_str().unwrap(),
            "--mock",
            mock.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    assert_eq!(
        run.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let transcript: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(transcript["exit"], "complete");
    assert_eq!(transcript["state"]["run.result"], "left");
    assert!(
        transcript["commands"]
            .as_array()
            .unwrap()
            .iter()
            .any(|command| command["text"] == "chose left"),
        "{transcript}"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn incomplete_block_at_eof_emits_error_without_finish() {
    let dir = temp_dir("incomplete");
    let prefix = write_at(&dir, "prefix.lute", &scene_prefix(""));
    let output = stream_to_end(
        &["compile-stream", prefix.to_str().unwrap()],
        b"<branch id=\"pick\">\n",
    );
    assert_eq!(output.status.code(), Some(1));
    let wire = events(&output.stdout);
    assert_eq!(wire.first().unwrap()["kind"], "start", "{wire:#?}");
    assert_eq!(wire.last().unwrap()["kind"], "error", "{wire:#?}");
    assert!(
        wire.last().unwrap()["diagnostics"]
            .as_array()
            .is_some_and(|diagnostics| !diagnostics.is_empty()),
        "{wire:#?}"
    );
    assert!(
        wire.iter().all(|event| event["kind"] != "finish"),
        "a malformed EOF must never claim success: {wire:#?}"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn project_defaults_components_and_identity_match_ordinary_compile() {
    let dir = temp_dir("project");
    write_at(
        &dir,
        "lute.project.yaml",
        "pluginsDir: plugins/\ndefaultProfile: core\nprofiles:\n  core:\n    plugins: { demo.plugin: true }\n\
identity:\n  lineId: \"{prefix}-{speaker}-{code}\"\n\
defaults:\n  kind: scene\n  character: hero\n  season: 1\n  episode: 7\n  uses: [world.schema.yaml]\n",
    );
    write_at(
        &dir,
        "world.schema.yaml",
        "state:\n  run.mood: { type: string, default: neutral }\n",
    );
    write_at(
        &dir,
        "plugins/demo.plugin/plugin.yaml",
        "id: demo.plugin\nversion: 0.1.0\nkind: capability\nexports:\n  directives: directives/\n",
    );
    write_at(
        &dir,
        "plugins/demo.plugin/directives/announce.yaml",
        "directives:\n  - name: announce\n    attrs:\n      - { name: id, required: true, type: { providerRef: speakerId } }\n",
    );
    write_at(
        &dir,
        "greet.component.lute",
        "---\ncomponent: greet\nparams:\n  who: string\n---\n\n## Body\n\n@narrator{code=\"0010\"}: hello from component\n",
    );
    let template = "---\ntitle: Stream project parity\ncomponents: [greet.component.lute]\n---\n\n## Opening\n\n";
    let prefix = write_at(&dir, "prefix.lute", template);
    let providers = dir.join("providers");
    write_at(
        &providers,
        "speakers.yaml",
        "manifestVersion: test\nproviderVersion: \"1\"\nstale: false\nentries:\n  speakerId: [hero]\n",
    );
    let body = "::announce{id=\"hero\"}\n::use{component=\"greet\" who=\"hero\"}\n::set{ run.mood = \"happy\" }\n";
    let output = stream_to_end(
        &[
            "compile-stream",
            prefix.to_str().unwrap(),
            "--project",
            dir.to_str().unwrap(),
            "--providers",
            providers.to_str().unwrap(),
        ],
        body.as_bytes(),
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let wire = events(&output.stdout);
    let streamed = wire
        .iter()
        .rev()
        .find(|event| event["kind"] == "update")
        .unwrap()["artifact"]
        .clone();

    let completed = write_at(&dir, "completed.lute", &format!("{template}{body}"));
    let ordinary = Command::new(BIN)
        .args([
            "compile",
            completed.to_str().unwrap(),
            "--project",
            dir.to_str().unwrap(),
            "--providers",
            providers.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(
        ordinary.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&ordinary.stderr)
    );
    let ordinary: serde_json::Value = serde_json::from_slice(&ordinary.stdout).unwrap();
    assert_eq!(streamed, ordinary, "streaming must use the ordinary setup");
    let mood = streamed["state"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["path"] == "run.mood")
        .expect("manifest defaults must resolve the imported schema");
    assert_eq!(mood["default"], "neutral");
    assert!(
        streamed["commands"]
            .as_array()
            .unwrap()
            .iter()
            .any(|command| {
                command["lineId"]
                    .as_str()
                    .is_some_and(|id| id.contains("-narrator-0010"))
            }),
        "project identity must govern expanded component lines: {streamed}"
    );
    assert_eq!(std::fs::read_to_string(&prefix).unwrap(), template);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn invalid_utf8_exits_two_without_finish() {
    let dir = temp_dir("utf8");
    let prefix = write_at(&dir, "prefix.lute", &scene_prefix(""));
    let output = stream_to_end(
        &["compile-stream", prefix.to_str().unwrap()],
        b"@narrator: valid\n\xff",
    );
    assert_eq!(output.status.code(), Some(2));
    let wire = events(&output.stdout);
    assert!(wire.iter().any(|event| event["kind"] == "update"));
    assert!(wire.iter().all(|event| event["kind"] != "finish"));
    assert!(String::from_utf8_lossy(&output.stderr).contains("UTF-8"));
    let _ = std::fs::remove_dir_all(dir);
}
