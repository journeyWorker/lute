---
title: Cheatsheet
description: "One page to keep open while writing Lute 0.21.0: every construct with a minimal checked snippet (project layout, frontmatter, lines, choices, match, state, CEL, beats, quests, lore, components, timelines), the CLI at a glance, the diagnostics authors hit most, and the gotchas."
---

Every construct on one page, as snippets you can copy. Each `lute` block below is compile-checked in CI
against the real toolchain, and a link after each section goes to the full page.

## Project layout

```
my-game/
├── lute.project.yaml            profiles, plugins, identity, defaults
├── world.schema.yaml            run/user/app state, enums, defs, facts, rules
├── plugins/game.occasions/      optional: plugin.yaml + occasions/*.yaml
├── scenes/*.lute                kind: scene
├── quests/*.lute                kind: quest
├── lore/*.lute                  kind: lore
├── components/*.component.lute  component: <name>
├── tests/*.test.yaml            lute test
└── plays/*.play.yaml            lute play
```

`lute.project.yaml`:

```yaml
pluginsDir: plugins/
defaultProfile: game
profiles:
  game:
    plugins: { game.occasions: true }   # true = active with defaults
identity:                               # lineId below is the default
  lineId: "{prefix}.{speaker}_{code}"
  voiceKey: "{prefix}.{speaker}-{code}" # default {speaker}-{code} collides across scenes (E-DUP-VOICEKEY)
defaults:                               # frontmatter every document inherits
  luteVersion: "0.21.0"
  uses: [world.schema.yaml]             # resolved against THIS file's directory
```

`defaults:` accepts only `kind`, `character`, `season`, `episode`, `pov`, `luteVersion`,
`contentLang`, `uses`, `extends`, `components`, and `extra` (anything else is `E-DEFAULTS-KEY`).
A document that writes a key at all replaces the default for that key entirely, with no merging
(`uses: []` means "no imports"). A default that is illegal on a document's kind is skipped for
that document.

`world.schema.yaml` is plain YAML with no `---` fence. Scenes reach it with `uses:`:

```yaml
state:                                   # scalar only: number | bool | string | enum
  run.pressure: { type: number, default: 0 }
  run.mood:     { type: { enum: [calm, tense] }, default: calm }
  run.rival:    { type: { enum: [kai, lee] } }       # no default: maybe-unset until set
  user.runs:    { type: number, default: 0 }
  app.rating:   { type: { enum: [teen, adult] }, default: teen }
enums:                                   # content vocabulary: you declare every member
  emotion: [neutral, happy, worried]
  action:  { members: [fade-in-up, fade-out-down], exits: [fade-out-down] }
  anchor:  { members: [left, center, right], default: center }
entities:
  crew:  { members: [vesna, toma] }
  topic: { members: [manifest, heading] }
relations:
  awake:    { args: [crew], tier: run }
  knows:    { args: [crew, topic], tier: run }
  can_halt: { args: [crew], derive: true }
facts:
  - "awake(vesna)"
rules:
  - "can_halt(C) :- awake(C), knows(C, manifest)"
defs:
  calm: "run.pressure < 2"                    # shorthand: the body alone, type inferred (bool)
  veteran: "user.runs >= 10"
  vesnaKnows: "holds(knows(vesna, manifest))"
  zoom: "run.pressure > 2 ? 1.3 : 1.1"        # inferred number
  closeUp: "1.3"                              # a constant: the only kind of def an attribute takes
  atLeast: { type: bool, params: { n: number }, cel: "user.runs >= n" }   # params need type:
```

Def type inference: comparisons, `&&` `||` `!`, `holds`, `has`, `isSet` give `bool`; `count`,
arithmetic, and number literals give `number`; a bare path read gives that path's type. A body the
checker cannot type (for example `"@other"`) needs the long form `{ type: …, cel: … }`. Otherwise it
is `E-DEF-DECL`.

The seven vocabulary slots are `emotion`, `action`, `anchor`, `mood`, `volume`, `musicAction`, and
`vfxType`. Using one that nothing declares is `E-DOMAIN-UNKNOWN`. `action` must list `exits:` and
`anchor` must name a `default:`.

→ [State schemas](/state/schemas/) · [Imports](/language/imports/) · [Content vocabulary](/language/vocabulary/) · [Facts & Datalog](/state/facts-and-datalog/)

## Frontmatter keys by kind

| Kind | Required | Kind-only keys |
|---|---|---|
| `kind: scene` | `id:`, or the legacy `character` + `season` + `episode` | `id`, `character`, `season`, `episode`, `episodeId`, `pov`, `after`, beat keys `on` / `target` / `when` / `priority` / `once` |
| `kind: quest` | one or more `<quest>` in the body | `id` (optional bundle name) |
| `kind: lore` | one or more `<entry>` in the body | `id`, `series` |
| component (no `kind:`) | `component: <name>` | `component`, `params` |

