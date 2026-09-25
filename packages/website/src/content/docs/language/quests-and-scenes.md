---
title: Quests & scenes
description: The time-axis document kinds — scenes sequenced with after:, and quests with objectives, derived completion, run or user tiers, and lifecycle-event reactions — beside the lore kind for looked-up content.
---

Every `.lute` document declares a **`kind`**: `scene`, `quest`, or `lore`. The kind selects the
required frontmatter keys, the admitted grammar, and the identity model. Scenes are the playable
episodes you author line by line; quests are the run-to-completion goal machines that gate and
reward them. Both sit on the story's time axis. The third kind, `lore` (dsl 0.19.0), holds content
the engine looks up rather than plays — item descriptions, found notes, barks — and has its own
page: [Lore entries](/language/lore-entries/).

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
graph. Since 0.21.0, `visited(…)` is also an ordinary condition function — see
[Quests meet scenes and occasions](#quests-meet-scenes-and-occasions) below.

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

A quest document MAY also declare a document **`id:`** (dsl 0.19.0 §2.1) naming the file as a
bundle — a quest chain written top to bottom, such as `haven.mainChain`. It follows the same shape
rules as a scene's `id:` (`E-META-ID`), becomes the artifact's `meta.id` and the document's key in
`project.index.json`, and shares one project-wide namespace with scene and lore document ids
(`E-CONN-EPISODE-ID-DUP`). It is optional: without it the document is keyed by its first declared
quest id, exactly as before.

```yaml
kind: quest
id: haven.mainChain
uses: ../world.schema.yaml
```

### `<quest>`

A `<quest id>` needs a project-unique CEL-safe `id` (it keys the `quest.<id>.*` state tier). Its
optional predicates are CEL strings: **`start`** transitions the quest `unset` → `active` when it
holds; **`fail`** transitions `active` → `failed`. `fail` takes precedence over completion — if
both hold in the same state, the quest fails (a deterministic tie-break).

**`tier`** (dsl 0.22.0) says how long a quest's outcome lasts. The default, `tier="user"`, keeps its
status across runs, as every quest did before 0.22.0. `tier="run"` resets it: when a new run
starts, the quest's status returns to `unset` and its objectives to not done. A roguelike's
per-run goal is then taken up, completed, or failed afresh each run, beside a user-tier quest that
tracks the whole save:

```lute check
---
kind: quest
title: The climb
state:
  run.floor: { type: number, default: 0 }
  user.bestFloor: { type: number, default: 0 }
---

<quest id="climb" title="Reach the tenth floor" start="true" tier="run">
  <objective id="top" title="Reach floor ten" done="run.floor >= 10"/>
  <on event="questComplete">
    @narrator: The wind at the top is colder than you expected.
  </on>
</quest>

<quest id="legend" title="Become a legend" start="true">
  <objective id="deep" title="Reach floor ten in any run" done="user.bestFloor >= 10"/>
</quest>
```

After the reset the lifecycle settles as usual: `climb`'s `start` holds again, so it re-activates
at the start of every run, while an accept-driven run-tier quest stays `unset` until it is accepted
again. `legend` keeps its status across runs. The tier belongs to each `<quest>`, not to the
document, so one quest document can mix both. Any value but `run` or `user` is `E-ATTR-TYPE`.
A [subquest](#subquests) is the exception: it must share its parent's tier (`E-QUEST-TIER-MIX`).
[`lute play`](/tooling/play/) performs the reset at every `newRun` step, so a play script can walk
several runs and assert each one.

A quest declares its own prerequisite as an **attribute** on the element, not as
a frontmatter key — the one place this page's opening heading, "Scenes and
`after:`", does not apply:

```lute
<quest id="manifestGap" title="The Manifest Gap" start="true" after="visited('haven.s01ep06') && completed('whoWakes')">
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

A `visited()` in `after=` (or in a scene's `after:`) may name a [bundle beat](/language/beats/#beat-bundles)
by its canonical id, `after="visited('talks.maud')"` (dsl 0.24.0 §2). Before 0.24.0 that was
`E-CONN-UNKNOWN-NODE`, "a bundle beat, not a scene".

An accept-driven quest (no `start`, see [below](#quests-meet-scenes-and-occasions)) usually needs
no `after=` at all. Without one, it is anchored at every scene, bundle beat, and quest body that
`::accept`s it: `lute scenario` draws an `accept` edge from each of them (`beat(talks.maud) ->
quest(salvage) [accept]`), and the quest no longer counts as unanchored. An explicit `after=` keeps
only the edges it declares.

### `<objective>`

An `<objective id done>` requires a `done` completion predicate over declared state. `when` gates
only the objective's visibility/tracking, not the completion obligation; `optional` excludes it from
completion; `by` and `until` set a [deadline](#deadlines). An empty-body objective should be written
self-closing (`<objective …/>`); a body — a log line, a per-objective `::set` reward — emits
**once**, when the objective first becomes `done`.

**Completion is derived**, never author-written: a quest becomes `complete` when every non-`optional`
objective is `done` (or any one of them, under [`complete="any"`](#alternatives-completeany)).
Objective completion is monotonic — once `done`, it stays recorded.

### Quests meet scenes and occasions

Three small additions (dsl 0.21.0 §7a) let a scene drive a quest directly, instead of relaying a
flag through state.

**`visited()` in any condition.** `visited('<scene id>')` — until 0.21.0 legal only inside
`after:` — is a Lute-CEL function in **every condition slot**: quest `start` / `fail`, objective
`done`, beat and entry `when`, and content-line and choice `when=`. It is true once that scene has
been presented in this save (the same visited set `after:` reads; a new run does not clear it), so
a scene advances a quest simply by being played:

```lute
<objective id="heardVesna" title="Hear Vesna out" done="visited('haven.s01ep04')"/>
```

The argument is one string literal. An id that names no scene in the project is
`E-CONN-UNKNOWN-NODE` at `check-project`, exactly as in `after:`. A single-file `check` cannot know
whether a scene has been played, so it never decides a lone `visited()` true or false. It still
sees a contradiction in the condition itself: `visited('a.b') && !visited('a.b')` is false
whatever was played (dsl 0.23.0, see [How a `when` is decided](/language/beats/#how-a-when-is-decided)).
A [bundle beat](/language/beats/#beat-bundles) is visited too, under its canonical id
`<document id>.<beat id>`.

**Objectives judged at an occasion.** `<objective on="<occasion>">` evaluates that objective's
`done` **only when the occasion is raised** while the quest is active — the end-of-run check point
that a continuously evaluated condition cannot express. Without `on`, an objective is evaluated
continuously, as before.

```lute
<objective id="lowPressure" title="Keep the shed calm" on="runEnd" done="run.shedPressure < 2"/>
```

Since dsl 0.23.0 an `on` objective may also name a **target**, the way a beat does:
`<objective on="talk" target="npc.maud" …>` is judged only when `talk` is raised for `npc.maud`.
Talking to anyone else leaves it alone. Without `target`, an objective is judged whenever its
occasion is raised, whatever the target.

```lute
<objective id="thankMaud" title="Thank Maud in person" on="talk" target="npc.maud" done="run.answered"/>
```

The occasion is checked against the vocabulary exactly as a beat's `on` (see
[Beats](/language/beats/#occasions)): `E-OCCASION-UNKNOWN` when a plugin declares
occasions and this one is not among them, shape-only otherwise; an `on` that is not an identifier
is `E-BEAT-ATTR`. A `target` is checked like a beat target, also as `E-BEAT-ATTR`: it must be a
quoted dotted id, it needs `on`, the occasion must take a target, and a
[target domain](/language/beats/#target-domains) must contain it. `lute trace` / `lute run` raise
occasions for a quest walk with the mock key `occasions: [runEnd]` or `--occasion runEnd`
(repeatable), applied in order after the walk settles, and raise one for a target as
`<occasion>@<target>` (`occasions: [talk@npc.maud]`, `--occasion talk@npc.maud`). `lute play`
judges them on every step that raises the occasion, for the step's `target:`, and an occasion
that only objectives reference is a legal step. When a world event of the same name is declared,
every raise fires it first, so an active quest's `<on event>` handler for that name runs before
the objectives are judged and they can read what it wrote (see
[Beats](/language/beats/#occasions)).

By default an occasion presents its beats first and judges its objectives after, so a beat on
`runEnd` still sees the quest `active`. An occasion that closes a chapter often wants the reverse,
an epilogue that knows how the chapter's quests ended. A plugin declares that occasion
**`judge: before`** (dsl 0.24.0 §2) in its [`occasions:` export](/plugins/manifests/):

```yaml
occasions:
  chapterEnd: { judge: before }
```

Raising `chapterEnd` then judges its `on=` objectives and settles the quests **before** its beats
are decided, so a beat with `when: "quest.tide.state == 'complete'"` can win the same raise that
completed `tide`. Only the judging moves. The handler bodies the raise answers, the
`<on event="chapterEnd">` handlers and the `questComplete` / `questFailed` handlers of the quests
it settled, still run after the beats, so their narration follows the scene:

```
── step 3 · chapterEnd ──────────────
  tide.home done
  quest tide -> complete
  ✓ end.tide [scene, priority 5]
  → end.tide
@narrator: Home before the water.
@narrator: Handler on chapterEnd.
@narrator: Tide beaten.
```

`lute play` prints the transitions under the step header, above the candidates (`--json`: the
step's `judgedBefore`; its `quests` are the ones after, with the handler bodies). The default,
`judge: after`, keeps the 0.21.0 order.

**Accepting a quest from a scene.** A quest without `start` is *accept-driven*. The scene-side
form of the engine's "accept quest" action (and of `lute trace --accept`) is the core directive
[`::accept{quest="<id>"}`](/language/directives/#accept--taking-up-a-quest), typically inside the
choice where the player agrees:

```lute
<branch id="request">
  <choice id="accept" label="I'll keep it calm">
    ::accept{quest="calmTheShed"}
    @vesna: Thank you.
  </choice>
  <choice id="decline" label="Not now">
    @vesna: Another time, then.
  </choice>
</branch>
```

Accepting while the quest is already active, complete, or failed does nothing. `lute check-project`
warns **`W-QUEST-NEVER-ACCEPTED`** (dsl 0.24.0 §2) at an accept-driven quest that no `::accept` in
the project names and no `accepts:` mock (`mocks/*.yaml`, `*.test.yaml`) reaches: nothing can ever
activate it. `--deny W-QUEST-NEVER-ACCEPTED` makes it an error.

**Accepting for the next run.** A run-tier quest accepted between runs, at a hub or a bounty board
after the run has ended, would activate in the ending run and then be reset by `newRun`.
`::accept{quest="<id>" at="nextRun"}` (dsl 0.24.0 §2) queues the acceptance instead. It applies
right after the next `newRun` reset, so the quest activates in the new run's first settle:

```lute check
---
kind: scene
id: hub.board
title: The bounty board
---

## Board

@maud: The bounties reset at dawn. Sign now and the hunt is yours next run.

<branch id="bounty">
  <choice id="sign" label="Sign for the next run">
    ::accept{quest="wolfBounty" at="nextRun"}
    @maud: Your name's on the board.
  </choice>
  <choice id="later" label="Not this time">
    @maud: Suit yourself.
  </choice>
</branch>
```

`nextRun` is the only value `at` takes; any other is `E-ACCEPT-TARGET`. `lute play` prints `quest
wolfBounty accepted (queued: applies after the next newRun)` where the choice ran, and `quest
wolfBounty accepted (queued at="nextRun")` under the next `newRun` step.

**Quest outcomes in scenario tests.** `lute test` asserts the lifecycle the trace ran with
`expect.quests: {<questId>: unset | active | complete | failed}` — no side-effect `::set` needed to
observe completion. A test, a trace mock, or a play script can also start from a saved status with
a top-level `quests: {<questId>: complete}` (dsl 0.22.0). That is the way to seed one: a play
script's `engine:` step refuses a `quest.*` write, because a status change belongs to the lifecycle,
whose transitions fire handlers and grants.

### Deadlines

Some goals have a window: answer the letter before the fourth day, reach the gate before the
bell. An objective's **`by`** (dsl 0.23.0) closes the window. It is a condition slot like `done`:

```lute check
---
kind: quest
title: The letter
state:
  run.day: { type: number, default: 1 }
  run.answered: { type: bool, default: false }
  run.apologized: { type: bool, default: false }
---

<quest id="letter" title="Answer the letter" start="true" tier="run">
  <objective id="reply" title="Write back before the fourth day" done="run.answered" by="run.day >= 4"/>
  <objective id="thank" title="Thank Maud in person" on="talk" target="npc.maud" done="run.answered" by="run.day >= 4"/>
  <objective id="sorry" title="Apologize to Oskar when you next see him" on="talk" target="npc.oskar" done="run.apologized" until="run.day >= 4" optional/>
  <on event="questFailed">
    @narrator: The letter goes unanswered. Maud stops asking.
  </on>
</quest>
```

While the objective is not done, the **first** time `by` becomes true the objective **fails**, and
it is never judged again. A failed required objective fails its quest, as a `fail` condition
would: the quest's `failed` rewards are granted, its `questFailed` handlers run, and the failure
cascades to its still-active subquests. A failed `optional` objective leaves its quest alone.

- In every settle `done` is judged before `by`. So an objective whose `done` and `by` become true
  at the same moment counts as done, and a done objective never fails later. The step that raises
  an `on=` objective's occasion judges that objective's `done` first and its `by` only after, in
  all of that step's presentations and settles, so a hearing whose beat files the right verdict
  completes the objective even when that verdict also makes `by` true.
- `by` is a moment: it is judged at every lifecycle settle, for every objective, `on=` or not
  (dsl 0.24.0 §2.1). `thank` above fails in the settle that reaches day four with
  `run.answered` still false, whether or not the player ever talks to Maud again.
  (0.23.1 judged an `on=` objective's `by` only at its occasion; 0.24.0 reverses that.)
- `until="<condition>"` is the place-bound deadline: it is judged only when the objective's
  occasion is raised (for its target, when it names one), right after its `done`. So the moment
  the occasion answers is both the judgement and the deadline, and a beat on that occasion that
  writes the state `until` reads cannot fail a correct answer. `sorry` above fails only if the
  player talks to Oskar on day four or later without having apologized; a player who never sees
  him again leaves it open. `until` needs `on=` (`E-BEAT-ATTR` without it: a deadline that holds
  everywhere is `by=`).
- Quests settle after every beat an occasion presents, so a `by` can pass between two beats of one
  [`select: sequence`](/language/beats/#a-routine-then-the-days-event) occasion.
- The failure is readable: `quest.<id>.objectives.<oid>.failed` is `true` from then on, and a
  failed required objective sets its quest's `failedBy` to `by` or `until`. A new run clears both
  for a `tier="run"` quest.
- `by` and `until` are checked like `done`: an undeclared path is `E-UNDECLARED`, and a read that
  may be unset is `E-MAYBE-UNSET`.

`lute trace` records the objective's decision as `failed`, and `lute run` / `lute play` print
`letter.thank failed (by)` or `failed (until)`; all three judge `done` and the deadlines in the
same order. The quest transition names its [`failedBy`](#why-a-quest-failed) the same way:
`quest letter -> failed (by)`, and likewise `(until)`, `(fail)`, `(cascade)` and `(superseded)`.
See [Playing a story](/tooling/play/#deadlines-and-targeted-objectives).

An `on=` objective with `by=` (and no `until=`) whose `done` provably implies its `by` can
complete only in the step that raises its occasion. Anywhere else, `by` comes true no later than
`done`, at a settle where the occasion is not raised, and fails the objective before `done` is
ever judged. That is content written for 0.23.1's raise-only `by`. `lute check` and
`lute check-project` warn **`W-DEADLINE-BEFORE-DONE`** (dsl 0.24.0 §2.1) at the `by=` and suggest
`until=` with the same condition; `--deny W-DEADLINE-BEFORE-DONE` makes it an error:

```lute check
---
kind: quest
title: The hearing
state:
  run.verdict: { type: { enum: [undecided, guilty, acquitted] }, default: undecided }
---

<quest id="hearing" title="See the hearing through" start="true">
  <objective id="verdict" title="Hear the verdict" on="hearing" done="run.verdict == 'guilty'" by="run.verdict != 'undecided'"/>
</quest>
```

<!-- lute-diagnostics -->
```
hearing.lute:9:100: warning [W-DEADLINE-BEFORE-DONE] objective `verdict`'s `by="run.verdict != 'undecided'"` holds whenever its `done="run.verdict == 'guilty'"` does, and `by` is a moment judged at every settle (dsl 0.24.0 §2.1): it fails the objective at the settle it comes true, before occasion `hearing` ever judges `done` (unless both happen in the step that raises `hearing`) — write `until="run.verdict != 'undecided'"` to judge the deadline only when `hearing` is raised
```

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
| Objective completion | Compiler synthesizes `done = "quest.<child>.state == 'complete'"`. Derived parent completion — "all non-`optional` objectives `done`" — is unchanged; `optional` on a subquest objective means the child's outcome does not gate the parent. A [`complete="any"`](#alternatives-completeany) parent needs only one. |
| Upward failure | Compiler synthesizes the parent's `fail` as the disjunction of the authored `fail` (if any) and one `quest.<c>.state == 'failed'` test per **required** child, in document order. `fail`'s precedence over completion is unchanged, so a required child failing resolves the parent to `failed` even if the remaining objectives could otherwise complete. Under `complete="any"` the synthesized part is a conjunction instead. |
| Downward cascade | Engine rule: on a parent's terminal transition (`failed` or `complete`) every child still `active` transitions to `failed` (`failedBy: cascade`). A required child cannot be `active` at parent completion — its `complete` is part of the derived completion — so the `complete` arm only fails still-running **optional** children; under `complete="any"` it fails every other running child, as `superseded`. Recursive. |
| Activation | Engine rule: a referenced child with no `start` activates when its parent activates (replacing the accept-driven default), unless it declares [`activate="accept"`](#taken-up-in-dialogue-activateaccept); one with `start` evaluates the predicate only while the parent is `active` (effective gate is the conjunction). Unreferenced quests keep today's semantics exactly. |

The project's parent→child tree is **derived**, not written:
`ObjectiveEntry.quest` records the reference per artifact, and engines union
the field across artifacts exactly as they union `relations`/`rules`/
`prereqEdges`. Multi-level trees fall out naturally — a child may itself
carry `quest=` objectives — bounded only by the structural checks below.
Child ids stay flat and project-unique; there is no `parent.child`
namespacing.

Five diagnostics guard the shape:

- `E-OBJECTIVE-QUEST-DONE` — `quest=` and `done=` on one objective.
- `E-QUEST-REF-UNKNOWN` — `quest=` names no known quest id. Same split as
  `after` targets: same-document references resolve at `check`,
  cross-document resolution is `check-project`'s.
- `E-QUEST-MULTI-PARENT` — one quest referenced by `quest=` from two
  parents (tree, not DAG).
- `E-QUEST-TREE-CYCLE` — the parent→child edges form a cycle;
  self-reference is a length-1 cycle and, when parent and child share a
  document, `check` catches it early.
- `E-QUEST-TIER-MIX` — a subquest's `tier` differs from its parent's. The
  error names both quests and both tiers. Same split again: `check` reports
  it when parent and child share a document, `check-project` across
  documents. A mixed tree would lock for good. Under a `tier="run"` parent,
  the parent's end cascades into a user-tier child that keeps that status
  across runs, so the parent restarts next run waiting on a child that never
  becomes active again. Under a user-tier parent, a run-tier child resets to
  `unset` at every new run while the parent keeps its status, so once the
  parent has ended the child is never activated again. Give both quests the
  same `tier`.

Reachability propagates through the reference: a required subquest objective
whose child is `E-QUEST-UNREACHABLE` reports `E-OBJECTIVE-UNSATISFIABLE`
against the objective's `quest=` span. The full engine treatment (both
cascade directions, activation, tree reconstruction) is in
[quest-lifecycle.md](https://github.com/journeyWorker/lute/blob/main/docs/runtime/quest-lifecycle.md);
worked example:
[`docs/examples/quest-subquest.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/quest-subquest.lute).

#### Taken up in dialogue: `activate="accept"`

A child without `start` joins the journal the moment its parent activates. A side path the player
takes up in conversation should join only when it is offered. **`activate="accept"`** on the child
(dsl 0.24.0 §2) makes it wait for an `::accept` (or an `accepts:` mock, or `lute trace --accept`),
like a root quest without `start`. It activates only while its parent is `active`: an accept that
runs while the parent is not active is spent without effect, and the tools say so. `lute play`
prints `note: accept of quest firewood spent — its parent quest camp is not active yet …`
(`--json`: an `acceptSpent` record), and `lute trace` / `lute test` note the same for an
`accepts:` mock.

#### Alternatives: `complete="any"`

"Cross the river by parley, by toll, or by force" is one goal with three routes. A parent declaring
**`complete="any"`** (dsl 0.24.0 §2) completes when **any one** required objective is done, a child
quest or a plain objective. Its other still-active children then fail with
`failedBy: superseded`, so the routes the player did not take close themselves, and nobody copies
the cascade into each sibling's `fail=`. The default is `complete="all"`.

```lute check
---
kind: quest
title: The river crossing
state:
  run.talked: { type: bool, default: false }
  run.paid: { type: bool, default: false }
  run.won: { type: bool, default: false }
---

<quest id="crossing" title="Cross the river" start="true" complete="any">
  <objective id="byParley" title="Talk your way across" quest="parley"/>
  <objective id="byToll" title="Pay the toll" quest="toll"/>
  <objective id="byForce" title="Force the bridge" quest="force"/>
</quest>

<quest id="parley" title="Parley with the warden" activate="accept">
  <objective id="terms" title="Win the warden over" done="run.talked"/>
</quest>

<quest id="toll" title="Pay the toll" activate="accept">
  <objective id="pay" title="Pay the ferryman" done="run.paid"/>
</quest>

<quest id="force" title="Force the bridge">
  <objective id="fight" title="Beat the guards" done="run.won"/>
</quest>
```

`force` has no `activate`, so it activates with `crossing`. `parley` and `toll` wait for a scene to
`::accept` them. Say the player accepts `toll`, then `parley`, then wins the warden over: `parley`
completes, `crossing` completes, and the routes still open, `toll` and `force`, fail as
superseded. Had the player never accepted `toll`, it would stay `unset`. `lute play` prints the
settle:

```
  parley.terms done
  quest parley -> complete
  crossing.byParley done
  quest crossing -> complete
  quest toll -> failed (superseded)
  quest force -> failed (superseded)
```

One failed route leaves the others open. The synthesized `fail` of a `complete="any"` quest is the
**conjunction** over its required objectives, each child by `quest.<c>.state == 'failed'` and each
plain objective by its `quest.<q>.objectives.<o>.failed` flag, so the quest fails (`failedBy:
fail`) only once no route is left.

Three diagnostics guard the accept side. `activate="accept"` beside `start` is `E-ATTR-TYPE`
(pick one). `::accept` of a child that activates with its parent used to do nothing; it is now
`E-ACCEPT-TARGET` at `check-project`, naming the parent. And `W-QUEST-NEVER-ACCEPTED` flags an
`activate="accept"` child no `::accept` reaches:

```lute expect="E-ATTR-TYPE"
---
kind: quest
title: The river crossing
state:
  run.talked: { type: bool, default: false }
---

<quest id="crossing" title="Cross the river" start="true">
  <objective id="byParley" title="Talk your way across" quest="parley"/>
</quest>

<quest id="parley" title="Parley with the warden" activate="accept" start="true">
  <objective id="terms" title="Win the warden over" done="run.talked"/>
</quest>
```

<!-- lute-diagnostics -->
```
scenes/ferry.lute:17:21: error [E-ACCEPT-TARGET] `::accept` targets quest `force`, which activates with its parent `crossing`; declare `activate="accept"` on it to accept it from a scene (dsl 0.24.0 §2)
quests/crossing.lute:20:12: warning [W-QUEST-NEVER-ACCEPTED] quest `toll` waits for `::accept` (`activate="accept"`), but no `::accept` in the project names it and no `accepts:` mock or test accepts it, so it never activates; accept it from a scene with `::accept{quest="toll"}`, or remove `activate="accept"` so it activates with its parent `crossing` (dsl 0.24.0 §2)
```

### Lifecycle reactions with `<on>`

`<on event>` is the language's event-condition-action rule: when its event fires and its optional
`when` guard holds, the arm's nodes are emitted. Three built-in **lifecycle events** are quest-scoped
— **`questActive`**, **`questComplete`**, **`questFailed`** — each firing only for its own enclosing
quest. World events (e.g. `combatEnd`) are capability-provided by plugins.

A world event that is also a targeted occasion can be answered for one target only (dsl 0.24.0
§2). `<on event="talk" target="npc.maud">` runs when `talk` is raised for `npc.maud`, and never for
another target, a plain `event:` step, or a lifecycle transition:

```lute
<on event="talk" target="npc.maud">
  @maud: Coin first, talk later.
</on>
```

The target is checked like an [objective's](#quests-meet-scenes-and-occasions) (`E-BEAT-ATTR`): it
must be a quoted dotted id, the occasion named like the event must take a target, and the target
must lie inside its domain. A
lifecycle event is never raised for a target, so `target` on `questActive`, `questComplete`, or
`questFailed` is `E-BEAT-ATTR` too. Without `target`, a handler answers every raise of its event.

A `questFailed` handler on a quest that can never fail is dead code. `lute check-project` warns
`W-QUEST-HANDLER-DEAD` at the handler's `event` when its quest has no `fail`, no required objective
with a `by=` or `until=` [deadline](#deadlines), no required subquest objective whose child can
itself fail (a failing required child fails its parent), and no parent quest (a parent's end
cascade-fails its still-active children). Add a `fail=` condition, or remove the handler:

<!-- lute-diagnostics -->
```
./q.lute:10:14: warning [W-QUEST-HANDLER-DEAD] `<on event="questFailed">` never runs: quest `lampOut` cannot fail — it has no `fail` condition, no required objective with a `by=` deadline, no required subquest that can fail (a failing required child fails it), and no parent quest whose end would cascade to it; add a `fail=` condition or remove the handler (dsl 0.22.0 §7)
```

The same code (dsl 0.24.0) names a world-event handler that can only fire on a completed quest:
one whose `when` contains every required objective's `done` (any one, under `complete="any"`),
`@def`s expanded. The quest settles `complete` as soon as that `when` holds, and an event reaches
only an active quest's handlers. `lute check` reports it too. Lifecycle events, and quests with an
`on=` or `quest=` objective, are exempt:

<!-- lute-diagnostics unverified="verbatim lute check-project output; the message is composed in crates/lute-check/src/reachability.rs, which names the code through the project_check::W_QUEST_HANDLER_DEAD constant rather than a string literal, so the scraper cannot pair quote and code" -->
```
quests/road.lute:25:14: warning [W-QUEST-HANDLER-DEAD] `<on event="combatEnd">` never runs: its `when` implies every required objective of quest `brawl` is done, so the quest has already completed when it holds, and an event reaches only an active quest's handlers; move the body to `<on event="questComplete">` or the objective's own body (dsl 0.24.0)
```

Content elsewhere can also gate on quest lifecycle by reading the reserved `quest.<id>.state` path.
It is **always assigned**: `unset` until the quest activates, then `active`, `complete`, or `failed`.
So a read needs no `isSet` guard (`isSet(quest.<id>.state)` is always true, `W-QUEST-STATE-ISSET`),
`when="quest.rescueHalsin.state == 'complete'"` is a complete guard, and "not taken up yet" is
`quest.rescueHalsin.state == 'unset'` or a `<when is="unset">` arm:

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

### Why a quest failed

`failed` says that a quest ended badly, not why. Two more reserved, read-only paths do (dsl 0.24.0
§2):

- **`quest.<id>.failedBy`** reads `unset` until the quest fails, then one of `fail` (its `fail`
  condition held), `by` or `until` (a required objective missed that [deadline](#deadlines)),
  `cascade` (its parent ended while it was active), or `superseded` (its
  [`complete="any"`](#alternatives-completeany) parent completed through another route).
- **`quest.<id>.objectives.<oid>.failed`** reads `true` once the objective's `by` or `until` failed
  it, and `false` before.

Any condition or interpolation may read them, so an epilogue can tell a route the player left
behind from a deadline they missed:

```lute check
---
kind: scene
id: river.epilogue
title: Across the river
---

## Epilogue

<match on="quest.toll.failedBy">
  <when is="superseded">
    @narrator: The ferryman is still waiting for a coin that never came.
  </when>
  <when is="by|until">
    @narrator: You reached the ferry too late.
  </when>
  <when is="fail|cascade">
    @narrator: The ferry burned before you could pay.
  </when>
  <when is="unset">
    @narrator: The river is behind you.
  </when>
</match>

@narrator{when="quest.letter.objectives.thank.failed"}: Maud's letter is still in your coat.
```

`failedBy` is an enum of those six values, so the match above is exhaustive without
`<otherwise>`, and a misspelled arm (`is="superceded"`) is `E-WHEN-LITERAL-DOMAIN`. When a missed
objective and `fail` fail a quest in the same settle, the objective's kind wins. Declaring either
path in `state:` is `E-QUEST-RESERVED-DECL`, and writing one with `::set` is
`E-QUEST-RESERVED-WRITE`. A run-tier quest's `newRun` reset clears both. `lute run`, `lute play`,
`lute trace`, and `lute test` agree on the values, and `lute run --json` carries `failedBy` on the
failing `quest` record.

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

`activatedAt` is also exempt from `E-MAYBE-UNSET`, because a maybe-unset verdict on it would be
undischargeable: no literal inhabits `narrativeTime`, so the slot can carry no `default:`, and both
guard forms — `isSet(p)` and `has(p)` — are themselves `E-TEMPORAL-ARG` on a narrative-time operand.
(`quest.<id>.state` is exempt for a different reason: it is always assigned.) The engine guarantees
the stamp exists for any activated instance, and a read is only meaningful inside one.
