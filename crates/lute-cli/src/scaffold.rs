//! `lute init` / `lute new` — project and document scaffolding.
//!
//! Every generated artifact is designed to pass the checker CLEAN: `lute init`
//! output survives `lute check-project <dir>` (and the `beats` template's
//! `lute test` and `lute play` as well), and `lute new` output survives
//! `lute check-project` of the project it lands in. Frontmatter stamps the
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

/// The `lute.project.yaml` of the `minimal` and `investigation` templates — a
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

/// The `investigation` template: a trimmed whodunit — two sequenced scenes, a
/// quest, and a relational fact world (entities/relations/facts/rules). A
/// structural echo of `docs/examples/investigation/`, kept small.
fn investigation_files() -> Vec<File> {
    vec![
        File {
            rel: "lute.project.yaml",
            content: project_manifest(),
        },
        File {
            rel: "world.schema.yaml",
            content: "\
# Investigation world schema (dsl §9 scalars + 0.3.0 §3/§4 relational
# vocabulary). Imported by every scene and the quest via `uses:`.

# --- Scalar run state (each path has a `default:`) ------------------------
state:
  run.cluesLogged:  { type: number, default: 0 }
  run.suspectFocus: { type: { enum: [none, blake, cass] }, default: none }

# --- Relational fact world (0.3.0 §3) ------------------------------------
entities:
  suspect: { members: [blake, cass] }
  clue:    { members: [ledger, knife] }

relations:
  # asserted by the crime scene as the detective logs evidence.
  foundClue:  { args: [clue], tier: run }
  # the static case map — which clue points at which suspect.
  implicates: { args: [clue, suspect], tier: run, key: [0] }
  # DERIVED: a suspect the found clues implicate (see `rules:`).
  points:     { args: [suspect], derive: true }

# Seed facts (0.3.0 §4): the fixed evidence-to-suspect map.
facts:
  - \"implicates(ledger, blake)\"
  - \"implicates(knife, cass)\"

# Datalog derivation (0.3.0 §7): a suspect is `points`-ed at once a clue that
# implicates them has been found.
rules:
  - \"points(S) :- foundClue(C), implicates(C, S)\"
"
            .replace("{lang}", lute_check::LUTE_LANG_VERSION),
        },
        File {
            rel: "vocabulary.schema.yaml",
            content: vocabulary_schema(),
        },
        File {
            rel: "scenes/crime-scene.lute",
            content: "\
---
kind: scene
luteVersion: \"{lang}\"
character: detective
season: 1
episode: 1
title: The Crime Scene
# Graph ROOT: no `after:`, so this scene is an unconditional entry point.
uses:
  - ../world.schema.yaml
  - ../vocabulary.schema.yaml
---

## The Study

@narrator: The victim's study, untouched since the coroner left.
::assert{ foundClue(ledger) }
::set{ run.cluesLogged += 1 }
@detective{emotion=\"surprised\"}: A ledger, its balances scratched out in red ink.
@detective{mono}: One name is starting to surface.
"
            .replace("{lang}", lute_check::LUTE_LANG_VERSION),
        },
        File {
            rel: "scenes/interview.lute",
            content: "\
---
kind: scene
luteVersion: \"{lang}\"
character: detective
season: 1
episode: 2
title: The Interview
# Sequenced AFTER the crime scene (canonical key `detective.s01ep01`).
after: 'visited(\"detective.s01ep01\")'
uses:
  - ../world.schema.yaml
  - ../vocabulary.schema.yaml
---

## The Interview Room

@narrator: Three chairs, one table, and the smell of cold coffee.

<hub id=\"interrogate\">
  <choice id=\"pressLedger\" label=\"Press them on the ledger\">
    @detective: These numbers were bled dry. Explain them.
    ::set{ run.suspectFocus = \"blake\" }
  </choice>
  <choice id=\"leave\" label=\"End the interview\" exit>
    @detective: We're done here. For now.
  </choice>
</hub>
"
            .replace("{lang}", lute_check::LUTE_LANG_VERSION),
        },
        File {
            rel: "quests/identify-killer.lute",
            content: "\
---
kind: quest
luteVersion: \"{lang}\"
uses: ../world.schema.yaml
title: Identify the Killer
---

<quest id=\"identifyKiller\" title=\"Identify the Killer\" start=\"true\">
  <objective id=\"gatherEvidence\" title=\"Log at least one clue\" done=\"run.cluesLogged >= 1\"/>
  <objective id=\"nameSuspect\" title=\"Focus on a suspect\" done=\"run.suspectFocus != 'none'\"/>
</quest>
"
            .replace("{lang}", lute_check::LUTE_LANG_VERSION),
        },
        File {
            rel: "mocks/playthrough.yaml",
            content: "\
# Trace mock (dsl 0.4.0 §4.3). `file:` names the document this mock previews,
# resolved against this file. Preview with:
#   lute trace scenes/interview.lute --mock mocks/playthrough.yaml
file: ../scenes/interview.lute
choose:
  interrogate: pressLedger
"
            .replace("{lang}", lute_check::LUTE_LANG_VERSION),
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
    let commands = if template == "beats" {
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
# `--target <prefix>.<member>`):
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
    if template == "beats" {
        println!("  lute test {d} --project {d}");
        println!(
            "  lute play {d} --script {}",
            dir.join("plays/first-day.play.yaml").display()
        );
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

/// Where a `lute new` document lands: the root of the project enclosing the
/// requested directory (so `lute new` from inside `scenes/` still writes
/// `<root>/scenes/…`), and the `defaults:` its manifest supplies — a
/// defaulted key is omitted from the new document's frontmatter (dsl 0.22.0
/// §13). Outside any project, the requested directory with no defaults.
struct Destination {
    root: PathBuf,
    defaults: MetaDefaults,
    in_project: bool,
}

impl Destination {
    fn find(dir: &Path) -> Self {
        let mut cur = Some(dir);
        while let Some(d) = cur {
            if d.join("lute.project.yaml").is_file() {
                let defaults = match lute_manifest::project::load_project(d) {
                    Ok(Some(config)) => config.defaults,
                    Ok(None) => MetaDefaults::default(),
                    Err(e) => {
                        eprintln!("lute new: warning: {e} — writing without its `defaults:`");
                        MetaDefaults::default()
                    }
                };
                return Destination {
                    root: d.to_path_buf(),
                    defaults,
                    in_project: true,
                };
            }
            cur = d.parent();
        }
        Destination {
            root: dir.to_path_buf(),
            defaults: MetaDefaults::default(),
            in_project: false,
        }
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
/// with `id:` = each path segment of the name as an identifier, joined by `.`
/// (`talk/mara-first` → `talk.maraFirst`). With `--on` it is a beat
/// answering that occasion (dsl 0.21.0 §3); the occasion and target are
/// validated against the project, and the file is removed again when they
/// do not resolve (exit `2`). Without `--on` it is a linear scene opening on
/// a `::bg`.
fn new_scene(name: &str, dest: &Destination, on: Option<&str>, target: Option<&str>) -> ExitCode {
    let path = dest.root.join("scenes").join(format!("{name}.lute"));
    let id: Vec<String> = name.split('/').map(|seg| to_ident(seg, "scene")).collect();
    let id = id.join(".");
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

/// `lute new quest <name>`.
///
/// Self-contained: declares its own `run.<ident>Progress` scalar (with a
/// `default:`, so the objective's `done` read is definitely assigned) and one
/// objective gated on it. The quest id / state segment is [`to_ident`] of the
/// name (a valid lower-camel identifier), while the file stem keeps the raw
/// name. The document id (dsl 0.19.0 §2.1) is `quest.<ident>` — namespaced
/// so it cannot collide with a scene id or a same-named `lute new lore`
/// bundle.
fn new_quest(name: &str, dest: &Destination) -> ExitCode {
    let path = dest.root.join("quests").join(format!("{name}.lute"));
    let ident = to_ident(name, "quest");
    let progress = format!("run.{ident}Progress");
    let content = format!(
        "{}\
# Self-contained progress counter — a scene can bump it with
# `::set{{ {progress} += 1 }}` to satisfy the objective below.
state:
  {progress}: {{ type: number, default: 0 }}
---

<quest id=\"{ident}\" title=\"{name}\" start=\"true\">
  <objective id=\"begin\" title=\"Make progress\" done=\"{progress} >= 1\"/>
</quest>
",
        dest.head("quest", &format!("quest.{ident}"), name)
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
/// document id (§2.1) is `lore.<ident>`, namespaced like `lute new quest`'s.
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
        dest.head("lore", &format!("lore.{ident}"), name)
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

/// Scaffold one new document into a project. See [`crate::Command::New`].
///
/// Kinds `scene`/`quest`/`lore`/`schema`; an unknown kind, or `--on` on a
/// non-scene, is a usage error (exit `2`). Refuses to overwrite an existing
/// target (exit `2`). Outside a project (no `lute.project.yaml` at or above
/// `dir`) it says so — and refuses `--on`, since no occasion is declared
/// there for the beat to answer (dsl 0.22.0 §13).
pub fn run_new(
    kind: &str,
    name: &str,
    dir: &Path,
    on: Option<&str>,
    target: Option<&str>,
) -> ExitCode {
    if !matches!(kind, "scene" | "quest" | "lore" | "schema") {
        eprintln!(
            "lute new: unknown kind `{kind}` (expected `scene`, `quest`, `lore`, or `schema`)"
        );
        eprintln!("usage: lute new <scene|quest|lore|schema> <name> [--dir <DIR>] [--on <OCCASION> [--target <TARGET>]]");
        return ExitCode::from(2);
    }
    if let (Some(_), false) = (on, kind == "scene") {
        eprintln!("lute new: `--on` makes a scene a beat; a {kind} takes no `--on`");
        return ExitCode::from(2);
    }
    let dest = Destination::find(dir);
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
        "quest" => new_quest(name, &dest),
        "lore" => new_lore(name, &dest),
        _ => new_schema(name, &dest),
    }
}
