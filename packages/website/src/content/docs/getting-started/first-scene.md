---
title: Write your first scene
description: Build one small, real Lute scene from an empty file step by step, running the lute tool at every step to see exactly what it reports.
---

This is the "start here" for a scenario writer who has never touched Lute — no compiler background
required. It builds **one small real scene** from an empty file, step by step, running the actual
`lute` tool at every step so you can see exactly what it says. It targets language version
**0.5.2**.

You need a plain-text editor, a terminal, and the `lute` command
([install it first](/getting-started/installation/)). Everything you write here is **core Lute
only** — no plugins, no project configuration. Just the language itself.

## Part 1 — The minimal skeleton

Create an empty file, `my-scene.lute`, and run the checker on it — the checker tells you whether a
`.lute` file is valid:

```
$ lute check my-scene.lute
my-scene.lute:1:1: error [E-KIND-MISSING] required frontmatter key `kind` is missing; every root document must declare `kind: scene` or `kind: quest`
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

- `kind: scene` — this file is a scene (the other kind, `quest`, is for quest-logic files).
- `character` — whose episode this is (the point-of-view character's storyline).
- `season` / `episode` — which episode this scene belongs to.
- `pov` — the id of the player character (the protagonist the player controls).

Save that and re-check:

```
$ lute check my-scene.lute
ok: my-scene.lute (0 warning(s))
```

Clean — but the file has no content yet. Try adding a line of narration directly under the
frontmatter:

```
@narrator: The diner is empty at this hour, and Mira likes it that way.
```

Check again:

```
$ lute check my-scene.lute
my-scene.lute:10:1: error [E-CONTENT-OUTSIDE-SHOT] content lives inside a shot/scene; add a `## Shot N.` heading above it
```

The rule to remember: **all content lives under a heading.** A Lute document is a sequence of
"shots" — beats of the scene — and every line of dialogue, narration, or staging sits inside one.
Add a heading before the line:

```lute
## Shot 1.

@narrator: The diner is empty at this hour, and Mira likes it that way.
```

(`## Shot 1.`, `## Shot 2.`, … — or `## Scene 1.` if you prefer that word. The number and the
trailing period are both required.)

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

- `@mira` is the speaker. `emotion="content"` and `variant="0"` pick which portrait/pose to show —
  these are catalog vocabulary, not something you invent; `lute context` (Part 4) lists the legal
  values for your project.
- `{{userName}}` is an **interpolation** — text wrapped in double braces gets filled in at runtime.
  `{{userName}}` is the one that's always available: the player's own name.

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

```
$ lute check my-scene.lute
my-scene.lute:19:1: error [E-LEGACY-CONTENT-SIGIL] content line sigil `:` was replaced by `@` — write `@speaker{…}: text`; `lute fix` applies this migration automatically
```

**Reading a diagnostic:** `file:line:col: error [CODE] message`. It names the exact line, the exact
problem, and exactly what to write instead. For this mechanical class of fix, run:

```
$ lute fix my-scene.lute
lute: migrated 1 edit(s)
```

`lute fix` rewrites the file in place (only what needs to change) and re-check comes back clean.

Once a file checks clean, `lute compile` turns it into the flat JSON command list the game engine
plays — one entry per line, choice, and jump, in order:

```
$ lute compile my-scene.lute
{
  "kind": "scene",
  "lute": "0.5.2",
  "meta": { "character": "mira", "season": 1, "episode": 1, "episodeId": "s01ep01", "title": "A Quiet Table" },
  "state": [ … ],
  "commands": [
    { "kind": "line", "role": "narration", "speaker": "narrator",
      "text": "The diner is empty at this hour, and Mira likes it that way." },
    { "kind": "line", "role": "dialogue", "speaker": "mira",
      "text": "{{userName}}, you made it.", "emotion": "content", "variant": 0 },
    …
  ]
}
```

(Shown reformatted for space.) You never hand-edit this file — it's the compiled artifact the
engine consumes. That it compiled without error is proof the scene is **statically valid**: every
construct well-formed, every state path declared, every `<match>` exhaustive. It is not proof the
scene is playable end to end — that's a runtime property, verified at integration time.

