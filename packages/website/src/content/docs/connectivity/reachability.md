---
title: Reachability
description: How Lute proves whether a node has a satisfiable route under the declared after graph, the Reachable / Unreachable / Unknown verdicts, per-node cycle degradation, and lute scenario reach.
---

Once the [scene graph](/connectivity/scene-graph/) is assembled, `check-project` asks whether each node has at least one **satisfiable route from the project's entry set** — computed by a memoized structural recursion over the formula AST, never by enumerating routes:

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

`lute scenario <dir> reach <nodeId>` reports one of three verdicts plus the node's declared `after` prerequisite structure:

- **Reachable** — a satisfiable route exists under the declared `after` graph.
- **Unreachable** — no declared route reaches the node (`E-CONN-UNREACHABLE`).
- **Unknown** — the node is on or downstream of a cycle, so its prerequisite ordering is unresolvable (see below).

`<nodeId>` is a scene's canonical key (e.g. `bianca.s01ep02`) or `quest:<id>` for a quest.

```console
$ lute scenario . reach narrator.s01ep02
project root: .
reach scene(narrator.s01ep02):
  verdict: Reachable — a satisfiable route exists under your declared routes.
  after: active("findkai")
  referenced node(s) (see `after` above for the && / || structure — this is NOT a flat requirement list):
    - quest(findkai): Reachable — a satisfiable route exists under your declared routes.
```

The referenced-node list is deliberately not a checklist: it reports each atom's own verdict, and the `after:` line above it is the only place the `&&` / `||` structure lives. A node whose formula is a disjunction needs only one of them.

## Cycle degradation is per-node

`E-CONN-CYCLE` marks a malformed ordering but does **not** blank the whole project root. Reachability is computed over the graph's natural topological order: a node enters that order once every prerequisite edge resolves, which recursively fails only for cycle members and nodes structurally downstream of them. So a node topologically independent of a cycle still receives its full, sound verdict; only nodes on or downstream of a cycle degrade to `Unknown`.

One accepted conservative gap: because the edge model over-approximates `||` position, a node reachable *only* via a disjunct that passes through a cyclic node is conservatively reported degraded even though its independent disjunct could prove it reachable. This is sound — **a false `Unknown` is always safe; a false `Reachable` never is** — and recovering it needs SCC-condensation-aware analysis (future work).

Under the locked A-hybrid enforcement posture the graph is advisory data the engine *may* honor, so every reachability message is worded "under your declared `after` routes," never as an unconditional runtime claim.
