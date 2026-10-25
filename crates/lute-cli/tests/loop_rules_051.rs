//! dsl 0.51.0 §3.4/§3.8 "0.50 rules kept": a loop head's fixed point
//! starts from the state the 0.50 walk computes on arrival and re-walks only
//! the region, so it never re-applies a body's starting rules. Each test
//! puts a bounded loop inside the body the 0.50 rule governs, on that rule's
//! own repro shape (`reward_effects_050.rs`, `deferred_handler_guard_050.rs`,
//! `quest_start_outlasts_050.rs`), and expects that rule's verdict: a head
//! that re-assumed a grant's assert, a deferred handler's guard or a quest's
//! start fact would make the guard guaranteed or its negation dead.

use std::path::Path;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn write(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, text).unwrap();
}

/// Every diagnostic line of `lute check --project .`, as `file:line:col
/// [CODE] message` (the severity dropped).
fn check(dir: &Path) -> Vec<String> {
    let out = Command::new(BIN)
        .args(["check", "--project", "."])
        .current_dir(dir)
        .env("LUTE_TELEMETRY", "0")
        .output()
        .unwrap();
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
    .lines()
    .filter_map(|l| {
        let l = l.strip_prefix("./")?;
        let (at, rest) = l.split_once(": ")?;
        let (_, d) = rest.split_once(' ')?;
        d.starts_with('[').then(|| format!("{at} {d}"))
    })
    .collect()
}

/// A `<match>` testing `test`, inside a loop headed by `label` that runs it
/// up to three times.
fn looped(label: &str, test: &str) -> String {
    format!(
        "::label{{name=\"{label}\"}}\n    <match>\n      <when test=\"{test}\">\n        \
         ::set{{run.other=true}}\n      </when>\n      <otherwise>\n        \
         ::set{{run.other=false}}\n      </otherwise>\n    </match>\n    \
         ::jump{{to=\"{label}\" max=\"2\"}}"
    )
}

const HOLDS: &str = "holds('met', ['mira'])";
const NOT_HOLDS: &str = "!holds('met', ['mira'])";

// --- 0.50 S3 fix 2: a reward's assert is possible only ----------------------

const REWARD_PROJECT: &str = "pluginsDir: plugins/\ndefaultProfile: game\nprofiles:\n  game:\n    \
                              plugins:\n      acme.stage: true\ndefaults:\n  contentLang: en\n  \
                              uses: [schema/world.schema.yaml]\n";

const REWARD_PLUGIN: &str = "id: acme.stage\nversion: 0.1.0\nkind: capability\ndepends:\n  - { \
                             id: lute.core, range: \"^0.0.1\" }\nexports:\n  rewardKinds: \
                             rewardkinds/\n";

const KINDS: &str = "rewardKinds:\n  BADGE: { target: { entity: person, required: true }, \
                     effects: { asserts: [\"met(@target)\"] } }\n";

const REWARD_SCHEMA: &str = "entities:\n  person: { members: [mira, theo] }\nrelations:\n  met: \
                             { args: [person], tier: user }\nstate:\n  run.flag: { type: bool, \
                             default: false }\n  run.other: { type: bool, default: false }\n\
                             cast:\n  mira: { name: Mira }\n";

/// `firstVisit` grants `BADGE` (the only producer of `met(mira)`) at its
/// objective `c1`, whose body loops over `test`; the dependent quest
/// `later`'s `questComplete` handler loops over it too.
fn reward_project(tag: &str, test: &str) -> lute_scratch::ScratchDir {
    let dir = lute_scratch::ScratchDir::named(tag);
    write(&dir, "lute.project.yaml", REWARD_PROJECT);
    write(&dir, "plugins/acme.stage/plugin.yaml", REWARD_PLUGIN);
    write(&dir, "plugins/acme.stage/rewardkinds/r.yaml", KINDS);
    write(&dir, "schema/world.schema.yaml", REWARD_SCHEMA);
    let (objective, handler) = (looped("granted", test), looped("later", test));
    write(
        &dir,
        "quests/q.lute",
        &format!(
            "---\nkind: quest\nid: acme.quests\n---\n\n\
             <quest id=\"firstVisit\" tier=\"user\" title=\"First visit\" start=\"true\">\n  \
             <objective id=\"c1\" title=\"Do it\" done=\"run.flag\">\n    \
             <reward kind=\"BADGE\" amount=\"1\" target=\"mira\"/>\n    {objective}\n  \
             </objective>\n</quest>\n\n\
             <quest id=\"later\" tier=\"user\" title=\"Later\" \
             start=\"quest.firstVisit.state == 'complete'\">\n  \
             <objective id=\"c1\" title=\"Do it\" done=\"run.flag\"/>\n  \
             <on event=\"questComplete\">\n    {handler}\n  </on>\n</quest>\n"
        ),
    );
    dir
}

#[test]
fn a_loop_head_keeps_a_reward_assert_possible_only() {
    for (tag, test) in [("lr-reward-holds", HOLDS), ("lr-reward-negated", NOT_HOLDS)] {
        let got = check(&reward_project(tag, test));
        assert_eq!(got, Vec::<String>::new(), "{tag}: {got:?}");
    }
}

// --- #203: a deferred handler guard -----------------------------------------

