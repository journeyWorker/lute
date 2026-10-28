//! Spec 0.53.0 §3.5: the language server keeps one input cache per project
//! root across requests, revalidated before each request and reclaimed after
//! it. The tests drive the analysis the backend publishes
//! ([`analyze_document`]) on a project under the scratch directory, editing
//! files between requests.

use std::path::{Path, PathBuf};

use lute_check::Mode;
use lute_load::cache::RetainedCounts;
use lute_load::{assemble_input_with_mode, InputCache};
use lute_lsp::backend::{analyze_document, ProjectInputs, SUPERSEDED_RETAINED};
use lute_scratch::ScratchDir;

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// A project root (canonical, so every recorded path is as the server sees
/// it) with `profiles:` `core` and `alt`, and the plugin `t.plug` installed.
fn project(tag: &str, manifest_tail: &str) -> (ScratchDir, PathBuf) {
    let dir = ScratchDir::named(tag);
    let root = std::fs::canonicalize(&*dir).unwrap();
    std::fs::create_dir_all(root.join("catalog")).unwrap();
    write(
        &root.join("lute.project.yaml"),
        &format!(
            "defaultProfile: core\nprofiles:\n  core:\n    plugins: {{}}\n  alt:\n    plugins: \
             {{ t.plug: {{}} }}\n{manifest_tail}"
        ),
    );
    write(
        &root.join("plugins/t.plug/plugin.yaml"),
        "id: t.plug\nversion: 0.1.0\nkind: capability\nexports:\n  directives: directives/\n",
    );
    write(
        &root.join("plugins/t.plug/directives/a.yaml"),
        "directives:\n  - { name: foo, attrs: [ { name: x, type: bool } ] }\n",
    );
    (dir, root)
}

/// A scene with `frontmatter` lines and `body`.
fn scene(frontmatter: &str, body: &str) -> String {
    format!("---\nkind: scene\nid: scene.a\n{frontmatter}---\n\n## A\n\n{body}\n")
}

/// The diagnostic codes of one request on `file` (opened in `inputs`) with
/// the buffer `text`; the file exists on disk, as an opened document's does.
fn codes(inputs: &ProjectInputs, file: &Path, text: &str) -> Vec<String> {
    if !file.exists() {
        write(file, text);
    }
    let doc = file.display().to_string();
    inputs.open(&doc);
    let analysis = analyze_document(inputs, &doc, file, text);
    let mut codes: Vec<String> = analysis
        .diagnostics
        .iter()
        .map(|d| d.code.clone())
        .chain(analysis.resolve_diags.iter().map(|d| d.code.clone()))
        .collect();
    codes.sort();
    codes
}

fn has(codes: &[String], code: &str) -> bool {
    codes.iter().any(|c| c == code)
}

const BAD: &str = "@narrator{emotion=\"gleeful\"}: Hi.";

#[test]
fn twenty_requests_build_one_base_and_one_vocabulary() {
    let (_dir, root) = project("lsp-twenty", "");
    write(
        &root.join("schema/w.yaml"),
        "enums:\n  emotion: [neutral]\n",
    );
    let file = root.join("scenes/a.lute");
    let inputs = ProjectInputs::default();
    for n in 0..20 {
        let text = scene(
            "uses: [../schema/w.yaml]\n",
            &format!("@narrator: Line {n}."),
        );
        codes(&inputs, &file, &text);
    }
    let stats = inputs.stats(Some(&root));
    assert_eq!((stats.bases, stats.vocabs, stats.assemblies), (1, 1, 20));
}

#[test]
fn an_edited_schema_is_read_on_the_next_request_without_a_notification() {
    let (_dir, root) = project("lsp-schema", "");
    let schema = root.join("schema/w.yaml");
    write(&schema, "enums:\n  emotion: [neutral]\n");
    let file = root.join("scenes/a.lute");
    let text = scene("uses: [../schema/w.yaml]\n", BAD);
    let inputs = ProjectInputs::default();
    assert!(has(&codes(&inputs, &file, &text), "E-BAD-ENUM"));
    write(&schema, "enums:\n  emotion: [neutral, gleeful]\n");
    assert!(!has(&codes(&inputs, &file, &text), "E-BAD-ENUM"));
}

