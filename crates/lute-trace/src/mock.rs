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

use lute_check::{check_atom, FoldedEnv};
use lute_core_span::{Diagnostic, Layer, Severity, Span};
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
    /// `visited:` scene ids (dsl 0.21.0 §7a.1): the scenes already presented
    /// in this save, which `visited('<id>')` reads — closed-world, like
    /// `facts:`. Scene ids are project-level, so a per-document validator
    /// cannot check them; an id nothing reads is simply inert.
    pub visited: Vec<String>,
    /// `--occasion`/`occasions:` names, in the order they are raised (dsl
    /// 0.21.0 §7a.2): after a quest walk settles, each raise evaluates the
    /// `<objective on="<occasion>">` objectives of every active quest. A raise
    /// FOR a target is written `<occasion>@<target>` (dsl 0.23.0 §2, e.g.
    /// `talk@npc.maud`): an objective with `target=` is judged only by a
    /// raise for that target. Read one with [`split_occasion`].
    pub occasions: Vec<String>,
    /// `derive:` (dsl 0.22.0 §6): `None`/`Some(true)` applies the project's
    /// seed facts and Datalog rules over the mocked and asserted facts (the
    /// default); `Some(false)` (`--no-derive`) restores the 0.21 model, in
    /// which an unmocked derived atom is unknown. Read it through
    /// [`MockSet::derives`].
    pub derive: Option<bool>,
}

impl MockSet {
    /// Whether derivation applies (dsl 0.22.0 §6: default `true`).
    pub fn derives(&self) -> bool {
        self.derive.unwrap_or(true)
    }
}

/// An occasion raise as written in `occasions:` / `--occasion` (dsl 0.23.0
/// §2): `talk` → `("talk", None)`, `talk@npc.maud` → `("talk",
/// Some("npc.maud"))`.
pub fn split_occasion(raw: &str) -> (&str, Option<&str>) {
    match raw.split_once('@') {
        Some((name, target)) => (name, Some(target)),
        None => (raw, None),
    }
}

