//! `lute doctor` (dsl 0.22.0 §13): the checklist reports what the project's
//! documents actually activate — plugins, occasions and the beats answering
//! them, play scripts, scenario tests — and warns when the `lute-lsp` an
//! editor would launch from `PATH` is not this toolchain's version.

use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-doctor-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_at(dir: &Path, rel: &str, content: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

/// A project activating one occasions plugin: `hubVisit` answered by a scene
/// beat and a lore entry beat, `talk` answered by nothing.
fn occasions_project(tag: &str) -> PathBuf {
    let proj = temp_dir(tag);
    write_at(
        &proj,
        "lute.project.yaml",
        "pluginsDir: plugins/\ndefaultProfile: game\nprofiles:\n  game:\n    plugins: { demo.occasions: true }\n",
    );
    write_at(
        &proj,
        "plugins/demo.occasions/plugin.yaml",
        "id: demo.occasions\nversion: 0.3.0\nkind: capability\ndepends: [ { id: lute.core, range: \"^0.0.1\" } ]\nexports:\n  occasions: occasions/\n",
    );
    write_at(
        &proj,
        "plugins/demo.occasions/occasions/game.yaml",
        "occasions:\n  hubVisit: { select: first }\n  talk: { select: first, target: true }\n",
    );
    write_at(
        &proj,
        "scenes/welcome.lute",
        "---\nkind: scene\nid: hub.welcome\non: hubVisit\n---\n\n## Welcome\n\n@narrator: Welcome back.\n",
    );
    write_at(
        &proj,
        "lore/barks.lute",
        "---\nkind: lore\nid: lore.barks\n---\n\n<entry id=\"idle\" on=\"hubVisit\" category=\"bark\">\n  @narrator: The fire crackles.\n</entry>\n",
    );
    write_at(&proj, "plays/first.play.yaml", "steps:\n  - occasion: hubVisit\n");
    write_at(&proj, "tests/welcome.test.yaml", "file: ../scenes/welcome.lute\n");
    proj
}

fn doctor(dir: &Path, path_env: &Path) -> String {
    let out = Command::new(BIN)
        .arg("doctor")
        .arg(dir)
        .env("PATH", path_env)
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    String::from_utf8_lossy(&out.stdout).to_string()
}

/// The checklist line whose label is `label` (`  <mark> <label>: <detail>`).
fn line<'a>(text: &'a str, label: &str) -> &'a str {
    text.lines()
        .find(|l| l.contains(&format!(" {label}: ")))
        .unwrap_or_else(|| panic!("no `{label}` line:\n{text}"))
}

#[test]
fn doctor_reports_plugins_occasions_plays_and_tests() {
    let proj = occasions_project("occasions");
    let empty_path = temp_dir("occasions-path");
    let text = doctor(&proj, &empty_path);

    assert_eq!(
        line(&text, "active plugins").trim(),
        "• active plugins: demo.occasions 0.3.0",
        "{text}"
    );
    assert_eq!(
        line(&text, "occasions (beats answering)").trim(),
        "• occasions (beats answering): 2 declared, 2 beat(s) — hubVisit (2), talk (0)",
        "{text}"
    );
    assert!(line(&text, "play scripts").ends_with("1 `*.play.yaml`"), "{text}");
    assert!(line(&text, "scenario tests").ends_with("1 `*.test.yaml`"), "{text}");
    // A project without a pinned catalog is not "core-only": it has a plugin.
    assert!(
        line(&text, "provider snapshots").ends_with("no pinned provider snapshots"),
        "{text}"
    );
    assert!(!text.contains("core-only project"), "{text}");
}

/// dsl 0.23.0 §4: a lore `<beat>` bundle answers its occasion like an entry
/// beat, so doctor counts it.
#[test]
fn doctor_counts_bundle_beats_answering_an_occasion() {
    let proj = occasions_project("bundle");
    write_at(
        &proj,
        "lore/talks.lute",
        "---\nkind: lore\nid: lore.talks\n---\n\n<beat id=\"greet\" on=\"talk\" target=\"npc.mara\">\n  @mara: Hi.\n</beat>\n",
    );
    let text = doctor(&proj, &temp_dir("bundle-path"));
    assert_eq!(
        line(&text, "occasions (beats answering)").trim(),
        "• occasions (beats answering): 2 declared, 3 beat(s) — hubVisit (2), talk (1)",
        "{text}"
    );
}

