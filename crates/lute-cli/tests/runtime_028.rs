//! dsl 0.28.0 runtime seams through the built `lute` binary: an
//! `outsideRun` occasion after `terminal:` holds (T2-9), a `judge: before`
//! judgement that ends the game without silencing its own occasion (T1-22),
//! a `select: sequence` raise judging each beat at its turn (T2-10), a
//! `for=` entry's first read per member (T1-6a), `failedBy: subquest`
//! (T2-13), and `lute test` reading an unwritten objective's reserved
//! default (T3-65).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir =
        std::env::temp_dir().join(format!("lute-runtime028-{tag}-{}-{n}", std::process::id()));
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

/// A project with one plugin declaring `occasions` (the body of its
/// `occasions:` map), the schema `schema`, and `docs` (path, text).
fn project(tag: &str, occasions: &str, schema: &str, docs: &[(&str, &str)]) -> PathBuf {
    let dir = temp_dir(tag);
    write(
        &dir,
        "lute.project.yaml",
        "pluginsDir: plugins/\ndefaultProfile: g\nprofiles:\n  g:\n    plugins: { p: true }\n\
         defaults:\n  uses: [world.schema.yaml]\n",
    );
    write(
        &dir,
        "plugins/p/plugin.yaml",
        "id: p\nversion: 0.1.0\nkind: capability\ndepends: [ { id: lute.core, range: \"^0.0.1\" } ]\n\
         exports:\n  occasions: occasions/\n",
    );
    write(
        &dir,
        "plugins/p/occasions/o.yaml",
        &format!("occasions:\n{occasions}"),
    );
    write(&dir, "world.schema.yaml", schema);
    for (rel, body) in docs {
        write(&dir, rel, body);
    }
    dir
}

fn scene(id: &str, front: &str, body: &str) -> String {
    format!("---\nkind: scene\nid: {id}\n{front}---\n\n## {id}\n\n{body}\n")
}

fn play(dir: &Path, script: &str) -> Output {
    write(dir, "s.play.yaml", script);
    Command::new(BIN)
        .args(["play", &dir.display().to_string(), "--script"])
        .arg(dir.join("s.play.yaml"))
        .output()
        .unwrap()
}

/// T2-9: once `terminal:` holds, an `outsideRun` occasion (a title screen)
/// is still raised and its beats present; an ordinary one is refused.
#[test]
fn an_outside_run_occasion_is_raised_after_the_game_is_over() {
    let finale = scene(
        "finale",
        "on: verdict\n",
        "::set{run.over = true}\n@narrator: It ends.",
    );
    let title = scene(
        "t",
        "on: title\nonce: false\n",
        "@narrator: The title screen.",
    );
    let gallery = scene("g", "on: gallery\nonce: false\n", "@narrator: The gallery.");
    let dir = project(
        "outside",
        "  verdict: { select: first }\n  title: { select: first, outsideRun: true }\n  gallery: { select: first }\n",
        "state:\n  run.over: { type: bool, default: false }\nterminal: \"run.over\"\n",
        &[
            ("scenes/finale.lute", &finale),
            ("scenes/t.lute", &title),
            ("scenes/g.lute", &gallery),
        ],
    );
    let out = play(
        &dir,
        "steps:\n  - occasion: verdict\n  - occasion: title\n    expect: { winner: t }\n",
    );
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(t.contains("@narrator: The title screen."), "{t}");

    let out = play(
        &dir,
        "steps:\n  - occasion: verdict\n  - occasion: gallery\n",
    );
    let t = text(&out);
    assert!(!out.status.success(), "{t}");
    assert!(t.contains("E-OCCASION-GATE"), "{t}");
    assert!(!t.contains("@narrator: The gallery."), "{t}");
}

