//! Injection goldens (arch #10): the four rules as SEPARATE sprite records
//! with provenance, placement relative to the authored record, and the D9
//! branch fork/join case.

use lute_check::ctx::Env;
use lute_check::{SpriteState, StageState};
use lute_compile::cfg::{Emitter, Rec};
use lute_compile::stage::{join_states, walk_seq, WalkCx};
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

fn sprite_desc(cmd: &Command) -> Option<String> {
    let Command::Sprite(s) = cmd else { return None };
    let by = s
        .stamp
        .provenance
        .as_ref()
        .map(|p| p.by.as_str())
        .unwrap_or("authored");
    let what = if s.pos_reset == Some(true) {
        "posReset"
    } else if s.preload == Some(true) {
        "preload"
    } else if s.exit == Some(true) {
        "exit"
    } else if s.anchor.is_some() && s.action.is_none() && s.stamp.provenance.is_some() {
        "anchor"
    } else {
        "show"
    };
    Some(format!("{}:{}:{}", s.character, what, by))
}

#[test]
fn anchor_and_preload_inject_after_authored_auto() {
    // No explicit anchor + a first emotion ahead => auto-anchor-on-show and
    // entry-emotion-lookahead, each a separate record AFTER the authored
    // sprite (§4.5 worked example).
    let (recs, _) = walk(
        "::auto{character=\"bianca\" action=\"fade-in-up\"}\n:bianca{emotion=\"surprised\"}: Oh!",
    );
    let sprites: Vec<String> = recs.iter().filter_map(|r| sprite_desc(&r.cmd)).collect();
    assert_eq!(
        sprites,
        vec![
            "bianca:show:authored".to_string(),
            "bianca:anchor:auto-anchor-on-show".to_string(),
            "bianca:preload:entry-emotion-lookahead".to_string(),
        ]
    );
    // The injected anchor record carries the default anchor.
    let Command::Sprite(anchor) = &recs[1].cmd else {
        panic!()
    };
    assert_eq!(anchor.anchor.as_deref(), Some("center"));
    assert_eq!(
        anchor.stamp.provenance.as_ref().map(|p| p.injected),
        Some(true)
    );
}

#[test]
fn pos_reset_injects_before_the_plain_line() {
    let body = "::auto{character=\"bianca\" anchor=\"center\" action=\"fade-in-up\"}\n\
:bianca{emotion=\"delighted\" action=\"pose-lean\"}: A!\n\
:bianca: B.";
    let (recs, _) = walk(body);
    let kinds: Vec<&str> = recs
        .iter()
        .map(|r| match &r.cmd {
            Command::Sprite(s) if s.pos_reset == Some(true) => "posReset",
            Command::Sprite(_) => "sprite",
            Command::Line(_) => "line",
            _ => "other",
        })
        .collect();
    // preload for the stateful first line, then: line A, posReset BEFORE line B.
    assert_eq!(kinds, vec!["sprite", "sprite", "line", "posReset", "line"]);
    let Command::Sprite(pr) = &recs[3].cmd else {
        panic!()
    };
    assert_eq!(
        pr.stamp.provenance.as_ref().map(|p| p.by.as_str()),
        Some("auto-pose-reset")
    );
}

#[test]
fn scene_change_hides_lingering_sprites_before_the_bg() {
    let body = "::auto{character=\"bianca\" anchor=\"center\" action=\"fade-in-up\"}\n\
::bg{location=\"street\" time=\"evening\"}";
    let (recs, state) = walk(body);
    let kinds: Vec<&str> = recs
        .iter()
        .map(|r| match &r.cmd {
            Command::Sprite(s) if s.exit == Some(true) => "hide",
            Command::Sprite(_) => "sprite",
            Command::Background(_) => "background",
            _ => "other",
        })
        .collect();
    assert_eq!(kinds, vec!["sprite", "hide", "background"]);
    let Command::Sprite(h) = &recs[1].cmd else {
        panic!()
    };
    assert_eq!(
        h.stamp.provenance.as_ref().map(|p| p.by.as_str()),
        Some("stage-bookkeeping")
    );
    assert!(
        state.on_stage.is_empty(),
        "stage cleared after scene change"
    );
}

#[test]
fn branch_arms_fork_from_entry_state_and_join_conservatively() {
    // D9 fork/join golden (spec §8): BOTH arms show bianca fresh (each gets
    // its own anchor injection — nothing leaks from arm 1 into arm 2), and
    // the post-join ::auto is a fresh show again (differing arm emotions =>
    // the join drops bianca).
    let body = r#"<branch id="fork">
  <choice id="a" label="A">
    ::auto{character="bianca" action="fade-in-up"}
    :bianca{emotion="surprised"}: Oh!
  </choice>
  <choice id="b" label="B">
    ::auto{character="bianca" action="fade-in-up"}
    :bianca{emotion="delighted"}: Ha!
  </choice>
</branch>
::auto{character="bianca" action="fade-in-up"}"#;
    let (recs, _) = walk(body);
    let anchors: Vec<usize> = recs
        .iter()
        .enumerate()
        .filter(
            |(_, r)| matches!(sprite_desc(&r.cmd).as_deref(), Some(d) if d.contains(":anchor:")),
        )
        .map(|(i, _)| i)
        .collect();
    // Three anchor injections: one per arm + one after the join.
    assert_eq!(anchors.len(), 3, "{recs:#?}");
}

