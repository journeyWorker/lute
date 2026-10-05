use std::path::{Path, PathBuf};
use lute_check::Mode;
use lute_model::{assemble_input_with_mode, BuiltInput, InputCache};
use tower_lsp_server::ls_types::Uri;
use super::Backend;

/// The last-known text of an open document plus the LSP version that produced it.
/// Republished diagnostics are stamped with `version` so the client can discard
/// results for a superseded edit.
#[derive(Clone, Debug)]
pub struct DocumentSnapshot {
    /// Full document text (FULL sync: the whole buffer on every change).
    pub text: String,
    /// LSP document version (monotonic per the client; from didOpen/didChange).
    pub version: i32,
}

impl Backend {
    /// The open document whose file is `path` (canonical), as the client
    /// spells its URI — a `/tmp/…` buffer is `/private/tmp/…` canonically.
    pub(super) fn open_uri_of(&self, path: &Path) -> Option<Uri> {
        self.docs
            .iter()
            .map(|e| e.key().clone())
            .find(|u| canonical_path_of(u).as_deref() == Some(path))
    }
}

impl Backend {
    /// The current full text of the open document `uri`, or `None` if it is not
    /// open. Cloned so the feature call runs without holding the `DashMap` guard.
    pub(super) fn document_text(&self, uri: &Uri) -> Option<String> {
        self.docs.get(uri).map(|d| d.text.clone())
    }
    pub(super) fn assemble(
        &self,
        uri: &Uri,
        text: String,
    ) -> Option<(BuiltInput, lute_syntax::ast::Document)> {
        let path = uri_to_path(uri)?;
        let root = find_project_root(&path);
        let request_cache = InputCache::default();
        let (built, (doc, _)) = assemble_input_with_mode(
            &request_cache,
            &path,
            text,
            None,
            root.as_deref(),
            None,
            Mode::Author,
        );
        Some((built, doc))
    }
}

/// Resolve a document [`Uri`] to a filesystem [`PathBuf`]. Only a `file` URI maps
/// to a real path: [`Uri::to_file_path`] does NOT check the scheme (verified — it
/// returns `Some` for `untitled:`/`vscode-vfs:` too), so guard it explicitly. A
/// non-file (virtual/unsaved) document returns `None`, and snapshot resolution
/// falls back to core-only. Schemes are case-insensitive (RFC 3986 §3.1). Ownership
/// is taken (`Cow` → `PathBuf`) so the path outlives the borrow of `uri`.
pub(super) fn uri_to_path(uri: &Uri) -> Option<PathBuf> {
    if !uri.scheme().as_str().eq_ignore_ascii_case("file") {
        return None;
    }
    uri.to_file_path().map(|p| p.into_owned())
}

/// [`uri_to_path`], canonicalized — how the checker names an imported file
/// (`RelatedDiagnostic::file`); the lexical path when it does not exist.
pub(super) fn canonical_path_of(uri: &Uri) -> Option<PathBuf> {
    let path = uri_to_path(uri)?;
    Some(std::fs::canonicalize(&path).unwrap_or(path))
}

/// Walk up from the document at `file_path`, returning the first ancestor
/// directory that contains a `lute.project.yaml` (plugin §11 project discovery).
/// Starts at the file's parent directory and climbs to the filesystem root;
/// `None` when no project is found (a loose scene → core-only). Purely lexical
/// on the path components, so it works for buffers not yet written to disk.
pub(super) fn find_project_root(file_path: &Path) -> Option<PathBuf> {
    let mut dir = file_path.parent();
    while let Some(d) = dir {
        if d.join("lute.project.yaml").is_file() {
            return Some(d.to_path_buf());
        }
        dir = d.parent();
    }
    None
}

/// Whether `file_path` is a project declaration `.yaml`/`.yml` the LSP claims
/// for semantic linting (data-catalog foundation B3): a YAML file under the
/// discovered project's `schema/` or `catalog/` subdirectory, or one named
/// `*.schema.yaml`/`*.schema.yml` anywhere in it (`world.schema.yaml`, the
/// conventional `uses:` target). Returns the project root on a claim (the
/// caller needs it again for baseline resolution), `None` otherwise — so
/// `lute.project.yaml` itself, a play script or test, and any unrelated
/// `.yaml` (CI configs, ...) are never claimed; `.lute` handling is untouched
/// (this gate is checked ONLY when the extension is `.yaml`/`.yml`). Per the
/// design notes' "simplest robust rule" guidance: prefer the conventional
/// declaration dirs and name over parsing every scene's `uses:`/`extends:`
/// project-wide to find which files are import-reachable.
pub(super) fn claimed_declaration_yaml(file_path: &Path) -> Option<PathBuf> {
    if !is_yaml(file_path) {
        return None;
    }
    let root = find_project_root(file_path)?;
    let rel = file_path.strip_prefix(&root).ok()?;
    let first = rel.components().next()?;
    let in_dir = matches!(first.as_os_str().to_str(), Some("schema") | Some("catalog"));
    let named = file_path
        .file_stem()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.ends_with(".schema"));
    (in_dir || named).then_some(root)
}

/// Whether `file_path` has a `.yaml`/`.yml` extension — a declaration, a
/// manifest, a play script or a test, never a `.lute` document.
pub(super) fn is_yaml(file_path: &Path) -> bool {
    matches!(
        file_path.extension().and_then(|e| e.to_str()),
        Some("yaml") | Some("yml")
    )
}
