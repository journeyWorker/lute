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
use lute_core_span::{Span, TextIndex};
use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::{
    CompletionOptions, CompletionParams, CompletionResponse, Diagnostic as LspDiagnostic,
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DocumentSymbolParams, DocumentSymbolResponse, FoldingRange, FoldingRangeParams,
    FoldingRangeProviderCapability, GotoDefinitionParams, GotoDefinitionResponse, Hover,
    HoverParams, HoverProviderCapability, InitializeParams, InitializeResult, Location,
    MessageType, OneOf, Position, Range, ReferenceParams, SemanticTokens, SemanticTokensFullOptions,
    SemanticTokensOptions, SemanticTokensParams, SemanticTokensResult,
    SemanticTokensServerCapabilities, ServerCapabilities, ServerInfo, TextDocumentSyncCapability,
    TextDocumentSyncKind, Uri, WorkDoneProgressOptions,
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

    /// The current full text of the open document `uri`, or `None` if it is not
    /// open. Cloned so the feature call runs without holding the `DashMap` guard.
    fn document_text(&self, uri: &Uri) -> Option<String> {
        self.docs.get(uri).map(|d| d.text.clone())
    }
}

impl LanguageServer for Backend {
    async fn initialize(&self, _params: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                // FULL document sync + publishDiagnostics (6.1) retained.
                text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
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
                semantic_tokens_provider: Some(SemanticTokensServerCapabilities::SemanticTokensOptions(
                    SemanticTokensOptions {
                        work_done_progress_options: WorkDoneProgressOptions::default(),
                        legend: semtok::legend(),
                        range: Some(false),
                        full: Some(SemanticTokensFullOptions::Bool(true)),
                    },
                )),
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
        let Some(text) = self.document_text(&pos.text_document.uri) else { return Ok(None) };
        let (doc, _) = lute_syntax::parse(&text);
        let snapshot = lute_manifest::core::load_core_snapshot();
        let off = position_to_byte(&text, pos.position);
        Ok(hover::hover_at(&doc, &snapshot, off))
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let pos = params.text_document_position;
        let Some(text) = self.document_text(&pos.text_document.uri) else { return Ok(None) };
        let (doc, _) = lute_syntax::parse(&text);
        let snapshot = lute_manifest::core::load_core_snapshot();
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
        let Some(text) = self.document_text(&uri) else { return Ok(None) };
        let (doc, _) = lute_syntax::parse(&text);
        let snapshot = lute_manifest::core::load_core_snapshot();
        let idx = TextIndex::new(&text);
        let off = position_to_byte(&text, pos.position);
        Ok(nav::definition_at(&doc, &snapshot, off).map(|span| {
            GotoDefinitionResponse::Scalar(Location { uri, range: span_to_range(&span, &idx) })
        }))
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let pos = params.text_document_position;
        let uri = pos.text_document.uri;
        let Some(text) = self.document_text(&uri) else { return Ok(None) };
        let (doc, _) = lute_syntax::parse(&text);
        let snapshot = lute_manifest::core::load_core_snapshot();
        let idx = TextIndex::new(&text);
        let off = position_to_byte(&text, pos.position);
        let locs: Vec<Location> = nav::references_at(&doc, &snapshot, off)
            .into_iter()
            .map(|span| Location { uri: uri.clone(), range: span_to_range(&span, &idx) })
            .collect();
        if locs.is_empty() {
            return Ok(None);
        }
        Ok(Some(locs))
    }

    async fn folding_range(
        &self,
        params: FoldingRangeParams,
    ) -> Result<Option<Vec<FoldingRange>>> {
        let Some(text) = self.document_text(&params.text_document.uri) else { return Ok(None) };
        let (doc, _) = lute_syntax::parse(&text);
        let idx = TextIndex::new(&text);
        Ok(Some(folding::folding_ranges(&doc, &idx)))
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let Some(text) = self.document_text(&params.text_document.uri) else { return Ok(None) };
        let (doc, _) = lute_syntax::parse(&text);
        let idx = TextIndex::new(&text);
        let data = semtok::semantic_tokens(&doc, &idx);
        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens { result_id: None, data })))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let Some(text) = self.document_text(&params.text_document.uri) else { return Ok(None) };
        let (doc, _) = lute_syntax::parse(&text);
        let idx = TextIndex::new(&text);
        Ok(Some(DocumentSymbolResponse::Nested(symbols::document_symbols(&doc, &idx))))
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
    let line_end = text[line_start..].find('\n').map_or(text.len(), |n| line_start + n);
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
    Range { start: byte_to_position(span.byte_start, idx), end: byte_to_position(span.byte_end, idx) }
}

pub(crate) fn byte_to_position(byte: usize, idx: &TextIndex) -> Position {
    let p = idx.position(byte);
    Position { line: p.line - 1, character: p.utf16_col }
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
            let pos = Position { line: p.line - 1, character: p.utf16_col };
            assert_eq!(position_to_byte(text, pos), byte, "byte {byte} did not round-trip");
        }
    }

    #[test]
    fn position_to_byte_clamps_out_of_range() {
        let text = "ab\ncd\n";
        // Character past line end clamps to the line end (before the `\n`).
        assert_eq!(position_to_byte(text, Position { line: 0, character: 99 }), 2);
        // Line past EOF clamps to text end.
        assert_eq!(position_to_byte(text, Position { line: 50, character: 0 }), text.len());
    }

    #[test]
    fn span_to_range_maps_utf16_columns() {
        let text = "π::x\n"; // `π` is 2 bytes, 1 UTF-16 unit.
        let idx = TextIndex::new(text);
        // Span over `::x` starts at byte 2 (after the 2-byte π) => UTF-16 col 1.
        let span = Span { byte_start: 2, byte_end: 5, line: 0, column: 0, utf16_range: (0, 0) };
        let range = span_to_range(&span, &idx);
        assert_eq!(range.start, Position { line: 0, character: 1 });
        assert_eq!(range.end, Position { line: 0, character: 4 });
    }
}
