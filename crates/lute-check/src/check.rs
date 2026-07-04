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
use lute_manifest::schema::{SlotDecl, StateShape};
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_manifest::types::{type_accepts, Literal, PathSegment, Type};
use lute_syntax::ast::{Arm, Attr, AttrValue, Choice, ClipNode, Directive, Document, Node};
use lute_syntax::parse;

use crate::ctx::{Ctx, ExpectedType, Mode};
use crate::directives::check_directive;
use crate::inject::{lower_node, InjectedCommand, StageState};
use crate::meta::parse_meta;
use crate::schema_import::SchemaImports;
use crate::set_op::resolve_type;
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
    /// Resolved `uses:` schema imports (dsl §9.2): imported state/defs merged
    /// into this document's schema, plus resolution diagnostics. Empty when the
    /// scene has no `uses:` (or on a surface that cannot resolve files).
    pub imports: SchemaImports,
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

/// Extract ordered `(param name, Type)` pairs from an inline/imported def's raw
/// YAML value (dsl §8.1). Reads the def's `params:` sub-MAPPING in SOURCE order
/// (`serde_yaml::Mapping` is insertion-ordered), deserializing each value to a
/// `Type` via the same serde path `Type` uses. An absent/non-mapping `params:`
/// or a malformed entry yields no pair — never a panic.
fn params_from_yaml(v: &serde_yaml::Value) -> Vec<(String, lute_manifest::types::Type)> {
    let Some(map) = v.get("params").and_then(|p| p.as_mapping()) else {
        return Vec::new();
    };
    map.iter()
        .filter_map(|(k, tv)| {
            let name = k.as_str()?.to_string();
            let ty = serde_yaml::from_value::<lute_manifest::types::Type>(tv.clone()).ok()?;
            Some((name, ty))
        })
        .collect()
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
    // Merge imported schema (dsl §9.2) first, then the scene's inline `state:`.
    // Precedence depends on WHERE the imported decl came from (see
    // `SchemaImports::state_overridable`): a `uses`-peer path may NOT be
    // redeclared (`E-STATE-REDECLARE`, imported wins), but an `extends`-base path
    // MAY be refined by the scene's inline decl (the inline wins; a TYPE change is
    // `E-EXTENDS-STATE-TYPE`, the persisted type must stay stable).
    let mut schema = input.imports.state.clone();
    let mut state_merge_diags: Vec<Diagnostic> = Vec::new();
    for (path, decl) in &typed.state.decls {
        match input.imports.state.decls.get(path) {
            Some(imported) if input.imports.state_overridable.contains(path) => {
                // Extends-base override: the inline decl wins; guard the type.
                if decl.ty != imported.ty {
                    state_merge_diags.push(Diagnostic {
                        code: "E-EXTENDS-STATE-TYPE".to_string(),
                        severity: Severity::Error,
                        message: format!(
                            "state path `{path}` overrides base declared type {:?} with {:?}; persisted state must keep a stable type",
                            imported.ty, decl.ty
                        ),
                        span: doc.meta.span,
                        layer: Layer::Content,
                        fixits: Vec::new(),
                        provenance: None,
                    });
                }
                schema.decls.insert(path.clone(), decl.clone());
            }
            Some(_) => {
                // Uses-peer path: a scene must not redeclare it (imported wins).
                state_merge_diags.push(Diagnostic {
                    code: "E-STATE-REDECLARE".to_string(),
                    severity: Severity::Error,
                    message: format!(
                        "state path `{path}` is declared by an imported schema (§9.2); a scene must not redeclare or override it"
                    ),
                    span: doc.meta.span,
                    layer: Layer::Content,
                    fixits: Vec::new(),
                    provenance: None,
                });
            }
            None => {
                schema.decls.insert(path.clone(), decl.clone());
            }
        }
    }
    let mut seen_branches = std::collections::BTreeSet::new();
    let mut branch_diags = Vec::new();
    fold_branches(&doc, &mut schema, &mut seen_branches, &mut branch_diags);

    // 4b. Expand every active directive's `state.declares[]` into concrete state
    //     slots at each use site (plugin §8/§9): a `::minigame{resultKey="k"}`
    //     opens `scene.minigame.k.<field>` for each field of its shape. This runs
    //     before the walk + defassign so plugin-declared state resolves.
    fold_directive_slots(&doc, &input.snapshot, &mut schema);

    // The def names the `@ref` resolver validates against (dsl §8.1): inline
    // frontmatter defs plus plugin-exported defs (both are declared refs).
    let mut defs: std::collections::BTreeSet<String> = typed.defs.keys().cloned().collect();
    defs.extend(input.snapshot.defs.keys().cloned());
    defs.extend(input.imports.defs.keys().cloned());

    // The def name -> produced `Type` table the `@ref` type-context check
    // (`E-REF-TYPE`, dsl §8) resolves against, merged from two sources.
    let mut def_types: std::collections::BTreeMap<String, lute_manifest::types::Type> =
        std::collections::BTreeMap::new();
    // Plugin defs are already typed.
    for (name, d) in &input.snapshot.defs {
        def_types.insert(name.clone(), d.ty.clone());
    }
    // Imported schema defs (untyped like inline; extract `type:`). Imported
    // overrides plugin; inline (below) overrides imported.
    for (name, v) in &input.imports.defs {
        if let Some(t) = v
            .get("type")
            .cloned()
            .and_then(|tv| serde_yaml::from_value::<lute_manifest::types::Type>(tv).ok())
        {
            def_types.insert(name.clone(), t);
        }
    }
    // Inline frontmatter defs are stored untyped; extract the `type:` sub-value
    // and deserialize it via the same serde path `Type` uses. Malformed/absent
    // -> skip (never a panic). Inline overrides plugin (scene-local).
    for (name, v) in &typed.defs {
        if let Some(t) = v
            .get("type")
            .cloned()
            .and_then(|tv| serde_yaml::from_value::<lute_manifest::types::Type>(tv).ok())
        {
            def_types.insert(name.clone(), t);
        }
    }

    // Parallel table of ORDERED params per def (dsl §8.1), for `@name(args)`
    // arity/arg-type checks. Same three sources & precedence as `def_types`
    // (plugin < imported < inline).
    let mut def_params: std::collections::BTreeMap<
        String,
        Vec<(String, lute_manifest::types::Type)>,
    > = std::collections::BTreeMap::new();
    // Plugin defs carry ordered `Vec<DefParam>` directly.
    for (name, d) in &input.snapshot.defs {
        def_params.insert(
            name.clone(),
            d.params
                .iter()
                .map(|p| (p.name.clone(), p.ty.clone()))
                .collect(),
        );
    }
    // Imported schema defs (untyped YAML): extract `params:` in order. Imported
    // overrides plugin; inline (below) overrides imported.
    for (name, v) in &input.imports.defs {
        def_params.insert(name.clone(), params_from_yaml(v));
    }
    // Inline frontmatter defs (untyped YAML): same extraction; scene-local override.
    for (name, v) in &typed.defs {
        def_params.insert(name.clone(), params_from_yaml(v));
    }

    let base_ctx = Ctx {
        in_match: false,
        match_subject: None,
        mode: input.mode,
        state: schema.clone(),
        defs,
        def_types,
        def_params,
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
    diags.extend(input.imports.diags.clone());
    diags.extend(state_merge_diags);
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
                Node::Line(l) => self.check_attr_refs(&l.attrs, ctx, None),
                Node::Directive(d) => {
                    self.diags
                        .extend(check_directive(d, self.snapshot, self.providers, ctx));
                    self.check_attr_refs(&d.attrs, ctx, Some(&d.tag));
                }
                Node::Set(s) => {
                    self.diags.extend(check_set(s, &ctx.state, ctx));
                    let expected = resolve_type(&s.path, &ctx.state)
                        .cloned()
                        .map(ExpectedType::Ty);
                    self.diags
                        .extend(check_cel_slot(&s.expr, self.arena, ctx, expected.as_ref()));
                }
                Node::Branch(b) => {
                    // `E-DUP-BRANCH` + decl folding happened in the pre-pass; here
                    // we only validate the branch's own attrs and recurse.
                    self.check_attr_refs(&b.attrs, ctx, None);
                    for choice in &b.choices {
                        self.check_attr_refs(&choice.attrs, ctx, None);
                        check_choice_persist(choice, ctx, &mut self.diags);
                        if let Some(when) = &choice.when {
                            self.diags.extend(check_cel_slot(
                                when,
                                self.arena,
                                ctx,
                                Some(&ExpectedType::Bool),
                            ));
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
                    let subject_expected = resolve_type(&m.subject.raw, &subject_ctx.state)
                        .cloned()
                        .map(ExpectedType::Ty);
                    self.diags.extend(check_cel_slot(
                        &m.subject,
                        self.arena,
                        &subject_ctx,
                        subject_expected.as_ref(),
                    ));
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
                                self.diags.extend(check_cel_slot(
                                    test,
                                    self.arena,
                                    &arm_ctx,
                                    Some(&ExpectedType::Bool),
                                ));
                                self.walk(body, &arm_ctx);
                            }
                            Arm::Otherwise { body, .. } => self.walk(body, &arm_ctx),
                        }
                    }
                }
                Node::Timeline(tl) => {
                    let (table, tdiags) = resolve_timeline(tl, ctx, self.snapshot);
                    self.timeline_tables.push(table);
                    self.diags.extend(tdiags);
                    if let Some(dur) = &tl.duration {
                        self.diags
                            .extend(check_cel_slot(dur, self.arena, ctx, None));
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
                                    self.check_attr_refs(&d.attrs, ctx, Some(&d.tag));
                                }
                                ClipNode::Set(s) => {
                                    self.diags.extend(check_set(s, &ctx.state, ctx));
                                    let expected = resolve_type(&s.path, &ctx.state)
                                        .cloned()
                                        .map(ExpectedType::Ty);
                                    self.diags.extend(check_cel_slot(
                                        &s.expr,
                                        self.arena,
                                        ctx,
                                        expected.as_ref(),
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Validate every `@ref`-valued attribute's CEL slot in the current scope.
    /// `directive_tag` is `Some` only for directive attrs, letting a `@ref` attr
    /// value be typed against the attr's declared type (`E-REF-TYPE`, dsl §8).
    fn check_attr_refs(&mut self, attrs: &[Attr], ctx: &Ctx, directive_tag: Option<&str>) {
        for attr in attrs {
            if let AttrValue::Ref(slot) = &attr.value {
                let expected = directive_tag
                    .and_then(|tag| self.snapshot.directive(tag))
                    .and_then(|decl| decl.attrs.iter().find(|a| a.name == attr.key))
                    .map(|a| ExpectedType::Ty(a.ty.clone()));
                self.diags
                    .extend(check_cel_slot(slot, self.arena, ctx, expected.as_ref()));
            }
        }
    }
}

/// Diagnostic codes for the `<choice … persist="run">` run-fact sugar (dsl §11.1.1).
const E_PERSIST_TARGET: &str = "E-PERSIST-TARGET";
const E_PERSIST_MISSING_AS: &str = "E-PERSIST-MISSING-AS";
const E_PERSIST_VALUE: &str = "E-PERSIST-VALUE";
const E_PERSIST_CONFLICT: &str = "E-PERSIST-CONFLICT";

/// Validate a `<choice>`'s run-fact promotion sugar (dsl §11.1.1):
/// `persist="run" as="run.<path>" [value="<lit>"]` records a NAMED, declared
/// `run.*` fact when the choice is selected. The sugar is EXACTLY a
/// `::set{run.<path> = <value>}` appended to the arm — the engine materializes
/// the write, so the checker only validates well-formedness. A `<choice>` with
/// no `persist` attr is untouched. The `persist`/`as`/`value` attrs are
/// recognized here, so they are never reported as unknown/extra.
fn check_choice_persist(choice: &Choice, ctx: &Ctx, diags: &mut Vec<Diagnostic>) {
    // Only choices carrying a `persist` attr participate.
    let Some(persist) = choice.attrs.iter().find(|a| a.key == "persist") else {
        return;
    };
    // Rule 1 (§11.1.1): cross-episode facts live in `run.*`, so `persist` MUST
    // be `"run"`.
    if str_attr(persist) != Some("run") {
        diags.push(persist_diag(
            E_PERSIST_TARGET,
            "`persist` must be `\"run\"`: cross-episode facts live in the `run.*` \
             namespace (dsl §11.1.1)"
                .to_string(),
            choice.span,
        ));
        return;
    }
    // Rule 2: `as` is REQUIRED and MUST name a `run.*` path.
    let Some(as_attr) = choice.attrs.iter().find(|a| a.key == "as") else {
        diags.push(persist_diag(
            E_PERSIST_MISSING_AS,
            "`persist=\"run\"` requires `as=\"run.<path>\"` naming the run fact to record \
             (dsl §11.1.1)"
                .to_string(),
            choice.span,
        ));
        return;
    };
    let Some(as_path) = str_attr(as_attr) else {
        diags.push(persist_diag(
            E_PERSIST_TARGET,
            "`as` must be a `run.<path>` string literal (dsl §11.1.1)".to_string(),
            choice.span,
        ));
        return;
    };
    if as_path
        .strip_prefix("run.")
        .filter(|rest| !rest.is_empty())
        .is_none()
    {
        diags.push(persist_diag(
            E_PERSIST_TARGET,
            format!(
                "`as=\"{as_path}\"` must name a `run.<path>` fact (a bare `run` or a \
                 non-`run.*` path is not a valid run target) (dsl §11.1.1)"
            ),
            choice.span,
        ));
        return;
    }
    // Rule 3 (§9.2/§11.1.1): the `as` path MUST already be declared in the
    // merged schema — a typo'd/undeclared `as` cannot silently create a field.
    let Some(ty) = resolve_type(as_path, &ctx.state) else {
        diags.push(persist_diag(
            "E-UNDECLARED",
            format!(
                "persist target `{as_path}` is not declared in the run schema (dsl §11.1.1); \
                 an undeclared/typo'd `as` cannot create a field"
            ),
            choice.span,
        ));
        return;
    };
    // Rule 4 (§11.1.1): the `value` policy depends on the declared type.
    check_persist_value(
        ty,
        choice.attrs.iter().find(|a| a.key == "value"),
        as_path,
        choice.span,
        diags,
    );
    // Rule 5: the arm must not already `::set` the same path — the persist write
    // would duplicate it.
    if choice
        .body
        .iter()
        .any(|n| matches!(n, Node::Set(s) if s.path == as_path))
    {
        diags.push(persist_diag(
            E_PERSIST_CONFLICT,
            format!(
                "the choice arm already `::set`s `{as_path}`, which `persist=\"run\" \
                 as=\"{as_path}\"` also writes (dsl §11.1.1)"
            ),
            choice.span,
        ));
    }
}

/// Rule 4 of the persist sugar (§11.1.1): a `bool` path's `value` is OPTIONAL
/// (defaults to `true`) and, when present, MUST be a bool literal; every other
/// path (`number`/`enum`/…) REQUIRES a type-compatible literal.
fn check_persist_value(
    ty: &Type,
    value: Option<&Attr>,
    as_path: &str,
    span: Span,
    diags: &mut Vec<Diagnostic>,
) {
    let accepted =
        |v: &Attr| persist_literal(ty, &v.value).is_some_and(|lit| type_accepts(ty, &lit));
    match ty {
        Type::Bool => {
            if let Some(v) = value {
                if !accepted(v) {
                    diags.push(persist_diag(
                        E_PERSIST_VALUE,
                        format!(
                            "`value` for the bool path `{as_path}` must be a bool literal \
                             (`true`/`false`) (dsl §11.1.1)"
                        ),
                        span,
                    ));
                }
            }
        }
        _ => match value {
            None => diags.push(persist_diag(
                E_PERSIST_VALUE,
                format!(
                    "`value` is required for `{as_path}` (only a `bool` path defaults to \
                     `true`) (dsl §11.1.1)"
                ),
                span,
            )),
            Some(v) if !accepted(v) => diags.push(persist_diag(
                E_PERSIST_VALUE,
                format!(
                    "`value` is not compatible with the declared type of `{as_path}` \
                     (dsl §11.1.1)"
                ),
                span,
            )),
            Some(_) => {}
        },
    }
}

/// The string value of an attr, when it is a plain string literal (`key="s"`).
/// A bare (`BoolTrue`) or `@ref`-valued attr yields `None`.
fn str_attr(attr: &Attr) -> Option<&str> {
    match &attr.value {
        AttrValue::Str(s) => Some(s),
        _ => None,
    }
}

/// Coerce a persist `value` attr into a manifest [`Literal`] *in the resolved
/// target type's domain* so [`type_accepts`] can judge it — mirroring the
/// directive attr coercion (`directives::literal_of`). A `number` target parses
/// the string as `f64`; a `bool` target accepts the bare `value` ident or the
/// strings `"true"`/`"false"`; every other target (`enum`/`str`/…) keeps the
/// value VERBATIM as [`Literal::Str`], so an enum member spelled like a bool or
/// number (`"true"`, `"3"`) still resolves by string membership. Returns `None`
/// when the value cannot inhabit the target's shape (a hard type error) or is a
/// `@ref`.
fn persist_literal(ty: &Type, v: &AttrValue) -> Option<Literal> {
    match (ty, v) {
        (Type::Number, AttrValue::Str(s)) => s.parse::<f64>().ok().map(Literal::Num),
        (Type::Bool, AttrValue::BoolTrue) => Some(Literal::Bool(true)),
        (Type::Bool, AttrValue::Str(s)) => match s.as_str() {
            "true" => Some(Literal::Bool(true)),
            "false" => Some(Literal::Bool(false)),
            _ => None,
        },
        (_, AttrValue::Str(s)) => Some(Literal::Str(s.clone())),
        // A bare-ident `value` against a non-bool target is a type error.
        (_, AttrValue::BoolTrue) => None,
        (_, AttrValue::Ref(_)) => None,
    }
}

/// Build a `Layer::Logic` diagnostic for the persist sugar (a §11 branch check).
fn persist_diag(code: &str, message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity: Severity::Error,
        message,
        span,
        layer: Layer::Logic,
        fixits: Vec::new(),
        provenance: None,
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

/// Pre-pass: expand every active directive's `state.declares[]` into concrete
/// state slots at each use site (plugin §8/§9). A `::minigame{resultKey="k"}`
/// whose declaration declares `scene.minigame.<resultKey>` with shape
/// `minigameResult` opens `scene.minigame.k.<field>` for each field of that
/// shape, feeding the SAME `schema` the walk + defassign consume. Walks every
/// directive location (top-level, branch choices, match arms, timeline clips),
/// mirroring the CEL/inject walkers' recursion.
fn fold_directive_slots(
    doc: &Document,
    snapshot: &CapabilitySnapshot,
    schema: &mut crate::meta::StateSchema,
) {
    for shot in &doc.shots {
        fold_slots_nodes(&shot.body, snapshot, schema);
    }
}

fn fold_slots_nodes(
    nodes: &[Node],
    snapshot: &CapabilitySnapshot,
    schema: &mut crate::meta::StateSchema,
) {
    for node in nodes {
        match node {
            Node::Directive(d) => expand_directive_slots(d, snapshot, schema),
            Node::Branch(b) => {
                for c in &b.choices {
                    fold_slots_nodes(&c.body, snapshot, schema);
                }
            }
            Node::Match(m) => {
                for arm in &m.arms {
                    match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => {
                            fold_slots_nodes(body, snapshot, schema)
                        }
                    }
                }
            }
            Node::Timeline(tl) => {
                for track in &tl.tracks {
                    for clip in &track.clips {
                        if let ClipNode::Directive(d) = &clip.node {
                            expand_directive_slots(d, snapshot, schema);
                        }
                    }
                }
            }
            Node::Line(_) | Node::Set(_) => {}
        }
    }
}

/// Expand one directive USE: look up its declaration, and for each declared slot
/// resolve the concrete path and insert one `StateDecl` per field of the
/// referenced shape. A directive with no declaration / no `state` / an
/// unresolvable path / a missing shape / an untierable base is skipped.
fn expand_directive_slots(
    dir: &Directive,
    snapshot: &CapabilitySnapshot,
    schema: &mut crate::meta::StateSchema,
) {
    let Some(decl) = snapshot.directive(&dir.tag) else {
        return;
    };
    let Some(state) = &decl.state else {
        return;
    };
    for slot in &state.declares {
        let Some(base) = resolve_slot_path(slot, dir) else {
            continue;
        };
        let Some(shape) = snapshot.state_shapes.get(&slot.shape) else {
            continue;
        };
        let Some(ns) = crate::meta::namespace_of(&base) else {
            continue;
        };
        insert_shape_fields(
            schema,
            &base,
            ns,
            shape,
            snapshot,
            &mut std::collections::BTreeSet::new(),
        );
    }
}

/// Resolve a `SlotDecl`'s path (scope + segments) into a concrete dotted path at
/// a use site: literal segments verbatim; `fromAttr` segments -> that attr's
/// value. Returns `None` if any `fromAttr` attr is absent or not a plain string.
fn resolve_slot_path(slot: &SlotDecl, dir: &Directive) -> Option<String> {
    let mut parts = vec![slot.scope.clone()];
    for seg in &slot.path {
        match seg {
            PathSegment::Literal(s) => parts.push(s.clone()),
            PathSegment::FromAttr { from_attr } => {
                let val = attr_str(dir, &from_attr.name)?;
                parts.push(val);
            }
        }
    }
    Some(parts.join("."))
}

/// The string value of a directive attribute — a plain string literal only; a
/// Ref/CEL-valued key cannot seed a static path, so it yields `None`.
fn attr_str(dir: &Directive, key: &str) -> Option<String> {
    dir.attrs
        .iter()
        .find(|a| a.key == key)
        .and_then(|a| match &a.value {
            AttrValue::Str(s) => Some(s.clone()),
            _ => None,
        })
}

/// Insert one `StateDecl` per shape field at `<base>.<field>`; a field that
/// itself references a nested shape recurses into that shape. `visiting` tracks
/// the shapes on the current expansion path (by name) with stack semantics: a
/// shape already on the path is a cycle and is skipped, guaranteeing
/// termination on self- or mutually-referential shapes. Removing the name after
/// the field loop keeps legitimate diamonds (a shape reached via two disjoint
/// paths) from being flagged as false cycles.
fn insert_shape_fields(
    schema: &mut crate::meta::StateSchema,
    base: &str,
    ns: crate::meta::Namespace,
    shape: &StateShape,
    snapshot: &CapabilitySnapshot,
    visiting: &mut std::collections::BTreeSet<String>,
) {
    if !visiting.insert(shape.name.clone()) {
        return;
    }
    for f in &shape.fields {
        let path = format!("{base}.{}", f.name);
        if let Some(nested_name) = &f.shape {
            if let Some(nested) = snapshot.state_shapes.get(nested_name) {
                insert_shape_fields(schema, &path, ns, nested, snapshot, visiting);
                continue;
            }
        }
        schema.decls.insert(
            path,
            crate::meta::StateDecl {
                ty: f.ty.clone(),
                default: f.default.clone(),
                namespace: ns,
            },
        );
    }
    visiting.remove(&shape.name);
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
