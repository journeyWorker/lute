//! One identifier rule: every name an author writes is a letter, then
//! letters, digits or `_`. A `-` is an error at the name, under that slot's
//! own code, naming the camelCase spelling. An id the engine owns keeps the
//! engine's spelling.

use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn project(files: &[(&str, &str)]) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-ident-{}-{n}", std::process::id()));
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

const MANIFEST: &str = "pluginsDir: plugins/\ndefaultProfile: game\nprofiles:\n  game:\n    \
    plugins: { game.occasions: true }\ndefaults:\n  uses: [world.schema.yaml]\n";
const PLUGIN: &str = "id: game.occasions\nversion: 0.1.0\nkind: capability\n\
    depends: [ { id: lute.core, range: \"^0.0.1\" } ]\nexports:\n  occasions: occasions/\n";

/// (slot, code, the bad name, its camelCase spelling), all in one project.
const SLOTS: &[(&str, &str, &str, &str)] = &[
    (
        "scene `id:`",
        "E-META-ID",
        "door-notes.intro",
        "doorNotes.intro",
    ),
    (
        "document `id:`",
        "E-META-ID",
        "lore.tomas-doc",
        "lore.tomasDoc",
    ),
    (
        "scene `share` key",
        "E-BEAT-ATTR",
        "nana-report",
        "nanaReport",
    ),
    (
        "`<entry>` `share` key",
        "E-BEAT-ATTR",
        "oil-news",
        "oilNews",
    ),
    ("`<beat>` id", "E-BEAT-ATTR", "tomas-beat", "tomasBeat"),
    ("entry id", "E-PATH-IDENT", "tomas-oil", "tomasOil"),
    ("quest id", "E-PATH-IDENT", "lamp-out", "lampOut"),
    ("objective id", "E-PATH-IDENT", "ask-tomas", "askTomas"),
    ("branch id", "E-PATH-IDENT", "mara-ask", "maraAsk"),
    ("choice id", "E-PATH-IDENT", "lamp-offer", "lampOffer"),
    ("hub id", "E-PATH-IDENT", "chat-hub", "chatHub"),
    ("mark id", "E-PATH-IDENT", "the-mark", "theMark"),
    ("enum", "E-PATH-IDENT", "bad-enum", "badEnum"),
    ("enum member", "E-PATH-IDENT", "half-loud", "halfLoud"),
    ("entity kind", "E-PATH-IDENT", "bad-kind", "badKind"),
    ("entity member", "E-PATH-IDENT", "old-man", "oldMan"),
    ("relation", "E-PATH-IDENT", "is-near", "isNear"),
    ("def", "E-PATH-IDENT", "lamp-lit", "lampLit"),
    (
        "season name",
        "E-SEASON-DECL",
        "harvest-time",
        "harvestTime",
    ),
    ("component param", "E-COMPONENT-PARSE", "the-who", "theWho"),
];

#[test]
fn every_name_slot_refuses_a_hyphen_under_its_own_code() {
    let dir = project(&[
        ("lute.project.yaml", MANIFEST),
        ("plugins/game.occasions/plugin.yaml", PLUGIN),
        (
            "plugins/game.occasions/occasions/game.yaml",
            "occasions:\n  talk: { select: first, target: { prefix: npc, entity: npc } }\n  \
             visit: { select: first }\n",
        ),
        (
            "world.schema.yaml",
            "state:\n  run.day: { type: number, default: 1 }\nenums:\n  tone: [soft, half-loud]\n  \
             bad-enum: [a, b]\nentities:\n  npc: { members: [mara, tomas, old-man] }\n  \
             bad-kind: { members: [x] }\nrelations:\n  is-near: { args: [npc], tier: run }\n\
             seasons:\n  harvest-time: { live: \"run.day > 1\" }\ndefs:\n  lamp-lit: \"run.day == 2\"\n",
        ),
        (
            "scenes/door.lute",
            "---\nkind: scene\nid: door-notes.intro\non: visit\nonce: user\nshare: nana-report\n---\n\n\
             ## Door\n\n@mara: Hi.\n::mark{id=\"the-mark\"}\n\n\
             <branch id=\"mara-ask\" prompt=\"?\">\n  <choice id=\"lamp-offer\" label=\"Offer\">\n    \
             @mara: Yes.\n  </choice>\n</branch>\n\
             <hub id=\"chat-hub\" prompt=\"More?\">\n  <choice id=\"bye\" label=\"Bye\" exit>\n    \
             @mara: Bye.\n  </choice>\n</hub>\n",
        ),
        (
            "quests/lamp.lute",
            "---\nkind: quest\nid: lamp\n---\n<quest id=\"lamp-out\" title=\"Lamp\" tier=\"run\" start=\"true\">\n  \
             <objective id=\"ask-tomas\" title=\"Ask\" done=\"run.day > 1\"/>\n</quest>\n",
        ),
        (
            "lore/tomas.lute",
            "---\nkind: lore\nid: lore.tomas-doc\n---\n\
             <entry id=\"tomas-oil\" on=\"talk\" target=\"npc.tomas\" once=\"user\" share=\"oil-news\">\n  \
             @tomas: Oil.\n</entry>\n\
             <beat id=\"tomas-beat\" on=\"talk\" target=\"npc.tomas\" once=\"user\" share=\"oil-news\">\n  \
             @tomas: Hm.\n</beat>\n",
        ),
        (
            "components/greet.component.lute",
            "---\ncomponent: greet\nparams:\n  the-who: string\n---\n@narrator: Hello.\n",
        ),
    ]);
    let out = check_project(&dir);
    for (what, code, bad, good) in SLOTS {
        let row = format!(
            "error [{code}] {what} `{bad}` is not {}",
            if bad.contains('.') {
                "a dotted id"
            } else {
                "an identifier"
            }
        );
        let line = out
            .lines()
            .find(|l| l.contains(&row))
            .unwrap_or_else(|| panic!("{what}: no `{row}` in:\n{out}"));
        assert!(line.contains(&format!("write `{good}`")), "{what}: {line}");
    }
}

