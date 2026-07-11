//! Task 10 (dsl 0.4.0 §7.2) — checker + LSP semantics for the `when=`
//! content-line guard: the slot is validated as an ordinary `Bool` Condition
//! (D9: `$` stays out of scope, matching `<on when>`, even inside a
//! `<match>` arm), it PROVES the line's own `{{…}}` interpolations (a
//! non-dominating fork mirroring `walk_hub`/`walk_on`), a provably-false
//! guard is `E-ARM-DEAD` (a one-arm construct, §5.2), a component body's
//! guard is params-only (`E-COMPONENT-STATE` for an ambient-state read), the
//! extracted `when` attr never trips `E-UNKNOWN-ATTR`, and it composes with
//! every other content-line attr check unchanged.

use lute_check::{check, parse_meta, resolve_components, CheckInput, Mode, SchemaImports};
use lute_manifest::provider::ProviderSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const HDR: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n";

fn codes(text: &str) -> Vec<String> {
    let input = CheckInput {
        text: text.to_string(),
        uri: "line_when".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
    };
    check(&input)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn guard_is_checked_as_condition() {
    // An undeclared path in the guard is the ordinary CEL resolver's
    // E-UNDECLARED (dsl §9.4/§9.6) — the same treatment any other guard gets.
    let undeclared = codes(&format!(
        "{HDR}state:\n  run.flag: {{ type: bool, default: false }}\n---\n## Shot 1.\n\
         @x{{when=\"run.nope\"}}: hi\n"
    ));
    assert!(
        undeclared.contains(&"E-UNDECLARED".to_string()),
        "{undeclared:?}"
    );

    // The guard runs through the SAME closed CEL profile as any other guard
    // (`<choice when>`, `<on when>`, …, dsl §8.4): an out-of-profile call
    // (`size(...)`) trips E-CEL-PROFILE exactly like it would anywhere else.
    let profile = codes(&format!(
        "{HDR}state:\n  run.flag: {{ type: bool, default: false }}\n---\n## Shot 1.\n\
         @x{{when=\"size(run.flag) > 0\"}}: hi\n"
    ));
    assert!(
        profile.contains(&"E-CEL-PROFILE".to_string()),
        "{profile:?}"
    );
}

#[test]
fn dollar_out_of_scope_even_in_match() {
    // D9: a content-line `when=` never sees `$`, even nested inside a
    // `<match>`'s `<when>` arm — mirrors the `<on when>` rule.
    let t = format!(
        "{HDR}state:\n  scene.g: {{ type: bool, default: false }}\n---\n## Shot 1.\n\
         <match on=\"scene.g\">\n\
         <when test=\"$ == true\">\n\
         @x{{when=\"$ == 1\"}}: hi\n\
         </when>\n\
         <otherwise>\n@narrator: b\n</otherwise>\n\
         </match>\n"
    );
    let cs = codes(&t);
    assert!(
        cs.contains(&"E-DOLLAR-OUTSIDE-MATCH".to_string()),
        "{cs:?}"
    );
}

#[test]
fn guard_proves_interp_reads() {
    // `run.tip` is declared but carries no default, so an unguarded read is
    // E-MAYBE-UNSET (dsl §9.4) — but `when="isSet(run.tip)"` proves the SAME
    // line's own `{{run.tip}}` interpolation (dsl §9.4's guard-proof rule).
    let hdr = format!("{HDR}state:\n  run.tip: {{ type: number }}\n---\n## Shot 1.\n");

    let guarded = format!("{hdr}@x{{when=\"isSet(run.tip)\"}}: got {{{{run.tip}}}}.\n");
    let cs = codes(&guarded);
    assert!(
        !cs.contains(&"E-MAYBE-UNSET".to_string()),
        "guard must prove the read: {cs:?}"
    );

    let unguarded = format!("{hdr}@x: got {{{{run.tip}}}}.\n");
    let cs2 = codes(&unguarded);
    assert!(
        cs2.contains(&"E-MAYBE-UNSET".to_string()),
        "without the guard the read stays maybe-unset: {cs2:?}"
    );
}

#[test]
fn decided_false_guard_is_arm_dead() {
    // A guard that decides false makes the line a one-arm construct whose
    // single arm can never fire (dsl §5.2 rule 1, applied to §7.2).
    let t = format!("{HDR}---\n## Shot 1.\n@x{{when=\"1 > 2\"}}: hi\n");
    let cs = codes(&t);
    assert!(cs.contains(&"E-ARM-DEAD".to_string()), "{cs:?}");
}

// -- component_line_when_params_only harness (mirrors component_match.rs) ---

static UNIQ: AtomicU64 = AtomicU64::new(0);

fn unique_dir() -> PathBuf {
    let n = UNIQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!(
        "lute_line_when_{}_{}_{}",
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

fn scene(components: &str, body: &str) -> String {
    format!(
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\ncomponents: [{components}]\n---\n## Shot 1.\n{body}\n"
    )
}

/// Resolve `components:` from `dir` and run the assembled `check()` — mirrors
/// `tests/component_match.rs`'s harness.
fn component_codes(dir: &Path, scene: &str) -> Vec<String> {
    let (doc, _) = lute_syntax::parse(scene);
    let (meta0, _) =
        parse_meta(&doc.meta, &lute_manifest::snapshot::CapabilitySnapshot::default());
    let components = resolve_components(dir, &meta0.components, doc.meta.span);
    let input = CheckInput {
        text: scene.to_string(),
        uri: "scene".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Ci,
        imports: Default::default(),
        components,
    };
    check(&input)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn component_line_when_params_only() {
    // A bare-param guard stays clean.
    let dir = unique_dir();
    write_lute(
        &dir,
        "reaction.lute",
        "---\ncomponent: reaction\nparams:\n  tier: { enum: [cold, warm, fond] }\n---\n\
         ## Scene 1.\n\
         @bianca{when=\"@tier == 'fond'\"}: You remembered!\n",
    );
    let s = scene("reaction.lute", "::use{component=\"reaction\" tier=\"fond\"}");
    let cs = component_codes(&dir, &s);
    assert!(
        !cs.iter().any(|c| c.starts_with("E-")),
        "a params-only when= guard must stay clean; got {cs:?}"
    );

    // An ambient-state guard is E-COMPONENT-STATE.
    let dir2 = unique_dir();
    write_lute(
        &dir2,
        "leaky.lute",
        "---\ncomponent: leaky\n---\n## Scene 1.\n@narrator{when=\"run.x\"}: hi\n",
    );
    let s2 = scene("leaky.lute", "::use{component=\"leaky\"}");
    let cs2 = component_codes(&dir2, &s2);
    assert!(
        cs2.contains(&"E-COMPONENT-STATE".to_string()),
        "an ambient-state when= guard must flag E-COMPONENT-STATE; got {cs2:?}"
    );
}

#[test]
fn component_line_when_also_runs_ordinary_cel_validation() {
    // Finding 1: a component-body `when=` must run the SAME ordinary CEL
    // validation (`check_cel_slot`) as every other component-body slot —
    // not just the positive `E-COMPONENT-STATE` ambient-state scan. An
    // undeclared `@ref` is E-UNDECLARED-REF (neither dedup'd code, dsl §13).
    let dir = unique_dir();
    write_lute(
        &dir,
        "undeclared_ref.lute",
        "---\ncomponent: undeclared_ref\n---\n## Scene 1.\n\
         @narrator{when=\"@missing\"}: hi\n",
    );
    let s = scene("undeclared_ref.lute", "::use{component=\"undeclared_ref\"}");
    let cs = component_codes(&dir, &s);
    assert!(
        cs.contains(&"E-UNDECLARED-REF".to_string()),
        "an undeclared @ref in a component when= must be E-UNDECLARED-REF; got {cs:?}"
    );

    // D9: `$` is out of scope in a content-line when=, even inside a
    // component body — check_cel_slot must run under a no-dollar ctx.
    let dir2 = unique_dir();
    write_lute(
        &dir2,
        "dollar_leak.lute",
        "---\ncomponent: dollar_leak\nparams:\n  tier: { enum: [cold, fond] }\n---\n\
         ## Scene 1.\n\
         @narrator{when=\"$ == 'fond'\"}: hi\n",
    );
    let s2 = scene("dollar_leak.lute", "::use{component=\"dollar_leak\" tier=\"fond\"}");
    let cs2 = component_codes(&dir2, &s2);
    assert!(
        cs2.contains(&"E-DOLLAR-OUTSIDE-MATCH".to_string()),
        "`$` in a component when= must be E-DOLLAR-OUTSIDE-MATCH; got {cs2:?}"
    );

    // A declared-param guard stays clean under the new validation, exactly
    // as it did before (no regression on the params-only happy path).
    let dir3 = unique_dir();
    write_lute(
        &dir3,
        "clean_param.lute",
        "---\ncomponent: clean_param\nparams:\n  tier: { enum: [cold, fond] }\n---\n\
         ## Scene 1.\n\
         @narrator{when=\"@tier == 'fond'\"}: hi\n",
    );
    let s3 = scene("clean_param.lute", "::use{component=\"clean_param\" tier=\"fond\"}");
    let cs3 = component_codes(&dir3, &s3);
    assert!(
        !cs3.iter().any(|c| c.starts_with("E-")),
        "a declared-param when= guard must stay clean; got {cs3:?}"
    );
}

#[test]
fn known_attrs_never_sees_when() {
    // Extraction (T9's parser change) precedes content_line.rs's known-attr
    // check, so `when=` never trips E-UNKNOWN-ATTR.
    let t = format!(
        "{HDR}state:\n  run.flag: {{ type: bool, default: false }}\n---\n## Shot 1.\n\
         @s{{when=\"run.flag\"}}: x\n"
    );
    let cs = codes(&t);
    assert!(!cs.contains(&"E-UNKNOWN-ATTR".to_string()), "{cs:?}");
}

#[test]
fn delivery_and_emotion_checks_hold() {
    // `when=` composes with every other content-line attr check, unchanged
    // (dsl §7.2: "All existing attr checks ... apply unchanged").
    let clean = format!(
        "{HDR}state:\n  run.flag: {{ type: bool, default: false }}\n---\n## Shot 1.\n\
         @x{{when=\"run.flag\" emotion=\"neutral\" mono}}: hi\n"
    );
    let cs = codes(&clean);
    assert!(
        !cs.iter().any(|c| c.starts_with("E-DELIVERY")
            || c == "E-BAD-ENUM"
            || c == "E-UNKNOWN-ATTR"),
        "{cs:?}"
    );

    // Delivery-flag exclusivity (dsl §D7) still fires alongside `when=`.
    let conflict = format!(
        "{HDR}state:\n  run.flag: {{ type: bool, default: false }}\n---\n## Shot 1.\n\
         @x{{when=\"run.flag\" mono os}}: hi\n"
    );
    let cs2 = codes(&conflict);
    assert!(cs2.contains(&"E-DELIVERY-CONFLICT".to_string()), "{cs2:?}");

    // A bad emotion value still fires alongside `when=`.
    let bad_emotion = format!(
        "{HDR}state:\n  run.flag: {{ type: bool, default: false }}\n---\n## Shot 1.\n\
         @x{{when=\"run.flag\" emotion=\"zzz\"}}: hi\n"
    );
    let cs3 = codes(&bad_emotion);
    assert!(cs3.contains(&"E-BAD-ENUM".to_string()), "{cs3:?}");
}
