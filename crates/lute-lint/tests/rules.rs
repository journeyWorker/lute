//! End-to-end rule tests (spec §10) — each core rule gets a fixture
//! document that fires it, and a control document that does not.
//!
//! These call the public [`lute_lint::lint`] entry point through
//! [`lute_syntax::parse`] the same way the CLI will, so any refactor of
//! the intermediate metric/eval layers is caught by contract-shape tests.

use std::path::PathBuf;

use lute_core_span::{Span, Severity};
use lute_lint::{lint, LintConfig, LintDocInput, LintScope};
use lute_manifest::provider::{IdStatus, ProviderSet, ProviderSnapshot};
use lute_syntax::parse;

fn empty_span() -> Span {
    Span { byte_start: 0, byte_end: 0, line: 1, column: 1, utf16_range: (0, 0) }
}

fn input(path: &str, text: &str) -> LintDocInput {
    let (doc, _diags) = parse(text);
    LintDocInput {
        path: PathBuf::from(path),
        doc,
        text: text.to_string(),
    }
}

fn codes(diags: &[(PathBuf, lute_core_span::Diagnostic)]) -> Vec<String> {
    diags.iter().map(|(_, d)| d.code.clone()).collect()
}

fn only_code(diags: &[(PathBuf, lute_core_span::Diagnostic)], code: &str) -> Vec<(PathBuf, lute_core_span::Diagnostic)> {
    diags.iter().filter(|(_, d)| d.code == code).cloned().collect()
}

// ---------------------------------------------------------------------------
// dialogue-length (line)
// ---------------------------------------------------------------------------

#[test]
fn dialogue_length_fires_when_over_cap() {
    let doc = input(
        "scene.lute",
        "---\nkind: scene\n---\n## Shot 1.\n\
         @alice: hello\n\
         @bob: one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen seventeen eighteen nineteen twenty twenty-one twenty-two twenty-three twenty-four twenty-five twenty-six twenty-seven twenty-eight twenty-nine thirty thirty-one thirty-two thirty-three thirty-four thirty-five thirty-six thirty-seven thirty-eight thirty-nine forty forty-one\n",
    );
    let out = lint(&[doc], &LintConfig::default(), &[], &ProviderSet::default(), None, empty_span(), LintScope::Full);
    let dl = only_code(&out.diagnostics, "L-DIALOGUE-LENGTH");
    assert_eq!(dl.len(), 1, "codes: {:?}", codes(&out.diagnostics));
    assert!(dl[0].1.message.contains("41 words"), "{}", dl[0].1.message);
}

// ---------------------------------------------------------------------------
// dialogue-ratio (scene)
// ---------------------------------------------------------------------------

#[test]
fn dialogue_ratio_fires_when_below_floor() {
    // 11 body nodes: 1 dialogue, 10 directive lines → ratio 1/11 ≈ 0.09.
    let mut body = String::from("---\nkind: scene\n---\n## Shot 1.\n\
        @alice: hello\n");
    for i in 0..10 {
        body.push_str(&format!("::bg{{location=\"a{i}\"}}\n"));
    }
    let doc = input("scene.lute", &body);
    let out = lint(&[doc], &LintConfig::default(), &[], &ProviderSet::default(), None, empty_span(), LintScope::Full);
    let rows = only_code(&out.diagnostics, "L-DIALOGUE-RATIO");
    assert_eq!(rows.len(), 1, "codes: {:?}", codes(&out.diagnostics));
}

#[test]
fn dialogue_ratio_silent_below_min_nodes() {
    // 5 body nodes total → inert.
    let doc = input(
        "scene.lute",
        "---\nkind: scene\n---\n## Shot 1.\n\
         @alice: hi\n\
         ::bg{location=\"a\"}\n\
         ::bg{location=\"b\"}\n\
         ::bg{location=\"c\"}\n\
         ::bg{location=\"d\"}\n",
    );
    let out = lint(&[doc], &LintConfig::default(), &[], &ProviderSet::default(), None, empty_span(), LintScope::Full);
    let rows = only_code(&out.diagnostics, "L-DIALOGUE-RATIO");
    assert!(rows.is_empty(), "unexpected: {:?}", codes(&out.diagnostics));
}

