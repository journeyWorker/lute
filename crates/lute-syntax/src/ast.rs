use lute_core_span::{Span, StableId};

#[derive(Clone, Debug)]
pub struct Document { pub meta: Meta, pub title: Option<(String, Span)>, pub shots: Vec<Shot>, pub span: Span }

#[derive(Clone, Debug)]
pub struct Meta { pub raw_yaml: String, pub span: Span } // parsed into typed form in check

#[derive(Clone, Debug)]
pub struct Shot { pub heading: String, pub number: Option<i64>, pub body: Vec<Node>, pub span: Span }

#[derive(Clone, Debug)]
pub enum Node {
    Line(Line),
    Directive(Directive),
    Set(Set),
    Branch(Branch),
    Match(Match),
    Timeline(Timeline),
}

#[derive(Clone, Debug)]
pub struct Line { pub speaker: String, pub attrs: Vec<Attr>, pub text: String, pub text_span: Span, pub span: Span }

#[derive(Clone, Debug)]
pub struct Directive { pub tag: String, pub attrs: Vec<Attr>, pub span: Span }

#[derive(Clone, Debug)]
pub struct Set { pub path: String, pub path_span: Span, pub op: String, pub expr: CelSlot, pub span: Span }

#[derive(Clone, Debug)]
pub struct Branch { pub id: String, pub attrs: Vec<Attr>, pub choices: Vec<Choice>, pub span: Span }

#[derive(Clone, Debug)]
pub struct Choice { pub id: String, pub label: String, pub when: Option<CelSlot>, pub attrs: Vec<Attr>, pub body: Vec<Node>, pub span: Span }

#[derive(Clone, Debug)]
pub struct Match { pub subject: CelSlot, pub arms: Vec<Arm>, pub span: Span }

#[derive(Clone, Debug)]
pub enum Arm { When { test: CelSlot, body: Vec<Node>, span: Span }, Otherwise { body: Vec<Node>, span: Span } }

#[derive(Clone, Debug)]
pub struct Timeline { pub duration: Option<CelSlot>, pub tracks: Vec<Track>, pub span: Span }

#[derive(Clone, Debug)]
pub struct Track { pub key: TrackKey, pub clips: Vec<Clip>, pub span: Span }

#[derive(Clone, Debug)]
pub enum TrackKey { Subject(String), Channel(String), Property { subject: String, property: String } }

#[derive(Clone, Debug)]
pub struct Clip { pub node: ClipNode, pub at: Option<f64>, pub span: Span }

#[derive(Clone, Debug)]
pub enum ClipNode { Directive(Directive), Set(Set) }

#[derive(Clone, Debug)]
pub struct Attr { pub key: String, pub value: AttrValue, pub value_span: Span, pub span: Span }

#[derive(Clone, Debug)]
pub enum AttrValue { Str(String), Ref(CelSlot), BoolTrue } // bare ident => true; @ref becomes a CelSlot

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CelKind { Condition, AttrValue, SetExpr, MatchSubject }

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
        Self { kind, raw, ast: None, span, id: StableId(0) }
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
        lute_core_span::Span { byte_start: 0, byte_end: 0, line: 1, column: 1, utf16_range: (0, 0) }
    }
}
