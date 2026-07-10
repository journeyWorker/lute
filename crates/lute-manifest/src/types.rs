use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// plugin §7 Type. Serde uses YAML's tagged-map forms
/// ({ enum: [...] }, { list: T }, { providerRef: name }, ...).
///
/// serde_yaml 0.9 serializes externally-tagged enums as YAML `!tags`, not the
/// single-key maps the spec mandates, so `Type` (de)serializes through
/// `serde_yaml::with::singleton_map_recursive` via the private `TypeDef` shadow
/// below. The public shape here is authoritative for all manifest tasks.
#[derive(Clone, Debug, PartialEq)]
pub enum Type {
    Bool,
    Number,
    Str,
    Enum(Vec<String>),
    List(Box<Type>),
    Record(Vec<Field>),
    Map { key: Box<Type>, value: Box<Type> },
    EnumFromOption(String),       // attribute types only
    ProviderRef(String),          // any typed position
    Domain(String),               // any typed position; membership checked at check-stage
    SlotId { namespace: String }, // attribute types only
    AssetKind(String),            // attribute types only
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
    Num(f64),
    Str(String),
    List(Vec<Literal>),
    /// A record/map literal (plugin §7): a YAML mapping. Deterministic order.
    Map(std::collections::BTreeMap<String, Literal>),
}

impl Literal {
    /// Convert a YAML value (e.g. a scene `plugins:` option) into a `Literal`.
    /// Returns `None` for values with no literal representation (null, tagged,
    /// non-string map keys). Numbers become `Num` (f64); sequences `List`;
    /// mappings `Map` (string keys only).
    pub fn from_yaml(v: &serde_yaml::Value) -> Option<Literal> {
        use serde_yaml::Value;
        match v {
            Value::Bool(b) => Some(Literal::Bool(*b)),
            Value::Number(n) => n.as_f64().map(Literal::Num),
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
        (Type::Number, Literal::Num(_)) => true,
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
    Number,
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
    SlotId {
        namespace: String,
    },
    AssetKind(String),
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
            TypeDef::Number => Type::Number,
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
            TypeDef::SlotId { namespace } => Type::SlotId { namespace },
            TypeDef::AssetKind(s) => Type::AssetKind(s),
        }
    }
}

impl From<&Type> for TypeDef {
    fn from(t: &Type) -> Self {
        match t {
            Type::Bool => TypeDef::Bool,
            Type::Number => TypeDef::Number,
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
            Type::SlotId { namespace } => TypeDef::SlotId {
                namespace: namespace.clone(),
            },
            Type::AssetKind(s) => TypeDef::AssetKind(s.clone()),
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
        serde_yaml::with::singleton_map_recursive::serialize(&TypeDef::from(self), serializer)
    }
}

impl<'de> Deserialize<'de> for Type {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let def: TypeDef = serde_yaml::with::singleton_map_recursive::deserialize(deserializer)?;
        Ok(def.into())
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
        let t = Type::List(Box::new(Type::Number));
        assert!(type_accepts(
            &t,
            &Literal::List(vec![Literal::Num(1.0), Literal::Num(2.0)])
        ));
        assert!(!type_accepts(
            &t,
            &Literal::List(vec![Literal::Num(1.0), Literal::Bool(true)])
        ));
    }

    #[test]
    fn yaml_roundtrips_provider_ref_type() {
        let y = "providerRef: character";
        let t: Type = serde_yaml::from_str(y).unwrap();
        assert!(matches!(&t, Type::ProviderRef(n) if n == "character"));
    }

    #[test]
    fn domain_type_roundtrips_and_accepts_string() {
        let ty: Type = serde_yaml::from_str("{ domain: emotion }").unwrap();
        assert_eq!(ty, Type::Domain("emotion".into()));
        assert_eq!(serde_yaml::to_string(&ty).unwrap().trim(), "domain: emotion");
        assert!(type_accepts(&ty, &Literal::Str("neutral".into()))); // structural: any string
        assert!(!type_accepts(&ty, &Literal::Bool(true)));
    }

