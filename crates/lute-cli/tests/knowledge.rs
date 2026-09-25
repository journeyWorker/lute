//! dsl 0.24.0 T3-1 / T3-2, through the built `lute` binary: `lute scenario
//! knowledge` over every guard slot (grouped per document, `--for
//! <scene>#<branch>.<choice>`), constants propagated into negated premises,
//! the negation wording ("cannot be defeated", "defeated when …"), a derived
//! tree printed once; `lute play --explain` provenance; `lute lore`'s entry
//! `when` and Derived section; `lute scenario envelope`'s producer wording.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-knowledge-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &Path, rel: &str, text: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, text).unwrap();
}

fn lute(args: &[&str]) -> Output {
    Command::new(BIN).args(args).output().unwrap()
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).to_string()
}

fn ok(args: &[&str]) -> String {
    let out = lute(args);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{args:?}\nstdout:\n{}\nstderr:\n{}",
        stdout(&out),
        String::from_utf8_lossy(&out.stderr)
    );
    stdout(&out)
}

/// A small inquiry: Hollis saw Tobias, Maren saw Ada but lied. An alibi
/// holds unless its witness is a liar; only a recorded lie makes a liar.
fn inquiry(tag: &str) -> PathBuf {
    let d = temp_dir(tag);
    write(
        &d,
        "lute.project.yaml",
        "pluginsDir: plugins/\ndefaultProfile: m\nprofiles:\n  m:\n    plugins: { m.occ: true }\n\
         defaults:\n  uses: [world.schema.yaml]\n",
    );
    write(
        &d,
        "plugins/m.occ/plugin.yaml",
        "id: m.occ\nversion: 0.1.0\nkind: capability\ndepends: [ { id: lute.core, range: \"^0.0.1\" } ]\n\
         exports:\n  occasions: occasions/\n",
    );
    write(
        &d,
        "plugins/m.occ/occasions/occ.yaml",
        "occasions:\n  look: { select: all }\n  report: {}\n",
    );
    write(
        &d,
        "world.schema.yaml",
        "state:\n  run.day: { type: number, default: 1 }\n\
         entities:\n  person: { members: [hollis, maren, tobias, ada] }\n\
         relations:\n  saw: { args: [person, person] }\n  lied: { args: [person] }\n\
         \x20 departed: { args: [person] }\n  present: { args: [person], reserved: true }\n\
         \x20 liar: { args: [person], derive: true }\n  alibied: { args: [person], derive: true }\n\
         facts:\n  - \"present(hollis)\"\n\
         rules:\n  - \"alibied(S) :- saw(W, S), not liar(W)\"\n  - \"liar(W) :- lied(W)\"\n",
    );
    write(
        &d,
        "lore/evidence.lute",
        "---\nkind: lore\nid: inquiry.evidence\n---\n\n\
         <entry id=\"hollisSaw\" on=\"look\" title=\"Hollis\">\n  @narrator: Hollis saw Tobias.\n  ::assert{saw(hollis, tobias)}\n</entry>\n\n\
         <entry id=\"marenSaw\" on=\"look\" title=\"Maren\">\n  @narrator: Maren saw Ada, and lied.\n  ::assert{saw(maren, ada)}\n  ::assert{lied(maren)}\n</entry>\n\n\
         <entry id=\"nbTobias\" on=\"look\" title=\"Alibi\" when=\"holds(alibied(tobias))\">\n  @narrator: Tobias is clear.\n</entry>\n",
    );
    write(
        &d,
        "scenes/report.lute",
        "---\nkind: scene\nid: inquiry.report\non: report\n---\n\n## Report\n\n\
         <branch id=\"fate\" prompt=\"Who?\">\n  \
         <choice id=\"tobias\" label=\"Tobias\" when=\"holds(alibied(tobias))\">\n    @narrator: Tobias.\n  </choice>\n  \
         <choice id=\"ada\" label=\"Ada\" when=\"holds(alibied(ada))\">\n    @narrator: Ada.\n  </choice>\n  \
         <choice id=\"nobody\" label=\"Nobody\">\n    @narrator: Nobody.\n  </choice>\n\
         </branch>\n\n\
         @narrator{when=\"holds(alibied(tobias))\"}: Tobias walks free.\n\
         @tobias{when=\"!holds(departed(tobias))\"}: I am still here.\n",
    );
    write(
        &d,
        "quests/case.lute",
        "---\nkind: quest\nid: inquiry.case\n---\n\n\
         <quest id=\"case\" start=\"holds(saw(hollis, tobias))\" fail=\"holds(departed(tobias))\">\n  \
         <objective id=\"clear\" done=\"holds(alibied(tobias))\"/>\n</quest>\n",
    );
    d
}

