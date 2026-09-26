use lute_core_span::{Span, StableId};

#[derive(Clone, Debug)]
pub struct Document {
    pub meta: Meta,
    pub title: Option<(String, Span)>,
    pub shots: Vec<Shot>,
    pub quests: Vec<Quest>,
    /// Top-level `<entry>` declarations (dsl 0.19.0 §2), in document order.
    pub entries: Vec<Entry>,
    /// Top-level `<beat>` declarations of a lore document (dsl 0.23.0 §4,
    /// beat bundles), in document order.
    pub beats: Vec<BundleBeat>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Meta {
    pub raw_yaml: String,
    pub span: Span,
} // parsed into typed form in check

#[derive(Clone, Debug)]
pub struct Shot {
    pub heading: String,
    pub body: Vec<Node>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum Node {
    Line(Line),
    Directive(Directive),
    Set(Set),
    Branch(Branch),
    Match(Match),
    Timeline(Timeline),
    Hub(Hub),
    Objective(Objective),
    On(On),
    Assert(Assert),
    Retract(Retract),
}

#[derive(Clone, Debug)]
pub struct Line {
    pub speaker: String,
    pub attrs: Vec<Attr>,
    /// The gated-line guard (dsl 0.4.0 §7.2): `@s{when="G"}: T` emits the
    /// line iff `G` holds — a `CelKind::Condition` slot, extracted from the
    /// `when` attr the same way `Choice.when` is (`take_cel`, parser.rs). `$`
    /// is NOT in scope (matches `<on when>`). `None` when no `when=` attr was
    /// authored (the common case — B1: parse-identical to pre-0.4.0 docs).
    pub when: Option<CelSlot>,
    pub text: String,
    pub text_span: Span,
    pub interps: Vec<Interp>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Directive {
    pub tag: String,
    pub attrs: Vec<Attr>,
    /// A directive's `when="COND"` guard — `::next` (dsl 0.12.0) and, since
    /// dsl 0.26.0 §4, `::use`, `::accept` and plugin passthrough directives
    /// (skipped when false, like `::set{… when=}`). Extracted into a typed
    /// CEL slot the SAME way `Line.when`/`Choice.when` are (`take_cel`,
    /// parser.rs) for every tag, so it rides the same CEL walk / `StableId` /
    /// `check_cel_slot` path; the checker refuses it where no guard applies
    /// (a builtin-lowered directive, a `<track>` clip).
    pub when: Option<CelSlot>,
    pub span: Span,
}

/// The tag of the core `::accept{quest="<id>"}` directive (dsl 0.21.0 §7a.3):
/// the scene-side "accept quest" action. It is part of the language, not of
/// any capability snapshot, so it is recognized by tag (like `::assert` /
/// `::retract` are recognized by the parser) and never looked up as a plugin
/// directive. Shared by the checker, the compiler (→ `Command::Accept`), and
/// the trace walk.
pub const ACCEPT_DIRECTIVE: &str = "accept";

impl Directive {
    /// True when this is the core [`ACCEPT_DIRECTIVE`].
    pub fn is_accept(&self) -> bool {
        self.tag == ACCEPT_DIRECTIVE
    }

    /// The target of an `::accept{quest="<id>"}` directive: the quoted
    /// `quest` attribute's value and value span, or `None` when this is not
    /// an accept directive or `quest` is missing / not a quoted string. Shape
    /// validity (a plain identifier) is the checker's (`E-ACCEPT-TARGET`).
    pub fn accept_quest(&self) -> Option<(&str, Span)> {
        if !self.is_accept() {
            return None;
        }
        self.attrs
            .iter()
            .find(|a| a.key == "quest")
            .and_then(|a| match &a.value {
                AttrValue::Str(s) => Some((s.as_str(), a.value_span)),
                _ => None,
            })
    }

