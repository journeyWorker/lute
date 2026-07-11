//! `Type::NarrativeTime` + `E-TEMPORAL-ARG` (dsl 0.3.0 §6, Task 12). Mirrors
//! `tests/fact_query.rs`'s harness helpers.
//!
//! **D8 (controller-tightened):** the admitted comparison set between two
//! narrative-time values is the FIVE ops `<`, `<=`, `==`, `>`, `>=` — `!=` is
//! REJECTED (`E-TEMPORAL-ARG`), diverging from an earlier plan-body reading
//! that treated `!=` as legal. Every fixture below reflects the tightened
//! reading.

use lute_check::{check, CheckInput, Mode, Namespace, SchemaImports, StateDecl};
use lute_manifest::provider::ProviderSet;
use lute_manifest::types::Type;

fn codes(text: &str) -> Vec<String> {
    let input = CheckInput {
        text: text.to_string(),
        uri: "t".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
    };
    check(&input).diagnostics.into_iter().map(|d| d.code).collect()
}

/// A scene whose `<choice when=…>` slot carries `cond`, plus a second
/// unguarded choice (keeps `E-BRANCH-ALL-GUARDED` out of scope, same
/// discipline as `tests/fact_query.rs::scene_when`).
fn scene_when(cond: &str) -> String {
    format!(
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n<branch>\n<choice id=\"a\" label=\"a\" when=\"{cond}\">\n@narrator: hi\n</choice>\n<choice id=\"b\" label=\"b\">\n@narrator: bye\n</choice>\n</branch>\n",
    )
}

/// An engine-declared narrative-time anchor (`app.episodeStart`), simulated by
/// injecting it into `imports.state` directly (the checker treats imported
/// state and capability shapes uniformly once folded) — mirrors the plan's
/// own fixture shape for a plugin `state_shapes` export.
fn input_with_anchor(cond: &str) -> CheckInput {
    let mut imports = SchemaImports::default();
    imports.state.decls.insert(
        "app.episodeStart".to_string(),
        StateDecl {
            ty: Type::NarrativeTime,
            default: None,
            namespace: Namespace::App,
        },
    );
    CheckInput {
        text: format!(
            "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n<branch>\n<choice id=\"a\" label=\"a\" when=\"{cond}\">\n@narrator: hi\n</choice>\n<choice id=\"b\" label=\"b\">\n@narrator: bye\n</choice>\n</branch>\n"
        ),
        uri: "t".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports,
        components: Default::default(),
    }
}

fn anchor_codes(cond: &str) -> Vec<String> {
    check(&input_with_anchor(cond)).diagnostics.into_iter().map(|d| d.code).collect()
}

const VOCAB: &str =
    "entities:\n  c: { members: [ana] }\nrelations:\n  inParty: { args: [c] }\n";

fn validat_scene(t_arg: &str) -> String {
    format!(
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n{VOCAB}---\n## Shot 1.\n<branch>\n<choice id=\"a\" label=\"a\" when=\"validAt(inParty(ana), {t_arg})\">\n@narrator: hi\n</choice>\n<choice id=\"b\" label=\"b\">\n@narrator: bye\n</choice>\n</branch>\n",
    )
}

#[test]
fn ordering_comparisons_between_times_are_legal() {
    // D8 (five ops): `<`, `<=`, `==`, `>`, `>=` — between two `now()` calls and
    // between an engine-declared anchor and `now()`.
    for cond in ["now() < now()", "now() <= now()", "now() == now()", "now() > now()", "now() >= now()"] {
        let c = codes(&scene_when(cond));
        assert!(!c.contains(&"E-TEMPORAL-ARG".to_string()), "{cond}: {c:?}");
    }
    for cond in [
        "app.episodeStart < now()",
        "now() <= app.episodeStart",
        "app.episodeStart == now()",
        "now() > app.episodeStart",
        "app.episodeStart >= now()",
    ] {
        let c = anchor_codes(cond);
        assert!(!c.contains(&"E-TEMPORAL-ARG".to_string()), "{cond}: {c:?}");
    }
}

#[test]
fn not_equals_between_times_is_rejected_by_d8() {
    // D8 (controller-tightened): `!=` is REJECTED even between two
    // narrative-time values — the earlier "all six ops" plan-body reading is
    // superseded by the Decisions section.
    for cond in ["now() != now()", "app.episodeStart != now()"] {
        let c = anchor_codes(cond);
        assert!(c.contains(&"E-TEMPORAL-ARG".to_string()), "D8: {cond}: {c:?}");
    }
}

#[test]
fn arithmetic_and_mixed_comparison_are_temporal_arg() {
    for cond in [
        "now() + 1 == now()",
        "now() < 5",
        "-now() == now()",
        "now() && true",
        "now()[0] == now()",
    ] {
        let c = codes(&scene_when(cond));
        assert!(c.contains(&"E-TEMPORAL-ARG".to_string()), "{cond}: {c:?}");
    }
    let c = anchor_codes("app.episodeStart * 2 > now()");
    assert!(c.contains(&"E-TEMPORAL-ARG".to_string()), "{c:?}");
}

#[test]
fn nt_at_bool_root_is_temporal_arg() {
    let c = codes(&scene_when("now()"));
    assert!(c.contains(&"E-TEMPORAL-ARG".to_string()), "{c:?}");
}

#[test]
fn nt_anchor_at_bool_root_is_temporal_arg() {
    let c = anchor_codes("app.episodeStart");
    assert!(c.contains(&"E-TEMPORAL-ARG".to_string()), "{c:?}");
}

#[test]
fn validat_second_arg_must_be_narrative_time() {
    let clean = codes(&validat_scene("now()"));
    assert!(!clean.contains(&"E-TEMPORAL-ARG".to_string()), "validAt(rel, now()) is clean: {clean:?}");

    let bad = codes(&validat_scene("5"));
    assert!(bad.contains(&"E-TEMPORAL-ARG".to_string()), "validAt(rel, 5): {bad:?}");
}

#[test]
fn author_state_decl_of_narrative_time_is_rejected() {
    // D11: `narrativeTime` is engine-surfaced only; an author `state:` decl of
    // it is E-TEMPORAL-ARG at the decl, and the path is NOT registered (so it
    // reads back as plain undeclared, never narrative-time-typed).
    let c = codes(
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  run.myClock: { type: narrativeTime }\n---\n## Shot 1.\n@narrator: hi\n",
    );
    assert!(c.contains(&"E-TEMPORAL-ARG".to_string()), "D11: {c:?}");
}

#[test]
fn undeclared_anchor_path_stays_e_undeclared() {
    // No `state:`/`uses:` decl at all for `app.neverDeclared` — reading it
    // stays a plain E-UNDECLARED; it must NOT be treated as narrative-time
    // just because it looks like an anchor path (D11 reuse note).
    let c = codes(&scene_when("app.neverDeclared"));
    assert!(c.contains(&"E-UNDECLARED".to_string()), "{c:?}");
    assert!(!c.contains(&"E-TEMPORAL-ARG".to_string()), "{c:?}");
}
