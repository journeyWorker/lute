//! Resolving what a script writes against the project: `state:` /
//! `engine:` / `newRun` values against their declared types, occasion
//! payloads, `bridges:` answers and ground facts.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use lute_manifest::relations::{EntityKindDecl, KindShape};
use serde_json::Value as Json;

use super::project::{state_entry_type, ExecProject};
use super::world::parse_ground_fact;
use crate::datalog::Fact;
use crate::driver::BridgeQueues;
use crate::Value;

/// A `state:` write resolved against the declared type.
#[derive(Clone, Debug, PartialEq)]
pub enum Write {
    Set(Value),
    Add(f64),
}

/// An `engine:` step's (or a `newRun` seed's) writes, validated.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Writes {
    pub state: Vec<(String, Write)>,
    pub facts: Vec<Fact>,
    pub retract: Vec<Fact>,
    /// dsl 0.26.0 §7 (T2-9): accept-driven quests the engine accepts.
    pub accept: Vec<String>,
}

impl Writes {
    /// Whether there is nothing to write.
    pub fn is_empty(&self) -> bool {
        self.state.is_empty()
            && self.facts.is_empty()
            && self.retract.is_empty()
            && self.accept.is_empty()
    }
}

/// Why a literal does not fit its declared type ([`typed_literal`]) or its
/// closed domain ([`member_of`]). `Display` is the runtime's reason,
/// unprefixed.
#[derive(Clone, Debug, PartialEq)]
pub enum LiteralError {
    /// A `bool` takes `true` or `false`.
    Bool,
    /// An `int` takes an integer.
    Int(std::num::ParseIntError),
    /// A `double` takes a number.
    Double(std::num::ParseFloatError),
    /// The literal is not one of the enum's members; `nearest` is the
    /// member it most likely meant.
    Enum {
        members: Vec<String>,
        nearest: Option<String>,
    },
    /// `lit` is not a member of the closed domain `domain`.
    NotMember {
        lit: String,
        domain: String,
        members: Vec<String>,
        nearest: Option<String>,
    },
}

impl std::fmt::Display for LiteralError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bool => f.write_str("a `bool` is `true` or `false`"),
            Self::Int(_) => f.write_str("an `int` takes an integer"),
            Self::Double(_) => f.write_str("a `double` takes a number"),
            Self::Enum { members, nearest } => {
                write!(f, "the enum's members are {}", members.join(", "))?;
                did_you_mean(f, nearest.as_deref())
            }
            Self::NotMember {
                lit,
                domain,
                members,
                nearest,
            } => {
                write!(
                    f,
                    "`{lit}` is not a member of `{domain}` ({})",
                    members.join(", ")
                )?;
                did_you_mean(f, nearest.as_deref())
            }
        }
    }
}

impl std::error::Error for LiteralError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Int(source) => Some(source),
            Self::Double(source) => Some(source),
            Self::Bool | Self::Enum { .. } | Self::NotMember { .. } => None,
        }
    }
}

