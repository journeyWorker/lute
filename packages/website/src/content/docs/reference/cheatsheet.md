---
title: Cheatsheet
description: "One page to keep open while writing Lute 0.23.1: every construct with a minimal checked snippet (project layout, frontmatter, lines, choices, match, state, CEL, beats, quests, lore, components, timelines), the CLI at a glance, the diagnostics authors hit most, and the gotchas."
---

Every construct on one page, as snippets you can copy. Each `lute` block below is compile-checked in CI
against the real toolchain, and a link after each section goes to the full page.

## Project layout

```
my-game/
├── lute.project.yaml            profiles, plugins, identity, defaults
├── world.schema.yaml            run/user/app state, enums, defs, facts, rules
├── plugins/game.occasions/      optional: plugin.yaml + occasions/*.yaml + events/*.yaml
├── scenes/*.lute                kind: scene
├── quests/*.lute                kind: quest
├── lore/*.lute                  kind: lore
├── components/*.component.lute  component: <name>
├── tests/*.test.yaml            lute test
└── plays/*.play.yaml            lute play; lute test runs those with an expect:
```

`lute init --template beats <dir>` scaffolds this layout: an occasions plugin, beats with `id:`, a
quest, entry beats, a play script with `engine:` steps and `expect:`, and scenario tests, all
passing `check-project`, `test`, and `play` as generated.

`lute.project.yaml`:

```yaml
pluginsDir: plugins/
defaultProfile: game
profiles:
  game:
    plugins: { game.occasions: true }   # true = active with defaults
identity:                               # both values below are the defaults
  lineId: "{prefix}.{speaker}_{code}"
  voiceKey: "{prefix}.{speaker}-{code}" # the 0.21 default was {speaker}-{code}: pin it to keep old keys
defaults:                               # frontmatter every document inherits
  luteVersion: "0.23.1"
  uses: [world.schema.yaml]             # resolved against THIS file's directory
```

`defaults:` accepts only `kind`, `character`, `season`, `episode`, `pov`, `luteVersion`,
`contentLang`, `uses`, `extends`, `components`, and `extra` (anything else is `E-DEFAULTS-KEY`).
A document that writes a key at all replaces the default for that key entirely, with no merging
(`uses: []` means "no imports"). A default that is illegal on a document's kind is skipped for
that document.

The default `voiceKey` carries `{prefix}` since 0.22.0, so voice keys are unique across the project.
A project that recorded audio against the 0.21 keys pins `identity: { voiceKey: "{speaker}-{code}" }`,
and then gets `E-DUP-VOICEKEY` wherever lines with different text land on one key.

`world.schema.yaml` is plain YAML with no `---` fence. Scenes reach it with `uses:`:

```yaml
state:                                   # scalar only: number | bool | string | enum
  run.pressure: { type: number, default: 0 }
  run.day:      { type: number, default: 1, owner: engine }   # content reads it; ::set is E-ENGINE-OWNED-WRITE
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
cast:                                    # optional (0.23.0): once declared, any other speaker is E-CAST-UNKNOWN
  vesna: { name: Vesna }
  toma:  { name: Toma }
  mira:  { name: Mira }
```

Def type inference: comparisons, `&&` `||` `!`, `holds`, `has`, `isSet` give `bool`; `count`,
arithmetic, and number literals give `number`; a bare path read gives that path's type. A body the
checker cannot type (for example `"@other"`) needs the long form `{ type: …, cel: … }`. Otherwise it
is `E-DEF-DECL`.

The seven vocabulary slots are `emotion`, `action`, `anchor`, `mood`, `volume`, `musicAction`, and
`vfxType`. Using one that nothing declares is `E-DOMAIN-UNKNOWN`. `action` must list `exits:` and
`anchor` must name a `default:`.

`cast:` (0.23.0) names the speakers, in a schema document or in a plugin's `cast` export
(`cast/*.yaml`, the same `cast:` map). Once any cast is declared, a speaker outside it (other than
`@narrator`) is `E-CAST-UNKNOWN` with a did-you-mean, in scenes, quests, entries, and bundle beats.
With no cast declared, any speaker id is accepted. A scene's frontmatter cannot declare `cast:`
(`E-META-UNKNOWN-KEY`). `lute context` lists the cast.

→ [State schemas](/state/schemas/) · [Imports](/language/imports/) · [Content vocabulary](/language/vocabulary/) · [Facts & Datalog](/state/facts-and-datalog/)

## Frontmatter keys by kind

| Kind | Required | Kind-only keys |
|---|---|---|
| `kind: scene` | `id:`, or the legacy `character` + `season` + `episode` | `id`, `character`, `season`, `episode`, `episodeId`, `pov`, `after`, beat keys `on` / `target` / `when` / `priority` / `once` / `also` |
| `kind: quest` | one or more `<quest>` in the body | `id` (optional bundle name) |
| `kind: lore` | one or more `<entry>` or `<beat>` in the body | `id` (required when it holds a `<beat>`), `series` |
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
| `@speaker{attrs}: text` | `@narrator` is narration; any other speaker is dialogue (the `pov:` speaker included — `pov` is descriptive only). Text after `: ` is literal to end of line. With a declared `cast:`, the speaker must be in it (`E-CAST-UNKNOWN`). |
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

