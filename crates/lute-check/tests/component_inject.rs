//! Task 7g — an imported component body's staging goes through the SAME
//! injection fold (`inject::lower_node`, dsl §10.3) a scene-level directive
//! does, so `W-INJECT-CONFLICT` is derived for a body reached through a
//! `::use`.
//!
//! Before this suite existed, `fold_injections` had exactly ONE callsite
//! (`check.rs` step 7, over the ROOT document's shots) and a `::use` was
//! OPAQUE to it: the directive folded as an unknown node and its body was
//! never entered. Measured on the same authored construct:
//!
//! ```text
//! at scene level       -> [W-INJECT-CONFLICT]
//! component standalone -> [W-INJECT-CONFLICT]
//! via ::use            -> []                    <-- the gap
//! ```
//!
//! `lute compile`'s own CFG walk DID derive it (it folds the NORMALIZED tree,
//! where the body is already inlined) but threw it away at
//! `lute-compile/src/lib.rs`'s `state.diags.clear()`, on a premise —
//! "check() already reported it" — that was true for root-level content and
//! FALSE for a component body. So the warning was reported by no tool at all.
//!
//! THE SEAM (the one decision here): the fix folds the body IN DOCUMENT
//! POSITION, threading the stage state INHERITED at the `::use` site, which is
//! exactly the context `lute compile` folds it in post-normalization. It is
//! NOT a per-body fold against a fresh `StageState`: the injection reducer's
//! entrance rules are stage-state dependent (`lower_auto` returns early when
//! the character is ALREADY on stage), so an empty-state body fold would
//! INVENT conflicts that do not exist in context —
//! [`use_site_inherits_stage_state_no_invented_conflict`] pins that.
//!
//! Harness: temp-dir component files resolved through `resolve_components` —
//! the SAME resolver the CLI/LSP call — mirroring `tests/component_line_code.rs`
//! (Task 7c) and `tests/component_reachability.rs` (Task 7e).

use lute_check::{
    check, parse_meta, resolve_components, CheckInput, ComponentSet, Mode, SchemaImports,
};
use lute_core_span::{Diagnostic, Severity};
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
        .join(format!("lute_cinj_{}_{}_{}", std::process::id(), n, nanos));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const HDR: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n";

/// The importing scene: nothing but a `::use` of the paramless component, so
/// the ONLY diagnostics it can contribute are the component body's own.
const USE_SCENE: &str =
    "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\ncomponents: [c.lute]\n---\n\
## Shot 1.\n::use{component=\"c\"}\n";

/// `::auto` whose explicit `anchor` EQUALS the `anchor` domain's declared
/// `default:` (`center`, `lute_test_vocab::test_domains`) — the one authored
/// shape `auto-anchor-on-show` reports `W-INJECT-CONFLICT` for.
const CONFLICT_BODY: &str = "::auto{character=\"bianca\" anchor=\"center\"}\n@bianca: Hello.\n";

fn result_for(text: String, components: ComponentSet) -> lute_check::CheckResult {
    let input = CheckInput {
        text,
        uri: "scene".into(),
        snapshot: vocab_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components,
    };
    check(&input)
}

fn check_text(text: String, components: ComponentSet) -> Vec<Diagnostic> {
    result_for(text, components).diagnostics
}

/// `body` sitting directly in a scene body — the reference behaviour.
fn scene_diags(body: &str) -> Vec<Diagnostic> {
    check_text(format!("{HDR}{body}"), Default::default())
}

/// `body` in a paramless component file checked STANDALONE (no importing scene
/// at all) — the component document folded as its own root.
fn standalone_diags(body: &str) -> Vec<Diagnostic> {
    check_text(
        format!("---\ncomponent: c\n---\n## Scene 1.\n{body}"),
        Default::default(),
    )
}

/// Resolve `files` (name -> full text) in a fresh temp dir and check `scene`
/// against them.
fn check_with_components(scene: &str, files: &[(&str, String)]) -> lute_check::CheckResult {
    let dir = unique_dir();
    for (name, text) in files {
        std::fs::write(dir.join(name), text).unwrap();
    }
    let text = scene.to_string();
    let (doc, _) = lute_syntax::parse(&text);
    let (meta0, _) = parse_meta(&doc.meta, &CapabilitySnapshot::default());
    let components = resolve_components(&dir, &meta0.components, doc.meta.span);
    result_for(text, components)
}

/// The SAME `body` in a paramless component body, reached through a `::use`
/// from a scene — i.e. the imported-component path.
fn component_diags(body: &str) -> Vec<Diagnostic> {
    check_with_components(
        USE_SCENE,
        &[("c.lute", format!("---\ncomponent: c\n---\n## Scene 1.\n{body}"))],
    )
    .diagnostics
}

