---
title: The scene graph and `after:`
description: How Lute assembles a project-wide prerequisite graph from each document's after declaration, the restricted visited/completed/active formula grammar, and the lute scenario graph view with its per-edge atom kinds and its bundle beat nodes.
---

Scenes and quests declare their **prerequisites** — what must have happened before this node is available — and whole-project `lute check` assembles them into a project-wide graph. This closes the one asymmetry in the language: quests already had a declarative activation predicate (`<quest start>`), but scenes had no prerequisite surface at all. `after:` gives episodes what quests already had.

## Declaring `after`

A scene declares its prerequisites via a frontmatter key `after:`; a quest declares them via an `after` attribute on its `<quest>` element, sibling to `start` / `fail`. The placement differs because a scene document is exactly one node while a quest document packs one or more `<quest>` declarations — but both share the **same formula grammar**.

```yaml
# scene frontmatter
after: 'visited("elena.ep02") && (completed("sideQuestA") || completed("sideQuestB"))'
```

```lute
<quest id="riverDebt" follows="visited('elena.ep02')" start="…" fail="…">
```

The value is CEL under a maximally-restricted profile admitting exactly conjunction and disjunction over three opaque predicates — **no negation, no arithmetic, no state reads**:

```
Formula ::= "visited(" StringLit ")" | "completed(" StringLit ")" | "active(" StringLit ")"
          | "(" Formula ")" | Formula "&&" Formula | Formula "||" Formula
```

