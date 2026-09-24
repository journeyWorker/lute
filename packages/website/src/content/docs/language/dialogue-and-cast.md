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
@marina{code="0010" emotion="delighted" variant="1"}: Mr. Fixer! You came back!
@fixer{code="0010"}: I did.
```

*(From [`docs/examples/showcase/episode01.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/showcase/episode01.lute).)*

## Line attributes

Attributes in `{…}` are content metadata: `code` (a stable per-line id), `emotion`, `variant`,
`action`, `dialogMotion`, and `as` (a one-off speaker-label override). Their *domains* are project
vocabulary, not grammar — run `lute context <file>` to list the legal `emotion`/`variant` values
for your project. None is required; a missing `code` is back-filled deterministically at compile
time and can be persisted with `lute tag`.

`action=` is the one line attribute with a **stage** consequence, and it is a
small one: it sets the speaker's pose for that line and marks them dirty, so
the next plain line from the same speaker gets a `posReset` injected ahead of
it (`provenance.by: "auto-pose-reset"`). That is the whole of it — it is a
delivery detail, not staging. **A character's entrance and exit are `::auto`**,
and only `::auto` can end a presence (see
[Core directives](/language/directives/); `lute context` lists
`mayExitCharacter` among `auto`'s semantics for exactly this reason). Writing a
member of the `action` domain's `exits:` list on a content line is
`W-EXIT-INERT`: the pose is honoured, the character stays on stage, and the
artifact gets no `exit` record. Once a character has left — by a declared exit
or a `::bg` scene change — a line from them before an `::auto` shows them again
is `W-STAGE-ABSENT`, judged along every path through choices and `<match>` arms
(see [Stage state](/language/directives/#stage-state)).

```lute
@marina{as="???"}: ...who's there?
```

`as` overrides only the shown label for that one line. When absent, a line renders its speaker id
as the label, and the engine maps that id to a display name. A display-name capability — the
[character/cast proposal](https://github.com/journeyWorker/lute/blob/main/docs/proposals/character-cast/0.0.1.md),
with a speaker registry, costumes, and name-reveal — is a draft, and no such plugin ships yet. So
Lute does not validate speaker ids against a cast, and `as=` is the only way to set a label from
the source.

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
@marina{code="0010" emotion="delighted" variant="1"}: You came back! Warmth so far: {{run.affection}}.
```

*(From [`docs/examples/showcase/hub-demo.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/showcase/hub-demo.lute).)*

`{{userName}}` is the always-available reserved token. Any other interpolation must name a
**declared** state path; an interpolation is a *read* for definite-assignment analysis, so a
maybe-unset path interpolated without a guard is `E-MAYBE-UNSET`. The text after the second colon
is otherwise opaque to end of line — parentheses, `<`, `//`, and anything else are literal, never
parsed.
