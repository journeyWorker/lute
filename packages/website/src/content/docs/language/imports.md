---
title: Imports (uses:)
description: How a scene imports its shared state schema with uses:, the plain-YAML .schema.yaml declaration files, and the DAG discipline shared by uses / extends / components.
---

The `run`/`user`/`app` state schema is **game/season-global** — one persisted value cannot have a
different type per scene — so it lives in a single source-of-truth schema document that each scene
imports, rather than being redeclared everywhere. That import is the **`uses:`** frontmatter key.

```lute
---
kind: scene
character: sofia
season: 1
episode: 3
uses: state.schema.yaml
---

## Shot 1.

@narrator: Previously, a choice was made.

<match on="run.choseHelp">
  <when test="$ == true"> @sofia: Thanks for helping me back then. </when>
  <otherwise>             @sofia: ... </otherwise>
</match>
```

*(From [`docs/examples/carry-ep.lute`](https://github.com/KantoRegion/lute/blob/main/docs/examples/carry-ep.lute).)* The scene reads `run.choseHelp`, which is declared in the imported
`state.schema.yaml` — not inline.

## Declaration files

A `.schema.yaml` file is a **plain YAML declaration map** — no `---` envelope, no body, just
`state:` / `defs:` (and, for the relational layer, `entities:` / `relations:` / `facts:` / `rules:`).
It carries no `character`/`season`/`episode`, because it is imported and validated in import mode,
never run as a scene.

```yaml
state:
  run.choseHelp: { type: bool, default: false }
```

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

```lute
---
kind: scene
character: sofia
season: 1
episode: 4
uses: child.schema.yaml
---
```

*(From [`docs/examples/extends-demo.lute`](https://github.com/KantoRegion/lute/blob/main/docs/examples/extends-demo.lute), whose `child.schema.yaml` itself `extends:` a base — the DAG runs several files deep.)*

Missing files, cycles, and parse errors on any imported file — schema or component — surface through
the shared `E-USES-{NOT-FOUND,CYCLE,PARSE}` diagnostics.