`visited(K)`'s string is the project's canonical `{character}.{episodeId}` episode key — the same join the compiler computes for `lineId`. `completed(Q)` and `active(Q)` both name a `<quest id>`. `completed` and `active` are scoped to this one slot (in an ordinary CEL guard, read `quest.<id>.state` instead); `visited(...)` is, since 0.21.0, also an ordinary condition function over the same visited set (see [Quests & scenes](/language/quests-and-scenes/#quests-meet-scenes-and-occasions)). Anything outside the grammar above is `E-CONN-PROFILE`.

`active(Q)` (0.8.0) is the third prerequisite atom. The quest lifecycle is `unset → active → complete | failed`, so a profile carrying only `visited`/`completed` could express two of the three observable states: "this scene unlocks *while* the investigation is running" had no spelling. It is the **strictly weaker** lifecycle claim — `completed(Q)` licenses "`Q` finished and its completion writes ran", `active(Q)` licenses only "`Q` reached `active`" — which is why [reachability](/connectivity/reachability/) treats the two identically while [envelopes](/connectivity/envelopes/) do not.

## Node assembly and edges

When whole-project `lute check` runs, the tooling walks every document's frontmatter, computes each episode's canonical key into a project-wide **set**, and resolves each `visited(K)` / `completed(K)` / `active(K)` by exact string equality — never by decomposing the key. From each formula it derives a **topological-precedence DAG**: an edge `p → n` for every node `p` referenced anywhere in `n`'s formula, regardless of `&&`/`||` position and regardless of which atom named it. This over-approximating edge set is used for cycle detection and as the traversal order for the reachability and envelope passes.

The checker's claims are entirely **graph-structural** — is the edge set acyclic, do referenced ids exist, is there a satisfiable route, what does the graph guarantee. The formula's truth at a play session is a runtime question the engine evaluates, exactly as it evaluates `<quest start>`; Lute never runs it.

### It is an availability lattice, not a route graph

An edge says a node *becomes available* once its prerequisite holds. It does not say a player went
that way, and nothing in the language records which way they went. Two scenes written as
alternatives are simply two edges out of the same prerequisite — both land in the same topological
layer, and both are `Reachable`:

```console
$ lute scenario --project .
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

`lute scenario --project <DIR>` prints the assembled node/edge graph as deterministic topological waves, per resolved project root — the whole-graph view, without any centralized edge-manifest file to keep in sync with frontmatter.

```console
$ lute scenario --project .
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

A quest joins the graph by declaring `follows` (even `follows=""`; graph metadata only — it never gates the quest), or, without one, through its anchors: the `::accept`s that take it up (`[accept]`, dsl 0.24.0 §2), its parent quest (`[subquest]`, dsl 0.25.0 §4), and the `visited(…)` / `entry.X.everRead` / `quest.Y.state == …` conjuncts of its `start` (`[start]`, dsl 0.25.0 §4; an entry becomes the node `entry(X)`). See [Quests & scenes](/language/quests-and-scenes/#quest). A quest with none of these is listed as unanchored; it is still addressable by `lute scenario --project <DIR> envelope --node quest:<id>`, but contributes no edges.

### Edge kinds

`completed(Q)` and `active(Q)` both land on the same `quest(Q)` node and are indistinguishable once the formula is flattened — so the graph records, alongside each edge, the atom kind(s) that justify it. All three output formats report it:

- **`text`** — a bracketed list after each edge, as above. One edge can carry both kinds when a formula names the same quest twice: `after: 'active("findkai") || completed("findkai")'` prints `[completed, active]`.
- **`json`** (`--json`) — each edge is `{"from":…,"to":…,"kinds":[…]}`; `kinds` is an array for the same reason.
- **`dot`** (`--format dot`) — an edge justified **only** by `active` atoms renders `[style=dashed]`; anything carrying a `visited` or `completed` atom stays solid, because the stronger claim is what a reader should see.

The kind is presentational and analytical, never structural: the DAG shape is identical either way, since an `active` edge constrains ordering exactly as a `completed` edge does. Besides the three atom kinds, an edge may be an anchor that no `after` formula wrote: `[accept]` from a unit that `::accept`s a quest, `[subquest]` from a parent quest to its child, and `[start]` from a source its `start` reads (dsl 0.24.0, 0.25.0); since 0.51, `[raises]` from the node whose directive call or reward raises an occasion to the scene or beat that answers it ([below](#edges-that-declared-raises-draw)).

### Edges that declared raises draw

A directive or reward kind can declare that the host raises an occasion in answer to it ([`effects.raises`](/plugins/manifests/#declared-occasion-raises), 0.51). When the occasion is declared `raisedBy: declared`, the graph reads that declaration as the only way the occasion is raised, so a scene or bundle beat that answers it is **anchored** to the nodes that raise it:

- **Sources.** A raise comes from the scene whose shots contain the call, the lore bundle beat whose body contains it, or the quest whose body contains it or that grants the reward. A call in a lore entry's body is a raise (it counts for `E-RAISES-TARGET` and `W-OCCASION-NEVER-RAISED`) but anchors nothing, as an entry's `::accept` anchors nothing. A call inside a component body counts at each `::use` site.
- **Which answerer a raise reaches.** One rule serves the anchors, `W-OCCASION-NEVER-RAISED` and `E-RAISES-TARGET`'s "no scene or beat answers it": the raise names the answerer's `on:` occasion, and the occasion is untargeted, or the answerer declares no `target:` (it answers every target), or the raise's target is `occasion.target` (it stands for every answerer), or its static target equals the answerer's `target:`.
- **The anchor.** A scene or bundle beat that answers a `raisedBy: declared` occasion, declares no `after:`, and has at least one raise source that is a graph node gets one anchor with every reaching source as an alternative. A node that declares `after:` keeps `after:` as its declared route, and no `[raises]` edge is drawn to it.
- **The edge.** It is drawn `source -> answerer`: `lute scenario` text prints `scene(intro) -> scene(chapter2) [raises]`, `--json` gives `"kinds": ["raises"]`, and `--format dot` draws a solid edge labelled `raises`. `lute scenario reach` follows it and prints the anchor as `[raises] quest(finale) || scene(lock)`, and the `scenario` node envelope names the raise (`` raised by `::playScene` in scene `intro` ``). Since 0.52 (#210) [`lute impact`](/tooling/cli/#impact) follows `[raises]` and `[accept]` edges too: a `raises` link is `proven` when a raise site behind it has a static target and `heuristic` when every site reads `occasion.target`, and an `accepts` link is `proven`. An edge that would close a cycle is not drawn, so a scene that raises its own occasion again is never `E-CONN-CYCLE`; since that edge's source may then be ordered after the node it anchors, reachability repeats to a fixed point in that case.

A declared raise is a host promise, not a proof, so it is a **may-edge** and no Must or guaranteed claim rests on it:

- An anchored node starts where an entry node does: a raiser's unconditional `::set` or `::assert` is neither in the answerer's guaranteed-write envelope nor a fact that must hold there. The scenario node envelope lists such a writer as "not provably before this node", because a raise's sources are alternatives.
- A `[raises]` edge does not gate a quest's start and does not join start-cycle detection.
- [Cast presence](/language/dialogue-and-cast/#the-cast) may follow `[raises]` edges, which can only add `W-CAST-ABSENT` may-warnings, never remove one.
- [Reachability](/connectivity/reachability/) reads such an anchor as it reads every anchor: `Unreachable` becomes `Unknown`, because a declared raise is a host promise, not a proof.

A scene or beat that answers a `raisedBy: declared` occasion that no declared raise reaches is `W-OCCASION-NEVER-RAISED` ([reachability](/connectivity/reachability/#occasions-that-nothing-raises)).

### Bundle beats

A lore document's [bundle beats](/tooling/play/#bundle-beats) (dsl 0.23.0) are nodes too, each drawn `beat(<document id>.<beat id>)`. A `<beat>` without `after=` is an entry node in layer 0, with no edges of its own: its occasion, target and `when` decide when it plays. Add a porter's line to the project above, in a lore document with `id: harbor`:

```lute
<beat id="porter" on="talk" target="npc.porter" when="visited('narrator.s01ep02')">
  @porter: Back again? The boats won't wait.
</beat>
```

```console
$ lute scenario --project .
project root: .
  topological layers:
    layer 0: scene(narrator.s01ep01), beat(harbor.porter)
    layer 1: quest(findkai)
    layer 2: scene(narrator.s01ep02)
    layer 3: scene(narrator.s01ep03)
  edges (prerequisite -> dependent) [atom kind(s)]:
    scene(narrator.s01ep01) -> quest(findkai) [visited]
    scene(narrator.s01ep02) -> scene(narrator.s01ep03) [visited]
    quest(findkai) -> scene(narrator.s01ep02) [active]
    quest(findkai) -> scene(narrator.s01ep03) [completed]
  unanchored (no `after` / `follows` — available from the start of play; no prerequisites in this graph):
    beat(harbor.porter) — its `when` reads visited('narrator.s01ep02'), which gates it but draws no edge; write `after="visited('narrator.s01ep02')"` to anchor it
```

The `visited()` in its `when` draws no edge: a `when` is a runtime condition, not a prerequisite. Since dsl 0.25.0 the bare view therefore lists the beat as unanchored, with the `after=` that would order it (`unanchoredHints` in the `--json` report). In the `--json` report the node's `kind` is `beat` (its `prereq` is `null`), and with `--format dot` it is drawn `shape=note`. `lute scenario reach --node` and `lute scenario envelope --node` take its canonical id, bare or as `beat:<id>`, and `reach` names where the beat is declared and what selects it:

```console
$ lute scenario --project . reach --node harbor.porter
project root: .
reach beat(harbor.porter):
  verdict: Reachable — a satisfiable route exists under your declared routes.
  after: (none declared) — an entry node: it plays when its occasion is raised and its `when` holds.
  declared in: ./lore/harbor.lute
  on: talk
  target: npc.porter
  when: visited('narrator.s01ep02')
```

Since dsl 0.25.0 §3 a bundle beat may declare `after="…"` itself, with a scene `after:`'s grammar and meaning. Written `<beat id="porter" on="talk" target="npc.porter" after="visited('narrator.s01ep02')">`, the porter moves out of layer 0 and the graph gains `scene(narrator.s01ep02) -> beat(harbor.porter) [visited]`; the beat is then eligible only once the prerequisite holds. `reach --node harbor.porter` then prints the formula on its `after:` line (`after: visited("narrator.s01ep02")`) instead of `(none declared)`. A bundle beat is also a legal `after:` predecessor (dsl 0.24.0 §2): a scene's or quest's `visited('harbor.porter')` names it, and connectivity routes through it like a scene.
