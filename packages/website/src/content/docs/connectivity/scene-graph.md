---
title: The scene graph and `after:`
description: How Lute assembles a project-wide prerequisite graph from each document's after declaration, the restricted formula grammar, and the lute scenario graph view.
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

The value is CEL under a maximally-restricted profile admitting exactly conjunction and disjunction over two opaque predicates — **no negation, no arithmetic, no state reads**:

```
Formula ::= "visited(" StringLit ")" | "completed(" StringLit ")"
          | "(" Formula ")" | Formula "&&" Formula | Formula "||" Formula
```

`visited(K)`'s string is the project's canonical `{character}.{episodeId}` episode key — the same join the compiler computes for `lineId`. `completed(Q)` names a `<quest id>`. These predicates are scoped to this one slot; writing `visited(...)` in any ordinary CEL guard is just an unknown-function error.

## Node assembly and edges

At `check-project` time the tooling walks every document's frontmatter, computes each episode's canonical key into a project-wide **set**, and resolves each `visited(K)` / `completed(K)` by exact string equality — never by decomposing the key. From each formula it derives a **topological-precedence DAG**: an edge `p → n` for every node `p` referenced anywhere in `n`'s formula, regardless of `&&`/`||` position. This over-approximating edge set is used for cycle detection and as the traversal order for the reachability and envelope passes.

The checker's claims are entirely **graph-structural** — is the edge set acyclic, do referenced ids exist, is there a satisfiable route, what does the graph guarantee. The formula's truth at a play session is a runtime question the engine evaluates, exactly as it evaluates `<quest start>`; Lute never runs it.

## Viewing the graph

`lute scenario <dir>` prints the assembled node/edge graph as deterministic topological waves, per resolved project root — the whole-graph view, without any centralized edge-manifest file to keep in sync with frontmatter.
