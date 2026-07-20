//! `lute init` / `lute new` — project and document scaffolding.
//!
//! Every generated artifact is designed to pass the checker CLEAN: `lute init`
//! output survives `lute check-project <dir>`, and `lute new` output survives
//! `lute check <file>`. Frontmatter stamps `luteVersion: "0.7.0"` (the current
//! [`lute_check::LUTE_LANG_VERSION`]) so no `W-LUTE-VERSION-STALE` fires, and
//! every read state path carries a `default:` so single-file definite
//! assignment holds without a cross-scene envelope.

use std::fs;
use std::path::Path;
use std::process::ExitCode;

/// One scaffolded file: a path RELATIVE to the target directory and its
/// verbatim contents.
struct File {
    rel: &'static str,
    content: String,
}

/// The `lute.project.yaml` shared by every template — a core-only profile
/// (no plugins), so each document resolves against the built-in `lute.core`
/// snapshot.
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
            .to_string(),
        },
        File {
            rel: "scenes/opening.lute",
            content: "\
---
kind: scene
luteVersion: \"0.7.0\"
character: narrator
season: 1
episode: 1
title: Opening
uses: ../world.schema.yaml
---

## Opening

@narrator: Welcome to your new Lute project.
::set{ run.greeted = true }
@narrator{when=\"run.greeted\"}: Edit this scene, then run `lute check-project`.
"
            .to_string(),
        },
        File {
            rel: "mocks/playthrough.yaml",
            content: "\
# Trace mock (dsl 0.4.0 §4.3) for scenes/opening.lute. Preview with:
#   lute trace scenes/opening.lute --mock mocks/playthrough.yaml
state:
  run.greeted: false
"
            .to_string(),
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
            .to_string(),
        },
        File {
            rel: "scenes/crime-scene.lute",
            content: "\
---
kind: scene
luteVersion: \"0.7.0\"
character: detective
season: 1
episode: 1
title: The Crime Scene
# Graph ROOT: no `after:`, so this scene is an unconditional entry point.
uses: ../world.schema.yaml
---

## The Study

@narrator: The victim's study, untouched since the coroner left.
::assert{ foundClue(ledger) }
::set{ run.cluesLogged += 1 }
@detective: A ledger, its balances scratched out in red ink.
@detective{mono when=\"holds(points(blake))\"}: One name is starting to surface.
"
            .to_string(),
        },
        File {
            rel: "scenes/interview.lute",
            content: "\
---
kind: scene
luteVersion: \"0.7.0\"
character: detective
season: 1
episode: 2
title: The Interview
# Sequenced AFTER the crime scene (canonical key `detective.s01ep01`).
after: 'visited(\"detective.s01ep01\")'
uses: ../world.schema.yaml
---

## The Interview Room

@narrator: Three chairs, one table, and the smell of cold coffee.

<hub id=\"interrogate\">
  <choice id=\"pressLedger\" label=\"Press them on the ledger\" when=\"holds(foundClue(ledger))\">
    @detective: These numbers were bled dry. Explain them.
    ::set{ run.suspectFocus = \"blake\" }
  </choice>
  <choice id=\"leave\" label=\"End the interview\" exit>
    @detective: We're done here. For now.
  </choice>
</hub>
"
            .to_string(),
        },
        File {
            rel: "quests/identify-killer.lute",
            content: "\
---
kind: quest
luteVersion: \"0.7.0\"
uses: ../world.schema.yaml
title: Identify the Killer
---

<quest id=\"identifyKiller\" title=\"Identify the Killer\" start=\"true\">
  <objective id=\"gatherEvidence\" title=\"Log at least one clue\" done=\"run.cluesLogged >= 1\"/>
  <objective id=\"nameSuspect\" title=\"Focus on a suspect\" done=\"run.suspectFocus != 'none'\"/>
</quest>
"
            .to_string(),
        },
        File {
            rel: "mocks/playthrough.yaml",
            content: "\
# Trace mock (dsl 0.4.0 §4.3) for scenes/interview.lute. Preview with:
#   lute trace scenes/interview.lute --mock mocks/playthrough.yaml
facts:
  - \"foundClue(ledger)\"
choose:
  interrogate: pressLedger
"
            .to_string(),
        },
        File {
            rel: "README.md",
            content: readme("investigation"),
        },
    ]
}

/// The generated project README with next-step commands.
fn readme(template: &str) -> String {
    let check = if template == "investigation" {
        "scenes/crime-scene.lute"
    } else {
        "scenes/opening.lute"
    };
    let trace = if template == "investigation" {
        "scenes/interview.lute"
    } else {
        "scenes/opening.lute"
    };
    format!(
        "\
# Lute project (`{template}` template)

Scaffolded by `lute init --template {template}`. Every file already passes the
checker.

## Next steps

```sh
# Validate the whole project (recursively):
lute check-project .

# Check one document:
lute check {check}

# Preview a scene against the trace mock:
lute trace {trace} --mock mocks/playthrough.yaml

# Report the scene graph / reachability:
lute scenario .

# Add more documents:
lute new scene <name>
lute new quest <name>
lute new schema <name>
```
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
        other => {
            eprintln!(
                "lute init: unknown template `{other}` (expected `minimal` or `investigation`)"
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

    println!(
        "Initialized `{template}` Lute project at {}",
        dir.display()
    );
    println!("  created {} file(s):", files.len());
    for file in &files {
        println!("    {}", dir.join(file.rel).display());
    }
    println!();
    println!("Next steps:");
    println!("  lute check-project {}", dir.display());
    println!("  lute scenario {}", dir.display());
    println!("  lute new scene <name> --dir {}", dir.display());
    ExitCode::SUCCESS
}

/// Turn an arbitrary document name into a valid lower-camel identifier for a
/// quest id / state path segment (dsl §9.4 forbids `-` in a path segment): the
/// name is split on every non-alphanumeric run, the first word lower-cased and
/// each subsequent word capitalized, then a leading digit is prefixed with `q`.
/// Empty input degrades to `quest`. Documented so `lute new`'s numbering /
/// naming rule is discoverable.
fn to_ident(name: &str) -> String {
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
        return "quest".to_string();
    }
    if out.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        out.insert(0, 'q');
    }
    out
}

/// Count existing `*.lute` files directly under `scenes_dir` — the basis for
/// the next scene's episode number. A missing directory counts as zero.
fn count_scenes(scenes_dir: &Path) -> usize {
    match fs::read_dir(scenes_dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path().extension().and_then(|x| x.to_str()) == Some("lute")
                    && e.file_type().map(|t| t.is_file()).unwrap_or(false)
            })
            .count(),
        Err(_) => 0,
    }
}

