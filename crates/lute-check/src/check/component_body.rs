//! Component-body validation (dsl §13, 0.4.0 §6): [`validate_components`], the
//! component-mode walk and its purity scans, and `::use` cycle detection.

use super::*;

/// Validate every component this document `::use`s (dsl §13), directly or
/// through another component's body: its presentational body plus the
/// `::use` expansion graph across components. Body diagnostics are
/// re-anchored to the FIRST `::use` in this document that brings the body in
/// (`use_sites`, [`component_use_sites`]) and prefixed with the component
/// name and its project-relative source path: a component file's own byte
/// spans cannot be represented in this document's diagnostic surface, so the
/// position inside the component rides along as a `related` entry, with its
/// line/column resolved against the component's own text. A component this
/// document imports but never `::use`s (e.g. one every document gets through
/// `defaults: components:`) contributes nothing here: its body is not this
/// document's fault, and the component file's own check reports it
/// (round-5 T3-4). Deterministic: components iterate in name order. `host`
/// is the importing document's env and `host_params` its own params (when
/// it is itself a component): a body reading one of `host`'s defs is
/// [`component_def_reads`]' report.
///
/// Each body ALSO gets its own isolated run of the whole-document
/// duplicate-line-code pass ([`check_line_codes`], dsl §12) — see the comment
/// at that call for the scope boundary.
pub(super) fn validate_components(
    components: &ComponentSet,
    snapshot: &CapabilitySnapshot,
    providers: &ProviderSet,
    domains: &std::collections::BTreeMap<String, Domain>,
    at: Span,
    use_sites: &std::collections::BTreeMap<String, Span>,
    cast: &std::collections::BTreeMap<String, lute_manifest::schema::CastMember>,
    host: &Env,
    host_params: &[lute_manifest::schema::DefParam],
) -> Vec<(PathBuf, Diagnostic)> {
    let mut out = Vec::new();
    for (name, def) in &components.table {
        let Some(&site) = use_sites.get(name) else {
            continue;
        };
        // dsl 0.24.0 §4: a `speaker` param is this host's cast ids.
        let params = crate::component_effects::host_param_types(&def.params, &def.speakers, cast);
        let env = component_env(&params);
        let ctx = Ctx {
            env: &env,
            in_match: false,
            match_subject: None,
        };
        // dsl 0.4.0 §6.3 (T7): the param-domain table this component's
        // admitted `<match>`es dispatch over, built ONCE from `def.params`
        // and threaded through every recursive `walk_component_body` call
        // so a nested arm's `<match>` sees every sibling param, not just
        // its own enclosing one.
        let param_domains: std::collections::BTreeMap<String, DomainInfo> = params
            .iter()
            .map(|(pname, ty)| (pname.clone(), param_domain(ty, domains)))
            .collect();
        let body_scope = BodyScope {
            effects: def.effects,
            speakers: &def.speakers,
            cast,
            own: component_own_slots(&def.body, snapshot, components),
        };
        // Fill the component body's OWN CEL slots into a fresh arena (independent
        // of the scene's).
        let mut body = def.body.clone();
        let mut arena = CelArena::default();
        let cel_errors = fill_document(&mut arena, &mut body);
        let mut body_diags: Vec<Diagnostic> = cel_parse_diagnostics(&body, cel_errors);
        for shot in &body.shots {
            walk_component_body(
                &shot.body,
                snapshot,
                providers,
                domains,
                &arena,
                &ctx,
                components,
                &param_domains,
                &body_scope,
                &mut body_diags,
            );
        }
        // A read of a host def: the one report the component's own check
        // gives too, standing for the undeclared-ref one its params-only env
        // raised at the same read.
        let reads = component_def_reads(&body, &param_domains, host, host_params);
        body_diags.retain(|d| {
            d.code != "E-UNDECLARED-REF"
                || !reads.iter().any(|r| r.span.byte_start == d.span.byte_start)
        });
        body_diags.extend(reads);
        // Task 7f, the SAME class as Task 7b/7c/7e — but one level out: not a
        // rule that skipped the body, a whole SURFACE nothing walked.
        // `check_admission` has exactly ONE callsite (`check()` step 8, over
        // the ROOT document) and the loop above iterates `body.shots` only, so
        // a component file's `doc.quests` reached NEITHER pass. A top-level
        // `<quest>` therefore errored `E-GRAMMAR-NOT-ADMITTED` when that file
        // was checked STANDALONE yet checked CLEAN through a `::use`, and
        // lowering then dropped the entire declaration — a `::set` state write
        // inside it included — without a word. `check_component_toplevel` owns
        // the general rule (content present at a component document's top level
        // but processed by nothing is an error, never a silent drop) and an
        // EXHAUSTIVE `Document` destructuring that fails to compile if a future
        // field could reintroduce the same hole; see its doc comment for the
        // per-field verdicts. Landed in `body_diags` so it inherits the
        // component re-anchoring below, matching the standalone check's code.
        body_diags.extend(crate::admission::check_component_toplevel(&body));
        // Task 7c, the SAME class of gap as Task 7b's (the content-line attr
        // checker) one arm up: `check_line_codes` had exactly ONE callsite —
        // `check()` step 6, over the ROOT document — so a component body never
        // reached it. Two identical `(speaker, code)` content lines inside a
        // body therefore passed `lute check` through a `::use` (exit 0) while
        // the same pair errored `E-DUP-LINE-CODE` at scene level AND when the
        // component was checked standalone, and `lute compile` went on to emit
        // two records carrying one `lineId` — the voice-key / i18n identity
        // spine, so two lines collapsing onto one id collide on one voice
        // asset.
        //
        // Reused verbatim rather than re-implemented: `check_line_codes` is
        // already a whole-`&Document` pass and `body` IS a `Document`, so
        // pointing it at the body needs no narrower entry point. Its own
        // identity scoping (dsl 0.2.0 §7 — one scope per document for shots,
        // one per `<quest>`) then makes each component body its own scope for
        // free, exactly as each quest is.
        //
        // SCOPE: this checks uniqueness WITHIN one body, which is the whole
        // question: since dsl 0.22.0 §11 each expansion is addressed under
        // its own `{prefix}.{component}#{n}` scope, so a component `::use`d
        // twice, or sharing a code with a host line, never shares a `lineId`.
        body_diags.extend(check_line_codes(&body));
        // Task 7e, the THIRD instance of the same class as Task 7b's
        // (content-line attrs) and Task 7c's (duplicate line codes):
        // `check_reachability` had exactly ONE callsite — `check()` step 8,
        // over the ROOT document. `walk_component_body` called
        // `check_match_reach` for `Node::Match` alone, so everything else the
        // §5.2/§5.3 pass owns escaped inside a component body: a content
        // line's `when=` guard was never reachability-checked, and content
        // following an allowed `::end` never flagged. A component whose line
        // carried a provably-false guard therefore checked CLEAN through a
        // `::use` (exit 0) while the identical line errored `E-ARM-DEAD` at
        // scene level AND when that same component file was checked
        // STANDALONE — the toolchain contradicting itself about one file
        // depending only on how it was reached.
        //
        // ENV SEAM (the one decision here): `check_reachability` needs a
        // resolution environment, and no `FoldedEnv` exists for a component
        // body (`ComponentDef` carries `params`/`body`/`src` only). Rather
        // than manufacture a second one — which is exactly how a NEW
        // divergence gets built while closing an old one — the pass was split
        // at its own seam: `check_reachability_in` takes the `DefTable` +
        // base `DecideCtx` its `FoldedEnv` entry point resolves into, and
        // BOTH the STANDALONE component self-check (reachability.rs's
        // `folded.typed.component` branch) and this call hand over the SAME
        // shape — `param_domain(ty)` over the component's own `params:` list,
        // an EMPTY `bodies` table (a component file has no frontmatter
        // `defs:`; a bodiless `@ref` marker resolves via `params`, D3), the
        // component's own (empty) state schema, and `dollar: None`. The two
        // paths therefore agree by construction, not by coincidence.
        //
        // This is now the SOLE owner of component-body reachability: the
        // `check_match_reach` call that used to sit in
        // `walk_component_body`'s `Node::Match` arm was removed, since this
        // whole-body walk reaches every `<match>` it did and more.
        let reach_bodies: std::collections::BTreeMap<String, String> =
            std::collections::BTreeMap::new();
        let reach_defs = DefTable {
            bodies: &reach_bodies,
            params: &env.def_params,
        };
        let reach_ctx = DecideCtx {
            schema: &env.state,
            dollar: None,
            params: &param_domains,
            facts: None,
        };
        body_diags.extend(crate::reachability::check_reachability_in(
            &body,
            &reach_defs,
            &reach_ctx,
            &crate::reachability::ReachEnv {
                def_types: &env.def_types,
                beat_when: None,
                snapshot: None,
                folded: None,
            },
        ));
        // D6 (dsl 0.4.0 §6.2): the positive `E-COMPONENT-STATE` scan
        // (`component_slot_state_scan`/`component_interp_scan`) is the
        // AUTHORITATIVE diagnosis for an ambient-state read inside a
        // component body. The SAME site also reaches the ordinary
        // `check_cel_slot`/`check_directive` pipeline (run unconditionally so
        // ref/arity/profile checks still apply) against the EMPTY component
        // `state:`/`RelVocab` (`component_env`), which incidentally
        // misreports it as `E-UNDECLARED`/`E-RELATION-UNKNOWN` — a path that
        // may be perfectly declared in the CONSUMING scene. Drop those two
        // incidental codes; `E-UNDECLARED-REF` (an unknown `@param`) is a
        // real defect and stays untouched (D6).
        body_diags.retain(|d| {
            d.code != "E-UNDECLARED" && d.code != crate::rel_schema::E_RELATION_UNKNOWN
        });
        // The component's own text resolves each inner span's line/column: a
        // CEL-slot span carries byte offsets only (`normalize_spans` never
        // walks `related`), which printed as `0:0`.
        let src_text = (!body_diags.is_empty())
            .then(|| std::fs::read_to_string(&def.src).ok())
            .flatten();
        let src_index = src_text.as_deref().map(lute_core_span::TextIndex::new);
        for mut d in body_diags {
            // dsl 0.10.0 §9 rule 1: the primary anchor stays at the caller with
            // the component prefix (0.9.0 §5) — for the injection case it is the
            // honest anchor, because the verdict depends on state inherited
            // there — but the position INSIDE the component is preserved as a
            // secondary location. `RelatedDiagnostic` is the codebase's only
            // cross-file attribution and it already renders as an indented
            // sub-line (`lute-cli`'s `render_diagnostics`), so this needs no new
            // surface. 0.9.0 §6.2 named the collapse a known limitation; this
            // narrows it rather than removing it, because `Diagnostic` still has
            // no file field of its own.
            //
            // The line/column ARE the component's own: these diagnostics were
            // produced against `def.body`, parsed from the component file's text
            // by `component_import`, so the parser's positions are already
            // component-relative — and `check()`'s `normalize_spans` walks
            // `d.span` and `d.fixits` only, never `related`, so nothing later
            // re-derives them against the importing document's `TextIndex`.
            let inner = Diagnostic {
                code: d.code.clone(),
                severity: d.severity,
                message: d.message.clone(),
                span: match &src_index {
                    Some(idx) => Span::from_bytes(idx, d.span.byte_start, d.span.byte_end),
                    None => d.span,
                },
                layer: d.layer,
                fixits: Vec::new(),
                provenance: None,
                covered: Vec::new(),
                related: Vec::new(),
            };
            d.message = format!(
                "component `{name}` ({}): {}",
                project_relative_display(&def.src),
                d.message
            );
            // 0.21.1 T3-7 (lamplight F4): the `::use` that brings this body in
            // is the one position in THIS document that caused the fault, and
            // where an editor should land — not the frontmatter's 1:1.
            d.span = site;
            // T13: a CEL-parse fixit's edit span (if any) is in the COMPONENT
            // file's own byte-space — this document's diagnostic surface
            // cannot represent it (same reason the span itself collapses to
            // `at` above), so it is dropped rather than shipped pointing at
            // the wrong document.
            d.fixits.clear();
            d.related.push(lute_core_span::RelatedDiagnostic {
                file: def.src.display().to_string(),
                diagnostic: inner,
            });
            out.push((def.src.clone(), d));
        }
    }
    // `detect_use_cycles`' output names no single component file (a cycle
    // spans several) — paired with an empty `PathBuf` so
    // `merge_component_body_diags` (dsl 0.5.1 §4) can never mistake it for a
    // real component's own `E-COMPONENT-PARSE` (whose `related[].file` is
    // always a real canonical path).
    let mut cycle_diags = Vec::new();
    detect_use_cycles(components, at, &mut cycle_diags);
    out.extend(cycle_diags.into_iter().map(|d| (PathBuf::new(), d)));
    out
}