// ---------------------------------------------------------------------------
// shot-starts-with-background (shot)
// ---------------------------------------------------------------------------

#[test]
fn shot_starts_with_background_ok_when_first_is_bg() {
    let doc = input(
        "scene.lute",
        "---\nkind: scene\n---\n## Shot 1.\n\
         ::bg{location=\"a\"}\n@alice: hi\n",
    );
    let out = lint(&[doc], &LintConfig::default(), &[], &ProviderSet::default(), None, empty_span(), LintScope::Full);
    let rows = only_code(&out.diagnostics, "L-SHOT-STARTS-WITH-BACKGROUND");
    assert!(rows.is_empty(), "unexpected: {:?}", codes(&out.diagnostics));
}

#[test]
fn shot_starts_with_background_fires_when_first_is_not_bg() {
    let doc = input(
        "scene.lute",
        "---\nkind: scene\n---\n## Shot 1.\n\
         ::music{action=\"start\"}\n@alice: hi\n",
    );
    let out = lint(&[doc], &LintConfig::default(), &[], &ProviderSet::default(), None, empty_span(), LintScope::Full);
    let rows = only_code(&out.diagnostics, "L-SHOT-STARTS-WITH-BACKGROUND");
    assert_eq!(rows.len(), 1, "codes: {:?}", codes(&out.diagnostics));
}

#[test]
fn shot_starts_with_background_fires_when_no_directives() {
    let doc = input(
        "scene.lute",
        "---\nkind: scene\n---\n## Shot 1.\n@alice: hi\n",
    );
    let out = lint(&[doc], &LintConfig::default(), &[], &ProviderSet::default(), None, empty_span(), LintScope::Full);
    let rows = only_code(&out.diagnostics, "L-SHOT-STARTS-WITH-BACKGROUND");
    assert_eq!(rows.len(), 1);
}

// ---------------------------------------------------------------------------
// scene-length-spread (project)
// ---------------------------------------------------------------------------

#[test]
fn scene_length_spread_fires_over_ratio() {
    // Two scenes: 4 words vs 40+ words.
    let small = input(
        "a.lute",
        "---\nkind: scene\n---\n## Shot 1.\n@alice: one two three four\n",
    );
    let big = input(
        "b.lute",
        "---\nkind: scene\n---\n## Shot 1.\n\
         @alice: one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen seventeen eighteen nineteen twenty twenty-one twenty-two twenty-three twenty-four twenty-five twenty-six twenty-seven twenty-eight twenty-nine thirty\n",
    );
    let out = lint(&[small, big], &LintConfig::default(), &[], &ProviderSet::default(), None, empty_span(), LintScope::Full);
    let rows = only_code(&out.diagnostics, "L-SCENE-LENGTH-SPREAD");
    assert_eq!(rows.len(), 1, "codes: {:?}", codes(&out.diagnostics));
}

#[test]
fn scene_length_spread_inert_on_single_scene() {
    let doc = input("a.lute", "---\nkind: scene\n---\n## Shot 1.\n@alice: hi\n");
    let out = lint(&[doc], &LintConfig::default(), &[], &ProviderSet::default(), None, empty_span(), LintScope::Full);
    let rows = only_code(&out.diagnostics, "L-SCENE-LENGTH-SPREAD");
    assert!(rows.is_empty());
}

#[test]
fn scene_length_spread_ignores_components_and_quests() {
    // A tiny component and a quest would explode the spread ratio if they
    // counted as scenes; only the two comparable SCENE documents aggregate,
    // and their ratio is under the 3.0 cap.
    let component = input(
        "c.component.lute",
        "---\ncomponent: stinger\n---\n## Shot 1.\n@alice: hi\n",
    );
    let quest = input(
        "q.lute",
        "---\nkind: quest\n---\n## Shot 1.\n@alice: hi there\n",
    );
    let scene_a = input(
        "a.lute",
        "---\nkind: scene\n---\n## Shot 1.\n@alice: one two three four five six seven eight nine ten\n",
    );
    let scene_b = input(
        "b.lute",
        "---\nkind: scene\n---\n## Shot 1.\n@alice: one two three four five six seven eight nine ten eleven twelve\n",
    );
    let out = lint(
        &[component, quest, scene_a, scene_b],
        &LintConfig::default(),
        &[],
        &ProviderSet::default(),
        None,
        empty_span(),
        LintScope::Full,
    );
    let rows = only_code(&out.diagnostics, "L-SCENE-LENGTH-SPREAD");
    assert!(rows.is_empty(), "codes: {:?}", codes(&out.diagnostics));
}

