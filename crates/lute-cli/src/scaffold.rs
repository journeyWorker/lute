//! `lute init` / `lute new` — project and document scaffolding.
//!
//! Every generated artifact is designed to pass the checker CLEAN: `lute init`
//! output survives `lute check-project <dir>` (and the `beats` and
//! `investigation` templates' `lute test` and `lute play` as well), and `lute
//! new` output survives `lute check-project` of the project it lands in
//! (modulo the advisory an accept-driven `lute new quest` stub carries until
//! a scene `::accept`s it). Frontmatter stamps the
//! current [`lute_check::LUTE_LANG_VERSION`] unless the manifest's
//! `defaults:` already supplies `luteVersion:`, so no `W-LUTE-VERSION-STALE`
//! fires, and every read state path carries a `default:` so definite
//! assignment holds without a cross-scene envelope.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lute_manifest::project::MetaDefaults;

/// One scaffolded file: a path RELATIVE to the target directory and its
/// verbatim contents.
struct File {
    rel: &'static str,
    content: String,
}

/// The `lute.project.yaml` of the `minimal` template — a
/// core-only profile (no plugins), so each document resolves against the
/// built-in `lute.core` snapshot. The default `voiceKey` already carries
/// `{prefix}` (dsl 0.22.0 §11), so no `identity:` block is needed.
fn project_manifest() -> String {
    "\
# Lute project manifest — core-only profile (no plugins). Every document under
# this directory resolves against the built-in `lute.core` capability snapshot.
defaultProfile: core
profiles:
  core:
    plugins: {}
"
    .to_string()
}

/// The starter content vocabulary shared by every template (dsl 0.9.0 D-F).
///
/// The compiler declares the seven vocabulary SLOTS and ships NO members
/// (`lute_manifest::core::load_core_snapshot`), so using any of them is
/// `E-DOMAIN-UNKNOWN` until a project declares its own. That default has to
/// live SOMEWHERE for a fresh project to check clean, and here — in a
/// scaffolded file the author owns — is the only place it can live without
/// baking a genre into the binary: editing this file is an edit, whereas
/// repudiating a compiled-in member list is a fight.
///
/// All seven slots are filled, not just the ones the starter scene happens to
/// use: a half-filled vocabulary would hand a fresh project an
/// `E-DOMAIN-UNKNOWN` the first time an author reached for `::music` or
/// `::vfx`, which is exactly the first contact this file exists to prevent.
fn vocabulary_schema() -> String {
    "\
# Your project's content vocabulary (dsl 0.9.0).
#
# Lute's compiler ships NO members — a general authoring tool should not decide
# what emotions your characters have. This file is yours to edit; the starter
# set below is a convention, not a rule.
#
# `action` must declare `exits:` (which members end a character's presence on
# stage) and `anchor` must declare `default:` (the member used when a `::auto`
# omits it). The compiler reads those instead of guessing from names.
enums:
  emotion: [neutral, surprised, delighted, shy, content, angry, sad]
  anchor:
    members: [left, center, right]
    default: center
  action:
    members: [fade-in-up, sway, lean, idle, fade-out, hide]
    exits: [fade-out, hide]
  # The four remaining slots, typed by the staging directives (`::music`,
  # `::vfx`, `::auto`). Declared up front so reaching for one is an edit to
  # THIS list rather than an `E-DOMAIN-UNKNOWN`.
  mood: [peaceful, tense, romantic, sad, upbeat]
  volume: [silent, down, normal, up, full]
  musicAction: [start, change, stop, resume, fade-out]
  vfxType: [whiteOut, blackOut, rain, snow, leaves, petals, raindrop]
"
    .to_string()
}

