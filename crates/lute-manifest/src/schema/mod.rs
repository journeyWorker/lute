use std::fmt;

/// Structured error from a manifest schema conversion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SchemaError {
    Enum(String),
    Fact(String),
    Write(String),
    Lowering(String),
    RewardTarget(String),
    RewardEffects(String),
    OccasionTarget(String),
    HostToken(String),
}
impl fmt::Display for SchemaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Enum(message)
            | Self::Fact(message)
            | Self::Write(message)
            | Self::Lowering(message)
            | Self::RewardTarget(message)
            | Self::RewardEffects(message)
            | Self::OccasionTarget(message)
            | Self::HostToken(message) => message,
        };
        f.write_str(message)
    }
}

impl std::error::Error for SchemaError {}

mod directives;
mod files;
mod host_tokens;
mod manifest;
mod occasions;
#[cfg(test)]
mod tests;
mod vocabulary;

pub use directives::{
    AttrDecl, BridgeRef, DirectiveDecl, DirectiveEffects, DirectiveState, FactEffect,
    FactEffectArg, Lowering, OpBy, SlotDecl, WriteDecl, WriteValue, BUILTIN_LOWERING_HOOKS,
    WRITE_OPS,
};
pub use files::{
    BridgeFile, DefsFile, DirectivesFile, EnumDecl, EnumsFile, EventsFile, FrontmatterDecl,
    FrontmatterFile, LintsFile, ProvidersFile, StampAttrEntry, StampAttrsFile, StateFile,
};
pub use host_tokens::{HostTokenBody, HostTokenDecl, HostTokenType, HostTokensFile, CORE_PLUGIN};
pub use manifest::{
    AssetKindDecl, AssetKindsFile, AssetMatch, AssetResolve, AssetSegment, Depends, OptionDecl,
    PluginManifest,
};
pub use occasions::{
    OccasionBody, OccasionDecl, OccasionJudge, OccasionSelect, OccasionTarget, OccasionsFile,
};
pub use vocabulary::{
    AcceptEffect, BridgeCapability, CastBody, CastFile, CastMember, DefDecl, DefParam, EventDecl,
    ProviderDecl, RewardEffects, RewardKindBody, RewardKindDecl, RewardKindsFile, RewardTarget,
    StateShape, StateTemplate, REWARD_TARGET_ATTR,
};
