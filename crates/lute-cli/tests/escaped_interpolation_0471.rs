//! lute 0.47.1 §3 (#134): `\{{` in a line is literal text (dsl 0.1.0 §7.6).
//! The compiler writes it `\{{` in the IR `text`, segment runs and merged
//! translations, doubles a literal backslash directly before a `{{`, and
//! emits one placeholder per marker; the runtime reads that encoding, so its
//! rendered `line` record shows the literal `{{` and substitutes the real
//! markers. Every scanner reads an odd backslash run before `{{` as the
//! escape, so `\\{{run.b}}` is a backslash and a marker everywhere.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use lute_runtime::index::Bundle;
use lute_runtime::runtime::{Event, Input, Seed};
use lute_runtime::Runtime;
use serde_json::{json, Value as Json};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

const SCENE: &str = r#"---
kind: scene
id: tally
on: tally
once: false
---

## Tally

::set{run.a = 3}
::set{run.b = 5}
@narrator{code="0010"}: Lit \{{run.a}} and {{ run.a }}.
@narrator{code="0020"}: :speed[x\\]{rate=2}{{run.a}} end.
@narrator{code="0030"}: :speed[Lit \{{run.a}}]{rate=1.5} and {{run.b}}.
@narrator{code="0040"}: Pair \\{{run.b}} and \\\{{run.b}}.
"#;

/// The ko translations: each keeps a literal `\{{…}}` beside its markers.
const KO: [(&str, &str); 3] = [
    ("narrator_0010", r"{{run.a}} 그리고 리터럴 \{{run.a}}."),
    ("narrator_0020", r":speed[x\\]{rate=2}{{run.a}} 끝."),
    (
        "narrator_0030",
        r":speed[리터럴 \{{run.b}}]{rate=1.5} 그리고 {{run.b}}.",
    ),
];

