//! Project schema, manifest, defaults, and mock-surface contracts.

use super::*;

#[test]
fn compile_all_validates_nested_manifests() {
    let dir = temp_dir("nested-manifest-compile");
    let inner = dir.join("inner/scenes");
    std::fs::create_dir_all(&inner).unwrap();
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::write(
        dir.join("lute.project.yaml"),
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("inner/lute.project.yaml"),
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n\
         identity:\n  lineId: \"{prefix}.{bogus}_{code}\"\n",
    )
    .unwrap();
    let scene = "---\nkind: scene\ncharacter: a\nseason: 1\nepisode: 1\n---\n\n## S\n\n@a: hi\n";
    std::fs::write(dir.join("scenes/outer.lute"), scene).unwrap();
    std::fs::write(inner.join("in.lute"), scene).unwrap();

    let out = std::process::Command::new(BIN)
        .args([
            "compile",
            "--all",
            "--project",
            dir.to_str().unwrap(),
            "-o",
            dir.join("out").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        out.status.code(),
        Some(1),
        "nested manifest must fail the build:\n{text}"
    );
    assert!(
        text.contains("E-IDENTITY-TEMPLATE"),
        "the inner manifest's own diagnostic must surface:\n{text}"
    );
    assert!(
        text.contains("inner/lute.project.yaml"),
        "a manifest diagnostic MUST carry the manifest's path (§7):\n{text}"
    );
}

/// 0.10.0 §7: the same diagnostic from `check-project` must also carry the
/// manifest's path. At HEAD both the inner and the outer run printed a
/// byte-identical, pathless `lute: E-IDENTITY-TEMPLATE: …`.
#[test]
fn check_project_manifest_diagnostic_names_the_manifest() {
    let dir = temp_dir("nested-manifest-check");
    let inner = dir.join("inner/scenes");
    std::fs::create_dir_all(&inner).unwrap();
    std::fs::write(
        dir.join("lute.project.yaml"),
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("inner/lute.project.yaml"),
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n\
         identity:\n  lineId: \"{prefix}.{bogus}_{code}\"\n",
    )
    .unwrap();
    std::fs::write(
        inner.join("in.lute"),
        "---\nkind: scene\ncharacter: b\nseason: 1\nepisode: 1\n---\n\n## S\n\n@b: hi\n",
    )
    .unwrap();

    let out = std::process::Command::new(BIN)
        .args(["check-project", dir.to_str().unwrap()])
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code(), Some(1), "{text}");
    assert!(
        text.contains("inner/lute.project.yaml") && text.contains("E-IDENTITY-TEMPLATE"),
        "the diagnostic must name the manifest it is about:\n{text}"
    );
}

/// 0.10.0 §7: a manifest that fails to LOAD must fail the build. At HEAD a
/// malformed governing manifest printed a `lute:` line and still exited 0,
/// and one in a directory holding no `.lute` files was never opened at all.
#[test]
fn check_project_fails_on_an_unloadable_manifest_anywhere_under_the_tree() {
    let dir = temp_dir("broken-manifest");
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::create_dir_all(dir.join("empty")).unwrap();
    std::fs::write(
        dir.join("lute.project.yaml"),
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n",
    )
    .unwrap();
    // No `.lute` lives under `empty/`, so nothing ever resolved this manifest.
    std::fs::write(
        dir.join("empty/lute.project.yaml"),
        "defaultProfile: core\nprofiles: [not, a, map]\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("scenes/s.lute"),
        "---\nkind: scene\ncharacter: a\nseason: 1\nepisode: 1\n---\n\n## S\n\n@a: hi\n",
    )
    .unwrap();

    let out = std::process::Command::new(BIN)
        .args(["check-project", dir.to_str().unwrap()])
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        out.status.code(),
        Some(1),
        "an unloadable manifest must fail the build:\n{text}"
    );
    assert!(
        text.contains("empty/lute.project.yaml"),
        "the manifest nothing resolves must still be validated:\n{text}"
    );
}

