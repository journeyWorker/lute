//! Task U1 — `SchemaImports` + `CheckInput.imports` pure merge (dsl §9.2).
//! Imported state resolves like inline decls; a scene redeclaring an imported
//! tier flags `E-STATE-REDECLARE` (imported wins); import diags are surfaced.
use lute_check::meta::{Namespace, StateDecl, StateSchema};
use lute_check::resolve_imports;
use lute_check::schema_import::SchemaImports;
use lute_check::{check, CheckInput, Mode};
use lute_core_span::Span;
use lute_manifest::provider::ProviderSet;
use lute_manifest::types::{Literal, Type};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

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

// --- Task U2 — `resolve_imports` DAG resolver (dsl §9.2) ---

fn zero_span() -> Span {
    Span {
        byte_start: 0,
        byte_end: 0,
        line: 1,
        column: 1,
        utf16_range: (0, 0),
    }
}

static UNIQ: AtomicU64 = AtomicU64::new(0);

/// A fresh temp dir per call; schema `.lute` files are written into it.
fn unique_dir() -> PathBuf {
    let n = UNIQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir =
        std::env::temp_dir().join(format!("lute_uses_{}_{}_{}", std::process::id(), n, nanos));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_lute(dir: &Path, name: &str, body: &str) {
    std::fs::write(dir.join(name), body).unwrap();
}

fn resolve_codes(res: &SchemaImports) -> Vec<&str> {
    res.diags.iter().map(|d| d.code.as_str()).collect()
}

#[test]
fn resolves_single_import() {
    let dir = unique_dir();
    write_lute(
        &dir,
        "schema.lute",
        "---\nstate:\n  run.x: { type: bool, default: false }\n---\n",
    );
    let res = resolve_imports(&dir, &["schema.lute".to_string()], zero_span());
    assert!(res.diags.is_empty(), "unexpected diags: {:?}", res.diags);
    assert!(
        res.state.decls.contains_key("run.x"),
        "run.x missing: {:?}",
        res.state.decls.keys().collect::<Vec<_>>()
    );
}

#[test]
fn cycle_is_e_uses_cycle() {
    let dir = unique_dir();
    write_lute(&dir, "a.lute", "---\nuses: b.lute\n---\n");
    write_lute(&dir, "b.lute", "---\nuses: a.lute\n---\n");
    let res = resolve_imports(&dir, &["a.lute".to_string()], zero_span());
    let codes = resolve_codes(&res);
    assert!(
        codes.contains(&"E-USES-CYCLE"),
        "expected E-USES-CYCLE, got {codes:?}"
    );
}

#[test]
fn dup_def_across_imports_errors() {
    let dir = unique_dir();
    write_lute(&dir, "x.lute", "---\ndefs:\n  foo: 1\n---\n");
    write_lute(&dir, "y.lute", "---\ndefs:\n  foo: 2\n---\n");
    write_lute(&dir, "a.lute", "---\nuses: [x.lute, y.lute]\n---\n");
    let res = resolve_imports(&dir, &["a.lute".to_string()], zero_span());
    let codes = resolve_codes(&res);
    assert!(
        codes.contains(&"E-USES-DUP-DEF"),
        "expected E-USES-DUP-DEF, got {codes:?}"
    );
}

#[test]
fn missing_file_is_not_found() {
    let dir = unique_dir();
    let res = resolve_imports(&dir, &["nope.lute".to_string()], zero_span());
    let codes = resolve_codes(&res);
    assert!(
        codes.contains(&"E-USES-NOT-FOUND"),
        "expected E-USES-NOT-FOUND, got {codes:?}"
    );
}

#[test]
fn diamond_is_one_identity_no_dup() {
    let dir = unique_dir();
    write_lute(&dir, "d.lute", "---\ndefs:\n  x: 1\n---\n");
    write_lute(&dir, "b.lute", "---\nuses: d.lute\n---\n");
    write_lute(&dir, "c.lute", "---\nuses: d.lute\n---\n");
    write_lute(&dir, "a.lute", "---\nuses: [b.lute, c.lute]\n---\n");
    let res = resolve_imports(&dir, &["a.lute".to_string()], zero_span());
    let codes = resolve_codes(&res);
    assert!(
        !codes.contains(&"E-USES-DUP-DEF"),
        "unexpected E-USES-DUP-DEF for diamond: {codes:?}"
    );
    assert!(
        res.defs.contains_key("x"),
        "def x missing: {:?}",
        res.defs.keys().collect::<Vec<_>>()
    );
    assert_eq!(
        res.defs.len(),
        1,
        "expected exactly one def: {:?}",
        res.defs
    );
}

#[test]
fn malformed_schema_is_e_uses_parse() {
    let dir = unique_dir();
    // Valid frontmatter, but the BODY has an unterminated `/* … */` block
    // comment -> `lute_syntax::parse` emits E-COMMENT-UNTERMINATED in its parse
    // diagnostics (pdiags). Before the fix those were dropped, so the malformed
    // import was silently treated as empty (no E-USES-PARSE).
    write_lute(
        &dir,
        "bad.lute",
        "---\nstate:\n  run.x: { type: bool, default: false }\n---\n/* unterminated",
    );
    let out = resolve_imports(&dir, &["bad.lute".to_string()], zero_span());
    let codes = resolve_codes(&out);
    assert!(
        codes.contains(&"E-USES-PARSE"),
        "malformed schema must flag E-USES-PARSE; got {codes:?}"
    );
}
