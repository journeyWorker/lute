//! Task 7f — a component document must not carry TOP-LEVEL content the
//! component walker does not process.
//!
//! `validate_components` iterates `body.shots` only, and `check_admission` has
//! exactly ONE callsite (`check.rs`, over the ROOT document). A component
//! file's `doc.quests` therefore reached NEITHER: a top-level `<quest>` in a
//! component file errored `E-GRAMMAR-NOT-ADMITTED` when that file was checked
//! STANDALONE, yet the identical file reached through a `::use` checked
//! clean — exit 0 — and `lute compile` then silently DROPPED the whole
//! `<quest>` (zero records, no trace of its `::set`). So the smuggled state
//! write never executed: this is a silent-drop plus standalone/imported
//! divergence, not an artifact-corrupting purity breach.
//!
//! The rule the fix installs (see `check_component_toplevel` in
//! `src/admission.rs`) is deliberately NOT a component-kind admission table:
//! it is the single statement *anything present at a component document's top
//! level that the component walker does not process is an ERROR, never a
//! silent drop*. Every existing §6 rule is untouched.
//!
//! The load-bearing tests are [`toplevel_quest_three_way_parity`] (the same
//! `<quest>` reports the same code at scene level, standalone, and through a
//! `::use`) and [`component_title_stays_clean_on_every_path`] (the one
//! `Document` field deliberately EXEMPT from the rule stays exempt on all
//! paths, so a future over-broad tightening fails loudly instead of inventing
//! the mirror-image divergence).
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
    let dir = std::env::temp_dir().join(format!("lute_ctc_{}_{}_{}", std::process::id(), n, nanos));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A scene frontmatter whose `state:` DECLARES `run.gold`, so the scene-level
/// leg of the parity tests below cannot be confounded by an `E-UNDECLARED` the
/// component legs never produce (a component env has an empty state schema).
const SCENE_HDR: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  \
                         run.gold: { type: number, default: 0 }\n---\n";

/// The importing scene: nothing but a `::use` of the paramless component, so
/// the ONLY diagnostics it can contribute are the component body's own.
const USE_SCENE: &str =
    "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\ncomponents: [c.lute]\n---\n\
## Shot 1.\n::use{component=\"c\"}\n";

/// The smuggled write: the thing dsl 0.4.0 §6.1 most flatly forbids in a
/// component body, here hidden inside a top-level `<quest>`.
const SMUGGLED_SET: &str = "::set{run.gold = 1}\n";

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

/// `component` is the FULL text of a component file (frontmatter included) —
/// unlike the body-only helpers in the sibling suites, because the construct
/// under test lives at the document top level, OUTSIDE any shot.
fn standalone_diags(component: &str) -> Vec<Diagnostic> {
    check_text(component.to_string(), Default::default())
}

/// The SAME component file reached through a `::use` from a scene — i.e. the
/// imported-component path this suite exists to close.
fn component_diags(component: &str) -> Vec<Diagnostic> {
    let dir = unique_dir();
    std::fs::write(dir.join("c.lute"), component).unwrap();
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

/// A component file whose top-level content is `<quest>`-shaped: the exact
/// document the controller used to reproduce the divergence.
fn quest_component(body: &str) -> String {
    format!("---\ncomponent: c\n---\n<quest id=\"q1\">\n{body}</quest>\n")
}

/// The load-bearing invariant: a top-level `<quest>` reports
/// `E-GRAMMAR-NOT-ADMITTED` on every path it can be reached by. A scene doc
/// admits no top-level `<quest>` either (dsl 0.2.0 §3.3), so all three legs
/// are the same code — the toolchain stops contradicting itself about one
/// construct depending only on how the file was reached.
#[test]
fn toplevel_quest_three_way_parity() {
    let scene =
        format!("{SCENE_HDR}<quest id=\"q1\">\n{SMUGGLED_SET}</quest>\n## Shot 1.\n@x: hi.\n");
    for (label, diags) in [
        ("scene", check_text(scene, Default::default())),
        ("standalone component", standalone_diags(&quest_component(SMUGGLED_SET))),
        ("component via ::use", component_diags(&quest_component(SMUGGLED_SET))),
    ] {
        assert!(
            codes(&diags).contains(&"E-GRAMMAR-NOT-ADMITTED".to_string()),
            "a top-level `<quest>` must report E-GRAMMAR-NOT-ADMITTED at {label}, got {:?}",
            codes(&diags)
        );
    }
}

/// The gap this suite closes: reached through a `::use`, EXACTLY ONE
/// `E-GRAMMAR-NOT-ADMITTED`, re-anchored with the component name + source path
/// the way every other component-body diagnostic is (`body_diags`, not a
/// second diagnostic style).
#[test]
fn toplevel_quest_in_component_is_reported_through_use() {
    let diags = component_diags(&quest_component(SMUGGLED_SET));
    let hits: Vec<_> = diags
        .iter()
        .filter(|d| d.code == "E-GRAMMAR-NOT-ADMITTED")
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "exactly one E-GRAMMAR-NOT-ADMITTED for the one top-level `<quest>`, got {:?}",
        codes(&diags)
    );
    let m = &hits[0].message;
    assert!(
        m.starts_with("component `c` (")
            && m.contains("): `<quest id=\"q1\">` is not admitted at the document top level"),
        "the diagnostic carries the component re-anchoring prefix, got {m:?}"
    );
    assert!(
        m.contains("component document"),
        "the message names the component document surface, got {m:?}"
    );
}

