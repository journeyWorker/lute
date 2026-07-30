---
title: CEL expressions
description: The restricted Lute-CEL profile — the closed environment behind every condition and assignment, where it appears, the fact-query surface it admits, and how it compiles to inline CEL strings.
---

Every condition and every `::set` right-hand side in Lute is [CEL](https://cel.dev) — terminating and side-effect-free — interpreted under the **Lute-CEL profile**, a maximally restricted subset. CEL is what keeps the language *total, not Turing-complete*: there is no host-language (JS, Lua, …) evaluation anywhere.

## Where CEL appears

CEL text sits in every guard and value slot:

- `<match on="S">` subject, `<when test="…">` guards, `<choice when="…">` and `when=` content-line gates
- `::set{path = celExpr}` right-hand sides
- quest `<quest start="…" fail="…">` and `<objective done="…" when="…">` predicates, `<on when="…">` handlers

```lute
<match on="scene.affect.sofia">
  <when test="$ >= 3">
    @sofia: …
  </when>
  <when test="@chose('couch', 'ignore')">
    @sofia: …
  </when>
  <otherwise>
    @sofia: …
  </otherwise>
</match>
```

Inside a `<match>`, the token `$` resolves to the subject expression `S` and MUST NOT appear elsewhere. A `@ref` / `@fn(args)` is a **compile-time macro** expanded to inline CEL before evaluation, with params bound from the call arguments — parenthesized and AST-safe.

## The closed environment

The Lute-CEL environment provides exactly: CEL operators, literals, list literals, the ternary `?:`, the `in` membership operator, the `has()` macro, and the single extension `isSet(<path>)` (true iff a state path is assigned). **No other functions** exist — a comprehension (`map`/`filter`/`exists`/`all`), `size`, or `matches` is a static error (`E-CEL-PROFILE`). State-path segments, `defs` names, and param names are CEL-facing identifiers and forbid `-` (`E-PATH-IDENT`).

When the relational layer is in play, conditions may also read the fact database through a bounded predicate surface: `holds(rel(args|_))` (valid-now membership, any slot may be `_`), `count(rel(args|_)) OP n` (distinct valid tuples), and `validAt(rel(args|_), T)` (historical form over base relations). `T` is a `narrativeTime` expression — in practice `quest.<id>.activatedAt`, the only one an author can name (see [Quests & scenes](/language/quests-and-scenes/)). Joins under aggregation are expressed as [derived relations](/state/facts-and-datalog/), never multi-relation `count`. A `<match on>` subject must stay a scalar/enum path; fact queries live in guards only.

## Compile target

`@ref` macros expand at **compile time**; the resulting inline CEL string is carried in the flat command-record artifact and evaluated at **runtime** by the engine. Everything desugars to flat records plus CEL strings — the compiler↔engine contract. The unset sentinel is CEL `null` (tested with `!isSet(path)` or a `<when is="unset">` arm), never the string `'unset'` — comparing to that string is `E-UNSET-LITERAL`.
