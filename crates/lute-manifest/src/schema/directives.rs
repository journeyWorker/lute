use super::occasions::shown_yaml;
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

/// A directive's declared effects (plugin §7.4): the state it `writes`,
/// — dsl 0.27.0 §4 — the facts it `asserts` / `retracts`, and — 0.51.0 §4.1
/// — the occasions the host `raises` in answer to the call. Every list is
/// optional; the engine applies the first three after the call (writes,
/// then retracts, then asserts); a raise is the host's (0.51.0 §4.4).
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
    /// 0.51.0 §4.1 (#170): the occasions the host raises after the
    /// presentation that ran the call ends. Non-empty when written.
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "raise_list"
    )]
    pub raises: Vec<RaiseDecl>,
}

impl DirectiveEffects {
    /// `true` when the block declares nothing at all.
    pub fn is_empty(&self) -> bool {
        self.writes.is_empty()
            && self.asserts.is_empty()
            && self.retracts.is_empty()
            && self.raises.is_empty()
    }

    /// `true` when the block declares a fact effect.
    pub fn has_facts(&self) -> bool {
        !self.asserts.is_empty() || !self.retracts.is_empty()
    }

    /// `true` when a call changes state or facts: a `writes`, `asserts` or
    /// `retracts` entry. A declared raise changes neither (the host raises
    /// the occasion afterwards, 0.51.0 §4.4), so a raises-only directive
    /// stays presentational.
    pub fn writes_state(&self) -> bool {
        !self.writes.is_empty() || self.has_facts()
    }
}

/// Hand-written so a block without `asserts`/`retracts`/`raises` prints
/// exactly as it did before they existed: `capabilitySnapshot` hashes this
/// `Debug`.
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
        if !self.raises.is_empty() {
            s.field("raises", &self.raises);
        }
        s.finish()
    }
}

/// One `effects.raises` entry (0.51.0 §4.1): the occasion the host raises
/// and, for a targeted occasion, what it is raised for. Parsed through
/// [`RaiseDecl::try_from`] so a malformed entry names the shape it takes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "yaml_serde::Value")]
pub struct RaiseDecl {
    pub occasion: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<RaiseTargetDecl>,
}

/// A [`RaiseDecl`]'s `target`: `@<attr>` (a directive's own attribute, or a
/// reward kind's `@target`) or a literal target written whole
/// (`scene.chapter2`), as a beat writes it. Written and serialized as that
/// one string.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum RaiseTargetDecl {
    /// `@<attr>`: the call's value for the attribute (a reward's `target=`).
    Attr(String),
    /// A dotted target, as written.
    Literal(String),
}

impl RaiseTargetDecl {
    /// The attribute an `@<attr>` target reads.
    pub fn attr(&self) -> Option<&str> {
        match self {
            RaiseTargetDecl::Attr(a) => Some(a),
            RaiseTargetDecl::Literal(_) => None,
        }
    }
}

impl std::fmt::Display for RaiseTargetDecl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RaiseTargetDecl::Attr(a) => write!(f, "@{a}"),
            RaiseTargetDecl::Literal(t) => f.write_str(t),
        }
    }
}

impl std::fmt::Debug for RaiseTargetDecl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.to_string())
    }
}

impl From<RaiseTargetDecl> for String {
    fn from(t: RaiseTargetDecl) -> String {
        t.to_string()
    }
}

const RAISE_SHAPE: &str = "a raise is `{ occasion: <occasion>, target: \"@<attr>\" | \
     <dotted target> }` (`target` only for an occasion raised for a target)";

impl TryFrom<String> for RaiseTargetDecl {
    type Error = SchemaError;
    fn try_from(s: String) -> Result<Self, SchemaError> {
        let t = s.trim();
        if let Some(attr) = t.strip_prefix('@') {
            if crate::ident::is_ident(attr) {
                return Ok(RaiseTargetDecl::Attr(attr.to_string()));
            }
        } else if !t.is_empty() && crate::ident::is_dotted_name(t) {
            return Ok(RaiseTargetDecl::Literal(t.to_string()));
        }
        Err(SchemaError::Raise(format!(
            "effects.raises target `{s}` is neither `@<attr>` nor a dotted target \
             (`scene.chapter2`); {RAISE_SHAPE}"
        )))
    }
}

impl TryFrom<yaml_serde::Value> for RaiseDecl {
    type Error = SchemaError;
    fn try_from(v: yaml_serde::Value) -> Result<Self, SchemaError> {
        use yaml_serde::Value;
        let bad = |what: String| {
            Err(SchemaError::Raise(format!(
                "effects.raises {what}; {RAISE_SHAPE}"
            )))
        };
        let Value::Mapping(m) = &v else {
            return bad(format!("entry `{}` is not a mapping", shown_yaml(&v)));
        };
        let mut occasion = None;
        let mut target = None;
        for (k, val) in m {
            match (k.as_str(), val) {
                (Some("occasion"), Value::String(o)) if crate::ident::is_name(o) => {
                    occasion = Some(o.clone());
                }
                (Some("occasion"), other) => {
                    return bad(format!(
                        "`occasion: {}` is not an occasion name",
                        shown_yaml(other)
                    ));
                }
                (Some("target"), Value::String(t)) => {
                    target = Some(RaiseTargetDecl::try_from(t.clone())?);
                }
                (Some("target"), other) => {
                    return bad(format!("`target: {}` is not a string", shown_yaml(other)));
                }
                (key, _) => {
                    let key = key.map_or_else(|| shown_yaml(k), str::to_string);
                    let hint = crate::suggest::nearest(&key, ["occasion", "target"], 2)
                        .map(|s| format!(" (did you mean `{s}`?)"))
                        .unwrap_or_default();
                    return bad(format!("entry has the unknown key `{key}`{hint}"));
                }
            }
        }
        let Some(occasion) = occasion else {
            return bad("entry names no `occasion:`".to_string());
        };
        Ok(RaiseDecl { occasion, target })
    }
}

/// `effects.raises`: a non-empty list (0.51.0 §4.2) — `raises: []` is
/// refused, not read as absent.
pub(crate) fn raise_list<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<RaiseDecl>, D::Error> {
    let list = Vec::<RaiseDecl>::deserialize(d)?;
    if list.is_empty() {
        return Err(serde::de::Error::custom(SchemaError::Raise(format!(
            "effects.raises is empty; list at least one raise, or remove `raises:` — {RAISE_SHAPE}"
        ))));
    }
    Ok(list)
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
#[serde(untagged, try_from = "yaml_serde::Value")]
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

impl TryFrom<yaml_serde::Value> for WriteValue {
    type Error = SchemaError;

    fn try_from(v: yaml_serde::Value) -> Result<Self, SchemaError> {
        use yaml_serde::Value;
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
                                    .map_or_else(|| "`op:`".into(), |s| format!("`op: {s}`"));
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
                                        "`by:` must be a number or `{ fromAttr: <attr> }`".into()
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
        fields: yaml_serde::Value,
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
    fields: Option<yaml_serde::Value>,
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
