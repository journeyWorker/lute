//! Recursive block assembly for logic + timeline nodes (dsl §7.3, §7.4).
//!
//! Each `<tag …>` open is matched to its `</tag>` close by name (JSX self-naming
//! close). Missing/mismatched closes → [`E_UNCLOSED_TAG`]. A `<timeline>` body is
//! restricted to `<track>`s and a `<track>` body to staging leaves + `::set`
//! (§7.4); anything else → [`E_TIMELINE_CONTENT`]. `at=` on a track clip is
//! lifted onto [`Clip::at`] (the "`at` outside a timeline" rule is a §7.5 schema
//! check, deferred to the checker).

use super::attrs::{take_bool, take_cel, take_str, take_str_spanned};
use super::{
    close_tag_name, open_tag_name, Parser, E_LOGIC_CONTENT, E_TAG_INLINE_BODY, E_TAG_NOT_ONE_LINE,
    E_TIMELINE_CONTENT, E_UNCLOSED_TAG,
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
    /// True for a self-closing `<tag …/>` (dsl 0.2.0 §6.4).
    self_closing: bool,
    /// True when the element's body AND its matching `</tag>` close were both
    /// written on the opener's own physical line — the unsupported single-line
    /// form (dsl §2.3), already reported as [`E_TAG_INLINE_BODY`]. The element
    /// IS closed, just in the wrong form, so the block parsers claim no
    /// following lines for it and [`Parser::consume_close`] reports no
    /// [`E_UNCLOSED_TAG`]: ONE diagnostic names the mistake.
    inline_closed: bool,
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
        // dsl 0.5.0 §2.1/§2.3 `E-TAG-NOT-ONE-LINE`: `scan_attrs` hard-stops on
        // `\n` (it never reads past a physical line, so a wrapped attribute
        // never gets misparsed as this line's content), so the terminator was
        // reached on THIS line iff the last byte it consumed is the
        // terminator itself AND that offset did not cross past this line's end
        // (RC2: the `after <= e` guard) — true for both a plain `>` close and
        // a self-closing `/>` (scan_attrs skips the lone `/` as an unparseable
        // token, then finds `>` right after). The `after <= e` half matters
        // even with the scanners stopping at `\n` (belt-and-suspenders): it
        // guards against a terminator byte that happens to sit right past a
        // multi-byte line boundary being misread as "on this line". When
        // either check fails, the opener's `>`/`/>` was not reached on its
        // own physical line — name it instead of leaving a misleading
        // `E-UNCLOSED-TAG`/`E-UNCLASSIFIED` to fire from wherever the parser
        // resyncs. Do NOT attempt to consume the wrap — the one-physical-line
        // model (§2.3) is retained, not relaxed.
        let one_line = after <= e && self.body.as_bytes().get(after.wrapping_sub(1)) == Some(&b'>');
        if !one_line {
            self.emit_o(
                E_TAG_NOT_ONE_LINE,
                "a tag and all its attributes must be on one physical line; wrapping is not \
                 supported (dsl §2.3)"
                    .to_string(),
                start_o,
                self.orig(e),
                Layer::Logic,
            );
        }
        // dsl 0.2.0 §6.4 self-closing `<tag/>`: the `>` was preceded by `/`. The
        // attr scanner tolerates the lone `/` (skips it as an unparseable token),
        // so detect it from the raw byte just before the consumed terminator.
        let self_closing = after >= 2 && self.body.as_bytes()[after - 2] == b'/';
        // dsl §2.3 `E-TAG-INLINE-BODY`: the opener is impeccable (it closed on
        // its own line) but text FOLLOWS its `>` — the author wrote the body,
        // and often the close too, on the opener's line. `parse_open_tag`
        // consumes whole lines, so that text is dropped; left unnamed it
        // resurfaces as an unclosed-tag claim against a close that is right
        // there, an "unexpected block here" against the next well-formed
        // sibling, and a fabricated `E-NONEXHAUSTIVE` against a `<match>`
        // whose arms simply never parsed. A self-closing `<tag …/>` is
        // excluded: it HAS no body, so trailing text there is a different
        // mistake and would need a different message — one code, one meaning.
        let rest = if one_line { &self.body[after..e] } else { "" };
        let inline_body = !self_closing && !rest.trim().is_empty();
        // The close on the SAME line means the element is complete there: the
        // recovery below claims no following lines for it, which is what keeps
        // the surrounding node stream (a `<match>`'s arms, say) intact.
        let inline_closed = inline_body && holds_inline_close(rest, &self.body[cstart + 1..j]);
        if inline_body {
            let name = self.body[cstart + 1..j].to_string();
            self.emit_o(
                E_TAG_INLINE_BODY,
                format!(
                    "<{name}>'s body must be on its own line: the opener, each body line, and \
                     `</{name}>` each need a physical line of their own — a single-line \
                     `<{name}>…</{name}>` with an inline body is not supported (dsl §2.3)"
                ),
                start_o,
                self.orig(e),
                Layer::Logic,
            );
        }
        self.cursor += 1;
        OpenTag {
            attrs,
            start_o,
            end_o,
            self_closing,
            inline_closed,
        }
    }

    /// True if `cursor` is a `</name>` close line for `name`.
    fn at_close(&self, name: &str) -> bool {
        self.cursor < self.lines.len()
            && close_tag_name(&self.trimmed(self.cursor)).as_deref() == Some(name)
    }

    /// Consume the matching close if present; else emit `E_UNCLOSED_TAG`.
    /// Returns the original-text end offset of the block.
    fn consume_close(&mut self, name: &str, open: &OpenTag, last_end: usize) -> usize {
        if open.inline_closed {
            // dsl §2.3: the close WAS written — on the opener's own line, in
            // the unsupported single-line form already reported as
            // `E_TAG_INLINE_BODY`. Claiming the element "is never closed" on
            // top of that is precisely the misdirection that code exists to
            // remove; the author has one mistake, not two.
            return open.end_o;
        }
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
            if self.block_body_done(&open) || self.at_close("branch") {
                break;
            }
            let trimmed = self.trimmed(self.cursor);
            if open_tag_name(&trimmed).as_deref() == Some("choice") {
                let c = self.parse_choice();
                last_end = c.span.byte_end;
                choices.push(c);
            } else {
                // §7.3: a <branch> body admits only <choice> children. Report the
                // stray line (mirroring <track>/E-TIMELINE-CONTENT) before skipping
                // it, so the checker/editor sees it rather than a silent drop.
                self.emit_line(
                    E_LOGIC_CONTENT,
                    "a <branch> body may contain only <choice> children (dsl §7.3)",
                    self.cursor,
                    Layer::Logic,
                );
                self.skip_stray();
            }
        }
        let end_o = self.consume_close("branch", &open, last_end);
        Branch {
            id,
            attrs,
            choices,
            span: self.span_o(open.start_o, end_o),
        }
    }

    /// `On ::= "<on" Attrs ">" Node* "</on>"` (dsl 0.2.0 §4.1). The ECA trigger:
    /// `event` is a plain String (NOT CEL); `when` is an optional CEL guard.
    pub(super) fn parse_on(&mut self) -> On {
        let open = self.parse_open_tag();
        let mut attrs = open.attrs.clone();
        let (event, event_span) = take_str_spanned(&mut attrs, "event")
            .unwrap_or_else(|| (String::new(), self.span_o(open.start_o, open.end_o)));
        let when = take_cel(&mut attrs, "when", CelKind::Condition);
        let (body, end_o) = self.parse_block_body("on", &open);
        On {
            event,
            event_span,
            when,
            attrs,
            body,
            span: self.span_o(open.start_o, end_o),
        }
    }

    /// `QuestDecl ::= "<quest" Attrs ">" QuestBody "</quest>"` (dsl 0.2.0 §6.3).
    /// TOP-LEVEL ONLY — the caller (`parse_document_inner`) invokes this
    /// directly; `<quest>` is never dispatched through [`Parser::next_node`].
    pub(super) fn parse_quest(&mut self) -> Quest {
        let open = self.parse_open_tag();
        let mut attrs = open.attrs.clone();
        let (id, id_span) = take_str_spanned(&mut attrs, "id")
            .unwrap_or_else(|| (String::new(), self.span_o(open.start_o, open.end_o)));
        let title = take_str(&mut attrs, "title");
        let start = take_cel(&mut attrs, "start", CelKind::Condition);
        let fail = take_cel(&mut attrs, "fail", CelKind::Condition);
        // `after` (connectivity layer, T2): kept as raw text + span, NEVER
        // routed through `take_cel` — it is validated under the restricted
        // `prereq::parse_prereq` grammar (checker layer), not general CEL.
        let (after, after_span) = take_str_spanned(&mut attrs, "after")
            .map(|(s, sp)| (Some(s), sp))
            .unwrap_or_else(|| (None, self.span_o(open.start_o, open.end_o)));
        let (body, rewards, end_o) = self.parse_owner_body("quest", &open);
        Quest {
            id,
            id_span,
            title,
            start,
            fail,
            after,
            after_span,
            attrs,
            body,
            rewards,
            span: self.span_o(open.start_o, end_o),
        }
    }

    /// `Objective ::= "<objective" Attrs ">" Node* "</objective>" | "<objective"
    /// Attrs "/>"` (dsl 0.2.0 §6.4). One of `done`/`quest=` is required but a
    /// MISSING `done` still yields a valid AST (empty CEL slot) —
    /// `E-OBJECTIVE-MISSING-DONE` / `E-OBJECTIVE-QUEST-DONE` are Plan C checker
    /// diagnostics, NOT parse errors. Mirrors `parse_when`/`parse_match`'s
    /// empty-slot idiom exactly. `quest=` is a plain string reference (a quest
    /// id, never CEL), mirroring `<quest after=>`'s `take_str_spanned`
    /// treatment.
    pub(super) fn parse_objective(&mut self) -> Objective {
        let open = self.parse_open_tag();
        let mut attrs = open.attrs.clone();
        let (id, id_span) = take_str_spanned(&mut attrs, "id")
            .unwrap_or_else(|| (String::new(), self.span_o(open.start_o, open.end_o)));
        let done = take_cel(&mut attrs, "done", CelKind::Condition).unwrap_or_else(|| {
            CelSlot::raw(
                CelKind::Condition,
                String::new(),
                self.span_o(open.start_o, open.end_o),
            )
        });
        let (quest, quest_span) = match take_str_spanned(&mut attrs, "quest") {
            Some((q, s)) => (Some(q), s),
            None => (None, self.span_o(open.start_o, open.end_o)),
        };
        let when = take_cel(&mut attrs, "when", CelKind::Condition);
        let title = take_str(&mut attrs, "title");
        let optional = take_bool(&mut attrs, "optional");
        let (body, rewards, end_o) = if open.self_closing {
            (Vec::new(), Vec::new(), open.end_o)
        } else {
            self.parse_owner_body("objective", &open)
        };
        Objective {
            id,
            id_span,
            done,
            quest,
            quest_span,
            when,
            title,
            optional,
            attrs,
            body,
            rewards,
            span: self.span_o(open.start_o, end_o),
        }
    }

    /// `Hub ::= "<hub" Attrs ">" Choice+ "</hub>"` (§7.3.2). Mirrors
    /// [`Parser::parse_branch`]: a `<hub>` body admits only `<choice>` children,
    /// strays → [`E_LOGIC_CONTENT`], same [`Parser::consume_close`]. The `once` /
    /// `exit` flags ride along as bare attrs on each [`Choice`] (Plan B extracts).
    /// [`Hub`] carries no `id` field, so `id=` stays in `attrs`.
    pub(super) fn parse_hub(&mut self) -> Hub {
        let open = self.parse_open_tag();
        let attrs = open.attrs.clone();
        let mut choices = Vec::new();
        let mut last_end = open.end_o;
        loop {
            self.skip_blanks();
            if self.block_body_done(&open) || self.at_close("hub") {
                break;
            }
            let trimmed = self.trimmed(self.cursor);
            if open_tag_name(&trimmed).as_deref() == Some("choice") {
                let c = self.parse_choice();
                last_end = c.span.byte_end;
                choices.push(c);
            } else {
                // §7.3.2: a <hub> body admits only <choice> children. Report the
                // stray line (mirroring <branch>/E-LOGIC-CONTENT) before skipping
                // it, so the checker/editor sees it rather than a silent drop.
                self.emit_line(
                    E_LOGIC_CONTENT,
                    "a <hub> body may contain only <choice> children (dsl §7.3.2)",
                    self.cursor,
                    Layer::Logic,
                );
                self.skip_stray();
            }
        }
        let end_o = self.consume_close("hub", &open, last_end);
        Hub {
            attrs,
            choices,
            span: self.span_o(open.start_o, end_o),
        }
    }

    /// `Choice ::= "<choice" Attrs ">" Node* "</choice>"` (§7.3, §11.1).
    fn parse_choice(&mut self) -> Choice {
        let open = self.parse_open_tag();
        let mut attrs = open.attrs.clone();
        let id = take_str(&mut attrs, "id").unwrap_or_default();
        let label = take_str(&mut attrs, "label").unwrap_or_default();
        let when = take_cel(&mut attrs, "when", CelKind::Condition);
        let (body, end_o) = self.parse_block_body("choice", &open);
        Choice {
            id,
            label,
            when,
            attrs,
            body,
            span: self.span_o(open.start_o, end_o),
        }
    }

    /// `Match ::= "<match" Attrs ">" When+ Otherwise? "</match>"` (§7.3, §11.2).
    pub(super) fn parse_match(&mut self) -> Match {
        let open = self.parse_open_tag();
        let mut attrs = open.attrs.clone();
        let subject = take_cel(&mut attrs, "on", CelKind::MatchSubject).unwrap_or_else(|| {
            CelSlot::raw(
                CelKind::MatchSubject,
                String::new(),
                self.span_o(open.start_o, open.end_o),
            )
        });
        let mut arms = Vec::new();
        let mut last_end = open.end_o;
        loop {
            self.skip_blanks();
            if self.block_body_done(&open) || self.at_close("match") {
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
                _ => {
                    // §7.3: a <match> body admits only <when>/<otherwise> arms.
                    // Report the stray line before skipping it (mirroring
                    // <track>/E-TIMELINE-CONTENT), not a silent drop.
                    self.emit_line(
                        E_LOGIC_CONTENT,
                        "a <match> body may contain only <when> and <otherwise> children (dsl §7.3)",
                        self.cursor,
                        Layer::Logic,
                    );
                    self.skip_stray();
                }
            }
        }
        let end_o = self.consume_close("match", &open, last_end);
        Match {
            subject,
            attrs,
            arms,
            span: self.span_o(open.start_o, end_o),
        }
    }

    /// `When ::= "<when" Attrs ">" Node* "</when>"` (§7.3, §11.2).
    fn parse_when(&mut self) -> Arm {
        let open = self.parse_open_tag();
        let mut attrs = open.attrs.clone();
        // `is="…"` (dsl §7.3.1) is a literal pattern, NOT a CEL expression:
        // preserve it verbatim (trimmed) with its value span. `None` when absent.
        let is = take_str_spanned(&mut attrs, "is").map(|(raw, span)| IsPattern {
            raw: raw.trim().to_string(),
            span,
        });
        let test = take_cel(&mut attrs, "test", CelKind::Condition).unwrap_or_else(|| {
            CelSlot::raw(
                CelKind::Condition,
                String::new(),
                self.span_o(open.start_o, open.end_o),
            )
        });
        let (body, end_o) = self.parse_block_body("when", &open);
        Arm::When {
            is,
            test,
            attrs,
            body,
            span: self.span_o(open.start_o, end_o),
        }
    }

    /// `Otherwise ::= "<otherwise>" Node* "</otherwise>"` (§7.3, §11.2).
    /// Attribute closure moved to the checker in 0.10.0 (§4, D-J): the residual
    /// list rides on the arm and `E-UNKNOWN-ATTR` reports it at the attribute's
    /// own column, uniformly with every other logic tag. `E-LOGIC-CONTENT`
    /// survives here for its three BODY-SHAPE rules (`:178`, `:309`, `:378`)
    /// and only those.
    fn parse_otherwise(&mut self) -> Arm {
        let open = self.parse_open_tag();
        let (body, end_o) = self.parse_block_body("otherwise", &open);
        Arm::Otherwise {
            attrs: open.attrs,
            body,
            span: self.span_o(open.start_o, end_o),
        }
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
            if self.block_body_done(&open) || self.at_close("timeline") {
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
        Timeline {
            duration,
            tracks,
            span: self.span_o(open.start_o, end_o),
        }
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
            if self.block_body_done(&open) || self.at_close("track") {
                break;
            }
            let trimmed = self.trimmed(self.cursor);
            if trimmed.starts_with("::set{") {
                if let Node::Set(set) = self.parse_set() {
                    last_end = set.span.byte_end;
                    clips.push(Clip {
                        at: None,
                        span: set.span,
                        node: ClipNode::Set(set),
                    });
                }
            } else if trimmed.starts_with("::") {
                if let Node::Directive(mut d) = self.parse_directive() {
                    let at = take_at(&mut d.attrs);
                    last_end = d.span.byte_end;
                    clips.push(Clip {
                        at,
                        span: d.span,
                        node: ClipNode::Directive(d),
                    });
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
        Track {
            key,
            clips,
            span: self.span_o(open.start_o, end_o),
        }
    }

    /// Parse the generic body of a `<tag>…</tag>` (choice/when/otherwise): full
    /// nodes until the matching close, a `## ` heading, or EOF. Returns
    /// `(body, end_o)` where `end_o` is the block's original-text end offset.
    fn parse_block_body(&mut self, name: &str, open: &OpenTag) -> (Vec<Node>, usize) {
        let mut body = Vec::new();
        let mut last_end = open.end_o;
        loop {
            self.skip_blanks();
            if self.block_body_done(open) || self.at_close(name) {
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

    /// Parse an owner block body (`<quest>` / `<objective>`) intercepting
    /// every self-closing `<reward/>` (dsl 0.16.0 §2) into a sibling vector
    /// while every other node still flows through the shared [`Parser::
    /// next_node`]. Rewards are OWNER FIELDS, never [`Node`] variants: the
    /// existing exhaustive `Node` match at each walker/checker/compiler site
    /// stays untouched. Returns `(body, rewards, end_o)`.
    fn parse_owner_body(
        &mut self,
        name: &str,
        open: &OpenTag,
    ) -> (Vec<Node>, Vec<crate::ast::Reward>, usize) {
        let mut body = Vec::new();
        let mut rewards = Vec::new();
        let mut last_end = open.end_o;
        loop {
            self.skip_blanks();
            if self.block_body_done(open) || self.at_close(name) {
                break;
            }
            let trimmed = self.trimmed(self.cursor);
            if trimmed.starts_with("</") {
                // A close for some other tag: our tag is unclosed — stop here.
                break;
            }
            if trimmed.starts_with('<')
                && super::open_tag_name(&trimmed).as_deref() == Some("reward")
            {
                let r = self.parse_reward();
                last_end = r.span.byte_end;
                rewards.push(r);
                continue;
            }
            if let Some(node) = self.next_node() {
                last_end = super::node_end(&node);
                body.push(node);
            }
        }
        let end_o = self.consume_close(name, open, last_end);
        (body, rewards, end_o)
    }

    /// Parse a self-closing `<reward kind= target= amount= when= on=/>`
    /// (dsl 0.16.0 §2). A non-self-closing form is a parse-layer error
    /// (reuses [`E_LOGIC_CONTENT`]: the closest "content on a construct that
    /// admits none" shape) and every downstream field is populated as if the
    /// element were empty; recovery skips subsequent lines up to a matching
    /// `</reward>` so the owner body loop is not left dangling.
    fn parse_reward(&mut self) -> crate::ast::Reward {
        use crate::ast::{Attr, AttrValue, Reward};
        let open = self.parse_open_tag();
        if !open.self_closing && !open.inline_closed {
            self.emit_o(
                E_LOGIC_CONTENT,
                "a `<reward>` element must be self-closing: `<reward … />` (dsl 0.16.0 §2)"
                    .to_string(),
                open.start_o,
                open.end_o,
                Layer::Logic,
            );
            // Skip until </reward>, a heading, or EOF — recovery only,
            // never a body parse (rewards are leaves).
            while self.cursor < self.lines.len() {
                if self.stop_at_heading() {
                    break;
                }
                if self.at_close("reward") {
                    self.cursor += 1;
                    break;
                }
                self.cursor += 1;
            }
        }
        let mut attrs = open.attrs.clone();
        let (kind, kind_span) = take_str_spanned(&mut attrs, "kind")
            .unwrap_or_else(|| (String::new(), self.span_o(open.start_o, open.end_o)));
        let target = take_str(&mut attrs, "target");
        let (amount, amount_span) = match take_str_spanned(&mut attrs, "amount") {
            Some((raw, sp)) => match parse_reward_amount(&raw) {
                Some(v) => (Some(v), Some(sp)),
                None => {
                    // Keep the raw attr so the checker can anchor
                    // `E-REWARD-ATTR` at the original value span.
                    attrs.push(Attr {
                        key: "amount".to_string(),
                        value: AttrValue::Str(raw),
                        value_span: sp,
                        span: sp,
                    });
                    (None, Some(sp))
                }
            },
            None => (None, None),
        };
        let when = take_cel(&mut attrs, "when", crate::ast::CelKind::Condition);
        let (on, on_span) = match take_str_spanned(&mut attrs, "on") {
            Some((v, sp)) => (Some(v), Some(sp)),
            None => (None, None),
        };
        Reward {
            kind,
            kind_span,
            target,
            amount,
            amount_span,
            when,
            on,
            on_span,
            attrs,
            span: self.span_o(open.start_o, open.end_o),
            self_closing: open.self_closing,
        }
    }

    /// True when the block opened by `open` claims no further physical lines,
    /// for either of the two reasons every child scan above shares: the cursor
    /// left this block's territory (EOF, or a `## ` shot heading — both hard
    /// terminators), or the element was ALREADY closed on its own opener line
    /// (dsl §2.3's unsupported single-line form, reported as
    /// [`E_TAG_INLINE_BODY`]). The second is what stops the parser swallowing
    /// the author's next well-formed sibling as this element's stray child and
    /// misreporting it — with the arm/child stream left intact, the checker
    /// also keeps a basis for the verdicts it draws from it.
    fn block_body_done(&self, open: &OpenTag) -> bool {
        open.inline_closed || self.cursor >= self.lines.len() || self.stop_at_heading()
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

/// True if `rest` — the text following a `<tag …>` opener's `>` on the opener's
/// OWN physical line — holds that tag's `</name>` close, i.e. the whole element
/// was written on one line (dsl §2.3's unsupported single-line form). The name
/// must end where [`close_tag_name`]'s own `take_while` would end it, so
/// `</when>` and `</when >` match while `</whenever>` does not.
fn holds_inline_close(rest: &str, name: &str) -> bool {
    rest.match_indices("</").any(|(i, _)| {
        rest[i + 2..].strip_prefix(name).is_some_and(|tail| {
            !tail.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        })
    })
}

/// Original-text end offset of an [`Arm`].
fn arm_end(a: &Arm) -> usize {
    match a {
        Arm::When { span, .. } => span.byte_end,
        Arm::Otherwise { span, .. } => span.byte_end,
    }
}

/// Take (remove) the `at="…"` clip-position attr, keeping its text and the
/// value's own span (dsl §7.4, §11.4). dsl 0.10.0 §10.2: no `f64` is parsed
/// here — the checker converts by shifting the decimal, and diagnoses a
/// sub-millisecond value at the span this returns.
fn take_at(attrs: &mut Vec<Attr>) -> Option<ClipAt> {
    let pos = attrs.iter().position(|a| a.key == "at")?;
    let attr = attrs.remove(pos);
    match attr.value {
        AttrValue::Str(raw) => Some(ClipAt {
            raw,
            span: attr.value_span,
        }),
        // A bare ident or an `@ref` is not a literal position; the clip behaves
        // as if `at` were absent, exactly as before.
        _ => None,
    }
}

/// Parse a `<reward amount="…">` literal (dsl 0.16.0 §2): either a signed
/// decimal integer (`Scalar`) or an inclusive `N..M` range (`Range`) where
/// both bounds are signed decimal integers and `N <= M`. Any other shape —
/// non-numeric, overflowing `i64`, `N > M`, extra whitespace, floats — is
/// `None`; the caller preserves the raw attr so the checker can anchor
/// `E-REWARD-ATTR` at the original value span.
fn parse_reward_amount(raw: &str) -> Option<crate::ast::RewardAmount> {
    use crate::ast::RewardAmount;
    let raw = raw.trim();
    // A `..` is the range separator; longer runs (`...`) are malformed.
    if let Some(mid) = raw.find("..") {
        if raw[mid..].starts_with("...") {
            return None;
        }
        let lo: i64 = raw[..mid].parse().ok()?;
        let hi: i64 = raw[mid + 2..].parse().ok()?;
        if lo > hi {
            return None;
        }
        return Some(RewardAmount::Range(lo, hi));
    }
    raw.parse::<i64>().ok().map(RewardAmount::Scalar)
}

#[cfg(test)]
mod tests {
    use crate::ast::Node;
    use crate::parse;

    #[test]
    fn hub_parses_choices_with_flags() {
        let src = "## Shot 1.\n<hub id=\"chat\">\n<choice id=\"a\" label=\"Ask\" once>\n@bianca: Sure.\n</choice>\n<choice id=\"leave\" label=\"Go\" exit>\n@fixer: Bye.\n</choice>\n</hub>\n";
        let (doc, diags) = parse(src);
        assert!(diags.is_empty(), "{diags:?}");
        let Node::Hub(h) = &doc.shots[0].body[0] else {
            panic!()
        };
        assert_eq!(h.choices.len(), 2);
        assert!(h.choices[0].attrs.iter().any(|a| a.key == "once"));
        assert!(h.choices[1].attrs.iter().any(|a| a.key == "exit"));
    }

    #[test]
    fn hub_rejects_non_choice_children() {
        let src = "## Shot 1.\n<hub id=\"chat\">\n@narrator: stray\n</hub>\n";
        let (_, diags) = parse(src);
        assert!(diags.iter().any(|d| d.code == "E-LOGIC-CONTENT"));
    }

    #[test]
    fn hub_nested_in_choice_bodies_parse() {
        // Node::Hub must flow through next_node inside <choice> bodies, both in a
        // sibling <hub> and inside a <branch>'s <choice> (dsl §7.3.2).
        let src = "## Shot 1.\n<hub id=\"outer\">\n<choice id=\"a\" label=\"A\">\n<hub id=\"inner\">\n<choice id=\"x\" label=\"X\">\n@bianca: hi\n</choice>\n</hub>\n</choice>\n</hub>\n<branch id=\"b\">\n<choice id=\"c\" label=\"C\">\n<hub id=\"h2\">\n<choice id=\"y\" label=\"Y\">\n@fixer: yo\n</choice>\n</hub>\n</choice>\n</branch>\n";
        let (doc, diags) = parse(src);
        assert!(diags.is_empty(), "{diags:?}");
        let Node::Hub(outer) = &doc.shots[0].body[0] else {
            panic!("expected outer Hub")
        };
        let Node::Hub(inner) = &outer.choices[0].body[0] else {
            panic!("expected inner Hub")
        };
        assert_eq!(inner.choices.len(), 1);
        let Node::Branch(br) = &doc.shots[0].body[1] else {
            panic!("expected Branch")
        };
        let Node::Hub(h2) = &br.choices[0].body[0] else {
            panic!("expected Hub in branch choice")
        };
        assert_eq!(h2.choices.len(), 1);
    }

    #[test]
    fn on_parses_event_when_and_body() {
        let (doc, diags) = crate::parse(
            "## Shot 1.\n<on event=\"combatEnd\" when=\"run.dead\">\n@narrator: silence.\n</on>\n",
        );
        assert!(diags.is_empty(), "{diags:?}");
        let Node::On(on) = &doc.shots[0].body[0] else {
            panic!("{:?}", doc.shots[0].body)
        };
        assert_eq!(on.event, "combatEnd");
        assert!(on.when.is_some());
        assert_eq!(on.body.len(), 1);
    }

    #[test]
    fn objective_self_closing_has_empty_body() {
        let (doc, diags) = crate::parse(
            "## Shot 1.\n<objective id=\"reach\" title=\"Reach\" done=\"run.here\"/>\n",
        );
        assert!(diags.is_empty(), "{diags:?}");
        let Node::Objective(o) = &doc.shots[0].body[0] else {
            panic!()
        };
        assert_eq!(o.id, "reach");
        assert_eq!(o.title.as_deref(), Some("Reach"));
        assert!(o.done.raw.contains("run.here"));
        assert!(o.body.is_empty());
        assert!(!o.optional);
    }

    #[test]
    fn objective_optional_flag_parses() {
        let (doc, _) = crate::parse("## Shot 1.\n<objective id=\"x\" done=\"a\" optional/>\n");
        let Node::Objective(o) = &doc.shots[0].body[0] else {
            panic!()
        };
        assert!(o.optional);
    }

    #[test]
    fn objective_long_form_body_emits() {
        let (doc, diags) = crate::parse(
            "## Shot 1.\n<objective id=\"x\" done=\"a\">\n::set{run.x = 1}\n</objective>\n",
        );
        assert!(diags.is_empty(), "{diags:?}");
        let Node::Objective(o) = &doc.shots[0].body[0] else {
            panic!()
        };
        assert_eq!(o.body.len(), 1);
    }

    #[test]
    fn quest_reward_parses_scalar_into_rewards_vector() {
        let src =
            "<quest id=\"q\">\n<reward kind=\"gold\" target=\"party\" amount=\"5\"/>\n</quest>\n";
        let (doc, diags) = crate::parse(src);
        assert!(diags.is_empty(), "{diags:?}");
        let q = &doc.quests[0];
        assert!(q.body.is_empty(), "reward MUST NOT reach the body stream");
        assert_eq!(q.rewards.len(), 1);
        let r = &q.rewards[0];
        assert_eq!(r.kind, "gold");
        assert_eq!(r.target.as_deref(), Some("party"));
        assert_eq!(r.amount, Some(crate::ast::RewardAmount::Scalar(5)));
        assert!(r.when.is_none());
        assert!(r.on.is_none());
        assert!(r.self_closing);
    }

    #[test]
    fn quest_reward_range_literal_parses_to_range() {
        let (doc, diags) =
            crate::parse("<quest id=\"q\">\n<reward kind=\"shard\" amount=\"1..5\"/>\n</quest>\n");
        assert!(diags.is_empty(), "{diags:?}");
        assert_eq!(
            doc.quests[0].rewards[0].amount,
            Some(crate::ast::RewardAmount::Range(1, 5))
        );
    }

    #[test]
    fn quest_reward_negative_scalar_parses() {
        let (doc, diags) =
            crate::parse("<quest id=\"q\">\n<reward kind=\"debt\" amount=\"-3\"/>\n</quest>\n");
        assert!(diags.is_empty(), "{diags:?}");
        assert_eq!(
            doc.quests[0].rewards[0].amount,
            Some(crate::ast::RewardAmount::Scalar(-3))
        );
    }

    #[test]
    fn objective_reward_lands_on_objective_rewards() {
        let src = "<quest id=\"q\">\n<objective id=\"o\" done=\"run.d\">\n<reward kind=\"gold\" amount=\"1\"/>\n</objective>\n</quest>\n";
        let (doc, diags) = crate::parse(src);
        assert!(diags.is_empty(), "{diags:?}");
        let q = &doc.quests[0];
        assert_eq!(q.rewards.len(), 0);
        assert_eq!(q.body.len(), 1);
        let Node::Objective(o) = &q.body[0] else {
            panic!()
        };
        assert!(o.body.is_empty());
        assert_eq!(o.rewards.len(), 1);
        assert_eq!(o.rewards[0].kind, "gold");
    }

    #[test]
    fn reward_in_scene_body_hits_unknown_tag_diagnostic() {
        // Outside an owner, `<reward/>` MUST fall through to the existing
        // unknown-tag arm (today's `E-UNCLASSIFIED`).
        let src = "## Shot 1.\n<reward kind=\"gold\" amount=\"1\"/>\n";
        let (doc, diags) = crate::parse(src);
        assert!(doc.shots[0].body.is_empty());
        assert!(diags.iter().any(|d| d.code == "E-UNCLASSIFIED"));
    }

    #[test]
    fn non_self_closing_reward_is_a_parse_error() {
        let src = "<quest id=\"q\">\n<reward kind=\"gold\">\n</reward>\n</quest>\n";
        let (_, diags) = crate::parse(src);
        assert!(
            diags.iter().any(|d| d.code == "E-LOGIC-CONTENT"),
            "want E-LOGIC-CONTENT for a non-self-closing <reward>, got {diags:?}"
        );
    }

    #[test]
    fn malformed_amount_survives_in_residual_attrs() {
        // The parser leaves `amount=` raw so the checker can anchor
        // `E-REWARD-ATTR` at its own span.
        let src = "<quest id=\"q\">\n<reward kind=\"gold\" amount=\"5..2\"/>\n</quest>\n";
        let (doc, _diags) = crate::parse(src);
        let r = &doc.quests[0].rewards[0];
        assert!(r.amount.is_none());
        assert!(r.amount_span.is_some());
        assert!(r.attrs.iter().any(|a| a.key == "amount"));
    }
}
