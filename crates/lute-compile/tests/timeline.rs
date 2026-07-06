//! Inline timeline goldens (§5 pass 5): deterministic (at, track) order,
//! timeline/at/duration stamps, the trailing barrier, a stage-changing clip
//! threading the reducer, and post-barrier state carry.

use lute_check::ctx::Env;
use lute_check::StageState;
use lute_compile::cfg::{Emitter, Rec};
use lute_compile::stage::{walk_seq, WalkCx};
use lute_compile::Command;
use lute_core_span::Severity;

fn walk(body: &str) -> (Vec<Rec>, StageState) {
    let src =
        format!("---\ncharacter: bianca\nseason: 1\nepisode: 2\n---\n\n## Shot 1.\n\n{body}\n");
    let (doc, diags) = lute_syntax::parse(&src);
    assert!(
        diags.iter().all(|d| d.severity != Severity::Error),
        "{diags:#?}"
    );
    let snapshot = lute_manifest::core::load_core_snapshot();
    let env = Env::default();
    let mut cx = WalkCx {
        snapshot: &snapshot,
        env: &env,
        components: Vec::new(),
        timelines: 0,
    };
    let mut em = Emitter::default();
    let state = walk_seq(
        &mut em,
        &doc.shots[0].body,
        StageState::default(),
        &mut cx,
        &[],
    );
    let (recs, _) = em.finish();
    (recs, state)
}

// The bianca-s01ep02 performance beat (docs/examples, Shot 3), verbatim.
const BEAT: &str = r#"<timeline duration="1.4">
  <track subject="camera">
    ::camera{focus="bianca" zoom="1.35" duration="0.4"}
    ::camera{shake="0.6" duration="0.3" at="0.5"}
  </track>
  <track channel="fg">
    ::cut{assetId="CUT.x.01" at="0.5"}
    ::cut{assetId="CUT.x.01" action="hide" at="1.1"}
  </track>
  <track channel="vfx">
    ::vfx{type="whiteOut" transition="flash" at="0.5"}
  </track>
  <track channel="sfx">
    ::sfx{sound="beam" assetId="P_beam" at="0.5"}
  </track>
</timeline>
:narrator: After."#;

#[test]
fn clips_emit_in_at_then_track_order_with_stamps_and_barrier() {
    let (recs, _) = walk(BEAT);
    let desc: Vec<String> = recs
        .iter()
        .map(|r| match &r.cmd {
            Command::Camera(c) => format!("camera@{}", c.stamp.at.unwrap()),
            Command::Cut(c) => format!("cut@{}", c.stamp.at.unwrap()),
            Command::Vfx(c) => format!("vfx@{}", c.stamp.at.unwrap()),
            Command::Sfx(c) => format!("sfx@{}", c.stamp.at.unwrap()),
            Command::Barrier(b) => format!("barrier@{}", b.at),
            Command::Line(_) => "line".to_string(),
            other => panic!("unexpected {other:?}"),
        })
        .collect();
    assert_eq!(
        desc,
        vec![
            "camera@0", // zoom, omitted at => track cursor 0.0
            "camera@0.5",
            "cut@0.5",
            "vfx@0.5",
            "sfx@0.5",
            "cut@1.1",
            "barrier@1.4", // authored duration wins (§11.4)
            "line",
        ]
    );
    // Every clip record is stamped with the document-order timeline ordinal.
    for r in &recs[..6] {
        let mut c = r.cmd.clone();
        let stamp = c.stamp_mut().expect("clip records are stamped").clone();
        assert_eq!(stamp.timeline, Some(1));
    }
    // Durations stamp through (zoom clip: 0.4).
    let Command::Camera(zoom) = &recs[0].cmd else {
        panic!()
    };
    assert_eq!(zoom.stamp.duration, Some(0.4));
    let Command::Barrier(b) = &recs[6].cmd else {
        panic!()
    };
    assert_eq!(b.timeline, 1);
}

#[test]
fn stage_changing_clip_threads_the_reducer_and_carries_post_barrier_state() {
    // bianca is on stage; a ::bg clip INSIDE the timeline is a scene change:
    // the auto-hide injects as a timeline-stamped record, and the walker's
    // post-barrier state carries the new bg forward.
    let body = r#"::auto{character="bianca" anchor="center" action="fade-in-up"}
<timeline>
  <track channel="scene">
    ::bg{location="street" time="night"}
  </track>
</timeline>"#;
    let (recs, state) = walk(body);
    let kinds: Vec<&str> = recs
        .iter()
        .map(|r| match &r.cmd {
            Command::Sprite(s) if s.exit == Some(true) => "hide",
            Command::Sprite(_) => "sprite",
            Command::Background(_) => "background",
            Command::Barrier(_) => "barrier",
            _ => "other",
        })
        .collect();
    assert_eq!(kinds, vec!["sprite", "hide", "background", "barrier"]);
    // The injected hide is stamped as part of the timeline too.
    let Command::Sprite(h) = &recs[1].cmd else {
        panic!()
    };
    assert_eq!(h.stamp.timeline, Some(1));
    assert_eq!(h.stamp.at, Some(0.0));
    // Post-barrier carry: stage cleared, bg recorded.
    assert!(state.on_stage.is_empty());
    assert_eq!(state.bg.as_deref(), Some("street"));
}

#[test]
fn second_timeline_gets_ordinal_two_and_barrier_defaults_to_max_end() {
    let body = r#"<timeline>
  <track channel="sfx">
    ::sfx{sound="a"}
  </track>
</timeline>
<timeline>
  <track subject="camera">
    ::camera{zoom="1.2" duration="0.7"}
  </track>
</timeline>"#;
    let (recs, _) = walk(body);
    let barriers: Vec<(u32, f64)> = recs
        .iter()
        .filter_map(|r| match &r.cmd {
            Command::Barrier(b) => Some((b.timeline, b.at)),
            _ => None,
        })
        .collect();
    // No authored duration => barrier at max clip end (0.0 and 0.7).
    assert_eq!(barriers, vec![(1, 0.0), (2, 0.7)]);
}

#[test]
fn timeline_auto_clip_preloads_post_timeline_emotion() {
    // T10 continuation threading: a fresh `::auto` for `bianca` scheduled
    // INSIDE a `<timeline>`, with her first emotion line AFTER `</timeline>`
    // — a clock-paced clip whose CFG-reachable successor is the post-timeline
    // continuation. entry-emotion-lookahead must find `surprised` through the
    // threaded continuation, exactly like a linear `::auto`, and the preload
    // stays stamped as part of the timeline.
    let body = r#"<timeline>
  <track channel="stage">
    ::auto{character="bianca" action="fade-in-up"}
  </track>
</timeline>
:bianca{emotion="surprised"}: Oh!"#;
    let (recs, _) = walk(body);
    let preload = recs
        .iter()
        .find_map(|r| match &r.cmd {
            Command::Sprite(s) if s.preload == Some(true) => Some(s),
            _ => None,
        })
        .expect("timeline ::auto must preload the post-timeline emotion");
    assert_eq!(preload.emotion.as_deref(), Some("surprised"));
    assert_eq!(
        preload.stamp.provenance.as_ref().map(|p| p.by.as_str()),
        Some("entry-emotion-lookahead"),
        "the preload must be provenance'd to the lookahead rule reached via the threaded continuation"
    );
    // The preload record is still stamped as part of timeline 1 (clip stamp intact).
    assert_eq!(preload.stamp.timeline, Some(1));
}