Every root kind also accepts `title`, `luteVersion`, `contentLang`, `profile`, `plugins`, `uses`,
`extends`, `components`, `state`, `defs`, `enums`, `entities`, `relations`, `facts`, `rules`,
`codesLocked`, `mode`, and `extra` (free descriptive data). A key outside this set is
`E-META-UNKNOWN-KEY`. A quest's prerequisite is the `after=` attribute on `<quest>`. It is not a
frontmatter key.

→ [Frontmatter & profiles](/language/frontmatter-and-profiles/)

## Lines, cast & staging

```lute check
---
kind: scene
id: diner.night
pov: fixer
enums:
  emotion: [neutral, happy]
  action: { members: [fade-in-up, fade-out-down], exits: [fade-out-down] }
  anchor: { members: [left, center, right], default: center }
  mood: [peaceful]
  volume: [down, normal]
  musicAction: [start, fade-out]
  vfxType: [whiteOut]
state:
  run.affection: { type: number, default: 0 }
---

## Counter

::bg{location="diner" time="night"}
::music{action="start" mood="peaceful" volume="down"}
::auto{character="mira" anchor="center" action="fade-in-up"}
::camera{focus="mira" zoom="1.2" duration="0.5" wait="true"}
@narrator: The diner hums.
@mira{code="0010" emotion="happy"}: You're back, {{userName}}! Warmth: {{run.affection}}.
@fixer: I am.
@fixer{mono}: She remembered.
@mira{os}: Hold on!
@mira{as="???"}: ...who's there?
::sfx{sound="door bell"}
::vfx{type="whiteOut"}
::auto{character="mira" action="fade-out-down"}
::music{action="fade-out"}
::end{reason="closing"}
```

