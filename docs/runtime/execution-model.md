# Runtime execution model

This directory is the **runtime contract**: what an engine must implement to
*consume* a compiled Lute artifact. Lute itself is a total, side-effect-free
compiler — it checks a `.lute` document and lowers it to the execution IR
described by the current schema (`0.36.4`; the file is
`lute-ir-0.36.schema.json`). It runs no CEL, no Datalog fixpoint, keeps no
fact store, and fires no bridge at compile time. Everything on the far side of
the execution IR is the engine's job.
These documents describe that job, grounded in `crates/lute-compile` and
`crates/lute-check`.

## Author-time project analysis (0.34.0)

`lute impact`, `lute constraints`, and the evidence fields on analysis
commands are author-time reports, not execution-IR commands. They do not add
runtime obligations: the host still executes the compiled artifact and owns
time advancement. A `bounded` or `heuristic` report must not be interpreted by
an engine as a universal runtime guarantee.

> **Permission boundary (0.17.0):** capability permissions reject forbidden
> authored effects before an artifact is emitted; they do not sandbox the
> engine or authorize its runtime resources. Hosts using permission ceilings
> must also follow
> [`capability-permissions.md`](capability-permissions.md).

## What Lute hands you

One execution IR is produced per `.lute` document (`lute compile <file>` →
`crates/lute-compile/src/lib.rs::compile`). Its shape is the `ExecutionIr`
struct (`ir.rs`):



