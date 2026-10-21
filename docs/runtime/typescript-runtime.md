# TypeScript runtime

`@lute-lang/runtime` is the Effect 4 host API for Lute's resumable runtime. It
loads a bundle produced by `lute compile --project`, drives the wasm binding for
`lute-runtime`, records inputs, exposes events and awaits as streams, and
replays an input log. It does not render presentations, answer bridge calls,
pace dialogue, settle grants, or apply other game policy: those remain the
host's responsibility.

The package's platform-free entry point exports the namespaces `Bundle`,
`BundleSource`, `Contract`, `Errors`, `Ir`, `InputLog`, `LuteRuntime`,
`LuteWasm`, `Program`, `Replay`, and `Session`. The platform entry points provide layers:
`@lute-lang/runtime/bun` for Bun and `@lute-lang/runtime/browser` for a
browser. The package uses services and layers, scoped resources, typed
expected failures, and generated Effect `Schema` values rather than a promise
wrapper around an imperative API.

## Install

Install the runtime and Effect, plus the platform packages for the host. Bun
hosts need both platform peers on the same exact Effect release line:

```console
bun add @lute-lang/runtime effect@4.0.2 @effect/platform-bun@4.0.2 @effect/platform-node-shared@4.0.2
# For a local pack, use the runtime tarball path printed by scripts/pack-local.sh:
# bun add @lute-lang/runtime@file:/path/to/lute-lang-runtime-<version>.tgz effect@4.0.2 @effect/platform-bun@4.0.2 @effect/platform-node-shared@4.0.2
```

For a browser application:

```console
bun add @lute-lang/runtime effect@4.0.2 @effect/platform-browser@4.0.2
```

`effect` is an exact peer dependency at `4.0.2`; the Bun, browser, and
node-shared platform peers are optional, but the selected platform layer
requires its matching packages. `@lute-lang/runtime` is released in lockstep
with the Lute toolchain. To run a host against an unreleased checkout, pack
the packages with `scripts/pack-local.sh`; it prints the tarball
`dependencies` and `overrides` entries to paste into the host's
`package.json`.

## Bun

Compile a project to the directory that the bundle layer reads. `compile --project <dir> -o <dir>` (no
document operand) is important: the output contains `project.index.json` and every artifact named by
that index.

```console
lute compile --project path/to/project -o bundle
```

This is a complete minimal session. It subscribes to `events` inside the
session scope, raises the `visit` occasion used by the conformance hub
project, answers its first open `awaitChoice`, and saves the resulting
snapshot. A real host normally chooses an option using its own UI rather than
choosing the first open option as this example does.

```ts
import { Effect, Stream } from "effect"
import { Session } from "@lute-lang/runtime"
import * as LuteBun from "@lute-lang/runtime/bun"

const play = Effect.gen(function* () {
  const session = yield* Session.Session

  // `events` is a scoped subscription. forkScoped keeps it alive until the
  // surrounding Effect.scoped program closes.
  const events = yield* session.events
  yield* Effect.forkScoped(
    Stream.runForEach(events, (event) =>
      Effect.sync(() => console.log("event", JSON.stringify(event)))
    )
  )

  const output = yield* session.send({
    type: "raiseOccasion",
    occasion: "visit"
  })

  if (output.await.type === "awaitChoice") {
    const option = output.await.menu.options.find((item) => item.verdict === "open")
    if (option === undefined) {
      return yield* Effect.die(new Error("the visit menu has no open option"))
    }
    yield* session.send({
      type: "choose",
      request: output.await.request,
      option: option.id
    })
  }

  const snapshot = yield* session.save
  console.log("saved", JSON.stringify(snapshot))
})

await Effect.runPromise(
  Effect.scoped(
    play.pipe(
      Effect.provide(
        LuteBun.session({
          bundle: "bundle",
          seed: {}
        })
      )
    )
  )
)
```

