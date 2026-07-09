//! `<match>` exhaustiveness + first-match-wins lint, and `<branch>` recording
//! (dsl §11.1, §11.2).
//!
//! ## `<match>` (§11.2)
//! Arms run **top-to-bottom, first match wins**. An `<otherwise>` is REQUIRED
//! unless the subject's domain is *finite* and *fully covered* by the `<when>`
//! arms. Domain finiteness is inferred from the subject's declared `state:` type
//! ([`StateSchema`]): a `bool` (domain `{true,false}`), an `enum` (domain = its
//! members), or a `scene.choices.<branchId>` path (domain = the branch's choice
//! ids ∪ `unset`) is FINITE; anything else (number/string/record/…) is INFINITE
//! and therefore requires `<otherwise>`.
//!
//! Coverage is computed from the arms' **`is` literal patterns** (§7.3.1) — the
//! NORMATIVE path (§11.2) — unioned with the conservative values a `test` guard
//! provably matches (recognizing `test` coverage is downgraded to MAY). An `is`
//! pattern (`Literal ("|" Literal)*`) contributes each literal to `covered`: an
//! enum member / choice id → the string value, `true`/`false` → the bool value,
//! a decimal `Number` → a numeric value, and `unset` → the unset case.
//! Diagnostics:
//!
//! - **`E-NONEXHAUSTIVE`** — no `<otherwise>` and the domain is either infinite,
//!   or finite but not every domain value is covered by a `<when>` arm.
//! - **`E-WHEN-PATTERN`** — a `<when>` arm with neither an `is` pattern nor a
//!   `test` guard (§7.3.1); one of the two is REQUIRED.
//! - **`E-UNSET-UNCOVERED`** — the subject is *maybe-unset* (`scene.choices.*`, or
//!   a `run.*`/`user.*`/`app.*` decl with no schema `default`) and the `unset`
//!   case is not covered by an `unset`-matching arm nor an `<otherwise>`.
//! - **`E-AGE-GATE`** — an age-gated `<match on="app.rating">` that covers neither
//!   a `teen` arm nor an `<otherwise>` (a release-build hard gate, §11.2).
//! - **`E-MATCH-DUP-OTHERWISE`** — more than one `<otherwise>`; §11.2 allows at
//!   most one. Flatten routes only the last, so earlier otherwise bodies would be
//!   unreachable. Flagged at every `<otherwise>` past the first.
//! - **`W-OVERLAP-ARMS`** (Warning) — two `<when>` arms that *provably* match the
//!   same value (kept conservative: identical literal equality tests only, never
//!   general SAT). First-match-wins means the later arm is dead.
//!
//! ## `<branch>` (§11.1)
//! `<branch id>` MUST be unique within the episode (the `.lute` document);
//! selecting a choice records `scene.choices.<branchId> = <choiceId>`, an
//! implicitly-declared, episode-scoped path whose domain is the branch's choice
//! ids ∪ `unset`. [`check_branch`] emits **`E-DUP-BRANCH`** on a repeat id,
//! **`E-BRANCH-EMPTY`** on a branch with no `<choice>` (§7.3 requires `Choice+`;
//! an empty branch would flatten to an unroutable choice), **`E-CHOICE-DUP`** on
//! a repeated choice id, and returns the implicit [`StateDecl`] to fold into the
//! schema.
//!
//! ### Branch-dup threading (for T4.9 assembly)
//! Duplicate detection is *episode-wide*, but this module checks one branch at a
//! time. Rather than hide episode state in `Ctx` (which is per-check and cloned),
//! the caller threads a `&mut BTreeSet<String>` of seen branch ids in **document
//! order** and folds each returned [`BranchRecord::decl`] into the accumulating
//! `StateSchema`. This keeps `Ctx` immutable and the episode set explicit and
//! caller-owned — the T4.9 whole-document walk already iterates shots/nodes in
//! order, so it owns the set and the schema it grows.
//!
//! ## Spans / layer
//! All diagnostics are [`Layer::Logic`] (§9/§11 logic checks). Per the cel-parser
//! 0.10.1 carry-forward (T3.1/T4.3) arm-test byte offsets are unavailable, so
//! coverage is reconstructed from a throwaway re-parse of each slot's raw CEL and
//! diagnostics fall back to the enclosing match/arm/branch span.

use std::collections::BTreeSet;

use cel_parser::ast::Expr;
use cel_parser::reference::Val;
use lute_cel::CelArena;
use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::types::{Literal, Type};
use lute_syntax::ast::{Arm, Attr, AttrValue, Branch, Document, Hub, IsPattern, Line, Match, Node};

use crate::meta::{Namespace, StateDecl, StateSchema};
use crate::Ctx;

/// `E-WHEN-PATTERN`: a `<when>` arm carrying neither an `is` literal pattern nor
/// a `test` guard (dsl §7.3.1, D-D). One of the two is REQUIRED — an empty
/// `<when>` has nothing to match on.
pub const E_WHEN_PATTERN: &str = "E-WHEN-PATTERN";

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

/// A concrete, statically-known value an arm can match against.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum DomainValue {
    Str(String),
    Bool(bool),
    /// A decimal `Number` literal (dsl §7.3.1), kept as its trimmed source text.
    /// A numeric subject has an INFINITE domain, so a `Num` never completes
    /// coverage — it is carried only for union/overlap accounting.
    Num(String),
}

/// The inferred value domain of a `<match>` subject (dsl §11.2).
#[derive(Clone, Debug, PartialEq, Eq)]
enum Domain {
    /// Finite domain with a known, enumerable set of values.
    Finite(Vec<DomainValue>),
    /// Infinite / unknowable domain (number, string, unresolved subject): an
    /// `<otherwise>` is mandatory.
    Infinite,
}

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

