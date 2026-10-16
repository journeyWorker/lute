//! 0.47.0 §4 (#119, #120): `bindingsVersion` follows the `luteVersion` rule —
//! `W-BINDINGS-VERSION-STALE` when it is not the toolchain line,
//! `E-BINDINGS-VERSION` at a section newer than the declared line, and
//! `lute fix` raises an older line — and `ui.title` gains `text` with
//! `background`/`music` optional. Expectations derive the toolchain line from
//! [`LUTE_LANG_VERSION`], so they hold on every release line.

use std::path::Path;
use std::process::Command;

use lute_check::LUTE_LANG_VERSION;
use serde_json::Value;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

/// The toolchain line: `MAJOR.MINOR` of the toolchain's language version.
fn line() -> String {
    let mut parts = LUTE_LANG_VERSION.split('.');
    format!("{}.{}", parts.next().unwrap(), parts.next().unwrap())
}

fn line_pair() -> (u64, u64) {
    let line = line();
    let (major, minor) = line.split_once('.').unwrap();
    (major.parse().unwrap(), minor.parse().unwrap())
}

fn write(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn project(tag: &str, bindings: &str) -> lute_scratch::ScratchDir {
    let dir = lute_scratch::ScratchDir::named(tag);
    write(
        &dir,
        "lute.project.yaml",
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\ndefaults:\n  uses: [world.schema.yaml]\n",
    );
    write(
        &dir,
        "world.schema.yaml",
        "enums:\n  ending: [restore, retire]\nstate:\n  run.ending: {type: {domain: ending}, default: restore}\n",
    );
    write(
        &dir,
        "scenes/start.lute",
        "---\nkind: scene\nid: start\non: start\n---\n## Start\n@narrator: Hello.\n",
    );
    write(&dir, "assets/title.png", "x");
    write(&dir, "assets/title.ogg", "x");
    write(&dir, "player.bindings.yaml", bindings);
    dir
}

fn check(tag: &str, bindings: &str) -> Vec<Value> {
    let dir = project(tag, bindings);
    let out = Command::new(BIN)
        .args(["check-project", &dir.to_string_lossy(), "--json"])
        .output()
        .unwrap();
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    value["project_diagnostics"].as_array().unwrap().clone()
}

fn with_code<'a>(diagnostics: &'a [Value], code: &str) -> Vec<&'a Value> {
    diagnostics.iter().filter(|d| d["code"] == code).collect()
}

fn message(d: &Value) -> &str {
    d["message"].as_str().unwrap()
}

const CANVAS: &str = "canvas: {width: 1, height: 1}\n";

