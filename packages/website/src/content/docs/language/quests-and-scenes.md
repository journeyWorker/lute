---
title: Quests & scenes
description: The two document kinds — scenes sequenced with after:, and quests with objectives, derived completion, and lifecycle-event reactions.
---

Every `.lute` document declares a **`kind`**: `scene` or `quest`. The kind selects the required
frontmatter keys, the admitted grammar, and the identity model. Scenes are the playable episodes
you author line by line; quests are the run-to-completion goal machines that gate and reward them.

## Scenes and `after:`

A scene is one episode — the frontmatter identity triple (`character`/`season`/`episode`) plus its
shots. Scenes are *sequenced* with the frontmatter key **`after:`**, which declares the routes the
checker and `lute scenario` assume reach this scene. It is advisory ordering metadata, not a jump.
Its vocabulary is exactly three predicates combined with `&&` / `||`:

- `visited("<sceneKey>")` — true once the player has seen that scene (key = `{character}.{episodeId}`);
- `completed("<questId>")` — true once that quest is finished;
- `active("<questId>")` — true once that quest has been taken up and not yet resolved.

```yaml
after: 'visited("mira.s01ep01") && active("theCoffeeDebt")'
```

`active` is new in 0.8.0, and it closes an asymmetry. The quest lifecycle is `unset` → `active` →
`complete` | `failed`, yet the prerequisite vocabulary could name only two of those three observable
states — "reachable while the debt is still outstanding" had no spelling at all. For the
reachability graph an `active` edge is **identical** to a `completed` one: both assert "that node
must be reachable before this one", so cycle and unreachability analysis is unchanged. The state
envelope it carries is strictly **weaker** — after `completed(q)` a consumer may assume
`quest.q.state == complete`; after `active(q)`, only that the quest reached `active`.

There is no negation, arithmetic, or state read in `after:`. See
[Scene graph & after:](/connectivity/scene-graph/) for how these declarations form the reachability
graph.

## The quest kind

A quest document declares `kind: quest` and carries `uses:` for the schema it gates on. Its body is
one or more `<quest>` declarations — quests forbid `<hub>`, `<timeline>`, and `#`/`##` headings.

```lute
<quest id="rescueHalsinGrove" title="Rescue the First Druid" start="run.act == 1" fail="run.npc.halsin.dead">
  <objective id="reachGrove" title="Reach the Emerald Grove" done="run.region == 'grove'"/>
  <objective id="freeHalsin" title="Free Halsin from the cage" done="run.npc.halsin.freed"/>

  <on event="questComplete">
    ::set{user.xp += 300}
    ::set{run.metHalsin = true}
    @narrator: Halsin rolled his shoulders and looked north. "Moonrise, then."
  </on>

  <on event="questFailed">
    ::set{run.groveOutcome = "halsinDead"}
    @narrator: The cage held only silence now.
  </on>
</quest>
```

