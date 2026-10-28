//! `lute-bench lsp-requests`: the language server's per-request cost (spec
//! 0.53.0 §3.5, `vocab-grid.py` tier `lsp`).
//!
//! Drives the backend's analysis ([`lute_lsp::backend::analyze_document`],
//! the protocol-free part of `textDocument/publishDiagnostics`) on one
//! document of a project, `--requests` times, each with a different buffer
//! (a line appended, the frontmatter unchanged: an edit). It prints the
//! median warm request time (every request after the first), the first
//! request's, and the base and vocabulary builds over the run. Peak RSS is
//! the caller's measurement of this process (`vocab-grid.py` reads it from
//! the child's resource usage, as for every tier).
//!
//! `--fresh` gives every request its own cache, as 0.52.0 did: the before
//! of the before/after table. `--churn K` instead cycles the document's
//! frontmatter through `K` configurations (distinct `plugins:` maps, `uses:`
//! lists of the existing import spelled distinctly, and `uses:` lists naming
//! missing imports) and prints, after each step, what the root's cache
//! retains in every memo and its dependency records next to the closure of
//! the live and the last four superseded configurations, built
//! independently on a fresh cache; it fails when they differ or when the
//! retained counts change after the fifth step but for the shape cycle.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use lute_check::Mode;
use lute_load::cache::RetainedCounts;
use lute_load::{assemble_input_with_mode, InputCache, InputCacheStats};
use lute_lsp::backend::{analyze_document, find_project_root, ProjectInputs, SUPERSEDED_RETAINED};

const USAGE: &str =
    "usage: lute-bench lsp-requests <project> --doc <file> [--requests <n>] [--fresh] [--churn <k>]";

/// How the requests use the cache.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Run {
    /// One cache per root across requests (0.53.0).
    Cached,
    /// A fresh cache per request (0.52.0).
    Fresh,
    /// Configuration churn over `k` configurations.
    Churn(usize),
}

/// `lute-bench lsp-requests <project> [flags]`: returns the exit code.
pub(crate) fn main(args: &[String]) -> i32 {
    let mut project: Option<PathBuf> = None;
    let mut doc: Option<PathBuf> = None;
    let mut requests = 20usize;
    let mut mode = Run::Cached;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--doc" => match args.next() {
                Some(v) => doc = Some(PathBuf::from(v)),
                None => return usage("--doc requires a file"),
            },
            "--requests" => match args.next().map(|v| v.parse::<usize>()) {
                Some(Ok(n)) if n >= 2 => requests = n,
                _ => return usage("--requests must be an integer >= 2"),
            },
            "--fresh" => mode = Run::Fresh,
            "--churn" => match args.next().map(|v| v.parse::<usize>()) {
                Some(Ok(k)) if k > SUPERSEDED_RETAINED + 1 => mode = Run::Churn(k),
                _ => {
                    return usage(&format!(
                        "--churn must be an integer > {}",
                        SUPERSEDED_RETAINED + 1
                    ))
                }
            },
            other if other.starts_with("--") => {
                return usage(&format!("unknown argument `{other}`"))
            }
            other if project.is_none() => project = Some(PathBuf::from(other)),
            other => return usage(&format!("unexpected argument `{other}`")),
        }
    }
    let (Some(_project), Some(doc)) = (project, doc) else {
        return usage("a project and --doc are required");
    };
    let text = match std::fs::read_to_string(&doc) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("lute-bench: cannot read {}: {error}", doc.display());
            return 2;
        }
    };
    let root = find_project_root(&doc);
    match mode {
        Run::Cached | Run::Fresh => {
            requests_run(&doc, root.as_deref(), &text, requests, mode);
            0
        }
        Run::Churn(k) => match churn(&doc, root.as_deref(), &text, k) {
            Ok(()) => 0,
            Err(error) => {
                eprintln!("lute-bench: lsp-requests --churn: {error}");
                1
            }
        },
    }
}

fn usage(error: &str) -> i32 {
    eprintln!("lute-bench: {error}");
    eprintln!("{USAGE}");
    2
}

