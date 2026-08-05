//! Task 7b — an imported component body's content lines go through the SAME
//! content-line attribute checker (`content_line::check_content_line_attrs`,
//! dsl 0.1.0 §7.1, 0.2.2 §D7, 0.9.0 D-C) a scene-level line does.
//!
//! Before this suite existed, `walk_component_body`'s `Node::Line` arm called
//! only `body_attr_refs` + `component_interp_scan`, so EVERY content-line
//! attribute rule — unknown key, `emotion`/`action` domain membership,
//! undeclared domain, and the delivery-flag rules — was silently skipped
//! inside a component. That let undeclared vocabulary reach the compiled
//! artifact through a `::use`, falsifying 0.9.0's headline invariant.
//!
//! The load-bearing test here is [`scene_component_parity`]: it asserts the
//! SAME line yields the SAME diagnostic codes on all THREE paths — at scene
//! level, in a component FILE checked standalone, and in that same component
//! body reached through a `::use` — so a future one-sided change fails loudly
//! (the resolution-drift-is-a-bug-class spirit of `lute-lsp/tests/divergence.rs`,
//! and the shape its four sibling suites already use).
//!
//! Harness: temp-dir component files resolved through `resolve_components` —
//! the SAME resolver the CLI/LSP call — mirroring `tests/component_match.rs`.
use lute_check::{
    check, parse_meta, resolve_components, CheckInput, ComponentSet, Mode, SchemaImports,
};
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
    let dir = std::env::temp_dir().join(format!("lute_ccl_{}_{}_{}", std::process::id(), n, nanos));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const HDR: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n";

/// The importing scene: nothing but a `::use` of the paramless component, so
/// the ONLY diagnostics it can contribute are the component body's own.
const USE_SCENE: &str =
    "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\ncomponents: [c.lute]\n---\n\
## Shot 1.\n::use{component=\"c\"}\n";

/// One `check()` run over `text`, with `components` already resolved. The three
/// path helpers below differ ONLY in what they hand this.
fn check_codes(
    text: String,
    components: ComponentSet,
    snapshot: CapabilitySnapshot,
) -> Vec<String> {
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
    sorted(check(&input).diagnostics.into_iter().map(|d| d.code))
}

/// A paramless component FILE carrying `line` — the same source text both the
/// standalone and the `::use` leg check, so the only variable between them is
/// which root walks it.
fn component_file(line: &str) -> String {
    format!("---\ncomponent: c\n---\n## Scene 1.\n{line}\n")
}

/// `line` sitting directly in a scene body — the reference behaviour every
/// assertion below is measured against.
fn scene_codes(line: &str, snapshot: CapabilitySnapshot) -> Vec<String> {
    check_codes(format!("{HDR}{line}\n"), Default::default(), snapshot)
}

/// The SAME `line` in a component FILE checked STANDALONE — no importing scene,
/// the component document walked as its own root. This leg is not decoration:
/// of the five bypasses this branch closed, the content-line one is the one
/// that CORRUPTED THE ARTIFACT (undeclared vocabulary reached `lute compile`),
/// so a scene-vs-`::use` comparison alone would stay green through exactly the
/// regression that costs most — a standalone root that stops calling
/// `check_content_line_attrs`.
fn standalone_codes(line: &str, snapshot: CapabilitySnapshot) -> Vec<String> {
    check_codes(component_file(line), Default::default(), snapshot)
}

/// The SAME `line` sitting in a paramless component body, reached through a
/// `::use` from a scene — i.e. the imported-component path.
fn component_codes(line: &str, snapshot: CapabilitySnapshot) -> Vec<String> {
    let dir = unique_dir();
    std::fs::write(dir.join("c.lute"), component_file(line)).unwrap();
    let text = USE_SCENE.to_string();
    let (doc, _) = lute_syntax::parse(&text);
    let (meta0, _) = parse_meta(&doc.meta, &CapabilitySnapshot::default());
    let components = resolve_components(&dir, &meta0.components, doc.meta.span);
    check_codes(text, components, snapshot)
}

