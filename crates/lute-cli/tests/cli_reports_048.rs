//! 0.48.0 §3.7, §3.8, §3.10, §3.12, §3.13, §3.15 (`loc import`), §4: the
//! rewriters and scaffolders report with `--json` (`0.48.0.tag`, `.fix`,
//! `.fmt`, `.catalog-refresh`, `.init`, `.new`, `.loc-import`), `tag
//! --renumber` previews and `--commit` writes, `--check` verifies without
//! writing, `fmt`/`fix`/`tag` default to `.`, and `init` refuses to write over
//! an existing file. Every test fails on 0.47.2 (the option, the report or the
//! behaviour does not exist there).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn lute_in(cwd: &Path, args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap()
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn all(out: &Output) -> String {
    format!("{}{}", stdout(out), stderr(out))
}

/// The one JSON document on stdout.
fn json(out: &Output) -> Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| panic!("{e}: {}", all(out)))
}

fn write(dir: &Path, rel: &str, text: &str) -> PathBuf {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, text).unwrap();
    path
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

const DRAFT: &str = "---\nkind: scene\nid: d\n---\n## Shot 1.\n\
                     @ann{code=\"0050\"}: one\n\
                     @ann{code=\"0020\"}: two\n\
                     @ann: three\n";
const UNTAGGED: &str = "---\nkind: scene\nid: u\n---\n## Shot 1.\n@ann: one\n@bob: two\n";
const LOCKED: &str = "---\nkind: scene\nid: l\ncodesLocked: true\n---\n## Shot 1.\n\
                      @ann{code=\"0050\"}: one\n::use{component=\"fire\"}\n";

// --- tag ---------------------------------------------------------------------

#[test]
fn renumber_previews_every_change_and_writes_nothing() {
    let dir = lute_lib_scratch("renumber-preview");
    let f = write(&dir, "d.lute", DRAFT);
    let out = lute_in(&dir, &["tag", "--renumber", "d.lute"]);
    assert_eq!(out.status.code(), Some(0), "{}", all(&out));
    assert_eq!(
        stdout(&out),
        "d.lute:6: 0050 -> 0010\n\
         d.lute:8: (none) -> 0030\n\
         lute: would renumber 2 line(s) in 1 of 1 file(s); pass --commit to apply\n"
    );
    assert_eq!(read(&f), DRAFT, "a preview writes nothing");
}

#[test]
fn renumber_commit_is_the_old_force() {
    let dir = lute_lib_scratch("renumber-commit");
    let f = write(&dir, "d.lute", DRAFT);
    let out = lute_in(&dir, &["tag", "--renumber", "--commit", "d.lute"]);
    assert_eq!(out.status.code(), Some(0), "{}", all(&out));
    assert_eq!(stdout(&out), "lute: renumbered 2 line(s)\n");
    let after = read(&f);
    assert!(after.contains("@ann{code=\"0010\"}: one"), "{after}");
    assert!(after.contains("@ann{code=\"0030\"}: three"), "{after}");
    // A second preview has nothing to do.
    let again = lute_in(&dir, &["tag", "--renumber", "d.lute"]);
    assert_eq!(again.status.code(), Some(0));
    assert!(
        stdout(&again).contains("codes already in order"),
        "{}",
        all(&again)
    );
}

#[test]
fn force_is_removed_and_names_the_replacement() {
    let dir = lute_lib_scratch("force-removed");
    write(&dir, "d.lute", DRAFT);
    let out = lute_in(&dir, &["tag", "--force", "d.lute", "--json"]);
    assert_eq!(out.status.code(), Some(2), "{}", all(&out));
    let v = json(&out);
    assert_eq!(v["error"]["code"], "E-CLI-USAGE", "{v}");
    assert_eq!(v["error"]["argument"], "--force", "{v}");
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("--renumber --commit"),
        "{v}"
    );
    assert_eq!(read(&dir.join("d.lute")), DRAFT);
}

