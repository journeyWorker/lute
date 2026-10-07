use super::SchemaError;
use crate::types::{Literal, PathSegment, Type};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DirectiveDecl {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<String>,
    /// A directive without attributes may omit `attrs:`.
    #[serde(default)]
    pub attrs: Vec<AttrDecl>,
    #[serde(default)]
    pub semantics: Vec<String>, // closed vocabulary; validated in Task 1.5
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<DirectiveState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effects: Option<DirectiveEffects>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bridge: Option<BridgeRef>,
    /// How the directive lowers. Absent (`Lowering::Passthrough`) means the
    /// generic `kind: "plugin"` passthrough record (dsl 0.24.0 T3-7).
    #[serde(default, skip_serializing_if = "Lowering::is_passthrough")]
    pub lower: Lowering,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttrDecl {
    pub name: String,
    #[serde(default)]
    pub required: bool,
    #[serde(rename = "type")]
    pub ty: Type,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Literal>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DirectiveState {
    pub declares: Vec<SlotDecl>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlotDecl {
    pub scope: String,
    pub path: Vec<PathSegment>,
    pub shape: String,
}

/// A directive's declared effects (plugin §7.4): the state it `writes`
/// and — dsl 0.27.0 §4 — the facts it `asserts` / `retracts`. Every list is
/// optional; the engine applies them after the call (writes, then retracts,
/// then asserts).
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DirectiveEffects {
    #[serde(default)]
    pub writes: Vec<WriteDecl>,
    /// dsl 0.27.0 §4: facts the call asserts — `holding(@item)`, each
    /// `@attr` the call's attribute (or its declared `default:`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub asserts: Vec<FactEffect>,
    /// dsl 0.27.0 §4: fact patterns the call retracts (`_` = any).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub retracts: Vec<FactEffect>,
}

impl DirectiveEffects {
    /// `true` when the block declares nothing at all.
    pub fn is_empty(&self) -> bool {
        self.writes.is_empty() && self.asserts.is_empty() && self.retracts.is_empty()
    }

    /// `true` when the block declares a fact effect.
    pub fn has_facts(&self) -> bool {
        !self.asserts.is_empty() || !self.retracts.is_empty()
    }
}

/// Hand-written so a block without `asserts`/`retracts` prints exactly as it
/// did before they existed: `capabilitySnapshot` hashes this `Debug`.
impl std::fmt::Debug for DirectiveEffects {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = f.debug_struct("DirectiveEffects");
        s.field("writes", &self.writes);
        if !self.asserts.is_empty() {
            s.field("asserts", &self.asserts);
        }
        if !self.retracts.is_empty() {
            s.field("retracts", &self.retracts);
        }
        s.finish()
    }
}

/// One `effects.asserts` / `effects.retracts` entry (dsl 0.27.0 §4): a fact
/// pattern `rel(arg, …)` whose arguments are members / `true` / `false`,
/// `@attr` references to the directive's own attributes, or (a retract
/// only) `_`. Written and serialized as that one string; any other shape is
/// an `E-PLUGIN-PARSE`.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct FactEffect {
    pub relation: String,
    pub args: Vec<FactEffectArg>,
}

/// One argument of a [`FactEffect`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FactEffectArg {
    /// A member, `true` or `false`, as written.
    Const(String),
    /// `@attr`: the call's value for the directive attribute `attr`.
    Attr(String),
    /// `_`: any value (a retract's bulk position).
    Wildcard,
}

impl FactEffect {
    /// The `@attr` names the pattern reads, in argument order.
    pub fn attrs(&self) -> impl Iterator<Item = &str> {
        self.args.iter().filter_map(|a| match a {
            FactEffectArg::Attr(n) => Some(n.as_str()),
            _ => None,
        })
    }
}

impl std::fmt::Display for FactEffect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}(", self.relation)?;
        for (i, a) in self.args.iter().enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            match a {
                FactEffectArg::Const(c) => f.write_str(c)?,
                FactEffectArg::Attr(n) => write!(f, "@{n}")?,
                FactEffectArg::Wildcard => f.write_str("_")?,
            }
        }
        f.write_str(")")
    }
}