    /// dsl 0.24.0 §2: `::accept{… at="nextRun"}` — the `at` attribute's
    /// quoted value and value span; `None` when absent (accept now) or not
    /// a quoted string. Value validity is the checker's (`E-ACCEPT-TARGET`).
    pub fn accept_at(&self) -> Option<(&str, Span)> {
        if !self.is_accept() {
            return None;
        }
        self.attrs
            .iter()
            .find(|a| a.key == "at")
            .and_then(|a| match &a.value {
                AttrValue::Str(s) => Some((s.as_str(), a.value_span)),
                _ => None,
            })
    }
}

#[derive(Clone, Debug)]
pub struct Set {
    pub path: String,
    pub path_span: Span,
    pub op: String,
    pub expr: CelSlot,
    pub span: Span,
    /// dsl 0.24.0 §1: `::set{… when="…"}` — the write applies only when this
    /// condition holds. `None` when unguarded. Never set on a `<timeline>`
    /// clip (`E-TIMELINE-CONTENT`: a conditional write is logic).
    pub when: Option<CelSlot>,
}

/// `::assert{ rel(a, b) }` (dsl 0.3.0 §5) — a pure leaf; args are compile-time-ground
/// (no `{{…}}`, no CEL). `pattern.relation.is_empty()` is the parse-failed sentinel (D13).
#[derive(Clone, Debug)]
pub struct Assert {
    pub pattern: crate::datalog::FactPattern,
    /// Byte offset of the payload interior start; pattern spans are relative to it.
    pub pattern_base: usize,
    pub raw: String,
    pub span: Span,
    /// dsl 0.26.0 §4: `::assert{ rel(a) when="…" }` — the write applies only
    /// when this condition holds (the meaning of `::set{… when=}`).
    pub when: Option<CelSlot>,
}

/// `::retract{ rel(a, _) }` (dsl 0.3.0 §5) — mirrors [`Assert`]; wildcard legality
/// is checked downstream (Task 10), not here.
#[derive(Clone, Debug)]
pub struct Retract {
    pub pattern: crate::datalog::FactPattern,
    pub pattern_base: usize,
    pub raw: String,
    pub span: Span,
    /// dsl 0.26.0 §4: the retract's guard, as [`Assert::when`].
    pub when: Option<CelSlot>,
}

#[derive(Clone, Debug)]
pub struct Branch {
    pub id: String,
    pub attrs: Vec<Attr>,
    pub choices: Vec<Choice>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Choice {
    pub id: String,
    pub label: String,
    pub when: Option<CelSlot>,
    pub attrs: Vec<Attr>,
    pub body: Vec<Node>,
    pub span: Span,
}

/// `<match on> When+ Otherwise? "</match>"` (dsl §7.3, §11.2). `attrs` is the
/// residual (post-`on`-extraction) list, mirroring [`Branch`]/[`Hub`]. It is
/// retained rather than dropped so the checker's per-tag attribute closure
/// (dsl 0.10.0 §4, D-J) has something to close over: a rule about attributes
/// the checker never receives is not a rule. Normally empty.
#[derive(Clone, Debug)]
pub struct Match {
    pub subject: CelSlot,
    pub attrs: Vec<Attr>,
    pub arms: Vec<Arm>,
    pub span: Span,
}

/// `<hub id> HubChoice+ </hub>` (dsl §7.3.2). Choices reuse [`Choice`];
/// the `once` / `exit` flags arrive as bare attrs on each choice.
#[derive(Clone, Debug)]
pub struct Hub {
    pub attrs: Vec<Attr>,
    pub choices: Vec<Choice>,
    pub span: Span,
}

/// `<quest id …> QuestBody </quest>` (dsl 0.2.0 §6.3). A TOP-LEVEL declaration
/// (never a [`Node`]); `body` reuses the shared `Node` stream (only the arms
/// admitted by dsl 0.2.0 §6.7 are legal — enforced in lute-check, not here).
/// `start`/`fail` are optional CEL guards; `title` is a localizable String
/// captured raw (interps recovered on demand via `scan_label_interps`).
/// `rewards` collects every self-closing `<reward/>` (dsl 0.16.0 §2) that
/// appeared as a direct child of this quest — the parse loop intercepts the
/// element and folds it here instead of the shared `body` stream, so every
/// existing exhaustive `Node` match stays untouched.
#[derive(Clone, Debug)]
pub struct Quest {
    pub id: String,
    pub id_span: Span,
    pub title: Option<String>,
    pub start: Option<CelSlot>,
    pub fail: Option<CelSlot>,
    /// The prerequisite `after` attribute (connectivity layer, T2): raw CEL
    /// text validated under the restricted `prereq::parse_prereq` grammar
    /// (never the general CEL pipeline — mirrors `<when is="…">`'s
    /// `take_str_spanned` treatment). `after_span` is meaningful only when
    /// `after` is `Some`; it defaults to the quest's open-tag span otherwise.
    pub after: Option<String>,
    pub after_span: Span,
    /// dsl 0.22.0 §7: the quest's lifetime tier, raw text + value span
    /// (`"run"` resets at `newRun`; absent = `user`). The checker validates
    /// the value (`E-ATTR-TYPE`).
    pub tier: Option<(String, Span)>,
    /// dsl 0.24.0 §2: `activate="accept"` — a subquest child that does NOT
    /// activate with its parent but waits for `::accept`. Raw text + value
    /// span; the checker validates the value (`E-ATTR-TYPE`).
    pub activate: Option<(String, Span)>,
    /// dsl 0.24.0 §2: `complete="all" | "any"` — how a parent's derived
    /// completion reads its required objectives. Raw text + value span; the
    /// checker validates the value (`E-ATTR-TYPE`).
    pub complete: Option<(String, Span)>,
    /// dsl 0.25.0 §5: `accept="external"` — the quest is accepted outside
    /// the script (a quest board, a menu, a UI). Raw text + value span; the
    /// checker validates the value (`E-ATTR-TYPE`).
    pub accept: Option<(String, Span)>,
    /// Residual (post-extraction) attrs, mirroring [`Branch`]; normally empty.
    pub attrs: Vec<Attr>,
    pub body: Vec<Node>,
    /// Self-closing `<reward/>` children in declaration order (dsl 0.16.0 §2).
    pub rewards: Vec<Reward>,
    pub span: Span,
}

impl Quest {
    /// dsl 0.24.0 §2: `activate="accept"` — the child waits for `::accept`
    /// instead of activating with its parent.
    pub fn activates_on_accept(&self) -> bool {
        self.activate.as_ref().is_some_and(|(v, _)| v == "accept")
    }