<hub id="chat" prompt="Anything else?">
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
| `<hub id>` | Re-presents eligible choices until an `exit`. `once` removes a choice after one take. It needs an unguarded `exit`, or every choice `once` (`E-HUB-NO-EXIT`). Each pick sets `scene.visited.<hub>.<choice>`. Optional `prompt=` (0.23.0) is the question shown with the options; an empty one is `E-BRANCH-PROMPT`. |
| `::next{to when}` | A forward-only jump to `::mark{id}` or a line's `id=`. A backward jump is `E-NEXT-BACKWARD`. Without `when`, the content after it is dead (`W-CODE-AFTER-NEXT`). |
| `::end{reason}` | Ends the scene. In `lute play` it ends only the presentation (or quest handler) it runs in; the playthrough goes on. Content after it in the same body is `W-CODE-AFTER-END`. |
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
  9.5. A maybe-unset subject needs `is="unset"` or `<otherwise>`. Arms narrow it: inside
  `<when is="x">` the subject is set, and after an `is="unset"` arm with no `test`, later arms and
  `<otherwise>` read it as set (no `E-MAYBE-UNSET`).
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
@vesna{when="isSet(prev.run.trust) && prev.run.trust >= 3"}: You trusted me last time.
@vesna{when="run.seen"}: You found the archive.
::assert{knows(vesna, manifest)}
@vesna{when="holds(knows(vesna, manifest))"}: I read the manifest.
@vesna{when="count(knows(_, manifest)) >= 2"}: So we both know.
::retract{knows(vesna, _)}
```

`_` is a wildcard in queries and in `::retract`. `app.*` is read-only (`E-APP-READONLY`), and so is a
path declared `owner: engine` (`E-ENGINE-OWNED-WRITE`): the engine writes it, and `lute play` writes
it with an `engine:` step. A relation with `derive: true` or `reserved:` cannot be asserted from
content. A relation's `key: [0]` makes its first argument functional, so a new assert replaces the
old fact.

`prev.run.<path>` (0.23.0) reads the value `run.<path>` had when the previous run ended, for every
declared `run.*` path, with the same type. It is read-only (`E-QUEST-RESERVED-WRITE`) and unset until
a first run ends, so every read needs `isSet(prev.run.x)` or an `unset` arm (`E-MAYBE-UNSET`).
Declaring a `prev.*` path yourself is `E-STATE-NAMESPACE`. `lute play` snapshots it at `newRun`; a
play script's `state:` or a mock's `state:` may seed it.

→ [State model](/state/state-model/) · [Facts & Datalog](/state/facts-and-datalog/)

## CEL cheat

| Namespace | Resets | Content may write |
|---|---|---|
| `scene.*` | when the scene ends | yes |
| `run.*` | at a new run | yes |
| `user.*` | on a profile wipe | yes |
| `app.*` | on uninstall | no |
| `quest.<id>.state` (always assigned: `unset` until the quest activates, then `active` `complete` `failed`), `quest.<id>.activatedAt`, `quest.<id>.objectives.<o>.done` | engine; a `tier="run"` quest returns to `unset` at a new run | no |
| `entry.<id>.read` | engine; run-tier | no |
| `entry.<id>.everRead` | engine; user-tier: set on the first read, never reset by a new run | no |
| a path declared `owner: engine` | as its namespace | no (`E-ENGINE-OWNED-WRITE`) |
| `prev.run.<path>` (0.23.0) | engine: snapshot of `run.<path>` at run end; unset before the first run ends | no (`E-QUEST-RESERVED-WRITE`) |
| `scene.choices.<branch>`, `scene.visited.<hub>.<choice>` | engine | no |
| `visited('<scene id>')`, `visited('<doc>.<beat>')` | never: the whole save | no |

| Operators | Functions & references |
|---|---|
| `== != < <= > >=` · `&& \|\| !` · `+ - * /` · `c ? a : b` · `x in ['a', 'b']` · string and number literals | `has(p)` / `isSet(p)` (assigned?) · `holds(rel(a, _))` · `count(rel(_)) >= n` · `validAt(rel(a), quest.q.activatedAt)` · `visited('scene.id')` · `@def` / `@def(args)` · `$` (inside `<match>` only) |

Not available: `%`, `size`, `matches`, `map`/`filter`/`exists`/`all` (`E-CEL-PROFILE`), in a guard or
in a def body. An unset value is not the string `'unset'` (`E-UNSET-LITERAL`); test it with
`!isSet(p)` or `is="unset"`. The exception is `quest.<id>.state`, where `unset` is a real member:
write `quest.q.state == 'unset'`, because `isSet(quest.q.state)` is always true
(`W-QUEST-STATE-ISSET`). Path segments, def names, and param names cannot contain `-`.

Where CEL goes: `<match on>`, `<when test>`, `when=` on a line or choice, `::set` right-hand sides,
`::next when`, beat `when:`, entry `when=`, quest `start` / `fail`, objective `done` / `by` / `when`,
`<on when>`, and `<reward when>`. In a directive attribute, a def reference is bare: write
`::camera{zoom=@closeUp}`, not `zoom="@closeUp"`. It is compiled as the constant the def folds to,
so a def that reads state, such as `zoom` above, is `E-ATTR-DEF-DYNAMIC`: branch with `<match>`
and write a literal in each arm.

→ [CEL expressions](/state/cel/) · [Definitions & params](/language/params/)

## Beats: scenes and entries that answer occasions

A beat is a scene, lore entry, or bundle beat that the engine can pick when an occasion is raised.

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

<entry id="vesnaFirst" on="talk" target="npc.vesna" category="bark" priority="20" once="user">
  @vesna: So you are the new one.
</entry>
```

A lore document can also hold `<beat>` blocks (0.23.0), bundle beats, beside its entries:

