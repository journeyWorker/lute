use lute_manifest::project::{
    load_project, project_providers, resolve_document_snapshot, IdentityTemplates,
};
use std::collections::BTreeMap;
use std::fs;

fn write_project(root: &std::path::Path) {
    // plugin package
    let pdir = root.join("plugins/idola.minigame/directives");
    fs::create_dir_all(&pdir).unwrap();
    fs::write(
        root.join("plugins/idola.minigame/plugin.yaml"),
        "id: idola.minigame\nversion: 0.1.0\nkind: capability\nexports:\n  directives: directives/\n",
    )
    .unwrap();
    fs::write(
        pdir.join("d.yaml"),
        "directives:\n  - { name: minigame, attrs: [ { name: kind, type: string } ], lower: { kind: builtin, name: n } }\n",
    )
    .unwrap();
    // project config
    fs::write(
        root.join("lute.project.yaml"),
        "pluginsDir: plugins/\ndefaultProfile: date\nprofiles:\n  global:\n    plugins: { lute.core: true }\n  date:\n    plugins: { idola.minigame: true }\n",
    )
    .unwrap();
}

#[test]
fn resolves_project_snapshot_with_active_plugin() {
    let root = std::env::temp_dir().join(format!("lute_proj_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    write_project(&root);
    let proj = load_project(&root)
        .expect("valid project loads")
        .expect("present");
    let (snap, diags) = resolve_document_snapshot(Some(&proj), None, &BTreeMap::new());
    assert!(
        diags.is_empty(),
        "{:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
    assert!(
        snap.directive("minigame").is_some(),
        "active plugin directive present"
    );
    fs::remove_dir_all(&root).ok();
}

#[test]
fn no_project_is_core_only() {
    let (snap, diags) = resolve_document_snapshot(None, None, &BTreeMap::new());
    assert!(diags.is_empty());
    assert!(snap.directive("bg").is_some());
    assert!(snap.directive("minigame").is_none());
}

#[test]
fn missing_project_is_ok_none() {
    let dir = std::env::temp_dir().join(format!("lute_noproj_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    assert!(
        matches!(load_project(&dir), Ok(None)),
        "absent lute.project.yaml -> Ok(None)"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn malformed_project_is_err() {
    let dir = std::env::temp_dir().join(format!("lute_badproj_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("lute.project.yaml"),
        "profiles: [this is not a map\n  : : :",
    )
    .unwrap();
    assert!(
        load_project(&dir).is_err(),
        "malformed lute.project.yaml -> Err"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn resolve_diag_carries_stable_code() {
    // temp project with two mutually-depending plugins (a.x->a.dep->a.x DependsCycle)
    let root = std::env::temp_dir().join(format!("lute_rdcode_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for (id, dep) in [("a.x", "a.dep"), ("a.dep", "a.x")] {
        let d = root.join(format!("plugins/{id}/directives"));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(root.join(format!("plugins/{id}/plugin.yaml")),
            format!("id: {id}\nversion: 0.1.0\nkind: capability\ndepends: [ {{ id: {dep}, range: \"^0.1.0\" }} ]\nexports:\n  directives: directives/\n")).unwrap();
        std::fs::write(d.join("x.yaml"), "directives: []\n").unwrap();
    }
    std::fs::write(
        root.join("lute.project.yaml"),
        "defaultProfile: p\nprofiles:\n  p:\n    plugins: { a.x: true }\n",
    )
    .unwrap();
    let proj = lute_manifest::project::load_project(&root)
        .unwrap()
        .unwrap();
    let (_snap, diags) = lute_manifest::project::resolve_document_snapshot(
        Some(&proj),
        None,
        &std::collections::BTreeMap::new(),
    );
    assert!(
        diags.iter().any(|d| d.code == "E-DEPENDS-CYCLE"),
        "expected E-DEPENDS-CYCLE code, got {:?}",
        diags.iter().map(|d| &d.code).collect::<Vec<_>>()
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn project_providers_loads_catalog() {
    let proj = load_project(std::path::Path::new("../../docs/examples/idola-project"))
        .unwrap()
        .unwrap();
    let ps = project_providers(Some(&proj));
    assert_eq!(
        ps.contains("minigameId", "bianca_service_01"),
        lute_manifest::provider::IdStatus::Fresh,
        "project catalog resolves the minigame id"
    );
    assert_eq!(
        project_providers(None).contains("minigameId", "bianca_service_01"),
        lute_manifest::provider::IdStatus::Absent,
        "no project -> empty providers"
    );
}

/// Write a minimal (plugin-free) project whose `lute.project.yaml` carries the
/// given `identity:` YAML body, returning its root.
fn identity_project(tag: &str, identity_yaml: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("lute_ident_{}_{tag}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("lute.project.yaml"),
        format!("defaultProfile: base\nprofiles:\n  base:\n    plugins: {{}}\n{identity_yaml}"),
    )
    .unwrap();
    root
}

/// A project with no `identity:` block resolves 0.7.0's hardcoded pair, so an
/// existing project compiles byte-identically (0.8.0 §9).
#[test]
fn absent_identity_block_defaults_to_070_templates() {
    let root = identity_project("absent", "");
    let proj = load_project(&root).unwrap().unwrap();
    assert_eq!(proj.identity, IdentityTemplates::default());
    assert_eq!(proj.identity.line_id, "{prefix}.{speaker}_{code}");
    assert_eq!(proj.identity.voice_key, "{speaker}-{code}");
    assert!(proj.identity_diags.is_empty());
    fs::remove_dir_all(&root).ok();
}

/// Each key is authored and resolved independently: retemplating `lineId`
/// alone leaves `voiceKey` at its default.
#[test]
fn authored_identity_templates_resolve_per_key() {
    let root = identity_project(
        "authored",
        "identity:\n  lineId: \"{prefix}/{speaker}#{code}\"\n",
    );
    let proj = load_project(&root).unwrap().unwrap();
    assert_eq!(proj.identity.line_id, "{prefix}/{speaker}#{code}");
    assert_eq!(proj.identity.voice_key, "{speaker}-{code}");
    assert!(proj.identity_diags.is_empty());
    assert_eq!(
        proj.identity
            .render_line_id("npc_koyuki_ep05", "koyuki", "0010"),
        "npc_koyuki_ep05/koyuki#0010"
    );
    fs::remove_dir_all(&root).ok();
}

/// An unknown `{token}` is `E-IDENTITY-TEMPLATE` at load. The project still
/// loads (its plugin graph is unaffected) with the offending key reset to its
/// default, and `resolve_document_snapshot` — the ONE resolver both the CLI
/// and the LSP call — replays the diagnostic, so neither surface can miss it.
#[test]
fn unknown_identity_token_is_reported_at_project_load() {
    let root = identity_project("unknown", "identity:\n  lineId: \"{prefix}.{foo}\"\n");
    let proj = load_project(&root).unwrap().unwrap();

    assert_eq!(proj.identity_diags.len(), 1, "{:?}", proj.identity_diags);
    assert_eq!(proj.identity_diags[0].code, "E-IDENTITY-TEMPLATE");
    assert_eq!(
        proj.identity_diags[0].message,
        "unknown token `{foo}` in identity template `lineId`; \
         valid tokens are {prefix}, {speaker}, {code}"
    );
    // Fail closed: the rejected key falls back to 0.7.0's shape.
    assert_eq!(proj.identity.line_id, "{prefix}.{speaker}_{code}");

    let (_snap, diags) = resolve_document_snapshot(Some(&proj), None, &BTreeMap::new());
    assert!(
        diags.iter().any(|d| d.code == "E-IDENTITY-TEMPLATE"),
        "the shared resolver replays it: {:?}",
        diags.iter().map(|d| &d.code).collect::<Vec<_>>()
    );
    fs::remove_dir_all(&root).ok();
}

/// A template that resolves to an empty string is rejected too — an empty
/// `lineId` would silently un-key every line.
#[test]
fn empty_identity_template_is_reported_at_project_load() {
    let root = identity_project("empty", "identity:\n  voiceKey: \"\"\n");
    let proj = load_project(&root).unwrap().unwrap();
    assert_eq!(proj.identity_diags.len(), 1, "{:?}", proj.identity_diags);
    assert_eq!(
        proj.identity_diags[0].message,
        "identity template `voiceKey` resolves to an empty string"
    );
    assert_eq!(proj.identity.voice_key, "{speaker}-{code}");
    fs::remove_dir_all(&root).ok();
}

/// The renderer is total: repeated tokens substitute every time, an
/// unterminated `{` is literal text, and multi-byte literals around a token
/// never split a char boundary. None of these panic.
#[test]
fn identity_renderer_is_total() {
    use lute_manifest::project::render_identity_template;

    assert_eq!(
        render_identity_template("{speaker}/{speaker}", "p", "koyuki", "0010"),
        "koyuki/koyuki"
    );
    assert_eq!(
        render_identity_template("日本{speaker}語{code}", "p", "koyuki", "0010"),
        "日本koyuki語0010"
    );
    // Unterminated `{` -> literal tail, nothing swallowed.
    assert_eq!(
        render_identity_template("{prefix}.{speaker", "ep05", "koyuki", "0010"),
        "ep05.{speaker"
    );
    // An empty substitution is still a total render (an empty `code` is a
    // back-fill failure the caller already handled by skipping the line).
    assert_eq!(
        render_identity_template("{speaker}-{code}", "p", "koyuki", ""),
        "koyuki-"
    );
    assert_eq!(render_identity_template("", "p", "s", "c"), "");
    // No tokens at all -> the literal, unchanged.
    assert_eq!(render_identity_template("fixed", "p", "s", "c"), "fixed");
}

// ── 0.10.0 §6: `defaults:` (backlog #34, D-F/D-P/D-Q) ──────────────────

fn write_manifest(tag: &str, body: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("lute-defaults-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("lute.project.yaml"), body).unwrap();
    dir
}

/// The closed defaultable set (§6.1, D-P) loads and is readable per key.
#[test]
fn defaults_block_loads_the_closed_key_set() {
    let dir = write_manifest(
        "ok",
        "defaultProfile: core\ndefaults:\n  kind: scene\n  character: anseo\n  season: 1\n",
    );
    let proj = lute_manifest::project::load_project(&dir).unwrap().unwrap();
    assert!(proj.defaults_diags.is_empty(), "{:?}", proj.defaults_diags);
    assert_eq!(
        proj.defaults.get("character").and_then(|v| v.as_str()),
        Some("anseo")
    );
    assert_eq!(
        proj.defaults.get("season").and_then(|v| v.as_i64()),
        Some(1)
    );
}

/// A key outside the closed set is `E-DEFAULTS-KEY` at project load, with a
/// did-you-mean over the set (§6.1). The project still resolves — a bad
/// `defaults:` must not collapse the whole manifest to core-only, exactly as
/// a bad `identity:` template does not.
#[test]
fn defaults_unknown_key_is_e_defaults_key_with_a_suggestion() {
    let dir = write_manifest(
        "typo",
        "defaultProfile: core\ndefaults:\n  charater: anseo\n",
    );
    let proj = lute_manifest::project::load_project(&dir).unwrap().unwrap();
    assert_eq!(proj.defaults_diags.len(), 1, "{:?}", proj.defaults_diags);
    let d = &proj.defaults_diags[0];
    assert_eq!(d.code, "E-DEFAULTS-KEY");
    assert!(
        d.message.contains("charater"),
        "names the offending key: {}",
        d.message
    );
    assert!(
        d.message.contains("character"),
        "did-you-mean over the set: {}",
        d.message
    );
    assert_eq!(
        proj.graph.default_profile, "core",
        "the project still resolves"
    );
    assert!(
        proj.defaults.get("charater").is_none(),
        "a rejected key is not applied"
    );
}

/// `mode:` is a legal core frontmatter key that NOTHING reads — the analysis
/// mode comes from the check invocation. A defaultable key that changes
/// nothing is a trap in a block whose whole purpose is to change many
/// documents at once, so D-P excludes it rather than shipping a no-op.
#[test]
fn defaults_rejects_inert_and_excluded_keys() {
    for key in [
        "mode",
        "episodeId",
        "title",
        "after",
        "profile",
        "plugins",
        "state",
        "component",
    ] {
        let dir = write_manifest(
            &format!("excl-{key}"),
            &format!("defaultProfile: core\ndefaults:\n  {key}: x\n"),
        );
        let proj = lute_manifest::project::load_project(&dir).unwrap().unwrap();
        assert!(
            proj.defaults_diags
                .iter()
                .any(|d| d.code == "E-DEFAULTS-KEY"),
            "`{key}` is not defaultable (§6.1): {:?}",
            proj.defaults_diags
        );
    }
}

/// A `defaults:` value whose SHAPE cannot inhabit its key's core type is
/// decidable without a document, so it is reported once at the manifest —
/// the only anchoring a spanless `ResolveDiag` can offer (D-Z).
#[test]
fn defaults_value_shape_is_checked_at_the_manifest() {
    let dir = write_manifest(
        "shape",
        "defaultProfile: core\ndefaults:\n  season: not-a-number\n  kind: sausage\n",
    );
    let proj = lute_manifest::project::load_project(&dir).unwrap().unwrap();
    let codes: Vec<&str> = proj
        .defaults_diags
        .iter()
        .map(|d| d.code.as_str())
        .collect();
    assert_eq!(
        codes,
        ["E-DEFAULTS-KEY", "E-DEFAULTS-KEY"],
        "{:?}",
        proj.defaults_diags
    );
    assert!(proj
        .defaults_diags
        .iter()
        .any(|d| d.message.contains("season")));
    assert!(proj
        .defaults_diags
        .iter()
        .any(|d| d.message.contains("kind")));
    assert!(
        proj.defaults.get("season").is_none(),
        "an ill-shaped default is not applied"
    );
}

/// Present-but-empty is PRESENT (§6.2). `uses: []` in the manifest is a real
/// default meaning "no imports", not an absent key.
#[test]
fn defaults_empty_list_is_a_present_value() {
    let dir = write_manifest("empty", "defaultProfile: core\ndefaults:\n  uses: []\n");
    let proj = lute_manifest::project::load_project(&dir).unwrap().unwrap();
    assert!(proj.defaults_diags.is_empty(), "{:?}", proj.defaults_diags);
    assert!(
        proj.defaults.get("uses").is_some(),
        "`uses: []` is a present key"
    );
}

/// D-Q, verified rather than assumed: `RawProject` has no
/// `deny_unknown_fields`, so a manifest with NO `defaults:` is unchanged and
/// a pre-0.10.0 binary silently ignores one that has it.
#[test]
fn a_manifest_without_defaults_has_an_empty_block() {
    let dir = write_manifest("none", "defaultProfile: core\n");
    let proj = lute_manifest::project::load_project(&dir).unwrap().unwrap();
    assert!(proj.defaults.is_empty());
    assert!(proj.defaults_diags.is_empty());
}

/// 0.10.0 §6.5: `defaults:` is per project ROOT. A nested root's block
/// **replaces** an outer root's entirely — no inheritance, no merge. Written
/// as a two-manifest fixture on disk, because "it cannot inherit" is a claim
/// about what the loader does NOT read, and that is only observable when there
/// is something above it to read.
#[test]
fn a_nested_roots_defaults_replaces_the_outer_roots_entirely() {
    let outer = write_manifest(
        "nest-outer",
        "defaultProfile: core\ndefaults:\n  contentLang: en\n  character: outer\n",
    );
    let inner = outer.join("inner");
    std::fs::create_dir_all(&inner).unwrap();
    std::fs::write(
        inner.join("lute.project.yaml"),
        "defaultProfile: core\ndefaults:\n  pov: third\n",
    )
    .unwrap();

    let proj = lute_manifest::project::load_project(&inner)
        .unwrap()
        .unwrap();
    // REPLACES: exactly the nested root's key set, in full.
    assert_eq!(proj.defaults.keys().collect::<Vec<_>>(), vec!["pov"]);
    // A merge would carry these two along. Named individually so the failure
    // says which rule broke rather than only that a count moved.
    assert!(
        proj.defaults.get("contentLang").is_none(),
        "no inheritance across roots (§6.5)"
    );
    assert!(
        proj.defaults.get("character").is_none(),
        "no inheritance across roots (§6.5)"
    );

    // And the outer root still resolves its own, unaffected by the nested one.
    let up = lute_manifest::project::load_project(&outer)
        .unwrap()
        .unwrap();
    assert_eq!(
        up.defaults.keys().collect::<Vec<_>>(),
        vec!["character", "contentLang"]
    );
    assert!(
        up.defaults.get("pov").is_none(),
        "a nested root does not leak upward either"
    );
}

/// 0.10.0 §6.5: there is **no `extends:` between manifests.** `RawProject` has
/// no `deny_unknown_fields`, so a manifest-level `extends:` is silently
/// discarded — the correct behaviour, and unpinned until this test. It reads
/// nothing: not the named file, not the parent root.
#[test]
fn a_manifest_level_extends_inherits_nothing() {
    let outer = write_manifest(
        "ext-outer",
        "defaultProfile: core\ndefaults:\n  character: outer\n",
    );
    let inner = outer.join("inner");
    std::fs::create_dir_all(&inner).unwrap();
    std::fs::write(
        inner.join("lute.project.yaml"),
        "defaultProfile: core\nextends: ../lute.project.yaml\n",
    )
    .unwrap();

    let proj = lute_manifest::project::load_project(&inner)
        .unwrap()
        .unwrap();
    assert!(
        proj.defaults.is_empty(),
        "`extends:` between manifests inherits nothing (§6.5)"
    );
    assert!(
        proj.defaults_diags.is_empty(),
        "and it is not an error either: {:?}",
        proj.defaults_diags
    );
}

/// D-Y/D-Z: a path written in `defaults:` resolves against the MANIFEST's
/// directory, and is canonicalised at load so the single-`base_dir` import
/// resolvers never have to know that.
#[test]
fn defaults_paths_canonicalise_against_the_manifest() {
    let dir = write_manifest(
        "paths",
        "defaultProfile: core\ndefaults:\n  uses: [world.schema.yaml]\n",
    );
    std::fs::write(
        dir.join("world.schema.yaml"),
        "state:\n  run.k: { type: number, default: 0 }\n",
    )
    .unwrap();
    let proj = lute_manifest::project::load_project(&dir).unwrap().unwrap();
    assert!(proj.defaults_diags.is_empty(), "{:?}", proj.defaults_diags);
    let resolved = proj.defaults.get("uses").unwrap().as_sequence().unwrap();
    let got = std::path::PathBuf::from(resolved[0].as_str().unwrap());
    assert!(
        got.is_absolute(),
        "a defaulted path is pre-resolved (D-Z): {got:?}"
    );
    assert_eq!(
        got,
        std::fs::canonicalize(dir.join("world.schema.yaml")).unwrap()
    );
}

/// D-Z: canonicalisation is I/O and can fail. A bad path fails ONCE, at the
/// manifest, naming the `defaults:` key — never once per inheriting document.
#[test]
fn defaults_bad_path_fails_at_the_manifest() {
    let dir = write_manifest(
        "badpath",
        "defaultProfile: core\ndefaults:\n  uses: [nope.schema.yaml]\n",
    );
    let proj = lute_manifest::project::load_project(&dir).unwrap().unwrap();
    assert_eq!(proj.defaults_diags.len(), 1, "{:?}", proj.defaults_diags);
    assert_eq!(proj.defaults_diags[0].code, "E-DEFAULTS-KEY");
    assert!(proj.defaults_diags[0].message.contains("nope.schema.yaml"));
    assert!(proj.defaults_diags[0].message.contains("uses"));
    assert!(
        proj.defaults.get("uses").is_none(),
        "an unresolvable default is not applied"
    );
}

/// A single string, not a sequence, is the other authored `uses:` shape and
/// canonicalises the same way.
#[test]
fn defaults_scalar_path_canonicalises_too() {
    let dir = write_manifest(
        "scalarpath",
        "defaultProfile: core\ndefaults:\n  extends: base.schema.yaml\n",
    );
    std::fs::write(dir.join("base.schema.yaml"), "state: {}\n").unwrap();
    let proj = lute_manifest::project::load_project(&dir).unwrap().unwrap();
    assert!(proj.defaults_diags.is_empty(), "{:?}", proj.defaults_diags);
    let got = proj.defaults.get("extends").unwrap().as_str().unwrap();
    assert!(std::path::Path::new(got).is_absolute(), "{got}");
}

// ── 0.15.0 §3/§8 D-D: `extra:` joins `DEFAULTABLE_KEYS`; `id:` deliberately does not.

/// A `defaults.extra:` mapping loads and its value is carried verbatim.
#[test]
fn defaults_extra_mapping_loads() {
    let dir = write_manifest(
        "meta-ok",
        "defaultProfile: core\ndefaults:\n  extra:\n    arc: main\n    tags: [harbor, night]\n",
    );
    let proj = lute_manifest::project::load_project(&dir).unwrap().unwrap();
    assert!(proj.defaults_diags.is_empty(), "{:?}", proj.defaults_diags);
    let value = proj
        .defaults
        .get("extra")
        .expect("`extra:` default is applied");
    let map = value.as_mapping().expect("stored as a YAML mapping");
    assert_eq!(
        map.get(serde_yaml::Value::String("arc".into()))
            .and_then(|v| v.as_str()),
        Some("main")
    );
}

/// A non-mapping `defaults.extra:` value is `E-DEFAULTS-KEY` (§3).
#[test]
fn defaults_extra_non_mapping_is_e_defaults_key() {
    let dir = write_manifest(
        "meta-scalar",
        "defaultProfile: core\ndefaults:\n  extra: not-a-mapping\n",
    );
    let proj = lute_manifest::project::load_project(&dir).unwrap().unwrap();
    assert_eq!(proj.defaults_diags.len(), 1, "{:?}", proj.defaults_diags);
    assert_eq!(proj.defaults_diags[0].code, "E-DEFAULTS-KEY");
    assert!(
        proj.defaults_diags[0].message.contains("extra"),
        "names the offending key: {}",
        proj.defaults_diags[0].message
    );
    assert!(
        proj.defaults.get("extra").is_none(),
        "an ill-shaped default is not applied"
    );
}

/// D-D: `id:` is per-document unique — outside the closed defaultable set.
#[test]
fn defaults_id_is_e_defaults_key() {
    let dir = write_manifest("id-default", "defaultProfile: core\ndefaults:\n  id: x\n");
    let proj = lute_manifest::project::load_project(&dir).unwrap().unwrap();
    assert!(
        proj.defaults_diags
            .iter()
            .any(|d| d.code == "E-DEFAULTS-KEY"),
        "`id:` is not defaultable (D-D): {:?}",
        proj.defaults_diags
    );
    assert!(
        proj.defaults.get("id").is_none(),
        "a rejected default is not applied"
    );
}
