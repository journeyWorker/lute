use std::collections::BTreeSet;

use lute_compile::index::{IndexOccasion, IndexUnions};
use lute_manifest::schema::WriteValue;
use lute_manifest::types::Type;
use lute_model::ProjectModel;

/// Collect the capability vocabulary that is shared by every compiled document.
/// Documents are visited in source-path order so a duplicate declaration's
/// first value is stable and matches the whole-project play collector.
pub(crate) fn collect_index_unions(model: &ProjectModel) -> IndexUnions {
    let mut documents: Vec<_> = model.documents().iter().collect();
    documents.sort_by(|a, b| a.path.cmp(&b.path));

    let mut unions = IndexUnions::default();
    let mut world_events = BTreeSet::new();
    for document in documents {
        if crate::compile_all::is_component_file(&document.path) {
            continue;
        }
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
                let ty = match result.ty {
                    Type::Bool => "bool",
                    Type::Int | Type::Double => "number",
                    _ => "string",
                };
                unions
                    .bridge_results
                    .entry(tag.clone())
                    .or_default()
                    .insert(field.to_string(), ty.to_string());
            }
        }
    }
    unions.world_events = world_events.into_iter().collect();
    unions
}

