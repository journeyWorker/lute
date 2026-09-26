//! 0.27 prerelease OT-F-3 / G-6: every value a play or a test writes to or
//! expects of a `{ domain: K }` / enum state path, and every occasion payload
//! value typed `{ domain: K }`, is one of the domain's members — a usage
//! error with a did-you-mean at the line it is written on, never a seed the
//! walk runs with or an expectation that holds (or misses) vacuously. A
//! payload field typed by an undeclared domain is `E-DOMAIN-UNKNOWN` at the
//! plugin's line.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-mb027-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &Path, rel: &str, text: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, text).unwrap();
}

fn run(dir: &Path, args: &[&str]) -> (Option<i32>, String) {
    let o: Output = Command::new(BIN)
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    );
    (o.status.code(), text)
}

/// The occasion `gift` carries a `to` payload typed `{ domain: route }`.
const OCCASIONS: &str =
    "occasions:\n  hubVisit: {}\n  gift: { payload: { to: { domain: route } } }\n";

/// `run.route` is typed by the enum `route` (ren, mika), `run.mood` by an
/// inline enum; `hub.idle` answers `hubVisit`, `g.gift` answers `gift`.
fn project(tag: &str) -> PathBuf {
    let dir = temp_dir(tag);
    write(
        &dir,
        "lute.project.yaml",
        "pluginsDir: plugins/\ndefaultProfile: g\nprofiles:\n  g:\n    plugins: { g.x: true }\n",
    );
    write(
        &dir,
        "plugins/g.x/plugin.yaml",
        "id: g.x\nversion: 0.1.0\nkind: capability\ndepends: [ { id: lute.core, range: \"^0.0.1\" } ]\n\
         exports:\n  occasions: occasions/\n",
    );
    write(&dir, "plugins/g.x/occasions/o.yaml", OCCASIONS);
    write(
        &dir,
        "world.schema.yaml",
        "enums:\n  route: { members: [ren, mika] }\n\
         state:\n  run.route: { type: { domain: route }, default: ren }\n  \
         run.mood: { type: { enum: [calm, tense] }, default: calm }\n",
    );
    write(
        &dir,
        "scenes/hub.lute",
        "---\nkind: scene\nid: hub.idle\nuses: ../world.schema.yaml\non: hubVisit\nonce: false\n---\n\n\
         ## Hub\n\n@narrator: Quiet.\n",
    );
    write(
        &dir,
        "lore/g.lute",
        "---\nkind: lore\nid: g\nuses: ../world.schema.yaml\n---\n\n\
         <beat id=\"gift\" on=\"gift\" once=\"false\" when=\"occasion.payload.to == 'ren'\">\n  \
         @narrator: A gift.\n</beat>\n",
    );
    let (code, t) = run(&dir, &["check-project", "."]);
    assert_eq!(code, Some(0), "{t}");
    dir
}

fn play(dir: &Path, script: &str) -> (Option<i32>, String) {
    write(dir, "s.play.yaml", script);
    run(dir, &["play", ".", "--script", "s.play.yaml"])
}

