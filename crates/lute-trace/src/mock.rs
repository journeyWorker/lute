//! The `--mock` surface (dsl 0.4.0 §4.3): parsing a `--mock <file.yaml>`
//! document, composing it with the CLI's own `--state`/`--fact`/`--choose`/
//! `--event` flags, and STRUCTURAL pre-walk validation — ids, arity, types,
//! declaredness.
//!
//! [`validate`] deliberately does NOT evaluate a forced choice's GUARD: §4.4
//! eligibility depends on in-flow writes the walk (Task 19) has not applied
//! yet, so a `--choose` naming a real branch/hub + choice id passes here
//! regardless of what that choice's `when=` would decide once the walk
//! actually reaches it. Only an id that is entirely absent from the walked
//! document is `E-TRACE-CHOICE` at this stage (§4.3's own text: "a forced
//! choice's *guard* is deliberately NOT evaluated here").
//!
//! D18: `E-TRACE-MOCK-FACT` reuses [`lute_check::check_atom`] — the ONE atom/
//! pattern closure checker (`lute-check/src/rel_schema.rs`) — for the
//! unknown-relation/arity/foreign-arg checks a `--fact` needs, re-coding
//! every diagnostic it produces under this module's one code. `check_atom`
//! ALONE (never `lute_check::check_assert`'s write-policy layer, which is
//! what rejects a document's OWN `::assert` of a `derive:true`/`reserved:
//! true` relation) is exactly why such a relation is a LEGAL mock fact — a
//! mock is a *supplied answer*, never a content write (§4.3).

use std::collections::{BTreeMap, BTreeSet};

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_check::{check_atom, FoldedEnv};
use lute_manifest::types::{type_accepts, Literal, Type};
use lute_syntax::ast::{Arm, AttrValue, Document, Hub, Node};
use lute_syntax::datalog::{parse_fact, DatalogError};

/// The merged `--mock` surface (dsl 0.4.0 §4.3): a scalar state seed, ground
/// facts, menu selections, and quest events — the same four surfaces a
/// `--mock <file.yaml>` document carries ([`parse_mock_yaml`]) and the CLI's
/// per-flag repeats compose with ([`merge`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MockSet {
    /// `(state path, literal TEXT, synthetic span)`. The literal arrives as
    /// raw text from either origin (a YAML scalar rendered back to text, or
    /// a CLI `path=literal` flag) — [`validate`] is the ONE place it is
    /// coerced against the path's declared [`Type`]. The span is never
    /// text-precise (there is no `--mock`/CLI source index to anchor
    /// against); every mock diagnostic renders at this same synthetic point.
    pub state: Vec<(String, String, Span)>,
    /// Raw `"rel(a, b)"` fact-pattern text, one per `--fact`/`facts:` entry,
    /// in the order supplied.
    pub facts: Vec<String>,
    /// branch/hub id -> the ordered choice id(s) forced at that presentation
    /// point (a hub's `--choose` may force a whole visit sequence, §4.4).
    pub choose: BTreeMap<String, Vec<String>>,
    /// `--event`/`events:` names, in the order they fire (Task 20's quest
    /// walk consumes this; T18 does not validate event names — no
    /// declaredness surface applies to a lifecycle/capability event kind).
    /// A lifecycle name (`questActive`/`questComplete`/`questFailed`) here
    /// is [`E_TRACE_EVENT`] (dsl 0.4.0 §4.3/§4.4): those are engine-derived
    /// transitions, never user-fired via `--event`.
    pub events: Vec<String>,
    /// `--accept`/`accept:`/`accepts:` quest ids, in the order supplied —
    /// simulates the player/engine accepting a `start`-less (accept-driven)
    /// quest (§4.4). An id absent from the document, or naming a quest that
    /// carries a `start` predicate (declarative — needs no accept), is
    /// [`E_TRACE_ACCEPT`].
    pub accepts: Vec<String>,
}