/// For every component this document reaches through `::use`, the span of the
/// FIRST `::use` in THIS document that brings it in (document order: shots,
/// then quests, then entries). A component reached only through another
/// component's body inherits the outer `::use`'s span — the importing
/// document's only position for it.
pub(super) fn component_use_sites(
    doc: &Document,
    components: &ComponentSet,
) -> std::collections::BTreeMap<String, Span> {
    let mut sites = std::collections::BTreeMap::new();
    let roots = doc
        .shots
        .iter()
        .map(|s| &s.body)
        .chain(doc.quests.iter().map(|q| &q.body))
        .chain(doc.entries.iter().map(|e| &e.body))
        .chain(doc.beats.iter().map(|b| &b.body));
    for body in roots {
        let mut direct = Vec::new();
        collect_use_names(body, &mut direct);
        for (name, span) in direct {
            // Transitive closure, each reached name keeping this outer span.
            let mut stack = vec![name];
            while let Some(n) = stack.pop() {
                if sites.contains_key(&n) {
                    continue;
                }
                sites.insert(n.clone(), span);
                if let Some(def) = components.table.get(&n) {
                    let mut inner = Vec::new();
                    for shot in &def.body.shots {
                        collect_use_names(&shot.body, &mut inner);
                    }
                    stack.extend(inner.into_iter().map(|(n, _)| n));
                }
            }
        }
    }
    sites
}

