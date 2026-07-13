//! Directive/attr/enum/providerRef validation against the CapabilitySnapshot
//! (dsl §7.2, plugin §8).
//!
//! `check_directive` resolves a directive's tag against the snapshot, then
//! validates each supplied attribute against its declared `AttrDecl` and reports
//! missing required attributes. Diagnostics carry accurate spans
//! (`directive.span` for the tag, `attr.span` for the attr key, `attr.value_span`
//! for the value) and sit at `Layer::Staging`.
//!
//! ## Snapshot-API degradation
//! - **Inactive-plugin fix-it (plugin §11.2):** when the resolved
//!   [`CapabilitySnapshot`] exposes an installed-but-inactive tag index
//!   (`snapshot.inactive`, tag → owning plugin id), an unknown tag present
//!   there yields `E-UNKNOWN-DIRECTIVE` carrying an "activate plugin" fix-it
//!   naming the plugin. A truly-unknown tag still emits the plain error with
//!   no fix-it.
//! - **`EnumFromOption` owner (plugin §7):** a [`DirectiveDecl`] does not record
//!   which plugin declared it, so we cannot scope the option lookup to the
//!   owning plugin. We best-effort resolve the option across all resolved
//!   plugins; if no plugin resolves that option to a string list we skip the
//!   membership check rather than emit a false `E-BAD-ENUM`.

use std::collections::BTreeMap;

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::asset::{self, AssetIssue, DecomposeError};
use lute_manifest::provider::{IdStatus, ProviderSet};
use lute_manifest::schema::AttrDecl;
use lute_manifest::snapshot::{CapabilitySnapshot, Domain};
use lute_manifest::types::{type_accepts, Literal, Type};
use lute_syntax::ast::{Attr, AttrValue, Directive};

use crate::ctx::Ctx;

/// `E-AT-CONTEXT`: the timeline-position key `at` on a directive OUTSIDE a
/// `<track>` clip (dsl §7.5). `at` is reserved to staging directives inside a
/// `<track>`; the parser strips it from track clips, so any `at` still present
/// on a directive reaching [`check_directive`] is a non-track use — a dedicated
/// diagnostic, never an `E-UNKNOWN-ATTR` fallthrough.
pub const E_AT_CONTEXT: &str = "E-AT-CONTEXT";

/// `Some(E-AT-CONTEXT)` when `dir` carries the reserved timeline-position key
/// `at` (dsl §7.5). `at` is valid only on a staging directive INSIDE a `<track>`;
/// the parser strips it from track clips, so any `at` still present on a
/// directive-form node — a plain `::directive` OR a reserved `::use` — is a
/// non-track use. Callers ([`check_directive`] and `check_use`) push this so
/// `at` never falls through to `E-UNKNOWN-ATTR` / `E-COMPONENT-ARG`.
pub fn at_context(dir: &Directive) -> Option<Diagnostic> {
    dir.attrs.iter().any(|a| a.key == "at").then(|| {
        diag(
            E_AT_CONTEXT,
            Severity::Error,
            format!(
                "`at` is valid only on a <track> clip; `::{}` here is not a timeline clip (dsl §7.5)",
                dir.tag
            ),
            dir.span,
        )
    })
}

