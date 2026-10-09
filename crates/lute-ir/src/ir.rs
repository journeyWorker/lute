//! The IR row types an artifact and the project index share.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};

use crate::ExprNode;

/// A CEL slot's standard text (`cel`) plus its portable lowered form.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CelPair {
    #[serde(rename = "cel")]
    pub raw: String,
    pub expr: ExprNode,
    /// A seam condition as the author wrote it when expansion changed it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authored: Option<String>,
}

impl CelPair {
    /// The condition as its author wrote it: [`CelPair::authored`], else
    /// `raw`.
    pub fn shown(&self) -> &str {
        self.authored.as_deref().unwrap_or(&self.raw)
    }
}

/// A scene beat's repetition policy (dsl 0.21.0 §3.1, 0.24.0 §1, 0.27.0
/// §5): `"run"` (once per run), `"user"` (once ever), `"none"` (repeatable;
/// source `once: false`), `"day"` / `"slot"` /
/// `"week"` (once per clock day / slot / week), `"season:<name>"` (once per
/// window of that season). A wire mirror of
/// `lute_check::BeatOnce`, kept separate for the same reason as
/// [`DocKind`]; it serializes as its spelling.
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
    /// The IR spelling.
    pub fn as_str(&self) -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed(match self {
            BeatOnce::Run => "run",
            BeatOnce::User => "user",
            BeatOnce::None => "none",
            BeatOnce::Day => "day",
            BeatOnce::Slot => "slot",
            BeatOnce::Week => "week",
            BeatOnce::Season(name) => {
                return std::borrow::Cow::Owned(format!(
                    "{}{name}",
                    lute_manifest::season::SEASON_PREFIX
                ))
            }
        })
    }

    /// The policy an IR spelling ([`BeatOnce::as_str`]) names.
    pub fn from_ir(spelling: &str) -> Option<Self> {
        Some(match spelling {
            "run" => BeatOnce::Run,
            "user" => BeatOnce::User,
            "none" => BeatOnce::None,
            "day" => BeatOnce::Day,
            "slot" => BeatOnce::Slot,
            "week" => BeatOnce::Week,
            _ => BeatOnce::Season(
                spelling
                    .strip_prefix(lute_manifest::season::SEASON_PREFIX)?
                    .to_string(),
            ),
        })
    }

    /// Spent per clock period (`day` / `slot` / `week`).
    pub fn is_clock(&self) -> bool {
        matches!(self, BeatOnce::Day | BeatOnce::Slot | BeatOnce::Week)
    }

    /// The season a `once: season:<name>` names.
    pub fn season(&self) -> Option<&str> {
        match self {
            BeatOnce::Season(name) => Some(name),
            _ => None,
        }
    }
}

impl Serialize for BeatOnce {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.as_str())
    }
}

/// Reads the IR spelling ([`BeatOnce::from_ir`]).
impl<'de> Deserialize<'de> for BeatOnce {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::from_ir(&s)
            .ok_or_else(|| serde::de::Error::custom(format!("invalid once policy {s:?}")))
    }
}

/// How a presented beat moves the engine clock (dsl 0.31.0 §1). This is a
/// declaration only; the engine performs the move when the beat is presented.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdvanceSpec {
    Slot,
    Day,
    Slots(u32),
}

impl AdvanceSpec {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Slot => "slot",
            Self::Day => "day",
            Self::Slots(_) => "n",
        }
    }
}

impl serde::Serialize for AdvanceSpec {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Slot => serializer.serialize_str("slot"),
            Self::Day => serializer.serialize_str("day"),
            Self::Slots(n) => serializer.serialize_u32(*n),
        }
    }
}

/// Reads `"slot"`, `"day"` or a slot count.
impl<'de> Deserialize<'de> for AdvanceSpec {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match serde_json::Value::deserialize(d)? {
            serde_json::Value::String(s) if s == "slot" => Ok(Self::Slot),
            serde_json::Value::String(s) if s == "day" => Ok(Self::Day),
            serde_json::Value::Number(n) => n
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .filter(|n| *n >= 1)
                .map(Self::Slots)
                .ok_or_else(|| serde::de::Error::custom("invalid advance count")),
            _ => Err(serde::de::Error::custom("invalid advance")),
        }
    }
}

