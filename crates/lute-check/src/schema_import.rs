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
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use lute_core_span::{Diagnostic, Layer, RelatedDiagnostic, Severity, Span};
use lute_manifest::relations::{
    kinds_to_domains, EntityKindDecl, KindShape, ParsedKinds, ParsedRelations, RelationDecl,
};
use lute_manifest::snapshot::{CapabilitySnapshot, Domain};
use lute_syntax::ast::Meta;

use crate::meta::{
    parse_meta_kind, FactDecl, MetaKind, RuleDecl, StateDecl, StateSchema, TypedMeta,
};

/// An import-reachable schema's `terminal:` ([`crate::gates::TerminalDecl`]
/// as its importers see it).
#[derive(Clone, Debug)]
pub struct ImportedTerminal {
    /// The schema file (canonical).
    pub file: PathBuf,
    /// The condition, raw.
    pub when: String,
    /// Its value's positioned span in [`Self::file`].
    pub span: Span,
    /// Its `persists` (`Yes` at the value's positioned span in [`Self::file`]).
    pub persists: crate::gates::Persists,
}

/// The resolved result of a scene's composition imports (dsl §9.2): the merged
/// imported state schema, the merged imported `defs` (untyped YAML values, like
/// inline defs), the resolution diagnostics, and the state paths the importing
/// scene may inline-refine.
#[derive(Clone, Debug, Default)]
pub struct SchemaImports {
    pub state: StateSchema,
    pub defs: BTreeMap<String, serde_yaml::Value>,
    /// def name -> the file whose declaration won (the resolved `defs` entry's
    /// origin), for a diagnostic about an imported def that is raised in the
    /// IMPORTER — `E-DEF-DECL` when its type cannot be inferred there
    /// (dsl 0.21.0 §7b).
    pub def_origins: BTreeMap<String, PathBuf>,
    /// Project-authored `enums:`/`entities:` domains, PROJECTED from the
    /// depth-resolved [`RelImports::kinds`]/[`RelImports::enums`] (below) via
    /// `kinds_to_domains` plus a closed `Domain` per resolved enum — so the
    /// 0.2.2 attr layer (`Type::Domain` resolution) sees the identical merged
    /// shape it always has. Cross-source collisions and `extends`-base growth
    /// are now resolved per-namespace on [`Self::rel`] (a `uses`-peer
    /// `entities:` dup is `E-KIND-NAME-CLASH`, a peer `relations:`/`enums:`
    /// dup is `E-USES-DUP-RELATION`, a non-superset/mismatched `extends`
    /// re-declaration is `E-EXTENDS-RELATION-SIG` — decisions D2/D5); this
    /// projection carries no dup diagnostics of its own. [`merge_domains`]
    /// unions it with the plugin/core baseline (`CapabilitySnapshot.domains`)
    /// — THAT is the actual merged vocabulary the checker consults for
    /// `Type::Domain` resolution.
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
    /// Every `<entry id>` reachable via the import graph (dsl 0.19.0 §3: entry
    /// ids are unique across the project) — the lore mirror of
    /// [`Self::imported_quest_ids`]: a collision between two import-reachable
    /// docs is `E-ENTRY-ID-DUP` here, and the importing document's own entry
    /// fold seeds its seen set from these keys.
    pub imported_entry_ids: BTreeMap<String, PathBuf>,
    /// dsl 0.23.0 §7: every `cast:` member declared by an import-reachable
    /// schema, keyed by speaker id (a union — the same id in two schemas
    /// keeps the byte-sorted-first file's entry).
    pub cast: BTreeMap<String, lute_manifest::schema::CastMember>,
    /// dsl 0.24.0 §1: every `clock:` an import-reachable schema declares,
    /// by file (canonical path order), with the positioned span of its
    /// `clock:` key there — where a problem with it is reported. A project
    /// declares at most one.
    pub clock: Vec<(PathBuf, lute_manifest::clock::ClockDecl, Span)>,
    /// dsl 0.27.0 §4: every `terminal:` an import-reachable schema declares,
    /// by file (canonical path order). The game is over when any of them
    /// holds.
    pub terminal: Vec<ImportedTerminal>,
    /// dsl 0.27.0 §5: every `seasons:` an import-reachable schema declares,
    /// by file (canonical path order), with the positioned span of its
    /// `seasons:` key there.
    pub seasons: Vec<(PathBuf, crate::season::Seasons, Span)>,
    /// Where each season of [`Self::seasons`] writes its condition, by file
    /// and season name: the positioned span of its `live:` key (of its name,
    /// in the short form) — where a problem with the condition is reported.
    pub season_lives: BTreeMap<PathBuf, BTreeMap<String, Span>>,
    /// dsl 0.27.0 §4: where the installed plugins declare what a document's
    /// check judges but cannot place — filled by the CLI, which knows the
    /// project's plugins directory; empty on every other surface (a fault
    /// is then reported at its use, as before).
    pub plugin_origins: crate::rel_schema::PluginOrigins,
    pub rel: RelImports,
}

