//! Project-wide data built once per project (spec 0.49.0 §9.2, ADR D14).
//!
//! A project check folds hundreds of documents against one set of imported
//! schemas. Before 0.49 every document cloned that whole set — the imported
//! state, the merged domains and relational vocabulary — and re-ran every
//! check over it, so the cost of a document grew with the size of the
//! project's schema (#173). Here the project-wide part is split off:
//!
//! - [`ProjectBase`]: what the manifest resolves for a profile (snapshot,
//!   providers, defaults, plugin origins), shared by every document of it;
//! - [`ProjectVocab`]: the imported schemas of one `(dir, uses, extends)`
//!   resolved against a base, with every fold stage that reads only them run
//!   once ([`BaseFold`]);
//! - [`StateView`]: a document's state, the vocab's base state plus the
//!   document's own overlay.
//!
//! `fold_env` keeps its 0.48.1 output byte for byte: it reuses a base stage
//! only while the document changes none of that stage's inputs, and reruns
//! the stage over base ∪ overlay otherwise (`check::fold`).

use std::any::{Any, TypeId};
use std::collections::{btree_map, BTreeMap, BTreeSet, HashMap};
use std::fmt;
use std::iter::Peekable;
use std::ops::Bound;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use lute_core_span::{Diagnostic, Span};
use lute_manifest::clock::ClockDecl;
use lute_manifest::project::MetaDefaults;
use lute_manifest::provider::ProviderSet;
use lute_manifest::snapshot::{CapabilitySnapshot, Domain, InvalidDecl};
use lute_manifest::types::Type;

use crate::component_import::ComponentSet;
use crate::meta::{RuleDecl, StateDecl, StateSchema};
use crate::rel_schema::{PluginOrigins, RelVocab};
use crate::schema_import::SchemaImports;

/// What a project's manifest resolves for one profile — shared by every
/// document checked against it. Memoized by `lute-load`'s `InputCache` on
/// `(root, profile, plugins, permission profile, providers dir)`.
#[derive(Debug)]
pub struct ProjectBase {
    /// The capability snapshot, with the permission profile and identity
    /// flags applied once.
    pub snapshot: Arc<CapabilitySnapshot>,
    pub providers: Arc<ProviderSet>,
    /// The governing manifest's `defaults:`.
    pub defaults: Arc<MetaDefaults>,
    /// Where the installed plugins declare what a check judges but cannot
    /// place; empty on a surface with no plugins directory.
    pub plugin_origins: Arc<PluginOrigins>,
    /// Process-unique identity: [`crate::ImportCache`] keys a vocab by the
    /// base it was built against, and an address could be reused.
    id: u64,
}

impl ProjectBase {
    pub fn new(
        snapshot: Arc<CapabilitySnapshot>,
        providers: Arc<ProviderSet>,
        defaults: Arc<MetaDefaults>,
        plugin_origins: Arc<PluginOrigins>,
    ) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        Self {
            snapshot,
            providers,
            defaults,
            plugin_origins,
            id: NEXT.fetch_add(1, Ordering::Relaxed),
        }
    }

    /// The identity [`crate::ImportCache`] keys vocabularies by.
    pub fn id(&self) -> u64 {
        self.id
    }
}

/// One `(dir, uses, extends)` import graph resolved against a
/// [`ProjectBase`], with the fold stages that read only project-wide data
/// already run. Memoized in [`crate::ImportCache`].
pub struct ProjectVocab {
    /// The resolved imports. Their diagnostics are anchored at a
    /// placeholder; a document reads its anchored copy from
    /// [`crate::CheckInput::import_diags`].
    pub imports: Arc<SchemaImports>,
    pub(crate) fold: BaseFold,
    /// What a downstream crate derives from this vocab once for all its
    /// documents (lute-compile's IR state rows, spec 0.49.0 §9.4), by type.
    derived: Mutex<HashMap<TypeId, DerivedCell>>,
}

/// One [`ProjectVocab::derived`] value, initialized once.
type DerivedCell = Arc<OnceLock<Arc<dyn Any + Send + Sync>>>;

