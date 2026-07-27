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

// --- dsl 0.8.0 §5: `quest.<id>.activatedAt`, the reserved narrative-time
// anchor. `validAt(rel, t)` shipped in 0.3.0 §8 with no author-writable `t`;
// the quest-instance activation instant is that `t`. -------------------------

/// Entity/relation vocabulary plus a seed fact, so the queried relation is
/// PRODUCIBLE (`producible.rs`) — an objective gated on a never-producible
/// relation is `E-OBJECTIVE-UNSATISFIABLE`, a §4.2 reachability verdict
/// orthogonal to narrative-time typing.
const QUEST_VOCAB: &str = "entities:\n  loc: { members: [map] }\nrelations:\n  \
                           sawClue: { args: [loc] }\nfacts:\n  - \"sawClue(map)\"\n";

/// A `kind: quest` document whose single `<objective done>` slot carries
/// `cond` — the spec's own §5 example shape.
fn quest_done(cond: &str) -> String {
    format!(
        "---\nkind: quest\n{QUEST_VOCAB}---\n<quest id=\"q1\">\n\
         <objective id=\"o\" done=\"{cond}\"/>\n</quest>\n",
    )
}

#[test]
fn validat_against_quest_activated_at_is_clean() {
    // The whole point of the slot: `validAt`'s second argument now has an
    // author-writable narrative-time expression. No E-TEMPORAL-ARG (the
    // anchor classifies as narrative time), no E-UNDECLARED (it is an
    // implicitly-declared reserved quest field), no E-MAYBE-UNSET (narrative
    // time admits no `isSet`/`has` guard — that would itself be
    // E-TEMPORAL-ARG — so the engine-populated anchor is definite).
    let cs = codes(&quest_done("validAt(sawClue(map), quest.q1.activatedAt)"));
    for code in ["E-TEMPORAL-ARG", "E-UNDECLARED", "E-MAYBE-UNSET", "E-CEL-PROFILE"] {
        assert!(!cs.contains(&code.to_string()), "{code}: {cs:?}");
    }
    assert!(!cs.iter().any(|c| c.starts_with("E-")), "{cs:?}");
}

#[test]
fn foreign_quest_activated_at_is_narrative_time_too() {
    // A quest THIS document never folds: the reserved shape is implicitly
    // declared project-wide, so `is_nt` must recognise it WITHOUT a schema
    // decl to resolve against — otherwise `validAt`'s second argument would
    // be E-TEMPORAL-ARG purely because the quest lives in another file.
    let cs = codes(&quest_done("validAt(sawClue(map), quest.elsewhere.activatedAt)"));
    assert!(!cs.contains(&"E-TEMPORAL-ARG".to_string()), "{cs:?}");
    assert!(!cs.contains(&"E-UNDECLARED".to_string()), "{cs:?}");
}

#[test]
fn author_decl_of_quest_activated_at_is_reserved_decl() {
    let cs = codes(
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n\
         state:\n  quest.q1.activatedAt: { type: number }\n---\n## Shot 1.\n@x: hi\n",
    );
    assert!(cs.contains(&"E-QUEST-RESERVED-DECL".to_string()), "{cs:?}");
}

#[test]
fn set_to_quest_activated_at_is_reserved_write() {
    let cs = codes(
        "---\nkind: quest\n---\n<quest id=\"q1\">\n<objective id=\"o\" done=\"run.d\"/>\n\
         <on event=\"questActive\">\n::set{quest.q1.activatedAt = 1}\n</on>\n</quest>\n",
    );
    assert!(cs.contains(&"E-QUEST-RESERVED-WRITE".to_string()), "{cs:?}");
}

#[test]
fn not_equals_between_two_activated_at_anchors_is_rejected_by_d8() {
    // D8 is unchanged by the new slot: `!=` stays outside the ordering-only
    // surface even between two genuine narrative-time values.
    let cs = codes(&quest_done("quest.q1.activatedAt != quest.q2.activatedAt"));
    assert!(cs.contains(&"E-TEMPORAL-ARG".to_string()), "D8: {cs:?}");

    // ...while the five admitted ordering ops between the same two anchors
    // stay legal, so the rejection above is D8 and not a classification miss.
    for op in ["<", "<=", "==", ">", ">="] {
        let ok = codes(&quest_done(&format!(
            "quest.q1.activatedAt {op} quest.q2.activatedAt"
        )));
        assert!(!ok.contains(&"E-TEMPORAL-ARG".to_string()), "{op}: {ok:?}");
    }
}
