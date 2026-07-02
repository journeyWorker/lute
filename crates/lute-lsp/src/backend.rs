//! The `LanguageServer` backend (Task 6.1).
//!
//! Holds the open-document map and runs the shared [`lute_check::check`] core on
//! every open/change, republishing the result via `publishDiagnostics`. It is a
//! pass-through to `check()` — no validation logic lives here.
//!
//! ## Sync model
//! We advertise `TextDocumentSyncKind::FULL`: the client resends the entire
//! document on each edit, so `did_change` simply takes the last content-change's
//! `text` as the new snapshot. Incremental sync (range-scoped edits) is out of
//! scope for 6.1.
//!
//! ## The divergence invariant
//! [`analyze`](Backend::analyze) builds a `TextIndex` from the *same* document
//! text `check()` saw and maps each diagnostic's byte offsets through it (via
//! [`crate::convert::to_lsp_diagnostic`]). Since `check()` already re-derived every
//! span from its bytes through one shared `TextIndex`, the positions the LSP
//! publishes match the headless CLI byte-for-byte — the property Task 6.2's
//! golden asserts.

use std::path::{Path, PathBuf};

use dashmap::DashMap;
use lute_check::{check, CheckInput, Mode};
use lute_core_span::{Span, TextIndex};
use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::{
    CompletionOptions, CompletionParams, CompletionResponse, Diagnostic as LspDiagnostic,
    DiagnosticSeverity, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DocumentSymbolParams, DocumentSymbolResponse, FoldingRange,
    FoldingRangeParams, FoldingRangeProviderCapability, GotoDefinitionParams,
    GotoDefinitionResponse, Hover, HoverParams, HoverProviderCapability, InitializeParams,
    InitializeResult, Location, MessageType, OneOf, Position, Range, ReferenceParams,
    SemanticTokens, SemanticTokensFullOptions, SemanticTokensOptions, SemanticTokensParams,
    SemanticTokensResult, SemanticTokensServerCapabilities, ServerCapabilities, ServerInfo,
    TextDocumentSyncCapability, TextDocumentSyncKind, Uri, WorkDoneProgressOptions,
};
use tower_lsp_server::{Client, LanguageServer};

use crate::convert::to_lsp_diagnostic;
use crate::features::{completion, folding, hover, nav, semtok, symbols};

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

/// The Lute language server: an LSP client handle plus the concurrent map of open
/// documents keyed by their [`Uri`].
#[derive(Debug)]
pub struct Backend {
    client: Client,
    docs: DashMap<Uri, DocumentSnapshot>,
}

impl Backend {
    /// Build a backend bound to `client` with an empty document map.
    pub fn new(client: Client) -> Self {
        Self {
            client,
            docs: DashMap::new(),
        }
    }

    /// Run `check()` over `snapshot`'s text and publish the converted diagnostics
    /// for `uri`, stamped with the snapshot version. Positions are derived exactly
    /// as the headless path derives them (see the divergence invariant above).
    ///
    /// Project-resolution diagnostics (a broken plugin graph above the document:
    /// load/cycle/unresolved-depends/assembly errors) are surfaced too, each as an
    /// Error at the document start — otherwise a scene that is itself clean would
    /// silently validate against a broken project (plugin §11).
    async fn analyze(&self, uri: Uri, snapshot: &DocumentSnapshot) {
        let (cap, rdiags) = self.snapshot_for(&uri, &snapshot.text);
        let input = CheckInput {
            text: snapshot.text.clone(),
            uri: uri.as_str().to_string(),
            snapshot: cap,
            providers: lute_manifest::provider::ProviderSet::default(),
            mode: Mode::Author,
        };
        let result = check(&input);
        let idx = lute_core_span::TextIndex::new(&snapshot.text);
        let mut diags: Vec<LspDiagnostic> = result
            .diagnostics
            .iter()
            .map(|d| to_lsp_diagnostic(d, &idx))
            .collect();
        diags.extend(rdiags.iter().map(resolve_diag_to_lsp));
        self.client
            .publish_diagnostics(uri, diags, Some(snapshot.version))
            .await;
    }