/// Validate a single directive against the resolved capability snapshot
/// (dsl §7.2, plugin §8). Returns every diagnostic the directive produces; an
/// empty vec means the directive and all its attributes are well-formed.
///
/// `domains` is the FULL merged domain vocabulary (data-catalog foundation
/// A4) — `snapshot.domains` (A2 baseline) UNION project-authored domains from
/// schema imports (A3's `schema_import::merge_domains`) — computed ONCE by
/// the caller and threaded by reference so `Type::Domain(name)` attrs resolve
/// without recomputing the union per attribute.
///
/// `_ctx` is threaded for parity with the other `check_*` entrypoints and for
/// the match-scope hooks later tasks consume; T4.2 does not branch on it.
pub fn check_directive(
    dir: &Directive,
    snapshot: &CapabilitySnapshot,
    providers: &ProviderSet,
    domains: &BTreeMap<String, Domain>,
    _ctx: &Ctx<'_>,
) -> Vec<Diagnostic> {
    let mut diags = Vec::new();

    let Some(decl) = snapshot.directive(&dir.tag) else {
        // plugin §11.2: an installed-but-inactive tag is a diagnostic WITH a
        // fix-it (naming the plugin to activate), never silently accepted; a
        // truly-unknown tag still yields the plain staging-layer error.
        let mut fixits = Vec::new();
        if let Some(plugin) = snapshot.inactive.get(&dir.tag) {
            fixits.push(lute_core_span::Fixit {
                title: format!(
                    "activate plugin `{plugin}` (add it to your profile or the scene `plugins:` block)"
                ),
                kind: "quickfix".to_string(),
                // Advisory: no auto-applicable text edit (activation is a manual
                // profile/scene change), so a mid confidence.
                edit: Vec::new(),
                confidence: 50,
            });
        }
        diags.push(Diagnostic {
            code: "E-UNKNOWN-DIRECTIVE".to_string(),
            severity: Severity::Error,
            message: format!("unknown directive `::{}`", dir.tag),
            span: dir.span,
            layer: Layer::Staging,
            fixits,
            provenance: None,
            covered: Vec::new(),
            related: Vec::new(),
        });
        return diags;
    };

    // E-AT-CONTEXT (dsl §7.5): reserved `at` outside a <track> clip. Shared with
    // `check_use` so it fires for EVERY directive-form node (::directive AND
    // ::use); track clips have `at` stripped by the parser, so they never trip
    // it. Emitted here so it never falls through to E-UNKNOWN-ATTR below.
    if let Some(d) = at_context(dir) {
        diags.push(d);
    }

    // Per-attribute validation.
    for attr in &dir.attrs {
        // `at` is handled above (E-AT-CONTEXT); never E-UNKNOWN-ATTR here.
        if attr.key == "at" {
            continue;
        }
        let Some(adecl) = decl.attrs.iter().find(|a| a.name == attr.key) else {
            // dsl §7.5: `duration`/`delay`/`wait` are cross-cutting reserved
            // timing keys valid on ANY directive (core or plugin) even when
            // its own schema does not declare them -- plugins are in fact
            // FORBIDDEN from declaring them (assembly-time
            // E-PLUGIN-RESERVED-NAME, plugin §10). Type-check the undeclared
            // use exactly like a directive that DID declare it would, via a
            // synthetic decl, so it never falls through to E-UNKNOWN-ATTR. A
            // directive that DOES declare one of these (core camera/video)
            // never reaches this branch -- the `find` above already matched.
            if let Some(universal) = universal_timing_decl(&attr.key) {
                check_attr_value(
                    &dir.tag, &universal, attr, snapshot, providers, domains, &mut diags,
                );
                continue;
            }
            diags.push(diag(
                "E-UNKNOWN-ATTR",
                Severity::Error,
                format!("`::{}` has no attribute `{}`", dir.tag, attr.key),
                attr.span,
            ));
            continue;
        };
        check_attr_value(&dir.tag, adecl, attr, snapshot, providers, domains, &mut diags);
    }

    // Missing required attributes (dsl §7.2).
    for adecl in decl.attrs.iter().filter(|a| a.required) {
        if !dir.attrs.iter().any(|a| a.key == adecl.name) {
            diags.push(diag(
                "E-MISSING-ATTR",
                Severity::Error,
                format!("`::{}` requires attribute `{}`", dir.tag, adecl.name),
                dir.span,
            ));
        }
    }

    diags
}

/// Synthetic [`AttrDecl`] for an undeclared reserved timing key, so
/// [`check_attr_value`] type-checks an undeclared `duration`/`delay`/`wait`
/// exactly like a directive that DID declare it would (dsl §7.5, §11.3):
/// `duration`/`delay` are `number`, `wait` is `bool`. Returns `None` for any
/// other key -- callers fall through to `E-UNKNOWN-ATTR` in that case. Never
/// `required` (a directive that omits it simply gets no timing behavior; the
/// compiler already treats an absent `wait` as non-blocking and an absent
/// `duration`/`delay` as zero -- dsl §11.3, §11.4).
fn universal_timing_decl(key: &str) -> Option<AttrDecl> {
    let ty = match key {
        "duration" | "delay" => Type::Number,
        "wait" => Type::Bool,
        _ => return None,
    };
    Some(AttrDecl {
        name: key.to_string(),
        required: false,
        ty,
        default: None,
    })
}

