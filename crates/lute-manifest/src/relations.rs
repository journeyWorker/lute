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

/// One `entities:` entry. `subset_of` is dsl 0.24.0 §3's `subsetOf: <kind>`:
/// every member is also a member of the named parent kind, so a sub-kind is
/// legal wherever a kind is and an argument of the sub-kind is also of the
/// parent. Raw — the checker validates the parent (`E-ENTITY-KIND-SHAPE`).
/// `labels` is dsl 0.27.0 §7's `labels: { <member>: <display text> }`: what
/// a `{{…}}` of a value of this kind renders instead of the member id
/// (partial — an unlabelled member renders its id). The well-formed entries
/// of the map ([`KindLabel`]); every shape mistake is in
/// [`ParsedKinds::label_problems`].
#[derive(Clone, Debug, PartialEq)]
pub struct EntityKindDecl {
    pub shape: KindShape,
    pub subset_of: Option<String>,
    pub labels: BTreeMap<String, KindLabel>,
}

/// One member's entry in a kind's `labels:` — a plain string (`text` only)
/// or `{ text, start, indefinite }`: `text` is what `{{…}}` renders, `start`
/// the sentence-start form a `:start` hint renders (`The smugglers' cut`),
/// `indefinite` the form with its article a `:indefinite` hint renders
/// (`an ashwraith`). Absent forms fall back
/// (the renderer applies `lute_syntax::ast::format_text`).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct KindLabel {
    pub text: String,
    pub start: Option<String>,
    pub indefinite: Option<String>,
}

impl KindLabel {
    /// `true` when the label declares a form beside its text.
    pub fn has_forms(&self) -> bool {
        self.start.is_some() || self.indefinite.is_some()
    }
}

/// The keys a `labels:` entry's mapping form may carry.
pub const KIND_LABEL_KEYS: &[&str] = &["text", "start", "indefinite"];

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
    /// dsl 0.25.0 §6: the occasions on which the engine may change this
    /// engine-`reserved` relation, as written (the checker owns
    /// `E-RELATION-DECL`). A non-string entry is preserved as "".
    pub changed_on: Vec<String>,
    /// Raw 0-based key indices; non-int entries preserved as -1 (checker:
    /// range/dup).
    pub key: Vec<i64>,
    /// dsl 0.25.0 §1: relations this one can never hold together with on the
    /// same arguments, as written (the checker closes it symmetrically and
    /// owns `E-RELATION-DECL`). A non-string entry is preserved as "".
    pub excludes: Vec<String>,
    /// Field names with a wrong YAML type, plus unknown decl keys (checker →
    /// `E-RELATION-DOMAIN`, decision D4).
    pub malformed_fields: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ParsedKinds {
    pub kinds: BTreeMap<String, EntityKindDecl>,
    /// The names of [`Self::kinds`] in the order the block declares them
    /// (first occurrence) — the order [`imply_sub_kind_members`] visits
    /// sub-kinds in, so a union kind's members follow the schema.
    pub order: Vec<String>,
    /// Names declared more than once IN THIS BLOCK, in encounter order
    /// (checker → `E-KIND-NAME-CLASH`). `serde_yaml::Value::as_mapping()`
    /// collapses duplicate YAML keys before this function ever sees them, so
    /// this is best-effort over the (already-deduped) mapping iterator; the
    /// authoritative raw-text occurrence scan lives at the meta lift layer.
    pub dups: Vec<String>,
    /// dsl 0.26.0 §2.3: `<kind>: { add: [<id>…] }` entries — members this
    /// block adds to a kind another schema declares, by kind name, as written.
    /// Never in [`Self::kinds`]; the checker merges them into the one base
    /// declaration (`E-ENTITY-KIND-SHAPE` when there is none).
    pub adds: BTreeMap<String, Vec<String>>,
    /// dsl 0.27.0 §7: the `labels:` written beside an `add:` list, by kind
    /// name — display text for the members that `add:` brings (the checker
    /// merges them with the members and reports a key the list does not add).
    pub add_labels: BTreeMap<String, BTreeMap<String, KindLabel>>,
    /// dsl 0.27.0 §7: `(kind, problem)` for every `labels:` shape mistake — a
    /// value that is not a mapping, a label that is not a string (checker →
    /// `E-ENTITY-KIND-SHAPE`). The mistaken entries are left out of `labels`.
    pub label_problems: Vec<(String, String)>,
    /// dsl 0.27.0 §2 (T3-15): `(kind, key)` for every key of a kind's mapping
    /// outside [`ENTITY_KIND_KEYS`], in encounter order (checker →
    /// `E-ENTITY-KIND-SHAPE` with did-you-mean). A typo'd `lables:` was
    /// silently ignored.
    pub unknown_keys: Vec<(String, String)>,
}