// ---------------------------------------------------------------------------
// emotion-distribution (speaker) — bard parity
// ---------------------------------------------------------------------------

#[test]
fn emotion_distribution_fires_when_run_exceeds_cap() {
    // Alice has 11 lines all with emotion=neutral → run=11 > 3, share=100%.
    let mut body = String::from("---\nkind: scene\n---\n## Shot 1.\n");
    for _ in 0..11 {
        body.push_str("@alice{emotion=\"neutral\"}: hi\n");
    }
    let doc = input("scene.lute", &body);
    let out = lint(&[doc], &LintConfig::default(), &[], &ProviderSet::default(), None, empty_span(), LintScope::Full);
    let rows = only_code(&out.diagnostics, "L-EMOTION-DISTRIBUTION");
    assert_eq!(rows.len(), 1, "codes: {:?}", codes(&out.diagnostics));
    assert!(rows[0].1.message.contains("emotion-run"), "{}", rows[0].1.message);
}

#[test]
fn emotion_distribution_skips_when_min_lines_not_met() {
    // 3 lines: below minLines (10), skip entirely.
    let doc = input(
        "scene.lute",
        "---\nkind: scene\n---\n## Shot 1.\n\
         @alice{emotion=\"neutral\"}: a\n\
         @alice{emotion=\"neutral\"}: b\n\
         @alice{emotion=\"neutral\"}: c\n",
    );
    let out = lint(&[doc], &LintConfig::default(), &[], &ProviderSet::default(), None, empty_span(), LintScope::Full);
    let rows = only_code(&out.diagnostics, "L-EMOTION-DISTRIBUTION");
    assert!(rows.is_empty(), "codes: {:?}", codes(&out.diagnostics));
}

// ---------------------------------------------------------------------------
// variant-composition (speaker/group)
// ---------------------------------------------------------------------------

#[test]
fn variant_composition_group_fires_when_thin() {
    // groupBy variant: 10 "neutral", 1 "smile" → smile group under floor=2.
    let mut body = String::from("---\nkind: scene\n---\n## Shot 1.\n");
    for _ in 0..10 {
        body.push_str("@alice{variant=\"neutral\"}: a\n");
    }
    body.push_str("@alice{variant=\"smile\"}: b\n");
    let doc = input("scene.lute", &body);
    let mut cfg = LintConfig::default();
    cfg.rules.insert(
        "variant-composition".into(),
        lute_lint::RuleOverride {
            level: None,
            options: serde_yaml::from_str("groupBy: variant\nminPerGroup: 2\n").unwrap(),
        },
    );
    let out = lint(&[doc], &cfg, &[], &ProviderSet::default(), None, empty_span(), LintScope::Full);
    let rows = only_code(&out.diagnostics, "L-VARIANT-COMPOSITION");
    assert_eq!(rows.len(), 1, "codes: {:?}", codes(&out.diagnostics));
}

#[test]
fn variant_composition_inert_when_attr_absent() {
    let mut body = String::from("---\nkind: scene\n---\n## Shot 1.\n");
    for _ in 0..12 {
        body.push_str("@alice: hi\n");
    }
    let doc = input("scene.lute", &body);
    let mut cfg = LintConfig::default();
    cfg.rules.insert(
        "variant-composition".into(),
        lute_lint::RuleOverride {
            level: None,
            options: serde_yaml::from_str("groupBy: variant\nminPerGroup: 2\n").unwrap(),
        },
    );
    let out = lint(&[doc], &cfg, &[], &ProviderSet::default(), None, empty_span(), LintScope::Full);
    let rows = only_code(&out.diagnostics, "L-VARIANT-COMPOSITION");
    assert!(rows.is_empty());
}