```lute check
---
kind: lore
id: cafe.talks
state:
  run.tips: { type: number, default: 0 }
---

<beat id="miraOrder" on="talk" target="npc.mira" title="Order" priority="10" when="run.tips >= 3">
  @mira: The usual?
  <hub id="order" prompt="What will it be?">
    <choice id="usual" label="The usual" exit>
      @mira: Coming up.
    </choice>
  </hub>
</beat>

<beat id="miraHum" on="talk" target="npc.mira" once="false" also>
  @narrator: Mira hums while she works.
</beat>
```

A bundle beat takes `id`, `on`, `target`, `title`, `when`, `priority`, `once`, and `also`, and its
body is a scene body (lines, branches, hubs, match, directives). The document needs `id:`, the beat
`id` is an identifier without `-`, and the beat's canonical id is `<document id>.<beat id>`
(`cafe.talks.miraOrder`): the id that `lute play`, `presented:`, `visited('cafe.talks.miraOrder')`,
and `lute trace --beat` use. It behaves like a scene beat: `once` defaults to `run`, presentation
spends it, and it has no `after:`. `title` labels it in a `select: all` menu. A canonical id equal to
a scene id is `E-CONN-EPISODE-ID-DUP`.

| Key | Meaning |
|---|---|
| `on` | The occasion answered. It makes the scene a beat. Any other beat key without `on` is `E-BEAT-ATTR`. |
| `target` | Optional dotted id (`npc.vesna`). The beat is a candidate only when the occasion is raised for that target. When the occasion declares a target domain, the target must be `<prefix>.<member>` of it. |
| `when` | CEL over `run` / `user` / `app`, `quest.*`, `entry.*.read` / `entry.*.everRead`, facts, and `visited()`. It cannot read `scene.*`. To compare strings, put the YAML value in double quotes so CEL keeps its single quotes: `when: "run.slot == 'night' && user.runs >= 3"`. Single quotes on both levels is `E-META-PARSE`. |
| `priority` | Integer, default `0`. Higher wins. |
| `once` | Scenes: `run` (the default), `user` (once ever), or `false` (repeatable). Entries: `once="run"` (until a new run resets `entry.<id>.read`) or `once="user"` (spent once `entry.<id>.everRead` is set); without it an entry repeats. |
| `also` | 0.23.0, scenes (`also: true`) and bundle beats (`also`) on a `select: first` occasion: presented after the winner, or alone when no main beat is eligible, and never replaces it. On an entry, or on a `select: all` / `sequence` occasion, it is `E-BEAT-ATTR`. `W-BEAT-SHADOWED` and `W-BEAT-PRIORITY-TIE` ignore `also` beats. |

Selection: the candidates are the beats whose `on` matches and whose `target` is absent or equal to
the raised target. A candidate is eligible when its `after:` and `when` hold and its `once` is
unspent. Eligible beats are ordered by priority, then document path, then declaration order.
`select: first` presents the top beat that is not `also`, then the eligible `also` beats;
`select: sequence` (0.23.0) presents every eligible beat in that order (a routine, then the day's
event); `select: all` lets the player pick. Eligibility is decided once, when the occasion is raised,
and quests settle after each presentation. Two `select: first` beats with equal priority and the
same (or no) target, whose `when`s are not provably exclusive, leave the winner to file order
(`W-BEAT-PRIORITY-TIE`, from `check-project`). A plugin declares the occasions, any world events,
and optionally reward kinds and the cast:

```yaml
# plugins/game.occasions/plugin.yaml
id: game.occasions
version: 0.1.0
kind: capability
depends: [ { id: lute.core, range: "^0.0.1" } ]
exports:
  occasions: occasions/
  events: events/
  rewardkinds: rewardkinds/             # rewardKinds: see Quests
  cast: cast/                           # cast: { <id>: { name } }, like a schema's cast:
```

```yaml
# plugins/game.occasions/occasions/game.yaml
occasions:
  hubVisit: { select: first }
  talk:     { select: first, target: { prefix: npc, entity: crew } }   # or `target: true`: any dotted id
  greet:    { select: first, target: { prefix: npc, entity: crew, members: [mira, vesna] } }   # only these members (0.23.1)
  runEnd:   { select: first }
  evening:  { select: sequence }        # every eligible beat, in selection order (0.23.0)
  inbox:    { select: all, description: Letters waiting at the fountain }
```

```yaml
# plugins/game.occasions/events/game.yaml: for <on event> and a play's `event:` step
events:
  - name: combatEnd
```

With no occasion-declaring plugin, any identifier is accepted. Once a plugin declares occasions, an
unknown one is `E-OCCASION-UNKNOWN`, and `target` on an occasion that declares no `target` is
`E-BEAT-ATTR`. A target domain `{ prefix, entity }` admits `<prefix>.<member>` for each member of
that `entities:` kind (any member of an `open:` kind), so `target: npc.vesan` is `E-BEAT-ATTR` with a
did-you-mean.

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
  run.day:   { type: number, default: 1 }
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

<quest id="lostCup" title="Find the lost cup" tier="run">
  <objective id="find" title="Find it by day 3" done="run.found" by="run.day > 3"/>
  <objective id="tell" title="Tell Mira" on="talk" target="npc.mira" done="run.found"/>
