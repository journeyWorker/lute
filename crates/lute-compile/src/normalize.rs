//! D8: AST normalization BEFORE lowering — (a) `::use` → the component body
//! inlined as real `Node`s with each `@param` bound (recursive; acyclic per
//! the checker's E-COMPONENT-CYCLE); (b) `<choice into="run.<path>" …>` → a
//! synthesized trailing `::set` node (dsl 0.6.0 §2.1: the record sugar IS
//! exactly a `::set{run.<path> = <value>}` appended to the arm).
//!
//! Component-sourced regions are wrapped in `__component-begin`/`-end`
//! sentinel directives (reserved `__` prefix — the parser can never produce
//! them from source). The stage walker (Task 8) consumes them into
//! `source { component }` stamps; they emit no records. The begin sentinel
//! also carries the expansion's identity segment `{component}#{n}` (dsl
//! 0.22.0 §11, [`component_scope`]).

use std::collections::BTreeMap;

use lute_check::component_effects::{
    bind_attrs, bind_slot_raw, cel_string_literal, fold_component_matches, speaker_display_args,
    use_args,
};
use lute_check::meta::StateSchema;
use lute_check::ComponentSet;
use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::schema::CastMember;
use lute_manifest::types::Type;
use lute_syntax::ast::{
    classify_interp, Arm, Attr, AttrValue, CelKind, CelSlot, Choice, ClipNode, Directive,
    Document, Interp, InterpKind, Line, Match, Node, Set,
};

pub const COMPONENT_BEGIN: &str = "__component-begin";
pub const COMPONENT_END: &str = "__component-end";

/// The begin sentinel's identity-segment attr: `{component}#{n}`, where `n`
/// is the 1-based ordinal of this `::use` among the host's uses of the SAME
/// component, in document order (dsl 0.22.0 §11). The host is one identity
/// scope of the document (all shots together; each `<quest>`; each
/// `<entry>`) or, for a nested `::use`, the enclosing component expansion.
/// A line's `lineId`/`voiceKey` prefix is the host prefix joined with every
/// enclosing segment, outermost first — so two uses never share an id and a
/// component line never shares one with its host.
const COMPONENT_SCOPE_ATTR: &str = "scope";

/// The identity segment a begin sentinel carries ([`COMPONENT_SCOPE_ATTR`]);
/// empty for any other directive. Every consumer that re-derives a
/// component line's `lineId` (the addressing pass via the stage walker,
/// `lute loc export`) reads it from here, so they cannot disagree.
pub fn component_scope(d: &Directive) -> &str {
    d.attrs
        .iter()
        .find_map(|a| match (&*a.key, &a.value) {
            (COMPONENT_SCOPE_ATTR, AttrValue::Str(s)) => Some(s.as_str()),
            _ => None,
        })
        .unwrap_or("")
}

/// The internal attr carrying a component line's SOURCE-ORDER back-filled
/// `code` (ashen N7). [`expand_use`] stamps it on every untagged line of the
/// cloned body BEFORE binding and folding; the lowering reads it when no
/// authored `code` exists. It is deliberately not `code`: the line is still
/// untagged (`lute loc` keeps reporting it so), it only no longer depends on
/// which arms a call site's arguments fold away.
pub const COMPONENT_CODE_ATTR: &str = "__code";

/// Give every untagged line of a component body the code `lute tag` would
/// write into the component file: one identity scope over the whole body in
/// source order, per speaker, max authored + 10 (lute-check `tag.rs`). Run on
/// the fresh clone, before any `<match>` folds, so a call site with a literal
/// argument and one with a `@def` argument mint the same code for the same
/// source line (decision 10: allocation never depends on the argument form).
fn backfill_component_codes(body: &mut [Node]) {
    let mut max: BTreeMap<String, u64> = BTreeMap::new();
    visit_lines(body, &mut |l| {
        let cur = max.entry(l.speaker.clone()).or_insert(0);
        for a in l.attrs.iter().filter(|a| a.key == "code") {
            if let AttrValue::Str(s) = &a.value {
                if let Ok(n) = s.trim().parse::<u64>() {
                    *cur = (*cur).max(n);
                }
            }
        }
    });
    visit_lines(body, &mut |l| {
        if l.attrs.iter().any(|a| a.key == "code") {
            return;
        }
        let cur = max.entry(l.speaker.clone()).or_insert(0);
        // Overflow fails closed for this line only, as `tag.rs` does.
        let Some(next) = cur.checked_add(10) else {
            return;
        };
        *cur = next;
        l.attrs.push(Attr {
            key: COMPONENT_CODE_ATTR.to_string(),
            value: AttrValue::Str(format!("{next:04}")),
            value_span: l.span,
            span: l.span,
        });
    });
}

/// Every content line of `nodes`, at any depth, in source order.
fn visit_lines(nodes: &mut [Node], f: &mut dyn FnMut(&mut Line)) {
    for node in nodes {
        match node {
            Node::Line(l) => f(l),
            Node::Branch(b) => b.choices.iter_mut().for_each(|c| visit_lines(&mut c.body, f)),
            Node::Hub(h) => h.choices.iter_mut().for_each(|c| visit_lines(&mut c.body, f)),
            Node::Match(m) => {
                for arm in &mut m.arms {
                    match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => visit_lines(body, f),
                    }
                }
            }
            Node::Objective(o) => visit_lines(&mut o.body, f),
            Node::On(o) => visit_lines(&mut o.body, f),
            Node::Directive(_)
            | Node::Set(_)
            | Node::Timeline(_)
            | Node::Assert(_)
            | Node::Retract(_) => {}
        }
    }
}

/// What a `::use` expands against: the imported components and the host's
/// declared cast (dsl 0.24.0 §4: a `speaker` param renders the member's name).
struct Components<'a> {
    set: &'a ComponentSet,
    cast: &'a BTreeMap<String, CastMember>,
}

/// Per-host `::use` ordinals, keyed by component name.
type UseOrdinals = BTreeMap<String, u32>;

