use super::*;

/// Fold one report's decisions + coverage counts into the run accumulator:
/// its document, the bundle beats / entries it presented (T3-20), its
/// branch/hub picks under the canonical site a play's picks share.
pub(super) fn accumulate_coverage(cov: &mut CoverageAccum, report: &TraceReport) {
    cov.paths += 1;
    let canonical = canonical_key(std::path::Path::new(&report.file));
    // OT-F-14: one spelling per file, whichever test directory traced it.
    let file = display_path(Path::new(&report.file));
    cov.traced_files.insert(canonical.clone());
    for s in &report.steps {
        if let lute_trace::Step::Entry { id, .. } | lute_trace::Step::Beat { id, .. } = s {
            cov.units.insert((canonical.clone(), id.clone()));
        }
    }
    for d in &report.decisions {
        match d.construct.as_str() {
            "branch" | "hub" => {
                let row = cov.choice_row(&canonical, &file, &d.id);
                row.chosen.insert(d.outcome.clone());
                row.eligible.extend(d.eligible.iter().cloned());
            }
            "match" | "guard" => {
                let site = lute_trace::report::site_key_in(&d.span, d.component.as_ref());
                let key = arm_row_key(&file, &site, d.component.as_ref());
                cov.arms
                    .entry(key)
                    // Ember N14: the author's `@def` spelling, not its
                    // expansion (as `lute trace` prints it, T3-12).
                    .or_insert_with(|| {
                        ArmRow::new(
                            d.authored_id.clone().unwrap_or_else(|| d.id.clone()),
                            d.construct == "guard",
                        )
                    })
                    .chosen
                    .insert(d.outcome.clone());
            }
            _ => {}
        }
    }
    for c in report.coverage.choices.values() {
        let row = cov.choice_row(&canonical, &file, &c.label);
        row.total = row.total.max(c.total);
    }
    for (site, c) in &report.coverage.arms {
        let key = arm_row_key(&file, site, c.component.as_ref());
        let row = cov
            .arms
            .entry(key)
            // N14: the author's `@def` spelling, not its expansion (T3-12).
            .or_insert_with(|| {
                ArmRow::new(
                    c.authored_label.clone().unwrap_or_else(|| c.label.clone()),
                    c.guard,
                )
            });
        row.total = row.total.max(c.total);
    }
}

