---
title: Manifests & resolution
description: The plugin.yaml manifest and its export files, cross-cutting stampAttrs, declarative lowering to core staging records, and how installed plugins resolve deterministically into one capability snapshot.
---

A plugin is a directory whose entry is a single `plugin.yaml`. Its `exports` map names which sub-directories the loader reads; any directory not listed is ignored. Everything is declarative YAML behind one plugin id — consumers reference the id only.

## `plugin.yaml` (manifest entry)

```yaml
id: idola.minigame          # REQUIRED — reverse-dotted, globally unique id
version: 0.1.0              # REQUIRED — the plugin's own semver
kind: capability           # REQUIRED — only "capability" is defined
depends:                   # OPTIONAL — { id, range } against other plugins
  - { id: lute.core, range: "^0.0.1" }
exports:                   # REQUIRED — which sub-directories the loader reads
  directives: directives/
  state: state/
  providers: providers/
  bridge: bridge/
  assetkinds: assetkinds/
  defs: defs/
  stampattrs: stampattrs/
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
- `stampattrs/*.yaml` — cross-cutting attributes admissible on every directive and content line (below).
- `enums/*.yaml`, `frontmatter/*.yaml`, `events/*.yaml`, `docs/*.md` — named enum domains, plugin-owned meta keys, world events, and hover docs.

`enums/` is the third route a project gets its [content vocabulary](/language/vocabulary/) from, and the only one that is *capability* rather than project data: `lute.core` declares the seven slots and exports an **empty** `enums`, so an engine or genre pack ships members to every project that activates it. Its entries take the same long form as an author's `enums:` block — a bare sequence is shorthand for `{ members: [...] }`, and `action` must carry `exits:` while `anchor` must carry `default:`.

## Cross-cutting attributes (`stampAttrs`)

Ordinary attributes are declared per directive. An engine that tags *every* record with the same key — an analytics id, an experiment bucket, a bonus hook — had no declaration site for one, and could never put it on a content line at all. `stampattrs/*.yaml` is that site:

```yaml
stampAttrs:
  - { name: bonusId,    type: string }
  - { name: bonusScore, type: number }
```

Entries are ordinary `AttrDecl`s — the same `{ name, required?, type, default? }` shape a directive attr uses — but they are admissible on **every** directive *and* on content lines (`@speaker{…}: text`), on top of that surface's own attributes. Resolution is strict: the surface's own declarations win, then `stampAttrs`, then `E-UNKNOWN-ATTR`. Value typing rides the existing attribute path, so a mistyped one is a plain `E-ATTR-TYPE` / `E-BAD-ENUM` — no new rules.

`::sfx{sound="chime" bonusId="b-02"}` followed by `@bianca{code="0010" bonusId="b-01" bonusScore="7"}: Welcome back.` compiles to two records that each carry the attribute **flattened into the record's stamp**, beside the reserved timing keys — never in the record's own `fields`:

```json
{ "kind": "sfx", "addr": "001-0100", "sound": "chime", "bonusId": "b-02" }
{ "kind": "line", "addr": "001-0200", "role": "dialogue", "speaker": "bianca",
  "text": "Welcome back.", "lineId": "bianca.s01ep01.bianca_0010",
  "voiceKey": "bianca-0010", "bonusId": "b-01", "bonusScore": 7.0 }
```

An **unauthored** stamp attribute is not injected — not even when its declaration carries a `default`. Absent means absent, so declaring a cross-cutting vocabulary and authoring none of it leaves the artifact byte-identical. The declaration is not free, though: `stampAttrs` participates in `capabilityVersion`, because a changed cross-cutting vocabulary is a changed capability surface and an engine must be able to refuse the mismatch.

### Reserved stamp keys

The core stamp owns seven names — `at`, `duration`, `delay`, `wait`, `timeline`, `provenance`, `source`. A plugin declaring an attribute under any of them is rejected at assembly with **`E-PLUGIN-RESERVED-STAMP-ATTR`**:

```
$ lute check scene.lute --project .
lute: E-PLUGIN-RESERVED-STAMP-ATTR: plugin `idola.bonus` declares reserved stamp attribute `duration`; `at`/`duration`/`delay`/`wait`/`timeline`/`provenance`/`source` are owned by the core stamp (plugin §14)
```

Both surfaces are covered — the `stampAttrs` export *and* an ordinary per-directive `attrs` entry — so a plugin cannot reach a reserved key through either door. The offending declaration is dropped rather than merged; its non-reserved siblings still land. This is why a blocking plugin directive names its own flag `sync` and not `wait` (see [Bridge](/plugins/bridge/)).

## Declarative lowering

A directive's `lower:` says what the compiler emits for it. There are two forms:

```yaml
lower: { record: <kind>, fields: { … } }   # a finite attrs → one core record
lower: { kind: builtin, name: <hook> }     # a named core hook
```

The `record` form targets one of the eight **non-control-flow staging kinds** — `background`, `music`, `sfx`, `vfx`, `sprite`, `camera`, `cut`, `video` — binding each target field to a `fromAttr` reference or a literal:

```yaml
directives:
  - name: backdrop
    attrs:
      - { name: img,  required: true, type: string }
      - { name: when, type: string }
    lower:
      record: background
      fields:
        assetId: { fromAttr: img }
        time:    { fromAttr: when }
```

`::backdrop{img="bg.lounge" when="night"}` then compiles to a real `background` record — not a `kind: "plugin"` passthrough:

```json
{ "kind": "background", "addr": "001-0100", "time": "night", "assetId": "bg.lounge", "wait": true }
```

The emitted record inherits the **target kind's** `wait` default (`background` and `video` block, `cut` and `camera` do not, the rest omit the key), so it is indistinguishable from the core directive an author could have written by hand. An optional source attribute that was not authored leaves its target field absent.

Those eight are the whole vocabulary, and the exclusion is principled rather than a shortlist: control-flow kinds (`jump`, `choice`, `match`, `hub`, `barrier`, `end`, `quest`, `on`) carry addresses the compiler's own passes resolve, and content kinds (`line`) carry identity — `lineId` / `voiceKey` — derived from the authored `code`. Neither is a finite attrs→fields mapping, so neither is data.

Both failures are caught at **assembly**, before anything is lowered — a declaration that fails validation never reaches the compiler:

- **`E-LOWER-RECORD-UNKNOWN`** — `record:` names something outside the eight.
- **`E-LOWER-RECORD-FIELD`** — a target field the record kind does not have, or a `fromAttr` naming an attribute the directive never declares.

```
$ lute check scene.lute --project .
lute: E-LOWER-RECORD-FIELD: directive `::backdrop` lowers to record `background`: unknown target field `mood` (record `background` binds: location, time, assetId)
lute: E-LOWER-RECORD-UNKNOWN: directive `::sting` lowers to unknown record `line`; declarative lowering targets the staging kinds (background, music, sfx, vfx, sprite, camera, cut, video)
```

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

The snapshot is one immutable artifact carrying `plugins`, `enums`, `providers`, `stateShapes`, `stateTemplates`, `assetKinds`, `directives`, `bridgeCapabilities`, `frontmatter`, `events`, `stampAttrs`, `diagnostics`, and more. Its `capabilityVersion` is a content hash over that whole resolved surface — plugin ids+versions and their merged option objects, and every directive, enum, provider, state shape, bridge capability, def, frontmatter key and stamp attribute in it. Any drift in a populated field yields a different version, and adding a directive to `lute.core` moves it for every project. Every generated artifact is stamped with the `capabilityVersion` it targets, and a consumer refuses mismatched stamps. Providers are **snapshot-first**: the compiler fails if required catalog data is missing but never blocks on the network, and the LSP keeps a stale snapshot with a *catalog-stale* diagnostic rather than false *unknown-id* errors.
