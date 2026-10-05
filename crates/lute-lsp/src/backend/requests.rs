use lute_core_span::TextIndex;
use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::{
    CodeActionOrCommand, CodeActionParams, CodeActionProviderCapability, CodeActionResponse,
    CompletionOptions, CompletionParams, CompletionResponse, 
    DidChangeTextDocumentParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, DocumentSymbolParams,
    DocumentSymbolResponse, FoldingRange, FoldingRangeParams, FoldingRangeProviderCapability,
    GotoDefinitionParams, GotoDefinitionResponse, Hover, HoverParams, HoverProviderCapability,
    InitializeParams, InitializeResult, Location, MessageType, OneOf, Position, Range,
    ReferenceParams, SemanticTokens, SemanticTokensFullOptions, SemanticTokensOptions,
    SemanticTokensParams, SemanticTokensResult, SemanticTokensServerCapabilities,
    DocumentFormattingParams, TextEdit as LspTextEdit,
    ServerCapabilities, ServerInfo, TextDocumentSyncCapability, TextDocumentSyncKind,
    WorkDoneProgressOptions,
};
use tower_lsp_server::LanguageServer;
use crate::code_action;
use crate::features::{completion, folding, hover, nav, semtok, symbols};
use super::{Backend, DocumentSnapshot, span_to_range};
use super::positions::position_to_byte;

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
                document_formatting_provider: Some(OneOf::Left(true)),
                // Task 15 (D16): quick fixes over `Diagnostic.fixits` — the
                // author surface for an `E-PERSIST-REMOVED` migrate remedy or
                // a §8.1 T2 CEL rewrite (`lute fix` applies the former too,
                // but this LSP surface stands independently). `Simple(true)`:
                // this server returns only plain `CodeAction`s, no `Command`s,
                // so it advertises no `code_action_kinds` allowlist.
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "lute-lsp".into(),
                // Advertise the LANGUAGE version this server implements, not the
                // raw crate version. An editor client uses this to detect a
                // STALE binary (one older than a document's `luteVersion`
                // target): the server cannot self-detect staleness — its own
                // `W-LUTE-VERSION-STALE` check compares a stamp against this
                // same `LUTE_LANG_VERSION`, so an old server would tell an
                // author to DOWNGRADE a valid stamp. The two axes are unified at
                // release (crate == language), but sourcing the language version
                // keeps the signal honest if they ever diverge again.
                version: Some(lute_check::LUTE_LANG_VERSION.into()),
            }),
            ..Default::default()
        })
    }
    /// Full-document formatting uses the same lossless formatter as `lute fmt`.
    /// No range printer is exposed in 0.35; unsaved text comes from the open
    /// document snapshot and is never written to disk.
    async fn formatting(
        &self,
        params: DocumentFormattingParams,
    ) -> Result<Option<Vec<LspTextEdit>>> {
        let uri = params.text_document.uri;
        let Some(text) = self.document_text(&uri) else {
            return Ok(None);
        };
        let Ok(formatted) =
            lute_syntax::format_source(&text, &lute_syntax::FormatOptions::default())
        else {
            return Ok(None);
        };
        let idx = TextIndex::new(&text);
        let end = idx.position(text.len());
        let full_range = Range {
            start: Position { line: 0, character: 0 },
            end: Position {
                line: end.line.saturating_sub(1),
                character: end.utf16_col,
            },
        };
        Ok(Some(vec![LspTextEdit {
            range: full_range,
            new_text: formatted.text,
        }]))
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        if self.stale().is_some() {
            return Ok(None);
        }
        let pos = params.text_document_position_params;
        let Some(text) = self.document_text(&pos.text_document.uri) else {
            return Ok(None);
        };
        let Some((built, doc)) = self.assemble(&pos.text_document.uri, text.clone()) else {
            return Ok(None);
        };
        let snapshot = built.input.snapshot;
        let imports = built.input.imports;
        let off = position_to_byte(&text, pos.position);
        Ok(hover::hover_at(&doc, &snapshot, &imports, off))
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        if self.stale().is_some() {
            return Ok(None);
        }
        let pos = params.text_document_position;
        let Some(text) = self.document_text(&pos.text_document.uri) else {
            return Ok(None);
        };
        let Some((built, doc)) = self.assemble(&pos.text_document.uri, text.clone()) else {
            return Ok(None);
        };
        let snapshot = built.input.snapshot;
        let providers = built.input.providers;
        let imports = built.input.imports;
        let off = position_to_byte(&text, pos.position);
        let items = completion::complete_at(&doc, &snapshot, &providers, &imports, off);
        if items.is_empty() {
            return Ok(None);
        }
        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        if self.stale().is_some() {
            return Ok(None);
        }
        let pos = params.text_document_position_params;
        let uri = pos.text_document.uri;
        let Some(text) = self.document_text(&uri) else {
            return Ok(None);
        };
        let Some((built, doc)) = self.assemble(&uri, text.clone()) else {
            return Ok(None);
        };
        let snapshot = built.input.snapshot;
        let imports = built.input.imports;
        let idx = TextIndex::new(&text);
        let off = position_to_byte(&text, pos.position);
        Ok(
            nav::definition_at(&doc, &snapshot, &imports, off).map(|span| {
                GotoDefinitionResponse::Scalar(Location {
                    uri,
                    range: span_to_range(&span, &idx),
                })
            }),
        )
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        if self.stale().is_some() {
            return Ok(None);
        }
        let pos = params.text_document_position;
        let uri = pos.text_document.uri;
        let Some(text) = self.document_text(&uri) else {
            return Ok(None);
        };
        let Some((built, doc)) = self.assemble(&uri, text.clone()) else {
            return Ok(None);
        };
        let snapshot = built.input.snapshot;
        let imports = built.input.imports;
        let idx = TextIndex::new(&text);
        let off = position_to_byte(&text, pos.position);
        let locs: Vec<Location> = nav::references_at(
            &doc,
            &snapshot,
            &imports,
            off,
            params.context.include_declaration,
        )
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

    /// `textDocument/codeAction` (Task 15, D16): map the cached ORIGINAL
    /// diagnostics' `fixits` (the published LSP diagnostics dropped them,
    /// `crate::convert`'s doc comment) that overlap `params.range` to
    /// `CodeAction`s, through [`code_action::code_actions_for_fixits`]. `None`
    /// when the document isn't open, has no cached diagnostics yet (never
    /// analyzed), or none overlap with a fixit — never an empty `Some(vec![])`.
    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        if self.stale().is_some() {
            return Ok(None);
        }
        let uri = params.text_document.uri;
        let Some(text) = self.document_text(&uri) else {
            return Ok(None);
        };
        let Some(diags) = self.diagnostics.get(&uri) else {
            return Ok(None);
        };
        let idx = TextIndex::new(&text);
        let actions = code_action::code_actions_for_fixits(&diags, &uri, params.range, &idx);
        if actions.is_empty() {
            return Ok(None);
        }
        Ok(Some(
            actions
                .into_iter()
                .map(CodeActionOrCommand::CodeAction)
                .collect(),
        ))
    }

    async fn initialized(&self, _params: tower_lsp_server::ls_types::InitializedParams) {
        self.client
            .log_message(
                MessageType::INFO,
                format!("lute-lsp {} initialized", env!("CARGO_PKG_VERSION")),
            )
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
        self.diagnostics.remove(&uri);
        // LSP diagnostics are server-owned and persist in the client until the
        // server replaces them. The buffer is gone, so publish an empty set to
        // clear any squiggles the last analyze() left behind (no version stamp:
        // the document has no live version once closed), and withdraw what it
        // reported in other open files.
        self.publish(uri, None, Vec::new()).await;
    }
}
