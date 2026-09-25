use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use lute_check::{check, CheckInput, ComponentDef, ComponentSet, Mode};
use lute_manifest::permissions::{PermissionSet, Permissions};
use lute_manifest::schema::{
    AttrDecl, BridgeRef, DirectiveDecl, DirectiveEffects, DirectiveState, Lowering, SlotDecl,
    StateShape, WriteDecl, WriteValue,
};
use lute_manifest::types::{Field, FromAttr, Literal, PathSegment, Type};

fn input(text: &str, permissions: PermissionSet) -> CheckInput {
    let mut snapshot = lute_manifest::core::load_core_snapshot();
    snapshot.restrict_permissions(&Permissions {
        layers: vec![permissions],
    });
    CheckInput {
        text: text.to_string(),
        uri: "permissions".into(),
        snapshot,
        providers: Default::default(),
        mode: Mode::Ci,
        imports: Default::default(),
        components: Default::default(),
        defaults: Default::default(),
    }
}

fn set(values: &[&str]) -> Option<BTreeSet<String>> {
    Some(values.iter().map(|value| (*value).to_string()).collect())
}

fn permission_diagnostics(input: &CheckInput) -> Vec<lute_core_span::Diagnostic> {
    check(input)
        .diagnostics
        .into_iter()
        .filter(|diag| diag.code.starts_with("E-PERMISSION-"))
        .collect()
}

#[test]
fn allowed_direct_and_implicit_writes_stay_clean() {
    let source = r#"---
kind: scene
character: x
season: 1
episode: 1
state:
  scene.allowed: { type: bool }
---
## Shot
::set{scene.allowed = true}
<branch id="menu">
  <choice id="yes" label="Yes">
    @narrator: yes
  </choice>
  <choice id="no" label="No">
    @narrator: no
  </choice>
</branch>
"#;
    let permissions = PermissionSet {
        directives: set(&["set"]),
        state_writes: set(&["scene.allowed", "scene.choices.*"]),
        ..Default::default()
    };

    assert!(
        permission_diagnostics(&input(source, permissions)).is_empty(),
        "the exact direct path and descendant choice path are allowed"
    );
}

#[test]
fn direct_choice_default_seed_and_implicit_hub_writes_are_all_enforced() {
    let source = r#"---
kind: scene
character: x
season: 1
episode: 1
state:
  scene.defaulted: { type: bool, default: false }
  run.direct: { type: bool }
  run.choice: { type: bool }
entities:
  actor: { members: [ana] }
relations:
  memory: { args: [actor] }
facts:
  - "memory(ana)"
---
## Shot
::set{run.direct = true}
::assert{ memory(ana) }
<hub id="menu">
  <choice id="yes" label="Yes" into="run.choice" exit>
    @narrator: yes
  </choice>
</hub>
"#;
    let permissions = PermissionSet {
        directives: set(&["set", "assert"]),
        state_writes: set(&[]),
        fact_writes: set(&[]),
        ..Default::default()
    };

    let diagnostics = permission_diagnostics(&input(source, permissions));
    let messages: Vec<&str> = diagnostics.iter().map(|diag| diag.message.as_str()).collect();
    assert!(messages.iter().any(|message| message.contains("state default")));
    assert!(messages.iter().any(|message| message.contains("`::set`")));
    assert!(messages.iter().any(|message| message.contains("choice `into`")));
    assert!(messages.iter().any(|message| message.contains("hub selection")));
    assert!(messages.iter().any(|message| message.contains("hub visit")));
    assert!(messages.iter().any(|message| message.contains("seed fact")));
    assert!(messages.iter().any(|message| message.contains("`::assert`")));
    assert!(diagnostics.iter().all(|diag| diag.span.byte_end >= diag.span.byte_start));
}

