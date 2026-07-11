//! Project-authored `entities:`/`relations:` declaration data model (dsl
//! 0.3.0 draft §3.1 entity kinds, §4 relations,
//! `docs/superpowers/proposals/scenario-dsl/0.3.0.md`). Supersedes
//! `entities::parse_entities` (`enums:` stays on [`crate::entities::parse_enums`]
//! — its shape and scope are unchanged by 0.3.0).
//!
//! Scope: PARSE ONLY, TOTAL — every function here is a plain YAML-shape walk
//! that never panics and never emits a diagnostic. A malformed decl (an
//! entity kind declaring neither/both `members:`/`open:`; a relation field of
//! the wrong YAML type; an unknown decl key) is PRESERVED as data — as
//! [`KindShape::Invalid`], an entry in [`RelationDecl::malformed_fields`], or
//! a default/empty value — rather than skipped or dropped, so the checker
//! (a later 0.3.0 task) can diagnose it with a span. This mirrors
//! `entities.rs`'s discipline (skip-not-panic) but goes one step further:
//! nothing here is silently skipped, because 0.3.0's checker needs to SEE
//! the malformed shape to report it (`E-ENTITY-KIND-SHAPE`, `E-RELATION-DOMAIN`
//! per decision D4).

use std::collections::BTreeMap;

use serde_yaml::Value;

use crate::snapshot::Domain;