/// Validate a `<match>` for exhaustiveness, unset coverage, the age-gate, and
/// provably-overlapping arms (dsl §11.2). All diagnostics are [`Layer::Logic`].
pub fn check_match(m: &Match, schema: &StateSchema, ctx: &Ctx<'_>) -> Vec<Diagnostic> {
    let _ = ctx; // reserved: subject typing is owned by T4.3; unused here.
    let mut diags = Vec::new();
    let subject = subject_path(m);
    let info = infer_domain(subject.as_deref(), schema);
    let has_otherwise = m.arms.iter().any(|a| matches!(a, Arm::Otherwise { .. }));

    // §11.2: a `<match>` admits AT MOST ONE `<otherwise>`. With more than one,
    // flatten routes only the last, making earlier otherwise bodies unreachable
    // — so flag every otherwise past the first at its own span (mirroring the
    // per-repeat shape of E-CHOICE-DUP).
    let mut seen_otherwise = false;
    for arm in &m.arms {
        if let Arm::Otherwise { span, .. } = arm {
            if seen_otherwise {
                diags.push(diag(
                    "E-MATCH-DUP-OTHERWISE",
                    Severity::Error,
                    "duplicate `<otherwise>` in `<match>`; at most one `<otherwise>` is allowed \
                     (dsl §11.2)"
                        .to_string(),
                    *span,
                ));
            }
            seen_otherwise = true;
        }
    }

    // One ordered pass over the `<when>` arms: accumulate covered values (+ the
    // `unset` case) and flag a provably-dead overlap. First-match-wins means an
    // arm whose concrete value was already covered by an EARLIER arm is dead.
    let mut covered: BTreeSet<DomainValue> = BTreeSet::new();
    let mut covers_unset = false;
    for arm in &m.arms {
        if let Arm::When { is, test, span, .. } = arm {
            // §7.3.1 (D-D): a `<when>` needs an `is` pattern and/or a `test` guard.
            if is.is_none() && test.raw.trim().is_empty() {
                diags.push(diag(
                    E_WHEN_PATTERN,
                    Severity::Error,
                    "`<when>` needs an `is` literal pattern and/or a `test` guard (dsl §7.3.1)"
                        .to_string(),
                    *span,
                ));
            }
            let cov = arm_coverage(is.as_ref(), &test.raw, subject.as_deref());
            if cov.values.iter().any(|v| covered.contains(v)) {
                diags.push(diag(
                    "W-OVERLAP-ARMS",
                    Severity::Warning,
                    "this `<when>` provably overlaps an earlier arm; first-match-wins makes it \
                     unreachable (dsl §11.2)"
                        .to_string(),
                    *span,
                ));
            }
            for v in cov.values {
                covered.insert(v);
            }
            covers_unset |= cov.covers_unset;
        }
    }

    // Age-gate special case (§11.2): an age-gated `<match on="app.rating">` MUST
    // carry a `teen` arm or an `<otherwise>` — a release-build hard gate.
    if subject.as_deref() == Some("app.rating")
        && !has_otherwise
        && !covered.contains(&DomainValue::Str("teen".to_string()))
    {
        diags.push(diag(
            "E-AGE-GATE",
            Severity::Error,
            "age-gated `<match on=\"app.rating\">` must cover a `teen` arm or carry an \
             `<otherwise>` (dsl §11.2)"
                .to_string(),
            m.span,
        ));
    }

    // An `<otherwise>` makes the match exhaustive and covers `unset` (§11.2).
    if has_otherwise {
        return diags;
    }

    let fully_covered = match &info.domain {
        Domain::Finite(vals) => vals.iter().all(|v| covered.contains(v)),
        Domain::Infinite => false,
    };
    if !fully_covered {
        diags.push(diag(
            "E-NONEXHAUSTIVE",
            Severity::Error,
            "non-exhaustive `<match>`: the subject's domain is not fully covered and there is no \
             `<otherwise>` (dsl §11.2)"
                .to_string(),
            m.span,
        ));
    }

    // A maybe-unset subject's `unset` case must be covered (§11.2/§9.4). This is
    // scoped to subjects whose nullability is derivable from the schema alone —
    // `run`/`user`/`app` (maybe-unset at scene entry) and `scene.choices.*` (a
    // branch may not have run). A plain `scene.*` subject's maybe-unset status is
    // path-sensitive; it is owned by `check_definite_assignment` (E-MAYBE-UNSET),
    // so emitting E-UNSET-UNCOVERED here would false-positive the written case.
    let unset_owned_here = subject
        .as_deref()
        .map(|p| p.starts_with("scene.choices.") || !p.starts_with("scene."))
        .unwrap_or(false);
    if info.maybe_unset && unset_owned_here && !covers_unset {
        diags.push(diag(
            "E-UNSET-UNCOVERED",
            Severity::Error,
            "maybe-unset `<match>` subject: the `unset` case is not covered by an `unset` arm or \
             an `<otherwise>` (dsl §11.2)"
                .to_string(),
            m.span,
        ));
    }

    diags
}

/// Record a `<branch>` (dsl §11.1): flag a duplicate id within the episode
/// (`E-DUP-BRANCH`) and return the implicit `scene.choices.<id>` declaration.
/// `seen` is the caller-owned, document-order set of branch ids seen so far (see
/// the module docs for why dup-detection is threaded rather than held in `Ctx`).
pub fn check_branch(branch: &Branch, seen: &mut BTreeSet<String>) -> BranchRecord {
    let path = format!("scene.choices.{}", branch.id);
    let mut diags = Vec::new();
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
        // E-CHOICE-ID-RESERVED (dsl §11.1): `unset` is the implicit choice-slot
        // DEFAULT SENTINEL — the `scene.choices.<id>` domain is the choice ids
        // ∪ `unset`, and the runtime seeds the slot `default: "unset"` before
        // any choice is taken. A `<choice id="unset">` collides with that
        // sentinel (an ambiguous selected value + a duplicate domain member), so
        // reject it here, at the offending choice's span.
        if choice.id == "unset" {
            diags.push(diag(
                "E-CHOICE-ID-RESERVED",
                Severity::Error,
                format!(
                    "`<choice id=\"unset\">` within `<branch id=\"{}\">`; `unset` is reserved as \
                     the implicit choice-slot default sentinel and may not be a choice id \
                     (dsl §11.1)",
                    branch.id
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
    };
    BranchRecord { path, decl, diags }
}

/// Record a `<hub>` (dsl §7.3.2, §11.1.3), mirroring [`check_branch`]. Emits:
/// `E-DUP-BRANCH` if the hub id collides in the shared per-episode `seen` set
/// (hub and branch ids record under one `scene.choices.*` domain); `E-CHOICE-DUP`
/// on a repeated choice id (and `E-CHOICE-ID-RESERVED` on `id="unset"`, the
/// implicit slot sentinel); `E-HUB-NO-EXIT` unless the hub has an UNGUARDED
/// `exit` choice OR every choice is `once`. Returns the implicit recording decls:
/// `scene.choices.<hubId>` (enum of choice ids ∪ `unset`, like a branch) plus a
/// per-choice `scene.visited.<hubId>.<choiceId>: bool` (default `false`, §9.6).
/// The `once`/`exit` flags arrive as bare (`BoolTrue`) attrs on each choice.
pub fn check_hub(hub: &Hub, seen: &mut BTreeSet<String>) -> HubRecord {
    let id = attr_str(&hub.attrs, "id").unwrap_or("");
    let mut diags = Vec::new();

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

    // E-CHOICE-DUP / E-CHOICE-ID-RESERVED (§11.1.3, reusing §11.1): each choice id
    // MUST be unique WITHIN the hub (it keys the recorded value + the option-label
    // lineId, §12), and `unset` is reserved as the `scene.choices.<hubId>` default
    // sentinel. One diagnostic per offending choice, at its span.
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
        if choice.id == "unset" {
            diags.push(diag(
                "E-CHOICE-ID-RESERVED",
                Severity::Error,
                format!(
                    "`<choice id=\"unset\">` within `<hub id=\"{id}\">`; `unset` is reserved as \
                     the implicit choice-slot default sentinel and may not be a choice id \
                     (dsl §11.1.3)"
                ),
                choice.span,
            ));
        }
    }

    // E-HUB-NO-EXIT (§7.3.2, §11.1.3, D-C): a hub can terminate iff it has at
    // least one UNGUARDED (`when`-less) `exit` choice, OR every choice is `once`
    // (the eligible set provably empties → auto-exit). An empty hub is neither.
    let has_unguarded_exit = hub
        .choices
        .iter()
        .any(|c| c.when.is_none() && has_bool_attr(&c.attrs, "exit"));
    let all_once =
        !hub.choices.is_empty() && hub.choices.iter().all(|c| has_bool_attr(&c.attrs, "once"));
    if !has_unguarded_exit && !all_once {
        diags.push(diag(
            E_HUB_NO_EXIT,
            Severity::Error,
            format!(
                "`<hub id=\"{id}\">` can never exit; it needs at least one unguarded \
                 (`when`-less) `exit` choice, or every choice must be `once` so the eligible \
                 set provably empties (dsl §7.3.2, §11.1.3)"
            ),
            hub.span,
        ));
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
        },
    ));
    for choice in &hub.choices {
        decls.push((
            format!("scene.visited.{id}.{}", choice.id),
            StateDecl {
                ty: Type::Bool,
                default: Some(Literal::Bool(false)),
                namespace: Namespace::Scene,
            },
        ));
    }

    HubRecord { decls, diags }
}

/// The plain string value of the attr keyed `key`, if present and a string
/// literal (`key="s"`). A bare/`@ref` value or a missing key yields `None`.
fn attr_str<'a>(attrs: &'a [Attr], key: &str) -> Option<&'a str> {
    attrs.iter().find(|a| a.key == key).and_then(|a| match &a.value {
        AttrValue::Str(s) => Some(s.as_str()),
        _ => None,
    })
}

/// True when a bare boolean flag attr (`key`, e.g. `once`/`exit`) is present —
/// parsed as [`AttrValue::BoolTrue`] (dsl §7.3.2 hub-choice flags).
fn has_bool_attr(attrs: &[Attr], key: &str) -> bool {
    attrs
        .iter()
        .any(|a| a.key == key && matches!(a.value, AttrValue::BoolTrue))
}

