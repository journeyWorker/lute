//! dsl 0.26.0 §5: a beat targeting a whole kind (`target="kind:bug"`)
//! answers every member of the kind in `lute play`, reads the raised member
//! as `occasion.target` in its `when`, body and text, and ranks after a
//! member-specific beat of the same priority; the compiled artifact and
//! project index carry the resolved `targetKind`, and `lute beats` lists the
//! kind's own ladder.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{json, Value as Json};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-kt026-{tag}-{}-{n}", std::process::id()));
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

/// `caught` is raised for `mon.<species>`; `bug` is a sub-kind of
/// `species`. `dex.bug` answers every bug and scores by the member; `dex.bee`
/// names one bug at the same priority.
fn project(tag: &str) -> PathBuf {
    let dir = temp_dir(tag);
    write(
        &dir,
        "lute.project.yaml",
        "pluginsDir: plugins/\ndefaultProfile: g\nprofiles:\n  g:\n    plugins: { g.dex: true }\n",
    );
    write(
        &dir,
        "plugins/g.dex/plugin.yaml",
        "id: g.dex\nversion: 0.1.0\nkind: capability\ndepends: [ { id: lute.core, range: \"^0.0.1\" } ]\n\
         exports:\n  occasions: occasions/\n",
    );
    write(
        &dir,
        "plugins/g.dex/occasions/o.yaml",
        "occasions:\n  caught: { target: { prefix: mon, entity: species } }\n",
    );
    write(
        &dir,
        "world.schema.yaml",
        "state:\n  run.score: { type: int, default: 0 }\nentities:\n  species: { members: [ant, bee, cat] }\n  \
         bug: { subsetOf: species, members: [ant, bee] }\n",
    );
    write(
        &dir,
        "lore/catch.lute",
        "---\nkind: lore\nid: dex\nuses: ../world.schema.yaml\n---\n\n\
         <beat id=\"bug\" on=\"caught\" target=\"kind:bug\" priority=\"5\" once=\"false\" \
         when=\"occasion.target != 'bee' || run.score >= 1\">\n\
         \x20 <match subject=\"occasion.target\">\n\
         \x20   <when is=\"ant\">\n      ::set{run.score += 1}\n    </when>\n\
         \x20   <when is=\"bee\">\n      ::set{run.score += 10}\n    </when>\n\
         \x20 </match>\n\
         \x20 @narrator: A {{occasion.target}}.\n\
         </beat>\n\n\
         <beat id=\"bee\" on=\"caught\" target=\"mon.bee\" priority=\"5\" once=\"false\">\n\
         \x20 @narrator: The bee, by name.\n\
         </beat>\n",
    );
    dir
}

