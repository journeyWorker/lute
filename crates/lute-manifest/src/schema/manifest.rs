use crate::types::{Literal, Type};
use serde::{Deserialize, Serialize};

/// plugin §5 manifest entry.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginManifest {
    pub id: String,
    pub version: String,
    pub kind: String,
    #[serde(default)]
    pub depends: Vec<Depends>,
    pub exports: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub options: Vec<OptionDecl>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Depends {
    pub id: String,
    pub range: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OptionDecl {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: Type,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Literal>,
}

/// plugin §6.9 asset-kind declaration (export file `assetKinds/*.yaml`).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssetKindDecl {
    pub kind: String,
    #[serde(default = "default_sep")]
    pub sep: String,
    #[serde(default)]
    pub resolve: AssetResolve,
    #[serde(default)]
    pub segments: Vec<AssetSegment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, rename = "match")]
    pub match_: Vec<AssetMatch>,
    #[serde(default)]
    pub aliases: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub fallback: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persistence: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum AssetResolve {
    #[default]
    Compose,
    Query,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssetSegment {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#const: Option<String>,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub ty: Option<Type>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssetMatch {
    pub attr: String,
    pub field: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub via: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetKindsFile {
    #[serde(rename = "assetKinds")]
    pub asset_kinds: Vec<AssetKindDecl>,
}

fn default_sep() -> String {
    ".".to_string()
}

