//! `check()` assembly + the LSP-facing `Resolved` view (Task 4.9).
//!
//! This is the single validation core the CLI (Phase 5) and LSP (Phase 6) both
//! wrap — "`check()` is the contract, not the LSP protocol". It wires the whole
//! pipeline together and owns NO validation logic of its own: every diagnostic
//! comes from a Phase-3/Phase-4 validator that already has its own tests.
//!
//! ```text
//! parse (syntax)                      -> Document + parse diags
//!   -> fill_document (cel)            -> CEL asts in arena + E-CEL-PARSE diags
//!   -> parse_meta                     -> StateSchema + defs + meta diags
//!   -> fold <branch> decls            -> folded schema (scene.choices.*) + E-DUP-BRANCH
//!   -> per-node walk                  -> directive/cel-slot/set/match/timeline diags
//!   -> document-level defassign       -> E-UNDECLARED / E-MAYBE-UNSET (whole stream)
//!   -> injection fold (lower_node)    -> InjectedCommand[] + W-INJECT-CONFLICT
//!   -> suppress / dedup / normalize / sort
//! ```
//!
//! ## The five binding carry-forwards (see the T4.9 brief)
//! 1. **Document-level definite-assignment.** `scene.*`/`run.*` persist across
//!    shots within the episode (dsl §9.1), so `check_definite_assignment` runs
//!    ONCE over the whole-document concatenated node stream (all shots' bodies in
//!    source order), never per-shot — a path written in shot 1 and read in shot 2
//!    must not read as maybe-unset.
//! 2. **`Ctx` construction.** `state` = the [`StateSchema`] from `parse_meta`
//!    (folded with the implicit `scene.choices.*` decls from every `<branch>`);
//!    `defs` = declared `defs:` names; `mode` from the input; `in_match` /
//!    `match_subject` set as the walk enters a `<match>` arm (for T4.3's `$`).
//! 3. **Determinism.** All diagnostics are sorted by `span.byte_start` then
//!    `code` before returning — the ordering the Phase-6 divergence golden
//!    (headless vs LSP) compares byte-for-byte. Every span's line/column/utf16 is
//!    re-derived from its bytes through one [`TextIndex`] so the two surfaces
//!    agree. A CEL parse failure is reported ONCE here (as `E-CEL-PARSE`,
//!    [`Layer::Cel`]) from `fill_document`'s errors and never aborts the walk.
//! 4. **`E-UNDECLARED` dedup.** A `::set` to an undeclared target is flagged by
//!    BOTH `check_set` ([`Layer::Staging`], the precise `path_span`) and
//!    `check_definite_assignment` ([`Layer::Logic`]). We collapse overlapping
//!    `E-UNDECLARED` spans to the narrowest (most precise) one — BEFORE the sort.
//! 5. **`E-REF-TYPE`.** Still deferred: the check needs per-def type info threaded
//!    into `Ctx` AND an expected-type per CEL slot (neither exists yet in T4.3's
//!    `check_cel_slot`), so wiring it here would touch T4.3. `defs` names ARE
//!    threaded (so `@ref` existence resolves); the type-context match is future.
//!
//! Plus: `is_exhaustive` (T4.6) suppresses T4.4's known false positive — a
//! maybe-unset `<match>` SUBJECT read on a domain-exhaustive match (the arms'
//! join covers every case, so the subject read cannot escape unhandled).
//!
//! ## `Resolved` view (Some-vs-None policy)
//! The resolved view is best-effort and computed unconditionally (parse, fill,
//! `resolve_timeline`, and the injection fold never panic). `resolved` is `Some`
//! unless a STRUCTURAL parse error corrupts the node stream itself
//! (`E-UNCLASSIFIED` / `E-UNCLOSED-TAG` / `E-COMMENT-UNTERMINATED` /
//! `E-META-PARSE`) — then the view would be misleading, so it is `None`. Semantic
//! errors (unknown directive, undeclared state, non-exhaustive match, …) still
//! yield a resolved view. A clean document is always `Some`.