impl fmt::Debug for ProjectVocab {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProjectVocab")
            .field("files", &self.imports.files)
            .finish_non_exhaustive()
    }
}

impl ProjectVocab {
    /// Run the project-wide fold stages over `imports` once.
    pub fn build(base: &ProjectBase, imports: Arc<SchemaImports>) -> Self {
        let fold = crate::check::base_fold(base, &imports);
        Self {
            imports,
            fold,
            derived: Mutex::default(),
        }
    }

    /// The merged domain vocabulary a document that adds no domain of its
    /// own sees (`snapshot.domains` ∪ the imported ones).
    pub fn domains(&self) -> &Arc<BTreeMap<String, Domain>> {
        &self.fold.domains
    }

    /// The merged relational vocabulary a document that declares none of
    /// its own sees.
    pub fn rel(&self) -> &Arc<RelVocab> {
        &self.fold.rel
    }

    /// The imported state, with the reserved `prev.run.*` mirrors and the
    /// members of every `{ domain: K }` path, as a view with no overlay —
    /// what every document's view starts from.
    pub fn view(&self) -> &StateView {
        &self.fold.view
    }

    /// `T` derived from this vocab, computed by `init` on first use and
    /// shared by every later caller (spec 0.49.0 §9.4: lute-compile renders
    /// the base state's IR rows once per vocab, not once per document).
    /// Concurrent first callers wait for the one computing it.
    pub fn derived<T: Any + Send + Sync>(&self, init: impl FnOnce(&Self) -> T) -> Arc<T> {
        let cell = Arc::clone(
            self.derived
                .lock()
                .expect("derived-data lock poisoned")
                .entry(TypeId::of::<T>())
                .or_default(),
        );
        let value = cell.get_or_init(|| Arc::new(init(self)));
        Arc::downcast(Arc::clone(value)).expect("derived data is stored under its own TypeId")
    }
}

/// Everything a [`crate::CheckInput`] is assembled from besides the text.
#[derive(Clone, Debug)]
pub struct InputSources {
    pub base: Arc<ProjectBase>,
    pub vocab: Arc<ProjectVocab>,
    pub components: Arc<ComponentSet>,
    /// The import diagnostics, anchored at this document.
    pub import_diags: Vec<Diagnostic>,
    /// The component diagnostics, anchored at this document.
    pub component_diags: Vec<Diagnostic>,
}

impl InputSources {
    /// Sources built from owned parts, without a cache: a surface that checks
    /// one document (the playground, a test). The diagnostics of `imports`
    /// and `components` are taken as already anchored.
    pub fn from_parts(
        snapshot: CapabilitySnapshot,
        providers: ProviderSet,
        mut imports: SchemaImports,
        mut components: ComponentSet,
        defaults: MetaDefaults,
    ) -> Self {
        let base = Arc::new(ProjectBase::new(
            Arc::new(snapshot),
            Arc::new(providers),
            Arc::new(defaults),
            Arc::default(),
        ));
        let import_diags = std::mem::take(&mut imports.diags);
        let component_diags = std::mem::take(&mut components.diags);
        let vocab = Arc::new(ProjectVocab::build(&base, Arc::new(imports)));
        Self {
            base,
            vocab,
            components: Arc::new(components),
            import_diags,
            component_diags,
        }
    }

    /// [`Self::from_parts`] with installed-plugin origins.
    pub fn with_plugin_origins(self, plugin_origins: PluginOrigins) -> Self {
        let base = Arc::new(ProjectBase::new(
            Arc::clone(&self.base.snapshot),
            Arc::clone(&self.base.providers),
            Arc::clone(&self.base.defaults),
            Arc::new(plugin_origins),
        ));
        let vocab = Arc::new(ProjectVocab::build(&base, Arc::clone(&self.vocab.imports)));
        Self {
            base,
            vocab,
            ..self
        }
    }
}

