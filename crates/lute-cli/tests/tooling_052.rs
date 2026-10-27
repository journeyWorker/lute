//! 0.52.0 §8 (#209, #210, #213, #214): the node envelope's provenance stops
//! at `[accept]` edges, `lute impact` follows `[accept]`/`[raises]`, `loc
//! export` and compile agree on every `lineId`, and `lute test` announces
//! a project only when it discovered it.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn examples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/examples")
}

fn write(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, text).unwrap();
}

fn lute(dir: &Path, args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .current_dir(dir)
        .env("LUTE_TELEMETRY", "0")
        .output()
        .unwrap()
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// The #209 issue project: scenes `a` and `b` each write `run.x` and
/// `::accept` quest `q`, which declares no `follows`.
fn accept_project() -> lute_scratch::ScratchDir {
    let dir = lute_scratch::ScratchDir::named("tooling-accept");
    write(
        &dir,
        "lute.project.yaml",
        "defaultProfile: core\nprofiles: { core: { plugins: {} } }\ndefaults: { uses: \
         [world.schema.yaml] }\n",
    );
    write(
        &dir,
        "world.schema.yaml",
        "state: { run.x: { type: int, default: 0 } }\n",
    );
    for id in ["a", "b"] {
        write(
            &dir,
            &format!("scenes/{id}.lute"),
            &format!(
                "---\nkind: scene\nid: {id}\n---\n\n## S\n\n@narrator: {id}.\n\n::set{{run.x = \
                 1}}\n::accept{{quest=\"q\"}}\n"
            ),
        );
    }
    write(
        &dir,
        "quests/q.lute",
        "---\nkind: quest\nid: q\n---\n\n<quest id=\"q\" tier=\"run\" title=\"Q\">\n  <objective \
         id=\"o\" title=\"O\" done=\"run.x == 1\"/>\n</quest>\n",
    );
    let check = lute(&dir, &["check", "--project", "."]);
    assert!(check.status.success(), "{}", stdout(&check));
    dir
}

/// #209: the accepting scenes are alternatives — `q` is accepted by one of
/// them — so neither is provably before it (v0.51.0 listed both as
/// writers of the guaranteed `run.x`).
#[test]
fn accepting_scenes_are_not_provably_before_the_accepted_quest() {
    let dir = accept_project();
    let out = lute(
        &dir,
        &[
            "scenario",
            "envelope",
            "--project",
            ".",
            "--node",
            "quest:q",
        ],
    );
    let env = stdout(&out);
    assert!(out.status.success(), "{env}{}", stderr(&out));
    assert!(
        env.contains(
            "    - run.x   written by: (nothing on a declared route reaching here — schema \
             default only); also written, but not provably before this node: scene(a), scene(b)\n"
        ),
        "{env}"
    );
}

/// #210: `lute impact` follows the `[accept]` edge `lute scenario` draws:
/// changing scene `a` affects quest `q`, `Proven` (v0.51.0 listed only
/// `a`'s own line). Human and JSON output.
#[test]
fn impact_follows_accept_edges() {
    let dir = accept_project();
    let out = lute(&dir, &["impact", "--project", ".", "--node", "scene:a"]);
    let human = stdout(&out);
    assert!(out.status.success(), "{human}{}", stderr(&out));
    assert!(
        human.contains(
            "quests\n  quest:q [proven]\n    - accepts: scene(a) accepts quest(q) \
             (scenes/a.lute:1)\n"
        ),
        "{human}"
    );
    let out = lute(
        &dir,
        &["impact", "--project", ".", "--node", "scene:a", "--json"],
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let quests = &json["items"]["quests"];
    assert_eq!(quests.as_array().map(Vec::len), Some(1), "{json:#}");
    assert_eq!(quests[0]["key"], "quest:q");
    assert_eq!(quests[0]["evidence"], "proven");
    assert_eq!(quests[0]["reasons"][0]["edge"], "accepts");
    assert_eq!(
        quests[0]["reasons"][0]["reason"],
        "scene(a) accepts quest(q)"
    );
    assert_eq!(quests[0]["reasons"][0]["file"], "scenes/a.lute");
    // `b` accepts `q` too, but is no dependent of `a`.
    let all = json["items"].to_string();
    assert!(!all.contains("scene:b"), "{json:#}");
}

/// S7 review: legacy scenes (no `id:`) are keyed `{character}.{episodeId}`
/// in the scenario graph but by their root-relative path in the semantic
/// graph. `porter.lute` accepts quest `q` and raises `chapter2.lute`
/// (answering `cutscene` for `scene.chapter2`): impact from
/// `scene:scenes/porter.lute` reaches both through their declared nodes,
/// and no node is minted under a scenario key.
#[test]
fn impact_maps_legacy_scene_endpoints_to_their_declared_nodes() {
    let dir = lute_scratch::ScratchDir::named("tooling-legacy-alternatives");
    write(
        &dir,
        "lute.project.yaml",
        "pluginsDir: plugins/\ndefaultProfile: game\nprofiles:\n  game:\n    plugins:\n      \
         acme.stage: true\ndefaults:\n  contentLang: en\n  uses: [world.schema.yaml]\n",
    );
    write(
        &dir,
        "plugins/acme.stage/plugin.yaml",
        "id: acme.stage\nversion: 0.1.0\nkind: capability\ndepends:\n  - { id: lute.core, \
         range: \"^0.0.1\" }\nexports:\n  directives: directives/\n  occasions: occasions/\n",
    );
    write(
        &dir,
        "plugins/acme.stage/occasions/o.yaml",
        "occasions:\n  cutscene: { target: { prefix: scene, entity: cutsceneId }, raisedBy: \
         declared }\n",
    );
    write(
        &dir,
        "plugins/acme.stage/directives/d.yaml",
        "directives:\n  - name: playScene\n    attrs:\n      - { name: scene, type: { entity: \
         cutsceneId }, default: chapter2 }\n    effects: { raises: [{ occasion: cutscene, \
         target: \"@scene\" }] }\n",
    );
    write(
        &dir,
        "world.schema.yaml",
        "state: { run.x: { type: int, default: 0 } }\nentities:\n  cutsceneId: { members: \
         [chapter2] }\n",
    );
    write(
        &dir,
        "scenes/porter.lute",
        "---\nkind: scene\ncharacter: mira\nseason: 1\nepisode: 1\n---\n\n## Shot 1.\n\n\
         @narrator: Porter.\n\n::set{run.x = 1}\n::accept{quest=\"q\"}\n::playScene\n",
    );
    write(
        &dir,
        "scenes/chapter2.lute",
        "---\nkind: scene\ncharacter: mira\nseason: 1\nepisode: 2\non: cutscene\ntarget: \
         scene.chapter2\n---\n\n## Shot 1.\n\n@narrator: Two.\n",
    );
    write(
        &dir,
        "quests/q.lute",
        "---\nkind: quest\nid: q\n---\n\n<quest id=\"q\" tier=\"run\" title=\"Q\">\n  <objective \
         id=\"o\" title=\"O\" done=\"run.x == 1\"/>\n</quest>\n",
    );
    let check = lute(&dir, &["check", "--project", "."]);
    assert!(
        check.status.success(),
        "{}{}",
        stdout(&check),
        stderr(&check)
    );
    let out = lute(
        &dir,
        &[
            "impact",
            "--project",
            ".",
            "--node",
            "scene:scenes/porter.lute",
            "--json",
        ],
    );
    assert!(out.status.success(), "{}{}", stdout(&out), stderr(&out));
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let items: Vec<&serde_json::Value> = json["items"]
        .as_object()
        .unwrap()
        .values()
        .flat_map(|group| group.as_array().unwrap())
        .collect();
    let reason = |key: &str| {
        let item = items
            .iter()
            .find(|item| item["key"] == key)
            .unwrap_or_else(|| panic!("no {key} in {json:#}"));
        (
            item["evidence"].clone(),
            item["reasons"][0]["edge"].clone(),
            item["reasons"][0]["reason"].clone(),
            item["reasons"][0]["file"].clone(),
        )
    };
    assert_eq!(
        reason("quest:q"),
        (
            "proven".into(),
            "accepts".into(),
            "scene(mira.s01ep01) accepts quest(q)".into(),
            "scenes/porter.lute".into()
        )
    );
    assert_eq!(
        reason("scene:scenes/chapter2.lute"),
        (
            "proven".into(),
            "raises".into(),
            "scene(mira.s01ep01) raises scene(mira.s01ep02)".into(),
            "scenes/porter.lute".into()
        )
    );
    let all = json["items"].to_string();
    assert!(!all.contains("scene:mira."), "{json:#}");
}

/// Every `lineId`-like value of a compiled artifact: `lineId`,
/// `promptLineId`, `titleLineId` at any depth (`localized.*.lineId`
/// included).
fn compiled_ids(value: &serde_json::Value, out: &mut BTreeSet<String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, v) in map {
                if let ("lineId" | "promptLineId" | "titleLineId", Some(id)) =
                    (key.as_str(), v.as_str())
                {
                    out.insert(id.to_string());
                }
                compiled_ids(v, out);
            }
        }
        serde_json::Value::Array(items) => {
            for v in items {
                compiled_ids(v, out);
            }
        }
        _ => {}
    }
}