/// One diagnostic PER unwalked top-level declaration — the check reports every
/// occurrence, not just the first (mirroring `check_admission`'s own
/// per-`<quest>` loop for a scene document).
#[test]
fn every_toplevel_quest_is_reported() {
    let component = "---\ncomponent: c\n---\n<quest id=\"q1\">\n@x: a.\n</quest>\n\
                     <quest id=\"q2\">\n@x: b.\n</quest>\n";
    let diags = component_diags(component);
    let hits: Vec<_> = diags
        .iter()
        .filter(|d| d.code == "E-GRAMMAR-NOT-ADMITTED")
        .collect();
    assert_eq!(hits.len(), 2, "one per top-level `<quest>`, got {:?}", codes(&diags));
    assert!(
        hits.iter().any(|d| d.message.contains("q1"))
            && hits.iter().any(|d| d.message.contains("q2")),
        "each diagnostic names its own quest, got {:?}",
        hits.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

/// The smuggled state write no longer escapes: `::set{run.gold = 1}` inside a
/// component's top-level `<quest>` used to yield NOTHING through a `::use`
/// (exit 0, and the write silently dropped at compile), while the same `::set`
/// one line lower — inside a shot, i.e. in a position the component walker
/// actually processes — is `E-COMPONENT-BODY`. Either way the file must now
/// fail to check; the shot leg still gets the §6.1 diagnosis it always had.
#[test]
fn smuggled_state_write_no_longer_escapes_silently() {
    let in_quest = component_diags(&quest_component(SMUGGLED_SET));
    assert!(
        in_quest
            .iter()
            .any(|d| d.severity == lute_core_span::Severity::Error),
        "a component smuggling a state write in a top-level `<quest>` must not check clean, \
         got {:?}",
        codes(&in_quest)
    );

    let in_shot = component_diags(&format!(
        "---\ncomponent: c\n---\n## Scene 1.\n{SMUGGLED_SET}"
    ));
    assert!(
        codes(&in_shot).contains(&"E-COMPONENT-BODY".to_string()),
        "the SAME `::set` in a walked position keeps its §6.1 diagnosis, got {:?}",
        codes(&in_shot)
    );
}

/// Regression guard: an ordinary component — shots only, the shape every real
/// component file has — gains no diagnostic from the new check.
#[test]
fn ordinary_component_body_stays_clean() {
    let diags = component_diags("---\ncomponent: c\n---\n## Scene 1.\n@bianca: hello.\n");
    assert!(
        diags.is_empty(),
        "a presentational component body must stay clean, got {:?}",
        codes(&diags)
    );
}

/// The enumeration's one deliberate EXEMPTION, pinned on every path: a
/// document `# ` title (`Document::title`) is inert across the whole toolchain
/// — its only reader anywhere is `check_admission`'s quest-kind rejection, and
/// lowering reads the frontmatter `title:` key, never this field — so the ROOT
/// document drops it just as a component body does. Flagging it would INVENT
/// the mirror-image divergence (standalone clean, imported error) rather than
/// close one.
#[test]
fn component_title_stays_clean_on_every_path() {
    let component = "---\ncomponent: c\n---\n# A Title\n## Scene 1.\n@bianca: hello.\n";
    for (label, diags) in [
        ("standalone component", standalone_diags(component)),
        ("component via ::use", component_diags(component)),
    ] {
        assert!(
            diags.is_empty(),
            "a component `# ` title must stay clean at {label}, got {:?}",
            codes(&diags)
        );
    }
}
