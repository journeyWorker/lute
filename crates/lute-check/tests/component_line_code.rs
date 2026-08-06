//! Task 7c — an imported component body's content lines go through the SAME
//! duplicate-line-code pass (`match_check::check_line_codes`, dsl §12) a
//! scene-level line does.
//!
//! Before this suite existed, `check_line_codes` had exactly ONE callsite
//! (`check.rs`, over the ROOT document) and `validate_components` never ran
//! it, so two identical `(speaker, code)` content lines inside a component
//! body passed `lute check` through a `::use` — exit 0 — while the same pair
//! errored `E-DUP-LINE-CODE` at scene level AND when the component was checked
//! standalone. `lute compile` then emitted two records carrying the identical
//! `lineId`, and `lineId` is the voice-key / i18n identity spine: two lines
//! colliding on one id collide on one voice asset.
//!
//! The load-bearing test here is [`dup_line_code_three_way_parity`]: the SAME
//! duplicate pair must report `E-DUP-LINE-CODE` at scene level, standalone,
//! and through a `::use`, so a future one-sided change fails loudly (the
//! resolution-drift-is-a-bug-class spirit of `lute-lsp/tests/divergence.rs`,
//! and the direct sibling of `component_content_line.rs`'s
//! `scene_component_parity`).
//!
//! SCOPE (see the comment at the fix in `check.rs::validate_components`): this
//! pass checks uniqueness WITHIN one component body. Post-expansion identity
//! across a component `::use`d twice is a separate, PRE-EXISTING 0.8.0
//! language property — a single perfectly valid `code="0010"` line in a
//! component `::use`d twice already compiles to two records with one `lineId`
//! today — and is deliberately not asserted here.
//!
//! Harness: temp-dir component files resolved through `resolve_components` —
//! the SAME resolver the CLI/LSP call — mirroring `tests/component_match.rs`
//! and `tests/component_content_line.rs`.
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
    let dir = std::env::temp_dir().join(format!("lute_clc_{}_{}_{}", std::process::id(), n, nanos));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const HDR: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n";

/// The importing scene: nothing but a `::use` of the paramless component, so
/// the ONLY diagnostics it can contribute are the component body's own.
const USE_SCENE: &str =
    "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\ncomponents: [c.lute]\n---\n\
## Shot 1.\n::use{component=\"c\"}\n";

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

/// `body` sitting in a paramless component file checked STANDALONE (no
/// importing scene at all) — the component document walked through
/// `Walker::walk` as its own root.
fn standalone_diags(body: &str) -> Vec<Diagnostic> {
    check_text(
        format!("---\ncomponent: c\n---\n## Scene 1.\n{body}"),
        Default::default(),
    )
}

/// The SAME `body` in a paramless component body, reached through a `::use`
/// from a scene — i.e. the imported-component path.
fn component_diags(body: &str) -> Vec<Diagnostic> {
    let dir = unique_dir();
    std::fs::write(
        dir.join("c.lute"),
        format!("---\ncomponent: c\n---\n## Scene 1.\n{body}"),
    )
    .unwrap();
    let text = USE_SCENE.to_string();
    let (doc, _) = lute_syntax::parse(&text);
    let (meta0, _) = parse_meta(&doc.meta, &CapabilitySnapshot::default());
    let components = resolve_components(&dir, &meta0.components, doc.meta.span);
    check_text(text, components)
}

fn codes(diags: &[Diagnostic]) -> Vec<String> {
    let mut v: Vec<String> = diags.iter().map(|d| d.code.clone()).collect();
    v.sort();
    v
}

/// Two `:line`s for one speaker carrying one `code` — the pair whose
/// `lineId`/`voiceKey` collide (dsl §12).
const DUP_BODY: &str = "@bianca{code=\"0010\"}: one\n@bianca{code=\"0010\"}: two\n";

/// The load-bearing invariant: the same duplicate pair reports the same code
/// on all three paths.
#[test]
fn dup_line_code_three_way_parity() {
    for (label, diags) in [
        ("scene", scene_diags(DUP_BODY)),
        ("standalone component", standalone_diags(DUP_BODY)),
        ("component via ::use", component_diags(DUP_BODY)),
    ] {
        assert!(
            codes(&diags).contains(&"E-DUP-LINE-CODE".to_string()),
            "the duplicate (bianca, 0010) pair must report E-DUP-LINE-CODE at {label}, got {:?}",
            codes(&diags)
        );
    }
}

