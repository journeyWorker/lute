//! Scene `uses:` schema imports (dsl §9.2): the resolved import result plus the
//! TOTAL, never-panicking DAG file resolver (`resolve_imports`).
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::snapshot::CapabilitySnapshot;

use crate::meta::{parse_meta_kind, MetaKind, StateSchema};

/// The resolved result of a scene's `uses:` imports: the merged imported state
/// schema, the merged imported `defs` (untyped YAML values, like inline defs),
/// and every `E-USES-*` diagnostic produced while resolving them.
#[derive(Clone, Debug, Default)]
pub struct SchemaImports {
    pub state: StateSchema,
    pub defs: BTreeMap<String, serde_yaml::Value>,
    pub diags: Vec<Diagnostic>,
}

/// Resolve a document's `uses:` imports (dsl §9.2) into a merged schema.
/// `base_dir` is the importing document's directory; each `uses` entry is a
/// relative path. `at` is the importing document's frontmatter span, used for
/// every diagnostic. TOTAL: any I/O/parse/cycle/dup failure yields a diagnostic,
/// never a panic.
pub fn resolve_imports(base_dir: &Path, uses: &[String], at: Span) -> SchemaImports {
    let mut acc = Acc::default();
    let mut visited: BTreeSet<PathBuf> = BTreeSet::new();
    let mut stack: Vec<PathBuf> = Vec::new();
    resolve_into(base_dir, uses, at, &mut acc, &mut visited, &mut stack);
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

fn resolve_into(
    base_dir: &Path,
    uses: &[String],
    at: Span,
    acc: &mut Acc,
    visited: &mut BTreeSet<PathBuf>,
    stack: &mut Vec<PathBuf>,
) {
    for r in uses {
        let joined = base_dir.join(r);
        // canonicalize does I/O; a missing file lands here (not a panic).
        let canon = match std::fs::canonicalize(&joined) {
            Ok(c) => c,
            Err(_) => {
                acc.diags.push(uses_diag(
                    "E-USES-NOT-FOUND",
                    format!(
                        "cannot resolve `uses:` import `{r}` (from {})",
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
                format!("`uses:` import cycle: {chain}"),
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
        let (doc, _pdiags) = lute_syntax::parse(&text);
        let (tm, mdiags) =
            parse_meta_kind(&doc.meta, &CapabilitySnapshot::default(), MetaKind::Schema);
        if !mdiags.is_empty() {
            acc.diags.push(uses_diag(
                "E-USES-PARSE",
                format!(
                    "schema import `{}` has frontmatter errors ({} issue(s))",
                    canon.display(),
                    mdiags.len()
                ),
                at,
            ));
        }
        for (path, decl) in &tm.state.decls {
            match acc.state_src.get(path) {
                Some(prev) if prev != &canon => acc.diags.push(uses_diag(
                    "E-USES-DUP-STATE",
                    format!(
                        "state path `{path}` is declared by two imports (`{}` and `{}`)",
                        prev.display(),
                        canon.display()
                    ),
                    at,
                )),
                Some(_) => {}
                None => {
                    acc.state.decls.insert(path.clone(), decl.clone());
                    acc.state_src.insert(path.clone(), canon.clone());
                }
            }
        }
        for (name, v) in &tm.defs {
            match acc.defs_src.get(name) {
                Some(prev) if prev != &canon => acc.diags.push(uses_diag(
                    "E-USES-DUP-DEF",
                    format!(
                        "def `{name}` is declared by two imports (`{}` and `{}`)",
                        prev.display(),
                        canon.display()
                    ),
                    at,
                )),
                Some(_) => {}
                None => {
                    acc.defs.insert(name.clone(), v.clone());
                    acc.defs_src.insert(name.clone(), canon.clone());
                }
            }
        }
        // Recurse into the schema doc's own `uses:`, relative to ITS directory.
        stack.push(canon.clone());
        let parent = canon.parent().unwrap_or_else(|| Path::new("."));
        resolve_into(parent, &tm.uses, at, acc, visited, stack);
        stack.pop();
    }
}
