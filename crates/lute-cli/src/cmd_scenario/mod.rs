//! `lute scenario` (connectivity T14, dsl §5:571-584) — project-wide,
//! read-only reporting surface over everything §4 computes. Evaluates no
//! CEL, runs no Datalog, takes no mocks: pure graph math over declared
//! structure, reusing [`collect_project_docs`]'s SAME per-root doc grouping
//! `check-project` builds (never a second file-walk/parse) plus the SAME
//! `lute_check::connectivity`/`envelope` analyses `check-project`'s own
//! per-root pass calls (never duplicated math — only the presentation, and
//! the omission of diagnostics, differ).

use std::path::{Path, PathBuf};
use std::process::ExitCode;


use crate::cli::ScenarioCommand;
use crate::cmd_scenario::graph::run_scenario_graph;
use crate::cmd_scenario::node_envelope::run_scenario_envelope;
use crate::cmd_scenario::reach::run_scenario_reach;
use crate::endings;
use crate::knowledge;
use crate::output::write_stdout;
use lute_model::{assemble_root_scenario, RootScenario};
use crate::project::{collect_project_docs, ByRoot};

pub(crate) mod graph;
pub(crate) mod node_envelope;
pub(crate) mod reach;

/// A bare scene-key, `quest:<id>`, `scene:<key>`, or `beat:<doc>.<beat>`
/// node reference, parsed from a `scenario reach`/`scenario envelope` CLI
/// argument (dsl §4.4's `envelope quest:<id>` syntax; `scene:<key>` and
/// `beat:<key>` are its symmetric counterparts -- see [`resolve_node_ref`]'s
/// doc comment for why explicit prefixes exist).
pub(crate) enum NodeRef {
    Scene(String),
    Quest(String),
    /// A bundle beat's canonical id (dsl 0.23.0 §4).
    Beat(String),
}

/// Parse an EXPLICIT `quest:<id>` / `scene:<key>` / `beat:<key>` prefix
/// only -- `None` for a bare (unprefixed) string, which [`resolve_node_ref`]
/// resolves against actual project candidates instead of guessing. An
/// explicit prefix is always authoritative: `quest:foo` is ALWAYS a quest
/// lookup and `scene:foo` is ALWAYS a scene lookup, never re-tried as
/// another kind (that would silently paper over a genuine "no such quest"
/// typo).
fn parse_node_ref_prefix(raw: &str) -> Option<NodeRef> {
    if let Some(id) = raw.strip_prefix("quest:") {
        return Some(NodeRef::Quest(id.to_string()));
    }
    if let Some(key) = raw.strip_prefix("scene:") {
        return Some(NodeRef::Scene(key.to_string()));
    }
    if let Some(key) = raw.strip_prefix("beat:") {
        return Some(NodeRef::Beat(key.to_string()));
    }
    None
}

pub(crate) fn node_ref_to_id(node: &NodeRef) -> lute_check::connectivity::NodeId {
    match node {
        NodeRef::Scene(key) => lute_check::connectivity::NodeId::Scene(key.clone()),
        NodeRef::Quest(id) => lute_check::connectivity::NodeId::Quest(id.clone()),
        NodeRef::Beat(key) => lute_check::connectivity::NodeId::Beat(key.clone()),
    }
}


/// Find EVERY resolved root (sorted, deterministic — [`ByRoot`] is a
/// `BTreeMap`) whose docs declare `node` (a scene key in `scene_key_set` or
/// a declared `<quest id>`), returning each match's root path alongside its
/// assembled [`RootScenario`]. A scene/quest id is only unique WITHIN one
/// resolved project root (dsl §2.3/§6.3) — the SAME id may legitimately
/// exist in two independently resolved sibling roots (the bare `lute
/// scenario` graph view already shows both). Callers MUST treat 2+ matches
/// as an ambiguous lookup (Main review: never silently pick the
/// lexicographically-first root), never collapse to one.
fn find_matching_roots<'a>(
    by_root: &'a ByRoot,
    file_results: &[(PathBuf, lute_check::CheckResult)],
    node: &NodeRef,
) -> Vec<(&'a PathBuf, RootScenario)> {
    let mut out = Vec::new();
    for (root, group_full) in by_root {
        let scenario = assemble_root_scenario(group_full, file_results);
        let present = match node {
            NodeRef::Scene(key) => scenario.key_set.contains_key(key),
            NodeRef::Quest(id) => scenario.quest_ids.contains(id),
            NodeRef::Beat(key) => scenario
                .graph
                .nodes
                .contains_key(&lute_check::connectivity::NodeId::Beat(key.clone())),
        };
        if present {
            out.push((root, scenario));
        }
    }
    out
}

