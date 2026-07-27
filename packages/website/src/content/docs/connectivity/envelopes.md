---
title: Guaranteed / Possible envelopes
description: The per-node available-state analysis — the Guaranteed and Possible tables over run/user state, why active(Q) is strictly weaker than completed(Q), the Possible-minus-Guaranteed warning, and quest addressing including the bare-quest defaults-only answer.
---

The **envelope** answers a proactive question: *by the time control reaches node X, what state is actually set?* This is distinct from what's legal to read (governed by schema import). The envelope tracks two sets per node, scoped to **`run.*` / `user.*` only** — the tiers whose writes are monotonic ("once set, stays set," so union/intersect over predecessors is sound). Quest scratch fields (`quest.<id>.*`) are excluded; "was it reachable at X" is answered directly by `completed(Q)` / `active(Q)` in the route structure.

- **Guaranteed(X)** — set on *every* declared route to X.
- **Possible(X)** — set on *some* declared route to X.

Both are a graph-lift of `defassign`'s own lattice: each node's guaranteed-write set is computed by the definite-assignment walk, then propagated by structural recursion over the formula AST:

```
visited(Y):   G = Guaranteed(Y) ∪ G(Y)      P = Possible(Y) ∪ P(Y)
completed(Q): G = P = writesOnComplete(Q)
active(Q):    G = ∅                         P = writesOnComplete(Q)
X && Y:       G = G(X) ∪ G(Y)               P = P(X) ∪ P(Y)
X || Y:       G = G(X) ∩ G(Y)               P = P(X) ∪ P(Y)
entry node:   G = P = D
```

`D`, the entry base case, is the set of `run.*`/`user.*` paths carrying a schema `default` — reused verbatim from the import layer. `D ⊆ Guaranteed(n)` at *every* node, matching definite-assignment's "a defaulted path is always assigned" invariant lifted to the whole graph. This structural recursion is provably identical to a per-route ∩/∪ computation, never materializing the exponential route set.

## `active(Q)` is strictly weaker than `completed(Q)`

This is the one place the two quest atoms diverge. `completed(Q)` licenses the assumption `quest.Q.state == complete`, so everything the quest's required objectives and its `<on quest complete>` handlers write — `writesOnComplete(Q)` — lands on **both** sides of the table. `active(Q)` licenses only "`Q` reached `active`": those completion writes have not necessarily run, so they contribute to **Possible** and to Guaranteed not at all.

Take a quest `findkai` whose required objective writes `run.kaiFound`, a scene gated `after: 'active("findkai")'` that reads it, and a second scene gated `after: 'completed("findkai")'`. Only the second one is safe:

```console
$ lute scenario . envelope narrator.s01ep02
project root: .
envelope for scene(narrator.s01ep02) (pre-entry — state available when control REACHES this node, before its own writes):
  Guaranteed (safe to read under your declared routes):
    - run.greeted
  Possible (set on at least one declared route reaching this node):
    - run.greeted
    - run.kaiFound
  Possible \ Guaranteed -- warning-grade reads (…):
    - ./scenes/whileActive.lute:14:17: state path `run.kaiFound` is set under your declared routes on SOME routes reaching this node, but not every one — not yet guaranteed (dsl §4.3)
```

Swap that node's `after` to `completed("findkai")` and `run.kaiFound` moves into Guaranteed, leaving `Possible \ Guaranteed` empty. (`run.greeted` carries a schema `default`, so it is in `D` and guaranteed under both.)

Reading a quest's completion write from a scene gated on `active` is therefore the `Possible \ Guaranteed` warning-grade class — which is correct, and is exactly the mistake the atom exists to make visible rather than to hide.

## Reading the tables

`lute scenario <dir> envelope <nodeId>` prints both tables. The diagnostic reads for a state path `P` at node X:

- `P ∈ Guaranteed(X)` → safe under your declared routes; no diagnostic.
- `P ∉ Possible(X)` → no declared route ever sets `P` before X — **error grade**, `E-STATE-MAYBE-UNAVAILABLE`, shipped by default in `check-project`.
- `P ∈ Possible(X) \ Guaranteed(X)` → set on some but not all routes — **warning grade**, default-suppressed to this command's output only. This is the `Possible \ Guaranteed` read `check-project` computes and drops by default.

Every message carries the verbatim "under your declared routes" qualifier (A-hybrid posture).

## Quest addressing

`lute scenario <dir> envelope quest:<id>` prints a real table for **every** quest:

- A quest **with** an `after` attribute → the full `Guaranteed`/`Possible` tables, computed exactly like a scene's.
- A quest **without** `after` → the defaults-only `D` table (never empty, never an error), plus a one-line note that declaring `after` would enrich it beyond defaults-only.

A quest is reactive — `<quest start>` may fire at the earliest possible instant, so nothing beyond schema defaults can be soundly guaranteed at quest entry unless the author opts into a graph position with `after`. The reactive diagnostic side ("is this specific guard's read safe?") is already handled by `defassign` on the quest's own guards.
