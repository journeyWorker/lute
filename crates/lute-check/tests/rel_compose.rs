//! Task 6 — `uses`/`extends` composition of the relational vocabulary
//! (`entities:`/`relations:`/`enums:`/`facts:`/`rules:`) across the import
//! DAG (dsl 0.3.0 draft §4.1, decisions D2/D5). Mirrors `tests/domains.rs`'s
//! temp-dir fixture pattern; `imp(dir, uses, extends)` calls
//! `lute_check::resolve_imports` directly, the SAME entrypoint `domains.rs`/
//! `uses_import.rs` exercise.
use std::path::{Path, PathBuf};

fn unique_dir() -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let d = std::env::temp_dir().join(format!(
        "lute_relcompose_{}_{}",
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

fn codes(imports: &lute_check::SchemaImports) -> Vec<&str> {
    imports.diags.iter().map(|d| d.code.as_str()).collect()
}

#[test]
fn peer_relation_dup_is_uses_dup_relation() {
    let dir = unique_dir();
    write(
        &dir,
        "a.yaml",
        "entities:\n  c: { members: [x] }\nrelations:\n  inParty: { args: [c] }\n",
    );
    write(
        &dir,
        "b.yaml",
        "entities:\n  d: { members: [y] }\nrelations:\n  inParty: { args: [d] }\n",
    );
    let imp = lute_check::resolve_imports(&dir, &["a.yaml".into(), "b.yaml".into()], &[], span());
    assert!(codes(&imp).contains(&"E-USES-DUP-RELATION"), "{:?}", imp.diags);
}

#[test]
fn peer_enum_dup_is_uses_dup_relation_not_domain_dup() {
    let dir = unique_dir();
    write(&dir, "a.yaml", "enums:\n  trust: [low]\n");
    write(&dir, "b.yaml", "enums:\n  trust: [high]\n");
    let imp = lute_check::resolve_imports(&dir, &["a.yaml".into(), "b.yaml".into()], &[], span());
    let c = codes(&imp);
    assert!(c.contains(&"E-USES-DUP-RELATION"), "{:?}", imp.diags);
    assert!(!c.contains(&"E-DOMAIN-DUP"), "D2 transition: {:?}", imp.diags);
}

#[test]
fn peer_kind_dup_is_kind_name_clash() {
    let dir = unique_dir();
    write(&dir, "a.yaml", "entities:\n  character: { members: [ana] }\n");
    write(&dir, "b.yaml", "entities:\n  character: { members: [bo] }\n");
    let imp = lute_check::resolve_imports(&dir, &["a.yaml".into(), "b.yaml".into()], &[], span());
    assert!(codes(&imp).contains(&"E-KIND-NAME-CLASH"), "{:?}", imp.diags);
}

#[test]
fn extends_child_may_grow_kind_but_not_shrink_or_flip() {
    let dir = unique_dir();
    write(
        &dir,
        "base.yaml",
        "entities:\n  character: { members: [ana, bo] }\nenums:\n  trust: [low, high]\n",
    );
    // grow: superset re-list — legal, merged = child's list
    write(
        &dir,
        "grow.yaml",
        "extends: base.yaml\nentities:\n  character: { members: [ana, bo, cy] }\n",
    );
    let ok = lute_check::resolve_imports(&dir, &["grow.yaml".into()], &[], span());
    assert!(!codes(&ok).contains(&"E-EXTENDS-RELATION-SIG"), "{:?}", ok.diags);
    assert_eq!(
        ok.rel.kinds["character"].shape,
        lute_manifest::relations::KindShape::Members(vec!["ana".into(), "bo".into(), "cy".into()])
    );
    // shrink: missing base member — E-EXTENDS-RELATION-SIG
    write(
        &dir,
        "shrink.yaml",
        "extends: base.yaml\nentities:\n  character: { members: [ana] }\n",
    );
    let bad = lute_check::resolve_imports(&dir, &["shrink.yaml".into()], &[], span());
    assert!(codes(&bad).contains(&"E-EXTENDS-RELATION-SIG"), "{:?}", bad.diags);
    // flip: members -> open
    write(
        &dir,
        "flip.yaml",
        "extends: base.yaml\nentities:\n  character: { open: engine }\n",
    );
    let flip = lute_check::resolve_imports(&dir, &["flip.yaml".into()], &[], span());
    assert!(codes(&flip).contains(&"E-EXTENDS-RELATION-SIG"), "{:?}", flip.diags);
}

#[test]
fn extends_child_may_grow_enum_but_not_shrink() {
    let dir = unique_dir();
    write(&dir, "base.yaml", "enums:\n  trust: [low, high]\n");
    write(&dir, "grow.yaml", "extends: base.yaml\nenums:\n  trust: [low, high, absolute]\n");
    let ok = lute_check::resolve_imports(&dir, &["grow.yaml".into()], &[], span());
    assert!(!codes(&ok).contains(&"E-EXTENDS-RELATION-SIG"), "{:?}", ok.diags);
    assert_eq!(
        ok.rel.enums["trust"],
        vec!["low".to_string(), "high".to_string(), "absolute".to_string()]
    );
    write(&dir, "shrink.yaml", "extends: base.yaml\nenums:\n  trust: [low]\n");
    let bad = lute_check::resolve_imports(&dir, &["shrink.yaml".into()], &[], span());
    assert!(codes(&bad).contains(&"E-EXTENDS-RELATION-SIG"), "{:?}", bad.diags);
}

#[test]
fn extends_relation_signature_must_match_exactly() {
    let dir = unique_dir();
    write(
        &dir,
        "base.yaml",
        "entities:\n  c: { members: [x] }\nrelations:\n  r: { args: [c], tier: run }\n",
    );
    write(
        &dir,
        "child.yaml",
        "extends: base.yaml\nrelations:\n  r: { args: [c], tier: user }\n",
    );
    let imp = lute_check::resolve_imports(&dir, &["child.yaml".into()], &[], span());
    assert!(codes(&imp).contains(&"E-EXTENDS-RELATION-SIG"), "{:?}", imp.diags);
    // identical re-declaration is fine
    write(
        &dir,
        "same.yaml",
        "extends: base.yaml\nrelations:\n  r: { args: [c], tier: run }\n",
    );
    let ok = lute_check::resolve_imports(&dir, &["same.yaml".into()], &[], span());
    assert!(!codes(&ok).contains(&"E-EXTENDS-RELATION-SIG"), "{:?}", ok.diags);
}

#[test]
fn extends_relation_signature_checks_key_too() {
    // D5: `key:` is included in the full-decl match even though the spec text
    // names only args/tier/derive/reserved — a differing functional key
    // silently changes engine auto-invalidation semantics.
    let dir = unique_dir();
    write(
        &dir,
        "base.yaml",
        "entities:\n  c: { members: [x] }\nrelations:\n  r: { args: [c, c], key: [0] }\n",
    );
    write(
        &dir,
        "child.yaml",
        "extends: base.yaml\nrelations:\n  r: { args: [c, c], key: [1] }\n",
    );
    let imp = lute_check::resolve_imports(&dir, &["child.yaml".into()], &[], span());
    assert!(codes(&imp).contains(&"E-EXTENDS-RELATION-SIG"), "{:?}", imp.diags);
}

#[test]
fn facts_and_rules_union_across_dag() {
    let dir = unique_dir();
    write(
        &dir,
        "base.yaml",
        "entities:\n  c: { members: [x] }\nrelations:\n  b: { args: [c] }\n  d: { args: [c], derive: true }\nfacts:\n  - \"b(x)\"\nrules:\n  - \"d(X) :- b(X)\"\n",
    );
    write(
        &dir,
        "child.yaml",
        "extends: base.yaml\nfacts:\n  - \"b(x)\"\nrules:\n  - \"d(X) :- b(X), b(X)\"\n",
    );
    let imp = lute_check::resolve_imports(&dir, &["child.yaml".into()], &[], span());
    assert_eq!(imp.rel.facts.len(), 2, "facts union (dups are harmless, §4)");
    assert_eq!(imp.rel.rules.len(), 2, "rules always union (§4.1)");
}

#[test]
fn facts_and_rules_union_is_deterministic_by_depth_then_file() {
    // Same DAG shape, opposite root ordering — the merged fact/rule order must
    // be depth-then-file-then-index, not import-list order (mirrors
    // `uses_import.rs`'s `extends_dup_detection_is_order_independent`).
    let dir = unique_dir();
    write(&dir, "a.yaml", "facts:\n  - \"p(a)\"\n");
    write(&dir, "b.yaml", "facts:\n  - \"p(b)\"\n");
    let forward = lute_check::resolve_imports(&dir, &["a.yaml".into(), "b.yaml".into()], &[], span());
    let reverse = lute_check::resolve_imports(&dir, &["b.yaml".into(), "a.yaml".into()], &[], span());
    let forward_raw: Vec<&str> = forward.rel.facts.iter().map(|f| f.raw.as_str()).collect();
    let reverse_raw: Vec<&str> = reverse.rel.facts.iter().map(|f| f.raw.as_str()).collect();
    assert_eq!(
        forward_raw, reverse_raw,
        "fact union order must be independent of the `uses:` list order"
    );
}

// --- D2 migration: the two project-project collisions that used to share
// `E-DOMAIN-DUP` now split into `E-USES-DUP-RELATION` (enums:/relations:)
// vs `E-KIND-NAME-CLASH` (entities:), never `E-DOMAIN-DUP`. ---

#[test]
fn d2_peer_enum_collision_never_e_domain_dup() {
    let dir = unique_dir();
    write(&dir, "x.yaml", "enums:\n  mood: [calm]\n");
    write(&dir, "y.yaml", "enums:\n  mood: [tense]\n");
    let imp = lute_check::resolve_imports(&dir, &["x.yaml".into(), "y.yaml".into()], &[], span());
    let c = codes(&imp);
    assert!(c.contains(&"E-USES-DUP-RELATION"), "{:?}", imp.diags);
    assert!(!c.contains(&"E-DOMAIN-DUP"), "{:?}", imp.diags);
}

#[test]
fn d2_peer_entity_collision_never_e_domain_dup() {
    let dir = unique_dir();
    write(&dir, "x.yaml", "entities:\n  npc: { members: [a] }\n");
    write(&dir, "y.yaml", "entities:\n  npc: { members: [b] }\n");
    let imp = lute_check::resolve_imports(&dir, &["x.yaml".into(), "y.yaml".into()], &[], span());
    let c = codes(&imp);
    assert!(c.contains(&"E-KIND-NAME-CLASH"), "{:?}", imp.diags);
    assert!(!c.contains(&"E-DOMAIN-DUP"), "{:?}", imp.diags);
}
