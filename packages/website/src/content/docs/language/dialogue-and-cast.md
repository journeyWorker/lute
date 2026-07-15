---
title: Dialogue & cast
description: Content lines — the @speaker syntax for dialogue, narration, and player voice — plus delivery flags, line attributes, interpolation, and the cast/display-name model.
---

Content is the spoken and narrated text of a scene. Every content line has the same shape:

```
@speaker{attributes}: the text they say
```

The **speaker** selects the line's kind:

- a **registered character** id → **dialogue**;
- the reserved **`narrator`** → **narration** (speakerless);
- the speaker whose id equals frontmatter `pov` → the reserved **player** (protagonist), which
  renders the runtime `{{userName}}` and carries no sprite.

There is no separate monologue or prose node — role is derived from the speaker plus its delivery
(below).

```lute
@narrator: Venny's again. The chain restaurant that has never offended anyone.
@bianca{code="0010" emotion="delighted" variant="1"}: Mr. Fixer! You came back!
@fixer{code="0010"}: I did.
```

*(From [`docs/examples/showcase/episode01.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/showcase/episode01.lute).)*

## Line attributes

Attributes in `{…}` are content metadata: `code` (a stable per-line id), `emotion`, `variant`,
`action`, `dialogMotion`, and `as` (a one-off speaker-label override). Their *domains* are project
vocabulary, not grammar — run `lute context <file>` to list the legal `emotion`/`variant` values
for your project. None is required; a missing `code` is back-filled deterministically at compile
time and can be persisted with `lute tag`.

```lute
@bianca{as="???"}: ...who's there?
```

`as` overrides only the shown label for that one line. When absent, the label is resolved by the
active display-name capability (the character/cast plugin), which adds registry validation,
costumes, and name-reveal. With no such capability active, a line still renders its speaker id as
the label.

## Delivery flags

A **delivery flag** is a bare word in the braces (no `=value`) that changes how a line is
delivered:

- **`{mono}`** — interior monologue / thought (not spoken aloud in-scene).
- **`{os}`** — off-screen: the speaker is heard but not currently staged or visible.
- **`{vo}`** — voiceover: narration-style delivery layered over the scene.

```lute
@fixer{mono}: An android, then. Which would, on reflection, explain the ramen.
```

The three are **mutually exclusive** — at most one per line (`E-DELIVERY-CONFLICT` on two) — and
none is allowed on `@narrator` (`E-DELIVERY-NARRATOR`). `{mono}` works for *any* character, not
just the player: a non-player `{mono}` line is that character's inner voice.

Roles derive from speaker + delivery: `narrator` → narration; any character with `{mono}` →
monologue; any character with `{vo}` → voiceover; any character otherwise → dialogue.

## Interpolation

Content `Text` (and a `<choice>` label) may embed **`{{…}}`** interpolations that read game state at
render time:

```lute
@narrator: Good to see you, {{userName}}.
@bianca{code="0010" emotion="delighted" variant="1"}: You came back! Warmth so far: {{run.affection}}.
```

*(From [`docs/examples/showcase/hub-demo.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/showcase/hub-demo.lute).)*

`{{userName}}` is the always-available reserved token. Any other interpolation must name a
**declared** state path; an interpolation is a *read* for definite-assignment analysis, so a
maybe-unset path interpolated without a guard is `E-MAYBE-UNSET`. The text after the second colon
is otherwise opaque to end of line — parentheses, `<`, `//`, and anything else are literal, never
parsed.
