use crate::types::{Field, Literal, PathSegment, Type};
use serde::{Deserialize, Serialize};

// Every struct below that a plugin export file deserializes into denies
// unknown keys (dsl 0.24.0 T1-3): a typo'd key (`selct: all`) or a flow-map
// value split at a comma (`description: Pick one, the player picks one`
// yields a null-valued key `the player picks one`) is an `E-PLUGIN-PARSE`
// naming the key, never a silently defaulted field. The loader
// (`loader::parse_error_msg`) adds the did-you-mean / quoting hint.

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DirectivesFile {
    pub directives: Vec<DirectiveDecl>,
}

/// A `state/*.yaml` export file: `stateShapes:` and/or `stateTemplates:`.
/// At least one must be present (the loader rejects a file with neither).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateFile {
    #[serde(default, rename = "stateShapes")]
    pub state_shapes: Option<Vec<StateShape>>,
    #[serde(default, rename = "stateTemplates")]
    pub state_templates: Option<Vec<StateTemplate>>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvidersFile {
    pub providers: Vec<ProviderDecl>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BridgeFile {
    #[serde(rename = "bridgeCapabilities")]
    pub bridge: Vec<BridgeCapability>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefsFile {
    pub defs: Vec<DefDecl>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrontmatterFile {
    pub frontmatter: Vec<FrontmatterDecl>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrontmatterDecl {
    pub key: String,
    pub schema: Type,
}

/// One `enums:` entry. A bare sequence is shorthand for `{ members: […] }`
/// (dsl 0.9.0 D-D), so every pre-0.9.0 `enums.yaml` keeps parsing byte-for-byte.
/// The long form MAY carry `labels: { <member>: <display text> }` (dsl 0.24.0
/// §1); a non-string label fails this deserialization. Deserialized through
/// [`EnumDecl::try_from`], so a misshapen entry names its fault and the
/// shapes an enum takes rather than serde's "did not match any variant".
#[derive(Clone, Debug, Deserialize)]
#[serde(try_from = "serde_yaml::Value")]
pub enum EnumDecl {
    Members(Vec<String>),
    Long {
        members: Vec<String>,
        #[serde(default)]
        default: Option<String>,
        #[serde(default)]
        exits: Vec<String>,
        #[serde(default)]
        labels: std::collections::BTreeMap<String, String>,
    },
}

impl TryFrom<serde_yaml::Value> for EnumDecl {
    type Error = String;

    fn try_from(v: serde_yaml::Value) -> Result<Self, String> {
        use serde_yaml::Value;
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Long {
            members: Vec<String>,
            #[serde(default)]
            default: Option<String>,
            #[serde(default)]
            exits: Vec<String>,
            #[serde(default)]
            labels: std::collections::BTreeMap<String, String>,
        }
        const SHAPES: &str = "an enum is a list of members `[calm, tense]` or \
             `{ members: [calm, tense], default: calm, exits: [tense], labels: { calm: Calm } }`";
        match &v {
            Value::Sequence(_) => serde_yaml::from_value::<Vec<String>>(v)
                .map(EnumDecl::Members)
                .map_err(|_| format!("lists something that is not a member name; {SHAPES}")),
            Value::Mapping(m) => {
                let keys = crate::entities::ENUM_LONG_FORM_KEYS;
                if let Some(key) = m
                    .keys()
                    .find(|k| !k.as_str().is_some_and(|k| keys.contains(&k)))
                {
                    let key = serde_yaml::to_string(key).unwrap_or_default();
                    let key = key.trim();
                    return Err(format!(
                        "has no key `{key}`{}; {SHAPES}",
                        crate::suggest::did_you_mean(key, keys)
                    ));
                }
                if !m.contains_key("members") {
                    return Err(format!("needs `members:`; {SHAPES}"));
                }
                let long = serde_yaml::from_value::<Long>(v).map_err(|_| {
                    format!(
                        "is misshapen: `members:` and `exits:` list member names and `labels:` \
                         maps each member to its text; {SHAPES}"
                    )
                })?;
                Ok(EnumDecl::Long {
                    members: long.members,
                    default: long.default,
                    exits: long.exits,
                    labels: long.labels,
                })
            }
            _ => Err(format!("is not an enum; {SHAPES}")),
        }
    }
}

impl EnumDecl {
    /// The declared members, in order.
    pub fn members(&self) -> &[String] {
        match self {
            EnumDecl::Members(members) | EnumDecl::Long { members, .. } => members,
        }
    }

    /// Project into the shared [`crate::snapshot::Domain`] shape.
    pub fn into_domain(self) -> crate::snapshot::Domain {
        match self {
            EnumDecl::Members(members) => crate::snapshot::Domain {
                members,
                ..Default::default()
            },
            EnumDecl::Long {
                members,
                default,
                exits,
                labels,
            } => crate::snapshot::Domain {
                members,
                open: false,
                default,
                exits,
                labels,
            },
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnumsFile {
    pub enums: std::collections::BTreeMap<String, EnumDecl>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventsFile {
    pub events: Vec<EventDecl>,
}

/// plugin §14.1 cross-cutting stamp-attribute declaration file (export
/// `stampAttrs/*.yaml`).
///
/// Each entry is an ordinary [`AttrDecl`] — the SAME name/required/type/default
/// surface a directive attr uses — but admissible on EVERY directive AND on
/// content lines rather than on one directive, and lowered into the IR
/// record's stamp (compile-IR §4.3 `Stamp.extra`) instead of the record's own
/// fields. A name colliding with a reserved stamp key (`at`/`duration`/
/// `delay`/`wait`/`timeline`/`provenance`/`source`) is an assembly-time
/// `E-PLUGIN-RESERVED-STAMP-ATTR` (plugin §14, Appendix C4).
///
/// `required` and `default` are inert on this surface by construction: a
/// cross-cutting attr is optional on every record (requiring one would make
/// EVERY directive in the document illegal without it), and an unauthored
/// stamp attr is NOT default-injected — absent means absent, so a document
/// that authors none produces a byte-identical artifact.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StampAttrsFile {
    #[serde(rename = "stampAttrs")]
    pub stamp_attrs: Vec<AttrDecl>,
}

/// plugin `lints/*.yaml` export file (lint-system design §6): a top-level
/// `lints:` sequence of [`crate::lint::LintRuleDecl`]. Mirrors
/// [`StampAttrsFile`]'s shape — the loader reads each file with the same
/// `read_kind` machinery, and a per-package duplicate id is caught by the
/// shared `merge_named` path (key = raw rule id).
///
/// Excluded from [`crate::snapshot::CapabilitySnapshot`] and
/// `capabilityVersion` by construction: neither ever references this
/// file (or [`crate::loader::LoadedPlugin::lints`]), so a plugin can add,
/// remove, or change lints without perturbing artifact identity.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LintsFile {
    pub lints: Vec<crate::lint::LintRuleDecl>,
}

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
/// did before they existed: `capabilityVersion` hashes this `Debug`.
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
    type Error = String;

    fn try_from(s: String) -> Result<Self, String> {
        let bad = |why: &str| {
            Err(format!(
                "effects fact `{s}` {why}; a fact effect is `relation(arg, …)` whose args are \
                 members, `true`/`false`, `@attr` (one of the directive's attrs) or `_`"
            ))
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

/// `capabilityVersion` hashes a directive's `Debug`: a numeric `by` renders
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
    type Error = String;

    fn try_from(v: serde_yaml::Value) -> Result<Self, String> {
        use serde_yaml::Value;
        let bad = |what: String| {
            Err(format!(
                "effects.writes value {what}; a write's value is {WRITE_VALUE_SHAPES}"
            ))
        };
        let attr_name = |v: &Value, key: &str| -> Result<String, String> {
            match v.get("fromAttr") {
                Some(Value::String(a)) if v.as_mapping().is_some_and(|m| m.len() == 1) => {
                    Ok(a.clone())
                }
                _ => Err(format!(
                    "effects.writes value: `{key}` must be `{{ fromAttr: <attr name> }}`"
                )),
            }
        };
        match &v {
            Value::Bool(b) => Ok(WriteValue::Literal(Literal::Bool(*b))),
            Value::Number(n) => match n.as_f64() {
                Some(f) => Ok(WriteValue::Literal(Literal::Num(f))),
                None => bad(format!("`{n}` is not a representable number")),
            },
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
                            Some(Value::Number(n)) if n.as_f64().is_some() => {
                                OpBy::Num(n.as_f64().unwrap_or_default())
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
/// derived `Debug`, which `capabilityVersion` hashes.
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
    "autoStage",
    "cameraTransform",
    "clearStage",
    "end",
    "mark",
    "next",
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
    type Error = String;

    fn try_from(raw: RawLowering) -> Result<Self, String> {
        match raw {
            RawLowering {
                record: Some(record),
                fields,
                kind: None,
                name: None,
            } => match fields {
                Some(fields) => Ok(Lowering::Record { record, fields }),
                None => Err(format!(
                    "`lower: {{ record: {record} }}` needs `fields:` (write `fields: {{}}` for none)"
                )),
            },
            RawLowering {
                record: None,
                fields: None,
                kind: Some(kind),
                name,
            } => {
                if kind != "builtin" {
                    return Err(format!(
                        "`lower.kind` is `{kind}`, but the only kind is `builtin` \
                         (or write `{{ record, fields }}`); {OMIT_LOWER}"
                    ));
                }
                let Some(name) = name else {
                    return Err(format!(
                        "`lower: {{ kind: builtin }}` needs `name:` (one of {}); {OMIT_LOWER}",
                        BUILTIN_LOWERING_HOOKS.join(", ")
                    ));
                };
                if !BUILTIN_LOWERING_HOOKS.contains(&name.as_str()) {
                    let max = (name.chars().count() / 3).clamp(1, 3);
                    let hint = crate::suggest::nearest(&name, BUILTIN_LOWERING_HOOKS.iter().copied(), max)
                        .map(|s| format!(" (did you mean `{s}`?)"))
                        .unwrap_or_default();
                    return Err(format!(
                        "`{name}` is not a builtin lowering hook{hint}; the core registers {}; {OMIT_LOWER}",
                        BUILTIN_LOWERING_HOOKS.join(", ")
                    ));
                }
                Ok(Lowering::Builtin { kind, name })
            }
            RawLowering {
                record: None,
                fields: None,
                kind: None,
                name: None,
            } => Err(format!(
                "`lower:` is empty; write `{{ record, fields }}` or `{{ kind: builtin, name }}`, or {OMIT_LOWER}"
            )),
            _ => Err(format!(
                "`lower:` is either `{{ record, fields }}` or `{{ kind: builtin, name }}`, not a mix; {OMIT_LOWER}"
            )),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateShape {
    pub name: String,
    pub fields: Vec<Field>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateTemplate {
    pub name: String,
    pub scope: String,
    pub path: Vec<PathSegment>,
    pub shape: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderDecl {
    pub name: String,
    #[serde(rename = "idShape", default, skip_serializing_if = "Option::is_none")]
    pub id_shape: Option<String>,
    pub snapshot: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BridgeCapability {
    pub service: String,
    pub operation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replay: Option<String>,
    #[serde(default)]
    pub result: Vec<Field>,
}

/// A single declared def parameter (dsl §8.1). Order-preserving: the position of
/// a `DefParam` in [`DefDecl::params`] is the positional-binding order for
/// `@name(args)` calls.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DefParam {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: Type,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefDecl {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: Type,
    #[serde(default, deserialize_with = "de_params")]
    pub params: Vec<DefParam>,
    pub cel: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<Vec<String>>,
}

/// A capability-declared world event (dsl 0.2.0 §4.5): a named event kind an
/// active plugin makes fireable via `<on event="…">`. Payload (if any) is
/// ordinary plugin `state`, written by the engine before the event fires — NOT
/// part of this declaration. Name is a `CelIdent`-shaped event kind.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventDecl {
    pub name: String,
}

/// plugin §14 / dsl 0.16.0 §4 reward-kind declaration file (export
/// `rewardKinds/*.yaml`).
///
/// A `rewardKinds:` mapping keyed by the kind id; a bare `{}` value declares
/// a shape-only kind (any `<reward kind>` name check passes without a
/// `target` constraint), and a `{ target: { provider: <name> } }` value
/// pins the id-space `target=` resolves against — the `providerRef` pattern
/// the checker already applies to directive-attr `providerRef:` values.
/// The optional `attrs:` sequence is an ordinary [`AttrDecl`] list for
/// game-specific slots (`chance=`, `min`/`max` variants, …) — parsed and
/// carried on the merged [`RewardKindDecl`] so a downstream extra-attr check
/// can key off it; the shape-only checker (Task 2) does not read it yet.
///
/// Per-package duplicate kind id is a [`crate::loader::LoadError::DuplicateId`]
/// (`kind = "rewardKind"`) from the shared `merge_named` path; a name colliding
/// with a peer plugin's kind is [`crate::assemble::AssembleError::DuplicateAcrossPlugins`]
/// (`kind = "rewardKind"`), same shape every other cross-plugin vocabulary
/// duplicate uses.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RewardKindsFile {
    #[serde(rename = "rewardKinds")]
    pub reward_kinds: std::collections::BTreeMap<String, RewardKindBody>,
}

/// The value half of a `rewardKinds:` map entry — the fields on the wire that
/// are NOT the kind id (the id is the map key, materialized onto
/// [`RewardKindDecl::name`] by the loader).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RewardKindBody {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<RewardTarget>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<AttrDecl>,
    /// dsl 0.23.0 §8: the state path a grant of this kind credits.
    #[serde(default)]
    pub credits: Option<String>,
}

/// A reward kind's `target:` contract (dsl 0.16.0 §4, 0.26.0 §2.5): the
/// id-space a `<reward target=…>` value must resolve against. `provider`
/// names one of the active-snapshot providers — the `providerRef` pattern —
/// and assembly rejects a kind that pins a provider no active plugin
/// declares (parallels [`crate::assemble::AssembleError::UnknownAssetKind`]).
/// `entity` names a project entity kind instead (checked against the merged
/// `entities:`). At most one of the two; `required: true` rejects a reward
/// of this kind with no `target=` at all. The checker reports a violation
/// as `E-REWARD-TARGET`.
#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, try_from = "RawRewardTarget")]
pub struct RewardTarget {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub required: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRewardTarget {
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    entity: Option<String>,
    #[serde(default)]
    required: bool,
}

impl TryFrom<RawRewardTarget> for RewardTarget {
    type Error = String;
    fn try_from(raw: RawRewardTarget) -> Result<Self, String> {
        if raw.provider.is_some() && raw.entity.is_some() {
            return Err(
                "a reward kind's `target:` takes `provider:` or `entity:`, not both".to_string(),
            );
        }
        Ok(RewardTarget {
            provider: raw.provider,
            entity: raw.entity,
            required: raw.required,
        })
    }
}

/// Hand-written so a pre-0.26 `{ provider: p }` contract prints exactly as
/// it did (`capabilityVersion` hashes [`RewardKindDecl`]'s `Debug`): only the
/// fields a contract sets are shown.
impl std::fmt::Debug for RewardTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = f.debug_struct("RewardTarget");
        if let Some(p) = &self.provider {
            s.field("provider", p);
        }
        if let Some(e) = &self.entity {
            s.field("entity", e);
        }
        if self.required {
            s.field("required", &true);
        }
        s.finish()
    }
}

/// A capability-declared reward kind (dsl 0.16.0 §4): the vocabulary a
/// plugin's `rewardKinds:` export contributes, consumed by the checker's
/// `E-REWARD-KIND` closure. `name` is the kind id (the map key, materialized
/// onto the struct at load-time); `target` (optional) pins the id space
/// `<reward target=…>` resolves against — the `providerRef` pattern
/// (`assemble` rejects a kind whose provider is absent); `attrs` (optional)
/// carries game-specific extra slots parsed as ordinary [`AttrDecl`]s.
/// `credits` (dsl 0.23.0 §8, optional) names the state path a grant of this
/// kind adds its amount to (`lute run` / `lute play` apply it).
#[derive(Clone, Serialize, Deserialize, Default)]
pub struct RewardKindDecl {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<RewardTarget>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<AttrDecl>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credits: Option<String>,
}

/// Hand-written so a kind without `credits` prints exactly as it did before
/// the field existed: `capabilityVersion` hashes this `Debug`, and a
/// vocabulary that declares no credit path must keep its stamp.
impl std::fmt::Debug for RewardKindDecl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = f.debug_struct("RewardKindDecl");
        s.field("name", &self.name)
            .field("target", &self.target)
            .field("attrs", &self.attrs);
        if let Some(credits) = &self.credits {
            s.field("credits", credits);
        }
        s.finish()
    }
}

/// dsl 0.23.0 §7 cast declaration file (export `cast/*.yaml`, or a schema
/// document's `cast:` key): a `cast:` mapping keyed by the speaker id.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CastFile {
    pub cast: std::collections::BTreeMap<String, CastBody>,
}

/// The value half of a `cast:` map entry (the id is the map key).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CastBody {
    #[serde(default)]
    pub name: Option<String>,
    /// dsl 0.24.0 §4: the CEL condition under which this speaker is present
    /// (`present: "holds(inParty(isolde))"`).
    #[serde(default)]
    pub present: Option<String>,
    /// dsl 0.24.0 §4: the `emotion=` values this speaker takes.
    #[serde(default)]
    pub emotions: Option<Vec<String>>,
    /// dsl 0.24.0 §4: `assume: true` — presence assumes the engine has not
    /// asserted any engine-`reserved` relation `present` negates
    /// (`!holds(fell(isolde))`, directly or through a rule).
    #[serde(default)]
    pub assume: Option<bool>,
    /// Prerelease N6 (dsl 0.26.0 §2.8): `sharedName: true` — this entry's
    /// display name is a role name meant for many speakers ("Eclipse
    /// Grunt"); `W-DISPLAY-NAME-DUP` does not count it.
    #[serde(default, rename = "sharedName")]
    pub shared_name: Option<bool>,
}

impl CastBody {
    /// The member this entry declares under `id`.
    pub fn into_member(self, id: String) -> CastMember {
        CastMember {
            id,
            name: self.name,
            present: self.present,
            emotions: self.emotions,
            assume: self.assume,
            shared_name: self.shared_name,
        }
    }
}

/// One declared cast member (dsl 0.23.0 §7): a speaker id and its display
/// name. When any cast is declared, a speaker outside it is `E-CAST-UNKNOWN`.
/// dsl 0.24.0 §4: `present` — a line by this speaker whose enclosing guards
/// do not imply it is `W-CAST-ABSENT`; `emotions` — the `emotion=` values the
/// speaker takes (`E-BAD-ENUM` outside it); `assume` — presence treats a
/// negated engine-`reserved` relation in `present` as holding;
/// `shared_name` — the display name is an intended role name, exempt from
/// `W-DISPLAY-NAME-DUP` (dsl 0.26.0 §2.8).
#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CastMember {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub present: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emotions: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assume: Option<bool>,
    #[serde(
        default,
        rename = "sharedName",
        skip_serializing_if = "Option::is_none"
    )]
    pub shared_name: Option<bool>,
}

/// Hand-written so a member without `present`/`emotions` prints exactly as
/// it did before those fields existed: `capabilityVersion` hashes this
/// `Debug`, and a cast that declares neither must keep its stamp.
impl std::fmt::Debug for CastMember {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = f.debug_struct("CastMember");
        s.field("id", &self.id).field("name", &self.name);
        if let Some(present) = &self.present {
            s.field("present", present);
        }
        if let Some(emotions) = &self.emotions {
            s.field("emotions", emotions);
        }
        if let Some(assume) = &self.assume {
            s.field("assume", assume);
        }
        if let Some(shared) = &self.shared_name {
            s.field("shared_name", shared);
        }
        s.finish()
    }
}

/// dsl 0.21.0 §2 occasion declaration file (export `occasions/*.yaml`): an
/// `occasions:` mapping keyed by the occasion name — engine vocabulary, the
/// named moments the engine raises and a beat answers with `on:`. A bare
/// `{}` value declares a `select: first`, untargeted occasion.
///
/// Per-package duplicate name is a [`crate::loader::LoadError::DuplicateId`]
/// (`kind = "occasion"`); a name colliding with a peer plugin's occasion is
/// [`crate::assemble::AssembleError::DuplicateAcrossPlugins`] — the
/// `rewardKinds` treatment.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OccasionsFile {
    pub occasions: std::collections::BTreeMap<String, OccasionBody>,
}

/// The value half of an `occasions:` map entry (the name is the map key,
/// materialized onto [`OccasionDecl::name`] by the loader).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OccasionBody {
    #[serde(default)]
    pub select: OccasionSelect,
    #[serde(default)]
    pub target: OccasionTarget,
    #[serde(default)]
    pub description: Option<String>,
    /// dsl 0.24.0 §2: `judge: before` — the occasion's `on=` objectives are
    /// judged before its beats are presented (default `after`).
    #[serde(default)]
    pub judge: OccasionJudge,
    /// dsl 0.27.0 §4 (T2-3): `raisedWhen: "<condition>"` — the engine raises
    /// the occasion only while the condition holds (it may read
    /// `occasion.target`). See [`OccasionDecl::raised_when`].
    #[serde(default, rename = "raisedWhen")]
    pub raised_when: Option<String>,
    /// dsl 0.27.0 §3 (T2-1): `payload: { copies: number }` — typed values the
    /// engine hands over with each raise. See [`OccasionDecl::payload`].
    /// Parsed by [`payload_fields`], so an unknown or incomplete type names
    /// the forms a payload field takes instead of serde's variant wording.
    #[serde(default, deserialize_with = "payload_fields")]
    pub payload: std::collections::BTreeMap<String, crate::types::Type>,
    /// dsl 0.28.0 (T2-9): `outsideRun: true` — the occasion belongs outside
    /// the run (a title screen, a gallery): the engine raises it even after
    /// the project's `terminal:` holds. See [`OccasionDecl::outside_run`].
    #[serde(default, rename = "outsideRun")]
    pub outside_run: bool,
}

/// How the engine presents an occasion's eligible beats (dsl 0.21.0 §2):
/// `first` (default) presents the single winner; `all` offers every
/// eligible beat in selection order; `sequence` (dsl 0.23.0 §3) presents
/// every eligible beat, one after another, in selection order. Appending a
/// variant leaves the `Debug` of `First`/`All` — and so every existing
/// snapshot's `capabilityVersion` — unchanged.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OccasionSelect {
    #[default]
    First,
    All,
    Sequence,
}

