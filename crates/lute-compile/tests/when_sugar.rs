//! Task 11 (dsl 0.4.0 §7.2/§7.4) — the compile-time `when=` gated-line
//! desugar (`normalize::synth_when_match`): a content line's `when="G"`
//! guard lowers to a one-arm `<match>` record BEFORE expand/stage/address,
//! IDENTITY-PRESERVING — the desugared line keeps the SAME `lineId`/
//! `voiceKey`/`code` its hand-written explicit-match twin gets (§7.4).
//! Written FIRST (TDD): every test here fails to compile against a pre-T11
//! `normalize.rs` (`synth_when_match` does not exist / the `Node::Line`
//! rewrite is absent), then fails to pass against a stub, then passes once
//! the desugar is implemented.
//!
//! Harness mirrors `crates/lute-compile/tests/component_fold.rs` (a real
//! `CheckInput` through the full `compile()` pipeline — the desugar is only
//! externally observable through the emitted artifact) plus its temp-dir
//! component fixture idiom for the component-body regression below.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use lute_check::{parse_meta, resolve_components, resolve_imports, CheckInput, Mode};
use lute_compile::compile;
use lute_manifest::core::load_core_snapshot;
use lute_manifest::provider::ProviderSet;
use lute_manifest::snapshot::CapabilitySnapshot;

/// Compile `text` with no imports/components (`base_dir` = `.`, never
/// walked — `uses`/`extends`/`components` are empty for every scene-level
/// fixture in this file).
fn compile_text(text: &str) -> serde_json::Value {
    compile_in(Path::new("."), text)
}

/// Compile `text` resolving `components:`/`uses:`/`extends:` against `dir`
/// (on-disk fixtures), exactly like the CLI/LSP.
fn compile_in(dir: &Path, text: &str) -> serde_json::Value {
    let (doc, _) = lute_syntax::parse(text);
    let (meta0, _) = parse_meta(&doc.meta, &CapabilitySnapshot::default());
    let components = resolve_components(dir, &meta0.components, doc.meta.span);
    let imports = resolve_imports(dir, &meta0.uses, &meta0.extends, doc.meta.span);
    let input = CheckInput {
        text: text.to_string(),
        uri: "scene.lute".into(),
        snapshot: load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Ci,
        imports,
        components,
    };
    let artifact =
        compile(&input).unwrap_or_else(|e| panic!("scene compiles clean: {e:#?}\n{text}"));
    serde_json::to_value(&artifact).expect("artifact serializes")
}

fn commands(artifact: &serde_json::Value) -> &[serde_json::Value] {
    artifact["commands"].as_array().expect("commands array")
}

/// Count `{"kind":"match", ...}` entries — the direct observable proof a
/// gated line DID (residual) or did NOT (folded/absent) lower to a match
/// command record.
fn match_count(artifact: &serde_json::Value) -> usize {
    commands(artifact).iter().filter(|c| c["kind"] == "match").count()
}

fn lines(artifact: &serde_json::Value) -> Vec<&serde_json::Value> {
    commands(artifact).iter().filter(|c| c["kind"] == "line").collect()
}

/// Recursively strip position/label churn: `addr` (a record's own
/// position), `converge`/`target`/`otherwise` (symbolic jump targets
/// resolved to positions) — the ONLY fields two structurally-isomorphic
/// documents may legitimately differ on. Everything else — including
/// `MatchCmd.subject`, `arms[].test`, `arms[].expr`, and the nested
/// `LineCmd` (identity fields included) — stays, and must compare equal.
fn strip_addressing(v: &mut serde_json::Value) {
    if let serde_json::Value::Object(map) = v {
        for key in ["addr", "converge", "target", "otherwise"] {
            map.remove(key);
        }
    }
    match v {
        serde_json::Value::Object(map) => map.values_mut().for_each(strip_addressing),
        serde_json::Value::Array(arr) => arr.iter_mut().for_each(strip_addressing),
        _ => {}
    }
}

fn scene(body: &str) -> String {
    format!(
        "---\nkind: scene\ncharacter: sofia\nseason: 1\nepisode: 1\nstate:\n  run.metHelpfully: {{ type: bool, default: false }}\n---\n## Shot 1.\n{body}\n"
    )
}

