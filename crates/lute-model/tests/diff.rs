use std::path::{Path, PathBuf};

use lute_model::{diff_models, project_revision, ModelOptions, ProjectModel};

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lute-s2-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn scene(text: &str) -> String {
    format!(
        "---\nkind: scene\nid: hall\ncharacter: hero\nseason: 1\nepisode: 1\n---\n\n## Opening\n\n@hero: {text}\n"
    )
}

fn model(dir: &Path) -> ProjectModel {
    ProjectModel::build_single_root(dir, &ModelOptions::default()).unwrap()
}

#[test]
fn revisions_are_length_delimited_and_traversal_order_independent() {
    let root = Path::new("/tmp/lute-s2-revision");
    let a = project_revision(
        root,
        &[(PathBuf::from("z.lute"), b"z".to_vec()), (PathBuf::from("a.lute"), b"a".to_vec())],
    )
    .unwrap();
    let b = project_revision(
        root,
        &[(PathBuf::from("a.lute"), b"a".to_vec()), (PathBuf::from("z.lute"), b"z".to_vec())],
    )
    .unwrap();
    assert_eq!(a, b);
    let mut changed = b"z".to_vec();
    changed.push(b'!');
    let c = project_revision(root, &[(PathBuf::from("a.lute"), b"a".to_vec()), (PathBuf::from("z.lute"), changed)]).unwrap();
    assert_ne!(a.sha256, c.sha256);
    assert_ne!(a.files[Path::new("z.lute")].sha256, c.files[Path::new("z.lute")].sha256);
}

#[test]
fn revisions_include_external_import_bytes() {
    let root = temp_dir("external-revision-root");
    let shared = temp_dir("external-revision-shared");
    std::fs::write(shared.join("world.schema.yaml"), "state:\n  run.flag: { type: bool, default: false }\n").unwrap();
    std::fs::write(
        root.join("scene.lute"),
        "---\nkind: scene\nid: hall\ncharacter: hero\nseason: 1\nepisode: 1\nuses: ../lute-s2-external-revision-shared-".to_string()
            + &std::process::id().to_string()
            + "/world.schema.yaml\n---\n\n## Opening\n\n@hero{when=\"run.flag == true\"}: Hello\n",
    )
    .unwrap();
    let first = model(&root);
    let key = first
        .revisions()
        .files
        .keys()
        .find(|path| path.to_string_lossy().contains("external-revision-shared"))
        .cloned()
        .expect("external schema revision");
    std::fs::write(shared.join("world.schema.yaml"), "state:\n  run.flag: { type: bool, default: true }\n").unwrap();
    let second = model(&root);
    assert_ne!(first.revisions().sha256, second.revisions().sha256);
    assert_ne!(first.revisions().files[&key].sha256, second.revisions().files[&key].sha256);
    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_dir_all(shared);
}

#[test]
fn trailing_whitespace_and_final_newline_are_semantically_empty() {
    let before = temp_dir("trivia-before");
    let after = temp_dir("trivia-after");
    std::fs::write(before.join("scene.lute"), scene("Hello")).unwrap();
    std::fs::write(after.join("scene.lute"), scene("Hello  \n")).unwrap();
    let diff = diff_models(&model(&before), &model(&after)).unwrap();
    assert!(diff.changes.is_empty(), "unexpected changes: {:?}", diff.changes);
    let _ = std::fs::remove_dir_all(before);
    let _ = std::fs::remove_dir_all(after);
}

#[test]
fn line_text_change_is_one_line_change_and_is_deterministic() {
    let before = temp_dir("line-before");
    let after = temp_dir("line-after");
    std::fs::write(before.join("scene.lute"), scene("Hello")).unwrap();
    std::fs::write(after.join("scene.lute"), scene("Goodbye")).unwrap();
    let b = model(&before);

    let a = model(&after);
    let first = diff_models(&b, &a).unwrap();
    let second = diff_models(&b, &a).unwrap();
    assert_eq!(first.changes, second.changes);
    assert_eq!(first.changes.len(), 1, "unexpected changes: {:?}", first.changes);
    assert!(matches!(&first.changes[0].kind, lute_model::ChangeKind::Field(name) if name == "lineText"));
    assert_eq!(first.changes[0].locations.len(), 2);
    let _ = std::fs::remove_dir_all(before);
    let _ = std::fs::remove_dir_all(after);
}
 
