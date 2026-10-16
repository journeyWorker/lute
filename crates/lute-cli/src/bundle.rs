use std::collections::{BTreeMap, BTreeSet};

use lute_compile::index::IndexUnions;
use lute_ir::{IndexDef, IndexOccasion};
use lute_manifest::schema::WriteValue;
use lute_model::ProjectModel;

/// Collect the capability vocabulary that is shared by every compiled document.
/// Documents are visited in source-path order so a duplicate declaration's
/// first value is stable and matches the whole-project play collector.
pub(crate) fn collect_index_unions(model: &ProjectModel) -> IndexUnions {
    let mut documents: Vec<_> = model
        .documents()
        .iter()
        .filter(|document| !crate::compile_all::is_component_file(&document.path))
        .collect();
    documents.sort_by(|a, b| a.path.cmp(&b.path));

    let mut unions = IndexUnions {
        // dsl 0.47.0 §2.2: the project's source language, for the index.
        content_lang: model
            .manifest()
            .and_then(|m| m.defaults.get("contentLang"))
            .and_then(|v| v.as_str())
            .map(str::to_string),
        ..IndexUnions::default()
    };
    let mut world_events = BTreeSet::new();
    for document in &documents {
        let input = &document.input;
        for (name, decl) in &input.snapshot.occasions {
            unions
                .occasions
                .entry(name.clone())
                .or_insert_with(|| IndexOccasion::from(decl));
        }
        world_events.extend(input.snapshot.events.keys().cloned());
        for (id, member) in lute_check::cast::declared_cast(&input.snapshot, &input.imports, &[]) {
            if let Some(name) = member.name {
                unions.cast.entry(id).or_insert(name);
            }
        }
        for (tag, decl) in &input.snapshot.directives {
            let Some(bridge) = &decl.bridge else { continue };
            let Some(capability) = input
                .snapshot
                .bridge_capabilities
                .get(&(bridge.service.clone(), bridge.operation.clone()))
            else {
                continue;
            };
            let writes = decl
                .effects
                .iter()
                .flat_map(|effects| &effects.writes)
                .filter_map(|write| match &write.value {
                    WriteValue::FromBridgeResult { from_bridge_result } => {
                        Some(from_bridge_result.as_str())
                    }
                    _ => None,
                });
            for field in writes {
                let Some(result) = capability.result.iter().find(|result| result.name == field)
                else {
                    continue;
                };
                // 0.41.0 §4.1: the declared result type, as occasion
                // payloads carry theirs.
                unions
                    .bridge_results
                    .entry(tag.clone())
                    .or_default()
                    .insert(field.to_string(), result.ty.clone());
            }
        }
    }
    unions.world_events = world_events.into_iter().collect();
    unions.defs = index_defs(documents.iter().map(|document| &document.input));
    unions
}

/// The project index `defs` over the inputs of the project's documents (its
/// component fragments excluded), given in source-path order.
///
/// dsl 0.41.0 §4.3: a def two documents see with different
/// bodies or parameter lists (their `uses:` differ) names no one condition,
/// so the index omits it. Bodies are compared as lowered `expr` plus
/// `params`: the same condition spelled differently (`@base` in one, written
/// out in the other) is one def, exported with the first document's
/// `cel`/`authored`.
pub(crate) fn index_defs<'a>(
    inputs: impl IntoIterator<Item = &'a lute_check::CheckInput>,
) -> BTreeMap<String, IndexDef> {
    let mut defs: BTreeMap<String, IndexDef> = BTreeMap::new();
    let mut ambiguous = BTreeSet::new();
    for input in inputs {
        for (name, def) in lute_compile::index::document_defs(input) {
            match defs.get(&name) {
                Some(seen) if seen.expr != def.expr || seen.params != def.params => {
                    ambiguous.insert(name);
                }
                Some(_) => {}
                None => {
                    defs.insert(name, def);
                }
            }
        }
    }
    defs.retain(|name, _| !ambiguous.contains(name));
    defs
}

/// Serialize the exact bundle shape emitted by `compile --all`.
pub(crate) fn build_bundle(
    compiled: &std::collections::BTreeMap<String, lute_compile::ExecutionIr>,
    unions: &IndexUnions,
) -> Result<lute_runtime::index::Bundle, (u8, Vec<String>)> {
    let inputs = compiled
        .iter()
        .map(|(path, artifact)| lute_compile::index::IndexInput {
            path: path.clone(),
            artifact_path: format!("{path}.json"),
            artifact,
        })
        .collect::<Vec<_>>();
    let index = lute_compile::index::build_index(lute_compile::LUTE_IR_VERSION, &inputs, unions)
        .map_err(|errors| {
            let mut lines = errors
                .iter()
                .map(|e| format!("lute play: {e}"))
                .collect::<Vec<_>>();
            lines.push(format!(
                "lute play: {} vocabulary conflict(s); refusing to play",
                errors.len()
            ));
            (1, lines)
        })?;
    let artifacts = compiled
        .iter()
        .map(|(path, artifact)| {
            serde_json::to_value(artifact)
                .map(|value| (format!("{path}.json"), value))
                .map_err(|e| {
                    (
                        2,
                        vec![format!(
                            "lute play: cannot serialize the artifact of {path}: {e}"
                        )],
                    )
                })
        })
        .collect::<Result<_, _>>()?;
    let index = serde_json::to_value(index).map_err(|e| {
        (
            2,
            vec![format!(
                "lute play: cannot serialize project.index.json: {e}"
            )],
        )
    })?;
    Ok(lute_runtime::index::Bundle::new(artifacts, index))
}