/// Reduce an already-computed [`find_matching_roots`] result to exactly
/// ONE matching root, or `Err(ExitCode::from(2))` with a clear stderr
/// message when it is declared in ZERO roots (unknown node) or 2+ roots
/// (ambiguous — Main review: a scene/quest id is only unique WITHIN one
/// resolved root, dsl §2.3/§6.3; the SAME id may legitimately exist in
/// independent sibling roots, so this NEVER silently picks the
/// lexicographically-first one).
fn pick_unique_root<'a>(
    mut matches: Vec<(&'a PathBuf, RootScenario)>,
    dir: &Path,
    node_id_raw: &str,
) -> Result<(&'a PathBuf, RootScenario), ExitCode> {
    match matches.len() {
        0 => {
            eprintln!("lute: unknown node `{node_id_raw}` under {}", dir.display());
            Err(ExitCode::from(2))
        }
        1 => Ok(matches.pop().expect("len == 1")),
        n => {
            let roots: Vec<String> = matches
                .iter()
                .map(|(r, _)| r.display().to_string())
                .collect();
            eprintln!(
                "lute: node `{node_id_raw}` is declared in {n} different project roots under \
                 {} -- ambiguous (a scene/quest id is only unique WITHIN one resolved project \
                 root); narrow the directory argument to a single project root: \
                 {}",
                dir.display(),
                roots.join(", ")
            );
            Err(ExitCode::from(2))
        }
    }
}

/// Resolve `node_ref` to exactly ONE matching root's [`RootScenario`] --
/// thin wrapper: [`find_matching_roots`] then [`pick_unique_root`].
fn resolve_unique_root<'a>(
    dir: &Path,
    by_root: &'a ByRoot,
    file_results: &[(PathBuf, lute_check::CheckResult)],
    node_ref: &NodeRef,
    node_id_raw: &str,
) -> Result<(&'a PathBuf, RootScenario), ExitCode> {
    let matches = find_matching_roots(by_root, file_results, node_ref);
    pick_unique_root(matches, dir, node_id_raw)
}