use lute_cel::{fill_document, CelArena};
use lute_core_span::{Diagnostic, Layer, Severity, Span, TextIndex};
use lute_manifest::provider::ProviderSet;
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_syntax::ast::{Arm, Attr, AttrValue, ClipNode, Document, Node};
use lute_syntax::parse;

use crate::ctx::{Ctx, Mode};
use crate::directives::check_directive;
use crate::inject::{lower_node, InjectedCommand, StageState};
use crate::meta::parse_meta;
use crate::timeline::{resolve_timeline, ResolvedTimeline};
use crate::{
    check_branch, check_cel_slot, check_definite_assignment, check_match, check_set, is_exhaustive,
};

/// Diagnostic code for a CEL fragment that failed to parse (surfaced once here
/// from `fill_document`'s errors; the slot's `ast` stays `None`).
const E_CEL_PARSE: &str = "E-CEL-PARSE";

/// Parse errors that corrupt the node stream, making the `Resolved` view
/// misleading (see the Some-vs-None policy in the module docs).
const STRUCTURAL_CODES: &[&str] = &[
    "E-UNCLASSIFIED",
    "E-UNCLOSED-TAG",
    "E-COMMENT-UNTERMINATED",
    "E-META-PARSE",
];

/// The input to one `check()` invocation — the document text plus the resolved
/// capability surface it is validated against.
pub struct CheckInput {
    /// Raw `.lute` document source.
    pub text: String,
    /// Document identity (LSP uri / CLI path); carried through for diagnostics.
    pub uri: String,
    /// The resolved capability snapshot (directives, enums, defs, frontmatter).
    pub snapshot: CapabilitySnapshot,
    /// Pinned provider snapshots for `providerRef` id resolution (plugin §10).
    pub providers: ProviderSet,
    /// Author (interactive LSP) vs. Ci (batch) analysis mode.
    pub mode: Mode,
}

/// The result of one `check()`: every diagnostic (deduped, byte-sorted) plus the
/// best-effort resolved view when the document is structurally intact.
#[derive(Clone, Debug, serde::Serialize)]
pub struct CheckResult {
    /// `true` when no `Error`-severity diagnostic is present (drives the CLI exit
    /// code and the LSP "problems" gutter).
    pub ok: bool,
    /// All diagnostics, deduped and sorted by `(span.byte_start, code)`.
    pub diagnostics: Vec<Diagnostic>,
    /// The resolved view; `None` when a structural parse error corrupts the tree.
    pub resolved: Option<Resolved>,
}

/// The LSP-facing resolved view (arch "resolved view"): the compiler's
/// best-effort read of what the document lowers to, WITHOUT final flat-record
/// codegen (scoped out of this plan).
#[derive(Clone, Debug, serde::Serialize)]
pub struct Resolved {
    /// A shallow, depth-1 textual preview of the authored top-level command
    /// stream in document order — one entry per top-level node (nested arm /
    /// choice / clip bodies are summarized by their opener, not expanded). Full
    /// desugaring is a Phase-5+ concern; this is a human-readable outline.
    pub commands_preview: Vec<String>,
    /// One resolved table per `<timeline>` in document order (dsl §11.4).
    pub timeline_tables: Vec<ResolvedTimeline>,
    /// Every command the injection reducer inserted, with provenance, folded over
    /// the document node stream (arch stateful resolution / auto-injection).
    pub injections: Vec<InjectedCommand>,
}

