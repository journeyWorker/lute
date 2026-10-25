---
title: Reachability
description: How Lute proves whether a node has a satisfiable route under the declared after graph, the Reachable / Unreachable / Unknown verdicts, per-node cycle degradation, and lute scenario reach.
---

Once the [scene graph](/connectivity/scene-graph/) is assembled, whole-project `lute check` asks whether each node has at least one **satisfiable route from the project's entry set** — computed by a memoized structural recursion over the formula AST, never by enumerating routes:

```
reachable(visited(Y)) = ¬unreachable(Y)
reachable(completed(Q)) = ¬E-QUEST-UNREACHABLE(Q)
reachable(active(Q))    = ¬E-QUEST-UNREACHABLE(Q)
reachable(X && Y) = reachable(X) ∧ reachable(Y)
reachable(X || Y) = reachable(X) ∨ reachable(Y)
```

`active(Q)` and `completed(Q)` are **identical here**. Both assert "that node must be reachable before this one," both resolve to the same `quest(Q)` node, and both contribute the same edge to the precedence DAG — so `E-CONN-CYCLE`, `E-CONN-UNREACHABLE`, and `E-CONN-UNKNOWN-NODE` behave exactly as they did before `active` existed, and `active` atoms count toward `E-CONN-FORMULA-TOO-COMPLEX` like any other. The distinction between the two only surfaces in [envelopes](/connectivity/envelopes/), which ask what a route *wrote*, not whether it exists. Reachability asks the weaker question and gets the same answer either way.

Any node with an absent or empty `after` is a graph entry point (`reachable = true` trivially) — no separate "declare the start" convention is needed. Because the grammar excludes negation, every formula is monotone, so each node's verdict is computed once and memoized over the topological order: linear, no blowup. A node with no satisfiable route is `E-CONN-UNREACHABLE` — the one connectivity error that needs no hedge, because it is a pure fact about the *authored* graph's self-consistency.

## Verdicts

`lute scenario --project <DIR> reach --node <NODE>` reports one of three verdicts plus the node's declared `after` prerequisite structure:

- **Reachable** — a satisfiable route exists under the declared `after` graph.
- **Unreachable** — no declared route reaches the node (`E-CONN-UNREACHABLE`).
- **Unknown** — the node is on or downstream of a cycle, so its prerequisite ordering is unresolvable (see below).

`<nodeId>` is a scene's canonical key (e.g. `marina.s01ep02`) or `quest:<id>` for a quest.

```console
$ lute scenario --project . reach --node narrator.s01ep02
project root: .
reach scene(narrator.s01ep02):
  verdict: Reachable — a satisfiable route exists under your declared routes.
  after: active("findkai")
  referenced node(s) (see `after` above for the && / || structure — this is NOT a flat requirement list):
    - quest(findkai): Reachable — a satisfiable route exists under your declared routes.
```

The referenced-node list is deliberately not a checklist: it reports each atom's own verdict, and the `after:` line above it is the only place the `&&` / `||` structure lives. A node whose formula is a disjunction needs only one of them.

## Endings

`lute scenario --project <DIR> reach --endings[=<OCCASION>]` (0.27.0) checks every ending at once: with `=<OCCASION>`, every beat answering that occasion; bare, every beat whose content can run `::end`. Each row puts the `after:` verdict above beside the `when` verdict whole-project `lute check` reaches — `E-BEAT-UNREACHABLE` / `E-ENTRY-UNREACHABLE` (it never holds) or `W-BEAT-SHADOWED` (it never wins) — and, for a `when` nothing refutes, the state paths and facts it reads with who produces them:

```console
$ lute scenario --project . reach --endings=termEnd
project root: .
endings (the beats answering `termEnd`):
  end.other.keeper (beat, lore/endings/other.lute): reachable
    after: Reachable — a satisfiable route exists under your declared routes.
    when: `run.route == 'hotaru'` — whole-project `check` does not refute it; it needs:
      run.route — written by scene `common.festival` (choice into)
  …
  end.ren.lantern (beat, lore/endings/ren.lute): reachable
    after: Reachable — a satisfiable route exists under your declared routes.
    when: `run.route == 'ren' && @devoted(run.aff.ren)` — whole-project `check` does not refute it; it needs:
      run.aff.ren — written by beat `hangouts.council` via component `hangout`, scene `common.arrival`, scene `common.clash`, scene `common.injury`, scene `ren.confession`, scene `ren.date`
      run.route — written by scene `common.festival` (choice into)
  …
10 ending(s): 10 reachable, 0 unreachable, 0 unknown
reachable here means nothing static refutes it, not that a run reaches it: a play that presents the ending is the proof — `lute test --coverage` lists the beats no play presents.
```

