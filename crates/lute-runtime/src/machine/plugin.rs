//! `plugin` records (dsl 0.24.0 §5, 0.25.0 §7, 0.27.0 §4): the bridge call,
//! its declared effects and facts, and [`BridgeReads`] — what content reads
//! of the bridge results.

use std::collections::{BTreeMap, BTreeSet};

use lute_manifest::schema::{OccasionDecl, OccasionTarget};
use serde_json::{json, Value as Json};

use super::format::value_to_json;
use super::{addr, fold_op, Machine, Site};
use crate::driver::{BridgeCall, BridgeReply, Driver, SiteKind};
use crate::eval::Read;
use crate::store::{json_arg_to_string, json_to_value, render_fact};
use crate::Value;

/// dsl 0.25.0 §7: what content reads of the plugin calls' bridge results,
/// over a set of compiled artifacts ([`BridgeReads::of`]). A result field
/// no content reads MAY be left out of an answer.
#[derive(Clone, Debug, Default)]
pub struct BridgeReads {
    /// Every state path a CEL expression or a `{{…}}` placeholder reads.
    pub paths: BTreeSet<String>,
    /// Per plugin directive tag, the bridge result fields some call of it
    /// writes to a path in `paths` — what every answer to the tag gives
    /// (answers queue per tag, not per call), and what a hint lists.
    pub fields: BTreeMap<String, BTreeSet<String>>,
    /// dsl 0.26.0 §3.1, 0.41.0 §4.1: per plugin directive tag, the declared
    /// type of each bridge result field an effect of the tag reads, from the
    /// capability's `result:` shape (the index `bridgeResults`) — what types
    /// an answer whose landing path no state slot declares. Empty for
    /// `lute run` (an artifact carries no capability snapshot).
    pub result_types: BTreeMap<String, BTreeMap<String, lute_manifest::types::Type>>,
}

impl BridgeReads {
    pub fn of<'a>(arts: impl IntoIterator<Item = &'a Json> + Clone) -> Self {
        let mut paths = BTreeSet::new();
        for art in arts.clone() {
            ir_read_paths(art, false, &mut paths);
        }
        let mut fields: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for art in arts {
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
                        if paths.contains(path) {
                            fields
                                .entry(tag.to_string())
                                .or_default()
                                .insert(field.to_string());
                        }
                    }
                }
            }
        }
        BridgeReads {
            paths,
            fields,
            result_types: BTreeMap::new(),
        }
    }

    /// Whether content reads `field` of a `tag` call's result.
    pub fn reads(&self, tag: &str, field: &str) -> bool {
        self.fields.get(tag).is_some_and(|f| f.contains(field))
    }

    /// dsl 0.26.0 §3.1: the scalar spelling of the capability-declared type
    /// of `tag`'s result `field` — `bool`, `int`, `double`, or `string` for
    /// any other declared type.
    pub fn result_type(&self, tag: &str, field: &str) -> Option<&'static str> {
        use lute_manifest::types::Type;
        Some(match self.result_types.get(tag)?.get(field)? {
            Type::Bool => "bool",
            Type::Int => "int",
            Type::Double => "double",
            _ => "string",
        })
    }
}

