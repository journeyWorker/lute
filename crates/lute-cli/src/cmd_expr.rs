//! `lute expr <CEL> [--project <dir>] [--json]` (0.41.0 §3.6): lower a host
//! condition to an IR `exprNode` against a project, with checker
//! diagnostics, so a host passes the result to the runtime's `evaluate`
//! query. The runtime stays parser-free: hosts lower at build time.
//!
//! The condition is checked as the `when:` of a scene document at the
//! project root whose frontmatter declares nothing but `uses:` of every
//! schema file the project's documents import, resolved against the
//! project's plugin set, with the project index `defs` (parameterised defs
//! included) as its def table — the union vocabulary a host evaluates
//! against through the bundle.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lute_core_span::{Diagnostic, Layer, Severity, Span, TextIndex};
use lute_load::{assemble_input, find_lute_files, read_document, BuiltInput, InputCache};
use serde::Serialize;

use crate::envelope::{self, CliError};
use crate::output::{diagnostics_json, insert_suppressed};
use crate::{render_diagnostics, write_stdout, SeverityPolicy};

/// The name diagnostics of the condition are rendered under.
const EXPR_LABEL: &str = "<expr>";

/// The synthetic root document the condition is checked in. Never read or
/// written: its text is built here.
const SYNTHETIC_DOCUMENT: &str = "lute-expr.lute";

/// The `--json` report: on success the index `celSlot` fields (`cel`, the
/// `@def`-expanded text; `authored`, the input when expansion changed it;
/// `expr`, the lowered node), on failure only `ok` and `diagnostics`.
#[derive(Serialize)]
struct Report<'a> {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    cel: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    authored: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expr: Option<&'a lute_ir::ExprNode>,
    diagnostics: &'a [Diagnostic],
}

/// What one `lute expr` run was asked (see [`crate::Command::Expr`]).
pub(crate) struct ExprOptions<'a> {
    pub(crate) cel: &'a str,
    pub(crate) project: Option<&'a Path>,
    pub(crate) permission_profile: Option<&'a str>,
    pub(crate) policy: &'a SeverityPolicy,
    pub(crate) json: bool,
}

/// Run `lute expr`. Exit `0` lowered, `1` a diagnostic error (or a
/// `--deny`-promoted one), `2` usage or I/O failure.
pub(crate) fn run_expr(opts: &ExprOptions<'_>) -> ExitCode {
    let ExprOptions {
        cel: raw,
        permission_profile,
        policy: gate_policy,
        json,
        ..
    } = *opts;
    // `--project`, else the nearest `lute.project.yaml` at or above the
    // current directory (0.48.0 §2.1).
    let project = match crate::project::require_project("expr", opts.project) {
        Ok(project) => project,
        Err(error) => return envelope::fail(error),
    };
    let project = project.as_path();
    // dsl 0.50.0 §6.2: the expression has no document path, so only a
    // project level without `paths` applies to its diagnostics.
    let policy = &gate_policy.for_project(Some(project));
    if !project.join("lute.project.yaml").is_file() {
        let message = format!("lute expr: no lute.project.yaml in {} — pass the project directory with --project <DIR>", project.display());
        // A directory that does not exist cannot be read; an existing one
        // without a manifest is the wrong `--project`.
        let error = if project.is_dir() {
            envelope::CliError::usage(message).with_argument("--project")
        } else {
            envelope::CliError::io(project, message)
        };
        return envelope::fail(error);
    }
    let cache = InputCache::default();
    let documents = match project_documents(&cache, project, permission_profile) {
        Ok(documents) => documents,
        Err(message) => return envelope::fail(envelope::CliError::io(project, message)),
    };
    let index_defs = crate::bundle::index_defs(documents.iter().map(|(_, built)| &built.input));
    let text = synthetic_document(project, &documents);
    let (built, _) = assemble_input(
        &cache,
        &project.join(SYNTHETIC_DOCUMENT),
        text,
        None,
        Some(project),
        permission_profile,
    );
    // The project's own resolution problems, and a schema import some
    // document names that does not resolve: in `--json` they are the
    // report's first diagnostics (stdout is self-contained); otherwise the
    // project's print as every command prints them.
    let mut diagnostics = if json {
        project_diagnostics(&cache, project, &built)
    } else {
        built.report_project_diags();
        Vec::new()
    };
    diagnostics.extend(import_failures(&built));
    if built.resolve_error || diagnostics.iter().any(|d| d.severity == Severity::Error) {
        return finish(json, None, raw, &diagnostics, policy);
    }
    let bodies: BTreeMap<String, String> = index_defs
        .iter()
        .map(|(name, def)| (name.clone(), def.cel.clone()))
        .collect();
    let params: BTreeMap<String, Vec<(String, lute_manifest::types::Type)>> = index_defs
        .iter()
        .map(|(name, def)| {
            let params = def
                .params
                .iter()
                .map(|p| (p.name.clone(), p.ty.clone()))
                .collect();
            (name.clone(), params)
        })
        .collect();
    let defs = lute_check::DefTable {
        bodies: &bodies,
        params: &params,
    };
    let checked = lute_check::check_host_condition(&built.input, &defs, raw);
    diagnostics.extend(checked.diagnostics);
    let lowered = checked
        .cel
        .and_then(|cel| match lute_compile::expr::lower_expr(&cel) {
            Some(expr) => Some((cel, expr)),
            None => {
                diagnostics.push(unlowered(raw, &cel));
                None
            }
        });
    // `--deny` / `--deny-warnings`: a promoted diagnostic fails the lowering
    // like an error, so nothing is lowered.
    let lowered = lowered.filter(|_| !policy.any_promoted(Path::new(EXPR_LABEL), &diagnostics));
    finish(json, lowered.as_ref(), raw, &diagnostics, policy)
}