#[test]
fn commit_needs_renumber_and_excludes_check() {
    let dir = lute_lib_scratch("commit-conflicts");
    write(&dir, "d.lute", DRAFT);
    for args in [
        &["tag", "--commit", "d.lute", "--json"][..],
        &[
            "tag",
            "--renumber",
            "--commit",
            "--check",
            "d.lute",
            "--json",
        ][..],
    ] {
        let out = lute_in(&dir, args);
        assert_eq!(out.status.code(), Some(2), "{args:?}: {}", all(&out));
        assert_eq!(json(&out)["error"]["code"], "E-CLI-USAGE", "{args:?}");
    }
    assert_eq!(read(&dir.join("d.lute")), DRAFT);
}

#[test]
fn tag_check_writes_nothing_and_exits_one_when_a_file_would_change() {
    let dir = lute_lib_scratch("tag-check");
    let f = write(&dir, "u.lute", UNTAGGED);
    let out = lute_in(&dir, &["tag", "--check", "u.lute"]);
    assert_eq!(out.status.code(), Some(1), "{}", all(&out));
    assert!(
        stdout(&out).contains("would tag 2 line(s)"),
        "{}",
        all(&out)
    );
    assert_eq!(read(&f), UNTAGGED);
    // Tag for real, then the check is clean.
    assert_eq!(lute_in(&dir, &["tag", "u.lute"]).status.code(), Some(0));
    assert_eq!(
        lute_in(&dir, &["tag", "--check", "u.lute"]).status.code(),
        Some(0)
    );
    // `--renumber --check` is the renumber verdict.
    write(&dir, "d.lute", DRAFT);
    let out = lute_in(&dir, &["tag", "--renumber", "--check", "d.lute"]);
    assert_eq!(out.status.code(), Some(1), "{}", all(&out));
    assert_eq!(read(&dir.join("d.lute")), DRAFT);
}

#[test]
fn tag_defaults_to_the_current_directory_and_takes_several_paths() {
    let dir = lute_lib_scratch("tag-default");
    write(&dir, "a/u.lute", UNTAGGED);
    write(&dir, "b/u.lute", UNTAGGED);
    let out = lute_in(&dir, &["tag"]);
    assert_eq!(out.status.code(), Some(0), "{}", all(&out));
    assert!(
        stdout(&out).contains("tagged 4 line(s) in 2 of 2 file(s)"),
        "{}",
        all(&out)
    );
    write(&dir, "c/u.lute", UNTAGGED);
    write(&dir, "d/u.lute", UNTAGGED);
    let out = lute_in(&dir, &["tag", "c", "d/u.lute", "--check"]);
    assert_eq!(out.status.code(), Some(1), "{}", all(&out));
    assert!(
        stdout(&out).contains("would tag 4 line(s) in 2 of 2 file(s)"),
        "{}",
        all(&out)
    );
}

#[test]
fn tag_report_in_both_modes() {
    let dir = lute_lib_scratch("tag-report");
    write(&dir, "u.lute", UNTAGGED);
    write(&dir, "d.lute", DRAFT);
    let out = lute_in(&dir, &["tag", "u.lute", "--json"]);
    assert_eq!(out.status.code(), Some(0), "{}", all(&out));
    let v = json(&out);
    assert_eq!(v["command"], "tag", "{v}");
    assert_eq!(v["schemaVersion"], "0.48.0.tag", "{v}");
    assert_eq!(v["ok"], true);
    assert_eq!(v["mode"], "tag");
    assert_eq!(v["applied"], true);
    assert_eq!(
        v["files"],
        serde_json::json!([{"path": "u.lute", "changed": true, "lines": 2, "instances": 0}])
    );
    // Untouched: nothing changed, nothing applied.
    let v = json(&lute_in(&dir, &["tag", "u.lute", "--json"]));
    assert_eq!(v["applied"], false, "{v}");
    assert_eq!(v["files"][0]["changed"], false, "{v}");

    let out = lute_in(&dir, &["tag", "--renumber", "d.lute", "--json"]);
    assert_eq!(out.status.code(), Some(0), "{}", all(&out));
    let v = json(&out);
    assert_eq!(v["mode"], "renumber");
    assert_eq!(v["applied"], false, "a preview applies nothing: {v}");
    assert_eq!(
        v["files"][0]["renumbered"],
        serde_json::json!([
            {"line": 6, "from": "0050", "to": "0010"},
            {"line": 8, "from": null, "to": "0030"}
        ])
    );
    assert_eq!(v["files"][0]["lines"], 2);
    // The human lines moved to stderr.
    assert!(
        stderr(&out).contains("d.lute:6: 0050 -> 0010"),
        "{}",
        all(&out)
    );

    let v = json(&lute_in(
        &dir,
        &["tag", "--renumber", "--commit", "d.lute", "--json"],
    ));
    assert_eq!(v["applied"], true, "{v}");
}

