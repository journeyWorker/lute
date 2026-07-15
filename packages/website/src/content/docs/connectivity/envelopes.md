---
title: Guaranteed / Possible envelopes
description: The per-node available-state analysis — the Guaranteed and Possible tables over run/user state, the Possible-minus-Guaranteed warning, lute scenario envelope, and quest addressing including the bare-quest defaults-only answer.
---

The **envelope** answers a proactive question: *by the time control reaches node X, what state is actually set?* This is distinct from what's legal to read (governed by schema import). The envelope tracks two sets per node, scoped to **`run.*` / `user.*` only** — the tiers whose writes are monotonic ("once set, stays set," so union/intersect over predecessors is sound). Quest scratch fields (`quest.<id>.*`) are excluded; "was it reachable at X" is answered directly by `completed(Q)` in the route structure.

- **Guaranteed(X)** — set on *every* declared route to X.
- **Possible(X)** — set on *some* declared route to X.

Both are a graph-lift of `defassign`'s own lattice: each node's guaranteed-write set is computed by the definite-assignment walk, then propagated by structural recursion over the formula AST:

```
visited(Y):   G = Guaranteed(Y) ∪ G(Y)      P = Possible(Y) ∪ P(Y)
completed(Q): G = P = writesOnComplete(Q)
X && Y:       G = G(X) ∪ G(Y)               P = P(X) ∪ P(Y)
X || Y:       G = G(X) ∩ G(Y)               P = P(X) ∪ P(Y)
entry node:   G = P = D
```

`D`, the entry base case, is the set of `run.*`/`user.*` paths carrying a schema `default` — reused verbatim from the import layer. `D ⊆ Guaranteed(n)` at *every* node, matching definite-assignment's "a defaulted path is always assigned" invariant lifted to the whole graph. This structural recursion is provably identical to a per-route ∩/∪ computation, never materializing the exponential route set.

## Reading the tables

`lute scenario envelope <nodeId>` prints both tables. The diagnostic reads for a state path `P` at node X:

- `P ∈ Guaranteed(X)` → safe under your declared routes; no diagnostic.
- `P ∉ Possible(X)` → no declared route ever sets `P` before X — **error grade**, `E-STATE-MAYBE-UNAVAILABLE`, shipped by default in `check-project`.
- `P ∈ Possible(X) \ Guaranteed(X)` → set on some but not all routes — **warning grade**, default-suppressed to this command's output only. This is the `Possible \ Guaranteed` read `check-project` computes and drops by default.

Every message carries the verbatim "under your declared routes" qualifier (A-hybrid posture).

## Quest addressing

`lute scenario envelope quest:<id>` prints a real table for **every** quest:

- A quest **with** an `after` attribute → the full `Guaranteed`/`Possible` tables, computed exactly like a scene's.
- A quest **without** `after` → the defaults-only `D` table (never empty, never an error), plus a one-line note that declaring `after` would enrich it beyond defaults-only.

A quest is reactive — `<quest start>` may fire at the earliest possible instant, so nothing beyond schema defaults can be soundly guaranteed at quest entry unless the author opts into a graph position with `after`. The reactive diagnostic side ("is this specific guard's read safe?") is already handled by `defassign` on the quest's own guards.
