# Runtime execution model

This directory is the **runtime contract**: what an engine must implement to
*consume* a compiled Lute artifact. Lute itself is a total, side-effect-free
compiler — it checks a `.lute` document and lowers it to the JSON IR described
by [`schemas/lute-ir-0.7.schema.json`](../../schemas/lute-ir-0.7.schema.json).
It runs **no CEL, no Datalog fixpoint, keeps no fact store, fires no bridge**
(design decision D1). Everything on the far side of the artifact is the
engine's job. These documents describe that job, grounded in
`crates/lute-compile` (the IR) and `crates/lute-check` (the static guarantees
the engine may rely on).

## What Lute hands you

One artifact is produced per `.lute` document (`lute compile <file>` →
`crates/lute-compile/src/lib.rs::compile`). Its shape is the `Artifact` struct
(`ir.rs`):

- an **envelope** — `kind` (`"scene"` | `"quest"`), `lute` (language version),
  `irVersion` (the version you gate on), `capabilityVersion` (a snapshot hash),
  and `meta`;
- a **folded state table** — `state: StateEntry[]` (see
  [state-lifecycle.md](./state-lifecycle.md));
- a **relational vocabulary** — `entities` / `enums` / `relations` /
  `seedFacts` / `rules`, emitted as data for your Datalog evaluator (see
  [cel-and-facts.md](./cel-and-facts.md)); each is omitted when empty;
- a flat, ordered **`commands: Command[]`** stream — the executable body;
- an advisory **`prereqEdges`** graph (this document's raw `after` formulas;
  connectivity T13 — see [quest-lifecycle.md](./quest-lifecycle.md) for how
  cross-document reachability is out of scope for a single artifact).

A project's engine **unions** the per-document `relations` / `rules` /
`seedFacts` / `entities` / `enums` / `prereqEdges` across every artifact it
loads, exactly as it concatenates the command streams.

## Version negotiation

Gate parsing on `irVersion` by **major.minor** (spec §4.1, A9):

- accept any artifact whose `irVersion` major.minor you implement;
- refuse one from a newer major.minor (the PATCH component is an advisory,
  backward-compatible refinement and never gates);
- **ignore unknown object fields** — new optional fields are added append-only
  within a minor line, so a newer PATCH artifact still loads on an older engine
  of the same major.minor;
- treat an **unknown command `kind` as an error** — a new command kind is a
  real capability you cannot fake.

`lute` (the language version) is informational for the runtime and does not
gate. `capabilityVersion` lets you refuse an artifact compiled against a plugin
snapshot you do not match.

## Addressing and control flow

Every executable record carries an `addr` (`address.rs`), a position string
`"{shot:03}-{(index+1)*100:04}"` (e.g. `"001-0300"`). `addr` is **regenerated
on every compile** — it is a position, not an identity. The stable content
joins are `lineId` / `voiceKey`, derived from per-speaker `code` (dsl §12), and
are what you key localization and voice assets on.

The `commands` array is already in **final execution order**. The engine walks
it with a program counter, resolving control-flow targets — which are all
`addr` strings — against an `addr → index` map:

- **`jump.target`** — unconditional transfer.
- **`choice` / `hub`** — each option carries a `target` (taken when the option
  is chosen) and the record carries a `converge` addr (where control resumes
  after the construct). A `converge` may point "one past the last record" of
  the addressing unit, i.e. fall-through.
- **`match`** — each arm carries a `target`, plus an optional `otherwise` and a
  `converge`.
- **`quest` / `on`** — declaration heads: `objective.body` and `on.body` are
  `addr` targets into separately-emitted body segments (see
  [quest-lifecycle.md](./quest-lifecycle.md)).
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

function run(artifact: Artifact, state: StateStore, facts: FactStore) {
  assertVersionCompatible(artifact.irVersion); // major.minor gate

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
      case "set":     writeState(state, cmd.path, cmd.op, evalExpr(cmd.expr, state)); break;
      case "assert":  facts.assert(cmd.relation, cmd.args); break;   // positive delta
      case "retract": facts.retract(cmd.relation, cmd.args); break;  // negative delta (args may be "_")

      // ── control flow ──
      case "choice":
      case "hub": {
        const opt = pickOption(cmd, state); // eligibility via evalExpr(opt.expr)
        next = opt ? opt.target : cmd.converge;
        break;
      }
      case "match": {
        const arm = cmd.arms.find(a => truthy(evalExpr(a.expr, state)));
        next = arm ? arm.target : (cmd.otherwise ?? cmd.converge);
        break;
      }
      case "jump":    next = cmd.target; break;
      case "barrier": joinTimeline(cmd.timeline, cmd.at); break; // see timeline-semantics.md

      // ── quest-kind declarations (consumed by the lifecycle driver) ──
      case "quest": registerQuest(cmd); break;
      case "on":    registerHandler(cmd); break;

      // ── plugin passthrough (bridge calls + resolved effects) ──
      case "plugin": callBridgeAndApplyEffects(cmd, state); break; // see bridge-protocol.md

      default:
        throw new UnknownCommandKind(cmd.kind); // version-negotiation: hard error
    }

    pc = next === null ? pc + 1 : index.get(next)!;
  }
}
```

`evalExpr` walks the portable `expr` AST (IR A7) carried alongside every guard;
`facts` is your Datalog store; `callBridgeAndApplyEffects` is the host bridge.
Each is specified in its own document here. Nothing above evaluates anything at
*compile* time — the artifact is inert data, and this loop is where behavior
lives.
