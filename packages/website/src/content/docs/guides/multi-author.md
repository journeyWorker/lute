---
title: Many authors, one project
description: "How a lead and several area writers share one Lute project: ownership by directory, kinds assembled with add:, defaults, id conventions, contract beats, hand-off plays, and the merge gates that catch what collides."
---

A game of any size is not written by one person. This guide is for a **lead** who owns the world
and a handful of **area writers** who each own a part of the map, all inside one Lute project and
one git repository. It describes the conventions that let them work in parallel, the language
features that support them, and the commands a branch must pass before it merges.

Everything here comes from a real round of that work: a monster-collecting RPG written by one
lead and four area writers at once (818 beats, 211 static facts, about 130 documents). Most of
what the language gained in dsl 0.26.0 (draft) exists because of it: kinds assembled across
files with [`add:`](#kinds-assembled-across-files-add), glob imports and a default quest tier in
[`defaults:`](#defaults-every-document-inherits), [`E-STATE-DECL-CONFLICT`](#state-declare-a-shared-path-once),
[`@@who:`](#shared-components) in components, [`advance: { to }`](#clock-hand-offs), and
[`lute refs`](#merge-gates).

## The shape: a world contract and areas

Split the project into two kinds of files.

- **The world contract** belongs to the lead: `lute.project.yaml`, the shared schemas (state,
  relations, rules, defs, the entity kinds), the engine plugin (occasions, events, reward kinds,
  directives, bridge calls), the shared components, and the **spine**, the documents and plays
  that tie the areas together.
- **An area** belongs to one writer: its scenes, lore, quests, tests, its steps file for the
  spine play, its own schema, and its own cast file.

Draw the line **by directory, never by a block inside a shared file**. Two writers who never edit
the same file never produce a merge conflict, and a reviewer can tell from the paths alone whether
a branch stayed in its lane.

```
lute.project.yaml                     lead
schema/world.schema.yaml              lead   shared state, relations, rules, defs
schema/roster.schema.yaml             lead   the kinds (person, trainer, place) and the lead's members
schema/areas/<area>.schema.yaml       area   the area's members (add:), private state, defs
plugins/<engine>/**                   lead   occasions, events, reward kinds, directives, bridge
plugins/<engine>/cast/<area>.yaml     area   the area's named characters
components/**                         lead   shared components
scenes/spine/ lore/spine/ quests/spine/ tests/spine/        lead
scenes/<area>/ lore/<area>/ quests/<area>/ tests/<area>/    area
plays/spine.play.yaml                 lead   includes every area's steps file, in map order
plays/steps/<area>.steps.yaml         area   keep the last step's expect: green
plays/<area>-*.play.yaml              area   the area's own playthroughs
```

Write this table down in the repository (an `AREAS.md` next to the manifest) together with
the shared ids each area may use and what each area hands to the next. Lute does not know who
owns a file; the table and code review do.

## Defaults every document inherits

Everything a document would otherwise repeat goes into the manifest's `defaults:`, so a new area
file needs no frontmatter beyond `kind:`, `id:` and `title:`:

```yaml
# lute.project.yaml (lead)
pluginsDir: plugins/
defaultProfile: harbor
profiles:
  harbor:
    plugins: { harbor.engine: true }
defaults:
  luteVersion: "0.25.1"
  uses:
    - schema/world.schema.yaml
    - schema/roster.schema.yaml
    - schema/areas/*.schema.yaml    # one file per area; a new area needs no edit here
  questTier: run                    # every <quest> that writes no tier= is a run quest
  components:
    - components/obtain.component.lute
    - components/supply.component.lute
```

- **Globs in `uses:`** (dsl 0.26.0 §2.4). An entry may be a glob (`*`, `**`), expanded in path
  order. A glob that matches nothing imports nothing and is no error, whether the directory is
  empty or does not exist yet (git keeps no empty directory), so the lead can add the glob before
  the first area schema lands. A literal path must still exist.
- **`questTier: run | user`** sets the `tier=` of every `<quest>` that writes none. A quest may
  still write its own, and a subquest's tier must equal its parent's (`E-QUEST-TIER-MIX`).
- **Never write `uses:` or `components:` in an area document.** A document that writes a key
  replaces the default for that key entirely, with no merging, so one document with its own
  `uses:` silently loses the whole world contract.

## Kinds assembled across files: `add:`

Entity kinds (`person`, `place`, a `trainer` sub-kind) are shared vocabulary: an occasion's
targets, a relation's arguments and a typed directive attribute all read them. Before 0.26 every
member had to be listed in the one file that declared the kind, so every area edited the lead's
roster. Since dsl 0.26.0 §2.3, **exactly one** schema declares a kind, and any other imported
schema adds members to it with `add:`:

```yaml
# schema/roster.schema.yaml (lead): the kinds, the spine's members, the contract ids
entities:
  person:
    members:
      - mira
      - keeper        # contract, written by north
  guard:
    subsetOf: person
    members: [northGuard1]
  place:
    members:
      - dock
      - lighthouse    # contract, written by north
```

```yaml unverified="an area schema: its add: lists extend kinds that schema/roster.schema.yaml declares, so it checks only inside its project (check-project clean there)"
# schema/areas/north.schema.yaml (area north)
state:
  run.northFogLifted: { type: bool, default: false }
entities:
  person: { add: [northGull] }
  guard:  { add: [northGuard2] }
  place:  { add: [northCliff] }
```

- **A sub-kind's members are members of its parent.** `northGuard2` above is added only to
  `guard`; because `guard` is `subsetOf: person`, it is a `person` too, so `talk@npc.northGuard2`
  and `spotted@guard.northGuard2` both work. Listing it in `person` as well is allowed and is not a
  duplicate. (Before 0.26, a sub-kind member missing from its parent was an error; that
  `E-ENTITY-KIND-SHAPE` cause is gone.)
- **A member listed twice is an error**, whether twice in one list or once by each of two area
  files: `E-ENTITY-KIND-SHAPE`, naming both files and lines (dsl 0.26.0 §2.2). This is what keeps
  two areas from silently sharing one trainer called `youngsterAda`.
- **`add:` needs a base.** An `add:` for a kind no import declares is `E-ENTITY-KIND-SHAPE` with a
  did-you-mean, and so is an `add:` onto an `open:` kind or beside `members:` / `open:` in the same
  entry.

Like the other schema errors, these are reported once, at the schema line, and folded
(`(+N more callers)`) instead of once for every document that imports the schema.

<!-- lute-diagnostics -->
```
schema/areas/south.schema.yaml:2:29: error [E-ENTITY-KIND-SHAPE] entity kind `person` lists `northGull` twice — in the `add:` of `schema/areas/north.schema.yaml` (line 5) and in the `add:` of `schema/areas/south.schema.yaml` (line 2); list each member once (dsl 0.26.0 §2.2)
```

Keep area-private declarations in the area schema, each name prefixed with the area
(`run.northFogLifted`, a def `northFogDay`, a relation `northRumor`). The glob imports every area
schema into every document, so a name two areas both declare collides: `E-USES-DUP-STATE`,
`E-USES-DUP-DEF` and `E-USES-DUP-RELATION` report it once, at the schema line.

## State: declare a shared path once

A path two areas both read or write belongs in a schema, declared once. If two documents each
declare the same path in their own frontmatter `state:`, `check-project` compares the
declarations: they must agree on `type`, `default`, `per` and `owner`, since they share one
runtime value. When they do not, it is **`E-STATE-DECL-CONFLICT`** (dsl 0.26.0 §2.1), naming
both files and lines, and `lute play` refuses a project that declares one path with two types.
`scene.*` paths are scene-local and exempt, and a declaration that refines one it `extends:`
(an overridden default) is not a conflict.

<!-- lute-diagnostics -->
```
./scenes/spine/dock.lute:6:3: error [E-STATE-DECL-CONFLICT] state path `run.tide` is declared as bool, default false at `./scenes/north/lighthouse.lute:6` but as number, default 0 at `./scenes/spine/dock.lute:6`; every declaration of one path must agree on type, default, per and owner — they share one runtime value (declare it once in a schema both documents import) (dsl 0.26.0 §2.1)
```

## Ids: prefix everything that is project-wide

Most ids in a Lute project are project-wide, and two areas that pick the same one collide. Prefix
them with the area:

| Thing | Convention | Example |
|---|---|---|
| scene id | `<area>.<camelCase>`, file in `scenes/<area>/` | `north.lighthouse` in `scenes/north/lighthouse.lute` |
| lore document id | `<area>.<topic>` | `north.guards` |
| bundle beat | camelCase; its canonical id is `<doc id>.<beat>` | `north.guards.anyGuard` |
| entry id | `<area><Topic>`, project-unique | `northGullAfterStorm` |
| quest id | `<area><Name>` | `northLostBuoy` |
| branch / hub id | `<area><Name>` (a play's `choose:` keys are project-wide) | `northKeeperAsk` |
| `share=` key | `<area><Name>` | `northFogBell` |
| entity member | `<placeOrRoute><Role><Name>` for generic people | `r3YoungsterAda`, `northGuard2` |
| area state, defs, relations | area prefix | `run.northFogLifted`, `northFogDay` |

Entry ids carry no document prefix, so they are the easiest to collide on. Plays and tests may
name an entry `<document id>.<entry id>` since dsl 0.26.0 (`north.guards.northGullAfterStorm`),
which makes a play easier to read, but the entry id itself must still be unique.

## Display names: one cast file per area

Give each area its own cast file under the engine plugin, and make the cast the only place a
character's display name is written. A shared component that speaks as its `speaker` param
(`@@who:`, [below](#shared-components)) then shows the right name without a `name=` argument.

```yaml
# plugins/harbor.engine/cast/north.yaml (area north)
cast:
  northGull:   { name: Old Gull }
  northGuard1: { name: Tower Guard, sharedName: true }
  northGuard2: { name: Tower Guard, sharedName: true }
```

With four areas naming people, two of them sooner or later pick the same display name for
different people. `check-project` warns **`W-DISPLAY-NAME-DUP`** (dsl 0.26.0 §2.8, advisory; also
`lute lint`) when two cast entries have the same `name:`, a cast name equals a `::use{… name="…"}`
display string, or two such strings name different speakers. A name that is meant to be shared,
a role like "Tower Guard" or "Eclipse Grunt", is marked `sharedName: true` on the cast entry and
not counted.

## Shared components

The lead writes the components every area uses: a trainer battle, handing over a key item, a
shop. Since dsl 0.26.0 a component can do what the areas used to copy by hand:

- **`@@who: …`** speaks as the cast member the `speaker` param `who` names at each `::use`:
  portrait, voice and cast checks apply as for a direct line.
- **Params take `default:`** (a literal or a `@def`); an omitted argument takes it.
- **A component reads its own directive results** (`scene.battle.fight.won` after its own
  `::battle`), so the host no longer passes the outcome back in.
- **`::use{… when="…"}`** runs the whole expansion or none of it.
- **A param typed by an entity kind** (`{ type: { entity: keyItem } }`), or passed whole into an
  attribute typed that way, is checked at each `::use` with a did-you-mean.

```lute unverified="an excerpt of a multi-file project: the ::battle bridge, run.money and the defeated relation come from its engine plugin and schemas"
---
component: trainerBattle
effects: true
params:
  who: speaker     # the trainer: a cast member, trainer.<who>, defeated(<who>)
  intro: string
  win: string
  lose: string
  prize: { type: number, default: 100 }
---

## Trainer battle

@@who: {{@intro}}
::battle{foe=@who kind="trainer" resultKey="fight"}
<match on="scene.battle.fight.won">
  <when is="true">
    @@who: {{@win}}
    ::assert{defeated(@who)}
    ::set{run.money += @prize}
  </when>
  <otherwise>
    @@who: {{@lose}}
  </otherwise>
</match>
```

An area's route trainer is then one line, and the name on the dialogue box comes from the cast:

```lute
<beat id="r3YoungsterAda" on="trainerSpotted" target="trainer.r3YoungsterAda" once="false" when="!holds(defeated(r3YoungsterAda))">
  ::use{component="trainerBattle" who="r3YoungsterAda" intro="I like shorts!" win="Aww, my shorts are torn." lose="Shorts win again!"}
</beat>
```

Freeze a shared component's params once areas depend on them: adding a param with a `default:`
is safe, renaming or removing one breaks every area at once. See
[Components](/language/components-and-extends/) for the full rules.

## Contract beats

The spine depends on the areas: the lighthouse must hand over the lamp oil before the finale can
light the lamp. Make each such dependency a **contract beat**. The lead writes it first, as a
stub, in the area's directory, and the area owns it from then on:

```lute unverified="an excerpt of a multi-file project: the enterPlace occasion, the place kind and the obtain component come from its manifest and schemas"
---
kind: scene
id: north.lighthouse
title: The lighthouse
on: enterPlace
target: place.lighthouse
once: run
---

/* CONTRACT (seeded by the lead, owned by north): the id, occasion and target
   stay; it produces hasItem(lampOil). Rewrite the lines, add shots, branches
   and after: edges freely. */

## Shot 1

@keeper: You came for the oil. Everyone does, eventually.
::use{component="obtain" item="lampOil" label="lamp oil" who="keeper"}
::set{run.northFogLifted = true}
```

The contract is the beat's **id, occasion and target, and the facts and state it produces**. The
area may rewrite everything else. Because the stub exists from day one, the spine's quests have a
producer for every fact they need, and `check-project` is green before the area has written a
line. If an area deletes the beat or stops asserting the fact, the spine's objective becomes
`E-OBJECTIVE-UNSATISFIABLE` at its next check: the checker enforces the contract.

**Contract beats or `--wip`.** A lead who drafts the spine before any contract beat exists can
check it with `lute check-project . --wip` instead. `E-OBJECTIVE-UNSATISFIABLE`,
`E-BEAT-UNREACHABLE`, `E-ENTRY-UNREACHABLE` and `E-ARM-DEAD` become warnings when they are caused
only by a relation nothing produces yet, and since dsl 0.26.0 §2.6 that includes a relation only a
shared component asserts with an unbound param (`::assert{hasItem(@item)}` in `obtain`, with no
`::use` yet). A relation whose producers exist but never match stays an error. Use `--wip` on a
spine branch; the merge gate runs without it.

## Plays: one spine, one steps file per area

The spine play walks the whole game, area by area, through `include:` steps. Each area owns its
steps file and may add steps freely, but keeps the **last step's `expect:`**: that is the area's
hand-off, what the next area and the spine rely on.

```yaml
# plays/spine.play.yaml (lead)
steps:
  - occasion: enterPlace
    target: place.dock
    expect: { winner: spine.dock, quests: { lamp: active } }
  - include: steps/north.steps.yaml
  - occasion: enterPlace
    target: place.dock
    expect: { winner: spine.dock }
expect:
  transcriptContains: ["You found oil."]
```

```yaml
# plays/steps/north.steps.yaml (area north)
steps:
  - occasion: spotted
    target: guard.northGuard2
    expect: { winner: north.guards.anyGuard }
  - occasion: enterPlace
    target: place.lighthouse
    label: north hand-off
    expect:
      winner: north.lighthouse
      facts: [hasItem(lampOil)]
      state: { run.northFogLifted: true }
      quests: { lamp: complete }
```

Rules for an included steps file:

- Put `choose:` and `bridges:` on the **step** (`bridges: { battle: [ { won: true } ] }`), never at
  the top level of the steps file: the spine play owns the top level.
- Name winners by their canonical ids: a scene by its id, a bundle beat as `<doc>.<beat>`, an entry
  by its id or `<doc>.<entry>`.
- An accept-driven quest the engine takes up outside any document (`accept="external"`, a quest
  board) is accepted mid-play with an `engine: { accept: [northLostBuoy] }` step instead of a save
  seed.

### Clock hand-offs

An included file cannot know how far the areas before it moved the clock. Since dsl 0.26.0 a
step moves the clock to a position rather than by a count, and states where the clock stands:

```yaml
steps:
  - advance: { to: night }                                   # the next night, never backward
    label: the tower by night
    expect: { clock: { slot: night } }
  - advance: { to: { weekday: Friday, slot: morning } }      # the next Friday morning
    expect: { clock: { weekday: Friday, slot: morning } }
```

`advance: { to }` always moves forward, to the next such position after the current one (never
zero steps: if the clock is already there, the next one). It is one advance: the slot occasion is
raised once, where the clock stops, and every midnight it crosses raises `dayEnd` and `dayStart`;
write separate `advance: slot` steps when content answers the slots in between. A step's
`expect: { clock: { weekday, slot, day } }` judges where the clock stands after the step. Put one
on an area's first step (the arrival) and on its hand-off, so a change in an
earlier area's clock shows up as a failed expectation instead of a beat that silently stops being
eligible. See [Playing a story](/tooling/play/).

## Merge gates

Before an area hands a branch to the lead, it runs, from the project root:

```sh
lute check-project . --deny-warnings   # 0 errors, 0 warnings
lute test . --project .                # every test and every play with expect:, spine included
lute beats .                           # no W-BEAT-PRIORITY-TIE / W-BEAT-SHADOWED you did not intend
lute refs . --attr give.item --reward ITEM   # who gives what
```

- **`check-project --deny-warnings`** turns every warning into an error. With many writers a
  tolerated warning is a warning nobody reads; zero is the only stable number. Silence an
  intended one at its source (`sharedName: true`, a `priority=`), not with a flag.
- **`lute test`** runs every `*.test.yaml` and every play with an `expect:`, so the spine play and
  every area's hand-off run on every branch. Since dsl 0.26.0 it loads and checks the project
  once and runs the tests and plays in parallel on all cores (`RAYON_NUM_THREADS` is respected);
  on the 818-beat project the whole suite went from about 100 s to under 10 s.
- **`lute beats`** shows, for every occasion and target, which beats answer it and in which order.
  Two areas answering the same `talk@npc.<id>` or the same fallback at equal priority show up
  here, and as `W-BEAT-PRIORITY-TIE`, which since 0.26 says why the two `when`s are not
  exclusive. A fallback that a higher-priority beat always beats is listed `covered by <id>`.
- **`lute refs`** (dsl 0.26.0 §2.5) lists every value of a directive attribute, or every reward
  target, with the documents and lines using it, including values passed through a component
  (`via component <name>`). It is how a lead sees "two areas both hand out the Super Rod" before
  it ships:

```
$ lute refs . --attr give.item
::give.item: 2 value(s)
  `flare` — 1 use(s) in 1 document(s)
    scenes/south/pier.lute:13 (via component `supply`)
  `rope` — 1 use(s) in 1 document(s)
    scenes/south/pier.lute:12
```

Commit only files you own. The lead's review is the one gate the tool cannot run.

## What the tool catches, and what it does not

| Collision | Caught by |
|---|---|
| Two areas add the same member to a kind | `E-ENTITY-KIND-SHAPE`, both files and lines |
| `add:` to a kind no import declares (a typo) | `E-ENTITY-KIND-SHAPE`, with a did-you-mean |
| Two documents declare one state path differently | `E-STATE-DECL-CONFLICT` |
| Two area schemas declare the same state, def or relation name | `E-USES-DUP-STATE`, `E-USES-DUP-DEF`, `E-USES-DUP-RELATION` |
| An engine id that does not exist (`::give{item="flair"}`) | `E-BAD-ENUM` with a did-you-mean, when the attribute is typed `{ entity: K }`, also through a component param |
| A reward with no target, or a target the engine does not know | `E-REWARD-TARGET`, when the reward kind declares a `target:` contract |
| Two speakers shown with the same name | `W-DISPLAY-NAME-DUP` (unless `sharedName: true`) |
| Two areas answer one occasion and target at equal priority | `W-BEAT-PRIORITY-TIE`, `W-BEAT-SHADOWED`, `lute beats` |
| An area removes or breaks a contract beat | `E-OBJECTIVE-UNSATISFIABLE` on the spine quest, and the hand-off `expect:` in `lute test` |
| An area moves the clock another area depends on | `expect: { clock: … }` in the steps files |

What Lute does **not** know:

- **Who owns a file.** An area that edits the lead's roster or another area's scene passes every
  gate. Ownership is the `AREAS.md` table plus review (a `CODEOWNERS` file enforces it on a
  hosting service).
- **Private state.** The `defaults.uses` glob imports every area schema into every document, so
  one area can read another's `run.southFossil`. The prefix makes it visible in review; nothing
  rejects it.
- **Whether two areas giving the same item is a bug.** `lute refs` lists it; judging it is yours.
- **Untyped ids.** An engine id in a plain `string` attribute, a colliding entry id, or two areas
  choosing the same `share=` key (their beats are then spent together) are not checked. Type
  engine ids with an entity kind, and prefix every project-wide id.
- **Voice and tone.** A character another area wrote, speaking out of character, is a review
  finding.
