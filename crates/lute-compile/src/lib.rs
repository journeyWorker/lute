//! `lute-compile` — lowers a checked `.lute` document to the typed JSON
//! command-record artifact (design spec
//! `docs/superpowers/specs/2026-07-04-lute-compile-json-ir-design.md`).

pub mod cfg;
pub mod expand;
pub mod ir;
pub mod lower;
pub mod normalize;
pub mod schedule;
pub mod stage;

pub use ir::*;

/// IR version stamped into every artifact envelope (`"lute": …`, spec §4.1).
pub const LUTE_IR_VERSION: &str = "0.0.1";

#[cfg(test)]
mod tests {
    #[test]
    fn ir_version_matches_language_version() {
        assert_eq!(super::LUTE_IR_VERSION, "0.0.1");
    }
}
