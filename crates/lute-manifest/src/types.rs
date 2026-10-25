use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// plugin §7 Type. Serde uses YAML's tagged-map forms
/// ({ enum: [...] }, { list: T }, { providerRef: name }, ...).
///
/// yaml_serde serializes externally-tagged enums as YAML `!tags`, not the
/// single-key maps the spec mandates, so `Type` (de)serializes through
/// `yaml_serde::with::singleton_map_recursive` via the private `TypeDef` shadow
/// below. The public shape here is authoritative for all manifest tasks.
#[derive(Clone, Debug, PartialEq)]
pub enum Type {
    Bool,
    Int,
    Double,
    Str,
    Enum(Vec<String>),
    List(Box<Type>),
    Record(Vec<Field>),
    Map {
        key: Box<Type>,
        value: Box<Type>,
    },
    EnumFromOption(String), // attribute types only
    ProviderRef(String),    // any typed position
    Domain(String),         // any typed position; membership checked at check-stage
    /// dsl 0.26.0 §2.5: `{ entity: <kind> }` — a member of a project entity
    /// kind (`entities:`). Attribute types only; membership (with a
    /// did-you-mean) is checked at check-stage against the merged kinds.
    Entity(String),
    SlotId {
        namespace: String,
    }, // attribute types only
    AssetKind(String), // attribute types only
    /// dsl 0.3.0 §6, D11: opaque, ordering-only narrative-time value. NEVER
    /// author-declarable state (an author `state:`/schema-doc decl of it is
    /// `E-TEMPORAL-ARG` at the decl, `lute-check/src/meta.rs`) — this variant
    /// exists only so a plugin capability's `state_shapes` export (the
    /// engine->state contract) can surface an anchor path. No `Literal`
    /// inhabits it (`type_accepts` below always returns `false`).
    NarrativeTime,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Field {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: Type,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Literal>,
    #[serde(default)]
    pub required: bool,
    /// state-shape fields MAY use `shape: <name>` instead of an inline type; attr types MAY NOT.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Literal {
    Bool(bool),
    Int(i64),
    Double(f64),
    Str(String),
    List(Vec<Literal>),
    /// A record/map literal (plugin §7): a YAML mapping. Deterministic order.
    Map(std::collections::BTreeMap<String, Literal>),
}

impl Literal {
    /// Convert a YAML value (e.g. a scene `plugins:` option) into a `Literal`.
    pub fn from_yaml(v: &yaml_serde::Value) -> Option<Literal> {
        use yaml_serde::Value;
        match v {
            Value::Bool(b) => Some(Literal::Bool(*b)),
            Value::Number(n) => n
                .as_i64()
                .map(Literal::Int)
                .or_else(|| n.as_f64().map(Literal::Double)),
            Value::String(s) => Some(Literal::Str(s.clone())),
            Value::Sequence(items) => items
                .iter()
                .map(Literal::from_yaml)
                .collect::<Option<Vec<_>>>()
                .map(Literal::List),
            Value::Mapping(m) => {
                let mut out = std::collections::BTreeMap::new();
                for (k, val) in m {
                    let key = k.as_str()?.to_string();
                    out.insert(key, Literal::from_yaml(val)?);
                }
                Some(Literal::Map(out))
            }
            Value::Null | Value::Tagged(_) => None,
        }
    }
}

/// Who writes a declared state path (dsl 0.22.0 §1.2). Only `engine` is
/// spellable (`owner: engine`); an absent `owner:` means content may write it.
/// An engine-owned path is written by the engine at runtime and by `engine:`
/// play steps / trace mocks in the toolchain — a content `::set` of it is
/// `E-ENGINE-OWNED-WRITE`. Reads are unrestricted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Owner {
    Engine,
}

impl Owner {
    /// The YAML spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Owner::Engine => "engine",
        }
    }
}

