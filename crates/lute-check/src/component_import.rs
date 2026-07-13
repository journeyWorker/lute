//! Reusable-content component imports (dsl §13): the resolved component set plus
//! the TOTAL, never-panicking DAG file resolver (`resolve_components`), mirroring
//! the `schema_import` resolver (dsl §9.2).
//!
//! A scene declares `components: [path, …]`; a component file declares
//! `component: <name>` + `params: { p: <type> }` (frontmatter) and a
//! presentational body. A component file MAY itself import further components via
//! its own `components:`, so the reachable files form a DAG. `resolve_components`:
//!
//! 1. **Traverses** the `components:` DAG from `base_dir`, canonicalizing each
//!    relative ref (so a diamond — two importers of one file — is a single
//!    identity) and parsing each canonical file exactly once in
//!    [`MetaKind::Component`]. An unreadable/unresolvable ref or a parse/
//!    frontmatter error is `E-COMPONENT-PARSE`; a directed `components:` import
//!    cycle is `E-COMPONENT-CYCLE`.
//! 2. **Builds** the flat `name -> ComponentDef` table. A component file that
//!    declares no `component:` name is `E-COMPONENT-PARSE`; two DISTINCT files
//!    declaring the SAME `component:` name is `E-COMPONENT-DUP` (the byte-sorted-
//!    first file wins the table entry, for a stable, order-independent result).
//!
//! The `::use` invocation checks (declared / named-arg match / expansion cycle)
//! and the presentational-body validation live in `check.rs` (Task C3); this
//! module only LOADS the component files.
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

use lute_core_span::{Diagnostic, Layer, RelatedDiagnostic, Severity, Span};
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_manifest::types::Type;
use lute_syntax::ast::Document;

use crate::meta::{parse_meta_kind, MetaKind};

/// One resolved component (dsl §13): its ordered params (the `::use` named-arg
/// namespace, source order), its parsed presentational body document, and the
/// file it was loaded from.
#[derive(Clone, Debug)]
pub struct ComponentDef {
    pub params: Vec<(String, Type)>,
    pub body: Document,
    pub src: PathBuf,
}

/// The resolved set of a scene's `components:` imports (dsl §13): the component
/// name -> definition table plus the resolution diagnostics. `default()` (empty)
/// on a scene with no `components:`, or on a surface that cannot resolve files.
#[derive(Clone, Debug, Default)]
pub struct ComponentSet {
    pub table: BTreeMap<String, ComponentDef>,
    pub diags: Vec<Diagnostic>,
}

fn comp_diag(code: &str, message: String, at: Span) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity: Severity::Error,
        message,
        span: at,
        layer: Layer::Content,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

/// The parsed subset of one component file kept after traversal.
struct ParsedComponent {
    /// The declared `component:` name, or `None` when the file omits it (an
    /// `E-COMPONENT-PARSE` — such a file never enters the table).
    name: Option<String>,
    params: Vec<(String, Type)>,
    body: Document,
    src: PathBuf,
}