/// Statically validate a `.lute` document and return its structured result.
///
/// Never panics: every stage degrades to diagnostics + a best-effort view.
pub fn check(input: &CheckInput) -> CheckResult {
    let idx = TextIndex::new(&input.text);

    // 1. Parse the DSL structure.
    let (mut doc, parse_diags) = parse(&input.text);

    // 2. Fill every CEL slot; a parse failure is reported ONCE here and never
    //    aborts the walk (CelSlot isolation). check_cel_slot skips the AST pass
    //    for a slot whose `ast` stayed `None`, so no duplicate CEL diagnostics.
    let mut arena = CelArena::default();
    let cel_errors = fill_document(&mut arena, &mut doc);
    let cel_diags: Vec<Diagnostic> = cel_errors
        .into_iter()
        .map(|e| Diagnostic {
            code: E_CEL_PARSE.to_string(),
            severity: Severity::Error,
            message: e.message,
            span: e.span,
            layer: Layer::Cel,
            fixits: Vec::new(),
            provenance: None,
        })
        .collect();

    // 3. Typed frontmatter + inline state schema.
    let (typed, meta_diags) = parse_meta(&doc.meta, &input.snapshot);

    // 4. Fold every `<branch>`'s implicit `scene.choices.<id>` decl into the
    //    schema BEFORE the checks that resolve against it (match subjects, CEL
    //    state paths). This pre-pass owns the episode-wide `E-DUP-BRANCH`
    //    detection so the main walk never double-counts branch ids.
    let mut schema = typed.state.clone();
    let mut seen_branches = std::collections::BTreeSet::new();
    let mut branch_diags = Vec::new();
    fold_branches(&doc, &mut schema, &mut seen_branches, &mut branch_diags);

    // The def names the `@ref` resolver validates against (dsl §8.1).
    let defs: std::collections::BTreeSet<String> = typed.defs.keys().cloned().collect();

    let base_ctx = Ctx {
        in_match: false,
        match_subject: None,
        mode: input.mode,
        state: schema.clone(),
        defs,
    };

    // 5. Per-node validator walk (directives / cel-slots / set / match / timeline).
    let mut walker = Walker {
        snapshot: &input.snapshot,
        providers: &input.providers,
        arena: &arena,
        diags: Vec::new(),
        timeline_tables: Vec::new(),
        exhaustive_subject_spans: Vec::new(),
    };
    for shot in &doc.shots {
        walker.walk(&shot.body, &base_ctx);
    }

    // 6. Document-level definite-assignment over the concatenated node stream
    //    (carry-forward #1): `scene.*`/`run.*` persist across shots.
    let all_nodes: Vec<Node> = doc
        .shots
        .iter()
        .flat_map(|s| s.body.iter().cloned())
        .collect();
    let defassign_diags = check_definite_assignment(&all_nodes, &schema, &base_ctx);

    // 7. Resolved view: injection fold + the timeline tables gathered in the walk.
    let mut inject_state = StageState::default();
    let mut injections = Vec::new();
    for shot in &doc.shots {
        fold_injections(&shot.body, &mut inject_state, &mut injections);
    }
    let inject_diags = std::mem::take(&mut inject_state.diags);
    let commands_preview: Vec<String> = doc
        .shots
        .iter()
        .flat_map(|s| s.body.iter().map(node_summary))
        .collect();

    // 8. Collect every diagnostic, then apply the ordering contract.
    let mut diags = Vec::new();
    diags.extend(parse_diags);
    diags.extend(cel_diags);
    diags.extend(meta_diags);
    diags.extend(branch_diags);
    diags.extend(std::mem::take(&mut walker.diags));
    diags.extend(defassign_diags);
    diags.extend(inject_diags);

    // is_exhaustive suppression (carry-forward, T4.6 x T4.4): drop a maybe-unset
    // read whose span is a domain-exhaustive `<match>` subject.
    suppress_exhaustive_subject_reads(&mut diags, &walker.exhaustive_subject_spans);

    // Dedup overlapping `E-UNDECLARED` (carry-forward #4) BEFORE the sort.
    let mut diags = dedup_undeclared(diags);

    // Normalize every span's line/column/utf16 from its bytes (some validators
    // leave them zeroed), then sort deterministically (carry-forward #3).
    normalize_spans(&idx, &input.text, &mut diags);
    diags.sort_by(|a, b| {
        a.span
            .byte_start
            .cmp(&b.span.byte_start)
            .then_with(|| a.code.cmp(&b.code))
    });

    // Some-vs-None policy for the resolved view.
    let structural_break = diags
        .iter()
        .any(|d| STRUCTURAL_CODES.contains(&d.code.as_str()));
    let resolved = if structural_break {
        None
    } else {
        Some(Resolved {
            commands_preview,
            timeline_tables: walker.timeline_tables,
            injections,
        })
    };

    let ok = !diags.iter().any(|d| d.severity == Severity::Error);
    let _ = &input.uri; // carried for the surfaces; check() itself is uri-agnostic.
    CheckResult {
        ok,
        diagnostics: diags,
        resolved,
    }
}

