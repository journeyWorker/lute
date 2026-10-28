//! 0.53.0 §7 (#227, #237): quest grants, routes and accept cycles.
//!
//! In-script accepts (`::accept`, a reward kind's `effects.accepts`) are
//! unconditional routes, built as an accept anchor even beside `follows`; an
//! `accept="external"` quest also has a host route (open, or open once its
//! `follows` holds — host policy, evidence `heuristic`); reach repeats over
//! undrawn accept edges; a cycle of `start` and in-script accept anchors
//! that nothing outside enters is `E-QUEST-START-CYCLE`. Each project is the
//! issue's repro or the spec's §7.5 shape; the comment on each test names
//! what v0.52.0 did instead.

use std::path::Path;
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

const PROJECT: &str = "pluginsDir: plugins/\ndefaultProfile: game\nprofiles:\n  game:\n    \
                       plugins:\n      acme.rewards: true\ndefaults:\n  contentLang: en\n  uses: \
                       [schema/world.schema.yaml]\n";

const PLUGIN: &str = "id: acme.rewards\nversion: 0.1.0\nkind: capability\ndepends:\n  - { id: \
                      lute.core, range: \"^0.0.1\" }\nexports:\n  rewardKinds: rewardkinds/\n";

const KINDS: &str = "rewardKinds:\n  OFFER_QUEST: { target: { required: true }, effects: { \
                     accepts: [\"@target\"] } }\n";

/// `run.done` has no default: only a route that writes it guarantees it.
const SCHEMA: &str = "state:\n  run.flag: { type: bool, default: false }\n  run.never: { type: \
                      bool, default: false }\n  run.done: { type: bool }\ncast:\n  mira: { name: \
                      Mira }\n";

fn write(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, text).unwrap();
}

/// A project with the `OFFER_QUEST` reward kind, `quests` as its one quest
/// document (`quests/q.lute`, the body starting on line 7), `manifest`
/// appended to the manifest, and `extra` files.
fn project(
    tag: &str,
    quests: &str,
    manifest: &str,
    extra: &[(&str, &str)],
) -> lute_scratch::ScratchDir {
    let dir = lute_scratch::ScratchDir::named(tag);
    write(&dir, "lute.project.yaml", &format!("{PROJECT}{manifest}"));
    write(&dir, "plugins/acme.rewards/plugin.yaml", PLUGIN);
    write(&dir, "plugins/acme.rewards/rewardkinds/kinds.yaml", KINDS);
    write(&dir, "schema/world.schema.yaml", SCHEMA);
    write(
        &dir,
        "quests/q.lute",
        &format!("---\nkind: quest\nid: q\ntitle: Quests\n---\n\n{quests}"),
    );
    for (rel, text) in extra {
        write(&dir, rel, text);
    }
    dir
}

fn lute(dir: &Path, args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .current_dir(dir)
        .env("LUTE_TELEMETRY", "0")
        .output()
        .unwrap()
}

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

fn json(dir: &Path, args: &[&str]) -> serde_json::Value {
    let out = lute(dir, args);
    serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&out.stdout)))
}

/// `lute check --project .`: exit code and output.
fn check(dir: &Path) -> (i32, String) {
    let out = lute(dir, &["check", "--project", "."]);
    (out.status.code().unwrap(), text(&out))
}

fn reach(dir: &Path, node: &str) -> String {
    text(&lute(
        dir,
        &["scenario", "--project", ".", "reach", "--node", node],
    ))
}

fn reach_json(dir: &Path, node: &str) -> serde_json::Value {
    json(
        dir,
        &[
            "scenario",
            "--project",
            ".",
            "reach",
            "--node",
            node,
            "--json",
        ],
    )
}

fn graph(dir: &Path) -> String {
    text(&lute(dir, &["scenario", "--project", "."]))
}

/// The first JSON object under `v` that `pred` accepts.
fn find<'v>(
    v: &'v serde_json::Value,
    pred: &dyn Fn(&serde_json::Map<String, serde_json::Value>) -> bool,
) -> &'v serde_json::Value {
    fn walk<'v>(
        v: &'v serde_json::Value,
        pred: &dyn Fn(&serde_json::Map<String, serde_json::Value>) -> bool,
    ) -> Option<&'v serde_json::Value> {
        match v {
            serde_json::Value::Object(o) if pred(o) => Some(v),
            serde_json::Value::Object(o) => o.values().find_map(|c| walk(c, pred)),
            serde_json::Value::Array(a) => a.iter().find_map(|c| walk(c, pred)),
            _ => None,
        }
    }
    walk(v, pred).unwrap_or_else(|| panic!("no such object in {v}"))
}