/// dsl §12: every content `:line`'s `lineId` (`{prefix}.{speaker}_{code}`) and
/// `voiceKey` (`{speaker}-{code}`) derive from its `(speaker, trimmed code)`
/// pair (see `lute-compile`'s addressing pass). Two `:line`s for the SAME
/// speaker carrying the SAME trimmed `code` therefore compile to IDENTICAL
/// `lineId`/`voiceKey` values — corrupting the translation + voice-bank join
/// keys. Flag the SECOND (and each later) occurrence of a repeated
/// `(speaker, code)` pair with `E-DUP-LINE-CODE`, at that line's span.
///
/// Codes are compared as TRIMMED STRINGS — exactly the key the addressing pass
/// uses (`code.trim()`), so ` 0050 ` and `0050` collide but `0050` and `50` do
/// not. Only authored string codes participate; an untagged line derives its
/// code later (uniquely per speaker) and a non-literal code (`@ref`) has no
/// static value to compare, so neither can statically collide. Document order,
/// deterministic (the caller's final `(byte_start, code)` sort settles ties).
pub fn check_line_codes(doc: &Document) -> Vec<Diagnostic> {
    let mut lines: Vec<&Line> = Vec::new();
    for shot in &doc.shots {
        collect_lines(&shot.body, &mut lines);
    }
    let mut seen: BTreeSet<(&str, String)> = BTreeSet::new();
    let mut diags = Vec::new();
    for line in lines {
        let Some(code) = authored_code(line) else {
            continue;
        };
        if !seen.insert((line.speaker.as_str(), code.clone())) {
            diags.push(diag(
                "E-DUP-LINE-CODE",
                Severity::Error,
                format!(
                    "duplicate `:line` `code=\"{code}\"` for speaker `{}`; a (speaker, code) pair \
                     must be unique — its `lineId`/`voiceKey` join keys derive from it (dsl §12)",
                    line.speaker
                ),
                line.span,
            ));
        }
    }
    diags
}

/// The line's authored `code`, trimmed to the exact string the addressing pass
/// keys `lineId`/`voiceKey` on. `None` when the line has no `code`, or its
/// `code` is not a string literal (an `@ref`/bare value cannot statically
/// collide).
fn authored_code(line: &Line) -> Option<String> {
    line.attrs
        .iter()
        .find(|a| a.key == "code")
        .and_then(|a| match &a.value {
            AttrValue::Str(s) => Some(s.trim().to_string()),
            _ => None,
        })
}

/// Collect every `Node::Line` in document order, descending into branch choices'
/// and match arms' bodies (mirrors `check.rs::Walker::walk` / `tag.rs`).
fn collect_lines<'a>(nodes: &'a [Node], out: &mut Vec<&'a Line>) {
    for node in nodes {
        match node {
            Node::Line(l) => out.push(l),
            Node::Branch(b) => {
                for choice in &b.choices {
                    collect_lines(&choice.body, out);
                }
            }
            Node::Match(m) => {
                for arm in &m.arms {
                    match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => {
                            collect_lines(body, out)
                        }
                    }
                }
            }
            Node::Hub(h) => {
                for choice in &h.choices {
                    collect_lines(&choice.body, out);
                }
            }
            Node::Directive(_) | Node::Set(_) | Node::Timeline(_) | Node::Objective(_) | Node::On(_) => {}
        }
    }
}

/// Whether a `<match>` is provably exhaustive (dsl §11.2): it has an
/// `<otherwise>`, or its finite domain — including the `unset` member when the
/// subject is maybe-unset — is fully covered by the `<when>` arms. Exposed for
/// T4.4 (definite-assignment) so a domain-exhaustive match without `<otherwise>`
/// is not treated as a possible fall-through (its arms' join is an intersection,
/// not the pre-block set). See the report's "exhaustiveness result shape".
pub fn is_exhaustive(m: &Match, schema: &StateSchema) -> bool {
    if m.arms.iter().any(|a| matches!(a, Arm::Otherwise { .. })) {
        return true;
    }
    let subject = subject_path(m);
    let info = infer_domain(subject.as_deref(), schema);
    let mut covered: BTreeSet<DomainValue> = BTreeSet::new();
    let mut covers_unset = false;
    for arm in &m.arms {
        if let Arm::When { is, test, .. } = arm {
            let cov = arm_coverage(is.as_ref(), &test.raw, subject.as_deref());
            for v in cov.values {
                covered.insert(v);
            }
            covers_unset |= cov.covers_unset;
        }
    }
    let domain_covered = match &info.domain {
        Domain::Finite(vals) => vals.iter().all(|v| covered.contains(v)),
        Domain::Infinite => false,
    };
    domain_covered && (!info.maybe_unset || covers_unset)
}

/// The inferred domain of a subject plus whether the subject is maybe-unset.
struct DomainInfo {
    domain: Domain,
    maybe_unset: bool,
}

/// Infer the subject's value domain (dsl §11.2). A `bool`/`enum` decl or a
/// `scene.choices.<id>` path is FINITE; anything else is INFINITE (requires
/// `<otherwise>`). Maybe-unset: a `scene.choices.*` subject always (a branch may
/// not have been reached), or a `run.*`/`user.*`/`app.*` decl with no `default`.
fn infer_domain(subject: Option<&str>, schema: &StateSchema) -> DomainInfo {
    let Some(path) = subject else {
        return DomainInfo {
            domain: Domain::Infinite,
            maybe_unset: false,
        };
    };
    // `scene.choices.<branchId>`: domain = branch choice ids ∪ `unset` (§11.1).
    if path.strip_prefix("scene.choices.").is_some() {
        let domain = match enum_members(path, schema) {
            Some(vals) => Domain::Finite(vals),
            // Members unknown (branch decl not folded in yet) => can't prove
            // coverage; treat as infinite so `<otherwise>` is required.
            None => Domain::Infinite,
        };
        return DomainInfo {
            domain,
            maybe_unset: true,
        };
    }
    match schema.decls.get(path) {
        Some(decl) => {
            let domain = match &decl.ty {
                Type::Bool => {
                    Domain::Finite(vec![DomainValue::Bool(true), DomainValue::Bool(false)])
                }
                Type::Enum(members) => Domain::Finite(
                    members
                        .iter()
                        .map(|m| DomainValue::Str(m.clone()))
                        .collect(),
                ),
                _ => Domain::Infinite,
            };
            let maybe_unset = decl.default.is_none()
                && matches!(
                    decl.namespace,
                    Namespace::Scene
                        | Namespace::Run
                        | Namespace::User
                        | Namespace::App
                        | Namespace::Quest
                );
            DomainInfo {
                domain,
                maybe_unset,
            }
        }
        None => DomainInfo {
            domain: Domain::Infinite,
            maybe_unset: false,
        },
    }
}

/// The enum members declared at `path`, if the decl is a `Type::Enum`.
fn enum_members(path: &str, schema: &StateSchema) -> Option<Vec<DomainValue>> {
    match &schema.decls.get(path)?.ty {
        Type::Enum(members) => Some(
            members
                .iter()
                .map(|m| DomainValue::Str(m.clone()))
                .collect(),
        ),
        _ => None,
    }
}

/// What a single `<when>` arm provably matches.
#[derive(Default)]
struct ArmCoverage {
    /// Concrete finite-domain values (`bool`/enum-string) the arm covers.
    values: Vec<DomainValue>,
    /// Whether the arm covers the `unset` case.
    covers_unset: bool,
}

/// Analyze a `<when test>` and extract the finite-domain values it provably
/// matches. Kept CONSERVATIVE (only forms we can prove): `$ == <lit>` /
/// `<lit> == $`, bare `$` (bool true) and `!$` (bool false), `$ in [<lit>,…]`,
/// `$ == null` and `!isSet($)`/`!has(p)` (`unset`). Anything else (a `@ref`
/// guard, a relational test) yields no coverage — soundly leaving the domain
/// under-covered rather than falsely claiming exhaustiveness.
fn analyze_arm(raw: &str, subject: Option<&str>) -> ArmCoverage {
    let mut cov = ArmCoverage::default();
    if let Some(expr) = parse_expr(raw) {
        analyze_expr(&expr, subject, &mut cov);
    }
    cov
}