fn play(dir: &Path, script: &str) -> Json {
    let script = write(dir, "s.play.yaml", script);
    let out = run(&[
        "play",
        dir.to_str().unwrap(),
        "--script",
        script.to_str().unwrap(),
        "--json",
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    serde_json::from_slice(&out.stdout).unwrap()
}

fn candidates(step: &Json) -> Vec<(&str, bool)> {
    step["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| (c["id"].as_str().unwrap(), c["eligible"] == true))
        .collect()
}

fn lines(step: &Json) -> Vec<&str> {
    step["presented"]["commands"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["kind"] == "line")
        .map(|r| r["text"].as_str().unwrap())
        .collect()
}

#[test]
fn a_kind_beat_answers_each_member_and_reads_it_as_occasion_target() {
    let dir = project("members");
    let v = play(
        &dir,
        "steps:\n  - occasion: caught\n    target: mon.ant\n  - occasion: caught\n    target: mon.cat\n",
    );
    let ant = &v["steps"][0];
    assert_eq!(ant["winner"], "dex.bug", "{ant}");
    // The body's `<match subject="occasion.target">` and `{{occasion.target}}`
    // both see the raised member, prefix stripped.
    assert_eq!(lines(ant), ["A ant."], "{ant}");
    assert_eq!(
        ant["presented"]["stateDelta"],
        json!({ "run.score": 1 }),
        "{ant}"
    );
    // A species outside the kind raises nothing the kind beat answers.
    assert!(candidates(&v["steps"][1]).is_empty(), "{}", v["steps"][1]);
}

#[test]
fn the_member_specific_beat_outranks_the_kind_beat_at_equal_priority() {
    let dir = project("rank");
    // `dex.bee` follows `dex.bug` in the index, yet ranks first on `mon.bee`.
    // The kind beat's `when` reads the member: false for a bee at score 0.
    let v = play(
        &dir,
        "steps:\n  - occasion: caught\n    target: mon.bee\n  - occasion: caught\n    target: mon.ant\n  \
         - occasion: caught\n    target: mon.bee\n",
    );
    assert_eq!(
        candidates(&v["steps"][0]),
        [("dex.bee", true), ("dex.bug", false)],
        "{}",
        v["steps"][0]
    );
    assert_eq!(
        candidates(&v["steps"][2]),
        [("dex.bee", true), ("dex.bug", true)],
        "{}",
        v["steps"][2]
    );
    assert_eq!(v["steps"][2]["winner"], "dex.bee");
}

#[test]
fn the_artifact_and_the_index_carry_the_resolved_kind() {
    let dir = project("ir");
    let out_dir = dir.join("out");
    let out = run(&[
        "compile",
        "--all",
        "--project",
        dir.to_str().unwrap(),
        "-o",
        out_dir.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let kind = json!({ "kind": "bug", "prefix": "mon", "members": ["ant", "bee"] });
    let index: Json =
        serde_json::from_str(&std::fs::read_to_string(out_dir.join("project.index.json")).unwrap())
            .unwrap();
    let rows = index["beats"].as_array().unwrap();
    let row = |id: &str| rows.iter().find(|b| b["id"] == id).unwrap();
    assert_eq!(row("dex.bug")["targetKind"], kind, "{index}");
    assert!(row("dex.bee").get("targetKind").is_none(), "{index}");
    let art: Json = serde_json::from_str(
        &std::fs::read_to_string(out_dir.join("lore/catch.lute.json")).unwrap(),
    )
    .unwrap();
    let head = art["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["kind"] == "beat" && c["id"] == "dex.bug")
        .unwrap();
    assert_eq!(head["target"], "kind:bug");
    assert_eq!(head["targetKind"], kind);
    // Prerelease N8: `{{occasion.target}}` carries the kind it ranges over.
    let line = art["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["kind"] == "line" && c["text"] == "A {{occasion.target}}.")
        .unwrap();
    assert_eq!(
        line["placeholders"],
        json!([{ "kind": "occasionTarget", "entityKind": "bug" }]),
        "{line}"
    );
}

/// Prerelease N8: play renders the raised member by its cast display name
/// when the member is a cast id, else by the id.
#[test]
fn play_renders_the_occasion_target_by_its_cast_name() {
    let dir = project("cast-name");
    let world = std::fs::read_to_string(dir.join("world.schema.yaml")).unwrap();
    write(
        &dir,
        "world.schema.yaml",
        &format!("{world}cast:\n  ant: {{ name: Inchlet }}\n"),
    );
    let v = play(
        &dir,
        "steps:\n  - occasion: caught\n    target: mon.ant\n  - occasion: caught\n    target: mon.bee\n",
    );
    assert_eq!(lines(&v["steps"][0]), ["A Inchlet."], "{}", v["steps"][0]);
}

#[test]
fn lute_beats_lists_the_kind_ladder_and_each_named_member() {
    let dir = project("beats");
    let out = run(&[
        "beats",
        dir.to_str().unwrap(),
        "--occasion",
        "caught",
        "--json",
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let v: Json = serde_json::from_slice(&out.stdout).unwrap();
    let ladders: Vec<(String, Vec<String>)> = v["roots"][0]["ladders"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| {
            (
                l["target"].as_str().unwrap().to_string(),
                l["beats"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|b| b["id"].as_str().unwrap().to_string())
                    .collect(),
            )
        })
        .collect();
    assert_eq!(
        ladders,
        [
            (
                "mon.bee".to_string(),
                vec!["dex.bee".to_string(), "dex.bug".to_string()]
            ),
            ("kind:bug".to_string(), vec!["dex.bug".to_string()]),
        ],
        "{v}"
    );
}

/// `summon` is raised for `hero.<member>`; `ssr` is a sub-kind of `hero`,
/// `limited` of `ssr`. `beats` is the lore body.
fn gacha(tag: &str, beats: &str) -> PathBuf {
    let dir = temp_dir(tag);
    write(
        &dir,
        "lute.project.yaml",
        "pluginsDir: plugins/\ndefaultProfile: g\nprofiles:\n  g:\n    plugins: { g.gacha: true }\n",
    );
    write(
        &dir,
        "plugins/g.gacha/plugin.yaml",
        "id: g.gacha\nversion: 0.1.0\nkind: capability\ndepends: [ { id: lute.core, range: \"^0.0.1\" } ]\n\
         exports:\n  occasions: occasions/\n",
    );
    write(
        &dir,
        "plugins/g.gacha/occasions/o.yaml",
        "occasions:\n  summon: { target: { prefix: hero, entity: hero } }\n",
    );
    write(
        &dir,
        "world.schema.yaml",
        "entities:\n  hero: { members: [aria, bram, cyra] }\n  \
         ssr: { subsetOf: hero, members: [aria, cyra] }\n  \
         limited: { subsetOf: ssr, members: [cyra] }\n",
    );
    write(
        &dir,
        "lore/summons.lute",
        &format!("---\nkind: lore\nid: s\nuses: ../world.schema.yaml\n---\n\n{beats}"),
    );
    dir
}

fn summon_beat(id: &str, target: &str, priority: i64) -> String {
    format!(
        "<beat id=\"{id}\" on=\"summon\" target=\"{target}\" once=\"false\" priority=\"{priority}\">\n\
         \x20 @narrator: {id}.\n</beat>\n\n"
    )
}

/// dsl 0.27.0 (T3-10): member > sub-kind > kind. The parent kind's beat is
/// first in the file, yet each member hears its most specific kind's beat,
/// and the checker calls none of them shadowed.
#[test]
fn play_picks_the_sub_kind_beat_over_the_parent_kind_beat() {
    let dir = gacha(
        "subkind",
        &[
            summon_beat("blue", "kind:hero", 0),
            summon_beat("gold", "kind:ssr", 0),
            summon_beat("silver", "kind:limited", 0),
        ]
        .concat(),
    );
    let v = play(
        &dir,
        "steps:\n  - occasion: summon\n    target: hero.aria\n  - occasion: summon\n    target: hero.cyra\n  \
         - occasion: summon\n    target: hero.bram\n",
    );
    let winners: Vec<&str> = v["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["winner"].as_str().unwrap())
        .collect();
    assert_eq!(winners, ["s.gold", "s.silver", "s.blue"], "{v}");
    assert_eq!(
        candidates(&v["steps"][1]),
        [("s.silver", true), ("s.gold", true), ("s.blue", true)]
    );
    let t = text(&run(&["check-project", dir.to_str().unwrap()]));
    assert!(!t.contains("W-BEAT-SHADOWED"), "{t}");
    assert!(!t.contains("W-BEAT-PRIORITY-TIE"), "{t}");
}

/// dsl 0.27.0 (T3-9): a kind beat that wins for one member but never for
/// another is shadowed on that member's ladder only — no project-wide
/// warning — and its row names the kind it answers through.
#[test]
fn lute_beats_gives_each_ladder_cell_its_own_verdict() {
    let dir = gacha(
        "cells",
        &[
            summon_beat("limitedGlow", "kind:limited", 2),
            summon_beat("goldLight", "kind:ssr", 1),
            summon_beat("cyraA", "hero.cyra", -5),
        ]
        .concat(),
    );
    let d = dir.to_str().unwrap();
    let t = text(&run(&["check-project", d]));
    assert!(!t.contains("`s.goldLight` can never win"), "{t}");
    let out = run(&["beats", d, "--target", "hero.cyra"]);
    let t = text(&out);
    let row = |id: &str| t.lines().find(|l| l.contains(id)).unwrap().to_string();
    assert!(row("s.goldLight").contains("s.goldLight (kind:ssr)"), "{t}");
    assert!(
        row("s.goldLight").contains("shadowed by s.limitedGlow"),
        "{t}"
    );
    assert!(row("s.cyraA").contains("shadowed by s.limitedGlow"), "{t}");
    assert!(!row("s.limitedGlow").contains("shadowed"), "{t}");
    let shadowed_by = |target: &str| -> Json {
        let out = run(&["beats", d, "--target", target, "--json"]);
        let v: Json = serde_json::from_slice(&out.stdout).unwrap();
        v["roots"][0]["ladders"][0]["beats"]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["id"] == "s.goldLight")
            .unwrap()
            .get("shadowedBy")
            .cloned()
            .unwrap_or(Json::Null)
    };
    assert_eq!(shadowed_by("hero.cyra"), json!(["s.limitedGlow"]));
    assert_eq!(shadowed_by("hero.aria"), Json::Null);
    let out = run(&["beats", d, "--target", "hero.cyra", "--json"]);
    let v: Json = serde_json::from_slice(&out.stdout).unwrap();
    let row = v["roots"][0]["ladders"][0]["beats"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["id"] == "s.goldLight")
        .unwrap();
    assert_eq!(row["shadowedByEvidence"], "heuristic");
}

/// dsl 0.28.0: a kind beat whose `when` never holds for one member — as
/// written, or because no fact it needs is produced for that member — is
/// marked `never for` on that member's ladder only; `check-project` calls it
/// reachable (it plays for the others).
#[test]
fn lute_beats_marks_the_members_a_kind_beat_never_plays_for() {
    let dir = gacha(
        "never",
        "<beat id=\"notBram\" on=\"summon\" target=\"kind:hero\" once=\"false\" \
         when=\"occasion.target != 'bram'\">\n  @narrator: Not Bram.\n</beat>\n\n\
         <beat id=\"fan\" on=\"summon\" target=\"kind:hero\" once=\"false\" priority=\"-1\" \
         when=\"holds('fan', [occasion.target])\">\n  @narrator: A fan.\n</beat>\n\n\
         <beat id=\"bramA\" on=\"summon\" target=\"hero.bram\" once=\"false\" priority=\"-2\">\n  \
         @narrator: Bram.\n</beat>\n",
    );
    write(
        &dir,
        "world.schema.yaml",
        "entities:\n  hero: { members: [aria, bram, cyra] }\n\
         relations:\n  fan: { args: [hero], tier: run }\nfacts:\n  - fan(cyra)\n",
    );
    let d = dir.to_str().unwrap();
    let t = text(&run(&["check-project", d]));
    assert!(!t.contains("E-BEAT-UNREACHABLE"), "{t}");
    let never_for = |target: &str| -> Vec<(String, Json)> {
        let out = run(&["beats", d, "--target", target, "--json"]);
        assert_eq!(out.status.code(), Some(0), "{}", text(&out));
        let v: Json = serde_json::from_slice(&out.stdout).unwrap();
        v["roots"][0]["ladders"][0]["beats"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| {
                let never = b.get("neverFor").cloned().unwrap_or(Json::Null);
                (b["id"].as_str().unwrap().to_string(), never)
            })
            .collect()
    };
    assert_eq!(
        never_for("hero.bram"),
        [
            ("s.notBram".to_string(), json!(["hero.bram"])),
            ("s.fan".to_string(), json!(["hero.bram"])),
            ("s.bramA".to_string(), Json::Null),
        ]
    );
    assert_eq!(
        never_for("hero.cyra"),
        [
            ("s.notBram".to_string(), Json::Null),
            ("s.fan".to_string(), Json::Null),
        ]
    );
    let t = text(&run(&["beats", d, "--target", "hero.aria"]));
    let row = |id: &str| t.lines().find(|l| l.contains(id)).unwrap().to_string();
    assert!(row("s.fan").contains("never for hero.aria"), "{t}");
    assert!(!row("s.notBram").contains("never"), "{t}");
}
