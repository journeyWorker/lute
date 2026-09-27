//! Merged relational vocabulary (`RelVocab`) + schema validation (dsl 0.3.0
//! §3/§4). Builds the ONE per-document merged vocabulary — schema imports
//! (`SchemaImports.rel`, Task 6) unioned with this document's own inline
//! `entities:`/`relations:`/`enums:`/`facts:`/`rules:` (Task 5) — validates
//! every declaration, checks every seed `facts:` entry, and exposes
//! [`check_atom`]: the ONE atom/pattern closure checker shared by seeds
//! (here), rule atoms (Task 8), `::assert`/`::retract` writes (Task 10), and
//! CEL fact queries (Task 11). Every rule atoms/writes/queries diagnostic
//! that reduces to "is this a legal use of a declared relation" MUST go
//! through [`check_atom`] rather than re-implementing the closure.
//!
//! D1 applies throughout: every check here is a syntactic/graph property of
//! the DECLARED schema — no evaluation, no fixpoint.

use std::collections::{BTreeMap, BTreeSet};

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::relations::{
    EntityKindDecl, KindShape, ParsedKinds, ParsedRelations, RelationDecl,
};
use lute_manifest::snapshot::Domain;
use lute_syntax::ast::Meta;
use lute_syntax::datalog::{FactArg, FactTerm};

use crate::meta::{meta_key_span, FactDecl, RuleDecl, TypedMeta};
use crate::schema_import::{
    kind_shape_mismatch, missing_members, relation_sig_diff, SchemaImports,
};

/// The document's merged, validated relational vocabulary (spec §3/§4):
/// schema imports (`SchemaImports.rel`) unioned with this document's own
/// inline decls, via [`build_rel_vocab`].
#[derive(Clone, Debug, Default)]
pub struct RelVocab {
    pub kinds: BTreeMap<String, EntityKindDecl>,
    pub enums: BTreeMap<String, Vec<String>>,
    pub relations: BTreeMap<String, RelationDecl>,
    pub facts: Vec<FactDecl>,
    pub rules: Vec<RuleDecl>,
    /// `derive:true` relations whose rule closure contains a CEL guard in ANY
    /// feeding stratum (spec §6) — filled by `datalog_check` (Task 9); empty
    /// until then.
    pub guard_tainted: BTreeSet<String>,
    /// dsl 0.24.0 §3: entity-indexed state families (`run.approval` → its
    /// `per:` kind `companion`), imports ∪ this document. A rule `cel()`
    /// guard reads `run.approval[P]` for a rule variable `P` only through
    /// one of these ([`crate::rule_index`]).
    pub indexed_state: BTreeMap<String, String>,
    /// dsl 0.24 T3-6: where each IMPORTED declaration lives (a name declared
    /// or redeclared inline by this document is absent), so a diagnostic
    /// about it is reported once, at the schema's own line
    /// ([`at_origin`]), instead of at `1:1` of every importer.
    pub origins: DeclOrigins,
    /// dsl 0.24 T3-6: relations heading a `rules:` entry that failed to
    /// parse (`E-DATALOG-PARSE`/`-FUNCTION`). Their derivation is unknown, so
    /// they draw no `W-DERIVE-NO-RULES` and are unbounded in the fact
    /// envelope (no emptiness verdict cascades from the parse error).
    pub unparsed_heads: BTreeSet<String>,
    /// dsl 0.27.0 §4: the plugin directives declaring fact effects
    /// (`effects.asserts` / `retracts`), by tag — what resolves a call such
    /// as `::give{item="brassKey"}` to the facts it writes
    /// ([`crate::directive_facts`]). Folded from the snapshot in `fold_env`.
    pub effect_directives: BTreeMap<String, lute_manifest::schema::DirectiveDecl>,
    /// dsl 0.27.0 §4: where each of those directives' fact effects is
    /// written in its plugin file ([`PluginOrigins::effect_facts`]) — a
    /// fault a call does not bind is reported there, once. Folded from the
    /// imports in `fold_env`.
    pub effect_origins: BTreeMap<String, DeclOrigin>,
}

/// One imported declaration's home (dsl 0.24 T3-6): the schema file and the
/// declaration's span IN THAT FILE (line/column already positioned).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeclOrigin {
    pub file: std::path::PathBuf,
    pub span: Span,
}

/// Imported declarations' homes, by kind of declaration (dsl 0.24 T3-6).
/// `rules`/`facts` are keyed by their authored text (`RuleDecl::raw`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeclOrigins {
    pub relations: BTreeMap<String, DeclOrigin>,
    pub kinds: BTreeMap<String, DeclOrigin>,
    pub defs: BTreeMap<String, DeclOrigin>,
    pub rules: BTreeMap<String, DeclOrigin>,
    pub facts: BTreeMap<String, DeclOrigin>,
    /// Every `enums:` / `entities:` domain the schema declares.
    pub domains: BTreeMap<String, DeclOrigin>,
    /// dsl 0.26 §2.1/§2.7: every declared `state:` path.
    pub state: BTreeMap<String, DeclOrigin>,
    /// Prerelease N4: each member an entity kind's `members:` lists, keyed
    /// [`member_origin_key`] — where a duplicate `add:` names its first line.
    pub members: BTreeMap<String, DeclOrigin>,
    /// dsl 0.26.0 §2.8: each `cast:` entry, at its id key — where an
    /// advisory about a cast entry nobody speaks as is anchored.
    pub cast: BTreeMap<String, DeclOrigin>,
    /// Keys inside an entity kind — its `labels:`, each label key, and each
    /// key the kind does not take — keyed [`kind_key_origin`], so a problem
    /// with one key is reported at that key.
    pub kind_keys: BTreeMap<String, DeclOrigin>,
    /// Keys inside a relation — its `tier:` — keyed [`kind_key_origin`]
    /// (relation name, key), so a bad value is reported at its key.
    pub relation_keys: BTreeMap<String, DeclOrigin>,
    /// Each `labels:` key of an `enums:` domain, keyed [`member_origin_key`].
    pub enum_labels: BTreeMap<String, DeclOrigin>,
    /// Each member an imported `add:` brings, keyed [`member_origin_key`],
    /// at its own line in that `add:` — where a clash it causes is reported.
    pub added: BTreeMap<String, DeclOrigin>,
}

/// dsl 0.27.0 §4: plugin declarations a document's check judges, at their
/// line in the plugin's export file (see [`SchemaImports::plugin_origins`]).
///
/// [`SchemaImports::plugin_origins`]: crate::SchemaImports::plugin_origins
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PluginOrigins {
    /// Each gated occasion's `raisedWhen:` value.
    pub gates: BTreeMap<String, DeclOrigin>,
    /// Each directive fact effect, keyed [`effect_origin_key`]: the fact's
    /// text in the directive's `effects.asserts` / `retracts`.
    pub effect_facts: BTreeMap<String, DeclOrigin>,
    /// G-6: each occasion payload field, keyed [`effect_origin_key`]`(occasion,
    /// field)`: the field's key inside the occasion's `payload:`.
    pub payloads: BTreeMap<String, DeclOrigin>,
}

/// The [`PluginOrigins::effect_facts`] key of `fact` (as displayed) in
/// directive `directive`'s effects.
pub fn effect_origin_key(directive: &str, fact: &str) -> String {
    format!("{directive}\u{1f}{fact}")
}

/// Byte offset (in `yaml`) of the `<id>:` key of `id`'s entry under the
/// top-level `cast:` map of a schema or a plugin cast export — a line scan,
/// never a YAML re-parse; `None` when the shape is not recognized.
pub fn cast_entry_offset(yaml: &str, id: &str) -> Option<usize> {
    map_entry_offset(yaml, "cast", id)
}

/// Byte offset (in `yaml`) of the `<id>:` key of `id`'s entry under the
/// top-level `<top>:` map (`cast`, `occasions`) — a line scan, never a
/// YAML re-parse; `None` when the shape is not recognized.
pub fn map_entry_offset(yaml: &str, top: &str, id: &str) -> Option<usize> {
    let header = format!("{top}:");
    // `Some(indent of the entry keys)` inside `<top>:`, once the first
    // entry is seen; `Some(None)` right after the `<top>:` line.
    let mut in_map: Option<Option<usize>> = None;
    let mut off = 0;
    for line in yaml.split_inclusive('\n') {
        let at = off;
        off += line.len();
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = line.len() - trimmed.len();
        if indent == 0 {
            in_map = trimmed.starts_with(&header).then_some(None);
            continue;
        }
        let Some(entry_indent) = in_map else {
            continue;
        };
        let want = entry_indent.unwrap_or(indent);
        if want != indent {
            continue;
        }
        in_map = Some(Some(want));
        let key = trimmed
            .strip_prefix(['"', '\''])
            .unwrap_or(trimmed)
            .strip_prefix(id);
        if key.is_some_and(|rest| {
            rest.trim_start_matches(['"', '\''])
                .trim_start()
                .starts_with(':')
        }) {
            return Some(at + indent);
        }
    }
    None
}

