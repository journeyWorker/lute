//! Scene/schema composition imports (dsl §9.2): the resolved import result plus
//! the TOTAL, never-panicking DAG file resolver (`resolve_imports`). Two edge
//! kinds: `uses:` (PEER union, dup = error) and `extends:` (BASE layer,
//! override-allowed).
//!
//! The resolver is COLLECT-THEN-RESOLVE and ORDER-INDEPENDENT:
//!
//! 1. **Traverse** the import DAG, recording each canonical file at its
//!    SHALLOWEST composition depth. From a doc at depth `d`, its `uses:` targets
//!    are peers at depth `d`, its `extends:` targets are bases at depth `d + 1`;
//!    the root's `uses:` sit at depth 0 and its `extends:` at depth 1. A 0-1 BFS
//!    (uses = weight 0, extends = weight 1) finalizes each file at its MINIMUM
//!    depth, so a diamond is one identity and a file reached both as a peer and a
//!    base counts as a peer. Missing/unreadable -> `E-USES-NOT-FOUND`;
//!    parse/frontmatter errors -> `E-USES-PARSE`; a directed cycle ->
//!    `E-USES-CYCLE`.
//! 2. **Resolve** each declared NAME (state path / def) from every declaring
//!    `(file, depth, decl)`: a depth level with >= 2 DISTINCT files declaring the
//!    name is a same-level collision (`E-USES-DUP-*`, a `uses` peer dup OR a
//!    base-base dup — never hidden by a closer override); the winner is the
//!    MIN-depth decl (byte-sorted-first file breaks a tie for stability); a
//!    deeper STATE decl whose `type` differs from the winner is
//!    `E-EXTENDS-STATE-TYPE`. A state path whose winner came from an `extends`
//!    base (depth >= 1) is marked `overridable`, so the importing scene's inline
//!    `state:` may refine it (dsl §9.2), while a `uses`-peer path may not.
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::snapshot::{CapabilitySnapshot, Domain};

use crate::meta::{parse_meta_kind, MetaKind, StateDecl, StateSchema};

