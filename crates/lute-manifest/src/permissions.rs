use std::collections::BTreeSet;

use serde::{Deserialize, Deserializer, Serialize};

/// One conjunctive capability-permission layer.
///
/// An absent field is unrestricted. An explicit empty set denies every member
/// of that category. `true` for a boolean category is likewise unrestricted,
/// while `false` denies it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionSet {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directives: Option<BTreeSet<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_writes: Option<BTreeSet<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fact_writes: Option<BTreeSet<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bridges: Option<BTreeSet<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rewards: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quests: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawPermissionSet {
    #[serde(default, deserialize_with = "deserialize_directives")]
    directives: Option<BTreeSet<String>>,
    #[serde(default, deserialize_with = "deserialize_state_writes")]
    state_writes: Option<BTreeSet<String>>,
    #[serde(default, deserialize_with = "deserialize_fact_writes")]
    fact_writes: Option<BTreeSet<String>>,
    #[serde(default, deserialize_with = "deserialize_bridges")]
    bridges: Option<BTreeSet<String>>,
    #[serde(default, deserialize_with = "deserialize_present_bool")]
    rewards: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_present_bool")]
    quests: Option<bool>,
}

impl<'de> Deserialize<'de> for PermissionSet {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = RawPermissionSet::deserialize(deserializer)?;
        Ok(Self {
            directives: raw.directives,
            state_writes: raw.state_writes,
            fact_writes: raw.fact_writes,
            bridges: raw.bridges,
            rewards: raw.rewards,
            quests: raw.quests,
        }
        .normalized())
    }
}

fn deserialize_present_bool<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<bool>, D::Error> {
    bool::deserialize(deserializer).map(Some)
}

fn deserialize_directives<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<BTreeSet<String>>, D::Error> {
    deserialize_set(deserializer, "directive", valid_directive)
}

fn deserialize_state_writes<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<BTreeSet<String>>, D::Error> {
    deserialize_set(deserializer, "state-write pattern", valid_state_pattern)
}

fn deserialize_fact_writes<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<BTreeSet<String>>, D::Error> {
    deserialize_set(deserializer, "relation", valid_relation)
}

fn deserialize_bridges<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<BTreeSet<String>>, D::Error> {
    deserialize_set(deserializer, "bridge", valid_bridge)
}

fn deserialize_set<'de, D: Deserializer<'de>>(
    deserializer: D,
    kind: &str,
    valid: fn(&str) -> bool,
) -> Result<Option<BTreeSet<String>>, D::Error> {
    let values = Vec::<String>::deserialize(deserializer)?;
    let mut set = BTreeSet::new();
    for value in values {
        if !valid(&value) {
            return Err(serde::de::Error::custom(format!(
                "invalid permission {kind} `{value}`"
            )));
        }
        set.insert(value);
    }
    Ok(Some(set))
}

fn valid_ident(value: &str, allow_hyphen: bool) -> bool {
    let mut bytes = value.bytes();
    matches!(bytes.next(), Some(c) if c.is_ascii_alphabetic())
        && bytes.all(|c| c.is_ascii_alphanumeric() || c == b'_' || (allow_hyphen && c == b'-'))
}

fn valid_directive(value: &str) -> bool {
    value == "*" || valid_ident(value, true)
}

fn valid_relation(value: &str) -> bool {
    value == "*" || valid_ident(value, false)
}

fn valid_state_path(value: &str, require_dot: bool) -> bool {
    let mut count = 0usize;
    for segment in value.split('.') {
        if !valid_ident(segment, false) {
            return false;
        }
        count += 1;
    }
    !require_dot || count >= 2
}

fn valid_state_pattern(value: &str) -> bool {
    if value == "*" {
        return true;
    }
    match value.strip_suffix(".*") {
        Some(prefix) => valid_state_path(prefix, false),
        None => valid_state_path(value, true),
    }
}

fn valid_bridge(value: &str) -> bool {
    if value == "*" {
        return true;
    }
    let Some((service, operation)) = value.split_once('/') else {
        return false;
    };
    !operation.contains('/') && valid_ident(service, true) && valid_ident(operation, true)
}

impl PermissionSet {
    fn normalized(mut self) -> Self {
        normalize_wildcard(&mut self.directives);
        normalize_wildcard(&mut self.state_writes);
        normalize_wildcard(&mut self.fact_writes);
        normalize_wildcard(&mut self.bridges);
        if self.rewards == Some(true) {
            self.rewards = None;
        }
        if self.quests == Some(true) {
            self.quests = None;
        }
        self
    }

    fn is_unrestricted(&self) -> bool {
        set_is_unrestricted(&self.directives)
            && set_is_unrestricted(&self.state_writes)
            && set_is_unrestricted(&self.fact_writes)
            && set_is_unrestricted(&self.bridges)
            && self.rewards != Some(false)
            && self.quests != Some(false)
    }
}

fn normalize_wildcard(set: &mut Option<BTreeSet<String>>) {
    if set.as_ref().is_some_and(|values| values.contains("*")) {
        *set = None;
    }
}

fn set_is_unrestricted(set: &Option<BTreeSet<String>>) -> bool {
    set.as_ref().is_none_or(|values| values.contains("*"))
}

/// Effective permissions represented as a conjunction of retained layers.
///
/// Keeping the layers avoids unsound attempts to intersect wildcard path
/// patterns. A candidate is allowed only when every layer allows it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Permissions {
    pub layers: Vec<PermissionSet>,
}

impl Permissions {
    pub fn is_unrestricted(&self) -> bool {
        self.layers.iter().all(PermissionSet::is_unrestricted)
    }

    pub fn allows_directive(&self, name: &str) -> bool {
        self.layers
            .iter()
            .all(|layer| allows_exact(&layer.directives, name))
    }