#[test]
fn an_edited_plugin_directive_and_manifest_default_are_read_on_the_next_request() {
    let (_dir, root) = project("lsp-plugin", "defaults:\n  uses: [schema/a.yaml]\n");
    write(
        &root.join("schema/a.yaml"),
        "enums:\n  emotion: [neutral]\n",
    );
    write(
        &root.join("schema/b.yaml"),
        "enums:\n  emotion: [neutral, gleeful]\n",
    );
    let file = root.join("scenes/a.lute");
    let inputs = ProjectInputs::default();

    // A directive attribute the plugin file does not declare, then does.
    let directive = scene("profile: alt\n", "::foo{y=true}");
    let before = codes(&inputs, &file, &directive);
    assert!(
        before.iter().any(|c| c.starts_with("E-")),
        "an undeclared attribute is an error: {before:?}"
    );
    write(
        &root.join("plugins/t.plug/directives/a.yaml"),
        "directives:\n  - { name: foo, attrs: [ { name: x, type: bool }, { name: y, type: bool } ] }\n",
    );
    let after = codes(&inputs, &file, &directive);
    assert!(
        !after.iter().any(|c| c.starts_with("E-")),
        "the directive file's new attribute is read: {after:?}"
    );

    // The manifest's `defaults: uses:` names another schema.
    let text = scene("", BAD);
    assert!(has(&codes(&inputs, &file, &text), "E-BAD-ENUM"));
    let manifest = std::fs::read_to_string(root.join("lute.project.yaml")).unwrap();
    write(
        &root.join("lute.project.yaml"),
        &manifest.replace("schema/a.yaml", "schema/b.yaml"),
    );
    assert!(!has(&codes(&inputs, &file, &text), "E-BAD-ENUM"));
}

#[test]
fn creating_a_missing_import_clears_its_error() {
    let (_dir, root) = project("lsp-missing", "");
    let file = root.join("scenes/a.lute");
    let text = scene("uses: [../schema/later.yaml]\n", "@narrator: Hi.");
    let inputs = ProjectInputs::default();
    assert!(has(&codes(&inputs, &file, &text), "E-USES-NOT-FOUND"));
    write(
        &root.join("schema/later.yaml"),
        "enums:\n  emotion: [neutral]\n",
    );
    assert!(!has(&codes(&inputs, &file, &text), "E-USES-NOT-FOUND"));
}

#[test]
fn a_new_glob_or_catalog_match_is_read_and_a_non_matching_file_keeps_the_cache() {
    let (_dir, root) = project("lsp-glob", "defaults:\n  uses: \"schema/*.schema.yaml\"\n");
    write(
        &root.join("schema/a.schema.yaml"),
        "state:\n  user.n: { type: int, default: 0 }\n",
    );
    let file = root.join("scenes/a.lute");
    let text = scene("", BAD);
    let inputs = ProjectInputs::default();
    let first = codes(&inputs, &file, &text);
    assert_eq!(inputs.stats(Some(&root)).bases, 1);

    // Non-matching new files: the cache is kept.
    write(&root.join("schema/notes.txt"), "x");
    write(&root.join("catalog/readme.md"), "x");
    codes(&inputs, &file, &text);
    assert_eq!(inputs.stats(Some(&root)).bases, 1, "nothing to rebuild");

    // A new schema matching the glob is imported.
    write(
        &root.join("schema/b.schema.yaml"),
        "enums:\n  emotion: [neutral, gleeful]\n",
    );
    let second = codes(&inputs, &file, &text);
    assert_eq!(inputs.stats(Some(&root)).bases, 2, "rebuilt once");
    assert_ne!(first, second, "the new schema's `emotion` is read");
    assert!(!has(&second, "E-BAD-ENUM"));

    // A new provider snapshot in the catalog directory.
    write(
        &root.join("catalog/cast.yaml"),
        "manifestVersion: test\nproviderVersion: \"1\"\nstale: false\nentries:\n  speakerId: [hero]\n",
    );
    codes(&inputs, &file, &text);
    assert_eq!(
        inputs.stats(Some(&root)).bases,
        3,
        "rebuilt for the catalog"
    );
}

