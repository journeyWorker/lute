//! The content and effect commands: `accept`, `line` and staging records,
//! `::set` / `assert` / `retract` over the one write path, `skipped`
//! records, `barrier` and `end`.

use std::collections::BTreeMap;

use serde_json::{json, Value as Json};

use super::format::{substitute_by_marker, substitute_markers, value_to_json};
use super::{addr, fold_op, Machine, Site, LINE_DELIVERY_KEYS};
use crate::driver::{Driver, MenuText, SiteKind};
use crate::eval::Read;
use crate::store::{json_arg_to_string, render_fact};
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
    pub(crate) fn exec_accept(&mut self, cmd: &Json) {
        let quest = cmd
            .get("quest")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        let mut rec = serde_json::Map::new();
        rec.insert("position".into(), Json::String(addr(cmd).to_string()));
        if cmd.get("applies").and_then(Json::as_str) == Some("nextRun") {
            rec.insert("kind".into(), Json::String("accept".into()));
            rec.insert("quest".into(), Json::String(quest.clone()));
            rec.insert("at".into(), Json::String("nextRun".into()));
            self.driver.emit(Json::Object(rec));
            self.accepted_next_run.push(quest);
            return;
        }
        self.accept_quest(quest, rec);
    }

    /// The one accept path (`::accept`, a host answer's `accept`, a reward
    /// effect): `rec` — the caller's identifying keys — gains `kind`,
    /// `quest` and, for a quest this walk already knows past `unset`,
    /// `ignored`; it is emitted and the id is accepted (an accept-driven
    /// quest of this artifact activates on the next lifecycle round).
    pub(crate) fn accept_quest(&mut self, quest: String, mut rec: serde_json::Map<String, Json>) {
        rec.insert("kind".into(), Json::String("accept".into()));
        rec.insert("quest".into(), Json::String(quest.clone()));
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

    /// The one assert/retract path (`::assert` / `::retract`, a plugin
    /// call's declared or host fact effects, a reward effect): the store
    /// write, then `rec` — the caller's identifying keys, gaining `kind` and
    /// `fact` / `pattern` — then the exclusivity check (dsl 0.25.0 §1). A
    /// retract's `_` positions are a bulk wildcard over the ground positions.
    pub(crate) fn write_fact(
        &mut self,
        rel: &str,
        args: &[String],
        assert: bool,
        mut rec: serde_json::Map<String, Json>,
    ) {
        let before = self.store.exclusive_now();
        let text = render_fact(rel, args);
        if assert {
            self.store.assert((rel.to_string(), args.to_vec()));
            rec.insert("kind".into(), json!("assert"));
            rec.insert("fact".into(), json!(text));
        } else {
            self.store.retract(rel, args);
            rec.insert("kind".into(), json!("retract"));
            rec.insert("pattern".into(), json!(text));
        }
        self.driver.emit(Json::Object(rec));
        self.exclusive_check(&before);
    }

    /// An accept-driven quest's activation signal: a mock `accepts:` entry
    /// or an `accept` record this walk executed.
    pub(crate) fn is_accepted(&self, id: &str) -> bool {
        self.seed.accepts.iter().any(|a| a == id) || self.accepted.iter().any(|a| a == id)
    }

    pub(crate) fn rec_line(&mut self, cmd: &Json) {
        let speaker = cmd.get("speaker").and_then(Json::as_str).unwrap_or("");
        let raw = cmd.get("text").and_then(Json::as_str).unwrap_or("");
        let placeholders = cmd.get("placeholders").and_then(Json::as_array);
        if self.text_halts(placeholders, addr(cmd)) {
            return;
        }
        // Every marker renders ONCE, by its global index in the plain text;
        // the line text, its translations and its segments all splice from
        // that list (the one rendering path, [`Machine::render_text`]).
        let (MenuText { text, texts }, rendered) =
            self.render_text(raw, placeholders, cmd.get("texts"));
        let mut rec = serde_json::Map::new();
        rec.insert("position".into(), Json::String(addr(cmd).to_string()));
        rec.insert("kind".into(), Json::String("line".into()));
        rec.insert("speaker".into(), Json::String(speaker.to_string()));
        rec.insert("text".into(), Json::String(text));
        // The line's identity, delivery and rendered translations ride the
        // record, so a `lute play --json` consumer or a player never has to
        // re-join the artifact to know who spoke, how, under which audio key
        // and what each locale reads. (`lute run`'s driver drops them: its
        // record is the conformance contract.)
        // dsl 0.47.0 §2.2: a translation's markers take the value the source
        // rendered for the marker with the same referent.
        for key in LINE_DELIVERY_KEYS {
            if let Some(v) = cmd.get(key).filter(|v| !v.is_null()) {
                let v = match key {
                    "segments" => segments_with(v, &rendered),
                    "texts" => texts_json(&texts),
                    "localeSegments" => {
                        per_locale(v, |runs| locale_segments_with(runs, raw, &rendered))
                    }
                    _ => v.clone(),
                };
                rec.insert(key.into(), v);
            }
        }
        self.driver.emit(Json::Object(rec));
    }
}