/// Relational vocabulary gathered across the uses/extends DAG (spec §4.1),
/// resolved with the SAME depth-aware machinery as `state`/`defs` above:
/// `kinds`/`relations`/`enums` are keyed by name from the MIN-depth
/// (shallowest) declaring file; a same-depth peer dup is `E-KIND-NAME-CLASH`
/// (kinds) or `E-USES-DUP-RELATION` (relations, `enums:`); a deeper
/// (`extends`-base) re-declaration that isn't a legal refinement is
/// `E-EXTENDS-RELATION-SIG` (decision D5). `facts`/`rules` always UNION,
/// never dup-checked (spec §4.1).
#[derive(Clone, Debug, Default)]
pub struct RelImports {
    pub kinds: BTreeMap<String, EntityKindDecl>,
    /// The names of [`Self::kinds`] in declaration order: file by file
    /// (shallowest import first, then path), each file's `entities:` top to
    /// bottom — the order a union kind lists its sub-kinds' members in.
    pub kind_order: Vec<String>,
    pub relations: BTreeMap<String, RelationDecl>,
    /// Project `enums:` per name (kept distinct from `domains` so relation-arg
    /// resolution can distinguish enum vs kind vs plugin domain).
    pub enums: BTreeMap<String, Vec<String>>,
    /// Seed facts, deterministic order: (depth, file, list index).
    pub facts: Vec<FactDecl>,
    /// Rules, same deterministic order. ALWAYS union (spec §4.1).
    pub rules: Vec<RuleDecl>,
    /// dsl 0.24.0 §3: entity-indexed state families (`run.approval` →
    /// `companion`) across every reachable schema — a union.
    pub indexed_state: BTreeMap<String, String>,
    /// dsl 0.24 T3-6: each imported relation / kind / def / rule / fact's
    /// declaring file and span there (the winning file for a resolved name).
    pub origins: crate::rel_schema::DeclOrigins,
    /// dsl 0.24 T3-6: heads of imported `rules:` entries that failed to parse.
    pub unparsed_heads: BTreeSet<String>,
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
/// state paths, defs, project-authored domains, entity-kind/relation decls,
/// seed facts/rules, and `<quest id>`s (the doc's own edges are consumed
/// during traversal).
struct ParsedDoc {
    state: BTreeMap<String, StateDecl>,
    /// dsl 0.28.0 (T3-1): the state paths whose own row this doc reported
    /// ([`StateSchema::faulty`]).
    faulty: BTreeSet<String>,
    /// T3-1: this doc wrote a `clock:` that was rejected.
    clock_rejected: bool,
    defs: BTreeMap<String, serde_yaml::Value>,
    /// Project `enums:`/`entities:` domains, ALREADY fused the same way
    /// `TypedMeta::domains` fuses them (entities win a same-doc name
    /// clash). Phase 2 (`resolve_imports`) recovers this doc's PURE
    /// `enums:` names by subtracting `rel_kinds.kinds`'s keys — an
    /// entity-kind name is never a project enum in the same doc, since
    /// entities always win.
    domains: BTreeMap<String, Domain>,
    /// Every non-empty `<quest id>` this doc declares (dsl 0.2.0 §6.3
    /// project-wide uniqueness); an id-less `<quest>` is that doc's OWN
    /// malformed-id problem (`E-QUEST-ID-MISSING`, reported when THAT doc is
    /// directly checked), not something this traversal can meaningfully
    /// collide on.
    quest_ids: BTreeSet<String>,
    /// Every non-empty `<entry id>` this doc declares (dsl 0.19.0 §3), the
    /// lore mirror of `quest_ids`.
    entry_ids: BTreeSet<String>,
    /// Project-authored `entities:`/`relations:` decls (0.3.0 spec §3.1/§4).
    rel_kinds: ParsedKinds,
    rel_relations: ParsedRelations,
    /// Seed `facts:`/`rules:` (0.3.0 spec §4/§7.1), in this doc's own order.
    facts: Vec<FactDecl>,
    rules: Vec<RuleDecl>,
    /// dsl 0.24.0 §3: this doc's `per:` state families.
    state_index: BTreeMap<String, String>,
    /// dsl 0.28.0: this doc's `per:` families over a kind it does not
    /// declare, each with its key's place in this file — expanded once every
    /// reachable schema's kinds are merged.
    per_pending: Vec<(crate::meta::PendingPer, crate::rel_schema::DeclOrigin)>,
    /// dsl 0.23.0 §7: this schema's `cast:` members.
    cast: Vec<lute_manifest::schema::CastMember>,
    /// dsl 0.24.0 §1: this schema's `clock:`.
    /// The `clock:` and its key's positioned span in this file.
    clock: Option<(lute_manifest::clock::ClockDecl, Span)>,
    /// dsl 0.27.0 §4: this schema's `terminal:` condition, its value's
    /// positioned span in this file, and whether it `persists`.
    terminal: Option<(String, Span, crate::gates::Persists)>,
    /// dsl 0.27.0 §5: this schema's `seasons:` (when it declares any), its
    /// key's positioned span in this file, and each season's `live:` key's.
    seasons: Option<(crate::season::Seasons, Span, BTreeMap<String, Span>)>,
    /// dsl 0.24 T3-6: this doc's declarations' spans, positioned in its text.
    origins: crate::rel_schema::DeclOrigins,
    /// dsl 0.24 T3-6: heads of this doc's `rules:` entries that failed to parse.
    failed_heads: BTreeSet<String>,
    /// dsl 0.26.0 §2.3: this doc's `entities:` `add:` lists, located here.
    kind_adds: Vec<crate::rel_schema::KindAdd>,
}

fn uses_diag(code: &str, message: String, at: Span) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity: Severity::Error,
        message,
        evidence: None,
        span: at,
        layer: Layer::Content,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

/// The importer-span placeholder [`ImportCache`] resolves with: no real span
/// has `usize::MAX` offsets, so every diagnostic span equal to it is exactly
/// one `resolve_imports` anchored at its `at` argument.
const AT_PLACEHOLDER: Span = Span {
    byte_start: usize::MAX,
    byte_end: usize::MAX,
    line: u32::MAX,
    column: u32::MAX,
    utf16_range: (u32::MAX, u32::MAX),
};

/// Per-run memo of [`resolve_imports`] and
/// [`crate::component_import::resolve_components`]: every document of a
/// project that names the same `uses:`/`extends:` (or `components:`) lists
/// from the same directory resolves the same import DAG, so a batch caller (a
/// project check over thousands of scenes) reads and parses each imported file
/// once instead of once per document.
///
/// Each result depends on the importing document only through `at`, which is
/// copied verbatim into diagnostic spans; the memo resolves once against
/// [`AT_PLACEHOLDER`] and rewrites those spans to each caller's `at`, so both
/// methods return exactly what the uncached resolvers would. Holds no
/// invalidation: the files are assumed not to change during one run.
#[derive(Default)]
pub struct ImportCache {
    imports: Memo<(PathBuf, Vec<String>, Vec<String>), SchemaImports>,
    components: Memo<(PathBuf, Vec<String>), crate::ComponentSet>,
}

impl ImportCache {
    /// [`resolve_imports`], memoized on `(base_dir, uses, extends)`.
    pub fn resolve(
        &self,
        base_dir: &Path,
        uses: &[String],
        extends: &[String],
        at: Span,
    ) -> SchemaImports {
        let key = (base_dir.to_path_buf(), uses.to_vec(), extends.to_vec());
        let mut out = self.imports.get_or_init(key, || {
            resolve_imports(base_dir, uses, extends, AT_PLACEHOLDER)
        });
        anchor_at(&mut out.diags, at);
        out
    }

