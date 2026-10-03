---
title: Write your first scene
description: Build one small, real Lute scene from an empty file step by step, running the lute tool at every step to see exactly what it reports, then pin it with a test.
---

This is the "start here" for a scenario writer who has never touched Lute — no compiler background
required. It builds **one small real scene** from an empty file, step by step, running the actual
`lute` tool at every step so you can see exactly what it says. It targets language version
**0.32.0**.

You need a plain-text editor, a terminal, and the `lute` command
([install it first](/getting-started/installation/)). Everything you write here is **core Lute
only** — no plugins, no project configuration. Just the language itself.

**If your editor and the terminal disagree** — red underlines in the editor on a file `lute check`
calls `ok`, or the other way round — trust the terminal and run `lute doctor .`. It names the usual
cause: an editor language server older than your `lute` (restart the editor after upgrading).

## Part 1 — The minimal skeleton

Create an empty file, `my-scene.lute`, and run the checker on it — the checker tells you whether a
`.lute` file is valid:

<!-- lute-diagnostics -->
```
$ lute check my-scene.lute
my-scene.lute:1:1: error [E-KIND-MISSING] required frontmatter key `kind` is missing; every root document must declare `kind: scene`, `kind: quest`, or `kind: lore`
my-scene.lute:1:1: error [E-META-MISSING] a scene needs an `id:`, its key in the project — write `id: opening` in the frontmatter
failed: my-scene.lute (2 error(s), 0 warning(s))
```

That's the whole idea of `lute check`: it reads your file and tells you, line by line, exactly
what is wrong and why — never a silent failure. Every `.lute` file starts with a YAML
**frontmatter block** (between two `---` lines) that answers "what is this document, and what is
it called?". Add one:

```yaml
---
kind: scene
id: mira.s01ep01
title: A Quiet Table
pov: fixer
---
```

- `kind: scene` — this file is a scene: dialogue the player sees. The other two kinds are `quest`
  (goals the story tracks) and `lore` (text the game looks up, such as item descriptions).
- `id` — the scene's name, unique in your project. Other scenes and your tests refer to this scene
  by it. It is names joined by `.` (a name is letters, digits, `_` or `-`, not starting with
  `-`: `door-notes` and `doorNotes` both work); `mira.s01ep01` reads as "Mira, season 1,
  episode 1", but any name works (`prologue`, `diner.opening`).
- `title` — a human-readable title for tools and search.
- `pov` — the id of the player character (the protagonist the player controls).

The `E-META-MISSING` error above is the `id:` line: its `opening` is only an example name, and
`mira.s01ep01` is this scene's.

Save that and re-check:

```
$ lute check my-scene.lute
ok: my-scene.lute (0 warning(s))
```

### Quotes and YAML for writers

The frontmatter is **YAML**, and so are the test and play files later on. Five rules cover
everything this guide writes:

- **`key: value`**, with a space after the colon. Indent with **spaces, never tabs**; a nested
  entry is indented under its parent.
- **A list** is `[a, b, c]` on one line, or one `- item` per line. **A map** is `{ key: value }` on
  one line, or indented `key: value` lines.
- **Quote a value that starts with a symbol** (`!`, `[`, `{`, `'`, `"`, `*`, `&`) or that contains
  `: ` or ` #`. Plain words and numbers need no quotes.
- **Quotes nest by alternating.** Inside double quotes, write single quotes; inside single quotes,
  write double quotes:

  ```yaml
  after: 'visited("mira.s01ep01")'    # YAML's single quotes around Lute's "…"
  when: "run.accused == 'ruben'"      # YAML's double quotes around Lute's '…'
  when: "!visited('accusation')"      # starts with ! — must be quoted
  ```

  Writing `when: "run.accused == "ruben""` ends the string at the second `"`: `lute check`
  reports `E-META-PARSE` on that line and suggests single quotes inside. Switch the inner pair.
- **Straight quotes only.** Word processors and note apps turn `"` into curly `“ ”`. Lute reads only
  the straight `"` and `'`, in the frontmatter and in tag attributes such as `label="…"` alike; a
  curly quote in a tag attribute is `E-ATTR-QUOTE`, which says to retype it.

