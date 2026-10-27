---
title: Manifests & resolution
description: The plugin.yaml manifest and its export files, cross-cutting stampAttrs, declarative lowering to core staging records, and how installed plugins resolve deterministically into one capability snapshot.
---

A plugin is a directory whose entry is a single `plugin.yaml`. Its `exports` map names which sub-directories the loader reads; any directory not listed is ignored. Everything is declarative YAML behind one plugin id — consumers reference the id only.

## `plugin.yaml` (manifest entry)

```yaml
id: arcia.minigame          # REQUIRED — reverse-dotted, globally unique id
version: 0.1.0              # REQUIRED — the plugin's own semver
kind: capability           # REQUIRED — only "capability" is defined
depends:                   # OPTIONAL — { id, range } against other plugins
  - { id: lute.core, range: "^0.0.1" }
exports:                   # REQUIRED — which sub-directories the loader reads (list only what you ship)
  directives: directives/
  state: state/
  providers: providers/
  bridge: bridge/
  assetKinds: assetkinds/
  defs: defs/
  stampAttrs: stampattrs/
  enums: enums/
  frontmatter: frontmatter/
  events: events/
  occasions: occasions/
  rewardKinds: rewardkinds/
  cast: cast/
  hostTokens: hosttokens/
  lints: lints/
  docs: docs/
options:                   # OPTIONAL — typed activation options
  - { name: resultScope,  type: { enum: [scene, run] }, default: scene }
  - { name: allowedKinds, type: { list: { enum: [rhythm, puzzle, timing] } }, default: [rhythm, puzzle, timing] }
```

`depends[].range` is pinned to two forms only — caret (`^x.y.z`, pre-1.0 semantics) or an exact three-component version. Any other spelling is unsatisfiable by definition.

An option's value is the active profile's, otherwise its declared `default:`. An attribute typed `{ enumFromOption: <option> }` is checked against that effective value (0.41.1): with the manifest above and a profile that only enables the plugin (`plugins: { arcia.minigame: true }`), `kind="racing"` on an attribute typed `{ enumFromOption: allowedKinds }` is `E-BAD-ENUM` against `rhythm, puzzle, timing`, and so is an omitted attribute's declared `default:` outside the list. A profile that sets `allowedKinds: [rhythm]` narrows it to that list. An option with neither a profile value nor a `default:` leaves the attribute unchecked.

## Export files

