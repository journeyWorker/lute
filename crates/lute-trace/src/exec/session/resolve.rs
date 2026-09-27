//! Resolving what a script writes against the project: `state:` /
//! `engine:` / `newRun` values against their declared types, occasion
//! payloads, `bridges:` answers and ground facts.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use lute_manifest::relations::{EntityKindDecl, KindShape};
use serde_json::Value as Json;

use super::project::{state_entry_type, ExecProject};
use super::world::parse_ground_fact;
use crate::datalog::Fact;
use crate::exec::BridgeQueues;
use crate::Value;

/// A `state:` write resolved against the declared type.
#[derive(Clone)]
pub enum Write {
    Set(Value),
    Add(f64),
}

/// An `engine:` step's (or a `newRun` seed's) writes, validated.
#[derive(Clone, Default)]
pub struct Writes {
    pub state: Vec<(String, Write)>,
    pub facts: Vec<Fact>,
    pub retract: Vec<Fact>,
    /// dsl 0.26.0 §7 (T2-9): accept-driven quests the engine accepts.
    pub accept: Vec<String>,
}

/// A literal against a state-table entry's declared type (the trace-mock
/// rule, `E-TRACE-MOCK-TYPE`): `bool` takes `true`/`false`, `number` a
/// number, `enum` a member of its domain; every other type the text.
pub fn typed_literal(entry: &Json, lit: &str) -> Result<Value, String> {
    match entry.get("type").and_then(Json::as_str) {
        Some("bool") => match lit {
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            _ => Err("a `bool` is `true` or `false`".to_string()),
        },
        Some("number") => lit
            .parse::<f64>()
            .map(Value::Num)
            .map_err(|_| "a `number` takes a number".to_string()),
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
                Err(format!(
                    "the enum's members are {}{}",
                    domain.join(", "),
                    did_you_mean(lit, domain.iter().copied())
                ))
            }
        }
        _ => Ok(Value::Str(lit.to_string())),
    }
}

/// ` — did you mean `x`?` for the member nearest a misspelt `lit`, or empty.
fn did_you_mean<'a>(lit: &str, members: impl IntoIterator<Item = &'a str>) -> String {
    lute_manifest::suggest::nearest(lit, members, 2)
        .map(|k| format!(" — did you mean `{k}`?"))
        .unwrap_or_default()
}

/// dsl 0.27.0 §2 (T1-2): `lit` against a closed domain `K` (a named enum or
/// an entity kind with `members:`) — `Err` names the members and the one
/// nearest the literal (`` `rne` is not a member of `route` (ren, mika) —
/// did you mean `ren`? ``).
pub fn member_of(domain: &str, members: &[String], lit: &str) -> Result<(), String> {
    if members.iter().any(|m| m == lit) {
        return Ok(());
    }
    Err(format!(
        "`{lit}` is not a member of `{domain}` ({}){}",
        members.join(", "),
        did_you_mean(lit, members.iter().map(String::as_str))
    ))
}