"Reachable" here means nothing static refutes the ending. Whether the writes can bring `run.aff.ren` to the value the `when` needs is not analysed; a play that presents the ending is the proof, and `lute test --coverage` lists the beats no play presents. A path nothing writes says `nothing writes it`, the sound case.

## Cycle degradation is per-node

`E-CONN-CYCLE` marks a malformed ordering but does **not** blank the whole project root. Reachability is computed over the graph's natural topological order: a node enters that order once every prerequisite edge resolves, which recursively fails only for cycle members and nodes structurally downstream of them. So a node topologically independent of a cycle still receives its full, sound verdict; only nodes on or downstream of a cycle degrade to `Unknown`.

One accepted conservative gap: because the edge model over-approximates `||` position, a node reachable *only* via a disjunct that passes through a cyclic node is conservatively reported degraded even though its independent disjunct could prove it reachable. This is sound — **a false `Unknown` is always safe; a false `Reachable` never is** — and recovering it needs SCC-condensation-aware analysis (future work).

Under the locked A-hybrid enforcement posture the graph is advisory data the engine *may* honor, so every reachability message is worded "under your declared `after` routes," never as an unconditional runtime claim.

## Start cycles and unavoidable deadlocks

A quest's `start` condition is part of the graph too. Every `start` anchor edge is drawn by `lute scenario`, including an edge that closes a cycle (0.48.1 silently dropped the closing edge).

A `start` keeps its AND/OR structure. Each top-level `&&` conjunct is one **anchor**, and every anchor must hold. An anchor's sources are alternatives: one source is enough.

A quest is **deadlocked** when it can never start and sits on a cycle of anchor edges. This is the least fixed point of *can-start*:

1. A node that is not a quest with anchors is decided by the reachability analysis above: scenes, beats and entries are routable unless they are unreachable or on an `after` cycle. A quest without anchors can start.
2. A quest can start when **every** anchor has **some** source that can start (a quest) or is routable (another node).
3. Iterate from "nothing can start" until nothing changes.

Each deadlocked quest is `E-QUEST-START-CYCLE` (error, project-wide) at its `start`, one per strongly connected set, reported at its least quest. The path is the shortest anchor cycle, rendered like `E-CONN-CYCLE`:

```text
quest start cycle: quest(cycA) -> quest(cycB) -> quest(cycA) — each `start` waits for the next and no source outside the cycle starts any of them; start one of them from something outside the cycle, or grant it with `accept="external"`
```

**An externally entered cycle is fine.** If quest A starts when B is active, and B starts when A is active **or** `visited("entry")`, and scene `entry` is routable, then B can start, so A can start: no diagnostic. Both edges are still drawn, and the two quests are layered after `entry` where the topological order allows; otherwise they are listed unlayered.

Output for a deadlock:

- The graph text prints the deadlocked quests on a line `(unlayered -- part of a start cycle, E-QUEST-START-CYCLE): …`. JSON and DOT carry `startCycle`. The text for an `after` cycle is unchanged.
- `reach` for a deadlocked quest, or one downstream only through deadlocked sources, is `Unknown` with the reason ``this quest's `start` waits on a start cycle (E-QUEST-START-CYCLE): <path>; it can never start under your declared routes.`` Reachability never reports `Reachable` for a quest that cannot start.

### A `start` that reads its own quest

Any read of `quest.<own id>.*` in a quest's own `start` is `E-QUEST-START-SELF` (error, per document, at `start`): the state has no value before the quest starts, so the condition can never start it. A quest only the host grants is written `accept="external"` without a `start`.

An anchoring self-read counts as a source that never helps, so a quest whose anchor has only that source is deadlocked: it and its dependents are not `Reachable`. A self-only deadlock is reported as `E-QUEST-START-SELF`, not `E-QUEST-START-CYCLE`.