impl<D: Driver> Machine<D> {
    /// A `plugin` command (bridge-protocol.md) and its declared effects (IR
    /// A12; T1-3). A `bridgeResult` effect reads the driver's answer to the
    /// call ([`Driver::bridge`]; dsl 0.24.0 §5): the answered values are
    /// typed by each result slot's declared type; a field the answer leaves
    /// out (dsl 0.25.0 §7: one no content reads), or every field of an
    /// unanswered call, is written UNKNOWN (D7) and listed in
    /// `unresolvedEffects`. An unanswered call whose results content reads is
    /// a [`SiteKind::BridgeResult`] site: a driver that halts stops the walk
    /// AT the call, before anything after it — a default arm over the result
    /// slot included — is walked.
    ///
    /// After the `plugin` record, every decided write — a literal (a
    /// resolved `fromAttr` included), an `op` fold, an answered result — goes
    /// through the one write path and is recorded as a `set` with `effectOf:
    /// <tag>`. `false` = the walk stops here (that halt, an answer that does
    /// not fit the call, or a write the exclusivity check refused).
    pub(crate) fn exec_plugin(&mut self, cmd: &Json) -> bool {
        // dsl 0.28.0 §3: a call passing `occasion.target` writes the member
        // the beat runs for — its effect paths, fact arguments and fields
        // name that member.
        let bound;
        let cmd = if mentions_target(cmd) {
            let tag = cmd.get("tag").and_then(Json::as_str).unwrap_or("");
            let site = Site::new(SiteKind::OccasionTarget, tag, addr(cmd));
            let Some(member) = self.bound_target(site, TARGET) else {
                return !self.stopped();
            };
            bound = bind_target(cmd, &member);
            &bound
        } else {
            cmd
        };
        let tag = cmd
            .get("tag")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        let effects = cmd
            .get("effects")
            .and_then(Json::as_array)
            .cloned()
            .unwrap_or_default();
        let reads: Vec<(String, String)> = effects
            .iter()
            .filter_map(|e| {
                let field = e.get("from")?.get("bridgeResult")?.as_str()?;
                let path = e.get("path").and_then(Json::as_str).unwrap_or("");
                Some((field.to_string(), path.to_string()))
            })
            .collect();
        // dsl 0.41.0 §5.5: the call's attributes as executed, `occasion.target`
        // bound — what every `plugin` record and the bridge await carry.
        let attrs = cmd.get("fields").and_then(Json::as_object);
        // 0.49.0 §3.1: the command's plugin stamps ride beside `attrs`, never
        // merged into them, and are omitted when the command has none.
        let stamps = cmd
            .get("stamps")
            .and_then(Json::as_object)
            .filter(|s| !s.is_empty());
        // 0.51.0 §4.6: the call's declared raises (`occasion.target` bound
        // above), omitted when it declares none.
        let raises = cmd
            .get("raises")
            .and_then(Json::as_array)
            .filter(|r| !r.is_empty())
            .map(|r| Json::Array(r.clone()));
        let (answer, host) = if reads.is_empty() {
            (None, crate::session::Writes::default())
        } else {
            let call = BridgeCall {
                tag: &tag,
                addr: addr(cmd),
                reads: &reads,
                attrs,
                stamps,
            };
            match self.driver.bridge(&call) {
                BridgeReply::Answer { fields, writes } => (Some(fields), writes),
                BridgeReply::Unanswered => (None, crate::session::Writes::default()),
            }
        };
        let attrs = Json::Object(attrs.cloned().unwrap_or_default());
        // dsl 0.25.0 §7: whether content reads one of this call's results.
        let read = reads
            .iter()
            .any(|(_, p)| self.bridge_reads.paths.contains(p));
        let halt = answer.is_none()
            && read
            && self.at_unknown(Site::new(SiteKind::BridgeResult, &tag, addr(cmd)), "", &[]);
        let answered = match &answer {
            Some(a) => match self.bridge_values(&tag, &reads, a) {
                Ok(values) => Some(values),
                Err(msg) => {
                    self.fatal = Some(msg);
                    return false;
                }
            },
            None if halt => {
                // The fields every answer to the tag gives (dsl 0.25.0 §7).
                let (fields, paths): (Vec<&str>, Vec<&str>) = reads
                    .iter()
                    .filter(|(f, _)| self.bridge_reads.reads(&tag, f))
                    .map(|(f, p)| (f.as_str(), p.as_str()))
                    .unzip();
                let mut rec = json!({
                    "position": addr(cmd),
                    "kind": "plugin",
                    "tag": tag,
                    "attrs": attrs,
                    "external": true,
                    "unresolvedEffects": paths,
                    "unanswered": fields,
                    "note": "external bridge call — no `bridges:` answer; halted at the call",
                });
                if let Some(stamps) = stamps {
                    rec["stamps"] = Json::Object(stamps.clone());
                }
                if let Some(raises) = raises.clone() {
                    rec["raises"] = raises;
                }
                self.driver.emit(rec);
                return false;
            }
            None => None,
        };
        let unresolved: Vec<&str> = reads
            .iter()
            .filter(|(f, _)| {
                !answered
                    .as_ref()
                    .is_some_and(|a| a.iter().any(|(af, _)| af == f))
            })
            .map(|(_, p)| p.as_str())
            .collect();
        let mut rec = serde_json::Map::new();
        rec.insert("position".into(), Json::String(addr(cmd).to_string()));
        rec.insert("kind".into(), Json::String("plugin".into()));
        rec.insert("tag".into(), Json::String(tag.clone()));
        rec.insert("attrs".into(), attrs);
        if let Some(stamps) = stamps {
            rec.insert("stamps".into(), Json::Object(stamps.clone()));
        }
        // 0.51.0 §4.6: the declared raises, `occasion.target` bound — the
        // host raises them after the presentation ends.
        if let Some(raises) = raises {
            rec.insert("raises".into(), raises);
        }
        rec.insert("external".into(), Json::Bool(true));
        rec.insert("unresolvedEffects".into(), json!(unresolved));
        match &answered {
            Some(values) => {
                let fields: Vec<Json> = values
                    .iter()
                    .map(|(f, v)| json!({ "field": f, "value": value_to_json(v) }))
                    .collect();
                rec.insert("answered".into(), Json::Array(fields));
                rec.insert(
                    "note".into(),
                    json!("external bridge call — answered from `bridges:`"),
                );
            }
            // A call the walk cannot make: its bridge results stay unresolved,
            // or it declares no effect at all (a fire-and-forget engine
            // action — nothing in the transcript stands in for it).
            None if !reads.is_empty() || (effects.is_empty() && !has_fact_effects(cmd)) => {
                rec.insert(
                    "note".into(),
                    json!("external bridge call — not invoked; bridgeResult effects unresolved"),
                );
            }
            // Only declared effects: each is its own `set` record naming the
            // call, so nothing about it is external.
            None => {}
        }
        self.driver.emit(Json::Object(rec));
        for e in &effects {
            let path = e.get("path").and_then(Json::as_str).unwrap_or("");
            let Some(from) = e.get("from") else {
                continue;
            };
            let value = if let Some(field) = from.get("bridgeResult").and_then(Json::as_str) {
                answered
                    .as_ref()
                    .and_then(|a| a.iter().find(|(f, _)| f == field))
                    .map_or(Value::Unknown, |(_, v)| v.clone())
            } else if let Some(op) = from.get("op").and_then(Json::as_str) {
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "a whole bridge increment from the plugin: past the i64 range `as` \
                              saturates, as before"
                )]
                let by = match (
                    self.store.types.get(path).map(String::as_str),
                    from.get("by").and_then(Json::as_str),
                    from.get("by").and_then(Json::as_f64),
                ) {
                    (Some("int"), Some(raw), _) => raw.parse::<i64>().map_or_else(
                        |_| Value::Error(format!("bridge increment `{raw}` is not an int")),
                        Value::Int,
                    ),
                    (Some("int"), None, Some(n)) if n.fract() == 0.0 => Value::Int(n as i64),
                    (Some("int"), None, Some(n)) => {
                        Value::Error(format!("bridge increment `{n}` is not an int"))
                    }
                    (Some("double") | _, _, Some(n)) => Value::Double(n),
                    _ => Value::Unknown,
                };
                let cur = match self.store.read(path) {
                    Read::Value(v) => v,
                    Read::Unset => Value::Unknown,
                };
                match op {
                    "increment" => fold_op("+=", &cur, &by),
                    "decrement" => fold_op("-=", &cur, &by),
                    _ => Value::Unknown,
                }
            } else {
                json_to_value(from).unwrap_or(Value::Unknown)
            };
            if value == Value::Unknown {
                // An unanswered result (D7), or a fold over no number.
                self.write(path, Value::Unknown);
            } else {
                let mut rec = json!({
                    "position": addr(cmd),
                    "kind": "set",
                    "path": path,
                    "value": value_to_json(&value),
                    "effectOf": tag,
                });
                // ML-F8: the write restates the answered bridge result.
                if let Some(field) = from.get("bridgeResult") {
                    rec["bridgeResult"] = field.clone();
                }
                self.write_recorded(path, value, rec);
            }
            if self.stopped() {
                return false;
            }
        }
        // dsl 0.27.0 §4: the declared fact effects, after the writes —
        // retracts, then asserts — through the one assert/retract path
        // (`::retract` / `::assert`'s), each recorded with `effectOf`.
        for (key, assert) in [("retracts", false), ("asserts", true)] {
            for f in cmd.get(key).and_then(Json::as_array).into_iter().flatten() {
                let (rel, args) = fact_record(f);
                let mut rec = serde_json::Map::new();
                rec.insert("position".into(), Json::String(addr(cmd).to_string()));
                rec.insert("effectOf".into(), Json::String(tag.clone()));
                self.write_fact(&rel, &args, assert, rec);
                if self.stopped() {
                    return false;
                }
            }
        }
        // dsl 0.41.0 §5.3: the host's writes with the answer, after the
        // call's declared effects — state, then facts, then retracts, then
        // accepts — each recorded with `effectOf: <tag>` and `by: "host"`.
        self.host_writes(cmd, &tag, &host)
    }

    /// dsl 0.41.0 §5.3: a bridge answer's `writes` at the call. `false` =
    /// the walk stops here (a write the exclusivity check refused, or an
    /// `add` to a path with no number).
    fn host_writes(&mut self, cmd: &Json, tag: &str, host: &crate::session::Writes) -> bool {
        use crate::session::Write;
        for (path, write) in &host.state {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "a host `add` to an int path: `as` drops the fraction and saturates past \
                          the i64 range, as before"
            )]
            let value = match write {
                Write::Set(v) => v.clone(),
                Write::Add(d) => match self.store.read(path) {
                    Read::Value(Value::Int(n)) => match n.checked_add(*d as i64) {
                        Some(n) => Value::Int(n),
                        None => {
                            self.fatal = Some(format!("`{path}` overflows adding {d}"));
                            return false;
                        }
                    },
                    Read::Value(Value::Double(n)) => Value::Double(n + d),
                    _ => {
                        self.fatal = Some(format!("`{path}` has no number value to add {d} to"));
                        return false;
                    }
                },
            };
            let rec = json!({
                "position": addr(cmd),
                "kind": "set",
                "path": path,
                "value": value_to_json(&value),
                "effectOf": tag,
                "by": "host",
            });
            self.write_recorded(path, value, rec);
            if self.stopped() {
                return false;
            }
        }
        for ((rel, args), assert) in host
            .facts
            .iter()
            .map(|f| (f, true))
            .chain(host.retract.iter().map(|f| (f, false)))
        {
            self.write_fact(rel, args, assert, host_rec(cmd, tag));
            if self.stopped() {
                return false;
            }
        }
        for quest in &host.accept {
            self.accept_quest(quest.clone(), host_rec(cmd, tag));
        }
        true
    }

    /// dsl 0.27.0 §4 + 0.19.0 §6: a plugin call in a lore entry re-read.
    /// Only a directive whose one behaviour is its declared effects is
    /// admitted there, and those effects apply on a first read only (as the
    /// entry's own `::set`): each is recorded `skipped`, naming the call.
    pub(crate) fn rec_skipped_plugin(&mut self, cmd: &Json) {
        let tag = cmd.get("tag").and_then(Json::as_str).unwrap_or("");
        for e in cmd
            .get("effects")
            .and_then(Json::as_array)
            .into_iter()
            .flatten()
        {
            let path = e.get("path").and_then(Json::as_str).unwrap_or("");
            self.driver
                .emit(json!({ "position": addr(cmd), "kind": "skipped",
                "effect": "set", "path": path, "effectOf": tag }));
        }
        for (key, effect, field) in [
            ("retracts", "retract", "pattern"),
            ("asserts", "assert", "fact"),
        ] {
            for f in cmd.get(key).and_then(Json::as_array).into_iter().flatten() {
                let (rel, args) = fact_record(f);
                let mut rec = serde_json::Map::new();
                rec.insert("position".into(), Json::String(addr(cmd).to_string()));
                rec.insert("kind".into(), json!("skipped"));
                rec.insert("effect".into(), json!(effect));
                rec.insert(field.into(), json!(render_fact(&rel, &args)));
                rec.insert("effectOf".into(), json!(tag));
                self.driver.emit(Json::Object(rec));
            }
        }
    }

    /// One `bridges:` answer against the call it answers (dsl 0.24.0 §5):
    /// only `bridgeResult` fields the call's effects read, every one content
    /// reads among them (dsl 0.25.0 §7), each a literal of its result slot's
    /// declared type. `(field, value)` in the effects' order; `Err` names the
    /// misfit (a usage error).
    fn bridge_values(
        &self,
        tag: &str,
        reads: &[(String, String)],
        answer: &crate::BridgeAnswer,
    ) -> Result<Vec<(String, Value)>, String> {
        let fields: Vec<&str> = reads.iter().map(|(f, _)| f.as_str()).collect();
        let at = format!("the `bridges.{tag}` answer to plugin call `{tag}`");
        if let Some((extra, _)) = answer.iter().find(|(f, _)| !fields.contains(&f.as_str())) {
            return Err(format!(
                "{at} gives `{extra}`, which no effect of the call reads (it reads: {})",
                fields.join(", ")
            ));
        }
        let mut out = Vec::with_capacity(reads.len());
        for (field, path) in reads {
            if out.iter().any(|(f, _): &(String, Value)| f == field) {
                continue;
            }
            let Some((_, lit)) = answer.iter().find(|(f, _)| f == field) else {
                if !self.bridge_reads.reads(tag, field) {
                    continue;
                }
                let read: Vec<&str> = fields
                    .iter()
                    .copied()
                    .filter(|f| self.bridge_reads.reads(tag, f))
                    .collect();
                return Err(format!(
                    "{at} lacks `{field}`, which content reads — an answer gives every bridge \
                     result of the call content reads ({})",
                    read.join(", ")
                ));
            };
            // dsl 0.26.0 §3.1: typed by the result slot, else by the bridge
            // capability's `result:` shape; an untyped answer is refused —
            // never stored as a string that no bool/number read can match.
            let ty = self
                .store
                .types
                .get(path)
                .map(String::as_str)
                .or_else(|| self.bridge_reads.result_type(tag, field));
            let Some(ty) = ty else {
                return Err(format!(
                    "{at}: `{field}` lands on `{path}`, which no state slot of this artifact \
                     declares, and no bridge capability declares a `result:` type for it — \
                     an untyped answer is refused"
                ));
            };
            let v = match ty {
                "bool" => match lit.as_str() {
                    "true" => Some(Value::Bool(true)),
                    "false" => Some(Value::Bool(false)),
                    _ => None,
                },
                "int" => lit.parse::<i64>().ok().map(Value::Int),
                "double" | "number" => lit.parse::<f64>().ok().map(Value::Double),
                _ => Some(Value::Str(lit.clone())),
            };
            let Some(v) = v else {
                return Err(format!(
                    "{at}: `{field}: {lit}` does not fit `{path}`, a `{ty}`"
                ));
            };
            out.push((field.clone(), v));
        }
        Ok(out)
    }
}