/// dsl 0.23.0 §2: whether an objective `on=on target=target` is judged by
/// the raise `raw` — the beat target rule: the occasion matches, and the
/// objective's target is absent or equal to the raise's.
pub fn raise_judges(raw: &str, on: &str, target: Option<&str>) -> bool {
    let (name, raised) = split_occasion(raw);
    name == on && target.is_none_or(|t| raised == Some(t))
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
/// `lute trace --entry <id>` (dsl 0.19.0 §8) on a document that is not
/// `kind: lore`, or naming an id no `<entry>` in the document declares — the
/// `--entry` analogue of [`E_TRACE_ACCEPT`]'s unknown-quest-id refusal.
pub const E_TRACE_ENTRY: &str = "E-TRACE-ENTRY";
/// `lute trace --beat <id>` (dsl 0.23.0 §4) on a
/// document that is not `kind: lore`, or naming an id no `<beat>` in the
/// document declares (neither the local `<beat id>` nor the canonical
/// `<document id>.<beat id>`) — the `--beat` analogue of [`E_TRACE_ENTRY`].
pub const E_TRACE_BEAT: &str = "E-TRACE-BEAT";

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
pub const E_TRACE_MOCK_PARSE: &str = "E-TRACE-MOCK-PARSE";

/// 0.10.0 §8 (#31, D-AC): a `mocks/*.yaml` with no `file:`, a `file:` naming
/// a path that does not exist, or a `file:` that disagrees with the document
/// named on a `lute trace` command line.
///
/// Anchored at the mock file and naming the offending key in its message,
/// never at a line and column: `parse_mock_yaml` deserializes into
/// `serde_yaml::Value`, which retains no position; every mock entry carries
/// the all-zeros [`synthetic_span`]; and `Diagnostic` has no file field at
/// all, so fixing the first two would put a correct position in the wrong
/// file (D-AB).
pub const E_MOCK_SUBJECT: &str = "E-MOCK-SUBJECT";

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

/// The complete legal top-level key set of a `--mock` / `mocks/*.yaml`
/// document — the surfaces [`parse_mock_yaml`] reads (`accept`/`accepts`
/// are two spellings of one key; `visited`/`occasions` are dsl 0.21.0 §7a;
/// `quests`/`entriesRead` are dsl 0.22.0 §3's save-history seeds) plus
/// `file:`, the subject key **D-AC** made required.
///
/// **CLOSED as of 0.10.0 (#2(a), D-B).** D-B's rationale names this exact
/// failure — *"a mis-keyed `choose:` currently passes while running the arm it
/// excludes"* — and 0.10.0 shipped the closed set for `*.test.yaml` only, so a
/// mock spelling `selections:` was still dropped in silence and `lute trace`
/// still auto-picked the arm the file excluded, at exit 0.
///
/// **The two families' key sets are NOT identical.** `*.test.yaml`'s set
/// (`lute-cli/src/testcmd.rs::TEST_TOP_KEYS`) is this one plus the
/// harness's own `expect:` and `entry:`/`entries:` (dsl 0.22.0 §5); none of
/// them means anything in a `mocks/*.yaml` — nothing runs it — so they are
/// unknown keys here. That difference is asserted by `testcmd.rs`'s
/// `the_test_key_set_is_the_mock_key_set_plus_the_harness_keys`.
pub const MOCK_TOP_KEYS: &[&str] = &[
    "accept",
    "accepts",
    "choose",
    "derive",
    "entriesRead",
    "events",
    "facts",
    "file",
    "occasions",
    "quests",
    "state",
    "visited",
];

/// Parse a `--mock <file.yaml>` document (dsl 0.4.0 §4.3, 0.10.0 §8):
/// `state:` (a map of path -> literal), `facts:` (a list of quoted
/// ground-fact-pattern strings, the `0.3 §4` `facts:` shape), `choose:` (a map
/// of branch/hub id -> one choice id or a list of them), `events:` (a list of
/// event names), `accept(s):` (quest ids), `visited:` (scene ids) and
/// `occasions:` (occasion names, dsl 0.21.0 §7a) — every key optional; an
/// absent/empty/`null` document yields
/// an empty [`MockSet`]. Every literal/pattern/id is carried as raw TEXT,
/// never resolved against a schema here — that is [`validate`]'s job, run
/// AFTER [`merge`]. Malformed YAML or a shape violating this contract is
/// `Err`; this function never panics.
///
/// The top-level key set is **CLOSED** ([`MOCK_TOP_KEYS`]) — an unknown key is
/// `E-TRACE-MOCK-PARSE`, the same channel every other mock-grammar violation
/// already travels, so the diagnostic reaches `lute trace --mock`,
/// `lute run --mock`, `check-project`'s `mocks/*.yaml` pass (§8) and the wasm
/// preview without a new code and without a new wiring point. Enforcing it
/// HERE rather than at each call site is the point: a mock document cannot be
/// consumed anywhere without the gate.
// `Diagnostic` is the crate's error currency by design (span + fixits travel
// with every refusal); boxing it here would ripple through every `?` caller
// for a cold path. clippy 1.96's `result_large_err` threshold is waived at
// the four mock-parser entry points.
#[allow(clippy::result_large_err)]
pub fn parse_mock_yaml(text: &str) -> Result<MockSet, Diagnostic> {
    parse_mock_document(text, Some(MOCK_TOP_KEYS))
}

/// [`parse_mock_yaml`]'s surfaces WITHOUT the closed-key gate.
///
/// One caller, and it is the reason the split exists: `lute test` reads a
/// `*.test.yaml` through this same parser, and that family's legal set is
/// [`MOCK_TOP_KEYS`] **plus `expect:`, `entry:` and `entries:`**. It also reports a key violation
/// differently — a per-test FAILURE naming *every* offender in one run
/// (`E-TEST-KEY`, exit 1, D-B), not a first-offender parse error (exit 2) —
/// so it cannot delegate the gate here even for the keys the sets share.
#[allow(clippy::result_large_err)]
pub fn parse_mock_surfaces(text: &str) -> Result<MockSet, Diagnostic> {
    parse_mock_document(text, None)
}

/// The one mock parser. `legal` is `Some` for the closed mock grammar and
/// `None` for `lute test`'s open read; see the two public wrappers.
#[allow(clippy::result_large_err)]
fn parse_mock_document(text: &str, legal: Option<&[&str]>) -> Result<MockSet, Diagnostic> {
    let span = synthetic_span();
    let value: serde_yaml::Value = serde_yaml::from_str(text).map_err(|e| {
        diag(
            E_TRACE_MOCK_PARSE,
            format!("malformed `--mock` YAML: {e}"),
            span,
        )
    })?;
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

    // #2(a)/D-B, extended to the mock family: an unknown top-level key is an
    // ERROR, not a silent drop. Checked before the surfaces so that a
    // `selections:` typo is reported as the typo it is, rather than as
    // whatever the surviving surfaces happen to say next.
    if let Some(legal) = legal {
        for (k, _) in &top {
            let Some(key) = k.as_str() else {
                return Err(diag(
                    E_TRACE_MOCK_PARSE,
                    "a mock's top-level keys must be strings (dsl 0.4.0 §4.3)".to_string(),
                    span,
                ));
            };
            if legal.contains(&key) {
                continue;
            }
            let sugg = lute_manifest::suggest::nearest(key, legal.iter().copied(), 2)
                .map(|k| format!(" — did you mean `{k}`?"))
                .unwrap_or_default();
            return Err(diag(
                E_TRACE_MOCK_PARSE,
                format!(
                    "unknown top-level key `{key}` in a mock{sugg} (legal: {}) (0.10.0 §8)",
                    legal.join(", ")
                ),
                span,
            ));
        }
    }

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
                "`choose:` must be a mapping of branch/hub id -> choice id(s) (dsl 0.4.0 §4.3)"
                    .to_string(),
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

    // dsl 0.21.0 §7a: `visited:` (scene ids) and `occasions:` (occasion
    // names) — both plain lists of strings.
    for (key, what) in [("visited", "scene ids"), ("occasions", "occasion names")] {
        let Some(v) = top.get(key) else { continue };
        let serde_yaml::Value::Sequence(items) = v else {
            return Err(diag(
                E_TRACE_MOCK_PARSE,
                format!("`{key}:` must be a list of {what} (dsl 0.21.0 §7a)"),
                span,
            ));
        };
        let out = if key == "visited" {
            &mut mocks.visited
        } else {
            &mut mocks.occasions
        };
        for item in items {
            let Some(s) = item.as_str() else {
                return Err(diag(
                    E_TRACE_MOCK_PARSE,
                    format!("every `{key}:` entry must be a string (dsl 0.21.0 §7a)"),
                    span,
                ));
            };
            if key == "occasions" {
                let (name, target) = split_occasion(s);
                if name.is_empty() || target.is_some_and(str::is_empty) {
                    return Err(diag(
                        E_TRACE_MOCK_PARSE,
                        format!(
                            "`occasions:` entry `{s}` must be an occasion name, or \
                             `<occasion>@<target>` for a raise for a target (dsl 0.23.0 §2)"
                        ),
                        span,
                    ));
                }
            }
            out.push(s.to_string());
        }
    }

    // dsl 0.22.0 §3: `quests:` / `entriesRead:` seed the save's history.
    // Both are spellings of reserved state paths, so they are carried AS
    // those `state:` seeds — the same admission rule (the document must
    // reference or declare the path), the same reserved domains and the
    // same seeding every `state: { quest.<id>.state: … }` mock already
    // gets, never a second channel.
    if let Some(v) = top.get("quests") {
        let serde_yaml::Value::Mapping(m) = v else {
            return Err(diag(
                E_TRACE_MOCK_PARSE,
                "`quests:` must be a map of quest id -> unset | active | complete | failed \
                 (dsl 0.22.0 §3)"
                    .to_string(),
                span,
            ));
        };
        for (k, status) in m {
            let (Some(id), Some(status)) = (k.as_str(), status.as_str()) else {
                return Err(diag(
                    E_TRACE_MOCK_PARSE,
                    "every `quests:` entry must be `<quest id>: <status>` (dsl 0.22.0 §3)"
                        .to_string(),
                    span,
                ));
            };
            if !matches!(status, "unset" | "active" | "complete" | "failed") {
                return Err(diag(
                    E_TRACE_MOCK_PARSE,
                    format!(
                        "`quests: {{ {id}: {status} }}` — a quest status is one of unset, \
                         active, complete, failed (dsl 0.22.0 §3)"
                    ),
                    span,
                ));
            }
            mocks
                .state
                .push((format!("quest.{id}.state"), status.to_string(), span));
        }
    }
    if let Some(v) = top.get("entriesRead") {
        let shape = "`entriesRead:` must be a map `{ run: [entry ids], user: [entry ids] }` \
                     (dsl 0.22.0 §3)";
        let serde_yaml::Value::Mapping(m) = v else {
            return Err(diag(E_TRACE_MOCK_PARSE, shape.to_string(), span));
        };
        for (tier, ids) in m {
            let tier = tier.as_str().unwrap_or_default();
            let (serde_yaml::Value::Sequence(ids), "run" | "user") = (ids, tier) else {
                return Err(diag(E_TRACE_MOCK_PARSE, shape.to_string(), span));
            };
            for id in ids {
                let Some(id) = id.as_str() else {
                    return Err(diag(E_TRACE_MOCK_PARSE, shape.to_string(), span));
                };
                // `run`: `entry.<id>.read`; `user`: `entry.<id>.everRead`
                // (dsl 0.22.0 §7). Neither implies the other: a new run
                // clears `read` and keeps `everRead`.
                let path = if tier == "run" {
                    lute_check::entry_read_path(id)
                } else {
                    format!("entry.{id}.everRead")
                };
                mocks.state.push((path, "true".to_string(), span));
            }
        }
    }

    // dsl 0.22.0 §6: `derive: false` restores the 0.21 lookup-only model.
    if let Some(v) = top.get("derive") {
        let Some(b) = v.as_bool() else {
            return Err(diag(
                E_TRACE_MOCK_PARSE,
                "`derive:` must be `true` or `false` (dsl 0.22.0 §6)".to_string(),
                span,
            ));
        };
        mocks.derive = Some(b);
    }

    Ok(mocks)
}

/// The document a mock previews, from its `file:` key (0.10.0 §8, D-AC) —
/// the SAME key, spelling and base rule a `*.test.yaml` has carried since
/// 0.4.0 (`lute-cli/src/testcmd.rs`), resolved by the caller against the
/// mock's own parent directory.
///
/// `Ok(None)` when the key is absent. Required-ness is NOT enforced here:
/// `lute trace <doc> --mock m.yaml` supplies the subject on the command line
/// and that wins, and the six `conformance/*/mock.yaml` acceptance fixtures
/// legitimately carry none. `check-project`'s pass over `mocks/*.yaml` is
/// where the key is required.
#[allow(clippy::result_large_err)]
pub fn mock_subject(text: &str) -> Result<Option<String>, Diagnostic> {
    let span = synthetic_span();
    let value: serde_yaml::Value = serde_yaml::from_str(text).map_err(|e| {
        diag(
            E_TRACE_MOCK_PARSE,
            format!("malformed mock YAML: {e}"),
            span,
        )
    })?;
    let Some(top) = value.as_mapping() else {
        return Ok(None);
    };
    match top.get("file") {
        None => Ok(None),
        Some(serde_yaml::Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(diag(
            E_TRACE_MOCK_PARSE,
            "`file:` must be a path to the document this mock previews, relative to this file \
             (0.10.0 §8)"
                .to_string(),
            span,
        )),
    }
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
/// * `visited` — set UNION, the same idiom (a scene presented is presented).
/// * `occasions` — compose file-then-flags, in that relative order (the
///   `events` rule: occasions are raised in sequence, not declared).
/// * `derive` — the flag (`--no-derive`) wins over the file's `derive:`.
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

    let mut seen_visited = std::collections::BTreeSet::new();
    let visited: Vec<String> = file
        .visited
        .into_iter()
        .chain(flags.visited)
        .filter(|id| seen_visited.insert(id.clone()))
        .collect();

    let mut occasions = file.occasions;
    occasions.extend(flags.occasions);

    MockSet {
        state,
        facts,
        choose,
        events,
        accepts,
        visited,
        occasions,
        derive: flags.derive.or(file.derive),
    }
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
    let mut referenced_entry_reads: Option<BTreeSet<String>> = None;
    for (path, literal, span) in &mocks.state {
        if crate::eval::is_reserved_quest_path(path) {
            let referenced = referenced_reserved.get_or_insert_with(|| {
                let mut set = crate::quest_refs::collect_referenced_reserved_quest_paths(doc);
                // A scene beat's frontmatter `when:` is a read too: trace
                // judges it (the beat-when note), so its seed must be legal.
                if let Some(when) = folded.typed.beat.as_ref().and_then(|b| b.when.as_ref()) {
                    crate::quest_refs::collect_referenced_in_raw(
                        &when.raw,
                        crate::eval::is_reserved_quest_path,
                        &mut set,
                    );
                }
                set
            });
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
            out.push(reserved_quest_unreferenced_diag(path, *span));
            continue;
        }
        // dsl 0.19.0 §5: `entry.<id>.read` is admitted when the document
        // declares that entry (the checker folds its bool decl) OR reads the
        // path from any CEL slot — the reserved-quest-path rule above,
        // checked against the reserved `bool` domain.
        if lute_check::is_reserved_entry_read(path) {
            let declared = folded.env.state.decls.contains_key(path);
            let referenced = declared
                || referenced_entry_reads
                    .get_or_insert_with(|| {
                        crate::quest_refs::collect_referenced_entry_read_paths(doc)
                    })
                    .contains(path);
            if !referenced {
                out.push(undeclared_diag(path, *span));
            } else if !matches!(literal.as_str(), "true" | "false") {
                out.push(diag(
                    E_TRACE_MOCK_TYPE,
                    format!(
                        "`--state {path}={literal}` is not compatible with `{path}`'s reserved \
                         domain (true, false) (dsl 0.19.0 §5)"
                    ),
                    *span,
                ));
            }
            continue;
        }
        if let Some(decl) = folded.env.state.decls.get(path) {
            let ok = coerce_state_literal(&decl.ty, literal)
                .is_some_and(|lit| type_accepts(&decl.ty, &lit));
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

/// A seed of a reserved quest path (a `quests:` entry, or `state:` /
/// `--state` on `quest.<id>.…`) that no condition of this document reads.
fn reserved_quest_unreferenced_diag(path: &str, span: Span) -> Diagnostic {
    diag(
        E_TRACE_MOCK_UNDECLARED,
        format!(
            "the seed of `{path}` (a `quests:` entry or `state:`/`--state` seed) is refused: \
             no condition in this document — body slot or beat `when:` — reads it, so the \
             seed could not change the walk (dsl 0.5.1 §1.1, 0.22.0 §3)"
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
    for beat in &doc.beats {
        collect_choice_ids_nodes(&beat.body, &mut out);
    }
    out
}

fn collect_choice_ids_nodes(nodes: &[Node], out: &mut BTreeMap<String, Vec<String>>) {
    for node in nodes {
        match node {
            Node::Branch(b) => {
                out.insert(
                    b.id.clone(),
                    b.choices.iter().map(|c| c.id.clone()).collect(),
                );
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

/// `--accept`/`accept:`/`accepts:` validation (§4.3/§4.4, extended by the
/// subquest design 2026-08-31 §2.4): an id absent from `doc.quests`,
/// naming a quest that carries a `start` predicate (declarative — it
/// activates on its own and needs no accept), or naming a REFERENCED
/// no-start child (activation is derived from its parent's activation,
/// not accept-driven), is [`E_TRACE_ACCEPT`]. The referenced-child guard
/// uses only same-doc `<objective quest=…>` refs — cross-file parents
/// are the CLI's project-aware concern (spec §2.3's own union note); an
/// unref'd cross-file child still admits `--accept` here.
fn validate_accept(mocks: &MockSet, doc: &Document) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let span = synthetic_span();
    let referenced_children = referenced_child_ids(doc);
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
            continue;
        }
        if referenced_children.contains(id.as_str()) {
            out.push(diag(
                E_TRACE_ACCEPT,
                format!(
                    "`--accept {id}` names quest `{id}`, which is referenced by a parent \
                     quest's `<objective quest=\"{id}\"/>` — a referenced no-start child \
                     activates when its parent activates and does not accept (subquest design 2026-08-31 §2.4)"
                ),
                span,
            ));
        }
    }
    out
}

/// Same-doc set of quest ids named by any `<objective quest=…>` — the
/// mock validator's own view of the subquest child→parent edges (spec
/// §2.4); mirrors `walk::build_child_parent_map` intentionally rather
/// than sharing it (both are tiny AST scans; keeping them local keeps
/// mock.rs free of a walk.rs dependency).
fn referenced_child_ids(doc: &Document) -> BTreeSet<&str> {
    let mut out = BTreeSet::new();
    for q in &doc.quests {
        for node in &q.body {
            let Node::Objective(o) = node else { continue };
            let Some(child) = &o.quest else { continue };
            if !child.is_empty() {
                out.insert(child.as_str());
            }
        }
    }
    out
}

/// `--entry <id>` validation (dsl 0.19.0 §8): the traced document must be
/// `kind: lore` and declare an `<entry id="<id>">` — [`E_TRACE_ENTRY`]
/// otherwise, naming the declared ids so the fix is one copy away.
pub(crate) fn validate_entry(folded: &FoldedEnv, doc: &Document, id: &str) -> Vec<Diagnostic> {
    let span = synthetic_span();
    if folded.doc_kind != lute_check::DocKind::Lore {
        return vec![diag(
            E_TRACE_ENTRY,
            format!(
                "`--entry {id}` needs a `kind: lore` document; this one declares no entries \
                 (dsl 0.19.0 §8)"
            ),
            span,
        )];
    }
    if doc.entries.iter().any(|e| e.id == id) {
        return Vec::new();
    }
    let declared: Vec<&str> = doc.entries.iter().map(|e| e.id.as_str()).collect();
    let canonical = |local: &str| match folded.typed.id.as_deref() {
        Some(doc_id) => lute_check::bundle_beat_key(doc_id, local),
        None => local.to_string(),
    };
    let beats: Vec<String> = doc.beats.iter().map(|b| canonical(&b.id)).collect();
    let is_beat = doc.beats.iter().any(|b| b.id == id || canonical(&b.id) == id);
    let beat_hint = if is_beat {
        format!(" — `{id}` is a `<beat>`: present it with `--beat {id}`")
    } else if beats.is_empty() {
        String::new()
    } else {
        format!(", and beats (`--beat`): {}", beats.join(", "))
    };
    vec![diag(
        E_TRACE_ENTRY,
        format!(
            "`--entry {id}` names an unknown entry id `{id}`; this document declares entries: \
             {}{beat_hint} (dsl 0.19.0 §8)",
            if declared.is_empty() { "none".to_string() } else { declared.join(", ") }
        ),
        span,
    )]
}

/// `--beat <id>` resolution (dsl 0.23.0 §4): the traced document must be
/// `kind: lore` and declare a `<beat>` whose local id is `id` or whose
/// canonical id `<document id>.<beat id>` is `id`. Returns the beat's index
/// in `doc.beats` and its canonical id; [`E_TRACE_BEAT`] otherwise, naming
/// the declared canonical ids so the fix is one copy away.
pub(crate) fn resolve_beat(
    folded: &FoldedEnv,
    doc: &Document,
    id: &str,
) -> Result<(usize, String), Diagnostic> {
    let span = synthetic_span();
    if folded.doc_kind != lute_check::DocKind::Lore {
        return Err(diag(
            E_TRACE_BEAT,
            format!(
                "`--beat {id}` needs a `kind: lore` document; this one declares no `<beat>` \
                 (dsl 0.23.0 §4)"
            ),
            span,
        ));
    }
    let canonical = |local: &str| match folded.typed.id.as_deref() {
        Some(doc_id) => lute_check::bundle_beat_key(doc_id, local),
        None => local.to_string(),
    };
    let found = doc
        .beats
        .iter()
        .position(|b| b.id == id)
        .or_else(|| doc.beats.iter().position(|b| canonical(&b.id) == id));
    if let Some(i) = found {
        return Ok((i, canonical(&doc.beats[i].id)));
    }
    let declared: Vec<String> = doc.beats.iter().map(|b| canonical(&b.id)).collect();
    let declared = if declared.is_empty() {
        "no `<beat>`".to_string()
    } else {
        declared.join(", ")
    };
    Err(diag(
        E_TRACE_BEAT,
        format!(
            "`--beat {id}` names an unknown beat id `{id}`; this document declares: {declared} \
             (dsl 0.23.0 §4)"
        ),
        span,
    ))
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
