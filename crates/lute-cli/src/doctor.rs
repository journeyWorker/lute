//! `lute doctor` — local toolchain + project setup diagnosis.
//!
//! A REPORT, never a gate: `doctor` prints a checklist of the local Lute setup
//! and always exits `0` — unless the target directory itself is unreadable
//! (exit `2`). Each check is a `✓`/`✗` line; a `✗` carries a remedy hint. The
//! `--json` variant emits the same checks as a stable-keyed object.

use std::collections::{BTreeMap, BTreeSet};
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

/// What the project's documents ACTUALLY resolve, unioned across every `.lute`
/// file under `root`: the merged domain vocabulary, the active plugins, the
/// occasion vocabulary with the beats answering each occasion, and the
/// DEDUPLICATED project-resolution problems resolution surfaced.
///
/// Deliberately no second resolution path: this reuses `crate::build_input` —
/// the SAME per-document resolution `lute check`/`check-project` perform (each
/// file's own project root via `crate::project_root_for`, its activated
/// capability snapshot per plugin §4/§11, then its `uses:`/`extends:` schema
/// imports per dsl §9.2) — and folds it through the SAME `merge_domains` the
/// checker consults for `Type::Domain` resolution. So a slot `doctor` calls
/// declared is a slot the checker resolves, and a plugin it calls active is a
/// plugin some document's snapshot carries, by construction rather than by two
/// implementations agreeing.
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
#[derive(Default)]
struct ProjectScan {
    domains: BTreeMap<String, Domain>,
    problems: Vec<String>,
    /// Active plugin id → version, `lute.core` excluded (every snapshot has it).
    plugins: BTreeMap<String, String>,
    /// Declared occasions (from any document's snapshot).
    declared_occasions: BTreeSet<String>,
    /// Occasion → beats answering it: scene documents with `on:` plus lore
    /// entries with `on=` (dsl 0.21.0 §3).
    beats: BTreeMap<String, usize>,
}

fn scan_documents(root: &Path, lute_files: &[PathBuf]) -> ProjectScan {
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
    let mut scan = ProjectScan::default();
    for file in lute_files {
        let project = crate::project_root_for(file, root);
        let Some(built) = crate::build_input(file, None, Some(&project), None) else {
            continue;
        };
        for m in &built.project_diags {
            if !scan.problems.iter().any(|p| p == m) {
                scan.problems.push(m.clone());
            }
        }
        let (merged, _diags) = lute_check::schema_import::merge_domains(
            &built.input.snapshot,
            &built.input.imports,
            &built.meta,
            at,
        );
        scan.domains.extend(merged);
        for (id, plugin) in &built.input.snapshot.plugins {
            if id != "lute.core" {
                scan.plugins.insert(id.clone(), plugin.version.clone());
            }
        }
        scan.declared_occasions
            .extend(built.input.snapshot.occasions.keys().cloned());
        if let Some(beat) = &built.meta.beat {
            *scan.beats.entry(beat.on.clone()).or_default() += 1;
        }
        let (doc, _) = lute_syntax::parse(&built.input.text);
        for entry in &doc.entries {
            if let Some((on, _)) = &entry.on {
                *scan.beats.entry(on.clone()).or_default() += 1;
            }
        }
        // dsl 0.23.0 §4: every lore `<beat>` bundle answers its `on` too.
        for beat in &doc.beats {
            if let Some((on, _)) = &beat.on {
                *scan.beats.entry(on.clone()).or_default() += 1;
            }
        }
    }
    scan
}

/// The occasions line: every declared occasion with the number of beats
/// answering it (a declared occasion nothing answers shows `0` — the engine
/// raises it and nothing happens), then any occasion beats answer that no
/// snapshot declares. With no declared vocabulary (shape-only, dsl 0.21.0
/// §2) the beats' own occasions are the whole report.
fn occasions_detail(scan: &ProjectScan) -> String {
    let beats_of = |name: &str| scan.beats.get(name).copied().unwrap_or(0);
    let declared: Vec<String> = scan
        .declared_occasions
        .iter()
        .map(|name| format!("{name} ({})", beats_of(name)))
        .collect();
    let undeclared: Vec<String> = scan
        .beats
        .iter()
        .filter(|(name, _)| !scan.declared_occasions.contains(*name))
        .map(|(name, n)| format!("{name} ({n})"))
        .collect();
    let total: usize = scan.beats.values().sum();
    if scan.declared_occasions.is_empty() {
        if undeclared.is_empty() {
            return "none declared, no beats".to_string();
        }
        return format!(
            "none declared (shape-only); {total} beat(s) answer: {}",
            undeclared.join(", ")
        );
    }
    let mut detail = format!(
        "{} declared, {total} beat(s) — {}",
        scan.declared_occasions.len(),
        declared.join(", ")
    );
    if !undeclared.is_empty() {
        detail.push_str(&format!("; answered but not declared: {}", undeclared.join(", ")));
    }
    detail
}

