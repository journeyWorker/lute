//! 0.41.0 §3.5: `host: true` defs — a schema file's or a plugin's — are
//! exempt from `W-DEF-UNUSED`; a document's own frontmatter def refuses
//! `host:` with a hint.

use std::path::Path;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn write(dir: &Path, rel: &str, text: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

#[test]
fn host_defs_are_never_unused_and_frontmatter_refuses_host() {
    let dir = lute_scratch::ScratchDir::named("host-defs-041");
    write(
        &dir,
        "lute.project.yaml",
        "pluginsDir: plugins/\ndefaultProfile: core\nprofiles:\n  core:\n    plugins: { game.defs: true }\n\
         defaults:\n  luteVersion: \"0.47.0\"\n  uses: [world.schema.yaml]\n",
    );
    write(
        &dir,
        "plugins/game.defs/plugin.yaml",
        "id: game.defs\nversion: 0.1.0\nkind: capability\ndepends: [ { id: lute.core, range: \"^0.0.1\" } ]\nexports:\n  defs: defs/\n",
    );
    write(
        &dir,
        "plugins/game.defs/defs/defs.yaml",
        "defs:\n  - { name: pluginOpen, type: bool, cel: \"user.level > 1\", host: true }\n",
    );
    write(
        &dir,
        "world.schema.yaml",
        "state:\n  user.level: { type: int, default: 0 }\ndefs:\n  \
         marketOpen: { cel: \"user.level > 2\", host: true }\n  \
         unused: \"user.level > 3\"\n",
    );
    write(
        &dir,
        "a.lute",
        "---\nkind: scene\nid: a\n---\n## A\n@narrator: {{user.level}}\n",
    );
    let out = Command::new(BIN)
        .arg("check-project")
        .arg(&*dir)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("def `unused` is declared but no `@unused`"),
        "{text}"
    );
    assert!(!text.contains("def `marketOpen` is declared"), "{text}");
    assert!(!text.contains("def `pluginOpen` is declared"), "{text}");
    assert!(
        !text.contains("E-DEF-DECL") && !text.contains("E-PLUGIN"),
        "{text}"
    );

    write(
        &dir,
        "b.lute",
        "---\nkind: scene\nid: b\ndefs:\n  local: { cel: \"user.level > 1\", host: true }\n---\n## B\n@narrator: {{@local}}\n",
    );
    let out = Command::new(BIN)
        .arg("check")
        .arg(dir.join("b.lute"))
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains(
        "[E-DEF-DECL] invalid def `local`: `host:` is not a def key; a def is `{ type: bool, cel: \"…\" }` \
         with an optional `params: { p: <type> }`, or the shorthand `local: \"<CEL>\"` — a frontmatter \
         def is local to its document; declare a host def in a project schema file"
    ), "{text}");
}
