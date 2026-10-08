//! `lute trace`: flag/mock assembly and rendering around
//! `lute_trace::trace_document`.

use std::path::Path;
use std::process::ExitCode;

use lute_check::check;
use lute_runtime::MockSet;
use lute_trace::{merge, parse_mock_yaml, TraceExit, TraceReport};

use crate::cmd_check::{component_name_of, component_root_diag};
use lute_load::{build_input, BuiltInput};
use crate::output::{print_diagnostics, write_stdout, DenyPolicy};
use lute_model::project_gate_result_in;
use crate::project::{discover_project, project_assert_relations_in, project_quest_ids_in};

/// Run `trace` over one file (dsl 0.4.0 §4.3/§4.5): resolve the document
/// IDENTICALLY to `check`/`compile` ([`build_input`]), load + merge the
/// `--mock` file with the CLI's own `--state`/`--fact`/`--choose`/`--event`/
/// `--accept`/`--occasion` flags into one [`MockSet`] ([`merge`] — "CLI flags compose with
/// the file; on a conflict the flag wins"), then hand off to
/// [`lute_trace::trace_document`] — the entire §4.3 mock-validation gate,
/// the §4.4 walk, and the §4.5 report are ITS concern; this function owns
/// only flag assembly, file I/O, and the exit-code/render mapping.
///
/// Exit codes (§4.5): `0` [`TraceExit::Complete`], `1`
/// [`TraceExit::Refused`] (a document check error OR an invalid mock — the
/// `E-TRACE-*` diagnostics render in EXACTLY [`print_diagnostics`]'s
/// check-diagnostic line format; a refusal whose diagnostics are NOT all
/// `E-TRACE-*` came from the `check` gate itself, so a "run `lute check`
/// first" hint is appended), `2` I/O (unreadable `.lute`/`--mock` file, or a
/// malformed `--mock` YAML document — the same tier `run_check`/`run_compile`
/// use for a read failure), `3` [`TraceExit::Incomplete`] (an `unknown`
/// guard halted the walk, or an unresolved objective/quest atom).
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_trace(
    file: &Path,
    state: Vec<(String, String)>,
    fact: Vec<String>,
    choose: Vec<(String, Vec<String>)>,
    event: Vec<String>,
    accept: Vec<String>,
    occasion: Vec<String>,
    mock: Option<&Path>,
    json: bool,
    providers: Option<&Path>,
    project: Option<&Path>,
    entry: Option<&str>,
    beat: Option<&str>,
    no_derive: bool,
    expand: bool,
) -> ExitCode {
    // FS-F2: resolved against the same project `lute check` resolves it
    // against — `--project`, else the nearest manifest. Only an explicit
    // `--project` switches the gate to the reconciled project verdict below.
    let discovered = discover_project(file, project);
    let resolved = project.or(discovered.as_deref());
    let Some(built) = build_input(file, providers, resolved, None) else {
        return ExitCode::from(2);
    };
    built.report_project_diags();
    let BuiltInput {
        input,
        resolve_error,
        ..
    } = built;
    // plugin 0.0.2 §2: an `E-` capability-resolution diagnostic (bad plugin
    // option, missing active plugin, bad identity template) is a build-failing
    // error; it printed above, and it MUST gate here or it would pass silently.
    if resolve_error {
        return ExitCode::from(1);
    }

    // dsl 0.19.0 §8: a lore document is looked up, not played — there is no
    // sequence to walk, so tracing one without `--entry` / `--beat` (dsl
    // 0.23.0 §4) is a usage error. (Either flag on a non-lore document / an
    // unknown id is `E-TRACE-ENTRY` / `E-TRACE-BEAT`, refused by `lute_trace`
    // below.)
    if entry.is_none() && beat.is_none() {
        let (doc, _) = lute_syntax::parse(&input.text);
        let (folded, _, _) = lute_check::fold_env(&doc, &input);
        if folded.doc_kind == lute_check::DocKind::Lore {
            let mut ways = Vec::new();
            if !doc.entries.is_empty() || doc.beats.is_empty() {
                let ids: Vec<&str> = doc.entries.iter().map(|e| e.id.as_str()).collect();
                ways.push(format!(
                    "`--entry <id>` to present one entry (declared: {})",
                    ids.join(", ")
                ));
            }
            if !doc.beats.is_empty() {
                let ids: Vec<String> = doc
                    .beats
                    .iter()
                    .map(|b| match folded.typed.id.as_deref() {
                        Some(doc_id) => lute_check::bundle_beat_key(doc_id, &b.id),
                        None => b.id.clone(),
                    })
                    .collect();
                ways.push(format!(
                    "`--beat <id>` to present one bundle beat (declared: {})",
                    ids.join(", ")
                ));
            }
            eprintln!(
                "lute trace: {} is a lore document — pass {}",
                file.display(),
                ways.join(" or ")
            );
            return ExitCode::from(2);
        }
    }

    let file_mocks = match mock {
        Some(path) => {
            let text = match std::fs::read_to_string(path) {
                Ok(t) => t,
                Err(e) => {
                    let e = lute_manifest::io_reason(&e);
                    eprintln!("lute: cannot read {}: {e}", path.display());
                    return ExitCode::from(2);
                }
            };
            // D-AC: the command line supplies the subject and it wins. A
            // `file:` that names a DIFFERENT document is the error — the two
            // ways of saying what a mock is for must not disagree in silence.
            match lute_trace::mock_subject(&text) {
                Ok(Some(rel)) => {
                    let base = path.parent().unwrap_or_else(|| Path::new("."));
                    let named = std::fs::canonicalize(base.join(&rel)).ok();
                    let target = std::fs::canonicalize(file).ok();
                    if named.is_none() || named != target {
                        eprintln!(
                            "lute: {}: [{}] `file: {rel}` names a different document than the one \
                             traced ({}) — the mock's subject and the command line must \
                             agree",
                            path.display(),
                            lute_trace::E_MOCK_SUBJECT,
                            file.display()
                        );
                        return ExitCode::from(2);
                    }
                }
                Ok(None) => {}
                Err(d) => {
                    eprintln!("lute: {}: [{}] {}", path.display(), d.code, d.text());
                    return ExitCode::from(2);
                }
            }
            match parse_mock_yaml(&text) {
                Ok(m) => m,
                Err(d) => {
                    // A malformed `--mock` YAML document is a file-level I/O/
                    // format failure, not a schema-validation refusal — `2`,
                    // matching `run_check`'s/`run_compile`'s read-failure tier.
                    //
                    // Rendered WITHOUT a line:column (D-AB). Every mock
                    // diagnostic carries `synthetic_span()`'s all-zeros, so
                    // the old `{line}:{column}` printed `mock.yaml:0:0` — a
                    // position that does not exist, which is the exact defect
                    // §8 opens with. The subject arm above already renders
                    // this way; now the grammar arm does too.
                    eprintln!("lute: {}: [{}] {}", path.display(), d.code, d.text());
                    return ExitCode::from(2);
                }
            }
        }
        None => MockSet::default(),
    };

    // `--state`/`--mock` literals and `--choose` targets carry no source
    // text: a diagnostic about one renders at the synthetic point.
    let flag_mocks = MockSet {
        state: state
            .into_iter()
            .map(|(path, literal)| (lute_trace::state_key(&path), literal, None))
            .collect(),
        facts: fact,
        choose: choose.into_iter().collect(),
        events: event,
        accepts: accept,
        occasions: occasion,
        visited: Vec::new(),
        derive: no_derive.then_some(false),
        bridges: Default::default(),
        bridge_spans: Default::default(),
        choose_spans: Default::default(),
        gate_eligibility: false,
        project_quests: None,
    };

    let mut mocks = merge(file_mocks, flag_mocks);
    // The project models this trace consults (gate, quest ids, producers),
    // each built once.
    let memo = lute_model::ModelMemo::default();
    // dsl 0.26.0 §7 (T3-5): `--accept` / `accepts:` resolve against every
    // quest of the project, not only this document's.
    if !mocks.accepts.is_empty() {
        mocks.project_quests = resolved.and_then(|dir| project_quest_ids_in(&memo, dir, providers));
    }
    // Project-aware gate (connectivity spec §5, mirrors `run_compile`): WITH
    // `--project <dir>` trace gates on the target's RECONCILED `check-project`
    // verdict; WITHOUT it, the standalone single-file `check` gate, unchanged.
    // The D1 quarantine holds — reconciliation is pure graph math, never
    // CEL/Datalog evaluation.
    let gate = match project {
        Some(dir) => match project_gate_result_in(&memo, file, dir, providers) {
            Ok(gate) => gate,
            Err(code) => return code,
        },
        None => check(&input),
    };

    // A component is not a root document (see [`component_root_diag`]). Refused
    // AFTER the gate above, so a component carrying real check errors still
    // reports them first — the kind refusal is what replaces the bare `ok`
    // path, not the diagnostic path.
    if gate.ok {
        if let Some((component, at)) = component_name_of(file) {
            let diag = component_root_diag(&component, at);
            if json {
                match serde_json::to_string_pretty(&[&diag]) {
                    Ok(s) => println!("{s}"),
                    Err(e) => {
                        eprintln!("lute: failed to serialize diagnostics: {e}");
                        return ExitCode::from(2);
                    }
                }
            } else {
                print_diagnostics(file, std::slice::from_ref(&diag), &DenyPolicy::default());
                println!(
                    "trace refused: {} is a component — trace a document that `::use`s it",
                    file.display()
                );
            }
            return ExitCode::from(1);
        }
    }

    // T1-14: judge mocked facts against the project's producers, not this
    // document's alone — `--project` when given, else the nearest manifest.
    // Only worth collecting when a fact was mocked at all.
    let project_asserts = if mocks.facts.is_empty() {
        None
    } else {
        match (project, &discovered) {
            (Some(dir), _) => project_assert_relations_in(&memo, dir, true, providers),
            (None, Some(root)) => project_assert_relations_in(&memo, root, false, providers),
            (None, None) => None,
        }
    };
    let (mut report, exit) = match (entry, beat) {
        (Some(id), _) => {
            lute_trace::trace_entry_with_check(&input, gate, mocks, id, project_asserts.as_ref())
        }
        (None, Some(id)) => {
            lute_trace::trace_beat_with_check(&input, gate, mocks, id, project_asserts.as_ref())
        }
        (None, None) => lute_trace::trace_with_check(&input, gate, mocks, project_asserts.as_ref()),
    };
    // T3-15: the project knows every quest — settle the
    // "existence is unverified" notes instead of repeating them.
    if !report.foreign_quests.is_empty() {
        if let Some(declared) = resolved.and_then(|dir| project_quest_ids_in(&memo, dir, providers)) {
            report.verify_quests(&declared);
        }
    }
    // A component's file prints the way the traced file does: relative to
    // the current directory, not as the canonical path the import resolved.
    report.respell_component_files(crate::output::cwd_relative);

    match exit {
        TraceExit::Complete => print_trace_report(&report, json, expand, ExitCode::SUCCESS),
        TraceExit::Incomplete => print_trace_report(&report, json, expand, ExitCode::from(3)),
        TraceExit::Refused(diags) => {
            if json {
                match serde_json::to_string_pretty(&diags) {
                    Ok(s) => println!("{s}"),
                    Err(e) => {
                        eprintln!("lute: failed to serialize diagnostics: {e}");
                        return ExitCode::from(2);
                    }
                }
            } else {
                // dsl 0.25.0 §1: a walk-time exclusive-relations refusal keeps
                // its transcript — the `✗ exclusive` line sits at the write.
                let exclusive = !diags.is_empty()
                    && diags
                        .iter()
                        .all(|d| d.code == lute_check::fact_check::E_FACT_EXCLUSIVE);
                if exclusive && write_stdout(&report.render_human()).is_err() {
                    return ExitCode::from(2);
                }
                // A `bridges:` answer's diagnostic is anchored in the mock's
                // own text (dsl 0.24.0 §5), so it renders against the mock.
                let (at_mock, at_doc): (Vec<_>, Vec<_>) = diags
                    .iter()
                    .cloned()
                    .partition(|d| d.provenance.as_deref() == Some(lute_trace::MOCK_TEXT));
                print_diagnostics(file, &at_doc, &DenyPolicy::default());
                print_diagnostics(mock.unwrap_or(file), &at_mock, &DenyPolicy::default());
                // Every `E-TRACE-*` code is mock/choice validation (D1
                // quarantine: `lute-check` cannot know that vocabulary, so
                // its OWN diagnostics never carry it) — a refusal carrying
                // anything else came from the `check` gate itself (§4.3:
                // "MUST refuse a document with check errors ... run `check`
                // first").
                let gate = lute_check::gates::E_OCCASION_GATE;
                if exclusive {
                    println!(
                        "trace refused: {} — exclusive relations hold together",
                        file.display()
                    );
                } else if diags.iter().all(|d| d.code == gate) {
                    println!(
                        "trace refused: {} — the engine would not raise a mocked occasion",
                        file.display()
                    );
                } else if diags
                    .iter()
                    .any(|d| !d.code.starts_with("E-TRACE-") && d.code != gate)
                {
                    println!(
                        "trace refused: {} has check error(s) — run `lute check` first",
                        file.display()
                    );
                } else if diags.iter().all(|d| d.code == lute_trace::E_TRACE_ENTRY) {
                    println!("trace refused: {} — invalid `--entry`", file.display());
                } else if diags.iter().all(|d| d.code == lute_trace::E_TRACE_BEAT) {
                    println!("trace refused: {} — invalid `--beat`", file.display());
                } else if diags.iter().all(|d| d.code == lute_trace::E_TRACE_CHOICE) {
                    println!(
                        "trace refused: {} — a `--choose` / `choose:` selection cannot be followed",
                        file.display()
                    );
                } else {
                    println!("trace refused: {} — invalid mock input", file.display());
                }
            }
            ExitCode::from(1)
        }
    }
}

/// Render one [`TraceReport`] to stdout — `--json` -> [`TraceReport::render_json`]
/// (§4.5 machine form), otherwise [`TraceReport::render_human`] (the
/// transcript already ends in `\n`; `--expand` →
/// [`TraceReport::render_human_expanded`]) — through [`write_stdout`], so a
/// closed pipe is an I/O exit `2` rather than a panic. `code` is the verdict
/// to return when the write succeeds.
fn print_trace_report(report: &TraceReport, json: bool, expand: bool, code: ExitCode) -> ExitCode {
    let text = if json {
        format!("{}\n", report.render_json())
    } else if expand {
        report.render_human_expanded()
    } else {
        report.render_human()
    };
    if write_stdout(&text).is_err() {
        return ExitCode::from(2);
    }
    code
}
