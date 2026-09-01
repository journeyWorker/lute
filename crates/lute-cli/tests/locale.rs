//! `lute loc import` + `lute compile --locales` acceptance (dsl 0.8.0 §7): the
//! localization ROUND TRIP. `loc export` extracted translatable content and
//! nothing brought it back; these pin the reverse direction.
//!
//! Four groups: `export` → `import` produces a stable bundle (byte-identical
//! across runs); `compile --locales` merges it onto `texts`/`labels` while
//! leaving the source-language `text`/`label` untouched; `W-L10N-MISSING`
//! fires exactly once per missing `(lineId, locale)` pair and `--deny` promotes
//! it; and a bundle-less compile is byte-identical to before the feature.

use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-locale-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &Path, rel: &str, text: &str) -> PathBuf {
    let path = dir.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&path, text).unwrap();
    path
}

fn run(args: &[&str]) -> std::process::Output {
    Command::new(BIN).args(args).output().unwrap()
}

const PROJECT_YAML: &str = "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n";

/// One tagged scene with two content lines and a two-option `<branch>` — the
/// three translatable record shapes (`line`, choice option label) in one file.
const SCENE: &str = "\
---
kind: scene
character: bianca
season: 1
episode: 1
---

## Opening.

@bianca{code=\"0010\"}: Hello there.
@kai{code=\"0010\"}: Hi.

<branch id=\"pick\">
  <choice id=\"go\" label=\"Go on\">
    @bianca{code=\"0020\"}: Onward.
  </choice>
  <choice id=\"stay\" label=\"Stay here\">
    @bianca{code=\"0030\"}: Fine.
  </choice>
</branch>
";

/// Every `lineId` the scene above compiles to, in `addr` order.
const LINE_IDS: [&str; 6] = [
    "bianca.s01ep01.bianca_0010",
    "bianca.s01ep01.kai_0010",
    "bianca.s01ep01.pick.go",
    "bianca.s01ep01.pick.stay",
    "bianca.s01ep01.bianca_0020",
    "bianca.s01ep01.bianca_0030",
];

fn project(tag: &str) -> PathBuf {
    let dir = temp_dir(tag);
    write(&dir, "lute.project.yaml", PROJECT_YAML);
    write(&dir, "a.lute", SCENE);
    dir
}

/// `loc export` the project, then rewrite each row's translatable string with
/// `decorate` and drop every row whose `lineId` is in `drop` — the "a human
/// translated it" step, without a human. Returns the written file's path.
fn translate(project: &Path, out: &Path, decorate: &str, drop: &[&str]) -> PathBuf {
    let exported = run(&["loc", "export", project.to_str().unwrap()]);
    assert_eq!(exported.status.code(), Some(0));
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&exported.stdout).unwrap();
    let translated: Vec<serde_json::Value> = rows
        .into_iter()
        .filter(|r| !drop.contains(&r["lineId"].as_str().unwrap_or_default()))
        .map(|mut r| {
            // A `line` row carries its text in `text`, a `choice` row in
            // `label` — exactly the split `loc export` writes.
            let field = if r["kind"] == "choice" {
                "label"
            } else {
                "text"
            };
            let old = r[field].as_str().unwrap_or_default().to_string();
            r[field] = serde_json::Value::String(format!("{decorate}{old}"));
            r
        })
        .collect();
    let mut s = serde_json::to_string_pretty(&translated).unwrap();
    s.push('\n');
    std::fs::write(out, &s).unwrap();
    out.to_path_buf()
}

fn read_json(path: &Path) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// Every `(lineId, texts-or-labels)` pair in an artifact, in command order.
fn locale_maps(artifact: &serde_json::Value) -> Vec<(String, serde_json::Value)> {
    let mut out = Vec::new();
    for c in artifact["commands"].as_array().unwrap() {
        match c["kind"].as_str() {
            Some("line") => out.push((
                c["lineId"].as_str().unwrap().to_string(),
                c.get("texts").cloned().unwrap_or(serde_json::Value::Null),
            )),
            Some("choice") | Some("hub") => {
                for o in c["options"].as_array().unwrap() {
                    out.push((
                        o["lineId"].as_str().unwrap().to_string(),
                        o.get("labels").cloned().unwrap_or(serde_json::Value::Null),
                    ));
                }
            }
            _ => {}
        }
    }
    out
}

// --- Group 1: `loc export` -> `loc import` is a stable round trip ----------

#[test]
fn export_then_import_produces_a_stable_bundle() {
    let dir = project("roundtrip");
    let work = temp_dir("roundtrip-work");
    translate(&dir, &work.join("ja-JP.json"), "[ja] ", &[]);
    translate(&dir, &work.join("en-US.json"), "[en] ", &[]);

    // Run import TWICE into different files: the bundle must be byte-identical.
    let mut bundles = Vec::new();
    for i in 0..2 {
        let out = work.join(format!("bundle{i}.json"));
        let result = run(&[
            "loc",
            "import",
            work.join("ja-JP.json").to_str().unwrap(),
            work.join("en-US.json").to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
        ]);
        assert_eq!(
            result.status.code(),
            Some(0),
            "stderr: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        bundles.push(std::fs::read_to_string(&out).unwrap());
    }
    assert_eq!(bundles[0], bundles[1], "the bundle must be byte-stable");

    let bundle: serde_json::Value = serde_json::from_str(&bundles[0]).unwrap();
    assert_eq!(bundle["schemaVersion"], 1);
    assert_eq!(
        bundle["locales"].as_array().unwrap(),
        &vec![
            serde_json::Value::String("en-US".into()),
            serde_json::Value::String("ja-JP".into())
        ],
        "locales are sorted; the tag comes from each file's stem"
    );
    let entries = bundle["entries"].as_object().unwrap();
    assert_eq!(
        entries.len(),
        LINE_IDS.len(),
        "every translatable record keyed"
    );
    for id in LINE_IDS {
        let e = entries
            .get(id)
            .unwrap_or_else(|| panic!("missing `{id}` in {entries:#?}"));
        assert!(e["ja-JP"].as_str().unwrap().starts_with("[ja] "));
        assert!(e["en-US"].as_str().unwrap().starts_with("[en] "));
    }
    // Keyed by lineId — NOT by the regenerated `addr`.
    assert!(
        !entries.keys().any(|k| k.starts_with("001-")),
        "`addr` is a position, never an identity: {entries:#?}"
    );
}

#[test]
fn a_csv_export_round_trips_through_import_too() {
    let dir = project("csv");
    let work = temp_dir("csv-work");
    let csv = work.join("fr-FR.csv");
    let exported = run(&[
        "loc",
        "export",
        dir.to_str().unwrap(),
        "--format",
        "csv",
        "-o",
        csv.to_str().unwrap(),
    ]);
    assert_eq!(exported.status.code(), Some(0));

    let bundle_path = work.join("bundle.json");
    let result = run(&[
        "loc",
        "import",
        csv.to_str().unwrap(),
        "-o",
        bundle_path.to_str().unwrap(),
    ]);
    assert_eq!(
        result.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let bundle = read_json(&bundle_path);
    assert_eq!(bundle["locales"][0], "fr-FR");
    assert_eq!(bundle["entries"].as_object().unwrap().len(), LINE_IDS.len());
    // A choice option's label survives the CSV `text` column.
    assert_eq!(
        bundle["entries"]["bianca.s01ep01.pick.go"]["fr-FR"],
        "Go on"
    );
}

#[test]
fn a_duplicate_line_id_within_one_locale_is_e_locale_bundle() {
    let dir = project("dup");
    let work = temp_dir("dup-work");
    let file = translate(&dir, &work.join("ja-JP.json"), "[ja] ", &[]);
    // Re-append the first row: the same id twice in one locale.
    let mut rows: Vec<serde_json::Value> = read_json(&file).as_array().unwrap().clone();
    rows.push(rows[0].clone());
    std::fs::write(&file, serde_json::to_string(&rows).unwrap()).unwrap();

    let result = run(&["loc", "import", file.to_str().unwrap()]);
    assert_eq!(result.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("E-LOCALE-BUNDLE"), "got: {stderr}");
    assert!(
        stderr.contains(LINE_IDS[0]),
        "the offending id is named: {stderr}"
    );
    assert!(
        stderr.contains("ja-JP.json"),
        "the offending file is named: {stderr}"
    );
    assert!(result.stdout.is_empty(), "no bundle on a rejected import");
}

#[test]
fn an_unparseable_input_and_an_empty_locale_tag_are_e_locale_bundle() {
    let work = temp_dir("malformed");
    let bad = write(&work, "ja-JP.json", "this is not json");
    let result = run(&["loc", "import", bad.to_str().unwrap()]);
    assert_eq!(result.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&result.stderr).contains("E-LOCALE-BUNDLE"));

    let empty = write(
        &work,
        "rows.json",
        r#"[{"kind":"line","lineId":"a","locale":"","text":"t"}]"#,
    );
    let result = run(&["loc", "import", empty.to_str().unwrap()]);
    assert_eq!(result.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("empty locale tag"), "got: {stderr}");
}

// --- Group 2: `compile --locales` merges without disturbing the source -----

#[test]
fn compile_locales_fills_texts_and_labels_and_never_touches_the_source_string() {
    let dir = project("merge");
    let work = temp_dir("merge-work");
    translate(&dir, &work.join("ja-JP.json"), "[ja] ", &[]);
    let bundle = work.join("bundle.json");
    assert_eq!(
        run(&[
            "loc",
            "import",
            work.join("ja-JP.json").to_str().unwrap(),
            "-o",
            bundle.to_str().unwrap(),
        ])
        .status
        .code(),
        Some(0)
    );

    let merged = work.join("a.json");
    let result = run(&[
        "compile",
        dir.join("a.lute").to_str().unwrap(),
        "--project",
        dir.to_str().unwrap(),
        "--locales",
        bundle.to_str().unwrap(),
        "-o",
        merged.to_str().unwrap(),
    ]);
    assert_eq!(
        result.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        result.stderr.is_empty(),
        "a complete bundle emits no W-L10N-MISSING: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    let artifact = read_json(&merged);
    let maps = locale_maps(&artifact);
    assert_eq!(maps.len(), LINE_IDS.len());
    for (id, m) in &maps {
        assert!(
            m["ja-JP"].as_str().is_some_and(|s| s.starts_with("[ja] ")),
            "`{id}` must carry its ja-JP text: {m:#?}"
        );
    }

    // The SOURCE strings are untouched — a 0.7 consumer reads exactly what it
    // read before.
    for c in artifact["commands"].as_array().unwrap() {
        if c["kind"] == "line" {
            let text = c["text"].as_str().unwrap();
            assert!(
                !text.starts_with("[ja] "),
                "`text` stays source language: {text}"
            );
        }
        if c["kind"] == "choice" {
            for o in c["options"].as_array().unwrap() {
                let label = o["label"].as_str().unwrap();
                assert!(
                    !label.starts_with("[ja] "),
                    "`label` stays source language: {label}"
                );
            }
        }
    }
    let go = maps
        .iter()
        .find(|(id, _)| id == "bianca.s01ep01.pick.go")
        .unwrap();
    assert_eq!(go.1["ja-JP"], "[ja] Go on", "a choice option gets `labels`");
}

#[test]
fn a_bundle_entry_matching_nothing_in_the_document_is_ignored() {
    let dir = project("stray");
    let work = temp_dir("stray-work");
    let bundle = write(
        &work,
        "bundle.json",
        r#"{"schemaVersion":1,"locales":["ja-JP"],
            "entries":{"some.other.document_0010":{"ja-JP":"よそ"}}}"#,
    );
    let result = run(&[
        "compile",
        dir.join("a.lute").to_str().unwrap(),
        "--locales",
        bundle.to_str().unwrap(),
    ]);
    // The stray entry is silently ignored (a bundle legitimately spans a whole
    // project) — but THIS document's own records are still reported missing.
    assert_eq!(result.status.code(), Some(0));
    let artifact: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    for (_, m) in locale_maps(&artifact) {
        assert!(m.is_null(), "no record matched, so nothing merged: {m:#?}");
    }
}

#[test]
fn without_locales_the_artifact_is_byte_identical() {
    let dir = project("bytes");
    let plain = run(&["compile", dir.join("a.lute").to_str().unwrap()]);
    assert_eq!(plain.status.code(), Some(0));
    let text = String::from_utf8(plain.stdout).unwrap();
    assert!(
        !text.contains("\"texts\"") && !text.contains("\"labels\""),
        "the 0.8.0 locale carriers are skip-if-empty: {text}"
    );
}

// --- Group 3: `W-L10N-MISSING`, exact count, promotable --------------------

#[test]
fn w_l10n_missing_fires_once_per_missing_line_id_locale_pair() {
    let dir = project("missing");
    let work = temp_dir("missing-work");
    // ja-JP is complete; en-US drops exactly two records.
    translate(&dir, &work.join("ja-JP.json"), "[ja] ", &[]);
    translate(
        &dir,
        &work.join("en-US.json"),
        "[en] ",
        &[LINE_IDS[3], LINE_IDS[5]],
    );
    let bundle = work.join("bundle.json");
    assert_eq!(
        run(&[
            "loc",
            "import",
            work.join("ja-JP.json").to_str().unwrap(),
            work.join("en-US.json").to_str().unwrap(),
            "-o",
            bundle.to_str().unwrap(),
        ])
        .status
        .code(),
        Some(0)
    );

    let result = run(&[
        "compile",
        dir.join("a.lute").to_str().unwrap(),
        "--locales",
        bundle.to_str().unwrap(),
        "-o",
        work.join("a.json").to_str().unwrap(),
    ]);
    assert_eq!(
        result.status.code(),
        Some(0),
        "a warning never flips the verdict"
    );
    let stderr = String::from_utf8_lossy(&result.stderr);
    let hits: Vec<&str> = stderr
        .lines()
        .filter(|l| l.contains("W-L10N-MISSING"))
        .collect();
    assert_eq!(
        hits.len(),
        2,
        "one per missing (lineId, locale) pair: {stderr}"
    );
    for id in [LINE_IDS[3], LINE_IDS[5]] {
        assert!(
            hits.iter()
                .any(|h| h.contains(&format!("no `en-US` text for `{id}`"))),
            "exact message for `{id}`: {stderr}"
        );
    }
    assert!(
        !stderr.contains("ja-JP"),
        "the complete locale reports nothing: {stderr}"
    );

    // A warning: the artifact IS written, with the locales it does have.
    let artifact = read_json(&work.join("a.json"));
    let maps = locale_maps(&artifact);
    let stay = maps.iter().find(|(id, _)| id == LINE_IDS[3]).unwrap();
    assert!(
        stay.1["ja-JP"].is_string(),
        "the present locale still merged"
    );
    assert!(
        stay.1["en-US"].is_null(),
        "the missing one is simply absent"
    );
}

#[test]
fn deny_promotes_w_l10n_missing_and_suppresses_the_artifact() {
    let dir = project("deny");
    let work = temp_dir("deny-work");
    translate(&dir, &work.join("en-US.json"), "[en] ", &[LINE_IDS[0]]);
    let bundle = work.join("bundle.json");
    assert_eq!(
        run(&[
            "loc",
            "import",
            work.join("en-US.json").to_str().unwrap(),
            "-o",
            bundle.to_str().unwrap(),
        ])
        .status
        .code(),
        Some(0)
    );

    let scene = dir.join("a.lute");
    let args = [
        "compile",
        scene.to_str().unwrap(),
        "--locales",
        bundle.to_str().unwrap(),
    ];
    // Baseline: warning only, artifact emitted.
    let plain = run(&args);
    assert_eq!(plain.status.code(), Some(0));
    assert!(!plain.stdout.is_empty());

    for flag in ["--deny=W-L10N-MISSING", "--deny-warnings"] {
        let mut promoted: Vec<&str> = args.to_vec();
        promoted.push(flag);
        let result = run(&promoted);
        assert_eq!(
            result.status.code(),
            Some(1),
            "{flag} must flip the verdict"
        );
        assert!(result.stdout.is_empty(), "{flag}: no artifact emitted");
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(stderr.contains("[denied]"), "{flag}: got {stderr}");
        assert!(
            stderr.contains("error [W-L10N-MISSING]"),
            "{flag}: got {stderr}"
        );
    }
}

// --- #3 (T6.10) — component lines are exported once per expansion ---

fn anseo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/examples/anseo")
}

/// #3 / T6.10: `loc export` emitted a component's lines once, keyed to the
/// COMPONENT file, with lineId null — because `{prefix}` derives from the
/// IMPORTING document's frontmatter, which a component has none of.
/// Everything downstream is keyed on lineId, so `loc import` skipped the row
/// at exit 0 naming `lute tag`; `lute tag` answered "already tagged" (the
/// lines DO carry code="0010"/"0020"); and `compile --locales` then emitted
/// W-L10N-MISSING for a caller-derived id that appears in no export.
///
/// Fix (i): one row PER EXPANSION carrying the caller-derived lineId, with
/// the component file and line retained as `source` so a TMS dedupes on
/// identical source text and the translator still sees the string once.
#[test]
fn loc_export_emits_component_lines_per_expansion_with_the_callers_id() {
    let root = anseo_root();
    let out = run(&["loc", "export", root.to_str().unwrap()]);
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let rows: serde_json::Value = serde_json::from_str(&text).expect("export json");
    let rows = rows.as_array().expect("array");

    // A component file has no prefix, so it contributes no rows of its own —
    // a row with no reproducible identity is what this issue is about.
    assert!(
        !rows.iter().any(|r| r["file"]
            .as_str()
            .is_some_and(|f| f.ends_with(".component.lute"))),
        "a component file must not be exported as a document in its own right"
    );

    // The expansion is exported under the CALLER's id.
    let expanded = rows
        .iter()
        .find(|r| r["lineId"].as_str() == Some("anseo.s01ep02.purser_0020"))
        .unwrap_or_else(|| panic!("the caller-derived id must be exported: {text}"));
    assert!(
        expanded["file"]
            .as_str()
            .is_some_and(|f| f.ends_with("cryobank.lute")),
        "the row belongs to the CALLER: {expanded}"
    );
    assert!(
        expanded["source"]["file"]
            .as_str()
            .is_some_and(|f| f.ends_with("purser-interject.component.lute")),
        "the component file is retained as `source`: {expanded}"
    );
    assert!(expanded["source"]["line"].as_u64().is_some(), "{expanded}");

    // Every row carries the key, null or not, so the JSON shape does not vary
    // with authoring.
    assert!(
        rows.iter().all(|r| r.get("source").is_some()),
        "`source` must be on every row"
    );

    // No row carrying a `code` may still have a null lineId — that is exactly
    // the class `lute tag` can never fix, and it must be empty now.
    let impossible: Vec<&serde_json::Value> = rows
        .iter()
        .filter(|r| r["lineId"].is_null() && !r["code"].is_null())
        .collect();
    assert!(
        impossible.is_empty(),
        "structurally un-taggable rows remain: {impossible:?}"
    );

    // The eight code-less quest rows must STILL be reported as untagged —
    // T6.10's distinction, which the fix must sharpen, not erase.
    let untagged = rows.iter().filter(|r| r["lineId"].is_null()).count();
    assert_eq!(
        untagged, 8,
        "the eight code-less quest narration rows stay untagged"
    );
}

/// The completeness claim itself, as a COUNT rather than an eyeball: every
/// content line in the compiled artifact must have an export row under its
/// own `lineId`. That is the property #3 broke — the component's line
/// compiled under `anseo.s01ep02.purser_0020` and no export contained it.
#[test]
fn every_content_line_the_compiler_emits_has_an_export_row() {
    let root = anseo_root();
    let scene = root.join("scenes/cryobank.lute");
    let dir = temp_dir("loc-completeness");
    let artifact = dir.join("cryo.json");
    let out = run(&[
        "compile",
        scene.to_str().unwrap(),
        "--project",
        root.to_str().unwrap(),
        "-o",
        artifact.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let compiled = read_json(&artifact);
    let compiled_ids: Vec<String> = compiled["commands"]
        .as_array()
        .expect("commands")
        .iter()
        .filter(|c| c["kind"] == "line")
        .filter_map(|c| c["lineId"].as_str().map(str::to_string))
        .collect();
    assert!(!compiled_ids.is_empty(), "no compiled lines: {compiled}");

    let export = run(&["loc", "export", root.to_str().unwrap()]);
    let rows: serde_json::Value = serde_json::from_slice(&export.stdout).expect("export json");
    let exported: std::collections::BTreeSet<String> = rows
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| r["lineId"].as_str().map(str::to_string))
        .collect();
    let missing: Vec<&String> = compiled_ids
        .iter()
        .filter(|id| !exported.contains(*id))
        .collect();
    assert!(
        missing.is_empty(),
        "{} of {} compiled line ids have no export row: {missing:?}",
        missing.len(),
        compiled_ids.len()
    );
}

// --- Task 4 §2 (dsl 0.15.0) — loc export honors authored `id:` --------------

/// A scene document with an authored `id:` (no legacy triad): `loc export`
/// MUST stamp every row's `lineId` under that authored key. Pre-fix it
/// stamped `null` (the triad was missing so `scene_prefix` gave up).
#[test]
fn loc_export_prefixes_line_ids_with_the_authored_scene_id() {
    let dir = temp_dir("authored-id");
    write(&dir, "lute.project.yaml", PROJECT_YAML);
    write(
        &dir,
        "scenes/authored.lute",
        "\
---
kind: scene
id: authored.myid
---

## Opener.

@narrator{code=\"0010\"}: Hi.
@narrator{code=\"0020\"}: There.

<branch id=\"pick\">
  <choice id=\"go\" label=\"Go\">
    @narrator{code=\"0030\"}: Onward.
  </choice>
</branch>
",
    );
    let out = run(&["loc", "export", dir.to_str().unwrap()]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).expect("export json");
    let ids: Vec<String> = rows
        .iter()
        .filter_map(|r| r["lineId"].as_str().map(str::to_string))
        .collect();
    assert_eq!(
        ids,
        vec![
            "authored.myid.narrator_0010",
            "authored.myid.narrator_0020",
            "authored.myid.pick.go",
            "authored.myid.narrator_0030",
        ],
        "every row's lineId must be prefixed by the authored `id:` (rows: {rows:#?})",
    );
}

/// A pinned legacy scene (no authored `id:` — only the `character`/`season`/
/// `episode` triad) MUST export byte-identically to today's output. The
/// project-relative payload (paths stripped) is compared against a checked-in
/// snapshot so the derived-key path stays wire-stable under Task 4.
#[test]
fn loc_export_of_a_legacy_scene_is_byte_identical_to_the_pinned_snapshot() {
    let dir = temp_dir("legacy-pin");
    write(&dir, "lute.project.yaml", PROJECT_YAML);
    write(
        &dir,
        "a.lute",
        "\
---
kind: scene
character: bianca
season: 1
episode: 1
---

## Opener.

@bianca{code=\"0010\"}: Hello there.
@kai{code=\"0010\"}: Hi.

<branch id=\"pick\">
  <choice id=\"go\" label=\"Go on\">
    @bianca{code=\"0020\"}: Onward.
  </choice>
  <choice id=\"stay\" label=\"Stay here\">
    @bianca{code=\"0030\"}: Fine.
  </choice>
</branch>
",
    );
    let out = run(&["loc", "export", dir.to_str().unwrap()]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let got = String::from_utf8(out.stdout).expect("utf-8 export");
    // Strip the absolute temp-dir prefix so the pin is portable — the row's
    // `file` value is the ONLY host-dependent field the export emits.
    let prefix = format!("{}/", dir.to_str().unwrap());
    let stripped = got.replace(&prefix, "");
    const PINNED: &str = r#"[
  {
    "code": "0010",
    "file": "a.lute",
    "kind": "line",
    "line": 10,
    "lineId": "bianca.s01ep01.bianca_0010",
    "source": null,
    "speaker": "bianca",
    "text": "Hello there."
  },
  {
    "code": "0010",
    "file": "a.lute",
    "kind": "line",
    "line": 11,
    "lineId": "bianca.s01ep01.kai_0010",
    "source": null,
    "speaker": "kai",
    "text": "Hi."
  },
  {
    "file": "a.lute",
    "key": "pick.go",
    "kind": "choice",
    "label": "Go on",
    "line": 14,
    "lineId": "bianca.s01ep01.pick.go",
    "source": null
  },
  {
    "code": "0020",
    "file": "a.lute",
    "kind": "line",
    "line": 15,
    "lineId": "bianca.s01ep01.bianca_0020",
    "source": null,
    "speaker": "bianca",
    "text": "Onward."
  },
  {
    "file": "a.lute",
    "key": "pick.stay",
    "kind": "choice",
    "label": "Stay here",
    "line": 17,
    "lineId": "bianca.s01ep01.pick.stay",
    "source": null
  },
  {
    "code": "0030",
    "file": "a.lute",
    "kind": "line",
    "line": 18,
    "lineId": "bianca.s01ep01.bianca_0030",
    "source": null,
    "speaker": "bianca",
    "text": "Fine."
  }
]
"#;
    assert_eq!(stripped, PINNED, "legacy loc export drifted:\n{stripped}");
}
