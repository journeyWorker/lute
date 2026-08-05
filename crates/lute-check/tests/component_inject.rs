//! Task 7g — an imported component body's staging goes through the SAME
//! injection fold (`inject::lower_node`, dsl §10.3) a scene-level directive
//! does, so a diagnostic the fold raises is derived identically for a body
//! reached through a `::use`.
//!
//! dsl 0.10.0 §12.3 (**D-U**) removed `W-INJECT-CONFLICT`, which is the code
//! this suite was originally built around. The invariant it defends is NOT
//! removed — *the same authored construct reports the same diagnostics
//! whichever leg sees it* — so the suite is retargeted onto the injection
//! channel's one surviving code, `E-DOMAIN-UNKNOWN` from
//! `missing_anchor_domain_diag`, and additionally pins the now-SILENT explicit
//! anchor case on all three legs.
//!
//! Before this suite existed, `fold_injections` had exactly ONE callsite
//! (`check.rs` step 7, over the ROOT document's shots) and a `::use` was
//! OPAQUE to it: the directive folded as an unknown node and its body was
//! never entered. Measured on the same authored construct:
//!
//! ```text
//! at scene level       -> [<the fold's diagnostic>]
//! component standalone -> [<the fold's diagnostic>]
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
/// `default:` (`center`, `lute_test_vocab::test_domains`). Until 0.10.0 §12.3
/// this was the one authored shape `W-INJECT-CONFLICT` fired on; it is now
/// SILENT, and [`explicit_default_anchor_is_silent_on_all_three_legs`] pins
/// that it is silent on every leg rather than on one.
const EXPLICIT_ANCHOR_BODY: &str =
    "::auto{character=\"bianca\" anchor=\"center\"}\n@bianca: Hello.\n";

/// `::auto` with NO explicit anchor, checked against a snapshot that declares
/// no `anchor` domain — the injection fold's own implicit vocabulary read, and
/// as of 0.10.0 §12.3 the only diagnostic `StageState::diags` still carries.
/// This is what keeps every three-way test below a real test rather than an
/// assertion that nothing happens twice.
const NO_ANCHOR_DOMAIN_BODY: &str = "::auto{character=\"bianca\"}\n@bianca: Hello.\n";

/// [`vocab_snapshot`] minus the `anchor` domain, re-stamped the way
/// `vocab_snapshot` re-stamps: `enums` is folded into the content hash, so a
/// snapshot whose `enums` we edited must not present the old version.
fn no_anchor_domain_snapshot() -> CapabilitySnapshot {
    let mut snap = vocab_snapshot();
    snap.domains.remove("anchor");
    snap.enums.remove("anchor");
    snap.version = lute_manifest::snapshot::capability_version(&snap);
    snap
}

fn result_for(text: String, components: ComponentSet) -> lute_check::CheckResult {
    result_with(text, components, vocab_snapshot())
}

/// `result_for` with an explicit snapshot, so a fixture can withdraw a domain
/// and exercise the fold's implicit vocabulary read.
fn result_with(
    text: String,
    components: ComponentSet,
    snapshot: CapabilitySnapshot,
) -> lute_check::CheckResult {
    let input = CheckInput {
        text,
        uri: "scene".into(),
        snapshot,
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components,
        defaults: Default::default(),
    };
    check(&input)
}

/// `body` sitting directly in a scene body — the reference behaviour.
fn scene_diags(body: &str, snapshot: CapabilitySnapshot) -> Vec<Diagnostic> {
    result_with(format!("{HDR}{body}"), Default::default(), snapshot).diagnostics
}

/// `body` in a paramless component file checked STANDALONE (no importing scene
/// at all) — the component document folded as its own root.
fn standalone_diags(body: &str, snapshot: CapabilitySnapshot) -> Vec<Diagnostic> {
    result_with(
        format!("---\ncomponent: c\n---\n## Scene 1.\n{body}"),
        Default::default(),
        snapshot,
    )
    .diagnostics
}

/// Resolve `files` (name -> full text) in a fresh temp dir and check `scene`
/// against them.
fn check_with_components(scene: &str, files: &[(&str, String)]) -> lute_check::CheckResult {
    check_with_components_snap(scene, files, vocab_snapshot())
}

