//! dsl 0.28.0 §4: chapters. `lute.project.yaml` MAY declare
//! `chapters: [{ on: chapter, scenes: [prologue, counter, kitchen] }]` — a
//! list of chains, one per occasion. Every scene a chain lists answers its
//! occasion (`on:`), waits for the one before it (`after:
//! visited("<previous>")`) and takes a descending `priority:` — unless its
//! own frontmatter already writes that key. On a `select: sequence`
//! occasion, which presents every eligible beat in one raise, a chain
//! derives `on:` and the descending priority only: the list is the order
//! within that raise.
//!
//! The derivation is a DESUGAR applied right after parsing
//! ([`apply_chapters`], beside `questTier`'s default): the derived keys are
//! appended to the scene's frontmatter below [`CHAPTERS_MARKER`], so every
//! pass, tool and the compiled artifact read one ordinary scene beat. A
//! derived key has no text of its own in the scene; [`crate::meta::meta_key_span`]
//! anchors it at the scene's `id:` (the entry that puts it in the chain), and
//! a message about it says where it came from ([`provenance`]).
//!
//! The manifest's shape is checked at load (`lute_manifest::project`,
//! `E-CHAPTERS`); [`check_project_chapters`] checks its ids against the
//! project's scenes.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::project::{chain_label, Chain, ChapterAnchor, MetaDefaults, E_CHAPTERS};
use lute_manifest::schema::{OccasionDecl, OccasionSelect};
use lute_syntax::ast::Document;

use crate::check::FoldedEnv;
use crate::meta::DocKind;

/// The line that separates a scene's authored frontmatter from the keys
/// [`apply_chapters`] derived. A YAML comment, so the frontmatter still
/// parses as one mapping.
pub const CHAPTERS_MARKER: &str = "# lute: derived from lute.project.yaml `chapters:`\n";

/// Appended instead of derived keys when the chain that lists the scene is
/// malformed and not applied ([`Unapplied::Rejected`]).
const REJECTED_MARKER: &str = "# lute: listed by a `chapters:` chain that is not applied\n";

/// Appended when the scene is listed by the retired `sequence:` key
/// ([`Unapplied::Retired`]).
const RETIRED_MARKER: &str = "# lute: listed by the retired `sequence:` key\n";

/// What every message about a key [`apply_chapters`] derived appends.
pub const PROVENANCE: &str = " (written by `chapters:` in lute.project.yaml)";

/// The authored part of a frontmatter (`raw_yaml` up to the first marker
/// [`apply_chapters`] appended).
pub fn authored_yaml(raw_yaml: &str) -> &str {
    [CHAPTERS_MARKER, REJECTED_MARKER, RETIRED_MARKER]
        .iter()
        .filter_map(|m| raw_yaml.find(m))
        .min()
        .map_or(raw_yaml, |i| &raw_yaml[..i])
}

/// Whether frontmatter `key` is one the manifest's `chapters:` derived
/// (below [`CHAPTERS_MARKER`]), not one the scene wrote.
pub fn derived(meta: &lute_syntax::ast::Meta, key: &str) -> bool {
    meta.raw_yaml.find(CHAPTERS_MARKER).is_some_and(|i| {
        meta.raw_yaml[i + CHAPTERS_MARKER.len()..]
            .lines()
            .any(|l| l.split_once(':').is_some_and(|(k, _)| k == key))
    })
}

/// [`PROVENANCE`] when `key` is [`derived`], else nothing — for a message
/// about the key's value.
pub fn provenance(meta: &lute_syntax::ast::Meta, key: &str) -> &'static str {
    if derived(meta, key) {
        PROVENANCE
    } else {
        ""
    }
}

/// Why a scene the manifest lists derives no keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unapplied {
    /// Its chain is malformed (`E-CHAPTERS` at the manifest).
    Rejected,
    /// It is listed by the retired `sequence:` key.
    Retired,
}

/// Whether the manifest lists this scene without deriving its keys.
pub fn unapplied(meta: &lute_syntax::ast::Meta) -> Option<Unapplied> {
    if meta.raw_yaml.contains(REJECTED_MARKER) {
        Some(Unapplied::Rejected)
    } else if meta.raw_yaml.contains(RETIRED_MARKER) {
        Some(Unapplied::Retired)
    } else {
        None
    }
}

/// How a scene without `on:` that the manifest lists (unapplied) is told
/// why it has none — instead of being told to list itself.
pub fn unapplied_note(u: Unapplied) -> &'static str {
    match u {
        Unapplied::Rejected => {
            "lute.project.yaml lists this scene in `chapters:`, but that chain is not applied \
             (its `E-CHAPTERS` error is reported at the manifest), so it gives the scene no \
             `on:` — fix the chain"
        }
        Unapplied::Retired => {
            "lute.project.yaml lists this scene under `sequence:`, which is now `chapters:`, so \
             it gives the scene no `on:` — rename the key"
        }
    }
}

/// Whether the chain on `occasion` chains its scenes with `after:` —
/// every occasion but a `select: sequence` one, which presents every
/// eligible beat in one raise (the priority order is the chain there).
pub fn chained(occasion: &str, occasions: &BTreeMap<String, OccasionDecl>) -> bool {
    occasions
        .get(occasion)
        .is_none_or(|d| d.select != OccasionSelect::Sequence)
}

