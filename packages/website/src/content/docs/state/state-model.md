---
title: The state model
description: Lute's tiered scalar state — the run, user, and app lifetime namespaces (plus episode-local scene), how paths are declared, and the path-sensitive definite-assignment rules that govern reads and writes.
---

Lute scalar state is a set of typed paths (`number`, `bool`, `string`, `enum`) grouped into **namespaces named by their reset boundary** — the moment the engine clears them. There are four tiers on one axis (*when does it reset?*):

| Namespace | Reset boundary | Typical use |
|---|---|---|
| `scene.*` | episode end (one `.lute` document; survives across its shots) | on-stage state, `scene.choices.*`, `scene.visited.*` |
| `run.*` | new run — one attempt, a sequence of episodes | per-attempt flags, affect, cross-episode carry within an attempt |
| `user.*` | profile/account wipe — survives runs | level, unlocks, meta-progression |
| `app.*` | app uninstall — identity-independent | language, age rating, settings |

The engine **owns and fires every reset**; the language never triggers one. The three persistent tiers — `run` / `user` / `app` — are game/season-global, so they live in a single shared schema document that scenes import with `uses:` (see [State schemas](/state/schemas/)). Only genuinely episode-local `scene.*` declarations may appear inline in a scene, and a scene MUST NOT redeclare or override an imported tier.

## Declaration

Every path read *or written* MUST be declared with a `type` and an optional `default`. There are no bare, un-namespaced state names.

```yaml
state:
  scene.affect.sofia: { type: number, default: 0 }
  run.choseHelp:      { type: bool,   default: false }
  user.level:         { type: number, default: 1 }
  app.rating:         { type: { enum: [teen, adult] }, default: teen }
```

Author `state:` is **scalar-only** — `number`, `bool`, `string`, or `enum`, and nothing else. An `enum` path nests its members inside `type:` — `{ type: { enum: [teen, adult] } }`. A sibling `values:` key is not a declaration key at all: `{ type: enum, values: [teen, adult] }` is `E-STATE-DECL` on the schema, and a document that `uses:` that schema fails with `E-USES-PARSE` carrying the `E-STATE-DECL` beneath it. [`docs/examples/showcase/schema/base.schema.yaml:9`](https://github.com/journeyWorker/lute/blob/main/docs/examples/showcase/schema/base.schema.yaml) is the same declaration, written correctly. A declaration whose `type` is `list`, `record`, or `map` is `E-STATE-COLLECTION`, and the declaration is *not installed*: a later read of that path reports a plain `E-UNDECLARED` rather than resolving against a phantom collection-typed slot.

Enforcement is new in 0.8.0, but it removes an ambiguity rather than adding a restriction. The normative text always said scalar, but the shape validator accepted the whole type union and the runtime documentation described `list<…>` / `map<…>` / `record` as valid entry types — three sources, three answers. All three now agree. Collections were always meant to be modelled **relationally**: an inventory is `ownsItem(item)`, not a `list<string>`, so reach for [`relations:`](/state/facts-and-datalog/) instead. Collection-shaped entry types do still reach the compiled artifact, but only through a plugin `state_shapes` expansion — never from an author's `state:` block.

## Reads and writes

`::set{path <op> celExpr}` writes one path per directive (`=`, `+=`, `-=`, `*=`). Writes target `scene.*` / `run.*` / `user.*`; `app.*` is **content-read-only** (the settings layer owns it — `::set{app.*}` is a static error).

Definite assignment is **path-sensitive**. Reading an undeclared path is `E-UNDECLARED`. A `scene.*` read follows ordinary flow analysis. A `run`/`user`/`app` path is **maybe-unset at scene entry** unless it carries a schema `default`; after entry, a dominating `::set{p = …}` write or a guard (`has(p)` / `isSet(p)`) proves it — otherwise the read is `E-MAYBE-UNSET`. A compound assignment (`+=`/`-=`/`*=`) reads the old value first, so only `=` may be a path's first write. A defaulted path is always assigned; the checker and engine share the one schema snapshot, so they can never disagree.
