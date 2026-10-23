use lute_load::{assemble_input, InputCache};
use lute_model::{ModelOptions, ProjectModel};
use lute_semantic::{IdentityMetadata, NodeKind};
use std::path::PathBuf;

fn temp_project(manifest: &str) -> (lute_scratch::ScratchDir, PathBuf) {
    let dir = lute_scratch::ScratchDir::named("identity-gate");
    std::fs::write(dir.join("lute.project.yaml"), manifest).unwrap();
    let file = dir.join("scene.lute");
    std::fs::write(
        &file,
        "---\nkind: scene\ncharacter: n\nseason: 1\nepisode: 1\n---\n## Shot 1.\n@n: line\n",
    )
    .unwrap();
    (dir, file)
}

fn stable_for(manifest: Option<&str>) -> bool {
    let (dir, file) = match manifest {
        Some(manifest) => temp_project(manifest),
        None => {
            let dir = lute_scratch::ScratchDir::named("identity-loose");
            let file = dir.join("scene.lute");
            std::fs::write(&file, "---\nkind: scene\n---\n## Shot 1.\n@n: line\n").unwrap();
            (dir, file)
        }
    };
    let (built, _) = assemble_input(
        &InputCache::default(),
        &file,
        std::fs::read_to_string(&file).unwrap(),
        None,
        manifest.map(|_| &*dir),
        None,
    );
    built.input.snapshot().identity_require_stable
}

#[test]
fn stable_identity_policy_follows_project_manifest_and_defaults_off() {
    assert!(stable_for(Some(
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\nidentity:\n  requireStable: true\n"
    )));
    assert!(!stable_for(Some(
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\nidentity:\n  requireStable: false\n"
    )));
    assert!(!stable_for(None));
}

fn graph_line_identity(code: Option<&str>) -> IdentityMetadata {
    let dir = lute_scratch::ScratchDir::named("graph-identity");
    let line = match code {
        Some(code) => format!("@n{{code=\"{code}\"}}: line\n"),
        None => "@n: line\n".to_string(),
    };
    std::fs::write(
        dir.join("scene.lute"),
        format!(
            "---\nkind: scene\nid: hall\ncharacter: n\nseason: 1\nepisode: 1\n---\n## Opening\n{line}"
        ),
    )
    .unwrap();
    let model = ProjectModel::build_single_root(&dir, &ModelOptions::default()).unwrap();
    model
        .graph()
        .nodes
        .values()
        .find(|node| node.id.kind == NodeKind::Line)
        .map(|node| node.identity.clone())
        .expect("scene line graph node")
}

#[test]
fn graph_marks_compiler_allocated_host_line_identity_as_unstable() {
    assert!(!graph_line_identity(None).stable);
    assert!(graph_line_identity(Some("0010")).stable);
}