/// Validate one supplied attribute's value against its declared type.
///
/// A `Ref` (CEL `@expr`) value is left untyped here — CEL type/scope resolution
/// is Task 4.3's job — so only literal `Str`/`BoolTrue` values are checked.
fn check_attr_value(
    tag: &str,
    adecl: &AttrDecl,
    attr: &Attr,
    snapshot: &CapabilitySnapshot,
    providers: &ProviderSet,
    domains: &BTreeMap<String, Domain>,
    diags: &mut Vec<Diagnostic>,
) {
    // CEL-valued attributes are resolved in T4.3, not here.
    if matches!(attr.value, AttrValue::Ref(_)) {
        return;
    }

    match &adecl.ty {
        Type::Enum(members) => {
            check_enum_member(tag, &attr.key, members, attr, diags);
        }
        Type::EnumFromOption(opt) => {
            // Owning plugin/option unresolvable from the snapshot API: skip
            // rather than emit a false E-BAD-ENUM (see module docs).
            if let Some(members) = resolve_option_domain(snapshot, opt) {
                check_enum_member(tag, &attr.key, &members, attr, diags);
            }
        }
        Type::ProviderRef(provider) => {
            check_provider_ref(provider, attr, providers, diags);
        }
        Type::Domain(name) => {
            check_domain_member(tag, name, attr, domains, snapshot, providers, diags);
        }
        Type::AssetKind(kind) => check_asset_id(kind, attr, snapshot, providers, diags),
        ty => {
            if let Some(lit) = literal_of(ty, &attr.value) {
                if !type_accepts(ty, &lit) {
                    diags.push(diag(
                        "E-ATTR-TYPE",
                        Severity::Error,
                        format!(
                            "attribute `{}` of `::{tag}` expects {}",
                            attr.key,
                            describe(ty)
                        ),
                        attr.value_span,
                    ));
                }
            } else {
                diags.push(diag(
                    "E-ATTR-TYPE",
                    Severity::Error,
                    format!(
                        "attribute `{}` of `::{tag}` expects {}",
                        attr.key,
                        describe(ty)
                    ),
                    attr.value_span,
                ));
            }
        }
    }
}

/// Enum-membership check shared by `Type::Enum` and resolved `EnumFromOption`.
fn check_enum_member(
    tag: &str,
    key: &str,
    members: &[String],
    attr: &Attr,
    diags: &mut Vec<Diagnostic>,
) {
    let ok = match &attr.value {
        AttrValue::Str(s) => members.iter().any(|m| m == s),
        // A bare-ident (`true`) is never a valid enum member.
        AttrValue::BoolTrue => false,
        AttrValue::Ref(_) => return,
    };
    if !ok {
        let got = match &attr.value {
            AttrValue::Str(s) => s.clone(),
            AttrValue::BoolTrue => "true".to_string(),
            AttrValue::Ref(_) => unreachable!(),
        };
        diags.push(diag(
            "E-BAD-ENUM",
            Severity::Error,
            format!(
                "`{got}` is not a valid value for `{key}` of `::{tag}` (expected one of: {})",
                members.join(", ")
            ),
            attr.value_span,
        ));
    }
}

/// Resolve a `providerRef` id against the pinned provider set (plugin §10):
/// `Fresh` → ok, `Stale` → `W-CATALOG-STALE` warning, `Absent` → `E-UNKNOWN-ID`.
fn check_provider_ref(
    provider: &str,
    attr: &Attr,
    providers: &ProviderSet,
    diags: &mut Vec<Diagnostic>,
) {
    let id = match &attr.value {
        AttrValue::Str(s) => s.as_str(),
        // A bare-ident value cannot name a provider id.
        AttrValue::BoolTrue => {
            diags.push(diag(
                "E-ATTR-TYPE",
                Severity::Error,
                format!("attribute `{}` expects a `{provider}` id string", attr.key),
                attr.value_span,
            ));
            return;
        }
        AttrValue::Ref(_) => return,
    };
    match providers.contains(provider, id) {
        IdStatus::Fresh => {}
        IdStatus::Stale => diags.push(diag(
            "W-CATALOG-STALE",
            Severity::Warning,
            format!("`{id}` not found in `{provider}` catalog (snapshot is stale/offline)"),
            attr.value_span,
        )),
        IdStatus::Absent => diags.push(diag(
            "E-UNKNOWN-ID",
            Severity::Error,
            format!("`{id}` is not a known `{provider}` id"),
            attr.value_span,
        )),
    }
}

