# Write your first Lute scene

This is the "start here" for a scenario writer who has never touched Lute — no compiler
background required. It builds **one small real scene** from an empty file, step by step,
running the actual `lute` tool at every step so you can see exactly what it says.

The other documents in this repo (the `docs/proposals/scenario-dsl/` spec stack) are the
**normative reference** — the precise rulebook the tool itself is built from. You will not need
them to finish this tutorial. Read them later, only for the specific thing you need to look up
(the last section below tells you where).

## What you need

1. A plain-text editor.
2. A terminal.
3. The `lute` command-line tool, built once from the repo root:

   ```sh
   cargo build -p lute-cli
   ```

   This produces `./target/debug/lute`. Every command below is that program — from the repo
   root, run it as `./target/debug/lute <command> <file>`.

Everything you write in this tutorial is **core Lute only** — no plugins, no project
configuration. Just the language itself.

## Part 1 — The minimal skeleton

Create an empty file, `my-scene.lute`, and run the checker on it — the checker is the tool that
tells you whether a `.lute` file is valid:

```
$ ./target/debug/lute check my-scene.lute
my-scene.lute:1:1: error [E-KIND-MISSING] required frontmatter key `kind` is missing; every root document must declare `kind: scene` or `kind: quest` (dsl 0.2.0 §3.1)
my-scene.lute:1:1: error [E-META-MISSING] required meta key `character` is missing
my-scene.lute:1:1: error [E-META-MISSING] required meta key `season` is missing
my-scene.lute:1:1: error [E-META-MISSING] required meta key `episode` is missing
```

That's the whole idea of `lute check`: it reads your file and tells you, line by line, exactly
what is wrong and why — never a silent failure. Every `.lute` file starts with a YAML
**frontmatter block** (between two `---` lines) that answers "what is this document, and whose
scene is it?". Add one:

```yaml
---
kind: scene
title: A Quiet Table
character: mira
season: 1
episode: 1
pov: fixer
---
```

