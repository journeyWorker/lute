//! `lute compile`: a document, the whole project, or the NDJSON stream.

use std::fmt::Write as _;
use std::path::Path;
use std::process::ExitCode;

use lute_check::check;
use lute_core_span::Severity;

use crate::cmd_check::{component_name_of, component_root_diag};
use crate::compile_all;
use crate::envelope::{self, CliError};
use crate::loc;
use crate::output::{
    diagnostics_json, insert_suppressed, marker_str, pretty_json, render_diagnostics, severity_str,
    suppressed_tail, write_stdout, SeverityPolicy,
};
use crate::project::resolve_project;
use lute_load::{build_input, read_document, BuiltInput};
use lute_model::project_gate_result;
use lute_model::{ModelError, ModelOptions, ProjectModel};

/// What `lute compile` was asked.
pub(crate) struct CompileOptions<'a> {
    pub(crate) file: Option<&'a Path>,
    pub(crate) json: bool,
    pub(crate) providers: Option<&'a Path>,
    pub(crate) project: Option<&'a Path>,
    pub(crate) permission_profile: Option<&'a str>,
    pub(crate) output: Option<&'a Path>,
    pub(crate) locale_bundle: Option<&'a Path>,
    pub(crate) deny: &'a [String],
    pub(crate) deny_warnings: bool,
    pub(crate) stream: bool,
}

/// A usage error naming the conflicting option.
fn conflict(message: String, argument: &str) -> ExitCode {
    envelope::fail(CliError::usage(message).with_argument(argument))
}

/// Route `lute compile` to the single-document ([`run_compile`]), whole-project
/// ([`compile_all::run`]) or stream ([`crate::stream::run`]) path, rejecting
/// every flag combination that means none of them (exit `2`, the usage tier —
/// clap cannot express these dependencies itself).
///
/// With no document the whole project (`--project`, else the nearest
/// `lute.project.yaml`) compiles into the REQUIRED `-o <DIR>` — there is no
/// single execution IR to put on stdout. `--stream` takes a document and
/// none of the options that only mean something for an artifact or a report.
pub(crate) fn dispatch_compile(opts: &CompileOptions<'_>) -> ExitCode {
    let CompileOptions {
        file,
        json,
        providers,
        project,
        permission_profile,
        output,
        locale_bundle,
        deny,
        deny_warnings,
        stream,
    } = *opts;
    if stream {
        let Some(file) = file else {
            return conflict(
                "lute compile: --stream needs the scene prefix: lute compile --stream <FILE>"
                    .to_string(),
                "--stream",
            );
        };
        let conflicting = [
            (output.is_some(), "--output"),
            (locale_bundle.is_some(), "--locale-bundle"),
            (json, "--json"),
            (!deny.is_empty(), "--deny"),
            (deny_warnings, "--deny-warnings"),
        ]
        .into_iter()
        .find_map(|(given, name)| given.then_some(name));
        if let Some(name) = conflicting {
            return conflict(
                format!(
                    "lute compile: --stream does not take {name} — it streams NDJSON snapshots on stdout"
                ),
                name,
            );
        }
        return crate::stream::run(file, providers, project, permission_profile);
    }
    let policy = SeverityPolicy::new(deny, deny_warnings);
    if let Some(file) = file {
        return run_compile(
            file,
            json,
            providers,
            project,
            permission_profile,
            output,
            locale_bundle,
            &policy,
        );
    }
    let Some(output) = output else {
        return conflict(
            "lute compile: compiling the whole project requires -o, --output <DIR>, an output DIRECTORY (there is no single execution IR to write to stdout); to compile one document name it: lute compile <FILE>"
                .to_string(),
            "--output",
        );
    };
    let project = match crate::project::require_project("compile", project) {
        Ok(project) => project,
        Err(error) => return envelope::fail(error),
    };
    let bundle = match locale_bundle.map(load_locale_bundle).transpose() {
        Ok(b) => b,
        Err(code) => return code,
    };
    compile_all::run(
        &project,
        output,
        providers,
        permission_profile,
        json,
        bundle.as_ref(),
        &policy.for_project(Some(&project)),
    )
}

