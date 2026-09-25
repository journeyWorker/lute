//! dsl 0.24.0 §1 — `::set{… when="…"}`: the guard is an ordinary `Bool`
//! Condition slot (the same profile/type/`$` rule as a content-line `when=`),
//! it narrows the write's own reads, a provably-false guard is `E-ARM-DEAD`,
//! and a guarded write is NOT a definite assignment (a later read of a
//! maybe-unset path stays `E-MAYBE-UNSET`).
use lute_check::{check, CheckInput, Mode, SchemaImports};
use lute_manifest::provider::ProviderSet;

const HDR: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  \
    run.a: { type: number, default: 0 }\n  \
    run.b: { type: number, default: 0 }\n  \
    run.day: { type: number, default: 1 }\n  \
    run.tip: { type: number }\n  \
    run.flag: { type: bool, default: false }\n\
    defs:\n  count: { type: number, cel: \"run.b + 1\" }\n  \
    warm: { type: bool, cel: \"run.b > 0\" }\n---\n## Shot 1.\n";

fn codes(body: &str) -> Vec<String> {
    let input = CheckInput {
        text: format!("{HDR}{body}\n"),
        uri: "set_when".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
        defaults: Default::default(),
    };
    check(&input)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn guarded_set_is_clean() {
    let cs = codes("::set{run.a += 1 when=\"run.b < run.day\"}");
    assert!(cs.is_empty(), "{cs:?}");
}

#[test]
fn guard_is_checked_as_a_bool_condition() {
    let undeclared = codes("::set{run.a += 1 when=\"run.nope\"}");
    assert!(
        undeclared.contains(&"E-UNDECLARED".to_string()),
        "{undeclared:?}"
    );

    // A guard is typed against `bool` exactly like a line `when=`: a
    // whole-slot `@ref` of another type is `E-REF-TYPE`, a bool one is clean.
    let not_bool = codes("::set{run.a += 1 when=\"@count\"}");
    assert!(not_bool.contains(&"E-REF-TYPE".to_string()), "{not_bool:?}");
    let line = codes("@x{when=\"@count\"}: hi");
    assert!(line.contains(&"E-REF-TYPE".to_string()), "{line:?}");
    let bool_ref = codes("::set{run.a += 1 when=\"@warm\"}");
    assert!(bool_ref.is_empty(), "{bool_ref:?}");
}

#[test]
fn dollar_is_out_of_scope_in_the_guard_even_in_a_match() {
    let cs = codes(
        "<match on=\"run.flag\">\n<when test=\"$ == true\">\n\
         ::set{run.a = 1 when=\"$ == true\"}\n</when>\n<otherwise>\n@narrator: b\n</otherwise>\n</match>",
    );
    assert!(cs.contains(&"E-DOLLAR-OUTSIDE-MATCH".to_string()), "{cs:?}");
}

#[test]
fn decided_false_guard_is_arm_dead() {
    let cs = codes("::set{run.a = 1 when=\"1 > 2\"}");
    assert!(cs.contains(&"E-ARM-DEAD".to_string()), "{cs:?}");
}

/// A guarded first write does not definitely assign: the later read stays
/// maybe-unset. The unguarded write still proves it.
#[test]
fn guarded_write_is_not_a_definite_assignment() {
    let guarded = codes("::set{run.tip = 5 when=\"run.flag\"}\n@x: tip {{run.tip}}");
    assert!(
        guarded.contains(&"E-MAYBE-UNSET".to_string()),
        "{guarded:?}"
    );

    let unguarded = codes("::set{run.tip = 5}\n@x: tip {{run.tip}}");
    assert!(
        !unguarded.contains(&"E-MAYBE-UNSET".to_string()),
        "{unguarded:?}"
    );
}

/// The guard narrows the write's OWN reads: `isSet(run.tip)` proves the
/// compound op's old-value read of `run.tip`.
#[test]
fn guard_proves_the_writes_own_reads() {
    let guarded = codes("::set{run.tip += 1 when=\"isSet(run.tip)\"}");
    assert!(
        !guarded.contains(&"E-MAYBE-UNSET".to_string()),
        "{guarded:?}"
    );

    let unguarded = codes("::set{run.tip += 1}");
    assert!(
        unguarded.contains(&"E-MAYBE-UNSET".to_string()),
        "{unguarded:?}"
    );
}
