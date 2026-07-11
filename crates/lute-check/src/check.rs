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
//!   -> fold <branch>/<hub> decls      -> folded schema (scene.choices.* + scene.visited.*) + E-DUP-BRANCH
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
//!    (folded with the implicit `scene.choices.*` / `scene.visited.*` decls from
//!    every `<branch>` and `<hub>`);
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

use lute_cel::{fill_document, parse_slot, scan_refs, CelArena};
use lute_core_span::{Diagnostic, Layer, Severity, Span, TextIndex};
use lute_manifest::provider::ProviderSet;
use lute_manifest::schema::{SlotDecl, StateShape};
use lute_manifest::snapshot::{CapabilitySnapshot, Domain};
use lute_manifest::types::{type_accepts, Literal, PathSegment, Type};
use lute_syntax::ast::{
    Arm, Attr, AttrValue, CelKind, CelSlot, Choice, ClipNode, Directive, Document,
    Interp, InterpKind, Node,
};
use lute_syntax::parse;
/// Delegate: the label-interp scanner now lives in `lute-syntax` (single source
/// of truth with the parser's content-line scan). Re-exported `pub(crate)` so
/// `crate::check::scan_label_interps` (used by `defassign`) and the local call
/// sites keep resolving without change.
pub(crate) use lute_syntax::scan_label_interps;

