use serde::{Deserialize, Serialize};

/// Precomputed line-start table for byte <-> (line, col, utf16) mapping.
pub struct TextIndex<'a> {
    text: &'a str,
    line_starts: Vec<usize>, // byte offset of each line start
}

#[derive(Clone, Copy, Debug)]
pub struct Position {
    pub line: u32,      // 1-based
    pub column: u32,    // 1-based byte column within line
    pub utf16_col: u32, // 0-based UTF-16 column within line
}

impl<'a> TextIndex<'a> {
    pub fn new(text: &'a str) -> Self {
        let mut line_starts = vec![0usize];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        Self { text, line_starts }
    }

    pub fn position(&self, byte: usize) -> Position {
        let line_ix = match self.line_starts.binary_search(&byte) {
            Ok(i) => i,
            Err(i) => i - 1,
        };
        let line_start = self.line_starts[line_ix];
        let slice = &self.text[line_start..byte];
        let byte_col = (byte - line_start) as u32;
        let utf16_col = slice.chars().map(|c| c.len_utf16() as u32).sum();
        Position {
            line: line_ix as u32 + 1,
            column: byte_col + 1,
            utf16_col,
        }
    }

    fn utf16_offset(&self, byte: usize) -> u32 {
        // total UTF-16 units from start of file to byte (for LSP ranges we use per-line cols,
        // but Span keeps a file-relative utf16_range for the divergence golden)
        self.text[..byte]
            .chars()
            .map(|c| c.len_utf16() as u32)
            .sum()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub byte_start: usize,
    pub byte_end: usize,
    pub line: u32,               // 1-based, of byte_start
    pub column: u32,             // 1-based byte column of byte_start
    pub utf16_range: (u32, u32), // file-relative UTF-16 offsets
}

impl Span {
    pub fn from_bytes(idx: &TextIndex, start: usize, end: usize) -> Self {
        let p = idx.position(start);
        Span {
            byte_start: start,
            byte_end: end,
            line: p.line,
            column: p.column,
            utf16_range: (idx.utf16_offset(start), idx.utf16_offset(end)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
    Hint,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Layer {
    Content,
    Staging,
    Logic,
    Cel,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fixit {
    pub title: String,
    pub kind: String, // e.g. "quickfix"
    pub edit: Vec<TextEdit>,
    pub confidence: u8, // 0..=100
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextEdit {
    pub span: Span,
    pub new_text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub code: String, // stable, e.g. "E-UNDECLARED"
    pub severity: Severity,
    pub message: String,
    pub span: Span,
    pub layer: Layer,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fixits: Vec<Fixit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<String>,
}

/// Stable node id: assigned once, survives edits (dsl §12 textUnitId principle).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StableId(pub u64);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_index_maps_byte_to_line_col_and_utf16() {
        // "a\nsé" : 'é' is 2 bytes (U+00E9), 1 UTF-16 unit
        let idx = TextIndex::new("a\nsé");
        // byte 0 = line 1 col 1
        let p0 = idx.position(0);
        assert_eq!((p0.line, p0.column), (1, 1));
        // byte 2 = start of line 2 ('s')
        let p2 = idx.position(2);
        assert_eq!((p2.line, p2.column), (2, 1));
        // 'é' begins at byte 3; its UTF-16 column within line 2 is 1 (0-based), byte column 2
        let p3 = idx.position(3);
        assert_eq!(p3.line, 2);
        assert_eq!(p3.utf16_col, 1);
    }

    #[test]
    fn span_from_bytes_fills_both_encodings() {
        let idx = TextIndex::new("hello");
        let s = Span::from_bytes(&idx, 1, 4);
        assert_eq!((s.byte_start, s.byte_end), (1, 4));
        assert_eq!(s.line, 1);
        assert_eq!(s.column, 2); // 1-based byte column
        assert_eq!(s.utf16_range, (1, 4));
    }
}