/// The project-wide fold stages, run once over a vocab's imports
/// (`check::fold::base_fold`). Diagnostics are kept as a document fold
/// emits them, anchored at [`crate::schema_import::AT_PLACEHOLDER`].
pub(crate) struct BaseFold {
    /// `merge_domains` with the kinds' final members and labels, before the
    /// clock's own enums join.
    pub(crate) domains_pre_clock: Arc<BTreeMap<String, Domain>>,
    /// [`Self::domains_pre_clock`] plus the enums of [`Self::clock`].
    pub(crate) domains: Arc<BTreeMap<String, Domain>>,
    /// The clock the imports declare, as the base state settles it.
    pub(crate) clock: Option<ClockDecl>,
    /// The relational vocabulary, rule guards expanded with the base defs.
    pub(crate) rel: Arc<RelVocab>,
    /// The rules as `build_rel_vocab` returns them, before guard expansion.
    pub(crate) pre_guard_rules: Vec<RuleDecl>,
    /// Some rule guard names a def (`@`), so a document whose def table
    /// differs expands the guards again.
    pub(crate) guards_use_defs: bool,
    /// The merge/vocab/rule/stratification diagnostics, in fold order.
    pub(crate) rel_diags: Vec<Diagnostic>,
    /// Rule-guard expansion diagnostics.
    pub(crate) guard_diags: Vec<Diagnostic>,
    /// No base relational diagnostic depends on the document's frontmatter
    /// text, so a document that changes none of their inputs reuses them.
    pub(crate) rel_shared: bool,
    /// The imported state plus the `prev.run.*` mirrors and the members of
    /// every `{ domain: K }` / `{ entity: K }` path under [`Self::domains`],
    /// as a view with no overlay, with its summary.
    pub(crate) view: StateView,
    /// Each domain name a base state path is typed by, with those paths.
    pub(crate) paths_by_domain: BTreeMap<String, Vec<String>>,
    /// The imported defs, settled against the base state.
    pub(crate) imported_defs: Arc<BTreeMap<String, yaml_serde::Value>>,
    /// `E-DEF-DECL` for the imported defs.
    pub(crate) def_diags: Vec<Diagnostic>,
    /// Every state path an imported def body names; a document declaring
    /// one of them (or a path above or below one) settles the defs again.
    pub(crate) def_reads: BTreeSet<String>,
    /// Plugin < imported def tables.
    pub(crate) def_types: BTreeMap<String, Type>,
    pub(crate) def_params: BTreeMap<String, Vec<(String, Type)>>,
    pub(crate) def_bodies: BTreeMap<String, String>,
    /// `E-STATE-DECL` value/prefix reports over the base state, by value path.
    pub(crate) value_prefix: BTreeMap<String, Diagnostic>,
    /// `{ domain: K }` reports over the base state, by reported path (`at`),
    /// with the first path that decided it.
    pub(crate) domain_types: BTreeMap<String, (String, Diagnostic)>,
    /// `snapshot.domains` extended with the imported domains — what a
    /// template header argument is member-checked against.
    pub(crate) template_domains: Arc<BTreeMap<String, Domain>>,
}

/// A span no document has: a base stage that reads the frontmatter text is
/// run against an empty frontmatter spanning this, so a diagnostic carrying
/// it depends on the document and is not reused.
pub(crate) const KEY_PLACEHOLDER: Span = Span {
    byte_start: usize::MAX / 2,
    byte_end: usize::MAX / 2,
    line: u32::MAX / 2,
    column: u32::MAX / 2,
    utf16_range: (u32::MAX / 2, u32::MAX / 2),
};

/// Whether any span of `diags` (outer or related) is `span`.
pub(crate) fn carries_span(diags: &[Diagnostic], span: Span) -> bool {
    diags.iter().any(|d| {
        d.span == span
            || d.related
                .iter()
                .any(|r| carries_span(std::slice::from_ref(&r.diagnostic), span))
    })
}

/// Whether two domains are the same declaration (`Domain` has no
/// `PartialEq`; every field is compared).
pub(crate) fn same_domain(a: &Domain, b: &Domain) -> bool {
    a.members == b.members
        && a.open == b.open
        && a.default == b.default
        && a.exits == b.exits
        && a.stops == b.stops
        && a.labels == b.labels
}