impl std::fmt::Debug for FactEffect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.to_string())
    }
}

impl From<FactEffect> for String {
    fn from(e: FactEffect) -> String {
        e.to_string()
    }
}

impl TryFrom<String> for FactEffect {
    type Error = SchemaError;

    fn try_from(s: String) -> Result<Self, SchemaError> {
        let bad = |why: &str| {
            Err(SchemaError::Fact(format!(
                "effects fact `{s}` {why}; a fact effect is `relation(arg, …)` whose args are \
                 members, `true`/`false`, `@attr` (one of the directive's attrs) or `_`"
            )))
        };
        let ident = crate::ident::is_name;
        let t = s.trim();
        let Some((relation, rest)) = t.split_once('(') else {
            return bad("has no `(`");
        };
        let relation = relation.trim();
        if !ident(relation) {
            return bad("does not start with a relation name");
        }
        let Some(inner) = rest.strip_suffix(')') else {
            return bad("does not end with `)`");
        };
        let mut args = Vec::new();
        if !inner.trim().is_empty() {
            for raw in inner.split(',') {
                let a = raw.trim();
                args.push(if a == "_" {
                    FactEffectArg::Wildcard
                } else if let Some(n) = a.strip_prefix('@') {
                    if !ident(n) {
                        return bad(&format!("has `{a}`, which names no attr"));
                    }
                    FactEffectArg::Attr(n.to_string())
                } else if ident(a) {
                    FactEffectArg::Const(a.to_string())
                } else {
                    return bad(&format!("has the argument `{a}`"));
                });
            }
        }
        Ok(FactEffect {
            relation: relation.to_string(),
            args,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WriteDecl {
    pub scope: String,
    pub path: Vec<PathSegment>,
    pub value: WriteValue,
}

/// One `effects.writes[].value` (plugin §7.4, dsl 0.27.0 §2): the write's
/// source. Parsed through [`WriteValue::try_from`] so every shape outside the
/// legal four is an `E-PLUGIN-PARSE` naming them — `#[serde(untagged)]` over
/// [`Literal`] used to take ANY mapping as a literal record, so a
/// `{ fromAttr: amount }` loaded clean and wrote nothing.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged, try_from = "serde_yaml::Value")]
pub enum WriteValue {
    /// `{ fromBridgeResult: <field> }` — the bridge answer's field.
    FromBridgeResult {
        #[serde(rename = "fromBridgeResult")]
        from_bridge_result: String,
    },
    /// `{ op: increment | decrement, by: <number> | { fromAttr: <attr> } }`.
    Op { op: String, by: OpBy },
    /// `{ fromAttr: <attr> }` — the call's attribute value (its declared
    /// `default:` when the call omits it).
    FromAttr {
        #[serde(rename = "fromAttr")]
        from_attr: String,
    },
    /// A scalar literal: a bool, number or string.
    Literal(Literal),
}

/// The `by:` of an `op` write: a number, or `{ fromAttr: <attr> }` naming a
/// `type: number` attribute of the directive.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OpBy {
    Num(f64),
    FromAttr {
        #[serde(rename = "fromAttr")]
        from_attr: String,
    },
}

/// `capabilitySnapshot` hashes a directive's `Debug`: a numeric `by` renders
/// as the bare number it was before `OpBy` existed, so an unchanged manifest
/// keeps its version.
impl std::fmt::Debug for OpBy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpBy::Num(n) => write!(f, "{n:?}"),
            OpBy::FromAttr { from_attr } => f
                .debug_struct("FromAttr")
                .field("from_attr", from_attr)
                .finish(),
        }
    }
}

/// The ops an `op` write may name (the play runner and the engine apply
/// exactly these).
pub const WRITE_OPS: &[&str] = &["increment", "decrement"];

const WRITE_VALUE_SHAPES: &str = "a bool/number/string literal, `{ fromAttr: <attr> }`, \
     `{ fromBridgeResult: <field> }`, or `{ op: increment|decrement, by: <number> | \
     { fromAttr: <attr> } }`";