/// Normalize the tree in place: no `::use` survives; persists are real `Set`s.
/// Total; failures (gate-proven unreachable) degrade to `E-COMPILE-COMPONENT`.
///
/// `pub` for the `lute-trace` consumer (dsl 0.4 §4.4: trace walks the
/// document exactly as compile expansion binds it — D14). `lute-trace`
/// calls this BEFORE [`crate::expand::expand_document`] so component
/// binding (§13.2), the §6.4 folds, and the `when=`/`into=` desugars are
/// inherited by construction, with zero duplicated logic.
pub fn normalize_document(
    doc: &mut Document,
    components: &ComponentSet,
    cast: &BTreeMap<String, CastMember>,
    schema: &StateSchema,
) -> Vec<Diagnostic> {
    let components = &Components {
        set: components,
        cast,
    };
    let mut diags = Vec::new();
    // One ordinal counter per identity scope (see `COMPONENT_SCOPE_ATTR`):
    // shots share one, each quest and each entry gets its own.
    let mut shot_uses = UseOrdinals::new();
    for shot in &mut doc.shots {
        normalize_nodes(&mut shot.body, components, schema, &mut shot_uses, &mut diags);
    }
    for quest in &mut doc.quests {
        normalize_nodes(&mut quest.body, components, schema, &mut UseOrdinals::new(), &mut diags);
    }
    // dsl 0.19.0 §4: entry bodies admit content lines (incl. `when=` guards)
    // and `<match>` — the same desugars a quest body gets.
    for entry in &mut doc.entries {
        normalize_nodes(&mut entry.body, components, schema, &mut UseOrdinals::new(), &mut diags);
    }
    // dsl 0.23.0 §4: a bundle beat body is a scene body, its own identity
    // scope (like an entry's).
    for beat in &mut doc.beats {
        normalize_nodes(&mut beat.body, components, schema, &mut UseOrdinals::new(), &mut diags);
    }
    // Subquest synthesis (2026-08-31 design §2.1/§2.2) — MUST run here (not
    // in `stage::walk_quest`) so `lute-trace` inherits the derived
    // predicates verbatim: trace calls `normalize_document` before its own
    // walk, so upward semantics need zero engine code. `stage.rs` then
    // reads the already-synthesized `done.raw`/`fail.raw` through the
    // ordinary `CelPair::from_raw` path — no subquest-aware branch in the
    // lowerer.
    synthesize_subquests(&mut doc.quests);
    diags
}

/// Fill the empty `done` slot of every `<objective quest=c>` with §2.1's
/// synthesized predicate and extend the parent quest's `fail` with §2.2's
/// required-child disjunction. Both text forms are load-bearing wire
/// contract shared with `lute-trace` (see `docs/superpowers/plans/
/// 2026-08-31-lute-subquest.md` Global Constraints):
///
/// - done raw: `quest.<c>.state == 'complete'` — exact, single-quoted.
/// - fail raw: `(<authoredFail>) || quest.<c1>.state == 'failed' || …` when
///   an authored fail exists; the disjunction alone (no wrapping parens)
///   otherwise. Only REQUIRED children (`!optional`) contribute a `failed`
///   test; children appear in document order.
/// - dsl 0.24.0 §2, `complete="any"`: the synthesized part is instead the
///   `&&`-conjunction over EVERY required objective (a child's
///   `quest.<c>.state == 'failed'`, any other's
///   `quest.<q>.objectives.<o>.failed`), parenthesized when it has more
///   than one term — one failed alternative leaves the others open.
///
/// Non-goals: this pass does NOT validate anything the checker owns
/// (`E-OBJECTIVE-QUEST-DONE` on a `quest=` + non-empty `done` collision;
/// unknown / cycled / multi-parent references). Compile only runs after
/// the checker's error gate, so a subquest objective reaching here has a
/// well-formed empty `done` slot; a non-empty `done` slot on a `quest=`
/// objective is left untouched (the checker already flagged it).
fn synthesize_subquests(quests: &mut [lute_syntax::ast::Quest]) {
    for quest in quests {
        // Collect required children in document order (`!optional` filter,
        // spec §2.2) BEFORE mutating anything: the fail synthesis needs
        // the ordered id list, and the done fill would otherwise force a
        // second scan. `Node::Objective` is the only body node that can
        // carry `quest=` (grammar admission, dsl 0.2.0 §6.4/§6.7 —
        // objectives never nest), so a single top-level pass is complete.
        let mut required_children: Vec<String> = Vec::new();
        // dsl 0.24.0 §2: a `complete="any"` quest fails only once EVERY
        // required objective has failed — a child by failing, any other
        // objective by its reserved `failed` flag.
        let mut required_exhausted: Vec<String> = Vec::new();
        for node in &mut quest.body {
            let Node::Objective(o) = node else { continue };
            let Some(child) = o.quest.clone() else {
                if !o.optional && !o.id.is_empty() {
                    required_exhausted
                        .push(format!("quest.{}.objectives.{}.failed", quest.id, o.id));
                }
                continue;
            };
            // §2.1: fill an empty `done` slot only. A non-empty `done`
            // means the author wrote both `quest=` and `done=` — the
            // checker fires `E-OBJECTIVE-QUEST-DONE` and compile aborts
            // upstream; if we somehow reach here with that shape we must
            // NOT clobber the authored text (would poison error recovery
            // and hide the collision from downstream tools).
            if o.done.raw.trim().is_empty() {
                o.done.raw = format!("quest.{child}.state == 'complete'");
            }
            if !o.optional {
                required_exhausted.push(format!("quest.{child}.state == 'failed'"));
                required_children.push(child);
            }
        }
        let any = quest.completes_on_any();
        if required_children.is_empty() && !(any && !required_exhausted.is_empty()) {
            continue;
        }
        let disjunction = if any {
            // One alternative failing leaves the others open; all of them
            // failing (document order, `&&`-joined) fails the quest.
            let all = required_exhausted.join(" && ");
            if required_exhausted.len() > 1 {
                format!("({all})")
            } else {
                all
            }
        } else {
            required_children
                .iter()
                .map(|c| format!("quest.{c}.state == 'failed'"))
                .collect::<Vec<_>>()
                .join(" || ")
        };
        // §2.2: `<authoredFail>` in parentheses (precedence-safe for any
        // authored expression — `||` binds looser than everything CEL
        // admits, but the parens make the wrapping legible and immune to
        // future operator changes), then one `failed` test per required
        // child, `||`-joined in document order. No authored fail → the
        // bare disjunction (no surrounding parens; the whole slot IS the
        // disjunction).
        match quest.fail.as_mut() {
            Some(slot) if !slot.raw.trim().is_empty() => {
                slot.raw = format!("({}) || {}", slot.raw, disjunction);
            }
            _ => {
                // No authored fail (or a degenerate empty-raw slot):
                // materialize a fresh `CelSlot` at the quest's id span
                // (the most localized span the AST offers a top-level
                // quest — mirrors how `quest_span`/`after_span` fall back
                // to the open-tag span when the attr is absent).
                quest.fail = Some(CelSlot::raw(CelKind::Condition, disjunction, quest.id_span));
            }
        }
    }
}

