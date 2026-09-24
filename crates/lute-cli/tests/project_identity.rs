//! 0.21.1 "no silent wrong answers" — project-level identity and snapshot
//! gates that `check`/`check-project` used to pass while `compile --all` or the
//! engine got it wrong:
//!
//! * T1-10 (lamplight F37): a tagged component line shared its `lineId` with
//!   another expansion of itself or with a host line (`E-DUP-LINE-CODE`).
//! * T1-9 (lamplight F38, ashen F39): the default `voiceKey` template has no
//!   `{prefix}`, so lines of different documents landed on one voice asset
//!   (`E-DUP-VOICEKEY`).
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

// ---- T1-10: post-expansion line identity -----------------------------------

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

fn assert_dup_line_code_in_check_and_check_project(dir: &Path) {
    let check = lute(&["check", p(&dir.join("scenes/a.lute"))]);
    assert_eq!(check.status.code(), Some(1), "{}", text(&check));
    assert!(text(&check).contains("E-DUP-LINE-CODE"), "{}", text(&check));
    let project = lute(&["check-project", p(dir)]);
    assert_eq!(project.status.code(), Some(1), "{}", text(&project));
    assert!(text(&project).contains("E-DUP-LINE-CODE"), "{}", text(&project));
}

#[test]
fn a_tagged_component_line_used_twice_collides_with_itself() {
    let dir = component_project(
        "use-twice",
        "@ann{code=\"0010\"}: comp line.",
        "@narrator: before.\n::use{component=\"c\"}\n::use{component=\"c\"}\n",
    );
    assert_dup_line_code_in_check_and_check_project(&dir);
}

#[test]
fn a_tagged_component_line_collides_with_the_host_line_it_shares_a_code_with() {
    let dir = component_project(
        "host-collision",
        "@ann{code=\"0010\"}: comp line.",
        "@ann{code=\"0010\"}: host line.\n::use{component=\"c\"}\n",
    );
    assert_dup_line_code_in_check_and_check_project(&dir);
}

/// Control: an UNTAGGED component line gets a fresh code at each use.
#[test]
fn an_untagged_component_line_used_twice_is_clean() {
    let dir = component_project(
        "untagged",
        "@ann: comp line.",
        "@ann{code=\"0010\"}: host line.\n::use{component=\"c\"}\n::use{component=\"c\"}\n",
    );
    let project = lute(&["check-project", p(&dir)]);
    assert_eq!(project.status.code(), Some(0), "{}", text(&project));
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

#[test]
fn two_documents_voicing_different_text_under_one_voice_key_fail_check_project_and_compile_all() {
    let dir = voice_project("voice-collide", MANIFEST, "Goodbye.");
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

/// Same text under one key is one recording, and a `{prefix}` template keeps
/// the two documents' keys apart.
#[test]
fn shared_text_or_a_prefixed_voice_key_template_is_clean() {
    let same = voice_project("voice-same-text", MANIFEST, "Hello.");
    let o = lute(&["check-project", p(&same)]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));

    let prefixed = voice_project(
        "voice-prefixed",
        &format!("{MANIFEST}identity:\n  voiceKey: \"{{prefix}}.{{speaker}}-{{code}}\"\n"),
        "Goodbye.",
    );
    let o = lute(&["check-project", p(&prefixed)]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    let out_dir = prefixed.join("out");
    let o = lute(&["compile", "--all", "--project", p(&prefixed), "-o", p(&out_dir)]);
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
        "directives:\n  - { name: announce, attrs: [ { name: text, type: string } ], \
         lower: { kind: builtin, name: noop } }\n",
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

    let all = lute(&["compile", "--all", "--project", p(&dir), "-o", p(&dir.join("out"))]);
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
