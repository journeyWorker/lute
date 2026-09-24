//! dsl 0.19.0 §8 CLI surfaces owned by the tooling slice: `lute new lore`
//! scaffolds a document `lute check` accepts, and `lute lore <dir>` reports
//! the world-narrative map (entries by target / series, and which facts lore
//! entries reveal versus scenes and quests) as text and as JSON.

use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-lore-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn run(args: &[&str]) -> std::process::Output {
    Command::new(BIN).args(args).output().unwrap()
}

fn stdout(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn new_lore_scaffolds_a_document_that_checks_clean() {
    let dir = temp_dir("new");
    let out = run(&["new", "lore", "ship-records", "--dir", dir.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let path = dir.join("lore/ship-records.lute");
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("kind: lore\nid: lore.shipRecords\n"), "{text}");
    assert!(text.contains("<entry id=\"shipRecords\""), "{text}");

    let check = run(&["check", path.to_str().unwrap(), "--json"]);
    let result: serde_json::Value = serde_json::from_slice(&check.stdout)
        .unwrap_or_else(|e| panic!("{e}: {}{}", stdout(&check), String::from_utf8_lossy(&check.stderr)));
    assert_eq!(check.status.code(), Some(0), "{result:#}");
    assert_eq!(
        result["diagnostics"].as_array().map(Vec::len),
        Some(0),
        "the scaffold must check with no diagnostics at all: {result:#}"
    );

    // Refuses to overwrite, like every other `lute new` kind.
    let again = run(&["new", "lore", "ship-records", "--dir", dir.to_str().unwrap()]);
    assert_eq!(again.status.code(), Some(2));
}

/// dsl 0.19.0 §2.1: `lute new quest` scaffolds a document id too, and the two
/// scaffolds for one name share a project without an id collision.
#[test]
fn new_quest_and_lore_scaffolds_check_clean_together() {
    let dir = temp_dir("new-quest");
    write(&dir, "lute.project.yaml", "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n");
    let d = dir.to_str().unwrap();
    assert_eq!(run(&["new", "quest", "ship-records", "--dir", d]).status.code(), Some(0));
    assert_eq!(run(&["new", "lore", "ship-records", "--dir", d]).status.code(), Some(0));
    let quest = std::fs::read_to_string(dir.join("quests/ship-records.lute")).unwrap();
    assert!(quest.contains("kind: quest\nid: quest.shipRecords\n"), "{quest}");

    let check = run(&["check-project", d, "--json"]);
    let result: serde_json::Value = serde_json::from_slice(&check.stdout)
        .unwrap_or_else(|e| panic!("{e}: {}{}", stdout(&check), String::from_utf8_lossy(&check.stderr)));
    assert_eq!(check.status.code(), Some(0), "{result:#}");
    let text = result.to_string();
    assert!(!text.contains("\"code\""), "no diagnostics at all: {result:#}");
}

/// dsl 0.19.0 §2.1 through `check-project`: a quest document id colliding
/// with a scene id is `E-CONN-EPISODE-ID-DUP` worded as a document id, and a
/// document-level `series:` colliding with another file's hand-numbered
/// position is `E-ENTRY-SERIES-ORDER`.
#[test]
fn check_project_reports_bundle_id_and_resolved_series_collisions() {
    let dir = temp_dir("bundle");
    write(&dir, "lute.project.yaml", "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n");
    write(&dir, "a-scene.lute", "---\nkind: scene\nid: haven.main\n---\n## One.\n@n: hi\n");
    write(
        &dir,
        "b-quest.lute",
        "---\nkind: quest\nid: haven.main\n---\n<quest id=\"q\" start=\"true\">\n</quest>\n",
    );
    write(
        &dir,
        "c-log.lute",
        "---\nkind: lore\nseries: log\n---\n<entry id=\"log1\">\n@n: a\n</entry>\n\
         <entry id=\"log2\">\n@n: b\n</entry>\n",
    );
    write(
        &dir,
        "d-extra.lute",
        "---\nkind: lore\n---\n<entry id=\"logX\" series=\"log\" order=\"2\">\n@n: c\n</entry>\n",
    );
    let out = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("{e}: {}{}", stdout(&out), String::from_utf8_lossy(&out.stderr)));
    let mut found: Vec<(String, String)> = Vec::new();
    fn walk(v: &serde_json::Value, found: &mut Vec<(String, String)>) {
        match v {
            serde_json::Value::Object(m) => {
                if let (Some(c), Some(msg)) = (m.get("code"), m.get("message")) {
                    found.push((
                        c.as_str().unwrap_or_default().to_string(),
                        msg.as_str().unwrap_or_default().to_string(),
                    ));
                }
                m.values().for_each(|x| walk(x, found));
            }
            serde_json::Value::Array(a) => a.iter().for_each(|x| walk(x, found)),
            _ => {}
        }
    }
    walk(&v, &mut found);
    let dup: Vec<_> = found.iter().filter(|(c, _)| c == "E-CONN-EPISODE-ID-DUP").collect();
    assert_eq!(dup.len(), 1, "{v:#}");
    assert!(dup[0].1.starts_with("duplicate document id `haven.main` across project files"), "{}", dup[0].1);
    let pos: Vec<_> = found.iter().filter(|(c, _)| c == "E-ENTRY-SERIES-ORDER").collect();
    assert_eq!(pos.len(), 1, "{v:#}");
    assert!(pos[0].1.contains("repeats position 2 of series `log`"), "{}", pos[0].1);
    assert_eq!(out.status.code(), Some(1), "{v:#}");
}

/// `lute lore` groups a document-level series by resolved position.
#[test]
fn lore_report_uses_resolved_series() {
    let dir = temp_dir("resolved");
    write(
        &dir,
        "log.lute",
        "---\nkind: lore\nseries: captainsLog\n---\n<entry id=\"day2\">\n@n: b\n</entry>\n\
         <entry id=\"day1\">\n@n: a\n</entry>\n",
    );
    let out = run(&["lore", dir.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(
        stdout(&out).contains("Series\n  captainsLog\n    1  day2  log.lute\n    2  day1  log.lute\n"),
        "{}",
        stdout(&out)
    );
}

const SCHEMA: &str = "\
entities:
  person: { members: [vesna, orin] }
  topic: { members: [project_lumen, reactor] }
relations:
  knows: { args: [person, topic], tier: run }
  met: { args: [person, person], tier: run }
";

const LORE: &str = "\
---
kind: lore
uses: ../world.schema.yaml
state:
  run.labBurned: { type: bool, default: false }
---

<entry id=\"log2\" target=\"item.torn_note_2\" category=\"note\" series=\"scientistLog\" order=\"2\">
  @scientist: Day four. Vesna asked about the reactor.
  ::assert{ knows(vesna, reactor) }
</entry>

<entry id=\"log1\" target=\"item.torn_note_1\" category=\"note\" series=\"scientistLog\" order=\"1\" title=\"Research log, day 3\">
  @scientist: Day three. Subject E does not respond to light.
  ::assert{ knows(vesna, project_lumen) }
</entry>

<entry id=\"rustyKey\" target=\"item.rusty_key\" category=\"item\">
  <match on=\"run.labBurned\">
    <when is=\"true\">
      @narrator: A scorched key.
    </when>
    <otherwise>
      @narrator: A rusty key, stamped \"Research wing B2\".
    </otherwise>
  </match>
</entry>

<entry id=\"bark\" category=\"bark\">
  @orin: Keep moving.
</entry>
";

const SCENE: &str = "\
---
kind: scene
character: vesna
season: 1
episode: 1
uses: ../world.schema.yaml
---

## Lab.

@vesna: So that is Project Lumen.
::assert{ knows(vesna, project_lumen) }
::assert{ met(vesna, orin) }
";

fn fixture() -> PathBuf {
    let dir = temp_dir("report");
    write(&dir, "lute.project.yaml", "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n");
    write(&dir, "world.schema.yaml", SCHEMA);
    write(&dir, "lore/ship.lute", LORE);
    write(&dir, "scenes/lab.lute", SCENE);
    dir
}

#[test]
fn lore_report_text_groups_entries_and_splits_fact_sources() {
    let dir = fixture();
    let out = run(&["lore", dir.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let expected = "\
Entries by target
  item.rusty_key
    rustyKey  [item]  lore/ship.lute
  item.torn_note_1
    log1  [note]  \"Research log, day 3\"  lore/ship.lute
  item.torn_note_2
    log2  [note]  lore/ship.lute
  (no target)
    bark  [bark]  lore/ship.lute

Series
  scientistLog
    1  log1  [note]  \"Research log, day 3\"  lore/ship.lute
    2  log2  [note]  lore/ship.lute

Facts by relation
  knows
    knows(vesna, project_lumen)  both
      entries: log1
      scenes/quests: scenes/lab.lute
    knows(vesna, reactor)  entries
      entries: log2
  met
    met(vesna, orin)  scenes
      scenes/quests: scenes/lab.lute
";
    assert_eq!(stdout(&out), expected);
}

#[test]
fn lore_report_json_carries_the_same_data() {
    let dir = fixture();
    let out = run(&["lore", dir.to_str().unwrap(), "--json"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let expected = serde_json::json!({
        "targets": [
            {"target": "item.rusty_key", "entries": [
                {"id": "rustyKey", "document": "lore/ship.lute", "target": "item.rusty_key", "category": "item"}
            ]},
            {"target": "item.torn_note_1", "entries": [
                {"id": "log1", "document": "lore/ship.lute", "target": "item.torn_note_1", "category": "note",
                 "title": "Research log, day 3", "series": "scientistLog", "order": 1}
            ]},
            {"target": "item.torn_note_2", "entries": [
                {"id": "log2", "document": "lore/ship.lute", "target": "item.torn_note_2", "category": "note",
                 "series": "scientistLog", "order": 2}
            ]},
            {"target": null, "entries": [
                {"id": "bark", "document": "lore/ship.lute", "category": "bark"}
            ]}
        ],
        "series": [
            {"series": "scientistLog", "entries": [
                {"id": "log1", "document": "lore/ship.lute", "target": "item.torn_note_1", "category": "note",
                 "title": "Research log, day 3", "series": "scientistLog", "order": 1},
                {"id": "log2", "document": "lore/ship.lute", "target": "item.torn_note_2", "category": "note",
                 "series": "scientistLog", "order": 2}
            ]}
        ],
        "relations": [
            {"relation": "knows", "facts": [
                {"fact": "knows(vesna, project_lumen)", "revealedBy": "both",
                 "entries": ["log1"], "documents": ["scenes/lab.lute"]},
                {"fact": "knows(vesna, reactor)", "revealedBy": "entries",
                 "entries": ["log2"], "documents": []}
            ]},
            {"relation": "met", "facts": [
                {"fact": "met(vesna, orin)", "revealedBy": "scenes",
                 "entries": [], "documents": ["scenes/lab.lute"]}
            ]}
        ]
    });
    assert_eq!(v, expected, "{v:#}");
}

/// dsl 0.23.0 §4: a lore `<beat>` bundle's `::assert` (even inside a branch
/// choice) reveals its fact like an entry's, attributed to the beat's
/// canonical `<document id>.<beat id>`.
#[test]
fn lore_report_attributes_bundle_beat_asserts_to_the_canonical_id() {
    let dir = fixture();
    write(
        &dir,
        "lore/dock.lute",
        "---\nkind: lore\nid: lore.dock\nuses: ../world.schema.yaml\n---\n\n\
         <beat id=\"talk\" on=\"talk\">\n  @orin: Keep moving.\n  <branch id=\"ask\">\n    \
         <choice id=\"why\" label=\"Why?\">\n      ::assert{ knows(vesna, reactor) }\n    </choice>\n  \
         </branch>\n</beat>\n",
    );
    let out = run(&["lore", dir.to_str().unwrap(), "--json"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let knows = v["relations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["relation"] == "knows")
        .unwrap();
    let reactor = knows["facts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["fact"] == "knows(vesna, reactor)")
        .unwrap();
    assert_eq!(
        reactor,
        &serde_json::json!({"fact": "knows(vesna, reactor)", "revealedBy": "entries",
            "entries": ["log2", "lore.dock.talk"], "documents": []})
    );
}

/// The report does not require a clean check: a lore document with a
/// checker error (`order` without `series`, a duplicate id) is still
/// reported. A missing directory is an I/O failure (exit 2).
#[test]
fn lore_report_reports_unchecked_documents_and_fails_on_io() {
    let dir = temp_dir("unchecked");
    write(
        &dir,
        "a.lute",
        "---\nkind: lore\n---\n<entry id=\"x\" order=\"3\">\n@n: a\n</entry>\n\
         <entry id=\"x\">\n@n: b\n</entry>\n",
    );
    let out = run(&["lore", dir.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let text = stdout(&out);
    assert!(text.contains("  (no target)\n    x  a.lute\n    x  a.lute\n"), "{text}");

    let missing = run(&["lore", dir.join("nope").to_str().unwrap()]);
    assert_eq!(missing.status.code(), Some(2), "{missing:?}");
}