fn normalize_nodes(
    nodes: &mut Vec<Node>,
    components: &Components<'_>,
    schema: &StateSchema,
    uses: &mut UseOrdinals,
    diags: &mut Vec<Diagnostic>,
) {
    let mut i = 0;
    while i < nodes.len() {
        let is_use = matches!(&nodes[i], Node::Directive(d) if d.tag == "use");
        if is_use {
            let d = match nodes.remove(i) {
                Node::Directive(d) => d,
                other => {
                    // Structurally impossible (guarded above); stay total.
                    nodes.insert(i, other);
                    i += 1;
                    continue;
                }
            };
            let spliced = expand_use(&d, components, schema, uses, diags);
            let n = spliced.len();
            nodes.splice(i..i, spliced);
            i += n; // bodies were normalized recursively — skip past them
            continue;
        }
        // §7.2/§7.4 (D8): a gated content line desugars to a one-arm
        // `<match>` BEFORE expand/stage/address — same identity-preserving
        // idiom as the `is_use` splice above (remove, rebuild, reinsert at
        // the SAME index so the outer loop's `i += 1` below still lands
        // past it). Recursion into `Node::Branch`/`Node::Hub`/`Node::Match`/
        // `Node::On`/`Node::Objective` bodies below already re-enters this
        // function, so a gated line nested in any of those is caught on
        // that recursive call — no extra wiring needed here.
        let is_gated_line = matches!(&nodes[i], Node::Line(l) if l.when.is_some());
        if is_gated_line {
            let line = match nodes.remove(i) {
                Node::Line(l) => l,
                other => {
                    // Structurally impossible (guarded above); stay total.
                    nodes.insert(i, other);
                    i += 1;
                    continue;
                }
            };
            nodes.insert(i, synth_when_match(line));
            i += 1;
            continue;
        }
        // dsl 0.12.0: a GUARDED `::next{to when}` desugars to a one-arm
        // `<match>` the SAME way a gated line does (`synth_when_next_match`
        // mirrors `synth_when_match` exactly, wrapping the directive
        // instead of the line) — so `stage::walk_match`'s existing two-arm
        // lowering handles both fall-through cases with zero new code.
        let is_gated_next = matches!(
            &nodes[i],
            Node::Directive(d) if d.tag == lute_manifest::core::NEXT_DIRECTIVE && d.when.is_some()
        );
        if is_gated_next {
            let d = match nodes.remove(i) {
                Node::Directive(d) => d,
                other => {
                    // Structurally impossible (guarded above); stay total.
                    nodes.insert(i, other);
                    i += 1;
                    continue;
                }
            };
            nodes.insert(i, synth_when_next_match(d));
            i += 1;
            continue;
        }
        match &mut nodes[i] {
            Node::Branch(b) => {
                for c in &mut b.choices {
                    synth_into(c, schema);
                    normalize_nodes(&mut c.body, components, schema, uses, diags);
                }
            }
            Node::Hub(h) => {
                for c in &mut h.choices {
                    synth_into(c, schema);
                    normalize_nodes(&mut c.body, components, schema, uses, diags);
                }
            }
            Node::Match(m) => {
                for arm in &mut m.arms {
                    match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => {
                            normalize_nodes(body, components, schema, uses, diags)
                        }
                    }
                }
            }
            Node::Timeline(t) => {
                // A `ClipNode` is `Directive|Set` (§13/AST), so a `::use` inside a
                // timeline clip cannot be inline-expanded. Fail loud here (before the
                // stage walk) rather than let it reach lowering and be dropped
                // silently (spec-gap note 9). Requires `use lute_syntax::ast::ClipNode;`.
                for track in &t.tracks {
                    for clip in &track.clips {
                        if let ClipNode::Directive(cd) = &clip.node {
                            if cd.tag == "use" {
                                diags.push(Diagnostic {
                                    code: "E-COMPILE-COMPONENT".to_string(),
                                    severity: Severity::Error,
                                    message: "`::use` is not allowed inside a <timeline> clip"
                                        .to_string(),
                                    span: cd.span,
                                    layer: Layer::Content,
                                    fixits: Vec::new(),
                                    provenance: None,
                                    covered: Vec::new(),
                                    related: Vec::new(),
                                });
                            }
                        }
                    }
                }
            }
            Node::On(on) => normalize_nodes(&mut on.body, components, schema, uses, diags),
            Node::Objective(o) => normalize_nodes(&mut o.body, components, schema, uses, diags),
            _ => {}
        }
        i += 1;
    }
}

/// §7.2/§7.4 (D8): `Node::Line{when: Some(g), ..}` → `Node::Match{ subject:
/// g, arms: [When{is: None, test: synthesized "$" Condition slot, body:
/// [the line, when=None]}, Otherwise{body: []}] }` — the guard `g` is
/// HOISTED to be the match subject verbatim (not re-typed to
/// `CelKind::MatchSubject`; downstream lowering (`stage::walk_match`,
/// `expr::synth_arm_expr`) reads only `.raw`, so the slot's `kind` never
/// reaches the artifact) and the arm's `test` is the literal text `"$"` —
/// the arm fires iff the guard itself decides true, exactly
/// `<match on="G"><when test="$">…</when><otherwise/></match>` (§7.4's
/// "MUST lower to that same match record", pinned by
/// `when_sugar::sugared_line_lowers_to_canonical_match_record`). The
/// `<otherwise>` alternative is the sugar's implicit empty else-case
/// (§7.2) — already a legal zero-body arm, no new IR shape. `line.when` is
/// cleared on the nested copy so a re-normalized desugared line can never
/// re-enter this rewrite (idempotent by construction).
fn synth_when_match(mut line: Line) -> Node {
    let guard = line
        .when
        .take()
        .expect("caller guarantees `line.when.is_some()`");
    let span = line.span;
    one_arm_match(guard, Node::Line(line), span)
}

/// dsl 0.12.0: `Node::Directive{tag:"next", when: Some(g), ..}` →
/// `Node::Match{ subject: g, arms: [When{test: "$", body: [the next,
/// when=None]}, Otherwise{body: []}] }` — mirrors [`synth_when_match`]
/// EXACTLY (same hoisted subject, same synthesized `"$"` test arm, same
/// implicit empty `<otherwise>` fall-through), so `stage::walk_match` lowers
/// a guarded `::next` through the IDENTICAL two-arm machinery a gated line
/// already uses — no new lowering code. `d.when` is cleared on the nested
/// copy so a re-normalized desugared `::next` can never re-enter this
/// rewrite (idempotent by construction, mirrors `synth_when_match`).
fn synth_when_next_match(mut d: Directive) -> Node {
    let guard = d.when.take().expect("caller guarantees `d.when.is_some()`");
    let span = d.span;
    one_arm_match(guard, Node::Directive(d), span)
}

/// dsl 0.24.0 §1: `Node::Set{when: Some(g), ..}` → the same one-arm
/// `<match on="g"><when test="$">::set{…}</when><otherwise/></match>` as
/// [`synth_when_match`], so the IR, `lute run`/`lute play` and `lute trace`
/// all apply the write exactly when the guard holds with no new record
/// shape. `s.when` is cleared on the nested copy (idempotent).
///
/// Unlike the line/`::next` sugar this runs from `expand::expand_nodes`, NOT
/// [`normalize_nodes`]: a `::set` RHS may read an enclosing `<match>`'s `$`,
/// which must expand against THAT subject before the write is wrapped in a
/// match of its own (whose `$` is the guard).
pub(crate) fn synth_when_set_match(mut s: Set) -> Node {
    let guard = s.when.take().expect("caller guarantees `s.when.is_some()`");
    let span = s.span;
    one_arm_match(guard, Node::Set(s), span)
}

/// The canonical guard desugar shared by every `when=`-sugar above:
/// `guard` hoisted verbatim as the subject, one `<when test="$">` arm
/// holding `node`, and the implicit empty `<otherwise>` fall-through.
fn one_arm_match(guard: CelSlot, node: Node, span: Span) -> Node {
    let test = CelSlot::raw(CelKind::Condition, "$".to_string(), span);
    Node::Match(Match {
        subject: guard,
        // Synthesized, not authored: no residual attributes exist.
        attrs: Vec::new(),
        arms: vec![
            Arm::When {
                is: None,
                test,
                attrs: Vec::new(),
                body: vec![node],
                span,
            },
            Arm::Otherwise {
                attrs: Vec::new(),
                body: Vec::new(),
                span,
            },
        ],
        span,
    })
}

