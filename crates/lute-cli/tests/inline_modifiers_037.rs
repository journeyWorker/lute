//! dsl 0.37.0 §3.6/§6 through the CLI, on the lighthouse-keeper corpus scene
//! that uses inline modifiers: `loc export` keeps the authored markup,
//! `compile --locales` refuses a translation whose modifiers differ
//! (`E-L10N-MODIFIERS`), `play --json` line events carry `segments` beside
//! the plain `text`, and `context` describes the modifier surface.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{json, Value as Json};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn game() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/examples/games/lighthouse-keeper")
}

fn run(args: &[&str]) -> Output {
    Command::new(BIN).args(args).output().unwrap()
}

fn text(o: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

const LINE_ID: &str = "rock.arrival.inspector_0020";

#[test]
fn loc_export_keeps_the_authored_markup() {
    let out = run(&["loc", "export", game().to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let all = text(&out);
    assert!(all.contains("A lamp does :emphasis[not] relight itself."), "{all}");
}

fn compile_with(tag: &str, translation: &str) -> Output {
    let dir = std::env::temp_dir().join(format!("lute-mod037-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let bundle = dir.join("bundle.json");
    let body = json!({
        "schemaVersion": 1,
        "locales": ["ja-JP"],
        "entries": { LINE_ID: { "ja-JP": translation } }
    });
    std::fs::write(&bundle, body.to_string()).unwrap();
    let scene = game().join("scenes/arrival.lute");
    run(&["compile", scene.to_str().unwrap(), "--locales", bundle.to_str().unwrap()])
}

#[test]
fn compile_locales_merges_plain_texts_and_locale_segments() {
    let out = compile_with("ok", "ランプは :emphasis[ひとりでに] 灯らない。");
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let ir: Json = serde_json::from_slice(&out.stdout).unwrap();
    let line = ir["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["lineId"] == LINE_ID)
        .unwrap();
    assert_eq!(line["texts"]["ja-JP"], "ランプは ひとりでに 灯らない。");
    assert_eq!(
        line["localeSegments"]["ja-JP"],
        json!([
            {"text": "ランプは "},
            {"text": "ひとりでに", "styles": ["emphasis"]},
            {"text": " 灯らない。"}
        ])
    );
}

#[test]
fn compile_locales_refuses_a_translation_with_other_modifiers() {
    let out = compile_with("bad", "ランプは :whisper[ひとりでに] 灯らない。");
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(text(&out).contains("E-L10N-MODIFIERS"), "{}", text(&out));
    assert!(out.stdout.is_empty(), "no execution IR on a refused translation");
}

#[test]
fn play_json_line_events_carry_segments_and_plain_text() {
    let g = game();
    let script = g.join("plays/true-account.play.yaml");
    let out = run(&["play", g.to_str().unwrap(), "--script", script.to_str().unwrap(), "--json"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let v: Json = serde_json::from_slice(&out.stdout).unwrap();
    let mut found = None;
    for step in v["steps"].as_array().unwrap() {
        for r in step["presented"]["commands"].as_array().into_iter().flatten() {
            if r["lineId"] == LINE_ID {
                found = Some(r.clone());
            }
        }
    }
    let line = found.expect("the modified line is presented");
    assert_eq!(line["text"], "Dark, then lit again. A lamp does not relight itself.");
    assert_eq!(
        line["segments"],
        json!([
            {"text": "Dark, then lit again. A lamp does "},
            {"text": "not", "styles": ["emphasis"]},
            {"text": " relight itself."}
        ])
    );
}

#[test]
fn context_describes_core_modifiers_and_text_styles() {
    let scene = game().join("scenes/arrival.lute");
    let out = run(&["context", scene.to_str().unwrap(), "--json"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let v: Json = serde_json::from_slice(&out.stdout).unwrap();
    let mods = &v["textModifiers"];
    let core: Vec<&str> = mods["core"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["name"].as_str().unwrap())
        .collect();
    assert_eq!(core, ["pause", "speed"]);
    assert_eq!(mods["textStyle"], json!(["emphasis", "whisper"]));

    let human = run(&["context", scene.to_str().unwrap()]);
    let human = text(&human);
    assert!(human.contains(":pause{s=0.5} [leaf]"), "{human}");
    assert!(human.contains(":emphasis[text], :whisper[text]"), "{human}");
}

fn write(dir: &Path, rel: &str, body: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, body).unwrap();
}

/// A styled run takes the value of ITS marker: markers map to placeholders
/// by global index across the whole line, the same placeholder twice
/// included, and each segment's text renders like the line text.
#[test]
fn play_json_segments_render_each_marker_by_its_global_index() {
    let dir = std::env::temp_dir().join(format!("lute-mod037-interp-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    write(
        &dir,
        "lute.project.yaml",
        "pluginsDir: plugins/\ndefaultProfile: g\nprofiles:\n  g:\n    plugins: { g.o: true }\n\
         defaults:\n  uses: [world.schema.yaml]\n",
    );
    write(
        &dir,
        "plugins/g.o/plugin.yaml",
        "id: g.o\nversion: 0.1.0\nkind: capability\ndepends: [ { id: lute.core, range: \"^0.0.1\" } ]\n\
         exports:\n  occasions: occasions/\n",
    );
    write(&dir, "plugins/g.o/occasions/o.yaml", "occasions:\n  visit: {}\n");
    write(
        &dir,
        "world.schema.yaml",
        "state:\n  run.a: { type: int, default: 1 }\n  run.b: { type: int, default: 2 }\n\
         enums:\n  textStyle: [emphasis]\n",
    );
    write(
        &dir,
        "scenes/s.lute",
        "---\nkind: scene\nid: s\non: visit\n---\n\n## A\n\n\
         @narrator: outside {{run.a}} :emphasis[inside {{run.b}} and {{run.a}}] end {{run.b}}\n",
    );
    write(&dir, "s.play.yaml", "steps:\n  - occasion: visit\n");
    let script = dir.join("s.play.yaml");
    let out = run(&["play", dir.to_str().unwrap(), "--script", script.to_str().unwrap(), "--json"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let v: Json = serde_json::from_slice(&out.stdout).unwrap();
    let line = v["steps"][0]["presented"]["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["kind"] == "line")
        .unwrap()
        .clone();
    assert_eq!(line["text"], "outside 1 inside 2 and 1 end 2");
    assert_eq!(
        line["segments"],
        json!([
            {"text": "outside 1 "},
            {"text": "inside 2 and 1", "styles": ["emphasis"]},
            {"text": " end 2"}
        ])
    );
}
