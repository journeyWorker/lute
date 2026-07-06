//! Regression (behavior-preservation): `check()`'s diagnostic byte-order
//! contract. Two `E-EXTENDS-STATE-TYPE` diagnostics — one from import/extends
//! resolution (`input.imports.diags`) and one from the scene's inline
//! state-merge — land at the SAME frontmatter span with the SAME code, so the
//! final stable sort on `(byte_start, code)` cannot reorder them: their relative
//! order is exactly their INSERTION order.
//!
//! The contract (commit 5583936): import diags are collected BEFORE the
//! state-merge diags (which sit after `validate_components(...)`), so the import
//! diag must precede the state-merge one. The `fold_env` extraction once bundled
//! the state-merge stream into the pre-import fold group, flipping this tie and
//! changing `check()`'s byte output. This test pins the correct order.

use lute_check::meta::{Namespace, StateDecl, StateSchema};
use lute_check::schema_import::SchemaImports;
use lute_check::{check, CheckInput, Mode};
use lute_core_span::{Diagnostic, Layer, Severity};
use lute_manifest::provider::ProviderSet;
use lute_manifest::types::Type;
use std::collections::BTreeSet;

// A scene that inline-refines an extends-base ("overridable") imported state
// path with a DIFFERENT type -> the inline state-merge emits
// `E-EXTENDS-STATE-TYPE` at the frontmatter span.
const SCENE: &str =
    "---\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  run.gold: { type: string }\n---\n## Shot 1.\n:x: hi\n";

#[test]
fn import_extends_type_diag_precedes_inline_state_merge_at_same_span() {
    // The frontmatter span both diagnostics collapse onto (check() re-parses the
    // same text internally, so this matches its `doc.meta.span`).
    let (doc, _) = lute_syntax::parse(SCENE);
    let meta_span = doc.meta.span;

    // Imported schema: `run.gold` is an extends-base path (overridable), declared
    // as `number`. The scene's inline `state: run.gold: string` refines it with a
    // different type -> a state-merge `E-EXTENDS-STATE-TYPE` at `meta_span`.
    let mut st = StateSchema::default();
    st.decls.insert(
        "run.gold".into(),
        StateDecl {
            ty: Type::Number,
            default: None,
            namespace: Namespace::Run,
        },
    );
    let mut overridable = BTreeSet::new();
    overridable.insert("run.gold".to_string());

    // A synthetic IMPORT-resolution `E-EXTENDS-STATE-TYPE` at the very same span,
    // distinguishable from the state-merge one by its message (a different path).
    let import_diag = Diagnostic {
        code: "E-EXTENDS-STATE-TYPE".to_string(),
        severity: Severity::Error,
        message: "IMPORT-SOURCED run.silver".to_string(),
        span: meta_span,
        layer: Layer::Content,
        fixits: Vec::new(),
        provenance: None,
    };

    let imports = SchemaImports {
        state: st,
        state_overridable: overridable,
        diags: vec![import_diag],
        ..Default::default()
    };
    let input = CheckInput {
        text: SCENE.into(),
        uri: "t".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports,
        components: Default::default(),
    };
    let diags = check(&input).diagnostics;

    let import_idx = diags
        .iter()
        .position(|d| d.code == "E-EXTENDS-STATE-TYPE" && d.message.contains("IMPORT-SOURCED"))
        .expect("import-sourced E-EXTENDS-STATE-TYPE present");
    let merge_idx = diags
        .iter()
        .position(|d| d.code == "E-EXTENDS-STATE-TYPE" && d.message.contains("run.gold"))
        .expect("inline state-merge E-EXTENDS-STATE-TYPE present");

    // Tie precondition: same byte_start + same code -> the stable sort preserves
    // insertion order. If this fails the test is vacuous rather than wrong.
    assert_eq!(
        diags[import_idx].span.byte_start, diags[merge_idx].span.byte_start,
        "both E-EXTENDS-STATE-TYPE diags must share a byte_start for the tie to be observable"
    );
    assert!(
        import_idx < merge_idx,
        "import-resolution E-EXTENDS-STATE-TYPE must precede the inline state-merge one \
         (pre-refactor order, commit 5583936); got import_idx={import_idx}, merge_idx={merge_idx}\n{diags:#?}"
    );
}