/// `::use{component="name" <arg>=…}` → `[begin, …bound body…, end]`.
fn expand_use(
    d: &Directive,
    components: &Components<'_>,
    schema: &StateSchema,
    uses: &mut UseOrdinals,
    diags: &mut Vec<Diagnostic>,
) -> Vec<Node> {
    let name = d
        .attrs
        .iter()
        .find(|a| a.key == "component")
        .and_then(|a| match &a.value {
            AttrValue::Str(s) => Some(s.clone()),
            _ => None,
        });
    let Some(def) = name.as_deref().and_then(|n| components.set.table.get(n)) else {
        // Gate-proven unreachable (E-COMPONENT-UNDECLARED); degrade.
        diags.push(Diagnostic {
            code: "E-COMPILE-COMPONENT".to_string(),
            severity: Severity::Error,
            message: "`::use` names no resolvable component (gate should have caught this)"
                .to_string(),
            span: d.span,
            layer: Layer::Content,
            fixits: Vec::new(),
            provenance: None,
            covered: Vec::new(),
            related: Vec::new(),
        });
        return Vec::new();
    };
    let name = name.unwrap_or_default();
    let args = use_args(d);
    // Defensive arg/param validation (checker gate: E-COMPONENT-ARG). The
    // invocation's arg key set MUST match `def.params` exactly — no missing,
    // no extra. Compile gates on a clean check, so reaching here with a
    // mismatch is gate-proven unreachable; degrade fail-loud (like the
    // unresolvable-component arm) rather than expand with an unbound `@param`.
    let missing: Vec<&str> = def
        .params
        .iter()
        .map(|(p, _)| p.as_str())
        .filter(|p| !args.contains_key(*p))
        .collect();
    let extra: Vec<&str> = args
        .keys()
        .map(String::as_str)
        .filter(|k| !def.params.iter().any(|(p, _)| p == k))
        .collect();
    if !missing.is_empty() || !extra.is_empty() {
        let mut parts = Vec::new();
        if !missing.is_empty() {
            parts.push(format!("missing [{}]", missing.join(", ")));
        }
        if !extra.is_empty() {
            parts.push(format!("unknown [{}]", extra.join(", ")));
        }
        diags.push(Diagnostic {
            code: "E-COMPILE-COMPONENT".to_string(),
            severity: Severity::Error,
            message: format!(
                "`::use` args for component `{name}` do not match its params: {} (gate should have caught this)",
                parts.join("; ")
            ),
            span: d.span,
            layer: Layer::Content,
            fixits: Vec::new(),
            provenance: None,
            covered: Vec::new(),
            related: Vec::new(),
        });
        return Vec::new();
    }
    let mut body: Vec<Node> = def
        .body
        .shots
        .iter()
        .flat_map(|s| s.body.iter().cloned())
        .collect();
    backfill_component_codes(&mut body);
    // dsl 0.24.0 §4: `{{@p}}` over a `speaker` param renders the cast
    // member's NAME; every other position (attrs, match subjects) binds the
    // id below.
    let names = speaker_display_args(def, &args, components.cast);
    if !names.is_empty() {
        visit_lines(&mut body, &mut |l| bind_text(l, &names));
    }
    bind_params(&mut body, &args, &def.params);
    // Nested `::use` in the body expands recursively (acyclic per checker);
    // this expansion is the nested uses' host, so they count from 1 afresh.
    normalize_nodes(&mut body, components, schema, &mut UseOrdinals::new(), diags);
    // §6.4: static selection / residual dispatch for any param-scoped
    // `<match>` in the bound body — runs ONLY here, on this clone (B2).
    fold_component_matches(&mut body, schema);

    let ordinal = uses.entry(name.clone()).or_insert(0);
    *ordinal += 1;
    let scope = format!("{name}#{ordinal}");
    let span = d.span;
    let attr = |key: &str, value: String| Attr {
        key: key.to_string(),
        value: AttrValue::Str(value),
        value_span: span,
        span,
    };
    let begin = Node::Directive(Directive {
        tag: COMPONENT_BEGIN.to_string(),
        attrs: vec![attr("component", name), attr(COMPONENT_SCOPE_ATTR, scope)],
        when: None,
        span,
    });
    let end = Node::Directive(Directive {
        tag: COMPONENT_END.to_string(),
        attrs: Vec::new(),
        when: None,
        span,
    });
    let mut out = Vec::with_capacity(body.len() + 2);
    out.push(begin);
    out.append(&mut body);
    out.push(end);
    out
}

/// Bind `@param` uses to `::use` args. A whole-slot `@param` attr value is
/// replaced VALUE-LEVEL (a string arg becomes a plain `Str` attr — what a
/// string-typed attr position needs); a `@param` inside a larger CEL is
/// substituted textually, typed by the param's declared [`Type`].
fn bind_params(nodes: &mut [Node], args: &BTreeMap<String, AttrValue>, params: &[(String, Type)]) {
    for node in nodes {
        match node {
            Node::Line(l) => {
                // T11 fix: a component-body gated line's `when=` slot is a
                // CEL fragment just like any attr — it must see the SAME
                // `@param` -> arg substitution `l.attrs` gets, or the T11
                // desugar (and T8's `fold_component_matches`, which runs
                // right after this on the same bound clone) would fold/
                // decide against an unbound `@tier`-shaped marker instead
                // of the caller's actual argument text.
                if let Some(w) = &mut l.when {
                    bind_slot_raw(w, args, params);
                }
                bind_attrs(&mut l.attrs, args, params);
                bind_text(l, args);
            }
            Node::Directive(d) => bind_attrs(&mut d.attrs, args, params),
            Node::Set(s) => {
                bind_slot(&mut s.expr, args, params);
                if let Some(w) = &mut s.when {
                    bind_slot(w, args, params);
                }
                lute_check::component_effects::bind_set_path(&mut s.path, args);
            }
            Node::Branch(b) => {
                for c in &mut b.choices {
                    if let Some(w) = &mut c.when {
                        bind_slot(w, args, params);
                    }
                    bind_attrs(&mut c.attrs, args, params);
                    bind_params(&mut c.body, args, params);
                }
            }
            Node::Match(m) => {
                bind_slot(&mut m.subject, args, params);
                for arm in &mut m.arms {
                    match arm {
                        Arm::When { test, body, .. } => {
                            bind_slot(test, args, params);
                            bind_params(body, args, params);
                        }
                        Arm::Otherwise { body, .. } => bind_params(body, args, params),
                    }
                }
            }
            Node::Timeline(t) => {
                for track in &mut t.tracks {
                    for clip in &mut track.clips {
                        match &mut clip.node {
                            ClipNode::Directive(d) => bind_attrs(&mut d.attrs, args, params),
                            ClipNode::Set(s) => bind_slot(&mut s.expr, args, params),
                        }
                    }
                }
            }
            Node::Hub(h) => {
                for c in &mut h.choices {
                    if let Some(w) = &mut c.when {
                        bind_slot(w, args, params);
                    }
                    bind_attrs(&mut c.attrs, args, params);
                    bind_params(&mut c.body, args, params);
                }
            }
            Node::On(on) => {
                if let Some(w) = &mut on.when {
                    bind_slot(w, args, params);
                }
                bind_attrs(&mut on.attrs, args, params);
                bind_params(&mut on.body, args, params);
            }
            Node::Objective(o) => {
                bind_slot(&mut o.done, args, params);
                if let Some(w) = &mut o.when {
                    bind_slot(w, args, params);
                }
                for deadline in o.by.iter_mut().chain(o.until.iter_mut()) {
                    bind_slot(deadline, args, params);
                }
                bind_attrs(&mut o.attrs, args, params);
                bind_params(&mut o.body, args, params);
            }
            // dsl 0.24.0 §4: an `effects: true` body's `@param` fact
            // arguments take their `::use` constants (the checker rejects a
            // non-constant argument, so a bound build leaves none).
            Node::Assert(a) => {
                lute_check::component_effects::bind_fact(&mut a.pattern, args);
            }
            Node::Retract(r) => {
                lute_check::component_effects::bind_fact(&mut r.pattern, args);
            }
        }
    }
}

