//! dsl 0.21.0 §7a.3: `::accept{quest="<id>"}` — the scene-side form of the
//! engine's "accept quest" action. A core directive of the language
//! ([`lute_syntax::ast::ACCEPT_DIRECTIVE`]), never a capability-snapshot
//! lookup.
//!
//! - Per file ([`check_accept_directive`]): `quest` present, a quoted
//!   string, and a quest identifier; `at` (dsl 0.24.0 §2), when present,
//!   the quoted `nextRun`; no other attribute.
//! - `check-project` ([`check_project_accepts`]): the target names a quest
//!   declared in the project, and that quest is accept-driven
//!   ([`accept_driven_quests`]); a `<quest accept="external">` is
//!   accept-driven too (dsl 0.25.0 §5).
//! - `check-project` ([`check_project_never_accepted`]): an accept-driven
//!   quest nothing accepts is [`W_QUEST_NEVER_ACCEPTED`] (dsl 0.24.0 §2);
//!   `accept="external"` is the one non-content acceptance (dsl 0.25.0 §5).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_syntax::ast::{Arm, AttrValue, Directive, Document, Node};

use crate::content_line::E_UNKNOWN_ATTR;

/// `::accept` names no quest, a malformed quest id, a bad `at`, or (at
/// `check-project`) a quest that is not an accept-driven quest of the
/// project (dsl 0.21.0 §7a.3, 0.24.0 §2); also `accept="external"` on a
/// subquest child that activates with its parent (dsl 0.25.0 §5).
pub const E_ACCEPT_TARGET: &str = "E-ACCEPT-TARGET";

/// An accept-driven quest that no `::accept` in the project names and that
/// is not `accept="external"`: nothing ever activates it (dsl 0.24.0 §2,
/// 0.25.0 §5, `check-project` only). A test or trace mock's `accepts:` is
/// no acceptance — it proves the test, not the game.
pub const W_QUEST_NEVER_ACCEPTED: &str = "W-QUEST-NEVER-ACCEPTED";

/// The one legal value of `::accept{… at=}` (dsl 0.24.0 §2): the
/// acceptance is queued and applies after the next `newRun` reset.
const AT_NEXT_RUN: &str = "nextRun";

/// Per-file shape of one `::accept` directive (dsl 0.21.0 §7a.3, 0.24.0
/// §2): [`E_ACCEPT_TARGET`] for a missing, non-string, or non-identifier
/// `quest`, or an `at` other than the quoted `nextRun`; `E-UNKNOWN-ATTR`
/// for any other attribute.
pub fn check_accept_directive(d: &Directive, diags: &mut Vec<Diagnostic>) {
    let mut quest_seen = false;
    for attr in &d.attrs {
        match attr.key.as_str() {
            "quest" => {
                quest_seen = true;
                match &attr.value {
                    AttrValue::Str(id) if crate::check::is_cel_ident(id) => {}
                    AttrValue::Str(id) => diags.push(accept_diag(
                        E_ACCEPT_TARGET,
                        format!(
                            "`::accept` `quest=\"{id}\"` is not a quest id — an identifier \
                             (`[A-Za-z_][A-Za-z0-9_]*`) (dsl 0.21.0 §7a.3)"
                        ),
                        attr.value_span,
                    )),
                    _ => diags.push(accept_diag(
                        E_ACCEPT_TARGET,
                        "`::accept` `quest` must be a quoted quest id (`quest=\"<id>\"`), not a \
                         reference or a bare flag (dsl 0.21.0 §7a.3)"
                            .to_string(),
                        attr.span,
                    )),
                }
            }
            "at" => {
                let span = match &attr.value {
                    AttrValue::Str(v) if v == AT_NEXT_RUN => continue,
                    AttrValue::Str(_) => attr.value_span,
                    _ => attr.span,
                };
                diags.push(accept_diag(
                    E_ACCEPT_TARGET,
                    "`::accept` `at` takes the single value \"nextRun\": `at=\"nextRun\"` queues \
                     the acceptance until after the next `newRun` reset; omit `at` to accept now \
                     (dsl 0.24.0 §2)"
                        .to_string(),
                    span,
                ));
            }
            _ => diags.push(accept_diag(
                E_UNKNOWN_ATTR,
                format!(
                    "`::accept` has no attribute `{}`; its attributes are `quest` and `at` \
                     (dsl 0.21.0 §7a.3, 0.24.0 §2)",
                    attr.key
                ),
                attr.span,
            )),
        }
    }
    if !quest_seen {
        diags.push(accept_diag(
            E_ACCEPT_TARGET,
            "`::accept` names no quest; write `::accept{quest=\"<quest id>\"}` \
             (dsl 0.21.0 §7a.3)"
                .to_string(),
            d.span,
        ));
    }
}

