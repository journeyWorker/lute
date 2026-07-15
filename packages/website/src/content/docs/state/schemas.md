---
title: State schemas
description: The shared, imported source of truth for run/user/app state and defs — the schema document shape, and how uses composes peers while extends refines a base layer.
---

The `run` / `user` / `app` tiers are game/season-global: one persisted value cannot carry per-scene types. So they live in a single **schema document**, the source of truth every scene imports with `uses:`. Scenes then declare only their own `scene.*` locals. Since 0.2.2 a declaration file is plain `.yaml` (no `---`/Lute envelope) — a pure declaration, not a scene.

## Schema shape

A schema declares `state:` (scalar tiers), `defs:` (named typed-CEL macros), and — when the relational layer is used — `entities:` / `enums:` / `relations:` / `facts:` / `rules:`.

```yaml
state:
  run.choseHelp: { type: bool, default: false }
  user.level:    { type: number, default: 1 }
defs:
  helped: { type: bool, cel: "run.choseHelp" }
```

Each `<path>` segment is a CEL-facing identifier (no `-`). A `default` is materialized into the tier's initial state at schema load **and** re-materialized whenever the engine fires that tier's reset — so a defaulted path is always assigned, and the checker and engine read the one snapshot.

## Composition: `uses` and `extends`

Imports form a **DAG**: cycles are a static error (the diagnostic prints the chain), schemas are loaded and checked before any scene, duplicate `defs` names across imports are an error (no silent shadow), and two paths to one file resolve to one identity.

`uses:` unions **peer** schemas — a name declared by two peers is a conflict. `extends:` names one or more **base** schemas this document refines: a base is a lower-precedence layer, so if the extending document (or its peers) redeclares a base name, the extending declaration **overrides** it — no duplicate error.

```yaml
# base.schema.yaml
state:
  run.blessed: { type: bool, default: false }
```

```yaml
# child.schema.yaml — refines the base
extends: base.schema.yaml
state:
  run.blessed: { type: bool, default: true }   # default-only override
```

Precedence runs low → high: a document's `extends` bases (recursively) < its `uses` peers < its own inline `state:`/`defs:`. Because persisted state must keep a stable type, an override that changes a path's declared `type` is `E-EXTENDS-STATE-TYPE`; a `default`-only refinement of the same type is allowed silently. `extends` edges reuse the same cycle / missing-file / parse diagnostics (`E-USES-{CYCLE,NOT-FOUND,PARSE}`) as `uses:`.

The state schema is *game content* — separate from the engine **capability manifest** (engine vocabulary), which has its own owner and change cadence.