/// The names whose domain differs between `base` and `doc`.
pub(crate) fn changed_domains(
    base: &BTreeMap<String, Domain>,
    doc: &BTreeMap<String, Domain>,
) -> BTreeSet<String> {
    let mut out: BTreeSet<String> = doc
        .iter()
        .filter(|(name, d)| base.get(*name).is_none_or(|b| !same_domain(b, d)))
        .map(|(name, _)| name.clone())
        .collect();
    out.extend(base.keys().filter(|name| !doc.contains_key(*name)).cloned());
    out
}

/// A document's state: the vocab's base state and the document's own
/// overlay (its inline decls, implicit decls, refinements and derived
/// entries). Lookups read the overlay, then the base; iteration merges both
/// in byte order, the overlay winning.
#[derive(Clone, Default)]
pub struct StateView {
    base: Arc<StateSchema>,
    /// What the project-wide passes read of [`Self::base`], aggregated once.
    summary: Arc<StateSummary>,
    overlay: StateSchema,
    /// Base paths whose domain this document changed into one that does
    /// not resolve (open, or undeclared): no members, though the base has.
    masked_domains: BTreeSet<String>,
}

/// What the project-wide passes read of a base state, aggregated once per
/// vocab instead of once per document (spec 0.49.0 §9.2 R5).
#[derive(Debug, Default)]
pub struct StateSummary {
    /// The `run.*`/`user.*` paths with a schema `default:` (the envelope's
    /// entry floor `D`, [`crate::envelope::schema_defaults`]).
    defaults: BTreeSet<String>,
    /// The `owner: engine` paths.
    engine_owned: BTreeSet<String>,
    /// Each domain name a declaration's type names, with how many
    /// declarations name it.
    domain_reads: BTreeMap<String, usize>,
}

impl StateSummary {
    fn of(schema: &StateSchema) -> Self {
        let mut out = Self::default();
        for (path, decl) in &schema.decls {
            if is_default(path, decl) {
                out.defaults.insert(path.clone());
            }
            if is_engine_owned(decl) {
                out.engine_owned.insert(path.clone());
            }
            let mut names = BTreeSet::new();
            crate::project_check::collect_domain_names(&decl.ty, &mut names);
            for name in names {
                *out.domain_reads.entry(name).or_default() += 1;
            }
        }
        out
    }
}

/// A path the envelope's entry floor holds (see [`StateSummary::defaults`]).
fn is_default(path: &str, decl: &StateDecl) -> bool {
    decl.default.is_some() && crate::envelope::in_envelope_scope(path)
}

fn is_engine_owned(decl: &StateDecl) -> bool {
    decl.owner == Some(lute_manifest::types::Owner::Engine)
}

impl From<StateSchema> for StateView {
    /// A view whose every entry is its own (no shared base).
    fn from(overlay: StateSchema) -> Self {
        Self {
            base: Arc::default(),
            summary: Arc::default(),
            overlay,
            masked_domains: BTreeSet::new(),
        }
    }
}

impl StateView {
    /// A view over `base` with an empty overlay; `base` is summarized here,
    /// once, for every view cloned from this one.
    pub fn over(base: Arc<StateSchema>) -> Self {
        let overlay = StateSchema {
            clock_rejected: base.clock_rejected,
            ..StateSchema::default()
        };
        Self {
            summary: Arc::new(StateSummary::of(&base)),
            base,
            overlay,
            masked_domains: BTreeSet::new(),
        }
    }

    /// Whether `self` and `other` share one base.
    pub fn shares_base(&self, other: &StateView) -> bool {
        Arc::ptr_eq(&self.base, &other.base)
    }

    /// Every path this view may answer differently from its base: the
    /// overlay's declarations, the base paths whose domain members it
    /// resolves again or masks. In byte order, each once.
    pub fn own_paths(&self) -> BTreeSet<&String> {
        self.overlay
            .decls
            .keys()
            .chain(self.overlay.domain_members.keys())
            .chain(&self.masked_domains)
            .collect()
    }

