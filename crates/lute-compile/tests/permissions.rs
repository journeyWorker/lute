use std::collections::BTreeSet;

use lute_check::{check, CheckInput, Mode};
use lute_compile::{compile, compile_with_check};
use lute_manifest::permissions::{PermissionSet, Permissions};
use lute_manifest::project::IdentityTemplates;

const SOURCE: &str = r#"---
kind: scene
character: x
season: 1
episode: 1
state:
  run.flag: { type: bool }
---
## Shot
::set{run.flag = true}
"#;

fn input() -> CheckInput {
    CheckInput {
        text: SOURCE.into(),
        uri: "permissions".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: Default::default(),
        mode: Mode::Ci,
        imports: Default::default(),
        components: Default::default(),
        defaults: Default::default(),
    }
}

#[test]
fn compile_with_check_rechecks_the_current_permission_policy() {
    let mut input = input();
    let checked_under_unrestricted_policy = check(&input);
    assert!(checked_under_unrestricted_policy.ok);

    input.snapshot.restrict_permissions(&Permissions {
        layers: vec![PermissionSet {
            directives: Some(BTreeSet::from(["set".to_string()])),
            state_writes: Some(BTreeSet::new()),
            ..Default::default()
        }],
    });

    let diagnostics = compile_with_check(
        &input,
        checked_under_unrestricted_policy,
        &IdentityTemplates::default(),
    )
    .expect_err("a CheckResult from a wider policy must not authorize lowering");
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == "E-PERMISSION-STATE"),
        "{diagnostics:#?}"
    );
}

#[test]
fn an_unrestricted_additional_ceiling_changes_neither_hash_nor_output() {
    let plain_input = input();
    let mut explicitly_unrestricted = input();
    let original_hash = explicitly_unrestricted.snapshot.version.clone();
    explicitly_unrestricted
        .snapshot
        .restrict_permissions(&Permissions::default());
    assert_eq!(explicitly_unrestricted.snapshot.version, original_hash);

    let plain = compile(&plain_input).expect("plain source compiles");
    let unrestricted = compile(&explicitly_unrestricted).expect("unrestricted source compiles");
    assert_eq!(
        serde_json::to_vec(&plain).unwrap(),
        serde_json::to_vec(&unrestricted).unwrap()
    );
}
