//! Task 8 (dsl 0.4.0 §6.4) — the compile-time static-selection/residual fold
//! for a param-scoped `<match>` inside `::use` expansion
//! (`normalize::fold_component_matches`). Written FIRST (TDD): every test
//! here fails to compile against a pre-T8 `normalize.rs` (the function does
//! not exist), then fails to pass against a stub, then passes once the fold
//! is implemented.
//!
//! Harness mirrors `crates/lute-check/tests/component_match.rs` (temp-dir
//! component fixtures) adapted compile-side, plus `crates/lute-compile/tests/
//! e2e.rs`'s `input_for`/`golden` idiom (assembling a real `CheckInput` and
//! running the full `compile()` pipeline — the fold is only externally
//! observable through the emitted artifact).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use lute_check::{parse_meta, resolve_components, resolve_imports, CheckInput, Mode};
use lute_compile::compile;
use lute_manifest::core::load_core_snapshot;
use lute_manifest::provider::ProviderSet;
use lute_manifest::snapshot::CapabilitySnapshot;

static UNIQ: AtomicU64 = AtomicU64::new(0);

fn unique_dir() -> PathBuf {
    let n = UNIQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!(
        "lute_fold_{}_{}_{}",
        std::process::id(),
        n,
        nanos
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_lute(dir: &Path, name: &str, body: &str) {
    std::fs::write(dir.join(name), body).unwrap();
}

/// Assemble a `CheckInput` for `scene_text` against on-disk fixtures in
/// `dir` (component files, resolved via `resolve_components` exactly like
/// the CLI/LSP) and compile it. Panics with the gating diagnostics on a
/// failed check/compile — every fixture below is expected to compile clean.
fn compile_scene(dir: &Path, scene_text: &str) -> serde_json::Value {
    let (doc, _) = lute_syntax::parse(scene_text);
    let (meta0, _) = parse_meta(&doc.meta, &CapabilitySnapshot::default());
    let components = resolve_components(dir, &meta0.components, doc.meta.span);
    let imports = resolve_imports(dir, &meta0.uses, &meta0.extends, doc.meta.span);
    let input = CheckInput {
        text: scene_text.to_string(),
        uri: "scene.lute".into(),
        snapshot: load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Ci,
        imports,
        components,
    };
    let artifact =
        compile(&input).unwrap_or_else(|e| panic!("scene compiles clean: {e:#?}\n{scene_text}"));
    serde_json::to_value(&artifact).expect("artifact serializes")
}

fn scene(components: &str, body: &str) -> String {
    format!(
        "---\nkind: scene\ncharacter: demo\nseason: 1\nepisode: 1\ncomponents: [{components}]\n---\n## Shot 1.\n{body}\n"
    )
}

fn scene_with(components: &str, extra_frontmatter: &str, body: &str) -> String {
    format!(
        "---\nkind: scene\ncharacter: demo\nseason: 1\nepisode: 1\ncomponents: [{components}]\n{extra_frontmatter}---\n## Shot 1.\n{body}\n"
    )
}

/// Every `commands[]` entry (top-level — `MatchArm` bodies never nest;
/// they're flattened into the same array, addressed and jumped to).
fn commands(artifact: &serde_json::Value) -> &[serde_json::Value] {
    artifact["commands"].as_array().expect("commands array")
}

/// Count `{"kind":"match", ...}` entries — the direct observable proof a
/// `<match>` DID (residual, §6.4 case 2) or did NOT (folded away, case 1)
/// lower to a match command record.
fn match_count(artifact: &serde_json::Value) -> usize {
    commands(artifact)
        .iter()
        .filter(|c| c["kind"] == "match")
        .count()
}

fn lines(artifact: &serde_json::Value) -> Vec<&serde_json::Value> {
    commands(artifact)
        .iter()
        .filter(|c| c["kind"] == "line")
        .collect()
}

/// The §6.5 worked example, verbatim (spec `docs/proposals/scenario-dsl/
/// 0.4.0.md` §6.5) — the SAME fixture `crates/lute-check/tests/
/// component_match.rs` uses to prove admission (T6/T7); this file proves the
/// compile-side fold over the identical shape.
const REACTION: &str = "---\ncomponent: reaction\nparams:\n  tier: { enum: [cold, warm, fond] }\n---\n\
## Scene 1.\n\
<match on=\"@tier\">\n\
<when is=\"fond\">\n@bianca{emotion=\"delighted\"}: You remembered! You actually remembered.\n</when>\n\
<when is=\"warm\">\n@bianca{emotion=\"content\"}: Not bad at all, Mr. Fixer.\n</when>\n\
<when is=\"cold\">\n@bianca{emotion=\"neutral\"}: ...Shall we begin?\n</when>\n\
</match>\n";

#[test]
fn literal_arg_folds_to_selected_arm() {
    let dir = unique_dir();
    write_lute(&dir, "reaction.lute", REACTION);
    let via_component = scene(
        "reaction.lute",
        "::use{component=\"reaction\" tier=\"fond\"}",
    );
    let artifact = compile_scene(&dir, &via_component);

    // §6.4 case 1: zero match records anywhere in the artifact.
    assert_eq!(
        match_count(&artifact),
        0,
        "a fully literal-bound param match must fold away entirely; artifact: {artifact:#}"
    );
    // Exactly the selected ("fond") arm's line — the other two arms' text
    // never appears (dead-arm elimination, not just first-match masking).
    let ls = lines(&artifact);
    assert_eq!(ls.len(), 1, "exactly the selected arm's one line; got {ls:?}");
    assert_eq!(ls[0]["speaker"], "bianca");
    assert_eq!(
        ls[0]["text"],
        "You remembered! You actually remembered."
    );
    assert_eq!(ls[0]["emotion"], "delighted");
    let full_text = serde_json::to_string(&artifact).unwrap();
    assert!(!full_text.contains("Not bad at all"));
    assert!(!full_text.contains("Shall we begin"));

    // "the compiled artifact is indistinguishable from the hand-duplicated
    // original" (§6.5) — compare the CONTENT fields of the folded line
    // against a hand-typed twin, modulo `addr`/`source` (component
    // provenance necessarily differs — the Task 11 precedent: identity
    // compares strip structural churn, not raw JSON equality).
    let hand_duplicated = "---\nkind: scene\ncharacter: demo\nseason: 1\nepisode: 1\n---\n\
## Shot 1.\n@bianca{emotion=\"delighted\"}: You remembered! You actually remembered.\n";
    let twin = compile_scene(&dir, hand_duplicated);
    let twin_lines = lines(&twin);
    assert_eq!(twin_lines.len(), 1);
    let mut folded_line = ls[0].clone();
    let mut twin_line = twin_lines[0].clone();
    for key in ["addr", "source", "lineId", "voiceKey"] {
        folded_line.as_object_mut().unwrap().remove(key);
        twin_line.as_object_mut().unwrap().remove(key);
    }
    assert_eq!(
        folded_line, twin_line,
        "folded line content must be byte-identical to the hand-duplicated twin (§6.5), modulo addr/source/identity churn"
    );
    // But provenance IS recorded — this really did come from the component.
    assert_eq!(ls[0]["source"]["component"], "reaction");
}

#[test]
fn otherwise_selected_when_no_is_matches() {
    // A `number` param: `is=` arms use Number literals (legal per §7.3.1's
    // grammar regardless of subject type); neither matches the bound `5`,
    // so `<otherwise>` (REQUIRED for an infinite domain, §6.3) is spliced.
    const LEVELED: &str = "---\ncomponent: leveled\nparams:\n  n: number\n---\n\
## Scene 1.\n\
<match on=\"@n\">\n\
<when is=\"10\">\n@narrator: exactly ten\n</when>\n\
<when is=\"20\">\n@narrator: exactly twenty\n</when>\n\
<otherwise>\n@narrator: something else\n</otherwise>\n\
</match>\n";
    let dir = unique_dir();
    write_lute(&dir, "leveled.lute", LEVELED);
    let s = scene("leveled.lute", "::use{component=\"leveled\" n=5}");
    let artifact = compile_scene(&dir, &s);

    assert_eq!(match_count(&artifact), 0, "otherwise-arm selection still folds away the match");
    let ls = lines(&artifact);
    assert_eq!(ls.len(), 1);
    assert_eq!(ls[0]["text"], "something else");
    let full_text = serde_json::to_string(&artifact).unwrap();
    assert!(!full_text.contains("exactly ten"));
    assert!(!full_text.contains("exactly twenty"));
}

#[test]
fn ref_bound_arg_stays_residual_match() {
    // §6.4 case 2, §6.5's dynamic-dispatch worked example: the CALLER owns
    // its own state; binding a state-derived def to the param cannot fold
    // (D3: an unexpanded ref is left intact -> undecided by construction).
    let dir = unique_dir();
    write_lute(&dir, "reaction.lute", REACTION);
    let s = scene_with(
        "reaction.lute",
        "state:\n  scene.tier: { type: { enum: [cold, warm, fond] }, default: cold }\n\
defs:\n  currentTier: { type: { enum: [cold, warm, fond] }, cel: \"scene.tier\" }\n",
        "::use{component=\"reaction\" tier=@currentTier}",
    );
    let artifact = compile_scene(&dir, &s);

    assert_eq!(
        match_count(&artifact),
        1,
        "a non-literal (def-bound) arg cannot fold: exactly ONE residual match record; artifact: {artifact:#}"
    );
    let m = commands(&artifact)
        .iter()
        .find(|c| c["kind"] == "match")
        .unwrap();
    // The subject is the SUBSTITUTED def CEL (D4 expansion of `@currentTier`
    // against the caller's own def table) — `@`/`$`-free, per the existing
    // `assert_cel_clean` invariant (e2e.rs) that applies to every artifact.
    let subject = m["subject"].as_str().unwrap();
    assert!(
        subject.contains("scene.tier") && !subject.contains('@') && !subject.contains('$'),
        "residual match subject must be the expanded def CEL, `@`/`$`-free; got {subject:?}"
    );
    // Arms intact: all three `tier` arms of REACTION survive, in order.
    let arms = m["arms"].as_array().unwrap();
    assert_eq!(arms.len(), 3, "all three arms survive the residual dispatch; got {arms:?}");
    // And every arm's line is still reachable in the flattened stream (the
    // dispatch is genuinely dynamic — nothing was eliminated).
    let full_text = serde_json::to_string(&artifact).unwrap();
    for needle in [
        "You remembered! You actually remembered.",
        "Not bad at all, Mr. Fixer.",
        "...Shall we begin?",
    ] {
        assert!(full_text.contains(needle), "arm line `{needle}` must survive a residual dispatch");
    }
}

#[test]
fn nested_param_match_folds_recursively() {
    // A param `<match>` NESTED directly inside another param `<match>`'s
    // arm (§6.2: "recursively — further param-scoped `<match>` blocks"),
    // both literal-bound at the call site. Proves `fold_component_matches`
    // recurses INTO its own spliced replacement (the parent IRC correction:
    // `normalize_nodes`'s outer `::use` loop does NOT re-scan past a
    // splice — the fold must own its recursion, not lean on that).
    const NESTED: &str = "---\ncomponent: nested\nparams:\n  tier: { enum: [cold, fond] }\n  budget: { enum: [low, high] }\n---\n\
## Scene 1.\n\
<match on=\"@tier\">\n\
<when is=\"fond\">\n\
<match on=\"@budget\">\n\
<when is=\"high\">\n@bianca: fond and high\n</when>\n\
<when is=\"low\">\n@bianca: fond and low\n</when>\n\
</match>\n\
</when>\n\
<when is=\"cold\">\n@bianca: cold, no nested dispatch\n</when>\n\
</match>\n";
    let dir = unique_dir();
    write_lute(&dir, "nested.lute", NESTED);
    let s = scene(
        "nested.lute",
        "::use{component=\"nested\" tier=\"fond\" budget=\"high\"}",
    );
    let artifact = compile_scene(&dir, &s);

    assert_eq!(
        match_count(&artifact),
        0,
        "both levels literal-bound: zero match records at ANY depth; artifact: {artifact:#}"
    );
    let ls = lines(&artifact);
    assert_eq!(ls.len(), 1, "exactly the doubly-selected leaf line; got {ls:?}");
    assert_eq!(ls[0]["text"], "fond and high");
}

#[test]
fn existing_goldens_untouched() {
    // B2: this task NEVER folds a scene-level match, and adding the fold
    // must not perturb the 5 pre-existing e2e goldens (`crates/lute-compile/
    // tests/e2e.rs`) by even one byte. Recompile each and diff directly
    // against its committed `.snap` file (stripping insta's 4-line header)
    // — a stronger, self-contained proof than merely re-running e2e.rs
    // (which this task's `cargo test -p lute-compile` run also exercises).
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let goldens: &[(&str, &str, Option<&str>)] = &[
        (
            "e2e__bianca_s01ep02.snap",
            "../../docs/examples/bianca-s01ep02.lute",
            None,
        ),
        (
            "e2e__showcase_episode01.snap",
            "../../docs/examples/showcase/episode01.lute",
            Some("../../docs/examples/showcase"),
        ),
        (
            "e2e__quest_grove.snap",
            "../../docs/examples/quest-grove.lute",
            None,
        ),
        (
            "e2e__quest_rescue_halsin.snap",
            "../../docs/examples/quest-rescue-halsin.lute",
            None,
        ),
        (
            "e2e__components_scene.snap",
            "../../docs/examples/components/scene.lute",
            None,
        ),
    ];
    for (snap_name, path, project_dir) in goldens {
        let file = manifest_dir.join(path);
        let text = std::fs::read_to_string(&file).unwrap();
        let project = project_dir.map(|d| manifest_dir.join(d));
        let project = project
            .as_deref()
            .and_then(|d| lute_manifest::project::load_project(d).expect("project loads"));
        let providers = lute_manifest::project::project_providers(project.as_ref());
        let (doc, _) = lute_syntax::parse(&text);
        let (meta0, _) = lute_check::parse_meta(&doc.meta, &CapabilitySnapshot::default());
        let (snapshot, _) = lute_manifest::project::resolve_document_snapshot(
            project.as_ref(),
            meta0.profile.as_deref(),
            &meta0.plugins,
        );
        let base = file.parent().unwrap();
        let imports = resolve_imports(base, &meta0.uses, &meta0.extends, doc.meta.span);
        let components = resolve_components(base, &meta0.components, doc.meta.span);
        let input = CheckInput {
            text: text.clone(),
            uri: path.to_string(),
            snapshot,
            providers,
            mode: Mode::Ci,
            imports,
            components,
        };
        let artifact = compile(&input).unwrap_or_else(|e| panic!("{path} compiles: {e:#?}"));
        let mut json = serde_json::to_string_pretty(&artifact).unwrap();
        json.push('\n');

        let snap_path = manifest_dir.join("tests/snapshots").join(snap_name);
        let snap_text = std::fs::read_to_string(&snap_path).unwrap();
        // Strip insta's `---\nsource: …\nexpression: …\n---\n` header.
        let body = snap_text
            .splitn(2, "---\n")
            .nth(1)
            .unwrap()
            .splitn(2, "---\n")
            .nth(1)
            .unwrap();
        assert_eq!(
            json, body,
            "{path}'s recompiled artifact must stay byte-identical to {snap_name} (B2)"
        );
    }
}
