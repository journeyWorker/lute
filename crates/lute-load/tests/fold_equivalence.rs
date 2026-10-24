//! spec 0.49.0 §9.2: the shared fold (`fold_env`, reusing the project
//! vocab's base stages) returns exactly what the forced full merge
//! (`fold_env_full_merge`, the 0.48.1 path) returns — the folded environment
//! and both diagnostic streams, in order. Inputs are assembled through one
//! `InputCache` per project, as a project check assembles them, so every
//! document of a project folds against the same shared `ProjectVocab`.

use std::path::{Path, PathBuf};

use lute_check::{fold_env, fold_env_full_merge};
use lute_load::{assemble_input, InputCache};

/// Fold `file` both ways and fail on the first difference.
fn assert_equivalent(cache: &InputCache, file: &Path, project: Option<&Path>) {
    let text = std::fs::read_to_string(file).expect("read the document");
    let (built, (doc, _)) = assemble_input(cache, file, text, None, project, None);
    let shared = format!("{:#?}", fold_env(&doc, &built.input));
    let full = format!("{:#?}", fold_env_full_merge(&doc, &built.input));
    if shared != full {
        panic!(
            "{}: the shared fold differs from the full merge\n{}",
            file.display(),
            first_difference(&shared, &full)
        );
    }
}

/// The first differing line of two renderings, with a little context.
fn first_difference(shared: &str, full: &str) -> String {
    let (a, b): (Vec<&str>, Vec<&str>) = (shared.lines().collect(), full.lines().collect());
    let at = a
        .iter()
        .zip(&b)
        .position(|(x, y)| x != y)
        .unwrap_or(a.len().min(b.len()));
    let from = at.saturating_sub(8);
    let window =
        |lines: &[&str]| lines[from.min(lines.len())..(at + 4).min(lines.len())].join("\n");
    format!(
        "--- shared (line {at})\n{}\n--- full merge\n{}",
        window(&a),
        window(&b)
    )
}

fn examples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/examples")
}

