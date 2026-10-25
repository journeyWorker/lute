//! #205 (0.50.0 §5.4): a quest's `start=` is judged once, when the quest
//! activates; its body (objective bodies, handlers) runs at any later
//! engine-chosen moment while the quest stays active. A fact `start=`
//! requires is therefore Must in the body only when it outlasts the quest:
//! a crossing fact of a `tier: user` / `app` relation, of a `tier: run`
//! relation (the default) in a `tier="run"` quest (a `newRun` resets both),
//! or of a `tier: season:<s>` relation in a `tier="season:<s>"` quest (the
//! season's opening resets both). Otherwise the reset can clear the fact
//! while the quest stays active.
//!
//! On 0.49.1 the season repro below is a false `E-ARM-DEAD`.

use std::path::Path;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

const PROJECT: &str = "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\ndefaults:\n  \
                       contentLang: en\n  uses: [schema/world.schema.yaml]\n";

/// `met`'s relation declared with `tier`.
fn schema(tier: &str) -> String {
    format!(
        "entities:\n  person: {{ members: [mira, theo] }}\nrelations:\n  met: {{ args: \
         [person], tier: \"{tier}\" }}\nstate:\n  user.sOn: {{ type: bool, default: false }}\n  run.other: \
         {{ type: bool, default: false }}\n  run.flag: {{ type: bool, default: false }}\ncast:\n  \
         mira: {{ name: Mira }}\nseasons:\n  s: {{ live: \"user.sOn\" }}\n  t: {{ live: \
         \"!user.sOn\" }}\n"
    )
}

/// A producer of `met(mira)`.
const MEET: &str = "---\nkind: scene\nid: meetMira\n---\n\n## Shot 1.\n\n@mira: Hi.\n\
                    ::assert{met(mira)}\n";

/// Where quest `a`'s `<match>` sits: its objective's body, or a
/// `questComplete` handler (a deferred lifecycle body).
#[derive(Clone, Copy)]
enum Body {
    Objective,
    Handler,
}

/// Quest `a` of `tier`, started by `holds('met', ['mira'])` (and, for a
/// season quest, its season's `live`), whose `body` holds a `<match>`
/// testing `!holds('met', ['mira'])`.
fn quest(tier: &str, body: Body) -> String {
    let live = match tier {
        "season:s" => " && user.sOn",
        "season:t" => " && !user.sOn",
        _ => "",
    };
    let (open, close) = match body {
        Body::Objective => (
            "<objective id=\"c1\" title=\"End\" done=\"run.flag\">",
            "</objective>",
        ),
        Body::Handler => (
            "<objective id=\"c1\" title=\"End\" done=\"run.flag\"/>\n  \
             <on event=\"questComplete\">",
            "</on>",
        ),
    };
    format!(
        "---\nkind: quest\nid: acme.quests\n---\n\n\
         <quest id=\"a\" tier=\"{tier}\" title=\"A\" start=\"holds('met', ['mira']){live}\">\n  \
         {open}\n    <match>\n      \
         <when test=\"!holds('met', ['mira'])\">\n        ::set{{run.other=true}}\n      \
         </when>\n      <otherwise>\n        ::set{{run.other=false}}\n      </otherwise>\n    \
         </match>\n  {close}\n</quest>\n"
    )
}

fn write(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, text).unwrap();
}

/// The #205 project: `met` of `relation_tier`, quest `a` of `quest_tier`
/// with its `<match>` in `body`.
fn project(
    tag: &str,
    relation_tier: &str,
    quest_tier: &str,
    body: Body,
) -> lute_scratch::ScratchDir {
    let dir = lute_scratch::ScratchDir::named(tag);
    write(&dir, "lute.project.yaml", PROJECT);
    write(&dir, "schema/world.schema.yaml", &schema(relation_tier));
    write(&dir, "scenes/meet.lute", MEET);
    write(&dir, "quests/q.lute", &quest(quest_tier, body));
    dir
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

/// The `!holds` arm at `at` is provably dead: `met(mira)` outlasts the
/// quest.
fn assert_dead(diags: &[String], tag: &str, at: &str) {
    assert_eq!(diags.len(), 1, "{tag}: {diags:?}");
    assert!(
        diags[0].starts_with(&format!("quests/q.lute:{at} [E-ARM-DEAD]")),
        "{tag}: {diags:?}"
    );
}

/// The #205 repro: season `s` reopening resets `met(mira)` to the seeds
/// while the user-tier quest `a` stays active, so the `!holds` arm is live.
/// A run-tier quest, or one of another season, stays active as well.
#[test]
fn a_season_tier_start_fact_does_not_outlast_another_tier_quest() {
    for (tag, quest_tier) in [
        ("qs-season-user", "user"),
        ("qs-season-run", "run"),
        ("qs-season-other", "season:t"),
    ] {
        let dir = project(tag, "season:s", quest_tier, Body::Objective);
        assert_eq!(check(&dir), Vec::<String>::new(), "{tag}");
    }
}

/// A `newRun` resets a run-tier fact while a user-tier or season-tier
/// quest stays active.
#[test]
fn a_run_tier_start_fact_does_not_outlast_a_user_or_season_quest() {
    for (tag, quest_tier) in [("qs-run-user", "user"), ("qs-run-season", "season:s")] {
        let dir = project(tag, "run", quest_tier, Body::Objective);
        assert_eq!(check(&dir), Vec::<String>::new(), "{tag}");
    }
}

/// Precision kept: a user-tier fact outlasts every quest, and a run-tier
/// (season-tier) fact outlasts a quest of the same tier, reset together
/// with it.
#[test]
fn a_start_fact_of_a_lasting_tier_still_crosses() {
    for (tag, relation_tier, quest_tier) in [
        ("qs-user-user", "user", "user"),
        ("qs-user-run", "user", "run"),
        ("qs-user-season", "user", "season:s"),
        ("qs-run-run", "run", "run"),
        ("qs-season-season", "season:s", "season:s"),
    ] {
        let dir = project(tag, relation_tier, quest_tier, Body::Objective);
        assert_dead(&check(&dir), tag, "9:7");
    }
}

/// A `questComplete` handler body may be deferred past a season opening
/// that resets the fact and the same-season quest together, and the
/// selected body still runs: a season-tier `start=` fact is not Must there
/// even in a quest of its season. A user-tier one still is.
#[test]
fn a_deferred_handler_gets_no_season_tier_start_fact() {
    let dir = project("qs-handler-season", "season:s", "season:s", Body::Handler);
    assert_eq!(check(&dir), Vec::<String>::new());
    let dir = project("qs-handler-user", "user", "user", Body::Handler);
    assert_dead(&check(&dir), "qs-handler-user", "10:7");
}
