//! 0.41.1 §3.1 (#74): `lute fix` scans YAML as bytes, so a non-ASCII scalar,
//! comment or quoted string never panics the domain rename; the rewrite keeps
//! every other byte and a second run changes nothing.

use std::path::Path;
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn fix(path: &Path) -> Output {
    let out = Command::new(BIN).arg("fix").arg(path).output().unwrap();
    assert!(
        out.status.success(),
        "exit {:?}: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// The issue's reproduction: one unquoted Korean scalar.
#[test]
fn an_unquoted_korean_scalar_is_ordinary_input() {
    let dir = lute_scratch::ScratchDir::named("fix-unicode-041-min");
    let file = dir.join("x.yaml");
    std::fs::write(&file, "a: 아침\n").unwrap();
    let out = fix(&file);
    assert_eq!(stdout(&out), "lute: nothing to fix\n");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "a: 아침\n");
}

/// Korean and em-dash text in plain scalars, quoted strings and comments
/// around one old `domain:` reference and one old `enums:` key: both are
/// renamed in place, a `domain:` inside a quoted string is not, and every
/// other byte survives; the second run is a no-op.
#[test]
fn unicode_yaml_renames_domains_in_place_idempotently() {
    let source = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/fix-unicode-041/world.schema.yaml"),
    )
    .unwrap();
    let dir = lute_scratch::ScratchDir::named("fix-unicode-041");
    let file = dir.join("world.schema.yaml");
    std::fs::write(&file, &source).unwrap();

    let out = fix(&dir);
    assert!(
        stdout(&out).contains("lute: applied 2 fix(es) in 1 of 1 file(s)"),
        "{}",
        stdout(&out)
    );
    let expected = source
        .replace(
            "  musicAction: [start, stop]",
            "  musicPlayback: [start, stop]",
        )
        .replace("{ domain: musicAction }", "{ domain: musicPlayback }");
    assert_ne!(expected, source);
    assert!(expected.contains("'domain: musicAction는 그대로'"));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), expected);

    let again = fix(&dir);
    assert!(
        stdout(&again).contains("lute: applied 0 fix(es) in 0 of 1 file(s)"),
        "{}",
        stdout(&again)
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), expected);
}

/// A `domain:` written right after a multibyte character, on the same line
/// as a real reference: the byte scan reads past both without panicking and
/// renames only the real reference.
#[test]
fn a_multibyte_character_touching_domain_is_scanned_by_bytes() {
    let dir = lute_scratch::ScratchDir::named("fix-unicode-041-touch");
    let file = dir.join("v.yaml");
    let text =
        "state:\n  run.a: { type: { domain: musicAction }, default: 아domain: x }\n  run.b: 'é'\n";
    std::fs::write(&file, text).unwrap();
    let out = fix(&file);
    assert!(
        stdout(&out).contains("applied 1 fix(es)"),
        "{}",
        stdout(&out)
    );
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        text.replace("{ domain: musicAction }", "{ domain: musicPlayback }")
    );
}
