//! dsl 0.27.0 §4 (T2-11), through the built `lute` binary: a directive's
//! declared fact effects (`effects: { asserts: ["holding(@item)"] }`) are
//! applied by `lute play` and `lute test`, drawn by `lute scenario --facts`,
//! and known to the checker; a lore entry may call an effect-only directive
//! (first read only). The project is modelled on round-5 hollow-ward.

use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-facts027-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &Path, rel: &str, text: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

fn run(dir: &Path, args: &[&str]) -> (Option<i32>, String) {
    let o = Command::new(BIN)
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    (
        o.status.code(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        ),
    )
}

/// `holding` is content-asserted only through `::give` (not reserved), so
/// the checker must see the directive as its producer.
fn project(tag: &str) -> PathBuf {
    let dir = temp_dir(tag);
    write(
        &dir,
        "lute.project.yaml",
        "pluginsDir: plugins/\ndefaultProfile: g\nprofiles:\n  g:\n    plugins: { ward: true }\n\
         defaults:\n  uses: [world.schema.yaml]\n",
    );
    write(
        &dir,
        "world.schema.yaml",
        "state:\n  run.sanity: { type: number, default: 10, owner: engine }\n\
         entities:\n  item: { members: [brassKey, lantern] }\n  room: { members: [lobby, office] }\n\
         relations:\n  holding: { args: [item], tier: run }\n  canEnter: { args: [room], derive: true }\n\
         rules:\n  - \"canEnter(office) :- holding(brassKey)\"\n",
    );
    write(
        &dir,
        "plugins/ward/plugin.yaml",
        "id: ward\nversion: 0.1.0\nkind: capability\ndepends: [ { id: lute.core, range: \"^0.0.1\" } ]\n\
         exports:\n  occasions: occasions/\n  directives: directives/\n",
    );
    write(
        &dir,
        "plugins/ward/occasions/o.yaml",
        "occasions:\n  search: { select: first }\n  door: { select: first }\n",
    );
    write(
        &dir,
        "plugins/ward/directives/d.yaml",
        "directives:\n  - name: give\n    attrs:\n      - { name: item, required: true, type: { entity: item } }\n    \
         effects:\n      asserts: [\"holding(@item)\"]\n  - name: fright\n    attrs:\n      \
         - { name: amount, type: number, default: 1 }\n    effects:\n      writes:\n        \
         - { scope: run, path: [sanity], value: { op: decrement, by: { fromAttr: amount } } }\n",
    );
    write(
        &dir,
        "scenes/tub.lute",
        "---\nkind: scene\nid: tub\non: search\n---\n\n## Tub\n\n::give{item=\"brassKey\"}\n\
         @narrator: A brass key.\n",
    );
    write(
        &dir,
        "scenes/office.lute",
        "---\nkind: scene\nid: office\non: door\nwhen: \"holds(canEnter(office))\"\n---\n\n## Office\n\n\
         @narrator: The office door gives.\n",
    );
    write(
        &dir,
        "lore/notes.lute",
        "---\nkind: lore\nid: notes\n---\n\n<entry id=\"diary\" category=\"note\">\n::fright{amount=3}\n\
         @narrator: The diary's last page is wet. Sanity {{run.sanity}}.\n</entry>\n",
    );
    dir
}

#[test]
fn the_checker_knows_a_directive_asserts_its_declared_facts() {
    let dir = project("check");
    let (code, t) = run(&dir, &["check-project", "."]);
    assert_eq!(code, Some(0), "{t}");
    // The entry's `::fright` is admitted; the office gate is not dead for
    // want of a `holding(brassKey)` producer.
    for code in ["E-GRAMMAR-NOT-ADMITTED", "E-ARM-DEAD", "E-BEAT"] {
        assert!(!t.contains(code), "{code}: {t}");
    }
    // The one warning is the lookup entry's first-read-only write (0.28
    // covers every entry that can be read again).
    let warnings: Vec<&str> = t.lines().filter(|l| l.contains("W-")).collect();
    assert_eq!(warnings.len(), 1, "{t}");
    assert!(
        warnings[0].contains("[W-ENTRY-WRITE-REREAD] `<entry id=\"diary\">`"),
        "{t}"
    );
}