impl OccasionSelect {
    /// The declaration spelling: `first`, `all`, or `sequence`.
    pub fn as_str(self) -> &'static str {
        match self {
            OccasionSelect::First => "first",
            OccasionSelect::All => "all",
            OccasionSelect::Sequence => "sequence",
        }
    }
}

/// When an occasion judges its `on=` objectives relative to presenting its
/// beats (dsl 0.24.0 §2): `after` (default, the 0.21 order) or `before`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OccasionJudge {
    #[default]
    After,
    Before,
}

impl OccasionJudge {
    /// `true` for the default `after` (skipped when serialized).
    pub fn is_after(&self) -> bool {
        *self == OccasionJudge::After
    }
}

/// A capability-declared occasion (dsl 0.21.0 §2): the vocabulary a plugin's
/// `occasions:` export contributes, consumed by the checker's
/// `E-OCCASION-UNKNOWN` closure and beat-target / shadowing analysis.
/// A targeted occasion is raised FOR something, so a beat may restrict
/// itself to one target (dsl 0.22.0 §8: optionally from a closed domain).
///
/// `Debug` is written by hand: it prints exactly the derived form of the
/// 0.23 four-field struct and adds `judge` only when it is `before`, so a
/// snapshot that never declares `judge:` keeps its `capabilityVersion` (the
/// hash folds this `Debug`).
#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OccasionDecl {
    pub name: String,
    #[serde(default)]
    pub select: OccasionSelect,
    #[serde(default)]
    pub target: OccasionTarget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// dsl 0.24.0 §2: see [`OccasionJudge`].
    #[serde(default, skip_serializing_if = "OccasionJudge::is_after")]
    pub judge: OccasionJudge,
    /// dsl 0.27.0 §4 (T2-3): the occasion's gate, raw CEL as declared —
    /// the engine raises the occasion only while it holds. The checker
    /// conjoins it into every beat answering the occasion, and `lute play`
    /// refuses a step raising it while it is false (`E-OCCASION-GATE`).
    #[serde(
        default,
        rename = "raisedWhen",
        skip_serializing_if = "Option::is_none"
    )]
    pub raised_when: Option<String>,
    /// dsl 0.27.0 §3 (T2-1): the occasion's typed payload, field -> type.
    /// Beats answering the occasion read a field as
    /// `occasion.payload.<field>` (engine-owned, bound by each raise).
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub payload: std::collections::BTreeMap<String, crate::types::Type>,
    /// dsl 0.28.0 (T2-9): raised even after the project's `terminal:` holds —
    /// the terminal condition ends the run, not what lies outside it.
    #[serde(
        default,
        rename = "outsideRun",
        skip_serializing_if = "std::ops::Not::not"
    )]
    pub outside_run: bool,
}

