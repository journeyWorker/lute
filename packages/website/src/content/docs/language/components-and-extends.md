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

```lute check="docs/examples/components/greet.component.lute"
---
component: greet
params:
  who: string
uses: ../base.schema.yaml
---

## A Familiar Face

::auto{character=@who action="fade-in-up"}
@narrator: A familiar face steps into the light.
```

*(From [`docs/examples/components/greet.component.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/components/greet.component.lute).)*

A parameter is referenced as `@<param>` in ref and attribute positions, and inside content text via
`{{@param}}` interpolation. `@who` binds to the invocation argument at expansion time — it is legal
in the `character` position only because that attribute is `string`-typed.

Not every param type is renderable. A `{{…}}` interpolation renders **number, bool or enum** only
(§7.6); a `string` param inside content text is `E-REF-TYPE` — *"`@who` produces a non-renderable
type; a `{{…}}` interpolation renders only number/bool/enum"*. So `who: string` above is usable as
the `character=` argument it is written for and **not** as `{{@who}}` in a line. That restriction
is under review. Until it changes, two idioms cover the need: take an **enum** param and `<match>`
on it to pick between lines written in the component, or keep the varying text in the calling
document and use the component for the staging around it.

The `uses:` line is the component's own [content vocabulary](/language/vocabulary/) import — since
`0.9.0` `action="fade-in-up"` resolves against a declared `action` domain, and a component file has
to reach one to check on its own. Through `::use` it is the **importing** document's vocabulary that
applies (see [the known limitation](/language/vocabulary/#known-limitation-a-component-body-resolves-against-the-importing-document)), so both sides declare it.

A scene imports components via a `components:` frontmatter key (canonicalized, cycle-checked, and
diamond-deduped like `uses:` — see [Imports](/language/imports/)), then invokes one with the
reserved built-in directive **`::use`**:

```lute check="docs/examples/components/scene.lute"
---
kind: scene
character: demo
season: 1
episode: 2
uses: ../base.schema.yaml
components: [greet.component.lute]
---

## Greeting by Component

::use{component="greet" who="marina"}
@narrator: And the scene carries on.
```

*(From [`docs/examples/components/scene.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/components/scene.lute).)*

`::use` expands the named component's body inline, binding each `@param` to the matching named arg;
argument count and type are checked (`E-COMPONENT-ARG`), and naming a component from no imported
file is `E-COMPONENT-UNDECLARED`. A literal argument is substituted into the expansion, so
`{{@weather}}` in a component line compiles to `Outside: grey.` for `weather="grey"`.

An argument may also be a **def of the calling document** — `::use{component="greet" tier=@mood}`.
The component still reads no state itself; the caller's def is where the state read lives, and it
is checked where it lands:

- for an enum param, every value the def body can produce must be a member, else `E-COMPONENT-ARG`
  (`@mood` = `"run.n > 2 ? 'warm' : 'hot'"` against `{ enum: [cold, warm] }` fails on `hot`);
- in a `<match on="@tier">`, it dispatches at run time like any subject;
- in content text, `{{@n}}` stays a placeholder naming the caller's def, and the engine evaluates it;
- in a directive attribute, it must fold to a constant, and a state-dependent def is
  `E-ATTR-DEF-DYNAMIC`.

### Component body rules