    /// The current full text of the open document `uri`, or `None` if it is not
    /// open. Cloned so the feature call runs without holding the `DashMap` guard.
    fn document_text(&self, uri: &Uri) -> Option<String> {
        self.docs.get(uri).map(|d| d.text.clone())
    }

    /// Build the capability snapshot for `uri`'s document by discovering a
    /// `lute.project.yaml` above the file and resolving through the SHARED
    /// [`resolve_document_snapshot`](lute_manifest::project::resolve_document_snapshot)
    /// — the *identical* resolution the CLI runs (plugin §11), so the two
    /// surfaces build byte-identical snapshots and cannot diverge.
    ///
    /// The scene's frontmatter `profile`/`plugins` are lifted with a default
    /// snapshot (both are built-in, not capability-gated). When no project is
    /// found above the document, `resolve_document_snapshot(None, ..)` yields the
    /// core-only baseline — today's behavior. A malformed project is logged and
    /// falls back to core-only rather than silently mis-validating (never panics).
    fn snapshot_for(
        &self,
        uri: &Uri,
        text: &str,
    ) -> (
        lute_manifest::snapshot::CapabilitySnapshot,
        Vec<lute_manifest::project::ResolveDiag>,
    ) {
        let (doc, _) = lute_syntax::parse(text);
        let (meta0, _) = lute_check::parse_meta(
            &doc.meta,
            &lute_manifest::snapshot::CapabilitySnapshot::default(),
        );
        let project = uri_to_path(uri)
            .and_then(|p| find_project_root(&p))
            .and_then(|root| match lute_manifest::project::load_project(&root) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("lute-lsp: {e}");
                    None
                }
            });
        lute_manifest::project::resolve_document_snapshot(
            project.as_ref(),
            meta0.profile.as_deref(),
            &meta0.plugins,
        )
    }
}