/// Every state path `v` (an artifact, or any part of one) reads: each
/// `path` leaf of an expression tree (an `expr` — conditions, `::set`
/// values, match arms, `ref` placeholders with their def inlined) and each
/// `path` placeholder of a `{{…}}`. Writes (`set` / effect `path`s) sit
/// outside both and are not reads.
fn ir_read_paths(v: &Json, in_expr: bool, out: &mut BTreeSet<String>) {
    match v {
        Json::Object(map) => {
            for (k, child) in map {
                match (k.as_str(), child) {
                    ("path", Json::String(p)) if in_expr => {
                        out.insert(p.clone());
                    }
                    ("placeholders", Json::Array(items)) => {
                        for ph in items {
                            if ph.get("kind").and_then(Json::as_str) == Some("path") {
                                if let Some(p) = ph.get("path").and_then(Json::as_str) {
                                    out.insert(p.to_string());
                                }
                            }
                            ir_read_paths(ph, in_expr, out);
                        }
                    }
                    _ => ir_read_paths(child, in_expr || k == "expr", out),
                }
            }
        }
        Json::Array(items) => {
            for i in items {
                ir_read_paths(i, in_expr, out);
            }
        }
        _ => {}
    }
}

/// dsl 0.27.0 §4: whether a `plugin` record carries declared fact effects.
fn has_fact_effects(cmd: &Json) -> bool {
    ["asserts", "retracts"].iter().any(|k| {
        cmd.get(k)
            .and_then(Json::as_array)
            .is_some_and(|a| !a.is_empty())
    })
}

