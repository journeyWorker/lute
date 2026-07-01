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
    EnumFromOption(String),   // attribute types only
    ProviderRef(String),      // any typed position
    SlotId { namespace: String }, // attribute types only
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
}

/// plugin §7.4 structured path segment.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PathSegment {
    Literal(String),
    FromAttr { #[serde(rename = "fromAttr")] from_attr: FromAttr },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
    Map { key: Box<TypeDef>, value: Box<TypeDef> },
    EnumFromOption(String),
    ProviderRef(String),
    SlotId { namespace: String },
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
            TypeDef::SlotId { namespace } => Type::SlotId { namespace },
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
            Type::SlotId { namespace } => TypeDef::SlotId { namespace: namespace.clone() },
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
        assert!(type_accepts(&t, &Literal::List(vec![Literal::Num(1.0), Literal::Num(2.0)])));
        assert!(!type_accepts(&t, &Literal::List(vec![Literal::Num(1.0), Literal::Bool(true)])));
    }

    #[test]
    fn yaml_roundtrips_provider_ref_type() {
        let y = "providerRef: character";
        let t: Type = serde_yaml::from_str(y).unwrap();
        assert!(matches!(&t, Type::ProviderRef(n) if n == "character"));
    }
}
