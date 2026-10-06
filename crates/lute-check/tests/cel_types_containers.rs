//! Pin the 0.36.5 behavior of type checking below container expressions.
//!
//! `cel_types::check_types` deliberately treats map, struct, and comprehension
//! nodes as opaque leaves.  These cases contain a scalar type error below that
//! leaf; the tests record that the checker does not report `E-CEL-TYPE` or
//! `E-SET-TYPE` for the nested expression today.
use lute_check::{check, CheckInput, CheckResult, Mode, SchemaImports};
use lute_manifest::provider::ProviderSet;

const HDR: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  \
    run.coins: { type: int, default: 1 }\n  \
    run.flag: { type: bool, default: false }\n---\n## Shot 1.\n";

fn run(body: &str) -> CheckResult {
    let input = CheckInput {
        text: format!("{HDR}{body}\n"),
        uri: "cel_types_containers".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
        defaults: Default::default(),
    };
    check(&input)
}

fn only_profile(body: &str) {
    let ds = run(body).diagnostics;
    assert_eq!(ds.len(), 1, "{ds:?}");
    assert_eq!(ds[0].code, "E-CEL-PROFILE");
    assert_eq!(ds[0].span.line, 11, "{ds:?}");
}

#[test]
fn map_value_type_error_is_opaque_in_a_guard() {
    only_profile("@narrator{when=\"{ 'a': run.coins + 'x' }\"}: hi");
}

#[test]
fn struct_field_type_error_is_opaque_in_a_guard() {
    only_profile("@narrator{when=\"types.Entry{a: run.coins + 'x'}\"}: hi");
}

#[test]
fn comprehension_body_type_error_is_opaque_in_a_guard() {
    only_profile("@narrator{when=\"[run.coins].all(x, x + 'x')\"}: hi");
}

#[test]
fn map_value_type_error_is_opaque_in_a_set() {
    only_profile("::set{run.flag = { 'a': run.coins + 'x' }}");
}

#[test]
fn struct_field_type_error_is_opaque_in_a_set() {
    only_profile("::set{run.flag = types.Entry{a: run.coins + 'x'}}");
}

#[test]
fn comprehension_body_type_error_is_opaque_in_a_set() {
    only_profile("::set{run.flag = [run.coins].all(x, x + 'x')}");
}