const SUGARED_LINE: &str = "@sofia{when=\"run.metHelpfully\"}: You helped me back then.\n";
const EXPLICIT_TWIN: &str = "<match on=\"run.metHelpfully\">\n  <when test=\"$\">\n    @sofia: You helped me back then.\n  </when>\n  <otherwise>\n  </otherwise>\n</match>\n";

#[test]
fn sugared_line_lowers_to_canonical_match_record() {
    // Compile TWIN documents: (a) the sugared line, (b) the hand-written
    // `<match on="…"><when test="$">…</when><otherwise></otherwise></match>`
    // (D8/§7.4's canonical desugar shape).
    let sugared = compile_text(&scene(SUGARED_LINE));
    let explicit = compile_text(&scene(EXPLICIT_TWIN));

    // Command-stream equality (§7.4's "MUST lower to that same match
    // record"): one match record; subject/arm-test/arm-expr and the nested
    // line all equal, modulo addr/converge/target-label churn.
    let mut sugared_cmds = sugared["commands"].clone();
    let mut explicit_cmds = explicit["commands"].clone();
    strip_addressing(&mut sugared_cmds);
    strip_addressing(&mut explicit_cmds);
    assert_eq!(
        sugared_cmds, explicit_cmds,
        "sugared vs. explicit command streams must be equal modulo addr/converge/target-label churn"
    );
    assert_eq!(match_count(&sugared), 1, "exactly one match record either way");

    // Identity (§7.4): the SAME lineId/voiceKey — wrapping cannot move them
    // (address.rs:85-104 — identity is shot-prefix + emission-order scoped,
    // and both docs emit exactly one line at the same position).
    let sugared_lines = lines(&sugared);
    let explicit_lines = lines(&explicit);
    assert_eq!(sugared_lines.len(), 1);
    assert_eq!(explicit_lines.len(), 1);
    assert!(
        sugared_lines[0]["lineId"].as_str().is_some_and(|s| !s.is_empty()),
        "lineId must be a real, non-empty identity string"
    );
    assert_eq!(
        sugared_lines[0]["lineId"], explicit_lines[0]["lineId"],
        "the sugared line's lineId must be IDENTICAL to its explicit twin's"
    );
    assert_eq!(
        sugared_lines[0]["voiceKey"], explicit_lines[0]["voiceKey"],
        "the sugared line's voiceKey must be IDENTICAL to its explicit twin's"
    );
}

#[test]
fn code_backfill_is_unaffected() {
    // An authored `code=` anchor first (so back-fill has a max to step
    // past), then the sugared/explicit second line, untagged.
    const ANCHOR: &str = "@sofia{code=\"0010\"}: An anchor line.\n";
    let sugared = compile_text(&scene(&format!("{ANCHOR}{SUGARED_LINE}")));
    let explicit = compile_text(&scene(&format!("{ANCHOR}{EXPLICIT_TWIN}")));

    let sl = lines(&sugared);
    let el = lines(&explicit);
    assert_eq!(sl.len(), 2);
    assert_eq!(el.len(), 2);

    // Authored code= survives, either way (encoded in lineId — `code` is
    // never itself serialized, ir.rs `LineCmd.code` is `#[serde(skip)]`).
    assert!(
        sl[0]["lineId"].as_str().unwrap().ends_with("sofia_0010"),
        "authored code= must survive on the anchor line: {:?}",
        sl[0]["lineId"]
    );
    assert_eq!(sl[0]["lineId"], el[0]["lineId"]);

    // The untagged sugared/explicit line back-fills to max(0010)+10 = 0020
    // exactly as its twin (§7.4).
    assert!(
        sl[1]["lineId"].as_str().unwrap().ends_with("sofia_0020"),
        "untagged sugared line must back-fill to max-authored+10: {:?}",
        sl[1]["lineId"]
    );
    assert_eq!(
        sl[1]["lineId"], el[1]["lineId"],
        "back-filled code must match the explicit twin's exactly"
    );
    assert_eq!(sl[1]["voiceKey"], el[1]["voiceKey"]);
}

#[test]
fn no_sugar_document_is_byte_identical() {
    // B2: a document with no `when=` anywhere passes through the new
    // normalize arm as a genuine no-op — determinism + absence of any
    // synthesized match record here; the authoritative byte-stability
    // proof is the untouched insta goldens (crates/lute-compile/tests/
    // e2e.rs bianca_s01ep02/showcase_episode01/quest_grove/
    // quest_rescue_halsin/components_scene/affinity_reaction).
    let text = scene("@sofia: Hello there.\n@sofia{code=\"0099\"}: Again.\n");
    let a = compile_text(&text);
    let b = compile_text(&text);
    assert_eq!(a, b, "compiling the same when=-free document twice must be byte-identical");
    assert_eq!(match_count(&a), 0, "no when= anywhere => no synthesized match record");
    assert_eq!(lines(&a).len(), 2);
}