fn check_with_components_snap(
    scene: &str,
    files: &[(&str, String)],
    snapshot: CapabilitySnapshot,
) -> lute_check::CheckResult {
    let dir = unique_dir();
    for (name, text) in files {
        std::fs::write(dir.join(name), text).unwrap();
    }
    let text = scene.to_string();
    let (doc, _) = lute_syntax::parse(&text);
    let (meta0, _) = parse_meta(&doc.meta, &CapabilitySnapshot::default());
    let components = resolve_components(&dir, &meta0.components, doc.meta.span);
    result_with(text, components, snapshot)
}

/// The SAME `body` in a paramless component body, reached through a `::use`
/// from a scene — i.e. the imported-component path.
fn component_diags(body: &str, snapshot: CapabilitySnapshot) -> Vec<Diagnostic> {
    check_with_components_snap(
        USE_SCENE,
        &[("c.lute", format!("---\ncomponent: c\n---\n## Scene 1.\n{body}"))],
        snapshot,
    )
    .diagnostics
}

fn codes(diags: &[Diagnostic]) -> Vec<String> {
    let mut v: Vec<String> = diags.iter().map(|d| d.code.clone()).collect();
    v.sort();
    v
}

/// 0.10.0 §12.3 (**D-U**): the code this suite was built around is gone. The
/// invariant it defends is not — the same authored construct must report the
/// SAME diagnostics whichever leg sees it — so the filter now names the
/// injection channel's one surviving code, `E-DOMAIN-UNKNOWN` from the fold's
/// implicit `anchor`-domain read.
fn fold_diags(diags: &[Diagnostic]) -> Vec<&Diagnostic> {
    diags
        .iter()
        .filter(|d| d.code == "E-DOMAIN-UNKNOWN")
        .collect()
}

/// The load-bearing invariant: the same authored construct reports the same
/// code on all three paths.
#[test]
fn injection_fold_three_way_parity() {
    let scene = codes(&scene_diags(NO_ANCHOR_DOMAIN_BODY, no_anchor_domain_snapshot()));
    let standalone = codes(&standalone_diags(
        NO_ANCHOR_DOMAIN_BODY,
        no_anchor_domain_snapshot(),
    ));
    let component = codes(&component_diags(
        NO_ANCHOR_DOMAIN_BODY,
        no_anchor_domain_snapshot(),
    ));
    assert_eq!(
        scene,
        vec!["E-DOMAIN-UNKNOWN".to_string()],
        "scene-level reference behaviour"
    );
    assert_eq!(scene, standalone, "standalone component must match scene level");
    assert_eq!(
        scene, component,
        "a component body reached through a `::use` must match scene level: {component:?}"
    );
}

/// 0.10.0 §12.3 (**D-U**): an explicit anchor EQUAL to the declared default is
/// silent — and silent on every leg, which is a stronger statement of the same
/// parity property than the warning ever made.
#[test]
fn explicit_default_anchor_is_silent_on_all_three_legs() {
    for (leg, diags) in [
        ("scene", scene_diags(EXPLICIT_ANCHOR_BODY, vocab_snapshot())),
        (
            "standalone",
            standalone_diags(EXPLICIT_ANCHOR_BODY, vocab_snapshot()),
        ),
        (
            "via ::use",
            component_diags(EXPLICIT_ANCHOR_BODY, vocab_snapshot()),
        ),
    ] {
        assert!(
            diags.is_empty(),
            "{leg}: an explicit anchor equal to the declared default is silent \
             as of 0.10.0 §12.3; got {:?}",
            codes(&diags)
        );
    }
}

