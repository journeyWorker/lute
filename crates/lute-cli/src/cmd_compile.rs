//! `lute compile`: single-file compile and the `--all` routing.

use std::fmt::Write as _;
use std::path::Path;
use std::process::ExitCode;

use lute_check::check;
use lute_core_span::Severity;
use lute_manifest::project::load_project;

use crate::cmd_check::{component_name_of, component_root_diag};
use crate::compile_all;
use crate::input::{build_input, BuiltInput};
use crate::loc;
use crate::output::{render_diagnostics, severity_str, write_stdout, DenyPolicy};
use crate::project::gate::project_gate_result;

/// Route `lute compile` to the single-file ([`run_compile`]) or whole-project
/// ([`compile_all::run`]) path, rejecting every flag combination that means
/// neither.
///
/// `--all` REQUIRES `--project <dir>` (it has no other way to know which
/// documents belong to the project, and the capability snapshot resolves per
/// project) and `-o <dir>` (there is no single artifact to put on stdout). It
/// also takes no `<file>`: naming one would imply the other documents are
/// somehow secondary, which they are not. Every violation is exit `2`, the
/// usage tier — clap cannot express these dependencies itself, so they are
/// checked here and reported in clap's own voice.
#[allow(clippy::too_many_arguments)]
pub(crate) fn dispatch_compile(
    file: Option<&Path>,
    json: bool,
    providers: Option<&Path>,
    project: Option<&Path>,
    permission_profile: Option<&str>,
    out: Option<&Path>,
    all: bool,
    locales: Option<&Path>,
    policy: &DenyPolicy,
) -> ExitCode {
    if !all {
        let Some(file) = file else {
            eprintln!(
                "error: the following required arguments were not provided:\n  <FILE>\n\n\
                 Usage: lute compile <FILE>\n       lute compile --all --project <DIR> -o <DIR>"
            );
            return ExitCode::from(2);
        };
        return run_compile(
            file,
            json,
            providers,
            project,
            permission_profile,
            out,
            locales,
            policy,
        );
    }

    let mut usage: Vec<&str> = Vec::new();
    if project.is_none() {
        usage.push("--all requires --project <DIR> (the document set and capability snapshot both resolve per project)");
    }
    if out.is_none() {
        usage.push("--all requires -o <DIR>, an output DIRECTORY (there is no single artifact to write to stdout)");
    }
    if file.is_some() {
        usage.push("--all takes no <FILE>: it compiles every document under --project");
    }
    if !usage.is_empty() {
        for message in usage {
            eprintln!("error: {message}");
        }
        eprintln!("\nUsage: lute compile --all --project <DIR> -o <DIR>");
        return ExitCode::from(2);
    }
    let bundle = match locales.map(load_locale_bundle).transpose() {
        Ok(b) => b,
        Err(code) => return code,
    };
    compile_all::run(
        project.expect("checked above"),
        out.expect("checked above"),
        providers,
        permission_profile,
        json,
        bundle.as_ref(),
        policy,
    )
}

