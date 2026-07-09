//! Table-driven grammar admission (dsl 0.2.0 §3.3, §6.7): per-kind, per-context
//! construct legality.
//!
//! The parser is kind-agnostic (Plan A): it happily parses a `<hub>` inside a
//! quest doc, or an `<on>` inside a scene shot — [`check_admission`] is the
//! layer that rejects such documents. Admission is **table-driven**
//! (0.3.0-forward, deliberately): [`NodeKind`] classifies a [`Node`] by its
//! grammar terminal, and [`admits`] is an EXHAUSTIVE `match` over every
//! `(GrammarContext, NodeKind)` pair — NO wildcard on [`NodeKind`], so a future
//! `Node` variant (0.3.0's `::assert`/`::retract`) is a compiler-flagged arm at
//! every dispatch site here, not a silently-accepted default.
//!
//! ## Grammar positions (dsl 0.2.0 §6.7)
//! - **document top level** — `<quest>` only (quest docs) or `## `/`# ` shot
//!   headings only (scene docs); checked directly against `doc.shots`/
//!   `doc.quests`, not through the [`NodeKind`] table (a shot heading is not a
//!   [`Node`] at all).
//! - **[`GrammarContext::SceneBody`]** — anywhere inside a scene shot body (dsl
//!   0.1.0 grammar, unchanged): admits `Line`/`Directive`/`Set`/`Branch`/
//!   `Match`/`Hub`/`Timeline`. Recursing into a `<branch>`/`<match>`/`<hub>`
//!   child body STAYS `SceneBody` — 0.1.0 has no context split.
//! - **[`GrammarContext::QuestBody`]** — directly inside a `<quest>` body:
//!   admits `Line`/`Directive`/`Set`/`Branch`/`Match`/`On`/`Objective`. Never
//!   `Hub`/`Timeline` (forbidden everywhere in a quest doc, dsl 0.2.0 §6.7).
//! - **[`GrammarContext::Emittable`]** — inside an `<objective>`/`<on>` body, OR
//!   inside any `<match>`/`<branch>` NESTED within a quest doc (i.e. the moment
//!   you descend past a quest-body-level `<branch>`/`<match>` into its own
//!   choice/arm bodies): admits only `Line`/`Directive`/`Set`/`Branch`/`Match`.
//!   No `<objective>`/`<on>`/`<hub>`/`<timeline>` — declarations and triggers
//!   are quest-body-level only; `<match>`/`<branch>` may nest freely within
//!   `Emittable` (staying `Emittable`).

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_syntax::ast::{Arm, Document, Node};

use crate::meta::DocKind;

/// The syntactic KIND of a [`Node`] (dsl grammar terminal), independent of its
/// payload. [`node_kind`] is exhaustive (no wildcard arm) — the compiler-forced
/// canary: adding a `Node` variant is a compile error here until this enum AND
/// every [`admits`] arm are updated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    Line,
    Directive,
    Set,
    Branch,
    Match,
    Hub,
    Timeline,
    On,
    Objective,
}

/// Classify a [`Node`] by its grammar terminal (exhaustive — see [`NodeKind`]).
pub fn node_kind(node: &Node) -> NodeKind {
    match node {
        Node::Line(_) => NodeKind::Line,
        Node::Directive(_) => NodeKind::Directive,
        Node::Set(_) => NodeKind::Set,
        Node::Branch(_) => NodeKind::Branch,
        Node::Match(_) => NodeKind::Match,
        Node::Hub(_) => NodeKind::Hub,
        Node::Timeline(_) => NodeKind::Timeline,
        Node::On(_) => NodeKind::On,
        Node::Objective(_) => NodeKind::Objective,
    }
}

/// The grammar POSITION a node stream is being validated in (dsl 0.2.0 §6.7).
/// See the module docs for the exact node sets each position admits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GrammarContext {
    SceneBody,
    QuestBody,
    Emittable,
}

