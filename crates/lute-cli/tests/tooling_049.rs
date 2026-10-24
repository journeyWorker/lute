//! 0.49.0 §7–§8: project checks, tooling and wording. #175 (a single-file
//! check names the quest references it left to whole-project `check`),
//! #187 (`asset-exists` selectors), #188 (`lint <DIR>` uses the project),
//! #160 / #161 (hints). Each project is the issue's repro. #167 is in
//! `check_project/late.rs`, next to the 0.28.0 test it narrows.

use std::path::Path;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

const MANIFEST: &str = "defaultProfile: game\nprofiles:\n  game:\n    plugins: {}\ndefaults:\n  contentLang: en\n  uses: [schema/world.schema.yaml]\n";
const SCHEMA: &str = "cast:\n  mira: { name: Mira }\n  theo: { name: Theo }\n";

/// A scratch project of `files` (path, text).
fn project(name: &str, files: &[(&str, &str)]) -> lute_scratch::ScratchDir {
    let dir = lute_scratch::ScratchDir::named(name);
    for (rel, text) in files {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().expect("file has a parent")).unwrap();
        std::fs::write(path, text).unwrap();
    }
    dir
}

/// Exit code and stdout+stderr of `lute <args>` run in `dir`.
fn run(dir: &Path, args: &[&str]) -> (i32, String) {
    let out = Command::new(BIN)
        .current_dir(dir)
        .env("LUTE_TELEMETRY", "0")
        .args(args)
        .output()
        .unwrap();
    (
        out.status.code().unwrap(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    )
}

// ---- #175 -------------------------------------------------------------

const QUEST_175: &str = "---\nkind: quest\nid: acme.quests\n---\n\n<quest id=\"firstVisit\" tier=\"run\" title=\"First visit\" start=\"true\">\n  <objective id=\"c1\" title=\"Do it\" done=\"quest.noSuchQuest.state == 'complete' && quest.noSuchQuest.objectives.x.done && quest.firstVisit.state == 'active'\"/>\n  <objective id=\"c2\" title=\"Again\" done=\"quest.other.state == 'complete'\"/>\n</quest>\n";

#[test]
fn a_single_file_check_names_the_skipped_quest_references() {
    let dir = project(
        "tooling-049-175",
        &[
            ("lute.project.yaml", MANIFEST),
            ("schema/world.schema.yaml", SCHEMA),
            ("quests/q.lute", QUEST_175),
        ],
    );
    let (code, out) = run(&dir, &["check", "quests/q.lute", "--project", "."]);
    assert_eq!(code, 0, "{out}");
    let notes: Vec<&str> = out
        .lines()
        .filter(|l| l.contains("[W-PROJECT-CHECK-SKIPPED]"))
        .collect();
    // One per distinct undeclared id; the declared `firstVisit` is judged.
    assert_eq!(notes.len(), 2, "{out}");
    assert!(
        notes[0].starts_with("quests/q.lute:7:")
            && notes[0].contains(": info [W-PROJECT-CHECK-SKIPPED] `quest.noSuchQuest.")
            && notes[0].contains(
                "names quest `noSuchQuest`, which this document does not declare; quest \
                 references are resolved only by whole-project `lute check` (`lute check \
                 --project <DIR>`), so this check did not judge it"
            ),
        "{out}"
    );
    assert!(
        notes[1].contains("`quest.other.state` names quest `other`"),
        "{out}"
    );
    assert!(
        out.contains("ok: quests/q.lute (0 warning(s), 2 info)"),
        "{out}"
    );

    // Whole-project check never raises it; it reports the reference itself.
    let (code, out) = run(&dir, &["check", "--project", "."]);
    assert_eq!(code, 1, "{out}");
    assert!(!out.contains("W-PROJECT-CHECK-SKIPPED"), "{out}");
    assert!(out.contains("[E-QUEST-REF-UNKNOWN]"), "{out}");
}

// ---- #187 -------------------------------------------------------------

fn asset_project(name: &str, lint: &str) -> lute_scratch::ScratchDir {
    let dir = project(
        name,
        &[
            (
                "lute.project.yaml",
                "pluginsDir: plugins/\ncatalogDir: catalog/\ndefaultProfile: game\nprofiles:\n  game:\n    plugins:\n      acme.stage: true\ndefaults:\n  contentLang: en\n  uses: [schema/world.schema.yaml]\n",
            ),
            (
                "plugins/acme.stage/plugin.yaml",
                "id: acme.stage\nversion: 0.1.0\nkind: capability\ndepends:\n  - { id: lute.core, range: \"^0.0.1\" }\nexports:\n  stampAttrs: stampattrs/\n  providers: providers/\n",
            ),
            (
                "plugins/acme.stage/stampattrs/s.yaml",
                "stampAttrs:\n  - { name: assetId, type: string }\n  - { name: voiceAssetId, type: string }\n",
            ),
            (
                "plugins/acme.stage/providers/p.yaml",
                "providers:\n  - name: asset\n    idShape: \"String\"\n    snapshot: asset-ids\n",
            ),
            (
                "catalog/asset-ids.yaml",
                "manifestVersion: \"pending\"\nproviderVersion: \"1\"\nstale: false\nentries:\n  asset: [BG.hall, SP.mira.smile, VO.mira.001]\n",
            ),
            ("lute.lint.yaml", lint),
            ("schema/world.schema.yaml", "cast:\n  mira: { name: Mira }\n"),
            (
                "scenes/intro.lute",
                "---\nkind: scene\nid: intro\n---\n\n## Hall {#hall}\n\n::bg{location=\"hall\" time=\"day\" assetId=\"BG.missing\"}\n@mira{assetId=\"SP.missing\" voiceAssetId=\"VO.missing\"}: Hello.\n@mira{assetId=\"SP.mira.smile\"}: Fine.\n::sfx{voiceAssetId=\"VO.gone\"}\n::sfx{voiceAssetId=\"none\"}\n",
            ),
        ],
    );
    let (code, out) = run(
        &dir,
        &[
            "catalog",
            "refresh",
            "--project",
            ".",
            "--providers",
            "catalog",
        ],
    );
    assert_eq!(code, 0, "{out}");
    dir
}

fn asset_lines(out: &str) -> Vec<&str> {
    out.lines()
        .filter(|l| l.contains("[L-ASSET-EXISTS]"))
        .collect()
}

#[test]
fn asset_exists_selectors_read_line_and_directive_attributes() {
    let dir = asset_project(
        "tooling-049-187",
        "rules:\n  asset-exists:\n    level: error\n    options:\n      providers:\n        bg: asset\n        \"@assetId\": asset\n        \"@voiceAssetId\": asset\n        sfx.voiceAssetId: asset\n",
    );
    let (code, out) = run(&dir, &["lint", "--project", "."]);
    assert_eq!(code, 1, "{out}");
    assert_eq!(
        asset_lines(&out),
        [
            "scenes/intro.lute:8:42: error [L-ASSET-EXISTS] asset id `BG.missing` not in provider `asset` (`::bg`)",
            "scenes/intro.lute:9:16: error [L-ASSET-EXISTS] asset id `SP.missing` not in provider `asset` (`@mira` attribute `assetId`)",
            "scenes/intro.lute:9:42: error [L-ASSET-EXISTS] asset id `VO.missing` not in provider `asset` (`@mira` attribute `voiceAssetId`)",
            "scenes/intro.lute:11:21: error [L-ASSET-EXISTS] asset id `VO.gone` not in provider `asset` (`::sfx` attribute `voiceAssetId`)",
        ],
        "{out}"
    );
}

#[test]
fn an_asset_exists_key_that_is_no_selector_is_a_config_error() {
    for key in ["\"bg.\"", "\"@\"", "\"a.b.c\"", "\"::bg\""] {
        let dir = asset_project(
            "tooling-049-187-bad",
            &format!(
                "rules:\n  asset-exists:\n    options:\n      providers:\n        {key}: asset\n"
            ),
        );
        let (code, out) = run(&dir, &["lint", "--project", "."]);
        assert_eq!(code, 1, "{key}: {out}");
        assert!(
            out.lines()
                .any(|l| l.contains("[E-LINT-CONFIG]") && l.contains("is not a selector")),
            "{key}: {out}"
        );
        assert!(asset_lines(&out).is_empty(), "{key}: {out}");
    }
}

// ---- #188 -------------------------------------------------------------

fn lint_project(name: &str) -> lute_scratch::ScratchDir {
    project(
        name,
        &[
            ("lute.project.yaml", MANIFEST),
            (
                "schema/world.schema.yaml",
                "cast:\n  mira: { name: Mira }\n",
            ),
            (
                "lute.lint.yaml",
                "rules:\n  section-starts-with-background: off\n",
            ),
            (
                "story/a.lute",
                "---\nkind: scene\nid: a\n---\n\n## Hall {#hall}\n\n@mira: Hello.\n",
            ),
            (
                "story/deep/b.lute",
                "---\nkind: scene\nid: b\n---\n\n## Hall {#hall}\n\n@mira: Hi.\n",
            ),
        ],
    )
}

#[test]
fn lint_of_a_directory_uses_its_project() {
    let dir = lint_project("tooling-049-188");
    let (code, out) = run(&dir, &["lint", "story"]);
    assert_eq!(code, 0, "{out}");
    assert!(!out.contains("L-SECTION-STARTS-WITH-BACKGROUND"), "{out}");
    assert!(
        out.contains("ok: story (0 error(s), 0 warning(s))"),
        "{out}"
    );

    // From inside the directory, the manifest above it still governs.
    let (code, out) = run(&dir.join("story"), &["lint", "deep"]);
    assert_eq!(code, 0, "{out}");
    assert!(!out.contains("L-SECTION-STARTS-WITH-BACKGROUND"), "{out}");

    // Findings print relative to the project root.
    std::fs::write(
        dir.join("lute.lint.yaml"),
        "rules:\n  section-starts-with-background: warn\n",
    )
    .unwrap();
    let (_, out) = run(&dir, &["lint", "story/deep"]);
    assert!(
        out.contains("story/deep/b.lute:6:1: warning [L-SECTION-STARTS-WITH-BACKGROUND]"),
        "{out}"
    );
}

#[test]
fn lint_of_a_directory_inside_a_named_project() {
    let dir = lint_project("tooling-049-188-project");
    let (code, out) = run(&dir, &["lint", "story", "--project", "."]);
    assert_eq!(code, 0, "{out}");
    assert!(!out.contains("L-SECTION-STARTS-WITH-BACKGROUND"), "{out}");
    let abs = dir.to_str().unwrap();
    let (code, out) = run(&dir, &["lint", "story", "--project", abs]);
    assert_eq!(code, 0, "{out}");

    // A directory outside the named project is a usage error.
    let (code, out) = run(&dir.join("story"), &["lint", "..", "--project", "deep"]);
    assert_eq!(code, 2, "{out}");
    assert!(
        out.contains("lute lint: .. is not inside the project deep"),
        "{out}"
    );
    let (code, out) = run(
        &dir.join("story"),
        &["lint", "..", "--project", "deep", "--json"],
    );
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("\"code\": \"E-CLI-USAGE\""), "{out}");
}