/// The resolved result of a scene's composition imports (dsl §9.2): the merged
/// imported state schema, the merged imported `defs` (untyped YAML values, like
/// inline defs), the resolution diagnostics, and the state paths the importing
/// scene may inline-refine.
#[derive(Clone, Debug, Default)]
pub struct SchemaImports {
    pub state: StateSchema,
    pub defs: BTreeMap<String, serde_yaml::Value>,
    /// Project-authored `enums:`/`entities:` domains (dsl data-catalog
    /// foundation A3), merged across the import graph the SAME way as
    /// `state`/`defs` above: gathered per name from every reachable schema
    /// doc, a name declared by >= 2 DISTINCT files is a cross-source
    /// collision — reported as `E-DOMAIN-DUP` (A2's cross-plugin dup code,
    /// reused verbatim rather than a new `E-USES-DUP-*` code, per the
    /// data-catalog foundation design) — and the winner is the MIN-depth
    /// decl (byte-sorted-first file breaks a tie). UNLIKE `state`/`defs`
    /// there is no `extends`-base "may add members" relaxation yet (0.3.0
    /// leaves additive `extends` growth for entity/relation composition;
    /// deferred here, see the module docs). This is ONLY the
    /// project-declared side; [`merge_domains`] unions it with the plugin/
    /// core baseline (`CapabilitySnapshot.domains`) — THAT is the actual
    /// merged vocabulary the checker consults for `Type::Domain` resolution.
    pub domains: BTreeMap<String, Domain>,
    pub diags: Vec<Diagnostic>,
    /// State paths whose resolved winner came from an `extends` base (composition
    /// depth >= 1). The importing scene's inline `state:` MAY refine such a path
    /// (override its default; a type change is `E-EXTENDS-STATE-TYPE`), whereas a
    /// path resolved from a `uses` peer (depth 0) stays `E-STATE-REDECLARE` if the
    /// scene redeclares it.
    pub state_overridable: BTreeSet<String>,
    /// Every `<quest id>` reachable via the import graph (dsl 0.2.0 §6.3: a quest
    /// id is unique PROJECT-WIDE — "like a named `run.*` fact ... not an
    /// implementation leak" — not merely per document), keyed by id -> one
    /// declaring file (byte-sorted-first on a same-id collision, for messaging).
    /// A collision BETWEEN two import-reachable docs (neither the document under
    /// check) is reported directly (`resolve_imports`, `E-QUEST-ID-DUP`), since
    /// the importing document's own `<quest>` fold (`check_quest`) only ever
    /// walks ITS OWN `<quest>`s; that fold instead seeds its `seen_quests` set
    /// from these keys, so redeclaring an import-reachable id is
    /// `E-QUEST-ID-DUP` too.
    pub imported_quest_ids: BTreeMap<String, PathBuf>,
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

/// The parsed subset of one imported doc kept after traversal: its declared
/// state paths, defs, project-authored domains, and `<quest id>`s (the doc's
/// own edges are consumed during traversal).
struct ParsedDoc {
    state: BTreeMap<String, StateDecl>,
    defs: BTreeMap<String, serde_yaml::Value>,
    domains: BTreeMap<String, Domain>,
    /// Every non-empty `<quest id>` this doc declares (dsl 0.2.0 §6.3
    /// project-wide uniqueness); an id-less `<quest>` is that doc's OWN
    /// malformed-id problem (`E-QUEST-ID-MISSING`, reported when THAT doc is
    /// directly checked), not something this traversal can meaningfully
    /// collide on.
    quest_ids: BTreeSet<String>,
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

/// Resolve a document's composition imports (dsl §9.2) into a merged schema.
/// `base_dir` is the importing document's directory; each `uses`/`extends` entry
/// is a relative path. `at` is the importing document's frontmatter span, used
/// for every diagnostic. TOTAL: any I/O/parse/cycle/dup failure yields a
/// diagnostic, never a panic; the result is INDEPENDENT of the order of the
/// `uses`/`extends` entries.
pub fn resolve_imports(
    base_dir: &Path,
    uses: &[String],
    extends: &[String],
    at: Span,
) -> SchemaImports {
    let mut diags = Vec::new();

    // --- Phase 1: traverse the DAG, finalizing each file at its SHALLOWEST depth.
    // `dist` = min composition depth per canonical file; `parsed` = its declared
    // state/defs (parsed exactly once); `adj` = out-edges, for cycle detection.
    let mut dist: BTreeMap<PathBuf, usize> = BTreeMap::new();
    let mut parsed: BTreeMap<PathBuf, ParsedDoc> = BTreeMap::new();
    let mut adj: BTreeMap<PathBuf, Vec<(PathBuf, Edge)>> = BTreeMap::new();
    // 0-1 BFS deque: `uses` edges (weight 0) push to the FRONT, `extends` (weight
    // 1) to the BACK, so files pop in non-decreasing depth order and each is
    // finalized (and its edges relaxed) at its true minimum depth.
    let mut dq: VecDeque<(usize, PathBuf)> = VecDeque::new();

    // Seed from the root's own edges (the root is virtual, at depth 0).
    for canon in resolve_edges(base_dir, uses, Edge::Uses, &mut diags, at) {
        relax(canon, 0, true, &mut dist, &mut dq);
    }
    for canon in resolve_edges(base_dir, extends, Edge::Extends, &mut diags, at) {
        relax(canon, 1, false, &mut dist, &mut dq);
    }

    while let Some((d, canon)) = dq.pop_front() {
        // Skip a stale entry (a shallower depth was finalized after this push) or
        // a file already processed at its minimum depth.
        if let Some(&best) = dist.get(&canon) {
            if best < d {
                continue;
            }
        }
        if parsed.contains_key(&canon) {
            continue;
        }
        let (doc, uses_refs, extends_refs) = read_and_parse(&canon, &mut diags, at);
        let dir = canon
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let mut out = Vec::new();
        for c in resolve_edges(&dir, &uses_refs, Edge::Uses, &mut diags, at) {
            relax(c.clone(), d, true, &mut dist, &mut dq);
            out.push((c, Edge::Uses));
        }
        for c in resolve_edges(&dir, &extends_refs, Edge::Extends, &mut diags, at) {
            relax(c.clone(), d + 1, false, &mut dist, &mut dq);
            out.push((c, Edge::Extends));
        }
        adj.insert(canon.clone(), out);
        parsed.insert(canon, doc);
    }

    // Directed-cycle detection over the reachable subgraph (DFS 3-coloring).
    detect_cycles(&adj, &mut diags, at);

    // --- Phase 2: gather EVERY declaration per NAME, then resolve deterministically.
    let mut state_by_name: BTreeMap<String, Vec<(PathBuf, usize, StateDecl)>> = BTreeMap::new();
    let mut def_by_name: BTreeMap<String, Vec<(PathBuf, usize, serde_yaml::Value)>> =
        BTreeMap::new();
    let mut domain_by_name: BTreeMap<String, Vec<(PathBuf, usize, Domain)>> = BTreeMap::new();
    for (canon, doc) in &parsed {
        let depth = dist.get(canon).copied().unwrap_or(0);
        for (path, decl) in &doc.state {
            state_by_name.entry(path.clone()).or_default().push((
                canon.clone(),
                depth,
                decl.clone(),
            ));
        }
        for (name, v) in &doc.defs {
            def_by_name
                .entry(name.clone())
                .or_default()
                .push((canon.clone(), depth, v.clone()));
        }
        for (name, dom) in &doc.domains {
            domain_by_name
                .entry(name.clone())
                .or_default()
                .push((canon.clone(), depth, dom.clone()));
        }
    }

    let mut state = StateSchema::default();
    let mut state_overridable = BTreeSet::new();
    for (path, entries) in state_by_name {
        // A depth level with >= 2 distinct files is a same-level collision — a
        // `uses` peer dup or a base-base dup, ALWAYS reported (never masked by a
        // closer override, which lives at a different depth).
        emit_level_dups("E-USES-DUP-STATE", &path, &entries, &mut diags, at);
        let Some((winner, winner_depth)) = pick_winner(&entries) else {
            continue;
        };
        // A deeper (overridden) base may refine the default but not the persisted
        // TYPE: flag every deeper decl whose type differs from the winner's.
        for (_, depth, decl) in &entries {
            if *depth > winner_depth && decl.ty != winner.ty {
                diags.push(uses_diag(
                    "E-EXTENDS-STATE-TYPE",
                    format!(
                        "state path `{path}` overrides base declared type {:?} with {:?}; persisted state must keep a stable type",
                        decl.ty, winner.ty
                    ),
                    at,
                ));
            }
        }
        if winner_depth >= 1 {
            state_overridable.insert(path.clone());
        }
        state.decls.insert(path, winner);
    }

    let mut defs = BTreeMap::new();
    for (name, entries) in def_by_name {
        emit_level_dups("E-USES-DUP-DEF", &name, &entries, &mut diags, at);
        if let Some((winner, _)) = pick_winner(&entries) {
            defs.insert(name, winner);
        }
    }

    // Project-authored `enums:`/`entities:` domains (data-catalog foundation
    // A3): gathered per name like state/defs above, but resolved like the
    // quest-id fold below — NOT depth-scoped (no `extends`-base "may add
    // members" relaxation yet, see `SchemaImports::domains`'s doc comment):
    // ANY name declared by >= 2 DISTINCT reachable files collides, reusing
    // A2's `E-DOMAIN-DUP` code (not a new `E-USES-DUP-*` one — the
    // data-catalog foundation design treats every domain-name collision,
    // plugin/plugin, plugin/project, or project/project, identically). The
    // winner is still the MIN-depth decl (byte-sorted-first file breaks a
    // tie), via the SAME `pick_winner` state/defs use, so resolution stays
    // deterministic even though the dup report itself is depth-agnostic.
    let mut domains = BTreeMap::new();
    for (name, entries) in domain_by_name {
        let mut files: Vec<&PathBuf> = entries.iter().map(|(f, _, _)| f).collect();
        files.sort();
        files.dedup();
        if files.len() >= 2 {
            diags.push(uses_diag(
                "E-DOMAIN-DUP",
                format!(
                    "domain `{name}` is declared by more than one schema import (`{}` and `{}`); a domain name must be declared by exactly one source",
                    files[0].display(),
                    files[1].display()
                ),
                at,
            ));
        }
        if let Some((winner, _)) = pick_winner(&entries) {
            domains.insert(name, winner);
        }
    }

    // Every `<quest id>` reachable via the import graph (dsl 0.2.0 §6.3): unlike
    // `state`/`defs` above, quest-id uniqueness is NOT depth-scoped (no
    // `extends` "closer override wins" relaxation applies — a quest id is a
    // flat, global identity, §6.3) — ANY id declared by >= 2 DISTINCT reachable
    // files collides, regardless of their depths.
    let mut quest_by_name: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    for (canon, doc) in &parsed {
        for id in &doc.quest_ids {
            quest_by_name.entry(id.clone()).or_default().push(canon.clone());
        }
    }
    let mut imported_quest_ids: BTreeMap<String, PathBuf> = BTreeMap::new();
    for (id, mut files) in quest_by_name {
        files.sort();
        files.dedup();
        if files.len() >= 2 {
            diags.push(uses_diag(
                "E-QUEST-ID-DUP",
                format!(
                    "duplicate `<quest id=\"{id}\">` across imports (`{}` and `{}`); quest \
                     ids must be unique project-wide (dsl 0.2.0 §6.3)",
                    files[0].display(),
                    files[1].display()
                ),
                at,
            ));
        }
        imported_quest_ids.insert(id, files[0].clone());
    }

    SchemaImports {
        state,
        defs,
        domains,
        diags,
        state_overridable,
        imported_quest_ids,
    }
}

/// Union a resolved schema import's project-declared domains (data-catalog
/// foundation A3, [`SchemaImports::domains`]) with the plugin/core baseline
/// already on `snapshot` ([`CapabilitySnapshot::domains`], A2) — the ACTUAL
/// merged domain vocabulary a later checker task (A4) resolves
/// `Type::Domain(name)` against, mirroring how `check.rs::fold_env` unions
/// `input.snapshot.defs` with `input.imports.defs`.
///
/// A name declared on BOTH sides — a plugin/project clash — is reported via
/// the SAME `E-DOMAIN-DUP` code `assemble.rs`'s `merge_map` uses for a
/// cross-plugin collision (data-catalog foundation design: "a plugin/project
/// name clash is an error, never a silent shadow"); the plugin/core entry
/// wins (first owner wins, matching `merge_map`'s drop-and-report
/// semantics) and the project entry is dropped, not merged/overridden. Pure
/// and total; never panics.
pub fn merge_domains(
    snapshot: &CapabilitySnapshot,
    imports: &SchemaImports,
    at: Span,
) -> (BTreeMap<String, Domain>, Vec<Diagnostic>) {
    let mut merged = snapshot.domains.clone();
    let mut diags = Vec::new();
    for (name, dom) in &imports.domains {
        if merged.contains_key(name) {
            diags.push(uses_diag(
                "E-DOMAIN-DUP",
                format!(
                    "domain `{name}` is declared by a project schema but already exists in \
                     the plugin/core vocabulary; a domain name must be declared by exactly \
                     one source"
                ),
                at,
            ));
            continue;
        }
        merged.insert(name.clone(), dom.clone());
    }
    (merged, diags)
}

/// Relax an edge in the 0-1 BFS: record `canon` at `depth` (and enqueue it) when
/// that is strictly shallower than any depth seen so far. `weight_zero` picks the
/// deque end (`uses` = front, `extends` = back).
fn relax(
    canon: PathBuf,
    depth: usize,
    weight_zero: bool,
    dist: &mut BTreeMap<PathBuf, usize>,
    dq: &mut VecDeque<(usize, PathBuf)>,
) {
    let better = match dist.get(&canon) {
        Some(&d) => depth < d,
        None => true,
    };
    if better {
        dist.insert(canon.clone(), depth);
        if weight_zero {
            dq.push_front((depth, canon));
        } else {
            dq.push_back((depth, canon));
        }
    }
}

/// Canonicalize each relative ref against `dir`; a missing target is
/// `E-USES-NOT-FOUND` (canonicalize does I/O, so a bad path lands here, never a
/// panic). Returns the successfully-resolved canonical paths.
fn resolve_edges(
    dir: &Path,
    refs: &[String],
    edge: Edge,
    diags: &mut Vec<Diagnostic>,
    at: Span,
) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for r in refs {
        match std::fs::canonicalize(dir.join(r)) {
            Ok(c) => out.push(c),
            Err(_) => diags.push(uses_diag(
                "E-USES-NOT-FOUND",
                format!(
                    "cannot resolve `{}:` import `{r}` (from {})",
                    edge.label(),
                    dir.display()
                ),
                at,
            )),
        }
    }
    out
}

/// Read + parse one canonical import, reporting `E-USES-NOT-FOUND` on an I/O
/// failure and `E-USES-PARSE` on any parse/frontmatter error. Returns the doc's
/// declared state/defs plus its own `uses`/`extends` refs (for further traversal).
fn read_and_parse(
    canon: &Path,
    diags: &mut Vec<Diagnostic>,
    at: Span,
) -> (ParsedDoc, Vec<String>, Vec<String>) {
    let empty = ParsedDoc {
        state: BTreeMap::new(),
        defs: BTreeMap::new(),
        domains: BTreeMap::new(),
        quest_ids: BTreeSet::new(),
    };
    let text = match std::fs::read_to_string(canon) {
        Ok(t) => t,
        Err(e) => {
            diags.push(uses_diag(
                "E-USES-NOT-FOUND",
                format!("cannot read schema import `{}`: {e}", canon.display()),
                at,
            ));
            return (empty, Vec::new(), Vec::new());
        }
    };
    let (doc, pdiags) = lute_syntax::parse(&text);
    let (tm, mdiags) = parse_meta_kind(&doc.meta, &CapabilitySnapshot::default(), MetaKind::Schema);
    let issues = pdiags.len() + mdiags.len();
    if issues > 0 {
        diags.push(uses_diag(
            "E-USES-PARSE",
            format!(
                "schema import `{}` has parse/frontmatter errors ({issues} issue(s))",
                canon.display()
            ),
            at,
        ));
    }
    let state = tm.state.decls;
    let defs = tm.defs;
    let domains = tm.domains;
    let uses = tm.uses;
    let extends = tm.extends;
    // `doc.quests` comes from the syntax-level parse above (kind-agnostic, Plan
    // A) — independent of `MetaKind::Schema`'s frontmatter-only extraction, so
    // a `<quest>` reachable through `uses`/`extends` is seen here even though
    // this traversal never resolves the imported doc's OWN `kind:`.
    let quest_ids: BTreeSet<String> = doc
        .quests
        .iter()
        .map(|q| q.id.clone())
        .filter(|id| !id.is_empty())
        .collect();
    (
        ParsedDoc {
            state,
            defs,
            domains,
            quest_ids,
        },
        uses,
        extends,
    )
}

/// Report `E-USES-DUP-*` for every depth level at which >= 2 DISTINCT files
/// declare `name`. Deterministic: levels ascend, and the two named files are the
/// byte-sorted-first pair.
fn emit_level_dups<T>(
    code: &str,
    name: &str,
    entries: &[(PathBuf, usize, T)],
    diags: &mut Vec<Diagnostic>,
    at: Span,
) {
    let noun = if code.ends_with("STATE") {
        "state path"
    } else {
        "def"
    };
    let mut by_depth: BTreeMap<usize, Vec<&PathBuf>> = BTreeMap::new();
    for (file, depth, _) in entries {
        by_depth.entry(*depth).or_default().push(file);
    }
    for (_depth, mut files) in by_depth {
        files.sort();
        files.dedup();
        if files.len() >= 2 {
            diags.push(uses_diag(
                code,
                format!(
                    "{noun} `{name}` is declared by two imports (`{}` and `{}`)",
                    files[0].display(),
                    files[1].display()
                ),
                at,
            ));
        }
    }
}

/// The winning declaration for a name: the MIN-depth decl, breaking a tie (a
/// same-min-depth dup, already reported) by the byte-sorted-first file for a
/// stable, order-independent result. `None` only for an (impossible) empty group.
fn pick_winner<T: Clone>(entries: &[(PathBuf, usize, T)]) -> Option<(T, usize)> {
    entries
        .iter()
        .min_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)))
        .map(|w| (w.2.clone(), w.1))
}

