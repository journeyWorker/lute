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
const {
  parseFrontmatterLuteVersion,
  serverIsStale,
  staleServerMessage,
} = require("./version-guard");

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
  // of a raw stack trace. On success, wire the stale-binary version guard.
  client.start().then(
    () => wireVersionGuard(context),
    (err) => {
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
    }
  );

  context.subscriptions.push({ dispose: () => void deactivate() });
}

/**
 * Warn once if the running server is older than a `.lute` document targets.
 * The server advertises the language version it implements as
 * `serverInfo.version` (see `backend.rs`); a document declares its target via
 * the frontmatter `luteVersion:` stamp. When the server is strictly older, its
 * diagnostics are untrustworthy for newer grammar — the exact failure the pilot
 * hit with a stale binary — so surface an actionable warning. Disabled by the
 * `lute.versionCheck` setting.
 * @param {import("vscode").ExtensionContext} context
 */
function wireVersionGuard(context) {
  if (!client) {
    return;
  }
  if (!workspace.getConfiguration("lute").get("versionCheck", true)) {
    return;
  }
  const info = client.initializeResult && client.initializeResult.serverInfo;
  const serverVersion = info && info.version;
  if (!serverVersion) {
    return;
  }
  let warned = false;
  const inspect = (doc) => {
    if (warned || !doc || doc.languageId !== "lute") {
      return;
    }
    const declared = parseFrontmatterLuteVersion(doc.getText());
    if (declared && serverIsStale(serverVersion, declared)) {
      warned = true;
      window.showWarningMessage(staleServerMessage(serverVersion, declared));
    }
  };
  workspace.textDocuments.forEach(inspect);
  context.subscriptions.push(workspace.onDidOpenTextDocument(inspect));
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
