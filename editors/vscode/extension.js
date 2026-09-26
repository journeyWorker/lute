// Lute VS Code extension: launches the `lute-lsp` stdio language server and wires
// it to `.lute` documents. Plain JavaScript (no TypeScript build) so the extension
// runs as-is after `npm install`.
//
// Resolves `lute-lsp` from the `lute.lsp.path` setting or PATH (see README.md).

const fs = require("fs");
const path = require("path");
const { execFile } = require("child_process");
const { workspace, window } = require("vscode");
const {
  LanguageClient,
  TransportKind,
} = require("vscode-languageclient/node");
const {
  parseFrontmatterLuteVersion,
  parseProjectLuteVersion,
  parseCliVersion,
  versionTarget,
  serverDisagrees,
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
 * Warn once if the running server disagrees with what a `.lute` document
 * targets. The server advertises the language version it implements as
 * `serverInfo.version` (see `backend.rs`). The target is the document's
 * frontmatter `luteVersion:` stamp, else the enclosing project's
 * `lute.project.yaml` `defaults: luteVersion`, else the `lute` CLI on PATH
 * (`lute --version`) — so an unstamped document in an unstamped project
 * still catches an editor server that is not the terminal's. A stale server's
 * diagnostics are untrustworthy — the exact failure the pilot and every
 * round-5 writer hit — so surface an actionable warning naming `lute doctor`.
 * Disabled by the `lute.versionCheck` setting.
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
  /** @type {string | null | undefined} undefined until `lute --version` answers */
  let cliVersion;
  const pending = [];
  const inspect = (doc) => {
    if (warned || !doc || doc.languageId !== "lute") {
      return;
    }
    if (cliVersion === undefined) {
      pending.push(doc);
      return;
    }
    const target = versionTarget({
      doc: parseFrontmatterLuteVersion(doc.getText()),
      project: projectLuteVersion(doc.uri && doc.uri.fsPath),
      cli: cliVersion,
    });
    if (serverDisagrees(serverVersion, target)) {
      warned = true;
      window.showWarningMessage(staleServerMessage(serverVersion, target));
    }
  };
  execFile("lute", ["--version"], { timeout: 3000 }, (err, stdout) => {
    cliVersion = err ? null : parseCliVersion(String(stdout));
    pending.splice(0).forEach(inspect);
  });
  workspace.textDocuments.forEach(inspect);
  context.subscriptions.push(workspace.onDidOpenTextDocument(inspect));
}

/**
 * The `defaults: luteVersion` of the nearest `lute.project.yaml` at or above
 * `file`'s directory, or `null`.
 * @param {string | undefined} file
 * @returns {string | null}
 */
function projectLuteVersion(file) {
  if (!file) {
    return null;
  }
  let dir = path.dirname(file);
  for (;;) {
    const manifest = path.join(dir, "lute.project.yaml");
    if (fs.existsSync(manifest)) {
      try {
        return parseProjectLuteVersion(fs.readFileSync(manifest, "utf8"));
      } catch {
        return null;
      }
    }
    const up = path.dirname(dir);
    if (up === dir) {
      return null;
    }
    dir = up;
  }
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