/// `--state`/`--mock` literals and `--choose` targets carry no real source
/// text — every diagnostic [`validate`]/[`parse_mock_yaml`] produces is
/// spanned at this zeroed placeholder (the interface's "CLI-arg synthetic
/// span"), mirroring the house zero-then-normalize convention
/// (`lute-check/src/check.rs`'s `zeroed_span`) other ad hoc span producers
/// use — there is simply no source `TextIndex` to normalize against here.
pub(crate) fn synthetic_span() -> Span {
    Span {
        byte_start: 0,
        byte_end: 0,
        line: 0,
        column: 0,
        utf16_range: (0, 0),
    }
}

/// A `--state`/mock-file path not declared in the resolved schema (§4.3):
/// "state-by-typo MUST fail in mocks exactly as in documents" (0.1 §11.1.1).
pub const E_TRACE_MOCK_UNDECLARED: &str = "E-TRACE-MOCK-UNDECLARED";
/// A mock literal incompatible with the path's declared type (§4.3).
pub const E_TRACE_MOCK_TYPE: &str = "E-TRACE-MOCK-TYPE";
/// A `--fact` naming an undeclared relation, wrong arity, or a foreign arg
/// (§4.3) — every [`lute_check::check_atom`] hit is re-coded here (D18).
pub const E_TRACE_MOCK_FACT: &str = "E-TRACE-MOCK-FACT";
/// `--choose` names an unknown branch/hub/choice id, pre-walk (§4.3); Task 19
/// re-emits this SAME code for the walk-time "forces a false guard" case
/// (§4.4) — this module only ever produces the structural half.
pub const E_TRACE_CHOICE: &str = "E-TRACE-CHOICE";
/// A `--event`/`events:` entry naming a built-in lifecycle event
/// (`questActive`/`questComplete`/`questFailed`) — those are engine-derived
/// transitions the engine fires on the `unset -> active`/completion/failure
/// transition, never impulses a writer fires directly (§4.3/§4.4): a
/// `start`-having quest activates declaratively, a `start`-less one via
/// `--accept`.
pub const E_TRACE_EVENT: &str = "E-TRACE-EVENT";
/// A `--accept`/`accept:`/`accepts:` entry naming an unknown quest id, or a
/// quest that carries a `start` predicate — it activates declaratively and
/// needs no accept (§4.3/§4.4).
pub const E_TRACE_ACCEPT: &str = "E-TRACE-ACCEPT";

/// spec §4 (0.6.1): a WARNING — not a refusal — for a supplied `--fact`/mock-
/// YAML fact whose relation `lute_check::producible::producible()` judges NOT
/// producible. The mocked answer can never arise from authored producers, so a
/// "complete" walk seeded with it proves nothing about reachable play. A
/// `reserved: true` / `open: engine`-argument relation is producible by
/// definition (0.4.0 §4.2, already encoded in `producible()`) and never warns.
/// Surfaced through the §3.1 additive `notes` key (report.rs), NEVER a
/// `Diagnostic` on the Refused path — the exit code is unchanged (D1: the mock
/// is a hypothesis; the checker's `producible()` computes, trace only displays).
pub const W_TRACE_MOCK_UNPRODUCIBLE: &str = "W-TRACE-MOCK-UNPRODUCIBLE";

/// A malformed `--mock` YAML file: bad syntax, or a top-level shape that
/// does not match §4.3's `state:`/`facts:`/`choose:`/`events:` contract.
/// Not an Appendix A code (no worked-example fixture cites it) — the CLI
/// (Task 21) renders it exactly like the four structural mock codes on the
/// Refused (exit 1) path, so it is still a plain `Diagnostic`, just not one
/// of the four the Task 18 interface names.
const E_TRACE_MOCK_PARSE: &str = "E-TRACE-MOCK-PARSE";

