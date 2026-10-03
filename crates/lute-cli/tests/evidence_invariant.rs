use std::path::PathBuf;

use lute_check::evidence::{classification, DiagnosticClass};
use lute_core_span::{Diagnostic, Evidence};
use lute_model::{ModelOptions, ProjectModel};

fn assert_diagnostic(diagnostic: &Diagnostic) {
    match classification(&diagnostic.code) {
        Some(DiagnosticClass::Analysis { evidence }) => {
            let actual = diagnostic
                .evidence
                .as_ref()
                .unwrap_or_else(|| panic!("analysis diagnostic {} lacks evidence", diagnostic.code));
            assert_eq!(actual, &evidence, "wrong evidence for {}", diagnostic.code);
            if let Evidence::Bounded { scope } = actual {
                assert!(!scope.is_empty(), "bounded diagnostic {} lacks scope", diagnostic.code);
            }
        }
        Some(DiagnosticClass::SourceError) => assert!(
            diagnostic.evidence.is_none(),
            "source diagnostic {} unexpectedly carries evidence",
            diagnostic.code
        ),
        None => panic!("unclassified diagnostic {}", diagnostic.code),
    }
    for related in &diagnostic.related {
        assert_diagnostic(&related.diagnostic);
    }
}

fn assert_model(model: &ProjectModel) {
    for document in model.documents() {
        for diagnostic in &document.check.diagnostics {
            assert_diagnostic(diagnostic);
        }
    }
    for (_, diagnostic) in model.project_diagnostics() {
        assert_diagnostic(diagnostic);
    }
    for (_, diagnostic) in &model.reconciled().diagnostics {
        assert_diagnostic(diagnostic);
    }
}

#[test]
fn docs_examples_and_reconciliation_diagnostics_have_evidence_contract() {
    let examples = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/examples");
    let models = ProjectModel::roots_under(&examples, &ModelOptions::default()).unwrap();
    assert!(!models.is_empty(), "docs/examples yielded no project roots");
    for model in &models {
        assert_model(model);
    }

    let root = std::env::temp_dir().join(format!("lute-evidence-invariant-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("lute.project.yaml"), "{}\n").unwrap();
    std::fs::write(
        root.join("quests.lute"),
        "---\nkind: quest\n---\n\
<quest id=\"parent\" start=\"true\">\n\
<objective id=\"child\" quest=\"never\"/>\n\
</quest>\n\
<quest id=\"never\" start=\"1 > 2\">\n\
<objective id=\"done\" done=\"true\"/>\n\
</quest>\n",
    )
    .unwrap();
    let model = ProjectModel::build(&root, &ModelOptions::default()).unwrap();
    assert_model(&model);
    let fixture_has_unsat = model
        .documents()
        .iter()
        .any(|document| {
            document
                .check
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "E-OBJECTIVE-UNSATISFIABLE")
        })
        || model
            .project_diagnostics()
            .iter()
            .any(|(_, diagnostic)| diagnostic.code == "E-OBJECTIVE-UNSATISFIABLE")
        || model
            .reconciled()
            .diagnostics
            .iter()
            .any(|(_, diagnostic)| diagnostic.code == "E-OBJECTIVE-UNSATISFIABLE");
    assert!(fixture_has_unsat, "fixture did not exercise unreachable child objective");
    std::fs::remove_dir_all(root).unwrap();
}
