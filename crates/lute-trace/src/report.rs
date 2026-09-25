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

use std::collections::{BTreeMap, BTreeSet};

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
        /// The authored `## <title>` heading. Already in the IR
        /// (`"shots":[{"shot":1,"heading":"Hydroponics"}]`); the transcript
        /// printed an ordinal over it (#10 row h, T7.10).
        heading: String,
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
        /// `true` when this `::auto` ends a character's presence — its
        /// `action=` value is in the resolved `action` domain's `exits:`.
        /// The entrance and the exit are the same construct with the same
        /// attribute names, and the entire difference lives in a list in
        /// another file (#32, T2.5). Also `true` on a `::clear` (dsl 0.24.0
        /// §4), which ends every presence on stage.
        exit: bool,
        /// `::end`'s `reason=`. Not one attribute among several: it is the
        /// terminator's entire payload, the only thing distinguishing it from
        /// falling off the end of the document (#32, T5.9).
        reason: Option<String>,
    },
    /// dsl 0.24.0 §5: the plugin call just recorded as a [`Step::Directive`]
    /// reads a bridge result. `answered` is the `bridges:` answer it consumed
    /// (`(field, literal)`, the mock's order), `None` when the mock gave it
    /// none — its result slots then read unknown.
    Bridge {
        tag: String,
        answered: Option<Vec<(String, String)>>,
    },
    /// dsl 0.16.0 §3 D-D: a declarative `<reward/>` fires at a fresh
    /// lifecycle transition — objective grants at first `done`, quest
    /// grants at fresh `complete`/`failed` (§2.3 cascade included). The
    /// step carries the granting quest id, the granting objective id
    /// (`Some` for an objective-level grant, `None` for a quest-level
    /// one), the reward's declaration data (`kind`/`target`/amount —
    /// range bounds are carried verbatim, spec D-C: never pre-rolled),
    /// and `on_failed: true` when the grant was on a `<reward on="failed"/>`
    /// entry firing at the fresh `failed` transition (authored fail OR
    /// §2.3 cascade). The reward's `when=` gate was already evaluated at
    /// the grant instant; a grant that fires is unconditionally true —
    /// the `when` text is not surfaced here.
    Grant {
        quest: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        objective: Option<String>,
        reward: GrantReward,
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        on_failed: bool,
        /// dsl 0.23.0 §8: the state path the reward kind's `credits:` names
        /// and the value it holds after this grant added its scalar amount —
        /// the same credit `lute run` / `lute play` apply.
        #[serde(skip_serializing_if = "Option::is_none")]
        credited: Option<GrantCredit>,
    },
    Decision(Decision),
    /// dsl 0.19.0 §8 (`lute trace --entry`): the presented `<entry>`'s head.
    /// `first_read` is `!entry.<id>.read` at presentation — `true` means its
    /// `::set`/`::assert`/`::retract` apply; `false` (the mock seeded
    /// `entry.<id>.read: true`) means they are reported as [`Step::Skipped`].
    /// `eligible` is the entry's `when` against the mocked state: `true` when
    /// absent or true, `false` when decided false, `null` when unknown (the
    /// guard is then also listed in `unresolved`). Trace presents the entry
    /// either way — eligibility is the engine's gate, shown, not enforced.
    Entry {
        id: String,
        first_read: bool,
        eligible: Option<bool>,
    },
    /// dsl 0.23.0 §4 (`lute trace --beat`): the presented bundle `<beat>`'s
    /// head. `id` is the canonical `<document id>.<beat id>`. `eligible` is
    /// the beat's `when` against the mocked state, exactly as on
    /// [`Step::Entry`] — shown, not enforced. A bundle beat is presented
    /// like a scene beat, so there is no first-read distinction: every
    /// effect of its body applies.
    Beat {
        id: String,
        eligible: Option<bool>,
    },
    /// A first-read-only effect record NOT applied on a re-read (dsl 0.19.0
    /// §6, `docs/runtime/lore-entries.md`): `effect` is `set`/`assert`/
    /// `retract`, `text` the authored write (`run.x += 1`, `knows(a, b)`).
    Skipped {
        effect: String,
        text: String,
    },
    /// dsl 0.21.0 §7a.3: an `::accept{quest="<id>"}` — the player accepts
    /// an accept-driven quest here. A trace walks one document, and a scene
    /// never holds quests, so the accept is recorded, not applied. dsl
    /// 0.24.0 §2: `next_run` marks `at="nextRun"` — queued until the next
    /// run start (serialized `nextRun: true` only then).
    Accept {
        quest: String,
        #[serde(rename = "nextRun", skip_serializing_if = "std::ops::Not::not")]
        next_run: bool,
    },
}

