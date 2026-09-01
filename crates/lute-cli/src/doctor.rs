//! `lute doctor` — local toolchain + project setup diagnosis.
//!
//! A REPORT, never a gate: `doctor` prints a checklist of the local Lute setup
//! and always exits `0` — unless the target directory itself is unreadable
//! (exit `2`). Each check is a `✓`/`✗` line; a `✗` carries a remedy hint. The
//! `--json` variant emits the same checks as a stable-keyed object.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lute_manifest::provider::ProviderSet;
use lute_manifest::snapshot::Domain;
use lute_manifest::validate::{SLOT_REQUIRES_DEFAULT, SLOT_REQUIRES_EXITS};

/// The seven vocabulary slots the language declares (dsl 0.9.0 D-A): six typed
/// by `lute.core`'s staging directives, plus the content-line `emotion`. The
/// core ships NO members for any of them, so each is declared by a document's
/// own inline `enums:`, a project schema (`enums:`), or a plugin — and using an
/// undeclared one is `E-DOMAIN-UNKNOWN` (D-C). Reported here so a project
/// missing a slot learns it from `doctor` rather than from a diagnostic
/// mid-scene.
const VOCAB_SLOTS: &[&str] = &[
    "emotion",
    "action",
    "anchor",
    "mood",
    "volume",
    "musicAction",
    "vfxType",
];

/// One checklist entry: a stable `key` (JSON), a human `label`, the boolean
/// `ok` state (`None` = informational, neither pass nor fail), a `detail`
/// string, and an optional remedy `hint` shown on `✗`.
struct Check {
    key: &'static str,
    label: String,
    ok: Option<bool>,
    detail: String,
    hint: Option<String>,
}

impl Check {
    fn info(key: &'static str, label: &str, detail: String) -> Self {
        Check {
            key,
            label: label.to_string(),
            ok: None,
            detail,
            hint: None,
        }
    }

    fn pass(key: &'static str, label: &str, detail: String) -> Self {
        Check {
            key,
            label: label.to_string(),
            ok: Some(true),
            detail,
            hint: None,
        }
    }

    fn fail(key: &'static str, label: &str, detail: String, hint: &str) -> Self {
        Check {
            key,
            label: label.to_string(),
            ok: Some(false),
            detail,
            hint: Some(hint.to_string()),
        }
    }
}

/// Walk upward from `dir` (inclusive) for the nearest ancestor carrying a
/// `lute.project.yaml`, mirroring `crate::project_root_for`'s ancestry walk
/// conceptually. Returns the manifest-bearing directory when found.
fn find_manifest_dir(dir: &Path) -> Option<PathBuf> {
    let mut cur: Option<&Path> = Some(dir);
    while let Some(d) = cur {
        if d.join("lute.project.yaml").exists() {
            return Some(d.to_path_buf());
        }
        cur = d.parent();
    }
    None
}

/// The merged domain vocabulary the project's documents ACTUALLY resolve,
/// unioned across every `.lute` file under `root`, plus the DEDUPLICATED
/// project-resolution problems that resolution surfaced.
///
/// Deliberately no second resolution path: this reuses `crate::build_input` —
/// the SAME per-document resolution `lute check`/`check-project` perform (each
/// file's own project root via `crate::project_root_for`, its activated
/// capability snapshot per plugin §4/§11, then its `uses:`/`extends:` schema
/// imports per dsl §9.2) — and folds it through the SAME `merge_domains` the
/// checker consults for `Type::Domain` resolution. So a slot `doctor` calls
/// declared is a slot the checker resolves, by construction rather than by
/// two implementations agreeing.
///
/// `root` is the WALK ROOT handed to `crate::project_root_for`, i.e. the lower
/// bound of each file's own ancestor search. It MUST be the directory `doctor`
/// was asked about, for the reason spelled out at the call site.
///
/// A slot counts as declared for the PROJECT when at least one document
/// resolves it. `merge_domains`'s diagnostics are dropped: `doctor` reports and
/// never gates, and a domain collision or a missing `exits:` is `check`'s to
/// report — at a span, in the file that caused it.
///
/// The project-resolution problems are NOT dropped, and are not printed either:
/// they are the very thing an author runs `doctor` to see. A broken
/// `lute.project.yaml` (or an unknown profile, a missing active plugin, a bad
/// plugin option) describes the PROJECT, so every document under it resolves the
/// identical message — returned deduplicated, in first-seen order, for the
/// caller to render as ONE `Check`.
fn resolved_domains(
    root: &Path,
    lute_files: &[PathBuf],
) -> (BTreeMap<String, Domain>, Vec<String>) {
    // `merge_domains` anchors its (discarded) diagnostics at this span; there
    // is no one document to blame for a project-wide report, so it gets the
    // same zeroed placeholder the CLI's other source-less call sites use.
    let at = lute_core_span::Span {
        byte_start: 0,
        byte_end: 0,
        line: 0,
        column: 0,
        utf16_range: (0, 0),
    };
    let mut out = BTreeMap::new();
    let mut problems: Vec<String> = Vec::new();
    for file in lute_files {
        let project = crate::project_root_for(file, root);
        let Some(built) = crate::build_input(file, None, Some(&project)) else {
            continue;
        };
        for m in &built.project_diags {
            if !problems.iter().any(|p| p == m) {
                problems.push(m.clone());
            }
        }
        let (merged, _diags) = lute_check::schema_import::merge_domains(
            &built.input.snapshot,
            &built.input.imports,
            &built.meta,
            at,
        );
        out.extend(merged);
    }
    (out, problems)
}