| Piece | Rule |
|---|---|
| `@speaker{attrs}: text` | `@narrator` is narration. The speaker equal to `pov:` is the player. Text after `: ` is literal to end of line. |
| line attrs | `code`, `emotion`, `variant`, `action`, `dialogMotion`, `as` (label override), `when` (guard), `id` (a jump label) |
| delivery flags | `{mono}` thought, `{os}` off-screen, `{vo}` voiceover. At most one per line, and never on `@narrator`. |
| `{{…}}` | `{{userName}}`, a declared state path, or `{{@def}}` (the artifact carries the def's body, and `lute run` / `lute play` evaluate it). Reading a maybe-unset path is `E-MAYBE-UNSET`. A line whose whole text is `@name` ships that literal text (`W-TEXT-LOOKS-LIKE-REF`); write `{{@name}}`. |
| shots | All content sits under a `## Heading`. A lone `# Title` does not open a shot. |
| directives | `::bg` `::music` `::sfx` `::auto` (entrance, pose, exit) `::camera` `::cut` `::vfx` `::video` `::end`. Timing keys: `duration`, `delay`, `wait="true"` (blocks). |
| comments | `/* … */` |

→ [Dialogue & cast](/language/dialogue-and-cast/) · [Core directives](/language/directives/)

## Choices, hubs, jumps & endings

```lute check
---
kind: scene
id: cafe.counter
state:
  scene.warmth: { type: number, default: 0 }
  run.metMira:  { type: bool, default: false }
  run.tip:      { type: number, default: 0 }
defs:
  warm: "scene.warmth >= 2"
---

## Counter

<branch id="greet" prompt="Mira looks up.">
  <choice id="wave" label="Wave" into="run.metMira">
    @mira: Hi!
    ::set{scene.warmth += 2}
  </choice>
  <choice id="tip" label="Leave a tip" into="run.tip" value="5">
    @mira: Thanks!
  </choice>
  <choice id="flirt" label="Flirt" when="@warm">
    @mira: Oh, stop.
  </choice>
</branch>

<hub id="chat">
  <choice id="coffee" label="Ask about the coffee" once>
    @mira: House blend.
  </choice>
  <choice id="cup" label="Ask about the missing cup">
    ::accept{quest="lostCup"}
    @mira: Find it and your next one is free.
  </choice>
  <choice id="leave" label="Leave" exit>
    @mira: Bye.
  </choice>
</hub>

::next{to="outro" when="run.tip > 0"}
@mira: No tip, huh.
::mark{id="outro"}
@narrator: The door closes behind you.
::end{reason="leftCafe"}
```

| Construct | Rule |
|---|---|
| `<branch id>` | A menu. The pick is recorded in `scene.choices.<id>`, which clears when the scene ends. At least one choice must be unguarded (`E-BRANCH-ALL-GUARDED`). Optional `prompt=` and `timeout="N"`. |
| `<choice id label>` | `when=` guard. `into="run.x"` writes `true`, or `value=` for number and enum paths, so later scenes can read it. |
| `<hub id>` | Re-presents eligible choices until an `exit`. `once` removes a choice after one take. It needs an unguarded `exit`, or every choice `once` (`E-HUB-NO-EXIT`). Each pick sets `scene.visited.<hub>.<choice>`. |
| `::next{to when}` | A forward-only jump to `::mark{id}` or a line's `id=`. A backward jump is `E-NEXT-BACKWARD`. Without `when`, the content after it is dead (`W-CODE-AFTER-NEXT`). |
| `::end{reason}` | Stops the walk. Content after it in the same body is `W-CODE-AFTER-END`. |
| `::accept{quest}` | Takes up an accept-driven quest (one with no `start`). See [Quests](#quests). |

→ [Choices & hubs](/language/choices-and-hubs/) · [Branch, match & when](/language/branch-match-when/)

## Match & when

```lute check
---
kind: scene
id: cafe.moods
state:
  run.tips:  { type: number, default: 0 }
  run.mood:  { type: { enum: [calm, tense, joyful] }, default: calm }
  run.rival: { type: { enum: [kai, lee] } }
---

## Moods

<match on="run.mood">
  <when is="calm">
    @mira: Quiet day.
  </when>
  <when is="tense|joyful">
    @mira: Busy day.
  </when>
</match>

<match on="run.tips">
  <when is="10..">
    @mira: My best customer.
  </when>
  <when is="1..9">
    @mira: Thanks, as always.
  </when>
  <otherwise>
    @mira: Hmm.
  </otherwise>
</match>

<match on="run.rival">
  <when is="kai">
    @mira: Kai was here earlier.
  </when>
  <when test="$ == 'lee' && run.tips > 3">
    @mira: Lee asked about you.
  </when>
  <when is="unset">
    @mira: Nobody asked about you.
  </when>
  <otherwise>
    @mira: Lee was here.
  </otherwise>
</match>

@mira{when="run.mood != 'calm'"}: Take a breath.
```

- `is=` takes enum members, `true`/`false`, numbers, inclusive ranges (`3..`, `..0`, `1..9`),
  `unset`, and `|` alternation. `test=` is a CEL guard with the subject bound to `$`. Writing both
  means pattern AND guard.
- Arms run top to bottom and the first match wins. A match must be exhaustive: an enum or bool with
  every member covered needs no `<otherwise>`. Numbers are real, so `1..9` and `10..` leave a gap at
  9.5. A maybe-unset subject needs `is="unset"` or `<otherwise>`.
- `test="$ == 'x'"` is `W-WHEN-TEST-LITERAL`, and `lute fix` rewrites it to `is="x"`.
- `@who{when="G"}: …` is sugar for a one-arm match. For a fact query (`holds(…)`) this line form is
  the only form, because `<match on="holds(…)">` is `E-MATCH-RELATION-SUBJECT`.
- Every tag sits alone on one physical line. `<when …>text</when>` on one line is
  `E-TAG-INLINE-BODY`, and a tag wrapped across lines is `E-TAG-NOT-ONE-LINE`.

→ [Branch, match & when](/language/branch-match-when/)

## State writes & facts

```lute check
---
kind: scene
id: ship.archive
state:
  run.trust: { type: number, default: 0 }
  run.seen:  { type: bool }
entities:
  crew:  { members: [vesna, toma] }
  topic: { members: [manifest, heading] }
relations:
  knows: { args: [crew, topic], tier: run }
---

## Archive

::set{run.seen = true}     /* the first write of a no-default path must be `=` */
::set{run.trust += 1}      /* also -= and *= */
@vesna{when="run.seen"}: You found the archive.
::assert{knows(vesna, manifest)}
@vesna{when="holds(knows(vesna, manifest))"}: I read the manifest.
@vesna{when="count(knows(_, manifest)) >= 2"}: So we both know.
::retract{knows(vesna, _)}
```

`_` is a wildcard in queries and in `::retract`. `app.*` is read-only (`E-APP-READONLY`). A relation
with `derive: true` or `reserved:` cannot be asserted from content. A relation's `key: [0]` makes
its first argument functional, so a new assert replaces the old fact.

→ [State model](/state/state-model/) · [Facts & Datalog](/state/facts-and-datalog/)

## CEL cheat

| Namespace | Resets | Content may write |
|---|---|---|
| `scene.*` | when the scene ends | yes |
| `run.*` | at a new run | yes |
| `user.*` | on a profile wipe | yes |
| `app.*` | on uninstall | no |
| `quest.<id>.state` (always assigned: `unset` until the quest activates, then `active` `complete` `failed`), `quest.<id>.activatedAt`, `quest.<id>.objectives.<o>.done` | engine | no |
| `entry.<id>.read` | engine; run-tier | no |
| `scene.choices.<branch>`, `scene.visited.<hub>.<choice>` | engine | no |
| `visited('<scene id>')` | never: the whole save | no |

| Operators | Functions & references |
|---|---|
| `== != < <= > >=` · `&& \|\| !` · `+ - * /` · `c ? a : b` · `x in ['a', 'b']` · string and number literals | `has(p)` / `isSet(p)` (assigned?) · `holds(rel(a, _))` · `count(rel(_)) >= n` · `validAt(rel(a), quest.q.activatedAt)` · `visited('scene.id')` · `@def` / `@def(args)` · `$` (inside `<match>` only) |

Not available: `%`, `size`, `matches`, `map`/`filter`/`exists`/`all` (`E-CEL-PROFILE`), in a guard or
in a def body. An unset value is not the string `'unset'` (`E-UNSET-LITERAL`); test it with
`!isSet(p)` or `is="unset"`. The exception is `quest.<id>.state`, where `unset` is a real member:
write `quest.q.state == 'unset'`, because `isSet(quest.q.state)` is always true
(`W-QUEST-STATE-ISSET`). Path segments, def names, and param names cannot contain `-`.

Where CEL goes: `<match on>`, `<when test>`, `when=` on a line or choice, `::set` right-hand sides,
`::next when`, beat `when:`, entry `when=`, quest `start` / `fail`, objective `done` / `when`,
`<on when>`, and `<reward when>`. In a directive attribute, a def reference is bare: write
`::camera{zoom=@closeUp}`, not `zoom="@closeUp"`. It is compiled as the constant the def folds to,
so a def that reads state, such as `zoom` above, is `E-ATTR-DEF-DYNAMIC`: branch with `<match>`
and write a literal in each arm.

→ [CEL expressions](/state/cel/) · [Definitions & params](/language/params/)

## Beats: scenes and entries that answer occasions

A beat is a scene or lore entry that the engine can pick when an occasion is raised.

```lute check
---
kind: scene
id: vesna.gift
on: talk
target: npc.vesna
when: 'user.runs >= 10 && !run.giftRefused'
priority: 50
once: user
after: 'visited("cafe.counter")'
state:
  user.runs:       { type: number, default: 0 }
  run.giftRefused: { type: bool, default: false }
---

## Gift

@vesna: You have been at this a while. Take this.
```

```lute check
---
kind: lore
id: vesna.barks
state:
  user.runs: { type: number, default: 0 }
---

<entry id="vesnaBark" on="talk" target="npc.vesna" category="bark">
  @vesna: Keep your head down.
</entry>

<entry id="vesnaBack" on="talk" target="npc.vesna" category="bark" priority="10" when="user.runs >= 3">
  @vesna: Back again?
</entry>

<entry id="vesnaFirst" on="talk" target="npc.vesna" category="bark" priority="20" when="!entry.vesnaFirst.read">
  @vesna: So you are the new one.
</entry>
```

| Key | Meaning |
|---|---|
| `on` | The occasion answered. It makes the scene a beat. Any other beat key without `on` is `E-BEAT-ATTR`. |
| `target` | Optional dotted id (`npc.vesna`). The beat is a candidate only when the occasion is raised for that target. |
| `when` | CEL over `run` / `user` / `app`, `quest.*`, `entry.*.read`, facts, and `visited()`. It cannot read `scene.*`. To compare strings, put the YAML value in double quotes so CEL keeps its single quotes: `when: "run.slot == 'night' && user.runs >= 3"`. Single quotes on both levels is `E-META-PARSE`. |
| `priority` | Integer, default `0`. Higher wins. |
| `once` | `run` (the default), `user` (once ever), or `false` (repeatable). Scenes only; entries repeat. |

Selection: the candidates are the beats whose `on` matches and whose `target` is absent or equal to
the raised target. A candidate is eligible when its `after:` and `when` hold and its `once` is
unspent. Eligible beats are ordered by priority, then document path, then declaration order.
`select: first` presents the top one; `select: all` lets the player pick. A plugin declares the
occasions:

```yaml
# plugins/game.occasions/plugin.yaml
id: game.occasions
version: 0.1.0
kind: capability
depends: [ { id: lute.core, range: "^0.0.1" } ]
exports:
  occasions: occasions/
```

```yaml
# plugins/game.occasions/occasions/game.yaml
occasions:
  hubVisit: { select: first }
  talk:     { select: first, target: true }
  runEnd:   { select: first }
  inbox:    { select: all, description: Letters waiting at the fountain }
```

With no occasion-declaring plugin, any identifier is accepted. Once a plugin declares occasions, an
unknown one is `E-OCCASION-UNKNOWN`, and `target` on an occasion without `target: true` is
`E-BEAT-ATTR`.

→ [Beats](/language/beats/) · [Playing a story](/tooling/play/)

## Quests

```lute check
---
kind: quest
id: cafe.quests
state:
  run.tips:  { type: number, default: 0 }
  run.fired: { type: bool, default: false }
  run.found: { type: bool, default: false }
  user.xp:   { type: number, default: 0 }
---

<quest id="regular" title="Become a regular" start="true" fail="run.fired" after="visited('cafe.counter')">
  <reward kind="BADGE" amount="1"/>
  <objective id="tip" title="Tip three times" done="run.tips >= 3">
    @narrator: Mira starts your order when you walk in.
  </objective>
  <objective id="calm" title="End the night calm" on="runEnd" done="run.tips > 0"/>
  <objective id="chat" title="Chat with Mira" done="visited('cafe.counter')" optional/>
  <objective id="help" title="Help out back" quest="sideJob"/>
  <on event="questComplete">
    ::set{user.xp += 50}
    @narrator: You are a regular now.
  </on>
  <on event="questFailed">
    @narrator: You are no longer welcome.
  </on>
</quest>

<quest id="sideJob" title="Help out back">
  <reward kind="GOLD" amount="10..20"/>
  <objective id="dishes" title="Do the dishes" done="run.tips >= 1"/>
</quest>

<quest id="lostCup" title="Find the lost cup">
  <objective id="find" title="Find it" done="run.found"/>
</quest>
```

| Piece | Rule |
|---|---|
| `start=` | Activates the quest (`unset` → `active`) when it holds. With no `start` the quest is accept-driven: it stays `unset` until a scene runs `::accept{quest="…"}`, or a mock accepts it. |
| `fail=` | `active` → `failed`. It wins over completion when both hold. |
| `after=` | The structural prerequisite for the scene graph: `visited` / `completed` / `active` with `&&` / `\|\|`. It does not gate activation; `start` does. Scenes write it as `after:` in frontmatter. |
| `<objective done>` | `done` is required (`E-OBJECTIVE-MISSING-DONE`). The quest completes when every non-`optional` objective is done. Completion is monotonic, and the body plays once. |
| `on="runEnd"` | `done` is judged only when that occasion is raised while the quest is active. |
| `when=` on an objective | Visibility only. It does not change completion. |
| `quest="child"` | A subquest: done when the child completes, and the parent fails when a required child fails. It cannot be combined with `done=` (`E-OBJECTIVE-QUEST-DONE`). A child with no `start` activates with its parent. |
| `<reward kind amount target when on/>` | Data for the engine. Content cannot read a reward, and `lute run` / `lute play` only print the `grant`, so do not also `::set` the same currency in an `<on>` handler: it would be paid twice. `amount` is an integer or a range `N..M`. `on="failed"` grants on failure. |
| `<on event>` | `questActive`, `questComplete`, `questFailed`, or a plugin world event. An optional `when=` guards it. |

Quest documents have no `#`/`##` headings, `<hub>`, or `<timeline>`. Other documents read a quest
through `<match on="quest.regular.state">` or `when="quest.regular.state == 'complete'"`. The state
is always assigned, so `quest.regular.state == 'unset'` means "not taken up yet".

→ [Quests & scenes](/language/quests-and-scenes/)

## Lore entries

```lute check
---
kind: lore
id: ship.records
series: captainsLog
entities:
  crew:  { members: [vesna] }
  topic: { members: [heading] }
relations:
  knows: { args: [crew, topic], tier: run }
state:
  run.fire: { type: bool, default: false }
---

<entry id="log1" target="item.captains_log" category="note" title="Day 1">
  @captain: We changed heading at midnight.
  ::assert{knows(vesna, heading)}
</entry>

<entry id="log2" target="item.captains_log" category="note" title="Day 2" when="entry.log1.read">
  <match on="run.fire">
    <when is="true">
      @narrator: The page is scorched.
    </when>
    <otherwise>
      @captain: Nobody noticed.
    </otherwise>
  </match>
</entry>
```

- `<entry>` attributes: `id` (required, unique in the project), `target`, `category`, `title`,
  `when`, `on`, and `priority`. Use `series` and `order` for a series that spans files. A
  document-level `series:` orders its entries by position in the file, and in such a document a
  per-entry `series=`/`order=` is `E-ENTRY-ATTR`.
- An entry body may hold content lines, `<match>`, `::set`, `::assert`, and `::retract`. Anything
  else, including `<branch>`, directives, and headings, is `E-GRAMMAR-NOT-ADMITTED`.
- Effects apply on the first read in a run only. After that, `entry.<id>.read` is `true` and any
  document can read it. It is run-tier: a new run resets it.

→ [Lore entries](/language/lore-entries/)

## Components, extends & params

```lute check
---
component: greet
params:
  who: string
  tier: { enum: [cold, warm] }
---

## Greet

<match on="@tier">
  <when is="warm">
    @narrator: A warm welcome.
  </when>
  <when is="cold">
    @narrator: A curt nod.
  </when>
</match>
```

A component file is `name.component.lute`. Its body holds lines, staging, `@param` refs, and a
`<match>` on a param, with no state reads or writes of its own. A `{{@param}}` renders a number,
bool, or enum param, never a `string` one. The importing scene lists it in `components:` and expands
it with `::use`. An argument may be the caller's `@def` (`tier=@mood`): an enum param requires every
value the def can produce to be a member (`E-COMPONENT-ARG`), and a state-dependent def that the
component puts into a directive attribute is `E-ATTR-DEF-DYNAMIC`:

```lute check="docs/examples/components/scene.lute"
---
kind: scene
character: demo
season: 1
episode: 2
uses: ../base.schema.yaml
components: [greet.component.lute]
---

## Greeting by Component

::use{component="greet" who="marina"}
@narrator: And the scene carries on.
```

A schema can refine another with `extends: base.schema.yaml`. The base is the lower layer, so
redeclaring a name overrides it; changing a state path's `type` is `E-EXTENDS-STATE-TYPE`. `uses:`
joins peer schemas, where a name declared twice is an error.

→ [Components & extends](/language/components-and-extends/) · [Definitions & params](/language/params/)

## Timeline

```lute check
---
kind: scene
id: storm.beat
---

## Storm

<timeline duration="1.2">
  <track subject="camera">
    ::camera{focus="mira" zoom="1.2" duration="0.6"}
    ::camera{shake="0.4" duration="0.3" at="0.7"}
  </track>
  <track channel="sfx">
    ::sfx{sound="thunder" at="0.5"}
  </track>
</timeline>

<timeline duration="1.0">
</timeline>
@narrator: After the pause.
```

A track holds staging directives and `::set` only. Its key (`subject=`, `channel=`, or `subject=` +
`property=`) must be unique. `at=` is absolute on the timeline's own clock. An empty timeline is a
timed pause.

→ [Timeline & property tracks](/language/timeline-and-property-tracks/)

## CLI

| Command | Use |
|---|---|
| `lute check <file> [--project <dir>]` | Check one document. Without `--project` it applies the nearest `lute.project.yaml` above the file and says so on stderr. |
| `lute check-project <dir>` | Check every document, plus connectivity, quest ids, `::accept` targets, occasions, and fact guards. It also compiles every clean document, so compile-stage errors (`E-DUP-VOICEKEY`, `E-CAPABILITY-MISMATCH`) fail here. |
| `lute fix <file>` | Mechanical migrations in place: the old `:line` sigil, `as=` → `into=`, and `test="$ == …"` → `is=`. |
| `lute tag <file>` | Back-fill a stable `code` on every line. |
| `lute compile <file> -o out.json` · `--all --project <dir> -o <outdir>` | Build artifacts. `--all` also writes `project.index.json`, including `beats`. |
| `lute trace <file> [--mock m.yaml] [--state P=V] [--fact "r(a)"] [--choose id=c[,c]] [--event e] [--accept q] [--occasion o] [--entry id]` | Preview the source against mocks. Exit `3` means a guard was unknown. |
| `lute run <artifact> [--mock m.yaml] [--occasion o] [--entry id]` | Run a compiled artifact the way an engine would. |
| `lute play <dir> --script p.play.yaml [--json]` | Raise occasions through the whole project, advancing quests. |
| `lute test [<dir>] [--project <dir>] [--coverage]` | Run every `*.test.yaml`. An incomplete walk fails. `--coverage` lists the untested documents of `--project`, or of the nearest `lute.project.yaml`. |
| `lute scenario <dir> [reach <node> \| envelope <node>] [--format text\|json\|dot]` | The `after:` graph, reachability, and guaranteed state and facts. A node is a scene id or `quest:<id>`. |
| `lute lore <dir>` | Entries by target and series, and which facts they reveal. |
| `lute context <file> [--project <dir>]` | Everything legal to write here: directives, vocabulary, state, occasions. |
| `lute lint [<path>] [--config lute.lint.yaml]` | Advisory editorial lints (`L-*`), configured per project. |
| `lute new scene\|quest\|lore\|schema <name> [--dir <dir>]` · `lute init <dir>` | Scaffold a document or a project. |

Trace mock (`--mock`), which also supplies a test's mock keys:

```yaml
# mocks/counter.yaml: lute trace scenes/counter.lute --project . --mock mocks/counter.yaml
file: ../scenes/counter.lute               # required under mocks/, which check-project validates
state:  { run.tip: 5 }                     # path: literal
facts:  ["awake(vesna)"]                   # trace does NOT load the schema's facts: seeds
choose: { greet: tip, chat: [coffee, leave] }   # branch: choice; hub: its visit order
events: [combatEnd]                        # world events, for <on event>
# quest walks also take:
#   accepts:   [lostCup]                   # accept-driven quests to take up
#   visited:   [cafe.counter]              # scenes already played; unlisted = not visited
#   occasions: [runEnd]                    # raised after the walk settles
```

`tests/regular.test.yaml` (`file:` is relative to the test file):

```yaml
file: ../quests/cafe.lute
state: { run.tips: 3 }
visited: [cafe.counter]
occasions: [runEnd]
expect:
  quests: { regular: complete, lostCup: unset }   # unset | active | complete | failed
  state: { user.xp: 50 }
  transcriptContains: ["You are a regular now."]
  exit: complete                                  # complete | incomplete; without it, incomplete fails
```

A test's `file:` must be a scene or quest document. A lore document is `E-TEST-LORE`; preview an
entry with `lute trace <file> --entry <id>` instead.

`plays/first.play.yaml`, which allows only these four top-level keys:

```yaml
state: { user.runs: 10, quest.lostCup.state: active }   # a quest.<id>.state seed sets that quest's status
facts: ["knows(vesna, manifest)"]
steps:
  - occasion: hubVisit
  - occasion: talk
    target: npc.vesna          # only on a `target: true` occasion
  - occasion: inbox
    pick: megNote              # required on `select: all`, refused on `select: first`
  - newRun: true               # resets run.*, run facts, once: run
  - occasion: runEnd           # judges <objective on="runEnd">
choose: { greet: wave, chat: [coffee, leave] }   # hub: visit order; a branch list: one per presentation
```

→ [CLI reference](/tooling/cli/) · [Tracing](/tooling/tracing/) · [Playing a story](/tooling/play/)

## Common diagnostics

| Code | What it usually means |
|---|---|
| `E-KIND-MISSING` | The frontmatter has no `kind:` and no `defaults:` supplies one. |
| `E-META-MISSING` | A scene has no `id:` and no `character` + `season` + `episode`. |
| `E-META-PARSE` | The frontmatter is not valid YAML, usually an unquoted value containing `: `. |
| `E-META-UNKNOWN-KEY` | The key is not legal on this kind, for example `after:` on a quest (use `<quest after=…>`). |
| `E-CONTENT-OUTSIDE-SHOT` | Content comes before the first `## Heading`. |
| `E-DOMAIN-UNKNOWN` | `emotion=`, `action=`, `anchor`, `mood`, … is used but its slot has no declared members. |
| `E-UNDECLARED` / `E-UNDECLARED-REF` | The state path, or the `@def`, is not declared, or its schema is not imported. |
| `E-MAYBE-UNSET` | A path is read that has no default and no dominating `::set` or `isSet` guard. |
| `W-QUEST-STATE-ISSET` | `isSet(quest.<id>.state)` is always true. Compare with `'unset'` instead. |
| `W-TEXT-LOOKS-LIKE-REF` | A line's whole text is `@name` for a def or param; it ships as that literal. Write `{{@name}}`. |
| `E-UNSET-UNCOVERED` / `E-NONEXHAUSTIVE` | A `<match>` misses `unset`, an enum member, or a numeric gap. Add arms or `<otherwise>`. |
| `E-TAG-INLINE-BODY` / `E-TAG-NOT-ONE-LINE` | A tag shares its line with its body, or a tag is wrapped across lines. |
| `E-BRANCH-ALL-GUARDED` / `E-HUB-NO-EXIT` | A menu could be empty, or a hub could never end. |
| `E-SET-TYPE` / `E-REF-TYPE` / `E-ATTR-TYPE` | A value has the wrong type for its slot, often a quoted `"@def"` in a directive. |
| `E-ATTR-DEF-DYNAMIC` | A directive attribute takes a `@def` that reads state. An attribute value must be a constant; branch with `<match>`. |
| `E-INTERP-DEF` | A `{{@def}}` whose body cannot be inlined into one expression (an expansion cycle, or a body that reads `$`). |
| `E-ATTR-QUOTE` | An attribute value in single quotes. Use `"…"`, and write a `"` inside it as `\"`. |
| `E-DEF-DECL` | A def is malformed: its type cannot be inferred, it has `params:` without `type:`, or it has an unknown key. |
| `E-OBJECTIVE-MISSING-DONE` / `E-OBJECTIVE-QUEST-DONE` | An objective has no `done`, or has both `quest=` and `done=`. |
| `E-GRAMMAR-NOT-ADMITTED` | The construct is not allowed in this kind, for example `<branch>` in an entry or a heading in a quest. |
| `E-BEAT-ATTR` | A beat key is malformed or has no `on`, `when` reads `scene.*`, or `target` is on an untargeted occasion. |
| `E-OCCASION-UNKNOWN` | A plugin declares occasions and this one is not among them. |
| `E-BEAT-UNREACHABLE` / `E-ARM-DEAD` | The condition can never hold. `check-project` also decides fact queries. |
| `E-ACCEPT-TARGET` | `::accept` names a quest that does not exist or that has a `start`. |
| `E-CONN-UNKNOWN-NODE` | `visited('…')` or `after` names no scene in the project. |
| `E-CONN-EPISODE-ID-DUP` / `E-QUEST-ID-DUP` | Two documents share a scene id or a quest id. |
| `E-DUP-VOICEKEY` | Lines with different text compile to one `voiceKey`. Set `identity.voiceKey: "{prefix}.{speaker}-{code}"`. |
| `E-CAPABILITY-MISMATCH` | The project's documents resolve two capability snapshots (different profiles or scene-local `plugins:`), so it cannot compile as one. |
| `E-TEST-LORE` | A `*.test.yaml` names a lore document, which has no walk. Use `lute trace <file> --entry <id>`. |
| `W-FACT-GUARANTEED` | A fact guard is always true on every route, so it is redundant. |
| `W-BEAT-SHADOWED` | An earlier beat that is always eligible and never spent wins every time. |
| `W-ENTRY-REF-UNKNOWN` | `entry.<id>.read` names an entry that nothing declares. |
| `E-LEGACY-CONTENT-SIGIL` · `W-WHEN-TEST-LITERAL` | Old syntax. `lute fix` rewrites it. |
| `E-PERSIST-REMOVED` | Delete `persist=` from the choice by hand; `into=` alone records the run fact. |

## Gotchas

**A quest with no `start` never activates by itself.** It is accept-driven: it stays `unset` until
a scene runs `::accept{quest="id"}`, or a mock or test lists it in `accepts:`. `::accept` on a
quest that has a `start` is `E-ACCEPT-TARGET`. The exception is a child named by `quest=`, which
activates with its parent.

**`once` defaults to `run`.** A scene beat plays at most once per run unless you write
`once: false`. Entries have no `once`: `when="!entry.<id>.read"` makes an entry heard once **per
run**, because `entry.<id>.read` is run-tier and a new run resets it. For once ever, guard on a
`user.*` flag the entry sets, or make it a scene with `once: user`.

**`after:` is structural and `when:` carries state.** `after:` reads only `visited`, `completed`,
and `active` with `&&` and `||`. It feeds the scene graph. A beat is eligible only when both hold.
On a `<quest>`, `after=` is graph metadata only and does not delay activation. To gate a quest on
a scene, put the condition in `start`: `start="visited('cafe.counter')"`.

**`::end` ends the whole `lute play` walk**, not only the current scene. The step still settles
first (quest progress and the occasion's `<objective on>`), then the walk stops. A hub or beat scene
that should hand control back to the game simply ends at its last line.

**`visited()` covers the whole save.** A new run does not clear it. A single-file `check` never
decides it, `check-project` validates the id, and `trace` and `test` need a `visited:` list.

**A def's type comes from its body.** `calm: "run.pressure < 2"` is a bool. A body that is only
another def (`"@calm"`) cannot be inferred, and a def with `params:` needs `type:`.

**Quote YAML values that contain `: ` or start with `!`, `@`, `{`, `[`, `*`, or `&`.**
`title: Chapter 1: Start` is a parse error, and an unquoted `when: !run.x` is a YAML tag, not a
negation:

```lute expect="E-META-PARSE"
---
kind: scene
id: chapter.one
title: Chapter 1: Start
---

## Start

@narrator: Once upon a time.
```

**`scene.choices.<branch>` can be unset, even directly after the branch.** Give the match an
`is="unset"` arm or an `<otherwise>`:

```lute expect="E-MAYBE-UNSET,E-UNSET-UNCOVERED"
---
kind: scene
id: choice.readback
---

## Ask

<branch id="ask">
  <choice id="yes" label="Yes">
    @mira: Great.
  </choice>
  <choice id="no" label="No">
    @mira: Oh.
  </choice>
</branch>

<match on="scene.choices.ask">
  <when is="yes">
    @mira: You said yes.
  </when>
  <when is="no">
    @mira: You said no.
  </when>
</match>
```

**`lute check` finds the project; the other single-file commands do not.** `lute check <file>`
applies the nearest `lute.project.yaml` above the file (a stderr note names it). `trace`, `compile`,
`context`, and `test` resolve the project only with `--project <dir>`; without it, anything hoisted
by `defaults:` or declared by a plugin is missing. `check-project`, `play`, and `scenario` load the
project from the directory you give them.

**`trace` and `test` start from an explicit world and do not derive.** Schema `facts:` seeds are
not loaded, so pass `--fact` or `facts:`. An unmocked base fact is false. An unmocked
`derive: true` fact is unknown and halts the walk (exit 3), even when you supplied the facts its
rule needs, so mock the derived fact itself. In `lute test` that halt fails the test unless it
declares `expect: { exit: incomplete }`. `lute run` and `lute play` apply the seeds and the Datalog
rules.

**Directive attributes take bare refs to constant defs.** Write `::camera{zoom=@closeUp}`. The
quoted `zoom="@closeUp"` is the literal string `@closeUp` (`E-ATTR-TYPE` on a number attribute). A
def that reads state is `E-ATTR-DEF-DYNAMIC`.

**Attribute values use double quotes.** `label='"Hi."'` is `E-ATTR-QUOTE`. Write
`label="\"Hi.\""`; the label is `"Hi."`.

**Numbers are real numbers in `<match>`.** `is="1..9"` followed by `is="10.."` does not cover
`9.5`. Add an `<otherwise>`, or use open ranges that meet.

**`lute trace` stops at a taken `::next`.** The trace reports the jump and ends there, so the
content after the target `::mark` is not shown. `lute run` on the compiled artifact follows the
jump.
