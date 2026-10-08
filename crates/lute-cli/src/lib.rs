//! The binary (`src/main.rs`) only calls [`run`]; the library exists so the
//! in-process benchmark (`lute-bench`) can drive the play runner ([`bench`]).
//!
//! Two subcommands, both thin shells over library code (arch: "`check()` is the
//! contract, not the LSP protocol" — the CLI adds argument parsing, file I/O, and
//! output formatting, and owns NO validation logic):
//!
//! - `lute check <file> [--json] [--providers <dir>]` — statically validate one
//!   `.lute` document against the built-in `lute.core` snapshot plus an optional
//!   pinned provider catalog. Exit `0` when clean, `1` when any `Error`-severity
//!   diagnostic is present (`CheckResult::ok`), `2` on an I/O failure. `--json`
//!   prints the serialized [`CheckResult`]; otherwise a human line per diagnostic.
//! - `lute check-project <dir> [--json] [--providers <dir>]` — recursively
//!   `check` every `*.lute` file under `<dir>` (deterministic sorted order),
//!   resolving EACH file's project root independently as its nearest
//!   ancestor directory containing a `lute.project.yaml` (bounded below by
//!   `<dir>` itself; falls back to `<dir>` when no ancestor has one) — so a
//!   `<dir>` containing nested subprojects checks each file against ITS OWN
//!   subproject, not the walk root. PLUS project-wide `<quest id>`
//!   uniqueness (dsl 0.2.0 §6.3), scoped PER RESOLVED PROJECT ROOT (two
//!   different subprojects declaring the same id is not a collision), for
//!   quest docs `check`'s own import-graph-scoped `E-QUEST-ID-DUP` (0.2.0
//!   F4) cannot see: two quest docs sharing an id with no `uses:`/`extends:`
//!   edge between them. ALSO, PER RESOLVED PROJECT ROOT, `W-QUEST-REF-UNKNOWN`
//!   (dsl 0.5.1 §1.4): every referenced reserved `quest.<id>.state` /
//!   `quest.<id>.objectives.<oid>.done` path across the root's docs must
//!   resolve to a quest (and objective) some quest doc in the root DEFINES —
//!   a WARNING (never flips the exit verdict) naming the referencing
//!   document and the unresolved path; single-file `lute check` has no
//!   project graph and never emits it. Exit `0` clean, `1` when any file has
//!   an `Error` or any resolved root's quest-id pass finds a collision, `2`
//!   on an I/O failure. `--json` prints a structured report (per-file
//!   `CheckResult`s + the project-wide diagnostics); otherwise per-file
//!   human lines plus a project-wide section.
//! - `lute catalog refresh <dir>` — re-stamp every pinned provider snapshot in
//!   `<dir>` against the current `capabilitySnapshot` and clear its `stale` flag,
//!   rewriting each file in the flat on-disk format `ProviderSet::load` reads
//!   (plugin §10; "an explicit `catalog refresh` precedes a build"). Correctness
//!   never depends on a live/remote catalog — refresh only canonicalizes and
//!   re-stamps the already-pinned artifacts, so `refresh` then `load` round-trips.

pub use play::{build_runtime, run_runtime_reference, RuntimePlayReference};
pub use play::events::{parse_script, run_events, ScriptStep};
use std::process::ExitCode;

use clap::Parser;

/// Append one formatted line to an output buffer — the EPIPE-safe
/// replacement for `println!` in a report that is written once through
/// [`write_stdout`] (T3-15: `println!` panics when the reader of a pipe goes
/// away, e.g. `lute scenario … | head`). Writing into a `String` cannot fail.
macro_rules! outln {
    ($out:expr) => {
        $out.push('\n')
    };
    ($out:expr, $($arg:tt)*) => {{
        use std::fmt::Write as _;
        let _ = writeln!($out, $($arg)*);
    }};
}

