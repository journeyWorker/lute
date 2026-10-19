//! 0.48.0 §2.2, §3.3–§3.6a, §3.9, §3.11–§3.15, §5: the surface of the commands
//! the grammar cutover renamed or re-homed (`impact`, `constraints`,
//! `context`, `trace`, `expr`, `patch`, `play`, `test`, `run`, `calendar`,
//! `scenario`, `loc`, `new`, `catalog refresh`, …): every removed spelling
//! answers with its replacement, each new spelling does what the old one did,
//! and each option the cutover threaded into a command (`--providers`,
//! `--permission-profile`, `--deny*`) changes that command's result. Every
//! test fails on 0.47.2 (the spelling does not parse, the option is not
//! known, or — for the `none found` rule — the command ran on the current
//! directory).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

const BIN: &str = env!("CARGO_BIN_EXE_lute");
const CORE_PROJECT: &str = "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n";

fn write(dir: &Path, rel: &str, text: &str) -> PathBuf {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, text).unwrap();
    path
}

fn lute_in(cwd: &Path, args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap()
}

fn lute(args: &[&str]) -> Output {
    Command::new(BIN).args(args).output().unwrap()
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn all(out: &Output) -> String {
    format!("{}{}", stdout(out), stderr(out))
}

fn json(out: &Output) -> Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| panic!("{e}: {}", all(out)))
}

fn s(path: &Path) -> &str {
    path.to_str().unwrap()
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn examples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/examples")
}

fn assert_usage(out: &Output, argument: &str, in_message: &[&str]) {
    assert_eq!(out.status.code(), Some(2), "{}", all(out));
    let v = json(out);
    assert_eq!(v["error"]["code"], "E-CLI-USAGE", "{v}");
    assert_eq!(v["error"]["argument"], argument, "{v}");
    let message = v["error"]["message"].as_str().unwrap();
    for want in in_message {
        assert!(message.contains(want), "{want}: {v}");
    }
}

// --- fixtures ---------------------------------------------------------------

/// A project whose one scene `a` names the provider id `hero` through a
/// plugin directive: the id resolves only against `providers/`.
fn provider_project(tag: &str) -> lute_scratch::ScratchDir {
    let dir = lute_scratch::ScratchDir::named(tag);
    write(
        &dir,
        "plugins/demo.plugin/plugin.yaml",
        "id: demo.plugin\nversion: 0.1.0\nkind: capability\n\
         depends: [ { id: lute.core, range: \"^0.0.1\" } ]\nexports:\n  directives: directives/\n",
    );
    write(
        &dir,
        "plugins/demo.plugin/directives/announce.yaml",
        "directives:\n  - name: announce\n    attrs:\n      - { name: id, required: true, type: { providerRef: speakerId } }\n",
    );
    write(
        &dir,
        "providers/speakers.yaml",
        "manifestVersion: test\nproviderVersion: \"1\"\nstale: false\nentries:\n  speakerId: [hero]\n",
    );
    write(
        &dir,
        "scenes/a.lute",
        "---\nkind: scene\nid: a\n---\n\n## A\n\n::announce{id=\"hero\"}\n@narrator: hi\n",
    );
    write(
        &dir,
        "plays/p.play.yaml",
        "steps:\n  - engine: { state: { run.n: 1 } }\n",
    );
    write(
        &dir,
        "world.schema.yaml",
        "state:\n  run.n: { type: int, default: 0 }\n",
    );
    write(
        &dir,
        "lute.project.yaml",
        "pluginsDir: plugins/\ndefaultProfile: demo\nprofiles:\n  demo:\n    plugins: { demo.plugin: true }\n\
         defaults:\n  uses: [world.schema.yaml]\n",
    );
    write(
        &dir,
        "tests/a.test.yaml",
        "file: ../scenes/a.lute\nexpect:\n  end: complete\n",
    );
    dir
}

/// A day-clock town (two slots, two places) for the calendar tests.
fn town(tag: &str) -> lute_scratch::ScratchDir {
    let d = lute_scratch::ScratchDir::named(tag);
    write(
        &d,
        "lute.project.yaml",
        "pluginsDir: plugins/\ndefaultProfile: town\nprofiles:\n  town:\n    plugins: { town.clock: true }\n\
         defaults:\n  uses: [world.schema.yaml]\n",
    );
    write(
        &d,
        "plugins/town.clock/plugin.yaml",
        "id: town.clock\nversion: 0.1.0\nkind: capability\ndepends: [ { id: lute.core, range: \"^0.0.1\" } ]\n\
         exports:\n  occasions: occasions/\n",
    );
    write(
        &d,
        "plugins/town.clock/occasions/clock.yaml",
        "occasions:\n  dayStart: {}\n  placeVisit: { target: true }\n",
    );
    write(
        &d,
        "world.schema.yaml",
        "state:\n  run.day: { type: int, default: 1 }\n  run.slot: { type: { enum: [morning, night] }, default: morning }\n\
         entities:\n  person: { members: [ada, bo] }\n  place: { members: [inn, dock] }\n\
         relations:\n  present: { args: [person, place], derive: true }\n\
         rules:\n  - \"present(ada, inn) :- cel(\\\"run.slot == 'night'\\\")\"\n\
         \x20 - \"present(bo, dock) :- cel(\\\"run.slot == 'morning'\\\")\"\n",
    );
    for (file, id, keys) in [
        (
            "dawn",
            "town.dawn",
            "on: dayStart\nonce: false\nwhen: \"run.slot == 'morning'\"\n",
        ),
        (
            "memo",
            "town.memo",
            "on: dayStart\nonce: false\nwhen: 'run.day >= 1'\n",
        ),
        (
            "day2",
            "town.day2",
            "on: dayStart\nwhen: 'run.day == 2'\npriority: 10\n",
        ),
        ("inn", "inn.visit", "on: placeVisit\ntarget: place.inn\n"),
    ] {
        write(
            &d,
            &format!("scenes/{file}.lute"),
            &format!(
                "---\nkind: scene\nid: {id}\n{keys}---\n\n## {id}\n\n@narrator: A moment passes.\n"
            ),
        );
    }
    d
}

// --- removed spellings (§5) -------------------------------------------------

