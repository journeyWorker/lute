//! dsl 0.21.0 §7b — a `defs:` entry's declaration shape, `E-DEF-DECL`.
//!
//! The defect: a def whose NAME resolved while its body was invisible to
//! `fold_env` (a bare-string value, silently skipped) passed `check` and then
//! failed `compile` with `E-COMPILE-EXPAND … (gate should have caught this)`.
//! The fix makes the bare string the legal SHORTHAND — `name: "<CEL>"`, its
//! type inferred from the body — and every other malformed shape a `check`
//! error, so a declared def always has a body.
use lute_check::{check, fold_env, resolve_imports, CheckInput, Mode, SchemaImports};
use lute_core_span::{Diagnostic, Span};
use lute_manifest::provider::ProviderSet;
use lute_manifest::types::Type;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const HDR: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n\
state:\n  scene.flag: { type: bool, default: false }\n  scene.n: { type: number, default: 0 }\n";

fn input(text: &str, imports: SchemaImports) -> CheckInput {
    CheckInput {
        text: text.to_string(),
        uri: "def_decl".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports,
        components: Default::default(),
        defaults: Default::default(),
    }
}

/// A scene declaring `defs:\n  <def>` and using `@<used>` as a bool guard.
fn scene(def: &str, used: &str) -> String {
    format!(
        "{HDR}defs:\n  {def}\n---\n## Shot 1.\n<match on=\"scene.flag\">\n\
         <when test=\"@{used}\">\n@narrator: a\n</when>\n\
         <otherwise>\n@narrator: b\n</otherwise>\n</match>\n"
    )
}

fn diags(text: &str) -> Vec<Diagnostic> {
    check(&input(text, SchemaImports::default())).diagnostics
}

fn codes(ds: &[Diagnostic]) -> Vec<&str> {
    ds.iter().map(|d| d.code.as_str()).collect()
}

/// The one `E-DEF-DECL` for `def`, or a panic naming everything produced.
fn def_decl(def: &str) -> Diagnostic {
    let ds = diags(&scene(def, "x"));
    ds.iter()
        .find(|d| d.code == "E-DEF-DECL")
        .cloned()
        .unwrap_or_else(|| panic!("E-DEF-DECL for `{def}`: {ds:#?}"))
}

static UNIQ: AtomicU64 = AtomicU64::new(0);

