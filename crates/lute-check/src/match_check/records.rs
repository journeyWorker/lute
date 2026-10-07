//! `<branch>` / `<hub>` / `<quest>` recording (dsl §11.1, 0.2.0 §5.2): ids,
//! choice/objective checks, and the implicit decls folded into the schema.

use super::*;

/// `E-BRANCH-ALL-GUARDED`: a `<branch>` whose every `<choice>` carries a `when`
/// guard (dsl §11.1, S5). At least one UNGUARDED (`when`-less) choice is REQUIRED
/// — otherwise every guard could be false at once and the branch would present a
/// provably-emptyable menu. (An empty branch is `E-BRANCH-EMPTY`, not this.)
pub const E_BRANCH_ALL_GUARDED: &str = "E-BRANCH-ALL-GUARDED";

/// `E-HUB-NO-EXIT`: a `<hub>` (dsl §7.3.2, §11.1.3, D-C) that can neither exit
/// nor auto-exit. A hub MUST carry at least one UNGUARDED (`when`-less) `exit`
/// choice, OR have EVERY choice flagged `once` (so the eligible set provably
/// empties and auto-exit fires). A hub satisfying neither loops forever.
pub const E_HUB_NO_EXIT: &str = "E-HUB-NO-EXIT";

/// `E-OBJECTIVE-QUEST-DONE`: an `<objective>` carries BOTH `quest=` (a
/// subquest reference, subquest design 2026-08-31 §1) and a non-empty
/// `done=` completion predicate. The two are mutually exclusive: `done`
/// authors the completion predicate directly; `quest` DELEGATES it to the
/// referenced child quest, which `lute-compile` synthesises downstream as
/// `quest.<child>.state == 'complete'` (spec §2.1). Anchored at the
/// `quest=` attribute value — that is the addition that turned a
/// well-formed authored `done` into an over-specification.
pub const E_OBJECTIVE_QUEST_DONE: &str = "E-OBJECTIVE-QUEST-DONE";

/// `E-QUEST-TREE-CYCLE`: the parent→child edges induced by `<objective
/// quest=…>` form a cycle (subquest design 2026-08-31 §4 — the tree, not
/// DAG, invariant). The general project-wide walk lives in
/// `check_project`; here we catch the doc-local length-1 case — an
/// objective that names its OWN enclosing quest as its child. Anchored at
/// the offending `quest=` attribute value.
pub const E_QUEST_TREE_CYCLE: &str = "E-QUEST-TREE-CYCLE";

/// One branch's recording result: the implicit `scene.choices.<id>` decl to fold
/// into the schema, plus any diagnostic (`E-DUP-BRANCH`).
#[derive(Clone, Debug)]
pub struct BranchRecord {
    /// The declared state path, e.g. `scene.choices.couch`.
    pub path: String,
    /// The implicit declaration (`enum` of choice ids, scene-scoped, no default).
    pub decl: StateDecl,
    /// Diagnostics for this branch (currently only `E-DUP-BRANCH`).
    pub diags: Vec<Diagnostic>,
}

/// One hub's recording result (dsl §11.1.3): the implicit declarations to fold
/// into the schema — `scene.choices.<hubId>` (enum of choice ids ∪ `unset`) plus
/// a per-choice `scene.visited.<hubId>.<choiceId>: bool` (default `false`) — and
/// any diagnostics (`E-DUP-BRANCH`, `E-CHOICE-DUP`, `E-HUB-NO-EXIT`).
#[derive(Clone, Debug)]
pub struct HubRecord {
    /// The implicit declarations (path -> decl) in document order.
    pub decls: Vec<(String, StateDecl)>,
    /// Diagnostics for this hub.
    pub diags: Vec<Diagnostic>,
}

/// A written branch, hub or choice id that is not a name
/// (`E-PATH-IDENT`): it is a segment of `scene.choices.<id>` and
/// `scene.visited.<hub>.<choice>`. A missing id has its own report.
fn ident_diag(what: &str, id: &str, span: Span) -> Option<Diagnostic> {
    if id.is_empty() {
        return None;
    }
    lute_manifest::ident::name_fault(what, id)
        .map(|message| diag(E_PATH_IDENT, Severity::Error, message, span))
}

