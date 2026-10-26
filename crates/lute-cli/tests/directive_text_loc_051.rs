//! 0.51.0 §5.5–§5.7 (#165): localizable directive text through the
//! localization tooling and the runtime — `lute loc export` rows of
//! `kind: "directive"`, `lute tag`, `lute loc import`, the
//! `--locale-bundle` merge (`W-L10N-MISSING`, `W-L10N-STALE`), JSON and CSV
//! round trips, `lute loc report` counts, and the `plugin` record's rendered
//! `attrs` and `localized`.
//!
//! On 0.50.0 `type: text` is an unknown type: the plugin fails to load, so
//! no export row, bundle entry or record here exists.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::{Command, Output};

use lute_runtime::index::Bundle;
use lute_runtime::runtime::{Event, Input, Seed};
use lute_runtime::Runtime;
use serde_json::{json, Value};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

const PROJECT: &str = "pluginsDir: plugins/\ndefaultProfile: game\nprofiles:\n  game:\n    \
                       plugins:\n      acme.ui: true\ndefaults:\n  contentLang: en\n  uses: \
                       [schema/world.schema.yaml]\n";

const PLUGIN: &str = "id: acme.ui\nversion: 0.1.0\nkind: capability\ndepends:\n  - { id: \
                      lute.core, range: \"^0.0.1\" }\nexports:\n  directives: directives/\n  \
                      occasions: occasions/\n";

const DIRECTIVES: &str = concat!(
    "directives:\n",
    "  - name: banner\n",
    "    attrs:\n",
    "      - { name: title, required: true, type: text }\n",
    "      - { name: subtitle, type: text }\n",
    "      - { name: style, type: { enum: [toast, card] } }\n",
);

const OCCASIONS: &str = "occasions:\n  hall: { select: first }\n";

const SCHEMA: &str = "state:\n  run.gold: { type: int, default: 0 }\n";

/// The source scene: one untagged `::banner` with two `text` attributes,
/// a host token and a state path in `title`.
const INTRO: &str = "---\nkind: scene\nid: intro\non: hall\nonce: false\n---\n\n## Hall\n\n\
                     ::set{run.gold = 3}\n\
                     ::banner{title=\"Welcome, {{userName}}: {{run.gold}} gold\" subtitle=\"Hi there\" style=\"card\"}\n\
                     @narrator{code=\"0010\"}: Hello.\n";

const TITLE_ID: &str = "intro~banner~0010~title";
const SUBTITLE_ID: &str = "intro~banner~0010~subtitle";

/// The ko title: its markers reordered and spaced unlike the source's.
const KO_TITLE: &str = "{{ run.gold }} 골드, {{userName}} 님 환영";

fn write(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, text).unwrap();
}

fn project(tag: &str) -> lute_scratch::ScratchDir {
    let dir = lute_scratch::ScratchDir::named(tag);
    write(&dir, "lute.project.yaml", PROJECT);
    write(&dir, "plugins/acme.ui/plugin.yaml", PLUGIN);
    write(&dir, "plugins/acme.ui/directives/d.yaml", DIRECTIVES);
    write(&dir, "plugins/acme.ui/occasions/o.yaml", OCCASIONS);
    write(&dir, "schema/world.schema.yaml", SCHEMA);
    write(&dir, "scenes/intro.lute", INTRO);
    dir
}

fn lute(dir: &Path, args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .current_dir(dir)
        .env("LUTE_TELEMETRY", "0")
        .output()
        .unwrap()
}