    /// [`crate::component_import::resolve_components`], memoized on
    /// `(base_dir, components)`.
    pub fn resolve_components(
        &self,
        base_dir: &Path,
        components: &[String],
        at: Span,
    ) -> crate::ComponentSet {
        let key = (base_dir.to_path_buf(), components.to_vec());
        let mut out = self.components.get_or_init(key, || {
            crate::component_import::resolve_components(base_dir, components, AT_PLACEHOLDER)
        });
        anchor_at(&mut out.diags, at);
        out
    }
}

/// A thread-safe compute-once map: the first caller for a key runs `init`,
/// concurrent callers for the same key block on it, later callers read it.
pub struct Memo<K, V> {
    map: Mutex<HashMap<K, Arc<OnceLock<V>>>>,
}

impl<K, V> Default for Memo<K, V> {
    fn default() -> Self {
        Memo {
            map: Mutex::new(HashMap::new()),
        }
    }
}

impl<K: Eq + std::hash::Hash, V: Clone> Memo<K, V> {
    /// A clone of the value for `key`, computing it with `init` on first use.
    /// The map lock is held only to find the key's cell, never while `init`
    /// runs, so distinct keys compute concurrently.
    pub fn get_or_init(&self, key: K, init: impl FnOnce() -> V) -> V {
        let cell = {
            let mut map = self.map.lock().unwrap_or_else(|e| e.into_inner());
            map.entry(key).or_default().clone()
        };
        cell.get_or_init(init).clone()
    }
}

/// Replace every [`AT_PLACEHOLDER`] span (outer or `related`) with `at`.
fn anchor_at(diags: &mut [Diagnostic], at: Span) {
    for d in diags {
        if d.span == AT_PLACEHOLDER {
            d.span = at;
        }
        for r in &mut d.related {
            anchor_at(std::slice::from_mut(&mut r.diagnostic), at);
        }
    }
}

/// Resolve a document's composition imports (dsl §9.2) into a merged schema.
/// `base_dir` is the importing document's directory; each `uses`/`extends` entry
/// is a relative path. `at` is the importing document's frontmatter span, used
/// for every diagnostic. TOTAL: any I/O/parse/cycle/dup failure yields a
/// diagnostic, never a panic; the merged schema is INDEPENDENT of the order
/// of the `uses`/`extends` entries (a same-level duplicate is reported at the
/// later import in that order).
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
    let mut roots = Vec::new();
    for canon in resolve_edges(base_dir, uses, Edge::Uses, &mut diags, at) {
        relax(canon.clone(), 0, true, &mut dist, &mut dq);
        roots.push(canon);
    }
    for canon in resolve_edges(base_dir, extends, Edge::Extends, &mut diags, at) {
        relax(canon.clone(), 1, false, &mut dist, &mut dq);
        roots.push(canon);
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
    let order = import_order(&roots, &adj);

    // 0.3.0 T7: structural relation-decl validation (`E-ENTITY-KIND-SHAPE`,
    // `E-KIND-NAME-CLASH`/`E-RELATION-DUP` same-block dups,
    // `E-RELATION-EMPTY`/`-DOMAIN`, `E-DERIVE-TIER`,
    // `E-RELATION-RESERVED-WRITE`, `E-RESERVED-NAME`) runs per
    // IMPORTED file too — so a malformed decl surfaces at every document that
    // imports it, not only when that file is checked directly — reported at
    // the declaration's own line in that file (dsl 0.24 T3-6); the project
    // roll-up folds the importers' identical copies into one.
    for (canon, doc) in &parsed {
        // A name only an `add:` writes has no `kinds` origin; the `add:`
        // kind's own key is its home.
        let span_of = |name: &str| {
            doc.origins
                .relations
                .get(name)
                .or_else(|| doc.origins.kinds.get(name))
                .or_else(|| {
                    doc.kind_adds
                        .iter()
                        .find(|a| a.kind == name)
                        .and_then(|a| a.origin.as_ref())
                })
                .map_or(at, |o| o.span)
        };
        let key_at = |kind: &str, path: &[&str]| {
            let key = crate::rel_schema::kind_key_origin(kind, path);
            doc.origins.kind_keys.get(&key).map(|o| o.span)
        };
        let rel_key_at = |rel: &str, k: &str| {
            let key = crate::rel_schema::kind_key_origin(rel, &[k]);
            doc.origins.relation_keys.get(&key).map(|o| o.span)
        };
        for d in crate::rel_schema::validate_rel_decls(
            &doc.rel_kinds,
            &doc.rel_relations,
            &span_of,
            &key_at,
            &rel_key_at,
        ) {
            let origin = crate::rel_schema::DeclOrigin {
                file: canon.clone(),
                span: d.span,
            };
            diags.push(crate::rel_schema::at_origin(d, Some(&origin)));
        }
    }

    // --- Phase 2: gather EVERY declaration per NAME, then resolve deterministically.
    let mut state_by_name: BTreeMap<String, Vec<(PathBuf, usize, StateDecl)>> = BTreeMap::new();
    let mut def_by_name: BTreeMap<String, Vec<(PathBuf, usize, serde_yaml::Value)>> =
        BTreeMap::new();
    let mut kind_by_name: BTreeMap<String, Vec<(PathBuf, usize, EntityKindDecl)>> = BTreeMap::new();
    let mut relation_by_name: BTreeMap<String, Vec<(PathBuf, usize, RelationDecl)>> =
        BTreeMap::new();
    let mut enum_by_name: BTreeMap<String, Vec<(PathBuf, usize, Domain)>> = BTreeMap::new();
    let mut fact_entries: Vec<(usize, PathBuf, usize, FactDecl)> = Vec::new();
    let mut rule_entries: Vec<(usize, PathBuf, usize, RuleDecl)> = Vec::new();
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
        for (name, decl) in &doc.rel_kinds.kinds {
            kind_by_name.entry(name.clone()).or_default().push((
                canon.clone(),
                depth,
                decl.clone(),
            ));
        }
        for (name, decl) in &doc.rel_relations.relations {
            relation_by_name.entry(name.clone()).or_default().push((
                canon.clone(),
                depth,
                decl.clone(),
            ));
        }
        // A pure `enums:` name never collides with this SAME doc's `entities:`
        // name in `doc.domains` (entities always win a same-doc clash, see
        // `ParsedDoc::domains`'s doc comment), so subtracting `rel_kinds.kinds`
        // recovers exactly this doc's project `enums:` names.
        for (name, dom) in &doc.domains {
            if !doc.rel_kinds.kinds.contains_key(name) {
                enum_by_name.entry(name.clone()).or_default().push((
                    canon.clone(),
                    depth,
                    dom.clone(),
                ));
            }
        }
        for (i, fact) in doc.facts.iter().enumerate() {
            fact_entries.push((depth, canon.clone(), i, fact.clone()));
        }
        for (i, rule) in doc.rules.iter().enumerate() {
            rule_entries.push((depth, canon.clone(), i, rule.clone()));
        }
    }