impl std::fmt::Debug for OccasionDecl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = f.debug_struct("OccasionDecl");
        s.field("name", &self.name)
            .field("select", &self.select)
            .field("target", &self.target)
            .field("description", &self.description);
        if !self.judge.is_after() {
            s.field("judge", &self.judge);
        }
        // dsl 0.27.0 §4: only when declared, so an ungated occasion keeps
        // its `capabilityVersion`.
        if let Some(gate) = &self.raised_when {
            s.field("raised_when", gate);
        }
        // dsl 0.27.0 §3: only when declared (capabilityVersion stability).
        if !self.payload.is_empty() {
            s.field("payload", &self.payload);
        }
        // dsl 0.28.0: only when declared (capabilityVersion stability).
        if self.outside_run {
            s.field("outside_run", &self.outside_run);
        }
        s.finish()
    }
}

/// An occasion's `target:` (dsl 0.21.0 §2, dsl 0.22.0 §8): `true` / `false`
/// (the 0.21 shape-only meaning — any dotted id), or a domain
/// `{ prefix, entity, members? }`: a target is then `<prefix>.<member>` for a
/// member of the project entity kind `entity` — or, with `members`, for one of
/// the listed members only (each of which must belong to `entity`).
///
/// `Debug` prints a shape-only target as the bare bool, exactly as the 0.21
/// `target: bool` field did, so a snapshot that never names a domain keeps
/// its `capabilityVersion` (the hash folds `OccasionDecl`'s `Debug`); a
/// domain without `members` prints exactly as the 0.22 `{ prefix, entity }`
/// did, for the same reason.
///
/// Deserialized through [`OccasionTarget::try_from`] so a typo'd or
/// misshapen target names the bad key and the legal shape, never serde's
/// "did not match any variant".
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged, try_from = "serde_yaml::Value")]
pub enum OccasionTarget {
    Shape(bool),
    Domain {
        prefix: String,
        entity: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        members: Option<Vec<String>>,
    },
}

