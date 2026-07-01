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

/// plugin §13: deterministic content hash over the whole resolved capability
/// surface — plugin ids+versions+option objects, directives, enums, providers,
/// state shapes, bridge capabilities, defs, and frontmatter. Every generated
/// artifact is stamped with this so a consumer can refuse a mismatched snapshot;
/// any drift in a populated field yields a different version. `snap.version`
/// itself is excluded (it is the output). Each field is written under a distinct
/// section marker so a value in one field can never alias a value in another, and
/// BTreeMap iteration is sorted -> order-independent by construction.
pub fn capability_version(snap: &CapabilitySnapshot) -> String {
    let mut h = Sha256::new();
    h.update(b"plugins\n");
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
    h.update(b"\ndirectives\n");
    for (name, d) in &snap.directives {
        h.update(name.as_bytes());
        h.update(b"=");
        h.update(format!("{d:?}").as_bytes());
        h.update(b";");
    }
    h.update(b"\nenums\n");
    for (name, variants) in &snap.enums {
        h.update(name.as_bytes());
        h.update(b"=");
        h.update(format!("{variants:?}").as_bytes());
        h.update(b";");
    }
    h.update(b"\nproviders\n");
    for (name, p) in &snap.providers {
        h.update(name.as_bytes());
        h.update(b"=");
        h.update(format!("{p:?}").as_bytes());
        h.update(b";");
    }
    h.update(b"\nstateShapes\n");
    for (name, s) in &snap.state_shapes {
        h.update(name.as_bytes());
        h.update(b"=");
        h.update(format!("{s:?}").as_bytes());
        h.update(b";");
    }
    h.update(b"\nbridgeCapabilities\n");
    for ((service, operation), c) in &snap.bridge_capabilities {
        h.update(service.as_bytes());
        h.update(b".");
        h.update(operation.as_bytes());
        h.update(b"=");
        h.update(format!("{c:?}").as_bytes());
        h.update(b";");
    }
    h.update(b"\ndefs\n");
    for (name, d) in &snap.defs {
        h.update(name.as_bytes());
        h.update(b"=");
        h.update(format!("{d:?}").as_bytes());
        h.update(b";");
    }
    h.update(b"\nfrontmatter\n");
    for (key, ty) in &snap.frontmatter {
        h.update(key.as_bytes());
        h.update(b"=");
        h.update(format!("{ty:?}").as_bytes());
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
    fn version_changes_when_a_non_plugin_field_changes() {
        // Identical plugins; differ in exactly one enum entry. Under the old
        // plugins-only hash both would collide (a §13 drift bug); the extended
        // digest must distinguish them.
        let base = CapabilitySnapshot::builder().plugin("lute.core", "0.0.1", &[]).build();
        let mut a = base.clone();
        a.enums.insert("mood".into(), vec!["peaceful".into(), "tense".into()]);
        a.version = capability_version(&a);
        let mut b = base.clone();
        b.enums.insert("mood".into(), vec!["peaceful".into(), "upbeat".into()]);
        b.version = capability_version(&b);
        assert_eq!(a.plugins.len(), b.plugins.len());
        assert_ne!(a.version, b.version);
    }

    #[test]
    fn version_changes_when_a_directive_changes() {
        // A single differing directive must change the version (§13 drift).
        let mk = |dir_name: &str| {
            let mut snap = CapabilitySnapshot::builder().plugin("lute.core", "0.0.1", &[]).build();
            snap.directives.insert(
                dir_name.into(),
                DirectiveDecl {
                    name: dir_name.into(),
                    layer: None,
                    attrs: vec![],
                    semantics: vec![],
                    state: None,
                    effects: None,
                    bridge: None,
                    lower: Lowering::Builtin { kind: "builtin".into(), name: "noop".into() },
                },
            );
            snap.version = capability_version(&snap);
            snap
        };
        let a = mk("alpha");
        let b = mk("beta");
        assert_ne!(a.version, b.version);
    }

    #[test]
    fn directive_lookup_finds_registered() {
        let snap = CapabilitySnapshot::builder().plugin("lute.core", "0.0.1", &[]).build();
        // (directives are wired via load in Task 1.6; here just assert empty lookup is None)
        assert!(snap.directive("nope").is_none());
    }
}