#[test]
fn removed_spellings_answer_with_the_replacement_in_text_and_envelope() {
    // (command line, `command` of the envelope, `argument`, the rewritten
    // command's words the message and the hint must carry)
    let rows: &[(&[&str], &str, &str, &[&str])] = &[
        (
            &["impact", "d", "scene:x"],
            "impact",
            "--project",
            &["impact --project d", "--node scene:x"],
        ),
        (
            &["constraints", "d", "--run"],
            "constraints",
            "--run",
            &["--project d", "--all-scripts"],
        ),
        (
            &["constraints", "d"],
            "constraints",
            "--project",
            &["constraints --project d"],
        ),
        (
            &["context", "f.lute", "--target", "x"],
            "context",
            "--target",
            &["--node x"],
        ),
        (
            &["context", "f.lute", "--run", "s.play.yaml"],
            "context",
            "--run",
            &["--script s.play.yaml"],
        ),
        (
            &["scenario", "d", "reach", "foo"],
            "scenario reach",
            "--project",
            &["scenario reach --project d", "--node foo"],
        ),
        (
            &["scenario", "d", "envelope", "foo"],
            "scenario envelope",
            "--project",
            &["scenario envelope --project d", "--node foo"],
        ),
        (
            &["scenario", "d", "knowledge", "--for", "foo"],
            "scenario knowledge",
            "--for",
            &["--node foo"],
        ),
        (
            &["scenario", "d", "--format", "json"],
            "scenario",
            "--format",
            &["scenario --project d", "--json"],
        ),
        (
            &["play", "d", "--script", "s", "--explain", "a(b)"],
            "play",
            "--explain",
            &["--derivation 'a(b)'", "--project d"],
        ),
        (
            &["play", "d", "--script", "s", "--dump-conditions", "f"],
            "play",
            "--dump-conditions",
            &["--conditions-output f"],
        ),
        (
            &["run", "ir.json", "--dump-conditions", "f"],
            "run",
            "--dump-conditions",
            &["run --conditions-output f"],
        ),
        (
            &["trace", "f.lute", "--choose", "a=b"],
            "trace",
            "--choose",
            &["--choice a=b"],
        ),
        (
            &["trace", "f.lute", "--accept", "q"],
            "trace",
            "--accept",
            &["--accepted-quest q"],
        ),
        (
            &["calendar", "d", "--facts", "r"],
            "calendar",
            "--facts",
            &["--relation r", "--project d"],
        ),
        (
            &["calendar", "d", "--csv"],
            "calendar",
            "--csv",
            &["--format csv"],
        ),
        (
            &["calendar", "d", "--occasion", "dayStart@run.day"],
            "calendar",
            "--occasion",
            &["--occasion-axes dayStart=run.day"],
        ),
        (
            &["loc", "export", "d", "--format", "json"],
            "loc export",
            "--format",
            &["loc export --project d"],
        ),
        (
            &["loc", "export", "d", "--out", "x"],
            "loc export",
            "--out",
            &["--output x"],
        ),
        (
            &["loc", "import", "a.json", "--out", "x"],
            "loc import",
            "--out",
            &["--output x"],
        ),
        (
            &["patch", "d", "p.json"],
            "patch",
            "--project",
            &["patch p.json --project d"],
        ),
        (
            &["catalog", "refresh", "d"],
            "catalog refresh",
            "--providers",
            &["--providers d"],
        ),
        (
            &["new", "scene", "x", "--dir", "d"],
            "new scene",
            "--dir",
            &["--project d"],
        ),
        (
            &["doctor", "d"],
            "doctor",
            "--project",
            &["doctor --project d"],
        ),
        (
            &["beats", "d"],
            "beats",
            "--project",
            &["beats --project d"],
        ),
        (&["lore", "d"], "lore", "--project", &["lore --project d"]),
        (&["refs", "d"], "refs", "--project", &["refs --project d"]),
        (
            &["loc", "report", "d"],
            "loc report",
            "--project",
            &["loc report --project d"],
        ),
        (
            &["play", "d", "--script", "s"],
            "play",
            "--project",
            &["play --project d"],
        ),
    ];
    let dir = lute_scratch::ScratchDir::named("removed-r2");
    for (args, command, argument, words) in rows {
        let mut with_json = args.to_vec();
        with_json.push("--json");
        let out = lute_in(&dir, &with_json);
        assert_eq!(out.status.code(), Some(2), "{args:?}: {}", all(&out));
        let v = json(&out);
        assert_eq!(v["command"], *command, "{args:?}: {v}");
        assert_eq!(v["error"]["code"], "E-CLI-USAGE", "{args:?}");
        assert_eq!(v["error"]["argument"], *argument, "{args:?}: {v}");
        let message = v["error"]["message"].as_str().unwrap();
        let hint = v["error"]["hint"].as_str().unwrap();
        for word in *words {
            assert!(
                hint.contains(word),
                "{args:?}: hint `{hint}` lacks `{word}`"
            );
            assert!(
                message.contains(word),
                "{args:?}: message `{message}` lacks `{word}`"
            );
        }
        assert!(message.contains(hint), "{args:?}: {message}");
        // The text form says it on stderr, exit 2.
        let text = lute_in(&dir, args);
        assert_eq!(text.status.code(), Some(2), "{args:?}");
        assert!(
            stderr(&text).contains(words[0]),
            "{args:?}: {}",
            stderr(&text)
        );
    }
}