#[test]
fn cel_whitespace_and_parentheses_are_semantically_empty() {
    let before = temp_dir("cel-before");
    let after = temp_dir("cel-after");
    let prefix = "---\nkind: scene\nid: hall\ncharacter: hero\nseason: 1\nepisode: 1\nuses: world.schema.yaml\n---\n\n## Opening\n\n";
    std::fs::write(before.join("world.schema.yaml"), "state:\n  run.flag: { type: bool, default: false }\n").unwrap();
    std::fs::write(after.join("world.schema.yaml"), "state:\n  run.flag: { type: bool, default: false }\n").unwrap();
    std::fs::write(before.join("scene.lute"), format!("{prefix}@hero{{when=\"run.flag == true\"}}: Hello\n")).unwrap();
    std::fs::write(after.join("scene.lute"), format!("{prefix}@hero{{when=\"( run.flag == true )\"}}: Hello\n")).unwrap();
    let diff = diff_models(&model(&before), &model(&after)).unwrap();
    assert!(diff.changes.is_empty(), "unexpected changes: {:?}", diff.changes);
    let _ = std::fs::remove_dir_all(before);
    let _ = std::fs::remove_dir_all(after);
}

#[test]
fn unparsable_condition_is_reported_without_text_fallback() {
    let before = temp_dir("bad-cel-before");
    let after = temp_dir("bad-cel-after");
    let prefix = "---\nkind: scene\nid: hall\ncharacter: hero\nseason: 1\nepisode: 1\nuses: world.schema.yaml\n---\n\n## Opening\n\n";
    let schema = "state:\n  run.flag: { type: bool, default: false }\n";
    std::fs::write(before.join("world.schema.yaml"), schema).unwrap();
    std::fs::write(after.join("world.schema.yaml"), schema).unwrap();
    std::fs::write(before.join("scene.lute"), format!("{prefix}@hero{{when=\"run.flag == true\"}}: Hello\n")).unwrap();
    std::fs::write(after.join("scene.lute"), format!("{prefix}@hero{{when=\"run.flag ==\"}}: Hello\n")).unwrap();
    let diff = diff_models(&model(&before), &model(&after)).unwrap();
    assert!(diff.changes.iter().any(|change| matches!(change.kind, lute_model::ChangeKind::ConditionUnparsable)), "unexpected changes: {:?}", diff.changes);
    let _ = std::fs::remove_dir_all(before);
    let _ = std::fs::remove_dir_all(after);
}

#[test]
fn path_move_with_same_identity_is_moved() {
    let before = temp_dir("move-before");
    let after = temp_dir("move-after");
    std::fs::write(before.join("scene.lute"), scene("Hello")).unwrap();
    std::fs::create_dir_all(after.join("nested")).unwrap();
    std::fs::write(after.join("nested/scene.lute"), scene("Hello")).unwrap();
    let diff = diff_models(&model(&before), &model(&after)).unwrap();
    assert!(diff.changes.iter().any(|change| matches!(change.kind, lute_model::ChangeKind::Moved)));
    assert!(!diff.changes.iter().any(|change| matches!(change.kind, lute_model::ChangeKind::Added | lute_model::ChangeKind::Removed)));
    let _ = std::fs::remove_dir_all(before);
    let _ = std::fs::remove_dir_all(after);
}

#[test]
fn guard_change_is_one_guard_change() {
    let before = temp_dir("guard-before");
    let after = temp_dir("guard-after");
    let prefix = "---\nkind: scene\nid: hall\ncharacter: hero\nseason: 1\nepisode: 1\nuses: world.schema.yaml\n---\n\n## Opening\n\n";
    let schema = "state:\n  run.flag: { type: bool, default: false }\n";
    std::fs::write(before.join("world.schema.yaml"), schema).unwrap();
    std::fs::write(after.join("world.schema.yaml"), schema).unwrap();
    std::fs::write(before.join("scene.lute"), format!("{prefix}@hero{{when=\"run.flag == true\"}}: Hello\n")).unwrap();
    std::fs::write(after.join("scene.lute"), format!("{prefix}@hero{{when=\"run.flag == false\"}}: Hello\n")).unwrap();
    let diff = diff_models(&model(&before), &model(&after)).unwrap();
    assert_one(&diff, "guard", "line:hall.hero_0010");
    cleanup(before, after);
}

