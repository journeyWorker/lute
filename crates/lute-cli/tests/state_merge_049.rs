//! 0.49.0 §2 (no silent drop), #190: an inline `per:` family over a kind
//! only an imported schema declares goes through the same merge as any
//! inline path. A `uses`-peer redeclaration of an imported family is
//! `E-STATE-REDECLARE`, an `extends` refinement applies (its members take
//! the refined default in the IR), and a type change is
//! `E-EXTENDS-STATE-TYPE` — exactly as for the family over a kind the scene
//! declares itself.

use std::path::Path;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

const WORLD: &str = "state:\n  run.bond: { type: int, default: 0, per: npc }\n  \
                     run.rep: { type: int, default: 0, per: npc }\n\
                     entities:\n  npc: { members: [tavi, brann] }\n";

fn write(dir: &Path, files: &[(&str, &str)]) {
    for (rel, text) in files {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().expect("file has a parent")).unwrap();
        std::fs::write(path, text).unwrap();
    }
}

fn run(dir: &Path, args: &[&str]) -> (i32, String) {
    let out = Command::new(BIN)
        .current_dir(dir)
        .env("LUTE_TELEMETRY", "0")
        .args(args)
        .output()
        .unwrap();
    (
        out.status.code().unwrap(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    )
}

/// A project of the shared schema and one scene linking it with `link`
/// (`uses` or `extends`), declaring `state` (and `entities`, if any).
fn scene_project(name: &str, link: &str, state: &str, entities: &str) -> lute_scratch::ScratchDir {
    let dir = lute_scratch::ScratchDir::named(name);
    let scene = format!(
        "---\nkind: scene\nid: scene.s000\n{link}: ../schema/world.schema.yaml\nstate:\n{state}{entities}---\n\n\
         ## Retyped\n\n@narrator: Bond {{{{run.bond.tavi}}}} {{{{run.rep.brann}}}}.\n\
         ::set{{run.bond.tavi += 1}}\n"
    );
    write(
        &dir,
        &[
            ("schema/world.schema.yaml", WORLD),
            ("scenes/s000.lute", &scene),
        ],
    );
    dir
}

/// The `E-…` codes and paths a project check reports for the scene.
fn state_errors(out: &str, code: &str) -> Vec<String> {
    out.lines()
        .filter(|l| l.contains(&format!("[{code}]")))
        .filter_map(|l| l.split('`').nth(1).map(str::to_string))
        .collect()
}

const OWN_NPC: &str = "entities:\n  npc: { members: [tavi, brann] }\n";

#[test]
fn a_uses_peer_family_redeclared_over_an_imported_kind_is_refused() {
    let dir = scene_project(
        "state-merge-049-uses",
        "uses",
        "  run.bond: { type: string, per: npc }\n",
        "",
    );
    let (code, out) = run(&dir, &["check", "--project", "."]);
    assert_eq!(code, 1, "{out}");
    assert_eq!(
        state_errors(&out, "E-STATE-REDECLARE"),
        ["run.bond.brann", "run.bond.tavi"],
        "{out}"
    );
    // The same as the family over a kind the scene declares itself.
    let own = scene_project(
        "state-merge-049-uses-own",
        "uses",
        "  run.bond: { type: string, per: npc }\n",
        OWN_NPC,
    );
    let (own_code, own_out) = run(&own, &["check", "--project", "."]);
    assert_eq!(
        (own_code, state_errors(&own_out, "E-STATE-REDECLARE")),
        (code, state_errors(&out, "E-STATE-REDECLARE"))
    );
}

#[test]
fn an_extends_refinement_over_an_imported_kind_reaches_the_ir() {
    let dir = scene_project(
        "state-merge-049-extends",
        "extends",
        "  run.rep: { type: int, default: 3, per: npc }\n  run.rep.tavi: { type: int, default: 7 }\n",
        "",
    );
    let (code, out) = run(&dir, &["compile", "--project", ".", "-o", "out"]);
    assert_eq!(code, 0, "{out}");
    let ir: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join("out/scenes/s000.lute.json")).unwrap(),
    )
    .unwrap();
    let default = |path: &str| {
        ir["state"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["path"] == path)
            .map(|row| row["default"].clone())
    };
    // The refined family default, a member the scene declares itself, and
    // an untouched imported family.
    assert_eq!(default("run.rep.brann"), Some(serde_json::json!(3)));
    assert_eq!(default("run.rep.tavi"), Some(serde_json::json!(7)));
    assert_eq!(default("run.bond.tavi"), Some(serde_json::json!(0)));
}

#[test]
fn an_extends_type_change_over_an_imported_kind_is_refused() {
    let dir = scene_project(
        "state-merge-049-retype",
        "extends",
        "  run.rep: { type: string, per: npc }\n",
        "",
    );
    let (code, out) = run(&dir, &["check", "--project", "."]);
    assert_eq!(code, 1, "{out}");
    assert_eq!(
        state_errors(&out, "E-EXTENDS-STATE-TYPE"),
        ["run.rep.brann", "run.rep.tavi"],
        "{out}"
    );
}