/// Write `content` to `path` (creating parent dirs), refusing to overwrite an
/// existing file (exit `2`). On success prints the created path and a caller
/// hint. Returns the exit code.
fn write_new(path: &Path, content: &str, hint: &str) -> ExitCode {
    if path.exists() {
        eprintln!(
            "lute new: `{}` already exists — refusing to overwrite",
            path.display()
        );
        return ExitCode::from(2);
    }
    if let Some(parent) = path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            eprintln!("lute new: cannot create `{}`: {e}", parent.display());
            return ExitCode::from(2);
        }
    }
    if let Err(e) = fs::write(path, content) {
        eprintln!("lute new: cannot write `{}`: {e}", path.display());
        return ExitCode::from(2);
    }
    println!("created {}", path.display());
    println!("  {hint}");
    ExitCode::SUCCESS
}

/// `lute new scene <name>`.
///
/// **Naming rule (documented in the generated file's comment):** `character`
/// is the given `name` verbatim; `season` is `1`; `episode` is `1 + <number of
/// existing scenes/*.lute>`, so the canonical key `{character}.s01ep{NN}`
/// stays unique against the scenes already present. The scene `uses:` the
/// project schema only when `<dir>/world.schema.yaml` exists (otherwise it is
/// self-contained with no state reads).
fn new_scene(name: &str, dir: &Path) -> ExitCode {
    let scenes_dir = dir.join("scenes");
    let path = scenes_dir.join(format!("{name}.lute"));
    let episode = count_scenes(&scenes_dir) + 1;
    let has_schema = dir.join("world.schema.yaml").exists();
    let uses_line = if has_schema {
        "uses: ../world.schema.yaml\n"
    } else {
        ""
    };
    let content = format!(
        "\
---
kind: scene
luteVersion: \"0.7.0\"
# Naming rule (`lute new scene`): `character` = the name you passed; `season` =
# 1; `episode` = 1 + the number of existing `scenes/*.lute` at creation time,
# so the canonical key `{name}.s01ep{episode:02}` is unique among the scenes
# present. Rename/renumber freely.
character: {name}
season: 1
episode: {episode}
title: {name}
{uses_line}---

## {name}

@{name}: Hello from the {name} scene. Replace this with your own lines.
"
    );
    write_new(
        &path,
        &content,
        &format!("check it with: lute check {}", path.display()),
    )
}

