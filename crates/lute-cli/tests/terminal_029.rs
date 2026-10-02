//! `terminal:`'s long form `{ when, persists }` through the built `lute`
//! binary: `persists: true` states that the ending outlives runs, so
//! `W-TERMINAL-PERSISTENT` is silent, `lute play` says the game is over for
//! good instead of offering a new run, and the IR carries
//! `terminalPersists`; on a condition over run state only it is
//! `E-META-VALUE`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lute-term029-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &Path, rel: &str, text: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

fn lute(dir: &Path, args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}

/// A realm whose crown, once taken, ends the game: `knock` crowns the
/// player (a user-tier write, or a run-tier one), `visit` is a later raise.
/// `terminal` is the schema's `terminal:` line.
fn realm(tag: &str, terminal: &str) -> PathBuf {
    let dir = temp_dir(tag);
    write(
        &dir,
        "lute.project.yaml",
        "pluginsDir: plugins/\ndefaultProfile: g\nprofiles:\n  g:\n    plugins: { realm: true }\n\
         defaults:\n  uses: [world.schema.yaml]\n",
    );
    write(
        &dir,
        "plugins/realm/plugin.yaml",
        "id: realm\nversion: 0.1.0\nkind: capability\ndepends: [ { id: lute.core, range: \"^0.0.1\" } ]\n\
         exports:\n  occasions: occasions/\n",
    );
    write(
        &dir,
        "plugins/realm/occasions/o.yaml",
        "occasions:\n  knock: { select: first }\n  visit: { select: first }\n",
    );
    write(
        &dir,
        "world.schema.yaml",
        &format!(
            "state:\n  user.crowned: {{ type: bool, default: false }}\n  \
             run.over: {{ type: bool, default: false }}\n{terminal}\n"
        ),
    );
    write(
        &dir,
        "scenes/crown.lute",
        "---\nkind: scene\nid: crown\non: knock\nonce: false\n---\n\n## Crown\n\n\
         ::set{user.crowned = true}\n::set{run.over = true}\n@narrator: The crown is yours.\n",
    );
    write(
        &dir,
        "scenes/town.lute",
        "---\nkind: scene\nid: town\non: visit\nonce: false\n---\n\n## Town\n\n@narrator: The town hums.\n",
    );
    dir
}

const PERSISTS: &str = "terminal: { when: \"user.crowned\", persists: true }";

/// The short form over state a new run keeps warns; the long form with
/// `persists: true` states the design and is clean.
#[test]
fn persists_silences_the_persistent_terminal_warning() {
    let dir = realm("warn", "terminal: \"user.crowned\"");
    let t = text(&lute(&dir, &["check-project", "."]));
    assert!(t.contains("W-TERMINAL-PERSISTENT"), "{t}");
    assert!(
        t.contains("say so: `terminal: { when: \"…\", persists: true }`"),
        "{t}"
    );

    let dir = realm("silent", PERSISTS);
    let out = lute(&dir, &["check-project", ".", "--deny-warnings"]);
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(!t.contains("W-TERMINAL-PERSISTENT"), "{t}");

    // `persists: false` is the short form: it warns.
    let dir = realm(
        "false",
        "terminal: { when: \"user.crowned\", persists: false }",
    );
    let t = text(&lute(&dir, &["check-project", "."]));
    assert!(t.contains("W-TERMINAL-PERSISTENT"), "{t}");
}