/// Build a `Layer::Logic` error diagnostic — mock validation is a
/// schema/graph-level property of the resolved document, the same layer
/// `rel_schema.rs`'s checks and the persist-sugar's `persist_diag` use.
fn diag(code: &str, message: String, span: Span) -> Diagnostic {
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

/// Render a YAML scalar to its literal TEXT form — the same shape a CLI
/// `--state path=literal` flag arrives in: `Bool` -> `true`/`false`,
/// `Number` -> its `Display` text, `String` verbatim. Any other shape (a
/// nested sequence/mapping, `null`, a tagged value) has no `--state` analog
/// and yields `None` — the caller reports it as a malformed mock.
fn scalar_to_text(v: &serde_yaml::Value) -> Option<String> {
    match v {
        serde_yaml::Value::Bool(b) => Some(b.to_string()),
        serde_yaml::Value::Number(n) => Some(n.to_string()),
        serde_yaml::Value::String(s) => Some(s.clone()),
        _ => None,
    }
}

/// Parse a `--mock <file.yaml>` document (dsl 0.4.0 §4.3): `state:` (a map
/// of path -> literal), `facts:` (a list of quoted ground-fact-pattern
/// strings, the `0.3 §4` `facts:` shape), `choose:` (a map of branch/hub id
/// -> one choice id or a list of them), `events:` (a list of event names) —
/// every key optional; an absent/empty/`null` document yields an empty
/// [`MockSet`]. Every literal/pattern/id is carried as raw TEXT, never
/// resolved against a schema here — that is [`validate`]'s job, run AFTER
/// [`merge`]. Malformed YAML or a shape violating this contract is `Err`;
/// this function never panics.
pub fn parse_mock_yaml(text: &str) -> Result<MockSet, Diagnostic> {
    let span = synthetic_span();
    let value: serde_yaml::Value = serde_yaml::from_str(text)
        .map_err(|e| diag(E_TRACE_MOCK_PARSE, format!("malformed `--mock` YAML: {e}"), span))?;
    if matches!(value, serde_yaml::Value::Null) {
        return Ok(MockSet::default());
    }
    let serde_yaml::Value::Mapping(top) = value else {
        return Err(diag(
            E_TRACE_MOCK_PARSE,
            "a `--mock` file must be a YAML mapping with `state:`/`facts:`/`choose:`/`events:` \
             keys (dsl 0.4.0 §4.3)"
                .to_string(),
            span,
        ));
    };

    let mut mocks = MockSet::default();

    if let Some(v) = top.get("state") {
        let serde_yaml::Value::Mapping(m) = v else {
            return Err(diag(
                E_TRACE_MOCK_PARSE,
                "`state:` must be a mapping of path -> literal (dsl 0.4.0 §4.3)".to_string(),
                span,
            ));
        };
        for (k, v) in m {
            let Some(path) = k.as_str() else {
                return Err(diag(
                    E_TRACE_MOCK_PARSE,
                    "`state:` keys must be strings (dsl 0.4.0 §4.3)".to_string(),
                    span,
                ));
            };
            let Some(literal) = scalar_to_text(v) else {
                return Err(diag(
                    E_TRACE_MOCK_PARSE,
                    format!("`state.{path}` must be a scalar literal (bool/number/string, dsl 0.4.0 §4.3)"),
                    span,
                ));
            };
            mocks.state.push((path.to_string(), literal, span));
        }
    }

    if let Some(v) = top.get("facts") {
        let serde_yaml::Value::Sequence(items) = v else {
            return Err(diag(
                E_TRACE_MOCK_PARSE,
                "`facts:` must be a list of quoted fact patterns (dsl 0.4.0 §4.3)".to_string(),
                span,
            ));
        };
        for item in items {
            let Some(s) = item.as_str() else {
                return Err(diag(
                    E_TRACE_MOCK_PARSE,
                    "every `facts:` entry must be a string (dsl 0.4.0 §4.3)".to_string(),
                    span,
                ));
            };
            mocks.facts.push(s.to_string());
        }
    }

    if let Some(v) = top.get("choose") {
        let serde_yaml::Value::Mapping(m) = v else {
            return Err(diag(
                E_TRACE_MOCK_PARSE,
                "`choose:` must be a mapping of branch/hub id -> choice id(s) (dsl 0.4.0 §4.3)".to_string(),
                span,
            ));
        };
        for (k, v) in m {
            let Some(id) = k.as_str() else {
                return Err(diag(
                    E_TRACE_MOCK_PARSE,
                    "`choose:` keys must be strings (dsl 0.4.0 §4.3)".to_string(),
                    span,
                ));
            };
            let ids = match v {
                serde_yaml::Value::String(s) => vec![s.clone()],
                serde_yaml::Value::Sequence(items) => {
                    let mut out = Vec::new();
                    for item in items {
                        let Some(s) = item.as_str() else {
                            return Err(diag(
                                E_TRACE_MOCK_PARSE,
                                format!("`choose.{id}` list entries must be strings (dsl 0.4.0 §4.3)"),
                                span,
                            ));
                        };
                        out.push(s.to_string());
                    }
                    out
                }
                _ => {
                    return Err(diag(
                        E_TRACE_MOCK_PARSE,
                        format!("`choose.{id}` must be a choice id or a list of choice ids (dsl 0.4.0 §4.3)"),
                        span,
                    ))
                }
            };
            mocks.choose.insert(id.to_string(), ids);
        }
    }

    if let Some(v) = top.get("events") {
        let serde_yaml::Value::Sequence(items) = v else {
            return Err(diag(
                E_TRACE_MOCK_PARSE,
                "`events:` must be a list of event names (dsl 0.4.0 §4.3)".to_string(),
                span,
            ));
        };
        for item in items {
            let Some(s) = item.as_str() else {
                return Err(diag(
                    E_TRACE_MOCK_PARSE,
                    "every `events:` entry must be a string (dsl 0.4.0 §4.3)".to_string(),
                    span,
                ));
            };
            mocks.events.push(s.to_string());
        }
    }

    // `accept:`/`accepts:` — either spelling, a list of quest ids (§4.4).
    for key in ["accept", "accepts"] {
        let Some(v) = top.get(key) else { continue };
        let serde_yaml::Value::Sequence(items) = v else {
            return Err(diag(
                E_TRACE_MOCK_PARSE,
                format!("`{key}:` must be a list of quest ids (dsl 0.4.0 §4.3/§4.4)"),
                span,
            ));
        };
        for item in items {
            let Some(s) = item.as_str() else {
                return Err(diag(
                    E_TRACE_MOCK_PARSE,
                    format!("every `{key}:` entry must be a string (dsl 0.4.0 §4.3/§4.4)"),
                    span,
                ));
            };
            mocks.accepts.push(s.to_string());
        }
    }

    Ok(mocks)
}

/// Compose a `--mock <file.yaml>`'s [`MockSet`] with the CLI's own
/// `--state`/`--fact`/`--choose`/`--event`/`--accept` flags (dsl 0.4.0
/// §4.3): "CLI flags compose with the file; on a conflict the flag wins
/// (facts union; a flag `choose` replaces that id's file entry)".
///
/// * `facts` — set UNION: every distinct fact text from either source,
///   file-then-flags document order, duplicates collapsed.
/// * `choose` — per-id REPLACE: a flag entry for an id overwrites that same
///   id's file entry wholesale; an id present in only one source passes
///   through unchanged.
/// * `state` — per-PATH REPLACE, flag wins: a flag entry for a path drops
///   the file's entry for that SAME path entirely (not merely reordered —
///   [`validate`] never sees the shadowed file value); a path present in
///   only one source passes through.
/// * `events` — compose file-then-flags, in that relative order (§4.3
///   specifies no override rule for this surface; events are impulses to
///   fire, not declarations to shadow).
/// * `accepts` — set UNION, same idiom as `facts` (accepting a quest twice,
///   from the file and a flag, is the same accept — not a shadow to
///   resolve).
pub fn merge(file: MockSet, flags: MockSet) -> MockSet {
    let flag_paths: std::collections::BTreeSet<&str> =
        flags.state.iter().map(|(p, _, _)| p.as_str()).collect();
    let mut state: Vec<_> = file
        .state
        .into_iter()
        .filter(|(p, _, _)| !flag_paths.contains(p.as_str()))
        .collect();
    state.extend(flags.state);

    let mut seen_facts = std::collections::BTreeSet::new();
    let facts: Vec<String> = file
        .facts
        .into_iter()
        .chain(flags.facts)
        .filter(|f| seen_facts.insert(f.clone()))
        .collect();

    let mut choose = file.choose;
    choose.extend(flags.choose);

    let mut events = file.events;
    events.extend(flags.events);

    let mut seen_accepts = std::collections::BTreeSet::new();
    let accepts: Vec<String> = file
        .accepts
        .into_iter()
        .chain(flags.accepts)
        .filter(|id| seen_accepts.insert(id.clone()))
        .collect();

    MockSet { state, facts, choose, events, accepts }
}

/// Coerce a raw `--state`/mock literal into a manifest [`Literal`] *in the
/// declared type's domain* so [`type_accepts`] can judge it — the same
/// idiom `lute-check/src/check.rs`'s `persist_literal` uses for the persist
/// sugar's `value` attr, adapted for a bare string (a CLI flag/YAML scalar
/// has no `AttrValue::BoolTrue`/`Ref` shape to distinguish). A `bool` target
/// accepts only the literal strings `"true"`/`"false"`; a `number` target
/// parses the string as `f64`; every other target (`enum`/`str`/`domain`/…)
/// keeps the value VERBATIM as [`Literal::Str`], so an enum member is judged
/// by string membership via `type_accepts` itself. `None` means the value
/// cannot inhabit the target's shape at all (a hard type error).
pub(crate) fn coerce_state_literal(ty: &Type, raw: &str) -> Option<Literal> {
    match ty {
        Type::Bool => match raw {
            "true" => Some(Literal::Bool(true)),
            "false" => Some(Literal::Bool(false)),
            _ => None,
        },
        Type::Number => raw.parse::<f64>().ok().map(Literal::Num),
        _ => Some(Literal::Str(raw.to_string())),
    }
}

/// §1.1 (dsl 0.5.1): a RESERVED `quest.<id>.state`/`quest.<id>.
/// objectives.<oid>.done` `--state` NEVER takes the ordinary schema-decl
/// branch below — checked FIRST, unconditionally — even when the traced
/// document itself DEFINES `<quest id>`: `check_quest`
/// (`lute-check/src/match_check.rs`) synthesizes a `folded.env.state.decls`
/// entry for a LOCAL quest's OWN reserved paths too (the engine's real
/// `state` enum `[active, complete, failed]`, no `default:`, no `unset`
/// member — `unset` is the pre-activation ABSENCE of a value, never an
/// enum member). Falling through to that decl would (a) admit the mock
/// unconditionally via ordinary schema-declaredness, bypassing §1.1's
/// "document REFERENCES it" gate entirely, and (b) reject a genuinely
/// referenced `--state quest.<id>.state=unset` via `type_accepts` against
/// an enum that has no `unset` member — both wrong. Local and foreign
/// quests are therefore unified into ONE admission rule: does the
/// document reference this exact path
/// ([`crate::quest_refs::collect_referenced_reserved_quest_paths`]),
/// checked against the reserved path's OWN domain (`active|complete|
/// failed|unset` for `.state`, `true|false` for `.objectives.*.done`)
/// rather than the synthesized schema `Type`. A reserved path the
/// document does NOT reference, or an ordinary undeclared path, is
/// unchanged: `E-TRACE-MOCK-UNDECLARED`, identity and message untouched
/// (Appendix A).
fn validate_state(mocks: &MockSet, folded: &FoldedEnv, doc: &Document) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let mut referenced_reserved: Option<BTreeSet<String>> = None;
    for (path, literal, span) in &mocks.state {
        if crate::eval::is_reserved_quest_path(path) {
            let referenced = referenced_reserved
                .get_or_insert_with(|| crate::quest_refs::collect_referenced_reserved_quest_paths(doc));
            if referenced.contains(path) {
                if !reserved_quest_literal_valid(path, literal) {
                    out.push(diag(
                        E_TRACE_MOCK_TYPE,
                        format!(
                            "`--state {path}={literal}` is not compatible with `{path}`'s reserved \
                             domain ({}) (dsl 0.5.1 §1.1)",
                            reserved_quest_domain_text(path)
                        ),
                        *span,
                    ));
                }
                continue;
            }
            out.push(undeclared_diag(path, *span));
            continue;
        }
        if let Some(decl) = folded.env.state.decls.get(path) {
            let ok = coerce_state_literal(&decl.ty, literal).is_some_and(|lit| type_accepts(&decl.ty, &lit));
            if !ok {
                out.push(diag(
                    E_TRACE_MOCK_TYPE,
                    format!(
                        "`--state {path}={literal}` is not compatible with `{path}`'s declared type \
                         (dsl 0.4.0 §4.3)"
                    ),
                    *span,
                ));
            }
            continue;
        }
        out.push(undeclared_diag(path, *span));
    }
    out
}

