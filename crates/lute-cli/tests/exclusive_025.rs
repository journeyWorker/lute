//! dsl 0.25.0 §1 exclusive relations (LH F17): `excludes:` on a relation
//! declaration — the checker's verdicts (`check-project`), the declaration
//! errors, the IR, and the runtime violation `lute play` / `lute trace` report.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-excl-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &Path, rel: &str, text: &str) -> PathBuf {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, text).unwrap();
    path
}

fn run(args: &[&str]) -> Output {
    Command::new(BIN).args(args).output().unwrap()
}

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

/// The lighthouse's case: `fell(S)` needs `not seenAfter(S)`, and the author
/// declares the two exclusive. `truthful`/`liar` are derived from base facts
/// with no such guard, so only the runtime can catch both holding; `calm` /
/// `panicked` are base relations.
const SCHEMA: &str = "state:\n  run.day: { type: number, default: 0 }\n\
entities:\n  character: { members: [elias, maren, ada] }\n  place: { members: [gallery, landing] }\n\
relations:\n  at: { args: [character, place], tier: run }\n  damaged: { args: [place], tier: run }\n  \
seen: { args: [character], tier: run }\n  \
seenAfter: { args: [character], derive: true, excludes: [fell] }\n  \
fell: { args: [character], derive: true }\n  vouched: { args: [character], tier: run }\n  \
lied: { args: [character], tier: run }\n  \
truthful: { args: [character], derive: true, excludes: [liar] }\n  \
liar: { args: [character], derive: true }\n  \
calm: { args: [character], tier: run, excludes: [panicked] }\n  \
panicked: { args: [character], tier: run }\n\
rules:\n  - \"seenAfter(S) :- seen(S)\"\n  \
- \"fell(S) :- at(S, gallery), damaged(gallery), not seenAfter(S)\"\n  \
- \"truthful(W) :- vouched(W)\"\n  - \"liar(W) :- lied(W)\"\n";

fn scene(id: &str, on: &str, body: &str) -> String {
    format!("---\nkind: scene\nid: {id}\nuses: ../world.schema.yaml\non: {on}\nonce: false\n---\n\n## {id}\n\n{body}")
}

/// `setup` makes every base fact possible; `landing` asserts `lied(maren)`
/// on one branch only, then `vouched(maren)` on every route.
fn project(tag: &str) -> PathBuf {
    let dir = temp_dir(tag);
    write(
        &dir,
        "lute.project.yaml",
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n",
    );
    write(&dir, "world.schema.yaml", SCHEMA);
    write(
        &dir,
        "scenes/setup.lute",
        &scene(
            "setup",
            "close",
            "@narrator: The storm.\n::assert{at(elias, gallery)}\n::assert{damaged(gallery)}\n\
             ::assert{seen(elias)}\n::assert{seen(maren)}\n",
        ),
    );
    write(
        &dir,
        "scenes/landing.lute",
        &scene(
            "landing",
            "talk",
            "@narrator: Voices.\n\n<branch id=\"pick\">\n<choice id=\"lie\" label=\"Lie\">\n\
             ::assert{lied(maren)}\n</choice>\n<choice id=\"stay\" label=\"Stay\">\n\
             @narrator: Quiet.\n</choice>\n</branch>\n\n::assert{vouched(maren)}\n@narrator: Vouched.\n",
        ),
    );
    dir
}

fn diag_line<'t>(out: &'t str, code: &str, needle: &str) -> Option<&'t str> {
    out.lines()
        .find(|l| l.contains(&format!("[{code}]")) && l.contains(needle))
}