#[cfg(unix)]
#[test]
fn a_retargeted_import_symlink_is_followed() {
    let (_dir, root) = project("lsp-symlink", "");
    write(
        &root.join("schema/a.yaml"),
        "enums:\n  emotion: [neutral]\n",
    );
    write(
        &root.join("schema/b.yaml"),
        "enums:\n  emotion: [neutral, gleeful]\n",
    );
    let link = root.join("schema/link.yaml");
    std::os::unix::fs::symlink("a.yaml", &link).unwrap();
    let file = root.join("scenes/a.lute");
    let text = scene("uses: [../schema/link.yaml]\n", BAD);
    let inputs = ProjectInputs::default();
    assert!(has(&codes(&inputs, &file, &text), "E-BAD-ENUM"));
    std::fs::remove_file(&link).unwrap();
    std::os::unix::fs::symlink("b.yaml", &link).unwrap();
    assert!(!has(&codes(&inputs, &file, &text), "E-BAD-ENUM"));
}

/// Set `path`'s mtime (what `touch -d` does).
fn set_mtime(path: &Path, at: std::time::SystemTime) {
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(at))
        .unwrap();
}

#[test]
fn a_touched_unchanged_schema_keeps_the_cache_with_and_without_a_notification() {
    let (_dir, root) = project("lsp-touch", "");
    let schema = root.join("schema/w.yaml");
    write(&schema, "enums:\n  emotion: [neutral]\n");
    let file = root.join("scenes/a.lute");
    let text = scene("uses: [../schema/w.yaml]\n", BAD);
    let inputs = ProjectInputs::default();
    codes(&inputs, &file, &text);
    let built = inputs.stats(Some(&root));
    let later = std::fs::metadata(&schema).unwrap().modified().unwrap()
        + std::time::Duration::from_secs(10);
    set_mtime(&schema, later);
    assert!(has(&codes(&inputs, &file, &text), "E-BAD-ENUM"));
    assert_eq!(inputs.stats(Some(&root)).bases, built.bases);
    assert_eq!(inputs.stats(Some(&root)).vocabs, built.vocabs);
    // `didSave` / `didChangeWatchedFiles` for it: a content compare, equal.
    set_mtime(&schema, later + std::time::Duration::from_secs(10));
    inputs.mark_dirty(&schema);
    assert!(has(&codes(&inputs, &file, &text), "E-BAD-ENUM"));
    assert_eq!(inputs.stats(Some(&root)).bases, built.bases);
    assert_eq!(inputs.stats(Some(&root)).vocabs, built.vocabs);
}

#[cfg(unix)]
#[test]
fn an_equal_length_edit_with_its_mtime_restored_is_detected() {
    let (_dir, root) = project("lsp-ctime", "");
    let schema = root.join("schema/w.yaml");
    write(&schema, "enums:\n  emotion: [neutral, gleefux]\n");
    let mtime = std::fs::metadata(&schema).unwrap().modified().unwrap();
    // Past the racy window, so only the fingerprint guards the record.
    std::thread::sleep(std::time::Duration::from_millis(2_100));
    let file = root.join("scenes/a.lute");
    let text = scene("uses: [../schema/w.yaml]\n", BAD);
    let inputs = ProjectInputs::default();
    assert!(has(&codes(&inputs, &file, &text), "E-BAD-ENUM"));
    std::fs::write(&schema, "enums:\n  emotion: [neutral, gleeful]\n").unwrap();
    set_mtime(&schema, mtime);
    assert_eq!(
        std::fs::metadata(&schema).unwrap().modified().unwrap(),
        mtime
    );
    assert!(
        !has(&codes(&inputs, &file, &text), "E-BAD-ENUM"),
        "the new bytes are read (the ctime moved)"
    );
}

