//! dsl 0.52.0 §6 (#218, #212). #218: the issue's scene through `lute play
//! --events` and, with a `ko` bundle, through the runtime: the awaiting menu
//! carries the rendered prompt and option labels and their rendered
//! translations (on v0.51.0 its options carry no text). #212: whole-project
//! `check` reads every label-grammar value for quest references — a
//! directive `text` attribute and a prompt (on v0.51.0 the `::banner` read is
//! clean) — and the single-file note follows the same set.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::{Command, Output};

use lute_runtime::index::Bundle;
use lute_runtime::runtime::{Await, Input, Seed};
use lute_runtime::Runtime;
use serde_json::{json, Value as Json};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

const PROJECT: &str = "pluginsDir: plugins/\ndefaultProfile: game\nprofiles:\n  game:\n    \
                       plugins:\n      acme.ui: true\ndefaults:\n  contentLang: en\n  uses: \
                       [world.schema.yaml]\n";
const PLUGIN: &str = "id: acme.ui\nversion: 0.1.0\nkind: capability\ndepends:\n  - { id: \
                      lute.core, range: \"^0.0.1\" }\nexports:\n  directives: directives/\n  \
                      occasions: occasions/\n";
/// `::banner` has two `text` attributes; `::tag`'s `label` is a plain string.
const DIRECTIVES: &str = "directives:\n  - name: banner\n    attrs:\n      - { name: title, \
                          required: true, type: text }\n      - { name: subtitle, type: text }\n  \
                          - name: tag\n    attrs:\n      - { name: label, type: string }\n";
const OCCASIONS: &str = "occasions:\n  visit: { select: first }\n";
const SCHEMA: &str = "state:\n  run.gold: { type: int, default: 5 }\n";

/// The issue's scene, answering `visit`.
const GOLD: &str = "---\nkind: scene\nid: gold\non: visit\n---\n## Shot 1.\n<branch id=\"pay\" \
                    prompt=\"You have {{run.gold}} gold.\">\n  <choice id=\"yes\" text=\"Pay \
                    {{run.gold}} gold\">\n    @narrator{code=\"0010\"}: Paid.\n  </choice>\n  \
                    <choice id=\"no\" text=\"Keep it\">\n    @narrator{code=\"0020\"}: Kept.\n  \
                    </choice>\n</branch>\n";

/// The quest the #212 reads mistype (`finalle`).
const FINALE: &str = "---\nkind: quest\nid: finaleQuests\n---\n<quest id=\"finale\" \
                      start=\"true\">\n  <objective id=\"o\" done=\"run.gold > 9\"/>\n</quest>\n";

fn write(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, text).unwrap();
}

