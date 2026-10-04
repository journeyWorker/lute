use super::{IdentityMetadata, IdentitySource};

#[test]
fn authored_identity_is_stable_and_not_positional() {
    let identity = IdentityMetadata::authored("componentInstance", "hearthFire#opening");
    assert_eq!(identity.source, IdentitySource::Authored);
    assert!(identity.stable);
    assert_eq!(identity.computed, "hearthFire#opening");
}

#[test]
fn fallback_identity_is_explicitly_unstable() {
    let identity = IdentityMetadata::fallback("line", "hearthFire#1.narrator_0010");
    assert_eq!(identity.source, IdentitySource::Fallback);
    assert!(!identity.stable);
}
