//! `lute loc` — localization export/import and production word-count reporting.
//!
//! Three surfaces over a project's `.lute` documents. `export` and `report` are
//! read-only walks built on the SAME deterministic file walk `check-project`
//! uses ([`crate::find_lute_files`] — byte-sorted, symlink-deduped) and the SAME
//! syntax-layer parse the checker runs ([`lute_syntax::parse`]). Neither
//! validates: a document that fails to parse (any `Error`-severity parse
//! diagnostic — the exact guard [`lute_check::tag_document`] uses before it will
//! rewrite) is reported to stderr and SKIPPED, never crashed on.
//! `lute_syntax::parse` itself never panics (best-effort AST + diagnostics), so
//! the skip is a policy choice, not a panic guard. `import` reads back what
//! `export` wrote and touches no `.lute` file at all.
//!
//! ## Translatable units (`export`)
//! Two kinds, both walked in document order (descending into `<branch>`/`<hub>`
//! choice bodies, `<match>` arms, `<objective>`/`<on>` bodies, and quest bodies
//! — mirroring `lute-check`'s own `collect_lines`):
//! - **content lines** (`@speaker: text`, dsl §7.1) — `file`, `line`, `lineId`,
//!   the stable `code` (dsl §12; `null` when the line carries no `code="…"`
//!   string attr, i.e. it has not been through `lute tag`), `speaker`, and
//!   `text`.
//! - **choice / hub labels** (dsl §7.3.1/§7.3.2) — `file`, `line`, `lineId`, the
//!   `key` `{branchOrHubId}.{choiceId}`, and the `label` text.
//!
//! ### `lineId` — the join that makes the round trip possible
//! `lineId` is the stable content join `lute-compile`'s addressing pass stamps
//! on every line and option record (`address.rs`), and the ONLY key
//! `compile --locales` can merge a translation back onto (`addr` is a
//! REGENERATED position, spec §4.2/§12). It is reproduced here at the SYNTAX
//! layer from exactly the same three inputs the addressing pass uses:
//! - the identity PREFIX — a scene's `{character}.{episodeId}`
//!   ([`canonical_episode_key`], the shared implementation compile's own prefix
//!   join calls), or the enclosing `<quest id>` for a quest document;
//! - the line's authored `code` and speaker, rendered through the project's
//!   [`IdentityTemplates`] (dsl 0.8.0 §9) — never a hardcoded shape, so a
//!   project that retemplated `lineId` exports the ids it actually compiles;
//! - a choice/hub option's structural `{prefix}.{branchOrHubId}.{optionId}`,
//!   which the spec fixes and does NOT template.
//!
//! `lineId` is `null` exactly when it cannot be reproduced faithfully: a line
//! with no authored `code` (the addressing pass BACK-FILLS one, and that
//! back-fill is a property of the post-expansion command stream, not of the
//! source text). Run `lute tag` first and those become real ids — which is
//! exactly what the untagged-line advisory below already tells you to do, and
//! it is now true of every null row without exception.
//!
//! ### `::use` is expanded before extraction (0.10.0, #3)
//! A component has no frontmatter and therefore no `{prefix}`, so exporting
//! its lines under their own file produced rows with `lineId: null` that
//! carried a `code` — rows `lute tag` could never fix, which `loc import`
//! skipped and which `compile --locales` then reported as `W-L10N-MISSING`
//! under a caller-derived id no export contained. Adopting the language's
//! only reuse mechanism removed a line from the localization pipeline.
//!
//! The walk therefore runs [`lute_compile::normalize::normalize_document`]
//! first — the same pass `lute trace` and `lute compile` run — so a
//! component's lines are extracted once PER CALL SITE, under the caller's
//! prefix and with its `@param`s bound. The component FILE and line ride
//! along in a `source` field, so a TMS dedupes identical source text and the
//! translator still sees the string once. A `component:`-declaring file
//! contributes no rows of its own. `expand_document` is deliberately NOT run:
//! `{{…}}` interpolation is what a translator must see intact.
//!
//! The export array is sorted by (`file`, the `::use` site's byte offset in
//! the CALLER, the unit's own byte offset) so it is byte-identical across
//! runs regardless of directory-iteration order, and a whole expansion sorts
//! where its invocation sits. `--format json`
//! (default) emits a stable JSON array; `--format csv` emits an RFC-4180 file
//! (header row, minimal quoting). An unknown format is a usage error (exit 2).
//! `-o <FILE>` writes the export there; otherwise it goes to stdout. When any
//! exported content line is untagged, a single `N lines untagged — run lute tag`
//! summary is written to stderr (advisory; never changes the exit code).
//!
//! ## `import` — the reverse direction
//! [`run_import`] canonicalizes one translated export PER LOCALE into a locale
//! bundle ([`LocaleBundle`]); see its doc comment for the accepted input shapes
//! and the `E-LOCALE-BUNDLE` rules.
//!
//! ## Word/line report (`report`)
//! Per-document and per-speaker word counts, total lines, tagged-vs-untagged
//! line counts, and choice-label counts, plus project-wide totals. `--json`
//! emits a stable object; otherwise aligned human tables.
//!
//! ### Word-counting rule
//! A content line's word count is computed from its `text` by first REMOVING the
//! `{{` and `}}` interpolation delimiters (dsl §7.6) — the interior referent
//! text is kept in place — then splitting on Unicode whitespace and counting
//! each maximal run of non-whitespace characters as one word. So
//! `Hello {{@player.name}}!` counts as two words (`Hello`, `@player.name}}`→
//! `@player.name!`). Choice/hub labels are counted as units (their count), not
//! folded into the word totals.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lute_check::meta::canonical_episode_key;
use lute_compile::locale::LocaleBundle;
use lute_core_span::Severity;
use lute_manifest::project::{load_project, IdentityTemplates};
use lute_syntax::ast::{Arm, Attr, AttrValue, Choice, Document, Node};