/// A literal written to the declared state path `path` (its state-table
/// `entry`): [`typed_literal`], and — a path typed `{ domain: K }` /
/// `{ entity: K }` over a closed `K` — a member of `K` ([`member_of`]).
pub fn state_literal(
    p: &ExecProject,
    path: &str,
    entry: &Json,
    lit: &str,
) -> Result<Value, String> {
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
) -> Result<Value, String> {
    use lute_manifest::types::Type;
    match ty {
        Type::Bool => typed_literal(&serde_json::json!({ "type": "bool" }), lit),
        Type::Number => typed_literal(&serde_json::json!({ "type": "number" }), lit),
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
) -> Result<BTreeMap<String, Value>, String> {
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
        return Err(format!(
            "`payload` — occasion `{occasion}` declares no `payload:`"
        ));
    };
    for (field, lit) in fields {
        let Some(ty) = declared.get(field) else {
            return Err(format!(
                "`payload.{field}` — occasion `{occasion}` declares no payload field `{field}` \
                 (declared: {})",
                declared
                    .keys()
                    .map(|k| format!("`{k}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        };
        let value =
            payload_value(p, ty, lit).map_err(|why| format!("`payload.{field}: {lit}` — {why}"))?;
        out.insert(
            format!("{}.{field}", lute_check::occasion_bind::OCCASION_PAYLOAD),
            value,
        );
    }
    match missing_payload(occasion, declared, |f| fields.iter().any(|(k, _)| k == f)) {
        Some(why) => Err(format!(
            "{why} — add `payload: {{ {} }}` to the step",
            payload_example(declared)
        )),
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
        Type::Number => "<number>".to_string(),
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
pub fn resolve_state(p: &ExecProject, path: &str, lit: &str) -> Result<Value, String> {
    if path.starts_with("scene.") {
        return Err(
            "is `scene.*`, which resets at every scene boundary and cannot be written".to_string(),
        );
    }
    if let Some((id, _)) = entry_flag(path) {
        if !p.entry_ids.contains(id) {
            return Err(format!("names entry `{id}`, which no document declares"));
        }
        return match lit {
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            _ => Err(format!("is a bool: `{lit}` is not `true` or `false`")),
        };
    }
    // dsl 0.23.0 §6: a save made after a run ended carries `prev.run.*`,
    // typed by the `run.*` path it mirrors.
    let declared = match path.strip_prefix("prev.") {
        Some(run) if run.starts_with("run.") => run,
        _ => path,
    };
    let Some(entry) = p.state_table.get(declared) else {
        return Err("is not a declared state path in this project".to_string());
    };
    state_literal(p, declared, entry, lit).map_err(|why| format!("does not take `{lit}`: {why}"))
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
) -> Result<BTreeMap<String, VecDeque<crate::BridgeAnswer>>, String> {
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
            let hint = lute_manifest::suggest::nearest(tag, calls.keys().copied(), 2)
                .map(|k| format!(" — did you mean `{k}`?"))
                .unwrap_or_default();
            return Err(format!(
                "{at}: `bridges.{tag}` answers no plugin call — no document of this project calls \
                 `::{tag}` with an effect that reads a bridge result{hint}"
            ));
        };
        let reads = fields.keys().copied().collect::<Vec<_>>().join(", ");
        for (i, answer) in answers.iter().enumerate() {
            let n = i + 1;
            for (field, lit) in answer {
                let Some(paths) = fields.get(field.as_str()) else {
                    return Err(format!(
                        "{at}: `bridges.{tag}` answer {n} gives `{field}`, which no effect of the \
                         `::{tag}` calls reads (they read: {reads})"
                    ));
                };
                for path in paths {
                    if let Some(entry) = p.state_table.get(*path) {
                        state_literal(p, path, entry, lit).map_err(|why| {
                            format!(
                                "{at}: `bridges.{tag}` answer {n}: `{field}: {lit}` does not fit \
                                 `{path}` — {why}"
                            )
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
                return Err(format!(
                    "{at}: `bridges.{tag}` answer {n} lacks `{missing}`, which content reads — an \
                     answer gives every bridge result `::{tag}` content reads: `{shape}`"
                ));
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
pub fn resolve_fact(p: &ExecProject, f: &str, next: Option<&str>) -> Result<Fact, String> {
    let fact = parse_ground_fact(f)
        .filter(|(_, args)| args.iter().all(|a| !a.is_empty() && a != "_"))
        .ok_or_else(|| {
            format!(
                "is not a ground fact `rel(arg, …)`{}",
                split_atom_hint(f, next)
            )
        })?;
    if p.index
        .relations
        .iter()
        .any(|r| r.name == fact.0 && r.derive)
    {
        return Err("is derived by rules and cannot be asserted".to_string());
    }
    match atom_problem(p, &fact.0, &fact.1) {
        Some(why) => Err(why),
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
    let open = |s: &str| s.matches('(').count() as isize - s.matches(')').count() as isize;
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
