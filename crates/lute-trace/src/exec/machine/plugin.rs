//! `plugin` records (dsl 0.24.0 §5, 0.25.0 §7, 0.27.0 §4): the bridge call,
//! its declared effects and facts, and [`BridgeReads`] — what content reads
//! of the bridge results.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Value as Json};

use super::format::value_to_json;
use super::{addr, fold_op, Machine, Site};
use crate::eval::Read;
use crate::exec::driver::{BridgeCall, BridgeReply, Driver, SiteKind};
use crate::exec::store::{json_arg_to_string, json_to_value, render_fact};
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
    /// dsl 0.26.0 §3.1: per plugin directive tag, the declared type (`bool`,
    /// `number`, `string`) of each bridge result field an effect of the tag
    /// reads, from the capability's `result:` shape — what types an answer
    /// whose landing path no state slot declares. Empty for `lute run` (an
    /// artifact carries no capability snapshot).
    pub result_types: BTreeMap<String, BTreeMap<String, &'static str>>,
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

    /// dsl 0.26.0 §3.1: `snapshot`'s bridge result types, per directive tag
    /// ([`Self::result_types`]), merged into `self`.
    pub fn with_result_types(
        mut self,
        snapshot: &lute_manifest::snapshot::CapabilitySnapshot,
    ) -> Self {
        use lute_manifest::types::Type;
        for (tag, decl) in &snapshot.directives {
            let Some(bridge) = &decl.bridge else {
                continue;
            };
            let Some(cap) = snapshot
                .bridge_capabilities
                .get(&(bridge.service.clone(), bridge.operation.clone()))
            else {
                continue;
            };
            for (field, _) in crate::mock::bridge_result_writes(decl) {
                let Some(f) = cap.result.iter().find(|f| f.name == field) else {
                    continue;
                };
                let ty = match f.ty {
                    Type::Bool => "bool",
                    Type::Number => "number",
                    _ => "string",
                };
                self.result_types
                    .entry(tag.clone())
                    .or_default()
                    .insert(field.to_string(), ty);
            }
        }
        self
    }

    /// Whether content reads `field` of a `tag` call's result.
    pub fn reads(&self, tag: &str, field: &str) -> bool {
        self.fields.get(tag).is_some_and(|f| f.contains(field))
    }

    /// dsl 0.26.0 §3.1: the capability-declared type of `tag`'s result `field`.
    pub fn result_type(&self, tag: &str, field: &str) -> Option<&'static str> {
        self.result_types.get(tag)?.get(field).copied()
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
    pub(super) fn exec_plugin(&mut self, cmd: &Json) -> bool {
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
        let answer = if reads.is_empty() {
            None
        } else {
            let call = BridgeCall {
                tag: &tag,
                addr: addr(cmd),
                reads: &reads,
            };
            match self.driver.bridge(&call) {
                BridgeReply::Answer(a) => Some(a),
                BridgeReply::Unanswered => None,
            }
        };
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
                self.driver.emit(json!({
                    "addr": addr(cmd),
                    "kind": "plugin",
                    "tag": tag,
                    "external": true,
                    "unresolvedEffects": paths,
                    "unanswered": fields,
                    "note": "external bridge call — no `bridges:` answer; halted at the call",
                }));
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
        rec.insert("addr".into(), Json::String(addr(cmd).to_string()));
        rec.insert("kind".into(), Json::String("plugin".into()));
        rec.insert("tag".into(), Json::String(tag.clone()));
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
                    .map(|(_, v)| v.clone())
                    .unwrap_or(Value::Unknown)
            } else if let Some(op) = from.get("op").and_then(Json::as_str) {
                let by = from
                    .get("by")
                    .and_then(Json::as_f64)
                    .map_or(Value::Unknown, Value::Num);
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
                    "addr": addr(cmd),
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
                let before = self.store.exclusive_now();
                let rec = if assert {
                    self.store.assert((rel.clone(), args.clone()));
                    json!({ "addr": addr(cmd), "kind": "assert",
                            "fact": render_fact(&rel, &args), "effectOf": tag })
                } else {
                    self.store.retract(&rel, &args);
                    json!({ "addr": addr(cmd), "kind": "retract",
                            "pattern": render_fact(&rel, &args), "effectOf": tag })
                };
                self.driver.emit(rec);
                self.exclusive_check(&before);
                if self.stopped() {
                    return false;
                }
            }
        }
        true
    }

    /// dsl 0.27.0 §4 + 0.19.0 §6: a plugin call in a lore entry re-read.
    /// Only a directive whose one behaviour is its declared effects is
    /// admitted there, and those effects apply on a first read only (as the
    /// entry's own `::set`): each is recorded `skipped`, naming the call.
    pub(super) fn rec_skipped_plugin(&mut self, cmd: &Json) {
        let tag = cmd.get("tag").and_then(Json::as_str).unwrap_or("");
        for e in cmd
            .get("effects")
            .and_then(Json::as_array)
            .into_iter()
            .flatten()
        {
            let path = e.get("path").and_then(Json::as_str).unwrap_or("");
            self.driver
                .emit(json!({ "addr": addr(cmd), "kind": "skipped",
                "effect": "set", "path": path, "effectOf": tag }));
        }
        for (key, effect, field) in [
            ("retracts", "retract", "pattern"),
            ("asserts", "assert", "fact"),
        ] {
            for f in cmd.get(key).and_then(Json::as_array).into_iter().flatten() {
                let (rel, args) = fact_record(f);
                let mut rec = serde_json::Map::new();
                rec.insert("addr".into(), Json::String(addr(cmd).to_string()));
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
                "number" => lit.parse::<f64>().ok().map(Value::Num),
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
        Json::Array(items) => items.iter().for_each(|i| ir_read_paths(i, in_expr, out)),
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