/// Count files under `dir` (recursive) whose name ends with `suffix` — the
/// `*.play.yaml` scripts and `*.test.yaml` scenario tests `lute play` /
/// `lute test` pick up. An unreadable subdirectory is skipped: the count is
/// a report, and `find_lute_files` has already vouched for the tree.
fn count_files(dir: &Path, suffix: &str) -> usize {
    let mut n = 0;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for entry in rd.flatten() {
            let path = entry.path();
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                stack.push(path);
            } else if path
                .file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.ends_with(suffix))
            {
                n += 1;
            }
        }
    }
    n
}

/// The `lute-lsp` an editor launches is whichever one is first on `PATH`, and
/// nothing else in the toolchain notices when it is an older build than the
/// CLI: all three dogfood projects ran a 0.17 server against a 0.21 CLI, so
/// the editor's diagnostics disagreed with `lute check`. Located by walking
/// `PATH` exactly as a process spawn would, then asked `--version` (dsl
/// 0.22.0 §13).
fn find_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

/// Run `<lsp> --version` and return its stdout. A pre-0.22.0 server ignores the
/// flag and starts serving; with stdin closed it reads EOF and exits, but it is
/// also killed after a short grace period so `doctor` can never hang on one.
fn lsp_version_output(lsp: &Path) -> Option<String> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    let mut child = Command::new(lsp)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break;
            }
        }
    }
    let mut out = String::new();
    child.stdout.take()?.read_to_string(&mut out).ok()?;
    Some(out)
}

/// The `languageServer` check: pass when the `lute-lsp` on `PATH` reports this
/// toolchain's version, fail (with the fix) when it reports another or none,
/// informational when there is none on `PATH`.
fn language_server_check() -> Check {
    const KEY: &str = "languageServer";
    const LABEL: &str = "lute-lsp on PATH";
    let ours = env!("CARGO_PKG_VERSION");
    let exe = if cfg!(windows) { "lute-lsp.exe" } else { "lute-lsp" };
    let Some(lsp) = find_on_path(exe) else {
        return Check::info(
            KEY,
            LABEL,
            "not found (editors that launch `lute-lsp` from PATH get no diagnostics)".to_string(),
        );
    };
    let reported = lsp_reported_version(&lsp);
    let hint = "reinstall the language server from this toolchain \
                (`cargo install --path crates/lute-lsp`) and restart the editor";
    match reported {
        Some(v) if v == ours => Check::pass(KEY, LABEL, format!("{v} at {}", lsp.display())),
        Some(v) => Check::fail(
            KEY,
            LABEL,
            format!("{v} at {} — differs from lute {ours}", lsp.display()),
            hint,
        ),
        None => Check::fail(
            KEY,
            LABEL,
            format!(
                "{} reports no version (older than 0.22.0) — differs from lute {ours}",
                lsp.display()
            ),
            hint,
        ),
    }
}

/// The version a `lute-lsp` binary reports for `--version`, `None` when it
/// reports none (older than 0.22.0).
fn lsp_reported_version(lsp: &Path) -> Option<String> {
    lsp_version_output(lsp).and_then(|out| {
        out.lines()
            .next()
            .and_then(|l| l.trim().strip_prefix("lute-lsp "))
            .map(|v| v.trim().to_string())
    })
}

/// One running `lute-lsp` process: its pid, the binary it was started from
/// (when it can be told), and whether that binary was replaced or deleted
/// since it started.
#[cfg(unix)]
struct RunningLsp {
    pid: u32,
    exe: Option<PathBuf>,
    replaced: bool,
}