/// EXHAUSTIVE `(DocKind, GrammarContext, NodeKind) -> bool` admission table
/// (dsl 0.2.0 §3.3, §6.7). `doc` is not itself part of the `(ctx, nk)` match —
/// every reachable `ctx` already implies its owning `doc` kind (`SceneBody`
/// only arises walking a scene, `QuestBody`/`Emittable` only walking a quest) —
/// but it is threaded through and asserted against `ctx` in DEBUG builds so a
/// future caller wiring a new `DocKind` cannot silently produce an
/// inconsistent `(doc, ctx)` pair. The inner match is exhaustive over EVERY
/// [`NodeKind`] variant with NO wildcard (0.3.0-forward: `::assert`/`::retract`
/// slot in as one arm each, compiler-flagged everywhere until handled).
fn admits(doc: DocKind, ctx: GrammarContext, nk: NodeKind) -> bool {
    debug_assert!(
        matches!(
            (doc, ctx),
            (DocKind::Scene, GrammarContext::SceneBody)
                | (DocKind::Quest, GrammarContext::QuestBody)
                | (DocKind::Quest, GrammarContext::Emittable)
        ),
        "admits called with a (doc, ctx) pair that never occurs: {doc:?} {ctx:?}"
    );
    match ctx {
        GrammarContext::SceneBody => match nk {
            NodeKind::Line
            | NodeKind::Directive
            | NodeKind::Set
            | NodeKind::Branch
            | NodeKind::Match
            | NodeKind::Hub
            | NodeKind::Timeline => true,
            NodeKind::On | NodeKind::Objective => false,
        },
        GrammarContext::QuestBody => match nk {
            NodeKind::Line
            | NodeKind::Directive
            | NodeKind::Set
            | NodeKind::Branch
            | NodeKind::Match
            | NodeKind::On
            | NodeKind::Objective => true,
            NodeKind::Hub | NodeKind::Timeline => false,
        },
        GrammarContext::Emittable => match nk {
            NodeKind::Line | NodeKind::Directive | NodeKind::Set | NodeKind::Branch | NodeKind::Match => {
                true
            }
            NodeKind::Hub | NodeKind::Timeline | NodeKind::On | NodeKind::Objective => false,
        },
    }
}

/// Validate the whole document's grammar admission against its resolved `kind`
/// (dsl 0.2.0 §3.3, §6.7). Emits `E-GRAMMAR-NOT-ADMITTED` for:
///
/// (a) a scene doc with a non-empty `doc.quests` (a `<quest>` where scene
///     forbids it — top-level `<quest>` is parsed independently of `doc.shots`,
///     Plan A, so it needs an explicit check here);
/// (b) a quest doc with a non-empty `doc.shots` (a `## `/`# ` heading — quest
///     forbids headings everywhere, dsl 0.2.0 §6.2/§6.7);
/// (c) any [`Node`] whose [`NodeKind`] is not admitted by its `(kind, context)`
///     position, walking `doc.shots` (scene) or `doc.quests` (quest) with the
///     context transitions described in the module docs.
pub fn check_admission(doc: &Document, kind: DocKind) -> Vec<Diagnostic> {
    let mut diags = Vec::new();

    match kind {
        DocKind::Scene => {
            for quest in &doc.quests {
                diags.push(diag(
                    format!(
                        "`<quest id=\"{}\">` is not admitted at the document top level of a \
                         scene document; only `<quest>` declarations belong in a quest-kind \
                         document (dsl 0.2.0 §3.3, §6.2)",
                        quest.id
                    ),
                    quest.span,
                ));
            }
            for shot in &doc.shots {
                walk(&shot.body, DocKind::Scene, GrammarContext::SceneBody, &mut diags);
            }
        }
        DocKind::Quest => {
            for shot in &doc.shots {
                diags.push(diag(
                    format!(
                        "a `{}` heading is not admitted in a quest document; the quest kind \
                         forbids `# `/`## ` headings everywhere and admits only `<quest>` at \
                         the document top level (dsl 0.2.0 §6.2, §6.7)",
                        shot.heading
                    ),
                    shot.span,
                ));
            }
            for quest in &doc.quests {
                walk(&quest.body, DocKind::Quest, GrammarContext::QuestBody, &mut diags);
            }
        }
    }

    diags
}

