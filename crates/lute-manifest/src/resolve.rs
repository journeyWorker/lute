use crate::types::{lit_str, type_accepts, type_str, Literal, Type};
use std::collections::BTreeMap;

/// One installed plugin's fully loaded package (plugin §4). Carries the parsed
/// `LoadedPlugin` (manifest + directives/shapes/providers/…) so the assembler
/// (Phase 5) can read the whole package; resolution reads the manifest's
/// id/version/depends through `manifest()`.
#[derive(Clone, Debug)]
pub struct InstalledPlugin {
    pub loaded: crate::loader::LoadedPlugin,
}

impl InstalledPlugin {
    pub fn manifest(&self) -> &crate::schema::PluginManifest {
        &self.loaded.manifest
    }
}

/// Every plugin discovered on disk, indexed by id (plugin §4). The resolver
/// walks this for the dependency closure (§11.1 step 6) and the inactive-plugin
/// fix-it (§11.2); the assembler merges the *active* subset into the snapshot.
#[derive(Clone, Debug, Default)]
pub struct InstalledPlugins {
    pub by_id: std::collections::BTreeMap<String, InstalledPlugin>,
}

impl InstalledPlugins {
    pub fn get(&self, id: &str) -> Option<&InstalledPlugin> {
        self.by_id.get(id)
    }
}

pub type ActivationMap = BTreeMap<String, BTreeMap<String, Literal>>;

#[derive(Clone, Debug)]
pub struct Profile {
    pub extends: Option<String>,
    pub plugins: ActivationMap,
}

#[derive(Clone, Debug)]
pub struct ProfileGraph {
    pub profiles: BTreeMap<String, Profile>,
    pub default_profile: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActivePlugin {
    pub id: String,
    pub options: BTreeMap<String, Literal>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ResolveError {
    UnknownProfile(String),
    ExtendsCycle(String),
    /// A `depends` id (plugin §5) is not installed (plugin §11.1 step 6).
    UnresolvedDepends {
        plugin: String,
        dep: String,
    },
    /// A `depends` is installed but its version fails the declared range.
    DependsVersionMismatch {
        plugin: String,
        dep: String,
        need: String,
        found: String,
    },
    /// The `depends` graph has a cycle.
    DependsCycle(String),
}

impl ResolveError {
    /// Stable, machine-readable code per variant (plugin §11); mirrors the
    /// checker's `E-*` diagnostic-code family so consumers can key on it.
    pub fn code(&self) -> &'static str {
        match self {
            ResolveError::UnknownProfile(_) => "E-PROFILE-UNKNOWN",
            ResolveError::ExtendsCycle(_) => "E-PROFILE-EXTENDS-CYCLE",
            ResolveError::UnresolvedDepends { .. } => "E-DEPENDS-UNRESOLVED",
            ResolveError::DependsVersionMismatch { .. } => "E-DEPENDS-VERSION",
            ResolveError::DependsCycle(_) => "E-DEPENDS-CYCLE",
        }
    }
}

impl std::fmt::Display for ResolveError {
    /// Human-readable rendering, surfaced by `project.rs` as the resolver's
    /// `ResolveDiag` message — the same `{e:?}` → `{e}` fix
    /// [`crate::loader::LoadError`] got (0.10.1: the toolchain says what it
    /// knows). This channel is reached less often than a load or option
    /// error (it requires an unknown/cyclic profile or an unsatisfiable
    /// `depends`), but it is exactly as user-facing when it does fire.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolveError::UnknownProfile(name) => write!(
                f,
                "profile `{name}` is not declared in this project's `lute.project.yaml`"
            ),
            ResolveError::ExtendsCycle(name) => write!(
                f,
                "profile `{name}`'s `extends` chain cycles back to itself"
            ),
            ResolveError::UnresolvedDepends { plugin, dep } => write!(
                f,
                "plugin `{plugin}` depends on `{dep}`, which is not installed"
            ),
            ResolveError::DependsVersionMismatch {
                plugin,
                dep,
                need,
                found,
            } => write!(
                f,
                "plugin `{plugin}` depends on `{dep}` {need}, but the installed `{dep}` is `{found}`"
            ),
            ResolveError::DependsCycle(dep) => {
                write!(f, "plugin `{dep}` is part of a `depends` cycle")
            }
        }
    }
}

