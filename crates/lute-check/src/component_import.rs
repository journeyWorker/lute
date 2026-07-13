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
