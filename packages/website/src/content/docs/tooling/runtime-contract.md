---
title: Runtime contract
description: What a game engine must implement to run a compiled Lute artifact — the envelope, version negotiation and the IR 0.8.0 delta, the addr width invariant, and the dispatcher loop over the twenty-one command kinds.
---

Lute is a total, side-effect-free compiler. `lute compile <file>` checks a
`.lute` document and lowers it to a JSON artifact — and then stops. It runs
**no CEL, no Datalog fixpoint, keeps no fact store, fires no bridge**. Every
behavior lives on the far side of the artifact, in the **engine**. This page is
the condensed runtime contract; the full, source-grounded specification is in
[`docs/runtime/`](https://github.com/journeyWorker/lute/tree/main/docs/runtime)
and the machine-checkable shape is
[`schemas/lute-ir-0.8.schema.json`](https://github.com/journeyWorker/lute/blob/main/schemas/lute-ir-0.8.schema.json)
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
| `shots` | authored `## ` shot headings, `{shot, heading}` (omitted when empty). |

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

### What IR 0.8.0 changed

The schema file was renamed `schemas/lute-ir-0.7.schema.json` →
`schemas/lute-ir-0.8.schema.json` along with the minor bump. Three deltas
matter to a consumer:

- **`end` is a new command `kind`.** By the unknown-kind rule above, an engine
  implementing only IR 0.7 **must refuse** an artifact carrying one — it cannot
  fall through the record, because `end` terminates the walk and falling
  through would play content the author marked unreachable. This is why
  termination is a core kind rather than a plugin directive: a plugin directive
  lowers to `kind: "plugin"`, which an older engine would happily skip.
- **`shots` and the locale maps are append-only optional fields**, so by the
  ignore-unknown-fields rule a 0.7 engine still loads a 0.8 artifact that
  carries no `end` record.
- **The `addr` width invariant is new**, and it is the one change that can
  alter bytes in an artifact you already consume. See below.

## Addressing

Every executable record carries an `addr`, a position string
`"{shot}-{(index + 1) * 100}"` (e.g. `"001-0300"`). It is **regenerated on
every compile** — a position, not an identity. The stable content joins are
`lineId` / `voiceKey`.

Both segments are zero-padded to a width computed from the document — at least
`3` for the shot and `4` for the index, wider when the document needs it — and
that width is **uniform across the whole artifact**. The guarantee that follows
is the one you can rely on:

> Within one artifact, every emitted `addr` has the same length; therefore
> **lexicographic order over `addr` equals execution order.**

A document whose every shot emits fewer than 100 addresses, with fewer than
1000 shots, is byte-identical to what 0.7.0 produced — the widths only grow
past their minimums when the artifact actually needs them.

**If you may load artifacts built by a 0.7-or-earlier toolchain, compare `addr`
segment-wise numerically, never as a plain string.** Before 0.8.0 the index
segment was fixed at 4 digits, so a shot with 100+ records emitted `001-11500`
beside `001-1400` and string comparison reported `"001-11500" < "001-1400"` —
an engine ordering or range-checking addresses lexicographically would rewind
into already-played content.

## The dispatcher

The `commands` array is already in execution order. Control-flow fields —
`jump.target`, choice/hub option `target` and `converge`, match arm
`target`/`otherwise`/`converge` — are all [addrs](#addressing). Walk with a
program counter over an `addr → index` map, dispatching on `kind`:

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
    case "end":     finish(cmd.reason); return;  // terminates the walk
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

The full command set is twenty-one kinds: `line`, `background`, `music`, `sfx`,
`vfx`, `sprite`, `camera`, `cut`, `video`, `set`, `assert`, `retract`,
`choice`, `match`, `hub`, `jump`, `end`, `barrier`, `quest`, `on`, `plugin`.

`end` carries an optional free-form `reason` — an author string
(`"completed"`, an ending id) Lute assigns no meaning to and the host MAY
surface. Terminating on it is identical to running off the end of `commands`,
except the reason is available.

## Localized text

A `line` record's `text` and a choice/hub option's `label` are always the
**source language** (`contentLang`). When the artifact was built with
[`lute compile --locales`](/tooling/cli/#--locales--merge-a-translation-bundle),
the record also carries a `texts` map (option: `labels`), locale tag →
translated string, keyed on the record's `lineId`:

```json
{
  "kind": "line",
  "addr": "001-0100",
  "role": "narration",
  "speaker": "narrator",
  "text": "Welcome to your new Lute project.",
  "lineId": "narrator.s01ep01.narrator_0010",
  "texts": { "ja-JP": "Lute プロジェクトへようこそ。" }
}
```

Both maps are omitted when empty, so an artifact compiled without `--locales`
is byte-identical to before, and a consumer that ignores them keeps rendering
the source language. Present a locale by looking it up in `texts` and falling
back to `text` — the compiler warns at build time (`W-L10N-MISSING`) about
exactly those gaps, so a complete bundle leaves nothing to fall back to.

## The runtime docs

Each surface has its own contract document under
[`docs/runtime/`](https://github.com/journeyWorker/lute/tree/main/docs/runtime):

- **[execution-model.md](https://github.com/journeyWorker/lute/blob/main/docs/runtime/execution-model.md)** — the artifact shape, version gate, addressing, and the dispatcher loop.
- **[state-lifecycle.md](https://github.com/journeyWorker/lute/blob/main/docs/runtime/state-lifecycle.md)** — the `scene`/`run`/`user`/`app`/`quest.<id>` tiers, initialization, and reset boundaries.
- **[cel-and-facts.md](https://github.com/journeyWorker/lute/blob/main/docs/runtime/cel-and-facts.md)** — evaluating the `expr` AST, the fact store's assert/retract deltas, and the stratified least-fixpoint the engine computes.
- **[quest-lifecycle.md](https://github.com/journeyWorker/lute/blob/main/docs/runtime/quest-lifecycle.md)** — `start`/`fail` precedence, required vs. optional objectives, monotone completion, and lifecycle events.
- **[timeline-semantics.md](https://github.com/journeyWorker/lute/blob/main/docs/runtime/timeline-semantics.md)** — the local clock, per-track cursors, barriers, and the one-writer-per-target invariant the checker guarantees.
- **[bridge-protocol.md](https://github.com/journeyWorker/lute/blob/main/docs/runtime/bridge-protocol.md)** — typed bridge calls, return shapes, `wait`, and resolved state effects.
