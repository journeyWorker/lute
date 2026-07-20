/* tslint:disable */
/* eslint-disable */

/**
 * Statically validate `src` against `lute.core` and return the serialized
 * [`lute_check::CheckResult`] — the SAME JSON shape `lute check --json`
 * prints (`ok`, `diagnostics[]` each with `severity`/`code`/`message`/`span`,
 * plus the best-effort `resolved` view). Never throws: a parse/semantic error
 * is reported as diagnostics inside the result, not as an exception.
 */
export function check_source(src: string): string;

/**
 * Compile `src` to its typed JSON IR artifact, gated on `check` (D6). Returns
 * `{"ok":true,"artifact":{…}}` with the full artifact object on a clean check,
 * or `{"ok":false,"diagnostics":[…]}` (same diagnostic shape as
 * [`check_source`]) when the gate fails. Never throws.
 */
export function compile_source(src: string): string;

/**
 * Install the panic hook once, automatically on module init (wasm-bindgen
 * runs a `start` function after the generated `init()` resolves). A Rust
 * panic then prints a readable stack to the browser console instead of a
 * bare `RuntimeError: unreachable`.
 */
export function start(): void;

/**
 * Trace `src` under the mock state in `mock_yaml` (the SAME YAML the CLI's
 * `--mock` file accepts), returning `{"exit":"complete|refused|incomplete",
 * "report":{…}}`. On `complete`/`incomplete` `report` is the full
 * [`lute_trace::TraceReport`]; on `refused` `report` carries the
 * `diagnostics` the CLI would print. A malformed mock YAML is itself a
 * `refused` exit whose diagnostics hold the synthetic `E-TRACE-MOCK-PARSE`
 * message — never a throw.
 */
export function trace_source(src: string, mock_yaml: string): string;

/**
 * The three independent version axes (docs/versioning.md), matching
 * `lute version --json`: `{"toolchain":"0.7.0","language":"0.7.0","ir":"0.7.0"}`.
 * The toolchain axis is this crate's workspace `CARGO_PKG_VERSION`.
 */
export function version(): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly check_source: (a: number, b: number) => [number, number];
    readonly compile_source: (a: number, b: number) => [number, number];
    readonly trace_source: (a: number, b: number, c: number, d: number) => [number, number];
    readonly version: () => [number, number];
    readonly start: () => void;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
