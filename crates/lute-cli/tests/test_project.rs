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
    let dir = std::env::temp_dir().join(format!(
        "lute-test-project-{tag}-{}-{n}",
        std::process::id()
    ));
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
    write_at(
        dir,
        "world.schema.yaml",
        "state:\n  run.mood: { type: string, default: neutral }\n",
    );
    write_at(
        dir,
        "plugins/demo.plugin/plugin.yaml",
        "id: demo.plugin\nversion: 0.1.0\nkind: capability\nexports:\n  directives: directives/\n",
    );
    write_at(
        dir,
        "plugins/demo.plugin/directives/d.yaml",
        "directives:\n  - { name: announce, attrs: [ { name: text, type: string } ] }\n",
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
        .args([
            "test",
            dir.join("tests").to_str().unwrap(),
            "--project",
            dir.to_str().unwrap(),
        ])
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

/// 0.23.1 (lamplight N7): the IDENTICAL fixture and test file, run WITHOUT
/// `--project`, resolves the document against the nearest `lute.project.yaml`
/// — as `lute check <file>` and this run's plays do — and says so on stderr.
/// Before, the manifest was never consulted and the test failed with
/// `E-UNKNOWN-DIRECTIVE` / `E-UNDECLARED` that looked like content bugs.
#[test]
fn same_test_yaml_without_project_resolves_the_nearest_manifest() {
    let dir = temp_dir("control");
    write_manifest_dependent_project(&dir);

    let out = Command::new(BIN)
        .args(["test", dir.join("tests").to_str().unwrap()])
        .output()
        .expect("run lute");
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert_eq!(out.status.code(), Some(0), "stdout: {text}\nstderr: {stderr}");
    assert!(text.contains("1 passed, 0 failed"), "{text}");
    assert!(stderr.contains("nearest lute.project.yaml"), "{stderr}");
}

/// Plugin §10's documented precedence — an explicit `--providers <dir>` wins over the
/// project's auto-discovered pinned catalog — must hold through `lute test` exactly as it
/// does through `lute check`/`lute trace`. `arcia-project`'s own catalog resolves
/// `marina_service_01` Fresh (clean trace); pointing `--providers` at an EMPTY directory
/// instead must make the SAME id Absent (`E-UNKNOWN-ID`), proving the flag overrode
/// auto-discovery rather than composing with it.
#[test]
fn explicit_providers_flag_wins_over_the_projects_pinned_catalog() {
    let arcia = std::fs::canonicalize("../../docs/examples/arcia-project").unwrap();
    let scene = arcia.join("date-minigame.lute");

    let dir = temp_dir("providers-precedence");
    write_at(
        &dir,
        "tests/t.test.yaml",
        // dsl 0.24.0 §5: an unanswered bridge result reads unknown, so the
        // `rank` match would halt the trace incomplete — answer the call.
        &format!(
            "file: {}\nbridges:\n  minigame:\n    - {{ score: 90, rank: gold, cleared: true }}\n\
             expect:\n  exit: complete\n",
            scene.display()
        ),
    );
    let empty_providers = temp_dir("providers-precedence-empty");

    let auto = Command::new(BIN)
        .args([
            "test",
            dir.join("tests").to_str().unwrap(),
            "--project",
            arcia.to_str().unwrap(),
        ])
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
            arcia.to_str().unwrap(),
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
    write_at(
        &dir,
        "tests/t.test.yaml",
        "file: ../scene.lute\nexpect:\n  exit: complete\n",
    );

    let out = Command::new(BIN)
        .args([
            "test",
            dir.join("tests").to_str().unwrap(),
            "--project",
            dir.to_str().unwrap(),
        ])
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

// ── 0.21.0 §7a.4: `expect.quests`, and the `visited:` / `occasions:` keys ──

/// A shape-only project whose `holdLine` completes only when `haven.shed` is
/// visited AND `runEnd` is raised; `sideJob` is accept-driven (no `start`)
/// and the shed scene's `take` arm accepts it. Writes `tests/t.test.yaml` =
/// `test_yaml` and runs `lute test tests/ --project <dir>` (plus `extra`),
/// returning `(exit code, stdout)`.
fn run_quest_test(tag: &str, test_yaml: &str, extra: &[&str]) -> (Option<i32>, String) {
    let dir = temp_dir(tag);
    write_at(
        &dir,
        "lute.project.yaml",
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n",
    );
    write_at(
        &dir,
        "world.schema.yaml",
        "state:\n  run.pressure: { type: number, default: 0 }\n",
    );
    write_at(
        &dir,
        "quests/hold.lute",
        "---\nkind: quest\nuses: ../world.schema.yaml\ntitle: Hold\n---\n\n\
         <quest id=\"holdLine\" title=\"Hold the line\" start=\"true\">\n\
         <objective id=\"sawShed\" title=\"See the shed\" done=\"visited('haven.shed')\"/>\n\
         <objective id=\"calm\" title=\"Keep calm\" on=\"runEnd\" done=\"run.pressure < 2\"/>\n\
         </quest>\n\n\
         <quest id=\"sideJob\" title=\"Side job\">\n\
         <objective id=\"paid\" title=\"Get paid\" done=\"run.pressure > 5\"/>\n\
         </quest>\n",
    );
    write_at(
        &dir,
        "scenes/shed.lute",
        "---\nkind: scene\nid: haven.shed\nuses: ../world.schema.yaml\n---\n\n\
         ## Shed\n\n@guard: The shed is quiet.\n\n<branch id=\"offer\">\n\
         <choice id=\"take\" label=\"Take the job\">\n@guard: Deal.\n\
         ::accept{quest=\"sideJob\"}\n</choice>\n\
         <choice id=\"pass\" label=\"Pass\">\n@guard: Suit yourself.\n</choice>\n</branch>\n",
    );
    write_at(&dir, "tests/t.test.yaml", test_yaml);
    let out = Command::new(BIN)
        .args([
            "test",
            dir.join("tests").to_str().unwrap(),
            "--project",
            dir.to_str().unwrap(),
        ])
        .args(extra)
        .output()
        .expect("run lute");
    (
        out.status.code(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    )
}

#[test]
fn expect_quests_passes_when_visited_and_occasions_drive_the_lifecycle() {
    let (code, text) = run_quest_test(
        "quests-pass",
        "file: ../quests/hold.lute\nvisited: [haven.shed]\noccasions: [runEnd]\n\
         accepts: [sideJob]\nexpect:\n  exit: complete\n  quests:\n    \
         holdLine: complete\n    sideJob: active\n",
        &[],
    );
    assert_eq!(code, Some(0), "{text}");
    assert!(text.contains("1 passed, 0 failed"), "{text}");

    // With neither key the same document leaves both quests short of that.
    let (code, text) = run_quest_test(
        "quests-pass-bare",
        "file: ../quests/hold.lute\nexpect:\n  quests:\n    holdLine: active\n    sideJob: unset\n",
        &[],
    );
    assert_eq!(code, Some(0), "{text}");
}

#[test]
fn expect_quests_mismatch_fails_naming_expected_and_actual() {
    // Visited but `runEnd` never raised: `calm` is never judged.
    let yaml = "file: ../quests/hold.lute\nvisited: [haven.shed]\nexpect:\n  quests:\n    \
                holdLine: complete\n";
    let (code, text) = run_quest_test("quests-fail", yaml, &[]);
    assert_eq!(code, Some(1), "{text}");
    assert!(
        text.contains("quests holdLine: expected \"complete\", got \"active\""),
        "{text}"
    );
    assert!(text.contains("0 passed, 1 failed"), "{text}");

    let (code, text) = run_quest_test("quests-fail-json", yaml, &["--json"]);
    assert_eq!(code, Some(1), "{text}");
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let e = &v["tests"][0]["expectations"][0];
    assert_eq!(e["kind"], "quests", "{v}");
    assert_eq!(e["subject"], "holdLine");
    assert_eq!(e["expected"], "complete");
    assert_eq!(e["actual"], "active");
    assert_eq!(e["passed"], false);
}

#[test]
fn expect_quests_naming_an_undeclared_quest_fails() {
    let (code, text) = run_quest_test(
        "quests-undeclared",
        "file: ../quests/hold.lute\nexpect:\n  quests:\n    ghost: active\n",
        &[],
    );
    assert_eq!(code, Some(1), "{text}");
    assert!(
        text.contains(
            "quests ghost: expected \"active\", but the traced document declares no quest `ghost`"
        ),
        "{text}"
    );
}

#[test]
fn expect_quests_with_an_unknown_state_name_fails() {
    let (code, text) = run_quest_test(
        "quests-bad-state",
        "file: ../quests/hold.lute\nexpect:\n  quests:\n    holdLine: done\n",
        &[],
    );
    assert_eq!(code, Some(1), "{text}");
    assert!(
        text.contains(
            "quests holdLine: \"done\" is not a quest state \
             (expected one of: unset, active, complete, failed)"
        ),
        "{text}"
    );
}

/// The top-level key set is closed; `visited`/`occasions` are in it.
#[test]
fn a_misspelt_visited_key_is_refused_and_suggests_the_real_one() {
    let (code, text) = run_quest_test(
        "quests-key-typo",
        "file: ../quests/hold.lute\nvisitd: [haven.shed]\nexpect:\n  exit: complete\n",
        &[],
    );
    assert_eq!(code, Some(1), "{text}");
    assert!(
        text.contains("E-TEST-KEY") && text.contains("did you mean `visited`?"),
        "{text}"
    );
}

// ── 0.24.0 T3-5: `expect.accepts` — the quests a scene's `::accept` took ──

#[test]
fn expect_accepts_is_the_set_of_quests_the_walk_accepted() {
    let (code, text) = run_quest_test(
        "accepts-pass",
        "file: ../scenes/shed.lute\nchoose: { offer: take }\nexpect:\n  accepts: [sideJob]\n",
        &[],
    );
    assert_eq!(code, Some(0), "{text}");
    assert!(text.contains("1 passed, 0 failed"), "{text}");

    // The arm that skips `::accept` accepts nothing: the empty list passes…
    let (code, text) = run_quest_test(
        "accepts-empty",
        "file: ../scenes/shed.lute\nchoose: { offer: pass }\nexpect:\n  accepts: []\n",
        &[],
    );
    assert_eq!(code, Some(0), "{text}");

    // …and expecting the accept there fails, naming both sets.
    let (code, text) = run_quest_test(
        "accepts-fail",
        "file: ../scenes/shed.lute\nchoose: { offer: pass }\nexpect:\n  accepts: [sideJob]\n",
        &[],
    );
    assert_eq!(code, Some(1), "{text}");
    assert!(text.contains("accepts: expected [sideJob], got []"), "{text}");
    assert!(text.contains("0 passed, 1 failed"), "{text}");
}

// ── 0.24.0 T1-12: a component `{{@param}}` bound to a def whose name length
// differs from the param's renders whole in trace/test, as it does in play ──

#[test]
fn component_param_bound_to_a_longer_def_renders_in_trace_and_test() {
    let dir = temp_dir("component-param-span");
    write_at(
        &dir,
        "lute.project.yaml",
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n\
         defaults:\n  uses: [world.schema.yaml]\n  components: [components/gauge.component.lute]\n",
    );
    write_at(
        &dir,
        "world.schema.yaml",
        "state:\n  user.bond: { type: number, default: 2 }\n\
         defs:\n  bondTimesTen: \"user.bond * 10\"\n  b: \"user.bond\"\n",
    );
    write_at(
        &dir,
        "components/gauge.component.lute",
        "---\ncomponent: gauge\nparams:\n  fathoms: number\n  marks: number\n---\n\n## Gauge\n\n\
         @narrator: The gauge shows {{@fathoms}} fathoms, {{@marks}} marks, done.\n",
    );
    write_at(
        &dir,
        "scenes/g.lute",
        "---\nkind: scene\nid: probe.g\ntitle: G\n---\n\n## G\n\n\
         ::use{component=\"gauge\" fathoms=@bondTimesTen marks=@b}\n\
         ::use{component=\"gauge\" fathoms=@b marks=@bondTimesTen}\n",
    );
    let trace = Command::new(BIN)
        .args(["trace", dir.join("scenes/g.lute").to_str().unwrap()])
        .args(["--project", dir.to_str().unwrap()])
        .output()
        .expect("run lute");
    let text = String::from_utf8_lossy(&trace.stdout);
    assert_eq!(trace.status.code(), Some(0), "{text}");
    // Longer def (`bondTimesTen` > `fathoms`) and shorter def (`b`): both the
    // rebound interp and the FOLLOWING one resolve.
    assert!(
        text.contains("The gauge shows 20 fathoms, 2 marks, done."),
        "{text}"
    );
    assert!(
        text.contains("The gauge shows 2 fathoms, 20 marks, done."),
        "{text}"
    );

    write_at(
        &dir,
        "tests/t.test.yaml",
        "file: ../scenes/g.lute\nexpect:\n  transcriptContains:\n    \
         - \"shows 20 fathoms, 2 marks, done.\"\n    - \"shows 2 fathoms, 20 marks\"\n",
    );
    let out = Command::new(BIN)
        .args(["test", dir.join("tests").to_str().unwrap()])
        .args(["--project", dir.to_str().unwrap()])
        .output()
        .expect("run lute");
    let text = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "{text}");
    assert!(text.contains("1 passed, 0 failed"), "{text}");
}
