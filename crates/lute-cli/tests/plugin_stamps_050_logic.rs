//! 0.50.0 §3.4–§3.5 (#164): plugin stamps on `<branch>`, `<hub>` and
//! `<choice>`.
//!
//! The fixture `plugin-stamps-050-logic` declares `cue` on all three
//! elements, `menuType` (an enum) on `<branch>` and `<hub>`, `optionKey`
//! required on `<choice>`, and a cross-cutting `track`, which no logic
//! element admits. Its project defs `@m` (int), `@k` (string) and `@g`
//! (string folding to `"grid"`) exercise the `@ref` typing. On 0.49.1 the
//! `on:` key is `E-PLUGIN-KEY` and the plugin fails to load, so every case
//! below fails there.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{json, Value as Json};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/plugin-stamps-050-logic")
}

fn lute(args: &[&str]) -> Output {
    Command::new(BIN).args(args).output().unwrap()
}

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// `line:col: error [CODE] message` for every diagnostic line of `out`.
fn diagnostics(out: &Output) -> Vec<String> {
    text(out)
        .lines()
        .filter_map(|l| {
            let (_, rest) = l.split_once("bad.lute:")?;
            let (at, d) = rest.split_once(": error ")?;
            Some(format!("{at} {d}"))
        })
        .collect()
}

fn run(verb: &str, scene: &str) -> Output {
    let project = fixture();
    let file = project.join("scenes").join(scene);
    lute(&[
        verb,
        "--project",
        project.to_str().unwrap(),
        file.to_str().unwrap(),
    ])
}

#[test]
fn logic_element_stamps_are_resolved_typed_and_required() {
    let out = run("check", "bad.lute");
    assert!(!out.status.success(), "{}", text(&out));
    assert_eq!(
        diagnostics(&out),
        [
            // `<branch menuType=@m>`, `@m` an int def: the folded literal is
            // outside the enum, and the reference itself has the wrong type
            // (the pair a directive attribute reports too).
            "9:29 [E-BAD-ENUM] `3` is not a valid value for `menuType` of `<branch>` (expected \
             one of: plain, timed, minigame)",
            "9:29 [E-REF-TYPE] `@m` produces an int but this position expects an enum",
            // A cross-cutting stamp never applies to a logic element.
            "9:32 [E-UNKNOWN-ATTR] `<branch>` has no attribute `track`; its attributes are \
             `id`, `prompt`, `timeout`, `default` and the stamps `cue` (`acme.logic`) and \
             `menuType` (`acme.logic`)",
            "9:43 [E-UNKNOWN-ATTR] `<branch>` has no attribute `optionKey`: it is a stamp of \
             `acme.logic` for `<choice>` only",
            "10:3 [E-MISSING-ATTR] `<choice id=\"goLeft\">` requires attribute `optionKey` (a \
             stamp of `acme.logic`)",
            "10:35 [E-UNKNOWN-ATTR] `<choice>` has no attribute `menuType`: it is a stamp of \
             `acme.logic` for `<branch>` and `<hub>` only",
            "13:52 [E-UNKNOWN-ATTR] `<choice>` has no attribute `cu` — did you mean `cue`?; its \
             attributes are `id`, `text`, `when`, `into`, `value` and the stamps `cue` \
             (`acme.logic`) and `optionKey` (`acme.logic`)",
            // `<hub menuType=@g>`, `@g` folding to `"grid"`.
            "17:25 [E-BAD-ENUM] `grid` is not a valid value for `menuType` of `<hub>` (expected \
             one of: plain, timed, minigame)",
            "17:28 [E-UNKNOWN-ATTR] `<hub>` has no attribute `optionKey`: it is a stamp of \
             `acme.logic` for `<choice>` only",
            "18:3 [E-MISSING-ATTR] `<choice id=\"stay\">` requires attribute `optionKey` (a \
             stamp of `acme.logic`)",
        ],
        "{}",
        text(&out)
    );
}

#[test]
fn a_string_def_satisfies_a_string_stamp() {
    // `<choice optionKey=@k>` with `@k` a string def checks clean.
    let out = run("check", "ok.lute");
    assert!(out.status.success(), "{}", text(&out));
}

/// Every record of `kind` in `ir`'s commands.
fn records<'a>(ir: &'a Json, kind: &str) -> Vec<&'a Json> {
    ir["commands"]
        .as_array()
        .map(|cmds| cmds.iter().filter(|c| c["kind"] == kind).collect())
        .unwrap_or_default()
}

#[test]
fn element_and_option_stamps_reach_the_ir() {
    let out = run("compile", "ok.lute");
    assert!(out.status.success(), "{}", text(&out));
    let ir: Json = serde_json::from_slice(&out.stdout).unwrap();
    let options = |record: &Json| -> Vec<Json> {
        record["options"]
            .as_array()
            .unwrap()
            .iter()
            .map(|o| o.get("stamps").cloned().unwrap_or(Json::Null))
            .collect()
    };

    let [choice] = records(&ir, "choice")[..] else {
        panic!("one `choice` record expected in {ir:#}")
    };
    assert_eq!(
        choice["stamps"],
        json!({ "cue": "b1", "menuType": "plain" })
    );
    assert_eq!(
        options(choice),
        [
            json!({ "cue": "c1", "optionKey": "L1" }),
            // `optionKey=@k` ships the folded literal.
            json!({ "optionKey": "L1" }),
        ]
    );

    let [hub] = records(&ir, "hub")[..] else {
        panic!("one `hub` record expected in {ir:#}")
    };
    assert_eq!(hub["stamps"], json!({ "cue": "h1", "menuType": "timed" }));
    assert_eq!(
        options(hub),
        [
            json!({ "cue": "hc1", "optionKey": "W1" }),
            json!({ "optionKey": "X1" }),
        ]
    );
    // Serialized last within the option (the raw bytes keep field order).
    let raw: String = String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .collect();
    assert!(
        raw.contains(r#""target":"001-0200","stamps":{"cue":"c1","optionKey":"L1"}}"#),
        "{raw}"
    );
}
