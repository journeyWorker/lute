//! `lute check`: one document (or one `.yaml` state schema), plus the
//! compile-stage gate and component call-site checks it shares with
//! `compile`/`trace`/`check-project`.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lute_check::{check, fold_env, CheckInput, Mode};
use lute_core_span::{Diagnostic, Severity, Span};
use lute_manifest::project::{load_project, resolve_document_snapshot};
use lute_manifest::snapshot::CapabilitySnapshot;

use crate::input::{build_input, build_input_with, BuiltInput};
use crate::input_cache::InputCache;
use crate::output::{apply_deny_json, print_human, DenyPolicy};
use crate::project::{discover_project, find_lute_files, nearest_manifest_dir};

/// Every document under `root` whose `::use` names component `name`
/// (dsl 0.10.0 §9 rule 4).
///
/// Reuses [`find_lute_files`] — the same walk `collect_project_docs` performs at
/// its own first line — so "in the resolved project" means exactly what it
/// means for `check-project`, including its symlink canonicalization and
/// deduplication. Parses each candidate rather than running `check()` on it: at
/// this point only the `::use` graph matters, and a full check per file would
/// make the standalone leg quadratic in the project.
///
/// Byte-sorted, for a deterministic "first caller".
fn callers_of_component(root: &Path, name: &str) -> Vec<PathBuf> {
    let Ok(files) = find_lute_files(root) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = Vec::new();
    for file in &files {
        let Ok(text) = std::fs::read_to_string(file) else {
            continue;
        };
        let (doc, _diags) = lute_syntax::parse(&text);
        if document_uses_component(&doc, name) {
            out.push(file.clone());
        }
    }
    out.sort();
    out
}

/// True when any `::use` anywhere in `doc` names component `name`, or a
/// `<beat use="name">` applies it as a beat template (dsl 0.27.0 §6). Reads
/// the same attribute `lute_check`'s `fold_use` reads.
fn document_uses_component(doc: &lute_syntax::ast::Document, name: &str) -> bool {
    doc.shots
        .iter()
        .any(|shot| nodes_use_component(&shot.body, name))
        || doc
            .quests
            .iter()
            .any(|quest| nodes_use_component(&quest.body, name))
        || doc
            .entries
            .iter()
            .any(|entry| nodes_use_component(&entry.body, name))
        || doc.beats.iter().any(|beat| {
            beat.template.as_ref().is_some_and(|t| t.name == name)
                || nodes_use_component(&beat.body, name)
        })
}

/// Whether `d` is a `::use` of `name`.
fn directive_uses_component(d: &lute_syntax::ast::Directive, name: &str) -> bool {
    d.tag == "use"
        && d.attrs.iter().any(|a| {
            a.key == "component"
                && matches!(&a.value, lute_syntax::ast::AttrValue::Str(s) if s == name)
        })
}

/// The recursion. Every node kind that can CONTAIN a `::use`, mirroring the node
/// set `lute-check`'s own walks recurse. Exhaustive on purpose: the next node
/// kind that can hold a `::use` must not be silently missed.
fn nodes_use_component(nodes: &[lute_syntax::ast::Node], name: &str) -> bool {
    use lute_syntax::ast::{Arm, ClipNode, Node};
    nodes.iter().any(|node| match node {
        Node::Directive(d) => directive_uses_component(d, name),
        Node::Branch(b) => b.choices.iter().any(|c| nodes_use_component(&c.body, name)),
        Node::Hub(h) => h.bodies().any(|b| nodes_use_component(b, name)),
        Node::On(o) => nodes_use_component(&o.body, name),
        Node::Objective(o) => nodes_use_component(&o.body, name),
        Node::Match(m) => m.arms.iter().any(|arm| match arm {
            Arm::When { body, .. } | Arm::Otherwise { body, .. } => nodes_use_component(body, name),
        }),
        Node::Timeline(t) => t.tracks.iter().any(|track| {
            track.clips.iter().any(|clip| match &clip.node {
                ClipNode::Directive(d) => directive_uses_component(d, name),
                ClipNode::Set(_) => false,
            })
        }),
        Node::Line(_) | Node::Set(_) | Node::Assert(_) | Node::Retract(_) => false,
    })
}

