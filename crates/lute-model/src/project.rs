use std::collections::{BTreeMap, BTreeSet};
use std::borrow::Cow;
use std::fmt;
use std::path::{Path, PathBuf};
fn annotate_diagnostic(diagnostic: &mut Diagnostic) {
    lute_check::evidence::annotate(std::slice::from_mut(diagnostic));
    for related in &mut diagnostic.related {
        annotate_diagnostic(&mut related.diagnostic);
    }
}

use lute_check::{fold_env, CheckInput, CheckResult, FoldedEnv, Mode};
use lute_compile::index::{build_index, IndexInput, ProjectIndex};
use lute_compile::{compile_mapped, ExecutionIr, SourceMap};
use lute_core_span::{Diagnostic, Layer, Severity, Span, TextIndex};
use rayon::prelude::*;

use crate::cache::InputCache;
use crate::input::{assemble_input_with_mode, read_document};

pub type DocGroup = Vec<(PathBuf, lute_syntax::ast::Document, FoldedEnv)>;
pub type ByRoot = BTreeMap<PathBuf, DocGroup>;

#[derive(Clone, Debug)]
pub struct ModelOptions {
    pub providers: Option<PathBuf>,
    pub permission_profile: Option<String>,
    pub mode: Mode,
    pub compile: bool,
    pub wip: bool,
}

impl Default for ModelOptions {
    fn default() -> Self {
        Self { providers: None, permission_profile: None, mode: Mode::Ci, compile: true, wip: false }
    }
}

#[derive(Debug)]
pub enum ModelError {
    Io(String),
    Input(String),
    Resolve(String),
    Compile { path: PathBuf, diagnostics: Vec<Diagnostic> },
    Index(Vec<lute_compile::index::IndexError>),
}

impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(s) | Self::Input(s) | Self::Resolve(s) => f.write_str(s),
            Self::Compile { path, diagnostics } => write!(f, "cannot compile {} ({} diagnostics)", path.display(), diagnostics.len()),
            Self::Index(errors) => write!(f, "cannot build project index ({} conflicts)", errors.len()),
        }
    }
}
impl std::error::Error for ModelError {}

pub struct ModelDocument {
    pub path: PathBuf,
    pub input: CheckInput,
    pub doc: lute_syntax::ast::Document,
    pub folded: FoldedEnv,
    pub check: CheckResult,
    pub artifact: Option<ExecutionIr>,
    pub source_map: Option<SourceMap>,
    pub resolve_error: bool,
    pub resolve_blocks: bool,
    pub resolve_diags: Vec<lute_manifest::project::ResolveDiag>,
    pub project_diags: Vec<String>,
}

pub struct ReconciledOutputs {
    pub checks: Vec<(PathBuf, CheckResult)>,
    pub diagnostics: Vec<(PathBuf, Diagnostic)>,
    pub nodes_by_path: BTreeMap<PathBuf, Vec<(lute_check::connectivity::NodeId, Span)>>,
    pub fact_envs: BTreeMap<PathBuf, lute_check::FactEnv>,
    pub scenarios: BTreeMap<PathBuf, crate::scenario::RootScenario>,
}

pub struct ProjectModel {
    root: PathBuf,
    manifest: Option<lute_manifest::project::ProjectConfig>,
    documents: Vec<ModelDocument>,
    project_diagnostics: Vec<(PathBuf, Diagnostic)>,
    index: Option<ProjectIndex>,
    reconciled: ReconciledOutputs,
}

impl ProjectModel {
    pub fn build(root: &Path, opts: &ModelOptions) -> Result<Self, ModelError> {
        Self::build_impl(root, opts, false)
    }

    /// Build one explicit project root without discovering nested roots.
    /// Capabilities and connectivity resolve against this root for every
    /// document under it, matching `--project <dir>` semantics.
    pub fn build_single_root(root: &Path, opts: &ModelOptions) -> Result<Self, ModelError> {
        Self::build_impl(root, opts, true)
    }