/// `persists: true` on a condition over run state only cannot persist: a
/// new run forgets it. One `E-META-VALUE`, at `persists`, naming why.
#[test]
fn persists_on_a_run_only_terminal_is_refused() {
    let dir = realm(
        "run-only",
        "terminal:\n  when: \"run.over\"\n  persists: true",
    );
    let out = lute(&dir, &["check-project", "."]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert_eq!(t.matches("[E-META-VALUE]").count(), 1, "{t}");
    assert!(
        t.contains(
            "`terminal: run.over` reads only state a new run forgets (`run.*`, the clock, \
             run-tier quests and relations), so its ending cannot outlive the run — drop \
             `persists: true`"
        ),
        "{t}"
    );
    // Anchored at the `persists` value (line 6 of the schema).
    assert!(t.contains("world.schema.yaml:6:13:"), "{t}");
}

/// The long form's keys are closed (a slip names the key it meant), and
/// `persists` is `true` or `false`. A faulty long form is the one report:
/// whether its ending may persist is not judged on top of it.
#[test]
fn the_long_form_keys_are_closed() {
    let dir = realm(
        "typo",
        "terminal: { when: \"user.crowned\", persist: true }",
    );
    let out = lute(&dir, &["check-project", "."]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(
        t.contains(
            "`terminal:` takes only `when` and `persists`, not `persist` — did you mean \
             `persists`?"
        ),
        "{t}"
    );
    assert!(!t.contains("W-TERMINAL-PERSISTENT"), "{t}");

    let dir = realm(
        "not-bool",
        "terminal: { when: \"user.crowned\", persists: 1 }",
    );
    let t = text(&lute(&dir, &["check-project", "."]));
    assert!(
        t.contains("`terminal:`'s `persists: 1` is not `true` or `false`"),
        "{t}"
    );
    assert!(!t.contains("W-TERMINAL-PERSISTENT"), "{t}");

    let dir = realm("no-when", "terminal: { persists: true }");
    let t = text(&lute(&dir, &["check-project", "."]));
    assert!(
        t.contains("`terminal:` names no condition — write `when: \"<condition>\"`"),
        "{t}"
    );
}

/// `lute play`: the step that ends a persisting game says it is over for
/// good, a new run says it did not reopen it, and a later raise is refused
/// without suggesting `newRun`.
#[test]
fn play_says_a_persisting_game_is_over_for_good() {
    let dir = realm("play", PERSISTS);
    write(
        &dir,
        "s.play.yaml",
        "steps:\n  - occasion: knock\n  - newRun: true\n  - occasion: visit\n",
    );
    let out = lute(&dir, &["play", ".", "--script", "s.play.yaml"]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(
        t.contains(
            "note: the game is over for good — `terminal: user.crowned` holds and persists \
             (`persists: true`), so the engine raises no occasion from here, in this run or any \
             later one"
        ),
        "{t}"
    );
    assert!(
        t.contains("note: the game is over for good — the new run does not reopen it"),
        "{t}"
    );
    assert!(
        t.contains(
            "step 3: E-OCCASION-GATE: the game is over — `terminal: user.crowned` holds, so the \
             engine raises no occasion (`visit` included); the ending persists (`persists: \
             true`), so the game is over for good and no new run reopens it"
        ),
        "{t}"
    );
    assert!(!t.contains("newRun: true"), "{t}");
    assert!(!t.contains("still holds after a new run"), "{t}");
}

/// The IR carries `terminalPersists: true` beside `terminal` — on the
/// artifact and in `project.index.json` — and omits it otherwise.
#[test]
fn the_ir_carries_terminal_persists() {
    let read = |dir: &Path, rel: &str| -> serde_json::Value {
        serde_json::from_str(&std::fs::read_to_string(dir.join(rel)).unwrap()).unwrap()
    };
    let dir = realm("ir", PERSISTS);
    let out = lute(&dir, &["compile", "--all", ".", "-o", "out"]);
    assert!(out.status.success(), "{}", text(&out));
    let index = read(&dir, "out/project.index.json");
    assert_eq!(index["terminal"]["cel"], "user.crowned", "{index:#}");
    assert_eq!(index["terminalPersists"], true, "{index:#}");
    let artifact = read(&dir, "out/scenes/crown.lute.json");
    assert_eq!(artifact["terminalPersists"], true, "{artifact:#}");
    let keys: Vec<&String> = artifact.as_object().unwrap().keys().collect();
    let at = |k: &str| keys.iter().position(|x| *x == k).unwrap();
    assert_eq!(at("terminalPersists"), at("terminal") + 1, "{keys:?}");

    let dir = realm("ir-short", "terminal: \"run.over\"");
    let out = lute(&dir, &["compile", "--all", ".", "-o", "out"]);
    assert!(out.status.success(), "{}", text(&out));
    let index = read(&dir, "out/project.index.json");
    assert_eq!(index["terminal"]["cel"], "run.over", "{index:#}");
    assert!(index.get("terminalPersists").is_none(), "{index:#}");
    let artifact = read(&dir, "out/scenes/crown.lute.json");
    assert!(artifact.get("terminalPersists").is_none(), "{artifact:#}");
}
