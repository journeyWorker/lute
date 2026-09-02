use lute_core_span::{Span, StableId};

#[derive(Clone, Debug)]
pub struct Document {
    pub meta: Meta,
    pub title: Option<(String, Span)>,
    pub shots: Vec<Shot>,
    pub quests: Vec<Quest>,
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
    /// `::next{when="COND"}` (dsl 0.12.0 §…) — a forward-jump guard,
    /// extracted into a typed CEL slot the SAME way `Line.when`/`Choice.when`
    /// are (`take_cel`, parser.rs), so it rides the same CEL walk /
    /// `StableId` / `check_cel_slot` validation path a content-line guard
    /// does. `None` for every OTHER directive tag — the parser only ever
    /// extracts `when` when `tag == "next"`; a `when=` attr elsewhere stays
    /// an ordinary residual `attrs` entry (`E-UNKNOWN-ATTR`, unchanged).
    pub when: Option<CelSlot>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Set {
    pub path: String,
    pub path_span: Span,
    pub op: String,
    pub expr: CelSlot,
    pub span: Span,
}

/// `::assert{ rel(a, b) }` (dsl 0.3.0 §5) — a pure leaf; args are compile-time-ground
/// (no `{{…}}`, no CEL). `pattern.relation.is_empty()` is the parse-failed sentinel (D13).
#[derive(Clone, Debug, PartialEq)]
pub struct Assert {
    pub pattern: crate::datalog::FactPattern,
    /// Byte offset of the payload interior start; pattern spans are relative to it.
    pub pattern_base: usize,
    pub raw: String,
    pub span: Span,
}

/// `::retract{ rel(a, _) }` (dsl 0.3.0 §5) — mirrors [`Assert`]; wildcard legality
/// is checked downstream (Task 10), not here.
#[derive(Clone, Debug, PartialEq)]
pub struct Retract {
    pub pattern: crate::datalog::FactPattern,
    pub pattern_base: usize,
    pub raw: String,
    pub span: Span,
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
    /// Residual (post-extraction) attrs, mirroring [`Branch`]; normally empty.
    pub attrs: Vec<Attr>,
    pub body: Vec<Node>,
    /// Self-closing `<reward/>` children in declaration order (dsl 0.16.0 §2).
    pub rewards: Vec<Reward>,
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
    pub attrs: Vec<Attr>,
    pub body: Vec<Node>,
    pub span: Span,
}

/// One `{{…}}` interpolation inside content `Text` (dsl §7.6).
#[derive(Clone, Debug)]
pub struct Interp {
    pub kind: InterpKind,
    /// Interior text, trimmed (e.g. `run.coins`, `@fond`, `userName`).
    pub raw: String,
    /// Span of the whole `{{…}}` in the original source.
    pub span: Span,
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
                    let inner = label[j + 2..j + 2 + rel].trim().to_string();
                    let kind = classify_interp(&inner);
                    out.push(Interp {
                        kind,
                        raw: inner,
                        span,
                    });
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
}

impl CelSlot {
    pub fn raw(kind: CelKind, raw: String, span: Span) -> Self {
        Self {
            kind,
            raw,
            ast: None,
            span,
            id: StableId(0),
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
