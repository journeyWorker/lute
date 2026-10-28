//! 0.41.0 §3.1: `lute fix` renames renamed domains in YAML (keys under
//! `enums:`/`entities:`, `domain:` scalars), preserving comments and layout,
//! idempotently; a mapping that already declares the new name keeps the old
//! key with a note. `E-DOMAIN-UNKNOWN` names the rename.

use std::path::Path;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn write(dir: &Path, rel: &str, text: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

fn fix(path: &Path) -> (String, String) {
    let out = Command::new(BIN).arg("fix").arg(path).output().unwrap();
    assert!(out.status.success());
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn fix_renames_domain_keys_and_references_in_place() {
    let dir = lute_scratch::ScratchDir::named("fix-domains-041");
    let schema = "# vocabulary\nenums:\n  # the music verbs\n  musicAction: [start, stop]   # kept comment\n  mood: [calm]\n\
                  state:\n  run.music: { type: { domain: musicAction }, default: start }\n  \
                  run.last:\n    type:\n      domain: \"musicAction\"\n";
    write(&dir, "world.schema.yaml", schema);
    let (stdout, _) = fix(&dir);
    assert!(
        stdout.contains("lute: applied 3 fix(es) in 1 of 1 file(s)"),
        "{stdout}"
    );
    let fixed = std::fs::read_to_string(dir.join("world.schema.yaml")).unwrap();
    assert_eq!(fixed, schema.replace("musicAction", "musicPlayback"));
    let (again, _) = fix(&dir);
    assert!(
        again.contains("lute: applied 0 fix(es) in 0 of 1 file(s)"),
        "{again}"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("world.schema.yaml")).unwrap(),
        fixed
    );
}

#[test]
fn fix_keeps_an_old_key_whose_mapping_declares_the_new_one() {
    let dir = lute_scratch::ScratchDir::named("fix-domains-041-kept");
    let text = "enums:\n  musicAction: [start]\n  musicPlayback: [start, stop]\n";
    write(&dir, "v.yaml", text);
    let (_, stderr) = fix(&dir.join("v.yaml"));
    let file = dir.join("v.yaml");
    assert!(
        stderr.contains(&format!(
            "lute fix: {}:2: kept `musicAction:` — `musicPlayback:` is already declared",
            file.display()
        )),
        "{stderr}"
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), text);
}

#[test]
fn domain_unknown_names_the_rename() {
    let dir = lute_scratch::ScratchDir::named("domain-hint-041");
    write(
        &dir,
        "a.lute",
        "---\nkind: scene\nid: a\nenums:\n  musicAction: [start]\n---\n## A\n::music{playback=\"start\"}\n",
    );
    let out = Command::new(BIN)
        .arg("check")
        .arg(dir.join("a.lute"))
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains(
        "[E-DOMAIN-UNKNOWN] `musicPlayback` is not a declared domain"
    ) && text.contains(
        " — this project declares `musicAction`, renamed `musicPlayback` in 0.37.0; run `lute fix`"
    ), "{text}");
}

