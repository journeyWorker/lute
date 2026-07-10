//! Project-authored `enums:`/`entities:` declaration parsing (data-catalog
//! foundation A3; 0.3.0 draft §3.1 entity kinds, `docs/superpowers/proposals/
//! scenario-dsl/0.3.0.md`). A schema doc's own `enums:` and `entities:`
//! frontmatter blocks parse into the SAME enum-style [`Domain`] (A2,
//! `crate::snapshot::Domain`) shape a plugin's `enums` export folds into
//! `CapabilitySnapshot.domains` (`assemble.rs`), so a project declaration
//! composes into the identical merged vocabulary — lifted by `lute-check`'s
//! `schema_import` module (`resolve_imports`/`merge_domains`) the same way
//! `state:`/`defs:` are lifted.
//!
//! Scope: PARSE ONLY, and only the two shapes A3/A4 need. This deliberately
//! does NOT build 0.3.0's full entity/relation/fact system (`relations:`,
//! `rules:`, `facts:` are out of scope here — a later 0.3.0 effort, not this
//! foundation): `entities: { <kind>: { open: engine } }` (an
//! engine-populated id set, not enumerable at compile time, 0.3.0 draft
//! §3.1) is represented MINIMALLY as `Domain { members: vec![], open: true }`
//! — a flag meaning "membership is engine/registry-provided, treat as
//! always-accept" for a later checker task (A4) to consult, rather than a
//! full registry/provider model. Every parse function here is TOTAL: a
//! malformed shape (wrong YAML node kind, non-string key/member) is skipped
//! for that entry — never a panic, never a diagnostic (this module has no
//! diagnostic machinery; `schema_import` reports collisions once names are
//! lifted into the merged vocabulary).

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

/// Parse a schema doc's `entities:` block: `{ <kind>: { members: [<id>…] } |
/// { open: engine } }` (0.3.0 draft §3.1). `{ members: […] }` is a closed
/// enum-style [`Domain`] (`open: false`), the same shape [`parse_enums`]
/// produces. `{ open: … }` (0.3.0 spells the value `engine`; the VALUE itself
/// is not inspected — presence of the `open` key alone selects this shape,
/// keeping the representation minimal per this module's scope) is the
/// engine-populated shape: `Domain { members: vec![], open: true }`. `value`
/// is the raw YAML node bound to the top-level `entities` key (pass
/// `&Value::Null` when absent). A kind entry declaring neither `members:` nor
/// `open:`, or whose value is not a mapping, is skipped — `members:` is
/// checked first, so a (malformed) entry declaring BOTH resolves as closed.
pub fn parse_entities(value: &Value) -> BTreeMap<String, Domain> {
    let mut out = BTreeMap::new();
    let Some(map) = value.as_mapping() else {
        return out;
    };
    for (k, v) in map {
        let Some(kind) = k.as_str() else {
            continue;
        };
        if let Some(members) = v.get("members").and_then(|m| m.as_sequence()) {
            let members = members
                .iter()
                .filter_map(|m| m.as_str().map(str::to_string))
                .collect();
            out.insert(kind.to_string(), Domain { members, open: false });
        } else if v.get("open").is_some() {
            out.insert(
                kind.to_string(),
                Domain {
                    members: Vec::new(),
                    open: true,
                },
            );
        }
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

    #[test]
    fn parse_entities_closed_members() {
        let v: Value =
            serde_yaml::from_str("character: { members: [shadowheart, halsin] }").unwrap();
        let doms = parse_entities(&v);
        assert_eq!(doms["character"].members, vec!["shadowheart", "halsin"]);
        assert!(!doms["character"].open);
    }

    #[test]
    fn parse_entities_open_engine() {
        let v: Value = serde_yaml::from_str("npc: { open: engine }").unwrap();
        let doms = parse_entities(&v);
        assert!(doms["npc"].open);
        assert!(doms["npc"].members.is_empty());
    }

    #[test]
    fn parse_entities_neither_shape_is_skipped() {
        let v: Value = serde_yaml::from_str("bogus: { nope: true }").unwrap();
        assert!(parse_entities(&v).is_empty());
    }
}