Inside a `.lute` body, tag attributes are always double-quoted, so a condition in one uses single
quotes inside: `when="run.accused == 'ruben'"`.

### Content lives under a heading

The file has no content yet. Try adding a line of narration directly under the frontmatter:

```lute
@narrator: The diner is empty at this hour, and Mira likes it that way.
```

Check again:

<!-- lute-diagnostics -->
```
$ lute check my-scene.lute
my-scene.lute:8:1: error [E-CONTENT-OUTSIDE-SHOT] content lives inside a shot; add a `## <title>` heading above it
failed: my-scene.lute (1 error(s), 0 warning(s))
```

The rule to remember: **all content lives under a heading.** A Lute document is a sequence of
"shots" — beats of the scene — and every line of dialogue, narration, or staging sits inside one.
Add a heading before the line:

```lute
## The Counter

@narrator: The diner is empty at this hour, and Mira likes it that way.
```

(The heading is free text after `## ` — `## The Counter`, `## Scene 1. The diner`, `## Prologue` are all
valid. `The Counter`, `The Regular`, … stays a fine convention, but the number is not grammar: shots are
numbered by their document order.)

```
$ lute check my-scene.lute
ok: my-scene.lute (0 warning(s))
```

That's the whole skeleton: frontmatter, a heading, one line under it.

## Part 2 — Speaking, narrating, feeling

A content line always has the same shape: `@who{attributes}: what they say`. Narration uses the
reserved speaker `@narrator`. Add a line where Mira speaks:

```lute
@mira{emotion="content" variant="0"}: {{userName}}, you made it.
```

- `@mira` is the speaker. `emotion="content"` and `variant="0"` pick which portrait/pose to show.
- `{{userName}}` is an **interpolation** — text wrapped in double braces gets filled in at runtime.
  `{{userName}}` is the one that's always available: the player's own name.

Save and check. This one does **not** pass yet:

<!-- lute-diagnostics -->
```
$ lute check my-scene.lute
my-scene.lute:12:16: error [E-DOMAIN-UNKNOWN] `emotion` is not a declared domain — declare its members in an `enums:` block in this document's own frontmatter, in a project schema reached through `uses:`, or in a plugin's `enums` export before using `emotion`
failed: my-scene.lute (1 error(s), 0 warning(s))
```

Nothing is misspelled — this is the rule that **Lute ships the slot, you ship the members.**
`emotion` is one of seven vocabulary slots the language knows about (`emotion`, `action`, `anchor`,
`mood`, `volume`, `musicAction`, `vfxType`), but the compiler holds no opinion about which emotions
your characters have. That is your story's call, so no value is legal until you say it is. Declare
the members where you declare everything else about the document — the frontmatter:

```yaml
enums:
  emotion: [neutral, surprised, delighted, shy, content, angry, sad]
```

Declare only the slots your scene actually uses; this one uses `emotion` and nothing else. (Two
slots carry required semantics once you declare them: `action` needs an `exits:` list naming the
members that take a character off stage, and `anchor` needs a `default:`. `lute init` scaffolds all
seven with a starter member list, in a shared `vocabulary.schema.yaml` that scenes pull in with
`uses:` — the right shape once several files share one vocabulary. For a single tutorial file,
frontmatter is simpler.)

Re-check:

```
$ lute check my-scene.lute
ok: my-scene.lute (0 warning(s))
```

Now add an inner-voice line for Mira — her private thought, not spoken aloud:

```lute
@mira{mono}: I should not be this pleased about a coffee order.
```

`{mono}` is a **delivery flag**: a bare word in the braces (no `=value`) that changes how the line
is delivered. `{mono}` means interior monologue — it renders as thought, not speech, and works for
any character. Two other delivery flags exist: `{os}` marks a line as **off-screen** (the speaker
is heard but not staged), and `{vo}` marks it as **voiceover** (narration-style delivery layered
over the scene). All three are mutually exclusive — at most one per line — and none is allowed on
`@narrator`.

The file so far:

