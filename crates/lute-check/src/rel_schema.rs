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
            diags.push(diag(
                E_RELATION_DOMAIN,
                format!(
                    "relation `{name}` argument domain `{arg}` is not a declared entity kind, enum, or domain (dsl 0.3.0 §4)"
                ),
                span_of(arg),
            ));
        }
    }

    // Merged check (b): a kind name and a relation name share one rule-body
    // predicate namespace (§4) — a name declared as both is E-KIND-NAME-CLASH.
    for name in kinds.keys() {
        if relations.contains_key(name) {
            diags.push(diag(
                E_KIND_NAME_CLASH,
                format!(
                    "`{name}` is declared as both an entity kind and a relation; kinds and relations share one predicate namespace (dsl 0.3.0 §4)"
                ),
                span_of(name),
            ));
        }
    }

    // Merged check (c): one-id-one-kind (§3.1) — an id in TWO closed kinds'
    // `members:` is E-ENTITY-KIND-CLASH.
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
            for id in members_a {
                if members_b.contains(id) {
                    diags.push(diag(
                        E_ENTITY_KIND_CLASH,
                        format!(
                            "id `{id}` is a member of both entity kinds `{name_a}` and `{name_b}`; an id belongs to exactly one kind (dsl 0.3.0 §3.1)"
                        ),
                        span_of(id),
                    ));
                }
            }
        }
    }

    // Facts/rules always UNION (spec §4.1) — imports first, then inline.
    let mut facts = imports.rel.facts.clone();
    facts.extend(typed.rel_facts.iter().cloned());
    let mut rules = imports.rel.rules.clone();
    rules.extend(typed.rel_rules.iter().cloned());

    let vocab = RelVocab {
        kinds,
        enums,
        relations,
        facts,
        rules,
        guard_tainted: BTreeSet::new(),
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
        diags.extend(fdiags);
    }

    (vocab, diags)
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
        } else {
            ""
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