#[derive(Clone, Debug, Default)]
pub struct ParsedRelations {
    pub relations: BTreeMap<String, RelationDecl>,
    /// Same-block duplicate names (checker → `E-RELATION-DUP`). Same
    /// best-effort caveat as [`ParsedKinds::dups`].
    pub dups: Vec<String>,
}

/// The keys an `entities:` entry may carry (spec §3.1, dsl 0.24.0 §3, 0.26.0
/// §2.3, 0.27.0 §7). Every other key is reported, never ignored.
pub const ENTITY_KIND_KEYS: &[&str] = &["members", "open", "add", "subsetOf", "labels"];

/// The keys one `relations:` entry takes; any other is reported.
pub const RELATION_KEYS: &[&str] = &[
    "args",
    "tier",
    "derive",
    "reserved",
    "changedOn",
    "key",
    "excludes",
];

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

/// The `labels:` of one `entities:` entry: the well-formed entries of its
/// mapping — a string, or `{ text, start, indefinite }` ([`KindLabel`]) —
/// each shape mistake pushed to `problems` as `(kind, problem)` (dsl 0.27.0
/// §7). Absent → empty.
fn kind_labels(
    kind: &str,
    v: &Value,
    problems: &mut Vec<(String, String)>,
) -> BTreeMap<String, KindLabel> {
    let mut out = BTreeMap::new();
    let Some(labels) = v.get("labels") else {
        return out;
    };
    let Some(map) = labels.as_mapping() else {
        problems.push((
            kind.to_string(),
            "`labels:` must map each member to its display text, as `labels: { chapel: \
             \"the chapel\" }`"
                .to_string(),
        ));
        return out;
    };
    for (member, label) in map {
        let member = member.as_str().unwrap_or_default();
        match kind_label(member, label) {
            Ok(label) => {
                out.insert(member.to_string(), label);
            }
            Err(problem) => problems.push((kind.to_string(), problem)),
        }
    }
    out
}

/// One `labels:` entry: a string, or a mapping with a string `text:` and
/// optional string `start:` / `indefinite:` forms.
fn kind_label(member: &str, label: &Value) -> Result<KindLabel, String> {
    if let Some(text) = label.as_str() {
        return Ok(KindLabel {
            text: text.to_string(),
            ..KindLabel::default()
        });
    }
    let shape = || {
        format!(
            "the label of `{member}` must be text, as `{member}: \"…\"`, or its forms, as \
             `{member}: {{ text: \"…\", start: \"…\", indefinite: \"…\" }}`"
        )
    };
    let map = label.as_mapping().ok_or_else(shape)?;
    for key in map.keys() {
        let key = key.as_str().unwrap_or_default();
        if !KIND_LABEL_KEYS.contains(&key) {
            return Err(format!(
                "the label of `{member}` has an unknown key `{key}`{}; a label's forms are \
                 `text:` (what `{{{{…}}}}` shows), `start:` (`:start`, at the start of a \
                 sentence) and `indefinite:` (`:indefinite`, with its article)",
                crate::suggest::did_you_mean(key, KIND_LABEL_KEYS.iter().copied())
            ));
        }
    }
    let form = |key: &str| -> Result<Option<String>, String> {
        match map.get(key) {
            None => Ok(None),
            Some(v) => v
                .as_str()
                .map(|s| Some(s.to_string()))
                .ok_or_else(|| format!("the `{key}:` form of `{member}`'s label must be text")),
        }
    };
    Ok(KindLabel {
        text: form("text")?.ok_or_else(|| {
            format!(
                "the label of `{member}` needs `text:` — what `{{{{…}}}}` shows; `start:` and \
                 `indefinite:` are forms beside it"
            )
        })?,
        start: form("start")?,
        indefinite: form("indefinite")?,
    })
}