/// A plugin's occasion, event and enum member follow the same rule, at the
/// plugin file (`E-PLUGIN-PARSE`).
#[test]
fn a_plugin_declared_name_refuses_a_hyphen() {
    for (file, text, what, bad, good) in [
        (
            "occasions/game.yaml",
            "occasions:\n  lamp-duty: { select: first }\n",
            "occasion",
            "lamp-duty",
            "lampDuty",
        ),
        (
            "events/game.yaml",
            "events:\n  - name: door-open\n",
            "event",
            "door-open",
            "doorOpen",
        ),
        (
            "enums/game.yaml",
            "enums:\n  tone: [soft, half-loud]\n",
            "enum member",
            "half-loud",
            "halfLoud",
        ),
    ] {
        let export = file.split('/').next().unwrap();
        let plugin = PLUGIN.replace("occasions: occasions/", &format!("{export}: {export}/"));
        let dir = project(&[
            ("lute.project.yaml", MANIFEST),
            ("plugins/game.occasions/plugin.yaml", &plugin),
            (&format!("plugins/game.occasions/{file}"), text),
            (
                "world.schema.yaml",
                "state:\n  run.day: { type: number, default: 1 }\n",
            ),
            (
                "scenes/a.lute",
                "---\nkind: scene\nid: a\n---\n## A\n@n: hi\n",
            ),
        ]);
        let out = check_project(&dir);
        let row = format!("[E-PLUGIN-PARSE] {what} `{bad}` is not an identifier");
        assert!(
            out.lines()
                .any(|l| l.contains(&row) && l.contains(&format!("write `{good}`"))),
            "{what}:\n{out}"
        );
    }
}

/// A name Lute declares is an identifier; an id the engine owns is written as
/// the engine spells it. A `target` no entity kind lists — no occasion, an
/// untyped `target: true` occasion, an `open:` kind — keeps its `-`, on an
/// entry, a bundle beat and a scene; so does an entry's `category` (engine
/// vocabulary). A kind that lists its members makes the member a declared
/// name: `npc.old-man` is outside the domain.
#[test]
fn an_engine_owned_target_keeps_its_hyphen() {
    let dir = project(&[
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
            "state:\n  run.day: { type: number, default: 1 }\nentities:\n  \
             npc: { members: [mara, oldMan] }\n  item: { open: engine }\n",
        ),
        (
            "lore/items.lute",
            "---\nkind: lore\nid: items\n---\n\
             <entry id=\"note\" target=\"item.torn-note\" category=\"key-item\">\n  \
             @narrator: A note.\n</entry>\n\
             <entry id=\"rustyKey\" on=\"pickup\" target=\"item.rusty-key\" once=\"run\">\n  \
             @narrator: A key.\n</entry>\n\
             <entry id=\"oldKey\" on=\"look\" target=\"item.old-key\" once=\"run\">\n  \
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
            "---\nkind: lore\nid: guard\n---\n\
             <entry id=\"guard\" on=\"talk\" target=\"npc.old-man\" once=\"run\">\n  \
             @narrator: Hm.\n</entry>\n",
        ),
    ]);
    let out = check_project(&dir);
    let faults: Vec<&str> = out
        .lines()
        .filter(|l| l.contains(" error [") || l.contains(" warning ["))
        .collect();
    assert_eq!(faults.len(), 1, "{out}");
    assert!(
        faults[0]
            .contains("[E-BEAT-ATTR] target `npc.old-man` is outside occasion `talk`'s domain"),
        "{out}"
    );
}
