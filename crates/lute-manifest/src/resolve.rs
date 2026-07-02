use crate::types::Literal;
use std::collections::BTreeMap;

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

/// plugin §11.1 resolution order + §11.2 merge: last-layer-wins scalar/value override (Literal has no map variant → no deep-merge).
pub fn resolve_activation(
    graph: &ProfileGraph,
    selected: &str,
    scene_local: &ActivationMap,
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
                entry.insert(k.clone(), v.clone());
            } // scalar override
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

    Ok(order
        .into_iter()
        .map(|id| ActivePlugin {
            options: merged.remove(&id).unwrap_or_default(),
            id,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

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
        let active = resolve_activation(&g, "date-minigame", &BTreeMap::new()).unwrap();
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
        let active = resolve_activation(&g, "date-minigame", &scene_local).unwrap();
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
            resolve_activation(&g, "date", &std::collections::BTreeMap::new()),
            Err(ResolveError::ExtendsCycle(_))
        ));
    }

    #[test]
    fn unknown_selected_profile_is_error() {
        let g = graph();
        assert!(matches!(
            resolve_activation(&g, "nope", &std::collections::BTreeMap::new()),
            Err(ResolveError::UnknownProfile(_))
        ));
    }

    #[test]
    fn unknown_parent_profile_is_error() {
        let mut g = graph();
        g.profiles.get_mut("date").unwrap().extends = Some("missing".into());
        assert!(matches!(
            resolve_activation(&g, "date", &std::collections::BTreeMap::new()),
            Err(ResolveError::UnknownProfile(_))
        ));
    }
}