fn quest(id: &str, attrs: &str, body: &str) -> String {
    format!(
        "<quest id=\"{id}\" title=\"Q\"{attrs}>\n  <objective id=\"o\" title=\"Do it\" \
         done=\"true\"/>\n{body}</quest>\n\n"
    )
}

fn offer(target: &str) -> String {
    format!("  <reward kind=\"OFFER_QUEST\" target=\"{target}\"/>\n")
}

const REACHABLE: &str = "verdict: Reachable — a satisfiable route exists under your declared \
                         routes.\n";

// ---- §7.1 (b): the host route ------------------------------------------

/// The issue's first project (#227 b). v0.52.0: clean, `letter` and
/// `intro` `Unknown` (the reward accept replaced the host's grant).
#[test]
fn an_external_quest_keeps_its_host_route_beside_a_reward_accept() {
    let quests = format!(
        "{}{}{}",
        quest(
            "intro",
            " start=\"quest.letter.state == 'active'\"",
            &offer("letter")
        ),
        quest("letter", " accept=\"external\"", ""),
        quest("solo", " accept=\"external\"", ""),
    );
    let dir = project("q227-b", &quests, "", &[]);
    let (code, out) = check(&dir);
    assert_eq!(code, 0, "{out}");
    assert!(
        !out.contains("error") && !out.contains("warning ["),
        "{out}"
    );

    let letter = reach(&dir, "quest:letter");
    assert!(letter.contains(REACHABLE), "{letter}");
    assert!(
        letter.contains("    [accept] the host (accept=\"external\") | quest(intro)\n"),
        "{letter}"
    );
    let intro = reach(&dir, "quest:intro");
    assert!(intro.contains(REACHABLE), "{intro}");
    let solo = reach(&dir, "quest:solo");
    assert!(
        solo.contains("verdict: Unanchored — a quest with no declared `follows` edge"),
        "{solo}"
    );

    let j = reach_json(&dir, "quest:letter");
    assert_eq!(j["reach"], "reachable", "{j}");
    assert_eq!(j["evidence"], "proven", "{j}");
    assert_eq!(
        j["anchors"],
        serde_json::json!([{ "kind": "accept", "from": ["quest(intro)"], "host": "always" }]),
        "{j}"
    );
    assert_eq!(j["prereq"], serde_json::Value::Null, "{j}");

    // The graph is 0.52's: the accept edge closes a cycle and stays undrawn.
    let g = graph(&dir);
    assert!(
        g.contains(
            "    layer 0: quest(letter)\n    layer 1: quest(intro)\n  edges (prerequisite -> \
             dependent) [atom kind(s)]:\n    quest(letter) -> quest(intro) [start]\n"
        ),
        "{g}"
    );
    assert!(g.contains("unanchored"), "{g}");
    assert!(g.contains("    quest(solo)\n"), "{g}");
}

// ---- §7.3: in-script accept deadlocks ------------------------------------

/// The same project without `accept="external"` on `letter`. v0.52.0:
/// clean, both `Unknown`, `letter` in layer 0.
#[test]
fn a_start_and_reward_accept_cycle_is_a_start_cycle() {
    let quests = format!(
        "{}{}",
        quest(
            "intro",
            " start=\"quest.letter.state == 'active'\"",
            &offer("letter")
        ),
        quest("letter", "", ""),
    );
    let dir = project("q227-cycle", &quests, "", &[]);
    let (code, out) = check(&dir);
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains(
            "./quests/q.lute:7:36: error [E-QUEST-START-CYCLE] quest start cycle: quest(intro) \
             -[start]-> quest(letter) -[accept]-> quest(intro) — each waits for the next (a \
             `start` for its condition, an accept for the `::accept` or reward that grants it) \
             and no source outside the cycle starts any of them; start one of them from \
             something outside the cycle, or let the host grant one with `accept=\"external\"`\n"
        ),
        "{out}"
    );
    assert_eq!(out.matches("E-QUEST-START-CYCLE").count(), 1, "{out}");

    let g = graph(&dir);
    assert!(
        g.contains(
            "    (unlayered -- part of a start cycle, E-QUEST-START-CYCLE): quest(intro), \
             quest(letter)\n"
        ),
        "{g}"
    );
    assert!(!g.contains("layer 0: quest(letter)"), "{g}");
    let letter = reach(&dir, "quest:letter");
    assert!(
        letter.contains(
            "verdict: Unknown — this quest's accept waits on a start cycle \
             (E-QUEST-START-CYCLE): quest(letter) -[accept]-> quest(intro) -[start]-> \
             quest(letter); it can never start under your declared routes. [unknown]\n"
        ),
        "{letter}"
    );
    let intro = reach(&dir, "quest:intro");
    assert!(
        intro.contains(
            "verdict: Unknown — this quest's `start` waits on a start cycle \
             (E-QUEST-START-CYCLE): quest(intro) -[start]-> quest(letter) -[accept]-> \
             quest(intro); it can never start under your declared routes. [unknown]\n"
        ),
        "{intro}"
    );
}

