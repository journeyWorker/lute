//! The label grammar of quoted attribute values (0.52.0 §3.2).
//!
//! These values take `{{…}}` interpolation with modifiers, read by
//! [`lute_syntax::line_text::interp_sites`] exactly as line text is: a
//! `<choice text>` of a `<branch>` or `<hub>`, a `<branch prompt>` and a
//! `<hub prompt>`, an `<objective title>` and a plugin directive attribute
//! declared `type: text` (0.51.0 §5.2) — [`takes_label_grammar`].
//!
//! In such a value `\{{` is a literal `{{`, the line-text spelling (lute
//! 0.47.1 §3). The lexer cannot tell a label from any other quoted value (a
//! `text` attribute is known only with the snapshot), so it keeps `\{{`
//! verbatim everywhere and [`check_escaped_openers`] refuses it, as
//! `E-STRING-ESCAPE`, in every value that does not take the label grammar.

use std::fmt;

use lute_core_span::{Diagnostic, Layer, Severity, Span, TextIndex};
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_syntax::ast::{Arm, ClipNode, Document, Node};
use lute_syntax::line_text::{interp_sites, InterpSite};

/// The node head a quoted attribute value is written on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AttrOwner<'a> {
    Choice,
    Branch,
    Hub,
    HubReturn,
    Objective,
    Match,
    When,
    Otherwise,
    Timeline,
    Track,
    On,
    Quest,
    Reward,
    Entry,
    Beat,
    /// A content line, by its speaker as written.
    Line(&'a str),
    /// A directive (`::use` included), by its tag.
    Directive(&'a str),
}

impl fmt::Display for AttrOwner<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let tag = match self {
            Self::Line(speaker) => return write!(f, "@{speaker}"),
            Self::Directive(tag) => return write!(f, "::{tag}"),
            Self::Choice => "choice",
            Self::Branch => "branch",
            Self::Hub => "hub",
            Self::HubReturn => "return",
            Self::Objective => "objective",
            Self::Match => "match",
            Self::When => "when",
            Self::Otherwise => "otherwise",
            Self::Timeline => "timeline",
            Self::Track => "track",
            Self::On => "on",
            Self::Quest => "quest",
            Self::Reward => "reward",
            Self::Entry => "entry",
            Self::Beat => "beat",
        };
        write!(f, "<{tag}>")
    }
}

/// Whether the value of `attr` on `owner` takes the label grammar; for a
/// directive, by the `text` attributes `snapshot` declares for its tag.
pub(crate) fn takes_label_grammar(
    owner: AttrOwner<'_>,
    attr: &str,
    snapshot: &CapabilitySnapshot,
) -> bool {
    match owner {
        AttrOwner::Choice => attr == "text",
        AttrOwner::Branch | AttrOwner::Hub => attr == "prompt",
        AttrOwner::Objective => attr == "title",
        AttrOwner::Directive(tag) => snapshot
            .directive(tag)
            .is_some_and(|d| d.text_attrs().any(|t| t.name == attr)),
        _ => false,
    }
}

/// `E-STRING-ESCAPE` for every quoted value of `doc` (`source` is its text,
/// `idx` indexes it) that holds `\{{` — an odd backslash run before `{{` —
/// and does not take the label grammar. Each head's attributes are read as
/// authored ([`lute_syntax::authored_attrs`], escapes undecoded), so a
/// value the parser lifts into a typed field or a CEL slot (`when`, `test`)
/// is judged too. Run on the document as parsed, before any pass splices
/// nodes.
pub(crate) fn check_escaped_openers(
    doc: &Document,
    source: &str,
    idx: &TextIndex<'_>,
    snapshot: &CapabilitySnapshot,
) -> Vec<Diagnostic> {
    let mut walk = Walk {
        source,
        snapshot,
        idx,
        diags: Vec::new(),
    };
    for section in &doc.sections {
        walk.body(&section.body);
    }
    for quest in &doc.quests {
        walk.head(AttrOwner::Quest, quest.span);
        walk.body(&quest.body);
        for reward in &quest.rewards {
            walk.head(AttrOwner::Reward, reward.span);
        }
    }
    for entry in &doc.entries {
        walk.head(AttrOwner::Entry, entry.span);
        walk.body(&entry.body);
    }
    for beat in &doc.beats {
        walk.head(AttrOwner::Beat, beat.span);
        walk.body(&beat.body);
    }
    walk.diags
}

struct Walk<'a> {
    source: &'a str,
    snapshot: &'a CapabilitySnapshot,
    idx: &'a TextIndex<'a>,
    diags: Vec<Diagnostic>,
}