/// Parse a schema doc's `entities:` block: `{ <kind>: { members: [<id>…] } |
/// { open: engine } | { add: [<id>…] } }` (spec §3.1, dsl 0.26.0 §2.3), each
/// optionally with `labels:` (dsl 0.27.0 §7).
/// `value` is the raw YAML node bound to the top-level `entities` key (pass
/// `&Value::Null` when absent — yields an empty map). Total: a non-mapping
/// top-level value yields no kinds; a non-string kind name is skipped (mirrors
/// `entities.rs`'s key handling); an `add:` alone (or with `labels:`) with a
/// list lands in [`ParsedKinds::adds`]; every OTHER malformed shape (an `add:`
/// beside another key, or not a list) is preserved as [`KindShape::Invalid`]
/// rather than skipped (see module doc).
pub fn parse_entity_kinds(value: &Value) -> ParsedKinds {
    let mut out = ParsedKinds::default();
    let Some(map) = value.as_mapping() else {
        return out;
    };
    for (k, v) in map {
        let Some(name) = k.as_str() else {
            continue;
        };
        if out.kinds.contains_key(name) || out.adds.contains_key(name) {
            out.dups.push(name.to_string());
        }
        if let Some(m) = v.as_mapping() {
            for key in m.keys() {
                let key = key.as_str().unwrap_or_default();
                if !ENTITY_KIND_KEYS.contains(&key) {
                    out.unknown_keys.push((name.to_string(), key.to_string()));
                }
            }
        }
        let add = v.get("add");
        // dsl 0.27.0 §7: `labels:` may ride beside `add:` to label the
        // members it brings.
        let add_keys = if v.get("labels").is_some() { 2 } else { 1 };
        if let (Some(seq), Some(true)) = (
            add.and_then(Value::as_sequence),
            v.as_mapping().map(|m| m.len() == add_keys),
        ) {
            let members = seq
                .iter()
                .filter_map(|m| m.as_str().map(str::to_string))
                .collect();
            out.adds.insert(name.to_string(), members);
            let labels = kind_labels(name, v, &mut out.label_problems);
            if !labels.is_empty() {
                out.add_labels.insert(name.to_string(), labels);
            }
            continue;
        }
        let labels = kind_labels(name, v, &mut out.label_problems);
        if !out.kinds.contains_key(name) {
            out.order.push(name.to_string());
        }
        out.kinds.insert(
            name.to_string(),
            EntityKindDecl {
                shape: if add.is_some() {
                    KindShape::Invalid
                } else {
                    kind_shape(v)
                },
                subset_of: v
                    .get("subsetOf")
                    .map(|p| p.as_str().unwrap_or_default().to_string()),
                labels,
            },
        );
    }
    out
}