/// dsl 0.26.0 §5: a `target="kind:<kind>"` beat, resolved against the
/// occasion's `{ prefix, entity }` target domain. It answers a raise for
/// `<prefix>.<member>` of every listed member (a sub-kind's members already
/// counted in its parent); while it runs, `occasion.target` is the raised
/// member's id (`<member>`, the prefix stripped), typed by the kind.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetKind {
    pub kind: String,
    pub prefix: String,
    pub members: Vec<String>,
}

impl TargetKind {
    /// The member a raise for `target` binds, when this kind answers it.
    pub fn member_of<'t>(&self, target: &'t str) -> Option<&'t str> {
        let member = target
            .strip_prefix(self.prefix.as_str())?
            .strip_prefix('.')?;
        self.members.iter().any(|m| m == member).then_some(member)
    }
}

/// dsl 0.27.0 §3 (T2-10): a `for="kind:<kind>"` beat on an untargeted
/// `select: sequence` occasion. Each raise presents it once per listed
/// member whose `when` holds, in member order; while one presentation
/// runs, `occasion.target` is that member's id.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForKind {
    pub kind: String,
    pub members: Vec<String>,
}

/// Document kind (dsl 0.2.0 §2/§3.1, dsl 0.19.0 §2): `"scene"` | `"quest"` |
/// `"lore"`, mirrors
/// `lute_check::meta::DocKind` — kept as a SEPARATE serde enum
/// so `Serialize` never leaks onto lute-check's public type (serialization
/// concerns stay in the crate that owns the wire format).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DocKind {
    #[default]
    Scene,
    Quest,
    Lore,
}

/// One merged entity kind (dsl 0.3.0 §3.1).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityKindEntry {
    pub name: String,
    /// `None` for `open: engine` kinds (§3.1) — the engine mints members.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub members: Option<Vec<String>>,
    #[serde(default)]
    pub open: bool,
    /// dsl 0.27.0 §7: member → display text (`labels:`, sub-kind labels
    /// implied), what a `{{…}}` of a value of this kind renders — an
    /// `occasionTarget` placeholder of `entityKind` this kind renders
    /// `labels[member]` (over a cast member's `name:`). Omitted when empty.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub labels: BTreeMap<String, String>,
    /// The declared label forms (`labels: { cut: { text, start, indefinite
    /// } }`) per member that declares one — what a `:start` / `:indefinite`
    /// placeholder of a value of this kind renders. Omitted when empty.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub label_forms: BTreeMap<String, LabelForms>,
}

/// One member's declared label forms beside its `labels` text: `start` is
/// the sentence-start form (`:start`), `indefinite` the form with its
/// article (`:indefinite`). An absent form falls back: `start` to the text
/// with its first letter capitalized, `indefinite` to `a` / `an` (by the
/// text's first letter, `an` before a vowel) and the text.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LabelForms {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub indefinite: Option<String>,
}

impl LabelForms {
    /// The forms `labels` declares, per member that declares one.
    pub fn of(
        labels: &BTreeMap<String, lute_manifest::relations::KindLabel>,
    ) -> BTreeMap<String, LabelForms> {
        labels
            .iter()
            .filter(|(_, l)| l.has_forms())
            .map(|(m, l)| {
                (
                    m.clone(),
                    LabelForms {
                        start: l.start.clone(),
                        indefinite: l.indefinite.clone(),
                    },
                )
            })
            .collect()
    }
}

/// One merged `enums:` entry (dsl 0.3.0 §3).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EnumEntry {
    pub name: String,
    pub members: Vec<String>,
}

