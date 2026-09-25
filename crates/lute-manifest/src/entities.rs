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
/// draft §3.1) or the dsl 0.9.0 D-D long form `{ <name>: { members: […],
/// default: …, exits: […], labels: { <member>: <text> } } }` (`labels:` dsl
/// 0.24.0 §1). Each entry becomes a closed, ordered
/// enum-style [`Domain`] (`open: false`) — the identical shape
/// `assemble.rs`/`core.rs` fold a plugin/core `enums` export into. `value` is
/// the raw YAML node bound to the top-level `enums` key (pass `&Value::Null`
/// when the key is absent — a non-mapping value yields an empty map). An
/// entry value that is neither a sequence nor a mapping, a long form without
/// a usable `members:`, or a non-string member is skipped for that entry; a
/// non-string label is skipped here and reported by [`label_shape_errors`].
pub fn parse_enums(value: &Value) -> BTreeMap<String, Domain> {
    let mut out = BTreeMap::new();
    let Some(map) = value.as_mapping() else {
        return out;
    };
    for (k, v) in map {
        let Some(name) = k.as_str() else { continue };
        // Flat sequence = `{ members: […] }` shorthand (dsl 0.9.0 D-D).
        let domain = if let Some(members) = v.as_sequence() {
            Domain {
                members: members
                    .iter()
                    .filter_map(|m| m.as_str().map(str::to_string))
                    .collect(),
                ..Default::default()
            }
        } else if let Some(long) = v.as_mapping() {
            let strings = |key: &str| -> Vec<String> {
                long.get(Value::from(key))
                    .and_then(Value::as_sequence)
                    .map(|s| {
                        s.iter()
                            .filter_map(|m| m.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default()
            };
            let members = strings("members");
            // TOTAL, like the rest of this module: a long form with no usable
            // `members:` is skipped for that entry rather than yielding an
            // empty closed domain that rejects every value.
            if members.is_empty() {
                continue;
            }
            Domain {
                members,
                open: false,
                default: long
                    .get(Value::from("default"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
                exits: strings("exits"),
                labels: long
                    .get(Value::from("labels"))
                    .and_then(Value::as_mapping)
                    .map(|m| {
                        m.iter()
                            .filter_map(|(k, v)| Some((k.as_str()?.to_string(), v.as_str()?.to_string())))
                            .collect()
                    })
                    .unwrap_or_default(),
            }
        } else {
            continue;
        };
        out.insert(name.to_string(), domain);
    }
    out
}

/// The `labels:` shape mistakes in an `enums:` block that [`parse_enums`]
/// (total, diagnostic-free) silently drops: a `labels:` value that is not a
/// mapping, or a label that is not a string (dsl 0.24.0 §1). One message per
/// mistake, in declaration order; the caller owns the diagnostic code.
pub fn label_shape_errors(value: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let Some(map) = value.as_mapping() else {
        return out;
    };
    for (k, v) in map {
        let (Some(name), Some(long)) = (k.as_str(), v.as_mapping()) else {
            continue;
        };
        let Some(labels) = long.get(Value::from("labels")) else {
            continue;
        };
        let Some(labels) = labels.as_mapping() else {
            out.push(format!(
                "enum `{name}`: `labels:` must map each member to its display text, \
                 as `labels: {{ sun: Sunday }}` (dsl 0.24.0 §1)"
            ));
            continue;
        };
        for (member, label) in labels {
            if label.as_str().is_none() {
                let member = member.as_str().map_or_else(|| format!("{member:?}"), str::to_string);
                out.push(format!(
                    "enum `{name}`: the label for `{member}` must be a string of display text \
                     (dsl 0.24.0 §1)"
                ));
            }
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
    fn parse_enums_reads_long_form() {
        let v: Value = serde_yaml::from_str(
            "anchor:\n  members: [left, center, right]\n  default: center\n\
             action:\n  members: [sway, fade-out, hide]\n  exits: [fade-out, hide]\n",
        )
        .unwrap();
        let doms = parse_enums(&v);
        assert_eq!(doms["anchor"].members, vec!["left", "center", "right"]);
        assert_eq!(doms["anchor"].default.as_deref(), Some("center"));
        assert!(doms["anchor"].exits.is_empty());
        assert_eq!(doms["action"].exits, vec!["fade-out", "hide"]);
        assert_eq!(doms["action"].default, None);
    }

    #[test]
    fn parse_enums_flat_list_is_shorthand() {
        let v: Value = serde_yaml::from_str("emotion: [neutral, sad]").unwrap();
        let doms = parse_enums(&v);
        assert_eq!(doms["emotion"].members, vec!["neutral", "sad"]);
        assert_eq!(doms["emotion"].default, None);
        assert!(doms["emotion"].exits.is_empty());
    }

    #[test]
    fn parse_enums_reads_labels_and_reports_non_string_ones() {
        let v: Value = serde_yaml::from_str(
            "weekday:\n  members: [mon, sun]\n  labels: { sun: Sunday }\n\
             slot:\n  members: [am, pm]\n  labels: { am: 7, pm: Evening }\n\
             mood:\n  members: [calm]\n  labels: [Calm]\n",
        )
        .unwrap();
        let doms = parse_enums(&v);
        assert_eq!(doms["weekday"].labels.get("sun").map(String::as_str), Some("Sunday"));
        assert_eq!(doms["weekday"].labels.get("mon"), None);
        // The non-string label is dropped; the string one beside it survives.
        assert_eq!(doms["slot"].labels.len(), 1);
        assert!(doms["mood"].labels.is_empty());
        let errs = label_shape_errors(&v);
        assert_eq!(errs.len(), 2, "{errs:?}");
        assert!(errs[0].contains("`slot`") && errs[0].contains("`am`"), "{errs:?}");
        assert!(errs[1].contains("`mood`") && errs[1].contains("must map"), "{errs:?}");
    }
}