/// Every `::use{component="…"}` in `nodes`, in document order, with its span.
fn collect_use_names(nodes: &[Node], out: &mut Vec<(String, Span)>) {
    let mut dirs = Vec::new();
    collect_use_directives(nodes, &mut dirs);
    out.extend(dirs.into_iter().filter_map(|d| {
        d.attrs.iter().find_map(|a| match (&*a.key, &a.value) {
            ("component", AttrValue::Str(s)) => Some((s.clone(), d.span)),
            _ => None,
        })
    }));
}

/// `src` (a canonical component path) as shown in a message: relative to the
/// nearest ancestor directory holding a `lute.project.yaml` — the project
/// root every other path in a report is read against — with `/` separators.
/// A component outside any project keeps its full path.
fn project_relative_display(src: &std::path::Path) -> String {
    src.ancestors()
        .skip(1)
        .find(|dir| dir.join("lute.project.yaml").is_file())
        .and_then(|root| src.strip_prefix(root).ok())
        .map(|rel| {
            rel.components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/")
        })
        .unwrap_or_else(|| src.display().to_string())
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
        // Component bodies get an empty `RelVocab` (above) — no relation is
        // ever declared/queryable inside one (dsl §13, presentational-scope
        // only), so the merged domains view is moot; kept empty to match.
        domains: std::collections::BTreeMap::new(),
        clock: None,
        terminal: None,
        terminal_persists: false,
        seasons: Default::default(),
        occasion_scopes: Default::default(),
    }
}

/// Positive scan over one component-body CEL slot for a dependency on
/// ambient state (dsl 0.4.0 §6.1/§6.2): a state-path READ
/// (`cel_paths::collect_path_uses`, the SAME reconstruction the scene-level
/// `check_cel_slot` uses), a fact query, or `now()` (`is_profile_fact_query`,
/// via a MARKED re-parse — mirrors `check_cel_slot`'s own Pass 3, since
/// `@ref`s must be marker-substituted before a real `now()`/`holds(...)` call
/// is distinguishable from a parameterized `@ref(args)`). One
/// `E-COMPONENT-STATE` diagnostic per occurrence, at the slot's span — this
/// is a POSITIVE scan (it finds what a component body may NOT do), unlike
/// `check_cel_slot`'s declared-schema resolution (D6: the empty component env
/// would otherwise misreport these same sites as
/// `E-UNDECLARED`/`E-RELATION-UNKNOWN`).
///
/// dsl 0.26.0 §3.3: `own` is the component's OWN result slots
/// ([`component_own_slots`]) — a read of one is no ambient read.
fn component_slot_state_scan(
    slot: &CelSlot,
    arena: &CelArena,
    own: &std::collections::BTreeSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    if let Some(handle) = slot.ast.clone() {
        if let Some(root) = arena.get(handle) {
            for use_ in crate::cel_paths::collect_path_uses(&root.expr) {
                if own.contains(&use_.path) {
                    continue;
                }
                diags.push(use_diag(
                    E_COMPONENT_STATE,
                    format!(
                        "`{}` reads ambient state — a component body may not depend on it; bind it through a param (dsl 0.4 §6.2)",
                        use_.path
                    ),
                    slot.span,
                ));
            }
        }
    }
    let mut marked = CelArena::default();
    if let Some(mh) = lute_cel::parse_slot_marked_refs(&mut marked, &slot.raw) {
        if let Some(mroot) = marked.get(mh) {
            scan_fact_queries(&mroot.expr, slot, diags);
        }
    }
}

/// Recurse `expr` (a MARKED re-parse) for every fact-query/`now()` call
/// (`is_profile_fact_query`, cel_resolve.rs) — mirrors `check_fact_queries`'s
/// own recursion shape (cel_resolve.rs) so a call nested inside an operator
/// (`count(x) >= 1`) or a `validAt` second arg is still found; a well-shaped
/// hit does NOT recurse into its OWN pattern arg (a relation `Call`, not a
/// CEL sub-expression).
fn scan_fact_queries(expr: &cel_parser::ast::Expr, slot: &CelSlot, diags: &mut Vec<Diagnostic>) {
    use cel_parser::ast::Expr;
    match expr {
        Expr::Call(c) => {
            if crate::cel_resolve::is_profile_fact_query(c) {
                let message = if c.func_name == "now" {
                    "`now()` reads narrative time — a component body may not depend on ambient state; bind it through a param (dsl 0.4 §6.2)".to_string()
                } else {
                    format!(
                        "`{}(…)` queries the fact store — a component body may not depend on ambient state; bind it through a param (dsl 0.4 §6.2)",
                        c.func_name
                    )
                };
                diags.push(use_diag(E_COMPONENT_STATE, message, slot.span));
                // `validAt`'s second arg is a genuine CEL expr (may itself
                // nest another fact query, e.g. `validAt(rel(a), now())`).
                if c.func_name == "validAt" {
                    if let Some(t) = c.args.get(1) {
                        scan_fact_queries(&t.expr, slot, diags);
                    }
                }
                return;
            }
            if let Some(t) = &c.target {
                scan_fact_queries(&t.expr, slot, diags);
            }
            for a in &c.args {
                scan_fact_queries(&a.expr, slot, diags);
            }
        }
        Expr::List(list) => {
            for el in &list.elements {
                scan_fact_queries(&el.expr, slot, diags);
            }
        }
        Expr::Select(sel) => scan_fact_queries(&sel.operand.expr, slot, diags),
        Expr::Comprehension(_)
        | Expr::Map(_)
        | Expr::Struct(_)
        | Expr::Ident(_)
        | Expr::Literal(_)
        | Expr::Unspecified => {}
    }
}

