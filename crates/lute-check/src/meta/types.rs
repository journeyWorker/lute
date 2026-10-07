use super::*;
use std::collections::{BTreeMap, BTreeSet};

use lute_manifest::snapshot::Domain;


/// State lifetime tier (dsl §9.1), keyed by the declared path's leading segment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Namespace {
    Scene,
    Run,
    User,
    App,
    /// `quest.<id>.*` (dsl 0.2.0 §5): a scratch tier scoped to one quest
    /// instance, MAY carry engine-reserved implicit sub-namespaces
    /// (`quest.<id>.state`, `quest.<id>.objectives.<oid>.done`, §5.2).
    Quest,
    /// dsl 0.27.0 §5: `season.<name>.*` — a declared season's tier, reset
    /// to its defaults each time the season opens.
    Season,
}

/// A single `state:` declaration (dsl §9.3): `type` + optional `default`, plus
/// the tier its path prefix maps to, and (dsl 0.22.0 §1.2) who writes it.
#[derive(Clone, Debug, PartialEq)]
pub struct StateDecl {
    pub ty: Type,
    pub default: Option<Literal>,
    pub namespace: Namespace,
    /// `owner: engine` (dsl 0.22.0 §1.2): content `::set` of this path is
    /// `E-ENGINE-OWNED-WRITE`. `None` = content-writable.
    pub owner: Option<lute_manifest::types::Owner>,
}

/// The document's inline `state:` schema (dsl §9), path -> decl.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StateSchema {
    pub decls: BTreeMap<String, StateDecl>,
    /// `number` paths known to hold only the whole numbers `lo..=hi` — the
    /// reserved `clock.weekday` (`0..length-1`, dsl 0.24.0 §1). A `<match>`
    /// over one is exhaustive once every whole number in range is covered.
    pub int_ranges: BTreeMap<String, (i64, i64)>,
    /// dsl 0.27.0 §2 (T1-2): the members of every path typed `{ domain: K }`
    /// or `{ entity: K }` whose `K` (an enum or a closed entity kind) is
    /// known, in declaration order — so a literal meeting the path (`::set`,
    /// `==`/`!=`/`in`, `<when is>`, `into=`) is member-checked and a
    /// `<match>` over it is exhaustive over `K`, exactly like an inline
    /// `{ enum: […] }` path. Filled by [`StateSchema::resolve_domains`].
    pub domain_members: BTreeMap<String, (String, Vec<String>)>,
    /// dsl 0.28.0 (T3-40): the slot members a finite clock that ends on
    /// its first day ever lets its slot path (and `clock.slot`) hold — the
    /// type's members from the starting slot to the last one. A literal
    /// outside them is a member of the type the clock never reaches.
    pub clock_members: BTreeMap<String, Vec<String>>,
    /// The project's declared clock (dsl 0.24.0 §1), when there is one —
    /// what names its `day` / `slot` paths to the checks that read them.
    pub clock: Option<lute_manifest::clock::ClockDecl>,
    /// dsl 0.28.0 §7 (T3-1): declared paths whose own `state:` row was
    /// reported (an unknown row key, a `default:` outside its enum) — the
    /// row's error is the cause, so judgements downstream of the path (it
    /// may be unset, a literal against it, a guard it decides) stay quiet.
    pub faulty: BTreeSet<String>,
    /// T3-1: a `clock:` was written (in this document or a schema it
    /// imports) and rejected (`E-CLOCK-DECL`), so the project has no clock
    /// because of that one error — what needs a clock stays quiet.
    pub clock_rejected: bool,
}

impl StateSchema {
    /// The finite string domain of a declared path: an inline enum's members,
    /// or a `{ domain: K }` / `{ entity: K }` path's `K` members.
    pub fn string_members(&self, path: &str) -> Option<&[String]> {
        if let Some((_, ms)) = self.domain_members.get(path) {
            return Some(ms);
        }
        match &self.decls.get(path)?.ty {
            Type::Enum(ms) => Some(ms),
            _ => None,
        }
    }

    /// dsl 0.28.0 (T3-40): whether the finite clock never lets `path` hold
    /// `member` ([`Self::clock_members`]).
    pub fn clock_excludes(&self, path: &str, member: &str) -> bool {
        self.clock_members
            .get(path)
            .is_some_and(|ms| !ms.iter().any(|m| m == member))
    }