impl ProfileGraph {
    fn extends_chain(&self, selected: &str) -> Result<Vec<String>, ResolveError> {
        // returns parent-first chain EXCLUDING global, INCLUDING selected last
        let mut chain = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        let mut cur = Some(selected.to_string());
        while let Some(name) = cur {
            if !self.profiles.contains_key(&name) {
                return Err(ResolveError::UnknownProfile(name));
            }
            if !seen.insert(name.clone()) {
                return Err(ResolveError::ExtendsCycle(name));
            }
            chain.push(name.clone());
            cur = self.profiles[&name].extends.clone();
        }
        chain.reverse(); // parent-first
        Ok(chain)
    }
}

/// plugin §11.1 resolution order + §11.2 merge: last-layer-wins for scalars/lists; map option values deep-merge across layers.
pub fn resolve_activation(
    graph: &ProfileGraph,
    selected: &str,
    scene_local: &ActivationMap,
    installed: &InstalledPlugins,
) -> Result<Vec<ActivePlugin>, ResolveError> {
    // ordered id list + merged options
    let mut order: Vec<String> = Vec::new();
    let mut merged: BTreeMap<String, BTreeMap<String, Literal>> = BTreeMap::new();

    let apply = |acts: &ActivationMap,
                 order: &mut Vec<String>,
                 merged: &mut BTreeMap<String, BTreeMap<String, Literal>>| {
        for (id, opts) in acts {
            if !merged.contains_key(id) {
                order.push(id.clone());
            }
            let entry = merged.entry(id.clone()).or_default();
            for (k, v) in opts {
                match (entry.get_mut(k), v) {
                    // map deep-merge (plugin §11.2)
                    (Some(Literal::Map(dst)), Literal::Map(src)) => merge_map(dst, src),
                    // scalar/list replace, or type change
                    _ => {
                        entry.insert(k.clone(), v.clone());
                    }
                }
            }
        }
    };

    // 1. lute.core is always first (language-required)
    if !merged.contains_key("lute.core") {
        order.push("lute.core".into());
        merged.insert("lute.core".into(), BTreeMap::new());
    }
    // 2. profiles.global
    if let Some(g) = graph.profiles.get("global") {
        apply(&g.plugins, &mut order, &mut merged);
    }
    // 3+4. extends chain (parent-first) then selected
    for name in graph.extends_chain(selected)? {
        if name == "global" {
            continue;
        }
        apply(&graph.profiles[&name].plugins, &mut order, &mut merged);
    }
    // 5. scene-local
    apply(scene_local, &mut order, &mut merged);

    // 6. Dependency closure (plugin §11.1 step 6): transitively activate every
    //    `depends` of an active plugin, in deterministic (sorted-id) order.
    //    depends-added plugins take default (empty) options.
    let mut queue: Vec<String> = order.clone();
    while let Some(id) = queue.pop() {
        let Some(inst) = installed.get(&id) else {
            // lute.core is always synthetic-present even if not installed on disk;
            // any other missing active id is the caller's concern (it was named by
            // a profile, not a depends) — skip closure for it.
            continue;
        };
        let mut deps = inst.manifest().depends.clone();
        deps.sort_by(|a, b| a.id.cmp(&b.id));
        for dep in deps {
            match installed.get(&dep.id) {
                None if dep.id == "lute.core" => { /* synthetic core, always ok */ }
                None => {
                    return Err(ResolveError::UnresolvedDepends {
                        plugin: id.clone(),
                        dep: dep.id.clone(),
                    })
                }
                Some(dep_inst) => {
                    if !range_satisfies(&dep.range, &dep_inst.manifest().version) {
                        return Err(ResolveError::DependsVersionMismatch {
                            plugin: id.clone(),
                            dep: dep.id.clone(),
                            need: dep.range.clone(),
                            found: dep_inst.manifest().version.clone(),
                        });
                    }
                }
            }
            if !merged.contains_key(&dep.id) {
                order.push(dep.id.clone());
                merged.insert(dep.id.clone(), BTreeMap::new());
                queue.push(dep.id.clone());
            }
        }
    }
    detect_depends_cycle(&order, installed)?;

    Ok(order
        .into_iter()
        .map(|id| ActivePlugin {
            options: merged.remove(&id).unwrap_or_default(),
            id,
        })
        .collect())
}

