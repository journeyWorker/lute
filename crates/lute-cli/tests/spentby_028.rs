//! dsl 0.28.0 §6: the `spentBy` latch made safe — it looks only at settled
//! worlds (a `start="true"` quest is `active`, never its `unset` before),
//! `W-BEAT-SPENT-AT-START` knows which quests start at once, and
//! `W-SPENT-BY-REVERSIBLE` names a latch whose condition can turn false
//! again. Checked and played through the built `lute` binary.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-spentby-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &Path, rel: &str, text: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, text).unwrap();
}

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

const SCHEMA: &str = "state:\n  run.done: { type: bool, default: false }\n  \
     run.day: { type: int, default: 1, owner: engine }\n  \
     season.frost.vigils: { type: int, default: 0 }\n\
     clock:\n  day: run.day\n\
     defs:\n  frostLive: \"run.day >= 2 && run.day <= 3\"\n\
     seasons:\n  frost: { live: \"@frostLive\" }\n\
     entities:\n  fixture: { members: [drawer] }\n  fish: { members: [marlin, cod] }\n\
     relations:\n  solved: { args: [fixture], tier: run }\n  caught: { args: [fish], tier: run }\n";

/// A project with `world.schema.yaml` ([`SCHEMA`]) and plugin `p`: the
/// occasions `visit` and `storm`, and `::sell{fish}`, whose declared effect
/// retracts `caught(@fish)`; plus every `(path, text)` document.
fn project(tag: &str, docs: &[(&str, &str)]) -> PathBuf {
    let dir = temp_dir(tag);
    write(
        &dir,
        "lute.project.yaml",
        "pluginsDir: plugins/\ndefaultProfile: base\nprofiles: { base: { plugins: { p: true } } }\n\
         defaults:\n  uses: [world.schema.yaml]\n  questTier: run\n",
    );
    write(&dir, "world.schema.yaml", SCHEMA);
    write(
        &dir,
        "plugins/p/plugin.yaml",
        "id: p\nversion: 0.1.0\nkind: capability\ndepends: [ { id: lute.core, range: \"^0.0.1\" } ]\n\
         exports: { occasions: occasions/, directives: directives/ }\n",
    );
    write(
        &dir,
        "plugins/p/occasions/o.yaml",
        "occasions:\n  visit: { select: first }\n  storm: { select: first }\n",
    );
    write(
        &dir,
        "plugins/p/directives/d.yaml",
        "directives:\n  - name: sell\n    attrs:\n      - { name: fish, required: true, type: { entity: fish } }\n    \
         effects:\n      retracts: [\"caught(@fish)\"]\n",
    );
    for (rel, body) in docs {
        write(&dir, rel, body);
    }
    dir
}

fn check_project(dir: &Path) -> String {
    text(
        &Command::new(BIN)
            .arg("check-project")
            .arg(dir)
            .output()
            .unwrap(),
    )
}

fn play(dir: &Path, script: &str) -> Output {
    write(dir, "p.play.yaml", script);
    Command::new(BIN)
        .args(["play", &dir.display().to_string(), "--script"])
        .arg(dir.join("p.play.yaml"))
        .output()
        .unwrap()
}

fn scene(id: &str, on: &str, keys: &str, body: &str) -> String {
    format!("---\nkind: scene\nid: {id}\non: {on}\n{keys}---\n\n## S\n\n{body}\n")
}

/// A lore document of bundle beats.
fn lore(id: &str, beats: &str) -> String {
    format!("---\nkind: lore\nid: {id}\n---\n\n{beats}")
}

/// The lines of `out` carrying `code`.
fn lines<'a>(out: &'a str, code: &str) -> Vec<&'a str> {
    out.lines().filter(|l| l.contains(code)).collect()
}

const CHORE: &str = "---\nkind: quest\nid: qs\n---\n\n\
     <quest id=\"chore\" title=\"Do the chore\" start=\"true\">\n  \
     <objective id=\"it\" title=\"Do it\" done=\"run.done\"/>\n</quest>\n";

const CHORE_BEAT: &str =
    "<beat id=\"chore\" on=\"visit\" spentBy=\"quest.chore.state != 'active'\">\n  \
     ::set{run.done = true}\n  @narrator: You do the chore.\n</beat>\n\
     <beat id=\"idle\" on=\"visit\" priority=\"-10\">\n  @narrator: Nothing to do.\n</beat>\n";