    /// dsl 0.24.0 §2: `complete="any"` — ANY required objective done
    /// completes the quest (the other open children fail `superseded`).
    pub fn completes_on_any(&self) -> bool {
        self.complete.as_ref().is_some_and(|(v, _)| v == "any")
    }

    /// dsl 0.25.0 §5: `accept="external"` — the engine accepts the quest
    /// outside any document, whenever the player chooses.
    pub fn accepted_externally(&self) -> bool {
        self.accept.as_ref().is_some_and(|(v, _)| v == "external")
    }
}

/// `<entry id …> EntryBody </entry>` (dsl 0.19.0 §3–§4). A TOP-LEVEL
/// declaration of a lore document (never a [`Node`]), mirroring [`Quest`]:
/// `body` reuses the shared `Node` stream (the admitted arms — lines,
/// `<match>`, `::set`/`::assert`/`::retract` — are enforced in lute-check,
/// not here). `id` is empty when the attribute is absent or not a quoted
/// string (checker: `E-ENTRY-ATTR`); `id_span` then falls back to the open
/// tag. The optional string attrs carry their value span so the checker can
/// anchor `E-ENTRY-ATTR` at the attribute; `order` stays raw text (the
/// checker validates the non-negative integer). `title` is a localizable
/// String captured raw, like [`Quest::title`].
#[derive(Clone, Debug)]
pub struct Entry {
    pub id: String,
    pub id_span: Span,
    pub target: Option<(String, Span)>,
    pub category: Option<(String, Span)>,
    pub title: Option<(String, Span)>,
    pub series: Option<(String, Span)>,
    pub order: Option<(String, Span)>,
    /// The occasion this entry answers (dsl 0.21.0 §3.2), raw text + value
    /// span; the checker validates the identifier shape (`E-BEAT-ATTR`).
    pub on: Option<(String, Span)>,
    /// Beat priority (dsl 0.21.0 §3.2), raw text like `order`; the checker
    /// validates the integer and that `on` is present (`E-BEAT-ATTR`).
    pub priority: Option<(String, Span)>,
    /// dsl 0.22.0 §7: the entry beat's repetition policy, raw text + value
    /// span (`"run"` / `"user"`; absent = repeatable). The checker validates
    /// the value (`E-BEAT-ATTR`).
    pub once: Option<(String, Span)>,
    /// dsl 0.25.0 §2: the shared-spend key, raw text + value span — beats
    /// with one key are spent together. The checker validates the shape
    /// and that `once` is written (`E-BEAT-ATTR`).
    pub share: Option<(String, Span)>,
    /// Optional eligibility guard (dsl 0.19.0 §3), like [`Quest::start`].
    pub when: Option<CelSlot>,
    /// Residual (post-extraction) attrs, mirroring [`Quest`]; normally empty.
    pub attrs: Vec<Attr>,
    pub body: Vec<Node>,
    pub span: Span,
}

/// `<beat id on …> SceneBody </beat>` (dsl 0.23.0 §4): a scene-like beat
/// declared inside a lore document — a beat bundle. A TOP-LEVEL declaration
/// (never a [`Node`]), mirroring [`Entry`]: `body` is the shared `Node`
/// stream, admitted as a scene shot body (lines, choices, hubs, matches,
/// directives — enforced in lute-check, not here). Its canonical id is
/// `<document id>.<id>`. `id` is empty when the attribute is absent or not a
/// quoted string (checker: `E-BEAT-ATTR`); `id_span` then falls back to the
/// open tag. The string attrs keep their value span so the checker can
/// anchor `E-BEAT-ATTR` at the value; `priority` / `once` stay raw text.
/// `also` is the bare flag (or `also="true"` / `"false"`) with its span; any
/// other `also=` value stays residual in `attrs`.
#[derive(Clone, Debug)]
pub struct BundleBeat {
    pub id: String,
    pub id_span: Span,
    pub on: Option<(String, Span)>,
    pub target: Option<(String, Span)>,
    /// A localizable label (a `select: all` menu names the beat by it),
    /// captured raw like [`Entry::title`].
    pub title: Option<(String, Span)>,
    pub priority: Option<(String, Span)>,
    /// `run` (the default) / `user` / `false`, like a scene's `once:`.
    pub once: Option<(String, Span)>,
    /// dsl 0.25.0 §2: the shared-spend key, like [`Entry::share`].
    pub share: Option<(String, Span)>,
    /// dsl 0.25.0 §3: the prerequisite, raw text + value span — a scene's
    /// `after:` on a bundle beat (an eligibility conjunct and a scenario
    /// edge), validated under the restricted `prereq::parse_prereq`
    /// grammar like [`Quest::after`], never routed through general CEL.
    pub after: Option<(String, Span)>,
    pub also: Option<(bool, Span)>,
    pub when: Option<CelSlot>,
    /// Residual (post-extraction) attrs; normally empty.
    pub attrs: Vec<Attr>,
    pub body: Vec<Node>,
    pub span: Span,
}

/// `<objective id done …> Node* </objective>` or self-closing
/// `<objective … />` (dsl 0.2.0 §6.4). Exactly one of `done`/`quest` carries
/// the completion source: `done` is the authored completion predicate;
/// `quest` (subquest design, 2026-08-31) references a child quest whose
/// completion IS this objective's completion (the predicate is synthesized
/// downstream — `quest.<child>.state == 'complete'` — never authored).
/// `when` gates visibility; `optional` is a bare boolean flag. `rewards`
/// collects every self-closing `<reward/>` (dsl 0.16.0 §2) child, folded
/// out of the shared `body` stream by the parse loop.
#[derive(Clone, Debug)]
pub struct Objective {
    pub id: String,
    pub id_span: Span,
    pub done: CelSlot,
    /// `quest=` child-quest reference (subquest); mutually exclusive with a
    /// non-empty `done` (`E-OBJECTIVE-QUEST-DONE`, checker-owned).
    pub quest: Option<String>,
    /// Span of the `quest=` attribute value; meaningful only when `quest` is
    /// `Some` (defaults to the open-tag span otherwise, mirroring
    /// [`Quest::after_span`]).
    pub quest_span: Span,
    pub when: Option<CelSlot>,
    pub title: Option<String>,
    pub optional: bool,
    /// The occasion at which this objective's `done`/`fail` is judged (dsl
    /// 0.21.0 §7a.2), raw text + value span like [`Entry::on`]; `None` →
    /// evaluated continuously. The checker validates the identifier shape
    /// (`E-BEAT-ATTR`) and the occasion vocabulary (`E-OCCASION-UNKNOWN`).
    pub on: Option<(String, Span)>,
    /// dsl 0.23.0 §2: `by="<condition>"` — while the objective is not done,
    /// the first time it becomes true the objective FAILS (a required
    /// objective fails its quest). A condition slot like `done`.
    pub by: Option<CelSlot>,
    /// dsl 0.23.0 §2: `target=` on an `on=` objective — judged only when the
    /// occasion is raised for that target (the beat target rule). Raw text +
    /// value span like [`Entry::target`].
    pub target: Option<(String, Span)>,
    /// dsl 0.24.0 §2.1: `until="<condition>"` — the place-bound deadline:
    /// judged only when the objective's `on` occasion (and `target`) is
    /// raised, after `done`. A condition slot like `by`.
    pub until: Option<CelSlot>,
    pub attrs: Vec<Attr>,
    pub body: Vec<Node>,
    /// Self-closing `<reward/>` children in declaration order (dsl 0.16.0 §2).
    pub rewards: Vec<Reward>,
    pub span: Span,
}

/// A `<reward kind= target= amount= when= on=/>` element (dsl 0.16.0 §2) —
/// an OWNER FIELD of the enclosing [`Quest`] / [`Objective`], NEVER a
/// [`Node`] variant. The parser accepts only the self-closing form
/// (`self_closing == true`); a body-form `<reward>…</reward>` draws a
/// parse-layer error. `kind` may be empty when the attribute is missing —
/// the checker owns `E-REWARD-ATTR`. A malformed `amount=` value keeps
/// `amount: None` and preserves the raw attribute inside `attrs` so the
/// checker can anchor its diagnostic at the original value span; a valid
/// literal is lifted into [`RewardAmount`] and removed from `attrs`.
#[derive(Clone, Debug)]
pub struct Reward {
    /// Value of `kind=`; the empty string when the attribute was absent
    /// (checker: `E-REWARD-ATTR`).
    pub kind: String,
    /// Span of `kind`'s value (or the open-tag span when absent).
    pub kind_span: Span,
    /// Value of `target=` when present.
    pub target: Option<String>,
    /// Span of `target`'s value when present (dsl 0.26.0 §2.5 `E-REWARD-TARGET`).
    pub target_span: Option<Span>,
    /// Parsed `amount=` literal; `None` when absent OR when the raw text
    /// failed to parse (the raw attr survives inside `attrs` in that case).
    pub amount: Option<RewardAmount>,
    /// Span of the `amount=` attribute value when the attribute was authored
    /// (whether or not it parsed); `None` when absent.
    pub amount_span: Option<Span>,
    /// Optional `when=` CEL guard (dsl 0.16.0 §2) — evaluated at the grant
    /// instant; joins the canonical [`CelSlot`] walk.
    pub when: Option<CelSlot>,
    /// Raw `on=` attribute value (only `"failed"` is legal, and only on a
    /// quest-level reward per dsl 0.16.0 §2); the checker validates the enum
    /// and the position.
    pub on: Option<String>,
    /// Span of `on`'s value when present.
    pub on_span: Option<Span>,
    /// Residual attrs (post-extraction) for the D-J per-tag attribute
    /// closure check. A malformed `amount=` is preserved here so the checker
    /// can anchor `E-REWARD-ATTR` at the original value span.
    pub attrs: Vec<Attr>,
    /// Span of the whole `<reward … />` element.
    pub span: Span,
    /// Parser recovery flag: `true` for the legal self-closing form; `false`
    /// when a body was written on this leaf (already reported by the parser).
    pub self_closing: bool,
}

/// `amount=` payload (dsl 0.16.0 §2): a scalar integer or an inclusive
/// `N..M` range (`N <= M`, both bounds may be negative). The parser rejects
/// `N > M`; a range is preserved verbatim through lowering and never
/// pre-rolled to a scalar (spec D-C).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RewardAmount {
    Scalar(i64),
    Range(i64, i64),
}

/// `<on event … [when …]> Node* </on>` (dsl 0.2.0 §4). The ECA trigger:
/// `event` names a built-in lifecycle or capability world event (a plain
/// String, NOT CEL); `when` is an optional CEL guard.
#[derive(Clone, Debug)]
pub struct On {
    pub event: String,
    pub event_span: Span,
    pub when: Option<CelSlot>,
    /// dsl 0.24.0 §2: `target=` — the handler fires only when the
    /// same-named occasion is raised for this target. Raw text + value span.
    pub target: Option<(String, Span)>,
    pub attrs: Vec<Attr>,
    pub body: Vec<Node>,
    pub span: Span,
}

/// One `{{…}}` interpolation inside content `Text` (dsl §7.6).
#[derive(Clone, Debug)]
pub struct Interp {
    pub kind: InterpKind,
    /// The referent, trimmed (e.g. `run.coins`, `@fond`, `userName`) — the
    /// interior text without any `:hint` suffix ([`Interp::format`]).
    pub raw: String,
    /// Span of the whole `{{…}}` in the original source.
    pub span: Span,
    /// dsl 0.24.0 §4: the format hint after the referent, trimmed —
    /// `ordinal` in `{{user.deaths:ordinal}}`. The parser keeps any
    /// identifier here; the checker rejects one that is not a known hint
    /// ([`INTERP_FORMATS`]).
    pub format: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterpKind {
    /// `scene.…` / `run.…` / `user.…` / `app.…` state path.
    Path,
    /// `@def` / `@fn(args)`.
    Ref,
    /// Reserved token (`userName`).
    Reserved,
}

/// Classify a `{{…}}` interpolation's interior text (already trimmed) into its
/// [`InterpKind`] (dsl §7.6): a `@…` is a `Ref`, the bare `userName` token is
/// `Reserved`, anything else is a `Path`. The checker owns rejecting a referent
/// that is not actually a bare state path / well-formed `@ref` (§7.6 grammar);
/// this only picks the syntactic bucket. Single source of truth shared by the
/// content-line scan (parser) and the `<choice label>` scan (checker).
pub fn classify_interp(inner: &str) -> InterpKind {
    if inner.starts_with('@') {
        InterpKind::Ref
    } else if inner == "userName" {
        InterpKind::Reserved
    } else {
        InterpKind::Path
    }
}

/// dsl 0.24.0 §4: `{{user.deaths:ordinal}}` renders the number as an
/// English ordinal ([`english_ordinal`]).
pub const INTERP_FORMAT_ORDINAL: &str = "ordinal";

/// dsl 0.25.0 §8: `{{run.day:ordinalWord}}` renders the number as an
/// English ordinal word ([`english_ordinal_word`]).
pub const INTERP_FORMAT_ORDINAL_WORD: &str = "ordinalWord";

/// Every interpolation format hint the language defines; the checker
/// rejects any other. Each formats a number ([`format_number`]).
pub const INTERP_FORMATS: [&str; 2] = [INTERP_FORMAT_ORDINAL, INTERP_FORMAT_ORDINAL_WORD];

/// `n` rendered in format hint `format` ([`INTERP_FORMATS`]) — the one rule
/// the reference runner, `lute trace` and a component's compile-time literal
/// splice all render with. `None` for an unknown hint or a number the hint
/// does not cover: the renderer then shows the number unchanged.
pub fn format_number(format: &str, n: f64) -> Option<String> {
    match format {
        INTERP_FORMAT_ORDINAL => english_ordinal(n),
        INTERP_FORMAT_ORDINAL_WORD => english_ordinal_word(n),
        _ => None,
    }
}

/// Build the [`Interp`] for one `{{…}}` interior (untrimmed `inner`),
/// spanned at `span`: a trailing `:hint` is split off into
/// [`Interp::format`] and the referent before it classified
/// ([`classify_interp`]). The hint is the text after the LAST `:` that sits
/// outside quotes and brackets, when that text is an identifier — so a
/// `@fn(a ? b : c)` argument never splits. Anything else stays in `raw`,
/// where the checker's grammar rule owns it. Shared by the content-line scan
/// (parser) and [`scan_label_interps`].
pub fn interp_from_inner(inner: &str, span: Span) -> Interp {
    let (referent, format) = match top_level_colon(inner) {
        Some(at) if is_hint_ident(inner[at + 1..].trim()) => {
            (inner[..at].trim(), Some(inner[at + 1..].trim().to_string()))
        }
        _ => (inner.trim(), None),
    };
    Interp {
        kind: classify_interp(referent),
        raw: referent.to_string(),
        span,
        format,
    }
}

/// Byte offset of the last `:` in `s` outside quotes and `()`/`[]`/`{}`.
fn top_level_colon(s: &str) -> Option<usize> {
    let (mut depth, mut quote, mut last) = (0usize, None::<u8>, None);
    let mut escaped = false;
    for (i, c) in s.bytes().enumerate() {
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if c == b'\\' {
                escaped = true;
            } else if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            b'"' | b'\'' => quote = Some(c),
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b':' if depth == 0 => last = Some(i),
            _ => {}
        }
    }
    last
}

/// A format-hint name: an ASCII letter, then letters, digits, `_` or `-`.
fn is_hint_ident(s: &str) -> bool {
    let mut it = s.bytes();
    matches!(it.next(), Some(c) if c.is_ascii_alphabetic())
        && it.all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}

/// dsl 0.24.0 §4: `n` as an English ordinal — `1st 2nd 3rd 4th … 11th 12th
/// 13th … 21st 22nd 23rd … 101st 111th 112th`. The suffix follows the last
/// digit, except that a number whose last two digits are `11`–`13` takes
/// `th`; `0` is `0th`. Defined for a non-negative integer only: any other
/// number (fractional, negative, non-finite, or ≥ 10^15 where a float stops
/// being an exact integer) is `None`, and the renderer then shows the number
/// unchanged. The one rule the reference runner, `lute trace` and a
/// component's compile-time literal splice all render with.
pub fn english_ordinal(n: f64) -> Option<String> {
    if !(n.is_finite() && n >= 0.0 && n.fract() == 0.0 && n < 1e15) {
        return None;
    }
    let i = n as u64;
    let suffix = match (i % 10, i % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    Some(format!("{i}{suffix}"))
}

/// dsl 0.25.0 §8: `n` as an English ordinal word — `first` … `twentieth`
/// for 1–20, and [`english_ordinal`]'s digits (`0th`, `21st`, `101st`)
/// otherwise. `None` exactly where [`english_ordinal`] is.
pub fn english_ordinal_word(n: f64) -> Option<String> {
    const WORDS: [&str; 20] = [
        "first",
        "second",
        "third",
        "fourth",
        "fifth",
        "sixth",
        "seventh",
        "eighth",
        "ninth",
        "tenth",
        "eleventh",
        "twelfth",
        "thirteenth",
        "fourteenth",
        "fifteenth",
        "sixteenth",
        "seventeenth",
        "eighteenth",
        "nineteenth",
        "twentieth",
    ];
    let digits = english_ordinal(n)?;
    Some(match n as usize {
        i @ 1..=20 => WORDS[i - 1].to_string(),
        _ => digits,
    })
}

/// Scan a `<choice label>` / `<hub label>` string for `{{…}}` interpolations
/// (dsl §7.6). Labels are String attrs, so — unlike content-line interps — their
/// `{{…}}` are NOT captured into the AST at parse time; this recovers them on
/// demand for the SAME classification model as content interps. The single
/// source of truth shared by the checker's label validation and the compiler's
/// option-label lowering. Classification reuses [`classify_interp`]. Every
/// recovered interp is spanned at the whole slot (`span`) — the label's own byte
/// offset is not retained on the AST — matching the resolver's whole-slot span
/// fallback. An unterminated `{{` in a label is simply not scanned (conservative,
/// never panics); a label never round-trips through the content-line parser, so
/// its `E-INTERP-UNTERMINATED` never applies here.
pub fn scan_label_interps(label: &str, span: Span) -> Vec<Interp> {
    let b = label.as_bytes();
    let mut out = Vec::new();
    let mut j = 0;
    while j + 1 < b.len() {
        if b[j] == b'\\' && label[j + 1..].starts_with("{{") {
            j += 3; // literal `\{{`
            continue;
        }
        if b[j] == b'{' && b[j + 1] == b'{' {
            match label[j + 2..].find("}}") {
                Some(rel) => {
                    out.push(interp_from_inner(&label[j + 2..j + 2 + rel], span));
                    j = j + 2 + rel + 2;
                    continue;
                }
                None => break, // unterminated — nothing more to scan
            }
        }
        j += 1;
    }
    out
}

/// The literal pattern of a `<when is="…">` arm (dsl §7.3.1). Unlike `test`,
/// this is NOT a CEL expression: `raw` is the verbatim (trimmed) attribute
/// value (e.g. `"soft | curt"`), preserved for match-coverage checking and
/// lowering. Stored distinctly from [`CelSlot`] so no CEL parsing is attempted.
#[derive(Clone, Debug)]
pub struct IsPattern {
    /// The `is` attribute's string value, trimmed.
    pub raw: String,
    /// Span of the attribute's value in the original source.
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum Arm {
    When {
        /// Literal `is="…"` pattern (dsl §7.3.1), preserved verbatim; `None` when absent.
        is: Option<IsPattern>,
        test: CelSlot,
        /// Residual (post-`is`/`test`-extraction) attrs — see [`Match::attrs`].
        /// A `@ref`-valued entry here is NOT visited by
        /// [`crate::walk::for_each_cel_slot`] and therefore keeps
        /// `ast: None`/`id: StableId(0)`: every key that can reach this list is
        /// outside `<when>`'s permitted set and is already `E-UNKNOWN-ATTR`
        /// (dsl 0.10.0 §4), so parsing its value would stack a second
        /// diagnostic on an attribute that must not exist — and adding slots to
        /// the pre-order would renumber every `StableId` downstream of them.
        attrs: Vec<Attr>,
        body: Vec<Node>,
        span: Span,
    },
    Otherwise {
        /// Residual attrs — see [`Arm::When`]'s `attrs`. `<otherwise>` extracts
        /// nothing, so every authored key lands here. Its permitted set is
        /// EMPTY (dsl 0.10.0 §4), so any entry is `E-UNKNOWN-ATTR`.
        attrs: Vec<Attr>,
        body: Vec<Node>,
        span: Span,
    },
}

#[derive(Clone, Debug)]
pub struct Timeline {
    pub duration: Option<CelSlot>,
    pub tracks: Vec<Track>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Track {
    pub key: TrackKey,
    pub clips: Vec<Clip>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum TrackKey {
    Subject(String),
    Channel(String),
    Property { subject: String, property: String },
}

/// A clip's authored `at="…"` (dsl §7.4, §11.4): the decimal text **verbatim**
/// plus the attribute value's own span.
///
/// dsl 0.10.0 §10.2: the text is deliberately NOT parsed here. Milliseconds are
/// obtained by shifting the authored decimal three places
/// (`lute_check::parse_time_ms`), never by multiplying a parsed `f64`, and a
/// value finer than a millisecond is `E-TIME-RESOLUTION` at this span. Keeping
/// the string is what lets the checker own both.
#[derive(Clone, Debug)]
pub struct ClipAt {
    pub raw: String,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Clip {
    pub node: ClipNode,
    pub at: Option<ClipAt>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum ClipNode {
    Directive(Directive),
    Set(Set),
}

#[derive(Clone, Debug)]
pub struct Attr {
    pub key: String,
    pub value: AttrValue,
    pub value_span: Span,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum AttrValue {
    Str(String),
    Ref(CelSlot),
    BoolTrue,
} // bare ident => true; @ref becomes a CelSlot

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CelKind {
    Condition,
    AttrValue,
    SetExpr,
    MatchSubject,
}

#[derive(Clone, Debug)]
pub struct CelSlot {
    pub kind: CelKind,
    pub raw: String,
    pub ast: Option<crate::cel_ast::CelAstHandle>, // filled by lute-cel
    pub span: Span,
    pub id: StableId,
    /// The author's text before `@def`/`$` expansion rewrote `raw` (dsl
    /// 0.24.0 T3-12) — `None` until an expansion changed it. Tools show this
    /// by default and the expansion on request.
    pub authored: Option<String>,
}

impl CelSlot {
    pub fn raw(kind: CelKind, raw: String, span: Span) -> Self {
        Self {
            kind,
            raw,
            ast: None,
            span,
            id: StableId(0),
            authored: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn celslot_defaults_to_unparsed() {
        let s = CelSlot::raw(CelKind::Condition, "$ == 'gold'".into(), test_span());
        assert!(s.ast.is_none());
        assert_eq!(s.raw, "$ == 'gold'");
        assert_eq!(s.kind, CelKind::Condition);
    }

    /// dsl 0.24.0 §4: the suffix follows the last digit, except 11–13 in the
    /// last two digits.
    #[test]
    fn english_ordinal_suffixes() {
        let cases = [
            (0.0, "0th"),
            (1.0, "1st"),
            (2.0, "2nd"),
            (3.0, "3rd"),
            (4.0, "4th"),
            (10.0, "10th"),
            (11.0, "11th"),
            (12.0, "12th"),
            (13.0, "13th"),
            (21.0, "21st"),
            (22.0, "22nd"),
            (23.0, "23rd"),
            (100.0, "100th"),
            (101.0, "101st"),
            (111.0, "111th"),
            (112.0, "112th"),
            (113.0, "113th"),
            (1002.0, "1002nd"),
        ];
        for (n, want) in cases {
            assert_eq!(english_ordinal(n).as_deref(), Some(want), "{n}");
        }
    }

    /// A number with no ordinal renders unchanged: the function says so.
    #[test]
    fn english_ordinal_is_undefined_off_the_non_negative_integers() {
        for n in [-1.0, 2.5, f64::NAN, f64::INFINITY, 1e15] {
            assert_eq!(english_ordinal(n), None, "{n}");
        }
    }

    /// dsl 0.25.0 §8: words for 1–20, digit ordinals on either side, and
    /// no ordinal where `:ordinal` has none.
    #[test]
    fn english_ordinal_word_spells_one_to_twenty_then_falls_back_to_digits() {
        for (n, want) in [
            (1.0, "first"),
            (2.0, "second"),
            (3.0, "third"),
            (12.0, "twelfth"),
            (20.0, "twentieth"),
            (21.0, "21st"),
            (0.0, "0th"),
            (112.0, "112th"),
        ] {
            assert_eq!(english_ordinal_word(n).as_deref(), Some(want), "{n}");
        }
        for n in [-1.0, 2.5, f64::NAN] {
            assert_eq!(english_ordinal_word(n), None, "{n}");
        }
    }
    fn test_span() -> lute_core_span::Span {
        lute_core_span::Span {
            byte_start: 0,
            byte_end: 0,
            line: 1,
            column: 1,
            utf16_range: (0, 0),
        }
    }
}
