//! dsl 0.53.0 §8.1 (#241): a translated text shows its source's `{{…}}`
//! markers. Each marker parses under the source rules (the §8 brace advice
//! included), and the markers are the source's, each as often, in any order
//! (the spaces at a marker's edges do not count). Any other translation is
//! `E-L10N-MARKERS` at `compile --locale-bundle` and is not merged;
//! `loc import` refuses a marker that does not parse. On v0.52.0 every one of
//! them merges verbatim (exit 0) and the runtime shows the marker as written.

use std::path::Path;
use std::process::{Command, Output};

use serde_json::{json, Value};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

const PROJECT: &str = "defaultProfile: game\nprofiles:\n  game: {}\ndefaults:\n  contentLang: en\n";

/// Scene `s`: a line, and a branch with a prompt and an option label, each
/// showing `run.a` (and the line `run.b`).
const SCENE: &str = "---\nkind: scene\nid: s\nstate:\n  run.a: { type: int, default: 3 }\n  \
                     run.b: { type: int, default: 4 }\n---\n\n## S\n\n\
                     @narrator{code=\"0010\"}: S {{run.a}} / {{run.b}} E\n\
                     <branch id=\"b\" prompt=\"P {{run.a}}\">\n<choice id=\"c\" text=\"Take \
                     {{run.a}}\">\n@narrator{code=\"0020\"}: ok\n</choice>\n</branch>\n";

const LINE: &str = "s.narrator_0010";

fn write(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, text).unwrap();
}

fn project(tag: &str) -> lute_scratch::ScratchDir {
    let dir = lute_scratch::ScratchDir::named(tag);
    write(&dir, "lute.project.yaml", PROJECT);
    write(&dir, "s.lute", SCENE);
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

/// Compile `s.lute` with a `ko` bundle of `entries` (lineId → text).
fn compile(dir: &Path, entries: &[(&str, &str)]) -> Output {
    let entries: serde_json::Map<String, Value> = entries
        .iter()
        .map(|(id, t)| ((*id).to_string(), json!({ "ko": t })))
        .collect();
    let bundle = json!({ "schemaVersion": 1, "locales": ["ko"], "entries": entries });
    write(dir, "b.json", &bundle.to_string());
    let out_path = dir.join("s.json");
    lute(
        dir,
        &[
            "compile",
            "s.lute",
            "--project",
            ".",
            "-o",
            out_path.to_str().unwrap(),
            "--locale-bundle",
            "b.json",
        ],
    )
}

/// The `E-L10N-MARKERS` lines of `out`.
fn refused(out: &Output) -> Vec<String> {
    text(out)
        .lines()
        .filter(|l| l.contains("[E-L10N-MARKERS]"))
        .map(str::to_string)
        .collect()
}

const PARITY: &str = "a translation shows each `{{…}}` marker of its source as often as the \
                      source does, in any order";

/// The issue's repro: a marker opened on a literal brace is refused with the
/// §8 advice.
#[test]
fn issue_241_a_brace_opened_marker_in_a_translation_is_refused() {
    let dir = project("l10n-markers-brace");
    let out = compile(&dir, &[(LINE, "S {{{run.a}} / {{run.b}} E")]);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert_eq!(
        refused(&out),
        [format!(
            "s.lute:1:1: error [E-L10N-MARKERS] `ko` text for `{LINE}` is not merged: its markers \
             do not parse (E-INTERP-BRACE: a literal `{{` directly before `{{{{run.a}}}}` cannot \
             be told apart from the marker in compiled text (lute 0.47.1 §3): put a character \
             between them (`{{ {{{{run.a}}}}`), or show the brace from a string state path \
             (`{{{{run.lbrace}}}}{{{{run.a}}}}`, with `run.lbrace` declared `{{ type: string, \
             default: \"{{\" }}`))"
        )]
    );
}

/// A path the source does not show, a missing marker and an extra one are
/// each refused, naming what differs.
#[test]
fn a_translation_must_show_the_source_markers_each_as_often() {
    for (ko, differs) in [
        (
            "S {{run.zzz}} / {{run.b}} E",
            "missing `{{run.a}}`; extra `{{run.zzz}}`",
        ),
        ("S / {{run.b}} E", "missing `{{run.a}}`"),
        ("S {{run.a}} {{run.a}} / {{run.b}} E", "extra `{{run.a}}`"),
        (
            "S {{run.a + 1}} / {{run.b}} {{run.b}} {{run.b}} E",
            "missing `{{run.a}}`; extra `{{run.a + 1}}`, `{{run.b}}`×2",
        ),
    ] {
        let dir = project("l10n-markers-parity");
        let out = compile(&dir, &[(LINE, ko)]);
        assert_eq!(out.status.code(), Some(1), "{ko}: {}", text(&out));
        assert_eq!(
            refused(&out),
            [format!(
                "s.lute:1:1: error [E-L10N-MARKERS] `ko` text for `{LINE}` is not merged: its \
                 markers differ from the source's ({differs}): {PARITY}"
            )],
            "{ko}"
        );
    }
}

/// The source's markers in another order, spaced differently, merge.
#[test]
fn the_same_markers_in_any_order_merge() {
    let dir = project("l10n-markers-clean");
    let out = compile(
        &dir,
        &[
            (LINE, "{{ run.b }} 다음 {{run.a}} 끝"),
            ("s.b.prompt", "고르기 {{run.a}}"),
            ("s.b.c", "{{run.a}} 받기"),
        ],
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let artifact: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("s.json")).unwrap()).unwrap();
    let cmds = artifact["commands"].as_array().unwrap();
    let line = cmds.iter().find(|c| c["lineId"] == LINE).unwrap();
    assert_eq!(line["texts"]["ko"], "{{ run.b }} 다음 {{run.a}} 끝");
    let choice = cmds.iter().find(|c| c["kind"] == "choice").unwrap();
    assert_eq!(choice["promptTexts"]["ko"], "고르기 {{run.a}}");
    assert_eq!(choice["options"][0]["texts"]["ko"], "{{run.a}} 받기");
}