`LuteBun.session` composes the packaged wasm layer, a directory
`BundleSource`, and an in-memory `InputLog` (unless an input-log layer is
provided through the `log` option). The session layer owns its loaded
`Program`; closing the scope frees it and ends the `events` and `awaits`
streams. `send` calls are serialized. An accepted input is logged before its
events are published; a rejected input is neither applied nor logged.

For a persistent Bun log, pass a layer implementing `InputLog.InputLog` as
`log`, for example `InputLog.keyValueStore(key)` with Bun's platform services.
The low-level composition is also available as `LuteBun.wasm`,
`LuteBun.runtime`, and `LuteBun.bundle(directory)`.

## Browser

Serve the release binding and the complete `lute compile --project` directory from
the same application (or from URLs allowed by the application's fetch/CORS
policy). The binding file is
`dist/wasm/lute_runtime_wasm_bg.wasm`; the bundle URL must expose
`project.index.json` and each artifact path listed by that index.

```ts
import { Effect } from "effect"
import { Session } from "@lute-lang/runtime"
import * as LuteBrowser from "@lute-lang/runtime/browser"

const app = Effect.gen(function* () {
  const session = yield* Session.Session
  console.log(session.started.eventVersion)
})

await Effect.runPromise(
  Effect.scoped(
    app.pipe(
      Effect.provide(
        LuteBrowser.session({
          wasm: "/dist/wasm/lute_runtime_wasm_bg.wasm",
          bundle: "/bundle/",
          log: "my-game-input-log",
          seed: {}
        })
      )
    )
  )
)
```

The browser layer exports `wasm(url)`, `runtime(url)`, `bundle(base)`,
`inputLog(key)`, and `session(options)`. `wasm(url)` fetches and instantiates
the binding; `bundle(base)` fetches the index and its artifacts;
`inputLog(key)` persists entries in `localStorage`; and `session` composes
those layers. The browser layer uses Effect's fetch and platform services and
has no direct promise API.


## Program API

`LuteRuntime` loads a `Bundle` into a scoped `Program`. A `Bundle` consists of
the `project.index.json`, typed by the generated `Ir.projectIndex` (0.42), and
an object mapping every index `artifact` path to its parsed artifact.
`BundleSource.directory(path)`, `BundleSource.url(base)`, and
`BundleSource.value(bundle)` construct the source layer; the first two decode
the index with `Ir.projectIndex` and fail with `BundleMalformed` when it does
not fit (a key the schema does not know included).

A loaded `Program` has this platform-free interface:

