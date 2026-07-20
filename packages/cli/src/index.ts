#!/usr/bin/env node
// `@lute-lang/lute`'s launcher: platform detection, binary resolution,
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
// installed `@lute-lang/lute-core-<platform>` package.
import { spawnSync, execSync } from "node:child_process";
import { existsSync, readdirSync, realpathSync } from "node:fs";
import { resolve, join, basename } from "node:path";
import { fileURLToPath } from "node:url";

const binaryName = process.platform === "win32" ? "lute.exe" : "lute";

const currentDir = fileURLToPath(new URL(".", import.meta.url));
const dirName = basename(currentDir);
// In npm/bun install (SCOPED under @lute-lang): currentDir = .../node_modules/@lute-lang/lute/dist/
//   cliDir = .../node_modules/@lute-lang/lute/
//   scopeDir = .../node_modules/@lute-lang/   (the npm scope dir; the sibling
//              platform package is node_modules/@lute-lang/lute-core-<platform>/)
//   workspaceRoot = .../node_modules/
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

// The three shipped platforms (macOS arm64, Linux x64 gnu, Windows x64
// msvc) resolve to a real `@lute-lang/lute-core-<platform>` package.
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

  if (process.platform === "win32") {
    if (arch === "x64") return "core-win32-x64";
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

// 3. The resolved platform optionalDependency, in every location a package
//    manager might have placed it. Packages are SCOPED under @lute-lang:
//    the launcher is `@lute-lang/lute` and each platform binary is
//    `@lute-lang/lute-core-<platform>`. `resolveTargetPackageName` returns
//    the short `core-<platform>` suffix, which doubles as the monorepo
//    `packages/<dir>` directory name; both the scope-relative suffix
//    (`lute-<suffix>`) and the full scoped name are derived from it here.
const platformSuffix = targetPackage ? `lute-${targetPackage}` : null;
const platformPkgName = platformSuffix ? `@lute-lang/${platformSuffix}` : null;
if (targetPackage) {
  searchPaths.push(
    // Monorepo development: prebuilt binary staged into packages/core-<platform>/bin
    join(workspaceRoot, "packages", targetPackage, "bin", binaryName),
  );
}
if (platformSuffix && platformPkgName) {
  searchPaths.push(
    // npm/bun install: sibling inside the same @lute-lang/ scope dir
    // (node_modules/@lute-lang/lute-core-<platform>/bin/...). A scoped
    // install nests one extra level, so scopeDir IS the @lute-lang/ scope
    // dir and the platform package sits right next to the launcher.
    join(scopeDir, platformSuffix, "bin", binaryName),
    // Nested node_modules: non-hoisted / pnpm
    join(cliDir, "node_modules", platformPkgName, "bin", binaryName),
    // Hoisted to an ancestor node_modules
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
    console.error("  no @lute-lang/lute-core-<platform> package exists for this platform/arch yet");
  }
  console.error("  build from source: cargo build --release -p lute-cli");
  process.exit(1);
}

const result = spawnSync(binary, process.argv.slice(2), { stdio: "inherit" });
process.exit(result.status ?? 1);
