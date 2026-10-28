//! The JSON contract every machine-readable CLI output shares (dsl 0.43.0
//! §3–§5): the command id, the report envelope, the error envelope, and the
//! table of each command's machine-readable outputs.
//!
//! [`run`](crate::run) records the invocation once ([`set_invocation`]): its
//! command id and whether it is in JSON mode. Report writers then wrap their
//! payload with [`report`], and every usage/I/O/input failure goes through
//! [`fail`], which keeps the human message on stderr and, in JSON mode, adds
//! the error envelope on stdout.

use std::ffi::OsString;
use std::path::Path;
use std::process::ExitCode;
use std::sync::OnceLock;

use clap::error::{ContextKind, ContextValue, ErrorKind};
use clap::CommandFactory;
use serde_json::{Map, Value};

use crate::cli::Cli;
use crate::output::{pretty_json, write_stdout};

/// Shape versions of the reports (dsl 0.43.0 §3.3). A version names a
/// payload shape, not a release: it changes only when that payload does.
pub(crate) mod version {
    pub(crate) const CHECK: &str = "0.48.0.check";
    pub(crate) const IMPACT: &str = "0.43.0.impact";
    pub(crate) const CONSTRAINTS: &str = "0.49.0.constraints";
    pub(crate) const LINT: &str = "0.43.0.lint";
    pub(crate) const COMPILE: &str = "0.43.0.compile";
    pub(crate) const EXPR: &str = "0.43.0.expr";
    pub(crate) const SURFACE: &str = "0.43.0.surface";
    pub(crate) const CONTEXT: &str = "0.36.0.context";
    pub(crate) const POSITION: &str = "0.36.0.position";
    pub(crate) const DIFF: &str = "0.36.0.diff";
    pub(crate) const PATCH: &str = "0.43.0.patch";
    pub(crate) const TRACE: &str = "0.43.0.trace";
    pub(crate) const PLAY: &str = "0.43.0.play";
    pub(crate) const TEST: &str = "0.43.0.test";
    pub(crate) const SCENARIO: &str = "0.43.0.scenario";
    pub(crate) const SCENARIO_REACH: &str = "0.43.0.scenario-reach";
    pub(crate) const SCENARIO_ENVELOPE: &str = "0.43.0.scenario-envelope";
    pub(crate) const SCENARIO_KNOWLEDGE: &str = "0.43.0.scenario-knowledge";
    pub(crate) const BEATS: &str = "0.43.0.beats";
    pub(crate) const CALENDAR: &str = "0.43.0.calendar";
    pub(crate) const REFS: &str = "0.43.0.refs";
    pub(crate) const LORE: &str = "0.43.0.lore";
    pub(crate) const LOC_REPORT: &str = "0.47.0.loc-report";
    pub(crate) const TAG: &str = "0.48.0.tag";
    /// 0.53.0 §4.3: `files[]` entries gain `rules` and `kept`.
    pub(crate) const FIX: &str = "0.53.0.fix";
    pub(crate) const FMT: &str = "0.48.0.fmt";
    pub(crate) const CATALOG_REFRESH: &str = "0.48.0.catalog-refresh";
    pub(crate) const INIT: &str = "0.48.0.init";
    pub(crate) const NEW: &str = "0.48.0.new";
    pub(crate) const LOC_IMPORT: &str = "0.48.0.loc-import";
    pub(crate) const DOCTOR: &str = "0.43.0.doctor";
    pub(crate) const SCHEMA: &str = "0.48.0.schema";
    pub(crate) const ERROR: &str = "0.43.0.error";
}

/// What a machine-readable output is (dsl 0.43.0 §5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OutputKind {
    /// An enveloped report (§3.3).
    Report,
    /// A document of its own consumed by engines or tools (§3.4).
    Artifact,
    /// A line-delimited stream with its own protocol (§3.4).
    Stream,
}

impl OutputKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            OutputKind::Report => "report",
            OutputKind::Artifact => "artifact",
            OutputKind::Stream => "stream",
        }
    }
}

/// One machine-readable output of a command: the flag(s) selecting it
/// (`None` when unconditional), its kind, and a report's shape version.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Output {
    pub(crate) flag: Option<&'static str>,
    pub(crate) kind: OutputKind,
    pub(crate) schema_version: Option<&'static str>,
}