```lute check
---
kind: scene
id: mira.s01ep01
title: A Quiet Table
pov: fixer
enums:
  emotion: [neutral, surprised, delighted, shy, content, angry, sad]
---

## The Counter

@narrator: The diner is empty at this hour, and Mira likes it that way.

@mira{emotion="content" variant="0"}: {{userName}}, you made it.

@mira{mono}: I should not be this pleased about a coffee order.
```

## Part 3 — Giving the player a choice

A `<branch>` presents the player with a menu; each `<choice>` inside it is one option, with its own
`id`, a `label` (the button text), and the lines that play if the player picks it.

Sometimes a choice should only appear under certain conditions — say, only if the player has met
Mira before. That's a **guard**: `when="<condition>"`. Guards read declared **state** — a small
named value the engine tracks — so first declare one in the frontmatter, inside a `state:` block:

```yaml
state:
  scene.knowsMira: { type: bool, default: false }
```

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

The first choice, `black`, has no `when` — it's always offered. The second, `familiar`, only shows
up once `scene.knowsMira` is true. A branch always needs at least one unguarded choice — otherwise
the player could be shown an empty menu, which the checker catches for you.

## Part 4 — The loop: check → read → fix → compile → trace

This is the day-to-day rhythm of writing Lute. `lute check` is your spellchecker — you'll run it
constantly, and often it catches something small enough that `lute fix` can repair it for you
automatically.

Say you type an old-style sigil out of habit — a colon instead of `@` — on the mono line:

```
:mira{mono}: I should not be this pleased about a coffee order.
```

<!-- lute-diagnostics -->
```
$ lute check my-scene.lute
my-scene.lute:18:1: error [E-LEGACY-CONTENT-SIGIL] content line sigil `:` was replaced by `@` in 0.2.2 — write `@speaker{…}: text`; `lute fix` applies this migration automatically
failed: my-scene.lute (1 error(s), 0 warning(s))
```

**Reading a diagnostic:** `file:line:col: error [CODE] message`. It names the exact line, the exact
problem, and exactly what to write instead. When a message is not enough, `lute --explain <CODE>`
prints what the code means and links its entry in the [diagnostics reference](/reference/diagnostics/),
which lists every code:

```
$ lute --explain E-LEGACY-CONTENT-SIGIL
E-LEGACY-CONTENT-SIGIL (error)

A content line uses the old `:` speaker sigil, which `@` replaced — write `@speaker{…}: text` instead.

Spec: dsl §7.1
More: https://lute-lang.vercel.app/reference/diagnostics/#e-legacy-content-sigil
```

For this mechanical class of fix, run:

```
$ lute fix my-scene.lute
lute: applied 1 fix(es)
```

`lute fix` rewrites the file in place (only what needs to change) and re-check comes back clean.

Once a file checks clean, `lute compile` turns it into the flat JSON command list the game engine
plays — one entry per line, choice, and jump, in order:

```
$ lute compile my-scene.lute
{
  "kind": "scene",
  "lute": "0.32.0",
  "irVersion": "0.32.0",
  "capabilityVersion": "f78bb8efcaab8c3ea4ccf1bbee976a80596a04b1aca59fbe74123abfa1f55225",
  "meta": {
    "id": "mira.s01ep01",
    "title": "A Quiet Table"
  },
  "state": [ … ],
  "enums": [ … ],
  "commands": [
    {
      "kind": "line",
      "addr": "001-0100",
      "role": "narration",
      "speaker": "narrator",
      "text": "The diner is empty at this hour, and Mira likes it that way.",
      "lineId": "mira.s01ep01.narrator_0010"
    },
    {
      "kind": "line",
      "addr": "001-0200",
      "role": "dialogue",
      "speaker": "mira",
      "text": "{{userName}}, you made it.",
      "emotion": "content",
      "variant": 0,
      "lineId": "mira.s01ep01.mira_0010",
      "voiceKey": "mira.s01ep01.mira-0010",
      "placeholders": [ … ]
    },
    …
  ],
  "shots": [
    {
      "shot": 1,
      "heading": "The Counter"
    }
  ]
}
```

