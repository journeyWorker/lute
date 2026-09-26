//! dsl 0.26.0 docs-pass tool fixes, through the built `lute` binary: the
//! T1-7 premise mocks the failure suggests are legal (`quests:` for an
//! `after:` quest, `entriesRead:` for a spent `once="user"` entry), an
//! imported rule's `E-RULE-AGGREGATE-CYCLE` is reported once at the schema,
//! `{{occasion.target}}` renders the cast name in `lute test` as in play, an
//! unspoken `W-DISPLAY-NAME-DUP` sits at the cast entry, and a plugin
//! directive attribute named `when` is refused at load.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-tp026b-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &Path, rel: &str, text: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

fn run(dir: &Path, args: &[&str]) -> (Option<i32>, String) {
    let o: Output = Command::new(BIN)
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    (
        o.status.code(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        ),
    )
}

const PROJECT: &str =
    "pluginsDir: plugins/\ndefaultProfile: g\nprofiles:\n  g:\n    plugins: { g.x: true }\n\
                       defaults:\n  uses: [world.schema.yaml]\n";

fn plugin(dir: &Path, directive: &str) {
    write(dir, "lute.project.yaml", PROJECT);
    write(
        dir,
        "plugins/g.x/plugin.yaml",
        "id: g.x\nversion: 0.1.0\nkind: capability\ndepends: [ { id: lute.core, range: \"^0.0.1\" } ]\n\
         exports:\n  occasions: occasions/\n  cast: cast/\n  directives: directives/\n",
    );
    write(
        dir,
        "plugins/g.x/occasions/o.yaml",
        "occasions:\n  arrive: {}\n  caught: { target: { prefix: mon, entity: species } }\n",
    );
    write(
        dir,
        "plugins/g.x/cast/c.yaml",
        "cast:\n  gus: { name: Hiker Gus }\n  r16Gus: { name: Hiker Gus }\n  ant: { name: Inchlet }\n",
    );
    write(
        dir,
        "plugins/g.x/directives/d.yaml",
        &format!("directives:\n  - name: ping\n    attrs:\n      - {{ name: {directive}, type: string }}\n"),
    );
    write(
        dir,
        "world.schema.yaml",
        "entities:\n  species: { members: [ant, bee] }\n",
    );
}

/// The T1-7 failure for `after: completed("q")` suggests `quests: { q:
/// complete }`; that mock is legal and makes the scene eligible.
#[test]
fn the_after_premise_quest_mock_is_accepted() {
    let dir = temp_dir("after");
    write(
        &dir,
        "wed.lute",
        "---\nkind: scene\nid: town.wed\non: arrive\nafter: completed(\"caseClosed\")\n---\n\n## Gate\n\n@guard: Wednesday.\n",
    );
    write(
        &dir,
        "a.test.yaml",
        "file: wed.lute\nexpect:\n  transcriptContains: [\"Wednesday.\"]\n",
    );
    let (code, t) = run(&dir, &["test", "."]);
    assert_eq!(code, Some(1), "{t}");
    assert!(t.contains("mock `quests: { caseClosed: complete }`"), "{t}");
    write(
        &dir,
        "a.test.yaml",
        "file: wed.lute\nquests: { caseClosed: complete }\nexpect:\n  transcriptContains: [\"Wednesday.\"]\n",
    );
    let (code, t) = run(&dir, &["test", "."]);
    assert_eq!(code, Some(0), "{t}");
}

/// A spent `once="user"` entry is mocked with `entriesRead: { user: [id] }`
/// (as a play save), and the failure names that mock.
#[test]
fn a_spent_once_user_entry_is_mocked_by_entries_read() {
    let dir = temp_dir("once-user");
    write(
        &dir,
        "gate.lute",
        "---\nkind: lore\nid: lore.gate\ntitle: Gate\n---\n\n\
         <entry id=\"gateNote\" title=\"Gate\" on=\"look\" once=\"user\">\n  @narrator: The gate is open.\n</entry>\n",
    );
    let test = |expect: &str| {
        write(
            &dir,
            "a.test.yaml",
            &format!("file: gate.lute\nentry: gateNote\nentriesRead: {{ user: [gateNote] }}\nexpect:\n  {expect}\n"),
        );
        run(&dir, &["test", "."])
    };
    let (code, t) = test("transcriptContains: [\"The gate is open.\"]");
    assert_eq!(code, Some(1), "{t}");
    assert!(
        t.contains("it is `once=\"user\"` and already spent — the mocked `entriesRead:`"),
        "{t}"
    );
    let (code, t) = test("eligible: { gateNote: false }");
    assert_eq!(code, Some(0), "{t}");
    // Unread, it presents.
    write(
        &dir,
        "a.test.yaml",
        "file: gate.lute\nentry: gateNote\nexpect:\n  transcriptContains: [\"The gate is open.\"]\n",
    );
    assert_eq!(run(&dir, &["test", "."]).0, Some(0));
}

