use serde_json::Value as Json;
use std::collections::BTreeMap;

/// Compute the stable identity of a runtime bundle.
pub(crate) fn fingerprint(index: &Json, artifacts: &BTreeMap<String, Json>) -> String {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&serde_json::to_vec(index).expect("JSON values serialize"));
    for value in artifacts.values() {
        bytes.extend_from_slice(&serde_json::to_vec(value).expect("JSON values serialize"));
    }
    let ir = index.get("irVersion").and_then(Json::as_str).unwrap_or("");
    let capability = index
        .get("capabilitySnapshot")
        .and_then(Json::as_str)
        .unwrap_or("");
    format!(
        "{ir}:{capability}:{}",
        lute_manifest::snapshot::sha256_hex(&bytes)
    )
}