/// A third quest that starts at once and also accepts `letter` enters the
/// cycle: no error, both reachable.
#[test]
fn a_start_accept_cycle_entered_from_outside_is_no_error() {
    let quests = format!(
        "{}{}{}",
        quest(
            "intro",
            " start=\"quest.letter.state == 'active'\"",
            &offer("letter")
        ),
        quest("letter", "", ""),
        quest("opener", " start=\"true\"", &offer("letter")),
    );
    let dir = project("q227-entered", &quests, "", &[]);
    let (code, out) = check(&dir);
    assert_eq!(code, 0, "{out}");
    assert!(!out.contains("E-QUEST-START-CYCLE"), "{out}");
    for node in ["quest:letter", "quest:intro"] {
        let r = reach(&dir, node);
        assert!(r.contains(REACHABLE), "{node}: {r}");
    }
}

/// Two accept-driven quests whose rewards accept each other: the error is
/// at the least quest's `id`. v0.52.0: clean, both `Unknown`.
#[test]
fn a_mutual_reward_accept_deadlock_is_reported_at_the_least_id() {
    let quests = format!(
        "{}{}",
        quest("a", "", &offer("b")),
        quest("b", "", &offer("a"))
    );
    let dir = project("q227-mutual", &quests, "", &[]);
    let (code, out) = check(&dir);
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains(
            "./quests/q.lute:7:12: error [E-QUEST-START-CYCLE] quest start cycle: quest(a) \
             -[accept]-> quest(b) -[accept]-> quest(a) — each waits for the next"
        ),
        "{out}"
    );
    assert_eq!(out.matches("E-QUEST-START-CYCLE").count(), 1, "{out}");
}

// ---- §7.2: reach over undrawn accept edges ------------------------------

/// The host on either side of a mutual-accept cycle reaches both quests,
/// and `constraints` agrees. v0.52.0: `Unknown` for both (no host route);
/// without the repeat over undrawn accept edges, the placement on `a`
/// leaves `b` `Unknown` (the graph draws `b -> a` and evaluates `b` first).
#[test]
fn the_host_on_either_side_of_a_mutual_accept_cycle_reaches_both() {
    let constraints = "constraints:\n  - id: a-reachable\n    kind: reachable\n    node: \
                       quest:a\n  - id: b-reachable\n    kind: reachable\n    node: quest:b\n";
    for (tag, a, b) in [
        ("q227-host-a", " accept=\"external\"", ""),
        ("q227-host-b", "", " accept=\"external\""),
    ] {
        let quests = format!(
            "{}{}",
            quest("a", a, &offer("b")),
            quest("b", b, &offer("a"))
        );
        let dir = project(tag, &quests, constraints, &[]);
        let (code, out) = check(&dir);
        assert_eq!(code, 0, "{tag}: {out}");
        for node in ["quest:a", "quest:b"] {
            let r = reach(&dir, node);
            assert!(r.contains(REACHABLE), "{tag} {node}: {r}");
        }
        let out = text(&lute(&dir, &["constraints", "--project", "."]));
        assert_eq!(
            out, "a-reachable: reachable holds [proven]\nb-reachable: reachable holds [proven]\n",
            "{tag}"
        );
    }
}

// ---- §7.1, §7.4: a host-gated grant --------------------------------------