// ---------------------------------------------------------------------------
// asset-exists
// ---------------------------------------------------------------------------

#[test]
fn asset_exists_fires_on_absent_id() {
    let doc = input(
        "scene.lute",
        "---\nkind: scene\n---\n## Shot 1.\n\
         ::bg{location=\"a\" assetId=\"ghost\"}\n@alice: hi\n",
    );
    let mut providers = ProviderSet::from_one(ProviderSnapshot {
        manifest_version: "v1".into(),
        provider_version: "1".into(),
        entries: [("bg-catalog".to_string(), vec!["known".to_string()])].into(),
        stale: false,
    });
    // Sanity check.
    assert_eq!(providers.contains("bg-catalog", "known"), IdStatus::Fresh);
    let _ = &mut providers;

    let mut cfg = LintConfig::default();
    cfg.rules.insert(
        "asset-exists".into(),
        lute_lint::RuleOverride {
            level: None,
            options: serde_yaml::from_str("providers: { bg: bg-catalog }\n").unwrap(),
        },
    );
    let out = lint(&[doc], &cfg, &[], &providers, None, empty_span(), LintScope::Full);
    let rows = only_code(&out.diagnostics, "L-ASSET-EXISTS");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].1.severity, Severity::Error);
}

#[test]
fn asset_exists_sentinel_skips() {
    let doc = input(
        "scene.lute",
        "---\nkind: scene\n---\n## Shot 1.\n\
         ::bg{location=\"a\" assetId=\"clear\"}\n@alice: hi\n",
    );
    let providers = ProviderSet::default();
    let mut cfg = LintConfig::default();
    cfg.rules.insert(
        "asset-exists".into(),
        lute_lint::RuleOverride {
            level: None,
            options: serde_yaml::from_str("providers: { bg: bg-catalog }\n").unwrap(),
        },
    );
    let out = lint(&[doc], &cfg, &[], &providers, None, empty_span(), LintScope::Full);
    let rows = only_code(&out.diagnostics, "L-ASSET-EXISTS");
    assert!(rows.is_empty());
}

#[test]
fn asset_exists_stale_downgrades_to_warning() {
    let doc = input(
        "scene.lute",
        "---\nkind: scene\n---\n## Shot 1.\n\
         ::bg{location=\"a\" assetId=\"ghost\"}\n@alice: hi\n",
    );
    let providers = ProviderSet::from_one(ProviderSnapshot {
        manifest_version: "v1".into(),
        provider_version: "1".into(),
        entries: [("bg-catalog".to_string(), vec!["known".to_string()])].into(),
        stale: true,
    });
    let mut cfg = LintConfig::default();
    cfg.rules.insert(
        "asset-exists".into(),
        lute_lint::RuleOverride {
            level: None,
            options: serde_yaml::from_str("providers: { bg: bg-catalog }\n").unwrap(),
        },
    );
    let out = lint(&[doc], &cfg, &[], &providers, None, empty_span(), LintScope::Full);
    let rows = only_code(&out.diagnostics, "L-ASSET-EXISTS");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].1.severity, Severity::Warning);
    assert!(rows[0].1.message.contains("stale"), "{}", rows[0].1.message);
}

// ---------------------------------------------------------------------------
// config: level=off disables a rule
// ---------------------------------------------------------------------------

#[test]
fn level_off_disables_rule() {
    let doc = input(
        "scene.lute",
        "---\nkind: scene\n---\n## Shot 1.\n\
         ::music{action=\"start\"}\n@alice: hi\n",
    );
    let mut cfg = LintConfig::default();
    cfg.rules.insert(
        "shot-starts-with-background".into(),
        lute_lint::RuleOverride {
            level: Some(lute_manifest::lint::LintLevel::Off),
            options: Default::default(),
        },
    );
    let out = lint(&[doc], &cfg, &[], &ProviderSet::default(), None, empty_span(), LintScope::Full);
    let rows = only_code(&out.diagnostics, "L-SHOT-STARTS-WITH-BACKGROUND");
    assert!(rows.is_empty());
}