/// `E-TRACE-MOCK-UNDECLARED` for `path` (dsl 0.4.0 §4.3, 0.1 §11.1.1) —
/// shared by [`validate_state`]'s two "no admissible schema/reserved
/// entry" exits (ordinary undeclared path; reserved path the document
/// does not reference) so the message stays byte-identical either way.
fn undeclared_diag(path: &str, span: Span) -> Diagnostic {
    diag(
        E_TRACE_MOCK_UNDECLARED,
        format!(
            "`--state {path}=…` names a state path not declared in the resolved schema \
             (state-by-typo MUST fail in mocks exactly as in documents, dsl 0.4.0 §4.3, \
             0.1 §11.1.1)"
        ),
        span,
    )
}

/// `true` iff `literal` inhabits the reserved path's own domain (§1.1):
/// `active|complete|failed|unset` for `quest.<id>.state`, `true|false` for
/// `quest.<id>.objectives.<oid>.done`.
fn reserved_quest_literal_valid(path: &str, literal: &str) -> bool {
    if crate::eval::is_reserved_quest_objective_done_path(path) {
        matches!(literal, "true" | "false")
    } else {
        matches!(literal, "active" | "complete" | "failed" | "unset")
    }
}

fn reserved_quest_domain_text(path: &str) -> &'static str {
    if crate::eval::is_reserved_quest_objective_done_path(path) {
        "true, false"
    } else {
        "active, complete, failed, unset"
    }
}

