//! Task U1 — `SchemaImports` + `CheckInput.imports` pure merge (dsl §9.2).
//! Imported state resolves like inline decls; a scene redeclaring an imported
//! tier flags `E-STATE-REDECLARE` (imported wins); import diags are surfaced.
use lute_check::meta::{Namespace, StateDecl, StateSchema};
use lute_check::schema_import::SchemaImports;
use lute_check::{check, CheckInput, Mode};
use lute_manifest::provider::ProviderSet;
use lute_manifest::types::{Literal, Type};

fn check_codes(text: &str, imports: SchemaImports) -> Vec<String> {
    let input = CheckInput {
        text: text.into(),
        uri: "t".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports,
    };
    check(&input)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

// Minimal valid scene reading an imported run path via <match>.
const SCENE_READS_RUN: &str = "---\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n\
<match on=\"run.choseHelp\">\n<when test=\"$ == true\">:line[x]: a\n</when>\n\
<otherwise>:line[x]: b\n</otherwise>\n</match>\n";
// Same but the scene ALSO inline-declares run.x which the import owns.
const SCENE_REDECLARES: &str = "---\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  run.x: { type: bool }\n---\n## Shot 1.\n:line[x]: hi\n";
const MINIMAL_SCENE: &str =
    "---\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n:line[x]: hi\n";

fn run_bool(default: bool) -> StateDecl {
    StateDecl {
        ty: Type::Bool,
        default: Some(Literal::Bool(default)),
        namespace: Namespace::Run,
    }
}

#[test]
fn imported_run_path_resolves_no_undeclared() {
    let mut st = StateSchema::default();
    st.decls.insert("run.choseHelp".into(), run_bool(false));
    let imports = SchemaImports {
        state: st,
        ..Default::default()
    };
    let codes = check_codes(SCENE_READS_RUN, imports);
    assert!(
        !codes.contains(&"E-UNDECLARED".to_string()),
        "imported path must resolve; got {codes:?}"
    );
}

#[test]
fn scene_override_of_imported_tier_flags_redeclare() {
    let mut st = StateSchema::default();
    st.decls.insert(
        "run.x".into(),
        StateDecl {
            ty: Type::Bool,
            default: None,
            namespace: Namespace::Run,
        },
    );
    let imports = SchemaImports {
        state: st,
        ..Default::default()
    };
    let codes = check_codes(SCENE_REDECLARES, imports);
    assert!(
        codes.contains(&"E-STATE-REDECLARE".to_string()),
        "got {codes:?}"
    );
}

#[test]
fn import_diags_are_surfaced() {
    let d = lute_core_span::Diagnostic {
        code: "E-USES-CYCLE".to_string(),
        severity: lute_core_span::Severity::Error,
        message: "synthetic".to_string(),
        span: lute_core_span::Span {
            byte_start: 0,
            byte_end: 0,
            line: 1,
            column: 1,
            utf16_range: (0, 0),
        },
        layer: lute_core_span::Layer::Content,
        fixits: Vec::new(),
        provenance: None,
    };
    let imports = SchemaImports {
        diags: vec![d],
        ..Default::default()
    };
    let codes = check_codes(MINIMAL_SCENE, imports);
    assert!(codes.contains(&"E-USES-CYCLE".to_string()));
}