/// The `minimal` template: one entry scene over a tiny scalar schema.
fn minimal_files() -> Vec<File> {
    vec![
        File {
            rel: "lute.project.yaml",
            content: project_manifest(),
        },
        File {
            rel: "world.schema.yaml",
            content: "\
# Minimal shared world schema (dsl §9). Imported by scenes via
# `uses: ../world.schema.yaml`. Every path carries a `default:` so each read is
# definitely assigned even in a standalone single-file `lute check`.
state:
  run.greeted: { type: bool, default: false }
"
            .replace("{lang}", lute_check::LUTE_LANG_VERSION),
        },
        File {
            rel: "vocabulary.schema.yaml",
            content: vocabulary_schema(),
        },
        File {
            rel: "scenes/opening.lute",
            content: "\
---
kind: scene
luteVersion: \"{lang}\"
character: narrator
season: 1
episode: 1
title: Opening
uses:
  - ../world.schema.yaml
  - ../vocabulary.schema.yaml
---

## Opening

@narrator{emotion=\"delighted\"}: Welcome to your new Lute project.
::set{ run.greeted = true }
@narrator{when=\"run.greeted\"}: Edit this scene, then run `lute check-project`.
"
            .replace("{lang}", lute_check::LUTE_LANG_VERSION),
        },
        File {
            rel: "mocks/playthrough.yaml",
            content: "\
# Trace mock (dsl 0.4.0 §4.3). `file:` names the document this mock previews,
# resolved against this file. Preview with:
#   lute trace scenes/opening.lute --mock mocks/playthrough.yaml
file: ../scenes/opening.lute
state:
  run.greeted: false
"
            .replace("{lang}", lute_check::LUTE_LANG_VERSION),
        },
        File {
            rel: "README.md",
            content: readme("minimal"),
        },
    ]
}

/// The `investigation` template: a small whodunit on the `beats` skeleton
/// (manifest `defaults:`, an occasions plugin, beats with `id:`, lore, a
/// quest, `plays/` and `tests/`). The engine raises `arrive`, the targeted
/// `examine` (evidence, `item.<member>`) and `interview` (suspects,
/// `npc.<member>`), and `accuse`. Evidence lore `::assert`s base facts; the
/// schema's rules DERIVE the conclusions, with stratified negation (a
/// suspect is cleared by an alibi unless evidence contradicts it); the
/// accusation's choices are guarded by those derived facts, and the quest
/// is accept-driven. `check-project`, `test` and `play` pass as scaffolded.
fn investigation_files() -> Vec<File> {
    let lang = lute_check::LUTE_LANG_VERSION;
    vec![
        File {
            rel: "lute.project.yaml",
            content: format!(
                "\
# Lute project manifest. The `case` profile activates this project's own
# occasions plugin (plugins/case.occasions): the moments the engine raises
# while the detective works, which beats and lore entries answer (dsl 0.21.0).
pluginsDir: plugins/
defaultProfile: case
profiles:
  case:
    plugins: {{ case.occasions: true }}
# Frontmatter every document inherits (dsl 0.10.0 §6): no document repeats
# its language version or its schema imports.
defaults:
  luteVersion: \"{lang}\"
  uses: [world.schema.yaml, vocabulary.schema.yaml]
"
            ),
        },
        File {
            rel: "plugins/case.occasions/plugin.yaml",
            content: "\
# A capability plugin that only declares occasions. Add more files under
# occasions/ as your engine raises more moments.
id: case.occasions
version: 0.1.0
kind: capability
depends: [ { id: lute.core, range: \"^0.0.1\" } ]
exports:
  occasions: occasions/
"
            .to_string(),
        },
        File {
            rel: "plugins/case.occasions/occasions/case.yaml",
            content: "\
# The moments your engine raises (dsl 0.21.0 §2). A targeted occasion is
# raised FOR something: `examine`'s targets are `item.<member>` of the
# `evidence` entity kind, `interview`'s are `npc.<member>` of `suspect`
# (world.schema.yaml, dsl 0.22.0 §8).
occasions:
  arrive:    { select: first, description: The detective arrives at the scene of the crime }
  examine:   { select: first, target: { prefix: item, entity: evidence }, description: The detective studies a piece of evidence (item.<name>) }
  interview: { select: first, target: { prefix: npc, entity: suspect }, description: The detective questions a suspect (npc.<name>) }
  accuse:    { select: first, description: The detective is ready to name the killer }
"
            .to_string(),
        },
        File {
            rel: "world.schema.yaml",
            content: "\
# The case file, imported by every document through the manifest's
# `defaults: uses:`. What the detective has ON RECORD is base facts, asserted
# by the lore and scenes that find it; what the detective CONCLUDES is
# derived by the rules below and never asserted by hand.
state:
  run.accused: { type: { enum: [nobody, blake, cass] }, default: nobody }

entities:
  suspect:  { members: [blake, cass] }
  evidence: { members: [ledger, knife] }

relations:
  # --- base: what is on record (asserted by lore/ and scenes/) ----------
  implicates:  { args: [evidence, suspect], tier: run }
  alibi:       { args: [suspect], tier: run }
  contradicts: { args: [evidence, suspect], tier: run }
  # --- derived: what the detective can conclude (see `rules:`) ---------
  suspected: { args: [suspect], derive: true }
  broken:    { args: [suspect], derive: true }
  cleared:   { args: [suspect], derive: true }
  culprit:   { args: [suspect], derive: true }

# Datalog (dsl 0.3.0 §7) with stratified negation: `not` reads a relation
# that is fully derived first. A suspect is cleared by an alibi UNLESS the
# evidence contradicts it, and is the culprit if implicated and not cleared.
rules:
  - \"suspected(S) :- implicates(E, S)\"
  - \"broken(S) :- contradicts(E, S)\"
  - \"cleared(S) :- alibi(S), not broken(S)\"
  - \"culprit(S) :- suspected(S), not cleared(S)\"

cast:
  detective: { name: The Detective }
  blake:     { name: Arthur Blake }
  cass:      { name: Cass Moreau }
"
            .to_string(),
        },
        File {
            rel: "vocabulary.schema.yaml",
            content: vocabulary_schema(),
        },
        File {
            rel: "scenes/case/arrival.lute",
            content: "\
---
kind: scene
id: case.arrival
title: The study
# A beat: answers `arrive`, once per run.
on: arrive
once: run
---

## The study

::bg{location=\"study\" time=\"night\"}
@narrator: Lord Ashby lies across his own desk. The ledger is open; a kitchen knife lies on the rug.
@detective{emotion=\"neutral\"}: Two people had a reason tonight. Let's find out which one had the chance.
::accept{quest=\"solveCase\"}
"
            .to_string(),
        },
        File {
            rel: "scenes/interview/blake.lute",
            content: "\
---
kind: scene
id: blake.statement
title: Blake's statement
on: interview
target: npc.blake
once: run
priority: 10
---

## Blake

@blake{emotion=\"angry\"}: I was at my club until one. Ask anyone there.
::assert{ alibi(blake) }
"
            .to_string(),
        },
        File {
            rel: "scenes/interview/cass.lute",
            content: "\
---
kind: scene
id: cass.statement
title: Cass's statement
on: interview
target: npc.cass
once: run
priority: 10
---

## Cass

@cass{emotion=\"sad\"}: I was asleep. Cook brought me tea at eleven and saw me in bed.
::assert{ alibi(cass) }
"
            .to_string(),
        },
        File {
            rel: "scenes/interview/again.lute",
            content: "\
---
kind: scene
id: interview.again
title: Nothing more to say
# The fallback for `interview`: no `target:`, so it answers every suspect;
# lowest priority and repeatable.
on: interview
once: false
---

## Again

@narrator: They have told you all they intend to.
"
            .to_string(),
        },
        File {
            rel: "scenes/accusation.lute",
            content: "\
---
kind: scene
id: case.accusation
title: The accusation
on: accuse
once: run
---

## The drawing room

@detective: I know who killed Lord Ashby.

// Each accusation is offered only when the case file supports it: `culprit`
// is derived, so what the detective has found decides what can be said.
<branch id=\"accusation\" prompt=\"Who do you accuse?\">
  <choice id=\"blake\" label=\"Arthur Blake\" when=\"holds(culprit(blake))\">
    @detective: You were not at your club, Blake. The ledger was written in this room at eleven, in your hand.
    ::set{ run.accused = \"blake\" }
  </choice>
  <choice id=\"cass\" label=\"Cass Moreau\" when=\"holds(culprit(cass))\">
    @detective: It was your knife, Cass.
    ::set{ run.accused = \"cass\" }
  </choice>
  <choice id=\"wait\" label=\"Not yet\">
    @detective: Not yet. Something doesn't fit.
  </choice>
</branch>
"
            .to_string(),
        },
        File {
            rel: "quests/case.lute",
            content: "\
---
kind: quest
id: quest.case
title: Who killed Lord Ashby?
---

// No `start`: the quest begins when a scene runs ::accept{quest=\"solveCase\"}.
<quest id=\"solveCase\" title=\"Who killed Lord Ashby?\">
  <objective id=\"evidence\" title=\"Examine the evidence\" done=\"count(suspected(_)) >= 2\"/>
  <objective id=\"statements\" title=\"Hear both suspects\" done=\"holds(alibi(blake)) && holds(alibi(cass))\"/>
  // `by=` fails the quest once a wrong name is on record.
  <objective id=\"accuse\" title=\"Name the killer\" done=\"run.accused == 'blake'\" by=\"run.accused != 'nobody'\"/>
  <on event=\"questComplete\">
    @narrator: Blake is led away. The ledger goes with him.
  </on>
  <on event=\"questFailed\">
    @narrator: The wrong name is in the papers by morning.
  </on>
</quest>
"
            .to_string(),
        },
        File {
            rel: "lore/evidence.lute",
            content: "\
---
kind: lore
id: lore.evidence
title: The evidence
---

// Entry beats: lore that answers `examine` at one item. Each ::assert puts
// a fact on record on the entry's first read only (`entry.<id>.read`).
<entry id=\"knife\" on=\"examine\" target=\"item.knife\" category=\"evidence\" title=\"The kitchen knife\">
  @narrator: A kitchen knife from the Moreau household, its handle monogrammed C.M.
  ::assert{ implicates(knife, cass) }
</entry>

<entry id=\"ledger\" on=\"examine\" target=\"item.ledger\" category=\"evidence\" title=\"The ledger\">
  @narrator: The last entry, dated tonight at eleven, is in Blake's hand: a debt to Lord Ashby, struck through.
  ::assert{ implicates(ledger, blake) }
  ::assert{ contradicts(ledger, blake) }
</entry>
"
            .to_string(),
        },
        File {
            rel: "plays/the-case.play.yaml",
            content: "\
# The whole case, end to end (dsl 0.22.0):
#   lute play . --script plays/the-case.play.yaml
# Each step raises an occasion the way your engine would; `expect:` asserts
# what happened, and `lute test` runs this script alongside tests/.
choose:
  accusation: blake
steps:
  - occasion: arrive
    expect: { winner: case.arrival, quests: { solveCase: active } }
  - occasion: examine
    target: item.knife
    expect: { winner: knife, facts: [\"culprit(cass)\"] }
  - label: an alibi clears a suspect
    occasion: interview
    target: npc.cass
    expect: { winner: cass.statement, facts: [\"cleared(cass)\"], notFacts: [\"culprit(cass)\"] }
  - occasion: interview
    target: npc.blake
    expect: { winner: blake.statement, facts: [\"cleared(blake)\"] }
  - label: the ledger breaks Blake's alibi
    occasion: examine
    target: item.ledger
    expect: { winner: ledger, facts: [\"culprit(blake)\"], notFacts: [\"cleared(blake)\"] }
  - occasion: interview
    target: npc.blake
    expect: { winner: interview.again }
  - occasion: accuse
    expect: { winner: case.accusation }
expect:
  exit: complete
  quests: { solveCase: complete }
  state: { run.accused: blake }
  facts: [\"culprit(blake)\", \"cleared(cass)\"]
  notFacts: [\"culprit(cass)\"]
"
            .to_string(),
        },
        File {
            rel: "tests/accusation.test.yaml",
            content: "\
# A scenario test traces one document against mocks and asserts the outcome:
#   lute test . --project .
# Cass's alibi stands, so `not cleared(cass)` fails and she cannot be accused;
# the ledger breaks Blake's, so he can.
file: ../scenes/accusation.lute
facts:
  - \"implicates(knife, cass)\"
  - \"implicates(ledger, blake)\"
  - \"alibi(cass)\"
  - \"alibi(blake)\"
  - \"contradicts(ledger, blake)\"
choose: { accusation: blake }
expect:
  offered: { accusation: [blake, wait] }
  facts: [\"culprit(blake)\", \"cleared(cass)\"]
  state: { run.accused: blake }
"
            .to_string(),
        },
        File {
            rel: "README.md",
            content: readme("investigation"),
        },
    ]
}

/// The `beats` template (dsl 0.22.0 §13): the shape the three dogfood games
/// converged on, ready to run. The engine raises occasions (a project
/// plugin's `occasions:` export) and beats — scenes with an `id:` and `on:`,
/// and lore entries with `on=` — answer them; a quest reacts to what they do.
/// The manifest's `defaults:` supplies `luteVersion:`/`uses:`, so no document
/// repeats them. The engine-written clock is `owner: engine`, and the play
/// script writes it with an `engine:` step instead of a fake-engine scene.
/// `plays/` and `tests/` exercise it all: `check-project`, `test` and `play`
/// pass as scaffolded.
fn beats_files() -> Vec<File> {
    let lang = lute_check::LUTE_LANG_VERSION;
    vec![
        File {
            rel: "lute.project.yaml",
            content: format!(
                "\
# Lute project manifest. The `game` profile activates this project's own
# occasions plugin (plugins/game.occasions): the moments the engine raises,
# which beats answer (dsl 0.21.0).
pluginsDir: plugins/
defaultProfile: game
profiles:
  game:
    plugins: {{ game.occasions: true }}
# Frontmatter every document inherits (dsl 0.10.0 §6): no document repeats
# its language version or its schema imports.
defaults:
  luteVersion: \"{lang}\"
  uses: [world.schema.yaml, vocabulary.schema.yaml]
"
            ),
        },
        File {
            rel: "plugins/game.occasions/plugin.yaml",
            content: "\
# A capability plugin that only declares occasions. Add more files under
# occasions/ as your engine raises more moments.
id: game.occasions
version: 0.1.0
kind: capability
depends: [ { id: lute.core, range: \"^0.0.1\" } ]
exports:
  occasions: occasions/
"
            .to_string(),
        },
        File {
            rel: "plugins/game.occasions/occasions/game.yaml",
            content: "\
# The moments your engine raises (dsl 0.21.0 §2). A beat answers one with
# `on:`; `select: first` presents the single best eligible beat. A targeted
# occasion is raised FOR something: `talk`'s targets are `npc.<member>` for a
# member of the `npc` entity kind (world.schema.yaml, dsl 0.22.0 §8).
occasions:
  hubVisit: { select: first, description: The player arrives at the hub }
  talk:     { select: first, target: { prefix: npc, entity: npc }, description: The player talks to someone (npc.<name>) }
  dayEnd:   { select: first, description: \"The engine closed the day; run.day is already advanced\" }
"
            .to_string(),
        },
        File {
            rel: "world.schema.yaml",
            content: "\
# The shared world schema, imported by every document through the manifest's
# `defaults: uses:`. Every path carries a `default:` so reads are definitely
# assigned.
state:
  # Written by the engine, read by content (dsl 0.22.0 §1.2): a `::set` of it
  # is an error. `lute play` writes it with an `engine:` step.
  run.day:        { type: number, default: 1, owner: engine }
  user.bond.mara: { type: number, default: 0 }

entities:
  npc:  { members: [mara, tomas] }
  item: { members: [lamp] }

relations:
  # What the player has learned this run.
  knows: { args: [item], tier: run }

# Named conditions, read as `@firstDay`. The shorthand is the CEL body alone;
# its type is inferred (bool).
defs:
  firstDay: \"run.day == 1\"
  trusted:  \"user.bond.mara >= 1\"
"
            .to_string(),
        },
        File {
            rel: "vocabulary.schema.yaml",
            content: vocabulary_schema(),
        },
        File {
            rel: "scenes/hub/welcome.lute",
            content: "\
---
kind: scene
id: hub.welcome
title: First arrival
# A beat: answers `hubVisit`, once per save (`once: user`).
on: hubVisit
once: user
priority: 10
---

## The hub

::bg{location=\"hub\" time=\"day\"}
@narrator: The lamps along the square are lit — all but the one by the door.
"
            .to_string(),
        },
        File {
            rel: "scenes/hub/morning.lute",
            content: "\
---
kind: scene
id: hub.morning
title: Another morning
# Repeatable (`once: false`) and only after the first day.
on: hubVisit
once: false
when: '!@firstDay'
---

## Morning

@narrator: Day {{run.day}}. The square is already awake.
"
            .to_string(),
        },
        File {
            rel: "scenes/hub/day-end.lute",
            content: "\
---
kind: scene
id: hub.dayEnd
title: Lamps out
on: dayEnd
once: false
---

## Night

@narrator: One by one, the lamps go out.
"
            .to_string(),
        },
        File {
            rel: "scenes/talk/mara-first.lute",
            content: "\
---
kind: scene
id: mara.first
title: Mara by the door
on: talk
target: npc.mara
once: user
priority: 10
---

## Mara

@mara{emotion=\"content\"}: You're new. The lamp by the door has been dark for a week.

<branch id=\"maraAsk\" prompt=\"What do you say?\">
  <choice id=\"lamp\" label=\"Offer to find out why\">
    @mara{emotion=\"delighted\"}: Would you? Tomas keeps the oil. Ask him.
    ::set{ user.bond.mara += 1 }
    ::accept{quest=\"lampOut\"}
  </choice>
  <choice id=\"leave\" label=\"Say nothing\">
    @mara: Suit yourself.
  </choice>
</branch>
"
            .to_string(),
        },
        File {
            rel: "scenes/talk/mara-idle.lute",
            content: "\
---
kind: scene
id: mara.idle
title: Mara, any other time
# The fallback for `talk` at npc.mara: lowest priority, repeatable.
on: talk
target: npc.mara
once: false
---

## Mara

@mara{emotion=\"shy\" when=\"@trusted\"}: Any luck with the lamp?
@mara{when=\"!@trusted\"}: Mm.
"
            .to_string(),
        },
        File {
            rel: "quests/lamp.lute",
            content: "\
---
kind: quest
id: quest.lamp
title: The lamp by the door
---

// No `start`: the quest begins when a scene runs ::accept{quest=\"lampOut\"}.
<quest id=\"lampOut\" title=\"The lamp by the door\">
  <objective id=\"ask\" title=\"Ask Tomas about the oil\" done=\"holds(knows(lamp))\"/>
  // Judged when the engine raises `dayEnd` (dsl 0.21.0 §7a).
  <objective id=\"wait\" title=\"Wait for the day to end\" on=\"dayEnd\" done=\"run.day >= 2\"/>
  <on event=\"questComplete\">
    @narrator: By morning the lamp by the door is burning again.
  </on>
</quest>
"
            .to_string(),
        },
        File {
            rel: "lore/tomas.lute",
            content: "\
---
kind: lore
id: lore.tomas
title: Tomas
---

// Entry beats: lore that answers `talk` at npc.tomas. The first applies its
// ::assert on its first read only (`entry.tomasOil.read`).
<entry id=\"tomasOil\" on=\"talk\" target=\"npc.tomas\" category=\"bark\" title=\"Tomas and the oil\" priority=\"10\" when=\"quest.lampOut.state == 'active'\">
  @tomas: Oil? Top shelf. Tell Mara it's the wick, not the oil.
  ::assert{ knows(lamp) }
</entry>

<entry id=\"tomasBusy\" on=\"talk\" target=\"npc.tomas\" category=\"bark\" title=\"Tomas, busy\">
  @tomas: Busy.
</entry>
"
            .to_string(),
        },
        File {
            rel: "plays/first-day.play.yaml",
            content: "\
# One day, end to end (dsl 0.22.0):
#   lute play . --script plays/first-day.play.yaml
# Each step raises an occasion the way your engine would; an `engine:` step
# writes what the engine owns; `expect:` asserts what happened, and `lute test`
# runs this script alongside tests/.
choose:
  maraAsk: lamp
steps:
  - occasion: hubVisit
    expect: { winner: hub.welcome }
  - occasion: talk
    target: npc.mara
    expect: { winner: mara.first }
  - occasion: talk
    target: npc.tomas
    expect: { winner: tomasOil, offered: [tomasOil, tomasBusy] }
  - label: the engine closes the day
    engine:
      state: { run.day: { add: 1 } }
  - occasion: dayEnd
  - occasion: hubVisit
    expect: { winner: hub.morning, notOffered: [hub.welcome] }
expect:
  exit: complete
  quests: { lampOut: complete }
  state: { run.day: 2, user.bond.mara: 1 }
  facts: [knows(lamp)]
"
            .to_string(),
        },
        File {
            rel: "tests/mara-first.test.yaml",
            content: "\
# A scenario test traces one document against mocks and asserts the outcome:
#   lute test . --project .
file: ../scenes/talk/mara-first.lute
choose: { maraAsk: lamp }
expect:
  transcriptContains: [\"Tomas keeps the oil. Ask him.\"]
  state: { user.bond.mara: 1 }
"
            .to_string(),
        },
        File {
            rel: "tests/lamp-quest.test.yaml",
            content: "\
# The quest completes once the lamp is understood and the day has ended.
file: ../quests/lamp.lute
accepts: [lampOut]
facts: [\"knows(lamp)\"]
state: { run.day: 2 }
occasions: [dayEnd]
expect:
  quests: { lampOut: complete }
  transcriptContains: [\"By morning the lamp by the door is burning again.\"]
"
            .to_string(),
        },
        File {
            rel: "README.md",
            content: readme("beats"),
        },
    ]
}

/// The generated project README with next-step commands.
///
/// **No command here names a concrete document** (#31, T10.4, D-E): a command
/// that names a scaffolded file rots the moment the author renames it — the
/// first thing an author does. A `<placeholder>` cannot be pasted and cannot
/// lie; every command WITHOUT one must run green in the fresh project (pinned
/// by `init_readme_names_no_document_that_can_rot`). §8's `mocks/*.yaml` pass
/// cannot reach a README, so the README must be unable to rot rather than
/// checked for rot.
fn readme(template: &str) -> String {
    // `investigation` is built on the `beats` skeleton, so it shares its
    // commands; only `minimal` still previews through trace mocks.
    let commands = if template != "minimal" {
        "\
# Validate the whole project (recursively):
lute check-project .

# Run every scenario test (and every play script that carries `expect:`):
lute test . --project .

# Play the project through a script of raised occasions:
lute play . --script plays/<your-play>.play.yaml

# Everything you can write against a document — occasions, state, defs, ids:
lute context scenes/<your-scene>.lute --project .

# Check the toolchain and the project setup:
lute doctor .

# Add more documents (a beat answers an occasion; a targeted one takes
# `--target <prefix>.<member>`). `/` in a name nests it (`talk/<name>` lands
# in scenes/talk/); `--dir` names the PROJECT, never a subfolder:
lute new scene <name> --on <occasion> --target <target>
lute new scene <name>
lute new quest <name>
lute new lore <name>
"
    } else {
        "\
# Validate the whole project (recursively):
lute check-project .

# Check one document:
lute check scenes/<your-scene>.lute

# Preview a scene against a trace mock. Keep the mock's own `file:` pointed at
# the scene you pass here — `lute check-project` reports a mock whose subject
# does not exist, and `lute trace` refuses a mock that names a different one:
lute trace scenes/<your-scene>.lute --mock mocks/<your-mock>.yaml

# Report the scene graph / reachability:
lute scenario .

# Add more documents:
lute new scene <name>
lute new quest <name>
lute new lore <name>
lute new schema <name>
"
    };
    format!(
        "\
# Lute project (`{template}` template)

Scaffolded by `lute init --template {template}`. Every file already passes the
checker.

## Next steps

Where a command below names a document it uses a `<placeholder>` — substitute
the one you mean. The scaffolded starting documents are yours to rename or
replace freely; these instructions stay true.

```sh
{commands}```
"
    )
}

/// Scaffold a new project directory. See [`crate::Command::Init`].
///
/// Refuses (exit `2`) an unknown template or a directory that already carries a
/// `lute.project.yaml`. Otherwise creates the directory tree and every template
/// file, then prints a friendly summary. Exit `0` on success, `2` on any I/O
/// failure.
pub fn run_init(dir: &Path, template: Option<&str>) -> ExitCode {
    let template = template.unwrap_or("minimal");
    let files = match template {
        "minimal" => minimal_files(),
        "investigation" => investigation_files(),
        "beats" => beats_files(),
        other => {
            eprintln!(
                "lute init: unknown template `{other}` (expected `minimal`, `investigation`, or `beats`)"
            );
            return ExitCode::from(2);
        }
    };

    // Refuse to clobber an existing project (dir exists AND has a manifest).
    if dir.join("lute.project.yaml").exists() {
        eprintln!(
            "lute init: `{}` already contains a lute.project.yaml — refusing to overwrite",
            dir.display()
        );
        return ExitCode::from(2);
    }

    for file in &files {
        let path = dir.join(file.rel);
        if let Some(parent) = path.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                eprintln!("lute init: cannot create `{}`: {e}", parent.display());
                return ExitCode::from(2);
            }
        }
        if let Err(e) = fs::write(&path, &file.content) {
            eprintln!("lute init: cannot write `{}`: {e}", path.display());
            return ExitCode::from(2);
        }
    }

    let d = dir.display();
    println!("Initialized `{template}` Lute project at {d}");
    println!("  created {} file(s):", files.len());
    for file in &files {
        println!("    {}", dir.join(file.rel).display());
    }
    println!();
    println!("Next steps:");
    println!("  lute check-project {d}");
    let play = files.iter().find(|f| f.rel.starts_with("plays/"));
    if let Some(play) = play {
        println!("  lute test {d} --project {d}");
        println!("  lute play {d} --script {}", dir.join(play.rel).display());
        println!("  lute new scene <name> --on <occasion> --dir {d}");
    } else {
        println!("  lute scenario {d}");
        println!("  lute new scene <name> --dir {d}");
    }
    ExitCode::SUCCESS
}

/// Turn an arbitrary document name into a valid lower-camel identifier for an
/// id / state path segment (dsl §9.4 forbids `-` in a path segment): the name
/// is split on every non-alphanumeric run, the first word lower-cased and each
/// subsequent word capitalized, then a leading digit is prefixed with `q`.
/// Empty input degrades to `fallback`. Documented so `lute new`'s naming rule
/// is discoverable.
fn to_ident(name: &str, fallback: &str) -> String {
    let mut out = String::new();
    let mut new_word = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            if out.is_empty() || !new_word {
                out.push(ch.to_ascii_lowercase());
            } else {
                out.extend(ch.to_uppercase());
            }
            new_word = false;
        } else {
            new_word = !out.is_empty();
        }
    }
    if out.is_empty() {
        return fallback.to_string();
    }
    if out.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        out.insert(0, 'q');
    }
    out
}

/// A `lute new` document id from its name: every `/`- or `.`-separated
/// segment through [`to_ident`], joined by `.` — so a dotted name keeps its
/// dots (`isolde.night` → `isolde.night`, `talk/mara-first` →
/// `talk.maraFirst`), matching the dotted `<group>.<name>` ids the templates
/// and docs use, while `-` (forbidden in a segment, dsl §9.4) still camels.
fn to_id(name: &str, fallback: &str) -> String {
    let segs: Vec<String> = name.split(['/', '.']).map(|seg| to_ident(seg, fallback)).collect();
    segs.join(".")
}

/// `path` made absolute against the current directory, with `.`/`..`
/// resolved lexically (the directory need not exist yet).
fn absolute(path: &Path) -> PathBuf {
    let abs = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let mut out = PathBuf::new();
    for comp in abs.components() {
        match comp {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// Where a `lute new` document lands: the project root `--dir` names, and
/// the `defaults:` its manifest supplies — a defaulted key is omitted from
/// the new document's frontmatter (dsl 0.22.0 §13). Outside any project, the
/// requested directory with no defaults. A directory INSIDE a project that is
/// not its root is [`Destination::find`]'s `Err`: `--dir` names the project,
/// and the document would otherwise land somewhere the author did not point.
struct Destination {
    root: PathBuf,
    defaults: MetaDefaults,
    in_project: bool,
}

impl Destination {
    /// `Err` carries the (absolute) root of the project enclosing `dir` when
    /// `dir` itself is not that root.
    fn find(dir: &Path) -> Result<Self, PathBuf> {
        if dir.join("lute.project.yaml").is_file() {
            let defaults = match lute_manifest::project::load_project(dir) {
                Ok(Some(config)) => config.defaults,
                Ok(None) => MetaDefaults::default(),
                Err(e) => {
                    eprintln!("lute new: warning: {e} — writing without its `defaults:`");
                    MetaDefaults::default()
                }
            };
            return Ok(Destination {
                root: dir.to_path_buf(),
                defaults,
                in_project: true,
            });
        }
        let abs = absolute(dir);
        if let Some(root) = abs.ancestors().skip(1).find(|d| d.join("lute.project.yaml").is_file()) {
            return Err(root.to_path_buf());
        }
        Ok(Destination {
            root: dir.to_path_buf(),
            defaults: MetaDefaults::default(),
            in_project: false,
        })
    }

    /// The frontmatter every new document opens with: `kind:`, `id:`, the
    /// language version unless the manifest defaults it, and `title:`.
    fn head(&self, kind: &str, id: &str, title: &str) -> String {
        let mut s = format!("---\nkind: {kind}\nid: {id}\n");
        if self.defaults.get("luteVersion").is_none() {
            s.push_str(&format!(
                "luteVersion: \"{}\"\n",
                lute_check::LUTE_LANG_VERSION
            ));
        }
        s.push_str(&format!("title: {title}\n"));
        s
    }

    /// A scene's `uses:` — nothing when the manifest defaults `uses:`;
    /// otherwise each project schema that EXISTS at the root —
    /// `world.schema.yaml` for state, `vocabulary.schema.yaml` for the dsl
    /// 0.9.0 vocabulary slots — relative to a file `depth` directories below
    /// the root.
    fn uses(&self, depth: usize) -> String {
        if self.defaults.get("uses").is_some() {
            return String::new();
        }
        let up = "../".repeat(depth);
        let schemas: Vec<&str> = ["world.schema.yaml", "vocabulary.schema.yaml"]
            .into_iter()
            .filter(|rel| self.root.join(rel).exists())
            .collect();
        match schemas.as_slice() {
            [] => String::new(),
            [one] => format!("uses: {up}{one}\n"),
            many => {
                let mut s = String::from("uses:\n");
                for rel in many {
                    s.push_str(&format!("  - {up}{rel}\n"));
                }
                s
            }
        }
    }
}

/// Create `path` with `content` (creating parent dirs), refusing to overwrite
/// an existing file. `Err` is the exit code (`2`).
fn create(path: &Path, content: &str) -> Result<(), ExitCode> {
    if path.exists() {
        eprintln!(
            "lute new: `{}` already exists — refusing to overwrite",
            path.display()
        );
        return Err(ExitCode::from(2));
    }
    if let Some(parent) = path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            eprintln!("lute new: cannot create `{}`: {e}", parent.display());
            return Err(ExitCode::from(2));
        }
    }
    if let Err(e) = fs::write(path, content) {
        eprintln!("lute new: cannot write `{}`: {e}", path.display());
        return Err(ExitCode::from(2));
    }
    Ok(())
}

fn created(path: &Path, hint: &str) -> ExitCode {
    println!("created {}", path.display());
    println!("  {hint}");
    ExitCode::SUCCESS
}

/// Whether a beat on `on` (and `target`) answers something the project
/// declares, judged against the new scene's OWN resolution — the snapshot
/// and entity vocabulary `lute check` resolves for it (`crate::build_input`,
/// `fold_env`), so the scaffold cannot disagree with the checker. With no
/// declared occasion vocabulary (shape-only, dsl 0.21.0 §2) any occasion is
/// accepted. A targeted occasion may be answered without a target (the beat
/// then answers every target).
fn validate_beat(path: &Path, root: &Path, on: &str, target: Option<&str>) -> Result<(), String> {
    let built = crate::build_input(path, None, Some(root), None)
        .ok_or_else(|| format!("cannot read back `{}`", path.display()))?;
    let occasions = &built.input.snapshot.occasions;
    if occasions.is_empty() {
        return Ok(());
    }
    let Some(decl) = occasions.get(on) else {
        let declared: Vec<&str> = occasions.keys().map(String::as_str).collect();
        let hint = lute_manifest::suggest::nearest(on, declared.iter().copied(), 2)
            .map_or_else(String::new, |near| format!(" — did you mean `{near}`?"));
        return Err(format!(
            "occasion `{on}` is not declared by the project's plugins{hint} (declared: {})",
            declared.join(", ")
        ));
    };
    match target {
        None => Ok(()),
        Some(t) if !decl.target.takes_target() => Err(format!(
            "occasion `{on}` is not raised for a target — drop `--target {t}`"
        )),
        Some(t) => {
            let (doc, _) = lute_syntax::parse(&built.input.text);
            let (folded, _, _) = lute_check::fold_env(&doc, &built.input);
            lute_check::occasion_target_ok(decl, t, &folded.env.rel_vocab.kinds)
        }
    }
}

/// `lute new scene <name> [--on <occasion> [--target <target>]]`.
///
/// The scene lands at `<root>/scenes/<name>.lute` (`/` in the name nests it)
/// with `id:` = [`to_id`] of the name (`talk/mara-first` → `talk.maraFirst`,
/// `isolde.night` → `isolde.night`). With `--on` it is a beat
/// answering that occasion (dsl 0.21.0 §3); the occasion and target are
/// validated against the project, and the file is removed again when they
/// do not resolve (exit `2`). Without `--on` it is a linear scene opening on
/// a `::bg`.
fn new_scene(name: &str, dest: &Destination, on: Option<&str>, target: Option<&str>) -> ExitCode {
    let path = dest.root.join("scenes").join(format!("{name}.lute"));
    let id = to_id(name, "scene");
    let title = name.rsplit('/').next().unwrap_or(name);
    let mut content = dest.head("scene", &id, title);
    let body = match on {
        Some(on) => {
            content.push_str(&format!(
                "# A beat (dsl 0.21.0 §3): presented when the engine raises `{on}`. Add\n\
                 # `priority:`, `once:` (run | user | false) and `when:` as needed.\n\
                 on: {on}\n"
            ));
            let raised_for = match target {
                Some(t) => {
                    content.push_str(&format!("target: {t}\n"));
                    format!(" for `{t}`")
                }
                None => String::new(),
            };
            format!(
                "## {title}\n\n@narrator: What happens when `{on}` is raised{raised_for}. Replace this with your own lines.\n"
            )
        }
        None => format!(
            "## {title}\n\n::bg{{location=\"{id}\"}}\n@narrator: Replace this with your own lines.\n"
        ),
    };
    content.push_str(&dest.uses(1 + name.matches('/').count()));
    content.push_str("---\n\n");
    content.push_str(&body);
    if let Err(code) = create(&path, &content) {
        return code;
    }
    if let Some(on) = on {
        if let Err(reason) = validate_beat(&path, &dest.root, on, target) {
            let _ = fs::remove_file(&path);
            eprintln!("lute new: {reason}; nothing was written");
            return ExitCode::from(2);
        }
    }
    created(&path, &format!("check it with: lute check {}", path.display()))
}

/// `lute new quest <name> [--start]`.
///
/// Self-contained: declares its own `run.<ident>Progress` scalar (with a
/// `default:`, so the objective's `done` read is definitely assigned) and one
/// objective gated on it. Accept-driven by default — no `start`, so the
/// quest stays inactive until content runs `::accept{quest="<ident>"}`, the
/// shape quest-heavy games start from; `--start` scaffolds the auto-starting
/// `start="true"` form instead. The quest id / state segment is
/// [`to_ident`] of the name (a single lower-camel identifier, as
/// `quest.<id>.state` needs), while the file stem keeps the raw name. The
/// document id (dsl 0.19.0 §2.1) is `quest.` + [`to_id`] of the name —
/// namespaced so it cannot collide with a scene id or a same-named `lute new
/// lore` bundle.
fn new_quest(name: &str, dest: &Destination, start: bool) -> ExitCode {
    let path = dest.root.join("quests").join(format!("{name}.lute"));
    let ident = to_ident(name, "quest");
    let progress = format!("run.{ident}Progress");
    let (lifecycle, start_attr) = if start {
        ("// `start=\"true\"`: active from the first moment of play.\n", " start=\"true\"")
    } else {
        (
            "// Accept-driven: inactive until a scene or lore entry runs\n\
             // ::accept{quest=\"IDENT\"} — add that where the player takes the quest on.\n",
            "",
        )
    };
    let lifecycle = lifecycle.replace("IDENT", &ident);
    let content = format!(
        "{}\
# Self-contained progress counter — a scene can bump it with
# `::set{{ {progress} += 1 }}` to satisfy the objective below.
state:
  {progress}: {{ type: number, default: 0 }}
---

{lifecycle}<quest id=\"{ident}\" title=\"{name}\"{start_attr}>
  <objective id=\"begin\" title=\"Make progress\" done=\"{progress} >= 1\"/>
</quest>
",
        dest.head("quest", &format!("quest.{}", to_id(name, "quest")), name)
    );
    if let Err(code) = create(&path, &content) {
        return code;
    }
    created(&path, &format!("check it with: lute check {}", path.display()))
}

/// `lute new lore <name>` (dsl 0.19.0 §2).
///
/// Self-contained: one `<entry>` whose id is [`to_ident`] of the name, attached
/// to `item.<ident>` as a `note`, with one content line. Entries live under
/// `lore/`, mirroring `quests/`; the file stem keeps the raw name. The
/// document id (§2.1) is `lore.` + [`to_id`] of the name, namespaced like
/// `lute new quest`'s.
fn new_lore(name: &str, dest: &Destination) -> ExitCode {
    let path = dest.root.join("lore").join(format!("{name}.lute"));
    let ident = to_ident(name, "entry");
    let content = format!(
        "{}\
# Each <entry> is text the engine looks up (an item description, a found
# note, a codex page). `target` names the engine-owned thing it belongs to;
# `::set`/`::assert` in a body apply on the first read only, after which
# `entry.<id>.read` is true.
---

<entry id=\"{ident}\" target=\"item.{ident}\" category=\"note\" title=\"{name}\">
  @narrator: A note about {name}. Replace this with your own text.
</entry>
",
        dest.head("lore", &format!("lore.{}", to_id(name, "entry")), name)
    );
    if let Err(code) = create(&path, &content) {
        return code;
    }
    created(&path, &format!("check it with: lute check {}", path.display()))
}

/// `lute new schema <name>` — a `<name>.schema.yaml` skeleton at the project
/// root. Schema files are declaration maps (no `.lute` body), imported via
/// `uses:` (or the manifest's `defaults: uses:`); they are not `lute
/// check`-able on their own.
fn new_schema(name: &str, dest: &Destination) -> ExitCode {
    let path = dest.root.join(format!("{name}.schema.yaml"));
    let content = format!(
        "\
# {name} schema (dsl §9). A pure declaration map — no `---`/body. Import it
# from a document with `uses:`, or list it in lute.project.yaml's
# `defaults: uses:`. Every path should carry a `default:` so reads are
# definitely assigned.
state:
  run.example: {{ type: number, default: 0 }}

# Relational vocabulary (0.3.0 §3/§4) — uncomment and extend as needed:
# entities:
#   thing: {{ members: [a, b] }}
# relations:
#   rel: {{ args: [thing], tier: run }}
# facts:
#   - \"rel(a)\"
# rules:
#   - \"derived(X) :- rel(X)\"
"
    );
    if let Err(code) = create(&path, &content) {
        return code;
    }
    let hint = if dest.defaults.get("uses").is_some() {
        format!("import it by adding `{name}.schema.yaml` to lute.project.yaml's `defaults: uses:`")
    } else {
        format!("import it with: uses: ./{name}.schema.yaml")
    };
    created(&path, &hint)
}

/// Where `--dir` should have pointed for a directory inside a project that is
/// not its root, spelled as the command to run instead: the subdirectory
/// relative to the root (minus the kind's own folder) moves into `<name>`,
/// and `--dir <root>` is added unless the root is the current directory.
fn nested_hint(kind: &str, name: &str, dir: &Path, root: &Path) -> String {
    let rel = absolute(dir);
    let rel = rel.strip_prefix(root).unwrap_or(&rel);
    let folder = match kind {
        "scene" => "scenes",
        "quest" => "quests",
        "lore" => "lore",
        _ => "",
    };
    let sub = if folder.is_empty() {
        Path::new("")
    } else {
        rel.strip_prefix(folder).unwrap_or(rel)
    };
    let mut suggestion = format!("lute new {kind} ");
    for seg in sub.components() {
        suggestion.push_str(&seg.as_os_str().to_string_lossy());
        suggestion.push('/');
    }
    suggestion.push_str(name);
    if std::env::current_dir().map(|cwd| absolute(&cwd)).ok().as_deref() != Some(root) {
        suggestion.push_str(&format!(" --dir {}", root.display()));
    }
    format!(
        "lute new: `--dir` names the project; did you mean `{suggestion}`? (`{}` is inside the \
         project at `{}`, not its root; nothing was written)",
        dir.display(),
        root.display()
    )
}

/// Scaffold one new document into a project. See [`crate::Command::New`].
///
/// Kinds `scene`/`quest`/`lore`/`schema`; an unknown kind, `--on` on a
/// non-scene, or `--start` on a non-quest is a usage error (exit `2`).
/// Refuses to overwrite an existing target (exit `2`). A `dir` inside a
/// project but not its root is refused (exit `2`, nothing written) with the
/// `<sub>/<name>` spelling to use: `--dir` names the project, and writing to
/// `<root>/scenes/<name>` when the author pointed at `scenes/talk/` would
/// land the document somewhere they did not ask for. Outside a project (no
/// `lute.project.yaml` at or above `dir`) it says so — and refuses `--on`,
/// since no occasion is declared there for the beat to answer (dsl 0.22.0
/// §13).
pub fn run_new(
    kind: &str,
    name: &str,
    dir: &Path,
    on: Option<&str>,
    target: Option<&str>,
    start: bool,
) -> ExitCode {
    if !matches!(kind, "scene" | "quest" | "lore" | "schema") {
        eprintln!(
            "lute new: unknown kind `{kind}` (expected `scene`, `quest`, `lore`, or `schema`)"
        );
        eprintln!("usage: lute new <scene|quest|lore|schema> <name> [--dir <PROJECT>] [--on <OCCASION> [--target <TARGET>]] [--start]");
        return ExitCode::from(2);
    }
    if let (Some(_), false) = (on, kind == "scene") {
        eprintln!("lute new: `--on` makes a scene a beat; a {kind} takes no `--on`");
        return ExitCode::from(2);
    }
    if start && kind != "quest" {
        eprintln!("lute new: `--start` makes a quest auto-start; a {kind} takes no `--start`");
        return ExitCode::from(2);
    }
    let dest = match Destination::find(dir) {
        Ok(dest) => dest,
        Err(root) => {
            eprintln!("{}", nested_hint(kind, name, dir, &root));
            return ExitCode::from(2);
        }
    };
    if !dest.in_project {
        if let Some(on) = on {
            eprintln!(
                "lute new: `{}` is not inside a Lute project (no lute.project.yaml at or above it), \
                 so no occasion `{on}` is declared for a beat to answer — create one with \
                 `lute init --template beats <dir>`",
                dir.display()
            );
            return ExitCode::from(2);
        }
        eprintln!(
            "lute new: note: `{}` is not inside a Lute project (no lute.project.yaml at or above \
             it) — writing a self-contained document; `lute init <dir>` creates a project",
            dir.display()
        );
    }
    match kind {
        "scene" => new_scene(name, &dest, on, target),
        "quest" => new_quest(name, &dest, start),
        "lore" => new_lore(name, &dest),
        _ => new_schema(name, &dest),
    }
}
