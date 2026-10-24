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
}
