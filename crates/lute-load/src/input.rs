//! Shared source input assembly for CLI, LSP, and project models.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use lute_check::{CheckInput, InputSources, Mode};
use lute_core_span::Diagnostic;
use lute_manifest::project::ResolveDiag;
use lute_manifest::snapshot::CapabilitySnapshot;

use crate::cache::InputCache;

#[derive(Debug)]
pub struct BuiltInput {
    pub input: CheckInput,
    pub resolve_error: bool,
    pub resolve_blocks: bool,
    pub project_diags: Vec<String>,
    pub resolve_diags: Vec<ResolveDiag>,
    pub meta: lute_check::TypedMeta,
    pub defaults: lute_manifest::project::MetaDefaults,
    pub identity: lute_manifest::project::IdentityTemplates,
    /// Authored project ledger declarations, used to cheaply decide whether a
    /// full project graph is needed to stamp a standalone artifact.
    pub identity_rename_decls: Vec<lute_manifest::project::IdentityRenameDecl>,
    /// Manifest-level ledger diagnostics that exist before graph validation.
    pub identity_rename_diags: Vec<ResolveDiag>,
}

impl BuiltInput {
    pub fn report_project_diags(&self) {
        for m in &self.project_diags {
            eprintln!("{}", project_diag_line(m));
        }
    }
}

pub fn project_diag_line(m: &str) -> String {
    let located = m.split_once(": ").and_then(|(code, rest)| {
        let (at, msg) = rest.split_once(": ")?;
        let mut parts = at.rsplitn(3, ':');
        let (col, line) = (parts.next()?, parts.next()?);
        let file = parts.next().filter(|f| !f.is_empty())?;
        let numeric = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
        (numeric(col) && numeric(line) && code.starts_with(['E', 'W']) && !code.contains(' ')).then(
            || {
                let severity = if code.starts_with("W-") {
                    "warning"
                } else {
                    "error"
                };
                format!("{file}:{line}:{col}: {severity} [{code}] {msg}")
            },
        )
    });
    located.unwrap_or_else(|| format!("lute: {m}"))
}

pub fn build_input(
    file: &Path,
    providers: Option<&Path>,
    project: Option<&Path>,
    permission_profile: Option<&str>,
) -> Option<BuiltInput> {
    build_input_with_mode(
        &InputCache::default(),
        file,
        providers,
        project,
        permission_profile,
        Mode::Ci,
    )
}

pub fn build_input_with(
    cache: &InputCache,
    file: &Path,
    providers: Option<&Path>,
    project: Option<&Path>,
    permission_profile: Option<&str>,
) -> Option<BuiltInput> {
    build_input_with_mode(
        cache,
        file,
        providers,
        project,
        permission_profile,
        Mode::Ci,
    )
}

pub fn build_input_with_mode(
    cache: &InputCache,
    file: &Path,
    providers: Option<&Path>,
    project: Option<&Path>,
    permission_profile: Option<&str>,
    mode: Mode,
) -> Option<BuiltInput> {
    match read_document(file) {
        Ok(text) => Some(
            assemble_input_with_mode(
                cache,
                file,
                text,
                providers,
                project,
                permission_profile,
                mode,
            )
            .0,
        ),
        Err(message) => {
            eprintln!("{message}");
            None
        }
    }
}

/// Read one source document.
///
/// # Errors
///
/// [`ReadError`]: the file cannot be read.
pub fn read_document(file: &Path) -> Result<String, ReadError> {
    std::fs::read_to_string(file).map_err(|source| ReadError {
        path: file.to_path_buf(),
        source,
    })
}

/// A source document that cannot be read. `Display` is the line the CLI
/// prints: `lute: cannot read <path>: <reason>`.
#[derive(Debug)]
pub struct ReadError {
    pub path: PathBuf,
    pub source: std::io::Error,
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "lute: cannot read {}: {}",
            self.path.display(),
            lute_manifest::io_reason(&self.source)
        )
    }
}

impl std::error::Error for ReadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

