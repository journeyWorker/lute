---
title: Dialogue & cast
description: Content lines — the @speaker syntax for dialogue and narration — plus the declared cast that closes the set of speakers (with per-speaker presence and emotions), delivery flags, line attributes, interpolation and ordinals, and display names.
---

Content is the spoken and narrated text of a scene. Every content line has the same shape:

```
@speaker{attributes}: the text they say
```

The **speaker** selects the line's kind:

- the reserved **`narrator`** → **narration** (speakerless);
- any other speaker id → **dialogue**, carrying that speaker.

Frontmatter `pov` names the scene's point-of-view character for readers of the source. It does not
change how that speaker's lines compile: the protagonist's lines are ordinary dialogue, and the
artifact carries neither `pov` nor a player role. An engine that renders its protagonist
differently (no sprite, the player's chosen name as the label) keys that on the speaker id; in line
text, write `{{userName}}` for the player's name.

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
and only `::auto` can end one character's presence; `::clear` (0.24.0) ends everyone's (see
[Core directives](/language/directives/); `lute context` lists
`mayExitCharacter` among `auto`'s semantics for exactly this reason). Writing a
member of the `action` domain's `exits:` list on a content line is
`W-EXIT-INERT`: the pose is honoured, the character stays on stage, and the
artifact gets no `exit` record. Once a character has left — by a declared exit,
a `::bg` scene change, or a `::clear` — a line from them before an `::auto` shows them again
is `W-STAGE-ABSENT`, judged along every path through choices and `<match>` arms
(see [Stage state](/language/directives/#stage-state)).

```lute
@marina{as="???"}: ...who's there?
```

`as` overrides only the shown label for that one line. When absent, a line renders its speaker id
as the label, and the engine maps that id to a display name: the `name` the project's
[cast](#the-cast) gives it. `as=` is the only way to set a label from the source. A richer
display-name capability — the
[character/cast proposal](https://github.com/journeyWorker/lute/blob/main/docs/proposals/character-cast/0.0.1.md),
with costumes and name-reveal — is a draft, and no such plugin ships yet.

## The cast

A project can declare its **cast** (dsl 0.23.0): the speaker ids its lines may use, each with an
optional display name. A schema document declares it under `cast:`, and the documents that import
the schema are checked against it:

```yaml
# cast.schema.yaml
cast:
  mira: { name: Mira }
  oskar: { name: "Oskar Lind" }
  vesna: {}
```

A plugin can ship one as well, with a `cast` export of `cast/*.yaml` files in the same shape (see
[Manifests](/plugins/manifests/#cast)), so an engine pack declares the characters its sprites and
voices exist for.

Once a cast is declared, every speaker must be in it. Scene lines, quest bodies, lore entries, and
[bundle beats](/language/beats/#beat-bundles) are all checked. A speaker outside the cast is
`E-CAST-UNKNOWN`, with a did-you-mean for a near miss such as `@oskr`. `narrator` is always a
speaker. The `pov` speaker is not special: a scene whose `pov` is `fixer` still needs `fixer` in the
cast.

<!-- lute-diagnostics -->
```
./scenes/arrival.lute:12:2: error [E-CAST-UNKNOWN] speaker `fixer` is not in the declared cast (dsl 0.23.0 §7)
```

Without a declared cast, speakers are checked for shape only, as before 0.23.0, so a project can
write dialogue before it settles its characters. The cast belongs to the project, not to one
scene: a `cast:` key in a scene's frontmatter is `E-META-UNKNOWN-KEY`.

`lute context <file>` lists the cast with its display names (pass `--project <dir>` to include a
plugin's cast), and the language server offers the cast when it completes a speaker after `@`.

### Display names shared by two speakers

Two speakers shown under the same name look like one person in the dialogue box. When several
authors add characters to one project, that happens by accident: two areas each write a
`Hiker Gus`. `check-project` reports it as the advisory **`W-DISPLAY-NAME-DUP`** (dsl 0.26.0
§2.8), and `lute lint` reports it too. It compares two cast entries with exactly the same `name:`,
a cast name equal to a component's `::use{… name="…"}` display string, and two such strings for
different speakers (`who=`):

<!-- lute-diagnostics -->
```
./scenes/a.lute:11:1: warning [W-DISPLAY-NAME-DUP] display name `Hiker Gus` is shown for 2 different speakers: `gus` (./scenes/a.lute:10), `gus2` (./scenes/a.lute:11) — the dialogue box cannot tell them apart; rename one (dsl 0.26.0 §2.8)
```

Some names are shared on purpose: a role several speakers play, such as a villain team's rank and
file. Mark such an entry **`sharedName: true`**, and it is not counted:

```yaml
cast:
  gus:    { name: Hiker Gus }
  gus2:   { name: Hiker Gus Jr. }
  grunt1: { name: Eclipse Grunt, sharedName: true }
  grunt2: { name: Eclipse Grunt, sharedName: true }
```

A plugin's `cast/*.yaml` entry takes the same key. A merge gate that runs
`check-project --deny-warnings` passes again once the intended role names are marked, and
`lute lint --deny W-DISPLAY-NAME-DUP` promotes the code on its own. See
[Multi-author projects](/guides/multi-author/).

### Staging is checked against the cast too

With a cast declared, the character a staging directive names must be in it, like a speaker (dsl 0.24.0 §4). `::auto{character}` and `::camera{focus}` outside the cast are `E-CAST-UNKNOWN`, with the same did-you-mean. Timeline clips and the bodies of `<match>` arms and choices are checked as well. Before 0.24.0, `::auto{character="marra"}` passed while `@marra:` did not:

<!-- lute-diagnostics unverified="composed in crates/lute-check/src/cast.rs from a runtime-built directive prefix plus a pushed did-you-mean, so no single format! literal pins it; copied verbatim from check-project output" -->
```
./scenes/ridge.lute:11:19: error [E-CAST-UNKNOWN] `::auto{character}` `corvn` is not in the declared cast (dsl 0.23.0 §7) — did you mean `corvin`?
./scenes/ridge.lute:12:17: error [E-CAST-UNKNOWN] `::camera{focus}` `isold` is not in the declared cast (dsl 0.23.0 §7) — did you mean `isolde`?
```

### Per-speaker emotions

A cast entry may list the emotions a character shows, `emotions: [calm, fierce]` (dsl 0.24.0 §4). Each member must also be a member of the project's `emotion` enum. On a line by that speaker, or on a directive whose literal `character=` names them, an `emotion=` outside the list is `E-BAD-ENUM`, even when the `emotion` enum has the value:

<!-- lute-diagnostics -->
```
./scenes/ridge.lute:14:18: error [E-BAD-ENUM] `sad` is not one of `isolde`'s emotions (expected one of: calm, fierce) — the cast declares `emotions:` for `isolde` (dsl 0.24.0 §4)
```

A value the `emotion` enum itself rejects keeps its one existing error. A speaker without `emotions:` may use any member of the enum.

### Presence: `present:`

In a party game, a companion is not always there to speak. A cast entry may say when they are, as a condition, `present: "<condition>"` (dsl 0.24.0 §4). The condition is ordinary state or facts, so party membership needs no new mechanism:

```yaml
# world.schema.yaml
entities:
  person: { members: [isolde, corvin] }
relations:
  inParty: { args: [person], tier: run }
cast:
  isolde: { name: Isolde, present: "holds(inParty(isolde))", emotions: [calm, fierce] }
  corvin: { name: "Corvin Hale" }
```

A plugin's `cast/*.yaml` entries take the same two keys (see [Manifests](/plugins/manifests/#cast)).

The checker then asks, for every line by `isolde`, whether the guards around the line **imply** her `present:` condition. When they do not, the line is **`W-CAST-ABSENT`**:

```lute
@corvin: Where is Isolde?
@isolde: Right here.
@isolde{when="holds(inParty(isolde))"}: Right behind you.
<branch id="ask">
  <choice id="call" label="Call her over" when="holds(inParty(isolde))">
    @isolde{emotion="calm"}: I came.
    ::retract{ inParty(isolde) }
    @isolde: And now I'm leaving.
  </choice>
  <choice id="wait" label="Wait">
    @corvin: We wait, then.
  </choice>
</branch>
::assert{ inParty(isolde) }
@isolde: Back again.
```

`@isolde: Right here.` has no guard, so it warns. The next line and `I came.` are guarded by the condition itself. `And now I'm leaving.` warns again, because the `::retract` between the choice guard and the line falsifies what the guard said. The message names the condition and the guard that would satisfy it:

<!-- lute-diagnostics -->
```
./scenes/camp.lute:12:2: warning [W-CAST-ABSENT] `isolde` may not be here: the cast declares `present: "holds(inParty(isolde))"` for `isolde`, and the guards around this line do not imply it (dsl 0.24.0 §4). Guard the line — `@isolde{when="holds(inParty(isolde))"}` — or move it under a guard that implies it
```

The guards that count are the line's own `when=`, every enclosing `<choice when>` (in a branch or a hub), `<match>` arms (the arm's `is=` or `test`, and the fact that no earlier arm matched), `<on when>`, `<objective done>`, and the scene beat's `when:`, an entry's `when=` or a bundle beat's `when=`. `@def`s in them are expanded. Some guards are implied by where the line sits:

- a lore entry's body assumes its own `entry.<id>.read` / `everRead`;
- a quest `<on>` handler assumes the quest's state at that event, and the conjuncts of its `start` that stay true once true (`entry.<id>.everRead`, `visited(…)`);
- under `check-project`, a beat assumes that every always-eligible `once` beat ranked above it for the same occasion has already been spent.

A `holds(A)` guard over a derived relation also implies the bodies of A's rules: with `inParty(P) :- recruited(P), not departed(P)`, a line guarded by `holds(inParty(isolde))` satisfies `present: "!holds(departed(isolde))"`. A relation whose rules are ground once bound, such as a `cel()`-only schedule, reads as the disjunction of those rule bodies.

A write cancels a guard only when it can falsify it on that path. An `::assert` or `::retract` voids a guard atom of the same relation with unifiable arguments, or one derived through a rule that has the written relation as a premise, and only when it moves the atom the wrong way. Asserting `inParty(corvin)` keeps a `holds(inParty(isolde))` guard, and so does asserting a positive premise of it. A `::set` voids a guard that reads the path, including through such a rule. Each `&&` conjunct of a guard is judged on its own, and a write in one `<choice>` or `<match>` arm does not reach its siblings.

A `{vo}` line is exempt, since a voiceover is not the speaker being there: they may speak from outside the scene's time, as a memory or a narration over it. An `{os}` line is still checked (dsl 0.25.0 D-E): `{os}` means the speaker is *in the scene*, heard but out of frame, so they must be present. There is no flag yet for a remote speaker, such as a voice over the radio or the phone.

Facts are the one place where `lute check` and `check-project` differ. `check-project` also counts the facts that hold on every route to the line: asserted earlier on every path, or seeded and never retracted. So the last line above, `@isolde: Back again.` after `::assert{ inParty(isolde) }`, is clean there. A single-file `lute check` cannot see those facts, so it warns on that line too and adds a note saying that `check-project` does see them. Like any warning, `--deny W-CAST-ABSENT` makes it an error.

Presence often has a second half that only the engine writes, such as "and she has not fallen":
`present: "holds(inParty(isolde)) && !holds(fell(isolde))"`, where `fell` is a `reserved: true`
relation. The checker does not know when the engine writes `fell`, so without help only a guard at
each line or beat satisfies that half, and every line by `isolde` warns. A cast entry that adds
**`assume: true`** (dsl 0.24.0 §4) reads every negated `holds` of a `reserved:` relation in
`present:` as true, whether it is negated directly or through a rule such as
`inParty(P) :- recruited(P), not fell(P)`. With it, the lines above warn exactly as they did with
the plain `holds(inParty(isolde))` condition. The price is that a line spoken after the engine event
needs its own guard, since the checker no longer asks for one, unless the relation names the
events it changes on ([`changedOn`](#presence-after-engine-events-changedon), dsl 0.25.0):

```yaml
cast:
  isolde: { name: Isolde, present: "holds(inParty(isolde)) && !holds(fell(isolde))", assume: true }
```

`present:`, `emotions:` and `assume:` are checker inputs. None of them reaches the compiled artifact.

### Presence after engine events: `changedOn`

`assume: true` is right until the engine event that the reserved relation stands for: after a battle, `fell(isolde)` may well hold, and a line there is exactly where the check matters. The reserved relation can say on which occasions the engine changes it (dsl 0.25.0 §6):

```yaml
entities:
  companion: { members: [isolde] }
relations:
  inParty: { args: [companion], tier: run }
  fell:    { args: [companion], reserved: true, changedOn: [battleEnd] }
cast:
  isolde: { name: Isolde, present: "holds(inParty(isolde)) && !holds(fell(isolde))", assume: true }
```

With `changedOn`, `assume: true` no longer reads `!holds(fell(…))` as true in a unit presented on one of those occasions: a scene, entry or bundle beat answering `on: battleEnd`, or a quest `<on event>` handler or `on=` objective body judged there. Under `check-project` the same holds for every **after-descendant** of such a unit in the [scenario graph](/connectivity/scene-graph/), over its `after:` / `after=` / `[start]` edges. Occasions have no static order of their own, so the graph is what says "after the battle". Take a camp scene that asserts `inParty(isolde)`, an aftermath scene on `battleEnd` with `after: 'visited("camp")'`, and a road scene with `after: 'visited("aftermath")'`:

```lute
@isolde{when="holds(inParty(isolde))"}: We should keep moving.
@isolde{os}: Wait for me!
@isolde{vo}: I remember that road.
```

On the road, the first two lines warn again, each with a note saying why `assume: true` did not cover them. The `{vo}` line stays exempt:

<!-- lute-diagnostics unverified="verbatim lute check-project output; the W-CAST-ABSENT message is composed from a base literal plus the dsl 0.25.0 §6 changedOn suffix appended in crates/lute-check/src/cast.rs, so no single format! literal matches" -->
```
./scenes/road.lute:11:2: warning [W-CAST-ABSENT] `isolde` may not be here: the cast declares `present: "holds(inParty(isolde)) && !holds(fell(isolde))"` for `isolde`, and the guards around this line do not imply it (dsl 0.24.0 §4). Guard the line — `@isolde{when="holds(inParty(isolde)) && !holds(fell(isolde))"}` — or move it under a guard that implies it; `assume: true` does not cover `fell`: this line follows an occasion its `changedOn:` names (dsl 0.25.0 §6)
```

Guard those lines with the whole condition, `@isolde{when="holds(inParty(isolde)) && !holds(fell(isolde))"}`, and the warnings go. Lines in the camp scene, which comes before the battle in the graph, stay covered by `assume: true`. Without `changedOn`, the 0.24 behaviour is unchanged: `assume: true` covers every line. `changedOn` on a relation that is not `reserved: true` is `E-RELATION-DECL`, and so is an occasion no plugin declares, with a did-you-mean (`` `changedOn: batleEnd` is not a declared occasion — did you mean `battleEnd`? ``). In a project whose plugins declare no occasions, the names are not checked.

### Leaving the stage: `::clear`

Presence is about who is *with the player*. Who is *on stage* is the stage state (see [Stage state](/language/directives/#stage-state)). An `::auto` brings a character on, and a declared exit or a `::bg` takes them off. Since 0.24.0 the leaf `::clear` takes everyone off at once, leaving the background and music in place (see [Core directives](/language/directives/#clear--emptying-the-stage)). A line from a cleared character before an `::auto` shows them again is `W-STAGE-ABSENT`, and the warning names the `::clear`.

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
just the protagonist: a `{mono}` line is that character's inner voice.

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

A path typed against an enum whose members carry display `labels:` renders the label, not the
member id: `Today is {{run.weekday}}.` reads `Today is Sunday.` (see
[Paths typed by a named enum](/state/state-model/#paths-typed-by-a-named-enum)).

### Ordinals: `{{x:ordinal}}`

A number can be rendered as an English ordinal with a **format hint** after a colon (dsl 0.24.0
§4):

```lute check
---
kind: scene
id: tower.gate
state:
  user.climbs: { type: number, default: 1 }
---

## The Gate

@narrator: Welcome back, {{userName}}. This is your {{user.climbs:ordinal}} climb.
```

`lute run`, `lute play`, `lute trace` and `lute test` render `1st` `2nd` `3rd` `4th` … `11th`
`12th` `13th` … `21st` `22nd` … `101st` `111th`. A value that has no ordinal, a fraction or a
negative number, renders unchanged (`2.5`, `-3`). In the artifact the line's placeholder carries
`"format": "ordinal"`, and the engine does the rendering. A placeholder without a hint has no
`format` key, so artifacts that use none are unchanged.

### Ordinal words: `{{x:ordinalWord}}`

`:ordinalWord` (dsl 0.25.0 §8) renders the ordinal as a word, for text that reads better spelled out:

```lute check
---
kind: scene
id: road.camp
state:
  run.day: { type: number, default: 1 }
---

## Camp

@narrator: The {{run.day:ordinalWord}} night on the road.
```

The IR placeholder carries `"format": "ordinalWord"`, and the engine localizes the word. `lute run`, `lute play`, `lute trace` and `lute test` render the English words `first` `second` … `twentieth` for 1–20, and fall back to the `:ordinal` digits outside that range (`0th`, `21st`). The checker admits `:ordinalWord` wherever it admits `:ordinal`.

`ordinal` and `ordinalWord` are the only hints: `{{run.floor:roman}}` is `E-CEL-PROFILE`, and so is a misspelt `{{run.day:ordinalword}}`. Either hint needs a number, so on a string, enum or bool path or def, or on `{{userName}}`, it is `E-REF-TYPE`.
