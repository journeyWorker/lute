// The npm launcher end to end under `node`: a copy of the built launcher in a
// fresh install layout, with a shell stub as the binary in the launcher's own
// `bin/` (a search path that wins on every platform, since the layout has no
// workspace `target/`).
import { afterAll, beforeAll, expect, test } from "bun:test";
import { chmodSync, copyFileSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const cliSrc = resolve(import.meta.dir, "..");
let root: string;
let pkg: string;

beforeAll(async () => {
  root = mkdtempSync(join(tmpdir(), "lute-launcher-"));
  pkg = join(root, "node_modules", "@lute-lang", "lute");
  mkdirSync(join(pkg, "bin"), { recursive: true });
  const built = await Bun.build({
    entrypoints: [join(cliSrc, "src", "index.ts")],
    outdir: join(pkg, "dist"),
    target: "node",
    format: "esm",
  });
  if (!built.success) throw new Error(built.logs.join("\n"));
  copyFileSync(join(cliSrc, "bin.js"), join(pkg, "bin.js"));
  copyFileSync(join(cliSrc, "lsp-bin.js"), join(pkg, "lsp-bin.js"));
  writeFileSync(join(pkg, "package.json"), JSON.stringify({ type: "module" }));
});

afterAll(() => rmSync(root, { recursive: true, force: true }));

function stub(name: "lute" | "lute-lsp", body: string): void {
  const file = join(pkg, "bin", name);
  writeFileSync(file, `#!/bin/sh\n${body}\n`);
  chmodSync(file, 0o755);
}

function launch(entry: string, opts: { args?: string[]; stdin?: Buffer; preload?: string } = {}) {
  const env = { ...process.env };
  delete env.LUTE_LAUNCHER_BINARY;
  const node = opts.preload ? ["node", "--require", opts.preload] : ["node"];
  return Bun.spawnSync([...node, join(pkg, entry), ...(opts.args ?? [])], {
    env,
    stdin: opts.stdin,
  });
}

test("lute replaces the launcher: the binary's parent is the caller, not node", () => {
  stub("lute", 'echo "ppid:$PPID args:$*"');
  const r = launch("bin.js", { args: ["check", "a b"] });
  expect(r.exitCode).toBe(0);
  expect(r.stdout.toString().trim()).toBe(`ppid:${process.pid} args:check a b`);
  expect(r.stderr.toString()).toBe("");
});

test("lute-lsp resolves its own binary and replaces the launcher too", () => {
  stub("lute", 'echo wrong; exit 9');
  stub("lute-lsp", 'echo "lsp ppid:$PPID"');
  const r = launch("lsp-bin.js");
  expect(r.exitCode).toBe(0);
  expect(r.stdout.toString().trim()).toBe(`lsp ppid:${process.pid}`);
});

test("stdin reaches lute-lsp and the launcher writes nothing to stderr (LSP over stdio)", () => {
  stub("lute-lsp", "cat");
  const msg = 'Content-Length: 46\r\n\r\n{"jsonrpc":"2.0","id":1,"method":"initialize"}';
  const r = launch("lsp-bin.js", { stdin: Buffer.from(msg) });
  expect(r.exitCode).toBe(0);
  expect(r.stdout.toString()).toBe(msg);
  expect(r.stderr.toString()).toBe("");
});

test("without process.execve the launcher runs the binary as a child and keeps its exit status", () => {
  const preload = join(root, "no-execve.cjs");
  writeFileSync(preload, "process.execve = undefined;\n");
  stub("lute", 'echo "ppid:$PPID"; exit 3');
  const r = launch("bin.js", { preload });
  expect(r.exitCode).toBe(3);
  expect(r.stdout.toString().trim()).not.toBe(`ppid:${process.pid}`);
});