#[test]
fn codes_locked_refuses_the_renumber_but_adds_instance_keys() {
    let dir = lute_lib_scratch("locked");
    let f = write(&dir, "l.lute", LOCKED);
    // Preview: nothing is written; the refusal still decides the exit.
    let out = lute_in(&dir, &["tag", "--renumber", "l.lute"]);
    assert_eq!(out.status.code(), Some(1), "{}", all(&out));
    assert!(
        stdout(&out).contains("l.lute: would add 1 component instance key(s)"),
        "{}",
        all(&out)
    );
    assert!(stderr(&out).contains("codesLocked"), "{}", all(&out));
    assert_eq!(read(&f), LOCKED);
    // Commit: the keys are added, the codes are kept, exit 1.
    let out = lute_in(&dir, &["tag", "--renumber", "--commit", "l.lute", "--json"]);
    assert_eq!(out.status.code(), Some(1), "{}", all(&out));
    let after = read(&f);
    assert!(after.contains("instance=\"use-001\""), "{after}");
    assert!(after.contains("code=\"0050\""), "{after}");
    let v = json(&out);
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["applied"], true, "{v}");
    assert_eq!(v["files"][0]["instances"], 1, "{v}");
    assert!(
        v["files"][0]["refused"]
            .as_str()
            .unwrap()
            .contains("codesLocked"),
        "{v}"
    );
}

// --- fix / fmt ---------------------------------------------------------------

const OLD_STAMP: &str = "---\nkind: scene\nid: s\nluteVersion: \"0.1.0\"\n---\n## Shot 1.\n\
                         @narrator: hi\n";

#[test]
fn fix_check_writes_nothing_and_reports() {
    let dir = lute_lib_scratch("fix-check");
    let f = write(&dir, "s.lute", OLD_STAMP);
    let out = lute_in(&dir, &["fix", "--check", "s.lute", "--json"]);
    assert_eq!(out.status.code(), Some(1), "{}", all(&out));
    let v = json(&out);
    assert_eq!(v["command"], "fix", "{v}");
    assert_eq!(v["schemaVersion"], "0.53.0.fix", "{v}");
    assert_eq!(v["ok"], false);
    assert_eq!(v["applied"], false);
    assert_eq!(v["files"][0]["path"], "s.lute");
    assert_eq!(v["files"][0]["changed"], true);
    assert_eq!(v["files"][0]["fixes"], 1);
    assert_eq!(
        v["files"][0]["rules"],
        serde_json::json!([{ "rule": "lute-version", "fixes": 1 }])
    );
    assert_eq!(v["files"][0]["kept"], serde_json::json!([]));
    assert_eq!(read(&f), OLD_STAMP);
    // Applying, with the default path `.`.
    let out = lute_in(&dir, &["fix", "--json"]);
    assert_eq!(out.status.code(), Some(0), "{}", all(&out));
    let v = json(&out);
    assert_eq!(v["applied"], true, "{v}");
    assert_ne!(read(&f), OLD_STAMP);
    let out = lute_in(&dir, &["fix", "--check"]);
    assert_eq!(out.status.code(), Some(0), "{}", all(&out));
}