/// Resolve a `{domain: name}`-typed attr value against the FULL merged
/// domain vocabulary (data-catalog foundation A4): `domains` is
/// `snapshot.domains` (A2 — core baseline + active-plugin `enums`) UNION
/// project-authored domains lifted from schema imports (A3's
/// `schema_import::merge_domains`), computed once by the caller.
///
/// Resolution order (structurally a string per `types::type_accepts` — a
/// bare-ident `true` is a hard `E-ATTR-TYPE` before any of this runs):
/// 1. `name` resolves to a CLOSED domain (`Domain.open == false`) — that
///    domain's own membership is AUTHORITATIVE, even over a same-named
///    declared provider: reuse [`check_enum_member`] verbatim (the SAME
///    membership check `Type::Enum` uses); a non-member is `E-BAD-ENUM`.
/// 2. `name` is a declared provider (`snapshot.providers`) — provider-backed:
///    reuse [`check_provider_ref`] (id membership, not static members). This
///    covers OPEN/registry-backed entity names (A3: ids minted by the engine
///    at runtime) AND any name absent from `domains` entirely that doubles
///    as a provider reference.
/// 3. `name` resolves to an OPEN domain (`Domain{open:true}`) that is NOT a
///    provider — accept any string; NEVER closed-checked.
/// 4. else — `name` is not a known domain at all: `E-DOMAIN-UNKNOWN`.
///
/// `pub(crate)`: content-line `emotion`/`action` (data-catalog foundation A5,
/// `crate::content_line::check_content_line_attrs`) call this DIRECTLY so
/// their domain-typed values resolve through the exact SAME resolver a
/// `{domain: X}`-typed directive attr uses, rather than a bespoke second
/// membership check.
pub(crate) fn check_domain_member(
    tag: &str,
    name: &str,
    attr: &Attr,
    domains: &BTreeMap<String, Domain>,
    snapshot: &CapabilitySnapshot,
    providers: &ProviderSet,
    diags: &mut Vec<Diagnostic>,
) {
    // Structural: a domain-typed value is always a string (types.rs
    // `type_accepts`); a bare-ident `true` cannot name a domain member/id.
    if matches!(attr.value, AttrValue::BoolTrue) {
        diags.push(diag(
            "E-ATTR-TYPE",
            Severity::Error,
            format!("attribute `{}` expects a `{name}` domain id string", attr.key),
            attr.value_span,
        ));
        return;
    }
    let dom = domains.get(name);
    if let Some(dom) = dom.filter(|d| !d.open) {
        // Step 1: a CLOSED domain's own membership wins — even over a
        // same-named provider (foundation A4 order; A5 reuses this resolver
        // for content-line `emotion`/`action`).
        check_enum_member(tag, &attr.key, &dom.members, attr, diags);
    } else if snapshot.providers.contains_key(name) {
        // Step 2: provider-backed (OPEN/registry-backed entity names, or a
        // name outside `domains` entirely that is a provider reference).
        check_provider_ref(name, attr, providers, diags);
    } else if dom.is_some() {
        // Step 3: OPEN, non-provider domain — registry-style ids minted by
        // the engine at runtime (A3); always-accept, never closed-checked.
    } else {
        // Step 4: not a known domain at all.
        diags.push(diag(
            "E-DOMAIN-UNKNOWN",
            Severity::Error,
            format!(
                "`{name}` is not a known domain — declared by neither the plugin/core \
                 vocabulary nor a project schema (dsl data-catalog foundation, `::{tag}` \
                 attribute `{}`)",
                attr.key
            ),
            attr.value_span,
        ));
    }
}