#[test]
fn the_toolchain_line_is_clean() {
    let bindings = format!("bindingsVersion: '{}'\n{CANVAS}", line());
    let diagnostics = check("bindings-version-current-047", &bindings);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn stale_newer_and_malformed_versions_warn_at_the_value() {
    let line = line();
    let (major, minor) = line_pair();
    let older = format!("{major}.{}", minor - 1);
    let newer = format!("{major}.{}", minor + 1);
    let zero_major = format!("0{line}");
    let zero_minor = format!("{major}.0{minor}");
    for (tag, value, wording) in [
        (
            "older",
            older.as_str(),
            format!("update it to `bindingsVersion: \"{line}\"` (`lute fix` does it)"),
        ),
        (
            "newer",
            newer.as_str(),
            "upgrade the toolchain; do not downgrade the stamp".to_string(),
        ),
        (
            "triple",
            LUTE_LANG_VERSION,
            format!("is not `MAJOR.MINOR` — restamp it as `bindingsVersion: \"{line}\"`"),
        ),
        (
            "word",
            "latest",
            format!("is not `MAJOR.MINOR` — restamp it as `bindingsVersion: \"{line}\"`"),
        ),
        // Leading zeros parse equal to the line but are not its spelling, and
        // `lute fix` does not raise them: restamp advice, not "stale".
        (
            "zero-major",
            zero_major.as_str(),
            format!("is not `MAJOR.MINOR` — restamp it as `bindingsVersion: \"{line}\"`"),
        ),
        (
            "zero-minor",
            zero_minor.as_str(),
            format!("is not `MAJOR.MINOR` — restamp it as `bindingsVersion: \"{line}\"`"),
        ),
    ] {
        let bindings = format!("bindingsVersion: \"{value}\"\n{CANVAS}");
        let diagnostics = check(&format!("bindings-version-{tag}-047"), &bindings);
        let stale = with_code(&diagnostics, "W-BINDINGS-VERSION-STALE");
        assert_eq!(stale.len(), 1, "{tag}: {diagnostics:?}");
        let d = stale[0];
        assert_eq!(d["severity"], "warning", "{d}");
        assert!(message(d).contains(&wording), "{tag}: {d}");
        assert!(
            message(d).contains(&format!("`bindingsVersion: \"{value}\"`")),
            "{d}"
        );
        // The span is the value (quotes included), not the key.
        assert_eq!(d["span"]["line"], 1, "{d}");
        assert_eq!(d["span"]["column"], 18, "{d}");
        assert_eq!(
            d["span"]["byte_end"].as_u64().unwrap() - d["span"]["byte_start"].as_u64().unwrap(),
            value.len() as u64 + 2,
            "{d}"
        );
        assert!(
            with_code(&diagnostics, "E-BINDINGS-VERSION").is_empty(),
            "{diagnostics:?}"
        );
    }
}

/// Each row of the section table, declared one line below what it needs:
/// an error at the section's key naming the needed line and the declared
/// value; declared at its line, no error.
#[test]
fn sections_newer_than_the_declared_line_are_errors() {
    for (tag, section, needs, body, key) in [
        (
            "vfx",
            "vfx.lanterns` with `effect: custom",
            (0, 41),
            "vfx:\n  lanterns: {effect: custom}\n",
            "lanterns",
        ),
        (
            "volumes",
            "volumes",
            (0, 42),
            "volumes:\n  quiet: 0.3\n  loud: 1\n",
            "volumes",
        ),
        (
            "ended",
            "ui.ended",
            (0, 46),
            "ui:\n  ended:\n    by: run.ending\n    screens:\n      restore: {title: Again, text: Welcome}\n",
            "ended",
        ),
        (
            "title-text",
            "ui.title.text",
            (0, 47),
            "ui:\n  title: {text: Brinewake}\n",
            "text",
        ),
    ] {
        let (major, minor): (u64, u64) = needs;
        let below = format!("{major}.{}", minor - 1);
        let bindings = format!("bindingsVersion: '{below}'\n{CANVAS}{body}");
        let diagnostics = check(&format!("bindings-section-{tag}-047"), &bindings);
        let errors = with_code(&diagnostics, "E-BINDINGS-VERSION");
        assert_eq!(errors.len(), 1, "{tag}: {diagnostics:?}");
        let d = errors[0];
        assert_eq!(
            message(d),
            format!(
                "`{section}` needs bindingsVersion \"{major}.{minor}\" or newer; this file \
                 declares \"{below}\""
            ),
            "{d}"
        );
        let start = d["span"]["byte_start"].as_u64().unwrap() as usize;
        let end = d["span"]["byte_end"].as_u64().unwrap() as usize;
        assert_eq!(&bindings[start..end], key, "{tag}: {d}");
        assert!(
            with_code(&diagnostics, "E-BINDING-SHAPE").is_empty(),
            "{tag}: {diagnostics:?}"
        );

        let at = format!("bindingsVersion: '{major}.{minor}'\n{CANVAS}{body}");
        let diagnostics = check(&format!("bindings-section-{tag}-at-047"), &at);
        assert!(
            with_code(&diagnostics, "E-BINDINGS-VERSION").is_empty(),
            "{tag}: {diagnostics:?}"
        );
    }
}

#[test]
fn sections_are_not_checked_against_a_malformed_version() {
    let bindings = format!("bindingsVersion: 'old'\n{CANVAS}ui:\n  title: {{text: Brinewake}}\n");
    let diagnostics = check("bindings-section-malformed-047", &bindings);
    assert!(
        with_code(&diagnostics, "E-BINDINGS-VERSION").is_empty(),
        "{diagnostics:?}"
    );
    assert_eq!(
        with_code(&diagnostics, "W-BINDINGS-VERSION-STALE").len(),
        1,
        "{diagnostics:?}"
    );
}

#[test]
fn an_unknown_key_stays_a_shape_error() {
    let bindings =
        format!("bindingsVersion: '0.40'\n{CANVAS}ui:\n  title: {{text: Brinewake, heading: x}}\n");
    let diagnostics = check("bindings-unknown-key-047", &bindings);
    assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
    assert_eq!(
        with_code(&diagnostics, "E-BINDING-SHAPE").len(),
        1,
        "{diagnostics:?}"
    );
    assert_eq!(
        with_code(&diagnostics, "W-BINDINGS-VERSION-STALE").len(),
        1,
        "{diagnostics:?}"
    );
}

/// A file newer than the toolchain that it cannot decode (an unknown
/// section, an unknown effect) still says to upgrade the toolchain.
#[test]
fn an_undecodable_newer_file_still_says_upgrade_the_toolchain() {
    let (major, minor) = line_pair();
    let newer = format!("{major}.{}", minor + 1);
    for (tag, body, code) in [
        ("shape", "futureSection: {x: 1}\n", "E-BINDING-SHAPE"),
        (
            "effect",
            "actions: {x: {effect: teleport}}\n",
            "E-BINDING-EFFECT",
        ),
    ] {
        let bindings = format!("bindingsVersion: '{newer}'\n{CANVAS}{body}");
        let diagnostics = check(&format!("bindings-undecodable-{tag}-047"), &bindings);
        assert_eq!(diagnostics.len(), 2, "{tag}: {diagnostics:?}");
        assert_eq!(with_code(&diagnostics, code).len(), 1, "{diagnostics:?}");
        let stale = with_code(&diagnostics, "W-BINDINGS-VERSION-STALE");
        assert_eq!(stale.len(), 1, "{tag}: {diagnostics:?}");
        assert!(
            message(stale[0]).contains("upgrade the toolchain; do not downgrade the stamp"),
            "{tag}: {}",
            stale[0]
        );
        let start = stale[0]["span"]["byte_start"].as_u64().unwrap() as usize;
        let end = stale[0]["span"]["byte_end"].as_u64().unwrap() as usize;
        assert_eq!(&bindings[start..end], format!("'{newer}'"), "{tag}");
    }
}

/// `lute fix` raises only a block-style top-level `bindingsVersion`; an older
/// value in a flow-style document is stale, but the warning does not promise
/// a fix that does not happen.
#[test]
fn a_flow_style_older_version_is_stale_without_naming_lute_fix() {
    let before = "{bindingsVersion: '0.40', canvas: {width: 1, height: 1}}\n";
    let diagnostics = check("bindings-flow-older-047", before);
    let stale = with_code(&diagnostics, "W-BINDINGS-VERSION-STALE");
    assert_eq!(stale.len(), 1, "{diagnostics:?}");
    let text = message(stale[0]);
    assert!(
        text.contains(&format!("update it to `bindingsVersion: \"{}\"`", line())),
        "{text}"
    );
    assert!(!text.contains("lute fix"), "{text}");

    let dir = project("fix-bindings-flow-047", before);
    let out = run(&["fix"], &dir);
    assert!(out.contains("lute: applied 0 fix(es)"), "{out}");
    assert_eq!(
        std::fs::read_to_string(dir.join("player.bindings.yaml")).unwrap(),
        before
    );
}

/// Diagnostics other than the "newer" stamp warning a pre-0.47 toolchain
/// gives a file that declares "0.47".
fn without_line_warning(diagnostics: Vec<Value>) -> Vec<Value> {
    let stale = with_code(&diagnostics, "W-BINDINGS-VERSION-STALE").len();
    assert_eq!(stale, usize::from(line() != "0.47"), "{diagnostics:?}");
    diagnostics
        .into_iter()
        .filter(|d| d["code"] != "W-BINDINGS-VERSION-STALE")
        .collect()
}

#[test]
fn title_text_alone_and_single_assets_are_accepted() {
    for (tag, title) in [
        ("text", "{text: Brinewake}"),
        ("background", "{background: title.png}"),
        ("music", "{music: title.ogg}"),
        (
            "all",
            "{text: Brinewake, background: title.png, music: title.ogg}",
        ),
    ] {
        let bindings = format!("bindingsVersion: '0.47'\n{CANVAS}ui:\n  title: {title}\n");
        let diagnostics =
            without_line_warning(check(&format!("bindings-title-{tag}-047"), &bindings));
        assert!(diagnostics.is_empty(), "{tag}: {diagnostics:?}");
    }
}

#[test]
fn a_present_title_asset_keeps_its_file_check() {
    let bindings =
        format!("bindingsVersion: '0.47'\n{CANVAS}ui:\n  title: {{background: missing.png}}\n");
    let diagnostics = without_line_warning(check("bindings-title-missing-047", &bindings));
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0]["code"], "E-BINDING-FILE", "{diagnostics:?}");
}

