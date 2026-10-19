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
        _ => None,
    }
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