#[test]
fn knowledge_covers_every_guard_slot_grouped_by_document() {
    let dir = inquiry("slots");
    let d = dir.to_str().unwrap();
    // Before 0.24 a scene with only choice and line guards was not a
    // fact-guarded node: `--for` exited 2.
    let s = ok(&["scenario", d, "knowledge", "--for", "inquiry.report"]);
    assert!(s.contains("\n  scenes/report.lute\n    choice `fate.tobias` (line 10)\n"), "{s}");
    assert!(s.contains("      when: holds(alibied(tobias))\n"), "{s}");
    assert!(s.contains("    choice `fate.ada` (line 13)\n"), "{s}");
    assert!(s.contains("    line `@narrator` (line 21)\n"), "{s}");
    assert!(s.contains("    line `@tobias` (line 22)\n"), "{s}");
    assert!(!s.contains("entry `nbTobias`"), "--for selects the scene's guards only: {s}");

    // One choice by `<scene>#<branch>.<choice>`.
    let one = ok(&["scenario", d, "knowledge", "--for", "inquiry.report#fate.ada"]);
    assert!(one.contains("choice `fate.ada`"), "{one}");
    assert!(!one.contains("choice `fate.tobias`") && !one.contains("line `@"), "{one}");

    // Quest `start`/`fail` are guard slots too.
    let q = ok(&["scenario", d, "knowledge", "--for", "quest:case"]);
    assert!(q.contains("\n  quests/case.lute\n    quest `case`\n"), "{q}");
    assert!(q.contains("      start: holds(saw(hollis, tobias))\n"), "{q}");
    assert!(q.contains("      fail: holds(departed(tobias))\n"), "{q}");
    assert!(q.contains("    objective `case.clear`\n"), "{q}");

    let miss = lute(&["scenario", d, "knowledge", "--for", "inquiry.report#fate.adda"]);
    assert_eq!(miss.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&miss.stderr).contains("did you mean `inquiry.report#fate.ada`?"),
        "{}",
        String::from_utf8_lossy(&miss.stderr)
    );
}

#[test]
fn knowledge_propagates_constants_into_negations_and_says_whether_they_can_be_defeated() {
    let dir = inquiry("negation");
    let d = dir.to_str().unwrap();
    let s = ok(&["scenario", d, "knowledge", "--for", "inquiry.report"]);
    // `W` is bound to `hollis` by the only `saw(_, tobias)`; nothing makes
    // Hollis a liar.
    assert!(
        s.contains(
            "not liar(hollis) — always holds (nothing can produce liar(hollis): its rule never \
             concludes it from what the project asserts) — cannot be defeated"
        ),
        "{s}"
    );
    assert!(!s.contains("not liar(_)"), "{s}");
    // Maren's lie defeats Ada's alibi, and the defeater names its source.
    assert!(s.contains("not liar(maren) — holds unless defeated\n"), "{s}");
    assert!(
        s.contains("defeated when liar(maren) is derived ⇐ lied(maren) [entry `marenSaw` (lore/evidence.lute)]"),
        "{s}"
    );
    // A guard-level `!holds(X)` nothing produces is the good case.
    assert!(
        s.contains(
            "not departed(tobias) — always holds (nothing produces departed(tobias): no assert, \
             seed fact, engine write or rule) — cannot be defeated"
        ),
        "{s}"
    );
    assert!(!s.contains("not departed(tobias) — NO PRODUCER"), "{s}");
}