/// `accept="external" follows="completed('intro')"`: clean; the host route
/// with the formula, evidence `heuristic`; the `reachable` constraint keeps
/// it. v0.52.0: `proven`, "connectivity proves reachable".
#[test]
fn a_host_gated_grant_is_heuristic_in_reach_and_constraints() {
    let quests = format!(
        "{}{}",
        quest("intro", " start=\"visited('opening')\"", ""),
        quest(
            "letter",
            " accept=\"external\" follows=\"completed('intro')\"",
            ""
        ),
    );
    let constraints =
        "constraints:\n  - id: letter-reachable\n    kind: reachable\n    node: quest:letter\n";
    let dir = project("q227-gated", &quests, constraints, &[OPENING]);
    let (code, out) = check(&dir);
    assert_eq!(code, 0, "{out}");
    assert!(!out.contains("error"), "{out}");

    let r = reach(&dir, "quest:letter");
    assert!(
        r.contains(
            "  verdict: Reachable — reachable if the host grants it once `follows` holds (host \
             policy; Lute does not enforce it). [heuristic]\n  follows: completed(\"intro\") \
             (declared ordering; not an activation route)\n  anchored; each anchor holds before \
             it activates, through any one of its sources:\n    [accept] the host once follows: \
             completed(\"intro\")\n"
        ),
        "{r}"
    );
    let j = reach_json(&dir, "quest:letter");
    assert_eq!(j["evidence"], "heuristic", "{j}");
    assert_eq!(j["reach"], "reachable", "{j}");
    assert_eq!(j["prereq"], "completed(\"intro\")", "{j}");
    assert_eq!(
        j["anchors"],
        serde_json::json!([{
            "kind": "accept", "from": [], "host": { "follows": "completed(\"intro\")" }
        }]),
        "{j}"
    );

    let report = json(&dir, &["constraints", "--project", ".", "--json"]);
    let c = &report["constraints"][0];
    assert_eq!(c["verdict"], "holds", "{report}");
    assert_eq!(c["evidence"], "heuristic", "{report}");
    assert_eq!(
        c["reason"],
        "Reachable — reachable if the host grants it once `follows` holds (host policy; Lute \
         does not enforce it).",
        "{report}"
    );
    let out = text(&lute(&dir, &["constraints", "--project", "."]));
    assert!(
        out.starts_with("letter-reachable: reachable holds [heuristic]\n"),
        "{out}"
    );

    // The `follows` edge is drawn as 0.52 drew it.
    let g = graph(&dir);
    assert!(
        g.contains("    quest(intro) -> quest(letter) [completed]\n"),
        "{g}"
    );
}

/// A scene `intro` quests start from (a `start` anchor makes them nodes).
const OPENING: (&str, &str) = (
    "scenes/opening.lute",
    "---\nkind: scene\nid: opening\n---\n\n## O {#o}\n\n@mira: Hello.\n",
);

// ---- §7.1: `follows` beside an in-script accept ---------------------------

/// A reward-accepted quest (not external) with `follows`, paid by a quest
/// other than `intro` that starts at once. `intro` is a graph node through
/// its `start` anchor.
fn reward_accepted_with_follows(tag: &str, follows: &str) -> lute_scratch::ScratchDir {
    let quests = format!(
        "<quest id=\"intro\" title=\"Q\" start=\"visited('opening')\">\n  <objective id=\"o\" \
         title=\"Do it\" done=\"run.never\"/>\n  <on event=\"questComplete\">\n    \
         ::set{{run.done=true}}\n  </on>\n</quest>\n\n{}<quest id=\"gift\" title=\"Q\" \
         follows=\"{follows}\">\n  <objective id=\"o\" title=\"Do it\" \
         done=\"run.flag\"/>\n</quest>\n\n",
        quest("giver", " start=\"true\"", &offer("gift")),
    );
    project(
        tag,
        &quests,
        "",
        &[
            OPENING,
            (
                "script.play.yaml",
                "steps:\n  - engine: { state: { run.never: false } }\n    expect: { quests: { \
                 giver: complete, gift: active } }\n",
            ),
        ],
    )
}

