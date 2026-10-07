use super::directives::AttrDecl;
use super::SchemaError;
use crate::types::{Field, PathSegment, Type};
use serde::{Deserialize, Serialize};
/// Deserialize `DefDecl.params` in SOURCE order (dsl §8.1). Accepts either the
/// §8.1 `params:` YAML MAPPING (`{ p: number }`) — read via `serde_yaml::Mapping`,
/// which is insertion-ordered in serde_yaml 0.9.34, so declaration order is
/// preserved for positional arg binding — OR a SEQUENCE of `{ name, type }`
/// entries (the plugin `defs.yaml` list spelling). A malformed mapping entry is
/// skipped, never a panic.
fn de_params<'de, D>(d: D) -> Result<Vec<DefParam>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    use serde::Deserialize;
    Ok(match serde_yaml::Value::deserialize(d)? {
        serde_yaml::Value::Sequence(v) => {
            let mut params = Vec::new();
            for value in v {
                match serde_yaml::from_value::<DefParam>(value.clone()) {
                    Ok(param) => params.push(param),
                    Err(_) if value.get("type").and_then(|v| v.as_str()) == Some("number") => {
                        return Err(D::Error::custom(crate::types::NUMBER_TYPE_REMOVED));
                    }
                    Err(_) => {}
                }
            }
            params
        }
        serde_yaml::Value::Mapping(m) => {
            let mut params = Vec::new();
            for (key, value) in m {
                let Some(name) = key.as_str() else { continue };
                if value.as_str() == Some("number") {
                    return Err(D::Error::custom(crate::types::NUMBER_TYPE_REMOVED));
                }
                if let Ok(ty) = serde_yaml::from_value::<Type>(value) {
                    params.push(DefParam { name: name.to_string(), ty });
                }
            }
            params
        }
        _ => {
            return Err(D::Error::custom(
                "`params:` maps each parameter to its type (`params: { n: number }`) or lists \
                 `{ name, type }` entries",
            ))
        }
    })
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateShape {
    pub name: String,
    pub fields: Vec<Field>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateTemplate {
    pub name: String,
    pub scope: String,
    pub path: Vec<PathSegment>,
    pub shape: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderDecl {
    pub name: String,
    #[serde(rename = "idShape", default, skip_serializing_if = "Option::is_none")]
    pub id_shape: Option<String>,
    pub snapshot: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BridgeCapability {
    pub service: String,
    pub operation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replay: Option<String>,
    #[serde(default)]
    pub result: Vec<Field>,
}

/// A single declared def parameter (dsl §8.1). Order-preserving: the position of
/// a `DefParam` in [`DefDecl::params`] is the positional-binding order for
/// `@name(args)` calls.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DefParam {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: Type,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefDecl {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: Type,
    #[serde(default, deserialize_with = "de_params")]
    pub params: Vec<DefParam>,
    pub cel: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<Vec<String>>,
}

/// A capability-declared world event (dsl 0.2.0 §4.5): a named event kind an
/// active plugin makes fireable via `<on event="…">`. Payload (if any) is
/// ordinary plugin `state`, written by the engine before the event fires — NOT
/// part of this declaration. Name is a `CelIdent`-shaped event kind.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventDecl {
    pub name: String,
}

/// plugin §14 / dsl 0.16.0 §4 reward-kind declaration file (export
/// `rewardKinds/*.yaml`).
///
/// A `rewardKinds:` mapping keyed by the kind id; a bare `{}` value declares
/// a shape-only kind (any `<reward kind>` name check passes without a
/// `target` constraint), and a `{ target: { provider: <name> } }` value
/// pins the id-space `target=` resolves against — the `providerRef` pattern
/// the checker already applies to directive-attr `providerRef:` values.
/// The optional `attrs:` sequence is an ordinary [`AttrDecl`] list for
/// game-specific slots (`chance=`, `min`/`max` variants, …) — parsed and
/// carried on the merged [`RewardKindDecl`] so a downstream extra-attr check
/// can key off it; the shape-only checker (Task 2) does not read it yet.
///
/// Per-package duplicate kind id is a [`crate::loader::LoadError::DuplicateId`]
/// (`kind = "rewardKind"`) from the shared `merge_named` path; a name colliding
/// with a peer plugin's kind is [`crate::assemble::AssembleError::DuplicateAcrossPlugins`]
/// (`kind = "rewardKind"`), same shape every other cross-plugin vocabulary
/// duplicate uses.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RewardKindsFile {
    #[serde(rename = "rewardKinds")]
    pub reward_kinds: std::collections::BTreeMap<String, RewardKindBody>,
}

/// The value half of a `rewardKinds:` map entry — the fields on the wire that
/// are NOT the kind id (the id is the map key, materialized onto
/// [`RewardKindDecl::name`] by the loader).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RewardKindBody {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<RewardTarget>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<AttrDecl>,
    /// dsl 0.23.0 §8: the state path a grant of this kind credits.
    #[serde(default)]
    pub credits: Option<String>,
}

