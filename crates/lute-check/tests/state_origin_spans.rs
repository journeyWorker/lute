//! #189, spec 0.49.0 §9.3: where an imported or inline `per:` family's
//! members are declared is located only when a diagnostic names one.
//! Locating a path ([`lute_check::rel_schema::state_key_span`]) scans the
//! file, so locating every member of a kind of `n` made a check O(n²).
#![expect(
    clippy::disallowed_methods,
    reason = "a test reads the fixtures it wrote; no cache is involved"
)]

use std::path::{Path, PathBuf};

use lute_check::rel_schema::state_key_span_count;
use lute_check::{fold_env, resolve_imports, CheckInput, FoldedEnv, Mode, ProjectDoc};
use lute_core_span::{Diagnostic, Span};
use lute_syntax::ast::Document;

const MEMBERS: usize = 100_000;

fn ids(range: std::ops::Range<usize>) -> String {
    range
        .map(|i| format!("m{i:06}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// A schema whose `npc` lists the first half of `n` ids and whose sub-kind
/// `bonded` adds the second half, with `user.seen` a family over `npc`.
fn schema(n: usize) -> String {
    format!(
        "state:\n  user.seen: {{ type: bool, default: false, per: npc }}\n\
         entities:\n  npc: {{ members: [{}] }}\n  \
         bonded: {{ subsetOf: npc, members: [{}] }}\n",
        ids(0..n / 2),
        ids(n / 2..n),
    )
}

/// `body`'s frontmatter lines in a scene `id`.
fn scene(id: &str, body: &str) -> String {
    format!("---\nkind: scene\nid: {id}\ntitle: T\n{body}---\n\n## Shot 1.\n\n@narrator: hi\n")
}

/// One project document, folded the way the project check folds it.
struct Folded {
    path: PathBuf,
    doc: Document,
    folded: FoldedEnv,
}

fn fold(dir: &Path, name: &str, text: &str, uses: &[&str]) -> Folded {
    let uses: Vec<String> = uses.iter().map(ToString::to_string).collect();
    let at = Span {
        byte_start: 0,
        byte_end: 0,
        line: 1,
        column: 1,
        utf16_range: (0, 0),
    };
    let imports = resolve_imports(dir, &uses, &[], at);
    assert!(imports.diags.is_empty(), "{:?}", imports.diags);
    let path = dir.join(name);
    std::fs::write(&path, text).unwrap();
    let input = CheckInput::new(
        text.to_string(),
        name.into(),
        Mode::Ci,
        lute_check::InputSources::from_parts(
            lute_manifest::core::load_core_snapshot(),
            Default::default(),
            imports,
            Default::default(),
            Default::default(),
        ),
    );
    let (doc, _) = lute_syntax::parse(text);
    let (folded, _, _) = fold_env(&doc, &input);
    Folded { path, doc, folded }
}

fn project_check(docs: &[Folded]) -> Vec<(PathBuf, Diagnostic)> {
    let views: Vec<ProjectDoc<'_>> = docs
        .iter()
        .map(|d| ProjectDoc::new(&d.path, &d.doc, &d.folded.typed, &d.folded.snapshot))
        .collect();
    let foldeds: Vec<&FoldedEnv> = docs.iter().map(|d| &d.folded).collect();
    lute_check::state_decls::check_project_state_decls(&views, &foldeds)
}

/// Two scenes import a 100 000-member family, each through its own import
/// resolution, and one declares an inline family over the imported kind.
/// Nothing is reported, so no member is located: the check locates no
/// state path at all (it located each member, several times, before).
#[test]
fn a_100_000_member_family_is_checked_without_locating_a_member() {
    let dir = lute_scratch::ScratchDir::new();
    std::fs::write(dir.join("w.yaml"), schema(MEMBERS)).unwrap();
    let before = state_key_span_count();
    let docs = [
        fold(&dir, "a.lute", &scene("a", "uses: w.yaml\n"), &["w.yaml"]),
        fold(
            &dir,
            "b.lute",
            &scene(
                "b",
                "uses: w.yaml\nstate:\n  user.met: { type: bool, default: false, per: npc }\n",
            ),
            &["w.yaml"],
        ),
    ];
    let seen = docs[1].folded.env.state.get("user.seen.m099999");
    assert!(
        seen.is_some(),
        "the imported family declares a sub-kind member"
    );
    assert!(
        docs[1].folded.env.state.get("user.met.m000000").is_some(),
        "the inline family declares a member"
    );
    let diags = project_check(&docs);
    assert!(diags.is_empty(), "{diags:?}");
    let located = state_key_span_count() - before;
    assert_eq!(
        located, 0,
        "import resolution, folding and the project check located {located} state paths \
         for families of {MEMBERS} members"
    );
}

/// A member a family declares, redeclared differently by a document that
/// does not import the family, is reported at the member's place in the
/// schema: its listing in the kind, the place `state_key_span` gives a
/// member path the file does not spell out.
#[test]
fn a_conflicting_family_member_is_reported_at_its_listing() {
    let dir = lute_scratch::ScratchDir::new();
    std::fs::write(
        dir.join("w.yaml"),
        "state:\n  user.seen: { type: bool, default: false, per: npc }\n\
         entities:\n  npc: { members: [m0, m1, m2] }\n",
    )
    .unwrap();
    let docs = [
        fold(&dir, "a.lute", &scene("a", "uses: w.yaml\n"), &["w.yaml"]),
        fold(
            &dir,
            "c.lute",
            &scene("c", "state:\n  user.seen.m1: { type: int, default: 0 }\n"),
            &[],
        ),
    ];
    let before = state_key_span_count();
    let diags = project_check(&docs);
    let located = state_key_span_count() - before;
    let [(file, diag)] = diags.as_slice() else {
        panic!("one conflict expected: {diags:?}");
    };
    assert_eq!(diag.code, "E-STATE-DECL-CONFLICT");
    let schema = std::fs::canonicalize(dir.join("w.yaml")).unwrap();
    assert_eq!(file, &schema, "{diag:?}");
    // `  npc: { members: [m0, m1, m2] }`: `m1` at column 24 of line 4.
    assert_eq!((diag.span.line, diag.span.column), (4, 24), "{diag:?}");
    assert_eq!(diag.span.byte_end - diag.span.byte_start, "m1".len());
    assert!(
        diag.message.contains(&format!("{}:4", schema.display())),
        "{}",
        diag.message
    );
    let related = &diag.related[0].diagnostic;
    assert_eq!(related.span.line, 6, "c.lute's `user.seen.m1:` key");
    assert_eq!(located, 2, "only the two declarations reported are located");
}
