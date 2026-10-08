//! Runtime-owned representation of `project.index.json`.
//!
//! The compiler owns writing the index; the runtime only consumes its stable
//! JSON shape so a shipped bundle does not depend on compiler IR types.

use std::collections::{BTreeMap};

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value as Json;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexDocument {
    pub path: String,
    pub artifact: String,
    #[serde(default)]
    pub kind: Json,
    #[serde(default)]
    pub key: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexEntry {
    pub id: String,
    pub document: String,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub series: Option<String>,
    #[serde(default)]
    pub order: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BeatKind {
    Scene,
    Entry,
    Bundle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BeatOnce {
    Run,
    User,
    None,
    Day,
    Slot,
    Week,
    Season(String),
}

impl BeatOnce {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "run" => Self::Run,
            "user" => Self::User,
            "none" | "false" => Self::None,
            "day" => Self::Day,
            "slot" => Self::Slot,
            "week" => Self::Week,
            s if s.starts_with("season:") => Self::Season(s[7..].to_string()),
            _ => return None,
        })
    }

    pub fn as_str(&self) -> std::borrow::Cow<'static, str> {
        match self {
            Self::Run => "run".into(),
            Self::User => "user".into(),
            Self::None => "none".into(),
            Self::Day => "day".into(),
            Self::Slot => "slot".into(),
            Self::Week => "week".into(),
            Self::Season(name) => format!("season:{name}").into(),
        }
    }
    pub fn season(&self) -> Option<&str> {
        match self {
            Self::Season(name) => Some(name),
            _ => None,
        }
    }
}


impl<'de> Deserialize<'de> for BeatOnce {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::parse(&s).ok_or_else(|| serde::de::Error::custom(format!("invalid once policy {s:?}")))
    }
}

impl Serialize for BeatOnce {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.as_str())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetKind {
    pub kind: String,
    pub prefix: String,
    pub members: Vec<String>,
}