#[test]
fn two_profiles_in_one_root_keep_two_bases() {
    let (_dir, root) = project("lsp-profiles", "");
    let a = root.join("scenes/a.lute");
    let b = root.join("scenes/b.lute");
    let core = scene("", "@narrator: Hi.");
    let alt = scene("profile: alt\n", "@narrator: Hi.");
    let inputs = ProjectInputs::default();
    for _ in 0..5 {
        codes(&inputs, &a, &core);
        codes(&inputs, &b, &alt);
    }
    assert_eq!(inputs.stats(Some(&root)).bases, 2);
}

#[test]
fn a_notification_invalidates_for_a_recorded_path_only() {
    let (_dir, root) = project("lsp-notify", "");
    let schema = root.join("schema/w.yaml");
    write(&schema, "enums:\n  emotion: [neutral]\n");
    let file = root.join("scenes/a.lute");
    let text = scene("uses: [../schema/w.yaml]\n", BAD);
    let inputs = ProjectInputs::default();
    codes(&inputs, &file, &text);
    let unrelated = root.join("notes.md");
    write(&unrelated, "x");
    inputs.mark_dirty(&unrelated);
    codes(&inputs, &file, &text);
    assert_eq!(inputs.stats(Some(&root)).bases, 1, "an unrelated path");
    write(&schema, "enums:\n  emotion: [neutral, gleeful]\n");
    inputs.mark_dirty(&schema);
    assert!(!has(&codes(&inputs, &file, &text), "E-BAD-ENUM"));
    assert_eq!(
        inputs.stats(Some(&root)).bases,
        2,
        "a changed recorded path"
    );
}

/// The entries a fresh cache builds for `texts` on `file`: the closure the
/// persistent cache must hold for the same configurations.
fn closure(root: &Path, file: &Path, texts: &[&String]) -> RetainedCounts {
    let cache = InputCache::tracked();
    cache.project(root);
    for text in texts {
        assemble_input_with_mode(
            &cache,
            file,
            (*text).clone(),
            None,
            Some(root),
            None,
            Mode::Author,
        );
    }
    cache.retained()
}

/// Configuration `i` of the churn: distinct `plugins:` maps, `uses:` lists
/// of existing imports and `uses:` lists naming missing imports.
fn churn_text(i: usize) -> String {
    let frontmatter = match i % 3 {
        0 => format!("plugins: {{ p{i}: {{}} }}\n"),
        1 => format!("uses: [../schema/s{i}.yaml]\n"),
        _ => format!("uses: [../schema/missing{i}.yaml]\n"),
    };
    scene(&frontmatter, "@narrator: Hi.")
}