const fn report_output(flag: &'static str, version: &'static str) -> Output {
    Output {
        flag: Some(flag),
        kind: OutputKind::Report,
        schema_version: Some(version),
    }
}

const fn document(flag: Option<&'static str>, kind: OutputKind) -> Output {
    Output {
        flag,
        kind,
        schema_version: None,
    }
}

/// The machine-readable outputs of each command (dsl 0.43.0 §3.3, §3.4) —
/// the one table both the envelope and `lute schema` read. A command absent
/// here has none.
static OUTPUTS: &[(&str, &[Output])] = &[
    ("check", &[report_output("--json", version::CHECK)]),
    ("impact", &[report_output("--json", version::IMPACT)]),
    (
        "constraints",
        &[report_output("--json", version::CONSTRAINTS)],
    ),
    ("lint", &[report_output("--json", version::LINT)]),
    ("tag", &[report_output("--json", version::TAG)]),
    ("fix", &[report_output("--json", version::FIX)]),
    ("fmt", &[report_output("--json", version::FMT)]),
    (
        "catalog refresh",
        &[report_output("--json", version::CATALOG_REFRESH)],
    ),
    ("init", &[report_output("--json", version::INIT)]),
    ("new scene", &[report_output("--json", version::NEW)]),
    ("new quest", &[report_output("--json", version::NEW)]),
    ("new lore", &[report_output("--json", version::NEW)]),
    ("new schema", &[report_output("--json", version::NEW)]),
    (
        "compile",
        &[
            document(None, OutputKind::Artifact),
            report_output("--json", version::COMPILE),
            document(Some("--stream"), OutputKind::Stream),
        ],
    ),
    ("expr", &[report_output("--json", version::EXPR)]),
    (
        "context",
        &[
            report_output("--json", version::SURFACE),
            report_output("--json --node", version::CONTEXT),
            report_output("--json --at", version::POSITION),
        ],
    ),
    ("diff", &[report_output("--json", version::DIFF)]),
    ("patch", &[report_output("--json", version::PATCH)]),
    ("trace", &[report_output("--json", version::TRACE)]),
    ("run", &[document(Some("--json"), OutputKind::Artifact)]),
    (
        "play",
        &[
            report_output("--json", version::PLAY),
            document(Some("--events"), OutputKind::Stream),
        ],
    ),
    ("test", &[report_output("--json", version::TEST)]),
    ("scenario", &[report_output("--json", version::SCENARIO)]),
    (
        "scenario reach",
        &[report_output("--json", version::SCENARIO_REACH)],
    ),
    (
        "scenario envelope",
        &[report_output("--json", version::SCENARIO_ENVELOPE)],
    ),
    (
        "scenario knowledge",
        &[report_output("--json", version::SCENARIO_KNOWLEDGE)],
    ),
    ("beats", &[report_output("--json", version::BEATS)]),
    ("calendar", &[report_output("--json", version::CALENDAR)]),
    ("refs", &[report_output("--json", version::REFS)]),
    ("lore", &[report_output("--json", version::LORE)]),
    (
        "loc export",
        &[
            document(None, OutputKind::Artifact),
            document(Some("--format csv"), OutputKind::Artifact),
        ],
    ),
    (
        "loc report",
        &[report_output("--json", version::LOC_REPORT)],
    ),
    (
        "loc import",
        &[report_output("--json", version::LOC_IMPORT)],
    ),
    ("doctor", &[report_output("--json", version::DOCTOR)]),
    ("version", &[document(Some("--json"), OutputKind::Artifact)]),
    (
        "schema",
        &[Output {
            flag: None,
            kind: OutputKind::Report,
            schema_version: Some(version::SCHEMA),
        }],
    ),
];

/// The machine-readable outputs of the command `id`; empty for a command
/// without one.
pub(crate) fn outputs(id: &str) -> &'static [Output] {
    OUTPUTS
        .iter()
        .find(|(name, _)| *name == id)
        .map_or(&[], |(_, outputs)| outputs)
}

struct Invocation {
    command: Option<String>,
    json: bool,
}

static INVOCATION: OnceLock<Invocation> = OnceLock::new();

/// Record this process's invocation (dsl 0.43.0 §3.1, §4.1). The first call
/// wins; later calls are ignored.
pub(crate) fn set_invocation(command: Option<String>, json: bool) {
    let _ = INVOCATION.set(Invocation { command, json });
}