fn project(tag: &str, files: &[(&str, &str)]) -> lute_scratch::ScratchDir {
    let dir = lute_scratch::ScratchDir::named(tag);
    write(&dir, "lute.project.yaml", PROJECT);
    write(&dir, "plugins/acme.ui/plugin.yaml", PLUGIN);
    write(&dir, "plugins/acme.ui/directives/d.yaml", DIRECTIVES);
    write(&dir, "plugins/acme.ui/occasions/o.yaml", OCCASIONS);
    write(&dir, "world.schema.yaml", SCHEMA);
    for (rel, text) in files {
        write(&dir, rel, text);
    }
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

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

fn ok(out: &Output) {
    assert_eq!(out.status.code(), Some(0), "{}", text(out));
}

fn read(path: &Path) -> Json {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// The awaiting menu `lute play --events` prints for an unscripted branch.
fn played_menu(dir: &Path) -> Json {
    write(dir, "script.play.yaml", "steps:\n  - occasion: visit\n");
    let out = lute(
        dir,
        &[
            "play",
            "--project",
            ".",
            "--script",
            "script.play.yaml",
            "--events",
        ],
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| serde_json::from_str::<Json>(l).ok())
        .find_map(|v| {
            let menu = &v["output"]["await"]["menu"];
            (!menu.is_null()).then(|| menu.clone())
        })
        .unwrap_or_else(|| panic!("an awaitChoice menu: {}", text(&out)))
}

#[test]
fn issue_218_the_awaiting_menu_carries_the_rendered_text() {
    let dir = project("menu-text-218", &[("gold.lute", GOLD)]);
    ok(&lute(&dir, &["check", "--project", "."]));
    let menu = played_menu(&dir);
    assert_eq!(menu["prompt"], "You have 5 gold.", "{menu:#}");
    assert_eq!(
        menu["options"],
        json!([
            { "id": "yes", "text": "Pay 5 gold", "verdict": "open", "exit": false, "once": false },
            { "id": "no", "text": "Keep it", "verdict": "open", "exit": false, "once": false }
        ])
    );
    assert!(menu.get("promptTexts").is_none(), "{menu:#}");
}

/// Export the rows, translate the prompt and the `yes` label into ko (the
/// translation spaces its marker unlike the source), import, compile the
/// project with the bundle and step the runtime to the menu.
#[test]
fn a_ko_translation_of_a_label_and_a_prompt_renders_by_marker() {
    let dir = project("menu-text-218-ko", &[("gold.lute", GOLD)]);
    let exported = lute(&dir, &["loc", "export", "--project", "."]);
    ok(&exported);
    let rows: Vec<Json> = serde_json::from_slice::<Vec<Json>>(&exported.stdout)
        .unwrap()
        .into_iter()
        .map(|mut row| {
            match row["lineId"].as_str().unwrap_or_default() {
                "gold.pay.prompt" => row["text"] = json!("금화가 {{ run.gold }}개 있다."),
                "gold.pay.yes" => row["text"] = json!("{{run.gold}}골드 낸다"),
                _ => {}
            }
            row
        })
        .collect();
    write(&dir, "work/ko.json", &serde_json::to_string(&rows).unwrap());
    ok(&lute(
        &dir,
        &["loc", "import", "work/ko.json", "-o", "work/bundle.json"],
    ));
    ok(&lute(
        &dir,
        &[
            "compile",
            "--project",
            ".",
            "-o",
            "out",
            "--locale-bundle",
            "work/bundle.json",
        ],
    ));
    let out = dir.join("out");
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
        serde_json::from_value(json!({ "type": "raiseOccasion", "occasion": "visit" })).unwrap();
    let (_, output) = runtime
        .step(state, input)
        .unwrap_or_else(|(_, rejected)| panic!("{} {}", rejected.code, rejected.message));
    let Await::Choice { menu, .. } = output.await_ else {
        panic!("an awaitChoice: {:?}", output.await_);
    };
    let menu = serde_json::to_value(&menu).unwrap();
    assert_eq!(menu["prompt"], "You have 5 gold.");
    assert_eq!(menu["promptTexts"], json!({ "ko": "금화가 5개 있다." }));
    assert_eq!(menu["options"][0]["text"], "Pay 5 gold");
    assert_eq!(menu["options"][0]["texts"], json!({ "ko": "5골드 낸다" }));
    assert_eq!(menu["options"][1]["text"], "Keep it");
    // An untranslated row is imported with its source text.
    assert_eq!(menu["options"][1]["texts"], json!({ "ko": "Keep it" }));
}

/// `body` as a scene that answers no occasion.
fn scene(id: &str, body: &str) -> String {
    format!("---\nkind: scene\nid: {id}\n---\n## Shot 1.\n{body}\n")
}

/// The `E-QUEST-REF-UNKNOWN` lines of `lute check --project .`.
fn quest_refs(dir: &Path) -> Vec<String> {
    text(&lute(dir, &["check", "--project", "."]))
        .lines()
        .filter(|l| l.contains("E-QUEST-REF-UNKNOWN"))
        .map(str::to_string)
        .collect()
}

#[test]
fn issue_212_a_text_attribute_read_is_judged_project_wide() {
    let dir = project(
        "menu-text-212",
        &[
            ("finale.lute", FINALE),
            (
                "s.lute",
                &scene(
                    "s",
                    "::banner{code=\"0010\" title=\"Done\" subtitle=\"{{quest.finalle.state}}\"}",
                ),
            ),
        ],
    );
    let hits = quest_refs(&dir);
    assert_eq!(hits.len(), 1, "{hits:#?}");
    assert!(hits[0].starts_with("./s.lute:6:"), "{hits:#?}");
    assert!(hits[0].contains("quest.finalle.state"), "{hits:#?}");
}

#[test]
fn a_prompt_read_is_judged_project_wide() {
    for prompt_owner in [
        "<branch id=\"b\" prompt=\"{{quest.finalle.state}}\">\n  <choice id=\"c\" text=\"C\">\n    \
         @narrator: c\n  </choice>\n</branch>",
        "<hub id=\"h\" prompt=\"{{quest.finalle.state}}\">\n  <choice id=\"c\" text=\"C\" exit>\n    \
         @narrator: c\n  </choice>\n</hub>",
    ] {
        let dir = project(
            "menu-text-212-prompt",
            &[("finale.lute", FINALE), ("s.lute", &scene("s", prompt_owner))],
        );
        let hits = quest_refs(&dir);
        assert_eq!(hits.len(), 1, "{prompt_owner}\n{hits:#?}");
        assert!(hits[0].contains("quest.finalle.state"), "{hits:#?}");
        // The single-file check names the read it leaves to the project pass.
        let single = text(&lute(&dir, &["check", "s.lute"]));
        assert!(
            single.contains("W-PROJECT-CHECK-SKIPPED") && single.contains("quest `finalle`"),
            "{single}"
        );
    }
}

/// A plain string attribute is no label: its `{{…}}` is not a read.
#[test]
fn a_directive_without_text_attributes_is_not_scanned() {
    let dir = project(
        "menu-text-212-plain",
        &[
            ("finale.lute", FINALE),
            (
                "s.lute",
                &scene("s", "::tag{label=\"{{quest.finalle.state}}\"}"),
            ),
        ],
    );
    assert!(quest_refs(&dir).is_empty());
}
