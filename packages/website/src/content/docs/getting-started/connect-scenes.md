---
title: Connect scenes into a story
description: Chain scenes into a whole story you can play and test without a game engine — one made-up occasion answered by every scene, after for the order, priority for the running order, when for branching endings, and a play script for lute play.
---

[Write your first scene](/getting-started/first-scene/) ended with two scenes and an `after:` line
between them. `after:` says which scene comes *after* which. It does not move the player: something
still has to **start** each scene. In a shipped game that something is the engine. Before an engine
exists, it is **`lute play`**: the tool plays your whole story from the first scene to an ending,
prints every line, and tells you why each scene did or did not play.

This page builds a five-scene whodunit and plays it. Everything here is core Lute: no plugin, no
engine, no programming.

## The idea in one paragraph

`lute play` moves the story forward by raising **occasions**: named moments such as "the next
chapter". Each scene says which occasion it answers with one frontmatter key, `on:`. When an
occasion is raised, every scene that answers it is a candidate. A candidate is **eligible** when its
`after:` and `when:` hold and it has not played yet this run. The eligible scene with the highest
`priority:` plays. Raise the occasion again and the next scene plays.

You do not declare the occasion anywhere. While no plugin declares occasions, any name is accepted,
so you can invent one. This page calls it `chapter`.

## The project

```
a-story/
  lute.project.yaml
  world.schema.yaml
  scenes/
    prologue.lute
    counter.lute
    accusation.lute
    ending/
      caught.lute
      wrong.lute
  plays/
    caught.play.yaml
  tests/
    accusation.test.yaml
```