/// [`cast_entry_offset`] of `id` in a schema's frontmatter, as a line-less
/// `meta`-document span (like [`kind_list_spans`]').
pub(crate) fn cast_entry_span(meta: &Meta, id: &str) -> Option<Span> {
    let (base, _) = frontmatter_base(meta);
    cast_entry_offset(&meta.raw_yaml, id).map(|o| Span {
        byte_start: base + o,
        byte_end: base + o + id.len(),
        line: 0,
        column: 0,
        utf16_range: (0, 0),
    })
}

/// The [`DeclOrigins::members`] key of `member` of entity kind `kind`.
pub fn member_origin_key(kind: &str, member: &str) -> String {
    format!("{kind}\u{1f}{member}")
}

/// The [`DeclOrigins::kind_keys`] key of the key at `path` inside entity
/// kind `kind` (`["labels"]`, `["labels", member]`, `["lables"]`).
pub fn kind_key_origin(kind: &str, path: &[&str]) -> String {
    let mut key = kind.to_string();
    for seg in path {
        key.push('\u{1f}');
        key.push_str(seg);
    }
    key
}

/// The key at `path` inside entity kind `kind` in `meta`'s frontmatter
/// ([`crate::meta::meta_path_span`]: the deepest key of the path written).
pub(crate) fn kind_key_span(meta: &Meta, kind: &str, path: &[&str]) -> Option<Span> {
    let mut full = vec!["entities", kind];
    full.extend_from_slice(path);
    Some(crate::meta::meta_path_span(meta, &full))
}

/// dsl 0.24 T3-6: re-home a diagnostic about an IMPORTED declaration. The
/// importer carries it at its frontmatter start (byte 0, like
/// `E-USES-PARSE`) with the schema file named, and the original — at the
/// declaration's own line in that file — as its `related` entry; the project
/// roll-up then folds the identical copies every importer carries into one
/// report. `None` (a local declaration) returns `d` unchanged.
pub fn at_origin(d: Diagnostic, origin: Option<&DeclOrigin>) -> Diagnostic {
    rehome(d, origin, "schema import")
}

/// dsl 0.27.0 §4: [`at_origin`] for a declaration an installed plugin makes
/// (an occasion's gate, a directive's fact effect) — reported once, at the
/// plugin file's line, instead of at every use.
pub fn at_plugin_origin(d: Diagnostic, origin: Option<&DeclOrigin>) -> Diagnostic {
    rehome(d, origin, "plugin file")
}

/// [`at_origin`] for a default the project manifest supplies (its
/// `defaults.questTier`) — reported once, at the manifest's key.
pub fn at_manifest_origin(d: Diagnostic, origin: Option<&DeclOrigin>) -> Diagnostic {
    rehome(d, origin, "project manifest")
}

fn rehome(d: Diagnostic, origin: Option<&DeclOrigin>, what: &str) -> Diagnostic {
    let Some(origin) = origin else {
        return d;
    };
    let file = origin.file.display().to_string();
    let mut inner = d.clone();
    inner.span = origin.span;
    Diagnostic {
        // The file NAME, not the importer-relative path: the message is the
        // roll-up key, and importers in different directories must fold into
        // one report; the `related` entry carries the full location.
        message: format!(
            "{} (declared in {what} `{}`)",
            d.message,
            origin
                .file
                .file_name()
                .map_or(file.clone(), |n| n.to_string_lossy().into_owned())
        ),
        span: Span {
            byte_start: 0,
            byte_end: 0,
            line: 0,
            column: 0,
            utf16_range: (0, 0),
        },
        related: vec![lute_core_span::RelatedDiagnostic {
            file,
            diagnostic: inner,
        }],
        ..d
    }
}

impl RelVocab {
    /// Effective tier of a base relation (default `run`, spec §4). `None` for
    /// `derive:true` (a derived relation has no write tier). The output
    /// lifetime ties to `rel`, not `self` — a caller may hold this past a
    /// `RelVocab` borrow as long as it holds the `RelationDecl` borrow.
    pub fn tier_of<'a>(&self, rel: &'a RelationDecl) -> Option<&'a str> {
        if rel.derive {
            None
        } else {
            Some(rel.tier.as_deref().unwrap_or("run"))
        }
    }

    /// dsl 0.25.0 §1: relations `a` and `b` can never hold together on the
    /// same arguments (`excludes:`, closed symmetrically).
    pub fn excludes(&self, a: &str, b: &str) -> bool {
        lute_manifest::relations::relations_exclude(&self.relations, a, b)
    }
}

pub const E_ENTITY_KIND_SHAPE: &str = "E-ENTITY-KIND-SHAPE"; // §3.1
pub const E_ENTITY_KIND_CLASH: &str = "E-ENTITY-KIND-CLASH"; // §3.1
pub const E_KIND_NAME_CLASH: &str = "E-KIND-NAME-CLASH"; // §4
pub const E_RELATION_DUP: &str = "E-RELATION-DUP"; // §4
pub const E_RELATION_EMPTY: &str = "E-RELATION-EMPTY"; // §4
pub const E_RELATION_DOMAIN: &str = "E-RELATION-DOMAIN"; // §4
pub const E_RELATION_UNKNOWN: &str = "E-RELATION-UNKNOWN"; // §4
pub const E_RELATION_ARITY: &str = "E-RELATION-ARITY"; // §4/§5/§7/§8
pub const E_FACT_DOMAIN: &str = "E-FACT-DOMAIN"; // §3.1/§5
pub const E_DERIVE_TIER: &str = "E-DERIVE-TIER"; // §4/§7.1
pub const E_RELATION_RESERVED_WRITE: &str = "E-RELATION-RESERVED-WRITE"; // §4/§5
pub const E_RETRACT_WILDCARD_ASSERT: &str = "E-RETRACT-WILDCARD-ASSERT"; // §5
pub const E_EXTENDS_RELATION_SIG: &str = "E-EXTENDS-RELATION-SIG"; // §4.1

/// dsl 0.25.0 §1/§6: a relation declaration whose `excludes` names a
/// relation of other argument kinds, or whose `changedOn` sits on a relation
/// that is not engine-`reserved` or names an undeclared occasion.
pub const E_RELATION_DECL: &str = "E-RELATION-DECL";

/// dsl 0.25.0 §1 (LH N17): a rule whose head relation excludes a relation its
/// positive body requires on the head's arguments — every firing makes both
/// hold.
pub const E_RULE_EXCLUSIVE: &str = "E-RULE-EXCLUSIVE";

/// dsl 0.28.0 §5: a stored (non-`derive`) relation declared without
/// `tier:` — it is run-tier by default, so its facts start over at every new
/// run (from its `facts:` seed, if any), which an engine-owned (`reserved`)
/// relation in particular rarely means.
pub const W_RELATION_TIER_IMPLICIT: &str = "W-RELATION-TIER-IMPLICIT";

/// dsl 0.28.0 (T3-45): one name declared both as an `enums:` domain and an
/// entity kind (in one document or across the schemas it merges).
pub const E_DOMAIN_NAME_CLASH: &str = "E-DOMAIN-NAME-CLASH";

/// [`E_DOMAIN_NAME_CLASH`]'s message: both member lists, and what the clash
/// does.
pub(crate) fn domain_clash_message(
    name: &str,
    enum_members: &[String],
    kind: &KindShape,
) -> String {
    let kind_members = match kind {
        KindShape::Members(ms) => format!(" [{}]", ms.join(", ")),
        _ => String::new(),
    };
    format!(
        "`{name}` is declared as both an enum [{}] and an entity kind{kind_members} — enums and \
         entity kinds share one domain namespace, so the kind's members would replace the enum's \
         wherever `{name}` types a value; rename one of them",
        enum_members.join(", ")
    )
}