/// The reward route, never `follows`: `reach` `Reachable` (evidence
/// unchanged), its reason names no `follows`; the declared `follows` stays
/// in the text, the JSON `prereq` and the graph's edge. v0.52.0: the route
/// was `follows` (prereq anchored nothing), no accept anchor.
#[test]
fn a_reward_accept_is_a_route_beside_follows_which_stays_metadata() {
    let dir = reward_accepted_with_follows("q227-follows", "completed('intro')");
    let (code, out) = check(&dir);
    assert_eq!(code, 0, "{out}");

    let r = reach(&dir, "quest:gift");
    assert!(
        r.contains(&format!(
            "  {REACHABLE}  follows: completed(\"intro\") (declared ordering; not an activation \
             route)\n  anchored; each anchor holds before it activates, through any one of its \
             sources:\n    [accept] quest(giver)\n  referenced node(s) (see `follows` and the \
             anchors above — this is NOT a flat requirement list):\n    - quest(giver): \
             Reachable"
        )),
        "{r}"
    );
    assert!(r.contains("    - quest(intro): "), "{r}");
    let j = reach_json(&dir, "quest:gift");
    assert_eq!(j["evidence"], "proven", "{j}");
    assert_eq!(j["prereq"], "completed(\"intro\")", "{j}");
    assert_eq!(
        j["anchors"],
        serde_json::json!([{ "kind": "accept", "from": ["quest(giver)"] }]),
        "{j}"
    );

    // `lute scenario`: the `follows` edge as 0.52 drew it, plus the accept edge.
    let g = graph(&dir);
    assert!(
        g.contains("    quest(intro) -> quest(gift) [completed]\n"),
        "{g}"
    );
    assert!(
        g.contains("    quest(giver) -> quest(gift) [accept]\n"),
        "{g}"
    );
    let full = json(&dir, &["scenario", "--project", ".", "--json"]);
    let edge = find(&full, &|o| {
        o.get("from") == Some(&"quest(intro)".into()) && o.get("to") == Some(&"quest(gift)".into())
    });
    assert_eq!(edge["kinds"], serde_json::json!(["completed"]), "{full}");
    let node = find(&full, &|o| {
        o.contains_key("prereq") && o.values().any(|v| v == "quest(gift)")
    });
    assert_eq!(node["prereq"], "completed(\"intro\")", "{full}");

    // The envelope takes nothing from `follows`: `intro`'s completion write
    // is no entry guarantee of `gift`.
    let env = text(&lute(
        &dir,
        &[
            "scenario",
            "--project",
            ".",
            "envelope",
            "--node",
            "quest:gift",
        ],
    ));
    assert!(!env.contains("run.done"), "{env}");

    // `lute play`: the reward activates `gift` while `intro` never completes.
    let out = lute(
        &dir,
        &["play", "--project", ".", "--script", "script.play.yaml"],
    );
    let play = text(&out);
    assert!(out.status.success(), "{play}");
    assert!(
        play.contains("quest gift accepted  (effect of reward OFFER_QUEST)"),
        "{play}"
    );
    assert!(!play.contains("quest intro complete"), "{play}");
}

/// The same quest when `intro` is a declared quest on no graph edge (its
/// `start` anchors nothing): the omitted reference stays listed.
#[test]
fn a_reward_accepted_quest_keeps_its_omitted_follows_reference() {
    let quests = format!(
        "{}{}{}",
        quest("intro", " start=\"true\"", ""),
        quest("giver", " start=\"true\"", &offer("gift")),
        quest("gift", " follows=\"completed('intro')\"", ""),
    );
    let dir = project("q227-omitted", &quests, "", &[]);
    let (code, out) = check(&dir);
    assert_eq!(code, 0, "{out}");
    let g = graph(&dir);
    assert!(
        g.contains(
            "    quest(gift) -> completed(\"intro\") — quest(intro) is on no edge (no `follows`, \
             tree, `start` anchor or `::accept`)\n"
        ),
        "{g}"
    );
    assert!(
        g.contains("    quest(giver) -> quest(gift) [accept]\n"),
        "{g}"
    );
}

/// A `start`-driven quest with `follows` keeps its 0.52 graph and route.
#[test]
fn a_start_driven_quest_keeps_its_follows_edge_and_route() {
    let quests = format!(
        "{}{}",
        quest(
            "intro",
            " start=\"true\" follows=\"visited('opening')\"",
            ""
        ),
        quest("next", " start=\"true\" follows=\"completed('intro')\"", ""),
    );
    let dir = project(
        "q227-start-follows",
        &quests,
        "",
        &[(
            "scenes/opening.lute",
            "---\nkind: scene\nid: opening\n---\n\n## O {#o}\n\n@mira: Hello.\n",
        )],
    );
    let (code, out) = check(&dir);
    assert_eq!(code, 0, "{out}");
    let g = graph(&dir);
    assert!(
        g.contains("    quest(intro) -> quest(next) [completed]\n"),
        "{g}"
    );
    let r = reach(&dir, "quest:next");
    assert!(
        r.contains(&format!("  {REACHABLE}  follows: completed(\"intro\")\n")),
        "{r}"
    );
}