/// Validate a content line's `{{…}}` interpolations inside a component body
/// (dsl 0.4.0 §6.2): the component analog of [`check_interps`], differing in
/// how an ordinary `Path` interpolation is treated — a component has
/// no `state:` schema to resolve one against, so any REAL dotted-path
/// referent is unconditionally `E-COMPONENT-STATE` (`{{$}}` is never
/// meaningful outside a `<match test>` and stays on the ordinary
/// [`check_interp_referent`]/`E-DOLLAR-OUTSIDE-MATCH` path, same as at scene
/// level) — and in rendering a bare `string` param (dsl 0.23.0 §5). A `@ref`
/// interpolation otherwise keeps its ordinary `E-UNDECLARED-REF`/`E-REF-TYPE`
/// semantics (resolved against the component's `@param` env in `ctx`).
fn component_interp_scan(
    interps: &[Interp],
    ctx: &Ctx<'_>,
    own: &std::collections::BTreeSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    let interp_ctx = Ctx {
        env: ctx.env,
        in_match: false,
        match_subject: None,
    };
    for interp in interps {
        match interp.kind {
            InterpKind::Reserved => check_interp_format(interp, ctx.env, false, diags),
            InterpKind::Path => {
                let has_dollar = scan_refs(&interp.raw).iter().any(|r| r.is_dollar);
                if has_dollar {
                    check_interp_referent(&interp.raw, interp, &interp_ctx, true, diags);
                    continue;
                }
                // 0.21.1 T3-7 (seven F9): `{{memory}}` where `memory` IS a
                // declared param is a missing `@`, not an ambient-state read —
                // "bind it through a param" is advice the author already
                // followed. Same code (the read is still not a param ref),
                // but the message names the fix.
                let name = interp.raw.trim();
                // dsl 0.26.0 §3.3: the component's own result slot.
                if own.contains(name) {
                    continue;
                }
                let message = if ctx.env.defs.contains(name) {
                    format!(
                        "`{{{{{name}}}}}` reads `{name}` as a state path, but `{name}` is a param \
                         of this component — write `{{{{@{name}}}}}` (dsl 0.4 §6.2)"
                    )
                } else {
                    format!(
                        "`{}` reads ambient state — a component body may not depend on it; bind it through a param (dsl 0.4 §6.2)",
                        interp.raw
                    )
                };
                diags.push(use_diag(E_COMPONENT_STATE, message, interp.span));
            }
            InterpKind::Ref => {
                if !is_bare_ref(&interp.raw) {
                    diags.push(interp_grammar_diag(&interp.raw, interp.span));
                    continue;
                }
                check_interp_referent(&interp.raw, interp, &interp_ctx, true, diags);
            }
        }
    }
}

/// `Some(name)` when `raw` (a component `<match on>` subject, dsl 0.4.0 §6.2)
/// is EXACTLY a bare `@param` reference — no call args, nothing before or
/// after it. Mirrors the `is_whole_slot` idiom (cel_resolve.rs:126-134) with
/// one deliberate narrowing: a call group (`@name(args)`) disqualifies here
/// too (`is_whole_slot` accepts it there) — a component `<match>` dispatches
/// on the PARAM VALUE, never a call result. `None` for a compound
/// expression, a literal, or `$`.
pub(crate) fn bare_param_ref(raw: &str) -> Option<String> {
    let content_start = raw.len() - raw.trim_start().len();
    let content_end = raw.trim_end().len();
    let mut whole = scan_refs(raw).into_iter().filter(|r| {
        !r.is_dollar
            && r.call.is_none()
            && r.span.byte_start == content_start
            && r.span.byte_end == content_end
    });
    let first = whole.next()?;
    whole.next().is_none().then_some(first.name)
}

/// D7 (dsl 0.4.0 §6.1/§6.2, the corrected non-`is_some()` form): `true` when
/// `tag`'s RESOLVED directive decl declares ACTUAL writes — non-empty
/// `state.declares` (engine-written result slots) or non-empty
/// `effects.writes` (incl. `WriteValue::FromBridgeResult`). Tested for
/// NON-EMPTINESS, not merely `Option::is_some`: a decl that carries a
/// `state`/`effects` block with nothing declared in it (or a bare `bridge:`
/// ref with no declared landing site) stays presentational. An unknown tag
/// (no resolved decl) is `false` here — `check_directive` reports
/// `E-UNKNOWN-DIRECTIVE` on its own path, never this one.
pub(crate) fn directive_writes_state(snapshot: &CapabilitySnapshot, tag: &str) -> bool {
    snapshot.directive(tag).is_some_and(|decl| {
        decl.state.as_ref().is_some_and(|s| !s.declares.is_empty())
            || decl.effects.as_ref().is_some_and(|e| !e.is_empty())
    })
}

/// What a component body may do beyond presenting (dsl 0.24.0 §4): its
/// `effects:` flag, its own `speaker` params, the host's cast they range
/// over, and (dsl 0.26.0 §3.3) the result slots its own plugin directives
/// declare ([`component_own_slots`]), which it may read.
pub(super) struct BodyScope<'a> {
    pub(super) effects: bool,
    pub(super) speakers: &'a [String],
    pub(super) cast: &'a std::collections::BTreeMap<String, lute_manifest::schema::CastMember>,
    pub(super) own: std::collections::BTreeSet<String>,
}

/// dsl 0.26.0 §3.3: the result slots the plugin directives of a component's
/// own body declare (a literal key: `::battle{resultKey="fight"}` →
/// `scene.battle.fight.won`, …) — state its body MAY read, since every
/// `::use` opens them in the host ([`fold_directive_slots`]); not ambient.
pub(super) fn component_own_slots(
    body: &Document,
    snapshot: &CapabilitySnapshot,
    components: &ComponentSet,
) -> std::collections::BTreeSet<String> {
    let mut schema = crate::meta::StateSchema::default();
    fold_directive_slots(body, snapshot, components, &mut schema);
    schema.decls.into_keys().collect()
}