/// Record a `<branch>` (dsl §11.1): flag a duplicate id within the episode
/// (`E-DUP-BRANCH`) and return the implicit `scene.choices.<id>` declaration.
/// `seen` is the caller-owned, document-order set of branch ids seen so far (see
/// the module docs for why dup-detection is threaded rather than held in `Ctx`).
pub fn check_branch(branch: &Branch, seen: &mut BTreeSet<String>) -> BranchRecord {
    let path = format!("scene.choices.{}", branch.id);
    let mut diags = Vec::new();
    diags.extend(ident_diag("branch id", &branch.id, branch.id_span));
    for choice in &branch.choices {
        diags.extend(ident_diag("choice id", &choice.id, choice.id_span));
    }
    // `insert` returns `false` when the id was already present => a duplicate.
    if !seen.insert(branch.id.clone()) {
        diags.push(diag(
            "E-DUP-BRANCH",
            Severity::Error,
            format!(
                "duplicate `<branch id=\"{}\">`; branch ids must be unique within the episode \
                 (dsl §11.1)",
                branch.id
            ),
            branch.span,
        ));
    }
    // E-BRANCH-EMPTY (dsl §7.3, `Branch ::= "<branch" Attrs ">" Choice+`): a
    // `<branch>` MUST carry at least one `<choice>`. An empty branch flattens to
    // a `choice` record with no options — unroutable, since a choice never falls
    // through (§7.1) — so reject it here before the compile gate.
    if branch.choices.is_empty() {
        diags.push(diag(
            "E-BRANCH-EMPTY",
            Severity::Error,
            format!(
                "empty `<branch id=\"{}\">`; a branch must contain at least one `<choice>` \
                 (dsl §7.3 `Choice+`)",
                branch.id
            ),
            branch.span,
        ));
    }
    // E-CHOICE-DUP (dsl §11.1): each `<choice id>` MUST be unique within its
    // branch — both the recorded value's domain and the option-label lineId
    // (`{branchId}.{choiceId}`, §12) key on it. One diagnostic per repeat, at
    // the duplicate choice's span.
    let mut choice_ids: BTreeSet<&str> = BTreeSet::new();
    for choice in &branch.choices {
        if !choice_ids.insert(choice.id.as_str()) {
            diags.push(diag(
                "E-CHOICE-DUP",
                Severity::Error,
                format!(
                    "duplicate `<choice id=\"{}\">` within `<branch id=\"{}\">`; choice ids \
                     must be unique within a branch (dsl §11.1)",
                    choice.id, branch.id
                ),
                choice.span,
            ));
        }
    }
    // E-BRANCH-ALL-GUARDED (dsl §11.1, S5): a non-empty branch whose EVERY
    // `<choice>` carries a `when` guard could have every guard false at a
    // presentation point, leaving an empty menu. At least one unguarded
    // (`when`-less) choice is REQUIRED. (An empty branch is already
    // `E-BRANCH-EMPTY` above; we skip it here to avoid double-flagging.)
    if !branch.choices.is_empty() && branch.choices.iter().all(|c| c.when.is_some()) {
        diags.push(diag(
            E_BRANCH_ALL_GUARDED,
            Severity::Error,
            format!(
                "`<branch id=\"{}\">` has no unguarded `<choice>`; every choice carries a \
                 `when`, so the menu could be empty — a branch must contain at least one \
                 unguarded choice (dsl §11.1)",
                branch.id
            ),
            branch.span,
        ));
    }
    // Implicit decl: enum of the branch's choice ids, scene-scoped, no default
    // (so it is maybe-unset — the domain is choice ids ∪ `unset`, §11.1).
    let members = branch.choices.iter().map(|c| c.id.clone()).collect();
    let decl = StateDecl {
        ty: Type::Enum(members),
        default: None,
        namespace: Namespace::Scene,
        owner: None,
    };
    BranchRecord { path, decl, diags }
}