/// HW27-06: a directive an entry cannot call is refused with the 0.27 rule
/// (effect-only directives are admitted) and why this one is not.
#[test]
fn an_entry_directive_without_effects_is_refused_with_the_reason() {
    let dir = project("lure");
    write(
        &dir,
        "plugins/ward/directives/d.yaml",
        "directives:\n  - name: fright\n    attrs:\n      \
         - { name: amount, type: number, default: 1 }\n    effects:\n      writes:\n        \
         - { scope: run, path: [sanity], value: { op: decrement, by: { fromAttr: amount } } }\n  \
         - name: lure\n    attrs:\n      - { name: room, type: { entity: room } }\n",
    );
    write(
        &dir,
        "scenes/tub.lute",
        "---\nkind: scene\nid: tub\non: search\n---\n\n## Tub\n\n@narrator: Tub.\n",
    );
    write(
        &dir,
        "lore/notes.lute",
        "---\nkind: lore\nid: notes\n---\n\n<entry id=\"diary\">\n::fright{amount=3}\n\
         ::lure{room=\"lobby\"}\n@narrator: Wet.\n</entry>\n",
    );
    let (code, t) = run(&dir, &["check-project", "."]);
    assert_ne!(code, Some(0), "{t}");
    assert!(
        t.contains(
            "notes.lute:8:1: error [E-GRAMMAR-NOT-ADMITTED] `::lure` is not admitted in an entry"
        ) && t.contains("`::lure` declares no `effects:`"),
        "{t}"
    );
    assert_eq!(t.matches("E-GRAMMAR-NOT-ADMITTED").count(), 1, "{t}");
}

#[test]
fn a_directive_fact_on_an_undeclared_relation_is_refused_at_the_call() {
    let dir = project("undeclared");
    write(
        &dir,
        "plugins/ward/directives/d.yaml",
        "directives:\n  - name: give\n    attrs:\n      - { name: item, required: true, type: { entity: item } }\n    \
         effects:\n      asserts: [\"carrying(@item)\"]\n",
    );
    write(
        &dir,
        "lore/notes.lute",
        "---\nkind: lore\nid: notes\n---\n\n<entry id=\"diary\">\n@narrator: Wet.\n</entry>\n",
    );
    let (code, t) = run(&dir, &["check-project", "."]);
    assert_ne!(code, Some(0), "{t}");
    assert!(
        t.contains("`::give` asserts `carrying(brassKey)` (its declared `effects.asserts`)"),
        "{t}"
    );
}

#[test]
fn play_applies_the_declared_assert_and_the_gated_follow_up_opens() {
    let dir = project("play");
    write(
        &dir,
        "p.play.yaml",
        "steps:\n  - occasion: door\n  - occasion: search\n  - occasion: door\n\
         expect:\n  facts: [holding(brassKey)]\n",
    );
    let (code, t) = run(&dir, &["play", ".", "--script", "p.play.yaml"]);
    assert_eq!(code, Some(0), "{t}");
    assert!(t.contains("holding(brassKey)"), "{t}");
    assert!(t.contains("effect of ::give"), "{t}");
    // Before the pickup the door passes; after it the office scene plays.
    assert_eq!(t.matches("The office door gives.").count(), 1, "{t}");
}

#[test]
fn test_and_trace_apply_the_declared_assert() {
    let dir = project("test");
    write(
        &dir,
        "tests/tub.test.yaml",
        "file: ../scenes/tub.lute\nexpect:\n  facts: [holding(brassKey)]\n",
    );
    let (code, t) = run(&dir, &["test", "tests/tub.test.yaml", "--project", "."]);
    assert_eq!(code, Some(0), "{t}");
    let (code, t) = run(
        &dir,
        &["trace", "scenes/tub.lute", "--project", ".", "--json"],
    );
    assert_eq!(code, Some(0), "{t}");
    assert!(
        t.contains("\"kind\": \"assert\"") && t.contains("holding(brassKey)"),
        "{t}"
    );
}

#[test]
fn scenario_facts_draws_the_directive_to_the_gated_scene() {
    let dir = project("scenario");
    let (code, t) = run(&dir, &["scenario", ".", "--facts"]);
    assert_eq!(code, Some(0), "{t}");
    assert!(t.contains("holding(brassKey)"), "{t}");
    assert!(t.contains("canEnter(office)"), "{t}");
}