fn bind_slot(slot: &mut CelSlot, args: &BTreeMap<String, AttrValue>, params: &[(String, Type)]) {
    bind_slot_raw(slot, args, params);
}

/// dsl §13 / 0.21.1 T1-3: bind a component body line's `{{@param}}`
/// interpolations to the `::use` args. A param is a compile-time constant, so
/// a literal arg is spliced into `text` verbatim and its interp dropped — each
/// expansion ships its OWN text (`Outside: grey.` / `Outside: still.`), never
/// a ref to a param that no longer exists. A `@ref` arg (a caller-side def)
/// stays an interpolation, rebound to the caller's ref text (`{{@wdef}}`), so
/// it renders through the ordinary def-placeholder path.
///
/// Markers are walked with the parser's own scan rule (`\{{` is a literal,
/// `{{…}}` pairs left to right), so the k-th marker IS `l.interps[k]`.
///
/// dsl 0.24.0 T1-12: every surviving interp's `span` is recomputed against
/// the REWRITTEN `text` (base `l.text_span`), since a rebound `{{@def}}` and a
/// spliced literal change the text's length — consumers slice `l.text` by
/// `interp.span - text_span.byte_start` (trace's `render_line_text`).
fn bind_text(l: &mut Line, args: &BTreeMap<String, AttrValue>) {
    let bound = |interp: &Interp| -> Option<&AttrValue> {
        (interp.kind == InterpKind::Ref)
            .then(|| interp.raw.strip_prefix('@'))
            .flatten()
            .and_then(|name| args.get(name))
    };
    if !l.interps.iter().any(|i| bound(i).is_some()) {
        return;
    }
    let base = l.text_span.byte_start;
    let text = &l.text;
    let b = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    // Each kept interp with its `[start, end)` in `out`.
    let mut kept: Vec<(Interp, usize, usize)> = Vec::with_capacity(l.interps.len());
    let mut interps = std::mem::take(&mut l.interps).into_iter();
    let (mut j, mut copied) = (0, 0);
    while j + 1 < b.len() {
        if b[j] == b'\\' && text[j + 1..].starts_with("{{") {
            j += 3;
            continue;
        }
        if b[j] == b'{' && b[j + 1] == b'{' {
            let Some(rel) = text[j + 2..].find("}}") else {
                break;
            };
            let end = j + 2 + rel + 2;
            let Some(mut interp) = interps.next() else {
                break;
            };
            out.push_str(&text[copied..j]);
            copied = end;
            let at = out.len();
            match bound(&interp).cloned() {
                Some(AttrValue::Ref(slot)) => {
                    out.push_str("{{");
                    out.push_str(&slot.raw);
                    // dsl 0.24.0 §4: the rebound marker keeps its hint.
                    if let Some(format) = &interp.format {
                        out.push(':');
                        out.push_str(format);
                    }
                    out.push_str("}}");
                    interp.kind = classify_interp(&slot.raw);
                    interp.raw = slot.raw;
                    kept.push((interp, at, out.len()));
                }
                Some(AttrValue::Str(s)) => {
                    // dsl 0.24.0 §4: no placeholder survives a literal splice
                    // to carry the hint, so it applies here — `ordinal` /
                    // `ordinalWord` on a number literal renders its ordinal.
                    let ordinal = interp
                        .format
                        .as_deref()
                        .zip(s.trim().parse::<f64>().ok())
                        .and_then(|(f, n)| lute_syntax::ast::format_number(f, n));
                    out.push_str(ordinal.as_deref().unwrap_or(&s));
                }
                Some(AttrValue::BoolTrue) => out.push_str("true"),
                None => {
                    out.push_str(&text[j..end]);
                    kept.push((interp, at, out.len()));
                }
            }
            j = end;
            continue;
        }
        j += 1;
    }
    // Markers the scan never reached keep their offset past `copied`, shifted
    // by the length change so far (the parser only emits well-formed pairs,
    // so this is defensive).
    let shift = |at: usize| (at + out.len()).saturating_sub(copied);
    for interp in interps {
        let (s, e) = (interp.span.byte_start, interp.span.byte_end);
        let (s, e) = (shift(s.saturating_sub(base)), shift(e.saturating_sub(base)));
        kept.push((interp, s, e));
    }
    out.push_str(&text[copied..]);
    l.interps = kept
        .into_iter()
        .map(|(mut interp, start, end)| {
            interp.span = text_sub_span(l.text_span, &out, start, end);
            interp
        })
        .collect();
    l.text = out;
}

/// The span of `text[start..end]` for a line text beginning at `text_span`
/// (single-line: `line` is the text's, `column`/UTF-16 advance by the prefix).
fn text_sub_span(text_span: Span, text: &str, start: usize, end: usize) -> Span {
    let (start, end) = (start.min(text.len()), end.min(text.len()));
    let u16_len = |s: &str| s.chars().map(|c| c.len_utf16() as u32).sum::<u32>();
    let u16_start = text_span.utf16_range.0 + u16_len(text.get(..start).unwrap_or(""));
    Span {
        byte_start: text_span.byte_start + start,
        byte_end: text_span.byte_start + end,
        line: text_span.line,
        column: text_span.column + start as u32,
        utf16_range: (
            u16_start,
            u16_start + u16_len(text.get(start..end).unwrap_or("")),
        ),
    }
}