fn ok(out: &Output) -> String {
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn read(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// The export's `kind: "directive"` rows.
fn directive_rows(dir: &Path) -> Vec<Value> {
    let rows: Vec<Value> =
        serde_json::from_str(&ok(&lute(dir, &["loc", "export", "--project", "."]))).unwrap();
    rows.into_iter()
        .filter(|r| r["kind"] == "directive")
        .collect()
}

/// `rows` with each directive row's `text` replaced from `ko` (by
/// `lineId`), written as `work/ko.json` and imported into
/// `work/bundle.json`.
fn import_ko(dir: &Path, ko: &[(&str, &str)]) {
    let rows: Vec<Value> =
        serde_json::from_str(&ok(&lute(dir, &["loc", "export", "--project", "."]))).unwrap();
    let rows: Vec<Value> = rows
        .into_iter()
        .filter_map(|mut row| {
            let id = row["lineId"].as_str().unwrap_or_default().to_string();
            let (_, text) = ko.iter().find(|(lid, _)| *lid == id)?;
            row["text"] = json!(text);
            Some(row)
        })
        .collect();
    write(dir, "work/ko.json", &serde_json::to_string(&rows).unwrap());
    ok(&lute(
        dir,
        &["loc", "import", "work/ko.json", "-o", "work/bundle.json"],
    ));
}

/// The compiled `plugin` command of `scenes/intro.lute` under the bundle,
/// and the compile's stderr.
fn compile_with_bundle(dir: &Path) -> (Value, String) {
    let out = lute(
        dir,
        &[
            "compile",
            "scenes/intro.lute",
            "--project",
            ".",
            "-o",
            "work/intro.json",
            "--locale-bundle",
            "work/bundle.json",
        ],
    );
    ok(&out);
    let artifact = read(&dir.join("work/intro.json"));
    let plugin = artifact["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["kind"] == "plugin")
        .unwrap()
        .clone();
    (plugin, stderr(&out))
}

/// The #165 repro end to end: export row, `lute tag`, import, the
/// `--locale-bundle` merge, `W-L10N-MISSING` and `W-L10N-STALE`.
#[test]
fn a_directive_text_round_trips_through_export_tag_import_and_merge() {
    let dir = project("dt-loc-e2e");

    // Untagged: the row exists, but its `lineId` cannot be reproduced.
    let out = lute(&dir, &["loc", "export", "--project", "."]);
    ok(&out);
    assert!(
        stderr(&out).contains("2 directive texts untagged — run lute tag"),
        "{}",
        stderr(&out)
    );
    let rows = directive_rows(&dir);
    assert_eq!(rows.len(), 2, "{rows:#?}");
    assert!(rows
        .iter()
        .all(|r| r["lineId"].is_null() && r["code"].is_null()));

    ok(&lute(&dir, &["tag", "scenes/intro.lute"]));
    let rows = directive_rows(&dir);
    // Declaration order: `title`, then `subtitle`.
    assert_eq!(
        rows[0],
        json!({
            "kind": "directive",
            "file": "./scenes/intro.lute",
            "line": 11,
            "lineId": TITLE_ID,
            "code": "0010",
            "key": "banner.title",
            "text": "Welcome, {{userName}}: {{run.gold}} gold",
            "source": null,
            "sourceHash": lute_compile::locale::source_hash("Welcome, {{userName}}: {{run.gold}} gold"),
        })
    );
    assert_eq!(rows[1]["lineId"], SUBTITLE_ID);
    assert_eq!(rows[1]["key"], "banner.subtitle");

    // Only `title` is translated: `subtitle` lacks the declared `ko`.
    import_ko(&dir, &[(TITLE_ID, KO_TITLE)]);
    let (plugin, warnings) = compile_with_bundle(&dir);
    assert_eq!(
        plugin["localized"]["title"]["texts"],
        json!({ "ko": KO_TITLE })
    );
    assert!(plugin["localized"]["subtitle"].get("texts").is_none());
    // The source stays in `fields`.
    assert_eq!(
        plugin["fields"]["title"],
        "Welcome, {{userName}}: {{run.gold}} gold"
    );
    assert!(
        warnings.contains(&format!("W-L10N-MISSING] no `ko` text for `{SUBTITLE_ID}`")),
        "{warnings}"
    );
    assert!(!warnings.contains("W-L10N-STALE"), "{warnings}");

    // The source changes after the export the translator worked from.
    let src = std::fs::read_to_string(dir.join("scenes/intro.lute")).unwrap();
    write(
        &dir,
        "scenes/intro.lute",
        &src.replace("{{run.gold}} gold", "{{run.gold}} coins"),
    );
    let (plugin, warnings) = compile_with_bundle(&dir);
    assert_eq!(plugin["localized"]["title"]["texts"]["ko"], KO_TITLE);
    assert!(
        warnings.contains(&format!("W-L10N-STALE] `ko` text for `{TITLE_ID}`")),
        "{warnings}"
    );
}

/// The bundle's `entries` and `sourceHashes` for `ids`.
fn bundle_rows(bundle: &Value, ids: &[&str]) -> Vec<(Value, Value)> {
    ids.iter()
        .map(|id| {
            (
                bundle["entries"][id].clone(),
                bundle["sourceHashes"][id].clone(),
            )
        })
        .collect()
}

/// JSON and CSV exports both import to the same bundle, `~` ids intact.
#[test]
fn json_and_csv_exports_round_trip_directive_texts() {
    let dir = project("dt-loc-formats");
    ok(&lute(&dir, &["tag", "scenes/intro.lute"]));
    std::fs::create_dir_all(dir.join("work")).unwrap();
    ok(&lute(
        &dir,
        &["loc", "export", "--project", ".", "-o", "work/ko.json"],
    ));
    ok(&lute(
        &dir,
        &[
            "loc",
            "export",
            "--project",
            ".",
            "--format",
            "csv",
            "-o",
            "work/ko.csv",
        ],
    ));
    let csv = std::fs::read_to_string(dir.join("work/ko.csv")).unwrap();
    assert!(
        csv.contains(&format!(
            "directive,./scenes/intro.lute,11,{TITLE_ID},0010,,banner.title,\
             \"Welcome, {{{{userName}}}}: {{{{run.gold}}}} gold\",,,sha256:"
        )),
        "{csv}"
    );
    ok(&lute(
        &dir,
        &["loc", "import", "work/ko.json", "-o", "work/from-json.json"],
    ));
    ok(&lute(
        &dir,
        &["loc", "import", "work/ko.csv", "-o", "work/from-csv.json"],
    ));
    let from_json = read(&dir.join("work/from-json.json"));
    let from_csv = read(&dir.join("work/from-csv.json"));
    assert_eq!(from_json, from_csv);
    let ids = [TITLE_ID, SUBTITLE_ID];
    assert_eq!(
        bundle_rows(&from_json, &ids),
        vec![
            (
                json!({ "ko": "Welcome, {{userName}}: {{run.gold}} gold" }),
                json!({ "ko": lute_compile::locale::source_hash("Welcome, {{userName}}: {{run.gold}} gold") }),
            ),
            (
                json!({ "ko": "Hi there" }),
                json!({ "ko": lute_compile::locale::source_hash("Hi there") }),
            ),
        ]
    );
}

/// `loc report`: directive texts count like prompts — their words join the
/// totals (no speaker's) — under `directiveTexts`, shown only when nonzero.
#[test]
fn loc_report_counts_directive_texts_only_when_present() {
    let dir = project("dt-loc-report");
    write(
        &dir,
        "scenes/plain.lute",
        "---\nkind: scene\nid: plain\n---\n\n## Shot\n\n@narrator{code=\"0010\"}: Two words.\n",
    );
    let report: Value = serde_json::from_str(&ok(&lute(
        &dir,
        &["loc", "report", "--project", ".", "--json"],
    )))
    .unwrap();
    let docs = report["documents"].as_array().unwrap();
    let intro = docs
        .iter()
        .find(|d| d["file"] == "./scenes/intro.lute")
        .unwrap();
    let plain = docs
        .iter()
        .find(|d| d["file"] == "./scenes/plain.lute")
        .unwrap();
    assert_eq!(intro["directiveTexts"], 2);
    // `Welcome, userName: run.gold gold` (4) + `Hi there` (2) + `Hello.` (1).
    assert_eq!(intro["words"], 7);
    assert_eq!(
        intro["speakers"],
        json!([{ "speaker": "narrator", "lines": 1, "words": 1 }])
    );
    assert!(plain.get("directiveTexts").is_none(), "{plain:#}");
    assert_eq!(report["totals"]["directiveTexts"], 2);
    assert_eq!(report["totals"]["words"], 9);

    let human = ok(&lute(&dir, &["loc", "report", "--project", "."]));
    assert!(human.contains("directive texts"), "{human}");
    assert!(human.ends_with(", 2 directive text(s)\n"), "{human}");

    // A project without `text` attributes reports as before.
    std::fs::remove_file(dir.join("scenes/intro.lute")).unwrap();
    let report = ok(&lute(&dir, &["loc", "report", "--project", ".", "--json"]));
    assert!(!report.contains("directiveTexts"), "{report}");
    let human = ok(&lute(&dir, &["loc", "report", "--project", "."]));
    assert!(!human.contains("directive"), "{human}");
}

/// A tagged project with the ko title merged, compiled whole into `bundle/`.
fn compiled_with_ko(tag: &str) -> lute_scratch::ScratchDir {
    let dir = project(tag);
    ok(&lute(&dir, &["tag", "scenes/intro.lute"]));
    import_ko(&dir, &[(TITLE_ID, KO_TITLE)]);
    ok(&lute(
        &dir,
        &[
            "compile",
            "--project",
            ".",
            "-o",
            "bundle",
            "--locale-bundle",
            "work/bundle.json",
        ],
    ));
    dir
}

/// Runtime: the `plugin` record carries the rendered source in `attrs` and
/// each `text` attribute's `lineId` and rendered translations in
/// `localized`; a translation's markers take the source's values, a host
/// token stays verbatim.
#[test]
fn the_plugin_record_renders_its_text_attributes() {
    let dir = compiled_with_ko("dt-loc-runtime");
    let out = dir.join("bundle");
    let index = read(&out.join("project.index.json"));
    let artifacts: BTreeMap<String, Value> = index["documents"]
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
        serde_json::from_value(json!({"type": "raiseOccasion", "occasion": "hall"})).unwrap();
    let (_, output) = runtime
        .step(state, input)
        .unwrap_or_else(|(_, rejected)| panic!("{} {}", rejected.code, rejected.message));
    let plugin = output
        .events
        .into_iter()
        .find_map(|event| match event {
            Event::Record { record, .. } if record["kind"] == "plugin" => Some(record),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        plugin["attrs"],
        json!({
            "title": "Welcome, {{userName}}: 3 gold",
            "subtitle": "Hi there",
            "style": "card",
        })
    );
    assert_eq!(
        plugin["localized"],
        json!({
            "title": { "lineId": TITLE_ID, "texts": { "ko": "3 골드, {{userName}} 님 환영" } },
            "subtitle": { "lineId": SUBTITLE_ID },
        })
    );
}

/// `lute run --json` (the conformance record) leaves `localized` out, as it
/// leaves out a line's translations; `attrs` stays rendered.
#[test]
fn lute_run_leaves_localized_out() {
    let dir = compiled_with_ko("dt-loc-run");
    let index = read(&dir.join("bundle/project.index.json"));
    let artifact = format!(
        "bundle/{}",
        index["documents"][0]["artifact"].as_str().unwrap()
    );
    let transcript = ok(&lute(&dir, &["run", &artifact, "--json"]));
    assert!(
        transcript.contains("Welcome, {{userName}}: 3 gold"),
        "{transcript}"
    );
    assert!(!transcript.contains("localized"), "{transcript}");
    assert!(!transcript.contains(TITLE_ID), "{transcript}");
}

/// The bridge await's `attrs` carry the same rendered text as the record.
#[test]
fn the_bridge_await_carries_rendered_text_attributes() {
    let dir = project("dt-loc-bridge");
    write(
        &dir,
        "plugins/acme.ui/directives/ask.yaml",
        "directives:\n  - name: ask\n    attrs:\n      - { name: question, required: true, \
         type: text }\n    effects:\n      writes:\n        - { scope: run, path: [answer], \
         value: { fromBridgeResult: answer } }\n",
    );
    write(
        &dir,
        "schema/world.schema.yaml",
        "state:\n  run.gold: { type: int, default: 0 }\n  run.answer: { type: string }\n",
    );
    write(
        &dir,
        "scenes/intro.lute",
        "---\nkind: scene\nid: intro\non: hall\nonce: false\n---\n\n## Hall\n\n\
         ::set{run.gold = 4}\n::ask{code=\"0010\" question=\"Pay {{run.gold}}, {{userName}}?\"}\n\
         @narrator{code=\"0010\"}: You said {{run.answer}}.\n",
    );
    ok(&lute(&dir, &["compile", "--project", ".", "-o", "bundle"]));
    let out = dir.join("bundle");
    let index = read(&out.join("project.index.json"));
    let artifacts: BTreeMap<String, Value> = index["documents"]
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
        serde_json::from_value(json!({"type": "raiseOccasion", "occasion": "hall"})).unwrap();
    let (state, _) = runtime
        .step(state, input)
        .unwrap_or_else(|(_, rejected)| panic!("{} {}", rejected.code, rejected.message));
    let awaited = serde_json::to_value(state.current_await()).unwrap();
    assert_eq!(awaited["type"], "awaitBridge", "{awaited}");
    assert_eq!(
        awaited["attrs"],
        json!({ "question": "Pay 4, {{userName}}?" }),
        "{awaited}"
    );
}
