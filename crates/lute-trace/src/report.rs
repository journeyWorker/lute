//! §4.5 output contract: the deterministic [`TraceReport`] + the human/JSON
//! renderers. Field declaration order on [`TraceReport`] IS the JSON key
//! order (serde struct-order keys — `serde_json` never reorders a struct's
//! fields) and matches §4.5's normative list VERBATIM: `file`, `seeds`,
//! `steps`, `decisions`, `unresolved`, `coverage`. Never reorder these
//! fields without re-reading §4.5 first.
//!
//! [`crate::walk`] builds one [`TraceReport`] per [`crate::walk::trace_document`]
//! call; this module owns only the SHAPE and the two renderers — it holds no
//! walk logic.

use std::collections::BTreeMap;

use lute_core_span::{Diagnostic, Span};
use serde::Serialize;

use crate::value::Value;

/// Exit-code contract (§4.5): `Complete` -> 0, `Refused` -> 1 (check errors,
/// invalid mocks, or a walk-time forced-guard-false `E-TRACE-CHOICE`),
/// `Incomplete` -> 3 (an `unknown` guard halted the walk, or D20's
/// none-true-some-unknown auto-selection). Exit `2` (I/O) is the CLI's own
/// concern (Task 21) — never produced here.
#[derive(Clone, Debug, PartialEq)]
pub enum TraceExit {
    Complete,
    Refused(Vec<Diagnostic>),
    Incomplete,
}

impl TraceExit {
    /// The §4.5 exit code this variant maps to.
    pub fn code(&self) -> i32 {
        match self {
            TraceExit::Complete => 0,
            TraceExit::Refused(_) => 1,
            TraceExit::Incomplete => 3,
        }
    }
}

/// Mock-seed counts (§4.6 human form: `"seeds: N paths, M facts; K
/// selection(s)"`) — a summary, not the raw mock content (which the human
/// transcript's decisions/steps already surface as they are consumed).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Seeds {
    pub state_paths: usize,
    pub facts: usize,
    pub choices: usize,
}

/// A component `::use` boundary annotation (D8's `__component-begin`/`-end`
/// sentinels, normalize.rs) — rendered so a trace reader can tell inlined
/// component content apart from the authoring document's own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ComponentBoundary {
    Begin,
    End,
}

/// One entry of the linear, document-ordered transcript (§4.5 human form:
/// "emitted content lines ..., staging directives, state writes, and one
/// line per decision"). `Decision` steps are ALSO indexed separately in
/// [`TraceReport::decisions`] — a `Step::Decision` is the SAME event,
/// inline in transcript position; `decisions[]` is the queryable index over
/// them.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Step {
    Shot {
        number: i64,
    },
    Line {
        speaker: String,
        text: String,
    },
    Set {
        path: String,
        value: String,
        /// `true` for a `<choice into="...">`-synthesized write (D8's
        /// `synth_into`) — rendered `(into sugar)` in the human form (D14's
        /// "`(persist sugar)`" precedent, §4.6, relabeled by 0.6.0 §2.1).
        sugar: bool,
    },
    Assert {
        text: String,
    },
    Retract {
        text: String,
    },
    Directive {
        tag: String,
        component_boundary: Option<ComponentBoundary>,
    },
    Decision(Decision),
}

