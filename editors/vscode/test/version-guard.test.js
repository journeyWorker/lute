import { test, expect } from "bun:test";
import {
  parseFrontmatterLuteVersion,
  parseProjectLuteVersion,
  parseCliVersion,
  compareVersions,
  versionTarget,
  serverDisagrees,
  staleServerMessage,
} from "../version-guard.js";

test("extracts the luteVersion stamp from the frontmatter fence", () => {
  const doc =
    '---\nkind: scene\ncharacter: marina\nseason: 1\nepisode: 2\nluteVersion: "0.7.0"\n---\n## Shot 1.\n@narrator: hi.\n';
  expect(parseFrontmatterLuteVersion(doc)).toBe("0.7.0");
});

test("accepts an unquoted stamp and CRLF line endings", () => {
  expect(parseFrontmatterLuteVersion("---\r\nluteVersion: 0.6.0\r\n---\r\nbody\r\n")).toBe("0.6.0");
});

test("returns null when there is no frontmatter fence", () => {
  expect(parseFrontmatterLuteVersion("## Shot 1.\nluteVersion: \"0.7.0\"\n")).toBeNull();
});

test("returns null when the fence has no luteVersion key", () => {
  expect(parseFrontmatterLuteVersion("---\nkind: scene\n---\n@narrator: hi.\n")).toBeNull();
});

test("does not read a luteVersion that lives in the body", () => {
  // The `luteVersion:` here is after the closing fence — not a stamp.
  const doc = "---\nkind: scene\n---\n@narrator: luteVersion: 9.9.9\n";
  expect(parseFrontmatterLuteVersion(doc)).toBeNull();
});

test("orders numeric version triples", () => {
  expect(compareVersions("0.6.0", "0.7.0")).toBe(-1);
  expect(compareVersions("0.7.0", "0.7.0")).toBe(0);
  expect(compareVersions("0.7.1", "0.7.0")).toBe(1);
  expect(compareVersions("1.0.0", "0.9.9")).toBe(1);
});

test("yields no verdict for a non-triple / garbage version", () => {
  expect(compareVersions("0.7", "0.7.0")).toBeNull();
  expect(compareVersions("0.7.0", "latest")).toBeNull();
});

test("a stamp flags the server only when the server is strictly older", () => {
  const doc = (version) => versionTarget({ doc: version });
  // Server predates the document's target → stale (the pilot's failure).
  expect(serverDisagrees("0.6.0", doc("0.7.0"))).toBe(true);
  // Server current or ahead → not stale (a stale STAMP is the checker's job).
  expect(serverDisagrees("0.7.0", doc("0.7.0"))).toBe(false);
  expect(serverDisagrees("0.7.1", doc("0.7.0"))).toBe(false);
  // Uncomputable verdict → never warn.
  expect(serverDisagrees("0.7.0", doc("garbage"))).toBe(false);
  expect(serverDisagrees("0.7.0", null)).toBe(false);
});

test("reads the project stamp from lute.project.yaml's defaults block", () => {
  const block =
    'defaultProfile: core\ndefaults:\n  uses: [world.schema.yaml]\n  luteVersion: "0.26.0" # pinned\nprofiles:\n  core:\n    plugins: {}\n';
  expect(parseProjectLuteVersion(block)).toBe("0.26.0");
  expect(parseProjectLuteVersion("defaults: { luteVersion: '0.26.0', uses: [a] }\n")).toBe("0.26.0");
  // A `luteVersion` outside `defaults:` is not the project stamp.
  expect(parseProjectLuteVersion("profiles:\n  core:\n    luteVersion: 0.26.0\n")).toBeNull();
  expect(parseProjectLuteVersion("defaults:\n  uses: [a]\nluteVersion: 0.26.0\n")).toBeNull();
});

test("reads the CLI version from `lute --version`", () => {
  expect(parseCliVersion("lute 0.26.0\n")).toBe("0.26.0");
  expect(parseCliVersion("command not found")).toBeNull();
});

test("an unstamped document falls back to the project stamp, then the CLI", () => {
  expect(versionTarget({ doc: "0.26.0", project: "0.25.0", cli: "0.24.0" })).toEqual({
    version: "0.26.0",
    source: "document",
  });
  expect(versionTarget({ doc: null, project: "0.25.0", cli: "0.24.0" })).toEqual({
    version: "0.25.0",
    source: "project",
  });
  expect(versionTarget({ doc: null, project: null, cli: "0.24.0" })).toEqual({
    version: "0.24.0",
    source: "cli",
  });
  expect(versionTarget({ doc: null, project: null, cli: null })).toBeNull();
});

test("against the CLI, any different server version disagrees (round-5: a 0.17 server)", () => {
  const cli = versionTarget({ cli: "0.26.0" });
  expect(serverDisagrees("0.17.1", cli)).toBe(true);
  expect(serverDisagrees("0.27.0", cli)).toBe(true);
  expect(serverDisagrees("0.26.0", cli)).toBe(false);
  const project = versionTarget({ project: "0.26.0" });
  expect(serverDisagrees("0.17.1", project)).toBe(true);
});

test("the warning names what disagrees and sends the user to `lute doctor`", () => {
  const cli = staleServerMessage("0.17.1", { version: "0.26.0", source: "cli" });
  expect(cli).toContain("lute-lsp 0.17.1");
  expect(cli).toContain("differs from the `lute` on PATH (0.26.0)");
  expect(cli).toContain("Run `lute doctor`");
  const project = staleServerMessage("0.17.1", { version: "0.26.0", source: "project" });
  expect(project).toContain("is older than the project targets");
  expect(project).toContain("Run `lute doctor`");
});