/// Recursive map deep-merge (plugin §11.2): src entries override dst; nested maps
/// recurse; scalars/lists replace.
fn merge_map(dst: &mut BTreeMap<String, Literal>, src: &BTreeMap<String, Literal>) {
    for (k, v) in src {
        match (dst.get_mut(k), v) {
            (Some(Literal::Map(d)), Literal::Map(s)) => merge_map(d, s),
            _ => {
                dst.insert(k.clone(), v.clone());
            }
        }
    }
}

/// An activation-time plugin-OPTION violation (plugin Appendix C1: "Activation
/// MUST reject an option value that is not valid for its declared type, and
/// MUST reject an unknown option name"). Separate from [`ResolveError`] because
/// it is REPORTED, never fatal: an unknown/mistyped option does not invalidate
/// the activation ORDER, so the snapshot still assembles and the author still
/// sees the document's real directive diagnostics instead of a cascade of
/// "unknown directive" noise from a core-only fallback.
#[derive(Clone, Debug, PartialEq)]
pub enum OptionError {
    /// An activated option name the owning manifest never declares.
    UnknownOption {
        plugin: String,
        name: String,
        declared: Vec<String>,
    },
    /// A declared option whose merged value fails [`type_accepts`].
    OptionType {
        plugin: String,
        name: String,
        expected: Type,
        got: Literal,
    },
}

impl OptionError {
    /// Stable, machine-readable code per variant; mirrors the checker's `E-*`
    /// diagnostic-code family so consumers can key on it.
    pub fn code(&self) -> &'static str {
        match self {
            OptionError::UnknownOption { .. } => "E-PLUGIN-OPTION-UNKNOWN",
            OptionError::OptionType { .. } => "E-PLUGIN-OPTION-TYPE",
        }
    }

    /// Author-facing prose: the fix (a name typo, a wrong literal shape) is
    /// only actionable with the declared set / expected type spelled out.
    pub fn message(&self) -> String {
        match self {
            OptionError::UnknownOption {
                plugin,
                name,
                declared,
            } => {
                let list = if declared.is_empty() {
                    "none".to_string()
                } else {
                    declared.join(", ")
                };
                format!("plugin `{plugin}` has no option `{name}` (declared: {list})")
            }
            OptionError::OptionType {
                plugin,
                name,
                expected,
                got,
            } => format!(
                "option `{plugin}.{name}` expects {}, got {}",
                type_str(expected),
                lit_str(got)
            ),
        }
    }
}

/// plugin Appendix C1: validate every MERGED option value against the owning
/// `PluginManifest.options`. Runs on the output of [`resolve_activation`] —
/// i.e. exactly the post-§11.2-merge map, so a value only has to be valid in
/// its FINAL layered form, never in each intermediate layer.
///
/// Collects EVERY violation (no bail on the first) so one pass reports the
/// whole broken profile. Deterministic: `active` is in resolution order and
/// each plugin's options are a `BTreeMap`.
///
/// An active id with no installed package is skipped — there is no manifest to
/// validate against. That covers the synthetic `lute.core` baseline and any id
/// a profile named but never installed (assembly reports the latter as
/// `E-PLUGIN-MISSING-ACTIVE`).
pub fn validate_activation_options(
    active: &[ActivePlugin],
    installed: &InstalledPlugins,
) -> Vec<OptionError> {
    let mut errs = Vec::new();
    for ap in active {
        let Some(inst) = installed.get(&ap.id) else {
            continue;
        };
        let decls = &inst.manifest().options;
        for (name, value) in &ap.options {
            let Some(decl) = decls.iter().find(|o| &o.name == name) else {
                errs.push(OptionError::UnknownOption {
                    plugin: ap.id.clone(),
                    name: name.clone(),
                    declared: decls.iter().map(|o| o.name.clone()).collect(),
                });
                continue;
            };
            if !type_accepts(&decl.ty, value) {
                errs.push(OptionError::OptionType {
                    plugin: ap.id.clone(),
                    name: name.clone(),
                    expected: decl.ty.clone(),
                    got: value.clone(),
                });
            }
        }
    }
    errs
}

