//! Multi-plugin capability-snapshot assembly (plugin §13). Merges every active
//! plugin's loaded package onto the embedded `lute.core` base into one
//! deterministic snapshot, rejecting cross-plugin duplicate ids and reserved
//! names, and stamping `capabilityVersion`.

use std::collections::{BTreeMap, BTreeSet};

use crate::core::load_core_snapshot;
use crate::resolve::{ActivePlugin, InstalledPlugins};
use crate::schema::StateShape;
use crate::snapshot::{capability_version, CapabilitySnapshot, ResolvedPlugin};

#[derive(Clone, Debug, PartialEq)]
pub enum AssembleError {
    DuplicateAcrossPlugins {
        kind: String,
        id: String,
        first: String,
        second: String,
    },
    ReservedName {
        id: String,
        plugin: String,
    },
    MissingActivePlugin {
        id: String,
    },
    InvalidDirective {
        plugin: String,
        directive: String,
        msg: String,
    },
    CyclicStateShape {
        shape: String,
    },
}

/// dsl §10 reserved terms a non-core plugin MUST NOT (re)define as a directive.
/// `cut` is core-owned, so it is only reserved against NON-core plugins.
const RESERVED_DIRECTIVE_NAMES: &[&str] = &["scene", "cut"];

/// Merge every ACTIVE plugin's loaded package onto the embedded `lute.core`
/// base into a single deterministic capability snapshot (plugin §13). Returns
/// the assembled snapshot plus any cross-plugin duplicate / reserved-name /
/// missing-plugin errors; an offending item is dropped, never merged. The
/// `inactive` index is populated from installed-minus-active, and the resolved
/// snapshot is finally stamped with its `capabilityVersion`.
pub fn assemble_snapshot(
    active: &[ActivePlugin],
    installed: &InstalledPlugins,
) -> (CapabilitySnapshot, Vec<AssembleError>) {
    let mut snap = load_core_snapshot();
    let mut errs = Vec::new();
    // Track which plugin owns each merged directive for precise dup errors.
    let mut dir_owner: BTreeMap<String, String> = snap
        .directives
        .keys()
        .map(|k| (k.clone(), "lute.core".to_string()))
        .collect();

    for ap in active {
        if ap.id == "lute.core" {
            // Already embedded; just record resolved options.
            let version = snap
                .plugins
                .get("lute.core")
                .map(|p| p.version.clone())
                .unwrap_or_default();
            snap.plugins.insert(
                ap.id.clone(),
                ResolvedPlugin {
                    version,
                    options: ap.options.clone(),
                },
            );
            continue;
        }
        let Some(inst) = installed.get(&ap.id) else {
            errs.push(AssembleError::MissingActivePlugin { id: ap.id.clone() });
            continue;
        };
        let pkg = &inst.loaded;

        for d in &pkg.directives {
            if RESERVED_DIRECTIVE_NAMES.contains(&d.name.as_str()) {
                errs.push(AssembleError::ReservedName {
                    id: d.name.clone(),
                    plugin: ap.id.clone(),
                });
                continue;
            }
            if let Some(first) = dir_owner.get(&d.name) {
                errs.push(AssembleError::DuplicateAcrossPlugins {
                    kind: "directive".into(),
                    id: d.name.clone(),
                    first: first.clone(),
                    second: ap.id.clone(),
                });
                continue;
            }
            for me in crate::validate::validate_directive(d) {
                errs.push(AssembleError::InvalidDirective {
                    plugin: ap.id.clone(),
                    directive: d.name.clone(),
                    msg: format!("{me:?}"),
                });
            }
            dir_owner.insert(d.name.clone(), ap.id.clone());
            snap.directives.insert(d.name.clone(), d.clone());
        }
        merge_map(
            &mut snap.state_shapes,
            pkg.state_shapes.iter().map(|s| (s.name.clone(), s.clone())),
            "shape",
            &ap.id,
            &mut errs,
        );
        merge_map(
            &mut snap.state_templates,
            pkg.state_templates
                .iter()
                .map(|t| (t.name.clone(), t.clone())),
            "template",
            &ap.id,
            &mut errs,
        );
        merge_map(
            &mut snap.providers,
            pkg.providers.iter().map(|p| (p.name.clone(), p.clone())),
            "provider",
            &ap.id,
            &mut errs,
        );
        merge_map(
            &mut snap.defs,
            pkg.defs.iter().map(|d| (d.name.clone(), d.clone())),
            "def",
            &ap.id,
            &mut errs,
        );
        merge_map(
            &mut snap.frontmatter,
            pkg.frontmatter.iter().map(|(k, v)| (k.clone(), v.clone())),
            "frontmatter",
            &ap.id,
            &mut errs,
        );
        merge_map(
            &mut snap.enums,
            pkg.enums.iter().map(|(k, v)| (k.clone(), v.clone())),
            "enum",
            &ap.id,
            &mut errs,
        );
        for b in &pkg.bridge {
            let k = (b.service.clone(), b.operation.clone());
            match snap.bridge_capabilities.entry(k) {
                std::collections::btree_map::Entry::Occupied(_) => {
                    errs.push(AssembleError::DuplicateAcrossPlugins {
                        kind: "bridge".into(),
                        id: format!("{}.{}", b.service, b.operation),
                        first: "?".into(),
                        second: ap.id.clone(),
                    });
                }
                std::collections::btree_map::Entry::Vacant(e) => {
                    e.insert(b.clone());
                }
            }
        }
        snap.plugins.insert(
            ap.id.clone(),
            ResolvedPlugin {
                version: pkg.manifest.version.clone(),
                options: ap.options.clone(),
            },
        );
    }

    // Inactive index (plugin §11.2 fix-it): every installed directive whose
    // plugin is not active, tag -> owning plugin id.
    let active_ids: BTreeSet<&str> = active.iter().map(|a| a.id.as_str()).collect();
    for (id, inst) in &installed.by_id {
        if active_ids.contains(id.as_str()) {
            continue;
        }
        for d in &inst.loaded.directives {
            snap.inactive
                .entry(d.name.clone())
                .or_insert_with(|| id.clone());
        }
    }

    // Reject cyclic state-shape references (a non-conforming package; the checker
    // also guards at expansion time for no-panic, but assembly diagnoses it up
    // front, like a depends-cycle). Runs over the fully merged shapes.
    detect_state_shape_cycles(&snap.state_shapes, &mut errs);

    snap.version = capability_version(&snap);
    (snap, errs)
}

