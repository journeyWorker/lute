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
    close_tag_name, open_tag_name, Parser, E_LOGIC_CONTENT, E_TAG_NOT_ONE_LINE,
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
        // terminator itself — true for both a plain `>` close and a
        // self-closing `/>` (scan_attrs skips the lone `/` as an unparseable
        // token, then finds `>` right after). When that fails, the opener's
        // `>`/`/>` was not reached on its own physical line — name it instead
        // of leaving a misleading `E-UNCLOSED-TAG`/`E-UNCLASSIFIED` to fire
        // from wherever the parser resyncs. Do NOT attempt to consume the
        // wrap — the one-physical-line model (§2.3) is retained, not relaxed.
        if self.body.as_bytes().get(after.wrapping_sub(1)) != Some(&b'>') {
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
        self.cursor += 1;
        OpenTag {
            attrs,
            start_o,
            end_o,
            self_closing,
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
            if self.cursor >= self.lines.len() || self.stop_at_heading() || self.at_close("branch")
            {
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
        let (body, end_o) = self.parse_block_body("quest", &open);
        Quest {
            id,
            id_span,
            title,
            start,
            fail,
            attrs,
            body,
            span: self.span_o(open.start_o, end_o),
        }
    }

    /// `Objective ::= "<objective" Attrs ">" Node* "</objective>" | "<objective"
    /// Attrs "/>"` (dsl 0.2.0 §6.4). `done` is required but a MISSING `done`
    /// still yields a valid AST (empty CEL slot) — `E-OBJECTIVE-MISSING-DONE`
    /// is a Plan C checker diagnostic, NOT a parse error. Mirrors
    /// `parse_when`/`parse_match`'s empty-slot idiom exactly.
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
        let when = take_cel(&mut attrs, "when", CelKind::Condition);
        let title = take_str(&mut attrs, "title");
        let optional = take_bool(&mut attrs, "optional");
        let (body, end_o) = if open.self_closing {
            (Vec::new(), open.end_o)
        } else {
            self.parse_block_body("objective", &open)
        };
        Objective {
            id,
            id_span,
            done,
            when,
            title,
            optional,
            attrs,
            body,
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
            if self.cursor >= self.lines.len() || self.stop_at_heading() || self.at_close("hub") {
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
            body,
            span: self.span_o(open.start_o, end_o),
        }
    }

    /// `Otherwise ::= "<otherwise>" Node* "</otherwise>"` (§7.3, §11.2).
    fn parse_otherwise(&mut self) -> Arm {
        let open = self.parse_open_tag();
        if !open.attrs.is_empty() {
            self.emit_o(
                E_LOGIC_CONTENT,
                "<otherwise> takes no attributes (dsl §7.3)".to_string(),
                open.start_o,
                open.end_o,
                Layer::Logic,
            );
        }
        let (body, end_o) = self.parse_block_body("otherwise", &open);
        Arm::Otherwise {
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
            if self.cursor >= self.lines.len()
                || self.stop_at_heading()
                || self.at_close("timeline")
            {
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
            if self.cursor >= self.lines.len() || self.stop_at_heading() || self.at_close("track") {
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

#[cfg(test)]
mod tests {
    use crate::ast::Node;
    use crate::parse;

    #[test]
    fn hub_parses_choices_with_flags() {
        let src = "## Shot 1.\n<hub id=\"chat\">\n<choice id=\"a\" label=\"Ask\" once>\n@bianca: Sure.\n</choice>\n<choice id=\"leave\" label=\"Go\" exit>\n@fixer: Bye.\n</choice>\n</hub>\n";
        let (doc, diags) = parse(src);
        assert!(diags.is_empty(), "{diags:?}");
        let Node::Hub(h) = &doc.shots[0].body[0] else { panic!() };
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
        let Node::Hub(outer) = &doc.shots[0].body[0] else { panic!("expected outer Hub") };
        let Node::Hub(inner) = &outer.choices[0].body[0] else { panic!("expected inner Hub") };
        assert_eq!(inner.choices.len(), 1);
        let Node::Branch(br) = &doc.shots[0].body[1] else { panic!("expected Branch") };
        let Node::Hub(h2) = &br.choices[0].body[0] else { panic!("expected Hub in branch choice") };
        assert_eq!(h2.choices.len(), 1);
    }

    #[test]
    fn on_parses_event_when_and_body() {
        let (doc, diags) = crate::parse(
            "## Shot 1.\n<on event=\"combatEnd\" when=\"run.dead\">\n@narrator: silence.\n</on>\n",
        );
        assert!(diags.is_empty(), "{diags:?}");
        let Node::On(on) = &doc.shots[0].body[0] else { panic!("{:?}", doc.shots[0].body) };
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
        let Node::Objective(o) = &doc.shots[0].body[0] else { panic!() };
        assert_eq!(o.id, "reach");
        assert_eq!(o.title.as_deref(), Some("Reach"));
        assert!(o.done.raw.contains("run.here"));
        assert!(o.body.is_empty());
        assert!(!o.optional);
    }

    #[test]
    fn objective_optional_flag_parses() {
        let (doc, _) = crate::parse(
            "## Shot 1.\n<objective id=\"x\" done=\"a\" optional/>\n",
        );
        let Node::Objective(o) = &doc.shots[0].body[0] else { panic!() };
        assert!(o.optional);
    }

    #[test]
    fn objective_long_form_body_emits() {
        let (doc, diags) = crate::parse(
            "## Shot 1.\n<objective id=\"x\" done=\"a\">\n::set{run.x = 1}\n</objective>\n",
        );
        assert!(diags.is_empty(), "{diags:?}");
        let Node::Objective(o) = &doc.shots[0].body[0] else { panic!() };
        assert_eq!(o.body.len(), 1);
    }
}