/// dsl 0.26.0 §3.3: `slot` reads state, and only the component's own result
/// slots (`own`) — no other path, no fact query, no `now()`.
fn reads_only_own(
    slot: &CelSlot,
    arena: &CelArena,
    own: &std::collections::BTreeSet<String>,
) -> bool {
    let Some(root) = slot.ast.clone().and_then(|h| arena.get(h)) else {
        return false;
    };
    let uses = crate::cel_paths::collect_path_uses(&root.expr);
    if uses.is_empty() || !uses.iter().all(|u| own.contains(&u.path)) {
        return false;
    }
    let mut queries = Vec::new();
    component_slot_state_scan(slot, arena, own, &mut queries);
    queries.is_empty()
}

/// A diagnostic a component's EMPTY schema/vocabulary raises for a path or
/// relation the host may well declare (D6) — dropped from an effects write's
/// component-side check, since the host judges that write at each `::use`.
fn is_host_schema_code(code: &str) -> bool {
    code == "E-UNDECLARED" || code == crate::rel_schema::E_RELATION_UNKNOWN
}

/// dsl 0.26.0 §4: a component-body directive's `when=` — refused where no
/// guard applies ([`directive_when_refused`]); a write's guard (`writes`)
/// is judged at each `::use` against the host (it rides the spliced write),
/// so here only its `@param`s resolve; any other guard is component logic
/// and reads params only (`E-COMPONENT-STATE`, like a line's `when=`).
fn component_guard(
    d: &Directive,
    snapshot: &CapabilitySnapshot,
    arena: &CelArena,
    ctx: &Ctx<'_>,
    writes: bool,
    scope: &BodyScope<'_>,
    diags: &mut Vec<Diagnostic>,
) {
    let Some(when) = &d.when else {
        return;
    };
    let ds = check_directive_when(d, snapshot, arena, ctx);
    if writes {
        diags.extend(ds.into_iter().filter(|d| !is_host_schema_code(&d.code)));
    } else {
        diags.extend(ds);
        if directive_when_refused(d, snapshot).is_none() {
            component_slot_state_scan(when, arena, &scope.own, diags);
        }
    }
}

