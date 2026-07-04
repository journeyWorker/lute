//! Task C2 — `resolve_components` DAG resolver (dsl §13). A `components:` import
//! DAG canonicalizes + dedups diamonds, parses each file in `MetaKind::Component`,
//! and builds a `name -> ComponentDef` table; missing/malformed files and a
//! missing `component:` name are `E-COMPONENT-PARSE`, a cross-file duplicate name
//! is `E-COMPONENT-DUP`, and a `components:` import cycle is `E-COMPONENT-CYCLE`.
//! Mirrors `uses_import.rs`'s temp-dir resolver tests.
use lute_check::resolve_components;
use lute_check::ComponentSet;
use lute_core_span::Span;
use lute_manifest::types::Type;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

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

/// A fresh temp dir per call; component `.lute` files are written into it.
fn unique_dir() -> PathBuf {
    let n = UNIQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!(
        "lute_components_{}_{}_{}",
        std::process::id(),
        n,
        nanos
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_lute(dir: &Path, name: &str, body: &str) {
    std::fs::write(dir.join(name), body).unwrap();
}

fn codes(res: &ComponentSet) -> Vec<&str> {
    res.diags.iter().map(|d| d.code.as_str()).collect()
}

const GREET: &str =
    "---\ncomponent: greet\nparams:\n  who: string\n---\n## Greeting.\n:line[x]: hi\n";

#[test]
fn resolves_single_component_with_params() {
    let dir = unique_dir();
    write_lute(&dir, "greet.lute", GREET);
    let res = resolve_components(&dir, &["greet.lute".to_string()], zero_span());
    assert!(res.diags.is_empty(), "unexpected diags: {:?}", res.diags);
    let def = res
        .table
        .get("greet")
        .expect("greet component must be in the table");
    assert_eq!(def.params.len(), 1);
    assert_eq!(def.params[0].0, "who");
    assert_eq!(def.params[0].1, Type::Str);
}

#[test]
fn missing_component_file_is_not_found() {
    let dir = unique_dir();
    let res = resolve_components(&dir, &["nope.lute".to_string()], zero_span());
    assert!(
        res.table.is_empty(),
        "a missing file must not enter the table: {:?}",
        res.table.keys().collect::<Vec<_>>()
    );
    assert!(
        codes(&res).contains(&"E-COMPONENT-PARSE"),
        "a missing component import must be reported; got {:?}",
        codes(&res)
    );
}

#[test]
fn duplicate_component_name_across_files_errors() {
    let dir = unique_dir();
    write_lute(&dir, "a.lute", GREET);
    // A second file declaring the SAME `component: greet` name.
    write_lute(
        &dir,
        "b.lute",
        "---\ncomponent: greet\nparams:\n  who: number\n---\n## G.\n:line[x]: yo\n",
    );
    let res = resolve_components(
        &dir,
        &["a.lute".to_string(), "b.lute".to_string()],
        zero_span(),
    );
    assert!(
        codes(&res).contains(&"E-COMPONENT-DUP"),
        "two files declaring `greet` must flag E-COMPONENT-DUP; got {:?}",
        codes(&res)
    );
}

#[test]
fn malformed_component_is_parse_error() {
    let dir = unique_dir();
    // Valid frontmatter, but the BODY has an unterminated `/* … */` block comment
    // -> `lute_syntax::parse` surfaces E-COMMENT-UNTERMINATED in its parse diags.
    write_lute(
        &dir,
        "bad.lute",
        "---\ncomponent: bad\n---\n## C.\n/* unterminated",
    );
    let res = resolve_components(&dir, &["bad.lute".to_string()], zero_span());
    assert!(
        codes(&res).contains(&"E-COMPONENT-PARSE"),
        "a malformed component must flag E-COMPONENT-PARSE; got {:?}",
        codes(&res)
    );
}

#[test]
fn component_malformed_params_is_parse() {
    // A `params:` entry whose value is not a valid `Type` must NOT be silently
    // dropped (which would let the component be invoked without the param); it
    // must surface as E-COMPONENT-PARSE for that file (dsl §13).
    let dir = unique_dir();
    write_lute(
        &dir,
        "bad.lute",
        "---\ncomponent: bad\nparams:\n  who: notAType\n---\n## C.\n:line[x]: hi\n",
    );
    let res = resolve_components(&dir, &["bad.lute".to_string()], zero_span());
    assert!(
        codes(&res).contains(&"E-COMPONENT-PARSE"),
        "a `params:` value that is not a valid Type must flag E-COMPONENT-PARSE, not silently drop the param; got {:?}",
        codes(&res)
    );

    // A non-mapping `params:` is likewise malformed.
    let dir2 = unique_dir();
    write_lute(
        &dir2,
        "bad2.lute",
        "---\ncomponent: bad2\nparams: 5\n---\n## C.\n:line[x]: hi\n",
    );
    let res2 = resolve_components(&dir2, &["bad2.lute".to_string()], zero_span());
    assert!(
        codes(&res2).contains(&"E-COMPONENT-PARSE"),
        "a non-mapping `params:` must flag E-COMPONENT-PARSE; got {:?}",
        codes(&res2)
    );
}

#[test]
fn component_missing_name_is_parse_error() {
    let dir = unique_dir();
    // No `component:` declaration — cannot enter the name table.
    write_lute(
        &dir,
        "anon.lute",
        "---\nparams:\n  who: string\n---\n## C.\n:line[x]: hi\n",
    );
    let res = resolve_components(&dir, &["anon.lute".to_string()], zero_span());
    assert!(
        codes(&res).contains(&"E-COMPONENT-PARSE"),
        "a component file with no `component:` name must flag E-COMPONENT-PARSE; got {:?}",
        codes(&res)
    );
    assert!(
        res.table.is_empty(),
        "an unnamed component must not enter the table"
    );
}

#[test]
fn import_cycle_is_component_cycle() {
    let dir = unique_dir();
    write_lute(
        &dir,
        "a.lute",
        "---\ncomponent: a\ncomponents: [b.lute]\n---\n## A.\n:line[x]: a\n",
    );
    write_lute(
        &dir,
        "b.lute",
        "---\ncomponent: b\ncomponents: [a.lute]\n---\n## B.\n:line[x]: b\n",
    );
    let res = resolve_components(&dir, &["a.lute".to_string()], zero_span());
    assert!(
        codes(&res).contains(&"E-COMPONENT-CYCLE"),
        "a `components:` import cycle must flag E-COMPONENT-CYCLE; got {:?}",
        codes(&res)
    );
}

#[test]
fn diamond_is_one_identity_no_dup() {
    let dir = unique_dir();
    write_lute(&dir, "shared.lute", GREET);
    write_lute(
        &dir,
        "b.lute",
        "---\ncomponent: b\ncomponents: [shared.lute]\n---\n## B.\n:line[x]: b\n",
    );
    write_lute(
        &dir,
        "c.lute",
        "---\ncomponent: c\ncomponents: [shared.lute]\n---\n## C.\n:line[x]: c\n",
    );
    let res = resolve_components(
        &dir,
        &["b.lute".to_string(), "c.lute".to_string()],
        zero_span(),
    );
    assert!(
        !codes(&res).contains(&"E-COMPONENT-DUP"),
        "a diamond import of `shared` must not flag E-COMPONENT-DUP; got {:?}",
        codes(&res)
    );
    assert!(
        res.table.contains_key("greet")
            && res.table.contains_key("b")
            && res.table.contains_key("c"),
        "all three components must resolve once: {:?}",
        res.table.keys().collect::<Vec<_>>()
    );
}