/// Write an executable `lute-lsp` into a fresh directory that prints `stdout`
/// — standing in for whatever build an editor would find on `PATH`.
#[cfg(unix)]
fn fake_lsp(tag: &str, stdout: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let dir = temp_dir(tag);
    let exe = dir.join("lute-lsp");
    std::fs::write(&exe, format!("#!/bin/sh\nprintf '{stdout}'\n")).unwrap();
    std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
    dir
}

#[cfg(unix)]
#[test]
fn doctor_flags_a_lute_lsp_whose_version_differs() {
    let proj = occasions_project("lsp");
    let ours = env!("CARGO_PKG_VERSION");

    let stale = fake_lsp("lsp-stale", "lute-lsp 0.17.1\\n");
    let text = doctor(&proj, &stale);
    let l = line(&text, "lute-lsp on PATH");
    assert!(
        l.contains('✗') && l.contains("0.17.1") && l.contains(&format!("differs from lute {ours}")),
        "{text}"
    );

    // A pre-0.22 server prints nothing for `--version`.
    let silent = fake_lsp("lsp-silent", "");
    let text = doctor(&proj, &silent);
    let l = line(&text, "lute-lsp on PATH");
    assert!(l.contains('✗') && l.contains("reports no version"), "{text}");

    let current = fake_lsp("lsp-current", &format!("lute-lsp {ours}\\n"));
    let text = doctor(&proj, &current);
    let l = line(&text, "lute-lsp on PATH");
    assert!(l.contains('✓') && l.contains(ours), "{text}");
}

/// seven-days F27: the editor's `lute-lsp` keeps running the build it was
/// started from. Doctor lists running servers and flags one whose binary
/// reports another version or was replaced after it started.
///
/// macOS only: the fake server is a shell script, and on Linux the process
/// table (and `/proc/<pid>/exe`) names the interpreter, not the script — a
/// real `lute-lsp` is a native binary, so that is an artefact of the fake.
#[cfg(target_os = "macos")]
#[test]
fn doctor_flags_a_running_lute_lsp_of_another_build() {
    use std::os::unix::fs::PermissionsExt;
    let proj = occasions_project("running");
    let ours = env!("CARGO_PKG_VERSION");
    let dir = temp_dir("running-lsp");
    let exe = dir.join("lute-lsp");
    let script = |version: &str| {
        format!("#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'lute-lsp {version}'; exit 0; fi\nsleep 30\n")
    };
    std::fs::write(&exe, script(ours)).unwrap();
    std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut server = Command::new(&exe).spawn().unwrap();
    let pid = format!("pid {} ({})", server.id(), exe.display());
    std::thread::sleep(std::time::Duration::from_millis(300));

    let running = |text: &str| {
        line(text, "running lute-lsp")
            .split("; ")
            .find(|part| part.contains(&pid))
            .map(str::to_string)
            .unwrap_or_else(|| panic!("{pid} not listed:\n{text}"))
    };
    let text = doctor(&proj, &temp_dir("running-path"));
    assert_eq!(running(&text).rsplit(": ").next().unwrap(), format!("{pid} {ours}"), "{text}");

    // Reinstalled over the running server: the file on disk is newer.
    std::thread::sleep(std::time::Duration::from_millis(2500));
    std::fs::write(&exe, script(ours)).unwrap();
    let text = doctor(&proj, &temp_dir("running-path2"));
    let l = line(&text, "running lute-lsp");
    assert!(l.contains('✗'), "{text}");
    assert!(running(&text).contains("started before its binary was replaced"), "{text}");
    assert!(text.contains("restart the editor"), "{text}");

    let _ = server.kill();
    let _ = server.wait();
}