/// Walk a component body in component mode (dsl 0.4.0 §6, §13). Lines +
/// purely-presentational staging directives (incl. nested `::use`) are
/// validated as before; `<branch>`/`<hub>`/`<timeline>`/`<on>`/`<objective>`/
/// `::set`/`::assert`/`::retract` stay `E-COMPONENT-BODY` (§6.1's ban).
/// `<match>` is no longer a blanket rejection (§6.2): a `<match on="@param">`
/// is ADMITTED and its arms walk recursively through this same function; any
/// other subject either reads ambient state (`E-COMPONENT-STATE`) or has no
/// domain to dispatch on (`E-COMPONENT-BODY`). A directive whose resolved
/// decl declares actual state/bridge-result writes is `E-COMPONENT-STATE`
/// wherever it appears in the body (D7), not just at the top level.
///
/// dsl 0.24.0 §4: in an `effects: true` body (`scope.effects`) the writes —
/// `::set`/`::assert`/`::retract`, a state-writing directive, a nested
/// `::use` of another effects component — are admitted. They are judged at
/// each `::use` against the HOST's schema (`component_effects` splices them
/// there), so here only their `@param` refs resolve; a write's own slots
/// (`::set` value, `when=`) MAY read host state. Every other position keeps
/// the purity contract: a guard or match subject reading ambient state is
/// still `E-COMPONENT-STATE`, and a presentational body `::use`-ing an
/// effects component is `E-COMPONENT-BODY`.
#[allow(clippy::too_many_arguments)]
pub(super) fn walk_component_body(
    nodes: &[Node],
    snapshot: &CapabilitySnapshot,
    providers: &ProviderSet,
    domains: &std::collections::BTreeMap<String, Domain>,
    arena: &CelArena,
    ctx: &Ctx<'_>,
    components: &ComponentSet,
    param_domains: &std::collections::BTreeMap<String, DomainInfo>,
    scope: &BodyScope<'_>,
    diags: &mut Vec<Diagnostic>,
) {
    for node in nodes {
        match node {
            Node::Line(l) => {
                body_attr_refs(&l.attrs, snapshot, arena, ctx, None, &scope.own, diags);
                // finding 2 (Task 7b, the SAME class as finding 1 below):
                // `check_content_line_attrs` was skipped here entirely, so
                // NO content-line attribute rule applied inside a component
                // body — not the unknown-key check (dsl 0.1.0 §7.1), not the
                // `emotion`/`action` domain-slot resolution (0.9.0 D-C), not
                // the delivery-flag rules (0.2.2 §D7). A `::use` was
                // therefore a hole straight through 0.9.0's central
                // invariant: an undeclared vocabulary member authored inside
                // a component reached the compiled artifact unflagged.
                //
                // VOCABULARY SCOPE (the one design decision here): `domains`
                // is the IMPORTING document's merged vocabulary — `check()`
                // threads `folded.domains` (= `merge_domains(input.snapshot,
                // input.imports, …)`) into `validate_components`, and
                // `ComponentDef` (component_import.rs) carries only
                // `params`/`body`/`src`, never the component file's OWN
                // `uses:` imports. Resolving against the component's own
                // declared vocabulary would be preferable (a component's
                // validity should not depend on its importer), but those
                // imports are simply not reachable from here, and inventing a
                // second resolution path for one call is worse than the
                // divergence it would fix. So: the importing document's
                // vocabulary, deliberately — with one residual divergence
                // from a STANDALONE `lute check <component>.lute` (which
                // walks the body through `Walker::walk` against the
                // component's own `uses:`): a domain that ONLY the component
                // declares is `E-DOMAIN-UNKNOWN` through a scene that does
                // not also import it. The narrow surface is why that is
                // tolerable — the content-line slots name exactly two fixed
                // domains (`emotion`/`action`), and `merge_domains` DROPS any
                // project-schema domain whose name the plugin/core baseline
                // already owns (`E-DOMAIN-DUP`), so whenever the baseline
                // declares them both paths resolve the identical members.
                crate::content_line::check_content_line_attrs(
                    l,
                    snapshot,
                    providers,
                    domains,
                    diags,
                );
                // `{{@param}}` in a component body is a referent too (dsl
                // §7.6, §6.2): resolved against the component `@param` env in
                // `ctx` — an undeclared ref is `E-UNDECLARED-REF`. A bare
                // `{{run.x}}`-style state path is ALWAYS `E-COMPONENT-STATE`
                // here (a component has no `state:` schema to resolve one
                // against, and the purity contract forbids the read anyway).
                component_interp_scan(&l.interps, ctx, &scope.own, diags);
                diags.extend(text_looks_like_ref(l, ctx));
                super::literal_text::line_text(l, ctx, None, diags);
                // dsl 0.26.0 §3.2: `@@p:` speaks as the member the `speaker`
                // param `p` names at each `::use`.
                if let Some(p) = l.speaker.strip_prefix('@') {
                    if !scope.speakers.iter().any(|s| s == p) {
                        let why = if ctx.env.defs.contains(p) {
                            format!("`{p}` is not a `speaker` param — declare `{p}: speaker`")
                        } else {
                            format!("this component has no param `{p}`")
                        };
                        diags.push(use_diag(
                            E_COMPONENT_ARG,
                            format!(
                                "`@@{p}:` speaks as the cast member a `speaker` param names, and \
                                 {why} (dsl 0.26.0 §3.2)"
                            ),
                            l.span,
                        ));
                    }
                }
                // dsl 0.4.0 §7.2/§6.2: a content-line `when=` guard gets BOTH
                // the positive ambient-state scan (D6: the AUTHORITATIVE
                // `E-COMPONENT-STATE` diagnosis for a bare-param guard vs. an
                // ambient-state read) AND the ordinary `check_cel_slot`
                // treatment every other component-body CEL slot gets — a
                // component `when=` is still a `Bool` Condition slot, so an
                // undeclared `@ref` is `E-UNDECLARED-REF` and a `$` read
                // trips `E-DOLLAR-OUTSIDE-MATCH` exactly as it would at scene
                // level (finding 1: `check_cel_slot` was skipped here
                // entirely, so both went unchecked). D9 (dsl 0.4.0 §7.2): `$`
                // is NOT in scope in a content-line `when=` — force
                // in_match=false/match_subject=None even when this line sits
                // inside a component-body `<match>` arm (mirrors the
                // scene-level rule, check.rs's `Walker::walk` `Node::Line`
                // arm).
                if let Some(when) = &l.when {
                    let ctx_no_dollar = Ctx {
                        env: ctx.env,
                        in_match: false,
                        match_subject: None,
                    };
                    diags.extend(check_cel_slot(
                        when,
                        arena,
                        &ctx_no_dollar,
                        Some(&ExpectedType::Bool),
                    ));
                    component_slot_state_scan(when, arena, &scope.own, diags);
                }
            }
            Node::Directive(d) if d.tag == "use" => {
                check_use(d, components, ctx, param_domains, diags);
                check_use_typed_args(d, components, snapshot, providers, domains, diags);
                check_speaker_args(d, components, scope.cast, scope.speakers, diags);
                body_attr_refs(&d.attrs, snapshot, arena, ctx, None, &scope.own, diags);
                let writes = scope.effects
                    && use_target(d).is_some_and(|n| components.table.get(n).is_some_and(|c| c.effects));
                component_guard(d, snapshot, arena, ctx, writes, scope, diags);
                if let Some(inner) = use_target(d)
                    .filter(|n| !scope.effects && components.table.get(*n).is_some_and(|c| c.effects))
                {
                    diags.push(use_diag(
                        E_COMPONENT_BODY,
                        format!(
                            "a component body must be presentational (dsl 0.4 §6.2): `::use` of `{inner}` writes state (it declares `effects: true`) — declare `effects: true` on this component too (dsl 0.24.0 §4)"
                        ),
                        d.span,
                    ));
                }
            }
            // dsl 0.27.0 §6: a template's `::body` marker (`templates::check_body_markers`).
            Node::Directive(d) if d.tag == crate::templates::BODY_DIRECTIVE => {}
            Node::Directive(d) if scope.effects && directive_writes_state(snapshot, &d.tag) => {
                // dsl 0.24.0 §4: an effects body's state-writing directive is
                // judged at each `::use` against the host (spliced there);
                // here its attrs resolve against the params only.
                let mut ds = check_directive(d, snapshot, providers, domains, ctx);
                body_attr_refs(&d.attrs, snapshot, arena, ctx, Some(&d.tag), &scope.own, &mut ds);
                diags.extend(ds.into_iter().filter(|d| !is_host_schema_code(&d.code)));
                component_guard(d, snapshot, arena, ctx, true, scope, diags);
            }
            Node::Directive(d) => {
                // D7 (dsl 0.4.0 §6.1/§6.2): a directive whose resolved decl
                // declares actual state/bridge-result writes affects ambient
                // state and is NOT covered by the staging allowance —
                // `E-COMPONENT-STATE`, wherever in the body it sits (an
                // ordinary position or a param-scoped `<match>` arm). Short-
                // circuits the ordinary attr/ref validation below, mirroring
                // how every other rejected construct in this walk skips
                // further validation once its purity verdict is decided.
                if directive_writes_state(snapshot, &d.tag) {
                    diags.push(use_diag(
                        E_COMPONENT_STATE,
                        format!(
                            "`::{}` declares state/bridge-result writes — a component body may not affect ambient state; declare `effects: true` to write state at each `::use` (dsl 0.4 §6.1, 0.24.0 §4)",
                            d.tag
                        ),
                        d.span,
                    ));
                } else {
                    diags.extend(check_directive(d, snapshot, providers, domains, ctx));
                    body_attr_refs(&d.attrs, snapshot, arena, ctx, Some(&d.tag), &scope.own, diags);
                    component_guard(d, snapshot, arena, ctx, false, scope, diags);
                }
            }
            Node::Set(s) if scope.effects => {
                // dsl 0.24.0 §4: judged at each `::use` against the host's
                // schema (spliced there, `component_effects`); here only the
                // `@param` refs of its value and `when=` resolve.
                let mut ds = check_cel_slot(&s.expr, arena, ctx, None);
                if let Some(when) = &s.when {
                    let ctx_no_dollar = Ctx {
                        env: ctx.env,
                        in_match: false,
                        match_subject: None,
                    };
                    ds.extend(check_cel_slot(
                        when,
                        arena,
                        &ctx_no_dollar,
                        Some(&ExpectedType::Bool),
                    ));
                }
                diags.extend(ds.into_iter().filter(|d| !is_host_schema_code(&d.code)));
            }
            // Ground fact args: nothing to resolve before the host judges
            // them; a guard's `@param`s resolve here (dsl 0.26.0 §4).
            Node::Assert(lute_syntax::ast::Assert { when, .. })
            | Node::Retract(lute_syntax::ast::Retract { when, .. })
                if scope.effects =>
            {
                diags.extend(
                    check_guard(when.as_ref(), arena, ctx)
                        .into_iter()
                        .filter(|d| !is_host_schema_code(&d.code)),
                );
            }
            Node::Set(s) => diags.push(use_diag(
                E_COMPONENT_BODY,
                format!(
                    "a component body must be presentational (dsl 0.4 §6.2): `::set` of `{}` writes state — only a param-scoped `<match>` is admitted for logic, not a state write; declare `effects: true` to write state at each `::use` (dsl 0.24.0 §4)",
                    s.path
                ),
                s.span,
            )),
            Node::Branch(b) => diags.push(use_diag(
                E_COMPONENT_BODY,
                format!(
                    "a component body must be presentational (dsl 0.4 §6.2): the `<branch {}>` logic block is not allowed — presenting a menu records the selection, a state write; only a param-scoped `<match>` is admitted",
                    b.id
                ),
                b.span,
            )),
            Node::Match(m) => {
                crate::logic_attrs::check_match_attrs(m, diags);
                for arm in &m.arms {
                    crate::logic_attrs::check_arm_attrs(arm, diags);
                }
                diags.extend(crate::match_check::check_match_has_subject(
                    m,
                    &ctx.env.state,
                ));
                // Subject-slot CEL validation happens OUTSIDE match scope
                // (dsl §8.2, mirrors the scene-level `Walker`): resolved
                // against the SAME component `@param` env every other slot
                // uses, so a bare ref to an UNDECLARED param (case iv, §6.2)
                // surfaces its own `E-UNDECLARED-REF` here — no admission
                // code is stacked on top of it.
                let subject_ctx = Ctx {
                    env: ctx.env,
                    in_match: false,
                    match_subject: None,
                };
                diags.extend(check_cel_slot(&m.subject, arena, &subject_ctx, None));

                match bare_param_ref(&m.subject.raw) {
                    Some(name) if ctx.env.defs.contains(&name) => {
                        // (i) Admitted: `on="@param"` (dsl §6.2). Arms
                        // evaluate WITHIN match scope (`$` binds to the
                        // param, mirroring the scene walker) so a nested
                        // `<when test="$ == …">` / nested param `<match>`
                        // sees it.
                        let arm_ctx = Ctx {
                            env: ctx.env,
                            in_match: true,
                            match_subject: Some(m.subject.raw.clone()),
                        };
                        // dsl 0.4.0 §6.3 (T7): exhaustiveness over the
                        // dispatched param's domain — `name` is guaranteed
                        // present in `param_domains` (built from the SAME
                        // `def.params` list `ctx.env.defs` was populated
                        // from, `component_env`).
                        let dom = param_domains.get(&name).cloned().expect(
                            "bare_param_ref confirmed `name` is a declared param; \
                             param_domains is built from the same def.params list",
                        );
                        diags.extend(check_param_match(m, dom, ctx));
                        // §5 reachability (`E-ARM-DEAD` / `W-OTHERWISE-DEAD` /
                        // `E-UNSET-LITERAL`) is NOT run here: Task 7e moved it
                        // to the ONE whole-body `check_reachability_in` call in
                        // `validate_components`, which owns it for the ENTIRE
                        // body — this `<match>`, every other one, and the
                        // content-line `when=` guards and post-`::end` content
                        // this arm-local call never reached. Calling it here too
                        // would double-report every dead `<when test>`.
                        for arm in &m.arms {
                            match arm {
                                Arm::When { test, body, .. } => {
                                    diags.extend(check_cel_slot(
                                        test,
                                        arena,
                                        &arm_ctx,
                                        Some(&ExpectedType::Bool),
                                    ));
                                    component_slot_state_scan(test, arena, &scope.own, diags);
                                    walk_component_body(
                                        body, snapshot, providers, domains, arena, &arm_ctx,
                                        components, param_domains, scope, diags,
                                    );
                                }
                                Arm::Otherwise { body, .. } => walk_component_body(
                                    body, snapshot, providers, domains, arena, &arm_ctx,
                                    components, param_domains, scope, diags,
                                ),
                            }
                        }
                    }
                    Some(_) => {
                        // (iv) A bare ref to an UNDECLARED param: the
                        // subject-slot check above already reported
                        // `E-UNDECLARED-REF` — that IS the root defect.
                    }
                    None if reads_only_own(&m.subject, arena, &scope.own) => {
                        // dsl 0.26.0 §3.3: a subject over the component's own
                        // result slots (`scene.battle.fight.won`) — not
                        // ambient; the host judges the match where each
                        // `::use` performs it. Arms evaluate within match
                        // scope, as for a param subject.
                        let arm_ctx = Ctx {
                            env: ctx.env,
                            in_match: true,
                            match_subject: Some(m.subject.raw.clone()),
                        };
                        for arm in &m.arms {
                            let body = match arm {
                                Arm::When { test, body, .. } => {
                                    diags.extend(
                                        check_cel_slot(test, arena, &arm_ctx, Some(&ExpectedType::Bool))
                                            .into_iter()
                                            .filter(|d| !is_host_schema_code(&d.code)),
                                    );
                                    component_slot_state_scan(test, arena, &scope.own, diags);
                                    body
                                }
                                Arm::Otherwise { body, .. } => body,
                            };
                            walk_component_body(
                                body, snapshot, providers, domains, arena, &arm_ctx, components,
                                param_domains, scope, diags,
                            );
                        }
                    }
                    None => {
                        // (ii)/(iii): not a bare param ref. A subject that
                        // reads ambient state (a state path, a fact query,
                        // `now()`) is `E-COMPONENT-STATE`; any other shape (a
                        // literal, a compound over params) has no domain to
                        // dispatch on and is `E-COMPONENT-BODY`.
                        let before = diags.len();
                        component_slot_state_scan(&m.subject, arena, &scope.own, diags);
                        if diags.len() == before {
                            diags.push(use_diag(
                                E_COMPONENT_BODY,
                                "a component body must be presentational (dsl 0.4 §6.2): a `<match>` subject must be a bare declared param, e.g. on=\"@tier\" — dispatch needs a domain".to_string(),
                                m.span,
                            ));
                        }
                    }
                }
            }
            Node::Timeline(tl) => diags.push(use_diag(
                E_COMPONENT_BODY,
                "a component body must be presentational (dsl 0.4 §6.2): a `<timeline>` is not allowed; only a param-scoped `<match>` is admitted for logic".to_string(),
                tl.span,
            )),
            Node::Hub(h) => diags.push(use_diag(
                E_COMPONENT_BODY,
                "a component body must be presentational (dsl 0.4 §6.2): a `<hub>` logic block is not allowed — presenting a menu records the selection, a state write; only a param-scoped `<match>` is admitted".to_string(),
                h.span,
            )),
            Node::Objective(o) => diags.push(use_diag(
                E_COMPONENT_BODY,
                "a component body must be presentational (dsl 0.4 §6.2): an `<objective>` logic block is not allowed; only a param-scoped `<match>` is admitted".to_string(),
                o.span,
            )),
            Node::On(o) => diags.push(use_diag(
                E_COMPONENT_BODY,
                "a component body must be presentational (dsl 0.4 §6.2): an `<on>` logic block is not allowed; only a param-scoped `<match>` is admitted".to_string(),
                o.span,
            )),
            Node::Assert(a) => diags.push(use_diag(
                E_COMPONENT_BODY,
                format!(
                    "a component body must be presentational (dsl 0.4 §6.2): `::assert` of `{}` writes state — only a param-scoped `<match>` is admitted for logic, not a state write",
                    a.pattern.relation
                ),
                a.span,
            )),
            Node::Retract(r) => diags.push(use_diag(
                E_COMPONENT_BODY,
                format!(
                    "a component body must be presentational (dsl 0.4 §6.2): `::retract` of `{}` writes state — only a param-scoped `<match>` is admitted for logic, not a state write",
                    r.pattern.relation
                ),
                r.span,
            )),
        }
    }
}