/// One translatable unit extracted from a document, carrying the byte offset
/// used to sort the export deterministically.
enum Unit {
    Line {
        file: String,
        line: u32,
        byte: usize,
        outer_byte: usize,
        source: Option<(String, u32)>,
        /// The compile-stable content join, or `None` when it cannot be
        /// reproduced from source alone (module doc, "`lineId`").
        line_id: Option<String>,
        code: Option<String>,
        speaker: String,
        text: String,
    },
    Choice {
        file: String,
        line: u32,
        byte: usize,
        outer_byte: usize,
        source: Option<(String, u32)>,
        line_id: Option<String>,
        key: String,
        label: String,
    },
}

impl Unit {
    fn file(&self) -> &str {
        match self {
            Unit::Line { file, .. } | Unit::Choice { file, .. } => file,
        }
    }
    fn byte(&self) -> usize {
        match self {
            Unit::Line { byte, .. } | Unit::Choice { byte, .. } => *byte,
        }
    }
    /// Primary sort key: the byte offset in the CALLER. For an authored unit
    /// this is its own offset; for one expanded out of a component it is the
    /// `::use` site's, so the whole expansion sorts where the invocation is
    /// and [`Unit::byte`] — an offset into the COMPONENT file, which would
    /// otherwise interleave nonsensically among the caller's — only breaks
    /// ties within it.
    fn outer_byte(&self) -> usize {
        match self {
            Unit::Line { outer_byte, .. } | Unit::Choice { outer_byte, .. } => *outer_byte,
        }
    }
    /// The component file and line this unit was expanded out of, if any.
    fn source(&self) -> Option<&(String, u32)> {
        match self {
            Unit::Line { source, .. } | Unit::Choice { source, .. } => source.as_ref(),
        }
    }
}

/// The component region a walk is currently inside: the component's file path
/// and the byte offset of the `::use` site IN THE CALLER (the sentinel's own
/// span, `normalize.rs:290-296`). `None` outside any region.
type Region<'a> = Option<(&'a str, usize)>;

/// Everything one document's walk needs to stamp a `lineId`: the display path,
/// the identity PREFIX in force for the sub-tree being walked (`None` when the
/// document has none — a schema fragment), the project's
/// [`IdentityTemplates`], and the resolved component table the
/// `__component-begin` sentinels name. Carried by reference so the recursive
/// walk allocates nothing per node.
struct Cx<'a> {
    file: &'a str,
    prefix: Option<&'a str>,
    templates: &'a IdentityTemplates,
    components: &'a lute_check::ComponentSet,
}

/// A line's authored stable `code` (dsl §12), trimmed to the exact string the
/// addressing pass keys `lineId`/`voiceKey` on — mirrors
/// `lute-check`'s own `authored_code`. `None` when the line has no `code`, or
/// its `code` is not a string literal (an `@ref`/bare value is not a stable
/// code).
fn line_code(attrs: &[Attr]) -> Option<String> {
    attrs
        .iter()
        .find(|a| a.key == "code")
        .and_then(|a| match &a.value {
            AttrValue::Str(s) => Some(s.trim().to_string()),
            _ => None,
        })
}

/// A string-valued attribute (used for a `<hub>`'s `id`, which — unlike a
/// `<branch>` — has no dedicated AST field, dsl §7.3.2).
fn attr_str<'a>(attrs: &'a [Attr], key: &str) -> Option<&'a str> {
    attrs.iter().find(|a| a.key == key).and_then(|a| match &a.value {
        AttrValue::Str(s) => Some(s.as_str()),
        _ => None,
    })
}

/// Push one choice/hub label unit plus recurse into its body. An option's
/// identity is the STRUCTURAL `{prefix}.{branchOrHubId}.{optionId}` the
/// addressing pass writes verbatim (`address.rs`'s `Choice`/`Hub` arms) — dsl
/// 0.8.0 §9 templates the LINE join only, never this one.
fn walk_choice<'a>(
    cx: &Cx<'a>,
    group_id: &str,
    choice: &Choice,
    region: Region<'a>,
    out: &mut Vec<Unit>,
) {
    let key = format!("{group_id}.{}", choice.id);
    out.push(Unit::Choice {
        file: cx.file.to_string(),
        line: choice.span.line,
        byte: choice.span.byte_start,
        outer_byte: region.map_or(choice.span.byte_start, |(_, at)| at),
        source: region.map(|(f, _)| (f.to_string(), choice.span.line)),
        line_id: cx.prefix.map(|p| format!("{p}.{key}")),
        key,
        label: choice.label.clone(),
    });
    walk_nodes(cx, &choice.body, region, out);
}