</quest>
```

| Piece | Rule |
|---|---|
| `start=` | Activates the quest (`unset` → `active`) when it holds. With no `start` the quest is accept-driven: it stays `unset` until a scene runs `::accept{quest="…"}`, or a mock accepts it. A play script seeds a save's status with `quests:`. |
| `fail=` | `active` → `failed`. It wins over completion when both hold. |
| `after=` | The structural prerequisite for the scene graph: `visited` / `completed` / `active` with `&&` / `\|\|`. It does not gate activation; `start` does. Scenes write it as `after:` in frontmatter. |
| `tier="run"` | A new run returns the quest to `unset` and undoes its objectives. The default `tier="user"` keeps its status across runs. A subquest's tier must equal its parent's (`E-QUEST-TIER-MIX`). |
| `<objective done>` | `done` is required (`E-OBJECTIVE-MISSING-DONE`). The quest completes when every non-`optional` objective is done. Completion is monotonic, and the body plays once. |
| `on="runEnd"` | `done` is judged only when that occasion is raised while the quest is active. Raising it first runs the handlers of a same-named declared world event (`<on event="runEnd">`), then judges the objectives. |
| `by=` (0.23.0) | A deadline. The first time it holds while the objective is not done, the objective fails for good; a failed required objective fails its quest (`failed` rewards, `questFailed`). `done` is judged first, so a deadline never fails a done objective. On an `on=` objective, `by` is judged only at that occasion's raise, after `done`. |
| `on="talk" target="npc.mira"` (0.23.0) | Judged only when the occasion is raised for that target, checked like a beat target (`E-BEAT-ATTR`). Tooling raises it as `talk@npc.mira`; a `lute play` step with `target:` judges it. |
| `when=` on an objective | Visibility only. It does not change completion. |
| `quest="child"` | A subquest: done when the child completes, and the parent fails when a required child fails. It cannot be combined with `done=` (`E-OBJECTIVE-QUEST-DONE`). A child with no `start` activates with its parent. |
| `<reward kind amount target when on/>` | Data for the engine, which pays it. Content cannot read a reward, so do not also `::set` the same currency in an `<on>` handler: it would be paid twice. `amount` is an integer or a range `N..M`. `on="failed"` grants on failure. `lute run`, `lute play`, `lute trace`, and `lute test` print the `grant`; when the kind declares `credits:` (below) they also add a scalar amount to that path, and a `::set` of that path in the quest's `<on>` or objective bodies is `W-REWARD-DOUBLE-CREDIT`. |
| `<on event>` | `questActive`, `questComplete`, `questFailed`, or a plugin world event. An optional `when=` guards it. A `questFailed` handler on a quest that can never fail (no `fail`, no required objective with a `by=` deadline, no required subquest objective that can fail, no parent quest) is `W-QUEST-HANDLER-DEAD`. |

Quest documents have no `#`/`##` headings, `<hub>`, or `<timeline>`. Other documents read a quest
through `<match on="quest.regular.state">` or `when="quest.regular.state == 'complete'"`. The state
is always assigned, so `quest.regular.state == 'unset'` means "not taken up yet".

A reward kind can name the state path its grants pay into (0.23.0):

```yaml
# plugins/game.occasions/rewardkinds/game.yaml
rewardKinds:
  GOLD:  { credits: user.gold }         # a grant adds its amount to user.gold (a range is the engine's roll)
  BADGE: {}
```

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
  `when`, `on`, `priority`, and `once` (`run` or `user`, on a beat only). Use `series` and `order`
  for a series that spans files. A document-level `series:` orders its entries by position in the
  file, and in such a document a per-entry `series=`/`order=` is `E-ENTRY-ATTR`.
- An entry body may hold content lines, `<match>`, `::set`, `::assert`, and `::retract`. Anything
  else, including `<branch>`, directives, and headings, is `E-GRAMMAR-NOT-ADMITTED`.
- Effects apply on the first read in a run only. After that, `entry.<id>.read` is `true` and any
  document can read it. It is run-tier: a new run resets it. `entry.<id>.everRead` is its user-tier
  twin, set on the first read and never reset by a new run.
