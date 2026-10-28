//! The one `{{` escape rule of line text (dsl 0.1.0 §7.6, lute 0.47.1 §3)
//! and the IR encoding that carries it into compiled `text`.
//!
//! **Source rule.** An odd run of backslashes directly before `{{` escapes
//! it: the `\{{` is literal text, consumed as one unit before the scan looks
//! for the next `{{`. An even run (none included) leaves the `{{` opening a
//! `{{…}}` marker, which ends at the first `}}` after it. The inline parser
//! ([`crate::parse_inline_text`]) reads line text this way, and every other
//! scanner of line text — the placeholder scan, label scanning, the
//! literal-text check, the normalizer — walks [`interp_sites`] so it agrees.
//!
//! **IR encoding.** The compiled `text`, each segment run's `text` and each
//! merged translation are the decoded plain text, except that a literal
//! `{{` is written `\{{` and every literal backslash directly before a `{{`
//! (literal or marker) is doubled ([`IrText`]). An engine reads n
//! backslashes before `{{` as ⌊n/2⌋ backslashes, then a literal `{{` (n
//! odd) or a marker (n even), so `placeholders` are 1:1 with the markers.
//!
//! **No literal `{` before a marker.** The encoding has no spelling for an
//! odd run of literal `{` directly before a marker: [`IrText`] escapes `{{`
//! pairs greedily, so the run's last `{` is written bare and every reader
//! opens the marker one brace early. [`brace_before_marker`] finds such a
//! marker; the inline parser reports it as `E-INTERP-BRACE` (0.52.0 §3.1). An
//! even run is representable (each pair is written `\{{`). A run the source
//! itself lets open the marker (`{{{run.a}}`: the marker's interior is
//! `{run.a`) is [`brace_opened`]; the checker reports it as `E-INTERP-BRACE`
//! in every `{{…}}` grammar (0.53.0 §8).

use crate::ast::{InlineNode, Interp};

/// One `{{` of line text, by the source rule. Offsets are byte offsets into
/// the scanned text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterpSite {
    /// An odd backslash run from `run` escapes the `{{` that ends at `end`:
    /// literal text.
    Escaped { run: usize, end: usize },
    /// The `{{…}}` marker `[start, end)`; `run` is where the (even, possibly
    /// empty) backslash run directly before it starts.
    Marker {
        run: usize,
        start: usize,
        end: usize,
    },
    /// A `{{` at `start` (after the even run from `run`) with no `}}` after
    /// it. The scan ends here.
    Unterminated { run: usize, start: usize },
}

impl InterpSite {
    /// Where the site's `{{` sits.
    pub fn open(&self) -> usize {
        match *self {
            Self::Escaped { end, .. } => end - 2,
            Self::Marker { start, .. } | Self::Unterminated { start, .. } => start,
        }
    }
}

/// Every `{{` site of `text`, left to right, by the source rule. Stops after
/// an [`InterpSite::Unterminated`].
pub fn interp_sites(text: &str) -> InterpSites<'_> {
    InterpSites { text, at: 0 }
}

/// The iterator [`interp_sites`] returns.
#[derive(Debug)]
pub struct InterpSites<'a> {
    text: &'a str,
    at: usize,
}

impl Iterator for InterpSites<'_> {
    type Item = InterpSite;

    fn next(&mut self) -> Option<InterpSite> {
        let text = self.text;
        let open = self.at + text.get(self.at..)?.find("{{")?;
        let b = text.as_bytes();
        let mut run = open;
        while run > self.at && b[run - 1] == b'\\' {
            run -= 1;
        }
        if (open - run) % 2 == 1 {
            self.at = open + 2;
            return Some(InterpSite::Escaped { run, end: open + 2 });
        }
        match text[open + 2..].find("}}") {
            Some(rel) => {
                let end = open + 2 + rel + 2;
                self.at = end;
                Some(InterpSite::Marker {
                    run,
                    start: open,
                    end,
                })
            }
            None => {
                self.at = text.len() + 1;
                Some(InterpSite::Unterminated { run, start: open })
            }
        }
    }
}

/// The `[start, end)` range of every `{{…}}` marker of `text`, left to
/// right, by the source rule.
pub fn interp_markers(text: &str) -> impl Iterator<Item = (usize, usize)> + '_ {
    interp_sites(text).filter_map(|site| match site {
        InterpSite::Marker { start, end, .. } => Some((start, end)),
        _ => None,
    })
}

/// A marker interior that opened on a literal brace run (0.53.0 §8): the
/// `{{` of `{{{run.a}}` is the literal `{` and the marker's own first `{`,
/// so its interior `source` is `{run.a`. Returns the run length n (every
/// leading `{`) and the interior the author meant (`run.a`), or `None` when
/// `source` does not begin with `{` or holds nothing after the run.
pub fn brace_opened(source: &str) -> Option<(usize, &str)> {
    let meant = source.trim_start_matches('{');
    let n = source.len() - meant.len();
    (n > 0 && !meant.trim().is_empty()).then_some((n, meant))
}