impl Default for OccasionTarget {
    fn default() -> Self {
        OccasionTarget::Shape(false)
    }
}

impl From<bool> for OccasionTarget {
    fn from(b: bool) -> Self {
        OccasionTarget::Shape(b)
    }
}

const OCCASION_TARGET_SHAPES: &str = "an occasion's `target:` is `true` (any dotted id), or a \
     domain `{ prefix: <id prefix>, entity: <entity kind>, members: [<member>, …] }` \
     (`members` optional)";

impl TryFrom<serde_yaml::Value> for OccasionTarget {
    type Error = String;

    fn try_from(v: serde_yaml::Value) -> Result<Self, String> {
        use serde_yaml::Value;
        let shown = |v: &Value| {
            serde_yaml::to_string(v)
                .unwrap_or_default()
                .trim()
                .to_string()
        };
        let name = |key: &str| -> Result<String, String> {
            match v.get(key) {
                Some(Value::String(s)) if !s.trim().is_empty() => Ok(s.clone()),
                None | Some(Value::Null) => Err(format!(
                    "`target:` names no `{key}:`; {OCCASION_TARGET_SHAPES}"
                )),
                Some(other) => Err(format!(
                    "`target.{key}: {}` is not a name; {OCCASION_TARGET_SHAPES}",
                    shown(other)
                )),
            }
        };
        match &v {
            Value::Bool(b) => Ok(OccasionTarget::Shape(*b)),
            Value::Mapping(m) => {
                const KEYS: [&str; 3] = ["prefix", "entity", "members"];
                if let Some(key) = m
                    .keys()
                    .find(|k| !k.as_str().is_some_and(|k| KEYS.contains(&k)))
                {
                    let key = shown(key);
                    return Err(format!(
                        "`target:` has no key `{key}`{}; {OCCASION_TARGET_SHAPES}",
                        crate::suggest::did_you_mean(&key, KEYS)
                    ));
                }
                let members = match v.get("members") {
                    None => None,
                    Some(Value::Sequence(items)) => Some(
                        items
                            .iter()
                            .map(|i| match i {
                                Value::String(s) => Ok(s.clone()),
                                other => Err(format!(
                                    "`target.members` lists `{}`, which is not a member name",
                                    shown(other)
                                )),
                            })
                            .collect::<Result<Vec<_>, _>>()?,
                    ),
                    Some(other) => {
                        return Err(format!(
                            "`target.members: {}` is not a list — write `members: [<member>, …]`",
                            shown(other)
                        ))
                    }
                };
                Ok(OccasionTarget::Domain {
                    prefix: name("prefix")?,
                    entity: name("entity")?,
                    members,
                })
            }
            Value::Null => Err(format!("`target:` is empty; {OCCASION_TARGET_SHAPES}")),
            other => Err(format!(
                "`target: {}` is not a target; {OCCASION_TARGET_SHAPES}",
                shown(other)
            )),
        }
    }
}