impl Walk<'_> {
    fn body(&mut self, nodes: &[Node]) {
        for node in nodes {
            self.node(node);
        }
    }

    fn node(&mut self, node: &Node) {
        match node {
            Node::Line(l) => self.head(AttrOwner::Line(&l.speaker), l.span),
            Node::Directive(d) => self.head(AttrOwner::Directive(&d.tag), d.span),
            Node::Branch(b) => {
                self.head(AttrOwner::Branch, b.span);
                for c in &b.choices {
                    self.head(AttrOwner::Choice, c.span);
                    self.body(&c.body);
                }
            }
            Node::Hub(h) => {
                self.head(AttrOwner::Hub, h.span);
                for c in &h.choices {
                    self.head(AttrOwner::Choice, c.span);
                    self.body(&c.body);
                }
                if let Some(r) = &h.on_return {
                    self.head(AttrOwner::HubReturn, r.span);
                    self.body(&r.body);
                }
            }
            Node::Match(m) => {
                self.head(AttrOwner::Match, m.span);
                for arm in &m.arms {
                    match arm {
                        Arm::When { span, body, .. } => {
                            self.head(AttrOwner::When, *span);
                            self.body(body);
                        }
                        Arm::Otherwise { span, body, .. } => {
                            self.head(AttrOwner::Otherwise, *span);
                            self.body(body);
                        }
                    }
                }
            }
            Node::Timeline(t) => {
                self.head(AttrOwner::Timeline, t.span);
                for track in &t.tracks {
                    self.head(AttrOwner::Track, track.span);
                    for clip in &track.clips {
                        if let ClipNode::Directive(d) = &clip.node {
                            self.head(AttrOwner::Directive(&d.tag), d.span);
                        }
                    }
                }
            }
            Node::Objective(o) => {
                self.head(AttrOwner::Objective, o.span);
                self.body(&o.body);
                for reward in &o.rewards {
                    self.head(AttrOwner::Reward, reward.span);
                }
            }
            Node::On(o) => {
                self.head(AttrOwner::On, o.span);
                self.body(&o.body);
            }
            // `::set`, `::assert` and `::retract` write a statement, not an
            // attribute list.
            Node::Set(_) | Node::Assert(_) | Node::Retract(_) => {}
        }
    }

    /// Judge every quoted value of the head that starts at `head`.
    fn head(&mut self, owner: AttrOwner<'_>, head: Span) {
        for attr in lute_syntax::authored_attrs(self.source, head) {
            let quoted = attr
                .value_span
                .start
                .checked_sub(1)
                .and_then(|i| self.source.as_bytes().get(i))
                == Some(&b'"');
            let escaped =
                interp_sites(attr.raw_value).any(|site| matches!(site, InterpSite::Escaped { .. }));
            if !quoted || !escaped || takes_label_grammar(owner, attr.key, self.snapshot) {
                continue;
            }
            self.diags.push(Diagnostic {
                code: lute_syntax::parser::E_STRING_ESCAPE.to_string(),
                severity: Severity::Error,
                message: escaped_opener_message(owner, attr.key),
                evidence: None,
                span: Span::from_bytes(self.idx, attr.value_span.start, attr.value_span.end),
                layer: Layer::Content,
                fixits: Vec::new(),
                provenance: None,
                covered: Vec::new(),
                related: Vec::new(),
            });
        }
    }
}

/// The `E-STRING-ESCAPE` message for `\{{` in `attr` of `owner`, a value
/// outside the label grammar. A `::use` argument names its own spelling.
fn escaped_opener_message(owner: AttrOwner<'_>, attr: &str) -> String {
    let message = format!(
        "`\\{{{{` (a literal `{{{{`) is defined only in line text and in the values that take \
         the label grammar — `<choice text>`, `prompt` and a plugin `text` attribute; `{attr}` \
         of `{owner}` is not one"
    );
    if owner == AttrOwner::Directive("use") {
        format!("{message}; a `::use` argument passes a literal `{{{{` as `\\\\{{{{` (lute 0.47.1)")
    } else {
        message
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owners_display_as_written() {
        assert_eq!(AttrOwner::Choice.to_string(), "<choice>");
        assert_eq!(AttrOwner::HubReturn.to_string(), "<return>");
        assert_eq!(AttrOwner::Line("mira").to_string(), "@mira");
        assert_eq!(AttrOwner::Directive("banner").to_string(), "::banner");
    }

    #[test]
    fn the_label_grammar_owners() {
        let snapshot = CapabilitySnapshot::default();
        for (owner, attr) in [
            (AttrOwner::Choice, "text"),
            (AttrOwner::Branch, "prompt"),
            (AttrOwner::Hub, "prompt"),
            (AttrOwner::Objective, "title"),
        ] {
            assert!(
                takes_label_grammar(owner, attr, &snapshot),
                "{owner} {attr}"
            );
        }
        for (owner, attr) in [
            (AttrOwner::Choice, "when"),
            (AttrOwner::Branch, "id"),
            (AttrOwner::Quest, "title"),
            (AttrOwner::When, "test"),
            (AttrOwner::Line("mira"), "code"),
            // No snapshot declaration: not a `text` attribute.
            (AttrOwner::Directive("banner"), "title"),
            (AttrOwner::Directive("use"), "p"),
        ] {
            assert!(
                !takes_label_grammar(owner, attr, &snapshot),
                "{owner} {attr}"
            );
        }
    }

    #[test]
    fn messages() {
        assert_eq!(
            escaped_opener_message(AttrOwner::Directive("banner"), "code"),
            "`\\{{` (a literal `{{`) is defined only in line text and in the values that take \
             the label grammar — `<choice text>`, `prompt` and a plugin `text` attribute; `code` \
             of `::banner` is not one"
        );
        assert!(
            escaped_opener_message(AttrOwner::Directive("use"), "p").ends_with(
                "`p` of `::use` is not one; a `::use` argument passes a literal `{{` as `\\\\{{` \
             (lute 0.47.1)"
            )
        );
    }
}