mod beats_cmd;
mod bundle;
pub mod bench;
mod cli;
mod cmd_catalog;
mod engine_matrix;
mod cmd_check;
mod cmd_check_project;
mod cmd_compile;
mod cmd_diff;
mod cmd_fmt;
mod cmd_patch;
mod cmd_context;
mod cmd_scenario;
mod cmd_trace;
mod cmd_constraints;
mod cmd_impact;
mod cmd_version;
mod codes;
mod compile_all;
mod context;
#[cfg(test)]
mod differential;
mod doctor;
mod endings;
mod explain;
mod knowledge;
mod lint;
mod loc;
mod lore_report;
mod manifests;
mod mockcheck;
mod output;
mod play;
mod play_expect;
mod plugin_origins;
mod project;
mod refs;
mod rewrite;
pub(crate) use engine_matrix::EngineMatrix;
mod runner;
mod scaffold;
mod scenario_fmt;
mod stream;
mod testcmd;

use cli::{CatalogCommand, Cli, Command, LocCommand};
use cmd_catalog::run_refresh;
use cmd_check::run_check;
use cmd_check_project::run_check_project;
use cmd_compile::dispatch_compile;
use cmd_context::run_context;
use cmd_constraints::run_constraints;
use cmd_impact::run_impact;
use cmd_scenario::run_scenario;
use cmd_trace::run_trace;
use cmd_version::run_version;

// The shared surface the other modules reach as `crate::…`.
pub(crate) use cli::ScenarioCommand;
pub(crate) use cmd_context::attr_type_str;
pub(crate) use cmd_scenario::graph::{fact_edge_label, topo_layers, when_visited_hint, FactGraph};
pub(crate) use cmd_scenario::reach::{format_prereq, reach_evidence, reach_verdict_text, unanchored_quests};
pub(crate) use cmd_scenario::{
    node_ref_to_id, primary_node_ambiguity_note, resolve_node_ref, NodeRef,
};
pub(crate) use lute_model::{
    assemble_root_scenario, node_cycle_degraded, RootScenario,
};
pub(crate) use lute_load::BuiltInput;
pub(crate) use output::{cwd_relative, render_diagnostics, severity_str, write_stdout, DenyPolicy};
pub(crate) use lute_model::{reconcile_collected, ReconciledProject};
pub(crate) use project::{
    collect_project_docs, find_lute_files, parse_project_docs, project_assert_relations,
    project_quest_ids, ByRoot, DocGroup,
};