const PAYLOAD_TYPE_FORMS: &str = "a payload field's type is `bool`, `number`, `string`, \
     `{ enum: [<member>, …] }`, `{ domain: <enum or entity kind> }` or `{ entity: <entity kind> }` \
     — a raise gives one value per field";

/// dsl 0.27.0 §3, 0.28.0: an occasion's `payload:` — field -> type, each
/// type one of [`PAYLOAD_TYPE_FORMS`]. A raise (a `lute play` step) gives
/// one literal per field, so a list, record or map type is refused along
/// with an unknown name (with a did-you-mean) or a form missing its
/// argument (`copies: enum`). Each field's type is judged while its value
/// is read, so the error sits at that field's line.
fn payload_fields<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<std::collections::BTreeMap<String, Type>, D::Error> {
    use serde::de::{DeserializeSeed, Error, MapAccess, Visitor};
    use serde_yaml::Value;

    /// One field's type, judged inside the value's own visit.
    struct FieldType<'f>(&'f str);
    impl<'de> DeserializeSeed<'de> for FieldType<'_> {
        type Value = Type;
        fn deserialize<D: serde::Deserializer<'de>>(self, d: D) -> Result<Type, D::Error> {
            d.deserialize_any(self)
        }
    }
    impl<'de> Visitor<'de> for FieldType<'_> {
        type Value = Type;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a payload field's type")
        }
        fn visit_str<E: Error>(self, s: &str) -> Result<Type, E> {
            payload_type(self.0, &Value::String(s.to_string())).map_err(E::custom)
        }
        fn visit_unit<E: Error>(self) -> Result<Type, E> {
            payload_type(self.0, &Value::Null).map_err(E::custom)
        }
        fn visit_bool<E: Error>(self, b: bool) -> Result<Type, E> {
            payload_type(self.0, &Value::Bool(b)).map_err(E::custom)
        }
        fn visit_i64<E: Error>(self, n: i64) -> Result<Type, E> {
            payload_type(self.0, &Value::Number(n.into())).map_err(E::custom)
        }
        fn visit_u64<E: Error>(self, n: u64) -> Result<Type, E> {
            payload_type(self.0, &Value::Number(n.into())).map_err(E::custom)
        }
        fn visit_f64<E: Error>(self, n: f64) -> Result<Type, E> {
            payload_type(self.0, &Value::Number(n.into())).map_err(E::custom)
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, seq: A) -> Result<Type, A::Error> {
            let v = Value::deserialize(serde::de::value::SeqAccessDeserializer::new(seq))?;
            payload_type(self.0, &v).map_err(A::Error::custom)
        }
        fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Type, A::Error> {
            let v = Value::deserialize(serde::de::value::MapAccessDeserializer::new(map))?;
            payload_type(self.0, &v).map_err(A::Error::custom)
        }
    }

    struct Fields;
    impl<'de> Visitor<'de> for Fields {
        type Value = std::collections::BTreeMap<String, Type>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a mapping of each payload field to its type")
        }
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
            let mut out = std::collections::BTreeMap::new();
            while let Some(key) = map.next_key::<Value>()? {
                let Some(field) = key.as_str() else {
                    return Err(A::Error::custom(format!(
                        "`payload:` has the key `{}`, which is not a field name",
                        shown_yaml(&key)
                    )));
                };
                let ty = map.next_value_seed(FieldType(field))?;
                out.insert(field.to_string(), ty);
            }
            Ok(out)
        }
    }
    d.deserialize_map(Fields)
}