/// The `component:` name a document declares, with its frontmatter span, or
/// `None` when it is not a component file.
///
/// Read through `lute_check`'s own frontmatter reader, never a filename
/// convention — a component is a document KIND, not a `.component.lute` suffix,
/// and `component_import.rs` treats a missing `component:` name as
/// `E-COMPONENT-PARSE` for exactly that reason. `TypedMeta.component` is `None`
/// for every other document kind, which makes it the discriminator rather than
/// something to compare a `kind:` against.
///
/// The default snapshot suffices: `component:` is a built-in key and is not
/// capability-gated, the same reason `build_input` types `profile`/`plugins`
/// against `CapabilitySnapshot::default()`. The diagnostics are discarded here —
/// `check()` reports them through its own run.
pub(crate) fn component_name_of(file: &Path) -> Option<(String, Span)> {
    let text = std::fs::read_to_string(file).ok()?;
    let (doc, _diags) = lute_syntax::parse(&text);
    let (typed, _mdiags) = lute_check::parse_meta_kind(
        &doc.meta,
        &lute_manifest::snapshot::CapabilitySnapshot::default(),
        lute_check::MetaKind::Component,
    );
    typed.component.map(|name| (name, doc.meta.span))
}

/// The diagnostics `lute compile` and `lute trace` produce AFTER the `check`
/// gate: `normalize_document` then `expand_document`, the same pair in the same
/// order both of them run (`lute_compile::compile_with_check` passes 2–3,
/// `lute_trace::trace_with_check` step 4).
///
/// `check()` runs neither. That is T9.12's root cause, and it is not confined to
/// components: on ANY document an `E-COMPILE-*` fault was invisible to
/// `lute check` and fatal to everything downstream of it. A scene whose `defs:`
/// bodies form a cycle reported `ok: … (0 warning(s))`, while `lute trace` on
/// the same file printed `E-COMPILE-EXPAND … def expansion cycle: a -> b -> a`
/// and then *"has check error(s) — run `lute check` first"*. `check` cannot emit
/// a code it never computes, so that advice was unfollowable **by
/// construction** for the whole class. Running the pass here is what makes it
/// followable — the false green is closed at the leg that was green, not by
/// quietening the leg that was right.
///
/// A COMPONENT's own `params:` are bound to a placeholder first, exactly as
/// `::use` binds them at a call site. `check` registers a component's params as
/// **bodiless markers** (`defs`/`def_types`/`def_params`, deliberately never
/// `def_bodies` — the D3 marker path `decide()` resolves them through), a shape
/// the expander has no notion of: it looks up `bodies` alone and calls any miss
/// `"names no known def body"`. Expanding a component AS A ROOT would therefore
/// report the absence of a call site as a fault of the component, which it is
/// not — `<match on="@p">` over a declared param is the one logic block a
/// component body admits (dsl 0.4.0 §6.2). Binding first measures the body,
/// which is the only thing this leg can decide.
pub(crate) fn compile_gate_diags(input: &CheckInput) -> Vec<Diagnostic> {
    let (mut doc, _parse_diags) = lute_syntax::parse(&input.text);
    let mut arena = lute_cel::CelArena::default();
    let _ = lute_cel::fill_document(&mut arena, &mut doc);
    let (folded, _, _) = fold_env(&doc, input);
    let cast = lute_check::declared_cast(&input.snapshot, &input.imports, &folded.typed.cast);
    let mut diags = lute_compile::normalize::normalize_document(
        &mut doc,
        &input.components,
        &cast,
        &folded.env.domains,
        &folded.env.state,
        &folded.env.occasion_scopes,
    );
    let bodies = if folded.typed.component.is_some() {
        let mut bodies = folded.def_bodies.clone();
        for p in &folded.typed.params {
            // The param's own name: what a `::use` splices is the caller's
            // argument text, and any `@`/`$`-free stand-in measures the same
            // body. `or_insert` so a real def never loses its body to a param
            // that shadows its name.
            bodies
                .entry(p.name.clone())
                .or_insert_with(|| p.name.clone());
        }
        std::borrow::Cow::Owned(bodies)
    } else {
        std::borrow::Cow::Borrowed(&folded.def_bodies)
    };
    let table = lute_check::DefTable {
        bodies: &bodies,
        params: &folded.env.def_params,
    };
    diags.extend(lute_compile::expand::expand_document(&mut doc, &table));
    diags
}