/// Run `compile` over one file. Exit `0` with the execution IR on stdout (or
/// `-o <FILE>`), `1` when the check gate fails (diagnostics to stdout,
/// human or `--json`), `2` on I/O or serialization failure.
///
/// With `--locale-bundle <bundle.json>` the compiled execution IR additionally carries
/// per-record locale texts ([`load_locale_bundle`] then
/// [`lute_compile::locale::merge_locales`], dsl 0.8.0 §7, 0.47.0 §3). Any
/// resulting `W-L10N-MISSING` / `W-L10N-STALE` prints to STDERR — stdout may be
/// carrying the execution IR — and, when `--deny` promotes it, flips the
/// verdict to `1` with NO execution IR written.
#[expect(
    clippy::too_many_arguments,
    reason = "CLI flags are kept explicit at the command boundary"
)]
fn run_compile(
    file: &Path,
    json: bool,
    providers: Option<&Path>,
    project: Option<&Path>,
    permission_profile: Option<&str>,
    out: Option<&Path>,
    locales: Option<&Path>,
    gate_policy: &SeverityPolicy,
) -> ExitCode {
    // Loaded BEFORE the compile so a malformed bundle fails fast, before any
    // work — and, with `-o`, before the previous execution IR is overwritten.
    let bundle = match locales.map(load_locale_bundle).transpose() {
        Ok(b) => b,
        Err(code) => return code,
    };
    // FS-F2: the same project `lute check` resolves the file against —
    // `--project`, else the nearest manifest (its `defaults:`, profile and
    // `identity:` apply, and the project verdict gates the compile).
    let resolved = resolve_project(file, project);
    let policy = &gate_policy.for_project(resolved.as_deref());
    let Some(built) = build_input(file, providers, resolved.as_deref(), permission_profile) else {
        if let Err(e) = read_document(file) {
            return envelope::fail_quietly(CliError::io(file, e.to_string()));
        }
        return envelope::fail(CliError::input(
            file,
            format!("lute compile: cannot parse {}", file.display()),
        ));
    };
    built.report_project_diags();
    let BuiltInput {
        input,
        resolve_error,
        identity,
        identity_rename_decls,
        identity_rename_diags,
        ..
    } = built;
    // plugin 0.0.2 §2: an `E-` capability-resolution diagnostic (bad plugin
    // option, missing active plugin, bad identity template) is a build-failing
    // error; it printed above, and it MUST gate here or it would pass silently.
    if resolve_error {
        return ExitCode::from(1);
    }
    // Project-aware gate (connectivity spec §5): the target compiles against
    // its RECONCILED project verdict (an envelope-Guaranteed `run.*`/`user.*`
    // read no longer blocks; a read no route guarantees blocks with
    // `E-STATE-MAYBE-UNAVAILABLE`). The project is the explicit `--project`
    // or the discovered nearest manifest — the two mean the same (0.48.0
    // §2.1); with no project at all, the standalone single-file `check` gate.
    //
    // 0.8.0 §9: the `identity:` block templates `lineId`/`voiceKey`. It is a
    // PROJECT setting, taken from the same manifest load as the snapshot: a
    // loose scene keeps `IdentityTemplates::default()`, i.e. 0.7.0's pair,
    // and so does a project that failed to load (its error already printed).
    let gate = match resolved.as_deref() {
        Some(dir) => match project_gate_result(file, dir, providers, permission_profile) {
            Ok(gate) => gate,
            Err(code) => return code,
        },
        None => check(&input),
    };

    // A component is not a root document (see [`component_root_diag`]): there is
    // no standalone execution IR to emit. Refused AFTER the gate, so a component
    // with real check errors still reports them. A diagnostic the policy
    // promotes (`--deny`, a project `error` level) fails the gate like an
    // error (dsl 0.50.0 §6.3).
    let compiled = if gate.ok && policy.any_promoted(file, &gate.diagnostics) {
        Err(gate.diagnostics)
    } else {
        match component_name_of(file).filter(|_| gate.ok) {
            Some((component, at)) => Err(vec![component_root_diag(&component, at)]),
            None => lute_compile::compile_with_check(&input, gate, &identity),
        }
    };
    match compiled {
        Ok(mut artifact) => {
            if let Err(code) = stamp_project_identity_renames(
                &mut artifact,
                resolved.as_deref(),
                providers,
                permission_profile,
                &identity_rename_decls,
                &identity_rename_diags,
                policy,
            ) {
                return code;
            }
            // dsl 0.8.0 §7: merge strictly downstream of the addressing pass —
            // `compile_with_check` has already stamped every final `lineId`,
            // which is the ONLY key a bundle joins on.
            if let Some(bundle) = &bundle {
                let merged = lute_compile::locale::merge_locales(&mut artifact, bundle);
                let refused = merged.iter().any(|d| d.severity == Severity::Error);
                // dsl 0.37.0 §6: a refused translation (`E-L10N-MODIFIERS`)
                // fails the build outright — a failed gate, so `--json`
                // prints the merge diagnostics as JSON on stdout, exactly as
                // a failed check gate does below.
                if refused && json {
                    let payload = serde_json::json!({ "diagnostics": merged });
                    let report = envelope::report(envelope::version::COMPILE, 1, payload);
                    let mut s = match pretty_json(&report) {
                        Ok(s) => s,
                        Err(error) => {
                            eprintln!("lute: failed to serialize diagnostics: {error}");
                            return ExitCode::from(2);
                        }
                    };
                    s.push('\n');
                    if write_stdout(&s).is_err() {
                        return envelope::fail_quietly(CliError::io_without_path(
                            "lute compile: cannot write stdout",
                        ));
                    }
                    return ExitCode::FAILURE;
                }
                // STDERR, not stdout: without `-o` the execution IR itself is on
                // stdout, and a warning line in the middle of it would make
                // the compile output unparseable.
                eprint!("{}", render_diagnostics(file, &merged, policy));
                if refused {
                    eprintln!("locale merge failed; no execution IR emitted");
                    return ExitCode::FAILURE;
                }
                let denied = merged.iter().filter(|d| policy.promoted(d, file)).count();
                if denied > 0 {
                    eprintln!("--deny promoted {denied} diagnostic(s); no execution IR emitted");
                    return ExitCode::FAILURE;
                }
            }
            let mut s = match pretty_json(&artifact) {
                Ok(s) => s,
                Err(error) => {
                    eprintln!("lute: failed to serialize execution IR: {error}");
                    return ExitCode::from(2);
                }
            };
            s.push('\n');
            match out {
                Some(path) => {
                    if let Err(e) = std::fs::write(path, &s) {
                        let e = lute_manifest::io_reason(&e);
                        return envelope::fail(CliError::io(
                            path,
                            format!("lute: cannot write {}: {e}", path.display()),
                        ));
                    }
                }
                None => {
                    if write_stdout(&s).is_err() {
                        return envelope::fail_quietly(CliError::io_without_path(
                            "lute compile: cannot write stdout",
                        ));
                    }
                }
            }
            ExitCode::SUCCESS
        }
        Err(diags) => {
            let s = if json {
                let payload = match diagnostics_json(&diags, file, policy) {
                    Ok((diagnostics, suppressed)) => {
                        let mut payload = serde_json::json!({ "diagnostics": diagnostics });
                        insert_suppressed(&mut payload, &suppressed);
                        payload
                    }
                    Err(error) => {
                        eprintln!("lute: failed to serialize diagnostics: {error}");
                        return ExitCode::from(2);
                    }
                };
                let wrapped = envelope::report(envelope::version::COMPILE, 1, payload);
                let mut s = match pretty_json(&wrapped) {
                    Ok(s) => s,
                    Err(error) => {
                        eprintln!("lute: failed to serialize diagnostics: {error}");
                        return ExitCode::from(2);
                    }
                };
                s.push('\n');
                s
            } else {
                let mut s = String::new();
                let mut errors = 0usize;
                for d in &diags {
                    let Some(reported) = policy.report(d, file) else {
                        continue;
                    };
                    errors += usize::from(reported.severity == Severity::Error);
                    let _ = writeln!(
                        s,
                        "{}:{}:{}: {} [{}]{} {}",
                        file.display(),
                        d.span.line,
                        d.span.column,
                        severity_str(reported.severity),
                        d.code,
                        marker_str(reported.marker),
                        d.text()
                    );
                }
                let suppressed =
                    suppressed_tail(&policy.suppressed(diags.iter().map(|d| (file, d))));
                let _ = writeln!(s, "{errors} error(s){suppressed}; no execution IR emitted");
                s
            };
            if write_stdout(&s).is_err() {
                return envelope::fail_quietly(CliError::io_without_path(
                    "lute compile: cannot write stdout",
                ));
            }
            ExitCode::FAILURE
        }
    }
}

