//! Removed spellings (0.48.0 §5): a command line that uses a spelling 0.48
//! removed is `E-CLI-USAGE` naming the replacement and, as far as the rest of
//! the line allows, the full rewritten command. Read from the raw arguments
//! before clap parses them, so a removed command (`check-project`,
//! `compile-stream`) is not a command at all: its error envelope has
//! `command: null` and telemetry records no command id for it.

use std::ffi::OsString;

use crate::envelope::CliError;

/// The usage error for the first removed spelling in `args` (without the
/// program name), if any.
pub(crate) fn check(args: &[OsString]) -> Option<CliError> {
    let words: Vec<String> = args
        .iter()
        .map(|a| a.to_string_lossy().into_owned())
        .take_while(|a| a != "--")
        .collect();
    let at = words.iter().position(|w| !w.starts_with('-'))?;
    let rest = &words[at + 1..];
    match words[at].as_str() {
        "check-project" => Some(check_project(rest)),
        "compile-stream" => Some(compile_stream(rest)),
        "compile" => compile(rest),
        "catalog" | "loc" | "scenario" | "new" => {
            let sub = rest.first().filter(|w| !w.starts_with('-')).cloned();
            match (words[at].as_str(), sub.as_deref()) {
                ("catalog", Some("refresh")) => rewrite(&["catalog", "refresh"], &rest[1..]),
                ("loc", Some("export" | "import" | "report")) => {
                    let sub = sub.unwrap_or_default();
                    rewrite(&["loc", sub.as_str()], &rest[1..])
                }
                ("scenario", _) => rewrite(&["scenario"], rest),
                ("new", Some(_)) => rewrite(&["new"], rest),
                _ => None,
            }
        }
        "impact" | "constraints" | "context" | "patch" | "doctor" | "play" | "run" | "trace"
        | "beats" | "calendar" | "lore" | "refs" => rewrite(&[words[at].as_str()], rest),
        _ => None,
    }
}

/// Whether `args` carry the removed `--format json` (before any `--`): its
/// usage error is an envelope although the line no longer selects JSON mode
/// (0.48.0 §2.3), as the author who typed it meant a machine-readable run.
pub(crate) fn names_format_json(args: &[OsString]) -> bool {
    let words: Vec<String> = args
        .iter()
        .map(|a| a.to_string_lossy().into_owned())
        .take_while(|a| a != "--")
        .collect();
    words.iter().enumerate().any(|(i, w)| {
        w == "--format=json" || (w == "--format" && words.get(i + 1).is_some_and(|v| v == "json"))
    })
}

/// `lute <words…>` as one shell-quotable command line.
fn line(words: &[String]) -> String {
    let mut out = String::from("lute");
    for word in words {
        out.push(' ');
        if matches!(word.as_str(), "<DIR>" | "<FILE>") {
            // A placeholder the author fills in, not a word to quote.
            out.push_str(word);
        } else {
            out.push_str(&crate::shell_word(word));
        }
    }
    out
}

/// Split `rest` into the options kept verbatim (valued ones with their
/// value) and the operands, in order.
fn split(rest: &[String], valued: &[&str]) -> (Vec<String>, Vec<String>) {
    let mut options = Vec::new();
    let mut operands = Vec::new();
    let mut iter = rest.iter();
    while let Some(word) = iter.next() {
        if valued.contains(&word.as_str()) {
            options.push(word.clone());
            if let Some(value) = iter.next() {
                options.push(value.clone());
            }
        } else if word.starts_with('-') {
            options.push(word.clone());
        } else {
            operands.push(word.clone());
        }
    }
    (options, operands)
}

fn check_project(rest: &[String]) -> CliError {
    let (options, operands) = split(rest, &["--providers", "--deny", "--engine"]);
    let mut words = vec!["check".to_string(), "--project".to_string()];
    words.push(
        operands
            .first()
            .cloned()
            .unwrap_or_else(|| "<DIR>".to_string()),
    );
    words.extend(options);
    let rewritten = line(&words);
    CliError::usage(format!("lute check-project is now `{rewritten}`"))
        .with_argument("check-project")
        .with_hint(rewritten)
}

