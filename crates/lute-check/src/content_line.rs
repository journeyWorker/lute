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
use lute_syntax::ast::{Attr, AttrValue, Line};

use crate::directives::check_domain_member;

/// Known content-line attribute keys (dsl 0.2.2 §7.1, §D7). Mirrors the
/// `get(...)`/`attr_bool(...)` reads in `lute-compile`'s `lower_line`.
const KNOWN_ATTRS: &[&str] = &[
    "code", "emotion", "variant", "action", "dialogMotion", "mono", "os", "vo", "as",
];

/// The mutually-exclusive delivery bare flags (dsl 0.2.2 §D7): at most one
/// may be set per content line.
const DELIVERY_FLAGS: &[&str] = &["mono", "os", "vo"];

pub const E_UNKNOWN_ATTR: &str = "E-UNKNOWN-ATTR";
pub const E_DELIVERY_CONFLICT: &str = "E-DELIVERY-CONFLICT";
pub const E_DELIVERY_NARRATOR: &str = "E-DELIVERY-NARRATOR";
pub const E_DELIVERY_FLAG_VALUE: &str = "E-DELIVERY-FLAG-VALUE";

fn err(code: &str, message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity: Severity::Error,
        message,
        span,
        layer: Layer::Content,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
    }
}

/// Validate a content line's attributes: unknown keys, the delivery bare-flag
/// exclusivity + narrator-forbids-delivery rules (dsl 0.2.2 §D7, carried
/// forward from 0.1.0 §12.1), and the `emotion`/`action` domain-typed values
/// (data-catalog foundation A5).
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
    let mut delivery_flags: Vec<&Attr> = Vec::new();
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
            // `mono`/`os`/`vo`: bare boolean delivery flags (dsl 0.2.2 §D7,
            // AttrValue::BoolTrue by grammar convention, `{ident}⇒true`). At
            // most one per line (checked once below, after the loop, so the
            // conflict diagnostic can point at every flag involved); any
            // flag on `narrator` is always an error regardless of conflict.
            "mono" | "os" | "vo" => {
                if line.speaker == "narrator" {
                    diags.push(err(
                        E_DELIVERY_NARRATOR,
                        format!(
                            "`{}` is not allowed on `narrator` — narration takes no delivery \
                             (dsl 0.1.0 §12.1)",
                            attr.key
                        ),
                        attr.span,
                    ));
                }
                // dsl 0.2.2 §D7: delivery flags are BARE (`{ident}⇒true`,
                // `AttrValue::BoolTrue`) — a valued form (`mono="yes"`) is
                // malformed, NOT a second delivery flag, so it is reported
                // on its own and excluded from the conflict tally below.
                if !matches!(attr.value, AttrValue::BoolTrue) {
                    diags.push(err(
                        E_DELIVERY_FLAG_VALUE,
                        format!(
                            "delivery flag `{}` is bare and takes no value (dsl 0.2.2 §D7)",
                            attr.key
                        ),
                        attr.span,
                    ));
                    continue;
                }
                delivery_flags.push(attr);
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
    // `mono`/`os`/`vo` are mutually exclusive (dsl 0.2.2 §D7) — two or more
    // set on the same line is `E-DELIVERY-CONFLICT`, stamped at EVERY
    // conflicting flag's span so an editor squiggles all of them.
    if delivery_flags.len() > 1 {
        for attr in &delivery_flags {
            diags.push(err(
                E_DELIVERY_CONFLICT,
                format!(
                    "content line carries {} delivery flags ({}); at most one of \
                     `mono`/`os`/`vo` is allowed (dsl 0.2.2 §D7)",
                    delivery_flags.len(),
                    DELIVERY_FLAGS
                        .iter()
                        .filter(|f| delivery_flags.iter().any(|a| &a.key == *f))
                        .cloned()
                        .collect::<Vec<_>>()
                        .join("/"),
                ),
                attr.span,
            ));
        }
    }
}