impl TryFrom<serde_yaml::Value> for WriteValue {
    type Error = SchemaError;

    fn try_from(v: serde_yaml::Value) -> Result<Self, SchemaError> {
        use serde_yaml::Value;
        let bad = |what: String| {
            Err(SchemaError::Write(format!(
                "effects.writes value {what}; a write's value is {WRITE_VALUE_SHAPES}"
            )))
        };
        let attr_name = |v: &Value, key: &str| -> Result<String, SchemaError> {
            match v.get("fromAttr") {
                Some(Value::String(a)) if v.as_mapping().is_some_and(|m| m.len() == 1) => {
                    Ok(a.clone())
                }
                _ => Err(SchemaError::Write(format!(
                    "effects.writes value: `{key}` must be `{{ fromAttr: <attr name> }}`"
                ))),
            }
        };
        match &v {
            Value::Bool(b) => Ok(WriteValue::Literal(Literal::Bool(*b))),
            Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ok(WriteValue::Literal(Literal::Int(i)))
                } else if let Some(f) = n.as_f64() {
                    Ok(WriteValue::Literal(Literal::Double(f)))
                } else {
                    bad(format!("`{n}` is not a representable number"))
                }
            }
            Value::String(s) => Ok(WriteValue::Literal(Literal::Str(s.clone()))),
            Value::Mapping(m) => {
                let keys: Vec<&str> = m.keys().filter_map(Value::as_str).collect();
                let mut sorted = keys.clone();
                sorted.sort_unstable();
                match sorted.as_slice() {
                    ["fromBridgeResult"] => match v.get("fromBridgeResult") {
                        Some(Value::String(f)) => Ok(WriteValue::FromBridgeResult {
                            from_bridge_result: f.clone(),
                        }),
                        _ => bad("`fromBridgeResult:` must name a result field".into()),
                    },
                    ["fromAttr"] => Ok(WriteValue::FromAttr {
                        from_attr: attr_name(&v, "value")?,
                    }),
                    ["by", "op"] => {
                        let op = match v.get("op") {
                            Some(Value::String(op)) if WRITE_OPS.contains(&op.as_str()) => {
                                op.clone()
                            }
                            other => {
                                let shown = other
                                    .and_then(Value::as_str)
                                    .map(|s| format!("`op: {s}`"))
                                    .unwrap_or_else(|| "`op:`".into());
                                return bad(format!(
                                    "{shown} is not an op (ops: {})",
                                    WRITE_OPS.join(", ")
                                ));
                            }
                        };
                        let by = match v.get("by") {
                            Some(Value::Number(n)) => {
                                let Some(value) = n.as_f64() else {
                                    return bad(
                                        "`by:` must be a number or `{ fromAttr: <attr> }`".into(),
                                    );
                                };
                                OpBy::Num(value)
                            }
                            Some(by @ Value::Mapping(_)) => OpBy::FromAttr {
                                from_attr: attr_name(by, "by")?,
                            },
                            _ => {
                                return bad(
                                    "`by:` must be a number or `{ fromAttr: <attr> }`".into()
                                )
                            }
                        };
                        Ok(WriteValue::Op { op, by })
                    }
                    _ => {
                        let known = ["fromBridgeResult", "fromAttr", "op", "by"];
                        let hint = keys
                            .iter()
                            .filter(|k| !known.contains(k))
                            .find_map(|k| {
                                crate::suggest::nearest(k, known, 2)
                                    .map(|s| format!(" (did you mean `{s}`?)"))
                            })
                            .unwrap_or_default();
                        bad(format!(
                            "`{{ {} }}` is not a write source{hint}",
                            keys.iter()
                                .map(|k| format!("{k}: …"))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ))
                    }
                }
            }
            Value::Sequence(_) => bad("is a list".into()),
            Value::Null => bad("is empty".into()),
            Value::Tagged(_) => bad("is a tagged value".into()),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BridgeRef {
    pub service: String,
    pub operation: String,
}

/// A directive's `lower:` (plugin §8.2): `{ record, fields }` (declarative,
/// one core staging record), `{ kind: builtin, name }` (a named hook from
/// the core registry, [`BUILTIN_LOWERING_HOOKS`]), or — when `lower:` is
/// absent — the generic `kind: "plugin"` passthrough.
///
/// Deserialized through [`RawLowering`] so a malformed `lower:` names what
/// is wrong (unknown key, unknown `kind`, unregistered hook) instead of
/// serde's "did not match any variant". `Record`/`Builtin` keep their
/// derived `Debug`, which `capabilitySnapshot` hashes.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(untagged, try_from = "RawLowering")]
pub enum Lowering {
    Record {
        record: String,
        fields: serde_yaml::Value,
    },
    Builtin {
        kind: String,
        name: String,
    },
    /// No `lower:` declared: the generic passthrough. Never serialized.
    #[default]
    Passthrough,
}

impl Lowering {
    pub fn is_passthrough(&self) -> bool {
        matches!(self, Lowering::Passthrough)
    }
}

/// The core's closed registry of builtin lowering hooks (plugin §8.2: "`name`
/// MUST resolve to a registered hook"), exactly the hooks the `lute.core`
/// staging manifest names. Adding one is a core code change.
pub const BUILTIN_LOWERING_HOOKS: &[&str] = &[
    "actorStage",
    "cameraTransform",
    "clearStage",
    "end",
    "jump",
    "label",
    "sequence",
];

/// The wire shape of `lower:`, validated into a [`Lowering`].
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLowering {
    record: Option<String>,
    fields: Option<serde_yaml::Value>,
    kind: Option<String>,
    name: Option<String>,
}

const OMIT_LOWER: &str = "omit `lower:` for the generic `kind: \"plugin\"` passthrough";

impl TryFrom<RawLowering> for Lowering {
    type Error = SchemaError;