/// D-Z, prophylactic: a canonical `defaults:` path is absolute and
/// machine-specific and MUST NOT reach any serialised surface — not the
/// capability snapshot, not `capabilityVersion`, not a compiled artifact,
/// not `lute context --json`. Pre-resolution is for resolution only.
#[test]
fn a_canonical_defaults_path_never_reaches_a_serialised_surface() {
    let dir = temp_dir("defaults-no-leak");
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::write(
        dir.join("lute.project.yaml"),
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n\
         defaults:\n  uses: [world.schema.yaml]\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("world.schema.yaml"),
        "state:\n  run.k: { type: int, default: 0 }\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("scenes/s.lute"),
        "---\nkind: scene\ncharacter: a\nseason: 1\nepisode: 1\n---\n\n## S\n\n::set{ run.k = 1 }\n@a: hi\n",
    )
    .unwrap();
    // The canonical form of the project dir is what the loader stores, and on
    // macOS it differs from the authored one (/tmp -> /private/tmp), so this
    // needle is sharp.
    let canonical = std::fs::canonicalize(&dir).unwrap().display().to_string();

    for args in [
        vec![
            "context",
            dir.join("scenes/s.lute").to_str().unwrap(),
            "--json",
            "--project",
            dir.to_str().unwrap(),
        ],
        vec![
            "compile",
            dir.join("scenes/s.lute").to_str().unwrap(),
            "--json",
            "--project",
            dir.to_str().unwrap(),
        ],
    ] {
        let out = std::process::Command::new(BIN)
            .args(&args)
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout).to_string();
        assert!(
            !text.contains(&canonical),
            "`{}` leaked a canonical defaults path into a serialised surface (D-Z):\n{text}",
            args[0]
        );
    }

    // And the stamp itself: the SAME project resolved from two different
    // absolute locations must compute the SAME capabilityVersion.
    let twin = temp_dir("defaults-no-leak-twin");
    std::fs::create_dir_all(twin.join("scenes")).unwrap();
    for f in ["lute.project.yaml", "world.schema.yaml", "scenes/s.lute"] {
        std::fs::copy(dir.join(f), twin.join(f)).unwrap();
    }
    let version = |root: &std::path::Path| {
        let out = std::process::Command::new(BIN)
            .args([
                "context",
                root.join("scenes/s.lute").to_str().unwrap(),
                "--json",
                "--project",
                root.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        v["capabilityVersion"].as_str().unwrap().to_string()
    };
    assert_eq!(
        version(&dir),
        version(&twin),
        "capabilityVersion must not depend on where the project lives on disk (D-Z)"
    );
}

/// 0.10.0 §6.2: the resolved frontmatter — authored keys plus defaults — is
/// what required-key checking sees. A scene omitting `character:` under a
/// root that defaults it is COMPLETE, not E-META-MISSING.
#[test]
fn defaults_satisfy_required_frontmatter_keys() {
    let dir = temp_dir("defaults-required");
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::write(
        dir.join("lute.project.yaml"),
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n\
         defaults:\n  kind: scene\n  character: haven\n  season: 1\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("scenes/s.lute"),
        "---\nepisode: 4\n---\n\n## S\n\n@haven: hi\n",
    )
    .unwrap();
    let out = std::process::Command::new(BIN)
        .args(["check-project", dir.to_str().unwrap()])
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code(), Some(0), "{text}");
    assert!(!text.contains("E-META-MISSING"), "{text}");
    assert!(
        !text.contains("E-KIND-MISSING"),
        "`kind:` is defaultable (§6.3):\n{text}"
    );
}