/// id is supplied by the already-folded metadata snapshot.
pub fn derived_after(doc: &Document, typed: &crate::meta::TypedMeta) -> Option<String> {
    if !derived(&doc.meta, "after") {
        return None;
    }
    Some(typed.yaml()?.get("id")?.as_str()?.trim().to_string())
}

/// Append the keys `defaults`' `chapters:` derives for this scene (dsl
/// 0.28.0 §4) to its frontmatter. A key the scene writes itself wins. A
/// scene an unapplied chain lists gets a marker instead ([`unapplied`]). A
/// no-op for a document that is no scene, a scene no chain lists, a
/// frontmatter that does not parse, or one already applied — idempotent, so
/// every surface may call it on the documents it parses. `occasions` is the
/// resolved vocabulary ([`chained`]).
pub fn apply_chapters(
    doc: &mut Document,
    defaults: &MetaDefaults,
    occasions: &BTreeMap<String, OccasionDecl>,
) {
    if defaults.chapters().is_empty()
        || authored_yaml(&doc.meta.raw_yaml).len() != doc.meta.raw_yaml.len()
        || crate::meta::resolve_doc_kind_with_defaults(&doc.meta, defaults).0
            != Some(DocKind::Scene)
    {
        return;
    }
    let Ok(serde_yaml::Value::Mapping(map)) =
        serde_yaml::from_str::<serde_yaml::Value>(&doc.meta.raw_yaml)
    else {
        return;
    };
    let Some(id) = map.get("id").and_then(serde_yaml::Value::as_str) else {
        return;
    };
    let Some(chain) = defaults.chain_of(id.trim()) else {
        return;
    };
    let raw = &mut doc.meta.raw_yaml;
    if !raw.is_empty() && !raw.ends_with('\n') {
        raw.push('\n');
    }
    // A chain on an occasion nothing declares is refused at the manifest
    // (`E-CHAPTERS`), as a malformed one is: apply neither.
    if !chain.applied || (!occasions.is_empty() && !occasions.contains_key(&chain.on)) {
        raw.push_str(if chain.retired {
            RETIRED_MARKER
        } else {
            REJECTED_MARKER
        });
        return;
    }
    let Some(derived) = chain.derived(id.trim(), chained(&chain.on, occasions)) else {
        return;
    };
    let mut lines = String::new();
    for (key, value) in derived {
        if map.contains_key(key) {
            continue;
        }
        let text = match value {
            // `visited("prev")`: single-quoted so the inner quotes survive.
            serde_yaml::Value::String(s) if key == "after" => format!("'{s}'"),
            serde_yaml::Value::String(s) => s,
            serde_yaml::Value::Number(n) => n.to_string(),
            _ => continue,
        };
        lines.push_str(&format!("{key}: {text}\n"));
    }
    if lines.is_empty() {
        return;
    }
    raw.push_str(CHAPTERS_MARKER);
    raw.push_str(&lines);
}