/// plugin §7.4 structured path segment.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PathSegment {
    Literal(String),
    FromAttr {
        #[serde(rename = "fromAttr")]
        from_attr: FromAttr,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FromAttr {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot_type: Option<String>,
}

pub fn type_accepts(ty: &Type, lit: &Literal) -> bool {
    match (ty, lit) {
        (Type::Bool, Literal::Bool(_)) => true,
        (Type::Int, Literal::Int(_)) => true,
        (Type::Double, Literal::Double(_)) => true,
        (Type::Str, Literal::Str(_)) => true,
        (Type::Enum(members), Literal::Str(s)) => members.iter().any(|m| m == s),
        (Type::List(inner), Literal::List(items)) => items.iter().all(|i| type_accepts(inner, i)),
        (Type::Record(fields), Literal::Map(m)) => {
            // every required field present + typed; no unknown keys.
            fields.iter().all(|f| match m.get(&f.name) {
                Some(v) => field_type_accepts(f, v),
                None => !f.required,
            }) && m.keys().all(|k| fields.iter().any(|f| &f.name == k))
        }
        (Type::Map { key, value }, Literal::Map(m)) => {
            // keys are strings in YAML; enum-typed keys checked against members.
            matches!(**key, Type::Str | Type::Enum(_))
                && m.iter()
                    .all(|(k, v)| key_accepts(key, k) && type_accepts(value, v))
        }
        // A `slotId`-typed attribute value is a bare local identifier string
        // (e.g. `resultKey="service01"`), which opens a typed state slot in the
        // declared namespace (plugin §8). Structurally it is a string; the slot
        // path expansion (Task 6.1) consumes the identifier downstream.
        (Type::SlotId { .. }, Literal::Str(_)) => true,
        // An `assetKind`-typed attribute value is an authored asset-id string
        // (e.g. a `CH` id); structural decompose/validate is the checker's job.
        (Type::AssetKind(_), Literal::Str(_)) => true,
        // A `domain`-typed value is a named reference into the merged
        // vocabulary; structurally any string, membership is a check-stage
        // concern (mirrors `assetKind`/`providerRef`).
        (Type::Domain(_), Literal::Str(_)) => true,
        (Type::Entity(_), Literal::Str(_)) => true,
        // dsl 0.3.0 §6: narrative-time is opaque — no `Literal` ever
        // inhabits it (not a bool, number, string, or anything else).
        (Type::NarrativeTime, _) => false,
        _ => false,
    }
}

/// A record field MAY use `shape: <name>` instead of an inline type; a shape
/// reference is validated at snapshot-assembly time (the shape registry is not
/// available here), so a `Map` literal against a shape field is accepted here.
fn field_type_accepts(f: &Field, v: &Literal) -> bool {
    if f.shape.is_some() {
        matches!(v, Literal::Map(_))
    } else {
        type_accepts(&f.ty, v)
    }
}

fn key_accepts(key: &Type, k: &str) -> bool {
    match key {
        Type::Str => true,
        Type::Enum(members) => members.iter().any(|m| m == k),
        _ => false,
    }
}

/// Render a [`Type`] for a DIAGNOSTIC message (plugin §7 spellings).
pub fn type_str(ty: &Type) -> String {
    match ty {
        Type::Bool => "bool".to_string(),
        Type::Int => "int".to_string(),
        Type::Double => "double".to_string(),
        Type::Str => "string".to_string(),
        Type::Enum(members) => format!("enum({})", members.join("|")),
        Type::List(inner) => format!("list<{}>", type_str(inner)),
        Type::Record(fields) => format!(
            "record{{{}}}",
            fields
                .iter()
                .map(|f| f.name.as_str())
                .collect::<Vec<_>>()
                .join(",")
        ),
        Type::Map { key, value } => format!("map<{},{}>", type_str(key), type_str(value)),
        Type::EnumFromOption(opt) => format!("enumFromOption:{opt}"),
        Type::ProviderRef(name) => format!("providerRef:{name}"),
        Type::Domain(name) => format!("domain:{name}"),
        Type::Entity(kind) => format!("entity:{kind}"),
        Type::SlotId { namespace } => format!("slotId:{namespace}"),
        Type::AssetKind(name) => format!("assetKind:{name}"),
        Type::NarrativeTime => "narrativeTime".to_string(),
    }
}

/// Render a [`Literal`] for a DIAGNOSTIC message.
pub fn lit_str(lit: &Literal) -> String {
    match lit {
        Literal::Bool(b) => b.to_string(),
        Literal::Int(n) => n.to_string(),
        Literal::Double(n) => n.to_string(),
        Literal::Str(s) => format!("\"{s}\""),
        Literal::List(items) => format!(
            "[{}]",
            items.iter().map(lit_str).collect::<Vec<_>>().join(", ")
        ),
        Literal::Map(m) => format!(
            "{{{}}}",
            m.iter()
                .map(|(k, v)| format!("{k}: {}", lit_str(v)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

// --- serde representation --------------------------------------------------
//
// `TypeDef`/`FieldDef` mirror the public types with the spec's serde attributes.
// They are fully self-contained so `singleton_map_recursive` wraps exactly once
// per public `Type` boundary and handles every nested form
// ({ list: { providerRef: x } }, { map: { key, value } }, { slotId: { namespace } }).

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum TypeDef {
    Bool,
    Int,
    Double,
    #[serde(rename = "string")]
    Str,
    Enum(Vec<String>),
    List(Box<TypeDef>),
    Record(Vec<FieldDef>),
    Map {
        key: Box<TypeDef>,
        value: Box<TypeDef>,
    },
    EnumFromOption(String),
    ProviderRef(String),
    Domain(String),
    Entity(String),
    SlotId {
        namespace: String,
    },
    AssetKind(String),
    NarrativeTime,
}

#[derive(Serialize, Deserialize)]
struct FieldDef {
    name: String,
    #[serde(rename = "type")]
    ty: TypeDef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    default: Option<Literal>,
    #[serde(default)]
    required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    shape: Option<String>,
}

impl From<TypeDef> for Type {
    fn from(d: TypeDef) -> Self {
        match d {
            TypeDef::Bool => Type::Bool,
            TypeDef::Int => Type::Int,
            TypeDef::Double => Type::Double,
            TypeDef::Str => Type::Str,
            TypeDef::Enum(m) => Type::Enum(m),
            TypeDef::List(inner) => Type::List(Box::new((*inner).into())),
            TypeDef::Record(fields) => Type::Record(fields.into_iter().map(Field::from).collect()),
            TypeDef::Map { key, value } => Type::Map {
                key: Box::new((*key).into()),
                value: Box::new((*value).into()),
            },
            TypeDef::EnumFromOption(s) => Type::EnumFromOption(s),
            TypeDef::ProviderRef(s) => Type::ProviderRef(s),
            TypeDef::Domain(s) => Type::Domain(s),
            TypeDef::Entity(s) => Type::Entity(s),
            TypeDef::SlotId { namespace } => Type::SlotId { namespace },
            TypeDef::AssetKind(s) => Type::AssetKind(s),
            TypeDef::NarrativeTime => Type::NarrativeTime,
        }
    }
}

impl From<&Type> for TypeDef {
    fn from(t: &Type) -> Self {
        match t {
            Type::Bool => TypeDef::Bool,
            Type::Int => TypeDef::Int,
            Type::Double => TypeDef::Double,
            Type::Str => TypeDef::Str,
            Type::Enum(m) => TypeDef::Enum(m.clone()),
            Type::List(inner) => TypeDef::List(Box::new((&**inner).into())),
            Type::Record(fields) => TypeDef::Record(fields.iter().map(FieldDef::from).collect()),
            Type::Map { key, value } => TypeDef::Map {
                key: Box::new((&**key).into()),
                value: Box::new((&**value).into()),
            },
            Type::EnumFromOption(s) => TypeDef::EnumFromOption(s.clone()),
            Type::ProviderRef(s) => TypeDef::ProviderRef(s.clone()),
            Type::Domain(s) => TypeDef::Domain(s.clone()),
            Type::Entity(s) => TypeDef::Entity(s.clone()),
            Type::SlotId { namespace } => TypeDef::SlotId {
                namespace: namespace.clone(),
            },
            Type::AssetKind(s) => TypeDef::AssetKind(s.clone()),
            Type::NarrativeTime => TypeDef::NarrativeTime,
        }
    }
}

impl From<FieldDef> for Field {
    fn from(d: FieldDef) -> Self {
        Field {
            name: d.name,
            ty: d.ty.into(),
            default: d.default,
            required: d.required,
            shape: d.shape,
        }
    }
}

impl From<&Field> for FieldDef {
    fn from(f: &Field) -> Self {
        FieldDef {
            name: f.name.clone(),
            ty: (&f.ty).into(),
            default: f.default.clone(),
            required: f.required,
            shape: f.shape.clone(),
        }
    }
}

impl Serialize for Type {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        yaml_serde::with::singleton_map_recursive::serialize(&TypeDef::from(self), serializer)
    }
}

impl<'de> Deserialize<'de> for Type {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = yaml_serde::Value::deserialize(deserializer)?;
        if value.as_str() == Some("number") {
            return Err(serde::de::Error::custom(NUMBER_TYPE_REMOVED));
        }
        let def: TypeDef = yaml_serde::with::singleton_map_recursive::deserialize(value)
            .map_err(serde::de::Error::custom)?;
        Ok(def.into())
    }
}

/// 0.41.0 §4.4: the serialized form above, as `fieldType` — a bare
/// spelling, or a single-key map whose key names the form. A nested type
/// (`list`, `map`, a `record` field's `type`) is a `$ref` to `fieldType`.
#[cfg(feature = "json-schema")]
impl schemars::JsonSchema for Type {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "fieldType".into()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let ty = generator.subschema_for::<Type>();
        let one = |key: &str, value: serde_json::Value| {
            serde_json::json!({
                "type": "object",
                "properties": { key: value },
                "required": [key],
                "additionalProperties": false
            })
        };
        let name = serde_json::json!({ "type": "string" });
        schemars::json_schema!({
            "oneOf": [
                { "enum": ["bool", "int", "double", "string", "narrativeTime"] },
                one("enum", serde_json::json!({ "type": "array", "items": { "type": "string" } })),
                one("list", ty.clone().into()),
                one("record", serde_json::json!({
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string" },
                            "type": ty,
                            "default": { "description": "the field's default literal (any JSON value but null)." },
                            "required": { "type": "boolean" },
                            "shape": { "type": "string" }
                        },
                        "required": ["name", "type", "required"],
                        "additionalProperties": false
                    }
                })),
                one("map", serde_json::json!({
                    "type": "object",
                    "properties": { "key": ty, "value": ty },
                    "required": ["key", "value"],
                    "additionalProperties": false
                })),
                one("enumFromOption", name.clone()),
                one("providerRef", name.clone()),
                one("domain", name.clone()),
                one("entity", name.clone()),
                one("slotId", serde_json::json!({
                    "type": "object",
                    "properties": { "namespace": { "type": "string" } },
                    "required": ["namespace"],
                    "additionalProperties": false
                })),
                one("assetKind", name)
            ],
            "description": "A declared value type (plugin §7) as the IR serializes it: a bare spelling (`bool`, `int`, `double`, `string`, `narrativeTime`) or a single-key map naming the form (`{\"enum\": [...]}`, `{\"list\": T}`, `{\"record\": [fields]}`, `{\"map\": {\"key\": T, \"value\": T}}`, `{\"domain\": name}`, `{\"entity\": kind}`, `{\"enumFromOption\": option}`, `{\"providerRef\": name}`, `{\"slotId\": {\"namespace\": ns}}`, `{\"assetKind\": kind}`)."
        })
    }
}

/// The sentence a `type: number` that reaches a `Type` decoder carries — the
/// fallback for a surface no [`number_type_sites`] scan covers. Every
/// declaration surface Lute reads scans first and reports [`E_TYPE_NUMBER`].
pub const NUMBER_TYPE_REMOVED: &str =
    "`type: number` is no longer a numeric type — choose `type: int` for whole numbers or `type: double` for fractional values";

/// 0.41.0 §3.2: a declaration typed `number`, the type 0.31 removed.
pub const E_TYPE_NUMBER: &str = "E-TYPE-NUMBER";

/// [`E_TYPE_NUMBER`]'s message. `seen` is every literal value the checker
/// sees the declaration take (its own `default:`, and for a state row the
/// literal writes of its path); when each one is whole the message suggests
/// `int`, listing at most three distinct values. No values: no suggestion.
#[expect(
    clippy::cast_possible_truncation,
    reason = "a whole authored number: past the i64 range `as` saturates, as before"
)]
pub fn number_type_message(seen: &[f64]) -> String {
    let whole = !seen.is_empty() && seen.iter().all(|v| v.is_finite() && v.fract() == 0.0);
    if !whole {
        return "`type: number` is no longer a numeric type — choose `type: int` for whole numbers or `type: double` for fractional values".to_string();
    }
    let mut shown: Vec<String> = Vec::new();
    for v in seen {
        let v = format!("{}", *v as i64);
        if !shown.contains(&v) {
            shown.push(v);
        }
    }
    let more = if shown.len() > 3 { ", …" } else { "" };
    shown.truncate(3);
    let values = shown.join(", ");
    format!(
        "`type: number` is no longer a numeric type — choose `type: int` for whole numbers or `type: double` for fractional values; every value it takes here is whole ({values}{more}), so `int` fits"
    )
}