/// Every `E-DOMAIN-UNKNOWN` site family appends the rename hint for a table
/// name whose old name the vocabulary declares; `anchor` and `textStyle` are
/// no renamed domain, so their messages never carry one.
#[test]
fn every_domain_unknown_site_names_the_rename() {
    const HINT: &str =
        "this project declares `musicAction`, renamed `musicPlayback` in 0.37.0; run `lute fix`";
    // (case, files written, whether the hint is expected)
    type Case<'a> = (&'a str, &'a [(&'a str, &'a str)], bool);
    let cases: [Case<'_>; 4] = [
        (
            "state-decl",
            &[(
                "a.lute",
                "---\nkind: scene\nid: a\nenums:\n  musicAction: [start]\nstate:\n  \
                 run.m: { type: { domain: musicPlayback }, default: start }\n---\n## A\n@narrator: x\n",
            )],
            true,
        ),
        (
            "occasion-bind",
            &[
                (
                    "lute.project.yaml",
                    "pluginsDir: plugins/\ndefaultProfile: game\nprofiles:\n  game:\n    plugins: { game.occ: true }\n\
                     defaults:\n  luteVersion: \"0.53.0\"\n",
                ),
                (
                    "plugins/game.occ/plugin.yaml",
                    "id: game.occ\nversion: 0.1.0\nkind: capability\ndepends: [ { id: lute.core, range: \"^0.0.1\" } ]\n\
                     exports:\n  occasions: occasions/\n",
                ),
                (
                    "plugins/game.occ/occasions/o.yaml",
                    "occasions:\n  dive: { select: first, payload: { song: { domain: musicPlayback } } }\n",
                ),
                (
                    "a.lute",
                    "---\nkind: scene\nid: a\non: dive\nenums:\n  musicAction: [start]\n---\n## A\n@narrator: x\n",
                ),
            ],
            true,
        ),
        (
            "anchor",
            &[(
                "a.lute",
                "---\nkind: scene\nid: a\nenums:\n  musicAction: [start]\n---\n## A\n::actor{character=\"mara\"}\n",
            )],
            false,
        ),
        (
            "text-style",
            &[(
                "a.lute",
                "---\nkind: scene\nid: a\nenums:\n  musicAction: [start]\n---\n## A\n@narrator: a :shout[b] c\n",
            )],
            false,
        ),
    ];
    for (tag, files, hinted) in cases {
        let dir = lute_scratch::ScratchDir::named(&format!("domain-hint-041-{tag}"));
        for (rel, text) in files {
            write(&dir, rel, text);
        }
        let out = Command::new(BIN)
            .arg("check")
            .arg("--project")
            .arg(&*dir)
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        let unknown: Vec<&str> = text
            .lines()
            .filter(|l| l.contains("[E-DOMAIN-UNKNOWN]"))
            .collect();
        assert!(!unknown.is_empty(), "{tag}: no E-DOMAIN-UNKNOWN:\n{text}");
        assert_eq!(
            unknown.iter().all(|l| l.contains(HINT)),
            hinted,
            "{tag}:\n{text}"
        );
    }
}

/// A flow-map `enums:` / `entities:` (quoted keys too, across lines) is
/// renamed at its first level only; a nested flow value is untouched; a flow
/// map that already declares the new name keeps the old key with the note.
#[test]
fn fix_renames_flow_map_keys_at_the_first_level_only() {
    let dir = lute_scratch::ScratchDir::named("fix-domains-041-flow");
    let text = "enums: { musicAction: [start, stop], mood: { members: [musicAction] } }  # kept\n\
                entities: {\n  'musicAction': { members: [a] },  # quoted\n  npc: { members: [musicAction] }\n}\n";
    write(&dir, "a.yaml", text);
    let (stdout, _) = fix(&dir.join("a.yaml"));
    assert!(stdout.contains("applied 2 fix(es)"), "{stdout}");
    let want = "enums: { musicPlayback: [start, stop], mood: { members: [musicAction] } }  # kept\n\
                entities: {\n  'musicPlayback': { members: [a] },  # quoted\n  npc: { members: [musicAction] }\n}\n";
    assert_eq!(std::fs::read_to_string(dir.join("a.yaml")).unwrap(), want);
    let (again, _) = fix(&dir.join("a.yaml"));
    assert!(again.contains("nothing to fix"), "{again}");
    assert_eq!(std::fs::read_to_string(dir.join("a.yaml")).unwrap(), want);

    let conflict = "enums: { musicAction: [start], musicPlayback: [start, stop] }\n";
    write(&dir, "b.yaml", conflict);
    let file = dir.join("b.yaml");
    let (_, stderr) = fix(&file);
    assert!(
        stderr.contains(&format!(
            "lute fix: {}:1: kept `musicAction:` — `musicPlayback:` is already declared",
            file.display()
        )),
        "{stderr}"
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), conflict);
}