/// One decision (§4.5: "the construct kind, its id/span, the outcome, and
/// the evaluated guard bindings"). `construct` is `"match"` / `"branch"` /
/// `"hub"`; `id` is the match subject's raw CEL text or the branch/hub's
/// declared `id`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Decision {
    pub construct: String,
    pub id: String,
    pub span: Span,
    /// The winning arm/choice: an arm index + rendered guard for a match,
    /// the chosen `<choice id>` for a branch/hub pick.
    pub outcome: String,
    /// The evaluated guard text and its read value, when the winning
    /// arm/choice carried one (`None` for an unguarded choice / the
    /// `<otherwise>` arm).
    pub guard: Option<String>,
    /// `--choose` selected this outcome past a guard that was `false`
    /// (branch/hub forcing already refused the walk before recording — see
    /// [`TraceExit::Refused`]) — this flag is set ONLY for the `unknown`
    /// forced-past case (§4.4: "forcing past an unknown guard is
    /// permitted... and reported as forced").
    pub forced: bool,
    /// No `--choose` entry named this construct; the walk auto-picked the
    /// first eligible arm/choice in document order (§4.4).
    pub auto: bool,
    /// Branch/hub only: every choice id eligible at THIS presentation point
    /// (§4.6: `"eligible: help, warmly, tip"`) — `Bool(true)`- or
    /// `Unknown`-guarded choices, in document order. Empty for a `match`
    /// decision (arm eligibility is inherently first-match-wins, not a
    /// menu).
    pub eligible: Vec<String>,
}

/// Why a construct HALTED the walk (§4.4/§4.5: "unresolved\[\] carries the
/// span, expression, and the atoms that need mocks").
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct UnresolvedEntry {
    pub construct: String,
    pub id: String,
    pub span: Span,
    pub expression: String,
    /// Rendered "supply it as a mock" hints (§4.6), one per
    /// [`crate::value::UnresolvedAtom`] the guard's evaluation recorded.
    pub atoms: Vec<String>,
}

/// Visited/total counts for one construct (§4.6: `"choices visited 1/3
/// (sofaHelp), arms 1/2 (match run.metHelpfully)"`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct CoverageCount {
    pub visited: usize,
    pub total: usize,
}

/// Coverage counters per construct (§4.4's own text): `choices` keys a
/// `<branch>`/`<hub>` by its declared `id`; `arms` keys a `<match>` by its
/// subject's raw (post-expand) CEL text — the only stable label a `<match>`
/// carries, since it has no `id` attribute. `BTreeMap` (deterministic key
/// order, §4.5).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Coverage {
    pub choices: BTreeMap<String, CoverageCount>,
    pub arms: BTreeMap<String, CoverageCount>,
}

/// The §4.5 output contract. Field order (declaration order = serde
/// serialization order) is NORMATIVE: `file`, `seeds`, `steps`,
/// `decisions`, `unresolved`, `coverage`. `notes` is an ADDITIVE §3.1 key
/// (0.4 §4.5: "implementations MAY add keys") — informational signage
/// only, never consulted for the exit-code/fact-set decision.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TraceReport {
    pub file: String,
    pub seeds: Seeds,
    pub steps: Vec<Step>,
    pub decisions: Vec<Decision>,
    pub unresolved: Vec<UnresolvedEntry>,
    pub coverage: Coverage,
    /// §3.1: an informational (never error, never reachability) note when
    /// the resolved schema declares seed `facts:` but none were supplied
    /// as mocks — names at least one declared-but-un-supplied relation.
    /// Empty on every other run, including a `Refused` (empty) report.
    pub notes: Vec<String>,
}

/// Render a decided [`Value`] to display text; `Unknown` has no decided
/// text (the caller renders `"unknown"` or keeps the source verbatim,
/// context-dependent — this stays a `None` rather than picking one for
/// them).
pub(crate) fn value_text(v: &Value) -> Option<String> {
    match v {
        Value::Unknown => None,
        Value::Bool(b) => Some(b.to_string()),
        Value::Str(s) => Some(s.clone()),
        Value::Num(n) => Some(format_num(*n)),
    }
}

/// Integral floats render without a trailing `.0` (matches
/// `lute_compile::literal_json`'s envelope convention: `0`, not `0.0`).
pub(crate) fn format_num(n: f64) -> String {
    if n.is_finite() && n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        n.to_string()
    }
}

