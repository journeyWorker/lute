//! dsl 0.43.0 §5 (#91): `lute schema` describes the command tree the parser
//! uses — every command, argument and option `--help` shows — as an
//! enveloped JSON report, narrowing one level per path word.

use std::collections::BTreeSet;
use std::process::Command;

use serde_json::Value;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn lute(args: &[&str]) -> (i32, String, String) {
    let out = Command::new(BIN).args(args).output().unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

fn schema(path: &[&str]) -> Value {
    let mut args = vec!["schema"];
    args.extend_from_slice(path);
    let (code, stdout, stderr) = lute(&args);
    assert_eq!(code, 0, "lute {args:?}: {stderr}");
    let value: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(value["command"], "schema");
    assert_eq!(value["schemaVersion"], "0.47.0.schema");
    assert_eq!(value["ok"], true);
    value
}

/// The `--long` options a `--help` page lists (minus `--help`/`--version`).
fn help_options(path: &[&str]) -> BTreeSet<String> {
    let mut args: Vec<&str> = path.to_vec();
    args.push("--help");
    let (code, stdout, _) = lute(&args);
    assert_eq!(code, 0);
    let options = stdout.split("\nOptions:\n").nth(1).unwrap_or_default();
    options
        .lines()
        .filter_map(|line| {
            let line = line.trim_start();
            let line = line
                .strip_prefix('-')
                .filter(|l| l.len() > 1 && l.as_bytes()[1] == b',')
                .map_or(line, |l| l[2..].trim_start());
            line.strip_prefix("--")
                .map(|rest| format!("--{}", rest.split([' ', '<', '[', '=']).next().unwrap()))
        })
        .filter(|o| o != "--help" && o != "--version")
        .collect()
}

/// The command names a `--help` page lists (minus `help`).
fn help_commands(path: &[&str]) -> Vec<String> {
    let mut args: Vec<&str> = path.to_vec();
    args.push("--help");
    let (_, stdout, _) = lute(&args);
    let Some(section) = stdout.split("\nCommands:\n").nth(1) else {
        return Vec::new();
    };
    section
        .lines()
        .take_while(|l| l.starts_with("  "))
        .map(|l| l.split_whitespace().next().unwrap().to_string())
        .filter(|name| name != "help")
        .collect()
}

#[test]
fn the_top_level_list_is_the_help_command_list() {
    let root = schema(&[]);
    let ids: Vec<String> = root["commands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(ids, help_commands(&[]));
    assert!(ids.iter().any(|id| id == "schema"));
    let scenario = root["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "scenario")
        .unwrap();
    assert_eq!(scenario["hasSubcommands"], true);
    assert!(scenario["summary"]
        .as_str()
        .unwrap()
        .starts_with("Report project connectivity"));
}

#[test]
fn every_command_describes_every_option_its_help_lists() {
    let root = schema(&[]);
    let mut paths: Vec<Vec<String>> = Vec::new();
    for command in root["commands"].as_array().unwrap() {
        let id = command["id"].as_str().unwrap().to_string();
        paths.push(vec![id.clone()]);
        for sub in help_commands(&[&id]) {
            paths.push(vec![id.clone(), sub]);
        }
    }
    assert!(paths.len() > 30, "{paths:?}");
    for path in paths {
        let words: Vec<&str> = path.iter().map(String::as_str).collect();
        let described = schema(&words);
        assert_eq!(described["id"], path.join(" "));
        let options: BTreeSet<String> = described["options"]
            .as_array()
            .unwrap()
            .iter()
            .map(|o| o["name"].as_str().unwrap().to_string())
            .filter(|n| n.starts_with("--"))
            .collect();
        assert_eq!(
            options,
            help_options(&words),
            "options of `{}`",
            path.join(" ")
        );
        let subcommands: Vec<String> = described["subcommands"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| {
                s["id"]
                    .as_str()
                    .unwrap()
                    .rsplit(' ')
                    .next()
                    .unwrap()
                    .to_string()
            })
            .collect();
        assert_eq!(
            subcommands,
            help_commands(&words),
            "subcommands of `{}`",
            path.join(" ")
        );
        // dsl 0.47.0 §5.2: every description has `inputs`; only `patch`
        // reads a structured request.
        let inputs = described["inputs"]
            .as_array()
            .unwrap_or_else(|| panic!("`{}` has no inputs list", path.join(" ")));
        assert_eq!(
            inputs.len(),
            usize::from(path == ["patch"]),
            "inputs of `{}`",
            path.join(" ")
        );
    }
}

#[test]
fn a_command_lists_its_arguments_options_and_outputs() {
    let check = schema(&["check"]);
    assert_eq!(check["arguments"][0]["name"], "FILE");
    assert_eq!(check["arguments"][0]["required"], false);
    assert_eq!(check["arguments"][0]["repeatable"], true);
    let options = check["options"].as_array().unwrap();
    let json = options.iter().find(|o| o["name"] == "--json").unwrap();
    assert_eq!(json["takesValue"], false);
    assert_eq!(json["required"], false);
    let deny = options.iter().find(|o| o["name"] == "--deny").unwrap();
    assert_eq!(deny["repeatable"], true);
    assert_eq!(deny["valueName"], "CODE");
    assert_eq!(
        check["outputs"],
        serde_json::json!([{"flag": "--json", "kind": "report", "schemaVersion": "0.48.0.check"}])
    );

    let reach = schema(&["scenario", "reach"]);
    assert!(
        reach["options"]
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o["name"] == "--format"),
        "global options propagate"
    );
    assert_eq!(
        reach["outputs"][0]["schemaVersion"],
        "0.43.0.scenario-reach"
    );

    let play = schema(&["play"]);
    let kinds: Vec<&str> = play["outputs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["report", "stream"]);
    assert_eq!(schema(&["fmt"])["outputs"], serde_json::json!([]));
    assert_eq!(schema(&["version"])["outputs"][0]["kind"], "artifact");
}

#[test]
fn an_unknown_path_is_a_usage_error_envelope() {
    for (path, argument) in [(&["nope"][..], "nope"), (&["scenario", "nope"][..], "nope")] {
        let mut args = vec!["schema"];
        args.extend_from_slice(path);
        let (code, stdout, stderr) = lute(&args);
        assert_eq!(code, 2);
        assert!(
            stderr.contains("lute schema: there is no command"),
            "{stderr}"
        );
        let value: Value = serde_json::from_str(&stdout).unwrap();
        assert_eq!(value["schemaVersion"], "0.43.0.error");
        assert_eq!(value["command"], "schema");
        assert_eq!(value["ok"], false);
        assert_eq!(value["error"]["code"], "E-CLI-USAGE");
        assert_eq!(value["error"]["argument"], argument);
    }
}
