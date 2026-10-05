use lute_core_span::Span;

/// Best-effort span of the mapping key `key` anywhere in whole-file YAML
/// `text` (a claimed declaration document — no `---` envelope, so byte 0 IS
/// the frontmatter start; unlike `features::find_yaml_key_span`'s `.lute`
/// `+4`-past-`"---\n"` convention). Matches only where `key` sits at a line
/// start (after indent) immediately followed by `:`, so a same-named
/// substring inside a value never steals the span — mirrors
/// `lute_check::meta`'s private `meta_key_span` (documented there as "kept in
/// sync" with `lute_lsp`'s own copy; this is a THIRD context — whole-file, no
/// envelope — so it gets its own copy rather than reusing either private
/// original). `None` when `key` never appears as a mapping key; callers fall
/// back to the whole-file span.
pub(super) fn find_key_span(text: &str, key: &str) -> Option<Span> {
    let mut line_start = 0usize;
    for line in text.split_inclusive('\n') {
        let indent = line.len() - line.trim_start().len();
        if let Some(rest) = line.trim_start().strip_prefix(key) {
            if rest.trim_start().starts_with(':') {
                let start = line_start + indent;
                return Some(Span {
                    byte_start: start,
                    byte_end: start + key.len(),
                    line: 0,
                    column: 0,
                    utf16_range: (0, 0),
                });
            }
        }
        line_start += line.len();
    }
    None
}

/// Byte offset where the line containing `byte` begins.
fn line_start(text: &str, byte: usize) -> usize {
    text[..byte].rfind('\n').map(|p| p + 1).unwrap_or(0)
}

/// Byte offset one past the end of the line containing `byte` (i.e. right
/// after its trailing `\n`, or `text.len()` on the last line).
fn line_end(text: &str, byte: usize) -> usize {
    text[byte..]
        .find('\n')
        .map(|p| byte + p + 1)
        .unwrap_or(text.len())
}

/// End of the indented block that opens right after `start` and is nested
/// under a parent key at `parent_indent`: the first byte offset, from
/// `start`, of a non-blank non-comment line whose OWN indent is `<=
/// parent_indent` (a sibling or dedented line), or `text.len()` if the
/// block runs to EOF. Blank/comment-only lines never end the block.
fn find_block_end(text: &str, start: usize, parent_indent: usize) -> usize {
    let mut pos = start;
    while pos < text.len() {
        let le = line_end(text, pos);
        let line = &text[pos..le];
        let content = line.trim_start_matches(' ');
        let indent = line.len() - content.len();
        let trimmed = content.trim_end();
        if !trimmed.is_empty() && !trimmed.starts_with('#') && indent <= parent_indent {
            return pos;
        }
        pos = le;
    }
    text.len()
}

/// Scoped, YAML-aware search for a DIRECT child mapping key inside
/// `[region_start, region_end)` — like [`find_key_span`], but bounded to
/// one block's own indent level so a same-named key belonging to an
/// unrelated mapping elsewhere in the file is never visited.
/// `region_start`/`region_end` MUST bound exactly one mapping's body (e.g.
/// via [`find_block_end`]). The block's own child indent is established
/// from its FIRST non-blank/non-comment line; only lines at that exact
/// indent are considered keys (deeper lines are a previous child's nested
/// content, skipped). Returns `(key_byte_start, colon_byte_offset,
/// child_indent)` for the first match.
fn find_scoped_child_key(
    text: &str,
    region_start: usize,
    region_end: usize,
    key: &str,
) -> Option<(usize, usize, usize)> {
    let mut pos = region_start;
    let mut child_indent: Option<usize> = None;
    while pos < region_end {
        let le = line_end(text, pos).min(region_end);
        let line = &text[pos..le];
        let content = line.trim_start_matches(' ');
        let line_indent = line.len() - content.len();
        let trimmed = content.trim_end();
        if !trimmed.is_empty() && !trimmed.starts_with('#') {
            let target_indent = *child_indent.get_or_insert(line_indent);
            if line_indent == target_indent {
                if let Some(rest) = trimmed.strip_prefix(key) {
                    let rest_trimmed = rest.trim_start();
                    if rest_trimmed.starts_with(':') {
                        let key_start = pos + line_indent;
                        let colon_off = key_start + key.len() + (rest.len() - rest_trimmed.len());
                        return Some((key_start, colon_off, line_indent));
                    }
                }
            }
        }
        pos = le;
    }
    None
}

/// Index right after the double-quoted YAML scalar starting at `text[start]
/// == '"'`, skipping `\`-escaped characters so an escaped inner quote
/// (`\"`) never ends the scan early. `None` if unterminated.
fn skip_double_quoted(text: &str, start: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'"' => return Some(i + 1),
            _ => i += 1,
        }
    }
    None
}

