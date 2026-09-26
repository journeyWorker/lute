//! `lute context` (dsl 0.22.0 §13): besides the capability snapshot, the
//! authoring surface lists the document's defs (type, params, body), relation
//! tiers and `reserved`, `owner: engine` state paths, component signatures,
//! the built-in directives, occasion target domains and descriptions, and
//! every scene / quest / entry id in the project.

use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-context-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_at(dir: &Path, rel: &str, content: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

fn project() -> PathBuf {
    let proj = temp_dir("surface");
    write_at(
        &proj,
        "lute.project.yaml",
        "pluginsDir: plugins/\ndefaultProfile: game\nprofiles:\n  game:\n    plugins: { demo.occasions: true }\n\
         defaults:\n  uses: [world.schema.yaml]\n",
    );
    write_at(
        &proj,
        "plugins/demo.occasions/plugin.yaml",
        "id: demo.occasions\nversion: 0.1.0\nkind: capability\ndepends: [ { id: lute.core, range: \"^0.0.1\" } ]\nexports:\n  occasions: occasions/\n",
    );
    write_at(
        &proj,
        "plugins/demo.occasions/occasions/game.yaml",
        "occasions:\n  talk: { select: first, target: { prefix: npc, entity: person }, description: The player talks to someone }\n",
    );
    write_at(
        &proj,
        "world.schema.yaml",
        "state:\n  run.day: { type: number, default: 1, owner: engine }\n  user.bond: { type: number, default: 0 }\n\
         entities:\n  person: { members: [mara, tomas] }\n\
         relations:\n  met: { args: [person], tier: user }\n  slew: { args: [person], tier: run, reserved: true }\n\
         defs:\n  trusted: \"user.bond >= 2\"\n  atLeast: { type: bool, params: { n: number }, cel: \"user.bond >= 1\" }\n",
    );
    write_at(
        &proj,
        "components/nod.component.lute",
        "---\ncomponent: nod\nparams:\n  who: string\n  mood: { enum: [warm, cold] }\n---\n\n## Nod\n\n@narrator: A nod.\n",
    );
    write_at(
        &proj,
        "scenes/mara.lute",
        "---\nkind: scene\nid: mara.first\non: talk\ntarget: npc.mara\ncomponents: [../components/nod.component.lute]\n---\n\n## Mara\n\n@mara: Hello.\n",
    );
    write_at(
        &proj,
        "quests/help.lute",
        "---\nkind: quest\nid: quest.help\n---\n\n<quest id=\"helpMara\" title=\"Help\">\n  <objective id=\"talk\" title=\"Talk\" done=\"user.bond >= 1\"/>\n</quest>\n",
    );
    write_at(
        &proj,
        "lore/notes.lute",
        "---\nkind: lore\nid: lore.notes\n---\n\n<entry id=\"lampNote\" target=\"item.lamp\" category=\"note\">\n  @narrator: A note.\n</entry>\n",
    );
    proj
}

fn context(proj: &Path, json: bool) -> String {
    let mut cmd = Command::new(BIN);
    cmd.arg("context").arg(proj.join("scenes/mara.lute"));
    if json {
        cmd.arg("--json");
    }
    let out = cmd.arg("--project").arg(proj).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    String::from_utf8_lossy(&out.stdout).to_string()
}

#[test]
fn context_json_lists_defs_ownership_tiers_builtins_and_ids() {
    let proj = project();
    let v: serde_json::Value = serde_json::from_str(&context(&proj, true)).unwrap();

    let def = |name: &str| {
        v["defs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["name"] == name)
            .unwrap_or_else(|| panic!("no def {name}: {}", v["defs"]))
            .clone()
    };
    assert_eq!(
        def("trusted"),
        serde_json::json!({ "name": "trusted", "type": "bool", "params": [], "body": "user.bond >= 2" })
    );
    assert_eq!(
        def("atLeast")["params"],
        serde_json::json!([{ "name": "n", "type": "number" }])
    );

    let state = |path: &str| {
        v["stateSchema"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["path"] == path)
            .unwrap()
            .clone()
    };
    assert_eq!(state("run.day")["owner"], "engine");
    assert!(state("user.bond").get("owner").is_none());

    let rel = |name: &str| {
        v["relations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["name"] == name)
            .unwrap()
            .clone()
    };
    assert_eq!(
        (rel("met")["tier"].clone(), rel("met")["reserved"].clone()),
        ("user".into(), false.into())
    );
    assert_eq!(
        (rel("slew")["tier"].clone(), rel("slew")["reserved"].clone()),
        ("run".into(), true.into())
    );

    let builtins: Vec<&str> = v["builtinDirectives"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|b| b["name"].as_str())
        .collect();
    assert_eq!(builtins, ["set", "assert", "retract", "accept", "use"]);

    assert_eq!(
        v["ids"],
        serde_json::json!({ "scenes": ["mara.first"], "quests": ["helpMara"], "entries": ["lampNote"] })
    );
}

#[test]
fn context_outline_shows_the_new_sections() {
    let proj = project();
    let text = context(&proj, false);
    for expected in [
        "  talk (select: first, target: npc.<person>) — The player talks to someone",
        "  run.day: number (owner: engine)",
        "  met/1(person) [user]",
        "  slew/1(person) [run, reserved]",
        "  nod(who: string, mood: enum[warm, cold])",
        "  @atLeast(n: number): bool = user.bond >= 1",
        "  @trusted: bool = user.bond >= 2",
        "  ::accept{quest=\"<questId>\" [when=\"<condition>\"]} — accept a quest that has no `start` condition",
        "scenes (1; read as visited(\"<id>\")):",
        "  mara.first",
        "  helpMara",
        "  lampNote",
    ] {
        assert!(
            text.lines().any(|l| l == expected),
            "missing line `{expected}`:\n{text}"
        );
    }
}

/// Round-5 UP lighthouse-1: a component param's declared `default:` (dsl
/// 0.26.0 §3) is part of its signature — a model writing `::use` against it
/// must know which params it may omit, and a `@def` default by name so it is
/// not overridden by a guessed literal. Every built-in directive shows its
/// optional `when=` guard (dsl 0.26.0 §4).
#[test]
fn context_shows_component_param_defaults_and_directive_guards() {
    let proj = project();
    write_at(
        &proj,
        "components/nod.component.lute",
        "---\ncomponent: nod\nparams:\n  who: { type: string, default: \"The inspector\" }\n  \
         mood: { type: { enum: [warm, cold] }, default: warm }\n  trust: { type: bool, default: \"@trusted\" }\n  \
         depth: number\n---\n\n## Nod\n\n@narrator: A nod.\n",
    );
    let v: serde_json::Value = serde_json::from_str(&context(&proj, true)).unwrap();
    let nod = v["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "nod")
        .unwrap_or_else(|| panic!("no nod: {}", v["components"]));
    let defaults: Vec<(&str, Option<&str>)> = nod["params"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p["name"].as_str().unwrap(), p["default"].as_str()))
        .collect();
    assert_eq!(
        defaults,
        [
            ("who", Some("The inspector")),
            ("mood", Some("warm")),
            ("trust", Some("@trusted")),
            ("depth", None),
        ],
        "{nod}"
    );

    let text = context(&proj, false);
    for expected in [
        "  nod(who: string = \"The inspector\", mood: enum[warm, cold] = warm, trust: bool = @trusted, depth: number)",
        "  ::set{ <path> = <expr> [when=\"<condition>\"] }  (also += / -=) — write a declared state path; `owner: engine` paths are the engine's (E-ENGINE-OWNED-WRITE)",
        "  ::assert{ <relation>(<arg>, …) [when=\"<condition>\"] } — assert a ground fact of a declared, non-derived, non-reserved relation",
        "  ::retract{ <relation>(<arg | _>, …) [when=\"<condition>\"] } — retract the matching facts of a declared, non-derived, non-reserved relation",
        "  ::use{component=\"<name>\" <param>=<value> … [when=\"<condition>\"]} — expand an imported component with named arguments; a param with a default may be omitted",
    ] {
        assert!(
            text.lines().any(|l| l == expected),
            "missing line `{expected}`:\n{text}"
        );
    }
}

/// dsl 0.23.0 §4: a lore document's `<beat>` bundles are listed by canonical
/// id (`<document id>.<beat id>`) — the key `visited()` reads — in both the
/// JSON surface and the outline.
#[test]
fn context_lists_bundle_beat_canonical_ids() {
    let proj = project();
    write_at(
        &proj,
        "lore/barks.lute",
        "---\nkind: lore\nid: lore.barks\n---\n\n<beat id=\"greet\" on=\"talk\" target=\"npc.mara\">\n  @mara: Hi.\n</beat>\n",
    );
    let v: serde_json::Value = serde_json::from_str(&context(&proj, true)).unwrap();
    assert_eq!(v["ids"]["beats"], serde_json::json!(["lore.barks.greet"]));
    assert_eq!(v["ids"]["entries"], serde_json::json!(["lampNote"]));
    let text = context(&proj, false);
    for expected in [
        "beats (1; bundle beats; read as visited(\"<id>\")):",
        "  lore.barks.greet",
    ] {
        assert!(
            text.lines().any(|l| l == expected),
            "missing line `{expected}`:\n{text}"
        );
    }
}

/// dsl 0.27.0: every new key shows where its siblings do — an occasion's
/// `raisedWhen` and `payload` beside `select` / `target`, a kind's `labels`
/// beside its members, a component's `beat:` header beside its params, and
/// the project's clock (`days` / `last`), `terminal`, `seasons` and the
/// manifest's `sequence`.
#[test]
fn context_shows_the_0_27_project_keys() {
    let proj = project();
    write_at(
        &proj,
        "lute.project.yaml",
        "pluginsDir: plugins/\ndefaultProfile: game\nprofiles:\n  game:\n    plugins: { demo.occasions: true }\n\
         defaults:\n  uses: [world.schema.yaml]\nsequence: { occasion: talk, scenes: [mara.first] }\n",
    );
    write_at(
        &proj,
        "plugins/demo.occasions/occasions/game.yaml",
        "occasions:\n  talk: { select: first, target: { prefix: npc, entity: person }, raisedWhen: \"run.day <= 3\" }\n  \
         summon: { select: sequence, payload: { copies: number } }\n",
    );
    write_at(
        &proj,
        "world.schema.yaml",
        "state:\n  run.day: { type: number, default: 1, owner: engine }\n  run.fate: { type: { enum: [alive, dead] }, default: alive }\n  \
         season.harvest.tokens: { type: number, default: 0 }\n\
         clock:\n  day: run.day\n  last: { day: 5 }\n\
         terminal: \"run.fate == 'dead'\"\n\
         seasons:\n  harvest: { live: \"run.day >= 2\" }\n\
         entities:\n  person: { members: [mara, tomas], labels: { mara: \"Mara Voss\" } }\n",
    );
    write_at(
        &proj,
        "components/nod.component.lute",
        "---\ncomponent: nod\nparams:\n  who: { entity: person }\nbeat:\n  on: talk\n  target: \"npc.@who\"\n  once: user\n---\n\n\
         ## Nod\n\n@narrator: A nod.\n",
    );
    let v: serde_json::Value = serde_json::from_str(&context(&proj, true)).unwrap();
    assert_eq!(v["occasions"]["talk"]["raisedWhen"], "run.day <= 3");
    assert_eq!(
        v["occasions"]["summon"]["payload"],
        serde_json::json!({ "copies": "number" })
    );
    let person = v["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == "person")
        .unwrap();
    assert_eq!(person["labels"], serde_json::json!({ "mara": "Mara Voss" }));
    let nod = v["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "nod")
        .unwrap();
    assert_eq!(
        nod["beat"],
        serde_json::json!({ "on": "talk", "target": "npc.@who", "once": "user" })
    );
    assert_eq!(
        v["clock"],
        serde_json::json!({ "day": "run.day", "last": { "day": 5 } })
    );
    assert_eq!(v["terminal"], "run.fate == 'dead'");
    assert_eq!(
        v["seasons"],
        serde_json::json!([{ "name": "harvest", "live": "run.day >= 2" }])
    );
    assert_eq!(
        v["sequence"],
        serde_json::json!({ "occasion": "talk", "scenes": ["mara.first"] })
    );

    let text = context(&proj, false);
    for expected in [
        "  talk (select: first, target: npc.<person>, raisedWhen: run.day <= 3)",
        "  summon (select: sequence, payload: { copies: number })",
        "  person: mara (\"Mara Voss\"), tomas",
        "  nod(who: entity:person)",
        "clock: day run.day, last day 5",
        "terminal: run.fate == 'dead' (no occasion is raised once it holds)",
        "  harvest — live: run.day >= 2",
        "sequence (occasion: talk): mara.first",
        "  prev.season.harvest.tokens: number (owner: engine)",
    ] {
        assert!(
            text.lines().any(|l| l == expected),
            "missing line `{expected}`:\n{text}"
        );
    }
    assert!(
        text.lines()
            .any(|l| l.starts_with("    beat: ") && l.contains("target: npc.@who")),
        "{text}"
    );
}

/// Every beat key and quest attribute the checker accepts is listed in
/// `beatKeys` / `questKeys` — a key added to the language shows in the
/// authoring surface (dsl 0.27.0: `spentBy`, `once: week | season:<name>`,
/// `for`, `use`, `rearm`).
#[test]
fn context_lists_every_beat_key_and_quest_attribute() {
    let proj = project();
    let v: serde_json::Value = serde_json::from_str(&context(&proj, true)).unwrap();
    let listed = |key: &str| -> Vec<(String, String)> {
        v[key]
            .as_array()
            .unwrap_or_else(|| panic!("no {key}: {v}"))
            .iter()
            .map(|k| {
                (
                    k["key"].as_str().unwrap().to_string(),
                    k["syntax"].as_str().unwrap().to_string(),
                )
            })
            .collect()
    };
    let beat_keys = listed("beatKeys");
    // Identity and catalogue attributes are not beat keys.
    let not_beat = ["id", "title", "category", "series", "order"];
    for key in lute_check::beats::BEAT_KEYS
        .iter()
        .chain(lute_check::BUNDLE_BEAT_ATTRS)
        .chain(lute_check::logic_attrs::ENTRY_ATTRS)
        .filter(|k| !not_beat.contains(k))
    {
        assert!(
            beat_keys.iter().any(|(k, _)| k == key),
            "beatKeys lacks `{key}`: {beat_keys:?}"
        );
    }
    let once = &beat_keys.iter().find(|(k, _)| k == "once").unwrap().1;
    assert!(
        once.contains("week") && once.contains("season:<name>"),
        "{once}"
    );
    let quest_keys = listed("questKeys");
    for key in lute_check::logic_attrs::QUEST_ATTRS
        .iter()
        .filter(|k| !["id", "title"].contains(k))
    {
        assert!(
            quest_keys.iter().any(|(k, _)| k == key),
            "questKeys lacks `{key}`: {quest_keys:?}"
        );
    }
}
