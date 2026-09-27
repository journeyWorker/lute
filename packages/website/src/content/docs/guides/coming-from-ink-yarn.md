---
title: Coming from Ink or Yarn
description: "A map from Ink and Yarn Spinner to Lute: knots and nodes, diverts and jumps, choices and loops, variables, conditional text, sequences, endings, comments and tags, and what the checker says when Ink or Yarn syntax slips into a .lute file."
---

Ink and Yarn Spinner are flows: text runs down the page, and a divert or a jump sends it anywhere,
back up included. Lute splits that job in two:

- **Inside a scene** the text runs forward only. Menus are `<branch>` (asked once) and `<hub>`
  (asked again until the player leaves), and state picks between lines with `<match>` and `when=`.
- **Between scenes** nothing diverts. The engine raises an **occasion** ("the next chapter", "the
  player talks to Tilly"), and every scene says which occasion it answers. Order comes from
  `after:`, `priority:` and the manifest's `chapters:`. [Connect scenes into a
  story](/getting-started/connect-scenes/) walks through it.

Because no path can jump backward or loop on its own, the checker can follow every path through
a scene, which is what lets it prove that none of them dead-ends.

## The map

| Ink | Yarn Spinner | Lute |
|---|---|---|
| `=== knot ===` | a node (`title:` … `===`) | a scene: its own `.lute` file, `kind: scene` and `id:` in the frontmatter |
| `= stitch` | — | a `## heading` inside the scene |
| `-> knot`, to the next part of the story | `<<jump Node>>` | another scene that answers an occasion (`on:`), ordered by `after:` or `chapters:` |
| `-> knot`, back to a menu the player returns to | `<<jump>>` back to a hub node | a `<hub>`: after each choice it asks again, until an `exit` choice |
| `-> label`, to a labelled gather further down | — | `::next{to="label"}` to a `::mark{id="label"}` further down, forward only |
| `* [choice]`, once only | `-> option <<once>>` | a hub `<choice … once>`; a `<branch>` asks only once anyway |
| `+ [choice]`, sticky | `-> option` | a plain `<choice>` in a `<hub>` |
| `- gather` | the lines after the options | the lines after `</branch>`, which run whichever choice was taken |
| `VAR oil = 1` | `<<declare $oil = 1>>` | `run.oil: { type: number, default: 1 }` under `state:`, in a schema or the frontmatter |
| `CONST MAX = 5` | — | a def under `defs:`, read as `@MAX` |
| `LIST mood = calm, stormy` | — | a path typed by an enum: `run.mood: { type: { enum: [calm, stormy] }, default: calm }` |
| `~ oil = oil + 2` | `<<set $oil to $oil + 2>>` | `::set{run.oil += 2}` |
| `{oil}` in text | `{$oil}` in text | `{{run.oil}}` |
| `{oil > 0: text}` | `<<if $oil > 0>>` | a guarded line: `@narrator{when="run.oil > 0"}: text` |
| `{x: a \| b}` | `<<if>> … <<else>>` | two guarded lines, or a `<match>` with an `<otherwise>` arm |
| `{a \| b \| c}`, `{&a \| b}`, `{!a \| b}` | `<<once>> … <<else>>` | a `scene.*` counter and a `<match>` on it ([below](#text-that-changes)) |
| `{~a \| b}` | line groups (`=>`), `dice()`, `random()` | no random choice in Lute ([below](#text-that-changes)) |
| visit count `{knot}`, `{knot > 2}` | `visited_count("Node")` | a number path you `::set{… += 1}`; `visited('<id>')` is only true or false |
| `-> DONE` | `<<stop>>` | `::end`, or simply the end of the scene |
| `-> END` | the end of the story | a schema `terminal:` condition that a `::set` makes true ([below](#ending-the-story)) |
| a tunnel, `-> knot ->` | — | a component: `::use{component="…"}` |
| `INCLUDE` | several `.yarn` files | a project: every `.lute` file under `lute.project.yaml`, shared state in schemas imported with `uses:` |
| — | node group (`when:` headers) | several scenes answering one occasion, each with its own `when:`; the eligible one with the highest `priority:` plays ([Beats](/language/beats/)) |
| — | `when: once`, `when: always` | the `once:` key: `once: run` (the default), `once: user`, `once: false` |
| `# tag` | `#tag` | no free-form line tags ([below](#comments-and-tags)) |
| — | `#line:…`, a localisation line id | a content line's `code=`: `@narrator{code="0010"}: …`, which `lute tag` fills in; the compiled `lineId` is built from it ([Dialogue & cast](/language/dialogue-and-cast/)) |

## A knot, ported

The lamp room from an Ink story: a menu the player keeps coming back to, a text that changes each
time the ledger is read, a once-only choice, and an inline condition.

```
VAR oil = 1

=== lamp_room ===
{lamp_room == 1: The lamp room smells of cold brass.|The lamp room again. The dark is closer.}
+ [Read the ledger] -> ledger
* [Go down to the stores] -> stores
+ [Wait for dark] -> dusk

=== ledger ===
{ledger:
- 1: The last entry is three weeks old. "Oil low. Ship due."
- 2: You read it again. The handwriting shakes toward the end.
- else: The words have stopped changing.
}
-> lamp_room

=== stores ===
You find two more cans of oil.
~ oil = oil + 2
-> lamp_room

=== dusk ===
{oil >= 3: The lamp catches.|The wick sputters.}
-> END
```

In Lute the four knots are one scene. Save it as `lamp.lute`:

```lute check
---
kind: scene
id: lamp
title: The Lamp Room
state:
  run.oil:           { type: number, default: 1 }
  scene.ledgerReads: { type: number, default: 0 }
---

## The Lamp Room

@narrator: The lamp room smells of cold brass.
<hub id="lamp">
  <return>
    @narrator: The lamp room again. The dark is closer.
  </return>
  <choice id="ledger" label="Read the ledger">
    ::set{scene.ledgerReads += 1}
    <match on="scene.ledgerReads">
      <when is="1">
        @narrator: The last entry is three weeks old. "Oil low. Ship due."
      </when>
      <when is="2">
        @narrator: You read it again. The handwriting shakes toward the end.
      </when>
      <otherwise>
        @narrator: The words have stopped changing.
      </otherwise>
    </match>
  </choice>
  <choice id="stores" label="Go down to the stores" once>
    @narrator: You find two more cans of oil.
    ::set{run.oil += 2}
  </choice>
  <choice id="dusk" label="Wait for dark" exit>
    @narrator{when="run.oil >= 3"}: The lamp catches.
    @narrator{when="run.oil < 3"}: The wick sputters.
  </choice>
</hub>
```

- **The knot the player returns to is the `<hub>`.** Each sub-knot is one of its choices, and the
  `-> lamp_room` at the end of each one is implied: after a choice's lines run, the hub asks
  again. Only the `exit` choice leaves.
- **`+` is a plain choice, `*` is `once`.** A `once` choice drops out of the menu after it is taken.
- **The first-visit text** is the line before `<hub>`, which plays once. The text for every return
  is the `<return>` block: it runs after each choice except an `exit` one, before the menu comes
  back. It also runs after the last `once` choice empties the menu, just before the hub closes.
- **The visit count** `{ledger: - 1 … - 2 … - else …}` is a number the choice counts up itself,
  and a `<match>` on it. The hub does record each pick, as `scene.visited.lamp.ledger`, but that is
  only true or false, and it is already true inside the choice's own lines: the pick is recorded
  before its lines run.
- **The inline condition** `{oil >= 3: …|…}` is two lines, each with its own `when=`.
- `run.oil` is declared in this scene so the file checks on its own. In a project it belongs in a
  schema every scene imports, since other scenes read it too.

`lute trace` plays one path through the scene. `--choose` lists the hub's picks in order:

```
$ lute trace lamp.lute --choose lamp=ledger,ledger,stores,ledger,dusk
trace: lamp.lute  (seeds: 0 paths, 0 facts; 5 selections)
  ## The Lamp Room
    @narrator  The lamp room smells of cold brass.
  <hub lamp>   eligible: ledger, stores, dusk   -> ledger
    ::set  scene.ledgerReads = 1
  <match scene.ledgerReads>   -> arm 1 (is="1")
    @narrator  The last entry is three weeks old. "Oil low. Ship due."
    -- return (hub lamp) --
    @narrator  The lamp room again. The dark is closer.
  <hub lamp>   eligible: ledger, stores, dusk   -> ledger
    ::set  scene.ledgerReads = 2
  <match scene.ledgerReads>   -> arm 2 (is="2")
    @narrator  You read it again. The handwriting shakes toward the end.
    -- return (hub lamp) --
    @narrator  The lamp room again. The dark is closer.
  <hub lamp>   eligible: ledger, stores, dusk   -> stores
    @narrator  You find two more cans of oil.
    ::set  run.oil = 3
    -- return (hub lamp) --
    @narrator  The lamp room again. The dark is closer.
  <hub lamp>   eligible: ledger, dusk   -> ledger
    ::set  scene.ledgerReads = 3
  <match scene.ledgerReads>   -> otherwise
    @narrator  The words have stopped changing.
    -- return (hub lamp) --
    @narrator  The lamp room again. The dark is closer.
  <hub lamp>   eligible: ledger, dusk   -> dusk
  guard `run.oil >= 3`: taken
    @narrator  The lamp catches.
  guard `run.oil < 3`: skipped
trace complete: 10 decisions; choices 3/3 (lamp), arms 3/3 (scene.ledgerReads @19:5), guard `run.oil >= 3` @36:5: taken, guard `run.oil < 3` @37:5: skipped
```

`stores` leaves the menu after one pick. [Choices & hubs](/language/choices-and-hubs/) has the
rest of the hub rules, including the one the checker enforces on every hub: it must be able to
end (an unguarded `exit` choice, or only `once` choices).

## Text that changes

Lute has no inline alternatives. Ink's `{a|b|c}` and Yarn's `<<once>> … <<else>>` become whole
lines chosen by a `<match>`, and the number the `<match>` reads is one you keep yourself.

- **Sequence** (`{a|b|c}`, or `{stopping: …}`): count up and match `1`, `2`, and `<otherwise>` for
  the last one, as the ledger does above.
- **Once only** (`{!a|b}`): the same, with an empty `<otherwise>`.
- **Cycle** (`{&a|b|c}`): match the count modulo the number of lines, through a def:

```lute check
---
kind: scene
id: harbor
title: The Harbor
state:
  scene.looks: { type: number, default: 0 }
defs:
  weather: { type: number, cel: "scene.looks % 3" }
---

## The Harbor

<hub id="harbor">
  <choice id="look" label="Look at the sea">
    ::set{scene.looks += 1}
    <match on="@weather">
      <when is="1">
        @narrator: Fog on the water.
      </when>
      <when is="2">
        @narrator: Mist over the rocks.
      </when>
      <otherwise>
        @narrator: Rain, sideways.
      </otherwise>
    </match>
  </choice>
  <choice id="leave" label="Walk inland" exit>
    @narrator: You turn your back on the sea.
  </choice>
</hub>
```

- **Shuffle** (`{~a|b}`, Yarn's `dice()`): Lute makes no random choice, so that a trace or a play
  of a path always prints the same thing. Let the engine roll: declare a path the engine owns
  (`owner: engine` in the schema), and `<match>` on it.

A `scene.*` count starts over each time the scene is presented. Count in `run.*` or `user.*`
instead to keep it for the run or for good.

## Moving between scenes

A divert to a later knot becomes a second scene, and the engine raises the occasion that
scene answers. The manifest's `chapters:` puts a list of scenes in order: each raise of the
occasion presents the next one. Ink's `{cond: -> a | -> b}` is two scenes on the same occasion,
each with a `when:`; when the two conditions cannot both hold, the checker accepts them at the
same priority. [Connect scenes into a story](/getting-started/connect-scenes/) builds exactly this.

Inside a scene, `::next{to="…"}` jumps forward to a `::mark{id="…"}` (or to a line's `id=`).
It never jumps backward: a menu the player comes back to is a `<hub>`, and a scene that plays
again answers its occasion again. A heading is not a jump target, and neither is a choice id
(Ink's labelled choice `* (inside)`): a choice id names the pick, not a place in the text.

`visited('<scene id>')` asks whether a scene has ever been presented. It is true or false, not
a count, and it covers the whole save: it stays true after a new run starts.

## Ending the story

Ink has two endings and so does Lute.

- **`-> DONE`** ends this bit of flow. In Lute that is `::end`, or the end of the scene: the
  presentation ends, and the next occasion can present another scene.
- **`-> END`** ends the story. In Lute that is a condition in a schema, `terminal:`. Once it
  holds, the engine raises no more occasions until a new run starts (except an occasion declared
  `outsideRun: true`, such as a title screen). A scene makes it hold with an ordinary `::set`.

```yaml
# world.schema.yaml
state:
  run.fate: { type: { enum: [open, drowned] }, default: open }
terminal: "run.fate == 'drowned'"
```

```lute
<choice id="leap" label="Climb the rail toward the light" once>
  @narrator: The rail is wet. The light is very far away.
  ::set{run.fate = "drowned"}
  ::end
</choice>
```

`lute play` shows both: `::end` closes the scene, and the game is over from then on.

```
@narrator: The rail is wet. The light is very far away.
  set run.fate = "drowned"
::end        (this presentation ends)
  note: the game is over — `terminal: run.fate == 'drowned'` holds, so the engine raises no occasion from here (`occasion:` / `advance:` steps are refused; `newRun: true` starts a new run)
── end: terminal — `terminal: run.fate == 'drowned'` holds ──────────────
```

A play script checks it with `expect: { end: terminal }` ([Playing a story](/tooling/play/)).

## Comments and tags

A `//` comment stands on a line of its own (or after a directive), and `/* … */` spans lines.
After the text of a line, `//` and `#` are part of the text: the player sees them, and the checker
warns (`W-TEXT-COMMENT-LIKE`).

Lute has no free-form tags. What an Ink or Yarn tag tells the engine has a checked place instead:

- how a line is delivered: line attributes, `@keeper{emotion="tired"}: …`
  ([Dialogue & cast](/language/dialogue-and-cast/));
- sound, music, camera: a directive on its own line, `::sfx{…}`, `::music{…}`
  ([Core directives](/language/directives/));
- a document's own tags (Yarn's `tags:` header): `extra: { tags: [...] }` in the frontmatter.

## What the checker says

Ink or Yarn syntax in a `.lute` file is an error or a warning that names the Lute form. A file
of Ink lines:

<!-- lute-diagnostics unverified="byte-exact lute check output; each E-UNCLASSIFIED hint is composed from per-shape parts, several of which contain a literal … that the matcher reads as an elision" -->
```
$ lute check ink.lute
ink.lute:10:1: error [E-UNCLASSIFIED] unrecognized line: `=== lamp_room ===` is an Ink knot; a Lute scene is its own `.lute` file (`kind: scene` and `id: lamp_room` in its frontmatter), and a section inside a scene is a `## lamp_room` heading
ink.lute:11:1: error [E-UNCLASSIFIED] unrecognized line: `= stitch` is an Ink stitch; a section inside a scene is a `## stitch` heading
ink.lute:12:1: error [E-UNCLASSIFIED] unrecognized line: `VAR` declares an Ink global; Lute declares state under `state:` in the frontmatter (`run.oil: { type: number, default: 1 }`) and writes it with `::set{…}`
ink.lute:13:1: error [E-UNCLASSIFIED] unrecognized line: `~ run.oil = run.oil + 2` is Ink logic; Lute writes state with `::set{run.oil = run.oil + 2}`, and the path is declared under `state:` in the frontmatter
ink.lute:14:1: error [E-UNCLASSIFIED] unrecognized line: a content line needs a speaker: narration is `@narrator: …`, dialogue `@<speaker>: …`
ink.lute:15:1: error [E-UNCLASSIFIED] unrecognized line: `* [Read the ledger]` is an Ink choice; Lute choices are `<choice id="…" label="…">` blocks inside a `<branch>` (asked once) or a `<hub>` (asked again until an `exit` choice); Ink's once-only `*` in a loop is a `<hub>` choice with the `once` flag
ink.lute:16:1: error [E-UNCLASSIFIED] unrecognized line: `+ [Wait for dark]` is an Ink choice; Lute choices are `<choice id="…" label="…">` blocks inside a `<branch>` (asked once) or a `<hub>` (asked again until an `exit` choice); Ink's sticky `+` is a plain `<hub>` choice
ink.lute:17:1: error [E-UNCLASSIFIED] unrecognized line: `- gather` is an Ink gather; Lute has no gathers: after a `<branch>` or `<hub>` closes, the lines below it run whichever choice was taken, so write the gathered text there as an ordinary line (`@narrator: …`)
ink.lute:18:1: error [E-UNCLASSIFIED] unrecognized line: `-> ledger` is an Ink divert; Lute has no diverts: `::next{to="ledger"}` jumps forward to a `::mark{id="ledger"}` later in this document, a `<hub>` repeats its choices until an `exit` choice, and another scene is reached through the occasion it answers (`on:` in its frontmatter)
ink.lute:19:1: error [E-UNCLASSIFIED] unrecognized line: `-> END` is an Ink divert; it ends the whole story, which in Lute is the schema's `terminal:` condition: a scene makes it hold with an ordinary `::set{…}` (`::end` is Ink's `-> DONE`: it ends only this scene)
failed: ink.lute (10 error(s), 0 warning(s))
```

The same for Yarn:

<!-- lute-diagnostics unverified="byte-exact lute check output; each E-UNCLASSIFIED hint is composed from per-shape parts, several of which contain a literal … that the matcher reads as an elision" -->
```
$ lute check yarn.lute
yarn.lute:10:1: error [E-UNCLASSIFIED] unrecognized line: `<<declare $oil = 1>>` is a Yarn declaration; Lute declares state under `state:` in the frontmatter (`run.oil: { type: number, default: 1 }`) and writes it with `::set{…}`
yarn.lute:11:1: error [E-UNCLASSIFIED] unrecognized line: `<<set $oil to 3>>` is a Yarn command; Lute writes state with `::set{run.oil = 3}`, and the path is declared under `state:` in the frontmatter
yarn.lute:12:1: error [E-UNCLASSIFIED] unrecognized line: `<<if $oil > 2>>` is a Yarn conditional; Lute chooses between lines with `<match on="…">` and its `<when is="…">`/`<when test="…">` arms, or guards one line: `@narrator{when="run.oil > 2"}: …`
yarn.lute:13:1: error [E-UNCLASSIFIED] unrecognized line: `<<jump Lamp_Room>>` is a Yarn jump; Lute has no diverts: `::next{to="Lamp_Room"}` jumps forward to a `::mark{id="Lamp_Room"}` later in this document, a `<hub>` repeats its choices until an `exit` choice, and another scene is reached through the occasion it answers (`on:` in its frontmatter)
yarn.lute:14:1: error [E-UNCLASSIFIED] unrecognized line: `-> Read the ledger` is a Yarn option; Lute choices are `<choice id="…" label="…">` blocks inside a `<branch>` (asked once) or a `<hub>` (asked again until an `exit` choice)
failed: yarn.lute (5 error(s), 0 warning(s))
```

Text in another language's markup is not an error, because it could be meant literally, so the
checker warns. Single braces, a trailing comment or tag, and a bracketed choice label:

<!-- lute-diagnostics unverified="byte-exact lute check output; W-TEXT-SINGLE-BRACE prints a literal … (a shortened quote, Ink's {~…|…} notation) that the matcher reads as an elision" -->
```
$ lute check text.lute
text.lute:10:21: warning [W-TEXT-SINGLE-BRACE] `{run.oil}` is literal line text: single braces are not read, so the player sees them — interpolation is `{{run.oil}}`
text.lute:11:12: warning [W-TEXT-SINGLE-BRACE] `{run.oil > 0: A can of oil sits by the…}` is literal line text: single braces are not read, so the player sees them — Lute has no inline conditional text; guard the whole line instead (`@narrator{when="run.oil > 0"}: …`), or choose between lines with `<match>`
text.lute:12:12: warning [W-TEXT-SINGLE-BRACE] `{~Fog|Mist|Rain}` is literal line text: single braces are not read, so the player sees them — Lute has no inline alternatives (Ink's `{~…|…}` shuffle, `{&…|…}` cycle, `{!…|…}` once-only, `{…|…}` sequence); choose between whole lines with `<match>` or guarded lines (`when="…"`)
text.lute:13:17: warning [W-TEXT-SINGLE-BRACE] `{$oil}` is literal line text: single braces are not read, so the player sees them — Yarn's `{$oil}` is an interpolation of a declared state path in Lute, `{{run.oil}}`
text.lute:14:48: warning [W-TEXT-COMMENT-LIKE] `# mood:cold` is part of the line text, so the player sees it — Lute has no line tags; keep a note as a `// …` comment on a line of its own
text.lute:15:30: warning [W-TEXT-COMMENT-LIKE] `// TODO darker` is part of the line text, so the player sees it — a comment is `// …` on a line of its own (or after a directive), never after text
text.lute:17:3: warning [W-TEXT-BRACKET-LABEL] choice label `[Go inside]` shows its brackets on the button — a label is shown exactly as written (Lute has no Ink-style bracket suppression); write `label="Go inside"`
ok: text.lute (7 warning(s))
```

Diverts spelled as directives, a backward jump, a heading used as a target, and Ink or Yarn habits
in conditions:

<!-- lute-diagnostics unverified="byte-exact lute check output; the did-you-mean and E-CEL-TYPE reasons are composed outside the file that declares their code, and E-NEXT-UNDEFINED matches two literals" -->
```
$ lute check flow.lute
flow.lute:12:1: error [E-UNKNOWN-DIRECTIVE] unknown directive `::goto` — did you mean `::next{to="…"}`? It jumps forward to a `::mark{id="…"}`
flow.lute:13:12: error [E-NEXT-BACKWARD] `::next` targets mark `top`, which is not forward of this `::next` in document order — `::next` only jumps forward; to offer choices again, use a `<hub>` (it asks until an `exit` choice is taken)
flow.lute:18:12: error [E-NEXT-UNDEFINED] `::next` targets `Gallery`, which no `::mark` or line `id=` in this document declares — `Gallery` is the `## Gallery` heading, not a mark; a `::next` target is a `::mark{id="…"}` (or a line's `id=`) later in this document, so put `::mark{id="Gallery"}` under that heading
failed: flow.lute (3 error(s), 0 warning(s))
$ lute check cond.lute
cond.lute:5:7: error [E-CEL-PROFILE] `once` is not a condition: how often a beat plays is its own key, `once` — `once: run` (once per run, the default), `once: user` (once ever) or `once: false` (every time); on a `<beat>` or `<entry>` it is `once="run"` — drop this `when`
cond.lute:12:17: error [E-CEL-TYPE] `visited('gallery') > 2`: `>` compares numbers, and `visited('gallery')` is a bool — `visited('gallery')` is a bool, whether the scene was ever presented, not how often; count visits in a `number` path you `::set` (for example `::set{run.visits += 1}`)
cond.lute:13:17: error [E-CEL-PROFILE] `$oil`: a state path takes no `$` (`$` alone is the `<match>` subject) — write the path with its tier — did you mean `run.oil`?
cond.lute:14:17: error [E-CEL-TYPE] `run.oil == true` compares a number with a bool, so it is never true
failed: cond.lute (4 error(s), 0 warning(s))
```

The first-visit test from Ink, ported onto the hub's record, is caught too: inside the choice's own
lines the record is already true, so an arm that needs it false can never run.

<!-- lute-diagnostics -->
```
first.lute:11:7: error [E-ARM-DEAD] arm can never fire: its pattern `false` never matches here — `scene.visited.lamp.ledger` is `true` in its option's own arm — the visit record is set when the choice is picked, before its arm runs
```

Every code is in the [diagnostics reference](/reference/diagnostics/), and `lute --explain <CODE>`
prints its entry.