fn json_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            json_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "json") {
            out.push(path);
        }
    }
}

fn projects(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            if path.join("lute.project.yaml").is_file() {
                out.push(path.clone());
            }
            projects(&path, out);
        }
    }
}

/// #213: for every project under `docs/examples` (the examples root itself
/// is a check-dirty demo collection, not one project), every `loc export`
/// unit with a `lineId` names a `lineId` of the project's compiled
/// artifacts — the export and compile identity rules agree (v0.51.0: 4207
/// units, 0 missing).
#[test]
fn loc_export_line_ids_agree_with_compile_over_the_examples() {
    let mut roots = Vec::new();
    projects(&examples(), &mut roots);
    roots.sort();
    assert!(roots.len() >= 20, "{roots:?}");
    let scratch = lute_scratch::ScratchDir::named("tooling-loc");
    let mut units = 0;
    let mut missing = Vec::new();
    for (i, root) in roots.iter().enumerate() {
        let project = root.to_str().unwrap();
        let out_dir = scratch.join(i.to_string());
        let compiled = lute(
            &scratch,
            &[
                "compile",
                "--project",
                project,
                "-o",
                out_dir.to_str().unwrap(),
            ],
        );
        assert!(
            compiled.status.success(),
            "{project}: {}",
            stderr(&compiled)
        );
        let mut have = BTreeSet::new();
        let mut files = Vec::new();
        if out_dir.is_dir() {
            json_files(&out_dir, &mut files);
        }
        for file in files {
            let artifact: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
            compiled_ids(&artifact, &mut have);
        }
        let export = lute(&scratch, &["loc", "export", "--project", project]);
        assert!(export.status.success(), "{project}: {}", stderr(&export));
        let rows: Vec<serde_json::Value> = serde_json::from_slice(&export.stdout).unwrap();
        for id in rows.iter().filter_map(|row| row["lineId"].as_str()) {
            units += 1;
            if !have.contains(id) {
                missing.push(format!("{project}: {id}"));
            }
        }
    }
    assert!(units >= 4000, "{units} units");
    assert!(
        missing.is_empty(),
        "{} missing: {missing:#?}",
        missing.len()
    );
}

const NOTE: &str = "lute: note: scenario tests use project";

/// #214: with `--project`, `lute test` names no discovered project (v0.51.0
/// printed the discovery note, advising `--project`); without it, it does,
/// once. The run is the same either way.
#[test]
fn lute_test_announces_only_a_discovered_project() {
    let project = examples().join("flow-and-text");
    let given = lute(
        &examples(),
        &["test", "--project", "flow-and-text", "--json"],
    );
    assert!(given.status.success(), "{}", stderr(&given));
    assert!(!stderr(&given).contains(NOTE), "{}", stderr(&given));
    let discovered = lute(&project, &["test", "--json"]);
    assert!(discovered.status.success(), "{}", stderr(&discovered));
    assert_eq!(
        stderr(&discovered).matches(NOTE).count(),
        1,
        "{}",
        stderr(&discovered)
    );
    assert!(
        stderr(&discovered)
            .contains("(nearest lute.project.yaml); pass --project to choose another"),
        "{}",
        stderr(&discovered)
    );
    let summary = |out: &Output| -> serde_json::Value {
        serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()["summary"].clone()
    };
    assert_eq!(summary(&given), summary(&discovered));
    assert_eq!(summary(&given)["failed"], 0);
}