- `kind: scene` — this file is a scene (the other kind, `quest`, is for quest-logic files —
  not what you're writing today).
- `character` — whose episode this is (the point-of-view character's storyline).
- `season` / `episode` — which episode this scene belongs to.
- `pov` — the id of the player character (the protagonist the player controls).

Save that and re-check:

```
$ ./target/debug/lute check my-scene.lute
ok: my-scene.lute (0 warning(s))
```

Clean — but the file has no content yet. Try adding a line of narration directly under the
frontmatter:

```
@narrator: The diner is empty at this hour, and Mira likes it that way.
```

Check again:

```
$ ./target/debug/lute check my-scene.lute
my-scene.lute:10:1: error [E-CONTENT-OUTSIDE-SHOT] content lives inside a shot; add a `## <title>` heading above it (dsl 0.6 §3.3)
```

This is the rule to remember: **all content lives under a heading.** A Lute document is a
sequence of "shots" — beats of the scene — and every line of dialogue, narration, or staging has
to sit inside one. Add a heading before the line:

```
## The Counter

@narrator: The diner is empty at this hour, and Mira likes it that way.
```

(The heading is free text after `## ` — `## The Counter`, `## Scene 1. The diner`, `## Prologue` are all
valid. `The Counter`, `The Regular`, … stays a fine convention, but the number is not grammar: shots are
numbered by their document order.)

```
$ ./target/debug/lute check my-scene.lute
ok: my-scene.lute (0 warning(s))
```

That's the whole skeleton: frontmatter, a heading, one line under it. Everything from here is
just filling it in.

## Part 2 — Speaking, narrating, feeling

A content line always has the same shape: `@who{attributes}: what they say`. Narration uses the
reserved speaker `@narrator`. Add a line where Mira speaks:

```
@mira{emotion="content" variant="0"}: {{userName}}, you made it.
```

- `@mira` is the speaker. `emotion="content"` and `variant="0"` pick which portrait/pose to show
  — these are catalog vocabulary, not something you invent; `lute context` (Part 4) lists the
  legal values for your project.
- `{{userName}}` is an **interpolation** — text wrapped in double braces gets filled in at
  runtime. `{{userName}}` is the one that's always available: the player's own name.

Now add an inner-voice line for Mira — her private thought, not spoken aloud:

```
@mira{mono}: I should not be this pleased about a coffee order.
```

`{mono}` is a **delivery flag**: a bare word in the braces (no `=value`) that changes how the
line is delivered. `{mono}` means "this is that character's inner monologue" — it renders as
thought, not speech, and works for any character, not just the player. Two other delivery flags
exist alongside it: `{os}` marks a line as **off-screen** (the speaker is heard but not currently
staged/visible), and `{vo}` marks it as **voiceover** (narration-style delivery layered over the
scene). All three are mutually exclusive — at most one per line — and none is allowed on
`@narrator`. `lute context` (Part 4) always lists the full set with its meanings.

Check the file again:

```
$ ./target/debug/lute check my-scene.lute
ok: my-scene.lute (0 warning(s))
```

The file so far:

```lute
---
kind: scene
title: A Quiet Table
character: mira
season: 1
episode: 1
pov: fixer
---

## The Counter

@narrator: The diner is empty at this hour, and Mira likes it that way.

@mira{emotion="content" variant="0"}: {{userName}}, you made it.

@mira{mono}: I should not be this pleased about a coffee order.
```

## Part 3 — Giving the player a choice

A `<branch>` presents the player with a menu; each `<choice>` inside it is one option, with its
own `id`, a `label` (the button text), and the lines that play if the player picks it.

Sometimes a choice should only appear under certain conditions — say, only if the player has met
Mira before. That's a **guard**: `when="<condition>"`. Guards read declared **state** — a small
named value the engine tracks — so first declare one in the frontmatter, inside a `state:` block:

```yaml
state:
  scene.knowsMira: { type: bool, default: false }
```

(`scene.knowsMira` is a true/false switch, scoped to this scene, starting `false`. State is its
own topic — Part 6 points you to where it's covered properly. For now, just: declare it, then
guard with it.)

Now the branch:

```lute
<branch id="orderChoice">
  <choice id="black" label="Order it black">
    @mira{emotion="content" variant="0"}: Good. No nonsense in a cup.
  </choice>
  <choice id="familiar" label="Say hi like an old friend" when="scene.knowsMira">
    @mira{emotion="surprised" variant="0"}: You remembered. That's new.
  </choice>
</branch>
```

The first choice, `black`, has no `when` — it's always offered. The second, `familiar`, only
shows up once `scene.knowsMira` is true. (A branch always needs at least one unguarded choice —
otherwise the player could be shown an empty menu, which the checker catches for you.)

Check once more:

```
$ ./target/debug/lute check my-scene.lute
ok: my-scene.lute (0 warning(s))
```

## Part 4 — The loop: check → read → fix → compile → trace

This is the day-to-day rhythm of writing Lute. `lute check` is your spellchecker — you'll run it
constantly, and often it catches something small enough that `lute fix` can repair it for you
automatically.

Say you type the old-style sigil out of habit — a colon instead of `@` — on the mono line:

```
:mira{mono}: I should not be this pleased about a coffee order.
```

```
$ ./target/debug/lute check my-scene.lute
my-scene.lute:19:1: error [E-LEGACY-CONTENT-SIGIL] content line sigil `:` was replaced by `@` in 0.2.2 — write `@speaker{…}: text` (dsl §7.1); `lute fix` applies this migration automatically
```

**Reading a diagnostic:** `file:line:col: error [CODE] message`. Here it names the exact line
(19), the exact problem (an old sigil), and exactly what to write instead. For this specific,
mechanical class of fix, you don't have to hand-edit it — run:

```
$ ./target/debug/lute fix my-scene.lute
lute: migrated 1 edit(s) to 0.2.2
```

`lute fix` rewrites the file in place (only what needs to change; nothing else is touched) and
re-check comes back clean:

```
$ ./target/debug/lute check my-scene.lute
ok: my-scene.lute (0 warning(s))
```

Once a file checks clean, `lute compile` turns it into the flat JSON command list the game
engine actually plays — one entry per line/choice/jump, in order:

```
$ ./target/debug/lute compile my-scene.lute
{
  "kind": "scene",
  "lute": "0.6.0",
  "irVersion": "0.6.0",
  "capabilityVersion": "b5187e53c769059a2413754ad831064a0383b51f79a4fbed268f2b484361f29d",
  "meta": {
    "character": "mira",
    "season": 1,
    "episode": 1,
    "episodeId": "s01ep01",
    "title": "A Quiet Table"
  },
  "state": [
    {
      "path": "scene.choices.orderChoice",
      "type": "enum",
      "domain": ["black", "familiar", "unset"],
      "default": "unset",
      "provenance": "branch:orderChoice"
    },
    { "path": "scene.knowsMira", "type": "bool", "default": false }
  ],
  "commands": [
    { "kind": "line", "addr": "001-0100", "role": "narration", "speaker": "narrator",
      "text": "The diner is empty at this hour, and Mira likes it that way.",
      "lineId": "mira.s01ep01.narrator_0010" },
    { "kind": "line", "addr": "001-0200", "role": "dialogue", "speaker": "mira",
      "text": "{{userName}}, you made it.", "emotion": "content", "variant": 0,
      "lineId": "mira.s01ep01.mira_0010", "voiceKey": "mira-0010",
      "placeholders": [{ "kind": "reserved", "token": "userName" }] },
    { "kind": "line", "addr": "001-0300", "role": "monologue", "speaker": "mira",
      "text": "I should not be this pleased about a coffee order.",
      "lineId": "mira.s01ep01.mira_0020" },
    { "kind": "choice", "addr": "001-0400", "branchId": "orderChoice",
      "recordKey": "scene.choices.orderChoice",
      "options": [
        { "id": "black", "label": "Order it black",
          "lineId": "mira.s01ep01.orderChoice.black", "target": "001-0500" },
        { "id": "familiar", "label": "Say hi like an old friend",
          "lineId": "mira.s01ep01.orderChoice.familiar", "when": "scene.knowsMira",
          "expr": { "path": "scene.knowsMira" }, "target": "001-0700" }
      ],
      "converge": "001-0900" },
    { "kind": "line", "addr": "001-0500", "role": "dialogue", "speaker": "mira",
      "text": "Good. No nonsense in a cup.", "emotion": "content", "variant": 0,
      "lineId": "mira.s01ep01.mira_0030", "voiceKey": "mira-0030" },
    { "kind": "jump", "addr": "001-0600", "target": "001-0900" },
    { "kind": "line", "addr": "001-0700", "role": "dialogue", "speaker": "mira",
      "text": "You remembered. That's new.", "emotion": "surprised", "variant": 0,
      "lineId": "mira.s01ep01.mira_0040", "voiceKey": "mira-0040" },
    { "kind": "jump", "addr": "001-0800", "target": "001-0900" }
  ]
}
```

(Shown reformatted for space; the real output is one JSON document, unindented choices included
verbatim.) You will never hand-edit this file — it's the compiled artifact the engine consumes.
Its existence, and that it compiled without error, is proof the scene is **statically valid** —
every construct well-formed, every state path declared, every `<match>` exhaustive — and that
the artifact emitted successfully. It is not proof the scene is playable end to end: that's a
runtime property, verified at integration time (see below).

Finally, `lute trace` lets you **preview a playthrough** without opening the game — you tell it
which choice to take at each branch with `--choose <branchId>=<choiceId>`, and it walks the
scene and prints exactly what would show on screen:

```
$ ./target/debug/lute trace my-scene.lute --choose orderChoice=black
trace: my-scene.lute  (seeds: 0 paths, 0 facts; 1 selection)
  ## The Counter
    @narrator  The diner is empty at this hour, and Mira likes it that way.
    @mira  {{userName}}, you made it.
    @mira  I should not be this pleased about a coffee order.
  <branch orderChoice>   eligible: black   -> black
    @mira  Good. No nonsense in a cup.
trace complete: 1 decision; choices 1/2 (orderChoice)
```

That transcript is a preview of exactly the mock scenario you supplied (the choice you told it
to take, with no other state/facts seeded) — an authoring aid for sanity-checking a branch reads
right, never proof of how the scene behaves at runtime. Genuine runtime behavior — the engine's
actual walk, with its own state and fact resolution — is verified at integration time, not here.

## Part 5 — Sequencing scenes with `after:`

So far you have written one scene in isolation. A real episode is a *sequence* — one
scene is meant to come after the player has seen another, or finished some quest. You
declare that intended ordering with one frontmatter key: **`after:`**.

`after:` **declares the routes that Lute's checker and `lute scenario` analyses assume
reach this scene** — the prerequisites those analyses treat as holding by the time control
arrives here. It doesn't move the player anywhere and it isn't a jump. It's *advisory*: the
tool uses it to verify your episodes fit together into one coherent, analysable graph, but
whether a game engine actually *enforces* this ordering at runtime is engine-dependent —
`after:` is not a Lute-guaranteed lock on when the scene may play.

### What you can write in `after:`

`after:` is deliberately tiny — not arbitrary code. You get exactly two building blocks:

- `visited("<sceneKey>")` — true once the player has seen that scene.
- `completed("<questId>")` — true once that quest is finished. The `<questId>` is the
  `id` on a `<quest>` (the *other* document kind, from Part 6's pointers).

You combine them with `&&` (both) and `||` (either):

```yaml
after: 'visited("mira.s01ep01")'
after: 'visited("mira.s01ep01") && completed("theCoffeeDebt")'
after: 'visited("mira.s01ep01") || visited("mira.s01ep02")'
```

That is the **whole** vocabulary. There is no `!` (you cannot say "*not* visited"), no
arithmetic, and no reading state (`run.*`, `scene.*`) — those are intentionally left out.
If you type `!visited(...)` or a state path in here, the checker rejects it. Ordering is all
`after:` expresses; anything conditional on runtime state stays in your `when=` guards.

### The scene key: how to name a scene in `visited(...)`

`visited("…")` needs a scene's **canonical key**, and this is the part worth memorizing,
because you can write it *without* compiling anything to look it up. The key is simply:

```
{character}.{episodeId}
```

- `{character}` is the `character:` from that scene's frontmatter.
- `{episodeId}` is its `episodeId:` frontmatter key — **or**, when you don't declare one
  (as in this tutorial), it's derived from `season`/`episode` as `s{season}ep{episode}`,
  zero-padded to two digits each.

Our tutorial scene declares `character: mira`, `season: 1`, `episode: 1` and no explicit
`episodeId:`, so its canonical key is **`mira.s01ep01`** — season 1, episode 1. That's the
exact same `mira.s01ep01` you already saw threaded through every `lineId` in the Part 4
compile output (`"lineId": "mira.s01ep01.narrator_0010"`); the key is that prefix. You never
have to reverse-engineer it from compiler output — read it straight off the frontmatter.

### Worked example: a second scene that follows the diner

Let's give Mira a follow-up. A scene that plays *after* the diner should be able to lean on
something the diner established — but remember from Part 3 that `scene.*` state is scoped to
one scene and clears at episode end. To carry a fact across episodes you need the **`run.`**
tier, which persists. So first, teach the diner scene to remember the meeting: declare a
`run.metMira` switch and set it. Add to the diner's frontmatter:

```yaml
state:
  run.metMira: { type: bool }
```

and one line at the end of The Counter:

```
::set{run.metMira = true}
```

(`::set` writes declared state — the flip side of the `when=` reads from Part 3.)

Now the follow-up. Because `after:` and cross-scene reads only make sense across *several*
files, put both scenes in a folder — an **episodes project**. Alongside them drop a one-line
`lute.project.yaml` so the tool knows the folder is a project root:

```
episodes/
  lute.project.yaml
  diner.lute        ← the scene from Parts 1–4 (now with run.metMira)
  booth.lute        ← the new follow-up, below
```

```yaml
# episodes/lute.project.yaml
defaultProfile: core
profiles:
  core:
    plugins: {}
```

```lute
---
kind: scene
title: The Usual Booth
character: mira
season: 1
episode: 2
pov: fixer
after: 'visited("mira.s01ep01")'
state:
  run.metMira: { type: bool }
---

## The Counter

@mira{emotion="content" variant="0" when="run.metMira"}: Back again. You know where you sit.

@narrator: The coffee is already poured.
```

`booth.lute` is `mira.s01ep02`. It declares `after: 'visited("mira.s01ep01")'` — it becomes
reachable only once the diner has been seen — and its guarded line reads `run.metMira`, the
switch the diner sets.

Single-file `lute check` can't judge any of this — a `.lute` file on its own has no idea what
other episodes exist. The **project** checker can. Point it at the folder:

```
$ ./target/debug/lute check-project episodes
ok: episodes/booth.lute (0 warning(s))
ok: episodes/diner.lute (0 warning(s))
ok: episodes (2 file(s), 0 project-wide warning(s))
```

Clean — the key resolved, the ordering is acyclic, and the cross-scene read is safe. Now
inspect the graph you just described. `lute scenario` is the read-only design surface for
everything `after:` implies. Bare, it prints the whole graph in play order:

```
$ ./target/debug/lute scenario episodes
project root: episodes
  topological layers:
    layer 0: scene(mira.s01ep01)
    layer 1: scene(mira.s01ep02)
  edges (prerequisite -> dependent):
    scene(mira.s01ep01) -> scene(mira.s01ep02)
```

`reach <key>` answers "can the player ever get here, and by what route?":

```
$ ./target/debug/lute scenario episodes reach mira.s01ep02
project root: episodes
reach scene(mira.s01ep02):
  verdict: Reachable — a satisfiable route exists under your declared routes.
  after: visited("mira.s01ep01")
  referenced node(s) (see `after` above for the && / || structure — this is NOT a flat requirement list):
    - scene(mira.s01ep01): Reachable — a satisfiable route exists under your declared routes.
```

And `envelope <key>` answers the question you most want before writing a `when=` guard:
*what state is safe to read here?* — i.e. what every route into this scene guarantees is
already set:

```
$ ./target/debug/lute scenario episodes envelope mira.s01ep02
project root: episodes
envelope for scene(mira.s01ep02) (pre-entry — state available when control REACHES this node, before its own writes):
  Guaranteed (safe to read under your declared routes):
    - run.metMira
  Possible (set on at least one declared route reaching this node):
    - run.metMira
  Possible \ Guaranteed -- warning-grade reads (set on SOME but not every declared route; suppressed by default in `check-project`, dsl §6, surfaced here per §5):
    (none)
```

`run.metMira` is **Guaranteed** *because of the route, not a default*: it's declared with no
`default:`, so it starts unset — the only thing that makes it safe to read here is that every
declared route into the booth passes through the diner, which always `::set`s it. That's a
genuine cross-scene guarantee: the diner's write propagates forward into the booth's pre-entry
envelope, so the booth's `when="run.metMira"` read is provably safe. (These verdicts describe
your *declared* `after` routes; they're what holds if play follows the graph you wrote.)

### What the project checker catches for you

These are the connectivity diagnostics `check-project` adds on top of single-file `check`
(which structurally can't see across files) — the ones you'll meet most, not an exhaustive
list. Each is a helpful catch, not a nuisance:

- **`E-CONN-UNKNOWN-NODE`** — a `visited("…")`/`completed("…")` key that no scene/quest in the
  project matches (usually a typo). It suggests the nearest real key: *"did you mean
  `mira.s01ep01`?"*.
- **`E-CONN-CYCLE`** — an unsatisfiable ordering among the scenes/quests in the offending chain
  (e.g. A `after` B `after` A, or quests waiting on each other): nothing in that chain can be
  sequenced. It prints the offending chain, and the bare `lute scenario` graph still isolates
  exactly which nodes form the cycle. Scenes on or downstream of the cycle can't have their
  `reach`/`envelope` computed (their ordering is unresolvable), and `envelope` reports the cycle
  instead of a silent empty table — but scenes *independent* of the cycle still get their real
  `reach`/`envelope`, so one tangled corner doesn't blind the rest of the project.
- **`E-CONN-UNREACHABLE`** — a scene no declared route ever reaches; it can't be played as
  authored.
- **`E-CONN-FORMULA-TOO-COMPLEX`** — an `after` formula with too many terms for the checker to
  analyse (past its complexity cap); usually a sign the clause was machine-generated rather
  than hand-written. Simplify it into something a reader — and the checker — can follow.
- **`E-CONN-EPISODE-ID-DUP`** — two scenes computing the *same* canonical key, so `visited("…")`
  would be ambiguous. Give one a distinct `episode`/`episodeId`.
- **`E-STATE-MAYBE-UNAVAILABLE`** — a `when=`/read of a `run.*`/`user.*` path that NO declared
  route into this scene sets at all — the very thing the `envelope` above screens for. (A path
  set on *some* but not all routes is only a suppressed warning, not this error.)

Reach for `lute scenario` whenever you want to *see* the shape of your story — the layers, the
routes, the guaranteed state at each node — instead of guessing.

## Part 6 — Where to go next

**Not sure what's legal to write?** `lute context <file>` prints exactly the vocabulary your
project accepts — the staging directives, their attributes, the enum values (like `emotion`),
the declared state, and the delivery-flag vocabulary — resolved for the specific file you give
it:

```
$ ./target/debug/lute context my-scene.lute
capabilityVersion: b5187e53c769059a2413754ad831064a0383b51f79a4fbed268f2b484361f29d
directives (8):
  auto: character, anchor, action
  bg: location, time, assetId
  camera: focus, zoom, move-x, move-y, shake, reset, duration, easing, delay, wait
  cut: assetId, action, full
  music: action, mood, volume, assetId, track
  sfx: sound, assetId, name
  vfx: type, label, transition
  video: assetId, action, wait
enums (6):
  anchor: left, center, right
  emotion: neutral, surprised, delighted, shy, content, angry, sad
  mood: peaceful, tense, romantic, sad, upbeat
  musicAction: start, change, stop, resume, fade-out
  vfxType: whiteOut, blackOut, rain, snow, leaves, petals, raindrop
  volume: silent, down, normal, up, full
stateSchema (2):
  scene.choices.orderChoice: enum [black, familiar, unset]
  scene.knowsMira: bool
deliveryFlags (3):
  {mono}: interior monologue / thought (not spoken aloud in-scene)
  {os}: off-screen: the speaker is heard but not currently staged/visible
  {vo}: voiceover: narration-style delivery layered over the scene
```

`deliveryFlags` is always present — a fixed, project-independent list — so it's the one place you
can always confirm what `{mono}`/`{os}`/`{vo}` mean without hunting through this tutorial. If
the scene reads a quest's reserved `quest.<id>.state` or `quest.<id>.objectives.<oid>.done`
path, `context` also lists exactly those referenced paths under a `reservedQuestPaths` section
(omitted here since this scene reads no quest state — see `docs/proposals/scenario-dsl/0.2.0.md`
for quests).

Run it any time you need to double check a directive name, an attribute, or a legal `emotion`
value instead of guessing.

From here, three worked full-length episodes to read (real, checkable `.lute` files, not
snippets):

- [`docs/examples/bianca-s01ep02.lute`](examples/bianca-s01ep02.lute) — a complete linear
  episode with staging (`::bg`, `::music`, `::camera`), a multi-track `<timeline>` beat, a
  `<branch>` with an `into=` run-record choice, and a `<match>` reacting to the choice afterward.
- [`docs/examples/showcase/episode01.lute`](examples/showcase/episode01.lute) and its
  [README](examples/showcase/README.md) — a feature-by-feature tour with a line-number index.
- [`docs/examples/quest-grove.lute`](examples/quest-grove.lute) — an example of the *other*
  document kind, `quest`.

And when you're ready to go deeper than this tutorial:

- **Quests, objectives, and long-running state** ("did the player already do this, three scenes
  ago?") — `docs/proposals/scenario-dsl/0.2.0.md`.
- **Facts about the world** (relationships, membership, "is this NPC in the party?") —
  `docs/proposals/scenario-dsl/0.3.0.md`.
- **`lute trace` in full** (seeding state, mocking facts, firing quest events) —
  `docs/proposals/scenario-dsl/0.4.0.md` §4, or `lute trace --help`.
- Everything else about the language, precisely — `docs/proposals/scenario-dsl/0.1.0.md` (the
  base spec) plus the version deltas layered on top of it. Use it as a reference, not required
  reading: look up the one construct you need, when you need it.
