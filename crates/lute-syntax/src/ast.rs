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
    pub number: Option<i64>,
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

#[derive(Clone, Debug)]
pub struct Match {
    pub subject: CelSlot,
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
#[derive(Clone, Debug)]
pub struct Quest {
    pub id: String,
    pub id_span: Span,
    pub title: Option<String>,
    pub start: Option<CelSlot>,
    pub fail: Option<CelSlot>,
    /// Residual (post-extraction) attrs, mirroring [`Branch`]; normally empty.
    pub attrs: Vec<Attr>,
    pub body: Vec<Node>,
    pub span: Span,
}

/// `<objective id done …> Node* </objective>` or self-closing
/// `<objective … />` (dsl 0.2.0 §6.4). `done` is the required completion
/// predicate; `when` gates visibility; `optional` is a bare boolean flag.
#[derive(Clone, Debug)]
pub struct Objective {
    pub id: String,
    pub id_span: Span,
    pub done: CelSlot,
    pub when: Option<CelSlot>,
    pub title: Option<String>,
    pub optional: bool,
    pub attrs: Vec<Attr>,
    pub body: Vec<Node>,
    pub span: Span,
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
                    out.push(Interp { kind, raw: inner, span });
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
        body: Vec<Node>,
        span: Span,
    },
    Otherwise {
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

#[derive(Clone, Debug)]
pub struct Clip {
    pub node: ClipNode,
    pub at: Option<f64>,
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