/// T1-22: a `judge: before` judgement that makes `terminal:` hold does not
/// refuse the beats of the occasion that judged it — the epilogue plays.
#[test]
fn a_judge_before_judgement_that_ends_the_game_still_presents_its_occasion() {
    let quest = "---\nkind: quest\nid: qs\n---\n\n\
                 <quest id=\"case\" title=\"Case\" start=\"true\" tier=\"run\">\n  \
                 <objective id=\"judged\" title=\"Judged\" on=\"hearing\" done=\"run.filed\"/>\n\
                 </quest>\n";
    let epilogue = scene(
        "h",
        "on: hearing\n",
        "<match on=\"quest.case.state\">\n  <when is=\"complete\">\n    \
         @narrator: The case is closed in your favour.\n  </when>\n  <otherwise>\n    \
         @narrator: Something else.\n  </otherwise>\n</match>",
    );
    let dir = project(
        "judgebefore",
        "  hearing: { select: first, judge: before }\n",
        "state:\n  run.filed: { type: bool, default: true }\nterminal: \"quest.case.state == 'complete'\"\n",
        &[("quests/q.lute", quest), ("scenes/h.lute", &epilogue)],
    );
    let out = play(
        &dir,
        "steps:\n  - occasion: hearing\n    expect: { winner: h, quests: { case: complete } }\n",
    );
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(
        t.contains("@narrator: The case is closed in your favour."),
        "{t}"
    );
    assert!(t.contains("end: terminal"), "{t}");
}

/// T2-10: in a `select: sequence` raise, a beat whose `when` an earlier
/// beat of the same raise made false is judged at its turn and skipped.
#[test]
fn a_sequence_raise_judges_each_beat_at_its_turn() {
    let leaves = scene(
        "leaves",
        "on: camp\npriority: 50\n",
        "::set{run.wrenGone = true}\n@narrator: Wren has left the party.",
    );
    let watch = scene(
        "watch",
        "on: camp\npriority: 10\nwhen: \"!run.wrenGone\"\n",
        "@narrator: Wren takes the watch.",
    );
    let fire = scene(
        "fire",
        "on: camp\npriority: 5\n",
        "@narrator: The fire burns low.",
    );
    let dir = project(
        "sequence",
        "  camp: { select: sequence }\n",
        "state:\n  run.wrenGone: { type: bool, default: false }\n",
        &[
            ("scenes/leaves.lute", &leaves),
            ("scenes/watch.lute", &watch),
            ("scenes/fire.lute", &fire),
        ],
    );
    let out = play(&dir, "steps:\n  - occasion: camp\n");
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(t.contains("@narrator: Wren has left the party."), "{t}");
    assert!(!t.contains("Wren takes the watch"), "{t}");
    assert!(t.contains("judged at its turn"), "{t}");
    // The raise goes on past the skipped beat.
    assert!(t.contains("@narrator: The fire burns low."), "{t}");
}

/// T1-6a: a `for=` entry's writes apply on each member's first read in a
/// run, not only on the first member's.
#[test]
fn a_for_entry_writes_on_each_members_first_read() {
    let lore = "---\nkind: lore\nid: send\n---\n\n\
                <entry id=\"sendOff\" on=\"departure\" for=\"kind:npc\" once=\"run\">\n  \
                @narrator: {{occasion.target}} waves.\n  <match on=\"occasion.target\">\n    \
                <when is=\"maud\">\n      ::set{run.bond.maud += 1}\n    </when>\n    \
                <when is=\"oskar\">\n      ::set{run.bond.oskar += 1}\n    </when>\n    \
                <when is=\"sable\">\n      ::set{run.bond.sable += 1}\n    </when>\n  \
                </match>\n</entry>\n";
    let dir = project(
        "forentry",
        "  departure: { select: sequence }\n",
        "state:\n  run.bond: { type: number, default: 0, per: npc }\n\
         entities:\n  npc: { members: [maud, oskar, sable] }\n\
         cast:\n  maud: { name: Maud }\n  oskar: { name: Oskar }\n  sable: { name: Sable }\n",
        &[("lore/send.lute", lore)],
    );
    let out = play(
        &dir,
        "steps:\n  - occasion: departure\n\
         expect:\n  state: { run.bond.maud: 1, run.bond.oskar: 1, run.bond.sable: 1 }\n",
    );
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(!t.contains("re-read"), "{t}");
}