/// Resolve and stamp a project's expanded identity ledger only when the
/// manifest actually declares one. The project model owns graph resolution;
/// this path merely applies its already-resolved ledger to the standalone
/// artifact.
fn stamp_project_identity_renames(
    artifact: &mut lute_compile::ExecutionIr,
    project: Option<&Path>,
    providers: Option<&Path>,
    permission_profile: Option<&str>,
    declarations: &[lute_manifest::project::IdentityRenameDecl],
    manifest_diags: &[lute_manifest::project::ResolveDiag],
    policy: &SeverityPolicy,
) -> Result<(), ExitCode> {
    if declarations.is_empty() && manifest_diags.is_empty() {
        return Ok(());
    }
    let Some(root) = project else {
        return Ok(());
    };
    let opts = ModelOptions {
        providers: providers.map(Path::to_path_buf),
        permission_profile: permission_profile.map(str::to_owned),
        mode: lute_check::Mode::Ci,
        compile: true,
        wip: false,
    };
    let model = match ProjectModel::build_single_root(root, &opts) {
        Ok(model) => model,
        Err(ModelError::Compile { path, diagnostics }) => {
            eprint!("{}", render_diagnostics(&path, &diagnostics, policy));
            return Err(ExitCode::FAILURE);
        }
        Err(error) => {
            eprintln!(
                "lute: cannot resolve identity ledger for {}: {error}",
                root.display()
            );
            return Err(ExitCode::from(2));
        }
    };
    if model.identity_renames().is_empty() {
        let mut emitted = false;
        for (path, diagnostic) in model.project_diagnostics() {
            if diagnostic.code.starts_with("E-RENAME-") {
                eprint!(
                    "{}",
                    render_diagnostics(path, std::slice::from_ref(diagnostic), policy)
                );
                emitted = true;
            }
        }
        if !emitted {
            eprintln!("lute: project identity ledger did not resolve");
        }
        return Err(ExitCode::FAILURE);
    }
    lute_compile::stamp_identity_renames(artifact, model.identity_renames());
    Ok(())
}

/// Read + parse a `--locale-bundle <bundle.json>` file (dsl 0.8.0 §7). A missing or
/// unreadable file is I/O (`2`, matching every other file the CLI opens); a
/// file that IS readable but is not a bundle is `E-LOCALE-BUNDLE` (`1`) — the
/// same code `lute loc import` reports for a malformed input, so the two ends
/// of the round trip name the same defect the same way.
fn load_locale_bundle(path: &Path) -> Result<lute_compile::locale::LocaleBundle, ExitCode> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        let e = lute_manifest::io_reason(&e);
        envelope::fail(CliError::io(
            path,
            format!("lute: cannot read {}: {e}", path.display()),
        ))
    })?;
    lute_compile::locale::LocaleBundle::parse(&text).map_err(|msg| {
        eprintln!("{}: error [{}] {msg}", path.display(), loc::E_LOCALE_BUNDLE);
        ExitCode::FAILURE
    })
}