/// Validate an authored `assetId` against its declared `assetKind` (plugin §6.9,
/// precedence step-1 checker half): a `PLACEHOLDER_*` sentinel warns; a segment-
/// less query kind checks provider-existence only; a segment-bearing kind
/// decomposes + validates each segment. The engine (compose/query-from-attrs,
/// fallback resolution) is out of scope — this only validates an AUTHORED id.
fn check_asset_id(
    kind_name: &str,
    attr: &Attr,
    snapshot: &CapabilitySnapshot,
    providers: &ProviderSet,
    diags: &mut Vec<Diagnostic>,
) {
    let id = match &attr.value {
        AttrValue::Str(s) => s.as_str(),
        AttrValue::BoolTrue => {
            diags.push(diag(
                "E-ATTR-TYPE",
                Severity::Error,
                format!("attribute `{}` expects an asset id string", attr.key),
                attr.value_span,
            ));
            return;
        }
        AttrValue::Ref(_) => return,
    };
    // §6.9: a PLACEHOLDER_* sentinel resolves to itself; surfaced as a warning.
    if asset::is_placeholder(id) {
        diags.push(diag(
            "W-ASSET-PLACEHOLDER",
            Severity::Warning,
            format!("`{id}` is a placeholder asset id (resolve before release)"),
            attr.value_span,
        ));
        return;
    }
    // Defensive: assembly should have provided the kind; if not, skip silently.
    let Some(kind) = snapshot.asset_kinds.get(kind_name) else {
        return;
    };

    if kind.segments.is_empty() {
        // pure-query kind: provider-existence only (decompose would give one
        // opaque value).
        if let Some(provider) = &kind.provider {
            match providers.contains(provider, id) {
                IdStatus::Fresh => {}
                IdStatus::Stale => diags.push(diag(
                    "W-CATALOG-STALE",
                    Severity::Warning,
                    format!("`{id}` not found in `{provider}` catalog (snapshot stale/offline)"),
                    attr.value_span,
                )),
                IdStatus::Absent => diags.push(diag(
                    "E-ASSET-UNKNOWN-ID",
                    Severity::Error,
                    format!("`{id}` is not a known `{provider}` asset"),
                    attr.value_span,
                )),
            }
        }
        return;
    }
    // segment-bearing kind: decompose then per-segment validate.
    match asset::decompose(kind, id) {
        Err(DecomposeError::Arity { expected, found }) => diags.push(diag(
            "E-ASSET-DECOMPOSE",
            Severity::Error,
            format!(
                "asset id `{id}` has {found} segment(s), expected {expected} for kind `{kind_name}`"
            ),
            attr.value_span,
        )),
        Err(DecomposeError::ConstMismatch {
            name,
            expected,
            found,
        }) => diags.push(diag(
            "E-ASSET-DECOMPOSE",
            Severity::Error,
            format!("asset id `{id}` segment `{name}` must be `{expected}`, found `{found}`"),
            attr.value_span,
        )),
        Ok(segs) => {
            for issue in asset::validate_segments(kind, &segs, providers) {
                match issue {
                    AssetIssue::StaleProviderId {
                        segment,
                        provider,
                        value,
                    } => diags.push(diag(
                        "W-CATALOG-STALE",
                        Severity::Warning,
                        format!(
                            "`{value}` not found in `{provider}` catalog (snapshot stale/offline; segment `{segment}`)"
                        ),
                        attr.value_span,
                    )),
                    AssetIssue::BadConst {
                        segment,
                        expected,
                        found,
                    } => diags.push(diag(
                        "E-ASSET-SEGMENT",
                        Severity::Error,
                        format!("segment `{segment}` must be `{expected}`, found `{found}`"),
                        attr.value_span,
                    )),
                    AssetIssue::NotEnumMember {
                        segment,
                        value,
                        members,
                    } => diags.push(diag(
                        "E-ASSET-SEGMENT",
                        Severity::Error,
                        format!(
                            "`{value}` is not a valid `{segment}` (expected one of: {})",
                            members.join(", ")
                        ),
                        attr.value_span,
                    )),
                    AssetIssue::NotNumber { segment, value } => diags.push(diag(
                        "E-ASSET-SEGMENT",
                        Severity::Error,
                        format!("segment `{segment}` expects a number, found `{value}`"),
                        attr.value_span,
                    )),
                    AssetIssue::UnknownProviderId {
                        segment,
                        provider,
                        value,
                    } => diags.push(diag(
                        "E-ASSET-SEGMENT",
                        Severity::Error,
                        format!("`{value}` is not a known `{provider}` id (segment `{segment}`)"),
                        attr.value_span,
                    )),
                }
            }
        }
    }
}

/// Best-effort resolve an `EnumFromOption` domain: find any resolved plugin
/// whose `options[opt]` is a string list/enum literal and return its members.
/// Returns `None` when no plugin resolves the option to a string list (see
/// module docs on the missing owning-plugin API).
fn resolve_option_domain(snapshot: &CapabilitySnapshot, opt: &str) -> Option<Vec<String>> {
    for plugin in snapshot.plugins.values() {
        if let Some(Literal::List(items)) = plugin.options.get(opt) {
            let mut members = Vec::with_capacity(items.len());
            for item in items {
                match item {
                    Literal::Str(s) => members.push(s.clone()),
                    // A non-string member means this isn't an enum-of-strings
                    // domain; bail rather than half-resolve.
                    _ => return None,
                }
            }
            return Some(members);
        }
    }
    None
}

