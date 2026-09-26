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
`<otherwise>`; a bool covered by `is="true"`/`is="false"` needs none either. When coverage falls
short, `E-NONEXHAUSTIVE` names the members left out (`` `tue`, `wed` are not covered ``). A **`number`**
subject is exhaustive when its ranges and points cover the whole real line: `is="..0"` +
`is="0.."` is; `is="..0"` + `is="1.."` is not, because `0.5` falls between them — the error names
the gap.

Otherwise, `<otherwise>` is **mandatory**: whenever the subject is maybe-unset (a `run.*`/`app.*`
path with no default — its `unset` case must be covered), the domain is a string or otherwise
opaque, or an arm uses a `test` guard the checker cannot prove covers the domain. A `<match>`
reading `app.rating` in a release build is a hard content gate that must cover `teen` or carry
`<otherwise>`.

### Matching on a def

A `<match on="@def">` (see [Definitions & params](/language/params/)) has a domain like any other
subject. A def whose body is one state path (`today: "run.wd"`) matches exactly like that path. Any
other def takes its declared or inferred type, so a def typed `{ enum: [early, late] }` ranges over
those members:

```lute check
---
kind: scene
id: town.square
title: The square
state:
  run.wd: { type: { enum: [mon, tue, wed] }, default: mon }
  run.day: { type: number, default: 1 }
defs:
  today: "run.wd"
  shift: { type: { enum: [early, late] }, cel: "run.day <= 3 ? 'early' : 'late'" }
---

## Square

<match on="@today">
  <when is="mon">
    @narrator: Market day.
  </when>
  <when is="tue|wed">
    @narrator: The square is quiet.
  </when>
</match>

<match on="@shift">
  <when is="early">
    @narrator: The stalls are still going up.
  </when>
  <when is="late">
    @narrator: The stalls are coming down.
  </when>
</match>
```

Both matches are exhaustive with no `<otherwise>`. Before 0.24.0 a def subject had no domain, so
both were `E-NONEXHAUSTIVE`. An arm outside the domain, such as a typo `is="lat"`, is
`E-WHEN-LITERAL-DOMAIN`. Since 0.26.0 so is a string a guard compares with an enum-typed subject,
such as `test="$ == 'lat'"` on a match over a path, with a did-you-mean.

### Arms narrow their subject

Reading a maybe-unset path — a `run.*`/`app.*` path with no default — is `E-MAYBE-UNSET` until
something proves it set. When the `<match>` subject is a plain state path, the arms supply that
proof for reads of the subject inside them:

- Inside `<when is="…">` whose alternatives name values and none of them is `unset`, the subject
  equals one of those values, so it is set.
- Once an arm takes every unset value — `is="unset"` with no `test` — no later arm and no
  `<otherwise>` can see the subject unset, so they read it as set.

```lute
<match on="run.rival">
  <when is="unset">
    @narrator: Nobody has taken your measure yet.
  </when>
  <when test="$ >= 3">
    @narrator: Your rival is {{run.rival}} bouts ahead.
  </when>
  <otherwise>
    @narrator: Your rival sits at {{run.rival}}.
  </otherwise>
</match>
```

Neither interpolation is `E-MAYBE-UNSET`: both arms come after the `unset` arm. The proof stays
inside the match and runs downward only. An `<otherwise>` (or a `test`-only arm) can still see the
subject unset when no arm above it takes every unset value — there is no `is="unset"` arm, or the
one there carries a `test` — so a read of the subject there still reports. A read after
`</match>` is as unproven as it was before the match.

## Guards narrow what they cover

A guard proves things about the state it has checked, and the checker uses that proof where the
guard holds.

**A beat's guard holds through its body.** A scene beat's `when:`, a bundle `<beat when>`, and an
entry's `when=` are true whenever their body runs. An `isSet(…)` in the guard proves the body's
reads of that path and its `<match>` subjects, so they are neither `E-MAYBE-UNSET` nor
`E-UNSET-UNCOVERED`. The guard also narrows the domain a `<match>` must cover: below, `died` is
ruled out, so two arms are exhaustive with no `<otherwise>`:

```lute check
---
kind: scene
id: dock.return
title: Back at the dock
on: dockReturn
when: "isSet(run.outcome) && run.outcome != 'died'"
state:
  run.outcome: { type: { enum: [surfaced, diving, died] } }
---

## Dock

@narrator: The run ended {{run.outcome}}.

<match on="run.outcome">
  <when is="surfaced">
    @narrator: You surfaced with air to spare.
  </when>
  <when is="diving">
    @narrator: They hauled you up mid-dive.
  </when>
</match>
```

An arm whose every value the guard excludes can never fire, and is `E-ARM-DEAD`. Adding
`<when is="died">` above reports:

<!-- lute-diagnostics -->
```
dock.lute:22:3: error [E-ARM-DEAD] arm can never fire: its pattern `died` is ruled out by the body's `when` guard `isSet(run.outcome) && run.outcome != 'died'`, which holds whenever this body runs (dsl 0.24.0)
```

An `<otherwise>` after the two arms is `W-OTHERWISE-DEAD` for the same reason; the warning names
"the domain left by the body's `when` guard". A body that writes the subject before the match
(`::set{run.outcome = "died"}`) keeps the whole domain, so there the match needs a `died` arm or an
`<otherwise>` again.

**A presence guard proves what its short-circuit protects.** Inside one expression, the part that
only runs once `isSet(p)` is true may read `p`:

```lute check
---
kind: scene
id: dock.board
title: The board
state:
  run.a: { type: number, default: 0 }
  run.o: { type: { enum: [won, lost] } }
---

## Board

@narrator{when="run.a == 1 || (isSet(run.o) && run.o == 'won')"}: A clean week.
@narrator{when="isSet(run.o) ? run.o == 'won' : false"}: You won.
@narrator{when="!isSet(run.o) || run.o == 'won'"}: Nothing lost yet.
```

All three are clean. The proof stays inside the expression: `isSet(run.o) || run.a == 1` does not
prove `run.o` for the line it guards, because the line also runs when only `run.a == 1` holds.

**`prev.run.*` is one snapshot.** A `newRun` copies the whole ended run at once, so once any
`prev.run.<p>` is known to be present, every `prev.run.<q>` whose `run.<q>` declares a `default` is
present too:

```lute check
---
kind: scene
id: dock.recap
title: Last time
state:
  run.depth: { type: number, default: 0 }
  run.gold: { type: number, default: 0 }
---

## Recap

@narrator{when="isSet(prev.run.depth)"}: Last run you reached {{prev.run.depth}} fathoms with {{prev.run.gold}} gold.
```

A `run.<q>` with no default may have been unset when the run ended, so its `prev.run.<q>` still
needs its own guard.

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

### Guarded writes

The same sugar covers a single write (dsl 0.24.0 §1).
`::set{run.aff += 1 when="run.warmed < run.day"}` writes only when its guard holds, and compiles to the same one-arm match a gated line
does, so one conditional write needs no `<match>` around it. The guard is checked like a line's
`when=` (a bool, no `$`, `E-ARM-DEAD` when provably false), and it is never a definite assignment:
a later read of a path with no default stays `E-MAYBE-UNSET`. See
[The clock](/language/clock/) for the once-a-day pattern it was made for.
