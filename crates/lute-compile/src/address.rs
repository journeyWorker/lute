//! §5 pass 6 — addressing + identity. `addr` is regenerated each compile (a
//! position); `lineId`/`voiceKey` are stable content joins derived from the
//! per-speaker `code` (dsl §12's Yarn `#line:` model — `lute tag` persists
//! codes into source; this pass only back-fills the not-yet-tagged remainder
//! deterministically, never rewriting source).

use std::collections::BTreeMap;

use lute_core_span::{Diagnostic, Layer, Severity, Span};

use crate::cfg::{Label, Rec};
use crate::ir::Command;

/// One shot's emitted records + labels left trailing past its end.
pub struct ShotRecords {
    pub shot: i64,
    pub recs: Vec<Rec>,
    pub trailing: Vec<Label>,
}

/// Identity context for `lineId`/`voiceKey` derivation (§4.2).
pub struct IdCx<'a> {
    pub character: &'a str,
    pub season: i64,
    pub episode: i64,
}

/// Assign every `addr`, resolve every symbolic target, and stamp identity.
/// Returns the flat command array in final order. An unresolved label is a
/// compiler bug surfaced as `E-COMPILE-INTERNAL` (never a panic, D6 aborts).
pub fn assign_addresses(shots: Vec<ShotRecords>, cx: &IdCx<'_>) -> (Vec<Command>, Vec<Diagnostic>) {
    let mut out: Vec<Command> = Vec::new();
    let mut diags: Vec<Diagnostic> = Vec::new();
    for shot in shots {
        // Label -> concrete addr (labels are per-shot, so the map is too).
        let mut labels: BTreeMap<u32, String> = BTreeMap::new();
        for (i, rec) in shot.recs.iter().enumerate() {
            let addr = addr_of(shot.shot, i);
            for l in &rec.labels {
                labels.insert(l.0, addr.clone());
            }
        }
        // End-of-shot converge: one past the last record (spec-gap note 2).
        let past_end = addr_of(shot.shot, shot.recs.len());
        for l in &shot.trailing {
            labels.insert(l.0, past_end.clone());
        }
        for (i, mut rec) in shot.recs.into_iter().enumerate() {
            *rec.cmd.addr_mut() = addr_of(shot.shot, i);
            rec.cmd.for_each_target(&mut |t: &mut String| {
                if let Some(n) = Label::parse_sym(t) {
                    match labels.get(&n) {
                        Some(addr) => *t = addr.clone(),
                        None => diags.push(internal(format!(
                            "unresolved control-flow label `@{n}` in shot {}",
                            shot.shot
                        ))),
                    }
                }
            });
            out.push(rec.cmd);
        }
    }
    assign_identity(&mut out, cx);
    (out, diags)
}

/// `"{shot:03}-{idx:04}"` with idx = (position+1) * 100 — the +100 gaps leave
/// room to hand-insert a row (§4.2).
fn addr_of(shot: i64, position: usize) -> String {
    format!("{:03}-{:04}", shot, (position as i64 + 1) * 100)
}

/// `lineId` on every line + option label; `voiceKey` on voiced lines; codes
/// back-filled per speaker (max authored + 10 steps, `{:04}` — tag.rs's
/// scheme).
fn assign_identity(cmds: &mut [Command], cx: &IdCx<'_>) {
    // Pass 1: per-speaker highest AUTHORED numeric code.
    let mut max_code: BTreeMap<String, u64> = BTreeMap::new();
    for cmd in cmds.iter() {
        if let Command::Line(l) = cmd {
            if let Some(n) = l.code.as_deref().and_then(|c| c.parse::<u64>().ok()) {
                let e = max_code.entry(l.speaker.clone()).or_insert(0);
                if n > *e {
                    *e = n;
                }
            }
        }
    }
    // Pass 2, final record order: fill codes, derive ids.
    let prefix = format!("{}.s{:02}ep{:02}", cx.character, cx.season, cx.episode);
    for cmd in cmds.iter_mut() {
        match cmd {
            Command::Line(l) => {
                let code = match &l.code {
                    Some(c) => c.clone(),
                    None => {
                        let e = max_code.entry(l.speaker.clone()).or_insert(0);
                        *e += 10;
                        format!("{:04}", *e)
                    }
                };
                l.line_id = format!("{prefix}.{}_{}", l.speaker, code);
                if l.role.voiced() {
                    // v1: voiceKey bank == characterId == the speaker (§11).
                    l.voice_key = Some(format!("{}-{}", l.speaker, code));
                }
                l.code = Some(code);
            }
            Command::Choice(c) => {
                for o in &mut c.options {
                    o.line_id = format!("{prefix}.{}.{}", c.branch_id, o.id);
                }
            }
            _ => {}
        }
    }
}

fn internal(message: String) -> Diagnostic {
    Diagnostic {
        code: "E-COMPILE-INTERNAL".to_string(),
        severity: Severity::Error,
        message,
        span: Span {
            byte_start: 0,
            byte_end: 0,
            line: 1,
            column: 1,
            utf16_range: (0, 0),
        },
        layer: Layer::Content,
        fixits: Vec::new(),
        provenance: None,
    }
}