    /// T3-1: whether a read of `path` depends on a declaration already
    /// reported ([`Self::faulty`]): the path itself, a member under a faulty
    /// state path (`run.bond.ines` under `run.bond`), or — while a written
    /// `clock:` was rejected — a `clock.*` path. A bare root in `faulty`
    /// (`run`, from a refused member of that name) covers only itself.
    pub fn is_faulty(&self, path: &str) -> bool {
        if self.clock_rejected && (path == "clock" || path.starts_with("clock.")) {
            return true;
        }
        self.faulty.contains(path)
            || self.faulty.iter().any(|f| {
                f.contains('.')
                    && path.len() > f.len()
                    && path.starts_with(f.as_str())
                    && path.as_bytes()[f.len()] == b'.'
            })
    }

    /// Fill [`Self::domain_members`] from the merged domains (enums and entity
    /// kinds). An open kind, or a name no domain declares, stays unresolved.
    pub fn resolve_domains(&mut self, domains: &BTreeMap<String, lute_manifest::snapshot::Domain>) {
        for (path, decl) in &self.decls {
            let (Type::Domain(name) | Type::Entity(name)) = &decl.ty else {
                continue;
            };
            if let Some(d) = domains.get(name).filter(|d| !d.open) {
                self.domain_members
                    .insert(path.clone(), (name.clone(), d.members.clone()));
            }
        }
    }
}

/// A parsed seed fact from a `facts:` list (spec §4). Seeds are ground
/// (checked as for `::assert` — no `_` wildcard, decision D12).
#[derive(Clone, Debug)]
pub struct FactDecl {
    pub fact: lute_syntax::datalog::FactPattern,
    pub raw: String,
    pub span: Span,
}

/// A parsed rule from a `rules:` list (spec §7.1).
#[derive(Clone, Debug)]
pub struct RuleDecl {
    pub rule: lute_syntax::datalog::Rule,
    pub raw: String,
    pub span: Span,
}

