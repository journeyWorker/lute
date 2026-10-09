//! dsl 0.27.0 §4: occasion gates.

use crate::schema::{OccasionDecl, OccasionTarget};

/// `E-OCCASION-GATE` (dsl 0.27.0 §4): a `lute play` step raises an occasion
/// the engine would not raise — its `raisedWhen` gate is false, or the
/// project's `terminal:` condition holds.
pub const E_OCCASION_GATE: &str = "E-OCCASION-GATE";

/// The member a concrete `target` (`room.office`) names on a domain-target
/// occasion (`office`); `None` for a shape-only occasion or another prefix.
pub fn target_member(decl: &OccasionDecl, target: &str) -> Option<String> {
    let OccasionTarget::Domain { prefix, .. } = &decl.target else {
        return None;
    };
    target
        .strip_prefix(prefix.as_str())?
        .strip_prefix('.')
        .filter(|m| !m.is_empty())
        .map(str::to_string)
}
