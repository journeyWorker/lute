//! One name rule: every name an author writes is letters, digits, `_` or
//! `-`, not starting with `-`. A `-` or a leading digit is legal in every
//! slot; a `.`, whitespace or a quote is an error at the name, under that
//! slot's own code, listing the allowed characters. Reserved names stay
//! refused. A name read bare as `@name` (a def, a component param) is an
//! identifier.

use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn project(files: &[(&str, &str)]) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-names-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (rel, text) in files {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    dir
}

fn check_project(dir: &Path) -> String {
    let out = Command::new(BIN)
        .args(["check-project", dir.to_str().unwrap()])
        .output()
        .unwrap();
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

fn errors(out: &str) -> Vec<&str> {
    out.lines().filter(|l| l.contains(" error [")).collect()
}

const MANIFEST: &str = "pluginsDir: plugins/\ndefaultProfile: game\nprofiles:\n  game:\n    \
    plugins: { game.occasions: true }\ndefaults:\n  uses: [world.schema.yaml]\n";
const PLUGIN: &str = "id: game.occasions\nversion: 0.1.0\nkind: capability\n\
    depends: [ { id: lute.core, range: \"^0.0.1\" } ]\nexports:\n  occasions: occasions/\n";
const BARE_MANIFEST: &str = "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n\
    defaults:\n  uses: [world.schema.yaml]\n";
const RULE: &str = "letters, digits, `_` or `-`, not starting with `-`";

/// One project writing every slot's name as `{name}`-shaped text: the slot
/// table below substitutes a good or a bad spelling per slot.
fn slot_project(names: &[&str; 18]) -> PathBuf {
    let [scene, doc, share, entry_share, beat, entry, quest, objective, branch, choice, hub, mark, enum_name, member, kind, entity, relation, season] =
        names;
    project(&[
        ("lute.project.yaml", MANIFEST),
        ("plugins/game.occasions/plugin.yaml", PLUGIN),
        (
            "plugins/game.occasions/occasions/game.yaml",
            "occasions:\n  talk: { select: first, target: { prefix: npc, entity: npc } }\n  \
             visit: { select: first }\n",
        ),
        (
            "world.schema.yaml",
            &format!(
                "state:\n  run.day: {{ type: int, default: 1 }}\nenums:\n  tone: [soft, \"{member}\"]\n  \
                 \"{enum_name}\": [a, b]\nentities:\n  npc: {{ members: [mara, tomas, \"{entity}\"] }}\n  \
                 \"{kind}\": {{ members: [x] }}\nrelations:\n  \"{relation}\": {{ args: [npc], tier: run }}\n\
                 seasons:\n  \"{season}\": {{ live: \"run.day > 1\" }}\n"
            ),
        ),
        (
            "scenes/door.lute",
            &format!(
                "---\nkind: scene\nid: \"{scene}\"\non: visit\nonce: user\nshare: \"{share}\"\n---\n\n\
                 ## Door\n\n@mara: Hi.\n::mark{{id=\"{mark}\"}}\n\n\
                 <branch id=\"{branch}\" prompt=\"?\">\n  <choice id=\"{choice}\" label=\"Offer\">\n    \
                 @mara: Yes.\n  </choice>\n</branch>\n\
                 <hub id=\"{hub}\" prompt=\"More?\">\n  <choice id=\"bye\" label=\"Bye\" exit>\n    \
                 @mara: Bye.\n  </choice>\n</hub>\n"
            ),
        ),
        (
            "quests/lamp.lute",
            &format!(
                "---\nkind: quest\nid: lamp\n---\n<quest id=\"{quest}\" title=\"Lamp\" tier=\"run\" \
                 start=\"true\">\n  <objective id=\"{objective}\" title=\"Ask\" done=\"run.day > 1\"/>\n\
                 </quest>\n"
            ),
        ),
        (
            "lore/tomas.lute",
            &format!(
                "---\nkind: lore\nid: \"{doc}\"\n---\n\
                 <entry id=\"{entry}\" on=\"talk\" target=\"npc.tomas\" once=\"user\" \
                 share=\"{entry_share}\">\n  @tomas: Oil.\n</entry>\n\
                 <beat id=\"{beat}\" on=\"talk\" target=\"npc.tomas\" once=\"user\" \
                 share=\"{entry_share}\">\n  @tomas: Hm.\n</beat>\n"
            ),
        ),
    ])
}

/// (slot, code, a legal name with `-` or a leading digit, a refused name).
const SLOTS: [(&str, &str, &str, &str); 18] = [
    (
        "scene `id:`",
        "E-META-ID",
        "door-notes.001-intro",
        "door notes.intro",
    ),
    (
        "document `id:`",
        "E-META-ID",
        "lore.tomas-doc",
        "lore.tomas doc",
    ),
    (
        "scene `share` key",
        "E-BEAT-ATTR",
        "nana-report",
        "nana.report",
    ),
    (
        "`<entry>` `share` key",
        "E-BEAT-ATTR",
        "oil-news",
        "oil news",
    ),
    ("`<beat>` id", "E-BEAT-ATTR", "tomas-beat", "tomas beat"),
    ("entry id", "E-ENTRY-ATTR", "tomas-oil", "tomas.oil"),
    ("quest id", "E-PATH-IDENT", "lamp-out", "-lamp"),
    ("objective id", "E-PATH-IDENT", "1st-ask", "ask tomas"),
    ("branch id", "E-PATH-IDENT", "mara-ask", "mara ask"),
    ("choice id", "E-PATH-IDENT", "lamp-offer", "lamp'offer"),
    ("hub id", "E-PATH-IDENT", "chat-hub", "chat hub"),
    ("mark id", "E-PATH-IDENT", "the-mark", "the mark"),
    ("enum", "E-PATH-IDENT", "tone-2", "tone 2"),
    ("enum member", "E-PATH-IDENT", "half-loud", "half loud"),
    ("entity kind", "E-PATH-IDENT", "street-cat", "street cat"),
    ("entity member", "E-PATH-IDENT", "7th-son", "old man"),
    ("relation", "E-PATH-IDENT", "is-near", "is near"),
    (
        "season name",
        "E-SEASON-DECL",
        "harvest-time",
        "harvest time",
    ),
];

#[test]
fn every_name_slot_accepts_a_hyphen_and_a_leading_digit() {
    let good = SLOTS.map(|(_, _, good, _)| good);
    let out = check_project(&slot_project(&good));
    assert_eq!(errors(&out), Vec::<&str>::new(), "{out}");
}

#[test]
fn every_name_slot_refuses_other_characters_under_its_own_code() {
    let bad = SLOTS.map(|(_, _, _, bad)| bad);
    let out = check_project(&slot_project(&bad));
    for (what, code, _, bad) in SLOTS {
        let row = if what.ends_with("`id:`") {
            format!(
                "error [{code}] {what} `{bad}` is not a dotted id: names joined by `.`, each {RULE}"
            )
        } else {
            format!("error [{code}] {what} `{bad}` is not a name: {RULE}")
        };
        assert!(
            out.lines().any(|l| l.contains(&row)),
            "{what}: no `{row}` in:\n{out}"
        );
    }
}

/// A def and a component param are read bare as `@name`, like a JavaScript
/// variable, so each is an identifier: `@lampLit` works, `lamp-lit` is
/// refused naming the identifier spelling.
#[test]
fn a_def_and_a_component_param_are_identifiers() {
    let run = |param: &str, def: &str| {
        check_project(&project(&[
            ("lute.project.yaml", BARE_MANIFEST),
            (
                "world.schema.yaml",
                &format!("state:\n  run.day: {{ type: int, default: 1 }}\ndefs:\n  \"{def}\": \"run.day == 2\"\n"),
            ),
            (
                "components/greet.component.lute",
                &format!("---\ncomponent: greet\nparams:\n  \"{param}\": string\n---\n## Greet\n@narrator: Hello.\n"),
            ),
            (
                "scenes/a.lute",
                &format!(
                    "---\nkind: scene\nid: a\n---\n## A\n@narrator: hi\n\
                     ::set{{run.day = 3 when=\"@{def}\"}}\n"
                ),
            ),
        ]))
    };
    let out = run("theWho", "lampLit");
    assert_eq!(errors(&out), Vec::<&str>::new(), "{out}");
    let out = run("the-who", "lamp-lit");
    for row in [
        "[E-COMPONENT-PARSE] component param `the-who` is not an identifier: it is read bare as \
         `@the-who` — a letter or `_`, then letters, digits or `_`; write `theWho`",
        "[E-PATH-IDENT] def `lamp-lit` is not an identifier: it is read bare as `@lamp-lit` — a \
         letter or `_`, then letters, digits or `_`; write `lampLit`",
    ] {
        assert!(
            out.lines().any(|l| l.contains(row)),
            "no `{row}` in:\n{out}"
        );
    }
}

/// A plugin's occasion, event and enum member follow the same rule, at the
/// plugin file (`E-PLUGIN-PARSE`).
#[test]
fn a_plugin_declared_name_follows_the_rule() {
    for (file, text, what, name) in [
        (
            "occasions/game.yaml",
            "occasions:\n  \"{}\": { select: first }\n",
            "occasion",
            "lamp-duty",
        ),
        (
            "events/game.yaml",
            "events:\n  - name: \"{}\"\n",
            "event",
            "door-open",
        ),
        (
            "enums/game.yaml",
            "enums:\n  tone: [soft, \"{}\"]\n",
            "enum member",
            "2nd-loud",
        ),
    ] {
        let export = file.split('/').next().unwrap();
        let plugin = PLUGIN.replace("occasions: occasions/", &format!("{export}: {export}/"));
        let run = |name: &str| {
            check_project(&project(&[
                ("lute.project.yaml", MANIFEST),
                ("plugins/game.occasions/plugin.yaml", &plugin),
                (
                    &format!("plugins/game.occasions/{file}"),
                    &text.replace("{}", name),
                ),
                (
                    "world.schema.yaml",
                    "state:\n  run.day: { type: int, default: 1 }\n",
                ),
                (
                    "scenes/a.lute",
                    "---\nkind: scene\nid: a\n---\n## A\n@n: hi\n",
                ),
            ]))
        };
        let out = run(name);
        assert!(!out.contains("E-PLUGIN-PARSE"), "{what} `{name}`:\n{out}");
        let bad = name.replace('-', " ");
        let out = run(&bad);
        let row = format!("[E-PLUGIN-PARSE] {what} `{bad}` is not a name: {RULE}");
        assert!(out.lines().any(|l| l.contains(&row)), "{what}:\n{out}");
    }
}

/// A target and a category are names like any other: `-` on an open kind, a
/// listed member (`npc.old-man`), an untyped occasion, a scene target and a
/// category all check clean; a space is refused at the target.
#[test]
fn targets_and_categories_take_any_name() {
    let run = |guard: &str| {
        check_project(&project(&[
            ("lute.project.yaml", MANIFEST),
            ("plugins/game.occasions/plugin.yaml", PLUGIN),
            (
                "plugins/game.occasions/occasions/game.yaml",
                "occasions:\n  pickup: { select: first, target: true }\n  \
                 look: { select: first, target: { prefix: item, entity: item } }\n  \
                 talk: { select: first, target: { prefix: npc, entity: npc } }\n",
            ),
            (
                "world.schema.yaml",
                "state:\n  run.day: { type: int, default: 1 }\nentities:\n  \
                 npc: { members: [mara, old-man] }\n  item: { open: engine }\n",
            ),
            (
                "lore/items.lute",
                "---\nkind: lore\nid: items\n---\n\
                 <entry id=\"note\" target=\"item.torn-note\" category=\"key-item\">\n  \
                 @narrator: A note.\n</entry>\n\
                 <entry id=\"rusty-key\" on=\"pickup\" target=\"item.rusty-key\" once=\"run\">\n  \
                 @narrator: A key.\n</entry>\n\
                 <entry id=\"old-key\" on=\"look\" target=\"item.001-key\" once=\"run\">\n  \
                 @narrator: Old.\n</entry>\n\
                 <beat id=\"lamp\" on=\"pickup\" target=\"item.brass-lamp\" once=\"run\">\n  \
                 @narrator: A lamp.\n</beat>\n",
            ),
            (
                "scenes/crate.lute",
                "---\nkind: scene\nid: crate\non: pickup\ntarget: item.wooden-crate\n---\n\
                 ## Crate\n\n@narrator: A crate.\n",
            ),
            (
                "lore/guard.lute",
                &format!(
                    "---\nkind: lore\nid: guard\n---\n\
                     <entry id=\"guard\" on=\"talk\" target=\"{guard}\" once=\"run\">\n  \
                     @narrator: Hm.\n</entry>\n"
                ),
            ),
        ]))
    };
    let out = run("npc.old-man");
    assert_eq!(errors(&out), Vec::<&str>::new(), "{out}");
    let out = run("npc.old man");
    let faults = errors(&out);
    assert_eq!(faults.len(), 1, "{out}");
    assert!(
        faults[0].contains("[E-ENTRY-ATTR] `<entry>` `target=\"npc.old man\"` must be a dotted id"),
        "{out}"
    );
}

/// The reserved-name table is unchanged: a name that follows the rule but is
/// one the language keeps for itself is still refused.
#[test]
fn a_reserved_name_is_still_refused() {
    let out = check_project(&project(&[
        ("lute.project.yaml", BARE_MANIFEST),
        (
            "world.schema.yaml",
            "state:\n  run.day: { type: int, default: 1 }\nenums:\n  tone: [soft, unset]\n\
             defs:\n  run: \"run.day == 2\"\n",
        ),
        (
            "scenes/a.lute",
            "---\nkind: scene\nid: a\n---\n## A\n@narrator: hi\n",
        ),
    ]));
    let reserved: Vec<&str> = out
        .lines()
        .filter(|l| l.contains("[E-RESERVED-NAME]"))
        .collect();
    assert_eq!(reserved.len(), 2, "{out}");
    assert!(!out.contains("is not a name"), "{out}");
}