impl TargetKind {
    pub fn member_of<'a>(&self, target: &'a str) -> Option<&'a str> {
        let member = target.strip_prefix(&self.prefix)?.strip_prefix('.')?;
        self.members.iter().any(|m| m == member).then_some(member)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForKind {
    pub kind: String,
    pub members: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CelPair {
    #[serde(rename = "cel")]
    pub raw: String,
    pub expr: Json,
    #[serde(default)]
    pub authored: Option<String>,
}

impl CelPair {
    pub fn shown(&self) -> &str {
        self.authored.as_deref().unwrap_or(&self.raw)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexBeat {
    pub id: String,
    pub kind: BeatKind,
    pub document: String,
    pub on: String,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub priority: i64,
    #[serde(default)]
    pub once: Option<BeatOnce>,
    #[serde(default)]
    pub when: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub share: Option<String>,
    #[serde(rename = "targetKind", default)]
    pub target_kind: Option<TargetKind>,
    #[serde(rename = "forKind", default)]
    pub for_kind: Option<ForKind>,
    #[serde(rename = "spentBy", default)]
    pub spent_by: Option<String>,
    #[serde(default)]
    pub advances: Option<AdvanceSpec>,
}

impl IndexBeat {
    pub fn answers<'a>(&self, occasion: &str, target: Option<&'a str>) -> Option<Option<&'a str>> {
        if self.on != occasion {
            return None;
        }
        match (&self.target_kind, self.target.as_deref()) {
            (Some(k), _) => target.and_then(|t| k.member_of(t)).map(Some),
            (None, None) => Some(None),
            (None, Some(t)) => (Some(t) == target).then_some(None),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdvanceSpec {
    Slot,
    Day,
    Slots(u32),
}

impl<'de> Deserialize<'de> for AdvanceSpec {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = Json::deserialize(d)?;
        match value {
            Json::String(s) if s == "slot" => Ok(Self::Slot),
            Json::String(s) if s == "day" => Ok(Self::Day),
            Json::Number(n) => n
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .map(Self::Slots)
                .ok_or_else(|| serde::de::Error::custom("invalid advance count")),
            _ => Err(serde::de::Error::custom("invalid advance")),
        }
    }
}

impl Serialize for AdvanceSpec {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Slot => s.serialize_str("slot"),
            Self::Day => s.serialize_str("day"),
            Self::Slots(n) => s.serialize_u32(*n),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityKindEntry {
    pub name: String,
    #[serde(default)]
    pub members: Option<Vec<String>>,
    #[serde(default)]
    pub open: bool,
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    #[serde(default)]
    pub label_forms: BTreeMap<String, LabelForms>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LabelForms {
    #[serde(default)]
    pub start: Option<String>,
    #[serde(default)]
    pub indefinite: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnumEntry {
    pub name: String,
    pub members: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelationEntry {
    pub name: String,
    pub args: Vec<String>,
    #[serde(default)]
    pub tier: Option<String>,
    #[serde(default)]
    pub derive: bool,
    #[serde(default)]
    pub reserved: bool,
    #[serde(default)]
    pub key: Vec<usize>,
    #[serde(default)]
    pub excludes: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SeedFactEntry {
    pub relation: String,
    pub args: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StateDomain {
    pub kind: String,
    pub members: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexOccasion {
    pub select: lute_manifest::schema::OccasionSelect,
    pub target: lute_manifest::schema::OccasionTarget,
    #[serde(default)]
    pub judge: Option<lute_manifest::schema::OccasionJudge>,
    #[serde(default)]
    pub payload: BTreeMap<String, lute_manifest::types::Type>,
    #[serde(default)]
    pub outside_run: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GateEntry {
    pub occasion: String,
    pub raised_when: CelPair,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeasonEntry {
    pub name: String,
    pub live: CelPair,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectIndex {
    pub ir_version: String,
    #[serde(default)]
    pub capability_snapshot: String,
    #[serde(default)]
    pub required_semantics: Vec<String>,
    #[serde(default)]
    pub identity_renames: Vec<Json>,
    #[serde(default)]
    pub documents: Vec<IndexDocument>,
    #[serde(default)]
    pub entities: Vec<EntityKindEntry>,
    #[serde(default)]
    pub enums: Vec<EnumEntry>,
    #[serde(default)]
    pub relations: Vec<RelationEntry>,
    #[serde(rename = "seedFacts", default)]
    pub seed_facts: Vec<SeedFactEntry>,
    #[serde(default)]
    pub rules: Vec<Json>,
    #[serde(rename = "prereqEdges", default)]
    pub prereq_edges: Vec<Json>,
    #[serde(default)]
    pub entries: Vec<IndexEntry>,
    #[serde(default)]
    pub beats: Vec<IndexBeat>,
    #[serde(default)]
    pub clock: Option<lute_manifest::clock::ClockDecl>,
    #[serde(default)]
    pub gates: Vec<GateEntry>,
    #[serde(default)]
    pub terminal: Option<CelPair>,
    #[serde(rename = "terminalPersists", default)]
    pub terminal_persists: bool,
    #[serde(default)]
    pub seasons: Vec<SeasonEntry>,
    #[serde(rename = "outsideRun", default)]
    pub outside_run: Vec<String>,
    #[serde(default)]
    pub occasions: BTreeMap<String, IndexOccasion>,
    #[serde(rename = "worldEvents", default)]
    pub world_events: Vec<String>,
    #[serde(rename = "bridgeResults", default)]
    pub bridge_results: BTreeMap<String, BTreeMap<String, String>>,
    #[serde(default)]
    pub cast: BTreeMap<String, String>,
    #[serde(rename = "stateDomains", default)]
    pub state_domains: BTreeMap<String, StateDomain>,
}

#[derive(Clone, Debug, Default)]
pub struct Bundle {
    pub artifacts: BTreeMap<String, Json>,
    pub index: Json,
}

impl Bundle {
    pub fn new(artifacts: BTreeMap<String, Json>, index: Json) -> Self {
        Self { artifacts, index }
    }
}

pub fn occasions(index: &ProjectIndex) -> BTreeMap<String, lute_manifest::schema::OccasionDecl> {
    index
        .occasions
        .iter()
        .map(|(name, value)| {
            (
                name.clone(),
                lute_manifest::schema::OccasionDecl {
                    name: name.clone(),
                    select: value.select,
                    target: value.target.clone(),
                    judge: value
                        .judge
                        .unwrap_or(lute_manifest::schema::OccasionJudge::After),
                    payload: value.payload.clone(),
                    outside_run: value.outside_run,
                    ..Default::default()
                },
            )
        })
        .collect()
}

pub fn bridge_result_types(index: &ProjectIndex) -> BTreeMap<String, BTreeMap<String, String>> {
    index.bridge_results.clone()
}