/// The gap this suite closes: reached through a `::use`, EXACTLY ONE
/// `E-DUP-LINE-CODE` (the second occurrence), re-anchored with the component
/// name + source path the way every other component-body diagnostic is.
#[test]
fn dup_line_code_in_component_body() {
    let diags = component_diags(DUP_BODY);
    let dups: Vec<_> = diags
        .iter()
        .filter(|d| d.code == "E-DUP-LINE-CODE")
        .collect();
    assert_eq!(
        dups.len(),
        1,
        "exactly one E-DUP-LINE-CODE for the one repeated pair, got {:?}",
        codes(&diags)
    );
    let m = &dups[0].message;
    assert!(
        m.starts_with("component `c` (") && m.contains("): duplicate `:line` `code=\"0010\"`"),
        "component body diagnostics carry the component re-anchoring prefix, got {m:?}"
    );
    assert!(
        m.contains("for speaker `bianca`"),
        "the message names the colliding speaker, got {m:?}"
    );
}

/// The check is scoped to ONE body, keyed on `(speaker, trimmed code)` — a
/// distinct code and a distinct speaker are both clean, and a code differing
/// only in surrounding whitespace still collides (`code.trim()` is the
/// addressing pass's key).
#[test]
fn distinct_pairs_in_component_body_stay_clean() {
    let clean = "@bianca{code=\"0010\"}: one\n@bianca{code=\"0020\"}: two\n\
                 @fixer{code=\"0010\"}: three\n";
    assert!(
        !codes(&component_diags(clean)).contains(&"E-DUP-LINE-CODE".to_string()),
        "distinct (speaker, code) pairs must not collide, got {:?}",
        codes(&component_diags(clean))
    );
    let trimmed = "@bianca{code=\"0010\"}: one\n@bianca{code=\" 0010 \"}: two\n";
    assert!(
        codes(&component_diags(trimmed)).contains(&"E-DUP-LINE-CODE".to_string()),
        "codes are compared TRIMMED, so ` 0010 ` collides with `0010`"
    );
}

/// Each component body is its OWN identity scope: the same `(speaker, code)`
/// pair appearing once in each of two components is clean, exactly as the same
/// pair across two `<quest>`s is (dsl 0.2.0 §7).
#[test]
fn each_component_body_is_its_own_scope() {
    let dir = unique_dir();
    for name in ["a", "b"] {
        std::fs::write(
            dir.join(format!("{name}.lute")),
            format!("---\ncomponent: {name}\n---\n## Scene 1.\n@bianca{{code=\"0010\"}}: hi\n"),
        )
        .unwrap();
    }
    let text = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n\
                components: [a.lute, b.lute]\n---\n## Shot 1.\n\
                ::use{component=\"a\"}\n::use{component=\"b\"}\n"
        .to_string();
    let (doc, _) = lute_syntax::parse(&text);
    let (meta0, _) = parse_meta(&doc.meta, &CapabilitySnapshot::default());
    let components = resolve_components(&dir, &meta0.components, doc.meta.span);
    let diags = check_text(text, components);
    assert!(
        !codes(&diags).contains(&"E-DUP-LINE-CODE".to_string()),
        "two components each using (bianca, 0010) once must not collide, got {:?}",
        codes(&diags)
    );
}

/// The pass must see lines nested inside an admitted param-scoped `<match>`
/// arm too (dsl 0.4.0 §6.2) — `collect_lines` recurses, but only if the pass
/// is reached at all.
#[test]
fn dup_line_code_across_match_arms_in_component_body() {
    let dir = unique_dir();
    std::fs::write(
        dir.join("c.lute"),
        "---\ncomponent: c\nparams:\n  tier: { enum: [cold, warm, fond] }\n---\n\
         ## Scene 1.\n<match on=\"@tier\">\n<when is=\"fond\">\n@bianca{code=\"0010\"}: a\n\
         </when>\n<otherwise>\n@bianca{code=\"0010\"}: b\n</otherwise>\n</match>\n",
    )
    .unwrap();
    let text = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n\
                components: [c.lute]\n---\n## Shot 1.\n\
                ::use{component=\"c\" tier=\"fond\"}\n"
        .to_string();
    let (doc, _) = lute_syntax::parse(&text);
    let (meta0, _) = parse_meta(&doc.meta, &CapabilitySnapshot::default());
    let components = resolve_components(&dir, &meta0.components, doc.meta.span);
    let diags = check_text(text, components);
    assert!(
        codes(&diags).contains(&"E-DUP-LINE-CODE".to_string()),
        "a (speaker, code) pair repeated across two <match> arms of one component \
         body still collides, got {:?}",
        codes(&diags)
    );
}