Make the folder and run every command below from inside it. The files are shown whole, so you can
type the project from this page. The same files are in the repository at
[`docs/examples/connect-scenes/`](https://github.com/journeyWorker/lute/tree/main/docs/examples/connect-scenes)
for reference.

`lute.project.yaml` marks the folder as a project. Its `defaults:` block gives every document the
same `uses:` line, so no scene has to repeat it ([Project defaults](/language/imports/#project-defaults)),
and its `chapters:` block chains the chapters in order:

```yaml
defaultProfile: core
profiles:
  core:
    plugins: {}
defaults:
  uses: [world.schema.yaml]
chapters:
  - on: chapter
    scenes: [prologue, counter, accusation]
```

`world.schema.yaml` declares the one piece of state the story remembers: whom the player accused.

```yaml
state:
  run.accused: { type: { enum: [nobody, ruben, tilly] }, default: nobody }
```

## The chapters: `chapters:`

`chapters:` is a list of chains, one per occasion, so a project can chain several occasions. A
chain names the occasion its scenes answer with `on:` and lists the scenes by their `id:`, in play
order. The scenes themselves only say who they are:

```lute check="docs/examples/connect-scenes/scenes/prologue.lute"
---
kind: scene
id: prologue
title: Closing Time
---

## Closing Time

@narrator: Five minutes to close, and the bakery smells of burnt sugar.
@wren: Mr. Pryce? We're closing.
```

```lute check="docs/examples/connect-scenes/scenes/counter.lute"
---
kind: scene
id: counter
title: The Counter
---

## The Counter

@narrator: Mr. Pryce is face down on the counter. The till is open.
@wren: Somebody here knows what happened.
```

For every listed scene, the chain writes three frontmatter keys you would otherwise write by
hand. `counter` behaves exactly as if its frontmatter said:

```yaml
on: chapter
after: 'visited("prologue")'
priority: 20
```

- `on: chapter` makes the scene a candidate whenever `chapter` is raised.
- `after: 'visited("prologue")'` keeps it waiting until the scene listed before it has played.
  `visited("…")` names a scene by its `id:`. The first scene has no `after:`, so it is eligible
  from the start. `visited()` is save-wide, not per run: a scene stays visited after a new run
  starts, so from the second run on every `after:` in the chain already holds, and only
  `priority:` keeps the chapters in order.
- A scene plays at most once per run, so after the prologue has played it drops out and the counter
  is the one left. The chain writes no `once:`. For a story told across runs, one chapter per run,
  give each listed scene `once: user`, so a chapter played in an earlier run stays spent.
- `priority:` states the running order outright: higher plays first. The chain counts down in
  steps of ten from the first scene (30, 20, 10 for three scenes), so every chapter outranks the
  scenes that come after the chain.

A key the scene writes itself wins over the chain: give one scene its own `priority:` or a
different `after:` and only that key changes. A listed id that no scene declares is `E-CHAPTERS`
in `lute check-project`, with a did-you-mean, and so is a listed scene whose own `on:` answers a
different occasion, a scene listed twice, two chains on one occasion, or an `on:` no plugin
declares. Scenes you do not list, like the endings below, still write their keys by hand.

A chapter that may not play, because its own `when:` reads state the story may never set, stalls
the chain: the next listed scene waits on it through its `after:`, and `lute check-project` warns
(`W-CHAPTER-STALL`). A `when:` that reads only the clock merely delays the chain, so it draws no
warning. Either give the scene after the optional chapter its own `after:` naming the chapter
before it, or, for the first chapter, take it out of the chain and give it its own `on:`. `lute
play` names a derived `after:` in its reasons: `after: visited("pryceWakes") is not satisfied
(written by `chapters:` in lute.project.yaml)`.

On an occasion declared `select: sequence`, which presents every eligible beat in one raise, the
chain writes only `on:` and `priority:`: the listed scenes play one after another in that raise,
in list order. On any other occasion one listed scene plays per raise. On an occasion raised *for*
a target, every listed scene must declare its own `target:`; one without would play for every
target, so it is `E-CHAPTERS`.

The third scene gives the player a choice and remembers it:

```lute check="docs/examples/connect-scenes/scenes/accusation.lute"
---
kind: scene
id: accusation
title: The Accusation
---

## The Accusation

@wren: One of you did this.

<branch id="accuse">
  <choice id="ruben" label="Ruben, the baker">
    ::set{ run.accused = "ruben" }
  </choice>
  <choice id="tilly" label="Tilly, the waitress">
    ::set{ run.accused = "tilly" }
  </choice>
</branch>
```

## Branching endings: `when:`

Two endings answer the same `chapter` after the accusation. `when:` decides which one is eligible,
reading the state the accusation wrote:

```lute check="docs/examples/connect-scenes/scenes/ending/caught.lute"
---
kind: scene
id: ending.caught
title: Caught
on: chapter
after: 'visited("accusation")'
when: "run.accused == 'ruben'"
---

## Caught

@ruben: The sugar tin. Of course you noticed the sugar tin.
```

```lute check="docs/examples/connect-scenes/scenes/ending/wrong.lute"
---
kind: scene
id: ending.wrong
title: The Wrong Name
on: chapter
after: 'visited("accusation")'
when: "run.accused != 'ruben'"
---

## The Wrong Name

@tilly: Me? I was in the back the whole time.
@narrator: Behind her, the baker quietly unties his apron.
```

The two endings share priority `0` and draw no warning: their `when:`s can never both be true, and
the checker can see that. Put the route in `after:` and the condition in `when:`. The quotes nest:
the outer double quotes belong to YAML and the inner single quotes to the condition. If that is new,
read [Quotes and YAML for writers](/getting-started/first-scene/#quotes-and-yaml-for-writers).

## Check it and see the order

```
$ lute check-project .
ok: ./scenes/accusation.lute (0 warning(s))
ok: ./scenes/counter.lute (0 warning(s))
ok: ./scenes/ending/caught.lute (0 warning(s))
ok: ./scenes/ending/wrong.lute (0 warning(s))
ok: ./scenes/prologue.lute (0 warning(s))
ok: . (5 file(s), 0 project-wide warning(s))
```

`lute beats` prints every scene that answers each occasion, in the order they compete:

```
$ lute beats .
project root: .

  chapter — select: first
    #  priority  beat                           kind   once  verdict  after                  when
    1  30        prologue "Closing Time"        scene  run   -        -                      -
    2  20        counter "The Counter"          scene  run   -        visited("prologue")    -
    3  10        accusation "The Accusation"    scene  run   -        visited("counter")     -
    4  0         ending.caught "Caught"         scene  run   -        visited("accusation")  run.accused == 'ruben'
    5  0         ending.wrong "The Wrong Name"  scene  run   -        visited("accusation")  run.accused != 'ruben'

```

## Play it

A **play script** lists the occasions to raise, in order, and the choice to take at each menu. Save
it as `plays/caught.play.yaml`:

```yaml
choose: { accuse: ruben }
steps:
  - occasion: chapter
    expect: { winner: prologue }
  - occasion: chapter
    expect: { winner: counter }
  - occasion: chapter
    expect: { winner: accusation }
  - occasion: chapter
    expect: { winner: ending.caught }
expect:
  state: { run.accused: ruben }
```

- `choose:` maps a branch id to the choice id to pick: at `<branch id="accuse">`, take `ruben`.
- Each step raises `chapter` once. Its `expect: { winner: … }` says which scene should play.
- The last `expect:` checks the state after the whole story.

```
$ lute play . --script plays/caught.play.yaml
── step 1 · chapter ──────────────
  ✓ prologue [scene, priority 30]
  ✗ counter [scene, priority 20] — after: visited("prologue") is not satisfied (written by `chapters:` in lute.project.yaml)
  ✗ accusation [scene, priority 10] — after: visited("counter") is not satisfied (written by `chapters:` in lute.project.yaml)
  ✗ ending.caught [scene, priority 0] — after: visited("accusation") is not satisfied
  ✗ ending.wrong [scene, priority 0] — after: visited("accusation") is not satisfied
  → prologue
@narrator: Five minutes to close, and the bakery smells of burnt sugar.
@wren: Mr. Pryce? We're closing.
── step 2 · chapter ──────────────
  ✓ counter [scene, priority 20]
  ✗ prologue [scene, priority 30] — once: run — already presented this run
  ✗ accusation [scene, priority 10] — after: visited("counter") is not satisfied (written by `chapters:` in lute.project.yaml)
  ✗ ending.caught [scene, priority 0] — after: visited("accusation") is not satisfied
  ✗ ending.wrong [scene, priority 0] — after: visited("accusation") is not satisfied
  → counter
@narrator: Mr. Pryce is face down on the counter. The till is open.
@wren: Somebody here knows what happened.
── step 3 · chapter ──────────────
  ✓ accusation [scene, priority 10]
  ✗ prologue [scene, priority 30] — once: run — already presented this run
  ✗ counter [scene, priority 20] — once: run — already presented this run
  ✗ ending.caught [scene, priority 0] — after: visited("accusation") is not satisfied
  ✗ ending.wrong [scene, priority 0] — after: visited("accusation") is not satisfied
  → accusation
@wren: One of you did this.
▷ choice accuse: [ruben] tilly        ← chosen: ruben
  set run.accused = "ruben"
── step 4 · chapter ──────────────
  ✓ ending.caught [scene, priority 0]
  ✗ prologue [scene, priority 30] — once: run — already presented this run
  ✗ counter [scene, priority 20] — once: run — already presented this run
  ✗ accusation [scene, priority 10] — once: run — already presented this run
  ✗ ending.wrong [scene, priority 0] — when: false
  → ending.caught
@ruben: The sugar tin. Of course you noticed the sugar tin.
── end: complete (4 steps) ──────────────
── expect: every expectation held ──────────────
```

Each step lists the candidates: `✓` is eligible, `✗` is not, with the reason. `→` is the scene that
played. Read it when a scene does not play where you expected: the reason is on its line. Change
`choose:` to `{ accuse: tilly }` and step 4 plays `ending.wrong` instead.

## Test it

`lute test` runs every play script that carries an `expect:`, so the play above is already a test.
A **scenario test** checks one scene on its own. Save `tests/accusation.test.yaml`:

```yaml
file: ../scenes/accusation.lute
visited: [counter]
choose: { accuse: tilly }
expect:
  transcriptContains: ["@wren: One of you did this."]
  state: { run.accused: tilly }
```

`file:` is relative to the test file, so a test in `tests/` names its scene `../scenes/…`.
`visited:` pretends the counter scene has already played. Leave it out and the test tells you it is
needed, because the accusation waits for `after: 'visited("counter")'`:

```
$ lute test . --project .
FAIL  ./tests/accusation.test.yaml  (./scenes/accusation.lute)
      eligible accusation: not eligible under these mocks (its `after: visited("counter")` (written by `chapters:` in lute.project.yaml) is false — add `visited: [counter]` to the mocks) — the engine would never present it, so the walk proves nothing about play; fix the mocks, or assert `expect: { eligible: { accusation: false } }` (the body is then not walked)
PASS  ./plays/caught.play.yaml  (play of .)

1 passed, 1 failed
```

With `visited: [counter]` in place:

```
$ lute test . --project .
PASS  ./tests/accusation.test.yaml  (./scenes/accusation.lute)
PASS  ./plays/caught.play.yaml  (play of .)

2 passed, 0 failed
```

The test file format is in the [CLI reference](/tooling/cli/#test), and every play script key is in
[Playing a story](/tooling/play/).

## Adding a scene

`lute new scene <name> --occasion chapter` writes a new scene named after its `id:`, which it takes
from the name as you type it (`lute new scene pryceWakes --occasion chapter` writes
`scenes/pryceWakes.lute` with `id: pryceWakes`). Because `chapter` is the occasion of a chain in
your `chapters:`, it leaves `on:`, `after:` and `priority:` out and tells you to add the id to that
chain: put it where it belongs in the list, and the chain gives it all three. Then add a step to
your play script. (On an occasion no chain answers, the command writes `on:` and a `priority:`
below every beat already there.)

A conversation the player chooses, rather than the next chapter, is a second occasion raised
*for* someone. The scene names the person with `target:`:

```yaml
on: talk
target: npc.tilly
after: 'visited("counter")'
when: "!visited('accusation')"
```

In the play script the step names the person too: `- occasion: talk` with `target: npc.tilly`.
The `when:` closes the conversation once the accusation has played.

## When the engine arrives

Nothing on this page is a stand-in. When the game is built, the engine raises `chapter` where the
play script did, and presents the scene that wins, exactly as `lute play` showed. When the engine's
plugin declares its occasions, the checker then also catches a misspelled `on:`. See
[Occasions](/tooling/play/#occasions) and [Beats](/language/beats/).