/// Walk a node stream, flagging every [`Node`] not admitted at `ctx`, and
/// recursing into nested bodies with the context transition the construct
/// implies (see the module docs). Always recurses (even into a node just
/// flagged as not-admitted) so a deeper violation is never masked by an outer
/// one.
fn walk(nodes: &[Node], doc: DocKind, ctx: GrammarContext, diags: &mut Vec<Diagnostic>) {
    for node in nodes {
        let nk = node_kind(node);
        if !admits(doc, ctx, nk) {
            diags.push(diag(
                format!(
                    "{} is not admitted here (dsl 0.2.0 §3.3, §6.7): {}",
                    describe(nk),
                    context_reason(doc, ctx)
                ),
                node_span(node),
            ));
        }
        match node {
            Node::Branch(b) => {
                let child_ctx = nested_ctx(doc);
                for choice in &b.choices {
                    walk(&choice.body, doc, child_ctx, diags);
                }
            }
            Node::Match(m) => {
                let child_ctx = nested_ctx(doc);
                for arm in &m.arms {
                    match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => {
                            walk(body, doc, child_ctx, diags);
                        }
                    }
                }
            }
            Node::Hub(h) => {
                // Only legal in `SceneBody`; if present elsewhere the whole
                // construct is already flagged above. Its children never
                // transition context (dsl 0.1.0 grammar, unchanged) — safe
                // regardless of `doc` since it just re-threads the current
                // (already doc-consistent) `ctx`.
                for choice in &h.choices {
                    walk(&choice.body, doc, ctx, diags);
                }
            }
            Node::On(o) => walk(&o.body, doc, nested_ctx(doc), diags),
            Node::Objective(o) => walk(&o.body, doc, nested_ctx(doc), diags),
            // `Timeline`'s clips are `ClipNode` (`Directive`/`Set` only) — a
            // strictly narrower shape than `Node` that cannot carry an
            // inadmissible construct, so there is nothing further to walk.
            Node::Line(_) | Node::Directive(_) | Node::Set(_) | Node::Timeline(_) => {}
        }
    }
}

/// The context a `<branch>`/`<match>`/`<on>`/`<objective>` child body walks in.
/// Driven ENTIRELY by `doc` (not the incoming `ctx`): `SceneBody` for a scene
/// document — dsl 0.1.0 has no context split, so even a rogue `<on>`/
/// `<objective>` wrongly nested in a scene body still recurses as ordinary
/// scene content — and `Emittable` for a quest document (`QuestBody` or
/// already-`Emittable`, both collapse to `Emittable` once nested). This keeps
/// every `(doc, ctx)` pair [`admits`] ever sees consistent with its
/// `debug_assert`.
fn nested_ctx(doc: DocKind) -> GrammarContext {
    match doc {
        DocKind::Scene => GrammarContext::SceneBody,
        DocKind::Quest => GrammarContext::Emittable,
    }
}

fn describe(nk: NodeKind) -> &'static str {
    match nk {
        NodeKind::Line => "a content line",
        NodeKind::Directive => "a `::directive`",
        NodeKind::Set => "a `::set`",
        NodeKind::Branch => "a `<branch>`",
        NodeKind::Match => "a `<match>`",
        NodeKind::Hub => "a `<hub>`",
        NodeKind::Timeline => "a `<timeline>`",
        NodeKind::On => "an `<on>`",
        NodeKind::Objective => "an `<objective>`",
    }
}

fn context_reason(doc: DocKind, ctx: GrammarContext) -> &'static str {
    match (doc, ctx) {
        (DocKind::Scene, _) => "the scene grammar admits no `<on>`/`<objective>` (quest-only constructs)",
        (DocKind::Quest, GrammarContext::QuestBody) => {
            "a quest body forbids `<hub>`/`<timeline>` (dsl 0.2.0 §6.7)"
        }
        (DocKind::Quest, _) => {
            "an emittable body (an `<objective>`/`<on>` arm, or nested `<match>`/`<branch>`) \
             admits no `<hub>`/`<timeline>`/`<on>`/`<objective>` (dsl 0.2.0 §6.7)"
        }
    }
}

fn node_span(node: &Node) -> Span {
    match node {
        Node::Line(l) => l.span,
        Node::Directive(d) => d.span,
        Node::Set(s) => s.span,
        Node::Branch(b) => b.span,
        Node::Match(m) => m.span,
        Node::Hub(h) => h.span,
        Node::Timeline(t) => t.span,
        Node::On(o) => o.span,
        Node::Objective(o) => o.span,
    }
}

fn diag(message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: "E-GRAMMAR-NOT-ADMITTED".to_string(),
        severity: Severity::Error,
        message,
        span,
        layer: Layer::Logic,
        fixits: Vec::new(),
        provenance: None,
    }
}
