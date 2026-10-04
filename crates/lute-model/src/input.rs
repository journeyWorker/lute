//! Shared source input assembly for CLI, LSP, and project models.

use std::path::Path;

use lute_check::rel_schema::PluginOrigins;
use lute_check::{CheckInput, Mode};
use lute_core_span::Diagnostic;
use lute_manifest::project::{resolve_permissions, ResolveDiag};
use lute_manifest::provider::ProviderSet;
use lute_manifest::snapshot::CapabilitySnapshot;

use crate::cache::InputCache;

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
        for m in &self.project_diags { eprintln!("{}", project_diag_line(m)); }
    }
}

pub fn project_diag_line(m: &str) -> String {
    let located = m.split_once(": ").and_then(|(code, rest)| {
        let (at, msg) = rest.split_once(": ")?;
        let mut parts = at.rsplitn(3, ':');
        let (col, line) = (parts.next()?, parts.next()?);
        let file = parts.next().filter(|f| !f.is_empty())?;
        let numeric = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
        (numeric(col) && numeric(line) && code.starts_with(['E', 'W']) && !code.contains(' ')).then(|| {
            let severity = if code.starts_with("W-") { "warning" } else { "error" };
            format!("{file}:{line}:{col}: {severity} [{code}] {msg}")
        })
    });
    located.unwrap_or_else(|| format!("lute: {m}"))
}

pub fn build_input(file: &Path, providers: Option<&Path>, project: Option<&Path>, permission_profile: Option<&str>) -> Option<BuiltInput> {
    build_input_with_mode(&InputCache::default(), file, providers, project, permission_profile, Mode::Ci)
}

pub fn build_input_with(cache: &InputCache, file: &Path, providers: Option<&Path>, project: Option<&Path>, permission_profile: Option<&str>) -> Option<BuiltInput> {
    build_input_with_mode(cache, file, providers, project, permission_profile, Mode::Ci)
}

pub fn build_input_with_mode(cache: &InputCache, file: &Path, providers: Option<&Path>, project: Option<&Path>, permission_profile: Option<&str>, mode: Mode) -> Option<BuiltInput> {
    match read_document(file) {
        Ok(text) => Some(assemble_input_with_mode(cache, file, text, providers, project, permission_profile, mode).0),
        Err(message) => { eprintln!("{message}"); None }
    }
}

pub fn read_document(file: &Path) -> Result<String, String> {
    std::fs::read_to_string(file).map_err(|e| format!("lute: cannot read {}: {}", file.display(), lute_manifest::io_reason(&e)))
}

pub fn assemble_input(cache: &InputCache, file: &Path, text: String, providers: Option<&Path>, project: Option<&Path>, permission_profile: Option<&str>) -> (BuiltInput, (lute_syntax::ast::Document, Vec<Diagnostic>)) {
    assemble_input_with_mode(cache, file, text, providers, project, permission_profile, Mode::Ci)
}

pub fn assemble_input_with_mode(cache: &InputCache, file: &Path, text: String, providers: Option<&Path>, project: Option<&Path>, permission_profile: Option<&str>, mode: Mode) -> (BuiltInput, (lute_syntax::ast::Document, Vec<Diagnostic>)) {
    let mut project_diags = Vec::new();
    let root = project;
    let loaded = root.map(|dir| cache.project(dir));
    let project = match loaded.as_deref() {
        Some(Ok(p)) => p.as_ref(),
        Some(Err(e)) => { project_diags.push(e.clone()); None }
        None => None,
    };
    let providers = ProviderSet::clone(&cache.providers(providers, root, project));
    let defaults = project.map(|p| p.defaults.clone()).unwrap_or_default();
    let mut parsed = lute_syntax::parse(&text);
    lute_check::meta::apply_quest_tier_default(&mut parsed.0, &defaults);
    let doc = &parsed.0;
    let (meta0, _) = lute_check::meta::parse_meta_kind_with_defaults(&doc.meta, &CapabilitySnapshot::default(), lute_check::meta::MetaKind::Scene, &defaults);
    let meta_span = doc.meta.span;
    let resolved = cache.snapshot(root, project, meta0.profile.as_deref(), &meta0.plugins);
    let (mut snapshot, mut rdiags) = (resolved.0.clone(), resolved.1.clone());
    snapshot.identity_require_stable =
        project.is_some_and(lute_manifest::project::ProjectConfig::identity_require_stable);
    if let Some(name) = permission_profile {
        match project.as_ref() {
            Some(config) => match resolve_permissions(config, name) {
                Ok(permissions) => snapshot.restrict_permissions(&permissions),
                Err(error) => rdiags.push(ResolveDiag { span: None, code: error.code().to_string(), message: error.to_string() }),
            },
            None => rdiags.push(ResolveDiag { span: None, code: "E-PERMISSION-PROFILE".to_string(), message: format!("`--permission-profile {name}` requires a loaded `lute.project.yaml` from `--project <DIR>`") }),
        }
    }
    let mut resolve_error = !project_diags.is_empty();
    let mut resolve_blocks = resolve_error;
    for d in &rdiags {
        project_diags.push(format!("{}: {}", d.code, lute_core_span::plain_message(&d.message)));
        resolve_error |= d.code.starts_with("E-");
        resolve_blocks |= d.stops_checking();
    }
    let identity = project.as_ref().map(|p| p.identity.clone()).unwrap_or_default();
    let base = file.parent().unwrap_or_else(|| Path::new("."));
    let mut imports = cache.imports.resolve(base, &meta0.uses, &meta0.extends, meta_span);
    if let Some(p) = project { imports.plugin_origins = PluginOrigins::clone(&cache.plugin_origins(&p.plugins_dir)); }
    let components = cache.imports.resolve_components(base, &meta0.components, meta_span);
    let built = BuiltInput {
        input: CheckInput { text, uri: file.display().to_string(), snapshot, providers, mode, imports, components, defaults: defaults.clone() },
        resolve_error, resolve_blocks, project_diags, resolve_diags: rdiags, meta: meta0, defaults, identity,
        identity_rename_decls: project.map(|p| p.identity_renames.clone()).unwrap_or_default(),
        identity_rename_diags: project.map(|p| p.identity_rename_diags.clone()).unwrap_or_default(),
    };
    (built, parsed)
}
