//! CEL source-text scanning.

/// For each byte of `raw`, whether that byte lies inside a CEL **string literal**
/// (§4.4). The opening and closing quote bytes are themselves marked `true`, so a
/// scanner can treat any position with `mask[i] == true` as string content to be
/// skipped.
///
/// CEL string literals are single- (`'…'`) or double-quoted (`"…"`) with `\`
/// escaping; an escaped quote (`\'`) does not close the literal. Every byte of a
/// multibyte character inside a string is marked, so indexing the returned `Vec`
/// by any byte offset into `raw` is always valid. An unterminated literal marks
/// through end-of-input (a malformed fragment degrades safely: its bytes are
/// treated as string content rather than mis-tokenized as DSL `@`/`$`).
///
/// Shared with the LSP feature layer (`lute_lsp::features::path_tokens`/`path_at`,
/// S3) so DSL-token and state-path scanning agree on string boundaries.
pub fn cel_string_mask(raw: &str) -> Vec<bool> {
    let b = raw.as_bytes();
    let mut mask = vec![false; b.len()];
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'\'' || c == b'"' {
            let quote = c;
            mask[i] = true; // opening quote
            i += 1;
            while i < b.len() {
                mask[i] = true;
                if b[i] == b'\\' {
                    // Escape: the next byte is literal string content, not a close.
                    i += 1;
                    if i < b.len() {
                        mask[i] = true;
                        i += 1;
                    }
                    continue;
                }
                if b[i] == quote {
                    i += 1; // closing quote (already marked)
                    break;
                }
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    mask
}