fn codes(diags: &[Diagnostic]) -> Vec<String> {
    let mut v: Vec<String> = diags.iter().map(|d| d.code.clone()).collect();
    v.sort();
    v
}

fn conflicts(diags: &[Diagnostic]) -> Vec<&Diagnostic> {
    diags.iter().filter(|d| d.code == "W-INJECT-CONFLICT").collect()
}

/// The load-bearing invariant: the same authored construct reports the same
/// code on all three paths.
#[test]
fn inject_conflict_three_way_parity() {
    let scene = codes(&scene_diags(CONFLICT_BODY));
    let standalone = codes(&standalone_diags(CONFLICT_BODY));
    let component = codes(&component_diags(CONFLICT_BODY));
    assert_eq!(
        scene,
        vec!["W-INJECT-CONFLICT".to_string()],
        "scene-level reference behaviour"
    );
    assert_eq!(scene, standalone, "standalone component must match scene level");
    assert_eq!(
        scene, component,
        "a component body reached through a `::use` must match scene level: {component:?}"
    );
}

/// Reached through a `::use`, EXACTLY ONE `W-INJECT-CONFLICT`, re-anchored the
/// way every other component-body diagnostic is (component name + source path
/// prefix) — but at the `::use` DIRECTIVE's span, not the scene frontmatter's:
/// the conflict is a property of THIS invocation site (it depends on the stage
/// state inherited there), so the site is the only honest anchor.
#[test]
fn inject_conflict_in_component_body_reported_once() {
    let diags = component_diags(CONFLICT_BODY);
    let cs = conflicts(&diags);
    assert_eq!(cs.len(), 1, "exactly one conflict: {diags:#?}");
    let d = cs[0];
    assert_eq!(d.severity, Severity::Warning);
    assert!(
        d.message.starts_with("component `c` (") && d.message.contains("c.lute): "),
        "re-anchored message must name the component and its source: {}",
        d.message
    );
    assert!(
        d.message.contains("`bianca` is shown with an explicit `anchor=\"center\"`"),
        "the reducer's own wording must survive the prefix: {}",
        d.message
    );
    // The `::use` directive is the last line of `USE_SCENE`.
    let at = USE_SCENE.find("::use").expect("scene has a ::use");
    assert_eq!(
        (d.span.byte_start, d.span.byte_end),
        (at, at + "::use{component=\"c\"}".len()),
        "anchored at the `::use` site"
    );
}

