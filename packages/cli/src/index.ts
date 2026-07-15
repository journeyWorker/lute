#!/usr/bin/env node
// `lutecli`'s launcher: platform detection, binary resolution,
// argv/exit-code passthrough. Adapted line-for-line (design D2) from
// the sibling `canon` monorepo's `packages/cli/src/index.ts`'s
// `resolveTargetPackageName` / libc-kind probing / search-path order /
// self-reference guarding — this file is dense, adversarially-tested
// logic already hardened against real install-topology bugs; re-deriving
// it risks reintroducing them.
//
// Design deviation ("Workspace dev build takes priority over the packaged
// binary"): the search-path order here puts the workspace dev build FIRST
// (`target/<triple>/release/`, then `target/release/`), so `bun run dev`
// iteration inside the lute monorepo never silently picks up a stale
// installed `lutecli-core-<platform>` package.
import { spawnSync, execSync } from "node:child_process";
import { existsSync, readdirSync, realpathSync } from "node:fs";
import { resolve, join, basename } from "node:path";
import { fileURLToPath } from "node:url";

const binaryName = process.platform === "win32" ? "lute.exe" : "lute";

const currentDir = fileURLToPath(new URL(".", import.meta.url));
const dirName = basename(currentDir);
// In npm/bun install: currentDir = .../node_modules/lutecli/dist/
//   cliDir = .../node_modules/lutecli/
//   scopeDir = .../node_modules/           (unscoped: siblings are
//              node_modules/lutecli-core-<platform>/)
// In monorepo dev (dist): currentDir = .../packages/cli/dist/
//   cliDir = .../packages/cli/
//   scopeDir = .../packages/
// In monorepo dev (src): currentDir = .../packages/cli/src/
//   cliDir = .../packages/cli/
//   scopeDir = .../packages/
const isSubDir = dirName === "dist" || dirName === "src";
const cliDir = isSubDir ? resolve(currentDir, "..") : currentDir;
const scopeDir = resolve(cliDir, "..");
const workspaceRoot = resolve(scopeDir, "..");

type LibcKind = "gnu" | "musl";

function detectLibcKind(): LibcKind {
  const override = process.env.LUTE_LIBC?.trim().toLowerCase();
  if (override === "musl") return "musl";
  if (override === "gnu" || override === "glibc") return "gnu";

  const report = process.report?.getReport?.() as
    | {
        header?: {
          glibcVersionRuntime?: string;
          release?: { sourceUrl?: string };
        };
        sharedObjects?: string[];
      }
    | undefined;

  if (report?.header?.glibcVersionRuntime) {
    return "gnu";
  }

  if (
    Array.isArray(report?.sharedObjects) &&
    report.sharedObjects.some((obj) => obj.toLowerCase().includes("musl"))
  ) {
    return "musl";
  }

  // Bun reports neither glibcVersionRuntime nor sharedObjects, but its
  // release.sourceUrl names the build flavor (e.g. bun-linux-x64-musl-baseline.zip).
  if (report?.header?.release?.sourceUrl?.toLowerCase().includes("musl")) {
    return "musl";
  }

  try {
    const output = execSync("ldd --version", {
      encoding: "utf-8",
      stdio: ["ignore", "pipe", "pipe"],
    }).toLowerCase();
    if (output.includes("musl")) return "musl";
    if (output.includes("glibc") || output.includes("gnu")) return "gnu";
  } catch (error) {
    // musl's ldd rejects --version: it prints "musl libc" to stderr and
    // exits non-zero, so the answer is in the error, not the output.
    const { stdout, stderr } = (error ?? {}) as { stdout?: unknown; stderr?: unknown };
    const combined = `${stdout ?? ""}\n${stderr ?? ""}`.toLowerCase();
    if (combined.includes("musl")) return "musl";
    if (combined.includes("glibc") || combined.includes("gnu")) return "gnu";
  }

  // ldd missing or inconclusive: look for dynamic loaders. Either loader
  // can coexist with the other's libc (Debian's musl package installs
  // ld-musl-*; Alpine's gcompat installs ld-linux-*), so when both are
  // present, let the distro break the tie.
  const hasGnuLoader = loaderPresent("ld-linux-");
  const hasMuslLoader = loaderPresent("ld-musl-");
  if (hasGnuLoader !== hasMuslLoader) return hasMuslLoader ? "musl" : "gnu";
  if (hasGnuLoader && hasMuslLoader) {
    return existsSync("/etc/alpine-release") ? "musl" : "gnu";
  }

  return "gnu";
}

// Glibc ships ld-linux-*.so.* in /lib64 (or /lib on some arches); musl
// distros (Alpine, Void-musl, ...) ship /lib/ld-musl-<arch>.so.1.
function loaderPresent(prefix: string): boolean {
  for (const dir of ["/lib", "/lib64"]) {
    try {
      if (readdirSync(dir).some((entry) => entry.startsWith(prefix))) {
        return true;
      }
    } catch {
      // Directory unreadable or missing; try the next one.
    }
  }
  return false;
}