    let mut state = StateSchema::default();
    let mut state_overridable = BTreeSet::new();
    // State paths two same-level imports declare differently: which type
    // the documents meant is unknown, so what follows from it is not
    // reported beside the duplicate.
    let mut clashing = BTreeSet::new();
    for (path, entries) in state_by_name {
        // A depth level with >= 2 distinct files is a same-level collision — a
        // `uses` peer dup or a base-base dup, ALWAYS reported (never masked by a
        // closer override, which lives at a different depth).
        let levels = emit_level_dups(
            "E-USES-DUP-STATE",
            "state path",
            &path,
            &entries,
            &|f| {
                parsed
                    .get(f)
                    .and_then(|d| d.origins.state.get(&path))
                    .cloned()
            },
            &order,
            &mut diags,
            at,
        );
        if levels.iter().any(|level| {
            let mut decls = entries.iter().filter(|(_, d, _)| d == level);
            let first = decls.next().map(|(_, _, s)| s);
            decls.any(|(_, _, s)| Some(s) != first)
        }) {
            clashing.insert(path.clone());
        }
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
    state.faulty = parsed
        .values()
        .flat_map(|d| d.faulty.iter().cloned())
        .collect();
    state.faulty.extend(clashing);
    state.clock_rejected = parsed.values().any(|d| d.clock_rejected);

    let mut defs = BTreeMap::new();
    let mut def_origins = BTreeMap::new();
    for (name, entries) in def_by_name {
        emit_level_dups(
            "E-USES-DUP-DEF",
            "def",
            &name,
            &entries,
            &|f| {
                parsed
                    .get(f)
                    .and_then(|d| d.origins.defs.get(&name))
                    .cloned()
            },
            &order,
            &mut diags,
            at,
        );
        if let Some((winner, winner_depth)) = pick_winner(&entries) {
            // `pick_winner`'s own tie-break: the byte-least path at the depth.
            if let Some(file) = entries
                .iter()
                .filter(|(_, d, _)| *d == winner_depth)
                .map(|(p, _, _)| p)
                .min()
            {
                def_origins.insert(name.clone(), file.clone());
            }
            defs.insert(name, winner);
        }
    }

    let mut rel_kinds: BTreeMap<String, EntityKindDecl> = BTreeMap::new();
    for (name, entries) in kind_by_name {
        // A depth level with >= 2 distinct files declaring the same entity-kind
        // NAME is a peer clash (spec §4/§4.1, decision D2) — never masked by a
        // closer `extends` override, which lives at a different depth.
        emit_level_dups(
            "E-KIND-NAME-CLASH",
            "entity kind",
            &name,
            &entries,
            &|f| {
                parsed
                    .get(f)
                    .and_then(|d| d.origins.kinds.get(&name))
                    .cloned()
            },
            &order,
            &mut diags,
            at,
        );
        let Some((winner, winner_depth)) = pick_winner(&entries) else {
            continue;
        };
        // A deeper (`extends`-base) re-declaration must be a SUPERSET
        // re-listing of the base's members, same shape (decision D5); a
        // missing base member or a `members:`/`open:` shape flip is
        // `E-EXTENDS-RELATION-SIG`. The merged entry is the child's (winner's).
        for (_, depth, decl) in &entries {
            if *depth > winner_depth {
                if let Some(msg) = kind_shape_mismatch(&winner.shape, &decl.shape) {
                    diags.push(uses_diag(
                        "E-EXTENDS-RELATION-SIG",
                        format!(
                            "entity kind `{name}` {msg}; an `extends` child must re-declare a superset of the base's members (dsl 0.3.0 §4.1)"
                        ),
                        at,
                    ));
                }
            }
        }
        rel_kinds.insert(name, winner);
    }
    // dsl 0.26.0 §2.3: every imported `add:` extends the one imported
    // declaration of its kind (file order, so the report is deterministic).
    let kind_adds: Vec<crate::rel_schema::KindAdd> = parsed
        .values()
        .flat_map(|d| d.kind_adds.iter().cloned())
        .collect();
    diags.extend(crate::rel_schema::apply_kind_adds(
        &mut rel_kinds,
        &kind_adds,
        &|kind| {
            parsed
                .iter()
                .filter(|(_, d)| d.rel_kinds.kinds.contains_key(kind))
                .min_by_key(|(p, _)| (dist.get(*p).copied().unwrap_or(0), *p))
                .and_then(|(_, d)| crate::rel_schema::kind_home(&d.origins, kind))
        },
    ));

    let mut rel_relations: BTreeMap<String, RelationDecl> = BTreeMap::new();
    for (name, entries) in relation_by_name {
        emit_level_dups(
            "E-USES-DUP-RELATION",
            "relation",
            &name,
            &entries,
            &|f| {
                parsed
                    .get(f)
                    .and_then(|d| d.origins.relations.get(&name))
                    .cloned()
            },
            &order,
            &mut diags,
            at,
        );
        let Some((winner, winner_depth)) = pick_winner(&entries) else {
            continue;
        };
        // A deeper (`extends`-base) re-declaration must match the FULL decl —
        // `args`+`tier`+`derive`+`reserved`+`key` (decision D5, a differing
        // functional key silently changes engine auto-invalidation semantics).
        for (_, depth, decl) in &entries {
            if *depth > winner_depth {
                let diff = relation_sig_diff(&winner, decl);
                if !diff.is_empty() {
                    diags.push(uses_diag(
                        "E-EXTENDS-RELATION-SIG",
                        format!(
                            "relation `{name}` re-declaration differs from its `extends` base in {}; a re-declared relation must match the full base decl (dsl 0.3.0 §4.1)",
                            diff.join(", ")
                        ),
                        at,
                    ));
                }
            }
        }
        rel_relations.insert(name, winner);
    }

    let mut rel_enums: BTreeMap<String, Domain> = BTreeMap::new();
    for (name, entries) in enum_by_name {
        emit_level_dups(
            "E-USES-DUP-RELATION",
            "enum",
            &name,
            &entries,
            &|f| {
                parsed
                    .get(f)
                    .and_then(|d| d.origins.domains.get(&name))
                    .cloned()
            },
            &order,
            &mut diags,
            at,
        );
        let Some((winner, winner_depth)) = pick_winner(&entries) else {
            continue;
        };
        for (_, depth, base_members) in &entries {
            if *depth > winner_depth {
                let missing = missing_members(&winner.members, &base_members.members);
                if !missing.is_empty() {
                    diags.push(uses_diag(
                        "E-EXTENDS-RELATION-SIG",
                        format!(
                            "enum `{name}` is missing base member(s) {missing:?}; an `extends` child must re-declare a superset of the base's members (dsl 0.3.0 §4.1)"
                        ),
                        at,
                    ));
                }
            }
        }
        rel_enums.insert(name, winner);
    }

    // Project the RESOLVED kinds/enums into the flat `Domain` shape the
    // 0.2.2 attr layer's `Type::Domain` resolution already consumes (same
    // "entities win a same-name clash" precedence `TypedMeta::domains` uses
    // per-doc: the enum projection runs first, `kinds_to_domains` overwrites).
    let mut domains: BTreeMap<String, Domain> = rel_enums
        .iter()
        .map(|(name, dom)| (name.clone(), dom.clone()))
        .collect();
    // dsl 0.26.0 §2.3: a parent's domain holds its sub-kinds' members too
    // (`rel.kinds` keeps the lists as declared; `build_rel_vocab` implies
    // them once the document's own decls are overlaid), in declaration
    // order: file by file, shallowest first, then by path.
    let mut files: Vec<(&PathBuf, &ParsedDoc)> = parsed.iter().collect();
    files.sort_by_key(|(p, _)| (dist.get(*p).copied().unwrap_or(0), *p));
    let mut kind_order: Vec<String> = Vec::new();
    for name in files.iter().flat_map(|(_, d)| &d.rel_kinds.order) {
        if rel_kinds.contains_key(name) && !kind_order.contains(name) {
            kind_order.push(name.clone());
        }
    }
    let mut implied_kinds = rel_kinds.clone();
    lute_manifest::relations::imply_sub_kind_members(&mut implied_kinds, &kind_order);
    domains.extend(kinds_to_domains(&implied_kinds));

    fact_entries.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    rule_entries.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    let facts: Vec<FactDecl> = fact_entries.into_iter().map(|(_, _, _, f)| f).collect();
    let rules: Vec<RuleDecl> = rule_entries.into_iter().map(|(_, _, _, r)| r).collect();
    let mut indexed_state: BTreeMap<String, String> = parsed
        .values()
        .flat_map(|doc| doc.state_index.iter().map(|(p, k)| (p.clone(), k.clone())))
        .collect();
    // dsl 0.28.0: a `per:` over a kind another reachable schema declares.
    for doc in parsed.values() {
        for (pending, origin) in &doc.per_pending {
            let (decls, index, faults) =
                crate::meta::expand_per_pending(std::slice::from_ref(pending), &implied_kinds);
            for (path, decl) in decls {
                state.decls.entry(path).or_insert(decl);
            }
            indexed_state.extend(index);
            for d in faults {
                diags.push(crate::rel_schema::at_origin(d, Some(origin)));
            }
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
            quest_by_name
                .entry(id.clone())
                .or_default()
                .push(canon.clone());
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

    // The lore mirror (dsl 0.19.0 §3): entry ids are a flat, project-wide
    // identity exactly as quest ids are.
    let mut entry_by_name: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    for (canon, doc) in &parsed {
        for id in &doc.entry_ids {
            entry_by_name
                .entry(id.clone())
                .or_default()
                .push(canon.clone());
        }
    }
    let mut imported_entry_ids: BTreeMap<String, PathBuf> = BTreeMap::new();
    for (id, mut files) in entry_by_name {
        files.sort();
        files.dedup();
        if files.len() >= 2 {
            diags.push(uses_diag(
                crate::lore::E_ENTRY_ID_DUP,
                format!(
                    "duplicate `<entry id=\"{id}\">` across imports (`{}` and `{}`); entry \
                     ids must be unique across the project (dsl 0.19.0 §3)",
                    files[0].display(),
                    files[1].display()
                ),
                at,
            ));
        }
        imported_entry_ids.insert(id, files[0].clone());
    }

    let mut cast: BTreeMap<String, lute_manifest::schema::CastMember> = BTreeMap::new();
    for doc in parsed.values() {
        for c in &doc.cast {
            cast.entry(c.id.clone()).or_insert_with(|| c.clone());
        }
    }
    // dsl 0.24.0 §1: every import-reachable `clock:`, path order —
    // `crate::clock::check_clock` reports more than one.
    let clock: Vec<(PathBuf, lute_manifest::clock::ClockDecl, Span)> = parsed
        .iter()
        .filter_map(|(path, doc)| doc.clock.clone().map(|(c, at)| (path.clone(), c, at)))
        .collect();
    let terminal: Vec<ImportedTerminal> = parsed
        .iter()
        .filter_map(|(path, doc)| {
            doc.terminal
                .clone()
                .map(|(when, span, persists)| ImportedTerminal {
                    file: path.clone(),
                    when,
                    span,
                    persists,
                })
        })
        .collect();
    let seasons: Vec<(PathBuf, crate::season::Seasons, Span)> = parsed
        .iter()
        .filter_map(|(path, doc)| doc.seasons.clone().map(|(s, at, _)| (path.clone(), s, at)))
        .collect();
    let season_lives = parsed
        .iter()
        .filter_map(|(path, doc)| {
            doc.seasons
                .as_ref()
                .map(|(_, _, lives)| (path.clone(), lives.clone()))
        })
        .collect();

    // dsl 0.24 T3-6: each resolved name's home — the shallowest declaring
    // file, byte-least on a tie (`pick_winner`'s own rule); rules and facts
    // union, so each keeps the first file (depth, path order) declaring it.
    let mut by_depth: Vec<(usize, &PathBuf, &ParsedDoc)> = parsed
        .iter()
        .map(|(p, d)| (dist.get(p).copied().unwrap_or(0), p, d))
        .collect();
    by_depth.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(b.1)));
    let mut origins = crate::rel_schema::DeclOrigins::default();
    let mut unparsed_heads = BTreeSet::new();
    for (_, _, doc) in &by_depth {
        for (dst, src) in [
            (&mut origins.relations, &doc.origins.relations),
            (&mut origins.kinds, &doc.origins.kinds),
            (&mut origins.defs, &doc.origins.defs),
            (&mut origins.rules, &doc.origins.rules),
            (&mut origins.facts, &doc.origins.facts),
            (&mut origins.domains, &doc.origins.domains),
            (&mut origins.state, &doc.origins.state),
            (&mut origins.members, &doc.origins.members),
            (&mut origins.cast, &doc.origins.cast),
            (&mut origins.kind_keys, &doc.origins.kind_keys),
            (&mut origins.enum_labels, &doc.origins.enum_labels),
        ] {
            for (k, v) in src {
                dst.entry(k.clone()).or_insert_with(|| v.clone());
            }
        }
        unparsed_heads.extend(doc.failed_heads.iter().cloned());
    }
    // Where each imported `add:` member is written — the first `add:` of it
    // in file order, like `apply_kind_adds` keeps.
    for add in &kind_adds {
        let Some(origin) = &add.origin else { continue };
        for (m, span) in add.members.iter().zip(&add.member_spans) {
            let Some(span) = span else { continue };
            origins
                .added
                .entry(crate::rel_schema::member_origin_key(&add.kind, m))
                .or_insert_with(|| crate::rel_schema::DeclOrigin {
                    file: origin.file.clone(),
                    span: *span,
                });
        }
    }

    SchemaImports {
        state,
        defs,
        def_origins,
        domains,
        diags,
        state_overridable,
        imported_quest_ids,
        imported_entry_ids,
        cast,
        clock,
        terminal,
        seasons,
        season_lives,
        plugin_origins: Default::default(),
        rel: RelImports {
            kinds: rel_kinds,
            kind_order,
            relations: rel_relations,
            enums: rel_enums
                .iter()
                .map(|(k, v)| (k.clone(), v.members.clone()))
                .collect(),
            facts,
            rules,
            indexed_state,
            origins,
            unparsed_heads,
        },
    }
}

/// Union the PROJECT's declared domains with the plugin/core baseline already
/// on `snapshot` ([`CapabilitySnapshot::domains`], A2) — the ACTUAL merged
/// domain vocabulary a later checker task (A4) resolves `Type::Domain(name)`
/// against, mirroring how `check.rs::fold_env` unions `input.snapshot.defs`
/// with `input.imports.defs`.
///
/// The project side has TWO sources, and both are the identical two-step
/// projection of the same YAML shape — `parse_enums`, then
/// `.extend(kinds_to_domains(..))`:
///
/// * `imports.domains` — every file reached via `uses:`/`extends:`, projected
///   by [`resolve_imports`] from the depth-resolved [`RelImports`].
/// * `inline.domains` — THIS document's own `enums:`/`entities:`, projected by
///   `parse_meta` ([`TypedMeta::domains`]). `enums:` is in `UNIVERSAL_KEYS`, so
///   an inline block is deliberately legal syntax in any document; before dsl
///   0.9.0 it parsed and was then dropped here, and `E-DOMAIN-UNKNOWN` told the
///   author to declare a domain they had already declared.
///
/// Neither is re-derived: each source hands over the projection it already
/// built, because two projections of one YAML shape drift.
///
/// Precedence, inline vs imported: the INLINE declaration wins, and a
/// non-superset re-declaration is decision D5's `E-EXTENDS-RELATION-SIG`. That
/// is not a choice made here — it is [`resolve_imports`]'s own shallowest-wins
/// rule (`pick_winner`) applied to a document that is depth 0 to any import's
/// depth >= 1, and it is exactly what `rel_schema::build_rel_vocab` already
/// does for the same names in `RelVocab::enums`/`kinds` one line below the
/// `merge_domains` call in `check.rs`. Those two maps MUST NOT disagree about
/// which member list is in force. The D5 diagnostic stays `build_rel_vocab`'s
/// alone — one owner, no duplicate — so this function only applies the
/// precedence.
///
/// A name declared on both the project and plugin/core sides — a
/// plugin/project clash, inline or imported alike — is reported via the SAME
/// `E-DOMAIN-DUP` code `assemble.rs`'s `merge_map` uses for a cross-plugin
/// collision (data-catalog foundation design: "a plugin/project name clash is
/// an error, never a silent shadow"); the plugin/core entry wins (first owner
/// wins, matching `merge_map`'s drop-and-report semantics) and the project
/// entry is dropped, not merged/overridden. `E-DOMAIN-DUP` is NOT reachable
/// for a project-project collision: decision D2 splits those into
/// `E-USES-DUP-RELATION` / `E-KIND-NAME-CLASH`, raised where the collision is
/// seen. Pure and total; never panics.
pub fn merge_domains(
    snapshot: &CapabilitySnapshot,
    imports: &SchemaImports,
    inline: &TypedMeta,
    at: Span,
) -> (BTreeMap<String, Domain>, Vec<Diagnostic>) {
    let mut merged = snapshot.domains.clone();
    let mut diags = Vec::new();
    let mut project = imports.domains.clone();
    for (name, dom) in &inline.domains {
        project.insert(name.clone(), dom.clone());
    }
    for (name, dom) in &project {
        if merged.contains_key(name) {
            diags.push(uses_diag(
                "E-DOMAIN-DUP",
                format!(
                    "domain `{name}` is declared by this project — in a document's own \
                     `enums:` frontmatter or in a project schema reached through \
                     `uses:`/`extends:` — but already exists in the plugin/core vocabulary; \
                     a domain name must be declared by exactly one source, so drop the \
                     project declaration or the plugin's `enums` export (the plugin's wins)"
                ),
                at,
            ));
            continue;
        }
        // dsl 0.9.0 D-D — ONE rule, but it needs PROVENANCE: a domain
        // occupying a semantics-bearing slot must be able to CARRY that
        // slot's semantics, and only an `enums:` decl can. Do not collapse
        // this branch back into one `validate_domain` call: each source's
        // `domains` fuses `enums:` and `entities:` (see above) and an
        // `EntityKindDecl` can express only `members` or `open` — an authored
        // `exits:`/`default:` key on it is discarded — so the shared
        // validator's generic "declare `exits:`" names a fix that cannot
        // exist for a kind-derived value, while skipping the check would let
        // an `action` slot with no exits through silently (the exact behavior
        // loss 0.9.0 removes).
        //
        // Provenance comes from the WINNING projection, not from whichever
        // map happens to contain the name: each source builds its `domains` as
        // the enum projection `.extend`ed with `kinds_to_domains`, so when one
        // name is declared under both `entities:` and `enums:`, BOTH maps
        // retain it and the KIND entry is the `Domain` in hand. Hence:
        // kind-derived (any name `kinds_to_domains` projects —
        // `KindShape::Invalid` is skipped there, so such a name reaching here
        // is the enum entry) → point the author at `enums:`; otherwise the
        // value is enum-derived and goes to the SHARED validator
        // `assemble.rs` runs on the plugin path (no duplicated rule set).
        //
        // Which source's kinds map to read follows the same precedence as the
        // value itself: an inline declaration overrode the imported one above,
        // so its provenance overrides too.
        let kinds = if inline.domains.contains_key(name) {
            &inline.rel_kinds.kinds
        } else {
            &imports.rel.kinds
        };
        let kind_derived = matches!(
            kinds.get(name).map(|decl| &decl.shape),
            Some(KindShape::Members(_) | KindShape::Open)
        );
        // An imported domain's problem is the schema's: reported at its
        // declaration there (dsl 0.24 T3-6), so the roll-up folds importers.
        let origin = (!inline.domains.contains_key(name))
            .then(|| imports.rel.origins.domains.get(name))
            .flatten();
        if !kind_derived {
            for issue in lute_manifest::validate::validate_domain(name, dom) {
                let d = uses_diag(issue.code(), issue.message(), at);
                // A label for a non-member is the label key's problem.
                let label = match (&issue, origin) {
                    (
                        lute_manifest::validate::DomainIssue::LabelNotMember { value, .. },
                        Some(_),
                    ) => imports
                        .rel
                        .origins
                        .enum_labels
                        .get(&crate::rel_schema::member_origin_key(name, value)),
                    _ => None,
                };
                diags.push(crate::rel_schema::at_origin(d, label.or(origin)));
            }
        } else if let Some(key) = missing_slot_semantics_key(name) {
            diags.push(uses_diag(
                "E-ENUM-MISSING-SEMANTICS",
                format!(
                    "domain `{name}` is declared as an `entities:` kind, which cannot \
                     express the `{key}:` member semantics this slot requires; declare \
                     `{name}` with `enums:` instead (dsl 0.9.0 D-D)"
                ),
                at,
            ));
        }
        merged.insert(name.clone(), dom.clone());
    }
    (merged, diags)
}

/// The member-semantics key a domain named `name` is REQUIRED to declare but
/// cannot, given it arrived from an `entities:` kind projection — `None` only
/// when the name is not a semantics-bearing slot (dsl 0.9.0 D-D
/// `SLOT_REQUIRES_EXITS`/`SLOT_REQUIRES_DEFAULT`).
///
/// Deliberately NOT exempting an open domain, unlike
/// [`lute_manifest::validate::validate_domain`]: that escape is right for
/// MEMBERSHIP rules (a registry-style domain has no static member list, so a
/// rule about its members is vacuous) and wrong here. A slot's semantics are
/// a compiler INPUT, not a statement about members — the compiler reads
/// `action`'s exits and `anchor`'s default — so openness cannot make the
/// requirement vacuous. It makes it unsatisfiable: an open domain cannot
/// enumerate its exits at all, so `entities: { action: { open: engine } }` is
/// not merely unvalidated, it is unworkable, and `enums:` is the only fix.
fn missing_slot_semantics_key(name: &str) -> Option<&'static str> {
    if lute_manifest::validate::SLOT_REQUIRES_EXITS.contains(&name) {
        Some("exits")
    } else if lute_manifest::validate::SLOT_REQUIRES_DEFAULT.contains(&name) {
        Some("default")
    } else {
        None
    }
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

/// Re-derive `line`/`column`/`utf16_range` from each diagnostic's byte offsets
/// against the IMPORTED file's own text.
///
/// The house convention is zero-then-normalize: producers such as
/// [`crate::meta::meta_key_span`] emit byte offsets and leave the position
/// fields at zero for `check`'s `normalize_spans` to fill in. That pass runs
/// over the IMPORTING document, and never walks `related` — so an import's
/// folded diagnostics would print at `0:0`, which is exactly as useless to the
/// author as the count they replace (#21, T3.9).
fn position_in(text: &str, diags: &mut [Diagnostic]) {
    let idx = lute_core_span::TextIndex::new(text);
    let len = text.len();
    for d in diags {
        let start = d.span.byte_start.min(len);
        let end = d.span.byte_end.min(len).max(start);
        d.span = Span::from_bytes(&idx, start, end);
    }
}

/// Read + parse one canonical import, reporting `E-USES-NOT-FOUND` on an I/O
/// failure and `E-USES-PARSE` on any parse/frontmatter error. Returns the doc's
/// declared state/defs plus its own `uses`/`extends` refs (for further traversal).
///
/// A `.yaml`/`.yml` target (data-catalog foundation B2) is a PURE declaration
/// map: no `---` envelope, no body — the whole file IS the frontmatter. It is
/// wrapped in a synthetic [`Meta`] spanning the whole file and fed through the
/// SAME [`parse_meta_kind`] lift a `.lute`/`.schema.lute`/`.component.lute`
/// target's REAL frontmatter uses, so state/defs/enums/entities merge
/// identically; the Lute body parser (`lute_syntax::parse`) is skipped
/// entirely — a bare YAML file has no shots/`<quest>`s to walk.
fn read_and_parse(
    canon: &Path,
    diags: &mut Vec<Diagnostic>,
    at: Span,
) -> (ParsedDoc, Vec<String>, Vec<String>) {
    let empty = ParsedDoc {
        state: BTreeMap::new(),
        faulty: BTreeSet::new(),
        clock_rejected: false,
        defs: BTreeMap::new(),
        domains: BTreeMap::new(),
        quest_ids: BTreeSet::new(),
        entry_ids: BTreeSet::new(),
        cast: Vec::new(),
        clock: None,
        terminal: None,
        seasons: None,
        rel_kinds: ParsedKinds::default(),
        rel_relations: ParsedRelations::default(),
        facts: Vec::new(),
        rules: Vec::new(),
        state_index: BTreeMap::new(),
        per_pending: Vec::new(),
        origins: Default::default(),
        failed_heads: BTreeSet::new(),
        kind_adds: Vec::new(),
    };
    let text = match std::fs::read_to_string(canon) {
        Ok(t) => t,
        Err(e) => {
            let e = lute_manifest::io_reason(&e);
            diags.push(uses_diag(
                "E-USES-NOT-FOUND",
                format!("cannot read schema import `{}`: {e}", canon.display()),
                at,
            ));
            return (empty, Vec::new(), Vec::new());
        }
    };
    let is_yaml_decl = matches!(
        canon.extension().and_then(|e| e.to_str()),
        Some("yaml") | Some("yml")
    );
    let (tm, issue_diags, quest_ids, entry_ids, meta) = if is_yaml_decl {
        let byte_end = text.len();
        let meta = Meta {
            raw_yaml: text.clone(),
            span: Span {
                byte_start: 0,
                byte_end,
                line: 1,
                column: 1,
                utf16_range: (0, 0),
            },
        };
        let (tm, mut mdiags) =
            parse_meta_kind(&meta, &CapabilitySnapshot::default(), MetaKind::Schema);
        position_in(&meta.raw_yaml, &mut mdiags);
        (tm, mdiags, BTreeSet::new(), BTreeSet::new(), meta)
    } else {
        let (doc, pdiags) = lute_syntax::parse(&text);
        let (tm, mdiags) =
            parse_meta_kind(&doc.meta, &CapabilitySnapshot::default(), MetaKind::Schema);
        // `doc.quests` comes from the syntax-level parse above (kind-agnostic,
        // Plan A) — independent of `MetaKind::Schema`'s frontmatter-only
        // extraction, so a `<quest>` reachable through `uses`/`extends` is seen
        // here even though this traversal never resolves the imported doc's
        // OWN `kind:`. A `.yaml` target has no body, hence no quests (above).
        let quest_ids: BTreeSet<String> = doc
            .quests
            .iter()
            .map(|q| q.id.clone())
            .filter(|id| !id.is_empty())
            .collect();
        let entry_ids: BTreeSet<String> = doc
            .entries
            .iter()
            .map(|e| e.id.clone())
            .filter(|id| !id.is_empty())
            .collect();
        let mut all = pdiags;
        all.extend(mdiags);
        position_in(&text, &mut all);
        (tm, all, quest_ids, entry_ids, doc.meta)
    };
    if !issue_diags.is_empty() {
        // dsl 0.5.0 §2.2, mirroring `component_import.rs:260-282`: carry the
        // import's OWN diagnostics (spans relative to the imported file) onto
        // the importer's `E-USES-PARSE`, so `--json` — and the human
        // renderer, which already walks `related` — surface what actually
        // failed. `(N issue(s))` on its own is a NUMBER, not the issue, and
        // it was the author's entire information (#21, T3.9). The count is
        // computed from the same vector, so the two cannot disagree.
        let file = canon.display().to_string();
        let mut d = uses_diag(
            "E-USES-PARSE",
            // The file's name, not its absolute path: the issues below
            // name it where the author's paths are relative.
            format!(
                "schema import `{}` has errors ({} issue(s))",
                canon.file_name().map_or_else(
                    || canon.display().to_string(),
                    |n| n.to_string_lossy().into_owned()
                ),
                issue_diags.len()
            ),
            at,
        );
        d.related = issue_diags
            .into_iter()
            .map(|diagnostic| RelatedDiagnostic {
                file: file.clone(),
                diagnostic,
            })
            .collect();
        diags.push(d);
    }
    // dsl 0.24 T3-6: where each declaration sits in THIS file, positioned, so
    // an importer's diagnostic about it can name the schema line.
    let idx = lute_core_span::TextIndex::new(&text);
    let here = |s: Span| {
        let end = s.byte_end.min(text.len());
        let start = s.byte_start.min(end);
        crate::rel_schema::DeclOrigin {
            file: canon.to_path_buf(),
            span: Span::from_bytes(&idx, start, end),
        }
    };
    let key = |name: &str| here(crate::meta::meta_key_span(&meta, name));
    let origins = crate::rel_schema::DeclOrigins {
        relations: tm
            .rel_relations
            .relations
            .keys()
            .map(|n| (n.clone(), key(n)))
            .collect(),
        kinds: tm
            .rel_kinds
            .kinds
            .keys()
            .map(|n| (n.clone(), key(n)))
            .collect(),
        defs: tm.defs.keys().map(|n| (n.clone(), key(n))).collect(),
        rules: tm
            .rel_rules
            .iter()
            .map(|r| (r.raw.clone(), here(r.span)))
            .collect(),
        facts: tm
            .rel_facts
            .iter()
            .map(|f| (f.raw.clone(), here(f.span)))
            .collect(),
        domains: tm.domains.keys().map(|n| (n.clone(), key(n))).collect(),
        state: tm
            .state
            .decls
            .keys()
            .map(|p| (p.clone(), here(crate::rel_schema::state_key_span(&meta, p))))
            .collect(),
        // Prerelease N4: each listed member's own line, the first place a
        // duplicate `add:` of it names.
        members: tm
            .rel_kinds
            .kinds
            .keys()
            .flat_map(|k| {
                crate::rel_schema::kind_list_spans(&meta, k, "members:")
                    .into_iter()
                    .rev()
                    .map(|(m, span)| (crate::rel_schema::member_origin_key(k, &m), here(span)))
            })
            .collect(),
        cast: tm
            .cast
            .iter()
            .filter_map(|c| {
                let span = crate::rel_schema::cast_entry_span(&meta, &c.id)?;
                Some((c.id.clone(), here(span)))
            })
            .collect(),
        kind_keys: {
            let mut paths: Vec<(&str, Vec<&str>)> = Vec::new();
            for (kind, labels) in tm
                .rel_kinds
                .kinds
                .iter()
                .map(|(k, d)| (k, d.labels.keys().collect::<Vec<_>>()))
                .chain(
                    tm.rel_kinds
                        .add_labels
                        .iter()
                        .map(|(k, l)| (k, l.keys().collect::<Vec<_>>())),
                )
            {
                paths.push((kind.as_str(), vec!["labels"]));
                paths.extend(
                    labels
                        .into_iter()
                        .map(|m| (kind.as_str(), vec!["labels", m.as_str()])),
                );
            }
            for (kind, key) in &tm.rel_kinds.unknown_keys {
                paths.push((kind.as_str(), vec![key.as_str()]));
            }
            paths
                .into_iter()
                .filter_map(|(kind, path)| {
                    let span = crate::rel_schema::kind_key_span(&meta, kind, &path)?;
                    Some((crate::rel_schema::kind_key_origin(kind, &path), here(span)))
                })
                .collect()
        },
        relation_keys: tm
            .rel_relations
            .relations
            .iter()
            .filter(|(_, d)| d.tier.is_some())
            .map(|(n, _)| {
                let span = crate::meta::meta_path_span(&meta, &["relations", n.as_str(), "tier"]);
                (crate::rel_schema::kind_key_origin(n, &["tier"]), here(span))
            })
            .collect(),
        enum_labels: tm
            .domains
            .iter()
            .filter(|(n, _)| !tm.rel_kinds.kinds.contains_key(*n))
            .flat_map(|(n, d)| d.labels.keys().map(move |m| (n, m)))
            .map(|(n, m)| {
                let span = crate::meta::meta_path_span(
                    &meta,
                    &["enums", n.as_str(), "labels", m.as_str()],
                );
                (crate::rel_schema::member_origin_key(n, m), here(span))
            })
            .collect(),
        // Filled once every file's `add:`s are known (`resolve_imports`).
        added: BTreeMap::new(),
    };
    // dsl 0.26.0 §2.2: a member listed twice in one of this file's kinds or
    // enums, reported at its own line (the importers' copies fold).
    let own_enums: BTreeMap<String, Vec<String>> = tm
        .domains
        .iter()
        .filter(|(n, _)| !tm.rel_kinds.kinds.contains_key(*n))
        .map(|(n, d)| (n.clone(), d.members.clone()))
        .collect();
    for d in crate::rel_schema::check_member_dups(&meta, &tm.rel_kinds, &own_enums) {
        let origin = here(d.span);
        diags.push(crate::rel_schema::at_origin(d, Some(&origin)));
    }
    let failed_heads = tm.rel_rule_failed_heads.clone();
    let kind_adds = tm
        .rel_kinds
        .adds
        .iter()
        .map(|(kind, members)| crate::rel_schema::KindAdd {
            kind: kind.clone(),
            members: members.clone(),
            member_spans: crate::rel_schema::add_member_spans(&meta, kind, members)
                .into_iter()
                .map(|s| s.map(|s| here(s).span))
                .collect(),
            origin: Some(key(kind)),
            span: at,
            labels: tm
                .rel_kinds
                .add_labels
                .get(kind)
                .cloned()
                .unwrap_or_default(),
        })
        .collect();
    let faulty = tm.state.faulty;
    let clock_rejected = tm.state.clock_rejected;
    let state = tm.state.decls;
    let defs = tm.defs;
    let domains = tm.domains;
    let rel_kinds = tm.rel_kinds;
    let rel_relations = tm.rel_relations;
    let facts = tm.rel_facts;
    let rules = tm.rel_rules;
    let state_index = tm.state_index;
    let per_pending = tm
        .per_pending
        .into_iter()
        .map(|p| {
            let origin = here(p.span);
            (p, origin)
        })
        .collect();
    let cast = tm.cast;
    let clock = tm.clock.map(|c| (c, key("clock").span));
    let terminal = tm.terminal.map(|t| {
        let persists = match t.persists {
            crate::gates::Persists::Yes(at) => crate::gates::Persists::Yes(here(at).span),
            p => p,
        };
        (t.when.raw, here(t.when.span).span, persists)
    });
    let seasons = (!tm.seasons.is_empty()).then(|| {
        let lives = tm
            .seasons
            .keys()
            .map(|n| {
                let at = crate::meta::meta_path_span(&meta, &["seasons", n.as_str(), "live"]);
                (n.clone(), here(at).span)
            })
            .collect();
        (tm.seasons, key("seasons").span, lives)
    });
    let uses = tm.uses;
    let extends = tm.extends;
    (
        ParsedDoc {
            state,
            faulty,
            clock_rejected,
            defs,
            domains,
            quest_ids,
            entry_ids,
            rel_kinds,
            rel_relations,
            facts,
            rules,
            state_index,
            per_pending,
            cast,
            clock,
            terminal,
            seasons,
            origins,
            failed_heads,
            kind_adds,
        },
        uses,
        extends,
    )
}

/// Report `E-USES-DUP-*`/`E-KIND-NAME-CLASH` for every depth level at which
/// >= 2 DISTINCT files declare `name`, and return those levels. The two named
/// files are the first pair in import `order` ([`import_order`]); dsl 0.26
/// §2.7: reported at the later one's declaration line
/// ([`crate::rel_schema::at_origin`]), naming the earlier by its
/// project-relative path and line ([`crate::rel_schema::origin_display`]),
/// so the project roll-up folds every importer's copy into one report;
/// `origin_of` locates `name` in a file (`None` keeps the importer anchor).
#[allow(clippy::too_many_arguments)]
fn emit_level_dups<T>(
    code: &str,
    noun: &str,
    name: &str,
    entries: &[(PathBuf, usize, T)],
    origin_of: &dyn Fn(&Path) -> Option<crate::rel_schema::DeclOrigin>,
    order: &BTreeMap<PathBuf, usize>,
    diags: &mut Vec<Diagnostic>,
    at: Span,
) -> Vec<usize> {
    let mut by_depth: BTreeMap<usize, Vec<&PathBuf>> = BTreeMap::new();
    for (file, depth, _) in entries {
        by_depth.entry(*depth).or_default().push(file);
    }
    let mut levels = Vec::new();
    for (depth, mut files) in by_depth {
        files.sort_by_key(|f| (order.get(*f).copied().unwrap_or(usize::MAX), *f));
        files.dedup();
        if files.len() >= 2 {
            let first = origin_of(files[0]);
            let line = first
                .as_ref()
                .filter(|o| o.span.line > 0)
                .map_or(String::new(), |o| format!(":{}", o.span.line));
            let d = uses_diag(
                code,
                format!(
                    "{noun} `{name}` is declared by two imports (`{}{line}` and `{}`); keep one",
                    crate::rel_schema::origin_display(files[0]),
                    crate::rel_schema::origin_display(files[1]),
                ),
                at,
            );
            diags.push(crate::rel_schema::at_origin(
                d,
                origin_of(files[1]).as_ref(),
            ));
            levels.push(depth);
        }
    }
    levels
}

/// Each imported file's place in import order: a preorder walk of the
/// `uses:`/`extends:` lists as written, from the importing document's own
/// (`roots`), each file at its first appearance.
fn import_order(
    roots: &[PathBuf],
    adj: &BTreeMap<PathBuf, Vec<(PathBuf, Edge)>>,
) -> BTreeMap<PathBuf, usize> {
    let mut order = BTreeMap::new();
    let mut stack: Vec<&PathBuf> = roots.iter().rev().collect();
    while let Some(file) = stack.pop() {
        if order.contains_key(file) {
            continue;
        }
        order.insert(file.clone(), order.len());
        if let Some(out) = adj.get(file) {
            stack.extend(out.iter().rev().map(|(f, _)| f));
        }
    }
    order
}

/// D5's `extends`-growth check for an entity kind or `enums:` re-declaration:
/// `child` MUST be a superset re-listing of `base` (same shape); a missing
/// base member, or a `members:`/`open:` shape flip, is the mismatch reported
/// as `E-EXTENDS-RELATION-SIG`. `None` when the re-declaration is legal.
pub(crate) fn kind_shape_mismatch(child: &KindShape, base: &KindShape) -> Option<String> {
    match (child, base) {
        (KindShape::Members(c), KindShape::Members(b)) => {
            let missing = missing_members(c, b);
            if missing.is_empty() {
                None
            } else {
                Some(format!("is missing base member(s) {missing:?}"))
            }
        }
        (KindShape::Open, KindShape::Open) => None,
        _ => Some("changes shape between `members:` and `open:`".to_string()),
    }
}

/// `base`'s members not re-listed in `child` — the D5 superset check shared
/// by entity kinds and `enums:` entries.
pub(crate) fn missing_members(child: &[String], base: &[String]) -> Vec<String> {
    let child_set: BTreeSet<&String> = child.iter().collect();
    base.iter()
        .filter(|m| !child_set.contains(m))
        .cloned()
        .collect()
}

/// D5's `extends`-growth check for a relation re-declaration: the child must
/// match the base's FULL decl (`args`+`tier`+`derive`+`reserved`+`key`, and
/// dsl 0.25.0 §1's `excludes` and §6's `changedOn` as sets; `malformed_fields`
/// excluded — it is not part of the declared signature).
/// Returns the differing field names, empty when identical.
pub(crate) fn relation_sig_diff(child: &RelationDecl, base: &RelationDecl) -> Vec<&'static str> {
    let mut out = Vec::new();
    if child.args != base.args {
        out.push("args");
    }
    if child.tier.as_deref().unwrap_or("run") != base.tier.as_deref().unwrap_or("run") {
        out.push("tier");
    }
    if child.derive != base.derive {
        out.push("derive");
    }
    if child.reserved != base.reserved {
        out.push("reserved");
    }
    if child.key != base.key {
        out.push("key");
    }
    let set = |v: &[String]| v.iter().cloned().collect::<std::collections::BTreeSet<_>>();
    if set(&child.excludes) != set(&base.excludes) {
        out.push("excludes");
    }
    if set(&child.changed_on) != set(&base.changed_on) {
        out.push("changedOn");
    }
    out
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