/// One merged relation declaration (dsl 0.3.0 §4).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelationEntry {
    pub name: String,
    pub args: Vec<String>,
    /// Effective tier for base relations (default `run` applied); ABSENT
    /// for `derive: true` (§4 — a derived relation has no write tier).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tier: Option<String>,
    #[serde(default)]
    pub derive: bool,
    #[serde(default)]
    pub reserved: bool,
    /// 0-based functional-key arg indices (§4); empty when undeclared.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub key: Vec<usize>,
    /// dsl 0.25.0 §1: the relations this one can never hold together with on
    /// the same arguments — the symmetric closure of `excludes:`, sorted;
    /// empty (absent) when none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excludes: Vec<String>,
}

/// One seed `facts:` ground tuple (dsl 0.3.0 §4).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SeedFactEntry {
    pub relation: String,
    /// Ground literals as strings; bools serialize as `"true"`/`"false"`
    /// (§4 — seed facts are ground, never `_`).
    pub args: Vec<String>,
}

/// One Datalog rule (dsl 0.3.0 §7.1), emitted as STRUCTURED data — head +
/// body — for the engine's least-fixpoint evaluator. Lute performs NO
/// evaluation (D1); this is the declared rule set, verbatim.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleEntry {
    pub head: AtomEntry,
    pub body: Vec<BodyEntry>,
    /// The rule's original source text (`rules:` entry), for engine
    /// diagnostics/tooling.
    pub raw: String,
}

/// One rule atom: a relation name applied to terms (dsl 0.3.0 §7.1).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AtomEntry {
    pub relation: String,
    pub terms: Vec<TermEntry>,
}

/// One rule term: a variable (leading-uppercase ident) or a ground constant
/// (dsl 0.3.0 §7.1). Bools lower to `Const` with a `"true"`/`"false"` value.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TermEntry {
    Var { name: String },
    Const { value: String },
}

/// One rule body literal (dsl 0.3.0 §7.1): a positive/negated atom, a CEL
/// guard, a term comparison, or (dsl 0.26.0 §6) a count.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum BodyEntry {
    Atom {
        atom: AtomEntry,
        negated: bool,
    },
    Guard {
        cel: CelPair,
    },
    Cmp {
        lhs: TermEntry,
        rhs: TermEntry,
        negated: bool,
    },
    /// `count(atom) op n` — the number of facts matching `atom` — or, with
    /// `distinct`, `countDistinct(atom, V…) op n`: the number of distinct
    /// values of those variables among them. A variable of `atom` bound by
    /// another literal is read; any other ranges over the facts. `op` is
    /// one of `==`, `!=`, `<`, `<=`, `>`, `>=`. `atom`'s relation sits in a
    /// strictly lower stratum than the rule's head.
    Count {
        atom: AtomEntry,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        distinct: Vec<String>,
        op: String,
        n: u64,
    },
}

/// One advisory prerequisite edge (connectivity spec §2.6, T13): a single
/// node's raw declared edge formula text, verbatim and unvalidated.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrereqEdgeEntry {
    /// The contributing node's canonical key: a scene's own
    /// `{character}.{episodeId}` (`lute_check::meta::canonical_episode_key`)
    /// or a quest's `<quest id>`.
    pub node: String,
    /// The RAW declared formula, keyed by what it means on the wire.
    #[serde(flatten)]
    pub edge: PrereqEdge,
    /// The parsed `after` formula (`lute_check::parse_prereq`), so the
    /// runtime never parses `after` text. `None` for `follows` edges and for
    /// an `after` outside the prerequisite profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formula: Option<lute_manifest::semantics::prereq::PrereqFormula>,
}

/// The two kinds of graph edge a node declares, serialized as the single
/// key `after` or `follows` of its [`PrereqEdgeEntry`].
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PrereqEdge {
    /// A scene's `after:` / a bundle beat's `after=`: an eligibility gate.
    After(String),
    /// A quest's `follows=`: graph metadata only — it never gates the quest.
    Follows(String),
}

/// dsl 0.27.0 §4: one occasion's `raisedWhen` gate.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GateEntry {
    pub occasion: String,
    pub raised_when: CelPair,
}

/// dsl 0.27.0 §5: one declared season.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeasonEntry {
    pub name: String,
    pub live: CelPair,
}