fn diag(code: &str, severity: Severity, message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity,
        message,
        evidence: None,
        span,
        layer: Layer::Content,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

fn error(message: String, span: Span) -> Diagnostic {
    diag(E_CHAPTERS, Severity::Error, message, span)
}

/// A listed scene whose own `when:` can stay false for good stops every
/// scene after it that waits on it through `after: visited("<it>")`.
pub const W_CHAPTER_STALL: &str = "W-CHAPTER-STALL";

/// A listed scene whose own `priority:` puts it out of the listed order on
/// a `select: sequence` occasion, where the chain IS the priority order
/// ([`chained`]).
pub const W_CHAPTER_ORDER: &str = "W-CHAPTER-ORDER";

fn span_at(start: usize, len: usize) -> Span {
    Span {
        byte_start: start,
        byte_end: start + len,
        line: 0,
        column: 0,
        utf16_range: (0, 0),
    }
}

/// Where `key:` sits in `text` from byte `from` on — as a mapping key (at a
/// line start after indent or a `- `, or after `{` / `,` in a flow mapping).
fn key_at(text: &str, key: &str, from: usize, to: usize) -> Option<usize> {
    text[from..to]
        .match_indices(key)
        .map(|(i, _)| from + i)
        .find(|&i| {
            let before = text[..i].trim_end_matches([' ', '\t']);
            let opens = before.is_empty()
                || before.ends_with(['\n', '{', ','])
                || (before.ends_with('-')
                    && before[..before.len() - 1]
                        .trim_end_matches([' ', '\t'])
                        .ends_with('\n'));
            opens
                && text[i + key.len()..]
                    .trim_start_matches([' ', '\t'])
                    .starts_with(':')
        })
}

/// Where a top-level `key:` sits, and where its block ends (the next line
/// that starts at column 0 with content).
fn top_block(text: &str, key: &str) -> Option<(usize, usize)> {
    let at = text.match_indices(key).map(|(i, _)| i).find(|&i| {
        (i == 0 || text[..i].ends_with('\n'))
            && text[i + key.len()..]
                .trim_start_matches([' ', '\t'])
                .starts_with(':')
    })?;
    let body = at + key.len();
    let end = text[body..]
        .match_indices('\n')
        .map(|(i, _)| body + i + 1)
        .find(|&i| {
            text[i..]
                .chars()
                .next()
                .is_some_and(|c| !c.is_whitespace() && c != '#' && c != '-')
        })
        .unwrap_or(text.len());
    Some((at, end))
}

/// The byte ranges of each chain of the `chapters:` block `[body, end)`:
/// block-style `- ` items at the list's indentation, or the `{ … }` items of
/// a flow list. One range covering the whole block when it holds neither
/// (a mapping written where the list belongs).
fn chain_ranges(text: &str, body: usize, end: usize) -> Vec<(usize, usize)> {
    let block = &text[body..end];
    if let Some(open) = block
        .find('[')
        .filter(|&o| block[..o].trim_start_matches(':').trim().is_empty())
    {
        let mut out = Vec::new();
        let (mut depth, mut start) = (0usize, None);
        for (i, c) in block[open..].char_indices() {
            let at = body + open + i;
            match c {
                '{' => {
                    if depth == 1 {
                        start = Some(at);
                    }
                    depth += 1;
                }
                '}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 1 {
                        if let Some(s) = start.take() {
                            out.push((s, at + 1));
                        }
                    }
                }
                '[' => depth += 1,
                ']' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
        return out;
    }
    let mut starts = Vec::new();
    let mut indent = None;
    let mut offset = body;
    for line in block.split_inclusive('\n') {
        let trimmed = line.trim_start_matches([' ', '\t']);
        let lead = line.len() - trimmed.len();
        if trimmed.starts_with("- ") || trimmed.trim_end() == "-" {
            if indent.is_none_or(|i| i == lead) {
                indent = Some(lead);
                starts.push(offset + lead);
            }
        }
        offset += line.len();
    }
    if starts.is_empty() {
        return vec![(body, end)];
    }
    starts
        .iter()
        .enumerate()
        .map(|(n, &s)| (s, starts.get(n + 1).copied().unwrap_or(end)))
        .collect()
}

/// The span `anchor` points at in the manifest text — falling back to the
/// chain's item, then the block's key, then byte 0.
fn locate(manifest: &str, anchor: &ChapterAnchor) -> Span {
    let top = |key: &str| {
        top_block(manifest, key).map_or_else(|| span_at(0, 0), |(at, _)| span_at(at, key.len()))
    };
    let Some((at, end)) = top_block(manifest, "chapters") else {
        return match anchor {
            ChapterAnchor::Top(key) => top(key),
            _ => span_at(0, 0),
        };
    };
    let body = at + "chapters".len();
    let chain = |n: usize| chain_ranges(manifest, body, end).get(n).copied();
    match anchor {
        ChapterAnchor::Top(key) => top(key),
        ChapterAnchor::Chain(n) => {
            chain(*n).map_or_else(|| top("chapters"), |(s, _)| span_at(s, 1))
        }
        ChapterAnchor::Key(n, key) => match chain(*n) {
            Some((s, e)) => {
                key_at(manifest, key, s, e).map_or_else(|| span_at(s, 1), |i| span_at(i, key.len()))
            }
            None => top("chapters"),
        },
        ChapterAnchor::Entry(n, text, nth) => {
            let Some((s, e)) = chain(*n) else {
                return top("chapters");
            };
            let from = key_at(manifest, "scenes", s, e).unwrap_or(s);
            let bounded = |i: usize| {
                let ok =
                    |c: Option<char>| !c.is_some_and(|c| c.is_alphanumeric() || "_.-".contains(c));
                ok(manifest[..i].chars().next_back())
                    && ok(manifest[i + text.len()..].chars().next())
            };
            if text.is_empty() {
                return span_at(s, 1);
            }
            manifest[from..e]
                .match_indices(text.as_str())
                .map(|(i, _)| from + i)
                .filter(|&i| bounded(i))
                .nth(*nth)
                .map_or_else(|| span_at(s, 1), |i| span_at(i, text.len()))
        }
    }
}

/// The byte range of `lute.project.yaml` text an `E-CHAPTERS` shape
/// diagnostic points at — for a command that reports the manifest alone
/// (the build gate).
pub fn locate_in_manifest(manifest: &str, anchor: &ChapterAnchor) -> std::ops::Range<usize> {
    let s = locate(manifest, anchor);
    s.byte_start..s.byte_end
}

/// dsl 0.28.0 §4, project-wide: a malformed `chapters:` (the load's shape
/// diagnostics, located in `lute.project.yaml` — the documents are still
/// checked; a malformed chain is simply not applied), then every chain's
/// ids against the scenes ([`check_chain`]). `docs` are the root's
/// documents, already desugared, parallel to `foldeds`.
pub fn check_project_chapters(
    root: &Path,
    docs: &[(PathBuf, Document)],
    foldeds: &[&FoldedEnv],
) -> Vec<(PathBuf, Diagnostic)> {
    let Ok(Some(project)) = lute_manifest::project::load_project(root) else {
        return Vec::new();
    };
    let manifest_path = root.join("lute.project.yaml");
    let manifest = std::fs::read_to_string(&manifest_path).unwrap_or_default();
    let mut out: Vec<(PathBuf, Diagnostic)> = project
        .chapter_diags
        .iter()
        .map(|d| {
            (
                manifest_path.clone(),
                error(d.message.clone(), locate(&manifest, &d.anchor)),
            )
        })
        .collect();
    let project_scope = ProjectScope { docs, foldeds };
    for (index, chain) in project.defaults.chapters().iter().enumerate() {
        if chain.retired {
            continue;
        }
        for (path, d) in check_chain(index, chain, &manifest, &project_scope) {
            out.push((path.unwrap_or_else(|| manifest_path.clone()), d));
        }
    }
    out
}

/// The desugared documents of one project root and their folds (parallel).
struct ProjectScope<'a> {
    docs: &'a [(PathBuf, Document)],
    foldeds: &'a [&'a FoldedEnv],
}

