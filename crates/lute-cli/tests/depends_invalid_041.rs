//! 0.41.0 §3.2: an invalid declaration does not cascade. The fixture
//! `fixtures/depends-invalid` declares `type: number` in a state row the
//! `clock:` names, two defs, a directive attribute, a bridge result, a state
//! shape and an occasion payload.

use std::path::PathBuf;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/depends-invalid")
}

fn check_project() -> (String, String) {
    let out = Command::new(BIN)
        .arg("check-project")
        .arg(".")
        .current_dir(fixture())
        .output()
        .unwrap();
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn invalid_declarations_report_once_and_dependants_carry_one_note_each() {
    let (stdout, stderr) = check_project();
    let all = format!("{stdout}{stderr}");
    for absent in [
        "E-CLOCK-DECL",
        "E-UNKNOWN-DIRECTIVE",
        "E-PLUGIN-PARSE",
        "E-PLUGIN-MISSING-ACTIVE",
        "E-STATE-DECL",
        "E-DEF-DECL",
        "E-CEL-TYPE",
        "E-CEL-PROFILE",
        "E-REF-ARG-TYPE",
        "E-OCCASION-UNKNOWN",
    ] {
        assert!(!all.contains(absent), "{absent} cascaded:\n{all}");
    }
    // One E-TYPE-NUMBER per `number`: four plugin sites, two state rows,
    // a def parameter and a def type.
    assert_eq!(all.matches("[E-TYPE-NUMBER]").count(), 8, "{all}");
    // The `int` suggestion reads the row's default and the project's literal
    // writes of it (`::set{run.gold = 5}`, `::set{run.gold += 2}`).
    assert!(stdout.contains(
        "world.schema.yaml:9:27: error [E-TYPE-NUMBER] `type: number` is no longer a numeric type \
         — choose `type: int` for whole numbers or `type: double` for fractional values; every \
         value it takes here is whole (0, 5, 2), so `int` fits (imported by 8 documents)"
    ), "{stdout}");
    let notes: Vec<&str> = stdout
        .lines()
        .filter(|l| l.contains("[W-DEPENDS-ON-INVALID]"))
        .collect();
    assert_eq!(notes.len(), 3, "{stdout}");
    assert!(notes.iter().any(|l| l.contains(
        "info [W-DEPENDS-ON-INVALID] `clock:` depends on `run.day`, which is invalid \
         (E-TYPE-NUMBER at world.schema.yaml:7); it is not checked until that declaration is fixed"
    )), "{stdout}");
    assert!(notes.iter().any(|l| l.contains(
        "`::check` depends on `check`, which is invalid (E-TYPE-NUMBER at \
         game.occasions/directives/check.yaml:9); it is not checked until that declaration \
         is fixed (1 more dependant(s))"
    )), "{stdout}");
    // The occasion's three answering beats (three documents) share one note.
    assert!(notes.iter().any(|l| l.contains("`on: townVisit` depends on `townVisit`")
        && l.ends_with("(2 more dependant(s))")), "{stdout}");
}