/// §6.2: whole-value override, no merging. A document declaring `uses:`
/// resolves to ITS list entire — not to the union with the default's.
#[test]
fn a_documents_own_key_overrides_the_default_entire() {
    let dir = temp_dir("defaults-override");
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::write(
        dir.join("lute.project.yaml"),
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n\
         defaults:\n  uses: [world.schema.yaml]\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("world.schema.yaml"),
        "state:\n  run.world: { type: int, default: 0 }\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("wake.schema.yaml"),
        "state:\n  run.wake: { type: int, default: 0 }\n",
    )
    .unwrap();
    // Declares its own `uses:`, so it gets `run.wake` and NOT `run.world`.
    std::fs::write(
        dir.join("scenes/own.lute"),
        "---\nkind: scene\ncharacter: a\nseason: 1\nepisode: 1\nuses: [../wake.schema.yaml]\n---\n\
         \n## S\n\n::set{ run.world = 1 }\n@a: hi\n",
    )
    .unwrap();
    let out = std::process::Command::new(BIN)
        .args(["check-project", dir.to_str().unwrap()])
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        out.status.code(),
        Some(1),
        "concatenation would have made this green:\n{text}"
    );
    assert!(text.contains("E-UNDECLARED"), "{text}");
}

/// §6.2: present-but-empty counts as present. `uses: []` means NO imports,
/// not "inherit". There is no "unset me" spelling and none is added.
#[test]
fn an_empty_authored_value_overrides_the_default() {
    let dir = temp_dir("defaults-empty");
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::write(
        dir.join("lute.project.yaml"),
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n\
         defaults:\n  uses: [world.schema.yaml]\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("world.schema.yaml"),
        "state:\n  run.world: { type: int, default: 0 }\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("scenes/empty.lute"),
        "---\nkind: scene\ncharacter: a\nseason: 1\nepisode: 1\nuses: []\n---\n\
         \n## S\n\n::set{ run.world = 1 }\n@a: hi\n",
    )
    .unwrap();
    let out = std::process::Command::new(BIN)
        .args(["check-project", dir.to_str().unwrap()])
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        out.status.code(),
        Some(1),
        "`uses: []` means no imports:\n{text}"
    );
    assert!(text.contains("E-UNDECLARED"), "{text}");
}

/// §6.3: a default whose key is not legal on a document's RESOLVED kind is
/// not applied to that document, and that is not an error. Without this rule
/// `defaults:` is unusable in any root holding more than one kind of
/// document — which is every real root.
#[test]
fn a_scene_only_default_does_not_reach_a_quest() {
    let dir = temp_dir("defaults-mixed-root");
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::create_dir_all(dir.join("quests")).unwrap();
    std::fs::write(
        dir.join("lute.project.yaml"),
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n\
         defaults:\n  character: haven\n  season: 1\n  episode: 1\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("scenes/s.lute"),
        "---\nkind: scene\n---\n\n## S\n\n@haven: hi\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("quests/q.lute"),
        "---\nkind: quest\n---\n\n<quest id=\"q\" title=\"Q\" start=\"true\">\n\
         \x20 <objective id=\"o\" title=\"O\" done=\"true\"/>\n</quest>\n",
    )
    .unwrap();
    let out = std::process::Command::new(BIN)
        .args(["check-project", dir.to_str().unwrap()])
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code(), Some(0), "{text}");
    assert!(
        !text.contains("E-META-UNKNOWN-KEY"),
        "a quest under a root defaulting `character:` simply does not receive it (§6.3):\n{text}"
    );
}

