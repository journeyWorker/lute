//! Markup from other languages that Lute ships as literal text (round-6
//! T1-24).
//!
//! A content line's text and a `<choice>` label are literal: only `{{…}}` is
//! read (dsl §7.6). An author porting from Ink or Yarn writes that
//! language's inline markup — `{run.oil}`, `{cond: text}`, `{~a|b}`, `{$x}`,
//! Yarn's `{0}`, Ink glue `<>`, a trailing `// note` or `# tag`, a
//! `[bracketed]` choice label — and the player sees it verbatim. Each shape
//! is a warning naming what Lute writes instead. Braces whose inside has
//! none of these shapes (`{sighs}`) are left alone, as is `{{…}}` and an
//! escaped `\{{`. Yarn's `[b]…[/b]` markup is left alone too: it is also
//! the BBCode an engine's rich-text label renders.

use lute_core_span::{Diagnostic, Layer, Severity, Span, TextIndex};
use lute_syntax::ast::Line;

use crate::ctx::Ctx;

/// A single-brace group whose inside reads as a state path, a def, a Yarn
/// variable, Ink conditional text or Ink alternatives.
pub const W_TEXT_SINGLE_BRACE: &str = "W-TEXT-SINGLE-BRACE";
/// A ` // note` or trailing `#tag` inside text: a comment or an Ink tag that
/// ships as text.
pub const W_TEXT_COMMENT_LIKE: &str = "W-TEXT-COMMENT-LIKE";
/// A choice label wrapped in `[…]`, Ink's bracket suppression: the brackets
/// show on the button.
pub const W_TEXT_BRACKET_LABEL: &str = "W-TEXT-BRACKET-LABEL";
/// Ink glue `<>` inside text: Lute joins no lines, so the player sees it.
pub const W_TEXT_GLUE: &str = "W-TEXT-GLUE";

/// Longest brace group or tail echoed back verbatim.
const ECHO_MAX: usize = 40;

/// Where a piece of literal text sits: what it is, for the message, and how
/// to anchor a finding at its exact bytes.
struct Text<'a> {
    text: &'a str,
    /// Span of `text` in the source; the fallback anchor.
    span: Span,
    /// The document source, when `span` indexes it and covers exactly
    /// `text` — then a finding is anchored at its own bytes.
    src: Option<&'a str>,
    site: Site<'a>,
}

#[derive(Clone, Copy)]
enum Site<'a> {
    /// A content line spoken by this speaker.
    Line(&'a str),
    /// A `<choice>` label.
    Label,
}

impl Text<'_> {
    /// What the text is, after "literal": `line text`, `text in a choice
    /// label`.
    fn noun(&self) -> &'static str {
        match self.site {
            Site::Line(_) => "line text",
            Site::Label => "text in a choice label",
        }
    }

    /// Span of `text[start..end]`, or the whole text's span when the source
    /// does not hold `text` at `span` (text spliced from elsewhere).
    fn span_of(&self, start: usize, end: usize) -> Span {
        let (s, e) = (self.span.byte_start, self.span.byte_end);
        match self.src {
            Some(src) if src.get(s..e) == Some(self.text) => {
                Span::from_bytes(&TextIndex::new(src), s + start, s + end)
            }
            _ => self.span,
        }
    }

    fn warn(&self, code: &str, message: String, start: usize, end: usize) -> Diagnostic {
        Diagnostic {
            code: code.to_string(),
            severity: Severity::Warning,
            message,
            span: self.span_of(start, end),
            layer: Layer::Content,
            fixits: Vec::new(),
            provenance: None,
            covered: Vec::new(),
            related: Vec::new(),
        }
    }
}

/// The literal-text warnings for a content line. `src` is the document
/// source `l`'s spans index, when the caller has it (a component body
/// checked from its importer does not; its findings anchor at the text).
pub(super) fn line_text(l: &Line, ctx: &Ctx<'_>, src: Option<&str>, diags: &mut Vec<Diagnostic>) {
    let text = Text {
        text: &l.text,
        span: l.text_span,
        src,
        site: Site::Line(&l.speaker),
    };
    single_braces(&text, ctx, diags);
    glue(&text, diags);
    comment_like(&text, diags);
}

