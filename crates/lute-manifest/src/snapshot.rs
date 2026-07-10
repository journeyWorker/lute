use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

use crate::schema::*;
use crate::types::Literal;

/// The built-in quest lifecycle events (dsl 0.2.0 §6.6): quest-scoped, fired
/// by the engine, usable as `<on event=…>`. NOT capability-provided (§4.5) —
/// the single source of truth shared by `assemble`'s reserved-name check and
/// the checker's `E-UNKNOWN-EVENT`.
pub const BUILTIN_LIFECYCLE_EVENTS: &[&str] = &["questActive", "questComplete", "questFailed"];

#[derive(Clone, Debug, Default)]
pub struct CapabilitySnapshot {
    pub version: String,                           // capabilityVersion
    pub plugins: BTreeMap<String, ResolvedPlugin>, // id -> {version, options}
    pub enums: BTreeMap<String, Vec<String>>,
    /// Enum-style named vocabularies folded from each active plugin's `enums`
    /// export (plugin foundation A2), referenced by `Type::Domain(name)`
    /// (foundation A1) and resolved at check-stage. Keyed the same as
    /// `enums`; kept as a distinct map (rather than reusing `enums` directly)
    /// so a later task can grow registry-backed domains without perturbing
    /// the existing `enums`/`EnumFromOption` surface. A cross-plugin name
    /// collision is dropped (first owner wins) and reported as
    /// `E-DOMAIN-DUP` by `assemble` — see `assemble.rs`'s `merge_map`.
    pub domains: BTreeMap<String, Domain>,
    pub directives: BTreeMap<String, DirectiveDecl>, // by ::name
    pub providers: BTreeMap<String, ProviderDecl>,
    pub state_shapes: BTreeMap<String, StateShape>,
    pub state_templates: BTreeMap<String, StateTemplate>,
    pub asset_kinds: BTreeMap<String, AssetKindDecl>,
    pub bridge_capabilities: BTreeMap<(String, String), BridgeCapability>,
    pub defs: BTreeMap<String, DefDecl>,
    pub frontmatter: BTreeMap<String, crate::types::Type>,
    /// Installed-but-inactive tag → owning plugin id (plugin §11.2 fix-it). Not
    /// part of the resolved capability surface, so NOT folded into the version.
    pub inactive: BTreeMap<String, String>,
    pub events: BTreeMap<String, EventDecl>,
}

/// An enum-style named vocabulary: an ordered member list, same shape as an
/// `enums` entry. See `CapabilitySnapshot::domains`.
///
/// `open` distinguishes the two data-catalog foundation A3 declaration shapes
/// (0.3.0 draft §3.1 `entities:`): `false` (the default — every plugin/core
/// `enums` fold and a project `enums:`/`entities: { members: […] }` entry) is
/// a CLOSED, statically-enumerable domain — `members` is authoritative and
/// membership is checked against it. `true` (a project `entities: { open:
/// engine }` entry, `lute-manifest::entities::parse_entities`) is an
/// OPEN/registry-style domain: ids are minted by the engine at runtime, not
/// enumerable at compile time, so `members` stays empty and a later checker
/// task (A4) must treat membership as always-accept (or provider-backed)
/// rather than closed-list membership.
#[derive(Clone, Debug)]
pub struct Domain {
    pub members: Vec<String>,
    pub open: bool,
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