/// A reward kind's `target:` contract (dsl 0.16.0 §4, 0.26.0 §2.5): the
/// id-space a `<reward target=…>` value must resolve against. `provider`
/// names one of the active-snapshot providers — the `providerRef` pattern —
/// and assembly rejects a kind that pins a provider no active plugin
/// declares (parallels [`crate::assemble::AssembleError::UnknownAssetKind`]).
/// `entity` names a project entity kind instead (checked against the merged
/// `entities:`). At most one of the two; `required: true` rejects a reward
/// of this kind with no `target=` at all. The checker reports a violation
/// as `E-REWARD-TARGET`.
#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, try_from = "RawRewardTarget")]
pub struct RewardTarget {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub required: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRewardTarget {
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    entity: Option<String>,
    #[serde(default)]
    required: bool,
}

impl TryFrom<RawRewardTarget> for RewardTarget {
    type Error = SchemaError;
    fn try_from(raw: RawRewardTarget) -> Result<Self, SchemaError> {
        if raw.provider.is_some() && raw.entity.is_some() {
            return Err(SchemaError::RewardTarget(
                "a reward kind's `target:` takes `provider:` or `entity:`, not both".to_string(),
            ));
        }
        Ok(RewardTarget {
            provider: raw.provider,
            entity: raw.entity,
            required: raw.required,
        })
    }
}

/// Hand-written so a pre-0.26 `{ provider: p }` contract prints exactly as
/// it did (`capabilitySnapshot` hashes [`RewardKindDecl`]'s `Debug`): only the
/// fields a contract sets are shown.
impl std::fmt::Debug for RewardTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = f.debug_struct("RewardTarget");
        if let Some(p) = &self.provider {
            s.field("provider", p);
        }
        if let Some(e) = &self.entity {
            s.field("entity", e);
        }
        if self.required {
            s.field("required", &true);
        }
        s.finish()
    }
}

/// A capability-declared reward kind (dsl 0.16.0 §4): the vocabulary a
/// plugin's `rewardKinds:` export contributes, consumed by the checker's
/// `E-REWARD-KIND` closure. `name` is the kind id (the map key, materialized
/// onto the struct at load-time); `target` (optional) pins the id space
/// `<reward target=…>` resolves against — the `providerRef` pattern
/// (`assemble` rejects a kind whose provider is absent); `attrs` (optional)
/// carries game-specific extra slots parsed as ordinary [`AttrDecl`]s.
/// `credits` (dsl 0.23.0 §8, optional) names the state path a grant of this
/// kind adds its amount to (`lute run` / `lute play` apply it).
#[derive(Clone, Serialize, Deserialize, Default)]
pub struct RewardKindDecl {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<RewardTarget>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<AttrDecl>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credits: Option<String>,
}

/// Hand-written so a kind without `credits` prints exactly as it did before
/// the field existed: `capabilitySnapshot` hashes this `Debug`, and a
/// vocabulary that declares no credit path must keep its stamp.
impl std::fmt::Debug for RewardKindDecl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = f.debug_struct("RewardKindDecl");
        s.field("name", &self.name)
            .field("target", &self.target)
            .field("attrs", &self.attrs);
        if let Some(credits) = &self.credits {
            s.field("credits", credits);
        }
        s.finish()
    }
}

/// dsl 0.23.0 §7 cast declaration file (export `cast/*.yaml`, or a schema
/// document's `cast:` key): a `cast:` mapping keyed by the speaker id.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CastFile {
    pub cast: std::collections::BTreeMap<String, CastBody>,
}

/// The value half of a `cast:` map entry (the id is the map key).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CastBody {
    #[serde(default)]
    pub name: Option<String>,
    /// dsl 0.24.0 §4: the CEL condition under which this speaker is present
    /// (`present: "holds(inParty(isolde))"`).
    #[serde(default)]
    pub present: Option<String>,
    /// dsl 0.24.0 §4: the `emotion=` values this speaker takes.
    #[serde(default)]
    pub emotions: Option<Vec<String>>,
    /// dsl 0.24.0 §4: `assume: true` — presence assumes the engine has not
    /// asserted any engine-`reserved` relation `present` negates
    /// (`!holds(fell(isolde))`, directly or through a rule).
    #[serde(default)]
    pub assume: Option<bool>,
    /// Prerelease N6 (dsl 0.26.0 §2.8): `sharedName: true` — this entry's
    /// display name is a role name meant for many speakers ("Eclipse
    /// Grunt"); `W-DISPLAY-NAME-DUP` does not count it.
    #[serde(default, rename = "sharedName")]
    pub shared_name: Option<bool>,
}

impl CastBody {
    /// The member this entry declares under `id`.
    pub fn into_member(self, id: String) -> CastMember {
        CastMember {
            id,
            name: self.name,
            present: self.present,
            emotions: self.emotions,
            assume: self.assume,
            shared_name: self.shared_name,
        }
    }
}

/// One declared cast member (dsl 0.23.0 §7): a speaker id and its display
/// name. When any cast is declared, a speaker outside it is `E-CAST-UNKNOWN`.
/// dsl 0.24.0 §4: `present` — a line by this speaker whose enclosing guards
/// do not imply it is `W-CAST-ABSENT`; `emotions` — the `emotion=` values the
/// speaker takes (`E-BAD-ENUM` outside it); `assume` — presence treats a
/// negated engine-`reserved` relation in `present` as holding;
/// `shared_name` — the display name is an intended role name, exempt from
/// `W-DISPLAY-NAME-DUP` (dsl 0.26.0 §2.8).
#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CastMember {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub present: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emotions: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assume: Option<bool>,
    #[serde(
        default,
        rename = "sharedName",
        skip_serializing_if = "Option::is_none"
    )]
    pub shared_name: Option<bool>,
}

/// Hand-written so a member without `present`/`emotions` prints exactly as
/// it did before those fields existed: `capabilitySnapshot` hashes this
/// `Debug`, and a cast that declares neither must keep its stamp.
impl std::fmt::Debug for CastMember {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = f.debug_struct("CastMember");
        s.field("id", &self.id).field("name", &self.name);
        if let Some(present) = &self.present {
            s.field("present", present);
        }
        if let Some(emotions) = &self.emotions {
            s.field("emotions", emotions);
        }
        if let Some(assume) = &self.assume {
            s.field("assume", assume);
        }
        if let Some(shared) = &self.shared_name {
            s.field("shared_name", shared);
        }
        s.finish()
    }
}

