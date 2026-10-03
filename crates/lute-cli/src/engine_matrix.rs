use std::collections::BTreeSet;
use std::path::Path;

use serde::Deserialize;
use serde_json::Value as Json;

/// `lute.engine.yaml` (dsl 0.33.0 §4). The optional descriptive `version` and
/// `description` keys never participate in negotiation, so like any unknown
/// key they are accepted and not read.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EngineMatrix {
    pub engine: String,
    pub ir_version: String,
    pub supported_ids: Vec<String>,
}

impl EngineMatrix {
    pub(crate) fn reference() -> Self {
        Self {
            engine: "reference".into(),
            ir_version: lute_compile::LUTE_IR_VERSION.into(),
            supported_ids: lute_compile::semantics::REGISTRY.iter().map(|e| e.id.as_str().to_string()).collect(),
        }
    }

    pub(crate) fn load(path: Option<&Path>) -> Result<Self, String> {
        let Some(path) = path else { return Ok(Self::reference()); };
        let text = std::fs::read_to_string(path).map_err(|e| format!("E-ENGINE-MATRIX: cannot read {}: {}", path.display(), e))?;
        let matrix: Self = serde_yaml::from_str(&text).map_err(|e| format!("E-ENGINE-MATRIX: {}", e))?;
        if matrix.engine.trim().is_empty() || parse_version(&matrix.ir_version).is_none() {
            return Err("E-ENGINE-MATRIX: `engine` must be non-empty and `irVersion` must be MAJOR.MINOR.PATCH".into());
        }
        let mut seen = BTreeSet::new();
        for id in &matrix.supported_ids {
            if !well_formed_id(id) {
                return Err(format!("E-ENGINE-MATRIX: malformed semantic id `{id}`"));
            }
            if !lute_compile::semantics::is_registered(id) {
                return Err(format!("E-ENGINE-MATRIX: unknown semantic id `{id}`"));
            }
            if !seen.insert(id) {
                return Err(format!("E-ENGINE-MATRIX: duplicate semantic id `{id}`"));
            }
        }
        Ok(matrix)
    }

    pub(crate) fn negotiate(&self, artifact: &Json) -> Result<(), String> {
        let ir = artifact.get("irVersion").and_then(Json::as_str).unwrap_or("");
        let accepted = major_minor_match(&self.ir_version, ir);
        if !accepted {
            return Err(format!("E-ENGINE-IR-VERSION: engine `{}` accepts `{}`, artifact has `{}`", self.engine, self.ir_version, ir));
        }
        let supported: BTreeSet<&str> = self.supported_ids.iter().map(String::as_str).collect();
        let required = artifact.get("requiredSemantics").and_then(Json::as_array).ok_or_else(|| "E-SEMANTICS-UNKNOWN: artifact has no valid `requiredSemantics`".to_string())?;
        let mut missing = BTreeSet::new();
        for value in required {
            let id = value.as_str().ok_or_else(|| "E-SEMANTICS-UNKNOWN: requiredSemantics contains a non-string".to_string())?;
            if !lute_compile::semantics::is_registered(id) { return Err(format!("E-SEMANTICS-UNKNOWN: unknown semantic id `{id}`")); }
            if !supported.contains(id) { missing.insert(id); }
        }
        if !missing.is_empty() {
            return Err(format!(
                "E-ENGINE-SEMANTICS: engine `{}` does not support semantic ids: {}",
                self.engine,
                missing.into_iter().map(|id| format!("`{id}`")).collect::<Vec<_>>().join(", "),
            ));
        }
        Ok(())
    }
}

fn well_formed_id(id: &str) -> bool {
    let Some((name, major)) = id.rsplit_once('/') else { return false };
    if major.is_empty() || !major.bytes().all(|b| b.is_ascii_digit()) || major == "0" {
        return false;
    }
    if name == "lute.core" {
        return true;
    }
    let Some(rest) = name.strip_prefix("lute.") else { return false };
    let mut parts = rest.split('.');
    parts.next().is_some_and(|module| !module.is_empty())
        && parts.all(|feature| !feature.is_empty())
    }

fn parse_version(v: &str) -> Option<(u64, u64, u64)> {
    let parts: Vec<_> = v.split('.').collect();
    if parts.len() != 3 || parts.iter().any(|part| part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit())) {
        return None;
    }
    Some((parts[0].parse().ok()?, parts[1].parse().ok()?, parts[2].parse().ok()?))
}

fn major_minor_match(expected: &str, actual: &str) -> bool {
    match (parse_version(expected), parse_version(actual)) {
        (Some((a, b, _)), Some((c, d, _))) => a == c && (a != 0 || b == d),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_minor_pre_one() {
        assert!(major_minor_match("0.33.0", "0.33.9"));
        assert!(!major_minor_match("0.33.0", "0.32.0"));
    }

    #[test]
    fn semantic_ids_require_a_module_feature_and_positive_major() {
        assert!(well_formed_id("lute.core/1"));
        assert!(well_formed_id("lute.quest.lifecycle/1"));
        assert!(!well_formed_id("lute./1"));
        assert!(!well_formed_id("lute.quest.lifecycle/0"));
        assert!(!well_formed_id("lute.quest.lifecycle"));
    }
}
