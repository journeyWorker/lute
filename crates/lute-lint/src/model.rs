//! Shared rule shape.
//!
//! The authoritative definitions live in `lute_manifest::lint` (spec §6, the
//! plugin/config/custom rule contract). This module re-exports them so the
//! lint engine — and any future in-crate rule intake — reads exactly the
//! types plugin YAML deserializes into. Do NOT duplicate the shape here: a
//! divergence would silently break the lints/plugin loader contract.

pub use lute_manifest::lint::{LintLevel, LintRuleDecl, LintTarget};

/// Well-known lint diagnostic codes.
pub const E_LINT_CONFIG: &str = "E-LINT-CONFIG";
pub const E_LINT_EXPR: &str = "E-LINT-EXPR";
pub const E_LINT_RULE: &str = "E-LINT-RULE";

/// Map a resolved [`LintLevel`] to the shared [`Severity`] enum. `Off` is not
/// a severity — it signals "do not run", so returning `None` lets the caller
/// tell the two apart.
pub fn level_severity(level: LintLevel) -> Option<lute_core_span::Severity> {
    match level {
        LintLevel::Off => None,
        LintLevel::Hint => Some(lute_core_span::Severity::Hint),
        LintLevel::Info => Some(lute_core_span::Severity::Info),
        LintLevel::Warn => Some(lute_core_span::Severity::Warning),
        LintLevel::Error => Some(lute_core_span::Severity::Error),
    }
}

/// The rule-id → diagnostic-code transform (spec §8): `L-` + uppercased id
/// with `/` and any non-alphanumeric run collapsed to `-`, trimmed of leading
/// and trailing dashes.
pub fn diagnostic_code(rule_id: &str) -> String {
    let mut out = String::with_capacity(rule_id.len() + 2);
    out.push_str("L-");
    let mut last_dash = true; // consumes leading dashes after "L-"
    for ch in rule_id.chars() {
        if ch.is_ascii_alphanumeric() {
            for u in ch.to_uppercase() {
                out.push(u);
            }
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_maps_kebab() {
        assert_eq!(diagnostic_code("dialogue-length"), "L-DIALOGUE-LENGTH");
    }

    #[test]
    fn code_maps_plugin_slash() {
        assert_eq!(
            diagnostic_code("my-plugin/variant-coverage"),
            "L-MY-PLUGIN-VARIANT-COVERAGE"
        );
    }

    #[test]
    fn code_trims_edges_and_collapses_runs() {
        assert_eq!(diagnostic_code("--weird__id--"), "L-WEIRD-ID");
    }
}