/// The `lute` command line: parse `std::env::args` and run the command.
pub fn run() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(err) => {
            if let Some(code) = explain_subcommand_typo() {
                eprintln!("error: unrecognized subcommand 'explain'; did you mean `lute --explain {code}`?");
                return ExitCode::from(2);
            }
            err.exit()
        }
    };
    if let Some(code) = cli.explain {
        return codes::explain(code);
    }
    let Some(command) = cli.command else {
        // `arg_required_else_help` already answers a bare `lute`.
        let _ = <Cli as clap::CommandFactory>::command().print_help();
        return ExitCode::from(2);
    };
    match command {
        Command::Check {
            file,
            json,
            providers,
            project,
            permission_profile,
            engine,
            deny,
            deny_warnings,
        } => run_check(
            &file,
            json,
            providers.as_deref(),
            project.as_deref(),
            permission_profile.as_deref(),
            engine.as_deref(),
            &DenyPolicy::new(&deny, deny_warnings),
        ),
        Command::CheckProject {
            dir,
            json,
            providers,
            engine,
            deny,
            deny_warnings,
            wip,
        } => run_check_project(
            &dir,
            json,
            providers.as_deref(),
            &DenyPolicy::new(&deny, deny_warnings),
            wip,
            engine.as_deref(),
        ),
        Command::Impact { dir, target, json, providers } => {
            run_impact(&dir, &target, json, providers.as_deref())
        }
        Command::Constraints { dir, json, run, providers } => {
            run_constraints(&dir, json, run, providers.as_deref())
        }
        Command::Lint {
            path,
            json,
            deny,
            deny_warnings,
            config,
        } => lint::run_lint(&path, json, &deny, deny_warnings, config.as_deref()),
        Command::Compile {
            file,
            json,
            providers,
            project,
            permission_profile,
            out,
            all,
            locales,
            deny,
            deny_warnings,
        } => dispatch_compile(
            file.as_deref(),
            json,
            providers.as_deref(),
            project.as_deref(),
            permission_profile.as_deref(),
            out.as_deref(),
            all,
            locales.as_deref(),
            &DenyPolicy::new(&deny, deny_warnings),
        ),
        Command::CompileStream {
            file,
            providers,
            project,
            permission_profile,
        } => stream::run(
            &file,
            providers.as_deref(),
            project.as_deref(),
            permission_profile.as_deref(),
        ),
        Command::Fmt { paths, check } => cmd_fmt::run(&paths, check),
        Command::Diff { before, after, json } => cmd_diff::run(&before, &after, json),
        Command::Patch {
            dir,
            patch,
            dry_run,
            json,
        } => cmd_patch::run(&dir, &patch, dry_run, json),
        Command::Context {
            file,
            target,
            at,
            max_items,
            run,
            json,
            providers,
            project,
            permission_profile,
        } => run_context(
            &file,
            json,
            providers.as_deref(),
            project.as_deref(),
            permission_profile.as_deref(),
            target.as_deref(),
            at.as_deref(),
            max_items,
            run.as_deref(),
        ),
        Command::Trace { retired_on, .. } if !retired_on.is_empty() => {
            retired_on_flag("trace", &retired_on)
        }
        Command::Beats { retired_on, .. } if !retired_on.is_empty() => {
            retired_on_flag("beats", &retired_on)
        }
        Command::Calendar { retired_on, .. } if !retired_on.is_empty() => {
            retired_on_flag("calendar", &retired_on)
        }
        Command::Trace {
            occasion, target, ..
        } if !target.is_empty() => {
            let t = &target[0];
            let spelled = match occasion.as_slice() {
                [o] if !o.contains('@') => format!("`--occasion {o}@{t}`"),
                _ => format!("`--occasion <occasion>@{t}`"),
            };
            eprintln!(
                "lute trace: `--target {t}`: trace names a raise's target in the occasion — \
                 {spelled}"
            );
            ExitCode::from(2)
        }
        Command::Trace {
            file,
            state,
            fact,
            choose,
            event,
            accept,
            occasion,
            target: _,
            retired_on: _,
            mock,
            json,
            providers,
            project,
            entry,
            beat,
            no_derive,
            expand,
        } => run_trace(
            &file,
            state,
            fact,
            choose,
            event,
            accept,
            occasion,
            mock.as_deref(),
            json,
            providers.as_deref(),
            project.as_deref(),
            entry.as_deref(),
            beat.as_deref(),
            no_derive,
            expand,
        ),
        Command::Tag { path, force } => rewrite::run_tag(&path, force),
        Command::Fix { paths } => rewrite::run_fix(&paths),
        Command::Catalog(CatalogCommand::Refresh { dir, project }) => {
            run_refresh(&dir, project.as_deref())
        }
        Command::Init { dir, template } => scaffold::run_init(&dir, template.as_deref()),
        Command::New {
            kind,
            name,
            dir,
            occasion,
            retired_on,
            target,
            start,
        } => match retired_on {
            Some(on) => {
                eprintln!(
                    "lute new: `--on` is now `--occasion`, as in every other command — \
                     `lute new {} {} --occasion {}`",
                    shell_word(&kind),
                    shell_word(&name),
                    shell_word(&on)
                );
                ExitCode::from(2)
            }
            None => scaffold::run_new(
                &kind,
                &name,
                dir.as_deref(),
                occasion.as_deref(),
                target.as_deref(),
                start,
            ),
        },
        Command::Lore { dir, json } => lore_report::run_lore(&dir, json),
        Command::Refs {
            dir,
            attr,
            reward,
            json,
        } => refs::run_refs(&dir, &attr, &reward, json),
        Command::Doctor { dir, json, strict } => doctor::run_doctor(&dir, json, strict),
        Command::Run {
            artifact,
            engine,
            mock,
            occasion,
            json,
            entry,
            beat,
            dump_conditions,
        } => runner::run_artifact(
            &artifact,
            engine.as_deref(),
            mock.as_deref(),
            occasion,
            json,
            entry.as_deref(),
            beat.as_deref(),
            dump_conditions.as_deref(),
        ),
        Command::Play {
            dir,
            script,
            engine,
            events,
            json,
            no_derive,
            explain,
            ir,
            quiet,
            dump_conditions,
        } => play::run_play(
            &dir,
            &script,
            engine.as_deref(),
            events,
            json,
            no_derive,
            &explain,
            ir,
            quiet,
            dump_conditions.as_deref(),
        ),
        Command::Test {
            dir,
            json,
            providers,
            project,
            coverage,
            no_derive,
        } => testcmd::run_test(
            &dir,
            json,
            providers.as_deref(),
            project.as_deref(),
            coverage,
            no_derive,
        ),
        Command::Loc(LocCommand::Export { dir, format, out }) => {
            loc::run_export(&dir, format.as_deref().unwrap_or("json"), out.as_deref())
        }
        Command::Loc(LocCommand::Import { files, out }) => loc::run_import(&files, out.as_deref()),
        Command::Loc(LocCommand::Report { dir, json }) => loc::run_report(&dir, json),
        Command::Scenario {
            dir,
            providers,
            format,
            facts,
            command,
        } => {
            if facts && command.is_some() {
                eprintln!(
                    "lute scenario: --facts applies to the graph view only; \
                     `reach`/`envelope`/`knowledge` already show a node's facts"
                );
                return ExitCode::from(2);
            }
            match format.as_deref() {
                None | Some("text") => run_scenario(&dir, providers.as_deref(), command, facts),
                Some(fmt) => scenario_fmt::run(&dir, providers.as_deref(), command, fmt, facts),
            }
        }
        Command::Beats {
            dir,
            occasion,
            target,
            json,
            expand,
            retired_on: _,
        } => beats_cmd::run_beats(&dir, &occasion, &target, json, expand),
        Command::Calendar {
            dir,
            axis,
            occasion,
            facts,
            target,
            script,
            until,
            where_,
            json,
            csv,
            retired_on: _,
        } => play::calendar::run_calendar(
            &dir,
            &play::calendar::CalendarArgs {
                axes: &axis,
                occasions: &occasion,
                facts: &facts,
                targets: &target,
                script: script.as_deref(),
                until: until.as_deref(),
                where_: where_.as_deref(),
                json,
                csv,
            },
        ),
        Command::Version { json } => run_version(json),
    }
}

