use std::collections::BTreeSet;
use std::path::PathBuf;
use lute_core_span::{Diagnostic, Layer, Severity};
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_manifest::types::Type;

/// `W-DOMAIN-UNREAD`: a domain the project declares that no active construct
/// reads (dsl 0.10.0 §11.1). Project-wide only (**D-V**).
pub const W_DOMAIN_UNREAD: &str = "W-DOMAIN-UNREAD";

/// Every domain name some active construct in `snapshot` reads.
///
/// dsl 0.10.0 §11.1: this is the set of domain-typed attribute slots in the
/// RESOLVED capability snapshot, not a fixed list — a plugin directive
/// declaring `{ domain: reason }` makes `reason` read, and the warning stops.
/// Three sources, and they are the whole closed rule:
///  1. every directive's own `AttrDecl`s;
///  2. every cross-cutting `stampAttrs` decl, which is admissible on EVERY
///     directive (plugin §14.1);
///  3. the content line's two domain slots
///     ([`crate::content_line::CONTENT_LINE_DOMAIN_SLOTS`]), which are not
///     `AttrDecl`s because a content line is not a directive.
pub fn domain_reading_set(snapshot: &CapabilitySnapshot) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for decl in snapshot.directives.values() {
        for attr in &decl.attrs {
            collect_domain_names(&attr.ty, &mut out);
        }
    }
    for decl in snapshot.stamp_attrs.values() {
        collect_domain_names(&decl.ty, &mut out);
    }
    for name in crate::content_line::CONTENT_LINE_DOMAIN_SLOTS {
        out.insert((*name).to_string());
    }
    // dsl 0.22.0 §8: an occasion target domain `{ prefix, entity }` checks
    // every beat target against `entity`'s members — a read.
    for occasion in snapshot.occasions.values() {
        if let lute_manifest::schema::OccasionTarget::Domain { entity, .. } = &occasion.target {
            out.insert(entity.clone());
        }
    }
    // dsl 0.26.0 §2.5: a reward kind's `target: { entity: K }` contract
    // checks every `<reward target=…>` of that kind against `K`.
    for kind in snapshot.reward_kinds.values() {
        if let Some(entity) = kind.target.as_ref().and_then(|t| t.entity.as_ref()) {
            out.insert(entity.clone());
        }
    }
    out
}

/// Every domain name a `relations:` declaration reads as an argument position
/// (dsl 0.10.0 §11.1, relational spec §4).
///
/// **This is not optional, and the spec's "domain-typed attribute slots" phrasing
/// is what makes it easy to miss.** A relation's `args: [crew]` closed-checks
/// every `awake(…)` atom against `crew`'s membership, which is exactly as active
/// a read as a directive attr typed `{ domain: crew }`. Leaving it out made
/// `W-DOMAIN-UNREAD` fire six times on `docs/examples` — `character`, `clue`,
/// `crew`, `location`, `suspect`, `topic`, every one of them an `entities:`
/// domain read by a relation signature — i.e. a false positive on the entire
/// relational half of the language.
pub fn domain_reads_from_relations(vocab: &crate::rel_schema::RelVocab) -> BTreeSet<String> {
    vocab
        .relations
        .values()
        .flat_map(|r| r.args.iter())
        .filter(|a| !a.is_empty())
        .cloned()
        .collect()
}

/// Every domain name a declared state path is typed against
/// (`run.wd: { type: { domain: weekday } }`). Since dsl 0.24.0 §1 such a path
/// renders the domain's member `labels` in `{{…}}`, so the declaration reaches
/// rendered text — a read as real as a directive attr typed against it.
pub fn domain_reads_from_state(schema: &crate::meta::StateSchema) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for decl in schema.decls.values() {
        collect_domain_names(&decl.ty, &mut out);
    }
    out
}

/// Every entity kind the relational vocabulary itself reads (round-3 CR N1,
/// ER N10): a `per: <kind>` state family's index (its members declare the
/// paths), a sub-kind's `subsetOf:` parent (the sub-kind's members are
/// checked against it), and a kind atom a rule body or a condition queries
/// (`trusts(P) :- confidant(P), …`, `holds('npc', [x])`) — `texts` are the
/// sources a condition may be written in. Each is as active a read as a
/// relation's `args: [kind]`.
pub fn domain_reads_from_kinds<'a>(
    vocab: &crate::rel_schema::RelVocab,
    texts: impl Iterator<Item = &'a str>,
) -> BTreeSet<String> {
    use lute_syntax::datalog::BodyLiteral;
    let mut out: BTreeSet<String> = vocab.indexed_state.values().cloned().collect();
    out.extend(vocab.kinds.values().filter_map(|k| k.subset_of.clone()));
    let mut queried = BTreeSet::new();
    for r in &vocab.rules {
        for lit in &r.rule.body {
            match lit {
                BodyLiteral::Pos(a) | BodyLiteral::Neg(a) | BodyLiteral::Count { atom: a, .. } => {
                    queried.insert(a.relation.clone());
                }
                BodyLiteral::Guard { cel, .. } => {
                    crate::usage::queried_relations(cel, &mut queried)
                }
                BodyLiteral::Cmp { .. } => {}
            }
        }
    }
    for text in texts {
        crate::usage::queried_relations(text, &mut queried);
    }
    out.extend(queried.into_iter().filter(|q| vocab.kinds.contains_key(q)));
    out
}