/// Detect any directed cycle in the reachable import subgraph and report it as
/// `E-USES-CYCLE`. Standard DFS 3-coloring: a `gray` (on-stack) target is a back
/// edge. Roots and neighbors are visited in sorted order for a deterministic,
/// order-independent result.
fn detect_cycles(
    adj: &BTreeMap<PathBuf, Vec<(PathBuf, Edge)>>,
    diags: &mut Vec<Diagnostic>,
    at: Span,
) {
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
    adj: &BTreeMap<PathBuf, Vec<(PathBuf, Edge)>>,
    on_stack: &mut BTreeSet<PathBuf>,
    done: &mut BTreeSet<PathBuf>,
    stack: &mut Vec<PathBuf>,
    diags: &mut Vec<Diagnostic>,
    at: Span,
) {
    on_stack.insert(node.to_path_buf());
    stack.push(node.to_path_buf());
    if let Some(edges) = adj.get(node) {
        let mut targets: Vec<&(PathBuf, Edge)> = edges.iter().collect();
        targets.sort_by(|a, b| a.0.cmp(&b.0));
        for (nbr, edge) in targets {
            if on_stack.contains(nbr) {
                // Back edge -> cycle: report the chain from `nbr` around to `node`.
                let start_idx = stack.iter().position(|p| p == nbr).unwrap_or(0);
                let chain = stack[start_idx..]
                    .iter()
                    .chain(std::iter::once(nbr))
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(" -> ");
                diags.push(uses_diag(
                    "E-USES-CYCLE",
                    format!("`{}:` import cycle: {chain}", edge.label()),
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