    /// Every domain name a declaration's type names
    /// ([`crate::project_check::domain_reads_from_state`]): the overlay's,
    /// and each the base names through a declaration the overlay does not
    /// replace.
    pub fn domain_reads(&self) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        let mut replaced: BTreeMap<String, usize> = BTreeMap::new();
        for (path, decl) in &self.overlay.decls {
            crate::project_check::collect_domain_names(&decl.ty, &mut out);
            if let Some(base) = self.base.decls.get(path) {
                let mut names = BTreeSet::new();
                crate::project_check::collect_domain_names(&base.ty, &mut names);
                for name in names {
                    *replaced.entry(name).or_default() += 1;
                }
            }
        }
        for (name, count) in &self.summary.domain_reads {
            if replaced.get(name).is_none_or(|r| r < count) {
                out.insert(name.clone());
            }
        }
        out
    }

    /// The declaration of `path`.
    pub fn get(&self, path: &str) -> Option<&StateDecl> {
        self.overlay
            .decls
            .get(path)
            .or_else(|| self.base.decls.get(path))
    }

    pub fn contains(&self, path: &str) -> bool {
        self.overlay.decls.contains_key(path) || self.base.decls.contains_key(path)
    }

    /// Every declaration, in byte order of the path.
    pub fn iter(&self) -> Merged<'_, StateDecl> {
        Merged::new(
            self.overlay.decls.range::<str, _>(..),
            self.base.decls.range::<str, _>(..),
        )
    }

    /// Every declared path, in byte order.
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.iter().map(|(k, _)| k)
    }

    /// The declarations whose path starts with `prefix`, in byte order.
    pub fn with_prefix<'a>(&'a self, prefix: &str) -> Prefixed<'a> {
        let from = (Bound::Included(prefix), Bound::Unbounded);
        Prefixed {
            inner: Merged::new(
                self.overlay.decls.range::<str, _>(from),
                self.base.decls.range::<str, _>(from),
            ),
            prefix: prefix.to_string(),
        }
    }

    /// The declarations directly under `family` (`family.<m>`, no further
    /// dot), in byte order — a `per:` family's members.
    pub fn members_of<'a>(
        &'a self,
        family: &str,
    ) -> impl Iterator<Item = (&'a String, &'a StateDecl)> + 'a {
        let at = family.len() + 1;
        self.with_prefix(&format!("{family}."))
            .filter(move |(k, _)| !k[at..].contains('.'))
    }

    /// The nearest declared strict ancestor of `path` (`run.a` for
    /// `run.a.b.c` when `run.a.b` is undeclared), with its declaration.
    pub fn nearest_ancestor<'p>(&self, path: &'p str) -> Option<(&'p str, &StateDecl)> {
        let mut end = path.len();
        while let Some(dot) = path[..end].rfind('.') {
            end = dot;
            if let Some(decl) = self.get(&path[..end]) {
                return Some((&path[..end], decl));
            }
        }
        None
    }

    /// `path` itself when declared, then each declared strict ancestor,
    /// nearest first.
    pub fn lineage<'p>(&self, path: &'p str) -> Vec<(&'p str, &StateDecl)> {
        let mut out = Vec::new();
        let mut end = path.len();
        loop {
            if let Some(decl) = self.get(&path[..end]) {
                out.push((&path[..end], decl));
            }
            match path[..end].rfind('.') {
                Some(dot) => end = dot,
                None => return out,
            }
        }
    }

    /// Whether `path` is declared or lies under a declared path.
    pub fn covers(&self, path: &str) -> bool {
        self.contains(path) || self.nearest_ancestor(path).is_some()
    }

    /// Whether some declared path is a strict descendant of `path`.
    pub fn has_descendant(&self, path: &str) -> bool {
        self.with_prefix(&format!("{path}.")).next().is_some()
    }

    /// The finite string domain of a declared path: an inline enum's
    /// members, or a `{ domain: K }` / `{ entity: K }` path's `K` members.
    pub fn string_members(&self, path: &str) -> Option<&[String]> {
        if let Some((_, ms)) = self.domain_members(path) {
            return Some(ms);
        }
        match &self.get(path)?.ty {
            Type::Enum(ms) => Some(ms),
            _ => None,
        }
    }

    /// The `{ domain: K }` name and members of `path`
    /// ([`StateSchema::domain_members`]).
    pub fn domain_members(&self, path: &str) -> Option<&(String, Vec<String>)> {
        if let Some(m) = self.overlay.domain_members.get(path) {
            return Some(m);
        }
        if self.overlay.decls.contains_key(path) || self.masked_domains.contains(path) {
            return None;
        }
        self.base.domain_members.get(path)
    }

    /// Every whole-number range, in byte order of the path.
    pub fn int_ranges(&self) -> Merged<'_, (i64, i64)> {
        Merged::new(
            self.overlay.int_ranges.range::<str, _>(..),
            self.base.int_ranges.range::<str, _>(..),
        )
    }

    /// Every clock-narrowed slot path, in byte order.
    pub fn clock_member_sets(&self) -> Merged<'_, Vec<String>> {
        Merged::new(
            self.overlay.clock_members.range::<str, _>(..),
            self.base.clock_members.range::<str, _>(..),
        )
    }

    /// The whole-number range of `path` ([`StateSchema::int_ranges`]).
    pub fn int_range(&self, path: &str) -> Option<&(i64, i64)> {
        self.overlay
            .int_ranges
            .get(path)
            .or_else(|| self.base.int_ranges.get(path))
    }

    /// The slot members a finite clock lets `path` hold
    /// ([`StateSchema::clock_members`]).
    pub fn clock_members(&self, path: &str) -> Option<&Vec<String>> {
        self.overlay
            .clock_members
            .get(path)
            .or_else(|| self.base.clock_members.get(path))
    }

    /// Whether the finite clock never lets `path` hold `member`.
    pub fn clock_excludes(&self, path: &str, member: &str) -> bool {
        self.clock_members(path)
            .is_some_and(|ms| !ms.iter().any(|m| m == member))
    }

    /// The project's clock, when there is one.
    pub fn clock(&self) -> Option<&ClockDecl> {
        self.overlay.clock.as_ref().or(self.base.clock.as_ref())
    }

    /// A written `clock:` was rejected ([`StateSchema::clock_rejected`]).
    pub fn clock_rejected(&self) -> bool {
        self.overlay.clock_rejected
    }

    /// Whether a read of `path` depends on a declaration already reported
    /// ([`StateSchema::is_faulty`]).
    pub fn is_faulty(&self, path: &str) -> bool {
        self.overlay.is_faulty(path) || self.base.faulty_covers(path)
    }

    /// The invalid row of `path` ([`StateSchema::invalid`]).
    pub fn invalid(&self, path: &str) -> Option<&InvalidDecl> {
        self.overlay
            .invalid
            .get(path)
            .or_else(|| self.base.invalid.get(path))
    }

    /// Every invalid row, in byte order of the path.
    pub fn invalid_rows(&self) -> Merged<'_, InvalidDecl> {
        Merged::new(
            self.overlay.invalid.range::<str, _>(..),
            self.base.invalid.range::<str, _>(..),
        )
    }

    /// The document's own entries.
    pub(crate) fn overlay(&self) -> &StateSchema {
        &self.overlay
    }

    pub(crate) fn overlay_mut(&mut self) -> &mut StateSchema {
        &mut self.overlay
    }

    /// The shared base entries.
    pub(crate) fn base(&self) -> &StateSchema {
        &self.base
    }

    /// The overlay of a view with no shared base ([`From<StateSchema>`]).
    pub(crate) fn into_overlay(self) -> StateSchema {
        debug_assert!(
            self.base.decls.is_empty(),
            "only a base-less view is one schema"
        );
        self.overlay
    }

    /// Declare `path` in the overlay (replacing any base declaration).
    pub(crate) fn insert(&mut self, path: String, decl: StateDecl) {
        self.overlay.decls.insert(path, decl);
    }

    /// Mark a base path's domain members as no longer resolved.
    pub(crate) fn mask_domain(&mut self, path: String) {
        self.masked_domains.insert(path);
    }
}