/// One payload field's type; `Err` is the author-facing sentence.
fn payload_type(field: &str, v: &serde_yaml::Value) -> Result<Type, String> {
    use serde_yaml::Value;
    const SCALARS: [&str; 3] = ["bool", "number", "string"];
    const FORMS: [&str; 3] = ["enum", "domain", "entity"];
    /// `Type` forms a single literal cannot carry.
    const MULTI: [&str; 3] = ["list", "record", "map"];
    let forms = SCALARS.iter().chain(FORMS.iter()).copied();
    let refuse = |name: &str| -> String {
        if FORMS.contains(&name) {
            let arg = if name == "enum" {
                "[<member>, …]"
            } else {
                "<kind>"
            };
            format!("payload field `{field}` is typed `{name}` without its argument — write `{field}: {{ {name}: {arg} }}`")
        } else if MULTI.contains(&name) {
            format!("payload field `{field}` cannot be a `{name}`; {PAYLOAD_TYPE_FORMS}")
        } else {
            format!(
                "payload field `{field}` has the type `{name}`, which is not a payload type{}; \
                 {PAYLOAD_TYPE_FORMS}",
                crate::suggest::did_you_mean(name, forms.clone())
            )
        }
    };
    match v {
        Value::String(s) => match s.as_str() {
            "bool" => Ok(Type::Bool),
            "number" => Ok(Type::Number),
            "string" => Ok(Type::Str),
            other => Err(refuse(other)),
        },
        Value::Mapping(m) if m.len() == 1 => {
            let (k, arg) = m.iter().next().expect("one entry");
            let Some(name) = k.as_str() else {
                return Err(refuse(&shown_yaml(k)));
            };
            let kind = |form: &str| match arg {
                Value::String(s) if !s.trim().is_empty() => Ok(s.clone()),
                _ => Err(format!(
                    "payload field `{field}`: `{{ {form}: {} }}` names no kind — write \
                     `{{ {form}: <kind> }}`",
                    shown_yaml(arg)
                )),
            };
            match name {
                "enum" => match arg {
                    Value::Sequence(items) if !items.is_empty() => items
                        .iter()
                        .map(|i| match i {
                            Value::String(s) => Ok(s.clone()),
                            other => Err(format!(
                                "payload field `{field}`'s `enum:` lists `{}`, which is not a \
                                 member name",
                                shown_yaml(other)
                            )),
                        })
                        .collect::<Result<Vec<_>, _>>()
                        .map(Type::Enum),
                    _ => Err(format!(
                        "payload field `{field}`'s `enum:` is not a list of members — write \
                         `{field}: {{ enum: [<member>, …] }}`"
                    )),
                },
                "domain" => kind("domain").map(Type::Domain),
                "entity" => kind("entity").map(Type::Entity),
                other if SCALARS.contains(&other) => Err(format!(
                    "payload field `{field}` writes `{{ {other}: … }}` — a `{other}` takes no \
                     argument: write `{field}: {other}`"
                )),
                other => Err(refuse(other)),
            }
        }
        Value::Null => Err(format!(
            "payload field `{field}` names no type; {PAYLOAD_TYPE_FORMS}"
        )),
        other => Err(format!(
            "payload field `{field}: {}` is not a type; {PAYLOAD_TYPE_FORMS}",
            shown_yaml(other)
        )),
    }
}

/// A YAML value as an author wrote it, on one line (`[a, b]`, `3`).
fn shown_yaml(v: &serde_yaml::Value) -> String {
    serde_yaml::to_string(v)
        .unwrap_or_default()
        .trim()
        .replace('\n', " ")
}

impl OccasionTarget {
    /// Whether the occasion is raised for a target at all (`target: true` or
    /// a domain).
    pub fn takes_target(&self) -> bool {
        !matches!(self, OccasionTarget::Shape(false))
    }

    /// The members a domain draws its targets from, given the entity kind's
    /// own closed member list `kind_members` (`None` for an `open:` kind,
    /// whose members are engine-populated): the declared `members` subset
    /// when there is one, else the whole kind. `None` for a shape-only
    /// target, or for a whole open kind. Every consumer that enumerates or
    /// judges a domain's targets goes through this, so a subset narrows all
    /// of them alike.
    pub fn domain_members<'a>(
        &'a self,
        kind_members: Option<&'a [String]>,
    ) -> Option<&'a [String]> {
        match self {
            OccasionTarget::Shape(_) => None,
            OccasionTarget::Domain {
                members: Some(subset),
                ..
            } => Some(subset),
            OccasionTarget::Domain { members: None, .. } => kind_members,
        }
    }
}

impl std::fmt::Debug for OccasionTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OccasionTarget::Shape(b) => std::fmt::Debug::fmt(b, f),
            OccasionTarget::Domain {
                prefix,
                entity,
                members,
            } => {
                let mut d = f.debug_struct("Domain");
                d.field("prefix", prefix).field("entity", entity);
                if let Some(members) = members {
                    d.field("members", members);
                }
                d.finish()
            }
        }
    }
}

/// Deserialize `DefDecl.params` in SOURCE order (dsl §8.1). Accepts either the
/// §8.1 `params:` YAML MAPPING (`{ p: number }`) — read via `serde_yaml::Mapping`,
/// which is insertion-ordered in serde_yaml 0.9.34, so declaration order is
/// preserved for positional arg binding — OR a SEQUENCE of `{ name, type }`
/// entries (the plugin `defs.yaml` list spelling). A malformed mapping entry is
/// skipped, never a panic.
fn de_params<'de, D>(d: D) -> Result<Vec<DefParam>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    use serde::Deserialize;
    Ok(match serde_yaml::Value::deserialize(d)? {
        serde_yaml::Value::Sequence(v) => v
            .into_iter()
            .filter_map(|v| serde_yaml::from_value::<DefParam>(v).ok())
            .collect(),
        serde_yaml::Value::Mapping(m) => m
            .into_iter()
            .filter_map(|(k, v)| {
                let name = k.as_str()?.to_string();
                let ty: Type = serde_yaml::from_value(v).ok()?;
                Some(DefParam { name, ty })
            })
            .collect(),
        _ => {
            return Err(D::Error::custom(
                "`params:` maps each parameter to its type (`params: { n: number }`) or lists \
                 `{ name, type }` entries",
            ))
        }
    })
}