/// Fold compile-stage diagnostics into a clean `check` verdict: skip any
/// already reported, restore document order (`check` hands back
/// `(byte_start, code)` order, so the merged list reads like one run), and
/// recompute `ok`. Shared by `lute check` and `check-project`.
pub(crate) fn merge_gate_diags(result: &mut lute_check::CheckResult, gate: Vec<Diagnostic>) {
    let mut added = false;
    for d in gate {
        let dup = result.diagnostics.iter().any(|e| {
            e.code == d.code && e.span.byte_start == d.span.byte_start && e.message == d.message
        });
        if !dup {
            result.diagnostics.push(d);
            added = true;
        }
    }
    if !added {
        return;
    }
    lute_check::order_diagnostics(&mut result.diagnostics);
    result.ok = !result
        .diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error);
}

/// `lute compile`/`lute trace` refusing a component file, at its frontmatter.
///
/// A component is not a root document. Its `params:` are bound at each `::use`,
/// so there is no standalone artifact to emit and no standalone walk to take —
/// binding a stand-in and walking anyway makes the trace FABRICATE a decision
/// (measured: `purser-interject.component.lute` reports
/// `trace complete: 1 decision; arms 1/2`, picking `<otherwise>` for a `@pressure`
/// no caller supplied), which is a false green in `trace` traded for a false
/// green in `check`.
///
/// So the invocation is refused, and refused for the reason that is true.
/// Before, the refusal leaked the expander's own internal invariant assertion
/// — `` `@pressure` names no known def body (gate should have caught this) `` —
/// and attributed it to `check`, which reported the same file `ok`. That is
/// T9.12: advice pointing at a tool that contradicted it.
pub(crate) fn component_root_diag(component: &str, at: Span) -> Diagnostic {
    Diagnostic {
        code: "E-COMPILE-COMPONENT".to_string(),
        severity: Severity::Error,
        message: format!(
            "`{component}` is a component (dsl §13): its `params:` are bound at each `::use`, so \
             it has no standalone compiled form — compile or trace a document that imports it. \
             `lute check` on this file gives the component's own verdict and `check-project` is \
             the deciding leg (dsl 0.10.0 §9)"
        ),
        span: at,
        layer: lute_core_span::Layer::Content,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

/// Every diagnostic that holds at EVERY call site, re-anchored inside the
/// component (dsl 0.10.0 §9 rule 4).
///
/// Runs `check()` once per caller — the same run `check-project` performs — and
/// INTERSECTS the component-body diagnostics by `(code, message)`, the same key
/// §9 rule 2's roll-up uses, and for the same reason: that string is
/// byte-identical across callers exactly when the problem is caller-independent.
/// A diagnostic present at only some callers drops out of the intersection and
/// stays with `check-project`, where the caller is visible.
///
/// The surviving diagnostics are then re-anchored from §9 rule 1's secondary
/// location onto the primary one, because HERE the component IS the document
/// being reported on, so its own position is representable and the
/// ``component `x` (path):`` prefix is redundant. Rule 1 measured those
/// line/columns as already resolved against the component's own source, so no
/// re-normalisation is needed.
fn caller_resolved_common(
    callers: &[PathBuf],
    component_file: &Path,
    providers: Option<&Path>,
    root: &Path,
) -> Vec<Diagnostic> {
    use std::collections::{BTreeMap, BTreeSet};

    // `related[].file` is `def.src.display().to_string()` and `def.src` is
    // CANONICAL (`resolve_components` canonicalises), while `component_file` is
    // whatever the user typed on the command line. Without this the intersection
    // is always empty and rule 4 silently does nothing.
    let component_file =
        std::fs::canonicalize(component_file).unwrap_or_else(|_| component_file.to_path_buf());

    let mut per_caller: Vec<BTreeSet<(String, String)>> = Vec::new();
    let mut sample: BTreeMap<(String, String), Diagnostic> = BTreeMap::new();
    let cache = InputCache::default();
    for caller in callers {
        let Some(built) = build_input_with(&cache, caller, providers, Some(root), None) else {
            continue;
        };
        let res = check(&built.input);
        let mut here: BTreeSet<(String, String)> = BTreeSet::new();
        for d in &res.diagnostics {
            // A component-body diagnostic for THIS component: §9 rule 1's
            // `related` entry names the component's source file.
            if !d
                .related
                .iter()
                .any(|r| Path::new(&r.file) == component_file)
            {
                continue;
            }
            let key = (d.code.clone(), d.message.clone());
            here.insert(key.clone());
            sample.entry(key).or_insert_with(|| d.clone());
        }
        per_caller.push(here);
    }
    let Some(first) = per_caller.first().cloned() else {
        return Vec::new();
    };
    let common: BTreeSet<(String, String)> = per_caller
        .iter()
        .skip(1)
        .fold(first, |acc, s| acc.intersection(s).cloned().collect());
    common
        .into_iter()
        .filter_map(|key| sample.remove(&key))
        .map(|mut d| {
            if let Some(r) = d.related.first() {
                d.span = r.diagnostic.span;
                d.message = r.diagnostic.message.clone();
            }
            d.related.clear();
            d
        })
        .collect()
}

/// Check a `.yaml`/`.yml` state-schema declaration file as a SCHEMA: no
/// `kind:`, no frontmatter envelope, no body. The whole file IS the
/// frontmatter, wrapped in a synthetic `Meta` and fed through
/// `MetaKind::Schema` — byte-for-byte the lift `schema_import::read_and_parse`
/// performs on the same file kind when it is reached through `uses:`, so the
/// two surfaces cannot disagree about whether a schema is valid (#21, T3.9).
///
/// Rendering, counting and the exit code all go through the same
/// `CheckResult` path `run_check` uses, so `--json` and the human summary read
/// identically for a schema and for a scene.
fn run_check_schema_yaml(file: &Path, json: bool, policy: &DenyPolicy) -> ExitCode {
    let text = match std::fs::read_to_string(file) {
        Ok(t) => t,
        Err(e) => {
            let e = lute_manifest::io_reason(&e);
            eprintln!("lute: cannot read {}: {e}", file.display());
            return ExitCode::from(2);
        }
    };
    let byte_end = text.len();
    let meta = lute_syntax::ast::Meta {
        raw_yaml: text,
        span: Span {
            byte_start: 0,
            byte_end,
            line: 1,
            column: 1,
            utf16_range: (0, 0),
        },
    };
    let (_tm, mut diagnostics) = lute_check::parse_meta_kind(
        &meta,
        &CapabilitySnapshot::default(),
        lute_check::MetaKind::Schema,
    );
    for d in schema_as_imported_diags(file) {
        let dup = diagnostics.iter().any(|e| {
            e.code == d.code && e.message == d.message && e.span.byte_start == d.span.byte_start
        });
        if !dup {
            diagnostics.push(d);
        }
    }
    // The house zero-then-normalize convention: `meta_key_span` emits byte
    // offsets and leaves `line`/`column` at zero for `check`'s own pass, which
    // this surface bypasses.
    let idx = lute_core_span::TextIndex::new(&meta.raw_yaml);
    for d in &mut diagnostics {
        let start = d.span.byte_start.min(byte_end);
        let end = d.span.byte_end.min(byte_end).max(start);
        d.span = Span::from_bytes(&idx, start, end);
    }
    lute_check::order_diagnostics(&mut diagnostics);
    let result = lute_check::CheckResult {
        ok: !diagnostics.iter().any(|d| d.severity == Severity::Error),
        diagnostics,
        resolved: None,
        domain_use: lute_check::DomainUse::default(),
    };
    render_check_result(file, &result, json, policy)
}

/// What `check` reports about the schema `file` when a document imports it
/// — the problems that need the resolved schema (its own `uses:` /
/// `extends:`) and, inside a project, the project's vocabulary: entity and
/// fact validation, enum labels, the clock's paths and `raise` occasions
/// (dsl 0.24 T3-6). A document that only `uses:` the schema is checked in
/// memory, and every diagnostic it attributes to the schema (its `related`
/// entry in this file) is kept, at the schema's own line. The schema's
/// frontmatter diagnostics (`E-USES-PARSE`) are the caller's already.
fn schema_as_imported_diags(file: &Path) -> Vec<Diagnostic> {
    let Ok(canon) = std::fs::canonicalize(file) else {
        return Vec::new();
    };
    let (Some(base), Some(name)) = (canon.parent(), canon.file_name()) else {
        return Vec::new();
    };
    let project = nearest_manifest_dir(file).and_then(|dir| load_project(&dir).ok().flatten());
    let (snapshot, _) = resolve_document_snapshot(project.as_ref(), None, &Default::default());
    let text = format!(
        "---\nkind: scene\nid: schema-check\nuses: '{}'\n---\n\n## Check\n",
        name.to_string_lossy().replace('\'', "''")
    );
    let (doc, _) = lute_syntax::parse(&text);
    let (meta, _) = lute_check::meta::parse_meta_kind(
        &doc.meta,
        &CapabilitySnapshot::default(),
        lute_check::meta::MetaKind::Scene,
    );
    let input = CheckInput {
        uri: base.join("schema-check.lute").display().to_string(),
        snapshot,
        providers: lute_manifest::project::project_providers(project.as_ref()),
        mode: Mode::Ci,
        imports: lute_check::resolve_imports(base, &meta.uses, &meta.extends, doc.meta.span),
        components: lute_check::resolve_components(base, &[], doc.meta.span),
        defaults: Default::default(),
        text,
    };
    let here = canon.display().to_string();
    check(&input)
        .diagnostics
        .into_iter()
        .filter(|d| d.code != "E-USES-PARSE")
        .flat_map(|d| d.related)
        .filter(|r| r.file == here)
        .map(|r| r.diagnostic)
        .collect()
}

/// Run `check` over one file and print its result. Exit `0` clean / `1` on an
/// error diagnostic (native OR `--deny`-promoted, spec §5) / `2` on an I/O
/// failure.
pub(crate) fn run_check(
    file: &Path,
    json: bool,
    providers: Option<&Path>,
    project: Option<&Path>,
    permission_profile: Option<&str>,
    policy: &DenyPolicy,
) -> ExitCode {
    // #21 / T3.9: `lute check world.schema.yaml` is the obvious next command
    // after an E-USES-PARSE, and it parsed the YAML schema AS A SCENE:
    // E-KIND-MISSING, three E-META-MISSING, and one E-UNCLASSIFIED per line —
    // the same flood for a perfectly VALID schema, never mentioning the real
    // defect. An author who follows that advice adds `kind: scene` to their
    // state schema and destroys it. A `.yaml`/`.yml` target is a pure
    // declaration map (data-catalog foundation B2) and is checked as one.
    if matches!(
        file.extension().and_then(|e| e.to_str()),
        Some("yaml") | Some("yml")
    ) {
        return run_check_schema_yaml(file, json, policy);
    }
    // 0.21.1 T3-9 (ashen F2): a file inside a project is checked against that
    // project. Without the manifest there is no `uses:` schema, no `defaults:`
    // and no profile, so the check reported `E-UNDECLARED`/`E-DOMAIN-UNKNOWN`
    // for paths the project declares, and advice that would have broken it.
    let discovered = discover_project(file, project);
    let project = project.or(discovered.as_deref());
    let Some(built) = build_input(file, providers, project, permission_profile) else {
        return ExitCode::from(2);
    };
    // `build_input` no longer prints these itself (`lute doctor` folds them into
    // its checklist instead); every gating command emits them exactly as before.
    built.report_project_diags();
    let BuiltInput {
        input,
        resolve_error,
        ..
    } = built;
    // plugin 0.0.2 §2: an `E-` capability-resolution diagnostic (bad plugin
    // option, missing active plugin, bad identity template) is a build-failing
    // error; it printed above, and it MUST gate here or it would pass silently.
    if resolve_error {
        return ExitCode::from(1);
    }
    let mut result = check(&input);

    // dsl 0.10.0 §9:962: *"`lute trace` on a component and `lute check` on the
    // same file stop disagreeing"*. They disagreed because `check` stopped one
    // pass short of where `trace` and `compile` stop, so `trace` refused over a
    // fault and then told the author to run the one tool that could not see it.
    // `check` now runs that pass — see [`compile_gate_diags`].
    //
    // Only on a clean check, mirroring both downstream pipelines exactly: they
    // gate on `result.ok` first and reach `normalize`/`expand` only past it, and
    // the expander's `"gate should have caught this"` arms are written on that
    // assumption. Running it on a red document would report consequences of the
    // errors already printed.
    if result.ok {
        merge_gate_diags(&mut result, compile_gate_diags(&input));
    }

    // dsl 0.10.0 §9 rule 4 (**D-W**): a standalone component check either
    // forwards the caller-resolved verdict or refuses to claim `ok`. Until
    // 0.10.0 it did neither: a component that cannot work with ANY of its
    // callers reported `ok`, `check-project` reported the fault once per caller
    // at line 1 of the wrong file, and `lute trace` refused with advice that
    // could not be followed. This is what makes that advice followable.
    //
    // "With no caller in scope" is a DISJUNCTION — *"no project resolved, or no
    // document in the project imports this component"* — and only the second
    // disjunct was built: the whole block hung off `Some(root)`, so
    // `lute check some.component.lute` with no `--project` fell straight
    // through to the bare `ok` the clause forbids. Since 0.21.1 (T3-9) the
    // nearest `lute.project.yaml` is discovered, so "no project resolved"
    // now means the file sits under no manifest at all.
    //
    // Both disjuncts now reach the same reporting point. They do NOT share a
    // message, because they are not the same situation: "no project resolved"
    // means the tool could not look, "no document imports this" means it looked
    // and found nothing. The next step differs — supply a project, versus
    // discover the component is unused — so the verdict names which one it is.
    if let Some((component, at)) = component_name_of(file) {
        match project.map(|root| (root, callers_of_component(root, &component))) {
            // Report only what holds at EVERY call site: a diagnostic holding at
            // some but not all callers is caller-specific and stays with
            // `check-project`, where the caller is visible. Anchored inside the
            // component — that is the whole point of running it here.
            Some((root, callers)) if !callers.is_empty() => {
                // A caller-independent fault the component's own check already
                // reports at the same position is one fault, printed once
                // (round-5 T3-4).
                let common: Vec<Diagnostic> =
                    caller_resolved_common(&callers, file, providers, root)
                        .into_iter()
                        .filter(|c| {
                            !result.diagnostics.iter().any(|d| {
                                d.span.byte_start == c.span.byte_start
                                    && d.span.byte_end == c.span.byte_end
                                    && d.message == c.message
                            })
                        })
                        .collect();
                result.diagnostics.extend(common);
            }
            Some(_) => result
                .diagnostics
                .push(lute_check::component_unverified_diag(
                    &component,
                    at,
                    lute_check::ComponentScope::NoImporter,
                )),
            None => result
                .diagnostics
                .push(lute_check::component_unverified_diag(
                    &component,
                    at,
                    lute_check::ComponentScope::NoProject,
                )),
        }
        result.ok = !result
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error);
    }
    render_check_result(file, &result, json, policy)
}

/// Render one `CheckResult` (`--json` or human) and derive the exit code: `0`
/// clean, `1` on a native OR `--deny`-promoted error. Shared by `run_check`
/// and `run_check_schema_yaml` so a schema and a scene cannot drift in
/// wording, JSON shape or verdict.
fn render_check_result(
    file: &Path,
    result: &lute_check::CheckResult,
    json: bool,
    policy: &DenyPolicy,
) -> ExitCode {
    // §5 verdict: a promoted (denied) diagnostic fails an otherwise-clean run.
    let ok = result.ok && !policy.any_denied(&result.diagnostics);

    if json {
        // Wrap the promotion at the CLI layer (spec §5): serialize lute-check's
        // own `CheckResult` shape, then overlay `severity: "error"` +
        // `denied: true` on each promoted diagnostic and the promoted `ok`.
        let mut value = match serde_json::to_value(result) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("lute: failed to serialize result: {e}");
                return ExitCode::from(2);
            }
        };
        if let Some(arr) = value.get_mut("diagnostics").and_then(|v| v.as_array_mut()) {
            for (d, jd) in result.diagnostics.iter().zip(arr.iter_mut()) {
                apply_deny_json(d, policy, jd);
            }
        }
        if let serde_json::Value::Object(map) = &mut value {
            map.insert("ok".into(), serde_json::json!(ok));
        }
        match serde_json::to_string_pretty(&value) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("lute: failed to serialize result: {e}");
                return ExitCode::from(2);
            }
        }
    } else {
        print_human(file, result, policy);
    }

    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
