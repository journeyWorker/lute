//! Subject value domains (dsl §11.2): the [`Domain`] model and inference from
//! the `state:` schema, a component param's type, or a whole-subject `@def`.

use super::*;

/// A concrete, statically-known value an arm can match against.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DomainValue {
    Str(String),
    Bool(bool),
}

/// The inferred value domain of a `<match>` subject (dsl §11.2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Domain {
    /// Finite domain with a known, enumerable set of values.
    Finite(Vec<DomainValue>),
    /// The real line (dsl 0.18.0 §4): a declared `number` subject (schema
    /// decl or component param). Coverage is the union of the arms' closed
    /// intervals ([`NumCoverage`]); exhaustive iff that union is the whole
    /// line.
    Number,
    /// The whole numbers `lo..=hi` of a `number` subject whose range the
    /// schema knows ([`StateSchema::int_ranges`] — `clock.weekday`, dsl
    /// 0.24.0 §1): exhaustive once every one of them is covered, and a
    /// literal matching none of them is foreign.
    IntRange { lo: i64, hi: i64 },
    /// Infinite / unknowable domain (string, opaque, unresolved subject): an
    /// `<otherwise>` is mandatory.
    Infinite,
}

/// The inferred domain of a subject plus whether the subject is maybe-unset.
/// `pub`: shared with `decide.rs`'s R2 (§5.1 finite-domain membership) and
/// re-exported at the crate root for `lute-compile`/`lute-trace` (0.4.0 T2).
#[derive(Clone, Debug, PartialEq)]
pub struct DomainInfo {
    pub domain: Domain,
    pub maybe_unset: bool,
    /// Whether the subject was actually resolved against the schema: a
    /// known `bool`/`enum` decl, a `scene.choices.*` branch with folded
    /// members, or any other declared decl (`Domain::Number`/
    /// `Domain::Infinite` included — e.g. a declared `number` or `string`).
    /// `false` when `infer_domain` has NO schema knowledge about the subject
    /// at all (an unparseable `on=`, an undeclared path, or
    /// `scene.choices.*` with members not yet folded).
    /// `E-WHEN-LITERAL-DOMAIN` (0.4.0 §5.2) requires `resolved` before
    /// claiming anything about the domain — an undeclared path already gets
    /// its own `E-UNDECLARED` elsewhere, and piling a domain claim atop it
    /// would be exactly the kind of unprovable pile-on §5.1's Closure
    /// clause forbids.
    pub resolved: bool,
}