fn describe_datalog_error(e: &DatalogError) -> String {
    match e {
        DatalogError::Malformed { msg, .. } => msg.clone(),
        DatalogError::FunctionTerm { name, .. } => {
            format!("compound term `{name}(…)` is not a legal fact argument")
        }
    }
}

/// `--fact` validation (§4.3): parse via [`lute_syntax::datalog::parse_fact`],
/// then D18's [`lute_check::check_atom`] reuse for unknown-relation/arity/
/// foreign-arg — every hit re-coded [`E_TRACE_MOCK_FACT`]. `check_atom`
/// alone (never the write-policy layer `::assert`/`::retract` go through)
/// is why a `derive:true`/`reserved:true` relation validates clean here — a
/// mock is a supplied answer, not a content write.
fn validate_facts(mocks: &MockSet, folded: &FoldedEnv) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let span = synthetic_span();
    for raw in &mocks.facts {
        match parse_fact(raw) {
            Err(e) => out.push(diag(
                E_TRACE_MOCK_FACT,
                format!(
                    "`--fact \"{raw}\"` does not parse as a ground fact pattern: {} (dsl 0.4.0 §4.3)",
                    describe_datalog_error(&e)
                ),
                span,
            )),
            Ok(pattern) => {
                let hits = check_atom(
                    &folded.env.rel_vocab,
                    &folded.env.domains,
                    &pattern.relation,
                    &pattern.args,
                    /* wildcard_ok = */ false,
                    span,
                );
                for h in hits {
                    out.push(diag(
                        E_TRACE_MOCK_FACT,
                        format!("`--fact \"{raw}\"`: {} (dsl 0.4.0 §4.3)", h.message),
                        span,
                    ));
                }
            }
        }
    }
    out
}