#[test]
fn speaker_and_line_id_changes_are_removed_and_added() {
    let before = temp_dir("identity-before");
    let after = temp_dir("identity-after");
    let prefix = "---\nkind: scene\nid: hall\ncharacter: hero\nseason: 1\nepisode: 1\n---\n\n## Opening\n\n";
    std::fs::write(before.join("scene.lute"), format!("{prefix}@hero{{code=\"0010\"}}: Hello\n")).unwrap();
    std::fs::write(after.join("scene.lute"), format!("{prefix}@villain{{code=\"0020\"}}: Hello\n")).unwrap();
    let diff = diff_models(&model(&before), &model(&after)).unwrap();
    assert_eq!(diff.changes.iter().filter(|c| matches!(c.kind, lute_model::ChangeKind::Removed)).count(), 1);
    assert_eq!(diff.changes.iter().filter(|c| matches!(c.kind, lute_model::ChangeKind::Added)).count(), 1);
    cleanup(before, after);
}

#[test]
fn authored_line_id_change_is_removed_and_added() {
    let before = temp_dir("line-id-before");
    let after = temp_dir("line-id-after");
    let prefix = "---\nkind: scene\nid: hall\ncharacter: hero\nseason: 1\nepisode: 1\n---\n\n## Opening\n\n";
    std::fs::write(before.join("scene.lute"), format!("{prefix}@hero{{code=\"0010\"}}: Hello\n")).unwrap();
    std::fs::write(after.join("scene.lute"), format!("{prefix}@hero{{code=\"0020\"}}: Hello\n")).unwrap();
    let diff = diff_models(&model(&before), &model(&after)).unwrap();
    assert!(diff.changes.iter().any(|c| matches!(c.kind, lute_model::ChangeKind::Removed) && c.node.canonical() == "line:hall.hero_0010"));
    assert!(diff.changes.iter().any(|c| matches!(c.kind, lute_model::ChangeKind::Added) && c.node.canonical() == "line:hall.hero_0020"));
    cleanup(before, after);
}

fn assert_one(diff: &lute_model::SemanticDiff, kind: &str, node: &str) {
    assert_eq!(diff.changes.len(), 1, "unexpected changes: {:?}", diff.changes);
    assert!(matches!(&diff.changes[0].kind, lute_model::ChangeKind::Field(name) if name == kind));
    assert_eq!(diff.changes[0].node.canonical(), node);
    assert_eq!(diff.changes[0].locations.len(), 2);
}

fn cleanup(before: PathBuf, after: PathBuf) {
    let _ = std::fs::remove_dir_all(before);
    let _ = std::fs::remove_dir_all(after);
}

#[test]
fn choice_effect_change_is_one_choice_effect_change() {
    let before = temp_dir("choice-before");
    let after = temp_dir("choice-after");
    let prefix = "---\nkind: scene\nid: hall\ncharacter: hero\nseason: 1\nepisode: 1\nuses: world.schema.yaml\n---\n\n## Opening\n\n<branch id=\"pick\">\n<choice id=\"go\" label=\"Go\">\n";
    let schema = "state:\n  run.flag: { type: bool, default: false }\n";
    std::fs::write(before.join("world.schema.yaml"), schema).unwrap();
    std::fs::write(after.join("world.schema.yaml"), schema).unwrap();
    std::fs::write(before.join("scene.lute"), format!("{prefix}::set{{ run.flag = true }}\n</choice>\n</branch>\n")).unwrap();
    std::fs::write(after.join("scene.lute"), format!("{prefix}::set{{ run.flag = false }}\n</choice>\n</branch>\n")).unwrap();
    let diff = diff_models(&model(&before), &model(&after)).unwrap();
    assert_eq!(diff.changes.iter().filter(|change| matches!(&change.kind, lute_model::ChangeKind::Field(name) if name == "choiceEffects")).count(), 1, "unexpected changes: {:?}", diff.changes);
    cleanup(before, after);
}