    pub fn event(&self, name: &str) -> Option<&EventDecl> {
        self.events.get(name)
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
                options: opts
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.clone()))
                    .collect(),
            },
        );
        self
    }

    pub fn build(self) -> CapabilitySnapshot {
        let mut snap = CapabilitySnapshot {
            plugins: self.plugins,
            ..Default::default()
        };
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
/// BTreeMap iteration is sorted -> order-independent by construction. NOTE: schema
/// values are folded via their `Debug` representation, which is stable *within* a
/// build but not guaranteed byte-identical across compiler versions; treat the
/// version as a per-build content stamp, not a cross-toolchain-portable identifier.
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
    h.update(b"\nstateTemplates\n");
    for (name, t) in &snap.state_templates {
        h.update(name.as_bytes());
        h.update(b"=");
        h.update(format!("{t:?}").as_bytes());
        h.update(b";");
    }
    h.update(b"\nassetKinds\n");
    for (name, k) in &snap.asset_kinds {
        h.update(name.as_bytes());
        h.update(b"=");
        h.update(format!("{k:?}").as_bytes());
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
    // GUARDED: only fold `events` when populated, so an event-LESS snapshot
    // hashes byte-identically to a pre-events snapshot (no lute-compile golden
    // churn for packages that don't use the dsl 0.2.0 §4.5 events export).
    if !snap.events.is_empty() {
        h.update(b"\nevents\n");
        for (name, e) in &snap.events {
            h.update(name.as_bytes());
            h.update(b"=");
            h.update(format!("{e:?}").as_bytes());
            h.update(b";");
        }
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
        let a = CapabilitySnapshot::builder()
            .plugin("lute.core", "0.0.1", &[])
            .build();
        let b = CapabilitySnapshot::builder()
            .plugin("lute.core", "0.0.2", &[])
            .build();
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
        let base = CapabilitySnapshot::builder()
            .plugin("lute.core", "0.0.1", &[])
            .build();
        let mut a = base.clone();
        a.enums
            .insert("mood".into(), vec!["peaceful".into(), "tense".into()]);
        a.version = capability_version(&a);
        let mut b = base.clone();
        b.enums
            .insert("mood".into(), vec!["peaceful".into(), "upbeat".into()]);
        b.version = capability_version(&b);
        assert_eq!(a.plugins.len(), b.plugins.len());
        assert_ne!(a.version, b.version);
    }

    #[test]
    fn version_changes_when_a_directive_changes() {
        // A single differing directive must change the version (§13 drift).
        let mk = |dir_name: &str| {
            let mut snap = CapabilitySnapshot::builder()
                .plugin("lute.core", "0.0.1", &[])
                .build();
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
                    lower: Lowering::Builtin {
                        kind: "builtin".into(),
                        name: "noop".into(),
                    },
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
        let snap = CapabilitySnapshot::builder()
            .plugin("lute.core", "0.0.1", &[])
            .build();
        // (directives are wired via load in Task 1.6; here just assert empty lookup is None)
        assert!(snap.directive("nope").is_none());
    }

    #[test]
    fn state_templates_change_the_version() {
        let mut a = CapabilitySnapshot::default();
        a.version = capability_version(&a);
        let mut b = CapabilitySnapshot::default();
        b.state_templates.insert(
            "slot".into(),
            crate::schema::StateTemplate {
                name: "slot".into(),
                scope: "scene".into(),
                path: vec![],
                shape: "s".into(),
            },
        );
        b.version = capability_version(&b);
        assert_ne!(
            a.version, b.version,
            "state_templates must affect capabilityVersion"
        );
    }

    #[test]
    fn asset_kinds_change_the_version() {
        let mut a = CapabilitySnapshot::default();
        a.version = capability_version(&a);
        let mut b = CapabilitySnapshot::default();
        b.asset_kinds.insert(
            "CH".into(),
            crate::schema::AssetKindDecl {
                kind: "CH".into(),
                sep: ".".into(),
                resolve: crate::schema::AssetResolve::Compose,
                segments: vec![],
                provider: None,
                match_: vec![],
                aliases: std::collections::BTreeMap::new(),
                fallback: vec![],
                persistence: None,
            },
        );
        b.version = capability_version(&b);
        assert_ne!(
            a.version, b.version,
            "asset_kinds must affect capabilityVersion"
        );
    }

    #[test]
    fn inactive_does_not_change_the_version() {
        let mut a = CapabilitySnapshot::default();
        let va = capability_version(&a);
        a.inactive
            .insert("minigame".into(), "idola.minigame".into());
        assert_eq!(
            va,
            capability_version(&a),
            "inactive index is metadata, not hashed"
        );
    }

    #[test]
    fn events_absent_keeps_capability_version_stable() {
        // An event-LESS snapshot must hash identically to a fresh default — the
        // guarded events section must NOT perturb existing (event-less) snapshots.
        let base = capability_version(&CapabilitySnapshot::default());
        assert_eq!(capability_version(&CapabilitySnapshot::default()), base);
    }

    #[test]
    fn declaring_an_event_changes_capability_version() {
        let a = CapabilitySnapshot::default();
        let mut b = CapabilitySnapshot::default();
        b.events.insert(
            "combatEnd".into(),
            EventDecl {
                name: "combatEnd".into(),
            },
        );
        assert_ne!(capability_version(&a), capability_version(&b));
    }

    #[test]
    fn event_accessor_finds_declared_event() {
        let mut s = CapabilitySnapshot::default();
        s.events.insert(
            "combatEnd".into(),
            EventDecl {
                name: "combatEnd".into(),
            },
        );
        assert!(s.event("combatEnd").is_some());
        assert!(s.event("nope").is_none());
    }
}