/// Record a `<hub>` (dsl §7.3.2, §11.1.3), mirroring [`check_branch`]. Emits:
/// `E-DUP-BRANCH` if the hub id collides in the shared per-episode `seen` set
/// (hub and branch ids record under one `scene.choices.*` domain); `E-CHOICE-DUP`
/// on a repeated choice id (a reserved choice id is `E-RESERVED-NAME`,
/// `crate::reserved_names`); `E-HUB-NO-EXIT` unless the hub has an UNGUARDED
/// `exit` choice OR every choice is `once`. Returns the implicit recording decls:
/// `scene.choices.<hubId>` (enum of choice ids ∪ `unset`, like a branch) plus a
/// per-choice `scene.visited.<hubId>.<choiceId>: bool` (default `false`, §9.6).
/// The `once`/`exit` flags stay as attrs on each choice.
pub fn check_hub(hub: &Hub, seen: &mut BTreeSet<String>) -> HubRecord {
    let id = attr_str(&hub.attrs, "id").unwrap_or("");
    let mut diags = Vec::new();
    let id_span = hub
        .attrs
        .iter()
        .find(|a| a.key == "id")
        .map_or(hub.span, |a| a.value_span);
    diags.extend(ident_diag("hub id", id, id_span));
    for choice in &hub.choices {
        diags.extend(ident_diag("choice id", &choice.id, choice.id_span));
    }

    // E-DUP-BRANCH (§11.1.3): hub and branch ids share ONE per-episode uniqueness
    // domain (both record under `scene.choices.*`), so a hub id may not collide
    // with a branch id (or another hub id) in the same episode.
    if !seen.insert(id.to_string()) {
        diags.push(diag(
            "E-DUP-BRANCH",
            Severity::Error,
            format!(
                "duplicate id `<hub id=\"{id}\">`; hub and branch ids share one uniqueness \
                 domain and must be unique within the episode (dsl §11.1.3)"
            ),
            hub.span,
        ));
    }

    // E-CHOICE-DUP (§11.1.3, reusing §11.1): each choice id MUST be unique
    // WITHIN the hub (it keys the recorded value + the option-label lineId,
    // §12). One diagnostic per repeated choice, at its span.
    let mut choice_ids: BTreeSet<&str> = BTreeSet::new();
    for choice in &hub.choices {
        if !choice_ids.insert(choice.id.as_str()) {
            diags.push(diag(
                "E-CHOICE-DUP",
                Severity::Error,
                format!(
                    "duplicate `<choice id=\"{}\">` within `<hub id=\"{id}\">`; choice ids must \
                     be unique within a hub (dsl §11.1.3)",
                    choice.id
                ),
                choice.span,
            ));
        }
    }

    // E-HUB-NO-EXIT (§7.3.2, §11.1.3, D-C): a hub can terminate iff it has at
    // least one UNGUARDED (`when`-less) `exit` choice, OR every choice is `once`
    // (the eligible set provably empties → auto-exit). An empty hub is neither.
    // A flag written with a value that is no flag (`exit="yes"`) is taken as
    // meant: its `E-FLAG-VALUE` is the one report (tea-hollin TH28-4b).
    let meant = |c: &lute_syntax::ast::Choice, key: &str| {
        c.attrs
            .iter()
            .find(|a| a.key == key)
            .is_some_and(|a| a.value.flag() != Some(false))
    };
    let has_unguarded_exit = hub
        .choices
        .iter()
        .any(|c| c.when.is_none() && meant(c, "exit"));
    let all_once = !hub.choices.is_empty() && hub.choices.iter().all(|c| meant(c, "once"));
    if !has_unguarded_exit && !all_once {
        // A choice named like the exit, missing only the flag: point at it.
        // One with an `exit=` value that is no flag has its own E-FLAG-VALUE.
        let named_exit = hub.choices.iter().find(|c| {
            c.when.is_none()
                && !c.attrs.iter().any(|a| a.key == "exit")
                && (c.id == "exit" || lute_manifest::suggest::nearest(&c.id, ["exit"], 1).is_some())
        });
        let d = match named_exit {
            Some(c) => {
                let label = if c.text.is_empty() {
                    String::new()
                } else {
                    format!(" text=\"{}\"", c.text)
                };
                diag(
                    E_HUB_NO_EXIT,
                    Severity::Error,
                    format!(
                        "`<hub id=\"{id}\">` can never exit: choice `{cid}` is not an exit — \
                         add the `exit` flag: `<choice id=\"{cid}\"{label} exit>`",
                        cid = c.id
                    ),
                    c.span,
                )
            }
            None => diag(
                E_HUB_NO_EXIT,
                Severity::Error,
                format!(
                    "`<hub id=\"{id}\">` can never exit; it needs at least one unguarded \
                     (`when`-less) `exit` choice, or every choice must be `once` so the eligible \
                     set provably empties (dsl §7.3.2, §11.1.3)"
                ),
                hub.span,
            ),
        };
        diags.push(d);
    }

    // Implicit recording decls (§9.6, §11.1.3):
    //  - `scene.choices.<hubId>`: enum of the hub's choice ids, scene-scoped, no
    //    default (maybe-unset; domain = choice ids ∪ `unset`), MIRRORING a branch.
    //  - per choice `scene.visited.<hubId>.<choiceId>: bool` default `false` — the
    //    per-choice "taken" flag, a NEW reserved namespace kept SEPARATE from
    //    `scene.choices.*` so `<hubId>` is both a leaf and a parent (§9.6).
    let mut decls: Vec<(String, StateDecl)> = Vec::new();
    let members = hub.choices.iter().map(|c| c.id.clone()).collect();
    decls.push((
        format!("scene.choices.{id}"),
        StateDecl {
            ty: Type::Enum(members),
            default: None,
            namespace: Namespace::Scene,
            owner: None,
        },
    ));
    for choice in &hub.choices {
        decls.push((
            format!("scene.visited.{id}.{}", choice.id),
            StateDecl {
                ty: Type::Bool,
                default: Some(Literal::Bool(false)),
                namespace: Namespace::Scene,
                owner: None,
            },
        ));
    }

    HubRecord { decls, diags }
}