/// Why a script's write, payload, `bridges:` answer or ground atom does not
/// resolve against the project ([`typed_payload`], [`resolve_state`],
/// [`resolve_bridges`], [`resolve_fact`]). `Display` is the runtime's
/// message: [`resolve_state`]'s and [`resolve_fact`]'s are predicates for
/// the caller to prefix.
#[derive(Clone, Debug, PartialEq)]
pub enum ResolveError {
    /// A raise gives a payload to an occasion that declares none.
    NoPayload { occasion: String },
    /// A raise gives a payload field the occasion does not declare.
    UnknownPayloadField {
        occasion: String,
        field: String,
        declared: Vec<String>,
    },
    /// A payload value its declared type refuses.
    PayloadValue {
        field: String,
        lit: String,
        source: Box<LiteralError>,
    },
    /// A raise leaves declared payload fields out: `missing` is
    /// [`missing_payload`]'s sentence, `example` [`payload_example`]'s.
    MissingPayload { missing: String, example: String },
    /// A write to `scene.*`.
    SceneWrite,
    /// An `entry.<id>.read` / `.everRead` path whose entry no document
    /// declares.
    UnknownEntry { id: String },
    /// An entry flag written a literal that is not `true` or `false`.
    EntryFlag { lit: String },
    /// A path the state table does not declare.
    UndeclaredState,
    /// A state value its declared type or domain refuses.
    StateValue {
        lit: String,
        source: Box<LiteralError>,
    },
    /// A `bridges:` tag no plugin call reads a bridge result of.
    UnknownBridge {
        at: String,
        tag: String,
        nearest: Option<String>,
    },
    /// A `bridges:` answer field no effect of the tag's calls reads.
    UnreadBridgeField {
        at: String,
        tag: String,
        answer: usize,
        field: String,
        reads: Vec<String>,
    },
    /// A `bridges:` answer value a result slot's declared type refuses.
    BridgeValue(Box<BridgeMisfit>),
    /// A `bridges:` answer lacks a field content reads; `shape` is the
    /// whole answer content reads.
    MissingBridgeField {
        at: String,
        tag: String,
        answer: usize,
        field: String,
        shape: String,
    },
    /// Not a ground atom `rel(arg, …)`; `hint` is [`split_atom_hint`]'s.
    NotGround { hint: &'static str },
    /// The relation is derived by rules.
    Derived,
    /// [`atom_problem`]'s reason.
    Atom(String),
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoPayload { occasion } => write!(
                f,
                "`payload` — occasion `{occasion}` declares no `payload:`"
            ),
            Self::UnknownPayloadField {
                occasion,
                field,
                declared,
            } => write!(
                f,
                "`payload.{field}` — occasion `{occasion}` declares no payload field `{field}` \
                 (declared: {})",
                declared
                    .iter()
                    .map(|k| format!("`{k}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::PayloadValue { field, lit, source } => {
                write!(f, "`payload.{field}: {lit}` — {source}")
            }
            Self::MissingPayload { missing, example } => {
                write!(f, "{missing} — add `payload: {{ {example} }}` to the step")
            }
            Self::SceneWrite => f.write_str(
                "is `scene.*`, which resets at every scene boundary and cannot be written",
            ),
            Self::UnknownEntry { id } => {
                write!(f, "names entry `{id}`, which no document declares")
            }
            Self::EntryFlag { lit } => write!(f, "is a bool: `{lit}` is not `true` or `false`"),
            Self::UndeclaredState => f.write_str("is not a declared state path in this project"),
            Self::StateValue { lit, source } => write!(f, "does not take `{lit}`: {source}"),
            Self::UnknownBridge { at, tag, nearest } => {
                write!(
                    f,
                    "{at}: `bridges.{tag}` answers no plugin call — no document of this project \
                     calls `::{tag}` with an effect that reads a bridge result"
                )?;
                did_you_mean(f, nearest.as_deref())
            }
            Self::UnreadBridgeField {
                at,
                tag,
                answer,
                field,
                reads,
            } => write!(
                f,
                "{at}: `bridges.{tag}` answer {answer} gives `{field}`, which no effect of the \
                 `::{tag}` calls reads (they read: {})",
                reads.join(", ")
            ),
            Self::BridgeValue(misfit) => {
                let BridgeMisfit {
                    at,
                    tag,
                    answer,
                    field,
                    lit,
                    path,
                    source,
                } = misfit.as_ref();
                write!(
                    f,
                    "{at}: `bridges.{tag}` answer {answer}: `{field}: {lit}` does not fit \
                     `{path}` — {source}"
                )
            }
            Self::MissingBridgeField {
                at,
                tag,
                answer,
                field,
                shape,
            } => write!(
                f,
                "{at}: `bridges.{tag}` answer {answer} lacks `{field}`, which content reads — an \
                 answer gives every bridge result `::{tag}` content reads: `{shape}`"
            ),
            Self::NotGround { hint } => {
                write!(f, "is not a ground fact `rel(arg, …)`{hint}")
            }
            Self::Derived => f.write_str("is derived by rules and cannot be asserted"),
            Self::Atom(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for ResolveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::PayloadValue { source, .. } | Self::StateValue { source, .. } => {
                Some(source.as_ref())
            }
            Self::BridgeValue(misfit) => Some(&misfit.source),
            Self::NoPayload { .. }
            | Self::UnknownPayloadField { .. }
            | Self::MissingPayload { .. }
            | Self::SceneWrite
            | Self::UnknownEntry { .. }
            | Self::EntryFlag { .. }
            | Self::UndeclaredState
            | Self::UnknownBridge { .. }
            | Self::UnreadBridgeField { .. }
            | Self::MissingBridgeField { .. }
            | Self::NotGround { .. }
            | Self::Derived
            | Self::Atom(_) => None,
        }
    }
}

/// The fields of [`ResolveError::BridgeValue`]: `bridges.<tag>` answer
/// `answer` (1-based, written at `at`) gives `field: lit`, which the result
/// slot `path` refuses for `source`.
#[derive(Clone, Debug, PartialEq)]
pub struct BridgeMisfit {
    pub at: String,
    pub tag: String,
    pub answer: usize,
    pub field: String,
    pub lit: String,
    pub path: String,
    pub source: LiteralError,
}

/// A literal against a state-table entry's declared type (the trace-mock
/// rule, `E-TRACE-MOCK-TYPE`): `bool` takes `true`/`false`, `number` a
/// number, `enum` a member of its domain; every other type the text.
pub fn typed_literal(entry: &Json, lit: &str) -> Result<Value, LiteralError> {
    match entry.get("type").and_then(Json::as_str) {
        Some("bool") => match lit {
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            _ => Err(LiteralError::Bool),
        },
        Some("int") => lit
            .parse::<i64>()
            .map(Value::Int)
            .map_err(LiteralError::Int),
        Some("double") => lit
            .parse::<f64>()
            .map(Value::Double)
            .map_err(LiteralError::Double),
        Some("enum") => {
            let domain: Vec<&str> = entry
                .get("domain")
                .and_then(Json::as_array)
                .into_iter()
                .flatten()
                .filter_map(Json::as_str)
                .collect();
            if domain.contains(&lit) {
                Ok(Value::Str(lit.to_string()))
            } else {
                Err(LiteralError::Enum {
                    nearest: nearest(lit, domain.iter().copied()),
                    members: domain.into_iter().map(str::to_string).collect(),
                })
            }
        }
        _ => Ok(Value::Str(lit.to_string())),
    }
}

/// The member nearest a misspelt `lit`, if any.
fn nearest<'a>(lit: &str, members: impl IntoIterator<Item = &'a str>) -> Option<String> {
    lute_manifest::suggest::nearest(lit, members, 2).map(str::to_string)
}

/// ` — did you mean `x`?` for the `nearest` member, or nothing.
fn did_you_mean(f: &mut std::fmt::Formatter<'_>, nearest: Option<&str>) -> std::fmt::Result {
    match nearest {
        Some(k) => write!(f, " — did you mean `{k}`?"),
        None => Ok(()),
    }
}

/// dsl 0.27.0 §2 (T1-2): `lit` against a closed domain `K` (a named enum or
/// an entity kind with `members:`) — `Err` names the members and the one
/// nearest the literal (`` `rne` is not a member of `route` (ren, mika) —
/// did you mean `ren`? ``).
pub fn member_of(domain: &str, members: &[String], lit: &str) -> Result<(), LiteralError> {
    if members.iter().any(|m| m == lit) {
        return Ok(());
    }
    Err(LiteralError::NotMember {
        lit: lit.to_string(),
        domain: domain.to_string(),
        members: members.to_vec(),
        nearest: nearest(lit, members.iter().map(String::as_str)),
    })
}

/// A literal written to the declared state path `path` (its state-table
/// `entry`): [`typed_literal`], and — a path typed `{ domain: K }` /
/// `{ entity: K }` over a closed `K` — a member of `K` ([`member_of`]).
pub fn state_literal(
    p: &ExecProject,
    path: &str,
    entry: &Json,
    lit: &str,
) -> Result<Value, LiteralError> {
    let value = typed_literal(entry, lit)?;
    if let Some((domain, members)) = p.state_domains.get(path) {
        member_of(domain, members, lit)?;
    }
    Ok(value)
}

/// dsl 0.27.0 §3: a payload literal against its declared type — the
/// [`typed_literal`] rule over a manifest [`lute_manifest::types::Type`];
/// a `{ domain: K }` / `{ entity: K }` field over a closed `K` takes a
/// member of `K` ([`member_of`]).
pub fn payload_value(
    p: &ExecProject,
    ty: &lute_manifest::types::Type,
    lit: &str,
) -> Result<Value, LiteralError> {
    use lute_manifest::types::Type;
    match ty {
        Type::Bool => typed_literal(&serde_json::json!({ "type": "bool" }), lit),
        Type::Int => typed_literal(&serde_json::json!({ "type": "int" }), lit),
        Type::Double => typed_literal(&serde_json::json!({ "type": "double" }), lit),
        Type::Enum(members) => typed_literal(
            &serde_json::json!({ "type": "enum", "domain": members }),
            lit,
        ),
        Type::Domain(name) | Type::Entity(name) => {
            if let Some(members) = domain_members(p, name) {
                member_of(name, members, lit)?;
            }
            Ok(Value::Str(lit.to_string()))
        }
        _ => Ok(Value::Str(lit.to_string())),
    }
}

/// dsl 0.27.0 §3: a raise's payload as authored (`copies: 2`), each field
/// typed by `occasion`'s `payload:` declaration — keyed by its
/// `occasion.payload.<field>` path. `Err` names an occasion without a
/// payload, an undeclared field, a value its type refuses, or a declared
/// field the raise leaves out (every raise carries its whole payload).
pub fn typed_payload(
    p: &ExecProject,
    occasion: &str,
    fields: &[(String, String)],
) -> Result<BTreeMap<String, Value>, ResolveError> {
    let mut out = BTreeMap::new();
    let declared = p
        .occasions
        .get(occasion)
        .map(|d| &d.payload)
        .filter(|p| !p.is_empty());
    let Some(declared) = declared else {
        if fields.is_empty() {
            return Ok(out);
        }
        return Err(ResolveError::NoPayload {
            occasion: occasion.to_string(),
        });
    };
    for (field, lit) in fields {
        let Some(ty) = declared.get(field) else {
            return Err(ResolveError::UnknownPayloadField {
                occasion: occasion.to_string(),
                field: field.clone(),
                declared: declared.keys().cloned().collect(),
            });
        };
        let value = payload_value(p, ty, lit).map_err(|source| ResolveError::PayloadValue {
            field: field.clone(),
            lit: lit.clone(),
            source: Box::new(source),
        })?;
        out.insert(
            format!(
                "{}.{field}",
                lute_manifest::semantics::occasion_bind::OCCASION_PAYLOAD
            ),
            value,
        );
    }
    match missing_payload(occasion, declared, |f| fields.iter().any(|(k, _)| k == f)) {
        Some(missing) => Err(ResolveError::MissingPayload {
            missing,
            example: payload_example(declared),
        }),
        None => Ok(out),
    }
}

/// dsl 0.28.0 (T1-13): the declared payload fields of `occasion` a raise
/// leaves out (`given` answers whether a field is supplied), as the
/// sentence naming them — `None` when every declared field is given.
pub fn missing_payload(
    occasion: &str,
    declared: &BTreeMap<String, lute_manifest::types::Type>,
    given: impl Fn(&str) -> bool,
) -> Option<String> {
    let missing: Vec<String> = declared
        .keys()
        .filter(|f| !given(f))
        .map(|f| format!("`{f}`"))
        .collect();
    if missing.is_empty() {
        return None;
    }
    let (noun, verb) = if missing.len() == 1 {
        ("field", "is")
    } else {
        ("fields", "are")
    };
    Some(format!(
        "occasion `{occasion}` declares payload {noun} {}, which {verb} missing from this raise",
        missing.join(", ")
    ))
}

/// `seconds: <number>, mood: <calm|storm>` — each declared payload field
/// with a placeholder of its type, for a remedy.
pub fn payload_example(declared: &BTreeMap<String, lute_manifest::types::Type>) -> String {
    declared
        .iter()
        .map(|(f, ty)| format!("{f}: {}", payload_placeholder(ty)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// A placeholder value for a payload field of type `ty` (`<number>`).
pub fn payload_placeholder(ty: &lute_manifest::types::Type) -> String {
    use lute_manifest::types::Type;
    match ty {
        Type::Bool => "<true|false>".to_string(),
        Type::Int | Type::Double => "<number>".to_string(),
        Type::Enum(members) => format!("<{}>", members.join("|")),
        Type::Domain(k) | Type::Entity(k) => format!("<a {k}>"),
        _ => "<value>".to_string(),
    }
}

/// The entry id and flag of a reserved `entry.<id>.read` /
/// `entry.<id>.everRead` path (dsl 0.19.0 §5, 0.22.0 §7).
pub fn entry_flag(path: &str) -> Option<(&str, &str)> {
    let rest = path.strip_prefix("entry.")?;
    let (id, flag) = rest.rsplit_once('.')?;
    (matches!(flag, "read" | "everRead") && !id.is_empty() && !id.contains('.'))
        .then_some((id, flag))
}

/// The `entry.<id>.everRead` path (dsl 0.22.0 §7: user tier, set on first
/// read, never reset by `newRun`).
pub fn ever_read_path(id: &str) -> String {
    format!("entry.{id}.everRead")
}

/// Resolve one written state value — a `state:` seed, an `engine:` write,
/// a `newRun` seed — against the project: the path is declared (a reserved
/// entry flag names a declared entry), not `scene.*`, and the literal fits
/// the declared type. `Err` is the reason as a predicate of the path
/// (`… is not a declared state path`), for the caller to prefix.
pub fn resolve_state(p: &ExecProject, path: &str, lit: &str) -> Result<Value, ResolveError> {
    if path.starts_with("scene.") {
        return Err(ResolveError::SceneWrite);
    }
    if let Some((id, _)) = entry_flag(path) {
        if !p.entry_ids.contains(id) {
            return Err(ResolveError::UnknownEntry { id: id.to_string() });
        }
        return match lit {
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            _ => Err(ResolveError::EntryFlag {
                lit: lit.to_string(),
            }),
        };
    }
    // dsl 0.23.0 §6: a save made after a run ended carries `prev.run.*`,
    // typed by the `run.*` path it mirrors.
    let declared = match path.strip_prefix("prev.") {
        Some(run) if run.starts_with("run.") => run,
        _ => path,
    };
    let Some(entry) = p.state_table.get(declared) else {
        return Err(ResolveError::UndeclaredState);
    };
    state_literal(p, declared, entry, lit).map_err(|source| ResolveError::StateValue {
        lit: lit.to_string(),
        source: Box::new(source),
    })
}

/// dsl 0.24.0 §5: resolve a script's `bridges:` (`at` names where it was
/// written) against the plugin calls of the project — a usage error unless
/// the tag names a plugin directive some document calls whose effects read
/// a bridge result, every answer gives only fields those effects read and
/// (dsl 0.25.0 §7) every one of them content reads ([`Project::bridge_reads`]),
/// and each value fits the declared type of every result slot a call writes
/// it to. The resolved answers, queued in order per tag.
pub fn resolve_bridges(
    p: &ExecProject,
    at: &str,
    raw: &BTreeMap<String, Vec<crate::BridgeAnswer>>,
) -> Result<BTreeMap<String, VecDeque<crate::BridgeAnswer>>, ResolveError> {
    // tag -> field -> the result slots the project's calls write it to.
    let mut calls: BTreeMap<&str, BTreeMap<&str, BTreeSet<&str>>> = BTreeMap::new();
    // tag -> the fields content reads, in effect order, typed by the slot
    // (the typed answer `lute trace` / `lute test` spell, ember N7).
    let mut shapes: BTreeMap<&str, Vec<(&str, Option<lute_manifest::types::Type>)>> =
        BTreeMap::new();
    for art in p.artifacts.values() {
        let cmds = art
            .get("commands")
            .and_then(Json::as_array)
            .into_iter()
            .flatten();
        for c in cmds.filter(|c| c.get("kind").and_then(Json::as_str) == Some("plugin")) {
            let tag = c.get("tag").and_then(Json::as_str).unwrap_or("");
            for e in c
                .get("effects")
                .and_then(Json::as_array)
                .into_iter()
                .flatten()
            {
                let field = e.pointer("/from/bridgeResult").and_then(Json::as_str);
                let path = e.get("path").and_then(Json::as_str);
                if let (Some(field), Some(path)) = (field, path) {
                    calls
                        .entry(tag)
                        .or_default()
                        .entry(field)
                        .or_default()
                        .insert(path);
                    let read = p.bridge_reads.reads(tag, field);
                    let shape = shapes.entry(tag).or_default();
                    if read && !shape.iter().any(|(f, _)| *f == field) {
                        shape.push((field, state_entry_type(art, path)));
                    }
                }
            }
        }
    }
    for (tag, answers) in raw {
        let Some(fields) = calls.get(tag.as_str()) else {
            return Err(ResolveError::UnknownBridge {
                at: at.to_string(),
                tag: tag.clone(),
                nearest: nearest(tag, calls.keys().copied()),
            });
        };
        for (i, answer) in answers.iter().enumerate() {
            let n = i + 1;
            for (field, lit) in answer {
                let Some(paths) = fields.get(field.as_str()) else {
                    return Err(ResolveError::UnreadBridgeField {
                        at: at.to_string(),
                        tag: tag.clone(),
                        answer: n,
                        field: field.clone(),
                        reads: fields.keys().map(|k| (*k).to_string()).collect(),
                    });
                };
                for path in paths {
                    if let Some(entry) = p.state_table.get(*path) {
                        state_literal(p, path, entry, lit).map_err(|source| {
                            ResolveError::BridgeValue(Box::new(BridgeMisfit {
                                at: at.to_string(),
                                tag: tag.clone(),
                                answer: n,
                                field: field.clone(),
                                lit: lit.clone(),
                                path: (*path).to_string(),
                                source,
                            }))
                        })?;
                    }
                }
            }
            if let Some(missing) = fields
                .keys()
                .filter(|f| p.bridge_reads.reads(tag, f))
                .find(|f| !answer.iter().any(|(a, _)| a == *f))
            {
                let shape = crate::bridge_answer_shape(
                    shapes
                        .get(tag.as_str())
                        .into_iter()
                        .flatten()
                        .map(|(f, t)| (*f, t.as_ref())),
                );
                return Err(ResolveError::MissingBridgeField {
                    at: at.to_string(),
                    tag: tag.clone(),
                    answer: n,
                    field: (*missing).to_string(),
                    shape,
                });
            }
        }
    }
    Ok(BridgeQueues::queue(raw))
}

/// Resolve one ground atom a script asserts or retracts (`facts:`,
/// `engine.facts` / `engine.retract`, a `newRun` seed): a declared,
/// non-derived relation at its arity whose closed-domain args are members.
/// Reserved relations are allowed — the engine is exactly who asserts them.
/// `next` is the list entry after `f`, for [`split_atom_hint`]. `Err` is
/// the reason, unprefixed.
pub fn resolve_fact(p: &ExecProject, f: &str, next: Option<&str>) -> Result<Fact, ResolveError> {
    let fact = parse_ground_fact(f)
        .filter(|(_, args)| args.iter().all(|a| !a.is_empty() && a != "_"))
        .ok_or_else(|| ResolveError::NotGround {
            hint: split_atom_hint(f, next),
        })?;
    if p.index
        .relations
        .iter()
        .any(|r| r.name == fact.0 && r.derive)
    {
        return Err(ResolveError::Derived);
    }
    match atom_problem(p, &fact.0, &fact.1) {
        Some(why) => Err(ResolveError::Atom(why)),
        None => Ok(fact),
    }
}

/// Why an atom whose parentheses do not balance is malformed, read after
/// the atom; empty when they balance. YAML splits an unquoted flow-list atom
/// at its comma — `[heard(tavi, regent)]` reaches us as `heard(tavi` then
/// `regent)` — so when `next` (the following list entry) closes what `atom`
/// opened, the fix is quoting; otherwise the atom itself is unbalanced (a
/// quoted `"empty(ada"`).
pub fn split_atom_hint(atom: &str, next: Option<&str>) -> &'static str {
    let count = |s: &str, c: char| {
        isize::try_from(s.matches(c).count()).expect("a count is at most isize::MAX")
    };
    let open = |s: &str| count(s, '(') - count(s, ')');
    match open(atom) {
        0 => "",
        o if o > 0 && next.is_some_and(|n| open(n) < 0) => {
            " — quote the atom: YAML splits an unquoted `[a(b, c)]` at its comma (write \
             `[\"a(b, c)\"]`)"
        }
        _ => " — its parentheses do not balance",
    }
}

/// Why the atom `rel(args)` names no fact this project can hold — an
/// undeclared relation, the wrong arity, or a closed-domain argument that
/// is not a member — with a did-you-mean; `None` when it can. A derived
/// relation is fine: an expectation (`facts:` / `notFacts:`) judges what
/// holds after derivation (0.27 prerelease OT N-2). The reason reads after
/// the atom, unprefixed.
pub fn atom_problem(p: &ExecProject, rel: &str, args: &[String]) -> Option<String> {
    let near = |s: &str, known: &mut dyn Iterator<Item = &str>| {
        lute_manifest::suggest::nearest(s, known, 2)
            .map(|k| format!(" — did you mean `{k}`?"))
            .unwrap_or_default()
    };
    let Some(r) = p.index.relations.iter().find(|r| r.name == rel) else {
        return Some(format!(
            "names an undeclared relation `{rel}`{}",
            near(rel, &mut p.index.relations.iter().map(|r| r.name.as_str()))
        ));
    };
    if r.args.len() != args.len() {
        return Some(format!(
            "has {} argument(s), and `{rel}` takes {}: `{rel}({})`",
            args.len(),
            r.args.len(),
            r.args.join(", ")
        ));
    }
    for (arg, domain) in args.iter().zip(&r.args) {
        if let Some(ms) = domain_members(p, domain).filter(|ms| !ms.contains(arg)) {
            return Some(format!(
                "names `{arg}`, which is not a member of `{domain}`{} ({})",
                near(arg, &mut ms.iter().map(String::as_str)),
                ms.join(", ")
            ));
        }
    }
    None
}

/// The members of a closed argument domain — an entity kind's `members:`
/// or an enum's — or `None` for an open one.
pub fn domain_members<'a>(p: &'a ExecProject, domain: &str) -> Option<&'a [String]> {
    match p.kinds.get(domain) {
        Some(EntityKindDecl {
            shape: KindShape::Members(ms),
            ..
        }) => Some(ms),
        Some(_) => None,
        None => p
            .index
            .enums
            .iter()
            .find(|e| e.name == domain)
            .map(|e| e.members.as_slice()),
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use serde_json::json;

    use super::*;

    /// `Display` is the text the `String` errors carried before 0.49.1, and
    /// `source()` keeps the parse error the message replaces.
    #[test]
    fn literal_errors_keep_their_text_and_cause() {
        let int = typed_literal(&json!({ "type": "int" }), "2.5").unwrap_err();
        assert_eq!(int.to_string(), "an `int` takes an integer");
        let cause = int.source().expect("an int error keeps its parse error");
        assert!(cause.is::<std::num::ParseIntError>());
        let double = typed_literal(&json!({ "type": "double" }), "x").unwrap_err();
        assert_eq!(double.to_string(), "a `double` takes a number");
        let cause = double
            .source()
            .expect("a double error keeps its parse error");
        assert!(cause.is::<std::num::ParseFloatError>());
        let bool_ = typed_literal(&json!({ "type": "bool" }), "yes").unwrap_err();
        assert_eq!(bool_.to_string(), "a `bool` is `true` or `false`");
        assert!(bool_.source().is_none());
        let members = ["ren".to_string(), "mika".to_string()];
        let enum_ =
            typed_literal(&json!({ "type": "enum", "domain": members }), "rne").unwrap_err();
        assert_eq!(
            enum_.to_string(),
            "the enum's members are ren, mika — did you mean `ren`?"
        );
        assert_eq!(
            member_of("route", &members, "rne").unwrap_err().to_string(),
            "`rne` is not a member of `route` (ren, mika) — did you mean `ren`?"
        );
        assert_eq!(
            member_of("route", &members, "zzzzzz")
                .unwrap_err()
                .to_string(),
            "`zzzzzz` is not a member of `route` (ren, mika)"
        );
        assert_eq!(member_of("route", &members, "mika"), Ok(()));
    }

    #[test]
    fn wrapping_resolve_errors_chain_to_the_literal_error() {
        let source = typed_literal(&json!({ "type": "int" }), "x").unwrap_err();
        let state = ResolveError::StateValue {
            lit: "x".to_string(),
            source: Box::new(source.clone()),
        };
        assert_eq!(
            state.to_string(),
            "does not take `x`: an `int` takes an integer"
        );
        let chained = state.source().expect("a state value error has a cause");
        assert_eq!(chained.to_string(), "an `int` takes an integer");
        let cause = chained
            .source()
            .expect("the literal error keeps its parse error");
        assert!(cause.is::<std::num::ParseIntError>());
        let bridge = ResolveError::BridgeValue(Box::new(BridgeMisfit {
            at: "step 2".to_string(),
            tag: "shop.buy".to_string(),
            answer: 1,
            field: "price".to_string(),
            lit: "x".to_string(),
            path: "run.gold".to_string(),
            source,
        }));
        assert_eq!(
            bridge.to_string(),
            "step 2: `bridges.shop.buy` answer 1: `price: x` does not fit `run.gold` — an \
             `int` takes an integer"
        );
        assert!(bridge.source().is_some());
    }
}
