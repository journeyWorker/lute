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

use dashmap::DashMap;
use lute_check::{check, CheckInput, Mode};
use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::{
    Diagnostic as LspDiagnostic, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, InitializeParams, InitializeResult, MessageType, ServerCapabilities,
    ServerInfo, TextDocumentSyncCapability, TextDocumentSyncKind, Uri,
};
use tower_lsp_server::{Client, LanguageServer};

use crate::convert::to_lsp_diagnostic;

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
        Self { client, docs: DashMap::new() }
    }

    /// Run `check()` over `snapshot`'s text and publish the converted diagnostics
    /// for `uri`, stamped with the snapshot version. Positions are derived exactly
    /// as the headless path derives them (see the divergence invariant above).
    async fn analyze(&self, uri: Uri, snapshot: &DocumentSnapshot) {
        let input = CheckInput {
            text: snapshot.text.clone(),
            uri: uri.as_str().to_string(),
            snapshot: lute_manifest::core::load_core_snapshot(),
            providers: lute_manifest::provider::ProviderSet::default(),
            mode: Mode::Author,
        };
        let result = check(&input);
        let idx = lute_core_span::TextIndex::new(&snapshot.text);
        let diags: Vec<LspDiagnostic> =
            result.diagnostics.iter().map(|d| to_lsp_diagnostic(d, &idx)).collect();
        self.client.publish_diagnostics(uri, diags, Some(snapshot.version)).await;
    }
}

impl LanguageServer for Backend {
    async fn initialize(&self, _params: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                // 6.1 supports FULL document sync + publishDiagnostics only.
                text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "lute-lsp".into(),
                version: Some(env!("CARGO_PKG_VERSION").into()),
            }),
            ..Default::default()
        })
    }

    async fn initialized(&self, _params: tower_lsp_server::ls_types::InitializedParams) {
        self.client.log_message(MessageType::INFO, "lute-lsp initialized").await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let doc = params.text_document;
        let snapshot = DocumentSnapshot { text: doc.text, version: doc.version };
        self.docs.insert(doc.uri.clone(), snapshot.clone());
        self.analyze(doc.uri, &snapshot).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        // FULL sync: the final content change carries the whole new document.
        let Some(change) = params.content_changes.into_iter().next_back() else { return };
        let uri = params.text_document.uri;
        let snapshot = DocumentSnapshot { text: change.text, version: params.text_document.version };
        self.docs.insert(uri.clone(), snapshot.clone());
        self.analyze(uri, &snapshot).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.docs.remove(&params.text_document.uri);
    }
}