/// A rendered translation map ([`MenuText::texts`]) as the record's `texts`
/// object — a line's, or a directive `text` attribute's (0.51.0 §5.6).
pub(crate) fn texts_json(texts: &BTreeMap<String, String>) -> Json {
    Json::Object(
        texts
            .iter()
            .map(|(locale, text)| (locale.clone(), Json::String(text.clone())))
            .collect(),
    )
}

/// A `texts` / `localeSegments` map (locale → translation) with each
/// locale's translation passed through `render`.
fn per_locale(map: &Json, render: impl Fn(&Json) -> Json) -> Json {
    match map.as_object() {
        Some(locales) => Json::Object(
            locales
                .iter()
                .map(|(locale, v)| (locale.clone(), render(v)))
                .collect(),
        ),
        None => map.clone(),
    }
}

/// A translation's `segments` with each text run's `{{…}}` markers replaced
/// by the value the source (`raw`, its markers `rendered`) gave the marker
/// with the same referent.
fn locale_segments_with(segments: &Json, raw: &str, rendered: &[String]) -> Json {
    let Some(runs) = segments.as_array() else {
        return segments.clone();
    };
    let runs = runs
        .iter()
        .map(|run| {
            let mut run = run.clone();
            if let Some(text) = run.get("text").and_then(Json::as_str) {
                let text = substitute_by_marker(text, raw, rendered);
                run["text"] = Json::String(text);
            }
            run
        })
        .collect();
    Json::Array(runs)
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

/// The resolved command fields a staging record of `kind` carries as
/// `attrs` (0.42.0 §4.2), in their IR (serialized) names.
fn staging_fields(kind: &str) -> &'static [&'static str] {
    match kind {
        "bg" => &["location", "time", "assetId"],
        "music" => &["playback", "stop", "mood", "volume", "assetId"],
        "sfx" => &["sound", "assetId"],
        "vfx" => &["type", "label", "transition"],
        "actor" => &[
            "character",
            "anchor",
            "action",
            "exit",
            "emotion",
            "costume",
            "posReset",
            "preload",
        ],
        "camera" => &["focus", "framing", "move", "transition"],
        "cg" => &["assetId", "display", "layout"],
        "video" => &["assetId", "display"],
        "sequence" => &["name"],
        _ => &[],
    }
}

impl<D: Driver> Machine<D> {
    /// A staging record (dsl 0.37.0 §5.1) with its resolved command fields
    /// as `attrs` (0.42.0 §4): the kind's own fields the command carries —
    /// authored values and the defaults lowering filled in — never the
    /// envelope (`kind`, `family`, `position`), `timing`, `provenance`,
    /// `source` or a plugin's cross-cutting stamp attrs. An absent optional
    /// field is omitted; `attrs` is always an object.
    pub(crate) fn rec_stage(&mut self, cmd: &Json, kind: &str) {
        let attrs: serde_json::Map<String, Json> = staging_fields(kind)
            .iter()
            .filter_map(|field| Some((field.to_string(), cmd.get(*field)?.clone())))
            .collect();
        let record = serde_json::json!({
            "kind": kind,
            "position": addr(cmd),
            "attrs": attrs,
        });
        self.stage.apply(kind, cmd);
        self.driver.emit(record);
    }

