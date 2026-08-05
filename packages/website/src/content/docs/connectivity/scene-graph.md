---
title: The scene graph and `after:`
description: How Lute assembles a project-wide prerequisite graph from each document's after declaration, the restricted visited/completed/active formula grammar, and the lute scenario graph view with its per-edge atom kinds.
---

Scenes and quests declare their **prerequisites** — what must have happened before this node is available — and `check-project` assembles them into a project-wide graph. This closes the one asymmetry in the language: quests already had a declarative activation predicate (`<quest start>`), but scenes had no prerequisite surface at all. `after:` gives episodes what quests already had.

## Declaring `after`

A scene declares its prerequisites via a frontmatter key `after:`; a quest declares them via an `after` attribute on its `<quest>` element, sibling to `start` / `fail`. The placement differs because a scene document is exactly one node while a quest document packs one or more `<quest>` declarations — but both share the **same formula grammar**.

```yaml
# scene frontmatter
after: 'visited("sofia.ep02") && (completed("sideQuestA") || completed("sideQuestB"))'
```

```lute
<quest id="riverDebt" after="visited('sofia.ep02')" start="…" fail="…">
```

The value is CEL under a maximally-restricted profile admitting exactly conjunction and disjunction over three opaque predicates — **no negation, no arithmetic, no state reads**:

```
Formula ::= "visited(" StringLit ")" | "completed(" StringLit ")" | "active(" StringLit ")"
          | "(" Formula ")" | Formula "&&" Formula | Formula "||" Formula
```

`visited(K)`'s string is the project's canonical `{character}.{episodeId}` episode key — the same join the compiler computes for `lineId`. `completed(Q)` and `active(Q)` both name a `<quest id>`. These predicates are scoped to this one slot; writing `visited(...)` in any ordinary CEL guard is just an unknown-function error, and anything outside the grammar above is `E-CONN-PROFILE`.

`active(Q)` (0.8.0) is the third prerequisite atom. The quest lifecycle is `unset → active → complete | failed`, so a profile carrying only `visited`/`completed` could express two of the three observable states: "this scene unlocks *while* the investigation is running" had no spelling. It is the **strictly weaker** lifecycle claim — `completed(Q)` licenses "`Q` finished and its completion writes ran", `active(Q)` licenses only "`Q` reached `active`" — which is why [reachability](/connectivity/reachability/) treats the two identically while [envelopes](/connectivity/envelopes/) do not.

## Node assembly and edges

At `check-project` time the tooling walks every document's frontmatter, computes each episode's canonical key into a project-wide **set**, and resolves each `visited(K)` / `completed(K)` / `active(K)` by exact string equality — never by decomposing the key. From each formula it derives a **topological-precedence DAG**: an edge `p → n` for every node `p` referenced anywhere in `n`'s formula, regardless of `&&`/`||` position and regardless of which atom named it. This over-approximating edge set is used for cycle detection and as the traversal order for the reachability and envelope passes.

The checker's claims are entirely **graph-structural** — is the edge set acyclic, do referenced ids exist, is there a satisfiable route, what does the graph guarantee. The formula's truth at a play session is a runtime question the engine evaluates, exactly as it evaluates `<quest start>`; Lute never runs it.

### It is an availability lattice, not a route graph

An edge says a node *becomes available* once its prerequisite holds. It does not say a player went
that way, and nothing in the language records which way they went. Two scenes written as
alternatives are simply two edges out of the same prerequisite — both land in the same topological
layer, and both are `Reachable`:

```console
$ lute scenario .
project root: .
  topological layers:
    layer 0: scene(x.s01ep01)
    layer 1: scene(x.s01ep02), scene(x.s01ep03)
  edges (prerequisite -> dependent) [atom kind(s)]:
    scene(x.s01ep01) -> scene(x.s01ep02) [visited]
    scene(x.s01ep01) -> scene(x.s01ep03) [visited]
```

Availability is also monotone by construction: the grammar above admits no negation, so no formula
can say "unless", and nothing an author can write takes an unlocked node back. Two alternatives both
unlock, and both stay unlocked. If your story needs the *choice* to matter, that belongs in state a
`<branch>` writes and a guard reads — not in `after:`.

## Viewing the graph

`lute scenario <dir>` prints the assembled node/edge graph as deterministic topological waves, per resolved project root — the whole-graph view, without any centralized edge-manifest file to keep in sync with frontmatter.

```console
$ lute scenario .
project root: .
  topological layers:
    layer 0: scene(narrator.s01ep01)
    layer 1: quest(findkai)
    layer 2: scene(narrator.s01ep02)
    layer 3: scene(narrator.s01ep03)
  edges (prerequisite -> dependent) [atom kind(s)]:
    scene(narrator.s01ep01) -> quest(findkai) [visited]
    scene(narrator.s01ep02) -> scene(narrator.s01ep03) [visited]
    quest(findkai) -> scene(narrator.s01ep02) [active]
    quest(findkai) -> scene(narrator.s01ep03) [completed]
```

A quest becomes a graph node by declaring `after` (even `after=""`); a quest that never opts into a graph position is still addressable by `lute scenario <dir> envelope quest:<id>`, but contributes no edges.

### Edge kinds

`completed(Q)` and `active(Q)` both land on the same `quest(Q)` node and are indistinguishable once the formula is flattened — so the graph records, alongside each edge, the atom kind(s) that justify it. All three output formats report it:

- **`text`** — a bracketed list after each edge, as above. One edge can carry both kinds when a formula names the same quest twice: `after: 'active("findkai") || completed("findkai")'` prints `[completed, active]`.
- **`json`** — each edge is `{"from":…,"to":…,"kinds":[…]}`; `kinds` is an array for the same reason.
- **`dot`** — an edge justified **only** by `active` atoms renders `[style=dashed]`; anything carrying a `visited` or `completed` atom stays solid, because the stronger claim is what a reader should see.

The kind is presentational and analytical, never structural: the DAG shape is identical either way, since an `active` edge constrains ordering exactly as a `completed` edge does.