impl LanguageServer for Backend {
    async fn initialize(&self, _params: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                // FULL document sync + publishDiagnostics (6.1) retained.
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                // 6.3 editor features. Trigger chars fire completion where the
                // resolver keys off punctuation: `::` (directive head), `@` (a
                // CEL `@ref`), `{` (a directive attr area), `.` (a state path).
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(
                        [":", "@", "{", "."].iter().map(|s| s.to_string()).collect(),
                    ),
                    ..Default::default()
                }),
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                // 6.4 structure features. Folding + document symbols are simple
                // providers; semantic tokens advertise the full-document legend
                // (the closed layer set) that the delta stream is decoded against.
                folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            work_done_progress_options: WorkDoneProgressOptions::default(),
                            legend: semtok::legend(),
                            range: Some(false),
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                        },
                    ),
                ),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "lute-lsp".into(),
                version: Some(env!("CARGO_PKG_VERSION").into()),
            }),
            ..Default::default()
        })
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let pos = params.text_document_position_params;
        let Some(text) = self.document_text(&pos.text_document.uri) else {
            return Ok(None);
        };
        let (doc, _) = lute_syntax::parse(&text);
        let snapshot = self.snapshot_for(&pos.text_document.uri, &text).0;
        let off = position_to_byte(&text, pos.position);
        Ok(hover::hover_at(&doc, &snapshot, off))
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let pos = params.text_document_position;
        let Some(text) = self.document_text(&pos.text_document.uri) else {
            return Ok(None);
        };
        let (doc, _) = lute_syntax::parse(&text);
        let snapshot = self.snapshot_for(&pos.text_document.uri, &text).0;
        let off = position_to_byte(&text, pos.position);
        let items = completion::complete_at(&doc, &snapshot, off);
        if items.is_empty() {
            return Ok(None);
        }
        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let pos = params.text_document_position_params;
        let uri = pos.text_document.uri;
        let Some(text) = self.document_text(&uri) else {
            return Ok(None);
        };
        let (doc, _) = lute_syntax::parse(&text);
        let snapshot = self.snapshot_for(&uri, &text).0;
        let idx = TextIndex::new(&text);
        let off = position_to_byte(&text, pos.position);
        Ok(nav::definition_at(&doc, &snapshot, off).map(|span| {
            GotoDefinitionResponse::Scalar(Location {
                uri,
                range: span_to_range(&span, &idx),
            })
        }))
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let pos = params.text_document_position;
        let uri = pos.text_document.uri;
        let Some(text) = self.document_text(&uri) else {
            return Ok(None);
        };
        let (doc, _) = lute_syntax::parse(&text);
        let snapshot = self.snapshot_for(&uri, &text).0;
        let idx = TextIndex::new(&text);
        let off = position_to_byte(&text, pos.position);
        let locs: Vec<Location> = nav::references_at(&doc, &snapshot, off)
            .into_iter()
            .map(|span| Location {
                uri: uri.clone(),
                range: span_to_range(&span, &idx),
            })
            .collect();
        if locs.is_empty() {
            return Ok(None);
        }
        Ok(Some(locs))
    }

    async fn folding_range(&self, params: FoldingRangeParams) -> Result<Option<Vec<FoldingRange>>> {
        let Some(text) = self.document_text(&params.text_document.uri) else {
            return Ok(None);
        };
        let (doc, _) = lute_syntax::parse(&text);
        let idx = TextIndex::new(&text);
        Ok(Some(folding::folding_ranges(&doc, &idx)))
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let Some(text) = self.document_text(&params.text_document.uri) else {
            return Ok(None);
        };
        let (doc, _) = lute_syntax::parse(&text);
        let idx = TextIndex::new(&text);
        let data = semtok::semantic_tokens(&doc, &idx);
        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data,
        })))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let Some(text) = self.document_text(&params.text_document.uri) else {
            return Ok(None);
        };
        let (doc, _) = lute_syntax::parse(&text);
        let idx = TextIndex::new(&text);
        Ok(Some(DocumentSymbolResponse::Nested(
            symbols::document_symbols(&doc, &idx),
        )))
    }

    async fn initialized(&self, _params: tower_lsp_server::ls_types::InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "lute-lsp initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let doc = params.text_document;
        let snapshot = DocumentSnapshot {
            text: doc.text,
            version: doc.version,
        };
        self.docs.insert(doc.uri.clone(), snapshot.clone());
        self.analyze(doc.uri, &snapshot).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        // FULL sync: the final content change carries the whole new document.
        let Some(change) = params.content_changes.into_iter().next_back() else {
            return;
        };
        let uri = params.text_document.uri;
        let snapshot = DocumentSnapshot {
            text: change.text,
            version: params.text_document.version,
        };
        self.docs.insert(uri.clone(), snapshot.clone());
        self.analyze(uri, &snapshot).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        self.docs.remove(&uri);
        // LSP diagnostics are server-owned and persist in the client until the
        // server replaces them. The buffer is gone, so publish an empty set to
        // clear any squiggles the last analyze() left behind (no version stamp:
        // the document has no live version once closed).
        self.client.publish_diagnostics(uri, Vec::new(), None).await;
    }
}

/// Convert an LSP [`Position`] (0-based line, 0-based UTF-16 character) to a byte
/// offset into `text`. The inverse of [`TextIndex::position`]: locate the line by
/// counting `\n`s, then walk that line's chars summing `len_utf16` until the
/// UTF-16 column is reached — the exact per-line UTF-16 accounting `TextIndex`
/// does forward, so the LSP and headless surfaces never drift a code unit.
/// A `character` past the line end clamps to the line end (defensive; LSP clients
/// occasionally over-report a column at EOL).
fn position_to_byte(text: &str, pos: Position) -> usize {
    let bytes = text.as_bytes();
    // Byte offset of the start of line `pos.line` (0-based).
    let mut line = 0u32;
    let mut line_start = 0usize;
    if pos.line > 0 {
        for (i, b) in bytes.iter().enumerate() {
            if *b == b'\n' {
                line += 1;
                if line == pos.line {
                    line_start = i + 1;
                    break;
                }
            }
        }
        if line < pos.line {
            // Line beyond EOF: clamp to end of text.
            return text.len();
        }
    }
    // Walk the target line, summing UTF-16 units until `pos.character`.
    let line_end = text[line_start..]
        .find('\n')
        .map_or(text.len(), |n| line_start + n);
    let mut utf16 = 0u32;
    for (off, ch) in text[line_start..line_end].char_indices() {
        if utf16 >= pos.character {
            return line_start + off;
        }
        utf16 += ch.len_utf16() as u32;
    }
    line_end
}

