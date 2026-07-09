//! §5 pass 6 — addressing + identity. `addr` is regenerated each compile (a
//! position); `lineId`/`voiceKey` are stable content joins derived from the
//! per-speaker `code` (dsl §12's Yarn `#line:` model — `lute tag` persists
//! codes into source; this pass only back-fills the not-yet-tagged remainder
//! deterministically, never rewriting source).

use std::collections::BTreeMap;

use lute_core_span::{Diagnostic, Layer, Severity, Span};

use crate::cfg::{Label, Rec};
use crate::ir::Command;

/// One addressing unit's emitted records + labels left trailing past its
/// end, plus the lineId-identity PREFIX for this unit (§4/§5.6, D7): a scene
/// caller sets `{character}.{episodeId}` on EVERY shot (one continuous
/// document-wide identity scope, byte-identical to 0.1.0); the quest loop
/// sets `{questId}` per quest (one scope per quest — code back-fill counters
/// reset per quest, IR addendum §4).
pub struct ShotRecords {
    pub shot: i64,
    pub prefix: String,
    pub recs: Vec<Rec>,
    pub trailing: Vec<Label>,
}

/// Assign every `addr`, resolve every symbolic target, and stamp identity.
/// Returns the flat command array in final order. An unresolved label is a
/// compiler bug surfaced as `E-COMPILE-INTERNAL` (never a panic, D6 aborts).
pub fn assign_addresses(shots: Vec<ShotRecords>) -> (Vec<Command>, Vec<Diagnostic>) {
    let mut out: Vec<Command> = Vec::new();
    let mut diags: Vec<Diagnostic> = Vec::new();
    let mut segments: Vec<(String, usize)> = Vec::new();
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
        let count = shot.recs.len();
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
        segments.push((shot.prefix, count));
    }
    assign_identity(&mut out, &segments);
    (out, diags)
}

/// `"{shot:03}-{idx:04}"` with idx = (position+1) * 100 — the +100 gaps leave
/// room to hand-insert a row (§4.2).
fn addr_of(shot: i64, position: usize) -> String {
    format!("{:03}-{:04}", shot, (position as i64 + 1) * 100)
}

/// `lineId` on every line + option label; `voiceKey` on voiced lines; codes
/// back-filled per speaker (max authored + 10 steps, `{:04}` — tag.rs's
/// scheme). `segments` describes each addressing unit's `(prefix, count)` in
/// EMISSION order (lengths sum to `cmds.len()`); ADJACENT segments sharing
/// the SAME prefix coalesce into one identity SCOPE (scene: every shot
/// shares one prefix, so the whole document folds into ONE continuous scope
/// — byte-identical to 0.1.0's single-pass behavior); a prefix change starts
/// a FRESH scope (its own Pass-1 max-code map, own back-fill counter — a
/// quest's per-quest prefix change resets identity per quest, IR addendum §4).
fn assign_identity(cmds: &mut [Command], segments: &[(String, usize)]) {
    let mut offset = 0usize;
    let mut i = 0usize;
    while i < segments.len() {
        let mut len = segments[i].1;
        let mut j = i + 1;
        while j < segments.len() && segments[j].0 == segments[i].0 {
            len += segments[j].1;
            j += 1;
        }
        assign_identity_scope(&mut cmds[offset..offset + len], &segments[i].0);
        offset += len;
        i = j;
    }
}