/// One declared slot's entry in the `doctor` report: the slot name, plus the
/// member-level semantics the compiler READS for the two slots that carry it
/// (dsl 0.9.0 D-D) — `action`'s `exits:` and `anchor`'s `default:`. Those two
/// are why a slot declaration is more than a member list, and both are
/// invisible in the scene text that depends on them, so the resolved values
/// belong on the line: a `default:` pointing at the wrong anchor is a silent
/// staging bug everywhere except here.
fn slot_entry(slot: &str, domain: &Domain) -> String {
    if SLOT_REQUIRES_EXITS.contains(&slot) {
        let exits = if domain.exits.is_empty() {
            "none".to_string()
        } else {
            domain.exits.join("/")
        };
        format!("{slot} (exits: {exits})")
    } else if SLOT_REQUIRES_DEFAULT.contains(&slot) {
        format!(
            "{slot} (default: {})",
            domain.default.as_deref().unwrap_or("none")
        )
    } else {
        slot.to_string()
    }
}

/// Assemble the full checklist for `dir`. Returns `None` when `dir` is
/// unreadable (the only hard failure — caller exits `2`).
fn collect_checks(dir: &Path) -> Option<Vec<Check>> {
    // Directory readability is the sole gating condition. `find_lute_files`
    // surfaces the same walk I/O errors `check-project` does.
    let lute_files = match crate::find_lute_files(dir) {
        Ok(files) => files,
        Err(_) => return None,
    };

    let mut checks = Vec::new();

    // --- Versions (the three independent axes, docs/versioning.md) --------
    checks.push(Check::info(
        "toolchain",
        "toolchain version",
        env!("CARGO_PKG_VERSION").to_string(),
    ));
    checks.push(Check::info(
        "language",
        "language version",
        lute_check::LUTE_LANG_VERSION.to_string(),
    ));
    checks.push(Check::info(
        "ir",
        "IR schema version",
        lute_compile::LUTE_IR_VERSION.to_string(),
    ));

    // --- Project manifest (walk up from `dir`) ---------------------------
    let manifest_dir = find_manifest_dir(dir);
    match &manifest_dir {
        Some(root) => checks.push(Check::pass(
            "project",
            "lute.project.yaml",
            format!("found at {}", root.join("lute.project.yaml").display()),
        )),
        None => checks.push(Check::fail(
            "project",
            "lute.project.yaml",
            "not found at or above this directory".to_string(),
            "run `lute init <dir>` to scaffold a project, or `cd` into one",
        )),
    }

    // --- Content: count of .lute documents -------------------------------
    let n = lute_files.len();
    if n > 0 {
        checks.push(Check::pass(
            "luteFiles",
            "content documents",
            format!("{n} `.lute` file(s) under {}", dir.display()),
        ));
    } else {
        checks.push(Check::fail(
            "luteFiles",
            "content documents",
            format!("no `.lute` files under {}", dir.display()),
            "add a scene with `lute new scene <name>`",
        ));
    }

    // --- Provider snapshots (`providers/` under the manifest dir) --------
    let providers_dir = manifest_dir
        .clone()
        .unwrap_or_else(|| dir.to_path_buf())
        .join("providers");
    if providers_dir.is_dir() {
        let set = ProviderSet::load(&providers_dir);
        let snaps = set.snapshots();
        let stale = snaps.iter().filter(|s| s.stale).count();
        if stale > 0 {
            checks.push(Check::fail(
                "providers",
                "provider snapshots",
                format!(
                    "{} snapshot(s) at {}, {stale} stale",
                    snaps.len(),
                    providers_dir.display()
                ),
                "re-stamp with `lute catalog refresh <providers-dir>`",
            ));
        } else {
            checks.push(Check::pass(
                "providers",
                "provider snapshots",
                format!(
                    "{} snapshot(s) at {}, none stale",
                    snaps.len(),
                    providers_dir.display()
                ),
            ));
        }
    } else {
        checks.push(Check::info(
            "providers",
            "provider snapshots",
            "no providers/ directory (core-only project)".to_string(),
        ));
    }

    // --- Vocabulary slots (dsl 0.9.0 D-A/D-C) ----------------------------
    // The core declares the slots and ships no members, so a project that
    // never declares one only finds out when an author writes the attr. This
    // is where it finds out first.
    //
    // INVARIANT: the walk root is the REQUESTED `dir`, never `manifest_dir`.
    // `doctor`'s root policy MUST equal `check-project`'s or `doctor` lies about
    // the checker's verdict: `check-project <dir>` passes `dir` as
    // `project_root_for`'s lower bound, so it cannot ascend above the directory
    // it was asked about. Handing `manifest_dir` in here would let `doctor` pick
    // up a parent `lute.project.yaml` — and report a plugin-declared slot as
    // declared while `check-project` on the SAME dir rejects it with
    // `E-DOMAIN-UNKNOWN`. `manifest_dir` remains the right answer for the
    // "found `lute.project.yaml` at …" line above, which reports the ancestry
    // rather than predicting a verdict.
    let (domains, project_problems) = resolved_domains(dir, &lute_files);
    let declared: Vec<String> = VOCAB_SLOTS
        .iter()
        .filter_map(|slot| domains.get(*slot).map(|dom| slot_entry(slot, dom)))
        .collect();
    let missing: Vec<&str> = VOCAB_SLOTS
        .iter()
        .copied()
        .filter(|slot| !domains.contains_key(*slot))
        .collect();
    checks.push(Check::info(
        "vocabularySlots",
        "vocabulary slots declared",
        if declared.is_empty() {
            "none".to_string()
        } else {
            declared.join(", ")
        },
    ));
    if !missing.is_empty() {
        checks.push(Check::info(
            "vocabularySlotsMissing",
            "not declared (using one errors)",
            format!(
                "{} — declare members in a project schema's `enums:`, in a \
                 document's own frontmatter, or in a plugin's `enums` export \
                 (`lute init` scaffolds a starter set)",
                missing.join(", ")
            ),
        ));
    }

    // --- Project resolution (what `build_input` used to print raw) -------
    // A `lute.project.yaml` that fails to load, an unknown profile, a missing
    // active plugin, a bad plugin option, a malformed `identity:` template: all
    // describe the PROJECT, so every document under it reports the identical
    // message. Reported here ONCE, through the same `Check` model as everything
    // else — a `doctor` whose findings only reach stderr is invisible to any
    // consumer parsing its output, and `doctor` is the command you run when
    // something is already wrong. Absent key == nothing to report.
    if !project_problems.is_empty() {
        checks.push(Check::fail(
            "projectResolution",
            "project resolution",
            project_problems.join("; "),
            "fix `lute.project.yaml` (or the plugin/profile it activates); \
             `lute check-project` fails on the same problem",
        ));
    }

    // --- Editor integration: not introspectable from the CLI -------------
    checks.push(Check::info(
        "vscode",
        "VS Code extension",
        "not detectable from the CLI".to_string(),
    ));

    Some(checks)
}