/// One project quest id as the accept rules read it. A duplicated id is
/// `E-QUEST-ID-DUP`'s; its declarations are merged here.
struct QuestFacts<'a> {
    /// Some declaration carries `start` — the quest activates on its own.
    start: bool,
    /// Some declaration carries `activate="accept"` (dsl 0.24.0 §2).
    on_accept: bool,
    /// Some declaration carries `accept="external"` (dsl 0.25.0 §5): the
    /// engine accepts the quest outside any document.
    external: bool,
    /// The first declaration's file and id span.
    anchor: (&'a Path, Span),
}

/// The quest structure of one resolved project root (dsl 0.24.0 §2).
struct ProjectQuests<'a> {
    quests: BTreeMap<&'a str, QuestFacts<'a>>,
    /// Subquest child id → the first parent quest whose `<objective
    /// quest="…">` names it. A self-reference is `E-QUEST-TREE-CYCLE`'s and
    /// never makes a quest a child here.
    parents: BTreeMap<&'a str, &'a str>,
}

impl<'a> ProjectQuests<'a> {
    fn new(docs: &'a [(PathBuf, Document)]) -> Self {
        let mut quests: BTreeMap<&str, QuestFacts> = BTreeMap::new();
        let mut parents: BTreeMap<&str, &str> = BTreeMap::new();
        for (path, doc) in docs {
            for q in doc.quests.iter().filter(|q| !q.id.is_empty()) {
                let facts = quests.entry(q.id.as_str()).or_insert(QuestFacts {
                    start: false,
                    on_accept: false,
                    external: false,
                    anchor: (path.as_path(), q.id_span),
                });
                facts.start |= q.start.is_some();
                facts.on_accept |= q.activates_on_accept();
                facts.external |= q.accepted_externally();
                for node in &q.body {
                    let Node::Objective(o) = node else { continue };
                    if let Some(child) = o.quest.as_deref().filter(|c| *c != q.id) {
                        parents.entry(child).or_insert(q.id.as_str());
                    }
                }
            }
        }
        ProjectQuests { quests, parents }
    }

    /// Accept-driven (dsl 0.21.0 §7a.3, 0.24.0 §2): no `start`, and either
    /// a root quest (no parent names it) or a child declared
    /// `activate="accept"`. `None` for an id no quest declares.
    fn accept_driven(&self, id: &str) -> Option<bool> {
        let q = self.quests.get(id)?;
        Some(!q.start && (q.on_accept || !self.parents.contains_key(id)))
    }
}

/// Every accept-driven quest id of one resolved project root (dsl 0.24.0
/// §2): a quest with no `start` that is either a root (no `<objective
/// quest=…>` in `docs` names it) or a subquest child declared
/// `activate="accept"`. An auto-activating child — one without
/// `activate="accept"` — activates with its parent and is not in the set.
pub(crate) fn accept_driven_quests(docs: &[(PathBuf, Document)]) -> BTreeSet<&str> {
    let project = ProjectQuests::new(docs);
    project
        .quests
        .keys()
        .copied()
        .filter(|id| project.accept_driven(id) == Some(true))
        .collect()
}