#[test]
fn plugin_effect_shape_default_bridge_and_directive_are_independent_gates() {
    let mut permissions = PermissionSet {
        directives: set(&["remote"]),
        state_writes: set(&[]),
        bridges: set(&[]),
        ..Default::default()
    };
    let mut input = input(
        r#"---
kind: scene
character: x
season: 1
episode: 1
---
## Shot
::remote{key="slot"}
::bg{location="room" time="day"}
"#,
        permissions.clone(),
    );

    input.snapshot.state_shapes.insert(
        "remoteResult".into(),
        StateShape {
            name: "remoteResult".into(),
            fields: vec![Field {
                name: "ready".into(),
                ty: Type::Bool,
                default: Some(Literal::Bool(false)),
                required: false,
                shape: None,
            }],
        },
    );
    input.snapshot.directives.insert(
        "remote".into(),
        DirectiveDecl {
            name: "remote".into(),
            layer: Some("bridge".into()),
            attrs: vec![AttrDecl {
                name: "key".into(),
                required: true,
                ty: Type::Str,
                default: None,
            }],
            semantics: Vec::new(),
            state: Some(DirectiveState {
                declares: vec![SlotDecl {
                    scope: "scene".into(),
                    path: vec![
                        PathSegment::Literal("remote".into()),
                        PathSegment::FromAttr {
                            from_attr: FromAttr {
                                name: "key".into(),
                                slot_type: None,
                            },
                        },
                    ],
                    shape: "remoteResult".into(),
                }],
            }),
            effects: Some(DirectiveEffects {
                writes: vec![WriteDecl {
                    scope: "run".into(),
                    path: vec![PathSegment::Literal("remoteCalls".into())],
                    value: WriteValue::Op {
                        op: "increment".into(),
                        by: 1.0,
                    },
                }],
            }),
            bridge: Some(BridgeRef {
                service: "remote".into(),
                operation: "invoke".into(),
            }),
            lower: Lowering::Builtin {
                kind: "builtin".into(),
                name: "remote".into(),
            },
        },
    );
    // Reapply the same policy to restamp the augmented capability surface.
    permissions.directives = set(&["remote"]);
    input.snapshot.permissions = Permissions::default();
    input.snapshot.restrict_permissions(&Permissions {
        layers: vec![permissions],
    });

    let diagnostics = permission_diagnostics(&input);
    assert!(diagnostics.iter().any(|diag| {
        diag.code == "E-PERMISSION-DIRECTIVE" && diag.message.contains("::bg")
    }));
    assert!(diagnostics.iter().any(|diag| {
        diag.code == "E-PERMISSION-STATE" && diag.message.contains("effect")
    }));
    assert!(diagnostics.iter().any(|diag| {
        diag.code == "E-PERMISSION-STATE" && diag.message.contains("state default")
    }));
    assert!(diagnostics
        .iter()
        .any(|diag| diag.code == "E-PERMISSION-BRIDGE"));
}

#[test]
fn quest_and_every_reward_declaration_are_gated_even_when_guards_are_dead() {
    let source = r#"---
kind: quest
---
<quest id="q" start="true">
  <reward kind="gold" when="false"/>
  <objective id="o" done="true">
    <reward kind="xp" when="false"/>
  </objective>
</quest>
"#;
    let permissions = PermissionSet {
        rewards: Some(false),
        quests: Some(false),
        ..Default::default()
    };

    let diagnostics = permission_diagnostics(&input(source, permissions));
    assert_eq!(
        diagnostics
            .iter()
            .filter(|diag| diag.code == "E-PERMISSION-QUEST")
            .count(),
        1
    );
    assert_eq!(
        diagnostics
            .iter()
            .filter(|diag| diag.code == "E-PERMISSION-REWARD")
            .count(),
        2
    );
}

#[test]
fn only_invoked_component_bodies_inherit_the_callers_permissions() {
    let component_source = r#"---
component: sound
params: {}
---
## Body
::sfx{sound="bell"}
"#;
    let (component, parse_diagnostics) = lute_syntax::parse(component_source);
    assert!(parse_diagnostics.is_empty(), "{parse_diagnostics:#?}");
    let components = ComponentSet {
        table: BTreeMap::from([(
            "sound".into(),
            ComponentDef {
                params: Vec::new(),
                speakers: Vec::new(),
                effects: false,
                body: component,
                src: PathBuf::from("sound.component.lute"),
            },
        )]),
        diags: Vec::new(),
    };
    let permissions = PermissionSet {
        directives: set(&["use"]),
        ..Default::default()
    };

    let mut unused = input(
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot\n@narrator: quiet\n",
        permissions.clone(),
    );
    unused.components = components.clone();
    assert!(permission_diagnostics(&unused).is_empty());

    let mut invoked = input(
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot\n::use{component=\"sound\"}\n",
        permissions,
    );
    invoked.components = components;
    let diagnostics = permission_diagnostics(&invoked);
    let denied = diagnostics
        .iter()
        .find(|diag| diag.code == "E-PERMISSION-DIRECTIVE")
        .expect("component ::sfx is denied");
    assert!(denied.message.contains("component `sound`"));
    assert_eq!(denied.related.len(), 1);
    assert_eq!(denied.related[0].file, "sound.component.lute");
    let invocation_start = invoked.text.find("::use").unwrap();
    assert_eq!(denied.span.byte_start, invocation_start);
}