/// Validate the `@ref`-valued attrs of a component-body node against the param
/// ref namespace (the free-function analog of [`Walker::check_attr_refs`]).
/// Every `AttrValue::Ref` slot ALSO gets the positive ambient-state scan (dsl
/// 0.4.0 §6.2) — a `Line`, a `::use` invocation, and an ordinary staging
/// directive's attrs all route through this one helper, so `E-COMPONENT-STATE`
/// covers every attr-valued CEL slot in a component body uniformly, not just
/// `<match>` subjects/tests and `{{…}}` interpolations.
fn body_attr_refs(
    attrs: &[Attr],
    snapshot: &CapabilitySnapshot,
    arena: &CelArena,
    ctx: &Ctx<'_>,
    directive_tag: Option<&str>,
    own: &std::collections::BTreeSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    for attr in attrs {
        if let AttrValue::Ref(slot) = &attr.value {
            let expected = directive_tag
                .and_then(|tag| snapshot.directive(tag))
                .and_then(|decl| decl.attrs.iter().find(|a| a.name == attr.key))
                .map(|a| ExpectedType::Ty(a.ty.clone()));
            diags.extend(check_cel_slot(slot, arena, ctx, expected.as_ref()));
            component_slot_state_scan(slot, arena, own, diags);
        }
    }
}

