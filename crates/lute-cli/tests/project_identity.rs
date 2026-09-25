//! Project-level identity and snapshot gates that `check`/`check-project`
//! used to pass while `compile --all` or the engine got it wrong:
//!
//! * T1-10 (lamplight F37): a tagged component line shared its `lineId` with
//!   another expansion of itself or with a host line. dsl 0.22.0 §11 gives
//!   each expansion its own `{prefix}.{component}#{n}` scope.
//! * T1-9 (lamplight F38, ashen F39): a `voiceKey` template without
//!   `{prefix}` (the 0.21 default) lands lines of different documents on one
//!   voice asset (`E-DUP-VOICEKEY`); the 0.22.0 default carries `{prefix}`.
//! * T1-11 (ashen F8): a two-profile project passed `check-project` and was
//!   then refused by `compile --all`/`play` (`E-CAPABILITY-MISMATCH`).
//! * T3-9 (ashen F2): `lute check <file>` ignored the project the file sits in.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

const MANIFEST: &str = "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n";

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "lute-project-identity-{tag}-{}-{n}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &Path, rel: &str, content: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

fn lute(args: &[&str]) -> Output {
    Command::new(BIN).args(args).output().unwrap()
}

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

fn p(path: &Path) -> &str {
    path.to_str().unwrap()
}

// ---- T1-10: component expansion identity (dsl 0.22.0 §11) -----------------

/// A project whose one scene `::use`s component `c` (a tagged `@ann` line)
/// after `host` (the scene body before the uses).
fn component_project(tag: &str, component_line: &str, host: &str) -> PathBuf {
    let dir = temp_dir(tag);
    write(&dir, "lute.project.yaml", MANIFEST);
    write(
        &dir,
        "components/c.component.lute",
        &format!("---\ncomponent: c\n---\n## Scene 1.\n{component_line}\n"),
    );
    write(
        &dir,
        "scenes/a.lute",
        &format!(
            "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n\
             components: [../components/c.component.lute]\n---\n## Shot 1.\n{host}"
        ),
    );
    dir
}