/// Resolve a scene's `components:` imports (dsl §13) into a `name -> ComponentDef`
/// table. `base_dir` is the importing document's directory; each entry is a
/// relative path. `at` is the importing document's frontmatter span, used for
/// every diagnostic. TOTAL: any I/O/parse/cycle/dup failure yields a diagnostic,
/// never a panic; the result is INDEPENDENT of the order of the `components:`
/// entries.
pub fn resolve_components(base_dir: &Path, components: &[String], at: Span) -> ComponentSet {
    let mut diags = Vec::new();

    // Phase 1: traverse the `components:` DAG, parsing each canonical file once.
    // `parsed` = its component decl (keyed by canonical path, so a diamond is one
    // identity); `adj` = its out-edges, for cycle detection.
    let mut parsed: BTreeMap<PathBuf, ParsedComponent> = BTreeMap::new();
    let mut adj: BTreeMap<PathBuf, Vec<PathBuf>> = BTreeMap::new();
    let mut dq: VecDeque<PathBuf> = VecDeque::new();

    for canon in resolve_edges(base_dir, components, &mut diags, at) {
        dq.push_back(canon);
    }
    while let Some(canon) = dq.pop_front() {
        if parsed.contains_key(&canon) {
            continue;
        }
        let (pc, child_refs) = read_and_parse(&canon, &mut diags, at);
        let dir = canon
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let children = resolve_edges(&dir, &child_refs, &mut diags, at);
        for c in &children {
            dq.push_back(c.clone());
        }
        adj.insert(canon.clone(), children);
        parsed.insert(canon, pc);
    }

    // Directed-cycle detection over the reachable `components:` subgraph.
    detect_cycles(&adj, &mut diags, at);

    // Phase 2: build the flat name table. Iterating `parsed` (a BTreeMap keyed by
    // canonical path) is byte-sorted, so on a cross-file name collision the
    // sorted-first file wins the entry and the later one flags `E-COMPONENT-DUP`
    // deterministically.
    let mut table: BTreeMap<String, ComponentDef> = BTreeMap::new();
    for pc in parsed.values() {
        let Some(name) = pc.name.clone() else {
            continue; // missing `component:` name — already `E-COMPONENT-PARSE`.
        };
        if let Some(existing) = table.get(&name) {
            diags.push(comp_diag(
                "E-COMPONENT-DUP",
                format!(
                    "component `{name}` is declared by two files (`{}` and `{}`)",
                    existing.src.display(),
                    pc.src.display()
                ),
                at,
            ));
            continue;
        }
        table.insert(
            name,
            ComponentDef {
                params: pc.params.clone(),
                body: pc.body.clone(),
                src: pc.src.clone(),
            },
        );
    }

    ComponentSet { table, diags }
}

/// dsl 0.5.1 §4: fold a COPY of each per-component BODY diagnostic
/// (`E-COMPONENT-STATE` et al. — produced by `check.rs`'s
/// `validate_components`, which needs the snapshot/providers/domain context
/// this module never has, so it cannot run that pass itself; an
/// unattributed `E-COMPONENT-CYCLE` from `detect_use_cycles` carries an
/// empty `PathBuf` and so can never match below) into the SAME component's
/// own `E-COMPONENT-PARSE` "(N issue(s))" diagnostic's `related` list, when
/// `component_diags` already carries one for that canonical file (`src`). A
/// component that both fails to parse cleanly (a genuine `pdiags`/`mdiags`
/// issue) AND has a body-level semantic defect — an ambient-state read,
/// reclassified to `E-COMPONENT-STATE` when found inside a component body —
/// surfaces BOTH as children of ONE aggregate import failure, with an
/// accurate count: the "(N issue(s))" message previously only reflected the
/// raw parse/frontmatter count, under-reporting any body diagnostic that
/// would otherwise be folded on afterward.
///
/// EVERY `(src, diag)` pair is returned UNCHANGED (in input order) for the
/// caller to keep extending its flat top-level diagnostic list exactly as
/// before this pass existed — merging is a strictly ADDITIVE cross-reference
/// on the matching `E-COMPONENT-PARSE`'s `related`, never a relocation: a
/// consumer that already finds `E-COMPONENT-STATE` (or any other body code)
/// by scanning the flat list must keep finding it there, parse failure or
/// not.
pub fn merge_component_body_diags(
    component_diags: &mut [Diagnostic],
    body_diags: Vec<(PathBuf, Diagnostic)>,
) -> Vec<Diagnostic> {
    let mut out = Vec::with_capacity(body_diags.len());
    for (src, diag) in body_diags {
        let file = src.display().to_string();
        if let Some(parent) = component_diags.iter_mut().find(|d| {
            d.code == "E-COMPONENT-PARSE" && d.related.iter().any(|r| r.file == file)
        }) {
            parent.related.push(RelatedDiagnostic { file: file.clone(), diagnostic: diag.clone() });
            let n = parent.related.len();
            parent.message = format!(
                "component import `{file}` has parse/frontmatter errors ({n} issue(s))"
            );
        }
        out.push(diag);
    }
    out
}