    fn build_impl(root: &Path, opts: &ModelOptions, own_root_only: bool) -> Result<Self, ModelError> {
        let root = root.to_path_buf();
        let files = find_lute_files(&root).map_err(|e| ModelError::Io(format!("lute: cannot walk {}: {}", root.display(), lute_manifest::io_reason(&e))))?;
        let files: Vec<PathBuf> = if own_root_only {
            files.into_iter().filter(|p| project_root_for(p, &root) == root).collect()
        } else {
            files
        };
        let cache = InputCache::default();
        let providers = opts.providers.as_deref();
        let permission_profile = opts.permission_profile.as_deref();
        let mut documents: Vec<ModelDocument> = files
            .into_par_iter()
            .map(|path| {
            let text = read_document(&path).map_err(ModelError::Input)?;
            let (built, parsed) = assemble_input_with_mode(&cache, &path, text, providers, Some(&root), permission_profile, opts.mode);
            let resolve_error = built.resolve_error;
            let resolve_blocks = built.resolve_blocks;
            let resolve_diags = built.resolve_diags.clone();
            let project_diags = built.project_diags.clone();
            let input = built.input;
            let mut doc = parsed.0.clone();
            let _ = lute_check::desugar_document(&mut doc, &input);
            lute_check::splice_component_effects(&mut doc, &input.components, &input.snapshot);
            let (folded, _, _) = fold_env(&doc, &input);
            let check = lute_check::check_parsed(&input, parsed);
            Ok(ModelDocument {
                path,
                input,
                doc,
                folded,
                check,
                artifact: None,
                source_map: None,
                resolve_error,
                resolve_blocks,
                resolve_diags,
                project_diags,
            })
            })
            .collect::<Result<Vec<_>, ModelError>>()?;
        let initial_checks: Vec<_> = documents.iter().map(|d| (d.path.clone(), d.check.clone())).collect();
        let group: DocGroup = documents.iter().map(|d| (d.path.clone(), d.doc.clone(), d.folded.clone())).collect();
        let mut by_root = ByRoot::new();
        by_root.insert(root.clone(), group);
        let scenarios = by_root.iter().map(|(root, group)| {
            (root.clone(), crate::scenario::assemble_root_scenario(group, &initial_checks))
        }).collect();
        let (mut checks, mut project_diagnostics, nodes_by_path, fact_envs) =
            crate::reconcile::reconcile_collected(initial_checks, &by_root, opts.wip);
        crate::reconcile::rollup_component_body_diags(&mut checks);
        let mut gate_checks = checks.clone();
        let mut gate_project_diagnostics = project_diagnostics.clone();
        crate::reconcile::relocate_imported_diags(
            &mut gate_checks,
            &mut gate_project_diagnostics,
            &root,
        );
        let project_blocked = gate_project_diagnostics
            .iter()
            .any(|(_, diagnostic)| diagnostic.severity == Severity::Error);
        for (path, check) in &checks {
            if let Some(document) = documents.iter_mut().find(|document| document.path == *path) {
                document.check = check.clone();
            }
        }
        if opts.compile {
            let loaded = cache.project(&root);
            let identity = loaded.as_ref().as_ref().ok().and_then(|p| p.as_ref()).map(|p| p.identity.clone()).unwrap_or_default();
            for document in &mut documents {
                let Some(raw_check) = checks
                    .iter()
                    .find(|(path, _)| path == &document.path)
                    .map(|(_, check)| check)
                else {
                    continue;
                };
                if project_blocked || !raw_check.ok || document.resolve_error || document.folded.typed.component.is_some() {
                    continue;
                }
                let gate_diags = compile_gate_diags(&document.input);
                if !gate_diags.is_empty() {
                    document.check.diagnostics.extend(gate_diags);
                    document.check.ok = false;
                    continue;
                }
                let mut gate = raw_check.clone();
                gate.diagnostics.extend(project_diagnostics.iter().filter(|(path, _)| path == &document.path).map(|(_, d)| d.clone()));
                gate.ok = !gate.diagnostics.iter().any(|d| d.severity == Severity::Error);
                if !gate.ok { continue; }
                match compile_mapped(&document.input, gate, &identity) {
                    Ok((artifact, source_map)) => {
                        document.artifact = Some(artifact);
                        document.source_map = Some(source_map);
                    }
                    Err(diagnostics) => {
                        document.check.diagnostics.extend(diagnostics);
                        document.check.ok = false;
                    }
                }
            }
        }
        let index = if opts.compile {
            let inputs: Vec<IndexInput<'_>> = documents
                .iter()
                .filter_map(|d| {
                    let artifact = d.artifact.as_ref()?;
                    Some(IndexInput {
                        path: relative_path(&root, &d.path),
                        artifact_path: relative_path(&root, &d.path),
                        artifact,
                    })
                })
                .collect();
            if inputs.is_empty() {
                None
            } else {
                match build_index(lute_compile::LUTE_IR_VERSION, &inputs) {
                    Ok(index) => Some(index),
                    Err(errors) => {
                        for error in errors {
                            let code = match error {
                                lute_compile::index::IndexError::CapabilityMismatch { .. } => {
                                    "E-CAPABILITY-MISMATCH"
                                }
                                lute_compile::index::IndexError::Conflict { .. } => {
                                    "E-USES-DUP-RELATION"
                                }
                            };
                            project_diagnostics.push((
                                root.join("lute.project.yaml"),
                                resolution_diagnostic(&format!("{code}: {error}")),
                            ));
                        }
                        None
                    }
                }
            }
        } else {
            None
        };
        let manifest = lute_manifest::project::load_project(&root).ok().flatten();
        if let Some(config) = manifest.as_ref() {
            let manifest_path = root.join("lute.project.yaml");
            let text = std::fs::read_to_string(&manifest_path).unwrap_or_default();
            let idx = lute_core_span::TextIndex::new(&text);
            for item in &config.constraint_diags {
                let mut diagnostic = resolution_diagnostic(&format!("{}: {}", item.code, item.message));
                if let Some(span) = &item.span {
                    diagnostic.span = lute_core_span::Span::from_bytes(&idx, span.start, span.end);
                }
                project_diagnostics.push((manifest_path.clone(), diagnostic));
            }
        }
        for document in &mut documents {
            for diagnostic in &mut document.check.diagnostics {
                annotate_diagnostic(diagnostic);
            }
        }
        for (_, diagnostic) in &mut project_diagnostics {
            annotate_diagnostic(diagnostic);
        }
        let checks = documents.iter().map(|d| (d.path.clone(), d.check.clone())).collect();
        let reconciled = ReconciledOutputs {
            checks,
            diagnostics: project_diagnostics.clone(),
            nodes_by_path,
            fact_envs,
            scenarios,
        };
        Ok(Self { root, manifest, documents, project_diagnostics, index, reconciled })
    }
    pub fn has_resolution_errors(&self) -> bool {
        self.documents.iter().any(|document| document.resolve_error)
    }