/// The invocation's command id, when known.
pub(crate) fn command() -> Option<&'static str> {
    INVOCATION.get().and_then(|i| i.command.as_deref())
}

/// Whether the invocation is in JSON mode (dsl 0.43.0 §4.1).
pub(crate) fn json_mode() -> bool {
    INVOCATION.get().is_some_and(|i| i.json)
}

/// The command id `args` (without the program name) names, read without
/// parsing: the first operand naming a top-level command, then — for a
/// command with subcommands — the first later operand naming one of those.
/// Used where the command line did not parse (a usage error) and by
/// telemetry; a parsed command line has [`crate::cli::Command::id`].
pub(crate) fn command_id_in(args: &[OsString]) -> Option<String> {
    let root = Cli::command();
    let mut rest = operands(args);
    let first = rest.next()?;
    let top = root.get_subcommands().find(|c| c.get_name() == first)?;
    let mut id = top.get_name().to_string();
    if top.has_subcommands() {
        if let Some(sub) = rest.find_map(|word| {
            top.get_subcommands()
                .find(|c| c.get_name() == word)
                .map(|c| c.get_name().to_string())
        }) {
            id.push(' ');
            id.push_str(&sub);
        }
    }
    Some(id)
}

/// The words of `args` that are neither options nor the value of the one
/// top-level option that takes one (`--explain CODE`), up to `--`.
fn operands(args: &[OsString]) -> impl Iterator<Item = String> + '_ {
    let mut skip_value = false;
    args.iter()
        .map(|a| a.to_string_lossy().into_owned())
        .take_while(|a| a != "--")
        .filter(move |a| {
            if std::mem::take(&mut skip_value) {
                return false;
            }
            if a == "--explain" {
                skip_value = true;
                return false;
            }
            !a.starts_with('-')
        })
}

/// Whether `args` request JSON mode (0.48.0 §2.3): `--json` before any `--`,
/// or the `schema` command. (`--format json` was JSON mode in 0.47.2; it is a
/// removed spelling now and [`crate::removed::names_format_json`] decides its
/// error is still an envelope.)
pub(crate) fn json_mode_in(args: &[OsString]) -> bool {
    let words: Vec<String> = args
        .iter()
        .map(|a| a.to_string_lossy().into_owned())
        .take_while(|a| a != "--")
        .collect();
    let flag = words.iter().any(|w| w == "--json");
    flag || operands(args).next().is_some_and(|first| first == "schema")
}

/// `payload` (a JSON object) as an enveloped report (dsl 0.43.0 §3.2):
/// `command`, `schemaVersion`, and `ok` — `true` exactly when the process
/// exits `exit` = `0`.
pub(crate) fn report(schema_version: &str, exit: u8, payload: Value) -> Value {
    let Value::Object(mut map) = payload else {
        unreachable!("a report payload is a JSON object");
    };
    map.insert(
        "command".into(),
        command().map_or(Value::Null, |c| Value::String(c.into())),
    );
    map.insert("schemaVersion".into(), Value::String(schema_version.into()));
    map.insert("ok".into(), Value::Bool(exit == 0));
    Value::Object(map)
}

/// Print `payload` as the enveloped report of the running command on stdout
/// and return the exit code `exit`; a failed write is an `E-CLI-IO` failure.
pub(crate) fn emit_report(schema_version: &str, exit: u8, payload: Value) -> ExitCode {
    let wrapped = report(schema_version, exit, payload);
    let mut text = match pretty_json(&wrapped) {
        Ok(text) => text,
        Err(e) => {
            return fail(CliError::io_without_path(format!(
                "lute: cannot serialize the report: {e}"
            )))
        }
    };
    text.push('\n');
    if let Err(e) = write_stdout(&text) {
        return fail_quietly(CliError::io_without_path(format!(
            "lute: cannot write stdout: {e}"
        )));
    }
    ExitCode::from(exit)
}

/// The class of a CLI failure (dsl 0.43.0 §4.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ErrorCode {
    /// `E-CLI-USAGE`: the command line is invalid.
    Usage,
    /// `E-CLI-IO`: a path cannot be read or written.
    Io,
    /// `E-CLI-INPUT`: an input file is not valid for the command.
    Input,
}

impl ErrorCode {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            ErrorCode::Usage => "E-CLI-USAGE",
            ErrorCode::Io => "E-CLI-IO",
            ErrorCode::Input => "E-CLI-INPUT",
        }
    }
}

