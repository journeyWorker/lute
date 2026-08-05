//! End-to-end CLI tests: spawn the built `lute` binary and assert exit codes +
//! output. These pin the Task 5.1 acceptance contract — `check` exit `0`/`1`
//! from `CheckResult::ok`, `--json` serializes the result, and `catalog refresh`
//! → `check --providers` round-trips the on-disk provider snapshot format.

use std::path::PathBuf;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

/// A fresh unique temp dir (no `tempfile` dev-dep needed for these small tests).
fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-cli-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn check_clean_file_exits_zero_json() {
    let out = Command::new(BIN)
        .args(["check", "../../docs/examples/bianca-s01ep02.lute", "--json"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], true);
}

#[test]
fn check_json_has_resolved_view_and_diagnostics_array() {
    let out = Command::new(BIN)
        .args(["check", "../../docs/examples/bianca-s01ep02.lute", "--json"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        v["diagnostics"].is_array(),
        "diagnostics must serialize as an array"
    );
    // A clean document carries a resolved view (Some-vs-None policy).
    assert!(
        v["resolved"].is_object(),
        "clean doc → resolved is Some: {v}"
    );
    assert!(v["resolved"]["commands_preview"].is_array());
    assert!(v["resolved"]["timeline_tables"].is_array());
    assert!(v["resolved"]["injections"].is_array());
}

#[test]
fn check_file_with_errors_exits_one() {
    let out = Command::new(BIN)
        .args(["check", "../../docs/examples/idola-project/date-minigame.lute", "--json"])
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "a file with error diagnostics must exit non-zero"
    );
    assert_eq!(out.status.code(), Some(1));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], false);
}

#[test]
fn check_human_output_lists_diagnostics() {
    let out = Command::new(BIN)
        .args(["check", "../../docs/examples/idola-project/date-minigame.lute"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("E-UNKNOWN-DIRECTIVE"),
        "human output names codes: {stdout}"
    );
    assert!(
        stdout.contains("failed:"),
        "human summary reports failure: {stdout}"
    );
}

#[test]
fn check_missing_file_exits_two() {
    let out = Command::new(BIN)
        .args(["check", "/no/such/file.lute"])
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(2),
        "an I/O failure exits 2, distinct from a check failure"
    );
}

