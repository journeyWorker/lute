# TypeScript runtime

`@lute-lang/runtime` is the Effect 4 host API for Lute's resumable runtime. It
loads a bundle produced by `lute compile --all`, drives the wasm binding for
`lute-runtime`, records inputs, exposes events and awaits as streams, and
replays an input log. It does not render presentations, answer bridge calls,
pace dialogue, settle grants, or apply other game policy: those remain the
host's responsibility.

The package's platform-free entry point exports the namespaces `Bundle`,
`BundleSource`, `Contract`, `Errors`, `InputLog`, `LuteRuntime`, `LuteWasm`,
`Program`, `Replay`, and `Session`. The platform entry points provide layers:
`@lute-lang/runtime/bun` for Bun and `@lute-lang/runtime/browser` for a
browser. The package uses services and layers, scoped resources, typed
expected failures, and generated Effect `Schema` values rather than a promise
wrapper around an imperative API.

## Install

Install the runtime and Effect, plus the platform package for the host:

```console
bun add @lute-lang/runtime effect@4.0.2 @effect/platform-bun@4.0.2
# or, for a browser application:
bun add @lute-lang/runtime effect@4.0.2 @effect/platform-browser@4.0.2
```

`effect` is an exact peer dependency at `4.0.2`; the Bun and browser platform
peers are optional, but the selected platform layer requires its matching
package. `@lute-lang/runtime` is released in lockstep with the Lute toolchain.

## Bun

Compile a project to the directory that the bundle layer reads. `--all` is
important: the output contains `project.index.json` and every artifact named by
that index.

```console
lute compile --all path/to/project -o bundle
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

Serve the release binding and the complete `lute compile --all` directory from
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
the parsed `project.index.json` and an object mapping every index `artifact`
path to its parsed artifact. `BundleSource.directory(path)`,
`BundleSource.url(base)`, and `BundleSource.value(bundle)` construct the
source layer.

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

`RuntimeState` is an opaque branded handle. It is never serialized; the
`Snapshot` is its serialized form. States are immutable values: stepping never
invalidates the input handle, so a host can retain states for undo or
branching. A rejected step returns the unchanged state. The loaded program is
scoped and must not be used after its scope closes.

`Session.Session` is the host-facing driver built on `Program`. It exposes
`fingerprint`, `started`, `current` (`SubscriptionRef<SessionState>`),
`send`, scoped `events` and `awaits` streams, `save`, and `restore`.
`Session.layer({ seed })` begins a playthrough; `Session.layer({ snapshot })`
restores one. The session does not answer awaits or pace events.

## Errors and contract drift

Expected failures are typed Effect errors. Runtime rejections map one-to-one
to their contract code:

| Error class | Runtime code | Meaning |
|---|---|---|
| `Errors.RuntimeBusy` | `E-RUNTIME-BUSY` | The current await does not accept this input. |
| `Errors.RequestMismatch` | `E-RUNTIME-REQUEST` | An answer names a different request. |
| `Errors.OptionUnavailable` | `E-RUNTIME-OPTION` | The choice is absent or closed. |
| `Errors.BridgeShapeMismatch` | `E-RUNTIME-BRIDGE-SHAPE` | A bridge field is missing or mistyped. |
| `Errors.InvalidInput` | `E-RUNTIME-INPUT` | A seed or input names an unknown or incompatible value. |
| `Errors.RuntimeHalted` | `E-RUNTIME-HALTED` | The lineage halted; restore an earlier snapshot. |
| `Errors.IrVersionMismatch` | `E-RUNTIME-IR-VERSION` | The bundle is not an IR version this runtime executes. |
| `Errors.SnapshotProjectMismatch` | `E-RUNTIME-SNAPSHOT-PROJECT` | The snapshot belongs to another bundle fingerprint. |
| `Errors.SnapshotVersionMismatch` | `E-RUNTIME-SNAPSHOT-VERSION` | The snapshot is from another Event minor. |

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
contract; `schemars` emits `schemas/lute-events-0.39.schema.json` and
`schemas/lute-snapshot-0.39.schema.json`, and the package's code generator
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

The CLI `lute play --events` stream is the same contract: each line is a
`Contract.StreamLine` (`{ seed, output }`, `{ input, output }`, or a rejection
line), and its output carries `eventVersion: "0.39.0"`. Schema regeneration is
part of the contract-drift check; a change belongs in the Rust types first.