/// T2-13: a parent quest failed by its failed required subquest records
/// `failedBy: subquest`, not `fail` (it has no `fail=`).
#[test]
fn a_failed_required_subquest_fails_its_parent_by_subquest() {
    let quests = "---\nkind: quest\nid: qs\n---\n\n\
                  <quest id=\"inquiry\" title=\"Inquiry\" start=\"true\" tier=\"run\">\n  \
                  <objective id=\"report\" title=\"Report\" quest=\"reconstruction\"/>\n</quest>\n\n\
                  <quest id=\"reconstruction\" title=\"Reconstruction\" tier=\"run\">\n  \
                  <objective id=\"file\" title=\"File it\" done=\"run.filed\" by=\"run.day > 0\"/>\n\
                  </quest>\n";
    let hall = scene("h", "on: hearing\n", "@narrator: The hearing sits.");
    let dir = project(
        "subquest",
        "  hearing: { select: first }\n",
        "state:\n  run.day: { type: number, default: 1 }\n  run.filed: { type: bool, default: false }\n",
        &[("quests/q.lute", quests), ("scenes/h.lute", &hall)],
    );
    let out = play(
        &dir,
        "steps:\n  - occasion: hearing\n\
         expect:\n  quests: { inquiry: failed, reconstruction: failed }\n  \
         state: { quest.reconstruction.failedBy: by, quest.inquiry.failedBy: subquest }\n",
    );
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(t.contains("quest inquiry -> failed (subquest)"), "{t}");
}

/// A subquest objective's own missed `by` fails its quest (`failedBy: by`)
/// and cascades to the still-active subquest — the T2-13 subquest judgement
/// must not shadow the objective's deadline.
#[test]
fn a_subquest_objective_missing_its_by_fails_its_quest_and_cascades() {
    let quests = "---\nkind: quest\nid: qs\n---\n\n\
                  <quest id=\"inquiry\" title=\"Inquiry\" start=\"true\" tier=\"run\">\n  \
                  <objective id=\"report\" title=\"Report\" quest=\"reconstruction\" by=\"run.day > 1\"/>\n\
                  </quest>\n\n\
                  <quest id=\"reconstruction\" title=\"Reconstruction\" tier=\"run\">\n  \
                  <objective id=\"file\" title=\"File it\" done=\"run.filed\"/>\n\
                  </quest>\n";
    let hall = scene(
        "h",
        "on: hearing\n",
        "@narrator: The hearing sits.\n::set{run.day += 1}",
    );
    let dir = project(
        "subquest-by",
        "  hearing: { select: first }\n",
        "state:\n  run.day: { type: number, default: 1 }\n  run.filed: { type: bool, default: false }\n",
        &[("quests/q.lute", quests), ("scenes/h.lute", &hall)],
    );
    let out = play(
        &dir,
        "steps:\n  - occasion: hearing\n\
         expect:\n  quests: { inquiry: failed, reconstruction: failed }\n  \
         state: { quest.inquiry.failedBy: by, quest.reconstruction.failedBy: cascade }\n",
    );
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(t.contains("quest inquiry -> failed (by)"), "{t}");
}

/// T3-65: `lute test` reads an objective's unwritten `failed` as `false`
/// (as `lute play` does), while a written `true` still fails that pin.
#[test]
fn a_quest_test_reads_an_unwritten_objective_failed_as_false() {
    let quests = "---\nkind: quest\nid: qs\n---\n\n\
                  <quest id=\"inquiry\" title=\"Inquiry\" start=\"true\" tier=\"run\">\n  \
                  <objective id=\"report\" title=\"Report\" done=\"run.filed\" by=\"run.day > 5\"/>\n  \
                  <objective id=\"other\" title=\"Other\" done=\"run.filed\"/>\n</quest>\n";
    let dir = project(
        "objfailed",
        "  hearing: { select: first }\n",
        "state:\n  run.day: { type: number, default: 1 }\n  run.filed: { type: bool, default: false }\n",
        &[("quests/q.lute", quests)],
    );
    let pin = "expect:\n  state: { quest.inquiry.objectives.report.failed: false }\n";
    write(
        &dir,
        "tests/unwritten.test.yaml",
        &format!("file: ../quests/q.lute\noccasions: [hearing]\n{pin}"),
    );
    write(
        &dir,
        "tests/missed.test.yaml",
        &format!("file: ../quests/q.lute\nstate: {{ run.day: 6 }}\noccasions: [hearing]\n{pin}"),
    );
    let out = Command::new(BIN)
        .args(["test", dir.join("tests").to_str().unwrap()])
        .args(["--project", dir.to_str().unwrap()])
        .output()
        .unwrap();
    let t = text(&out);
    let line = |name: &str| {
        t.lines()
            .find(|l| l.contains(name))
            .unwrap_or_else(|| panic!("no line for {name}:\n{t}"))
            .to_string()
    };
    assert!(line("unwritten.test.yaml").starts_with("PASS"), "{t}");
    assert!(line("missed.test.yaml").starts_with("FAIL"), "{t}");
    assert!(!t.contains("never written"), "{t}");
}
