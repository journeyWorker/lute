# @lute-lang/runtime

The Lute runtime for TypeScript hosts, in [Effect](https://effect.website) 4:
load a bundle compiled by `lute compile --all`, drive the resumable step
function, query the state (`view`, `evaluate`, `def`, `gate`, `candidates`, …),
and replay input logs, on Bun and in browsers.

```console
bun add @lute-lang/runtime effect@4.0.2 @effect/platform-bun@4.0.2 @effect/platform-node-shared@4.0.2
# For a local pack, use the runtime tarball path printed by scripts/pack-local.sh:
# bun add @lute-lang/runtime@file:/path/to/lute-lang-runtime-<version>.tgz effect@4.0.2 @effect/platform-bun@4.0.2 @effect/platform-node-shared@4.0.2
# or use the dependencies and overrides printed by scripts/pack-local.sh.
# or, for a browser application:
bun add @lute-lang/runtime effect@4.0.2 @effect/platform-browser@4.0.2
```

The host guide — sessions, the `Program` API, errors, state interchangeability,
replay, and local dogfooding with packed tarballs — is
[`docs/runtime/typescript-runtime.md`](https://github.com/journeyWorker/lute/blob/main/docs/runtime/typescript-runtime.md).

## Building from the repository

The package wraps the wasm binding of `crates/lute-runtime-wasm`. In
`packages/runtime`:

```console
bun install
bun run wasm:build      # debug binding -> src/wasm/
bun run build           # dist/: ESM entry points, declarations, the binding
bun run typecheck
bun run test            # builds the binding first (incremental)
```

`bun run wasm:build:release` builds the release binding and optimizes it with
`wasm-opt -O`; a published package ships that one.

### Tools

`wasm:build` needs Rust with the `wasm32-unknown-unknown` target and
**`wasm-bindgen` 0.2.126** — exactly the binding crate's `wasm-bindgen`
version. The script resolves it in this order:

1. `$WASM_BINDGEN`, a path to the binary;
2. `wasm-bindgen` on `PATH`;
3. the wasm-pack cache entry of the pinned version,
   `<cache>/.wasm-pack/wasm-bindgen-cargo-install-0.2.126/wasm-bindgen`
   (`wasm-bindgen.exe` on Windows), where `<cache>` is `~/Library/Caches` on
   macOS, `$XDG_CACHE_HOME` or `~/.cache` on Linux, and `%LOCALAPPDATA%` on
   Windows — wasm-pack leaves it there after any build of a crate pinned to
   that version.

When none resolves, or the binary cannot be run or reports another version,
the script prints

```text
wasm-build: need wasm-bindgen 0.2.126 (set WASM_BINDGEN, put it on PATH, or cargo install --locked wasm-bindgen-cli --version 0.2.126)
```

and exits 1. Install it with
`cargo install --locked wasm-bindgen-cli --version 0.2.126`.

`wasm:build:release` also needs binaryen's `wasm-opt`, from `$WASM_OPT` or
`PATH`.

The contract schemas under `src/generated/` are generated from the Rust types:
`bun run contract:gen` regenerates them and `bun run contract:check` fails on
drift. Never edit them by hand.
