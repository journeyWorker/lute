//! 0.42.0 §3.2 (#77): `player.bindings.yaml`'s `volumes:` maps each member
//! of the project's `volume` domain to a gain in `[0, 1]` — an unknown key is
//! `E-BINDING-KEY`, a member without a key is `W-BINDING-MISSING`, and a gain
//! that is no finite number in range is `E-BINDING-SHAPE` at its value.

use std::path::Path;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn write(dir: &Path, rel: &str, text: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

/// A project whose schema declares `volume` when `declared`, plus bindings
/// with the given `volumes:` line; returns the whole-project check text output.
fn check(tag: &str, declared: bool, volumes: &str) -> (i32, String) {
    let dir = lute_scratch::ScratchDir::named(tag);
    write(
        &dir,
        "lute.project.yaml",
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\ndefaults:\n  uses: [world.schema.yaml]\n",
    );
    let enums = if declared {
        "enums:\n  volume: [silent, quiet, normal, loud, full]\n"
    } else {
        "enums:\n  mood: [calm]\n"
    };
    write(&dir, "world.schema.yaml", enums);
    write(
        &dir,
        "scenes/start.lute",
        "---\nkind: scene\nid: start\non: start\n---\n## Start\n@narrator: Hello.\n",
    );
    write(
        &dir,
        "player.bindings.yaml",
        &format!("bindingsVersion: \"0.49\"\ncanvas: {{width: 1920, height: 1080}}\n{volumes}"),
    );
    let out = Command::new(BIN)
        .arg("check")
        .arg("--project")
        .arg(&*dir)
        .output()
        .unwrap();
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (
        out.status.code().unwrap(),
        text.replace(&*dir.to_string_lossy(), "."),
    )
}

#[test]
fn a_full_table_is_clean() {
    let (code, text) = check(
        "volume-042-clean",
        true,
        "volumes: { silent: 0, quiet: 0.25, normal: 0.5, loud: 0.75, full: 1 }\n",
    );
    assert_eq!(code, 0, "{text}");
    assert!(!text.contains("BINDING"), "{text}");
}

#[test]
fn unknown_keys_and_missing_members_are_reported() {
    let (code, text) = check(
        "volume-042-keys",
        true,
        "volumes:\n  silent: 0\n  quite: 0.25\n  normal: 0.5\n  loud: 0.75\n",
    );
    assert_eq!(code, 1, "{text}");
    assert!(
        text.contains(
            "error [E-BINDING-KEY] volume binding key `quite` is not declared; did you mean `quiet`?"
        ),
        "{text}"
    );
    for member in ["quiet", "full"] {
        assert!(
            text.contains(&format!(
                "warning [W-BINDING-MISSING] declared volume member `{member}` has no binding"
            )),
            "{text}"
        );
    }
    assert!(!text.contains("member `silent` has no binding"), "{text}");
}

#[test]
fn gains_outside_the_unit_interval_are_shape_errors_at_the_value() {
    let (code, text) = check(
        "volume-042-shape",
        true,
        "volumes:\n  silent: -0.1\n  quiet: .nan\n  normal: 0.5\n  loud: 1.5\n  full: .inf\n",
    );
    assert_eq!(code, 1, "{text}");
    for (at, member, got) in [
        ("4:11", "silent", "-0.1"),
        ("5:10", "quiet", "NaN"),
        ("7:9", "loud", "1.5"),
        ("8:9", "full", "inf"),
    ] {
        assert!(
            text.contains(&format!(
                "./player.bindings.yaml:{at}: error [E-BINDING-SHAPE] volume binding `{member}` gain \
                 must be a finite number in [0, 1], got `{got}`\n"
            )),
            "{member}: {text}"
        );
    }
    assert!(!text.contains("volume binding `normal`"), "{text}");
}

#[test]
fn a_gain_that_is_no_number_does_not_decode() {
    let (code, text) = check("volume-042-string", true, "volumes: { silent: zero }\n");
    assert_eq!(code, 1, "{text}");
    assert!(text.contains("error [E-BINDING-SHAPE]"), "{text}");
}

#[test]
fn a_project_without_a_volume_domain_gets_no_member_diagnostics() {
    let (code, text) = check("volume-042-none", false, "volumes: { anything: 0.5 }\n");
    assert_eq!(code, 0, "{text}");
    assert!(!text.contains("volume"), "{text}");
}
