//! dsl 0.26.0 §2.3: a kind assembled across schema imports (`add:`), and a
//! sub-kind's members implied in its parent.
use std::path::{Path, PathBuf};

use lute_manifest::relations::KindShape;

fn unique_dir() -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let d = std::env::temp_dir().join(format!(
        "lute_kinds026_{}_{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn write(dir: &Path, name: &str, body: &str) {
    std::fs::write(dir.join(name), body).unwrap();
}

fn span() -> lute_core_span::Span {
    lute_core_span::Span {
        byte_start: 0,
        byte_end: 0,
        line: 1,
        column: 1,
        utf16_range: (0, 0),
    }
}

fn imports(files: &[(&str, &str)]) -> lute_check::SchemaImports {
    let dir = unique_dir();
    for (name, body) in files {
        write(&dir, name, body);
    }
    let uses: Vec<String> = files.iter().map(|(n, _)| n.to_string()).collect();
    lute_check::resolve_imports(&dir, &uses, &[], span())
}

fn members(imp: &lute_check::SchemaImports, kind: &str) -> Vec<String> {
    match &imp.rel.kinds[kind].shape {
        KindShape::Members(ms) => ms.clone(),
        other => panic!("{kind}: {other:?}"),
    }
}

fn shape_errors(imp: &lute_check::SchemaImports) -> Vec<String> {
    imp.diags
        .iter()
        .filter(|d| d.code == "E-ENTITY-KIND-SHAPE")
        .map(|d| d.message.clone())
        .collect()
}

const ROSTER: &str = "entities:\n  person: { members: [lead] }\n";

#[test]
fn area_schemas_add_members_to_a_kind_the_roster_declares() {
    let imp = imports(&[
        ("roster.yaml", ROSTER),
        (
            "south.yaml",
            "entities:\n  person: { add: [grannyWren, oldSalt] }\n",
        ),
        ("east.yaml", "entities:\n  person: { add: [oldNed] }\n"),
    ]);
    assert!(shape_errors(&imp).is_empty(), "{:?}", imp.diags);
    assert!(
        !imp.diags.iter().any(|d| d.code == "E-KIND-NAME-CLASH"),
        "{:?}",
        imp.diags
    );
    assert_eq!(
        members(&imp, "person"),
        ["lead", "oldNed", "grannyWren", "oldSalt"]
    );
    // The attr-layer domain carries the added members too.
    assert!(imp.domains["person"]
        .members
        .contains(&"oldNed".to_string()));
}

#[test]
fn an_add_without_a_declaration_is_a_shape_error_with_a_suggestion() {
    let imp = imports(&[
        ("roster.yaml", ROSTER),
        ("south.yaml", "entities:\n  persn: { add: [grannyWren] }\n"),
    ]);
    let errs = shape_errors(&imp);
    assert_eq!(errs.len(), 1, "{:?}", imp.diags);
    assert!(
        errs[0].contains("no schema import declares `persn`"),
        "{}",
        errs[0]
    );
    assert!(errs[0].contains("did you mean `person`"), "{}", errs[0]);
    assert!(errs[0].contains("south.yaml"), "{}", errs[0]);
}

#[test]
fn a_member_added_by_two_files_names_both() {
    let imp = imports(&[
        ("roster.yaml", ROSTER),
        ("east.yaml", "entities:\n  person: { add: [ada] }\n"),
        ("south.yaml", "entities:\n  person: { add: [ada] }\n"),
    ]);
    let errs = shape_errors(&imp);
    assert_eq!(errs.len(), 1, "{:?}", imp.diags);
    assert!(
        errs[0].contains("`east.yaml`") && errs[0].contains("`south.yaml`"),
        "{}",
        errs[0]
    );
    // A member re-added over the declaration is the same error.
    let imp = imports(&[
        ("roster.yaml", ROSTER),
        ("south.yaml", "entities:\n  person: { add: [lead] }\n"),
    ]);
    let errs = shape_errors(&imp);
    assert_eq!(errs.len(), 1, "{:?}", imp.diags);
    assert!(errs[0].contains("roster.yaml"), "{}", errs[0]);
}

#[test]
fn an_add_to_an_open_kind_or_beside_members_is_a_shape_error() {
    let imp = imports(&[
        ("roster.yaml", "entities:\n  npc: { open: engine }\n"),
        ("south.yaml", "entities:\n  npc: { add: [x] }\n"),
    ]);
    assert_eq!(shape_errors(&imp).len(), 1, "{:?}", imp.diags);
    let imp = imports(&[(
        "roster.yaml",
        "entities:\n  person: { members: [a], add: [b] }\n",
    )]);
    assert_eq!(shape_errors(&imp).len(), 1, "{:?}", imp.diags);
}

#[test]
fn a_sub_kind_in_another_file_implies_its_members_in_the_parent_domain() {
    let imp = imports(&[
        ("roster.yaml", ROSTER),
        (
            "south.yaml",
            "entities:\n  trainer: { subsetOf: person, members: [ada, lead] }\n",
        ),
    ]);
    assert!(shape_errors(&imp).is_empty(), "{:?}", imp.diags);
    assert_eq!(imp.domains["person"].members, ["lead", "ada"]);
}
