//! 0.41.1 §3.4 (#67): an `enumFromOption` attr is checked against the
//! option's effective value — the selected profile's, otherwise the plugin
//! manifest's `options[].default` — for an authored value and for an omitted
//! attr's declared `default:` alike. An option with neither keeps the skip.

use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/enum-option-default-041")
}

/// `lute check <doc>` from the fixture root: (exit code, stdout).
fn check(doc: &str, json: bool) -> (i32, String) {
    let mut cmd = Command::new(BIN);
    cmd.current_dir(fixture()).arg("check");
    if json {
        cmd.arg("--json");
    }
    let out = cmd.arg(doc).output().unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

/// The issue: the profile only enables the plugin, so the manifest default
/// `[easy, hard]` is the domain — `bogus` is refused, `hrad` gets the
/// did-you-mean.
#[test]
fn manifest_default_supplies_the_domain() {
    let (code, text) = check("invalid.lute", false);
    assert_eq!(code, 1, "{text}");
    assert_eq!(
        text,
        "invalid.lute:6:16: error [E-BAD-ENUM] `bogus` is not a valid value for `value` of \
         `::level` (expected one of: easy, hard)\n\
         invalid.lute:7:16: error [E-BAD-ENUM] `hrad` is not a valid value for `value` of \
         `::level` (expected one of: easy, hard) — did you mean `hard`?\n\
         failed: invalid.lute (2 error(s), 0 warning(s))\n"
    );
}

/// A member of the default domain is clean, and an option with no profile
/// value and no declared default (`tiers`) is still not checked.
#[test]
fn valid_member_and_undeclared_default_are_clean() {
    let (code, text) = check("valid.lute", false);
    assert_eq!(code, 0, "{text}");
    assert_eq!(text, "ok: valid.lute (0 warning(s))\n");
}

/// An omitted attr's declared `default:` is judged against the manifest
/// default domain too.
#[test]
fn omitted_declared_default_is_checked_against_the_manifest_default() {
    let (code, text) = check("defaulted.lute", false);
    assert_eq!(code, 1, "{text}");
    assert!(
        text.contains(
            "defaulted.lute:6:1: error [E-BAD-ENUM] `medium` is not a valid value for `start` of \
             `::rank` (expected one of: easy, hard) — this is the declared `default:` of `start`, \
             used because the call omits it\n"
        ),
        "{text}"
    );
}

/// The profile's value wins over the manifest default: under `hard`, `easy`
/// is refused — authored, and as `mode`'s declared default.
#[test]
fn profile_value_overrides_the_manifest_default() {
    let (code, text) = check("override.lute", false);
    assert_eq!(code, 1, "{text}");
    assert_eq!(
        text,
        "override.lute:7:1: error [E-BAD-ENUM] `easy` is not a valid value for `mode` of \
         `::level` (expected one of: hard) — this is the declared `default:` of `mode`, used \
         because the call omits it\n\
         override.lute:8:16: error [E-BAD-ENUM] `easy` is not a valid value for `value` of \
         `::level` (expected one of: hard)\n\
         failed: override.lute (2 error(s), 0 warning(s))\n"
    );
}

/// The diagnostic is the existing `E-BAD-ENUM` error, unchanged in shape.
#[test]
fn json_reports_the_existing_error() {
    let (code, text) = check("invalid.lute", true);
    assert_eq!(code, 1, "{text}");
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let diags = v["files"][0]["diagnostics"]
        .as_array()
        .expect("diagnostics array");
    assert_eq!(diags.len(), 2, "{text}");
    for d in diags {
        assert_eq!(d["code"], "E-BAD-ENUM", "{d}");
        assert_eq!(d["severity"], "error", "{d}");
    }
}
