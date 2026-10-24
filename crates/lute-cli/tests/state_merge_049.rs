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

/// F5b review R2: an inline family over an imported kind beside an
/// explicit member of it reports its members in path order — the order the
/// family over the scene's own kind gives — for a `uses` redeclaration and
/// an `extends` retype alike.
#[test]
fn a_family_beside_an_explicit_member_reports_in_path_order() {
    for (link, code) in [
        ("uses", "E-STATE-REDECLARE"),
        ("extends", "E-EXTENDS-STATE-TYPE"),
    ] {
        let state = "  run.rep: { type: string, default: x, per: npc }\n  \
                     run.rep.tavi: { type: string, default: y }\n";
        let pending = scene_project(&format!("state-merge-049-r2-{link}"), link, state, "");
        let own = scene_project(
            &format!("state-merge-049-r2-{link}-own"),
            link,
            state,
            OWN_NPC,
        );
        let (_, out) = run(&pending, &["check", "--project", "."]);
        let (_, own_out) = run(&own, &["check", "--project", "."]);
        assert_eq!(
            state_errors(&out, code),
            ["run.rep.brann", "run.rep.tavi"],
            "{link}: {out}"
        );
        assert_eq!(
            state_errors(&out, code),
            state_errors(&own_out, code),
            "{link}: {own_out}"
        );
    }
}

// ---- #196: the schema tier ---------------------------------------------

/// A base schema declaring `npc` and `run.rep` per `npc`, a second schema
/// linking it with `link` and declaring `family` over `npc` (a kind only
/// the base declares), and a scene using the second schema.
fn schema_project(name: &str, link: &str, family: &str) -> lute_scratch::ScratchDir {
    let dir = lute_scratch::ScratchDir::named(name);
    let child = format!("{link}: a.schema.yaml\nstate:\n  run.rep: {family}\n");
    write(
        &dir,
        &[
            (
                "a.schema.yaml",
                "state:\n  run.rep: { type: int, default: 0, per: npc }\n\
                 entities:\n  npc: { members: [tavi, brann] }\n",
            ),
            ("b.schema.yaml", &child),
            (
                "s.lute",
                "---\nkind: scene\nid: s\nuses: b.schema.yaml\n---\n\n## X\n\n\
                 @narrator: {{run.rep.tavi}}\n",
            ),
        ],
    );
    dir
}

/// The paths of the top-level `code` reports (related lines are indented).
fn top_errors(out: &str, code: &str) -> Vec<String> {
    let top: String = out
        .lines()
        .filter(|l| !l.starts_with(' '))
        .collect::<Vec<_>>()
        .join("\n");
    state_errors(&top, code)
}

#[test]
fn a_uses_peer_schema_family_over_an_imported_kind_is_a_duplicate() {
    let dir = schema_project(
        "state-merge-049-196-uses",
        "uses",
        "{ type: int, default: 3, per: npc }",
    );
    let (code, out) = run(&dir, &["check", "s.lute"]);
    assert_eq!(code, 1, "{out}");
    assert_eq!(
        top_errors(&out, "E-USES-DUP-STATE"),
        ["run.rep.brann", "run.rep.tavi"],
        "{out}"
    );
    // Named at the family's key in the later import.
    assert!(
        out.contains("(`b.schema.yaml:3` and `a.schema.yaml`)"),
        "{out}"
    );
}

#[test]
fn an_extends_schema_family_over_an_imported_kind_refines_the_ir() {
    let dir = schema_project(
        "state-merge-049-196-extends",
        "extends",
        "{ type: int, default: 3, per: npc }",
    );
    let (code, out) = run(&dir, &["compile", "s.lute", "-o", "s.json"]);
    assert_eq!(code, 0, "{out}");
    let ir: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("s.json")).unwrap()).unwrap();
    let defaults: Vec<_> = ir["state"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["path"].as_str().unwrap().starts_with("run.rep."))
        .map(|row| (row["path"].clone(), row["default"].clone()))
        .collect();
    assert_eq!(
        defaults,
        [
            (serde_json::json!("run.rep.brann"), serde_json::json!(3)),
            (serde_json::json!("run.rep.tavi"), serde_json::json!(3)),
        ]
    );
}

#[test]
fn an_extends_schema_family_retyping_an_imported_kind_is_refused() {
    let dir = schema_project(
        "state-merge-049-196-retype",
        "extends",
        "{ type: string, default: x, per: npc }",
    );
    let (code, out) = run(&dir, &["check", "s.lute"]);
    assert_eq!(code, 1, "{out}");
    assert_eq!(
        top_errors(&out, "E-EXTENDS-STATE-TYPE"),
        ["run.rep.brann", "run.rep.tavi"],
        "{out}"
    );
}

/// F5b review R1: the winning schema's family over another schema's kind
/// is where its paths are declared. Its default outside the enum is
/// `E-STATE-DECL` at its own key in `b.schema.yaml` — whether the base
/// family in `a` is itself over another schema's kind (`c`) or over `a`'s
/// own.
#[test]
fn a_refined_family_over_an_imported_kind_is_located_at_the_refinement() {
    const B: &str = "extends: a.schema.yaml\nstate:\n  \
                     run.rep: { type: { domain: mood }, default: bad, per: npc }\n";
    const SCENE: &str = "---\nkind: scene\nid: s\nuses: b.schema.yaml\n---\n\n## X\n\n\
                         @narrator: {{run.rep.tavi}}\n";
    let kinds = "entities:\n  npc: { members: [tavi] }\nenums:\n  mood: [good]\n";
    let family = "  run.rep: { type: { domain: mood }, default: good, per: npc }\n";
    let in_c = format!("uses: c.schema.yaml\nstate:\n{family}");
    let in_a = format!("{kinds}state:\n{family}");
    for (name, files) in [
        (
            "kind-in-c",
            vec![("c.schema.yaml", kinds), ("a.schema.yaml", in_c.as_str())],
        ),
        ("kind-in-a", vec![("a.schema.yaml", in_a.as_str())]),
    ] {
        let dir = lute_scratch::ScratchDir::named(&format!("state-merge-049-r1-{name}"));
        let mut all = files;
        all.extend([("b.schema.yaml", B), ("s.lute", SCENE)]);
        write(&dir, &all);
        let (code, out) = run(&dir, &["check", "s.lute"]);
        assert_eq!(code, 1, "{name}: {out}");
        assert!(
            out.contains(
                "    b.schema.yaml:3:3: error [E-STATE-DECL] `run.rep`'s `default: bad` is not a \
                 member of enum `mood` [good]"
            ),
            "{name}: {out}"
        );
    }
}