/// The literal string `id` attr of a `<hub>` (mirrors
/// `lute-check/src/match_check.rs`'s private `attr_str` — not reusable
/// across the D1 quarantine boundary, so this carries its own copy).
pub(crate) fn hub_id(h: &Hub) -> Option<String> {
    h.attrs
        .iter()
        .find(|a| a.key == "id")
        .and_then(|a| match &a.value {
            AttrValue::Str(s) => Some(s.clone()),
            _ => None,
        })
}

/// branch/hub id -> its choice ids, in document order — collected by walking
/// `doc` directly (mirrors `lute-check/src/check.rs`'s `fold_branches_nodes`
/// recursion: a `<branch>`/`<hub>` may nest inside a `<match>` arm, an
/// `<on>`/`<objective>` quest arm, or another choice's body).
fn collect_choice_ids(doc: &Document) -> BTreeMap<String, Vec<String>> {
    let mut out = BTreeMap::new();
    for shot in &doc.shots {
        collect_choice_ids_nodes(&shot.body, &mut out);
    }
    for quest in &doc.quests {
        collect_choice_ids_nodes(&quest.body, &mut out);
    }
    out
}

fn collect_choice_ids_nodes(nodes: &[Node], out: &mut BTreeMap<String, Vec<String>>) {
    for node in nodes {
        match node {
            Node::Branch(b) => {
                out.insert(b.id.clone(), b.choices.iter().map(|c| c.id.clone()).collect());
                for choice in &b.choices {
                    collect_choice_ids_nodes(&choice.body, out);
                }
            }
            Node::Hub(h) => {
                if let Some(id) = hub_id(h) {
                    out.insert(id, h.choices.iter().map(|c| c.id.clone()).collect());
                }
                for choice in &h.choices {
                    collect_choice_ids_nodes(&choice.body, out);
                }
            }
            Node::Match(m) => {
                for arm in &m.arms {
                    match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => {
                            collect_choice_ids_nodes(body, out)
                        }
                    }
                }
            }
            Node::On(o) => collect_choice_ids_nodes(&o.body, out),
            Node::Objective(o) => collect_choice_ids_nodes(&o.body, out),
            Node::Line(_)
            | Node::Directive(_)
            | Node::Set(_)
            | Node::Timeline(_)
            | Node::Assert(_)
            | Node::Retract(_) => {}
        }
    }
}