/// Each non-member a play writes or expects is refused before anything
/// plays, at its line, with the members and the nearest one. Without the
/// check the seed and the `engine:` write were taken as written (exit 0) and
/// the expectation was judged after the play (a miss, exit 1).
#[test]
fn a_play_refuses_a_non_member_it_writes_or_expects() {
    let dir = project("play");
    for (script, at, why) in [
        (
            "state: { run.route: rne }\nsteps:\n  - occasion: hubVisit\n",
            "s.play.yaml:1:",
            "`state.run.route` does not take `rne`: `rne` is not a member of `route` (ren, mika) \
             — did you mean `ren`?",
        ),
        (
            "steps:\n  - occasion: hubVisit\n  - engine: { state: { run.route: rne } }\n",
            "s.play.yaml:3:",
            "step 2: `engine.state.run.route` does not take `rne`: `rne` is not a member of \
             `route` (ren, mika) — did you mean `ren`?",
        ),
        (
            "steps:\n  - occasion: hubVisit\n    expect: { state: { run.route: rne } }\n",
            "s.play.yaml:3:",
            "step 1: `expect.state.run.route: rne` can never hold: `rne` is not a member of \
             `route` (ren, mika) — did you mean `ren`?",
        ),
        (
            "steps:\n  - occasion: hubVisit\nexpect:\n  state: { run.mood: tnse }\n",
            "s.play.yaml:4:",
            "end of play: `expect.state.run.mood: tnse` can never hold: `tnse` is not a member \
             of `run.mood` (calm, tense) — did you mean `tense`?",
        ),
        (
            "steps:\n  - occasion: gift\n    payload: { to: rne }\n",
            "s.play.yaml:3:",
            "step 1: `payload.to: rne` — `rne` is not a member of `route` (ren, mika) — did you \
             mean `ren`?",
        ),
    ] {
        let (code, t) = play(&dir, script);
        assert_eq!(code, Some(2), "{script}\n{t}");
        let line = t
            .lines()
            .find(|l| l.contains(why))
            .unwrap_or_else(|| panic!("no `{why}` for\n{script}\n{t}"));
        assert!(line.contains(at), "{line}");
    }
    // A member plays.
    let (code, t) = play(
        &dir,
        "steps:\n  - occasion: gift\n    payload: { to: ren }\n",
    );
    assert_eq!(code, Some(0), "{t}");
    assert!(t.contains("A gift."), "{t}");
}

/// A test's `state:` seed outside the domain refuses the trace, located at
/// the seed's key in the test file — not at the traced document's `0:0` — and
/// an `expect.state` value outside it can never hold, so it is refused too,
/// not reported as a miss.
#[test]
fn a_test_refuses_a_non_member_seed_or_expectation_at_its_line() {
    let dir = project("test");
    write(
        &dir,
        "tests/seed.test.yaml",
        "file: ../scenes/hub.lute\nstate:\n  run.mood: calm\n  run.route: rne\n\
         expect:\n  transcriptContains: [\"Quiet.\"]\n",
    );
    write(
        &dir,
        "tests/expect.test.yaml",
        "file: ../scenes/hub.lute\nexpect:\n  transcriptContains: [\"Quiet.\"]\n  \
         state: { run.route: rne }\n",
    );
    write(
        &dir,
        "tests/good.test.yaml",
        "file: ../scenes/hub.lute\nstate: { run.route: mika }\n\
         expect:\n  state: { run.route: mika }\n",
    );
    let (code, t) = run(&dir, &["test", "tests", "--project", "."]);
    assert_eq!(code, Some(1), "{t}");
    assert!(t.contains("1 passed, 2 failed"), "{t}");
    for want in [
        "tests/seed.test.yaml:4:3: error [E-TRACE-MOCK-TYPE] `state: { run.route: rne }` is not \
         compatible with `run.route`'s declared type: `rne` is not a member of `route` (ren, mika) \
         — did you mean `ren`?",
        "tests/expect.test.yaml:4:12: error [E-TRACE-MOCK-TYPE] `expect.state.run.route: rne` can \
         never hold: `rne` is not a member of `route` (ren, mika) — did you mean `ren`?",
    ] {
        assert!(t.contains(want), "missing `{want}` in:\n{t}");
    }
    assert!(!t.contains("hub.lute:0:0"), "{t}");
}

/// G-6: a payload field typed by a domain nobody declares is refused at the
/// plugin's line, with the nearest declared one.
#[test]
fn a_payload_field_typed_by_an_unknown_domain_is_refused_at_the_plugin() {
    let dir = project("unknown");
    write(
        &dir,
        "plugins/g.x/occasions/o.yaml",
        &OCCASIONS.replace("domain: route", "domain: rotue"),
    );
    let (code, t) = run(&dir, &["check-project", "."]);
    assert_eq!(code, Some(1), "{t}");
    let line = t
        .lines()
        .find(|l| l.contains("[E-DOMAIN-UNKNOWN]"))
        .unwrap_or_else(|| panic!("{t}"));
    assert!(line.contains("plugins/g.x/occasions/o.yaml:3:"), "{line}");
    assert!(
        line.contains("payload field `to` is typed `{ domain: rotue }`")
            && line.contains("did you mean `route`?"),
        "{line}"
    );
}