#[test]
fn check_project_reads_exclusive_relations() {
    let dir = project("check");
    write(
        &dir,
        "scenes/gallery.lute",
        &scene(
            "gallery",
            "visit",
            "@narrator{when=\"holds(seenAfter(elias)) && holds(fell(elias))\"}: Both at once.\n\
             @narrator{when=\"holds(seenAfter(maren)) && !holds(fell(maren))\"}: Maren walks on.\n\n\
             <branch id=\"look\">\n<choice id=\"saw\" label=\"Saw\" when=\"holds(seenAfter(elias))\">\n\
             @narrator{when=\"!holds(fell(elias))\"}: He did not fall.\n</choice>\n\
             <choice id=\"go\" label=\"Go\">\n@narrator: On.\n</choice>\n</branch>\n\n\
             ::assert{panicked(ada)}\n::assert{calm(ada)}\n",
        ),
    );
    let out = run(&["check-project", dir.to_str().unwrap()]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    // A guard needing both is dead, and says why.
    let dead = diag_line(
        &t,
        "E-ARM-DEAD",
        "holds(seenAfter(elias)) && holds(fell(elias))",
    )
    .unwrap_or_else(|| panic!("dead guard: {t}"));
    assert!(dead.contains("can never hold together"), "{dead}");
    // `!holds(fell(x))` follows from `holds(seenAfter(x))` — in the same
    // guard and under an enclosing one.
    let same = diag_line(
        &t,
        "W-FACT-GUARANTEED",
        "holds(seenAfter(maren)) && !holds(fell(maren))",
    )
    .unwrap_or_else(|| panic!("same-guard redundancy: {t}"));
    assert!(same.contains("`seenAfter` excludes `fell`"), "{same}");
    let nested = diag_line(&t, "W-FACT-GUARANTEED", "`!holds(fell(elias))`")
        .unwrap_or_else(|| panic!("enclosing-guard redundancy: {t}"));
    assert!(nested.contains("the enclosing guard"), "{nested}");
    // An assert of one while the other holds on every route.
    let excl = diag_line(&t, "E-FACT-EXCLUSIVE", "calm(ada)").unwrap_or_else(|| panic!("{t}"));
    assert!(excl.contains("panicked(ada)"), "{excl}");
    // Only possible (one branch asserts `lied`): never a static verdict.
    assert!(!t.contains("landing.lute:"), "{t}");
    // ER C4: `calm`/`panicked` and `truthful`/`liar` are read by nothing but
    // their exclusion — which is a read (the checker and runners check them).
    assert!(!t.contains("W-RELATION-UNREAD"), "{t}");
}

#[test]
fn an_excludes_entry_must_name_a_relation_of_the_same_kinds() {
    let dir = temp_dir("decl");
    write(
        &dir,
        "lute.project.yaml",
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n",
    );
    write(
        &dir,
        "a.lute",
        "---\nkind: scene\nid: a\nentities:\n  c: { members: [x] }\n  p: { members: [y] }\n\
         relations:\n  up: { args: [c], tier: run, excludes: [dwon, near, up] }\n  \
         down: { args: [c], tier: run }\n  near: { args: [c, p], tier: run }\n---\n\n## A\n\n@n: Hi.\n",
    );
    let out = run(&["check-project", dir.to_str().unwrap()]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    let decl: Vec<&str> = t
        .lines()
        .filter(|l| l.contains("[E-RELATION-DECL]"))
        .collect();
    assert_eq!(decl.len(), 3, "{t}");
    assert!(
        decl.iter()
            .any(|l| l.contains("`dwon` is not a declared relation — did you mean `down`?")),
        "{t}"
    );
    assert!(
        decl.iter()
            .any(|l| l.contains("`near` takes [c, p] but `up` takes [c]")),
        "{t}"
    );
    assert!(
        decl.iter().any(|l| l.contains("cannot exclude itself")),
        "{t}"
    );
}

#[test]
fn the_ir_carries_the_symmetric_closure() {
    let dir = project("ir");
    let out = run(&[
        "compile",
        dir.join("scenes/setup.lute").to_str().unwrap(),
        "--project",
        dir.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let art: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let excludes = |name: &str| {
        art["relations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["name"] == name)
            .map(|r| r.get("excludes").cloned())
            .unwrap_or_else(|| panic!("{name}: {art}"))
    };
    assert_eq!(excludes("seenAfter"), Some(serde_json::json!(["fell"])));
    assert_eq!(
        excludes("fell"),
        Some(serde_json::json!(["seenAfter"])),
        "symmetric"
    );
    assert_eq!(excludes("seen"), None, "absent when empty");
}

#[test]
fn play_reports_a_derived_violation_at_the_step_and_fails() {
    let dir = project("play");
    let script = write(
        &dir,
        "plays/lie.play.yaml",
        "steps:\n  - occasion: talk\n  - occasion: close\nchoose:\n  pick: lie\n",
    );
    let out = run(&[
        "play",
        dir.to_str().unwrap(),
        "--script",
        script.to_str().unwrap(),
    ]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    let at = t
        .find("  ✗ exclusive: liar(maren) and truthful(maren) both hold")
        .unwrap_or_else(|| panic!("{t}"));
    assert!(
        t[..at].contains("step 1"),
        "reported at the step that made it: {t}"
    );
    assert!(!t.contains("step 2 ·"), "the play halts there: {t}");

    // The other branch never makes both hold.
    let ok = write(
        &dir,
        "plays/stay.play.yaml",
        "steps:\n  - occasion: talk\n  - occasion: close\nchoose:\n  pick: stay\n",
    );
    let out = run(&[
        "play",
        dir.to_str().unwrap(),
        "--script",
        ok.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(!text(&out).contains("exclusive"), "{}", text(&out));
}

#[test]
fn trace_refuses_at_the_write_that_makes_both_hold() {
    let dir = project("trace");
    let scene = dir.join("scenes/landing.lute");
    let out = run(&[
        "trace",
        scene.to_str().unwrap(),
        "--project",
        dir.to_str().unwrap(),
        "--choose",
        "pick=lie",
    ]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(
        t.contains("✗ exclusive: liar(maren) and truthful(maren) both hold"),
        "{t}"
    );
    assert!(t.contains("E-FACT-EXCLUSIVE"), "{t}");
    assert!(!t.contains("Vouched."), "the walk stops at the write: {t}");

    let out = run(&[
        "trace",
        scene.to_str().unwrap(),
        "--project",
        dir.to_str().unwrap(),
        "--choose",
        "pick=stay",
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
}

/// LH N16: facts a mock or a play script seeds are checked before the walk.
#[test]
fn seeded_facts_that_break_an_exclusion_are_refused() {
    let dir = project("seeded");
    let scene = dir.join("scenes/setup.lute");
    let out = run(&[
        "trace",
        scene.to_str().unwrap(),
        "--project",
        dir.to_str().unwrap(),
        "--fact",
        "calm(ada)",
        "--fact",
        "panicked(ada)",
    ]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(
        t.contains("✗ exclusive: calm(ada) and panicked(ada) both hold"),
        "{t}"
    );
    assert!(t.contains("[E-FACT-EXCLUSIVE] the seeded facts"), "{t}");
    assert!(!t.contains("The storm."), "nothing is walked: {t}");

    // A scenario test's `facts:` — through a derivation (`lied` → `liar`).
    write(
        &dir,
        "tests/seeded.test.yaml",
        "file: ../scenes/setup.lute\nfacts: [\"lied(maren)\", \"vouched(maren)\"]\n",
    );
    let out = run(&[
        "test",
        dir.join("tests").to_str().unwrap(),
        "--project",
        dir.to_str().unwrap(),
    ]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(
        t.contains("FAIL") && t.contains("liar(maren) and truthful(maren) both hold"),
        "{t}"
    );

    // A play script's top-level `facts:` halts before step 1.
    let script = write(
        &dir,
        "plays/seeded.play.yaml",
        "facts: [\"calm(ada)\", \"panicked(ada)\"]\nsteps:\n  - occasion: close\n",
    );
    let out = run(&[
        "play",
        dir.to_str().unwrap(),
        "--script",
        script.to_str().unwrap(),
    ]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(
        t.contains("seeded world holds exclusive relations together before step 1"),
        "{t}"
    );
    assert!(!t.contains("The storm."), "{t}");
}

/// ER C3: an overlap a later write of the same presentation undoes still
/// halts `lute play` at the write, as it refuses `lute trace`.
#[test]
fn play_halts_at_a_transient_overlap() {
    let dir = project("transient");
    write(
        &dir,
        "scenes/turn.lute",
        &scene("turn", "visit", "::assert{calm(ada)}\n@narrator: For a moment.\n::retract{panicked(ada)}\n@narrator: After.\n"),
    );
    let script = write(
        &dir,
        "plays/turn.play.yaml",
        "facts: [\"panicked(ada)\"]\nsteps:\n  - occasion: visit\n",
    );
    let out = run(&[
        "play",
        dir.to_str().unwrap(),
        "--script",
        script.to_str().unwrap(),
    ]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(
        t.contains("  ✗ exclusive: calm(ada) and panicked(ada) both hold"),
        "{t}"
    );
    assert!(!t.contains("For a moment."), "halted at the write: {t}");
}

/// crown M1: a guard's `holds(F)` over a `reserved: true` relation holds
/// inside the region it guards, for exclusion like any other fact.
#[test]
fn a_reserved_fact_a_guard_requires_excludes_its_partner() {
    let dir = temp_dir("reserved");
    write(
        &dir,
        "lute.project.yaml",
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n",
    );
    write(
        &dir,
        "a.lute",
        "---\nkind: scene\nid: a\nentities:\n  foe: { members: [regent] }\n\
         relations:\n  dead: { args: [foe], reserved: true }\n  \
         alive: { args: [foe], tier: run, excludes: [dead] }\n---\n\n## A\n\n\
         <branch id=\"b\">\n<choice id=\"c\" label=\"C\" when=\"holds(dead(regent))\">\n\
         @n{when=\"!holds(alive(regent))\"}: Gone.\n::assert{alive(regent)}\n</choice>\n\
         <choice id=\"d\" label=\"D\">\n@n: On.\n</choice>\n</branch>\n",
    );
    let out = run(&["check-project", dir.to_str().unwrap()]);
    let t = text(&out);
    assert!(
        diag_line(&t, "W-FACT-GUARANTEED", "`!holds(alive(regent))`").is_some(),
        "{t}"
    );
    let e = diag_line(&t, "E-FACT-EXCLUSIVE", "alive(regent)").unwrap_or_else(|| panic!("{t}"));
    assert!(e.contains("the enclosing guard"), "{e}");
}

/// LH N17: a rule that derives one relation from its exclusive partner.
#[test]
fn a_rule_deriving_from_its_exclusive_partner_is_an_error() {
    let dir = temp_dir("rule");
    write(
        &dir,
        "lute.project.yaml",
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n",
    );
    write(
        &dir,
        "a.lute",
        "---\nkind: scene\nid: a\nentities:\n  item: { members: [lamp] }\n  c: { members: [x] }\n\
         relations:\n  lit: { args: [item], tier: run }\n  dark: { args: [item], derive: true, excludes: [lit] }\n  \
         hid: { args: [c], tier: run }\n  seen: { args: [c], derive: true, excludes: [hid] }\n\
         rules:\n  - \"dark(X) :- lit(X)\"\n  - \"seen(X) :- c(X), not hid(X)\"\n---\n\n## A\n\n\
         @n{when=\"holds(dark(lamp)) || holds(seen(x))\"}: Hm.\n::assert{lit(lamp)}\n::assert{hid(x)}\n",
    );
    let out = run(&["check-project", dir.to_str().unwrap()]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    let rules: Vec<&str> = t
        .lines()
        .filter(|l| l.contains("[E-RULE-EXCLUSIVE]"))
        .collect();
    assert_eq!(
        rules.len(),
        1,
        "a negated premise is the exclusion's own shape: {t}"
    );
    assert!(rules[0].contains("`dark(X) :- lit(X)`"), "{t}");
}

/// LH N18: a stale stamp inherited from `defaults:` is one report, at the
/// manifest's line.
#[test]
fn an_inherited_stale_stamp_is_reported_once_at_the_manifest() {
    let dir = temp_dir("stale");
    write(
        &dir,
        "lute.project.yaml",
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\ndefaults:\n  luteVersion: \"0.23.1\"\n",
    );
    write(
        &dir,
        "a.lute",
        "---\nkind: scene\nid: a\n---\n\n## A\n\n@n: Hi.\n",
    );
    write(
        &dir,
        "b.lute",
        "---\nkind: scene\nid: b\n---\n\n## B\n\n@n: Hi.\n",
    );
    let out = run(&["check-project", dir.to_str().unwrap()]);
    let t = text(&out);
    let stale: Vec<&str> = t
        .lines()
        .filter(|l| l.contains("[W-LUTE-VERSION-STALE]"))
        .collect();
    assert_eq!(stale.len(), 1, "{t}");
    assert!(stale[0].contains("lute.project.yaml:6:3"), "{t}");
    assert!(stale[0].contains("(2 documents)"), "{t}");
}
