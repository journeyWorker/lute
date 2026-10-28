//! spec 0.53.0 §4 (#224): `lute fix` reads each document's capability
//! snapshot, keeps a plugin stamp named like a 0.37 renamed attribute, names
//! its rules per file and reports the kept sites (stderr, and `kept` in the
//! `0.53.0.fix` report). The #224 project fails on v0.52.0: `fix --check`
//! reports 3 fixes on `scenes/intro.lute`, and the fixed project fails
//! `check`.

mod support;

use std::path::Path;
use std::process::{Command, Output};

use serde_json::json;
use support::{TempProject, BIN};

fn lute_in(cwd: &Path, args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .current_dir(cwd)
        .env("LUTE_TELEMETRY", "0")
        .output()
        .expect("run lute CLI")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

const MANIFEST: &str = "pluginsDir: plugins/\ndefaultProfile: game\nprofiles:\n  game:\n    \
                        plugins:\n      acme.stage: true\ndefaults:\n  luteVersion: 0.48.1\n  \
                        contentLang: en\n  uses: [schema/world.schema.yaml]\n";
const PLUGIN: &str = "id: acme.stage\nversion: 0.1.0\nkind: capability\ndepends:\n  - { id: \
                      lute.core, range: \"^0.0.1\" }\nexports:\n  stampAttrs: stampattrs/\n";
const SCENE: &str = "---\nkind: scene\nid: intro\n---\n\n## Hall {#hall}\n\n\
                     ::bg{assetId=\"BG.hall\"}\n\
                     ::cg{assetId=\"CUT.a\" action=\"fade-in\"}\n\
                     ::music{assetId=\"BGM.a\" action=\"duck\"}\n\
                     ::video{assetId=\"MOV.a\" action=\"slow\"}\n\
                     @mira: Hello.\n";

/// The issue's project; `stamp` is the plugin's `stampAttrs` entry.
fn issue_project(tag: &str, stamp: &str) -> TempProject {
    let p = TempProject::new(tag);
    p.write("lute.project.yaml", MANIFEST);
    p.write("plugins/acme.stage/plugin.yaml", PLUGIN);
    p.write(
        "plugins/acme.stage/stampattrs/columns.yaml",
        &format!("stampAttrs:\n  - {stamp}\n"),
    );
    p.write(
        "schema/world.schema.yaml",
        "cast:\n  mira: { name: Mira }\n",
    );
    p.write("scenes/intro.lute", SCENE);
    p
}

const KEPT: [(&str, u32, u32, &str); 3] = [
    ("cg", 9, 22, "display"),
    ("music", 10, 25, "playback"),
    ("video", 11, 25, "display"),
];

#[test]
fn the_issue_project_keeps_its_stamps() {
    let p = issue_project("fix-stamps-224", "{ name: action, type: string }");
    let out = lute_in(&p, &["fix", "--check", "."]);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out.stderr));
    assert_eq!(
        text(&out.stdout),
        "lute: ./lute.project.yaml: would apply 1 fix(es): lute-version 1\n\
         lute: would apply 1 fix(es) in 1 of 5 file(s)\n"
    );
    let want: String = KEPT
        .iter()
        .map(|(tag, line, column, new)| {
            format!(
                "lute fix: ./scenes/intro.lute:{line}:{column}: kept `action` on `::{tag}` — \
                 the active profile declares it as a stamp attribute; the 0.37 rename to \
                 `{new}` does not apply\n"
            )
        })
        .collect();
    assert_eq!(text(&out.stderr), want);

    let out = lute_in(&p, &["fix", "--check", "--json", "."]);
    assert_eq!(out.status.code(), Some(1));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("JSON report");
    assert_eq!(v["schemaVersion"], "0.53.0.fix");
    assert_eq!(
        v["files"][0],
        json!({
            "path": "./lute.project.yaml",
            "changed": true,
            "fixes": 1,
            "rules": [{ "rule": "lute-version", "fixes": 1 }],
            "kept": [],
        })
    );
    assert_eq!(
        v["files"][3],
        json!({
            "path": "./scenes/intro.lute",
            "changed": false,
            "fixes": 0,
            "rules": [],
            "kept": KEPT
                .iter()
                .map(|(_, line, column, _)| json!({
                    "line": line,
                    "column": column,
                    "rule": "renamed-attr",
                    "reason": "stamp",
                }))
                .collect::<Vec<_>>(),
        })
    );

    let out = lute_in(&p, &["fix", "."]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(
        text(&out.stdout).ends_with("lute: applied 1 fix(es) in 1 of 5 file(s)\n"),
        "{}",
        text(&out.stdout)
    );
    assert_eq!(
        std::fs::read_to_string(p.join("scenes/intro.lute")).unwrap(),
        SCENE
    );
    let check = lute_in(&p, &["check", "--project", "."]);
    assert_eq!(check.status.code(), Some(0), "{}", text(&check.stdout));
    // Idempotent: a second run applies nothing.
    let again = lute_in(&p, &["fix", "--check", "."]);
    assert_eq!(again.status.code(), Some(0), "{}", text(&again.stdout));
}

