//! `lute test --project <dir>` (this release's fix): before it, `testcmd::run_one_test`
//! called `crate::build_input(&lute_path, providers, None)` with the project argument
//! HARDCODED `None` — no `--project` flag existed at all — so a document whose schema
//! depends on the manifest's `defaults: uses:` hoist, or whose directives depend on a
//! `profile:`-activated plugin, failed EVERY test with `E-DOMAIN-UNKNOWN` / `E-UNDECLARED`
//! / `E-UNKNOWN-DIRECTIVE` regardless of test content. `lute trace <doc> --project <dir>`
//! on the identical document walked clean.
//!
//! These tests build a fixture combining BOTH failure modes at once — a project
//! `defaults: uses:` hoist supplying `run.mood`'s declaration, and a `profile:`-activated
//! plugin supplying the `::announce` directive — since those are the two things that
//! actually broke.

use std::path::PathBuf;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-test-project-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_at(dir: &std::path::Path, rel: &str, content: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

/// A project manifest hoisting `world.schema.yaml` (`defaults: uses:`) and declaring two
/// profiles: `core` (no plugins — the manifest's own `defaultProfile`, so a document that
/// omits `profile:` still resolves core-only) and `withAnnounce` (activates `demo.plugin`,
/// which exports the `::announce` directive). `scene.lute` needs BOTH: `::announce` from
/// the plugin, and `run.mood` (declared only via the hoisted schema) from `::set`.
fn write_manifest_dependent_project(dir: &std::path::Path) {
    write_at(
        dir,
        "lute.project.yaml",
        "pluginsDir: plugins/\n\
         defaultProfile: core\n\
         profiles:\n\
         \x20\x20core:\n\
         \x20\x20\x20\x20plugins: {}\n\
         \x20\x20withAnnounce:\n\
         \x20\x20\x20\x20plugins: { demo.plugin: true }\n\
         defaults:\n\
         \x20\x20uses: [world.schema.yaml]\n",
    );
    write_at(dir, "world.schema.yaml", "state:\n  run.mood: { type: string, default: neutral }\n");
    write_at(
        dir,
        "plugins/demo.plugin/plugin.yaml",
        "id: demo.plugin\nversion: 0.1.0\nkind: capability\nexports:\n  directives: directives/\n",
    );
    write_at(
        dir,
        "plugins/demo.plugin/directives/d.yaml",
        "directives:\n  - { name: announce, attrs: [ { name: text, type: string } ], \
         lower: { kind: builtin, name: noop } }\n",
    );
    write_at(
        dir,
        "scene.lute",
        "---\nkind: scene\ncharacter: narrator\nseason: 1\nepisode: 1\nprofile: withAnnounce\n---\n\
         \n## Shot 1.\n\n::announce{text=\"Welcome.\"}\n::set{ run.mood = \"happy\" }\n\
         @narrator: Hello there.\n",
    );
    write_at(
        dir,
        "tests/t.test.yaml",
        "file: ../scene.lute\nexpect:\n  exit: complete\n  state:\n    run.mood: happy\n  \
         transcriptContains: [\"Hello there.\"]\n",
    );
}

/// The change under test: a `*.test.yaml` naming a document that needs the manifest
/// (both the `defaults: uses:` hoist AND a `profile:`-activated plugin) passes once
/// `--project <dir>` is supplied.
#[test]
fn test_yaml_needing_manifest_passes_under_project() {
    let dir = temp_dir("clean");
    write_manifest_dependent_project(&dir);

    let out = Command::new(BIN)
        .args(["test", dir.join("tests").to_str().unwrap(), "--project", dir.to_str().unwrap()])
        .output()
        .expect("run lute");
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert_eq!(
        out.status.code(),
        Some(0),
        "a project-resolved test must pass:\nstdout: {text}\nstderr: {stderr}"
    );
    assert!(text.contains("1 passed, 0 failed"), "{text}");
}

/// Positive control for the test above: the IDENTICAL fixture and test file, run WITHOUT
/// `--project`, must still fail — carrying the resolution diagnostics (`E-UNKNOWN-DIRECTIVE`
/// for the plugin directive, `E-UNDECLARED` for the hoisted state path) — proving `--project`
/// is what changed the outcome, not something else in the harness. "No output" is never
/// proof; this asserts the SPECIFIC codes the missing resolution produces.
#[test]
fn same_test_yaml_without_project_still_fails_with_resolution_diagnostics() {
    let dir = temp_dir("control");
    write_manifest_dependent_project(&dir);

    let out = Command::new(BIN)
        .args(["test", dir.join("tests").to_str().unwrap()])
        .output()
        .expect("run lute");
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert_eq!(
        out.status.code(),
        Some(1),
        "without --project the manifest is never resolved, so the test must fail: {text}"
    );
    assert!(
        text.contains("E-UNKNOWN-DIRECTIVE"),
        "the plugin-only `::announce` directive must be unresolved with no project: {text}"
    );
    assert!(
        text.contains("E-UNDECLARED"),
        "the hoisted-only `run.mood` state path must be undeclared with no project: {text}"
    );
    assert!(text.contains("0 passed, 1 failed"), "{text}");
}

/// Plugin §10's documented precedence — an explicit `--providers <dir>` wins over the
/// project's auto-discovered pinned catalog — must hold through `lute test` exactly as it
/// does through `lute check`/`lute trace`. `idola-project`'s own catalog resolves
/// `bianca_service_01` Fresh (clean trace); pointing `--providers` at an EMPTY directory
/// instead must make the SAME id Absent (`E-UNKNOWN-ID`), proving the flag overrode
/// auto-discovery rather than composing with it.
#[test]
fn explicit_providers_flag_wins_over_the_projects_pinned_catalog() {
    let idola = std::fs::canonicalize("../../docs/examples/idola-project").unwrap();
    let scene = idola.join("date-minigame.lute");

    let dir = temp_dir("providers-precedence");
    write_at(
        &dir,
        "tests/t.test.yaml",
        &format!("file: {}\nexpect:\n  exit: complete\n", scene.display()),
    );
    let empty_providers = temp_dir("providers-precedence-empty");

    let auto = Command::new(BIN)
        .args(["test", dir.join("tests").to_str().unwrap(), "--project", idola.to_str().unwrap()])
        .output()
        .expect("run lute");
    let auto_text = String::from_utf8_lossy(&auto.stdout).to_string();
    assert_eq!(
        auto.status.code(),
        Some(0),
        "the project's own pinned catalog resolves the id Fresh: {auto_text}"
    );

    let overridden = Command::new(BIN)
        .args([
            "test",
            dir.join("tests").to_str().unwrap(),
            "--project",
            idola.to_str().unwrap(),
            "--providers",
            empty_providers.to_str().unwrap(),
        ])
        .output()
        .expect("run lute");
    let overridden_text = String::from_utf8_lossy(&overridden.stdout).to_string();
    assert_eq!(
        overridden.status.code(),
        Some(1),
        "an explicit --providers must win over the project's catalog, making the SAME id \
         unresolvable: {overridden_text}"
    );
    assert!(
        overridden_text.contains("E-UNKNOWN-ID"),
        "the id must now resolve Absent against the empty explicit catalog: {overridden_text}"
    );
}

/// A project-resolution `E-` diagnostic (bad plugin activation — here, a profile activating
/// a plugin that is not installed) must gate the exit code, printed on the `lute:` stderr
/// channel via `report_project_diags` exactly as `check`/`trace` do — never silently folded
/// into a per-test pass/fail verdict where a broken manifest could be mistaken for a failing
/// assertion. Exit code read directly off `Command::output()` (never through a pipe, which
/// would report the pipe's own status instead).
#[test]
fn project_resolution_error_gates_the_exit_code() {
    let dir = temp_dir("resolve-error");
    write_at(
        &dir,
        "lute.project.yaml",
        "pluginsDir: plugins/\ndefaultProfile: broken\nprofiles:\n  broken:\n    \
         plugins: { nonexistent.plugin: true }\n",
    );
    write_at(
        &dir,
        "scene.lute",
        "---\nkind: scene\ncharacter: narrator\nseason: 1\nepisode: 1\n---\n\n## Shot 1.\n\n@narrator: hi\n",
    );
    write_at(&dir, "tests/t.test.yaml", "file: ../scene.lute\nexpect:\n  exit: complete\n");

    let out = Command::new(BIN)
        .args(["test", dir.join("tests").to_str().unwrap(), "--project", dir.to_str().unwrap()])
        .output()
        .expect("run lute");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert_eq!(
        out.status.code(),
        Some(1),
        "an E- project-resolution diagnostic must exit non-zero: stderr: {stderr}"
    );
    assert!(
        stderr.contains("E-PLUGIN-MISSING-ACTIVE"),
        "the resolve diagnostic must surface on the lute: stderr channel: {stderr}"
    );
}