/// Canonicalize each relative `components:` ref against `dir`; a missing/
/// unresolvable target is `E-COMPONENT-PARSE` (canonicalize does I/O, so a bad
/// path lands here, never a panic). Returns the successfully-resolved canonical
/// paths.
fn resolve_edges(
    dir: &Path,
    refs: &[String],
    diags: &mut Vec<Diagnostic>,
    at: Span,
) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for r in refs {
        match std::fs::canonicalize(dir.join(r)) {
            Ok(c) => out.push(c),
            Err(_) => diags.push(comp_diag(
                "E-COMPONENT-PARSE",
                format!(
                    "cannot resolve `components:` import `{r}` (from {})",
                    dir.display()
                ),
                at,
            )),
        }
    }
    out
}

/// Read + parse one canonical component file. An I/O failure or any parse/
/// frontmatter error is `E-COMPONENT-PARSE`; a file that declares no `component:`
/// name is also `E-COMPONENT-PARSE` (and never enters the table); a PRESENT but
/// malformed `params:` (not a mapping, non-string key, or a value that is not a
/// valid `Type`) is likewise `E-COMPONENT-PARSE` — the signature is never
/// silently shrunk. Returns the file's declared name/params/body plus its own
/// `components:` refs (for further traversal).
fn read_and_parse(
    canon: &Path,
    diags: &mut Vec<Diagnostic>,
    at: Span,
) -> (ParsedComponent, Vec<String>) {
    let text = match std::fs::read_to_string(canon) {
        Ok(t) => t,
        Err(e) => {
            diags.push(comp_diag(
                "E-COMPONENT-PARSE",
                format!("cannot read component import `{}`: {e}", canon.display()),
                at,
            ));
            let (empty, _) = lute_syntax::parse("");
            return (
                ParsedComponent {
                    name: None,
                    params: Vec::new(),
                    body: empty,
                    src: canon.to_path_buf(),
                },
                Vec::new(),
            );
        }
    };
    let (doc, pdiags) = lute_syntax::parse(&text);
    let (tm, mdiags) = parse_meta_kind(
        &doc.meta,
        &CapabilitySnapshot::default(),
        MetaKind::Component,
    );
    let issues = pdiags.len() + mdiags.len();
    if issues > 0 {
        let file = canon.display().to_string();
        // dsl 0.5.0 §2.2: carry the component's OWN diagnostics (spans
        // relative to the component file) onto the importer's
        // `E-COMPONENT-PARSE`, so `--json` — and the human renderer, which
        // walks `related` — surface what actually failed without a separate
        // re-`check` of the component file.
        let mut d = comp_diag(
            "E-COMPONENT-PARSE",
            format!(
                "component import `{}` has parse/frontmatter errors ({issues} issue(s))",
                canon.display()
            ),
            at,
        );
        d.related = pdiags
            .iter()
            .chain(mdiags.iter())
            .cloned()
            .map(|diagnostic| RelatedDiagnostic { file: file.clone(), diagnostic })
            .collect();
        diags.push(d);
    }
    if tm.component.is_none() {
        diags.push(comp_diag(
            "E-COMPONENT-PARSE",
            format!(
                "component file `{}` must declare a `component:` name (dsl §13)",
                canon.display()
            ),
            at,
        ));
    }
    if tm.params_malformed {
        diags.push(comp_diag(
            "E-COMPONENT-PARSE",
            format!(
                "component file `{}` has a malformed `params:` — each entry must be `name: <type>` (dsl §13)",
                canon.display()
            ),
            at,
        ));
    }
    let params = tm
        .params
        .iter()
        .map(|p| (p.name.clone(), p.ty.clone()))
        .collect();
    (
        ParsedComponent {
            name: tm.component.clone(),
            params,
            body: doc,
            src: canon.to_path_buf(),
        },
        tm.components.clone(),
    )
}