/// §6.4: defaults resolve BEFORE identity rendering, so a defaulted
/// `character:` participates in `{prefix}` identically to an authored one —
/// #34's own motivation, the manifest that consumes these keys can now
/// supply them.
#[test]
fn a_defaulted_character_reaches_the_line_id_prefix() {
    let dir = temp_dir("defaults-identity");
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::write(
        dir.join("lute.project.yaml"),
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n\
         defaults:\n  kind: scene\n  character: haven\n  season: 1\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("scenes/s.lute"),
        "---\nepisode: 7\n---\n\n## S\n\n@vesna: hi\n",
    )
    .unwrap();
    let out = std::process::Command::new(BIN)
        .args([
            "compile",
            dir.join("scenes/s.lute").to_str().unwrap(),
            "--json",
            "--project",
            dir.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        text.contains("haven.s01ep07.vesna_0010"),
        "the defaulted character/season must reach {{prefix}} (§6.4):\n{text}"
    );
}

/// 0.10.0 §8 / §13.5: the reds are SEQUENTIAL. Run 1 reports only
/// `E-MOCK-SUBJECT` — with no subject there is no resolved schema, so the
/// schema rules have nothing to decide and are suppressed. Run 2, after the
/// author adds `file:`, surfaces the undeclared seed.
#[test]
fn a_subject_less_mock_is_reported_once_then_its_seed_is_caught() {
    let dir = temp_dir("mock-pass-ordering");
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::create_dir_all(dir.join("mocks")).unwrap();
    std::fs::write(
        dir.join("lute.project.yaml"),
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("world.schema.yaml"),
        "state:\n  run.pressure: { type: int, default: 0 }\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("scenes/s.lute"),
        "---\nkind: scene\ncharacter: a\nseason: 1\nepisode: 1\nuses: [../world.schema.yaml]\n---\n\
         \n## S\n\n::set{ run.pressure = 1 }\n@a: hi\n",
    )
    .unwrap();
    let mock = dir.join("mocks/play.yaml");
    // Run 1: no `file:`, and a seed that is ALSO wrong. Only the subject is
    // reported.
    std::fs::write(&mock, "state:\n  run.greeted: false\n").unwrap();
    let run = || {
        let out = std::process::Command::new(BIN)
            .args(["check-project", dir.to_str().unwrap()])
            .output()
            .unwrap();
        (
            out.status.code(),
            format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ),
        )
    };
    let (code, text) = run();
    assert_eq!(code, Some(1), "{text}");
    assert!(text.contains("E-MOCK-SUBJECT"), "{text}");
    assert!(
        !text.contains("E-TRACE-MOCK-UNDECLARED"),
        "E-MOCK-SUBJECT suppresses the schema rules for that file (§8):\n{text}"
    );
    assert!(
        text.contains("mocks/play.yaml") && !text.contains("mocks/play.yaml:0:0"),
        "anchored at the mock, with no fabricated position (D-AB):\n{text}"
    );

    // Run 2: the author supplies `file:`. The schema is now knowable and the
    // seed is caught.
    std::fs::write(
        &mock,
        "file: ../scenes/s.lute\nstate:\n  run.greeted: false\n",
    )
    .unwrap();
    let (code, text) = run();
    assert_eq!(code, Some(1), "{text}");
    assert!(!text.contains("E-MOCK-SUBJECT"), "{text}");
    assert!(text.contains("E-TRACE-MOCK-UNDECLARED"), "{text}");
    assert!(
        text.contains("mocks/play.yaml"),
        "still anchored at the mock:\n{text}"
    );

    // Run 3: the author corrects the seed. Green.
    std::fs::write(&mock, "file: ../scenes/s.lute\nstate:\n  run.pressure: 1\n").unwrap();
    let (code, text) = run();
    assert_eq!(code, Some(0), "{text}");
}

/// §8: a `file:` naming a path that does not exist is `E-MOCK-SUBJECT` —
/// D-E's "a mock naming a deleted scene", which was uncheckable while the
/// Haven mock named its subject in a YAML comment.
#[test]
fn a_mock_naming_a_deleted_scene_is_e_mock_subject() {
    let dir = temp_dir("mock-deleted-subject");
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::create_dir_all(dir.join("mocks")).unwrap();
    std::fs::write(
        dir.join("lute.project.yaml"),
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("scenes/s.lute"),
        "---\nkind: scene\ncharacter: a\nseason: 1\nepisode: 1\n---\n\n## S\n\n@a: hi\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("mocks/play.yaml"),
        "file: ../scenes/opening.lute\n",
    )
    .unwrap();
    let out = std::process::Command::new(BIN)
        .args(["check-project", dir.to_str().unwrap()])
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code(), Some(1), "{text}");
    assert!(
        text.contains("E-MOCK-SUBJECT") && text.contains("opening.lute"),
        "{text}"
    );
}