/// `--choose` structural validation (§4.3): ids only — an id or choice id
/// absent from the walked document is [`E_TRACE_CHOICE`]. The choice's
/// `when=` guard is NEVER consulted here (see the module doc); Task 19's
/// walk re-emits this SAME code when a forced choice's guard decides false
/// AT ITS PRESENTATION POINT (§4.4) — a walk-time property this pre-walk
/// pass cannot see.
fn validate_choose(mocks: &MockSet, doc: &Document) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let span = synthetic_span();
    let known = collect_choice_ids(doc);
    for (id, choice_ids) in &mocks.choose {
        let Some(valid_choices) = known.get(id) else {
            out.push(diag(
                E_TRACE_CHOICE,
                format!("`--choose {id}=…` names an unknown branch/hub id `{id}` (dsl 0.4.0 §4.3)"),
                span,
            ));
            continue;
        };
        for cid in choice_ids {
            if !valid_choices.iter().any(|c| c == cid) {
                out.push(diag(
                    E_TRACE_CHOICE,
                    format!(
                        "`--choose {id}={cid}` names an unknown choice id `{cid}` for \
                         `<branch/hub id=\"{id}\">` (dsl 0.4.0 §4.3)"
                    ),
                    span,
                ));
            }
        }
    }
    out
}