// ---------------------------------------------------------------------------
// config: unknown rule id → E-LINT-CONFIG
// ---------------------------------------------------------------------------

#[test]
fn unknown_rule_id_is_config_error() {
    let doc = input("scene.lute", "---\nkind: scene\n---\n## Shot 1.\n@alice: hi\n");
    let mut cfg = LintConfig::default();
    cfg.rules.insert(
        "does-not-exist".into(),
        lute_lint::RuleOverride {
            level: Some(lute_manifest::lint::LintLevel::Warn),
            options: Default::default(),
        },
    );
    let out = lint(&[doc], &cfg, &[], &ProviderSet::default(), None, empty_span(), LintScope::Full);
    let cfg_rows: Vec<&str> = out
        .config_diagnostics
        .iter()
        .map(|(_, d)| d.code.as_str())
        .collect();
    assert!(cfg_rows.contains(&"E-LINT-CONFIG"), "{cfg_rows:?}");
}

// ---------------------------------------------------------------------------
// deterministic ordering
// ---------------------------------------------------------------------------

#[test]
fn diagnostics_are_sorted_by_path_then_byte_then_code() {
    let a = input(
        "a.lute",
        "---\nkind: scene\n---\n## Shot 1.\n@alice: one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen seventeen eighteen nineteen twenty twenty-one twenty-two twenty-three twenty-four twenty-five twenty-six twenty-seven twenty-eight twenty-nine thirty thirty-one thirty-two thirty-three thirty-four thirty-five thirty-six thirty-seven thirty-eight thirty-nine forty forty-one\n",
    );
    let b = input(
        "b.lute",
        "---\nkind: scene\n---\n## Shot 1.\n@bob: hi\n",
    );
    let out = lint(&[b, a], &LintConfig::default(), &[], &ProviderSet::default(), None, empty_span(), LintScope::Full);
    // The `a.lute` finding must precede `b.lute` regardless of input order.
    let paths: Vec<String> = out
        .diagnostics
        .iter()
        .map(|(p, _)| p.display().to_string())
        .collect();
    assert!(paths.first().map(|p| p == "a.lute").unwrap_or(false), "{paths:?}");
}

// ---------------------------------------------------------------------------
// custom rule: parses and fires
// ---------------------------------------------------------------------------

#[test]
fn custom_rule_fires() {
    let doc = input(
        "scene.lute",
        "---\nkind: scene\n---\n## Shot 1.\n@alice: hi\n",
    );
    let cfg = lute_lint::parse_config(
        r#"
custom:
  - id: too-few-shots
    target: scene
    when: "scene.shots < options.min"
    level: warn
    message: "only {scene.shots} shots (need {options.min})"
    options: { min: 3 }
"#,
        empty_span(),
    )
    .unwrap()
    .0;
    let out = lint(&[doc], &cfg, &[], &ProviderSet::default(), None, empty_span(), LintScope::Full);
    let rows = only_code(&out.diagnostics, "L-TOO-FEW-SHOTS");
    assert_eq!(rows.len(), 1, "codes: {:?}", codes(&out.diagnostics));
    assert!(rows[0].1.message.contains("only 1 shots"), "{}", rows[0].1.message);
}

// ---------------------------------------------------------------------------
// E-LINT-EXPR: unresolvable field
// ---------------------------------------------------------------------------

#[test]
fn e_lint_expr_on_bad_field_reference() {
    let doc = input(
        "scene.lute",
        "---\nkind: scene\n---\n## Shot 1.\n@alice: hi\n",
    );
    let cfg = lute_lint::parse_config(
        r#"
custom:
  - id: bad-rule
    target: scene
    when: "scene.doesNotExist > 0"
    level: warn
    message: "never seen"
"#,
        empty_span(),
    )
    .unwrap()
    .0;
    let out = lint(&[doc], &cfg, &[], &ProviderSet::default(), None, empty_span(), LintScope::Full);
    let rows = only_code(&out.diagnostics, "E-LINT-EXPR");
    assert_eq!(rows.len(), 1, "codes: {:?}", codes(&out.diagnostics));
}
