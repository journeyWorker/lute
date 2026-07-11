//! Task 6 (dsl 0.4.0 §6.1/§6.2) — param-scoped `<match>` admission inside a
//! component body, plus the purity contract's own code `E-COMPONENT-STATE`
//! (narrowing `E-COMPONENT-BODY`). Scenes + component files are written to a
//! temp dir, resolved via `resolve_components` (the SAME resolver the
//! CLI/LSP call), and validated through the assembled `check()` — mirrors
//! `tests/components_use.rs`'s harness.
use lute_check::{check, parse_meta, resolve_components, CheckInput, Mode};
use lute_manifest::core::load_core_snapshot;
use lute_manifest::provider::ProviderSet;
use lute_manifest::schema::{AttrDecl, DirectiveDecl, DirectiveState, Lowering, SlotDecl};
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_manifest::types::{PathSegment, Type};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static UNIQ: AtomicU64 = AtomicU64::new(0);

fn unique_dir() -> PathBuf {
    let n = UNIQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!(
        "lute_match_{}_{}_{}",
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

fn scene(components: &str, body: &str) -> String {
    format!(
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\ncomponents: [{components}]\n---\n## Shot 1.\n{body}\n"
    )
}

/// A `scene()` variant taking extra frontmatter lines (e.g. `state:`/`defs:`),
/// inserted before the closing `---`.
fn scene_with(components: &str, extra_frontmatter: &str, body: &str) -> String {
    format!(
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\ncomponents: [{components}]\n{extra_frontmatter}---\n## Shot 1.\n{body}\n"
    )
}

/// Resolve `components:` from `dir` and run the assembled `check()` over the
/// scene text against a CUSTOM capability snapshot; return every diagnostic
/// code (mirrors the CLI/LSP wiring).
fn codes_with_snapshot(dir: &Path, scene: &str, snapshot: CapabilitySnapshot) -> Vec<String> {
    let (doc, _) = lute_syntax::parse(scene);
    let (meta0, _) = parse_meta(&doc.meta, &CapabilitySnapshot::default());
    let components = resolve_components(dir, &meta0.components, doc.meta.span);
    let input = CheckInput {
        text: scene.to_string(),
        uri: "scene".into(),
        snapshot,
        providers: ProviderSet::default(),
        mode: Mode::Ci,
        imports: Default::default(),
        components,
    };
    check(&input)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

/// The core-baseline convenience: no synthetic directive is in play.
fn codes(dir: &Path, scene: &str) -> Vec<String> {
    codes_with_snapshot(dir, scene, load_core_snapshot())
}

/// Register a synthetic directive on a CLONE of the core snapshot (mirrors
/// `tests/domains.rs`'s `codes_with_domain_attr_against` idiom).
fn snapshot_with_directive(decl: DirectiveDecl) -> CapabilitySnapshot {
    let mut snap = load_core_snapshot();
    snap.directives.insert(decl.name.clone(), decl);
    snap
}

/// The §6.5 worked example, verbatim (spec `docs/proposals/scenario-dsl/0.4.0.md`
/// §6.5): a `tier`-enum param dispatching a three-arm reaction, no `<otherwise>`
/// (exhaustiveness is Task 7 — not checked here).
const REACTION: &str = "---\ncomponent: reaction\nparams:\n  tier: { enum: [cold, warm, fond] }\n---\n\
## Scene 1.\n\
<match on=\"@tier\">\n\
<when is=\"fond\">\n@bianca{emotion=\"delighted\"}: You remembered! You actually remembered.\n</when>\n\
<when is=\"warm\">\n@bianca{emotion=\"content\"}: Not bad at all, Mr. Fixer.\n</when>\n\
<when is=\"cold\">\n@bianca{emotion=\"neutral\"}: ...Shall we begin?\n</when>\n\
</match>\n";

#[test]
fn param_match_is_admitted() {
    let dir = unique_dir();
    write_lute(&dir, "reaction.lute", REACTION);
    let s = scene("reaction.lute", "::use{component=\"reaction\" tier=\"fond\"}");
    let cs = codes(&dir, &s);
    assert!(
        !cs.contains(&"E-COMPONENT-BODY".to_string()),
        "the §6.5 reaction body (a `<match on=\"@tier\">`) must NOT flag E-COMPONENT-BODY; got {cs:?}"
    );
    assert!(
        !cs.contains(&"E-COMPONENT-STATE".to_string()),
        "a pure param dispatch must NOT flag E-COMPONENT-STATE; got {cs:?}"
    );
}

#[test]
fn ambient_test_is_component_state() {
    let dir = unique_dir();
    write_lute(
        &dir,
        "reaction.lute",
        "---\ncomponent: reaction\nparams:\n  tier: { enum: [cold, warm, fond] }\n---\n\
## Scene 1.\n\
<match on=\"@tier\">\n\
<when test=\"run.affection > 1\">\n@bianca: hi\n</when>\n\
<otherwise>\n@bianca: bye\n</otherwise>\n\
</match>\n",
    );
    let s = scene("reaction.lute", "::use{component=\"reaction\" tier=\"fond\"}");
    let cs = codes(&dir, &s);
    assert!(
        cs.contains(&"E-COMPONENT-STATE".to_string()),
        "an arm `test=` reading ambient `run.*` state inside an admitted param match must flag E-COMPONENT-STATE; got {cs:?}"
    );
}

#[test]
fn state_subject_is_component_state() {
    let dir = unique_dir();
    write_lute(
        &dir,
        "logic.lute",
        "---\ncomponent: logic\n---\n## Scene 1.\n\
<match on=\"scene.affect.bianca\">\n\
<when test=\"$ >= 1\">\n@narrator: hi\n</when>\n\
<otherwise>\n@narrator: bye\n</otherwise>\n\
</match>\n",
    );
    let s = scene("logic.lute", "::use{component=\"logic\"}");
    let cs = codes(&dir, &s);
    assert!(
        cs.contains(&"E-COMPONENT-STATE".to_string()),
        "a `<match on=\"scene....\">` subject reads ambient state and must flag E-COMPONENT-STATE; got {cs:?}"
    );
    assert!(
        !cs.contains(&"E-COMPONENT-BODY".to_string()),
        "an ambient-state subject is E-COMPONENT-STATE, not E-COMPONENT-BODY; got {cs:?}"
    );
}

#[test]
fn literal_subject_is_component_body() {
    let dir = unique_dir();
    write_lute(
        &dir,
        "logic.lute",
        "---\ncomponent: logic\n---\n## Scene 1.\n\
<match on=\"'fond'\">\n\
<when is=\"fond\">\n@narrator: hi\n</when>\n\
<otherwise>\n@narrator: bye\n</otherwise>\n\
</match>\n",
    );
    let s = scene("logic.lute", "::use{component=\"logic\"}");
    let cs = codes(&dir, &s);
    assert!(
        cs.contains(&"E-COMPONENT-BODY".to_string()),
        "a literal `<match on=\"'fond'\">` subject is not an admitted form and must flag E-COMPONENT-BODY; got {cs:?}"
    );
    assert!(
        !cs.contains(&"E-COMPONENT-STATE".to_string()),
        "a literal subject reads no ambient state — must NOT flag E-COMPONENT-STATE; got {cs:?}"
    );
}

#[test]
fn fact_query_and_now_flag() {
    let dir = unique_dir();
    write_lute(
        &dir,
        "reaction.lute",
        "---\ncomponent: reaction\nparams:\n  tier: { enum: [cold, warm, fond] }\n---\n\
## Scene 1.\n\
<match on=\"@tier\">\n\
<when test=\"holds(inParty(x))\">\n@bianca: hi\n</when>\n\
<when test=\"now() < run.t\">\n@bianca: yo\n</when>\n\
<otherwise>\n@bianca: bye\n</otherwise>\n\
</match>\n",
    );
    let s = scene("reaction.lute", "::use{component=\"reaction\" tier=\"fond\"}");
    let cs = codes(&dir, &s);
    assert!(
        cs.contains(&"E-COMPONENT-STATE".to_string()),
        "a fact query (`holds(...)`) and `now()` inside an admitted match's arm tests must flag E-COMPONENT-STATE; got {cs:?}"
    );
}

#[test]
fn ambient_interp_is_component_state() {
    let dir = unique_dir();
    write_lute(
        &dir,
        "reaction.lute",
        "---\ncomponent: reaction\nparams:\n  tier: { enum: [cold, warm, fond] }\n---\n\
## Scene 1.\n\
<match on=\"@tier\">\n\
<when is=\"fond\">\n@bianca: you have {{run.tip}}\n</when>\n\
<otherwise>\n@bianca: bye\n</otherwise>\n\
</match>\n",
    );
    let s = scene("reaction.lute", "::use{component=\"reaction\" tier=\"fond\"}");
    let cs = codes(&dir, &s);
    assert!(
        cs.contains(&"E-COMPONENT-STATE".to_string()),
        "a `{{{{run.tip}}}}` interpolation in a body line must flag E-COMPONENT-STATE; got {cs:?}"
    );
    assert!(
        !cs.contains(&"E-UNDECLARED".to_string()),
        "the ambient interpolation must NOT surface as the incidental E-UNDECLARED; got {cs:?}"
    );
}

#[test]
fn writing_directive_is_component_state() {
    let dir = unique_dir();
    write_lute(
        &dir,
        "c.lute",
        "---\ncomponent: c\n---\n## Scene 1.\n::checkstate{x=true}\n",
    );
    let s = scene("c.lute", "::use{component=\"c\"}");
    let decl = DirectiveDecl {
        name: "checkstate".to_string(),
        layer: None,
        attrs: vec![AttrDecl {
            name: "x".to_string(),
            required: false,
            ty: Type::Bool,
            default: None,
        }],
        semantics: Vec::new(),
        state: Some(DirectiveState {
            declares: vec![SlotDecl {
                scope: "run".to_string(),
                path: vec![PathSegment::Literal("result".to_string())],
                shape: "number".to_string(),
            }],
        }),
        effects: None,
        bridge: None,
        lower: Lowering::Builtin {
            kind: "builtin".to_string(),
            name: "noop".to_string(),
        },
    };
    let snapshot = snapshot_with_directive(decl);
    let cs = codes_with_snapshot(&dir, &s, snapshot);
    assert!(
        cs.contains(&"E-COMPONENT-STATE".to_string()),
        "a directive whose resolved decl declares `state.declares` writes must flag E-COMPONENT-STATE (D7); got {cs:?}"
    );
}

#[test]
fn effectless_directive_stays_admitted() {
    let dir = unique_dir();
    // `sfx` (lute.core staging.yaml) declares no `state:`/`effects:` — a plain
    // presentational staging directive.
    write_lute(
        &dir,
        "c.lute",
        "---\ncomponent: c\n---\n## Scene 1.\n::sfx{sound=\"drip\" assetId=\"a\" name=\"amb\"}\n",
    );
    let s = scene("c.lute", "::use{component=\"c\"}");
    let cs = codes(&dir, &s);
    assert!(
        !cs.iter().any(|c| c.starts_with("E-")),
        "an effectless staging directive in a component body must stay clean/admitted; got {cs:?}"
    );
}

#[test]
fn set_branch_hub_stay_component_body() {
    let dir = unique_dir();
    write_lute(
        &dir,
        "c.lute",
        "---\ncomponent: c\n---\n## Scene 1.\n\
::set{scene.x = 1}\n\
<branch id=\"b\">\n<choice id=\"c1\" label=\"C1\">\n@narrator: hi\n</choice>\n</branch>\n\
<hub id=\"h\">\n<choice id=\"c2\" label=\"C2\" exit>\n@narrator: hey\n</choice>\n</hub>\n",
    );
    let s = scene("c.lute", "::use{component=\"c\"}");
    let cs = codes(&dir, &s);
    let body_count = cs.iter().filter(|c| c.as_str() == "E-COMPONENT-BODY").count();
    assert!(
        body_count >= 3,
        "`::set`/`<branch>`/`<hub>` in a component body must ALL still flag E-COMPONENT-BODY (unadmitted constructs, dsl §6.2); got {cs:?}"
    );
}

#[test]
fn param_forwarding_use_is_clean() {
    // (a) A forwarded declared param inside an admitted match arm is clean.
    let dir = unique_dir();
    write_lute(
        &dir,
        "inner.lute",
        "---\ncomponent: inner\nparams:\n  tier: { enum: [cold, warm, fond] }\n---\n\
## Scene 1.\n@narrator: inner {{@tier}}\n",
    );
    write_lute(
        &dir,
        "outer.lute",
        "---\ncomponent: outer\nparams:\n  tier: { enum: [cold, warm, fond] }\n---\n\
## Scene 1.\n\
<match on=\"@tier\">\n\
<when is=\"fond\">\n::use{component=\"inner\" tier=@tier}\n</when>\n\
<otherwise>\n@narrator: nothing\n</otherwise>\n\
</match>\n",
    );
    let s = scene(
        "inner.lute, outer.lute",
        "::use{component=\"outer\" tier=\"fond\"}",
    );
    let cs = codes(&dir, &s);
    assert!(
        !cs.iter().any(|c| c.starts_with("E-")),
        "a `::use` forwarding a declared param (`tier=@tier`) inside an admitted match arm must be clean; got {cs:?}"
    );

    // (b) The SAME nested-`::use`-inside-a-match shape still detects a cycle.
    let dir2 = unique_dir();
    write_lute(
        &dir2,
        "a.lute",
        "---\ncomponent: a\nparams:\n  tier: { enum: [x, y] }\n---\n\
## Scene 1.\n\
<match on=\"@tier\">\n\
<when is=\"x\">\n::use{component=\"b\" tier=@tier}\n</when>\n\
<otherwise>\n@narrator: hi\n</otherwise>\n\
</match>\n",
    );
    write_lute(
        &dir2,
        "b.lute",
        "---\ncomponent: b\nparams:\n  tier: { enum: [x, y] }\n---\n\
## Scene 1.\n::use{component=\"a\" tier=@tier}\n",
    );
    let s2 = scene("a.lute, b.lute", "::use{component=\"a\" tier=\"x\"}");
    let cs2 = codes(&dir2, &s2);
    assert!(
        cs2.contains(&"E-COMPONENT-CYCLE".to_string()),
        "a `::use` expansion cycle reached through a param-scoped match arm must still flag E-COMPONENT-CYCLE; got {cs2:?}"
    );
}

#[test]
fn caller_side_state_binding_is_legal() {
    let dir = unique_dir();
    write_lute(&dir, "reaction.lute", REACTION);
    // The CALLER scene owns its own state; binding a state-derived def to the
    // param is exactly the §6.2 invocation surface (the component body itself
    // never reads it — only the caller's `@currentTier` def does).
    let s = scene_with(
        "reaction.lute",
        "state:\n  scene.tier: { type: { enum: [cold, warm, fond] }, default: cold }\n\
defs:\n  currentTier: { type: { enum: [cold, warm, fond] }, cel: \"scene.tier\" }\n",
        "::use{component=\"reaction\" tier=@currentTier}",
    );
    let cs = codes(&dir, &s);
    assert!(
        !cs.iter().any(|c| c.starts_with("E-")),
        "a caller-side state-derived def bound to a component param is legal (§6.2 invocation surface); got {cs:?}"
    );
}