/// A stamp scoped `on: [music]` keeps `::music{action}`; `::cg{action}` and
/// `::video{action}` are still the 0.37 spelling there.
#[test]
fn a_scoped_stamp_keeps_only_its_surface() {
    let p = issue_project(
        "fix-stamps-scoped",
        "{ name: action, type: string, on: [music] }",
    );
    let out = lute_in(&p, &["fix", "--check", "scenes/intro.lute"]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        text(&out.stdout),
        "lute: would apply 2 fix(es): renamed-attr 2\n"
    );
    assert_eq!(
        text(&out.stderr),
        "lute fix: scenes/intro.lute:10:25: kept `action` on `::music` — the active profile \
         declares it as a stamp attribute; the 0.37 rename to `playback` does not apply\n"
    );
}

/// A file selected through a directory symlink, which model discovery does
/// not descend, is assembled under its root: its snapshot resolves and the
/// scoped stamp and the renames apply as for a listed file (F review).
#[test]
fn a_file_behind_a_directory_symlink_reads_its_snapshot() {
    let p = issue_project(
        "fix-stamps-symlink",
        "{ name: action, type: string, on: [music] }",
    );
    let sources = TempProject::new("fix-stamps-symlink-sources");
    sources.write("intro.lute", SCENE);
    std::fs::remove_file(p.join("scenes/intro.lute")).unwrap();
    std::os::unix::fs::symlink(sources.join("."), p.join("linked")).unwrap();
    let out = lute_in(&p, &["fix", "--check", "linked/intro.lute"]);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out.stderr));
    assert_eq!(
        text(&out.stdout),
        "lute: would apply 2 fix(es): renamed-attr 2\n"
    );
    assert_eq!(
        text(&out.stderr),
        "lute fix: linked/intro.lute:10:25: kept `action` on `::music` — the active profile \
         declares it as a stamp attribute; the 0.37 rename to `playback` does not apply\n"
    );
}

/// A document whose plugin is missing: its snapshot does not resolve, so
/// every rename site is kept with `unresolved`; the other rules still run.
#[test]
fn an_unresolved_document_is_kept() {
    let p = issue_project("fix-stamps-missing", "{ name: action, type: string }");
    std::fs::remove_dir_all(p.join("plugins/acme.stage")).unwrap();
    let out = lute_in(&p, &["fix", "--check", "--json", "."]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("JSON report");
    let scene = v["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["path"] == "./scenes/intro.lute")
        .expect("the scene is reported");
    assert_eq!(scene["fixes"], 0, "{v}");
    let reasons: Vec<&str> = scene["kept"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k["reason"].as_str().unwrap())
        .collect();
    assert_eq!(reasons, ["unresolved"; 3], "{v}");
    assert!(
        text(&out.stderr).contains(
            "lute fix: ./scenes/intro.lute:9:22: kept `action` on `::cg` — the document's \
             capability snapshot did not resolve"
        ),
        "{}",
        text(&out.stderr)
    );
}

/// A file under no manifest is judged against its own assembly (the core
/// snapshot here): a real 0.37 spelling is renamed.
#[test]
fn a_loose_file_still_renames_a_real_037_spelling() {
    let p = TempProject::new("fix-stamps-loose");
    p.write(
        "s.lute",
        "---\nkind: scene\nid: s\n---\n## S\n::cg{assetId=\"C\" action=\"show\"}\n",
    );
    let out = lute_in(&p, &["fix", "s.lute"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert_eq!(
        text(&out.stdout),
        "lute: applied 1 fix(es): renamed-attr 1\n"
    );
    assert_eq!(text(&out.stderr), "");
    assert_eq!(
        std::fs::read_to_string(p.join("s.lute")).unwrap(),
        "---\nkind: scene\nid: s\n---\n## S\n::cg{assetId=\"C\" display=\"show\"}\n"
    );
}