/// `ps`'s `etime` (`[[dd-]hh:]mm:ss`) in seconds.
#[cfg(unix)]
fn parse_etime(s: &str) -> Option<u64> {
    let (days, clock) = match s.split_once('-') {
        Some((d, rest)) => (d.parse::<u64>().ok()?, rest),
        None => (0, s),
    };
    let mut secs = 0u64;
    for part in clock.split(':') {
        secs = secs * 60 + part.parse::<u64>().ok()?;
    }
    Some(days * 86_400 + secs)
}

/// The binary a process command line (`ps`'s `args`) was started from, when
/// it is `lute-lsp`: the longest prefix ending in `lute-lsp` that is the bare
/// name or an existing file (a path may hold spaces; a script shows as
/// `/bin/sh /path/lute-lsp`).
#[cfg(unix)]
fn lsp_argv_binary(args: &str) -> Option<&str> {
    const NAME: &str = "lute-lsp";
    for (idx, _) in args.match_indices(NAME) {
        let end = idx + NAME.len();
        if !args[end..].is_empty() && !args[end..].starts_with(' ') {
            continue;
        }
        let starts = std::iter::once(0).chain(args[..idx].match_indices(' ').map(|(i, _)| i + 1));
        for start in starts {
            let candidate = &args[start..end];
            if candidate == NAME || (candidate.ends_with("/lute-lsp") && Path::new(candidate).is_file()) {
                return Some(candidate);
            }
        }
    }
    None
}

/// Every running `lute-lsp` server (`ps`), `None` when processes cannot be
/// listed. A `--version` probe is not a server and is skipped.
#[cfg(unix)]
fn running_lsps() -> Option<Vec<RunningLsp>> {
    let ps = if Path::new("/bin/ps").is_file() { "/bin/ps" } else { "ps" };
    let out = std::process::Command::new(ps)
        .args(["-Ao", "pid=,etime=,args="])
        .env("LC_ALL", "C")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let now = std::time::SystemTime::now();
    let text = String::from_utf8_lossy(&out.stdout);
    let mut found = Vec::new();
    for row in text.lines() {
        let row = row.trim_start();
        let Some((pid, rest)) = row.split_once(char::is_whitespace) else { continue };
        let Some((etime, args)) = rest.trim_start().split_once(char::is_whitespace) else { continue };
        let (Ok(pid), Some(age)) = (pid.parse::<u32>(), parse_etime(etime)) else { continue };
        let args = args.trim();
        let Some(argv) = lsp_argv_binary(args) else { continue };
        if args[argv.len()..].split_whitespace().any(|a| a == "--version") {
            continue;
        }
        // Linux names the binary a process runs, and marks it replaced.
        let proc_exe = std::fs::read_link(format!("/proc/{pid}/exe")).ok();
        let deleted = proc_exe
            .as_ref()
            .is_some_and(|p| p.to_string_lossy().ends_with(" (deleted)"));
        let exe = match proc_exe {
            Some(p) if !deleted => Some(p),
            _ if argv.contains('/') => Some(PathBuf::from(argv)),
            _ => find_on_path("lute-lsp"),
        };
        // `etime` is whole seconds, and macOS computes it as
        // floor(now) - floor(start), so it can read one second more than the
        // process has lived. Allow that second of slack: only a binary written
        // more than a second after `now - etime` was replaced while the
        // server ran.
        let started = now.checked_sub(std::time::Duration::from_secs(age));
        let rebuilt = exe
            .as_ref()
            .and_then(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())
            .zip(started)
            .is_some_and(|(modified, started)| {
                modified > started + std::time::Duration::from_secs(1)
            });
        found.push(RunningLsp {
            pid,
            exe,
            replaced: deleted || rebuilt,
        });
    }
    Some(found)
}

