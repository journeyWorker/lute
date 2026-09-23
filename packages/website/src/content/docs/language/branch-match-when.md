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
    @fixer{code="0050"}: Marina. Your number.
  </choice>
  <choice id="soft" label="Ask gently">
    @fixer{code="0052"}: Marina — would you mind terribly if I had your number?
    ::set{scene.affect.marina += 1}
  </choice>
</branch>
```

*(From [`docs/examples/marina-s01ep02.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/marina-s01ep02.lute).)*

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
  <when is="blunt">
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
  enum member ids, `true`/`false`, decimal numbers, inclusive numeric ranges (`2..`, `..0`,
  `1..3`), and the keyword `unset`. Matching is equality on the subject (range membership for a
  range). `<when is="joyful|playful">`, `<when is="unset">`, `<when is="1 | 2 | 3">`,
  `<when is="..0 | 10..">`.
- **`test`** is a CEL guard, with the `$` subject in scope (`$` is the value of `on`). `$` may only
  appear inside a `<match>`. Use it for conditions a pattern cannot say (`$ > 2`, `@fond`); a
  plain literal comparison such as `test="$ == 'gold'"` is `W-WHEN-TEST-LITERAL`, and `lute fix`
  rewrites it to `is="gold"`.
- **`is` + `test`** together means pattern AND guard.
- A `<when>` with neither is `E-WHEN-PATTERN`.

```lute
<match on="scene.mood">
  <when is="calm">
    @fixer{mono}: Steady breathing. Nothing to prove tonight.
  </when>
  <when is="tense">
    @fixer{mono}: Shoulders drawn tight — I should tread carefully.
  </when>
  <when is="joyful|playful">
    @fixer{mono}: Light in the eyes.
  </when>
</match>
```

*(From [`docs/examples/showcase/when-is-demo.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/showcase/when-is-demo.lute).)*

### Numeric ranges

On a `number` subject, `is` takes ranges. Both bounds are inclusive; either may be left open.
The descending-threshold cascade reads top to bottom, first match wins:

```lute
<match on="scene.affect.marina">
  <when is="3..">
    ::use{component="reaction" tier="fond"}
  </when>
  <when is="1..">
    ::use{component="reaction" tier="warm"}
  </when>
  <otherwise>
    ::use{component="reaction" tier="cold"}
  </otherwise>
</match>
```

*(From [`docs/examples/affinity-reaction.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/affinity-reaction.lute).)*

A malformed or empty range (`a..b`, `3..1`) is `E-WHEN-RANGE`; a range on a non-numeric subject is
`E-WHEN-LITERAL-DOMAIN`. Exclusive bounds have no pattern form — write `test="$ > 2"`.

### Exhaustiveness

A `<match>` must be exhaustive. Exhaustiveness is computed from the union of `is` literals (plus
any `unset` arm): for a **finite domain** — an enum, a bool, or a branch's choice ids — full `is`
coverage is exhaustive with **no `<otherwise>`** needed. The four-member enum above needs no
`<otherwise>`; a bool covered by `is="true"`/`is="false"` needs none either. A **`number`**
subject is exhaustive when its ranges and points cover the whole real line: `is="..0"` +
`is="0.."` is; `is="..0"` + `is="1.."` is not, because `0.5` falls between them — the error names
the gap.

Otherwise, `<otherwise>` is **mandatory**: whenever the subject is maybe-unset (a `run.*`/`app.*`
path with no default — its `unset` case must be covered), the domain is a string or otherwise
opaque, or an arm uses a `test` guard the checker cannot prove covers the domain. A `<match>`
reading `app.rating` in a release build is a hard content gate that must cover `teen` or carry
`<otherwise>`.

## The `when=` content-line sugar

A single content line may carry a `when="G"` guard directly: the line is emitted only if `G` holds.

```lute
@elena{when="run.metHelpfully"}: You helped me back then. I've been meaning to thank you.
```

*(From [`docs/examples/gated-line.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/gated-line.lute).)*

This is exact sugar for a one-arm match **wherever the guard is a legal `<match>` subject**, and lowers to that record identically, leaving the line's
`code`/`lineId`/`voiceKey` unchanged. Written out it is the explicit twin below — every tag takes a
line of its own, because there is no inline `<when>…</when>` form:

```lute
<match on="run.metHelpfully">
  <when test="$">
    @elena: You helped me back then. I've been meaning to thank you.
  </when>
  <otherwise>
  </otherwise>
</match>
```

*(That file keeps both forms, one shot each, so they stay visibly interchangeable.)*

A **relational** guard is the exception. `@elena{when="holds(awake(toma))"}: …` checks clean on a
content line; the same guard as a subject — `<match on="holds(awake(toma))">` — is
`E-MATCH-RELATION-SUBJECT`, *"relations are guard-only; a `<match on>` subject must stay
enum/bool/scalar so exhaustiveness stays decidable"* (§8). Where the guard queries facts, the line
form is the only form, and the two are interchangeable only for the scalar and enum subjects a
`<match>` can take — which is why the file above uses one.