/// Map a literal `AttrValue` to a `Literal` in the declared type's domain so
/// `type_accepts` can judge it. Numeric attrs arrive as strings from the parser,
/// so a `Number` target parses the string; a `Bool` target accepts the bare
/// `true` ident or the strings `true`/`false`. Returns `None` when the value
/// cannot be coerced into the target's shape (a hard type error).
fn literal_of(ty: &Type, value: &AttrValue) -> Option<Literal> {
    match (ty, value) {
        (Type::Number, AttrValue::Str(s)) => s.parse::<f64>().ok().map(Literal::Num),
        (Type::Bool, AttrValue::BoolTrue) => Some(Literal::Bool(true)),
        (Type::Bool, AttrValue::Str(s)) => match s.as_str() {
            "true" => Some(Literal::Bool(true)),
            "false" => Some(Literal::Bool(false)),
            _ => None,
        },
        (_, AttrValue::Str(s)) => Some(Literal::Str(s.clone())),
        // A bare-ident `true` against a non-bool scalar is a type error.
        (_, AttrValue::BoolTrue) => None,
        (_, AttrValue::Ref(_)) => None,
    }
}

/// Human-readable name for a scalar type, for `E-ATTR-TYPE` messages.
fn describe(ty: &Type) -> &'static str {
    match ty {
        Type::Bool => "a boolean",
        Type::Number => "a number",
        Type::Str => "a string",
        Type::List(_) => "a list",
        Type::Record(_) => "a record",
        Type::Map { .. } => "a map",
        Type::SlotId { .. } => "a slot id",
        Type::Enum(_) | Type::EnumFromOption(_) => "an enum value",
        Type::ProviderRef(_) => "a provider id",
        Type::Domain(_) => "a domain id",
        Type::AssetKind(_) => "an asset id",
        Type::NarrativeTime => "a narrative-time value",
    }
}

