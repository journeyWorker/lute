//! Task 7e — an imported component body goes through the SAME §5.2/§5.3
//! reachability pass (`reachability::check_reachability`, dsl 0.4.0 T4/T5) a
//! scene-level body does.
//!
//! Before this suite existed, `check_reachability` had exactly ONE callsite
//! (`check.rs`, over the ROOT document) and `validate_components` never ran it.
//! `walk_component_body` called `check_match_reach` for `Node::Match` ONLY, so
//! everything else the pass owns escaped inside a component body:
//!
//! * a content line's `when=` guard was never reachability-checked — a
//!   provably-false guard (`E-ARM-DEAD`, dsl 0.4.0 §7.2) authored in a
//!   component checked CLEAN through a `::use` while the identical line errored
//!   at scene level AND when that same component file was checked standalone;
//! * content following an allowed `::end` (`W-CODE-AFTER-END`, dsl 0.8.0) was
//!   never flagged, for the same reason.
//!
//! The load-bearing tests here are the THREE-WAY parity ones
//! ([`dead_line_guard_three_way_parity`], [`code_after_end_three_way_parity`]):
//! the SAME authored line must report the SAME code at scene level, standalone,
//! and through a `::use`. That is the shape a fourth regression fails loudly
//! against — the resolution-drift-is-a-bug-class spirit of
//! `lute-lsp/tests/divergence.rs`, and the direct sibling of
//! `component_line_code.rs`'s `dup_line_code_three_way_parity` and
//! `component_content_line.rs`'s `scene_component_parity`.
//!
//! Harness: temp-dir component files resolved through `resolve_components` —
//! the SAME resolver the CLI/LSP call — mirroring `tests/component_line_code.rs`.
use lute_check::{
    check, parse_meta, resolve_components, CheckInput, ComponentSet, Mode, SchemaImports,
};
use lute_core_span::Diagnostic;
use lute_manifest::provider::ProviderSet;
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_test_vocab::vocab_snapshot;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static UNIQ: AtomicU64 = AtomicU64::new(0);

