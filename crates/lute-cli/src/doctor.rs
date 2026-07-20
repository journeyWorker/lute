//! `lute doctor` — local toolchain + project setup diagnosis.
//!
//! A REPORT, never a gate: `doctor` prints a checklist of the local Lute setup
//! and always exits `0` — unless the target directory itself is unreadable
//! (exit `2`). Each check is a `✓`/`✗` line; a `✗` carries a remedy hint. The
//! `--json` variant emits the same checks as a stable-keyed object.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lute_manifest::provider::ProviderSet;

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
    match find_manifest_dir(dir) {
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
    let providers_dir = find_manifest_dir(dir)
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
        obj.insert("label".to_string(), serde_json::Value::from(c.label.clone()));
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