/// The first `{{…}}` marker of `nodes` whose preceding decoded literal text
/// ends in an odd run of `{` (0.52.0 §3.1), with the run's length. The
/// literal text is joined as [`crate::ast::inline_plain_text`] joins it —
/// modifier delimiters dropped, so a run continues across a modifier
/// boundary — and a marker ends the run. A marker that opened on a literal
/// brace itself ([`brace_opened`]) is the checker's to report, with its own
/// run (0.53.0 §8), so it is not returned here: one diagnostic per marker.
pub fn brace_before_marker(nodes: &[InlineNode]) -> Option<(&Interp, usize)> {
    fn walk<'a>(nodes: &'a [InlineNode], run: &mut usize) -> Option<(&'a Interp, usize)> {
        for node in nodes {
            match node {
                InlineNode::Text { text, .. } => {
                    let trailing = text.bytes().rev().take_while(|&b| b == b'{').count();
                    *run = if trailing == text.len() {
                        *run + trailing
                    } else {
                        trailing
                    };
                }
                InlineNode::Interpolation(i) => {
                    if *run % 2 == 1 && brace_opened(&i.source).is_none() {
                        return Some((i, *run));
                    }
                    *run = 0;
                }
                InlineNode::Modifier(m) => {
                    if let Some(found) = walk(&m.children, run) {
                        return Some(found);
                    }
                }
            }
        }
        None
    }
    walk(nodes, &mut 0)
}

/// Builds one IR text (a line's `text`, a segment run's `text`, a merged
/// translation) from decoded literal text and `{{…}}` markers, writing the
/// IR encoding: a literal `{{` as `\{{`, each literal backslash directly
/// before a `{{` doubled. Adjacent literal pieces join first, so a literal
/// `{{` split across pieces is still encoded.
#[derive(Debug, Default)]
pub struct IrText {
    out: String,
    literal: String,
}

impl IrText {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append decoded literal text.
    pub fn push_literal(&mut self, text: &str) {
        self.literal.push_str(text);
    }

    /// Append a `{{…}}` marker, written as is.
    pub fn push_marker(&mut self, marker: &str) {
        self.flush(true);
        self.out.push_str(marker);
    }

    /// Whether nothing has been appended.
    pub fn is_empty(&self) -> bool {
        self.out.is_empty() && self.literal.is_empty()
    }

    /// The encoded text.
    pub fn finish(mut self) -> String {
        self.flush(false);
        self.out
    }

