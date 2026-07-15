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
character: bianca
season: 1
episode: 1
pov: fixer
profile: showcase
uses: schema/game.schema.yaml
state:
  scene.affect.bianca: { type: number, default: 0 }
defs:
  fond: { type: bool, cel: "scene.affect.bianca >= 1" }
---
```

*(Frontmatter excerpt from [`docs/examples/showcase/episode01.lute`](https://github.com/KantoRegion/lute/blob/main/docs/examples/showcase/episode01.lute).)*

## Required keys

A root document must declare its **`kind`** — either `scene` or `quest` — and, for a scene, the
identity triple **`character`**, **`season`**, and **`episode`**. Omitting any of these is a static
error (`E-KIND-MISSING`, `E-META-MISSING`).

## Optional keys

- **`episodeId`** — a stable opaque episode id, the prefix input to every derived `lineId`. When
  omitted it defaults to `s{season:02}ep{episode:02}` (e.g. `season: 1, episode: 2` → `s01ep02`).
  Pinning it explicitly lets you renumber `season`/`episode` without breaking translation or voice
  keys.
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
Plugins may contribute additional frontmatter keys through their manifest (for example, `cast:` is
owned by the character/cast capability).

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