/// Typed frontmatter (dsl §6.1). Built-in core keys are lifted into fields;
/// `plugins`/`defs` are retained structurally for downstream tasks.
///
/// `yaml` is the parsed authored mapping snapshot. It is retained so project
/// passes can inspect uncommon keys without reparsing `Meta::raw_yaml`.
#[derive(Clone, Debug, Default)]
pub struct TypedMeta {
    pub(crate) yaml: Option<serde_yaml::Value>,
    pub character: Option<String>,
    pub season: Option<i64>,
    pub episode: Option<i64>,
    /// dsl §2.3: authored `episodeId:` frontmatter value (if any). A non-empty
    /// authored value overrides the derived `s{season:02}ep{episode:02}` default
    /// in [`canonical_episode_id`]; an absent or empty value falls back to the
    /// default. Lifted so [`canonical_scene_key`] (dsl 0.15.0 §2) can reproduce
    /// the derived scene key from `TypedMeta` alone without a second raw-YAML
    /// peek.
    pub episode_id: Option<String>,
    pub pov: Option<String>,
    /// dsl 0.37.0 §3.4: the frontmatter (or defaulted) `monoSpeakers:` —
    /// speakers beside the effective POV who may speak `mono`.
    pub mono_speakers: Vec<String>,
    /// The frontmatter `luteVersion:` stamp (dsl §6.1), lifted straight from
    /// the raw YAML mapping like `character`/`pov`. D13 stands: it is NEVER
    /// validated against capabilities — `check()` only compares it against
    /// the toolchain's [`crate::LUTE_LANG_VERSION`] for the warning-grade
    /// `W-LUTE-VERSION-STALE` freshness signal (dsl 0.6.1 §3).
    pub lute_version: Option<String>,
    /// The scene-level prerequisite `after:` frontmatter key (connectivity
    /// layer, T2): raw CEL text, lifted straight from the raw YAML mapping
    /// the same way `character`/`season`/`episode`/`pov` are — validated
    /// separately (grammar only, `crate::prereq::parse_prereq`) by `check()`,
    /// never here.
    pub after: Option<String>,
    /// dsl 0.15.0 §2: authored canonical scene key. When present, this string
    /// is the canonical scene identity everywhere the derived
    /// `{character}.{episodeId}` join is consumed today (lineId prefix,
    /// connectivity `visited(K)` targets, `prereqEdges[].node`,
    /// `project.index.json` document key, `lute play`'s visited-set). The
    /// value is validated on lift: non-empty and matching `[A-Za-z0-9_.-]+`
    /// — anything else stays `None` and draws `E-META-ID`. Absent → derived
    /// fallback via [`canonical_scene_key`], byte-identical to 0.14.0.
    ///
    /// dsl 0.19.0 §2.1: on a quest or lore document the same key (same
    /// shape rule) is the optional document id — the artifact's `meta.id`
    /// and its `ProjectIndex` key. [`canonical_scene_key`] is scene-only;
    /// callers never consult it for another kind.
    pub id: Option<String>,
    /// dsl 0.19.0 §2.1 (D-K): a lore document's `series:` — every entry of
    /// the document belongs to this series, ordered by position. Lifted only
    /// on `MetaKind::Lore` and only when the value is an `Ident`
    /// (`E-META-VALUE` otherwise). Resolution into per-entry positions is
    /// [`crate::lore::resolve_entry_series`].
    pub series: Option<String>,
    /// dsl 0.21.0 §3.1: a scene's beat declaration (`on` / `target` / `when`
    /// / `priority` / `once`), lifted on `MetaKind::Scene` only and only when
    /// `on:` is present and an identifier ([`crate::beats::lift_scene_beat`]).
    pub beat: Option<crate::beats::BeatMeta>,
    /// dsl 0.15.0 §3: authored `extra:` descriptive block. Free open mapping
    /// with scalar or flat-scalar-list values, never consulted by any
    /// checker/compiler/runtime rule and never routed through CEL — the
    /// sanctioned home for team search metadata (`arc`, `location`, whatever)
    /// carried verbatim into the artifact (`SceneMeta.meta`/`QuestMeta.meta`,
    /// omitted when empty). A nested mapping or non-scalar list entry stays
    /// out of this map and draws `E-META-VALUE`.
    pub extra_block: BTreeMap<String, serde_json::Value>,
    pub profile: Option<String>,
    pub plugins: BTreeMap<String, serde_yaml::Value>,
    pub uses: Vec<String>,
    pub extends: Vec<String>,
    pub state: StateSchema,
    pub defs: BTreeMap<String, serde_yaml::Value>,
    /// Project-authored `enums:`/`entities:` declarations (dsl data-catalog
    /// foundation A3; 0.3.0 draft §3.1), parsed via
    /// `lute_manifest::entities::parse_enums` /
    /// `lute_manifest::relations::{parse_entity_kinds, kinds_to_domains}`
    /// into enum-style/open [`Domain`]s. Lifted into the checker's merged
    /// domain vocabulary the SAME way as `state`/`defs` (`crate::schema_import`,
    /// alongside `CapabilitySnapshot.domains`, A2). A same-doc `enums:`/
    /// `entities:` name collision is NOT diagnosed here (`entities:` simply
    /// wins) — cross-source collisions are `schema_import`'s job
    /// (`E-DOMAIN-DUP`).
    pub domains: BTreeMap<String, Domain>,
    /// Scene-level reusable-content component imports (dsl §13): each entry is a
    /// relative path to a component file resolved via `resolve_components`.
    pub components: Vec<String>,
    /// A component file's own declared name (dsl §13): `Some` only when this
    /// document was parsed as a `MetaKind::Component` file that declared
    /// `component:`. A scene leaves this `None`.
    pub component: Option<String>,
    /// A component file's declared params (dsl §13), in source order (the
    /// named-arg binding namespace for `::use`). Empty for a scene.
    pub params: Vec<DefParam>,
    /// True when a `params:` key is PRESENT but malformed (not a mapping, a
    /// non-string key, or a value that is not a valid [`Type`]) — the resolver
    /// surfaces this as `E-COMPONENT-PARSE` rather than silently entering a
    /// shrunken signature (dsl §13). `false` when `params:` is absent or wholly
    /// valid.
    pub params_malformed: bool,
    /// dsl 0.24.0 §4: a component file's `effects: true` — its body may
    /// `::set`/`::assert`/`::retract`, each write checked at every `::use`
    /// site against the host's schema. `false` when absent.
    pub effects: bool,
    /// dsl 0.27.0 §6: a component file's `beat:` header template — the
    /// component is then a beat template (`<beat use="name">`).
    pub beat_template: Option<crate::templates::BeatTemplate>,
    /// dsl 0.24.0 §4: the component params declared `speaker` (a cast id;
    /// `{{@p}}` renders the cast name). Each is ALSO in [`Self::params`],
    /// typed `string` there — the host's cast narrows it at a `::use`.
    pub speaker_params: Vec<String>,
    /// dsl 0.26.0 §3.3: each param's `default:` (`{ type: X, default: V }`)
    /// as the `::use` argument an omitted param takes — a literal (`Str`) or
    /// a `@def` (`Ref`, resolved in the host at each `::use`).
    pub param_defaults: BTreeMap<String, lute_syntax::ast::AttrValue>,
    /// Project-authored `entities:` entity-kind decls (0.3.0 draft §3.1, T4),
    /// parsed via `lute_manifest::relations::parse_entity_kinds`. Distinct
    /// from [`Self::domains`] (the 0.2.2 attr-layer projection): this is the
    /// full decl shape the relational checker (Tasks 6/7) needs.
    pub rel_kinds: lute_manifest::relations::ParsedKinds,
    /// Project-authored `relations:` decls (0.3.0 draft §4, T4), parsed via
    /// `lute_manifest::relations::parse_relations`.
    pub rel_relations: lute_manifest::relations::ParsedRelations,
    /// Project-authored `facts:` seeds (0.3.0 draft §4), each string parsed
    /// via `lute_syntax::datalog::parse_fact`. A malformed entry is diagnosed
    /// here at lift (`E-DATALOG-PARSE`/`E-DATALOG-FUNCTION`) and simply
    /// omitted from this list.
    pub rel_facts: Vec<FactDecl>,
    /// Project-authored `rules:` (0.3.0 draft §7.1), each string parsed via
    /// `lute_syntax::datalog::parse_rule`. Same omit-on-error discipline as
    /// [`Self::rel_facts`].
    pub rel_rules: Vec<RuleDecl>,
    /// dsl 0.24 T3-6: the head relation of every `rules:` entry that failed
    /// to parse (when its head is still readable) — see
    /// `RelVocab::unparsed_heads`.
    pub rel_rule_failed_heads: std::collections::BTreeSet<String>,
    /// dsl 0.24.0 §3: entity-indexed state families this document declares —
    /// `run.approval: { …, per: companion }` maps `run.approval` →
    /// `companion`. Each member's path (`run.approval.isolde`, …) is an
    /// ordinary [`Self::state`] decl; this map is what lets a rule `cel()`
    /// guard read `run.approval[P]` for a rule variable `P`.
    pub state_index: BTreeMap<String, String>,
    /// dsl 0.28.0: `per:` families over a kind this document does not
    /// declare, expanded against the merged kinds ([`expand_per_pending`]).
    pub per_pending: Vec<PendingPer>,
    /// dsl 0.23.0 §7: a schema document's `cast:` — declared speaker ids
    /// (and display names), in key order. Legal only on `MetaKind::Schema`.
    pub cast: Vec<lute_manifest::schema::CastMember>,
    /// dsl 0.24.0 §1: a schema document's `clock:`, shape-checked. Legal
    /// only on `MetaKind::Schema`; its paths are checked against the folded
    /// schema by [`crate::clock::check_clock`].
    pub clock: Option<lute_manifest::clock::ClockDecl>,
    /// dsl 0.27.0 §4 (T2-4): a schema document's `terminal:` — the
    /// condition under which the game is over and the engine raises no
    /// occasion, and whether that ending outlives runs on purpose. Legal
    /// only on `MetaKind::Schema`; parsed by [`crate::gates::parse_terminal`].
    pub terminal: Option<crate::gates::TerminalDecl>,
    /// dsl 0.27.0 §5: a schema document's `seasons:` (name -> `{ live }`).
    pub seasons: crate::season::Seasons,
}

impl TypedMeta {
    /// Parsed authored frontmatter, retained as the one source of truth for
    /// project-wide passes that need keys not lifted into a typed field.
    pub fn yaml(&self) -> Option<&serde_yaml::Value> {
        self.yaml.as_ref()
    }
}