/// Run `compile` over one file. Exit `0` with the artifact on stdout (or
/// `-o <FILE>`), `1` when the check gate fails (diagnostics to stdout,
/// human or `--json`), `2` on I/O or serialization failure.
///
/// With `--locales <bundle.json>` the compiled artifact additionally carries
/// per-record locale texts ([`load_locale_bundle`] then
/// [`lute_compile::locale::merge_locales`], dsl 0.8.0 §7). Any resulting
/// `W-L10N-MISSING` prints to STDERR — stdout may be carrying the artifact —
/// and, when `--deny` promotes it, flips the verdict to `1` with NO artifact
/// written.
fn run_compile(
    file: &Path,
    json: bool,
    providers: Option<&Path>,
    project: Option<&Path>,
    permission_profile: Option<&str>,
    out: Option<&Path>,
    locales: Option<&Path>,
    policy: &DenyPolicy,
) -> ExitCode {
    // Loaded BEFORE the compile so a malformed bundle fails fast, before any
    // work — and, with `-o`, before the previous artifact is overwritten.
    let bundle = match locales.map(load_locale_bundle).transpose() {
        Ok(b) => b,
        Err(code) => return code,
    };
    let Some(built) = build_input(file, providers, project, permission_profile) else {
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
    // Project-aware gate (connectivity spec §5): WITH `--project <dir>` the
    // target compiles against its RECONCILED `check-project` verdict (an
    // envelope-Guaranteed `run.*`/`user.*` read no longer blocks; a read no
    // route guarantees blocks with `E-STATE-MAYBE-UNAVAILABLE`). WITHOUT it,
    // the standalone single-file `check` gate, unchanged.
    // 0.8.0 §9: the `identity:` block templates `lineId`/`voiceKey`. It is a
    // PROJECT setting, so it only applies on the `--project` path; a loose
    // scene keeps `IdentityTemplates::default()`, i.e. 0.7.0's pair. A project
    // that fails to load already printed its error in `build_input`; falling
    // back to the default here matches that core-only degradation.
    let (gate, identity) = match project {
        Some(dir) => {
            let identity = load_project(dir)
                .ok()
                .flatten()
                .map(|p| p.identity)
                .unwrap_or_default();
            match project_gate_result(file, dir, providers) {
                Ok(gate) => (gate, identity),
                Err(code) => return code,
            }
        }
        None => (check(&input), Default::default()),
    };

    // A component is not a root document (see [`component_root_diag`]): there is
    // no standalone artifact to emit. Refused AFTER the gate, so a component
    // with real check errors still reports them.
    let compiled = match component_name_of(file).filter(|_| gate.ok) {
        Some((component, at)) => Err(vec![component_root_diag(&component, at)]),
        None => lute_compile::compile_with_check(&input, gate, &identity),
    };
    match compiled {
        Ok(mut artifact) => {
            // dsl 0.8.0 §7: merge strictly downstream of the addressing pass —
            // `compile_with_check` has already stamped every final `lineId`,
            // which is the ONLY key a bundle joins on.
            if let Some(bundle) = &bundle {
                let missing = lute_compile::locale::merge_locales(&mut artifact, bundle);
                // STDERR, not stdout: without `-o` the artifact itself is on
                // stdout, and a warning line in the middle of it would make
                // the compile output unparseable.
                eprint!("{}", render_diagnostics(file, &missing, policy));
                let denied = missing.iter().filter(|d| policy.denied(d)).count();
                if denied > 0 {
                    eprintln!("--deny promoted {denied} diagnostic(s); no artifact emitted");
                    return ExitCode::FAILURE;
                }
            }
            let mut s = match serde_json::to_string_pretty(&artifact) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("lute: failed to serialize artifact: {e}");
                    return ExitCode::from(2);
                }
            };
            s.push('\n');
            match out {
                Some(path) => {
                    if let Err(e) = std::fs::write(path, &s) {
                        eprintln!("lute: cannot write {}: {e}", path.display());
                        return ExitCode::from(2);
                    }
                }
                None => {
                    if write_stdout(&s).is_err() {
                        return ExitCode::from(2);
                    }
                }
            }
            ExitCode::SUCCESS
        }
        Err(diags) => {
            let s = if json {
                let mut s = match serde_json::to_string_pretty(&diags) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("lute: failed to serialize diagnostics: {e}");
                        return ExitCode::from(2);
                    }
                };
                s.push('\n');
                s
            } else {
                let mut s = String::new();
                for d in &diags {
                    let _ = writeln!(
                        s,
                        "{}:{}:{}: {} [{}] {}",
                        file.display(),
                        d.span.line,
                        d.span.column,
                        severity_str(d.severity),
                        d.code,
                        d.text()
                    );
                }
                let errors = diags
                    .iter()
                    .filter(|d| d.severity == Severity::Error)
                    .count();
                let _ = writeln!(s, "{errors} error(s); no artifact emitted");
                s
            };
            if write_stdout(&s).is_err() {
                return ExitCode::from(2);
            }
            ExitCode::FAILURE
        }
    }
}

/// Read + parse a `--locales <bundle.json>` file (dsl 0.8.0 §7). A missing or
/// unreadable file is I/O (`2`, matching every other file the CLI opens); a
/// file that IS readable but is not a bundle is `E-LOCALE-BUNDLE` (`1`) — the
/// same code `lute loc import` reports for a malformed input, so the two ends
/// of the round trip name the same defect the same way.
fn load_locale_bundle(path: &Path) -> Result<lute_compile::locale::LocaleBundle, ExitCode> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        eprintln!("lute: cannot read {}: {e}", path.display());
        ExitCode::from(2)
    })?;
    lute_compile::locale::LocaleBundle::parse(&text).map_err(|msg| {
        eprintln!("{}: error [{}] {msg}", path.display(), loc::E_LOCALE_BUNDLE);
        ExitCode::FAILURE
    })
}
