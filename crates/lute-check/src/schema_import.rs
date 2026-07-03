//! Scene `uses:` schema imports (dsl §9.2). This module owns the resolved
//! import result; the DAG file resolver (`resolve_imports`) lands in Task U2.
use std::collections::BTreeMap;

use lute_core_span::Diagnostic;

use crate::meta::StateSchema;

/// The resolved result of a scene's `uses:` imports: the merged imported state
/// schema, the merged imported `defs` (untyped YAML values, like inline defs),
/// and every `E-USES-*` diagnostic produced while resolving them.
#[derive(Clone, Debug, Default)]
pub struct SchemaImports {
    pub state: StateSchema,
    pub defs: BTreeMap<String, serde_yaml::Value>,
    pub diags: Vec<Diagnostic>,
}
