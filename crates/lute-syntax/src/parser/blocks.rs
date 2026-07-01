//! Recursive block assembly for logic + timeline nodes (dsl §7.3, §7.4).
//!
//! Each `<tag …>` open is matched to its `</tag>` close by name (JSX self-naming
//! close). Missing/mismatched closes → [`E_UNCLOSED_TAG`]. A `<timeline>` body is
//! restricted to `<track>`s and a `<track>` body to staging leaves + `::set`
//! (§7.4); anything else → [`E_TIMELINE_CONTENT`]. `at=` on a track clip is
//! lifted onto [`Clip::at`] (the "`at` outside a timeline" rule is a §7.5 schema
//! check, deferred to the checker).

use super::attrs::{take_cel, take_str};
use super::{
    close_tag_name, open_tag_name, Parser, E_TIMELINE_CONTENT, E_UNCLOSED_TAG,
};
use crate::ast::*;
use lute_core_span::Layer;

/// A parsed `<tag …>` open line.
struct OpenTag {
    attrs: Vec<Attr>,
    /// Original-text offset of the tag's first char.
    start_o: usize,
    /// Original-text offset just past the `>` (or line end).
    end_o: usize,
}

impl Parser<'_> {
    /// Parse the `<tag …>` open line at `cursor` and advance past it.
    fn parse_open_tag(&mut self) -> OpenTag {
        let i = self.cursor;
        let (s, e) = self.lines[i];
        let cstart = s + super::leading_ws(&self.body[s..e]);
        let b = self.body.as_bytes();
        let mut j = cstart + 1; // past '<'
        while j < e && super::is_ident_byte(b[j]) {
            j += 1;
        }
        let (attrs, after) = self.scan_attrs(j, b'>');
        let start_o = self.orig(cstart);
        let end_o = self.orig(after);
        self.cursor += 1;
        OpenTag { attrs, start_o, end_o }
    }

    /// True if `cursor` is a `</name>` close line for `name`.
    fn at_close(&self, name: &str) -> bool {
        self.cursor < self.lines.len()
            && close_tag_name(&self.trimmed(self.cursor)).as_deref() == Some(name)
    }

    /// Consume the matching close if present; else emit `E_UNCLOSED_TAG`.
    /// Returns the original-text end offset of the block.
    fn consume_close(&mut self, name: &str, open: &OpenTag, last_end: usize) -> usize {
        if self.at_close(name) {
            let end = self.orig(self.line_content_end(self.cursor));
            self.cursor += 1;
            end
        } else {
            self.emit_o(
                E_UNCLOSED_TAG,
                format!("<{name}> is never closed"),
                open.start_o,
                open.end_o,
                Layer::Logic,
            );
            last_end
        }
    }

    /// `Branch ::= "<branch" Attrs ">" Choice+ "</branch>"` (§7.3, §11.1).
    pub(super) fn parse_branch(&mut self) -> Branch {
        let open = self.parse_open_tag();
        let mut attrs = open.attrs.clone();
        let id = take_str(&mut attrs, "id").unwrap_or_default();
        let mut choices = Vec::new();
        let mut last_end = open.end_o;
        loop {
            self.skip_blanks();
            if self.cursor >= self.lines.len() || self.stop_at_heading() || self.at_close("branch") {
                break;
            }
            let trimmed = self.trimmed(self.cursor);
            if open_tag_name(&trimmed).as_deref() == Some("choice") {
                let c = self.parse_choice();
                last_end = c.span.byte_end;
                choices.push(c);
            } else {
                // Stray content inside <branch>: skip (checker validates structure).
                self.skip_stray();
            }
        }
        let end_o = self.consume_close("branch", &open, last_end);
        Branch { id, attrs, choices, span: self.span_o(open.start_o, end_o) }
    }

    /// `Choice ::= "<choice" Attrs ">" Node* "</choice>"` (§7.3, §11.1).
    fn parse_choice(&mut self) -> Choice {
        let open = self.parse_open_tag();
        let mut attrs = open.attrs.clone();
        let id = take_str(&mut attrs, "id").unwrap_or_default();
        let label = take_str(&mut attrs, "label").unwrap_or_default();
        let when = take_cel(&mut attrs, "when", CelKind::Condition);
        let (body, end_o) = self.parse_block_body("choice", &open);
        Choice { id, label, when, attrs, body, span: self.span_o(open.start_o, end_o) }
    }

    /// `Match ::= "<match" Attrs ">" When+ Otherwise? "</match>"` (§7.3, §11.2).
    pub(super) fn parse_match(&mut self) -> Match {
        let open = self.parse_open_tag();
        let mut attrs = open.attrs.clone();
        let subject = take_cel(&mut attrs, "on", CelKind::MatchSubject)
            .unwrap_or_else(|| CelSlot::raw(CelKind::MatchSubject, String::new(), self.span_o(open.start_o, open.end_o)));
        let mut arms = Vec::new();
        let mut last_end = open.end_o;
        loop {
            self.skip_blanks();
            if self.cursor >= self.lines.len() || self.stop_at_heading() || self.at_close("match") {
                break;
            }
            let trimmed = self.trimmed(self.cursor);
            match open_tag_name(&trimmed).as_deref() {
                Some("when") => {
                    let a = self.parse_when();
                    last_end = arm_end(&a);
                    arms.push(a);
                }
                Some("otherwise") => {
                    let a = self.parse_otherwise();
                    last_end = arm_end(&a);
                    arms.push(a);
                }
                _ => self.skip_stray(),
            }
        }
        let end_o = self.consume_close("match", &open, last_end);
        Match { subject, arms, span: self.span_o(open.start_o, end_o) }
    }

    /// `When ::= "<when" Attrs ">" Node* "</when>"` (§7.3, §11.2).
    fn parse_when(&mut self) -> Arm {
        let open = self.parse_open_tag();
        let mut attrs = open.attrs.clone();
        let test = take_cel(&mut attrs, "test", CelKind::Condition)
            .unwrap_or_else(|| CelSlot::raw(CelKind::Condition, String::new(), self.span_o(open.start_o, open.end_o)));
        let (body, end_o) = self.parse_block_body("when", &open);
        Arm::When { test, body, span: self.span_o(open.start_o, end_o) }
    }

    /// `Otherwise ::= "<otherwise>" Node* "</otherwise>"` (§7.3, §11.2).
    fn parse_otherwise(&mut self) -> Arm {
        let open = self.parse_open_tag();
        let (body, end_o) = self.parse_block_body("otherwise", &open);
        Arm::Otherwise { body, span: self.span_o(open.start_o, end_o) }
    }

    /// `Timeline ::= "<timeline" Attrs? ">" Track+ "</timeline>"` (§7.4).
    pub(super) fn parse_timeline(&mut self) -> Timeline {
        let open = self.parse_open_tag();
        let mut attrs = open.attrs.clone();
        let duration = take_cel(&mut attrs, "duration", CelKind::AttrValue);
        let mut tracks = Vec::new();
        let mut last_end = open.end_o;
        loop {
            self.skip_blanks();
            if self.cursor >= self.lines.len() || self.stop_at_heading() || self.at_close("timeline") {
                break;
            }
            let trimmed = self.trimmed(self.cursor);
            if open_tag_name(&trimmed).as_deref() == Some("track") {
                let t = self.parse_track();
                last_end = t.span.byte_end;
                tracks.push(t);
            } else {
                // §7.4: a <timeline> body admits only <track>s.
                self.emit_line(
                    E_TIMELINE_CONTENT,
                    "a <timeline> body may contain only <track>s",
                    self.cursor,
                    Layer::Logic,
                );
                self.skip_stray();
            }
        }
        let end_o = self.consume_close("timeline", &open, last_end);
        Timeline { duration, tracks, span: self.span_o(open.start_o, end_o) }
    }

    /// `Track ::= "<track" Attrs ">" Clip+ "</track>"` (§7.4). Body restricted to
    /// staging leaves (`::name`) + `::set`; anything else → `E_TIMELINE_CONTENT`.
    fn parse_track(&mut self) -> Track {
        let open = self.parse_open_tag();
        let mut attrs = open.attrs.clone();
        let subject = take_str(&mut attrs, "subject");
        let channel = take_str(&mut attrs, "channel");
        let property = take_str(&mut attrs, "property");
        let key = if let (Some(subject), Some(property)) = (subject.clone(), property) {
            TrackKey::Property { subject, property }
        } else if let Some(subject) = subject {
            TrackKey::Subject(subject)
        } else if let Some(channel) = channel {
            TrackKey::Channel(channel)
        } else {
            TrackKey::Subject(String::new()) // missing key: checker validates.
        };
        let mut clips = Vec::new();
        let mut last_end = open.end_o;
        loop {
            self.skip_blanks();
            if self.cursor >= self.lines.len() || self.stop_at_heading() || self.at_close("track") {
                break;
            }
            let trimmed = self.trimmed(self.cursor);
            if trimmed.starts_with("::set{") {
                if let Node::Set(set) = self.parse_set() {
                    last_end = set.span.byte_end;
                    clips.push(Clip { at: None, span: set.span, node: ClipNode::Set(set) });
                }
            } else if trimmed.starts_with("::") {
                if let Node::Directive(mut d) = self.parse_directive() {
                    let at = take_at(&mut d.attrs);
                    last_end = d.span.byte_end;
                    clips.push(Clip { at, span: d.span, node: ClipNode::Directive(d) });
                }
            } else {
                // §7.4: no :line / logic block inside a <track>.
                self.emit_line(
                    E_TIMELINE_CONTENT,
                    "a <track> body may contain only staging directives and ::set",
                    self.cursor,
                    Layer::Logic,
                );
                self.skip_stray();
            }
        }
        let end_o = self.consume_close("track", &open, last_end);
        Track { key, clips, span: self.span_o(open.start_o, end_o) }
    }

    /// Parse the generic body of a `<tag>…</tag>` (choice/when/otherwise): full
    /// nodes until the matching close, a `## ` heading, or EOF. Returns
    /// `(body, end_o)` where `end_o` is the block's original-text end offset.
    fn parse_block_body(&mut self, name: &str, open: &OpenTag) -> (Vec<Node>, usize) {
        let mut body = Vec::new();
        let mut last_end = open.end_o;
        loop {
            self.skip_blanks();
            if self.cursor >= self.lines.len() || self.stop_at_heading() || self.at_close(name) {
                break;
            }
            let trimmed = self.trimmed(self.cursor);
            if trimmed.starts_with("</") {
                // A close for some other tag: our tag is unclosed — stop here.
                break;
            }
            if let Some(node) = self.next_node() {
                last_end = super::node_end(&node);
                body.push(node);
            }
        }
        let end_o = self.consume_close(name, open, last_end);
        (body, end_o)
    }

    /// True if `cursor` sits on a shot heading (`## `) — a hard block terminator.
    fn stop_at_heading(&self) -> bool {
        self.cursor < self.lines.len() && self.trimmed(self.cursor).starts_with("## ")
    }

    /// Skip one stray line inside a block (structure the checker will flag).
    fn skip_stray(&mut self) {
        self.cursor += 1;
    }
}

/// Original-text end offset of an [`Arm`].
fn arm_end(a: &Arm) -> usize {
    match a {
        Arm::When { span, .. } => span.byte_end,
        Arm::Otherwise { span, .. } => span.byte_end,
    }
}

/// Take (remove) the `at="…"` clip-position attr as an `f64` (§7.4, §11.4).
fn take_at(attrs: &mut Vec<Attr>) -> Option<f64> {
    let pos = attrs.iter().position(|a| a.key == "at")?;
    let val = match &attrs[pos].value {
        AttrValue::Str(s) => s.parse::<f64>().ok(),
        _ => None,
    };
    attrs.remove(pos);
    val
}