Each export kind has a normative schema. All are typed by one small manifest type system (`bool` / `int` / `double` / `string`, `enum`, `list`, `record`, `map`, plus `enumFromOption`, `providerRef`, `slotId`, `assetKind`, `domain`, and shape refs; since dsl 0.26.0 a directive attribute may also be typed `{ entity: <kind> }`, [below](#engine-ids-typed-by-an-entity-kind); since 0.51 a directive attribute may be typed `text`, [below](#localizable-text-attributes-type-text)). State paths use **structured segments**, never `$name` interpolation. A *directive attribute* or `stampAttrs` entry cannot be typed `list`, `map` or `record` at its top level (since 0.49): an attribute value is written as one scalar, so no `.lute` value could satisfy it, and the plugin load fails with `E-PLUGIN-INVALID-DIRECTIVE`, naming the attribute (``stamp attribute `tags` `` for a stamp). Since 0.51 the message ends with this hint: `` … so no `.lute` value can satisfy a `list`, `map` or `record` (structured attribute values are not supported yet): where it fits, give each field its own attribute (a stamp can be scoped to its surfaces with `on:`), use a `text` attribute for player-visible text in several languages, or write a list of options as `<choice>` elements ``. For a stamp the `text` clause is omitted, because a stamp cannot be `text`. Structured authoring itself (enum-list metadata such as `::tagged{tags=…}`, language-keyed voice-asset maps) is deferred: 0.51 gives it no spelling, so use the scalar decompositions the hint names. Plugin `options:`, frontmatter keys, state shapes and bridge results keep these types, because YAML can write them.

- `directives/*.yaml` — `::name` directive declarations (see [Bridge](/plugins/bridge/)).
- `state/*.yaml` — reusable typed record shapes (`stateShapes:`) and structured path templates (`stateTemplates:`); one file may hold both (since 0.24.0; before, the second was dropped).
- `providers/*.yaml` — id registries resolved against a pinned snapshot.
- `bridge/*.yaml` — typed runtime bridge capabilities.
- `defs/*.yaml` — shared typed-CEL `@refs`.
- `assetKinds` (`assetkinds/*.yaml`) — asset-id segment templates (compose / query modes) with ordered `fallback` hooks.
- `stampAttrs` (`stampattrs/*.yaml`) — extra attributes on content lines, directives and (since 0.50) `<branch>` / `<hub>` / `<choice>`, cross-cutting or scoped to named surfaces with `on:` (below).
- `enums/*.yaml`, `frontmatter/*.yaml`, `docs/*.md` — named enum domains, plugin-owned meta keys, and hover docs.
- `events/*.yaml` — world events a quest's `<on event>` may name and `lute trace --event` fires.
- `occasions/*.yaml` — the engine moments [beats](/language/beats/) answer (dsl 0.21.0), each optionally raised for a target drawn from a project entity kind (dsl 0.22.0), presented as one winner, an offered list, or a sequence (dsl 0.23.0), and optionally gated by a [`raisedWhen`](#occasion-gates-raisedwhen) condition (dsl 0.27.0).
- `rewardKinds` (`rewardkinds/*.yaml`) — the closed set of `<reward kind>` values, with an optional [target contract](#reward-target-contracts), extra attributes, the state path a grant credits (dsl 0.23.0), and (since 0.50) the facts and quest accepts a grant applies ([Reward effects](#reward-effects)).
- `cast/*.yaml` — the speakers content lines may use, with display names (dsl 0.23.0).
- `hostTokens` (`hosttokens/*.yaml`) — names the engine substitutes into `{{name}}` interpolations at render time (since 0.50; [below](#host-interpolation-tokens-hosttokens)).
- `lints/*.yaml` — advisory [lint rules](/tooling/linting/), namespaced `<plugin-id>/<rule-id>`; excluded from the capability snapshot.

An export name outside this list is a load error. Every export file is also read strictly (0.24.0): a key the declaration does not know is `E-PLUGIN-KEY` at its line, with a did-you-mean, instead of being ignored (0.28.0 also reads `plugin.yaml` itself this way). This covers directives and their attrs, state, effects and bridge, state shapes and templates, providers, bridge capabilities, defs, enums, events, frontmatter, asset kinds, stamp attributes, reward kinds, occasions, cast entries and lints. Before, `{ selct: all }` loaded as `select: first` without a word:

<!-- lute-diagnostics unverified="the loader's E-PLUGIN-KEY line (crates/lute-manifest export_error); no single format! literal pins it; copied verbatim from lute check --project output" -->
```
./plugins/demo.pack/occasions/o.yaml:3:12: error [E-PLUGIN-KEY] `occasions.inbox` has no key `selct` — did you mean `select`? (its keys: select, target, description, judge, raisedWhen, payload, outsideRun)
```

A common way to get there is an unquoted description in a flow map. YAML ends an unquoted value at the first comma, so `{ select: all, description: Pick one, the player picks one }` leaves `the player picks one` as a key with no value. The error says so and spells the quoted form:

<!-- lute-diagnostics unverified="serde's unknown-field text with the flow-map hint appended by crates/lute-manifest, wrapped in the loader's E-PLUGIN-PARSE line; copied verbatim from lute check --project output" -->
```
./plugins/demo.pack/occasions/o.yaml:3:48: error [E-PLUGIN-KEY] `occasions.inbox` has no key `the player picks one` (its keys: select, target, description, judge, raisedWhen, payload, outsideRun); `the player picks one` has no value — in a flow map `{ … }` an unquoted value ends at the first comma, so the rest became a key; quote the description: `description: "Pick one, the player picks one"`
```

A plugin whose `plugin.yaml` parsed but one of whose exports did not is not loaded at all. The follow-up error for a profile that activates it says the plugin **failed to load** and points back at the parse error. Before 0.24.0 it told the author to install a plugin that was already installed:

<!-- lute-diagnostics unverified="assembled in crates/lute-manifest from the plugin id and a fixed suffix; copied verbatim from lute check --project output" -->
```
lute: E-PLUGIN-MISSING-ACTIVE: plugin `demo.pack` is activated by the project manifest but failed to load (see E-PLUGIN-KEY above); fix its package
```

The newest kinds, one minimal file each (every file carries one top-level key; the id is the map key or `name`/`id`):

```yaml
# events/world.yaml
events:
  - { name: combatEnd }
  - { name: npcSpoke }
```

```yaml
# occasions/game.yaml — a bare {} is select: first, untargeted, judged after
occasions:
  townVisit: {}
  examine:   { select: first, target: true }
  talk:      { select: first, target: { prefix: npc, entity: person } }
  bossDefeated: { select: first, target: { prefix: boss, entity: foe, members: [gatekeeper, warden] } }
  inbox:     { select: all, description: Letters waiting at the fountain }
  evening:   { select: sequence }
  dayEnd:    { select: first, judge: before }
  enter:     { select: first, target: { prefix: room, entity: room }, raisedWhen: "holds('canEnter', [occasion.target])" }
  summon:    { select: first, target: { prefix: hero, entity: hero }, payload: { copies: int } }
  title:     { select: first, outsideRun: true }
```

An occasion takes these keys, and no others: `select`, `target`, `description`, `judge`,
`raisedWhen`, `payload`, `outsideRun`, and (since 0.51) `raisedBy`.

An occasion's `select:` says what the engine presents when it is raised. `first` (the default) presents the single winning beat, plus any eligible [`also`](/language/beats/#side-remarks-with-also) beat after it. `all` offers every eligible beat and the player picks one. `sequence` (dsl 0.23.0) presents every eligible beat in selection order, such as an evening routine followed by the day's event (see [Composing an occasion](/language/beats/#composing-an-occasion)). A beat's `also: true` on an `all` or `sequence` occasion is `E-BEAT-ATTR`. When a world event of the same name is also declared under `events:`, every raise of the occasion fires that event after the beats, before the occasion judges its `on=` objectives (see [Occasions](/language/beats/#occasions)).

An occasion's `judge:` (dsl 0.24.0 §2) says when a raise judges the quest objectives that name it with `on=`. The default `after` judges them after the beats are presented, the order since 0.21.0. `before` judges them and settles the quests before the beats are decided, so an epilogue on `dayEnd` can read how its quests ended (`quest.<id>.failedBy`, see [Quests & scenes](/language/quests-and-scenes/)). Only the judging moves. The `<on>` handler bodies the raise answers, both the same-named `<on event>` handlers and the `questComplete` / `questFailed` handlers of the quests it settles, still run after the beats, so their narration follows the scene. Any other value is `E-PLUGIN-PARSE`. An occasion without `judge: before` keeps the `capabilitySnapshot` stamp it had.

An occasion's `target:` says what it is raised for. Absent or `false`, it is untargeted. `true` keeps its 0.21.0 meaning: the occasion is raised for some dotted id, and a beat's target is checked for shape only. A **domain** `{ prefix, entity }` (dsl 0.22.0) also closes the set: a target is `<prefix>.<member>`, where `entity` names an entity kind the *project* declares under `entities:` in its schema. The plugin supplies the prefix and the kind, and the project supplies the members (or declares the kind `open:` for engine-populated members). A beat target outside the domain is `E-BEAT-ATTR`, with a did-you-mean when a member is close, and so is every target of an occasion whose kind the document's schema does not declare (see [Beats](/language/beats/#target-domains)). A `target:` that is neither a bool nor a `{ prefix, entity }` map (with an optional `members:`) fails the plugin load with `E-PLUGIN-PARSE`.

A domain's optional **`members:`** list narrows it to a subset of the kind: `bossDefeated` above is raised only for `boss.gatekeeper` and `boss.warden`, even when the project's `foe` kind has more members. Which members a kind has is project vocabulary the plugin cannot see, so the checker judges the list: a listed member the kind does not declare makes every target of the occasion `E-BEAT-ATTR`, naming the stray member. On an `open:` kind the list is taken as written. The subset narrows every consumer of the domain alike: beat and objective targets, a [`lute play`](/tooling/play/) step's `target:`, `lute new scene --target`, and `lute calendar --target`. The loader itself rejects two shapes, as `E-PLUGIN-PARSE` on the occasion file:

- an empty list: ``occasion `bossDefeated`'s `target.members` is empty; list at least one member of entity kind `foe`, or drop `members` to draw targets from the whole kind``
- a member listed twice: ``occasion `bossDefeated`'s `target.members` lists `warden` more than once``

An occasion's **`raisedWhen:`** (dsl 0.27.0 §4) says when the engine may raise it at all: `enter` above is raised for a room only once the player may enter it. See [Occasion gates](#occasion-gates-raisedwhen) below.

An occasion's **`payload:`** declares the typed values each raise hands its beats, a map from field to a type of the manifest type system above (`copies: int`): a gacha pull says how many copies came with the hero drawn. Beats read them as `occasion.payload.<field>`, and so do an `<objective on=>` of that occasion and an `<on event=>` handler named like it (see [Occasion payloads](/language/beats/#occasion-payloads)). Every raise gives every field: a `lute play` step that leaves one out is a usage error naming it, `lute trace` and `lute test` halt incomplete on a line that reads a field the mock does not seed, and a clock `raise:` naming an occasion with a payload is `E-CLOCK-DECL`.

An occasion's **`outsideRun: true`** marks a moment that lives outside any run, a title screen or a gallery between runs: the engine raises it even after the project's [`terminal:`](/state/schemas/#the-end-of-the-game-terminal) holds, and the checker does not judge its beats under `!terminal`. The compiled artifact lists such occasions under `outsideRun`.

An occasion's **`raisedBy: declared`** (0.51) says the host raises the occasion only where a directive's or a reward kind's [declared `raises`](#declared-occasion-raises) names it. The only value is `declared`; leaving the key out keeps the default, where the host may raise the occasion at any time. A `raisedBy: declared` occasion that no `raises` entry can reach draws `W-OCCASION-NEVER-RAISED` at the scene or beat that answers it. An occasion the project [clock](/language/clock/) raises is raised outside any declared raise, so `raisedBy: declared` draws no anchor for it and `W-OCCASION-NEVER-RAISED` stays silent. Any other `raisedBy` value is `E-PLUGIN-PARSE`. The key changes `capabilitySnapshot` only for a snapshot that declares it.

Occasions are part of the capability snapshot, so they fold into `capabilitySnapshot`. Declaring a domain restamps, and so does adding or changing its `members:` list; an occasion that only ever says `target: true` or `false` keeps the stamp it had under 0.21.0. Likewise, `select: sequence` and `raisedWhen` change the stamp only for a snapshot that declares them.

```yaml
# rewardkinds/game.yaml
rewardKinds:
  XP: {}
  ITEM: { target: { provider: items }, attrs: [ { name: rarity, type: string } ] }   # `items` must be an active provider
  EMBERS: { credits: user.embers }
```

```yaml
# lints/style.yaml
lints:
  - id: too-many-choices
    target: scene
    when: "scene.choices > options.max"
    level: warn
    message: "scene has {scene.choices} choices (budget {options.max})"
    options: { max: 6 }
```

`enums/` is the third route a project gets its [content vocabulary](/language/vocabulary/) from, and the only one that is *capability* rather than project data: `lute.core` declares the vocabulary slots and exports an **empty** `enums`, so an engine or genre pack ships members to every project that activates it. Its entries take the same long form as an author's `enums:` block — a bare sequence is shorthand for `{ members: [...] }`, and `action` must carry `exits:` while `anchor` must carry `default:`.

### Occasion gates: `raisedWhen`

An occasion may declare **`raisedWhen:`** (dsl 0.27.0 §4), a CEL condition naming when the engine raises it. The engine raises the occasion only while the condition holds. `enter` above is the moment the player walks into a room, and the project's schema supplies the rooms and the relation the gate reads:

```yaml
# world.schema.yaml
entities:
  room: { members: [lobby, office] }
relations:
  canEnter: { args: [room], tier: run }
```

The gate may read **`occasion.target`**, the member the occasion is raised for. Raised for `room.office`, `occasion.target` is `office`, so the gate asks `holds('canEnter', ['office'])`. An occasion raised for no target has no member to read: a gate that reads `occasion.target` on an untargeted occasion is `E-BEAT-ATTR`, reported at the `on` of a beat answering it. Declare the occasion's `target:` as `{ prefix, entity }`, or drop the read.

A beat answering a gated occasion can only be presented while the gate holds, so the checker judges every beat and [entry beat](/language/beats/#entry-beats) that answers the occasion together with its gate, per member when the gate reads `occasion.target`:

- The gate's text is checked like any condition slot. An error in it is reported at the beat's `on`, prefixed ``occasion `enter`'s `raisedWhen: …`:``.
- A beat for which the gate is provably false is `E-BEAT-UNREACHABLE` (`E-ENTRY-UNREACHABLE` for an entry): the engine never raises the occasion for it. So is a beat whose `when` is provably false whenever the gate holds, since the engine raises no occasion otherwise. In whole-project `lute check` the gate's `holds(…)` is decided from the project's [fact analysis](/state/facts-and-datalog/#how-whole-project-lute-check-analyzes-relational-guards): when nothing in the project can assert `canEnter(office)`, every `enter` beat for `room.office` is never eligible.
- A beat that is never eligible this way takes no part in `W-BEAT-PRIORITY-TIE`.
- The relations the gate reads count as read for `W-RELATION-UNREAD`.

A project's [`terminal:`](/state/schemas/#the-end-of-the-game-terminal) condition closes every occasion the same way, once the game is over.

[`lute beats`](/tooling/cli/#beats) shows a gated ladder's gate in its header, `` · raisedWhen: … ``, and marks one that never holds: the header then ends `` · raisedWhen: … · gate never holds ``, or, on a kind ladder, names the members the gate never holds for: `` · raisedWhen: … · gate never holds for room.office ``. Under `--json` the ladder carries `raisedWhen`, and `gateNeverHolds: true` or `gateNeverHoldsFor: [<targets>]`.

[`lute play`](/tooling/play/) raises an occasion only as the engine would. An `occasion:` step that raises it while its gate is false halts the playthrough with `E-OCCASION-GATE` (exit 1): make the gate hold first, with an `engine:` write or an earlier step, or drop the step. A raise the clock makes (`raise.slot`, `dayStart`, `dayEnd`) while its gate is false is simply not made, and the step gets a note that the clock moved on without it.

The compiled artifact and `project.index.json` carry the gates at the top level as `gates: [{ occasion, raisedWhen: { cel, expr, authored? } }]`, each condition after `@def` expansion, and omit the key when no occasion declares one.

### Rewards that credit state

A reward is data: the engine grants it, and content never reads it. When a reward kind is a currency your own state tracks, `credits:` (dsl 0.23.0) names the state path a grant adds its amount to. The compiler stamps the path on every reward of that kind (`RewardEntry.credits` in the IR, omitted for a kind without one), and the engine owns that write, as it owns the grant itself. [`lute run`](/tooling/cli/#run) and [`lute play`](/tooling/play/) do the same for a **scalar** amount, and record the credit on the grant:

```
grant climb EMBERS 100 (credits user.embers = 100)
```

A range amount (`amount="10..20"`) is the engine's roll, so the toolchain grants it without crediting anything. Crediting by hand as well pays twice: a content `::set` of the credited path in one of the same quest's `<on>` handlers or objective bodies is `W-REWARD-DOUBLE-CREDIT`:

<!-- lute-diagnostics -->
```
./quests/climb.lute:11:11: warning [W-REWARD-DOUBLE-CREDIT] `::set` of `user.embers` in a handler of quest `climb`: its `<reward kind="EMBERS">` already credits `user.embers` when granted, so the player is paid twice — drop the `::set` or the reward
```

A kind without `credits:` hashes exactly as it did before 0.23.0, so adding the key to one kind restamps only the snapshots that declare it.

### Directive effects

A directive may declare the state it writes when it runs: `effects.writes`, a list of
`{ scope, path, value }`. [Typed bridge directives](/plugins/bridge/#the-directive-that-invokes-it)
shows a bridge directive writing its result slot. A passthrough directive may write too, such as a
scare that costs the player sanity:

```yaml
# directives/ward.yaml
directives:
  - name: fright
    attrs:
      - { name: amount, type: int }
    effects:
      writes:
        - { scope: run, path: [sanity], value: { op: decrement, by: { fromAttr: amount } } }
```

`scope` and `path` name the state path, `run.sanity` here. It may be an
[`owner: engine`](/state/state-model/#owner-engine) path: content cannot `::set` it
(`E-ENGINE-OWNED-WRITE`), but a directive's effect is a write the engine makes, and
[`lute play`](/tooling/play/) applies it where the directive runs. With `run.sanity` at 10,
`::fright{amount=2}` leaves it at 8.

`value` takes one of four shapes (dsl 0.27.0 §2):

| `value` | Writes |
|---|---|
| a bool, int, double, or string literal | that value |
| `{ fromAttr: <attr> }` | the value the call gives one of the directive's own attrs, or that attr's declared `default:`. A call that omits an attr with no default writes nothing. |
| `{ fromBridgeResult: <field> }` | a field of the bridge's answer; see [Typed bridge directives](/plugins/bridge/) |
| `{ op: increment \| decrement, by: <int> \| { fromAttr: <attr> } }` | adds `by` to the path, or subtracts it. An attr named in `by:` must be `type: int`. |

Any other value fails the plugin load with `E-PLUGIN-PARSE`, and the message lists the four
shapes. So does a `fromAttr` naming an attr the directive does not declare, with a did-you-mean.

#### Facts a directive asserts or retracts

Beside `writes:`, a directive's `effects:` may declare the facts it **asserts** and
**retracts** (dsl 0.27.0 §4). Every list is optional. An inventory the player carries items in —
`give` adds one item to the bag, `consume` takes that one item out:

```yaml
# directives/ward.yaml
directives:
  - name: give
    attrs:
      - { name: item, required: true, type: { entity: item } }
    effects:
      asserts: ["holding(@item)"]
  - name: consume
    attrs:
      - { name: item, required: true, type: { entity: item } }
    effects:
      retracts: ["holding(@item)"]
```

Each entry is one fact pattern, `relation(arg, …)`. An argument is a member, `true` or `false`,
or `@attr`: the value the call gives one of the directive's own attrs, or that attr's declared
`default:`. In `retracts:` an argument may also be `_`, which matches any value. Use it only for a
single slot: `retracts: ["holding(_)"]` beside `asserts: ["holding(@item)"]` makes `give` a pair
of hands that drops whatever the player held before, so on a bag it would empty the inventory at
every pickup. A pattern that does not parse, an `@attr` the directive
does not declare (with a did-you-mean), and a `_` in `asserts:` each fail the plugin load with
`E-PLUGIN-PARSE`. A call that leaves an `@attr` without a value, and with no default, writes
nothing for that fact.

The relation and its arguments are the project's, so the checker judges them at each call, as it
judges an `::assert` or `::retract` of the same fact: the relation must be declared, and the arity
and every argument's kind must fit it. `::give{item="brassKey"}` against a `holding(item)`
relation asserts `holding(brassKey)`; an error there is prefixed with the call and the fact, such as
`` `::give` asserts `holding(brassKey)` (its declared `effects.asserts`): ``. A derived or
`app`-tier relation is refused as it is for `::assert`, but a `reserved: true` relation may be
written this way: the engine applies a directive's effects itself, so this is the engine's own
write.

The compiled `plugin` record carries the resolved facts beside `effects`, as `retracts` and
`asserts` lists of `{ relation, args }` with each `@attr` already substituted (`"_"` for a
wildcard); both are omitted when empty, so an artifact without fact effects is unchanged.
[`lute play`](/tooling/play/), `lute test`, `lute trace` and `lute run` apply them after the call's
`writes`, first the retracts and then the asserts, through the same path as an `::assert` /
`::retract`. Play names the call on the transcript line:

```
assert holding(brassKey)  (effect of ::give)
```

Every analysis that reads what content asserts reads these calls as `::assert`s and `::retract`s
of their facts: the project's [fact analysis](/state/facts-and-datalog/#how-whole-project-lute-check-analyzes-relational-guards)
(what may and what must hold), `W-RELATION-UNREAD`, `E-FACT-EXCLUSIVE` (which suggests declaring the
`retracts:` the directive is missing), cast presence, `lute scenario --facts`,
[`lute scenario knowledge`](/tooling/overviews/#lute-scenario-knowledge) and `lute lore`. A
`holds('holding', ['brassKey'])` guard is therefore satisfiable once some call of `::give` can assert it,
with no `::assert` written anywhere.

### Engine ids typed by an entity kind

An attribute that names something the engine owns, such as a bag item, a species, or a shop's stock list, may be typed by an **entity kind** the project declares (dsl 0.26.0 §2.5):

```yaml
# directives/engine.yaml
directives:
  - name: give
    attrs:
      - { name: item, required: true, type: { entity: bagItem } }
      - { name: qty,  type: int, default: 1 }
```

The plugin names the kind and the project lists its members, the same split as an occasion's [target domain](/language/beats/#target-domains). The kind lives in a schema every document imports, typically one lead-owned file of the ids the engine ships:

```yaml
# schema/items.schema.yaml
entities:
  bagItem: { members: [potion, superPotion, goodRod, nugget] }
```

A value outside the kind is `E-BAD-ENUM`, with a did-you-mean. An attribute typed by a kind that no schema of the document declares is `E-DOMAIN-UNKNOWN` at every use:

<!-- lute-diagnostics unverified="verbatim lute check --project output; the E-DOMAIN-UNKNOWN message spells the type with escaped braces, which the scraper does not read back as a literal" -->
```
./scenes/mart.lute:9:14: error [E-BAD-ENUM] `potoin` is not a member of entity kind `bagItem` (attribute `item` of `::give`) — did you mean `potion`?
./scenes/mart.lute:10:22: error [E-DOMAIN-UNKNOWN] attribute `species` of `::encounter` is typed `{ entity: monster }`, but `monster` is not a declared entity kind — declare it under `entities:` in a project schema this document reaches through `uses:`
```

A value that reaches the attribute through a component param is checked at the `::use` argument, or at the param's `default:` (see [Engine ids through a param](/language/components-and-extends/#engine-ids-through-a-param)). `lute refs --project <DIR> --attr give.item` lists every value with the documents and lines that use it, so a lead sees who gives what before merging several authors' work.

### Localizable text attributes: `type: text`

Since 0.51 a directive attribute may be typed **`text`**: player-visible text that can be translated, so it is addressed and localized the way a dialogue line is.

```yaml
directives:
  - name: banner
    attrs:
      - { name: title,    required: true, type: text }
      - { name: subtitle, type: text }
      - { name: style,    type: { enum: [toast, card] } }
```

`text` is admitted **only** as the type of a directive attribute (`attrs:`) of a directive that compiles to a `plugin` record: one with no `lower:` of any kind and no `bridge:`. The plugin load fails with `E-PLUGIN-PARSE`, at the attribute, for:

- `text` on a directive with `lower:` (a staging record or a builtin) or with `bridge:`: `` `title` of `::banner` is `text`, but `::banner` lowers to the `actor` record, which carries no localized text — declare it `string`, or drop `lower:` so the call stays a `plugin` record ``;
- `text` with a `default:`, because an untranslatable source text has nothing to translate;
- `text` anywhere else the type grammar is read: a stamp (`stampAttrs`, refused at assembly beside the structured-type refusal), plugin `options:`, an occasion `payload:`, a reward kind's `attrs:`, bridge results, or `{ list: text }`. Each place keeps its own invalid-type diagnostic, which names the one place `text` is admitted. A [host token](#host-interpolation-tokens-hosttokens) cannot be `text` either: its type is `string`, `int` or an enum;
- a directive with a `text` attribute that also declares an attribute named `code`, because `code` is the identity attribute of such a directive.

A stamp scoped with `on: [banner]` and named `code` is shadowed by that core attribute: `E-PLUGIN-RESERVED-STAMP-ATTR`. A cross-cutting stamp named `code` keeps applying to other directives; on a directive with a `text` attribute the core `code` is matched first, as a line's own `code` is.

In a document, a `text` value is a quoted string in the choice-label grammar: `{{…}}` interpolations of state paths, `@def`s and host tokens, with their modifiers; inline markup is not parsed. `\{{` writes a literal `{{` (since 0.52, #211), as in a choice label. An `@ref` value is `E-ATTR-TYPE`: `` `title` of `::banner` is localizable `text`, written as a quoted string; a `@def` value has no translation of its own ``. The directive then takes `code=` (a string) as a content line does, on any directive with at least one `text` attribute; on any other directive `code` stays `E-UNKNOWN-ATTR`, and the message says why.

```lute
::banner{code="0010" title="Welcome to the hall, {{userName}}" style="card"}
```

The localization id of each `text` attribute is `{prefix}~{tag}~{code}~{attr}` (`intro~banner~0010~title` above, in `scenes/intro.lute`), and a `code` containing `~` is `E-LINE-CODE-RESERVED` (`` `code="a~1"` contains `~`, which is reserved for directive text identities ``). Two calls of one directive with `text` attributes sharing a `code` in one scope are `E-DUP-LINE-CODE`, and two units of the project that compile to one `lineId` are `E-LINE-ID-DUP` (`lute check --project`). `lute tag` writes the missing `code=` values (for a component, under the snapshots of the documents that use it; users that disagree on its text directives are `E-COMPONENT-TEXT-DIRECTIVE`), `lute loc export` lists one `directive` row per `text` attribute, and `lute loc import` and `lute loc report` take them. The compiled `plugin` record keeps the source string in `fields.<attr>` and adds a `localized` map ([Runtime contract](/tooling/runtime-contract/#051-additions)); a host without localization reads `fields` as before. A directive with no `text` attribute compiles byte-identically to 0.50, and `text` changes `capabilitySnapshot` only for a snapshot that declares it. (The whole-project quest-reference pass does not read `text` values yet; #212.)

### Reward target contracts

A reward kind's `target:` says what a `<reward target="…">` value names, and since dsl 0.26.0 §2.5 the checker holds rewards to it:

```yaml
# rewardkinds/league.yaml
rewardKinds:
  MONEY: { credits: run.money }
  ITEM:  { target: { entity: bagItem, required: true } }
  TM:    { target: { entity: tm, required: true } }
```

| Contract key | Meaning |
|---|---|
| `entity: <kind>` | the target is a member of that project entity kind; a non-member is `E-REWARD-TARGET` with a did-you-mean |
| `provider: <name>` | the target is an id of that provider's catalog; the provider must be active (the plugin load fails otherwise), an unknown id is `E-REWARD-TARGET`, and a stale snapshot only warns (`W-CATALOG-STALE`) |
| `required: true` | a reward of the kind without `target=` is `E-REWARD-TARGET`; without it a target may be omitted |
| `forbidden: true` | the kind takes no target: a `<reward>` of it with `target=` is `E-REWARD-TARGET` at the `target=` attribute (since 0.49) |

A contract names `entity:` or `provider:`, not both: one with both fails the plugin load (`E-PLUGIN-PARSE`). A kind without `target:` takes any `target=` or none, as before. A kind that takes no target says so with `target: { forbidden: true }`, which stands **alone**: combined with `provider`, `entity` or `required` it fails the plugin load, because a kind that takes no target cannot also name an id space or require one. For `COINS: { target: { forbidden: true } }`, ``<reward kind="COINS" target="x"/>`` is `E-REWARD-TARGET`: `` `<reward kind="COINS">` takes no `target=`: its reward kind declares `target: { forbidden: true }` — remove the attribute ``. Declarations without `forbidden: true` keep their `capabilitySnapshot`; setting it changes the snapshot. The ITEM contract above catches both mistakes a hand-maintained list lets through:

<!-- lute-diagnostics -->
```
./quests/dex.lute:11:3: error [E-REWARD-TARGET] `<reward kind="ITEM">` needs a `target=`: the reward kind declares `target: { required: true }`
./quests/dex.lute:12:31: error [E-REWARD-TARGET] `goodRd` is not a member of entity kind `bagItem`, which `<reward kind="ITEM">` targets — did you mean `goodRod`?
```

Only a quoted `target="…"` reaches the artifact: a reward's target is static data the engine grants against. An `@ref` (`target=@n`) or a bare `target` is therefore `E-REWARD-ATTR` at the value (since 0.49), under every contract and for a kind with none: ``<reward target=…> takes a quoted id, e.g. `target="potion"`: `target=@n` is a reference, and a reward's target is static data the engine grants against``. A bare `target` says `a bare `target` has no value` instead. `required: true` does not also report the written target as missing, and an `entity:` or `provider:` contract adds nothing: the refusal is the one diagnostic. Under `forbidden: true`, `E-REWARD-TARGET` stays the only report.

Which form to use for ids the engine owns: an **entity kind** when the ids can be listed in the repository and reviewed there (one lead-owned schema such as `schema/items.schema.yaml` above), a **provider** when the engine's catalog is the source of truth and the project checks against a pinned snapshot of it (see [Providers & catalog](/tooling/providers-and-catalog/)). Either way, type the directive attribute and the reward kind by the **same** kind or provider, so `::give{item}` and `<reward kind="ITEM" target>` share one id space, and `lute refs --project <DIR> --attr give.item --reward ITEM` lists both (a reward without a target as `(no target)`).

### Reward effects

Since 0.50 a reward kind may declare **`effects:`**: the facts its grant asserts or retracts and the quests it accepts. Every list is optional:

```yaml
# rewardkinds/league.yaml
rewardKinds:
  BADGE:       { target: { entity: person, required: true }, effects: { asserts: ["met(@target)"] } }
  REVOKE:      { target: { entity: person, required: true }, effects: { retracts: ["met(@target)"] } }
  OFFER_QUEST: { target: { required: true }, effects: { accepts: ["@target"] } }
```

`effects` takes `asserts`, `retracts`, `accepts` and (since 0.51) [`raises`](#declared-occasion-raises), and no other key. An empty `effects: {}` is `E-PLUGIN-PARSE` ("declares no effect"; the message lists `raises:` among the keys), while `effects: { raises: [...] }` alone is a valid block. Each `asserts` / `retracts` entry is the same fact pattern a [directive's effects](#facts-a-directive-asserts-or-retracts) use, `relation(arg, …)`.

- **`writes` is not admitted.** A reward's state write is `credits:`, so `effects.writes` is `E-PLUGIN-KEY`, and its message adds `` — a reward's state write is `credits:` ``.
- **Pattern arguments** are members, `true` / `false`, `@target`, `@<attr>` for an attribute the kind declares (since 0.52, [below](#reward-kind-attributes)), and `_` (in `retracts` only). Any other `@name` is `E-PLUGIN-PARSE`, with a did-you-mean: `` reward kind `BADGE` effect `badge(@target, @rarty)` reads `@rarty`, but the kind declares no such attribute (declared: target, rarity); did you mean `@rarity`? ``.
- **`accepts`** entries are `@target` or a literal quest id (an identifier). Anything else is `E-PLUGIN-PARSE`.
- A kind with `target: { forbidden: true }` whose effects read `@target` is `E-PLUGIN-PARSE`: the effect could never bind.
- `@target` is bound to the reward's quoted `target=`. A `<reward>` with no `target=` applies no effect for a pattern that reads it, with no extra diagnostic; a kind with `target: { required: true }` already reports `E-REWARD-TARGET` there.

The checker judges each `<reward>` of such a kind with its `@target` and its attributes bound. A fact pattern is held to exactly what an `::assert` / `::retract` of it is (relation declared, arity, argument kinds and members), with the same codes, at the reward's `kind=`; each message is prefixed `` `<reward kind="BADGE">` asserts `met(mira)` (its kind's `effects`): ``. An `accepts` id is judged by `::accept`'s rule: `E-ACCEPT-TARGET` for an unknown id (with a did-you-mean), for a quest that has `start`, and for an auto-activating child. It is reported at the reward's `target=` value when the id came from `@target`, otherwise at `kind=`: `` `<reward kind="OFFER_QUEST" target="secnd">` accepts quest `secnd` (its kind's `effects.accepts`), but no quest in the project declares that id — did you mean `second`? ``. What the effects mean at the grant, and how analyses read them, is in [Quests & scenes](/language/quests-and-scenes/). A kind without `effects:` hashes as it did before.

### Reward-kind attributes

A reward kind's **`attrs:`** list follows the directive attribute declaration rules
(`{ name, required?, type, default? }`). Since 0.52 (#201) a `<reward>` of the kind authors them,
and its `effects` read them as `@<attr>`:

```yaml
# rewardkinds/r.yaml
rewardKinds:
  BADGE:
    target: { entity: person, required: true }
    attrs:
      - { name: rarity, type: { enum: [common, rare, gold] }, default: common }
    effects: { asserts: ["badge(@target, @rarity)"] }
```

**Load faults.** Each of these is `E-PLUGIN-PARSE` at the entry, and the plugin fails to load:

<!-- lute-diagnostics unverified="verbatim lute check --project output; ManifestError::RewardAttr (crates/lute-manifest/src/validate.rs) writes the `reward kind … declares attribute …` head and each fault's tail with separate write! calls, so no single format! literal spans a line" -->
```
./plugins/acme.stage/rewardkinds/r.yaml:5:9: error [E-PLUGIN-PARSE] reward kind `BADGE` declares attribute `amount`, which `<reward>` already has (`id`, `kind`, `target`, `amount`, `when`, `outcome`) — give it another name
./plugins/acme.stage/rewardkinds/r.yaml:6:9: error [E-PLUGIN-PARSE] reward kind `BADGE` declares attribute `rarity` as `string` with `default: 3`, which is not of type `string` — give a default of the declared type
./plugins/acme.stage/rewardkinds/r.yaml:7:9: error [E-PLUGIN-PARSE] reward kind `BADGE` declares attribute `tags` as `list<string>`; an attribute value is written as one scalar (string, number, bool, an enum or entity member, a provider or asset id), so no `.lute` value can satisfy a `list`, `map` or `record` (structured attribute values are not supported yet): give each field its own attribute
```

A name declared twice is refused too (`` reward kind `BADGE` declares attribute `rarity` more than once — keep one declaration ``), and `type: text` stays refused when the file parses.

**In a document.** The `<reward>` closure is its six keys plus the kind's attributes. Each value is
a quoted literal checked by the directive attribute checker, with the owner written
`` <reward kind="BADGE"> `` (`E-ATTR-TYPE`, `E-BAD-ENUM`, `E-UNKNOWN-ID` and the entity, provider and
asset checks); a `required: true` attribute left out is `E-MISSING-ATTR`; an `@ref` value is
`E-REWARD-ATTR`; and `E-UNKNOWN-ATTR` lists the kind's attributes after the six. A kind that
declares none, or a project with no plugin, keeps the six-key closure and its message. See
[Quests & scenes](/language/quests-and-scenes/#reward-kind-attributes).

**Binding and artifact.** An effect's `@<attr>` is the reward's authored value, else the declared
`default:`; with neither it is unbound and the effect is not applied, as an unbound `@target` is.
The compiled reward entry carries `attrs`, written last and only when non-empty (each authored
attribute and each `default:` not authored, typed as a `plugin` record's `fields`), and the `grant`
record copies them onto its `reward`. `attrs` is descriptive: it adds no semantic id, and a host
reads it by the reward kind's contract, which `capabilityVersion` already pins.

### Declared occasion raises

Since 0.51 a directive's `effects:` and a reward kind's `effects:` may declare the occasions the **host** raises in answer to the call or the grant. Lute does not execute a raise: raising occasions is a host duty. A declared raise is a promise about what the host does, and it lets the checker connect the call to the scene or beat that answers the occasion.

```yaml
# plugins/acme.stage/occasions/o.yaml
occasions:
  cutscene: { select: first, target: { prefix: scene, entity: cutsceneId }, raisedBy: declared }
```

```yaml
# plugins/acme.stage/directives/d.yaml
directives:
  - name: playScene
    attrs:
      - { name: scene, required: true, type: { entity: cutsceneId } }
    effects:
      raises: [{ occasion: cutscene, target: "@scene" }]
```

```yaml
# plugins/acme.stage/rewardkinds/r.yaml
rewardKinds:
  CUTSCENE: { target: { entity: cutsceneId, required: true }, effects: { raises: [{ occasion: cutscene, target: "@target" }] } }
```

```lute
--- # scenes/chapter2.lute
kind: scene
id: chapter2
on: cutscene
target: scene.chapter2
---
```

- **`effects.raises`** is a non-empty list of `{ occasion, target? }` entries, and no other keys. It sits beside `writes`, `asserts` and `retracts` on a directive, and beside `asserts`, `retracts` and `accepts` on a reward kind. A reward kind that declares only `raises` keeps the `lute.quest.rewardEffects/1` semantic id out of `requiredSemantics`.
- **`target`** is one of: absent (only for an occasion declared without a target); `@<attr>` on a directive, naming one of its declared attributes; `@target` on a reward kind, the reward's `target=`, or (since 0.52) `@<attr>`, one of the kind's declared [attributes](#reward-kind-attributes); or a literal target written whole (`scene.chapter2`), as a beat writes it. A kind with `target: { forbidden: true }` cannot raise for `@target`.
- **`raisedBy: declared`** on the occasion makes the host raise it only through declared raises (see [the occasion keys](#export-files) above). Without it, a declared raise of the occasion is recorded in the artifact and checked, but draws no graph edge.

**Load faults** (`E-PLUGIN-PARSE`, at the entry): `raises: []`; an entry that is not a mapping with `occasion` and an optional `target`; a `target` that is neither `@<name>` nor a dotted target; on a directive, an `@<name>` that names no declared attribute; on a reward kind, an `@<name>` other than `@target` that names none of the kind's attributes, or `@target` on a kind with `target: { forbidden: true }`; an unknown `raisedBy` value.

A directive whose lowering has no carrier for raises, which is any `lower:` (a staging record or a builtin), cannot declare `effects.raises`. It is `E-PLUGIN-INVALID-DIRECTIVE`, at the `raises` key: `` directive `::playVideo` lowers to the `video` record, which carries no effects (only a `plugin` record does), so its `effects.raises` could never reach the host — drop `lower:` so the call stays a `plugin` record, or raise the occasion from a passthrough directive ``. This is the boundary `effects.writes` already has on record lowerings.

**Assembly.** Occasions may come from another plugin, so the occasion is looked up when the plugins assemble. A refused entry is `E-PLUGIN-RAISES` and is dropped, and its siblings still land:

| Fault | Message shape |
|---|---|
| the occasion is not in the snapshot | `` directive `::playScene` (`acme.stage`) raises occasion `cutscne`, which no active plugin declares (a plugin raising another plugin's occasion lists it in `depends:`) — did you mean `cutscene`? `` (when the occasion exists in an installed but inactive plugin, the message names that plugin instead) |
| a `target` on an untargeted occasion, or none on a targeted one | `` … raises `cutscene` without a target, but `cutscene` is raised for a target (`scene.<cutsceneId>`) — add `target: "@<attr>"` `` |
| for a domain occasion `{ prefix, entity }`, an `@attr` whose declared type is not `{ entity: <entity> }` (on a reward kind, `@target` whose `target:` contract is not `{ entity: <entity> }`, or an attribute of the kind declared otherwise) | `` … raises `cutscene` for `@scene`, which is typed `string`; the target of `cutscene` is a `cutsceneId` member, so type the attribute `{ entity: cutsceneId }` `` |
| a literal target outside the occasion's domain shape (`<prefix>.<member>`; `members:` when listed) | `` … raises `cutscene` for `room.office`, but its targets are `scene.<cutsceneId>` `` |

**Per call and per grant.** The project pass (`lute check --project`) reports `E-RAISES-TARGET` at the call's attribute value for an `@attr` target, at the directive head for a literal, and at the reward's `target=` (or `kind=`). A resolved target is the call's authored value, else the attribute's declared `default:`; only a target with neither is unbound, and an unbound target suppresses the raise with no extra diagnostic (`required` already reports). For each resolved target:

- a domain target `<prefix>.<value>` that is not a member of the project's kind (or of the occasion's `members:`) is `E-RAISES-TARGET`, with a did-you-mean;
- a target that **no** scene or beat answers (no document with `on:` that occasion and that target, or no `target:`) is `E-RAISES-TARGET`: `` `::playScene{scene="chaptr2"}` raises `cutscene` for `scene.chaptr2`, but no scene or beat answers it — did you mean `scene.chapter2`? ``.

An `@def` attribute value folds to a constant like every attribute value, so it is a static target, and a call inside a component body is resolved at each `::use` with its arguments bound. The only dynamic target is a value that reads `occasion.target`, in a call inside a beat. It is not judged here; in the graph it stands for every scene or beat that answers the occasion.

**What the host does.** After the presentation that ran a directive call ends (`presentationEnd`), the host raises each listed occasion for its resolved target, in record order and list order, as a `raiseOccasion` input once the runtime is idle. After the output that carries a reward's `grant` record has been applied, it does the same for the reward kind's raises. The occasion's own gate (`raisedWhen`) may still refuse a raise: that refusal is the host's to report, so Lute reads a declared raise as *possible*, never as guaranteed. It adds no fact, write or Must claim anywhere, and every analysis reads it as a may-edge. See [the scene graph](/connectivity/scene-graph/#edges-that-declared-raises-draw) and the host duties in [Custom hosts](/player/custom-hosts/#declared-raises).

The compiled `plugin` record and the `grant` record carry the resolved `raises` list (`{ occasion, target? }`), with `@attr` bound to the attribute's value and the domain prefix applied (`scene.chapter2`); a target that reads `occasion.target` is written `scene.occasion.target` with `"dynamic": true`, and the runtime binds it to the member the beat runs for. The field is omitted when empty, so a manifest without `raises` leaves every artifact and `capabilitySnapshot` as it was. The [W-OCCASION-NEVER-RAISED](/connectivity/reachability/#occasions-that-nothing-raises) warning flags a scene or beat that answers a `raisedBy: declared` occasion no declared raise reaches.

### Cast

```yaml
# cast/harbor.yaml
cast:
  mira:  { name: Mira }
  oskar: { name: "Oskar Lind", present: "run.oskarAboard", emotions: [calm, grim] }
  vesna: {}
```

A `cast` export (dsl 0.23.0) declares the speakers an engine pack is built for: each map key is a speaker id, and every field is optional. `name` is the display name. Since 0.24.0 an entry may also declare `present:`, the condition under which the character is with the player, `emotions:`, the `emotion=` values their lines may use, and `assume: true`, which lets the presence check take the engine's `reserved:` facts that `present:` negates as absent. A line whose guards do not imply `present:` is `W-CAST-ABSENT`, and an `emotion=` outside `emotions:` is `E-BAD-ENUM` (see [The cast](/language/dialogue-and-cast/#the-cast)). Any other key is `E-PLUGIN-PARSE`. Once any cast is declared, by a plugin or by a schema document's `cast:` key, a content line whose speaker is outside it is `E-CAST-UNKNOWN` with a did-you-mean, and so is an `::actor{character}` or `::camera{focus}` that names someone outside it. The plugin casts and the schema casts a document imports are unioned, and a plugin's entry wins an id they share, because it carries the engine's display name. Two active plugins declaring the same id is `E-PLUGIN-DUP-ACROSS` at assembly. A non-empty cast folds into `capabilitySnapshot`; a plugin that exports none leaves the stamp alone, and a cast that uses none of the 0.24.0 keys keeps the stamp it had under 0.23.

Since dsl 0.26.0 §2.8 an entry may also say `sharedName: true`: its `name` is a role name several speakers share on purpose, such as a villain team's rank and file (`grunt1: { name: Eclipse Grunt, sharedName: true }`). Whole-project `lute check` and `lute lint` report two speakers shown under exactly the same name as the advisory `W-DISPLAY-NAME-DUP`, and a `sharedName` entry is not counted (see [Display names shared by two speakers](/language/dialogue-and-cast/#display-names-shared-by-two-speakers)).

## Cross-cutting attributes (`stampAttrs`)

Ordinary attributes are declared per directive. An engine that tags *every* record with the same key — an analytics id, an experiment bucket, a bonus hook — had no declaration site for one, and could never put it on a content line at all. `stampattrs/*.yaml` is that site:

```yaml
stampAttrs:
  - { name: bonusId,    type: string }
  - { name: bonusScore, type: int }
```

Entries are ordinary `AttrDecl`s — the same `{ name, required?, type, default? }` shape a directive attr uses. An entry without `on:` is **cross-cutting**: admissible on **every** directive that has a stamp carrier *and* on content lines (`@speaker{…}: text`), on top of that surface's own attributes, and never on `<branch>`, `<hub>` or `<choice>`. An entry with `on:` is **scoped** to the surfaces it names (since 0.50, [below](#scoping-a-stamp-to-surfaces-on)). Resolution is strict: the surface's own declarations win, then `stampAttrs`, then `E-UNKNOWN-ATTR`. Value typing rides the existing attribute path, so a mistyped one is a plain `E-ATTR-TYPE` / `E-BAD-ENUM` — no new rules.

`::sfx{sound="chime" bonusId="b-02"}` followed by `@marina{code="0010" bonusId="b-01" bonusScore="7"}: Welcome back.` compiles to two records that each carry the attribute in **one nested `stamps` object** (since 0.49), after the core keys (`kind`, `family`, `position`, `timing`) — never flattened beside the record's own keys and never in its `fields`:

```json
{ "kind": "sfx", "family": "staging", "position": "001-0100", "sound": "chime", "stamps": { "bonusId": "b-02" } }
{ "kind": "line", "family": "content", "position": "001-0200", "role": "dialogue",
  "speaker": "marina", "text": "Welcome back.", "lineId": "marina.s01ep01.marina_0010",
  "voiceKey": "marina.s01ep01.marina-0010", "stamps": { "bonusId": "b-01", "bonusScore": 7 } }
```

Each entry's JSON value is typed by the declaring plugin's attribute declaration, as before. The object is omitted when no stamp was authored, so a record without one is unchanged. Because the stamps live under their own key, a plugin stamp named like a key of its carrier (`tag`, `speaker`, `text`, `label`, `name`, `id`, …) can no longer collide with it; before 0.49 such a stamp wrote that key twice. The same `stamps` object reaches the host beside, and never merged into, the `attrs` of a plugin event record and of a [bridge](/plugins/bridge/) call.

An **unauthored** stamp attribute is not injected — not even when its declaration carries a `default`. Absent means absent, so declaring a cross-cutting vocabulary and authoring none of it leaves the artifact byte-identical. The declaration is not free, though: `stampAttrs` participates in `capabilitySnapshot`, because a changed cross-cutting vocabulary is a changed capability surface and an engine must be able to refuse the mismatch.

### Scoping a stamp to surfaces: `on:`

Since 0.50 an entry may carry one more key, **`on:`**, a non-empty list of surface names. A scoped stamp is admissible only on the surfaces it names, and may be `required`:

```yaml
# plugins/acme.stage/stampattrs/columns.yaml
stampAttrs:
  - { name: voiceAssetId, type: { providerRef: voice }, required: true, on: [line] }
  - { name: assetName,    type: string, on: [bg, actor] }
  - { name: menuType,     type: { enum: [plain, timed, minigame] }, on: [branch, hub] }
  - { name: optionKey,    type: string, required: true, on: [choice] }
  - { name: cue,          type: string }                          # cross-cutting, as before
```

```lute
<branch id="route" prompt="Which way?" menuType="plain">
  <choice id="goLeft" text="Left" optionKey="L1">
    @mira{voiceAssetId="VO.mira.0010"}: Left it is.
  </choice>
</branch>
```

The surface vocabulary is closed:

| Surface name | Names |
|---|---|
| `line` | every content line (`@speaker{…}: text`), including lines in component, quest, objective, `<on>`, entry and beat bodies |
| `branch`, `hub` | the element of that name |
| `choice` | every `<choice>`, in a `<branch>` or a `<hub>` |
| a directive tag | a directive of the resolved snapshot that has a stamp carrier, that is, not `jump`, `label` or `clear` |

`line`, `branch`, `hub` and `choice` always mean the core surface (`branch`, `hub` and `choice` are already reserved plugin directive names, `E-PLUGIN-RESERVED-NAME`), so a plugin directive tagged `line` cannot be named in `on:`. The order of `on:` is not significant.

**Without `on:`** the entry keeps its meaning exactly: it is admissible on every directive that has a stamp carrier and on every content line, and it is **not** admissible on `<branch>`, `<hub>` or `<choice>`. Those three stay closed to a cross-cutting stamp; only a stamp whose `on:` names them is admissible there.

**Load faults** (per package, `E-PLUGIN-PARSE` at the entry):

| Fault | Message |
|---|---|
| `on:` is not a list of strings, or is empty | `` stamp attribute `X` has an empty `on:` — list the surfaces it applies to, or omit `on:` for every directive and content line `` |
| a surface repeated in `on:` | `` stamp attribute `X` lists `bg` twice in `on:` `` |
| `required: true` without `on:` | `` stamp attribute `X` is `required: true` but has no `on:`; a stamp required on every record is not a column — name its surfaces with `on: […]` `` |

A cross-cutting stamp cannot be `required`: there is no single place to report it missing. Before 0.50 such a `required: true` was a silent no-op. A directive attribute with `on:` stays `E-PLUGIN-KEY`; `on:` belongs to `stampAttrs` entries only.

**Uniqueness within a package.** Two entries may share a name only if both have `on:` and their surface sets are disjoint, so one name may be typed differently per surface (`assetName: string` on `bg`, `assetName: int` on `actor`), and the IR serializes each with its own type. Any overlap (an entry without `on:` overlaps every entry) is `E-PLUGIN-DUP-ID`, naming the shared surfaces: `` stampAttr `assetName` is declared twice on the surface `bg` — give each entry disjoint `on:` surfaces ``.

**Assembly.** Once the active snapshot's directives are known, each entry is checked in this order. A refused entry is dropped and its siblings still land:

1. The core stamp reservation ([below](#reserved-stamp-keys)): `E-PLUGIN-RESERVED-STAMP-ATTR`.
2. Each `on:` name resolves to a surface. A name that is none of the four core surfaces and no directive of the snapshot is **`E-PLUGIN-STAMP-SURFACE`**: `` stamp attribute `assetName` (`acme.stage`) names `bgg` in `on:`, which is not `line`, `branch`, `hub`, `choice` or a directive of the resolved profile — did you mean `bg`? ``. A directive without a stamp carrier draws the same code: `` … names `jump` in `on:`, but `::jump` emits no stamped record `` (it is one of `jump`, `label`, `clear`). A plugin that scopes a stamp to another plugin's directive declares that plugin in `depends:`, so that the directive is in every profile that activates it; the message says so when the tag exists in an installed but inactive plugin.
3. **Shadowing.** A scoped entry whose name is an attribute the named surface already owns is `E-PLUGIN-RESERVED-STAMP-ATTR`: `` stamp attribute `prompt` (`acme.stage`) is scoped to `branch`, but `prompt` is an attribute of `<branch>` itself, so the stamp could never be read ``. A surface's own attributes are: for a directive, its declaration's `attrs` (plus `when` on `::jump`); for `line`, the content line's known attributes (including `wait`) and the guard `when`; for `branch`, `id`, `prompt`, `timeout`, `default`; for `hub`, `id`, `prompt`; for `choice`, `id`, `text`, `when`, `into`, `value`, `once`, `exit`, and the renamed or removed `label`, `as`, `persist`. A cross-cutting entry keeps its earlier rule: the surface's own declaration wins and the entry is not refused.
4. **Uniqueness across plugins.** Two plugins' entries with the same name and overlapping surfaces are `E-PLUGIN-DUP-ACROSS`, naming both plugins and the shared surfaces. Disjoint entries from two plugins are legal.

**Checking.** An attribute `k` on a surface resolves strictly, first match wins: the surface's own attributes; for a directive, the unstamped refusal and the universal timing keys; the stamp declared for that surface (typed by its declaration: `E-ATTR-TYPE`, `E-BAD-ENUM`, `@ref` typing); renamed or removed attribute diagnostics; then `E-UNKNOWN-ATTR`. When the name is declared for other surfaces only, `E-UNKNOWN-ATTR` says where the stamp applies instead of listing the surface's attributes:

- `` `::bg` has no attribute `voiceAssetId`: it is a stamp of `acme.stage` for content lines only ``
- `` `<branch>` has no attribute `optionKey`: it is a stamp of `acme.stage` for `<choice>` only ``

A **required** stamp that its surface does not author is `E-MISSING-ATTR`, at the surface's head (the directive or element's opening tag, or the line):

- `` `::bg` requires attribute `assetName` (a stamp of `acme.stage`) ``
- `` this content line requires attribute `voiceAssetId` (a stamp of `acme.stage` for content lines) ``
- `` `<choice id="goLeft">` requires attribute `optionKey` (a stamp of `acme.stage`) ``

The rule applies wherever the surface is authored, component bodies included; a line that `::use` splices in is checked once, in its component. It does not look at a declared `default:`, because a stamp is never injected. For the logic-tag surfaces see [Choices & hubs](/language/choices-and-hubs/#plugin-stamps-on-branch-hub-and-choice).

**Snapshot.** A scoped entry folds into `capabilitySnapshot` with its sorted surface list; an entry without `on:` hashes exactly as before, so a manifest that uses no `on:` keeps its snapshot. `lute context` prints each scoped stamp's surfaces (`"on": "::bg, ::actor (stampAttrs)"`).

### Reserved stamp keys

The core stamp owns eight names — `at`, `duration`, `delay`, `wait`, `timeline`, `timing`, `provenance`, `source`. `at`, `duration`, `delay`, `wait` and `timeline` are authoring keys that compile into `timing`; `timing`, `provenance` and `source` stay flattened record keys. The nested `stamps` object removes the *JSON* collision for every other name, but these eight stay reserved, since a plugin stamp named like one would still be ambiguous to the author. A plugin declaring an attribute under any of them is rejected at assembly with **`E-PLUGIN-RESERVED-STAMP-ATTR`**:

<!-- lute-diagnostics -->
```
$ lute check scene.lute --project .
lute: E-PLUGIN-RESERVED-STAMP-ATTR: plugin `arcia.bonus` declares reserved stamp attribute `duration`; `at`/`duration`/`delay`/`wait`/`timeline`/`timing`/`provenance`/`source` are owned by the core stamp (plugin §14)
```

Both surfaces are covered — the `stampAttrs` export *and* an ordinary per-directive `attrs` entry — so a plugin cannot reach a reserved key through either door. The offending declaration is dropped rather than merged; its non-reserved siblings still land. This is why a blocking plugin directive names its own flag `sync` and not `wait` (see [Bridge](/plugins/bridge/)).

## Host interpolation tokens (`hostTokens`)

A line may carry a bare `{{playerName}}` for a value only the engine knows. The plugin export `hostTokens` (`hosttokens/*.yaml`, since 0.50) declares such names:

```yaml
# hosttokens/tokens.yaml
hostTokens:
  playerName: { type: string }
  rank:       { type: int }
  platform:   { type: { enum: [ios, android, web] } }
```

- The key is the token name and the value is `{ type }`, where `type` is `string`, `int` or `{ enum: [members…] }`. Any other type is `E-PLUGIN-PARSE`, and any other key is `E-PLUGIN-KEY`.
- A token name is an **identifier**: an ASCII letter or `_`, then letters, digits or `_` (not merely a name; a host token is written bare inside `{{…}}`). A non-identifier key is `E-PLUGIN-PARSE`: `` host token `player-name` is not an identifier (letters, digits and `_`, not starting with a digit) `` (`player-name`, `2ndName`, `-x` and the empty key are all refused).
- A name that is a state root or CEL literal (`scene`, `run`, `user`, `app`, `quest`, `entry`, `prev`, `clock`, `occasion`, `season`, `true`, `false`, `null`) is `E-PLUGIN-RESERVED-NAME` at assembly, with the reason in the message: `` `{{run}}` would read as a state root `` (see [Reserved names](/reference/reserved-names/)).
- **`lute.core` declares `userName: { type: string }`.** Redeclaring a token that another plugin or `lute.core` declares is `E-PLUGIN-DUP-ACROSS`.
- `hostTokens` folds into `capabilitySnapshot` only when a token not owned by `lute.core` exists, and only those tokens fold in, so a project that declares none keeps its snapshot. `lute context` lists the declared non-core tokens (name, type, plugin).

How a token is checked in text, and that the runtime leaves it verbatim, is in [Dialogue & cast](/language/dialogue-and-cast/#host-tokens).

## Declarative lowering

A directive's `lower:` says what the compiler emits for it. It is optional: without it the directive compiles to the generic `kind: "plugin"` passthrough record ([below](#passthrough-ownership-and-dispatch)). Before 0.24.0 a missing `lower:` was `E-PLUGIN-PARSE missing field lower`, although the passthrough was documented as the default. When present it takes one of two forms:

```yaml
lower: { record: <kind>, fields: { … } }   # a finite attrs → one core record
lower: { kind: builtin, name: <hook> }     # a named core hook
```

A `builtin` name must be one of the hooks the core registers for its own directives: `actorStage`, `cameraTransform`, `clearStage`, `end`, `jump`, `label`, `sequence`. Any other name is `E-PLUGIN-PARSE` when the plugin loads, with a did-you-mean for a near miss and the advice to omit `lower:` for the passthrough. Before 0.24.0 an unregistered name lowered as a silent passthrough, and the shipped bridge examples named hooks that never existed (`bridgeMinigame`, `bridgeServe`, …). A bridge directive needs no `lower:`; its `bridge: { service, operation }` binds the call (see [Bridge](/plugins/bridge/)).

<!-- lute-diagnostics unverified="built in crates/lute-manifest/src/schema.rs from several format! pieces and wrapped in the loader's E-PLUGIN-PARSE line; copied verbatim from lute check --project output" -->
```
./plugins/demo.pack/directives/d.yaml:6:5: error [E-PLUGIN-PARSE] directives[1]: `clearStag` is not a builtin lowering hook (did you mean `clearStage`?); the core registers actorStage, cameraTransform, clearStage, end, jump, label, sequence; omit `lower:` for the generic `kind: "plugin"` passthrough
```

The `record` form targets one of the nine **non-control-flow staging kinds** — `bg`, `music`, `sfx`, `vfx`, `actor`, `camera`, `cg`, `video`, `sequence` — binding each target field to a `fromAttr` reference or a literal:

```yaml
directives:
  - name: backdrop
    attrs:
      - { name: img,  required: true, type: string }
      - { name: time, type: string }
    lower:
      record: bg
      fields:
        assetId: { fromAttr: img }
        time:    { fromAttr: time }
```

`::backdrop{img="bg.lounge" time="night"}` then compiles to a real `bg` record — not a `kind: "plugin"` passthrough:

```json
{ "kind": "bg", "family": "staging", "position": "001-0300", "time": "night", "assetId": "bg.lounge", "timing": { "wait": true } }
```

The emitted record inherits the **target kind's** `wait` default (`bg`, `video` and `sequence` block, `cg` and `camera` do not, the rest omit the key), so it has the same kind and resolved fields as the core record. An optional source attribute that was not authored leaves its target field absent.

**Stage meaning follows the lowered record kind (0.45).** A plugin directive lowered to `bg` changes the scene like `::bg`: it auto-hides characters on stage before the record, records their departures, drops pending pose resets, and records the new background. A directive lowered to `actor` stages its resolved `character` like `::actor`; an `exit: true` field takes that character off stage. Its `character` field must bind to a required `fromAttr` attribute (`required: true`) or a literal string; assembly reports `E-LOWER-RECORD-FIELD` otherwise. Bound `posReset` and `preload` fields are neutral to the stage model. `music` only updates music bookkeeping; other lowered record kinds do not affect the stage.

Those nine are the whole vocabulary, and the exclusion is principled rather than a shortlist: control-flow kinds (`jump`, `choice`, `match`, `hub`, `barrier`, `end`, `quest`, `on`) carry positions the compiler's own passes resolve, and content kinds (`line`) carry identity — `lineId` / `voiceKey` — derived from the authored `code`. Neither is a finite attrs→fields mapping, so neither is data.

Like the core staging directive it stands for, a directive with a `lower:` record refuses the [`when=` guard](/language/directives/#guarding-a-directive-when) (`E-UNKNOWN-ATTR`); a passthrough or bridge directive takes it. Since dsl 0.26.0 §4 `when` is that guard on every directive, so a plugin attribute of that name can no longer be written: a use of it is `E-UNKNOWN-ATTR`, asking to rename the attribute in the plugin.

Both failures are caught at **assembly**, before anything is lowered — a declaration that fails validation never reaches the compiler:

- **`E-LOWER-RECORD-UNKNOWN`** — `record:` names something outside the nine.
- **`E-LOWER-RECORD-FIELD`** — a target field the record kind does not have, or a `fromAttr` naming an attribute the directive never declares.

<!-- lute-diagnostics -->
```
$ lute check scene.lute --project .
lute: E-LOWER-RECORD-FIELD: directive `::backdrop` lowers to record `bg`: unknown target field `mood` (record `bg` binds: location, time, assetId)
lute: E-LOWER-RECORD-UNKNOWN: directive `::sting` lowers to unknown record `line`; declarative lowering targets the staging kinds (bg, music, sfx, vfx, actor, camera, cg, video, sequence)
```

### Passthrough ownership and dispatch

When a directive declares no `lower:`, the compiler emits a generic passthrough record:

```json
{
  "kind": "plugin",
  "family": "plugin",
  "position": "001-0400",
  "tag": "host-panel",
  "plugin": "game.presentation",
  "fields": {
    "assetId": "CG.chapter_one.first_choice",
    "resultKey": "first-choice",
    "text": "First choice"
  }
}
```

The optional `plugin` field is the resolved owning plugin package id. It is assembly metadata, not
an authored attribute, so source cannot override it. Hosts route extension operations by
**`(plugin, tag)`**; core or unresolved passthrough records omit it. Plugin ids never become
dynamic `kind` values.

This does not change declarative presentation lowering: a directive with
`lower: { record: bg, fields: ... }` still emits a stable core `bg` record (and
likewise for the other permitted staging kinds), not a `kind: "plugin"` record. The owner metadata
applies only to generic passthrough plugin records.

## Installation & the profile graph

A project's `lute.project.yaml` declares `pluginsDir`, a `defaultProfile`, and a profile graph. A profile is a root-level capability selector; the reserved `global` profile is inherited by every other, and profiles compose via `extends`:

```yaml
pluginsDir: plugins/
defaultProfile: date-minigame
profiles:
  global:       { plugins: { lute.core: true } }
  story:        { plugins: { arcia.minigame: true } }
  date:         { extends: story }
  date-minigame:
    extends: date
    plugins:
      arcia.minigame: { resultScope: scene, allowedKinds: [rhythm, timing] }
```

`plugins` is a **map** from plugin id to a typed option object (or `true`, normalizing to defaults). Presence of a legal key **activates** the plugin — there is no `plugins.use` list. See [Profiles](/plugins/profiles/) for selection and merge rules.

## Resolution & the capability snapshot

Given the same installed plugins, selected profile, and scene frontmatter, resolution produces a **byte-identical** capability snapshot. It applies, in exact order: `lute.core` → `profiles.global` → the selected profile's `extends` chain (parent first) → the selected profile → scene-local `plugins:` → the dependency closure. Scalar options override, maps deep-merge, lists replace.

The snapshot is one immutable artifact carrying `plugins`, `enums`, `providers`, `stateShapes`, `stateTemplates`, `assetKinds`, `directives`, `bridgeCapabilities`, `frontmatter`, `events`, `stampAttrs`, `diagnostics`, and more. Its `capabilitySnapshot` is a content hash over that whole resolved surface — plugin ids+versions and their merged option objects, and every directive, enum, provider, state shape, bridge capability, def, frontmatter key and stamp attribute in it. Any drift in a populated field yields a different hash, and adding a directive to `lute.core` moves it for every project. Every generated artifact is stamped with the `capabilitySnapshot` it targets, and a consumer refuses mismatched stamps. Providers are **snapshot-first**: the compiler fails if required catalog data is missing but never blocks on the network, and the LSP keeps a stale snapshot with a *catalog-stale* diagnostic rather than false *unknown-id* errors.