(`…` marks where output was trimmed for space; everything else is verbatim.) Your `enums:`
declaration rides along into the artifact's own **`enums`** block, so the engine resolves values
against exactly the vocabulary the checker used. Your `id:` is the prefix of every `lineId` and
`voiceKey`, so each line of this scene has a name no other scene's line can take. Two other fields
are worth knowing on sight.
**`addr`** is the record's address, `{shot}-{index}`, and every `addr` in one
artifact is padded to the same width — so sorting the `addr` strings gives you execution order, no
parsing required. **`shots`** carries your `## ` headings through to the artifact, so a tool
downstream can still say *which beat* a record belongs to.

You never hand-edit this file — it's the compiled artifact the engine consumes. That it compiled
without error is proof the scene is **statically valid**: every construct well-formed, every state
path declared, every `<match>` exhaustive. It is not proof the scene plays the way you meant —
that is what `lute trace` and, in Part 6, `lute test` are for.

`lute trace` previews a playthrough without opening the game — you tell it which choice to
take at each branch with `--choose <branchId>=<choiceId>`, and it walks the scene and prints what
would show on screen:

```
$ lute trace my-scene.lute --choose orderChoice=black
trace: my-scene.lute  (seeds: 0 paths, 0 facts; 1 selection)
  ## The Counter
    @narrator  The diner is empty at this hour, and Mira likes it that way.
    @mira{emotion="content" variant="0"}  {{userName}}, you made it.
    @mira{mono}  I should not be this pleased about a coffee order.
  <branch orderChoice>   eligible: black   -> black
    @mira{emotion="content" variant="0"}  Good. No nonsense in a cup.
trace complete: 1 decision; choices 1/2 (orderChoice)
```

That transcript previews exactly the choices you supplied — a quick way to read a branch the way a
player would. A line keeps its attributes in braces, so `@mira{mono}` shows at a glance which line
is a thought.

## Part 5 — Sequencing scenes with `after:`

A real episode is a *sequence* — one scene is meant to come after the player has seen another. You
declare that intended ordering with one frontmatter key: **`after:`**.

`after:` declares the routes Lute's checker and `lute scenario` analyses assume reach this scene. It
doesn't move the player anywhere and it isn't a jump: it says "this scene comes after that one",
and the tool uses it to verify your episodes fit together into one coherent, analysable graph.
What actually *starts* each scene is the game engine — or, before there is one, `lute play`, which
[Connect scenes into a story](/getting-started/connect-scenes/) sets up on the next page.

`after:` is deliberately tiny. You get exactly three building blocks:

- `visited("<id>")` — true once the player has seen the scene with that `id:`.
- `completed("<questId>")` — true once that quest is finished.
- `active("<questId>")` — true while that quest is running: started, not yet finished.

`completed` and `active` are not opposites. `completed` is a permanent fact about the past;
`active` is a window that opens and closes. Reach for `active` when a scene only makes sense
*during* a quest.

Combine them with `&&` (both) and `||` (either):

```yaml
after: 'visited("mira.s01ep01")'
after: 'visited("mira.s01ep01") && completed("theCoffeeDebt")'
after: 'visited("mira.s01ep01") && active("theCoffeeDebt")'
after: 'visited("mira.s01ep01") || visited("mira.s01ep03")'
```

