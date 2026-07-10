use crate::types::Literal;
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
}