/// The `runningLanguageServers` check (seven-days F27): a server the editor
/// started keeps running its old build after `lute-lsp` is reinstalled, so
/// the binary on `PATH` can be current while the editor still reports a
/// stale toolchain. Fail when a running `lute-lsp` was started from a binary
/// replaced since, or from one that reports another version.
#[cfg(unix)]
fn running_language_servers_check() -> Check {
    const KEY: &str = "runningLanguageServers";
    const LABEL: &str = "running lute-lsp";
    let ours = env!("CARGO_PKG_VERSION");
    let Some(servers) = running_lsps() else {
        return Check::info(KEY, LABEL, "cannot list processes (`ps` failed)".to_string());
    };
    if servers.is_empty() {
        return Check::info(KEY, LABEL, "none running".to_string());
    }
    let mut versions: BTreeMap<PathBuf, Option<String>> = BTreeMap::new();
    let (mut stale, mut current, mut unknown) = (Vec::new(), Vec::new(), Vec::new());
    for s in &servers {
        let Some(exe) = &s.exe else {
            unknown.push(format!("pid {} (binary not found)", s.pid));
            continue;
        };
        let at = format!("pid {} ({})", s.pid, exe.display());
        if s.replaced {
            stale.push(format!("{at} started before its binary was replaced, so it runs an older build"));
            continue;
        }
        let reported = versions
            .entry(exe.clone())
            .or_insert_with(|| lsp_reported_version(exe));
        match reported {
            Some(v) if v == ours => current.push(format!("{at} {v}")),
            Some(v) => stale.push(format!("{at} is {v}, not lute {ours}")),
            None => stale.push(format!("{at} reports no version (older than 0.22.0)")),
        }
    }
    let all: Vec<String> = stale.iter().chain(&current).chain(&unknown).cloned().collect();
    if !stale.is_empty() {
        Check::fail(
            KEY,
            LABEL,
            all.join("; "),
            "restart the editor (or its language server) so it launches this toolchain's lute-lsp",
        )
    } else if !current.is_empty() {
        Check::pass(KEY, LABEL, all.join("; "))
    } else {
        Check::info(KEY, LABEL, all.join("; "))
    }
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

    // --- Plays and scenario tests -----------------------------------------
    // `lute play` scripts and `lute test` scenarios are part of the project
    // as much as its documents; a count is the at-a-glance answer to "is any
    // of this exercised".
    checks.push(Check::info(
        "plays",
        "play scripts",
        format!("{} `*.play.yaml`", count_files(dir, ".play.yaml")),
    ));
    checks.push(Check::info(
        "tests",
        "scenario tests",
        format!("{} `*.test.yaml`", count_files(dir, ".test.yaml")),
    ));

    // --- Provider snapshots (the project's pinned catalog) ---------------
    // The SAME directory `check` resolves provider ids against
    // (`lute_manifest::project::project_providers`): the manifest's
    // `catalogDir:`, default `catalog/`. Its absence says nothing about
    // plugins — only that no provider id is pinned.
    let catalog_dir = manifest_dir.as_deref().map(|root| {
        match lute_manifest::project::load_project(root) {
            Ok(Some(config)) => config.catalog_dir,
            _ => root.join("catalog"),
        }
    });
    match catalog_dir.filter(|d| d.is_dir()) {
        Some(catalog_dir) => {
            let set = ProviderSet::load(&catalog_dir);
            let snaps = set.snapshots();
            let stale = snaps.iter().filter(|s| s.stale).count();
            if stale > 0 {
                checks.push(Check::fail(
                    "providers",
                    "provider snapshots",
                    format!(
                        "{} snapshot(s) at {}, {stale} stale",
                        snaps.len(),
                        catalog_dir.display()
                    ),
                    "re-stamp with `lute catalog refresh <catalog-dir>`",
                ));
            } else {
                checks.push(Check::pass(
                    "providers",
                    "provider snapshots",
                    format!(
                        "{} snapshot(s) at {}, none stale",
                        snaps.len(),
                        catalog_dir.display()
                    ),
                ));
            }
        }
        None => checks.push(Check::info(
            "providers",
            "provider snapshots",
            "no pinned provider snapshots".to_string(),
        )),
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
    let scan = scan_documents(dir, &lute_files);
    let domains = &scan.domains;

    // --- Plugins and occasions (dsl 0.21.0 §2) ---------------------------
    // What the documents resolve, not what the manifest lists: a profile no
    // document selects activates nothing.
    checks.push(Check::info(
        "plugins",
        "active plugins",
        if scan.plugins.is_empty() {
            "none (core-only)".to_string()
        } else {
            scan.plugins
                .iter()
                .map(|(id, v)| format!("{id} {v}"))
                .collect::<Vec<_>>()
                .join(", ")
        },
    ));
    checks.push(Check::info(
        "occasions",
        "occasions (beats answering)",
        occasions_detail(&scan),
    ));
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
    if !scan.problems.is_empty() {
        checks.push(Check::fail(
            "projectResolution",
            "project resolution",
            scan.problems.join("; "),
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
    checks.push(language_server_check());
    #[cfg(unix)]
    checks.push(running_language_servers_check());

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