/// One step from a YAML document's root towards a node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum YamlKey {
    Key(String),
    Item(usize),
}

impl YamlKey {
    /// The [`crate::yaml_text::YamlStep`] this step is.
    pub fn step(&self) -> crate::yaml_text::YamlStep<'_> {
        match self {
            YamlKey::Key(k) => crate::yaml_text::YamlStep::Key(k),
            YamlKey::Item(i) => crate::yaml_text::YamlStep::Item(*i),
        }
    }
}

/// A `number` written where a type is declared (0.41.0 §3.2).
#[derive(Clone, Debug, PartialEq)]
pub struct NumberSite {
    /// From the scanned value's root to the `number` scalar: the value of a
    /// `type:` key, or of a key in a shorthand `params:` / `payload:` map.
    pub path: Vec<YamlKey>,
    /// The declaration's own `default:`, when it is a number.
    pub default: Option<f64>,
}

impl NumberSite {
    /// The byte range of the `number` word in `text`, the YAML source the
    /// scanned value was parsed from.
    pub fn range_in(&self, text: &str) -> Option<std::ops::Range<usize>> {
        let mut steps: Vec<_> = self.path.iter().map(YamlKey::step).collect();
        steps.push(crate::yaml_text::YamlStep::Value);
        let span = crate::yaml_text::yaml_span(text, &steps)?;
        let start = span.byte_start + text.get(span.byte_start..)?.find("number")?;
        (start + "number".len() <= text.len()).then_some(start..start + "number".len())
    }
}

