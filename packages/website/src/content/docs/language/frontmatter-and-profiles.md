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
  scene.affect.marina: { type: number, default: 0 }
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

- **`id`** — the scene's canonical key (`hub.welcome`, `mara.first`): what `visited('…')` and
  `after:` name, the artifact's `meta.id`, and the `{prefix}` of every derived `lineId`. Letters,
  digits, `_`, `.`, and `-` only (`E-META-ID`). When omitted, the key is derived as
  `{character}.{episodeId}`.
- **`episodeId`** — a stable opaque episode id, the input to that derived key. When omitted it
  defaults to `s{season:02}ep{episode:02}` (e.g. `season: 1, episode: 2` → `s01ep02`). Pinning it
  explicitly lets you renumber `season`/`episode` without breaking translation or voice keys.
- **`title`** — an optional human title (localizable).
- **`pov`** — the id of the player/protagonist speaker. The content-line speaker whose id equals
  `pov` renders as the reserved **player** kind (see [Dialogue & cast](/language/dialogue-and-cast/)).
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
`check-project` and `compile --all` report `E-DUP-VOICEKEY`, naming every line on the key. A
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
