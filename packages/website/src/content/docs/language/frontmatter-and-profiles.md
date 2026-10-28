---
title: Frontmatter & profiles
description: The YAML frontmatter block that opens every .lute document — required and optional keys, plus the profile/plugins capability selectors.
---

Every `.lute` document opens with a **YAML frontmatter block** delimited by two `---` lines. It
must be the document's first construct, before any body content. It answers "what is this document,
and what capabilities does it use?".

```yaml
---
kind: scene
title: The Full Showcase
character: marina
season: 1
episode: 1
pov: fixer
profile: showcase
uses: schema/game.schema.yaml
state:
  scene.affect.marina: { type: int, default: 0 }
defs:
  fond: { type: bool, cel: "scene.affect.marina >= 1" }
---
```

*(Frontmatter excerpt from [`docs/examples/showcase/episode01.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/showcase/episode01.lute).)*

## Required keys

A root document must declare its **`kind`** — `scene`, `quest`, or [`lore`](/language/lore-entries/) — and a scene
must declare an identity: an **`id`**, or the triple **`character`**, **`season`**, and
**`episode`**. Omitting these is a static error (`E-KIND-MISSING`, `E-META-MISSING`).

## Optional keys

- **`id`** — the scene's canonical key (`town.welcome`, `mara.first`): what `visited('…')` and
  `after:` name, the artifact's `meta.id`, and the `{prefix}` of every derived `lineId`. Letters,
  digits, `_`, `.`, and `-` only (`E-META-ID`). When omitted, the key is derived as
  `{character}.{episodeId}`.
- **`episodeId`** — a stable opaque episode id, the input to that derived key. When omitted it
  defaults to `s{season:02}ep{episode:02}` (e.g. `season: 1, episode: 2` → `s01ep02`). Pinning it
  explicitly lets you renumber `season`/`episode` without breaking translation or voice keys.
- **`title`** — an optional human title (localizable), carried as the artifact's `meta.title`. It
  is the document's only title: a `# ` heading in the body is `E-INERT-TITLE`, and the message
  names `title:`.
- **`pov`** — the id of the point-of-view (protagonist) speaker. It decides who may speak `mono`:
  a `{mono}` line's speaker must be the effective POV or appear in `monoSpeakers`, else
  `E-MONO-POV` (`E-MONO-NO-POV` when no POV resolves at all). The effective POV is the document's
  `pov:`, or else the project's `defaults.pov` in `lute.project.yaml`. Otherwise the POV speaker's
  lines compile as ordinary lines, and the artifact does not carry `pov` (see
  [Dialogue & cast](/language/dialogue-and-cast/)).
- **`monoSpeakers`** — a list of further speaker ids who may speak `mono` beside the POV, such as
  `monoSpeakers: [mira]` for a scene that also gives `mira` interior monologue. Like `pov`, it can
  be set once for the project under `defaults.monoSpeakers`; a document's own list replaces the
  project's. A line expanded from a [component](/language/components-and-extends/#speaker-params)
  is checked against the POV and `monoSpeakers` of the document whose `::use` expands it.
- **`luteVersion`** — the language-version pin; distinct from `app.lang` game state.
- **`contentLang`** — the source authoring language (a BCP 47 code such as `en-US` or `ko-KR`).
- **`mode`** — the authoring mode; `inline` is the only defined form.
- **`after`** — the scene's connectivity prerequisites (see
  [Quests & scenes](/language/quests-and-scenes/)).
- **`uses` / `components` / `extends`** — import the shared state schema and reusable content
  components (see [Imports](/language/imports/) and
  [Components & extends](/language/components-and-extends/)).
- inline **`state`** and **`defs`** blocks (see [State model](/state/state-model/) and
  [Params](/language/params/)).

A top-level key that is neither a core key nor owned by an active plugin is a static error.
Plugins may contribute additional frontmatter keys through their manifest (a `frontmatter` export;
the draft character/cast capability, for example, would own a `cast:` key).

## `identity:` — the id shapes a project compiles to

`episodeId` above is one input to a larger machine, and the machine is declared
once per project rather than per document. In `lute.project.yaml`:

```yaml
identity:
  lineId: "{prefix}.{speaker}_{code}"
  voiceKey: "{prefix}.{speaker}-{code}"
```

Both values above are the **defaults** — a project that declares no `identity:`
block compiles exactly as if it had declared this one, and a project that
declares one of the two keys leaves the other at its default.

`{prefix}` is derived, not authored: it is the document's key — its `id:`, or else
`{character}.{episodeId}`, with `episodeId` defaulting to `s{season:02}ep{episode:02}` as
described above. So `character: haven`, `season: 1`, `episode: 2` gives the prefix
`haven.s01ep02`, and a line `@purser{code="0020"}` compiles to
`lineId: "haven.s01ep02.purser_0020"` and `voiceKey: "haven.s01ep02.purser-0020"`. Pinning
`episodeId: pilot` on that same scene gives `haven.pilot.purser_0020`; declaring `id: haven.deck`
gives `haven.deck.purser_0020`.

**Breaking in 0.22.0: the default `voiceKey` carries `{prefix}`.** Through 0.21 it was
`{speaker}-{code}`, which is the same in every document: a `@purser{code="0020"}` line in
episode 3 was also `purser-0020`, one voice asset for two different lines (0.21.1 began refusing
that as `E-DUP-VOICEKEY`). With the prefix the default is unique across the project. A project
that recorded audio against the old keys pins the old template in `lute.project.yaml` to keep
them:

```yaml
identity:
  voiceKey: "{speaker}-{code}"
```

Under such a pin, two lines that say different things under one key still refuse to build:
Whole-project `lute check` and whole-project `lute compile` (no document operand, `-o <DIR>`) report `E-DUP-VOICEKEY`, naming every line on the key. A
project that already declared `voiceKey: "{prefix}.{speaker}-{code}"` compiles unchanged, and can
drop the pin.

Lines expanded from a [component](/language/components-and-extends/#line-identity) get their own
scope (0.22.0): inside a `::use` expansion, `{prefix}` is `{prefix}.{component}#{n}`, where `n`
counts the host's uses of that component. So the second `::use{component="toast"}` in
`harbor.dinner` compiles its `@mira{code="0010"}` line to `harbor.dinner.toast#2.mira_0010`, and its
default `voiceKey` to `harbor.dinner.toast#2.mira-0010`. A pinned template without `{prefix}`
drops that scope from the `voiceKey`, so the component line shares `mira-0010` with the host's own
`@mira{code="0010"}`.

The two templates govern **spoken content lines only**. Two other ids in the
artifact are also called `lineId`/`titleLineId` and are *not* templated:

- a branch option's `lineId` is always `{prefix}.{branch id}.{choice id}`;
- a quest's `titleLineId` is always `{quest id}.title`, and an objective's is
  `{quest id}.{objective id}` — a quest has no `{prefix}` at all.

The token set is closed — `{prefix}`, `{speaker}`, `{code}` — and
`E-IDENTITY-TEMPLATE` enumerates it on any mistake:

<!-- lute-diagnostics -->
```
lute: E-IDENTITY-TEMPLATE: unknown token `{character}` in identity template `lineId`; valid tokens are {prefix}, {speaker}, {code}
```

## Profiles & plugins

Lute's authoring vocabulary — which directives, attributes, enums, and events are legal — is
resolved from a **capability profile**.

- **`profile`** is a root-level capability selector. It names one of the profiles declared in the
  project's `lute.project.yaml`. If absent, the project's `defaultProfile` applies. The reserved
  profile name `global` is inherited by every other profile.
- **`plugins`** adds scene-local plugin activations and options *on top of* the selected profile.
  It maps a plugin id to a typed option object; the presence of the id activates that plugin for
  this scene. An empty object means "active with defaults", and the shorthand `true` normalizes to
  the same. A value that is neither a map nor `true` is a static error (`E-PROFILE-PLUGIN-VALUE`).

```yaml
profile: showcase
plugins:
  showcase.pack:
    resultScope: scene
    allowedKinds: [rhythm]
```

Effective capabilities resolve deterministically: `lute.core` → `global` → the selected profile's
`extends` chain (parent first) → the selected profile → scene-local `plugins` → dependency closure.
Activation is purely additive. The checker, LSP, and compiler all validate the document against the
same resolved capability snapshot, so what checks clean is exactly what compiles.

## Plugin frontmatter keys

A plugin's `frontmatter/*.yaml` export declares keys a document may carry, each with a `schema`
(see [Plugin concepts](/plugins/concepts/)). A value that does not fit its schema is
`E-FRONTMATTER-SCHEMA`.

Since 0.53 a reference is checked **wherever it sits** in the value, not only at the top of the
key. A key typed `entity`, `domain`, `enumFromOption`, `providerRef` or `assetKind` is checked
like an attribute of that type, and so is every such value inside a `list`, a `record` field or a
`map` key or value, at any depth. The diagnostic keeps the code the attribute check gives
(`E-BAD-ENUM`, `E-UNKNOWN-ID`, `E-ASSET-*`) and names the **field path** in the message, as in the
output below: the field of a record, the item of a list, the key or the entry of a map. It sits at
the key's line. A `record` field the declaration does not know and a field
of a non-reference type yield no reference. A value that is not representable in the schema (a
`null`, a `!tag`, a map key of an unsupported type) and a shape that does not match stay
`E-FRONTMATTER-SCHEMA`.

```yaml
# plugins/acme.stage/frontmatter/keys.yaml
frontmatter:
  - { key: lead, schema: { entity: person } }
  - { key: guests, schema: { list: { entity: person } } }
  - key: talk
    schema:
      record:
        - { name: partner, type: { entity: person } }
        - { name: turns, type: int }
  - key: mood
    schema:
      map: { key: { entity: person }, value: { list: { entity: person } } }
```

With `person: { members: [mira, theo] }` declared, a scene that misspells each of them:

```yaml
lead: mirra
guests: [mira, theoo]
talk: { partner: mirra, turns: 3 }
mood:
  mira: [theo]
  theoo: [mirra]
```

<!-- lute-diagnostics -->
```
./scenes/intro.lute:4:1: error [E-BAD-ENUM] `mirra` is not a member of entity kind `person` (attribute `lead` of `frontmatter`) — did you mean `mira`?
./scenes/intro.lute:5:1: error [E-BAD-ENUM] `theoo` is not a member of entity kind `person` (attribute `guests` of `frontmatter`) — did you mean `theo`?
```

<!-- lute-diagnostics unverified="the field, map key and map entry path forms are not in the doc matcher's message templates; verbatim lute check output" -->
```
./scenes/intro.lute:6:1: error [E-BAD-ENUM] `mirra` is not a member of entity kind `person` (field `partner` of `talk` of `frontmatter`) — did you mean `mira`?
./scenes/intro.lute:7:1: error [E-BAD-ENUM] `theoo` is not a member of entity kind `person` (key `theoo` of `mood` of `frontmatter`) — did you mean `theo`?
./scenes/intro.lute:7:1: error [E-BAD-ENUM] `mirra` is not a member of entity kind `person` (item 1 of entry `theoo` of `mood` of `frontmatter`) — did you mean `mira`?
```

Plugin frontmatter types also count as **readers** for `W-DOMAIN-UNREAD`: an entity kind or enum
that only a frontmatter key's type (at any depth) names is read, so it no longer draws the
warning. With no frontmatter key typed by it, the warning lists "no plugin frontmatter key is typed
by it" among the readers it looked for:

<!-- lute-diagnostics -->
```
./schema/world.schema.yaml:2:3: warning [W-DOMAIN-UNREAD] domain `person` is declared but no active construct reads it: no directive attribute, content-line slot, or state path is typed `{ domain: person }`, no plugin frontmatter key is typed by it, no `relations:` entry takes it as an argument, no `per: person` state family or `subsetOf: person` kind builds on it, and no rule or condition queries `person(…)` — so it enforces nothing and only reaches the artifact's `enums` array. Read it, or remove the declaration
```