### Unanchored quests

An unanchored quest is `reachable holds [proven]` with the reason `unanchored: available from the start of play`. `lute scenario reach` and `lute constraints` read the same single reachability claim, so they cannot disagree.

## Edges that reward effects draw

A plugin reward kind can declare [effects](/language/quests-and-scenes/#reward-effects) (0.50), and the graph reads them at the grant point of the reward's owner (the quest's completion or failure, or the objective's completion):

- A reward that **asserts** a fact draws the fact edge to a quest whose `start` reads that fact, as an `::assert` in a handler would. `lute scenario --facts` prints it for a quest that is in the graph (one with `follows`), as in the plugin-surfaces example: `quest(firstVisit) -> quest(secondVisit) [met(mira)]`. A quest gated on `holds('met', ['mira'])` is therefore reachable when a reward of `firstVisit` asserts `met(mira)`: no `E-QUEST-UNREACHABLE`. A conditional reward (`when=`) is a conditional producer.
- A reward that **accepts** a quest draws `quest(first) -> quest(second) [accept]`, the edge a scene's `::accept` draws. An accept-driven quest named only by a reward `accepts` is reachable, and draws no `W-QUEST-NEVER-ACCEPTED`. That warning's message now reads "… but no `::accept` or reward `accepts` in the project names it …".
- **Start cycles are unchanged.** `[accept]` anchors, a reward accept's included, do not gate a quest's start: the engine may accept a quest outside any `::accept`, and accept deadlocks are out of scope. A cycle of accept-driven quests that accept each other only through rewards never draws `E-QUEST-START-CYCLE` or `E-QUEST-START-SELF`.
- `lute scenario knowledge` and `lute lore` name the producer of a reward-asserted fact: `` met(mira) — produced by reward BADGE of quest `firstVisit` ``.

## Occasions that nothing raises

An occasion declared `raisedBy: declared` is raised by the host only through a directive's or reward kind's [declared `raises`](/plugins/manifests/#declared-occasion-raises) (0.51). A scene or bundle beat that answers such an occasion, with no `after:`, is anchored to the nodes whose raises reach it, and the graph draws a `[raises]` edge from each (see [the scene graph](/connectivity/scene-graph/#edges-that-declared-raises-draw)).

- **Reachability reads the anchor as it reads every anchor.** For an anchored node, `Unreachable` reads `Unknown`: a declared raise is a host promise, not a proof.
- **`W-OCCASION-NEVER-RAISED`** (warning, project-wide) is the analogue of `W-QUEST-NEVER-ACCEPTED`. It is reported at the `on:` of each scene or beat that answers a `raisedBy: declared` occasion (for a target, or targetless) that no declared raise reaches: `` scene `chapter2` answers `cutscene` for `scene.chapter2`, which `acme.stage` declares `raisedBy: declared`, but no `effects.raises` in the project raises it — add a `raises` entry to the directive or reward kind that leads here, or drop `raisedBy: declared` if the host raises `cutscene` on its own ``.
- **The clock is not a declared raise.** An occasion the project clock raises (its `raise:` map) is raised outside any declared raise, so for it `raisedBy: declared` draws no anchor and `W-OCCASION-NEVER-RAISED` stays silent.
- **A play step is no source.** A `lute play` or `lute test` step that raises the occasion does not count: a test proves a test, not the game.
- **Clock analyses count declared raises.** A declared raise of a clock-mapped occasion may deliver it at any clock position, not only where the clock's `raise:` map schedules it. For a scene or beat that some declared raise reaches, the checker therefore no longer claims the occasion is raised only at the scheduled positions: schedule-only claims such as `W-BEAT-UNRAISED` ("never plays") do not fire for it. A genuine `when`-window failure is still reported: `W-CHAPTER-STALL` can still fire for a chain whose next scene's `when` has expired at every position. Answerers that no declared raise reaches are judged as before.

## Evidence in 0.34.0

Scenario reach results include `evidence` in JSON. A structural reachability
proof is `proven`; a cycle-degraded or undecided result is `unknown`. Human
wording remains scoped to the declared `after` routes and never claims a
bounded analysis proves that no runtime path exists.
