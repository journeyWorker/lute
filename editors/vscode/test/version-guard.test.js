import { test, expect } from "bun:test";
import {
  parseFrontmatterLuteVersion,
  compareVersions,
  serverIsStale,
} from "../version-guard.js";

test("extracts the luteVersion stamp from the frontmatter fence", () => {
  const doc =
    '---\nkind: scene\ncharacter: bianca\nseason: 1\nepisode: 2\nluteVersion: "0.7.0"\n---\n## Shot 1.\n@narrator: hi.\n';
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

test("serverIsStale is true only when the server is strictly older", () => {
  // Server predates the document's target → stale (the pilot's failure).
  expect(serverIsStale("0.6.0", "0.7.0")).toBe(true);
  // Server current or ahead → not stale (a stale STAMP is the checker's job).
  expect(serverIsStale("0.7.0", "0.7.0")).toBe(false);
  expect(serverIsStale("0.7.1", "0.7.0")).toBe(false);
  // Uncomputable verdict → never warn.
  expect(serverIsStale("0.7.0", "garbage")).toBe(false);
});