#[test]
fn fmt_reports_every_selected_file_and_defaults_to_the_current_directory() {
    let dir = lute_lib_scratch("fmt-report");
    let messy = "## S\n@narrator: hi  \n";
    write(&dir, "a.lute", messy);
    write(&dir, "b.lute", "## S\n@narrator: hi\n");
    let out = lute_in(&dir, &["fmt", "--check", "--json"]);
    assert_eq!(out.status.code(), Some(1), "{}", all(&out));
    let v = json(&out);
    assert_eq!(v["command"], "fmt", "{v}");
    assert_eq!(v["schemaVersion"], "0.48.0.fmt", "{v}");
    assert_eq!(v["applied"], false);
    assert_eq!(
        v["files"],
        serde_json::json!([
            {"path": "./a.lute", "changed": true},
            {"path": "./b.lute", "changed": false}
        ])
    );
    // The path line of check mode went to stderr.
    assert!(stderr(&out).contains("a.lute"), "{}", all(&out));
    assert_eq!(read(&dir.join("a.lute")), messy);
    let out = lute_in(&dir, &["fmt", "--json"]);
    assert_eq!(out.status.code(), Some(0), "{}", all(&out));
    let v = json(&out);
    assert_eq!(v["applied"], true, "{v}");
    assert_ne!(read(&dir.join("a.lute")), messy);
}

// --- catalog refresh ----------------------------------------------------------

#[test]
fn catalog_refresh_reports_refreshed_and_skipped_snapshots() {
    let dir = lute_lib_scratch("catalog");
    write(
        &dir,
        "lute.project.yaml",
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n",
    );
    write(&dir, "catalog/notes.yaml", "not: [a provider snapshot\n");
    write(
        &dir,
        "catalog/speakers.yaml",
        "manifestVersion: test\nproviderVersion: \"1\"\nstale: false\nentries:\n  speakerId: [hero]\n",
    );
    let out = lute_in(&dir, &["catalog", "refresh", "--json"]);
    assert_eq!(out.status.code(), Some(0), "{}", all(&out));
    let v = json(&out);
    assert_eq!(v["command"], "catalog refresh", "{v}");
    assert_eq!(v["schemaVersion"], "0.48.0.catalog-refresh", "{v}");
    assert!(v["providers"].as_str().unwrap().contains("catalog"), "{v}");
    assert!(v["capabilitySnapshot"].is_string(), "{v}");
    assert_eq!(
        v["refreshed"],
        serde_json::json!(["./catalog/speakers.yaml"])
    );
    assert_eq!(v["skipped"][0]["path"], "./catalog/notes.yaml", "{v}");
    assert!(v["skipped"][0]["reason"].is_string(), "{v}");
}

// --- init / new --------------------------------------------------------------

#[test]
fn init_defaults_to_the_current_directory_and_reports() {
    let dir = lute_lib_scratch("init-default");
    let out = lute_in(&dir, &["init", "--json"]);
    assert_eq!(out.status.code(), Some(0), "{}", all(&out));
    let v = json(&out);
    assert_eq!(v["command"], "init", "{v}");
    assert_eq!(v["schemaVersion"], "0.48.0.init", "{v}");
    assert_eq!(v["dir"], ".");
    assert_eq!(v["template"], "minimal");
    let files = v["files"].as_array().unwrap();
    assert!(files.iter().any(|f| f == "./lute.project.yaml"), "{v}");
    for file in files {
        assert!(dir.join(file.as_str().unwrap()).is_file(), "{file}");
    }
    // The human summary is on stderr in JSON mode.
    assert!(
        stderr(&out).contains("Initialized `minimal`"),
        "{}",
        all(&out)
    );
}