/// One `plugin` record fact (`{ relation, args }`, dsl 0.27.0 §4).
fn fact_record(f: &Json) -> (String, Vec<String>) {
    let rel = f
        .get("relation")
        .and_then(Json::as_str)
        .unwrap_or("")
        .to_string();
    let args = f
        .get("args")
        .and_then(Json::as_array)
        .map(|a| a.iter().map(json_arg_to_string).collect())
        .unwrap_or_default();
    (rel, args)
}

const TARGET: &str = lute_manifest::semantics::beats::OCCASION_TARGET;

/// Whether a plugin record writes through `occasion.target`: an effect path
/// `F[occasion.target]`, a fact argument, a field given it, or a declared
/// raise for it (0.51.0 §4.6).
fn mentions_target(cmd: &Json) -> bool {
    let paths = cmd
        .get("effects")
        .and_then(Json::as_array)
        .into_iter()
        .flatten();
    let facts = ["asserts", "retracts"]
        .into_iter()
        .filter_map(|k| cmd.get(k).and_then(Json::as_array))
        .flatten();
    paths
        .filter_map(|e| e.get("path").and_then(Json::as_str))
        .any(|p| lute_manifest::semantics::target_writes::indexed_family(p).is_some())
        || facts
            .filter_map(|f| f.get("args").and_then(Json::as_array))
            .flatten()
            .any(|a| a.as_str() == Some(TARGET))
        || cmd
            .get("fields")
            .and_then(Json::as_object)
            .is_some_and(|f| f.values().any(|v| v.as_str() == Some(TARGET)))
        || raises_of(cmd).any(is_dynamic)
}

