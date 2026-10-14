//! `lute schema [COMMAND]...` (dsl 0.43.0 §5): the command tree as JSON,
//! narrowing one level per path word. Arguments come from the clap command
//! the parser uses; outputs from [`envelope::outputs`] — neither can drift
//! from the CLI.

use std::process::ExitCode;

use clap::{Arg, ArgAction, Command, CommandFactory};
use serde_json::{json, Map, Value};

use crate::cli::Cli;
use crate::envelope::{self, CliError};
use crate::output::{pretty_json, write_stdout};

pub(crate) fn run(path: &[String]) -> ExitCode {
    let mut root = Cli::command();
    root.build();
    let payload = if path.is_empty() {
        json!({ "commands": visible_subcommands(&root).map(|c| json!({
            "id": c.get_name(),
            "summary": summary(c),
            "hasSubcommands": c.has_subcommands(),
        })).collect::<Vec<_>>() })
    } else {
        let mut command = &root;
        let mut id = String::new();
        for word in path {
            let Some(next) = visible_subcommands(command).find(|c| c.get_name() == word) else {
                let unknown = if id.is_empty() {
                    word.clone()
                } else {
                    format!("{id} {word}")
                };
                return envelope::fail(
                    CliError::usage(format!(
                        "lute schema: there is no command `{unknown}`; `lute schema` lists them"
                    ))
                    .with_argument(word.clone()),
                );
            };
            if !id.is_empty() {
                id.push(' ');
            }
            id.push_str(next.get_name());
            command = next;
        }
        describe(&id, command)
    };
    let report = envelope::report(envelope::version::SCHEMA, 0, payload);
    match pretty_json(&report).map(|text| write_stdout(&(text + "\n"))) {
        Ok(Ok(())) => ExitCode::SUCCESS,
        _ => ExitCode::from(2),
    }
}

/// Subcommands an author can type: not hidden, and not clap's `help`.
fn visible_subcommands(command: &Command) -> impl Iterator<Item = &Command> {
    command
        .get_subcommands()
        .filter(|c| !c.is_hide_set() && c.get_name() != "help")
}

/// The one-sentence summary `lute --help` shows.
fn summary(command: &Command) -> String {
    command
        .get_about()
        .map(ToString::to_string)
        .unwrap_or_default()
}

fn describe(id: &str, command: &Command) -> Value {
    let arguments: Vec<&Arg> = command
        .get_arguments()
        .filter(|a| !a.is_hide_set() && !matches!(a.get_id().as_str(), "help" | "version"))
        .collect();
    let outputs: Vec<Value> = envelope::outputs(id)
        .iter()
        .map(|o| {
            json!({
                "flag": o.flag,
                "kind": o.kind.as_str(),
                "schemaVersion": o.schema_version,
            })
        })
        .collect();
    json!({
        "id": id,
        "summary": summary(command),
        "arguments": arguments.iter().filter(|a| a.is_positional()).map(|a| argument(a)).collect::<Vec<_>>(),
        "options": arguments.iter().filter(|a| !a.is_positional()).map(|a| argument(a)).collect::<Vec<_>>(),
        "subcommands": visible_subcommands(command).map(|c| json!({
            "id": format!("{id} {}", c.get_name()),
            "summary": summary(c),
        })).collect::<Vec<_>>(),
        "outputs": outputs,
    })
}

fn argument(arg: &Arg) -> Value {
    let mut map = Map::new();
    let value_names: Vec<String> = arg
        .get_value_names()
        .map(|names| names.iter().map(ToString::to_string).collect())
        .unwrap_or_default();
    let name = match (arg.is_positional(), arg.get_long(), arg.get_short()) {
        (true, _, _) => value_names
            .first()
            .cloned()
            .unwrap_or_else(|| arg.get_id().as_str().to_ascii_uppercase()),
        (false, Some(long), _) => format!("--{long}"),
        (false, None, Some(short)) => format!("-{short}"),
        (false, None, None) => arg.get_id().as_str().to_string(),
    };
    let takes_value = arg.get_action().takes_values();
    let repeatable = matches!(arg.get_action(), ArgAction::Append | ArgAction::Count)
        || arg.get_num_args().is_some_and(|n| n.max_values() > 1);
    map.insert("name".into(), Value::String(name));
    map.insert("required".into(), Value::Bool(arg.is_required_set()));
    map.insert("repeatable".into(), Value::Bool(repeatable));
    map.insert("takesValue".into(), Value::Bool(takes_value));
    if let (false, Some(short)) = (arg.is_positional(), arg.get_short()) {
        if arg.get_long().is_some() {
            map.insert("short".into(), Value::String(format!("-{short}")));
        }
    }
    if takes_value && !arg.is_positional() {
        if let Some(first) = value_names.first() {
            map.insert("valueName".into(), Value::String(first.clone()));
        }
    }
    let possible: Vec<Value> = arg
        .get_possible_values()
        .iter()
        .filter(|v| !v.is_hide_set())
        .map(|v| Value::String(v.get_name().to_string()))
        .collect();
    if takes_value && !possible.is_empty() {
        map.insert("possibleValues".into(), Value::Array(possible));
    }
    let defaults: Vec<String> = arg
        .get_default_values()
        .iter()
        .map(|v| v.to_string_lossy().into_owned())
        .collect();
    if takes_value {
        match defaults.as_slice() {
            [] => {}
            [one] => {
                map.insert("default".into(), Value::String(one.clone()));
            }
            many => {
                map.insert(
                    "default".into(),
                    Value::Array(many.iter().cloned().map(Value::String).collect()),
                );
            }
        }
    }
    if let Some(help) = arg.get_help() {
        let help = help.to_string();
        if let Some(first) = help.lines().next().filter(|l| !l.trim().is_empty()) {
            map.insert("help".into(), Value::String(first.trim().to_string()));
        }
    }
    Value::Object(map)
}