fn compile_stream(rest: &[String]) -> CliError {
    let (options, operands) = split(rest, &["--providers", "--project", "--permission-profile"]);
    let mut words = vec!["compile".to_string(), "--stream".to_string()];
    words.push(
        operands
            .first()
            .cloned()
            .unwrap_or_else(|| "<FILE>".to_string()),
    );
    // `--stream` is not a JSON command (it is a conflict), so `--json` is
    // not carried over.
    words.extend(options.into_iter().filter(|o| o != "--json"));
    let rewritten = line(&words);
    CliError::usage(format!("lute compile-stream is now `{rewritten}`"))
        .with_argument("compile-stream")
        .with_hint(rewritten)
}

/// `compile --all`, `--out`, `--locales`.
fn compile(rest: &[String]) -> Option<CliError> {
    let valued = [
        "--providers",
        "--project",
        "--permission-profile",
        "-o",
        "--output",
        "--out",
        "--locale-bundle",
        "--locales",
        "--deny",
    ];
    let removed = |word: &str| {
        ["--all", "--out", "--locales"]
            .into_iter()
            .find(|r| word == *r || word.strip_prefix(r).is_some_and(|t| t.starts_with('=')))
    };
    let first = rest.iter().find_map(|w| removed(w))?;
    let has_all = rest.iter().any(|w| w == "--all");
    let has_project = rest
        .iter()
        .any(|w| w == "--project" || w.starts_with("--project="));
    let mut words = vec!["compile".to_string()];
    let mut iter = rest.iter();
    while let Some(word) = iter.next() {
        match removed(word) {
            Some("--all") => {}
            Some(old) => {
                let new = if old == "--out" {
                    "--output"
                } else {
                    "--locale-bundle"
                };
                match word.split_once('=') {
                    Some((_, value)) => words.push(format!("{new}={value}")),
                    None => {
                        words.push(new.to_string());
                        if let Some(value) = iter.next() {
                            words.push(value.clone());
                        }
                    }
                }
            }
            None if valued.contains(&word.as_str()) => {
                words.push(word.clone());
                if let Some(value) = iter.next() {
                    words.push(value.clone());
                }
            }
            None if !word.starts_with('-') && has_all && !has_project => {
                words.push("--project".to_string());
                words.push(word.clone());
            }
            None => words.push(word.clone()),
        }
    }
    if has_all
        && !words
            .iter()
            .any(|w| w == "-o" || w == "--output" || w.starts_with("--output="))
    {
        // The whole project needs an output directory; name the slot.
        words.push("-o".to_string());
        words.push("<DIR>".to_string());
    }
    let rewritten = line(&words);
    let message = match first {
        "--all" => format!(
            "lute compile --all is now `{rewritten}` (compile with no document compiles the whole project)"
        ),
        "--out" => format!("lute compile --out is now `-o, --output`: `{rewritten}`"),
        _ => format!("lute compile --locales is now `--locale-bundle`: `{rewritten}`"),
    };
    Some(
        CliError::usage(message)
            .with_argument(first)
            .with_hint(rewritten),
    )
}

/// Options that take a value in some command this module rewrites, so the
/// word after one is never mistaken for an operand.
const VALUED: &[&str] = &[
    "--providers",
    "--project",
    "--permission-profile",
    "--deny",
    "--engine",
    "--script",
    "--format",
    "--occasion",
    "--occasion-axes",
    "--target",
    "--axis",
    "--relation",
    "--facts",
    "--until",
    "--where",
    "--attr",
    "--reward",
    "-o",
    "--output",
    "--out",
    "--node",
    "--for",
    "--at",
    "--max-items",
    "--state",
    "--fact",
    "--choice",
    "--choose",
    "--event",
    "--accepted-quest",
    "--accept",
    "--mock",
    "--entry",
    "--beat",
    "--derivation",
    "--explain",
    "--conditions-output",
    "--dump-conditions",
    "--template",
    "--dir",
    "--events-file",
    "--on",
];

/// An option renamed in 0.48: `(old, new, takes a value)`.
type Rename = (&'static str, &'static str, bool);

/// What one command's removed spellings are.
#[derive(Default)]
struct Spec {
    renames: &'static [Rename],
    /// Whether the command used to take the project directory as an operand.
    dir_operand: bool,
}