/// Build a staging-layer diagnostic at `span`.
fn diag(code: &str, severity: Severity, message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity,
        message,
        span,
        layer: Layer::Staging,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ctx::Ctx;
    use crate::ctx::Env;
    use lute_core_span::Span;
    use lute_manifest::core::load_core_snapshot;
    use lute_manifest::provider::ProviderSet;
    use lute_manifest::schema::{DirectiveDecl, Lowering};
    use lute_syntax::ast::{Attr, AttrValue, CelKind, CelSlot};
    use std::sync::LazyLock;

    fn span() -> Span {
        Span {
            byte_start: 0,
            byte_end: 0,
            line: 1,
            column: 1,
            utf16_range: (0, 0),
        }
    }

    fn directive(tag: &str, attrs: &[(&str, &str)]) -> Directive {
        Directive {
            tag: tag.to_string(),
            attrs: attrs
                .iter()
                .map(|(k, v)| Attr {
                    key: k.to_string(),
                    value: AttrValue::Str(v.to_string()),
                    value_span: span(),
                    span: span(),
                })
                .collect(),
            span: span(),
        }
    }

    /// A directive with one attr set to an arbitrary [`AttrValue`] (Str/Ref/
    /// BoolTrue), for cases the Str-only `directive()` helper cannot build.
    fn directive_with_value(tag: &str, key: &str, value: AttrValue) -> Directive {
        Directive {
            tag: tag.to_string(),
            attrs: vec![Attr {
                key: key.to_string(),
                value,
                value_span: span(),
                span: span(),
            }],
            span: span(),
        }
    }

    /// Core snapshot plus one plugin-shaped directive (`p`) that declares NO
    /// timing attrs at all -- standing in for a real plugin directive, which
    /// (0.2.1 §7.5 assembly enforcement, commit 791304b) is FORBIDDEN from
    /// declaring `duration`/`delay`/`wait` itself.
    fn plugin_snapshot() -> CapabilitySnapshot {
        let mut snap = load_core_snapshot();
        snap.directives.insert(
            "p".to_string(),
            DirectiveDecl {
                name: "p".to_string(),
                layer: None,
                attrs: vec![AttrDecl {
                    name: "label".to_string(),
                    required: false,
                    ty: Type::Str,
                    default: None,
                }],
                semantics: Vec::new(),
                state: None,
                effects: None,
                bridge: None,
                lower: Lowering::Builtin {
                    kind: "builtin".to_string(),
                    name: "noop".to_string(),
                },
            },
        );
        snap
    }

    fn empty_providers() -> ProviderSet {
        ProviderSet::default()
    }

    fn empty_domains() -> BTreeMap<String, Domain> {
        BTreeMap::new()
    }

    /// The A2 baseline `domains` view (`snapshot.domains`, no project schema
    /// imports) — for tests that exercise a core `{domain: X}`-typed staging
    /// attr (`music.action`/`volume`, `auto.anchor`, `vfx.type` — A5 dedupe).
    /// `empty_domains()` above stays correct for every OTHER test here, none
    /// of which touch a domain-typed attr.
    fn core_domains() -> BTreeMap<String, Domain> {
        load_core_snapshot().domains
    }

    fn ctx() -> Ctx<'static> {
        static ENV: LazyLock<Env> = LazyLock::new(Env::default);
        Ctx {
            env: &ENV,
            in_match: false,
            match_subject: None,
        }
    }

    #[test]
    fn unknown_directive_errors_with_layer_staging() {
        let d = directive("teleport", &[]);
        let errs = check_directive(&d, &load_core_snapshot(), &empty_providers(), &empty_domains(), &ctx());
        assert!(errs
            .iter()
            .any(|e| e.code == "E-UNKNOWN-DIRECTIVE" && e.layer == lute_core_span::Layer::Staging));
    }

    #[test]
    fn bad_enum_value_errors() {
        let d = directive("music", &[("action", "explode")]); // not in musicAction enum
        let errs = check_directive(&d, &load_core_snapshot(), &empty_providers(), &core_domains(), &ctx());
        assert!(errs.iter().any(|e| e.code == "E-BAD-ENUM"));
    }

    #[test]
    fn known_directive_valid_attrs_pass() {
        let d = directive("music", &[("action", "start"), ("mood", "peaceful")]);
        let errs = check_directive(&d, &load_core_snapshot(), &empty_providers(), &core_domains(), &ctx());
        assert!(errs.is_empty(), "{errs:?}");
    }

    #[test]
    fn camera_non_numeric_zoom_errors() {
        // Numeric camera attrs are declared `number`; a non-numeric literal is a
        // check-time type error (closes the coercion seam — Plan C review).
        let d = directive("camera", &[("zoom", "hard")]);
        let errs = check_directive(&d, &load_core_snapshot(), &empty_providers(), &empty_domains(), &ctx());
        assert!(
            errs.iter().any(|e| e.code == "E-ATTR-TYPE"),
            "expected E-ATTR-TYPE for non-numeric zoom, got {errs:?}"
        );
    }

    #[test]
    fn camera_non_numeric_shake_errors() {
        // `::camera{shake="hard"}` must be rejected at check, not silently dropped
        // by the compiler's get_f64 coercion (Plan C review).
        let d = directive("camera", &[("shake", "hard")]);
        let errs = check_directive(&d, &load_core_snapshot(), &empty_providers(), &empty_domains(), &ctx());
        assert!(
            errs.iter().any(|e| e.code == "E-ATTR-TYPE"),
            "expected E-ATTR-TYPE for non-numeric shake, got {errs:?}"
        );
    }

    #[test]
    fn camera_numeric_attrs_pass() {
        // Valid numeric literals for zoom/move-x/move-y/shake still validate.
        let d = directive(
            "camera",
            &[("zoom", "1.1"), ("move-x", "0.2"), ("move-y", "0.3"), ("shake", "0.4")],
        );
        let errs = check_directive(&d, &load_core_snapshot(), &empty_providers(), &empty_domains(), &ctx());
        assert!(errs.is_empty(), "{errs:?}");
    }

    // -- 0.2.1 §7.5: undeclared duration/delay/wait as universal timing attrs --

    #[test]
    fn undeclared_duration_and_wait_on_plugin_directive_no_unknown_attr() {
        // `::p{duration="0.5" wait="true"}` -- plugin directive `p` declares
        // neither key (and, per 791304b, is FORBIDDEN from declaring them);
        // the fallback must accept both instead of E-UNKNOWN-ATTR.
        let d = Directive {
            tag: "p".to_string(),
            attrs: vec![
                Attr {
                    key: "duration".to_string(),
                    value: AttrValue::Str("0.5".to_string()),
                    value_span: span(),
                    span: span(),
                },
                Attr {
                    key: "wait".to_string(),
                    value: AttrValue::Str("true".to_string()),
                    value_span: span(),
                    span: span(),
                },
            ],
            span: span(),
        };
        let errs = check_directive(&d, &plugin_snapshot(), &empty_providers(), &empty_domains(), &ctx());
        assert!(errs.is_empty(), "{errs:?}");
    }

    #[test]
    fn undeclared_delay_on_core_directive_without_timing_decl_no_unknown_attr() {
        // `music` (core) declares no timing attrs at all -- the fallback must
        // apply to core directives just as much as plugin ones.
        let d = directive("music", &[("action", "start"), ("delay", "1.0")]);
        let errs = check_directive(&d, &load_core_snapshot(), &empty_providers(), &core_domains(), &ctx());
        assert!(errs.is_empty(), "{errs:?}");
    }

    #[test]
    fn undeclared_duration_wrong_type_errors() {
        // `::p{duration="soon"}` -- the fallback type-checks; it is not a
        // blanket accept.
        let d = directive("p", &[("duration", "soon")]);
        let errs = check_directive(&d, &plugin_snapshot(), &empty_providers(), &empty_domains(), &ctx());
        assert!(
            errs.iter().any(|e| e.code == "E-ATTR-TYPE"),
            "expected E-ATTR-TYPE for non-numeric duration, got {errs:?}"
        );
        assert!(
            !errs.iter().any(|e| e.code == "E-UNKNOWN-ATTR"),
            "duration must never be E-UNKNOWN-ATTR, got {errs:?}"
        );
    }

    #[test]
    fn undeclared_wait_wrong_type_errors() {
        // `::p{wait="maybe"}` -- not `true`/`false`, so the bool type
        // diagnostic fires just like a declared `wait` attr would.
        let d = directive("p", &[("wait", "maybe")]);
        let errs = check_directive(&d, &plugin_snapshot(), &empty_providers(), &empty_domains(), &ctx());
        assert!(
            errs.iter().any(|e| e.code == "E-ATTR-TYPE"),
            "expected E-ATTR-TYPE for non-bool wait, got {errs:?}"
        );
        assert!(
            !errs.iter().any(|e| e.code == "E-UNKNOWN-ATTR"),
            "wait must never be E-UNKNOWN-ATTR, got {errs:?}"
        );
    }

    #[test]
    fn undeclared_duration_cel_ref_is_left_for_later_resolution() {
        // `::p{duration=@someNumberRef}` -- a CEL ref is skipped here (T4.3's
        // job), same as a declared numeric attr would be; it must not be
        // rejected as a literal type mismatch.
        let d = directive_with_value(
            "p",
            "duration",
            AttrValue::Ref(CelSlot::raw(
                CelKind::AttrValue,
                "someNumberRef".to_string(),
                span(),
            )),
        );
        let errs = check_directive(&d, &plugin_snapshot(), &empty_providers(), &empty_domains(), &ctx());
        assert!(errs.is_empty(), "{errs:?}");
    }

    #[test]
    fn at_on_non_track_directive_still_at_context() {
        // Regression guard: `at` stays context-gated, never folded into the
        // universal-timing fallback.
        let d = directive("music", &[("at", "1.0")]);
        let errs = check_directive(&d, &load_core_snapshot(), &empty_providers(), &empty_domains(), &ctx());
        assert!(
            errs.iter().any(|e| e.code == "E-AT-CONTEXT"),
            "expected E-AT-CONTEXT, got {errs:?}"
        );
        assert!(
            !errs.iter().any(|e| e.code == "E-UNKNOWN-ATTR"),
            "at must never fall through to E-UNKNOWN-ATTR, got {errs:?}"
        );
    }

    #[test]
    fn genuinely_unknown_attr_still_errors() {
        // Regression guard: the fallback is scoped to exactly
        // duration/delay/wait -- any other undeclared key is still
        // E-UNKNOWN-ATTR.
        let d = directive("p", &[("bogus", "1")]);
        let errs = check_directive(&d, &plugin_snapshot(), &empty_providers(), &empty_domains(), &ctx());
        assert!(
            errs.iter().any(|e| e.code == "E-UNKNOWN-ATTR"),
            "expected E-UNKNOWN-ATTR for `bogus`, got {errs:?}"
        );
    }

    #[test]
    fn camera_declared_timing_attrs_still_pass() {
        // Regression guard: core `camera` DECLARES duration/delay/wait
        // (staging.yaml) -- the declared path must keep taking precedence
        // over the fallback and stay clean.
        let d = directive("camera", &[("duration", "0.5"), ("wait", "false")]);
        let errs = check_directive(&d, &load_core_snapshot(), &empty_providers(), &empty_domains(), &ctx());
        assert!(errs.is_empty(), "{errs:?}");
    }
}