    pub fn roots_under(dir: &Path, opts: &ModelOptions) -> Result<Vec<ProjectModel>, ModelError> {
        let files = find_lute_files(dir).map_err(|e| ModelError::Io(format!("lute: cannot walk {}: {}", dir.display(), lute_manifest::io_reason(&e))))?;
        let mut roots = BTreeSet::new();
        for file in files { roots.insert(project_root_for(&file, dir)); }
        roots.into_iter().map(|root| Self::build_single_root(root.as_path(), opts)).collect()
    }

    pub fn root(&self) -> &Path { &self.root }
    pub fn manifest(&self) -> Option<&lute_manifest::project::ProjectConfig> { self.manifest.as_ref() }
    pub fn documents(&self) -> &[ModelDocument] { &self.documents }
    pub fn project_diagnostics(&self) -> &[(PathBuf, Diagnostic)] { &self.project_diagnostics }
    pub fn index(&self) -> Option<&ProjectIndex> { self.index.as_ref() }
    pub fn reconciled(&self) -> &ReconciledOutputs { &self.reconciled }
}

fn resolution_diagnostic(raw: &str) -> Diagnostic {
    let (code, message) = raw.split_once(": ").unwrap_or(("E-PROJECT-CONFIG", raw));
    Diagnostic {
        code: code.to_string(),
        severity: if code.starts_with("W-") { Severity::Warning } else { Severity::Error },
        message: message.to_string(),
        span: Span { byte_start: 0, byte_end: 0, line: 0, column: 0, utf16_range: (0, 0) },
        layer: Layer::Logic,
        evidence: None,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}
fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root).unwrap_or(path).to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/")
}