#[test]
fn init_refuses_when_any_file_it_would_write_exists() {
    let dir = lute_lib_scratch("init-refuse");
    let readme = write(&dir, "README.md", "mine\n");
    let out = lute_in(&dir, &["init", "--json"]);
    assert_eq!(out.status.code(), Some(2), "{}", all(&out));
    let v = json(&out);
    assert_eq!(v["ok"], false);
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("README.md"),
        "names the file: {v}"
    );
    assert_eq!(read(&readme), "mine\n", "never overwritten");
    assert!(
        !dir.join("lute.project.yaml").exists(),
        "nothing is written when it refuses"
    );
    // An unrelated file does not stop it.
    let other = lute_lib_scratch("init-other");
    write(&other, "notes.txt", "x\n");
    assert_eq!(lute_in(&other, &["init"]).status.code(), Some(0));
    assert_eq!(read(&other.join("notes.txt")), "x\n");
    // A second run refuses on the manifest.
    let out = lute_in(&other, &["init"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("lute.project.yaml"), "{}", all(&out));
}

#[test]
fn new_reports_kind_id_and_path_for_each_kind() {
    let dir = lute_lib_scratch("new-report");
    assert_eq!(lute_in(&dir, &["init"]).status.code(), Some(0));
    for (args, kind, id) in [
        (&["new", "scene", "harborNight"][..], "scene", "harborNight"),
        (&["new", "quest", "findKey"][..], "quest", "findKey"),
        (&["new", "lore", "oldMap"][..], "lore", "oldMap"),
        (&["new", "schema", "extra"][..], "schema", "extra"),
    ] {
        let mut full: Vec<&str> = args.to_vec();
        full.push("--json");
        let out = lute_in(&dir, &full);
        assert_eq!(out.status.code(), Some(0), "{full:?}: {}", all(&out));
        let v = json(&out);
        assert_eq!(v["command"], format!("new {kind}"), "{v}");
        assert_eq!(v["schemaVersion"], "0.48.0.new", "{v}");
        assert_eq!(v["kind"], kind);
        assert_eq!(v["id"], id);
        let path = v["path"].as_str().unwrap();
        assert!(dir.join(path).is_file() || Path::new(path).is_file(), "{v}");
    }
    // A refusal is the error envelope.
    let out = lute_in(&dir, &["new", "scene", "harborNight", "--json"]);
    assert_eq!(out.status.code(), Some(2), "{}", all(&out));
    assert_eq!(json(&out)["error"]["code"], "E-CLI-IO");
}

// --- loc import ----------------------------------------------------------------

#[test]
fn loc_import_reports_locales_lines_skipped_and_diagnostics() {
    let dir = lute_lib_scratch("loc-import");
    write(
        &dir,
        "ja-JP.json",
        r#"[{"kind":"line","lineId":"a.ann.0010","text":"こんにちは"},
            {"kind":"line","lineId":"a.ann.0020","text":"さようなら"},
            {"kind":"line","text":"untagged"}]"#,
    );
    // `--json` needs `--output`: stdout carries the report, not the bundle.
    let out = lute_in(&dir, &["loc", "import", "ja-JP.json", "--json"]);
    assert_eq!(out.status.code(), Some(2), "{}", all(&out));
    let v = json(&out);
    assert_eq!(v["error"]["code"], "E-CLI-USAGE", "{v}");
    assert_eq!(v["error"]["argument"], "--output", "{v}");

    let out = lute_in(
        &dir,
        &["loc", "import", "ja-JP.json", "-o", "bundle.json", "--json"],
    );
    assert_eq!(out.status.code(), Some(0), "{}", all(&out));
    let v = json(&out);
    assert_eq!(v["command"], "loc import", "{v}");
    assert_eq!(v["schemaVersion"], "0.48.0.loc-import", "{v}");
    assert_eq!(v["output"], "bundle.json");
    assert_eq!(v["locales"], serde_json::json!(["ja-JP"]));
    assert_eq!(v["lines"], 2);
    assert_eq!(v["skipped"], 1);
    assert_eq!(v["diagnostics"], serde_json::json!([]));
    assert!(dir.join("bundle.json").is_file());

    // A duplicate lineId is E-LOCALE-BUNDLE: exit 1, still one report.
    write(
        &dir,
        "en-US.json",
        r#"[{"kind":"line","lineId":"x","text":"a"},{"kind":"line","lineId":"x","text":"b"}]"#,
    );
    let out = lute_in(
        &dir,
        &["loc", "import", "en-US.json", "-o", "en.json", "--json"],
    );
    assert_eq!(out.status.code(), Some(1), "{}", all(&out));
    let v = json(&out);
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["output"], Value::Null);
    assert_eq!(v["diagnostics"][0]["code"], "E-LOCALE-BUNDLE", "{v}");
    assert_eq!(v["diagnostics"][0]["path"], "en-US.json", "{v}");
    assert!(!dir.join("en.json").exists());
}

