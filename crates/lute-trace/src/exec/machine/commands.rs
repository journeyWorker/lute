//! The content and effect commands: `accept`, `line` and staging records,
//! `::set` / `assert` / `retract` over the one write path, `skipped`
//! records, `barrier` and `end`.

use serde_json::{json, Value as Json};

use super::format::{substitute_markers, value_to_json};
use super::{addr, fold_op, Machine, Site, LINE_DELIVERY_KEYS};
use crate::eval::Read;
use crate::exec::driver::{Driver, SiteKind};
use crate::exec::store::{json_arg_to_string, render_fact};
use crate::{UnresolvedAtom, Value};

impl<D: Driver> Machine<D> {
    /// dsl 0.21.0 §7a.3 (`docs/runtime/quest-lifecycle.md`): the player
    /// accepts quest `quest` here. Recorded as `quest <id> accepted`; the
    /// activation itself belongs to the quest lifecycle — an accept-driven
    /// quest of THIS artifact activates on the next lifecycle round
    /// ([`Machine::is_accepted`]), and `lute play` hands the id to the quest
    /// documents' next advance. A quest this walk already knows to be past
    /// `unset` is left alone, and the record says so. dsl 0.24.0 §2: an
    /// accept with `applies: "nextRun"` is queued instead
    /// ([`Machine::accepted_next_run`]) and recorded with `at: "nextRun"`.
    pub(super) fn exec_accept(&mut self, cmd: &Json) {
        let quest = cmd
            .get("quest")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        let mut rec = serde_json::Map::new();
        rec.insert("position".into(), Json::String(addr(cmd).to_string()));
        rec.insert("kind".into(), Json::String("accept".into()));
        rec.insert("quest".into(), Json::String(quest.clone()));
        if cmd.get("applies").and_then(Json::as_str) == Some("nextRun") {
            rec.insert("at".into(), Json::String("nextRun".into()));
            self.driver.emit(Json::Object(rec));
            self.accepted_next_run.push(quest);
            return;
        }
        if let Some(state) = self
            .quest_status
            .get(&quest)
            .filter(|s| s.as_str() != "unset")
        {
            rec.insert("ignored".into(), Json::String(format!("already {state}")));
        }
        self.driver.emit(Json::Object(rec));
        self.accepted.push(quest);
    }

    /// An accept-driven quest's activation signal: a mock `accepts:` entry
    /// or an `accept` record this walk executed.
    pub(super) fn is_accepted(&self, id: &str) -> bool {
        self.seed.accepts.iter().any(|a| a == id) || self.accepted.iter().any(|a| a == id)
    }

    pub(super) fn rec_line(&mut self, cmd: &Json) {
        let speaker = cmd.get("speaker").and_then(Json::as_str).unwrap_or("");
        let raw = cmd.get("text").and_then(Json::as_str).unwrap_or("");
        let placeholders = cmd.get("placeholders").and_then(Json::as_array);
        // dsl 0.28.0 (T1-13): a payload field the raise did not carry has
        // no text to print; the driver decides whether the walk stops here.
        if let Some(path) = self.unset_payload(placeholders) {
            let atoms = [UnresolvedAtom::Path(path.clone())];
            self.unresolved.extend(atoms.iter().cloned());
            if self.at_unknown(
                Site::new(SiteKind::Payload, &path, addr(cmd)),
                &path,
                &atoms,
            ) {
                return;
            }
        }
        // A kind beat's `{{occasion.target}}` with no member bound has no
        // text to print either.
        if self.unbound_target(placeholders) {
            let target = lute_check::beats::OCCASION_TARGET;
            let atoms = [UnresolvedAtom::Path(target.to_string())];
            self.unresolved.extend(atoms.iter().cloned());
            if self.at_unknown(
                Site::new(SiteKind::OccasionTarget, target, addr(cmd)),
                target,
                &atoms,
            ) {
                return;
            }
        }
        // Every marker renders ONCE, by its global index in the plain text;
        // the line text and its segments both splice from that list.
        let rendered = self.render_markers(raw, placeholders);
        let text = substitute_markers(raw, &rendered, &mut 0);
        let mut rec = serde_json::Map::new();
        rec.insert("position".into(), Json::String(addr(cmd).to_string()));
        rec.insert("kind".into(), Json::String("line".into()));
        rec.insert("speaker".into(), Json::String(speaker.to_string()));
        rec.insert("text".into(), Json::String(text));
        // The line's identity and delivery ride the record verbatim, so a
        // `lute play --json` consumer never has to re-join the artifact to
        // know who spoke, how, and under which audio key. (`lute run`'s
        // driver drops them: its record is the conformance contract.)
        for key in LINE_DELIVERY_KEYS {
            if let Some(v) = cmd.get(key).filter(|v| !v.is_null()) {
                let v = if key == "segments" {
                    segments_with(v, &rendered)
                } else {
                    v.clone()
                };
                rec.insert(key.into(), v);
            }
        }
        self.driver.emit(Json::Object(rec));
    }
}

