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
        "state:\n  run.score: { type: number, default: 0 }\nentities:\n  species: { members: [ant, bee, cat] }\n  \
         bug: { subsetOf: species, members: [ant, bee] }\n",
    );
    write(
        &dir,
        "lore/catch.lute",
        "---\nkind: lore\nid: dex\nuses: ../world.schema.yaml\n---\n\n\
         <beat id=\"bug\" on=\"caught\" target=\"kind:bug\" priority=\"5\" once=\"false\" \
         when=\"occasion.target != 'bee' || run.score >= 1\">\n\
         \x20 <match on=\"occasion.target\">\n\
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
    // The body's `<match on="occasion.target">` and `{{occasion.target}}`
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
