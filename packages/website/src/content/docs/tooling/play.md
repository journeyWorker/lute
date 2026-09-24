---
title: Playing a story
description: "Occasions and beats (dsl 0.21.0) — how a project says which piece of story answers which engine moment — and `lute play`, the reference player that walks a scripted sequence of raised occasions through a whole project and prints every candidate, its verdict, and the winner."
---

A narrative game picks its next piece of story at moments of its own: a hub visit, entering a room, talking to an NPC, a new day, the start of a run. Lute calls those moments **occasions** and the pieces of story that answer them **beats** (dsl 0.21.0). The engine raises occasions; Lute defines which beats are eligible and which one wins. `lute play` is the reference player for that contract: give it a script of raised occasions and it walks the whole project, printing every candidate beat, its verdict, and the winner, and playing the winner through the same reference runner as `lute run`.

The normative text is the [0.21.0 proposal](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.21.0.md); the engine-side contract (the IR fields and the selection algorithm an engine implements) is [`docs/runtime/beats-and-occasions.md`](https://github.com/journeyWorker/lute/blob/main/docs/runtime/beats-and-occasions.md). Before 0.21.0, `lute play` walked a tick-clock schedule file; that layer, its clock/lane/placement model, and its flags are removed. Time is now one input to a beat's condition, not the frame.

## Occasions

An occasion is **engine vocabulary**: a named moment the engine raises, optionally *for* a target (`talk` → `npc.achilles`). A plugin declares its occasions through an `occasions` export — list it in `plugin.yaml` (`exports: { occasions: occasions/ }`) and put the map in `occasions/*.yaml`:

```yaml
occasions:
  hubVisit:  { select: first }
  talk:      { select: first, target: true }
  roomEnter: { select: first, target: true }
  inbox:     { select: all, description: Letters waiting at the fountain }
```

- `select: first` (the default) — the engine presents the single winning beat.
- `select: all` — the engine offers every eligible beat, in selection order, and the player picks one (a message inbox, an area map, quest givers "in any order").
- `target: true` — the occasion is raised **for** something, and a beat may restrict itself to one target. Default `false`.
- `description` — optional prose for tooling.

When no resolved plugin declares occasions, occasion names are **shape-only**: any identifier is accepted, so you can write beats before the engine's plugin exists. Once any plugin declares them, a beat naming an undeclared occasion is `E-OCCASION-UNKNOWN`, and a `target` on an occasion declared without `target: true` is `E-BEAT-ATTR`. The export folds into the capability snapshot as a guarded section, so a project whose plugins declare no occasions keeps its `capabilityVersion`.

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
| `target` | Optional. The scene is a candidate only when the occasion is raised for this target (a dotted id, the `<entry target>` shape). |
| `when` | Optional CEL condition over `run` / `user` / `app` state, `quest.*`, `entry.<id>.read`, and fact queries. The scene's own `scene.*` state does not exist yet and is rejected. |
| `priority` | Optional integer, default `0`. Higher wins. |
| `once` | `run` (the default — once per run), `user` (once ever), or `false` (repeatable). |

`after:` keeps its meaning — the structural prerequisite over `visited` / `completed` / `active` that [connectivity](/connectivity/scene-graph/) analyzes — and a beat is eligible only when both `after:` and `when` hold. `when`, `target`, `priority`, or `once` without `on` is `E-BEAT-ATTR`: a scene that answers no occasion is reached by explicit flow, as before.

### Entry beats

A [lore entry](/language/lore-entries/) answers an occasion with `on=` and `priority=` beside its existing `target` and `when`:

```lute
<entry id="achillesBark" on="talk" target="npc.achilles" category="bark" priority="10" when="user.runs >= 3">
  @achilles: Back again, lad.
</entry>
```

Entries have no `once`: re-presentation is their nature. An entry that should be heard once per run guards on its own `entry.<id>.read`; that flag is run-tier, so a `newRun` step resets it. For once ever, guard on a `user.*` flag the entry sets.

## Selection

When the engine raises occasion `O`, optionally for target `T`:

1. **Candidates** are the beats with `on: O` whose `target` is absent or equal to `T`. An occasion raised without a target has only untargeted candidates.
2. A candidate is **eligible** when its `after:` holds (scene beats), its `when` holds, and its `once` is not spent — `run`: not yet presented this run; `user`: never presented; `false`: never spent. Entry beats are never spent.
3. Eligible beats are **ordered by priority, descending, then project order**: document path, then declaration order within the document — the order of `beats` in `project.index.json`. Scene and entry beats on the same occasion compete in one list.
4. `select: first` presents the first eligible beat; `select: all` offers the ordered list and presents the one the player picks.
5. **No eligible beat** — the occasion passes with no story, and the engine's default behavior for that moment applies.

Selection is deterministic: the same state, facts, and presentation history pick the same beat in every engine. Weighted randomness or cooldowns are engine policy layered on top; the reference tooling implements exactly this order.

## `lute play`

```console
$ lute play <PROJECT_DIR> --script <FILE> [--json]
```

- `<PROJECT_DIR>` — the project root (`lute.project.yaml` and its plugins). The project is compiled whole, in memory, with the same gate and declaration union `compile --all` uses (scene, quest, and lore documents).
- `--script <FILE>` — required: the play script, a `*.play.yaml` file.
- `--json` — the same transcript as one JSON object on stdout.

There is nothing else to seed on the command line: state, facts, and decisions all live in the script, so a playthrough is one reviewable file.

### The play script

A play script has exactly four top-level keys — `state`, `facts`, `choose`, and `steps`:

```yaml
state: { user.runs: 9 }          # path -> scalar literal, over the declared defaults
facts: ["knows(achilles)"]       # ground facts, added to the project's seed facts
steps:                           # required, non-empty
  - occasion: hubVisit           # raise an occasion
  - occasion: talk
    target: npc.achilles         # only for an occasion declared `target: true`
  - occasion: inbox
    pick: megNote                # required for a `select: all` occasion, refused for `select: first`
  - newRun: true                 # start a new run
choose:                          # branch/hub id -> choice id (a list for a hub's visit sequence)
  gift: accept                   # applied to every presented scene
```

`state:`, `facts:`, and `choose:` use exactly the grammar of a [`lute trace --mock`](/tooling/tracing/) file. Each step is either `{occasion, target?, pick?}` or `{newRun: true}`.

- A `quest.<id>.state` seed (`state: { quest.lostCup.state: active }`) is that quest's lifecycle status from the start, as if the save already held it. The id must name a declared quest and the value must be `unset`, `active`, `complete`, or `failed`; anything else is a usage error (exit 2).
- A single `choose:` decision answers every presentation of its branch or hub. A list for a **hub** is one visit sequence, replayed at each presentation. A list of two or more for a **branch** is consumed one entry per presentation, in order, across the whole playthrough — so a scene that plays on three nights can decide differently each night. When the list runs out, the walk halts incomplete (exit 3) and says so.
- A decision the menu does not offer at that moment halts the walk with an error (exit 1): a choice whose guard is false, or a `once` hub option already taken (`E-TRACE-CHOICE`, as in `lute trace`).

The script is rejected before anything plays — a **usage error, exit 2** — when it is unreadable or malformed YAML; it has an unknown top-level key; `steps` is missing or empty; a step is not exactly one of the two shapes; a `quest.<id>.state` seed names an undeclared quest or a value outside the four states; a step raises a `target: true` occasion without a `target`; a step names an occasion no resolved plugin declares (when some plugin declares occasions) or, in a shape-only project, an occasion that neither a beat answers nor an `<objective on>` judges; a step carries `target` on an occasion not declared `target: true`; `pick` appears on a `select: first` occasion; a `select: all` step has no `pick`; or a `pick` names no beat that answers that occasion.

### What each step does

1. **Candidates** — every beat in the project's beat list with `on` equal to the step's occasion whose `target` is absent or equals the step's `target`.
2. **Verdicts** — a candidate is eligible when its `once` is not spent (scene beats only: `run` — not presented since the last `newRun`; `user` — never presented in this play; `false` — never spent), its `after:` holds (scene beats; evaluated against the **live** `visited` set of presented scenes and the `completed` / `active` sets of real quest states), and its `when` holds (evaluated by the reference runner's CEL evaluator over live state and facts, with the Datalog rules applied). A `when` that evaluates to unknown — `validAt(…)`, `now()` — halts the walk **incomplete (exit 3)** naming the beat, unless a definitely-eligible beat outranks it on a `select: first` occasion, where it cannot change the winner.
3. **Order** — eligible beats by priority descending, then project order.
4. **Select** — the occasion's `select` comes from the resolved plugins' `occasions` export (an undeclared occasion is `first`). `first`: the first eligible beat wins; none eligible, and the occasion passes with no story. `all`: the step's `pick` is presented; a pick that is not eligible at that moment is an **error (exit 1)**.
5. **Present** — a scene beat runs through the reference runner (`lute run`'s evaluator): `scene.*` resets to the scene's own defaults, `run.*` / `user.*` / `app.*` / `quest.*` state and facts carry over, and `choose:` decides branches and hubs; an unscripted decision halts **incomplete (exit 3)**. An entry beat is presented by the lore-entry rules: its effects apply on the first read only, then `entry.<id>.read` becomes true. A scene's `::end` ends the whole playthrough, complete — after the step settles: the quest advance (6) and the occasion's objective judging (7) still run for that step, then the walk stops. A scene's `::accept{quest="<id>"}` prints `quest <id> accepted`; the quest activates at the advance right after the presentation. A scene counts as visited — for `after:` and for `visited('<id>')` in any condition — once its presentation finished.
6. **Quests** — after every presentation, and once before step 1, every quest lifecycle advances exactly as `lute run` advances a quest artifact: activation (`start`, or — for a quest with no `start` — an accept from a presented scene's `::accept`; a start-less quest never activates on its own), objective completion (monotone; objective bodies play once), `fail` before completion, `<on>` handlers, and `<reward>` grants. So a later `when` over `quest.*`, or an `after: completed(…)` / `active(…)`, sees real progress.
7. **Occasion-judged objectives** — then the step's occasion judges the `<objective on="<occasion>">` objectives of every **active** quest (dsl 0.21.0 §7a.2), and the lifecycles settle again, so a quest can complete (or fail) at exactly that step. An `on` objective is judged at no other time. An occasion that only objectives judge is a legal step even in a shape-only project; with no beat to present it prints `(no candidates)` and passes, then judges.

A `newRun: true` step resets `run.*` state to its declared defaults, run-tier facts to the project's seed facts, the run-tier `entry.<id>.read` flags (so an entry's effects apply again on its first read in the new run), and `once: run` spending. `user.*` / `app.*` / `quest.*` state, user- and app-tier facts, the `visited` history, and `once: user` spending persist. `once: user` across separate `lute play` invocations is not modelled — put the runs in one script, separated by `newRun` steps.

### Exit codes

| Code | Meaning |
|---|---|
| `0` | Complete — every step played, or a scene's `::end` ended the playthrough. |
| `1` | Error — the project fails to compile, a vocabulary conflict, a `pick` that is not eligible, or a `choose:` decision the menu does not offer (an ineligible choice, or a spent `once` hub option). |
| `2` | Usage or I/O — a bad script, an unknown occasion, a `target: true` occasion raised without `target:`, an invalid `quest.<id>.state` seed, an unreadable project, a malformed artifact. |
| `3` | Incomplete — an unscripted choice or hub, a branch `choose:` list that ran out, a `when` or quest objective that evaluates to unknown, or an unresolved `now()` / `validAt()` / plugin `bridgeResult`. |

## The transcript

The human transcript names every step, lists its candidates with their verdicts, and marks the winner before the presented beat plays:

```
── start ──────────────
  quest oldSoldier -> active
── step 1 · hubVisit ──────────────
  ✓ hub.firstEver [scene, priority 20]
  ✓ hub.welcome [scene, priority 10]
  → hub.firstEver
@hypnos: Oh! You're back already? I mean — welcome home, I guess.
```

- Every header is its text followed by a fixed `──────────────` rule. `── start` carries the quest transitions made before step 1; each step opens with `── step <n> · <occasion>`, plus `→ <target>` for a targeted step and `(select: all, pick: <id>)` on a pick.
- Candidates are listed eligible first (`✓`), in selection order, then ineligible (`✗`), in selection order, each with its kind and priority. An ineligible candidate carries its reason: `once: run — already presented this run`, `once: user — already presented`, `after: prerequisite not satisfied`, or `when: false`. A step whose occasion no beat answers lists `(no candidates)`.
- `→ <id>` names the winner. With no winner the line reads `→ (no eligible beat — the occasion passes)`.
- The presented beat's own transcript follows, written the way the source reads: content lines as `@speaker: text`, keeping their delivery (`@wren{mono}: …`, `@maud{as="Barkeep"}: …`); a line whose `when=` is false as `skip @maud "You again." — when: false`; authored staging directives as themselves, while compiler-injected staging (preloads, pose resets, the `::bg` auto-hide) is left out; decisions as `▷ choice <id>: … ← chosen: <id>` (or `▷ hub <id>: …`), where the menu marks the chosen option `[table]`, an option whose guard is false `piano✗`, and a `once` option already taken `table(spent)`; state writes as `set <path> = <value>`, a scene's `::accept` as `quest <id> accepted` (JSON: an `{"kind": "accept", "quest": "<id>"}` record in `presented.commands`), and an entry's `entry <id> (first read)` — or `entry <id> (re-read: effects skipped)`, with each skipped effect marked `(skipped: re-read)`. Quest transitions come last — those the presentation caused, then those the step's occasion judged: `<quest>.<objective> done`, `quest <id> -> <state>`, and reward grants. In JSON both land in the step's `quests`.
- In `--json`, a line record in `presented.commands` carries `role`, `lineId`, `voiceKey`, `as`, and `emotion` where they apply, and a choice or hub record lists the options not offered under `ineligible` and the taken `once` options under `spent`.
- A `newRun` step prints `── step <n> · new run` and `run.* state, run-tier facts and once: run reset`.
- The walk ends with `── end: complete (<n> steps)`, or `── halted: <message>` when it stops early.

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
  steps: (OccasionStep | NewRunStep)[];
  endReason?: string;
  error?: { message: string };
};

type OccasionStep = {
  step: number;
  occasion: string;
  target?: string;
  select: "first" | "all";
  pick?: string;
  candidates: {
    id: string;
    kind: "scene" | "entry";
    document: string;
    priority: number;
    eligible: boolean | null;              // null: the `when` evaluated to unknown
    reason?: string;                       // e.g. "when: false", "when: unknown (<detail>)"
  }[];
  winner: string | null;
  presented?: {
    id: string;
    kind: "scene" | "entry";
    document: string;
    commands: RunnerRecord[];              // the records `lute run --json` emits
    stateDelta: Record<string, unknown>;   // path -> value
  };
  quests: QuestGroup[];                    // transitions after this presentation
};

type NewRunStep = { step: number; newRun: true };

type QuestGroup = {
  document: string;                        // the quest document
  commands: RunnerRecord[];                // its `objective` / `quest` / `grant` records
};
```

## Worked example

A small hub game in the shape of *Hades*: a lounge the player returns to between runs, an old soldier with a gift for a tenth escape attempt, and an inbox of letters. The project:

```
house/
├── lute.project.yaml
├── house.schema.yaml
├── plugins/house.occasions/
│   ├── plugin.yaml
│   └── occasions/house.yaml
├── scenes/
│   ├── hub-first-ever.lute
│   ├── hub-welcome.lute
│   └── achilles-gift.lute
├── quests/old-soldier.lute
├── lore/letters.lute
└── plays/tenth-run.play.yaml
```

The project activates one plugin whose only export is the engine's occasions — `lute.project.yaml`, `plugins/house.occasions/plugin.yaml`, and `plugins/house.occasions/occasions/house.yaml`:

```yaml
pluginsDir: plugins/
defaultProfile: house
profiles:
  house:
    plugins: { house.occasions: true }
```

```yaml
id: house.occasions
version: 0.1.0
kind: capability
depends: [ { id: lute.core, range: "^0.0.1" } ]
exports:
  occasions: occasions/
```

```yaml
occasions:
  hubVisit: { select: first }
  talk:     { select: first, target: true }
  inbox:    { select: all }
```

The shared state, `house.schema.yaml`:

```yaml
state:
  user.runs:         { type: number, default: 0 }
  user.giftAccepted: { type: bool, default: false }
```

Two lounge scenes answer `hubVisit`. The first-ever greeting outranks the everyday one and is heard once ever; the everyday one keeps the default `once: run`:

```lute unverified="one file of the multi-file worked project on this page; it needs the occasion-declaring plugin and house.schema.yaml shown beside it"
---
kind: scene
id: hub.firstEver
uses: ../house.schema.yaml
on: hubVisit
priority: 20
once: user
---

## The lounge

@hypnos: Oh! You're back already? I mean — welcome home, I guess.
```

```lute unverified="one file of the multi-file worked project on this page; it needs the occasion-declaring plugin and house.schema.yaml shown beside it"
---
kind: scene
id: hub.welcome
uses: ../house.schema.yaml
on: hubVisit
priority: 10
---

## The lounge

@hypnos: Welcome back. Try not to die so much.
```

The old soldier's gift answers `talk` for one target, and only from the tenth run on:

```lute unverified="one file of the multi-file worked project on this page; it needs the occasion-declaring plugin and house.schema.yaml shown beside it"
---
kind: scene
id: achilles.gift
uses: ../house.schema.yaml
on: talk
target: npc.achilles
when: 'user.runs >= 10'
priority: 50
once: user
---

## The courtyard

@achilles: Ten times through that door, lad. That has earned you something.

<branch id="gift">
  <choice id="accept" label="Accept the gift">
    ::set{ user.giftAccepted = true }
    @achilles: Wear it well.
  </choice>
  <choice id="decline" label="Decline">
    @achilles: Another time, then.
  </choice>
</branch>
```

A quest tracks the gift, and two letters answer the `inbox` — one only after the gift:

```lute unverified="one file of the multi-file worked project on this page; it needs house.schema.yaml shown beside it"
---
kind: quest
uses: ../house.schema.yaml
title: The old soldier
---

<quest id="oldSoldier" title="The old soldier" start="true">
  <objective id="takeGift" title="Accept the old soldier's gift" done="user.giftAccepted"/>
</quest>
```

```lute unverified="one file of the multi-file worked project on this page; it needs the occasion-declaring plugin and house.schema.yaml shown beside it"
---
kind: lore
id: house.letters
uses: ../house.schema.yaml
---

<entry id="megNote" on="inbox" category="note" priority="5" when="user.giftAccepted">
  @meg: Heard the old soldier gave you something. Don't get sentimental.
</entry>

<entry id="dusaNote" on="inbox" category="note">
  @dusa: The lounge is spotless! Well, almost.
</entry>
```

The script plays the tenth run and the start of the eleventh:

```yaml
# plays/tenth-run.play.yaml
state: { user.runs: 10 }
steps:
  - occasion: hubVisit
  - occasion: inbox
    pick: dusaNote
  - occasion: talk
    target: npc.achilles
  - occasion: hubVisit
  - occasion: hubVisit
  - newRun: true
  - occasion: hubVisit
  - occasion: inbox
    pick: megNote
choose:
  gift: accept
```

```console
$ lute play house --script house/plays/tenth-run.play.yaml
```

```
── start ──────────────
  quest oldSoldier -> active
── step 1 · hubVisit ──────────────
  ✓ hub.firstEver [scene, priority 20]
  ✓ hub.welcome [scene, priority 10]
  → hub.firstEver
@hypnos: Oh! You're back already? I mean — welcome home, I guess.
── step 2 · inbox (select: all, pick: dusaNote) ──────────────
  ✓ dusaNote [entry, priority 0]
  ✗ megNote [entry, priority 5] — when: false
  → dusaNote
  entry dusaNote (first read)
@dusa: The lounge is spotless! Well, almost.
── step 3 · talk → npc.achilles ──────────────
  ✓ achilles.gift [scene, priority 50]
  → achilles.gift
@achilles: Ten times through that door, lad. That has earned you something.
▷ choice gift: [accept] decline        ← chosen: accept
  set user.giftAccepted = true
@achilles: Wear it well.
  oldSoldier.takeGift done
  quest oldSoldier -> complete
── step 4 · hubVisit ──────────────
  ✓ hub.welcome [scene, priority 10]
  ✗ hub.firstEver [scene, priority 20] — once: user — already presented
  → hub.welcome
@hypnos: Welcome back. Try not to die so much.
── step 5 · hubVisit ──────────────
  ✗ hub.firstEver [scene, priority 20] — once: user — already presented
  ✗ hub.welcome [scene, priority 10] — once: run — already presented this run
  → (no eligible beat — the occasion passes)
── step 6 · new run ──────────────
  run.* state, run-tier facts and once: run reset
── step 7 · hubVisit ──────────────
  ✓ hub.welcome [scene, priority 10]
  ✗ hub.firstEver [scene, priority 20] — once: user — already presented
  → hub.welcome
@hypnos: Welcome back. Try not to die so much.
── step 8 · inbox (select: all, pick: megNote) ──────────────
  ✓ megNote [entry, priority 5]
  ✓ dusaNote [entry, priority 0]
  → megNote
  entry megNote (first read)
@meg: Heard the old soldier gave you something. Don't get sentimental.
── end: complete (8 steps) ──────────────
```

Reading it step by step:

- **Start** — the quest's `start` is `true`, so it activates before the first step. (A quest with no `start` would wait for a scene's `::accept`.)
- **Step 1** — both lounge scenes are eligible; priority 20 beats 10.
- **Step 2** — `inbox` is `select: all`, so the script picks. `megNote`'s `when` is still false, so it is listed but not offered; picking it here would be an error (exit 1).
- **Step 3** — `user.runs` is 10, so the gift is eligible. Accepting it sets `user.giftAccepted`, and the quest advances right after the presentation.
- **Steps 4–5** — the first-ever greeting is spent for good (`once: user`); the everyday one plays once this run, and the next visit passes with no story.
- **Steps 6–7** — the new run resets `once: run` spending, so the everyday greeting is eligible again; `user.*` state and `once: user` spending persist.
- **Step 8** — the gift made `megNote` eligible, and the quest's completion is the same fact any later `after: completed("oldSoldier")` would see.