/// A prompt and an option label are held to their source too.
#[test]
fn a_prompt_and_an_option_label_are_judged_too() {
    let dir = project("l10n-markers-labels");
    let out = compile(
        &dir,
        &[("s.b.prompt", "고르기"), ("s.b.c", "{{run.zzz}} 받기")],
    );
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert_eq!(
        refused(&out),
        [
            format!(
                "s.lute:1:1: error [E-L10N-MARKERS] `ko` text for `s.b.prompt` is not merged: its \
                 markers differ from the source's (missing `{{{{run.a}}}}`): {PARITY}"
            ),
            format!(
                "s.lute:1:1: error [E-L10N-MARKERS] `ko` text for `s.b.c` is not merged: its \
                 markers differ from the source's (missing `{{{{run.a}}}}`; extra \
                 `{{{{run.zzz}}}}`): {PARITY}"
            ),
        ]
    );
}

/// `loc import` refuses a marker that does not parse under its row's source
/// grammar and writes no bundle: an unterminated `{{` in a line, a marker
/// opened on a literal brace anywhere. A `{{` with no `}}` in a prompt is
/// literal text (the label grammar), and a marker it cannot compare without
/// the source imports.
#[test]
fn loc_import_refuses_a_marker_that_does_not_parse() {
    let dir = project("l10n-markers-import");
    let exported = lute(&dir, &["loc", "export", "--project", "."]);
    assert_eq!(exported.status.code(), Some(0), "{}", text(&exported));
    let rows: Vec<Value> = serde_json::from_slice(&exported.stdout).unwrap();
    let translate = |line: &str, prompt: &str| -> String {
        let rows: Vec<Value> = rows
            .iter()
            .cloned()
            .map(|mut row| {
                match row["lineId"].as_str().unwrap_or_default() {
                    LINE => row["text"] = json!(line),
                    "s.b.prompt" => row["text"] = json!(prompt),
                    _ => {}
                }
                row
            })
            .collect();
        serde_json::to_string(&rows).unwrap()
    };
    write(
        &dir,
        "work/ko.json",
        &translate("S {{run.a E", "P {{{run.a}}"),
    );
    let out = lute(
        &dir,
        &["loc", "import", "work/ko.json", "-o", "work/b.json"],
    );
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(!dir.join("work/b.json").exists());
    let lines: Vec<&str> = std::str::from_utf8(&out.stderr).unwrap().lines().collect();
    assert_eq!(lines.len(), 3, "{lines:#?}");
    assert_eq!(
        lines[0],
        format!(
            "work/ko.json: error [E-L10N-MARKERS] row 1: `ko` text for `{LINE}` cannot be \
             merged: its markers do not parse (E-INTERP-UNTERMINATED: `{{{{` has no closing \
             `}}}}`)"
        ),
        "{lines:#?}"
    );
    assert!(
        lines[1].starts_with(
            "work/ko.json: error [E-L10N-MARKERS] row 2: `ko` text for `s.b.prompt` cannot be \
             merged: its markers do not parse (E-INTERP-BRACE: a literal `{` directly before \
             `{{run.a}}`"
        ),
        "{lines:#?}"
    );
    assert_eq!(lines[2], "2 E-L10N-MARKERS error(s); no bundle emitted");

    write(
        &dir,
        "work/ko.json",
        &translate("S {{run.zzz}} E", "P {{run.a}} {{"),
    );
    let out = lute(
        &dir,
        &["loc", "import", "work/ko.json", "-o", "work/b.json"],
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
}

/// Scene `lit`: a prompt and an option label whose `{{` has no `}}` — literal
/// text in the label grammar — and a line with a marker.
const LITERAL_SCENE: &str = "---\nkind: scene\nid: lit\nstate:\n  run.a: { type: int, default: \
                             3 }\n---\n\n## L\n\n@narrator{code=\"0010\"}: S {{run.a}} E\n\
                             <branch id=\"b\" prompt=\"Pick {{ now\">\n<choice id=\"c\" \
                             text=\"Literal {{\">\n@narrator{code=\"0020\"}: ok\n</choice>\n\
                             </branch>\n";

/// A translation is read by its source surface's grammar: a prompt and an
/// option label whose source shows a `{{` with no `}}` as literal text
/// merge a translation that does too (v0.53 before this fix: refused,
/// `E-INTERP-UNTERMINATED`); a line, whose grammar refuses it, still
/// refuses its translation.
#[test]
fn a_translation_follows_its_surface_grammar() {
    let dir = lute_scratch::ScratchDir::named("l10n-markers-literal");
    write(&dir, "lute.project.yaml", PROJECT);
    write(&dir, "lit.lute", LITERAL_SCENE);
    let check = lute(&dir, &["check", "lit.lute", "--project", "."]);
    assert_eq!(check.status.code(), Some(0), "{}", text(&check));
    let bundle = |line: &str| {
        json!({ "schemaVersion": 1, "locales": ["ko"], "entries": {
            "lit.narrator_0010": { "ko": line },
            "lit.b.prompt": { "ko": "고르기 {{ 지금" },
            "lit.b.c": { "ko": "리터럴 {{" },
        }})
        .to_string()
    };
    let compile = |dir: &Path| {
        lute(
            dir,
            &[
                "compile",
                "lit.lute",
                "--project",
                ".",
                "-o",
                dir.join("lit.json").to_str().unwrap(),
                "--locale-bundle",
                "b.json",
            ],
        )
    };
    write(&dir, "b.json", &bundle("S {{run.a}} 끝"));
    let out = compile(&dir);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let artifact: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("lit.json")).unwrap()).unwrap();
    let choice = artifact["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["kind"] == "choice")
        .unwrap();
    assert_eq!(choice["promptTexts"]["ko"], "고르기 {{ 지금", "{choice}");
    assert_eq!(choice["options"][0]["texts"]["ko"], "리터럴 {{", "{choice}");

    // The line grammar refuses a `{{` with no `}}`: not merged.
    write(&dir, "b.json", &bundle("S {{run.a}} 끝 {{"));
    let out = compile(&dir);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    let refused: Vec<String> = text(&out)
        .lines()
        .filter(|l| l.contains("`ko` text for `lit.narrator_0010` is not merged"))
        .map(str::to_string)
        .collect();
    assert_eq!(refused.len(), 1, "{}", text(&out));
    assert!(refused[0].contains("E-INTERP-UNTERMINATED"), "{refused:?}");
}