/// Recursively collect translatable units from a node stream in document order
/// (mirrors `lute-check`'s `collect_lines` descent).
///
/// `region` is the component expansion the stream sits inside, if any. The
/// stream is post-`normalize_document`, so an expansion appears INLINE as
/// `__component-begin`, the bound body, `__component-end` — siblings in this
/// very list, and nestable, hence the local stack rather than a recursion.
/// Every other descent forwards `region` unchanged.
fn walk_nodes<'a>(cx: &Cx<'a>, nodes: &[Node], region: Region<'a>, out: &mut Vec<Unit>) {
    let mut region = region;
    let mut stack: Vec<Region<'a>> = Vec::new();
    for node in nodes {
        match node {
            Node::Directive(d) if d.tag == lute_compile::normalize::COMPONENT_BEGIN => {
                let name = attr_str(&d.attrs, "component").unwrap_or("");
                // `'a`-bound, never borrowed from `nodes`: the fallback is
                // a literal, not `name`, so the region outlives this node.
                let src: &'a str = cx
                    .components
                    .table
                    .get(name)
                    .and_then(|def| def.src.to_str())
                    .unwrap_or("<unresolved component>");
                stack.push(region);
                // The FILE is the innermost component — that is where the line
                // is written. The OFFSET stays the outermost `::use` site,
                // because only that one is an offset into the caller; a nested
                // `::use`'s span points into the enclosing component file and
                // would sort the inner expansion nowhere near its invocation.
                region = Some((src, region.map_or(d.span.byte_start, |(_, at)| at)));
            }
            Node::Directive(d) if d.tag == lute_compile::normalize::COMPONENT_END => {
                region = stack.pop().flatten();
            }
            Node::Line(l) => {
                let code = line_code(&l.attrs);
                // Both halves must be known: an untagged line's code is
                // BACK-FILLED by the addressing pass from the post-expansion
                // command stream, which no source-only walk can reproduce.
                let line_id = match (cx.prefix, &code) {
                    (Some(prefix), Some(code)) => {
                        Some(cx.templates.render_line_id(prefix, &l.speaker, code))
                    }
                    _ => None,
                };
                out.push(Unit::Line {
                    file: cx.file.to_string(),
                    line: l.span.line,
                    byte: l.span.byte_start,
                    outer_byte: region.map_or(l.span.byte_start, |(_, at)| at),
                    // The node is a CLONE of the component's own, so its span
                    // is the component file's position — exactly the line
                    // `source` wants.
                    source: region.map(|(f, _)| (f.to_string(), l.span.line)),
                    line_id,
                    code,
                    speaker: l.speaker.clone(),
                    text: l.text.clone(),
                });
            }
            Node::Branch(b) => {
                for choice in &b.choices {
                    walk_choice(cx, &b.id, choice, region, out);
                }
            }
            Node::Hub(h) => {
                let id = attr_str(&h.attrs, "id").unwrap_or("");
                for choice in &h.choices {
                    walk_choice(cx, id, choice, region, out);
                }
            }
            Node::Match(m) => {
                for arm in &m.arms {
                    match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => {
                            walk_nodes(cx, body, region, out)
                        }
                    }
                }
            }
            Node::Objective(o) => walk_nodes(cx, &o.body, region, out),
            Node::On(o) => walk_nodes(cx, &o.body, region, out),
            Node::Directive(_) | Node::Set(_) | Node::Timeline(_) => {}
            Node::Assert(_) | Node::Retract(_) => {}
        }
    }
}

/// A scene document's identity prefix: `{character}.{episodeId}` via the SHARED
/// [`canonical_episode_key`] `lute-compile`'s own prefix join and
/// `check-project`'s scene-key grouping both call. `None` when the frontmatter
/// carries no usable `character`/`season`/`episode` triad — a quest document
/// (whose prefix is per-`<quest>`), a component/schema fragment, or a scene too
/// broken to identify. Reads the raw mapping directly (mirroring
/// `connectivity.rs`'s own `scene_identity`) because `episodeId` is never
/// lifted into `TypedMeta`.
fn scene_prefix(doc: &Document) -> Option<String> {
    let value: serde_yaml::Value = serde_yaml::from_str(&doc.meta.raw_yaml).ok()?;
    let map = match value {
        serde_yaml::Value::Mapping(m) => m,
        _ => return None,
    };
    let key = |k: &str| serde_yaml::Value::String(k.to_string());
    let character = map.get(key("character"))?.as_str()?.to_string();
    if character.is_empty() {
        return None;
    }
    let season = map.get(key("season"))?.as_i64()?;
    let episode = map.get(key("episode"))?.as_i64()?;
    let episode_id = map.get(key("episodeId")).and_then(|v| v.as_str());
    Some(canonical_episode_key(&character, season, episode, episode_id))
}

/// Collect every translatable unit from one parsed document. A scene's shots
/// all share ONE document-wide prefix (compile folds them into a single
/// identity scope); each `<quest>` is its OWN scope prefixed by its id (IR
/// addendum §4) — mirroring `address.rs`'s two `ShotRecords.prefix` callers
/// exactly.
fn document_units(
    file: &str,
    doc: &Document,
    templates: &IdentityTemplates,
    components: &lute_check::ComponentSet,
    out: &mut Vec<Unit>,
) {
    let scene = scene_prefix(doc);
    let cx = Cx {
        file,
        prefix: scene.as_deref(),
        templates,
        components,
    };
    for shot in &doc.shots {
        walk_nodes(&cx, &shot.body, None, out);
    }
    for quest in &doc.quests {
        let cx = Cx {
            file,
            prefix: Some(&quest.id),
            templates,
            components,
        };
        walk_nodes(&cx, &quest.body, None, out);
    }
}

/// A component declaration file (`component: <name>` in its frontmatter). It
/// has no `{character}.{episodeId}` triad and therefore no identity prefix,
/// so it is not a document for export purposes — its lines are exported once
/// per `::use` expansion, under the caller's prefix (#3, T6.10).
fn is_component_document(doc: &Document) -> bool {
    serde_yaml::from_str::<serde_yaml::Value>(&doc.meta.raw_yaml)
        .ok()
        .and_then(|v| v.get("component").cloned())
        .is_some()
}

