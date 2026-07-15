---
title: Choices & hubs
description: Choice mechanics — when guards and the persist run-fact sugar — plus revisit <hub> conversations with once/exit flags and the no-dead-end guarantee.
---

A `<choice>` is one option inside a [`<branch>`](/language/branch-match-when/) or a `<hub>`. Every
choice requires an **`id`** (the recorded key) and a **`label`** (the button text, which may
interpolate). Beyond that it carries guards and persistence sugar.

## Guards

A choice may carry a **`when`** CEL guard; the choice is offered only when the guard holds. A
branch must still contain at least one unguarded choice (`E-BRANCH-ALL-GUARDED`) so the menu is
never provably empty.

```lute
<choice id="soft" label="Ask gently" when="@showcaseReady">
  @fixer{code="0052"}: Bianca — would you mind terribly if I had your number?
</choice>
```

## Recording a choice

Selecting a choice records its id into the reserved path `scene.choices.<branchId>` (domain: the
branch's choice ids ∪ `unset`). That path clears at episode end, so it drives **intra-episode**
reactions only — a later shot's `<match on="scene.choices.…">`.

To make a choice affect a **later episode** of the same run, persist a **named** `run.*` fact with
the `persist` sugar:

```lute
<branch id="sofaHelp">
  <choice id="help" label="Help her up" persist="run" into="run.metHelpfully">
    @sofia: Thank you. I won't forget this.
  </choice>
  <choice id="warmly" label="Help, and stay a while" persist="run" into="run.sofaHelpOutcome" value="warm">
    @sofia: You're very kind — really.
  </choice>
  <choice id="tip" label="Leave a little something" persist="run" into="run.tip" value="5">
    @sofia: Oh — you didn't have to.
  </choice>
</branch>
```

*(From [`docs/examples/choice-persist.lute`](https://github.com/KantoRegion/lute/blob/main/docs/examples/choice-persist.lute).)*

`persist="run"` appends `::set{run.<path> = <value>}` to that arm. The target path is named by
**`into`** and must be declared in your schema. **`value`** defaults to `true` for a `bool` path;
an `enum` or `number` path requires an explicit `value`. A later episode reacts by reading the
named fact — never the raw choice key, which has already cleared:

```lute
<match on="run.metHelpfully">
  <when test="$ == true">
    @sofia: You helped me back then. I've been meaning to thank you again.
  </when>
  <otherwise>
    @sofia: ...Have we met before?
  </otherwise>
</match>
```

## Revisit hubs

A `<hub id>` is a revisit conversation: on entry, and after each non-`exit` arm completes, it
**re-presents** every currently eligible choice, letting the player weave through them in any
order. Hub choices carry two extra boolean flags — **`once`** and **`exit`**:

```lute
<hub id="chatWithBianca">
  <choice id="askCoffee" label="Ask about the coffee" once>
    @bianca{code="0020" emotion="content" variant="0"}: House blend. Bold, like the clientele.
  </choice>
  <choice id="compliment" label="Say she was kind earlier" when="@helped">
    @fixer{code="0030"}: You were gentle about it before. It stuck with me.
    ::set{scene.affect.bianca += 1}
  </choice>
  <choice id="leave" label="Head out" exit>
    @fixer{code="0040"}: I'd better get moving.
  </choice>
</hub>
```

*(From [`docs/examples/showcase/hub-demo.lute`](https://github.com/KantoRegion/lute/blob/main/docs/examples/showcase/hub-demo.lute).)*

- **Eligibility.** A choice is eligible when its `when` guard (if any) holds and — if flagged
  `once` — it has not yet been taken in this scene.
- **`once`** removes a choice from the eligible set after its first take. A choice without `once`
  stays selectable and may be re-taken.
- **Recording.** *Every* selection sets `scene.visited.<hub>.<choice> = true` (default `false`),
  regardless of `once`, so the engine can grey out an already-seen topic. The hub also folds
  `scene.choices.<hubId>` (the last-selected enum) — both are readable in a `<match>`.
- **Exit.** Taking an `exit` choice runs its arm and leaves the hub. If no choice is eligible at a
  presentation point, the hub auto-exits.

### No dead ends

A hub must guarantee it can end: it needs at least one **unguarded `exit`** choice, **or** all of
its choices must be `once` (so the eligible set provably empties and auto-exit fires). A hub that
satisfies neither is `E-HUB-NO-EXIT`. Because a hub reduces at build time to one finite option
table plus its arms, its runtime re-presentation adds no unbounded computation — totality is
preserved.