/// Map a byte [`Span`] to an LSP [`Range`] through `idx`. Mirrors
/// [`crate::convert`]'s private byte-span mapping (`TextIndex::position`,
/// de-1-indexing the line, 0-based UTF-16 column) so navigation results carry the
/// same UTF-16-correct positions as diagnostics. Byte-only spans synthesized by
/// the feature layer (zeroed line/col) resolve correctly here because the mapping
/// only reads `byte_start`/`byte_end`.
pub(crate) fn span_to_range(span: &Span, idx: &TextIndex) -> Range {
    Range {
        start: byte_to_position(span.byte_start, idx),
        end: byte_to_position(span.byte_end, idx),
    }
}

pub(crate) fn byte_to_position(byte: usize, idx: &TextIndex) -> Position {
    let p = idx.position(byte);
    Position {
        line: p.line - 1,
        character: p.utf16_col,
    }
}

/// Convert a project-resolution diagnostic (a broken plugin graph above the
/// document) into an LSP diagnostic. Resolver diagnostics have no source span, so
/// they anchor at the document start (line 0, char 0) as an Error sourced "lute"
/// — matching the CLI, which already surfaces the same `ResolveDiag` messages to
/// stderr. This is the seam that keeps the LSP from silently dropping them.
fn resolve_diag_to_lsp(d: &lute_manifest::project::ResolveDiag) -> LspDiagnostic {
    let start = Position {
        line: 0,
        character: 0,
    };
    LspDiagnostic {
        range: Range { start, end: start },
        severity: Some(DiagnosticSeverity::ERROR),
        source: Some("lute".into()),
        message: d.message.clone(),
        ..Default::default()
    }
}

/// Resolve a document [`Uri`] to a filesystem [`PathBuf`]. Only a `file` URI maps
/// to a real path: [`Uri::to_file_path`] does NOT check the scheme (verified — it
/// returns `Some` for `untitled:`/`vscode-vfs:` too), so guard it explicitly. A
/// non-file (virtual/unsaved) document returns `None`, and snapshot resolution
/// falls back to core-only. Schemes are case-insensitive (RFC 3986 §3.1). Ownership
/// is taken (`Cow` → `PathBuf`) so the path outlives the borrow of `uri`.
fn uri_to_path(uri: &Uri) -> Option<PathBuf> {
    if !uri.scheme().as_str().eq_ignore_ascii_case("file") {
        return None;
    }
    uri.to_file_path().map(|p| p.into_owned())
}