/// dsl 0.16.0 §3: the reward-declaration data carried by a fired [`Step::Grant`],
/// mirroring `lute-compile`'s `RewardEntry` MINUS the `when` slot and the
/// `on` marker. `when` was already evaluated at the grant instant (a fired
/// grant is unconditionally true), and the `on="failed"` marker lifts to
/// [`Step::Grant::on_failed`] on the outer transcript entry so a consumer
/// reads the transition kind without unpacking the reward record.
///
/// Exactly one of `amount` XOR (`amount_min` + `amount_max`) is present
/// after amount defaulting (unauthored → `amount: 1`, spec Global
/// Constraints). Range bounds serialize verbatim; a runner never
/// pre-rolls a value here (D-C).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrantReward {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_min: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_max: Option<i64>,
}

/// dsl 0.23.0 §8: a grant's credit — the path its kind `credits:` and the
/// effective value there after the grant (`unknown` when the path had no
/// value to add to).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct GrantCredit {
    pub path: String,
    pub value: String,
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
    /// dsl 0.24.0 T3-12: the author's text for `id` (a `<match>` subject)
    /// when `@def`/`$` expansion rewrote it — what the human transcript
    /// shows unless asked to expand. Additive key, absent when unchanged.
    #[serde(rename = "authoredId", skip_serializing_if = "Option::is_none")]
    pub authored_id: Option<String>,
    /// The same for `guard`.
    #[serde(rename = "authoredGuard", skip_serializing_if = "Option::is_none")]
    pub authored_guard: Option<String>,
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

impl UnresolvedEntry {
    /// The transcript's line for a construct that HALTED on (or left
    /// undecided) this entry, naming the mocks that would decide it.
    pub fn render_unresolved(&self) -> String {
        format!(
            "unresolved: {} `{}` ({} {}) — supply {} as a mock",
            self.construct,
            self.expression,
            self.id,
            self.construct,
            self.atoms.join(", ")
        )
    }

    /// The line for a selection forced past this undecided guard
    /// ([`TraceReport::forced_unknown`]): the walk continued, so this names
    /// what was never decided rather than why the walk stopped.
    pub fn render_forced(&self) -> String {
        let supply = if self.atoms.is_empty() {
            String::new()
        } else {
            format!(" — supply {} as a mock to decide it", self.atoms.join(", "))
        };
        format!(
            "unresolved (forced): {} `{}` choice guard `{}` was unknown{supply}",
            self.construct, self.id, self.expression
        )
    }
}

/// Visited/total counts for one construct (§4.6: `"choices visited 1/3
/// (sofaHelp), arms 1/2 (match run.metHelpfully)"`), plus the construct's
/// authored LABEL. The label is not the identity — that is the whole point of
/// #24/T9.13: six `<match on="true">` blocks share a label and are six
/// constructs.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct CoverageCount {
    pub visited: usize,
    pub total: usize,
    /// A `<branch>`/`<hub>`'s declared `id`, or a `<match>` subject's raw
    /// (post-expand) CEL text. Rendered beside the site; never keyed on.
    pub label: String,
    /// dsl 0.24.0 T3-12: a `<match>` subject's authored text when expansion
    /// rewrote it (see [`Decision::authored_id`]).
    #[serde(rename = "authoredLabel", skip_serializing_if = "Option::is_none")]
    pub authored_label: Option<String>,
}

