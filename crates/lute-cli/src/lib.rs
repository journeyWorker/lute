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
//! - `lute check --project <dir> [--json] [--providers <dir>]` — recursively
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
//! - `lute catalog refresh --providers <dir>` — re-stamp every pinned provider snapshot in
//!   `<dir>` against the current `capabilitySnapshot` and clear its `stale` flag,
//!   rewriting each file in the flat on-disk format `ProviderSet::load` reads
//!   (plugin §10; "an explicit `catalog refresh` precedes a build"). Correctness
//!   never depends on a live/remote catalog — refresh only canonicalizes and
//!   re-stamps the already-pinned artifacts, so `refresh` then `load` round-trips.

pub use play::events;
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
pub mod bench;
mod bindings;
mod bundle;
mod cli;
mod cmd_catalog;
mod cmd_check;
mod cmd_check_project;
mod cmd_compile;
mod cmd_constraints;
mod cmd_context;
mod cmd_diff;
mod cmd_expr;
mod cmd_fmt;
mod cmd_impact;
mod cmd_patch;
mod cmd_scenario;
mod cmd_schema;
mod cmd_trace;
mod cmd_version;
mod codes;
mod compile_all;
mod context;
#[cfg(test)]
mod differential;
mod doctor;
mod endings;
mod engine_matrix;
mod envelope;
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
mod removed;
mod rewrite;
pub(crate) use engine_matrix::EngineMatrix;
mod runner;
mod scaffold;
mod scenario_fmt;
mod stream;
mod telemetry;
mod testcmd;

use cli::{CatalogCommand, Cli, Command, LocCommand, NewCommand};
use cmd_catalog::run_refresh;
use cmd_compile::dispatch_compile;
use cmd_constraints::{run_constraints, ScriptSelection};
use cmd_context::run_context;
use cmd_impact::run_impact;
use cmd_scenario::run_scenario;
use cmd_trace::run_trace;
use cmd_version::run_version;
use envelope::CliError;

// The shared surface the other modules reach as `crate::…`.
pub(crate) use cli::ScenarioCommand;
pub(crate) use cmd_context::attr_type_str;
pub(crate) use cmd_scenario::graph::{fact_edge_label, topo_layers, when_visited_hint, FactGraph};
pub(crate) use cmd_scenario::reach::{
    format_prereq, reach_evidence, reach_verdict_text, unanchored_quests,
};
pub(crate) use cmd_scenario::{
    node_ref_to_id, primary_node_ambiguity_note, resolve_node_ref, NodeRef,
};
pub(crate) use lute_load::BuiltInput;
pub(crate) use lute_model::{assemble_root_scenario, node_cycle_degraded, RootScenario};
pub(crate) use lute_model::{reconcile_collected, ReconciledProject};
pub(crate) use output::{
    cwd_relative, render_diagnostics, severity_str, write_stdout, SeverityPolicy,
};
pub(crate) use project::{
    collect_project_docs, find_lute_files, parse_project_docs, project_assert_relations,
    project_quest_ids, ByRoot, DocGroup,
};

/// The `lute` command line: parse `std::env::args`, run the command, and
/// record the invocation in the local telemetry file (dsl 0.43.0 §6).
pub fn run() -> ExitCode {
    let started = std::time::Instant::now();
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    let code = dispatch(&args);
    telemetry::record(&args, code, started.elapsed());
    code
}

