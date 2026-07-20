---
title: Runtime contract
description: What a game engine must implement to run a compiled Lute artifact — the envelope and version-negotiation policy, the dispatcher loop over command kinds, and the split between what Lute proves and what the engine executes.
---

Lute is a total, side-effect-free compiler. `lute compile <file>` checks a
`.lute` document and lowers it to a JSON artifact — and then stops. It runs
**no CEL, no Datalog fixpoint, keeps no fact store, fires no bridge**. Every
behavior lives on the far side of the artifact, in the **engine**. This page is
the condensed runtime contract; the full, source-grounded specification is in
[`docs/runtime/`](https://github.com/journeyWorker/lute/tree/main/docs/runtime)
and the machine-checkable shape is
[`schemas/lute-ir-0.7.schema.json`](https://github.com/journeyWorker/lute/blob/main/schemas/lute-ir-0.7.schema.json)
(JSON Schema draft 2020-12).

## What Lute does vs. what the engine does

| Lute (compile time) | Engine (runtime) |
| ------------------- | ---------------- |
| Statically check the document; refuse to emit on any error. | Trust the artifact — it compiled clean. |
| Fold the state schema into an init/type table. | Initialize state from that table; own the tier lifetimes. |
| Lower every CEL guard to a portable `expr` AST. | **Evaluate** guards against live state. |
| Emit facts, `assert`/`retract` deltas, and Datalog rules as **data**; prove the rules are stratified and safe. | **Compute the minimal model** (least fixpoint) over the fact store. |
| Emit quests, objectives, and `<on>` handlers as **declarations**. | **Derive** the quest lifecycle from `start`/`fail`/objective completion. |
| Resolve plugin bridge calls and their state-write bindings. | **Make the call** and apply the effects. |
| Schedule timeline clips and prove no write races. | Replay the schedule (or run tracks concurrently) and honor the barrier. |

The through-line: Lute proves *shape and structure*; the engine supplies
*evaluation and effect*. Lute's static analyses are also honest about their
limits — reachability is conservative under the declared `after:` routes,
relational fact gates yield **Unknown** verdicts behind a human-review
boundary, and `lute trace` walks one deterministic mock-driven path, never a
proof of all paths.

## The envelope

Every artifact opens with a fixed envelope (the `Artifact` struct in
`crates/lute-compile/src/ir.rs`):

| field | meaning |
| ----- | ------- |
| `kind` | `"scene"` \| `"quest"` — read first; selects `meta`'s shape. |
| `lute` | language-version pin (informational for the runtime). |
| `irVersion` | the IR schema version you **gate on**. |
| `capabilityVersion` | a plugin-snapshot hash; refuse a mismatch. |
| `meta` | scene meta (`character`/`season`/`episode`/`episodeId`) or quest meta. |
| `state` | the folded init/type table. |
| `entities` / `enums` / `relations` / `seedFacts` / `rules` | the relational vocabulary (omitted when empty). |
| `commands` | the flat, ordered, addressed command stream. |
| `prereqEdges` | advisory raw `after` prerequisite edges (omitted when empty). |

One artifact is produced per document. A project's engine **unions** the
`relations` / `rules` / `seedFacts` / `entities` / `enums` / `prereqEdges`
across every artifact it loads, exactly as it concatenates the command streams.

## Version negotiation

Gate on `irVersion` by **major.minor**:

- **Accept** any artifact whose `irVersion` major.minor you implement.
- **Refuse** one from a newer major.minor — the PATCH component is an advisory,
  backward-compatible refinement and never gates.
- **Ignore unknown object fields** — optional fields are added append-only
  within a minor line, so a newer PATCH artifact still loads on an older engine.
- **Treat an unknown command `kind` as an error** — a new command kind is a
  real capability you cannot fake.

## The dispatcher

The `commands` array is already in execution order. Each record carries an
`addr` (a regenerated position string); control-flow fields — `jump.target`,
choice/hub option `target` and `converge`, match arm `target`/`otherwise`/
`converge` — are all addrs. Walk with a program counter over an `addr → index`
map, dispatching on `kind`:

```ts
const index = new Map(artifact.commands.map((c, i) => [c.addr, i]));
let pc = 0;
while (pc < artifact.commands.length) {
  const cmd = artifact.commands[pc];
  let next: string | null = null; // null ⇒ fall through to pc + 1

  switch (cmd.kind) {
    // content & staging
    case "line":       present(cmd, state); break; // substitute cmd.placeholders
    case "background": case "music": case "sfx": case "vfx":
    case "sprite":     case "camera": case "cut": case "video":
      stage(cmd); break;

    // state & facts
    case "set":     writeState(state, cmd.path, cmd.op, evalExpr(cmd.expr, state)); break;
    case "assert":  facts.assert(cmd.relation, cmd.args); break;
    case "retract": facts.retract(cmd.relation, cmd.args); break;

    // control flow
    case "choice":
    case "hub": {
      const opt = pickOption(cmd, state);          // eligibility via evalExpr(opt.expr)
      next = opt ? opt.target : cmd.converge; break;
    }
    case "match": {
      const arm = cmd.arms.find(a => truthy(evalExpr(a.expr, state)));
      next = arm ? arm.target : (cmd.otherwise ?? cmd.converge); break;
    }
    case "jump":    next = cmd.target; break;
    case "barrier": joinTimeline(cmd.timeline, cmd.at); break;

    // quest declarations & plugin bridges
    case "quest":   registerQuest(cmd); break;
    case "on":      registerHandler(cmd); break;
    case "plugin":  callBridgeAndApplyEffects(cmd, state); break;

    default: throw new UnknownCommandKind(cmd.kind); // version gate: hard error
  }

  pc = next === null ? pc + 1 : index.get(next)!;
}
```

The full command set is twenty kinds: `line`, `background`, `music`, `sfx`,
`vfx`, `sprite`, `camera`, `cut`, `video`, `set`, `assert`, `retract`,
`choice`, `match`, `hub`, `jump`, `barrier`, `quest`, `on`, `plugin`.

## The runtime docs

Each surface has its own contract document under
[`docs/runtime/`](https://github.com/journeyWorker/lute/tree/main/docs/runtime):

- **[execution-model.md](https://github.com/journeyWorker/lute/blob/main/docs/runtime/execution-model.md)** — the artifact shape, version gate, addressing, and the dispatcher loop.
- **[state-lifecycle.md](https://github.com/journeyWorker/lute/blob/main/docs/runtime/state-lifecycle.md)** — the `scene`/`run`/`user`/`app`/`quest.<id>` tiers, initialization, and reset boundaries.
- **[cel-and-facts.md](https://github.com/journeyWorker/lute/blob/main/docs/runtime/cel-and-facts.md)** — evaluating the `expr` AST, the fact store's assert/retract deltas, and the stratified least-fixpoint the engine computes.
- **[quest-lifecycle.md](https://github.com/journeyWorker/lute/blob/main/docs/runtime/quest-lifecycle.md)** — `start`/`fail` precedence, required vs. optional objectives, monotone completion, and lifecycle events.
- **[timeline-semantics.md](https://github.com/journeyWorker/lute/blob/main/docs/runtime/timeline-semantics.md)** — the local clock, per-track cursors, barriers, and the one-writer-per-target invariant the checker guarantees.
- **[bridge-protocol.md](https://github.com/journeyWorker/lute/blob/main/docs/runtime/bridge-protocol.md)** — typed bridge calls, return shapes, `wait`, and resolved state effects.