    fn flush(&mut self, before_marker: bool) {
        let lit = std::mem::take(&mut self.literal);
        let b = lit.as_bytes();
        let mut copied = 0;
        let mut j = 0;
        // Byte tests only: `j` can sit inside a multi-byte character.
        while j < b.len() {
            if b[j] == b'\\' {
                let start = j;
                while j < b.len() && b[j] == b'\\' {
                    j += 1;
                }
                let before_open = if j == b.len() {
                    before_marker
                } else {
                    b[j..].starts_with(b"{{")
                };
                if before_open {
                    self.out.push_str(&lit[copied..j]);
                    self.out.push_str(&lit[start..j]);
                    copied = j;
                }
                continue;
            }
            if b[j..].starts_with(b"{{") {
                self.out.push_str(&lit[copied..j]);
                self.out.push_str("\\{{");
                j += 2;
                copied = j;
                continue;
            }
            j += 1;
        }
        self.out.push_str(&lit[copied..]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sites(text: &str) -> Vec<InterpSite> {
        interp_sites(text).collect()
    }

    #[test]
    fn odd_runs_escape_even_runs_open() {
        assert_eq!(sites("\\{{x}}"), [InterpSite::Escaped { run: 0, end: 3 }]);
        assert_eq!(
            sites("\\\\{{y}}"),
            [InterpSite::Marker {
                run: 0,
                start: 2,
                end: 7
            }]
        );
        assert_eq!(
            sites("\\\\\\{{z}}"),
            [InterpSite::Escaped { run: 0, end: 5 }]
        );
        // The escaped opener is one unit: no marker hides inside it.
        assert_eq!(sites("\\{{{a}}"), [InterpSite::Escaped { run: 0, end: 3 }]);
        assert_eq!(
            sites("a {{b"),
            [InterpSite::Unterminated { run: 2, start: 2 }]
        );
    }

    #[test]
    fn encodes_literal_openers_and_backslashes_before_them() {
        let mut t = IrText::new();
        t.push_literal("Lit {{run.a}} and ");
        t.push_marker("{{ run.a }}");
        t.push_literal(".");
        assert_eq!(t.finish(), "Lit \\{{run.a}} and {{ run.a }}.");
        // A literal backslash before a marker is doubled; elsewhere kept.
        let mut t = IrText::new();
        t.push_literal("x\\");
        t.push_marker("{{run.a}}");
        t.push_literal(" a\\b \\");
        assert_eq!(t.finish(), "x\\\\{{run.a}} a\\b \\");
        // A literal `\` then a literal `{{`: `\\` + `\{{`.
        let mut t = IrText::new();
        t.push_literal("\\{{");
        assert_eq!(t.finish(), "\\\\\\{{");
        // A literal `{{` split across pieces.
        let mut t = IrText::new();
        t.push_literal("{");
        t.push_literal("{{");
        assert_eq!(t.finish(), "\\{{{");
        // Multi-byte text around the escapes.
        let mut t = IrText::new();
        t.push_literal("글자 {{a}} 끝\\");
        t.push_marker("{{b}}");
        assert_eq!(t.finish(), "글자 \\{{a}} 끝\\\\{{b}}");
    }

    /// The run length and the marker source [`brace_before_marker`] finds
    /// in `text`, read by the inline parser, as `n:{{source}}`.
    fn brace_marker(text: &str) -> Option<String> {
        let (nodes, _) = crate::parse_inline_text(text);
        brace_before_marker(&nodes).map(|(i, n)| format!("{n}:{{{{{}}}}}", i.source))
    }

    #[test]
    fn an_odd_brace_run_before_a_marker_is_found() {
        // #139: `{` + the literal `{{` + the marker: a run of three.
        assert_eq!(
            brace_marker("S }}{\\{{{{run.a}}} E").as_deref(),
            Some("3:{{run.a}}")
        );
        // A plain `{` then an escaped pair.
        assert_eq!(
            brace_marker("{\\{{{{run.a}}").as_deref(),
            Some("3:{{run.a}}")
        );
        // The run continues across a modifier boundary, out of a span (an
        // escaped brace in it) and over an empty span.
        assert_eq!(
            brace_marker("x{:pause{s=0.5}{{run.a}}").as_deref(),
            Some("1:{{run.a}}")
        );
        assert_eq!(
            brace_marker(":em[x\\{]:pause{s=0.5}{{run.a}}").as_deref(),
            Some("1:{{run.a}}")
        );
        assert_eq!(
            brace_marker("{:em[]:pause{s=0.5}{{run.a}}").as_deref(),
            Some("1:{{run.a}}")
        );
        // The first such marker, after a clean one.
        assert_eq!(
            brace_marker("{{run.a}} and {\\{{{{run.b}}").as_deref(),
            Some("3:{{run.b}}")
        );
    }

    #[test]
    fn a_marker_opened_on_a_literal_brace_is_the_checkers() {
        // 0.53.0 §8: the run inside the marker is every leading `{`.
        assert_eq!(brace_opened("{run.a"), Some((1, "run.a")));
        assert_eq!(brace_opened("{{run.a:ordinal"), Some((2, "run.a:ordinal")));
        assert_eq!(brace_opened("{ run.a "), Some((1, " run.a ")));
        assert_eq!(brace_opened("run.a"), None);
        assert_eq!(brace_opened(" {run.a"), None);
        // Nothing after the run: no marker was meant.
        assert_eq!(brace_opened("{"), None);
        assert_eq!(brace_opened("{{ "), None);
        // The parser's finder leaves such a marker alone, even after an odd
        // run of literal braces, and finds a later one.
        assert_eq!(brace_marker("{{{run.a}}"), None);
        assert_eq!(brace_marker("{\\{{{{{run.a}}"), None);
        assert_eq!(
            brace_marker("{\\{{{{{run.a}} x{:pause{s=0.5}{{run.b}}").as_deref(),
            Some("1:{{run.b}}")
        );
    }

    #[test]
    fn an_even_run_or_a_separated_brace_is_not() {
        // A literal `{{` then the marker renders `{{3`.
        assert_eq!(brace_marker("\\{{{{run.a}}"), None);
        assert_eq!(brace_marker("\\{{\\{{{{run.a}}"), None);
        assert_eq!(brace_marker("{ {{run.a}}"), None);
        assert_eq!(brace_marker("{x{{run.a}}"), None);
        // A marker ends the run: `{{run.a}}{{run.b}}` has none between.
        assert_eq!(brace_marker("{{run.lbrace}}{{run.a}}"), None);
        // A brace after the last marker precedes none.
        assert_eq!(brace_marker("{{run.a}}{"), None);
        // A run inside the span is broken by text after it.
        assert_eq!(brace_marker(":em[{]x{{run.a}}"), None);
    }
}