/// An imported rule's aggregate cycle: once at the schema line, folded
/// across importers, and in a standalone `lute check <schema>.yaml`.
#[test]
fn an_imported_aggregate_cycle_is_reported_once_at_the_schema() {
    let dir = temp_dir("agg");
    write(
        &dir,
        "lute.project.yaml",
        "defaultProfile: g\nprofiles:\n  g:\n    plugins: {}\ndefaults:\n  uses: [world.schema.yaml]\n",
    );
    write(
        &dir,
        "world.schema.yaml",
        "entities:\n  door: { members: [earth] }\nrelations:\n  open: { args: [door], derive: true }\n  \
         knock: { args: [door], tier: run }\nrules:\n  - \"open(D) :- knock(D), count(open(_)) >= 1\"\n",
    );
    for s in ["a", "b", "c"] {
        write(
            &dir,
            &format!("scenes/{s}.lute"),
            &format!("---\nkind: scene\nid: {s}\n---\n\n## One\n\n@narrator: Hi.\n::assert{{knock(earth)}}\n"),
        );
    }
    let (code, t) = run(&dir, &["check-project", "."]);
    assert_eq!(code, Some(1), "{t}");
    let tops: Vec<&str> = t
        .lines()
        .filter(|l| !l.starts_with(' ') && l.contains("E-RULE-AGGREGATE-CYCLE"))
        .collect();
    assert_eq!(tops.len(), 1, "{t}");
    assert!(tops[0].contains("(+2 more callers)"), "{t}");
    assert!(t.contains("    world.schema.yaml:7:"), "{t}");
    let (code, t) = run(&dir, &["check", "world.schema.yaml"]);
    assert_eq!(code, Some(1), "{t}");
    assert!(
        t.contains("world.schema.yaml:7:6: error [E-RULE-AGGREGATE-CYCLE]"),
        "{t}"
    );
}

/// `lute test` renders `{{occasion.target}}` by the member's cast name, as
/// `lute play` does; an unspoken display-name clash sits at the cast entry.
#[test]
fn occasion_target_renders_the_cast_name_and_unspoken_dups_sit_at_the_cast() {
    let dir = temp_dir("target");
    plugin(&dir, "level");
    write(
        &dir,
        "lore/catch.lute",
        "---\nkind: lore\nid: dex\n---\n\n\
         <beat id=\"bug\" on=\"caught\" target=\"kind:species\" once=\"false\">\n  @narrator: A {{occasion.target}}.\n</beat>\n",
    );
    write(
        &dir,
        "t.test.yaml",
        "file: lore/catch.lute\nbeat: bug\nstate: { occasion.target: ant }\nexpect:\n  transcriptContains: [\"A Inchlet.\"]\n",
    );
    let (code, t) = run(&dir, &["test", "."]);
    assert_eq!(code, Some(0), "{t}");
    let (_, t) = run(&dir, &["check-project", "."]);
    let dup = t
        .lines()
        .find(|l| l.contains("W-DISPLAY-NAME-DUP"))
        .unwrap_or_else(|| panic!("{t}"));
    assert!(
        dup.starts_with("./plugins/g.x/cast/c.yaml:3:3:"),
        "anchored at the second entry: {t}"
    );
}

#[test]
fn a_plugin_directive_attribute_named_when_is_refused_at_load() {
    let dir = temp_dir("when-attr");
    plugin(&dir, "when");
    write(
        &dir,
        "scenes/s.lute",
        "---\nkind: scene\nid: s\n---\n\n## One\n\n@narrator: Hi.\n",
    );
    let (code, t) = run(&dir, &["check-project", "."]);
    assert_ne!(code, Some(0), "{t}");
    assert!(
        t.contains("E-PLUGIN-PARSE")
            && t.contains("declares an attribute `when`, which is reserved"),
        "{t}"
    );
}