#[test]
fn sugared_line_survives_compilation() {
    // examples_compile-style regression guard (27653b6 precedent): a gated
    // line must not silently vanish under desugar. Compiles the shipped
    // worked example (docs/examples/gated-line.lute) and asserts BOTH the
    // sugared line (shot 1) and its living explicit twin (shot 2) emit
    // their (speaker, text) pair.
    let text = std::fs::read_to_string("../../docs/examples/gated-line.lute")
        .expect("docs/examples/gated-line.lute exists");
    let artifact = compile_text(&text);
    let pairs: Vec<(String, String)> = lines(&artifact)
        .into_iter()
        .map(|l| {
            (
                l["speaker"].as_str().unwrap().to_string(),
                l["text"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    let expected =
        "You helped me back then. I've been meaning to thank you.".to_string();
    assert_eq!(
        pairs,
        vec![
            ("sofia".to_string(), expected.clone()),
            ("sofia".to_string(), expected),
        ],
        "both the sugared line and its explicit twin must survive compilation intact"
    );
}

// -- component-body gated-line `@param` binding (bind_params fix) ----------

static UNIQ: AtomicU64 = AtomicU64::new(0);

fn unique_dir() -> PathBuf {
    let n = UNIQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!(
        "lute_when_sugar_{}_{}_{}",
        std::process::id(),
        n,
        nanos
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_lute(dir: &Path, name: &str, body: &str) {
    std::fs::write(dir.join(name), body).unwrap();
}

/// A component body whose ONLY content is a gated line guarded by a param
/// (dsl §7.2/§6.2: "Inside a component body the guard may reference only
/// params, else `E-COMPONENT-STATE`" — `@tier` is a bare param read, clean).
const GREETING_COMPONENT: &str = "---\ncomponent: greeting\nparams:\n  tier: { enum: [cold, warm, fond] }\n---\n## Scene 1.\n@bianca{when=\"@tier == 'fond'\"}: You remembered!\n";

fn use_scene(tier_arg: &str) -> String {
    format!(
        "---\nkind: scene\ncharacter: demo\nseason: 1\nepisode: 1\ncomponents: [greeting.lute]\n---\n## Shot 1.\n::use{{component=\"greeting\" tier=\"{tier_arg}\"}}\n"
    )
}

#[test]
fn component_gated_line_when_binds_param_before_fold() {
    // T9 left a real gap: `bind_params`'s `Node::Line` arm bound only
    // `l.attrs`, never `l.when` — a component-body gated line's `@param`
    // stayed UNBOUND through `::use` expansion, so this task's when=>match
    // desugar (and T8's `fold_component_matches`, which runs immediately
    // after on the SAME bound clone) would operate on unresolved text
    // instead of the caller's actual argument.
    let dir = unique_dir();
    write_lute(&dir, "greeting.lute", GREETING_COMPONENT);

    // tier="fond" binds `@tier == 'fond'` -> `'fond' == 'fond'` (decides
    // true) -> the synthesized one-arm match's "$" test decides true ->
    // §6.4 case 1: zero residual match, the line shows.
    let fond = compile_in(&dir, &use_scene("fond"));
    assert_eq!(
        match_count(&fond),
        0,
        "a fully literal-bound gated-line guard must fold away entirely"
    );
    let fond_lines = lines(&fond);
    assert_eq!(fond_lines.len(), 1, "the gated line shows exactly once");
    assert_eq!(fond_lines[0]["speaker"], "bianca");
    assert_eq!(fond_lines[0]["text"], "You remembered!");

    // tier="cold" binds `'cold' == 'fond'` -> decides false -> the
    // implicit empty <otherwise/> is selected -> zero lines, zero residual
    // match. Proves genuine binding, not a guard-always-true bug.
    let cold = compile_in(&dir, &use_scene("cold"));
    assert_eq!(
        match_count(&cold),
        0,
        "a fully literal-bound gated-line guard must fold away entirely"
    );
    assert_eq!(
        lines(&cold).len(),
        0,
        "the guard decided false: the gated line must not show"
    );
}
