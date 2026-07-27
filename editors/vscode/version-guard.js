// Pure helpers for the lute-lsp stale-binary version guard. No `vscode`
// dependency, so they are unit-testable under `bun test` (see
// `test/version-guard.test.js`).
//
// Why this exists: `lute-lsp` funnels every diagnostic through the shared
// `lute_check` core, so its diagnostics are byte-for-byte the CLI's — but only
// for the language version it was BUILT at. A server binary older than the
// language a document targets silently mis-analyzes newer grammar (the pilot's
// "cinematic shot heading" misdiagnosis). The server cannot self-detect this:
// its `W-LUTE-VERSION-STALE` check compares a document's `luteVersion` against
// its OWN `LUTE_LANG_VERSION`, so a stale server would even tell an author to
// DOWNGRADE a valid stamp. The only reliable signal is comparing the running
// server's advertised version (LSP `serverInfo.version`) against the version
// the author declares in frontmatter.

/**
 * Parse a `.lute` document's frontmatter `luteVersion:` stamp (dsl §6.1).
 * Returns the trimmed version string, or `null` when there is no leading
 * frontmatter fence or no `luteVersion` key inside it (a `luteVersion:` in the
 * body is not a stamp, so only the fenced block is scanned).
 * @param {string} text
 * @returns {string | null}
 */
function parseFrontmatterLuteVersion(text) {
  if (typeof text !== "string") return null;
  const fence = /^---\r?\n([\s\S]*?)\r?\n---/.exec(text);
  if (!fence) return null;
  const line = /^[ \t]*luteVersion[ \t]*:[ \t]*(.+?)[ \t]*$/m.exec(fence[1]);
  if (!line) return null;
  const value = line[1].replace(/^["']|["']$/g, "").trim();
  return value || null;
}

/**
 * Parse a dotted numeric version string ("x.y.z") into a 3-number array, or
 * `null` when it is not a clean numeric triple.
 * @param {string} v
 * @returns {number[] | null}
 */
function parseTriple(v) {
  if (typeof v !== "string") return null;
  const parts = v.trim().split(".");
  if (parts.length !== 3) return null;
  const nums = parts.map((p) => (/^\d+$/.test(p) ? Number(p) : NaN));
  return nums.some(Number.isNaN) ? null : nums;
}

/**
 * Compare two dotted numeric version strings. Returns -1/0/1, or `null` when
 * either side is not a clean numeric triple (garbage yields no verdict).
 * @param {string} a
 * @param {string} b
 * @returns {number | null}
 */
function compareVersions(a, b) {
  const pa = parseTriple(a);
  const pb = parseTriple(b);
  if (!pa || !pb) return null;
  for (let i = 0; i < 3; i++) {
    if (pa[i] !== pb[i]) return pa[i] < pb[i] ? -1 : 1;
  }
  return 0;
}

/**
 * True when `serverVersion` is strictly OLDER than `declaredVersion` — the
 * running server predates the language the document targets, so its diagnostics
 * are untrustworthy for newer grammar. Newer-or-equal is fine (the reverse — a
 * stale stamp — is the checker's own `W-LUTE-VERSION-STALE` job). An
 * unparseable version on either side yields `false`: never warn on a verdict we
 * cannot compute.
 * @param {string} serverVersion
 * @param {string} declaredVersion
 * @returns {boolean}
 */
function serverIsStale(serverVersion, declaredVersion) {
  return compareVersions(serverVersion, declaredVersion) === -1;
}

/**
 * The user-facing warning shown once when a stale server is detected.
 * @param {string} serverVersion
 * @param {string} declaredVersion
 * @returns {string}
 */
function staleServerMessage(serverVersion, declaredVersion) {
  return (
    `Lute: the language server (v${serverVersion}) is older than a document ` +
    `targets (luteVersion "${declaredVersion}"). Its diagnostics may be wrong ` +
    `for newer grammar. Rebuild it (\`cargo install --path crates/lute-lsp\`) ` +
    "or point `lute.lsp.path` at a current binary."
  );
}

module.exports = {
  parseFrontmatterLuteVersion,
  parseTriple,
  compareVersions,
  serverIsStale,
  staleServerMessage,
};
