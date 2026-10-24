//! lute 0.47.1 §2 (#133): a bindings `flow.*.target` is the full occasion
//! target the runtime's `raiseOccasion` takes (`place.square`), checked by
//! the rule beat and play step targets share (`occasion_target_ok`). The
//! `flow-target-0471` fixture writes its idle menu's targets prefixed; the
//! player suite raises them through the runtime
//! (`packages/player/test/flow-target.test.ts`).

use std::path::{Path, PathBuf};
use std::process::Command;

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/flow-target-0471")
}

/// `whole-project check --json`'s exit code and `E-BINDING-KEY` messages for the
/// fixture with its bindings (and, when given, its occasions) replaced.
fn check(tag: &str, bindings: Option<&str>, occasions: Option<&str>) -> (i32, Vec<String>) {
    let dir = lute_scratch::ScratchDir::named(tag);
    copy_dir(&fixture(), &dir);
    if let Some(bindings) = bindings {
        std::fs::write(dir.join("player.bindings.yaml"), bindings).unwrap();
    }
    if let Some(occasions) = occasions {
        std::fs::write(dir.join("plugins/town/occasions/town.yaml"), occasions).unwrap();
    }
    let out = Command::new(env!("CARGO_BIN_EXE_lute"))
        .args(["check", "--project", &dir.to_string_lossy(), "--json"])
        .output()
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let keys = value["project_diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["code"] == "E-BINDING-KEY")
        .map(|d| d["message"].as_str().unwrap().to_string())
        .collect();
    (out.status.code().unwrap(), keys)
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn menu(targets: &[(&str, &str)]) -> String {
    let items: String = targets
        .iter()
        .map(|(occasion, target)| {
            format!("      - {{ label: \"Go\", occasion: {occasion}, target: {target} }}\n")
        })
        .collect();
    format!(
        "bindingsVersion: \"0.49\"\ncanvas: {{ width: 1920, height: 1080 }}\nflow:\n  \
         newGame: {{ occasion: begin }}\n  idle:\n    menu:\n{items}"
    )
}

#[test]
fn prefixed_flow_targets_are_accepted() {
    let (code, keys) = check("flow-target-prefixed", None, None);
    assert_eq!((code, keys), (0, Vec::<String>::new()));
}

#[test]
fn a_bare_member_is_refused_naming_the_prefixed_form() {
    let (code, keys) = check(
        "flow-target-bare",
        Some(&menu(&[("visit", "square")])),
        None,
    );
    assert_eq!(code, 1);
    assert_eq!(keys.len(), 1, "{keys:?}");
    assert!(
        keys[0].starts_with("flow target `square`: target `square` is outside occasion `visit`'s domain `place.<place>`"),
        "{keys:?}"
    );
    assert!(
        keys[0].ends_with("did you mean `place.square`?"),
        "{keys:?}"
    );
}

#[test]
fn a_member_outside_the_domain_is_refused() {
    let (code, keys) = check(
        "flow-target-outside",
        Some(&menu(&[("visit", "place.harbor")])),
        None,
    );
    assert_eq!(code, 1);
    assert_eq!(keys.len(), 1, "{keys:?}");
    assert!(
        keys[0].starts_with("flow target `place.harbor`: target `place.harbor` is outside"),
        "{keys:?}"
    );
}

#[test]
fn a_target_on_an_occasion_without_one_is_refused() {
    let (code, keys) = check(
        "flow-target-targetless",
        Some(&menu(&[("begin", "place.square")])),
        None,
    );
    assert_eq!(code, 1);
    assert_eq!(
        keys,
        [
            "flow target `place.square`: occasion `begin` is not declared `target: true`, so it \
          cannot be raised for `place.square`"
        ]
    );
}

#[test]
fn an_open_kind_admits_any_prefixed_id_and_names_the_form() {
    let world = "occasions:\n  begin: { select: first }\n  \
                 visit: { select: first, target: { prefix: place, entity: place } }\n  \
                 roam: { select: first, target: { prefix: spot, entity: spot } }\n";
    let schema = |dir: &Path| {
        std::fs::write(
            dir.join("world.schema.yaml"),
            "state:\n  run.over: { type: bool, default: false }\nentities:\n  \
             place: { members: [square, pier] }\n  spot: { open: engine }\n",
        )
        .unwrap();
    };
    for (target, refused) in [("spot.anywhere", false), ("anywhere", true)] {
        let dir = lute_scratch::ScratchDir::named("flow-target-open");
        copy_dir(&fixture(), &dir);
        schema(&dir);
        std::fs::write(dir.join("plugins/town/occasions/town.yaml"), world).unwrap();
        std::fs::write(dir.join("player.bindings.yaml"), menu(&[("roam", target)])).unwrap();
        let out = Command::new(env!("CARGO_BIN_EXE_lute"))
            .args(["check", "--project", &dir.to_string_lossy(), "--json"])
            .output()
            .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        let keys: Vec<&str> = value["project_diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|d| d["code"] == "E-BINDING-KEY")
            .map(|d| d["message"].as_str().unwrap())
            .collect();
        if refused {
            assert_eq!(
                keys,
                [
                    "flow target `anywhere`: target `anywhere` is outside occasion `roam`'s \
                  domain: its targets are `spot.<spot>`"
                ],
                "{value}"
            );
        } else {
            assert!(keys.is_empty(), "{value}");
        }
    }
}

#[test]
fn an_undeclared_entity_kind_refuses_its_flow_targets() {
    let occasions = "occasions:\n  begin: { select: first }\n  \
                     visit: { select: first, target: { prefix: place, entity: place } }\n  \
                     roam: { select: first, target: { prefix: spot, entity: spot } }\n";
    let (code, keys) = check(
        "flow-target-undeclared",
        Some(&menu(&[("roam", "spot.anywhere")])),
        Some(occasions),
    );
    assert_eq!(code, 1);
    assert_eq!(
        keys,
        [
            "flow target `spot.anywhere`: occasion `roam` draws its targets from entity kind \
          `spot`, which the project does not declare under `entities:`"
        ]
    );
}
