use super::directives::{AttrDecl, FactEffect, RaiseDecl, RaiseTargetDecl};
use super::SchemaError;
use crate::types::{Field, PathSegment, Type};
use serde::{Deserialize, Serialize};
/// Deserialize `DefDecl.params` in SOURCE order (dsl §8.1). Accepts either the
/// §8.1 `params:` YAML MAPPING (`{ p: number }`) — read via `yaml_serde::Mapping`,
/// which is insertion-ordered in yaml_serde, so declaration order is
/// preserved for positional arg binding — OR a SEQUENCE of `{ name, type }`
/// entries (the plugin `defs.yaml` list spelling). A malformed mapping entry is
/// skipped, never a panic.
fn de_params<'de, D>(d: D) -> Result<Vec<DefParam>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    Ok(match yaml_serde::Value::deserialize(d)? {
        yaml_serde::Value::Sequence(v) => {
            let mut params = Vec::new();
            for value in v {
                match yaml_serde::from_value::<DefParam>(value.clone()) {
                    Ok(param) => params.push(param),
                    Err(_) if value.get("type").and_then(|v| v.as_str()) == Some("number") => {
                        return Err(D::Error::custom(crate::types::NUMBER_TYPE_REMOVED));
                    }
                    Err(_) => {}
                }
            }
            params
        }
        yaml_serde::Value::Mapping(m) => {
            let mut params = Vec::new();
            for (key, value) in m {
                let Some(name) = key.as_str() else { continue };
                if value.as_str() == Some("number") {
                    return Err(D::Error::custom(crate::types::NUMBER_TYPE_REMOVED));
                }
                if let Ok(ty) = yaml_serde::from_value::<Type>(value) {
                    params.push(DefParam {
                        name: name.to_string(),
                        ty,
                    });
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
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DefParam {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: Type,
}

#[derive(Clone, Serialize, Deserialize)]
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
    /// 0.41.0 §3.5: the host reads this def (`Program.def`), so it is never
    /// unused; exported as `host: true` in the index `defs`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub host: bool,
}

/// The derived shape, with `host` only when it is `true`: `capabilitySnapshot`
/// hashes this text, so a snapshot without host defs keeps its 0.40 identity.
impl std::fmt::Debug for DefDecl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = f.debug_struct("DefDecl");
        s.field("name", &self.name)
            .field("ty", &self.ty)
            .field("params", &self.params)
            .field("cel", &self.cel)
            .field("min", &self.min)
            .field("max", &self.max)
            .field("values", &self.values);
        if self.host {
            s.field("host", &self.host);
        }
        s.finish()
    }
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
    /// 0.50.0 §5.1 (#169): the facts a grant of this kind asserts and
    /// retracts and the quests it accepts.
    #[serde(default)]
    pub effects: Option<RewardEffects>,
}

/// A reward kind's `effects:` (0.50.0 §5.1): what a grant of the kind does
/// besides its `credits:` write — the facts it retracts, then asserts
/// ([`FactEffect`] patterns, the directive grammar; an `@attr` may only be
/// `@target`), then the quests it accepts — and (0.51.0 §4.1) the occasions
/// the host raises after settling the grant. Every list is optional, but
/// the block declares at least one effect. A reward's state write is
/// `credits:`, so `writes` is no key here.
#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawRewardEffects")]
pub struct RewardEffects {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub asserts: Vec<FactEffect>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub retracts: Vec<FactEffect>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accepts: Vec<AcceptEffect>,
    /// 0.51.0 §4.1 (#170): the occasions the host raises for the grant.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub raises: Vec<RaiseDecl>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRewardEffects {
    #[serde(default)]
    asserts: Vec<FactEffect>,
    #[serde(default)]
    retracts: Vec<FactEffect>,
    #[serde(default)]
    accepts: Vec<AcceptEffect>,
    #[serde(default, deserialize_with = "super::directives::raise_list")]
    raises: Vec<RaiseDecl>,
}

