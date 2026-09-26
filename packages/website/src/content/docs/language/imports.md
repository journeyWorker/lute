---
title: Imports (uses:)
description: How a scene imports its shared state schema with uses:, the plain-YAML .schema.yaml declaration files, and the DAG discipline shared by uses / extends / components.
---

The `run`/`user`/`app` state schema is **game/season-global** — one persisted value cannot have a
different type per scene — so it lives in a single source-of-truth schema document that each scene
imports, rather than being redeclared everywhere. That import is the **`uses:`** frontmatter key.

```lute check="docs/examples/carry-ep.lute"
---
kind: scene
character: elena
season: 1
episode: 3
uses: state.schema.yaml
---

## Carrying the Choice Forward

@narrator: Previously, a choice was made.

<match on="run.choseHelp">
  <when is="true">
    @elena: Thanks for helping me back then.
  </when>
  <otherwise>
    @elena: ...
  </otherwise>
</match>
```

*(From [`docs/examples/carry-ep.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/carry-ep.lute).)* The scene reads `run.choseHelp`, which is declared in the imported
`state.schema.yaml` — not inline.

## Declaration files

A `.schema.yaml` file is a **plain YAML declaration map** — no `---` envelope, no body, just
`state:` / `defs:` (and, for the relational layer, `entities:` / `relations:` / `facts:` / `rules:`).
It carries no `character`/`season`/`episode`, because it is imported and validated in import mode,
never run as a scene.

```yaml
state:
  run.choseHelp: { type: bool, default: false }
  user.level:    { type: number, default: 1 }
defs:
  helped: "run.choseHelp"
  atLeast: { type: bool, params: { n: number }, cel: "user.level >= n" }
```

A def is its CEL body as a string — `helped: "run.choseHelp"` — and its type is inferred from that
body. Since dsl 0.26.0 §2.7 a body that is a `visited('…')` or `validAt(…)` call infers `bool`, so
`pierSeen: "visited('south.pier')"` needs no long form. A def with `params:`, or one whose type the
checker cannot infer, takes the long form `{ type: bool, cel: "…" }`; any other shape is
`E-DEF-DECL`. For a def in an imported schema, `E-DEF-DECL` is reported once, at the schema line,
with the other importers folded into `(+N more callers)`, rather than at every importing document.

Import paths are resolved **relative to the importing scene file**, so a scene and its schema must
travel together (copying a scene to `/tmp` without its schema reports `E-USES-NOT-FOUND`).

Only genuinely scene-local `scene.*` declarations may appear inline in a scene's `state:` block. A
scene must **not** redeclare or override an imported `run`/`user`/`app` path.

## The import DAG

`uses:`, [`extends:`](/language/components-and-extends/), and
[`components:`](/language/components-and-extends/) all share one import discipline — they form a
**directed acyclic graph**:

- **Cycles are a static error** — the diagnostic prints the offending chain (`E-USES-CYCLE`).
- Imported schemas are **loaded and checked before** any scene.
- **Duplicate `defs` names** across imports are an error — no silent shadowing.
- Import refs are **canonicalized**, so two paths to the same file are one schema identity, and a
  file reached by two routes (a diamond) is **deduped**, not double-declared.

`uses:` **unions peers**: a name declared by two peer schemas is a conflict. To *refine* rather than
union — overriding a base's declaration with your own — use `extends:` instead, whose
lower-precedence base layering and override rules are covered in
[Components & extends](/language/components-and-extends/).

```lute check="docs/examples/extends-demo.lute"
---
kind: scene
character: elena
season: 1
episode: 4
uses: child.schema.yaml
---
```

*(From [`docs/examples/extends-demo.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/extends-demo.lute), whose `child.schema.yaml` itself `extends:` a base — the DAG runs several files deep.)*

Missing files, cycles, and parse errors on any imported file — schema or component — surface through
the shared `E-USES-{NOT-FOUND,CYCLE,PARSE}` diagnostics.

## Project defaults

A project whose every document imports the same schemas says so once, under `defaults:` in
`lute.project.yaml`. Every document under the manifest inherits the keys, and `uses:` paths there
resolve against the manifest's directory:

```yaml
defaults:
  luteVersion: "0.25.1"
  uses:
    - schema/world.schema.yaml
    - schema/items.schema.yaml
    - schema/areas/*.schema.yaml
  questTier: run
```

Since dsl 0.26.0 §2.4:

- A `defaults.uses` entry may be a **glob** (`*`, `**`). It expands in path order, so each area of
  a project written by several authors keeps its own `schema/areas/<area>.schema.yaml`, and a new
  area needs no manifest edit. A glob that matches nothing, over an empty directory or one that
  does not exist yet (git keeps no empty directory), imports nothing and is no error. A literal
  path must exist.
- **`questTier: run | user`** sets the `tier=` of every `<quest>` that writes none (see
  [`<quest>`](/language/quests-and-scenes/#quest)). Without it the default stays `user`.

The files a glob brings in are peers like any other `uses:` import: a name two of them declare is
still a conflict, except that an entity kind may be [extended with `add:`](/state/schemas/#kinds-assembled-across-files-add).
See [Multi-author projects](/guides/multi-author/) for how the areas divide the schema.