/// `lute new quest <name>`.
///
/// Self-contained: declares its own `run.<ident>Progress` scalar (with a
/// `default:`, so the objective's `done` read is definitely assigned) and one
/// objective gated on it. The quest id / state segment is [`to_ident`] of the
/// name (a valid lower-camel identifier), while the file stem keeps the raw
/// name.
fn new_quest(name: &str, dir: &Path) -> ExitCode {
    let path = dir.join("quests").join(format!("{name}.lute"));
    let ident = to_ident(name);
    let progress = format!("run.{ident}Progress");
    let content = format!(
        "\
---
kind: quest
luteVersion: \"0.7.0\"
title: {name}
# Self-contained progress counter — a scene can bump it with
# `::set{{ {progress} += 1 }}` to satisfy the objective below.
state:
  {progress}: {{ type: number, default: 0 }}
---

<quest id=\"{ident}\" title=\"{name}\" start=\"true\">
  <objective id=\"begin\" title=\"Make progress\" done=\"{progress} >= 1\"/>
</quest>
"
    );
    write_new(
        &path,
        &content,
        &format!("check it with: lute check {}", path.display()),
    )
}

/// `lute new schema <name>` — a `<name>.schema.yaml` skeleton at the project
/// root. Schema files are declaration maps (no `.lute` body), imported via
/// `uses:`; they are not `lute check`-able on their own.
fn new_schema(name: &str, dir: &Path) -> ExitCode {
    let path = dir.join(format!("{name}.schema.yaml"));
    let content = format!(
        "\
# {name} schema (dsl §9). A pure declaration map — no `---`/body. Import it
# from a scene or quest with `uses: ./{name}.schema.yaml`. Every path should
# carry a `default:` so reads are definitely assigned.
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
    write_new(
        &path,
        &content,
        &format!("import it with: uses: ./{name}.schema.yaml"),
    )
}

/// Scaffold one new document into an existing project. See [`crate::Command::New`].
///
/// Kinds `scene`/`quest`/`schema`; an unknown kind is a usage error (exit `2`).
/// Refuses to overwrite an existing target (exit `2`).
pub fn run_new(kind: &str, name: &str, dir: &Path) -> ExitCode {
    match kind {
        "scene" => new_scene(name, dir),
        "quest" => new_quest(name, dir),
        "schema" => new_schema(name, dir),
        other => {
            eprintln!("lute new: unknown kind `{other}` (expected `scene`, `quest`, or `schema`)");
            eprintln!("usage: lute new <scene|quest|schema> <name> [--dir <DIR>]");
            ExitCode::from(2)
        }
    }
}