#[test]
fn knowledge_prints_a_derived_tree_once_and_references_it() {
    let dir = inquiry("once");
    let d = dir.to_str().unwrap();
    let s = ok(&["scenario", d, "knowledge"]);
    // Before, every guard re-derived `alibied(tobias)` (4 guards read it).
    assert_eq!(s.matches("rule: alibied(S) :- saw(W, S), not liar(W)").count(), 2, "tobias and ada: {s}");
    assert!(
        s.contains("alibied(tobias) — derived by 1 rule — traced above under entry `nbTobias` in lore/evidence.lute"),
        "{s}"
    );
    let scene = ok(&["scenario", d, "knowledge", "--for", "inquiry.report"]);
    assert!(
        scene.contains("alibied(tobias) — derived by 1 rule — traced above under choice `fate.tobias` (line 10)\n"),
        "a later mention in the same document names the element only: {scene}"
    );
}

#[test]
fn play_explain_names_the_asserting_beat_and_step_and_expands_an_absent_negation() {
    let dir = inquiry("explain");
    write(
        &dir,
        "plays/p.play.yaml",
        "steps:\n  - { occasion: look, pick: hollisSaw }\n  - { occasion: look, pick: marenSaw }\n  \
         - engine: { facts: [\"saw(tobias, ada)\"] }\n",
    );
    let d = dir.to_str().unwrap();
    let script = dir.join("plays/p.play.yaml");
    let s = ok(&[
        "play",
        d,
        "--script",
        script.to_str().unwrap(),
        "--explain",
        "alibied(tobias)",
        "--explain",
        "alibied(ada)",
    ]);
    let tobias = s.split("explain alibied(tobias): holds\n").nth(1).unwrap_or_else(|| panic!("{s}"));
    assert!(tobias.contains("saw(hollis, tobias)  (asserted by entry `hollisSaw`, step 1)"), "{s}");
    assert!(tobias.contains("not liar(hollis)  (absent — no rule concludes it:)\n"), "{s}");
    assert!(tobias.contains("liar(W) :- lied(W)\n"), "{s}");
    assert!(tobias.contains("✗ lied(hollis)  (absent)"), "{s}");
    // Ada's alibi holds through the engine's witness, not Maren's.
    let ada = s.split("explain alibied(ada): holds\n").nth(1).unwrap_or_else(|| panic!("{s}"));
    assert!(ada.contains("saw(tobias, ada)  (asserted by engine step 3)"), "{s}");
}

#[test]
fn lore_shows_entry_when_and_the_derived_conclusions() {
    let dir = inquiry("lore");
    let d = dir.to_str().unwrap();
    let s = ok(&["lore", d]);
    assert!(
        s.contains("    nbTobias  \"Alibi\"  lore/evidence.lute\n      when: holds(alibied(tobias))\n"),
        "{s}"
    );
    let derived = s.split("\nDerived (").nth(1).unwrap_or_else(|| panic!("{s}"));
    assert!(
        derived.contains(
            "    alibied(tobias)\n      ⇐ saw(hollis, tobias) [entry `hollisSaw` (lore/evidence.lute)], not liar(hollis)\n"
        ),
        "{derived}"
    );
    assert!(derived.contains("      evidence: entry `hollisSaw` (lore/evidence.lute)\n"), "{derived}");
    let tobias = derived.split("alibied(tobias)").nth(1).unwrap();
    assert!(tobias.contains("gates: entry `nbTobias` (lore/evidence.lute)"), "{derived}");
    assert!(derived.contains("    liar(maren)\n      ⇐ lied(maren) [entry `marenSaw` (lore/evidence.lute)]\n"), "{derived}");

    let v: serde_json::Value = serde_json::from_str(&ok(&["lore", d, "--json"])).unwrap();
    let liar = v["derived"].as_array().unwrap().iter().find(|g| g["relation"] == "liar").unwrap();
    assert_eq!(liar["facts"][0]["fact"], "liar(maren)");
    assert_eq!(liar["facts"][0]["evidence"][0], "entry `marenSaw` (lore/evidence.lute)");
}