/// The union, over a project's documents, of what each one's state view
/// holds of a [`StateSummary`] set — computed from each shared base once
/// plus each document's overlay, never by iterating a base per document
/// (spec 0.49.0 §9.2 R5). Equal to the union of the per-view sets.
#[derive(Default)]
pub struct StateUnion<'a> {
    own_defaults: BTreeSet<String>,
    own_engine_owned: BTreeSet<String>,
    bases: Vec<BaseUse<'a>>,
}

impl fmt::Debug for StateUnion<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StateUnion")
            .field("bases", &self.bases.len())
            .finish_non_exhaustive()
    }
}

/// The views of one shared base a [`StateUnion`] has seen: how many, and
/// for each base path in a summary set, how many of them replace it with a
/// declaration outside that set.
struct BaseUse<'a> {
    summary: &'a StateSummary,
    views: usize,
    unset_defaults: HashMap<&'a str, usize>,
    unset_engine_owned: HashMap<&'a str, usize>,
}

impl<'a> StateUnion<'a> {
    pub fn add(&mut self, view: &'a StateView) {
        let summary: &'a StateSummary = &view.summary;
        let mut base = if summary.defaults.is_empty() && summary.engine_owned.is_empty() {
            // Nothing of this base to subtract from (a view with no shared
            // base, or a base with neither kind of path).
            None
        } else {
            let at = match self
                .bases
                .iter()
                .position(|b| std::ptr::eq(b.summary, summary))
            {
                Some(at) => at,
                None => {
                    self.bases.push(BaseUse {
                        summary,
                        views: 0,
                        unset_defaults: HashMap::new(),
                        unset_engine_owned: HashMap::new(),
                    });
                    self.bases.len() - 1
                }
            };
            let base = &mut self.bases[at];
            base.views += 1;
            Some(base)
        };
        for (path, decl) in &view.overlay.decls {
            if is_default(path, decl) {
                self.own_defaults.insert(path.clone());
            } else if let Some(b) = base.as_mut().filter(|_| summary.defaults.contains(path)) {
                *b.unset_defaults.entry(path.as_str()).or_default() += 1;
            }
            if is_engine_owned(decl) {
                self.own_engine_owned.insert(path.clone());
            } else if let Some(b) = base
                .as_mut()
                .filter(|_| summary.engine_owned.contains(path))
            {
                *b.unset_engine_owned.entry(path.as_str()).or_default() += 1;
            }
        }
    }