use crate::cel_resolve::{check_rule_guards, compatible};
use crate::component_import::ComponentSet;
use crate::ctx::{Ctx, Env, ExpectedType, Mode};
use crate::directives::{at_context, check_directive};
use crate::inject::{lower_node, InjectedCommand, StageState};
use crate::schema_import::{merge_domains, SchemaImports};
use crate::set_op::resolve_type;
use crate::timeline::{resolve_timeline, ResolvedTimeline};
use crate::{
    check_branch, check_cel_slot, check_definite_assignment, check_hub, check_line_codes,
    check_match, check_quest, check_quest_guard_defassign, check_set, is_exhaustive,
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
    /// Resolved `components:` imports (dsl §13): the component name -> definition
    /// table (params + presentational body) plus resolution diagnostics. Empty
    /// when the scene has no `components:` (or on a surface that cannot resolve
    /// files); validated against `::use` invocations in [`check`].
    pub components: ComponentSet,
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

/// The folded compile inputs `check()` builds internally (compile-spec §11
/// reuse-input exposure): the typed frontmatter, the analysis [`Env`] whose
/// `state` is the FOLDED schema (imported ∪ inline ∪ implicit
/// `scene.choices.*` ∪ plugin-declared slots), and the merged def CEL bodies
/// (`plugin < imported < inline`, mirroring `def_types`). One source of
/// truth: `check()` itself consumes this fold.
#[derive(Clone, Debug)]
pub struct FoldedEnv {
    pub typed: crate::meta::TypedMeta,
    pub env: Env,
    /// def name -> raw CEL body, merged plugin < imported < inline (D4 input).
    pub def_bodies: std::collections::BTreeMap<String, String>,
    /// The resolved root document kind (dsl 0.2.0 §3.1): `Scene` when
    /// unresolved (missing/unknown `kind:`) so scene stays the degrade-safe
    /// path and downstream dispatch never panics.
    pub doc_kind: crate::meta::DocKind,
    /// The FULL merged domain vocabulary (data-catalog foundation A4):
    /// `snapshot.domains` UNION project-authored schema-import domains (A3's
    /// `merge_domains`) — computed ONCE here (0.3.0 T7 moved this from
    /// `check()`) so a caller that folds directly, like `lute-compile`, sees
    /// the SAME vocabulary without recomputing it or double-emitting
    /// `E-DOMAIN-DUP`.
    pub domains: std::collections::BTreeMap<String, Domain>,
}

/// Fold the analysis environment from an already-parsed document. Returns two
/// diagnostic streams kept SEPARATE so `check()` preserves its exact diagnostic
/// byte-order contract (a stable sort on `(byte_start, code)` makes same-span
/// ties order-sensitive): `.1` = the pre-import fold diags (meta + branch
/// dup/choice-dup) emitted just before import diags, and `.2` = the state-merge
/// diags (`E-EXTENDS-STATE-TYPE`/`E-STATE-REDECLARE`) emitted AFTER component
/// validation. Pure and total; never panics.
pub fn fold_env(
    doc: &Document,
    input: &CheckInput,
) -> (FoldedEnv, Vec<Diagnostic>, Vec<Diagnostic>) {
    // 3. Resolve the root document kind (dsl 0.2.0 §3.1) FIRST: it gates which
    //    per-kind frontmatter keys the meta parse below allows. Defaults to
    //    `Scene` — the degrade-safe path — when unresolved (missing/unknown
    //    `kind:`), so a mis-kinded doc still gets the scene-triad required-key
    //    treatment it had pre-0.2.0.
    let (resolved_kind, kind_diags) = crate::meta::resolve_doc_kind(&doc.meta);
    let has_body = !doc.shots.is_empty() || !doc.quests.is_empty();
    let (doc_kind, meta_kind, kind_diags) = match resolved_kind {
        Some(crate::meta::DocKind::Scene) => {
            (crate::meta::DocKind::Scene, crate::meta::MetaKind::Scene, kind_diags)
        }
        Some(crate::meta::DocKind::Quest) => {
            (crate::meta::DocKind::Quest, crate::meta::MetaKind::Quest, kind_diags)
        }
        None => match crate::meta::infer_meta_kind_from_shape(&doc.meta, has_body) {
            // A fragment opened standalone: validate in its import role, drop the
            // root-only E-KIND-MISSING/E-META-MISSING false positives. Any body
            // (e.g. a component's `## Scene`) still walks as `DocKind::Scene` —
            // the same degrade-safe shape as the genuine-missing-kind default.
            Some(mk) => (crate::meta::DocKind::Scene, mk, Vec::new()),
            None => (crate::meta::DocKind::Scene, crate::meta::MetaKind::Scene, kind_diags), // genuine missing kind
        },
    };

    // 3b. Typed frontmatter + inline state schema, dispatched by the resolved
    //     kind (dsl 0.2.0 §3.1, §6.1): a Quest doc carries none of the scene
    //     triad and rejects it as an unknown key.
    let (typed, mut fold_diags) = crate::meta::parse_meta_kind(&doc.meta, &input.snapshot, meta_kind);
    fold_diags.splice(0..0, kind_diags);

    // 3c. The FULL merged domain vocabulary (data-catalog foundation A4):
    //     `snapshot.domains` (A2 — core baseline + active-plugin `enums`)
    //     UNION project-authored domains lifted from this scene's schema
    //     imports (A3's `merge_domains`) — computed ONCE here (0.3.0 T7 moved
    //     this from `check()`) so `lute-compile` (which calls `fold_env`
    //     directly) sees the SAME vocabulary, never double-emitting
    //     `E-DOMAIN-DUP`. Then the merged, validated relational vocabulary
    //     (dsl 0.3.0 §3/§4): imports ∪ this document's inline
    //     `entities:`/`relations:`/`enums:`/`facts:`/`rules:`, every
    //     declaration checked (§3.1/§4) and every seed `facts:` entry
    //     validated via `check_atom` (D12 wildcard-in-seed included).
    let (domains, domain_diags) = merge_domains(&input.snapshot, &input.imports, doc.meta.span);
    let (mut vocab, rel_diags) =
        crate::rel_schema::build_rel_vocab(&input.imports, &typed, &domains, &doc.meta);
    fold_diags.extend(domain_diags);
    fold_diags.extend(rel_diags);
    // Per-rule Datalog checks (dsl 0.3.0 §7.1/§7.2, 0.3.0 T8): heads, body
    // atoms, safety — over the MERGED rule set. Runs here (not in `check()`)
    // so `lute-compile`'s direct `fold_env` caller sees the same diagnostics;
    // the vocab is still a plain local here (not yet frozen into `Env`'s
    // `Arc`), which Task 9's stratification/guard-taint pass relies on too.
    fold_diags.extend(crate::datalog_check::check_rules(&vocab, &domains));
    // Whole-rule-set graph analyses (dsl 0.3.0 §7.2/§6, 0.3.0 T9): negation-
    // cycle stratification + the guard-taint closure. Mutates `vocab` in
    // place (fills `guard_tainted`) BEFORE it is frozen into `Env`'s `Arc`
    // below — the one place in this pipeline `vocab` is still a plain local.
    fold_diags.extend(crate::datalog_check::check_stratification(&mut vocab));

    // 4. Fold every `<branch>`/`<hub>`'s implicit recording decls
    //    (`scene.choices.<id>` + a hub's per-choice `scene.visited.<id>.*`) into
    //    the schema BEFORE the checks that resolve against them (match subjects,
    //    CEL state paths). This pre-pass owns the episode-wide `E-DUP-BRANCH`
    //    detection (hub + branch ids share one domain) so the main walk never
    //    double-counts ids.
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
    fold_branches(doc, &mut schema, &mut seen_branches, &mut fold_diags);

    // 4a. Fold every `<quest>`'s implicit reserved `quest.<id>.*` decls (dsl
    //     0.2.0 §5.2) into the schema, threading a SEPARATE per-document `seen`
    //     id set (quest ids key the `quest.<id>.*` tier, a namespace distinct
    //     from `scene.choices.*`) so `E-QUEST-ID-DUP` fires exactly once per
    //     duplicate. `seen_quests` is SEEDED from `input.imports.imported_quest_ids`
    //     (dsl 0.2.0 §6.3: quest ids are unique PROJECT-WIDE, across the import
    //     graph, not merely within this document) — redeclaring an
    //     import-reachable id then fails the same `seen_quests.insert` check as
    //     an in-document repeat, reusing `E-QUEST-ID-DUP` unchanged. A collision
    //     BETWEEN two import-reachable docs that this document itself never
    //     redeclares is instead caught in `resolve_imports` directly (this
    //     document's own `<quest>` fold never sees it).
    //
    //     `quest.<id>.state` / `quest.<id>.objectives.<oid>.done` are RESERVED
    //     (dsl 0.2.0 §5.2/§9.3: "implicitly declared and MUST NOT be
    //     author-declared") — snapshot every state path that already exists
    //     BEFORE any reserved decl is folded (the author's inline `state:` and
    //     any imported schema, both merged above) so a reserved path that
    //     collides with one of THOSE is flagged (`E-QUEST-RESERVED-DECL`)
    //     instead of silently clobbered by `schema.decls.insert`. A collision
    //     with a path THIS loop itself already folded (the `E-QUEST-ID-DUP`
    //     repeat-id case, an identical decl either way) is NOT flagged — the
    //     snapshot is frozen before the loop starts, so a same-id repeat
    //     resolves against the pre-loop state and simply re-inserts the
    //     identical decl.
    let pre_existing_state: std::collections::BTreeSet<String> =
        schema.decls.keys().cloned().collect();
    let mut seen_quests: std::collections::BTreeSet<String> =
        input.imports.imported_quest_ids.keys().cloned().collect();
    for quest in &doc.quests {
        let record = check_quest(quest, &mut seen_quests);
        for (path, decl) in record.decls {
            if pre_existing_state.contains(&path) {
                fold_diags.push(Diagnostic {
                    code: "E-QUEST-RESERVED-DECL".to_string(),
                    severity: Severity::Error,
                    message: format!(
                        "state path `{path}` collides with an implicitly-declared reserved \
                         quest field (dsl 0.2.0 §5.2); it must not be author-declared in \
                         `state:`"
                    ),
                    span: doc.meta.span,
                    layer: Layer::Content,
                    fixits: Vec::new(),
                    provenance: None,
                });
            } else {
                schema.decls.insert(path, decl);
            }
        }
        fold_diags.extend(record.diags);
    }

    // 4b. Expand every active directive's `state.declares[]` into concrete state
    //     slots at each use site (plugin §8/§9): a `::minigame{resultKey="k"}`
    //     opens `scene.minigame.k.<field>` for each field of its shape. This runs
    //     before the walk + defassign so plugin-declared state resolves.
    fold_directive_slots(doc, &input.snapshot, &mut schema);

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

    // def name -> raw CEL body for the D4 expander. Same three sources and the
    // same precedence as `def_types`: plugin < imported < inline.
    let mut def_bodies: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();
    for (name, d) in &input.snapshot.defs {
        def_bodies.insert(name.clone(), d.cel.clone());
    }
    for (name, v) in &input.imports.defs {
        if let Some(c) = v.get("cel").and_then(|c| c.as_str()) {
            def_bodies.insert(name.clone(), c.to_string());
        }
    }
    for (name, v) in &typed.defs {
        if let Some(c) = v.get("cel").and_then(|c| c.as_str()) {
            def_bodies.insert(name.clone(), c.to_string());
        }
    }

    let env = Env {
        mode: input.mode,
        state: schema,
        defs,
        def_types,
        def_params,
        rel_vocab: std::sync::Arc::new(vocab),
    };
    (
        FoldedEnv {
            typed,
            env,
            def_bodies,
            doc_kind,
            domains,
        },
        fold_diags,
        state_merge_diags,
    )
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

    // 3–4b. Typed frontmatter + folded schema + merged def tables (one SoT:
    // the public fold_env accessor the compiler also consumes).
    let (folded, fold_diags, state_merge_diags) = fold_env(&doc, input);
    let env = &folded.env;
    let base_ctx = Ctx {
        env,
        in_match: false,
        match_subject: None,
    };

    // 4c. The FULL merged domain vocabulary a `{domain: X}`-typed attr
    // resolves against (data-catalog foundation A4): `snapshot.domains`
    // (A2 — core baseline + active-plugin `enums`) UNION project-authored
    // domains lifted from this scene's schema imports (A3's
    // `merge_domains`) — computed ONCE in `fold_env` (0.3.0 T7 moved this
    // out of `check()` so a direct `fold_env` caller, like `lute-compile`,
    // sees the identical vocabulary) and threaded by reference to every
    // `check_directive` call site (the scene walk, timeline clips, and
    // component bodies) below — never recomputed per-attr.
    let domains = &folded.domains;

    // 5. Per-node validator walk (directives / cel-slots / set / match / timeline).
    let mut walker = Walker {
        snapshot: &input.snapshot,
        providers: &input.providers,
        domains,
        arena: &arena,
        diags: Vec::new(),
        timeline_tables: Vec::new(),
        exhaustive_subject_spans: Vec::new(),
        components: &input.components,
    };
    // Kind-dispatched walk (dsl 0.2.0 §3.1): scene walks `doc.shots` (dsl
    // 0.1.0 grammar, unchanged); quest walks `doc.quests` — each quest's own
    // `start`/`fail` CEL guards (dsl 0.2.0 §6.3) are pure predicates the
    // engine derives `quest.<id>.state` from, so they get the SAME `Bool`
    // `check_cel_slot` treatment a `<when test>` guard gets — then the
    // quest's body, which recurses through the real `Node::On`/
    // `Node::Objective` walk arms below.
    match folded.doc_kind {
        crate::meta::DocKind::Scene => {
            for shot in &doc.shots {
                walker.walk(&shot.body, &base_ctx);
            }
        }
        crate::meta::DocKind::Quest => {
            for quest in &doc.quests {
                if let Some(start) = &quest.start {
                    walker.diags.extend(check_cel_slot(
                        start,
                        &arena,
                        &base_ctx,
                        Some(&ExpectedType::Bool),
                    ));
                }
                if let Some(fail) = &quest.fail {
                    walker.diags.extend(check_cel_slot(
                        fail,
                        &arena,
                        &base_ctx,
                        Some(&ExpectedType::Bool),
                    ));
                }
                walker.walk(&quest.body, &base_ctx);
            }
        }
    }

    // 6. Definite assignment, kind-dispatched (dsl 0.2.0 §4.4): scene runs
    //    ONCE over the whole concatenated shot stream (carry-forward #1 —
    //    `scene.*`/`run.*` persist across shots within the episode); quest
    //    runs PER `quest.body` — quest instances share no dominance relation
    //    with one another, so each quest's def-assignment is its own scope,
    //    never folded across quests.
    let defassign_diags: Vec<Diagnostic> = match folded.doc_kind {
        crate::meta::DocKind::Scene => {
            let all_nodes: Vec<Node> = doc
                .shots
                .iter()
                .flat_map(|s| s.body.iter().cloned())
                .collect();
            check_definite_assignment(&all_nodes, &env.state, &base_ctx)
        }
        crate::meta::DocKind::Quest => doc
            .quests
            .iter()
            .flat_map(|q| {
                // `start`/`fail` (dsl 0.2.0 §6.3) are evaluated at QUEST ENTRY —
                // nothing dominates them, so they get their own fresh
                // (empty-assigned-set) defassign check rather than folding into
                // `q.body`'s walk (which would wrongly let an in-body write
                // "prove" a guard evaluated strictly before the body runs).
                let mut ds = Vec::new();
                if let Some(start) = &q.start {
                    ds.extend(check_quest_guard_defassign(start, &env.state));
                }
                if let Some(fail) = &q.fail {
                    ds.extend(check_quest_guard_defassign(fail, &env.state));
                }
                ds.extend(check_definite_assignment(&q.body, &env.state, &base_ctx));
                ds
            })
            .collect(),
    };

    // 6b. Duplicate authored line codes (dsl §12): two `:line`s for the same
    //     speaker with the same trimmed `code` derive identical `lineId`/
    //     `voiceKey` join keys — a clean-check invariant the compile gate relies
    //     on. Whole-document, per-speaker; owns `E-DUP-LINE-CODE`.
    let line_code_diags = check_line_codes(&doc);

    // 7. Resolved view: injection fold + the timeline tables gathered in the walk.
    let mut inject_state = StageState::default();
    let mut injections = Vec::new();
    for shot in &doc.shots {
        fold_injections(&shot.body, &mut inject_state, &mut injections);
    }
    let inject_diags = std::mem::take(&mut inject_state.diags);
    // `node_summary` already covers `Node::On`/`Node::Objective` (Plan A), so
    // the quest arm reuses it verbatim — no wildcard, both surfaces summarized
    // identically.
    let commands_preview: Vec<String> = match folded.doc_kind {
        crate::meta::DocKind::Scene => doc
            .shots
            .iter()
            .flat_map(|s| s.body.iter().map(node_summary))
            .collect(),
        crate::meta::DocKind::Quest => doc
            .quests
            .iter()
            .flat_map(|q| q.body.iter().map(node_summary))
            .collect(),
    };

    // 8. Collect every diagnostic, then apply the ordering contract.
    let mut diags = Vec::new();
    diags.extend(parse_diags);
    diags.extend(cel_diags);
    diags.extend(fold_diags);
    // Rule-guard CEL firewall (dsl 0.3.0 §7.2/§7.3, D7, 0.3.0 T8): holds()/
    // count()/validAt()/now() inside a rule-body guard, plus the ordinary
    // profile/path-declaredness checks — after `base_ctx` (needs `ctx.env`)
    // is constructed above.
    diags.extend(check_rule_guards(&env.rel_vocab, &base_ctx));
    diags.extend(input.imports.diags.clone());
    // Component-import resolution diagnostics (dsl §13) + the per-component body
    // validation and `::use` expansion-cycle diagnostics, both reported at the
    // scene frontmatter span (a component file's own spans cannot be represented
    // in this document's diagnostic surface).
    diags.extend(input.components.diags.clone());
    diags.extend(validate_components(
        &input.components,
        &input.snapshot,
        &input.providers,
        domains,
        doc.meta.span,
    ));
    diags.extend(state_merge_diags);
    diags.extend(std::mem::take(&mut walker.diags));
    diags.extend(defassign_diags);
    diags.extend(line_code_diags);
    diags.extend(inject_diags);
    // Table-driven grammar admission (dsl 0.2.0 §3.3, §6.7): per-kind,
    // per-context construct legality. `E-GRAMMAR-NOT-ADMITTED` is semantic, NOT
    // a `STRUCTURAL_CODE` — the resolved view stays `Some` even when a document
    // uses a construct its kind forbids.
    diags.extend(crate::admission::check_admission(&doc, folded.doc_kind));

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
    /// The FULL merged domain vocabulary (data-catalog foundation A4):
    /// `snapshot.domains` UNION project-authored schema-import domains
    /// (A3's `merge_domains`), computed ONCE in `check()` and threaded here
    /// so `Type::Domain(name)` attrs resolve without recomputing the union.
    domains: &'a std::collections::BTreeMap<String, Domain>,
    arena: &'a CelArena,
    diags: Vec<Diagnostic>,
    timeline_tables: Vec<ResolvedTimeline>,
    /// Subject spans of domain-exhaustive `<match>`es, for the T4.4 suppression.
    exhaustive_subject_spans: Vec<Span>,
    /// Resolved `components:` table (dsl §13): the target of `::use` invocations.
    components: &'a ComponentSet,
}

impl Walker<'_> {
    fn walk(&mut self, nodes: &[Node], ctx: &Ctx<'_>) {
        for node in nodes {
            match node {
                Node::Line(l) => {
                    self.check_attr_refs(&l.attrs, ctx, None);
                    crate::content_line::check_content_line_attrs(
                        l,
                        self.snapshot,
                        self.providers,
                        self.domains,
                        &mut self.diags,
                    );
                    check_interps(&l.interps, ctx, &mut self.diags);
                }
                Node::Directive(d) if d.tag == "use" => {
                    // `use` is a reserved directive (dsl §13): recognized BEFORE
                    // the unknown-directive check, so it is never
                    // `E-UNKNOWN-DIRECTIVE`. It is a component invocation, not a
                    // snapshot directive.
                    check_use(d, self.components, ctx, &mut self.diags);
                    // `@ref`-valued args still resolve in the current scope; there
                    // is no directive decl to type them against.
                    self.check_attr_refs(&d.attrs, ctx, None);
                }
                Node::Directive(d) => {
                    self.diags.extend(check_directive(
                        d,
                        self.snapshot,
                        self.providers,
                        self.domains,
                        ctx,
                    ));
                    self.check_attr_refs(&d.attrs, ctx, Some(&d.tag));
                }
                Node::Set(s) => {
                    self.diags.extend(check_set(s, &ctx.env.state, ctx));
                    let expected = resolve_type(&s.path, &ctx.env.state)
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
                        // §7.6: a `<choice label>` string MAY embed `{{…}}`
                        // interpolations. Labels are String attrs, so their interps
                        // are not in the AST — scan and validate them via the same
                        // referent path as content lines (E-UNDECLARED /
                        // E-UNDECLARED-REF / E-REF-TYPE / §7.6 grammar).
                        check_interps(
                            &scan_label_interps(&choice.label, choice.span),
                            ctx,
                            &mut self.diags,
                        );
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
                    self.diags.extend(check_match(m, &ctx.env.state, ctx));
                    // The subject expression is evaluated OUTSIDE match scope: `$`
                    // is only valid in a `<when test>` (dsl §8.2), never in `on=`.
                    // Force `in_match=false` so a nested `<match on="$">` (whose
                    // incoming ctx has in_match=true from the enclosing arm) is
                    // correctly flagged E-DOLLAR-OUTSIDE-MATCH.
                    let subject_ctx = Ctx {
                        env: ctx.env,
                        in_match: false,
                        match_subject: None,
                    };
                    let subject_expected = resolve_type(&m.subject.raw, &subject_ctx.env.state)
                        .cloned()
                        .map(ExpectedType::Ty);
                    self.diags.extend(check_cel_slot(
                        &m.subject,
                        self.arena,
                        &subject_ctx,
                        subject_expected.as_ref(),
                    ));
                    if is_exhaustive(m, &ctx.env.state) {
                        self.exhaustive_subject_spans.push(m.subject.span);
                    }
                    // Arms (tests + bodies) evaluate WITHIN match scope: `$` binds
                    // to the subject (carry-forward #2).
                    let arm_ctx = Ctx {
                        env: ctx.env,
                        in_match: true,
                        match_subject: Some(m.subject.raw.clone()),
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
                                ClipNode::Directive(d) if d.tag == "use" => {
                                    check_use(d, self.components, ctx, &mut self.diags);
                                    self.check_attr_refs(&d.attrs, ctx, None);
                                }
                                ClipNode::Directive(d) => {
                                    self.diags.extend(check_directive(
                                        d,
                                        self.snapshot,
                                        self.providers,
                                        self.domains,
                                        ctx,
                                    ));
                                    self.check_attr_refs(&d.attrs, ctx, Some(&d.tag));
                                }
                                ClipNode::Set(s) => {
                                    self.diags.extend(check_set(s, &ctx.env.state, ctx));
                                    let expected = resolve_type(&s.path, &ctx.env.state)
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
                Node::Hub(h) => {
                    // `E-HUB-NO-EXIT` / `E-DUP-BRANCH` / `E-CHOICE-DUP` and the
                    // implicit `scene.choices.*` + `scene.visited.*` decl folding
                    // happened in the pre-pass (`fold_branches`); here we only
                    // validate the hub's own attrs and recurse into each choice
                    // (attrs, persist sugar, `when` guard, body) so the B1–B5
                    // node checks apply inside hub arms too (dsl §7.3.2).
                    self.check_attr_refs(&h.attrs, ctx, None);
                    for choice in &h.choices {
                        self.check_attr_refs(&choice.attrs, ctx, None);
                        check_choice_persist(choice, ctx, &mut self.diags);
                        // §7.6: hub choice labels carry `{{…}}` interpolations too
                        // (same as branch choices) — validate their referents.
                        check_interps(
                            &scan_label_interps(&choice.label, choice.span),
                            ctx,
                            &mut self.diags,
                        );
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
                Node::Objective(o) => {
                    // Completion predicate (dsl 0.2.0 §6.4): a value READ like a
                    // match subject — it doesn't gate the body (§6.3: `when`
                    // controls visibility, not the completion obligation), so it
                    // gets the SAME `Bool` `check_cel_slot` treatment a
                    // `<when test>` guard gets. `E-OBJECTIVE-MISSING-DONE` (an
                    // empty `done`) was already flagged by `check_quest`'s fold
                    // pass (Task 4); an empty raw's CEL `ast` stays `None`, so
                    // this pass is a no-op for a missing `done` beyond the
                    // `E-CEL-PARSE` `fill_document` already reported once.
                    self.diags.extend(check_cel_slot(
                        &o.done,
                        self.arena,
                        ctx,
                        Some(&ExpectedType::Bool),
                    ));
                    if let Some(when) = &o.when {
                        self.diags.extend(check_cel_slot(
                            when,
                            self.arena,
                            ctx,
                            Some(&ExpectedType::Bool),
                        ));
                    }
                    // §7.6: an objective `title` MAY embed `{{…}}` interpolations,
                    // same as a choice label.
                    if let Some(title) = &o.title {
                        check_interps(
                            &scan_label_interps(title, o.span),
                            ctx,
                            &mut self.diags,
                        );
                    }
                    self.check_attr_refs(&o.attrs, ctx, None);
                    self.walk(&o.body, ctx);
                }
                Node::On(o) => {
                    // ECA trigger (dsl 0.2.0 §4.1): the `event` name (a plain
                    // String, NOT CEL) resolves against the built-in lifecycle
                    // events + capability-declared world events; the `when`
                    // guard flows through the SAME `check_cel_slot` profile gate
                    // every other boolean guard gets.
                    self.diags
                        .extend(crate::on::check_on_event(o, self.snapshot));
                    if let Some(when) = &o.when {
                        self.diags.extend(check_cel_slot(
                            when,
                            self.arena,
                            ctx,
                            Some(&ExpectedType::Bool),
                        ));
                    }
                    self.check_attr_refs(&o.attrs, ctx, None);
                    self.walk(&o.body, ctx);
                }
                Node::Assert(a) => self
                    .diags
                    .extend(crate::fact_write::check_assert(a, self.domains, ctx)),
                Node::Retract(r) => self
                    .diags
                    .extend(crate::fact_write::check_retract(r, self.domains, ctx)),
            }
        }
    }

    /// Validate every `@ref`-valued attribute's CEL slot in the current scope.
    /// `directive_tag` is `Some` only for directive attrs, letting a `@ref` attr
    /// value be typed against the attr's declared type (`E-REF-TYPE`, dsl §8).
    fn check_attr_refs(&mut self, attrs: &[Attr], ctx: &Ctx<'_>, directive_tag: Option<&str>) {
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

/// Validate the `{{…}}` interpolation referents on a content line (dsl §7.6).
/// An interpolation is a state READ, so each referent gets the SAME cel-layer
/// treatment a `<when>` guard / `::set` RHS read gets: the referent is routed
/// through the shared [`check_cel_slot`] resolver, so a `Path` resolves against
/// the folded `state:` schema (`E-UNDECLARED`) and a `Ref` against `defs:`
/// (`E-UNDECLARED-REF`) exactly as a guard read does. A `Ref` additionally MUST
/// produce a **renderable** type (number/bool/enum, §7.6) — a declared def of any
/// other type is `E-REF-TYPE`. The reserved `userName` token always renders. The
/// definite-assignment half (`E-MAYBE-UNSET`, §9.4) is proven in the
/// path-sensitive `defassign` pass at the line's position — not here.
///
/// A free function (not a `Walker` method) so BOTH the scene walk and the
/// component-body walk (dsl §13) validate interps with their OWN `Env`: a scene's
/// `state:`/`defs:` or a component's `@param` namespace, respectively.
fn check_interps(interps: &[Interp], ctx: &Ctx<'_>, diags: &mut Vec<Diagnostic>) {
    // An interpolation is content `Text`, never a `<match>` arm test, so `$` is
    // out of scope there (dsl §8.2). Resolve in a forced NON-match context so a
    // `{{$}}` (which the parser classifies as a `Path` raw `"$"`) fires
    // `E-DOLLAR-OUTSIDE-MATCH`, regardless of any enclosing arm ctx the caller
    // threads in.
    let interp_ctx = Ctx {
        env: ctx.env,
        in_match: false,
        match_subject: None,
    };
    for interp in interps {
        // §7.6 grammar: an interp is EXACTLY a bare state `Path`, a def `Ref`
        // (`@name` / `@name(args)`), or the reserved `userName` — NOT an arbitrary
        // CEL expression (the ONLY CEL admitted inside `{{…}}` is a `@fn(args)`
        // argument). Enforce the grammar BEFORE CEL-validating, so a profile-valid
        // but interp-illegal body like `{{run.coins + 1}}` is rejected rather than
        // silently accepted as a generic slot. The `$` subject (parser-classified
        // as a `Path` raw `"$"`) is exempt here — the resolver owns its scope
        // diagnostic (`E-DOLLAR-OUTSIDE-MATCH`, §8.2), so let it flow through.
        let referent = match interp.kind {
            // The reserved player-name token always renders (dsl §7.6).
            InterpKind::Reserved => continue,
            InterpKind::Path => {
                let has_dollar = scan_refs(&interp.raw).iter().any(|r| r.is_dollar);
                if !has_dollar && !is_bare_state_path(&interp.raw) {
                    diags.push(interp_grammar_diag(&interp.raw, interp.span));
                    continue;
                }
                &interp.raw
            }
            InterpKind::Ref => {
                if !is_bare_ref(&interp.raw) {
                    diags.push(interp_grammar_diag(&interp.raw, interp.span));
                    continue;
                }
                &interp.raw
            }
        };
        // Reuse the guard/`::set` read-check verbatim: parse the referent as a
        // value-read CEL slot over the interpolation's span, then run the shared
        // resolver. `CelKind::AttrValue` is a non-guard read context — the
        // guard-only §9.6 `run.choiceLog` rule must not fire on a content
        // interpolation. A parse failure leaves `ast = None`, so the resolver
        // skips its AST pass and never double-reports malformed CEL.
        let mut arena = CelArena::default();
        let mut slot = CelSlot::raw(CelKind::AttrValue, referent.clone(), interp.span);
        if let Ok(handle) = parse_slot(&mut arena, &slot.raw, interp.span.byte_start) {
            slot.ast = Some(handle);
        }
        // Every cel-layer diagnostic here pertains to THIS interpolation; pin its
        // span to the whole `{{…}}` (matching the resolver's own state-path
        // fallback) so ref/arity/undeclared spans stay consistent and never shift
        // to the leading `{` from the interior-relative `scan_refs` offsets.
        for mut d in check_cel_slot(&slot, &arena, &interp_ctx, None) {
            d.span = interp.span;
            diags.push(d);
        }
        // §7.6 rendering: an interpolated `@ref` MUST resolve to a renderable
        // type (number/bool/enum). A DECLARED def whose produced type is known and
        // non-renderable is `E-REF-TYPE`; an undeclared ref already flagged
        // `E-UNDECLARED-REF` above (its name is absent from `def_types`, so this
        // never double-reports).
        if interp.kind == InterpKind::Ref {
            if let Some(name) = scan_refs(referent).into_iter().find(|r| !r.is_dollar).map(|r| r.name) {
                if let Some(ty) = interp_ctx.env.def_types.get(&name) {
                    if !is_renderable(ty) {
                        diags.push(Diagnostic {
                            code: "E-REF-TYPE".to_string(),
                            severity: Severity::Error,
                            message: format!(
                                "`@{name}` produces a non-renderable type; a `{{{{…}}}}` interpolation renders only number/bool/enum (dsl §7.6)"
                            ),
                            span: interp.span,
                            layer: Layer::Cel,
                            fixits: Vec::new(),
                            provenance: None,
                        });
                    }
                }
            }
        }
    }
}

/// §7.6 renderable types for an interpolated `@ref`: a **number** (shortest
/// decimal), a **bool** (`true`/`false`), or an **enum** (member text). Any other
/// produced type cannot render inside `{{…}}` and is a static error.
fn is_renderable(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Number | Type::Bool | Type::Enum(_) | Type::EnumFromOption(_)
    )
}

/// §7.6 interpolation-grammar violation: a `{{…}}` interior that is neither a
/// bare state path, a well-formed `@ref`/`@ref(args)`, nor `userName`. Reuses
/// [`E_CEL_PROFILE`] (the closed-CEL-surface code) — a bare CEL expression here
/// is CEL used where the profile does not admit it (§7.6: the only CEL inside
/// `{{…}}` is a `@fn(args)` argument).
fn interp_grammar_diag(raw: &str, span: Span) -> Diagnostic {
    Diagnostic {
        code: crate::cel_resolve::E_CEL_PROFILE.to_string(),
        severity: Severity::Error,
        message: format!(
            "interpolation `{raw}` is not a valid `{{{{…}}}}` form — only a state path, a \
             def `@ref` / `@ref(args)`, or `userName` are permitted; a bare CEL expression \
             is not (name a computed value with a `@def`, dsl §7.6)"
        ),
        span,
        layer: Layer::Cel,
        fixits: Vec::new(),
        provenance: None,
    }
}

/// `true` when `s` is a `CelIdent` (dsl §4.4): a leading `_`/ASCII-letter then
/// `_`/ASCII-alphanumerics. No `-` (CEL parses it as subtraction, §8.4). Empty
/// is not an ident.
fn is_cel_ident(s: &str) -> bool {
    let mut it = s.bytes();
    matches!(it.next(), Some(c) if c == b'_' || c.is_ascii_alphabetic())
        && it.all(|c| c == b'_' || c.is_ascii_alphanumeric())
}

/// `true` when `raw` (trimmed) is EXACTLY a bare dotted state path (dsl §7.6/§9.1):
/// a state-tier root (`scene`/`run`/`user`/`app`) followed by `.`-separated
/// `CelIdent` segments — no operators, whitespace, calls, or literals. `run.coins`
/// passes; `run.coins + 1`, `size(x)`, `foo.bar` (non-tier root) do not.
fn is_bare_state_path(raw: &str) -> bool {
    crate::cel_paths::is_state_path(raw) && raw.split('.').all(is_cel_ident)
}

/// `true` when `raw` (trimmed) is EXACTLY a `@name` or `@name(args)` reference
/// (dsl §7.6/§8.1): the OUTERMOST `@ref` starts at byte 0 and its bare/call group
/// reaches the end — nothing before or trailing. Nested `@ref`s inside a
/// `@fn(args)` argument are legitimate CEL (§8.1) and do not disqualify.
fn is_bare_ref(raw: &str) -> bool {
    scan_refs(raw).iter().any(|r| {
        !r.is_dollar
            && r.span.byte_start == 0
            && match r.call.as_ref() {
                None => r.span.byte_end == raw.len(),
                Some(c) => c.span.byte_end == raw.len(),
            }
    })
}

// --- Reusable content components (`::use`, dsl §13) ---

/// Component-invocation / body diagnostic codes (dsl §13).
const E_COMPONENT_UNDECLARED: &str = "E-COMPONENT-UNDECLARED";
const E_COMPONENT_ARG: &str = "E-COMPONENT-ARG";
const E_COMPONENT_CYCLE: &str = "E-COMPONENT-CYCLE";
const E_COMPONENT_BODY: &str = "E-COMPONENT-BODY";

/// Build a `Layer::Staging` diagnostic for a `::use` invocation / component body
/// (dsl §13).
fn use_diag(code: &str, message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity: Severity::Error,
        message,
        span,
        layer: Layer::Staging,
        fixits: Vec::new(),
        provenance: None,
    }
}

/// Validate a `::use{ component="name" <arg>=<value> … }` invocation (dsl §13)
/// against the resolved component table. `use` is reserved (recognized before the
/// unknown-directive check), so it is never `E-UNKNOWN-DIRECTIVE`.
///
/// * the `component=` attr must be a plain string naming a declared component
///   (`E-COMPONENT-UNDECLARED` when the name is absent from the table);
/// * every remaining attr is a NAMED arg bound to a param by name — an unknown
///   arg, a missing required param, or a value incompatible with its param's
///   declared type is `E-COMPONENT-ARG`.
fn check_use(
    dir: &Directive,
    components: &ComponentSet,
    ctx: &Ctx<'_>,
    diags: &mut Vec<Diagnostic>,
) {
    // E-AT-CONTEXT (dsl §7.5): a reserved `at` on a `::use` OUTSIDE a <track>
    // (track clips strip `at` via the parser). `::use` is dispatched here before
    // `check_directive`, so this shared check keeps `at` from being misread as a
    // component arg. Emitted regardless of the component's validity below.
    if let Some(d) = at_context(dir) {
        diags.push(d);
    }
    let Some(name_attr) = dir.attrs.iter().find(|a| a.key == "component") else {
        diags.push(use_diag(
            E_COMPONENT_ARG,
            "`::use` requires a `component` attribute naming the component (dsl §13)".to_string(),
            dir.span,
        ));
        return;
    };
    let AttrValue::Str(name) = &name_attr.value else {
        diags.push(use_diag(
            E_COMPONENT_ARG,
            "`::use` `component` must be a plain string naming the component (dsl §13)".to_string(),
            name_attr.value_span,
        ));
        return;
    };
    let Some(def) = components.table.get(name) else {
        diags.push(use_diag(
            E_COMPONENT_UNDECLARED,
            format!("unknown component `{name}`: not declared in `components:` (dsl §13)"),
            name_attr.value_span,
        ));
        return;
    };
    // Named-arg validation: each supplied arg binds to a param by name.
    // `at` is reserved (E-AT-CONTEXT above), never a component arg.
    for attr in dir
        .attrs
        .iter()
        .filter(|a| a.key != "component" && a.key != "at")
    {
        match def.params.iter().find(|(p, _)| p == &attr.key) {
            None => diags.push(use_diag(
                E_COMPONENT_ARG,
                format!(
                    "component `{name}` has no parameter `{}` (dsl §13)",
                    attr.key
                ),
                attr.span,
            )),
            Some((_, pty)) => {
                if !use_arg_ok(pty, &attr.value, ctx) {
                    diags.push(use_diag(
                        E_COMPONENT_ARG,
                        format!(
                            "argument `{}` to component `{name}` is not compatible with its declared type (dsl §13)",
                            attr.key
                        ),
                        attr.value_span,
                    ));
                }
            }
        }
    }
    // Every param must be supplied (v1 has no param defaults).
    for (p, _) in &def.params {
        if !dir.attrs.iter().any(|a| &a.key == p) {
            diags.push(use_diag(
                E_COMPONENT_ARG,
                format!("component `{name}` requires argument `{p}` (dsl §13)"),
                dir.span,
            ));
        }
    }
}

/// A `::use` arg value is compatible with its param type (dsl §13) when:
///
/// * it is a `@ref` (CEL, bound at expansion and typed in the ENCLOSING scene
///   scope) whose produced type — resolved via [`Env::def_types`] — is either
///   UNRESOLVABLE (not a known def: skipped, no false positive) or COMPATIBLE
///   with the param type (reusing the `E-REF-TYPE` [`compatible`] relation). A
///   ref whose def type is DEFINITELY incompatible flags `E-COMPONENT-ARG`;
/// * a value-level string for a `providerRef` param (id existence is out of the
///   v1 presentational scope); or
/// * a literal that coerces into the param type's domain and `type_accepts` it
///   (reusing the persist value-coercion helper).
fn use_arg_ok(ty: &Type, value: &AttrValue, ctx: &Ctx<'_>) -> bool {
    match value {
        AttrValue::Ref(slot) => match ref_produced_type(&slot.raw, ctx) {
            Some(produced) => compatible(produced, &ExpectedType::Ty(ty.clone())),
            None => true, // unresolvable ref — conservative, never flag
        },
        _ if matches!(ty, Type::ProviderRef(_)) => matches!(value, AttrValue::Str(_)),
        _ => persist_literal(ty, value).is_some_and(|lit| type_accepts(ty, &lit)),
    }
}

/// The produced [`Type`] of a whole-slot `@ref` arg (a `::use` value is only ever
/// a single `@name` or `@name(args)` — the attr parser captures nothing else),
/// resolved against the enclosing scope's [`Env::def_types`]. `None` when the
/// slot is not a resolvable def ref (e.g. the `$` subject, or a name with no
/// known produced type): conservatively skipped so no false positive fires.
fn ref_produced_type<'a>(raw: &str, ctx: &'a Ctx<'_>) -> Option<&'a Type> {
    let r = scan_refs(raw).into_iter().find(|r| !r.is_dollar)?;
    ctx.env.def_types.get(&r.name)
}

/// Validate every imported component (dsl §13): its presentational body plus the
/// `::use` expansion graph across components. Body diagnostics are re-anchored to
/// `at` (the scene frontmatter span) and prefixed with the component name/source
/// — a component file's own byte spans cannot be represented in this document's
/// diagnostic surface (mirroring how import diagnostics report at the scene
/// frontmatter). Deterministic: components iterate in name order.
fn validate_components(
    components: &ComponentSet,
    snapshot: &CapabilitySnapshot,
    providers: &ProviderSet,
    domains: &std::collections::BTreeMap<String, Domain>,
    at: Span,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for (name, def) in &components.table {
        let env = component_env(&def.params);
        let ctx = Ctx {
            env: &env,
            in_match: false,
            match_subject: None,
        };
        // Fill the component body's OWN CEL slots into a fresh arena (independent
        // of the scene's).
        let mut body = def.body.clone();
        let mut arena = CelArena::default();
        let cel_errors = fill_document(&mut arena, &mut body);
        let mut body_diags: Vec<Diagnostic> = cel_errors
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
        for shot in &body.shots {
            walk_component_body(
                &shot.body,
                snapshot,
                providers,
                domains,
                &arena,
                &ctx,
                components,
                &mut body_diags,
            );
        }
        for mut d in body_diags {
            d.message = format!("component `{name}` ({}): {}", def.src.display(), d.message);
            d.span = at;
            out.push(d);
        }
    }
    detect_use_cycles(components, at, &mut out);
    out
}

/// The component-mode analysis environment (dsl §13): the params are the ONLY ref
/// namespace (each a 0-arity `@param`), and the state schema is EMPTY so any
/// scene/run/user/app read in a body is undeclared.
fn component_env(params: &[(String, Type)]) -> Env {
    let mut defs = std::collections::BTreeSet::new();
    let mut def_types = std::collections::BTreeMap::new();
    let mut def_params = std::collections::BTreeMap::new();
    for (name, ty) in params {
        defs.insert(name.clone());
        def_types.insert(name.clone(), ty.clone());
        // 0 params: a bare `@p` is well-formed; `@p(x)` is `E-REF-ARITY`.
        def_params.insert(name.clone(), Vec::new());
    }
    Env {
        mode: Mode::Author,
        state: crate::meta::StateSchema::default(),
        defs,
        def_types,
        def_params,
        rel_vocab: std::sync::Arc::new(crate::rel_schema::RelVocab::default()),
    }
}

/// Walk a component body in component mode (dsl §13). Lines + staging directives
/// (incl. nested `::use`) are presentational and validated; a `::set` (state
/// write), `<branch>`/`<match>` (logic block), or `<timeline>` is the v1
/// presentational-scope error `E-COMPONENT-BODY`.
#[allow(clippy::too_many_arguments)]
fn walk_component_body(
    nodes: &[Node],
    snapshot: &CapabilitySnapshot,
    providers: &ProviderSet,
    domains: &std::collections::BTreeMap<String, Domain>,
    arena: &CelArena,
    ctx: &Ctx<'_>,
    components: &ComponentSet,
    diags: &mut Vec<Diagnostic>,
) {
    for node in nodes {
        match node {
            Node::Line(l) => {
                body_attr_refs(&l.attrs, snapshot, arena, ctx, None, diags);
                // `{{@param}}` / `{{run.x}}` in a component body are referents too
                // (dsl §7.6, §13): resolve against the component `@param` env in
                // `ctx` — an undeclared ref is `E-UNDECLARED-REF`, any state read is
                // `E-UNDECLARED` (a component body has an empty `state:` schema).
                check_interps(&l.interps, ctx, diags);
            }
            Node::Directive(d) if d.tag == "use" => {
                check_use(d, components, ctx, diags);
                body_attr_refs(&d.attrs, snapshot, arena, ctx, None, diags);
            }
            Node::Directive(d) => {
                diags.extend(check_directive(d, snapshot, providers, domains, ctx));
                body_attr_refs(&d.attrs, snapshot, arena, ctx, Some(&d.tag), diags);
            }
            Node::Set(s) => diags.push(use_diag(
                E_COMPONENT_BODY,
                format!(
                    "a component body must be presentational (dsl §13): `::set` of `{}` writes state, not allowed in v1",
                    s.path
                ),
                s.span,
            )),
            Node::Branch(b) => diags.push(use_diag(
                E_COMPONENT_BODY,
                format!(
                    "a component body must be presentational (dsl §13): the `<branch {}>` logic block is not allowed in v1",
                    b.id
                ),
                b.span,
            )),
            Node::Match(m) => diags.push(use_diag(
                E_COMPONENT_BODY,
                "a component body must be presentational (dsl §13): a `<match>` logic block is not allowed in v1".to_string(),
                m.span,
            )),
            Node::Timeline(tl) => diags.push(use_diag(
                E_COMPONENT_BODY,
                "a component body must be presentational (dsl §13): a `<timeline>` is not allowed in v1".to_string(),
                tl.span,
            )),
            Node::Hub(h) => diags.push(use_diag(
                E_COMPONENT_BODY,
                "a component body must be presentational (dsl §13): a `<hub>` logic block is not allowed in v1".to_string(),
                h.span,
            )),
            Node::Objective(o) => diags.push(use_diag(
                E_COMPONENT_BODY,
                "a component body must be presentational (dsl §13): an `<objective>` logic block is not allowed in v1".to_string(),
                o.span,
            )),
            Node::On(o) => diags.push(use_diag(
                E_COMPONENT_BODY,
                "a component body must be presentational (dsl §13): an `<on>` logic block is not allowed in v1".to_string(),
                o.span,
            )),
            Node::Assert(a) => diags.push(use_diag(
                E_COMPONENT_BODY,
                format!(
                    "a component body must be presentational (dsl §13): `::assert` of `{}` writes state, not allowed in v1",
                    a.pattern.relation
                ),
                a.span,
            )),
            Node::Retract(r) => diags.push(use_diag(
                E_COMPONENT_BODY,
                format!(
                    "a component body must be presentational (dsl §13): `::retract` of `{}` writes state, not allowed in v1",
                    r.pattern.relation
                ),
                r.span,
            )),
        }
    }
}

/// Validate the `@ref`-valued attrs of a component-body node against the param
/// ref namespace (the free-function analog of [`Walker::check_attr_refs`]).
fn body_attr_refs(
    attrs: &[Attr],
    snapshot: &CapabilitySnapshot,
    arena: &CelArena,
    ctx: &Ctx<'_>,
    directive_tag: Option<&str>,
    diags: &mut Vec<Diagnostic>,
) {
    for attr in attrs {
        if let AttrValue::Ref(slot) = &attr.value {
            let expected = directive_tag
                .and_then(|tag| snapshot.directive(tag))
                .and_then(|decl| decl.attrs.iter().find(|a| a.name == attr.key))
                .map(|a| ExpectedType::Ty(a.ty.clone()));
            diags.extend(check_cel_slot(slot, arena, ctx, expected.as_ref()));
        }
    }
}

/// The component NAME a `::use` directive targets (a plain-string `component=`
/// attr), or `None` for any other directive / a non-string component attr.
fn use_target(dir: &Directive) -> Option<&str> {
    if dir.tag != "use" {
        return None;
    }
    dir.attrs
        .iter()
        .find(|a| a.key == "component")
        .and_then(|a| match &a.value {
            AttrValue::Str(s) => Some(s.as_str()),
            _ => None,
        })
}

/// Collect the component NAMES a body `::use`s, recursing into nested bodies for
/// robustness (a presentational body flags nested logic separately, but a `::use`
/// there still forms an expansion edge for cycle detection).
fn collect_use_targets(nodes: &[Node], out: &mut Vec<String>) {
    for node in nodes {
        match node {
            Node::Directive(d) => {
                if let Some(t) = use_target(d) {
                    out.push(t.to_string());
                }
            }
            Node::Branch(b) => {
                for c in &b.choices {
                    collect_use_targets(&c.body, out);
                }
            }
            Node::Match(m) => {
                for arm in &m.arms {
                    match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => {
                            collect_use_targets(body, out)
                        }
                    }
                }
            }
            Node::Timeline(tl) => {
                for tr in &tl.tracks {
                    for clip in &tr.clips {
                        if let ClipNode::Directive(d) = &clip.node {
                            if let Some(t) = use_target(d) {
                                out.push(t.to_string());
                            }
                        }
                    }
                }
            }
            Node::Hub(h) => {
                for c in &h.choices {
                    collect_use_targets(&c.body, out);
                }
            }
            Node::Line(_)
            | Node::Set(_)
            | Node::Objective(_)
            | Node::On(_)
            | Node::Assert(_)
            | Node::Retract(_) => {}
        }
    }
}

/// Detect `::use` expansion cycles across component bodies (dsl §13): component A
/// whose body `::use`s B whose body … `::use`s A. Reported once per back edge as
/// `E-COMPONENT-CYCLE` at `at`. Deterministic: adjacency + neighbors are sorted.
fn detect_use_cycles(components: &ComponentSet, at: Span, diags: &mut Vec<Diagnostic>) {
    let mut adj: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for (name, def) in &components.table {
        let mut targets = Vec::new();
        for shot in &def.body.shots {
            collect_use_targets(&shot.body, &mut targets);
        }
        targets.sort();
        targets.dedup();
        adj.insert(name.clone(), targets);
    }
    let mut on_stack = std::collections::BTreeSet::new();
    let mut done = std::collections::BTreeSet::new();
    let mut stack: Vec<String> = Vec::new();
    for start in adj.keys() {
        if !done.contains(start) && !on_stack.contains(start) {
            dfs_use_cycle(start, &adj, &mut on_stack, &mut done, &mut stack, at, diags);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn dfs_use_cycle(
    node: &str,
    adj: &std::collections::BTreeMap<String, Vec<String>>,
    on_stack: &mut std::collections::BTreeSet<String>,
    done: &mut std::collections::BTreeSet<String>,
    stack: &mut Vec<String>,
    at: Span,
    diags: &mut Vec<Diagnostic>,
) {
    on_stack.insert(node.to_string());
    stack.push(node.to_string());
    if let Some(targets) = adj.get(node) {
        for nbr in targets {
            if on_stack.contains(nbr) {
                let start_idx = stack.iter().position(|p| p == nbr).unwrap_or(0);
                let chain = stack[start_idx..]
                    .iter()
                    .cloned()
                    .chain(std::iter::once(nbr.clone()))
                    .collect::<Vec<_>>()
                    .join(" -> ");
                diags.push(use_diag(
                    E_COMPONENT_CYCLE,
                    format!("`::use` expansion cycle across components: {chain} (dsl §13)"),
                    at,
                ));
            } else if !done.contains(nbr) {
                dfs_use_cycle(nbr, adj, on_stack, done, stack, at, diags);
            }
        }
    }
    stack.pop();
    on_stack.remove(node);
    done.insert(node.to_string());
}

/// Diagnostic codes for the `<choice … persist="run">` run-fact sugar (dsl §11.1.1).
const E_PERSIST_TARGET: &str = "E-PERSIST-TARGET";
const E_PERSIST_MISSING_INTO: &str = "E-PERSIST-MISSING-INTO";
const E_PERSIST_VALUE: &str = "E-PERSIST-VALUE";
const E_PERSIST_CONFLICT: &str = "E-PERSIST-CONFLICT";

/// Validate a `<choice>`'s run-fact promotion sugar (dsl §11.1.1):
/// `persist="run" into="run.<path>" [value="<lit>"]` records a NAMED, declared
/// `run.*` fact when the choice is selected. The sugar is EXACTLY a
/// `::set{run.<path> = <value>}` appended to the arm — the engine materializes
/// the write, so the checker only validates well-formedness. A `<choice>` with
/// no `persist` attr is untouched. The `persist`/`into`/`value` attrs are
/// recognized here, so they are never reported as unknown/extra. (The persist
/// target attribute is `into`, renamed from 0.0.1 `as`; `as` survives only on
/// content lines as the display-label override, §7.1 — untouched here.)
fn check_choice_persist(choice: &Choice, ctx: &Ctx<'_>, diags: &mut Vec<Diagnostic>) {
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
    // Rule 2: `into` is REQUIRED and MUST name a `run.*` path.
    let Some(into_attr) = choice.attrs.iter().find(|a| a.key == "into") else {
        diags.push(persist_diag(
            E_PERSIST_MISSING_INTO,
            "`persist=\"run\"` requires `into=\"run.<path>\"` naming the run fact to record \
             (dsl §11.1.1)"
                .to_string(),
            choice.span,
        ));
        return;
    };
    let Some(into_path) = str_attr(into_attr) else {
        diags.push(persist_diag(
            E_PERSIST_TARGET,
            "`into` must be a `run.<path>` string literal (dsl §11.1.1)".to_string(),
            choice.span,
        ));
        return;
    };
    if into_path
        .strip_prefix("run.")
        .filter(|rest| !rest.is_empty())
        .is_none()
    {
        diags.push(persist_diag(
            E_PERSIST_TARGET,
            format!(
                "`into=\"{into_path}\"` must name a `run.<path>` fact (a bare `run` or a \
                 non-`run.*` path is not a valid run target) (dsl §11.1.1)"
            ),
            choice.span,
        ));
        return;
    }
    // Rule 3 (§9.2/§11.1.1): the `into` path MUST already be declared in the
    // merged schema — a typo'd/undeclared `into` cannot silently create a field.
    let Some(ty) = resolve_type(into_path, &ctx.env.state) else {
        diags.push(persist_diag(
            "E-UNDECLARED",
            format!(
                "persist target `{into_path}` is not declared in the run schema (dsl §11.1.1); \
                 an undeclared/typo'd `into` cannot create a field"
            ),
            choice.span,
        ));
        return;
    };
    // Rule 4 (§11.1.1): the `value` policy depends on the declared type.
    check_persist_value(
        ty,
        choice.attrs.iter().find(|a| a.key == "value"),
        into_path,
        choice.span,
        diags,
    );
    // Rule 5: the arm must not already `::set` the same path — the persist write
    // would duplicate it.
    if choice
        .body
        .iter()
        .any(|n| matches!(n, Node::Set(s) if s.path == into_path))
    {
        diags.push(persist_diag(
            E_PERSIST_CONFLICT,
            format!(
                "the choice arm already `::set`s `{into_path}`, which `persist=\"run\" \
                 into=\"{into_path}\"` also writes (dsl §11.1.1)"
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
    into_path: &str,
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
                            "`value` for the bool path `{into_path}` must be a bool literal \
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
                    "`value` is required for `{into_path}` (only a `bool` path defaults to \
                     `true`) (dsl §11.1.1)"
                ),
                span,
            )),
            Some(v) if !accepted(v) => diags.push(persist_diag(
                E_PERSIST_VALUE,
                format!(
                    "`value` is not compatible with the declared type of `{into_path}` \
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

/// Pre-pass: fold every `<branch>`'s and `<hub>`'s implicit recording decls
/// (`scene.choices.<id>`, plus a hub's per-choice `scene.visited.<id>.<choiceId>`)
/// into `schema` in document order, threading the episode-wide `seen` id set
/// (branch + hub ids share it) so `E-DUP-BRANCH` fires exactly once per duplicate.
/// Recurses into nested bodies (a branch/hub may live inside a match arm or
/// another choice body). A `<branch>` is ALSO admitted directly inside a quest
/// body (dsl 0.2.0 §6.7), reached via `<quest>` top level or an `<on>`/
/// `<objective>` arm — `doc.quests` is folded here too so a quest `<branch>`
/// gets the SAME `E-CHOICE-DUP`/implicit-decl treatment a scene one gets
/// (`<hub>` is never legal in a quest doc, dsl 0.2.0 §6.7 — grammar admission
/// rejects it separately, so it is never reached from a quest walk in
/// practice, but the recursion below tolerates it structurally regardless).
fn fold_branches(
    doc: &Document,
    schema: &mut crate::meta::StateSchema,
    seen: &mut std::collections::BTreeSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    for shot in &doc.shots {
        fold_branches_nodes(&shot.body, schema, seen, diags);
    }
    for quest in &doc.quests {
        fold_branches_nodes(&quest.body, schema, seen, diags);
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
            Node::Hub(h) => {
                // Hub ids share the branch uniqueness domain (`seen`); fold the
                // implicit `scene.choices.<hubId>` + `scene.visited.<hubId>.*`
                // decls, then recurse into hub arms for any nested branch/hub/
                // match (dsl §7.3.2, §11.1.3).
                let rec = check_hub(h, seen);
                for (path, decl) in rec.decls {
                    schema.decls.insert(path, decl);
                }
                diags.extend(rec.diags);
                for choice in &h.choices {
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
            // Quest-only arms (dsl 0.2.0 §4, §6.4): a `<branch>`/`<match>` may
            // live directly inside an `<on>` event arm or an `<objective>`
            // body (grammar admission's `Emittable` context), so the fold
            // recurses through them too.
            Node::On(o) => fold_branches_nodes(&o.body, schema, seen, diags),
            Node::Objective(o) => fold_branches_nodes(&o.body, schema, seen, diags),
            Node::Line(_) | Node::Directive(_) | Node::Set(_) | Node::Timeline(_) => {}
            Node::Assert(_) | Node::Retract(_) => {}
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
    for quest in &doc.quests {
        fold_slots_nodes(&quest.body, snapshot, schema);
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
            Node::Hub(h) => {
                for c in &h.choices {
                    fold_slots_nodes(&c.body, snapshot, schema);
                }
            }
            // Quest-only arms (dsl 0.2.0 §4, §6.4): a directive-opening slot
            // (e.g. `::minigame{resultKey="k"}`) may be used directly inside
            // an `<on>` event arm or an `<objective>` body — recurse so its
            // declared state slots open for the quest walk + defassign, same
            // as a scene shot's directives do.
            Node::On(o) => fold_slots_nodes(&o.body, snapshot, schema),
            Node::Objective(o) => fold_slots_nodes(&o.body, snapshot, schema),
            Node::Line(_) | Node::Set(_) => {}
            Node::Assert(_) | Node::Retract(_) => {}
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
/// starts with a state tier (`scene.`/`run.`/`user.`/`app.`/`quest.`) —
/// `set_op` also quotes `::set` first, so we scan for the tier-prefixed token,
/// not just the first quote. `None` (no tier token) falls back to span-only
/// collapse.
fn undeclared_path(message: &str) -> Option<&str> {
    message.split('`').find(|tok| {
        tok.starts_with("scene.")
            || tok.starts_with("run.")
            || tok.starts_with("user.")
            || tok.starts_with("app.")
            || tok.starts_with("quest.")
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
        Node::Line(l) => format!(":{}", l.speaker),
        Node::Directive(d) => format!("::{}", d.tag),
        Node::Set(s) => format!("::set{{{} {} …}}", s.path, s.op),
        Node::Branch(b) => format!("<branch id=\"{}\"> ({} choices)", b.id, b.choices.len()),
        Node::Match(m) => format!("<match on=\"{}\"> ({} arms)", m.subject.raw, m.arms.len()),
        Node::Timeline(tl) => format!("<timeline> ({} tracks)", tl.tracks.len()),
        Node::Hub(h) => format!("<hub> ({} choices)", h.choices.len()),
        Node::On(o) => format!("<on event=\"{}\">", o.event),
        Node::Objective(o) => format!("<objective id=\"{}\">", o.id),
        Node::Assert(a) => format!("::assert{{{}(…)}}", a.pattern.relation),
        Node::Retract(r) => format!("::retract{{{}(…)}}", r.pattern.relation),
    }
}