/// dsl 0.26.0 §2.3: every member of a `subsetOf:` sub-kind is a member of its
/// parent, so the parent need not restate it. Appends each `members:`
/// sub-kind's members to every `members:` ancestor: after the ancestor's own
/// list, sub-kinds in declaration order (`order`, the kind names as the
/// schemas declare them; a kind it does not name follows, in name order),
/// each member once, where it first appears. The walk stops at a missing or
/// `open:` parent and skips a `subsetOf:` loop — the checker reports those
/// (`E-ENTITY-KIND-SHAPE`).
///
/// dsl 0.27.0 §7: a label names a member, whichever kind wrote it — so an
/// ancestor takes its sub-kinds' labels, and a sub-kind its ancestors' labels
/// for its own members (nearest first). A kind's own label always wins.
pub fn imply_sub_kind_members(kinds: &mut BTreeMap<String, EntityKindDecl>, order: &[String]) {
    let mut implied: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut up: BTreeMap<String, Vec<(String, KindLabel)>> = BTreeMap::new();
    let mut chains: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let declared = order.iter().filter(|n| kinds.contains_key(*n));
    let rest = kinds.keys().filter(|n| !order.contains(n));
    for name in declared.chain(rest) {
        let decl = &kinds[name];
        let KindShape::Members(members) = &decl.shape else {
            continue;
        };
        let mut chain = Vec::new();
        let mut cur = decl.subset_of.as_deref();
        while let Some(parent) = cur {
            if parent == name || chain.contains(&parent) {
                chain.clear();
                break;
            }
            match kinds.get(parent) {
                Some(p) if matches!(p.shape, KindShape::Members(_)) => {
                    chain.push(parent);
                    cur = p.subset_of.as_deref();
                }
                _ => break,
            }
        }
        for parent in &chain {
            implied
                .entry(parent.to_string())
                .or_default()
                .extend(members.iter().cloned());
            up.entry(parent.to_string())
                .or_default()
                .extend(decl.labels.iter().map(|(m, l)| (m.clone(), l.clone())));
        }
        if !chain.is_empty() {
            chains.insert(name.clone(), chain.iter().map(|p| p.to_string()).collect());
        }
    }
    for (parent, extra) in implied {
        if let Some(EntityKindDecl {
            shape: KindShape::Members(ms),
            ..
        }) = kinds.get_mut(&parent)
        {
            let mut have: std::collections::BTreeSet<String> = ms.iter().cloned().collect();
            for m in extra {
                if have.insert(m.clone()) {
                    ms.push(m);
                }
            }
        }
    }
    for (parent, labels) in up {
        if let Some(p) = kinds.get_mut(&parent) {
            for (m, l) in labels {
                p.labels.entry(m).or_insert(l);
            }
        }
    }
    for (name, chain) in chains {
        let inherited: Vec<(String, KindLabel)> = chain
            .iter()
            .filter_map(|p| kinds.get(p))
            .flat_map(|p| p.labels.iter().map(|(m, l)| (m.clone(), l.clone())))
            .collect();
        if let Some(EntityKindDecl {
            shape: KindShape::Members(ms),
            labels,
            ..
        }) = kinds.get_mut(&name)
        {
            for (m, l) in inherited {
                if ms.contains(&m) {
                    labels.entry(m).or_insert(l);
                }
            }
        }
    }
}

/// `true` when `kind` is `ancestor` or a (transitive) `subsetOf` descendant of
/// it (dsl 0.24.0 §3). Cycle-safe: a malformed `subsetOf` loop stops after one
/// visit per kind (the checker reports the loop itself).
pub fn kind_within(kinds: &BTreeMap<String, EntityKindDecl>, kind: &str, ancestor: &str) -> bool {
    let mut cur = kind;
    for _ in 0..=kinds.len() {
        if cur == ancestor {
            return true;
        }
        match kinds.get(cur).and_then(|d| d.subset_of.as_deref()) {
            Some(parent) => cur = parent,
            None => return false,
        }
    }
    false
}

/// dsl 0.26.0 §2.2: every member listed more than once in `members`, once
/// each, in the order of its second occurrence (an entity kind's or enum's
/// list; the checker owns `E-ENTITY-KIND-SHAPE`).
pub fn duplicate_members(members: &[String]) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    let mut dups: Vec<String> = Vec::new();
    for m in members {
        if !seen.insert(m.as_str()) && !dups.contains(m) {
            dups.push(m.clone());
        }
    }
    dups
}

