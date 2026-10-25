//! 0.50.0 §3 (#163): plugin stamps scoped to named surfaces with `on:`.
//!
//! The fixture `plugin-stamps-050` declares `voiceAssetId` required on
//! content lines only, `assetName` typed `string` on `::bg` and `int` on
//! `::sfx`, and a cross-cutting `cue`. On 0.49.1 the `on:` key itself is
//! `E-PLUGIN-KEY` and the plugin fails to load, so every case below fails
//! there.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value as Json;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn lute(args: &[&str]) -> Output {
    Command::new(BIN).args(args).output().unwrap()
}

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// The `error [CODE] message` part of every diagnostic line of `out`.
fn diagnostics(out: &Output) -> Vec<String> {
    text(out)
        .lines()
        .filter_map(|l| l.split_once(": error ").map(|(_, d)| d.to_string()))
        .collect()
}

#[test]
fn a_line_scoped_stamp_is_refused_elsewhere_and_required_on_lines() {
    let project = fixture("plugin-stamps-050");
    let bad = project.join("scenes/bad.lute");
    let out = lute(&[
        "check",
        "--project",
        project.to_str().unwrap(),
        bad.to_str().unwrap(),
    ]);
    assert!(!out.status.success(), "{}", text(&out));
    assert_eq!(
        diagnostics(&out),
        [
            "[E-UNKNOWN-ATTR] `::bg` has no attribute `voiceAssetId`: it is a stamp of \
             `acme.stage` for content lines only",
            "[E-UNKNOWN-ATTR] `::sfx` has no attribute `voiceAssetId`: it is a stamp of \
             `acme.stage` for content lines only",
            "[E-MISSING-ATTR] this content line requires attribute `voiceAssetId` (a stamp of \
             `acme.stage` for content lines)",
            "[E-UNKNOWN-ATTR] `::end` has no attribute `voiceAssetId`: it is a stamp of \
             `acme.stage` for content lines only",
        ],
        "{}",
        text(&out)
    );
}

/// The first record of `kind` anywhere in `ir`.
fn record<'a>(ir: &'a Json, kind: &str) -> Option<&'a Json> {
    match ir {
        Json::Object(map) if map.get("kind").is_some_and(|k| k == kind) => Some(ir),
        Json::Object(map) => map.values().find_map(|v| record(v, kind)),
        Json::Array(items) => items.iter().find_map(|v| record(v, kind)),
        _ => None,
    }
}

#[test]
fn a_stamp_serializes_with_the_type_of_its_surface() {
    let project = fixture("plugin-stamps-050");
    let ok = project.join("scenes/ok.lute");
    let out = lute(&[
        "compile",
        "--project",
        project.to_str().unwrap(),
        ok.to_str().unwrap(),
    ]);
    assert!(out.status.success(), "{}", text(&out));
    let ir: Json = serde_json::from_slice(&out.stdout).unwrap();
    let stamps = |kind: &str| -> Json {
        record(&ir, kind).map_or_else(
            || panic!("no `{kind}` record in {ir:#}"),
            |c| c["stamps"].clone(),
        )
    };
    assert_eq!(
        stamps("bg"),
        serde_json::json!({ "assetName": "room", "cue": "open" })
    );
    assert_eq!(stamps("sfx"), serde_json::json!({ "assetName": 7 }));
    assert_eq!(
        stamps("line"),
        serde_json::json!({ "cue": "soft", "voiceAssetId": "VO.mira.0010" })
    );
}

#[test]
fn a_required_stamp_without_on_fails_to_load() {
    let scratch = lute_scratch::ScratchDir::named("plugin-stamps-050-required");
    let root = scratch.join("p");
    let plugin = root.join("plugins/acme.stage");
    std::fs::create_dir_all(plugin.join("stampattrs")).unwrap();
    std::fs::create_dir_all(root.join("scenes")).unwrap();
    std::fs::write(
        root.join("lute.project.yaml"),
        "pluginsDir: plugins/\ndefaultProfile: stage\nprofiles:\n  stage:\n    plugins: { \
         acme.stage: true }\n",
    )
    .unwrap();
    std::fs::write(
        plugin.join("plugin.yaml"),
        "id: acme.stage\nversion: 0.1.0\nkind: capability\ndepends: [{ id: lute.core, range: \
         \"^0.0.1\" }]\nexports:\n  stampAttrs: stampattrs/\n",
    )
    .unwrap();
    std::fs::write(
        plugin.join("stampattrs/s.yaml"),
        "stampAttrs:\n  - { name: voiceAssetId, type: string, required: true }\n",
    )
    .unwrap();
    std::fs::write(
        root.join("scenes/a.lute"),
        "---\nkind: scene\ncharacter: mira\nseason: 1\nepisode: 1\n---\n## Shot 1.\n\n@mira: \
         Hi.\n",
    )
    .unwrap();
    let out = lute(&["check", "--project", root.to_str().unwrap()]);
    assert!(!out.status.success(), "{}", text(&out));
    assert!(
        text(&out).contains(
            "error [E-PLUGIN-PARSE] stamp attribute `voiceAssetId` is `required: true` but has \
             no `on:`; a stamp required on every record is not a column — name its surfaces \
             with `on: […]`"
        ),
        "{}",
        text(&out)
    );
}

/// 0.50.0 §3.3, §14: a manifest without `on:` keeps its 0.49
/// `capabilitySnapshot`: `lute` 0.49.1 and 0.50.0 both compile the
/// `plugin-stamps-049` fixture to `aa317768…`. dsl 0.51.0 §3.1 moved the
/// core once (the core `::jump` declares `max`, §13 class 4), which moves
/// every project's hash, this one included, to the value below.
#[test]
fn a_manifest_without_on_keeps_its_0_49_capability_snapshot() {
    let project = fixture("plugin-stamps-049");
    let fight = project.join("fight.lute");
    let out = lute(&[
        "compile",
        "--project",
        project.to_str().unwrap(),
        fight.to_str().unwrap(),
    ]);
    assert!(out.status.success(), "{}", text(&out));
    let ir: Json = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        ir["capabilitySnapshot"],
        "032912f4abf26f42b719e8ad1a58e73ca30695e60f92407eb9e2994b6e69a382"
    );
}

/// 0.50.0 §3.7: `lute context` lists each scoped declaration with its
/// surfaces; a cross-cutting one keeps its 0.49 text.
#[test]
fn context_lists_the_surfaces_of_each_stamp() {
    let project = fixture("plugin-stamps-050");
    let ok = project.join("scenes/ok.lute");
    let out = lute(&[
        "context",
        "--project",
        project.to_str().unwrap(),
        ok.to_str().unwrap(),
    ]);
    assert!(out.status.success(), "{}", text(&out));
    let shown = String::from_utf8_lossy(&out.stdout);
    for row in [
        "  assetName: string — ::bg (stampAttrs)\n",
        "  assetName: int — ::sfx (stampAttrs)\n",
        "  cue: string — every directive but ::jump, ::label, ::clear (stampAttrs)\n",
        "  voiceAssetId: string — content lines (stampAttrs)\n",
    ] {
        assert!(shown.contains(row), "{row:?} in {shown}");
    }
}