/// `lute <command> --on X`: the flag is `--occasion` everywhere since 0.28;
/// name the right one rather than clap's `-- --on` tip.
fn retired_on_flag(command: &str, on: &[String]) -> ExitCode {
    let flags: Vec<String> = on
        .iter()
        .map(|o| format!("--occasion {}", shell_word(o)))
        .collect();
    eprintln!(
        "lute {command}: there is no `--on`; an occasion is named with `--occasion`, as in \
         every command — `{}`",
        flags.join(" ")
    );
    ExitCode::from(2)
}

/// `word` as one shell argument: bare when it is plain, else single-quoted.
fn shell_word(word: &str) -> String {
    let plain = !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./@:,=+".contains(c));
    if plain {
        word.to_string()
    } else {
        format!("'{}'", word.replace('\'', r"'\''"))
    }
}

/// `lute explain E-FOO`: the diagnostic-code lookup is the `--explain` flag,
/// not a subcommand, and clap would otherwise suggest `play`. Returns the
/// code-shaped argument (`^[EWL]-[A-Z0-9-]+$`, any case) when the first
/// positional argument is `explain` and the next one looks like a code.
fn explain_subcommand_typo() -> Option<String> {
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    let at = args.iter().position(|a| !a.starts_with('-'))?;
    if args[at] != "explain" {
        return None;
    }
    let code = args.get(at + 1)?;
    let upper = code.to_ascii_uppercase();
    let bytes = upper.as_bytes();
    let looks_like_code = bytes.len() > 2
        && matches!(bytes[0], b'E' | b'W' | b'L')
        && bytes[1] == b'-'
        && bytes[2..]
            .iter()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || *b == b'-');
    looks_like_code.then(|| code.clone())
}