/// `<choice … into="run.<path>" [value="<lit>"]>` → append
/// `Node::Set(run.<path> = <value>)` (dsl 0.6.0 §2.1). The record sugar is
/// now triggered by the presence of a well-formed `into=` alone — `persist=`
/// was removed from the language (a `persist=`-carrying doc is gated out by
/// E-PERSIST-REMOVED before compile ever runs). Well-formedness is
/// gate-proven (0.6.0 §2.2: `E-INTO-*`); anything unresolvable here is
/// skipped, total.
fn synth_into(choice: &mut Choice, schema: &StateSchema) {
    let find = |k: &str| choice.attrs.iter().find(|a| a.key == k);
    let Some(AttrValue::Str(into_path)) = find("into").map(|a| &a.value) else {
        return; // no `into=` → not the record sugar
    };
    let into_path = into_path.clone();
    let Some(decl) = schema.decls.get(into_path.as_str()) else {
        return; // gate: E-INTO-TARGET / E-INTO-UNDECLARED
    };
    let value = find("value").and_then(|a| match &a.value {
        AttrValue::Str(s) => Some(s.clone()),
        AttrValue::BoolTrue => Some("true".to_string()),
        AttrValue::Ref(_) => None, // gate: E-INTO-VALUE
    });
    let cel = into_value_cel(&decl.ty, value.as_deref());
    let span = find("into").map(|a| a.span).unwrap_or(choice.span);
    push_set(choice, into_path, cel, span);
}

fn push_set(choice: &mut Choice, path: String, cel: String, span: Span) {
    choice.body.push(Node::Set(Set {
        path,
        path_span: span,
        op: "=".to_string(),
        expr: CelSlot::raw(CelKind::SetExpr, cel, span),
        span,
        when: None,
    }));
}

