//! dsl 0.26.0 §6 (T3-9), through the built `lute` binary: a rule body's
//! `count(…) <op> n` / `countDistinct(…) <op> n` derives in `lute play`
//! ("5 of 8 badges"), the checker's project analyses read it (the counted
//! relation is read, the head is producible), a count over the head's own
//! recursion is `E-RULE-AGGREGATE-CYCLE`, and `lute scenario knowledge`
//! traces the counted facts under the premise.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-agg026-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &Path, rel: &str, text: &str) -> PathBuf {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, text).unwrap();
    p
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

const RULES: &str = "rules:\n  - \"open(earth) :- count(hasBadge(_)) >= 5\"\n  \
                     - \"traveled(P) :- listed(P), countDistinct(toured(P, T), T) >= 2\"\n";

/// Eight badges and three towns, asserted only by the `gym` scene; the earth
/// door opens on five badges (`count`), a person has travelled once they
/// toured two distinct towns (`countDistinct`, grouped by the bound `P`).
/// `hasBadge` and `toured` are read nowhere but inside those counts.
fn project(tag: &str, extra_rules: &str) -> PathBuf {
    let dir = temp_dir(tag);
    write(
        &dir,
        "lute.project.yaml",
        "pluginsDir: plugins/\ndefaultProfile: g\nprofiles:\n  g:\n    plugins: { g.occ: true }\n\
         defaults:\n  uses: [world.schema.yaml]\n",
    );
    write(
        &dir,
        "plugins/g.occ/plugin.yaml",
        "id: g.occ\nversion: 0.1.0\nkind: capability\ndepends: [ { id: lute.core, range: \"^0.0.1\" } ]\n\
         exports:\n  occasions: occasions/\n",
    );
    write(
        &dir,
        "plugins/g.occ/occasions/o.yaml",
        "occasions:\n  gym: {}\n  gate: { select: first }\n",
    );
    write(
        &dir,
        "world.schema.yaml",
        &format!(
            "entities:\n  badge: {{ members: [b1, b2, b3, b4, b5, b6, b7, b8] }}\n  \
             person: {{ members: [ann, bob] }}\n  town: {{ members: [t1, t2, t3] }}\n  \
             door: {{ members: [earth] }}\n\
             relations:\n  hasBadge: {{ args: [badge], tier: run }}\n  \
             toured: {{ args: [person, town], tier: run }}\n  listed: {{ args: [person] }}\n  \
             open: {{ args: [door], derive: true }}\n  traveled: {{ args: [person], derive: true }}\n\
             facts:\n  - \"listed(ann)\"\n  - \"listed(bob)\"\n{RULES}{extra_rules}"
        ),
    );
    let asserts: String = (1..=8)
        .map(|i| format!("::assert{{hasBadge(b{i})}}\n"))
        .chain(
            ["t1", "t2", "t3"]
                .iter()
                .map(|t| format!("::assert{{toured(ann, {t})}}\n")),
        )
        .chain(
            ["t1", "t2", "t3"]
                .iter()
                .map(|t| format!("::assert{{toured(bob, {t})}}\n")),
        )
        .collect();
    write(
        &dir,
        "scenes/gym.lute",
        &format!("---\nkind: scene\nid: gym\non: gym\nonce: false\n---\n\n## Gym\n\n@narrator: A badge.\n{asserts}"),
    );
    write(
        &dir,
        "scenes/door.lute",
        "---\nkind: scene\nid: door\non: gate\nwhen: \"holds(open(earth))\"\n---\n\n## Door\n\n\
         @narrator: The earth door opens.\n\
         <branch id=\"who\" prompt=\"Who?\">\n  \
         <choice id=\"ann\" label=\"Ann\" when=\"holds(traveled(ann))\">\n    @narrator: Ann.\n  </choice>\n  \
         <choice id=\"bob\" label=\"Bob\" when=\"holds(traveled(bob))\">\n    @narrator: Bob.\n  </choice>\n  \
         <choice id=\"none\" label=\"Nobody\">\n    @narrator: Nobody.\n  </choice>\n\
         </branch>\n",
    );
    write(
        &dir,
        "quests/road.lute",
        "---\nkind: quest\nid: road.quests\n---\n\n\
         <quest id=\"road\" title=\"Road\" start=\"true\" tier=\"run\">\n  \
         <objective id=\"door\" title=\"Open the door\" done=\"holds(open(earth))\"/>\n</quest>\n",
    );
    dir
}