/// A usage, I/O or input failure: the human message printed to stderr as
/// it always was, plus the fields of the error envelope (dsl 0.43.0 §4.2).
#[derive(Debug)]
pub(crate) struct CliError {
    code: ErrorCode,
    message: String,
    path: Option<String>,
    argument: Option<String>,
    hint: Option<String>,
}

impl CliError {
    fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        CliError {
            code,
            message: message.into(),
            path: None,
            argument: None,
            hint: None,
        }
    }

    /// `E-CLI-USAGE` with the human `message`.
    pub(crate) fn usage(message: impl Into<String>) -> Self {
        CliError::new(ErrorCode::Usage, message)
    }

    /// `E-CLI-IO` for `path` with the human `message`.
    pub(crate) fn io(path: &Path, message: impl Into<String>) -> Self {
        CliError::new(ErrorCode::Io, message).with_path(path)
    }

    /// `E-CLI-IO` for a document [`lute_load::build_input`] could not read:
    /// the same message the loader already printed to stderr.
    pub(crate) fn unreadable_document(path: &Path) -> Self {
        let message = lute_load::read_document(path).err().map_or_else(
            || format!("lute: cannot read {}", path.display()),
            |e| e.to_string(),
        );
        CliError::io(path, message)
    }

    /// `E-CLI-INPUT` for `path` with the human `message`.
    pub(crate) fn input(path: &Path, message: impl Into<String>) -> Self {
        CliError::new(ErrorCode::Input, message).with_path(path)
    }

    /// `E-CLI-INPUT` when no single file is at fault (e.g. standard input).
    pub(crate) fn input_without_path(message: impl Into<String>) -> Self {
        CliError::new(ErrorCode::Input, message)
    }

    /// `E-CLI-IO` when no single path is at fault (e.g. standard output).
    pub(crate) fn io_without_path(message: impl Into<String>) -> Self {
        CliError::new(ErrorCode::Io, message)
    }

    pub(crate) fn with_path(mut self, path: &Path) -> Self {
        self.path = Some(path.display().to_string());
        self
    }

    /// The offending argument as typed (`--jsn`).
    pub(crate) fn with_argument(mut self, argument: impl Into<String>) -> Self {
        self.argument = Some(argument.into());
        self
    }

    /// A suggestion that would make the invocation valid.
    pub(crate) fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// The error envelope (dsl 0.43.0 §4.2).
    pub(crate) fn envelope(&self) -> Value {
        let mut error = Map::new();
        error.insert("code".into(), Value::String(self.code.as_str().into()));
        error.insert("message".into(), Value::String(self.message.clone()));
        for (key, value) in [
            ("path", &self.path),
            ("argument", &self.argument),
            ("hint", &self.hint),
        ] {
            if let Some(value) = value {
                error.insert(key.into(), Value::String(value.clone()));
            }
        }
        let mut map = Map::new();
        map.insert(
            "command".into(),
            command().map_or(Value::Null, |c| Value::String(c.into())),
        );
        map.insert("schemaVersion".into(), Value::String(version::ERROR.into()));
        map.insert("ok".into(), Value::Bool(false));
        map.insert("error".into(), Value::Object(error));
        Value::Object(map)
    }
}

/// The human message, as [`fail`] prints it.
impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CliError {}

/// Report `err` and return exit `2`: the human message on stderr, unchanged,
/// and in JSON mode the error envelope on stdout (dsl 0.43.0 §4.2).
pub(crate) fn fail(err: CliError) -> ExitCode {
    eprintln!("{err}");
    print_envelope(&err);
    ExitCode::from(2)
}

/// [`fail`] for a caller that already printed the human message itself:
/// only the JSON-mode envelope is added. Returns exit `2`.
pub(crate) fn fail_quietly(err: CliError) -> ExitCode {
    print_envelope(&err);
    ExitCode::from(2)
}

fn print_envelope(err: &CliError) {
    if json_mode() {
        if let Ok(text) = pretty_json(&err.envelope()) {
            let _ = write_stdout(&(text + "\n"));
        }
    }
}