/// Render the checklist as human `✓`/`✗`/`•` lines.
fn print_human(dir: &Path, checks: &[Check]) {
    println!("lute doctor — {}", dir.display());
    for c in checks {
        let mark = match c.ok {
            Some(true) => "✓",
            Some(false) => "✗",
            None => "•",
        };
        println!("  {mark} {}: {}", c.label, c.detail);
        if c.ok == Some(false) {
            if let Some(hint) = &c.hint {
                println!("      → {hint}");
            }
        }
    }
}

/// Render the checklist as a stable-keyed JSON object: `{ "dir": …, "checks":
/// { <key>: { "label", "ok", "detail", "hint" } } }`. `serde_json::Map` sorts
/// keys lexicographically, so the object is deterministic across runs.
fn print_json(dir: &Path, checks: &[Check]) {
    let mut map = serde_json::Map::new();
    for c in checks {
        let mut obj = serde_json::Map::new();
        obj.insert(
            "label".to_string(),
            serde_json::Value::from(c.label.clone()),
        );
        obj.insert(
            "ok".to_string(),
            match c.ok {
                Some(b) => serde_json::Value::Bool(b),
                None => serde_json::Value::Null,
            },
        );
        obj.insert(
            "detail".to_string(),
            serde_json::Value::from(c.detail.clone()),
        );
        obj.insert(
            "hint".to_string(),
            match &c.hint {
                Some(h) => serde_json::Value::from(h.clone()),
                None => serde_json::Value::Null,
            },
        );
        map.insert(c.key.to_string(), serde_json::Value::Object(obj));
    }
    let root = serde_json::json!({
        "dir": dir.display().to_string(),
        "checks": serde_json::Value::Object(map),
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&root).expect("doctor report serializes")
    );
}

/// Diagnose the toolchain and project setup. See [`crate::Command::Doctor`].
///
/// Always exits `0` (doctor reports, never gates) unless `dir` is unreadable,
/// which is exit `2`.
pub fn run_doctor(dir: &Path, json: bool) -> ExitCode {
    let Some(checks) = collect_checks(dir) else {
        eprintln!("lute doctor: cannot read `{}`", dir.display());
        return ExitCode::from(2);
    };
    if json {
        print_json(dir, &checks);
    } else {
        print_human(dir, &checks);
    }
    ExitCode::SUCCESS
}