/// `check-project` resolution of every well-formed `::accept` target in one
/// resolved project root (dsl 0.21.0 §7a.3, 0.24.0 §2): the quest must be
/// declared in the project and accept-driven — a quest with `start`
/// activates on its own, and a subquest child without `activate="accept"`
/// activates with its parent, so accepting either is a silent no-op. Every
/// fault is [`E_ACCEPT_TARGET`] at the `quest` value. A malformed target is
/// the per-file check's, never re-reported here. The engine's acceptance of
/// a `<quest accept="external">` child that activates with its parent is
/// the same no-op (dsl 0.25.0 §5), [`E_ACCEPT_TARGET`] at the `accept`
/// value; beside `start` it is the per-file `E-ATTR-TYPE`.
pub fn check_project_accepts(docs: &[(PathBuf, Document)]) -> Vec<(PathBuf, Diagnostic)> {
    let project = ProjectQuests::new(docs);
    let mut out = Vec::new();
    for_each_accept(docs, |path, d| check_target(d, &project, path, &mut out));
    for (path, doc) in docs {
        for q in doc.quests.iter().filter(|q| q.accepted_externally()) {
            let (Some(facts), Some(parent), Some((_, span))) =
                (project.quests.get(q.id.as_str()), project.parents.get(q.id.as_str()), &q.accept)
            else {
                continue;
            };
            if facts.on_accept || facts.start {
                continue;
            }
            out.push((
                path.clone(),
                accept_diag(
                    E_ACCEPT_TARGET,
                    format!(
                        "quest `{}` declares `accept=\"external\"`, but it activates with its \
                         parent `{parent}`, so the engine's acceptance does nothing; declare \
                         `activate=\"accept\"` on it too so it waits to be accepted (dsl 0.25.0 §5)",
                        q.id
                    ),
                    *span,
                ),
            ));
        }
    }
    out
}

/// [`W_QUEST_NEVER_ACCEPTED`] over one resolved project root (dsl 0.24.0
/// §2, 0.25.0 §5): every accept-driven quest ([`accept_driven_quests`])
/// that no well-formed `::accept{quest=…}` in `docs` names (either `at`)
/// and that is not `accept="external"`. Anchored at the quest's id.
/// `mocked` — per quest id, the root's trace mocks and scenario tests
/// whose `accepts:` list names it — is no source (a mock proves a test,
/// not the game, dsl 0.25.0 D-C); the warning names them so the author
/// sees why a passing test does not silence it.
pub fn check_project_never_accepted(
    docs: &[(PathBuf, Document)],
    mocked: &BTreeMap<String, Vec<PathBuf>>,
) -> Vec<(PathBuf, Diagnostic)> {
    let project = ProjectQuests::new(docs);
    let mut named: BTreeSet<&str> = BTreeSet::new();
    for_each_accept(docs, |_, d| {
        if let Some((id, _)) = d.accept_quest() {
            named.insert(id);
        }
    });
    let mut out = Vec::new();
    for (id, q) in &project.quests {
        if project.accept_driven(id) != Some(true) || named.contains(id) || q.external {
            continue;
        }
        let only_mocks = match mocked.get(*id).map(Vec::as_slice) {
            None | Some([]) => String::new(),
            Some(files) => {
                let files: Vec<String> =
                    files.iter().map(|f| format!("`{}`", f.display())).collect();
                format!(
                    " (only the `accepts:` mock of {} does, and a test mock is no acceptance in \
                     the game)",
                    files.join(", ")
                )
            }
        };
        let external = "declare `accept=\"external\"` if the engine accepts it outside the \
                        script (a quest board, a menu)";
        let message = match project.parents.get(id) {
            Some(parent) if q.on_accept => format!(
                "quest `{id}` waits for an acceptance (`activate=\"accept\"`), but no \
                 `::accept` in the project names it{only_mocks}, so it never activates; \
                 accept it from a scene with `::accept{{quest=\"{id}\"}}`, {external}, or \
                 remove `activate=\"accept\"` so it activates with its parent `{parent}` \
                 (dsl 0.24.0 §2, 0.25.0 §5)"
            ),
            _ => format!(
                "quest `{id}` is accept-driven (no `start`), but no `::accept` in the project \
                 names it{only_mocks}, so it never activates; accept it from a scene with \
                 `::accept{{quest=\"{id}\"}}`, {external}, or give it a `start` condition \
                 (dsl 0.24.0 §2, 0.25.0 §5)"
            ),
        };
        let (path, span) = q.anchor;
        let mut d = accept_diag(W_QUEST_NEVER_ACCEPTED, message, span);
        d.severity = Severity::Warning;
        out.push((path.to_path_buf(), d));
    }
    out
}