/// Coverage counters per construct. `choices` keys a `<branch>`/`<hub>` by its
/// declared `id`, which is document-unique by `E-DUP-BRANCH`. `arms` keys a
/// `<match>` by its own SITE — `"{line}:{column}"` of its span — because a
/// `<match>` has no id at all, and keying on the subject's TEXT is what made
/// eight blocks render as three rows, with `3/3` certifying a set of six
/// blocks no traced path ever visited together (#24, T9.13). A string key
/// keeps `render_json` total and the order deterministic (§4.5).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Coverage {
    pub choices: BTreeMap<String, CoverageCount>,
    pub arms: BTreeMap<String, CoverageCount>,
}

/// The site key for a construct's span: `"{line}:{column}"`. Document-unique
/// by construction — no two constructs open at one position.
pub fn site_key(span: &Span) -> String {
    format!("{}:{}", span.line, span.column)
}

/// The §4.5 output contract. Field order (declaration order = serde
/// serialization order) is NORMATIVE: `file`, `seeds`, `steps`,
/// `decisions`, `unresolved`, `coverage`. `notes`, `disposition` and
/// `endReason` are ADDITIVE §3.1 keys (0.4 §4.5: "implementations MAY add
/// keys") and sit AFTER the six normative fields, which are unreordered.
/// `notes` is informational signage only, never consulted for the
/// exit-code/fact-set decision.
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
    /// How the walk ENDED, as opposed to what it produced: `"complete"` (ran
    /// out of nodes), `"ended"` (an `::end` terminated it), `"incomplete"`
    /// (an unknown guard halted it), `"refused"`. §3.1 additive key — a
    /// harness could not previously tell a terminated walk from a spent one
    /// (#32, T5.9).
    pub disposition: String,
    /// The `reason=` of the `::end` that terminated the walk, when one did.
    /// §3.1 additive key.
    #[serde(rename = "endReason")]
    pub end_reason: Option<String>,
    /// Every `--choose`/`choose:` selection the walk FORCED past a guard that
    /// decided `unknown` (§4.4 permits it and records the decision as
    /// `forced`). The walk went on as asked, so the exit code is unchanged —
    /// but the guard was never decided, so it is counted as unresolved in the
    /// summary and its atoms are named like any other unresolved entry.
    /// Before 0.21.1 the only trace of it was a `(forced)` suffix on the
    /// decision line, and the walk read as a plain `complete`. §3.1 additive
    /// key.
    #[serde(rename = "forcedUnknown")]
    pub forced_unknown: Vec<UnresolvedEntry>,
    /// The effective value of every state path that has one when the walk
    /// ends, rendered as display text — §4.3's read order (trace write →
    /// mock seed → declared `default:`), the same order every guard in the
    /// walk read through; an undecided write reads `"unknown"`. A harness
    /// asserting final state (`lute test`'s `expect.state`) compares against
    /// THIS, so a declared default or a seed is a value, not "never written".
    /// Not part of the §4.5 JSON contract (the transcript's `set` steps
    /// already carry every write), hence never serialized.
    #[serde(skip)]
    pub final_state: BTreeMap<String, String>,
    /// Every fact that holds when the walk ends, after derivation (unless
    /// `derive: false`), rendered `rel(a, b)` — what `lute test`'s
    /// `expect.facts` / `notFacts` judge, as `lute play`'s end-of-play
    /// expectations do. Never serialized, like `final_state`.
    #[serde(skip)]
    pub final_facts: BTreeSet<String>,
    /// Derived relations whose derivation read undecided state at the end —
    /// a fact of one neither holds nor fails to hold.
    #[serde(skip)]
    pub final_undecided: BTreeSet<String>,
    /// The foreign quest ids (read or mocked, declared by no `<quest>` of
    /// this document) the "existence is unverified" notes name — what
    /// [`TraceReport::verify_quests`] settles against a project. Never
    /// serialized.
    #[serde(skip)]
    pub foreign_quests: BTreeSet<String>,
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
    /// atoms. A `<match>` subject and a guard read as the author wrote them
    /// (`@weekday`, `@atLeast(2)`, dsl 0.24.0 T3-12); see
    /// [`Self::render_human_expanded`].
    pub fn render_human(&self) -> String {
        self.render(false)
    }

    /// [`Self::render_human`] with every `@def`/`$` shown expanded — the
    /// text the walk evaluated (`lute trace --expand`).
    pub fn render_human_expanded(&self) -> String {
        self.render(true)
    }

    fn render(&self, expand: bool) -> String {
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
            render_step(step, &mut out, expand);
        }
        let forced = self.forced_unknown.len();
        let forced_summary = if forced == 0 {
            String::new()
        } else {
            format!(
                "; {forced} unresolved (forced past an unknown guard{} — the walk continued, \
                 exit unchanged)",
                if forced == 1 { "" } else { "s" }
            )
        };
        if self.unresolved.is_empty() {
            out.push_str(&format!(
                "trace complete: {} decision{}{forced_summary}",
                self.decisions.len(),
                if self.decisions.len() == 1 { "" } else { "s" }
            ));
        } else {
            out.push_str(&format!(
                "trace incomplete: {} unresolved atom{} (exit 3){forced_summary}",
                self.unresolved.len(),
                if self.unresolved.len() == 1 { "" } else { "s" }
            ));
            for u in &self.unresolved {
                out.push_str(&format!("\n  {}", u.render_unresolved()));
            }
        }
        if !self.coverage.choices.is_empty() || !self.coverage.arms.is_empty() {
            let mut parts = Vec::new();
            for c in self.coverage.choices.values() {
                parts.push(format!("choices {}/{} ({})", c.visited, c.total, c.label));
            }
            for (site, c) in &self.coverage.arms {
                let label = match (&c.authored_label, expand) {
                    (Some(a), false) => a,
                    _ => &c.label,
                };
                parts.push(format!("arms {}/{} ({label} @{site})", c.visited, c.total));
            }
            out.push_str(&format!("; {}", parts.join(", ")));
        }
        out.push('\n');
        for u in &self.forced_unknown {
            out.push_str(&format!("  {}\n", u.render_forced()));
        }
        out
    }

    /// The walk's presented content lines, one `@speaker: text` per content
    /// line the walk played, in order — the canonical form `lute test`'s
    /// `transcriptContains` / `transcriptLacks` match (dsl 0.24.0, T1-2),
    /// identical to what `lute play` matches its own presentations against.
    /// No header, notes, staging, writes or decisions.
    pub fn said(&self) -> String {
        let mut out = String::new();
        for step in &self.steps {
            if let Step::Line { speaker, text } = step {
                out.push_str(&format!("@{speaker}: {text}\n"));
            }
        }
        out
    }

    /// `lute trace --project` (dsl 0.24.0, T3-15): settle every "quest
    /// `<id>`'s existence is unverified" note against the project's declared
    /// quest ids. A declared quest's note is dropped — its existence IS
    /// verified; an undeclared one's note says so, naming the nearest
    /// declared id when one is close.
    pub fn verify_quests(&mut self, declared: &BTreeSet<String>) {
        for id in std::mem::take(&mut self.foreign_quests) {
            let head = crate::walk::unverified_quest_note_head(&id);
            let Some(at) = self.notes.iter().position(|n| n.starts_with(&head)) else {
                continue;
            };
            if declared.contains(&id) {
                self.notes.remove(at);
                continue;
            }
            let hint = lute_manifest::suggest::nearest(&id, declared.iter().map(String::as_str), 2)
                .map(|s| format!(" — did you mean `{s}`?"))
                .unwrap_or_default();
            self.notes[at] = format!(
                "quest `{id}` is declared by no quest document of the project{hint} (every read \
                 of `quest.{id}.*` takes its reserved default)"
            );
        }
    }
}