/// The printed coverage site of a `<match>` / guard at trace `site` of the
/// document `file`: `file:line:column`, or — expanded from a component
/// `::use` (SD-15) — the component's own `file:line:column` and which use
/// in which document, so a component's match never merges into its host's
/// construct at the same position, nor into another use's.
pub(super) fn arm_row_key(file: &str, site: &str, component: Option<&lute_trace::ComponentSite>) -> String {
    match component {
        None => format!("{file}:{site}"),
        Some(c) => {
            // `site_key_in`: `"{c.file}:{line}:{column} ({use})"`.
            let at = site
                .strip_prefix(c.file.as_str())
                .and_then(|rest| rest.split_once(" ("))
                .map_or("", |(at, _)| at);
            format!(
                "{}{at} ({} in {file})",
                display_path(Path::new(&c.file)),
                c.scope
            )
        }
    }
}
/// The coverage units of the testable documents under `root` (T3-20):
/// components are out (untestable, see [`run_test`]); a lore document's
/// units are its bundle beats (`<document id>.<beat id>`) and entries; any
/// other document is one unit.
pub(super) fn coverage_units(root: &Path) -> std::io::Result<Vec<CoverageUnit>> {
    let mut out = Vec::new();
    let paths: Vec<PathBuf> = crate::find_lute_files(root)?
        .into_iter()
        .filter(|p| !crate::compile_all::is_component_file(p))
        .collect();
    // Desugared as `check` sees them: template- and sequence-derived beats
    // are units too (dsl 0.27.0 §6).
    let parsed = crate::parse_project_docs(root, &paths);
    let mut source_docs = Vec::new();
    for (path, parsed) in paths.iter().zip(parsed) {
        let Ok((doc, _)) = parsed else {
            continue;
        };
        source_docs.push((path.clone(), doc));
    }
    let snapshot = lute_model::manifest_context(root)
        .map(|context| context.snapshot)
        .unwrap_or_else(|_| lute_manifest::core::load_core_snapshot());
    let owned = lute_check::ProjectDocs::parse(source_docs, &snapshot);
    let views = owned.views();
    for item in views {
        let path = item.path;
        let doc = item.doc;
        let file = display_path(path);
        let canonical = canonical_key(path);
        if doc.entries.is_empty() && doc.beats.is_empty() {
            let meta = item.meta.yaml().and_then(serde_yaml::Value::as_mapping);
            let scene = lute_check::connectivity::scene_key(item.meta);
            let doc_id = scene.clone().or_else(|| {
                meta.and_then(|m| m.get("id"))
                    .and_then(serde_yaml::Value::as_str)
                    .map(str::to_string)
            });
            out.push(CoverageUnit {
                file,
                canonical,
                id: None,
                local: scene.clone().unwrap_or_default(),
                doc: doc_id,
                kind: if !doc.quests.is_empty() {
                    "quest"
                } else if scene.is_some() {
                    "scene"
                } else {
                    "document"
                },
                beat: meta.is_some_and(|m| m.contains_key("on")),
            });
            continue;
        }
        // Entries and bundle beats in source order, as the index lists them.
        let bundle = lute_check::connectivity::bundle_id(item.meta);
        let mut units: Vec<(usize, CoverageUnit)> = doc
            .entries
            .iter()
            .map(|e| {
                (
                    e.span.byte_start,
                    CoverageUnit {
                        file: file.clone(),
                        canonical: canonical.clone(),
                        id: Some(e.id.clone()),
                        local: e.id.clone(),
                        doc: bundle.clone(),
                        kind: "entry",
                        beat: e.on.is_some(),
                    },
                )
            })
            .chain(doc.beats.iter().map(|b| {
                let id = match &bundle {
                    Some(d) => lute_check::bundle_beat_key(d, &b.id),
                    None => b.id.clone(),
                };
                (
                    b.span.byte_start,
                    CoverageUnit {
                        file: file.clone(),
                        canonical: canonical.clone(),
                        id: Some(id),
                        local: b.id.clone(),
                        doc: bundle.clone(),
                        kind: "beat",
                        beat: b.on.is_some(),
                    },
                )
            }))
            .collect();
        units.sort_by_key(|(at, _)| *at);
        out.extend(units.into_iter().map(|(_, u)| u));
    }
    Ok(out)
}

/// A path in the one spelling both sides of the untested-set difference can
/// agree on. `TraceReport.file` comes from `base.join(&rel)` — for
/// `tests/../scenes/wake.lute` that is NOT what `find_lute_files` yields — so
/// the difference would report every document as untested without this.
/// Canonical paths are absolute and machine-specific and are used for the
/// comparison ONLY; the printed list keeps the walk's own display paths.
pub(super) fn canonical_key(p: &std::path::Path) -> String {
    std::fs::canonicalize(p)
        .unwrap_or_else(|_| p.to_path_buf())
        .display()
        .to_string()
}

/// A path as the report prints it (T3-23): relative to the directory
/// `lute test` runs in — `scenes/a.lute`, `.` for that directory itself —
/// whatever spelling reached it (a test's `file: ../scenes/a.lute`, a
/// play's project root, the nearest manifest's absolute directory); a path
/// outside that directory keeps its `..`-folded spelling.
pub(super) fn display_path(p: &Path) -> String {
    static CWD: std::sync::LazyLock<Option<PathBuf>> =
        std::sync::LazyLock::new(|| std::env::current_dir().and_then(std::fs::canonicalize).ok());
    let rel = CWD.as_deref().and_then(|cwd| {
        let abs = std::fs::canonicalize(p).ok()?;
        abs.strip_prefix(cwd).ok().map(Path::to_path_buf)
    });
    match rel {
        Some(r) if r.as_os_str().is_empty() => ".".to_string(),
        Some(r) => r.display().to_string(),
        None => fold_parent_dirs(p).display().to_string(),
    }
}