/// §8's glob boundary is CHOSEN, not incidental. A `*.test.yaml` already
/// carries a required `file:` read by the same parser and is already
/// validated when `lute test` runs it; a `conformance/*/mock.yaml` is a
/// fixture whose whole purpose is to pin behaviour, including behaviour this
/// pass would call wrong.
#[test]
fn the_pass_reaches_mocks_yaml_and_nothing_else() {
    let dir = temp_dir("mock-pass-glob");
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::create_dir_all(dir.join("mocks")).unwrap();
    std::fs::create_dir_all(dir.join("conformance/fx")).unwrap();
    std::fs::write(
        dir.join("lute.project.yaml"),
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("scenes/s.lute"),
        "---\nkind: scene\ncharacter: a\nseason: 1\nepisode: 1\n---\n\n## S\n\n@a: hi\n",
    )
    .unwrap();
    // Neither of these is `mocks/*.yaml`; neither may be reported.
    std::fs::write(
        dir.join("conformance/fx/mock.yaml"),
        "choose:\n  path: left\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("mocks/a.test.yaml"),
        "file: ../scenes/s.lute\nexpect: {}\n",
    )
    .unwrap();
    // This one is, and it is sound.
    std::fs::write(dir.join("mocks/ok.yaml"), "file: ../scenes/s.lute\n").unwrap();

    let out = std::process::Command::new(BIN)
        .args(["check-project", dir.to_str().unwrap()])
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code(), Some(0), "{text}");
    assert!(!text.contains("E-MOCK-SUBJECT"), "{text}");
}

/// T3.10 / #2(a) / D-B, extended to the mock family (0.10.0 §8): a
/// `mocks/*.yaml` mis-keying a surface is an ERROR from `check-project`, not
/// a silent drop. Before this, the identical file was `ok: … (N file(s), 0
/// project-wide warning(s))` at exit 0, byte-identical to a tree with no
/// mock in it at all.
#[test]
fn a_mis_keyed_mock_surface_is_reported_by_check_project() {
    let dir = temp_dir("mock-closed-keys");
    write(
        &dir,
        "lute.project.yaml",
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n",
    );
    write(
        &dir,
        "world.schema.yaml",
        "state:\n  run.pressure: { type: int, default: 0 }\n",
    );
    write(
        &dir,
        "scenes/s.lute",
        "---\nkind: scene\ncharacter: a\nseason: 1\nepisode: 1\nuses: [../world.schema.yaml]\n---\n\
         \n## S\n\n::set{ run.pressure = 1 }\n@a: hi\n",
    );
    std::fs::create_dir_all(dir.join("mocks")).unwrap();
    let mock = dir.join("mocks/play.yaml");

    // The good file first: this is what makes the red below mean something.
    // A gate that reddened every mock would pass the assertions after it.
    std::fs::write(&mock, "file: ../scenes/s.lute\nstate:\n  run.pressure: 2\n").unwrap();
    let out = run(&["check-project", dir.to_str().unwrap()]);
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "a well-keyed mock stays green:\n{text}"
    );

    // Now mis-key exactly one surface.
    std::fs::write(&mock, "file: ../scenes/s.lute\nselections:\n  h: a\n").unwrap();
    let out = run(&["check-project", dir.to_str().unwrap()]);
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code(), Some(1), "{text}");
    assert!(
        text.contains("E-TRACE-MOCK-PARSE") && text.contains("`selections`"),
        "{text}"
    );
    assert!(
        text.contains("mocks/play.yaml") && !text.contains("mocks/play.yaml:0:0"),
        "anchored at the mock, with no fabricated position (D-AB):\n{text}"
    );
}