- A lore document may also hold `<beat>` blocks with a scene body (0.23.0); see
  [Beats](#beats-scenes-and-entries-that-answer-occasions).

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

@narrator: {{@who}} walks in.

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
bool, or enum param, and since 0.23.0 a `string` one: a literal `::use` argument is substituted at
expansion, so each call site ships its own sentence under its own `lineId`. Binding an interpolated
`string` param to a `@def` is `E-REF-TYPE`. The importing scene lists it in `components:` and expands
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

A line a component expands to is addressed `{prefix}.{component}#{n}.{speaker}_{code}`, where `n`
counts the host's `::use`s of that component (1-based, in document order), so two uses never share
a `lineId` and a component line's code does not depend on the host lines around it.

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
| `lute check-project <dir> [--wip]` | Check every document, plus connectivity, quest ids, `::accept` targets, occasions, and fact guards. It also compiles every clean document, so compile-stage errors (`E-DUP-VOICEKEY`, `E-CAPABILITY-MISMATCH`) fail here. Project advisories such as `W-BEAT-PRIORITY-TIE` and `W-QUEST-HANDLER-DEAD` come from here too. `--wip` (0.23.0) reports `E-BEAT-UNREACHABLE`, `E-ENTRY-UNREACHABLE`, and `E-OBJECTIVE-UNSATISFIABLE` as warnings when the guard is dead only because a relation has no producer yet (no seed, assert, rule, or `reserved`); a relation that has producers but never matches stays an error. |
| `lute fix <file\|dir>` | Mechanical migrations in place: the old `:line` sigil, `as=` → `into=`, and `test="$ == …"` → `is=`. A directory covers every `.lute` file under it, recursively, in sorted order. |
| `lute tag <file\|dir>` | Back-fill a stable `code` on every line, of one file or every `.lute` file under a directory. |
| `lute compile <file> -o out.json` · `--all --project <dir> -o <outdir>` | Build artifacts. `--all` also writes `project.index.json`, including `beats`. |
| `lute trace <file> [--mock m.yaml] [--state P=V] [--fact "r(a)"] [--choose id=c[,c]] [--event e] [--accept q] [--occasion o[@target]] [--entry id \| --beat id] [--no-derive]` | Preview the source against mocks, with the project's seed facts and rules applied. Exit `3` means a guard was unknown. `--occasion talk@npc.mira` raises an occasion for a target (0.23.0). `--beat` presents one bundle beat by local or canonical id (`E-TRACE-BEAT` when it names none). |
| `lute run <artifact> [--mock m.yaml] [--occasion o[@target]] [--entry id \| --beat id]` | Run a compiled artifact the way an engine would. A lore artifact needs exactly one of `--entry` and `--beat` (a bundle beat's canonical id, or its bare id when unambiguous). |
| `lute play <dir> --script p.play.yaml [--json] [--ir] [--explain <atom>] [--no-derive]` | Raise occasions through the whole project, advancing quests. A missed `expect:` exits `1`. Staging prints as authored; `--ir` prints the lowered records instead, injected ones marked. `--explain` (repeatable) prints, after the play, the derivation tree of a ground atom, or the failing premises of every rule that could conclude it. |
| `lute test [<dir> \| <file>] [--project <dir>] [--coverage] [--no-derive]` | Run every `*.test.yaml`, and every `*.play.yaml` that carries an `expect:`, or the one test or play file given. Without `--project`, tests resolve against the nearest `lute.project.yaml` (noted on stderr). An incomplete walk fails, and so does a test whose `file:` is missing (`E-TEST-FILE`), without stopping the suite. `--coverage` lists the documents no test traced and no play presented, of `--project` or of the nearest `lute.project.yaml`. |
| `lute scenario <dir> [reach <node> \| envelope <node> \| knowledge [--for <node>]] [--format text\|json\|dot]` | The `after:` graph, reachability, and guaranteed state and facts. A node is a scene id, `quest:<id>`, or a bundle beat's canonical id (bare or `beat:<doc>.<beat>`; drawn as an edgeless entry node). `knowledge` (0.23.0) traces every fact-guarded beat, entry, and objective to the relations it queries and each relation to its producers through the rules: asserting documents, seed facts, the engine (`reserved`), or no producer, and names what can defeat a negated premise. Its `--for` also takes an entry id or `<quest>.<objective>`. |
| `lute beats <dir> [--occasion o] [--target t] [--json]` | 0.23.0. Each occasion's (and target's) beat ladder in selection order, with priority, `once`, `also`, `after:`, `when`, title, and the `check-project` verdicts (unreachable, shadowed, tied, once-run-user). The project need not check clean. |
| `lute calendar <dir> [--axis run.day=1..7] [--axis quest.q.state=unset,active] [--axis 'holds(awake(toma))=true,false'] [--occasion o] [--target t] [--script p.play.yaml [--until <step \| label>]] [--where <cel>] [--json \| --csv]` | 0.23.0. For every cell of the axes' product (first axis slowest), play's own eligibility per occasion: the winner or the presented list, `+N` shadowed eligible beats, `?` for an undecided cell, then the beats never eligible in any cell. It starts from the script's save with its steps replayed (up to `--until`), or the declared defaults. `--where` drops cells where the condition does not hold. A targeted occasion gets one column per target its beats name. |
| `lute lore <dir>` | Entries and beats by target and series, and which facts they reveal. |
| `lute context <file> [--project <dir>]` | Everything legal to write here: directives (built-ins included), vocabulary, state (marking `owner: engine`), defs, relations with their tier and `reserved`, occasions with target domains, the cast, component signatures, and every scene, quest, and entry id. |
| `lute lint [<path>] [--config lute.lint.yaml]` | Advisory editorial lints (`L-*`), configured per project. The linear-VN metrics skip beats, components, quests, and lore. |
| `lute doctor [<dir>]` | Toolchain and project setup: versions, active plugins, occasions with the number of beats answering each, play scripts and tests, whether the `lute-lsp` on `PATH` is this version, and whether a running `lute-lsp` is stale (restart the editor). |
| `lute new scene\|quest\|lore\|schema <name> [--dir <dir>]` · `lute init <dir> [--template minimal\|investigation\|beats]` | Scaffold a document or a project. New documents get an `id:` and omit what `defaults:` supplies. `lute new scene <name> --on <occasion> [--target <prefix>.<member>]` writes a beat, checking both against the project. |

Trace mock (`--mock`), which also supplies a test's mock keys:

```yaml
# mocks/counter.yaml: lute trace scenes/counter.lute --project . --mock mocks/counter.yaml
file: ../scenes/counter.lute               # required under mocks/, which check-project validates
state:  { run.tip: 5 }                     # path: literal
facts:  ["knows(vesna, manifest)"]         # base facts, on top of the schema's facts: seeds
choose: { greet: tip, chat: [coffee, leave] }   # branch: choice; hub: its visit order
events: [combatEnd]                        # world events, for <on event>
# derive: false                            # the 0.21 model: no seeds, no rules
# a save's history, for the paths the document reads:
#   visited:     [cafe.counter]            # scenes already played; unlisted = not visited
#   quests:      { regular: complete }     # quest.<id>.state
#   entriesRead: { run: [log1], user: [vesnaFirst] }   # entry.<id>.read / entry.<id>.everRead
# quest walks also take:
#   accepts:   [lostCup]                   # accept-driven quests to take up
#   occasions: [runEnd, talk@npc.mira]     # raised after the walk settles; <occasion>@<target> for a target
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
  facts: ["regular(mira)"]                        # hold at the end, after derivation
  notFacts: ["banned(mira)"]
  transcriptContains: ["You are a regular now."]
  transcriptLacks: ["You are no longer welcome."]
  exit: complete                                  # complete | incomplete; without it, incomplete fails
```

```yaml
# tests/counter.test.yaml
file: ../scenes/counter.lute
choose: { greet: wave, chat: [coffee, leave] }
expect:
  offered: { greet: [wave, tip] }       # the exact set offered: flirt's `when` failed
```

```yaml
# tests/barks.test.yaml: a lore test names the entries it presents
file: ../lore/barks.lute
entries: [vesnaFirst, vesnaBack]        # in order, read flags set between; or `entry: <id>`
state: { user.runs: 3 }
expect:
  transcriptContains: ["So you are the new one.", "Back again?"]
  eligible: { vesnaFirst: true }        # the `when` verdict; `eligible: false` for the one presented
```

```yaml
# tests/order.test.yaml: a bundle beat, by its bare or canonical id
file: ../lore/talks.lute
beat: miraOrder                         # or cafe.talks.miraOrder
state: { run.tips: 3 }
expect:
  eligible: true
```

`offered:` is the exact option set a branch or hub showed, across its presentations. A lore test
without `entry:`, `entries:`, or `beat:` is `E-TEST-LORE`; `lute trace <file> --entry <id>` previews
one entry. A test that presents an ineligible entry or beat passes with a note unless it asserts
`eligible:`.

`plays/first.play.yaml`. Its top-level keys are `state`, `facts`, `choose`, `derive`, the save
seeds `visited`, `presented`, `quests`, and `entriesRead`, `expect`, and `steps`. Each step is one
`occasion`, `engine`, `event`, `newRun`, or `end`:

```yaml
visited: [cafe.counter]                 # save seeds, applied before step 1
presented: { user: [vesna.gift] }       # spent `once: user` / `once: run` beats
quests: { lostCup: active }             # unset | active | complete | failed; objectives start undone
entriesRead: { user: [vesnaFirst] }     # run: entry.<id>.read · user: entry.<id>.everRead
state: { user.runs: 10, run.tips: 3, quest.lostCup.objectives.find.done: true }   # objective progress
facts: ["knows(vesna, manifest)"]
choose: { greet: wave, chat: [coffee, leave] }   # hub: visit order; a branch list: one per presentation
steps:
  - occasion: hubVisit
    label: arrival                      # printed in the step header
    choose: { greet: tip }              # this step only, replacing that key
  - occasion: talk
    target: npc.vesna                   # <prefix>.<member> of the occasion's target domain
    expect: { winner: vesnaBack, offered: [vesnaBack, vesnaBark], notOffered: [vesna.gift] }
  - occasion: talk
    target: npc.mira
    expect: { presented: [cafe.talks.miraOrder, cafe.talks.miraHum] }   # the winner, then its `also` beats
  - occasion: inbox
    pick: megNote                       # required on `select: all`, refused on `select: first` and `sequence`
  - occasion: inbox
    pick: none                          # pass: nothing presented or spent
  - occasion: evening                   # select: sequence: every eligible beat, in selection order
  - occasion: runEnd                    # a same-named world event's <on event> first, then <objective on="runEnd">
    expect: { quests: { lostCup: active }, state: { run.tips: 3 } }   # judged right after this step settles
  - event: combatEnd                    # a world event: active quests' <on event> run
  - engine:                             # writes what the engine owns; presents nothing
      state: { run.day: { add: 1 } }    # a literal, or { add: n }; quest.* is refused
      facts: ["awake(toma)"]            # any declared base relation, reserved ones included
      retract: ["awake(vesna)"]
  - newRun: { facts: ["knows(vesna, manifest)"] }   # or `true`; resets run.*, run facts, once: run, tier="run" quests
  - occasion: hubVisit
    repeat: 2
  - end: true                           # ends the playthrough (exit 0); later steps print as skipped
  - occasion: hubVisit                  # skipped
expect:                                 # judged at the end; a miss exits 1
  exit: complete
  quests: { regular: complete, lostCup: unset }
  state: { user.xp: 50, run.day: 1 }
  facts: ["can_halt(vesna)"]            # after derivation
  notFacts: ["awake(toma)"]
  transcriptContains: ["You are a regular now."]
  transcriptLacks: ["You are no longer welcome."]
```

- An `engine:` step writes declared state, facts, and retracts, checked against their types before
  anything plays, then settles the quest lifecycle, so a write can complete a quest on the spot. It
  refuses `quest.*`: a quest's status is the lifecycle's, seeded with `quests:`. `newRun` takes the
  same `state:` and `facts:` as the new run's seed.
- `target`, `pick`, and `choose` belong to an `occasion` step; `label` goes on any step, and `repeat`
  on any but `end`. A step `expect:` takes `winner` (`none` when the occasion passes), `offered` (a subset
  of the eligible beats), `notOffered`, and `presented` (0.23.0: the exact ids presented, in order)
  on an `occasion` step, and `quests`, `state`, `facts`, `notFacts` (0.23.1) on any step but `end`.
  A miss names the step and both values. A step with `target:` also judges that target's
  `<objective on target>`.
- A `newRun` step prints the `prev.run.*` snapshot it took and each run-tier quest it reset, with
  its previous status.
- `lute play . --script plays/first.play.yaml --explain "can_halt(vesna)"` shows which rule
  concluded the atom and where each premise came from.

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
| `E-MAYBE-UNSET` | A path is read that has no default and no dominating `::set` or `isSet` guard. Every `prev.run.*` read needs one. |
| `E-CAST-UNKNOWN` | A cast is declared (a schema's `cast:` or a plugin `cast` export) and this speaker is not in it. The message suggests the nearest id. |
| `E-ENGINE-OWNED-WRITE` | A `::set` writes a path declared `owner: engine`. Content only reads it; `lute play` writes it with an `engine:` step, and trace and test with a mock's `state:`. |
| `W-QUEST-STATE-ISSET` | `isSet(quest.<id>.state)` is always true. Compare with `'unset'` instead. |
| `W-TEXT-LOOKS-LIKE-REF` | A line's whole text is `@name` for a def or param; it ships as that literal. Write `{{@name}}`. |
| `E-UNSET-UNCOVERED` / `E-NONEXHAUSTIVE` | A `<match>` misses `unset`, an enum member, or a numeric gap. Add arms or `<otherwise>`. |
| `E-TAG-INLINE-BODY` / `E-TAG-NOT-ONE-LINE` | A tag shares its line with its body, or a tag is wrapped across lines. |
| `E-BRANCH-ALL-GUARDED` / `E-HUB-NO-EXIT` | A menu could be empty, or a hub could never end. |
| `E-SET-TYPE` / `E-REF-TYPE` / `E-ATTR-TYPE` | A value has the wrong type for its slot, often a quoted `"@def"` in a directive, or a `@def` bound to a component `string` param the component interpolates. |
| `E-ATTR-DEF-DYNAMIC` | A directive attribute takes a `@def` that reads state. An attribute value must be a constant; branch with `<match>`. |
| `E-INTERP-DEF` | A `{{@def}}` whose body cannot be inlined into one expression (an expansion cycle, or a body that reads `$`). |
| `E-ATTR-QUOTE` | An attribute value in single quotes. Use `"…"`, and write a `"` inside it as `\"`. |
| `E-DEF-DECL` | A def is malformed: its type cannot be inferred, it has `params:` without `type:`, or it has an unknown key. |
| `E-OBJECTIVE-MISSING-DONE` / `E-OBJECTIVE-QUEST-DONE` | An objective has no `done`, or has both `quest=` and `done=`. |
| `E-QUEST-TIER-MIX` | A subquest's `tier` differs from its parent's. Give both the same tier: a mixed tree locks for good once a new run resets only one side. |
| `E-GRAMMAR-NOT-ADMITTED` | The construct is not allowed in this kind, for example `<branch>` in an entry or a heading in a quest. |
| `E-BEAT-ATTR` | A beat key is malformed or has no `on`, `when` reads `scene.*`, `target` is on an untargeted occasion or outside its target domain (with a did-you-mean), an entry's `once` is not `run` or `user`, or `also` is not a bool, is on an entry, or is on a `select: all` / `sequence` occasion. So is a `<beat>` with no `id`, a `-` in its `id`, or no document `id:`, and an objective `target=` that is malformed or has no `on`. |
| `E-OCCASION-UNKNOWN` | A plugin declares occasions and this one is not among them. |
| `W-BEAT-PRIORITY-TIE` | Two beats on one `select: first` occasion, with the same (or no) target and equal priority, whose `when`s are not provably exclusive: file order picks the winner. |
| `W-BEAT-ONCE-RUN-USER` | A beat left at the default `once: run` whose `when` reads only user-tier state, so it replays every run. Write `once: run` if that is intended, or `once: user`. |
| `W-QUEST-HANDLER-DEAD` | `<on event="questFailed">` on a quest that can never fail: no `fail`, no required objective with a `by=` deadline, no required subquest that can fail, no parent quest. |
| `W-STAGE-ABSENT` | A line stages a character who has exited, or was auto-hidden by a `::bg` scene change, on some path to it. Each choice and `<match>` arm is followed separately, so an exit in one arm does not warn in its sibling; after the arms rejoin, a character is on stage only if every arm left them there. |
| `E-BEAT-UNREACHABLE` / `E-ARM-DEAD` | The condition can never hold. `check-project` also decides fact queries. Since 0.23.0 contradictions inside one `&&` (`run.n > 5 && run.n < 3`) count too. Under `check-project --wip`, a guard dead only for a relation nothing produces yet is a warning. |
| `E-ACCEPT-TARGET` | `::accept` names a quest that does not exist or that has a `start`. |
| `E-CONN-UNKNOWN-NODE` | `visited('…')` or `after` names no scene in the project. |
| `E-CONN-EPISODE-ID-DUP` / `E-QUEST-ID-DUP` | Two documents share a scene id or a quest id, or a bundle beat's canonical `<doc>.<beat>` id equals a scene id. |
| `E-DUP-VOICEKEY` | Lines with different text compile to one `voiceKey`, typically under a pinned `{speaker}-{code}` template. Use the default `{prefix}.{speaker}-{code}`, or give the lines distinct `code=`s. |
| `E-CAPABILITY-MISMATCH` | The project's documents resolve two capability snapshots (different profiles or scene-local `plugins:`), so it cannot compile as one. |
| `E-TEST-LORE` | A `*.test.yaml` names a lore document without `entry:`, `entries:`, or `beat:`. Name what it presents. |
| `E-TEST-FILE` | A `*.test.yaml`'s `file:` names no document. The test fails; the rest of the suite still runs. |
| `E-TRACE-BEAT` | `lute trace --beat <id>` names no bundle beat of the document (or the document is not lore). `lute run --beat` refuses the same with exit `2`. |
| `W-REWARD-DOUBLE-CREDIT` | A quest's `<on>` or objective body `::set`s the path its reward kind already `credits:`, so the player is paid twice. Drop the `::set` or the reward. |
| `W-FACT-GUARANTEED` | A fact guard is always true on every route, so it is redundant. |
| `W-BEAT-SHADOWED` | An earlier beat that is always eligible and never spent wins every time. |
| `W-ENTRY-REF-UNKNOWN` | `entry.<id>.read` or `entry.<id>.everRead` names an entry that nothing declares. |
| `E-LEGACY-CONTENT-SIGIL` · `W-WHEN-TEST-LITERAL` | Old syntax. `lute fix` rewrites it. |
| `E-PERSIST-REMOVED` | Delete `persist=` from the choice by hand; `into=` alone records the run fact. |

## Gotchas

**A quest with no `start` never activates by itself.** It is accept-driven: it stays `unset` until
a scene runs `::accept{quest="id"}`, or a mock or test lists it in `accepts:` (a play script can
seed its status with `quests:`). `::accept` on a quest that has a `start` is `E-ACCEPT-TARGET`. The
exception is a child named by `quest=`, which activates with its parent.

**`once` means different things on scenes and entries.** A scene beat defaults to `once: run`: it
plays at most once per run unless you write `once: false`. An entry without `once` repeats.
`once="run"` spends an entry until a new run resets `entry.<id>.read`, and `once="user"` spends it
for good (`entry.<id>.everRead`). A beat left at the default `once: run` whose `when` reads only
user-tier state replays every run (`W-BEAT-ONCE-RUN-USER`); you probably meant `once: user`, and
an authored `once: run` says the replay is intended.

**`after:` is structural and `when:` carries state.** `after:` reads only `visited`, `completed`,
and `active` with `&&` and `||`. It feeds the scene graph. A beat is eligible only when both hold.
On a `<quest>`, `after=` is graph metadata only and does not delay activation. To gate a quest on
a scene, put the condition in `start`: `start="visited('cafe.counter')"`.

**`::end` ends only its presentation in `lute play`**, not the playthrough. The step still settles
and the play goes on with the next step. To stop a play early, add a step `- end: true`: later
steps print as skipped and the play exits `0`.

**`visited()` covers the whole save.** A new run does not clear it. A single-file `check` never
decides it, `check-project` validates the id, `trace` and `test` need a `visited:` list, and a play
script that starts from a save lists them in top-level `visited:`.

**Engine-owned state comes from the harness, not from content.** A `::set` of an `owner: engine`
path is an error. `lute play` writes it with an `engine:` step, and trace and test with a mock's
`state:`:

```lute expect="E-ENGINE-OWNED-WRITE"
---
kind: scene
id: clock.cheat
state:
  run.day: { type: number, default: 1, owner: engine }
---

## Night

::set{run.day += 1}
@narrator: The day ends.
```

A quest's status is not an engine write either: an `engine:` step refuses `quest.*`. Seed a save's
quest status with the script's top-level `quests:`.

**A `::bg` scene change takes everyone off stage.** A character it auto-hid is recorded as exited,
so a later line by them is `W-STAGE-ABSENT` until an `::auto` shows them again:

```lute check
---
kind: scene
id: dock.night
enums:
  action: { members: [fade-in-up, fade-out-down], exits: [fade-out-down] }
  anchor: { members: [left, center, right], default: center }
---

## Dock

::auto{character="mira" action="fade-in-up"}
@mira: Over here.
::bg{location="street" time="night"}
::auto{character="mira" action="fade-in-up"}
@mira: Keep walking.
```

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
project from the directory you give them, and `lute test` plays its `*.play.yaml` files against
`--project` or the nearest `lute.project.yaml`.

**`trace`, `test`, and `play` derive: mock the premises, not the conclusion.** They load the
schema's `facts:` seeds and apply its Datalog rules over the mocked and asserted facts, as `lute run`
does. A mocked derived atom still counts, as one more seed. An unmocked base fact is false. A rule
guard over state the trace cannot decide leaves its conclusion unknown and halts the walk (exit 3);
in `lute test` that fails the test unless it declares `expect: { exit: incomplete }`. `derive: false`
(a mock, test, or play-script key) or `--no-derive` restores the 0.21 explicit world: no seeds, and
an unmocked derived atom is unknown.

**Directive attributes take bare refs to constant defs.** Write `::camera{zoom=@closeUp}`. The
quoted `zoom="@closeUp"` is the literal string `@closeUp` (`E-ATTR-TYPE` on a number attribute). A
def that reads state is `E-ATTR-DEF-DYNAMIC`.

**Attribute values use double quotes.** `label='"Hi."'` is `E-ATTR-QUOTE`. Write
`label="\"Hi.\""`; the label is `"Hi."`.

**Numbers are real numbers in `<match>`.** `is="1..9"` followed by `is="10.."` does not cover
`9.5`. Add an `<otherwise>`, or use open ranges that meet.

**Since 0.23.0 the checker proves more conditions false.** It reasons per path across an `&&`:
`run.n > 5 && run.n < 3`, `run.slot == 'a' && run.slot == 'b'`, and `x && !x` never hold, and an
`||` whose cases cover a path's domain always does. A project that checked clean on 0.22 may now
report `E-BEAT-UNREACHABLE`, `E-ENTRY-UNREACHABLE`, `E-ARM-DEAD`, `E-OBJECTIVE-UNSATISFIABLE`, or
`W-BEAT-PRIORITY-TIE`. The contradiction is real: fix the condition. `unset` counts as a value, so
on a path with no default `run.m > 5 && run.m < 3` stays undecided, while
`isSet(run.m) && run.m > 5 && run.m < 3` is false.

```lute expect="E-BEAT-UNREACHABLE"
---
kind: scene
id: late.shift
on: hubVisit
when: "run.day > 5 && run.day < 3"
state:
  run.day: { type: number, default: 1 }
---

## Late

@mira: You're early.
```

**`lute trace` stops at a taken `::next`.** The trace reports the jump and ends there, so the
content after the target `::mark` is not shown. `lute run` on the compiled artifact follows the
jump.