#[test]
fn an_empty_title_or_empty_text_is_a_shape_error() {
    for (tag, title, wording, at) in [
        ("empty", "{}", "`ui.title` binds nothing", "title"),
        (
            "empty-text",
            "{text: '', music: title.ogg}",
            "`ui.title.text` is empty",
            "text",
        ),
    ] {
        let bindings = format!("bindingsVersion: '0.47'\n{CANVAS}ui:\n  title: {title}\n");
        let diagnostics =
            without_line_warning(check(&format!("bindings-title-{tag}-047"), &bindings));
        assert_eq!(diagnostics.len(), 1, "{tag}: {diagnostics:?}");
        let d = &diagnostics[0];
        assert_eq!(d["code"], "E-BINDING-SHAPE", "{d}");
        assert!(message(d).contains(wording), "{d}");
        let start = d["span"]["byte_start"].as_u64().unwrap() as usize;
        let end = d["span"]["byte_end"].as_u64().unwrap() as usize;
        assert_eq!(&bindings[start..end], at, "{tag}: {d}");
    }
}

fn run(args: &[&str], path: &Path) -> String {
    let out = Command::new(BIN).args(args).arg(path).output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// `lute fix` raises an older `MAJOR.MINOR` value to the toolchain line,
/// splicing only the scalar (quote style, comments and layout survive); a
/// rerun changes nothing and `check-project` no longer warns.
#[test]
fn fix_raises_an_older_bindings_version_in_place() {
    let line = line();
    let template =
        "# bindings\nbindingsVersion: 'OLD'   # stamp\ncanvas: {width: 1, height: 1}\n# trailing\n";
    for (tag, old) in [
        ("single", "'0.40'"),
        ("double", "\"0.40\""),
        ("plain", "0.40"),
    ] {
        let before = template.replace("'OLD'", old);
        let dir = project(&format!("fix-bindings-version-{tag}-047"), &before);
        let out = run(&["fix"], &dir);
        assert!(out.contains("lute: applied 1 fix(es) in 1 of"), "{out}");
        let quote = old.strip_suffix("0.40").map_or(&old[..1], |_| "");
        assert_eq!(
            std::fs::read_to_string(dir.join("player.bindings.yaml")).unwrap(),
            template.replace("'OLD'", &format!("{quote}{line}{quote}")),
            "{tag}"
        );
        let again = run(&["fix"], &dir);
        assert!(again.contains("lute: applied 0 fix(es)"), "{again}");
        let checked = run(&["check-project"], &dir);
        assert!(!checked.contains("W-BINDINGS-VERSION-STALE"), "{checked}");
    }
}

#[test]
fn fix_leaves_current_newer_and_malformed_versions_alone() {
    let (major, minor) = line_pair();
    for (tag, value) in [
        ("current", line()),
        ("newer", format!("{major}.{}", minor + 1)),
        ("triple", "0.40.0".to_string()),
        ("word", "latest".to_string()),
        ("zero-major", format!("0{}", line())),
        ("zero-minor", format!("{major}.0{minor}")),
    ] {
        let before = format!("bindingsVersion: '{value}'\n{CANVAS}");
        let dir = project(&format!("fix-bindings-keep-{tag}-047"), &before);
        let out = run(&["fix"], &dir);
        assert!(out.contains("lute: applied 0 fix(es)"), "{tag}: {out}");
        assert_eq!(
            std::fs::read_to_string(dir.join("player.bindings.yaml")).unwrap(),
            before,
            "{tag}"
        );
    }
}
