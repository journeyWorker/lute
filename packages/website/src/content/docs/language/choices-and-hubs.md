---
title: Choices & hubs
description: Choice mechanics — when guards and the into= run-record sugar — plus revisit <hub> conversations with once/exit flags, hub prompts, and the no-dead-end guarantee.
---

A `<choice>` is one option inside a [`<branch>`](/language/branch-match-when/) or a `<hub>`. Every
choice requires an **`id`** (the recorded key) and a **`label`** (the button text, which may
interpolate). Beyond that it carries guards and run-record sugar.

## Guards

A choice may carry a **`when`** CEL guard; the choice is offered only when the guard holds. A
branch must still contain at least one unguarded choice (`E-BRANCH-ALL-GUARDED`) so the menu is
never provably empty.

```lute
<choice id="soft" label="Ask gently" when="@showcaseReady">
  @fixer{code="0052"}: Marina — would you mind terribly if I had your number?
</choice>
```

## Recording a choice

Selecting a choice records its id into the reserved path `scene.choices.<branchId>` (domain: the
branch's choice ids ∪ `unset`). That path clears at episode end, so it drives **intra-episode**
reactions only — a later shot's `<match on="scene.choices.…">`.

To make a choice affect a **later episode** of the same run, record a **named** `run.*` fact with
the `into=` sugar:

```lute
<branch id="sofaHelp">
  <choice id="help" label="Help her up" into="run.metHelpfully">
    @elena: Thank you. I won't forget this.
  </choice>
  <choice id="warmly" label="Help, and stay a while" into="run.sofaHelpOutcome" value="warm">
    @elena: You're very kind — really.
  </choice>
  <choice id="tip" label="Leave a little something" into="run.tip" value="5">
    @elena: Oh — you didn't have to.
  </choice>
</branch>
```

*(From [`docs/examples/choice-persist.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/choice-persist.lute).)*

`into="run.<path>"` alone appends `::set{run.<path> = <value>}` to that arm. The target path is
named by **`into`** and must be declared in your schema. **`value`** defaults to `true` for a
`bool` path; an `enum` or `number` path requires an explicit `value`. (0.6.0 removed the old
`persist="run"` attribute; a stray `persist=` is now `E-PERSIST-REMOVED`, and `lute fix` deletes it
automatically.) A later episode reacts by reading the named fact — never the raw choice key, which
has already cleared:

```lute
<match on="run.metHelpfully">
  <when is="true">
    @elena: You helped me back then. I've been meaning to thank you again.
  </when>
  <otherwise>
    @elena: ...Have we met before?
  </otherwise>
</match>
```

## Revisit hubs

A `<hub id>` is a revisit conversation: on entry, and after each non-`exit` arm completes, it
**re-presents** every currently eligible choice, letting the player weave through them in any
order. Hub choices carry two extra boolean flags — **`once`** and **`exit`**:

```lute
<hub id="chatWithMarina">
  <choice id="askCoffee" label="Ask about the coffee" once>
    @marina{code="0020" emotion="content" variant="0"}: House blend. Bold, like the clientele.
  </choice>
  <choice id="compliment" label="Say she was kind earlier" when="@helped">
    @fixer{code="0030"}: You were gentle about it before. It stuck with me.
    ::set{scene.affect.marina += 1}
  </choice>
  <choice id="leave" label="Head out" exit>
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
  regardless of `once`, so the engine can grey out an already-seen topic. The hub also folds
  `scene.choices.<hubId>` (the last-selected enum) — both are readable in a `<match>`.
- **Exit.** Taking an `exit` choice runs its arm and leaves the hub. If no choice is eligible at a
  presentation point, the hub auto-exits.

### Hub prompts

A hub can say what it is asking. `prompt="…"` (dsl 0.23.0) attaches a line the host shows with the
hub's options every time it presents them, the way a `<branch prompt>` (dsl 0.11.1) does for a
one-off menu:

```lute check
---
kind: scene
id: bar.marina
---

# The bar

## Shot 1.

<hub id="chat" prompt="Marina polishes a glass and waits.">
  <choice id="coffee" label="Ask about the coffee" once>
    @marina: House blend. Bold, like the clientele.
  </choice>
  <choice id="leave" label="Head out" exit>
    @fixer: I'd better get moving.
  </choice>
</hub>
```

The prompt compiles onto the hub record (`prompt` on `HubCmd`, omitted when unauthored), and
`lute run` / `lute play` print it with every presentation of the hub. An empty prompt is
`E-BRANCH-PROMPT`, as it is on a branch.

### No dead ends

A hub must guarantee it can end: it needs at least one **unguarded `exit`** choice, **or** all of
its choices must be `once` (so the eligible set provably empties and auto-exit fires). A hub that
satisfies neither is `E-HUB-NO-EXIT`. Because a hub reduces at build time to one finite option
table plus its arms, its runtime re-presentation adds no unbounded computation — totality is
preserved.
