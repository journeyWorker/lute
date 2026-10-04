use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use lute_model::{apply_patch, ModelOptions, NodeKind, PatchBase, PatchEdit, PatchRequest, ProjectModel};

/// A fresh directory per fixture. Tests run in parallel, so the name carries
/// the process id and a per-process counter, not only a timestamp.
fn unique_root(prefix: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let serial = NEXT.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("{prefix}-{}-{stamp}-{serial}", std::process::id()))
}

fn project() -> PathBuf {
    let root = unique_root("lute-model-patch");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("scene.lute"), "---\nkind: scene\nid: hall\ncharacter: hero\nseason: 1\nepisode: 1\n---\n\n## Opening\n\n@hero: Hello\n").unwrap();
    root
}
fn scene_project(body: &str) -> PathBuf {
    let root = unique_root("lute-model-patch-fixture");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("scene.lute"), format!("---\nkind: scene\nid: hall\ncharacter: hero\nseason: 1\nepisode: 1\nuses: world.schema.yaml\n---\n\n{body}\n")).unwrap();
    std::fs::write(root.join("world.schema.yaml"), "state:\n  run.flag: { type: bool, default: false }\n").unwrap();
    root
}
fn quest_project(body: &str) -> PathBuf {
    let root = unique_root("lute-model-patch-quest");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("quest.lute"), format!("---\nkind: quest\nid: q\n---\n\n{body}\n")).unwrap();
    root
}
fn lore_project(body: &str) -> PathBuf {
    let root = unique_root("lute-model-patch-lore");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("lore.lute"), format!("---\nkind: lore\nid: lore\n---\n\n{body}\n")).unwrap();
    root
}

fn base(root: &Path) -> PatchBase {
    let model = ProjectModel::build_single_root(root, &ModelOptions::default()).unwrap();
    PatchBase { project: format!("sha256:{}", model.revisions().sha256), files: Default::default() }
}

fn node(root: &Path, kind: NodeKind, suffix: &str) -> lute_model::NodeKey {
    ProjectModel::build_single_root(root, &ModelOptions::default()).unwrap()
        .graph().nodes.keys().find(|key| key.kind == kind && key.key.ends_with(suffix)).cloned()
        .unwrap_or_else(|| panic!("missing {kind:?} node ending in {suffix}; keys: {:?}", ProjectModel::build_single_root(root, &ModelOptions::default()).unwrap().graph().nodes.keys().filter(|key| key.kind == kind).collect::<Vec<_>>()))
}
fn patch(root: &Path, targets: Vec<lute_model::NodeKey>, edits: Vec<PatchEdit>, preserve: Vec<lute_model::Preserve>) -> PatchRequest {
    PatchRequest { base: base(root), targets, edits, preserve }
}

fn line_info(root: &Path) -> (lute_model::NodeKey, lute_core_span::Span, String) {
    let model = ProjectModel::build_single_root(root, &ModelOptions::default()).unwrap();
    let graph = model.graph();
    let (key, node) = graph.nodes.iter().find(|(key, node)| key.kind == NodeKind::Line && node.file.is_some() && node.span.is_some()).unwrap();
    let file = node.file.as_ref().unwrap().strip_prefix(root).unwrap().to_path_buf();
    (key.clone(), node.span.unwrap(), file.to_string_lossy().into_owned())
}

fn request(root: &Path) -> PatchRequest {
    let model = ProjectModel::build_single_root(root, &ModelOptions::default()).unwrap();
    let (node, _, _) = line_info(root);
    PatchRequest {
        base: PatchBase { project: format!("sha256:{}", model.revisions().sha256), files: Default::default() },
        targets: vec![node.clone()],
        edits: vec![PatchEdit::ReplaceNode { node, text: "@hero: Goodbye".into() }],
        preserve: vec![],
    }
}