fn merge_map<V: Clone>(
    dst: &mut BTreeMap<String, V>,
    items: impl Iterator<Item = (String, V)>,
    kind: &str,
    plugin: &str,
    errs: &mut Vec<AssembleError>,
) {
    for (k, v) in items {
        match dst.entry(k) {
            std::collections::btree_map::Entry::Occupied(e) => {
                errs.push(AssembleError::DuplicateAcrossPlugins {
                    kind: kind.into(),
                    id: e.key().clone(),
                    first: "?".into(),
                    second: plugin.into(),
                });
            }
            std::collections::btree_map::Entry::Vacant(e) => {
                e.insert(v);
            }
        }
    }
}

/// Detect a cycle in the merged state-shape graph (plugin §6.2: a conforming
/// package's shapes form a DAG). An edge is a field's `shape:` reference to
/// another shape that also exists in the snapshot; a field naming a MISSING
/// shape is a separate concern and is ignored here. Iterative DFS with
/// visiting/done marks — catches self-cycles (A -> A) and mutual cycles
/// (A -> B -> A) without false-positiving diamonds (A -> B, A -> C, B -> D,
/// C -> D). Deterministic: roots iterate in BTreeMap order, deps sorted+deduped.
/// Each shape on a back-edge is reported once.
fn detect_state_shape_cycles(shapes: &BTreeMap<String, StateShape>, errs: &mut Vec<AssembleError>) {
    #[derive(Clone, Copy, PartialEq)]
    enum Mark {
        Visiting,
        Done,
    }
    fn deps(shapes: &BTreeMap<String, StateShape>, name: &str) -> Vec<String> {
        let mut d: Vec<String> = shapes
            .get(name)
            .map(|s| {
                s.fields
                    .iter()
                    .filter_map(|f| f.shape.clone())
                    .filter(|n| shapes.contains_key(n))
                    .collect()
            })
            .unwrap_or_default();
        d.sort();
        d.dedup();
        d
    }
    let mut state: BTreeMap<String, Mark> = BTreeMap::new();
    let mut reported: BTreeSet<String> = BTreeSet::new();
    for root in shapes.keys() {
        if state.contains_key(root) {
            continue;
        }
        let mut stack: Vec<(String, Vec<String>, usize)> =
            vec![(root.clone(), deps(shapes, root), 0)];
        state.insert(root.clone(), Mark::Visiting);
        while let Some((name, ds, cursor)) = stack.last_mut() {
            if *cursor < ds.len() {
                let dep = ds[*cursor].clone();
                *cursor += 1;
                match state.get(&dep) {
                    Some(Mark::Visiting) => {
                        if reported.insert(dep.clone()) {
                            errs.push(AssembleError::CyclicStateShape { shape: dep });
                        }
                    }
                    Some(Mark::Done) => {}
                    None => {
                        state.insert(dep.clone(), Mark::Visiting);
                        let dd = deps(shapes, &dep);
                        stack.push((dep, dd, 0));
                    }
                }
            } else {
                let done = name.clone();
                stack.pop();
                state.insert(done, Mark::Done);
            }
        }
    }
}
