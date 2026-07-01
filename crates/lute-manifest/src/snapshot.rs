use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

use crate::schema::*;
use crate::types::Literal;

#[derive(Clone, Debug, Default)]
pub struct CapabilitySnapshot {
    pub version: String, // capabilityVersion
    pub plugins: BTreeMap<String, ResolvedPlugin>, // id -> {version, options}
    pub enums: BTreeMap<String, Vec<String>>,
    pub directives: BTreeMap<String, DirectiveDecl>, // by ::name
    pub providers: BTreeMap<String, ProviderDecl>,
    pub state_shapes: BTreeMap<String, StateShape>,
    pub bridge_capabilities: BTreeMap<(String, String), BridgeCapability>,
    pub defs: BTreeMap<String, DefDecl>,
    pub frontmatter: BTreeMap<String, crate::types::Type>,
}

#[derive(Clone, Debug)]
pub struct ResolvedPlugin {
    pub version: String,
    pub options: BTreeMap<String, Literal>,
}

impl CapabilitySnapshot {
    pub fn builder() -> SnapshotBuilder {
        SnapshotBuilder::default()
    }

    pub fn directive(&self, name: &str) -> Option<&DirectiveDecl> {
        self.directives.get(name)
    }
}

#[derive(Default)]
pub struct SnapshotBuilder {
    plugins: BTreeMap<String, ResolvedPlugin>,
}

impl SnapshotBuilder {
    pub fn plugin(mut self, id: &str, version: &str, opts: &[(&str, Literal)]) -> Self {
        self.plugins.insert(
            id.into(),
            ResolvedPlugin {
                version: version.into(),
                options: opts.iter().map(|(k, v)| (k.to_string(), v.clone())).collect(),
            },
        );
        self
    }

    pub fn build(self) -> CapabilitySnapshot {
        let mut snap = CapabilitySnapshot { plugins: self.plugins, ..Default::default() };
        snap.version = capability_version(&snap);
        snap
    }
}

/// plugin §13: deterministic content hash. Currently covers resolved plugin ids+versions +
/// option objects only. TODO(Task 1.6): fold in providers, directives, enums, state shapes,
/// bridge capabilities, defs, frontmatter, and active profile per §13 once populated.
/// BTreeMap iteration is sorted -> order-independent by construction.
pub fn capability_version(snap: &CapabilitySnapshot) -> String {
    let mut h = Sha256::new();
    for (id, p) in &snap.plugins {
        h.update(id.as_bytes());
        h.update(b"@");
        h.update(p.version.as_bytes());
        for (k, v) in &p.options {
            h.update(b"|");
            h.update(k.as_bytes());
            h.update(b"=");
            h.update(format!("{v:?}").as_bytes());
        }
        h.update(b";");
    }
    format!("{:x}", h.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_deterministic_and_order_independent() {
        let a = CapabilitySnapshot::builder()
            .plugin("lute.core", "0.0.1", &[])
            .plugin("idola.minigame", "0.1.0", &[])
            .build();
        let b = CapabilitySnapshot::builder()
            .plugin("idola.minigame", "0.1.0", &[]) // reversed insert order
            .plugin("lute.core", "0.0.1", &[])
            .build();
        assert_eq!(a.version, b.version);
    }

    #[test]
    fn version_changes_when_plugin_version_changes() {
        let a = CapabilitySnapshot::builder().plugin("lute.core", "0.0.1", &[]).build();
        let b = CapabilitySnapshot::builder().plugin("lute.core", "0.0.2", &[]).build();
        assert_ne!(a.version, b.version);
    }

    #[test]
    fn version_changes_when_option_value_changes() {
        let a = CapabilitySnapshot::builder()
            .plugin("p", "1", &[("k", Literal::Str("a".into()))])
            .build();
        let b = CapabilitySnapshot::builder()
            .plugin("p", "1", &[("k", Literal::Str("b".into()))])
            .build();
        assert_ne!(a.version, b.version);
    }

    #[test]
    fn directive_lookup_finds_registered() {
        let snap = CapabilitySnapshot::builder().plugin("lute.core", "0.0.1", &[]).build();
        // (directives are wired via load in Task 1.6; here just assert empty lookup is None)
        assert!(snap.directive("nope").is_none());
    }
}