fn dispatch(args: &[std::ffi::OsString]) -> ExitCode {
    let json = envelope::json_mode_in(args);
    if let Some(error) = removed::check(args) {
        envelope::set_invocation(
            envelope::command_id_in(args),
            json || removed::names_format_json(args),
        );
        return envelope::fail(error);
    }
    let cli = match Cli::try_parse_from(
        std::iter::once(std::ffi::OsString::from("lute")).chain(args.iter().cloned()),
    ) {
        Ok(cli) => cli,
        Err(err) => {
            envelope::set_invocation(envelope::command_id_in(args), json);
            if let Some(code) = explain_subcommand_typo() {
                eprintln!("error: unrecognized subcommand 'explain'; did you mean `lute --explain {code}`?");
                return envelope::fail_quietly(
                    CliError::usage(format!(
                        "unrecognized subcommand 'explain'; did you mean `lute --explain {code}`?"
                    ))
                    .with_argument("explain")
                    .with_hint(format!("lute --explain {code}")),
                );
            }
            if !json || envelope::is_display(&err) {
                // What `err.exit()` does, minus the exit: telemetry records
                // usage errors and help displays too.
                let _ = err.print();
                return ExitCode::from(u8::try_from(err.exit_code()).unwrap_or(2));
            }
            let _ = err.print();
            return envelope::fail_quietly(envelope::usage_from_clap(&err));
        }
    };
    envelope::set_invocation(cli.command.as_ref().map(|c| c.id().to_string()), json);
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
            files,
            project,
            providers,
            permission_profile,
            engine,
            wip,
            gate,
            json,
        } => cmd_check::run(
            &files,
            &cmd_check::CheckOptions {
                json: json.json,
                providers: providers.providers.as_deref(),
                project: project.project.as_deref(),
                permission_profile: permission_profile.permission_profile.as_deref(),
                engine: engine.as_deref(),
                wip,
                policy: &SeverityPolicy::new(&gate.deny, gate.deny_warnings),
            },
        ),
        Command::Impact {
            node,
            project,
            providers,
            json,
        } => match project_for("impact", &project) {
            Ok(dir) => run_impact(&dir, &node, json.json, providers.providers.as_deref()),
            Err(code) => code,
        },
        Command::Constraints {
            project,
            providers,
            script,
            all_scripts,
            json,
        } => match project_for("constraints", &project) {
            Ok(dir) => run_constraints(
                &dir,
                json.json,
                &script,
                all_scripts,
                providers.providers.as_deref(),
            ),
            Err(code) => code,
        },
        Command::Lint {
            path,
            project,
            providers,
            deny,
            deny_warnings,
            config,
            json,
        } => lint::run_lint(&lint::LintOptions {
            path: path.as_deref(),
            project: project.project.as_deref(),
            providers: providers.providers.as_deref(),
            json: json.json,
            deny: &deny,
            deny_warnings,
            config: config.as_deref(),
        }),
        Command::Compile {
            file,
            project,
            providers,
            permission_profile,
            output,
            locale_bundle,
            gate,
            json,
            stream,
        } => dispatch_compile(&cmd_compile::CompileOptions {
            file: file.as_deref(),
            json: json.json,
            providers: providers.providers.as_deref(),
            project: project.project.as_deref(),
            permission_profile: permission_profile.permission_profile.as_deref(),
            output: output.as_deref(),
            locale_bundle: locale_bundle.as_deref(),
            deny: &gate.deny,
            deny_warnings: gate.deny_warnings,
            stream,
        }),
        Command::Expr {
            cel,
            project,
            permission_profile,
            gate,
            json,
        } => cmd_expr::run_expr(&cmd_expr::ExprOptions {
            cel: &cel,
            project: project.project.as_deref(),
            permission_profile: permission_profile.permission_profile.as_deref(),
            policy: &SeverityPolicy::new(&gate.deny, gate.deny_warnings),
            json: json.json,
        }),
        Command::Fmt { paths, check, json } => cmd_fmt::run(&paths, check, json.json),
        Command::Diff {
            before,
            after,
            json,
        } => cmd_diff::run(&before, &after, json),
        Command::Patch {
            file,
            project,
            providers,
            permission_profile,
            dry_run,
            json,
        } => match project_for("patch", &project) {
            Ok(dir) => cmd_patch::run(
                &dir,
                &file,
                dry_run,
                json.json,
                &lute_model::ModelOptions {
                    providers: providers.providers,
                    permission_profile: permission_profile.permission_profile,
                    ..lute_model::ModelOptions::default()
                },
            ),
            Err(code) => code,
        },
        Command::Context {
            file,
            project,
            providers,
            permission_profile,
            node,
            at,
            max_items,
            script,
            all_scripts,
            json,
        } => run_context(&cmd_context::ContextOptions {
            file: &file,
            json: json.json,
            providers: providers.providers.as_deref(),
            project: project.project.as_deref(),
            permission_profile: permission_profile.permission_profile.as_deref(),
            node: node.as_deref(),
            at: at.as_deref(),
            max_items,
            scripts: ScriptSelection {
                scripts: &script,
                all: all_scripts,
                providers: providers.providers.as_deref(),
                permission_profile: permission_profile.permission_profile.as_deref(),
            },
        }),
        Command::Trace {
            file,
            project,
            providers,
            permission_profile,
            gate,
            state,
            fact,
            choice,
            event,
            accepted_quest,
            occasion,
            mock,
            json,
            entry,
            beat,
            no_derive,
            expand,
        } => run_trace(&cmd_trace::TraceOptions {
            file: &file,
            state,
            fact,
            choose: choice,
            event,
            accept: accepted_quest,
            occasion,
            mock: mock.as_deref(),
            json: json.json,
            providers: providers.providers.as_deref(),
            project: project.project.as_deref(),
            permission_profile: permission_profile.permission_profile.as_deref(),
            policy: &SeverityPolicy::new(&gate.deny, gate.deny_warnings),
            entry: entry.as_deref(),
            beat: beat.as_deref(),
            no_derive,
            expand,
        }),
        Command::Tag {
            paths,
            renumber,
            commit,
            check,
            json,
        } => rewrite::run_tag(&rewrite::TagOptions {
            paths: &paths,
            renumber,
            commit,
            check,
            json: json.json,
        }),
        Command::Fix { paths, check, json } => rewrite::run_fix(&paths, check, json.json),
        Command::Catalog(CatalogCommand::Refresh {
            project,
            providers,
            json,
        }) => run_refresh(
            project.project.as_deref(),
            providers.providers.as_deref(),
            json.json,
        ),
        Command::Init { dir, template, .. } => {
            scaffold::run_init(dir.as_deref(), template.as_deref())
        }
        Command::New(new) => {
            let (kind, name, project, occasion, target, start) = match &new {
                NewCommand::Scene {
                    name,
                    project,
                    occasion,
                    target,
                    ..
                } => (
                    "scene",
                    name,
                    project,
                    occasion.as_deref(),
                    target.as_deref(),
                    false,
                ),
                NewCommand::Quest {
                    name,
                    project,
                    start,
                    ..
                } => ("quest", name, project, None, None, *start),
                NewCommand::Lore { name, project, .. } => {
                    ("lore", name, project, None, None, false)
                }
                NewCommand::Schema { name, project, .. } => {
                    ("schema", name, project, None, None, false)
                }
            };
            scaffold::run_new(
                kind,
                name,
                project.project.as_deref(),
                occasion,
                target,
                start,
            )
        }
        Command::Lore { project, json } => match project_for("lore", &project) {
            Ok(dir) => lore_report::run_lore(&dir, json.json),
            Err(code) => code,
        },
        Command::Refs {
            project,
            attr,
            reward,
            json,
        } => match project_for("refs", &project) {
            Ok(dir) => refs::run_refs(&dir, &attr, &reward, json.json),
            Err(code) => code,
        },
        Command::Doctor {
            project,
            json,
            strict,
        } => {
            // With no project the current directory is scanned and the
            // missing manifest reported (0.48.0 §3.14).
            let dir = project
                .project
                .or_else(project::discover_project_from_cwd)
                .unwrap_or_else(|| std::path::PathBuf::from("."));
            doctor::run_doctor(&dir, json.json, strict)
        }
        Command::Run {
            artifact,
            engine,
            mock,
            occasion,
            json,
            entry,
            beat,
            conditions_output,
        } => runner::run_artifact(
            &artifact,
            engine.as_deref(),
            mock.as_deref(),
            occasion,
            json,
            entry.as_deref(),
            beat.as_deref(),
            conditions_output.as_deref(),
        ),
        Command::Play {
            project,
            providers,
            permission_profile,
            gate,
            script,
            engine,
            events,
            json,
            no_derive,
            derivation,
            ir,
            quiet,
            conditions_output,
        } => match project_for("play", &project) {
            Ok(dir) => play::run_play(&play::PlayOptions {
                dir: &dir,
                script: &script,
                engine: engine.as_deref(),
                events,
                json: json.json,
                no_derive,
                derivation: &derivation,
                ir,
                quiet,
                conditions_output: conditions_output.as_deref(),
                build: play::Build {
                    providers: providers.providers.as_deref(),
                    permission_profile: permission_profile.permission_profile.as_deref(),
                    policy: SeverityPolicy::new(&gate.deny, gate.deny_warnings)
                        .for_project(Some(dir.as_path())),
                },
            }),
            Err(code) => code,
        },
        Command::Test {
            paths,
            project,
            providers,
            permission_profile,
            gate,
            json,
            coverage,
            no_derive,
        } => match project_for("test", &project) {
            Ok(dir) => testcmd::run_test(&testcmd::TestOptions {
                project: &dir,
                paths: &paths,
                json: json.json,
                coverage,
                no_derive,
                build: play::Build {
                    providers: providers.providers.as_deref(),
                    permission_profile: permission_profile.permission_profile.as_deref(),
                    policy: SeverityPolicy::new(&gate.deny, gate.deny_warnings)
                        .for_project(Some(dir.as_path())),
                },
            }),
            Err(code) => code,
        },
        Command::Loc(LocCommand::Export {
            project,
            format,
            output,
        }) => match project_for("loc export", &project) {
            Ok(dir) => {
                loc::run_export(&dir, format.as_deref().unwrap_or("json"), output.as_deref())
            }
            Err(code) => code,
        },
        Command::Loc(LocCommand::Import {
            files,
            output,
            json,
        }) => loc::run_import(&files, output.as_deref(), json.json),
        Command::Loc(LocCommand::Report { project, json }) => {
            match project_for("loc report", &project) {
                Ok(dir) => loc::run_report(&dir, json.json),
                Err(code) => code,
            }
        }
        Command::Scenario {
            project,
            providers,
            format,
            json,
            facts,
            command,
        } => {
            if facts && command.is_some() {
                return envelope::fail(
                    CliError::usage(
                        "lute scenario: --facts applies to the graph view only; \
                         `reach`/`envelope`/`knowledge` already show a node's facts",
                    )
                    .with_argument("--facts"),
                );
            }
            if json.json && format.is_some() {
                return envelope::fail(
                    CliError::usage(
                        "lute scenario: --json prints the JSON report; --format selects a text \
                         rendering (`text` or `dot`) — pass one of them",
                    )
                    .with_argument("--format"),
                );
            }
            let id = match &command {
                None => "scenario",
                Some(cli::ScenarioCommand::Reach { .. }) => "scenario reach",
                Some(cli::ScenarioCommand::Envelope { .. }) => "scenario envelope",
                Some(cli::ScenarioCommand::Knowledge { .. }) => "scenario knowledge",
            };
            let dir = match project_for(id, &project) {
                Ok(dir) => dir,
                Err(code) => return code,
            };
            let providers = providers.providers.as_deref();
            match format.as_deref() {
                _ if json.json => scenario_fmt::run(&dir, providers, command, "json", facts),
                None | Some("text") => run_scenario(&dir, providers, command, facts),
                Some(fmt) => scenario_fmt::run(&dir, providers, command, fmt, facts),
            }
        }
        Command::Beats {
            project,
            occasion,
            target,
            json,
            expand,
        } => match project_for("beats", &project) {
            Ok(dir) => beats_cmd::run_beats(&dir, &occasion, &target, json.json, expand),
            Err(code) => code,
        },
        Command::Calendar {
            project,
            providers,
            permission_profile,
            gate,
            axis,
            occasion,
            occasion_axes,
            relation,
            target,
            script,
            until,
            where_,
            format,
            json,
        } => {
            if json.json && format.is_some() {
                return envelope::fail(
                    CliError::usage(
                        "lute calendar: --json prints the JSON grid; --format selects a text \
                         rendering (`text` or `csv`) — pass one of them",
                    )
                    .with_argument("--format"),
                );
            }
            match project_for("calendar", &project) {
                Ok(dir) => play::calendar::run_calendar(
                    &dir,
                    &play::calendar::CalendarArgs {
                        axes: &axis,
                        occasions: &occasion,
                        occasion_axes: &occasion_axes,
                        relations: &relation,
                        targets: &target,
                        script: script.as_deref(),
                        until: until.as_deref(),
                        where_: where_.as_deref(),
                        json: json.json,
                        csv: format.as_deref() == Some("csv"),
                        build: play::Build {
                            providers: providers.providers.as_deref(),
                            permission_profile: permission_profile.permission_profile.as_deref(),
                            policy: SeverityPolicy::new(&gate.deny, gate.deny_warnings)
                                .for_project(Some(dir.as_path())),
                        },
                    },
                ),
                Err(code) => code,
            }
        }
        Command::Version { json } => run_version(json),
        Command::Schema { path } => cmd_schema::run(&path),
    }
}

/// The project a directory command works on (0.48.0 §2.1): `--project`, else
/// the nearest `lute.project.yaml` at or above the current directory, else
/// the `E-CLI-USAGE` exit naming `--project`.
fn project_for(command: &str, project: &cli::ProjectOpt) -> Result<std::path::PathBuf, ExitCode> {
    project::require_project(command, project.project.as_deref()).map_err(envelope::fail)
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
