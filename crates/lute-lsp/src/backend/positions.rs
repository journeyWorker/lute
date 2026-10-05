use lute_core_span::{Span, TextIndex};
use tower_lsp_server::ls_types::{Position, Range};

/// Convert an LSP [`Position`] (0-based line, 0-based UTF-16 character) to a byte
/// offset into `text`. The inverse of [`TextIndex::position`]: locate the line by
/// counting `\n`s, then walk that line's chars summing `len_utf16` until the
/// UTF-16 column is reached — the exact per-line UTF-16 accounting `TextIndex`
/// does forward, so the LSP and headless surfaces never drift a code unit.
/// A `character` past the line end clamps to the line end (defensive; LSP clients
/// occasionally over-report a column at EOL).
pub(super) fn position_to_byte(text: &str, pos: Position) -> usize {
    let bytes = text.as_bytes();
    // Byte offset of the start of line `pos.line` (0-based).
    let mut line = 0u32;
    let mut line_start = 0usize;
    if pos.line > 0 {
        for (i, b) in bytes.iter().enumerate() {
            if *b == b'\n' {
                line += 1;
                if line == pos.line {
                    line_start = i + 1;
                    break;
                }
            }
        }
        if line < pos.line {
            // Line beyond EOF: clamp to end of text.
            return text.len();
        }
    }
    // Walk the target line, summing UTF-16 units until `pos.character`.
    let line_end = text[line_start..]
        .find('\n')
        .map_or(text.len(), |n| line_start + n);
    let mut utf16 = 0u32;
    for (off, ch) in text[line_start..line_end].char_indices() {
        if utf16 >= pos.character {
            return line_start + off;
        }
        utf16 += ch.len_utf16() as u32;
    }
    line_end
}

/// Map a byte [`Span`] to an LSP [`Range`] through `idx`. Mirrors
/// [`crate::convert`]'s private byte-span mapping (`TextIndex::position`,
/// de-1-indexing the line, 0-based UTF-16 column) so navigation results carry the
/// same UTF-16-correct positions as diagnostics. Byte-only spans synthesized by
/// the feature layer (zeroed line/col) resolve correctly here because the mapping
/// only reads `byte_start`/`byte_end`.
pub(crate) fn span_to_range(span: &Span, idx: &TextIndex) -> Range {
    Range {
        start: byte_to_position(span.byte_start, idx),
        end: byte_to_position(span.byte_end, idx),
    }
}

pub(crate) fn byte_to_position(byte: usize, idx: &TextIndex) -> Position {
    let p = idx.position(byte);
    Position {
        line: p.line - 1,
        character: p.utf16_col,
    }
}