#[test]
fn configuration_churn_retains_the_closure_of_live_and_four_superseded() {
    let (_dir, root) = project("lsp-churn", "");
    for i in 0..20 {
        write(
            &root.join(format!("schema/s{i}.yaml")),
            &format!("enums:\n  e{i}: [a]\n"),
        );
    }
    let file = root.join("scenes/a.lute");
    let texts: Vec<String> = (0..20).map(churn_text).collect();
    let inputs = ProjectInputs::default();
    let mut history: Vec<usize> = Vec::new();
    // The configurations retained after `history`: the last one live, then
    // the four before it.
    let retained = |history: &[usize]| -> Vec<usize> {
        let mut out: Vec<usize> = Vec::new();
        for &i in history.iter().rev() {
            if !out.contains(&i) {
                out.push(i);
            }
        }
        out.truncate(1 + SUPERSEDED_RETAINED);
        out
    };
    let mut held_at: Vec<RetainedCounts> = Vec::new();
    for i in 0..20 {
        codes(&inputs, &file, &texts[i]);
        history.push(i);
        let keep: Vec<&String> = retained(&history).iter().map(|&j| &texts[j]).collect();
        let held = inputs.retained(Some(&root)).unwrap();
        assert_eq!(held, closure(&root, &file, &keep), "after step {i}");
        assert!(inputs.retained_configurations(Some(&root)).len() <= 1 + SUPERSEDED_RETAINED);
        held_at.push(held);
    }
    // No growth past the fifth step: the configurations repeat their shape
    // every three steps, and so does what the cache holds.
    for i in 7..20 {
        assert_eq!(held_at[i], held_at[i - 3], "step {i}");
    }

    // Back to one of the last four: reused, nothing built.
    let before = inputs.stats(Some(&root));
    codes(&inputs, &file, &texts[17]);
    history.push(17);
    let after = inputs.stats(Some(&root));
    assert_eq!((after.bases, after.vocabs), (before.bases, before.vocabs));
    // Back to the sixth-last: rebuilt.
    codes(&inputs, &file, &texts[13]);
    history.push(13);
    let rebuilt = inputs.stats(Some(&root));
    assert!(rebuilt.vocabs > after.vocabs, "{rebuilt} after {after}");

    // An in-flight request pins a configuration the churn supersedes.
    let pinned = churn_text(30);
    let mut request = inputs.begin(Some(&root), None);
    let (built, _) = assemble_input_with_mode(
        request.cache(),
        &file,
        pinned.clone(),
        None,
        Some(&root),
        None,
        Mode::Author,
    );
    request.pin(InputCache::configuration(
        &file,
        None,
        Some(&root),
        None,
        &built,
    ));
    for i in 0..6 {
        codes(&inputs, &file, &texts[i]);
        history.push(i);
        let mut keep: Vec<&String> = retained(&history).iter().map(|&j| &texts[j]).collect();
        keep.push(&pinned);
        assert_eq!(
            inputs.retained(Some(&root)).unwrap(),
            closure(&root, &file, &keep),
            "the pinned configuration stays while its request runs"
        );
    }
    drop(request);
    codes(&inputs, &file, &texts[6]);
    history.push(6);
    let keep: Vec<&String> = retained(&history).iter().map(|&j| &texts[j]).collect();
    assert_eq!(
        inputs.retained(Some(&root)).unwrap(),
        closure(&root, &file, &keep),
        "released when its request ended"
    );
}

#[test]
fn closing_documents_supersedes_their_configurations_and_the_last_drops_the_root() {
    let (_dir, root) = project("lsp-close", "\n  beta:\n    plugins: {}\n");
    let files: Vec<PathBuf> = ["a", "b", "c"]
        .iter()
        .map(|n| root.join(format!("scenes/{n}.lute")))
        .collect();
    let texts = [
        scene("", "@narrator: Hi."),
        scene("profile: alt\n", "@narrator: Hi."),
        scene("plugins: { p9: {} }\n", "@narrator: Hi."),
    ];
    let inputs = ProjectInputs::default();
    for (file, text) in files.iter().zip(&texts) {
        codes(&inputs, file, text);
    }
    let all = |cache: &InputCache| {
        cache.project(&root);
        for (file, text) in files.iter().zip(&texts) {
            assemble_input_with_mode(
                cache,
                file,
                text.clone(),
                None,
                Some(&root),
                None,
                Mode::Author,
            );
        }
        cache.retained()
    };
    let three = all(&InputCache::tracked());
    assert_eq!(inputs.retained(Some(&root)).unwrap(), three);
    inputs.close(&files[0].display().to_string());
    inputs.close(&files[1].display().to_string());
    // One live and two superseded: still the closure of the three.
    assert_eq!(inputs.retained_configurations(Some(&root)).len(), 3);
    assert_eq!(inputs.retained(Some(&root)).unwrap(), three);
    inputs.close(&files[2].display().to_string());
    assert_eq!(
        inputs.retained(Some(&root)),
        None,
        "the root's cache is dropped"
    );
}