/// The `identity:` templates (dsl 0.8.0 §9) in force for `root`, loaded once
/// per resolved root and cached. A root with no (or an unreadable)
/// `lute.project.yaml` gets [`IdentityTemplates::default`] — the 0.7.0 shapes,
/// byte-for-byte. `load_project`'s own `E-IDENTITY-TEMPLATE` reporting belongs
/// to `check`/`compile`; `loc` is a read-only extraction surface and stays
/// silent, using whatever the loader resolved (a rejected template is already
/// reset to its default there).
fn templates_for<'a>(
    root: &Path,
    cache: &'a mut BTreeMap<PathBuf, IdentityTemplates>,
) -> &'a IdentityTemplates {
    cache.entry(root.to_path_buf()).or_insert_with(|| {
        load_project(root)
            .ok()
            .flatten()
            .map(|p| p.identity)
            .unwrap_or_default()
    })
}

/// Parse every `.lute` under `dir` (skipping — with a stderr note — any file
/// whose parse produces an `Error`-severity diagnostic) and collect all
/// translatable units, sorted by (`file`, byte offset). `Err(2)` on a walk or
/// read I/O failure. `String` display paths are the byte-sorted walk paths, so
/// the whole result is deterministic.
///
/// Each file's `identity:` templates resolve against its OWN nearest-ancestor
/// project root ([`crate::project_root_for`], bounded below by `dir`) — the
/// same nested-subproject rule `check-project` and `lute scenario` use, so a
/// walk spanning two subprojects exports each one's real ids.
fn collect_units(dir: &Path) -> Result<Vec<Unit>, ExitCode> {
    let files = crate::find_lute_files(dir).map_err(|e| {
        eprintln!("lute loc: cannot walk {}: {e}", dir.display());
        ExitCode::from(2)
    })?;
    let mut units = Vec::new();
    let mut templates: BTreeMap<PathBuf, IdentityTemplates> = BTreeMap::new();
    for path in &files {
        // #3 / T6.10 fix (i): expand `::use` before extracting, so a
        // component's lines are exported once PER CALL SITE under the
        // caller's identity prefix — the id `compile --locales` actually
        // merges on. This is `lute-trace`'s own pipeline (`walk.rs`'s
        // `trace_document`), in the same order, with the same three passes;
        // `expand_document` is deliberately NOT run, because `{{…}}`
        // interpolation is what a translator must see intact.
        let root = crate::project_root_for(path, dir);
        let Some(built) = crate::build_input(path, None, Some(&root)) else {
            eprintln!("lute loc: skipping {} — cannot resolve inputs", path.display());
            continue;
        };
        let input = built.input;
        let (mut doc, diags) = lute_syntax::parse(&input.text);
        let errors = diags.iter().filter(|d| d.severity == Severity::Error).count();
        if errors > 0 {
            eprintln!(
                "lute loc: skipping {} — parse failed ({errors} error(s))",
                path.display()
            );
            continue;
        }
        // A component file has no identity prefix of its own — that is the
        // whole defect — so it is not a document here. Its lines reach the
        // export through every caller that expands it.
        if is_component_document(&doc) {
            continue;
        }
        let mut arena = lute_cel::CelArena::default();
        let _ = lute_cel::fill_document(&mut arena, &mut doc);
        let (folded, _meta_diags, _cel_diags) = lute_check::fold_env(&doc, &input);
        let _ = lute_compile::normalize::normalize_document(
            &mut doc,
            &input.components,
            &folded.env.state,
        );
        let ident = templates_for(&root, &mut templates).clone();
        document_units(
            &path.display().to_string(),
            &doc,
            &ident,
            &input.components,
            &mut units,
        );
    }
    // The primary key is the offset IN THE CALLER, so a whole expansion sorts
    // where its `::use` sits; the unit's own offset (an offset into the
    // component file, for an expanded unit) only orders it within that.
    units.sort_by(|a, b| {
        a.file()
            .cmp(b.file())
            .then(a.outer_byte().cmp(&b.outer_byte()))
            .then(a.byte().cmp(&b.byte()))
    });
    Ok(units)
}

/// Extract translatable lines to a localization export. See [`crate::LocCommand::Export`].
pub fn run_export(dir: &Path, format: &str, out: Option<&Path>) -> ExitCode {
    if format != "json" && format != "csv" {
        eprintln!("lute loc export: unknown format `{format}` (expected `json` or `csv`)");
        return ExitCode::from(2);
    }
    let units = match collect_units(dir) {
        Ok(u) => u,
        Err(code) => return code,
    };

    let untagged = units
        .iter()
        .filter(|u| matches!(u, Unit::Line { code: None, .. }))
        .count();

    let rendered = match format {
        "json" => render_json(&units),
        _ => render_csv(&units),
    };

    let write_result = match out {
        Some(path) => std::fs::write(path, rendered.as_bytes()).map_err(|e| {
            eprintln!("lute loc export: cannot write {}: {e}", path.display());
        }),
        None => crate::write_stdout(&rendered).map_err(|_| {}),
    };
    if write_result.is_err() {
        return ExitCode::from(2);
    }

    if untagged > 0 {
        eprintln!("{untagged} lines untagged — run lute tag");
    }
    ExitCode::SUCCESS
}

