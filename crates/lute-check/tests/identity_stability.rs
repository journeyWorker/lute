use lute_check::{check, resolve_components, CheckInput, Mode, SchemaImports};
use lute_manifest::provider::ProviderSet;
use lute_test_vocab::vocab_snapshot;

const COMPONENT: &str = "---\ncomponent: c\n---\n## Scene 1.\n";
const SCENE: &str = "---\nkind: scene\ncharacter: n\nseason: 1\nepisode: 1\ncomponents: [c.lute]\n---\n## Shot 1.\n::use{component=\"c\"}\n@n: uncoded\n";

fn diagnostics(require_stable: bool) -> Vec<lute_core_span::Diagnostic> {
    let dir = lute_scratch::ScratchDir::new();
    std::fs::write(dir.join("c.lute"), COMPONENT).unwrap();
    let (doc, _) = lute_syntax::parse(SCENE);
    let typed = lute_check::parse_meta(
        &doc.meta,
        &lute_manifest::snapshot::CapabilitySnapshot::default(),
    )
    .0;
    let components = resolve_components(&dir, &typed.components, doc.meta.span);
    let mut snapshot = vocab_snapshot();
    snapshot.identity_require_stable = require_stable;
    let input = CheckInput::new(
        SCENE.into(),
        dir.join("scene.lute").display().to_string(),
        Mode::Ci,
        lute_check::InputSources::from_parts(
            snapshot,
            ProviderSet::default(),
            SchemaImports::default(),
            components,
            Default::default(),
        ),
    );
    check(&input).diagnostics
}

#[test]
fn project_requiring_stable_identity_reports_both_warnings_at_source_spans() {
    let diagnostics = diagnostics(true);
    let use_start = SCENE.find("::use").unwrap();
    let line_start = SCENE.find("@n: uncoded").unwrap();
    assert_eq!(
        diagnostics
            .iter()
            .filter(|d| {
                matches!(
                    d.code.as_str(),
                    "W-COMPONENT-INSTANCE-UNTAGGED" | "W-LINE-CODE-UNTAGGED"
                )
            })
            .count(),
        2
    );
    let component = diagnostics
        .iter()
        .find(|d| d.code == "W-COMPONENT-INSTANCE-UNTAGGED")
        .expect("stable project reports untagged component use");
    assert_eq!(component.span.byte_start, use_start);
    assert!(component.message.contains("lute tag"));
    let line = diagnostics
        .iter()
        .find(|d| d.code == "W-LINE-CODE-UNTAGGED")
        .expect("stable project reports untagged line");
    assert_eq!(line.span.byte_start, line_start);
    assert!(line.message.contains("lute tag"));
}

#[test]
fn project_without_stable_identity_reports_neither_warning() {
    let diagnostics = diagnostics(false);
    assert!(!diagnostics.iter().any(|d| {
        matches!(
            d.code.as_str(),
            "W-COMPONENT-INSTANCE-UNTAGGED" | "W-LINE-CODE-UNTAGGED"
        )
    }));
}

#[test]
fn standalone_check_without_project_reports_neither_warning() {
    let text =
        "---\nkind: scene\ncharacter: n\nseason: 1\nepisode: 1\n---\n## Shot 1.\n@n: uncoded\n";
    let input = CheckInput::new(
        text.into(),
        "standalone.lute".into(),
        Mode::Ci,
        lute_check::InputSources::from_parts(
            vocab_snapshot(),
            ProviderSet::default(),
            SchemaImports::default(),
            Default::default(),
            Default::default(),
        ),
    );
    let diagnostics = check(&input).diagnostics;
    assert!(!diagnostics.iter().any(|d| {
        matches!(
            d.code.as_str(),
            "W-COMPONENT-INSTANCE-UNTAGGED" | "W-LINE-CODE-UNTAGGED"
        )
    }));
}
