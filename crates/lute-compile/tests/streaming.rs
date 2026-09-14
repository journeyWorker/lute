use lute_check::{CheckInput, Mode};
use lute_compile::streaming::{
    ContinuationCompiler, E_STREAM_BODY, E_STREAM_CLOSED, E_STREAM_PREFIX_CHANGED,
    E_STREAM_TEMPLATE,
};
use lute_compile::{compile_with_check, Command};
use lute_manifest::project::IdentityTemplates;

fn input(text: impl Into<String>) -> CheckInput {
    CheckInput {
        text: text.into(),
        uri: "stream-test".into(),
        snapshot: lute_test_vocab::vocab_snapshot(),
        providers: Default::default(),
        mode: Mode::Ci,
        imports: Default::default(),
        components: Default::default(),
        defaults: Default::default(),
    }
}

fn scene(body: &str, state: &str) -> String {
    format!(
        "---\nkind: scene\ncharacter: marina\nseason: 1\nepisode: 1\n{state}---\n\n## Opening\n{body}"
    )
}

fn artifact_json(artifact: &lute_compile::Artifact) -> serde_json::Value {
    serde_json::to_value(artifact).expect("artifact serializes")
}

fn batch(text: &str) -> lute_compile::Artifact {
    let input = input(text);
    let checked = lute_check::check(&input);
    compile_with_check(&input, checked, &IdentityTemplates::default())
        .unwrap_or_else(|diagnostics| panic!("batch compile failed: {diagnostics:#?}"))
}

#[test]
fn cumulative_reanalysis_carries_state_between_units() {
    let prefix = scene(
        "",
        "state:\n  run.value: { type: number }\n  run.result: { type: number }\n",
    );
    let mut compiler = ContinuationCompiler::new(input(&prefix), IdentityTemplates::default())
        .expect("valid scene template");

    let first = compiler.push("::set{run.value = 7}\n");
    assert_eq!(first.updates.len(), 1);
    assert!(first.diagnostics.iter().all(|d| d.severity != lute_core_span::Severity::Error));

    let second = compiler.push("::set{run.result = run.value}\n");
    assert_eq!(second.updates.len(), 1, "earlier write must satisfy later read");
    assert!(second.diagnostics.iter().all(|d| d.severity != lute_core_span::Severity::Error));
    assert_eq!(second.updates[0].append_from, first.updates[0].artifact.commands.len());

    let finished = compiler.finish();
    assert!(finished.finished && finished.diagnostics.is_empty());
    assert_eq!(
        artifact_json(compiler.artifact()),
        artifact_json(&batch(&format!(
            "{prefix}::set{{run.value = 7}}\n::set{{run.result = run.value}}\n"
        )))
    );
}

#[test]
fn branch_state_and_auto_injection_use_ordinary_compiler() {
    let prefix = scene("", "");
    let mut compiler = ContinuationCompiler::new(input(&prefix), IdentityTemplates::default())
        .expect("valid scene template");

    let body = "::auto{character=\"marina\" action=\"fade-in-up\"}\n\
                <branch id=\"route\">\n\
                <choice id=\"left\" label=\"Left\">\n\
                @marina{code=\"0010\"}: Left.\n\
                </choice>\n\
                <choice id=\"right\" label=\"Right\">\n\
                @marina{code=\"0020\"}: Right.\n\
                </choice>\n\
                </branch>\n";
    let result = compiler.push(body);
    assert_eq!(result.updates.len(), 2);
    let final_artifact = &result.updates[1].artifact;
    assert!(final_artifact.state.iter().any(|entry| {
        entry.path == "scene.choices.route"
            && entry
                .domain
                .as_ref()
                .is_some_and(|domain| domain.iter().any(|v| v == "unset"))
    }));
    assert!(final_artifact
        .commands
        .iter()
        .any(|command| matches!(command, Command::Sprite(_))));
    assert!(final_artifact
        .commands
        .iter()
        .any(|command| matches!(command, Command::Choice(_))));
    assert_eq!(
        artifact_json(final_artifact),
        artifact_json(&batch(&format!("{prefix}{body}")))
    );
}

#[test]
fn chunk_partition_does_not_change_updates_or_final_artifact() {
    let prefix = scene("", "");
    let body = "@marina{code=\"0010\"}: 안녕.\n@narrator{code=\"0010\"}: Done.\n";

    let run = |chunks: Vec<&str>| {
        let mut compiler = ContinuationCompiler::new(input(&prefix), IdentityTemplates::default())
            .expect("valid scene template");
        let mut updates = Vec::new();
        for chunk in chunks {
            let result = compiler.push(chunk);
            assert!(!result.finished, "valid input remains open before EOF");
            updates.extend(result.updates.into_iter().map(|u| artifact_json(&u.artifact)));
        }
        let finish = compiler.finish();
        assert!(finish.finished && finish.diagnostics.is_empty());
        (updates, artifact_json(compiler.artifact()))
    };

    let whole = run(vec![body]);
    let chars: Vec<String> = body.chars().map(|ch| ch.to_string()).collect();
    let char_refs = chars.iter().map(String::as_str).collect();
    let split = run(char_refs);
    assert_eq!(whole, split);
}

