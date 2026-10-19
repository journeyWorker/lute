//! 0.48.0 §6: the CLI grammar (`docs/design/cli-grammar.md`) enforced over
//! `lute schema`. Every command and subcommand is checked against seven
//! mechanical rules and the two checked-in files beside the grammar
//! (`cli-vocabulary.tsv`, `cli-commands.tsv`); a new command or option cannot
//! drift without a row.
//!
//! The surface comes from the built binary, or — when `LUTE_GRAMMAR_SCHEMA_DIR`
//! names a directory — from the `lute schema <command>` JSON documents dumped
//! there (one `*.json` per command, as `lute schema` prints them). The
//! `base_surface_is_rejected` test is how the rules are shown to fail on the
//! 0.47.2 surface: point the variable at a 0.47.2 dump and the test run fails
//! (the failing rules are printed with counts).
//!
//! Rules (the number is the §6 number):
//! 1. option names are lowercase kebab-case;
//! 2. one option name has one value name and one arity (takes a value, the
//!    value is optional) across commands;
//! 3. every option is in the vocabulary (with its value name) and every
//!    vocabulary row is used;
//! 4. every placeholder token of a value name is in grammar §5;
//! 5. a valued option's name is not a denylisted verb;
//! 6. each command's options equal its row of the commands file, and
//!    commands and rows pair up;
//! 7. no short alias but `-o`.
//!
//! Rule 8 is the report contract the commands file implies: a command whose
//! row says `--json` has an output selected by `--json` in the outputs table
//! (a report carries its `schemaVersion`); an exempt command has none.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn design_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/design")
}

// ---------------------------------------------------------------------------
// The surface
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct Opt {
    name: String,
    short: Option<String>,
    value_name: Option<String>,
    takes_value: bool,
    value_optional: bool,
}

#[derive(Clone, Debug)]
struct Cmd {
    id: String,
    options: Vec<Opt>,
    /// Operand value names.
    operands: Vec<String>,
    has_subcommands: bool,
    /// `(flag, kind, schemaVersion)` of the outputs table.
    outputs: Vec<(Option<String>, String, Option<String>)>,
}

