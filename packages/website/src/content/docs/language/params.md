---
title: Definitions & params
description: Named typed CEL macros declared in defs, referenced as @name, and parameterized as @name(args) — the reusable-value layer of the language.
---

All conditions and `::set` right-hand sides in Lute are [CEL](https://cel.dev) expressions. Rather
than repeat a condition everywhere it is needed, you name it once as a **def** and reference it as
`@name`. Defs are the language's reusable-*value* layer (distinct from schema reuse via
[imports](/language/imports/) and content reuse via [components](/language/components-and-extends/)).

## Declaring a def

`defs` declares named, typed CEL values, either inline in a scene's frontmatter or in an imported
schema. Each entry has a `type`, an optional `params` block, and a `cel` body:

```yaml
defs:
  warm:    { type: bool,   cel: "scene.affect.sofia >= 2" }
  closeUp: { type: number, cel: "scene.affect.sofia >= 5 ? 1.35 : 1.15" }
  fond:    { type: bool,   cel: "scene.affect.bianca >= 1" }
```

A def is referenced with `@name`. Because `@` is a **compile-time macro**, the reference is expanded
to its inline CEL before evaluation — a def is not a runtime function call, just a named piece of
CEL. A bool def reads as a guard; a number def reads as a staging value:

```lute
<when test="@fond"> @fixer{mono}: I asked nicely. </when>
::camera{zoom="@closeUp"}
```

A `@ref` must appear in a position whose required type matches the def's declared `type`, and its
name must be declared in `defs`. Def names and param names are CEL identifiers (no `-`).

## Parameterized defs

A def may declare typed **`params`**, turning it into a parameterized macro invoked as
`@name(args)`. The arguments are bound to the params at expansion time:

```yaml
defs:
  atLeast: { type: bool, params: { n: number }, cel: "user.level >= n" }
  chose:   { type: bool, params: { q: choiceRef, opt: choiceId }, cel: "scene.choices[q] == opt" }
```

*(From [`docs/examples/showcase/schema/base.schema.yaml`](https://github.com/KantoRegion/lute/blob/main/docs/examples/showcase/schema/base.schema.yaml).)*

```lute
<match on="scene.affect.bianca">
  <when test="@atLeast(3)"> @fixer{mono}: A veteran's welcome. </when>
  <otherwise>              @fixer{mono}: Early days yet. </otherwise>
</match>
```

*(Call site from [`docs/examples/showcase/episode01.lute`](https://github.com/KantoRegion/lute/blob/main/docs/examples/showcase/episode01.lute); a minimal parameterized def is in [`docs/examples/param-def.lute`](https://github.com/KantoRegion/lute/blob/main/docs/examples/param-def.lute).)*

## Where defs live

A def whose CEL reads `run`/`user`/`app` state belongs in the shared schema document, so it can be
imported by every scene that needs it (and refined via [`extends:`](/language/components-and-extends/)).
A def that reads only `scene.*` can live inline in the scene. Duplicate def names across imports are
a static error — no silent shadowing.

## What the checker verifies

The static checker owns four `@ref` checks, each conservative (only provably-wrong cases flag):

- the name is declared in `defs` — else `E-UNDECLARED-REF`;
- the call arity matches the declared `params` — else `E-REF-ARITY`;
- each statically-resolvable argument matches its param's type — else `E-REF-ARG-TYPE`;
- a whole-slot `@ref` produces the position's required type — else `E-REF-TYPE`.

The `min`/`max`/`values` def fields from earlier drafts are removed: a def is fully described by its
`type`, optional `params`, and `cel`.
