//! Addressing + identity (§4.2): dense +100 addrs, label resolution (incl.
//! one-past-end converges), lineId/voiceKey derivation, per-speaker code
//! back-fill mirroring `lute tag`.

use lute_check::ctx::Env;
use lute_check::StageState;
use lute_compile::address::{assign_addresses, ShotRecords};
use lute_compile::cfg::Emitter;
use lute_compile::stage::{walk_seq, WalkCx};
use lute_compile::Command;
use lute_core_span::Severity;

/// Walk every shot of `src` and address the result — the same wiring
/// `compile()` (Task 12) uses.
fn addressed(src: &str) -> (Vec<Command>, Vec<lute_core_span::Diagnostic>) {
    let (doc, diags) = lute_syntax::parse(src);
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
    let mut state = StageState::default();
    let mut shots = Vec::new();
    for (i, shot) in doc.shots.iter().enumerate() {
        let mut em = Emitter::default();
        state = walk_seq(&mut em, &shot.body, state, &mut cx, &[], &mut Vec::new());
        // dsl 0.6.0 §3.2: positional 1-based shot number.
        let shot_no = i as i64 + 1;
        let (recs, trailing) = em.finish();
        shots.push(ShotRecords {
            shot: shot_no,
            prefix: "bianca.s01ep02".to_string(),
            recs,
            trailing,
        });
    }
    assign_addresses(shots)
}

const SRC: &str = r#"---
kind: scene
character: bianca
season: 1
episode: 2
---

## Shot 1.

@fixer{code="0010"}: ...
@narrator: He waited.

## Shot 4.

@fixer{code="0050"}: Bianca. Your number.
@fixer: And again.
<branch id="number">
  <choice id="blunt" label="Just ask, flatly">
    @bianca{code="0010" emotion="surprised"}: Oh!
  </choice>
  <choice id="soft" label="Ask gently">
    ::set{scene.affect.bianca += 1}
  </choice>
</branch>
"#;

#[test]
fn addrs_are_dense_per_shot_and_labels_resolve() {
    let (cmds, diags) = addressed(SRC);
    assert!(diags.is_empty(), "{diags:#?}");
    let addrs: Vec<String> = cmds
        .iter()
        .map(|c| {
            let mut c = c.clone();
            c.addr_mut().clone()
        })
        .collect();
    // Shot 1: two lines. The second shot's `## Shot 4.` heading is now opaque
    // (dsl 0.6.0 §3.2): the "4" is ignored, its number is its position (2) —
    // line, line, choice header, arm records + jumps.
    assert_eq!(addrs[0], "001-0100");
    assert_eq!(addrs[1], "001-0200");
    assert_eq!(addrs[2], "002-0100");
    // +100 gaps, strictly increasing within each shot.
    for w in addrs.windows(2) {
        assert!(w[0] < w[1], "addr order: {w:?}");
    }
    // Every control-flow target resolved to a real addr — no symbolic '@'.
    for c in &cmds {
        let mut c = c.clone();
        c.for_each_target(&mut |t: &mut String| {
            assert!(!t.starts_with('@'), "unresolved label {t}");
            assert_eq!(t.len(), 8, "addr shape: {t}");
        });
    }
    // The branch is the LAST node of the second shot (positional shot 2): its
    // one-past-end addr (plan spec-gap note 2).
    let Some(Command::Choice(choice)) = cmds.iter().find(|c| matches!(c, Command::Choice(_)))
    else {
        panic!()
    };
    let last_addr = {
        let mut last = cmds.last().unwrap().clone();
        last.addr_mut().clone()
    };
    let expected_past_end = format!("002-{:04}", last_addr[4..].parse::<i64>().unwrap() + 100);
    assert_eq!(choice.converge, expected_past_end);
    // Option targets point at the arms' first records.
    let Command::Line(blunt_line) = &cmds[5] else {
        panic!("{:#?}", cmds)
    };
    assert_eq!(choice.options[0].target, "002-0400");
    assert_eq!(blunt_line.text, "Oh!");
}

#[test]
fn line_ids_and_voice_keys_follow_the_speaker_code_model() {
    let (cmds, _) = addressed(SRC);
    let lines: Vec<(&str, &str, Option<&str>)> = cmds
        .iter()
        .filter_map(|c| match c {
            Command::Line(l) => Some((
                l.speaker.as_str(),
                l.line_id.as_str(),
                l.voice_key.as_deref(),
            )),
            _ => None,
        })
        .collect();
    assert_eq!(
        lines,
        vec![
            // Authored code kept.
            ("fixer", "bianca.s01ep02.fixer_0010", Some("fixer-0010")),
            // Narrator: lineId for i18n, NO voiceKey (unvoiced role).
            ("narrator", "bianca.s01ep02.narrator_0010", None),
            ("fixer", "bianca.s01ep02.fixer_0050", Some("fixer-0050")),
            // Back-filled: fixer's max authored code is 0050 => next is 0060.
            ("fixer", "bianca.s01ep02.fixer_0060", Some("fixer-0060")),
            ("bianca", "bianca.s01ep02.bianca_0010", Some("bianca-0010")),
        ]
    );
    // Option labels get structural lineIds: {character}.s{s}ep{e}.{branchId}.{choiceId}.
    let Some(Command::Choice(choice)) = cmds.iter().find(|c| matches!(c, Command::Choice(_)))
    else {
        panic!()
    };
    assert_eq!(choice.options[0].line_id, "bianca.s01ep02.number.blunt");
    assert_eq!(choice.options[1].line_id, "bianca.s01ep02.number.soft");
}