/// `--event`/`events:` validation (§4.3/§4.4): a name matching one of the
/// engine's built-in lifecycle events (`questActive`/`questComplete`/
/// `questFailed`) is [`E_TRACE_EVENT`] — those transitions are
/// engine-derived (a `start`-having quest activates declaratively, a
/// `start`-less one via `--accept`), never a writer-fired impulse.
fn validate_events(mocks: &MockSet) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let span = synthetic_span();
    for name in &mocks.events {
        if lute_manifest::snapshot::BUILTIN_LIFECYCLE_EVENTS.contains(&name.as_str()) {
            out.push(diag(
                E_TRACE_EVENT,
                format!(
                    "`--event {name}` names a built-in lifecycle event — `{name}` is \
                     engine-derived (a `start`-having quest activates declaratively, a \
                     `start`-less one via `--accept`), never user-fired via `--event` \
                     (dsl 0.4.0 §4.3/§4.4)"
                ),
                span,
            ));
        }
    }
    out
}

/// `--accept`/`accept:`/`accepts:` validation (§4.3/§4.4): an id absent
/// from `doc.quests`, or naming a quest that carries a `start` predicate
/// (declarative — it activates on its own and needs no accept), is
/// [`E_TRACE_ACCEPT`].
fn validate_accept(mocks: &MockSet, doc: &Document) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let span = synthetic_span();
    for id in &mocks.accepts {
        let Some(quest) = doc.quests.iter().find(|q| &q.id == id) else {
            out.push(diag(
                E_TRACE_ACCEPT,
                format!("`--accept {id}` names an unknown quest id `{id}` (dsl 0.4.0 §4.3/§4.4)"),
                span,
            ));
            continue;
        };
        if quest.start.is_some() {
            out.push(diag(
                E_TRACE_ACCEPT,
                format!(
                    "`--accept {id}` names quest `{id}`, which carries a `start` predicate — \
                     it activates declaratively and needs no accept (dsl 0.4.0 §4.3/§4.4)"
                ),
                span,
            ));
        }
    }
    out
}

/// STRUCTURAL pre-walk validation (dsl 0.4.0 §4.3): ids/arity/types/
/// declaredness, plus the two lifecycle guards (§4.3/§4.4): a lifecycle
/// name in `--event` ([`validate_events`]) and an unknown/`start`-having
/// quest id in `--accept` ([`validate_accept`]). Forced-choice GUARDS are
/// deliberately NOT evaluated here — eligibility is a presentation-point
/// property (§4.4) that depends on in-flow writes the walk has not applied
/// yet; Task 19 owns it. Runs every surface independently (a document with
/// a bad state path AND a bad fact reports both) and returns every
/// diagnostic, unsorted — the caller (Task 19's pipeline / the CLI's
/// Refused path) owns presentation order.
pub fn validate(mocks: &MockSet, folded: &FoldedEnv, doc: &Document) -> Vec<Diagnostic> {
    let mut diags = validate_state(mocks, folded, doc);
    diags.extend(validate_facts(mocks, folded));
    diags.extend(validate_choose(mocks, doc));
    diags.extend(validate_events(mocks));
    diags.extend(validate_accept(mocks, doc));
    diags
}