/// Detect any directed cycle in the reachable `components:` subgraph and report
/// it as `E-COMPONENT-CYCLE`. Standard DFS 3-coloring: a `gray` (on-stack) target
/// is a back edge. Roots and neighbors are visited in sorted order for a
/// deterministic, order-independent result.
fn detect_cycles(adj: &BTreeMap<PathBuf, Vec<PathBuf>>, diags: &mut Vec<Diagnostic>, at: Span) {
    let mut on_stack: BTreeSet<PathBuf> = BTreeSet::new();
    let mut done: BTreeSet<PathBuf> = BTreeSet::new();
    let mut stack: Vec<PathBuf> = Vec::new();
    for start in adj.keys() {
        if !done.contains(start) && !on_stack.contains(start) {
            dfs_cycle(start, adj, &mut on_stack, &mut done, &mut stack, diags, at);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn dfs_cycle(
    node: &Path,
    adj: &BTreeMap<PathBuf, Vec<PathBuf>>,
    on_stack: &mut BTreeSet<PathBuf>,
    done: &mut BTreeSet<PathBuf>,
    stack: &mut Vec<PathBuf>,
    diags: &mut Vec<Diagnostic>,
    at: Span,
) {
    on_stack.insert(node.to_path_buf());
    stack.push(node.to_path_buf());
    if let Some(edges) = adj.get(node) {
        let mut targets: Vec<&PathBuf> = edges.iter().collect();
        targets.sort();
        targets.dedup();
        for nbr in targets {
            if on_stack.contains(nbr) {
                // Back edge -> cycle: report the chain from `nbr` around to `node`.
                let start_idx = stack.iter().position(|p| p == nbr).unwrap_or(0);
                let chain = stack[start_idx..]
                    .iter()
                    .chain(std::iter::once(nbr))
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(" -> ");
                diags.push(comp_diag(
                    "E-COMPONENT-CYCLE",
                    format!("`components:` import cycle: {chain}"),
                    at,
                ));
            } else if !done.contains(nbr) {
                dfs_cycle(nbr, adj, on_stack, done, stack, diags, at);
            }
        }
    }
    stack.pop();
    on_stack.remove(node);
    done.insert(node.to_path_buf());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zero_span() -> Span {
        Span { byte_start: 0, byte_end: 0, line: 1, column: 1, utf16_range: (0, 0) }
    }

    fn body_diag(code: &str, message: &str) -> Diagnostic {
        Diagnostic {
            code: code.to_string(),
            severity: Severity::Error,
            message: message.to_string(),
            span: zero_span(),
            layer: Layer::Staging,
            fixits: Vec::new(),
            provenance: None,
            covered: Vec::new(),
            related: Vec::new(),
        }
    }

    /// A component that failed `E-COMPONENT-PARSE` with exactly one
    /// pre-existing child (mirrors `read_and_parse`'s own construction: the
    /// "(N issue(s))" count matches `related.len()` at push time).
    fn parse_failure_with_one_child(file: &str) -> Diagnostic {
        let mut d = comp_diag(
            "E-COMPONENT-PARSE",
            format!("component import `{file}` has parse/frontmatter errors (1 issue(s))"),
            zero_span(),
        );
        d.related = vec![RelatedDiagnostic {
            file: file.to_string(),
            diagnostic: body_diag("E-UNCLASSIFIED", "unrecognized line"),
        }];
        d
    }

    #[test]
    fn folds_a_matching_body_diag_into_related_and_updates_the_count() {
        let file = "comp.lute";
        let mut component_diags = vec![parse_failure_with_one_child(file)];
        let body_diag_entry = body_diag("E-COMPONENT-STATE", "ambient state read in a component body");
        let body_diags = vec![(PathBuf::from(file), body_diag_entry.clone())];

        let out = merge_component_body_diags(&mut component_diags, body_diags);
        assert_eq!(
            out,
            vec![body_diag_entry],
            "the body diagnostic must still be returned for the caller's own top-level list"
        );
        assert_eq!(component_diags.len(), 1, "{component_diags:?}");
        let parent = &component_diags[0];
        assert_eq!(parent.related.len(), 2, "{parent:?}");
        assert!(
            parent.related.iter().any(|r| r.diagnostic.code == "E-COMPONENT-STATE"),
            "{parent:?}"
        );
        assert!(
            parent.message.contains("(2 issue(s))"),
            "count must reflect BOTH children, not just the original parse issue: {}",
            parent.message
        );
    }

    #[test]
    fn leaves_an_unmatched_body_diag_untouched_when_no_parse_failure_exists() {
        // The common case: a component that imports CLEANLY (no
        // `E-COMPONENT-PARSE` at all) but has a body-level semantic defect.
        let mut component_diags: Vec<Diagnostic> = Vec::new();
        let diag = body_diag("E-COMPONENT-STATE", "ambient state read in a component body");
        let body_diags = vec![(PathBuf::from("clean.lute"), diag.clone())];

        let out = merge_component_body_diags(&mut component_diags, body_diags);
        assert_eq!(out, vec![diag]);
        assert!(component_diags.is_empty());
    }

    #[test]
    fn leaves_an_unattributed_cycle_diag_untouched() {
        // `detect_use_cycles`' output is never file-attributable -- callers
        // pass it through with an empty `PathBuf`, which must never spuriously
        // match a REAL component's `E-COMPONENT-PARSE` (whose `related[].file`
        // is always a real canonical path, never empty).
        let mut component_diags = vec![parse_failure_with_one_child("comp.lute")];
        let cycle = body_diag("E-COMPONENT-CYCLE", "`components:` import cycle: a -> b -> a");
        let body_diags = vec![(PathBuf::new(), cycle.clone())];

        let out = merge_component_body_diags(&mut component_diags, body_diags);
        assert_eq!(out, vec![cycle]);
        assert_eq!(component_diags[0].related.len(), 1, "untouched: {component_diags:?}");
    }

    #[test]
    fn only_merges_into_the_matching_files_own_parse_failure() {
        let mut component_diags =
            vec![parse_failure_with_one_child("a.lute"), parse_failure_with_one_child("b.lute")];
        let body_diag_entry = body_diag("E-COMPONENT-STATE", "ambient state read in a component body");
        let body_diags = vec![(PathBuf::from("b.lute"), body_diag_entry.clone())];

        let out = merge_component_body_diags(&mut component_diags, body_diags);
        assert_eq!(out, vec![body_diag_entry]);
        assert_eq!(component_diags[0].related.len(), 1, "a.lute must stay untouched");
        assert!(component_diags[0].message.contains("(1 issue(s))"));
        assert_eq!(component_diags[1].related.len(), 2, "b.lute gets the merge");
        assert!(component_diags[1].message.contains("(2 issue(s))"));
    }

    #[test]
    fn every_body_diag_is_returned_regardless_of_merge_outcome() {
        // The relocation-safety contract: a caller extending its flat
        // top-level list with the returned `Vec` must see EVERY body
        // diagnostic, in input order, whether or not it also got folded
        // into some `E-COMPONENT-PARSE`'s `related` (dsl 0.5.1 §4 must be
        // strictly additive, never a relocation — pre-existing consumers
        // that scan the flat list for e.g. `E-COMPONENT-STATE` must keep
        // finding it there).
        let mut component_diags = vec![parse_failure_with_one_child("a.lute")];
        let matched = body_diag("E-COMPONENT-STATE", "matched");
        let unmatched = body_diag("E-COMPONENT-BODY", "unmatched");
        let body_diags = vec![
            (PathBuf::from("a.lute"), matched.clone()),
            (PathBuf::from("clean.lute"), unmatched.clone()),
        ];

        let out = merge_component_body_diags(&mut component_diags, body_diags);
        assert_eq!(out, vec![matched, unmatched]);
    }
}