/// A clap parse failure as `E-CLI-USAGE`: the first line of clap's message
/// (without its `error: ` prefix), the offending argument and clap's
/// suggestion when it has them.
pub(crate) fn usage_from_clap(err: &clap::Error) -> CliError {
    let rendered = err.render().to_string();
    let first = rendered.lines().next().unwrap_or_default();
    let message = first.strip_prefix("error: ").unwrap_or(first).to_string();
    let mut cli_error = CliError::usage(message);
    let string = |kind| match err.get(kind) {
        Some(ContextValue::String(s)) => Some(s.clone()),
        // clap lists suggestions from the least to the most similar.
        Some(ContextValue::Strings(v)) => v.last().cloned(),
        _ => None,
    };
    // clap names an option with its value placeholder (`--project <DIR>`);
    // the argument is the option itself.
    if let Some(argument) =
        string(ContextKind::InvalidArg).or_else(|| string(ContextKind::InvalidSubcommand))
    {
        let token = argument.split_whitespace().next().map(str::to_string);
        cli_error = cli_error.with_argument(token.unwrap_or(argument));
    }
    if let Some(hint) = string(ContextKind::SuggestedArg)
        .or_else(|| string(ContextKind::SuggestedSubcommand))
        .or_else(|| string(ContextKind::SuggestedValue))
    {
        cli_error = cli_error.with_hint(hint);
    }
    cli_error
}

/// Whether a clap "error" is a help or version display, which is not an
/// error (dsl 0.43.0 §4.2).
pub(crate) fn is_display(err: &clap::Error) -> bool {
    matches!(
        err.kind(),
        ErrorKind::DisplayHelp
            | ErrorKind::DisplayVersion
            | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(words: &[&str]) -> Vec<OsString> {
        words.iter().map(OsString::from).collect()
    }

    #[test]
    fn command_id_names_the_typed_subcommand_path() {
        assert_eq!(
            command_id_in(&args(&["check", "--json", "x.lute"])).as_deref(),
            Some("check")
        );
        assert_eq!(
            command_id_in(&args(&["scenario", "reach", "--node", "scene:a", "--json"])).as_deref(),
            Some("scenario reach")
        );
        assert_eq!(
            command_id_in(&args(&["scenario", "--json"])).as_deref(),
            Some("scenario")
        );
        assert_eq!(
            command_id_in(&args(&["loc", "report", "--project", "dir"])).as_deref(),
            Some("loc report")
        );
        assert_eq!(command_id_in(&args(&["--explain", "check", "chek"])), None);
        assert_eq!(command_id_in(&args(&["chek", "x"])), None);
        assert_eq!(command_id_in(&args(&[])), None);
    }

    #[test]
    fn json_mode_reads_flags_before_the_separator_and_the_schema_command() {
        assert!(json_mode_in(&args(&["check", "--json", "x"])));
        assert!(!json_mode_in(&args(&["scenario", "--format", "json"])));
        assert!(!json_mode_in(&args(&["scenario", "--format=json"])));
        assert!(json_mode_in(&args(&["schema"])));
        assert!(json_mode_in(&args(&["schema", "check"])));
        assert!(!json_mode_in(&args(&["scenario", "--format", "dot"])));
        assert!(!json_mode_in(&args(&["loc", "export"])));
        assert!(!json_mode_in(&args(&["check", "x", "--", "--json"])));
    }

    #[test]
    fn every_report_output_has_a_version_and_every_document_none() {
        for id in [
            "check",
            "impact",
            "constraints",
            "lint",
            "compile",
            "expr",
            "context",
            "diff",
            "patch",
            "trace",
            "run",
            "play",
            "test",
            "scenario",
            "scenario reach",
            "scenario envelope",
            "scenario knowledge",
            "beats",
            "calendar",
            "refs",
            "lore",
            "loc export",
            "loc report",
            "doctor",
            "version",
            "loc import",
            "schema",
        ] {
            let outs = outputs(id);
            assert!(!outs.is_empty(), "{id} has a machine-readable output");
            for out in outs {
                assert_eq!(
                    out.kind == OutputKind::Report,
                    out.schema_version.is_some(),
                    "{id}"
                );
            }
        }
        for id in ["tag", "fix", "fmt", "catalog refresh", "init", "new scene"] {
            assert!(!outputs(id).is_empty(), "{id} reports with --json");
        }
        assert_eq!(outputs("new lore")[0].schema_version, Some("0.48.0.new"));
        assert!(outputs("check-project").is_empty());
    }
}