/// `check-project` is clean, and `compile` mints these `(lineId, voiceKey)`
/// pairs for the scene's `@ann` lines, in order.
fn assert_clean_with_ann_ids(dir: &Path, want: &[(&str, &str)]) {
    let project = lute(&["check-project", p(dir)]);
    assert_eq!(project.status.code(), Some(0), "{}", text(&project));
    let out = lute(&[
        "compile",
        p(&dir.join("scenes/a.lute")),
        "--project",
        p(dir),
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let art: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let got: Vec<(String, String)> = art["commands"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["kind"] == "line" && c["speaker"] == "ann")
        .map(|c| {
            (
                c["lineId"].as_str().unwrap().to_string(),
                c["voiceKey"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    let want: Vec<(String, String)> = want
        .iter()
        .map(|(l, v)| (l.to_string(), v.to_string()))
        .collect();
    assert_eq!(got, want);
}

#[test]
fn each_use_of_a_tagged_component_line_gets_its_own_ids() {
    let dir = component_project(
        "use-twice",
        "@ann{code=\"0010\"}: comp line.",
        "@narrator: before.\n::use{component=\"c\"}\n::use{component=\"c\"}\n",
    );
    assert_clean_with_ann_ids(
        &dir,
        &[
            ("x.s01ep01.c#1.ann_0010", "x.s01ep01.c#1.ann-0010"),
            ("x.s01ep01.c#2.ann_0010", "x.s01ep01.c#2.ann-0010"),
        ],
    );
}

#[test]
fn a_component_line_never_shares_ids_with_a_host_line_of_the_same_code() {
    let dir = component_project(
        "host-collision",
        "@ann{code=\"0010\"}: comp line.",
        "@ann{code=\"0010\"}: host line.\n::use{component=\"c\"}\n",
    );
    assert_clean_with_ann_ids(
        &dir,
        &[
            ("x.s01ep01.ann_0010", "x.s01ep01.ann-0010"),
            ("x.s01ep01.c#1.ann_0010", "x.s01ep01.c#1.ann-0010"),
        ],
    );
}

/// An untagged component line back-fills within its own expansion, so its
/// code does not depend on how many host lines precede the `::use`, and host
/// lines after it keep the codes `lute tag` would give them.
#[test]
fn untagged_component_lines_backfill_per_expansion() {
    let dir = component_project(
        "untagged",
        "@ann: comp line.",
        "@ann{code=\"0010\"}: host line.\n::use{component=\"c\"}\n::use{component=\"c\"}\n\
         @ann: after.\n",
    );
    assert_clean_with_ann_ids(
        &dir,
        &[
            ("x.s01ep01.ann_0010", "x.s01ep01.ann-0010"),
            ("x.s01ep01.c#1.ann_0010", "x.s01ep01.c#1.ann-0010"),
            ("x.s01ep01.c#2.ann_0010", "x.s01ep01.c#2.ann-0010"),
            ("x.s01ep01.ann_0020", "x.s01ep01.ann-0020"),
        ],
    );
}

/// A nested `::use` is scoped under its host expansion, and `loc export`
/// reproduces the ids `compile` mints.
#[test]
fn nested_uses_nest_scopes_and_loc_export_agrees() {
    let dir = component_project(
        "nested",
        "@ann{code=\"0010\"}: comp line.",
        "::use{component=\"outer\"}\n::use{component=\"outer\"}\n",
    );
    write(
        &dir,
        "components/outer.component.lute",
        "---\ncomponent: outer\ncomponents: [c.component.lute]\n---\n## Scene 1.\n\
         ::use{component=\"c\"}\n::use{component=\"c\"}\n",
    );
    write(
        &dir,
        "scenes/a.lute",
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n\
         components: [../components/outer.component.lute]\n---\n## Shot 1.\n\
         ::use{component=\"outer\"}\n::use{component=\"outer\"}\n",
    );
    let ids = [
        "x.s01ep01.outer#1.c#1.ann_0010",
        "x.s01ep01.outer#1.c#2.ann_0010",
        "x.s01ep01.outer#2.c#1.ann_0010",
        "x.s01ep01.outer#2.c#2.ann_0010",
    ];
    let want: Vec<(&str, String)> = ids
        .iter()
        .map(|id| (*id, id.replace("_0010", "-0010")))
        .collect();
    assert_clean_with_ann_ids(
        &dir,
        &want
            .iter()
            .map(|(l, v)| (*l, v.as_str()))
            .collect::<Vec<_>>(),
    );

    let export = lute(&["loc", "export", p(&dir)]);
    assert_eq!(export.status.code(), Some(0), "{}", text(&export));
    let rows: serde_json::Value = serde_json::from_slice(&export.stdout).unwrap();
    let exported: Vec<&str> = rows
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| r["lineId"].as_str())
        .filter(|id| id.starts_with("x.s01ep01."))
        .collect();
    assert_eq!(exported, ids);
}

// ---- T1-9: project-wide voiceKey collisions --------------------------------

fn voice_project(tag: &str, manifest: &str, second_text: &str) -> PathBuf {
    let dir = temp_dir(tag);
    write(&dir, "lute.project.yaml", manifest);
    for (file, character, line) in [("a", "x", "Hello."), ("b", "y", second_text)] {
        write(
            &dir,
            &format!("scenes/{file}.lute"),
            &format!(
                "---\nkind: scene\ncharacter: {character}\nseason: 1\nepisode: 1\n---\n\
                 ## Shot 1.\n@ann{{code=\"0010\"}}: {line}\n"
            ),
        );
    }
    dir
}

/// Under the 0.21 default pinned (`{speaker}-{code}`, no `{prefix}`), two
/// documents voicing different text under one key still fail.
#[test]
fn two_documents_voicing_different_text_under_one_voice_key_fail_check_project_and_compile_all() {
    let pinned = format!("{MANIFEST}identity:\n  voiceKey: \"{{speaker}}-{{code}}\"\n");
    let dir = voice_project("voice-collide", &pinned, "Goodbye.");
    let project = lute(&["check-project", p(&dir)]);
    assert_eq!(project.status.code(), Some(1), "{}", text(&project));
    let out = text(&project);
    assert!(
        out.contains("E-DUP-VOICEKEY") && out.contains("ann-0010"),
        "{out}"
    );

    let out_dir = dir.join("out");
    let all = lute(&["compile", "--all", "--project", p(&dir), "-o", p(&out_dir)]);
    assert_eq!(all.status.code(), Some(1), "{}", text(&all));
    assert!(text(&all).contains("E-DUP-VOICEKEY"), "{}", text(&all));
    assert!(!out_dir.exists(), "no output may be written on a collision");
}

/// Same text under one key is one recording, and the default `{prefix}`ed
/// template (dsl 0.22.0 §11) keeps the two documents' keys apart.
#[test]
fn shared_text_or_the_default_prefixed_voice_key_is_clean() {
    let pinned = format!("{MANIFEST}identity:\n  voiceKey: \"{{speaker}}-{{code}}\"\n");
    let same = voice_project("voice-same-text", &pinned, "Hello.");
    let o = lute(&["check-project", p(&same)]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));

    let default = voice_project("voice-default", MANIFEST, "Goodbye.");
    let o = lute(&["check-project", p(&default)]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    let out_dir = default.join("out");
    let o = lute(&[
        "compile",
        "--all",
        "--project",
        p(&default),
        "-o",
        p(&out_dir),
    ]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
}

// ---- T1-11: one capability snapshot per project ----------------------------

#[test]
fn check_project_refuses_the_two_profile_project_compile_all_refuses() {
    let dir = temp_dir("two-profiles");
    write(
        &dir,
        "lute.project.yaml",
        "pluginsDir: plugins/\ndefaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n  \
         withAnnounce:\n    plugins: { demo.plugin: true }\n",
    );
    write(
        &dir,
        "plugins/demo.plugin/plugin.yaml",
        "id: demo.plugin\nversion: 0.1.0\nkind: capability\nexports:\n  directives: directives/\n",
    );
    write(
        &dir,
        "plugins/demo.plugin/directives/d.yaml",
        "directives:\n  - { name: announce, attrs: [ { name: text, type: string } ] }\n",
    );
    write(
        &dir,
        "a.lute",
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nprofile: withAnnounce\n---\n\
         ## Shot 1.\n::announce{text=\"hi\"}\n@narrator: a.\n",
    );
    write(
        &dir,
        "b.lute",
        "---\nkind: scene\ncharacter: y\nseason: 1\nepisode: 1\n---\n## Shot 1.\n@narrator: b.\n",
    );

    let all = lute(&[
        "compile",
        "--all",
        "--project",
        p(&dir),
        "-o",
        p(&dir.join("out")),
    ]);
    assert_eq!(all.status.code(), Some(1), "{}", text(&all));
    let project = lute(&["check-project", p(&dir)]);
    assert_eq!(project.status.code(), Some(1), "{}", text(&project));
    let out = text(&project);
    assert!(out.contains("E-CAPABILITY-MISMATCH"), "{out}");
    // The SAME message `compile --all` refuses with.
    let refusal = String::from_utf8_lossy(&all.stderr);
    let reason = refusal
        .lines()
        .find(|l| l.contains("different capability snapshots"))
        .and_then(|l| l.strip_prefix("lute compile --all: "))
        .unwrap_or_else(|| panic!("compile --all names the mismatch: {refusal}"));
    assert!(out.contains(reason), "{out}\n---\n{reason}");
}

// ---- T3-9: `lute check <file>` finds its project ---------------------------

#[test]
fn check_on_a_file_applies_the_nearest_project_and_says_so() {
    let dir = temp_dir("discover");
    write(
        &dir,
        "lute.project.yaml",
        &format!("{MANIFEST}defaults:\n  uses: [world.schema.yaml]\n"),
    );
    write(
        &dir,
        "world.schema.yaml",
        "state:\n  run.mood: { type: string, default: neutral }\n",
    );
    write(
        &dir,
        "scenes/a.lute",
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n\
         ::set{ run.mood = \"happy\" }\n@narrator: hi.\n",
    );
    let o = lute(&["check", p(&dir.join("scenes/a.lute"))]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    let stderr = String::from_utf8_lossy(&o.stderr);
    assert!(
        stderr.contains("note: using project") && stderr.contains("lute.project.yaml"),
        "{stderr}"
    );

    // Outside any project: no note, and the undeclared path is still an error.
    let loose = temp_dir("loose");
    std::fs::copy(dir.join("scenes/a.lute"), loose.join("a.lute")).unwrap();
    let o = lute(&["check", p(&loose.join("a.lute"))]);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o));
    assert!(!String::from_utf8_lossy(&o.stderr).contains("note: using project"));
}