/// Every `Type::Domain(name)` / `Type::Entity(name)` reachable from `ty`,
/// including through the container types — a `{ list: { domain: X } }` slot
/// reads `X` as surely as a bare one does.
fn collect_domain_names(ty: &Type, out: &mut BTreeSet<String>) {
    match ty {
        Type::Domain(name) | Type::Entity(name) => {
            out.insert(name.clone());
        }
        Type::List(inner) => collect_domain_names(inner, out),
        Type::Map { key, value } => {
            collect_domain_names(key, out);
            collect_domain_names(value, out);
        }
        Type::Record(fields) => {
            for f in fields {
                collect_domain_names(&f.ty, out);
            }
        }
        _ => {}
    }
}

/// `W-DOMAIN-UNREAD` over a resolved project root (dsl 0.10.0 §11.1, **D-V**).
///
/// The declared set and the read set are both UNIONED across every document
/// under the root before the difference is taken: a domain declared in a shared
/// schema is read by *some* document, and warning on the scene that happens not
/// to read it would be a false positive on the most common layout there is.
///
/// One diagnostic per unread DOMAIN, not per declaring document, anchored at
/// its declaration (dsl 0.24 T3-6): the schema file and line an import
/// resolved it from, or the `enums:` / `entities:` key of the byte-sorted-first
/// document declaring it inline. A domain neither places (a plugin's) falls
/// back to that document's frontmatter span. The path of an imported home is
/// the canonical schema path; the caller prints it walk-relative.
///
/// [`Layer::Staging`], matching `E-DOMAIN-UNKNOWN` — the same fact asked in the
/// other direction, so the two must not land on different layers.
pub fn check_project_domain_reads(
    per_file: &[(PathBuf, &crate::check::DomainUse)],
) -> Vec<(PathBuf, Diagnostic)> {
    use crate::check::DomainHome;
    let mut declared: BTreeSet<&str> = BTreeSet::new();
    let mut read: BTreeSet<&str> = BTreeSet::new();
    for (_, u) in per_file {
        declared.extend(u.declared.iter().map(String::as_str));
        read.extend(u.read.iter().map(String::as_str));
    }
    let mut sorted: Vec<&(PathBuf, &crate::check::DomainUse)> = per_file.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));

    let mut out = Vec::new();
    for name in declared.difference(&read) {
        let imported = sorted.iter().find_map(|(_, u)| match u.homes.get(*name) {
            Some(DomainHome::Imported(o)) => Some((o.file.clone(), o.span)),
            _ => None,
        });
        let local = || {
            sorted.iter().find_map(|(p, u)| match u.homes.get(*name) {
                Some(DomainHome::Local(span)) => Some((p.clone(), *span)),
                _ => None,
            })
        };
        let first = || {
            sorted
                .iter()
                .find(|(_, u)| u.declared.contains(*name))
                .map(|(p, u)| (p.clone(), u.at))
        };
        let Some((path, span)) = imported.or_else(local).or_else(first) else {
            continue;
        };
        out.push((
            path,
            Diagnostic {
                code: W_DOMAIN_UNREAD.to_string(),
                severity: Severity::Warning,
                message: format!(
                    "domain `{name}` is declared but no active construct reads it: no directive \
                     attribute, content-line slot, or state path is typed `{{ domain: {name} }}`, \
                     no `relations:` entry takes it as an argument, no `per: {name}` state \
                     family or `subsetOf: {name}` kind builds on it, and no rule or condition \
                     queries `{name}(…)` — so it enforces nothing and only reaches the \
                     artifact's `enums` array. Read it, or remove the declaration \
                     (dsl 0.10.0 §11.1)"
                ),
                evidence: None,
                span,
                layer: Layer::Staging,
                fixits: Vec::new(),
                provenance: None,
                covered: Vec::new(),
                related: Vec::new(),
            },
        ));
    }
    out
}