fn sorted(codes: impl Iterator<Item = String>) -> Vec<String> {
    let mut v: Vec<String> = codes.collect();
    v.sort();
    v
}

/// Every line whose diagnostics must be identical on both paths, one per rule
/// family `check_content_line_attrs` owns. A paramless, interp-free,
/// `code=`-free line so the ONLY diagnostics either path can produce are the
/// content-line attribute ones (no `E-DUP-LINE-CODE`, no `E-MAYBE-UNSET`, no
/// `E-COMPONENT-STATE`) — anything else showing up is itself a finding.
const PARITY_LINES: &[&str] = &[
    // unknown key (`pose` is not a content-line attribute — dsl 0.1.0 §7.1)
    "@bianca{pose=\"idle\"}: hi",
    // closed-domain non-member (`emotion`, 0.9.0 D-C)
    "@bianca{emotion=\"TOTALLY-UNDECLARED\"}: hi",
    // narrator takes no delivery (dsl 0.1.0 §12.1)
    "@narrator{mono}: hi",
    // at most one delivery flag (dsl 0.2.2 §D7)
    "@bianca{mono os vo}: hi",
    // a valued delivery flag is malformed, not a second flag (§D7)
    "@bianca{mono=\"yes\"}: hi",
    // all four families at once, so the whole code MULTISET is compared, not
    // one family in isolation (both sides go through `sorted`, so this pins
    // the multiset — relative ORDER is deliberately not asserted: the checker
    // promises no per-rule emission order, only `check()`'s final
    // byte-offset-then-code sort, which a component body's collapsed span
    // makes meaningless anyway)
    "@narrator{pose=\"idle\" emotion=\"nope\" mono os}: hi",
    // clean line — parity must hold for the empty diagnostic set as well
    "@bianca{emotion=\"neutral\" variant=\"a\" vo}: hi",
];

/// The load-bearing invariant: scene level, a standalone component file, and a
/// component body reached through a `::use` agree, code for code, on every line
/// above — all three sorted multisets, so a bypass on ANY of the three paths
/// fails here.
#[test]
fn scene_component_parity() {
    for line in PARITY_LINES {
        let at_scene = scene_codes(line, vocab_snapshot());
        assert_eq!(
            at_scene,
            standalone_codes(line, vocab_snapshot()),
            "content-line diagnostics diverge between scene level and a standalone \
             component file for `{line}`"
        );
        assert_eq!(
            at_scene,
            component_codes(line, vocab_snapshot()),
            "content-line diagnostics diverge between scene level and a component \
             body reached through a `::use` for `{line}`"
        );
    }
}

#[test]
fn unknown_attr_in_component_body() {
    let cs = component_codes("@bianca{pose=\"idle\"}: hi", vocab_snapshot());
    assert!(
        cs.contains(&"E-UNKNOWN-ATTR".to_string()),
        "`pose` is not a content-line attribute; a component body must flag it: {cs:?}"
    );
}

#[test]
fn bad_enum_in_component_body() {
    let cs = component_codes(
        "@bianca{emotion=\"TOTALLY-UNDECLARED\"}: hi",
        vocab_snapshot(),
    );
    assert!(
        cs.contains(&"E-BAD-ENUM".to_string()),
        "a non-member `emotion` inside a component body must flag E-BAD-ENUM: {cs:?}"
    );
    let clean = component_codes("@bianca{emotion=\"neutral\"}: hi", vocab_snapshot());
    assert!(
        !clean.contains(&"E-BAD-ENUM".to_string()),
        "a declared member must stay clean: {clean:?}"
    );
}