/// Resolve a RAW `scenario reach`/`scenario envelope` CLI argument to its
/// [`NodeRef`] plus the single matching root's [`RootScenario`].
///
/// ## Why bare strings are never guessed
/// A scene's canonical key (`{character}.{episodeId}`,
/// `meta::canonical_episode_key`) is an UNVALIDATED, author-controlled
/// string — `character`/`episodeId` accept arbitrary YAML scalars, no
/// charset restriction — so a scene key CAN literally begin with
/// `quest:` (e.g. `character: "quest:foo"`). Unconditionally reserving
/// that prefix for quest lookups (the original design) would make such a
/// scene permanently unselectable. The fix:
/// - An EXPLICIT `quest:<id>` / `scene:<key>` / `beat:<key>` prefix
///   ([`parse_node_ref_prefix`]) is always authoritative — never re-tried as
///   another kind.
/// - A BARE (unprefixed) string is resolved against ACTUAL project
///   candidates: a declared scene key, a declared quest id, or a bundle
///   beat's canonical id (dsl 0.23.0 §4) in some root. Exactly one kind
///   matching → use it (the overwhelmingly common case — no prefix needed
///   at all). Two or more kinds matching is genuinely ambiguous — none is
///   silently preferred; the user is told to disambiguate with an explicit
///   prefix (mirrors [`primary_node_ambiguity_note`]'s honesty pattern:
///   never silently pick one candidate over another equally-valid one).
pub(crate) fn resolve_node_ref<'a>(
    dir: &Path,
    by_root: &'a ByRoot,
    file_results: &[(PathBuf, lute_check::CheckResult)],
    node_id_raw: &str,
) -> Result<(NodeRef, &'a PathBuf, RootScenario), ExitCode> {
    if let Some(explicit) = parse_node_ref_prefix(node_id_raw) {
        return resolve_unique_root(dir, by_root, file_results, &explicit, node_id_raw)
            .map(|(root, scenario)| (explicit, root, scenario));
    }
    let raw = node_id_raw.to_string();
    let mut found: Vec<(NodeRef, Vec<(&'a PathBuf, RootScenario)>)> = [
        NodeRef::Scene(raw.clone()),
        NodeRef::Quest(raw.clone()),
        NodeRef::Beat(raw),
    ]
    .into_iter()
    .map(|r| {
        let matches = find_matching_roots(by_root, file_results, &r);
        (r, matches)
    })
    .filter(|(_, matches)| !matches.is_empty())
    .collect();
    match found.len() {
        0 => {
            eprintln!("lute: unknown node `{node_id_raw}` under {}", dir.display());
            Err(ExitCode::from(2))
        }
        1 => {
            let (node_ref, matches) = found.pop().expect("len == 1");
            pick_unique_root(matches, dir, node_id_raw)
                .map(|(root, scenario)| (node_ref, root, scenario))
        }
        _ => {
            let kinds: Vec<&str> = found
                .iter()
                .map(|(r, _)| match r {
                    NodeRef::Scene(_) => "a scene key",
                    NodeRef::Quest(_) => "a quest id",
                    NodeRef::Beat(_) => "a bundle beat id",
                })
                .collect();
            let prefixes: Vec<String> = found
                .iter()
                .map(|(r, _)| match r {
                    NodeRef::Scene(_) => format!("`scene:{node_id_raw}`"),
                    NodeRef::Quest(_) => format!("`quest:{node_id_raw}`"),
                    NodeRef::Beat(_) => format!("`beat:{node_id_raw}`"),
                })
                .collect();
            eprintln!(
                "lute: node `{node_id_raw}` matches {} in this project -- ambiguous (none is \
                 silently preferred); disambiguate with an explicit {} prefix",
                kinds.join(" and "),
                prefixes.join(" or "),
            );
            Err(ExitCode::from(2))
        }
    }
}

/// `Some(message)` when the PRIMARY requested node itself is ambiguous
/// WITHIN its resolved root — a duplicated scene key
/// (`E-CONN-EPISODE-ID-DUP`, T3: 2+ scene documents computing the same
/// canonical key) or a duplicated quest id (`E-QUEST-ID-DUP`) — in which
/// case neither `reach` nor `envelope` has a single well-defined
/// declaration to report on. Callers MUST check this BEFORE any deeper
/// analysis so neither command ever silently displays one
/// arbitrarily-chosen declaration's data as if it were authoritative
/// (Main review: symmetric honesty treatment for scenes and quests —
/// `assemble_root_scenario`'s own `key_set[key].first()` / graph
/// admission both already pick an arbitrary declaration internally,
/// mirroring `assemble_graph`'s own "anchored at first occurrence"
/// precedent, which is fine for the underlying graph math but must never
/// be surfaced to the user as if it were an unambiguous answer).
pub(crate) fn primary_node_ambiguity_note(
    scenario: &RootScenario,
    node_ref: &NodeRef,
) -> Option<String> {
    match node_ref {
        NodeRef::Scene(key) => {
            let occurrences = scenario.key_set.get(key)?;
            (occurrences.len() > 1).then(|| {
                format!(
                    "ambiguous scene key (E-CONN-EPISODE-ID-DUP): `{key}` is computed by {} \
                     different scene documents in this project root, so a single \
                     reach/envelope report cannot be given.",
                    occurrences.len()
                )
            })
        }
        NodeRef::Quest(id) => scenario.ambiguous_quests.contains(id).then(|| {
            format!(
                "ambiguous quest id (E-QUEST-ID-DUP): `{id}` has more than one declaration in \
                 this project root, so a single reach/envelope report cannot be given."
            )
        }),
        NodeRef::Beat(key) => {
            let occurrences = scenario.beat_keys.get(key)?;
            (occurrences.len() > 1).then(|| {
                format!(
                    "ambiguous bundle beat id (E-CONN-EPISODE-ID-DUP): `{key}` is declared by {} \
                     different lore documents in this project root, so a single reach/envelope \
                     report cannot be given.",
                    occurrences.len()
                )
            })
        }
    }
}

/// `lute scenario` dispatch (dsl §5:571-584): reuses [`collect_project_docs`]
/// — the SAME per-root doc collection `check-project` builds — then routes
/// to the bare graph view, `reach`, or `envelope`.
pub(crate) fn run_scenario(
    dir: &Path,
    providers: Option<&Path>,
    command: Option<ScenarioCommand>,
    facts: bool,
) -> ExitCode {
    let (file_results, by_root) = match collect_project_docs(dir, providers, false) {
        Ok(v) => v,
        Err(code) => return code,
    };
    // T3-15: the report is built into one buffer and written once through
    // [`write_stdout`] — `lute scenario … | head` used to panic on EPIPE
    // from a bare `println!`.
    let mut out = String::new();
    let code = match command {
        None => run_scenario_graph(&mut out, &by_root, facts),
        Some(ScenarioCommand::Reach {
            endings: Some(occasion),
            ..
        }) => endings::run_text(
            &mut out,
            &by_root,
            &file_results,
            (!occasion.is_empty()).then_some(occasion.as_str()),
        ),
        Some(ScenarioCommand::Reach { node_id, .. }) => run_scenario_reach(
            &mut out,
            dir,
            &by_root,
            &file_results,
            node_id.as_deref().unwrap_or_default(),
        ),
        Some(ScenarioCommand::Envelope { node_id }) => {
            run_scenario_envelope(&mut out, dir, &by_root, &file_results, &node_id)
        }
        Some(ScenarioCommand::Knowledge { for_node }) => {
            knowledge::run_text(&mut out, &by_root, &file_results, for_node.as_deref())
        }
    };
    if write_stdout(&out).is_err() {
        return ExitCode::from(2);
    }
    code
}