/// HW27-09: a gate reads too — `::give{item="brassKey"}` → `holding(brassKey)`
/// → `canEnter(office)` (a rule) → the `enter` occasion's `raisedWhen` for
/// `room.office`: the edge runs to the scene answering it, which has no
/// `when` of its own.
#[test]
fn scenario_facts_draws_the_directive_through_an_occasion_gate() {
    let dir = project("scenario-gate");
    write(
        &dir,
        "plugins/ward/occasions/enter.yaml",
        "occasions:\n  enter: { select: first, target: { prefix: room, entity: room }, \
         raisedWhen: \"holds(canEnter(occasion.target))\" }\n",
    );
    write(
        &dir,
        "scenes/study.lute",
        "---\nkind: scene\nid: study\non: enter\ntarget: room.office\n---\n\n## Study\n\n\
         @narrator: Papers everywhere.\n",
    );
    let (code, t) = run(&dir, &["scenario", ".", "--facts"]);
    assert_eq!(code, Some(0), "{t}");
    let edge = t
        .lines()
        .find(|l| l.contains("-> scene(study)"))
        .unwrap_or_default();
    assert!(
        edge.contains("scene(tub)")
            && edge.contains("holding(brassKey)")
            && edge.contains("canEnter(office)"),
        "{t}"
    );
}

#[test]
fn an_entry_applies_its_effect_directive_on_the_first_read_only() {
    let dir = project("entry");
    write(
        &dir,
        "tests/diary.test.yaml",
        "file: ../lore/notes.lute\nentries: [diary, diary]\nexpect:\n  state: { run.sanity: 7 }\n",
    );
    let (code, t) = run(&dir, &["test", "tests/diary.test.yaml", "--project", "."]);
    assert_eq!(code, Some(0), "{t}");
}

/// HW27-12: an entry that can be read again in a run applies an effect
/// directive's `writes` on the first read in a run only, like its own
/// `::set` — the same `W-ENTRY-WRITE-REREAD`, for an entry beat without
/// `once` and (0.28) a lookup entry alike. An `asserts`-only effect
/// (idempotent) is not a write that could be lost.
#[test]
fn an_entry_beats_effect_write_warns_it_applies_on_the_first_read_only() {
    let dir = project("reread");
    write(
        &dir,
        "lore/tape.lute",
        "---\nkind: lore\nid: tape\n---\n\n<entry id=\"tape\" on=\"search\" category=\"note\">\n\
         ::fright{amount=2}\n@narrator: The tape hisses.\n</entry>\n\n\
         <entry id=\"key\" on=\"door\" category=\"note\">\n::give{item=\"lantern\"}\n\
         @narrator: A lantern on the hook.\n</entry>\n",
    );
    let (_, t) = run(&dir, &["check-project", "."]);
    let hits: Vec<&str> = t
        .lines()
        .filter(|l| l.contains("[W-ENTRY-WRITE-REREAD]"))
        .collect();
    assert_eq!(hits.len(), 2, "{t}");
    assert!(
        hits.iter().any(|h| h.contains("notes.lute:7:")
            && h.contains("`<entry id=\"diary\">` answers no occasion")
            && h.contains("its `::fright` applies on the first read in a run only")),
        "{t}"
    );
    assert!(
        hits.iter().any(|h| h.contains("tape.lute:7:")
            && h.contains("`<entry id=\"tape\">` has no `once`")
            && h.contains("its `::fright` applies on the first read in a run only")),
        "{t}"
    );
    // The `::give` entry only asserts: nothing to lose on a reread.
    assert!(!hits.iter().any(|h| h.contains("id=\"key\"")), "{t}");
}

#[test]
fn a_by_from_attr_write_is_applied_by_test_and_trace_too() {
    let dir = project("by-attr");
    write(
        &dir,
        "scenes/scare.lute",
        "---\nkind: scene\nid: scare\non: search\n---\n\n## Scare\n\n::fright{amount=3}\n\
         @narrator: Sanity {{run.sanity}}.\n",
    );
    write(
        &dir,
        "tests/scare.test.yaml",
        "file: ../scenes/scare.lute\nexpect:\n  state: { run.sanity: 7 }\n",
    );
    let (code, t) = run(&dir, &["test", "tests/scare.test.yaml", "--project", "."]);
    assert_eq!(code, Some(0), "{t}");
}

#[test]
fn an_occasion_step_may_carry_engine_facts() {
    let dir = project("step-engine");
    write(
        &dir,
        "p.play.yaml",
        "steps:\n  - engine: { facts: [holding(brassKey)] }\n    occasion: door\n",
    );
    let (code, t) = run(&dir, &["play", ".", "--script", "p.play.yaml"]);
    assert_eq!(code, Some(0), "{t}");
    assert!(t.contains("The office door gives."), "{t}");
}