/// dsl 0.25.0 §1: `a` and `b` can never hold together on the same arguments —
/// either names the other in `excludes:` (the symmetric closure), both are
/// declared, they are distinct, and their argument kinds agree (a mismatch is
/// the checker's `E-RELATION-DECL` and excludes nothing).
pub fn relations_exclude(relations: &BTreeMap<String, RelationDecl>, a: &str, b: &str) -> bool {
    if a == b {
        return false;
    }
    let (Some(da), Some(db)) = (relations.get(a), relations.get(b)) else {
        return false;
    };
    da.args == db.args && (da.excludes.iter().any(|x| x == b) || db.excludes.iter().any(|x| x == a))
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
            "changedOn" => {
                if let Some(seq) = val.as_sequence() {
                    decl.changed_on = seq
                        .iter()
                        .map(|a| a.as_str().map(str::to_string).unwrap_or_default())
                        .collect();
                } else {
                    decl.malformed_fields.push("changedOn".to_string());
                }
            }
            "key" => {
                if let Some(seq) = val.as_sequence() {
                    decl.key = seq.iter().map(|k| k.as_i64().unwrap_or(-1)).collect();
                } else {
                    decl.malformed_fields.push("key".to_string());
                }
            }
            "excludes" => {
                if let Some(seq) = val.as_sequence() {
                    decl.excludes = seq
                        .iter()
                        .map(|a| a.as_str().map(str::to_string).unwrap_or_default())
                        .collect();
                } else {
                    decl.malformed_fields.push("excludes".to_string());
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
/// `ParsedKinds`, not this projection). A closed kind's `labels:` (dsl 0.27.0
/// §7) become the domain's, so a `{ domain: K }` path renders them.
pub fn kinds_to_domains(kinds: &BTreeMap<String, EntityKindDecl>) -> BTreeMap<String, Domain> {
    let mut out = BTreeMap::new();
    for (name, decl) in kinds {
        match &decl.shape {
            KindShape::Members(members) => {
                out.insert(
                    name.clone(),
                    Domain {
                        members: members.clone(),
                        labels: decl
                            .labels
                            .iter()
                            .map(|(m, l)| (m.clone(), l.text.clone()))
                            .collect(),
                        ..Default::default()
                    },
                );
            }
            KindShape::Open => {
                out.insert(
                    name.clone(),
                    Domain {
                        members: Vec::new(),
                        open: true,
                        ..Default::default()
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
        let p = parse_relations(&yaml(
            "weird: { args: 5, derive: \"yes\", keys: [0] }\nempty: {}\nscalar: 3",
        ));
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
        let p = parse_entity_kinds(&yaml(
            "character: { members: [a] }\nnpc: { open: engine }\nbad: {}",
        ));
        let d = kinds_to_domains(&p.kinds);
        assert_eq!(d["character"].members, vec!["a"]);
        assert!(d["npc"].open && d["npc"].members.is_empty());
        assert!(!d.contains_key("bad"));
    }

    #[test]
    fn parses_sub_kinds_and_follows_the_chain() {
        let p = parse_entity_kinds(&yaml(
            "person: { members: [a, b, c] }\ncompanion: { subsetOf: person, members: [a, b] }\n\
             sworn: { subsetOf: companion, members: [a] }\nloop: { subsetOf: loop, members: [x] }",
        ));
        assert_eq!(p.kinds["companion"].subset_of.as_deref(), Some("person"));
        assert_eq!(
            p.kinds["companion"].shape,
            KindShape::Members(vec!["a".into(), "b".into()])
        );
        assert_eq!(p.kinds["person"].subset_of, None);
        assert!(kind_within(&p.kinds, "sworn", "person"));
        assert!(kind_within(&p.kinds, "companion", "companion"));
        assert!(!kind_within(&p.kinds, "person", "companion"));
        assert!(
            !kind_within(&p.kinds, "loop", "person"),
            "a loop terminates"
        );
    }

    /// dsl 0.27.0 §7: `labels:` is a kind key; string labels are kept, each
    /// shape mistake is reported, and `labels:` may ride beside `add:`.
    #[test]
    fn parses_kind_labels_and_their_mistakes() {
        let p = parse_entity_kinds(&yaml(
            "room: { members: [chapel, ward], labels: { chapel: the chapel, ward: 7 } }\n\
             cg: { members: [a], labels: [A] }\n\
             item: { add: [lamp], labels: { lamp: the lamp } }",
        ));
        assert_eq!(
            p.kinds["room"]
                .labels
                .get("chapel")
                .map(|l| l.text.as_str()),
            Some("the chapel")
        );
        assert_eq!(p.kinds["room"].labels.get("ward"), None);
        assert!(p.kinds["cg"].labels.is_empty());
        assert_eq!(p.label_problems.len(), 2, "{:?}", p.label_problems);
        assert!(p.unknown_keys.is_empty(), "{:?}", p.unknown_keys);
        assert_eq!(p.adds["item"], vec!["lamp"]);
        assert_eq!(p.add_labels["item"]["lamp"].text, "the lamp");
        let d = kinds_to_domains(&p.kinds);
        assert_eq!(d["room"].labels["chapel"], "the chapel");
    }

    /// A label may carry its forms: `{ text, start, indefinite }`; a form
    /// that is not text, an unknown key and a missing `text:` are mistakes.
    #[test]
    fn kind_labels_take_start_and_indefinite_forms() {
        let p = parse_entity_kinds(&yaml(
            "place: { members: [cut, well, bog, fen, moor], labels: {\n\
             cut: { text: \"the smugglers' cut\", start: \"The smugglers' cut\" },\n\
             well: the well,\n\
             bog: { text: bog, indefinte: a bog },\n\
             fen: { start: The fen },\n\
             moor: { text: moor, indefinite: 3 } } }",
        ));
        let cut = &p.kinds["place"].labels["cut"];
        assert_eq!(cut.text, "the smugglers' cut");
        assert_eq!(cut.start.as_deref(), Some("The smugglers' cut"));
        assert_eq!(cut.indefinite, None);
        assert!(!p.kinds["place"].labels["well"].has_forms());
        assert_eq!(p.kinds["place"].labels.len(), 2);
        let problems: Vec<&str> = p.label_problems.iter().map(|(_, m)| m.as_str()).collect();
        assert_eq!(problems.len(), 3, "{problems:?}");
        assert!(
            problems[0].contains("did you mean `indefinite`"),
            "{problems:?}"
        );
        assert!(problems[1].contains("needs `text:`"), "{problems:?}");
        assert!(
            problems[2].contains("`indefinite:` form of `moor`"),
            "{problems:?}"
        );
    }

    /// A label names a member whichever kind wrote it: the parent takes its
    /// sub-kind's labels and the sub-kind its parent's; own labels win.
    #[test]
    fn labels_follow_the_sub_kind_chain() {
        let parsed = parse_entity_kinds(&yaml(
            "room: { members: [hall], labels: { hall: the hall, ward: a room } }\n\
             ward: { subsetOf: room, members: [ward, crypt], labels: { crypt: the crypt } }",
        ));
        let mut kinds = parsed.kinds;
        imply_sub_kind_members(&mut kinds, &parsed.order);
        assert_eq!(kinds["room"].labels["crypt"].text, "the crypt");
        assert_eq!(
            kinds["room"].labels["ward"].text, "a room",
            "own label wins"
        );
        assert_eq!(kinds["ward"].labels["ward"].text, "a room");
        assert_eq!(kinds["ward"].labels.get("hall"), None, "not a ward member");
    }

    /// G-8: a union kind's members follow the schema — its sub-kinds in the
    /// order they are declared, not by name — each member where it first
    /// appears.
    #[test]
    fn a_union_kind_lists_its_sub_kinds_members_in_declaration_order() {
        let parsed = parse_entity_kinds(&yaml(
            "hero: { members: [] }\nssr: { subsetOf: hero, members: [aria, cyra, vesper] }\n\
             sr: { subsetOf: hero, members: [bram, dax] }\n\
             limited: { subsetOf: ssr, members: [cyra, vesper] }",
        ));
        assert_eq!(parsed.order, ["hero", "ssr", "sr", "limited"]);
        let mut kinds = parsed.kinds;
        imply_sub_kind_members(&mut kinds, &parsed.order);
        assert_eq!(
            kinds["hero"].shape,
            KindShape::Members(
                ["aria", "cyra", "vesper", "bram", "dax"]
                    .map(String::from)
                    .to_vec()
            )
        );
    }
}