/// The component NAME a `::use` directive targets (a plain-string `component=`
/// attr), or `None` for any other directive / a non-string component attr.
pub(super) fn use_target(dir: &Directive) -> Option<&str> {
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
                for b in h.bodies() {
                    collect_use_targets(b, out);
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

/// A component body reads its params only (dsl §13): every `@name` a CEL
/// slot or a `{{@…}}` interpolation of `body` reads that is no param of the
/// component (`params`) but a def of `env` (bar `env_params`, the params of
/// a component whose env it is) is `E-COMPONENT-STATE` at the read, naming
/// the param that carries the def in. The component's own check and each
/// host's `::use` report it alike, so `check-project` keeps the one in the
/// component.
pub(super) fn component_def_reads(
    body: &Document,
    params: &std::collections::BTreeMap<String, DomainInfo>,
    env: &Env,
    env_params: &[lute_manifest::schema::DefParam],
) -> Vec<Diagnostic> {
    let is_def = |n: &str| {
        !params.contains_key(n) && env.defs.contains(n) && !env_params.iter().any(|p| p.name == n)
    };
    let report = |name: &str, span: Span| {
        use_diag(
            E_COMPONENT_STATE,
            format!(
                "a component body cannot read defs or state, and `@{name}` is a def, not a \
                 param — declare the param `{name}: {{ type: {}, default: \"@{name}\" }}` under \
                 `params:`, so each `::use` passes the def in",
                param_type_spelling(env.def_types.get(name))
            ),
            span,
        )
    };
    let mut out = Vec::new();
    for_each_cel_slot(body, &mut |slot| {
        for r in scan_refs(&slot.raw) {
            if !r.is_dollar && is_def(&r.name) {
                let base = slot.span.byte_start;
                let span = Span {
                    byte_start: base + r.span.byte_start,
                    byte_end: base + r.span.byte_end,
                    line: 0,
                    column: 0,
                    utf16_range: (0, 0),
                };
                out.push(report(&r.name, span));
            }
        }
    });
    fn interps<'a>(nodes: &'a [Node], out: &mut Vec<&'a Interp>) {
        for node in nodes {
            match node {
                Node::Line(l) => out.extend(&l.interps),
                Node::Match(m) => {
                    for arm in &m.arms {
                        let (Arm::When { body, .. } | Arm::Otherwise { body, .. }) = arm;
                        interps(body, out);
                    }
                }
                _ => {}
            }
        }
    }
    let mut found = Vec::new();
    for shot in &body.shots {
        interps(&shot.body, &mut found);
    }
    for interp in found.into_iter().filter(|i| i.kind == InterpKind::Ref) {
        if let Some(r) = scan_refs(&interp.raw)
            .into_iter()
            .find(|r| !r.is_dollar && is_def(&r.name))
        {
            out.push(report(&r.name, interp.span));
        }
    }
    out
}

/// A [`Type`] as a `params:` entry spells it (`bool`, `{ enum: [a, b] }`);
/// `<type>` for one no param declares.
fn param_type_spelling(ty: Option<&Type>) -> String {
    match ty {
        Some(Type::Bool) => "bool".to_string(),
        Some(Type::Number) => "number".to_string(),
        Some(Type::Str) => "string".to_string(),
        Some(Type::Enum(members)) => format!("{{ enum: [{}] }}", members.join(", ")),
        Some(Type::Domain(d)) => format!("{{ domain: {d} }}"),
        Some(Type::Entity(k)) => format!("{{ entity: {k} }}"),
        _ => "<type>".to_string(),
    }
}