    /// ∪ [`crate::envelope::schema_defaults`] of every added view.
    pub fn defaults(&self) -> BTreeSet<String> {
        self.union(&self.own_defaults, |b| {
            (&b.summary.defaults, &b.unset_defaults)
        })
    }

    /// ∪ of every added view's `owner: engine` paths.
    pub fn engine_owned(&self) -> BTreeSet<String> {
        self.union(&self.own_engine_owned, |b| {
            (&b.summary.engine_owned, &b.unset_engine_owned)
        })
    }

    fn union(
        &self,
        own: &BTreeSet<String>,
        set: impl for<'b> Fn(&'b BaseUse<'a>) -> (&'a BTreeSet<String>, &'b HashMap<&'a str, usize>),
    ) -> BTreeSet<String> {
        let mut out = own.clone();
        for base in &self.bases {
            let (paths, unset) = set(base);
            // A base path is in the union unless every view replaced it.
            out.extend(
                paths
                    .iter()
                    .filter(|p| unset.get(p.as_str()).is_none_or(|n| *n < base.views))
                    .cloned(),
            );
        }
        out
    }
}

/// Printed as the merged state, so a view equals any other view with the
/// same entries however they are split between base and overlay.
impl fmt::Debug for StateView {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let domain_members: BTreeMap<&String, &(String, Vec<String>)> = self
            .keys()
            .filter_map(|k| self.domain_members(k).map(|m| (k, m)))
            .collect();
        let ranges: Vec<_> = Merged::new(
            self.overlay.int_ranges.range::<str, _>(..),
            self.base.int_ranges.range::<str, _>(..),
        )
        .collect();
        let clock_members: Vec<_> = Merged::new(
            self.overlay.clock_members.range::<str, _>(..),
            self.base.clock_members.range::<str, _>(..),
        )
        .collect();
        let faulty: BTreeSet<&String> = self
            .overlay
            .faulty
            .iter()
            .chain(&self.base.faulty)
            .collect();
        f.debug_struct("StateView")
            .field("decls", &self.iter().collect::<Vec<_>>())
            .field("int_ranges", &ranges)
            .field("domain_members", &domain_members)
            .field("clock_members", &clock_members)
            .field("clock", &self.clock())
            .field("faulty", &faulty)
            .field("clock_rejected", &self.clock_rejected())
            .field("invalid", &self.invalid_rows().collect::<Vec<_>>())
            .finish()
    }
}