#[test]
fn check_with_empty_providers_dir_is_permissive() {
    // `--providers` on an empty dir yields an empty set → no provider-id errors;
    // the example uses no `providerRef` attrs, so it stays clean either way.
    let dir = temp_dir("empty-providers");
    let out = Command::new(BIN)
        .args([
            "check",
            "../../docs/examples/bianca-s01ep02.lute",
            "--providers",
        ])
        .arg(&dir)
        .arg("--json")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], true);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn catalog_refresh_then_load_round_trips() {
    let dir = temp_dir("refresh");
    // A stale snapshot with an old manifest stamp.
    std::fs::write(
        dir.join("core.yaml"),
        "manifestVersion: old-stamp\nproviderVersion: \"3\"\nstale: true\nentries:\n  character: [bianca]\n",
    )
    .unwrap();

    let out = Command::new(BIN)
        .arg("catalog")
        .arg("refresh")
        .arg(&dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // The rewritten file must still parse as a snapshot, with stale cleared and
    // the manifest re-stamped to the current capabilityVersion.
    let refreshed = std::fs::read_to_string(dir.join("core.yaml")).unwrap();
    let snap: serde_yaml::Value = serde_yaml::from_str(&refreshed).unwrap();
    assert_eq!(snap["stale"], serde_yaml::Value::Bool(false));
    assert_ne!(
        snap["manifestVersion"],
        serde_yaml::Value::String("old-stamp".into())
    );

    // And `ProviderSet::load` reads the refreshed dir back (the load consumer).
    let set = lute_manifest::provider::ProviderSet::load(&dir);
    assert_eq!(set.snapshots().len(), 1);
    use lute_manifest::provider::IdStatus;
    assert_eq!(set.contains("character", "bianca"), IdStatus::Fresh);
    assert_eq!(set.contains("character", "ghost"), IdStatus::Absent);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn catalog_refresh_missing_dir_is_created() {
    let base = temp_dir("refresh-missing");
    let target = base.join("brand/new");
    let out = Command::new(BIN)
        .arg("catalog")
        .arg("refresh")
        .arg(&target)
        .output()
        .unwrap();
    assert!(out.status.success(), "refresh creates a missing dir");
    assert!(target.is_dir(), "the target dir now exists");
    std::fs::remove_dir_all(&base).unwrap();
}

// --- 0.1.0 golden coverage: the showcase `hub-demo.lute` exercises a `<hub>`,
// `<when is="…">` literal arms, and `{{…}}` interpolation (dsl §7.3.2, §7.3.1,
// §7.6). A `<hub>` PASSES `lute check` AND (Plan C, IR A2) COMPILES to a `hub`
// record. These two tests pin both halves: a clean, feature-bearing check, and a
// successful compile whose artifact carries the hub record.

#[test]
fn hub_demo_example_checks_clean() {
    let out = Command::new(BIN)
        .args([
            "check",
            "../../docs/examples/showcase/hub-demo.lute",
            "--project",
            "../../docs/examples/showcase",
            "--json",
        ])
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "hub-demo must check clean; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], true, "hub-demo → ok:true; got {v}");
    assert_eq!(
        v["diagnostics"].as_array().map(Vec::len),
        Some(0),
        "hub-demo must be diagnostic-free (0 errors, 0 warnings); got {v}"
    );
    // Prove the 0.1.0 features are actually present in the resolved view — the
    // `<hub>` and both `<when is>`-bearing matches — not a trivially clean doc.
    let preview = v["resolved"]["commands_preview"].to_string();
    assert!(preview.contains("<hub>"), "resolved view must contain the hub; got {preview}");
    assert!(
        preview.contains("scene.choices.chatWithBianca"),
        "resolved view must contain the `<when is>` match over the hub's recorded choices; got {preview}"
    );
}

#[test]
fn hub_demo_example_compiles() {
    // Plan C: `<hub>` now LOWERS (IR A2), so hub-demo COMPILES — exit 0 with the
    // artifact on stdout, carrying a `hub` record for the revisit menu.
    let out = Command::new(BIN)
        .args([
            "compile",
            "../../docs/examples/showcase/hub-demo.lute",
            "--project",
            "../../docs/examples/showcase",
        ])
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "hub compile succeeds → exit 0; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let artifact: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let hub = artifact["commands"]
        .as_array()
        .expect("commands array")
        .iter()
        .find(|c| c["kind"] == "hub")
        .expect("a `hub` record in the compiled artifact");
    assert_eq!(hub["id"], "chatWithBianca");
    assert_eq!(hub["recordKey"], "scene.choices.chatWithBianca");
}

// --- 0.1.0 golden coverage: the NON-HUB companion `when-is-demo.lute` exercises
// `<when is="…">` literal-pattern arms (dsl §7.3.1) — including an `is="a|b"`
// alternation — over a PLAIN scene-local finite enum (`scene.mood`), not a hub's
// implicit recording enums. A default-valued enum is definitely assigned, so full
// `is` coverage is exhaustive with NO `<otherwise>` (§11.2). This pins a clean,
// feature-bearing check for that path.

#[test]
fn when_is_demo_example_checks_clean() {
    let out = Command::new(BIN)
        .args([
            "check",
            "../../docs/examples/showcase/when-is-demo.lute",
            "--project",
            "../../docs/examples/showcase",
            "--json",
        ])
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "when-is-demo must check clean; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], true, "when-is-demo → ok:true; got {v}");
    assert_eq!(
        v["diagnostics"].as_array().map(Vec::len),
        Some(0),
        "when-is-demo must be diagnostic-free (0 errors, 0 warnings); got {v}"
    );
    // Prove the `<when is>` feature is actually present in the resolved view — the
    // `<match>` over the plain scene enum — not a trivially clean doc.
    let preview = v["resolved"]["commands_preview"].to_string();
    assert!(
        preview.contains("<match"),
        "resolved view must contain the match; got {preview}"
    );
    assert!(
        preview.contains("scene.mood"),
        "resolved view must contain the `<when is>` match over the plain scene enum; got {preview}"
    );
}

// --- `lute context`: the project-resolved AUTHORING SURFACE an AI needs to
// write valid Lute against THIS file's project (Task D4). Reuses the SAME
// build_input/fold_env resolution check/compile use (no divergence); it is a
// capability query, NOT validation, so it emits the surface regardless of
// document diagnostics. exit 0 on success / 2 on an I/O failure.

#[test]
fn context_surface_has_plugin_and_core_directives() {
    // With `--project`, the resolved snapshot activates the showcase plugin, so
    // the surface carries the plugin `serve` directive (with its attrs +
    // semantics) alongside the core directives, a non-empty enum map, folded
    // `scene.*` state paths, and the resolved capabilityVersion.
    let out = Command::new(BIN)
        .args([
            "context",
            "../../docs/examples/showcase/episode01.lute",
            "--project",
            "../../docs/examples/showcase",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();

    let ver = v["capabilityVersion"]
        .as_str()
        .expect("capabilityVersion is a string");
    assert!(!ver.is_empty(), "resolved capabilityVersion is non-empty: {v}");

    let dirs = v["directives"].as_array().expect("directives array");
    let names: Vec<&str> = dirs.iter().filter_map(|d| d["name"].as_str()).collect();
    assert!(
        names.contains(&"serve"),
        "plugin `serve` directive is present: {names:?}"
    );
    for core in ["bg", "music", "camera"] {
        assert!(
            names.contains(&core),
            "core directive `{core}` is present: {names:?}"
        );
    }

    let serve = dirs
        .iter()
        .find(|d| d["name"] == "serve")
        .expect("the serve directive view");
    let serve_attrs: Vec<&str> = serve["attrs"]
        .as_array()
        .expect("serve attrs array")
        .iter()
        .filter_map(|a| a["name"].as_str())
        .collect();
    assert!(
        serve_attrs.contains(&"resultKey"),
        "serve carries its resultKey attr: {serve_attrs:?}"
    );
    let serve_semantics: Vec<&str> = serve["semantics"]
        .as_array()
        .expect("serve semantics array")
        .iter()
        .filter_map(|s| s.as_str())
        .collect();
    assert!(
        serve_semantics.contains(&"bridgeCall"),
        "serve semantics carry `bridgeCall`: {serve_semantics:?}"
    );

    assert!(
        v["enums"].as_object().is_some_and(|o| !o.is_empty()),
        "enum map is non-empty: {v}"
    );

    let paths: Vec<&str> = v["stateSchema"]
        .as_array()
        .expect("stateSchema array")
        .iter()
        .filter_map(|s| s["path"].as_str())
        .collect();
    assert!(
        paths.iter().any(|p| p.starts_with("scene.")),
        "a folded `scene.*` state path is present: {paths:?}"
    );
}

#[test]
fn context_core_only_has_eight_core_directives() {
    // No `--project` → the core-only `lute.core` snapshot: exactly the 8 baseline
    // directives, no plugin `serve`, and the core capabilityVersion.
    let out = Command::new(BIN)
        .args(["context", "../../docs/examples/bianca-s01ep02.lute", "--json"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let names: Vec<&str> = v["directives"]
        .as_array()
        .expect("directives array")
        .iter()
        .filter_map(|d| d["name"].as_str())
        .collect();
    for core in ["bg", "music", "sfx", "auto", "vfx", "cut", "video", "camera"] {
        assert!(
            names.contains(&core),
            "core directive `{core}` is present: {names:?}"
        );
    }
    assert!(
        !names.contains(&"serve"),
        "core-only surface excludes the plugin `serve` directive: {names:?}"
    );
    assert!(
        !v["capabilityVersion"]
            .as_str()
            .expect("capabilityVersion string")
            .is_empty(),
        "core capabilityVersion is non-empty: {v}"
    );
}

#[test]
fn context_missing_file_exits_two() {
    let out = Command::new(BIN)
        .args(["context", "/no/such/file.lute", "--json"])
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(2),
        "an unreadable file exits 2 (I/O), matching run_check"
    );
}

#[test]
fn context_choice_slot_domain_includes_unset() {
    // A REAL implicit `scene.choices.<hubId|branchId>` slot's authorable domain is
    // choice ids ∪ `unset` — the author must write `<when is="unset">` for the
    // pre-choice state — so `lute context` MUST carry `unset` LAST (members then
    // unset), byte-identical to compile's/check's implicit-slot domain (no
    // divergence). An author-declared enum at any OTHER path keeps its declared
    // members (no spurious `unset`). `hub-demo` folds the hub `chatWithBianca` into
    // one implicit choice slot AND declares plain enums (`run.sofaOutcome`, …).
    let out = Command::new(BIN)
        .args([
            "context",
            "../../docs/examples/showcase/hub-demo.lute",
            "--project",
            "../../docs/examples/showcase",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let schema = v["stateSchema"].as_array().expect("stateSchema array");
    let domain_of = |path: &str| -> Vec<String> {
        let entry = schema
            .iter()
            .find(|e| e["path"] == path)
            .unwrap_or_else(|| panic!("stateSchema entry for {path}; got {v}"));
        entry["domain"]
            .as_array()
            .unwrap_or_else(|| panic!("domain array for {path}; got {entry}"))
            .iter()
            .filter_map(|x| x.as_str().map(str::to_string))
            .collect()
    };

    // The implicit hub choice slot: choice ids ∪ `unset`, `unset` LAST — matching
    // the compiled artifact's `state` table domain for the same path.
    assert_eq!(
        domain_of("scene.choices.chatWithBianca"),
        vec!["askCoffee", "compliment", "leave", "unset"],
        "implicit choice-slot domain is choice ids ∪ unset (matching compile): {v}"
    );

    // A plain author enum (NOT a branch/hub slot) keeps ONLY its declared members.
    let author = domain_of("run.sofaOutcome");
    assert_eq!(
        author,
        vec!["warm", "cold"],
        "author enum keeps its declared members, no spurious unset: {v}"
    );
    assert!(
        !domain_of("app.lang").contains(&"unset".to_string())
            && !domain_of("app.rating").contains(&"unset".to_string()),
        "author enums must never gain an implicit-slot unset: {v}"
    );
}

#[test]
fn context_json_surfaces_relational_vocabulary() {
    // spec §5: `lute context --json` MUST surface the relational vocabulary
    // `fold_env` already merges (`RelVocab`) — entity kinds, relations (name +
    // arity + argument domains + `derive`), seed facts, rules, and the
    // project-level `enums:` (kept under its OWN `projectEnums` key so it
    // never clobbers the plugin/core `enums` map).
    let dir = temp_dir("context-rel-vocab");
    let f = dir.join("scene.lute");
    std::fs::write(
        &f,
        "---\n\
         character: x\n\
         season: 1\n\
         episode: 1\n\
         enums:\n\
         \x20 emotion: [neutral, surprised, delighted, worried]\n\
         entities:\n\
         \x20 npc: { members: [ana, bo] }\n\
         relations:\n\
         \x20 friend: { args: [npc, npc] }\n\
         \x20 allied: { args: [npc, npc], derive: true }\n\
         facts:\n\
         \x20 - \"friend(ana, bo)\"\n\
         rules:\n\
         \x20 - \"allied(A, B) :- friend(A, B)\"\n\
         ---\n\
         ## Shot 1.\n\
         Hello there.\n",
    )
    .unwrap();

    let out = Command::new(BIN)
        .args(["context", f.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();

    // Entities: the declared `npc` kind, closed with its two members.
    let entities = v["entities"].as_array().expect("entities array");
    assert!(!entities.is_empty(), "entities is non-empty: {v}");
    let npc = entities
        .iter()
        .find(|e| e["name"] == "npc")
        .unwrap_or_else(|| panic!("npc entity kind present: {v}"));
    assert_eq!(npc["shape"], "members");
    let npc_members: Vec<&str> = npc["members"]
        .as_array()
        .expect("npc members array")
        .iter()
        .filter_map(|x| x.as_str())
        .collect();
    assert_eq!(npc_members, vec!["ana", "bo"], "npc member set: {v}");

    // Relations: `friend` (base) + `allied` (derive: true), both arity 2 over
    // the `npc` domain twice.
    let relations = v["relations"].as_array().expect("relations array");
    assert!(!relations.is_empty(), "relations is non-empty: {v}");
    let friend = relations
        .iter()
        .find(|r| r["name"] == "friend")
        .unwrap_or_else(|| panic!("friend relation present: {v}"));
    assert_eq!(friend["arity"], 2, "friend arity: {v}");
    assert_eq!(
        friend["args"],
        serde_json::json!(["npc", "npc"]),
        "friend argument domains: {v}"
    );
    assert_eq!(friend["derive"], false, "friend is not derive: {v}");
    let allied = relations
        .iter()
        .find(|r| r["name"] == "allied")
        .unwrap_or_else(|| panic!("allied relation present: {v}"));
    assert_eq!(allied["arity"], 2, "allied arity: {v}");
    assert_eq!(allied["derive"], true, "allied IS derive: {v}");

    // Seed facts + rules: raw source text, non-empty.
    let facts: Vec<&str> = v["facts"]
        .as_array()
        .expect("facts array")
        .iter()
        .filter_map(|x| x.as_str())
        .collect();
    assert!(
        facts.contains(&"friend(ana, bo)"),
        "seed fact is surfaced verbatim: {v}"
    );
    let rules: Vec<&str> = v["rules"]
        .as_array()
        .expect("rules array")
        .iter()
        .filter_map(|x| x.as_str())
        .collect();
    assert!(
        rules.contains(&"allied(A, B) :- friend(A, B)"),
        "rule is surfaced verbatim: {v}"
    );

    // Project-level `enums:` land under `projectEnums`, distinct from the
    // plugin/core `enums` key (which still exists and is untouched).
    assert!(v["enums"].is_object(), "capability enums key still present: {v}");
    let project_emotion = v["projectEnums"]["emotion"]
        .as_array()
        .unwrap_or_else(|| panic!("projectEnums.emotion array: {v}"))
        .iter()
        .filter_map(|x| x.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        project_emotion,
        vec!["neutral", "surprised", "delighted", "worried"],
        "project enum members: {v}"
    );
}

#[test]
fn context_human_output_shows_enum_members_and_relational_vocabulary() {
    // spec §5: the HUMAN (non-`--json`) output MUST list enum MEMBERS (not
    // just names) and the relational vocabulary (entity kinds, relations with
    // arity, seed facts, rules) — an author should not need `--json` for
    // either.
    let dir = temp_dir("context-rel-vocab-human");
    let f = dir.join("scene.lute");
    std::fs::write(
        &f,
        "---\n\
         character: x\n\
         season: 1\n\
         episode: 1\n\
         enums:\n\
         \x20 emotion: [neutral, surprised, delighted, worried]\n\
         entities:\n\
         \x20 npc: { members: [ana, bo] }\n\
         relations:\n\
         \x20 friend: { args: [npc, npc] }\n\
         \x20 allied: { args: [npc, npc], derive: true }\n\
         facts:\n\
         \x20 - \"friend(ana, bo)\"\n\
         rules:\n\
         \x20 - \"allied(A, B) :- friend(A, B)\"\n\
         ---\n\
         ## Shot 1.\n\
         Hello there.\n",
    )
    .unwrap();

    let out = Command::new(BIN)
        .args(["context", f.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);

    // Enum MEMBERS, not just the enum name, appear in the human output.
    assert!(
        text.contains("neutral") && text.contains("surprised") && text.contains("delighted"),
        "enum members appear in human output: {text}"
    );

    // Relational vocabulary sections: entity kinds, relations w/ arity, seed
    // facts, rules.
    assert!(
        text.contains("npc") && text.contains("ana") && text.contains("bo"),
        "entity kind + members appear: {text}"
    );
    assert!(
        text.contains("friend/2") || text.contains("friend("),
        "relation with arity appears: {text}"
    );
    assert!(
        text.contains("allied") && text.contains("derive"),
        "derive:true relation is flagged: {text}"
    );
    assert!(
        text.contains("friend(ana, bo)"),
        "seed fact appears verbatim: {text}"
    );
    assert!(
        text.contains("allied(A, B) :- friend(A, B)"),
        "rule appears verbatim: {text}"
    );
}

#[test]
fn context_json_lists_referenced_reserved_quest_paths() {
    // dsl 0.5.1 §2: `lute context --json` MUST list the reserved
    // `quest.<id>.state` / `quest.<id>.objectives.<oid>.done` paths the
    // document actually REFERENCES (via a CEL slot), alongside the ordinary
    // `stateSchema`, with their domain — like other stateSchema entries.
    let dir = temp_dir("context-reserved-quest-refs");
    let f = dir.join("scene.lute");
    std::fs::write(
        &f,
        "---\n\
         character: x\n\
         season: 1\n\
         episode: 1\n\
         ---\n\
         ## Shot 1.\n\
         <match on=\"quest.foo.state\">\n\
         <when is=\"active\" test=\"quest.foo.objectives.bar.done\">\n\
         @x: a\n\
         </when>\n\
         <otherwise>\n\
         @x: b\n\
         </otherwise>\n\
         </match>\n",
    )
    .unwrap();

    let out = Command::new(BIN)
        .args(["context", f.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let reserved = v["reservedQuestPaths"]
        .as_array()
        .expect("reservedQuestPaths array");
    assert_eq!(reserved.len(), 2, "{v}");

    let state = reserved
        .iter()
        .find(|p| p["path"] == "quest.foo.state")
        .unwrap_or_else(|| panic!("quest.foo.state entry present: {v}"));
    assert_eq!(state["type"], "enum", "{state}");
    let state_domain: Vec<&str> = state["domain"]
        .as_array()
        .expect("state domain array")
        .iter()
        .filter_map(|x| x.as_str())
        .collect();
    assert_eq!(
        state_domain,
        vec!["active", "complete", "failed", "unset"],
        "quest.<id>.state domain: {v}"
    );

    let done = reserved
        .iter()
        .find(|p| p["path"] == "quest.foo.objectives.bar.done")
        .unwrap_or_else(|| panic!("quest.foo.objectives.bar.done entry present: {v}"));
    assert_eq!(done["type"], "bool", "{done}");
}

#[test]
fn context_json_omits_unreferenced_reserved_quest_paths() {
    // A document that never reads a reserved quest path lists none — the
    // reserved namespace is unbounded, so absence, not exhaustive listing.
    let out = Command::new(BIN)
        .args(["context", "../../docs/examples/bianca-s01ep02.lute", "--json"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let reserved = v["reservedQuestPaths"]
        .as_array()
        .expect("reservedQuestPaths array");
    assert!(reserved.is_empty(), "{v}");
}

#[test]
fn context_human_shows_referenced_reserved_quest_paths() {
    let dir = temp_dir("context-reserved-quest-refs-human");
    let f = dir.join("scene.lute");
    std::fs::write(
        &f,
        "---\n\
         character: x\n\
         season: 1\n\
         episode: 1\n\
         ---\n\
         ## Shot 1.\n\
         <match on=\"quest.foo.state\">\n\
         <when is=\"active\" test=\"quest.foo.objectives.bar.done\">\n\
         @x: a\n\
         </when>\n\
         <otherwise>\n\
         @x: b\n\
         </otherwise>\n\
         </match>\n",
    )
    .unwrap();

    let out = Command::new(BIN)
        .args(["context", f.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("quest.foo.state") && text.contains("quest.foo.objectives.bar.done"),
        "human output shows the referenced reserved quest paths: {text}"
    );
}

#[test]
fn context_json_lists_delivery_flags() {
    // dsl 0.5.1 §3: the fixed `{mono}`/`{os}`/`{vo}` delivery flags are
    // surfaced in the authoring surface, human + `--json`.
    let out = Command::new(BIN)
        .args(["context", "../../docs/examples/bianca-s01ep02.lute", "--json"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let flags = v["deliveryFlags"].as_array().expect("deliveryFlags array");
    let names: Vec<&str> = flags.iter().filter_map(|f| f["flag"].as_str()).collect();
    assert_eq!(names, vec!["mono", "os", "vo"], "{v}");
    for f in flags {
        assert!(
            f["meaning"].as_str().is_some_and(|m| !m.is_empty()),
            "delivery flag carries a non-empty meaning: {f}"
        );
    }
}

#[test]
fn context_human_lists_delivery_flags() {
    let out = Command::new(BIN)
        .args(["context", "../../docs/examples/bianca-s01ep02.lute"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("{mono}") && text.contains("{os}") && text.contains("{vo}"),
        "human output lists all three delivery flags: {text}"
    );
}

// --- `lute fix`: the pre-0.2.2 migration codemod (Task C3/D5). Rewrites the
// file in place — `:line[speaker]{…}: text` → `@speaker{…}: text`, any other
// content line's leading `:` sigil → `@` (phase 1, parser migrate fix-its),
// AND `<choice>`/`<hub>` choice `as="…"` → `into="…"` (phase 2, AST walk).
// Exit 0; re-running is an idempotent no-op.

#[test]
fn fix_migrates_line_and_choice_as_in_place_idempotent() {
    let dir = temp_dir("fix");
    let f = dir.join("scene.lute");
    let before = "---\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n:line[bianca]{emotion=\"x\"}: hi\n<branch id=\"b\">\n<choice id=\"c\" label=\"L\" as=\"run.flag\">\n:fixer: yo\n</choice>\n</branch>\n";
    std::fs::write(&f, before).unwrap();

    let out = Command::new(BIN)
        .args(["fix", f.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let after = std::fs::read_to_string(&f).unwrap();
    let expected = "---\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n@bianca{emotion=\"x\"}: hi\n<branch id=\"b\">\n<choice id=\"c\" label=\"L\" into=\"run.flag\">\n@fixer: yo\n</choice>\n</branch>\n";
    assert_eq!(after, expected, "both phases must migrate in place");

    // Idempotent: a second run rewrites nothing (file byte-identical).
    let out2 = Command::new(BIN)
        .args(["fix", f.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out2.status.success());
    assert_eq!(
        std::fs::read_to_string(&f).unwrap(),
        expected,
        "second fix run must be a no-op"
    );
}

// --- dsl 0.9.0 D-F: vocabulary OWNERSHIP. The compiler declares the seven
// slots and ships no members, so the opinionated starter set lives in the
// TEMPLATE (`lute init`) — a file the author owns and edits — and `lute
// doctor` reports which slots a project actually resolves, so a missing one
// surfaces before an author hits `E-DOMAIN-UNKNOWN`.

/// `lute init` must produce a project that checks clean out of the box, which
/// is only possible if it declares a vocabulary AND the scaffolded scene can
/// REACH it: the starter scene uses an `emotion=` attr, so an unreachable
/// (un-`uses:`-ed) declaration fails this with `E-DOMAIN-UNKNOWN`.
#[test]
fn init_scaffolds_a_checkable_vocabulary() {
    let dir = temp_dir("init-vocab");
    let proj = dir.join("proj");
    let out = Command::new(BIN)
        .args(["init", proj.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success(), "init failed: {out:?}");
    assert!(
        proj.join("vocabulary.schema.yaml").is_file(),
        "init must scaffold a vocabulary declaration"
    );
    let scene = std::fs::read_to_string(proj.join("scenes/opening.lute")).unwrap();
    assert!(
        scene.contains("emotion="),
        "the starter scene must USE a vocabulary slot, or `check-project` below \
         would pass without ever resolving the declaration:\n{scene}"
    );
    let check = Command::new(BIN)
        .args(["check-project", proj.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        check.status.success(),
        "scaffolded project must check clean:\n{}{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );
}

/// `lute doctor` reports the seven vocabulary slots against what the project
/// actually resolves: every slot declared for a fresh scaffold, and every slot
/// missing once the declaration is deleted.
#[test]
fn doctor_reports_which_vocabulary_slots_resolve() {
    const SLOTS: &[&str] = &[
        "emotion",
        "action",
        "anchor",
        "mood",
        "volume",
        "musicAction",
        "vfxType",
    ];
    let dir = temp_dir("doctor-vocab");
    let proj = dir.join("proj");
    assert!(Command::new(BIN)
        .args(["init", proj.to_str().unwrap()])
        .output()
        .unwrap()
        .status
        .success());

    let out = Command::new(BIN)
        .args(["doctor", proj.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let declared = text
        .lines()
        .find(|l| l.contains("vocabulary slots declared"))
        .unwrap_or_else(|| panic!("doctor must report declared slots:\n{text}"))
        .to_string();
    for slot in SLOTS {
        assert!(
            declared.contains(slot),
            "the scaffolded vocabulary declares `{slot}`:\n{declared}"
        );
    }
    assert!(
        !text.contains("not declared"),
        "a fresh scaffold leaves no slot undeclared:\n{text}"
    );

    // Delete the declaration: the SAME report must now name every slot as
    // undeclared, so it tracks the project rather than printing a constant.
    std::fs::remove_file(proj.join("vocabulary.schema.yaml")).unwrap();
    let out = Command::new(BIN)
        .args(["doctor", proj.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let missing = text
        .lines()
        .find(|l| l.contains("not declared"))
        .unwrap_or_else(|| panic!("doctor must report undeclared slots:\n{text}"))
        .to_string();
    for slot in SLOTS {
        assert!(
            missing.contains(slot),
            "`{slot}` is undeclared once the vocabulary is gone:\n{missing}"
        );
    }
    assert!(
        text.contains("vocabulary slots declared: none"),
        "no slot resolves without the declaration:\n{text}"
    );
}

/// Write `rel` under `dir`, creating parents. The vocabulary/doctor fixtures
/// below are nested plugin trees, so every one of them needs this.
fn write_at(dir: &std::path::Path, rel: &str, content: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

/// `doctor <dir>` and `check-project <dir>` MUST resolve the same project root
/// for the same target, or `doctor` reports a vocabulary the checker rejects.
///
/// The fixture is the asymmetry itself: `proj/lute.project.yaml` activates a
/// plugin whose `enums` export declares `musicAction`, `proj/scenes/` carries NO
/// manifest, and `proj/scenes/a.lute` writes `::music{action="start"}`.
/// `check-project proj/scenes` cannot ascend above the requested dir, loads no
/// project, and reports `E-DOMAIN-UNKNOWN`. A `doctor` that walks up to `proj`
/// would call the same slot declared.
///
/// A plugin `enums` export is the ONLY root-dependent domain source: a project
/// schema's `enums:` reaches a scene through its own `uses:`, resolved relative
/// to the scene's directory and so independent of the project root. Only plugin
/// activation flows through `lute.project.yaml`.
#[test]
fn doctor_and_check_project_agree_on_the_project_root_boundary() {
    let dir = temp_dir("doctor-root-boundary");
    let proj = dir.join("proj");
    write_at(
        &proj,
        "lute.project.yaml",
        "pluginsDir: plugins/\ndefaultProfile: demo\nprofiles:\n  demo:\n    plugins: { demo.vocab: true }\n",
    );
    write_at(
        &proj,
        "plugins/demo.vocab/plugin.yaml",
        "id: demo.vocab\nversion: 0.1.0\nkind: capability\ndepends: [ { id: lute.core, range: \"^0.0.1\" } ]\nexports:\n  enums: enums/\n",
    );
    write_at(
        &proj,
        "plugins/demo.vocab/enums/vocab.yaml",
        "enums:\n  musicAction: [start, stop]\n",
    );
    write_at(
        &proj,
        "scenes/a.lute",
        "---\nkind: scene\ncharacter: narrator\nseason: 1\nepisode: 1\n---\n\n## Shot 1.\n\n::music{action=\"start\"}\n@narrator: hello\n",
    );
    let scenes = proj.join("scenes");

    let check = Command::new(BIN)
        .args(["check-project", scenes.to_str().unwrap()])
        .output()
        .unwrap();
    let check_text = format!(
        "{}{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );
    assert!(
        check_text.contains("E-DOMAIN-UNKNOWN") && check_text.contains("musicAction"),
        "fixture premise: the checker cannot ascend above the requested dir, so \
         `musicAction` is undeclared for it:\n{check_text}"
    );

    let out = Command::new(BIN)
        .args(["doctor", scenes.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let declared = text
        .lines()
        .find(|l| l.contains("vocabulary slots declared"))
        .unwrap_or_else(|| panic!("doctor must report declared slots:\n{text}"))
        .to_string();
    assert!(
        !declared.contains("musicAction"),
        "doctor called `musicAction` declared while `check-project` on the SAME \
         dir reports E-DOMAIN-UNKNOWN — doctor ascended above the requested \
         directory:\n{declared}"
    );
    let missing = text
        .lines()
        .find(|l| l.contains("not declared"))
        .unwrap_or_else(|| panic!("doctor must report undeclared slots:\n{text}"))
        .to_string();
    assert!(
        missing.contains("musicAction"),
        "doctor must agree with the checker's verdict for this dir:\n{missing}"
    );
}

/// A project-resolution failure is doctor's REASON TO EXIST, so it must ride the
/// `Check` model: once (not once per document), in the human checklist AND under
/// a stable `--json` key, with NOTHING on stderr — stderr plus exit `2` stays
/// reserved for an unreadable target.
#[test]
fn doctor_reports_a_broken_project_manifest_once_as_a_check() {
    let dir = temp_dir("doctor-broken-project");
    let proj = dir.join("proj");
    write_at(&proj, "lute.project.yaml", "defaultProfile: [not, a, string]\n");
    write_at(
        &proj,
        "scenes/a.lute",
        "---\nkind: scene\ncharacter: narrator\nseason: 1\nepisode: 1\n---\n\n## Shot 1.\n\n@narrator: a\n",
    );
    write_at(
        &proj,
        "scenes/b.lute",
        "---\nkind: scene\ncharacter: narrator\nseason: 1\nepisode: 2\n---\n\n## Shot 1.\n\n@narrator: b\n",
    );

    // Two documents resolve the same broken project: the per-file loop used to
    // print the identical message twice, straight to stderr.
    let out = Command::new(BIN)
        .args(["doctor", proj.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success(), "doctor reports, never gates: {out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        stderr.is_empty(),
        "a reportable project problem belongs in the checklist, not on stderr:\n{stderr}"
    );
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let hits = text.matches("invalid type: sequence").count();
    assert_eq!(
        hits, 1,
        "the project problem must appear ONCE regardless of document count:\n{text}"
    );

    let out = Command::new(BIN)
        .args(["doctor", proj.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).is_empty(),
        "`--json` must be self-contained too: {out:?}"
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let entry = &v["checks"]["projectResolution"];
    assert_eq!(entry["ok"], false, "a broken project is a ✗: {v}");
    let detail = entry["detail"].as_str().unwrap_or_default();
    assert_eq!(
        detail.matches("invalid type: sequence").count(),
        1,
        "deduplicated in `--json` as well: {v}"
    );
    assert!(
        entry["hint"].is_string(),
        "a ✗ carries a remedy hint: {v}"
    );
}

/// #25 / T9.11: a refused test held a diagnostic vector and printed four
/// words over it. Three different mock faults produced the identical line;
/// the harness even INSPECTED the codes to choose between two canned
/// strings, then discarded them.
#[test]
fn refused_test_prints_the_held_diagnostics_not_a_canned_string() {
    let dir = temp_dir("test-refusal-vector");
    write_at(
        &dir,
        "s.lute",
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n\
         \n## One\n\n<branch id=\"pick\">\n<choice id=\"a\" label=\"A\">\n@narrator: a.\n\
         </choice>\n<choice id=\"b\" label=\"B\">\n@narrator: b.\n</choice>\n</branch>\n",
    );
    write_at(
        &dir,
        "stale.test.yaml",
        "file: s.lute\nchoose:\n  pick: noSuchArm\nexpect:\n  exit: complete\n",
    );
    let out = Command::new(BIN).args(["test", dir.to_str().unwrap()]).output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout).to_string();

    assert!(text.contains("E-TRACE-CHOICE"), "the held code must print: {text}");
    assert!(text.contains("noSuchArm"), "the offending value must print: {text}");
    assert!(
        !text.contains("invalid mock input"),
        "the canned string is what this issue deletes: {text}"
    );
    // T9.11's second half: the diagnostic renders a CLI flag spelling for
    // input that came from a YAML key, so even printed it points at the
    // wrong syntax.
    assert!(!text.contains("--choose"), "render the YAML key, not the flag: {text}");
    assert!(text.contains("choose:"), "the YAML key must appear: {text}");
}

/// #2(a) / T9.8, T3.10: `expect:` knew three keys and silently discarded
/// every other, at BOTH levels. Reproduced at HEAD: a file spelling
/// `chooses:` and asserting a bogus `transcriptContain:` returned
/// "1 passed, 0 failed" — the selection was dropped, trace auto-picked the
/// first eligible arm, and the assertions written for the arm the file names
/// were checked against the arm it excluded.
#[test]
fn test_file_with_a_typoed_top_level_key_fails_and_suggests_the_real_one() {
    let dir = temp_dir("test-closed-keys-top");
    write_at(
        &dir,
        "s.lute",
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n\
         \n## One\n\n<branch id=\"pick\">\n<choice id=\"a\" label=\"A\">\n@narrator: a.\n\
         </choice>\n<choice id=\"b\" label=\"B\">\n@narrator: b.\n</choice>\n</branch>\n",
    );
    write_at(
        &dir,
        "typo.test.yaml",
        "file: s.lute\nchooses:\n  pick: b\nexpect:\n  transcriptContains: [\"b.\"]\n",
    );
    let out = Command::new(BIN).args(["test", dir.to_str().unwrap()]).output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert_eq!(out.status.code(), Some(1), "a mis-keyed test must not pass: {text}");
    assert!(text.contains("E-TEST-KEY"), "{text}");
    assert!(text.contains("`chooses`"), "{text}");
    assert!(text.contains("did you mean `choose`"), "{text}");
}

/// The same closure at the `expect:` level. T9.8's own control: the
/// one-letter-corrected twin must still FAIL on the assertion, which is what
/// proves the typo was the only reason it was green.
#[test]
fn test_file_with_a_typoed_expect_key_fails_and_its_corrected_twin_still_fails() {
    let dir = temp_dir("test-closed-keys-expect");
    write_at(
        &dir,
        "s.lute",
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n\n## One\n\n@narrator: a.\n",
    );
    write_at(&dir, "typo.test.yaml", "file: s.lute\nexpect:\n  transcriptContain: [\"nope\"]\n");
    let out = Command::new(BIN).args(["test", dir.to_str().unwrap()]).output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert_eq!(out.status.code(), Some(1), "{text}");
    assert!(text.contains("E-TEST-KEY"), "{text}");
    assert!(text.contains("did you mean `transcriptContains`"), "{text}");

    let dir2 = temp_dir("test-closed-keys-expect-fixed");
    write_at(
        &dir2,
        "s.lute",
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n\n## One\n\n@narrator: a.\n",
    );
    write_at(&dir2, "fixed.test.yaml", "file: s.lute\nexpect:\n  transcriptContains: [\"nope\"]\n");
    let out2 = Command::new(BIN).args(["test", dir2.to_str().unwrap()]).output().unwrap();
    let text2 = String::from_utf8_lossy(&out2.stdout).to_string();
    assert_eq!(
        out2.status.code(),
        Some(1),
        "the corrected twin must fail on the ASSERTION: {text2}"
    );
    assert!(!text2.contains("E-TEST-KEY"), "{text2}");
    assert!(text2.contains("transcriptContains"), "{text2}");
}

/// #2(d), first half: a test with zero RECOGNISED expectations reported PASS,
/// because the verdict is `all()` over an empty vector.
#[test]
fn test_with_zero_expectations_is_an_error_not_a_pass() {
    let dir = temp_dir("test-no-expect");
    write_at(
        &dir,
        "s.lute",
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n\n## One\n\n@narrator: a.\n",
    );
    write_at(&dir, "empty.test.yaml", "file: s.lute\nexpect: {}\n");
    write_at(&dir, "absent.test.yaml", "file: s.lute\n");
    let out = Command::new(BIN).args(["test", dir.to_str().unwrap()]).output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert_eq!(out.status.code(), Some(1), "{text}");
    assert_eq!(
        text.matches("E-TEST-NO-EXPECT").count(),
        2,
        "both files, not just one: {text}"
    );
}

/// #2(d), second half: when no supplied selection survives, the walk
/// auto-picks the first eligible arm in document order. That is legal and
/// deliberate — but silent, and it is exactly what turned the `chooses:` typo
/// into a green run against the wrong arm. Report it.
#[test]
fn autopicked_branch_is_reported_not_silent() {
    let dir = temp_dir("test-autopick");
    write_at(
        &dir,
        "s.lute",
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n\
         \n## One\n\n<branch id=\"pick\">\n<choice id=\"a\" label=\"A\">\n@narrator: a.\n\
         </choice>\n<choice id=\"b\" label=\"B\">\n@narrator: b.\n</choice>\n</branch>\n",
    );
    write_at(&dir, "auto.test.yaml", "file: s.lute\nexpect:\n  exit: complete\n");
    let out = Command::new(BIN).args(["test", dir.to_str().unwrap()]).output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert_eq!(out.status.code(), Some(0), "auto-picking is legal, not a failure: {text}");
    assert!(text.contains("auto-picked"), "{text}");
    assert!(text.contains("pick"), "the branch id must be named: {text}");
    assert!(text.contains("-> a"), "the arm actually taken must be named: {text}");
}

/// A `*.test.yaml` and a `mocks/*.yaml` both key their subject on `file:`,
/// and both resolve it relative to THE FILE THAT CONTAINS IT (D-AC, D-Y).
/// One key, one meaning, one base rule — pinned here so the two cannot drift
/// into two once plan 02's mock work lands.
#[test]
fn test_file_key_resolves_relative_to_the_test_file_not_the_cwd() {
    let dir = temp_dir("test-file-base");
    write_at(
        &dir,
        "scenes/s.lute",
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n\n## One\n\n@narrator: a.\n",
    );
    write_at(&dir, "tests/t.test.yaml", "file: ../scenes/s.lute\nexpect:\n  exit: complete\n");
    let out = Command::new(BIN).args(["test", dir.to_str().unwrap()]).output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert_eq!(out.status.code(), Some(0), "{text}");
    assert!(text.contains("1 passed, 0 failed"), "{text}");
}
