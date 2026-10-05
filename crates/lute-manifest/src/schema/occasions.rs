use super::SchemaError;
use crate::types::Type;
use serde::{Deserialize, Serialize};
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
    type Error = SchemaError;

    fn try_from(v: serde_yaml::Value) -> Result<Self, SchemaError> {
        use serde_yaml::Value;
        let shown = |v: &Value| {
            serde_yaml::to_string(v).map_or_else(
                |_| String::new(),
                |s| s.trim().to_string(),
            )
        };
        let name = |key: &str| -> Result<String, SchemaError> {
            match v.get(key) {
                Some(Value::String(s)) if !s.trim().is_empty() => Ok(s.clone()),
                None | Some(Value::Null) => Err(SchemaError::OccasionTarget(format!(
                    "`target:` names no `{key}:`; {OCCASION_TARGET_SHAPES}"
                ))),
                Some(other) => Err(SchemaError::OccasionTarget(format!(
                    "`target.{key}: {}` is not a name; {OCCASION_TARGET_SHAPES}",
                    shown(other)
                ))),
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
                    return Err(SchemaError::OccasionTarget(format!(
                        "`target:` has no key `{key}`{}; {OCCASION_TARGET_SHAPES}",
                        crate::suggest::did_you_mean(&key, KEYS)
                    )));
                }
                let members = match v.get("members") {
                    None => None,
                    Some(Value::Sequence(items)) => Some(
                        items
                            .iter()
                            .map(|i| match i {
                                Value::String(s) => Ok(s.clone()),
                                other => Err(SchemaError::OccasionTarget(format!(
                                    "`target.members` lists `{}`, which is not a member name",
                                    shown(other)
                                ))),
                            })
                            .collect::<Result<Vec<_>, _>>()?,
                    ),
                    Some(other) => {
                        return Err(SchemaError::OccasionTarget(format!(
                            "`target.members: {}` is not a list — write `members: [<member>, …]`",
                            shown(other)
                        )))
                    }
                };
                Ok(OccasionTarget::Domain {
                    prefix: name("prefix")?,
                    entity: name("entity")?,
                    members,
                })
            }
            Value::Null => Err(SchemaError::OccasionTarget(format!("`target:` is empty; {OCCASION_TARGET_SHAPES}"))),
            other => Err(SchemaError::OccasionTarget(format!(
                "`target: {}` is not a target; {OCCASION_TARGET_SHAPES}",
                shown(other)
            ))),
        }
    }
}

const PAYLOAD_TYPE_FORMS: &str = "a payload field's type is `bool`, `int`, `double`, `string`, \
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
    const SCALARS: [&str; 4] = ["bool", "int", "double", "string"];
    const FORMS: [&str; 3] = ["enum", "domain", "entity"];
    /// `Type` forms a single literal cannot carry.
    const MULTI: [&str; 3] = ["list", "record", "map"];
    let forms = SCALARS.iter().chain(FORMS.iter()).copied();
    let refuse = |name: &str| -> String {
        if name == "number" {
            format!("payload field `{field}`: {}", crate::types::NUMBER_TYPE_REMOVED)
        } else if FORMS.contains(&name) {
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
            "int" => Ok(Type::Int),
            "double" => Ok(Type::Double),
            "string" => Ok(Type::Str),
            other => Err(refuse(other)),
        },
        Value::Mapping(m) if m.len() == 1 => {
            let Some((k, arg)) = m.iter().next() else {
                return Err(refuse("empty"));
            };
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