/// Two sorted map ranges merged in key order; on an equal key the first
/// (the overlay) wins and the second's entry is skipped.
pub struct Merged<'a, V> {
    overlay: Peekable<btree_map::Range<'a, String, V>>,
    base: Peekable<btree_map::Range<'a, String, V>>,
}

impl<V> fmt::Debug for Merged<'_, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Merged").finish_non_exhaustive()
    }
}

impl<'a, V> Merged<'a, V> {
    fn new(
        overlay: btree_map::Range<'a, String, V>,
        base: btree_map::Range<'a, String, V>,
    ) -> Self {
        Self {
            overlay: overlay.peekable(),
            base: base.peekable(),
        }
    }
}

/// [`StateView::with_prefix`]: the merged declarations from `prefix` on, up
/// to the first path that does not start with it.
pub struct Prefixed<'a> {
    inner: Merged<'a, StateDecl>,
    prefix: String,
}

impl fmt::Debug for Prefixed<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Prefixed")
            .field("prefix", &self.prefix)
            .finish_non_exhaustive()
    }
}

impl<'a> Iterator for Prefixed<'a> {
    type Item = (&'a String, &'a StateDecl);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner
            .next()
            .filter(|(k, _)| k.starts_with(&self.prefix))
    }
}

impl<'a, V> Iterator for Merged<'a, V> {
    type Item = (&'a String, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        match (self.overlay.peek(), self.base.peek()) {
            (Some((o, _)), Some((b, _))) => match o.as_str().cmp(b.as_str()) {
                std::cmp::Ordering::Less => self.overlay.next(),
                std::cmp::Ordering::Greater => self.base.next(),
                std::cmp::Ordering::Equal => {
                    self.base.next();
                    self.overlay.next()
                }
            },
            (Some(_), None) => self.overlay.next(),
            (None, _) => self.base.next(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::Namespace;

    fn decl(ty: Type) -> StateDecl {
        StateDecl {
            ty,
            default: None,
            namespace: Namespace::Run,
            owner: None,
        }
    }

    /// spec 0.49.0 §10: the view iterates in the order the 0.48.1 merged
    /// `BTreeMap` did, the overlay replacing a base entry in place.
    #[test]
    fn view_order_equals_merged_map_order() {
        let mut base = StateSchema::default();
        let mut overlay = StateSchema::default();
        let mut merged = BTreeMap::new();
        for (i, path) in ["run.a", "run.a.b", "run.a-z", "run.b", "user.x"]
            .iter()
            .enumerate()
        {
            let d = decl(if i % 2 == 0 { Type::Int } else { Type::Bool });
            base.decls.insert(path.to_string(), d.clone());
            merged.insert(path.to_string(), d);
        }
        for path in ["run.a.a", "run.b", "scene.c", "app.z"] {
            let d = decl(Type::Str);
            overlay.decls.insert(path.to_string(), d.clone());
            merged.insert(path.to_string(), d);
        }
        let mut view = StateView::over(Arc::new(base));
        *view.overlay_mut() = overlay;
        let got: Vec<(&String, &StateDecl)> = view.iter().collect();
        let want: Vec<(&String, &StateDecl)> = merged.iter().collect();
        assert_eq!(got, want);
        let prefixed: Vec<&String> = view.with_prefix("run.a").map(|(k, _)| k).collect();
        assert_eq!(prefixed, ["run.a", "run.a-z", "run.a.a", "run.a.b"]);
        assert_eq!(
            view.nearest_ancestor("run.a.b.c").map(|(p, _)| p),
            Some("run.a.b")
        );
        assert!(view.has_descendant("run.a"));
        assert!(!view.has_descendant("run.b"));
    }
}
