use std::fmt;

/// Structured error from a manifest schema conversion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SchemaError {
    Enum(String),
    Fact(String),
    Write(String),
    Lowering(String),
    RewardTarget(String),
    OccasionTarget(String),
}
impl fmt::Display for SchemaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Enum(message)
            | Self::Fact(message)
            | Self::Write(message)
            | Self::Lowering(message)
            | Self::RewardTarget(message)
            | Self::OccasionTarget(message) => message,
        };
        f.write_str(message)
    }
}

impl std::error::Error for SchemaError {}

mod directives;
mod files;
mod manifest;
mod occasions;
mod vocabulary;
#[cfg(test)]
mod tests;

pub use directives::{
    AttrDecl, BridgeRef, BUILTIN_LOWERING_HOOKS, DirectiveDecl, DirectiveEffects, DirectiveState,
    FactEffect, FactEffectArg, Lowering, OpBy, SlotDecl, WriteDecl, WriteValue, WRITE_OPS,
};
pub use files::{
    BridgeFile, DefsFile, DirectivesFile, EnumDecl, EnumsFile, EventsFile, FrontmatterDecl,
    FrontmatterFile, LintsFile, ProvidersFile, StampAttrsFile, StateFile,
};
pub use manifest::{
    AssetKindDecl, AssetKindsFile, AssetMatch, AssetResolve, AssetSegment, Depends, OptionDecl,
    PluginManifest,
};
pub use occasions::{
    OccasionBody, OccasionDecl, OccasionJudge, OccasionSelect, OccasionTarget, OccasionsFile,
};
pub use vocabulary::{
    BridgeCapability, CastBody, CastFile, CastMember, DefDecl, DefParam, EventDecl, ProviderDecl,
    RewardKindBody, RewardKindDecl, RewardKindsFile, RewardTarget, StateShape, StateTemplate,
};