/// A malformed `follows` beside an accept anchor, non-external and
/// external: the JSON marker and the reach marker stay beside the anchor,
/// no edge or omitted reference comes from it, the external host route is
/// `Unknown`, and the one `E-CONN-PROFILE` is reported once. v0.52.0: the
/// JSON `prereq` was the marker only because nothing anchored the quest.
#[test]
fn a_malformed_follows_stays_visible_beside_the_accept_anchor() {
    let quests = format!(
        "{}{}{}",
        quest("giver", " start=\"true\"", &offer("gift")),
        quest("gift", " follows=\"completed(\"", ""),
        quest("letter", " accept=\"external\" follows=\"completed(\"", ""),
    );
    let dir = project("q227-malformed", &quests, "", &[]);
    let (code, out) = check(&dir);
    assert_eq!(code, 1, "{out}");
    assert_eq!(out.matches("[E-CONN-PROFILE]").count(), 2, "{out}");
    let errors = out.lines().filter(|l| l.contains(": error [")).count();
    assert_eq!(errors, 2, "{out}");

    let marker = "(malformed — E-CONN-PROFILE)";
    let gift = reach(&dir, "quest:gift");
    assert!(
        gift.contains(&format!(
            "  follows: {marker} (declared ordering; not an activation route)\n  anchored; each \
             anchor holds before it activates, through any one of its sources:\n    [accept] \
             quest(giver)\n"
        )),
        "{gift}"
    );
    assert!(gift.contains(REACHABLE), "{gift}");
    let letter = reach(&dir, "quest:letter");
    assert!(
        letter.contains(&format!(
            "  follows: {marker} (declared ordering; not an activation route)\n  anchored; each \
             anchor holds before it activates, through any one of its sources:\n    [accept] the \
             host once follows: {marker}\n"
        )),
        "{letter}"
    );
    assert!(letter.contains("verdict: Unknown"), "{letter}");
    for (node, from) in [
        ("quest:gift", serde_json::json!(["quest(giver)"])),
        ("quest:letter", serde_json::json!([])),
    ] {
        let j = reach_json(&dir, node);
        assert_eq!(j["prereq"], marker, "{j}");
        assert_eq!(j["anchors"][0]["kind"], "accept", "{j}");
        assert_eq!(j["anchors"][0]["from"], from, "{j}");
    }
    let full = json(&dir, &["scenario", "--project", ".", "--json"]);
    for quest in ["quest(gift)", "quest(letter)"] {
        let node = find(&full, &|o| {
            o.contains_key("prereq") && o.values().any(|v| v == quest)
        });
        assert_eq!(node["prereq"], marker, "{full}");
    }
    let g = graph(&dir);
    assert!(!g.contains("note:"), "{g}");
    assert!(!g.contains("[completed]"), "{g}");
}

// ---- §7.4 (a): messages ---------------------------------------------------