#[test]
fn objective_schedule_change_is_one_scheduling_change() {
    let before = temp_dir("schedule-before");
    let after = temp_dir("schedule-after");
    let prefix = "---\nkind: quest\nid: q\nstate:\n  run.done: { type: bool, default: false }\n---\n\n<quest id=\"q\" start=\"true\">\n";
    std::fs::write(before.join("quest.lute"), format!("{prefix}<objective id=\"o\" done=\"run.done\" by=\"run.done\"/>\n</quest>\n")).unwrap();
    std::fs::write(after.join("quest.lute"), format!("{prefix}<objective id=\"o\" done=\"run.done\" by=\"!run.done\"/>\n</quest>\n")).unwrap();
    let diff = diff_models(&model(&before), &model(&after)).unwrap();
    assert_eq!(diff.changes.iter().filter(|change| matches!(&change.kind, lute_model::ChangeKind::Field(name) if name == "scheduling")).count(), 1, "unexpected changes: {:?}", diff.changes);
    cleanup(before, after);
}

#[test]
fn reward_change_is_one_reward_change() {
    let before = temp_dir("reward-before");
    let after = temp_dir("reward-after");
    let prefix = "---\nkind: quest\nid: q\n---\n\n<quest id=\"q\" start=\"true\">\n";
    std::fs::write(before.join("quest.lute"), format!("{prefix}<reward kind=\"gold\" amount=\"1\"/>\n</quest>\n")).unwrap();
    std::fs::write(after.join("quest.lute"), format!("{prefix}<reward kind=\"gold\" amount=\"2\"/>\n</quest>\n")).unwrap();
    let diff = diff_models(&model(&before), &model(&after)).unwrap();
    assert_eq!(diff.changes.len(), 1, "unexpected changes: {:?}", diff.changes);
    assert!(matches!(&diff.changes[0].kind, lute_model::ChangeKind::Field(name) if name == "reward"));
    assert_eq!(diff.changes[0].node.kind, lute_model::NodeKind::Reward);
    assert_eq!(diff.changes[0].locations.len(), 2);
    cleanup(before, after);
}

#[test]
fn identical_reward_insert_reports_ambiguity_instead_of_guessing() {
    let before = temp_dir("reward-ambiguous-before");
    let after = temp_dir("reward-ambiguous-after");
    let prefix = "---\nkind: quest\nid: q\n---\n\n<quest id=\"q\" start=\"true\">\n";
    std::fs::write(before.join("quest.lute"), format!("{prefix}<reward kind=\"gold\" amount=\"1\"/>\n</quest>\n")).unwrap();
    std::fs::write(after.join("quest.lute"), format!("{prefix}<reward kind=\"gold\" amount=\"1\"/>\n<reward kind=\"gold\" amount=\"1\"/>\n</quest>\n")).unwrap();
    let diff = diff_models(&model(&before), &model(&after)).unwrap();
    assert_eq!(diff.changes.len(), 1, "unexpected changes: {:?}", diff.changes);
    assert!(matches!(&diff.changes[0].kind, lute_model::ChangeKind::Field(name) if name == "rewardAmbiguous"), "{:?}", diff.changes);
    cleanup(before, after);
}

#[test]
fn inserting_choice_before_existing_choice_does_not_change_existing_node() {
    let before = temp_dir("choice-insert-before");
    let after = temp_dir("choice-insert-after");
    let prefix = "---\nkind: scene\nid: hall\ncharacter: hero\nseason: 1\nepisode: 1\n---\n\n## Opening\n\n<branch id=\"pick\">\n";
    let suffix = "</branch>\n";
    std::fs::write(before.join("scene.lute"), format!("{prefix}<choice id=\"b\" label=\"B\">\n@hero{{code=\"0010\"}}: B\n</choice>\n{suffix}")).unwrap();
    std::fs::write(after.join("scene.lute"), format!("{prefix}<choice id=\"a\" label=\"A\">\n@hero{{code=\"0005\"}}: A\n</choice>\n<choice id=\"b\" label=\"B\">\n@hero{{code=\"0010\"}}: B\n</choice>\n{suffix}")).unwrap();
    let diff = diff_models(&model(&before), &model(&after)).unwrap();
    assert!(diff.changes.iter().all(|change| change.node.canonical() != "choice:hall:pick.b"), "{:?}", diff.changes);
    assert!(diff.changes.iter().any(|change| matches!(change.kind, lute_model::ChangeKind::Added)));
    cleanup(before, after);
}

