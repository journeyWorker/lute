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
// the author declares in frontmatter — or, when the document carries no stamp
// (a writer's documents often do not), the project manifest's
// `defaults: luteVersion`, and failing that the `lute` CLI on PATH: an editor
// server that disagrees with the terminal is the round-5 failure (every new
// writer met a 0.17 server and found it only through `lute doctor`).

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
 * Parse `lute.project.yaml`'s `defaults: luteVersion` (the project stamp
 * every document inherits, 0.10.0 §6). Handles the block form
 * (`defaults:` then an indented `luteVersion:` line) and the flow form
 * (`defaults: { luteVersion: "x" }`); `null` when absent.
 * @param {string} text
 * @returns {string | null}
 */
function parseProjectLuteVersion(text) {
  if (typeof text !== "string") return null;
  const lines = text.split(/\r?\n/);
  for (let i = 0; i < lines.length; i++) {
    const head = /^defaults[ \t]*:(.*)$/.exec(lines[i]);
    if (!head) continue;
    const flow = /luteVersion[ \t]*:[ \t]*["']?([^"',}\s]+)/.exec(head[1]);
    if (flow) return flow[1];
    for (let j = i + 1; j < lines.length; j++) {
      const line = lines[j];
      if (/^\S/.test(line)) break; // back at top level
      const m = /^[ \t]+luteVersion[ \t]*:[ \t]*(.+?)[ \t]*$/.exec(line);
      if (m) {
        const value = m[1].replace(/[ \t]+#.*$/, "").replace(/^["']|["']$/g, "").trim();
        return value || null;
      }
    }
    return null;
  }
  return null;
}

/**
 * Parse `lute --version` output (`lute 0.26.0`) into its version, or `null`.
 * @param {string} out
 * @returns {string | null}
 */
function parseCliVersion(out) {
  if (typeof out !== "string") return null;
  const m = /^lute[ \t]+(\S+)/m.exec(out);
  return m ? m[1] : null;
}

/**
 * What the running server is judged against, in order: the document's own
 * stamp, the project manifest's `defaults: luteVersion`, the `lute` CLI on
 * PATH. `null` when none is known.
 * @param {{ doc?: string | null, project?: string | null, cli?: string | null }} known
 * @returns {{ version: string, source: "document" | "project" | "cli" } | null}
 */
function versionTarget(known) {
  if (known.doc) return { version: known.doc, source: "document" };
  if (known.project) return { version: known.project, source: "project" };
  if (known.cli) return { version: known.cli, source: "cli" };
  return null;
}

/**
 * True when the server should be flagged against `target`: older than a
 * document or project stamp (newer is fine — a stale stamp is the checker's
 * `W-LUTE-VERSION-STALE`), or any different version from the `lute` CLI (the
 * editor and the terminal disagree). An uncomputable verdict never warns.
 * @param {string} serverVersion
 * @param {{ version: string, source: string } | null} target
 * @returns {boolean}
 */
function serverDisagrees(serverVersion, target) {
  if (!target) return false;
  const cmp = compareVersions(serverVersion, target.version);
  return target.source === "cli" ? cmp === -1 || cmp === 1 : cmp === -1;
}

/**
 * The user-facing warning shown once when a stale server is detected. Says
 * what disagrees, then the fix: `lute doctor` names the stale install.
 * @param {string} serverVersion
 * @param {{ version: string, source: "document" | "project" | "cli" }} target
 * @returns {string}
 */
function staleServerMessage(serverVersion, target) {
  const against = {
    document: `the document targets (luteVersion "${target.version}")`,
    project: `the project targets (lute.project.yaml defaults: luteVersion "${target.version}")`,
    cli: `the \`lute\` on PATH (${target.version})`,
  }[target.source];
  const relation = target.source === "cli" ? "differs from" : "is older than";
  return (
    `Lute: the editor's language server (lute-lsp ${serverVersion}) ${relation} ${against}, ` +
    "so its diagnostics may be wrong. Run `lute doctor` in a terminal to find the " +
    "stale install, then restart the editor (or point `lute.lsp.path` at a current binary)."
  );
}

module.exports = {
  parseFrontmatterLuteVersion,
  parseProjectLuteVersion,
  parseCliVersion,
  parseTriple,
  compareVersions,
  versionTarget,
  serverDisagrees,
  staleServerMessage,
};
