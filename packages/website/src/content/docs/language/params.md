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
  warm:    { type: bool,   cel: "scene.affect.elena >= 2" }
  closeUp: { type: number, cel: "1.35" }
  fond:    { type: bool,   cel: "scene.affect.marina >= 1" }
```

A def is referenced with `@name`. Because `@` is a **compile-time macro**, the reference is expanded
to its inline CEL before evaluation — a def is not a runtime function call, just a named piece of
CEL. A bool def reads as a guard; a number def reads as a staging value:

```lute
<when test="@fond">
  @fixer{mono}: I asked nicely.
</when>
::camera{zoom=@closeUp}
```

A `@ref` must appear in a position whose required type matches the def's declared `type`, and its
name must be declared in `defs`. Def names and param names are CEL identifiers (no `-`).

In a directive attribute the ref is **bare** — `zoom=@closeUp`. A quoted `zoom="@closeUp"` is the
literal string `@closeUp`, which a number attribute rejects as `E-ATTR-TYPE`. An attribute value is
a constant, not an expression, so the compiler writes the literal the def folds to (`zoom: 1.35`) and
checks it like an authored one (`E-BAD-ENUM`, `E-ATTR-TYPE`). A def that reads state does not fold,
and using it there is `E-ATTR-DEF-DYNAMIC`. Branch instead, with a literal in each arm:

```lute
<match on="scene.affect.elena">
  <when is="5..">
    ::camera{zoom="1.35"}
  </when>
  <otherwise>
    ::camera{zoom="1.15"}
  </otherwise>
</match>
```

In content text, `{{@name}}` renders the def's value: the artifact carries the def body with the
placeholder, and the engine evaluates it like any guard. A def whose body cannot be inlined into one
expression (an expansion cycle, or a body that reads `$`) is `E-INTERP-DEF`. A def body gets the same
CEL profile check as any guard, so `%` or `size()` in one is `E-CEL-PROFILE` at the def's own key.

## Parameterized defs

A def may declare typed **`params`**, turning it into a parameterized macro invoked as
`@name(args)`. The arguments are bound to the params at expansion time:

```yaml
defs:
  atLeast: { type: bool, params: { n: number }, cel: "user.level >= n" }
  chose:   { type: bool, params: { q: choiceRef, opt: choiceId }, cel: "scene.choices[q] == opt" }
```

*(From [`docs/examples/showcase/schema/base.schema.yaml`](https://github.com/journeyWorker/lute/blob/main/docs/examples/showcase/schema/base.schema.yaml).)*

```lute
<match on="scene.affect.marina">
  <when test="@atLeast(3)">
    @fixer{mono}: A veteran's welcome.
  </when>
  <otherwise>
    @fixer{mono}: Early days yet.
  </otherwise>