/// Detect a cycle in the `depends` graph restricted to activated plugins
/// (plugin §15: a conforming resolution has no depends cycles). Iterative DFS
/// with visiting/done marks; deterministic (roots in `order`, deps sorted).
fn detect_depends_cycle(
    order: &[String],
    installed: &InstalledPlugins,
) -> Result<(), ResolveError> {
    #[derive(Clone, Copy, PartialEq)]
    enum Mark {
        Visiting,
        Done,
    }
    let deps_of = |id: &str| -> Vec<String> {
        match installed.get(id) {
            Some(inst) => {
                let mut d: Vec<String> = inst
                    .manifest()
                    .depends
                    .iter()
                    .map(|x| x.id.clone())
                    .collect();
                d.sort();
                d
            }
            None => Vec::new(),
        }
    };
    let mut state: BTreeMap<String, Mark> = BTreeMap::new();
    for root in order {
        if state.contains_key(root) {
            continue;
        }
        let mut stack: Vec<(String, Vec<String>, usize)> = vec![(root.clone(), deps_of(root), 0)];
        state.insert(root.clone(), Mark::Visiting);
        while let Some((id, deps, cursor)) = stack.last_mut() {
            if *cursor < deps.len() {
                let dep = deps[*cursor].clone();
                *cursor += 1;
                match state.get(&dep) {
                    Some(Mark::Visiting) => return Err(ResolveError::DependsCycle(dep)),
                    Some(Mark::Done) => {}
                    None => {
                        state.insert(dep.clone(), Mark::Visiting);
                        let d = deps_of(&dep);
                        stack.push((dep, d, 0));
                    }
                }
            } else {
                let done = id.clone();
                stack.pop();
                state.insert(done, Mark::Done);
            }
        }
    }
    Ok(())
}

