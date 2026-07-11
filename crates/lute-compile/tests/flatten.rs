//! Flatten-shape goldens (§7): header/arm/jump/converge order, symbolic
//! label binding, the §7.2 nesting rule, empty arms, end-of-shot converges,
//! and component-sentinel consumption.

use lute_check::ctx::Env;
use lute_check::StageState;
use lute_compile::cfg::{Emitter, Label, Rec};
use lute_compile::stage::{walk_seq, WalkCx};
use lute_compile::Command;
use lute_core_span::Severity;

fn flatten(body: &str) -> (Vec<Rec>, Vec<Label>) {
    let src =
        format!("---\nkind: scene\ncharacter: bianca\nseason: 1\nepisode: 2\n---\n\n## Shot 1.\n\n{body}\n");
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
    let _ = walk_seq(
        &mut em,
        &doc.shots[0].body,
        StageState::default(),
        &mut cx,
        &[],
        &mut Vec::new(),
    );
    em.finish()
}

fn kind(cmd: &Command) -> &'static str {
    match cmd {
        Command::Line(_) => "line",
        Command::Background(_) => "background",
        Command::Music(_) => "music",
        Command::Sfx(_) => "sfx",
        Command::Vfx(_) => "vfx",
        Command::Sprite(_) => "sprite",
        Command::Camera(_) => "camera",
        Command::Cut(_) => "cut",
        Command::Video(_) => "video",
        Command::Set(_) => "set",
        Command::Choice(_) => "choice",
        Command::Match(_) => "match",
        Command::Hub(_) => "hub",
        Command::Jump(_) => "jump",
        Command::Barrier(_) => "barrier",
        Command::Other(_) => "plugin",
        Command::Quest(_) => "quest",
        Command::On(_) => "on",
    }
}

const BRANCH: &str = r#"<branch id="number">
  <choice id="blunt" label="Just ask, flatly">
    @fixer{code="0050"}: Bianca. Your number.
  </choice>
  <choice id="soft" label="Ask gently">
    ::set{scene.affect.bianca += 1}
  </choice>
</branch>
@narrator: She answers."#;

#[test]
fn branch_flattens_to_header_arms_jumps_converge() {
    let (recs, trailing) = flatten(BRANCH);
    let kinds: Vec<_> = recs.iter().map(|r| kind(&r.cmd)).collect();
    assert_eq!(kinds, vec!["choice", "line", "jump", "set", "jump", "line"]);
    assert!(trailing.is_empty());

    let Command::Choice(c) = &recs[0].cmd else {
        panic!()
    };
    assert_eq!(c.branch_id, "number");
    assert_eq!(c.record_key, "scene.choices.number");
    // Option targets point at each arm's first record's label.
    assert_eq!(c.options[0].target, recs[1].labels[0].sym());
    assert_eq!(c.options[1].target, recs[3].labels[0].sym());
    // Converge label binds on the narrator line after the block.
    assert_eq!(c.converge, recs[5].labels[0].sym());
    // Both arm-trailing jumps return to the converge.
    for i in [2usize, 4] {
        let Command::Jump(j) = &recs[i].cmd else {
            panic!()
        };
        assert_eq!(j.target, c.converge);
    }
}

#[test]
fn match_flattens_with_otherwise_and_omits_it_when_absent() {
    let m = r#"<match on="scene.flags.saw_beam">
  <when test="$ == true">
    @fixer{mono}: saw
  </when>
  <otherwise>
    @fixer{mono}: not
  </otherwise>
</match>
@narrator: on."#;
    let (recs, _) = flatten(m);
    let kinds: Vec<_> = recs.iter().map(|r| kind(&r.cmd)).collect();
    assert_eq!(kinds, vec!["match", "line", "jump", "line", "jump", "line"]);
    let Command::Match(mc) = &recs[0].cmd else {
        panic!()
    };
    assert_eq!(mc.subject, "scene.flags.saw_beam");
    assert_eq!(mc.arms.len(), 1);
    assert_eq!(mc.arms[0].target, recs[1].labels[0].sym());
    assert_eq!(
        mc.otherwise.as_deref(),
        Some(recs[3].labels[0].sym().as_str())
    );
    assert_eq!(mc.converge, recs[5].labels[0].sym());

    // No <otherwise> arm (gate-proven covered) => field omitted (§11.2).
    let covered = r#"<match on="scene.flags.saw_beam">
  <when test="$ == true">
    @fixer{mono}: t
  </when>
  <when test="$ == false">
    @fixer{mono}: f
  </when>
</match>
@narrator: on."#;
    let (recs, _) = flatten(covered);
    let Command::Match(mc) = &recs[0].cmd else {
        panic!()
    };
    assert!(mc.otherwise.is_none());
}

