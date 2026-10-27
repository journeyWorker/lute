---
title: Choices & hubs
description: Choice mechanics — when guards and the into= run-record sugar — plus revisit <hub> conversations with once/exit flags, hub prompts, and the no-dead-end guarantee.
---

A `<choice>` is one option inside a [`<branch>`](/language/branch-match-when/) or a `<hub>`. Every
choice requires an **`id`** (the recorded key) and a **`text`** (the button text, which may
interpolate; it takes the [label grammar](/language/dialogue-and-cast/#labels-and-prompts), so
`\{{` writes a literal `{{`). Beyond that it carries guards and run-record sugar.

## Guards

A choice may carry a **`when`** CEL guard; the choice is offered only when the guard holds. A
branch must still contain at least one unguarded choice (`E-BRANCH-ALL-GUARDED`) so the menu is
never provably empty.

```lute
<choice id="soft" text="Ask gently" when="@showcaseReady">
  @fixer{code="0052"}: Marina — would you mind terribly if I had your number?
</choice>
```

## Recording a choice

Selecting a choice records its id into the reserved path `scene.choices.<branchId>` (domain: the
branch's choice ids ∪ `unset`). That path clears at episode end, so it drives **intra-episode**
reactions only — a later section's `<match subject="scene.choices.…">`.

To make a choice affect a **later episode** of the same run, record a **named** `run.*` fact with
the `into=` sugar:

```lute
<branch id="sofaHelp">
  <choice id="help" text="Help her up" into="run.metHelpfully">
    @elena: Thank you. I won't forget this.
  </choice>
  <choice id="warmly" text="Help, and stay a while" into="run.sofaHelpOutcome" value="warm">
    @elena: You're very kind — really.
  </choice>
  <choice id="tip" text="Leave a little something" into="run.tip" value="5">
    @elena: Oh — you didn't have to.
  </choice>
</branch>
```

*(From [`docs/examples/choice-persist.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/choice-persist.lute).)*

### `into=` writes first

`into="run.<path>"` (with its optional `value`) means `::set{run.<path> = <value>}` placed **first** in
the arm, before any line or directive in it. Since 0.49.0 the checker sees it that way: a read of the
path inside the arm is already assigned, so no `has()` guard or `unset` arm is needed there. An arm
that ends in `::jump` records the write before it leaves. The `E-INTO-*` diagnostics, and the error
for an arm that already `::set`s the same path, are unchanged.

`into="run.<path>"` alone is that `::set{run.<path> = <value>}`. The target path is
named by **`into`** and must be declared in your schema. **`value`** defaults to `true` for a
bool path; an `enum` or numeric (`int` or `double`) path requires an explicit `value`. (0.6.0 removed the old
`persist="run"` attribute; a stray `persist=` is now `E-PERSIST-REMOVED`, and `lute fix` deletes it
automatically.) A later episode reacts by reading the named fact — never the raw choice key, which
has already cleared:

```lute
<match subject="run.metHelpfully">
  <when is="true">
    @elena: You helped me back then. I've been meaning to thank you again.
  </when>
  <otherwise>
    @elena: ...Have we met before?
  </otherwise>
</match>
```

## Free-text input

Lute has no text-entry construct and does not need one: asking the player to type an answer is a
[bridge](/plugins/bridge/) call. A plugin declares a blocking bridge directive whose `effects.writes`
entry reads `fromBridgeResult`; the host shows its text box, and the typed answer lands in a state
slot the directive declares. A `<match>` or `<branch>` then reads that slot like any other state.

```lute
@guard: Halt. What is the password?

::ask{slot="password"}

<match subject="scene.ask.password.answer">
  <when is="swordfish">
    @guard: Pass, friend.
  </when>
  <otherwise>
    @guard: Wrong. Move along.
  </otherwise>
</match>
```

The call blocks, so `scene.ask.password.answer` is assigned after `::ask` (see
[Sync semantics](/plugins/bridge/#sync-semantics)) and the `<match>` reads it
without a guard. Write `wait="false"` only for a fire-and-continue call: a non-blocking call adds no
definite-assignment guarantee, so a read of an undefaulted result that nothing else
protects can report `E-MAYBE-UNSET`.
The host half — what it receives and how it answers — is in the
[runtime contract](/tooling/runtime-contract/). The checked example is
[`docs/examples/free-text-input/`](https://github.com/journeyWorker/lute/tree/main/docs/examples/free-text-input), with a
`lute test` for the right and the wrong answer.

## The compiled record

A `<branch>` compiles to one `choice` record. A `prompt="…"` is the line the host shows above the
options, and `timeout="N"` is a countdown in whole seconds (a positive integer, else
`E-BRANCH-TIMEOUT`). A timed branch MUST also name its timeout outcome with
`default="<choice id>"`; the two attributes require each other, the id must be a direct option,
and that option cannot carry `when` (`E-BRANCH-DEFAULT`).

```lute check
---
kind: scene
id: bar.counter
title: The number
---

## Counter

<branch id="number" prompt="Ask for her number?" timeout="10" default="soft">
  <choice id="blunt" text="Just ask, flatly">
    @fixer: Marina. Your number.
  </choice>
  <choice id="soft" text="Ask gently">
    @fixer: Would you mind terribly?
  </choice>
</branch>
```

```json
{
  "kind": "choice",
  "family": "control",
  "position": "001-0100",
  "branchId": "number",
  "selectionKey": "scene.choices.number",
  "options": [
    {"id": "blunt", "text": "Just ask, flatly", "lineId": "bar.counter.number.blunt", "target": "001-0200"},
    {"id": "soft", "text": "Ask gently", "lineId": "bar.counter.number.soft", "target": "001-0400"}
  ],
  "converge": "001-0600",
  "prompt": "Ask for her number?",
  "promptLineId": "bar.counter.number.prompt",
  "timeout": 10,
  "default": "soft"
}
```

- **`selectionKey`** is the path the pick is recorded into, `scene.choices.<branchId>`.
- Each option carries its button **`text`**, a `lineId` of `{prefix}.{branch id}.{choice id}` for
  translation, and the `target` position its arm starts at. Every arm continues at `converge`.
- **`prompt`**, **`timeout`** and **`default`** are present only when authored. `timeout` and
  `default` require each other, and expiry takes the named option exactly as if it had been
  selected, so a timed branch always has an outcome.
- A prompt is translatable like an option label (dsl 0.47.0): it gets the lineId
  **`promptLineId`**, `{prefix}.{branch id}.prompt`, present exactly when `prompt` is.
  [`lute loc export`](/tooling/cli/#loc-export) writes it as a `kind: "prompt"` row and
  `lute compile --locale-bundle` merges its translations into **`promptTexts`** (locale → text, omitted
  when empty); `prompt` stays the source string. Because the prompt and the options share the
  `{prefix}.{branch id}.…` namespace, a `<choice id="prompt">` in a branch that has `prompt=` is
  `E-CHOICE-DUP` — rename the choice. Without `prompt=` the id is legal.
- Since 0.52 a prompt takes the label grammar, as an option's `text` does: its `{{…}}` markers are
  checked as a label's (an undeclared path is `E-CEL-PROFILE`, a maybe-unset one `E-MAYBE-UNSET`,
  read before the options), and a prompt with a marker compiles **`promptPlaceholders`** beside
  `prompt`, built as an option's `placeholders` are.

A `<hub>` compiles to a `hub` record of the same shape: `id`, `selectionKey`
(`scene.choices.<hubId>`), `options` whose entries add the `once` and `exit` flags, `converge`, and
an optional `prompt` (with its `promptLineId`, `promptTexts` and `promptPlaceholders`, as on a branch) and, for a hub
with a [`<return>`](#coming-back-return) block, the `return` position that block starts at.

### The menu shows rendered text

The IR keeps each label and prompt as written, markers and all. Since 0.52 the runtime renders them
at every presentation, as it renders a line, and puts the result on the `awaitChoice` menu: each
option's **`text`** (and its translations, **`texts`**), the **`prompt`** and **`promptTexts`**. A
host shows those, not the IR's strings. For this branch, with `run.gold` at 5:

```lute check
---
kind: scene
id: shop.toll
title: The toll
state:
  run.gold: { type: int, default: 5 }
---

## Gate

<branch id="toll" prompt="You have {{run.gold}} gold.">
  <choice id="pay" text="Pay {{run.gold}} gold">
    @narrator: Paid.
  </choice>
  <choice id="code" text="Say \{{open}}">
    @narrator: The gate hums.
  </choice>
</branch>
```

Played in a project as `scenes/gate.lute`, `lute play --events` shows the menu the runtime awaits on:

```json
{"construct":"branch","document":"scenes/gate.lute","id":"toll","options":[{"exit":false,"id":"pay","once":false,"text":"Pay 5 gold","verdict":"open"},{"exit":false,"id":"code","once":false,"text":"Say {{open}}","verdict":"open"}],"position":"001-0100","presentation":0,"prompt":"You have 5 gold."}
```

A hub renders again at each presentation, so a label reads the state the last option left. A host
token (`{{userName}}`) and a marker whose path is unset keep their written text, as in a line, and
menu text never stops the walk: the menu is still presented.

## Revisit hubs

A `<hub id>` is a revisit conversation: on entry, and after each non-`exit` arm completes, it
**re-presents** every currently eligible choice, letting the player weave through them in any
order. Hub choices carry two extra boolean flags — **`once`** and **`exit`**:

```lute
<hub id="chatWithMarina">
  <choice id="askCoffee" text="Ask about the coffee" once>
    @marina{code="0020" emotion="content" variant="0"}: House blend. Bold, like the clientele.
  </choice>
  <choice id="compliment" text="Say she was kind earlier" when="@helped">
    @fixer{code="0030"}: You were gentle about it before. It stuck with me.
    ::set{scene.affect.marina += 1}
  </choice>
  <choice id="leave" text="Head out" exit>
    @fixer{code="0040"}: I'd better get moving.
  </choice>
</hub>
```

*(From [`docs/examples/showcase/hub-demo.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/showcase/hub-demo.lute).)*

- **Eligibility.** A choice is eligible when its `when` guard (if any) holds and — if flagged
  `once` — it has not yet been taken in this scene.
- **`once`** removes a choice from the eligible set after its first take. A choice without `once`
  stays selectable and may be re-taken.
- **Recording.** *Every* selection sets `scene.visited.<hub>.<choice> = true` (default `false`),
  regardless of `once`, so the engine can grey out an already-seen topic. It is set when the
  choice is picked, before its arm runs, so inside that arm it is always `true` (a `<match>` arm
  needing it `false` there is `E-ARM-DEAD`). To tell the first pick from later ones, count picks
  in a `scene.*` int path the arm sets. The hub also folds `scene.choices.<hubId>` (the
  last-selected enum) — both are readable in a `<match>`.
- **Exit.** Taking an `exit` choice runs its arm and leaves the hub. If no choice is eligible at a
  presentation point, the hub auto-exits.
- **Flags take no value.** `once` and `exit` are written bare (`exit="true"` / `exit="false"`
  also read as written). Any other value is `E-FLAG-VALUE`, and so is a beat/entry period such as
  `once="run"`: a choice's `once` means once per hub visit, nothing longer. The same rule covers
  `<objective optional>` and `<beat also>`.

### Hub prompts

A hub can say what it is asking. `prompt="…"` (dsl 0.23.0) attaches a line the host shows with the
hub's options every time it presents them, the way a `<branch prompt>` (dsl 0.11.1) does for a
one-off menu:

```lute check
---
kind: scene
id: bar.marina
title: The bar
---

## Bar

<hub id="chat" prompt="Marina polishes a glass and waits.">
  <choice id="coffee" text="Ask about the coffee" once>
    @marina: House blend. Bold, like the clientele.
  </choice>
  <choice id="leave" text="Head out" exit>
    @fixer: I'd better get moving.
  </choice>
</hub>
```

The prompt compiles onto the hub record (`prompt` on `HubCmd`, omitted when unauthored, with
`promptPlaceholders` when it has a marker), and `lute run` / `lute play` print it with every
presentation of the hub; the `awaitChoice` menu carries it rendered. An empty prompt is
`E-BRANCH-PROMPT`, as it is on a branch. Like a branch prompt it is localizable: lineId
`{prefix}.{hub id}.prompt` (`promptLineId`, here `bar.marina.chat.prompt`), translations in
`promptTexts`, and a `<choice id="prompt">` in a prompted hub is `E-CHOICE-DUP`.

### Coming back: `<return>`

Text written before the `<hub>` runs once. For text that runs each time the player comes back to
the hub, put a `<return>` block inside it (dsl 0.28.0). It runs after each non-`exit` arm, before
the options are shown again. It does not run before the first menu or after an `exit` arm. When a
`once` pick empties the menu, it still runs, just before the hub closes:

```lute check
---
kind: scene
id: lighthouse.lamp
title: The lighthouse
---

## Lamp room

@narrator: A lamp room, all brass and salt.
<hub id="lamp">
  <return>
    @narrator: The lamp room again.
  </return>
  <choice id="ledger" text="Read the ledger">
    @narrator: The last entry is smudged.
  </choice>
  <choice id="leave" text="Go down the stairs" exit>
    @narrator: You leave the lamp burning.
  </choice>
</hub>
```

`lute trace` with `--choice lamp=ledger,ledger,leave` prints "The lamp room again." twice. A hub
takes at most one `<return>` block, and the block takes no attributes. To vary the text, guard its
lines with `when=` or wrap them in a `<match>`. [Coming from Ink or
Yarn](/guides/coming-from-ink-yarn/) ports an Ink knot the player keeps returning to this way.

### Leaving a hub with `::jump`

A forward [`::jump`](/language/directives/#jump-and-label--forward-and-bounded-backward-jumps) from an option body (or
from `<return>`) to a label after the hub **leaves the hub** (0.52, #216): the visit ends there, no
`<return>` runs, and the menu is not presented again. A jump out of two nested hubs leaves both.

```lute check
---
kind: scene
id: office.desk
title: The desk
---

## Office

@narrator: Before the hub.
<hub id="desk">
  <choice id="take" text="Take it">
    @narrator: Taking it, then jumping past the hub.
    ::jump{to="after"}
  </choice>
  <choice id="leave" text="Leave" exit>
    @narrator: Leaving.
  </choice>
</hub>
::label{name="after"}
@narrator: After the hub.
```

```console
$ lute trace desk.lute --choice desk=take,leave
trace: desk.lute  (seeds: 0 paths, 0 facts; 2 selections)
  ## Office
    @narrator  Before the hub.
  <hub desk>   eligible: take, leave   -> take
    @narrator  Taking it, then jumping past the hub.
    <jump -> after>
    <label>
    @narrator  After the hub.
trace complete: 1 decision; choices 1/2 (desk)
```

The scripted `leave` is never needed: `take` already left. The compiled `jump` carries
**`leaves`**, the number of hub bodies it leaves (here `"leaves": 1`), and an artifact with such a
jump requires the semantic id `lute.core.hubLeave/1` (see the
[runtime contract](/tooling/runtime-contract/)).

A jump may not go the other way. An option body, a `<return>` body, an `<on>` or `<objective>` body,
a lore entry and a beat each run as their own range, so a jump into one of them from outside is
`E-JUMP-INTO-BODY` (0.52):

<!-- lute-diagnostics unverified="verbatim lute check output; two format! literals in crates/lute-check/src/next_labels.rs match this text, so the scraper cannot pin it to one" -->
```
into.lute:11:16: error [E-JUMP-INTO-BODY] `::jump{to="mid"}` enters the body of hub `desk`'s option `take`, which runs only when that option is chosen — jump to a label in the same body, or after the hub
```

A jump stays in its own body or leaves hub option and `<return>` bodies outward. `<choice>` bodies
of a `<branch>` and `<match>` arms run inline, so a jump may cross them as before.

### Who is on stage at the options

An option can run after any sequence of earlier picks, so since 0.52 (#217) the checker and the
compiler judge each option from the **hub head**: the stage on entry joined with the stage every
non-`exit` option (and `<return>`) can leave behind, iterated until it stops changing. A character
that one option sends off is therefore maybe-absent in every other option:

```lute check
---
kind: scene
id: desk
title: The desk
enums:
  action:
    members: [show, hide]
    exits: [hide]
---

## Desk

@mira: Hi.
<hub id="desk">
  <choice id="send" text="Send her away">
    ::actor{character="mira" action="hide"}
  </choice>
  <choice id="talk" text="Talk">
    @mira: Still here.
  </choice>
  <choice id="leave" text="Leave" exit>
    @narrator: Bye.
  </choice>
</hub>
```

<!-- lute-diagnostics unverified="verbatim lute check output; the second message splices ACTOR_LOWERING_HINT (crates/lute-check/src/inject.rs), whose span two literals match, so the scraper cannot pin it to one" -->
```
head.lute:16:5: warning [W-STAGE-ABSENT] `mira` already left on the declared exit at line 16 on a path that reaches here and has not been shown again, so this exit does nothing. Delete it, or show them again with an `::actor` before it
head.lute:19:5: warning [W-STAGE-ABSENT] `mira` left the stage on an earlier declared exit (line 16) on a path that reaches here and has not been shown again, so a spoken line here stages someone who is not present. Show them again with an `::actor` (or a plugin directive declared `lower: { record: actor, fields: { character: { fromAttr: character } } }`) before this point, or remove the earlier exit
```

A `<return>` that shows her again keeps `talk` clean, since every way back to the options passes
it. An `exit` option never feeds the head, and the stage after the hub joins the head with the exit
options' ends. The compiler injects the anchors and preloads this same state calls for.

Where routes join (after a hub, a `<branch>`, a `<match>` or at a label), a character on stage on
every route stays on stage even when the routes left them with different emotions or poses (#222):
the sprite becomes unknown rather than the character maybe-absent, so the next plain line still
gets its `auto-pose-reset`, and the next `::actor` of them injects what a fresh show does.

### No dead ends

A hub must guarantee it can end: it needs at least one **unguarded `exit`** choice, **or** all of
its choices must be `once` (so the eligible set provably empties and auto-exit fires). A hub that
satisfies neither is `E-HUB-NO-EXIT`. Because a hub reduces at build time to one finite option
table plus its arms, its runtime re-presentation adds no unbounded computation — totality is
preserved.

## A hub, or a loop

Both ask the player again, but they answer different questions. Since 0.51 a bounded
[`::jump{to="…" max="…"}`](/language/directives/#bounded-backward-jumps--max) can send the walk back
to an earlier `::label`, so a `<branch>` can be asked more than once.

- **Use a `<hub>`** when the player decides when to leave: a conversation with several topics, a
  menu that stays until an `exit` choice is taken, with `once` choices that drop out and a
  `<return>` block between visits. The player's cooperation ends it, and the checker proves it can
  end ([No dead ends](#no-dead-ends)).
- **Use a loop** when the *game* decides how often: a retry after a failed minigame, "try again
  (at most three times)", a prompt repeated a fixed number of times. The bound is a literal `max`
  from 1 to 99; the walk ends without the player's cooperation.

```lute
::label{name="retry"}
<branch id="again" prompt="The lock is still stuck.">
  <choice id="push" text="Push harder">
    @mira: It won't budge.
    ::jump{to="retry" max="2"}
  </choice>
  <choice id="leave" text="Give up">
    @mira: Fine. Another way, then.
  </choice>
</branch>
```

Here `push` sends the walk back to the label, and after two takes the third `push` falls through
past the `<branch>`. A `<branch>` reached again is a new menu, and its `presentation` stays `0`. A
`<hub>` inside a loop starts each visit at presentation 0, but a `once` choice taken on an earlier
visit stays spent: its memory is the scene's visit record (`scene.visited.<hub>.<choice>`), so the
hub offers only what is left. A loop may sit inside a hub option body (put the `::label` inside that
option), but a loop cannot jump out of a hub option from inside it: `E-JUMP-LOOP-SCOPE`. A forward
jump can: it [leaves the hub](#leaving-a-hub-with-jump).

## Plugin stamps on `<branch>`, `<hub>` and `<choice>`

Since 0.50 a plugin can attach its own attributes to these three elements. A [`stampAttrs`](/plugins/manifests/#scoping-a-stamp-to-surfaces-on) entry whose `on:` names `branch`, `hub` or `choice` is admissible there, and nowhere else on them: a cross-cutting stamp (no `on:`) is still `E-UNKNOWN-ATTR` on `<branch>`, `<hub>` and `<choice>`. With this manifest:

```yaml
stampAttrs:
  - { name: menuType, type: { enum: [plain, timed, minigame] }, on: [branch, hub] }
  - { name: optionKey, type: string, required: true, on: [choice] }
```

a menu is written

```lute
<branch id="route" prompt="Which way?" menuType="plain">
  <choice id="goLeft" text="Left" optionKey="L1">
    @mira: Left it is.
  </choice>
  <choice id="goRight" text="Right" optionKey="R1">
    @mira: Right, then.
  </choice>
</branch>
```

The same attribute may be scoped to several of the three (`on: [branch, choice]`), and a hub's choices are `choice` too. Resolution is the usual strict order: the element's own attributes win (`id`, `text`, `when`, `into`, `value`, `once`, `exit` on a choice; `id`, `prompt`, `timeout`, `default` on a branch; `id`, `prompt` on a hub), then the stamp declared for that surface, then `E-UNKNOWN-ATTR`.

- **Unknown here.** A name that is a stamp for other surfaces only is `E-UNKNOWN-ATTR` that says where it applies, instead of listing the element's attributes: `` `<branch>` has no attribute `optionKey`: it is a stamp of `acme.stage` for `<choice>` only ``. When the name is no stamp at all, the message lists the element's attributes and the stamps it admits: `` `<choice>` has no attribute `cue`; its attributes are `id`, `text`, `when`, `into`, `value` and the stamps `optionKey` (`acme.stage`) ``.
- **Required.** A stamp declared `required: true` for the surface and missing from it is `E-MISSING-ATTR` at the element's opening tag: `` `<choice id="goLeft">` requires attribute `optionKey` (a stamp of `acme.stage`) ``. A declared `default:` does not fill it in.
- **Typing.** The value is typed by the surface's declaration. A folded value outside an enum is `E-BAD-ENUM` (`<hub menuType=@m>` where `@m` folds to `"grid"`, not in the enum), and a wrong scalar is `E-ATTR-TYPE`. A `@ref` value is typed by the same declaration: `<branch menuType=@m>` with `@m` an `int` def is `E-REF-TYPE`, and `<choice optionKey=@k>` with `@k` a string def is clean.

**In the artifact.** The `choice` or `hub` record carries the `<branch>` / `<hub>` element's stamps in its `stamps` object, and each option carries the `<choice>` element's stamps in its own `stamps`, serialized last. Each value is typed by the declaration of its surface. An option without stamps is byte-identical to before, and a stamp that was not authored is never injected:

```json
{
  "kind": "choice",
  "branchId": "route",
  "options": [
    {"id": "goLeft", "text": "Left", "lineId": "…", "target": "001-0200", "stamps": {"optionKey": "L1"}},
    {"id": "goRight", "text": "Right", "lineId": "…", "target": "001-0400", "stamps": {"optionKey": "R1"}}
  ],
  "stamps": {"menuType": "plain"}
}
```

Stamps are descriptive: the runtime never reads them, and the `awaitChoice` menu does not carry them. The reference player's `ChoiceView` carries the element's `stamps` and each option's `stamps`, read from the IR record at the menu's address; each is omitted when absent, and a presenter receives them through `showChoice`. A host that does not use the reference player reads the same fields from the IR record (see the [runtime contract](/tooling/runtime-contract/)); option text and the prompt it takes from the menu, already rendered.