/// N-1: a `start="true"` quest is `active` at every raise, and the latch
/// never sees the `unset` it had before its first settle finished — so a
/// beat spent by the quest leaving `active` plays, and is not reported as
/// spent at the start.
#[test]
fn a_start_true_quest_is_active_when_the_latch_first_looks() {
    let dir = project(
        "start-quest",
        &[
            ("quests/q.lute", CHORE),
            ("lore/b.lute", &lore("b", CHORE_BEAT)),
        ],
    );
    let out = check_project(&dir);
    assert!(lines(&out, "W-BEAT-SPENT-AT-START").is_empty(), "{out}");
    let played = play(
        &dir,
        "steps:\n  - occasion: visit\n    expect: { winner: b.chore }\n  \
         - occasion: visit\n    expect: { winner: b.idle }\n",
    );
    let t = text(&played);
    assert!(played.status.success(), "{t}");
    // Spent once the quest completed, not before.
    assert_eq!(
        t.matches("spentBy: `quest.chore.state != 'active'` held")
            .count(),
        1,
        "{t}"
    );
}

/// A quest another document declares that waits for its `start` is `unset`
/// at the start of play: a `spentBy` over it that holds there is reported
/// project-wide, naming the `when: "!(…)"` rewrite.
#[test]
fn a_spent_by_over_a_waiting_quest_elsewhere_holds_at_the_start() {
    let waiting = CHORE.replace("start=\"true\"", "start=\"run.day >= 2\"");
    let dir = project(
        "waiting-quest",
        &[
            ("quests/q.lute", &waiting),
            ("lore/b.lute", &lore("b", CHORE_BEAT)),
        ],
    );
    let out = check_project(&dir);
    let found = lines(&out, "W-BEAT-SPENT-AT-START");
    assert_eq!(found.len(), 1, "{out}");
    assert!(
        found[0].contains("lore/b.lute")
            && found[0].contains("when: \"!(quest.chore.state != 'active')\""),
        "{out}"
    );
}

const DRAWER: &str = "@narrator: You open the drawer.\n::assert{solved(drawer)}";
const IDLE: &str = "---\nkind: scene\nid: idle\non: visit\nonce: false\npriority: -10\n---\n\n\
     ## Idle\n\n@narrator: Nothing more to do here.\n";
const STORM: &str = "---\nkind: scene\nid: storm\non: storm\nonce: false\n---\n\n## Storm\n\n\
     @narrator: The storm slams the drawer shut.\n::retract{solved(drawer)}\n";

/// HW28-01: a `spentBy` fact another scene retracts can turn false again,
/// while the beat stays spent for the run. The warning names the retract
/// and the rewrite; the rewrite plays the beat again after the retract,
/// and a written `once: run` keeps the latch on purpose, silently.
#[test]
fn a_retracted_fact_warns_that_the_beat_stays_spent() {
    let docs = |keys: &str| {
        vec![
            ("scenes/puzzle.lute", scene("puzzle", "visit", keys, DRAWER)),
            ("scenes/idle.lute", IDLE.to_string()),
            ("scenes/storm.lute", STORM.to_string()),
        ]
    };
    let make = |tag: &str, keys: &str| {
        let docs = docs(keys);
        let refs: Vec<(&str, &str)> = docs.iter().map(|(p, t)| (*p, t.as_str())).collect();
        project(tag, &refs)
    };
    let dir = make("retract", "spentBy: \"holds('solved', ['drawer'])\"\n");
    let out = check_project(&dir);
    let found = lines(&out, "W-SPENT-BY-REVERSIBLE");
    assert_eq!(found.len(), 1, "{out}");
    assert!(
        found[0].contains("scenes/puzzle.lute")
            && found[0].contains("`::retract{solved(drawer)}` (scenes/storm.lute:")
            && found[0].contains("`once: false` with `when: \"!holds('solved', ['drawer'])\"`")
            && found[0].contains("`once: run`"),
        "{out}"
    );

    let script = "steps:\n  - occasion: visit\n    expect: { winner: puzzle }\n  \
         - occasion: storm\n  - occasion: visit\n    expect: { winner: puzzle }\n";
    let rewritten = make(
        "retract-rewritten",
        "once: false\nwhen: \"!holds('solved', ['drawer'])\"\n",
    );
    let out = check_project(&rewritten);
    assert!(lines(&out, "W-SPENT-BY-REVERSIBLE").is_empty(), "{out}");
    let played = play(&rewritten, script);
    assert!(played.status.success(), "{}", text(&played));

    let on_purpose = make(
        "retract-once",
        "once: run\nspentBy: \"holds('solved', ['drawer'])\"\n",
    );
    let out = check_project(&on_purpose);
    assert!(lines(&out, "W-SPENT-BY-REVERSIBLE").is_empty(), "{out}");
}