/// One identity scope's Pass 1 (per-speaker highest AUTHORED numeric code,
/// scoped to `cmds`) + Pass 2 (final record order: fill codes, derive ids
/// under `prefix`).
fn assign_identity_scope(cmds: &mut [Command], prefix: &str) {
    let mut max_code: BTreeMap<String, u64> = BTreeMap::new();
    for cmd in cmds.iter() {
        if let Command::Line(l) = cmd {
            if let Some(n) = l.code.as_deref().and_then(|c| c.trim().parse::<u64>().ok()) {
                let e = max_code.entry(l.speaker.clone()).or_insert(0);
                if n > *e {
                    *e = n;
                }
            }
        }
    }
    for cmd in cmds.iter_mut() {
        match cmd {
            Command::Line(l) => {
                let code = match &l.code {
                    Some(c) => c.trim().to_string(),
                    None => {
                        // Back-fill this speaker's next code (max authored + 10).
                        // A speaker whose counter overflows at/near u64::MAX fails
                        // closed for THIS line only (`continue`, not `break`), so
                        // other speakers/lines still get identities and no colliding
                        // code is emitted — mirroring lute-check's tag.rs.
                        let e = max_code.entry(l.speaker.clone()).or_insert(0);
                        let Some(nc) = e.checked_add(10) else {
                            continue;
                        };
                        *e = nc;
                        format!("{:04}", nc)
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
            Command::Hub(h) => {
                for o in &mut h.options {
                    o.line_id = format!("{prefix}.{}.{}", h.id, o.id);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{LineCmd, Role, Stamp};

    fn line(speaker: &str, code: Option<&str>) -> Command {
        Command::Line(LineCmd {
            addr: String::new(),
            role: Role::Dialogue,
            speaker: speaker.to_string(),
            text: String::new(),
            emotion: None,
            variant: None,
            action: None,
            dialog_motion: None,
            as_label: None,
            line_id: String::new(),
            voice_key: None,
            placeholders: Vec::new(),
            code: code.map(str::to_string),
            stamp: Stamp::default(),
        })
    }

    fn as_line(cmd: &Command) -> &LineCmd {
        match cmd {
            Command::Line(l) => l,
            _ => panic!("expected Command::Line"),
        }
    }

    /// An authored `code` with surrounding whitespace (the attr parser preserves
    /// quoted whitespace) must be trimmed for BOTH the per-speaker max and the
    /// derived identity — mirroring `lute-check`'s `tag.rs`. So ` 0050 ` counts
    /// as 50 (a later untagged line back-fills to 0060, not 0010) and its own
    /// `lineId`/`voiceKey` use the trimmed `0050`.
    #[test]
    fn whitespaced_authored_code_is_trimmed_for_max_and_identity() {
        let segments = [("bardstale.s01ep02".to_string(), 2usize)];
        let mut cmds = vec![line("fixer", Some(" 0050 ")), line("fixer", None)];
        assign_identity(&mut cmds, &segments);

        let tagged = as_line(&cmds[0]);
        assert_eq!(tagged.code.as_deref(), Some("0050"));
        assert_eq!(tagged.line_id, "bardstale.s01ep02.fixer_0050");
        assert_eq!(tagged.voice_key.as_deref(), Some("fixer-0050"));

        let untagged = as_line(&cmds[1]);
        assert_eq!(untagged.code.as_deref(), Some("0060"));
        assert_eq!(untagged.line_id, "bardstale.s01ep02.fixer_0060");
        assert_eq!(untagged.voice_key.as_deref(), Some("fixer-0060"));
    }

    /// A speaker's authored `code` at `u64::MAX` followed by an untagged line for
    /// the SAME speaker: the counter overflows on back-fill, so — mirroring
    /// `lute-check`'s `tag.rs` (`code_at_u64_max_fails_closed_no_collision`) — the
    /// untagged line fails closed (no back-filled code, hence no derived identity
    /// and no colliding code emitted). Never panics, never wraps.
    #[test]
    fn code_at_u64_max_fails_closed_no_collision() {
        let segments = [("bardstale.s01ep02".to_string(), 2usize)];
        let mut cmds = vec![
            line("fixer", Some("18446744073709551615")),
            line("fixer", None),
        ];
        assign_identity(&mut cmds, &segments);

        // Authored u64::MAX line keeps its code and derives identity normally.
        let tagged = as_line(&cmds[0]);
        assert_eq!(tagged.code.as_deref(), Some("18446744073709551615"));
        assert_eq!(
            tagged.line_id,
            "bardstale.s01ep02.fixer_18446744073709551615"
        );
        assert_eq!(
            tagged.voice_key.as_deref(),
            Some("fixer-18446744073709551615")
        );

        // Untagged line fails closed: no back-filled code, no identity, no
        // colliding code emitted (tag.rs's u64::MAX semantics).
        let untagged = as_line(&cmds[1]);
        assert_eq!(untagged.code, None);
        assert_eq!(untagged.line_id, "");
        assert_eq!(untagged.voice_key, None);
    }

    /// Two addressing units with DIFFERENT prefixes (IR addendum §4, D7):
    /// the SAME (speaker, no-code) line in each unit gets a DISTINCT
    /// `{prefix}.{speaker}_{code}` id, and the back-fill code counter RESETS
    /// per prefix — unit 2's first back-filled code is NOT continued from
    /// unit 1 (both back-fill to `0010`, the scope's own first step).
    #[test]
    fn code_backfill_counter_resets_per_prefix_scope() {
        let segments = [("questA".to_string(), 1usize), ("questB".to_string(), 1usize)];
        let mut cmds = vec![line("narrator", None), line("narrator", None)];
        assign_identity(&mut cmds, &segments);

        let first = as_line(&cmds[0]);
        assert_eq!(first.code.as_deref(), Some("0010"));
        assert_eq!(first.line_id, "questA.narrator_0010");

        let second = as_line(&cmds[1]);
        assert_eq!(second.code.as_deref(), Some("0010"));
        assert_eq!(second.line_id, "questB.narrator_0010");
    }
}
