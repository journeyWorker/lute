use super::directives::{AttrDecl, DirectiveDecl};
use super::vocabulary::{
    BridgeCapability, DefDecl, EventDecl, ProviderDecl, StateShape, StateTemplate,
};
use super::SchemaError;
use crate::types::Type;
use serde::Deserialize;
// Every struct below that a plugin export file deserializes into denies
// unknown keys (dsl 0.24.0 T1-3): a typo'd key (`selct: all`) or a flow-map
// value split at a comma (`description: Pick one, the player picks one`
// yields a null-valued key `the player picks one`) is an `E-PLUGIN-PARSE`
// naming the key, never a silently defaulted field. The loader
// (`loader::parse_error_msg`) adds the did-you-mean / quoting hint.

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DirectivesFile {
    pub directives: Vec<DirectiveDecl>,
}

/// A `state/*.yaml` export file: `stateShapes:` and/or `stateTemplates:`.
/// At least one must be present (the loader rejects a file with neither).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateFile {
    #[serde(default, rename = "stateShapes")]
    pub state_shapes: Option<Vec<StateShape>>,
    #[serde(default, rename = "stateTemplates")]
    pub state_templates: Option<Vec<StateTemplate>>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvidersFile {
    pub providers: Vec<ProviderDecl>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BridgeFile {
    #[serde(rename = "bridgeCapabilities")]
    pub bridge: Vec<BridgeCapability>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefsFile {
    pub defs: Vec<DefDecl>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrontmatterFile {
    pub frontmatter: Vec<FrontmatterDecl>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrontmatterDecl {
    pub key: String,
    pub schema: Type,
}

/// One `enums:` entry. A bare sequence is shorthand for `{ members: […] }`
/// (dsl 0.9.0 D-D), so every pre-0.9.0 `enums.yaml` keeps parsing byte-for-byte.
/// The long form MAY carry `labels: { <member>: <display text> }` (dsl 0.24.0
/// §1); a non-string label fails this deserialization. Deserialized through
/// [`EnumDecl::try_from`], so a misshapen entry names its fault and the
/// shapes an enum takes rather than serde's "did not match any variant".
#[derive(Clone, Debug, Deserialize)]
#[serde(try_from = "serde_yaml::Value")]
pub enum EnumDecl {
    Members(Vec<String>),
    Long {
        members: Vec<String>,
        #[serde(default)]
        default: Option<String>,
        #[serde(default)]
        exits: Vec<String>,
        #[serde(default)]
        labels: std::collections::BTreeMap<String, String>,
    },
}

impl TryFrom<serde_yaml::Value> for EnumDecl {
    type Error = SchemaError;

    fn try_from(v: serde_yaml::Value) -> Result<Self, SchemaError> {
        use serde_yaml::Value;
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Long {
            members: Vec<String>,
            #[serde(default)]
            default: Option<String>,
            #[serde(default)]
            exits: Vec<String>,
            #[serde(default)]
            labels: std::collections::BTreeMap<String, String>,
        }
        const SHAPES: &str = "an enum is a list of members `[calm, tense]` or \
             `{ members: [calm, tense], default: calm, exits: [tense], labels: { calm: Calm } }`";
        match &v {
            Value::Sequence(_) => serde_yaml::from_value::<Vec<String>>(v)
                .map(EnumDecl::Members)
                .map_err(|_| {
                    SchemaError::Enum(format!("lists something that is not a member name; {SHAPES}"))
                }),
            Value::Mapping(m) => {
                let keys = crate::entities::ENUM_LONG_FORM_KEYS;
                if let Some(key) = m
                    .keys()
                    .find(|k| !k.as_str().is_some_and(|k| keys.contains(&k)))
                {
                    let key = serde_yaml::to_string(key).map_or_else(|_| String::new(), |s| s);
                    let key = key.trim();
                    return Err(SchemaError::Enum(format!(
                        "has no key `{key}`{}; {SHAPES}",
                        crate::suggest::did_you_mean(key, keys)
                    )));
                }
                if !m.contains_key("members") {
                    return Err(SchemaError::Enum(format!("needs `members:`; {SHAPES}")));
                }
                let long = serde_yaml::from_value::<Long>(v).map_err(|_| {
                    SchemaError::Enum(format!(
                        "is misshapen: `members:` and `exits:` list member names and `labels:` \
                         maps each member to its text; {SHAPES}"
                    ))
                })?;
                Ok(EnumDecl::Long {
                    members: long.members,
                    default: long.default,
                    exits: long.exits,
                    labels: long.labels,
                })
            }
            _ => Err(SchemaError::Enum(format!("is not an enum; {SHAPES}"))),
        }
    }
}

impl EnumDecl {
    /// The declared members, in order.
    pub fn members(&self) -> &[String] {
        match self {
            EnumDecl::Members(members) | EnumDecl::Long { members, .. } => members,
        }
    }

    /// Project into the shared [`crate::snapshot::Domain`] shape.
    pub fn into_domain(self) -> crate::snapshot::Domain {
        match self {
            EnumDecl::Members(members) => crate::snapshot::Domain {
                members,
                ..Default::default()
            },
            EnumDecl::Long {
                members,
                default,
                exits,
                labels,
            } => crate::snapshot::Domain {
                members,
                open: false,
                default,
                exits,
                labels,
            },
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnumsFile {
    pub enums: std::collections::BTreeMap<String, EnumDecl>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventsFile {
    pub events: Vec<EventDecl>,
}

/// plugin §14.1 cross-cutting stamp-attribute declaration file (export
/// `stampAttrs/*.yaml`).
///
/// Each entry is an ordinary [`AttrDecl`] — the SAME name/required/type/default
/// surface a directive attr uses — but admissible on EVERY directive AND on
/// content lines rather than on one directive, and lowered into the IR
/// record's stamp (compile-IR §4.3 `Stamp.extra`) instead of the record's own
/// fields. A name colliding with a reserved stamp key (`at`/`duration`/
/// `delay`/`wait`/`timeline`/`provenance`/`source`) is an assembly-time
/// `E-PLUGIN-RESERVED-STAMP-ATTR` (plugin §14, Appendix C4).
///
/// `required` and `default` are inert on this surface by construction: a
/// cross-cutting attr is optional on every record (requiring one would make
/// EVERY directive in the document illegal without it), and an unauthored
/// stamp attr is NOT default-injected — absent means absent, so a document
/// that authors none produces a byte-identical artifact.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StampAttrsFile {
    #[serde(rename = "stampAttrs")]
    pub stamp_attrs: Vec<AttrDecl>,
}

/// plugin `lints/*.yaml` export file (lint-system design §6): a top-level
/// `lints:` sequence of [`crate::lint::LintRuleDecl`]. Mirrors
/// [`StampAttrsFile`]'s shape — the loader reads each file with the same
/// `read_kind` machinery, and a per-package duplicate id is caught by the
/// shared `merge_named` path (key = raw rule id).
///
/// Excluded from [`crate::snapshot::CapabilitySnapshot`] and
/// `capabilitySnapshot` by construction: neither ever references this
/// file (or [`crate::loader::LoadedPlugin::lints`]), so a plugin can add,
/// remove, or change lints without perturbing artifact identity.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LintsFile {
    pub lints: Vec<crate::lint::LintRuleDecl>,
}