#[test]
fn body_scaffolding_and_invalid_state_are_terminal_without_exposing_ir() {
    let prefix = scene("", "");
    let mut heading = ContinuationCompiler::new(input(&prefix), IdentityTemplates::default())
        .expect("valid scene template");
    let rejected = heading.push("## Another shot\n@narrator: hidden\n");
    let body_diagnostic = rejected
        .diagnostics
        .iter()
        .find(|d| d.code == E_STREAM_BODY)
        .expect("stream body diagnostic");
    assert_eq!(
        body_diagnostic.span.byte_start,
        prefix.len(),
        "service spans are cumulative-source offsets"
    );
    assert!(rejected.finished && rejected.updates.is_empty());
    assert!(rejected.diagnostics.iter().any(|d| d.code == E_STREAM_BODY));
    assert_eq!(heading.artifact().commands.len(), 0);

    let mut state = ContinuationCompiler::new(input(&prefix), IdentityTemplates::default())
        .expect("valid scene template");
    let rejected = state.push("@marina{code=\"0010\"}: accepted\n::set{run.missing = 1}\n");
    assert!(rejected.finished);
    assert_eq!(rejected.updates.len(), 1, "accepted earlier unit is retained");
    assert!(rejected.diagnostics.iter().any(|d| d.code == "E-UNDECLARED"));
    assert_eq!(state.artifact().commands.len(), 1);
    let closed = state.push("@narrator: ignored\n");
    assert!(closed.finished && closed.updates.is_empty());
    assert!(closed.diagnostics.iter().any(|d| d.code == E_STREAM_CLOSED));
}

#[test]
fn incomplete_eof_is_terminal_and_keeps_last_artifact() {
    let prefix = scene("", "");
    let mut compiler = ContinuationCompiler::new(input(&prefix), IdentityTemplates::default())
        .expect("valid scene template");
    let pending = compiler.push("<branch id=\"open\">\n");
    assert!(pending.updates.is_empty() && pending.need_more.is_some());

    let finished = compiler.finish();
    assert!(finished.finished && finished.updates.is_empty());

    let mut leaf = ContinuationCompiler::new(input(&prefix), IdentityTemplates::default())
        .expect("valid scene template");
    assert!(leaf.push("@narrator{code=\"0010\"}: eof leaf").updates.is_empty());
    let completed = leaf.finish();
    assert!(completed.finished && completed.diagnostics.is_empty());
    assert_eq!(completed.updates.len(), 1, "EOF delimits a complete leaf");
    assert_eq!(
        artifact_json(leaf.artifact()),
        artifact_json(&batch(&format!(
            "{prefix}@narrator{{code=\"0010\"}}: eof leaf"
        )))
    );
    assert!(finished.diagnostics.iter().any(|d| d.code == "E-UNCLOSED-TAG"));
    assert!(compiler.artifact().commands.is_empty());
}

#[test]
fn decimal_padding_growth_does_not_rewrite_the_semantic_prefix() {
    let mut initial = String::new();
    for number in 1..=99 {
        initial.push_str(&format!("@narrator{{code=\"{number:04}\"}}: line {number}\n"));
    }
    let prefix = scene(&initial, "");
    let mut compiler = ContinuationCompiler::new(input(&prefix), IdentityTemplates::default())
        .expect("valid scene template");
    let first = serde_json::to_value(&compiler.artifact().commands[0]).unwrap();
    assert_eq!(first["addr"], "001-0100");

    let update = compiler.push("@narrator{code=\"0100\"}: line 100\n");
    assert_eq!(update.updates.len(), 1, "address-width-only changes are allowed");
    assert_eq!(update.updates[0].append_from, 99);
    let first = serde_json::to_value(&update.updates[0].artifact.commands[0]).unwrap();
    assert_eq!(first["addr"], "001-00100");
}

#[test]
fn retroactive_identity_change_is_rejected() {
    let prefix = scene("@marina: implicit identity\n", "");
    let mut compiler = ContinuationCompiler::new(input(&prefix), IdentityTemplates::default())
        .expect("valid scene template");
    let before = artifact_json(compiler.artifact());

    let rejected = compiler.push("@marina{code=\"9000\"}: later authored code\n");
    assert!(rejected.finished && rejected.updates.is_empty());
    assert!(rejected
        .diagnostics
        .iter()
        .any(|d| d.code == E_STREAM_PREFIX_CHANGED));
    assert_eq!(artifact_json(compiler.artifact()), before);
}

#[test]
fn finish_and_failure_both_close_the_service() {
    let prefix = scene("", "");
    let mut compiler = ContinuationCompiler::new(input(&prefix), IdentityTemplates::default())
        .expect("valid scene template");
    assert!(compiler.finish().finished);
    let closed = compiler.push("@narrator: no\n");
    assert!(closed.finished && closed.updates.is_empty());
    assert!(closed.diagnostics.iter().any(|d| d.code == E_STREAM_CLOSED));

    let no_shot = "---\nkind: scene\ncharacter: marina\nseason: 1\nepisode: 1\n---\n";
    let diagnostics =
        match ContinuationCompiler::new(input(no_shot), IdentityTemplates::default()) {
            Ok(_) => panic!("scene without a shot is not a continuation template"),
            Err(diagnostics) => diagnostics,
        };
    assert!(diagnostics.iter().any(|d| d.code == E_STREAM_TEMPLATE));
}

