// Lute VS Code extension: launches the `lute-lsp` stdio language server and wires
// it to `.lute` documents. Plain JavaScript (no TypeScript build) so the extension
// runs as-is after `npm install`.
//
// Resolves `lute-lsp` from the `lute.lsp.path` setting or PATH (see README.md).

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
  // Resolve the `lute-lsp` server binary in order:
  //   1. the `lute.lsp.path` user setting, if set (absolute path preferred);
  //   2. otherwise `lute-lsp` from PATH.
  // Auto-download of a matching server build is planned but NOT implemented in
  // this pass — see README.md ("Planned: auto-download").
  const configuredPath = workspace
    .getConfiguration("lute")
    .get("lsp.path", "")
    .trim();
  const command = configuredPath || "lute-lsp";

  const serverExecutable = {
    command,
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

  // start() rejects if the server binary is missing; surface a hint instead
  // of a raw stack trace.
  client.start().catch((err) => {
    const where = configuredPath
      ? `the configured 'lute.lsp.path' (${configuredPath})`
      : "your PATH";
    window.showErrorMessage(
      `Lute: failed to start '${command}' from ${where}. ` +
        "Install it with `cargo install --path crates/lute-lsp`, or set " +
        "`lute.lsp.path` to the binary. (" +
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
