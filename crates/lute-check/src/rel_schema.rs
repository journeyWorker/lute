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

use crate::meta::{meta_key_span, namespace_of, FactDecl, RuleDecl, TypedMeta};
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
}

/// dsl 0.24 T3-6: re-home a diagnostic about an IMPORTED declaration. The
/// importer carries it at its frontmatter start (byte 0, like
/// `E-USES-PARSE`) with the schema file named, and the original — at the
/// declaration's own line in that file — as its `related` entry; the project
/// roll-up then folds the identical copies every importer carries into one
/// report. `None` (a local declaration) returns `d` unchanged.
pub fn at_origin(d: Diagnostic, origin: Option<&DeclOrigin>) -> Diagnostic {
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
            "{} (declared in schema import `{}`)",
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

/// dsl 0.24 T3-8: a relation named like a CEL call/macro/keyword can never be
/// queried — `holds(has(lamp))` does not even parse.
pub const E_RELATION_RESERVED_NAME: &str = "E-RELATION-RESERVED-NAME";

/// dsl 0.25.0 §1/§6: a relation declaration whose `excludes` names a
/// relation of other argument kinds, or whose `changedOn` sits on a relation
/// that is not engine-`reserved` or names an undeclared occasion.
pub const E_RELATION_DECL: &str = "E-RELATION-DECL";

/// dsl 0.25.0 §1 (LH N17): a rule whose head relation excludes a relation its
/// positive body requires on the head's arguments — every firing makes both
/// hold.
pub const E_RULE_EXCLUSIVE: &str = "E-RULE-EXCLUSIVE";

/// Names a relation may not take (dsl 0.24 T3-8): the Lute-CEL profile's
/// calls (`isSet`, `holds`, `count`, `countDistinct`, `validAt`, `now`,
/// `visited`), CEL's macros (`has`, `all`, `exists`, `exists_one`, `map`,
/// `filter`) and CEL's reserved words — each either parses as something
/// else inside a fact query or is not an identifier at all.
pub const RESERVED_RELATION_NAMES: &[&str] = &[
    "all",
    "as",
    "break",
    "const",
    "continue",
    "count",
    "countDistinct",
    "else",
    "exists",
    "exists_one",
    "false",
    "filter",
    "for",
    "function",
    "has",
    "holds",
    "if",
    "import",
    "in",
    "isSet",
    "let",
    "loop",
    "map",
    "namespace",
    "now",
    "null",
    "package",
    "return",
    "true",
    "validAt",
    "var",
    "visited",
    "void",
    "while",
];

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
/// directly.
pub fn validate_rel_decls(
    kinds: &ParsedKinds,
    rels: &ParsedRelations,
    span_of: &dyn Fn(&str) -> Span,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for (name, decl) in &kinds.kinds {
        if matches!(decl.shape, KindShape::Invalid) {
            out.push(diag(
                E_ENTITY_KIND_SHAPE,
                format!(
                    "entity kind `{name}` must declare exactly one of `members:`/`open:` (dsl 0.3.0 §3.1)"
                ),
                span_of(name),
            ));
        }
    }
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
        if RESERVED_RELATION_NAMES.contains(&name.as_str()) {
            out.push(diag(
                E_RELATION_RESERVED_NAME,
                format!(
                    "relation `{name}` uses a reserved CEL name; `holds({name}(…))` cannot be \
                     written — rename the relation (dsl 0.24 T3-8)"
                ),
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
            out.push(diag(
                E_RELATION_DOMAIN,
                format!(
                    "relation `{name}` field `{field}` is malformed or unknown (dsl 0.3.0 §4, D4)"
                ),
                span_of(name),
            ));
        }
        if let Some(tier) = &decl.tier {
            if namespace_of(tier).is_none() {
                out.push(diag(
                    E_RELATION_DOMAIN,
                    format!(
                        "relation `{name}` has unknown `tier: {tier}` (expected one of scene/run/user/app/quest, dsl 0.3.0 §4)"
                    ),
                    span_of(name),
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
    for i in 0..closed.len() {
        for j in (i + 1)..closed.len() {
            let (name_a, members_a) = closed[i];
            let (name_b, members_b) = closed[j];
            if root_kind(&kinds, name_a) == root_kind(&kinds, name_b) {
                continue;
            }
            for id in members_a {
                if members_b.contains(id) {
                    diags.push(at_origin(
                        diag(
                            E_ENTITY_KIND_CLASH,
                            format!(
                                "id `{id}` is a member of both entity kinds `{name_a}` and `{name_b}`; an id belongs to exactly one kind (dsl 0.3.0 §3.1) — declare one a sub-kind of the other with `subsetOf:` (dsl 0.24.0 §3)"
                            ),
                            span_of(id),
                        ),
                        origins.kinds.get(name_b).filter(|_| origins.kinds.contains_key(name_a)),
                    ));
                }
            }
        }
    }
    diags.extend(check_sub_kinds(&kinds, &span_of));

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

/// dsl 0.24.0 §3: every `subsetOf:` names a declared, closed parent kind, is
/// not a loop, belongs to a kind that lists its own `members:`, and every
/// member is a member of the parent — else `E-ENTITY-KIND-SHAPE`.
fn check_sub_kinds(
    kinds: &BTreeMap<String, EntityKindDecl>,
    span_of: &dyn Fn(&str) -> Span,
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
            (KindShape::Members(members), Some(KindShape::Members(parent_members))) => {
                if lute_manifest::relations::kind_within(kinds, parent, name) {
                    format!(
                        "entity kind `{name}` is `subsetOf: {parent}`, which is `{name}` itself or one of its own sub-kinds; `subsetOf:` chains must not loop (dsl 0.24.0 §3)"
                    )
                } else {
                    let outside: Vec<&str> = members
                        .iter()
                        .filter(|m| !parent_members.contains(m))
                        .map(String::as_str)
                        .collect();
                    if outside.is_empty() {
                        continue;
                    }
                    format!(
                        "entity kind `{name}` is `subsetOf: {parent}`, but {} not a member of `{parent}`; every member of a sub-kind is a member of its parent — add {} to `{parent}`'s `members:` (dsl 0.24.0 §3)",
                        outside.iter().map(|m| format!("`{m}`")).collect::<Vec<_>>().join(", ")
                            + if outside.len() == 1 { " is" } else { " are" },
                        if outside.len() == 1 { "it" } else { "them" }
                    )
                }
            }
        };
        out.push(diag(E_ENTITY_KIND_SHAPE, message, span_of(name)));
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
            " (an entity kind is a rule-body predicate, not an assertable fact — dsl 0.3.0 §3.1)"
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
                                "`{id}` is not a declared member of entity kind `{dname}` (relation `{relation}` argument {i}, dsl 0.3.0 §3.1)"
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
                        "`{id}` is not a declared member of enum `{dname}` (relation `{relation}` argument {i}, dsl 0.3.0 §4)"
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
                        "`{id}` is not a declared member of domain `{dname}` (relation `{relation}` argument {i}, dsl 0.3.0 §4)"
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