impl ProjectScope<'_> {
    fn occasions(&self) -> Option<&BTreeMap<String, OccasionDecl>> {
        self.foldeds.first().map(|f| &f.occasions)
    }
    fn fold_of(&self, path: &Path) -> Option<&FoldedEnv> {
        let i = self.docs.iter().position(|(p, _)| p == path)?;
        self.foldeds.get(i).copied()
    }
    fn doc(&self, path: &Path) -> Option<&Document> {
        self.docs.iter().find(|(p, _)| p == path).map(|(_, d)| d)
    }
    fn typed_of(&self, path: &Path) -> Option<&crate::meta::TypedMeta> {
        let i = self.docs.iter().position(|(p, _)| p == path)?;
        self.foldeds.get(i).map(|f| &f.typed)
    }
}

/// authored `on:`, an entry's or bundle beat's `on=` — with how many.
fn answered_occasions(scope: &ProjectScope<'_>) -> BTreeMap<String, usize> {
    let mut out: BTreeMap<String, usize> = BTreeMap::new();
    for ((_, doc), folded) in scope.docs.iter().zip(scope.foldeds) {
        let scene_on = folded.typed.beat.as_ref().map(|b| b.on.clone());
        let ons = scene_on.into_iter().chain(
                doc.entries
                    .iter()
                    .filter_map(|e| Some(e.on.as_ref()?.0.clone())),
            )
            .chain(
                doc.beats
                    .iter()
                    .filter_map(|b| Some(b.on.as_ref()?.0.clone())),
            );
        for on in ons {
            *out.entry(on.trim().to_string()).or_default() += 1;
        }
    }
    out
}

/// What `id` names when it is no scene key: a bundle beat, a lore entry, a
/// quest or lore document — or the nearest scene key (by its last segment
/// first, `c4s1` → `main.c4s1`).
fn not_a_scene(id: &str, label: &str, scope: &ProjectScope<'_>, scenes: &SceneKeys<'_>) -> String {
    let bundles = crate::connectivity::bundle_beat_key_set(scope.docs);
    if bundles.contains_key(id) {
        return format!(
            "{label} lists `{id}`, which is a bundle beat — `chapters:` chains scenes; give the \
             beat its own `on=` and `after=` instead"
        );
    }
    // An entry's key is its bare `id` (`entry.<id>.*`); a writer who
    // qualifies it by its document, as bundle beats are, means it too.
    if scope.docs.iter().any(|(_, d)| {
        let doc_id = crate::connectivity::bundle_id(d);
        d.entries.iter().any(|e| {
            e.id == id
                || doc_id.as_deref().is_some_and(|doc| {
                    id.strip_prefix(doc).and_then(|r| r.strip_prefix('.')) == Some(e.id.as_str())
                })
        })
    }) {
        return format!(
            "{label} lists `{id}`, which is a lore entry — `chapters:` chains scenes; give the \
             entry its own `on=` instead"
        );
    }
    if scope
        .docs
        .iter()
        .any(|(_, d)| crate::connectivity::bundle_id(d).as_deref() == Some(id))
    {
        return format!(
            "{label} lists `{id}`, which is a quest or lore document, not a scene — \
             `chapters:` chains scenes"
        );
    }
    let by_segment = scenes
        .keys()
        .filter(|k| k.rsplit('.').next() == Some(id))
        .collect::<Vec<_>>();
    let hint = match by_segment.as_slice() {
        [one] => format!(" — did you mean `{one}`?"),
        _ => lute_manifest::suggest::did_you_mean(id, scenes.keys().map(String::as_str)),
    };
    format!("{label} lists `{id}`, but no scene in this project declares `id: {id}`{hint}")
}

type SceneKeys<'a> = BTreeMap<String, Vec<(PathBuf, Span)>>;