/// Walk up from the document at `file_path`, returning the first ancestor
/// directory that contains a `lute.project.yaml` (plugin §11 project discovery).
/// Starts at the file's parent directory and climbs to the filesystem root;
/// `None` when no project is found (a loose scene → core-only). Purely lexical
/// on the path components, so it works for buffers not yet written to disk.
fn find_project_root(file_path: &Path) -> Option<PathBuf> {
    let mut dir = file_path.parent();
    while let Some(d) = dir {
        if d.join("lute.project.yaml").is_file() {
            return Some(d.to_path_buf());
        }
        dir = d.parent();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `position_to_byte` is the exact inverse of `TextIndex::position` on a
    /// multibyte document: every byte offset round-trips through its own Position.
    #[test]
    fn position_to_byte_round_trips_through_text_index() {
        let text = "## Shot 1.\n::café{x=\"π\"}\n:line[Ω]: 世界\n";
        let idx = TextIndex::new(text);
        for byte in 0..=text.len() {
            // Only test char boundaries; a byte mid-char has no LSP position.
            if !text.is_char_boundary(byte) {
                continue;
            }
            let p = idx.position(byte);
            let pos = Position {
                line: p.line - 1,
                character: p.utf16_col,
            };
            assert_eq!(
                position_to_byte(text, pos),
                byte,
                "byte {byte} did not round-trip"
            );
        }
    }

    #[test]
    fn position_to_byte_clamps_out_of_range() {
        let text = "ab\ncd\n";
        // Character past line end clamps to the line end (before the `\n`).
        assert_eq!(
            position_to_byte(
                text,
                Position {
                    line: 0,
                    character: 99
                }
            ),
            2
        );
        // Line past EOF clamps to text end.
        assert_eq!(
            position_to_byte(
                text,
                Position {
                    line: 50,
                    character: 0
                }
            ),
            text.len()
        );
    }

    #[test]
    fn span_to_range_maps_utf16_columns() {
        let text = "π::x\n"; // `π` is 2 bytes, 1 UTF-16 unit.
        let idx = TextIndex::new(text);
        // Span over `::x` starts at byte 2 (after the 2-byte π) => UTF-16 col 1.
        let span = Span {
            byte_start: 2,
            byte_end: 5,
            line: 0,
            column: 0,
            utf16_range: (0, 0),
        };
        let range = span_to_range(&span, &idx);
        assert_eq!(
            range.start,
            Position {
                line: 0,
                character: 1
            }
        );
        assert_eq!(
            range.end,
            Position {
                line: 0,
                character: 4
            }
        );
    }

    /// S1: closing a document MUST publish an EMPTY diagnostics set for its URI.
    /// LSP diagnostics are server-owned and persist in the client until replaced,
    /// so without this the last analyze()'s squiggles linger after the buffer is
    /// gone. Drives a real `LspService<Backend>`: initialize -> didOpen (expect a
    /// non-empty publish) -> didClose (expect an empty publish for the same URI).
    #[tokio::test(flavor = "current_thread")]
    async fn did_close_publishes_empty_diagnostics() {
        use futures::StreamExt;
        use tower::{Service, ServiceExt};
        use tower_lsp_server::jsonrpc::Request as RpcRequest;
        use tower_lsp_server::LspService;

        let (mut service, mut socket) = LspService::new(Backend::new);
        let uri_str = "file:///t.lute";
        // A body line that matches no §4.3 rule → a guaranteed parse diagnostic.
        let text = "## Shot 1.\ngarbage prose line\n";

        let init = RpcRequest::build("initialize")
            .params(serde_json::json!({ "capabilities": {} }))
            .id(1)
            .finish();
        service.ready().await.unwrap().call(init).await.unwrap();

        let open = RpcRequest::build("textDocument/didOpen")
            .params(serde_json::json!({
                "textDocument": {
                    "uri": uri_str, "languageId": "lute", "version": 1, "text": text
                }
            }))
            .finish();
        service.ready().await.unwrap().call(open).await.unwrap();
        let opened = socket.next().await.expect("didOpen should publish");
        assert_eq!(opened.method(), "textDocument/publishDiagnostics");
        let odiags = opened
            .params()
            .and_then(|p| p.get("diagnostics"))
            .and_then(|d| d.as_array())
            .expect("publish carries a diagnostics array");
        assert!(
            !odiags.is_empty(),
            "an errored open doc should publish squiggles"
        );

        let close = RpcRequest::build("textDocument/didClose")
            .params(serde_json::json!({ "textDocument": { "uri": uri_str } }))
            .finish();
        service.ready().await.unwrap().call(close).await.unwrap();
        let closed = tokio::time::timeout(std::time::Duration::from_secs(2), socket.next())
            .await
            .expect("did_close must publish (empty) diagnostics; none arrived")
            .expect("socket closed without a close-publish");
        assert_eq!(closed.method(), "textDocument/publishDiagnostics");
        let cparams = closed.params().expect("close publish carries params");
        assert_eq!(
            cparams.get("uri").and_then(|u| u.as_str()),
            Some(uri_str),
            "close publish targets the closed URI"
        );
        let cdiags = cparams
            .get("diagnostics")
            .and_then(|d| d.as_array())
            .expect("close publish carries a diagnostics array");
        assert!(
            cdiags.is_empty(),
            "closing must clear diagnostics, got {cdiags:?}"
        );
    }

    #[test]
    fn uri_to_path_rejects_non_file_schemes() {
        use std::str::FromStr;
        // file: URI -> Some path
        let f = Uri::from_str("file:///tmp/x/doc.lute").unwrap();
        assert!(
            uri_to_path(&f).is_some(),
            "file: URI must resolve to a path"
        );
        // non-file (virtual/unsaved) schemes -> None (core-only fallback)
        for s in [
            "untitled:/repo/sub/doc.lute",
            "vscode-vfs://host/repo/doc.lute",
            "untitled:Untitled-1",
        ] {
            let u = Uri::from_str(s).unwrap();
            assert!(
                uri_to_path(&u).is_none(),
                "non-file URI {s:?} must NOT resolve to a filesystem path"
            );
        }
    }

    /// FINDING 1 guard: a document under a project whose plugin graph has a
    /// `DependsCycle` MUST publish that resolver diagnostic as an LSP diagnostic,
    /// even when the document itself is core-clean. Before the fix, `snapshot_for`
    /// discarded the resolver `Vec<ResolveDiag>`, so `analyze` never published it
    /// and the editor silently mis-validated against a broken project. Drives a
    /// real `LspService<Backend>` end to end: initialize -> didOpen a `file://`
    /// scene under a temp project with two mutually-depending plugins, then assert
    /// the published set carries a `DependsCycle` diagnostic sourced "lute" at the
    /// document start.
    #[tokio::test(flavor = "current_thread")]
    async fn analyze_publishes_project_resolver_diagnostics() {
        use futures::StreamExt;
        use std::fs;
        use tower::{Service, ServiceExt};
        use tower_lsp_server::jsonrpc::Request as RpcRequest;
        use tower_lsp_server::LspService;

        // Temp project with a plugin dependency cycle: a.x -> a.dep -> a.x.
        let root = std::env::temp_dir().join(format!("lute_lsp_cycle_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for (id, dep) in [("a.x", "a.dep"), ("a.dep", "a.x")] {
            let pdir = root.join("plugins").join(id);
            fs::create_dir_all(&pdir).unwrap();
            fs::write(
                pdir.join("plugin.yaml"),
                format!(
                    "id: {id}\nversion: 0.1.0\nkind: capability\ndepends: [ {{ id: {dep}, range: \"^0.1.0\" }} ]\nexports: {{}}\n"
                ),
            )
            .unwrap();
        }
        fs::write(
            root.join("lute.project.yaml"),
            "pluginsDir: plugins/\ndefaultProfile: s\nprofiles:\n  s:\n    plugins: { a.x: true, a.dep: true }\n",
        )
        .unwrap();
        let scene_path = root.join("scene.lute");
        let uri_str = format!("file://{}", scene_path.display());

        let (mut service, mut socket) = LspService::new(Backend::new);
        let init = RpcRequest::build("initialize")
            .params(serde_json::json!({ "capabilities": {} }))
            .id(1)
            .finish();
        service.ready().await.unwrap().call(init).await.unwrap();

        let open = RpcRequest::build("textDocument/didOpen")
            .params(serde_json::json!({
                "textDocument": {
                    "uri": uri_str, "languageId": "lute", "version": 1, "text": "## Shot 1.\n"
                }
            }))
            .finish();
        service.ready().await.unwrap().call(open).await.unwrap();
        let opened = socket.next().await.expect("didOpen should publish");
        assert_eq!(opened.method(), "textDocument/publishDiagnostics");
        let diags = opened
            .params()
            .and_then(|p| p.get("diagnostics").cloned())
            .and_then(|d| d.as_array().cloned())
            .expect("publish carries a diagnostics array");
        let resolver = diags.iter().find(|d| {
            d.get("message")
                .and_then(|m| m.as_str())
                .is_some_and(|m| m.contains("DependsCycle"))
        });
        let resolver = resolver.unwrap_or_else(|| {
            panic!("resolver DependsCycle diagnostic must be published, got {diags:?}")
        });
        assert_eq!(
            resolver.get("source").and_then(|s| s.as_str()),
            Some("lute"),
            "resolver diagnostic must be sourced \"lute\""
        );
        assert_eq!(
            resolver.get("severity").and_then(|s| s.as_u64()),
            Some(1),
            "resolver diagnostic must be Error severity"
        );
        let start = resolver
            .get("range")
            .and_then(|r| r.get("start"))
            .expect("range.start present");
        assert_eq!(start.get("line").and_then(|l| l.as_u64()), Some(0));
        assert_eq!(start.get("character").and_then(|c| c.as_u64()), Some(0));
        fs::remove_dir_all(&root).ok();
    }
}
