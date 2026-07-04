//! Task C3 — `::use` component-invocation validation + presentational body
//! validation + expansion-cycle detection (dsl §13). Scenes + component files are
//! written to a temp dir, resolved via `resolve_components` (the SAME resolver the
//! CLI/LSP call), and validated through the assembled `check()`.
use lute_check::{check, parse_meta, resolve_components, CheckInput, Mode};
use lute_manifest::core::load_core_snapshot;
use lute_manifest::provider::ProviderSet;
use lute_manifest::snapshot::CapabilitySnapshot;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static UNIQ: AtomicU64 = AtomicU64::new(0);

fn unique_dir() -> PathBuf {
    let n = UNIQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("lute_use_{}_{}_{}", std::process::id(), n, nanos));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_lute(dir: &Path, name: &str, body: &str) {
    std::fs::write(dir.join(name), body).unwrap();
}

/// Resolve `components:` from `dir` and run the assembled `check()` over the scene
/// text; return every diagnostic code (mirrors the CLI/LSP wiring).
fn codes(dir: &Path, scene: &str) -> Vec<String> {
    let (doc, _) = lute_syntax::parse(scene);
    let (meta0, _) = parse_meta(&doc.meta, &CapabilitySnapshot::default());
    let components = resolve_components(dir, &meta0.components, doc.meta.span);
    let input = CheckInput {
        text: scene.to_string(),
        uri: "scene".into(),
        snapshot: load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Ci,
        imports: Default::default(),
        components,
    };
    check(&input)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

const GREET: &str = "---\ncomponent: greet\nparams:\n  who: string\n---\n\
## Greeting.\n::auto{character=@who}\n:line[narrator]: Hello there.\n";

fn scene(components: &str, body: &str) -> String {
    format!("---\ncharacter: x\nseason: 1\nepisode: 1\ncomponents: [{components}]\n---\n## Shot 1.\n{body}\n")
}

#[test]
fn valid_use_is_clean() {
    let dir = unique_dir();
    write_lute(&dir, "greet.lute", GREET);
    let s = scene("greet.lute", "::use{component=\"greet\" who=\"bianca\"}");
    let cs = codes(&dir, &s);
    assert!(
        !cs.iter().any(|c| c.starts_with("E-")),
        "a valid ::use of a valid presentational component must be error-clean; got {cs:?}"
    );
}

#[test]
fn unknown_component_is_undeclared() {
    let dir = unique_dir();
    write_lute(&dir, "greet.lute", GREET);
    let s = scene("greet.lute", "::use{component=\"nope\" who=\"x\"}");
    let cs = codes(&dir, &s);
    assert!(
        cs.contains(&"E-COMPONENT-UNDECLARED".to_string()),
        "an unknown component must flag E-COMPONENT-UNDECLARED; got {cs:?}"
    );
    assert!(
        !cs.contains(&"E-UNKNOWN-DIRECTIVE".to_string()),
        "`use` is a reserved directive and must never be E-UNKNOWN-DIRECTIVE; got {cs:?}"
    );
}

#[test]
fn missing_required_arg_is_component_arg() {
    let dir = unique_dir();
    write_lute(&dir, "greet.lute", GREET);
    let s = scene("greet.lute", "::use{component=\"greet\"}");
    let cs = codes(&dir, &s);
    assert!(
        cs.contains(&"E-COMPONENT-ARG".to_string()),
        "a missing required arg must flag E-COMPONENT-ARG; got {cs:?}"
    );
}

#[test]
fn unknown_arg_is_component_arg() {
    let dir = unique_dir();
    write_lute(&dir, "greet.lute", GREET);
    let s = scene(
        "greet.lute",
        "::use{component=\"greet\" who=\"b\" extra=\"y\"}",
    );
    let cs = codes(&dir, &s);
    assert!(
        cs.contains(&"E-COMPONENT-ARG".to_string()),
        "an unknown arg must flag E-COMPONENT-ARG; got {cs:?}"
    );
}

#[test]
fn mistyped_arg_is_component_arg() {
    let dir = unique_dir();
    // A number-typed param, supplied a non-numeric string.
    write_lute(
        &dir,
        "badge.lute",
        "---\ncomponent: badge\nparams:\n  count: number\n---\n## B.\n:line[narrator]: badge.\n",
    );
    let s = scene(
        "badge.lute",
        "::use{component=\"badge\" count=\"notanumber\"}",
    );
    let cs = codes(&dir, &s);
    assert!(
        cs.contains(&"E-COMPONENT-ARG".to_string()),
        "a mistyped arg must flag E-COMPONENT-ARG; got {cs:?}"
    );
}

#[test]
fn numeric_arg_of_number_param_is_clean() {
    let dir = unique_dir();
    write_lute(
        &dir,
        "badge.lute",
        "---\ncomponent: badge\nparams:\n  count: number\n---\n## B.\n:line[narrator]: badge.\n",
    );
    let s = scene("badge.lute", "::use{component=\"badge\" count=\"3\"}");
    let cs = codes(&dir, &s);
    assert!(
        !cs.contains(&"E-COMPONENT-ARG".to_string()),
        "a numeric string for a number param must NOT flag E-COMPONENT-ARG; got {cs:?}"
    );
}

#[test]
fn recursive_components_is_cycle() {
    let dir = unique_dir();
    // No `components:` import cycle: the SCENE imports both a and b directly, but
    // a's body `::use`s b and b's body `::use`s a — a pure ::use EXPANSION cycle.
    write_lute(
        &dir,
        "a.lute",
        "---\ncomponent: a\n---\n## A.\n::use{component=\"b\"}\n",
    );
    write_lute(
        &dir,
        "b.lute",
        "---\ncomponent: b\n---\n## B.\n::use{component=\"a\"}\n",
    );
    let s = scene("a.lute, b.lute", "::use{component=\"a\"}");
    let cs = codes(&dir, &s);
    assert!(
        cs.contains(&"E-COMPONENT-CYCLE".to_string()),
        "a ::use expansion cycle across components must flag E-COMPONENT-CYCLE; got {cs:?}"
    );
}

#[test]
fn state_read_in_body_is_v1_error() {
    let dir = unique_dir();
    // A `<match>` reading scene state in a body is a v1 presentational-scope error.
    write_lute(
        &dir,
        "logic.lute",
        "---\ncomponent: logic\n---\n## L.\n<match on=\"scene.x\">\n\
<when test=\"$ == true\">:line[narrator]: a\n</when>\n\
<otherwise>:line[narrator]: b\n</otherwise>\n</match>\n",
    );
    let s = scene("logic.lute", "::use{component=\"logic\"}");
    let cs = codes(&dir, &s);
    assert!(
        cs.contains(&"E-COMPONENT-BODY".to_string()),
        "a `<match>` logic block / state read in a body must flag E-COMPONENT-BODY; got {cs:?}"
    );
}

#[test]
fn state_write_in_body_is_v1_error() {
    let dir = unique_dir();
    write_lute(
        &dir,
        "writer.lute",
        "---\ncomponent: writer\n---\n## W.\n::set{scene.x = 1}\n",
    );
    let s = scene("writer.lute", "::use{component=\"writer\"}");
    let cs = codes(&dir, &s);
    assert!(
        cs.contains(&"E-COMPONENT-BODY".to_string()),
        "a `::set` state write in a body must flag E-COMPONENT-BODY; got {cs:?}"
    );
}

#[test]
fn undeclared_ref_in_body_is_flagged() {
    let dir = unique_dir();
    // `@stranger` is not a declared param — only params are the body ref namespace.
    write_lute(
        &dir,
        "greet.lute",
        "---\ncomponent: greet\nparams:\n  who: string\n---\n\
## G.\n::auto{character=@stranger}\n",
    );
    let s = scene("greet.lute", "::use{component=\"greet\" who=\"b\"}");
    let cs = codes(&dir, &s);
    assert!(
        cs.contains(&"E-UNDECLARED-REF".to_string()),
        "a body `@ref` to a non-param must be flagged; got {cs:?}"
    );
}