#[test]
fn arm_end_entrance_preloads_post_convergence_emotion() {
    // D9 continuation threading: each arm ENDS with a fresh `::auto` entrance
    // for `bianca`, and her first emotion line sits AFTER `</branch>` — a
    // post-convergence, CFG-reachable successor, not inside any arm.
    // entry-emotion-lookahead must find `surprised` through the threaded
    // continuation, so BOTH arm entrances preload it, while sibling arms are
    // never consulted.
    let body = r#"<branch id="fork">
  <choice id="a" label="A">
    ::auto{character="bianca" action="fade-in-up"}
  </choice>
  <choice id="b" label="B">
    ::auto{character="bianca" action="fade-in-up"}
  </choice>
</branch>
:bianca{emotion="surprised"}: Oh!"#;
    let (recs, _) = walk(body);
    let preloads: Vec<String> = recs
        .iter()
        .filter_map(|r| match &r.cmd {
            Command::Sprite(s) if s.preload == Some(true) => {
                assert_eq!(
                    s.stamp.provenance.as_ref().map(|p| p.by.as_str()),
                    Some("entry-emotion-lookahead"),
                    "post-convergence preload must be provenance'd to the lookahead rule"
                );
                Some(s.emotion.clone().unwrap_or_default())
            }
            _ => None,
        })
        .collect();
    // One preload per arm entrance, each pre-loading the post-convergence
    // emotion found only via the threaded continuation.
    assert_eq!(
        preloads,
        vec!["surprised".to_string(), "surprised".to_string()],
        "{recs:#?}"
    );
}

#[test]
fn join_states_unit_semantics() {
    let sprite = |emotion: &str| SpriteState {
        anchor: Some("center".to_string()),
        pose: None,
        emotion: Some(emotion.to_string()),
    };
    let mut a = StageState::default();
    a.on_stage.insert("bianca".into(), sprite("surprised"));
    a.on_stage.insert("takeru".into(), sprite("neutral"));
    a.dirty.insert("takeru".into());
    a.bg = Some("street".into());
    let mut b = StageState::default();
    b.on_stage.insert("bianca".into(), sprite("delighted"));
    b.on_stage.insert("takeru".into(), sprite("neutral"));
    b.dirty.insert("takeru".into());
    b.bg = Some("cafe".into());

    let joined = join_states(&StageState::default(), vec![a, b]);
    // Differing emotion => bianca dropped (Unknown, §7.3).
    assert!(!joined.on_stage.contains_key("bianca"));
    // Identical in every arm => carried, dirty intersection kept.
    assert!(joined.on_stage.contains_key("takeru"));
    assert!(joined.dirty.contains("takeru"));
    // Differing bg => Unknown (None).
    assert!(joined.bg.is_none());
    // Empty exits degrade to the entry state.
    let entry = StageState::default();
    assert!(join_states(&entry, Vec::new()).on_stage.is_empty());
}

#[test]
fn dirty_survives_join_when_only_one_arm_dirties_the_speaker() {
    // Regression (dirty-join under-injection): a `variant`-only stateful line
    // marks the speaker `dirty` WITHOUT changing `SpriteState`. When one arm
    // dirties bianca and the other doesn't, but BOTH keep the same SpriteState,
    // the join must carry bianca AND union her dirty flag — so the next plain
    // line still fires `auto-pose-reset`. Under the old intersection merge the
    // flag was dropped and the reset silently lost.
    let body = r#"::auto{character="bianca" anchor="left" action="fade-in-up"}
<branch id="fork">
  <choice id="a" label="A">
    :bianca{variant="closeup"}: Hm.
  </choice>
  <choice id="b" label="B">
    :bianca: Yo.
  </choice>
</branch>
:bianca: Well."#;
    let (recs, _) = walk(body);
    let pos_resets: Vec<&str> = recs
        .iter()
        .filter_map(|r| match &r.cmd {
            Command::Sprite(s) if s.pos_reset == Some(true) => Some(
                s.stamp
                    .provenance
                    .as_ref()
                    .map(|p| p.by.as_str())
                    .unwrap_or(""),
            ),
            _ => None,
        })
        .collect();
    // Exactly one posReset — the post-convergence plain line — provenance'd to
    // the auto-pose-reset rule. (Absent entirely before the union fix.)
    assert_eq!(pos_resets, vec!["auto-pose-reset"], "{recs:#?}");
}

#[test]
fn join_unions_dirty_but_only_over_carried_characters() {
    let sprite = |pose: Option<&str>| SpriteState {
        anchor: Some("center".to_string()),
        pose: pose.map(str::to_string),
        emotion: Some("neutral".to_string()),
    };
    // Same SpriteState in every arm => carried; dirty in ANY arm => dirty.
    let mut a = StageState::default();
    a.on_stage.insert("bianca".into(), sprite(None));
    a.dirty.insert("bianca".into());
    let mut b = StageState::default();
    b.on_stage.insert("bianca".into(), sprite(None));
    // A char DROPPED at the join (differing SpriteState) must NOT be
    // resurrected into `dirty`, even though an arm marked it dirty.
    a.on_stage
        .insert("takeru".into(), sprite(Some("pose-lean")));
    a.dirty.insert("takeru".into());
    b.on_stage.insert("takeru".into(), sprite(None));

    let joined = join_states(&StageState::default(), vec![a, b]);
    assert!(joined.on_stage.contains_key("bianca"));
    assert!(joined.dirty.contains("bianca"));
    assert!(!joined.on_stage.contains_key("takeru"));
    assert!(!joined.dirty.contains("takeru"));
}