fn unique_dir() -> PathBuf {
    let n = UNIQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute_def_decl_{}_{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn zero_span() -> Span {
    Span {
        byte_start: 0,
        byte_end: 0,
        line: 1,
        column: 1,
        utf16_range: (0, 0),
    }
}

// --- the shorthand, and inference ---

#[test]
fn shorthand_bool_def_is_clean_and_has_a_body() {
    let text = scene("ready: \"scene.n >= 1\"", "ready");
    let ds = diags(&text);
    assert!(ds.is_empty(), "{ds:#?}");
    let (doc, _) = lute_syntax::parse(&text);
    let (folded, _, _) = fold_env(&doc, &input(&text, SchemaImports::default()));
    assert_eq!(
        folded.def_bodies.get("ready").map(String::as_str),
        Some("scene.n >= 1")
    );
    assert_eq!(folded.env.def_types.get("ready"), Some(&Type::Bool));
    assert_eq!(folded.env.def_params.get("ready"), Some(&Vec::new()));
}

/// The inferred type is load-bearing: a shorthand producing a number used in
/// a bool guard is `E-REF-TYPE`, exactly as the long form `type: number` is.
#[test]
fn shorthand_number_def_is_typed_by_inference() {
    let ds = diags(&scene("n1: \"scene.n + 1\"", "n1"));
    assert!(codes(&ds).contains(&"E-REF-TYPE"), "{ds:#?}");
    assert!(!codes(&ds).contains(&"E-DEF-DECL"), "{ds:#?}");
}

#[test]
fn long_form_type_is_optional_and_inferred() {
    let ds = diags(&scene("n1: { cel: \"scene.n\" }", "n1"));
    assert!(codes(&ds).contains(&"E-REF-TYPE"), "{ds:#?}");
    assert!(!codes(&ds).contains(&"E-DEF-DECL"), "{ds:#?}");
}

#[test]
fn an_undecidable_shorthand_asks_for_the_long_form() {
    let d = def_decl("x: \"scene.flag ? 1 : 'one'\"");
    assert!(d.message.contains("cannot be inferred"), "{}", d.message);
    assert!(
        d.message.contains("x: { type: bool, cel: \"scene.flag ? 1 : 'one'\" }"),
        "the fix, spelled out: {}",
        d.message
    );
    // Anchored at the def's own key (line 10: `  x: …`), not the meta block.
    assert_eq!(d.span.line, 10, "{:?}", d.span);
}

#[test]
fn an_explicit_type_must_agree_with_the_body() {
    let d = def_decl("x: { type: bool, cel: \"scene.n + 1\" }");
    assert!(
        d.message.contains("declares `type:` a bool")
            && d.message.contains("produces a number"),
        "{}",
        d.message
    );
    // The string family stays compatible, as it is for `E-REF-TYPE`.
    let ds = diags(&scene("x: { type: string, cel: \"'a'\" }", "x"));
    assert!(!codes(&ds).contains(&"E-DEF-DECL"), "{ds:#?}");
}

// --- malformed shapes ---

#[test]
fn a_non_string_non_mapping_def_is_rejected() {
    let d = def_decl("x: 5");
    assert!(d.message.contains("but this is a number"), "{}", d.message);
    assert!(
        d.message.contains("`x: \"<CEL>\"`") && d.message.contains("{ type: bool, cel: \"…\" }"),
        "both legal shapes named: {}",
        d.message
    );
}

#[test]
fn a_mapping_without_a_string_cel_is_rejected() {
    let d = def_decl("x: { type: bool }");
    assert!(d.message.contains("no `cel:` key"), "{}", d.message);
    let d = def_decl("x: { type: bool, cel: 5 }");
    assert!(d.message.contains("must be a string of CEL"), "{}", d.message);
}

#[test]
fn a_bad_type_is_rejected() {
    let d = def_decl("x: { type: nonsense, cel: \"true\" }");
    assert!(d.message.contains("`type:` is not a type"), "{}", d.message);
}

#[test]
fn an_unknown_key_is_rejected_by_name() {
    let d = def_decl("x: { type: bool, body: \"true\", cel: \"true\" }");
    assert!(d.message.contains("`body:` is not a def key"), "{}", d.message);
}

#[test]
fn params_require_an_explicit_type() {
    let d = def_decl("x: { params: { n: number }, cel: \"scene.n >= n\" }");
    assert!(d.message.contains("must declare its `type:`"), "{}", d.message);
    // With the type, the same def is legal.
    let ds = diags(&scene(
        "x: { type: bool, params: { n: number }, cel: \"scene.n >= n\" }",
        "x(1)",
    ));
    assert!(ds.is_empty(), "{ds:#?}");
}

// --- across `uses:` ---

#[test]
fn an_imported_shorthand_def_has_a_body_and_an_inferred_type() {
    let dir = unique_dir();
    std::fs::write(
        dir.join("world.schema.yaml"),
        "state:\n  run.x: { type: number, default: 0 }\n\
         defs:\n  ready: \"run.x >= 2\"\n  level: \"run.x\"\n",
    )
    .unwrap();
    let imports = resolve_imports(&dir, &["world.schema.yaml".to_string()], &[], zero_span());
    assert!(imports.diags.is_empty(), "{:#?}", imports.diags);
    let text = scene("local: \"scene.flag\"", "ready");
    let res = check(&input(&text, imports.clone()));
    assert!(res.diagnostics.is_empty(), "{:#?}", res.diagnostics);
    let (doc, _) = lute_syntax::parse(&text);
    let (folded, _, _) = fold_env(&doc, &input(&text, imports.clone()));
    assert_eq!(
        folded.def_bodies.get("ready").map(String::as_str),
        Some("run.x >= 2")
    );
    assert_eq!(folded.env.def_types.get("ready"), Some(&Type::Bool));
    assert_eq!(folded.env.def_types.get("level"), Some(&Type::Number));
    // And the inferred number type is enforced in the importer.
    let res = check(&input(&scene("local: \"scene.flag\"", "level"), imports));
    assert!(
        res.diagnostics.iter().any(|d| d.code == "E-REF-TYPE"),
        "{:#?}",
        res.diagnostics
    );
}

/// The regression: a malformed imported def fails `check` — not only
/// `compile` — carrying the schema's own `E-DEF-DECL` beneath `E-USES-PARSE`.
#[test]
fn a_malformed_imported_def_fails_check() {
    let dir = unique_dir();
    std::fs::write(dir.join("world.schema.yaml"), "defs:\n  ready: [holds]\n").unwrap();
    let imports = resolve_imports(&dir, &["world.schema.yaml".to_string()], &[], zero_span());
    let res = check(&input(&scene("local: \"scene.flag\"", "ready"), imports));
    assert!(!res.ok, "{:#?}", res.diagnostics);
    let parent = res
        .diagnostics
        .iter()
        .find(|d| d.code == "E-USES-PARSE")
        .unwrap_or_else(|| panic!("{:#?}", res.diagnostics));
    let child = parent
        .related
        .iter()
        .find(|r| r.diagnostic.code == "E-DEF-DECL")
        .unwrap_or_else(|| panic!("{parent:#?}"));
    assert!(
        child.diagnostic.message.contains("but this is a list"),
        "{}",
        child.diagnostic.message
    );
    // `  ready` starts at byte 8: "defs:\n" is 6, then two spaces.
    assert_eq!(child.diagnostic.span.byte_start, 8);
}

#[test]
fn an_undecidable_imported_shorthand_is_named_in_the_importer() {
    let dir = unique_dir();
    std::fs::write(
        dir.join("world.schema.yaml"),
        "state:\n  run.x: { type: number, default: 0 }\n\
         defs:\n  odd: \"run.x > 0 ? 1 : 'none'\"\n",
    )
    .unwrap();
    let imports = resolve_imports(&dir, &["world.schema.yaml".to_string()], &[], zero_span());
    assert!(imports.diags.is_empty(), "{:#?}", imports.diags);
    let ds = check(&input(&scene("local: \"scene.flag\"", "local"), imports)).diagnostics;
    let d = ds
        .iter()
        .find(|d| d.code == "E-DEF-DECL")
        .unwrap_or_else(|| panic!("{ds:#?}"));
    assert!(
        d.message.contains("world.schema.yaml") && d.message.contains("def `odd`"),
        "{}",
        d.message
    );
}
