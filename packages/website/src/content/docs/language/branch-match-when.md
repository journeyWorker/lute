---
title: Branch, match & when
description: The logic layer — player-driven <branch> menus and state-driven <match>/<when>/<otherwise> arms, with literal patterns, CEL guards, first-match-wins, and exhaustiveness.
---

Lute's logic layer is a small set of nesting blocks that select which content plays. Two kinds
branch the flow: **`<branch>`** takes *player* input, and **`<match>`** dispatches on *state* with
no input. Both reduce to finite command records at compile time — the language stays total.

## `<branch>` — player choice

A `<branch id>` presents a menu; each `<choice>` is one option. The selected choice id is recorded
into the reserved path `scene.choices.<branchId>`, which a later `<match>` can read.

```lute
<branch id="number">
  <choice id="blunt" label="Just ask, flatly">
    @fixer{code="0050"}: Bianca. Your number.
  </choice>
  <choice id="soft" label="Ask gently">
    @fixer{code="0052"}: Bianca — would you mind terribly if I had your number?
    ::set{scene.affect.bianca += 1}
  </choice>
</branch>
```

*(From [`docs/examples/bianca-s01ep02.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/bianca-s01ep02.lute).)*

A branch id must be unique within the episode, and a branch must contain at least one **unguarded**
choice so the menu is never provably empty (`E-BRANCH-ALL-GUARDED`). Choice mechanics — `when`
guards, the `into=` run-record sugar, and revisit `<hub>`s — are covered in
[Choices & hubs](/language/choices-and-hubs/).

## `<match>` — state dispatch

A `<match on="S">` evaluates the subject expression `S` and runs the first matching `<when>` arm,
falling through to `<otherwise>` if none match. No player input is involved — this is how a scene
reacts to a choice made earlier, a fact, or a plugin result.

```lute
<match on="scene.choices.number">
  <when test="@fond">
    @fixer{mono}: I asked nicely, which I am electing not to examine.
  </when>
  <when test="$ == 'blunt'">
    @fixer{mono}: Straight to the point.
  </when>
  <otherwise>
    @fixer{mono}: Whatever it was, it is done.
  </otherwise>
</match>
```

Arms are evaluated **top to bottom; first match wins.**

### `<when>` patterns and guards

A `<when>` arm matches on a literal pattern (`is`), a CEL guard (`test`), or both:

- **`is`** is a literal pattern: one literal, or a `|`-alternation of literals. Legal literals are
  enum member ids, `true`/`false`, decimal numbers, and the keyword `unset`. Matching is equality
  on the subject. `<when is="joyful|playful">`, `<when is="unset">`, `<when is="1 | 2 | 3">`.
- **`test`** is a CEL guard, with the `$` subject in scope (`$` is the value of `on`). `$` may only
  appear inside a `<match>`.
- **`is` + `test`** together means pattern AND guard.
- A `<when>` with neither is `E-WHEN-PATTERN`.

```lute
<match on="scene.mood">
  <when is="calm">          @fixer{mono}: Steady breathing. </when>
  <when is="tense">         @fixer{mono}: Shoulders drawn tight. </when>
  <when is="joyful|playful">@fixer{mono}: Light in the eyes. </when>
</match>
```

*(From [`docs/examples/showcase/when-is-demo.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/showcase/when-is-demo.lute).)*

### Exhaustiveness

A `<match>` must be exhaustive. Exhaustiveness is computed from the union of `is` literals (plus
any `unset` arm): for a **finite domain** — an enum, a bool, or a branch's choice ids — full `is`
coverage is exhaustive with **no `<otherwise>`** needed. The four-member enum above needs no
`<otherwise>`; a bool covered by `is="true"`/`is="false"` needs none either.

Otherwise, `<otherwise>` is **mandatory**: whenever the subject is maybe-unset (a `run.*`/`app.*`
path with no default — its `unset` case must be covered), the domain is not finite, or an arm uses
a `test` guard the checker cannot prove covers the domain. A `<match>` reading `app.rating` in a
release build is a hard content gate that must cover `teen` or carry `<otherwise>`.

## The `when=` content-line sugar

A single content line may carry a `when="G"` guard directly: the line is emitted only if `G` holds.

```lute
@sofia{when="run.metHelpfully"}: You helped me back then. I've been meaning to thank you.
```

*(From [`docs/examples/gated-line.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/gated-line.lute).)*

This is exact sugar for a one-arm match — `<match on="G"><when test="$">…</when><otherwise/></match>`
— and lowers to that record identically, leaving the line's `code`/`lineId`/`voiceKey` unchanged.
