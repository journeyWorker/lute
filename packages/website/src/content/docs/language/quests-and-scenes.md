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
Its vocabulary is exactly two predicates combined with `&&` / `||`:

- `visited("<sceneKey>")` — true once the player has seen that scene (key = `{character}.{episodeId}`);
- `completed("<questId>")` — true once that quest is finished.

```yaml
after: 'visited("mira.s01ep01") && completed("theCoffeeDebt")'
```

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

*(From [`docs/examples/quest-grove.lute`](https://github.com/KantoRegion/lute/blob/main/docs/examples/quest-grove.lute).)*

### `<quest>`

A `<quest id>` needs a project-unique CEL-safe `id` (it keys the `quest.<id>.*` state tier). Its
optional predicates are CEL strings: **`start`** transitions the quest `unset` → `active` when it
holds; **`fail`** transitions `active` → `failed`. `fail` takes precedence over completion — if
both hold in the same state, the quest fails (a deterministic tie-break).

### `<objective>`

An `<objective id done>` requires a `done` completion predicate over declared state. `when` gates
only the objective's visibility/tracking, not the completion obligation; `optional` excludes it from
completion. An empty-body objective should be written self-closing (`<objective …/>`); a body — a
log line, a per-objective `::set` reward — emits **once**, when the objective first becomes `done`.

**Completion is derived**, never author-written: a quest becomes `complete` when every non-`optional`
objective is `done`. Objective completion is monotonic — once `done`, it stays recorded.

### Lifecycle reactions with `<on>`

`<on event>` is the language's event-condition-action rule: when its event fires and its optional
`when` guard holds, the arm's nodes are emitted. Three built-in **lifecycle events** are quest-scoped
— **`questActive`**, **`questComplete`**, **`questFailed`** — each firing only for its own enclosing
quest. World events (e.g. `combatEnd`) are capability-provided by plugins.

Content elsewhere can also gate on quest lifecycle by reading the reserved `quest.<id>.state` path:

```lute
<match on="quest.rescueHalsin.state">
  <when is="complete"> @shadowheart: You did well back there. </when>
  <when is="failed">   @shadowheart: We were too late. </when>
  <otherwise>          @shadowheart: We should keep moving. </otherwise>
</match>
```

Quests can gate on relational facts too — `start="holds(inParty(shadowheart))"` — see
[Facts & Datalog](/state/facts-and-datalog/) for the fact surface, worked in full by
[`docs/examples/quest-rescue-halsin.lute`](https://github.com/KantoRegion/lute/blob/main/docs/examples/quest-rescue-halsin.lute).