fn unique_dir() -> PathBuf {
    let n = UNIQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir()
        .join(format!("lute_creach_{}_{}_{}", std::process::id(), n, nanos));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const HDR: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n";

/// The importing scene: nothing but a `::use` of the paramless component, so
/// the ONLY diagnostics it can contribute are the component body's own.
const USE_SCENE: &str =
    "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\ncomponents: [c.lute]\n---\n\
## Shot 1.\n::use{component=\"c\"}\n";

const E_ARM_DEAD: &str = "E-ARM-DEAD";
const W_CODE_AFTER_END: &str = "W-CODE-AFTER-END";

fn check_text(text: String, components: ComponentSet) -> Vec<Diagnostic> {
    let input = CheckInput {
        text,
        uri: "scene".into(),
        snapshot: vocab_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components,
        defaults: Default::default(),
    };
    check(&input).diagnostics
}

/// `body` sitting directly in a scene body — the reference behaviour.
fn scene_diags(body: &str) -> Vec<Diagnostic> {
    check_text(format!("{HDR}{body}"), Default::default())
}

/// `body` in a paramless component FILE checked STANDALONE (no importing scene
/// at all) — the component document walked as its own root, which is the path
/// `check_reachability`'s `folded.typed.component` branch (reachability.rs)
/// already carries component awareness for.
fn standalone_diags(body: &str) -> Vec<Diagnostic> {
    check_text(component_file(body), Default::default())
}

/// The SAME `body` in a paramless component body, reached through a `::use`
/// from a scene — i.e. the imported-component path, the one that leaked.
fn component_diags(body: &str) -> Vec<Diagnostic> {
    let dir = unique_dir();
    std::fs::write(dir.join("c.lute"), component_file(body)).unwrap();
    let text = USE_SCENE.to_string();
    let (doc, _) = lute_syntax::parse(&text);
    let (meta0, _) = parse_meta(&doc.meta, &CapabilitySnapshot::default());
    let components = resolve_components(&dir, &meta0.components, doc.meta.span);
    check_text(text, components)
}

fn component_file(body: &str) -> String {
    format!("---\ncomponent: c\n---\n## Scene 1.\n{body}")
}

fn codes(diags: &[Diagnostic]) -> Vec<String> {
    let mut v: Vec<String> = diags.iter().map(|d| d.code.clone()).collect();
    v.sort();
    v
}

fn count(diags: &[Diagnostic], code: &str) -> usize {
    diags.iter().filter(|d| d.code == code).count()
}

/// A gated line whose guard is a decided-false GROUND comparison (§5.1 R3) —
/// the line can provably never be shown (dsl 0.4.0 §7.2).
const DEAD_GUARD_BODY: &str = "@bianca{when=\"1 > 2\"}: hi\n";

/// An allowed `::end` (a component body may terminate the walk — it is
/// presentational, not a state write) followed by content that can therefore
/// never run (dsl 0.8.0).
const AFTER_END_BODY: &str = "@bianca: bye\n::end\n@bianca: never\n";

// --- the load-bearing three-way parity invariants --------------------------

/// The SAME dead line guard must report `E-ARM-DEAD` on all three paths.
#[test]
fn dead_line_guard_three_way_parity() {
    for (label, diags) in [
        ("scene", scene_diags(DEAD_GUARD_BODY)),
        ("standalone component", standalone_diags(DEAD_GUARD_BODY)),
        ("component via ::use", component_diags(DEAD_GUARD_BODY)),
    ] {
        assert!(
            codes(&diags).contains(&E_ARM_DEAD.to_string()),
            "a provably-false line guard must report E-ARM-DEAD at {label}, got {:?}",
            codes(&diags)
        );
    }
}

/// The SAME content-after-`::end` must report `W-CODE-AFTER-END` on all three
/// paths.
#[test]
fn code_after_end_three_way_parity() {
    for (label, diags) in [
        ("scene", scene_diags(AFTER_END_BODY)),
        ("standalone component", standalone_diags(AFTER_END_BODY)),
        ("component via ::use", component_diags(AFTER_END_BODY)),
    ] {
        assert!(
            codes(&diags).contains(&W_CODE_AFTER_END.to_string()),
            "content after an allowed `::end` must report W-CODE-AFTER-END at {label}, got {:?}",
            codes(&diags)
        );
    }
}

/// Full code-MULTISET parity, not just membership: the three paths must agree
/// on the whole diagnostic set for both bodies, so a pass that starts
/// over-reporting inside a component body fails here too.
#[test]
fn three_way_code_multiset_parity() {
    for body in [DEAD_GUARD_BODY, AFTER_END_BODY] {
        let at_scene = codes(&scene_diags(body));
        assert_eq!(
            at_scene,
            codes(&standalone_diags(body)),
            "scene level and a standalone component file diverge for `{body}`"
        );
        assert_eq!(
            at_scene,
            codes(&component_diags(body)),
            "scene level and a component body reached through a `::use` diverge for `{body}`"
        );
    }
}

// --- the gap, from the imported side --------------------------------------

/// Reached through a `::use`: EXACTLY ONE `E-ARM-DEAD`, re-anchored with the
/// component name + source path the way every other component-body diagnostic
/// is.
#[test]
fn dead_line_guard_in_component_body() {
    let diags = component_diags(DEAD_GUARD_BODY);
    assert_eq!(
        count(&diags, E_ARM_DEAD),
        1,
        "exactly one E-ARM-DEAD for one dead line guard, got {:?}",
        codes(&diags)
    );
    let d = diags.iter().find(|d| d.code == E_ARM_DEAD).unwrap();
    assert!(
        d.message.starts_with("component `c` (") && d.message.contains("c.lute)"),
        "a component-body diagnostic must carry the component name + source path: {}",
        d.message
    );
    assert!(
        d.fixits.is_empty(),
        "a component-body diagnostic's fixits point into the component file's own byte space \
         and must be cleared: {:?}",
        d.fixits
    );
}

/// Same, for the `::end` warning — one per body, at the first unreachable node.
#[test]
fn code_after_end_in_component_body() {
    let diags = component_diags(AFTER_END_BODY);
    assert_eq!(
        count(&diags, W_CODE_AFTER_END),
        1,
        "exactly one W-CODE-AFTER-END per straight-line body, got {:?}",
        codes(&diags)
    );
    assert!(
        diags
            .iter()
            .find(|d| d.code == W_CODE_AFTER_END)
            .unwrap()
            .message
            .starts_with("component `c` ("),
        "the warning must be re-anchored with the component name"
    );
}

/// The PROVABLE-ONLY boundary (§5.1) holds inside a component body too: a
/// guard that decides TRUE, and an `::end` with nothing after it, are both
/// clean — the pass must not start flagging live content once it runs here.
#[test]
fn live_guard_and_terminal_end_in_component_body_stay_clean() {
    for body in [
        "@bianca{when=\"1 < 2\"}: hi\n",
        "@bianca: bye\n::end\n",
        "@bianca: hi\n",
    ] {
        let diags = component_diags(body);
        assert!(
            codes(&diags).is_empty(),
            "`{body}` in a component body must be clean, got {:?}",
            codes(&diags)
        );
    }
}

/// The pass must reach lines nested inside an admitted param-scoped `<match>`
/// arm (dsl 0.4.0 §6.2) — `walk_reach` recurses into arm bodies, but only if
/// the pass is reached at all.
#[test]
fn dead_line_guard_inside_match_arm_in_component_body() {
    let dir = unique_dir();
    std::fs::write(
        dir.join("c.lute"),
        "---\ncomponent: c\nparams:\n  tier: { enum: [cold, warm] }\n---\n\
## Scene 1.\n\
<match on=\"@tier\">\n\
<when is=\"warm\">\n@bianca{when=\"1 > 2\"}: hi\n</when>\n\
<otherwise>\n@bianca: ok\n</otherwise>\n\
</match>\n",
    )
    .unwrap();
    let text = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\ncomponents: [c.lute]\n---\n\
## Shot 1.\n::use{component=\"c\" tier=\"warm\"}\n"
        .to_string();
    let (doc, _) = lute_syntax::parse(&text);
    let (meta0, _) = parse_meta(&doc.meta, &CapabilitySnapshot::default());
    let components = resolve_components(&dir, &meta0.components, doc.meta.span);
    let diags = check_text(text, components);
    assert_eq!(
        count(&diags, E_ARM_DEAD),
        1,
        "a dead line guard inside an admitted `<match>` arm must report exactly one E-ARM-DEAD, \
         got {:?}",
        codes(&diags)
    );
}

/// Ownership regression guard: reachability for a component-body `<match>` has
/// exactly ONE owner. `walk_component_body`'s `Node::Match` arm used to call
/// `check_match_reach` itself; that call moved into the whole-body
/// `check_reachability` run, so a dead `<when test>` must still be reported
/// exactly once — never twice.
#[test]
fn dead_arm_guard_in_component_match_reported_once() {
    let dir = unique_dir();
    std::fs::write(
        dir.join("c.lute"),
        "---\ncomponent: c\nparams:\n  tier: { enum: [cold, warm] }\n---\n\
## Scene 1.\n\
<match on=\"@tier\">\n\
<when test=\"1 > 2\">\n@bianca: hi\n</when>\n\
<otherwise>\n@bianca: ok\n</otherwise>\n\
</match>\n",
    )
    .unwrap();
    let text = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\ncomponents: [c.lute]\n---\n\
## Shot 1.\n::use{component=\"c\" tier=\"warm\"}\n"
        .to_string();
    let (doc, _) = lute_syntax::parse(&text);
    let (meta0, _) = parse_meta(&doc.meta, &CapabilitySnapshot::default());
    let components = resolve_components(&dir, &meta0.components, doc.meta.span);
    let diags = check_text(text, components);
    assert_eq!(
        count(&diags, E_ARM_DEAD),
        1,
        "a decided-false `<when test>` in a component body must report E-ARM-DEAD exactly once, \
         got {:?}",
        codes(&diags)
    );
}

/// Each component body is its OWN reachability scope: an `::end` in one
/// component says nothing about a sibling component's body (the same per-body
/// scoping `W-CODE-AFTER-END` has within one document, reachability.rs).
#[test]
fn each_component_body_is_its_own_end_scope() {
    let dir = unique_dir();
    std::fs::write(
        dir.join("a.lute"),
        "---\ncomponent: a\n---\n## Scene 1.\n@bianca: bye\n::end\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("b.lute"),
        "---\ncomponent: b\n---\n## Scene 1.\n@bianca: hi\n",
    )
    .unwrap();
    let text = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n\
components: [a.lute, b.lute]\n---\n\
## Shot 1.\n::use{component=\"a\"}\n::use{component=\"b\"}\n"
        .to_string();
    let (doc, _) = lute_syntax::parse(&text);
    let (meta0, _) = parse_meta(&doc.meta, &CapabilitySnapshot::default());
    let components = resolve_components(&dir, &meta0.components, doc.meta.span);
    let diags = check_text(text, components);
    assert!(
        codes(&diags).is_empty(),
        "an `::end` ending component `a`'s body must not flag component `b`'s content, got {:?}",
        codes(&diags)
    );
}