/// Minimal semver-range check for plugin `depends` (plugin §5). Supports the
/// caret form used in 0.0.1 (`^MAJOR.MINOR.PATCH`) and a bare exact version.
/// Caret semantics: pre-1.0 the caret pins to the leftmost non-zero component —
/// `^0.0.z` requires exactly `0.0.z`; `^0.y.z` requires `0.y.*` with patch ≥ z;
/// `^x.y.z` (x≥1) requires `x.*` with (minor,patch) ≥ (y,z). An unparseable
/// range or version is treated as NOT satisfied (conservative) (a version/range
/// MUST have exactly three numeric components).
fn range_satisfies(range: &str, version: &str) -> bool {
    fn parse(v: &str) -> Option<(u64, u64, u64)> {
        let parts: Vec<&str> = v.trim().split('.').collect();
        if parts.len() != 3 {
            return None;
        }
        Some((
            parts[0].parse().ok()?,
            parts[1].parse().ok()?,
            parts[2].parse().ok()?,
        ))
    }
    let Some((vmaj, vmin, vpat)) = parse(version) else {
        return false;
    };
    if let Some(caret) = range.strip_prefix('^') {
        let Some((rmaj, rmin, rpat)) = parse(caret) else {
            return false;
        };
        if rmaj == 0 && rmin == 0 {
            return (vmaj, vmin, vpat) == (rmaj, rmin, rpat);
        }
        if rmaj == 0 {
            return vmaj == 0 && vmin == rmin && vpat >= rpat;
        }
        return vmaj == rmaj && (vmin, vpat) >= (rmin, rpat);
    }
    parse(range) == Some((vmaj, vmin, vpat))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn installed_plugins_lookup() {
        use crate::schema::{Depends, PluginManifest};
        use std::collections::BTreeMap;
        let m = PluginManifest {
            id: "idola.minigame".into(),
            version: "0.1.0".into(),
            kind: "capability".into(),
            depends: vec![Depends {
                id: "lute.core".into(),
                range: "^0.0.1".into(),
            }],
            exports: BTreeMap::new(),
            options: vec![],
        };
        let reg = InstalledPlugins {
            by_id: BTreeMap::from([(
                "idola.minigame".to_string(),
                InstalledPlugin { loaded: loaded(m) },
            )]),
        };
        assert_eq!(
            reg.get("idola.minigame").unwrap().manifest().version,
            "0.1.0"
        );
        assert!(reg.get("nope").is_none());
    }

    fn graph() -> ProfileGraph {
        // global -> story -> date -> date-minigame, per plugin §11 example
        let mut profiles = BTreeMap::new();
        profiles.insert(
            "global".into(),
            Profile {
                extends: None,
                plugins: map(&[("lute.core", opts(&[]))]),
            },
        );
        profiles.insert(
            "story".into(),
            Profile {
                extends: None,
                plugins: map(&[("idola.vn", opts(&[]))]),
            },
        );
        profiles.insert(
            "date".into(),
            Profile {
                extends: Some("story".into()),
                plugins: map(&[("idola.date", opts(&[]))]),
            },
        );
        profiles.insert(
            "date-minigame".into(),
            Profile {
                extends: Some("date".into()),
                plugins: map(&[(
                    "idola.minigame",
                    opts(&[("resultScope", Literal::Str("scene".into()))]),
                )]),
            },
        );
        ProfileGraph {
            profiles,
            default_profile: "story".into(),
        }
    }
    fn opts(kv: &[(&str, Literal)]) -> BTreeMap<String, Literal> {
        kv.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
    }
    fn map(
        kv: &[(&str, BTreeMap<String, Literal>)],
    ) -> BTreeMap<String, BTreeMap<String, Literal>> {
        kv.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
    }

    #[test]
    fn resolves_extends_chain_parent_first_with_core_and_global() {
        let g = graph();
        let active = resolve_activation(
            &g,
            "date-minigame",
            &BTreeMap::new(),
            &InstalledPlugins::default(),
        )
        .unwrap();
        let ids: Vec<_> = active.iter().map(|a| a.id.as_str()).collect();
        // §11.1 order: lute.core, global's plugins, extends chain parent-first, selected, scene-local
        assert_eq!(
            ids,
            vec!["lute.core", "idola.vn", "idola.date", "idola.minigame"]
        );
    }

    #[test]
    fn scalar_option_later_layer_overrides() {
        let g = graph();
        let scene_local = map(&[(
            "idola.minigame",
            opts(&[("resultScope", Literal::Str("run".into()))]),
        )]);
        let active = resolve_activation(
            &g,
            "date-minigame",
            &scene_local,
            &InstalledPlugins::default(),
        )
        .unwrap();
        let mg = active.iter().find(|a| a.id == "idola.minigame").unwrap();
        assert_eq!(
            mg.options.get("resultScope"),
            Some(&Literal::Str("run".into()))
        );
    }

    #[test]
    fn extends_cycle_is_error() {
        let mut g = graph();
        g.profiles.get_mut("story").unwrap().extends = Some("date".into()); // story<-date<-story
        assert!(matches!(
            resolve_activation(
                &g,
                "date",
                &std::collections::BTreeMap::new(),
                &InstalledPlugins::default()
            ),
            Err(ResolveError::ExtendsCycle(_))
        ));
    }

    #[test]
    fn unknown_selected_profile_is_error() {
        let g = graph();
        assert!(matches!(
            resolve_activation(
                &g,
                "nope",
                &std::collections::BTreeMap::new(),
                &InstalledPlugins::default()
            ),
            Err(ResolveError::UnknownProfile(_))
        ));
    }

    #[test]
    fn unknown_parent_profile_is_error() {
        let mut g = graph();
        g.profiles.get_mut("date").unwrap().extends = Some("missing".into());
        assert!(matches!(
            resolve_activation(
                &g,
                "date",
                &std::collections::BTreeMap::new(),
                &InstalledPlugins::default()
            ),
            Err(ResolveError::UnknownProfile(_))
        ));
    }

    #[test]
    fn map_option_values_deep_merge_across_layers() {
        use crate::types::Literal;
        use std::collections::BTreeMap;
        // parent sets cast.bianca={costume:a}; child adds cast.ren={costume:b}.
        let mut parent_opt = BTreeMap::new();
        let mut cast_p = BTreeMap::new();
        cast_p.insert("bianca".to_string(), Literal::Str("a".into()));
        parent_opt.insert("cast".to_string(), Literal::Map(cast_p));
        let mut child_opt = BTreeMap::new();
        let mut cast_c = BTreeMap::new();
        cast_c.insert("ren".to_string(), Literal::Str("b".into()));
        child_opt.insert("cast".to_string(), Literal::Map(cast_c));

        let mut parent = BTreeMap::new();
        parent.insert("p.plug".to_string(), parent_opt);
        let mut child = BTreeMap::new();
        child.insert("p.plug".to_string(), child_opt);

        let graph = ProfileGraph {
            profiles: BTreeMap::from([
                (
                    "parent".to_string(),
                    Profile {
                        extends: None,
                        plugins: parent,
                    },
                ),
                (
                    "child".to_string(),
                    Profile {
                        extends: Some("parent".into()),
                        plugins: child,
                    },
                ),
            ]),
            default_profile: "child".to_string(),
        };
        let active = resolve_activation(
            &graph,
            "child",
            &BTreeMap::new(),
            &InstalledPlugins::default(),
        )
        .unwrap();
        let plug = active.iter().find(|a| a.id == "p.plug").unwrap();
        match plug.options.get("cast").unwrap() {
            Literal::Map(m) => {
                assert!(m.contains_key("bianca"), "parent entry retained");
                assert!(m.contains_key("ren"), "child entry merged in");
            }
            other => panic!("expected merged Map, got {other:?}"),
        }
    }

    fn manifest(id: &str, version: &str, deps: &[(&str, &str)]) -> crate::schema::PluginManifest {
        crate::schema::PluginManifest {
            id: id.into(),
            version: version.into(),
            kind: "capability".into(),
            depends: deps
                .iter()
                .map(|(i, r)| crate::schema::Depends {
                    id: i.to_string(),
                    range: r.to_string(),
                })
                .collect(),
            exports: std::collections::BTreeMap::new(),
            options: vec![],
        }
    }

    fn loaded(m: crate::schema::PluginManifest) -> crate::loader::LoadedPlugin {
        crate::loader::LoadedPlugin {
            manifest: m,
            directives: vec![],
            enums: Default::default(),
            state_shapes: vec![],
            state_templates: vec![],
            providers: vec![],
            bridge: vec![],
            defs: vec![],
            frontmatter: Default::default(),
            asset_kinds: vec![],
            events: vec![],
            stamp_attrs: vec![],
            lints: vec![],
        }
    }

    fn installed(ms: Vec<crate::schema::PluginManifest>) -> InstalledPlugins {
        InstalledPlugins {
            by_id: ms
                .into_iter()
                .map(|m| (m.id.clone(), InstalledPlugin { loaded: loaded(m) }))
                .collect(),
        }
    }

    #[test]
    fn dependency_closure_pulls_transitive_deps() {
        use std::collections::BTreeMap;
        // story activates idola.vn; idola.vn depends idola.base; base depends lute.core.
        let graph = ProfileGraph {
            profiles: BTreeMap::from([(
                "story".to_string(),
                Profile {
                    extends: None,
                    plugins: BTreeMap::from([("idola.vn".to_string(), BTreeMap::new())]),
                },
            )]),
            default_profile: "story".to_string(),
        };
        let inst = installed(vec![
            manifest("lute.core", "0.0.1", &[]),
            manifest("idola.base", "0.1.0", &[("lute.core", "^0.0.1")]),
            manifest("idola.vn", "0.1.0", &[("idola.base", "^0.1.0")]),
        ]);
        let active = resolve_activation(&graph, "story", &BTreeMap::new(), &inst).unwrap();
        let ids: Vec<_> = active.iter().map(|a| a.id.as_str()).collect();
        assert!(
            ids.contains(&"idola.base"),
            "transitive dep must be activated: {ids:?}"
        );
        assert!(ids.contains(&"idola.vn"));
        assert!(ids.contains(&"lute.core"));
    }

    #[test]
    fn unresolved_depends_is_error() {
        use std::collections::BTreeMap;
        let graph = ProfileGraph {
            profiles: BTreeMap::from([(
                "s".to_string(),
                Profile {
                    extends: None,
                    plugins: BTreeMap::from([("a.x".to_string(), BTreeMap::new())]),
                },
            )]),
            default_profile: "s".to_string(),
        };
        let inst = installed(vec![manifest("a.x", "0.1.0", &[("a.missing", "^0.1.0")])]);
        assert!(matches!(
            resolve_activation(&graph, "s", &BTreeMap::new(), &inst),
            Err(ResolveError::UnresolvedDepends { .. })
        ));
    }

    /// Companion to `unresolved_depends_is_error`: pins the `Display`
    /// rendering (`project.rs`'s `resolve_activation` call site switched
    /// `{e:?}` → `{e}` alongside `LoadError`'s identical fix) so this variant
    /// never regresses back to a `Debug` struct dump.
    #[test]
    fn unresolved_depends_message_is_prose() {
        use std::collections::BTreeMap;
        let graph = ProfileGraph {
            profiles: BTreeMap::from([(
                "s".to_string(),
                Profile {
                    extends: None,
                    plugins: BTreeMap::from([("a.x".to_string(), BTreeMap::new())]),
                },
            )]),
            default_profile: "s".to_string(),
        };
        let inst = installed(vec![manifest("a.x", "0.1.0", &[("a.missing", "^0.1.0")])]);
        let err = resolve_activation(&graph, "s", &BTreeMap::new(), &inst).unwrap_err();
        let msg = err.to_string();
        assert!(!msg.contains("UnresolvedDepends {"), "must not be a Debug dump: {msg}");
        assert_eq!(
            msg,
            "plugin `a.x` depends on `a.missing`, which is not installed"
        );
    }

    #[test]
    fn depends_version_mismatch_is_error() {
        use std::collections::BTreeMap;
        let graph = ProfileGraph {
            profiles: BTreeMap::from([(
                "s".to_string(),
                Profile {
                    extends: None,
                    plugins: BTreeMap::from([("a.x".to_string(), BTreeMap::new())]),
                },
            )]),
            default_profile: "s".to_string(),
        };
        let inst = installed(vec![
            manifest("a.x", "0.1.0", &[("a.dep", "^0.2.0")]),
            manifest("a.dep", "0.1.0", &[]),
        ]);
        assert!(matches!(
            resolve_activation(&graph, "s", &BTreeMap::new(), &inst),
            Err(ResolveError::DependsVersionMismatch { .. })
        ));
    }

    #[test]
    fn depends_cycle_is_error() {
        use std::collections::BTreeMap;
        let graph = ProfileGraph {
            profiles: BTreeMap::from([(
                "s".to_string(),
                Profile {
                    extends: None,
                    plugins: BTreeMap::from([("a.x".to_string(), BTreeMap::new())]),
                },
            )]),
            default_profile: "s".to_string(),
        };
        // a.x -> a.dep -> a.x (cycle)
        let inst = installed(vec![
            manifest("a.x", "0.1.0", &[("a.dep", "^0.1.0")]),
            manifest("a.dep", "0.1.0", &[("a.x", "^0.1.0")]),
        ]);
        assert!(matches!(
            resolve_activation(&graph, "s", &BTreeMap::new(), &inst),
            Err(ResolveError::DependsCycle(_))
        ));
    }

    #[test]
    fn malformed_range_or_version_is_not_satisfied() {
        use std::collections::BTreeMap;
        let graph = ProfileGraph {
            profiles: BTreeMap::from([(
                "s".to_string(),
                Profile {
                    extends: None,
                    plugins: BTreeMap::from([("a.x".to_string(), BTreeMap::new())]),
                },
            )]),
            default_profile: "s".to_string(),
        };
        // a.dep installed at 1.2.3; a.x depends with a malformed 4-component range.
        let inst = installed(vec![
            manifest("a.x", "0.1.0", &[("a.dep", "1.2.3.4")]),
            manifest("a.dep", "1.2.3", &[]),
        ]);
        assert!(matches!(
            resolve_activation(&graph, "s", &BTreeMap::new(), &inst),
            Err(ResolveError::DependsVersionMismatch { .. })
        ));
    }

    /// plugin Appendix C1 fixture: one plugin declaring two typed options.
    fn opt_manifest() -> crate::schema::PluginManifest {
        let mut m = manifest("idola.minigame", "0.1.0", &[]);
        m.options = vec![
            crate::schema::OptionDecl {
                name: "resultScope".into(),
                ty: Type::Enum(vec!["scene".into(), "run".into()]),
                default: None,
            },
            crate::schema::OptionDecl {
                name: "rounds".into(),
                ty: Type::Number,
                default: None,
            },
        ];
        m
    }

    fn one_plugin_graph(opts: BTreeMap<String, Literal>) -> ProfileGraph {
        ProfileGraph {
            profiles: BTreeMap::from([(
                "story".to_string(),
                Profile {
                    extends: None,
                    plugins: BTreeMap::from([("idola.minigame".to_string(), opts)]),
                },
            )]),
            default_profile: "story".to_string(),
        }
    }

    fn resolve_and_validate(opts: BTreeMap<String, Literal>) -> Vec<OptionError> {
        let inst = installed(vec![opt_manifest()]);
        let active =
            resolve_activation(&one_plugin_graph(opts), "story", &BTreeMap::new(), &inst).unwrap();
        validate_activation_options(&active, &inst)
    }

    #[test]
    fn unknown_option_name_is_rejected() {
        let errs = resolve_and_validate(opts(&[("resultScop", Literal::Str("scene".into()))]));
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert_eq!(errs[0].code(), "E-PLUGIN-OPTION-UNKNOWN");
        assert_eq!(
            errs[0].message(),
            "plugin `idola.minigame` has no option `resultScop` (declared: resultScope, rounds)"
        );
    }

    #[test]
    fn wrong_typed_option_value_is_rejected() {
        // `resultScope` is enum(scene|run); `rounds` is number.
        let errs = resolve_and_validate(opts(&[
            ("resultScope", Literal::Str("galaxy".into())),
            ("rounds", Literal::Str("three".into())),
        ]));
        let mut msgs: Vec<_> = errs.iter().map(|e| (e.code(), e.message())).collect();
        msgs.sort();
        assert_eq!(
            msgs,
            vec![
                (
                    "E-PLUGIN-OPTION-TYPE",
                    "option `idola.minigame.resultScope` expects enum(scene|run), got \"galaxy\""
                        .to_string()
                ),
                (
                    "E-PLUGIN-OPTION-TYPE",
                    "option `idola.minigame.rounds` expects number, got \"three\"".to_string()
                ),
            ]
        );
    }

    #[test]
    fn valid_options_still_resolve_clean() {
        let errs = resolve_and_validate(opts(&[
            ("resultScope", Literal::Str("run".into())),
            ("rounds", Literal::Num(3.0)),
        ]));
        assert!(errs.is_empty(), "{errs:?}");
    }

    #[test]
    fn options_validate_against_the_merged_value_not_each_layer() {
        // plugin §11.2: a parent layer's bad value that a child layer OVERRIDES
        // is not an error — only the final merged value is validated.
        let inst = installed(vec![opt_manifest()]);
        let graph = ProfileGraph {
            profiles: BTreeMap::from([
                (
                    "base".to_string(),
                    Profile {
                        extends: None,
                        plugins: BTreeMap::from([(
                            "idola.minigame".to_string(),
                            opts(&[("resultScope", Literal::Str("galaxy".into()))]),
                        )]),
                    },
                ),
                (
                    "story".to_string(),
                    Profile {
                        extends: Some("base".to_string()),
                        plugins: BTreeMap::from([(
                            "idola.minigame".to_string(),
                            opts(&[("resultScope", Literal::Str("scene".into()))]),
                        )]),
                    },
                ),
            ]),
            default_profile: "story".to_string(),
        };
        let active = resolve_activation(&graph, "story", &BTreeMap::new(), &inst).unwrap();
        assert!(
            validate_activation_options(&active, &inst).is_empty(),
            "the overridden layer must not be validated"
        );
    }

    #[test]
    fn uninstalled_active_plugin_options_are_skipped() {
        // `lute.core` is synthetic (never in `installed`); there is no manifest
        // to validate against, so its options must not fabricate an error.
        let inst = installed(vec![opt_manifest()]);
        let active = vec![ActivePlugin {
            id: "lute.core".into(),
            options: opts(&[("whatever", Literal::Bool(true))]),
        }];
        assert!(validate_activation_options(&active, &inst).is_empty());
    }
}