#[test]
fn removed_spellings_rewrite_to_the_whole_runnable_line() {
    let dir = lute_scratch::ScratchDir::named("removed-lines");
    for (args, line) in [
        (
            vec!["new", "scene", "foo", "--dir", "/tmp/nx"],
            "lute new scene foo --project /tmp/nx",
        ),
        (
            vec!["new", "--dir", "d", "scene", "foo", "--occasion", "talk"],
            "lute new scene foo --project d --occasion talk",
        ),
        (
            vec!["scenario", "reach", "marina.s01"],
            "lute scenario reach --node marina.s01",
        ),
        (
            vec!["scenario", "d", "envelope", "quest:x"],
            "lute scenario envelope --project d --node quest:x",
        ),
        (
            vec!["scenario", "--facts", "."],
            "lute scenario --project . --facts",
        ),
        (
            vec!["scenario", "--facts", ".", "reach", "foo"],
            "lute scenario reach --project . --facts --node foo",
        ),
        (vec!["impact", "scene:x"], "lute impact --node scene:x"),
        (
            vec!["impact", "d", "scene:x", "--json"],
            "lute impact --project d --node scene:x --json",
        ),
    ] {
        let out = lute_in(&dir, &args);
        assert_eq!(out.status.code(), Some(2), "{args:?}: {}", all(&out));
        assert!(
            stderr(&out).contains(&format!("use `{line}`")),
            "{args:?}: {}",
            stderr(&out)
        );
    }
    // `--endings` takes its occasion only with `=`: no removed spelling.
    let out = lute_in(&dir, &["scenario", "reach", "--endings=occ"]);
    assert!(
        !stderr(&out).contains("use `lute scenario"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn removed_format_json_is_an_error_envelope_although_it_no_longer_selects_json() {
    let dir = lute_scratch::ScratchDir::named("removed-format-json");
    let out = lute_in(&dir, &["scenario", "d", "--format", "json"]);
    assert_usage(&out, "--format", &["`--format json` is now `--json`"]);
    let out = lute_in(&dir, &["loc", "export", "--format", "json"]);
    assert_usage(&out, "--format", &["JSON is the default"]);
}

#[test]
fn the_hidden_on_refusals_are_plain_unknown_arguments() {
    let dir = lute_scratch::ScratchDir::named("removed-on");
    write(&dir, "lute.project.yaml", CORE_PROJECT);
    write(
        &dir,
        "a.lute",
        "---\nkind: scene\nid: a\n---\n## A\n@narrator: hi\n",
    );
    for args in [
        vec!["trace", "a.lute", "--on", "x"],
        vec!["beats", "--on", "x"],
        vec!["calendar", "--on", "x"],
        vec!["new", "scene", "n", "--on", "x"],
        vec!["trace", "a.lute", "--target", "x"],
    ] {
        let out = lute_in(&dir, &args);
        assert_eq!(out.status.code(), Some(2), "{args:?}: {}", all(&out));
        assert!(
            stderr(&out).contains("unexpected argument"),
            "{args:?}: {}",
            stderr(&out)
        );
        assert!(!stderr(&out).contains("`--on` was removed"), "{args:?}");
    }
}

// --- none found (§2.1) ------------------------------------------------------

#[test]
fn a_directory_command_without_a_project_is_e_cli_usage_naming_project() {
    let dir = lute_scratch::ScratchDir::named("none-found");
    let out = lute_in(&dir, &["impact", "--node", "scene:x", "--json"]);
    assert_usage(
        &out,
        "--project",
        &[
            "lute impact: no lute.project.yaml at or above",
            "pass the project directory with --project <DIR>",
        ],
    );
    // `loc export` is an artifact writer: it has no `--json`.
    let out = lute_in(&dir, &["loc", "export"]);
    assert_eq!(out.status.code(), Some(2), "{}", all(&out));
    assert!(
        stderr(&out).contains("lute loc export: no lute.project.yaml at or above"),
        "{}",
        stderr(&out)
    );
    for args in [
        vec!["constraints"],
        vec!["beats"],
        vec!["calendar"],
        vec!["lore"],
        vec!["refs"],
        vec!["loc", "report"],
        vec!["scenario"],
        vec!["expr", "true"],
        vec!["test"],
        vec!["play", "--script", "x.play.yaml"],
    ] {
        let mut with_json = args.clone();
        with_json.push("--json");
        let out = lute_in(&dir, &with_json);
        assert_eq!(out.status.code(), Some(2), "{args:?}: {}", all(&out));
        let v = json(&out);
        assert_eq!(v["error"]["code"], "E-CLI-USAGE", "{args:?}: {v}");
        assert_eq!(v["error"]["argument"], "--project", "{args:?}: {v}");
    }
}

// --- impact, constraints, context ------------------------------------------

#[test]
fn impact_takes_the_target_as_node() {
    let hub = fixture("play-hub");
    let out = lute(&[
        "impact",
        "--project",
        s(&hub),
        "--node",
        "quest:firstEscape",
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", all(&out));
    assert!(
        stdout(&out).contains("state:quest.firstEscape.state"),
        "{}",
        stdout(&out)
    );
    let out = lute(&[
        "impact",
        "--project",
        s(&hub),
        "--node",
        "quest:firstEscape",
        "--json",
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", all(&out));
    assert_eq!(json(&out)["command"], "impact");
    // Without `--node` the command has nothing to explain.
    let out = lute(&["impact", "--project", s(&hub)]);
    assert_eq!(out.status.code(), Some(2), "{}", all(&out));
}

/// A project whose constraint `c` is witnessed only by `tests/q.test.yaml`.
fn witness_project(tag: &str) -> lute_scratch::ScratchDir {
    let dir = lute_scratch::ScratchDir::named(tag);
    write(
        &dir,
        "lute.project.yaml",
        &format!("{CORE_PROJECT}constraints:\n  - id: c\n    kind: completable\n    quest: q\n"),
    );
    write(
        &dir,
        "q.lute",
        "---\nkind: quest\nid: q\nstate:\n  run.done: { type: bool, default: false }\n---\n\
         <quest id=\"q\" start=\"true\">\n  <objective id=\"o\" done=\"run.done\"/>\n</quest>\n",
    );
    write(
        &dir,
        "tests/q.test.yaml",
        "file: ../q.lute\nstate: { run.done: true }\nexpect:\n  quests: { q: complete }\n  end: complete\n",
    );
    write(
        &dir,
        "tests/z.test.yaml",
        "file: ../q.lute\nquests: { q: complete }\nexpect:\n  quests: { q: complete }\n  end: complete\n",
    );
    dir
}

#[test]
fn constraints_script_selects_the_scripts_that_run_for_witnesses() {
    let dir = witness_project("constraints-script");
    let p = s(&dir);
    let plain = lute(&["constraints", "--project", p]);
    assert!(
        stdout(&plain).contains("c: completable unknown [unknown]"),
        "{}",
        all(&plain)
    );
    for selection in [
        vec!["--all-scripts"],
        vec!["--script", "tests/q.test.yaml"],
        vec!["--script", "tests"],
    ] {
        let mut args = vec!["constraints", "--project", p];
        args.extend(&selection);
        let out = lute_in(&dir, &args);
        assert_eq!(out.status.code(), Some(0), "{selection:?}: {}", all(&out));
        assert!(
            stdout(&out).contains("c: completable holds [witnessed]"),
            "{selection:?}: {}",
            all(&out)
        );
    }
    // A script that does not witness the constraint leaves it unknown.
    let out = lute_in(
        &dir,
        &[
            "constraints",
            "--project",
            p,
            "--script",
            "tests/z.test.yaml",
        ],
    );
    assert!(
        stdout(&out).contains("c: completable unknown [unknown]"),
        "{}",
        all(&out)
    );
    // `--script` and `--all-scripts` exclude each other.
    let out = lute_in(
        &dir,
        &[
            "constraints",
            "--project",
            p,
            "--script",
            "tests",
            "--all-scripts",
        ],
    );
    assert_eq!(out.status.code(), Some(2), "{}", all(&out));
    assert!(
        stderr(&out).contains("cannot be used with"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn context_node_script_and_all_scripts() {
    let dir = witness_project("context-script");
    let q = s(&dir.join("q.lute")).to_string();
    let classes = |extra: &[&str]| -> Vec<(String, String)> {
        let mut args = vec![
            "context",
            q.as_str(),
            "--project",
            s(&dir),
            "--node",
            "quest:q",
            "--json",
        ];
        args.extend(extra);
        let out = lute_in(&dir, &args);
        assert_eq!(out.status.code(), Some(0), "{extra:?}: {}", all(&out));
        let v = json(&out);
        assert_eq!(v["schemaVersion"], "0.36.0.context", "{v}");
        let mut rows: Vec<(String, String)> = v["tests"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| {
                (
                    t["file"].as_str().unwrap().to_string(),
                    t["classification"].as_str().unwrap().to_string(),
                )
            })
            .collect();
        rows.sort();
        rows
    };
    let static_only = [
        ("tests/q.test.yaml".to_string(), "static".to_string()),
        ("tests/z.test.yaml".to_string(), "static".to_string()),
    ];
    assert_eq!(classes(&[]), static_only);
    assert_eq!(
        classes(&["--script", "tests/q.test.yaml"]),
        [
            ("tests/q.test.yaml".to_string(), "witnessed".to_string()),
            ("tests/z.test.yaml".to_string(), "static".to_string()),
        ]
    );
    let both = [
        ("tests/q.test.yaml".to_string(), "witnessed".to_string()),
        ("tests/z.test.yaml".to_string(), "witnessed".to_string()),
    ];
    assert_eq!(
        classes(&["--script", "tests"]),
        both,
        "a directory selects every script under it"
    );
    assert_eq!(classes(&["--all-scripts"]), both);
    let out = lute_in(
        &dir,
        &["context", q.as_str(), "--script", "tests", "--all-scripts"],
    );
    assert_eq!(out.status.code(), Some(2), "{}", all(&out));
    assert!(
        stderr(&out).contains("cannot be used with"),
        "{}",
        stderr(&out)
    );
}

// --- trace, run -------------------------------------------------------------

#[test]
fn trace_choice_and_accepted_quest_take_the_old_values() {
    let examples = examples();
    let out = lute(&[
        "trace",
        s(&examples.join("choice-persist.lute")),
        "--choice",
        "sofaHelp=help",
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", all(&out));
    assert!(
        stdout(&out).contains("<branch sofaHelp>") && stdout(&out).contains("-> help"),
        "{}",
        stdout(&out)
    );
    let out = lute(&[
        "trace",
        s(&examples.join("quest-rescue-halsin.lute")),
        "--accepted-quest",
        "rescueHalsin",
        "--project",
        s(&examples),
    ]);
    assert_eq!(out.status.code(), Some(1), "{}", all(&out));
    assert!(stdout(&out).contains("E-TRACE-ACCEPT"), "{}", stdout(&out));
}

#[test]
fn run_occasion_takes_a_targeted_raise_and_judges_it() {
    let dir = lute_scratch::ScratchDir::named("run-occasion");
    let version = env!("CARGO_PKG_VERSION");
    let art = write(
        &dir,
        "catch.json",
        &format!(
            r#"{{"kind":"lore","lute":"{version}","irVersion":"{version}",
 "requiredSemantics":["lute.core/1","lute.lore/1","lute.occasions.selection/1","lute.time.cadence/1"],
 "meta":{{"id":"catch"}},"state":[],
 "commands":[
  {{"kind":"beat","family":"declaration","position":"001-0100","id":"catch.land","on":"landed","target":"kind:fish",
    "targetKind":{{"kind":"fish","prefix":"fish","members":["cod"]}},"body":"001-0200"}},
  {{"kind":"line","family":"content","position":"001-0200","role":"narration","speaker":"narrator",
    "text":"You land a {{{{occasion.target}}}}.","placeholders":[{{"kind":"occasionTarget"}}]}}]}}"#
        ),
    );
    let raised = lute(&[
        "run",
        s(&art),
        "--beat",
        "catch.land",
        "--occasion",
        "landed@fish.cod",
    ]);
    assert_eq!(raised.status.code(), Some(0), "{}", all(&raised));
    assert!(
        stdout(&raised).contains("You land a cod."),
        "{}",
        stdout(&raised)
    );
    let unraised = lute(&["run", s(&art), "--beat", "catch.land"]);
    assert!(
        stdout(&unraised).contains("You land a kind:fish."),
        "{}",
        stdout(&unraised)
    );
}

// --- play -------------------------------------------------------------------

#[test]
fn play_derivation_and_conditions_output() {
    let hub = fixture("play-hub");
    let script = hub.join("plays/tour.play.yaml");
    let base = [
        "play",
        "--project",
        s(&hub),
        "--script",
        s(&script),
        "--quiet",
    ];
    let mut args = base.to_vec();
    args.extend(["--derivation", "heardOf(patroclus)"]);
    let out = lute(&args);
    assert_eq!(out.status.code(), Some(0), "{}", all(&out));
    assert!(
        stdout(&out).contains("explain heardOf(patroclus): does not hold"),
        "{}",
        stdout(&out)
    );
    let mut args = base.to_vec();
    args.extend(["--derivation", "not an atom"]);
    let out = lute(&args);
    assert_eq!(out.status.code(), Some(2), "{}", all(&out));
    assert!(stderr(&out).contains("--derivation"), "{}", stderr(&out));

    let target = lute_scratch::ScratchDir::named("play-conditions");
    let file = target.join("conditions.ndjson");
    let mut args = base.to_vec();
    args.extend(["--conditions-output", s(&file)]);
    let out = lute(&args);
    assert_eq!(out.status.code(), Some(0), "{}", all(&out));
    let lines = std::fs::read_to_string(&file).unwrap();
    assert!(lines.lines().count() > 2, "{lines}");
    assert!(
        lines.lines().next().unwrap().contains("\"kind\":\"env\""),
        "{lines}"
    );
    for refused in [
        ["--derivation", "heardOf(patroclus)"],
        ["--conditions-output", s(&file)],
    ] {
        let out = lute(&[
            "play",
            "--project",
            s(&hub),
            "--script",
            s(&script),
            "--events",
            refused[0],
            refused[1],
        ]);
        assert_eq!(out.status.code(), Some(2), "{refused:?}: {}", all(&out));
        assert!(
            stderr(&out).contains("cannot be combined with"),
            "{}",
            stderr(&out)
        );
    }
}

// --- test operands ----------------------------------------------------------

#[test]
fn test_operands_select_scripts_and_none_walks_the_project() {
    let dir = lute_scratch::ScratchDir::named("test-operands");
    write(&dir, "lute.project.yaml", CORE_PROJECT);
    write(
        &dir,
        "scenes/a.lute",
        "---\nkind: scene\nid: a\n---\n## A\n@narrator: hi\n",
    );
    write(
        &dir,
        "tests/pass.test.yaml",
        "file: ../scenes/a.lute\nexpect:\n  end: complete\n",
    );
    write(
        &dir,
        "other/fail.test.yaml",
        "file: ../scenes/a.lute\nexpect:\n  end: halted\n",
    );
    let p = s(&dir);
    let run = |extra: &[&str]| {
        let mut args = vec!["test", "--project", p, "--json"];
        args.extend(extra);
        let out = lute_in(&dir, &args);
        let v = json(&out);
        (out.status.code(), v)
    };
    let (code, v) = run(&[]);
    assert_eq!(code, Some(1), "{v}");
    assert_eq!(v["summary"]["passed"], 1, "{v}");
    assert_eq!(v["summary"]["failed"], 1, "{v}");
    let (code, v) = run(&["tests"]);
    assert_eq!(code, Some(0), "a directory operand: {v}");
    assert_eq!(v["summary"]["passed"], 1, "{v}");
    assert_eq!(v["summary"]["failed"], 0, "{v}");
    let (code, v) = run(&["tests/pass.test.yaml"]);
    assert_eq!(code, Some(0), "a file operand: {v}");
    assert_eq!(v["summary"]["passed"], 1, "{v}");
    let (code, v) = run(&["tests/pass.test.yaml", "other"]);
    assert_eq!(code, Some(1), "two operands: {v}");
    assert_eq!(v["summary"]["passed"], 1, "{v}");
    assert_eq!(v["summary"]["failed"], 1, "{v}");
}

// --- calendar ---------------------------------------------------------------

#[test]
fn calendar_occasion_axes_relation_and_csv() {
    let dir = town("calendar-spellings");
    let p = s(&dir);
    let base = [
        "calendar",
        "--project",
        p,
        "--axis",
        "run.day=1..2",
        "--axis",
        "run.slot=morning,night",
    ];
    let with = |extra: &[&str]| {
        let mut args = base.to_vec();
        args.extend(extra);
        lute(&args)
    };
    let out = with(&["--occasion-axes", "dayStart=run.day", "--json"]);
    assert_eq!(out.status.code(), Some(0), "{}", all(&out));
    let v = json(&out);
    let col = v["columns"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["occasion"] == "dayStart")
        .unwrap();
    assert_eq!(col["varies"], serde_json::json!(["run.day"]), "{v}");
    assert_eq!(
        col["heldAt"],
        serde_json::json!({"run.slot": "morning"}),
        "{v}"
    );

    let out = with(&["--occasion", "dayStart", "--relation", "present"]);
    assert_eq!(out.status.code(), Some(0), "{}", all(&out));
    assert!(
        stdout(&out).contains("facts present(person, place):"),
        "{}",
        stdout(&out)
    );

    let out = with(&["--occasion", "dayStart", "--format", "csv"]);
    assert_eq!(out.status.code(), Some(0), "{}", all(&out));
    assert!(
        stdout(&out).starts_with("run.day,run.slot,occasion,target,select,winner,"),
        "{}",
        stdout(&out)
    );
    let out = with(&["--occasion", "dayStart", "--format", "text"]);
    assert_eq!(out.status.code(), Some(0), "{}", all(&out));
    assert!(
        !stdout(&out).starts_with("run.day,run.slot,occasion"),
        "{}",
        stdout(&out)
    );
}

#[test]
fn calendar_refusals_name_the_new_options() {
    let dir = town("calendar-refusals");
    let p = s(&dir);
    let base = ["calendar", "--project", p, "--axis", "run.day=1..2"];
    let run = |extra: &[&str]| {
        let mut args = base.to_vec();
        args.extend(extra);
        lute(&args)
    };
    // `--json` and `--format` are two ways to say the output form.
    let out = run(&["--json", "--format", "csv"]);
    assert_eq!(out.status.code(), Some(2), "{}", all(&out));
    let v = json(&out);
    assert_eq!(v["error"]["code"], "E-CLI-USAGE", "{v}");
    assert!(
        v["error"]["message"].as_str().unwrap().contains("--format"),
        "{v}"
    );
    // The same occasion twice.
    for extra in [
        vec![
            "--occasion",
            "dayStart",
            "--occasion-axes",
            "dayStart=run.day",
        ],
        vec![
            "--occasion-axes",
            "dayStart=run.day",
            "--occasion-axes",
            "dayStart=run.slot",
        ],
    ] {
        let out = run(&extra);
        assert_eq!(out.status.code(), Some(2), "{extra:?}: {}", all(&out));
        assert!(
            stderr(&out).contains("dayStart") && stderr(&out).contains("--occasion-axes"),
            "{extra:?}: {}",
            stderr(&out)
        );
    }
    // An unknown occasion is named under the option that carried it.
    let out = run(&["--occasion-axes", "zz=run.day"]);
    assert_eq!(out.status.code(), Some(2), "{}", all(&out));
    assert!(
        stderr(&out).contains("`--occasion-axes zz=run.day` is not an occasion of this project"),
        "{}",
        stderr(&out)
    );
    // An empty axes item names the grammar of `--occasion-axes`, not `@`.
    let out = run(&["--occasion-axes", "dayStart=run.day,"]);
    assert_eq!(out.status.code(), Some(2), "{}", all(&out));
    assert!(
        stderr(&out).contains(
            "`--occasion-axes dayStart=run.day,`: expected <OCCASION>=<AXIS>[=<VALUE>],…, with no empty item"
        ),
        "{}",
        stderr(&out)
    );
    let out = run(&["--occasion", "zz"]);
    assert!(
        stderr(&out).contains("`--occasion zz` is not an occasion of this project"),
        "{}",
        stderr(&out)
    );
    // `--occasion O@AXES` is not an occasion name.
    let out = run(&["--occasion", "dayStart@run.day"]);
    assert_eq!(out.status.code(), Some(2), "{}", all(&out));
    assert!(
        stderr(&out).contains("--occasion-axes dayStart=run.day"),
        "{}",
        stderr(&out)
    );
}

// --- scenario, loc ----------------------------------------------------------

#[test]
fn scenario_json_is_the_old_format_json_payload() {
    let hub = fixture("play-hub");
    let out = lute(&["scenario", "--project", s(&hub), "--json"]);
    assert_eq!(out.status.code(), Some(0), "{}", all(&out));
    let v = json(&out);
    assert_eq!(v["command"], "scenario");
    assert_eq!(v["schemaVersion"], "0.43.0.scenario");
    assert_eq!(v["ok"], true);
    assert!(v["roots"][0]["layers"].is_array(), "{v}");
    for (sub, version) in [
        (
            vec!["reach", "--node", "scene:hub.welcome"],
            "0.43.0.scenario-reach",
        ),
        (
            vec!["envelope", "--node", "scene:hub.welcome"],
            "0.43.0.scenario-envelope",
        ),
        (vec!["knowledge"], "0.43.0.scenario-knowledge"),
    ] {
        let mut args = vec!["scenario", "--project", s(&hub)];
        args.extend(&sub);
        args.push("--json");
        let out = lute(&args);
        assert_eq!(
            json(&out)["schemaVersion"],
            version,
            "{sub:?}: {}",
            all(&out)
        );
    }
    // `--format` keeps `text` and `dot`; with `--json` the two conflict.
    let out = lute(&["scenario", "--project", s(&hub), "--format", "dot"]);
    assert_eq!(out.status.code(), Some(0), "{}", all(&out));
    assert!(stdout(&out).starts_with("digraph"), "{}", stdout(&out));
    let out = lute(&[
        "scenario",
        "--project",
        s(&hub),
        "--json",
        "--format",
        "dot",
    ]);
    assert_eq!(out.status.code(), Some(2), "{}", all(&out));
    assert_eq!(json(&out)["error"]["code"], "E-CLI-USAGE");
}

#[test]
fn loc_export_is_json_by_default_csv_on_request_and_refuses_format_json() {
    let hub = fixture("play-hub");
    let out = lute(&["loc", "export", "--project", s(&hub)]);
    assert_eq!(out.status.code(), Some(0), "{}", all(&out));
    let rows: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(rows.as_array().unwrap().len() > 3, "{rows}");
    let out = lute(&["loc", "export", "--project", s(&hub), "--format", "csv"]);
    assert_eq!(out.status.code(), Some(0), "{}", all(&out));
    assert!(
        stdout(&out).starts_with("kind,file,line,lineId,code,speaker,"),
        "{}",
        stdout(&out)
    );
    let out = lute(&["loc", "export", "--project", s(&hub), "--format", "json"]);
    assert_usage(&out, "--format", &["JSON is the default"]);
    // `-o, --output` writes the artifact instead of stdout.
    let target = lute_scratch::ScratchDir::named("loc-export-output");
    let file = target.join("export.json");
    let out = lute(&["loc", "export", "--project", s(&hub), "-o", s(&file)]);
    assert_eq!(out.status.code(), Some(0), "{}", all(&out));
    assert!(stdout(&out).is_empty(), "{}", stdout(&out));
    let written: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    assert_eq!(written, rows);
}

// --- new, catalog refresh ---------------------------------------------------

#[test]
fn new_from_a_project_subfolder_writes_into_the_project() {
    let dir = lute_scratch::ScratchDir::named("new-subfolder");
    write(&dir, "lute.project.yaml", CORE_PROJECT);
    write(
        &dir,
        "scenes/keep.lute",
        "---\nkind: scene\nid: keep\n---\n## K\n@narrator: hi\n",
    );
    let sub = dir.join("scenes");
    let out = lute_in(&sub, &["new", "scene", "fresh"]);
    assert_eq!(out.status.code(), Some(0), "{}", all(&out));
    assert!(dir.join("scenes/fresh.lute").is_file(), "{}", all(&out));
    assert!(!sub.join("scenes").exists());
    // An explicit `--project` names the project, not a folder inside it.
    let out = lute_in(&dir, &["new", "scene", "other", "--project", s(&sub)]);
    assert_eq!(out.status.code(), Some(2), "{}", all(&out));
    assert!(stderr(&out).contains("--project"), "{}", stderr(&out));
    let out = lute_in(&dir, &["new", "scene", "viaflag", "--project", s(&dir)]);
    assert_eq!(out.status.code(), Some(0), "{}", all(&out));
    assert!(dir.join("scenes/viaflag.lute").is_file());
}

#[test]
fn catalog_refresh_defaults_to_the_projects_catalog_dir() {
    let providers = "manifestVersion: test\nproviderVersion: \"1\"\nstale: false\nentries:\n  speakerId: [hero]\n";
    let dir = lute_scratch::ScratchDir::named("catalog-default");
    write(&dir, "lute.project.yaml", CORE_PROJECT);
    write(&dir, "catalog/speakers.yaml", providers);
    let out = lute_in(&dir, &["catalog", "refresh", "--project", s(&dir)]);
    assert_eq!(out.status.code(), Some(0), "{}", all(&out));
    let stamped = std::fs::read_to_string(dir.join("catalog/speakers.yaml")).unwrap();
    assert_ne!(
        stamped, providers,
        "the default catalog directory was refreshed"
    );

    // `--providers` names the directory; a project is not needed for it.
    let elsewhere = lute_scratch::ScratchDir::named("catalog-providers");
    write(&elsewhere, "snap/speakers.yaml", providers);
    let out = lute_in(&elsewhere, &["catalog", "refresh", "--providers", "snap"]);
    assert_eq!(out.status.code(), Some(0), "{}", all(&out));
    assert_ne!(
        std::fs::read_to_string(elsewhere.join("snap/speakers.yaml")).unwrap(),
        providers
    );

    // Neither a project nor `--providers`: nothing to refresh.
    let empty = lute_scratch::ScratchDir::named("catalog-neither");
    let out = lute_in(&empty, &["catalog", "refresh"]);
    assert_eq!(out.status.code(), Some(2), "{}", all(&out));
    assert!(stderr(&out).contains("--providers"), "{}", stderr(&out));
}

/// The witness scripts of `context --node … --script/--all-scripts` (and of
/// `constraints`, which builds them through the same `ScriptSelection`) run
/// under the command's `--providers`: the test traces the scene that names a
/// provider id, which resolves only through them.
#[test]
fn context_witness_scripts_run_under_providers() {
    let dir = provider_project("witness-providers");
    write(
        &dir,
        "scenes/a.lute",
        "---\nkind: scene\nid: a\n---\n\n## A\n\n::announce{id=\"hero\"}\n<branch id=\"b\">\n  \
         <choice id=\"x\" text=\"X\">\n    @narrator: x\n  </choice>\n  \
         <choice id=\"y\" text=\"Y\">\n    @narrator: y\n  </choice>\n</branch>\n",
    );
    write(
        &dir,
        "tests/a.test.yaml",
        "file: ../scenes/a.lute\nchoose: { b: x }\nexpect:\n  end: complete\n",
    );
    let p = s(&dir);
    let providers = s(&dir.join("providers")).to_string();
    let scene = s(&dir.join("scenes/a.lute")).to_string();
    let class = |extra: &[&str]| {
        let mut args = vec![
            "context",
            scene.as_str(),
            "--project",
            p,
            "--node",
            "choice:a:b.x",
            "--script",
            "tests",
            "--json",
        ];
        args.extend(extra);
        let out = lute_in(&dir, &args);
        let v = json(&out);
        v["tests"]
            .as_array()
            .unwrap_or_else(|| panic!("{v}"))
            .iter()
            .map(|t| t["classification"].as_str().unwrap().to_string())
            .collect::<Vec<_>>()
    };
    // Without them the traced scene's provider id is unknown: the script
    // observes nothing, so it is not even listed.
    assert!(class(&[]).is_empty());
    assert_eq!(class(&["--providers", providers.as_str()]), ["witnessed"]);
}

// --- --providers / --permission-profile take effect -------------------------

#[test]
fn providers_resolve_ids_in_play_test_trace_and_calendar() {
    let dir = provider_project("providers-effect");
    let p = s(&dir);
    let providers = s(&dir.join("providers")).to_string();
    let play = s(&dir.join("plays/p.play.yaml")).to_string();
    for (name, args) in [
        (
            "play",
            vec!["play", "--project", p, "--script", play.as_str()],
        ),
        ("test", vec!["test", "--project", p]),
        ("trace", vec!["trace", "scenes/a.lute", "--project", p]),
        ("calendar", vec!["calendar", "--project", p]),
    ] {
        let without = lute_in(&dir, &args);
        assert_eq!(
            without.status.code(),
            Some(1),
            "{name} without: {}",
            all(&without)
        );
        assert!(
            all(&without).contains("E-UNKNOWN-ID"),
            "{name}: {}",
            all(&without)
        );
        let mut with = args.clone();
        with.extend(["--providers", providers.as_str()]);
        let with = lute_in(&dir, &with);
        assert_eq!(with.status.code(), Some(0), "{name} with: {}", all(&with));
    }
}

#[test]
fn lint_providers_feed_the_asset_exists_rule() {
    let dir = lute_scratch::ScratchDir::named("lint-providers");
    write(&dir, "lute.project.yaml", CORE_PROJECT);
    write(
        &dir,
        "lute.lint.yaml",
        "rules:\n  asset-exists: { level: error, options: { providers: { bg: bg-catalog } } }\n",
    );
    write(
        &dir,
        "scenes/a.lute",
        "---\nkind: scene\nid: a\n---\n\n## S\n\n::bg{location=\"a\" assetId=\"ghost\"}\n@narrator: hi\n",
    );
    let known = |ids: &str| {
        format!("manifestVersion: test\nproviderVersion: \"1\"\nstale: false\nentries:\n  bg-catalog: [{ids}]\n")
    };
    write(&dir, "absent/bg.yaml", &known("known"));
    write(&dir, "present/bg.yaml", &known("ghost"));
    let p = s(&dir);
    let out = lute(&[
        "lint",
        "--project",
        p,
        "--providers",
        s(&dir.join("absent")),
    ]);
    assert_eq!(out.status.code(), Some(1), "{}", all(&out));
    assert!(stdout(&out).contains("L-ASSET-EXISTS"), "{}", stdout(&out));
    let out = lute(&[
        "lint",
        "--project",
        p,
        "--providers",
        s(&dir.join("present")),
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", all(&out));
}

#[test]
fn an_unknown_permission_profile_refuses_play_test_trace_expr_and_calendar() {
    let dir = provider_project("profile-effect");
    let p = s(&dir);
    let play = s(&dir.join("plays/p.play.yaml")).to_string();
    let providers = s(&dir.join("providers")).to_string();
    for (name, args) in [
        (
            "play",
            vec!["play", "--project", p, "--script", play.as_str()],
        ),
        ("test", vec!["test", "--project", p]),
        ("trace", vec!["trace", "scenes/a.lute", "--project", p]),
        ("expr", vec!["expr", "true", "--project", p]),
        ("calendar", vec!["calendar", "--project", p]),
    ] {
        let mut ok = args.clone();
        ok.extend(["--providers", providers.as_str()]);
        if name == "expr" {
            ok = args.clone();
        }
        let fine = lute_in(&dir, &ok);
        assert_eq!(
            fine.status.code(),
            Some(0),
            "{name} baseline: {}",
            all(&fine)
        );
        ok.extend(["--permission-profile", "nope"]);
        let out = lute_in(&dir, &ok);
        assert_eq!(out.status.code(), Some(1), "{name}: {}", all(&out));
        assert!(
            all(&out).contains("E-PROFILE-UNKNOWN"),
            "{name}: {}",
            all(&out)
        );
    }
}

/// A patch that rewrites the only line of `a.lute`, as `lute patch` takes it.
fn patch_request(dir: &Path, providers: Option<&Path>) -> PathBuf {
    use lute_model::{ModelOptions, ProjectModel};
    use lute_semantic::NodeKind;
    let model = ProjectModel::build_single_root(
        dir,
        &ModelOptions {
            providers: providers.map(Path::to_path_buf),
            ..ModelOptions::default()
        },
    )
    .unwrap();
    let line = model
        .graph()
        .nodes
        .keys()
        .find(|key| key.kind == NodeKind::Line)
        .unwrap()
        .canonical();
    let request = serde_json::json!({
        "base": {"project": format!("sha256:{}", model.revisions().sha256)},
        "targets": [line.clone()],
        "edits": [{"op": "replaceNode", "node": line, "text": "@narrator: bye"}],
        "preserve": []
    });
    write(dir, "patch.json", &serde_json::to_string(&request).unwrap())
}

#[test]
fn patch_threads_providers_and_the_permission_profile_into_the_model() {
    let dir = provider_project("patch-effect");
    let providers = dir.join("providers");
    let patch = patch_request(&dir, Some(&providers));
    let p = s(&dir);
    let patch = s(&patch);
    let run = |extra: &[&str]| {
        let mut args = vec!["patch", patch, "--project", p, "--dry-run", "--json"];
        args.extend(extra);
        let out = lute(&args);
        (out.status.code(), json(&out))
    };
    // The request was built over the model with the providers: only the
    // same model accepts it.
    let (_, v) = run(&["--providers", s(&providers)]);
    assert_eq!(v["accepted"], true, "{v}");
    let (code, v) = run(&[]);
    assert_eq!(code, Some(2), "{v}");
    assert_eq!(v["code"], "E-PATCH-STALE", "{v}");
    let (code, v) = run(&["--providers", s(&providers), "--permission-profile", "nope"]);
    assert_eq!(code, Some(2), "{v}");
    assert_eq!(v["accepted"], false, "{v}");
}

// --- --deny / --deny-warnings promote real warnings -------------------------

#[test]
fn deny_promotes_a_real_warning_to_exit_one_on_trace_expr_play_test_and_calendar() {
    let hub = fixture("play-hub");
    let tour = hub.join("plays/tour.play.yaml");
    let quest = hub.join("quests/first-escape.lute");
    // trace: the document's own check verdict (W-QUEST-TIER-IMPLICIT).
    let plain = lute(&["trace", s(&quest)]);
    assert_eq!(plain.status.code(), Some(0), "{}", all(&plain));
    for deny in [
        vec!["--deny-warnings"],
        vec!["--deny", "W-QUEST-TIER-IMPLICIT"],
    ] {
        let mut args = vec!["trace", s(&quest)];
        args.extend(&deny);
        let out = lute(&args);
        assert_eq!(out.status.code(), Some(1), "trace {deny:?}: {}", all(&out));
        assert!(all(&out).contains("W-QUEST-TIER-IMPLICIT"), "{}", all(&out));
    }
    // expr: a lowering diagnostic.
    let expr = ["expr", "has(quest.firstEscape.state)", "--project", s(&hub)];
    let plain = lute(&expr);
    assert_eq!(plain.status.code(), Some(0), "{}", all(&plain));
    assert!(
        stderr(&plain).contains("W-QUEST-STATE-HAS"),
        "{}",
        stderr(&plain)
    );
    let mut args = expr.to_vec();
    args.push("--deny-warnings");
    let out = lute(&args);
    assert_eq!(out.status.code(), Some(1), "expr: {}", all(&out));
    assert!(
        stdout(&out).is_empty(),
        "nothing is lowered: {}",
        stdout(&out)
    );
    let mut args = expr.to_vec();
    args.extend(["--deny", "W-QUEST-STATE-HAS"]);
    assert_eq!(lute(&args).status.code(), Some(1));
    // play and calendar: the project compile gate.
    let play = [
        "play",
        "--project",
        s(&hub),
        "--script",
        s(&tour),
        "--quiet",
    ];
    assert_eq!(lute(&play).status.code(), Some(0));
    let calendar = ["calendar", "--project", s(&hub)];
    assert_eq!(lute(&calendar).status.code(), Some(0));
    for base in [&play[..], &calendar[..]] {
        for deny in [
            vec!["--deny-warnings"],
            vec!["--deny", "W-QUEST-TIER-IMPLICIT"],
        ] {
            let mut args = base.to_vec();
            args.extend(&deny);
            let out = lute(&args);
            assert_eq!(
                out.status.code(),
                Some(1),
                "{base:?} {deny:?}: {}",
                all(&out)
            );
            assert!(all(&out).contains("[denied]"), "{}", all(&out));
        }
    }
    // test: a traced document's check verdict.
    let dir = lute_scratch::ScratchDir::named("deny-test");
    write(&dir, "lute.project.yaml", CORE_PROJECT);
    write(
        &dir,
        "quests/q.lute",
        "---\nkind: quest\nid: q\nstate:\n  run.done: { type: bool, default: false }\n---\n\
         <quest id=\"q\" start=\"true\">\n  <objective id=\"o\" done=\"run.done\"/>\n</quest>\n",
    );
    write(
        &dir,
        "tests/q.test.yaml",
        "file: ../quests/q.lute\nexpect:\n  end: complete\n",
    );
    let p = s(&dir);
    let plain = lute(&["test", "--project", p]);
    assert_eq!(plain.status.code(), Some(0), "{}", all(&plain));
    for deny in [
        vec!["--deny-warnings"],
        vec!["--deny", "W-QUEST-TIER-IMPLICIT"],
    ] {
        let mut args = vec!["test", "--project", p];
        args.extend(&deny);
        let out = lute(&args);
        assert_eq!(out.status.code(), Some(1), "test {deny:?}: {}", all(&out));
        assert!(all(&out).contains("W-QUEST-TIER-IMPLICIT"), "{}", all(&out));
    }
}