/// Serialize the export as a stable JSON array (object keys are emitted in
/// `serde_json`'s sorted order — deterministic). `lineId` is `null` on a row
/// whose identity cannot be reproduced from source (module doc); every other
/// field is verbatim.
///
/// `source` is present on EVERY row, `null` when the line was authored in the
/// document itself, so the shape does not vary with authoring.
fn render_json(units: &[Unit]) -> String {
    let source_of = |u: &Unit| match u.source() {
        Some((file, line)) => serde_json::json!({ "file": file, "line": line }),
        None => serde_json::Value::Null,
    };
    let arr: Vec<serde_json::Value> = units
        .iter()
        .map(|u| match u {
            Unit::Line {
                file,
                line,
                line_id,
                code,
                speaker,
                text,
                ..
            } => serde_json::json!({
                "kind": "line",
                "file": file,
                "line": line,
                "lineId": line_id,
                "code": code,
                "speaker": speaker,
                "text": text,
                "source": source_of(u),
            }),
            Unit::Choice {
                file,
                line,
                line_id,
                key,
                label,
                ..
            } => serde_json::json!({
                "kind": "choice",
                "file": file,
                "line": line,
                "lineId": line_id,
                "key": key,
                "label": label,
                "source": source_of(u),
            }),
        })
        .collect();
    let mut s = serde_json::to_string_pretty(&serde_json::Value::Array(arr))
        .expect("Value -> JSON serialization is infallible");
    s.push('\n');
    s
}

/// One RFC-4180 field: quote when it contains a comma, quote, CR, or LF;
/// escape an embedded quote by doubling it.
fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\r', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// The RFC-4180 column schema, shared by [`render_csv`] and the import parser
/// so a header rename can never desynchronize the two directions. ONE schema
/// covers both unit kinds: a line row leaves `key` empty, a choice row leaves
/// `code`/`speaker` empty and carries its label in `text`. The two `source*`
/// columns are empty for a line the document authored itself.
pub(crate) const CSV_COLUMNS: [&str; 10] = [
    "kind",
    "file",
    "line",
    "lineId",
    "code",
    "speaker",
    "key",
    "text",
    "sourceFile",
    "sourceLine",
];

/// Serialize the export as RFC-4180 CSV over [`CSV_COLUMNS`]. `lineId` is empty
/// on a row whose identity cannot be reproduced from source (module doc) —
/// CSV has no `null`, and an empty cell is the same signal the import side
/// reads.
fn render_csv(units: &[Unit]) -> String {
    let mut s = CSV_COLUMNS.join(",");
    s.push_str("\r\n");
    for u in units {
        let (source_file, source_line) = match u.source() {
            Some((file, line)) => (file.clone(), line.to_string()),
            None => (String::new(), String::new()),
        };
        let row = match u {
            Unit::Line {
                file,
                line,
                line_id,
                code,
                speaker,
                text,
                ..
            } => [
                "line".to_string(),
                file.clone(),
                line.to_string(),
                line_id.clone().unwrap_or_default(),
                code.clone().unwrap_or_default(),
                speaker.clone(),
                String::new(),
                text.clone(),
                source_file,
                source_line,
            ],
            Unit::Choice {
                file,
                line,
                line_id,
                key,
                label,
                ..
            } => [
                "choice".to_string(),
                file.clone(),
                line.to_string(),
                line_id.clone().unwrap_or_default(),
                String::new(),
                String::new(),
                key.clone(),
                label.clone(),
                source_file,
                source_line,
            ],
        };
        let cells: Vec<String> = row.iter().map(|c| csv_field(c)).collect();
        s.push_str(&cells.join(","));
        s.push_str("\r\n");
    }
    s
}

/// Count words in a content line's `text` per the module's documented rule:
/// remove the `{{`/`}}` interpolation delimiters, then count whitespace-split
/// non-empty tokens.
fn word_count(text: &str) -> usize {
    text.replace("{{", "").replace("}}", "").split_whitespace().count()
}

/// Per-speaker accumulator within one document (or project-wide).
#[derive(Default, Clone)]
struct SpeakerStat {
    lines: usize,
    words: usize,
}

/// One document's aggregated report row.
#[derive(Default)]
struct DocStat {
    lines: usize,
    tagged: usize,
    untagged: usize,
    words: usize,
    choices: usize,
    speakers: BTreeMap<String, SpeakerStat>,
}

/// Word/line-count report per document and speaker. See [`crate::LocCommand::Report`].
pub fn run_report(dir: &Path, json: bool) -> ExitCode {
    let units = match collect_units(dir) {
        Ok(u) => u,
        Err(code) => return code,
    };

    // Aggregate per document (BTreeMap keeps documents in stable path order).
    let mut docs: BTreeMap<String, DocStat> = BTreeMap::new();
    let mut totals = DocStat::default();
    for u in &units {
        let stat = docs.entry(u.file().to_string()).or_default();
        match u {
            Unit::Line {
                code, speaker, text, ..
            } => {
                let words = word_count(text);
                stat.lines += 1;
                stat.words += words;
                if code.is_some() {
                    stat.tagged += 1;
                } else {
                    stat.untagged += 1;
                }
                let sp = stat.speakers.entry(speaker.clone()).or_default();
                sp.lines += 1;
                sp.words += words;

                totals.lines += 1;
                totals.words += words;
                if code.is_some() {
                    totals.tagged += 1;
                } else {
                    totals.untagged += 1;
                }
                let tsp = totals.speakers.entry(speaker.clone()).or_default();
                tsp.lines += 1;
                tsp.words += words;
            }
            Unit::Choice { .. } => {
                stat.choices += 1;
                totals.choices += 1;
            }
        }
    }

    let rendered = if json {
        render_report_json(&docs, &totals)
    } else {
        render_report_human(&docs, &totals)
    };
    if crate::write_stdout(&rendered).is_err() {
        return ExitCode::from(2);
    }
    ExitCode::SUCCESS
}