/// A frontmatter's authored top-level scalar `key`, as text.
fn own(typed: &crate::meta::TypedMeta, key: &str) -> Option<String> {
    let v = typed.yaml()?.as_mapping()?;
    match v.get(serde_yaml::Value::String(key.to_string()))? {
        serde_yaml::Value::String(s) => Some(s.trim().to_string()),
        serde_yaml::Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// A frontmatter's effective (authored or derived) top-level string `key`.
fn effective(typed: &crate::meta::TypedMeta, key: &str) -> Option<String> {
    typed.yaml()?.get(key)?.as_str().map(|s| s.trim().to_string())
}

/// Whether the `after:` formula cannot hold until `visited("<id>")` does.
fn requires_visited(f: &crate::prereq::PrereqFormula, id: &str) -> bool {
    use crate::prereq::PrereqFormula as F;
    match f {
        F::Visited(v) => v == id,
        F::Completed(_) | F::Active(_) => false,
        F::And(a, b) => requires_visited(a, id) || requires_visited(b, id),
        F::Or(a, b) => requires_visited(a, id) && requires_visited(b, id),
    }
}

/// Whether a chain waits on scene `id`: some scene's `after:` the chain
/// derived cannot hold until `visited("<id>")` does — so a stop there is
/// [`W_CHAPTER_STALL`]'s to report.
pub fn waited_on(
    docs: &[(PathBuf, Document)],
    typed: &[&crate::meta::TypedMeta],
    id: &str,
) -> bool {
    docs.iter().zip(typed.iter().copied()).any(|((_, d), typed)| {
        derived(&d.meta, "after")
            && effective(typed, "after")
                .and_then(|a| crate::prereq::parse_prereq(&a, span_at(0, 0)).0)
                .is_some_and(|f| requires_visited(&f, id))
    })
}

/// The state paths a `when:` (its `@def`s expanded) reads that neither the
/// clock nor the engine advances — empty when it reads one the walk cannot
/// place. `None` when every read is the clock's or another `owner: engine`
/// path (or it reads nothing, or calls a function but the CEL operators):
/// the story cannot make those true, so whether the chain waits is the
/// clock's question ([`crate::clock_positions::chapter_window`]) or the
/// engine's.
fn paths_that_may_stay(when: &str, folded: &FoldedEnv) -> Option<Vec<String>> {
    use cel_parser::ast::Expr;
    let defs = crate::cel_expand::DefTable {
        bodies: &folded.def_bodies,
        params: &folded.env.def_params,
    };
    let expanded = crate::cel_expand::expand_cel(when, &defs, None, &mut Vec::new())
        .unwrap_or_else(|_| when.to_string());
    let clock = folded.env.clock.as_ref();
    let is_clock = |p: &str| {
        p.starts_with("clock.")
            || clock.is_some_and(|c| c.day == p || c.slot.as_deref() == Some(p))
            || folded
                .env
                .state
                .decls
                .get(p)
                .is_some_and(|d| d.owner == Some(lute_manifest::types::Owner::Engine))
    };
    let mut arena = lute_cel::CelArena::default();
    let ided = lute_cel::parse_slot_marked_refs(&mut arena, &expanded).and_then(|h| arena.get(h));
    let Some(ided) = ided else {
        return Some(Vec::new());
    };
    let (mut clock_reads, mut other) = (0usize, Vec::new());
    let mut invalid = false;
    lute_cel::walk(&ided.expr, &mut |node| {
        let lute_cel::Node::Expr(expr) = node else {
            return lute_cel::Flow::Continue;
        };
        match expr {
            Expr::Ident(_) | Expr::Select(_) => {
                let Some(path) = crate::cel_paths::select_path(expr) else {
                    invalid = true;
                    return lute_cel::Flow::Stop;
                };
                if is_clock(&path) {
                    clock_reads += 1;
                } else if !other.contains(&path) {
                    other.push(path);
                }
                lute_cel::Flow::Skip
            }
            Expr::Call(c)
                if c.target.is_none()
                    && !c.func_name.starts_with(|ch: char| ch.is_ascii_alphabetic()) =>
            {
                lute_cel::Flow::Continue
            }
            Expr::List(_) => lute_cel::Flow::Continue,
            Expr::Literal(_) => lute_cel::Flow::Skip,
            Expr::Call(_)
            | Expr::Comprehension(_)
            | Expr::Map(_)
            | Expr::Struct(_)
            | Expr::Unspecified => {
                invalid = true;
                lute_cel::Flow::Stop
            }
        }
    });
    if invalid {
        return Some(other);
    }
    if other.is_empty() {
        None
    } else {
        Some(other)
    }
}

/// One chain's project-wide checks. Every chain but a retired one:
///
/// * `on:` names a declared occasion (with a did-you-mean); while no plugin
///   declares occasions (shape-only), a near-miss of an occasion other
///   beats answer when none answers the chain's;
/// * every listed id names a scene (a bundle beat, entry or document says
///   so; else a did-you-mean, by the id's last segment first).
///
/// An applied chain also:
///
/// * no listed scene answers another occasion with its own `on:` — the
///   chain would skip it;
/// * on a targeted occasion, every listed scene declares `target:` — one
///   without would play for every target (at the scene's `id:`);
/// * [`W_CHAPTER_STALL`]: a listed scene whose own `when:` may stay false
///   for good, when the next listed scene's `after:` (derived or written)
///   waits on it;
/// * [`W_CHAPTER_ORDER`]: on a `select: sequence` occasion, a listed scene's
///   own `priority:` that breaks the listed order.
///
/// A diagnostic with no path is the manifest's.
fn check_chain(
    index: usize,
    chain: &Chain,
    manifest: &str,
    scope: &ProjectScope<'_>,
) -> Vec<(Option<PathBuf>, Diagnostic)> {
    let empty = BTreeMap::new();
    let occasions = scope.occasions().unwrap_or(&empty);
    let scenes = crate::connectivity::scene_key_set(scope.docs);
    let label = chain_label(index, &chain.on);
    let mut out: Vec<(Option<PathBuf>, Diagnostic)> = Vec::new();
    let on = &chain.on;
    if !on.is_empty() {
        let on_span = || locate(manifest, &ChapterAnchor::Key(index, "on".to_string()));
        if !occasions.is_empty() && !occasions.contains_key(on) {
            let hint =
                lute_manifest::suggest::did_you_mean(on, occasions.keys().map(String::as_str));
            out.push((
                None,
                error(
                    format!(
                        "{label} answers `{on}`, which no resolved plugin or manifest \
                         declares{hint}"
                    ),
                    on_span(),
                ),
            ));
        } else if occasions.is_empty() {
            let answered = answered_occasions(scope);
            if !answered.contains_key(on) {
                let near =
                    lute_manifest::suggest::nearest(on, answered.keys().map(String::as_str), 2);
                if let Some(near) = near {
                    let n = answered[near];
                    out.push((
                        None,
                        error(
                            format!(
                                "{label} answers `{on}`, which no other beat answers, while {n} \
                                 beat{s} answer{v} `{near}` — did you mean `{near}`?",
                                s = if n == 1 { "" } else { "s" },
                                v = if n == 1 { "s" } else { "" },
                            ),
                            on_span(),
                        ),
                    ));
                }
            }
        }
    }
    // A chain whose `on:` is refused is reported there, not also as stalls.
    let on_refused = !out.is_empty();
    let decl = occasions.get(on);
    let is_chained = chained(on, occasions);
    for (i, id) in chain.scenes.iter().enumerate() {
        let span = locate(manifest, &ChapterAnchor::Entry(index, id.clone(), 0));
        let Some(homes) = scenes.get(id) else {
            out.push((None, error(not_a_scene(id, &label, scope, &scenes), span)));
            continue;
        };
        if !chain.applied {
            continue;
        }
        for (path, _) in homes {
            let Some(doc) = scope.doc(path) else {
                continue;
            };
            let own_on = scope
                .typed_of(path)
                .and_then(|typed| own(typed, "on"))
                .filter(|o| o != on);
            if let Some(own_on) = &own_on {
                out.push((
                    None,
                    error(
                        format!(
                            "{label} lists `{id}`, but its frontmatter says `on: {own_on}`, so it \
                             never answers `{on}` and the chain stops before it — remove its \
                             `on:` or take it out of the chain"
                        ),
                        span,
                    ),
                ));
            }
            // T1-14: a scene without `target:` on a targeted occasion
            // answers every target — the chapter would replay at each one.
            if decl.is_some_and(|d| d.target.takes_target())
                && scope.typed_of(path).is_none_or(|typed| own(typed, "target").is_none())
            {
                out.push((
                    Some(path.clone()),
                    error(
                        format!(
                            "{label} lists `{id}`, but `{on}` is raised for a target and \
                             `{id}` declares no `target:`, so it would play for every target — \
                             give it `target: <the target it belongs to>`"
                        ),
                        crate::meta::meta_key_span(&doc.meta, "id"),
                    ),
                ));
            }
            // A scene that answers another occasion (or a chain whose `on:`
            // is refused) is reported as such, not also as a stall.
            if own_on.is_some() || on_refused {
                continue;
            }
            if let Some(stall) = stall(index, chain, i, is_chained, manifest, path, scope) {
                out.push((None, stall));
            }
        }
    }
    if chain.applied && !is_chained {
        out.extend(
            check_chain_order(chain, scope.docs)
                .into_iter()
                .map(|(p, d)| (Some(p), d)),
        );
    }
    out
}

/// [`W_CHAPTER_STALL`] for the `i`th listed scene (`doc`, at `path`): its own
/// `when:` may stay false for good — it reads state the story may never
/// set, or a clock window that closes ([`crate::clock_positions::chapter_window`])
/// — and the next listed scene's effective `after:` — derived or written —
/// cannot hold until it plays.
#[allow(clippy::too_many_arguments)]
fn stall(
    index: usize,
    chain: &Chain,
    i: usize,
    is_chained: bool,
    manifest: &str,
    path: &Path,
    scope: &ProjectScope<'_>,
) -> Option<Diagnostic> {
    let id = &chain.scenes[i];
    let next = chain.scenes.get(i + 1)?;
    if !is_chained {
        return None;
    }
    let typed = scope.typed_of(path)?;
    let when = own(typed, "when").filter(|w| w != "true")?;
    let scenes = crate::connectivity::scene_key_set(scope.docs);
    let next_path = scenes.get(next)?.iter().find_map(|(p, _)| Some(p))?;
    let next_doc = scope.doc(next_path)?;
    let next_typed = scope.typed_of(next_path)?;
    let after = effective(next_typed, "after")?;
    let formula = crate::prereq::parse_prereq(&after, span_at(0, 0)).0?;
    if !requires_visited(&formula, id) {
        return None;
    }
    let folded = scope.fold_of(path)?;
    let (reads, must) = match paths_that_may_stay(&when, folded) {
        Some(paths) => {
            let reads = match paths.as_slice() {
                [] => String::new(),
                [one] => format!(" (it reads `{one}`, which may never make it true)"),
                many => format!(
                    " (it reads {}, which may never make it true)",
                    many.iter()
                        .map(|p| format!("`{p}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            };
            (
                reads,
                "make sure the story makes its condition true".to_string(),
            )
        }
        None => {
            let prev = i.checked_sub(1).map(|p| {
                let prev_id = chain.scenes[p].as_str();
                let prev_when = scenes
                    .get(prev_id)
                    .and_then(|homes| homes.iter().find_map(|(p, _)| scope.typed_of(p)))
                    .and_then(|typed| own(typed, "when"));
                (prev_id, prev_when)
            });
            let (why, fix) = crate::clock_positions::chapter_window(
                &when,
                prev.as_ref().map(|(id, w)| (*id, w.as_deref())),
                &chain.on,
                folded,
            )?;
            (format!(", and {why}"), fix)
        }
    };
    let whose = if derived(&next_doc.meta, "after") {
        format!("the `after: visited(\"{id}\")` the chain gives it")
    } else {
        format!("its own `after: {after}`")
    };
    let optional = match i.checked_sub(1).map(|p| &chain.scenes[p]) {
        Some(prev) => format!(
            "if `{id}` may be skipped, give `{next}` `after: 'visited(\"{prev}\")'` so it can \
             follow `{prev}` directly (`{id}` still plays first whenever it is eligible — it ranks \
             higher)"
        ),
        None => format!(
            "if `{id}` may be skipped, take it out of the chain and give it its own `on:`, so the \
             chain starts at `{next}`"
        ),
    };
    Some(diag(
        W_CHAPTER_STALL,
        Severity::Warning,
        format!(
            "{label} lists `{id}`, which plays only `when: {when}`{reads}; `{next}` waits on it \
             through {whose}, so while that stays false the chapters stop at `{id}` — \
             {optional}; if `{id}` must play, {must}",
            label = chain_label(index, &chain.on),
        ),
        locate(manifest, &ChapterAnchor::Entry(index, id.clone(), 0)),
    ))
}

/// [`W_CHAPTER_ORDER`], at the scene's own `priority:` key: on a `select:
/// sequence` occasion, a listed scene that writes its own `priority:` whose
/// value breaks the strictly descending order the list derives — against the
/// effective (own or derived) priority of every other listed scene. `docs`
/// are already desugared.
fn check_chain_order(chain: &Chain, docs: &[(PathBuf, Document)]) -> Vec<(PathBuf, Diagnostic)> {
    let scenes = crate::connectivity::scene_key_set(docs);
    let priority = |yaml: &str| {
        serde_yaml::from_str::<serde_yaml::Value>(yaml)
            .ok()?
            .get("priority")?
            .as_f64()
    };
    // (index in the list, path, doc, effective priority, own priority)
    let listed: Vec<(usize, &PathBuf, &Document, f64, Option<f64>)> = chain
        .scenes
        .iter()
        .enumerate()
        .flat_map(|(i, id)| {
            scenes
                .get(id)
                .into_iter()
                .flatten()
                .filter_map(move |(path, _)| {
                    let (p, doc) = docs.iter().find(|(p, _)| p == path)?;
                    let effective = priority(&doc.meta.raw_yaml)?;
                    Some((
                        i,
                        p,
                        doc,
                        effective,
                        priority(authored_yaml(&doc.meta.raw_yaml)),
                    ))
                })
        })
        .collect();
    let mut out = Vec::new();
    for &(i, path, doc, effective, own) in &listed {
        let Some(own) = own else { continue };
        let out_of_order = listed.iter().any(|&(j, _, _, other, _)| {
            (j < i && other <= effective) || (j > i && other >= effective)
        });
        if !out_of_order {
            continue;
        }
        let id = &chain.scenes[i];
        out.push((
            path.clone(),
            diag(
                W_CHAPTER_ORDER,
                Severity::Warning,
                format!(
                    "scene `{id}` sets its own `priority: {own}`, so it plays out of the order \
                     the chain on `{on}` lists — on a `select: sequence` occasion the list is the \
                     order within one raise; remove its `priority:` to play it where it is \
                     listed, or move it in the chain's `scenes:`",
                    on = chain.on
                ),
                crate::meta::meta_key_span(&doc.meta, "priority"),
            ),
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chain(on: &str, scenes: &[&str]) -> Chain {
        Chain {
            on: on.to_string(),
            scenes: scenes.iter().map(|s| s.to_string()).collect(),
            applied: true,
            retired: false,
        }
    }

    fn seq() -> MetaDefaults {
        MetaDefaults::default()
            .with_chapters(vec![chain("chapter", &["prologue", "counter", "kitchen"])])
    }

    fn scene(front: &str) -> Document {
        lute_syntax::parse(&format!(
            "---\nkind: scene\n{front}---\n\n## S\n\n@narrator: hi\n"
        ))
        .0
    }

    fn map(doc: &Document) -> serde_yaml::Mapping {
        serde_yaml::from_str(&doc.meta.raw_yaml).unwrap()
    }

    fn typed(doc: &Document) -> crate::meta::TypedMeta {
        crate::meta::parse_meta(
            &doc.meta,
            &lute_manifest::core::load_core_snapshot(),
        )
        .0
    }

    #[test]
    fn listed_scenes_derive_on_after_and_descending_priority() {
        let mut first = scene("id: prologue\n");
        apply_chapters(&mut first, &seq(), &BTreeMap::new());
        let m = map(&first);
        assert_eq!(m.get("on").unwrap().as_str(), Some("chapter"));
        assert!(
            m.get("after").is_none(),
            "the first scene waits for nothing"
        );
        assert_eq!(m.get("priority").unwrap().as_i64(), Some(30));

        let mut third = scene("id: kitchen\n");
        apply_chapters(&mut third, &seq(), &BTreeMap::new());
        let m = map(&third);
        assert_eq!(
            m.get("after").unwrap().as_str(),
            Some("visited(\"counter\")")
        );
        assert_eq!(m.get("priority").unwrap().as_i64(), Some(10));
        assert_eq!(
            derived_after(&third, &typed(&third)).as_deref(),
            Some("kitchen")
        );
    }

    #[test]
    fn each_chain_derives_for_its_own_scenes() {
        let defaults = MetaDefaults::default().with_chapters(vec![
            chain("chapter", &["a1", "a2"]),
            chain("night", &["b1", "b2", "b3"]),
        ]);
        let mut b2 = scene("id: b2\n");
        apply_chapters(&mut b2, &defaults, &BTreeMap::new());
        let m = map(&b2);
        assert_eq!(m.get("on").unwrap().as_str(), Some("night"));
        assert_eq!(m.get("after").unwrap().as_str(), Some("visited(\"b1\")"));
        assert_eq!(m.get("priority").unwrap().as_i64(), Some(20));
    }

    #[test]
    fn explicit_keys_win_and_apply_is_idempotent() {
        let mut doc = scene("id: counter\npriority: 5\nafter: 'visited(\"prologue\")'\n");
        apply_chapters(&mut doc, &seq(), &BTreeMap::new());
        let once = doc.meta.raw_yaml.clone();
        apply_chapters(&mut doc, &seq(), &BTreeMap::new());
        assert_eq!(doc.meta.raw_yaml, once);
        let m = map(&doc);
        assert_eq!(m.get("priority").unwrap().as_i64(), Some(5));
        assert_eq!(m.get("on").unwrap().as_str(), Some("chapter"));
        // T3-18: an `after:` the scene wrote is its own, even when its text
        // is the one the chain would derive.
        assert!(!derived(&doc.meta, "after"));
        assert_eq!(derived_after(&doc, &typed(&doc)), None);
    }

    #[test]
    fn unlisted_scene_and_non_scene_are_untouched() {
        let mut other = scene("id: epilogue\n");
        apply_chapters(&mut other, &seq(), &BTreeMap::new());
        assert_eq!(authored_yaml(&other.meta.raw_yaml), other.meta.raw_yaml);
        let mut quest = lute_syntax::parse(
            "---\nkind: quest\nid: prologue\n---\n<quest id=\"q\" title=\"Q\">\n</quest>\n",
        )
        .0;
        apply_chapters(&mut quest, &seq(), &BTreeMap::new());
        assert_eq!(authored_yaml(&quest.meta.raw_yaml), quest.meta.raw_yaml);
    }

    #[test]
    fn a_scene_an_unapplied_chain_lists_is_marked_not_derived() {
        let mut rejected = chain("chapter", &["prologue"]);
        rejected.applied = false;
        let defaults = MetaDefaults::default().with_chapters(vec![rejected]);
        let mut doc = scene("id: prologue\nwhen: 'true'\n");
        apply_chapters(&mut doc, &defaults, &BTreeMap::new());
        assert!(map(&doc).get("on").is_none());
        assert_eq!(unapplied(&doc.meta), Some(Unapplied::Rejected));
        assert!(!authored_yaml(&doc.meta.raw_yaml).contains("# lute:"));
    }

    #[test]
    fn derived_key_spans_anchor_at_the_scene_id() {
        let mut doc = scene("id: counter\n");
        let id_span = crate::meta::meta_key_span(&doc.meta, "id");
        apply_chapters(&mut doc, &seq(), &BTreeMap::new());
        for key in ["after", "on", "priority", "id"] {
            assert_eq!(
                crate::meta::meta_key_span(&doc.meta, key).byte_start,
                id_span.byte_start,
                "{key}"
            );
        }
    }

    #[test]
    fn locate_finds_each_chain_in_block_and_flow_lists() {
        let block = "defaultProfile: core\nchapters:\n  - on: chapter\n    scenes: [a, b]\n  - on: night\n    scenes: [c, a]\nprofiles: {}\n";
        let at = |m: &str, a: ChapterAnchor| {
            let s = locate(m, &a);
            (s.byte_start, m[s.byte_start..s.byte_end].to_string())
        };
        let (i, t) = at(block, ChapterAnchor::Key(1, "on".into()));
        assert_eq!(t, "on");
        assert!(i > block.find("night").unwrap() - 6);
        let (i, t) = at(block, ChapterAnchor::Entry(1, "a".into(), 0));
        assert_eq!(t, "a");
        assert!(i > block.find("night").unwrap());
        let flow = "chapters: [{ on: chapter, scenes: [a] }, { occasion: night, scenes: [b] }]\n";
        let (i, t) = at(flow, ChapterAnchor::Key(1, "occasion".into()));
        assert_eq!(t, "occasion");
        assert!(i > flow.find("chapter,").unwrap());
    }
}
