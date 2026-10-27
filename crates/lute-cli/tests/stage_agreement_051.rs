//! dsl 0.51.0 §3.4 (stage state, compiler row): `lute compile` binds the
//! stage state at every `::label` and loop head from the checker's one
//! function, so `check()` and the compile walk agree. For every conformance
//! and example document with a `::jump`, and for generated loops, each label
//! the checker's fold binds is bound to the same stage by the compile walk.
//! dsl 0.52.0 §5.4: the same holds for every hub head, recorded as label
//! `hub:<id>`, in every corpus document with a `<hub>` and generated hubs.

use std::path::{Path, PathBuf};

use lute_check::{check, record_stage_labels, take_stage_labels, CheckInput, StageLabel, WalkKind};
use lute_model::{ModelOptions, ProjectModel};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Both walks' label states for `input`: (checker, compiler).
fn label_states(input: &CheckInput) -> (Vec<StageLabel>, Vec<StageLabel>) {
    record_stage_labels();
    let result = check(input);
    assert!(result.ok, "{}: {:?}", input.uri, result.diagnostics);
    let compiled = lute_compile::compile_with_check(
        input,
        result,
        &lute_manifest::project::IdentityTemplates::default(),
    );
    assert!(compiled.is_ok(), "{}: {compiled:?}", input.uri);
    take_stage_labels()
        .into_iter()
        .partition(|l| l.kind == WalkKind::Stage)
}

fn assert_agree(what: &str, input: &CheckInput) -> usize {
    let (checker, compiler) = label_states(input);
    assert!(compiler.iter().all(|l| l.kind == WalkKind::StageCompile));
    for c in &checker {
        let bound: Vec<&StageLabel> = compiler.iter().filter(|l| l.label == c.label).collect();
        assert_eq!(
            bound.len(),
            1,
            "{what}: label `{}` in the compile walk: {compiler:#?}",
            c.label
        );
        assert!(
            c.same_state(bound[0]),
            "{what}: label `{}`\nchecker: {:#?}\ncompiler: {:#?}",
            c.label,
            c.state,
            bound[0].state
        );
    }
    checker.len()
}

#[test]
fn the_compile_walk_binds_the_checker_label_states_in_the_corpus() {
    let mut labels = 0;
    for project in [
        "docs/examples/games/monster-league",
        "docs/examples/games/hollow-ward",
        "conformance/jump-label",
        "conformance/choice-into-jump",
    ] {
        let model =
            ProjectModel::build_single_root(&root().join(project), &ModelOptions::default())
                .unwrap();
        for doc in model.documents() {
            if doc.input.text.contains("::jump") {
                labels += assert_agree(&doc.path.display().to_string(), &doc.input);
            }
        }
    }
    // The corpus has labels a jump reaches in every one of its documents.
    assert!(labels >= 5, "{labels}");
}

