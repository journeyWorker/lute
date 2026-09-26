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
  assetkinds: assetkinds/
  defs: defs/
  stampattrs: stampattrs/
  enums: enums/
  frontmatter: frontmatter/
  events: events/
  occasions: occasions/
  rewardkinds: rewardkinds/
  cast: cast/
  lints: lints/
  docs: docs/
options:                   # OPTIONAL — typed activation options
  - { name: resultScope,  type: { enum: [scene, run] }, default: scene }
  - { name: allowedKinds, type: { list: { enum: [rhythm, puzzle, timing] } }, default: [rhythm, puzzle, timing] }
```

`depends[].range` is pinned to two forms only — caret (`^x.y.z`, pre-1.0 semantics) or an exact three-component version. Any other spelling is unsatisfiable by definition.

## Export files

Each export kind has a normative schema. All are typed by one small manifest type system (`bool` / `number` / `string`, `enum`, `list`, `record`, `map`, plus `enumFromOption`, `providerRef`, `slotId`, `assetKind`, `domain`, and shape refs; since dsl 0.26.0 a directive attribute may also be typed `{ entity: <kind> }`, [below](#engine-ids-typed-by-an-entity-kind)). State paths use **structured segments**, never `$name` interpolation.

- `directives/*.yaml` — `::name` directive declarations (see [Bridge](/plugins/bridge/)).
- `state/*.yaml` — reusable typed record shapes (`stateShapes:`) and structured path templates (`stateTemplates:`); one file may hold both (since 0.24.0; before, the second was dropped).
- `providers/*.yaml` — id registries resolved against a pinned snapshot.
- `bridge/*.yaml` — typed runtime bridge capabilities.
- `defs/*.yaml` — shared typed-CEL `@refs`.
- `assetkinds/*.yaml` — asset-id segment templates (compose / query modes) with ordered `fallback` hooks.
- `stampattrs/*.yaml` — cross-cutting attributes admissible on every directive and content line (below).
- `enums/*.yaml`, `frontmatter/*.yaml`, `docs/*.md` — named enum domains, plugin-owned meta keys, and hover docs.
- `events/*.yaml` — world events a quest's `<on event>` may name and `lute trace --event` fires.
- `occasions/*.yaml` — the engine moments [beats](/language/beats/) answer (dsl 0.21.0), each optionally raised for a target drawn from a project entity kind (dsl 0.22.0), and presented as one winner, an offered list, or a sequence (dsl 0.23.0).
- `rewardkinds/*.yaml` — the closed set of `<reward kind>` values, with an optional [target contract](#reward-target-contracts), extra attributes, and the state path a grant credits (dsl 0.23.0).
- `cast/*.yaml` — the speakers content lines may use, with display names (dsl 0.23.0).
- `lints/*.yaml` — advisory [lint rules](/tooling/linting/), namespaced `<plugin-id>/<rule-id>`; excluded from the capability snapshot.

An export name outside this list is a load error. Every export file is also read strictly (0.24.0): a key the declaration does not know is `E-PLUGIN-PARSE`, with a did-you-mean, instead of being ignored. This covers directives and their attrs, state, effects and bridge, state shapes and templates, providers, bridge capabilities, defs, enums, events, frontmatter, asset kinds, stamp attributes, reward kinds, occasions, cast entries and lints. Before, `{ selct: all }` loaded as `select: first` without a word:

<!-- lute-diagnostics unverified="serde's unknown-field text with a did-you-mean appended by crates/lute-manifest, then wrapped in the loader's E-PLUGIN-PARSE line; no single format! literal pins it; copied verbatim from lute check --project output" -->
```
lute: E-PLUGIN-PARSE: `./plugins/demo.pack/occasions/o.yaml` failed to parse: occasions.inbox: unknown field `selct`, expected one of `select`, `target`, `description`, `judge` at line 3 column 12; did you mean `select`?
```

A common way to get there is an unquoted description in a flow map. YAML ends an unquoted value at the first comma, so `{ select: all, description: Pick one, the player picks one }` leaves `the player picks one` as a key with no value. The error says so and spells the quoted form:

<!-- lute-diagnostics unverified="serde's unknown-field text with the flow-map hint appended by crates/lute-manifest, wrapped in the loader's E-PLUGIN-PARSE line; copied verbatim from lute check --project output" -->
```
lute: E-PLUGIN-PARSE: `./plugins/demo.pack/occasions/o.yaml` failed to parse: occasions.inbox: unknown field `the player picks one`, expected one of `select`, `target`, `description`, `judge` at line 3 column 48; `the player picks one` has no value — in a flow map `{ … }` an unquoted value ends at the first comma, so the rest became a key; quote the description: `description: "Pick one, the player picks one"`
```

A plugin whose `plugin.yaml` parsed but one of whose exports did not is not loaded at all. The follow-up error for a profile that activates it says the plugin **failed to load** and points back at the parse error. Before 0.24.0 it told the author to install a plugin that was already installed:

<!-- lute-diagnostics unverified="assembled in crates/lute-manifest from the plugin id and a fixed suffix; copied verbatim from lute check --project output" -->
```
lute: E-PLUGIN-MISSING-ACTIVE: plugin `demo.pack` is activated by the project manifest but failed to load (see E-PLUGIN-PARSE above); fix its package
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
  hubVisit: {}
  examine:  { select: first, target: true }
  talk:     { select: first, target: { prefix: npc, entity: person } }
  bossDefeated: { select: first, target: { prefix: boss, entity: foe, members: [gatekeeper, warden] } }
  inbox:    { select: all, description: Letters waiting at the fountain }
  evening:  { select: sequence }
  dayEnd:   { select: first, judge: before }
```

An occasion's `select:` says what the engine presents when it is raised. `first` (the default) presents the single winning beat, plus any eligible [`also`](/language/beats/#side-remarks-with-also) beat after it. `all` offers every eligible beat and the player picks one. `sequence` (dsl 0.23.0) presents every eligible beat in selection order, such as an evening routine followed by the day's event (see [Composing an occasion](/language/beats/#composing-an-occasion)). A beat's `also: true` on an `all` or `sequence` occasion is `E-BEAT-ATTR`. When a world event of the same name is also declared under `events:`, every raise of the occasion fires that event after the beats, before the occasion judges its `on=` objectives (see [Occasions](/language/beats/#occasions)).

An occasion's `judge:` (dsl 0.24.0 §2) says when a raise judges the quest objectives that name it with `on=`. The default `after` judges them after the beats are presented, the order since 0.21.0. `before` judges them and settles the quests before the beats are decided, so an epilogue on `dayEnd` can read how its quests ended (`quest.<id>.failedBy`, see [Quests & scenes](/language/quests-and-scenes/)). Only the judging moves. The `<on>` handler bodies the raise answers, both the same-named `<on event>` handlers and the `questComplete` / `questFailed` handlers of the quests it settles, still run after the beats, so their narration follows the scene. Any other value is `E-PLUGIN-PARSE`. An occasion without `judge: before` keeps the `capabilityVersion` stamp it had.

An occasion's `target:` says what it is raised for. Absent or `false`, it is untargeted. `true` keeps its 0.21.0 meaning: the occasion is raised for some dotted id, and a beat's target is checked for shape only. A **domain** `{ prefix, entity }` (dsl 0.22.0) also closes the set: a target is `<prefix>.<member>`, where `entity` names an entity kind the *project* declares under `entities:` in its schema. The plugin supplies the prefix and the kind, and the project supplies the members (or declares the kind `open:` for engine-populated members). A beat target outside the domain is `E-BEAT-ATTR`, with a did-you-mean when a member is close, and so is every target of an occasion whose kind the document's schema does not declare (see [Beats](/language/beats/#target-domains)). A `target:` that is neither a bool nor a `{ prefix, entity }` map (with an optional `members:`) fails the plugin load with `E-PLUGIN-PARSE`.

A domain's optional **`members:`** list narrows it to a subset of the kind: `bossDefeated` above is raised only for `boss.gatekeeper` and `boss.warden`, even when the project's `foe` kind has more members. Which members a kind has is project vocabulary the plugin cannot see, so the checker judges the list: a listed member the kind does not declare makes every target of the occasion `E-BEAT-ATTR`, naming the stray member. On an `open:` kind the list is taken as written. The subset narrows every consumer of the domain alike: beat and objective targets, a [`lute play`](/tooling/play/) step's `target:`, `lute new scene --target`, and `lute calendar --target`. The loader itself rejects two shapes, as `E-PLUGIN-PARSE` on the occasion file:

- an empty list: ``occasion `bossDefeated`'s `target.members` is empty; list at least one member of entity kind `foe`, or drop `members` to draw targets from the whole kind``
- a member listed twice: ``occasion `bossDefeated`'s `target.members` lists `warden` more than once``

Occasions are part of the capability snapshot, so they fold into `capabilityVersion`. Declaring a domain restamps, and so does adding or changing its `members:` list; an occasion that only ever says `target: true` or `false` keeps the stamp it had under 0.21.0. Likewise, `select: sequence` changes the stamp only for a snapshot that declares it.

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

`enums/` is the third route a project gets its [content vocabulary](/language/vocabulary/) from, and the only one that is *capability* rather than project data: `lute.core` declares the seven slots and exports an **empty** `enums`, so an engine or genre pack ships members to every project that activates it. Its entries take the same long form as an author's `enums:` block — a bare sequence is shorthand for `{ members: [...] }`, and `action` must carry `exits:` while `anchor` must carry `default:`.

### Rewards that credit state

A reward is data: the engine grants it, and content never reads it. When a reward kind is a currency your own state tracks, `credits:` (dsl 0.23.0) names the state path a grant adds its amount to. The compiler stamps the path on every reward of that kind (`RewardEntry.credits` in the IR, omitted for a kind without one), and the engine owns that write, as it owns the grant itself. [`lute run`](/tooling/cli/#run) and [`lute play`](/tooling/play/) do the same for a **scalar** amount, and record the credit on the grant:

```
grant climb EMBERS 100 (credits user.embers = 100.0)
```

A range amount (`amount="10..20"`) is the engine's roll, so the toolchain grants it without crediting anything. Crediting by hand as well pays twice: a content `::set` of the credited path in one of the same quest's `<on>` handlers or objective bodies is `W-REWARD-DOUBLE-CREDIT`:

<!-- lute-diagnostics -->
```
./quests/climb.lute:11:11: warning [W-REWARD-DOUBLE-CREDIT] `::set` of `user.embers` in a handler of quest `climb`: its `<reward kind="EMBERS">` already credits `user.embers` when granted, so the player is paid twice — drop the `::set` or the reward (dsl 0.23.0 §8)
```

A kind without `credits:` hashes exactly as it did before 0.23.0, so adding the key to one kind restamps only the snapshots that declare it.

### Engine ids typed by an entity kind

An attribute that names something the engine owns, such as a bag item, a species, or a shop's stock list, may be typed by an **entity kind** the project declares (dsl 0.26.0 §2.5):

```yaml
# directives/engine.yaml
directives:
  - name: give
    attrs:
      - { name: item, required: true, type: { entity: bagItem } }
      - { name: qty,  type: number, default: 1 }
```

The plugin names the kind and the project lists its members, the same split as an occasion's [target domain](/language/beats/#target-domains). The kind lives in a schema every document imports, typically one lead-owned file of the ids the engine ships:

```yaml
# schema/items.schema.yaml
entities:
  bagItem: { members: [potion, superPotion, goodRod, nugget] }
```

A value outside the kind is `E-BAD-ENUM`, with a did-you-mean. An attribute typed by a kind that no schema of the document declares is `E-DOMAIN-UNKNOWN` at every use:

<!-- lute-diagnostics unverified="verbatim lute check-project output; the E-DOMAIN-UNKNOWN message spells the type with escaped braces, which the scraper does not read back as a literal" -->
```
./scenes/mart.lute:9:14: error [E-BAD-ENUM] `potoin` is not a member of entity kind `bagItem` (attribute `item` of `::give`) — did you mean `potion`?
./scenes/mart.lute:10:22: error [E-DOMAIN-UNKNOWN] attribute `species` of `::encounter` is typed `{ entity: monster }`, but `monster` is not a declared entity kind — declare it under `entities:` in a project schema this document reaches through `uses:` (dsl 0.26.0 §2.5)
```

A value that reaches the attribute through a component param is checked at the `::use` argument, or at the param's `default:` (see [Engine ids through a param](/language/components-and-extends/#engine-ids-through-a-param)). `lute refs <dir> --attr give.item` lists every value with the documents and lines that use it, so a lead sees who gives what before merging several authors' work.

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

A contract names `entity:` or `provider:`, not both: one with both fails the plugin load (`E-PLUGIN-PARSE`). A kind without `target:` takes any `target=` or none, as before. The ITEM contract above catches both mistakes a hand-maintained list lets through:

<!-- lute-diagnostics -->
```
./quests/dex.lute:11:3: error [E-REWARD-TARGET] `<reward kind="ITEM">` needs a `target=`: the reward kind declares `target: { required: true }` (dsl 0.26.0 §2.5)
./quests/dex.lute:12:31: error [E-REWARD-TARGET] `goodRd` is not a member of entity kind `bagItem`, which `<reward kind="ITEM">` targets (dsl 0.26.0 §2.5) — did you mean `goodRod`?
```

Which form to use for ids the engine owns: an **entity kind** when the ids can be listed in the repository and reviewed there (one lead-owned schema such as `schema/items.schema.yaml` above), a **provider** when the engine's catalog is the source of truth and the project checks against a pinned snapshot of it (see [Providers & catalog](/tooling/providers-and-catalog/)). Either way, type the directive attribute and the reward kind by the **same** kind or provider, so `::give{item}` and `<reward kind="ITEM" target>` share one id space, and `lute refs <dir> --attr give.item --reward ITEM` lists both (a reward without a target as `(no target)`).

### Cast

```yaml
# cast/harbor.yaml
cast:
  mira:  { name: Mira }
  oskar: { name: "Oskar Lind", present: "run.oskarAboard", emotions: [calm, grim] }
  vesna: {}
```

A `cast` export (dsl 0.23.0) declares the speakers an engine pack is built for: each map key is a speaker id, and every field is optional. `name` is the display name. Since 0.24.0 an entry may also declare `present:`, the condition under which the character is with the player, `emotions:`, the `emotion=` values their lines may use, and `assume: true`, which lets the presence check take the engine's `reserved:` facts that `present:` negates as absent. A line whose guards do not imply `present:` is `W-CAST-ABSENT`, and an `emotion=` outside `emotions:` is `E-BAD-ENUM` (see [The cast](/language/dialogue-and-cast/#the-cast)). Any other key is `E-PLUGIN-PARSE`. Once any cast is declared, by a plugin or by a schema document's `cast:` key, a content line whose speaker is outside it is `E-CAST-UNKNOWN` with a did-you-mean, and so is an `::auto{character}` or `::camera{focus}` that names someone outside it. The plugin casts and the schema casts a document imports are unioned, and a plugin's entry wins an id they share, because it carries the engine's display name. Two active plugins declaring the same id is `E-PLUGIN-DUP-ACROSS` at assembly. A non-empty cast folds into `capabilityVersion`; a plugin that exports none leaves the stamp alone, and a cast that uses none of the 0.24.0 keys keeps the stamp it had under 0.23.

Since dsl 0.26.0 §2.8 an entry may also say `sharedName: true`: its `name` is a role name several speakers share on purpose, such as a villain team's rank and file (`grunt1: { name: Eclipse Grunt, sharedName: true }`). `check-project` and `lute lint` report two speakers shown under exactly the same name as the advisory `W-DISPLAY-NAME-DUP`, and a `sharedName` entry is not counted (see [Display names shared by two speakers](/language/dialogue-and-cast/#display-names-shared-by-two-speakers)).

## Cross-cutting attributes (`stampAttrs`)

Ordinary attributes are declared per directive. An engine that tags *every* record with the same key — an analytics id, an experiment bucket, a bonus hook — had no declaration site for one, and could never put it on a content line at all. `stampattrs/*.yaml` is that site:

```yaml
stampAttrs:
  - { name: bonusId,    type: string }
  - { name: bonusScore, type: number }
```

Entries are ordinary `AttrDecl`s — the same `{ name, required?, type, default? }` shape a directive attr uses — but they are admissible on **every** directive *and* on content lines (`@speaker{…}: text`), on top of that surface's own attributes. Resolution is strict: the surface's own declarations win, then `stampAttrs`, then `E-UNKNOWN-ATTR`. Value typing rides the existing attribute path, so a mistyped one is a plain `E-ATTR-TYPE` / `E-BAD-ENUM` — no new rules.

`::sfx{sound="chime" bonusId="b-02"}` followed by `@marina{code="0010" bonusId="b-01" bonusScore="7"}: Welcome back.` compiles to two records that each carry the attribute **flattened into the record's stamp**, beside the reserved timing keys — never in the record's own `fields`:

```json
{ "kind": "sfx", "addr": "001-0100", "sound": "chime", "bonusId": "b-02" }
{ "kind": "line", "addr": "001-0200", "role": "dialogue", "speaker": "marina",
  "text": "Welcome back.", "lineId": "marina.s01ep01.marina_0010",
  "voiceKey": "marina.s01ep01.marina-0010", "bonusId": "b-01", "bonusScore": 7.0 }
```

An **unauthored** stamp attribute is not injected — not even when its declaration carries a `default`. Absent means absent, so declaring a cross-cutting vocabulary and authoring none of it leaves the artifact byte-identical. The declaration is not free, though: `stampAttrs` participates in `capabilityVersion`, because a changed cross-cutting vocabulary is a changed capability surface and an engine must be able to refuse the mismatch.

### Reserved stamp keys

The core stamp owns seven names — `at`, `duration`, `delay`, `wait`, `timeline`, `provenance`, `source`. A plugin declaring an attribute under any of them is rejected at assembly with **`E-PLUGIN-RESERVED-STAMP-ATTR`**:

<!-- lute-diagnostics -->
```
$ lute check scene.lute --project .
lute: E-PLUGIN-RESERVED-STAMP-ATTR: plugin `arcia.bonus` declares reserved stamp attribute `duration`; `at`/`duration`/`delay`/`wait`/`timeline`/`provenance`/`source` are owned by the core stamp (plugin §14)
```

Both surfaces are covered — the `stampAttrs` export *and* an ordinary per-directive `attrs` entry — so a plugin cannot reach a reserved key through either door. The offending declaration is dropped rather than merged; its non-reserved siblings still land. This is why a blocking plugin directive names its own flag `sync` and not `wait` (see [Bridge](/plugins/bridge/)).

## Declarative lowering

A directive's `lower:` says what the compiler emits for it. It is optional: without it the directive compiles to the generic `kind: "plugin"` passthrough record ([below](#passthrough-ownership-and-dispatch)). Before 0.24.0 a missing `lower:` was `E-PLUGIN-PARSE missing field lower`, although the passthrough was documented as the default. When present it takes one of two forms:

```yaml
lower: { record: <kind>, fields: { … } }   # a finite attrs → one core record
lower: { kind: builtin, name: <hook> }     # a named core hook
```

A `builtin` name must be one of the hooks the core registers for its own directives: `autoStage`, `cameraTransform`, `clearStage`, `end`, `mark`, `next`. Any other name is `E-PLUGIN-PARSE` when the plugin loads, with a did-you-mean for a near miss and the advice to omit `lower:` for the passthrough. Before 0.24.0 an unregistered name lowered as a silent passthrough, and the shipped bridge examples named hooks that never existed (`bridgeMinigame`, `bridgeServe`, …). A bridge directive needs no `lower:`; its `bridge: { service, operation }` binds the call (see [Bridge](/plugins/bridge/)).

<!-- lute-diagnostics unverified="built in crates/lute-manifest/src/schema.rs from several format! pieces and wrapped in the loader's E-PLUGIN-PARSE line; copied verbatim from lute check --project output" -->
```
lute: E-PLUGIN-PARSE: `./plugins/demo.pack/directives/d.yaml` failed to parse: directives[1]: `clearStag` is not a builtin lowering hook (did you mean `clearStage`?); the core registers autoStage, cameraTransform, clearStage, end, mark, next; omit `lower:` for the generic `kind: "plugin"` passthrough at line 6 column 5
```

The `record` form targets one of the eight **non-control-flow staging kinds** — `background`, `music`, `sfx`, `vfx`, `sprite`, `camera`, `cut`, `video` — binding each target field to a `fromAttr` reference or a literal:

```yaml
directives:
  - name: backdrop
    attrs:
      - { name: img,  required: true, type: string }
      - { name: time, type: string }
    lower:
      record: background
      fields:
        assetId: { fromAttr: img }
        time:    { fromAttr: time }
```

`::backdrop{img="bg.lounge" time="night"}` then compiles to a real `background` record — not a `kind: "plugin"` passthrough:

```json
{ "kind": "background", "addr": "001-0100", "time": "night", "assetId": "bg.lounge", "wait": true }
```

The emitted record inherits the **target kind's** `wait` default (`background` and `video` block, `cut` and `camera` do not, the rest omit the key), so it is indistinguishable from the core directive an author could have written by hand. An optional source attribute that was not authored leaves its target field absent.

Those eight are the whole vocabulary, and the exclusion is principled rather than a shortlist: control-flow kinds (`jump`, `choice`, `match`, `hub`, `barrier`, `end`, `quest`, `on`) carry addresses the compiler's own passes resolve, and content kinds (`line`) carry identity — `lineId` / `voiceKey` — derived from the authored `code`. Neither is a finite attrs→fields mapping, so neither is data.

Like the core staging directive it stands for, a directive with a `lower:` record refuses the [`when=` guard](/language/directives/#guarding-a-directive-when) (`E-UNKNOWN-ATTR`); a passthrough or bridge directive takes it. Since dsl 0.26.0 §4 `when` is that guard on every directive, so a plugin attribute of that name can no longer be written: a use of it is `E-UNKNOWN-ATTR`, asking to rename the attribute in the plugin.

Both failures are caught at **assembly**, before anything is lowered — a declaration that fails validation never reaches the compiler:

- **`E-LOWER-RECORD-UNKNOWN`** — `record:` names something outside the eight.
- **`E-LOWER-RECORD-FIELD`** — a target field the record kind does not have, or a `fromAttr` naming an attribute the directive never declares.

<!-- lute-diagnostics -->
```
$ lute check scene.lute --project .
lute: E-LOWER-RECORD-FIELD: directive `::backdrop` lowers to record `background`: unknown target field `mood` (record `background` binds: location, time, assetId)
lute: E-LOWER-RECORD-UNKNOWN: directive `::sting` lowers to unknown record `line`; declarative lowering targets the staging kinds (background, music, sfx, vfx, sprite, camera, cut, video)
```

### Passthrough ownership and dispatch

When a directive declares no `lower:`, the compiler emits a generic passthrough record:

```json
{
  "kind": "plugin",
  "addr": "001-0100",
  "plugin": "game.presentation",
  "tag": "host-panel",
  "fields": {}
}
```

The optional `plugin` field is the resolved owning plugin package id. It is assembly metadata, not
an authored attribute, so source cannot override it. Hosts route extension operations by
**`(plugin, tag)`**; older artifacts may omit `plugin`, and core or unresolved passthrough records
omit it. Plugin ids never become dynamic `kind` values.

This does not change declarative presentation lowering: a directive with
`lower: { record: background, fields: ... }` still emits a stable core `background` record (and
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

The snapshot is one immutable artifact carrying `plugins`, `enums`, `providers`, `stateShapes`, `stateTemplates`, `assetKinds`, `directives`, `bridgeCapabilities`, `frontmatter`, `events`, `stampAttrs`, `diagnostics`, and more. Its `capabilityVersion` is a content hash over that whole resolved surface — plugin ids+versions and their merged option objects, and every directive, enum, provider, state shape, bridge capability, def, frontmatter key and stamp attribute in it. Any drift in a populated field yields a different version, and adding a directive to `lute.core` moves it for every project. Every generated artifact is stamped with the `capabilityVersion` it targets, and a consumer refuses mismatched stamps. Providers are **snapshot-first**: the compiler fails if required catalog data is missing but never blocks on the network, and the LSP keeps a stale snapshot with a *catalog-stale* diagnostic rather than false *unknown-id* errors.