fn lute_lib_scratch(name: &str) -> lute_scratch::ScratchDir {
    lute_scratch::ScratchDir::named(name)
}

// --- review fixes: 0.47.2 parity of the tree summaries ------------------------

const WITH_USE: &str =
    "---\nkind: scene\nid: c\n---\n## Shot 1.\n@ann: one\n::use{component=\"fire\"}\n";

#[test]
fn tag_directory_summary_counts_codes_and_instance_keys() {
    let dir = lute_lib_scratch("tag-summary");
    write(&dir, "a/c.lute", WITH_USE);
    write(&dir, "b/u.lute", UNTAGGED);
    let out = lute_in(&dir, &["tag", "--check", "a", "b"]);
    assert_eq!(out.status.code(), Some(1), "{}", all(&out));
    assert!(
        stdout(&out).contains("would tag 4 line(s) in 2 of 2 file(s)"),
        "{}",
        all(&out)
    );
    let out = lute_in(&dir, &["tag"]);
    assert!(
        stdout(&out).contains("tagged 4 line(s) in 2 of 2 file(s)"),
        "{}",
        all(&out)
    );
}

#[test]
fn tree_summaries_are_printed_before_an_io_failure() {
    let dir = lute_lib_scratch("summary-io");
    write(
        &dir,
        "a.lute",
        "---\nkind: scene\nid: a\n---\n## S\n@ann: one\n",
    );
    std::fs::write(dir.join("bad.lute"), [0xff, 0xfe, 0x00]).unwrap();
    for (args, summary) in [
        (&["tag"][..], "tagged 1 line(s) in 1 of 2 file(s)"),
        (&["fix"][..], "applied 0 fix(es) in 0 of 2 file(s)"),
    ] {
        let out = lute_in(&dir, args);
        assert_eq!(out.status.code(), Some(2), "{args:?}: {}", all(&out));
        assert!(stdout(&out).contains(summary), "{args:?}: {}", all(&out));
        assert!(stderr(&out).contains("cannot read"), "{}", all(&out));
        // JSON mode: the error envelope instead of the report.
        let mut json_args = args.to_vec();
        json_args.push("--json");
        let out = lute_in(&dir, &json_args);
        assert_eq!(out.status.code(), Some(2));
        assert_eq!(json(&out)["error"]["code"], "E-CLI-IO");
    }
}

#[test]
fn fix_counts_files_per_operand_as_before() {
    let dir = lute_lib_scratch("fix-overlap");
    write(&dir, "d/s.lute", OLD_STAMP);
    let out = lute_in(&dir, &["fix", "d", "d/s.lute"]);
    assert_eq!(out.status.code(), Some(0), "{}", all(&out));
    assert!(
        stdout(&out).contains("in 1 of 2 file(s)"),
        "one entry per operand: {}",
        all(&out)
    );
}

#[test]
fn init_refuses_before_writing_when_a_parent_is_not_a_directory() {
    let dir = lute_lib_scratch("init-parent");
    write(&dir, "scenes", "a file where a directory goes\n");
    let out = lute_in(&dir, &["init", "--json"]);
    assert_eq!(out.status.code(), Some(2), "{}", all(&out));
    let v = json(&out);
    assert!(
        v["error"]["message"].as_str().unwrap().contains("scenes"),
        "{v}"
    );
    assert!(!dir.join("lute.project.yaml").exists(), "nothing written");
    assert_eq!(read(&dir.join("scenes")), "a file where a directory goes\n");
}
