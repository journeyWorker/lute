//! Scene/schema composition imports (dsl §9.2): the resolved import result plus
//! the TOTAL, never-panicking DAG file resolver (`resolve_imports`). Two edge
//! kinds: `uses:` (PEER union, dup = error) and `extends:` (BASE layer,
//! override-allowed). Precedence low -> high: `extends` bases (recursively) <
//! `uses` peers; a closer layer OVERRIDES a base's same-named decl (no dup),
//! while a same-layer collision stays `E-USES-DUP-*`.
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::snapshot::CapabilitySnapshot;

use crate::meta::{parse_meta_kind, MetaKind, StateDecl, StateSchema};

/// The resolved result of a scene's `uses:` imports: the merged imported state
/// schema, the merged imported `defs` (untyped YAML values, like inline defs),
/// and every `E-USES-*` diagnostic produced while resolving them.
#[derive(Clone, Debug, Default)]
pub struct SchemaImports {
    pub state: StateSchema,
    pub defs: BTreeMap<String, serde_yaml::Value>,
    pub diags: Vec<Diagnostic>,
}

/// Resolve a document's composition imports (dsl §9.2) into a merged schema.
/// `base_dir` is the importing document's directory; each `uses`/`extends` entry
/// is a relative path. `uses` peers form one same-precedence layer (a name
/// declared by two peers is `E-USES-DUP-*`); each `extends` base is a LOWER,
/// overridable layer. `at` is the importing document's frontmatter span, used
/// for every diagnostic. TOTAL: any I/O/parse/cycle/dup failure yields a
/// diagnostic, never a panic.
pub fn resolve_imports(
    base_dir: &Path,
    uses: &[String],
    extends: &[String],
    at: Span,
) -> SchemaImports {
    let mut acc = Acc::default();
    let mut visited: BTreeSet<PathBuf> = BTreeSet::new();
    let mut stack: Vec<PathBuf> = Vec::new();
    // Peer imports sit at depth 0 (dup-guarded among peers); each `extends` base
    // is one layer BELOW (depth 1), overridable by the peer/closer layer.
    resolve_into(
        base_dir,
        uses,
        Edge::Uses,
        0,
        at,
        &mut acc,
        &mut visited,
        &mut stack,
    );
    resolve_into(
        base_dir,
        extends,
        Edge::Extends,
        1,
        at,
        &mut acc,
        &mut visited,
        &mut stack,
    );
    SchemaImports {
        state: acc.state,
        defs: acc.defs,
        diags: acc.diags,
    }
}

#[derive(Default)]
struct Acc {
    state: StateSchema,
    defs: BTreeMap<String, serde_yaml::Value>,
    diags: Vec<Diagnostic>,
    /// canonical file that first declared each state path / def name (for
    /// cross-file dup detection; a diamond re-import of the SAME file is skipped
    /// before it can dup).
    state_src: BTreeMap<String, PathBuf>,
    defs_src: BTreeMap<String, PathBuf>,
    /// composition depth of each name's current winner: 0 = the peer layer
    /// (`uses`, dup-guarded), >= 1 = an `extends` base layer. A same-depth
    /// collision is a dup; a cross-depth collision is an `extends` override
    /// (the closer/lower-depth decl wins, no dup).
    state_depth: BTreeMap<String, usize>,
    defs_depth: BTreeMap<String, usize>,
}

/// Which frontmatter edge reached an imported document — used only to word the
/// `E-USES-{NOT-FOUND,CYCLE}` messages accurately (`uses:` vs `extends:`).
#[derive(Clone, Copy)]
enum Edge {
    Uses,
    Extends,
}

impl Edge {
    fn label(self) -> &'static str {
        match self {
            Edge::Uses => "uses",
            Edge::Extends => "extends",
        }
    }
}

fn uses_diag(code: &str, message: String, at: Span) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity: Severity::Error,
        message,
        span: at,
        layer: Layer::Content,
        fixits: Vec::new(),
        provenance: None,
    }
}

