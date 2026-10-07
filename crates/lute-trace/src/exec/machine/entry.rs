//! Lore presentations: one `entry` (dsl 0.19.0, first-read effects) or one
//! bundle `beat` (dsl 0.23.0 §4), each run as the body segment its head
//! record opens.

use std::sync::Arc;

use serde_json::{json, Value as Json};

use super::{addr, cel_raw, Machine, Site};
use crate::eval::Read;
use crate::exec::driver::{Driver, SiteKind};
use crate::Value;



impl<D: Driver> Machine<D> {
    /// Present ONE lore entry (dsl 0.19.0 §6, `docs/runtime/lore-entries.md`
    /// `present()`): `firstRead = !entry.<id>.read`; run the body segment —
    /// from `body` up to the next `entry` or `beat` record (dsl 0.23.0 §4:
    /// a lore artifact interleaves both), the `<on>`-body
    /// termination rule — with `set`/`assert`/`retract` applied only on a
    /// first read; then, on a first read that ran to completion, the
    /// ENGINE's `entry.<id>.read = true` write. `when` is evaluated and
    /// recorded as `eligible` (true / false / null = unknown) on the `entry`
    /// transcript event, not enforced: `--entry` asks for the presentation.
    pub(super) fn run_entry(&mut self) {
        let id = self.entry.clone().unwrap_or_default();
        let Some(at) = self.code.commands.iter().position(|c| {
            c.get("kind").and_then(Json::as_str) == Some("entry")
                && c.get("id").and_then(Json::as_str) == Some(id.as_str())
        }) else {
            let declared: Vec<&str> = self
                .code
                .commands
                .iter()
                .filter(|c| c.get("kind").and_then(Json::as_str) == Some("entry"))
                .filter_map(|c| c.get("id").and_then(Json::as_str))
                .collect();
            self.fatal = Some(format!(
                "`--entry {id}` names no entry in this artifact (declared: {})",
                declared.join(", ")
            ));
            return;
        };
        let code = Arc::clone(&self.code);
        let cmd = &code.commands[at];
        // dsl 0.28.0 (T1-6): a `for="kind:<kind>"` entry is read per member
        // — the member bound as `occasion.target` has its own first read.
        let member = match self.store.read(lute_check::beats::OCCASION_TARGET) {
            Read::Value(Value::Str(m)) if cmd.get("forKind").is_some() => Some(m),
            _ => None,
        };
        let any_read = format!("entry.{id}.read");
        let read_path = match &member {
            Some(m) => crate::exec::cadence::entry_member_read_path(&id, m),
            None => any_read.clone(),
        };
        let first_read = self.store.read(&read_path) != Read::Value(Value::Bool(true));
        let eligible = match cel_raw(cmd.get("when")) {
            None => Json::Bool(true),
            Some(raw) => {
                let site = Site::new(SiteKind::EntryWhen, &id, addr(cmd));
                self.judge(raw, site).map(Json::Bool).unwrap_or(Json::Null)
            }
        };
        if self.stopped() {
            return;
        }
        self.driver.emit(json!({
            "position": addr(cmd),
            "kind": "entry",
            "id": id,
            "firstRead": first_read,
            "eligible": eligible,
        }));
        let body = cmd.get("body").and_then(Json::as_str).unwrap_or("");
        let start = self.resolve(body);
        let stop = self.segment_stop(at);
        self.apply_effects = first_read;
        self.run_range(start, stop);
        self.apply_effects = true;
        if !self.incomplete && self.fatal.is_none() {
            // D11: the engine's post-presentation flags, for every driver —
            // `read` on a first read, `everRead` on any completed read.
            if first_read {
                self.write(&read_path, Value::Bool(true));
                if member.is_some() {
                    self.write(&any_read, Value::Bool(true));
                }
            }
            self.write(&format!("entry.{id}.everRead"), Value::Bool(true));
        }
    }

    /// Where the body segment of the lore head record at `at` ends: the next
    /// `entry` or `beat` head record, or the end of the artifact.
    fn segment_stop(&self, at: usize) -> usize {
        self.code.commands[at + 1..]
            .iter()
            .position(|c| matches!(c.get("kind").and_then(Json::as_str), Some("entry" | "beat")))
            .map(|i| at + 1 + i)
            .unwrap_or(self.code.commands.len())
    }

    /// Present ONE bundle beat (dsl 0.23.0 §4): its body segment runs like a
    /// scene's — every effect applies (a beat is spent by presentation, not
    /// by a read flag). `when` is evaluated and recorded as `eligible` on the
    /// `beat` transcript event, not enforced, exactly as an entry's. The id
    /// matches the record's canonical `<document id>.<beat id>`, or its bare
    /// beat id.
    pub(super) fn run_bundle_beat(&mut self) {
        let id = self.bundle_beat.clone().unwrap_or_default();
        let suffix = format!(".{id}");
        let beats: Vec<usize> = (0..self.code.commands.len())
            .filter(|&i| self.code.commands[i].get("kind").and_then(Json::as_str) == Some("beat"))
            .collect();
        let record_id = |i: usize| {
            self.code.commands[i]
                .get("id")
                .and_then(Json::as_str)
                .unwrap_or("")
        };
        let at = beats
            .iter()
            .copied()
            .find(|&i| record_id(i) == id)
            .or_else(|| {
                beats
                    .iter()
                    .copied()
                    .find(|&i| record_id(i).ends_with(&suffix))
            });
        let Some(at) = at else {
            let declared: Vec<&str> = beats.iter().map(|&i| record_id(i)).collect();
            self.fatal = Some(format!(
                "`--beat {id}` names no beat in this artifact (declared: {})",
                declared.join(", ")
            ));
            return;
        };
        let code = Arc::clone(&self.code);
        let cmd = &code.commands[at];
        let canonical = cmd
            .get("id")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        let eligible = match cel_raw(cmd.get("when")) {
            None => Json::Bool(true),
            Some(raw) => {
                let site = Site::new(SiteKind::BeatWhen, &canonical, addr(cmd));
                self.judge(raw, site).map(Json::Bool).unwrap_or(Json::Null)
            }
        };
        if self.stopped() {
            return;
        }
        self.driver.emit(json!({
            "position": addr(cmd),
            "kind": "beat",
            "id": cmd.get("id").cloned().unwrap_or(Json::Null),
            "eligible": eligible,
        }));
        let body = cmd.get("body").and_then(Json::as_str).unwrap_or("");
        let start = self.resolve(body);
        let stop = self.segment_stop(at);
        self.run_range(start, stop);
    }
}