/// Serialize the report as a stable JSON object.
fn render_report_json(docs: &BTreeMap<String, DocStat>, totals: &DocStat) -> String {
    let speakers_json = |speakers: &BTreeMap<String, SpeakerStat>| -> Vec<serde_json::Value> {
        speakers
            .iter()
            .map(|(name, s)| {
                serde_json::json!({ "speaker": name, "lines": s.lines, "words": s.words })
            })
            .collect()
    };
    let documents: Vec<serde_json::Value> = docs
        .iter()
        .map(|(file, d)| {
            serde_json::json!({
                "file": file,
                "lines": d.lines,
                "tagged": d.tagged,
                "untagged": d.untagged,
                "words": d.words,
                "choices": d.choices,
                "speakers": speakers_json(&d.speakers),
            })
        })
        .collect();
    let value = serde_json::json!({
        "documents": documents,
        "totals": {
            "documents": docs.len(),
            "lines": totals.lines,
            "tagged": totals.tagged,
            "untagged": totals.untagged,
            "words": totals.words,
            "choices": totals.choices,
            "speakers": speakers_json(&totals.speakers),
        },
    });
    let mut s =
        serde_json::to_string_pretty(&value).expect("Value -> JSON serialization is infallible");
    s.push('\n');
    s
}

/// Render the report as aligned human tables: a per-document summary, a
/// per-speaker project-wide summary, and a totals line.
fn render_report_human(docs: &BTreeMap<String, DocStat>, totals: &DocStat) -> String {
    let mut s = String::new();

    // Per-document table.
    let file_w = docs
        .keys()
        .map(|f| f.len())
        .chain(std::iter::once("document".len()))
        .max()
        .unwrap_or(8);
    s.push_str(&format!(
        "{:<file_w$}  {:>6}  {:>6}  {:>8}  {:>6}  {:>7}\n",
        "document", "lines", "words", "untagged", "tagged", "choices"
    ));
    for (file, d) in docs {
        s.push_str(&format!(
            "{:<file_w$}  {:>6}  {:>6}  {:>8}  {:>6}  {:>7}\n",
            file, d.lines, d.words, d.untagged, d.tagged, d.choices
        ));
    }
    s.push_str(&format!(
        "{:<file_w$}  {:>6}  {:>6}  {:>8}  {:>6}  {:>7}\n",
        "TOTAL", totals.lines, totals.words, totals.untagged, totals.tagged, totals.choices
    ));

    // Per-speaker (project-wide) table.
    if !totals.speakers.is_empty() {
        let sp_w = totals
            .speakers
            .keys()
            .map(|n| n.len())
            .chain(std::iter::once("speaker".len()))
            .max()
            .unwrap_or(7);
        s.push('\n');
        s.push_str(&format!("{:<sp_w$}  {:>6}  {:>6}\n", "speaker", "lines", "words"));
        for (name, sp) in &totals.speakers {
            s.push_str(&format!("{:<sp_w$}  {:>6}  {:>6}\n", name, sp.lines, sp.words));
        }
    }

    s.push_str(&format!(
        "\n{} document(s), {} line(s), {} word(s), {} choice(s)\n",
        docs.len(),
        totals.lines,
        totals.words,
        totals.choices
    ));
    s
}

// ===========================================================================
// `lute loc import` (dsl 0.8.0 §7) — the reverse of `export`.
// ===========================================================================

/// A translation round trip is a round TRIP: `export` extracted content and
/// nothing brought it back. This canonicalizes translated exports into the
/// locale bundle `lute compile --locales` merges.
pub const E_LOCALE_BUNDLE: &str = "E-LOCALE-BUNDLE";

/// One parsed input row: the join key and the translated string.
struct ImportRow {
    /// `None`/empty when the source row carries no reproducible `lineId`.
    line_id: Option<String>,
    /// A per-row locale override; `None` falls back to the file's own tag.
    locale: Option<String>,
    text: String,
    /// 1-based position for error messages (a CSV record index including the
    /// header row, or a JSON array index + 1).
    row: usize,
}