/// The full coverage of a `<when>` arm: the union of its `is` literal pattern
/// (the NORMATIVE path, dsl §11.2) and any coverage [`analyze_arm`] derives from
/// its `test` guard (the conservative MAY path). Both [`check_match`] and
/// [`is_exhaustive`] fold coverage through here so they stay consistent.
fn arm_coverage(is: Option<&IsPattern>, test_raw: &str, subject: Option<&str>) -> ArmCoverage {
    let mut cov = analyze_arm(test_raw, subject);
    if let Some(pat) = is {
        analyze_is_pattern(&pat.raw, &mut cov);
    }
    cov
}

/// Parse a `<when is="…">` literal pattern (dsl §7.3.1):
/// `WhenPattern ::= Literal ("|" Literal)*`, `Literal = EnumMember | "true" |
/// "false" | Number | "unset"`. This is NOT CEL — split on `|`, trim each
/// literal, and fold it into `cov`: `true`/`false` are bool domain values,
/// `unset` covers the unset case (§9.4), a decimal `Number` is a numeric value,
/// and any other ident is an enum member matched by string equality on the
/// subject (§8.2). Empty alternatives (a stray `|`) are skipped.
fn analyze_is_pattern(raw: &str, cov: &mut ArmCoverage) {
    for lit in raw.split('|') {
        let lit = lit.trim();
        if lit.is_empty() {
            continue;
        }
        match lit {
            "true" => cov.values.push(DomainValue::Bool(true)),
            "false" => cov.values.push(DomainValue::Bool(false)),
            "unset" => cov.covers_unset = true,
            _ if is_number_literal(lit) => cov.values.push(DomainValue::Num(lit.to_string())),
            _ => cov.values.push(DomainValue::Str(lit.to_string())),
        }
    }
}

/// True when `lit` is a decimal `Number` literal (dsl §7.3.1) rather than an
/// enum-member ident. Enum members are `Ident`s (letter/`_` lead, MAY contain
/// `-`), so a leading digit / sign / dot plus a successful `f64` parse cleanly
/// disambiguates a number from a member name.
fn is_number_literal(lit: &str) -> bool {
    let head = lit.strip_prefix(['+', '-']).unwrap_or(lit);
    matches!(head.bytes().next(), Some(b'0'..=b'9' | b'.')) && lit.parse::<f64>().is_ok()
}

fn analyze_expr(expr: &Expr, subject: Option<&str>, cov: &mut ArmCoverage) {
    match expr {
        // `$ == <lit>` / `<lit> == $`.
        Expr::Call(c) if c.target.is_none() && c.func_name == "_==_" && c.args.len() == 2 => {
            let (a, b) = (&c.args[0].expr, &c.args[1].expr);
            if is_subject(a, subject) {
                push_literal(b, cov);
            } else if is_subject(b, subject) {
                push_literal(a, cov);
            }
        }
        // `$ in [<lit>, <lit>, …]`.
        Expr::Call(c)
            if c.target.is_none()
                && c.func_name == "@in"
                && c.args.len() == 2
                && is_subject(&c.args[0].expr, subject) =>
        {
            if let Expr::List(list) = &c.args[1].expr {
                for el in &list.elements {
                    push_literal(&el.expr, cov);
                }
            }
        }
        // `!$` (bool false) or `!isSet($)` / `!has(p)` (unset).
        Expr::Call(c) if c.target.is_none() && c.func_name == "!_" && c.args.len() == 1 => {
            let inner = &c.args[0].expr;
            if is_subject(inner, subject) {
                cov.values.push(DomainValue::Bool(false));
            } else if is_unset_test(inner, subject) {
                cov.covers_unset = true;
            }
        }
        // Bare `$` used as a boolean condition (bool true).
        _ if is_subject(expr, subject) => cov.values.push(DomainValue::Bool(true)),
        _ => {}
    }
}

/// Push a scalar literal onto the coverage: a string / bool is a finite-domain
/// value; `null` covers `unset`; numbers/bytes match no `bool`/`enum` member.
fn push_literal(expr: &Expr, cov: &mut ArmCoverage) {
    if let Expr::Literal(v) = expr {
        match v {
            Val::String(s) => cov.values.push(DomainValue::Str(s.clone())),
            Val::Boolean(b) => cov.values.push(DomainValue::Bool(*b)),
            Val::Null => cov.covers_unset = true,
            _ => {}
        }
    }
}

/// True when `expr` is the match subject: the substituted `$` (`Ident("_")`) or a
/// dotted chain equal to the subject path.
fn is_subject(expr: &Expr, subject: Option<&str>) -> bool {
    if let Expr::Ident(name) = expr {
        if name == "_" {
            return true;
        }
    }
    match (crate::cel_paths::select_path(expr), subject) {
        (Some(p), Some(s)) => p == s,
        _ => false,
    }
}

/// True when `expr` is a presence test of the subject (`isSet($)` or `has(p)`) —
/// negating it (in `analyze_expr`) is what covers the `unset` case.
fn is_unset_test(expr: &Expr, subject: Option<&str>) -> bool {
    match expr {
        // `has(p)` expands to a test-only Select of the subject path.
        Expr::Select(sel) if sel.test => crate::cel_paths::select_path(expr).as_deref() == subject,
        // `isSet($)` — a DSL global with the subject as its sole argument.
        Expr::Call(c)
            if c.target.is_none()
                && c.func_name.eq_ignore_ascii_case("isSet")
                && c.args.len() == 1 =>
        {
            is_subject(&c.args[0].expr, subject)
        }
        _ => false,
    }
}

/// Reconstruct the subject's dotted path (`run.rank`, `scene.choices.x`). Returns
/// `None` for a non-path subject (`isSet(run.x)`, an empty/missing `on=`) — an
/// unresolved subject is treated as an infinite domain.
fn subject_path(m: &Match) -> Option<String> {
    let expr = parse_expr(&m.subject.raw)?;
    crate::cel_paths::select_path(&expr)
}

/// Throwaway re-parse of a raw CEL fragment into its root [`Expr`]. Per the
/// cel-parser 0.10.1 carry-forward (T3.1) the AST is structure-only, so a fresh
/// parse yields identical structure; malformed CEL (already reported in Phase 3)
/// yields `None`.
fn parse_expr(raw: &str) -> Option<Expr> {
    if raw.trim().is_empty() {
        return None;
    }
    let mut arena = CelArena::default();
    match lute_cel::parse_slot(&mut arena, raw, 0) {
        Ok(handle) => arena.get(handle).map(|root| root.expr.clone()),
        Err(_) => None,
    }
}