impl Cmd {
    fn from_json(doc: &Value) -> Cmd {
        let text = |v: &Value| v.as_str().map(str::to_string);
        let options = doc["options"]
            .as_array()
            .map(|options| {
                options
                    .iter()
                    .map(|o| Opt {
                        name: o["name"].as_str().unwrap_or_default().to_string(),
                        short: text(&o["short"]),
                        value_name: text(&o["valueName"]),
                        takes_value: o["takesValue"].as_bool().unwrap_or(false),
                        value_optional: o["valueOptional"].as_bool().unwrap_or(false),
                    })
                    .collect()
            })
            .unwrap_or_default();
        Cmd {
            id: doc["id"].as_str().unwrap_or_default().to_string(),
            options,
            operands: doc["arguments"]
                .as_array()
                .map(|a| a.iter().filter_map(|o| text(&o["name"])).collect())
                .unwrap_or_default(),
            has_subcommands: doc["subcommands"]
                .as_array()
                .is_some_and(|subs| !subs.is_empty()),
            outputs: doc["outputs"]
                .as_array()
                .map(|outs| {
                    outs.iter()
                        .map(|o| {
                            (
                                text(&o["flag"]),
                                o["kind"].as_str().unwrap_or_default().to_string(),
                                text(&o["schemaVersion"]),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default(),
        }
    }

    fn has(&self, option: &str) -> bool {
        self.options.iter().any(|o| o.name == option)
    }

    /// A group that only groups subcommands (`new`, `loc`, `catalog`): no
    /// row of the commands file.
    fn is_pure_group(&self) -> bool {
        self.has_subcommands && self.options.is_empty() && self.operands.is_empty()
    }
}

fn lute_schema(path: &[&str]) -> Value {
    let out = Command::new(BIN)
        .arg("schema")
        .args(path)
        .output()
        .expect("run lute schema");
    assert!(out.status.success(), "lute schema {path:?} failed");
    serde_json::from_slice(&out.stdout).expect("lute schema prints JSON")
}

fn walk_binary(path: &[String], out: &mut Vec<Cmd>) {
    let words: Vec<&str> = path.iter().map(String::as_str).collect();
    let doc = lute_schema(&words);
    let cmd = Cmd::from_json(&doc);
    let subs: Vec<String> = doc["subcommands"]
        .as_array()
        .map(|subs| {
            subs.iter()
                .filter_map(|s| s["id"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    out.push(cmd);
    for sub in subs {
        let next: Vec<String> = sub.split(' ').map(str::to_string).collect();
        walk_binary(&next, out);
    }
}

fn surface_from_binary() -> Vec<Cmd> {
    let top = lute_schema(&[]);
    let mut out = Vec::new();
    for command in top["commands"].as_array().expect("commands") {
        let path = vec![command["id"].as_str().expect("id").to_string()];
        walk_binary(&path, &mut out);
    }
    out
}

fn surface_from_dir(dir: &Path) -> Vec<Cmd> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("LUTE_GRAMMAR_SCHEMA_DIR {}: {e}", dir.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    files
        .iter()
        .map(|file| {
            let text = std::fs::read_to_string(file).expect("read schema dump");
            Cmd::from_json(&serde_json::from_str(&text).expect("schema dump is JSON"))
        })
        .collect()
}

fn current_surface() -> Vec<Cmd> {
    match std::env::var_os("LUTE_GRAMMAR_SCHEMA_DIR") {
        Some(dir) => surface_from_dir(Path::new(&dir)),
        None => surface_from_binary(),
    }
}

// ---------------------------------------------------------------------------
// The checked-in files
// ---------------------------------------------------------------------------

/// Data rows of a TSV: `#` comment lines, blank lines and the header row
/// (the first non-comment line) are skipped.
fn tsv_rows(path: &Path) -> Vec<Vec<String>> {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    text.lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .skip(1)
        .map(|l| l.split('\t').map(str::to_string).collect())
        .collect()
}

/// `option -> (value name or "-", concept)`; malformed rows are returned.
fn vocabulary(path: &Path) -> (BTreeMap<String, (String, String)>, Vec<String>) {
    let mut rows = BTreeMap::new();
    let mut malformed = Vec::new();
    for row in tsv_rows(path) {
        match row.as_slice() {
            [option, value, concept] if !concept.trim().is_empty() => {
                if rows
                    .insert(option.clone(), (value.clone(), concept.clone()))
                    .is_some()
                {
                    malformed.push(format!("duplicate vocabulary row `{option}`"));
                }
            }
            other => malformed.push(format!("malformed vocabulary row {other:?}")),
        }
    }
    (rows, malformed)
}

/// `command id -> [project, providers, permission-profile, json, gate]`.
fn command_rows(path: &Path) -> (BTreeMap<String, [String; 5]>, Vec<String>) {
    let mut rows = BTreeMap::new();
    let mut malformed = Vec::new();
    for row in tsv_rows(path) {
        match row.as_slice() {
            [id, a, b, c, d, e] => {
                if rows
                    .insert(
                        id.clone(),
                        [a.clone(), b.clone(), c.clone(), d.clone(), e.clone()],
                    )
                    .is_some()
                {
                    malformed.push(format!("duplicate command row `{id}`"));
                }
            }
            other => malformed.push(format!("malformed command row {other:?}")),
        }
    }
    (rows, malformed)
}

/// The uppercase placeholder words grammar §5 allows: every `<WORD>` between
/// the `## 5.` and `## 6.` headings, so the list lives in one place.
fn grammar_placeholders() -> BTreeSet<String> {
    let text = std::fs::read_to_string(design_dir().join("cli-grammar.md")).expect("grammar");
    let start = text.find("\n## 5. ").expect("grammar §5");
    let section = &text[start + 1..];
    let end = section[3..].find("\n## ").map_or(section.len(), |i| i + 3);
    let section = &section[..end];
    let mut words = BTreeSet::new();
    let mut rest = section;
    while let Some(open) = rest.find('<') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('>') else { break };
        let word = &after[..close];
        if !word.is_empty() && word.bytes().all(|b| b.is_ascii_uppercase()) {
            words.insert(word.to_string());
        }
        rest = &after[close + 1..];
    }
    words
}

// ---------------------------------------------------------------------------
// The rules
// ---------------------------------------------------------------------------

const DENYLIST: &[&str] = &["choose", "accept", "run", "explain", "out"];

fn is_kebab(name: &str) -> bool {
    let Some(body) = name.strip_prefix("--") else {
        return false;
    };
    !body.is_empty()
        && body.split('-').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        })
        && body.as_bytes()[0].is_ascii_lowercase()
}

/// The uppercase words of a value name (`FILE:LINE:COLUMN` → FILE, LINE,
/// COLUMN; `EXECUTION-IR` → EXECUTION, IR).
fn tokens(value_name: &str) -> Vec<String> {
    value_name
        .split(|c: char| !c.is_ascii_uppercase())
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

/// `(rule, message)` for every violation on `surface`.
fn violations(
    surface: &[Cmd],
    vocab: &BTreeMap<String, (String, String)>,
    vocab_problems: &[String],
    rows: &BTreeMap<String, [String; 5]>,
    row_problems: &[String],
    placeholders: &BTreeSet<String>,
) -> Vec<(u8, String)> {
    let mut out: Vec<(u8, String)> = Vec::new();
    let mut value_names: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    let mut arities: BTreeMap<&str, BTreeSet<(bool, bool)>> = BTreeMap::new();
    let mut used: BTreeSet<&str> = BTreeSet::new();

    for cmd in surface {
        for opt in &cmd.options {
            let name = opt.name.as_str();
            // 7: short aliases.
            let single_dash = name.starts_with('-') && !name.starts_with("--");
            if single_dash && name != "-o" {
                out.push((7, format!("{}: short option `{name}`", cmd.id)));
            }
            if let Some(short) = &opt.short {
                if short != "-o" {
                    out.push((7, format!("{}: `{name}` has short alias `{short}`", cmd.id)));
                }
            }
            if single_dash {
                continue;
            }
            // 1: kebab-case.
            if !is_kebab(name) {
                out.push((1, format!("{}: option `{name}` is not kebab-case", cmd.id)));
            }
            used.insert(name);
            // 2: one value name and one arity per option name.
            value_names
                .entry(name)
                .or_default()
                .insert(opt.value_name.clone().unwrap_or_else(|| "-".into()));
            arities
                .entry(name)
                .or_default()
                .insert((opt.takes_value, opt.takes_value && opt.value_optional));
            // 4: placeholders.
            if let Some(value) = &opt.value_name {
                for token in tokens(value) {
                    if !placeholders.contains(&token) {
                        out.push((
                            4,
                            format!(
                                "{}: `{name}` value name `{value}` uses `{token}` outside grammar §5",
                                cmd.id
                            ),
                        ));
                    }
                }
            }
            // 5: denylisted verbs on valued options.
            if opt.takes_value {
                let bare = &name[2..];
                if DENYLIST.contains(&bare) || bare.starts_with("dump-") {
                    out.push((5, format!("{}: valued option `{name}` is a verb", cmd.id)));
                }
            }
        }
        for operand in &cmd.operands {
            for token in tokens(operand) {
                if !placeholders.contains(&token) {
                    out.push((
                        4,
                        format!(
                            "{}: operand `{operand}` uses `{token}` outside grammar §5",
                            cmd.id
                        ),
                    ));
                }
            }
        }
    }

    for (name, names) in &value_names {
        if names.len() > 1 {
            out.push((2, format!("`{name}` has several value names: {names:?}")));
        }
    }
    for (name, set) in &arities {
        if set.len() > 1 {
            out.push((2, format!("`{name}` has several arities: {set:?}")));
        }
    }

    // 3: the vocabulary.
    for problem in vocab_problems {
        out.push((3, problem.clone()));
    }
    for name in &used {
        match vocab.get(*name) {
            None => out.push((3, format!("`{name}` is not in cli-vocabulary.tsv"))),
            Some((value, _)) => {
                let actual: BTreeSet<&String> = value_names[name].iter().collect();
                if !actual.iter().any(|a| *a == value) {
                    out.push((
                        3,
                        format!("`{name}`: vocabulary says `{value}`, the surface says {actual:?}"),
                    ));
                }
            }
        }
    }
    for name in vocab.keys() {
        if !used.contains(name.as_str()) {
            out.push((3, format!("vocabulary row `{name}` is unused")));
        }
    }

    // 6 and 8: the commands file.
    for problem in row_problems {
        out.push((6, problem.clone()));
    }
    let yes = |b: bool| if b { "yes" } else { "no" };
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for cmd in surface.iter().filter(|c| !c.is_pure_group()) {
        seen.insert(cmd.id.as_str());
        let Some(row) = rows.get(&cmd.id) else {
            out.push((
                6,
                format!("command `{}` has no row in cli-commands.tsv", cmd.id),
            ));
            continue;
        };
        let json = if cmd.has("--json") { "yes" } else { "exempt" };
        let gate = cmd.has("--deny") && cmd.has("--deny-warnings");
        let partial = cmd.has("--deny") != cmd.has("--deny-warnings");
        let actual = [
            yes(cmd.has("--project")),
            yes(cmd.has("--providers")),
            yes(cmd.has("--permission-profile")),
            json,
            yes(gate),
        ];
        for ((column, want), got) in [
            "--project",
            "--providers",
            "--permission-profile",
            "--json",
            "gate",
        ]
        .iter()
        .zip(row.iter())
        .zip(actual)
        {
            if want != got {
                out.push((
                    6,
                    format!(
                        "{}: {column} is `{got}` on the surface, `{want}` in cli-commands.tsv",
                        cmd.id
                    ),
                ));
            }
        }
        if partial {
            out.push((
                6,
                format!("{}: only one of --deny / --deny-warnings", cmd.id),
            ));
        }
        // 8: the outputs table agrees with `--json`.
        let json_outputs: Vec<_> = cmd
            .outputs
            .iter()
            .filter(|(flag, _, _)| flag.as_deref().is_some_and(|f| f.starts_with("--json")))
            .collect();
        if cmd.has("--json") && json_outputs.is_empty() {
            out.push((
                8,
                format!("{}: --json has no entry in the outputs table", cmd.id),
            ));
        }
        if !cmd.has("--json") && !json_outputs.is_empty() {
            out.push((
                8,
                format!("{}: outputs name --json, which it does not take", cmd.id),
            ));
        }
        for (flag, kind, version) in &cmd.outputs {
            if kind == "report" && version.is_none() {
                out.push((
                    8,
                    format!("{}: report output {flag:?} has no schemaVersion", cmd.id),
                ));
            }
            if kind != "report" && version.is_some() {
                out.push((
                    8,
                    format!("{}: {kind} output {flag:?} has a schemaVersion", cmd.id),
                ));
            }
        }
    }
    for id in rows.keys() {
        if !seen.contains(id.as_str()) {
            out.push((
                6,
                format!("row `{id}` of cli-commands.tsv matches no command"),
            ));
        }
    }
    out
}

fn check(surface: &[Cmd]) -> Vec<(u8, String)> {
    let (vocab, vocab_problems) = vocabulary(&design_dir().join("cli-vocabulary.tsv"));
    let (rows, row_problems) = command_rows(&design_dir().join("cli-commands.tsv"));
    violations(
        surface,
        &vocab,
        &vocab_problems,
        &rows,
        &row_problems,
        &grammar_placeholders(),
    )
}

fn counts(found: &[(u8, String)]) -> BTreeMap<u8, usize> {
    let mut counts = BTreeMap::new();
    for (rule, _) in found {
        *counts.entry(*rule).or_insert(0) += 1;
    }
    counts
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// The gate: the surface obeys every rule. With `LUTE_GRAMMAR_SCHEMA_DIR` set
/// it checks that dump instead (a 0.47.2 dump fails it).
#[test]
fn the_cli_surface_obeys_the_grammar() {
    let found = check(&current_surface());
    assert!(
        found.is_empty(),
        "grammar violations by rule {:?}:\n{}",
        counts(&found),
        found
            .iter()
            .map(|(rule, message)| format!("  rule {rule}: {message}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn the_binary_lists_every_command_the_files_describe() {
    let surface = surface_from_binary();
    let ids: BTreeSet<&str> = surface.iter().map(|c| c.id.as_str()).collect();
    // The 34 leaf commands of 0.48 (0.47.2's 34, minus `check-project` and
    // `compile-stream`, plus the four `new` kinds in place of one) and the
    // groups.
    for id in [
        "check",
        "compile",
        "tag",
        "fix",
        "fmt",
        "init",
        "new scene",
        "new quest",
        "new lore",
        "new schema",
        "loc import",
        "catalog refresh",
        "scenario reach",
        "schema",
    ] {
        assert!(ids.contains(id), "lute schema lists `{id}`");
    }
    assert!(!ids.contains("check-project") && !ids.contains("compile-stream"));
}

/// The vocabulary is exactly the options of the surface — no stale rows.
#[test]
fn the_vocabulary_rows_are_the_surface_options() {
    let (vocab, problems) = vocabulary(&design_dir().join("cli-vocabulary.tsv"));
    assert!(problems.is_empty(), "{problems:?}");
    let surface = surface_from_binary();
    let used: BTreeSet<String> = surface
        .iter()
        .flat_map(|c| c.options.iter().map(|o| o.name.clone()))
        .collect();
    assert_eq!(vocab.keys().cloned().collect::<BTreeSet<_>>(), used);
}

/// §5 holds the words the surface uses, parsed from the grammar document.
#[test]
fn the_placeholder_vocabulary_is_parsed_from_the_grammar() {
    let words = grammar_placeholders();
    for word in [
        "DIR",
        "FILE",
        "PATH",
        "NODE",
        "OCCASION",
        "TARGET",
        "CODE",
        "NAME",
        "EXPR",
        "FORMAT",
        "N",
        "ATOM",
        "ID",
        "STEP",
        "KIND",
        "RELATION",
        "QUEST",
        "COMMAND",
        "BEFORE",
        "AFTER",
        "AXES",
        "AXIS",
        "VALUES",
        "DIRECTIVE",
        "ATTR",
        "BRANCH",
        "CHOICE",
        "LITERAL",
        "LINE",
        "COLUMN",
    ] {
        assert!(words.contains(word), "§5 lists <{word}>");
    }
    for derived in [
        "MAX_ITEMS",
        "NODE_ID",
        "PATHS",
        "NODEKEY",
        "CHOICEID",
        "QUESTID",
    ] {
        assert!(!words.contains(derived), "{derived} is not a placeholder");
    }
}

// --- each rule trips on a mutated copy of the real surface -----------------

fn mutated(edit: impl FnOnce(&mut Vec<Cmd>)) -> BTreeMap<u8, usize> {
    let mut surface = surface_from_binary();
    edit(&mut surface);
    counts(&check(&surface))
}

fn option(cmd: &mut Cmd, name: &str) -> usize {
    cmd.options
        .iter()
        .position(|o| o.name == name)
        .unwrap_or_else(|| panic!("{} has {name}", cmd.id))
}

fn command<'a>(surface: &'a mut [Cmd], id: &str) -> &'a mut Cmd {
    surface.iter_mut().find(|c| c.id == id).expect("command")
}

#[test]
fn rule_1_catches_an_option_that_is_not_kebab_case() {
    let found = mutated(|s| {
        let cmd = command(s, "check");
        let at = option(cmd, "--wip");
        cmd.options[at].name = "--Wip_mode".into();
    });
    assert!(found.contains_key(&1), "{found:?}");
}

#[test]
fn rule_2_catches_two_value_names_and_two_arities() {
    let found = mutated(|s| {
        let cmd = command(s, "check");
        let at = option(cmd, "--providers");
        cmd.options[at].value_name = Some("PATH".into());
    });
    assert!(found.contains_key(&2), "{found:?}");
    let found = mutated(|s| {
        let cmd = command(s, "check");
        let at = option(cmd, "--project");
        cmd.options[at].value_optional = true;
    });
    assert!(found.contains_key(&2), "{found:?}");
}

#[test]
fn rule_3_catches_a_missing_and_an_unused_vocabulary_row() {
    let found = mutated(|s| {
        let cmd = command(s, "check");
        cmd.options.push(Opt {
            name: "--brand-new".into(),
            short: None,
            value_name: None,
            takes_value: false,
            value_optional: false,
        });
    });
    assert!(found.contains_key(&3), "{found:?}");
    // Dropping the only commands that take `--wip` leaves its row unused.
    let found = mutated(|s| {
        let cmd = command(s, "check");
        let at = option(cmd, "--wip");
        cmd.options.remove(at);
    });
    assert!(found.contains_key(&3), "{found:?}");
}

#[test]
fn rule_4_catches_a_placeholder_outside_the_grammar() {
    let found = mutated(|s| {
        let cmd = command(s, "check");
        let at = option(cmd, "--engine");
        cmd.options[at].value_name = Some("MAX_ITEMS".into());
    });
    assert!(found.contains_key(&4), "{found:?}");
    let found = mutated(|s| command(s, "check").operands = vec!["PATHS".into()]);
    assert!(found.contains_key(&4), "{found:?}");
}

#[test]
fn rule_5_catches_a_verb_as_a_valued_option() {
    let found = mutated(|s| {
        let cmd = command(s, "trace");
        let at = option(cmd, "--choice");
        cmd.options[at].name = "--choose".into();
    });
    assert!(found.contains_key(&5), "{found:?}");
    let found = mutated(|s| {
        let cmd = command(s, "play");
        let at = option(cmd, "--conditions-output");
        cmd.options[at].name = "--dump-conditions".into();
    });
    assert!(found.contains_key(&5), "{found:?}");
}

#[test]
fn rule_6_catches_a_command_that_differs_from_its_row() {
    let found = mutated(|s| {
        let cmd = command(s, "beats");
        cmd.options.retain(|o| o.name != "--project");
    });
    assert!(found.contains_key(&6), "{found:?}");
    let found = mutated(|s| {
        let cmd = command(s, "fmt");
        cmd.options.retain(|o| o.name != "--json");
    });
    assert!(found.contains_key(&6), "{found:?}");
    let found = mutated(|s| {
        s.retain(|c| c.id != "loc import");
    });
    assert!(found.contains_key(&6), "a row without a command: {found:?}");
    let found = mutated(|s| {
        let mut extra = command(s, "version").clone();
        extra.id = "extra".into();
        s.push(extra);
    });
    assert!(found.contains_key(&6), "a command without a row: {found:?}");
}

#[test]
fn rule_7_catches_a_short_alias() {
    let found = mutated(|s| {
        let cmd = command(s, "check");
        let at = option(cmd, "--wip");
        cmd.options[at].short = Some("-w".into());
    });
    assert!(found.contains_key(&7), "{found:?}");
}

#[test]
fn rule_8_catches_a_json_command_without_a_report_entry() {
    let found = mutated(|s| command(s, "tag").outputs.clear());
    assert!(found.contains_key(&8), "{found:?}");
    let found = mutated(|s| {
        let cmd = command(s, "fix");
        cmd.outputs[0].2 = None;
    });
    assert!(found.contains_key(&8), "{found:?}");
}

/// Every `--json` command's outputs table has a report entry carrying a
/// `0.NN.M.<name>` version (the artifact writers `run` and `version` print
/// documents of their own).
#[test]
fn every_json_command_has_a_versioned_report_or_artifact_entry() {
    for cmd in surface_from_binary() {
        if !cmd.has("--json") {
            continue;
        }
        let selected: Vec<_> = cmd
            .outputs
            .iter()
            .filter(|(flag, _, _)| flag.as_deref().is_some_and(|f| f.starts_with("--json")))
            .collect();
        assert!(!selected.is_empty(), "{}", cmd.id);
        if !matches!(cmd.id.as_str(), "run" | "version") {
            assert!(
                selected.iter().any(|(_, kind, version)| kind == "report"
                    && version.as_deref().is_some_and(|v| {
                        let mut parts = v.splitn(4, '.');
                        parts.next() == Some("0")
                            && parts.next().is_some_and(|m| m.parse::<u32>().is_ok())
                            && parts.next().is_some_and(|p| p.parse::<u32>().is_ok())
                            && parts.next().is_some_and(|n| !n.is_empty())
                    })),
                "{}: no versioned report entry",
                cmd.id
            );
        }
    }
}