/// Every `.lute` under `dir`, sorted.
fn lute_files(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).expect("read a corpus directory") {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|e| e == "lute") {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// The corpus (`docs/examples`, the CLI snapshot's documents): each document
/// folded inside its project, as `check --project` folds it, and loose, as
/// a single-file `check` without a manifest folds it.
#[test]
fn the_corpus_folds_identically() {
    let root = examples();
    let files = lute_files(&root);
    assert!(files.len() > 400, "corpus found: {}", files.len());
    let mut caches: std::collections::BTreeMap<PathBuf, InputCache> = Default::default();
    let loose = InputCache::default();
    for file in &files {
        let project = lute_load::discovery::nearest_manifest_dir(file);
        if let Some(project) = &project {
            let cache = caches.entry(project.clone()).or_default();
            assert_equivalent(cache, file, Some(project));
        }
        assert_equivalent(&loose, file, None);
    }
}

/// A scratch project: `files` written under a fresh directory, then every
/// `.lute` among them folded both ways against one shared cache.
fn project(files: &[(&str, &str)]) {
    let dir = lute_scratch::ScratchDir::named("fold-eq");
    for (name, text) in files {
        let path = dir.join(name);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("mkdir");
        std::fs::write(&path, text).expect("write a fixture");
    }
    let cache = InputCache::default();
    let mut folded = 0;
    for (name, _) in files {
        if name.ends_with(".lute") {
            assert_equivalent(&cache, &dir.join(name), None);
            folded += 1;
        }
    }
    assert!(folded > 0, "no document folded");
}

/// A scene that changes nothing of its imports: it reuses every base stage,
/// so each fixture below also checks the unchanged path beside the changed
/// one.
const PLAIN: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n\
                     uses: world.schema.yaml\n---\n## Shot 1.\n@narrator: hi\n";

/// R1: a base path typed by a domain the document extends is resolved
/// against the document's members (`run.person` gains `b`).
#[test]
fn r1_an_inline_add_reresolves_the_base_paths_of_its_domain() {
    project(&[
        (
            "world.schema.yaml",
            "entities:\n  person: { members: [a] }\nstate:\n  run.person: { type: { entity: person } }\n",
        ),
        ("plain.lute", PLAIN),
        (
            "add.lute",
            "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nuses: world.schema.yaml\n\
             entities:\n  person:\n    add: [b]\n---\n## Shot 1.\n\
             @narrator{when=\"run.person == 'b'\"}: hi\n",
        ),
    ]);
}

const CLOCK_BASE: &str = "enums:\n  slot: { members: [morning, night] }\n\
                          state:\n  run.day: { type: int, default: 1, owner: engine }\n  \
                          run.slot: { type: { domain: slot }, default: morning, owner: engine }\n";

/// R2: the clock stage runs on the document's effective state — an
/// `extends` refinement of the day path's default moves the clock's start.
#[test]
fn r2_an_extends_refinement_of_the_clock_day_reruns_the_clock() {
    project(&[
        (
            "world.schema.yaml",
            &format!(
                "{CLOCK_BASE}clock:\n  day: run.day\n  slot: run.slot\n  slots: [morning, night]\n  days: 7\n"
            ),
        ),
        ("plain.lute", PLAIN),
        (
            "refine.lute",
            "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nextends: world.schema.yaml\n\
             state:\n  run.day: { type: int, default: 3, owner: engine }\n---\n## Shot 1.\n\
             @narrator{when=\"clock.day == 3\"}: hi\n",
        ),
    ]);
}

/// R2: a schema document's own `clock:` (a kindless document of
/// declarations only) over imported paths.
#[test]
fn r2_an_inline_clock_reruns_the_clock() {
    project(&[
        ("world.schema.yaml", CLOCK_BASE),
        ("plain.lute", PLAIN),
        (
            "clock.lute",
            "---\nuses: world.schema.yaml\nstate:\n  run.seen: { type: bool, default: false }\n\
             clock:\n  day: run.day\n  slot: run.slot\n  slots: [morning, night]\n  days: 7\n---\n",
        ),
    ]);
}

/// R3: an inline def the imported defs read changes their settled types
/// (`outer` reads `@inner`, which the document redefines as a string).
#[test]
fn r3_an_inline_def_resettles_the_imported_defs() {
    project(&[
        (
            "world.schema.yaml",
            "state:\n  run.n: { type: int, default: 0 }\ndefs:\n  inner: \"run.n + 1\"\n  outer: \"@inner\"\n",
        ),
        ("plain.lute", PLAIN),
        (
            "inner.lute",
            "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nuses: world.schema.yaml\n\
             defs:\n  inner: \"'one'\"\n---\n## Shot 1.\n@narrator{when=\"@outer == 'one'\"}: hi\n",
        ),
    ]);
}

/// An inline enum overriding an imported one of the same name, read by an
/// imported path.
#[test]
fn an_inline_enum_overriding_an_imported_one_folds_identically() {
    project(&[
        (
            "world.schema.yaml",
            "enums:\n  trust: [low, high]\nstate:\n  run.trust: { type: { domain: trust }, default: low }\n",
        ),
        ("plain.lute", PLAIN),
        (
            "enum.lute",
            "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nuses: world.schema.yaml\n\
             enums:\n  trust: [low, high, absolute]\n---\n## Shot 1.\n\
             @narrator{when=\"run.trust == 'absolute'\"}: hi\n",
        ),
    ]);
}

/// R3: a document declaring a path an imported def reads settles the
/// imported defs again (`mode`'s type is `run.mode`'s, which only the
/// document declares).
#[test]
fn r3_a_declared_path_an_imported_def_reads_resettles_it() {
    project(&[
        (
            "world.schema.yaml",
            "state:\n  run.n: { type: int, default: 0 }\ndefs:\n  mode: \"run.mode\"\n",
        ),
        ("plain.lute", PLAIN),
        (
            "mode.lute",
            "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nuses: world.schema.yaml\n\
             state:\n  run.mode: { type: { enum: [x, y] } }\n---\n## Shot 1.\n\
             @narrator{when=\"@mode == 'x'\"}: hi\n",
        ),
    ]);
}

/// A `uses:`-peer redeclaration is rejected from the effective state but is
/// still the document's own `state:` key, where a value/prefix report on it
/// is placed.
#[test]
fn a_rejected_redeclaration_of_a_prefix_path_is_reported_at_the_document() {
    project(&[
        (
            "world.schema.yaml",
            "state:\n  run.a: { type: int }\n  run.a.b: { type: int }\n",
        ),
        ("plain.lute", PLAIN),
        (
            "redeclare.lute",
            "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nuses: world.schema.yaml\n\
             state:\n  run.a: { type: int }\n---\n## Shot 1.\n@narrator: hi\n",
        ),
    ]);
}

/// The same for a `{ domain: K }` report: an imported path whose default is
/// outside its domain, redeclared (and rejected) by the document.
#[test]
fn a_rejected_redeclaration_of_a_domain_path_is_reported_at_the_document() {
    project(&[
        (
            "world.schema.yaml",
            "enums:\n  mood: [calm, cross]\nstate:\n  run.m: { type: { domain: mood }, default: glad }\n",
        ),
        ("plain.lute", PLAIN),
        (
            "redeclare.lute",
            "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nuses: world.schema.yaml\n\
             state:\n  run.m: { type: { domain: mood }, default: glad }\n---\n## Shot 1.\n@narrator: hi\n",
        ),
    ]);
}

/// #190: an inline `per:` family over a kind only the imports declare is
/// merged member by member against them — a `uses`-peer redeclaration is
/// refused, an `extends` refinement (one member declared explicitly) and a
/// retype are applied — on the shared fold as on the full merge.
#[test]
fn an_inline_family_over_an_imported_kind_merges_identically() {
    const WORLD: &str = "entities:\n  npc: { members: [tavi, brann] }\nstate:\n  \
                         run.bond: { type: int, default: 0, per: npc }\n";
    let scene = |link: &str, state: &str| {
        format!(
            "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n{link}: world.schema.yaml\n\
             state:\n{state}---\n## Shot 1.\n@narrator{{when=\"run.bond.tavi > 1\"}}: hi\n"
        )
    };
    project(&[
        ("world.schema.yaml", WORLD),
        ("plain.lute", PLAIN),
        (
            "redeclare.lute",
            &scene("uses", "  run.bond: { type: string, per: npc }\n"),
        ),
        (
            "refine.lute",
            &scene(
                "extends",
                "  run.bond: { type: int, default: 3, per: npc }\n  run.bond.tavi: { type: int, default: 7 }\n",
            ),
        ),
        (
            "retype.lute",
            &scene("extends", "  run.bond: { type: string, per: npc }\n"),
        ),
        (
            "redeclare-member.lute",
            &scene(
                "uses",
                "  run.bond: { type: string, per: npc }\n  run.bond.tavi: { type: string }\n",
            ),
        ),
    ]);
}

/// #196: a schema's `per:` family over a kind another schema declares is
/// merged by the family rules — an `extends` refinement, a `uses`-peer
/// duplicate — on the shared fold as on the full merge, beside a scene that
/// refines the refined family once more.
#[test]
fn a_schema_family_over_an_imported_kind_merges_identically() {
    let scene = |schema: &str, state: &str| {
        format!(
            "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nextends: {schema}\n\
             state:\n{state}---\n## Shot 1.\n@narrator{{when=\"run.bond.tavi > 1\"}}: hi\n"
        )
    };
    project(&[
        (
            "world.schema.yaml",
            "entities:\n  npc: { members: [tavi, brann] }\nstate:\n  \
             run.bond: { type: int, default: 0, per: npc }\n",
        ),
        (
            "refine.schema.yaml",
            "extends: world.schema.yaml\nstate:\n  run.bond: { type: int, default: 3, per: npc }\n",
        ),
        (
            "peer.schema.yaml",
            "uses: world.schema.yaml\nstate:\n  run.bond: { type: int, default: 3, per: npc }\n",
        ),
        (
            "plain.lute",
            &scene("refine.schema.yaml", "  run.seen: { type: bool }\n"),
        ),
        (
            "refine.lute",
            &scene(
                "refine.schema.yaml",
                "  run.bond: { type: int, default: 5, per: npc }\n",
            ),
        ),
        (
            "peer.lute",
            &scene("peer.schema.yaml", "  run.seen: { type: bool }\n"),
        ),
    ]);
}

/// F5b review R1: a schema family over another schema's kind that wins the
/// merge is where its paths are declared — its default outside the enum is
/// rejected at that schema (`b.schema.yaml`) on the shared fold and on the
/// full merge alike, whether the base family's kind is another schema's or
/// the base's own.
#[test]
fn a_winning_family_over_an_imported_kind_is_located_at_its_schema() {
    let kinds = "entities:\n  npc: { members: [tavi] }\nenums:\n  mood: [good]\n";
    let family = "  run.rep: { type: { domain: mood }, default: good, per: npc }\n";
    let in_c = format!("uses: c.schema.yaml\nstate:\n{family}");
    let in_a = format!("{kinds}state:\n{family}");
    for files in [
        vec![("c.schema.yaml", kinds), ("a.schema.yaml", in_c.as_str())],
        vec![("a.schema.yaml", in_a.as_str())],
    ] {
        let dir = lute_scratch::ScratchDir::named("fold-eq-r1");
        let mut all = files;
        all.push((
            "b.schema.yaml",
            "extends: a.schema.yaml\nstate:\n  \
             run.rep: { type: { domain: mood }, default: bad, per: npc }\n",
        ));
        all.push((
            "s.lute",
            "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nuses: b.schema.yaml\n---\n\
             ## Shot 1.\n@narrator: hi\n",
        ));
        for (name, text) in &all {
            std::fs::write(dir.join(name), text).expect("write a fixture");
        }
        let file = dir.join("s.lute");
        let cache = InputCache::default();
        assert_equivalent(&cache, &file, None);
        let text = std::fs::read_to_string(&file).expect("read the document");
        let (built, (doc, _)) = assemble_input(&cache, &file, text, None, None, None);
        for (how, (_, diags, _)) in [
            ("shared", fold_env(&doc, &built.input)),
            ("full", fold_env_full_merge(&doc, &built.input)),
        ] {
            let rejected: Vec<_> = diags
                .iter()
                .filter(|d| d.code == "E-STATE-DECL" && d.message.contains("default: bad"))
                .collect();
            assert_eq!(rejected.len(), 1, "{how}: {diags:#?}");
            assert!(
                rejected[0]
                    .message
                    .contains("(declared in schema import `b.schema.yaml`)"),
                "{how}: {rejected:#?}"
            );
        }
    }
}

/// 0.49.0 §4.3 (#178): a reference-typed plugin frontmatter key is checked
/// against the document's own merged vocabulary, on the shared fold as on the
/// full merge: `lead: b` is a member where the document `add:`s `b` to the
/// imported `person: [a]`, and `E-BAD-ENUM` in a sibling that does not.
#[test]
fn a_frontmatter_entity_reads_the_document_s_own_members() {
    let dir = lute_scratch::ScratchDir::named("fold-eq-fm");
    let files = [
        (
            "lute.project.yaml",
            "pluginsDir: plugins/\ndefaultProfile: p\nprofiles:\n  p:\n    plugins: { t.lead: true }\n",
        ),
        (
            "plugins/t.lead/plugin.yaml",
            "id: t.lead\nversion: 0.1.0\nkind: capability\n\
             depends: [ { id: lute.core, range: \"^0.0.1\" } ]\nexports:\n  frontmatter: fm/\n",
        ),
        (
            "plugins/t.lead/fm/lead.yaml",
            "frontmatter:\n  - key: lead\n    schema: { entity: person }\n",
        ),
        (
            "world.schema.yaml",
            "entities:\n  person: { members: [a] }\n",
        ),
        (
            "add.lute",
            "---\nkind: scene\nid: add\nuses: world.schema.yaml\nlead: b\n\
             entities:\n  person:\n    add: [b]\n---\n## Shot 1.\n@narrator: hi\n",
        ),
        (
            "plain.lute",
            "---\nkind: scene\nid: plain\nuses: world.schema.yaml\nlead: b\n---\n## Shot 1.\n@narrator: hi\n",
        ),
    ];
    for (name, text) in files {
        let path = dir.join(name);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("mkdir");
        std::fs::write(&path, text).expect("write a fixture");
    }
    let cache = InputCache::default();
    let codes = |name: &str| {
        let file = dir.join(name);
        assert_equivalent(&cache, &file, Some(&dir));
        let text = std::fs::read_to_string(&file).expect("read the document");
        let (built, (doc, _)) = assemble_input(&cache, &file, text, None, Some(&dir), None);
        let (_, a, b) = fold_env(&doc, &built.input);
        a.into_iter()
            .chain(b)
            .filter(|d| d.severity == lute_core_span::Severity::Error)
            .map(|d| format!("{}: {}", d.code, d.message))
            .collect::<Vec<_>>()
    };
    assert_eq!(codes("add.lute"), Vec::<String>::new());
    assert_eq!(
        codes("plain.lute"),
        vec![
            "E-BAD-ENUM: `b` is not a member of entity kind `person` (attribute `lead` of \
             `frontmatter`)"
                .to_string()
        ]
    );
}
