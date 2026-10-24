//! 0.49.0 §5.1 (#158): `<choice into=… value=…>` lowers its record write as
//! the arm's first command, so an arm ending in `::jump` still records the
//! pick.

use lute_check::{CheckInput, Mode};
use lute_compile::compile;

fn input(text: &str) -> CheckInput {
    CheckInput::new(
        text.to_string(),
        "into_049".into(),
        Mode::Ci,
        lute_check::InputSources::from_parts(
            lute_test_vocab::vocab_snapshot(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        ),
    )
}

/// `(position, kind, path or target)` of every compiled command.
fn commands(text: &str) -> Vec<(String, String, String)> {
    let artifact =
        compile(&input(text)).unwrap_or_else(|e| panic!("compiles clean: {e:?}\n{text}"));
    let json = serde_json::to_value(&artifact).expect("artifact serializes");
    json["commands"]
        .as_array()
        .expect("commands array")
        .iter()
        .map(|c| {
            let field = |k: &str| c[k].as_str().unwrap_or_default().to_string();
            let what = if c["kind"] == "jump" {
                field("target")
            } else {
                field("path")
            };
            (field("position"), field("kind"), what)
        })
        .collect()
}

const SCENE: &str = r#"---
kind: scene
character: x
season: 1
episode: 1
state:
  run.pick: { type: { enum: [none, left, right] }, default: none }
---

## Hall

<branch id="route" prompt="Which way?">
  <choice id="goLeft" text="Left" into="run.pick" value="left">
    @narrator: Left it is.
    ::jump{to="after"}
  </choice>
  <choice id="goRight" text="Right" into="run.pick" value="right">
    @narrator: Right it is.
  </choice>
</branch>
::label{name="after"}
@narrator: Done.
"#;

#[test]
fn into_record_precedes_the_arm_body_and_its_jump() {
    let cmds = commands(SCENE);
    let kinds: Vec<(&str, &str)> = cmds
        .iter()
        .map(|(_, k, w)| (k.as_str(), w.as_str()))
        .collect();
    assert_eq!(
        kinds,
        [
            ("choice", ""),
            ("set", "run.pick"),
            ("line", ""),
            ("jump", "001-0900"),
            ("jump", "001-0900"),
            ("set", "run.pick"),
            ("line", ""),
            ("jump", "001-0900"),
            ("line", ""),
        ],
        "{cmds:#?}"
    );
}