    pub fn allows_state_write(&self, path: &str) -> bool {
        self.layers
            .iter()
            .all(|layer| allows_state(&layer.state_writes, path))
    }

    pub fn allows_fact_write(&self, relation: &str) -> bool {
        self.layers
            .iter()
            .all(|layer| allows_exact(&layer.fact_writes, relation))
    }

    pub fn allows_bridge(&self, service: &str, operation: &str) -> bool {
        self.layers.iter().all(|layer| {
            layer.bridges.as_ref().is_none_or(|allowed| {
                allowed.contains("*")
                    || allowed.iter().any(|candidate| {
                        candidate
                            .split_once('/')
                            .is_some_and(|(s, o)| s == service && o == operation)
                    })
            })
        })
    }

    pub fn allows_rewards(&self) -> bool {
        self.layers.iter().all(|layer| layer.rewards != Some(false))
    }

    pub fn allows_quests(&self) -> bool {
        self.layers.iter().all(|layer| layer.quests != Some(false))
    }

    /// Add every restrictive layer in `other` to this conjunction.
    pub fn restrict(&mut self, other: &Permissions) {
        for layer in &mut self.layers {
            *layer = std::mem::take(layer).normalized();
        }
        self.layers.retain(|layer| !layer.is_unrestricted());
        self.layers.extend(
            other
                .layers
                .iter()
                .cloned()
                .map(PermissionSet::normalized)
                .filter(|layer| !layer.is_unrestricted()),
        );
    }

    pub(crate) fn normalized(&self) -> Self {
        let mut normalized = Self::default();
        normalized.restrict(self);
        normalized
    }

    pub(crate) fn push(&mut self, layer: PermissionSet) {
        let layer = layer.normalized();
        if !layer.is_unrestricted() {
            self.layers.push(layer);
        }
    }
}

fn allows_exact(set: &Option<BTreeSet<String>>, candidate: &str) -> bool {
    set.as_ref()
        .is_none_or(|allowed| allowed.contains("*") || allowed.contains(candidate))
}

fn allows_state(set: &Option<BTreeSet<String>>, path: &str) -> bool {
    set.as_ref().is_none_or(|allowed| {
        allowed.contains("*")
            || allowed.iter().any(|pattern| {
                if let Some(prefix) = pattern.strip_suffix(".*") {
                    path.len() > prefix.len()
                        && path.starts_with(prefix)
                        && path.as_bytes().get(prefix.len()) == Some(&b'.')
                } else {
                    pattern == path
                }
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(yaml: &str) -> PermissionSet {
        serde_yaml::from_str(yaml).unwrap()
    }

    #[test]
    fn absent_and_wildcard_fields_are_unrestricted() {
        assert!(Permissions::default().is_unrestricted());
        let mut permissions = Permissions::default();
        permissions.push(parse(
            "directives: ['*']\nstateWrites: ['*', scene.dialogue]\nrewards: true\n",
        ));
        assert!(permissions.is_unrestricted());
        assert!(permissions.allows_directive("anything"));
        assert!(permissions.allows_state_write("scene.anything"));
        assert!(permissions.allows_rewards());
    }

    #[test]
    fn explicit_empty_set_denies_its_category() {
        let mut permissions = Permissions::default();
        permissions.push(parse("directives: []\nfactWrites: []\nbridges: []\n"));
        assert!(!permissions.allows_directive("bg"));
        assert!(!permissions.allows_fact_write("foundClue"));
        assert!(!permissions.allows_bridge("dialogue", "respond"));
        assert!(permissions.allows_state_write("scene.line"));
    }

    #[test]
    fn exact_wildcard_and_descendant_state_patterns_match() {
        let mut permissions = Permissions::default();
        permissions.push(parse(
            "directives: [camera, use]\nstateWrites: [scene.line, scene.dialogue.*]\nfactWrites: [foundClue]\nbridges: [dialogue/respond]\n",
        ));
        assert!(permissions.allows_directive("camera"));
        assert!(!permissions.allows_directive("bg"));
        assert!(permissions.allows_state_write("scene.line"));
        assert!(permissions.allows_state_write("scene.dialogue.current"));
        assert!(!permissions.allows_state_write("scene.dialogue"));
        assert!(!permissions.allows_state_write("scene.dialogueOther.current"));
        assert!(permissions.allows_fact_write("foundClue"));
        assert!(!permissions.allows_fact_write("other"));
        assert!(permissions.allows_bridge("dialogue", "respond"));
        assert!(!permissions.allows_bridge("dialogue", "other"));
    }

    #[test]
    fn malformed_unknown_and_null_permission_config_is_rejected() {
        for yaml in [
            "directive: [bg]",
            "directives: null",
            "rewards: null",
            "directives: bg",
            "directives: ['::bg']",
            "stateWrites: [scene]",
            "stateWrites: [scene.*.value]",
            "factWrites: [found-clue]",
            "bridges: [dialogue.respond]",
            "bridges: [dialogue/respond/extra]",
        ] {
            assert!(
                serde_yaml::from_str::<PermissionSet>(yaml).is_err(),
                "must reject {yaml:?}"
            );
        }
    }

    #[test]
    fn serialized_permission_names_are_camel_case_and_sets_are_sorted() {
        let set = parse("stateWrites: [scene.z, scene.a]\nfactWrites: [zRelation, aRelation]\n");
        let json = serde_json::to_value(&set).unwrap();
        assert_eq!(
            json["stateWrites"],
            serde_json::json!(["scene.a", "scene.z"])
        );
        assert_eq!(
            json["factWrites"],
            serde_json::json!(["aRelation", "zRelation"])
        );
        assert!(json.get("state_writes").is_none());
    }
}