#[test]
fn revisions_include_manifest_schema_and_plugin_bytes() {
    let root = Path::new("/tmp/lute-s2-inputs");
    let input = |manifest: &[u8], schema: &[u8], plugin: &[u8]| {
        project_revision(root, &[
            (PathBuf::from("lute.project.yaml"), manifest.to_vec()),
            (PathBuf::from("world.schema.yaml"), schema.to_vec()),
            (PathBuf::from("plugins/demo/plugin.yaml"), plugin.to_vec()),
        ]).unwrap()
    };
    let base = input(b"manifest: a", b"schema: a", b"plugin: a");
    assert_ne!(base.sha256, input(b"manifest: b", b"schema: a", b"plugin: a").sha256);
    assert_ne!(base.sha256, input(b"manifest: a", b"schema: b", b"plugin: a").sha256);
    assert_ne!(base.sha256, input(b"manifest: a", b"schema: a", b"plugin: b").sha256);
}

#[test]
fn document_id_change_is_removed_and_added() {
    let before = temp_dir("id-before");
    let after = temp_dir("id-after");
    std::fs::write(before.join("scene.lute"), scene("Hello")).unwrap();
    std::fs::write(after.join("scene.lute"), scene("Hello").replace("id: hall", "id: other")).unwrap();
    let diff = diff_models(&model(&before), &model(&after)).unwrap();
    assert!(diff.changes.iter().any(|change| matches!(change.kind, lute_model::ChangeKind::Removed)));
    assert!(diff.changes.iter().any(|change| matches!(change.kind, lute_model::ChangeKind::Added)));
    cleanup(before, after);
}

#[test]
fn identical_project_copies_with_schema_inputs_have_no_moves() {
    let before = temp_dir("copy-before");
    let after = temp_dir("copy-after");
    let schema = "state:\n  run.flag: { type: bool, default: false }\ndefs:\n  helped: { cel: \"run.flag\", type: bool }\n";
    let text = "---\nkind: scene\nid: hall\ncharacter: hero\nseason: 1\nepisode: 1\nuses: world.schema.yaml\n---\n\n## Opening\n\n@hero: Hello\n";
    std::fs::write(before.join("world.schema.yaml"), schema).unwrap();
    std::fs::write(after.join("world.schema.yaml"), schema).unwrap();
    std::fs::write(before.join("scene.lute"), text).unwrap();
    std::fs::write(after.join("scene.lute"), text).unwrap();
    let diff = diff_models(&model(&before), &model(&after)).unwrap();
    assert!(diff.changes.is_empty(), "unexpected changes: {:?}", diff.changes);
    cleanup(before, after);
}

#[test]
fn moved_schema_file_with_same_identity_is_moved() {
    let before = temp_dir("schema-move-before");
    let after = temp_dir("schema-move-after");
    let schema = "state:\n  run.flag: { type: bool, default: false }\ndefs:\n  helped: { cel: \"run.flag\", type: bool }\n";
    let scene_before = "---\nkind: scene\nid: hall\ncharacter: hero\nseason: 1\nepisode: 1\nuses: world.schema.yaml\n---\n\n## Opening\n\n@hero: Hello\n";
    let scene_after = scene_before.replace("uses: world.schema.yaml", "uses: nested/world.schema.yaml");
    std::fs::write(before.join("world.schema.yaml"), schema).unwrap();
    std::fs::create_dir_all(after.join("nested")).unwrap();
    std::fs::write(after.join("nested/world.schema.yaml"), schema).unwrap();
    std::fs::write(before.join("scene.lute"), scene_before).unwrap();
    std::fs::write(after.join("scene.lute"), scene_after).unwrap();
    let diff = diff_models(&model(&before), &model(&after)).unwrap();
    assert!(diff.changes.iter().any(|change| matches!(change.kind, lute_model::ChangeKind::Moved)));
    cleanup(before, after);
}