pub fn assemble_input(
    cache: &InputCache,
    file: &Path,
    text: String,
    providers: Option<&Path>,
    project: Option<&Path>,
    permission_profile: Option<&str>,
) -> (BuiltInput, (lute_syntax::ast::Document, Vec<Diagnostic>)) {
    assemble_input_with_mode(
        cache,
        file,
        text,
        providers,
        project,
        permission_profile,
        Mode::Ci,
    )
}

pub fn assemble_input_with_mode(
    cache: &InputCache,
    file: &Path,
    text: String,
    providers: Option<&Path>,
    project: Option<&Path>,
    permission_profile: Option<&str>,
    mode: Mode,
) -> (BuiltInput, (lute_syntax::ast::Document, Vec<Diagnostic>)) {
    let mut project_diags = Vec::new();
    let root = project;
    let loaded = root.map(|dir| cache.project(dir));
    let project = match loaded.as_deref() {
        Some(Ok(p)) => p.as_ref(),
        Some(Err(e)) => {
            project_diags.push(e.to_string());
            None
        }
        None => None,
    };
    let defaults = project.map(|p| p.defaults.clone()).unwrap_or_default();
    let mut parsed = lute_syntax::parse(&text);
    lute_check::meta::apply_quest_tier_default(&mut parsed.0, &defaults);
    let doc = &parsed.0;
    let (meta0, _) = lute_check::meta::parse_meta_kind_with_defaults(
        &doc.meta,
        &CapabilitySnapshot::default(),
        lute_check::meta::MetaKind::Scene,
        &defaults,
    );
    let meta_span = doc.meta.span;
    let resolved = cache.base(
        root,
        project,
        providers,
        meta0.profile.as_deref(),
        &meta0.plugins,
        permission_profile,
    );
    let rdiags = resolved.diags.clone();
    let mut resolve_error = !project_diags.is_empty();
    let mut resolve_blocks = resolve_error;
    for d in &rdiags {
        project_diags.push(format!(
            "{}: {}",
            d.code,
            lute_core_span::plain_message(&d.message)
        ));
        resolve_error |= d.code.starts_with("E-");
        resolve_blocks |= d.stops_checking();
    }
    let identity = project
        .as_ref()
        .map(|p| p.identity.clone())
        .unwrap_or_default();
    let dir = file.parent().unwrap_or_else(|| Path::new("."));
    let (vocab, import_diags) =
        cache
            .imports
            .vocab(&resolved.base, dir, &meta0.uses, &meta0.extends, meta_span);
    let (components, component_diags) =
        cache
            .imports
            .resolve_components(dir, &meta0.components, meta_span);
    let sources = InputSources {
        base: Arc::clone(&resolved.base),
        vocab,
        components,
        import_diags,
        component_diags,
    };
    let built = BuiltInput {
        input: CheckInput::new(text, file.display().to_string(), mode, sources),
        resolve_error,
        resolve_blocks,
        project_diags,
        resolve_diags: rdiags,
        meta: meta0,
        defaults,
        identity,
        identity_rename_decls: project
            .map(|p| p.identity_renames.clone())
            .unwrap_or_default(),
        identity_rename_diags: project
            .map(|p| p.identity_rename_diags.clone())
            .unwrap_or_default(),
    };
    (built, parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 0.49.1: an unreadable document renders the 0.49.0 line and keeps the
    /// I/O error as its `source()`.
    #[test]
    fn read_error_text_and_source() {
        let path = Path::new("no-such-dir-0491/missing.lute");
        let error = read_document(path).unwrap_err();
        assert_eq!(
            error.to_string(),
            format!(
                "lute: cannot read {}: {}",
                path.display(),
                lute_manifest::io_reason(&error.source)
            )
        );
        let source = std::error::Error::source(&error).expect("the I/O error");
        assert_eq!(
            source
                .downcast_ref::<std::io::Error>()
                .map(std::io::Error::kind),
            Some(std::io::ErrorKind::NotFound)
        );
    }
}
