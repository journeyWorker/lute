---
title: Manifests & resolution
description: The lute.project.yaml profile graph, the plugin.yaml manifest and its export files, and how installed plugins resolve deterministically into one capability snapshot.
---

A plugin is a directory whose entry is a single `plugin.yaml`. Its `exports` map names which sub-directories the loader reads; any directory not listed is ignored. Everything is declarative YAML behind one plugin id — consumers reference the id only.

## `plugin.yaml` (manifest entry)

```yaml
id: idola.minigame          # REQUIRED — reverse-dotted, globally unique id
version: 0.1.0              # REQUIRED — the plugin's own semver
kind: capability           # REQUIRED — only "capability" is defined in 0.0.1
depends:                   # OPTIONAL — { id, range } against other plugins
  - { id: lute.core, range: "^0.0.1" }
exports:                   # REQUIRED — which sub-directories the loader reads
  directives: directives/
  state: state/
  providers: providers/
  bridge: bridge/
  assetkinds: assetkinds/
  defs: defs/
options:                   # OPTIONAL — typed activation options
  - { name: resultScope,  type: { enum: [scene, run] }, default: scene }
  - { name: allowedKinds, type: { list: { enum: [rhythm, puzzle, timing] } }, default: [rhythm, puzzle, timing] }
```

`depends[].range` is pinned to two forms only — caret (`^x.y.z`, pre-1.0 semantics) or an exact three-component version. Any other spelling is unsatisfiable by definition.

## Export files

Each export kind has a normative schema. All are typed by one small manifest type system (`bool` / `number` / `string`, `enum`, `list`, `record`, `map`, plus `enumFromOption`, `providerRef`, `slotId`, `assetKind`, and shape refs). State paths use **structured segments**, never `$name` interpolation.

- `directives/*.yaml` — `::name` directive declarations (see [Bridge](/plugins/bridge/)).
- `state/shapes.yaml` — reusable typed record shapes; `state/templates.yaml` — structured path templates.
- `providers/*.yaml` — id registries resolved against a pinned snapshot.
- `bridge/*.yaml` — typed runtime bridge capabilities.
- `defs/*.yaml` — shared typed-CEL `@refs`.
- `assetkinds/*.yaml` — asset-id segment templates (compose / query modes) with ordered `fallback` hooks.
- `enums/*.yaml`, `frontmatter/*.yaml`, `events/*.yaml`, `docs/*.md` — named enum domains, plugin-owned meta keys, world events, and hover docs.

## Installation & the profile graph

A project's `lute.project.yaml` declares `pluginsDir`, a `defaultProfile`, and a profile graph. A profile is a root-level capability selector; the reserved `global` profile is inherited by every other, and profiles compose via `extends`:

```yaml
pluginsDir: plugins/
defaultProfile: date-minigame
profiles:
  global:       { plugins: { lute.core: true } }
  story:        { plugins: { idola.minigame: true } }
  date:         { extends: story }
  date-minigame:
    extends: date
    plugins:
      idola.minigame: { resultScope: scene, allowedKinds: [rhythm, timing] }
```

`plugins` is a **map** from plugin id to a typed option object (or `true`, normalizing to defaults). Presence of a legal key **activates** the plugin — there is no `plugins.use` list. See [Profiles](/plugins/profiles/) for selection and merge rules.

## Resolution & the capability snapshot

Given the same installed plugins, selected profile, and scene frontmatter, resolution produces a **byte-identical** capability snapshot. It applies, in exact order: `lute.core` → `profiles.global` → the selected profile's `extends` chain (parent first) → the selected profile → scene-local `plugins:` → the dependency closure. Scalar options override, maps deep-merge, lists replace.

The snapshot is one immutable artifact carrying `plugins`, `enums`, `providers`, `stateShapes`, `stateTemplates`, `assetKinds`, `directives`, `bridgeCapabilities`, `frontmatter`, `events`, `diagnostics`, and more. Its `capabilityVersion` is a content hash over the resolved plugin ids+versions, their options, the active profile, and the bound provider snapshot versions. Every generated artifact is stamped with the `capabilityVersion` it targets, and a consumer refuses mismatched stamps. Providers are **snapshot-first**: the compiler fails if required catalog data is missing but never blocks on the network, and the LSP keeps a stale snapshot with a *catalog-stale* diagnostic rather than false *unknown-id* errors.
