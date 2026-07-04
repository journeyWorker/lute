// Lute VS Code extension: launches the `lute-lsp` stdio language server and wires
// it to `.lute` documents. Plain JavaScript (no TypeScript build) so the extension
// runs as-is after `npm install`.
//
// Requires `lute-lsp` on PATH (see ../README.md for the cargo install prerequisite).

const { workspace, window } = require("vscode");
const {
  LanguageClient,
  TransportKind,
} = require("vscode-languageclient/node");

/** @type {import("vscode-languageclient/node").LanguageClient | undefined} */
let client;

/**
 * @param {import("vscode").ExtensionContext} context
 */
function activate(context) {
  // The server is the `lute-lsp` binary on PATH, talking LSP over stdio.
  const serverExecutable = {
    command: "lute-lsp",
    transport: TransportKind.stdio,
  };
  /** @type {import("vscode-languageclient/node").ServerOptions} */
  const serverOptions = {
    run: serverExecutable,
    debug: serverExecutable,
  };

  /** @type {import("vscode-languageclient/node").LanguageClientOptions} */
  const clientOptions = {
    documentSelector: [{ scheme: "file", language: "lute" }],
    synchronize: {
      // Reload diagnostics when project/plugin/schema manifests change.
      fileEvents: workspace.createFileSystemWatcher(
        "**/*.{lute,yaml,yml}"
      ),
    },
  };

  client = new LanguageClient(
    "lute-lsp",
    "Lute Language Server",
    serverOptions,
    clientOptions
  );

  // start() rejects if the `lute-lsp` binary is missing; surface a hint instead
  // of a raw stack trace.
  client.start().catch((err) => {
    window.showErrorMessage(
      "Lute: failed to start 'lute-lsp'. Is it on your PATH? " +
        "Install with `cargo install --path crates/lute-lsp`. (" +
        String(err) +
        ")"
    );
  });

  context.subscriptions.push({ dispose: () => void deactivate() });
}

function deactivate() {
  if (!client) {
    return undefined;
  }
  const stopping = client.stop();
  client = undefined;
  return stopping;
}

module.exports = { activate, deactivate };