pub fn find_lute_files(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d)? {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir() { stack.push(path); }
            else if path.extension().and_then(|e| e.to_str()) == Some("lute") { out.push(path); }
        }
    }
    out.sort();
    let mut deduped = Vec::with_capacity(out.len());
    let mut seen = BTreeSet::new();
    for path in out {
        let canon = std::fs::canonicalize(&path)?;
        if seen.insert(canon) {
            deduped.push(path);
        }
    }
    Ok(deduped)
}

pub fn project_root_for(file: &Path, walk_root: &Path) -> PathBuf {
    let mut dir = file.parent().unwrap_or(walk_root);
    loop {
        if dir.join("lute.project.yaml").is_file() { return dir.to_path_buf(); }
        if dir == walk_root { return walk_root.to_path_buf(); }
        dir = match dir.parent() { Some(parent) => parent, None => return walk_root.to_path_buf() };
    }
}

fn compile_gate_diags(input: &CheckInput) -> Vec<Diagnostic> {
    let (mut doc, _) = lute_syntax::parse(&input.text);
    let mut arena = lute_cel::CelArena::default();
    let _ = lute_cel::fill_document(&mut arena, &mut doc);
    let (folded, _, _) = fold_env(&doc, input);
    let cast = lute_check::declared_cast(&input.snapshot, &input.imports, &folded.typed.cast);
    let mut diags = lute_compile::normalize::normalize_document(
        &mut doc,
        &input.components,
        &cast,
        &folded.env.domains,
        &folded.env.state,
        &folded.env.occasion_scopes,
    );
    let bodies = if folded.typed.component.is_some() {
        let mut bodies = folded.def_bodies.clone();
        for param in &folded.typed.params {
            bodies.entry(param.name.clone()).or_insert_with(|| param.name.clone());
        }
        Cow::Owned(bodies)
    } else {
        Cow::Borrowed(&folded.def_bodies)
    };
    let table = lute_check::DefTable { bodies: &bodies, params: &folded.env.def_params };
    diags.extend(lute_compile::expand::expand_document(&mut doc, &table));
    diags
}

pub fn parse_project_docs(dir: &Path, files: &[PathBuf]) -> Vec<std::io::Result<(lute_syntax::ast::Document, Vec<Diagnostic>)>> {
    let cache = InputCache::default();
    files.par_iter().map(|file| {
        let text = std::fs::read_to_string(file)?;
        let root = project_root_for(file, dir);
        let (built, (mut doc, diags)) = crate::input::assemble_input(&cache, file, text, None, Some(&root), None);
        let _ = lute_check::desugar_document(&mut doc, &built.input);
        Ok((doc, diags))
    }).collect()
}

pub fn normalize_span_from_text(text: &str, span: Span) -> Span {
    let len = text.len();
    let mut start = span.byte_start.min(len);
    let mut end = span.byte_end.min(len).max(start);
    while start > 0 && !text.is_char_boundary(start) { start -= 1; }
    while end < len && !text.is_char_boundary(end) { end += 1; }
    let idx = TextIndex::new(text);
    Span::from_bytes(&idx, start, end)
}

pub fn nearest_manifest_dir(file: &Path) -> Option<PathBuf> {
    let abs = std::fs::canonicalize(file).ok()?;
    let start = if abs.is_dir() { abs.as_path() } else { abs.parent()? };
    start.ancestors().find(|d| d.join("lute.project.yaml").is_file()).map(Path::to_path_buf)
}

pub fn discover_project(file: &Path, project: Option<&Path>) -> Option<PathBuf> {
    if project.is_some() { return None; }
    let dir = nearest_manifest_dir(file)?;
    eprintln!("lute: note: using project {} (nearest lute.project.yaml); pass --project to choose another", dir.display());
    Some(dir)
}