/// Every [`NumberSite`] of `value`, in document order.
pub fn number_type_sites(value: &yaml_serde::Value) -> Vec<NumberSite> {
    fn is_number(v: &yaml_serde::Value) -> bool {
        v.as_str() == Some("number")
    }
    fn walk(v: &yaml_serde::Value, path: &mut Vec<YamlKey>, out: &mut Vec<NumberSite>) {
        match v {
            yaml_serde::Value::Mapping(m) => {
                for (k, child) in m {
                    let Some(key) = k.as_str() else { continue };
                    path.push(YamlKey::Key(key.to_string()));
                    if key == "type" && is_number(child) {
                        out.push(NumberSite {
                            path: path.clone(),
                            default: m.get("default").and_then(yaml_serde::Value::as_f64),
                        });
                    } else if let (true, Some(shorthand)) =
                        (matches!(key, "params" | "payload"), child.as_mapping())
                    {
                        for (name, ty) in shorthand {
                            let Some(name) = name.as_str() else { continue };
                            path.push(YamlKey::Key(name.to_string()));
                            if is_number(ty) {
                                out.push(NumberSite {
                                    path: path.clone(),
                                    default: None,
                                });
                            } else {
                                walk(ty, path, out);
                            }
                            path.pop();
                        }
                    } else {
                        walk(child, path, out);
                    }
                    path.pop();
                }
            }
            yaml_serde::Value::Sequence(items) => {
                for (i, item) in items.iter().enumerate() {
                    path.push(YamlKey::Item(i));
                    walk(item, path, out);
                    path.pop();
                }
            }
            yaml_serde::Value::Tagged(t) => walk(&t.value, path, out),
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(value, &mut Vec::new(), &mut out);
    out
}

/// Replace the `number` scalar at every site of `value` with `double`, so
/// the declaration decodes and the rest of it is still read; the caller has
/// reported each site and marks its declaration invalid.
pub fn patch_number_sites(value: &mut yaml_serde::Value, sites: &[NumberSite]) {
    fn node<'a>(
        v: &'a mut yaml_serde::Value,
        path: &[YamlKey],
    ) -> Option<&'a mut yaml_serde::Value> {
        let Some((step, rest)) = path.split_first() else {
            return Some(v);
        };
        let next = match step {
            YamlKey::Key(k) => v.get_mut(k.as_str())?,
            YamlKey::Item(i) => v.get_mut(*i)?,
        };
        node(next, rest)
    }
    for site in sites {
        if let Some(at) = node(value, &site.path).filter(|at| at.as_str() == Some("number")) {
            *at = yaml_serde::Value::String("double".into());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enum_type_accepts_member_rejects_nonmember() {
        let t = Type::Enum(vec!["gold".into(), "silver".into()]);
        assert!(type_accepts(&t, &Literal::Str("gold".into())));
        assert!(!type_accepts(&t, &Literal::Str("bronze".into())));
    }

    #[test]
    fn list_type_accepts_homogeneous_only() {
        let t = Type::List(Box::new(Type::Int));
        assert!(type_accepts(
            &t,
            &Literal::List(vec![Literal::Int(1), Literal::Int(2)])
        ));
        assert!(!type_accepts(
            &t,
            &Literal::List(vec![Literal::Int(1), Literal::Bool(true)])
        ));
    }

    #[test]
    fn yaml_roundtrips_provider_ref_type() {
        let y = "providerRef: character";
        let t: Type = yaml_serde::from_str(y).unwrap();
        assert!(matches!(&t, Type::ProviderRef(n) if n == "character"));
    }

    #[test]
    fn domain_type_roundtrips_and_accepts_string() {
        let ty: Type = yaml_serde::from_str("{ domain: emotion }").unwrap();
        assert_eq!(ty, Type::Domain("emotion".into()));
        assert_eq!(
            yaml_serde::to_string(&ty).unwrap().trim(),
            "domain: emotion"
        );
        assert!(type_accepts(&ty, &Literal::Str("neutral".into()))); // structural: any string
        assert!(!type_accepts(&ty, &Literal::Bool(true)));
    }

    /// dsl 0.26.0 §2.5: the `{ entity: <kind> }` attr type.
    #[test]
    fn entity_type_roundtrips() {
        let ty: Type = yaml_serde::from_str("{ entity: bagItem }").unwrap();
        assert_eq!(ty, Type::Entity("bagItem".into()));
        assert_eq!(
            yaml_serde::to_string(&ty).unwrap().trim(),
            "entity: bagItem"
        );
        assert!(!type_accepts(&ty, &Literal::Int(1)));
    }

    #[test]
    fn from_attr_binds_camelcase_slottype() {
        let y = "fromAttr:\n  name: resultKey\n  slotType: localId";
        let seg: PathSegment = yaml_serde::from_str(y).unwrap();
        match seg {
            PathSegment::FromAttr { from_attr } => {
                assert_eq!(from_attr.name, "resultKey");
                assert_eq!(from_attr.slot_type.as_deref(), Some("localId"));
            }
            _ => panic!("expected FromAttr segment"),
        }
    }

    #[test]
    fn type_wire_forms_roundtrip() {
        let cases = [
            "bool",
            "int",
            "double",
            "string",
            "enum:\n  - gold\n  - silver",
            "list: int",
            "map:\n  key: string\n  value: double",
            "enumFromOption: allowedKinds",
            "providerRef: character",
            "slotId:\n  namespace: scene.minigame",
            "record:\n  - name: hp\n    type: int",
        ];
        for src in cases {
            let t: Type =
                yaml_serde::from_str(src).unwrap_or_else(|e| panic!("parse {src:?}: {e}"));
            let out = yaml_serde::to_string(&t).unwrap();
            let t2: Type = yaml_serde::from_str(&out)
                .unwrap_or_else(|e| panic!("reparse {src:?} -> {out:?}: {e}"));
            assert_eq!(t, t2, "roundtrip mismatch for {src:?}");
        }
    }

    #[test]
    fn type_accepts_record_literal() {
        use std::collections::BTreeMap;
        let ty = Type::Record(vec![
            Field {
                name: "costume".into(),
                ty: Type::Str,
                default: None,
                required: true,
                shape: None,
            },
            Field {
                name: "sealed".into(),
                ty: Type::Bool,
                default: Some(Literal::Bool(false)),
                required: false,
                shape: None,
            },
        ]);
        let mut m = BTreeMap::new();
        m.insert("costume".to_string(), Literal::Str("waitress".into()));
        m.insert("sealed".to_string(), Literal::Bool(true));
        assert!(type_accepts(&ty, &Literal::Map(m)));

        // missing required field -> reject
        let mut bad = BTreeMap::new();
        bad.insert("sealed".to_string(), Literal::Bool(true));
        assert!(!type_accepts(&ty, &Literal::Map(bad)));
    }

    #[test]
    fn type_accepts_map_literal() {
        use std::collections::BTreeMap;
        let ty = Type::Map {
            key: Box::new(Type::Str),
            value: Box::new(Type::Double),
        };
        let mut m = BTreeMap::new();
        m.insert("a".to_string(), Literal::Double(1.0));
        m.insert("b".to_string(), Literal::Double(2.0));
        assert!(type_accepts(&ty, &Literal::Map(m)));

        let mut bad = BTreeMap::new();
        bad.insert("a".to_string(), Literal::Str("x".into())); // value type mismatch
        assert!(!type_accepts(&ty, &Literal::Map(bad)));
    }

    #[test]
    fn type_accepts_slotid() {
        let ty = Type::SlotId {
            namespace: "scene.minigame".into(),
        };
        assert!(type_accepts(&ty, &Literal::Str("service01".into())));
        assert!(!type_accepts(&ty, &Literal::Int(1)));
        assert!(!type_accepts(&ty, &Literal::Bool(true)));
    }

    #[test]
    fn type_accepts_assetkind() {
        let ty = Type::AssetKind("CH".into());
        assert!(type_accepts(&ty, &Literal::Str("waitress".into())));
        assert!(!type_accepts(&ty, &Literal::Int(1)));
        assert!(!type_accepts(&ty, &Literal::Bool(true)));
    }

    #[test]
    fn assetkind_wire_roundtrip() {
        // Wire form is `{ assetKind: <name> }` (camelCase), like providerRef/slotId.
        let y = "assetKind: CH";
        let t: Type = yaml_serde::from_str(y).unwrap();
        assert!(matches!(&t, Type::AssetKind(n) if n == "CH"));
        let out = yaml_serde::to_string(&t).unwrap();
        let t2: Type = yaml_serde::from_str(&out).unwrap();
        assert_eq!(t, t2, "roundtrip mismatch for {y:?}");
        assert_eq!(out.trim_end(), "assetKind: CH");
    }

    #[test]
    fn narrative_time_roundtrips_and_accepts_nothing() {
        let ty: Type = yaml_serde::from_str("narrativeTime").unwrap();
        assert_eq!(ty, Type::NarrativeTime);
        assert_eq!(yaml_serde::to_string(&ty).unwrap().trim(), "narrativeTime");
        assert!(!type_accepts(&ty, &Literal::Str("x".into())));
        assert!(!type_accepts(&ty, &Literal::Int(1)));
        assert!(!type_accepts(&ty, &Literal::Double(1.0)));
        assert!(!type_accepts(&ty, &Literal::Bool(true)));
    }

    #[test]
    fn literal_from_yaml_scalars_list_map() {
        let y: yaml_serde::Value = yaml_serde::from_str("[rhythm, timing]").unwrap();
        assert_eq!(
            Literal::from_yaml(&y),
            Some(Literal::List(vec![
                Literal::Str("rhythm".into()),
                Literal::Str("timing".into())
            ]))
        );
        let y2: yaml_serde::Value = yaml_serde::from_str("scene").unwrap();
        assert_eq!(Literal::from_yaml(&y2), Some(Literal::Str("scene".into())));
        let y3: yaml_serde::Value = yaml_serde::from_str("{ a: 1, b: 2.0 }").unwrap();
        match Literal::from_yaml(&y3).unwrap() {
            Literal::Map(m) => {
                assert_eq!(m.get("a"), Some(&Literal::Int(1)));
                assert_eq!(m.get("b"), Some(&Literal::Double(2.0)));
            }
            other => panic!("expected Map, got {other:?}"),
        }
    }
}