fn render_step(step: &Step, out: &mut String, expand: bool) {
    match step {
        Step::Shot { number, heading } => {
            if heading.is_empty() {
                out.push_str(&format!("  ## Shot {number}.\n"));
            } else {
                out.push_str(&format!("  ## {heading}\n"));
            }
        }
        Step::Line { speaker, text } => out.push_str(&format!("    @{speaker}  {text}\n")),
        Step::Set { path, value, sugar } => {
            let annot = if *sugar { "  (into sugar)" } else { "" };
            out.push_str(&format!("    ::set  {path} = {value}{annot}\n"));
        }
        Step::Assert { text } => out.push_str(&format!("    ::assert  {text}\n")),
        Step::Retract { text } => out.push_str(&format!("    ::retract  {text}\n")),
        Step::Directive {
            tag,
            component_boundary,
            exit,
            reason,
        } => match component_boundary {
            // §3.3: `tag` on a boundary step IS the internal
            // `__component-begin`/`-end` sentinel (`normalize.rs`'s
            // `COMPONENT_BEGIN`/`COMPONENT_END`) — never interpolated into
            // the human transcript (it would both leak the sentinel name
            // and double the marker word, "begin begin"/"end end").
            Some(ComponentBoundary::Begin) => out.push_str("    -- component begin --\n"),
            Some(ComponentBoundary::End) => out.push_str("    -- component end --\n"),
            None => {
                let annot = match (exit, reason) {
                    (true, _) => " exit".to_string(),
                    (false, Some(r)) => format!(" reason={r}"),
                    (false, None) => String::new(),
                };
                out.push_str(&format!("    <{tag}{annot}>\n"));
            }
        },
        Step::Bridge { tag, answered } => match answered {
            Some(fields) => {
                let fields: Vec<String> = fields.iter().map(|(f, v)| format!("{f}={v}")).collect();
                out.push_str(&format!("      (bridge answered: {})\n", fields.join(", ")));
            }
            None => out.push_str(&format!(
                "      (bridge unanswered: no `bridges.{tag}` answer — its results read unknown)\n"
            )),
        },
        Step::Decision(d) => {
            let annot = if d.forced {
                " (forced)"
            } else if d.auto {
                " (auto)"
            } else {
                ""
            };
            // T3-12: the author's `@def(args)` unless `--expand`.
            let (id, guard) = if expand {
                (&d.id, &d.guard)
            } else {
                (
                    d.authored_id.as_ref().unwrap_or(&d.id),
                    if d.authored_guard.is_some() { &d.authored_guard } else { &d.guard },
                )
            };
            let guard = guard.as_deref().map(|g| format!(" ({g})")).unwrap_or_default();
            let eligible = if d.eligible.is_empty() {
                String::new()
            } else {
                format!("   eligible: {}", d.eligible.join(", "))
            };
            out.push_str(&format!(
                "  <{} {}>{}   -> {}{}{}\n",
                d.construct, id, eligible, d.outcome, guard, annot
            ));
        }
        Step::Entry {
            id,
            first_read,
            eligible,
        } => {
            let read = if *first_read {
                "first read"
            } else {
                "re-read: effects skipped"
            };
            let gate = match eligible {
                Some(true) => "",
                Some(false) => ", not eligible (`when` is false)",
                None => ", eligibility unknown",
            };
            out.push_str(&format!("  <entry {id}>   ({read}{gate})\n"));
        }
        Step::Beat { id, eligible } => {
            let gate = match eligible {
                Some(true) => "",
                Some(false) => "   (not eligible: `when` is false)",
                None => "   (eligibility unknown)",
            };
            out.push_str(&format!("  <beat {id}>{gate}\n"));
        }
        Step::Skipped { effect, text } => {
            out.push_str(&format!("    ::{effect}  {text}  (skipped: re-read)\n"));
        }
        Step::Accept { quest, next_run } => {
            let queued = if *next_run { " (queued: applies after the next run start)" } else { "" };
            out.push_str(&format!("    quest {quest} accepted{queued}\n"))
        }
        Step::Grant {
            quest,
            objective,
            reward,
            on_failed,
            credited,
        } => {
            let owner = match objective {
                Some(oid) => format!("{quest}.{oid}"),
                None => quest.clone(),
            };
            let amount = match (reward.amount, reward.amount_min, reward.amount_max) {
                (Some(n), _, _) => n.to_string(),
                (None, Some(lo), Some(hi)) => format!("{lo}..{hi}"),
                _ => "?".to_string(),
            };
            let target = reward
                .target
                .as_deref()
                .map(|t| format!(" -> {t}"))
                .unwrap_or_default();
            let annot = if *on_failed { " (on failed)" } else { "" };
            let credit = credited
                .as_ref()
                .map(|c| format!(" (credits {} = {})", c.path, c.value))
                .unwrap_or_default();
            out.push_str(&format!(
                "    grant {owner}  {} {}{}{}{}\n",
                reward.kind, amount, target, annot, credit
            ));
        }
    }
}