| Operation | Result | Purpose |
|---|---|---|
| `begin(seed)` | `Effect<Step, RuntimeRejection>` | Start a lineage and return its state and first output. |
| `step(state, input)` | `Effect<Step, RuntimeRejection>` | Apply one input and return the successor state and output. |
| `snapshot(state)` | `Effect<Snapshot>` | Encode state at an input point. |
| `restore(snapshot)` | `Effect<RuntimeState, RuntimeRejection>` | Restore a snapshot for this program's bundle. |
| `candidates(state, occasion, target?)` | `Effect<ReadonlyArray<Candidate>>` | Read candidates in selection order. |
| `eligibility(state, beat, member?)` | `Effect<Option<Candidate>>` | Read one beat's candidate, if present. |
| `clock(state)` | `Effect<Option<ClockAt>>` | Read the current clock position, if the project has one. |
| `terminal(state)` | `Effect<boolean>` | Test the state's terminal condition. |
| `view(state, { facts? })` | `Effect<WorldView>` | Read effective state, quests, optional facts, and clock. |
| `evaluate(state, expr)` | `Effect<Evaluation, QueryRejection>` | Evaluate an IR `exprNode` (the `expr` of a `{cel, expr}` pair, e.g. an objective's `visibleWhen.expr`, or the output of `lute expr`) as a walk's guard reads it. |
| `def(state, name, args?)` | `Effect<Evaluation, QueryRejection>` | Evaluate the index def `name`, its parameters bound to `args` (0.41). |
| `gate(state, occasion, target?)` | `Effect<Gate, InvalidInput>` | Whether a `raiseOccasion` of `occasion` (for `target`) with no writes would pass the seam now (0.41). |

### Clock values and labels

`Program.clock(state)` is the numeric clock position: `day` and `slot`
(the zero-based slot index). It does not return a slot name or a weekday label.
Read those presentation values from `program.view(state).clock`, whose
`ClockView` is `{ day, slot, weekday, weekdayLabel, ended, last }` (the
optional fields are `null` or absent when that clock does not declare them).
For example, a HUD can keep the numeric position for ordering while displaying
the view's labels:

```ts
import { Effect, Option } from "effect"

const hud = Effect.gen(function* () {
  const position = yield* program.clock(state)
  const world = yield* program.view(state)
  if (Option.isNone(position) || world.clock == null) return "No clock"
  const clock = world.clock
  return `${clock.weekdayLabel ?? `Day ${clock.day}`} — ${
    clock.slot ?? `slot ${position.value.slot}`
  }`
})
```

Here `program` is a loaded `Program` and `state` is its `RuntimeState`; the
snippet intentionally uses `ClockView.slot` for the authored slot name and
`ClockView.weekdayLabel` for the display label.

`Evaluation` is `{ value: Value, unresolved?: string[] }`: `value` is `bool`,
`int`, `double`, `str`, or `unknown` (with `unresolved` naming why), never
`error`. `Errors.QueryRejection` is `InvalidInput` (`E-RUNTIME-INPUT`) or
`EvaluationError` (`E-RUNTIME-EVAL`, the condition evaluates to an error).
`InvalidInput` covers a malformed `exprNode` — including a `param` node, which
only a def body carries — a def the index does not carry, and a `def`
argument fault. A host that wants a boolean gate decides what `unknown` means
for it; the runtime's own guards treat it as undecided. See
`docs/runtime/event-contract.md` for the activation the queries read.

`def` takes `args: ReadonlyArray<string | number | boolean>`, one JSON scalar
per declared parameter in declaration order; omit it (or pass `[]`) for a
parameterless def. The wrong number of arguments, or an argument that does
not fit its parameter type, is `InvalidInput`: an `int` takes an integer
within ±2^53, a `double` any number, a `bool` a boolean, a `string` a string,
an `enum` one of its members, and a `domain` or `entity` a string, checked
against the index's `enums` / `entities` members when the kind is closed.

```ts
// defs: { atLeast: { type: bool, params: { n: int }, cel: "run.count >= n" } }
const ready = yield* program.def(state, "atLeast", [2])
```

`gate` answers `Gate` `{ open: boolean, unresolved?: string[] }` by the raise
rule a `step` applies: `{ open: false }` is decided closed — the occasion's
`raisedWhen` is false, or the game is over and the occasion is not
`outsideRun`; `open: false` with `unresolved` is undecided, one reason per
unresolved read (raising it would halt `incomplete`). `target` maps through
the occasion's target domain as a raise maps it (`room.office` or `office`).
An occasion the bundle does not declare is `InvalidInput`. A raise the gate
closes is rejected with `Errors.OccasionGateClosed` and leaves the state
unchanged, so a host may also just send it and handle the rejection.

`RuntimeState` is an opaque branded handle. It is never serialized; the
`Snapshot` is its serialized form. States are immutable values: stepping never
invalidates the input handle, so a host can retain states for undo or
branching. A rejected step returns the unchanged state. The loaded program is
scoped and must not be used after its scope closes.

A `RuntimeState` is valid for every `Program` loaded from a bundle with the
same fingerprint as the program that created it: a host may query the state of
a session it does not drive (a player's, for example) with its own program of
that bundle. Each handle records the fingerprint of the program that created
it, and a `Program` operation given a state of another fingerprint dies — a
defect, never a typed error, like using a closed program.

`Session.Session` is the host-facing driver built on `Program`; its service
type is `Session.SessionShape`. It exposes `fingerprint`, `program`,
`started`, `current` (`SubscriptionRef<SessionState>`), `send`, scoped
`events` and `awaits` streams, `save`, and `restore`. `program` is the
`Program` the layer loaded, valid for the session's scope: query `current`'s
state through it rather than loading the bundle a second time.
`Session.layer({ seed })` begins a playthrough; `Session.layer({ snapshot })`
restores one. The session does not answer awaits or pace events.

```ts
const session = yield* Session.Session
const { state } = yield* SubscriptionRef.get(session.current)
const talk = yield* session.program.gate(state, "talk", "npc.tilly")
```

## Errors and contract drift

Expected failures are typed Effect errors. Runtime rejections map one-to-one
to their contract code:

| Error class | Runtime code | Meaning |
|---|---|---|
| `Errors.RuntimeBusy` | `E-RUNTIME-BUSY` | The current await does not accept this input. |
| `Errors.RequestMismatch` | `E-RUNTIME-REQUEST` | An answer names a different request. |
| `Errors.OptionUnavailable` | `E-RUNTIME-OPTION` | The choice is absent or closed. |
| `Errors.BridgeShapeMismatch` | `E-RUNTIME-BRIDGE-SHAPE` | A bridge field is missing or mistyped. |
| `Errors.InvalidInput` | `E-RUNTIME-INPUT` | A seed, input, or query names an unknown or incompatible value. |
| `Errors.RuntimeHalted` | `E-RUNTIME-HALTED` | The lineage halted; restore an earlier snapshot. |
| `Errors.IrVersionMismatch` | `E-RUNTIME-IR-VERSION` | The bundle is not an IR version this runtime executes. |
| `Errors.SnapshotProjectMismatch` | `E-RUNTIME-SNAPSHOT-PROJECT` | The snapshot belongs to another bundle fingerprint. |
| `Errors.SnapshotVersionMismatch` | `E-RUNTIME-SNAPSHOT-VERSION` | The snapshot is from another Event minor. |
| `Errors.OccasionGateClosed` | `E-OCCASION-GATE` | The engine would not make this raise: its `raisedWhen` is false, or the game is over and the occasion is not `outsideRun` (0.41). The state is unchanged; the message is `lute play`'s refusal text. |
| `Errors.EvaluationError` | `E-RUNTIME-EVAL` | An `evaluate`/`def` condition evaluates to an error (not a `RuntimeRejection`). |

`Program.gate` fails only with `InvalidInput`.

`Errors.BundleError` is the union of `BundleNotFound`, `BundleUnreadable`, and
`BundleMalformed`, each identifying the path concerned. `Errors.WasmLoadError`
means the binding could not be fetched, read, or instantiated.
`Errors.InputLogError` means a persistent input log could not be read or
written. `Session.send` and `Session.restore` can carry this error in addition
to a `RuntimeRejection`.

These are not recovery cases:

- A runtime rejection code unknown to this package is a defect, because it
  means the generated contract and the binding have drifted.
- Output, snapshot, and query JSON that fails the generated `Schema`, or a
  binding result with neither a value nor a rejection, is a defect.
- A wasm panic/trap is a defect, not a typed runtime rejection.

## Replay

`Replay.parseLog(text)` decodes non-empty `inputs.jsonl` lines with the
contract schema. It returns an Effect failing with `Schema.SchemaError` for a
malformed host log. The first entry must be `{ seed }` or `{ snapshot }`,
followed by `{ input }` entries. `Replay.run(program, entries)` returns a
`Stream<Output, RuntimeRejection>`: a seed emits `begin`'s output, a snapshot
start emits no initial output, and each input emits one output. Against the
same bundle, replay reproduces every serialized output.

```ts
import { Effect, Stream } from "effect"
import { Replay, Program } from "@lute-lang/runtime"

const replay = (program: Program.Program, text: string) =>
  Effect.gen(function* () {
    const entries = yield* Replay.parseLog(text)
    yield* Stream.runForEach(Replay.run(program, entries), (output) =>
      Effect.sync(() => console.log(JSON.stringify(output)))
    )
  })
```

## Generated contract

`Contract` is the generated Effect-schema namespace. Rust types in
`crates/lute-runtime` are the single definition of the event and snapshot
contract; `schemars` emits `schemas/lute-events-0.48.schema.json` and
`schemas/lute-snapshot-0.48.schema.json`, and the package's code generator
emits `packages/runtime/src/generated/Contract.ts` from those schemas. Do not
hand-edit the generated TypeScript.

The `Contract` namespace exports one schema and one `Type` alias per contract
definition (`Input`, `Seed`, `Output`, `Event`, `Await`, `Snapshot`, query
results, and their component types). It also exports `parseOptions`, which is
configured with `onExcessProperty: "error"`; use it when decoding or encoding
contract JSON so unknown fields are rejected rather than silently accepted.
The generated schemas use tagged unions for `Input`, `Event`, `Await`,
`Verdict`, and `Premise`, and preserve the Rust serde shapes and canonical JSON
ordering.

`Ir` is generated the same way from `schemas/lute-ir-*.schema.json`: the
command and artifact shapes and, since 0.41, the `project.index.json` types
(`Ir.projectIndex`, `Ir.indexBeat`, `Ir.indexOccasion`, `Ir.indexDef`,
`Ir.fieldType`, …) that type `Bundle.index`.

The CLI `lute play --events` stream is the same contract: each line is a
`Contract.StreamLine` (`{ seed, output }`, `{ input, output }`, or a rejection
line), and its output carries `eventVersion: "0.48.1"`. Schema regeneration is
part of the contract-drift check; a change belongs in the Rust types first.

## Local dogfooding

To run a host against an unreleased checkout of this repository, pack the
packages and depend on the tarballs. From the repository root:

```console
bash scripts/pack-local.sh [--web] [OUT_DIR]
```

The script builds `@lute-lang/runtime` (`bun run wasm:build`, then
`bun run build`) and `@lute-lang/player` (`bun run build`), plus
`@lute-lang/player-web` with `--web`, packs each with `bun pm pack` into
`OUT_DIR` (default `dist/local`, git-ignored), and prints the
`dependencies` and `overrides` blocks to paste into the host's
`package.json`, each package pointing at its tarball by absolute path:

```json
  "dependencies": {
    "@lute-lang/runtime": "file:/path/to/lute/dist/local/lute-lang-runtime-<version>.tgz",
    "@lute-lang/player": "file:/path/to/lute/dist/local/lute-lang-player-<version>.tgz"
  },
  "overrides": {
    "@lute-lang/runtime": "file:/path/to/lute/dist/local/lute-lang-runtime-<version>.tgz",
    "@lute-lang/player": "file:/path/to/lute/dist/local/lute-lang-player-<version>.tgz"
  }
```

Keep the host's own `effect` at `4.0.2` (an exact peer of every package) and
its platform package (`@effect/platform-bun` or `@effect/platform-browser`),
then `bun install`. The `overrides` make every transitive `@lute-lang/*`
reference resolve to the same tarball, so the host loads one runtime and one
`effect`. Re-run the script and `bun install` after each change in the
repository. `wasm:build` needs `wasm-bindgen` 0.2.126; see
[`packages/runtime/README.md`](../../packages/runtime/README.md) for how the
script finds it. The CLI is not packed (its npm launcher needs a platform
binary package): use a released `lute` or `cargo run -p lute-cli`.

Do not link the package directories instead (`"file:../lute/packages/runtime"`
and the like). Bun installs such a dependency with its `package.json` linked
back into this repository, so `@lute-lang/runtime` imports this repository's
own `effect`: the host's `Schema` and the runtime's generated schemas then come
from two `effect` instances, and decoding with them dies (`TypeError: Cannot
convert a symbol to a string`). Directory links also install each package's
`devDependencies` and can create a second runtime entry.
