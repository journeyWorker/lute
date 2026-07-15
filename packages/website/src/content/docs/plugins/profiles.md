---
title: Profiles & activation
description: Selecting a capability profile in scene frontmatter, the reserved global profile and extends inheritance, and layering scene-local plugin options on top.
---

A **profile** is a root-level capability selector: it decides which plugins — and therefore which vocabulary — are active for a scene. The project declares a profile graph and a `defaultProfile` in `lute.project.yaml`; a scene picks one with frontmatter `profile:` and MAY layer scene-local `plugins:` on top.

## The profile graph

```yaml
profiles:
  global:                       # reserved name; inherited by every profile
    plugins: { lute.core: true }
    lint: { unknownDirective: error }
  story:
    plugins: { idola.vn: true }
  date:
    extends: story
    plugins: { idola.date: { phoneSurface: enabled } }
  date-minigame:
    extends: date
    plugins:
      idola.minigame: { resultScope: scene, allowedKinds: [rhythm, timing] }

defaultProfile: story
```

- **`global` is reserved** and is applied before any other profile — every profile inherits it, which is where `lute.core` and the base language domains come from.
- **`extends`** names a single parent profile; the chain MUST be acyclic. Parents apply before children.
- **`plugins`** is a map from plugin id to a typed option object, or `true` (which normalizes to defaults). Presence of a legal key **activates** that plugin. There is no `plugins.use` list, and 0.0.1 has no scene-local *deactivation* — a `false` value is a static error (`E-PROFILE-PLUGIN-VALUE`), never an off switch. To exclude a plugin, do not inherit a profile that activates it.

## Selecting a profile in a scene

A scene names its profile in frontmatter (absent ⇒ `defaultProfile`) and MAY add scene-local `plugins:` — additive only. Here the scene narrows `allowedKinds` so `rhythm` is the only legal minigame kind:

```lute
---
kind: scene
character: bianca
season: 1
episode: 5
pov: fixer
profile: date-minigame
plugins:
  idola.minigame:
    resultScope: scene
    allowedKinds: [rhythm]
---
```

## Resolution & merge

Activation resolves deterministically in this exact order: `lute.core` → `global` → the selected profile's `extends` chain (parent first) → the selected profile → scene-local `plugins:` → the dependency closure. When the same plugin's options are set at multiple layers, later layers win: **scalar** values override, **map** values deep-merge, and **list** values replace by default. The result is exactly one option object per active plugin and exactly one [capability snapshot](/plugins/manifests/).

A reference to a directive, attribute, or id from an installed-but-**inactive** plugin is a diagnostic with fix-its ("change profile" / "activate plugin") — never silently accepted syntax.
