//! Timeline clip scheduling (§5 pass 5): REUSES `lute-check::timeline`'s
//! cursor/barrier math (`resolve_timeline`) and zips its rows — emitted one
//! per clip, per track, in document order — back onto the clip nodes, then
//! orders deterministically by `(at, track index, clip index)`. Invoked by
//! `stage.rs` DURING the CFG walk; never a separate pass.

use lute_check::{resolve_timeline, Ctx};
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_syntax::ast::{ClipNode, Timeline};

/// One clip with its resolved absolute placement.
pub struct ScheduledClip<'a> {
    pub at: f64,
    pub duration: f64,
    pub track: usize,
    pub node: &'a ClipNode,
}

/// Resolve and order a `<timeline>`'s clips; returns them with `barrier_at`
/// (authored `duration` when present, else max resolved clip end — §11.4).
/// Total: a row/clip count mismatch (impossible by construction) degrades to
/// the zipped prefix.
pub fn schedule_timeline<'a>(
    tl: &'a Timeline,
    ctx: &Ctx<'_>,
    snapshot: &CapabilitySnapshot,
) -> (Vec<ScheduledClip<'a>>, f64) {
    let (resolved, _diags) = resolve_timeline(tl, ctx, snapshot);
    let mut rows = resolved.rows.iter();
    let mut clips = Vec::new();
    for (track_ix, track) in tl.tracks.iter().enumerate() {
        for clip in &track.clips {
            let Some(row) = rows.next() else { break };
            clips.push(ScheduledClip {
                at: row.at,
                duration: row.duration,
                track: track_ix,
                node: &clip.node,
            });
        }
    }
    // Deterministic playback order: (at, track index); the sort is stable, so
    // same-(at, track) clips keep document order.
    clips.sort_by(|a, b| a.at.total_cmp(&b.at).then(a.track.cmp(&b.track)));
    (clips, resolved.barrier_at)
}