fn spec_of(command: &[&str]) -> Spec {
    let renames: &'static [Rename] = match command {
        ["context"] => &[("--target", "--node", true), ("--run", "--script", true)],
        ["constraints"] => &[("--run", "--all-scripts", false)],
        ["trace"] => &[
            ("--choose", "--choice", true),
            ("--accept", "--accepted-quest", true),
        ],
        ["play"] => &[
            ("--explain", "--derivation", true),
            ("--dump-conditions", "--conditions-output", true),
        ],
        ["run"] => &[("--dump-conditions", "--conditions-output", true)],
        ["calendar"] => &[("--facts", "--relation", true)],
        ["loc", "export" | "import"] => &[("--out", "--output", true)],
        ["new"] => &[("--dir", "--project", true)],
        _ => &[],
    };
    let dir_operand = matches!(
        command,
        ["impact"]
            | ["constraints"]
            | ["doctor"]
            | ["play"]
            | ["loc", "export" | "report"]
            | ["beats"]
            | ["calendar"]
            | ["scenario"]
            | ["lore"]
            | ["refs"]
    );
    Spec {
        renames,
        dir_operand,
    }
}

/// The 0.48 spelling of `command`'s remaining arguments `rest`, as one
/// `E-CLI-USAGE`, when any of them is a removed spelling (§5).
fn rewrite(command: &[&str], rest: &[String]) -> Option<CliError> {
    let spec = spec_of(command);
    let name = command.join(" ");
    let mut options: Vec<String> = Vec::new();
    let mut operands: Vec<String> = Vec::new();
    let mut reasons: Vec<String> = Vec::new();
    let mut first: Option<String> = None;
    let mut note = |reasons: &mut Vec<String>, argument: &str, reason: String| {
        first.get_or_insert_with(|| argument.to_string());
        reasons.push(reason);
    };
    let mut iter = rest.iter().peekable();
    // `scenario`'s own sub-view words (`reach`, `envelope`, `knowledge`).
    let mut scenario_sub: Option<String> = None;
    while let Some(word) = iter.next() {
        let (flag, inline) = match word.split_once('=') {
            Some((flag, value)) if word.starts_with("--") => (flag, Some(value.to_string())),
            _ => (word.as_str(), None),
        };
        if let Some((old, new, valued)) = spec.renames.iter().find(|(old, ..)| *old == flag) {
            note(&mut reasons, old, format!("`{old}` is now `{new}`"));
            options.push((*new).to_string());
            if *valued {
                match inline {
                    Some(value) => {
                        *options.last_mut().expect("just pushed") = format!("{new}={value}")
                    }
                    None => {
                        if let Some(value) = iter.next() {
                            options.push(value.clone());
                        }
                    }
                }
            }
            continue;
        }
        match (command, flag) {
            (["calendar"], "--csv") => {
                note(
                    &mut reasons,
                    "--csv",
                    "`--csv` is now `--format csv`".to_string(),
                );
                options.extend(["--format".to_string(), "csv".to_string()]);
                continue;
            }
            (["calendar"], "--occasion") => {
                let value = match inline {
                    Some(value) => Some(value),
                    None => iter.next().cloned(),
                };
                match value.as_deref().and_then(|v| v.split_once('@')) {
                    Some((occasion, axes)) => {
                        note(
                            &mut reasons,
                            "--occasion",
                            format!(
                                "`--occasion {occasion}@{axes}` is now `--occasion-axes {occasion}={axes}`"
                            ),
                        );
                        options
                            .extend(["--occasion-axes".to_string(), format!("{occasion}={axes}")]);
                    }
                    None => {
                        options.push("--occasion".to_string());
                        options.extend(value);
                    }
                }
                continue;
            }
            (["scenario"] | ["loc", "export"], "--format") => {
                let value = match inline {
                    Some(value) => Some(value),
                    None => iter.next().cloned(),
                };
                match (command, value.as_deref()) {
                    (["scenario"], Some("json")) => {
                        note(
                            &mut reasons,
                            "--format",
                            "`--format json` is now `--json`".to_string(),
                        );
                        options.push("--json".to_string());
                    }
                    (["loc", _], Some("json")) => note(
                        &mut reasons,
                        "--format",
                        "`--format json` is gone: JSON is the default (omit `--format`)"
                            .to_string(),
                    ),
                    _ => {
                        options.push("--format".to_string());
                        options.extend(value);
                    }
                }
                continue;
            }
            (["scenario"], "--for") => {
                note(&mut reasons, "--for", "`--for` is now `--node`".to_string());
                options.push("--node".to_string());
                match inline {
                    Some(value) => {
                        *options.last_mut().expect("just pushed") = format!("--node={value}")
                    }
                    None => options.extend(iter.next().cloned()),
                }
                continue;
            }
            _ => {}
        }
        if VALUED.contains(&flag) && inline.is_none() {
            options.push(word.clone());
            options.extend(iter.next().cloned());
        } else if word.starts_with('-') && word != "-" {
            options.push(word.clone());
        } else if command == ["scenario"]
            && scenario_sub.is_none()
            && matches!(word.as_str(), "reach" | "envelope" | "knowledge")
        {
            scenario_sub = Some(word.clone());
        } else {
            operands.push(word.clone());
        }
    }

    // Operands: the project directory (and `impact`'s target, `scenario`'s
    // node), `catalog refresh`'s snapshot directory.
    let mut leading: Vec<String> = Vec::new();
    let mut operands = operands.into_iter();
    match command {
        ["catalog", "refresh"] => {
            if let Some(dir) = operands.next() {
                note(
                    &mut reasons,
                    "--providers",
                    "`catalog refresh <DIR>` is now `--providers <DIR>`".to_string(),
                );
                leading.extend(["--providers".to_string(), dir]);
            }
        }
        ["patch"] => {
            // `patch <DIR> <PATCH>`: one operand is the 0.48 `patch <PATCH>`.
            let both: Vec<String> = operands.by_ref().collect();
            if let [dir, patch] = both.as_slice() {
                note(
                    &mut reasons,
                    "--project",
                    "a project directory operand is now `--project <DIR>`".to_string(),
                );
                leading.extend([patch.clone(), "--project".to_string(), dir.clone()]);
            } else {
                leading.extend(both);
            }
        }
        ["impact"] => {
            let dir = operands.next();
            let target = operands.next();
            if dir.is_some() {
                note(
                    &mut reasons,
                    "--project",
                    "`impact <DIR> <TARGET>` is now `impact --project <DIR> --node <TARGET>`"
                        .to_string(),
                );
            }
            if let Some(dir) = dir {
                leading.extend(["--project".to_string(), dir]);
            }
            if let Some(target) = target {
                leading.extend(["--node".to_string(), target]);
            }
        }
        _ if spec.dir_operand => {
            if let Some(dir) = operands.next() {
                note(
                    &mut reasons,
                    "--project",
                    "a project directory operand is now `--project <DIR>`".to_string(),
                );
                leading.extend(["--project".to_string(), dir]);
            }
        }
        _ => {}
    }
    // `scenario reach <NODE>` / `scenario envelope <NODE>`.
    if let Some(sub) = &scenario_sub {
        if let Some(node) = operands.next() {
            note(
                &mut reasons,
                "--node",
                format!("`scenario {sub} <NODE>` is now `scenario {sub} --node <NODE>`"),
            );
            options.extend(["--node".to_string(), node]);
        }
    }
    // `--format json` becomes `--json`; a line that already says `--json`
    // keeps one.
    let mut seen_json = false;
    options.retain(|o| o != "--json" || !std::mem::replace(&mut seen_json, true));
    let rest_operands: Vec<String> = operands.collect();
    let first = first?;
    let mut words: Vec<String> = command.iter().map(|w| (*w).to_string()).collect();
    if let Some(sub) = scenario_sub {
        words.push(sub);
    }
    words.extend(leading);
    words.extend(options);
    words.extend(rest_operands);
    let rewritten = line(&words);
    Some(
        CliError::usage(format!(
            "lute {name}: {}; use `{rewritten}`",
            reasons.join("; ")
        ))
        .with_argument(first)
        .with_hint(rewritten),
    )
}
