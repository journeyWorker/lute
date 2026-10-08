//! Runtime input data shared by concrete and preview walkers.
//!
//! Parsing and validation of these surfaces belongs to `lute-trace`; this
//! module only carries the already-parsed values the runtime consumes.

use serde_json::Value as Json;

use std::collections::{BTreeMap, BTreeSet};

use lute_core_span::Span;
use lute_manifest::schema::DirectiveDecl;
use lute_manifest::types::Type;

/// The merged mock surface supplied to a runtime walk.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MockSet<B = (), C = ()> {
    /// `(state path, literal text, source span)` seed entries.
    pub state: Vec<(String, String, Option<Span>)>,
    /// Raw ground fact-pattern text.
    pub facts: Vec<String>,
    /// Branch/hub id to ordered forced choice ids.
    pub choose: BTreeMap<String, Vec<String>>,
    /// World event names, in firing order.
    pub events: Vec<String>,
    /// Accepted quest ids, in acceptance order.
    pub accepts: Vec<String>,
    /// Previously visited scene ids.
    pub visited: Vec<String>,
    /// Raised occasion names, optionally suffixed with `@target`.
    pub occasions: Vec<String>,
    /// Whether Datalog derivation is enabled; defaults to true.
    pub derive: Option<bool>,
    /// Bridge answers by plugin tag, consumed in call order.
    pub bridges: BTreeMap<String, Vec<BridgeAnswer>>,
    /// Parser-owned source locations for bridge entries.
    pub bridge_spans: B,
    /// Parser-owned source locations for forced choices.
    pub choose_spans: C,
    /// Test-harness-only eligibility gate.
    pub gate_eligibility: bool,
    /// Project quest ids known to a test harness.
    pub project_quests: Option<BTreeSet<String>>,
}

impl<B, C> MockSet<B, C> {
    /// Whether derivation applies (the default is enabled).
    pub fn derives(&self) -> bool {
        self.derive.unwrap_or(true)
    }
}

/// One bridge answer: `(bridgeResult field, literal text)` pairs in source order.
pub type BridgeAnswer = Vec<(String, String)>;


/// Whether an occasion raise `raw` judges an objective with `on` and `target`.
pub fn raise_judges(raw: &str, on: &str, target: Option<&str>) -> bool {
    let (name, raised) = split_occasion(raw);
    name == on && target.is_none_or(|t| raised == Some(t))
}

/// Split an occasion raise into its name and optional target.
pub fn split_occasion(raw: &str) -> (&str, Option<&str>) {
    match raw.split_once('@') {
        Some((name, target)) => (name, Some(target)),
        None => (raw, None),
    }
}

/// Return effects that write bridge result fields.
pub fn bridge_result_writes(
    decl: &DirectiveDecl,
) -> Vec<(&str, &lute_manifest::schema::WriteDecl)> {
    decl.effects
        .iter()
        .flat_map(|effect| &effect.writes)
        .filter_map(|write| match &write.value {
            lute_manifest::schema::WriteValue::FromBridgeResult { from_bridge_result } => {
                Some((from_bridge_result.as_str(), write))
            }
            _ => None,
        })
        .collect()
}

/// Read a record's string field, returning the empty string when absent.
pub fn str_of<'a>(rec: &'a Json, key: &str) -> &'a str {
    rec.get(key).and_then(Json::as_str).unwrap_or("")
}

/// Placeholder for a bridge-result field of the declared type.
pub fn type_placeholder(ty: Option<&Type>) -> String {
    match ty {
        Some(Type::Bool) => "<bool>".to_string(),
        Some(Type::Int | Type::Double) => "<number>".to_string(),
        Some(Type::Str) => "<string>".to_string(),
        Some(Type::Enum(members)) => format!("<one of: {}>", members.join("|")),
        _ => "<value>".to_string(),
    }
}

/// Render the fields a bridge call must answer, preserving first occurrence.
pub fn bridge_answer_shape<'t>(
    fields: impl IntoIterator<Item = (&'t str, Option<&'t Type>)>,
) -> String {
    let mut seen = BTreeSet::new();
    let parts: Vec<String> = fields
        .into_iter()
        .filter(|(field, _)| seen.insert(*field))
        .map(|(field, ty)| format!("{field}: {}", type_placeholder(ty)))
        .collect();
    format!("{{ {} }}", parts.join(", "))
}