#[allow(clippy::too_many_arguments)]
fn resolve_into(
    base_dir: &Path,
    refs: &[String],
    edge: Edge,
    depth: usize,
    at: Span,
    acc: &mut Acc,
    visited: &mut BTreeSet<PathBuf>,
    stack: &mut Vec<PathBuf>,
) {
    for r in refs {
        let joined = base_dir.join(r);
        // canonicalize does I/O; a missing file lands here (not a panic).
        let canon = match std::fs::canonicalize(&joined) {
            Ok(c) => c,
            Err(_) => {
                acc.diags.push(uses_diag(
                    "E-USES-NOT-FOUND",
                    format!(
                        "cannot resolve `{}:` import `{r}` (from {})",
                        edge.label(),
                        base_dir.display()
                    ),
                    at,
                ));
                continue;
            }
        };
        // Cycle check BEFORE the diamond-dedup check.
        if stack.contains(&canon) {
            let chain = stack
                .iter()
                .chain(std::iter::once(&canon))
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(" -> ");
            acc.diags.push(uses_diag(
                "E-USES-CYCLE",
                format!("`{}:` import cycle: {chain}", edge.label()),
                at,
            ));
            continue;
        }
        if visited.contains(&canon) {
            continue; // diamond: one file = one identity, processed once.
        }
        visited.insert(canon.clone());

        let text = match std::fs::read_to_string(&canon) {
            Ok(t) => t,
            Err(e) => {
                acc.diags.push(uses_diag(
                    "E-USES-NOT-FOUND",
                    format!("cannot read schema import `{}`: {e}", canon.display()),
                    at,
                ));
                continue;
            }
        };
        let (doc, pdiags) = lute_syntax::parse(&text);
        let (tm, mdiags) =
            parse_meta_kind(&doc.meta, &CapabilitySnapshot::default(), MetaKind::Schema);
        let issues = pdiags.len() + mdiags.len();
        if issues > 0 {
            acc.diags.push(uses_diag(
                "E-USES-PARSE",
                format!(
                    "schema import `{}` has parse/frontmatter errors ({issues} issue(s))",
                    canon.display()
                ),
                at,
            ));
        }
        for (path, decl) in &tm.state.decls {
            merge_state(acc, path, decl, &canon, depth, at);
        }
        for (name, v) in &tm.defs {
            merge_def(acc, name, v, &canon, depth, at);
        }
        // Recurse into this doc's OWN composition edges, relative to ITS
        // directory: `uses:` peers stay at the same layer; `extends:` bases sit
        // one layer deeper (lower precedence).
        stack.push(canon.clone());
        let parent = canon.parent().unwrap_or_else(|| Path::new("."));
        resolve_into(parent, &tm.uses, Edge::Uses, depth, at, acc, visited, stack);
        resolve_into(
            parent,
            &tm.extends,
            Edge::Extends,
            depth + 1,
            at,
            acc,
            visited,
            stack,
        );
        stack.pop();
    }
}

/// Merge one imported state decl at composition `depth`. A never-before-seen
/// path is recorded; a same-depth re-declaration by a DIFFERENT file is a peer
/// dup (`E-USES-DUP-STATE`); a cross-depth collision is an `extends` override —
/// the closer (lower-depth) decl wins with no dup, but a change to the declared
/// TYPE flags `E-EXTENDS-STATE-TYPE` (persisted state must keep a stable type).
fn merge_state(acc: &mut Acc, path: &str, decl: &StateDecl, canon: &Path, depth: usize, at: Span) {
    let prev = acc.state_src.get(path).cloned();
    match prev {
        None => {
            acc.state.decls.insert(path.to_string(), decl.clone());
            acc.state_src.insert(path.to_string(), canon.to_path_buf());
            acc.state_depth.insert(path.to_string(), depth);
        }
        Some(prev) if prev == canon => {} // same file (diamond): already recorded
        Some(prev) => {
            let prev_depth = acc.state_depth[path];
            if depth == prev_depth {
                acc.diags.push(uses_diag(
                    "E-USES-DUP-STATE",
                    format!(
                        "state path `{path}` is declared by two imports (`{}` and `{}`)",
                        prev.display(),
                        canon.display()
                    ),
                    at,
                ));
            } else {
                // Cross-layer override: identify base (deeper) vs override (closer)
                // for the type-stability check, then let the closer decl win.
                let existing_ty = acc.state.decls[path].ty.clone();
                let (base_ty, over_ty) = if depth < prev_depth {
                    (&existing_ty, &decl.ty) // existing is the deeper base
                } else {
                    (&decl.ty, &existing_ty) // incoming is the deeper base
                };
                if base_ty != over_ty {
                    acc.diags.push(uses_diag(
                        "E-EXTENDS-STATE-TYPE",
                        format!(
                            "state path `{path}` overrides base declared type {base_ty:?} with {over_ty:?}; persisted state must keep a stable type"
                        ),
                        at,
                    ));
                }
                if depth < prev_depth {
                    acc.state.decls.insert(path.to_string(), decl.clone());
                    acc.state_src.insert(path.to_string(), canon.to_path_buf());
                    acc.state_depth.insert(path.to_string(), depth);
                }
            }
        }
    }
}

/// Merge one imported def at composition `depth`. Same-depth cross-file
/// re-declaration is a peer dup (`E-USES-DUP-DEF`); a cross-depth collision is
/// an `extends` override (the closer decl replaces the base, no dup — defs are
/// pure CEL macros, so no type-stability concern).
fn merge_def(
    acc: &mut Acc,
    name: &str,
    v: &serde_yaml::Value,
    canon: &Path,
    depth: usize,
    at: Span,
) {
    let prev = acc.defs_src.get(name).cloned();
    match prev {
        None => {
            acc.defs.insert(name.to_string(), v.clone());
            acc.defs_src.insert(name.to_string(), canon.to_path_buf());
            acc.defs_depth.insert(name.to_string(), depth);
        }
        Some(prev) if prev == canon => {} // same file (diamond): already recorded
        Some(prev) => {
            let prev_depth = acc.defs_depth[name];
            if depth == prev_depth {
                acc.diags.push(uses_diag(
                    "E-USES-DUP-DEF",
                    format!(
                        "def `{name}` is declared by two imports (`{}` and `{}`)",
                        prev.display(),
                        canon.display()
                    ),
                    at,
                ));
            } else if depth < prev_depth {
                acc.defs.insert(name.to_string(), v.clone());
                acc.defs_src.insert(name.to_string(), canon.to_path_buf());
                acc.defs_depth.insert(name.to_string(), depth);
            }
        }
    }
}
