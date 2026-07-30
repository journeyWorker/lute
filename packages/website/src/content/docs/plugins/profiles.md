---
title: Profiles & activation
description: Selecting a capability profile in scene frontmatter, the reserved global profile and extends inheritance, layering scene-local plugin options, and how those options are type-checked at activation.
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

- **`global` is reserved** and is applied before any other profile — every profile inherits it, which is where `lute.core`'s directives, attributes, and vocabulary *slots* come from. Since `0.9.0` it is only the slots: `lute.core` ships an empty `enums:`, so the [members are the project's](/language/vocabulary/) to declare.
- **`extends`** names a single parent profile; the chain MUST be acyclic. Parents apply before children.
- **`plugins`** is a map from plugin id to a typed option object, or `true` (which normalizes to defaults). Presence of a legal key **activates** that plugin. There is no `plugins.use` list, and there is no scene-local *deactivation* — a `false` value is a static error (`E-PROFILE-PLUGIN-VALUE`), never an off switch. To exclude a plugin, do not inherit a profile that activates it.

## Selecting a profile in a scene

A scene names its profile in frontmatter (absent ⇒ `defaultProfile`) and MAY add scene-local `plugins:` — additive only. Here the scene narrows `allowedKinds` so `rhythm` is the only legal minigame kind:

```lute check="docs/examples/idola-project/date-minigame.lute"
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

*(Frontmatter of [`docs/examples/idola-project/date-minigame.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/idola-project/date-minigame.lute), whose [`lute.project.yaml`](https://github.com/journeyWorker/lute/blob/main/docs/examples/idola-project/lute.project.yaml) declares the profile graph above.)*

## Resolution & merge

Activation resolves deterministically in this exact order: `lute.core` → `global` → the selected profile's `extends` chain (parent first) → the selected profile → scene-local `plugins:` → the dependency closure. When the same plugin's options are set at multiple layers, later layers win: **scalar** values override, **map** values deep-merge, and **list** values replace by default. The result is exactly one option object per active plugin and exactly one [capability snapshot](/plugins/manifests/).

A reference to a directive, attribute, or id from an installed-but-**inactive** plugin is a diagnostic with fix-its ("change profile" / "activate plugin") — never silently accepted syntax.

## Options are checked, not just merged

Activation validates every supplied option against the owning plugin's `options:` declaration. The check runs on the **post-merge** value, so only the final layered value is judged — a bad value in a parent profile that a child overrides is never reported, and a bad final value is reported once no matter how many layers set it.

An option name the plugin does not declare is **`E-PLUGIN-OPTION-UNKNOWN`**, and the message names the whole declared set so the fix is usually a typo away:

<!-- lute-diagnostics -->
```
$ lute check scene.lute --project .
lute: E-PLUGIN-OPTION-UNKNOWN: plugin `idola.bonus` has no option `resultScop` (declared: resultScope, rounds)
```

A value that fails its declared type is **`E-PLUGIN-OPTION-TYPE`**. Every violation in the profile is collected in one pass rather than bailing on the first:

<!-- lute-diagnostics -->
```
$ lute check scene.lute --project .
lute: E-PLUGIN-OPTION-TYPE: option `idola.bonus.resultScope` expects enum(scene|run), got "galaxy"
lute: E-PLUGIN-OPTION-TYPE: option `idola.bonus.rounds` expects number, got "three"
```

Both describe the project rather than a span in any one file, and both are errors: they print without a file position and they set the CLI exit code. They travel the same resolution channel as load errors and unresolved `depends`, so the CLI, the checker, and the LSP observe them identically.

An option no layer sets is absent from the merge and takes its declared default, which is not re-checked — the declaration is the source of both the type and the value.