/// Build a `Layer::Logic` diagnostic (a §9/§11 logic check).
fn diag(code: &str, severity: Severity, message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity,
        message,
        span,
        layer: Layer::Logic,
        fixits: Vec::new(),
        provenance: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ctx::Env;
    use lute_core_span::StableId;
    use lute_syntax::ast::{CelKind, CelSlot};
    use std::collections::BTreeMap;
    use std::sync::LazyLock;

    fn span() -> Span {
        Span {
            byte_start: 0,
            byte_end: 0,
            line: 1,
            column: 1,
            utf16_range: (0, 0),
        }
    }

    fn subject_slot(raw: &str) -> CelSlot {
        CelSlot {
            kind: CelKind::MatchSubject,
            raw: raw.into(),
            ast: None,
            span: span(),
            id: StableId(0),
        }
    }

    fn when_arm(test: &str) -> Arm {
        Arm::When {
            is: None,
            test: CelSlot {
                kind: CelKind::Condition,
                raw: test.into(),
                ast: None,
                span: span(),
                id: StableId(0),
            },
            body: Vec::new(),
            span: span(),
        }
    }

    /// A `<match on="run.rank">` over an enum subject: one `<when test="$ ==
    /// '<v>'">` per covered value, plus an optional `<otherwise>`.
    fn match_on_enum(_domain: &[&str], covered_arms: &[&str], has_otherwise: bool) -> Match {
        let mut arms: Vec<Arm> = covered_arms
            .iter()
            .map(|v| when_arm(&format!("$ == '{v}'")))
            .collect();
        if has_otherwise {
            arms.push(Arm::Otherwise {
                body: Vec::new(),
                span: span(),
            });
        }
        Match {
            subject: subject_slot("run.rank"),
            arms,
            span: span(),
        }
    }

    /// `run.rank` declared as an enum WITH a default => finite, never unset.
    fn schema_enum_subject() -> StateSchema {
        let mut decls = BTreeMap::new();
        decls.insert(
            "run.rank".to_string(),
            StateDecl {
                ty: Type::Enum(vec!["fail".into(), "gold".into()]),
                default: Some(lute_manifest::types::Literal::Str("fail".into())),
                namespace: Namespace::Run,
            },
        );
        StateSchema { decls }
    }

    /// `run.rank` declared as an enum WITHOUT a default => finite but maybe-unset.
    fn schema_maybe_unset_subject() -> StateSchema {
        let mut decls = BTreeMap::new();
        decls.insert(
            "run.rank".to_string(),
            StateDecl {
                ty: Type::Enum(vec!["fail".into(), "gold".into()]),
                default: None,
                namespace: Namespace::Run,
            },
        );
        StateSchema { decls }
    }

    fn ctx() -> Ctx<'static> {
        static ENV: LazyLock<Env> = LazyLock::new(Env::default);
        Ctx {
            env: &ENV,
            in_match: false,
            match_subject: None,
        }
    }

    #[test]
    fn enum_domain_without_otherwise_and_missing_arm_errors() {
        // subject domain {fail,gold}; arms cover only gold; no otherwise
        let m = match_on_enum(&["fail", "gold"], &["gold"], false);
        let errs = check_match(&m, &schema_enum_subject(), &ctx());
        assert!(errs.iter().any(|e| e.code == "E-NONEXHAUSTIVE"));
    }

    #[test]
    fn full_coverage_no_error() {
        let m = match_on_enum(&["fail", "gold"], &["fail", "gold"], false);
        let errs = check_match(&m, &schema_enum_subject(), &ctx());
        assert!(!errs.iter().any(|e| e.code == "E-NONEXHAUSTIVE"));
    }

    #[test]
    fn maybe_unset_subject_needs_unset_or_otherwise() {
        let m = match_on_enum(&["fail", "gold"], &["fail", "gold"], false); // no unset arm/otherwise
        let errs = check_match(&m, &schema_maybe_unset_subject(), &ctx());
        assert!(errs.iter().any(|e| e.code == "E-UNSET-UNCOVERED"));
    }

    // ---- helpers for the remaining behaviors --------------------------------

    fn match_with(subject: &str, arms: Vec<Arm>) -> Match {
        Match {
            subject: subject_slot(subject),
            arms,
            span: span(),
        }
    }

    fn schema_bool(path: &str, default: Option<bool>) -> StateSchema {
        let mut decls = BTreeMap::new();
        decls.insert(
            path.to_string(),
            StateDecl {
                ty: Type::Bool,
                default: default.map(lute_manifest::types::Literal::Bool),
                namespace: crate::meta::namespace_of(path).unwrap_or(Namespace::Run),
            },
        );
        StateSchema { decls }
    }

    fn branch(id: &str, choice_ids: &[&str]) -> Branch {
        use lute_syntax::ast::Choice;
        let choices = choice_ids
            .iter()
            .map(|c| Choice {
                id: (*c).to_string(),
                label: String::new(),
                when: None,
                attrs: Vec::new(),
                body: Vec::new(),
                span: span(),
            })
            .collect();
        Branch {
            id: id.to_string(),
            attrs: Vec::new(),
            choices,
            span: span(),
        }
    }

    // ---- E-NONEXHAUSTIVE / otherwise ----------------------------------------

    #[test]
    fn otherwise_makes_infinite_domain_exhaustive() {
        // number subject (infinite) is fine as long as `<otherwise>` is present.
        let mut decls = BTreeMap::new();
        decls.insert(
            "run.n".to_string(),
            StateDecl {
                ty: Type::Number,
                default: None,
                namespace: Namespace::Run,
            },
        );
        let schema = StateSchema { decls };
        let m = match_with(
            "run.n",
            vec![
                when_arm("$ == 1"),
                Arm::Otherwise {
                    body: Vec::new(),
                    span: span(),
                },
            ],
        );
        assert!(check_match(&m, &schema, &ctx()).is_empty());
    }

    #[test]
    fn infinite_domain_without_otherwise_is_nonexhaustive() {
        let mut decls = BTreeMap::new();
        decls.insert(
            "run.n".to_string(),
            StateDecl {
                ty: Type::Number,
                default: Some(lute_manifest::types::Literal::Num(0.0)),
                namespace: Namespace::Run,
            },
        );
        let schema = StateSchema { decls };
        let m = match_with("run.n", vec![when_arm("$ == 1"), when_arm("$ == 2")]);
        let errs = check_match(&m, &schema, &ctx());
        assert!(errs.iter().any(|e| e.code == "E-NONEXHAUSTIVE"));
    }

    // ---- bool domain --------------------------------------------------------

    #[test]
    fn bool_full_coverage_true_false_no_error() {
        // `<when test="$">` (true) + `<when test="!$">` (false) covers a bool.
        let m = match_with("scene.sealed", vec![when_arm("$"), when_arm("!$")]);
        let errs = check_match(&m, &schema_bool("scene.sealed", None), &ctx());
        assert!(errs.is_empty(), "bool fully covered, got {errs:?}");
    }

    #[test]
    fn bool_missing_false_is_nonexhaustive() {
        let m = match_with("scene.sealed", vec![when_arm("$")]);
        let errs = check_match(&m, &schema_bool("scene.sealed", None), &ctx());
        assert!(errs.iter().any(|e| e.code == "E-NONEXHAUSTIVE"));
    }

    // ---- E-UNSET-UNCOVERED coverage forms -----------------------------------

    #[test]
    fn unset_covered_by_null_arm_no_error() {
        // full enum coverage + `$ == null` arm covers the maybe-unset case.
        let m = match_with(
            "run.rank",
            vec![
                when_arm("$ == 'fail'"),
                when_arm("$ == 'gold'"),
                when_arm("$ == null"),
            ],
        );
        let errs = check_match(&m, &schema_maybe_unset_subject(), &ctx());
        assert!(
            !errs.iter().any(|e| e.code == "E-UNSET-UNCOVERED"),
            "got {errs:?}"
        );
    }

    #[test]
    fn unset_covered_by_isset_negation_no_error() {
        let m = match_with(
            "run.rank",
            vec![
                when_arm("$ == 'fail'"),
                when_arm("$ == 'gold'"),
                when_arm("!isSet($)"),
            ],
        );
        let errs = check_match(&m, &schema_maybe_unset_subject(), &ctx());
        assert!(
            !errs.iter().any(|e| e.code == "E-UNSET-UNCOVERED"),
            "got {errs:?}"
        );
    }

    #[test]
    fn defaulted_enum_full_coverage_is_not_unset_uncovered() {
        // WITH default => not maybe-unset => no E-UNSET-UNCOVERED even without an
        // unset arm. Also no E-NONEXHAUSTIVE (T4.4-interaction: this match IS
        // domain-exhaustive; T4.4 consumes `is_exhaustive` to drop its false +).
        let m = match_on_enum(&["fail", "gold"], &["fail", "gold"], false);
        let errs = check_match(&m, &schema_enum_subject(), &ctx());
        assert!(
            errs.is_empty(),
            "defaulted full-coverage enum should be clean, got {errs:?}"
        );
    }

    // ---- age-gate (§11.2) ---------------------------------------------------

    #[test]
    fn age_gate_without_teen_or_otherwise_errors() {
        let mut decls = BTreeMap::new();
        decls.insert(
            "app.rating".to_string(),
            StateDecl {
                ty: Type::Enum(vec!["everyone".into(), "teen".into(), "mature".into()]),
                default: Some(lute_manifest::types::Literal::Str("everyone".into())),
                namespace: Namespace::App,
            },
        );
        let schema = StateSchema { decls };
        let m = match_with(
            "app.rating",
            vec![when_arm("$ == 'everyone'"), when_arm("$ == 'mature'")],
        );
        let errs = check_match(&m, &schema, &ctx());
        assert!(errs.iter().any(|e| e.code == "E-AGE-GATE"), "got {errs:?}");
    }

    #[test]
    fn age_gate_with_teen_arm_ok() {
        let mut decls = BTreeMap::new();
        decls.insert(
            "app.rating".to_string(),
            StateDecl {
                ty: Type::Enum(vec!["everyone".into(), "teen".into(), "mature".into()]),
                default: Some(lute_manifest::types::Literal::Str("everyone".into())),
                namespace: Namespace::App,
            },
        );
        let schema = StateSchema { decls };
        let m = match_with(
            "app.rating",
            vec![
                when_arm("$ == 'everyone'"),
                when_arm("$ == 'teen'"),
                when_arm("$ == 'mature'"),
            ],
        );
        let errs = check_match(&m, &schema, &ctx());
        assert!(!errs.iter().any(|e| e.code == "E-AGE-GATE"), "got {errs:?}");
    }

    // ---- W-OVERLAP-ARMS (conservative) --------------------------------------

    #[test]
    fn duplicate_literal_arms_warn_overlap() {
        let m = match_on_enum(&["fail", "gold"], &["gold", "gold", "fail"], false);
        let warns = check_match(&m, &schema_enum_subject(), &ctx());
        let overlaps: Vec<_> = warns
            .iter()
            .filter(|e| e.code == "W-OVERLAP-ARMS")
            .collect();
        assert_eq!(
            overlaps.len(),
            1,
            "exactly the duplicate `gold` arm warns, got {warns:?}"
        );
        assert_eq!(overlaps[0].severity, Severity::Warning);
    }

    #[test]
    fn distinct_literal_arms_do_not_warn() {
        let m = match_on_enum(&["fail", "gold"], &["fail", "gold"], false);
        let warns = check_match(&m, &schema_enum_subject(), &ctx());
        assert!(
            !warns.iter().any(|e| e.code == "W-OVERLAP-ARMS"),
            "got {warns:?}"
        );
    }

    // ---- scene.choices.<id> domain ------------------------------------------

    #[test]
    fn scene_choices_full_coverage_still_needs_unset() {
        // domain = {help, ignore} ∪ unset; cover both choice ids but not unset.
        let mut decls = BTreeMap::new();
        decls.insert(
            "scene.choices.couch".to_string(),
            StateDecl {
                ty: Type::Enum(vec!["help".into(), "ignore".into()]),
                default: None,
                namespace: Namespace::Scene,
            },
        );
        let schema = StateSchema { decls };
        let m = match_with(
            "scene.choices.couch",
            vec![when_arm("$ == 'help'"), when_arm("$ == 'ignore'")],
        );
        let errs = check_match(&m, &schema, &ctx());
        assert!(
            errs.iter().any(|e| e.code == "E-UNSET-UNCOVERED"),
            "got {errs:?}"
        );
        assert!(
            !errs.iter().any(|e| e.code == "E-NONEXHAUSTIVE"),
            "choice ids fully covered: {errs:?}"
        );
    }

    // ---- E-DUP-BRANCH + recording (§11.1) -----------------------------------

    #[test]
    fn branch_records_scene_choices_decl() {
        let mut seen = BTreeSet::new();
        let rec = check_branch(&branch("couch", &["help", "ignore"]), &mut seen);
        assert_eq!(rec.path, "scene.choices.couch");
        assert_eq!(rec.decl.namespace, Namespace::Scene);
        assert_eq!(
            rec.decl.ty,
            Type::Enum(vec!["help".into(), "ignore".into()])
        );
        assert!(rec.decl.default.is_none());
        assert!(rec.diags.is_empty());
    }

    #[test]
    fn duplicate_branch_id_errors_second_time() {
        let mut seen = BTreeSet::new();
        let first = check_branch(&branch("couch", &["help"]), &mut seen);
        assert!(first.diags.is_empty(), "first occurrence is clean");
        let second = check_branch(&branch("couch", &["help"]), &mut seen);
        assert!(
            second.diags.iter().any(|e| e.code == "E-DUP-BRANCH"),
            "got {:?}",
            second.diags
        );
    }

    // ---- is_exhaustive (T4.4 consumer) --------------------------------------

    #[test]
    fn is_exhaustive_true_for_full_finite_coverage() {
        let m = match_on_enum(&["fail", "gold"], &["fail", "gold"], false);
        assert!(is_exhaustive(&m, &schema_enum_subject()));
    }

    #[test]
    fn is_exhaustive_false_for_missing_unset() {
        let m = match_on_enum(&["fail", "gold"], &["fail", "gold"], false);
        assert!(!is_exhaustive(&m, &schema_maybe_unset_subject()));
    }

    #[test]
    fn is_exhaustive_true_with_otherwise() {
        let m = match_on_enum(&["fail", "gold"], &["gold"], true);
        assert!(is_exhaustive(&m, &schema_maybe_unset_subject()));
    }

    // ---- E-CHOICE-DUP (dsl §11.1) -------------------------------------------

    #[test]
    fn duplicate_choice_ids_flag_e_choice_dup() {
        use lute_syntax::ast::Choice;
        let sp = Span {
            byte_start: 0,
            byte_end: 0,
            line: 1,
            column: 1,
            utf16_range: (0, 0),
        };
        let choice = |id: &str| Choice {
            id: id.into(),
            label: id.into(),
            when: None,
            attrs: Vec::new(),
            body: Vec::new(),
            span: sp,
        };
        let branch = Branch {
            id: "number".into(),
            attrs: Vec::new(),
            choices: vec![choice("blunt"), choice("soft"), choice("blunt")],
            span: sp,
        };
        let mut seen = BTreeSet::new();
        let rec = check_branch(&branch, &mut seen);
        let dups: Vec<_> = rec
            .diags
            .iter()
            .filter(|d| d.code == "E-CHOICE-DUP")
            .collect();
        assert_eq!(
            dups.len(),
            1,
            "exactly one E-CHOICE-DUP for the one repeat id"
        );
        assert_eq!(dups[0].severity, Severity::Error);
        assert!(dups[0].message.contains("blunt"), "{}", dups[0].message);

        // Unique ids stay clean.
        let ok = Branch {
            id: "other".into(),
            attrs: Vec::new(),
            choices: vec![choice("a"), choice("b")],
            span: sp,
        };
        let rec = check_branch(&ok, &mut seen);
        assert!(rec.diags.iter().all(|d| d.code != "E-CHOICE-DUP"));
    }

    // ---- E-BRANCH-EMPTY (dsl §7.3 `Choice+`) --------------------------------

    #[test]
    fn empty_branch_flags_e_branch_empty() {
        let mut seen = BTreeSet::new();
        let rec = check_branch(&branch("dead", &[]), &mut seen);
        let empties: Vec<_> = rec
            .diags
            .iter()
            .filter(|d| d.code == "E-BRANCH-EMPTY")
            .collect();
        assert_eq!(empties.len(), 1, "one E-BRANCH-EMPTY, got {:?}", rec.diags);
        assert_eq!(empties[0].severity, Severity::Error);
        assert_eq!(empties[0].layer, Layer::Logic);
        assert!(
            empties[0].message.contains("dead"),
            "{}",
            empties[0].message
        );
    }

    #[test]
    fn well_formed_branch_has_no_empty_diag() {
        let mut seen = BTreeSet::new();
        let rec = check_branch(&branch("couch", &["help", "ignore"]), &mut seen);
        assert!(rec.diags.iter().all(|d| d.code != "E-BRANCH-EMPTY"));
    }

    // ---- E-BRANCH-ALL-GUARDED (dsl §11.1, S5) -------------------------------

    /// Build a branch whose choices each carry an optional `when` guard (raw CEL
    /// text; `None` = unguarded). Only `when.is_some()` matters to `check_branch`.
    fn guarded_branch(id: &str, choices: &[(&str, Option<&str>)]) -> Branch {
        use lute_syntax::ast::Choice;
        let choices = choices
            .iter()
            .map(|(cid, guard)| Choice {
                id: (*cid).to_string(),
                label: String::new(),
                when: guard.map(|g| CelSlot::raw(CelKind::Condition, g.into(), span())),
                attrs: Vec::new(),
                body: Vec::new(),
                span: span(),
            })
            .collect();
        Branch {
            id: id.to_string(),
            attrs: Vec::new(),
            choices,
            span: span(),
        }
    }

    #[test]
    fn branch_all_guarded_rejected() {
        // Every `<choice>` carries a `when` → the eligible set can be empty, so
        // the branch could present an empty menu: one E-BRANCH-ALL-GUARDED at the
        // branch span.
        let mut seen = BTreeSet::new();
        let b = guarded_branch(
            "approach",
            &[("soft", Some("scene.x")), ("blunt", Some("scene.y"))],
        );
        let rec = check_branch(&b, &mut seen);
        let guarded: Vec<_> = rec
            .diags
            .iter()
            .filter(|d| d.code == E_BRANCH_ALL_GUARDED)
            .collect();
        assert_eq!(
            guarded.len(),
            1,
            "one E-BRANCH-ALL-GUARDED, got {:?}",
            rec.diags
        );
        assert_eq!(guarded[0].severity, Severity::Error);
        assert!(
            guarded[0].message.contains("approach"),
            "{}",
            guarded[0].message
        );
    }

    #[test]
    fn branch_one_unguarded_ok() {
        // At least one `when`-less choice → the menu is never provably empty.
        let mut seen = BTreeSet::new();
        let b = guarded_branch("approach", &[("soft", Some("scene.x")), ("blunt", None)]);
        let rec = check_branch(&b, &mut seen);
        assert!(
            rec.diags.iter().all(|d| d.code != E_BRANCH_ALL_GUARDED),
            "got {:?}",
            rec.diags
        );
    }

    #[test]
    fn empty_branch_is_not_all_guarded() {
        // An empty branch is E-BRANCH-EMPTY, NOT also E-BRANCH-ALL-GUARDED
        // (all-guarded applies only to a non-empty branch).
        let mut seen = BTreeSet::new();
        let rec = check_branch(&branch("dead", &[]), &mut seen);
        assert!(
            rec.diags.iter().any(|d| d.code == "E-BRANCH-EMPTY"),
            "got {:?}",
            rec.diags
        );
        assert!(
            rec.diags.iter().all(|d| d.code != E_BRANCH_ALL_GUARDED),
            "empty branch must not be double-flagged; got {:?}",
            rec.diags
        );
    }

    // ---- E-MATCH-DUP-OTHERWISE (dsl §11.2 at-most-one) ----------------------

    #[test]
    fn two_otherwise_flag_e_match_dup_otherwise() {
        let second_sp = Span {
            byte_start: 42,
            byte_end: 50,
            line: 3,
            column: 1,
            utf16_range: (42, 50),
        };
        let m = match_with(
            "run.rank",
            vec![
                Arm::Otherwise {
                    body: Vec::new(),
                    span: span(),
                },
                Arm::Otherwise {
                    body: Vec::new(),
                    span: second_sp,
                },
            ],
        );
        let errs = check_match(&m, &schema_enum_subject(), &ctx());
        let dups: Vec<_> = errs
            .iter()
            .filter(|d| d.code == "E-MATCH-DUP-OTHERWISE")
            .collect();
        assert_eq!(
            dups.len(),
            1,
            "one dup for the second otherwise, got {errs:?}"
        );
        assert_eq!(dups[0].severity, Severity::Error);
        assert_eq!(dups[0].layer, Layer::Logic);
        assert_eq!(dups[0].span, second_sp, "flagged at the second otherwise");
    }

    #[test]
    fn single_otherwise_has_no_dup_diag() {
        let m = match_with(
            "run.rank",
            vec![
                when_arm("$ == 'gold'"),
                Arm::Otherwise {
                    body: Vec::new(),
                    span: span(),
                },
            ],
        );
        let errs = check_match(&m, &schema_enum_subject(), &ctx());
        assert!(errs.iter().all(|d| d.code != "E-MATCH-DUP-OTHERWISE"));
    }

    // ---- E-CHOICE-ID-RESERVED (dsl §11.1, reserved `unset`) -----------------

    #[test]
    fn choice_id_unset_flags_e_choice_id_reserved() {
        let mut seen = BTreeSet::new();
        let rec = check_branch(&branch("number", &["help", "unset"]), &mut seen);
        let reserved: Vec<_> = rec
            .diags
            .iter()
            .filter(|d| d.code == "E-CHOICE-ID-RESERVED")
            .collect();
        assert_eq!(
            reserved.len(),
            1,
            "one E-CHOICE-ID-RESERVED for the reserved id, got {:?}",
            rec.diags
        );
        assert_eq!(reserved[0].severity, Severity::Error);
        assert_eq!(reserved[0].layer, Layer::Logic);
        assert!(
            reserved[0].message.contains("unset"),
            "{}",
            reserved[0].message
        );
    }

    #[test]
    fn non_reserved_choice_ids_have_no_reserved_diag() {
        let mut seen = BTreeSet::new();
        let rec = check_branch(&branch("number", &["help", "ignore"]), &mut seen);
        assert!(rec.diags.iter().all(|d| d.code != "E-CHOICE-ID-RESERVED"));
    }

    // ---- E-DUP-LINE-CODE (dsl §12, unique (speaker, code)) ------------------

    fn code_line(speaker: &str, code: Option<&str>, byte: usize) -> Node {
        use lute_syntax::ast::{Attr, Line};
        let sp = Span {
            byte_start: byte,
            byte_end: byte,
            line: 1,
            column: 1,
            utf16_range: (byte as u32, byte as u32),
        };
        let attrs = match code {
            Some(c) => vec![Attr {
                key: "code".into(),
                value: AttrValue::Str(c.into()),
                value_span: sp,
                span: sp,
            }],
            None => Vec::new(),
        };
        Node::Line(Line {
            speaker: speaker.into(),
            attrs,
            text: "…".into(),
            text_span: sp,
            interps: Vec::new(),
            span: sp,
        })
    }

    fn doc_with(body: Vec<Node>) -> Document {
        use lute_syntax::ast::{Meta, Shot};
        Document {
            meta: Meta {
                raw_yaml: String::new(),
                span: span(),
            },
            title: None,
            shots: vec![Shot {
                heading: "Shot 1".into(),
                number: Some(1),
                body,
                span: span(),
            }],
            quests: Vec::new(),
            span: span(),
        }
    }

    #[test]
    fn duplicate_line_code_flags_at_second_occurrence() {
        // (bianca, 0050) twice => one E-DUP-LINE-CODE at the SECOND occurrence.
        // The (fixer, 0050) line is a different speaker (distinct lineId) and the
        // (bianca, 0060) line a distinct code — both stay clean.
        let doc = doc_with(vec![
            code_line("bianca", Some("0050"), 10),
            code_line("fixer", Some("0050"), 20),
            code_line("bianca", Some("0050"), 30),
            code_line("bianca", Some("0060"), 40),
        ]);
        let diags = check_line_codes(&doc);
        assert_eq!(
            diags.len(),
            1,
            "exactly one E-DUP-LINE-CODE for the one repeat pair, got {diags:?}"
        );
        assert_eq!(diags[0].code, "E-DUP-LINE-CODE");
        assert_eq!(diags[0].severity, Severity::Error);
        assert_eq!(diags[0].layer, Layer::Logic);
        assert_eq!(
            diags[0].span.byte_start, 30,
            "flagged at the second (bianca, 0050)"
        );
        assert!(diags[0].message.contains("0050"), "{}", diags[0].message);
        assert!(diags[0].message.contains("bianca"), "{}", diags[0].message);
    }

    #[test]
    fn distinct_codes_and_speakers_have_no_dup_line_code() {
        let doc = doc_with(vec![
            code_line("bianca", Some("0050"), 10),
            code_line("fixer", Some("0050"), 20),
            code_line("bianca", Some("0060"), 30),
            code_line("bianca", None, 40), // untagged: no static collision
        ]);
        assert!(check_line_codes(&doc).is_empty());
    }

    #[test]
    fn line_code_collision_is_trimmed_and_descends_into_arms() {
        use lute_syntax::ast::Choice;
        // ` 0050 ` and `0050` trim to the same key => collide (the addressing
        // pass keys `lineId`/`voiceKey` on the trimmed string). The colliding
        // occurrence sits inside a `<branch>` choice body, proving the walk
        // descends into nested bodies (mirroring tag.rs).
        let branch = Branch {
            id: "b".into(),
            attrs: Vec::new(),
            choices: vec![Choice {
                id: "a".into(),
                label: String::new(),
                when: None,
                attrs: Vec::new(),
                body: vec![code_line("bianca", Some("0050"), 60)],
                span: span(),
            }],
            span: span(),
        };
        let doc = doc_with(vec![
            code_line("bianca", Some(" 0050 "), 10),
            Node::Branch(branch),
        ]);
        let diags = check_line_codes(&doc);
        assert_eq!(diags.len(), 1, "trimmed codes collide, got {diags:?}");
        assert_eq!(diags[0].code, "E-DUP-LINE-CODE");
        assert_eq!(
            diags[0].span.byte_start, 60,
            "flagged at the nested second occurrence"
        );
    }
    // ---- B4: <when is> literal-pattern coverage + E-WHEN-PATTERN (§7.3.1) ----

    /// Build a `<when>` arm from an optional `is` pattern + a `test` guard raw
    /// (empty `test` = absent, mirroring the parser's empty-slot default).
    fn when_is(is_raw: Option<&str>, test_raw: &str) -> Arm {
        Arm::When {
            is: is_raw.map(|r| IsPattern {
                raw: r.trim().to_string(),
                span: span(),
            }),
            test: CelSlot {
                kind: CelKind::Condition,
                raw: test_raw.into(),
                ast: None,
                span: span(),
                id: StableId(0),
            },
            body: Vec::new(),
            span: span(),
        }
    }

    /// `run.rank` as the 4-member enum `fail|bronze|silver|gold`. With a default
    /// it is finite-and-never-unset; without, finite-but-maybe-unset.
    fn schema_rank4(with_default: bool) -> StateSchema {
        let mut decls = BTreeMap::new();
        decls.insert(
            "run.rank".to_string(),
            StateDecl {
                ty: Type::Enum(vec![
                    "fail".into(),
                    "bronze".into(),
                    "silver".into(),
                    "gold".into(),
                ]),
                default: with_default.then(|| lute_manifest::types::Literal::Str("fail".into())),
                namespace: Namespace::Run,
            },
        );
        StateSchema { decls }
    }

    #[test]
    fn is_arms_cover_enum_no_otherwise_ok() {
        // `is="fail | bronze"`, `is="silver"`, `is="gold"` covers the enum with NO
        // <otherwise> => exhaustive (`is` is the NORMATIVE coverage path, §11.2).
        let m = match_with(
            "run.rank",
            vec![
                when_is(Some("fail | bronze"), ""),
                when_is(Some("silver"), ""),
                when_is(Some("gold"), ""),
            ],
        );
        let errs = check_match(&m, &schema_rank4(true), &ctx());
        assert!(errs.is_empty(), "is arms fully cover the enum: {errs:?}");
    }

    #[test]
    fn is_arms_missing_member_nonexhaustive() {
        // omit `gold`, no <otherwise> => E-NONEXHAUSTIVE.
        let m = match_with(
            "run.rank",
            vec![
                when_is(Some("fail | bronze"), ""),
                when_is(Some("silver"), ""),
            ],
        );
        let errs = check_match(&m, &schema_rank4(true), &ctx());
        assert!(
            errs.iter().any(|e| e.code == "E-NONEXHAUSTIVE"),
            "missing `gold` arm: {errs:?}"
        );
    }

    #[test]
    fn is_unset_covers_unset() {
        // maybe-unset subject: `is="unset"` covers the unset member (§9.4/§11.2).
        let m = match_with(
            "run.rank",
            vec![
                when_is(Some("fail|bronze"), ""),
                when_is(Some("silver|gold"), ""),
                when_is(Some("unset"), ""),
            ],
        );
        let errs = check_match(&m, &schema_rank4(false), &ctx());
        assert!(
            !errs.iter().any(|e| e.code == "E-UNSET-UNCOVERED"),
            "`is=unset` covers unset: {errs:?}"
        );
        assert!(
            !errs.iter().any(|e| e.code == "E-NONEXHAUSTIVE"),
            "members fully covered: {errs:?}"
        );
    }

    #[test]
    fn when_with_neither_is_nor_test_is_e_when_pattern() {
        // a `<when>` with neither `is` nor `test` => E-WHEN-PATTERN (§7.3.1, D-D).
        let m = match_with(
            "run.rank",
            vec![
                when_is(None, ""),
                Arm::Otherwise {
                    body: Vec::new(),
                    span: span(),
                },
            ],
        );
        let errs = check_match(&m, &schema_rank4(true), &ctx());
        assert!(
            errs.iter().any(|e| e.code == E_WHEN_PATTERN),
            "empty <when> must be E-WHEN-PATTERN: {errs:?}"
        );
    }

    #[test]
    fn is_and_test_both_ok_is_drives_coverage() {
        // `is="gold" test="$ != 'x'"` parses+checks; `is` drives coverage so the
        // match stays exhaustive despite the extra guard.
        let m = match_with(
            "run.rank",
            vec![
                when_is(Some("fail|bronze"), ""),
                when_is(Some("silver"), ""),
                when_is(Some("gold"), "$ != 'x'"),
            ],
        );
        let errs = check_match(&m, &schema_rank4(true), &ctx());
        assert!(
            !errs.iter().any(|e| e.code == "E-NONEXHAUSTIVE"),
            "is drives coverage even with a guard: {errs:?}"
        );
        assert!(
            !errs.iter().any(|e| e.code == E_WHEN_PATTERN),
            "an arm carrying `is` is never E-WHEN-PATTERN: {errs:?}"
        );
    }

    #[test]
    fn is_exhaustive_consults_is_coverage() {
        // is_exhaustive (shared with defassign) MUST see `is` coverage too.
        let full = match_with(
            "run.rank",
            vec![
                when_is(Some("fail|bronze"), ""),
                when_is(Some("silver|gold"), ""),
            ],
        );
        assert!(is_exhaustive(&full, &schema_rank4(true)));
        let partial = match_with(
            "run.rank",
            vec![
                when_is(Some("fail|bronze"), ""),
                when_is(Some("silver"), ""),
            ],
        );
        assert!(!is_exhaustive(&partial, &schema_rank4(true)));
    }

    #[test]
    fn is_true_false_covers_bool() {
        // bool subject covered by `is="true"` + `is="false"`, no <otherwise>.
        let m = match_with(
            "scene.sealed",
            vec![when_is(Some("true"), ""), when_is(Some("false"), "")],
        );
        let errs = check_match(&m, &schema_bool("scene.sealed", Some(false)), &ctx());
        assert!(errs.is_empty(), "bool covered by is=true/false: {errs:?}");
    }

}