*(From [`docs/examples/quest-grove.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/quest-grove.lute).)*

### `<quest>`

A `<quest id>` needs a project-unique CEL-safe `id` (it keys the `quest.<id>.*` state tier). Its
optional predicates are CEL strings: **`start`** transitions the quest `unset` → `active` when it
holds; **`fail`** transitions `active` → `failed`. `fail` takes precedence over completion — if
both hold in the same state, the quest fails (a deterministic tie-break).

A quest declares its own prerequisite as an **attribute** on the element, not as
a frontmatter key — the one place this page's opening heading, "Scenes and
`after:`", does not apply:

```lute
<quest id="manifestGap" title="The Manifest Gap" start="true" after="visited('anseo.s01ep06') && completed('whoWakes')">
  <objective id="reconcile" title="Reconcile the count" done="true"/>
</quest>
```

`after=` takes the same restricted prerequisite profile as a scene's `after:`
key — `visited("id")`, `completed("id")`, `active("id")`, `&&`, `||`, and
nothing else (`E-CONN-PROFILE`). Writing `after:` in a quest's *frontmatter* is
`E-META-UNKNOWN-KEY`, and the diagnostic names the attribute form:

<!-- lute-diagnostics -->
```
error [E-META-UNKNOWN-KEY] unknown top-level meta key `after` (not a core key and not owned by an active plugin) — a quest's prerequisite is the `after=` ATTRIBUTE on its `<quest>` element, not a frontmatter key (dsl §4.1)
```

### `<objective>`

An `<objective id done>` requires a `done` completion predicate over declared state. `when` gates
only the objective's visibility/tracking, not the completion obligation; `optional` excludes it from
completion. An empty-body objective should be written self-closing (`<objective …/>`); a body — a
log line, a per-objective `::set` reward — emits **once**, when the objective first becomes `done`.

**Completion is derived**, never author-written: a quest becomes `complete` when every non-`optional`
objective is `done`. Objective completion is monotonic — once `done`, it stays recorded.

### Subquests

An `<objective quest="childId"/>` names a child quest whose completion is the
objective. `quest=` and `done=` are mutually exclusive on one objective
(`E-OBJECTIVE-QUEST-DONE` — exactly one is required); every other objective
attribute admits alongside `quest=` (`when=`, `optional`, `title=`, a
completion body), and the body still plays exactly once — when the objective
first becomes `done`, i.e. when the child completes — which is the natural
"child resolved" journal slot. The child is an ordinary `<quest>` in the same
or another document; the one-line tag rule, id rules, and quest-body grammar
are unchanged, and authored `done=` and subquest `quest=` objectives mix
freely in one parent.

```lute
<quest id="saveTheGrove" title="Save the Grove" start="run.act == 1">
<objective id="halsin" title="Find Halsin" quest="findHalsin"/>
<objective id="ritual" title="Stop the ritual" quest="stopRitual"/>
<objective id="scout" title="Scout the perimeter" quest="scoutPerimeter" optional/>
<objective id="talkRath" title="Speak to Rath" done="run.spokeRath"/>
</quest>
```

There is no new tag, no new command kind, and no `parent=` attribute anywhere
— the reference points parent → child, and the mechanism reuses two existing
surfaces plus two engine-derived rules:

| Direction | Rule |
| --- | --- |
| Objective completion | Compiler synthesizes `done = "quest.<child>.state == 'complete'"`. Derived parent completion — "all non-`optional` objectives `done`" — is unchanged; `optional` on a subquest objective means the child's outcome does not gate the parent. |
| Upward failure | Compiler synthesizes the parent's `fail` as the disjunction of the authored `fail` (if any) and one `quest.<c>.state == 'failed'` test per **required** child, in document order. `fail`'s precedence over completion is unchanged, so a required child failing resolves the parent to `failed` even if the remaining objectives could otherwise complete. |
| Downward cascade | Engine rule: on a parent's terminal transition (`failed` or `complete`) every child still `active` transitions to `failed`. A required child cannot be `active` at parent completion — its `complete` is part of the derived completion — so the `complete` arm only fails still-running **optional** children. Recursive. |
| Activation | Engine rule: a referenced child with no `start` activates when its parent activates (replacing the walk-start / accept default); one with `start` evaluates the predicate only while the parent is `active` (effective gate is the conjunction). Unreferenced quests keep today's semantics exactly. |

The project's parent→child tree is **derived**, not written:
`ObjectiveEntry.quest` records the reference per artifact, and engines union
the field across artifacts exactly as they union `relations`/`rules`/
`prereqEdges`. Multi-level trees fall out naturally — a child may itself
carry `quest=` objectives — bounded only by the structural checks below.
Child ids stay flat and project-unique; there is no `parent.child`
namespacing.

Four new diagnostics guard the shape:

- `E-OBJECTIVE-QUEST-DONE` — `quest=` and `done=` on one objective.
- `E-QUEST-REF-UNKNOWN` — `quest=` names no known quest id. Same split as
  `after` targets: same-document references resolve at `check`,
  cross-document resolution is `check-project`'s.
- `E-QUEST-MULTI-PARENT` — one quest referenced by `quest=` from two
  parents (tree, not DAG).
- `E-QUEST-TREE-CYCLE` — the parent→child edges form a cycle;
  self-reference is a length-1 cycle and, when parent and child share a
  document, `check` catches it early.

Reachability propagates through the reference: a required subquest objective
whose child is `E-QUEST-UNREACHABLE` reports `E-OBJECTIVE-UNSATISFIABLE`
against the objective's `quest=` span. The full engine treatment (both
cascade directions, activation, tree reconstruction) is in
[quest-lifecycle.md](https://github.com/journeyWorker/lute/blob/main/docs/runtime/quest-lifecycle.md);
worked example:
[`docs/examples/quest-subquest.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/quest-subquest.lute).

### Lifecycle reactions with `<on>`

`<on event>` is the language's event-condition-action rule: when its event fires and its optional
`when` guard holds, the arm's nodes are emitted. Three built-in **lifecycle events** are quest-scoped
— **`questActive`**, **`questComplete`**, **`questFailed`** — each firing only for its own enclosing
quest. World events (e.g. `combatEnd`) are capability-provided by plugins.

Content elsewhere can also gate on quest lifecycle by reading the reserved `quest.<id>.state` path:

```lute
<match on="quest.rescueHalsin.state">
  <when is="complete">
    @shadowheart: You did well back there.
  </when>
  <when is="failed">
    @shadowheart: We were too late.
  </when>
  <otherwise>
    @shadowheart: We should keep moving.
  </otherwise>
</match>
```

Quests can gate on relational facts too — `start="holds(inParty(shadowheart))"` — see
[Facts & Datalog](/state/facts-and-datalog/) for the fact surface, worked in full by
[`docs/examples/quest-rescue-halsin.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/quest-rescue-halsin.lute).

### `quest.<id>.activatedAt`

Beside `quest.<id>.state`, every quest carries a second reserved path: **`quest.<id>.activatedAt`**,
the `narrativeTime` instant the engine stamps at the `unset` → `active` transition. Like
`quest.<id>.state` it is engine-populated, so it is neither author-declarable
(`E-QUEST-RESERVED-DECL`) nor author-writable (`E-QUEST-RESERVED-WRITE`) — but content may *read*
it, which is the whole point: it is the time anchor `validAt(rel, t)` never had.

```lute
<objective id="visitedSinceAccept" title="Go back to the station" done="holds(arrivedSpace(station_front)) && !validAt(arrivedSpace(station_front), quest.theCoffeeDebt.activatedAt)"/>
```

A tag and all of its attributes must sit on **one physical line**, so do not wrap a long
`<objective …/>` for readability — it is a parse error. Note the shape: `validAt` is a
*point-in-time* query, true iff the fact was valid **at** that instant, so "since activation"
is the conjunction "true now, and not yet true at activation" — a bare
`validAt(R, …activatedAt)` asks the opposite question. Away from `validAt`, a narrative-time value
admits only the ordering comparisons `<`, `<=`, `==`, `>`, `>=`; `!=` is `E-TEMPORAL-ARG`, as is any
arithmetic on one. See [Facts & Datalog](/state/facts-and-datalog/) for the interval semantics.

`activatedAt` is also the one reserved path exempt from `E-MAYBE-UNSET`, because a maybe-unset
verdict on it would be undischargeable: no literal inhabits `narrativeTime`, so the slot can carry
no `default:`, and both guard forms — `isSet(p)` and `has(p)` — are themselves `E-TEMPORAL-ARG` on a
narrative-time operand. `quest.<id>.state` has an escape hatch (`<match>` exhaustiveness over its
lifecycle enum); this has none. The engine guarantees the stamp exists for any activated instance,
and a read is only meaningful inside one.