/// The literal-text warnings for a `<choice>` label whose value sits at
/// `span` in `src` (the document source, when the caller has it).
pub(super) fn choice_label(
    label: &str,
    span: Span,
    src: Option<&str>,
    ctx: &Ctx<'_>,
    diags: &mut Vec<Diagnostic>,
) {
    let text = Text {
        text: label,
        span,
        src,
        site: Site::Label,
    };
    single_braces(&text, ctx, diags);
    glue(&text, diags);
    comment_like(&text, diags);
    // Ink's `* [Go inside]` hides the bracketed label from the output; the
    // whole label in brackets is that habit. A leading tag followed by more
    // text (`[Persuasion] Step into the light`) is a label meant to show.
    let trimmed = label.trim();
    if let Some(inner) = trimmed
        .strip_prefix('[')
        .and_then(|r| r.strip_suffix(']'))
        .filter(|i| !i.contains(['[', ']']) && !i.trim().is_empty())
    {
        diags.push(text.warn(
            W_TEXT_BRACKET_LABEL,
            format!(
                "choice label `{}` shows its brackets on the button — a label is shown exactly \
                 as written (Lute has no Ink-style bracket suppression); write `label=\"{}\"`",
                echo(trimmed),
                inner.trim()
            ),
            0,
            label.len(),
        ));
    }
}

/// `W-TEXT-GLUE` for Ink glue `<>` inside text.
fn glue(text: &Text<'_>, diags: &mut Vec<Diagnostic>) {
    if let Some(at) = text.text.find("<>") {
        diags.push(text.warn(
            W_TEXT_GLUE,
            format!(
                "`<>` is Ink glue and literal {}, so the player sees it — Lute joins nothing: \
                 write the whole sentence on one line",
                text.noun()
            ),
            at,
            at + 2,
        ));
    }
}

/// `W-TEXT-SINGLE-BRACE` for every `{…}` group (not `{{…}}`, not escaped
/// `\{{`) whose inside has a recognized shape.
fn single_braces(text: &Text<'_>, ctx: &Ctx<'_>, diags: &mut Vec<Diagnostic>) {
    let s = text.text;
    let b = s.as_bytes();
    let mut j = 0;
    while j < b.len() {
        if b[j] == b'\\' && s[j + 1..].starts_with("{{") {
            j += 3;
            continue;
        }
        // Byte tests only: `j` walks bytes, so it can sit inside a
        // multi-byte character (`—`), where slicing `s[j..]` panics.
        if b[j] == b'{' && b.get(j + 1) == Some(&b'{') {
            match s[j + 2..].find("}}") {
                Some(rel) => {
                    j += 2 + rel + 2;
                    continue;
                }
                None => break,
            }
        }
        if b[j] == b'{' {
            if let Some(rel) = s[j + 1..].find(['{', '}']) {
                let close = j + 1 + rel;
                if b[close] == b'}' {
                    // `\{run.oil\}`: a backslash escapes nothing in line
                    // text, so it ships beside the braces.
                    let escaped = j > 0 && b[j - 1] == b'\\';
                    let start = if escaped { j - 1 } else { j };
                    let inner = &s[j + 1..close];
                    let inner = if escaped {
                        inner.strip_suffix('\\').unwrap_or(inner)
                    } else {
                        inner
                    };
                    if let Some(why) = brace_shape(inner, text.site, ctx) {
                        let seen = if escaped {
                            "a backslash does not escape a brace here, so the player sees the \
                             braces and the backslashes"
                        } else {
                            "single braces are not read, so the player sees them"
                        };
                        diags.push(text.warn(
                            W_TEXT_SINGLE_BRACE,
                            format!(
                                "`{}` is literal {}: {seen} — {why}",
                                echo(&s[start..=close]),
                                text.noun()
                            ),
                            start,
                            close + 1,
                        ));
                    }
                    j = close + 1;
                    continue;
                }
            }
        }
        j += 1;
    }
}

/// What Lute writes for a single-brace group with inside `inner`, or `None`
/// when `inner` has no recognized shape (`{sighs}` is a stage direction).
fn brace_shape(inner: &str, site: Site<'_>, ctx: &Ctx<'_>) -> Option<String> {
    let t = inner.trim();
    if let Some(var) = t.strip_prefix('$').filter(|v| is_ident(v)) {
        return Some(format!(
            "Yarn's `{{${var}}}` is an interpolation of a declared state path in Lute, \
             `{{{{run.{var}}}}}`"
        ));
    }
    if let Some((cond, _)) = t.split_once(':').filter(|(c, _)| looks_like_condition(c)) {
        let cond = cond.trim();
        let guard = match site {
            Site::Line(speaker) => format!("`@{speaker}{{when=\"{cond}\"}}: …`"),
            Site::Label => format!("`<choice … when=\"{cond}\">`"),
        };
        return Some(format!(
            "Lute has no inline conditional text; guard the whole {} instead ({guard}), or \
             choose between lines with `<match>`",
            match site {
                Site::Line(_) => "line",
                Site::Label => "choice",
            }
        ));
    }
    if t.starts_with(['~', '&', '!']) || t.contains('|') {
        return Some(
            "Lute has no inline alternatives (Ink's `{~…|…}` shuffle, `{&…|…}` cycle, `{!…|…}` \
             once-only, `{…|…}` sequence); choose between whole lines with `<match>` or guarded \
             lines (`when=\"…\"`)"
                .to_string(),
        );
    }
    if is_path(t) {
        return Some(format!("interpolation is `{{{{{t}}}}}`"));
    }
    if !t.is_empty() && t.bytes().all(|c| c.is_ascii_digit()) {
        return Some(format!(
            "Yarn's `{{{t}}}` is a placeholder its line provider fills; Lute fills none, so \
             interpolate the value itself (`{{{{run.…}}}}`)"
        ));
    }
    if let Some(name) = t.strip_prefix('@').filter(|n| ctx.env.defs.contains(*n)) {
        return Some(format!("interpolation is `{{{{@{name}}}}}`"));
    }
    None
}