/// Reached through a `::use`, EXACTLY ONE fold diagnostic, re-anchored the
/// way every other component-body diagnostic is (component name + source path
/// prefix) — but at the `::use` DIRECTIVE's span, not the scene frontmatter's:
/// the verdict is a property of THIS invocation site (it depends on the stage
/// state inherited there), so the site is the only honest anchor.
#[test]
fn fold_diag_in_component_body_reported_once() {
    let diags = component_diags(NO_ANCHOR_DOMAIN_BODY, no_anchor_domain_snapshot());
    let cs = fold_diags(&diags);
    assert_eq!(cs.len(), 1, "exactly one fold diagnostic: {diags:#?}");
    let d = cs[0];
    assert_eq!(d.severity, Severity::Error);
    assert!(
        d.message.starts_with("component `c` (") && d.message.contains("c.lute): "),
        "re-anchored message must name the component and its source: {}",
        d.message
    );
    assert!(
        d.message
            .contains("`bianca` is shown without an explicit `anchor`"),
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

/// A scene-level fold diagnostic must NOT become two identical reports: with no
/// component in play at all the count stays one, and a scene carrying BOTH a
/// root-level one and a component-body one reports each exactly once.
#[test]
fn scene_level_fold_diag_is_not_duplicated() {
    assert_eq!(
        fold_diags(&scene_diags(
            NO_ANCHOR_DOMAIN_BODY,
            no_anchor_domain_snapshot()
        ))
        .len(),
        1
    );

    // Root site on `takeru`, body site on `bianca`: two distinct sites, one
    // diagnostic each.
    let scene = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n\
                 components: [c.lute]\n---\n## Shot 1.\n\
                 ::auto{character=\"takeru\"}\n\
                 ::use{component=\"c\"}\n";
    let diags = check_with_components_snap(
        scene,
        &[(
            "c.lute",
            format!("---\ncomponent: c\n---\n## Scene 1.\n{NO_ANCHOR_DOMAIN_BODY}"),
        )],
        no_anchor_domain_snapshot(),
    )
    .diagnostics;
    let cs = fold_diags(&diags);
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
/// `lower_auto` takes its already-staged early return and the body contributes
/// NOTHING — the exact false positive a per-body fold against a fresh
/// `StageState` would invent.
#[test]
fn use_site_inherits_stage_state_no_invented_diag() {
    let scene = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n\
                 components: [c.lute]\n---\n## Shot 1.\n\
                 ::auto{character=\"bianca\"}\n\
                 ::use{component=\"c\"}\n";
    let diags = check_with_components_snap(
        scene,
        &[(
            "c.lute",
            format!("---\ncomponent: c\n---\n## Scene 1.\n{NO_ANCHOR_DOMAIN_BODY}"),
        )],
        no_anchor_domain_snapshot(),
    )
    .diagnostics;
    let cs = fold_diags(&diags);
    assert_eq!(
        cs.len(),
        1,
        "the ROOT `::auto` raises it once; the body's re-show of an already-staged \
         character raises nothing: {diags:#?}"
    );
    assert!(
        !cs[0].message.starts_with("component `"),
        "and the surviving one is the root-level site, not the body's: {}",
        cs[0].message
    );
}

/// The same threading in the other direction: `::use`ing the SAME component
/// twice reports ONCE — the first invocation stages `bianca`, so the second
/// takes the already-staged path, exactly as `lute compile`'s walk over the
/// normalized tree does.
#[test]
fn second_use_of_the_same_component_does_not_re_report() {
    let scene = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n\
                 components: [c.lute]\n---\n## Shot 1.\n\
                 ::use{component=\"c\"}\n::use{component=\"c\"}\n";
    let diags = check_with_components_snap(
        scene,
        &[(
            "c.lute",
            format!("---\ncomponent: c\n---\n## Scene 1.\n{NO_ANCHOR_DOMAIN_BODY}"),
        )],
        no_anchor_domain_snapshot(),
    )
    .diagnostics;
    assert_eq!(fold_diags(&diags).len(), 1, "{diags:#?}");
}

/// A nested `::use` (dsl 0.4.0 §6.1 admits one inside a component body) is
/// entered too, and the message names the whole invocation path.
#[test]
fn nested_use_body_fold_diag_is_reported() {
    let scene = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n\
                 components: [outer.lute, inner.lute]\n---\n## Shot 1.\n\
                 ::use{component=\"outer\"}\n";
    let diags = check_with_components_snap(
        scene,
        &[
            (
                "outer.lute",
                "---\ncomponent: outer\n---\n## Scene 1.\n::use{component=\"inner\"}\n".to_string(),
            ),
            (
                "inner.lute",
                format!("---\ncomponent: inner\n---\n## Scene 1.\n{NO_ANCHOR_DOMAIN_BODY}"),
            ),
        ],
        no_anchor_domain_snapshot(),
    )
    .diagnostics;
    let cs = fold_diags(&diags);
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