#[test]
fn envelope_names_producers_in_the_knowledge_words() {
    let dir = inquiry("envelope");
    let d = dir.to_str().unwrap();
    let s = ok(&["scenario", d, "envelope", "inquiry.report"]);
    assert!(
        s.contains("    - present/1 (producible) — seed facts present(hollis); reserved — the engine asserts it\n"),
        "{s}"
    );
    assert!(s.contains("    - liar/1 (producible) — derived by 1 rule\n"), "{s}");
}

/// Round-3 prerelease (CR N2, SU N6, ER N9): an entity-kind atom in a rule
/// body is a membership premise, not an "undeclared relation", and the
/// rule's `cel()` premise is listed with the member it reads.
#[test]
fn knowledge_reads_an_entity_kind_premise_as_membership_and_lists_cel_premises() {
    let d = temp_dir("kinds");
    write(
        &d,
        "lute.project.yaml",
        "pluginsDir: plugins/\ndefaultProfile: m\nprofiles:\n  m:\n    plugins: { m.occ: true }\n\
         defaults:\n  uses: [world.schema.yaml]\n",
    );
    write(
        &d,
        "plugins/m.occ/plugin.yaml",
        "id: m.occ\nversion: 0.1.0\nkind: capability\ndepends: [ { id: lute.core, range: \"^0.0.1\" } ]\n\
         exports:\n  occasions: occasions/\n",
    );
    write(&d, "plugins/m.occ/occasions/occ.yaml", "occasions:\n  tick: {}\n");
    write(
        &d,
        "world.schema.yaml",
        "state:\n  run.aff: { type: number, default: 0, per: suitor }\n  run.day: { type: number, default: 1 }\n\
         entities:\n  person: { members: [ines, sol, wren] }\n  suitor: { subsetOf: person, members: [sol, wren] }\n\
         relations:\n  ready: { args: [suitor], derive: true }\n  plain: { args: [person], derive: true }\n\
         rules:\n  - \"ready(P) :- suitor(P), cel(\\\"run.aff[P] >= 3\\\")\"\n\
         \x20 - \"plain(P) :- person(P), not suitor(P), cel(\\\"run.day >= 2\\\")\"\n",
    );
    write(
        &d,
        "scenes/s.lute",
        "---\nkind: scene\nid: s\non: tick\nwhen: \"holds(ready(sol)) || holds(plain(ines))\"\n---\n\n## S\n\n@narrator: Ready.\n",
    );
    let dir = d.to_str().unwrap();
    let s = ok(&["scenario", dir, "knowledge", "--for", "s"]);
    assert!(!s.contains("undeclared relation"), "{s}");
    assert!(s.contains("suitor(sol) — entity kind `suitor`; sol is a member\n"), "{s}");
    assert!(
        s.contains("cel(\"run.aff.sol >= 3\") — state condition on run.aff.sol, decided at run time\n"),
        "the guard is grounded by the head's binding: {s}"
    );
    assert!(s.contains("person(ines) — entity kind `person`; ines is a member\n"), "{s}");
    assert!(s.contains("not suitor(ines) — entity kind `suitor`; ines is not a member — always holds\n"), "{s}");
    assert!(s.contains("cel(\"run.day >= 2\") — state condition on run.day"), "{s}");

    let out = lute(&["scenario", dir, "--format", "json", "knowledge", "--for", "s"]);
    let j: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let rels = &j["roots"][0]["relations"];
    assert!(rels.get("suitor").is_none() && rels.get("person").is_none(), "{rels}");
    let premises = &rels["ready"]["rules"][0]["premises"];
    assert_eq!(premises[0], serde_json::json!({ "entityKind": "suitor" }), "{premises}");
    assert_eq!(premises[1], serde_json::json!({ "cel": "run.aff[P] >= 3" }), "{premises}");
}