/// The two legal shapes of an entity kind (spec §3.1) — `Invalid` preserves a
/// neither/both decl for the checker's `E-ENTITY-KIND-SHAPE`.
#[derive(Clone, Debug, PartialEq)]
pub enum KindShape {
    Members(Vec<String>),
    Open,
    Invalid,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EntityKindDecl {
    pub shape: KindShape,
}

/// A `relations:` entry (spec §4). Raw — nothing here is validated; the
/// checker owns `E-RELATION-EMPTY`/`-DOMAIN`/`-DUP`, `E-DERIVE-TIER`,
/// `E-RELATION-RESERVED-WRITE`.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct RelationDecl {
    /// Ordered arg domain names; a non-string YAML entry is preserved as "".
    pub args: Vec<String>,
    /// Raw `tier:` string if present (validated by the checker; `None` =
    /// default `run`).
    pub tier: Option<String>,
    pub derive: bool,
    pub reserved: bool,
    /// Raw 0-based key indices; non-int entries preserved as -1 (checker:
    /// range/dup).
    pub key: Vec<i64>,
    /// Field names with a wrong YAML type, plus unknown decl keys (checker →
    /// `E-RELATION-DOMAIN`, decision D4).
    pub malformed_fields: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ParsedKinds {
    pub kinds: BTreeMap<String, EntityKindDecl>,
    /// Names declared more than once IN THIS BLOCK, in encounter order
    /// (checker → `E-KIND-NAME-CLASH`). `serde_yaml::Value::as_mapping()`
    /// collapses duplicate YAML keys before this function ever sees them, so
    /// this is best-effort over the (already-deduped) mapping iterator; the
    /// authoritative raw-text occurrence scan lives at the meta lift layer.
    pub dups: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ParsedRelations {
    pub relations: BTreeMap<String, RelationDecl>,
    /// Same-block duplicate names (checker → `E-RELATION-DUP`). Same
    /// best-effort caveat as [`ParsedKinds::dups`].
    pub dups: Vec<String>,
}

/// Classify one `entities:` value: `{ members: […] }` (closed), `{ open: … }`
/// (engine-populated — the value itself is not inspected, only key
/// presence), both/neither key present, or a non-mapping value → `Invalid`.
fn kind_shape(v: &Value) -> KindShape {
    if v.as_mapping().is_none() {
        return KindShape::Invalid;
    }
    let has_members = v.get("members").is_some();
    let has_open = v.get("open").is_some();
    match (has_members, has_open) {
        (true, false) => {
            let members = v
                .get("members")
                .and_then(|m| m.as_sequence())
                .map(|seq| {
                    seq.iter()
                        .filter_map(|m| m.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            KindShape::Members(members)
        }
        (false, true) => KindShape::Open,
        _ => KindShape::Invalid,
    }
}

/// Parse a schema doc's `entities:` block: `{ <kind>: { members: [<id>…] } |
/// { open: engine } }` (spec §3.1). `value` is the raw YAML node bound to the
/// top-level `entities` key (pass `&Value::Null` when absent — yields an
/// empty map). Total: a non-mapping top-level value yields no kinds; a
/// non-string kind name is skipped (mirrors `entities.rs`'s key handling);
/// every OTHER malformed shape is preserved as [`KindShape::Invalid`] rather
/// than skipped (see module doc).
pub fn parse_entity_kinds(value: &Value) -> ParsedKinds {
    let mut out = ParsedKinds::default();
    let Some(map) = value.as_mapping() else {
        return out;
    };
    for (k, v) in map {
        let Some(name) = k.as_str() else {
            continue;
        };
        if out.kinds.contains_key(name) {
            out.dups.push(name.to_string());
        }
        out.kinds.insert(
            name.to_string(),
            EntityKindDecl {
                shape: kind_shape(v),
            },
        );
    }
    out
}

/// Parse one `relations:` entry into a [`RelationDecl`]. A non-mapping decl
/// value (including a bare scalar) yields `RelationDecl::default()`. Each
/// known field (`args`, `tier`, `derive`, `reserved`, `key`) is pulled with a
/// type check: the right YAML shape sets the field, the wrong shape leaves
/// the field at its default AND pushes the field name into
/// `malformed_fields`. An unknown decl key is always pushed into
/// `malformed_fields` (its value is never inspected).
fn relation_decl(v: &Value) -> RelationDecl {
    let mut decl = RelationDecl::default();
    let Some(map) = v.as_mapping() else {
        return decl;
    };
    for (k, val) in map {
        let Some(field) = k.as_str() else {
            continue;
        };
        match field {
            "args" => {
                if let Some(seq) = val.as_sequence() {
                    decl.args = seq
                        .iter()
                        .map(|a| a.as_str().map(str::to_string).unwrap_or_default())
                        .collect();
                } else {
                    decl.malformed_fields.push("args".to_string());
                }
            }
            "tier" => {
                if let Some(s) = val.as_str() {
                    decl.tier = Some(s.to_string());
                } else {
                    decl.malformed_fields.push("tier".to_string());
                }
            }
            "derive" => {
                if let Some(b) = val.as_bool() {
                    decl.derive = b;
                } else {
                    decl.malformed_fields.push("derive".to_string());
                }
            }
            "reserved" => {
                if let Some(b) = val.as_bool() {
                    decl.reserved = b;
                } else {
                    decl.malformed_fields.push("reserved".to_string());
                }
            }
            "key" => {
                if let Some(seq) = val.as_sequence() {
                    decl.key = seq.iter().map(|k| k.as_i64().unwrap_or(-1)).collect();
                } else {
                    decl.malformed_fields.push("key".to_string());
                }
            }
            other => decl.malformed_fields.push(other.to_string()),
        }
    }
    decl
}

/// Parse a schema doc's `relations:` block: `{ <name>: { args: […],
/// tier?, derive?, reserved?, key? } }` (spec §4). `value` is the raw YAML
/// node bound to the top-level `relations` key (pass `&Value::Null` when
/// absent). Total: see [`relation_decl`] for per-entry preservation rules.
pub fn parse_relations(value: &Value) -> ParsedRelations {
    let mut out = ParsedRelations::default();
    let Some(map) = value.as_mapping() else {
        return out;
    };
    for (k, v) in map {
        let Some(name) = k.as_str() else {
            continue;
        };
        if out.relations.contains_key(name) {
            out.dups.push(name.to_string());
        }
        out.relations.insert(name.to_string(), relation_decl(v));
    }
    out
}

/// Domain projection for the 0.2.2 attr layer (spec `state:`/attr checking
/// unchanged by 0.3.0): `Members` → closed `Domain`, `Open` → open `Domain`,
/// `Invalid` → skipped (an entity kind that doesn't parse to either legal
/// shape has no domain to project; the checker diagnoses the decl itself via
/// `ParsedKinds`, not this projection).
pub fn kinds_to_domains(kinds: &BTreeMap<String, EntityKindDecl>) -> BTreeMap<String, Domain> {
    let mut out = BTreeMap::new();
    for (name, decl) in kinds {
        match &decl.shape {
            KindShape::Members(members) => {
                out.insert(
                    name.clone(),
                    Domain {
                        members: members.clone(),
                        open: false,
                    },
                );
            }
            KindShape::Open => {
                out.insert(
                    name.clone(),
                    Domain {
                        members: Vec::new(),
                        open: true,
                    },
                );
            }
            KindShape::Invalid => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn yaml(s: &str) -> serde_yaml::Value {
        serde_yaml::from_str(s).unwrap()
    }

    #[test]
    fn parses_kind_shapes() {
        let p = parse_entity_kinds(&yaml(
            "character: { members: [shadowheart, halsin] }\nnpc: { open: engine }\nbad: {}\nboth: { members: [x], open: engine }",
        ));
        assert_eq!(
            p.kinds["character"].shape,
            KindShape::Members(vec!["shadowheart".into(), "halsin".into()])
        );
        assert_eq!(p.kinds["npc"].shape, KindShape::Open);
        assert_eq!(p.kinds["bad"].shape, KindShape::Invalid);
        assert_eq!(p.kinds["both"].shape, KindShape::Invalid);
        assert!(p.dups.is_empty());
    }

    #[test]
    fn parses_relation_decl_fields() {
        let p = parse_relations(&yaml(
            "atLocation: { args: [character, location], tier: run, key: [0] }\ncanReach: { args: [character, location], derive: true }\ntrustTier: { args: [character, character, trustLevel], tier: run, reserved: true }",
        ));
        let a = &p.relations["atLocation"];
        assert_eq!(a.args, vec!["character", "location"]);
        assert_eq!(a.tier.as_deref(), Some("run"));
        assert_eq!(a.key, vec![0]);
        assert!(p.relations["canReach"].derive);
        assert!(p.relations["trustTier"].reserved);
    }

    #[test]
    fn preserves_malformed_fields_and_empty_args() {
        let p = parse_relations(&yaml("weird: { args: 5, derive: \"yes\", keys: [0] }\nempty: {}\nscalar: 3"));
        let w = &p.relations["weird"];
        assert!(w.args.is_empty());
        assert!(w.malformed_fields.contains(&"args".to_string()));
        assert!(w.malformed_fields.contains(&"derive".to_string()));
        assert!(w.malformed_fields.contains(&"keys".to_string())); // unknown key preserved
        assert!(p.relations["empty"].args.is_empty());
        assert!(p.relations["scalar"].args.is_empty());
    }

    #[test]
    fn kinds_project_to_domains() {
        let p = parse_entity_kinds(&yaml("character: { members: [a] }\nnpc: { open: engine }\nbad: {}"));
        let d = kinds_to_domains(&p.kinds);
        assert_eq!(d["character"].members, vec!["a"]);
        assert!(d["npc"].open && d["npc"].members.is_empty());
        assert!(!d.contains_key("bad"));
    }
}