fn write(dir: &Path, rel: &str, text: &str) -> PathBuf {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
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
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

fn read(path: &Path) -> Json {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn project(dir: &Path) {
    write(
        dir,
        "lute.project.yaml",
        "defaultProfile: g\nprofiles:\n  g:\n    plugins: { tally: true }\ndefaults:\n  uses: [world.schema.yaml]\n",
    );
    write(
        dir,
        "world.schema.yaml",
        "state:\n  run.a: { type: int, default: 0 }\n  run.b: { type: int, default: 0 }\n",
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

/// `lute loc export` rows, `lute loc import` of them translated into ko,
/// then `compile --all --locales` into `out`. Returns the exported rows and
/// the compile's output.
fn compile_localized(dir: &Path, out: &Path) -> (Vec<Json>, Output) {
    let exported = run(&["loc", "export", dir.to_str().unwrap()]);
    ok(&exported);
    let rows: Vec<Json> = serde_json::from_slice(&exported.stdout).unwrap();
    let translated: Vec<Json> = rows
        .iter()
        .cloned()
        .map(|mut row| {
            let line = row["lineId"].as_str().unwrap_or_default().to_string();
            if let Some((_, ko)) = KO.iter().find(|(id, _)| line.ends_with(id)) {
                row["text"] = json!(ko);
            }
            row
        })
        .collect();
    let ko = write(
        dir,
        "work/ko.json",
        &serde_json::to_string(&translated).unwrap(),
    );
    let bundle = dir.join("work/bundle.json");
    ok(&run(&[
        "loc",
        "import",
        ko.to_str().unwrap(),
        "-o",
        bundle.to_str().unwrap(),
    ]));
    let compiled = run(&[
        "compile",
        "--all",
        dir.to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
        "--locales",
        bundle.to_str().unwrap(),
    ]);
    ok(&compiled);
    (rows, compiled)
}

fn artifacts(out: &Path) -> (Json, BTreeMap<String, Json>) {
    let index = read(&out.join("project.index.json"));
    let artifacts = index["documents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|doc| {
            let artifact = doc["artifact"].as_str().unwrap().to_string();
            (artifact.clone(), read(&out.join(&artifact)))
        })
        .collect();
    (index, artifacts)
}

fn ir_lines(out: &Path) -> Vec<Json> {
    let (_, artifacts) = artifacts(out);
    artifacts
        .into_values()
        .flat_map(|a| a["commands"].as_array().unwrap().clone())
        .filter(|c| c["kind"] == "line")
        .collect()
}

fn line_records(out: &Path) -> Vec<Json> {
    let (index, artifacts) = artifacts(out);
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

fn localized(tag: &str) -> (lute_scratch::ScratchDir, PathBuf, Vec<Json>, Output) {
    let scratch = lute_scratch::ScratchDir::named(tag);
    project(&scratch);
    let out = scratch.join("bundle");
    let (rows, compiled) = compile_localized(&scratch, &out);
    (scratch, out, rows, compiled)
}

#[test]
fn the_ir_writes_a_literal_opener_escaped_and_one_placeholder_per_marker() {
    let (_scratch, out, _, _) = localized("escaped-interp-0471-ir");
    let lines = ir_lines(&out);
    assert_eq!(lines.len(), 4, "{lines:#?}");
    let path = |p: &str| json!({"kind": "path", "path": p});

    assert_eq!(lines[0]["text"], r"Lit \{{run.a}} and {{ run.a }}.");
    assert_eq!(lines[0]["placeholders"], json!([path("run.a")]));
    assert_eq!(
        lines[0]["texts"],
        json!({"ko": r"{{run.a}} 그리고 리터럴 \{{run.a}}."})
    );

    // A literal backslash right before a marker is doubled in `text`; the
    // run that ends with it holds it alone, undoubled.
    assert_eq!(lines[1]["text"], r"x\\{{run.a}} end.");
    assert_eq!(lines[1]["placeholders"], json!([path("run.a")]));
    assert_eq!(
        lines[1]["segments"],
        json!([{"text": r"x\", "rate": 2.0}, {"text": "{{run.a}} end."}])
    );
    assert_eq!(lines[1]["texts"], json!({"ko": r"x\\{{run.a}} 끝."}));

    assert_eq!(lines[2]["text"], r"Lit \{{run.a}} and {{run.b}}.");
    assert_eq!(lines[2]["placeholders"], json!([path("run.b")]));
    assert_eq!(
        lines[2]["segments"],
        json!([{"text": r"Lit \{{run.a}}", "rate": 1.5}, {"text": " and {{run.b}}."}])
    );
    assert_eq!(
        lines[2]["localeSegments"],
        json!({"ko": [{"text": r"리터럴 \{{run.b}}", "rate": 1.5}, {"text": " 그리고 {{run.b}}."}]})
    );

    // `\\{{run.b}}`: an even run, a backslash and a marker; `\\\{{run.b}}`:
    // an odd run, a backslash and a literal opener.
    assert_eq!(lines[3]["text"], r"Pair \\{{run.b}} and \\\{{run.b}}.");
    assert_eq!(lines[3]["placeholders"], json!([path("run.b")]));
}

#[test]
fn the_runtime_record_shows_the_literal_and_substitutes_the_markers() {
    let (_scratch, out, _, _) = localized("escaped-interp-0471-runtime");
    let lines = line_records(&out);
    assert_eq!(lines.len(), 4, "{lines:#?}");

    assert_eq!(lines[0]["text"], "Lit {{run.a}} and 3.");
    assert_eq!(
        lines[0]["texts"],
        json!({"ko": "3 그리고 리터럴 {{run.a}}."})
    );

    assert_eq!(lines[1]["text"], r"x\3 end.");
    assert_eq!(
        lines[1]["segments"],
        json!([{"text": r"x\", "rate": 2.0}, {"text": "3 end."}])
    );
    assert_eq!(lines[1]["texts"], json!({"ko": r"x\3 끝."}));
    assert_eq!(
        lines[1]["localeSegments"],
        json!({"ko": [{"text": r"x\", "rate": 2.0}, {"text": "3 끝."}]})
    );

    assert_eq!(lines[2]["text"], "Lit {{run.a}} and 5.");
    assert_eq!(
        lines[2]["segments"],
        json!([{"text": "Lit {{run.a}}", "rate": 1.5}, {"text": " and 5."}])
    );
    assert_eq!(
        lines[2]["texts"],
        json!({"ko": "리터럴 {{run.b}} 그리고 5."})
    );
    assert_eq!(
        lines[2]["localeSegments"],
        json!({"ko": [{"text": "리터럴 {{run.b}}", "rate": 1.5}, {"text": " 그리고 5."}]})
    );

    assert_eq!(lines[3]["text"], r"Pair \5 and \{{run.b}}.");
}

#[test]
fn lute_run_renders_the_same_text() {
    let (_scratch, out, _, _) = localized("escaped-interp-0471-run");
    let index = read(&out.join("project.index.json"));
    let artifact = out.join(index["documents"][0]["artifact"].as_str().unwrap());
    let ran = run(&["run", artifact.to_str().unwrap(), "--json"]);
    ok(&ran);
    let transcript: Json = serde_json::from_slice(&ran.stdout).unwrap();
    let texts: Vec<&str> = transcript["commands"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["kind"] == "line")
        .map(|c| c["text"].as_str().unwrap())
        .collect();
    assert_eq!(
        texts,
        [
            "Lit {{run.a}} and 3.",
            r"x\3 end.",
            "Lit {{run.a}} and 5.",
            r"Pair \5 and \{{run.b}}."
        ]
    );
}

#[test]
fn a_fresh_export_hashes_the_compiled_text_so_import_is_not_stale() {
    let (_scratch, out, rows, compiled) = localized("escaped-interp-0471-hash");
    let stderr = String::from_utf8_lossy(&compiled.stderr);
    assert!(!stderr.contains("W-L10N-STALE"), "{stderr}");
    let lines = ir_lines(&out);
    for line in &lines {
        let id = line["lineId"].as_str().unwrap();
        let row = rows
            .iter()
            .find(|r| r["lineId"] == id)
            .unwrap_or_else(|| panic!("no exported row for {id}"));
        assert_eq!(
            row["sourceHash"],
            json!(lute_compile::locale::source_hash(
                line["text"].as_str().unwrap()
            )),
            "{id}"
        );
    }
}

#[test]
fn the_checker_reads_an_even_run_before_a_marker_as_a_marker() {
    let scratch = lute_scratch::ScratchDir::named("escaped-interp-0471-check");
    project(&scratch);
    write(
        &scratch,
        "scenes/tally.lute",
        &SCENE.replace(r"Pair \\{{run.b}}", r"Pair \\{{run.nope}}"),
    );
    let checked = run(&["check-project", scratch.to_str().unwrap()]);
    let text = String::from_utf8_lossy(&checked.stdout).to_string()
        + &String::from_utf8_lossy(&checked.stderr);
    assert_eq!(checked.status.code(), Some(1), "{text}");
    assert!(text.contains("run.nope"), "{text}");
}

/// A `::use` string argument is spliced as its decoded value (`\\` is one
/// backslash), and the line-text rule judges its `{{` there: `\\{{` passes a
/// literal `{{`, `\\\\{{` a backslash and the marker, a bare `{{` the marker.
#[test]
fn a_use_string_argument_is_judged_on_its_decoded_value() {
    let scratch = lute_scratch::ScratchDir::named("escaped-interp-0471-use");
    project(&scratch);
    write(
        &scratch,
        "scenes/note.component.lute",
        "---\ncomponent: note\nparams:\n  p: string\n---\n\n## Note\n@narrator: A {{@p}} B.\n",
    );
    write(
        &scratch,
        "scenes/tally.lute",
        r#"---
kind: scene
id: tally
on: tally
once: false
components: [note.component.lute]
---

## Tally

::set{run.a = 3}
::use{component="note" p="lit \\{{run.a}}"}
::use{component="note" p="lit {{run.a}}"}
::use{component="note" p="lit \\\\{{run.a}}"}
"#,
    );
    let out = scratch.join("bundle");
    ok(&run(&[
        "compile",
        "--all",
        scratch.to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
    ]));
    let path = json!([{"kind": "path", "path": "run.a"}]);
    let ir: Vec<(Json, Json)> = ir_lines(&out)
        .into_iter()
        .map(|l| {
            (
                l["text"].clone(),
                l.get("placeholders").cloned().unwrap_or(Json::Null),
            )
        })
        .collect();
    assert_eq!(
        ir,
        [
            (json!(r"A lit \{{run.a}} B."), Json::Null),
            (json!("A lit {{run.a}} B."), path.clone()),
            (json!(r"A lit \\{{run.a}} B."), path),
        ]
    );
    let texts: Vec<Json> = line_records(&out)
        .into_iter()
        .map(|r| r["text"].clone())
        .collect();
    assert_eq!(
        texts,
        [
            json!("A lit {{run.a}} B."),
            json!("A lit 3 B."),
            json!(r"A lit \3 B."),
        ]
    );
}