- an **envelope** — `kind` (`"scene"` | `"quest"`), `lute` (language version),
  `irVersion` (the version you gate on), `capabilityVersion` (a snapshot hash),
  and `meta`. `meta.id` (present since dsl 0.15.0 §2) is the **canonical scene
  key** engines and tools join on — the string a `visited("…")` prereq
  resolves to, the prefix every `lineId` / `voiceKey` was derived from, and
  what `project.index.json` keys documents by. A quest keeps its authored quest
  id as identity in the same slot. The legacy scene-meta fields
  (`character` / `season` / `episode` / `episodeId`) are now optional and
  purely descriptive — emitted only when the source supplied them, never
  something a runtime rederives an id from. A scene whose `meta.beat` is
  present (dsl 0.21.0) is a **beat**: besides explicit flow, the engine may
  select it when it raises the named occasion (see
  [beats-and-occasions.md](./beats-and-occasions.md));
  an envelope field **`requiredSemantics`** immediately after
  `capabilityVersion`: a compiler-derived, sorted, duplicate-free list of
  semantic ids used by this artifact. Every executable artifact includes
  `lute.core/1`; authors cannot provide, remove, or reorder the list. The
  registry and trigger mapping are normative in the
  [0.33.0 proposal §2–§3](../proposals/scenario-dsl/0.33.0.md#2-semantic-id-registry).
- a **folded state table** — `state: StateEntry[]` (see
  [state-lifecycle.md](./state-lifecycle.md));
- a **declared vocabulary** — `entities` / `enums` / `relations` / `seedFacts` /
  `rules`, emitted as data; each is omitted when empty. `entities` /
  `relations` / `seedFacts` / `rules` feed your Datalog evaluator (see
  [cel-and-facts.md](./cel-and-facts.md)). **`enums` carries both** the
  relational enum domains AND, since dsl 0.9.0, the document's declared
  **content** vocabulary (`emotion`, `action`, `anchor`, `mood`, `volume`,
  `musicAction`, `vfxType`) when the project declares it in a schema the
  document imports — the compiler ships no members of its own, so the artifact
  is self-describing about the vocabulary it was compiled against. Each entry is
  `{ name, members }`; member-level semantics (`exits:`/`default:`) are **not**
  serialized, because the compiler has already resolved them into `sprite.exit`
  and the emitted anchor. A vocabulary supplied by a plugin `enums` export does
  **not** appear here — it is part of `capabilityVersion` instead. None of this
  changes the artifact *shape*, and an engine that ignores `enums` is unaffected;
- a flat, ordered **`commands: Command[]`** stream — the executable body;
- an advisory **`prereqEdges`** graph (this document's raw `after` / quest `follows` formulas;
  connectivity T13 — see [quest-lifecycle.md](./quest-lifecycle.md) for how
  cross-document reachability is out of scope for a single artifact).

A project's engine **unions** the per-document `relations` / `rules` /
`seedFacts` / `entities` / `enums` / `prereqEdges` across every artifact it
loads, exactly as it concatenates the command streams.

## Version negotiation

An engine MUST negotiate in this order, before it opens a playback session or
executes any command:

1. **Exact-minor IR gate.** For pre-1.0 IR, an engine implementing `0.33.x`
   accepts only `0.33.*` artifacts and refuses every other major.minor line
   (`0.32.*`, `0.34.*`, and so on). Patch policy is optional within `0.33`.
   From 1.0 onward, the gate changes to the released major's policy.
2. **Semantic capability gate.** Load the engine's immutable
   `lute.engine.yaml` matrix. Its required fields are `engine`, `irVersion`,
   and unique `supportedIds`; `version` and `description` are descriptive.
   Compare every artifact `requiredSemantics` entry with `supportedIds`.
   Unknown or malformed matrix ids are errors, and an unknown artifact id is
   `E-SEMANTICS-UNKNOWN`.

`lute run --engine <file>` and `lute play --engine <file>` perform both gates.
If any required id is absent, playback MUST be refused **before playback** —
no command, condition, asset, or bridge may run — with exit code 2 and
`E-ENGINE-SEMANTICS`. A malformed matrix uses `E-ENGINE-MATRIX`; an
unsupported exact-minor line uses `E-ENGINE-IR-VERSION`. The built-in
`reference` matrix supports every current registry id.

`lute check --engine <file>` and `lute check-project --engine <file>` perform
the same negotiation at author time, without playback. They report each
lowered construct whose semantic id is absent, including its source span,
semantic id, and engine name, as `E-CHECK-ENGINE-SEMANTICS`. The project form
negotiates the sorted union in its project index and identifies each
contributing document/span.

The project index carries `requiredSemantics`, the sorted union of its
document artifacts, so an engine can negotiate before opening every document.
Plugin snapshot compatibility remains the exact `capabilityVersion` check; it
is not part of the semantic matrix. The complete registry, trigger table,
matrix format, and version rules live in the
[0.33.0 proposal](../proposals/scenario-dsl/0.33.0.md#2-semantic-id-registry),
not in this runtime guide.

`lute` (the language version) is informational for the runtime and does not
gate.

## CEL slots and execution IR expressions

Every CEL slot is `{ "cel": "<standard CEL>", "expr": <exprNode>,
"authored"?: "<source when expanded>" }`; `expr` is present on every slot.
`cel` is authoritative for evaluation, while `expr` is a portable walker view.
The IR also carries `celEnv`, declaring exactly the roots and host functions
used by its expressions.

`exprNode` is the typed full-profile tree:

| Shape | Meaning |
|---|---|
| `{"int":3}`, `{"double":3.5}`, `{"bool":true}`, `{"string":"x"}` | typed literal |
| `{"list":[<node>,…]}` | list |
| `{"path":"a.b.c"}` | canonical state read |
| `{"index":<node>,"key":<node>}` | computed index |
| `{"has":"a.b.c"}` | presence |
| `{"op":"!"\|"-","l":<node>}` | unary |
| `{"op":<sym>,"l":<node>,"r":<node>}` | binary, including `in` |
| `{"cond":<node>,"then":<node>,"else":<node>}` | ternary |
| `{"call":"<name>","args":[<node>,…]}` | conversion or host call |

## Activation and evaluation

An engine evaluates each CEL slot against nested maps built from live state:

- each declared root and every intermediate map on the path to a declared slot
  is present;
- a non-reserved slot is present iff it has an effective value (write, else
  seed, else default), so `has(run.tip)` tests whether the slot is present;
- values retain their declared CEL types: `int`, `double`, `bool`, `string`,
  enum members as `string`, and lists as lists;
- reserved engine paths are always present with their reserved defaults: for
  every quest the IR declares, `quest.<id>.state` and
  `quest.<id>.failedBy` are `"unset"`; each objective's
  `quest.<id>.objectives.<oid>.done` and `.failed` are `false`; and every lore
  entry's `entry.<id>.read` and `.everRead` are `false`.

For a `<match>` whose subject is a bare state path with no effective value, an
`is` arm is satisfied only when its value is exactly `"unset"`; other `is`
values are definitely false. Other arms evaluate normally, and an evaluation
error means the arm is not satisfied.

> **History.** Through `0.12.0` the gate was **major.minor**, and consecutive
> releases (`0.11.0`, `0.12.0`) moved the gated line while changing nothing
> an engine reads. `0.13.0` relaxed the gate to MAJOR only for the 0.x line,
> but D6 now makes the pre-1.0 exception explicit: minors MAY change the IR
> shape, so engines pin the exact `irVersion` minor until 1.0. After 1.0,
> compatibility is governed by that major line's policy.

**IR `0.10.0` changes the shape** — one field rename, the first since `0.8.0`.
The injection provenance stamp's `reason` becomes **`explanation`**:
`{ injected: true, by: "auto-pose-reset", explanation: "…" }`. The old name
collided with `end.reason`, which is an opaque author token you dispatch on,
while this field is human-readable English the compiler wrote and nothing
dispatches on. Nothing else moves — no field added or retyped, no new command
`kind`, and `Provenance.injected` is retained but is now constant-`true`, so do
not read a `true` as distinguishing anything. The rename is published in the
current schema above; `schemas/lute-ir-0.9.schema.json` is the one older schema
file still kept in the repo, for an engine that has not crossed the rename. An
engine that reads the provenance stamp and consumes artifacts from both sides
of `0.10.0` must handle the rename itself; the version gate no longer refuses
on its behalf.

## Addressing and control flow

Every executable record carries an `addr` (`address.rs`), a position string
`"{shot}-{(index+1)*100}"` (e.g. `"001-0300"`). `addr` is **regenerated on
every compile** — it is a position, not an identity. The stable content joins
are `lineId` / `voiceKey`, derived from per-speaker `code` (dsl §12), and are
what you key localization and voice assets on. Under the default templates
(dsl 0.22.0 §11) a line's `lineId` is `{prefix}.{speaker}_{code}` and its
`voiceKey` `{prefix}.{speaker}-{code}`, with `{prefix}` = `meta.id`; a line
expanded from a component `::use` is minted under
`{prefix}.{component}#{instance}`, where `instance` is the authored key from
`instance="…"`, not a sibling ordinal. Missing keys are warning-bearing
positional fallbacks until `lute tag` writes them. A project may re-template
both (`identity:` in `lute.project.yaml` — a pinned
`voiceKey: "{speaker}-{code}"` restores the 0.21 keys), so treat them as
opaque keys and never parse them.

Artifacts may carry `identityRenames`, an optional sorted list of
`{from, to}` canonical `NodeKey` pairs. Engines apply these authored migrations
when loading save-shaped identity; they MUST NOT infer renames from position,
text, spans, or `addr`. The same list is exposed by `project.index.json`.

```lute check
---
kind: scene
id: identity-runtime
---
## Opening
@narrator{code="intro"}: Stable content.
```

**Field width (IR 0.8.0, dsl 0.8.0 §2).** Both segments are zero-padded to a
width computed from the document — at least `3` for the shot and `4` for the
index, wider when the document needs it — and that width is **uniform across
the whole artifact**. Therefore, *within one artifact, lexicographic order over
every emitted `addr` equals execution order.*

> Before 0.8.0 the index field was fixed at 4 digits, so a shot with 100+
> records emitted `001-11500` beside `001-1400` and string comparison reported
> `"001-11500" < "001-1400"` — an engine ordering or range-checking addresses
> lexicographically would rewind into already-played content. If you may load
> artifacts built by a 0.7-or-earlier toolchain, **compare `addr` segment-wise
> numerically**, never as a plain string.

The `commands` array is already in **final execution order**. The engine walks
it with a program counter, resolving control-flow targets — which are all
`addr` strings — against an `addr → index` map:

- **`jump.target`** — unconditional transfer.
- **`choice` / `hub`** — each option carries a `target` (taken when the option
  is chosen) and the record carries a `converge` addr (where control resumes
  after the construct). A `converge` may point "one past the last record" of
  the addressing unit, i.e. fall-through.
- **`match`** — each arm carries a `target`, plus an optional `otherwise` and a
  `converge`; an `is` field marks the semantic shorthand. When the subject is
  an unset bare state path, `is` values other than `"unset"` are definitely
  false, while `is: "unset"` is satisfied. Other arms evaluate normally;
  evaluation errors are not satisfied.
- **`quest` / `on`** — declaration heads: `objective.body` and `on.body` are
  `addr` targets into separately-emitted body segments (see
  [quest-lifecycle.md](./quest-lifecycle.md)).
- **`end`** — terminates the walk (dsl 0.8.0 §3); carries an optional free-form
  `reason`. Equivalent to running off the end of `commands`, except the reason
  is available to the host.
- **`barrier`** — a timeline join (see
  [timeline-semantics.md](./timeline-semantics.md)).

All control-flow targets are resolved to concrete addrs at compile time
(`Command::for_each_target`); an unresolved label is a compiler bug, never
shipped.

## Dispatcher loop

A minimal engine is a program counter over `commands`, dispatching on `kind`.
The kinds below are exactly the `Command` variants (`ir.rs`); an unknown `kind`
must halt with an error.

```ts
type Addr = string;

// Every CEL slot carries its verbatim source under its own key — `option.when`,
// `arm.test`, `set.value` — and the lowered portable `expr` AST (IR A7) ONLY
// when that CEL is inside the closed §8.4 profile. A relational fact query
// (`holds()`/`count()`) or a `visited()` read is outside it and carries CEL
// text alone, so this two-way read is mandatory, not an optimisation.
const evalSlot = (cel, expr, state, facts) =>
  expr !== undefined ? evalExpr(expr, state) : evalCel(cel, state, facts);

function run(artifact: ExecutionIr, state: StateStore, facts: FactStore) {
  assertExactMinorCompatible(artifact.irVersion); // pre-1.0 0.33.* gate
  assertRequiredSemantics(artifact.requiredSemantics, engineMatrix);

  // scene: one continuous command stream. quest: see quest-lifecycle.md —
  // `quest`/`on` records are declarations the lifecycle driver consults, not
  // sequential steps.
  const index = new Map<Addr, number>();
  artifact.commands.forEach((c, i) => index.set(c.addr, i));

  let pc = 0;
  while (pc < artifact.commands.length) {
    const cmd = artifact.commands[pc];
    let next: Addr | null = null; // null ⇒ fall through to pc + 1

    switch (cmd.kind) {
      // ── content & staging (all carry the optional Stamp fields:
      //    wait, duration, delay, at, timeline, provenance, source) ──
      case "line":       present(cmd, state); break; // substitute cmd.placeholders
      case "background": stageBackground(cmd); break;
      case "music":      stageMusic(cmd); break;
      case "sfx":        stageSfx(cmd); break;
      case "vfx":        stageVfx(cmd); break;
      case "sprite":     stageSprite(cmd); break; // cmd.stamp.provenance ⇒ injected
      case "camera":     stageCamera(cmd); break;
      case "cut":        stageCut(cmd); break;
      case "video":      stageVideo(cmd); break;

      // ── state & facts ──
      case "set":     writeState(state, cmd.path, cmd.op, evalSlot(cmd.value, cmd.expr, state, facts)); break;
      case "assert":  facts.assert(cmd.relation, cmd.args); break;   // positive delta
      case "retract": facts.retract(cmd.relation, cmd.args); break;  // negative delta (args may be "_")

      // ── control flow ──
      case "choice":
      case "hub": {
        const opt = pickOption(cmd, state); // per option: evalSlot(o.when, o.expr, …)
        next = opt ? opt.target : cmd.converge;
        break;
      }
      case "match": {
        const arm = cmd.arms.find(a => truthy(evalSlot(a.test, a.expr, state, facts)));
        next = arm ? arm.target : (cmd.otherwise ?? cmd.converge);
        break;
      }
      case "jump":    next = cmd.target; break;
      case "end":     finish(cmd.reason); return;      // dsl 0.8.0 §3 — terminate the walk
      case "barrier": joinTimeline(cmd.timeline, cmd.at); break; // see timeline-semantics.md

      // ── quest-kind declarations (consumed by the lifecycle driver) ──
      case "quest": registerQuest(cmd); break;
      case "on":    registerHandler(cmd); break;

      // ── quest acceptance (dsl 0.21.0 §7a.3; quest-lifecycle.md) ──
      case "accept": acceptQuest(cmd.quest); break; // activates it iff `unset`

      // ── plugin passthrough (bridge calls + resolved effects) ──
      case "plugin": callBridgeAndApplyEffects(cmd, state); break; // see bridge-protocol.md

      default:
        throw new UnknownCommandKind(cmd.kind); // version-negotiation: hard error
    }

    pc = next === null ? pc + 1 : index.get(next)!;
  }
}
```
`evalSlot` evaluates the slot's `cel` with the declared `celEnv`; an engine
may additionally use the typed `expr` walker for inspection or evaluation.
Facts remain the Datalog store and bridge commands remain host operations.
The execution IR is inert data; behavior begins in this dispatcher.

