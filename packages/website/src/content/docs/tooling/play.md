---
title: Playing a story
description: "Occasions and beats (dsl 0.21.0) — how a project says which piece of story answers which engine moment — and `lute play`, the reference player that walks a scripted playthrough through a whole project: raised occasions, the engine's own writes, a save to start from, and assertions `lute test` runs (dsl 0.22.0); occasions that play a whole sequence, side remarks, deadlines, targeted objectives, and beat bundles (dsl 0.23.0)."
---

A narrative game picks its next piece of story at moments of its own: a hub visit, entering a room, talking to an NPC, a new day, the start of a run. Lute calls those moments **occasions** and the pieces of story that answer them **beats** (dsl 0.21.0). The engine raises occasions; Lute defines which beats are eligible and which one wins. `lute play` is the reference player for that contract: give it a script of raised occasions and it walks the whole project, printing every candidate beat, its verdict, and the winner, and playing the winner through the same reference runner as `lute run`.

Since 0.22.0 a play script also stands in for the rest of the engine. An `engine:` step writes the state and facts the engine owns — a day advancing, a run counter, a kill — a script can start from a save, decisions can differ per step, a step can fire a world event, and `expect:` turns a playthrough into an assertion that [`lute test`](/tooling/cli/#test) runs beside the scenario tests. A project no longer needs scenes, occasions, or plugins that exist only to fake the engine.

Since 0.23.0 an occasion can present **every** eligible beat in turn (`select: sequence`), a beat can ride along after the winner (`also: true`), an objective can have a deadline (`by=`) or wait for one target (`target=`), and a lore document can hold several scene-like **bundle beats**. To see which beat would play in every state rather than walk one playthrough, use the [story overviews](/tooling/overviews/) — `lute calendar` evaluates this page's eligibility over a grid of state values.

The normative text is the [0.21.0 proposal](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.21.0.md), extended by the [0.22.0 proposal](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.22.0.md) (the play and test harness) and the [0.23.0 proposal](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.23.0.md) (composing occasions, deadlines, bundles); the engine-side contract (the IR fields and the selection algorithm an engine implements) is [`docs/runtime/beats-and-occasions.md`](https://github.com/journeyWorker/lute/blob/main/docs/runtime/beats-and-occasions.md). Before 0.21.0, `lute play` walked a tick-clock schedule file; that layer, its clock/lane/placement model, and its flags are removed. Time is now one input to a beat's condition, not the frame.

## Occasions

An occasion is **engine vocabulary**: a named moment the engine raises, optionally *for* a target (`talk` → `npc.achilles`). A plugin declares its occasions through an `occasions` export — list it in `plugin.yaml` (`exports: { occasions: occasions/ }`) and put the map in `occasions/*.yaml`:

```yaml
occasions:
  hubVisit:  { select: first }
  talk:      { select: first, target: { prefix: npc, entity: person } }
  roomEnter: { select: first, target: true }
  inbox:     { select: all, description: Letters waiting at the fountain }
```

- `select: first` (the default) — the engine presents the single winning beat.
- `select: all` — the engine offers every eligible beat, in selection order, and the player picks one (a message inbox, an area map, quest givers "in any order").
- `select: sequence` (dsl 0.23.0) — the engine presents **every** eligible beat, one after another, in selection order: a routine followed by the day's event, a run's opening followed by a recap. See [Composing occasions](#composing-occasions).
- `target: { prefix, entity }` (dsl 0.22.0) — the occasion is raised **for** something, and its targets are a vocabulary: `<prefix>.<member>`, where `<member>` is a member of the `entities:` kind `entity` (any id for an `open:` kind). `talk` above is raised for `npc.achilles` or `npc.patroclus` when the schema declares `person: { members: [achilles, patroclus] }`.
- `target: true` — raised for a target, shape only: any dotted id. Default `false`, not raised for anything.
- `description` — optional prose for tooling.

When no resolved plugin declares occasions, occasion names are **shape-only**: any identifier is accepted, so you can write beats before the engine's plugin exists. Once any plugin declares them, a beat naming an undeclared occasion is `E-OCCASION-UNKNOWN`, and a `target` on an occasion declared without a target is `E-BEAT-ATTR`. On an occasion with a target domain, a beat target outside it — `npc.achiles` for `npc.achilles` — is `E-BEAT-ATTR` with a did-you-mean, and so is a domain naming an entity kind the project does not declare. The export folds into the capability snapshot as a guarded section, so a project whose plugins declare no occasions keeps its `capabilityVersion`. A target domain is part of the snapshot; `target: true` hashes exactly as it did in 0.21.

Quest objectives judged at an occasion (`<objective on="runEnd">`) take `on=`, and since 0.23.0 an optional `target=` — `<objective on="talk" target="npc.maud">` is judged only when `talk` is raised for `npc.maud`, and is checked like a beat target (`E-BEAT-ATTR`: a dotted id, only beside `on`, on an occasion that takes a target, inside its domain). See [Deadlines and targeted objectives](#deadlines-and-targeted-objectives).

## Beats

### Scene beats

A scene becomes a beat by naming the occasion it answers in its frontmatter:

```yaml
---
kind: scene
id: achilles.gift
on: talk
target: npc.achilles
when: 'user.runs >= 10'
priority: 50
once: user
---
```

| Key | Meaning |
|---|---|
| `on` | The occasion this scene answers. Makes the scene a beat. |
| `target` | Optional. The scene is a candidate only when the occasion is raised for this target (a dotted id, the `<entry target>` shape; `<prefix>.<member>` when the occasion declares a target domain). |
| `when` | Optional CEL condition over `run` / `user` / `app` state, `quest.*`, `entry.<id>.read` / `entry.<id>.everRead`, and fact queries. The scene's own `scene.*` state does not exist yet and is rejected. |
| `priority` | Optional integer, default `0`. Higher wins. |
| `once` | `run` (the default — once per run), `user` (once ever), or `false` (repeatable). |
| `also` | Optional `true` (dsl 0.23.0): on a `select: first` occasion the beat never wins — it is presented **after** the winner, as a side remark, and even when no main beat is eligible. See [Composing occasions](#composing-occasions). |

`after:` keeps its meaning — the structural prerequisite over `visited` / `completed` / `active` that [connectivity](/connectivity/scene-graph/) analyzes — and a beat is eligible only when both `after:` and `when` hold. `when`, `target`, `priority`, `once`, or `also` without `on` is `E-BEAT-ATTR`: a scene that answers no occasion is reached by explicit flow, as before. So is an `also` that is not a bool, or `also: true` on a `select: all` or `select: sequence` occasion, where every eligible beat is already presented or offered.

### Entry beats

A [lore entry](/language/lore-entries/) answers an occasion with `on=` and `priority=` beside its existing `target` and `when`, and since 0.22.0 an optional `once=`:

```lute
<entry id="achillesBark" on="talk" target="npc.achilles" category="bark" priority="10" once="run" when="user.runs >= 3">
  @achilles: Back again, lad.
</entry>
```

Without `once`, an entry is repeatable: re-presentation is its nature, and its effects apply on the first read only. `once="run"` makes it ineligible once its run-tier `entry.<id>.read` flag is set, so it is heard once per run — a `newRun` resets the flag. `once="user"` makes it ineligible once `entry.<id>.everRead` is set: a reserved **user-tier** flag beside `read`, set on the first read and never reset by a new run, readable wherever `entry.<id>.read` is. A bad `once`, or `once` without `on`, is `E-BEAT-ATTR`.

### Bundle beats

A scene is one beat per file. When a character has many short scenes — an NPC's lines at each visit, a set of interview questions — a [lore document](/language/lore-entries/#entries-and-beats-in-one-file) can hold them as **bundle beats** (dsl 0.23.0): `<beat>` blocks with a full scene body — lines, branches, hubs, `<match>`, directives — and the same attributes a scene beat puts in its frontmatter (`on`, `target`, `when`, `priority`, `once`, `also`, and a `title` for a `select: all` menu). In the tower the [examples below](#lute-play) climb, `lore/oskar.lute` declares `id: oskar` and gives Oskar two:

```lute
<beat id="hunt" on="talk" target="npc.oskar" priority="10" title="The hound">
  @oskar: The hound took my dog's collar. Bring it back before you pass floor four.
  ::accept{quest="houndHunt"}
</beat>

<beat id="rumor" on="talk" target="npc.oskar" also="true" once="false" when="run.floor >= 2">
  @oskar: They say the warden sleeps on floor six.
</beat>
```

A bundle beat behaves like a scene beat, not an entry: its id is `<document id>.<beat id>` — `oskar.hunt`, `oskar.rumor` — so the document needs an `id:`; its `once` defaults to `run` and is spent by presentation; presenting it marks that id visited, so `visited('oskar.hunt')` reads it in any condition (an `after:` still names only scenes and quests); and `check-project` judges it like any beat (`E-BEAT-UNREACHABLE`, `W-BEAT-SHADOWED`, `W-BEAT-PRIORITY-TIE`, `W-BEAT-ONCE-RUN-USER`). The language rules are on [Beats](/language/beats/#beat-bundles). In a transcript a bundle beat's kind is `beat`; `lute trace --beat` and `lute run --beat` present one on its own (see [Tracing](/tooling/tracing/#bundle-beats)).

## Selection

When the engine raises occasion `O`, optionally for target `T`:

1. **Candidates** are the beats with `on: O` whose `target` is absent or equal to `T`. An occasion raised without a target has only untargeted candidates.
2. A candidate is **eligible** when its `after:` holds (scene beats), its `when` holds, and its `once` is not spent. A scene's (or bundle beat's) `once`: `run` — not yet presented this run; `user` — never presented; `false` — never spent. An entry's `once`: `run` — `entry.<id>.read` not set; `user` — `entry.<id>.everRead` not set; absent — never spent.
3. Eligible beats are **ordered by priority, descending, then project order**: document path, then declaration order within the document — the order of `beats` in `project.index.json`. Scene, entry, and bundle beats on the same occasion compete in one list.
4. `select: first` presents the first eligible beat that is not `also`, then every eligible `also` beat; `select: all` offers the ordered list and presents the one the player picks; `select: sequence` presents the whole ordered list.
5. **No eligible beat** — the occasion passes with no story, and the engine's default behavior for that moment applies.

Selection is deterministic: the same state, facts, and presentation history pick the same beat in every engine. Weighted randomness or cooldowns are engine policy layered on top; the reference tooling implements exactly this order. When file order is what decides a `select: first` winner — two beats with equal priority whose `when`s are not provably exclusive — `check-project` warns `W-BEAT-PRIORITY-TIE`; see [Beats](/language/beats/) for it and the other beat advisories.

For `select: sequence` and `also`, eligibility is decided **once, when the occasion is raised**: presenting the first beat does not make a later one eligible or ineligible. The quest lifecycles settle after each presentation, so a [deadline](#deadlines-and-targeted-objectives) is judged between beats. `W-BEAT-SHADOWED` and `W-BEAT-PRIORITY-TIE` ignore `also` beats, which never compete for the win.

## `lute play`

```console
$ lute play <PROJECT_DIR> --script <FILE> [--json] [--no-derive] [--explain <ATOM>]…
```

- `<PROJECT_DIR>` — the project root (`lute.project.yaml` and its plugins). The project is compiled whole, in memory, with the same gate and declaration union `compile --all` uses (scene, quest, and lore documents).
- `--script <FILE>` — required: the play script, a `*.play.yaml` file.
- `--json` — the same transcript as one JSON object on stdout.
- `--no-derive` — do not apply the project's Datalog rules (dsl 0.22.0 §6); overrides the script's `derive:`. See [Derivation and `--explain`](#derivation-and---explain).
- `--explain <ATOM>` — repeatable: after the play, print why a ground atom holds or does not.

There is nothing else to seed on the command line: state, facts, the save, decisions, and assertions all live in the script, so a playthrough is one reviewable file.

The examples in this section play a small roguelike project: a tower the player climbs run after run. Its plugin declares four occasions, one world event (an `events` export — see [Manifests](/plugins/manifests/)), and one reward kind whose grants [credit state](/plugins/manifests/#rewards-that-credit-state) (dsl 0.23.0):

```yaml
occasions:
  hubVisit: { select: first }
  talk:     { select: first, target: { prefix: npc, entity: person } }
  board:    { select: all, description: Notices pinned by the stair }
  runStart: { select: sequence, description: A run begins at the foot of the stair }
```

```yaml
events:
  - name: storm
```

```yaml
rewardKinds:
  EMBERS: { credits: user.embers }
```

Its world schema gives the engine the floor and the run counter, the player a purse of embers, a reserved kill fact, and one rule — the warden is a threat until it is slain:

```yaml
state:
  run.floor:   { type: number, default: 0, owner: engine }
  user.runs:   { type: number, default: 0, owner: engine }
  user.embers: { type: number, default: 0 }
entities:
  person: { members: [maud, oskar] }
  foe:    { members: [warden, hound] }
relations:
  boss:   { args: [foe] }
  slew:   { args: [foe], tier: run, reserved: true }
  threat: { args: [foe], derive: true }
facts:
  - "boss(warden)"
rules:
  - "threat(F) :- boss(F), not slew(F)"
```

The beats: `hub.idle` answers `hubVisit` every time (`once: false`); `hub.victory` (priority 10, `when: "!holds(threat(warden))"`, also `once: false`) outranks it as soon as the warden is no longer a threat; `maud.talk` answers `talk` for `npc.maud`; Oskar's two [bundle beats](#bundle-beats), `oskar.hunt` and the side remark `oskar.rumor`, answer `talk` for `npc.oskar`; `start.gear` (priority 10, `once: false`) and `start.recap` (`once: false`, `when: "isSet(prev.run.floor)"`, the [previous run's](#run-boundaries) floor) answer `runStart`; and three entries answer `board` — `notice` (`once="user"`), `memo` (`once="run"`), and `old` (`when="entry.notice.everRead"`). One quest document holds three quests, all `start="true"`:

```lute
<quest id="climb" title="Reach the fifth floor" start="true" tier="run">
  <objective id="high" title="Reach floor five" done="run.floor >= 5"/>
  <on event="storm">
    @narrator: Thunder rolls over the stair.
  </on>
</quest>

<quest id="veteran" title="Three runs" start="true">
  <objective id="three" title="Climb three times" done="user.runs >= 3"/>
</quest>

<quest id="notices" title="Read the board" start="true">
  <objective id="looked" title="Look at the board" on="board" done="true"/>
</quest>
```

A second, `quests/hound.lute`, holds the quest `oskar.hunt` accepts. Its collar has a deadline, the report waits for Oskar, and the reward pays out in embers:

```lute
<quest id="houndHunt" title="The hound's collar">
  <reward kind="EMBERS" amount="50"/>
  <objective id="collar" title="Take the collar before floor four" done="holds(slew(hound))" by="run.floor >= 4"/>
  <objective id="report" title="Bring it to Oskar" on="talk" target="npc.oskar" done="holds(slew(hound))"/>
  <on event="questFailed">
    @oskar: Floor four already? Then it's gone to ground.
  </on>
</quest>
```

### The play script

A play script is a YAML mapping. `steps` is required; every other top-level key is optional:

| Key | Meaning |
|---|---|
| `steps` | Required, non-empty: what happens, in order. |
| `state` | Seeds: state path → scalar literal, over the declared defaults. |
| `facts` | Ground facts added to the project's seed facts. |
| `choose` | Branch/hub id → choice id (a list for a hub's visit sequence or a branch that decides differently each time): the decision for every presentation. |
| `visited`, `presented`, `quests`, `entriesRead` | The save the play starts from — see [Starting from a save](#starting-from-a-save). |
| `expect` | Assertions about the end of the play — see [Expectations](#expectations). |
| `derive` | `false` stops applying the project's Datalog rules — see [Derivation and `--explain`](#derivation-and---explain). |

Every step does exactly one thing — raises an `occasion`, applies `engine` writes, starts a `newRun`, or fires an `event` — and any step may carry a `label` and a `repeat` count. A tour of every shape, against the tower:

```yaml
state: { user.runs: 2 }                   # path -> scalar literal, over the declared defaults
facts: ["slew(hound)"]                    # ground facts, added to the project's seed facts
entriesRead: { user: [notice] }           # the save this play starts from
steps:                                    # required, non-empty
  - occasion: hubVisit                    # raise an occasion
    expect: { winner: hub.idle }          # assert what this step did
  - occasion: talk
    target: npc.maud                      # a targeted occasion: a target in its domain
  - occasion: board
    pick: none                            # `select: all`: a beat id, or `none`
  - event: storm                          # fire a declared world event
  - label: the warden falls on floor six  # printed in the step header
    engine:                               # write what the engine owns
      state: { run.floor: 6, user.runs: { add: 1 } }
      facts: [slew(warden)]
  - newRun: { state: { run.floor: 1 } }   # start a new run (`newRun: true` without a seed)
  - occasion: hubVisit
    repeat: 2                             # the same step, twice
expect:                                   # assert the end of the play
  quests: { climb: active, veteran: complete }
  notFacts: [slew(warden)]
```

Its transcript is the example in [The transcript](#the-transcript).

`state:`, `facts:`, and `choose:` use exactly the grammar of a [`lute trace --mock`](/tooling/tracing/) file.

- A `state:` seed names a declared path — never `scene.*` — and its value must fit the declared type (a number for a `number`, a member for an enum); anything else is a usage error (exit 2). A `quest.<id>.state` seed (`state: { quest.lostCup.state: active }`) is that quest's lifecycle status from the start, exactly as a `quests:` entry. A `prev.run.<path>` seed (dsl 0.23.0) is the value `run.<path>` had when the previous run ended, typed like its run path — see [Run boundaries](#run-boundaries).
- A `facts:` entry is a ground atom of a declared, non-derived relation at its arity, with members of its closed argument domains. A **reserved** relation is allowed — the engine is exactly who asserts one.
- A single `choose:` decision answers every presentation of its branch or hub. A list for a **hub** is one visit sequence, replayed at each presentation. A list of two or more for a **branch** is consumed one entry per presentation, in order, across the whole playthrough — so a scene that plays on three nights can decide differently each night. When the list runs out, the walk halts incomplete (exit 3) and says so.
- A decision the menu does not offer at that moment halts the walk with an error (exit 1): a choice whose guard is false, or a `once` hub option already taken (`E-TRACE-CHOICE`, as in `lute trace`).

### Occasion steps

`{ occasion, target?, pick?, choose?, expect? }` raises an occasion, exactly as the engine would.

- `target` — required on an occasion declared with a target, refused on one without. With a target domain, the target must be `<prefix>.<member>` of it; outside it is a usage error with a did-you-mean (`` target `npc.mawd` is outside occasion `talk`'s domain `npc.<person>` (`npc.maud`, `npc.oskar`) — did you mean `npc.maud`? (dsl 0.22.0 §8) ``). A member that no beat answers is legal: the occasion passes. The target also decides which [targeted objectives](#deadlines-and-targeted-objectives) the step judges.
- `pick` — required on a `select: all` occasion, refused on `select: first` and `select: sequence` (`` step 1: `pick: start.gear` applies only to a `select: all` occasion; `runStart` is `select: sequence` ``): the id of a beat answering the occasion (a pick that is not eligible at that moment is an error, exit 1), or `pick: none`. `none` closes the list — the player walks past the board: nothing is presented and nothing is spent, but the occasion's `<objective on>` objectives are still judged:

```yaml
steps:
  - occasion: board
    pick: none
```

```
── start ──────────────
  quest climb -> active
  quest veteran -> active
  quest notices -> active
── step 1 · board (select: all, pick: none) ──────────────
  ✓ notice [entry, priority 0]
  ✓ memo [entry, priority 0]
  ✗ old [entry, priority 0] — when: false
  → (pick: none — the list closes; nothing presented)
  notices.looked done
  quest notices -> complete
── end: complete (1 step) ──────────────
```

- `choose` — this step's own decisions. For this presentation it replaces the script's `choose:` key by key; the script's map stays the default for every other step. A step-local list starts from its first entry and leaves the consumption of the script-wide list for the same id where it was. The [worked example](#one-step-decided-the-other-way) uses one.
- `expect` — what this step must have done; see [Expectations](#expectations).

### Engine steps

`{ engine: { state?, facts?, retract? } }` writes what the engine owns, as the engine would between occasions:

- `state:` — declared path → literal, or `{ add: <number> }` to add to a `number` path's current value. A path is refused when it is undeclared, `scene.*`, or a `quest.*` path: quest status belongs to the quest lifecycle, whose transitions fire handlers and grants — seed a save's quest status with top-level `quests:` instead.
- `facts:` / `retract:` — ground atoms of declared base relations, **reserved ones included**; the same checks as top-level `facts:`. Retracting an atom that does not hold is recorded, not refused.

Writes apply in that order — state, then facts, then retractions. The step presents nothing and raises no occasion; the quest lifecycle settles right after it, so a write can complete or fail a quest at that step. [`owner: engine`](/state/state-model/#owner-engine) state is exactly what these steps are for: content may not `::set` it, but an `engine:` step may — as it may any other declared state outside `scene.*` and `quest.*`.

```yaml
steps:
  - occasion: hubVisit
  - label: the warden falls on floor six
    engine:
      state: { run.floor: 6 }
      facts: [slew(warden)]
  - occasion: hubVisit
  - engine:
      retract: [slew(warden), slew(hound)]
  - occasion: hubVisit
```

```
── start ──────────────
  quest climb -> active
  quest veteran -> active
  quest notices -> active
── step 1 · hubVisit ──────────────
  ✓ hub.idle [scene, priority 0]
  ✗ hub.victory [scene, priority 10] — when: false
  → hub.idle
@maud: Quiet night.
── step 2 (the warden falls on floor six) · engine ──────────────
  set run.floor = 6
  assert slew(warden)
  climb.high done
  quest climb -> complete
── step 3 · hubVisit ──────────────
  ✓ hub.victory [scene, priority 10]
  ✓ hub.idle [scene, priority 0]
  → hub.victory
@maud: The warden is dead. I never thought I'd say it.
── step 4 · engine ──────────────
  retract slew(warden)
  retract slew(hound) (did not hold)
── step 5 · hubVisit ──────────────
  ✓ hub.idle [scene, priority 0]
  ✗ hub.victory [scene, priority 10] — when: false
  → hub.idle
@maud: Quiet night.
── end: complete (5 steps) ──────────────
```

The kill makes `threat(warden)` stop deriving, so `hub.victory` becomes eligible at step 3; the retraction closes it again.

### Events

`{ event: <name> }` fires a world event some plugin declares, exactly as trace's `events:` does: the `<on event="<name>">` handlers of every **active** quest run, then the lifecycle settles. An `event:` naming an occasion, or an `occasion:` naming a world event, is a usage error that names the right key (`` `event: dayEnd` names no declared world event — `dayEnd` is an occasion; raise it with `occasion: dayEnd` ``). The quest lifecycle events `questActive` / `questComplete` / `questFailed` fire on transitions and cannot be fired by a script.

```yaml
steps:
  - event: storm
  - engine: { state: { run.floor: 5 } }
  - event: storm
```

```
── start ──────────────
  quest climb -> active
  quest veteran -> active
  quest notices -> active
── step 1 · event storm ──────────────
@narrator: Thunder rolls over the stair.
── step 2 · engine ──────────────
  set run.floor = 5
  climb.high done
  quest climb -> complete
── step 3 · event storm ──────────────
── end: complete (3 steps) ──────────────
```

`climb` completed at step 2, so its handler no longer answers the second storm.

### Labels and repetition

`label: <text>` names a step: it is printed in the step header (`── step 4 (the engine closes the day) · engine`), carried in `--json`, and named by every expectation miss on the step. `repeat: <n>` (a whole number ≥ 1) runs the step `n` times — `── step 7 [1/2] · hubVisit`, `── step 7 [2/2] · hubVisit` — and each repetition is its own step record, settles the quest lifecycle on its own, and counts in `── end: complete (<n> steps)`. `repeat` suits the engine's routine: three runs ending (`engine: { state: { user.runs: { add: 1 } } }`, `repeat: 3`), or a visit the player makes every day.

### Starting from a save

Four top-level keys seed the playthrough's history before step 1, so a script can begin where a real player's save stands instead of replaying everything before it:

| Key | Meaning |
|---|---|
| `visited: [scene ids]` | Scenes presented in this save — read by `visited('<id>')` and `after: visited(…)`. |
| `presented: { run: [beat ids], user: [beat ids] }` | Scene beats already presented: `user` — in an earlier run, so a `once: user` beat is spent; `run` — in the current run, so `once: run` and `once: user` are both spent. Every listed scene also counts as visited. |
| `quests: { <id>: unset \| active \| complete \| failed }` | Quest lifecycle status. The start settle resumes it rather than starting the quest over. |
| `entriesRead: { run: [entry ids], user: [entry ids] }` | `run` — read in the current run: `entry.<id>.read` and `entry.<id>.everRead`; `user` — read in an earlier run: `entry.<id>.everRead` only. |

An id the project does not declare is a usage error with a did-you-mean (`` `visited:` names `hub.welcom`, which is no scene in this project — did you mean `hub.welcome`? ``), and so is an entry under `presented:` (an entry's read history is `entriesRead:`) or a status outside the four.

```yaml
quests: { veteran: complete }
entriesRead: { user: [notice], run: [memo] }
steps:
  - occasion: board
    pick: old
```

```
── start ──────────────
  quest climb -> active
  quest notices -> active
── step 1 · board (select: all, pick: old) ──────────────
  ✓ old [entry, priority 0]
  ✗ notice [entry, priority 0] — once: user — already read
  ✗ memo [entry, priority 0, read] — once: run — already read this run
  → old
  entry old (first read)
@maud: The same notice as ever.
  notices.looked done
  quest notices -> complete
── end: complete (1 step) ──────────────
```

`veteran` stays complete — it is not activated again at the start — and the save's reads spend both entries while opening `old`. `memo` is marked `read` (dsl 0.23.0): an entry beat whose `entry.<id>.read` is set in this run says so in its candidate line, eligible or not, so a `select: all` menu can show which entries the player has already seen. `notice` was read in an earlier run only, so it is not.

### Run boundaries

`newRun: true` starts a new run. First it snapshots every `run.*` value as **`prev.run.*`** (dsl 0.23.0) — the value each path had when the run ended — then it resets:

- `run.*` state to its declared defaults, and every run-tier `entry.<id>.read` flag, so an entry's effects apply again on its first read in the new run and a `once="run"` entry is eligible again;
- run-tier facts to the project's seed facts;
- every `<quest tier="run">` quest to `unset`, with its objectives undone — a `start`-having one activates again at the settle that follows, an accept-driven one waits for a new accept;
- `once: run` spending.

`user.*` / `app.*` state, user-tier quests (the default `tier`), `entry.<id>.everRead`, user- and app-tier facts, the `visited` history, and `once: user` spending persist. `once: user` across separate `lute play` invocations is not modelled — put the runs in one script separated by `newRun` steps, or start from a save.

The long form `newRun: { state: {…}, facts: […] }` then applies its writes as the new run's seed — the same `state:` (literal or `{ add: n }`) and `facts:` rules as an `engine:` step, without `retract:`. Then the quest lifecycle settles.

```yaml
steps:
  - label: a run ends
    engine:
      state: { run.floor: 6, user.runs: { add: 1 } }
      facts: [slew(warden)]
  - newRun: { state: { run.floor: 1 } }
  - occasion: hubVisit
  - label: a run ends
    engine:
      state: { user.runs: { add: 1 } }
    repeat: 2
```

```
── start ──────────────
  quest climb -> active
  quest veteran -> active
  quest notices -> active
── step 1 (a run ends) · engine ──────────────
  set run.floor = 6
  set user.runs = 1
  assert slew(warden)
  climb.high done
  quest climb -> complete
── step 2 · new run ──────────────
  run.* state, run-tier facts and once: run reset
  quest climb -> unset (tier: run)
  set run.floor = 1
  quest climb -> active
── step 3 · hubVisit ──────────────
  ✓ hub.idle [scene, priority 0]
  ✗ hub.victory [scene, priority 10] — when: false
  → hub.idle
@maud: Quiet night.
── step 4 (a run ends) [1/2] · engine ──────────────
  set user.runs = 2
── step 4 (a run ends) [2/2] · engine ──────────────
  set user.runs = 3
  veteran.three done
  quest veteran -> complete
── end: complete (5 steps) ──────────────
```

`slew` is a run-tier relation, so the kill does not survive the new run and `hub.victory` is closed again; `climb` (`tier="run"`) starts over, while `veteran` keeps counting.

[`prev.run.<path>`](/state/state-model/#the-previous-run) is read-only and `unset` until a run has ended, so content must guard it (`isSet(prev.run.floor)`); the tower's `start.recap` does, and [Composing occasions](#composing-occasions) shows it after a `newRun`. A script that starts mid-save seeds it like any path — `state: { prev.run.floor: 5 }` — and the first `runStart` then plays the recap with `Floor 5 last time.`

Entry `once` across a run boundary, from the `board` entries:

```yaml
steps:
  - occasion: board
    pick: notice
  - occasion: board
    pick: memo
  - occasion: board
    pick: old
  - newRun: true
  - occasion: board
    pick: memo
```

```
── start ──────────────
  quest climb -> active
  quest veteran -> active
  quest notices -> active
── step 1 · board (select: all, pick: notice) ──────────────
  ✓ notice [entry, priority 0]
  ✓ memo [entry, priority 0]
  ✗ old [entry, priority 0] — when: false
  → notice
  entry notice (first read)
@maud: "Climbers wanted. No refunds."
  notices.looked done
  quest notices -> complete
── step 2 · board (select: all, pick: memo) ──────────────
  ✓ memo [entry, priority 0]
  ✓ old [entry, priority 0]
  ✗ notice [entry, priority 0, read] — once: user — already read
  → memo
  entry memo (first read)
@maud: "Floor three is flooded again."
── step 3 · board (select: all, pick: old) ──────────────
  ✓ old [entry, priority 0]
  ✗ notice [entry, priority 0, read] — once: user — already read
  ✗ memo [entry, priority 0, read] — once: run — already read this run
  → old
  entry old (first read)
@maud: The same notice as ever.
── step 4 · new run ──────────────
  run.* state, run-tier facts and once: run reset
  quest climb -> unset (tier: run)
  quest climb -> active
── step 5 · board (select: all, pick: memo) ──────────────
  ✓ memo [entry, priority 0]
  ✓ old [entry, priority 0]
  ✗ notice [entry, priority 0] — once: user — already read
  → memo
  entry memo (first read)
@maud: "Floor three is flooded again."
── end: complete (5 steps) ──────────────
```

### Composing occasions

A `select: first` occasion answers with one beat. Two additions (dsl 0.23.0) let an occasion say more without giving up that order.

**`select: sequence`** presents every eligible beat, in selection order. The tower raises `runStart` as a run begins: `start.gear` (priority 10) is the routine, and `start.recap` recalls the previous run's floor, so it is eligible only once a run has ended:

```yaml
steps:
  - occasion: runStart
  - label: the run ends on floor three
    engine: { state: { run.floor: 3, user.runs: { add: 1 } } }
  - newRun: true
  - occasion: runStart
    expect: { presented: [start.gear, start.recap] }
```

```
── start ──────────────
  quest climb -> active
  quest veteran -> active
  quest notices -> active
── step 1 · runStart (select: sequence) ──────────────
  ✓ start.gear [scene, priority 10]
  ✗ start.recap [scene, priority 0] — when: false
  → start.gear
@maud: Rope, lamp, bread. Up you go.
── step 2 (the run ends on floor three) · engine ──────────────
  set run.floor = 3
  set user.runs = 1
── step 3 · new run ──────────────
  run.* state, run-tier facts and once: run reset
  quest climb -> unset (tier: run)
  quest climb -> active
── step 4 · runStart (select: sequence) ──────────────
  ✓ start.gear [scene, priority 10]
  ✓ start.recap [scene, priority 0]
  → start.gear
  → start.recap
@maud: Rope, lamp, bread. Up you go.
@maud: Floor 3 last time. Beat it.
── end: complete (4 steps) ──────────────
── expect: every expectation held ──────────────
```

- **Step 1** — no run has ended, so `prev.run.floor` is unset and only the routine plays.
- **Step 3** — the `newRun` snapshots `run.floor` (3) into `prev.run.floor` before resetting it.
- **Step 4** — both beats are eligible: each gets its own `→` line, and each plays in turn and spends its own `once`. The step's `presented:` expectation asserts the whole list, in order.

Which beats play is decided when the occasion is raised: a beat that becomes eligible because an earlier beat in the list changed the state does not join, and one that stops being eligible still plays. The quest lifecycles settle after **each** presentation, so an objective — or a deadline — is judged between two beats of one step. There is nothing to pick, so `pick:` on a `sequence` occasion is a usage error.

**`also: true`** makes a beat a side remark on a `select: first` occasion. It never wins, whatever its priority; when it is eligible it is presented **after** the winner — or on its own, when no main beat is eligible. Oskar's `rumor` bundle beat is one:

```yaml
steps:
  - engine: { state: { run.floor: 2 } }
  - occasion: talk
    target: npc.oskar
  - label: the hound falls
    engine: { facts: [slew(hound)] }
  - occasion: talk
    target: npc.maud
  - occasion: talk
    target: npc.oskar
expect:
  quests: { houndHunt: complete }
  state: { user.embers: 50 }
```

```
── start ──────────────
  quest climb -> active
  quest veteran -> active
  quest notices -> active
── step 1 · engine ──────────────
  set run.floor = 2
── step 2 · talk → npc.oskar ──────────────
  ✓ oskar.hunt [beat, priority 10]
  ✓ oskar.rumor [beat, priority 0, also]
  → oskar.hunt
  + oskar.rumor (also)
  beat
@oskar: The hound took my dog's collar. Bring it back before you pass floor four.
  quest houndHunt accepted
  beat
@oskar: They say the warden sleeps on floor six.
  quest houndHunt -> active
── step 3 (the hound falls) · engine ──────────────
  assert slew(hound)
  houndHunt.collar done
── step 4 · talk → npc.maud ──────────────
  ✓ maud.talk [scene, priority 0]
  → maud.talk
@maud: Up again?
── step 5 · talk → npc.oskar ──────────────
  ✓ oskar.rumor [beat, priority 0, also]
  ✗ oskar.hunt [beat, priority 10] — once: run — already presented this run
  → (no eligible main beat)
  + oskar.rumor (also)
  beat
@oskar: They say the warden sleeps on floor six.
  houndHunt.report done
  quest houndHunt -> complete
  grant houndHunt EMBERS 50 (credits user.embers = 50.0)
── end: complete (5 steps) ──────────────
── expect: every expectation held ──────────────
```

- **Step 2** — `oskar.hunt` wins; the eligible `also` beat is listed `+ oskar.rumor (also)` and plays after it. Both are bundle beats, so each presentation opens with its `beat` record. The hunt accepts `houndHunt`, which activates in the settle after the step.
- **Step 5** — the hunt is spent (`once` defaults to `run`), so no main beat is eligible — `→ (no eligible main beat)` — and the side remark still plays. The rest of the transcript is the quest: see [Deadlines and targeted objectives](#deadlines-and-targeted-objectives).

An `also` beat spends its `once` when presented, like any beat; `oskar.rumor` is `once="false"`, so it repeats. In `--json`, a step's `presented` is the winner — or, with no winner, the first `also` beat — and `then` lists the beats presented after it, in order, each with `"also": true` when it is one; the candidate records carry `"also": true` too:

```json
{
  "step": 2,
  "occasion": "talk",
  "target": "npc.oskar",
  "select": "first",
  "candidates": [
    { "id": "oskar.hunt", "kind": "beat", "document": "lore/oskar.lute", "priority": 10, "eligible": true },
    { "id": "oskar.rumor", "kind": "beat", "document": "lore/oskar.lute", "priority": 0, "eligible": true, "also": true }
  ],
  "winner": "oskar.hunt",
  "presented": { "id": "oskar.hunt", "kind": "beat", "document": "lore/oskar.lute", "commands": […], "stateDelta": {} },
  "then": [
    { "id": "oskar.rumor", "kind": "beat", "document": "lore/oskar.lute", "also": true, "commands": […], "stateDelta": {} }
  ]
}
```

A `select: sequence` step uses the same two keys: `presented` is the first beat of the list and `then` the rest.

### Deadlines and targeted objectives

Two objective attributes (dsl 0.23.0) change **when** an objective is judged — the language side is in [Quests & scenes](/language/quests-and-scenes/#deadlines):

- `by="<condition>"` — a deadline. While the objective is not done, the first time `by` holds the objective **fails** and is never judged again. A failed required objective fails its quest: `failed` rewards, the `questFailed` handlers, and the cascade to a parent quest. `done` is judged first in every settle, so an objective done at the moment its deadline passes does not fail, and `by` is judged at every settle — after a presentation, an `engine:` write, a `newRun` — even on an `on=` objective.
- `target="<target>"`, beside `on=` — the objective is judged only at an occasion step raised **for that target**. An occasion step with another target, or none, leaves it alone.

In the [`also` example](#composing-occasions) above, the collar objective completes in the settle after step 3, when the engine asserts the kill. Step 4 raises `talk` for Maud, so the report objective (`on="talk" target="npc.oskar"`) is not judged; step 5 raises it for Oskar, and the quest completes. Without the kill, the climb passes the deadline:

```yaml
steps:
  - occasion: talk
    target: npc.oskar
  - label: the climber passes floor four
    engine: { state: { run.floor: 4 } }
```

```
── start ──────────────
  quest climb -> active
  quest veteran -> active
  quest notices -> active
── step 1 · talk → npc.oskar ──────────────
  ✓ oskar.hunt [beat, priority 10]
  ✗ oskar.rumor [beat, priority 0, also] — when: false
  → oskar.hunt
  beat
@oskar: The hound took my dog's collar. Bring it back before you pass floor four.
  quest houndHunt accepted
  quest houndHunt -> active
── step 2 (the climber passes floor four) · engine ──────────────
  set run.floor = 4
  houndHunt.collar failed (by)
  quest houndHunt -> failed
@oskar: Floor four already? Then it's gone to ground.
── end: complete (2 steps) ──────────────
```

`houndHunt.collar failed (by)` is the deadline (`--json`: an `objective` record with `"failed": true`), and the required objective fails the quest, whose `questFailed` handler plays. A failure is lifecycle state, not a state path; a `newRun` clears it for a `tier="run"` quest.

**Rewards that credit state.** The tower's `EMBERS` reward kind declares [`credits: user.embers`](/plugins/manifests/#rewards-that-credit-state), so when step 5 above grants the quest's reward, the scalar amount is added to that path — `grant houndHunt EMBERS 50 (credits user.embers = 50.0)`, and the end-of-play `state: { user.embers: 50 }` holds. In `--json`, the `grant` record carries `"credited": { "path": "user.embers", "value": 50.0 }`. A range amount is the engine's roll and credits nothing here, and a `::set` of the same path in one of the quest's own `<on>` or objective bodies would pay twice (`W-REWARD-DOUBLE-CREDIT`).

### Expectations

An occasion step may carry `expect:`, judged against what that step did:

| Key | Holds when |
|---|---|
| `winner: <beat id>` | that beat was presented; `winner: none` — nothing was (no eligible beat, or `pick: none`) |
| `offered: [beat ids]` | every listed beat was eligible at the step — a subset, in any order |
| `notOffered: [beat ids]` | none of the listed beats was eligible |
| `presented: [beat ids]` | exactly these beats were presented, in this order (dsl 0.23.0): the whole list of a [`select: sequence`](#composing-occasions) step, or a winner followed by its `also` beats; `[]` — nothing was |

A top-level `expect:` judges the end of the play:

| Key | Holds when |
|---|---|
| `exit: complete \| incomplete \| error` | the walk ended that way |
| `quests: { <id>: <status> }` | the quest ended in that status (`unset` for one nothing activated) |
| `state: { <path>: <value> }` | the path's final **effective** value — the last write, else the seed, else the declared default — equals the value, compared typed (`1` is not `"1"`) |
| `facts: [atoms]` / `notFacts: [atoms]` | each atom holds / does not hold at the end, **after derivation** |
| `transcriptContains: [text]` / `transcriptLacks: [text]` | each text is / is not a substring of the human transcript |

An expectation on a `repeat:` step is judged at every repetition; one on a step the walk never reached is itself a miss. Each `expect:` is validated before anything plays — an unknown key is a usage error (exit 2) listing the legal keys, with a did-you-mean, and saying when the key belongs at the other level (`` unknown top-level `expect:` key `winner` (`winner` belongs in a step `expect:`) ``).

After the transcript, a script with any expectation prints `── expect: every expectation held` or `── expect: <n> missed`, one line per miss naming the step, its label, the occasion, the repetition, and the actual value:

```
── expect: 3 missed ──────────────
  ✗ step 3 (ask about the oil) at talk npc.tomas: expect winner: expected tomasOil, actual tomasBusy
  ✗ end of play: expect quests lampOut: expected active, actual unset
  ✗ end of play: expect state user.bond.mara: expected 1, actual 0
```

A `presented:` miss prints both lists, so an order mistake is visible at a glance:

```
── expect: 1 missed ──────────────
  ✗ step 1 at runStart: expect presented: expected [start.recap, start.gear], actual [start.gear, start.recap]
```

A miss makes `lute play` exit 1, unless the walk itself already ended in an error (exit 1) or a runner failure on a malformed artifact (exit 2). A walk that halted incomplete (3) with every expectation holding still exits 3.

`lute test` runs every `*.play.yaml` under its directory that carries an `expect:` — on a step or at the top — alongside the `*.test.yaml` scenario tests, against `--project` or else the nearest `lute.project.yaml` above the play, with a `PASS` / `FAIL` line each (`--json`: entries with `"kind": "play"` and their `misses`). A play without `expect:` is not a test and is skipped. A play that halts fails unless its top-level `expect:` declares the exit (`expect: { exit: incomplete }`). With `--coverage`, every document a play presented, and every quest document whose lifecycle it moved, counts as covered. See [`lute test`](/tooling/cli/#test).

### Derivation and `--explain`

`lute play` evaluates every `when`, `done`, and guard over the live facts **with the project's Datalog rules applied** (stratified negation) — the same evaluator `lute run`, and since 0.22.0 `lute trace` and `lute test`, use. The project's seed facts are loaded, and the script's `facts:`, `engine:` writes, and every `::assert` / `::retract` a presentation makes feed the fixpoint.

`derive: false` in the script, or `--no-derive` on the command line (the flag wins), stops applying the rules: the seed facts still load, but every atom of a derived relation reads unknown — a script cannot assert one. A `when` that depends on one halts the walk incomplete (exit 3), and the end-of-play `facts:` / `notFacts:` expectations see base facts only:

```console
$ lute play tower --script tower/plays/night.play.yaml --no-derive
```

```
── start ──────────────
  quest climb -> active
  quest veteran -> active
  quest notices -> active
── step 1 · hubVisit ──────────────
  ✓ hub.idle [scene, priority 0]
  ? hub.victory [scene, priority 10] — when: unknown (`!holds(threat(warden))` evaluates unknown: fact `threat(warden)` is undetermined)
── halted: step 1: the `when` of scene `hub.victory` (scenes/hub-victory.lute) decides the hubVisit outcome but `!holds(threat(warden))` evaluates unknown: fact `threat(warden)` is undetermined ──────────────
```

`--explain <atom>` (repeatable) prints, after the play, why a ground atom holds at the end or why it does not. When it holds: the rule used, and each premise's own support — a `seed fact`, `asserted` during the play, or derived in turn, indented beneath it — with a negated premise shown `(absent)`. When it does not: every rule that could conclude it, with its premises marked — `✗ <atom>  (absent)` for a missing base fact, `✗ <atom>  (not derived)` for a missing derived one (explained in turn), `✗ not <atom>  (but it holds: …)` for a negated premise that is present, `✗ <test>  (false)` / `? <test>  (undecided)` for a comparison or guard, and `· <premise>  (not reached)` for premises after the first failure. With `plays/night.play.yaml` a single `hubVisit` step:

```console
$ lute play tower --script tower/plays/night.play.yaml --explain "threat(warden)" --explain "threat(hound)"
```

```
── start ──────────────
  quest climb -> active
  quest veteran -> active
  quest notices -> active
── step 1 · hubVisit ──────────────
  ✓ hub.idle [scene, priority 0]
  ✗ hub.victory [scene, priority 10] — when: false
  → hub.idle
@maud: Quiet night.
── end: complete (1 step) ──────────────
explain threat(warden): holds
  threat(warden)  ⇐ threat(F) :- boss(F), not slew(F)
  ├─ boss(warden)  (seed fact)
  └─ not slew(warden)  (absent)
explain threat(hound): does not hold
  threat(F) :- boss(F), not slew(F)
  ├─ ✗ boss(hound)  (absent)
  └─ · not slew(hound)  (not reached)
```

After an `engine:` step asserts `slew(warden)`, the same atom reads:

```
explain threat(warden): does not hold
  threat(F) :- boss(F), not slew(F)
  ├─ boss(warden)  (seed fact)
  └─ ✗ not slew(warden)  (but it holds: asserted)
```

The explanation is printed before the `── expect:` block, and `--json` carries it as `explain`. An atom that is not ground (`knows(X)`, `slew(_)`) is a usage error (exit 2). `--explain` evaluates the rules over the final facts and state even under `--no-derive`.

### Usage errors

The script is rejected before anything plays — a **usage error, exit 2**, naming the step — when:

- it is unreadable or malformed YAML, has an unknown top-level key, or `steps` is missing or empty;
- a step names no action or more than one, has an unknown key, carries `target` / `pick` / `choose` / `expect` on a step that is not an `occasion`, or has a `repeat` that is not a whole number ≥ 1;
- an occasion step names an occasion no resolved plugin declares (when some plugin declares occasions) or, in a shape-only project, one that neither a beat answers nor an `<objective on>` judges; raises a targeted occasion without `target`, or an untargeted one with it; names a target outside the occasion's domain; carries `pick` on a `select: first` or `select: sequence` occasion; lacks it on a `select: all` one; or picks a beat that does not answer that occasion;
- an `event:` names no declared world event, or a quest lifecycle event;
- an `engine:` or `newRun` write names an undeclared or `scene.*` / `quest.*` path, a value that does not fit the declared type, `{ add: … }` on a path that is not a `number`, or a fact that is not ground, names an undeclared or derived relation, has the wrong arity, or names a non-member of a closed domain; or it writes nothing;
- a `state:` / `facts:` seed fails the same checks, or a save seed names an unknown id or quest status;
- an `expect:` has an unknown key or a malformed value.

The message names the key and what fits: `` step 1: `engine.state.quest.climb.state`: quest state is written by the quest lifecycle, not the engine — seed a save's quest status with top-level `quests:` ``.

### What each step does

An occasion step:

1. **Candidates** — every beat in the project's beat list with `on` equal to the step's occasion whose `target` is absent or equals the step's `target`.
2. **Verdicts** — a candidate is eligible when its `once` is not spent (a scene or bundle beat: `run` — not presented since the last `newRun`; `user` — never presented in this play or the save; `false` — never spent. An entry: `run` — `entry.<id>.read` not set; `user` — `entry.<id>.everRead` not set; no `once` — never spent), its `after:` holds (scene beats; evaluated against the **live** `visited` set of presented scenes and the `completed` / `active` sets of real quest states), and its `when` holds (evaluated by the reference runner's CEL evaluator over live state and facts, with the Datalog rules applied). A `when` that evaluates to unknown — `validAt(…)`, `now()`, or a derived atom under `--no-derive` — halts the walk **incomplete (exit 3)** naming the beat, unless a definitely-eligible beat outranks it on a `select: first` occasion, where it cannot change the winner. Verdicts are decided once, here: nothing a presentation does in (5) changes which of this step's beats play.
3. **Order** — eligible beats by priority descending, then project order.
4. **Select** — the occasion's `select` comes from the resolved plugins' `occasions` export (an undeclared occasion is `first`). `first`: the first eligible beat that is not `also` wins, and every eligible `also` beat follows it (or plays alone when nothing else is eligible); none eligible, and the occasion passes with no story. `all`: the step's `pick` is presented; a pick that is not eligible at that moment is an **error (exit 1)**; `pick: none` presents nothing. `sequence`: every eligible beat, in order.
5. **Present** — a scene beat runs through the reference runner (`lute run`'s evaluator): `scene.*` resets to the scene's own defaults, `run.*` / `user.*` / `app.*` / `quest.*` state and facts carry over, and the script's `choose:` — with the step's own `choose:` over it — decides branches and hubs; an unscripted decision halts **incomplete (exit 3)**. A bundle beat runs the same way, from its `beat` record in the lore artifact. An entry beat is presented by the lore-entry rules: its effects apply on the first read only, then `entry.<id>.read` and `entry.<id>.everRead` become true. A scene's `::end` ends the whole playthrough, complete — after the step settles: the quest advance (6) and the occasion's objective judging (7) still run for that step, then the walk stops. A scene's `::accept{quest="<id>"}` prints `quest <id> accepted`; the quest activates at the advance right after the presentation. A scene or bundle beat counts as visited — for `after:` and for `visited('<id>')` in any condition — once its presentation finished. When a step presents several beats (`sequence`, `also`), (5) and (6) run for each in turn.
6. **Quests** — after every presentation, every quest lifecycle advances exactly as `lute run` advances a quest artifact: activation (`start`, or — for a quest with no `start` — an accept from a presented scene's `::accept`; a start-less quest never activates on its own), objective completion (monotone; objective bodies play once), then each open objective's `by` deadline (a first true fails it, `failed (by)`), `fail` before completion, `<on>` handlers, and `<reward>` grants — adding a scalar amount to the reward kind's `credits:` path. So a later `when` over `quest.*`, or an `after: completed(…)` / `active(…)`, sees real progress.
7. **Occasion-judged objectives** — then the step's occasion judges the `<objective on="<occasion>">` objectives of every **active** quest (dsl 0.21.0 §7a.2) — an objective that also carries `target=` only when the step's `target` equals it (dsl 0.23.0) — and the lifecycles settle again, so a quest can complete (or fail) at exactly that step. An `on` objective is judged at no other time. An occasion that only objectives judge is a legal step even in a shape-only project; with no beat to present it prints `(no candidates)` and passes, then judges.

The lifecycles also settle once before step 1 (with the save's quest statuses already in place), after every `engine:` step, and after every `newRun` (reset, then seed, then settle). An `event:` step raises its event once for every quest document, then settles.

### Exit codes

| Code | Meaning |
|---|---|
| `0` | Complete — every step played, or a scene's `::end` ended the playthrough — and every expectation held. |
| `1` | Error — the project fails to compile, a vocabulary conflict, a `pick` that is not eligible, a `choose:` decision the menu does not offer (an ineligible choice, or a spent `once` hub option), or a missed expectation. |
| `2` | Usage or I/O — a bad script (see [Usage errors](#usage-errors)), an unknown occasion or world event, a missing or out-of-domain `target`, an invalid seed or `engine:` write, a non-ground `--explain` atom, an unreadable project, a malformed artifact. |
| `3` | Incomplete — an unscripted choice or hub, a branch `choose:` list that ran out, a `when` or quest objective that evaluates to unknown, or an unresolved `now()` / `validAt()` / plugin `bridgeResult`. |

## The transcript

The human transcript names every step, lists its candidates with their verdicts, and marks the winner before the presented beat plays. The tour script from [The play script](#the-play-script) prints:

```
── start ──────────────
  quest climb -> active
  quest veteran -> active
  quest notices -> active
── step 1 · hubVisit ──────────────
  ✓ hub.idle [scene, priority 0]
  ✗ hub.victory [scene, priority 10] — when: false
  → hub.idle
@maud: Quiet night.
── step 2 · talk → npc.maud ──────────────
  ✓ maud.talk [scene, priority 0]
  → maud.talk
@maud: Up again?
── step 3 · board (select: all, pick: none) ──────────────
  ✓ memo [entry, priority 0]
  ✓ old [entry, priority 0]
  ✗ notice [entry, priority 0] — once: user — already read
  → (pick: none — the list closes; nothing presented)
  notices.looked done
  quest notices -> complete
── step 4 · event storm ──────────────
@narrator: Thunder rolls over the stair.
── step 5 (the warden falls on floor six) · engine ──────────────
  set run.floor = 6
  set user.runs = 3
  assert slew(warden)
  climb.high done
  quest climb -> complete
  veteran.three done
  quest veteran -> complete
── step 6 · new run ──────────────
  run.* state, run-tier facts and once: run reset
  quest climb -> unset (tier: run)
  set run.floor = 1
  quest climb -> active
── step 7 [1/2] · hubVisit ──────────────
  ✓ hub.idle [scene, priority 0]
  ✗ hub.victory [scene, priority 10] — when: false
  → hub.idle
@maud: Quiet night.
── step 7 [2/2] · hubVisit ──────────────
  ✓ hub.idle [scene, priority 0]
  ✗ hub.victory [scene, priority 10] — when: false
  → hub.idle
@maud: Quiet night.
── end: complete (8 steps) ──────────────
── expect: every expectation held ──────────────
```

- Every header is its text followed by a fixed `──────────────` rule. `── start` carries the quest transitions made before step 1. Each step opens with `── step <n>`, then its `(label)`, then `[k/n]` for a repetition, then what it does: `· <occasion>` — plus `→ <target>` for a targeted step, `(select: all, pick: <id>)` on a `select: all` occasion, and `(select: sequence)` on a `select: sequence` one — `· engine`, `· new run`, or `· event <name>`.
- Candidates are listed eligible first (`✓`), in selection order, then the rest, in selection order, each with its kind — `scene`, `entry`, or `beat` for a bundle beat — and priority, then `also` for an `also` beat and `read` for an entry already read in this run (dsl 0.23.0). An ineligible candidate (`✗`) carries its reason: `once: run — already presented this run`, `once: user — already presented`, `once: run — already read this run`, `once: user — already read`, `after: prerequisite not satisfied`, or `when: false`; a candidate whose `when` evaluated to unknown is marked `?` with `when: unknown (<detail>)`. A step whose occasion no beat answers lists `(no candidates)`.
- `→ <id>` names the winner — one `→` line per beat on a `select: sequence` step — and `+ <id> (also)` each `also` beat that follows it. With no winner the line reads `→ (no eligible beat — the occasion passes)`, `→ (no eligible main beat)` when only `also` beats play, or `→ (pick: none — the list closes; nothing presented)`.
- The presented beat's own transcript follows, written the way the source reads: content lines as `@speaker: text`, keeping their delivery (`@wren{mono}: …`, `@maud{as="Barkeep"}: …`); a line whose `when=` is false as `skip @maud "You again." — when: false`; authored staging directives as themselves, while compiler-injected staging (preloads, pose resets, the `::bg` auto-hide) is left out; decisions as `▷ choice <id>: … ← chosen: <id>` (or `▷ hub <id>: …`), where the menu marks the chosen option `[table]`, an option whose guard is false `piano✗`, and a `once` option already taken `table(spent)`; state writes as `set <path> = <value>`, a scene's `::accept` as `quest <id> accepted` (JSON: an `{"kind": "accept", "quest": "<id>"}` record in `presented.commands`), and an entry's `entry <id> (first read)` — or `entry <id> (re-read: effects skipped)`, with each skipped effect marked `(skipped: re-read)`. A bundle beat's presentation opens with a bare `beat` line, its `beat` record. Several presented beats follow one another in presentation order. Quest transitions come last — those the presentations caused, then those the step's occasion judged: `<quest>.<objective> done`, `<quest>.<objective> failed (by)` for a missed deadline, `quest <id> -> <state>`, and reward grants — `grant <quest> <KIND> <amount>`, followed by `(credits <path> = <value>)` when the reward kind credits state. In JSON both land in the step's `quests`.
- An `engine:` step lists its writes: `set <path> = <value>`, `assert <atom>`, `retract <atom>` — `retract <atom> (did not hold)` when it was not a fact.
- A `newRun` step prints `run.* state, run-tier facts and once: run reset`, then `quest <id> -> unset (tier: run)` for each run-tier quest, then its seed's writes.
- An `event:` step prints the handlers that ran; the lifecycle's transitions follow every step kind.
- In `--json`, a line record in `presented.commands` carries `role`, `lineId`, `voiceKey`, `as`, and `emotion` where they apply, and a choice or hub record lists the options not offered under `ineligible` and the taken `once` options under `spent`.
- The walk ends with `── end: complete (<n> steps)` — every repetition counted — or `── halted: <message>` when it stops early. Then the `--explain` trees, then the `── expect:` block.

A shape-only project (no plugins) with a hub scene that offers a side job, and a quest whose `calm` objective is judged at `runEnd`:

```lute unverified="one file of a multi-file project: the scene answering hubVisit and a world schema declaring run.pressure sit beside it"
<quest id="holdLine" title="Hold the line" start="true">
  <objective id="sawShed" title="See the shed" done="visited('haven.shed')"/>
  <objective id="calm" title="Keep it calm" on="runEnd" done="run.pressure < 2"/>
</quest>

<quest id="sideJob" title="Side job">
  <objective id="mind" title="Mind the shed" done="run.pressure < 5"/>
</quest>
```

With `steps: [{occasion: hubVisit}, {occasion: runEnd}]` and `choose: { offer: take }` — the choice whose body is `::accept{quest="sideJob"}` — the playthrough accepts the side job during the presentation, activates it right after, and completes `holdLine` only when `runEnd` is raised:

```
── start ──────────────
  quest holdLine -> active
── step 1 · hubVisit ──────────────
  ✓ haven.shed [scene, priority 0]
  → haven.shed
@vesna: Somebody has to mind the shed.
▷ choice offer: [take] leave        ← chosen: take
  quest sideJob accepted
@vesna: Good. It's yours.
  quest sideJob -> active
  holdLine.sawShed done
  sideJob.mind done
  quest sideJob -> complete
── step 2 · runEnd ──────────────
  (no candidates)
  → (no eligible beat — the occasion passes)
  holdLine.calm done
  quest holdLine -> complete
── end: complete (2 steps) ──────────────
```

`--json` emits the same walk as one object:

```ts
type PlayTranscript = {
  exit: "complete" | "incomplete" | "error";
  start: { quests: QuestGroup[] };         // transitions made before step 1
  steps: Step[];                           // one record per repetition
  endReason?: string;
  error?: { message: string };
  expect?: { misses: ExpectMiss[] };       // when the script carries an `expect:`
  explain?: Explanation[];                 // one per `--explain` atom
};

type Step = (OccasionStep | EngineStep | NewRunStep | EventStep) & {
  step: number;                            // the script step (shared by its repetitions)
  label?: string;
  iteration?: number;                      // 1-based repetition, when `repeat` > 1
  repeat?: number;
  quests: QuestGroup[];                    // transitions this step caused
};

type OccasionStep = {
  occasion: string;
  target?: string;
  select: "first" | "all" | "sequence";
  pick?: string;                           // a beat id, or "none"
  candidates: {
    id: string;
    kind: "scene" | "entry" | "beat";      // "beat": a bundle beat
    document: string;
    priority: number;
    eligible: boolean | null;              // null: the `when` evaluated to unknown
    reason?: string;                       // e.g. "when: false", "when: unknown (<detail>)"
    also?: true;                           // an `also` beat
    read?: true;                           // an entry already read in this run
  }[];
  winner: string | null;
  presented?: Presentation;                // the winner, else the first `also` beat; a sequence's first beat
  then?: Presentation[];                   // the beats presented after it, in order
};

type Presentation = {
  id: string;
  kind: "scene" | "entry" | "beat";
  document: string;
  also?: true;
  commands: RunnerRecord[];                // the records `lute run --json` emits
  stateDelta: Record<string, unknown>;     // path -> value
};

type EngineStep = { engine: WriteRecord[] };
type NewRunStep = { newRun: true; seed: WriteRecord[] };
type EventStep = { event: string };

type WriteRecord =
  | { kind: "set"; path: string; value: unknown }
  | { kind: "assert"; fact: string }
  | { kind: "retract"; pattern: string; held: boolean };

type QuestGroup = {
  document: string;                        // the quest document
  commands: RunnerRecord[];                // its `objective` (`failed: true` on a missed deadline) / `quest` /
                                           // `grant` (`credited: { path, value }`) / handler records
};

type ExpectMiss = {
  step: number | null;                     // null: the top-level `expect:`
  label: string | null;
  occasion: string | null;                 // "talk npc.tomas"
  repetition: number | null;
  key: string;                             // "winner", "quests lampOut", "state run.day", …
  expected: string;
  actual: string;
};

type Explanation =
  | { atom: string; holds: true; proof: Proof }
  | { atom: string; holds: false; derived: boolean; attempts: Attempt[] };
type Proof =
  | { fact: string; support: "seed fact" | "asserted" }
  | { fact: string; support: "derived"; rule: string; premises: Premise[] };
type Attempt = { rule: string; premises: Premise[] };
type Premise =
  | { status: "holds"; proof: Proof }
  | { status: "missing"; atom: string; attempts: Attempt[] }
  | { status: "absent"; negated: string }
  | { status: "present"; proof: Proof }
  | { status: "test"; test: string; holds: boolean | null }
  | { status: "unreached"; premise: string };
```

## Worked example

`lute init --template beats` scaffolds a small project that already passes `check-project`, `test`, and `play`: the first day in a town square, a lamp that has gone dark, and the two people who know why.

```console
$ lute init --template beats town
```

```
town/
├── lute.project.yaml
├── world.schema.yaml
├── vocabulary.schema.yaml
├── plugins/game.occasions/
│   ├── plugin.yaml
│   └── occasions/game.yaml
├── scenes/
│   ├── hub/welcome.lute
│   ├── hub/morning.lute
│   ├── hub/day-end.lute
│   ├── talk/mara-first.lute
│   └── talk/mara-idle.lute
├── quests/lamp.lute
├── lore/tomas.lute
├── plays/first-day.play.yaml
├── tests/
│   ├── mara-first.test.yaml
│   └── lamp-quest.test.yaml
└── README.md
```

The manifest activates one plugin whose only export is the engine's occasions, and its `defaults:` give every document its language version and both schema imports. The occasions, `plugins/game.occasions/occasions/game.yaml`:

```yaml
occasions:
  hubVisit: { select: first, description: The player arrives at the hub }
  talk:     { select: first, target: { prefix: npc, entity: npc }, description: The player talks to someone (npc.<name>) }
  dayEnd:   { select: first, description: "The engine closed the day; run.day is already advanced" }
```

The shared state, `world.schema.yaml`. The day is the engine's: content reads `run.day`, but only the engine — and so only an `engine:` step — writes it:

```yaml
state:
  run.day:        { type: number, default: 1, owner: engine }
  user.bond.mara: { type: number, default: 0 }

entities:
  npc:  { members: [mara, tomas] }
  item: { members: [lamp] }

relations:
  knows: { args: [item], tier: run }

defs:
  firstDay: "run.day == 1"
  trusted:  "user.bond.mara >= 1"
```

The beats:

| Beat | Document | Answers | Conditions | `once` |
|---|---|---|---|---|
| `hub.welcome` | `scenes/hub/welcome.lute` | `hubVisit` | priority 10 | `user` |
| `hub.morning` | `scenes/hub/morning.lute` | `hubVisit` | `when: '!@firstDay'` — prints `Day {{run.day}}.` | `false` |
| `hub.dayEnd` | `scenes/hub/day-end.lute` | `dayEnd` | — | `false` |
| `mara.first` | `scenes/talk/mara-first.lute` | `talk` → `npc.mara` | priority 10 | `user` |
| `mara.idle` | `scenes/talk/mara-idle.lute` | `talk` → `npc.mara` | — | `false` |
| `tomasOil` (entry) | `lore/tomas.lute` | `talk` → `npc.tomas` | priority 10, `when="quest.lampOut.state == 'active'"`; asserts `knows(lamp)` | — |
| `tomasBusy` (entry) | `lore/tomas.lute` | `talk` → `npc.tomas` | — | — |

`mara.first` asks the question the day turns on:

```lute
<branch id="maraAsk" prompt="What do you say?">
  <choice id="lamp" label="Offer to find out why">
    @mara{emotion="delighted"}: Would you? Tomas keeps the oil. Ask him.
    ::set{ user.bond.mara += 1 }
    ::accept{quest="lampOut"}
  </choice>
  <choice id="leave" label="Say nothing">
    @mara: Suit yourself.
  </choice>
</branch>
```

The quest it accepts, `quests/lamp.lute`, finishes only when the engine closes the day:

```lute
<quest id="lampOut" title="The lamp by the door">
  <objective id="ask" title="Ask Tomas about the oil" done="holds(knows(lamp))"/>
  <objective id="wait" title="Wait for the day to end" on="dayEnd" done="run.day >= 2"/>
  <on event="questComplete">
    @narrator: By morning the lamp by the door is burning again.
  </on>
</quest>
```

### The first day

The scaffold's play script, `plays/first-day.play.yaml`, plays the day and asserts what it should do:

```yaml
choose:
  maraAsk: lamp
steps:
  - occasion: hubVisit
    expect: { winner: hub.welcome }
  - occasion: talk
    target: npc.mara
    expect: { winner: mara.first }
  - occasion: talk
    target: npc.tomas
    expect: { winner: tomasOil, offered: [tomasOil, tomasBusy] }
  - label: the engine closes the day
    engine:
      state: { run.day: { add: 1 } }
  - occasion: dayEnd
  - occasion: hubVisit
    expect: { winner: hub.morning, notOffered: [hub.welcome] }
expect:
  exit: complete
  quests: { lampOut: complete }
  state: { run.day: 2, user.bond.mara: 1 }
  facts: [knows(lamp)]
```

From the project directory:

```console
$ lute play . --script plays/first-day.play.yaml
```

```
── step 1 · hubVisit ──────────────
  ✓ hub.welcome [scene, priority 10]
  ✗ hub.morning [scene, priority 0] — when: false
  → hub.welcome
::background{location="hub" time="day" wait=true}
@narrator: The lamps along the square are lit — all but the one by the door.
── step 2 · talk → npc.mara ──────────────
  ✓ mara.first [scene, priority 10]
  ✓ mara.idle [scene, priority 0]
  → mara.first
@mara{emotion="content"}: You're new. The lamp by the door has been dark for a week.
▷ choice maraAsk "What do you say?": [lamp] leave        ← chosen: lamp
@mara{emotion="delighted"}: Would you? Tomas keeps the oil. Ask him.
  set user.bond.mara = 1
  quest lampOut accepted
  quest lampOut -> active
── step 3 · talk → npc.tomas ──────────────
  ✓ tomasOil [entry, priority 10]
  ✓ tomasBusy [entry, priority 0]
  → tomasOil
  entry tomasOil (first read)
@tomas: Oil? Top shelf. Tell Mara it's the wick, not the oil.
  assert knows(lamp)
  lampOut.ask done
── step 4 (the engine closes the day) · engine ──────────────
  set run.day = 2
── step 5 · dayEnd ──────────────
  ✓ hub.dayEnd [scene, priority 0]
  → hub.dayEnd
@narrator: One by one, the lamps go out.
  lampOut.wait done
  quest lampOut -> complete
@narrator: By morning the lamp by the door is burning again.
── step 6 · hubVisit ──────────────
  ✓ hub.morning [scene, priority 0]
  ✗ hub.welcome [scene, priority 10] — once: user — already presented
  → hub.morning
@narrator: Day 2. The square is already awake.
── end: complete (6 steps) ──────────────
── expect: every expectation held ──────────────
```

Reading it step by step:

- **Start** — `lampOut` has no `start`, so nothing activates before the first step and there is no `── start` block.
- **Step 1** — it is day 1, so `hub.morning`'s `@firstDay` guard keeps it out and the welcome plays.
- **Step 2** — `talk` is raised for `npc.mara`, a member of the `npc` kind; both of Mara's scenes are candidates and the first meeting outranks the fallback. Choosing `lamp` raises the bond and accepts the quest, which activates right after the presentation.
- **Step 3** — with the quest active, Tomas's oil entry is eligible and outranks his bark. Its first read asserts `knows(lamp)`, which completes the first objective.
- **Step 4** — the engine closes the day. An `engine:` step writes `run.day`, which content may not (`owner: engine`); nothing is presented and no occasion is raised.
- **Step 5** — `dayEnd` presents the night scene, then judges `lampOut.wait` (`on="dayEnd"`), and the quest completes; its `questComplete` handler plays.
- **Step 6** — `hub.welcome` is spent for good (`once: user`), and `run.day` is 2, so the morning plays.

### One step decided the other way

A step's own `choose:` changes one presentation and nothing else. Here the player says nothing to Mara, while the expectations still describe the first day — so they miss:

```yaml
choose:
  maraAsk: lamp
steps:
  - occasion: hubVisit
  - occasion: talk
    target: npc.mara
    choose: { maraAsk: leave }
  - occasion: talk
    target: npc.tomas
    label: ask about the oil
    expect: { winner: tomasOil }
expect:
  quests: { lampOut: active }
  state: { user.bond.mara: 1 }
```

```
── step 1 · hubVisit ──────────────
  ✓ hub.welcome [scene, priority 10]
  ✗ hub.morning [scene, priority 0] — when: false
  → hub.welcome
::background{location="hub" time="day" wait=true}
@narrator: The lamps along the square are lit — all but the one by the door.
── step 2 · talk → npc.mara ──────────────
  ✓ mara.first [scene, priority 10]
  ✓ mara.idle [scene, priority 0]
  → mara.first
@mara{emotion="content"}: You're new. The lamp by the door has been dark for a week.
▷ choice maraAsk "What do you say?": lamp [leave]        ← chosen: leave
@mara: Suit yourself.
── step 3 (ask about the oil) · talk → npc.tomas ──────────────
  ✓ tomasBusy [entry, priority 0]
  ✗ tomasOil [entry, priority 10] — when: false
  → tomasBusy
  entry tomasBusy (first read)
@tomas: Busy.
── end: complete (3 steps) ──────────────
── expect: 3 missed ──────────────
  ✗ step 3 (ask about the oil) at talk npc.tomas: expect winner: expected tomasOil, actual tomasBusy
  ✗ end of play: expect quests lampOut: expected active, actual unset
  ✗ end of play: expect state user.bond.mara: expected 1, actual 0
```

The walk itself completed; the misses make `lute play` exit 1.

### Coming back

A later session does not replay the first day: the script starts from the save a player who finished it would have, and the engine starts run two on day 3:

```yaml
state: { user.bond.mara: 1 }
presented: { user: [hub.welcome, mara.first] }
quests: { lampOut: complete }
steps:
  - label: the engine starts run two on day 3
    newRun: { state: { run.day: 3 } }
  - occasion: hubVisit
    expect: { winner: hub.morning, notOffered: [hub.welcome] }
  - occasion: talk
    target: npc.mara
    expect: { winner: mara.idle }
  - occasion: talk
    target: npc.tomas
    expect: { winner: tomasBusy, notOffered: [tomasOil] }
  - label: a quiet day passes
    engine:
      state: { run.day: { add: 1 } }
    repeat: 2
  - occasion: dayEnd
    expect: { winner: hub.dayEnd }
expect:
  exit: complete
  state: { run.day: 5 }
  transcriptContains: ["Any luck with the lamp?", "Day 3."]
  transcriptLacks: ["You're new."]
```

```
── step 1 (the engine starts run two on day 3) · new run ──────────────
  run.* state, run-tier facts and once: run reset
  set run.day = 3
── step 2 · hubVisit ──────────────
  ✓ hub.morning [scene, priority 0]
  ✗ hub.welcome [scene, priority 10] — once: user — already presented
  → hub.morning
@narrator: Day 3. The square is already awake.
── step 3 · talk → npc.mara ──────────────
  ✓ mara.idle [scene, priority 0]
  ✗ mara.first [scene, priority 10] — once: user — already presented
  → mara.idle
@mara{emotion="shy"}: Any luck with the lamp?
  skip @mara "Mm." — when: false
── step 4 · talk → npc.tomas ──────────────
  ✓ tomasBusy [entry, priority 0]
  ✗ tomasOil [entry, priority 10] — when: false
  → tomasBusy
  entry tomasBusy (first read)
@tomas: Busy.
── step 5 (a quiet day passes) [1/2] · engine ──────────────
  set run.day = 4
── step 5 (a quiet day passes) [2/2] · engine ──────────────
  set run.day = 5
── step 6 · dayEnd ──────────────
  ✓ hub.dayEnd [scene, priority 0]
  → hub.dayEnd
@narrator: One by one, the lamps go out.
── end: complete (7 steps) ──────────────
── expect: every expectation held ──────────────
```

- **Save** — `presented.user` spends both `once: user` scenes, `quests:` resumes `lampOut` as complete (so the oil entry's `when` is false), and the `user.bond.mara` seed makes Mara's `@trusted` line play.
- **Step 1** — the long-form `newRun` seeds the new run: `run.day` resets to its default, then the seed sets it to 3.
- **Step 5** — one `engine:` step, repeated: each repetition is its own record, and the run ends with `run.day` at 5.

### In the test suite

Every play that carries an `expect:` runs under `lute test`, beside the scenario tests. With the two plays above saved as `plays/say-nothing.play.yaml` and `plays/returning.play.yaml`:

```console
$ lute test . --project .
```

```
PASS  ./tests/lamp-quest.test.yaml  (./tests/../quests/lamp.lute)
PASS  ./tests/mara-first.test.yaml  (./tests/../scenes/talk/mara-first.lute)
PASS  ./plays/first-day.play.yaml  (play of .)
PASS  ./plays/returning.play.yaml  (play of .)
FAIL  ./plays/say-nothing.play.yaml  (play of .)
      step 3 (ask about the oil) at talk npc.tomas: expect winner: expected tomasOil, actual tomasBusy
      end of play: expect quests lampOut: expected active, actual unset
      end of play: expect state user.bond.mara: expected 1, actual 0

4 passed, 1 failed
```

With only the scaffold's own play, `lute test . --project . --coverage` counts it among the tests:

```
PASS  ./tests/lamp-quest.test.yaml  (./tests/../quests/lamp.lute)
PASS  ./tests/mara-first.test.yaml  (./tests/../scenes/talk/mara-first.lute)
PASS  ./plays/first-day.play.yaml  (play of .)

3 passed, 0 failed

coverage over 2 traced path(s) and 1 play(s):
  branch/hub maraAsk (./tests/../scenes/talk/mara-first.lute:maraAsk): 1/2 chosen [lamp]; never chosen [leave]
  1 untested document(s) under . — no *.test.yaml names them and no play presents them:
    ./scenes/talk/mara-idle.lute
```

The play presented the welcome, the morning, the night scene, Mara's first meeting, and Tomas's entries, so only `mara-idle.lute` is left — the scene `plays/returning.play.yaml` covers.