// ---- #160 / #161 ------------------------------------------------------

#[test]
fn the_kind_clash_names_a_shared_root_and_the_ident_fault_a_provider() {
    let dir = project(
        "tooling-049-hints",
        &[
            ("lute.project.yaml", MANIFEST),
            (
                "schema/world.schema.yaml",
                "entities:\n  product: { members: [crate, lamp] }\n  item: { members: [lamp, rope] }\n  command: { members: [puzzle, \"vendor:reply\"] }\nenums:\n  cue: [a, \"x:y\"]\ncast:\n  mira: { name: Mira }\n",
            ),
            (
                "scenes/intro.lute",
                "---\nkind: scene\nid: intro\n---\n\n## Hall {#hall}\n\n@mira: Hello.\n",
            ),
        ],
    );
    let (code, out) = run(&dir, &["check", "--project", "."]);
    assert_eq!(code, 1, "{out}");
    let clash = out
        .lines()
        .find(|l| l.contains("[E-ENTITY-KIND-CLASH]"))
        .unwrap_or_else(|| panic!("{out}"));
    assert!(
        clash.contains(
            "; if they are two id spaces whose ids only spell `lamp` the same, declare a shared \
             root kind listing both spaces' members and make each a sub-kind of it \
             (`product: { subsetOf: <root>, members: […] }`) — every reference still checks \
             against its own kind"
        ),
        "{clash}"
    );
    let route = " — to validate ids from another system that are not names (`vendor:reply`), \
                 declare a provider (`plugins/<id>/providers/`, pinned snapshot in \
                 `catalogDir`) and type the attribute `{ providerRef: <provider> }`";
    for member in ["entity member `vendor:reply`", "enum member `x:y`"] {
        let line = out
            .lines()
            .find(|l| l.contains("[E-PATH-IDENT]") && l.contains(member))
            .unwrap_or_else(|| panic!("{member}: {out}"));
        assert!(line.contains(route), "{line}");
    }
    // Names that are not members keep the plain rule.
    assert!(
        !out.lines()
            .any(|l| l.contains("[E-PATH-IDENT]") && l.contains(" kind ") && l.contains(route)),
        "{out}"
    );
}

/// The shared-root pattern the #160 hint names checks clean.
#[test]
fn the_shared_root_pattern_checks() {
    let dir = project(
        "tooling-049-160-root",
        &[
            ("lute.project.yaml", MANIFEST),
            (
                "schema/world.schema.yaml",
                "entities:\n  slug: { members: [crate, lamp, rope] }\n  product: { subsetOf: slug, members: [crate, lamp] }\n  item: { subsetOf: slug, members: [lamp, rope] }\nstate:\n  run.picked: { type: { domain: item }, default: rope }\n  run.sold: { type: { domain: product }, default: crate }\ncast:\n  mira: { name: Mira }\n",
            ),
            (
                "scenes/intro.lute",
                "---\nkind: scene\nid: intro\n---\n\n## Hall {#hall}\n\n@mira: Hello.\n::set{run.picked = 'lamp'}\n",
            ),
        ],
    );
    let (code, out) = run(&dir, &["check", "--project", "."]);
    assert!(!out.contains("E-ENTITY-KIND-CLASH"), "{out}");
    assert_eq!(code, 0, "{out}");
}