fn play(dir: &Path, script: &str) -> Output {
    let s = write(dir, "s.play.yaml", script);
    run(&[
        "play",
        dir.to_str().unwrap(),
        "--script",
        s.to_str().unwrap(),
    ])
}

fn badges(n: usize) -> String {
    let facts: Vec<String> = (1..=n).map(|i| format!("\"hasBadge(b{i})\"")).collect();
    facts.join(", ")
}

#[test]
fn a_counted_relation_is_read_and_the_head_is_producible() {
    let dir = project("clean", "");
    let out = run(&["check-project", dir.to_str().unwrap()]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(0), "{t}");
    // Read only inside the counts: not W-RELATION-UNREAD. `open(earth)` is
    // producible: its objective is not E-OBJECTIVE-UNSATISFIABLE, its beat
    // not E-BEAT-UNREACHABLE.
    for code in [
        "W-RELATION-UNREAD",
        "E-OBJECTIVE-UNSATISFIABLE",
        "E-BEAT-UNREACHABLE",
    ] {
        assert!(!t.contains(code), "{code}: {t}");
    }
}

#[test]
fn play_derives_five_of_eight_badges_and_distinct_towns() {
    let dir = project("play", "");
    let five = play(
        &dir,
        &format!(
            "facts: [{}, \"toured(ann, t1)\", \"toured(ann, t2)\", \"toured(bob, t1)\"]\n\
             steps:\n  - occasion: gate\n    choose: {{ who: ann }}\n",
            badges(5)
        ),
    );
    let t = text(&five);
    assert_eq!(five.status.code(), Some(0), "{t}");
    assert!(t.contains("✓ door"), "five badges open the door: {t}");
    assert!(t.contains("quest road -> complete"), "{t}");
    // `bob` toured one town: `countDistinct` sees one value of `T`.
    assert!(t.contains("[ann] bob✗"), "{t}");

    let four = play(
        &dir,
        &format!("facts: [{}]\nsteps:\n  - occasion: gate\n", badges(4)),
    );
    let t = text(&four);
    assert_eq!(four.status.code(), Some(0), "{t}");
    assert!(
        t.contains("✗ door [scene, priority 0] — when: false"),
        "{t}"
    );
    assert!(!t.contains("quest road -> complete"), "{t}");

    // Two tuples of one town are one distinct town.
    let bob = play(
        &dir,
        &format!(
            "facts: [{}, \"toured(bob, t1)\"]\nsteps:\n  - occasion: gate\n    choose: {{ who: bob }}\n",
            badges(8)
        ),
    );
    let t = text(&bob);
    assert_eq!(bob.status.code(), Some(1), "{t}");
    assert!(
        t.contains("E-TRACE-CHOICE") && t.contains("holds(traveled(bob))"),
        "{t}"
    );
}

#[test]
fn a_count_inside_its_own_recursion_is_e_rule_aggregate_cycle() {
    let dir = project("cycle", "  - \"open(D) :- door(D), count(open(_)) >= 1\"\n");
    let out = run(&["check-project", dir.to_str().unwrap()]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(
        t.contains("error [E-RULE-AGGREGATE-CYCLE] a rule deriving `open` counts `open`"),
        "{t}"
    );
}

#[test]
fn knowledge_traces_the_counted_facts_under_the_premise() {
    let dir = project("knowledge", "");
    let out = run(&[
        "scenario",
        dir.to_str().unwrap(),
        "knowledge",
        "--for",
        "door",
    ]);
    let s = text(&out);
    assert_eq!(out.status.code(), Some(0), "{s}");
    assert!(
        s.contains(
            "        rule: open(earth) :- count(hasBadge(_)) >= 5\n\
             \x20         count(hasBadge(_)) >= 5 — counts:\n\
             \x20           hasBadge(_) — asserted by scene `gym` (scenes/gym.lute)\n"
        ),
        "{s}"
    );
    // The bound group variable is substituted; the counted one stays.
    assert!(
        s.contains(
            "          countDistinct(toured(bob, T), T) >= 2 — counts:\n\
             \x20           toured(bob, _) — asserted by scene `gym` (scenes/gym.lute)\n"
        ),
        "{s}"
    );
}