/// plugin §5 manifest entry.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginManifest {
    pub id: String,
    pub version: String,
    pub kind: String,
    #[serde(default)]
    pub depends: Vec<Depends>,
    pub exports: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub options: Vec<OptionDecl>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Depends {
    pub id: String,
    pub range: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OptionDecl {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: Type,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Literal>,
}

/// plugin §6.9 asset-kind declaration (export file `assetKinds/*.yaml`).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssetKindDecl {
    pub kind: String,
    #[serde(default = "default_sep")]
    pub sep: String,
    #[serde(default)]
    pub resolve: AssetResolve,
    #[serde(default)]
    pub segments: Vec<AssetSegment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, rename = "match")]
    pub match_: Vec<AssetMatch>,
    #[serde(default)]
    pub aliases: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub fallback: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persistence: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum AssetResolve {
    #[default]
    Compose,
    Query,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssetSegment {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#const: Option<String>,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub ty: Option<Type>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssetMatch {
    pub attr: String,
    pub field: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub via: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetKindsFile {
    #[serde(rename = "assetKinds")]
    pub asset_kinds: Vec<AssetKindDecl>,
}

fn default_sep() -> String {
    ".".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIGAME_DIR: &str = r#"
directives:
  - name: minigame
    layer: bridge
    attrs:
      - { name: kind, required: true, type: { enumFromOption: allowedKinds } }
      - { name: id, required: true, type: { providerRef: minigameId } }
      - { name: wait, type: bool, default: true }
    semantics: [ "writes.sceneState", "bridgeCall" ]
    bridge: { service: minigame, operation: play }
    lower: { kind: builtin, name: autoStage }
"#;

    #[test]
    fn parses_directive_with_attrs_and_lower() {
        let file: DirectivesFile = serde_yaml::from_str(MINIGAME_DIR).unwrap();
        let d = &file.directives[0];
        assert_eq!(d.name, "minigame");
        assert_eq!(d.attrs.len(), 3);
        assert!(d.attrs[0].required);
        assert!(matches!(d.lower, Lowering::Builtin { .. }));
    }

    #[test]
    fn state_shape_field_defaults_are_typed() {
        let y = r#"
stateShapes:
  - name: minigameResult
    fields:
      - { name: rank, type: { enum: [fail, gold] }, default: fail }
"#;
        let f: StateFile = serde_yaml::from_str(y).unwrap();
        assert_eq!(f.state_shapes.unwrap()[0].fields[0].name, "rank");
    }
    #[test]
    fn write_value_untagged_variants_bind() {
        let y = r#"
writes:
  - { scope: scene, path: [minigame, rank], value: { fromBridgeResult: rank } }
  - { scope: scene, path: [minigame, attempts], value: { op: increment, by: 1 } }
  - { scope: scene, path: [flags, done], value: true }
"#;
        let e: DirectiveEffects = serde_yaml::from_str(y).unwrap();
        assert!(matches!(
            e.writes[0].value,
            WriteValue::FromBridgeResult { .. }
        ));
        assert!(matches!(e.writes[1].value, WriteValue::Op { .. }));
        assert!(matches!(e.writes[2].value, WriteValue::Literal(_)));
    }

    #[test]
    fn fact_effects_parse_and_keep_the_debug_of_a_writes_only_block() {
        // dsl 0.27.0 §4: `writes` is optional, facts are `rel(arg, …)`.
        let e: DirectiveEffects =
            serde_yaml::from_str("asserts: [\"holding(@item)\", \"seen(key, true)\"]").unwrap();
        assert!(e.writes.is_empty());
        assert_eq!(e.asserts[0].relation, "holding");
        assert_eq!(e.asserts[0].args, vec![FactEffectArg::Attr("item".into())]);
        assert_eq!(e.asserts[1].to_string(), "seen(key, true)");
        // capabilityVersion hashes `Debug`: a block without facts prints as
        // it did before `asserts`/`retracts` existed.
        let w: DirectiveEffects = serde_yaml::from_str(
            "writes: [ { scope: run, path: [sanity], value: { op: increment, by: -1 } } ]",
        )
        .unwrap();
        let dbg = format!("{w:?}");
        assert!(dbg.starts_with("DirectiveEffects { writes: ["), "{dbg}");
        assert!(
            !dbg.contains("asserts") && !dbg.contains("retracts"),
            "{dbg}"
        );
        assert!(format!("{e:?}").contains("asserts: [\"holding(@item)\""));
        // …and serializes without the empty lists.
        let back = serde_yaml::to_string(&w).unwrap();
        assert!(!back.contains("asserts"), "{back}");
    }

    #[test]
    fn malformed_fact_effects_are_refused_naming_the_shape() {
        for bad in [
            "holding",
            "holding(@)",
            "holding(a b)",
            "(item)",
            "holding(\"x\")",
        ] {
            let err = serde_yaml::from_str::<DirectiveEffects>(&format!("asserts: ['{bad}']"))
                .unwrap_err()
                .to_string();
            assert!(
                err.contains("a fact effect is `relation(arg, …)`"),
                "{bad}: {err}"
            );
        }
        let err = serde_yaml::from_str::<DirectiveEffects>("asserts: [ { holding: item } ]")
            .unwrap_err()
            .to_string();
        assert!(err.contains("expected a string"), "{err}");
    }

    #[test]
    fn lowering_record_form_binds() {
        let y = "record: setBackground\nfields: {}";
        let l: Lowering = serde_yaml::from_str(y).unwrap();
        assert!(matches!(l, Lowering::Record { .. }));
    }

    #[test]
    fn absent_lower_is_the_generic_passthrough() {
        let y = "directives:\n  - { name: encounter, attrs: [ { name: id, type: string } ] }\n";
        let file: DirectivesFile = serde_yaml::from_str(y).unwrap();
        assert!(file.directives[0].lower.is_passthrough());
        // …and it serializes as no `lower:` at all.
        let back = serde_yaml::to_string(&file.directives[0]).unwrap();
        assert!(!back.contains("lower"), "{back}");
    }

    #[test]
    fn unregistered_builtin_hook_is_rejected_naming_the_registry() {
        let err = serde_yaml::from_str::<Lowering>("{ kind: builtin, name: encounter }")
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("`encounter` is not a builtin lowering hook"),
            "{err}"
        );
        assert!(err.contains(&BUILTIN_LOWERING_HOOKS.join(", ")), "{err}");
        assert!(err.contains("omit `lower:`"), "{err}");
        let err = serde_yaml::from_str::<Lowering>("{ kind: builtin, name: autoStag }")
            .unwrap_err()
            .to_string();
        assert!(err.contains("did you mean `autoStage`?"), "{err}");
    }

    #[test]
    fn malformed_lowering_shapes_are_named() {
        for (y, want) in [
            (
                "{ kind: record, name: background }",
                "the only kind is `builtin`",
            ),
            ("{ kind: builtin }", "needs `name:`"),
            ("{ record: background }", "needs `fields:`"),
            (
                "{ record: background, fields: {}, kind: builtin, name: end }",
                "not a mix",
            ),
            ("{}", "`lower:` is empty"),
            (
                "{ record: background, feilds: {} }",
                "unknown field `feilds`",
            ),
        ] {
            let err = serde_yaml::from_str::<Lowering>(y).unwrap_err().to_string();
            assert!(err.contains(want), "{y}: {err}");
        }
    }

    #[test]
    fn builtin_hook_registry_is_exactly_the_core_hooks() {
        // The registry is the set of hooks the embedded `lute.core` manifest
        // names; a hook added to one without the other fails here.
        let core: DirectivesFile =
            serde_yaml::from_str(include_str!("../assets/lute.core/directives/staging.yaml"))
                .unwrap();
        let mut named: Vec<String> = core
            .directives
            .iter()
            .filter_map(|d| match &d.lower {
                Lowering::Builtin { name, .. } => Some(name.clone()),
                _ => None,
            })
            .collect();
        named.sort();
        assert_eq!(named, BUILTIN_LOWERING_HOOKS);
    }

    #[test]
    fn export_bodies_reject_unknown_keys() {
        let occ = serde_yaml::from_str::<OccasionsFile>("occasions:\n  report: { selct: all }\n")
            .unwrap_err()
            .to_string();
        assert!(occ.contains("unknown field `selct`"), "{occ}");
        let rk =
            serde_yaml::from_str::<RewardKindsFile>("rewardKinds:\n  gold: { credit: run.gold }\n")
                .unwrap_err()
                .to_string();
        assert!(rk.contains("unknown field `credit`"), "{rk}");
        let dir = serde_yaml::from_str::<DirectivesFile>(
            "directives:\n  - { name: x, attrs: [ { name: a, type: string, requird: true } ] }\n",
        )
        .unwrap_err()
        .to_string();
        assert!(dir.contains("unknown field `requird`"), "{dir}");
        let target = serde_yaml::from_str::<OccasionsFile>(
            "occasions:\n  talk: { target: { prefix: npc, entity: npc, member: [a] } }\n",
        );
        assert!(target.is_err(), "a typo'd target domain key must not parse");
    }

    #[test]
    fn plugin_manifest_parses_spec_entry() {
        let y = r#"
id: arcia.minigame
version: 0.1.0
kind: capability
depends: [ { id: lute.core, range: "^0.0.1" } ]
exports: { directives: directives/, state: state/ }
options:
  - { name: resultScope, type: { enum: [scene, run] }, default: scene }
"#;
        let m: PluginManifest = serde_yaml::from_str(y).unwrap();
        assert_eq!(m.id, "arcia.minigame");
        assert_eq!(m.kind, "capability");
        assert_eq!(m.depends.len(), 1);
        assert_eq!(m.options[0].name, "resultScope");
    }

    #[test]
    fn asset_kind_decl_parses_ch() {
        let y = r#"
assetKinds:
  - kind: CH
    sep: "."
    segments:
      - { name: prefix,      const: CH }
      - { name: characterId, type: { providerRef: character } }
      - { name: costume,     type: string }
      - { name: emotion,     type: { enum: [delighted, content, neutral] } }
      - { name: variant,     type: number }
    fallback: [emotionGroup, neutral, variant0]
    persistence: scene
"#;
        let file: AssetKindsFile = serde_yaml::from_str(y).unwrap();
        let d = &file.asset_kinds[0];
        assert_eq!(d.kind, "CH");
        assert_eq!(d.sep, ".");
        assert_eq!(d.resolve, AssetResolve::Compose);
        assert_eq!(d.segments.len(), 5);
        assert_eq!(
            d.segments
                .iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>(),
            ["prefix", "characterId", "costume", "emotion", "variant"]
        );
        assert_eq!(d.segments[0].r#const.as_deref(), Some("CH"));
        assert_eq!(d.segments[0].ty, None);
        assert_eq!(
            d.segments[1].ty,
            Some(Type::ProviderRef("character".into()))
        );
        assert_eq!(d.segments[2].ty, Some(Type::Str));
        assert_eq!(
            d.segments[3].ty,
            Some(Type::Enum(vec![
                "delighted".into(),
                "content".into(),
                "neutral".into(),
            ]))
        );
        assert_eq!(d.segments[4].ty, Some(Type::Number));
        assert_eq!(d.fallback, ["emotionGroup", "neutral", "variant0"]);
        assert_eq!(d.persistence.as_deref(), Some("scene"));
    }

    #[test]
    fn asset_kind_decl_parses_bg_query() {
        let y = r#"
assetKinds:
  - kind: BG
    resolve: query
    provider: backgrounds
    aliases: { location: locationAlias }
    match: [ { attr: location, field: spaceId, via: locationAlias },
             { attr: time, field: timeOfDay }, { attr: view, field: view },
             { attr: variation, field: variation } ]
    fallback: [ dropVariation, areaKind, preferAfternoon, anyView ]
"#;
        let file: AssetKindsFile = serde_yaml::from_str(y).unwrap();
        let d = &file.asset_kinds[0];
        assert_eq!(d.kind, "BG");
        assert_eq!(d.resolve, AssetResolve::Query);
        assert_eq!(d.provider.as_deref(), Some("backgrounds"));
        assert_eq!(d.match_.len(), 4);
        assert_eq!(d.match_[0].attr, "location");
        assert_eq!(d.match_[0].field, "spaceId");
        assert_eq!(d.match_[0].via.as_deref(), Some("locationAlias"));
        assert_eq!(d.match_[1].via, None);
        assert_eq!(d.aliases["location"], "locationAlias");
        assert_eq!(
            d.fallback,
            ["dropVariation", "areaKind", "preferAfternoon", "anyView"]
        );
    }

    #[test]
    fn def_params_mapping_deserializes_in_source_order() {
        // §8.1 `params:` MAPPING spelling — order MUST be preserved for positional
        // arg binding (serde_yaml::Mapping is insertion-ordered).
        let src = "defs:\n  - name: pair\n    type: bool\n    cel: \"true\"\n    params: { a: number, b: bool }\n";
        let file: DefsFile = serde_yaml::from_str(src).unwrap();
        let d = &file.defs[0];
        assert_eq!(
            d.params,
            vec![
                DefParam {
                    name: "a".into(),
                    ty: Type::Number
                },
                DefParam {
                    name: "b".into(),
                    ty: Type::Bool
                },
            ]
        );
    }

    #[test]
    fn def_params_sequence_spelling_deserializes() {
        // The plugin `defs.yaml` list spelling `[{ name, type }]` also works.
        let src = "defs:\n  - name: pair\n    type: bool\n    cel: \"true\"\n    params:\n      - { name: a, type: number }\n      - { name: b, type: bool }\n";
        let file: DefsFile = serde_yaml::from_str(src).unwrap();
        assert_eq!(
            file.defs[0].params,
            vec![
                DefParam {
                    name: "a".into(),
                    ty: Type::Number
                },
                DefParam {
                    name: "b".into(),
                    ty: Type::Bool
                },
            ]
        );
    }

    #[test]
    fn def_params_sequence_skips_malformed_entry() {
        // §8.1 SEQUENCE spelling MUST be fail-soft: one malformed entry (here
        // missing `type`) is skipped, not fatal — the file still keeps its good
        // params rather than being rejected wholesale (mirrors the MAPPING path).
        let src = "defs:\n  - name: pair\n    type: bool\n    cel: \"true\"\n    params:\n      - { name: a, type: number }\n      - { name: b }\n";
        let file: DefsFile = serde_yaml::from_str(src).unwrap();
        assert_eq!(
            file.defs[0].params,
            vec![DefParam {
                name: "a".into(),
                ty: Type::Number
            }]
        );
    }

    #[test]
    fn def_params_absent_yields_empty() {
        let src = "defs:\n  - name: bare\n    type: bool\n    cel: \"true\"\n";
        let file: DefsFile = serde_yaml::from_str(src).unwrap();
        assert!(file.defs[0].params.is_empty());
    }
}