    /// `::set` (state-lifecycle.md): `=` stores the value; a compound op
    /// (`+=`, `-=`, `*=`, `/=`) folds it into the current value — an operand
    /// that is not a number (unknown, absent, another type) makes the result
    /// unknown, never a guessed `0` (D5). An unknown result is a
    /// [`SiteKind::SetValue`] site. dsl 0.28.0 §3: a `F[occasion.target]`
    /// target writes `F.<member>`, the member bound as `occasion.target`.
    pub(crate) fn exec_set(&mut self, cmd: &Json) {
        let written = cmd.get("path").and_then(Json::as_str).unwrap_or("");
        let path = match lute_manifest::semantics::target_writes::indexed_family(written) {
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
    pub(crate) fn write_recorded(&mut self, path: &str, v: Value, rec: Json) {
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
    /// [`UnknownSite`]: crate::driver::UnknownSite
    pub(crate) fn bound_target(&mut self, site: Site<'_>, raw: &str) -> Option<String> {
        match self
            .store
            .read(lute_manifest::semantics::beats::OCCASION_TARGET)
        {
            Read::Value(Value::Str(member)) => Some(member),
            _ => {
                let atoms = [UnresolvedAtom::Path(
                    lute_manifest::semantics::beats::OCCASION_TARGET.to_string(),
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
        if args
            .iter()
            .any(|a| a == lute_manifest::semantics::beats::OCCASION_TARGET)
        {
            let written = render_fact(&rel, &args);
            let site = Site::new(SiteKind::OccasionTarget, &written, addr(cmd));
            let member = self.bound_target(site, &written)?;
            for a in args
                .iter_mut()
                .filter(|a| *a == lute_manifest::semantics::beats::OCCASION_TARGET)
            {
                a.clone_from(&member);
            }
        }
        Some((rel, args))
    }

    pub(crate) fn exec_assert(&mut self, cmd: &Json) {
        self.exec_fact(cmd, true);
    }

    pub(crate) fn exec_retract(&mut self, cmd: &Json) {
        self.exec_fact(cmd, false);
    }

    fn exec_fact(&mut self, cmd: &Json, assert: bool) {
        let Some((rel, args)) = self.fact_of(cmd) else {
            return;
        };
        let mut rec = serde_json::Map::new();
        rec.insert("position".into(), Json::String(addr(cmd).to_string()));
        self.write_fact(&rel, &args, assert, rec);
    }

    /// dsl 0.25.0 §1: a write that made exclusive relations hold together —
    /// even for a moment a later write undoes — is recorded at the write and
    /// refuses the walk (`lute trace` exit 1, `lute play` exit 1).
    pub(crate) fn exclusive_check(&mut self, before: &[String]) {
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
    pub(crate) fn rec_skipped(&mut self, cmd: &Json, kind: &str) {
        let mut rec = serde_json::Map::new();
        rec.insert("position".into(), Json::String(addr(cmd).to_string()));
        rec.insert("kind".into(), Json::String("skipped".into()));
        rec.insert("effect".into(), Json::String(kind.to_string()));
        // A write through `occasion.target` names the member it would have
        // written, when one is bound.
        let member = match self
            .store
            .read(lute_manifest::semantics::beats::OCCASION_TARGET)
        {
            Read::Value(Value::Str(m)) => Some(m),
            _ => None,
        };
        match kind {
            "set" => {
                let path = cmd.get("path").and_then(Json::as_str).unwrap_or("");
                let path = match &member {
                    Some(m) => lute_manifest::semantics::target_writes::member_path(path, m),
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
                                if a == lute_manifest::semantics::beats::OCCASION_TARGET {
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

    pub(crate) fn rec_barrier(&mut self, cmd: &Json) {
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
    pub(crate) fn rec_end(&mut self, cmd: &Json) {
        self.terminated = true;
        self.driver.emit(json!({
            "position": addr(cmd),
            "kind": "end",
            "reason": cmd.get("reason").cloned().unwrap_or(Json::Null),
        }));
    }
}
