//! Built-in content-line (`:speaker{…}:`) attribute schema (dsl 0.1.0 §7.1, §12.1).
//! Content lines are NOT capability-schema-driven; their attribute set is a fixed
//! part of the scene-kind vocabulary, validated here.

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_syntax::ast::{AttrValue, Line};

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
/// and the narrator-forbids-delivery rule (dsl 0.1.0 §12.1).
pub fn check_content_line_attrs(line: &Line, diags: &mut Vec<Diagnostic>) {
    for attr in &line.attrs {
        if !KNOWN_ATTRS.contains(&attr.key.as_str()) {
            diags.push(err(
                E_UNKNOWN_ATTR,
                format!("unknown content-line attribute `{}` (dsl 0.1.0 §7.1)", attr.key),
                attr.span,
            ));
            continue;
        }
        if attr.key == "delivery" {
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
    }
}