fn run_edit(root: &Path, target: lute_model::NodeKey, edit: PatchEdit, preserve: Vec<lute_model::Preserve>) -> Result<lute_model::PatchReport, lute_model::PatchRefusal> {
    let model = ProjectModel::build_single_root(root, &ModelOptions::default()).unwrap();
    apply_patch(root, PatchRequest {
        base: PatchBase { project: model.revisions().sha256.clone(), files: Default::default() },
        targets: vec![target], edits: vec![edit], preserve,
    }, true)
}

#[test]
fn dry_run_stages_diff_without_writing() {
    let root = project();
    let original = std::fs::read_to_string(root.join("scene.lute")).unwrap();
    let report = apply_patch(&root, request(&root), true).unwrap();
    assert_eq!(std::fs::read_to_string(root.join("scene.lute")).unwrap(), original);
    assert_eq!(report.writes, vec![PathBuf::from("scene.lute")]);
    assert_eq!(report.diff.changes.len(), 1);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn stale_project_revision_refuses_before_staging() {
    let root = project();
    let mut patch = request(&root);
    patch.base.project = "sha256:stale".into();
    let refusal = apply_patch(&root, patch, true).unwrap_err();
    assert_eq!(refusal.code(), "E-PATCH-STALE");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn target_and_edit_refusals_are_stable_and_atomic() {
    let root = project();
    let model = ProjectModel::build_single_root(&root, &ModelOptions::default()).unwrap();
    let unknown = PatchRequest { base: PatchBase { project: model.revisions().sha256.clone(), files: Default::default() }, targets: vec![lute_model::NodeKey::new(NodeKind::Line, "missing")], edits: vec![], preserve: vec![] };
    assert_eq!(apply_patch(&root, unknown, true).unwrap_err().code(), "E-PATCH-TARGET");
    let (line, span, file) = line_info(&root);
    let original = std::fs::read(root.join(&file)).unwrap();
    let outside = PatchRequest { base: PatchBase { project: model.revisions().sha256.clone(), files: Default::default() }, targets: vec![line.clone()], edits: vec![PatchEdit::ReplaceText { file: file.clone().into(), rev: model.revisions().files[Path::new(&file)].sha256.clone(), span: lute_core_span::Span { byte_start: 0, byte_end: 1, ..span }, text: "x".into() }], preserve: vec![] };
    assert_eq!(apply_patch(&root, outside, true).unwrap_err().code(), "E-PATCH-EDIT");
    let overlap = PatchRequest { base: PatchBase { project: model.revisions().sha256.clone(), files: Default::default() }, targets: vec![line.clone()], edits: vec![PatchEdit::ReplaceNode { node: line.clone(), text: "@hero: A".into() }, PatchEdit::ReplaceNode { node: line, text: "@hero: B".into() }], preserve: vec![] };
    assert_eq!(apply_patch(&root, overlap, true).unwrap_err().code(), "E-PATCH-EDIT");
    assert_eq!(std::fs::read(root.join(&file)).unwrap(), original);
    assert!(std::fs::read_dir(&root).unwrap().flatten().all(|e| !e.file_name().to_string_lossy().contains("lute-patch-")));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn every_node_operation_and_exact_replace_text_can_stage() {
    for operation in ["replace", "before", "after", "remove", "text"] {
        let root = project();
        let (line, span, file) = line_info(&root);
        let edit = match operation {
            "replace" => PatchEdit::ReplaceNode { node: line.clone(), text: "@hero: Goodbye".into() },
            "before" => PatchEdit::InsertBefore { node: line.clone(), text: "@hero: Before\n".into() },
            "after" => PatchEdit::InsertAfter { node: line.clone(), text: "\n@hero: After".into() },
            "remove" => PatchEdit::RemoveNode { node: line.clone() },
            _ => {
                let model = ProjectModel::build_single_root(&root, &ModelOptions::default()).unwrap();
                PatchEdit::ReplaceText { file: file.clone().into(), rev: model.revisions().files[Path::new(&file)].sha256.clone(), span, text: "@hero: Text".into() }
            }
        };
        let result = run_edit(&root, line, edit, vec![]);
        assert!(result.is_ok(), "{operation}: {:?}", result.err());
        let _ = std::fs::remove_dir_all(root);
    }
}

#[test]
fn create_and_move_preserve_identity_and_write_only_after_success() {
    let root = project();
    let model = ProjectModel::build_single_root(&root, &ModelOptions::default()).unwrap();
    let request = PatchRequest { base: PatchBase { project: model.revisions().sha256.clone(), files: Default::default() }, targets: vec![], edits: vec![PatchEdit::CreateFile { path: "new.lute".into(), text: "---\nkind: scene\nid: new\n---\n\n## New\n\n@narrator: New\n".into() }], preserve: vec![] };
    let report = apply_patch(&root, request, true).unwrap();
    assert!(report.writes.contains(&PathBuf::from("new.lute")));
    assert!(!root.join("new.lute").exists());
    let model = ProjectModel::build_single_root(&root, &ModelOptions::default()).unwrap();
    let request = PatchRequest { base: PatchBase { project: model.revisions().sha256.clone(), files: Default::default() }, targets: vec![], edits: vec![PatchEdit::MoveFile { from: "scene.lute".into(), to: "moved.lute".into() }], preserve: vec![] };
    let report = apply_patch(&root, request, true).unwrap();
    assert!(report.diff.changes.iter().any(|change| matches!(change.kind, lute_model::ChangeKind::Moved)));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn check_and_ids_preserve_refusals_are_reported() {
    let root = project();
    let (line, _, _) = line_info(&root);
    let bad = run_edit(&root, line.clone(), PatchEdit::ReplaceNode { node: line.clone(), text: "::bogus{}".into() }, vec![]).unwrap_err();
    assert_eq!(bad.code(), "E-PATCH-CHECK");
    let refused = run_edit(&root, line.clone(), PatchEdit::RemoveNode { node: line.clone() }, vec![lute_model::Preserve::Ids(vec![line])]).unwrap_err();
    assert_eq!(refused.code(), "E-PATCH-PRESERVE");
    let _ = std::fs::remove_dir_all(root);
}
#[test]
fn acceptance_refusal_matrix_has_named_cases() {
    let mut cases = Vec::new();
    let root = project();
    let mut stale = request(&root);
    stale.base.project = "sha256:stale".into();
    cases.push(("stale project base", root, stale, "E-PATCH-STALE"));

    let root = project();
    let (line, span, file) = line_info(&root);
    let stale_file = patch(&root, vec![line.clone()], vec![PatchEdit::ReplaceText {
        file: file.clone().into(), rev: "sha256:stale".into(), span, text: "x".into(),
    }], vec![]);
    cases.push(("stale file rev in replaceText", root, stale_file, "E-PATCH-EDIT"));

    let root = project();
    cases.push(("unknown target", root.clone(), patch(&root, vec![lute_model::NodeKey::new(NodeKind::Line, "missing")], vec![], vec![]), "E-PATCH-TARGET"));

    let root = scene_project("## Opening\n\n<branch id=\"b\">\n<choice id=\"go\" label=\"Go\">\n@hero: A\n</choice>\n<choice id=\"go\" label=\"Go again\">\n@hero: B\n</choice>\n</branch>");
    let ambiguous = node(&root, NodeKind::Choice, ".go");
    cases.push(("ambiguous target", root.clone(), patch(&root, vec![ambiguous], vec![], vec![]), "E-PATCH-TARGET"));

    let root = scene_project("## Opening\n\n@hero: Hello\n::set{run.flag = true}");
    std::fs::write(root.join("world.schema.yaml"), "state:\n  run.flag: { type: bool, default: false }\n").unwrap();
    let dependency = node(&root, NodeKind::State, "run.flag");
    cases.push(("dependency-only target (no source anchor)", root.clone(), patch(&root, vec![dependency], vec![], vec![]), "E-PATCH-TARGET"));

    let root = project();
    let (line, _, file) = line_info(&root);
    let model = ProjectModel::build_single_root(&root, &ModelOptions::default()).unwrap();
    let span = lute_core_span::Span { byte_start: 0, byte_end: 1, ..model.graph().nodes[&line].span.unwrap() };
    cases.push(("edit outside target", root.clone(), patch(&root, vec![line], vec![PatchEdit::ReplaceText {
        file: file.into(), rev: model.revisions().files[Path::new("scene.lute")].sha256.clone(), span, text: "x".into(),
    }], vec![]), "E-PATCH-EDIT"));

    let root = project();
    let (line, _, _) = line_info(&root);
    cases.push(("overlapping edits", root.clone(), patch(&root, vec![line.clone()], vec![
        PatchEdit::ReplaceNode { node: line.clone(), text: "@hero: A".into() },
        PatchEdit::ReplaceNode { node: line, text: "@hero: B".into() },
    ], vec![]), "E-PATCH-EDIT"));

    let root = scene_project("## Opening\n@hero: Héllo");
    let (line, span, file) = line_info(&root);
    let source = std::fs::read_to_string(root.join(&file)).unwrap();
    let at = source.find('é').unwrap();
    let bad_span = lute_core_span::Span { byte_start: at + 1, byte_end: at + 2, ..span };
    let model = ProjectModel::build_single_root(&root, &ModelOptions::default()).unwrap();
    cases.push(("non-UTF-8-boundary span", root.clone(), patch(&root, vec![line], vec![PatchEdit::ReplaceText {
        file: file.into(), rev: model.revisions().files[Path::new("scene.lute")].sha256.clone(), span: bad_span, text: "x".into(),
    }], vec![]), "E-PATCH-EDIT"));

    let root = project();
    let (line, _, _) = line_info(&root);
    cases.push(("new check error", root.clone(), patch(&root, vec![line.clone()], vec![PatchEdit::ReplaceNode {
        node: line, text: "::bogus{}".into(),
    }], vec![]), "E-PATCH-CHECK"));
    let root = project();
    std::fs::write(root.join("bad.lute"), "---\nkind: scene\nid: bad\n---\n\n## Bad\n::bogus{}\n").unwrap();
    let (line, _, _) = line_info(&root);
    cases.push(("edit only shifts a pre-existing error", root.clone(), patch(&root, vec![line.clone()], vec![PatchEdit::InsertBefore {
        node: line, text: "@hero: Added\n".into(),
    }], vec![]), "ACCEPT"));

    for (name, root, request, expected) in cases {
        let result = apply_patch(&root, request, true);
        if expected == "ACCEPT" {
            assert!(result.is_ok(), "{name}: {:?}", result.err());
        } else {
            assert_eq!(result.unwrap_err().code(), expected, "{name}");
        }
        let _ = std::fs::remove_dir_all(root);
    }
}
#[test]
fn acceptance_preserve_matrix_has_named_cases() {
    let mut cases = Vec::new();

    let root = project();
    let line = line_info(&root).0;
    cases.push(("ids (node removed)", root, line.clone(), PatchEdit::RemoveNode { node: line.clone() }, lute_model::Preserve::Ids(vec![line])));

    let root = scene_project("## Opening\n\n@hero{code=\"0010\"}: Hello");
    let line = line_info(&root).0;
    cases.push(("lineIds", root, line.clone(), PatchEdit::ReplaceNode { node: line.clone(), text: "@hero{code=\"0020\"}: Hello".into() }, lute_model::Preserve::LineIds));

    let root = scene_project("## Opening\n\n@hero{code=\"0010\"}: Hello");
    let line = line_info(&root).0;
    cases.push(("voiceKeys", root, line.clone(), PatchEdit::ReplaceNode { node: line.clone(), text: "@villain{code=\"0010\"}: Hello".into() }, lute_model::Preserve::VoiceKeys));

    let root = scene_project("## Opening\n\n<branch id=\"b\">\n<choice id=\"go\" label=\"Go\">\n::set{run.flag = true}\n</choice>\n<choice id=\"stop\" label=\"Stop\">\n@hero: Stop\n</choice>\n</branch>");
    let choice = node(&root, NodeKind::Choice, ".go");
    cases.push(("choiceEffects", root, choice.clone(), PatchEdit::ReplaceNode {
        node: choice.clone(), text: "<choice id=\"go\" label=\"Go\">\n::set{run.flag = false}\n</choice>".into(),
    }, lute_model::Preserve::ChoiceEffects(vec![choice])));

    let root = quest_project("<quest id=\"q\" start=\"true\">\n<reward kind=\"gold\" amount=\"1\"/>\n</quest>");
    let reward = node(&root, NodeKind::Reward, "#0");
    cases.push(("rewards", root, reward.clone(), PatchEdit::ReplaceNode {
        node: reward.clone(), text: "<reward kind=\"gold\" amount=\"2\"/>".into(),
    }, lute_model::Preserve::Rewards(vec![reward])));

    let root = quest_project("<quest id=\"q\" start=\"true\">\n<reward kind=\"gold\" amount=\"1\"/>\n</quest>");
    let reward = node(&root, NodeKind::Reward, "#0");
    cases.push(("rewards (identical-reward ambiguity)", root, reward.clone(), PatchEdit::InsertAfter {
        node: reward.clone(), text: "\n<reward kind=\"gold\" amount=\"1\"/>".into(),
    }, lute_model::Preserve::Rewards(vec![reward])));

    let root = lore_project("<entry id=\"e\" when=\"occasion.target == 'x'\">\n@hero: Entry\n</entry>");
    let entry = node(&root, NodeKind::Entry, "e");
    cases.push(("conditions", root, entry.clone(), PatchEdit::ReplaceAttr {
        node: entry.clone(), attr: "when".into(), value: "occasion.target == 'y'".into(),
    }, lute_model::Preserve::Conditions(vec![entry])));


    for (name, root, target, edit, preserve) in cases {
        let result = apply_patch(&root, patch(&root, vec![target.clone()], vec![edit], vec![preserve]), true);
        assert!(matches!(&result, Err(refusal) if refusal.code() == "E-PATCH-PRESERVE"), "{name} target={target:?}: {result:?}");
        let _ = std::fs::remove_dir_all(root);
    }
}
#[test]
fn atomicity_refused_multi_file_patch_preserves_bytes_and_leaves_no_temps() {
    let root = project();
    std::fs::write(root.join("other.lute"), "---\nkind: scene\nid: other\n---\n\n## Other\n\n@narrator: Other\n").unwrap();
    let model = ProjectModel::build_single_root(&root, &ModelOptions::default()).unwrap();
    let scene_line = line_info(&root).0;
    let other_line = model.graph().nodes.iter()
        .find(|(key, node)| key.kind == NodeKind::Line && node.file.as_ref().is_some_and(|path| path.ends_with("other.lute")) && node.span.is_some())
        .map(|(key, _)| key.clone()).unwrap();
    let before_scene = std::fs::read(root.join("scene.lute")).unwrap();
    let before_other = std::fs::read(root.join("other.lute")).unwrap();
    let request = patch(&root, vec![scene_line.clone(), other_line.clone()], vec![
        PatchEdit::ReplaceNode { node: scene_line, text: "@hero: Changed".into() },
        PatchEdit::ReplaceNode { node: other_line, text: "::bogus{}".into() },
    ], vec![]);
    assert_eq!(apply_patch(&root, request, false).unwrap_err().code(), "E-PATCH-CHECK");
    assert_eq!(std::fs::read(root.join("scene.lute")).unwrap(), before_scene);
    assert_eq!(std::fs::read(root.join("other.lute")).unwrap(), before_other);
    assert!(std::fs::read_dir(&root).unwrap().flatten().all(|entry| !entry.file_name().to_string_lossy().contains("lute-patch-")));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn formatting_only_touched_regions_changes_and_far_noncanonical_bytes_remain() {
    let root = scene_project("## Opening\n\n@hero: Far   \n@hero: Target");
    let before = std::fs::read_to_string(root.join("scene.lute")).unwrap();
    let far = before.lines().find(|line| line.contains("Far")).unwrap().to_string();
    let model = ProjectModel::build_single_root(&root, &ModelOptions::default()).unwrap();
    let source = std::fs::read_to_string(root.join("scene.lute")).unwrap();
    let line = model.graph().nodes.iter().find(|(key, node)| key.kind == NodeKind::Line && node.span.is_some_and(|span| source[span.byte_start..span.byte_end].contains("Target"))).map(|(key, _)| key.clone()).unwrap();
    let request = patch(&root, vec![line.clone()], vec![PatchEdit::ReplaceNode {
        node: line, text: "@hero: Changed".into(),
    }], vec![]);
    apply_patch(&root, request, false).unwrap();
    let after = std::fs::read_to_string(root.join("scene.lute")).unwrap();
    assert!(after.contains(&far), "far line was reformatted: {after:?}");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn acceptance_success_matrix_has_named_cases() {
    let cases = [
        ("replaceNode", PatchEdit::ReplaceNode { node: lute_model::NodeKey::new(NodeKind::Line, "placeholder"), text: "@hero: Goodbye".into() }),
        ("insertBefore", PatchEdit::InsertBefore { node: lute_model::NodeKey::new(NodeKind::Line, "placeholder"), text: "@hero: Before\n".into() }),
        ("insertAfter", PatchEdit::InsertAfter { node: lute_model::NodeKey::new(NodeKind::Line, "placeholder"), text: "\n@hero: After".into() }),
        ("removeNode", PatchEdit::RemoveNode { node: lute_model::NodeKey::new(NodeKind::Line, "placeholder") }),
    ];
    for (name, operation) in cases {
        let root = project();
        let (line, _, _) = line_info(&root);
        let operation = match operation {
            PatchEdit::ReplaceNode { text, .. } => PatchEdit::ReplaceNode { node: line.clone(), text },
            PatchEdit::InsertBefore { text, .. } => PatchEdit::InsertBefore { node: line.clone(), text },
            PatchEdit::InsertAfter { text, .. } => PatchEdit::InsertAfter { node: line.clone(), text },
            PatchEdit::RemoveNode { .. } => PatchEdit::RemoveNode { node: line.clone() },
            _ => unreachable!(),
        };
        assert!(apply_patch(&root, patch(&root, vec![line], vec![operation], vec![]), true).is_ok(), "{name}");
        let _ = std::fs::remove_dir_all(root);
    }

    let root = scene_project("## Opening\n\n<branch id=\"b\">\n<choice id=\"go\" label=\"Go\">\n@hero: Hello\n</choice>\n</branch>");
    let choice = node(&root, NodeKind::Choice, ".go");
    assert!(apply_patch(&root, patch(&root, vec![choice.clone()], vec![PatchEdit::ReplaceAttr {
        node: choice, attr: "label".into(), value: "Continue".into(),
    }], vec![]), true).is_ok(), "replaceAttr");
    let _ = std::fs::remove_dir_all(root);

    let root = project();
    assert!(apply_patch(&root, patch(&root, vec![], vec![PatchEdit::CreateFile {
        path: "new.lute".into(), text: "---\nkind: scene\nid: new\n---\n\n## New\n\n@narrator: New\n".into(),
    }], vec![]), true).is_ok(), "createFile");
    assert!(!root.join("new.lute").exists(), "dry-run createFile wrote source");
    let _ = std::fs::remove_dir_all(root);

    let root = project();
    let report = apply_patch(&root, patch(&root, vec![], vec![PatchEdit::MoveFile {
        from: "scene.lute".into(), to: "moved.lute".into(),
    }], vec![]), true).unwrap();
    assert!(report.diff.changes.iter().any(|change| matches!(change.kind, lute_model::ChangeKind::Moved)), "moveFile must report moved");
    let _ = std::fs::remove_dir_all(root);

    let root = project();
    let (line, span, file) = line_info(&root);
    let model = ProjectModel::build_single_root(&root, &ModelOptions::default()).unwrap();
    let rev = model.revisions().files[Path::new(&file)].sha256.clone();
    assert!(apply_patch(&root, patch(&root, vec![line], vec![PatchEdit::ReplaceText {
        file: file.into(), rev, span, text: "@hero: Text".into(),
    }], vec![]), true).is_ok(), "replaceText");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn move_file_with_stale_reference_is_check_refusal() {
    let root = project();
    std::fs::write(root.join("refs.lute"), "---\nkind: scene\nid: refs\n---\n\n::use{file=\"scene.lute\"}\n").unwrap();
    let result = apply_patch(&root, patch(&root, vec![], vec![PatchEdit::MoveFile {
        from: "scene.lute".into(), to: "moved.lute".into(),
    }], vec![]), true);
    assert_eq!(result.unwrap_err().code(), "E-PATCH-CHECK");
    assert!(root.join("scene.lute").exists() && !root.join("moved.lute").exists());
    let _ = std::fs::remove_dir_all(root);
}

fn copy_tree_for_patch_test(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for entry in std::fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_tree_for_patch_test(&from, &to);
        } else {
            std::fs::copy(from, to).unwrap();
        }
    }
}

fn plugin_scene_project() -> PathBuf {
    let root = unique_root("lute-model-patch-plugin");
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../conformance/edit-tasks/06-host-result/base");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::copy(fixture.join("lute.project.yaml"), root.join("lute.project.yaml")).unwrap();
    copy_tree_for_patch_test(&fixture.join("plugins"), &root.join("plugins"));
    copy_tree_for_patch_test(&fixture.join("schema"), &root.join("schema"));
    std::fs::create_dir_all(root.join("scenes/spine")).unwrap();
    std::fs::copy(fixture.join("scenes/spine/elite-morwen.lute"), root.join("scenes/spine/elite-morwen.lute")).unwrap();
    root
}

fn shot_text(root: &Path) -> (lute_model::NodeKey, String) {
    let model = ProjectModel::build_single_root(root, &ModelOptions::default()).unwrap();
    let graph = model.graph();
    let (key, node) = graph.nodes.iter()
        .find(|(key, _node)| key.kind == NodeKind::Shot && key.key == "spine.elite.morwen:The chamber")
        .unwrap();
    let source = std::fs::read_to_string(node.file.as_ref().unwrap()).unwrap();
    (key.clone(), source[node.span.unwrap().byte_start..node.span.unwrap().byte_end].to_string())
}

#[test]
fn replace_node_noop_preserves_plugin_context_and_bytes() {
    let root = plugin_scene_project();
    let (shot, text) = shot_text(&root);
    let report = apply_patch(&root, patch(&root, vec![shot.clone()], vec![PatchEdit::ReplaceNode { node: shot, text }], vec![]), true).unwrap();
    assert!(report.diff.changes.is_empty(), "no-op replacement changed semantics: {:?}", report.diff.changes);
    assert!(report.writes.is_empty(), "no-op replacement changed source: {:?}", report.writes);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn replace_node_adds_match_arm_with_plugin_directives() {
    let root = plugin_scene_project();
    let (shot, text) = shot_text(&root);
    let replacement = text.replacen(
        "    ::assert{defeated(morwen)}\n  </when>",
        "    ::assert{defeated(morwen)}\n    <match on=\"scene.battle.fight.fainted\">\n      <when is=\"1..\">\n        @narrator: You won, but victory came at a cost.\n      </when>\n      <otherwise>\n        // no fainted cost was reported\n      </otherwise>\n    </match>\n  </when>",
        1,
    );
    assert_ne!(replacement, text);
    let report = apply_patch(&root, patch(&root, vec![shot.clone()], vec![PatchEdit::ReplaceNode { node: shot, text: replacement }], vec![]), true).unwrap();
    assert!(!report.diff.changes.is_empty(), "new match arm was lost from semantic diff");
    let _ = std::fs::remove_dir_all(root);
}