impl TryFrom<RawRewardEffects> for RewardEffects {
    type Error = SchemaError;
    fn try_from(raw: RawRewardEffects) -> Result<Self, SchemaError> {
        if raw.asserts.is_empty()
            && raw.retracts.is_empty()
            && raw.accepts.is_empty()
            && raw.raises.is_empty()
        {
            return Err(SchemaError::RewardEffects(
                "a reward kind's `effects:` declares no effect; list at least one of `asserts:`, \
                 `retracts:`, `accepts:` or `raises:`, or remove `effects:`"
                    .to_string(),
            ));
        }
        Ok(RewardEffects {
            asserts: raw.asserts,
            retracts: raw.retracts,
            accepts: raw.accepts,
            raises: raw.raises,
        })
    }
}

impl RewardEffects {
    /// The fact patterns in the order a grant applies them: `retracts`,
    /// then `asserts` (`true` for an assert).
    pub fn facts(&self) -> impl Iterator<Item = (&FactEffect, bool)> {
        self.retracts
            .iter()
            .map(|f| (f, false))
            .chain(self.asserts.iter().map(|f| (f, true)))
    }

    /// Whether any effect reads the reward's `@target` — a fact, an accept
    /// or (0.51.0 §4.1) a raise.
    pub fn reads_target(&self) -> bool {
        self.facts()
            .any(|(f, _)| f.attrs().any(|a| a == REWARD_TARGET_ATTR))
            || self.accepts.contains(&AcceptEffect::Target)
            || self.target_raises().next().is_some()
    }

    /// The raises whose target is the reward's `@target`.
    pub fn target_raises(&self) -> impl Iterator<Item = &RaiseDecl> {
        self.raises.iter().filter(|r| {
            r.target.as_ref().and_then(RaiseTargetDecl::attr) == Some(REWARD_TARGET_ATTR)
        })
    }
}

/// Printed as the four lists, each only when non-empty.
impl std::fmt::Debug for RewardEffects {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = f.debug_struct("RewardEffects");
        if !self.asserts.is_empty() {
            s.field("asserts", &self.asserts);
        }
        if !self.retracts.is_empty() {
            s.field("retracts", &self.retracts);
        }
        if !self.accepts.is_empty() {
            s.field("accepts", &self.accepts);
        }
        if !self.raises.is_empty() {
            s.field("raises", &self.raises);
        }
        s.finish()
    }
}

/// The one attribute a reward kind's effects may read: the `<reward>`'s
/// quoted `target=` (0.50.0 §5.1; a `<reward>` takes no other attribute).
pub const REWARD_TARGET_ATTR: &str = "target";

/// One `effects.accepts` entry (0.50.0 §5.1): the reward's `@target`, or a
/// literal quest id. Written and serialized as that one string.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum AcceptEffect {
    /// `@target`: the quest the reward's `target=` names.
    Target,
    /// A quest id, as written.
    Quest(String),
}

impl std::fmt::Display for AcceptEffect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AcceptEffect::Target => write!(f, "@{REWARD_TARGET_ATTR}"),
            AcceptEffect::Quest(id) => f.write_str(id),
        }
    }
}

impl std::fmt::Debug for AcceptEffect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.to_string())
    }
}

impl From<AcceptEffect> for String {
    fn from(a: AcceptEffect) -> String {
        a.to_string()
    }
}

impl TryFrom<String> for AcceptEffect {
    type Error = SchemaError;
    fn try_from(s: String) -> Result<Self, SchemaError> {
        let t = s.trim();
        if t.strip_prefix('@') == Some(REWARD_TARGET_ATTR) {
            return Ok(AcceptEffect::Target);
        }
        if crate::ident::is_name(t) {
            return Ok(AcceptEffect::Quest(t.to_string()));
        }
        Err(SchemaError::RewardEffects(format!(
            "a reward kind's `effects.accepts` entry `{s}` is neither `@{REWARD_TARGET_ATTR}` nor \
             a quest id; an accept names the reward's `@{REWARD_TARGET_ATTR}` or one quest id"
        )))
    }
}