/// A scene-level conflict must NOT become two identical warnings: with no
/// component in play at all the count stays one, and a scene carrying BOTH a
/// root-level conflict and a component-body one reports each exactly once.
#[test]
fn scene_level_conflict_is_not_duplicated() {
    assert_eq!(conflicts(&scene_diags(CONFLICT_BODY)).len(), 1);

    // Root conflict on `takeru`, body conflict on `bianca`: two distinct sites,
    // one warning each.
    let scene = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n\
                 components: [c.lute]\n---\n## Shot 1.\n\
                 ::auto{character=\"takeru\" anchor=\"center\"}\n\
                 ::use{component=\"c\"}\n";
    let diags = check_with_components(
        scene,
        &[(
            "c.lute",
            format!("---\ncomponent: c\n---\n## Scene 1.\n{CONFLICT_BODY}"),
        )],
    )
    .diagnostics;
    let cs = conflicts(&diags);
    assert_eq!(cs.len(), 2, "one per site, never a duplicate: {diags:#?}");
    assert!(
        !cs[0].message.starts_with("component `"),
        "the root-level one is NOT component-prefixed: {}",
        cs[0].message
    );
    assert!(
        cs[1].message.starts_with("component `c` ("),
        "the body one IS: {}",
        cs[1].message
    );
    assert!(
        cs[0].message.contains("`takeru`") && cs[1].message.contains("`bianca`"),
        "each names its own character: {cs:#?}"
    );
}

/// The seam, pinned: the body folds against the stage state INHERITED at the
/// `::use` site. `bianca` is already on stage when the `::use` runs, so
/// `lower_auto` takes its already-staged early return and there is NO
/// conflict — the exact false positive a per-body fold against a fresh
/// `StageState` would invent.
#[test]
fn use_site_inherits_stage_state_no_invented_conflict() {
    let scene = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n\
                 components: [c.lute]\n---\n## Shot 1.\n\
                 ::auto{character=\"bianca\" anchor=\"left\"}\n\
                 ::use{component=\"c\"}\n";
    let diags = check_with_components(
        scene,
        &[(
            "c.lute",
            format!("---\ncomponent: c\n---\n## Scene 1.\n{CONFLICT_BODY}"),
        )],
    )
    .diagnostics;
    assert!(
        conflicts(&diags).is_empty(),
        "an already-staged character re-shown inside the body is not a conflict: {diags:#?}"
    );
}

/// The same threading in the other direction: `::use`ing the SAME conflicting
/// component twice warns ONCE — the first invocation stages `bianca`, so the
/// second takes the already-staged path, exactly as `lute compile`'s walk over
/// the normalized tree does.
#[test]
fn second_use_of_the_same_component_does_not_re_warn() {
    let scene = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n\
                 components: [c.lute]\n---\n## Shot 1.\n\
                 ::use{component=\"c\"}\n::use{component=\"c\"}\n";
    let diags = check_with_components(
        scene,
        &[(
            "c.lute",
            format!("---\ncomponent: c\n---\n## Scene 1.\n{CONFLICT_BODY}"),
        )],
    )
    .diagnostics;
    assert_eq!(conflicts(&diags).len(), 1, "{diags:#?}");
}

/// A nested `::use` (dsl 0.4.0 §6.1 admits one inside a component body) is
/// entered too, and the message names the whole invocation path.
#[test]
fn nested_use_body_conflict_is_reported() {
    let scene = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n\
                 components: [outer.lute, inner.lute]\n---\n## Shot 1.\n\
                 ::use{component=\"outer\"}\n";
    let diags = check_with_components(
        scene,
        &[
            (
                "outer.lute",
                "---\ncomponent: outer\n---\n## Scene 1.\n::use{component=\"inner\"}\n".to_string(),
            ),
            (
                "inner.lute",
                format!("---\ncomponent: inner\n---\n## Scene 1.\n{CONFLICT_BODY}"),
            ),
        ],
    )
    .diagnostics;
    let cs = conflicts(&diags);
    assert_eq!(cs.len(), 1, "{diags:#?}");
    assert!(
        cs[0].message.starts_with("component `outer` (")
            && cs[0].message.contains("component `inner` ("),
        "the message names the whole `::use` path: {}",
        cs[0].message
    );
}

/// A `::use` cycle is `E-COMPONENT-CYCLE` (reported elsewhere); the fold must
/// TERMINATE on it rather than recurse forever — the same stack-discipline
/// guard `insert_shape_fields` uses for self-referential state shapes.
#[test]
fn use_cycle_terminates() {
    let scene = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n\
                 components: [a.lute, b.lute]\n---\n## Shot 1.\n\
                 ::use{component=\"a\"}\n";
    let diags = check_with_components(
        scene,
        &[
            (
                "a.lute",
                "---\ncomponent: a\n---\n## Scene 1.\n::use{component=\"b\"}\n".to_string(),
            ),
            (
                "b.lute",
                "---\ncomponent: b\n---\n## Scene 1.\n::use{component=\"a\"}\n".to_string(),
            ),
        ],
    )
    .diagnostics;
    // The point is that we got here at all; the cycle itself is diagnosed by
    // the component-expansion checker, not by the fold.
    assert!(
        diags.iter().any(|d| d.code.starts_with("E-COMPONENT")),
        "the cycle is still diagnosed: {diags:#?}"
    );
}

/// The fold is the WHOLE pass, not just its diagnostic: a body `::auto` with
/// NO anchor contributes its INJECTED anchor command to the resolved view's
/// `injections` preview, exactly as the same directive at scene level does.
#[test]
fn component_body_injections_reach_the_resolved_view() {
    let body = "::auto{character=\"bianca\"}\n@bianca: Hi.\n";
    let scene_injections = result_for(format!("{HDR}{body}"), Default::default())
        .resolved
        .expect("resolved view")
        .injections;
    let via_use = check_with_components(
        USE_SCENE,
        &[("c.lute", format!("---\ncomponent: c\n---\n## Scene 1.\n{body}"))],
    )
    .resolved
    .expect("resolved view")
    .injections;

    assert!(
        scene_injections
            .iter()
            .any(|c| c.provenance.by == "auto-anchor-on-show"),
        "scene-level reference: {scene_injections:#?}"
    );
    let by: Vec<&str> = via_use.iter().map(|c| c.provenance.by.as_str()).collect();
    let scene_by: Vec<&str> = scene_injections
        .iter()
        .map(|c| c.provenance.by.as_str())
        .collect();
    assert_eq!(
        by, scene_by,
        "the body's injections must match the same content at scene level"
    );
}
