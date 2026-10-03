use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

const BIN: &str = env!("CARGO_BIN_EXE_lute");
static NEXT: AtomicUsize = AtomicUsize::new(0);

fn temp(tag: &str) -> PathBuf {
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-constraints-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(root: &Path, rel: &str, text: &str) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn manifest(constraints: &str) -> String {
    format!("defaultProfile: core\nprofiles:\n  core:\n    plugins: {{}}\nconstraints:\n{constraints}")
}

fn run(root: &Path, args: &[&str]) -> (i32, String) {
    let output = Command::new(BIN).args(args).arg(root).output().unwrap();
    let text = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    (output.status.code().unwrap_or(2), text)
}

fn scene(id: &str, extra: &str) -> String {
    format!("---\nkind: scene\nid: {id}\n{extra}---\n\n## Main\n@narrator: hello\n")
}

#[test]
fn reachable_reports_holds_and_unknown_without_false_violation() {
    let root = temp("reachable");
    write(&root, "lute.project.yaml", &manifest("  - id: r\n    kind: reachable\n    node: scene:open\n  - id: u\n    kind: reachable\n    node: scene:missing\n"));
    write(&root, "open.lute", &scene("open", ""));
    let (code, out) = run(&root, &["constraints"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("r: reachable holds [proven]"), "{out}");
    assert!(out.contains("u: reachable unknown [unknown]"), "{out}");
    assert!(!out.contains("W-CONSTRAINT-UNKNOWN"), "{out}");
    assert!(!out.contains("E-CONSTRAINT-VIOLATED"), "{out}");
}

#[test]
fn completable_violation_and_run_witness_are_distinct() {
    let root = temp("completable");
    write(&root, "lute.project.yaml", &manifest("  - id: c\n    kind: completable\n    quest: q\n"));
    write(&root, "q.lute", "---\nkind: quest\nid: q\nstate:\n  run.done: { type: bool, default: false }\n---\n<quest id=\"q\" start=\"false\">\n  <objective id=\"o\" done=\"run.done\"/>\n</quest>\n");
    let (code, out) = run(&root, &["constraints"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("c: completable violated [proven]"), "{out}");
    write(&root, "plays/q.play.yaml", "steps: []\n# mentions complete but never executes a completion transition\n");
    let (code, out) = run(&root, &["constraints", "--run"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("c: completable violated [proven]") || out.contains("c: completable unknown [unknown]"), "{out}");
    assert!(!out.contains("witness:"), "{out}");
    write(
        &root,
        "q.lute",
        "---\nkind: quest\nid: q\nstate:\n  run.done: { type: bool, default: false }\n---\n<quest id=\"q\" start=\"true\">\n  <objective id=\"o\" done=\"run.done\"/>\n</quest>\n",
    );
    write(
        &root,
        "tests/q-seeded.test.yaml",
        "file: ../q.lute\nquests: { q: complete }\nexpect:\n  quests: { q: complete }\n  end: complete\n",
    );
    let (code, out) = run(&root, &["constraints", "--run"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("c: completable unknown [unknown]"), "{out}");
    write(
        &root,
        "tests/q.test.yaml",
        "file: ../q.lute\nstate: { run.done: true }\nexpect:\n  quests: { q: complete }\n  end: complete\n",
    );
    let (code, out) = run(&root, &["constraints", "--run"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("c: completable holds [witnessed]"), "{out}");
    assert!(out.contains("tests/q.test.yaml: q -> complete"), "{out}");
}

#[test]
fn speaks_only_when_uses_full_guard_context_and_voice_over_lines() {
    let root = temp("speaks");
    write(&root, "lute.project.yaml", &manifest("  - id: h\n    kind: speaksOnlyWhen\n    speaker: brann\n    when: \"holds('ok', ['brann'])\"\n"));
    write(&root, "world.schema.yaml", "entities:\n  npc: { members: [brann] }\nrelations:\n  ok: { args: [npc], tier: run }\n");
    write(&root, "scene.lute", "---\nkind: scene\nid: test\nuses: [world.schema.yaml]\n---\n<beat id=\"b\" when=\"holds('ok', ['brann'])\">\n  <branch id=\"x\"><choice id=\"y\" when=\"holds('ok', ['brann'])\">\n    @brann{vo}: guarded\n  </choice></branch>\n</beat>\n");
    let (code, out) = run(&root, &["constraints"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("h: speaksOnlyWhen holds [proven]"), "{out}");

    write(&root, "scene.lute", "---\nkind: scene\nid: test\nuses: [world.schema.yaml]\n---\n<beat id=\"b\" when=\"!holds('ok', ['brann'])\">\n  @brann{os}: contradicted\n</beat>\n");
    let (code, out) = run(&root, &["constraints"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("h: speaksOnlyWhen violated [proven]"), "{out}");
    assert!(out.contains("scene.lute"), "{out}");

    write(&root, "scene.lute", "---\nkind: scene\nid: test\nuses: [world.schema.yaml]\nstate:\n  run.flag: { type: bool, default: false }\n---\n## Main\n@brann{vo}: undecided\n");
    let (code, out) = run(&root, &["constraints"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("h: speaksOnlyWhen unknown [unknown]"), "{out}");
    assert!(!out.contains("W-CONSTRAINT-UNKNOWN"), "{out}");
    assert!(!out.contains("E-CONSTRAINT-VIOLATED"), "{out}");
}

#[test]
fn no_single_slot_progress_is_bounded_and_scoped() {
    let root = temp("slots");
    write(&root, "lute.project.yaml", &manifest("  - id: n\n    kind: noSingleSlotProgress\n    quest: '*'\n"));
    write(&root, "world.schema.yaml", "state:\n  run.day: { type: int, default: 1, owner: engine }\n  run.slot: { type: { enum: [morning, night] }, default: morning, owner: engine }\nclock:\n  day: run.day\n  slot: run.slot\n  slots: [morning, night]\n");
    write(&root, "q.lute", "---\nkind: quest\nid: q\nuses: [world.schema.yaml]\n---\n<quest id=\"q\" start=\"true\">\n  <objective id=\"o\" done=\"false\"/>\n</quest>\n");
    let (code, out) = run(&root, &["constraints", "--json"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("\"verdict\": \"holds\""), "{out}");
    assert!(out.contains("declared clock windows; no path search"), "{out}");
}

#[test]
fn declaration_errors_are_field_anchored_and_unknown_is_not_a_warning() {
    let root = temp("decl");
    write(&root, "lute.project.yaml", &manifest("  - id: bad\n    kind: nope\n    unknown: x\n"));
    write(&root, "scene.lute", "---\nkind: scene\nid: test\n---\n@narrator: hi\n");
    let (code, out) = run(&root, &["check-project"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E-CONSTRAINT-DECL"), "{out}");
    assert!(!out.contains("W-CONSTRAINT-UNKNOWN"), "{out}");

    write(&root, "lute.project.yaml", &manifest("  - id: bad\n    kind: speaksOnlyWhen\n    speaker: brann\n    when: \"holds('felled', ['brann'])\"\n"));
    write(&root, "world.schema.yaml", "relations:\n  felled: { args: [foe], kind: fact }\nentities:\n  foe: { members: [regent] }\n  npc: { members: [brann] }\n");
    write(&root, "scene.lute", "---\nkind: scene\nuses: [world.schema.yaml]\n---\n@brann: hi\n");
    let (code, out) = run(&root, &["check-project"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E-CONSTRAINT-DECL"), "{out}");
    for (constraints, needle) in [
        ("  - id: m\n    kind: reachable\n    extra: x\n    node: scene:test\n", "unknown key"),
        ("  - id: m\n    kind: nope\n", "unknown constraint kind"),
        ("  - id: m\n    kind: reachable\n", "missing a required field"),
        ("  - id: m\n    kind: reachable\n    node: malformed\n", "bad constraint node reference"),
        ("  - id: m\n    kind: reachable\n    node: scene:test\n  - id: m\n    kind: reachable\n    node: scene:test\n", "duplicate constraint id"),
        ("  - id: m\n    kind: speaksOnlyWhen\n    speaker: brann\n    when: \"[\"\n", "not valid CEL"),
    ] {
        write(&root, "lute.project.yaml", &manifest(constraints));
        let (code, out) = run(&root, &["check-project"]);
        assert_eq!(code, 1, "{out}");
        assert!(out.contains("E-CONSTRAINT-DECL"), "{out}");
        assert!(out.contains(needle), "{out}");
    }
}