/// A modified line's `segments` (dsl 0.37.0 §3.6) with each text run's
/// `{{…}}` markers replaced from `rendered`, the line's markers rendered
/// once in plain-text order: the runs concatenate to the plain text, so a
/// run's first marker has the global index of every marker before it.
fn segments_with(segments: &Json, rendered: &[String]) -> Json {
    let Some(runs) = segments.as_array() else {
        return segments.clone();
    };
    let mut next = 0;
    let runs = runs
        .iter()
        .map(|run| {
            let mut run = run.clone();
            if let Some(text) = run.get("text").and_then(Json::as_str).map(str::to_string) {
                run["text"] = Json::String(substitute_markers(&text, rendered, &mut next));
            }
            run
        })
        .collect();
    Json::Array(runs)
}

impl<D: Driver> Machine<D> {
    pub(super) fn rec_stage(&mut self, cmd: &Json, kind: &str) {
        self.driver.emit(json!({
            "position": addr(cmd),
            "kind": kind,
        }));
    }

    /// `::set` (state-lifecycle.md): `=` stores the value; a compound op
    /// (`+=`, `-=`, `*=`, `/=`) folds it into the current value — an operand
    /// that is not a number (unknown, absent, another type) makes the result
    /// unknown, never a guessed `0` (D5). An unknown result is a
    /// [`SiteKind::SetValue`] site. dsl 0.28.0 §3: a `F[occasion.target]`
    /// target writes `F.<member>`, the member bound as `occasion.target`.
    pub(super) fn exec_set(&mut self, cmd: &Json) {
        let written = cmd.get("path").and_then(Json::as_str).unwrap_or("");
        let path = match lute_check::target_writes::indexed_family(written) {
            None => written.to_string(),
            Some(family) => {
                let site = Site::new(SiteKind::OccasionTarget, written, addr(cmd));
                let Some(member) = self.bound_target(site, written) else {
                    return;
                };
                format!("{family}.{member}")
            }
        };
        let op = cmd.get("op").and_then(Json::as_str).unwrap_or("=");
        let Some(rhs) = self.slot(cmd.get("value")) else {
            self.fatal = Some(format!(
                "set `{path}` has a malformed value (expected a CEL pair)"
            ));
            return;
        };
        let (rhs_value, mut atoms) = self.eval_atoms(&rhs);
        let new = if op == "=" {
            rhs_value
        } else {
            let cur = match self.store.read(&path) {
                Read::Value(v) => v,
                Read::Unset => {
                    atoms.push(UnresolvedAtom::Path(path.clone()));
                    Value::Unknown
                }
            };
            fold_op(op, &cur, &rhs_value)
        };
        if let Value::Error(message) = &new {
            self.fatal = Some(format!("set `{path}` failed: {message}"));
            return;
        }
        if new == Value::Unknown {
            let site = Site::new(SiteKind::SetValue, &path, addr(cmd));
            if self.at_unknown(site, &rhs.raw, &atoms) {
                return;
            }
        }
        let rec = json!({
            "position": addr(cmd),
            "kind": "set",
            "path": path,
            "value": value_to_json(&new),
        });
        self.write_recorded(&path, new, rec);
    }

    /// [`Machine::write`] whose record is emitted between the write and its
    /// exclusivity check, so an `exclusive` record follows the write that
    /// caused it.
    pub(super) fn write_recorded(&mut self, path: &str, v: Value, rec: Json) {
        let before = self.store.exclusive_now();
        self.store.write(path, v);
        self.driver.emit(rec);
        self.exclusive_check(&before);
    }

    /// dsl 0.28.0 §3: the member bound as `occasion.target` — what a write
    /// through it names (`F[occasion.target]`, `r(occasion.target)`, a
    /// plugin attribute). With none bound (the beat is not a kind or `for=`
    /// beat's, or a trace left it unmocked) the write is an [`UnknownSite`]
    /// at `site` and is not applied.
    ///
    /// [`UnknownSite`]: crate::exec::driver::UnknownSite
    pub(super) fn bound_target(&mut self, site: Site<'_>, raw: &str) -> Option<String> {
        match self.store.read(lute_check::beats::OCCASION_TARGET) {
            Read::Value(Value::Str(member)) => Some(member),
            _ => {
                let atoms = [UnresolvedAtom::Path(
                    lute_check::beats::OCCASION_TARGET.to_string(),
                )];
                self.unresolved.extend(atoms.iter().cloned());
                self.at_unknown(site, raw, &atoms);
                None
            }
        }
    }