/// Canonicalize translated exports into a locale bundle. See
/// [`crate::LocCommand::Import`].
///
/// ## Accepted input — exactly what `export` emits
/// Both of `loc export`'s own output formats, chosen by file extension
/// (`.csv` → CSV, anything else → JSON):
/// - **JSON** — the array [`render_json`] writes. A `line` row's translation is
///   its `text`, a `choice` row's is its `label`; the join key is `lineId`.
/// - **CSV** — the [`CSV_COLUMNS`] table [`render_csv`] writes, read BY HEADER
///   NAME (so column order is free). Only `lineId` and `text` are required —
///   in CSV both unit kinds already carry their translatable string in `text`.
///
/// ## Where the locale tag comes from
/// `export` emits no locale (it extracts the SOURCE language), so the normal
/// workflow is **one file per locale**: copy the export to `ja-JP.json`,
/// translate the `text`/`label` values, and the file STEM is the locale tag.
/// A single merged file is also accepted: any row carrying a non-empty
/// `locale` field (JSON) or `locale` column (CSV) overrides the stem for that
/// row, so one file may span every locale.
///
/// ## `E-LOCALE-BUNDLE` (exit 1)
/// A file that is not valid JSON/CSV or does not match the shape above; a
/// `lineId` appearing twice within ONE locale (which translation is the real
/// one is unanswerable); or an empty locale tag. Every defect found is
/// reported, each naming the offending file and id/row — never just the first.
///
/// A row with NO `lineId` is skipped, not rejected: an untagged line simply has
/// no stable identity yet, and a single stderr summary points at `lute tag`
/// exactly as `export`'s own untagged advisory does.
///
/// Exit `0` on success, `1` on `E-LOCALE-BUNDLE`, `2` on an I/O failure.
pub fn run_import(files: &[PathBuf], out: Option<&Path>) -> ExitCode {
    let mut errors = 0usize;
    let mut skipped = 0usize;
    let mut triples: Vec<(String, String, String)> = Vec::new();
    // `(locale, lineId)` already claimed — the duplicate guard is per LOCALE,
    // so the same id legitimately recurs once per translated language.
    let mut seen: BTreeSet<(String, String)> = BTreeSet::new();

    for file in files {
        let text = match std::fs::read_to_string(file) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("lute loc import: cannot read {}: {e}", file.display());
                return ExitCode::from(2);
            }
        };
        let is_csv = file
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("csv"));
        let parsed = if is_csv {
            parse_csv_rows(&text)
        } else {
            parse_json_rows(&text)
        };
        let rows = match parsed {
            Ok(rows) => rows,
            Err(msg) => {
                bundle_error(file, &msg);
                errors += 1;
                continue;
            }
        };

        // The file's default locale tag, used only by rows that DECLARE none —
        // so a fully `locale`-columned file needs no meaningful name.
        let stem = file
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();

        for row in rows {
            // Keys are trimmed (values never are — leading space in a
            // translation is the translator's, not noise): a spreadsheet round
            // trip routinely pads cells, and a padded key would silently join
            // to nothing.
            let Some(line_id) = row
                .line_id
                .map(|id| id.trim().to_string())
                .filter(|id| !id.is_empty())
            else {
                skipped += 1;
                continue;
            };
            // A row that DECLARES a locale is held to it, empty included — the
            // stem is a fallback for rows with no locale field at all, never a
            // silent repair for one the input got wrong.
            let locale = row
                .locale
                .map(|l| l.trim().to_string())
                .unwrap_or_else(|| stem.trim().to_string());
            if locale.is_empty() {
                bundle_error(
                    file,
                    &format!(
                        "row {} has an empty locale tag — give the row a `locale` value, or \
                         name the file after its locale (e.g. `ja-JP.json`)",
                        row.row
                    ),
                );
                errors += 1;
                continue;
            }
            if !seen.insert((locale.clone(), line_id.clone())) {
                bundle_error(
                    file,
                    &format!(
                        "row {}: duplicate `{line_id}` for locale `{locale}` — one lineId \
                         carries exactly one translation per locale",
                        row.row
                    ),
                );
                errors += 1;
                continue;
            }
            triples.push((line_id, locale, row.text));
        }
    }

    if errors > 0 {
        eprintln!("{errors} {E_LOCALE_BUNDLE} error(s); no bundle emitted");
        return ExitCode::FAILURE;
    }

    let rendered = LocaleBundle::from_triples(triples).to_json();
    let write_result = match out {
        Some(path) => std::fs::write(path, rendered.as_bytes()).map_err(|e| {
            eprintln!("lute loc import: cannot write {}: {e}", path.display());
        }),
        None => crate::write_stdout(&rendered).map_err(|_| {}),
    };
    if write_result.is_err() {
        return ExitCode::from(2);
    }

    if skipped > 0 {
        eprintln!("{skipped} rows skipped (no lineId) — run lute tag, then re-export");
    }
    ExitCode::SUCCESS
}

/// One `E-LOCALE-BUNDLE` line, naming the offending file. Mirrors
/// `print_diagnostics`'s `severity [CODE] message` tail; the head is the file
/// rather than a `line:column` because an export row has no source span (the
/// row number lives in `message` instead).
fn bundle_error(file: &Path, message: &str) {
    eprintln!("{}: error [{E_LOCALE_BUNDLE}] {message}", file.display());
}

/// Parse `loc export --format json`'s array back into rows.
fn parse_json_rows(text: &str) -> Result<Vec<ImportRow>, String> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("not valid JSON: {e}"))?;
    let arr = value
        .as_array()
        .ok_or_else(|| "root must be an array of export rows".to_string())?;
    let mut rows = Vec::with_capacity(arr.len());
    for (i, entry) in arr.iter().enumerate() {
        let row = i + 1;
        let obj = entry
            .as_object()
            .ok_or_else(|| format!("entry {row} is not an object"))?;
        // A `choice` row's translatable string is its `label`; a `line` row's
        // is its `text` — exactly the split `render_json` writes.
        let field = match obj.get("kind").and_then(serde_json::Value::as_str) {
            Some("line") => "text",
            Some("choice") => "label",
            Some(other) => {
                return Err(format!(
                    "entry {row} has unknown `kind` `{other}` (expected `line` or `choice`)"
                ))
            }
            None => return Err(format!("entry {row} has no `kind`")),
        };
        let text = obj
            .get(field)
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("entry {row} has no string `{field}`"))?;
        rows.push(ImportRow {
            line_id: obj
                .get("lineId")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
            // PRESENT-but-empty is preserved (the caller reports it as an
            // empty locale tag); only an absent/`null` key means "no override".
            // JSON, unlike CSV, can tell those two apart, so it does.
            locale: match obj.get("locale") {
                None | Some(serde_json::Value::Null) => None,
                Some(serde_json::Value::String(s)) => Some(s.clone()),
                Some(_) => return Err(format!("entry {row} has a non-string `locale`")),
            },
            text: text.to_string(),
            row,
        });
    }
    Ok(rows)
}