(The outer single quotes are YAML's; the inner double quotes are the scene id's. See
[Quotes and YAML for writers](#quotes-and-yaml-for-writers).) That is the whole vocabulary. There is
no `!`, no arithmetic, and no reading state — those are intentionally left out. Anything conditional
on runtime state stays in your `when=` guards.

`visited("mira.s01ep01")` names the diner by the `id:` you gave it in Part 1. That is the whole
reason every scene carries an `id:`.

(**Older scenes.** Before `id:` existed, a scene was named by three keys, `character:`,
`season:` and `episode:`, and its name was built from them: `character: mira`, `season: 1`,
`episode: 1` named `mira.s01ep01`. Scenes written that way still check. Write `id:` in new
scenes; a scene that has `id:` **and** any of those three keys draws one `W-META-LEGACY` warning
per key, and anything descriptive belongs under `extra:`.)

To carry a fact across episodes, use the persistent **`run.`** tier. Teach the diner to remember
the meeting — declare `run.metMira` and set it at the end of the scene:

```yaml
state:
  run.metMira: { type: bool }
```

```lute
::set{run.metMira = true}
```

`after:` and cross-scene reads only make sense across several files, so put both scenes in a folder
with a `lute.project.yaml` marking it a project root:

```
episodes/
  lute.project.yaml
  diner.lute        ← the scene from Parts 1–4, plus run.metMira and the ::set
  booth.lute        ← the new follow-up, below
```

Every voice line is already keyed by its scene — the default `voiceKey` is
`{prefix}.{speaker}-{code}` — so the diner's and the booth's first Mira lines are
`mira.s01ep01.mira-0010` and `mira.s01ep02.mira-0010`, two recordings, with nothing to configure:

```yaml
# episodes/lute.project.yaml
defaultProfile: core
profiles:
  core:
    plugins: {}
```

(Before 0.22.0 the default was the unprefixed `{speaker}-{code}`, so both lines were `mira-0010` —
one recording for two lines, which `check-project` refuses as `E-DUP-VOICEKEY`. A project that
already recorded audio against those old keys keeps them by pinning
`identity: { voiceKey: "{speaker}-{code}" }` in `lute.project.yaml`.)

```lute check-project="docs/examples/episodes/booth.lute"
---
kind: scene
id: mira.s01ep02
title: The Usual Booth
pov: fixer
after: 'visited("mira.s01ep01")'
enums:
  emotion: [neutral, surprised, delighted, shy, content, angry, sad]
state:
  run.metMira: { type: bool }
---

## The Counter

@mira{emotion="content" variant="0" when="run.metMira"}: Back again. You know where you sit.

@narrator: The coffee is already poured.
```

Vocabulary is declared per document, so the booth repeats the diner's `enums:` block. Two files is
the point where you stop copying it: move the block into a `vocabulary.schema.yaml` next to them and
replace it in each scene with `uses: [./vocabulary.schema.yaml]`. That is exactly the layout
`lute init` scaffolds.

Single-file `lute check` can't judge cross-file relationships — a `.lute` file on its own has no
idea what other episodes exist. The **project** checker can:

```
$ lute check-project episodes
ok: episodes/booth.lute (0 warning(s))
ok: episodes/diner.lute (0 warning(s))
ok: episodes (2 file(s), 0 project-wide warning(s))
```

Now inspect the graph. `lute scenario` is the read-only design surface for everything `after:`
implies. Bare, it prints the whole graph in play order:

```
$ lute scenario episodes
project root: episodes
  topological layers:
    layer 0: scene(mira.s01ep01)
    layer 1: scene(mira.s01ep02)
  edges (prerequisite -> dependent) [atom kind(s)]:
    scene(mira.s01ep01) -> scene(mira.s01ep02) [visited]
```

`reach <id>` answers "can the player ever get here, and by what route?"; `envelope <id>` answers
the question you most want before writing a `when=` guard — *what state is safe to read here?*:

```
$ lute scenario episodes envelope mira.s01ep02
project root: episodes
envelope for scene(mira.s01ep02) (pre-entry — state available when control REACHES this node, before its own writes):
  Guaranteed (safe to read under your declared routes):
    - run.metMira   written by: scene(mira.s01ep01)
  Possible (set on SOME but not every declared route reaching this node; the Guaranteed paths above are not repeated):
    (none)
  Guaranteed facts (hold on every declared route reaching this node):
    (none)
  Possible \ Guaranteed -- warning-grade reads (set on SOME but not every declared route; suppressed by default in `check-project`, surfaced here):
    (none)
```

`run.metMira` is **Guaranteed** because of the route: every declared route into the booth passes
through the diner, which always `::set`s it. That's a genuine cross-scene guarantee — the booth's
`when="run.metMira"` read is provably safe.

## Part 6 — Pin your story with tests

A trace shows you one path once. A **test** writes down what must be true and checks it every time
you run `lute test`, so a later edit that breaks the scene fails loudly instead of slipping by.

Tests live in a `tests/` folder in the project, one `*.test.yaml` file each. Create
`episodes/tests/diner.test.yaml`:

```yaml
file: ../diner.lute
choose: { orderChoice: black }
expect:
  transcriptContains: ["@mira: Good. No nonsense in a cup."]
  transcriptLacks: ["@mira: You remembered. That's new."]
  state: { run.metMira: true }
```

- `file:` is the scene under test, **relative to the test file**: the test sits in `tests/`, so
  the diner is `../diner.lute`. (In a project with a `scenes/` folder it would be
  `../scenes/<name>.lute`.)
- `choose:` picks at each branch, exactly like `--choose` on `lute trace`.
- `expect:` lists what must hold: `transcriptContains` lines that must be shown, written
  `@speaker: text`; `transcriptLacks` lines that must not; `state` the values after the scene.

```
$ lute test episodes --project episodes
PASS  episodes/tests/diner.test.yaml  (episodes/tests/../diner.lute)

1 passed, 0 failed
```

A failing test says why. Change the choice to `familiar`, which is guarded by `scene.knowsMira`:

<!-- lute-diagnostics unverified="lute test respells the walk.rs literal `--choose {id}={choice}` as the test key `choose: {id}={choice}` and composes the reason, so no single format! literal matches; the block is byte-exact binary output" -->
```
$ lute test episodes --project episodes
FAIL  episodes/tests/diner.test.yaml  (episodes/tests/../diner.lute)
      trace refused:
        episodes/tests/../diner.lute:25:3: error [E-TRACE-CHOICE] `choose: orderChoice=familiar` is ineligible at its presentation point: its guard `scene.knowsMira` decided false: `scene.knowsMira` is false (mock `state: { scene.knowsMira: <value> }`)

0 passed, 1 failed
```

The choice's guard is false because nothing set `scene.knowsMira`. A test can start from any state
you name: add `state: { scene.knowsMira: true }` above `choose:` and the `familiar` path is testable
too. `lute test episodes --project episodes --coverage` then lists which choices no test has taken
yet, and which scenes no test names — a to-do list for your next tests.

Every key a test file takes is in the [CLI reference](/tooling/cli/#test).

## Part 7 — Where to go next

**Play the whole story.** Tests check one scene at a time; to play every scene in order, from the
first to an ending, go to [Connect scenes into a story](/getting-started/connect-scenes/). It adds
one line to each scene and a play script, and `lute play` walks the story the way a player would.

**Not sure what's legal to write?** `lute context <file>` prints exactly the vocabulary your
project accepts — the staging directives, their attributes, the vocabulary members in scope (your
`emotion` list, say), the declared state, the delivery-flag vocabulary, the language's own
built-in directives, and the scene ids you can name in `visited(…)` — resolved for the specific
file you give it:

```
$ lute context episodes/diner.lute
lute: note: using project episodes (nearest lute.project.yaml); pass --project to choose another
capabilityVersion: f78bb8efcaab8c3ea4ccf1bbee976a80596a04b1aca59fbe74123abfa1f55225
permissions: unrestricted (authoring/compile-time restrictions; not runtime sandbox enforcement)
directives (12):
  auto: character: string, anchor: domain:anchor, action: domain:action   [reads.onStage usesAnchor mayExitCharacter writes.characterState]
  bg: location: string, time: string, assetId: string   [mutatesScene]
  camera: focus: string, zoom: double, moveX: double, moveY: double, shake: double, reset: bool, duration: double, easing: string, delay: double, wait: bool
  clear:    [reads.onStage mayExitCharacter]
  cut: assetId: string, action: enum[show, hide], full: bool
  end: reason: string   [terminatesWalk]
  mark: id: string (required)
  music: action: domain:musicAction, mood: domain:mood, volume: domain:volume, assetId: string, track: string   [mutatesScene]
  next: to: string (required), when: string
  sfx: sound: string, assetId: string, name: string
  vfx: type: domain:vfxType, label: string, transition: string
  video: assetId: string, action: enum[show, hide], wait: bool
bridges (0):
rewardKinds (0):
occasions (0):
questsAllowed: true
enums (0):
stateSchema (4):
  prev.run.metMira: bool (owner: engine)
  run.metMira: bool
  scene.choices.orderChoice: enum [black, familiar, unset] (owner: engine)
  scene.knowsMira: bool
deliveryFlags (3):
  {mono}: interior monologue / thought (not spoken aloud in-scene)
  {os}: off-screen: the speaker is heard but not currently staged/visible
  {vo}: voiceover: narration-style delivery layered over the scene
projectEnums (1):
  emotion: neutral, surprised, delighted, shy, content, angry, sad
builtinDirectives (10):
  ::set{ <path> = <expr> [when="<condition>"] }  (also += / -=) — write a declared state path; engine-owned paths are the engine's (E-ENGINE-OWNED-WRITE)
  ::assert{ <relation>(<arg>, …) [when="<condition>"] } — assert a ground fact of a declared, non-derived, non-reserved relation
  ::retract{ <relation>(<arg | _>, …) [when="<condition>"] } — retract the matching facts of a declared, non-derived, non-reserved relation
  ::accept{quest="<questId>" [at="nextRun"] [when="<condition>"]} — accept a quest that has no `start` condition; `at="nextRun"` queues it until after the next new run
  ::use{component="<name>" <param>=<value> … [when="<condition>"]} — expand an imported component with named arguments; a param with a default may be omitted
  ::body — in a component with a `beat:` header, at the top level of its body: where a `<beat use=…>`'s own body goes
  ::next{to="<string>" [when="<condition>"]} — jump forward to the `::mark` named by `to` (only while `when` holds)
  ::mark{id="<string>" [when="<condition>"]} — name the position a `::next{to=…}` jumps to
  ::end{[reason="<string>"] [when="<condition>"]} — end this presentation here
  ::clear{[when="<condition>"]} — take every character on stage off it; takes no attributes
directiveAttrs (5; beyond each directive's own):
  when: condition — every directive
  duration: double — every directive but ::clear
  delay: double — every directive but ::clear
  wait: bool — every directive but ::clear
  at: time — a directive inside a <track> clip only
beatKeys (11; scene frontmatter `key: value`; <entry> / <beat> attributes `key="value"`):
  on: <occasion> — the occasion the beat answers
  target: <prefix>.<member> | kind:<kind> — the one target it answers, or every member of a kind (read as occasion.target)
  for: kind:<kind> — on an untargeted `select: sequence` occasion: presented once per member whose `when` holds, binding occasion.target
  when: <condition> — eligible only while it holds
  priority: <integer> — the higher eligible beat wins
  once: run | user | false | day | slot | week | season:<name> — presented at most once per run, ever, without limit, per clock day / slot / week, or per window of a season
  spentBy: <condition> — spent once the condition has held; `once` sets how long it stays spent (`run` unless written)
  also: true — scene and bundle beats, on a `select: first` occasion: presented after the winner too
  share: <key> — beats with one `share` key spend one `once` together
  after: <prerequisite> — scene and bundle beats: eligible once it holds, e.g. visited("<id>")
  use: <component> — bundle `<beat>`: its header from the component's `beat:` template, the component's params as attributes
questKeys (10; <quest> attributes):
  id="<questId>" — read as quest.<id>.state
  title="<text>" — the quest's name
  start="<condition>" — activates the quest when it holds; without it the quest is accept-driven
  fail="<condition>" — fails the active quest when it holds
  follows="<prerequisite>" — the quest's place in the scene graph; never gates activation (to wait, write start="visited('…')")
  tier="user | run | season:<name>" — when it returns to unset: never, at each new run, or each time the season opens
  activate="accept" — a child that waits for an ::accept instead of activating with its parent
  complete="all | any" — completes when every / any one required objective is done
  accept="external" — the engine accepts the quest outside any document
  rearm="<condition>" — returns the quest to unset (objectives cleared) each time the condition goes false→true
objectiveKeys (10; <objective> attributes):
  id="<objectiveId>" — read as quest.<quest>.objectives.<id>.done / .failed
  done="<condition>" — the objective is done once it holds
  quest="<questId>" — a subquest objective: done when that quest completes
  visibleWhen="<condition>" — hides the objective while false; never gates `done`
  title="<text>" — the objective's name
  optional="true" — not required for the quest to complete
  on="<occasion>" — judged when that occasion is raised
  by="<condition>" — a deadline: the first time it holds while not done, the objective fails
  target="<prefix>.<member>" — with `on`: judged only for a raise for that target
  until="<condition>" — with `on`: a deadline judged only when the objective's occasion is raised, after `done`
rewardKeys (5; <reward> attributes):
  kind="<rewardKind>" — what the engine pays
  target="<id>" — what the reward is for, per its kind
  amount="<integer> | <N>..<M>" — how much
  when="<condition>" — granted only while it holds
  outcome="failed" — grant when the quest fails; without it the reward grants on complete
enginePaths (13; the engine writes these — read them, never declare or ::set them):
  quest.<quest>.state: enum [active, complete, failed, unset] [quest] — the quest's lifecycle; `unset` until it activates (always assigned)
  quest.<quest>.failedBy: enum [unset, fail, by, until, subquest, cascade, superseded] [quest] — why the quest failed: its `fail`, a required objective's `by` / `until`, a required `subquest` that failed, a `cascade` from its parent, or `superseded` by a sibling; `unset` while it has not failed
  quest.<quest>.activatedAt: narrativeTime [quest] — the moment the quest activated
  quest.<quest>.objectives.<objective>.done: bool [quest] — the objective is done
  quest.<quest>.objectives.<objective>.failed: bool [quest] — the objective failed (its `by` / `until` deadline passed first)
  entry.<entry>.read: bool [run] — the entry was read this run
  entry.<entry>.everRead: bool [user] — the entry was ever read (a new run does not reset it)
  scene.choices.<branch> [scene] — the choice a `<branch>` / `<hub>` took: one of its choice ids, `unset` before
  scene.visited.<hub>.<choice> [scene] — that `<hub>` choice was ever taken (bool)
  occasion.target [occasion] — in a beat of a targeted occasion: the member the answered raise is for
  occasion.payload.<field> [occasion] — in a beat of an occasion with a `payload:`: that field of the answered raise
  clock.<field> [run] — derived from the declared `clock:` (day, slot, weekday, index, ended …)
  prev.<path> [run] — the previous run's (or season window's) value of `<path>`
scenes (2; read as visited("<id>")):
  mira.s01ep01, mira.s01ep02
```

`enums (0)` is not a bug: that line counts members supplied by the active *plugins*, and this file
activates none. Your own declarations show up under **`projectEnums`** — the vocabulary that
actually resolves `emotion="content"`.

`stateSchema` is the same story for state: your own declarations (`scene.knowsMira` from Part 3,
`run.metMira` from Part 5), `scene.choices.orderChoice`, which the `<branch>` declares on your
behalf so a later construct can read which option the player took, and `prev.run.metMira`, the
read-only value `run.metMira` had when the previous run ended.

`builtinDirectives` lists the directives the language itself provides — `::set` you have already
used. `beatKeys` and `questKeys` list every key a beat's header (a scene's frontmatter, an
`<entry>` or `<beat>`) and a `<quest>` element may carry, with one line on what each does — the
place to look when you meet `on:` or `once:` in someone else's scene. `scenes` lists the ids
`visited("…")` can name. `episodes/` holds a `lute.project.yaml`, so `context` resolves the file
against that project, as `lute check` does, and says so in the `lute: note:` line; the list therefore
covers every scene in the project, alongside its quests and lore entries.

Run it any time you need to double-check a directive name, an attribute, or a legal `emotion` value
instead of guessing. From here, follow the **Language** section for each construct in depth, keep
the [cheatsheet](/reference/cheatsheet/) open while you write, or read the
[full-spec showcase](/examples/showcase/) for a feature-by-feature tour of a real project. For whole
games — a cozy mystery, an Ink port, roguelikes, visual novels — each a checked project with its
tests and plays, see the [example games](/examples/games/).

**Editor and terminal disagree?** Run `lute doctor .` in the project. It checks your toolchain and
project setup and names an editor language server that is older than your `lute`.

**Building a game the engine drives by moments** — the player walks into the hub, talks to
someone, ends the day — rather than a fixed episode order? Scaffold a working example to start
from:

```
$ lute init --template beats my-game
```

It sets up an occasions plugin, beats that answer those moments, a quest, lore entries, a play
script, and scenario tests — `lute check-project`, `lute test`, and `lute play` all pass as
scaffolded. [Beats](/language/beats/) and [Playing a story](/tooling/play/) explain the model.