#[test]
fn nested_block_lays_inner_convergence_before_outer_jump() {
    let nested = r#"<branch id="outer">
  <choice id="a" label="A">
    <match on="scene.flags.saw_beam">
      <when test="$ == true">
        @fixer{mono}: saw
      </when>
      <otherwise>
        @fixer{mono}: not
      </otherwise>
    </match>
  </choice>
  <choice id="b" label="B">
    @fixer{code="0010"}: b
  </choice>
</branch>
@narrator: end."#;
    let (recs, _) = flatten(nested);
    let kinds: Vec<_> = recs.iter().map(|r| kind(&r.cmd)).collect();
    assert_eq!(
        kinds,
        vec!["choice", "match", "line", "jump", "line", "jump", "jump", "line", "jump", "line"]
    );
    // §7.2: the INNER convergence label binds on the OUTER arm-a trailing jump
    // (recs[6]), so control reaches the outer converge through it.
    let Command::Match(mc) = &recs[1].cmd else {
        panic!()
    };
    assert_eq!(mc.converge, recs[6].labels[0].sym());
    let Command::Choice(c) = &recs[0].cmd else {
        panic!()
    };
    let Command::Jump(outer_a) = &recs[6].cmd else {
        panic!()
    };
    assert_eq!(outer_a.target, c.converge);
    // Inner arm jumps return to the inner converge, not the outer one.
    for i in [3usize, 5] {
        let Command::Jump(j) = &recs[i].cmd else {
            panic!()
        };
        assert_eq!(j.target, mc.converge);
    }
    assert_eq!(c.converge, recs[9].labels[0].sym());
}

#[test]
fn empty_arm_is_a_bare_labeled_jump_and_last_block_converges_past_end() {
    let b = r#"<branch id="tail">
  <choice id="go" label="Go">
  </choice>
</branch>"#;
    let (recs, trailing) = flatten(b);
    let kinds: Vec<_> = recs.iter().map(|r| kind(&r.cmd)).collect();
    assert_eq!(kinds, vec!["choice", "jump"]);
    let Command::Choice(c) = &recs[0].cmd else {
        panic!()
    };
    // Empty arm: the arm label sits ON the bare jump (§7.2).
    assert_eq!(c.options[0].target, recs[1].labels[0].sym());
    // Branch is the LAST node: converge label is left trailing for Task 11's
    // one-past-end addr (plan spec-gap note 2).
    assert_eq!(trailing.len(), 1);
    assert_eq!(c.converge, trailing[0].sym());
}

#[test]
fn component_sentinels_stamp_source_and_emit_nothing() {
    let src = r#"::__component-begin{component="greet"}
::auto{character="bianca" anchor="center" action="fade-in-up"}
::__component-end
@narrator: after."#;
    let (recs, _) = flatten(src);
    let kinds: Vec<_> = recs.iter().map(|r| kind(&r.cmd)).collect();
    assert_eq!(kinds, vec!["sprite", "line"]);
    let Command::Sprite(s) = &recs[0].cmd else {
        panic!()
    };
    assert_eq!(
        s.stamp.source.as_ref().map(|s| s.component.as_str()),
        Some("greet")
    );
    let Command::Line(l) = &recs[1].cmd else {
        panic!()
    };
    assert!(l.stamp.source.is_none());
}