/// Print the outcome and pick the exit code.
fn finish(
    json: bool,
    lowered: Option<&(String, lute_ir::ExprNode)>,
    raw: &str,
    diagnostics: &[Diagnostic],
    policy: &SeverityPolicy,
) -> ExitCode {
    let report = Report {
        ok: lowered.is_some(),
        cel: lowered.map(|(cel, _)| cel.as_str()),
        authored: lowered.filter(|(cel, _)| cel != raw).map(|_| raw),
        expr: lowered.map(|(_, expr)| expr),
        diagnostics,
    };
    let stdout = if json {
        let mut payload = match serde_json::to_value(&report) {
            Ok(value) => value,
            Err(e) => {
                eprintln!("lute expr: failed to serialize the result: {e}");
                return ExitCode::from(2);
            }
        };
        let reported = diagnostics_json(diagnostics, Path::new(EXPR_LABEL), policy);
        let (items, suppressed) = match reported {
            Ok(v) => v,
            Err(e) => {
                eprintln!("lute expr: failed to serialize the result: {e}");
                return ExitCode::from(2);
            }
        };
        if let serde_json::Value::Object(map) = &mut payload {
            map.insert("diagnostics".into(), items.into());
        }
        insert_suppressed(&mut payload, &suppressed);
        serde_json::to_string(&envelope::report(
            envelope::version::EXPR,
            if report.ok { 0 } else { 1 },
            payload,
        ))
        .map(Some)
    } else {
        eprint!(
            "{}",
            render_diagnostics(Path::new(EXPR_LABEL), diagnostics, policy)
        );
        report.expr.map(serde_json::to_string).transpose()
    };
    let stdout = match stdout {
        Ok(stdout) => stdout,
        Err(e) => {
            eprintln!("lute expr: failed to serialize the result: {e}");
            return ExitCode::from(2);
        }
    };
    if let Some(mut line) = stdout {
        line.push('\n');
        if write_stdout(&line).is_err() {
            return envelope::fail_quietly(CliError::io_without_path(
                "lute expr: cannot write stdout",
            ));
        }
    }
    if report.ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Every non-component document of the project, assembled as whole-project `compile`
/// assembles it, in source-path order.
fn project_documents(
    cache: &InputCache,
    project: &Path,
    permission_profile: Option<&str>,
) -> Result<Vec<(PathBuf, BuiltInput)>, String> {
    let mut files = find_lute_files(project).map_err(|e| {
        format!(
            "lute expr: cannot walk {}: {}",
            project.display(),
            lute_manifest::io_reason(&e)
        )
    })?;
    files.retain(|path| !crate::compile_all::is_component_file(path));
    files.sort();
    files
        .into_iter()
        .map(|path| {
            let text = read_document(&path).map_err(|e| e.to_string())?;
            let (built, _) =
                assemble_input(cache, &path, text, None, Some(project), permission_profile);
            Ok((path, built))
        })
        .collect()
}

/// The root scene the condition is checked in: `uses:` of every schema file
/// the documents import (their own `uses:`/`extends:`, the manifest's
/// `defaults: uses:` included), each relative to the project root when it
/// lies under it. An import that does not resolve is kept, so the root
/// reports it ([`import_failures`]) rather than checking against less.
fn synthetic_document(project: &Path, documents: &[(PathBuf, BuiltInput)]) -> String {
    let root = std::fs::canonicalize(project).unwrap_or_else(|_| project.to_path_buf());
    let mut schemas = BTreeSet::new();
    for (path, built) in documents {
        let base = path.parent().unwrap_or(project);
        let base = std::fs::canonicalize(base).unwrap_or_else(|_| base.to_path_buf());
        for import in built.meta.uses.iter().chain(&built.meta.extends) {
            let target = base.join(import);
            schemas.insert(std::fs::canonicalize(&target).unwrap_or_else(|_| lexical(&target)));
        }
    }
    let uses: Vec<String> = schemas
        .iter()
        .map(|file| match file.strip_prefix(&root) {
            Ok(rel) => rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/"),
            Err(_) => file.display().to_string(),
        })
        .collect();
    // A JSON array of strings is a YAML flow sequence.
    let uses = serde_json::to_string(&uses).unwrap_or_else(|_| "[]".to_string());
    format!("---\nkind: scene\nuses: {uses}\n---\n")
}

/// `path` with its `.` and `..` components folded away, without touching
/// the filesystem (the file it names does not exist).
fn lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// The synthetic root's project resolution problems as diagnostics (for
/// `--json`): a manifest that does not load, then each capability
/// resolution diagnostic. They have no place in the expression.
fn project_diagnostics(cache: &InputCache, project: &Path, built: &BuiltInput) -> Vec<Diagnostic> {
    use lute_manifest::project::{ProjectLoadError, E_MANIFEST};
    let mut out = Vec::new();
    if let Err(error) = cache.project(project).as_ref() {
        let message = match error {
            ProjectLoadError::Io { .. } => error.to_string(),
            ProjectLoadError::Yaml {
                path,
                line,
                column,
                message,
                ..
            }
            | ProjectLoadError::Invalid {
                path,
                line,
                column,
                reason: message,
            } => {
                format!("{}:{line}:{column}: {message}", path.display())
            }
        };
        out.push(placeless(E_MANIFEST, Severity::Error, message));
    }
    for d in &built.resolve_diags {
        let severity = if d.code.starts_with("W-") {
            Severity::Warning
        } else {
            Severity::Error
        };
        let message = lute_core_span::plain_message(&d.message).into_owned();
        out.push(placeless(&d.code, severity, message));
    }
    lute_check::evidence::annotate(&mut out);
    out
}

/// A schema import of the synthetic root that does not resolve or parse —
/// one a project document names. Placeless: the root's text is not the
/// author's.
fn import_failures(built: &BuiltInput) -> Vec<Diagnostic> {
    let mut out: Vec<Diagnostic> = built
        .input
        .import_diags
        .iter()
        .filter(|d| matches!(d.code.as_str(), "E-USES-NOT-FOUND" | "E-USES-PARSE"))
        .map(|d| Diagnostic {
            span: NO_PLACE,
            fixits: Vec::new(),
            ..d.clone()
        })
        .collect();
    lute_check::evidence::annotate(&mut out);
    out
}

/// The span of a diagnostic with no place in the expression: rendered as
/// `<expr>: …`, `line` 0 in JSON.
const NO_PLACE: Span = Span {
    byte_start: 0,
    byte_end: 0,
    line: 0,
    column: 0,
    utf16_range: (0, 0),
};

fn placeless(code: &str, severity: Severity, message: String) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity,
        message,
        evidence: None,
        span: NO_PLACE,
        layer: Layer::Logic,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

/// 0.41.0 §3.6: a condition that checks but has no `exprNode` form.
fn unlowered(raw: &str, cel: &str) -> Diagnostic {
    let mut diagnostic = Diagnostic {
        code: lute_check::E_CEL_PROFILE.to_string(),
        severity: Severity::Error,
        message: format!(
            "`{}` has no IR expression form: a host condition must lower to an `exprNode` the \
             runtime evaluates, and this one uses a construct outside the lowered Lute-CEL profile",
            cel.trim()
        ),
        evidence: None,
        span: Span::from_bytes(&TextIndex::new(raw), 0, raw.len()),
        layer: Layer::Cel,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    };
    lute_check::evidence::annotate(std::slice::from_mut(&mut diagnostic));
    diagnostic
}