/// Build a `Layer::Logic` error diagnostic — rel_schema.rs's checks are
/// schema/graph-level (Global Constraints' layer table).
fn diag(code: &str, message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity: Severity::Error,
        message,
        span,
        layer: Layer::Logic,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

/// Per-declaration-set structural validation (§3.1/§4). Called for the
/// INLINE decls in [`build_rel_vocab`] and, per imported file, from
/// `schema_import::resolve_imports` — so a malformed decl is diagnosed
/// wherever it is declared, not only when that file happens to be checked
/// directly. `span_of` locates a declared name; `key_at(kind, path)` the key
/// at `path` inside entity kind `kind` (`["labels", member]`, or a key of the
/// kind itself), when it can be found — a problem with one key is reported
/// there, not at the kind. `rel_key_at(relation, key)` likewise locates a key
/// inside a relation (its `tier:`).
pub fn validate_rel_decls(
    kinds: &ParsedKinds,
    rels: &ParsedRelations,
    span_of: &dyn Fn(&str) -> Span,
    key_at: &dyn Fn(&str, &[&str]) -> Option<Span>,
    rel_key_at: &dyn Fn(&str, &str) -> Option<Span>,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for (name, decl) in &kinds.kinds {
        if matches!(decl.shape, KindShape::Invalid) {
            out.push(diag(
                E_ENTITY_KIND_SHAPE,
                format!(
                    "entity kind `{name}` must declare exactly one of `members:` or `open:` \
                     (`subsetOf:` and `labels:` may sit beside it), or be an `add:` list that \
                     extends a kind another schema declares, with at most `labels:` beside the \
                     `add:` (dsl 0.3.0 §3.1, 0.26.0 §2.3, 0.27.0 §7)"
                ),
                span_of(name),
            ));
        }
    }
    for (name, key) in &kinds.unknown_keys {
        let hint = lute_manifest::suggest::did_you_mean(
            key,
            lute_manifest::relations::ENTITY_KIND_KEYS.iter().copied(),
        );
        out.push(diag(
            E_ENTITY_KIND_SHAPE,
            format!(
                "entity kind `{name}` has an unknown key `{key}:` (an entity kind takes \
                 `members:`, `open:`, `add:`, `subsetOf:` and `labels:`){hint} (dsl 0.27.0 §2)"
            ),
            key_at(name, &[key]).unwrap_or_else(|| span_of(name)),
        ));
    }
    out.extend(check_kind_labels(kinds, span_of, key_at));
    for name in &kinds.dups {
        out.push(diag(
            E_KIND_NAME_CLASH,
            format!(
                "entity kind `{name}` is declared more than once in this `entities:` block (dsl 0.3.0 §4)"
            ),
            span_of(name),
        ));
    }
    for name in &rels.dups {
        out.push(diag(
            E_RELATION_DUP,
            format!(
                "relation `{name}` is declared more than once in this `relations:` block (dsl 0.3.0 §4)"
            ),
            span_of(name),
        ));
    }
    for (name, decl) in &rels.relations {
        // dsl 0.28.0 §1: the one reserved-names table.
        if let Some(r) =
            lute_manifest::reserved::refusal(lute_manifest::reserved::Slot::Relation, name)
        {
            out.push(diag(
                crate::reserved_names::E_RESERVED_NAME,
                r.message("a relation"),
                span_of(name),
            ));
        }
        if decl.args.is_empty() {
            out.push(diag(
                E_RELATION_EMPTY,
                format!("relation `{name}` declares no `args:` (dsl 0.3.0 §4)"),
                span_of(name),
            ));
        }
        for field in &decl.malformed_fields {
            let keys = lute_manifest::relations::RELATION_KEYS;
            let message = if keys.contains(&field.as_str()) {
                format!("relation `{name}` field `{field}` has a value of the wrong shape")
            } else {
                let listed: Vec<String> = keys.iter().map(|k| format!("`{k}`")).collect();
                format!(
                    "relation `{name}` has no key `{field}`{}; a relation takes {}",
                    lute_manifest::suggest::did_you_mean(field, keys.iter().copied()),
                    listed.join(", ")
                )
            };
            out.push(diag(E_RELATION_DOMAIN, message, span_of(name)));
        }
        if let Some(tier) = &decl.tier {
            // dsl 0.28.0 §5 (T2-7): `season:<name>` too, as a quest's `tier=`
            // and a beat's `once:` spell it; the season's existence is
            // checked with the seasons (`crate::season::check_relation_tiers`).
            const TIERS: [&str; 5] = ["scene", "run", "user", "app", "quest"];
            let season = lute_manifest::season::season_ref(tier)
                .is_some_and(lute_manifest::season::is_season_name);
            if !TIERS.contains(&tier.as_str()) && !season {
                // A state-path spelling (`season.lanterns`, `run.x`) names
                // the tier it starts with.
                let hint = match tier.split_once('.') {
                    Some(("season", s)) if lute_manifest::season::is_season_name(s) => {
                        format!(" — did you mean `season:{s}`?")
                    }
                    Some((ns, _)) if TIERS.contains(&ns) => format!(" — did you mean `{ns}`?"),
                    _ => lute_manifest::suggest::did_you_mean(tier, TIERS),
                };
                out.push(diag(
                    E_RELATION_DOMAIN,
                    format!(
                        "relation `{name}` has unknown `tier: {tier}`{hint} (expected one of \
                         scene, run, user, app, quest, or season:<name>)",
                    ),
                    rel_key_at(name, "tier").unwrap_or_else(|| span_of(name)),
                ));
            }
            if decl.derive {
                out.push(diag(
                    E_DERIVE_TIER,
                    format!(
                        "relation `{name}` is `derive: true` but also declares `tier:`; a derived relation has no write tier (dsl 0.3.0 §4/§7.1)"
                    ),
                    span_of(name),
                ));
            }
        }
        // A misspelt `teir:` is already an error that names `tier`: the
        // warning would tell the author the relation has none.
        let tier_misspelt = decl.malformed_fields.iter().any(|f| {
            f == "tier"
                || lute_manifest::suggest::nearest(
                    f,
                    lute_manifest::relations::RELATION_KEYS.iter().copied(),
                    2,
                ) == Some("tier")
        });
        if decl.tier.is_none() && !decl.derive && !tier_misspelt {
            let owner = if decl.reserved {
                "at every new run the engine's facts in it are forgotten"
            } else {
                "at every new run its facts start over from its `facts:` seed (if any), \
                 forgetting what the run asserted or retracted"
            };
            out.push(Diagnostic {
                severity: Severity::Warning,
                ..diag(
                    W_RELATION_TIER_IMPLICIT,
                    format!(
                        "relation `{name}` has no `tier:`, so it is run-tier: {owner}; write \
                         `tier: run` to keep that, or `tier: user` (or `season:<name>`) for facts \
                         that outlive the run"
                    ),
                    span_of(name),
                )
            });
        }
        if decl.derive && decl.reserved {
            out.push(diag(
                E_RELATION_RESERVED_WRITE,
                format!(
                    "relation `{name}` is both `derive: true` and `reserved: true`; a relation may not have two conflicting write owners (dsl 0.3.0 §4/§5)"
                ),
                span_of(name),
            ));
        }
        if !decl.changed_on.is_empty() && !decl.reserved {
            out.push(diag(
                E_RELATION_DECL,
                format!(
                    "relation `{name}` declares `changedOn:` but is not `reserved: true`; only the \
                     engine changes a relation on an occasion — script writes are ordered by the \
                     scenario already (dsl 0.25.0 §6)"
                ),
                span_of(name),
            ));
        }
        if !decl.key.is_empty() {
            let n = decl.args.len() as i64;
            let mut seen = BTreeSet::new();
            let mut bad = false;
            for &k in &decl.key {
                if k < 0 || k >= n {
                    bad = true;
                }
                if !seen.insert(k) {
                    bad = true;
                }
            }
            if bad {
                out.push(diag(
                    E_RELATION_DOMAIN,
                    format!(
                        "relation `{name}` declares an out-of-range or duplicate `key:` index (dsl 0.3.0 §4)"
                    ),
                    span_of(name),
                ));
            }
        }
    }
    out
}

/// dsl 0.27.0 §7: `labels:` of an `entities:` block — each shape mistake the
/// parse kept ([`ParsedKinds::label_problems`]), a label on an `open:` kind
/// (the engine mints its members, so none can be named here), and a label
/// for an id the kind does not have (its own members and those of its
/// sub-kinds in the same block; beside `add:`, the members that list adds) —
/// each `E-ENTITY-KIND-SHAPE`, the last with a did-you-mean.
fn check_kind_labels(
    kinds: &ParsedKinds,
    span_of: &dyn Fn(&str) -> Span,
    key_at: &dyn Fn(&str, &[&str]) -> Option<Span>,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for (name, problem) in &kinds.label_problems {
        out.push(diag(
            E_ENTITY_KIND_SHAPE,
            format!("entity kind `{name}`: {problem}"),
            key_at(name, &["labels"]).unwrap_or_else(|| span_of(name)),
        ));
    }
    let mut closed = kinds.kinds.clone();
    lute_manifest::relations::imply_sub_kind_members(&mut closed, &kinds.order);
    // Each label key that is not a member, at the key itself. Beside an
    // `add:` the members are only those that `add:` brings: the kind's own
    // members are labelled where the kind is declared.
    let not_members = |name: &str, labels: Vec<&String>, members: &[String], beside_add: bool| {
        labels
            .into_iter()
            .filter(|member| !members.contains(member))
            .map(|member| {
                let hint = lute_manifest::suggest::did_you_mean(
                    member,
                    members.iter().map(String::as_str),
                );
                let message = if beside_add {
                    let added = members
                        .iter()
                        .map(|m| format!("`{m}`"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!(
                        "entity kind `{name}` labels `{member}` beside its `add:`, but labels \
                         there cover only the members that `add:` brings ({added}){hint}; \
                         label any other `{name}` member in the schema that declares `{name}`"
                    )
                } else {
                    format!(
                        "entity kind `{name}` labels `{member}`, which is not one of its \
                         members{hint}"
                    )
                };
                diag(
                    E_ENTITY_KIND_SHAPE,
                    message,
                    key_at(name, &["labels", member]).unwrap_or_else(|| span_of(name)),
                )
            })
            .collect::<Vec<_>>()
    };
    for (name, decl) in &kinds.kinds {
        match closed.get(name).map(|d| &d.shape) {
            Some(KindShape::Members(members)) => out.extend(not_members(
                name,
                decl.labels.keys().collect(),
                members,
                false,
            )),
            Some(KindShape::Open) if !decl.labels.is_empty() => out.push(diag(
                E_ENTITY_KIND_SHAPE,
                format!(
                    "entity kind `{name}` is `open:` — the engine registers its members, so \
                     `labels:` cannot name them; label a kind that lists its `members:`"
                ),
                key_at(name, &["labels"]).unwrap_or_else(|| span_of(name)),
            )),
            _ => {}
        }
    }
    for (name, labels) in &kinds.add_labels {
        out.extend(not_members(
            name,
            labels.keys().collect(),
            kinds.adds.get(name).map_or(&[], Vec::as_slice),
            true,
        ));
    }
    out
}

/// dsl 0.25.0 §6: `E-RELATION-DECL` for every `changedOn:` entry of the
/// merged `vocab` that names no declared occasion (with a did-you-mean),
/// reported where the relation is declared — an imported one at its schema
/// line ([`at_origin`]). Silent when no occasion is declared (a project-less
/// check has no occasion vocabulary, D-F), like every occasion-name check.
pub fn check_changed_on(
    vocab: &RelVocab,
    occasions: &BTreeMap<String, lute_manifest::schema::OccasionDecl>,
    meta: &Meta,
) -> Vec<Diagnostic> {
    if occasions.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (name, decl) in &vocab.relations {
        for occasion in decl
            .changed_on
            .iter()
            .filter(|o| !occasions.contains_key(*o))
        {
            let hint =
                lute_manifest::suggest::nearest(occasion, occasions.keys().map(String::as_str), 2)
                    .map_or_else(String::new, |near| format!(" — did you mean `{near}`?"));
            out.push(at_origin(
                diag(
                    E_RELATION_DECL,
                    format!(
                        "relation `{name}` `changedOn: {occasion}` is not a declared occasion{hint} \
                         (dsl 0.25.0 §6)"
                    ),
                    meta_key_span(meta, name),
                ),
                vocab.origins.relations.get(name),
            ));
        }
    }
    out
}

/// The pure `enums:` names this document declares inline. `typed.domains`
/// (dsl §3/A3) mixes project `enums:` and `entities:` projections with
/// entities winning a same-name clash (`TypedMeta::domains`'s doc comment);
/// subtracting `typed.rel_kinds.kinds`'s keys recovers exactly the `enums:`
/// names — the SAME trick `schema_import::resolve_imports` uses for each
/// imported file's `ParsedDoc::domains`.
fn inline_enums(typed: &TypedMeta) -> BTreeMap<String, Vec<String>> {
    typed
        .domains
        .iter()
        .filter(|(name, _)| !typed.rel_kinds.kinds.contains_key(*name))
        .map(|(name, dom)| (name.clone(), dom.members.clone()))
        .collect()
}

/// Merge `imports.rel` with this document's inline decls, run the merged-
/// vocabulary checks (§3.1/§4), and validate seed `facts:` (incl. D12).
/// `domains` = `merge_domains` output (plugin/core ∪ project).
pub fn build_rel_vocab(
    imports: &SchemaImports,
    typed: &TypedMeta,
    domains: &BTreeMap<String, Domain>,
    meta: &Meta,
) -> (RelVocab, Vec<Diagnostic>) {
    let mut diags = Vec::new();
    let span_of = |name: &str| meta_key_span(meta, name);

    // Start from the resolved imports, overlay this document's inline decls.
    // An inline redeclaration of an imported name uses the SAME D5
    // growth/full-match rule Task 6 uses for an `extends` child vs its base
    // (regardless of whether the import edge was `uses:` or `extends:` — by
    // the time it reaches here, `imports.rel` is already the DAG-resolved
    // vocabulary, and an inline decl always plays the "child" role over it).
    let mut kinds = imports.rel.kinds.clone();
    for (name, decl) in &typed.rel_kinds.kinds {
        if let Some(base) = kinds.get(name) {
            if let Some(msg) = kind_shape_mismatch(&decl.shape, &base.shape) {
                diags.push(diag(
                    E_EXTENDS_RELATION_SIG,
                    format!(
                        "entity kind `{name}` {msg}; an inline re-declaration must re-declare a superset of the imported base's members (dsl 0.3.0 §4.1)"
                    ),
                    span_of(name),
                ));
            }
        }
        kinds.insert(name.clone(), decl.clone());
    }
    // dsl 0.26.0 §2.3: this document's own `add:`s extend the kind one
    // import declares (imported `add:`s were merged in `resolve_imports`).
    let inline_adds: Vec<KindAdd> = typed
        .rel_kinds
        .adds
        .iter()
        .map(|(kind, members)| KindAdd {
            kind: kind.clone(),
            members: members.clone(),
            member_spans: add_member_spans(meta, kind, members)
                .into_iter()
                .map(|s| s.map(|s| meta_position(meta, s)))
                .collect(),
            origin: None,
            span: span_of(kind),
            labels: typed
                .rel_kinds
                .add_labels
                .get(kind)
                .cloned()
                .unwrap_or_default(),
        })
        .collect();
    diags.extend(apply_kind_adds(&mut kinds, &inline_adds, &|kind| {
        kind_home(&imports.rel.origins, kind)
    }));

    let mut enums = imports.rel.enums.clone();
    for (name, members) in inline_enums(typed) {
        if let Some(base_members) = enums.get(&name) {
            let missing = missing_members(&members, base_members);
            if !missing.is_empty() {
                diags.push(diag(
                    E_EXTENDS_RELATION_SIG,
                    format!(
                        "enum `{name}` is missing base member(s) {missing:?}; an inline re-declaration must re-declare a superset of the imported base's members (dsl 0.3.0 §4.1)"
                    ),
                    span_of(&name),
                ));
            }
        }
        enums.insert(name, members);
    }

    let mut relations = imports.rel.relations.clone();
    for (name, decl) in &typed.rel_relations.relations {
        if let Some(base) = relations.get(name) {
            let diff = relation_sig_diff(decl, base);
            if !diff.is_empty() {
                diags.push(diag(
                    E_EXTENDS_RELATION_SIG,
                    format!(
                        "relation `{name}` re-declaration differs from its imported base in {}; a re-declared relation must match the full base decl (dsl 0.3.0 §4.1)",
                        diff.join(", ")
                    ),
                    span_of(name),
                ));
            }
        }
        relations.insert(name.clone(), decl.clone());
    }

    // Structural validation of the INLINE decls (imported-file decls are
    // validated where they are declared, inside `resolve_imports`, 0.3.0 T7).
    diags.extend(validate_rel_decls(
        &typed.rel_kinds,
        &typed.rel_relations,
        &span_of,
        &|kind, path| kind_key_span(meta, kind, path),
        &|rel, key| Some(crate::meta::meta_path_span(meta, &["relations", rel, key])),
    ));
    diags.extend(check_member_dups(
        meta,
        &typed.rel_kinds,
        &inline_enums(typed),
    ));

    // dsl 0.24 T3-6: the imported declarations' homes, minus every name this
    // document (re)declares inline — a merged-check diagnostic about an
    // imported declaration is reported at the schema's line, once.
    let mut origins = imports.rel.origins.clone();
    for name in typed.rel_kinds.kinds.keys() {
        origins.kinds.remove(name);
    }
    for name in typed.rel_relations.relations.keys() {
        origins.relations.remove(name);
    }
    for name in typed.defs.keys() {
        origins.defs.remove(name);
    }
    for name in typed.domains.keys() {
        origins.domains.remove(name);
    }
    for path in typed.state.decls.keys() {
        origins.state.remove(path);
    }
    for r in &typed.rel_rules {
        origins.rules.remove(&r.raw);
    }
    for f in &typed.rel_facts {
        origins.facts.remove(&f.raw);
    }

    // Merged check (a): every relation arg domain name must resolve to a
    // declared entity kind, enum, plugin/core domain, or `bool` — else
    // E-RELATION-DOMAIN (D4 residual bucket). Runs over the FULL merged set
    // since a name may only resolve once cross-file imports are unioned in.
    for (name, decl) in &relations {
        for arg in &decl.args {
            if arg.as_str() == "bool"
                || kinds.contains_key(arg)
                || enums.contains_key(arg)
                || domains.contains_key(arg)
            {
                continue;
            }
            diags.push(at_origin(
                diag(
                    E_RELATION_DOMAIN,
                    format!(
                        "relation `{name}` argument domain `{arg}` is not a declared entity kind, enum, or domain (dsl 0.3.0 §4)"
                    ),
                    span_of(arg),
                ),
                origins.relations.get(name),
            ));
        }
    }

    // Merged check (a2), dsl 0.25.0 §1: every `excludes:` entry names a
    // declared relation other than itself, with the same argument kinds —
    // else E-RELATION-DECL (the partner may live in an imported schema).
    for (name, decl) in &relations {
        for other in &decl.excludes {
            let problem = match relations.get(other) {
                _ if other == name => "a relation cannot exclude itself".to_string(),
                None => {
                    let hint = lute_manifest::suggest::nearest(
                        other,
                        relations
                            .keys()
                            .map(String::as_str)
                            .filter(|n| *n != name.as_str()),
                        2,
                    )
                    .map(|s| format!(" — did you mean `{s}`?"))
                    .unwrap_or_default();
                    format!("`{other}` is not a declared relation{hint}")
                }
                Some(od) if od.args != decl.args => format!(
                    "`{other}` takes [{}] but `{name}` takes [{}]; excluded relations must \
                     have the same argument kinds",
                    od.args.join(", "),
                    decl.args.join(", ")
                ),
                Some(_) => continue,
            };
            diags.push(at_origin(
                diag(
                    E_RELATION_DECL,
                    format!("relation `{name}` `excludes: [{other}]`: {problem} (dsl 0.25.0 §1)"),
                    span_of(name),
                ),
                origins.relations.get(name),
            ));
        }
    }

    // Merged check (b): a kind name and a relation name share one rule-body
    // predicate namespace (§4) — a name declared as both is E-KIND-NAME-CLASH.
    for name in kinds.keys() {
        if relations.contains_key(name) {
            diags.push(at_origin(
                diag(
                    E_KIND_NAME_CLASH,
                    format!(
                        "`{name}` is declared as both an entity kind and a relation; kinds and relations share one predicate namespace (dsl 0.3.0 §4)"
                    ),
                    span_of(name),
                ),
                origins.relations.get(name).filter(|_| origins.kinds.contains_key(name)),
            ));
        }
    }

    // dsl 0.28.0 (T3-45): an enum and an entity kind share one domain
    // namespace — the kind's members would silently replace the enum's
    // wherever the name types a value.
    for (name, members) in &enums {
        let Some(kind) = kinds.get(name) else {
            continue;
        };
        diags.push(at_origin(
            diag(
                E_DOMAIN_NAME_CLASH,
                domain_clash_message(name, members, &kind.shape),
                span_of(name),
            ),
            origins.kinds.get(name),
        ));
    }

    // Merged check (c): one-id-one-kind (§3.1) — an id in TWO closed kinds'
    // `members:` is E-ENTITY-KIND-CLASH, unless the two kinds share a root:
    // a dsl 0.24.0 §3 sub-kind (`subsetOf:`) re-lists members of its parent,
    // and two sub-kinds of one parent may overlap.
    let closed: Vec<(&String, &Vec<String>)> = kinds
        .iter()
        .filter_map(|(name, decl)| match &decl.shape {
            KindShape::Members(members) => Some((name, members)),
            _ => None,
        })
        .collect();
    // Where `kind` gets member `id`: the file writing it (`None`: this
    // document), the span there, and whether an `add:` brought it.
    let listing = |kind: &str, id: &str| -> (Option<DeclOrigin>, Span, bool) {
        let key = member_origin_key(kind, id);
        if typed.rel_kinds.kinds.contains_key(kind) {
            let span = kind_list_spans(meta, kind, "members:")
                .into_iter()
                .find(|(m, _)| m == id)
                .map_or_else(|| span_of(kind), |(_, s)| s);
            return (None, meta_position(meta, span), false);
        }
        if let Some(add) = inline_adds.iter().find(|a| a.kind == kind) {
            if let Some(i) = add.members.iter().position(|m| m == id) {
                let span = add.member_spans.get(i).copied().flatten();
                return (None, span.unwrap_or(add.span), true);
            }
        }
        if let Some(o) = origins.added.get(&key) {
            return (Some(o.clone()), o.span, true);
        }
        match origins
            .members
            .get(&key)
            .or_else(|| origins.kinds.get(kind))
        {
            Some(o) => (Some(o.clone()), o.span, false),
            None => (None, span_of(id), false),
        }
    };
    let place = |(origin, span, _): &(Option<DeclOrigin>, Span, bool)| match origin {
        Some(o) => format!("`{}:{}`", origin_display(&o.file), o.span.line),
        None if span.line > 0 => format!("this document, line {}", span.line),
        None => "this document".to_string(),
    };
    for i in 0..closed.len() {
        for j in (i + 1)..closed.len() {
            let (name_a, members_a) = closed[i];
            let (name_b, members_b) = closed[j];
            if root_kind(&kinds, name_a) == root_kind(&kinds, name_b) {
                continue;
            }
            for id in members_a {
                if !members_b.contains(id) {
                    continue;
                }
                // Reported where the clash was made: an `add:` over a
                // declaration, this document over an import, else the later
                // file and line.
                let (a, b) = (listing(name_a, id), listing(name_b, id));
                let rank = |l: &(Option<DeclOrigin>, Span, bool)| {
                    (
                        l.2,
                        l.0.is_none(),
                        l.0.as_ref().map(|o| origin_display(&o.file)),
                        l.1.line,
                    )
                };
                let ((first_kind, first), (second_kind, second)) = if rank(&a) > rank(&b) {
                    ((name_b, b), (name_a, a))
                } else {
                    ((name_a, a), (name_b, b))
                };
                let message = format!(
                    "id `{id}` is a member of both entity kinds `{first_kind}` ({}) and \
                     `{second_kind}` ({}); an id belongs to exactly one kind (dsl 0.3.0 §3.1) — \
                     if every `{first_kind}` is also a `{second_kind}` (or the other way round), \
                     declare one a sub-kind of the other with `subsetOf:` (dsl 0.24.0 §3); if \
                     the `{first_kind}` and the `{second_kind}` called `{id}` are two different \
                     things, rename one of them",
                    place(&first),
                    place(&second),
                );
                let (origin, span, _) = second;
                diags.push(match origin {
                    Some(o) => at_origin(diag(E_ENTITY_KIND_CLASH, message, span_of(id)), Some(&o)),
                    None => diag(E_ENTITY_KIND_CLASH, message, span),
                });
            }
        }
    }
    // dsl 0.26.0 §2.3: a sub-kind's members are members of its parent — after
    // the one-kind check above, which reads the lists as authored. Sub-kinds
    // in declaration order: the imported ones as the imports declare them,
    // then this document's own.
    let mut order = imports.rel.kind_order.clone();
    order.extend(
        typed
            .rel_kinds
            .order
            .iter()
            .filter(|n| !imports.rel.kind_order.contains(n))
            .cloned(),
    );
    lute_manifest::relations::imply_sub_kind_members(&mut kinds, &order);
    diags.extend(check_sub_kinds(&kinds, &span_of, &origins));

    // Facts/rules always UNION (spec §4.1) — imports first, then inline.
    let mut facts = imports.rel.facts.clone();
    facts.extend(typed.rel_facts.iter().cloned());
    let mut rules = imports.rel.rules.clone();
    rules.extend(typed.rel_rules.iter().cloned());
    // dsl 0.25.0 §1 (LH N17): a rule whose head relation excludes a relation
    // its positive body requires on the head's own arguments derives, every
    // time it fires, a fact whose excluded partner holds — the rule and the
    // declaration contradict each other by construction (E-RULE-EXCLUSIVE).
    for r in &rules {
        let head = &r.rule.head;
        for lit in &r.rule.body {
            let lute_syntax::datalog::BodyLiteral::Pos(b) = lit else {
                continue;
            };
            if b.terms != head.terms
                || !lute_manifest::relations::relations_exclude(
                    &relations,
                    &head.relation,
                    &b.relation,
                )
            {
                continue;
            }
            let h = &head.relation;
            let p = &b.relation;
            diags.push(at_origin(
                diag(
                    E_RULE_EXCLUSIVE,
                    format!(
                        "rule `{}` derives `{h}` only where `{p}` holds on the same arguments, but \
                         `{h}` and `{p}` are declared exclusive — every derivation breaks the \
                         exclusion (dsl 0.25.0 §1); fix the rule or the `excludes:` declaration",
                        r.raw.trim()
                    ),
                    r.span,
                ),
                origins.rules.get(&r.raw),
            ));
        }
    }
    let mut indexed_state = imports.rel.indexed_state.clone();
    indexed_state.extend(
        typed
            .state_index
            .iter()
            .map(|(p, k)| (p.clone(), k.clone())),
    );

    let vocab = RelVocab {
        kinds,
        enums,
        relations,
        facts,
        rules,
        guard_tainted: BTreeSet::new(),
        indexed_state,
        origins,
        unparsed_heads: imports
            .rel
            .unparsed_heads
            .iter()
            .chain(&typed.rel_rule_failed_heads)
            .cloned()
            .collect(),
        effect_directives: BTreeMap::new(),
        effect_origins: BTreeMap::new(),
    };

    // Merged check (d): every seed `facts:` entry is GROUND — checked as for
    // `::assert` (spec §4), incl. D12: a `_` in a seed is
    // E-RETRACT-WILDCARD-ASSERT with a seed-specific message override.
    for f in &vocab.facts {
        let mut fdiags = check_atom(
            &vocab,
            domains,
            &f.fact.relation,
            &f.fact.args,
            /* wildcard_ok = */ false,
            f.span,
        );
        for d in &mut fdiags {
            if d.code == E_RETRACT_WILDCARD_ASSERT {
                d.message = format!(
                    "seed fact `{}` contains `_`; seed facts are ground (checked as for ::assert, dsl 0.3.0 §4, D12)",
                    f.raw
                );
            }
        }
        let origin = vocab.origins.facts.get(&f.raw);
        diags.extend(fdiags.into_iter().map(|d| at_origin(d, origin)));
    }

    (vocab, diags)
}

/// The top of `kind`'s `subsetOf:` chain (itself when it has no parent).
/// Cycle-safe: stops after one step per declared kind.
fn root_kind<'a>(kinds: &'a BTreeMap<String, EntityKindDecl>, kind: &'a str) -> &'a str {
    let mut cur = kind;
    for _ in 0..kinds.len() {
        match kinds.get(cur).and_then(|d| d.subset_of.as_deref()) {
            Some(parent) if kinds.contains_key(parent) => cur = parent,
            _ => break,
        }
    }
    cur
}

/// dsl 0.26.0 §2.3: one `entities: { <kind>: { add: [<id>…] } }` extension.
#[derive(Clone, Debug)]
pub struct KindAdd {
    pub kind: String,
    pub members: Vec<String>,
    /// Prerelease N4: where each of `members` is written, index-aligned —
    /// positioned in the `origin` file for an imported `add:`, in the checked
    /// document for its own; `None` where the line scan did not find it.
    pub member_spans: Vec<Option<Span>>,
    /// The imported schema writing it (a problem is reported there, once,
    /// via [`at_origin`]); `None` when this document writes it.
    pub origin: Option<DeclOrigin>,
    /// The problem anchor in the checked document.
    pub span: Span,
    /// dsl 0.27.0 §7: the `labels:` beside the `add:` — display text for the
    /// members it adds, merged into the kind's labels with them.
    pub labels: BTreeMap<String, lute_manifest::relations::KindLabel>,
}

/// Where the one declaration of a kind lives (prerelease N4): its schema as
/// a message names it ([`origin_display`]) and the line of each member its
/// `members:` lists there.
pub(crate) struct KindHome {
    pub shown: String,
    pub member_lines: BTreeMap<String, u32>,
}

/// `file` as a message names it (prerelease N4): relative to the nearest
/// ancestor holding a `lute.project.yaml` — the same text from every
/// importer, so the roll-up still folds — else its file name.
pub(crate) fn origin_display(file: &std::path::Path) -> String {
    file.ancestors()
        .skip(1)
        .find(|dir| dir.join("lute.project.yaml").is_file())
        .and_then(|root| file.strip_prefix(root).ok())
        .map(|rel| {
            rel.components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/")
        })
        .unwrap_or_else(|| {
            file.file_name().map_or_else(
                || file.display().to_string(),
                |n| n.to_string_lossy().into_owned(),
            )
        })
}

/// dsl 0.26.0 §2.3: merge each `add:` into the one declaration of its kind,
/// in `adds` order. An `add:` whose kind no declaration names, or names an
/// `open:` kind, is `E-ENTITY-KIND-SHAPE`; so is a member the kind already
/// has — from its declaration or an earlier `add:` (§2.2), naming both
/// places with their lines and anchored at the second member's own line
/// (prerelease N4). `base_home` locates the declaration of a kind, when
/// known.
pub(crate) fn apply_kind_adds(
    kinds: &mut BTreeMap<String, EntityKindDecl>,
    adds: &[KindAdd],
    base_home: &dyn Fn(&str) -> Option<KindHome>,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    // (kind, member) -> where it was first listed.
    let mut listed: BTreeMap<(&str, String), String> = BTreeMap::new();
    let here = |add: &KindAdd| {
        add.origin.as_ref().map_or_else(
            || "this document".to_string(),
            |o| format!("`{}`", origin_display(&o.file)),
        )
    };
    let at_line = |line: Option<u32>| {
        line.filter(|l| *l > 0)
            .map_or(String::new(), |l| format!(" (line {l})"))
    };
    for add in adds {
        let report = |message: String, at: Option<Span>| match (&add.origin, at) {
            (Some(origin), Some(span)) => at_origin(
                diag(E_ENTITY_KIND_SHAPE, message, add.span),
                Some(&DeclOrigin {
                    file: origin.file.clone(),
                    span,
                }),
            ),
            (Some(origin), None) => {
                at_origin(diag(E_ENTITY_KIND_SHAPE, message, add.span), Some(origin))
            }
            (None, at) => diag(E_ENTITY_KIND_SHAPE, message, at.unwrap_or(add.span)),
        };
        let kind = add.kind.as_str();
        let Some(decl) = kinds.get_mut(kind) else {
            let hint = lute_manifest::suggest::nearest(kind, kinds.keys().map(String::as_str), 2)
                .map(|s| format!(" — did you mean `{s}`?"))
                .unwrap_or_default();
            out.push(report(format!(
                "entity kind `{kind}` has an `add:` list, but no schema import declares `{kind}`{hint}; exactly one schema declares a kind with `members:`, and others `add:` to it (dsl 0.26.0 §2.3)"
            ), None));
            continue;
        };
        let members = match &mut decl.shape {
            KindShape::Open => {
                out.push(report(format!(
                    "entity kind `{kind}` is `open:` (the engine registers its members); `add:` extends a kind that lists its `members:` (dsl 0.26.0 §2.3)"
                ), None));
                continue;
            }
            KindShape::Invalid => continue,
            KindShape::Members(ms) => ms,
        };
        let home = base_home(kind);
        for m in members.iter() {
            listed
                .entry((kind, m.clone()))
                .or_insert_with(|| match &home {
                    Some(h) => format!(
                        "`{kind}`'s declaration in `{}`{}",
                        h.shown,
                        at_line(h.member_lines.get(m).copied())
                    ),
                    None => format!("`{kind}`'s declaration"),
                });
        }
        for (i, m) in add.members.iter().enumerate() {
            let span = add.member_spans.get(i).copied().flatten();
            let this = format!(
                "the `add:` of {}{}",
                here(add),
                at_line(span.map(|s| s.line))
            );
            match listed.get(&(kind, m.clone())) {
                Some(first) => out.push(report(
                    format!(
                        "entity kind `{kind}` lists `{m}` twice — in {first} and in {this}; list each member once (dsl 0.26.0 §2.2)"
                    ),
                    span,
                )),
                None => {
                    listed.insert((kind, m.clone()), this);
                    members.push(m.clone());
                }
            }
        }
        for (m, label) in &add.labels {
            decl.labels
                .entry(m.clone())
                .or_insert_with(|| label.clone());
        }
    }
    out
}

/// dsl 0.24.0 §3: every `subsetOf:` names a declared, closed parent kind, is
/// not a loop, and belongs to a kind that lists its own `members:` — else
/// `E-ENTITY-KIND-SHAPE`. A sub-kind's members are its parent's (dsl 0.26.0
/// §2.3, [`lute_manifest::relations::imply_sub_kind_members`]), so none is
/// ever missing from the parent.
fn check_sub_kinds(
    kinds: &BTreeMap<String, EntityKindDecl>,
    span_of: &dyn Fn(&str) -> Span,
    origins: &DeclOrigins,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for (name, decl) in kinds {
        let Some(parent) = decl.subset_of.as_deref() else {
            continue;
        };
        let message = match (&decl.shape, kinds.get(parent).map(|p| &p.shape)) {
            (KindShape::Invalid, _) | (_, Some(KindShape::Invalid)) => continue,
            (_, None) => format!(
                "entity kind `{name}` is `subsetOf: {parent}`, but `{parent}` is not a declared entity kind (dsl 0.24.0 §3)"
            ),
            (KindShape::Open, _) => format!(
                "entity kind `{name}` is `subsetOf: {parent}` and `open:`; a sub-kind lists its `members:`, each a member of `{parent}` (dsl 0.24.0 §3)"
            ),
            (_, Some(KindShape::Open)) => format!(
                "entity kind `{name}` is `subsetOf: {parent}`, but `{parent}` is `open:`; a sub-kind's parent lists its `members:` (dsl 0.24.0 §3)"
            ),
            (KindShape::Members(_), Some(KindShape::Members(_))) => {
                if !lute_manifest::relations::kind_within(kinds, parent, name) {
                    continue;
                }
                format!(
                    "entity kind `{name}` is `subsetOf: {parent}`, which is `{name}` itself or one of its own sub-kinds; `subsetOf:` chains must not loop (dsl 0.24.0 §3)"
                )
            }
        };
        // dsl 0.26 §2.7: an imported sub-kind is reported once, at its schema
        // line, and the importers' copies fold into one report.
        out.push(at_origin(
            diag(E_ENTITY_KIND_SHAPE, message, span_of(name)),
            origins.kinds.get(name),
        ));
    }
    out
}

/// dsl 0.26.0 §2.2: a member listed more than once in one entity kind's
/// `members:` or one `enums:` list is `E-ENTITY-KIND-SHAPE`, reported at the
/// second position with both lines named. `enums` maps each enum declared
/// in `meta` to its members as written. Spans are `meta`-document offsets
/// (line/column zeroed, like [`meta_key_span`]'s).
pub fn check_member_dups(
    meta: &Meta,
    kinds: &ParsedKinds,
    enums: &BTreeMap<String, Vec<String>>,
) -> Vec<Diagnostic> {
    let kind_lists = kinds
        .kinds
        .iter()
        .filter_map(|(name, decl)| match &decl.shape {
            KindShape::Members(ms) => Some(("entity kind", name, ms)),
            _ => None,
        });
    let enum_lists = enums.iter().map(|(name, ms)| ("enum", name, ms));
    let mut out = Vec::new();
    for (noun, name, members) in kind_lists.chain(enum_lists) {
        let dups = lute_manifest::relations::duplicate_members(members);
        if dups.is_empty() {
            continue;
        }
        let key = meta_key_span(meta, name);
        let (base, first_line) = frontmatter_base(meta);
        let written = member_list_offsets(&meta.raw_yaml, key.byte_start.saturating_sub(base));
        for member in dups {
            let mut at = written
                .iter()
                .filter(|(_, m)| *m == member)
                .map(|(o, _)| *o);
            let (first, second) = (at.next(), at.next());
            let line_of = |o: usize| first_line + meta.raw_yaml[..o].matches('\n').count();
            let lines = match (first, second) {
                (Some(a), Some(b)) => format!(" (lines {} and {})", line_of(a), line_of(b)),
                _ => String::new(),
            };
            let span = second.map_or(key, |o| Span {
                byte_start: base + o,
                byte_end: base + o + member.len(),
                line: 0,
                column: 0,
                utf16_range: (0, 0),
            });
            out.push(diag(
                E_ENTITY_KIND_SHAPE,
                format!(
                    "{noun} `{name}` lists member `{member}` more than once{lines}; each member \
                     is listed once — two authors adding the same id would silently merge \
                     (dsl 0.26.0 §2.2)"
                ),
                span,
            ));
        }
    }
    out
}

/// dsl 0.26 §2.1: where the `state:` declaration of `path` sits in `meta` —
/// a flat `run.x:` key as written, a nested `run: { x: … }` at its last
/// segment's key.
pub fn state_key_span(meta: &Meta, path: &str) -> Span {
    let needle = if meta.raw_yaml.contains(path) {
        path
    } else {
        path.rsplit('.').next().unwrap_or(path)
    };
    meta_key_span(meta, needle)
}

/// `span` (a [`meta_key_span`]-style, line-less document offset inside
/// `meta`'s frontmatter) with its 1-based line and character column filled in.
pub(crate) fn meta_position(meta: &Meta, span: Span) -> Span {
    let (base, first_line) = frontmatter_base(meta);
    let mut off = span
        .byte_start
        .saturating_sub(base)
        .min(meta.raw_yaml.len());
    while !meta.raw_yaml.is_char_boundary(off) {
        off -= 1;
    }
    let before = &meta.raw_yaml[..off];
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    Span {
        line: (first_line + before.matches('\n').count()) as u32,
        column: before[line_start..].chars().count() as u32 + 1,
        ..span
    }
}

/// Where `meta.raw_yaml` starts in the document (byte offset) and its first
/// line number — [`meta_key_span`]'s envelope rule.
fn frontmatter_base(meta: &Meta) -> (usize, usize) {
    let enveloped = meta.span.byte_end.saturating_sub(meta.span.byte_start) != meta.raw_yaml.len();
    let line = meta.span.line.max(1) as usize;
    if enveloped {
        (meta.span.byte_start + 4, line + 1)
    } else {
        (meta.span.byte_start, line)
    }
}

/// The members of the list declared under the key starting at `key_off` in
/// `raw` (a YAML frontmatter), as `(offset, member)` pairs in written order:
/// a flow `[a, b]` or block `- a` list directly under the key, or the
/// `members:` list of a `{ members: … }` / block-mapping long form. A line
/// scan, never a YAML re-parse; an unrecognized shape yields nothing.
fn member_list_offsets(raw: &str, key_off: usize) -> Vec<(usize, String)> {
    list_offsets_under(raw, key_off, "members:", true)
}

/// [`member_list_offsets`] for the list under `sub` (`members:` / `add:`)
/// inside the key at `key_off`; `direct` also reads a list written right
/// under the key when `sub` is absent.
fn list_offsets_under(raw: &str, key_off: usize, sub: &str, direct: bool) -> Vec<(usize, String)> {
    let Some((colon, end)) = value_region(raw, key_off) else {
        return Vec::new();
    };
    // Long form: the list is the value of the `sub` key inside it.
    let start = match find_sub_key(&raw[colon..end], sub) {
        Some(m) => colon + m,
        None if direct => colon,
        None => return Vec::new(),
    };
    list_items(raw, start, end)
}

/// The value of the YAML key starting at `key_off` in `raw`: from just past
/// its `:` to the end of the last line indented deeper than the key (blank
/// lines included). `None` when `key_off` is not inside `raw` or no `:`
/// follows.
fn value_region(raw: &str, key_off: usize) -> Option<(usize, usize)> {
    let line_start = raw.get(..key_off)?.rfind('\n').map_or(0, |i| i + 1);
    let key_indent = key_off - line_start;
    let colon = key_off + raw[key_off..].find(':')? + 1;
    let mut end = raw[colon..].find('\n').map_or(raw.len(), |i| colon + i);
    while end < raw.len() {
        let next = end + 1;
        let line_end = raw[next..].find('\n').map_or(raw.len(), |i| next + i);
        let line = &raw[next..line_end];
        let trimmed = line.trim_start();
        if !trimmed.is_empty() && line.len() - trimmed.len() <= key_indent {
            break;
        }
        end = line_end;
    }
    Some((colon, end))
}

/// Offset (in `region`) just past a `sub` key (`members:`), outside comments.
fn find_sub_key(region: &str, sub: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(i) = region[from..].find(sub) {
        let at = from + i;
        let before = region[..at].chars().next_back();
        let line = &region[region[..at].rfind('\n').map_or(0, |n| n + 1)..at];
        if !line.contains('#') && before.is_none_or(|c| !c.is_alphanumeric() && c != '_') {
            return Some(at + sub.len());
        }
        from = at + 1;
    }
    None
}

/// dsl 0.26.0 §2.2 (prerelease N4): each item of entity kind `kind`'s
/// `members:` (`sub = "members:"`) or `add:` (`sub = "add:"`) list in `meta`,
/// as it is written, with its span — `meta`-document offsets, line-less,
/// like [`check_member_dups`]'s. Empty for a shape the line scan does not
/// recognize.
pub fn kind_list_spans(meta: &Meta, kind: &str, sub: &str) -> Vec<(String, Span)> {
    let key = meta_key_span(meta, kind);
    let (base, _) = frontmatter_base(meta);
    list_offsets_under(
        &meta.raw_yaml,
        key.byte_start.saturating_sub(base),
        sub,
        sub == "members:",
    )
    .into_iter()
    .map(|(o, m)| {
        let span = Span {
            byte_start: base + o,
            byte_end: base + o + m.len(),
            line: 0,
            column: 0,
            utf16_range: (0, 0),
        };
        (m, span)
    })
    .collect()
}

/// Where each of `members` (kind `kind`'s `add:` list as parsed) is written
/// in `meta`, index-aligned: the k-th listing of an id takes the k-th place
/// the line scan found it ([`kind_list_spans`]); `None` when it found fewer.
pub(crate) fn add_member_spans(meta: &Meta, kind: &str, members: &[String]) -> Vec<Option<Span>> {
    let written = kind_list_spans(meta, kind, "add:");
    let mut taken = vec![false; written.len()];
    members
        .iter()
        .map(|m| {
            let i = written
                .iter()
                .enumerate()
                .position(|(i, (w, _))| !taken[i] && w == m)?;
            taken[i] = true;
            Some(written[i].1)
        })
        .collect()
}

/// Where imported kind `kind` is declared, with its members' lines
/// (prerelease N4), from the resolved imports' `origins`.
pub(crate) fn kind_home(origins: &DeclOrigins, kind: &str) -> Option<KindHome> {
    let origin = origins.kinds.get(kind)?;
    let prefix = member_origin_key(kind, "");
    Some(KindHome {
        shown: origin_display(&origin.file),
        member_lines: origins
            .members
            .range(prefix.clone()..)
            .take_while(|(k, _)| k.starts_with(&prefix))
            .filter(|(_, o)| o.file == origin.file)
            .map(|(k, o)| (k[prefix.len()..].to_string(), o.span.line))
            .collect(),
    })
}

/// The items of the flow or block list starting at `start` (just past its
/// key's `:`), bounded by `end`.
fn list_items(raw: &str, start: usize, end: usize) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let push = |out: &mut Vec<(usize, String)>, at: usize, tok: &str| {
        let lead = tok.len() - tok.trim_start().len();
        let t = tok.trim();
        let q = t.starts_with(['"', '\'']) as usize;
        let bare = t.trim_matches(['"', '\'']);
        if !bare.is_empty() {
            out.push((at + lead + q, bare.to_string()));
        }
    };
    let text = &raw[start..end];
    let body = text.trim_start();
    if body.starts_with('[') {
        let open = start + (text.len() - body.len()) + 1;
        let mut i = open;
        let mut tok_start = open;
        let bytes = raw.as_bytes();
        while i < end {
            match bytes[i] {
                b'#' if i == 0 || bytes[i - 1].is_ascii_whitespace() => {
                    while i < end && bytes[i] != b'\n' {
                        i += 1;
                    }
                    tok_start = i;
                    continue;
                }
                b',' | b']' => {
                    push(&mut out, tok_start, &raw[tok_start..i]);
                    if bytes[i] == b']' {
                        break;
                    }
                    tok_start = i + 1;
                }
                _ => {}
            }
            i += 1;
        }
        return out;
    }
    // Block list: the run of `- item` lines (blank and comment lines skipped)
    // right under the key; the next other line ends it.
    let mut off = start;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if let Some(item) = trimmed.strip_prefix("- ") {
            let item_at = off + (line.len() - trimmed.len()) + 2;
            let item = item.split(" #").next().unwrap_or(item).trim_end();
            push(&mut out, item_at, item);
        } else if off != start && !trimmed.is_empty() && !trimmed.starts_with('#') {
            break;
        }
        off += line.len();
    }
    out
}

/// The ONE atom/pattern closure checker (§4's closure checks, D10 included):
/// shared by seed `facts:` (here), rule atoms (Task 8), `::assert`/
/// `::retract` writes (Task 10), and CEL fact queries (Task 11).
/// `wildcard_ok` gates `_` (true for a retract pattern, false everywhere
/// else — a seed, an assert, a rule atom, a query pattern); returns
/// diagnostics only, never mutates.
pub fn check_atom(
    vocab: &RelVocab,
    domains: &BTreeMap<String, Domain>,
    relation: &str,
    args: &[FactArg],
    wildcard_ok: bool,
    span: Span,
) -> Vec<Diagnostic> {
    // D13 sentinel: a malformed `::assert`/`::retract`/`facts:` entry already
    // parsed to relation == "" and was already diagnosed at parse — never
    // double-report.
    if relation.is_empty() {
        return Vec::new();
    }
    let Some(decl) = vocab.relations.get(relation) else {
        let hint = if vocab.kinds.contains_key(relation) {
            " (an entity kind is a rule-body predicate, not an assertable fact, dsl 0.3.0 §3.1)"
                .to_string()
        } else {
            // dsl 0.5.0 §2.2 "did you mean": `relations:` is a CLOSED declared
            // set, so this is the cheapest suggestion in the language to
            // compute — and it was the one identifier class without one (#35,
            // T4.6). Advisory text only: no new code, no severity change.
            // `lute_manifest::suggest::nearest` is the workspace's ONE
            // Levenshtein, and `relations` is a `BTreeMap`, so the tie-break
            // is deterministic. The entity-kind branch keeps precedence: a
            // name that IS a declared kind gets the categorical explanation,
            // because that author's mistake is not a typo.
            match lute_manifest::suggest::nearest(
                relation,
                vocab.relations.keys().map(String::as_str),
                2,
            ) {
                Some(sugg) => format!(" — did you mean `{sugg}`?"),
                None => String::new(),
            }
        };
        return vec![diag(
            E_RELATION_UNKNOWN,
            format!("unknown relation `{relation}`{hint} (dsl 0.3.0 §4)"),
            span,
        )];
    };
    if args.len() != decl.args.len() {
        return vec![diag(
            E_RELATION_ARITY,
            format!(
                "relation `{relation}` expected {} argument(s), got {} (dsl 0.3.0 §4/§5)",
                decl.args.len(),
                args.len()
            ),
            span,
        )];
    }
    let mut out = Vec::new();
    for (i, (arg, dname)) in args.iter().zip(decl.args.iter()).enumerate() {
        if let FactTerm::Param(p) = &arg.term {
            out.push(diag(
                E_FACT_DOMAIN,
                format!(
                    "relation `{relation}` argument {i} is `@{p}`, a component param: only an \
                     `effects: true` component body may use one, and each `::use` binds it to \
                     a constant (dsl 0.24.0 §4)"
                ),
                span,
            ));
            continue;
        }
        if matches!(arg.term, FactTerm::Wildcard) {
            if !wildcard_ok {
                out.push(diag(
                    E_RETRACT_WILDCARD_ASSERT,
                    format!(
                        "relation `{relation}` argument {i} is `_`; only a retract pattern may contain a wildcard (dsl 0.3.0 §5)"
                    ),
                    span,
                ));
            }
            continue;
        }
        if dname.as_str() == "bool" {
            if !matches!(arg.term, FactTerm::Bool(_)) {
                out.push(diag(
                    E_FACT_DOMAIN,
                    format!(
                        "relation `{relation}` argument {i} must be `true`/`false` (declared `bool`, dsl 0.3.0 §4)"
                    ),
                    span,
                ));
            }
            continue;
        }
        if let Some(kind) = vocab.kinds.get(dname) {
            match &kind.shape {
                KindShape::Members(members) => {
                    let FactTerm::Ident(id) = &arg.term else {
                        out.push(diag(
                            E_FACT_DOMAIN,
                            format!(
                                "relation `{relation}` argument {i} must be a member of entity kind `{dname}` (dsl 0.3.0 §3.1)"
                            ),
                            span,
                        ));
                        continue;
                    };
                    if !members.contains(id) {
                        out.push(diag(
                            E_FACT_DOMAIN,
                            format!(
                                "`{id}` is not a declared member of entity kind `{dname}` (relation `{relation}` argument {i}, dsl 0.3.0 §3.1){}",
                                member_hint(id, members)
                            ),
                            span,
                        ));
                    }
                }
                KindShape::Open => {
                    let FactTerm::Ident(id) = &arg.term else {
                        out.push(diag(
                            E_FACT_DOMAIN,
                            format!(
                                "relation `{relation}` argument {i} must be an id (declared `open` entity kind `{dname}`, dsl 0.3.0 §3.1)"
                            ),
                            span,
                        ));
                        continue;
                    };
                    // D10: an open kind's membership is engine-deferred, never
                    // statically checked — only the one-id-one-kind cross-check.
                    if let Some(other) = closed_kind_owning(vocab, id) {
                        out.push(diag(
                            E_FACT_DOMAIN,
                            format!(
                                "`{id}` already belongs to entity kind `{other}`; an id belongs to exactly one kind (relation `{relation}` argument {i}, dsl 0.3.0 §3.1)"
                            ),
                            span,
                        ));
                    }
                }
                KindShape::Invalid => {
                    // The decl itself already got E-ENTITY-KIND-SHAPE; never cascade.
                }
            }
            continue;
        }
        if let Some(members) = vocab.enums.get(dname) {
            let FactTerm::Ident(id) = &arg.term else {
                out.push(diag(
                    E_FACT_DOMAIN,
                    format!(
                        "relation `{relation}` argument {i} must be a member of enum `{dname}` (dsl 0.3.0 §4)"
                    ),
                    span,
                ));
                continue;
            };
            if !members.contains(id) {
                out.push(diag(
                    E_FACT_DOMAIN,
                    format!(
                        "`{id}` is not a declared member of enum `{dname}` (relation `{relation}` argument {i}, dsl 0.3.0 §4){}",
                        member_hint(id, members)
                    ),
                    span,
                ));
            }
            continue;
        }
        if let Some(dom) = domains.get(dname) {
            let FactTerm::Ident(id) = &arg.term else {
                out.push(diag(
                    E_FACT_DOMAIN,
                    format!(
                        "relation `{relation}` argument {i} must be a member of domain `{dname}` (dsl 0.3.0 §4)"
                    ),
                    span,
                ));
                continue;
            };
            // An open plugin/core domain gets the SAME D10 treatment as an
            // open entity kind: membership is never statically checked.
            if !dom.open && !dom.members.contains(id) {
                out.push(diag(
                    E_FACT_DOMAIN,
                    format!(
                        "`{id}` is not a declared member of domain `{dname}` (relation `{relation}` argument {i}, dsl 0.3.0 §4){}",
                        member_hint(id, &dom.members)
                    ),
                    span,
                ));
            }
            continue;
        }
        // `dname` resolves to nothing: the DECL already got E-RELATION-DOMAIN
        // (build_rel_vocab's merged check (a)) — never cascade onto every use site.
    }
    out
}

/// dsl 0.26.0 §8 (T3-7): ` — did you mean `x`?` for a member typo, empty
/// when nothing is close.
pub fn member_hint(id: &str, members: &[String]) -> String {
    lute_manifest::suggest::nearest(id, members.iter().map(String::as_str), 3)
        .map_or_else(String::new, |near| format!(" — did you mean `{near}`?"))
}

/// The name of a CLOSED entity kind that already claims `id` as a member, if
/// any (§3.1 one-id-one-kind, D10's cross-check for an open-kind arg).
fn closed_kind_owning<'a>(vocab: &'a RelVocab, id: &str) -> Option<&'a str> {
    vocab
        .kinds
        .iter()
        .find_map(|(name, decl)| match &decl.shape {
            KindShape::Members(members) if members.iter().any(|m| m.as_str() == id) => {
                Some(name.as_str())
            }
            _ => None,
        })
}
