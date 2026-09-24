//! `lute-lsp` stdio entrypoint (Task 6.1).
//!
//! Builds the [`Backend`](lute_lsp::backend::Backend) service and serves LSP over
//! stdin/stdout — the transport every editor host launches a language server on.
//!
//! `lute-lsp --version` (or `-V`) prints `lute-lsp <version>` and exits instead
//! of serving, so `lute doctor` can tell when the server an editor launches
//! from `PATH` is not the toolchain the CLI is (dsl 0.22.0 §13). Every other
//! argument is ignored: editor hosts pass transport flags such as `--stdio`.

use lute_lsp::backend::Backend;
use tower_lsp_server::{LspService, Server};

#[tokio::main]
async fn main() {
    if std::env::args()
        .skip(1)
        .any(|a| a == "--version" || a == "-V")
    {
        println!("lute-lsp {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