/// A directive whose declared effect retracts the fact warns like a
/// `::retract`, naming the directive; a fact nothing takes back (only
/// asserted), or a `count(…) >= n` only asserts can move, stays quiet.
#[test]
fn a_directive_effect_retract_warns_and_a_monotone_fact_does_not() {
    let beats = "<beat id=\"launch\" on=\"visit\" spentBy=\"holds('caught', ['marlin'])\">\n  \
         @narrator: Out to sea.\n  ::assert{caught(marlin)}\n</beat>\n\
         <beat id=\"haul\" on=\"visit\" priority=\"-1\" spentBy=\"count('solved', ['_']) >= 1\">\n  \
         @narrator: The drawer.\n  ::assert{solved(drawer)}\n</beat>\n";
    let market = scene(
        "market",
        "storm",
        "once: false\n",
        "@narrator: Sold.\n::sell{fish=\"marlin\"}",
    );
    let dir = project(
        "effect",
        &[
            ("lore/t.lute", &lore("t", beats)),
            ("scenes/market.lute", &market),
        ],
    );
    let out = check_project(&dir);
    let found = lines(&out, "W-SPENT-BY-REVERSIBLE");
    assert_eq!(found.len(), 1, "{out}");
    assert!(
        found[0].contains("beat `t.launch`")
            && found[0].contains("`::sell` (it retracts `caught(marlin)`) (scenes/market.lute:"),
        "{out}"
    );
}

/// P28S-04: a condition over `season.<name>.*` goes back to its default each
/// time the season opens, so the warning names `once: season:<name>`; a
/// quest `rearm` returns to `unset`, so a condition over it warns too. With
/// the season's period written, the season beat is quiet.
#[test]
fn a_season_or_rearmed_quest_condition_warns_with_its_period() {
    let quest = "---\nkind: quest\nid: qs\n---\n\n\
         <quest id=\"round\" title=\"Round\" start=\"@frostLive\" rearm=\"@frostLive\">\n  \
         <objective id=\"it\" title=\"It\" done=\"run.done\"/>\n</quest>\n";
    let beats = |once: &str| {
        format!(
            "<beat id=\"nag\" on=\"visit\"{once} when=\"@frostLive\" \
             spentBy=\"season.frost.vigils >= 3\">\n  @narrator: Keep the vigil.\n</beat>\n\
             <beat id=\"cheer\" on=\"visit\" priority=\"-1\" \
             spentBy=\"quest.round.state == 'complete'\">\n  @narrator: Go on.\n</beat>\n"
        )
    };
    let dir = project(
        "season",
        &[
            ("quests/q.lute", quest),
            ("lore/r.lute", &lore("r", &beats(""))),
        ],
    );
    let out = check_project(&dir);
    let found = lines(&out, "W-SPENT-BY-REVERSIBLE");
    assert_eq!(found.len(), 2, "{out}");
    assert!(
        found.iter().any(|l| l.contains("beat `r.nag`")
            && l.contains("`season.frost.vigils` goes back to its default")
            && l.contains("write `once: season:frost`")),
        "{out}"
    );
    assert!(
        found.iter().any(|l| l.contains("beat `r.cheer`")
            && l.contains("quest `round` returns to `unset` when its `rearm` holds")),
        "{out}"
    );
    let dir = project(
        "season-once",
        &[
            ("quests/q.lute", quest),
            ("lore/r.lute", &lore("r", &beats(" once=\"season:frost\""))),
        ],
    );
    let out = check_project(&dir);
    assert!(
        !lines(&out, "W-SPENT-BY-REVERSIBLE")
            .iter()
            .any(|l| l.contains("r.nag")),
        "{out}"
    );
}