/// A plugin record's declared raises (0.51.0 §4.6).
fn raises_of(cmd: &Json) -> impl Iterator<Item = &Json> {
    cmd.get("raises")
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
}

/// A raise whose target the compiler marked `dynamic`: it reads
/// `occasion.target`. Only the mark decides — a static target that happens
/// to read `….occasion.target` is not bound.
fn is_dynamic(raise: &Json) -> bool {
    raise.get("dynamic").and_then(Json::as_bool) == Some(true)
}

/// 0.51.0 §4.6: why a record's `dynamic` raise is malformed, if one is. The
/// compiler marks only the `occasion.target` reference with the occasion's
/// domain prefix applied (`scene.occasion.target`; bare for a shape-only
/// target), so load refuses any other marked target, a marked raise without
/// one, and a mark on an untargeted occasion — binding would otherwise
/// rewrite an arbitrary target to a bare member. An occasion `occasions`
/// does not declare has no known prefix: its reference passes with any
/// prefix or none. A static raise is never bound and is not checked.
pub(crate) fn dynamic_raise_fault(
    cmd: &Json,
    occasions: &BTreeMap<String, OccasionDecl>,
) -> Option<String> {
    raises_of(cmd).filter(|r| is_dynamic(r)).find_map(|r| {
        let occasion = r.get("occasion").and_then(Json::as_str).unwrap_or("");
        let why = match (
            r.get("target").and_then(Json::as_str),
            occasions.get(occasion).map(|d| &d.target),
        ) {
            (_, Some(OccasionTarget::Shape(false))) => format!("`{occasion}` takes no target"),
            (None, _) => "it has no target".to_string(),
            (Some(target), Some(declared)) => {
                let expected = match declared {
                    OccasionTarget::Domain { prefix, .. } => format!("{prefix}.{TARGET}"),
                    OccasionTarget::Shape(_) => TARGET.to_string(),
                };
                if target == expected {
                    return None;
                }
                format!("its target `{target}` is not `{expected}`")
            }
            (Some(target), None) => {
                let prefixed = |p: &str| p.strip_suffix('.').is_some_and(|p| !p.is_empty());
                if target
                    .strip_suffix(TARGET)
                    .is_some_and(|p| p.is_empty() || prefixed(p))
                {
                    return None;
                }
                format!("its target `{target}` is not `{TARGET}` with a domain prefix or none")
            }
        };
        let kind = cmd.get("kind").and_then(Json::as_str).unwrap_or("?");
        let position = cmd.get("position").and_then(Json::as_str).unwrap_or("?");
        Some(format!(
            "malformed `{kind}` record at {position}: its raise of `{occasion}` is marked \
             `dynamic` but {why}"
        ))
    })
}