/// Parse `loc export --format csv`'s table back into rows, BY HEADER NAME.
fn parse_csv_rows(text: &str) -> Result<Vec<ImportRow>, String> {
    let records = parse_csv(text)?;
    let mut records = records.into_iter().enumerate();
    let (_, header) = records
        .next()
        .ok_or_else(|| "empty CSV: no header row".to_string())?;
    let column = |name: &str| header.iter().position(|h| h.trim() == name);
    let expected = CSV_COLUMNS.join(",");
    let line_id_at = column("lineId")
        .ok_or_else(|| format!("CSV header has no `lineId` column (expected `{expected}`)"))?;
    let text_at = column("text")
        .ok_or_else(|| format!("CSV header has no `text` column (expected `{expected}`)"))?;
    let locale_at = column("locale");

    let mut rows = Vec::new();
    for (i, record) in records {
        let row = i + 1;
        // A blank separator line is not a row.
        if record.iter().all(String::is_empty) {
            continue;
        }
        let Some(text) = record.get(text_at) else {
            return Err(format!(
                "row {row} has {} field(s), too few for the {} declared column(s)",
                record.len(),
                header.len()
            ));
        };
        rows.push(ImportRow {
            line_id: record.get(line_id_at).cloned(),
            locale: locale_at
                .and_then(|at| record.get(at))
                .filter(|s| !s.is_empty())
                .cloned(),
            text: text.clone(),
            row,
        });
    }
    Ok(rows)
}

/// Split RFC-4180 text into records of fields — the exact dialect
/// [`render_csv`]/[`csv_field`] write: `,` separators, `CRLF` (or bare `LF`)
/// record breaks, `"`-quoted fields with `""` escaping a literal quote, and
/// quoted fields free to contain separators and newlines.
///
/// Hand-rolled rather than pulling in a CSV crate for one reader: the dialect
/// is fixed by our own writer sitting twenty lines above, and the whole thing
/// is a character loop with one flag.
fn parse_csv(text: &str) -> Result<Vec<Vec<String>>, String> {
    let mut records: Vec<Vec<String>> = Vec::new();
    let mut record: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if in_quotes {
            match c {
                // A doubled quote is one literal quote; a lone one closes.
                '"' if chars.peek() == Some(&'"') => {
                    chars.next();
                    field.push('"');
                }
                '"' => in_quotes = false,
                _ => field.push(c),
            }
            continue;
        }
        match c {
            '"' if field.is_empty() => in_quotes = true,
            '"' => return Err("unescaped `\"` inside an unquoted field".to_string()),
            ',' => record.push(std::mem::take(&mut field)),
            '\r' | '\n' => {
                if c == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
            }
            _ => field.push(c),
        }
    }
    if in_quotes {
        return Err("unterminated quoted field".to_string());
    }
    // A file not ending in a newline still has one last record.
    if !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push(record);
    }
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_reader_round_trips_the_writers_quoting() {
        // Every quoting case `csv_field` produces: a comma, a doubled quote,
        // and an embedded newline inside one quoted field.
        let written = format!(
            "{}\r\n{}\r\n",
            CSV_COLUMNS.join(","),
            [
                csv_field("line"),
                csv_field("a.lute"),
                csv_field("7"),
                csv_field("x.n_0010"),
                csv_field("0010"),
                csv_field("n"),
                csv_field(""),
                csv_field("he said \"hi, there\"\nand left"),
            ]
            .join(",")
        );
        let rows = parse_csv_rows(&written).expect("writer output re-reads");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].line_id.as_deref(), Some("x.n_0010"));
        assert_eq!(rows[0].text, "he said \"hi, there\"\nand left");
        assert_eq!(rows[0].locale, None, "no `locale` column -> the file stem decides");
    }

    #[test]
    fn csv_reader_rejects_an_unterminated_quote() {
        let bad = format!("{}\r\nline,a.lute,1,x,0010,n,,\"oops\r\n", CSV_COLUMNS.join(","));
        assert!(parse_csv_rows(&bad).is_err());
    }

    #[test]
    fn csv_reader_takes_columns_by_name_and_honors_a_locale_column() {
        let src = "text,locale,lineId\r\nこんにちは,ja-JP,x.n_0010\r\n";
        let rows = parse_csv_rows(src).expect("column order is free");
        assert_eq!(rows[0].line_id.as_deref(), Some("x.n_0010"));
        assert_eq!(rows[0].locale.as_deref(), Some("ja-JP"));
        assert_eq!(rows[0].text, "こんにちは");
    }

    #[test]
    fn json_reader_takes_label_for_a_choice_and_text_for_a_line() {
        let src = r#"[
          {"kind":"line","file":"a.lute","line":1,"lineId":"x.n_0010","code":"0010","speaker":"n","text":"hi"},
          {"kind":"choice","file":"a.lute","line":2,"lineId":"x.b.go","key":"b.go","label":"Go"},
          {"kind":"line","file":"a.lute","line":3,"lineId":null,"code":null,"speaker":"n","text":"untagged"}
        ]"#;
        let rows = parse_json_rows(src).expect("export shape parses");
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].text, "hi");
        assert_eq!(rows[1].text, "Go", "a choice row's translation is its `label`");
        assert_eq!(rows[2].line_id, None, "an untagged row carries no join key");
    }

    #[test]
    fn json_reader_rejects_a_shape_export_never_writes() {
        assert!(parse_json_rows("{}").is_err(), "root must be an array");
        assert!(parse_json_rows(r#"[{"text":"hi"}]"#).is_err(), "`kind` is required");
        assert!(
            parse_json_rows(r#"[{"kind":"song","text":"hi"}]"#).is_err(),
            "an unknown `kind` is a defect, not something to guess past"
        );
    }
}
