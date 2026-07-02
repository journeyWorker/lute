//! `lute-lsp` stdio entrypoint (Task 6.1).
//!
//! Builds the [`Backend`](lute_lsp::backend::Backend) service and serves LSP over
//! stdin/stdout — the transport every editor host launches a language server on.

use lute_lsp::backend::Backend;
use tower_lsp_server::{LspService, Server};

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