/// Infer the subject's value domain (dsl §11.2). A `bool`/`enum` decl or a
/// `scene.choices.<id>` path is FINITE; a `number` decl is the real line
/// ([`Domain::Number`], dsl 0.18.0 §4); anything else is INFINITE (requires
/// `<otherwise>`). Maybe-unset: a `scene.choices.*` subject always (a branch may
/// not have been reached), or a `run.*`/`user.*`/`app.*` decl with no `default`.
pub(crate) fn infer_domain(subject: Option<&str>, schema: &StateSchema) -> DomainInfo {
    let Some(path) = subject else {
        return DomainInfo {
            domain: Domain::Infinite,
            maybe_unset: false,
            resolved: false,
        };
    };
    // `scene.choices.<branchId>`: domain = branch choice ids ∪ `unset` (§11.1).
    if path.strip_prefix("scene.choices.").is_some() {
        return match enum_members(path, schema) {
            Some(vals) => DomainInfo {
                domain: Domain::Finite(vals),
                maybe_unset: true,
                resolved: true,
            },
            // Members unknown (branch decl not folded in yet) => can't prove
            // coverage OR a domain claim; treat as infinite/unresolved so
            // `<otherwise>` is required and no §5.2 literal check runs.
            None => DomainInfo {
                domain: Domain::Infinite,
                maybe_unset: true,
                resolved: false,
            },
        };
    }
    // 0.21.1 T1-1: `quest.<id>.state` — local, imported, or foreign alike —
    // is the ALWAYS-ASSIGNED lifecycle enum the engine writes: `unset` until
    // the quest activates, then `active`/`complete`/`failed`. `unset` is a
    // MEMBER (the string the runtime stores; the IR domain lists it too), not
    // the CEL-`null` sentinel, so the subject is never maybe-unset: `== 'unset'`
    // is an ordinary comparison, `<when is="unset">` names the member
    // ([`quest_state_is_literal`]), and `== null` can never hold.
    if crate::cel_paths::is_reserved_quest_state(path) {
        return DomainInfo {
            domain: Domain::Finite(
                QUEST_STATES
                    .iter()
                    .map(|s| DomainValue::Str((*s).to_string()))
                    .collect(),
            ),
            maybe_unset: false,
            resolved: true,
        };
    }
    // dsl 0.24.0 §2: the engine-derived failure reason and objective failure
    // flag are never folded into a schema — every quest's, local or foreign,
    // is typed here by shape. Both are always assigned: `failedBy` holds the
    // member `unset` until the quest fails ([`quest_state_is_literal`]), and
    // `failed` is `false` until the objective fails.
    if crate::cel_paths::is_reserved_quest_failed_by(path) {
        return DomainInfo {
            domain: Domain::Finite(
                crate::cel_paths::QUEST_FAILED_BY
                    .iter()
                    .map(|s| DomainValue::Str((*s).to_string()))
                    .collect(),
            ),
            maybe_unset: false,
            resolved: true,
        };
    }
    if crate::cel_paths::is_reserved_quest_objective_failed(path) {
        return DomainInfo {
            domain: Domain::Finite(vec![DomainValue::Bool(true), DomainValue::Bool(false)]),
            maybe_unset: false,
            resolved: true,
        };
    }
    match schema.decls.get(path) {
        Some(decl) => {
            // dsl 0.27.0 §2: a `{ domain: K }` / `{ entity: K }` path is as
            // finite as an inline enum (`StateSchema::string_members`).
            let members = schema.string_members(path);
            let domain = match (&decl.ty, members) {
                (_, Some(members)) => Domain::Finite(
                    members
                        .iter()
                        .map(|m| DomainValue::Str(m.clone()))
                        .collect(),
                ),
                (Type::Bool, _) => {
                    Domain::Finite(vec![DomainValue::Bool(true), DomainValue::Bool(false)])
                }
                (Type::Number, _) => match schema.int_ranges.get(path) {
                    Some(&(lo, hi)) => Domain::IntRange { lo, hi },
                    None => Domain::Number,
                },
                _ => Domain::Infinite,
            };
            // dsl 0.26.0 §5: `occasion.target` is bound whenever its kind
            // beat runs.
            let maybe_unset = decl.default.is_none()
                && path != crate::beats::OCCASION_TARGET
                && matches!(
                    decl.namespace,
                    Namespace::Scene
                        | Namespace::Run
                        | Namespace::User
                        | Namespace::App
                        | Namespace::Quest
                );
            DomainInfo {
                domain,
                maybe_unset,
                resolved: true,
            }
        }
        None => {
            // RC3 (dsl 0.2.0 §5.2): reserved `quest.<id>.objectives.<oid>.done`
            // / `activatedAt` (and `entry.<id>.read`) reads are admitted
            // UNCONDITIONALLY (`cel_resolve.rs::is_declared`), even for a
            // quest THIS document never locally folds (foreign/imported).
            // Synthesize the SAME domain info the local fold would give, so
            // a foreign one gets identical exhaustiveness treatment.
            // (`quest.<id>.state` returned above, before the schema lookup.)
            if is_reserved_quest_objective_done(path) || is_reserved_entry_read(path) {
                DomainInfo {
                    domain: Domain::Finite(vec![DomainValue::Bool(true), DomainValue::Bool(false)]),
                    // `check_quest` seeds this decl with `default: Some(false)`;
                    // `entry.<id>.read` (dsl 0.19.0 §5) is the same `bool`
                    // defaulting to `false` (`crate::lore::entry_read_decl`).
                    maybe_unset: false,
                    resolved: true,
                }
            } else if is_reserved_quest_activated_at(path) {
                // dsl 0.8.0 §5: narrative time is OPAQUE — no enumerable
                // domain, so `Domain::Infinite` exactly as `infer_domain`'s
                // `Some(decl)` arm computes for the LOCALLY folded
                // `Type::NarrativeTime` decl (`_ => Domain::Infinite`).
                // `resolved: true` (the path IS declared, its domain simply
                // is not finite) and `maybe_unset: true` (`default: None` in
                // `Namespace::Quest`) mirror that arm's verdict too.
                DomainInfo {
                    domain: Domain::Infinite,
                    maybe_unset: true,
                    resolved: true,
                }
            } else {
                DomainInfo {
                    domain: Domain::Infinite,
                    maybe_unset: false,
                    resolved: false,
                }
            }
        }
    }
}