/// The per-node validator walk. Holds the read-only capability surface and the
/// mutable diagnostic/table/suppression accumulators; `Ctx` is passed per level
/// so `in_match`/`match_subject` can be toggled for `<match>` arms without
/// re-cloning the schema on every node.
struct Walker<'a> {
    snapshot: &'a CapabilitySnapshot,
    providers: &'a ProviderSet,
    arena: &'a CelArena,
    diags: Vec<Diagnostic>,
    timeline_tables: Vec<ResolvedTimeline>,
    /// Subject spans of domain-exhaustive `<match>`es, for the T4.4 suppression.
    exhaustive_subject_spans: Vec<Span>,
}

impl Walker<'_> {
    fn walk(&mut self, nodes: &[Node], ctx: &Ctx) {
        for node in nodes {
            match node {
                Node::Line(l) => self.check_attr_refs(&l.attrs, ctx),
                Node::Directive(d) => {
                    self.diags
                        .extend(check_directive(d, self.snapshot, self.providers, ctx));
                    self.check_attr_refs(&d.attrs, ctx);
                }
                Node::Set(s) => {
                    self.diags.extend(check_set(s, &ctx.state, ctx));
                    self.diags.extend(check_cel_slot(&s.expr, self.arena, ctx));
                }
                Node::Branch(b) => {
                    // `E-DUP-BRANCH` + decl folding happened in the pre-pass; here
                    // we only validate the branch's own attrs and recurse.
                    self.check_attr_refs(&b.attrs, ctx);
                    for choice in &b.choices {
                        self.check_attr_refs(&choice.attrs, ctx);
                        if let Some(when) = &choice.when {
                            self.diags.extend(check_cel_slot(when, self.arena, ctx));
                        }
                        self.walk(&choice.body, ctx);
                    }
                }
                Node::Match(m) => {
                    self.diags.extend(check_match(m, &ctx.state, ctx));
                    // The subject expression is evaluated OUTSIDE match scope: `$`
                    // is only valid in a `<when test>` (dsl §8.2), never in `on=`.
                    // Force `in_match=false` so a nested `<match on="$">` (whose
                    // incoming ctx has in_match=true from the enclosing arm) is
                    // correctly flagged E-DOLLAR-OUTSIDE-MATCH.
                    let subject_ctx = Ctx {
                        in_match: false,
                        match_subject: None,
                        ..ctx.clone()
                    };
                    self.diags
                        .extend(check_cel_slot(&m.subject, self.arena, &subject_ctx));
                    if is_exhaustive(m, &ctx.state) {
                        self.exhaustive_subject_spans.push(m.subject.span);
                    }
                    // Arms (tests + bodies) evaluate WITHIN match scope: `$` binds
                    // to the subject (carry-forward #2).
                    let arm_ctx = Ctx {
                        in_match: true,
                        match_subject: Some(m.subject.raw.clone()),
                        ..ctx.clone()
                    };
                    for arm in &m.arms {
                        match arm {
                            Arm::When { test, body, .. } => {
                                self.diags
                                    .extend(check_cel_slot(test, self.arena, &arm_ctx));
                                self.walk(body, &arm_ctx);
                            }
                            Arm::Otherwise { body, .. } => self.walk(body, &arm_ctx),
                        }
                    }
                }
                Node::Timeline(tl) => {
                    let (table, tdiags) = resolve_timeline(tl, ctx);
                    self.timeline_tables.push(table);
                    self.diags.extend(tdiags);
                    if let Some(dur) = &tl.duration {
                        self.diags.extend(check_cel_slot(dur, self.arena, ctx));
                    }
                    for track in &tl.tracks {
                        for clip in &track.clips {
                            match &clip.node {
                                ClipNode::Directive(d) => {
                                    self.diags.extend(check_directive(
                                        d,
                                        self.snapshot,
                                        self.providers,
                                        ctx,
                                    ));
                                    self.check_attr_refs(&d.attrs, ctx);
                                }
                                ClipNode::Set(s) => {
                                    self.diags.extend(check_set(s, &ctx.state, ctx));
                                    self.diags.extend(check_cel_slot(&s.expr, self.arena, ctx));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Validate every `@ref`-valued attribute's CEL slot in the current scope.
    fn check_attr_refs(&mut self, attrs: &[Attr], ctx: &Ctx) {
        for attr in attrs {
            if let AttrValue::Ref(slot) = &attr.value {
                self.diags.extend(check_cel_slot(slot, self.arena, ctx));
            }
        }
    }
}

/// Pre-pass: fold every `<branch>`'s implicit `scene.choices.<id>` declaration
/// into `schema` in document order, threading the episode-wide `seen` id set so
/// `E-DUP-BRANCH` fires exactly once per duplicate. Recurses into nested bodies
/// (a branch may live inside a match arm / another branch's choice).
fn fold_branches(
    doc: &Document,
    schema: &mut crate::meta::StateSchema,
    seen: &mut std::collections::BTreeSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    for shot in &doc.shots {
        fold_branches_nodes(&shot.body, schema, seen, diags);
    }
}

fn fold_branches_nodes(
    nodes: &[Node],
    schema: &mut crate::meta::StateSchema,
    seen: &mut std::collections::BTreeSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    for node in nodes {
        match node {
            Node::Branch(b) => {
                let rec = check_branch(b, seen);
                schema.decls.insert(rec.path, rec.decl);
                diags.extend(rec.diags);
                for choice in &b.choices {
                    fold_branches_nodes(&choice.body, schema, seen, diags);
                }
            }
            Node::Match(m) => {
                for arm in &m.arms {
                    match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => {
                            fold_branches_nodes(body, schema, seen, diags)
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// Fold the injection reducer over a node slice, threading `StageState` and
/// collecting every injected command. Recurses into `<branch>`/`<match>` bodies
/// in document order (best-effort linearization: parallel-arm state divergence
/// is not modeled — a preview, not final codegen). `<timeline>` clips are staged
/// separately in `timeline_tables` and do not participate in stage-entity
/// lifetime here (see the injection reducer's node-kind coverage).
fn fold_injections(nodes: &[Node], state: &mut StageState, out: &mut Vec<InjectedCommand>) {
    for (i, node) in nodes.iter().enumerate() {
        let taken = std::mem::take(state);
        let (next, emit) = lower_node(taken, node, &nodes[i + 1..]);
        *state = next;
        out.extend(emit);
        match node {
            Node::Branch(b) => {
                for choice in &b.choices {
                    fold_injections(&choice.body, state, out);
                }
            }
            Node::Match(m) => {
                for arm in &m.arms {
                    match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => {
                            fold_injections(body, state, out)
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// Drop `E-MAYBE-UNSET` diagnostics whose span is a domain-exhaustive `<match>`
/// subject (T4.6 x T4.4 carry-forward). A subject read that maybe-unset on entry
/// is nonetheless safe when the match's arms cover every case (the join is an
/// intersection over all arms, not a fall-through), so the read cannot escape
/// unhandled — reporting it would be a false positive.
fn suppress_exhaustive_subject_reads(diags: &mut Vec<Diagnostic>, subject_spans: &[Span]) {
    if subject_spans.is_empty() {
        return;
    }
    diags.retain(|d| {
        !(d.code == "E-MAYBE-UNSET"
            && subject_spans
                .iter()
                .any(|s| s.byte_start == d.span.byte_start && s.byte_end == d.span.byte_end))
    });
}

/// Collapse overlapping `E-UNDECLARED` diagnostics to the single most precise
/// (narrowest) span per location (carry-forward #4). The same undeclared `::set`
/// target is flagged by `check_set` (`Layer::Staging`, precise `path_span`) and
/// `check_definite_assignment` (`Layer::Logic`); we keep one. Non-`E-UNDECLARED`
/// diagnostics pass through untouched.
fn dedup_undeclared(diags: Vec<Diagnostic>) -> Vec<Diagnostic> {
    let mut undeclared: Vec<Diagnostic> = Vec::new();
    let mut out: Vec<Diagnostic> = Vec::new();
    for d in diags {
        if d.code == "E-UNDECLARED" {
            undeclared.push(d);
        } else {
            out.push(d);
        }
    }
    // Narrowest span first at each start offset, so the most precise entry is the
    // one kept when a wider overlapping entry follows.
    undeclared.sort_by(|a, b| {
        a.span.byte_start.cmp(&b.span.byte_start).then_with(|| {
            (a.span.byte_end - a.span.byte_start).cmp(&(b.span.byte_end - b.span.byte_start))
        })
    });
    let mut kept: Vec<Diagnostic> = Vec::new();
    for d in undeclared {
        let dp = undeclared_path(&d.message);
        // Collapse only when an already-kept entry names the SAME state path AND
        // overlaps in span. Two distinct undeclared paths that share one CEL
        // slot's whole-slot fallback span (cel-parser 0.10.1 has no per-node
        // offsets) therefore BOTH survive; the sanctioned T4.4(Logic)+T4.5(Staging)
        // pair for one `::set` target (identical path + span) still merges to one.
        if !kept
            .iter()
            .any(|k| undeclared_path(&k.message) == dp && spans_overlap(k.span, d.span))
        {
            kept.push(d);
        }
    }
    out.extend(kept);
    out
}

/// Half-open byte-interval overlap.
fn spans_overlap(a: Span, b: Span) -> bool {
    a.byte_start < b.byte_end && b.byte_start < a.byte_end
}

/// Extract the state path an `E-UNDECLARED` message names, for path-aware dedup.
/// All three producers embed the path as the first backtick-quoted token that
/// starts with a state tier (`scene.`/`run.`/`user.`/`app.`) — `set_op` also
/// quotes `::set` first, so we scan for the tier-prefixed token, not just the
/// first quote. `None` (no tier token) falls back to span-only collapse.
fn undeclared_path(message: &str) -> Option<&str> {
    message.split('`').find(|tok| {
        tok.starts_with("scene.")
            || tok.starts_with("run.")
            || tok.starts_with("user.")
            || tok.starts_with("app.")
    })
}

/// Re-derive every diagnostic's `line`/`column`/`utf16_range` from its byte
/// offsets through one shared [`TextIndex`], so both the CLI and the LSP report
/// identical positions (the divergence golden). Offsets are clamped to the text
/// length defensively; they are within bounds by construction.
fn normalize_spans(idx: &TextIndex, text: &str, diags: &mut [Diagnostic]) {
    let len = text.len();
    for d in diags {
        let mut start = d.span.byte_start.min(len);
        let mut end = d.span.byte_end.min(len).max(start);
        // Snap to char boundaries so from_bytes never slices mid-code-point
        // (honors the "never panics" contract even if a producer ever emits an
        // interior offset; unreachable today, all producers emit boundary offsets).
        while start > 0 && !text.is_char_boundary(start) {
            start -= 1;
        }
        while end < len && !text.is_char_boundary(end) {
            end += 1;
        }
        d.span = Span::from_bytes(idx, start, end);
    }
}

/// A short, one-line desugared summary of a top-level node for the preview.
fn node_summary(node: &Node) -> String {
    match node {
        Node::Line(l) => format!(":line[{}]", l.speaker),
        Node::Directive(d) => format!("::{}", d.tag),
        Node::Set(s) => format!("::set{{{} {} …}}", s.path, s.op),
        Node::Branch(b) => format!("<branch id=\"{}\"> ({} choices)", b.id, b.choices.len()),
        Node::Match(m) => format!("<match on=\"{}\"> ({} arms)", m.subject.raw, m.arms.len()),
        Node::Timeline(tl) => format!("<timeline> ({} tracks)", tl.tracks.len()),
    }
}