/// A dynamic raise target — the reference, a domain prefix applied
/// (`scene.occasion.target`) — with `member` in its place.
fn bound_raise_target(target: &str, member: &str) -> String {
    match target
        .strip_suffix(TARGET)
        .and_then(|p| p.strip_suffix('.'))
    {
        Some(prefix) => format!("{prefix}.{member}"),
        None => member.to_string(),
    }
}

/// The record with every write through `occasion.target` naming `member`.
fn bind_target(cmd: &Json, member: &str) -> Json {
    let mut cmd = cmd.clone();
    for e in cmd
        .get_mut("effects")
        .and_then(Json::as_array_mut)
        .into_iter()
        .flatten()
    {
        if let Some(Json::String(p)) = e.get_mut("path") {
            *p = lute_manifest::semantics::target_writes::member_path(p, member);
        }
    }
    for key in ["asserts", "retracts"] {
        for f in cmd
            .get_mut(key)
            .and_then(Json::as_array_mut)
            .into_iter()
            .flatten()
        {
            for a in f
                .get_mut("args")
                .and_then(Json::as_array_mut)
                .into_iter()
                .flatten()
            {
                if a.as_str() == Some(TARGET) {
                    *a = Json::String(member.to_string());
                }
            }
        }
    }
    if let Some(fields) = cmd.get_mut("fields").and_then(Json::as_object_mut) {
        for v in fields.values_mut() {
            if v.as_str() == Some(TARGET) {
                *v = Json::String(member.to_string());
            }
        }
    }
    for r in cmd
        .get_mut("raises")
        .and_then(Json::as_array_mut)
        .into_iter()
        .flatten()
        .filter(|r| is_dynamic(r))
    {
        if let Some(Json::String(t)) = r.get_mut("target") {
            *t = bound_raise_target(t, member);
        }
        // Bound: the record names the target the host raises.
        if let Some(r) = r.as_object_mut() {
            r.remove("dynamic");
        }
    }
    cmd
}

/// The identifying keys of a host-answer write record at plugin call `cmd`
/// (dsl 0.41.0 §5.3): its position, `effectOf: <tag>`, `by: "host"`.
fn host_rec(cmd: &Json, tag: &str) -> serde_json::Map<String, Json> {
    let mut rec = serde_json::Map::new();
    rec.insert("position".into(), Json::String(addr(cmd).to_string()));
    rec.insert("effectOf".into(), Json::String(tag.to_string()));
    rec.insert("by".into(), Json::String("host".into()));
    rec
}