/// `true` for a dotted state path rooted at a tier: `run.oil`, `user.a.b`.
fn is_path(s: &str) -> bool {
    s.contains('.') && crate::cel_paths::is_state_path(s) && s.split('.').all(is_ident)
}

fn is_ident(s: &str) -> bool {
    s.chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// `true` when the text before a `:` in `{…: …}` reads as a condition: it
/// names a state path, a Yarn `$var` or a def, or compares.
fn looks_like_condition(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() {
        return false;
    }
    let names_state = s
        .split(|c: char| {
            !(c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '$' || c == '@')
        })
        .any(|tok| {
            is_path(tok)
                || tok.strip_prefix('$').is_some_and(is_ident)
                || tok.strip_prefix('@').is_some_and(is_ident)
        });
    names_state
        || ["==", "!=", ">", "<", "&&", "||"]
            .iter()
            .any(|op| s.contains(op))
}

/// `W-TEXT-COMMENT-LIKE` for a ` // …` comment (`//` standing alone as a
/// word, so `http://` is not one) and a trailing Ink `#tag` run.
fn comment_like(text: &Text<'_>, diags: &mut Vec<Diagnostic>) {
    let s = text.text;
    if let Some(at) = s.match_indices("//").map(|(i, _)| i).find(|&i| {
        s[..i].chars().next_back().is_none_or(char::is_whitespace)
            && s[i + 2..].chars().next().is_none_or(char::is_whitespace)
    }) {
        diags.push(text.warn(
            W_TEXT_COMMENT_LIKE,
            format!(
                "`{}` is part of the {}, so the player sees it — a comment is `// …` on a line \
                 of its own (or after a directive), never after text",
                echo(s[at..].trim_end()),
                text.noun()
            ),
            at,
            s.len(),
        ));
    }
    if let Some(at) = tag_tail(s) {
        diags.push(text.warn(
            W_TEXT_COMMENT_LIKE,
            format!(
                "`{}` is part of the {}, so the player sees it — Lute has no line tags; keep a \
                 note as a `// …` comment on a line of its own",
                echo(s[at..].trim_end()),
                text.noun()
            ),
            at,
            s.len(),
        ));
    }
}

/// Byte offset of a trailing run of Ink tags (`# mood:cold`, `#loud #fast`)
/// ending `s`: each `#` starts a word, may be followed by one space, and
/// names a tag that does not start with a digit (`#1` is text).
fn tag_tail(s: &str) -> Option<usize> {
    let is_tag_run = |mut rest: &str| -> bool {
        loop {
            let Some(r) = rest.strip_prefix('#') else {
                return false;
            };
            let r = r.strip_prefix(' ').unwrap_or(r);
            let word_len = r.find(char::is_whitespace).unwrap_or(r.len());
            let word = &r[..word_len];
            if word.is_empty() || word.starts_with(|c: char| c.is_ascii_digit() || c == '#') {
                return false;
            }
            rest = r[word_len..].trim_start();
            if rest.is_empty() {
                return true;
            }
        }
    };
    s.match_indices('#').map(|(i, _)| i).find(|&i| {
        s[..i].chars().next_back().is_none_or(char::is_whitespace) && is_tag_run(&s[i..])
    })
}

/// `s` for a message: verbatim up to [`ECHO_MAX`] characters.
fn echo(s: &str) -> String {
    if s.chars().count() <= ECHO_MAX {
        return s.to_string();
    }
    let cut: String = s.chars().take(ECHO_MAX - 2).collect();
    match s.chars().last() {
        Some(c @ ('}' | ']')) => format!("{}…{c}", cut.trim_end()),
        _ => format!("{}…", cut.trim_end()),
    }
}