fn check_target(
    d: &Directive,
    project: &ProjectQuests<'_>,
    path: &Path,
    out: &mut Vec<(PathBuf, Diagnostic)>,
) {
    let Some((id, span)) = d.accept_quest() else {
        return;
    };
    if !crate::check::is_cel_ident(id) {
        return;
    }
    let message = match project.quests.get(id) {
        Some(q) if q.start => format!(
            "`::accept` targets quest `{id}`, which has a `start` condition and activates on \
             its own; only an accept-driven quest (no `start`) can be accepted (dsl 0.21.0 §7a.3)"
        ),
        Some(q) => match project.parents.get(id) {
            Some(parent) if !q.on_accept => format!(
                "`::accept` targets quest `{id}`, which activates with its parent `{parent}`; \
                 declare `activate=\"accept\"` on it to accept it from a scene (dsl 0.24.0 §2)"
            ),
            _ => return,
        },
        None => {
            let hint = lute_manifest::suggest::nearest(id, project.quests.keys().copied(), 2)
                .map_or_else(String::new, |near| format!(" — did you mean `{near}`?"));
            format!(
                "`::accept` targets quest `{id}`, but no quest in the project declares that \
                 id{hint} (dsl 0.21.0 §7a.3)"
            )
        }
    };
    out.push((path.to_path_buf(), accept_diag(E_ACCEPT_TARGET, message, span)));
}

/// Every `::accept` directive of `docs`, with its document's path: scene
/// shots, quest bodies, lore entries, and bundle beats.
fn for_each_accept<'a>(docs: &'a [(PathBuf, Document)], mut f: impl FnMut(&'a Path, &'a Directive)) {
    for (path, doc) in docs {
        let mut visit = |d: &'a Directive| f(path.as_path(), d);
        for shot in &doc.shots {
            walk(&shot.body, &mut visit);
        }
        for q in &doc.quests {
            walk(&q.body, &mut visit);
        }
        for e in &doc.entries {
            walk(&e.body, &mut visit);
        }
        for b in &doc.beats {
            walk(&b.body, &mut visit);
        }
    }
}

/// Every `::accept` directive in `nodes`, recursing into every nested body.
/// A `<track>` clip is not walked: `::accept` there is not a staging leaf
/// and never reaches a walk.
pub(crate) fn walk<'a>(nodes: &'a [Node], f: &mut impl FnMut(&'a Directive)) {
    for node in nodes {
        match node {
            Node::Directive(d) if d.is_accept() => f(d),
            Node::Branch(b) => b.choices.iter().for_each(|c| walk(&c.body, f)),
            Node::Hub(h) => h.choices.iter().for_each(|c| walk(&c.body, f)),
            Node::Match(m) => {
                for arm in &m.arms {
                    match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => walk(body, f),
                    }
                }
            }
            Node::Objective(o) => walk(&o.body, f),
            Node::On(o) => walk(&o.body, f),
            Node::Line(_)
            | Node::Directive(_)
            | Node::Set(_)
            | Node::Timeline(_)
            | Node::Assert(_)
            | Node::Retract(_) => {}
        }
    }
}

fn accept_diag(code: &str, message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity: Severity::Error,
        message,
        span,
        layer: Layer::Logic,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}