    fn try_from(raw: RawLowering) -> Result<Self, SchemaError> {
        match raw {
            RawLowering {
                record: Some(record),
                fields,
                kind: None,
                name: None,
            } => match fields {
                Some(fields) => Ok(Lowering::Record { record, fields }),
                None => Err(SchemaError::Lowering(format!(
                    "`lower: {{ record: {record} }}` needs `fields:` (write `fields: {{}}` for none)"
                ))),
            },
            RawLowering {
                record: None,
                fields: None,
                kind: Some(kind),
                name,
            } => {
                if kind != "builtin" {
                    return Err(SchemaError::Lowering(format!(
                        "`lower.kind` is `{kind}`, but the only kind is `builtin` \
                         (or write `{{ record, fields }}`); {OMIT_LOWER}"
                    )));
                }
                let Some(name) = name else {
                    return Err(SchemaError::Lowering(format!(
                        "`lower: {{ kind: builtin }}` needs `name:` (one of {}); {OMIT_LOWER}",
                        BUILTIN_LOWERING_HOOKS.join(", ")
                    )));
                };
                if !BUILTIN_LOWERING_HOOKS.contains(&name.as_str()) {
                    let max = (name.chars().count() / 3).clamp(1, 3);
                    let hint = crate::suggest::nearest(&name, BUILTIN_LOWERING_HOOKS.iter().copied(), max)
                        .map(|s| format!(" (did you mean `{s}`?)"))
                        .unwrap_or_default();
                    return Err(SchemaError::Lowering(format!(
                        "`{name}` is not a builtin lowering hook{hint}; the core registers {}; {OMIT_LOWER}",
                        BUILTIN_LOWERING_HOOKS.join(", ")
                    )));
                }
                Ok(Lowering::Builtin { kind, name })
            }
            RawLowering {
                record: None,
                fields: None,
                kind: None,
                name: None,
            } => Err(SchemaError::Lowering(format!(
                "`lower:` is empty; write `{{ record, fields }}` or `{{ kind: builtin, name }}`, or {OMIT_LOWER}"
            ))),
            _ => Err(SchemaError::Lowering(format!(
                "`lower:` is either `{{ record, fields }}` or `{{ kind: builtin, name }}`, not a mix; {OMIT_LOWER}"
            ))),
        }
    }
}

