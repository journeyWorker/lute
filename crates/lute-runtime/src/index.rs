//! Runtime bundle helpers for `project.index.json`.

use std::collections::BTreeMap;

use lute_ir::ProjectIndex;
use serde_json::Value as Json;

/// The runtime bundle: artifacts remain JSON while the index is decoded into
/// the shared wire types when the project is assembled.
#[derive(Clone, Debug, Default)]
pub struct Bundle {
    pub artifacts: BTreeMap<String, Json>,
    pub index: Json,
}

impl Bundle {
    pub fn new(artifacts: BTreeMap<String, Json>, index: Json) -> Self {
        Self { artifacts, index }
    }
}

pub fn occasions(index: &ProjectIndex) -> BTreeMap<String, lute_manifest::schema::OccasionDecl> {
    index
        .occasions
        .iter()
        .map(|(name, value)| {
            (
                name.clone(),
                lute_manifest::schema::OccasionDecl {
                    name: name.clone(),
                    select: value.select,
                    target: value.target.clone(),
                    judge: value
                        .judge
                        .unwrap_or(lute_manifest::schema::OccasionJudge::After),
                    payload: value.payload.clone(),
                    outside_run: value.outside_run,
                    ..Default::default()
                },
            )
        })
        .collect()
}

pub fn bridge_result_types(index: &ProjectIndex) -> BTreeMap<String, BTreeMap<String, String>> {
    index.bridge_results.clone()
}