// Only the two S0 acceptance-bar platforms (design D3: macOS arm64, Linux
// x64 gnu) resolve to a real `lutecli-core-<platform>` package today.
// Every other platform/arch/libc combination returns null and hits the
// "Unsupported platform fails with an actionable error" scenario — adding
// a target later is a new `packages/core-<platform>/` + a CI matrix row,
// not a change to this function's shape (design non-goals).
function resolveTargetPackageName(): string | null {
  const arch = process.arch;

  if (process.platform === "darwin") {
    if (arch === "arm64") return "core-darwin-arm64";
    return null;
  }

  if (process.platform === "linux") {
    const libc = detectLibcKind();
    if (arch === "x64" && libc === "gnu") return "core-linux-x64";
    return null;
  }

  return null;
}

// Kept even though S0 only ships one Linux flavor (gnu-glibc x64): the
// libc-kind probing generalizes to musl targets without a rewrite (design
// D2), and the workspace-dev-build search path below wants the full Rust
// target triple regardless of whether a published platform package exists
// for it yet.
function resolveRustTargetTriple(): string | null {
  const arch = process.arch;

  if (process.platform === "darwin") {
    if (arch === "arm64") return "aarch64-apple-darwin";
    if (arch === "x64") return "x86_64-apple-darwin";
    return null;
  }

  if (process.platform === "linux") {
    const libc = detectLibcKind();
    if (arch === "arm64") {
      return libc === "musl" ? "aarch64-unknown-linux-musl" : "aarch64-unknown-linux-gnu";
    }
    if (arch === "x64") {
      return libc === "musl" ? "x86_64-unknown-linux-musl" : "x86_64-unknown-linux-gnu";
    }
    return null;
  }

  if (process.platform === "win32") {
    if (arch === "arm64") return "aarch64-pc-windows-msvc";
    if (arch === "x64") return "x86_64-pc-windows-msvc";
    return null;
  }

  return null;
}

const targetPackage = resolveTargetPackageName();
const rustTargetTriple = resolveRustTargetTriple();
const searchPaths: string[] = [];

// 1. Workspace dev build — always wins over an installed package, so
//    `cargo build` iteration never needs a fresh npm publish (native-launcher
//    spec, "Workspace dev build takes priority over the packaged binary").
if (rustTargetTriple) {
  searchPaths.push(join(workspaceRoot, "target", rustTargetTriple, "release", binaryName));
}
searchPaths.push(join(workspaceRoot, "target", "release", binaryName));

// 2. This package's own bundled bin/ (used when the launcher ships its own
//    binary directly rather than through an optionalDependency).
searchPaths.push(join(cliDir, "bin", binaryName));

// 3. The resolved `lutecli-core-<platform>` optionalDependency, in every
//    location a package manager might have placed it. The npm package is
//    UNSCOPED (`lutecli-core-<platform>`); `resolveTargetPackageName`
//    returns the short `core-<platform>` suffix, which doubles as the
//    monorepo `packages/<dir>` directory name, so name and dir are built
//    from the one suffix here.
const platformPkgName = targetPackage ? `lutecli-${targetPackage}` : null;
if (targetPackage) {
  searchPaths.push(
    // Monorepo development: prebuilt binary staged into packages/core-<platform>/bin
    join(workspaceRoot, "packages", targetPackage, "bin", binaryName),
  );
}
if (platformPkgName) {
  searchPaths.push(
    // npm/bun install: sibling unscoped package (node_modules/lutecli-core-<platform>/bin/...)
    join(scopeDir, platformPkgName, "bin", binaryName),
    // Nested node_modules: non-hoisted / pnpm
    join(cliDir, "node_modules", platformPkgName, "bin", binaryName),
    // Hoisted edge cases
    join(scopeDir, "node_modules", platformPkgName, "bin", binaryName),
    join(workspaceRoot, "node_modules", platformPkgName, "bin", binaryName),
  );
}

function tryRealpath(p: string): string {
  try {
    return realpathSync(p);
  } catch {
    return p;
  }
}

// Paths that would re-enter this wrapper if executed - using any of these as
// the "real" binary causes infinite recursion (a fork bomb). We compare by
// realpath so symlinks (e.g. npm/bun bin shims) are dereferenced.
const selfPaths = new Set<string>([
  tryRealpath(fileURLToPath(import.meta.url)),
  tryRealpath(join(cliDir, "bin.js")),
]);
if (process.argv[1]) {
  selfPaths.add(tryRealpath(process.argv[1]));
}

const binary = searchPaths.find((p) => existsSync(p) && !selfPaths.has(tryRealpath(p)));

if (!binary) {
  console.error(`lute: no binary found for ${process.platform}/${process.arch}`);
  if (platformPkgName) {
    console.error(`  expected optional package: ${platformPkgName}`);
  } else {
    console.error("  no lutecli-core-<platform> package exists for this platform/arch yet");
  }
  console.error("  build from source: cargo build --release -p lute-cli");
  process.exit(1);
}

const result = spawnSync(binary, process.argv.slice(2), { stdio: "inherit" });
process.exit(result.status ?? 1);
