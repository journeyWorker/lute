//! Project-authored `enums:` declaration parsing (data-catalog foundation
//! A3; 0.3.0 draft §3.1 entity kinds, `docs/superpowers/proposals/
//! scenario-dsl/0.3.0.md`). A schema doc's own `enums:` frontmatter block
//! parses into the SAME enum-style [`Domain`] (A2, `crate::snapshot::Domain`)
//! shape a plugin's `enums` export folds into `CapabilitySnapshot.domains`
//! (`assemble.rs`), so a project declaration composes into the identical
//! merged vocabulary — lifted by `lute-check`'s `schema_import` module
//! (`resolve_imports`/`merge_domains`) the same way `state:`/`defs:` are
//! lifted.
//!
//! `entities:` (0.3.0 draft §3.1 kind declarations: `{ members: […] }` /
//! `{ open: engine }`) and 0.3.0's full `relations:`/`facts:`/`rules:`
//! relational system live in [`crate::relations`] — `parse_entity_kinds`
//! there supersedes this module's former `parse_entities` (deleted 0.3.0 T4;
//! its sole caller migrated to `relations::{parse_entity_kinds,
//! kinds_to_domains}`).
//!
//! [`parse_enums`] is TOTAL: a malformed shape (wrong YAML node kind,
//! non-string key/member) is skipped for that entry — never a panic, never a
//! diagnostic (this module has no diagnostic machinery; `schema_import`
//! reports collisions once names are lifted into the merged vocabulary).

use std::collections::BTreeMap;

use serde_yaml::Value;

use crate::snapshot::Domain;

/// Parse a schema doc's `enums:` block: `{ <name>: [<member>…] }` (0.3.0
/// draft §3.1). Each entry becomes a closed, ordered enum-style [`Domain`]
/// (`open: false`) — the identical shape `assemble.rs`/`core.rs` fold a
/// plugin/core `enums` export into. `value` is the raw YAML node bound to the
/// top-level `enums` key (pass `&Value::Null` when the key is absent — a
/// non-mapping value yields an empty map). A non-list entry value or a
/// non-string member is skipped for that entry.
pub fn parse_enums(value: &Value) -> BTreeMap<String, Domain> {
    let mut out = BTreeMap::new();
    let Some(map) = value.as_mapping() else {
        return out;
    };
    for (k, v) in map {
        let (Some(name), Some(members)) = (k.as_str(), v.as_sequence()) else {
            continue;
        };
        let members = members
            .iter()
            .filter_map(|m| m.as_str().map(str::to_string))
            .collect();
        out.insert(name.to_string(), Domain { members, open: false });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_enums_reads_member_lists() {
        let v: Value = serde_yaml::from_str("action: [wave, bow]\nmood: [calm, tense]").unwrap();
        let doms = parse_enums(&v);
        assert_eq!(doms["action"].members, vec!["wave", "bow"]);
        assert!(!doms["action"].open);
        assert_eq!(doms["mood"].members, vec!["calm", "tense"]);
    }

    #[test]
    fn parse_enums_absent_or_malformed_is_empty() {
        assert!(parse_enums(&Value::Null).is_empty());
        let v: Value = serde_yaml::from_str("action: notAList").unwrap();
        assert!(parse_enums(&v).is_empty());
    }
}