A component body is **presentational**: lines, staging directives, and `@param` refs only. It may
**not** read or write scene/run state and may **not** contain logic blocks (`E-COMPONENT-BODY`) —
pass values in through params instead (a caller's def is a legal argument, above). One notable
exception: a `<match>` that dispatches on the component's own param is admitted, because dispatch on
a param is a pure read of an invocation argument, not of ambient state:

```lute check="docs/examples/components/reaction.component.lute"
---
component: reaction
params:
  tier: { enum: [cold, warm, fond] }
uses: ../base.schema.yaml
---

## The Tiered Greeting

<match on="@tier">
  <when is="fond">
    @marina{emotion="delighted"}: You remembered!
  </when>
  <when is="warm">
    @marina{emotion="content"}: Not bad at all, Mr. Fixer.
  </when>
  <when is="cold">
    @marina{emotion="neutral"}: ...Shall we begin?
  </when>
</match>
```

*(From [`docs/examples/components/reaction.component.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/components/reaction.component.lute).)* The three arms cover the declared
enum and a param is never `unset`, so no `<otherwise>` is needed.

### Line identity

Each `::use` expansion is its own identity scope (0.22.0). A line expanded from a component is
addressed `{prefix}.{component}#{n}.{speaker}_{code}`, where `{prefix}` is the host document's
key and `n` counts the host's `::use`s of that component — 1-based, in document order. The scene
above compiles the component's narrator line to `demo.s01ep02.greet#1.narrator_0010` and its own
`@narrator: And the scene carries on.` to `demo.s01ep02.narrator_0010`. A voiced line's default
`voiceKey` takes the same scope (see [Frontmatter & profiles](/language/frontmatter-and-profiles/)
for the `identity:` templates).

Take a component with one tagged and one untagged line:

```lute check
---
component: toast
---

## A Toast

@mira{code="0010"}: To the harbor.
@oskar: To the harbor.
```

A scene with `id: harbor.dinner` imports it (`components: [toast.component.lute]`) and uses it
twice, between lines of its own:

```lute
@mira{code="0010"}: Sit, everyone.
::use{component="toast"}
@oskar: Again?
::use{component="toast"}
@mira: Last one.
```

Every line gets its own `lineId`:

| Line | `lineId` |
|---|---|
| `@mira{code="0010"}: Sit, everyone.` | `harbor.dinner.mira_0010` |
| first `toast` | `harbor.dinner.toast#1.mira_0010`, `harbor.dinner.toast#1.oskar_0010` |
| `@oskar: Again?` | `harbor.dinner.oskar_0010` |
| second `toast` | `harbor.dinner.toast#2.mira_0010`, `harbor.dinner.toast#2.oskar_0010` |
| `@mira: Last one.` | `harbor.dinner.mira_0020` |

- **Each expansion back-fills its own untagged codes.** The component's `@oskar` is `0010` at every
  use, however many host lines precede it, and the host's own untagged lines keep the codes
  `lute tag` writes for them (`@oskar: Again?` → `0010`, `@mira: Last one.` → `0020`).
- **Codes no longer collide across the boundary.** Two uses of a tagged component, or a component
  line sharing a code with a host line (both `@mira` lines above are `0010`), compile clean with
  distinct ids. Before 0.22.0 both were `E-DUP-LINE-CODE`.
- **A nested `::use` adds a segment.** It counts within its enclosing expansion: a `feast`
  component that uses `toast`, itself used twice, gives `…feast#1.toast#1.mira_0010` and
  `…feast#2.toast#1.mira_0010`.
- **`lute loc export` emits the same ids.**

Migrating from 0.21: every line a component contributes has a new `lineId` and `voiceKey`, so
re-export the localization and voice manifests of each document that `::use`s a component.

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

*(From [`docs/examples/child.schema.yaml`](https://github.com/journeyWorker/lute/blob/main/docs/examples/child.schema.yaml) and [`base.schema.yaml`](https://github.com/journeyWorker/lute/blob/main/docs/examples/base.schema.yaml).)*

Precedence, low → high: a document's `extends` bases (recursively) < its `uses` peers < its own
inline `state:`/`defs:`. When the extending layer redeclares a base name, it **overrides** it — no
duplicate error. A `defs` entry is replaced wholesale. A `state` entry is overridden too, but
because persisted state must keep a stable type, an override that changes the declared **type** is
`E-EXTENDS-STATE-TYPE`; a `default`-only refinement (same type) is allowed silently. `extends` edges
share the same DAG discipline as `uses:` — cycles, missing files, and parse errors reuse the
`E-USES-*` diagnostics.