/// One quest's recording result (dsl 0.2.0 §5.2, §6.3, §6.4): the folded
/// reserved `quest.<id>.*` decls to fold into the schema, plus any
/// diagnostics (`E-QUEST-ID-DUP`, `E-OBJECTIVE-ID-DUP`,
/// `E-OBJECTIVE-MISSING-DONE`, plus the subquest doc-level pair
/// `E-OBJECTIVE-QUEST-DONE` / `E-QUEST-TREE-CYCLE` — subquest design
/// 2026-08-31 §1/§4).
#[derive(Clone, Debug)]
pub struct QuestRecord {
    /// The implicit reserved declarations (path -> decl), quest-state first
    /// then per-objective `done` in document order.
    pub decls: Vec<(String, StateDecl)>,
    /// Diagnostics for this quest.
    pub diags: Vec<Diagnostic>,
}

/// Record a `<quest>` (dsl 0.2.0 §5.2, §6.3, §6.4), mirroring [`check_hub`].
/// Emits `E-QUEST-ID-DUP` on a repeat id in the caller-owned `seen_quests` set
/// — a namespace SEPARATE from the branch/hub `scene.choices.*` `seen` set
/// (quest ids key the `quest.<id>.*` tier, dsl 0.2.0 §5.2); `E-OBJECTIVE-ID-DUP`
/// on a repeated `<objective id>` WITHIN this quest; `E-OBJECTIVE-MISSING-DONE`
/// on an `<objective>` whose `done` slot is empty AND no `quest=` reference
/// stands in for it (the parser always yields a syntactically valid — possibly
/// empty — CEL slot for a missing `done`, dsl 0.2.0 §6.4). Objectives are
/// found by scanning `quest.body` for `Node::Objective` — grammar admission
/// (Task 5) guarantees they appear only directly in a quest body, never nested.
///
/// Subquest surface (subquest design 2026-08-31): an `<objective quest="c">`
/// delegates its completion to child quest `c`. Doc-level checks here:
/// `E-OBJECTIVE-QUEST-DONE` when `quest=` and a non-empty `done=` coexist
/// (§1 — mutually exclusive), and `E-QUEST-TREE-CYCLE` when `quest=` names
/// the enclosing quest itself (§4 — length-1 cycle, doc-local early catch;
/// deeper cycles and cross-doc `quest=` resolution are `check_project`'s
/// job). An unknown `quest=` id stays silent here — it may resolve in
/// another artifact.
///
/// `id`/`<objective id>` are REQUIRED (dsl 0.2.0 §6.3/§6.4); the parser still
/// yields a syntactically valid AST with `id = ""` for a missing attr (the
/// same empty-slot idiom as a missing `done`, blocks.rs). An empty quest id is
/// `E-QUEST-ID-MISSING`, an empty objective id is `E-OBJECTIVE-ID-MISSING` —
/// EITHER short-circuits the corresponding reserved-decl fold (below) so a
/// malformed `quest..state` / `quest.<id>.objectives..done` path never reaches
/// the schema; every other per-construct diagnostic (dup / hyphen / missing
/// `done`) still runs so a malformed id doesn't hide its siblings' problems.
///
/// Returns the implicit reserved decls (dsl 0.2.0 §5.2): `quest.<id>.state`
/// (an enum `[active, complete, failed]`, deterministic order, no default —
/// `lute-compile` appends the `unset` member; the checker reads the path as
/// the always-assigned lifecycle enum, [`infer_domain`], 0.21.1 T1-1) plus, per objective,
/// `quest.<id>.objectives.<oid>.done: bool` (default `false`) — omitted for a
/// quest or objective with a missing id (see above).
pub fn check_quest(quest: &Quest, seen_quests: &mut BTreeSet<String>) -> QuestRecord {
    let id = quest.id.as_str();
    let mut diags = Vec::new();
    crate::logic_attrs::check_quest_attrs(quest, &mut diags);

    if id.is_empty() {
        diags.push(diag(
            "E-QUEST-ID-MISSING",
            Severity::Error,
            "`<quest>` has no `id`; a quest id is required (dsl 0.2.0 §6.3)".to_string(),
            quest.id_span,
        ));
    } else {
        if !seen_quests.insert(id.to_string()) {
            diags.push(diag(
                "E-QUEST-ID-DUP",
                Severity::Error,
                format!(
                    "duplicate `<quest id=\"{id}\">`; quest ids must be unique (dsl 0.2.0 §6.3)"
                ),
                quest.id_span,
            ));
        }

        // §8.4 CelIdent alignment: the quest id is ONE CEL-facing segment of
        // the reserved `quest.<id>.state`/`quest.<id>.objectives.*` paths — a
        // `-` (CEL subtraction) or a `.` (a second segment) there is illegal.
        // Still fold the decl below so downstream reads don't cascade to
        // E-UNDECLARED (mirrors how meta.rs treats a hyphenated inline
        // `state:` path).
        if let Some(message) = crate::cel_paths::quest_id_fault("quest", id) {
            diags.push(diag(E_PATH_IDENT, Severity::Error, message, quest.id_span));
        }
    }

    // A missing quest id makes every `quest.<id>.*` path malformed
    // (`quest..state`, `quest..objectives.<oid>.done`) — fold nothing for this
    // quest rather than poison the schema with an unaddressable path.
    let mut decls: Vec<(String, StateDecl)> = if id.is_empty() {
        Vec::new()
    } else {
        vec![
            (
                format!("quest.{id}.state"),
                StateDecl {
                    ty: Type::Enum(vec![
                        "active".to_string(),
                        "complete".to_string(),
                        "failed".to_string(),
                    ]),
                    default: None,
                    namespace: Namespace::Quest,
                    owner: None,
                },
            ),
            // dsl 0.8.0 §5: the quest-instance activation instant — the
            // author-readable `t` `validAt(rel, t)` never had. Engine-
            // populated at the `unset → active` transition, so `default:
            // None` exactly like `quest.<id>.state`; narrative time is
            // OPAQUE (no literal inhabits `Type::NarrativeTime`), so it is
            // never author-declarable (`E-QUEST-RESERVED-DECL`) nor
            // author-writable (`E-QUEST-RESERVED-WRITE`).
            (
                format!("quest.{id}.activatedAt"),
                StateDecl {
                    ty: Type::NarrativeTime,
                    default: None,
                    namespace: Namespace::Quest,
                    owner: None,
                },
            ),
        ]
    };

    let mut objective_ids: BTreeSet<&str> = BTreeSet::new();
    for node in &quest.body {
        let Node::Objective(o) = node else { continue };
        if o.id.is_empty() {
            diags.push(diag(
                "E-OBJECTIVE-ID-MISSING",
                Severity::Error,
                format!(
                    "an `<objective>` within `<quest id=\"{id}\">` has no `id`; an objective \
                     id is required (dsl 0.2.0 §6.4)"
                ),
                o.id_span,
            ));
        } else {
            if !objective_ids.insert(o.id.as_str()) {
                diags.push(diag(
                    "E-OBJECTIVE-ID-DUP",
                    Severity::Error,
                    format!(
                        "duplicate `<objective id=\"{}\">` within `<quest id=\"{id}\">`; objective \
                         ids must be unique within a quest (dsl 0.2.0 §6.4)",
                        o.id
                    ),
                    o.span,
                ));
            }
            // §8.4 CelIdent alignment: the objective id is a CEL-facing segment
            // of `quest.<id>.objectives.<oid>.done` — the quest id's rule.
            if let Some(message) = crate::cel_paths::quest_id_fault("objective", &o.id) {
                diags.push(diag(E_PATH_IDENT, Severity::Error, message, o.id_span));
            }
        }
        // Subquest triage (subquest design 2026-08-31 §1): `quest=` and a
        // non-empty `done=` are mutually exclusive — `done` authors the
        // completion predicate directly, `quest` DELEGATES it to a child
        // (synthesised downstream as `quest.<child>.state == 'complete'`,
        // spec §2.1). A `quest=` reference SATISFIES the "completion
        // predicate required" obligation, so `E-OBJECTIVE-MISSING-DONE`
        // must NOT fire on an empty `done` when `quest=` is present.
        // Empty-CEL-slot diagnostics elsewhere already treat an empty raw
        // as a structural gap (lute-cel `fill.rs`: no `E-CEL-PARSE`; the
        // check-pass `check_cel_slot` is a no-op on a `None` ast), so no
        // further suppression is needed here.
        // A misspelt `done=` (`complete=`, `doen=`) is already an
        // `E-UNKNOWN-ATTR` that names `done`: one cause, one report.
        let done_misspelt = o.attrs.iter().any(|a| {
            let keys = crate::logic_attrs::OBJECTIVE_ATTRS;
            !keys.contains(&a.key.as_str())
                && lute_manifest::suggest::nearest(&a.key, keys.iter().copied(), 2) == Some("done")
        });
        match (&o.quest, o.done.raw.trim().is_empty()) {
            (Some(_), false) => diags.push(diag(
                E_OBJECTIVE_QUEST_DONE,
                Severity::Error,
                format!(
                    "`<objective id=\"{}\">` within `<quest id=\"{id}\">` carries BOTH `quest=` \
                     and a non-empty `done=`; the two are mutually exclusive — `done` authors the \
                     completion predicate, `quest` delegates it to the referenced child (subquest \
                     design 2026-08-31 §1)",
                    o.id
                ),
                o.quest_span,
            )),
            (None, true) if !done_misspelt => diags.push(diag(
                "E-OBJECTIVE-MISSING-DONE",
                Severity::Error,
                format!(
                    "`<objective id=\"{}\">` within `<quest id=\"{id}\">` has no `done` \
                     completion predicate; `done` is required (dsl 0.2.0 §6.4)",
                    o.id
                ),
                o.span,
            )),
            // (Some(_), true): subquest delegation — completion synthesised
            // downstream. (None, false): plain authored predicate — nothing
            // to flag here.
            _ => {}
        }
        // Doc-local length-1 cycle (subquest design 2026-08-31 §4 — tree,
        // not DAG): an objective whose `quest=` names its own enclosing
        // quest is a self-parent. The general parent→child cycle walk is
        // project-wide (`check_project`); catching the length-1 case here
        // means an author sees it without needing a project pass, and
        // matches the spec's "same-document self-reference is caught
        // early" note (§4 table). Non-empty `id` gate: an empty enclosing
        // id is already `E-QUEST-ID-MISSING` above, and the `quest=`
        // string cannot syntactically be empty either (attr parsing).
        if !id.is_empty() && o.quest.as_deref() == Some(id) {
            diags.push(diag(
                E_QUEST_TREE_CYCLE,
                Severity::Error,
                format!(
                    "`<objective id=\"{}\" quest=\"{id}\">` within `<quest id=\"{id}\">` names its \
                     own enclosing quest as its subquest — a length-1 cycle in the parent→child \
                     tree (subquest design 2026-08-31 §4)",
                    o.id
                ),
                o.quest_span,
            ));
        }
        // A malformed (empty) quest OR objective id makes this decl's path
        // unaddressable — skip folding it (`E-QUEST-ID-MISSING` /
        // `E-OBJECTIVE-ID-MISSING` already flagged the construct above).
        if !id.is_empty() && !o.id.is_empty() {
            decls.push((
                format!("quest.{id}.objectives.{}.done", o.id),
                StateDecl {
                    ty: Type::Bool,
                    default: Some(Literal::Bool(false)),
                    namespace: Namespace::Quest,
                    owner: None,
                },
            ));
        }
    }

    QuestRecord { decls, diags }
}

/// The plain string value of the attr keyed `key`, if present and a string
/// literal (`key="s"`). A bare/`@ref` value or a missing key yields `None`.
fn attr_str<'a>(attrs: &'a [Attr], key: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|a| a.key == key)
        .and_then(|a| match &a.value {
            AttrValue::Str(s) => Some(s.as_str()),
            _ => None,
        })
}