</match>
```

*(Call site from [`docs/examples/showcase/episode01.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/showcase/episode01.lute); a minimal parameterized def is in [`docs/examples/param-def.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/param-def.lute).)*

An argument is spliced into the body as a unit. An atomic argument — a path, a number, a plain
string literal, or an already parenthesized group — goes in bare, so `@atLeast(3)` expands to
`(user.level >= 3)`. Anything else keeps parentheses, so `@atLeast(user.level + 1)` expands to
`(user.level >= (user.level + 1))` and the argument can never regroup with the body's operators.
(Before 0.24.0 every argument was parenthesized; the compiled expression is the same, only the IR's
`raw` text is shorter.)

### Defs in traces and beat tables

`lute trace` and `lute beats` print a def reference as you wrote it: a `<match on="@today">` header
reads `<match @today>`, an arm or choice guard `(@atLeast(3))`, the coverage summary names the
def, and a beat's `when` column reads `@runsAtLeast(2) && @firstDay`. Pass `--expand` to see the
inlined CEL instead. For a scene with the gated lines
`@narrator{when="@atLeast(3)"}: …` and `@narrator{when="@atLeast(user.level + 1)"}: …`:

```console
$ lute trace board.lute
trace: board.lute  (seeds: 0 paths, 0 facts; 0 selections)
  ## Board
  <match @atLeast(3)>   -> otherwise
  <match @atLeast(user.level + 1)>   -> otherwise
trace complete: 2 decisions; arms 1/2 (@atLeast(3) @13:1), arms 1/2 (@atLeast(user.level + 1) @14:1)
$ lute trace board.lute --expand
trace: board.lute  (seeds: 0 paths, 0 facts; 0 selections)
  ## Board
  <match (user.level >= 3)>   -> otherwise
  <match (user.level >= (user.level + 1))>   -> otherwise
trace complete: 2 decisions; arms 1/2 ((user.level >= 3) @13:1), arms 1/2 ((user.level >= (user.level + 1)) @14:1)
```

`--json` keeps the expansion in its `id`/`guard`/`label`/`when` fields and adds the authored text
beside it (`authoredGuard` and friends in a trace, `whenAuthored` in `lute beats`). See
[Tracing](/tooling/tracing/).

## Where defs live

A def whose CEL reads `run`/`user`/`app` state belongs in the shared schema document, so it can be
imported by every scene that needs it (and refined via [`extends:`](/language/components-and-extends/)).
A def that reads only `scene.*` can live inline in the scene. Duplicate def names across imports are
a static error — no silent shadowing.

## Defs in rule guards

A rule's scalar guard may use a def like any condition: `lit(lamp) :- cel("@firstDay")`, or with
arguments, `cel("@dayAtLeast(3)")`. The guard is expanded against the project's defs before it is
checked and evaluated, and the expanded body still has to pass the rule-guard firewall (scalar
state only, no fact query). A def that does not exist, a wrong argument count, or a `$` in a rule
guard is `E-RULE-GUARD-DEF`:

```lute expect="E-RULE-GUARD-DEF"
---
kind: scene
id: shed.lamp
title: The lamp
state:
  run.day: { type: number, default: 1 }
defs:
  firstDay: { type: bool, cel: "run.day == 1" }
entities:
  item: { members: [lamp, rope] }
relations:
  lit: { args: [item], derive: true }
rules:
  - "lit(lamp) :- cel(\"@firstDy\")"
---

## Lamp

@narrator{when="holds(lit(lamp))"}: The lamp is lit.
```

Before 0.24.0 such a rule passed `check` and then silently derived nothing, because the guard was
evaluated unexpanded. See [Facts & Datalog](/state/facts-and-datalog/) for rules and their guards.

## What the checker verifies

The static checker owns five `@ref` checks, each conservative (only provably-wrong cases flag):

- the name is declared in `defs` — else `E-UNDECLARED-REF`;
- the call arity matches the declared `params` — else `E-REF-ARITY`;
- each statically-resolvable argument matches its param's type — else `E-REF-ARG-TYPE`;
- a whole-slot `@ref` produces the position's required type — else `E-REF-TYPE`;
- every state path the def body reads is safe to read **where the def is used** — else
  `E-MAYBE-UNSET`, naming the path and the def.

The last check runs at every use: `{{@lastF}}`, a `when="@lastF > 30"`, a `::use` argument
`fathoms=@lastF`, a `::set` right-hand side, and a `<match on="@lastF">` subject all read what the
def's body reads. A def over a path with no default is therefore guarded where it is used, or
guards itself:

```lute expect="E-MAYBE-UNSET"
---
kind: scene
id: dock.recap
title: Last time
state:
  run.depth: { type: number, default: 0 }
defs:
  lastF: { type: number, cel: "prev.run.depth * 10" }
  deepLast: { type: bool, cel: "isSet(prev.run.depth) && prev.run.depth > 3" }
---

## Recap

@narrator{when="isSet(prev.run.depth)"}: Last run you reached {{@lastF}} feet.
@narrator{when="@deepLast"}: Deeper than most.
@narrator: That was {{@lastF}} feet.
```

The first two lines are clean: the line's guard proves `prev.run.depth`, and `@deepLast` carries its
own `isSet`. The third reads `prev.run.depth` through `@lastF` with nothing proving it set:

<!-- lute-diagnostics -->
```
recap.lute:16:21: error [E-MAYBE-UNSET] state path `prev.run.depth` may be read before it is set, read through `@lastF` (no default, no dominating `::set`, no guard) (dsl §9.4)
```

The `min`/`max`/`values` def fields from earlier drafts are removed: a def is fully described by its
`type`, optional `params`, and `cel`.