/// The issue's second project (#227 a): both refusals stay, with the
/// `follows=` advice.
#[test]
fn the_refusals_name_follows_as_the_hosts_condition() {
    let quests = "<quest id=\"letter\" title=\"A letter\" accept=\"external\" \
                  start=\"quest.intro.state == 'complete'\">\n  <objective id=\"open\" \
                  title=\"Open it\" done=\"true\"/>\n</quest>\n\n<quest id=\"intro\" \
                  title=\"Intro\" start=\"true\">\n  <objective id=\"read\" title=\"Read it\" \
                  done=\"true\"/>\n  <reward kind=\"OFFER_QUEST\" target=\"gated\"/>\n</quest>\n\n\
                  <quest id=\"gated\" title=\"Gated\" start=\"quest.intro.state == 'active'\">\n  \
                  <objective id=\"go\" title=\"Go\" done=\"true\"/>\n</quest>\n";
    let dir = project("q227-a", quests, "", &[]);
    let (code, out) = check(&dir);
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains(
            "./quests/q.lute:7:45: error [E-ATTR-TYPE] `<quest id=\"letter\">` carries both \
             `accept=\"external\"` and `start`: a `start` condition activates a quest by itself, \
             so the host could never decide; drop `start`. If the host grants it only once some \
             scenes or quests are done, write that as `follows=\"…\"` (for example \
             `follows=\"completed('intro')\"`): the host applies it, and analysis reads it as the \
             host's condition\n"
        ),
        "{out}"
    );
    assert!(
        out.contains(
            "./quests/q.lute:13:38: error [E-ACCEPT-TARGET] `<reward kind=\"OFFER_QUEST\" \
             target=\"gated\">` accepts quest `gated` (its kind's `effects.accepts`), which has a \
             `start` condition and activates on its own; only an accept-driven quest (no \
             `start`) can be accepted; drop its `start` — the accept activates it whatever other \
             condition it had (a `follows=\"…\"` records such conditions for analysis and the \
             host, but does not gate the accept)\n"
        ),
        "{out}"
    );

    // Both suggested spellings check clean.
    let fixed = "<quest id=\"letter\" title=\"A letter\" accept=\"external\" \
                 follows=\"completed('intro')\">\n  <objective id=\"open\" title=\"Open it\" \
                 done=\"true\"/>\n</quest>\n\n<quest id=\"intro\" title=\"Intro\" \
                 start=\"true\">\n  <objective id=\"read\" title=\"Read it\" done=\"true\"/>\n  \
                 <reward kind=\"OFFER_QUEST\" target=\"gated\"/>\n</quest>\n\n<quest \
                 id=\"gated\" title=\"Gated\" follows=\"active('intro')\">\n  <objective \
                 id=\"go\" title=\"Go\" done=\"true\"/>\n</quest>\n";
    let dir = project("q227-a-fixed", fixed, "", &[]);
    let (code, out) = check(&dir);
    assert_eq!(code, 0, "{out}");
    assert!(!out.contains("error"), "{out}");
}

/// `E-QUEST-START-SELF` names `follows=` as host policy, and its spelling
/// checks clean.
#[test]
fn the_self_read_names_follows_as_the_hosts_condition() {
    let dir = project(
        "q227-self",
        &quest("board", " start=\"quest.board.state == 'active'\"", ""),
        "",
        &[],
    );
    let (code, out) = check(&dir);
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains(
            "[E-QUEST-START-SELF] `start` of quest `board` reads its own state \
             (`quest.board.state`), which has no value before the quest starts, so this \
             condition can never start it as intended; a quest that only the host grants is \
             written `accept=\"external\"` without a `start`; if the host grants it only once \
             other scenes or quests are done, write those as `follows=\"…\"`, which the host \
             applies and analysis reads\n"
        ),
        "{out}"
    );
    let fixed = format!(
        "{}{}",
        quest("intro", " start=\"true\"", ""),
        quest(
            "board",
            " accept=\"external\" follows=\"completed('intro')\"",
            ""
        ),
    );
    let dir = project("q227-self-fixed", &fixed, "", &[]);
    let (code, out) = check(&dir);
    assert_eq!(code, 0, "{out}");
}

/// QReview053 Q1 (§7.1): a non-external accept-driven quest with `follows`
/// and nothing accepting it is never activated: reach `Unknown`, its
/// reachable constraint does not hold, `W-QUEST-NEVER-ACCEPTED` stays, and
/// the declared `follows` stays metadata with its edge. Failing before:
/// `Reachable [proven]` and the constraint held.
#[test]
fn a_source_less_accept_driven_quest_has_no_follows_route() {
    let quests = format!(
        "{}{}",
        quest("intro", " start=\"visited('opening')\"", ""),
        quest("gift", " follows=\"completed('intro')\"", ""),
    );
    let constraints =
        "constraints:\n  - id: gift-reachable\n    kind: reachable\n    node: quest:gift\n";
    let dir = project("q-sourceless", &quests, constraints, &[OPENING]);
    let (_, out) = check(&dir);
    assert!(out.contains("W-QUEST-NEVER-ACCEPTED"), "{out}");

    let j = reach_json(&dir, "quest:gift");
    assert_eq!(j["reach"], "unknown", "{j}");
    assert_eq!(j["prereq"], "completed(\"intro\")", "{j}");
    assert_eq!(
        j["anchors"],
        serde_json::json!([{ "kind": "accept", "from": [] }]),
        "{j}"
    );
    let report = json(&dir, &["constraints", "--project", ".", "--json"]);
    assert_ne!(report["constraints"][0]["verdict"], "holds", "{report}");
    let g = graph(&dir);
    assert!(
        g.contains("    quest(intro) -> quest(gift) [completed]\n"),
        "{g}"
    );
}
