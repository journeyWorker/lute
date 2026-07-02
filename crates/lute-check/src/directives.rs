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

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::provider::{IdStatus, ProviderSet};
use lute_manifest::schema::AttrDecl;
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_manifest::types::{type_accepts, Literal, Type};
use lute_syntax::ast::{Attr, AttrValue, Directive};

use crate::ctx::Ctx;

/// Validate a single directive against the resolved capability snapshot
/// (dsl §7.2, plugin §8). Returns every diagnostic the directive produces; an
/// empty vec means the directive and all its attributes are well-formed.
///
/// `_ctx` is threaded for parity with the other `check_*` entrypoints and for
/// the match-scope hooks later tasks consume; T4.2 does not branch on it.
pub fn check_directive(
    dir: &Directive,
    snapshot: &CapabilitySnapshot,
    providers: &ProviderSet,
    _ctx: &Ctx,
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
        });
        return diags;
    };

    // Per-attribute validation.
    for attr in &dir.attrs {
        let Some(adecl) = decl.attrs.iter().find(|a| a.name == attr.key) else {
            diags.push(diag(
                "E-UNKNOWN-ATTR",
                Severity::Error,
                format!("`::{}` has no attribute `{}`", dir.tag, attr.key),
                attr.span,
            ));
            continue;
        };
        check_attr_value(&dir.tag, adecl, attr, snapshot, providers, &mut diags);
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ctx::Ctx;
    use lute_core_span::Span;
    use lute_manifest::core::load_core_snapshot;
    use lute_manifest::provider::ProviderSet;
    use lute_syntax::ast::{Attr, AttrValue};

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

    fn empty_providers() -> ProviderSet {
        ProviderSet::default()
    }

    fn ctx() -> Ctx {
        Ctx::default()
    }

    #[test]
    fn unknown_directive_errors_with_layer_staging() {
        let d = directive("teleport", &[]);
        let errs = check_directive(&d, &load_core_snapshot(), &empty_providers(), &ctx());
        assert!(errs
            .iter()
            .any(|e| e.code == "E-UNKNOWN-DIRECTIVE" && e.layer == lute_core_span::Layer::Staging));
    }

    #[test]
    fn bad_enum_value_errors() {
        let d = directive("music", &[("action", "explode")]); // not in musicAction enum
        let errs = check_directive(&d, &load_core_snapshot(), &empty_providers(), &ctx());
        assert!(errs.iter().any(|e| e.code == "E-BAD-ENUM"));
    }

    #[test]
    fn known_directive_valid_attrs_pass() {
        let d = directive("music", &[("action", "start"), ("mood", "peaceful")]);
        let errs = check_directive(&d, &load_core_snapshot(), &empty_providers(), &ctx());
        assert!(errs.is_empty(), "{errs:?}");
    }
}
