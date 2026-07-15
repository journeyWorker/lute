---
title: Components & extends
description: Two reuse mechanisms — reusable content components invoked with ::use, and extends schema composition with base-layer override precedence.
---

Lute has three reuse mechanisms, each for a different thing: `defs` reuse typed CEL *values*,
`uses`/`extends` reuse *schema*, and **components** reuse *content*. This page covers content
components and schema `extends:` composition.

## Reusable content components

A **component** is a named, parameterized block of lines and staging that is expanded inline
wherever it is invoked. It lives in its own **component file** — a `.lute` document whose
frontmatter declares `component: <name>` and, optionally, `params:` (typed exactly like a
[def param](/language/params/)). The body is a **presentational template**.

```lute
---
component: greet
params:
  who: string
---

## Scene 1.

::auto{character=@who action="fade-in-up"}
@narrator: A familiar face steps into the light.
```

*(From [`docs/examples/components/greet.component.lute`](https://github.com/KantoRegion/lute/blob/main/docs/examples/components/greet.component.lute).)*

A parameter is referenced as `@<param>` in ref and attribute positions, and inside content text via
`{{@param}}` interpolation. `@who` binds to the invocation argument at expansion time — it is legal
in the `character` position only because that attribute is `string`-typed.

A scene imports components via a `components:` frontmatter key (canonicalized, cycle-checked, and
diamond-deduped like `uses:` — see [Imports](/language/imports/)), then invokes one with the
reserved built-in directive **`::use`**:

```lute
---
kind: scene
character: demo
season: 1
episode: 2
components: [greet.component.lute]
---

## Shot 1.

::use{component="greet" who="bianca"}
@narrator: And the scene carries on.
```

*(From [`docs/examples/components/scene.lute`](https://github.com/KantoRegion/lute/blob/main/docs/examples/components/scene.lute).)*

`::use` expands the named component's body inline, binding each `@param` to the matching named arg;
argument count and type are checked (`E-COMPONENT-ARG`), and naming a component from no imported
file is `E-COMPONENT-UNDECLARED`.

### Component body rules

A component body is **presentational**: lines, staging directives, and `@param` refs only. It may
**not** read or write scene/run state and may **not** contain logic blocks (`E-COMPONENT-BODY`) —
pass values in through params instead. One notable exception: a `<match>` that dispatches on the
component's own param is admitted, because dispatch on a param is a pure read of an invocation
argument, not of ambient state:

```lute
---
component: reaction
params:
  tier: { enum: [cold, warm, fond] }
---

## Scene 1.

<match on="@tier">
  <when is="fond"> @bianca{emotion="delighted"}: You remembered! </when>
  <when is="warm"> @bianca{emotion="content"}: Not bad at all, Mr. Fixer. </when>
  <when is="cold"> @bianca{emotion="neutral"}: ...Shall we begin? </when>
</match>
```

*(From [`docs/examples/components/reaction.component.lute`](https://github.com/KantoRegion/lute/blob/main/docs/examples/components/reaction.component.lute).)* The three arms cover the declared
enum and a param is never `unset`, so no `<otherwise>` is needed.

## Schema `extends:`

Where `uses:` unions **peer** schemas (a name declared by two peers is an error), **`extends:`**
names one or more **base** schemas that a document *refines*. A base is a lower-precedence layer.

```yaml
# base.schema.yaml
state:
  run.blessed: { type: bool, default: false }
defs:
  wealthy: { type: bool, cel: "run.blessed" }
```

```yaml
# child.schema.yaml
extends: base.schema.yaml
state:
  run.blessed: { type: bool, default: true }   # overrides the base default
```

*(From [`docs/examples/child.schema.yaml`](https://github.com/KantoRegion/lute/blob/main/docs/examples/child.schema.yaml) and [`base.schema.yaml`](https://github.com/KantoRegion/lute/blob/main/docs/examples/base.schema.yaml).)*

Precedence, low → high: a document's `extends` bases (recursively) < its `uses` peers < its own
inline `state:`/`defs:`. When the extending layer redeclares a base name, it **overrides** it — no
duplicate error. A `defs` entry is replaced wholesale. A `state` entry is overridden too, but
because persisted state must keep a stable type, an override that changes the declared **type** is
`E-EXTENDS-STATE-TYPE`; a `default`-only refinement (same type) is allowed silently. `extends` edges
share the same DAG discipline as `uses:` — cycles, missing files, and parse errors reuse the
`E-USES-*` diagnostics.