    #[test]
    fn from_attr_binds_camelcase_slottype() {
        let y = "fromAttr:\n  name: resultKey\n  slotType: localId";
        let seg: PathSegment = serde_yaml::from_str(y).unwrap();
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
            "number",
            "string",
            "enum:\n  - gold\n  - silver",
            "list: number",
            "map:\n  key: string\n  value: number",
            "enumFromOption: allowedKinds",
            "providerRef: character",
            "slotId:\n  namespace: scene.minigame",
            "record:\n  - name: hp\n    type: number",
        ];
        for src in cases {
            let t: Type =
                serde_yaml::from_str(src).unwrap_or_else(|e| panic!("parse {src:?}: {e}"));
            let out = serde_yaml::to_string(&t).unwrap();
            let t2: Type = serde_yaml::from_str(&out)
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
            value: Box::new(Type::Number),
        };
        let mut m = BTreeMap::new();
        m.insert("a".to_string(), Literal::Num(1.0));
        m.insert("b".to_string(), Literal::Num(2.0));
        assert!(type_accepts(&ty, &Literal::Map(m)));

        let mut bad = BTreeMap::new();
        bad.insert("a".to_string(), Literal::Str("x".into())); // value type mismatch
        assert!(!type_accepts(&ty, &Literal::Map(bad)));
    }

    #[test]
    fn type_accepts_slotid() {
        // A slotId attribute value is a bare local-identifier string
        // (`resultKey="service01"`); a non-string literal is rejected.
        let ty = Type::SlotId {
            namespace: "scene.minigame".into(),
        };
        assert!(type_accepts(&ty, &Literal::Str("service01".into())));
        assert!(!type_accepts(&ty, &Literal::Num(1.0)));
        assert!(!type_accepts(&ty, &Literal::Bool(true)));
    }

    #[test]
    fn type_accepts_assetkind() {
        // An assetKind attribute value is an authored asset-id string; a
        // non-string literal is rejected (structural validation is the checker's job).
        let ty = Type::AssetKind("CH".into());
        assert!(type_accepts(&ty, &Literal::Str("waitress".into())));
        assert!(!type_accepts(&ty, &Literal::Num(1.0)));
        assert!(!type_accepts(&ty, &Literal::Bool(true)));
    }

    #[test]
    fn assetkind_wire_roundtrip() {
        // Wire form is `{ assetKind: <name> }` (camelCase), like providerRef/slotId.
        let y = "assetKind: CH";
        let t: Type = serde_yaml::from_str(y).unwrap();
        assert!(matches!(&t, Type::AssetKind(n) if n == "CH"));
        let out = serde_yaml::to_string(&t).unwrap();
        let t2: Type = serde_yaml::from_str(&out).unwrap();
        assert_eq!(t, t2, "roundtrip mismatch for {y:?}");
        assert_eq!(out.trim_end(), "assetKind: CH");
    }

    #[test]
    fn literal_from_yaml_scalars_list_map() {
        let y: serde_yaml::Value = serde_yaml::from_str("[rhythm, timing]").unwrap();
        assert_eq!(
            Literal::from_yaml(&y),
            Some(Literal::List(vec![
                Literal::Str("rhythm".into()),
                Literal::Str("timing".into())
            ]))
        );
        let y2: serde_yaml::Value = serde_yaml::from_str("scene").unwrap();
        assert_eq!(Literal::from_yaml(&y2), Some(Literal::Str("scene".into())));
        let y3: serde_yaml::Value = serde_yaml::from_str("{ a: 1, b: two }").unwrap();
        match Literal::from_yaml(&y3).unwrap() {
            Literal::Map(m) => {
                assert_eq!(m.get("a"), Some(&Literal::Num(1.0)));
                assert_eq!(m.get("b"), Some(&Literal::Str("two".into())));
            }
            other => panic!("expected Map, got {other:?}"),
        }
    }
}