/// A reward kind's `target:` contract (dsl 0.16.0 §4, 0.26.0 §2.5): the
/// id-space a `<reward target=…>` value must resolve against. `provider`
/// names one of the active-snapshot providers — the `providerRef` pattern —
/// and assembly rejects a kind that pins a provider no active plugin
/// declares (parallels [`crate::assemble::AssembleError::UnknownAssetKind`]).
/// `entity` names a project entity kind instead (checked against the merged
/// `entities:`). At most one of the two; `required: true` rejects a reward
/// of this kind with no `target=` at all, and (0.49.0 §3.3) `forbidden:
/// true` — alone — rejects one WITH a `target=`. The checker reports a
/// violation as `E-REWARD-TARGET`.
#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, try_from = "RawRewardTarget")]
pub struct RewardTarget {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub required: bool,
    /// 0.49.0 §3.3 (#168): the kind takes no `target=`. Exclusive with every
    /// other key ([`RewardTarget::try_from`]).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub forbidden: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRewardTarget {
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    entity: Option<String>,
    /// `Option` so `forbidden: true` can refuse any `required:` key, even
    /// `required: false` (0.49.0 §3.3: `forbidden: true` alone). A present
    /// key must be a bool: `required: null` is refused, as on 0.48.1, not
    /// read as omitted.
    #[serde(default, deserialize_with = "present_bool")]
    required: Option<bool>,
    #[serde(default)]
    forbidden: bool,
}

/// A present key read as a `bool` (`null` is a type error); `#[serde(default)]`
/// keeps an omitted key `None`.
fn present_bool<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<bool>, D::Error> {
    bool::deserialize(deserializer).map(Some)
}

impl TryFrom<RawRewardTarget> for RewardTarget {
    type Error = SchemaError;
    fn try_from(raw: RawRewardTarget) -> Result<Self, SchemaError> {
        if raw.provider.is_some() && raw.entity.is_some() {
            return Err(SchemaError::RewardTarget(
                "a reward kind's `target:` takes `provider:` or `entity:`, not both".to_string(),
            ));
        }
        if raw.forbidden
            && (raw.provider.is_some() || raw.entity.is_some() || raw.required.is_some())
        {
            return Err(SchemaError::RewardTarget(
                "a reward kind's `target:` takes `forbidden: true` alone — a kind that takes \
                 no target cannot also name an id space or require one"
                    .to_string(),
            ));
        }
        Ok(RewardTarget {
            provider: raw.provider,
            entity: raw.entity,
            required: raw.required.unwrap_or(false),
            forbidden: raw.forbidden,
        })
    }
}

/// Hand-written so a pre-0.26 `{ provider: p }` contract prints exactly as
/// it did (`capabilitySnapshot` hashes [`RewardKindDecl`]'s `Debug`): only the
/// fields a contract sets are shown, so a contract without `forbidden`
/// keeps its hash.
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
        if self.forbidden {
            s.field("forbidden", &true);
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
/// kind adds its amount to (`lute run` / `lute play` apply it); `effects`
/// (0.50.0 §5.1, optional) the facts it writes and the quests it accepts.
#[derive(Clone, Serialize, Deserialize, Default)]
pub struct RewardKindDecl {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<RewardTarget>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<AttrDecl>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credits: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effects: Option<RewardEffects>,
}

/// Hand-written so a kind without `credits` or `effects` prints exactly as
/// it did before those fields existed: `capabilitySnapshot` hashes this
/// `Debug`, and a vocabulary that declares neither must keep its stamp.
impl std::fmt::Debug for RewardKindDecl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = f.debug_struct("RewardKindDecl");
        s.field("name", &self.name)
            .field("target", &self.target)
            .field("attrs", &self.attrs);
        if let Some(credits) = &self.credits {
            s.field("credits", credits);
        }
        if let Some(effects) = &self.effects {
            s.field("effects", effects);
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