    /// A fact record's `relation` and `args`, each `occasion.target` argument
    /// read as the bound member ([`Machine::bound_target`]); `None` when one
    /// is written with no member bound.
    fn fact_of(&mut self, cmd: &Json) -> Option<(String, Vec<String>)> {
        let rel = cmd
            .get("relation")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        let mut args: Vec<String> = cmd
            .get("args")
            .and_then(Json::as_array)
            .map(|a| a.iter().map(json_arg_to_string).collect())
            .unwrap_or_default();
        if args.iter().any(|a| a == lute_check::beats::OCCASION_TARGET) {
            let written = render_fact(&rel, &args);
            let site = Site::new(SiteKind::OccasionTarget, &written, addr(cmd));
            let member = self.bound_target(site, &written)?;
            for a in args
                .iter_mut()
                .filter(|a| *a == lute_check::beats::OCCASION_TARGET)
            {
                a.clone_from(&member);
            }
        }
        Some((rel, args))
    }

    pub(super) fn exec_assert(&mut self, cmd: &Json) {
        let Some((rel, args)) = self.fact_of(cmd) else {
            return;
        };
        let before = self.store.exclusive_now();
        self.store.assert((rel.clone(), args.clone()));
        self.driver.emit(json!({
            "position": addr(cmd),
            "kind": "assert",
            "fact": render_fact(&rel, &args),
        }));
        self.exclusive_check(&before);
    }

    pub(super) fn exec_retract(&mut self, cmd: &Json) {
        let Some((rel, args)) = self.fact_of(cmd) else {
            return;
        };
        let before = self.store.exclusive_now();
        // `_` positions are a bulk wildcard over the ground positions.
        self.store.retract(&rel, &args);
        self.driver.emit(json!({
            "position": addr(cmd),
            "kind": "retract",
            "pattern": render_fact(&rel, &args),
        }));
        self.exclusive_check(&before);
    }

    /// dsl 0.25.0 §1: a write that made exclusive relations hold together —
    /// even for a moment a later write undoes — is recorded at the write and
    /// refuses the walk (`lute trace` exit 1, `lute play` exit 1).
    pub(super) fn exclusive_check(&mut self, before: &[String]) {
        if !self.store.has_excludes() {
            return;
        }
        let new: Vec<String> = self
            .store
            .exclusive_now()
            .into_iter()
            .filter(|v| !before.contains(v))
            .collect();
        if new.is_empty() {
            return;
        }
        for v in &new {
            self.driver.emit(json!({ "kind": "exclusive", "text": v }));
        }
        self.refuse(format!(
            "exclusive relations hold together — {}",
            new.join("; ")
        ));
    }

    /// dsl 0.19.0 §6: a first-read-only effect record NOT applied on a
    /// re-read — recorded with the same identifying field its applied form
    /// carries (`path` / `fact` / `pattern`), never evaluated.
    pub(super) fn rec_skipped(&mut self, cmd: &Json, kind: &str) {
        let mut rec = serde_json::Map::new();
        rec.insert("position".into(), Json::String(addr(cmd).to_string()));
        rec.insert("kind".into(), Json::String("skipped".into()));
        rec.insert("effect".into(), Json::String(kind.to_string()));
        // A write through `occasion.target` names the member it would have
        // written, when one is bound.
        let member = match self.store.read(lute_check::beats::OCCASION_TARGET) {
            Read::Value(Value::Str(m)) => Some(m),
            _ => None,
        };
        match kind {
            "set" => {
                let path = cmd.get("path").and_then(Json::as_str).unwrap_or("");
                let path = match &member {
                    Some(m) => lute_check::target_writes::member_path(path, m),
                    None => path.to_string(),
                };
                rec.insert("path".into(), Json::String(path));
            }
            _ => {
                let rel = cmd.get("relation").and_then(Json::as_str).unwrap_or("");
                let args: Vec<String> = cmd
                    .get("args")
                    .and_then(Json::as_array)
                    .map(|a| a.iter().map(json_arg_to_string).collect())
                    .map(|args: Vec<String>| match &member {
                        Some(m) => args
                            .into_iter()
                            .map(|a| {
                                if a == lute_check::beats::OCCASION_TARGET {
                                    m.clone()
                                } else {
                                    a
                                }
                            })
                            .collect(),
                        None => args,
                    })
                    .unwrap_or_default();
                let key = if kind == "assert" { "fact" } else { "pattern" };
                rec.insert(key.into(), Json::String(render_fact(rel, &args)));
            }
        }
        self.driver.emit(Json::Object(rec));
    }

    pub(super) fn rec_barrier(&mut self, cmd: &Json) {
        self.driver.emit(json!({
            "position": addr(cmd),
            "kind": "barrier",
            "timeline": cmd.get("timeline").cloned().unwrap_or(Json::Null),
            "at": cmd.get("at").cloned().unwrap_or(Json::Null),
            "note": "timeline join — no real clock simulated",
        }));
    }

    /// Record the `end` record and mark the walk over (dsl 0.8.0). `reason` is
    /// optional in the IR; it rides the transcript as JSON `null` when absent so
    /// the machine record's key set never varies with authoring.
    pub(super) fn rec_end(&mut self, cmd: &Json) {
        self.terminated = true;
        self.driver.emit(json!({
            "position": addr(cmd),
            "kind": "end",
            "reason": cmd.get("reason").cloned().unwrap_or(Json::Null),
        }));
    }
}