fn scene(body: &str) -> CheckInput {
    let text = format!(
        "---\nkind: scene\nid: agree\nstate:\n  run.f: {{ type: bool, default: false }}\n\
         enums:\n  action:\n    members: [show, hide]\n    exits: [hide]\n  \
         anchor:\n    members: [left, center, right]\n    default: center\n---\n## Shot 1.\n\
         {body}\n"
    );
    CheckInput::new(
        text,
        "agree.lute".into(),
        lute_check::Mode::Author,
        lute_check::InputSources::from_parts(
            lute_manifest::core::load_core_snapshot(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        ),
    )
}

#[test]
fn the_compile_walk_binds_the_checker_head_states_of_generated_loops() {
    let cases = [
        // A forward jump past a re-entrance.
        "@mira: Hi.\n::actor{character=\"mira\" action=\"hide\"}\n\
         ::jump{to=\"l\" when=\"run.f\"}\n::actor{character=\"mira\"}\n::label{name=\"l\"}\n\
         @narrator: Then.",
        // An exit late in a loop body.
        "::actor{character=\"mira\"}\n::label{name=\"top\"}\n@narrator: Round.\n\
         ::actor{character=\"mira\" action=\"hide\"}\n::jump{to=\"top\" max=\"2\"}",
        // Nested loops, a forward jump inside the inner one, an exit jump.
        "::actor{character=\"mira\"}\n::label{name=\"outer\"}\n::actor{character=\"theo\"}\n\
         ::label{name=\"inner\"}\n::jump{to=\"skip\" when=\"run.f\"}\n\
         ::actor{character=\"theo\" action=\"hide\"}\n::label{name=\"skip\"}\n\
         ::jump{to=\"inner\" max=\"3\"}\n::jump{to=\"out\" when=\"run.f\"}\n\
         ::actor{character=\"mira\" action=\"hide\"}\n::jump{to=\"outer\" max=\"2\"}\n\
         ::label{name=\"out\"}\n@narrator: After.",
        // A loop inside a branch arm.
        "<branch id=\"b\">\n<choice id=\"c\" text=\"Go\">\n::label{name=\"again\"}\n\
         ::actor{character=\"mira\"}\n::actor{character=\"mira\" action=\"hide\"}\n\
         ::jump{to=\"again\" max=\"2\" when=\"run.f\"}\n</choice>\n\
         <choice id=\"d\" text=\"Stay\">\n@narrator: Stay.\n</choice>\n</branch>",
    ];
    for (i, body) in cases.iter().enumerate() {
        let labels = assert_agree(&format!("case {i}"), &scene(body));
        assert!(labels >= 1, "case {i}");
    }
}

#[test]
fn the_compile_walk_binds_the_checker_hub_heads_in_the_corpus() {
    let mut heads = 0;
    for project in [
        "docs/examples/games/ember-road",
        "docs/examples/games/ledger",
        "docs/examples/games/monster-league",
        "docs/examples/games/hollow-ward",
        "conformance/hub-leave",
        "conformance/session/hub-once/project",
    ] {
        let model =
            ProjectModel::build_single_root(&root().join(project), &ModelOptions::default())
                .unwrap();
        for doc in model.documents() {
            if doc.input.text.contains("<hub") {
                let (checker, _) = label_states(&doc.input);
                heads += checker
                    .iter()
                    .filter(|l| l.label.starts_with("hub:"))
                    .count();
                assert_agree(&doc.path.display().to_string(), &doc.input);
            }
        }
    }
    // Tollbridge, the keeper's lamp room and the conformance hubs at least.
    assert!(heads >= 4, "{heads}");
}

#[test]
fn the_compile_walk_binds_the_checker_head_states_of_generated_hubs() {
    let cases = [
        // #217: an exit in one option, a line in another.
        "@mira: Hi.\n<hub id=\"desk\">\n<choice id=\"send\" text=\"Send\">\n\
         ::actor{character=\"mira\" action=\"hide\"}\n</choice>\n\
         <choice id=\"talk\" text=\"Talk\">\n@mira: Still here.\n</choice>\n\
         <choice id=\"leave\" text=\"Leave\" exit>\n@narrator: Bye.\n</choice>\n</hub>",
        // A `<return>` that shows her again; an exit option that hides her.
        "::actor{character=\"mira\"}\n<hub id=\"desk\">\n<return>\n\
         ::actor{character=\"mira\"}\n</return>\n<choice id=\"send\" text=\"Send\">\n\
         ::actor{character=\"mira\" action=\"hide\"}\n</choice>\n\
         <choice id=\"leave\" text=\"Leave\" exit>\n\
         ::actor{character=\"mira\" action=\"hide\"}\n</choice>\n</hub>\n@narrator: After.",
        // Options that only leave or end; a label after the hub.
        "::actor{character=\"mira\"}\n<hub id=\"desk\">\n<return>\n\
         ::actor{character=\"mira\" action=\"hide\"}\n</return>\n\
         <choice id=\"stop\" text=\"Stop\">\n::end{reason=\"done\"}\n</choice>\n\
         <choice id=\"go\" text=\"Go\">\n::jump{to=\"out\"}\n</choice>\n\
         <choice id=\"leave\" text=\"Leave\" exit>\n@narrator: Bye.\n</choice>\n</hub>\n\
         ::label{name=\"out\"}\n@narrator: Out.",
        // Nested hubs inside a loop region.
        "::actor{character=\"mira\"}\n::label{name=\"top\"}\n<hub id=\"outer\">\n\
         <choice id=\"enter\" text=\"In\">\n<hub id=\"inner\">\n\
         <choice id=\"send\" text=\"Send\">\n::actor{character=\"mira\" action=\"hide\"}\n\
         </choice>\n<choice id=\"back\" text=\"Back\" exit>\n@narrator: Back.\n</choice>\n\
         </hub>\n</choice>\n<choice id=\"theo\" text=\"Theo\">\n::actor{character=\"theo\"}\n\
         </choice>\n<choice id=\"leave\" text=\"Leave\" exit>\n@narrator: Bye.\n</choice>\n\
         </hub>\n::actor{character=\"mira\"}\n::jump{to=\"top\" max=\"2\"}",
    ];
    for (i, body) in cases.iter().enumerate() {
        let (checker, _) = label_states(&scene(body));
        assert!(
            checker.iter().any(|l| l.label.starts_with("hub:")),
            "case {i}"
        );
        assert_agree(&format!("case {i}"), &scene(body));
    }
}
