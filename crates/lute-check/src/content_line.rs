//! Built-in content-line (`@speaker{…}:`) attribute schema (dsl 0.1.0 §7.1, §12.1).
//! Content lines are NOT capability-schema-driven; their attribute set is a fixed
//! part of the scene-kind vocabulary, validated here — EXCEPT `emotion`/`action`,
//! which are domain-typed (data-catalog foundation A5) and resolve through the
//! SAME merged-vocabulary resolver (`crate::directives::check_domain_member`) a
//! `{domain: X}`-typed directive attr uses, not a bespoke local list.

use std::collections::BTreeMap;

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::provider::ProviderSet;
use lute_manifest::snapshot::{CapabilitySnapshot, Domain};
use lute_syntax::ast::{AttrValue, Line};

use crate::directives::check_domain_member;

/// Known content-line attribute keys (dsl 0.1.0 §7.1). Mirrors the `get(...)`
/// reads in `lute-compile`'s `lower_line`.
const KNOWN_ATTRS: &[&str] = &[
    "code", "emotion", "variant", "action", "dialogMotion", "delivery", "as",
];

const DELIVERY_DOMAIN: &[&str] = &["spoken", "thought", "voiceover"];

pub const E_UNKNOWN_ATTR: &str = "E-UNKNOWN-ATTR";
pub const E_DELIVERY_VALUE: &str = "E-DELIVERY-VALUE";
pub const E_DELIVERY_NARRATOR: &str = "E-DELIVERY-NARRATOR";

fn err(code: &str, message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity: Severity::Error,
        message,
        span,
        layer: Layer::Content,
        fixits: Vec::new(),
        provenance: None,
    }
}

/// Validate a content line's attributes: unknown keys, the `delivery` domain,
/// the narrator-forbids-delivery rule (dsl 0.1.0 §12.1), and the `emotion`/
/// `action` domain-typed values (data-catalog foundation A5).
///
/// `snapshot`/`providers`/`domains` are the SAME merged capability surface
/// `check()` threads through the `Walker` (`domains` computed once in
/// `check()` — see `check.rs`'s `merge_domains` call — and passed here
/// unchanged) so `emotion`/`action` resolve through the exact same
/// vocabulary a `{domain: X}`-typed directive attr would.
pub fn check_content_line_attrs(
    line: &Line,
    snapshot: &CapabilitySnapshot,
    providers: &ProviderSet,
    domains: &BTreeMap<String, Domain>,
    diags: &mut Vec<Diagnostic>,
) {
    for attr in &line.attrs {
        if !KNOWN_ATTRS.contains(&attr.key.as_str()) {
            diags.push(err(
                E_UNKNOWN_ATTR,
                format!("unknown content-line attribute `{}` (dsl 0.1.0 §7.1)", attr.key),
                attr.span,
            ));
            continue;
        }
        match attr.key.as_str() {
            "delivery" => {
                if line.speaker == "narrator" {
                    diags.push(err(
                        E_DELIVERY_NARRATOR,
                        "`delivery` is not allowed on `narrator` — narration takes no delivery \
                         (dsl 0.1.0 §12.1)".to_string(),
                        attr.span,
                    ));
                }
                if let AttrValue::Str(v) = &attr.value {
                    if !DELIVERY_DOMAIN.contains(&v.as_str()) {
                        diags.push(err(
                            E_DELIVERY_VALUE,
                            format!(
                                "unknown `delivery` value `{v}`; expected one of \
                                 spoken|thought|voiceover (dsl 0.1.0 §12.1)"
                            ),
                            attr.span,
                        ));
                    }
                }
            }
            // `emotion`: a CLOSED lute.core baseline enum-style domain (seeded in
            // A2) — always present in the merged `domains` view, so this always
            // resolves through the closed-membership step of the shared resolver
            // (`E-BAD-ENUM` on a non-member). No bespoke local emotion list.
            //
            // `check_domain_member` is shared with `{domain: X}`-typed directive
            // attrs and stamps `Layer::Staging` on everything it emits; content
            // lines are `Layer::Content` diagnostics (dsl 0.1.0 §7.1), so we
            // collect into a scratch vec and re-layer before folding into `diags`.
            "emotion" => {
                let mut scratch = Vec::new();
                check_domain_member(&line.speaker, "emotion", attr, domains, snapshot, providers, &mut scratch);
                for mut d in scratch {
                    d.layer = Layer::Content;
                    diags.push(d);
                }
            }
            // `action`: OPEN by default (preserves 0.1.0's free-string behavior —
            // core ships no `action` domain at all). Only consult the shared
            // resolver when SOMETHING actually declares `action` — a project
            // schema's closed `enums:`/`entities:` (A3) or a plugin-declared
            // `action` provider; core-only docs never trip this branch, so
            // `action="wave"` stays clean with zero domain lookups.
            "action" => {
                if domains.contains_key("action") || snapshot.providers.contains_key("action") {
                    let mut scratch = Vec::new();
                    check_domain_member(&line.speaker, "action", attr, domains, snapshot, providers, &mut scratch);
                    for mut d in scratch {
                        d.layer = Layer::Content;
                        diags.push(d);
                    }
                }
            }
            _ => {}
        }
    }
}