Finally, `lute trace` previews a playthrough without opening the game — you tell it which choice to
take at each branch with `--choose <branchId>=<choiceId>`, and it walks the scene and prints what
would show on screen:

```
$ lute trace my-scene.lute --choose orderChoice=black
trace: my-scene.lute
  ## Shot 1.
    @narrator  The diner is empty at this hour, and Mira likes it that way.
    @mira  {{userName}}, you made it.
    @mira  I should not be this pleased about a coffee order.
  <branch orderChoice>   eligible: black   -> black
    @mira  Good. No nonsense in a cup.
trace complete: 1 decision; choices 1/2 (orderChoice)
```

That transcript previews exactly the mock scenario you supplied — an authoring aid for
sanity-checking that a branch reads right, never proof of runtime behavior.

## Part 5 — Sequencing scenes with `after:`

A real episode is a *sequence* — one scene is meant to come after the player has seen another. You
declare that intended ordering with one frontmatter key: **`after:`**.

`after:` declares the routes Lute's checker and `lute scenario` analyses assume reach this scene. It
doesn't move the player anywhere and it isn't a jump — it's *advisory*: the tool uses it to verify
your episodes fit together into one coherent, analysable graph.

`after:` is deliberately tiny. You get exactly two building blocks:

- `visited("<sceneKey>")` — true once the player has seen that scene.
- `completed("<questId>")` — true once that quest is finished.

Combine them with `&&` (both) and `||` (either):

```yaml
after: 'visited("mira.s01ep01")'
after: 'visited("mira.s01ep01") && completed("theCoffeeDebt")'
after: 'visited("mira.s01ep01") || visited("mira.s01ep02")'
```

That is the whole vocabulary. There is no `!`, no arithmetic, and no reading state — those are
intentionally left out. Anything conditional on runtime state stays in your `when=` guards.

A scene's **canonical key** is `{character}.{episodeId}`, where `episodeId` defaults to
`s{season}ep{episode}` (zero-padded). Our tutorial scene (`character: mira`, `season: 1`,
`episode: 1`) has key **`mira.s01ep01`**.

To carry a fact across episodes, use the persistent **`run.`** tier. Teach the diner to remember
the meeting — declare `run.metMira` and set it:

```yaml
state:
  run.metMira: { type: bool }
```

```lute
::set{run.metMira = true}
```

`after:` and cross-scene reads only make sense across several files, so put both scenes in a folder
with a one-line `lute.project.yaml` marking it a project root:

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

## Shot 1.

@mira{emotion="content" variant="0" when="run.metMira"}: Back again. You know where you sit.

@narrator: The coffee is already poured.
```

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
  edges (prerequisite -> dependent):
    scene(mira.s01ep01) -> scene(mira.s01ep02)
```

`reach <key>` answers "can the player ever get here, and by what route?"; `envelope <key>` answers
the question you most want before writing a `when=` guard — *what state is safe to read here?*:

```
$ lute scenario episodes envelope mira.s01ep02
envelope for scene(mira.s01ep02) (pre-entry):
  Guaranteed (safe to read under your declared routes):
    - run.metMira
```

`run.metMira` is **Guaranteed** because of the route: every declared route into the booth passes
through the diner, which always `::set`s it. That's a genuine cross-scene guarantee — the booth's
`when="run.metMira"` read is provably safe.

## Part 6 — Where to go next

**Not sure what's legal to write?** `lute context <file>` prints exactly the vocabulary your
project accepts — the staging directives, their attributes, the enum values (like `emotion`), the
declared state, and the delivery-flag vocabulary — resolved for the specific file you give it:

```
$ lute context my-scene.lute
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
  emotion: neutral, surprised, delighted, shy, content, angry, sad
  mood: peaceful, tense, romantic, sad, upbeat
  …
deliveryFlags (3):
  {mono}: interior monologue / thought (not spoken aloud in-scene)
  {os}: off-screen: the speaker is heard but not currently staged/visible
  {vo}: voiceover: narration-style delivery layered over the scene
```

Run it any time you need to double-check a directive name, an attribute, or a legal `emotion` value
instead of guessing. From here, follow the **Language** section for each construct in depth, or read
the [full-spec showcase](/examples/showcase/) for a feature-by-feature tour of a real project.