/// Quest `a`, completing on a `judge: before` `chapterEnd`, whose
/// `questComplete` handler is guarded by `holds('met', ['mira'])` and loops
/// over `test`; a `chapterEnd` beat retracts the fact.
fn deferred_project(tag: &str, test: &str) -> lute_scratch::ScratchDir {
    let dir = lute_scratch::ScratchDir::named(tag);
    write(
        &dir,
        "lute.project.yaml",
        "pluginsDir: plugins/\ndefaultProfile: game\nprofiles:\n  game:\n    plugins:\n      \
         acme: true\ndefaults:\n  contentLang: en\n  uses: [schema/world.schema.yaml]\n",
    );
    write(
        &dir,
        "plugins/acme/plugin.yaml",
        "id: acme\nversion: 0.1.0\nkind: capability\ndepends:\n  - { id: lute.core, range: \
         \"^0.0.1\" }\nexports:\n  occasions: occasions/\n",
    );
    write(
        &dir,
        "plugins/acme/occasions/o.yaml",
        "occasions:\n  chapterEnd: { select: first, judge: before }\n",
    );
    write(
        &dir,
        "schema/world.schema.yaml",
        "entities:\n  person: { members: [mira, theo] }\nrelations:\n  met: { args: [person], \
         tier: user }\nstate:\n  run.other: { type: bool, default: false }\ncast:\n  mira: { \
         name: Mira }\n",
    );
    write(
        &dir,
        "scenes/meet.lute",
        "---\nkind: scene\nid: meetMira\n---\n\n## Shot 1.\n\n@mira: Hi.\n::assert{met(mira)}\n",
    );
    write(
        &dir,
        "scenes/part.lute",
        "---\nkind: scene\nid: partWays\non: chapterEnd\n---\n\n## Shot 1.\n\n@mira: Bye.\n\
         ::retract{met(mira)}\n",
    );
    write(
        &dir,
        "quests/q.lute",
        &format!(
            "---\nkind: quest\nid: acme.quests\n---\n\n\
             <quest id=\"a\" tier=\"user\" title=\"A\" start=\"true\">\n  \
             <objective id=\"c1\" title=\"End\" on=\"chapterEnd\" done=\"true\"/>\n  \
             <on event=\"questComplete\" when=\"{HOLDS}\">\n    {}\n  </on>\n</quest>\n",
            looped("again", test)
        ),
    );
    dir
}

#[test]
fn a_loop_head_does_not_reassume_a_deferred_handler_guard() {
    for (tag, test) in [
        ("lr-deferred-holds", HOLDS),
        ("lr-deferred-negated", NOT_HOLDS),
    ] {
        let got = check(&deferred_project(tag, test));
        assert_eq!(got, Vec::<String>::new(), "{tag}: {got:?}");
    }
}

// --- #205: a quest's start fact ----------------------------------------------

/// Quest `a` of `quest_tier`, started by `holds('met', ['mira'])` with `met`
/// of `relation_tier`, whose questComplete handler (`handler`) or objective
/// body loops over `!holds('met', ['mira'])`.
fn start_project(
    tag: &str,
    relation_tier: &str,
    quest_tier: &str,
    handler: bool,
) -> lute_scratch::ScratchDir {
    let dir = lute_scratch::ScratchDir::named(tag);
    write(
        &dir,
        "lute.project.yaml",
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\ndefaults:\n  contentLang: \
         en\n  uses: [schema/world.schema.yaml]\n",
    );
    write(
        &dir,
        "schema/world.schema.yaml",
        &format!(
            "entities:\n  person: {{ members: [mira, theo] }}\nrelations:\n  met: {{ args: \
             [person], tier: \"{relation_tier}\" }}\nstate:\n  user.sOn: {{ type: bool, \
             default: false }}\n  run.other: {{ type: bool, default: false }}\n  run.flag: {{ \
             type: bool, default: false }}\ncast:\n  mira: {{ name: Mira }}\nseasons:\n  s: {{ \
             live: \"user.sOn\" }}\n  t: {{ live: \"!user.sOn\" }}\n"
        ),
    );
    write(
        &dir,
        "scenes/meet.lute",
        "---\nkind: scene\nid: meetMira\n---\n\n## Shot 1.\n\n@mira: Hi.\n::assert{met(mira)}\n",
    );
    let (open, close) = if handler {
        (
            "<objective id=\"c1\" title=\"End\" done=\"run.flag\"/>\n  \
             <on event=\"questComplete\">",
            "</on>",
        )
    } else {
        (
            "<objective id=\"c1\" title=\"End\" done=\"run.flag\">",
            "</objective>",
        )
    };
    write(
        &dir,
        "quests/q.lute",
        &format!(
            "---\nkind: quest\nid: acme.quests\n---\n\n\
             <quest id=\"a\" tier=\"{quest_tier}\" title=\"A\" start=\"{HOLDS}\">\n  \
             {open}\n    {}\n  {close}\n</quest>\n",
            looped("again", NOT_HOLDS)
        ),
    );
    dir
}

#[test]
fn a_loop_head_does_not_reassume_a_start_fact_the_quest_outlasts() {
    // A season-tier start fact in a quest of another tier, in an `<on>`
    // body and in an objective body: not Must at the head, so the `!holds`
    // arm stays live.
    for (tag, handler) in [("lr-start-on", true), ("lr-start-objective", false)] {
        let got = check(&start_project(tag, "season:s", "user", handler));
        assert_eq!(got, Vec::<String>::new(), "{tag}: {got:?}");
    }
    // Precision kept: a user-tier start fact in a user-tier quest is Must at
    // the body's entry, nothing in the loop removes it, so it is Must at the
    // head and the `!holds` arm is dead there.
    let got = check(&start_project("lr-start-kept", "user", "user", false));
    assert_eq!(got.len(), 1, "{got:?}");
    assert!(
        got[0].starts_with("quests/q.lute:10:7 [E-ARM-DEAD]"),
        "{got:?}"
    );
}