/// dsl 0.9.0 D-C: a domain slot with no declared domain is an ERROR. Runs
/// against the BARE CORE (which declares no `action` domain), matching
/// `tests/content_line.rs::undeclared_action_domain_is_an_error` — against
/// `vocab_snapshot()` this would only prove membership.
#[test]
fn undeclared_domain_in_component_body() {
    let bare = lute_manifest::core::load_core_snapshot();
    let cs = component_codes("@bianca{action=\"wave\"}: hi", bare.clone());
    assert!(
        cs.contains(&"E-DOMAIN-UNKNOWN".to_string()),
        "an undeclared `action` domain inside a component body must flag E-DOMAIN-UNKNOWN: {cs:?}"
    );
    assert_eq!(
        scene_codes("@bianca{action=\"wave\"}: hi", bare.clone()),
        cs,
        "the undeclared-domain verdict must not depend on where the line sits"
    );
}

#[test]
fn delivery_rules_in_component_body() {
    let narrator = component_codes("@narrator{mono}: hi", vocab_snapshot());
    assert!(
        narrator.contains(&"E-DELIVERY-NARRATOR".to_string()),
        "`mono` on `narrator` inside a component body must flag E-DELIVERY-NARRATOR: {narrator:?}"
    );
    let conflict = component_codes("@bianca{mono os vo}: hi", vocab_snapshot());
    assert_eq!(
        conflict
            .iter()
            .filter(|c| *c == "E-DELIVERY-CONFLICT")
            .count(),
        3,
        "three conflicting delivery flags must each be flagged: {conflict:?}"
    );
}

/// The gate must close for a line nested inside an admitted param-scoped
/// `<match>` arm too (dsl 0.4.0 §6.2) — the recursive `walk_component_body`
/// call, not just the top level.
#[test]
fn nested_match_arm_line_is_checked() {
    let dir = unique_dir();
    std::fs::write(
        dir.join("c.lute"),
        "---\ncomponent: c\nparams:\n  tier: { enum: [cold, warm] }\n---\n## Scene 1.\n\
<match on=\"@tier\">\n\
<when is=\"warm\">\n@bianca{emotion=\"TOTALLY-UNDECLARED\"}: hi\n</when>\n\
<when is=\"cold\">\n@bianca{pose=\"idle\"}: hi\n</when>\n\
</match>\n",
    )
    .unwrap();
    let text = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\ncomponents: [c.lute]\n---\n\
## Shot 1.\n::use{component=\"c\" tier=\"warm\"}\n"
        .to_string();
    let (doc, _) = lute_syntax::parse(&text);
    let (meta0, _) = parse_meta(&doc.meta, &CapabilitySnapshot::default());
    let components = resolve_components(&dir, &meta0.components, doc.meta.span);
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
    let cs = sorted(check(&input).diagnostics.into_iter().map(|d| d.code));
    assert!(
        cs.contains(&"E-BAD-ENUM".to_string()),
        "a match-arm line's bad `emotion` must be flagged: {cs:?}"
    );
    assert!(
        cs.contains(&"E-UNKNOWN-ATTR".to_string()),
        "a match-arm line's unknown attr must be flagged: {cs:?}"
    );
}

/// A plugin-declared cross-cutting `stampAttrs` key is admissible on a content
/// line (plugin §14.1) — the component path must honour that allowance too,
/// not merely the `E-UNKNOWN-ATTR` half of the rule.
#[test]
fn stamp_attr_is_admitted_in_component_body() {
    let mut snap = vocab_snapshot();
    snap.stamp_attrs.insert(
        "beat".to_string(),
        lute_manifest::schema::AttrDecl {
            name: "beat".to_string(),
            ty: lute_manifest::types::Type::Str,
            required: false,
            default: None,
        },
    );
    let cs = component_codes("@bianca{beat=\"b1\"}: hi", snap.clone());
    assert!(
        !cs.contains(&"E-UNKNOWN-ATTR".to_string()),
        "a declared cross-cutting stampAttr must not be E-UNKNOWN-ATTR in a component body: {cs:?}"
    );
    assert_eq!(
        scene_codes("@bianca{beat=\"b1\"}: hi", snap),
        cs,
        "the stampAttrs allowance must not depend on where the line sits"
    );
}