impl TraceReport {
    /// §4.5 machine form: pretty JSON, struct-field key order (`file`,
    /// `seeds`, `steps`, `decisions`, `unresolved`, `coverage`) — byte-
    /// identical across runs for identical inputs (§4.5 determinism).
    /// Serialization of this shape is total (no non-finite floats, no
    /// non-string map keys), so this never panics.
    pub fn render_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("TraceReport is always JSON-serializable")
    }

    /// §4.5 human form: an indented, ordered transcript — one line per
    /// [`Step`] (shots, content lines with interpolations already resolved
    /// where decided, staging directives, state writes, decisions) — plus a
    /// trailing summary of decisions taken, coverage, and any unresolved
    /// atoms.
    pub fn render_human(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "trace: {}  (seeds: {} paths, {} facts; {} selection{})\n",
            self.file,
            self.seeds.state_paths,
            self.seeds.facts,
            self.seeds.choices,
            if self.seeds.choices == 1 { "" } else { "s" }
        ));
        for note in &self.notes {
            out.push_str(&format!("note: {note}\n"));
        }
        for step in &self.steps {
            render_step(step, &mut out);
        }
        if self.unresolved.is_empty() {
            out.push_str(&format!(
                "trace complete: {} decision{}",
                self.decisions.len(),
                if self.decisions.len() == 1 { "" } else { "s" }
            ));
        } else {
            out.push_str(&format!(
                "trace incomplete: {} unresolved atom{} (exit 3)",
                self.unresolved.len(),
                if self.unresolved.len() == 1 { "" } else { "s" }
            ));
            for u in &self.unresolved {
                out.push_str(&format!(
                    "\n  unresolved: {} `{}` ({} {}) — supply {} as a mock",
                    u.construct,
                    u.expression,
                    u.id,
                    u.construct,
                    u.atoms.join(", ")
                ));
            }
        }
        if !self.coverage.choices.is_empty() || !self.coverage.arms.is_empty() {
            let mut parts = Vec::new();
            for (id, c) in &self.coverage.choices {
                parts.push(format!("choices {}/{} ({id})", c.visited, c.total));
            }
            for (id, c) in &self.coverage.arms {
                parts.push(format!("arms {}/{} ({id})", c.visited, c.total));
            }
            out.push_str(&format!("; {}", parts.join(", ")));
        }
        out.push('\n');
        out
    }
}

fn render_step(step: &Step, out: &mut String) {
    match step {
        Step::Shot { number } => out.push_str(&format!("  ## Shot {number}.\n")),
        Step::Line { speaker, text } => out.push_str(&format!("    @{speaker}  {text}\n")),
        Step::Set { path, value, sugar } => {
            let annot = if *sugar { "  (into sugar)" } else { "" };
            out.push_str(&format!("    ::set  {path} = {value}{annot}\n"));
        }
        Step::Assert { text } => out.push_str(&format!("    ::assert  {text}\n")),
        Step::Retract { text } => out.push_str(&format!("    ::retract  {text}\n")),
        Step::Directive { tag, component_boundary } => match component_boundary {
            // §3.3: `tag` on a boundary step IS the internal
            // `__component-begin`/`-end` sentinel (`normalize.rs`'s
            // `COMPONENT_BEGIN`/`COMPONENT_END`) — never interpolated into
            // the human transcript (it would both leak the sentinel name
            // and double the marker word, "begin begin"/"end end").
            Some(ComponentBoundary::Begin) => out.push_str("    -- component begin --\n"),
            Some(ComponentBoundary::End) => out.push_str("    -- component end --\n"),
            None => out.push_str(&format!("    <{tag}>\n")),
        },
        Step::Decision(d) => {
            let annot = if d.forced {
                " (forced)"
            } else if d.auto {
                " (auto)"
            } else {
                ""
            };
            let guard = d.guard.as_deref().map(|g| format!(" ({g})")).unwrap_or_default();
            let eligible = if d.eligible.is_empty() {
                String::new()
            } else {
                format!("   eligible: {}", d.eligible.join(", "))
            };
            out.push_str(&format!(
                "  <{} {}>{}   -> {}{}{}\n",
                d.construct, d.id, eligible, d.outcome, guard, annot
            ));
        }
    }
}
