//! Host interpolation tokens (0.50.0 §4.1): the bare `{{name}}` markers the
//! engine substitutes at render time. A plugin declares them in its
//! `hostTokens` export; `lute.core` declares `userName`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::SchemaError;
use crate::types::{type_str, Type};

/// A `hosttokens/*.yaml` file: a `hostTokens:` mapping keyed by the token
/// name.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostTokensFile {
    #[serde(rename = "hostTokens")]
    pub host_tokens: BTreeMap<String, HostTokenBody>,
}

/// The value half of a `hostTokens:` entry (the name is the map key).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostTokenBody {
    #[serde(rename = "type")]
    pub ty: HostTokenType,
}

/// What a host token renders as, which decides the format hints it takes:
/// text hints on a `string` or `enum` token, number hints on an `int` one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Type", into = "Type")]
pub enum HostTokenType {
    Str,
    Int,
    /// `{ enum: [members…] }`, in declaration order.
    Enum(Vec<String>),
}

impl HostTokenType {
    /// The manifest [`Type`] a referent of this token has, for the checks
    /// that type an interpolation.
    pub fn as_type(&self) -> Type {
        match self {
            HostTokenType::Str => Type::Str,
            HostTokenType::Int => Type::Int,
            HostTokenType::Enum(members) => Type::Enum(members.clone()),
        }
    }
}

impl std::fmt::Display for HostTokenType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&type_str(&self.as_type()))
    }
}

impl TryFrom<Type> for HostTokenType {
    type Error = SchemaError;
    fn try_from(ty: Type) -> Result<Self, SchemaError> {
        match ty {
            Type::Str => Ok(HostTokenType::Str),
            Type::Int => Ok(HostTokenType::Int),
            Type::Enum(members) => Ok(HostTokenType::Enum(members)),
            other => Err(SchemaError::HostToken(format!(
                "a host token's `type` is `string`, `int` or `{{ enum: [...] }}`, not `{}`",
                type_str(&other)
            ))),
        }
    }
}

impl From<HostTokenType> for Type {
    fn from(ty: HostTokenType) -> Type {
        ty.as_type()
    }
}

/// One host token of the resolved capability surface: its type and the
/// plugin that declares it (`lute.core` for `userName`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostTokenDecl {
    pub ty: HostTokenType,
    pub plugin: String,
}

/// The plugin that owns the core host token, `userName`.
pub const CORE_PLUGIN: &str = "lute.core";

impl HostTokenDecl {
    /// `true` for a token `lute.core` declares: it is part of every
    /// snapshot, so it is not folded into `capabilitySnapshot` and not
    /// listed as a project's capability.
    pub fn is_core(&self) -> bool {
        self.plugin == CORE_PLUGIN
    }
}
