//! dsl 0.47.0 §2.2 (#122): the runtime's `line` record carries `texts` and
//! `localeSegments` with each translation's `{{…}}` markers replaced by the
//! value the source rendered for the marker with the same referent (inner
//! text trimmed) — markers a translator reordered or spaced differently keep
//! their values, and a marker the source does not have stays as written.
//! `lute run --json` (the conformance record) leaves them out.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use lute_runtime::index::Bundle;
use lute_runtime::runtime::{Event, Input, Seed};
use lute_runtime::Runtime;
use serde_json::{json, Value as Json};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn write(dir: &Path, rel: &str, text: &str) -> PathBuf {
    let path = dir.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&path, text).unwrap();
    path
}

fn run(args: &[&str]) -> Output {
    Command::new(BIN).args(args).output().unwrap()
}

fn ok(out: &Output) {
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn read(path: &Path) -> Json {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

const SCENE: &str = "\
---
kind: scene
id: tally
on: tally
once: false
---

## Tally

::set{run.a = 3}
::set{run.b = 5}
@narrator{code=\"0010\"}: First {{run.a}}, then {{run.b}}.
@narrator{code=\"0020\"}: :speed[Count {{run.a}}]{rate=1.5} and {{run.b}}.
@narrator{code=\"0030\"}: Total {{ run.a }}.
";

/// The ko translations: line 0010 reorders its markers and adds one the
/// source does not have; line 0020 moves `{{run.b}}` into the modified run;
/// line 0030 spaces its markers unlike the source and writes an escaped
/// `\{{run.a}}`, which the compiler's plain text keeps as `{{run.a}}`.
const KO_0010: &str = "먼저 {{ run.b }}, 그다음 {{run.a}} {{run.c}}.";
const KO_0020: &str = ":speed[{{run.b}} 세기]{rate=1.5} 그리고 {{run.a}}.";
const KO_0030: &str = "합계 {{run.a}}, 글자 \\{{run.a}}.";

fn project(dir: &Path) {
    write(
        dir,
        "lute.project.yaml",
        "defaultProfile: g\nprofiles:\n  g:\n    plugins: { tally: true }\ndefaults:\n  uses: [world.schema.yaml]\n",
    );
    write(
        dir,
        "world.schema.yaml",
        "state:\n  run.a: { type: int, default: 0 }\n  run.b: { type: int, default: 0 }\n  run.c: { type: int, default: 0 }\n",
    );
    write(
        dir,
        "plugins/tally/plugin.yaml",
        "id: tally\nversion: 0.1.0\nkind: capability\ndepends: [{ id: lute.core, range: \"^0.0.1\" }]\nexports:\n  occasions: occasions/\n",
    );
    write(
        dir,
        "plugins/tally/occasions/o.yaml",
        "occasions:\n  tally: { select: first }\n",
    );
    write(dir, "scenes/tally.lute", SCENE);
}

/// Export the project's rows, translate both lines into ko, import and
/// compile the whole project with the bundle into `out`.
fn compile_localized(dir: &Path, out: &Path) {
    let exported = run(&["loc", "export", dir.to_str().unwrap()]);
    ok(&exported);
    let rows: Vec<Json> = serde_json::from_slice::<Vec<Json>>(&exported.stdout)
        .unwrap()
        .into_iter()
        .map(|mut row| {
            let line = row["lineId"].as_str().unwrap_or_default().to_string();
            if line.ends_with("narrator_0010") {
                row["text"] = json!(KO_0010);
            } else if line.ends_with("narrator_0020") {
                row["text"] = json!(KO_0020);
            } else if line.ends_with("narrator_0030") {
                row["text"] = json!(KO_0030);
            }
            row
        })
        .collect();
    let ko = write(dir, "work/ko.json", &serde_json::to_string(&rows).unwrap());
    let bundle = dir.join("work/bundle.json");
    ok(&run(&[
        "loc",
        "import",
        ko.to_str().unwrap(),
        "-o",
        bundle.to_str().unwrap(),
    ]));
    ok(&run(&[
        "compile",
        "--all",
        dir.to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
        "--locales",
        bundle.to_str().unwrap(),
    ]));
}

fn line_records(out: &Path) -> Vec<Json> {
    let index = read(&out.join("project.index.json"));
    let artifacts: BTreeMap<String, Json> = index["documents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|doc| {
            let artifact = doc["artifact"].as_str().unwrap().to_string();
            (artifact.clone(), read(&out.join(&artifact)))
        })
        .collect();
    let runtime = Runtime::load(Bundle::new(artifacts, index)).unwrap();
    let (state, _) = runtime.begin(Seed::default()).unwrap();
    let input: Input =
        serde_json::from_value(json!({"type": "raiseOccasion", "occasion": "tally"})).unwrap();
    let (_, output) = runtime
        .step(state, input)
        .unwrap_or_else(|(_, rejected)| panic!("{} {}", rejected.code, rejected.message));
    output
        .events
        .into_iter()
        .filter_map(|event| match event {
            Event::Record { record, .. } if record["kind"] == "line" => Some(record),
            _ => None,
        })
        .collect()
}

#[test]
fn a_line_record_carries_its_translations_rendered_by_marker_text() {
    let scratch = lute_scratch::ScratchDir::named("rendered-translations-047");
    project(&scratch);
    let out = scratch.join("bundle");
    compile_localized(&scratch, &out);
    let lines = line_records(&out);
    assert_eq!(lines.len(), 3, "{lines:#?}");

    assert_eq!(lines[0]["text"], "First 3, then 5.");
    assert_eq!(
        lines[0]["texts"],
        json!({"ko": "먼저 5, 그다음 3 {{run.c}}."}),
        "reordered and spaced markers keep their values; an unknown one stays as written"
    );
    assert!(lines[0].get("localeSegments").is_none(), "{:#?}", lines[0]);

    assert_eq!(lines[1]["text"], "Count 3 and 5.");
    assert_eq!(lines[1]["texts"], json!({"ko": "5 세기 그리고 3."}));
    assert_eq!(
        lines[1]["localeSegments"],
        json!({"ko": [
            {"text": "5 세기", "rate": 1.5},
            {"text": " 그리고 3."}
        ]})
    );
    assert_eq!(
        lines[1]["segments"],
        json!([{"text": "Count 3", "rate": 1.5}, {"text": " and 5."}])
    );

    // A spaced source marker matches an unspaced translation marker. The
    // plain text keeps no escape, so a literal `{{run.a}}` equal to a source
    // marker is replaced too (documented in the runtime contract).
    assert_eq!(lines[2]["text"], "Total 3.");
    assert_eq!(lines[2]["texts"], json!({"ko": "합계 3, 글자 3."}));
}

#[test]
fn lute_run_leaves_the_rendered_translations_out() {
    let scratch = lute_scratch::ScratchDir::named("rendered-translations-047-run");
    project(&scratch);
    let out = scratch.join("bundle");
    compile_localized(&scratch, &out);
    let index = read(&out.join("project.index.json"));
    let artifact = out.join(index["documents"][0]["artifact"].as_str().unwrap());
    let ran = run(&["run", artifact.to_str().unwrap(), "--json"]);
    ok(&ran);
    let transcript = String::from_utf8_lossy(&ran.stdout);
    assert!(transcript.contains("First 3, then 5."), "{transcript}");
    assert!(!transcript.contains("texts"), "{transcript}");
    assert!(!transcript.contains("localeSegments"), "{transcript}");
}
