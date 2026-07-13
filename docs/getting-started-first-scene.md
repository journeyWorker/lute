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
my-scene.lute:10:1: error [E-CONTENT-OUTSIDE-SHOT] content lives inside a shot/scene; add a `## Shot N.` heading above it (dsl 0.1 §6)
```

This is the rule to remember: **all content lives under a heading.** A Lute document is a
sequence of "shots" — beats of the scene — and every line of dialogue, narration, or staging has
to sit inside one. Add a heading before the line:

```
## Shot 1.

@narrator: The diner is empty at this hour, and Mira likes it that way.
```

(`## Shot 1.`, `## Shot 2.`, … — or `## Scene 1.` if you prefer that word. The number and the
trailing period are both required.)

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
thought, not speech, and works for any character, not just the player.

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

## Shot 1.

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
own topic — Part 5 points you to where it's covered properly. For now, just: declare it, then
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
  "lute": "0.5.0",
  "irVersion": "0.5.0",
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
  ## Shot 1.
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

## Part 5 — Where to go next

**Not sure what's legal to write?** `lute context <file>` prints exactly the vocabulary your
project accepts — the staging directives, their attributes, the enum values (like `emotion`),
and the declared state — resolved for the specific file you give it:

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
```

Run it any time you need to double check a directive name, an attribute, or a legal `emotion`
value instead of guessing.

From here, three worked full-length episodes to read (real, checkable `.lute` files, not
snippets):

- [`docs/examples/bianca-s01ep02.lute`](examples/bianca-s01ep02.lute) — a complete linear
  episode with staging (`::bg`, `::music`, `::camera`), a multi-track `<timeline>` beat, a
  `<branch>` with `persist`, and a `<match>` reacting to the choice afterward.
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