/// dsl §11.1.1 rule 4: bool target's value is optional (defaults `true`);
/// number stays bare; everything else (enum/str) is a CEL string literal.
fn into_value_cel(ty: &Type, value: Option<&str>) -> String {
    match ty {
        Type::Bool => value.unwrap_or("true").to_string(),
        Type::Number => value.unwrap_or("0").to_string(),
        _ => cel_string_literal(value.unwrap_or_default()),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use lute_check::meta::{Namespace, StateDecl, StateSchema};
    use lute_check::resolve_components;
    use lute_core_span::Severity;
    use lute_manifest::types::{Literal, Type};
    use lute_syntax::ast::{AttrValue, Node};

    use super::*;

    fn parse_clean(src: &str) -> lute_syntax::ast::Document {
        let (doc, diags) = lute_syntax::parse(src);
        assert!(
            diags.iter().all(|d| d.severity != Severity::Error),
            "{diags:#?}"
        );
        doc
    }

    #[test]
    fn use_expands_component_inline_with_bound_params_and_sentinels() {
        // Real fixture: docs/examples/components/greet.component.lute declares
        // `component: greet`, `params: { who: string }`, body =
        // `::auto{character=@who action="fade-in-up"}` + a narrator line.
        let base = Path::new("../../docs/examples/components");
        let scene = std::fs::read_to_string(base.join("scene.lute")).unwrap();
        let mut doc = parse_clean(&scene);
        let comps = resolve_components(base, &["greet.component.lute".to_string()], doc.meta.span);
        assert!(comps.diags.is_empty(), "{:#?}", comps.diags);
        let diags = normalize_document(&mut doc, &comps, &Default::default(), &StateSchema::default());
        assert!(diags.is_empty(), "{diags:#?}");

        let body = &doc.shots[0].body;
        // ::use replaced by: begin sentinel, ::auto (param bound), line, end sentinel, then the scene's own line.
        let tags: Vec<String> = body
            .iter()
            .map(|n| match n {
                Node::Directive(d) => format!("::{}", d.tag),
                Node::Line(l) => format!("@{}", l.speaker),
                _ => "other".to_string(),
            })
            .collect();
        assert_eq!(
            tags,
            vec![
                format!("::{COMPONENT_BEGIN}"),
                "::auto".to_string(),
                "@narrator".to_string(),
                format!("::{COMPONENT_END}"),
                "@narrator".to_string(),
            ]
        );
        // `character=@who` became the VALUE-LEVEL string arg (whole-slot bind).
        let Node::Directive(auto) = &body[1] else {
            panic!("auto")
        };
        let ch = auto.attrs.iter().find(|a| a.key == "character").unwrap();
        assert!(
            matches!(&ch.value, AttrValue::Str(s) if s == "marina"),
            "{ch:?}"
        );
        // No `::use` survives normalization (D8).
        assert!(body
            .iter()
            .all(|n| !matches!(n, Node::Directive(d) if d.tag == "use")));
    }

    #[test]
    fn use_inside_a_timeline_clip_fails_loud() {
        // A `::use` clip cannot be inline-expanded (a ClipNode is Directive|Set),
        // so normalization emits E-COMPILE-COMPONENT rather than dropping it
        // silently (spec-gap note 9); compile() then aborts at the §5 diag gate,
        // so no artifact is produced. RED before the Timeline arm in normalize_nodes.
        let src = r#"---
kind: scene
character: x
season: 1
episode: 1
---
## Shot 1.
<timeline>
  <track channel="fg">
    ::use{component="greet"}
  </track>
</timeline>
"#;
        let mut doc = parse_clean(src);
        let comps = resolve_components(Path::new("."), &[], doc.meta.span);
        let diags = normalize_document(&mut doc, &comps, &Default::default(), &StateSchema::default());
        assert!(
            diags.iter().any(|d| d.code == "E-COMPILE-COMPONENT"),
            "expected E-COMPILE-COMPONENT for a ::use timeline clip, got {diags:#?}"
        );
    }

    #[test]
    fn use_with_mismatched_args_fails_loud_no_unbound_param_leaks() {
        // Defensive backstop for the checker's E-COMPONENT-ARG: greet declares
        // `params: { who: string }`, but this `::use` supplies no `who` and an
        // unknown `extra` arg. Normalization must degrade to E-COMPILE-COMPONENT
        // rather than expand the body with an unbound `@who` — no component
        // sentinels, no spliced `::auto`, no residual `::use`.
        let base = Path::new("../../docs/examples/components");
        let src = r#"---
kind: scene
character: demo
season: 1
episode: 1
components: [greet.component.lute]
---

## Shot 1.

::use{component="greet" extra="oops"}
@narrator: And the scene carries on.
"#;
        let mut doc = parse_clean(src);
        let comps = resolve_components(base, &["greet.component.lute".to_string()], doc.meta.span);
        assert!(comps.diags.is_empty(), "{:#?}", comps.diags);
        let diags = normalize_document(&mut doc, &comps, &Default::default(), &StateSchema::default());
        let arg_err = diags
            .iter()
            .find(|d| d.code == "E-COMPILE-COMPONENT" && d.severity == Severity::Error);
        assert!(
            arg_err.is_some(),
            "expected E-COMPILE-COMPONENT for mismatched ::use args, got {diags:#?}"
        );
        // Message names the component and both the missing and the extra arg.
        let msg = &arg_err.unwrap().message;
        assert!(
            msg.contains("greet") && msg.contains("who") && msg.contains("extra"),
            "diagnostic should name component + mismatched params: {msg:?}"
        );
        // No expansion leaked: no sentinels, no `::auto` body, no `::use` remnant.
        let body = &doc.shots[0].body;
        assert!(
            body.iter().all(|n| !matches!(n, Node::Directive(d)
                if d.tag == COMPONENT_BEGIN
                    || d.tag == COMPONENT_END
                    || d.tag == "auto"
                    || d.tag == "use")),
            "no component body should splice in on arg mismatch: {body:#?}"
        );
    }

    #[test]
    fn into_synthesizes_trailing_set_nodes() {
        let src = r#"---
kind: scene
character: elena
season: 1
episode: 1
---

## Shot 1.

<branch id="sofaHelp">
  <choice id="help" label="Help her up" into="run.metHelpfully">
    @elena: Thank you.
  </choice>
  <choice id="warmly" label="Stay a while" into="run.outcome" value="warm">
    @elena: Kind.
  </choice>
  <choice id="tip" label="Leave a tip" into="run.tip" value="5">
    @elena: Oh.
  </choice>
</branch>
"#;
        let mut doc = parse_clean(src);
        let mut schema = StateSchema::default();
        schema.decls.insert(
            "run.metHelpfully".to_string(),
            StateDecl {
                ty: Type::Bool,
                default: Some(Literal::Bool(false)),
                namespace: Namespace::Run,
                owner: None,
            },
        );
        schema.decls.insert(
            "run.outcome".to_string(),
            StateDecl {
                ty: Type::Enum(vec!["warm".into(), "cold".into()]),
                default: None,
                namespace: Namespace::Run,
                owner: None,
            },
        );
        schema.decls.insert(
            "run.tip".to_string(),
            StateDecl {
                ty: Type::Number,
                default: None,
                namespace: Namespace::Run,
                owner: None,
            },
        );
        let diags = normalize_document(&mut doc, &Default::default(), &Default::default(), &schema);
        assert!(diags.is_empty(), "{diags:#?}");

        let Node::Branch(b) = &doc.shots[0].body[0] else {
            panic!("branch")
        };
        let last_set = |i: usize| -> (&str, &str, &str) {
            let Some(Node::Set(s)) = b.choices[i].body.last() else {
                panic!("choice {i} ends in a synthesized Set");
            };
            (s.path.as_str(), s.op.as_str(), s.expr.raw.as_str())
        };
        // bool target, no value => `= true` (dsl §11.1.1 rule 4).
        assert_eq!(last_set(0), ("run.metHelpfully", "=", "true"));
        // enum target => quoted CEL string literal.
        assert_eq!(last_set(1), ("run.outcome", "=", "'warm'"));
        // number target => bare numeric literal.
        assert_eq!(last_set(2), ("run.tip", "=", "5"));
        // The authored line plus exactly one synthesized Set per persisting choice.
        assert_eq!(b.choices[0].body.len(), 2);
    }

    fn tag_of(n: &Node) -> String {
        match n {
            Node::Directive(d) => format!("::{}", d.tag),
            Node::Line(l) => format!("@{}", l.speaker),
            Node::Objective(_) => "objective".to_string(),
            Node::On(_) => "on".to_string(),
            _ => "other".to_string(),
        }
    }

    /// No `::use` survives normalization anywhere, including nested
    /// `<on>`/`<objective>` bodies.
    fn no_use_directive(nodes: &[Node]) -> bool {
        nodes.iter().all(|n| match n {
            Node::Directive(d) => d.tag != "use",
            Node::On(o) => no_use_directive(&o.body),
            Node::Objective(o) => no_use_directive(&o.body),
            _ => true,
        })
    }

    // Plan D review (Important finding 1): `normalize_nodes` fell into
    // `_ => {}` for `Node::On`/`Node::Objective`, so a `::use` inside an
    // `<on>`/`<objective>` body was never inline-expanded — it survived as a
    // literal `::use` directive, which `lower_directive` silently drops at
    // lowering time (the component's content never reaches the artifact).
    #[test]
    fn normalize_document_traverses_quest_bodies_expanding_use_in_on_and_objective() {
        let base = Path::new("../../docs/examples/components");
        let src = r#"---
kind: quest
components: [greet.component.lute]
---

<quest id="q1">
<objective id="o1" done="true"/>

::use{component="greet" who="marina"}

<on event="questComplete">
::use{component="greet" who="halsin"}
</on>
</quest>
"#;
        let mut doc = parse_clean(src);
        assert_eq!(doc.quests.len(), 1, "fixture must parse one <quest>");
        let comps = resolve_components(base, &["greet.component.lute".to_string()], doc.meta.span);
        assert!(comps.diags.is_empty(), "{:#?}", comps.diags);
        let diags = normalize_document(&mut doc, &comps, &Default::default(), &StateSchema::default());
        assert!(diags.is_empty(), "{diags:#?}");

        let quest = &doc.quests[0];
        let tags: Vec<String> = quest.body.iter().map(tag_of).collect();
        assert_eq!(
            tags,
            vec![
                "objective".to_string(),
                format!("::{COMPONENT_BEGIN}"),
                "::auto".to_string(),
                "@narrator".to_string(),
                format!("::{COMPONENT_END}"),
                "on".to_string(),
            ]
        );
        let Node::On(on) = &quest.body[5] else {
            panic!("expected on, got {:?}", quest.body.get(5));
        };
        let on_tags: Vec<String> = on.body.iter().map(tag_of).collect();
        assert_eq!(
            on_tags,
            vec![
                format!("::{COMPONENT_BEGIN}"),
                "::auto".to_string(),
                "@narrator".to_string(),
                format!("::{COMPONENT_END}"),
            ],
            "the ::use inside <on> must expand, not be dropped silently"
        );
        assert!(no_use_directive(&quest.body));
    }

    // Plan D review (Important finding 1, `bind_params`): line 270 was
    // `Node::On(_) | Node::Objective(_) => {}`, so a component whose OWN
    // body declares an `<on>`/`<objective>` guarded by `@param` leaked the
    // unbound `@param` into the spliced quest content instead of the
    // resolved `::use` arg.
    #[test]
    fn use_binds_params_inside_component_on_and_objective_bodies() {
        let component_src = r#"---
component: reactor
params:
  n: number
---

## Scene 1.

<on event="questComplete" when="@n > 0" foo=@n>
::set{run.score = run.score + @n}
</on>
<objective id="bonus" done="@n > 3" when="@n > 1">
::set{run.bonus = @n}
</objective>
"#;
        let comp_doc = parse_clean(component_src);
        let mut table = BTreeMap::new();
        table.insert(
            "reactor".to_string(),
            lute_check::ComponentDef {
                params: vec![("n".to_string(), Type::Number)],
                speakers: Vec::new(),
                effects: false,
                body: comp_doc,
                src: std::path::PathBuf::from("test://reactor"),
            },
        );
        let comps = ComponentSet {
            table,
            diags: Vec::new(),
        };

        let src = r#"---
kind: quest
---

<quest id="q1">
<objective id="o1" done="true"/>

::use{component="reactor" n=5}
</quest>
"#;
        let mut doc = parse_clean(src);
        let diags = normalize_document(&mut doc, &comps, &Default::default(), &StateSchema::default());
        assert!(diags.is_empty(), "{diags:#?}");

        let quest = &doc.quests[0];
        let Node::On(on) = &quest.body[2] else {
            panic!("expected on, got {:?}", quest.body.get(2));
        };
        let when = on.when.as_ref().expect("on.when");
        assert_eq!(when.raw, "5 > 0");
        assert!(!when.raw.contains('@'));
        let foo = on.attrs.iter().find(|a| a.key == "foo").expect("foo attr");
        assert!(
            matches!(&foo.value, AttrValue::Str(s) if s == "5"),
            "{foo:?}"
        );
        let Node::Set(s) = &on.body[0] else {
            panic!("expected set, got {:?}", on.body.first());
        };
        assert_eq!(s.expr.raw, "run.score + 5");

        let Node::Objective(obj) = &quest.body[3] else {
            panic!("expected objective, got {:?}", quest.body.get(3));
        };
        assert_eq!(obj.done.raw, "5 > 3");
        assert!(!obj.done.raw.contains('@'));
        let owhen = obj.when.as_ref().expect("objective.when");
        assert_eq!(owhen.raw, "5 > 1");
        let Node::Set(s2) = &obj.body[0] else {
            panic!("expected set, got {:?}", obj.body.first());
        };
        assert_eq!(s2.expr.raw, "5");
    }

    fn normalize_quests(src: &str) -> lute_syntax::ast::Document {
        let mut doc = parse_clean(src);
        let comps = resolve_components(Path::new("."), &[], doc.meta.span);
        assert!(comps.diags.is_empty(), "{:#?}", comps.diags);
        let diags = normalize_document(&mut doc, &comps, &Default::default(), &StateSchema::default());
        // Subquest synthesis is pure text: it never produces diagnostics of
        // its own (structural / cross-doc violations are the checker's job).
        assert!(diags.is_empty(), "{diags:#?}");
        doc
    }

    fn quest_by_id<'a>(
        doc: &'a lute_syntax::ast::Document,
        id: &str,
    ) -> &'a lute_syntax::ast::Quest {
        doc.quests
            .iter()
            .find(|q| q.id == id)
            .expect("quest present")
    }

    fn objective_by_id<'a>(
        quest: &'a lute_syntax::ast::Quest,
        id: &str,
    ) -> &'a lute_syntax::ast::Objective {
        quest
            .body
            .iter()
            .find_map(|n| match n {
                Node::Objective(o) if o.id == id => Some(o),
                _ => None,
            })
            .expect("objective present")
    }

    /// §2.1: an empty-`done` `<objective quest="c"/>` receives the exact
    /// synthesized completion predicate (single-quoted, no whitespace
    /// variation) so `lute-trace` and any engine reading the wire text see
    /// the identical bytes the plan pins.
    #[test]
    fn subquest_done_raw_is_synthesized_verbatim() {
        let src = r#"---
kind: quest
luteVersion: "0.14.0"
---
<quest id="parent" title="Parent">
  <objective id="findChild" quest="child"/>
</quest>
<quest id="child" title="Child">
  <objective id="reach" done="true"/>
</quest>
"#;
        let doc = normalize_quests(src);
        let obj = objective_by_id(quest_by_id(&doc, "parent"), "findChild");
        assert_eq!(obj.done.raw, "quest.child.state == 'complete'");
        assert_eq!(obj.quest.as_deref(), Some("child"));
    }

    /// §2.2 no-authored-fail case: a required subquest child materializes
    /// a brand-new `fail` slot carrying the bare disjunction (no wrapping
    /// parens — the whole slot IS the disjunction).
    #[test]
    fn subquest_fail_synthesizes_from_scratch_when_unauthored() {
        let src = r#"---
kind: quest
luteVersion: "0.14.0"
---
<quest id="parent">
  <objective id="a" quest="c1"/>
  <objective id="b" quest="c2"/>
</quest>
<quest id="c1">
  <objective id="x" done="true"/>
</quest>
<quest id="c2">
  <objective id="y" done="true"/>
</quest>
"#;
        let doc = normalize_quests(src);
        let parent = quest_by_id(&doc, "parent");
        let fail = parent.fail.as_ref().expect("fail synthesized");
        assert_eq!(
            fail.raw,
            "quest.c1.state == 'failed' || quest.c2.state == 'failed'"
        );
    }

    /// §2.2 authored-fail case: authored text is wrapped in parens, then
    /// the required-child disjunction is appended in document order.
    /// Optional children contribute nothing.
    #[test]
    fn subquest_fail_wraps_authored_and_appends_required_children_only() {
        let src = r#"---
kind: quest
luteVersion: "0.14.0"
state:
  run.dead: { type: bool, default: false }
---
<quest id="parent" fail="run.dead">
  <objective id="a" quest="c1"/>
  <objective id="opt" quest="c2" optional/>
  <objective id="b" quest="c3"/>
</quest>
<quest id="c1">
  <objective id="x" done="true"/>
</quest>
<quest id="c2">
  <objective id="y" done="true"/>
</quest>
<quest id="c3">
  <objective id="z" done="true"/>
</quest>
"#;
        let doc = normalize_quests(src);
        let parent = quest_by_id(&doc, "parent");
        let fail = parent.fail.as_ref().expect("fail present");
        // Optional child (`c2`) does NOT appear; required siblings appear
        // in document order (`c1` then `c3`).
        assert_eq!(
            fail.raw,
            "(run.dead) || quest.c1.state == 'failed' || quest.c3.state == 'failed'"
        );
    }

    /// A quest whose only subquest child is `optional` gets no fail
    /// synthesis (optional failure does not gate parent completion, §2.1
    /// tail note, §2.2 required-only rule).
    #[test]
    fn subquest_optional_only_child_leaves_fail_untouched() {
        let src = r#"---
kind: quest
luteVersion: "0.14.0"
---
<quest id="parent">
  <objective id="opt" quest="c1" optional/>
</quest>
<quest id="c1">
  <objective id="x" done="true"/>
</quest>
"#;
        let doc = normalize_quests(src);
        assert!(quest_by_id(&doc, "parent").fail.is_none());
    }

    /// A quest with no subquest objectives at all is byte-identical to
    /// pre-subquest normalization: no `fail` synthesized, no `done`
    /// rewrites — the byte-stability guarantee of §3 in the design doc.
    #[test]
    fn quests_without_subquest_children_untouched() {
        let src = r#"---
kind: quest
luteVersion: "0.14.0"
state:
  run.here: { type: bool, default: false }
---
<quest id="plain">
  <objective id="a" done="run.here"/>
</quest>
"#;
        let doc = normalize_quests(src);
        let q = quest_by_id(&doc, "plain");
        assert!(q.fail.is_none());
        let obj = objective_by_id(q, "a");
        assert_eq!(obj.done.raw, "run.here");
        assert!(obj.quest.is_none());
    }

    /// A non-empty authored `done` on a `quest=` objective is the
    /// `E-OBJECTIVE-QUEST-DONE` collision — the checker owns it, and this
    /// pass MUST NOT clobber the authored text (would erase the collision
    /// for downstream tools re-running lower stages on the same AST).
    #[test]
    fn subquest_leaves_authored_done_untouched() {
        let src = r#"---
kind: quest
luteVersion: "0.14.0"
state:
  run.override: { type: bool, default: false }
---
<quest id="parent">
  <objective id="a" quest="c1" done="run.override"/>
</quest>
<quest id="c1">
  <objective id="x" done="true"/>
</quest>
"#;
        let doc = normalize_quests(src);
        let obj = objective_by_id(quest_by_id(&doc, "parent"), "a");
        assert_eq!(obj.done.raw, "run.override");
    }
}