/// Index right after the single-quoted YAML scalar starting at `text[start]
/// == '\''`, treating a doubled `''` as an escaped literal quote (YAML's
/// single-quote escape — no backslash escapes in this style). `None` if
/// unterminated.
fn skip_single_quoted(text: &str, start: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut i = start + 1;
    while i < bytes.len() {
        if bytes[i] == b'\'' {
            if bytes.get(i + 1) == Some(&b'\'') {
                i += 2;
                continue;
            }
            return Some(i + 1);
        }
        i += 1;
    }
    None
}

/// Index of the `}` matching the `{` at `text[open_abs]`, skipping quoted
/// scalar interiors (so a `}`/`{` inside a CEL string literal is never
/// mistaken for flow-mapping structure) and nested flow mappings. `None` if
/// unterminated.
fn find_matching_brace(text: &str, open_abs: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut i = open_abs + 1;
    let mut depth = 1;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => i = skip_double_quoted(text, i)?,
            b'\'' => i = skip_single_quoted(text, i)?,
            b'{' => {
                depth += 1;
                i += 1;
            }
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    None
}

/// The byte range, bounded by `region_end`, that holds def entry `name`'s
/// own body — everything belonging to THIS entry, never a sibling's. Flow
/// form (`name: { ... }`) resolves via brace matching (the returned range
/// sits strictly inside the braces); block form (`name:\n  cel: ...`)
/// resolves via indentation (every following line more-indented than
/// `entry_indent`). `None` when the value right after `name:`'s colon is
/// neither `{` nor blank/comment (e.g. a bare scalar) — that shape isn't
/// the `type:`/`cel:` mapping this locator expects, so the caller must fall
/// back rather than guess.
fn find_entry_extent(
    text: &str,
    colon_abs: usize,
    key_line_end: usize,
    entry_indent: usize,
    region_end: usize,
) -> Option<(usize, usize)> {
    let after_colon = colon_abs + 1;
    let rest_of_line = &text[after_colon..key_line_end];
    let trimmed = rest_of_line.trim_start_matches(' ');
    let value_col_start = after_colon + (rest_of_line.len() - trimmed.len());
    let trimmed_no_nl = trimmed.trim_end();
    if trimmed_no_nl.starts_with('{') {
        let close = find_matching_brace(text, value_col_start)?;
        if close >= region_end {
            return None;
        }
        Some((value_col_start + 1, close))
    } else if trimmed_no_nl.is_empty() || trimmed_no_nl.starts_with('#') {
        let mut pos = key_line_end;
        let mut end = key_line_end;
        while pos < region_end {
            let le = line_end(text, pos).min(region_end);
            let line = &text[pos..le];
            let content = line.trim_start_matches(' ');
            let line_indent = line.len() - content.len();
            if content.trim_end().is_empty() {
                pos = le;
                continue;
            }
            if line_indent <= entry_indent {
                break;
            }
            end = le;
            pos = le;
        }
        Some((key_line_end, end))
    } else {
        None
    }
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Locates `key`'s `:` inside `[region.0, region.1)`, skipping the interior
/// of any single/double-quoted scalar so CEL text that happens to contain a
/// `"cel:"`-shaped substring can never be mistaken for the real key.
/// Requires a whole-word match on both sides (`concel:` never matches
/// `cel`). Returns the absolute byte offset of the `:` character.
fn find_unquoted_key_colon(text: &str, region: (usize, usize), key: &str) -> Option<usize> {
    let (start, end) = region;
    let bytes = text.as_bytes();
    let mut i = start;
    while i < end {
        match bytes[i] {
            b'"' => i = skip_double_quoted(text, i)?,
            b'\'' => i = skip_single_quoted(text, i)?,
            b if is_word_byte(b) => {
                let word_start = i;
                let mut j = i;
                while j < end && is_word_byte(bytes[j]) {
                    j += 1;
                }
                if &text[word_start..j] == key {
                    let after = &text[j..end];
                    let after_trimmed = after.trim_start_matches(' ');
                    if after_trimmed.starts_with(':') {
                        return Some(j + (after.len() - after_trimmed.len()));
                    }
                }
                i = j;
            }
            _ => i += 1,
        }
    }
    None
}

fn mk_span(byte_start: usize, byte_end: usize) -> Span {
    Span {
        byte_start,
        byte_end,
        line: 0,
        column: 0,
        utf16_range: (0, 0),
    }
}

/// Source span of a `|`/`>` block scalar's CONTENT lines: every line after
/// `key_line_end` with indent strictly greater than `key_indent` (the
/// `key:` line's own indent), bounded by `region_end`. The block scalar's
/// chomping/indent indicators (`|-`, `>+`, `|2`, ...) only ever change how
/// the DECODED text is built from those lines — never WHERE the lines
/// themselves sit in source — so they play no part in locating this span;
/// the caller never even inspects them. Interior blank lines are included
/// (they're part of the content run); trailing blank lines / the final
/// newline are trimmed off the end. `None` for an empty block scalar (no
/// more-indented line follows the key at all).
fn block_scalar_content_span(
    text: &str,
    key_line_end: usize,
    key_indent: usize,
    region_end: usize,
) -> Option<Span> {
    let mut pos = key_line_end;
    let mut content_start: Option<usize> = None;
    let mut content_end = key_line_end;
    while pos < region_end {
        let le = line_end(text, pos).min(region_end);
        let line = &text[pos..le];
        let content = line.trim_start_matches(' ');
        let line_indent = line.len() - content.len();
        if content.trim_end().is_empty() {
            pos = le;
            continue;
        }
        if line_indent <= key_indent {
            break;
        }
        content_start.get_or_insert(pos);
        content_end = le;
        pos = le;
    }
    let start = content_start?;
    let end = start + text[start..content_end].trim_end().len();
    Some(mk_span(start, end))
}

/// The whole scalar VALUE span for a `key:` whose `:` sits at `colon_abs`,
/// bounded by `region_end`. Handles double-quoted (`\`-escaped),
/// single-quoted (`''`-escaped), and bare/plain flow scalars (terminated by
/// `,`, `}`, or end of line) — quotes included in the returned span. A `|`/
/// `>` literal/folded block scalar resolves to its CONTENT-lines span via
/// [`block_scalar_content_span`] (indicators are irrelevant to locating
/// source lines, only to decoding them — see that function's doc comment).
/// `None` when the block scalar has no content, or a quoted scalar is
/// unterminated.
fn parse_scalar_value_span(text: &str, colon_abs: usize, region_end: usize) -> Option<Span> {
    let after = colon_abs + 1;
    if after > region_end {
        return None;
    }
    let rest = &text[after..region_end];
    let lead = rest.len() - rest.trim_start_matches(' ').len();
    let val_start = after + lead;
    if val_start >= region_end {
        return None;
    }
    let bytes = text.as_bytes();
    match bytes[val_start] {
        b'"' => {
            let end = skip_double_quoted(text, val_start)?;
            (end <= region_end).then(|| mk_span(val_start, end))
        }
        b'\'' => {
            let end = skip_single_quoted(text, val_start)?;
            (end <= region_end).then(|| mk_span(val_start, end))
        }
        b'|' | b'>' => {
            let key_line_start = line_start(text, colon_abs);
            let key_line_end = line_end(text, colon_abs);
            let key_line = &text[key_line_start..key_line_end];
            let key_indent = key_line.len() - key_line.trim_start_matches(' ').len();
            block_scalar_content_span(text, key_line_end, key_indent, region_end)
        }
        _ => {
            let sub = &text[val_start..region_end];
            let end_rel = sub.find([',', '}', '\n']).unwrap_or(sub.len());
            let trimmed = sub[..end_rel].trim_end();
            (!trimmed.is_empty()).then(|| mk_span(val_start, val_start + trimmed.len()))
        }
    }
}

/// YAML-aware source-span locator scoped to `defs.<name>.cel` (dsl §8.1 —
/// closes the finding-7 gap `analyze_declaration`'s doc comment used to
/// flag): locates the entry for `name` under the top-level `defs:` mapping,
/// then the `cel:` key within THAT entry, then the whole scalar VALUE span
/// in source bytes. Handles inline/flow entries (`name: { ..., cel: "..."
/// }`), block entries (`name:\n  cel: "..."`), single/double-quoted and
/// `\`-escaped scalars (quotes included in the span), `|`/`>` literal/
/// folded block scalars (resolved to their CONTENT-lines span — chomping/
/// indent indicators only affect decoding, never where the source lines
/// sit, so they're irrelevant here), and a file where the same CEL text is
/// duplicated elsewhere (the search is scoped to `name`'s own entry, so a
/// duplicate can never steal the span). Returns `None` — never a guessed or
/// wrong span — on anything it genuinely can't resolve: no `defs:`, no
/// entry for `name` (e.g. a quoted key `"name":` the byte-level scanner
/// doesn't unquote), no `cel:` key in it, an entry value shape it doesn't
/// understand (e.g. an anchor tag `&name` before `{`, or an alias `*name`),
/// or an empty/unterminated scalar: callers MUST fall back to
/// [`find_key_span`]'s key span in that case.
pub(super) fn find_def_cel_value_span(text: &str, name: &str) -> Option<Span> {
    let defs_key = find_key_span(text, "defs")?;
    let defs_indent = defs_key.byte_start - line_start(text, defs_key.byte_start);
    let defs_body_start = line_end(text, defs_key.byte_start);
    let defs_body_end = find_block_end(text, defs_body_start, defs_indent);

    let (entry_key_start, entry_colon, entry_indent) =
        find_scoped_child_key(text, defs_body_start, defs_body_end, name)?;
    let entry_key_line_end = line_end(text, entry_key_start).min(defs_body_end);
    let (cel_region_start, cel_region_end) = find_entry_extent(
        text,
        entry_colon,
        entry_key_line_end,
        entry_indent,
        defs_body_end,
    )?;

    let cel_colon = find_unquoted_key_colon(text, (cel_region_start, cel_region_end), "cel")?;
    parse_scalar_value_span(text, cel_colon, cel_region_end)
}
