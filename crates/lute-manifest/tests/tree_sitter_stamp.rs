//! Drift guard (plugin §13): the tree-sitter artifacts are stamped with the
//! capabilityVersion they target; the grammar is data-not-grammar so it targets
//! the CORE snapshot. If capability_version's inputs change, these must be
//! re-stamped or this test fails.
use std::fs;

fn stamp_from(path: &str) -> String {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let v: serde_json::Value =
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {path}: {e}"));
    v.get("metadata")
        .and_then(|m| m.get("capabilityVersion"))
        .and_then(|s| s.as_str())
        .unwrap_or_else(|| panic!("{path} has no metadata.capabilityVersion"))
        .to_string()
}

#[test]
fn tree_sitter_stamp_matches_core_capability_version() {
    let core = lute_manifest::core::load_core_snapshot().version;
    for path in [
        "../../tree-sitter-lute/tree-sitter.json",
        "../../tree-sitter-lute/package.json",
    ] {
        assert_eq!(
            stamp_from(path),
            core,
            "{path} capabilityVersion is stale; re-stamp to the current core capability_version"
        );
    }
}