/// `text` as the `n`th edit: a line appended to the body.
fn edited(text: &str, n: usize) -> String {
    format!("{text}\n@narrator: Request {n}.\n")
}

fn requests_run(doc: &Path, root: Option<&Path>, text: &str, requests: usize, mode: Run) {
    let key = doc.display().to_string();
    let mut inputs = ProjectInputs::default();
    let mut stats = InputCacheStats::default();
    let mut times = Vec::with_capacity(requests);
    for n in 0..requests {
        if mode == Run::Fresh {
            stats += inputs.stats(root);
            inputs = ProjectInputs::default();
        }
        inputs.open(&key);
        let buffer = edited(text, n);
        let start = Instant::now();
        let analysis = analyze_document(&inputs, &key, doc, &buffer);
        times.push(start.elapsed());
        std::hint::black_box(analysis);
    }
    stats += inputs.stats(root);
    let first = times[0];
    let mut warm: Vec<Duration> = times[1..].to_vec();
    warm.sort();
    let median = warm[warm.len() / 2];
    println!(
        "lsp-requests: mode={} requests={requests} first_ms={:.3} median_warm_ms={:.3} {stats}",
        if mode == Run::Fresh {
            "fresh"
        } else {
            "cached"
        },
        first.as_secs_f64() * 1000.0,
        median.as_secs_f64() * 1000.0,
    );
}

/// The document `text` with its `uses:` line replaced by configuration `j`.
fn configuration(text: &str, j: usize) -> String {
    let line = match j % 3 {
        0 => format!("plugins: {{ bench{j}: {{}} }}\nuses: ../schema/world.schema.yaml"),
        1 => format!("uses: ../schema/{}world.schema.yaml", "./".repeat(j)),
        _ => format!("uses: ../schema/missing{j}.schema.yaml"),
    };
    text.lines()
        .map(|l| {
            if l.starts_with("uses:") {
                line.as_str()
            } else {
                l
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

/// The entries a fresh cache builds for `texts`: the closure the server's
/// cache must hold for the same configurations.
fn closure(doc: &Path, root: Option<&Path>, texts: &[&String]) -> RetainedCounts {
    let cache = InputCache::tracked();
    if let Some(root) = root {
        cache.project(root);
    }
    for text in texts {
        assemble_input_with_mode(&cache, doc, (*text).clone(), None, root, None, Mode::Author);
    }
    cache.retained()
}

fn churn(doc: &Path, root: Option<&Path>, text: &str, k: usize) -> Result<(), String> {
    if !text.lines().any(|l| l.starts_with("uses:")) {
        return Err("the document has no `uses:` line to vary".to_string());
    }
    let key = doc.display().to_string();
    let inputs = ProjectInputs::default();
    inputs.open(&key);
    let texts: Vec<String> = (0..k).map(|j| configuration(text, j)).collect();
    let mut held_at = Vec::with_capacity(k);
    let mut failures = Vec::new();
    for (j, text) in texts.iter().enumerate() {
        analyze_document(&inputs, &key, doc, text);
        let first = j.saturating_sub(SUPERSEDED_RETAINED);
        let keep: Vec<&String> = texts[first..=j].iter().collect();
        let held = inputs
            .retained(root)
            .ok_or_else(|| "the root has no cache".to_string())?;
        let expected = closure(doc, root, &keep);
        println!("lsp-requests: churn step={j} retained {held}");
        if held != expected {
            println!("lsp-requests: churn step={j} closure  {expected}");
            failures.push(format!("step {j}: retained {held}, closure {expected}"));
        }
        held_at.push(held);
    }
    // After the fifth step the live and superseded configurations repeat
    // their shape every three steps, and so must the retained counts.
    let constant = (SUPERSEDED_RETAINED + 3..k).all(|j| held_at[j] == held_at[j - 3]);
    let equal = failures.is_empty();
    println!(
        "lsp-requests: churn configurations={k} closure_equal={equal} constant_after_step5={constant}"
    );
    if !constant {
        failures.push("the retained counts grew after the fifth step".to_string());
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("; "))
    }
}