/// The domain a component param's declared TYPE induces (dsl 0.4.0 §6.3):
/// `Bool` -> `Finite[true, false]`; `Enum(members)` -> `Finite(members)`
/// (declaration order); `Number` -> `Number` (the real line, dsl 0.18.0 §4);
/// `{ domain: K }` -> `Finite` over K's members when `domains` declares K
/// closed (the clock's `clock.slot` / `clock.weekdayLabel` included), so a
/// component names the host's vocabulary instead of copying it; `Str`/anything else -> `Infinite` (`<otherwise>`
/// REQUIRED, `E-NONEXHAUSTIVE`). ALWAYS `maybe_unset: false,
/// resolved: true` — every `::use` binds every param (`E-COMPONENT-ARG`
/// enforces count/type, dsl §13.3), so `unset` is never a member of a
/// param's domain: `is="unset"` on a param subject is
/// `E-WHEN-LITERAL-DOMAIN` (rule 3), and `E-UNSET-UNCOVERED` — gated on
/// `maybe_unset` in [`check_match_with_domain`] — is structurally
/// unreachable for a param-subject `<match>`.
pub(crate) fn param_domain(
    ty: &Type,
    domains: &BTreeMap<String, lute_manifest::snapshot::Domain>,
) -> DomainInfo {
    let finite = |members: &[String]| {
        Domain::Finite(
            members
                .iter()
                .map(|m| DomainValue::Str(m.clone()))
                .collect(),
        )
    };
    let domain = match ty {
        Type::Bool => Domain::Finite(vec![DomainValue::Bool(true), DomainValue::Bool(false)]),
        Type::Enum(members) => finite(members),
        Type::Number => Domain::Number,
        Type::Domain(name) => match domains.get(name) {
            Some(d) if !d.open => finite(&d.members),
            _ => Domain::Infinite,
        },
        _ => Domain::Infinite,
    };
    DomainInfo {
        domain,
        maybe_unset: false,
        resolved: true,
    }
}

/// The enum members declared at `path`, if the decl is a `Type::Enum`.
fn enum_members(path: &str, schema: &StateSchema) -> Option<Vec<DomainValue>> {
    match &schema.decls.get(path)?.ty {
        Type::Enum(members) => Some(
            members
                .iter()
                .map(|m| DomainValue::Str(m.clone()))
                .collect(),
        ),
        _ => None,
    }
}

/// Reconstruct the subject's dotted path (`run.rank`, `scene.choices.x`). Returns
/// `None` for a non-path subject (`isSet(run.x)`, an empty/missing `on=`) — an
/// unresolved subject is treated as an infinite domain.
pub(crate) fn subject_path(m: &Match) -> Option<String> {
    let expr = parse_expr(&m.subject.raw)?;
    crate::cel_paths::select_path(&expr)
}

/// `m`'s subject for domain inference (dsl §11.2): its state path, when it has
/// one, and its domain. dsl 0.24.0: a whole-subject `@def` (`on="@wd"`,
/// `on="@f(x)"`) is resolved through its expanded body — a body that is one
/// state path (`wd2: "run.wd"`) is that path, domain included; any other body
/// takes the domain of the def's declared or inferred result type, maybe-unset
/// exactly when the body makes a read that is neither defaulted nor guarded
/// inside it. Every other subject is [`subject_path`] + [`infer_domain`].
pub(crate) fn resolve_subject(
    m: &Match,
    defs: &crate::cel_expand::DefTable<'_>,
    def_types: &BTreeMap<String, Type>,
    schema: &StateSchema,
) -> (Option<String>, DomainInfo) {
    if let Some(resolved) = def_subject(&m.subject.raw, defs, def_types, schema) {
        return resolved;
    }
    let path = subject_path(m);
    let info = infer_domain(path.as_deref(), schema);
    (path, info)
}

/// [`resolve_subject`] for a subject that is exactly one `@def` use; `None`
/// for anything else, or a def that does not expand (another pass reports it).
fn def_subject(
    raw: &str,
    defs: &crate::cel_expand::DefTable<'_>,
    def_types: &BTreeMap<String, Type>,
    schema: &StateSchema,
) -> Option<(Option<String>, DomainInfo)> {
    let text = raw.trim();
    // The first ref scanned is the outermost: a call's argument refs follow it.
    let r = lute_cel::scan_refs(text).into_iter().next()?;
    let end = r.call.as_ref().map_or(r.span.byte_end, |c| c.span.byte_end);
    if r.is_dollar || r.span.byte_start != 0 || end != text.len() {
        return None;
    }
    def_subject_of(&r, text, defs, def_types, schema)
}

fn def_subject_of(
    r: &lute_cel::RefUse,
    text: &str,
    defs: &crate::cel_expand::DefTable<'_>,
    def_types: &BTreeMap<String, Type>,
    schema: &StateSchema,
) -> Option<(Option<String>, DomainInfo)> {
    if !defs.bodies.contains_key(&r.name) {
        return None;
    }
    let expanded = crate::cel_expand::expand_cel(text, defs, None, &mut Vec::new()).ok()?;
    let expr = parse_expr(&expanded)?;
    if let Some(path) = crate::cel_paths::select_path(&expr) {
        let info = infer_domain(Some(&path), schema);
        return Some((Some(path), info));
    }
    let ty = def_types.get(&r.name)?;
    let mut info = param_domain(ty, &BTreeMap::new());
    info.maybe_unset = crate::defassign::may_read_unset(&expr, schema);
    Some((None, info))
}
