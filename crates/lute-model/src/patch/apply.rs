use lute_core_span::Span;

pub(crate) fn span_valid(span: Span, length: usize) -> bool {
    span.byte_start <= span.byte_end && span.byte_end <= length
}

/// Map a byte offset of the original text into the written text after every
/// edit of `edits` (sorted by start, non-overlapping) has been applied.
///
/// An offset moves by the net growth of the edits wholly before it. An offset
/// strictly inside an edit maps to the replacement's start (`end == false`) or
/// end (`end == true`), so a region always covers the replacement text. A
/// zero-width insertion exactly at a region start belongs to the region; one
/// exactly at a region end belongs to it too.
pub(crate) fn map_offset(edits: &[TextEdit], offset: usize, end: bool) -> usize {
    let signed = |n: usize| isize::try_from(n).expect("a text offset is at most isize::MAX");
    // Sorted, non-overlapping edits wholly before an offset remove at most
    // the bytes before it, so a shifted offset is never negative.
    let unsigned = |n: isize| usize::try_from(n).expect("a shifted offset is non-negative");
    let mut shift = 0isize;
    for edit in edits {
        if edit.start < offset && offset < edit.end {
            let start = unsigned(signed(edit.start) + shift);
            return if end { start + edit.text.len() } else { start };
        }
        let before = if end {
            edit.end <= offset
        } else {
            edit.end < offset || (edit.end == offset && edit.start < edit.end)
        };
        if before {
            shift += signed(edit.text.len()) - signed(edit.end - edit.start);
        }
    }
    unsigned(signed(offset) + shift)
}

/// The written-text byte ranges of `edits`: each edit's original region (the
/// node span, which contains the edit) mapped through every edit of the file.
pub(crate) fn written_regions(edits: &[TextEdit]) -> Vec<(usize, usize)> {
    edits
        .iter()
        .map(|edit| {
            (
                map_offset(edits, edit.region.byte_start.min(edit.start), false),
                map_offset(edits, edit.region.byte_end.max(edit.end), true),
            )
        })
        .collect()
}

fn blank(line: &str) -> bool {
    line.trim_matches([' ', '\t', '\r', '\n', '\u{feff}'])
        .is_empty()
}

/// Lines (with terminators) of `text`; a final line without `\n` is kept.
fn lines(text: &str) -> Vec<&str> {
    text.split_inclusive('\n').collect()
}

/// A region-formatted text and the byte ranges of its formatted line runs.
pub(crate) struct Formatted {
    pub(crate) text: String,
    pub(crate) touched: Vec<(usize, usize)>,
}

/// Format the lines the byte `regions` touch and leave every other byte alone.
///
/// The whole file is formatted once (block depth depends on every line before
/// a touched one), then, per touched line run, the lines from its first to its
/// last non-blank source line are replaced by their formatted counterparts.
/// The formatter emits exactly one line per non-blank source line, in order,
/// so the k-th non-blank source line corresponds to the k-th non-blank
/// formatted line. YAML files are formatted whole.
pub(crate) fn format_touched(
    source: &str,
    regions: &[(usize, usize)],
    yaml: bool,
) -> Result<Formatted, String> {
    if yaml {
        return Ok(Formatted {
            text: lute_syntax::format_yaml_source(source).text,
            touched: Vec::new(),
        });
    }
    let full = lute_syntax::format_source(source, &lute_syntax::FormatOptions::default())
        .map_err(|error| format!("{error:?}"))?
        .text;
    let source_lines = lines(source);
    let formatted_lines = lines(&full);
    let formatted_nonblank: Vec<usize> = formatted_lines
        .iter()
        .enumerate()
        .filter(|(_, line)| !blank(line))
        .map(|(index, _)| index)
        .collect();
    // Ordinal of each non-blank source line among the non-blank lines.
    let mut ordinal = vec![None; source_lines.len()];
    let mut count = 0usize;
    for (index, line) in source_lines.iter().enumerate() {
        if !blank(line) {
            ordinal[index] = Some(count);
            count += 1;
        }
    }
    if count != formatted_nonblank.len() {
        return Err("formatter output does not align with the source lines".into());
    }
    let mut starts = Vec::with_capacity(source_lines.len());
    let mut offset = 0usize;
    for line in &source_lines {
        starts.push(offset);
        offset += line.len();
    }
    let line_of = |offset: usize| match starts.binary_search(&offset) {
        Ok(index) => index,
        Err(index) => index.saturating_sub(1),
    };
    let formatted_of = |line: usize| ordinal[line].map(|k| formatted_nonblank[k]);
    // Each touched line run as a source line range `a..b` and its formatted
    // counterpart `fa..fb`. A run edge on a blank line widens to the nearest
    // non-blank line (exclusive), so removed or inserted blank lines take the
    // formatter's blank-line layout without touching the neighbour itself.
    let mut runs: Vec<(usize, usize, usize, usize)> = Vec::with_capacity(regions.len());
    if !source_lines.is_empty() {
        for &(start, end) in regions {
            let start = start.min(source.len() - 1);
            let end = end.min(source.len());
            let first = line_of(start);
            let last = if end > start { line_of(end - 1) } else { first };
            let (a, fa) = match formatted_of(first) {
                Some(f) => (first, f),
                None => match (0..first)
                    .rev()
                    .find_map(|i| formatted_of(i).map(|f| (i, f)))
                {
                    Some((i, f)) => (i + 1, f + 1),
                    None => (0, 0),
                },
            };
            let (b, fb) = match formatted_of(last) {
                Some(f) => (last + 1, f + 1),
                None => match (last + 1..source_lines.len())
                    .find_map(|i| formatted_of(i).map(|f| (i, f)))
                {
                    Some((i, f)) => (i, f),
                    None => (source_lines.len(), formatted_lines.len()),
                },
            };
            runs.push((a, b, fa, fb.max(fa)));
        }
    }
    runs.sort_unstable();
    let mut merged: Vec<(usize, usize, usize, usize)> = Vec::with_capacity(runs.len());
    for run in runs {
        match merged.last_mut() {
            Some(last) if run.0 <= last.1 => {
                last.1 = last.1.max(run.1);
                last.3 = last.3.max(run.3);
            }
            _ => merged.push(run),
        }
    }
    let mut text = String::with_capacity(full.len().max(source.len()));
    let mut touched = Vec::with_capacity(merged.len());
    let mut next = 0usize;
    for (a, b, fa, fb) in merged {
        for line in &source_lines[next..a] {
            text.push_str(line);
        }
        let start = text.len();
        for line in &formatted_lines[fa..fb] {
            text.push_str(line);
        }
        if text.len() > start {
            touched.push((start, text.len()));
        }
        next = b;
    }
    for line in &source_lines[next..] {
        text.push_str(line);
    }
    Ok(Formatted { text, touched })
}

/// The format guarantee of a region-formatted `.lute` file (dsl 0.47.0 §5.1):
/// a file whose original bytes were `fmt --check` clean is clean after the
/// patch; any other file's touched line runs are canonical (formatting them
/// again changes nothing).
pub(crate) fn format_guard_holds(original: &str, written: &Formatted) -> bool {
    let canonical = |text: &str| {
        lute_syntax::format_source(text, &lute_syntax::FormatOptions::default())
            .is_ok_and(|result| result.text == text)
    };
    if canonical(original) {
        return canonical(&written.text);
    }
    format_touched(&written.text, &written.touched, false)
        .is_ok_and(|again| again.text == written.text)
}

pub(crate) fn attr_range(text: &str, span: Span, attr: &str) -> Option<(usize, usize)> {
    let body = &text[span.byte_start..span.byte_end];
    let needle = format!("{attr}=");
    let start = body.find(&needle)? + span.byte_start + needle.len();
    let bytes = text.as_bytes();
    let mut end = start;
    if bytes.get(start) == Some(&b'"') || bytes.get(start) == Some(&b'\'') {
        let quote = bytes[start];
        end += 1;
        while end < text.len() {
            if bytes[end] == quote && bytes[end - 1] != b'\\' {
                end += 1;
                break;
            }
            end += 1;
        }
    }
    Some((start, end))
}

#[derive(Clone)]
pub(crate) struct TextEdit {
    pub(crate) file: std::path::PathBuf,
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) text: String,
    pub(crate) index: usize,
    pub(crate) region: Span,
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str = "---\nkind: scene\nid: fair\n---\n\n## Opening\n\n";

    fn edit(start: usize, end: usize, text: &str) -> TextEdit {
        TextEdit {
            file: "scene.lute".into(),
            start,
            end,
            text: text.into(),
            index: 0,
            region: Span {
                byte_start: start,
                byte_end: end,
                line: 1,
                column: 1,
                utf16_range: (0, 0),
            },
        }
    }

    #[test]
    fn offsets_move_by_the_net_growth_of_every_edit_before_them() {
        // "aaaa|bbbb|cccc": replace 0..4 with 8 bytes, 8..12 with 1 byte.
        let edits = [edit(0, 4, "AAAAAAAA"), edit(8, 12, "C")];
        assert_eq!(written_regions(&edits), vec![(0, 8), (12, 13)]);
        // Inside an edit: a start maps to the replacement start, an end to
        // its end, both shifted by the earlier growth.
        assert_eq!(map_offset(&edits, 10, false), 12);
        assert_eq!(map_offset(&edits, 10, true), 13);
        // An insertion at a region start belongs to the region.
        let insert = [edit(4, 4, "xy")];
        assert_eq!(map_offset(&insert, 4, false), 4);
        assert_eq!(map_offset(&insert, 4, true), 6);
    }

    #[test]
    fn touched_lines_are_spliced_per_run_and_far_lines_stay() {
        let source = format!(
            "{HEAD}@a: one   \n<branch id=\"pick\">\n<choice id=\"go\" text=\"Go\">\n@b: two\n  </choice>\n</branch>\n@c: far   \n"
        );
        let line = |needle: &str| {
            let start = source.find(needle).unwrap();
            (start, start + needle.len())
        };
        let formatted =
            format_touched(&source, &[line("@a: one"), line("@b: two")], false).unwrap();
        assert_eq!(
            formatted.text,
            format!(
                "{HEAD}@a: one\n<branch id=\"pick\">\n<choice id=\"go\" text=\"Go\">\n    @b: two\n  </choice>\n</branch>\n@c: far   \n"
            )
        );
        assert_eq!(formatted.touched.len(), 2);
        assert_eq!(
            &formatted.text[formatted.touched[1].0..formatted.touched[1].1],
            "    @b: two\n"
        );
    }

    #[test]
    fn format_guard_keeps_clean_files_clean_and_touched_runs_canonical() {
        let clean = format!("{HEAD}@a: one\n@b: two\n");
        let dirty = format!("{HEAD}@a: one   \n@b: two\n");
        let far = dirty.find("@a").unwrap();
        let near = dirty.find("@b").unwrap();
        let written = |text: &str, touched: Vec<(usize, usize)>| Formatted {
            text: text.into(),
            touched,
        };
        // A clean original must stay clean, wherever the damage is.
        assert!(format_guard_holds(&clean, &written(&clean, vec![])));
        assert!(!format_guard_holds(
            &clean,
            &written(&dirty, vec![(near, dirty.len())])
        ));
        // A non-canonical original only needs canonical touched runs.
        assert!(format_guard_holds(
            &dirty,
            &written(&dirty, vec![(near, dirty.len())])
        ));
        assert!(!format_guard_holds(
            &dirty,
            &written(&dirty, vec![(far, near)])
        ));
    }

    #[test]
    fn a_removed_line_takes_the_formatter_blank_layout() {
        // `@b: two` removed next to a blank line: the left empty line joins
        // the blank run, which collapses to one; the neighbours stay as they are.
        let source = format!("{HEAD}@a: one   \n\n\n@c: three   \n");
        let at = HEAD.len() + "@a: one   \n\n".len();
        let formatted = format_touched(&source, &[(at, at)], false).unwrap();
        assert_eq!(
            formatted.text,
            format!("{HEAD}@a: one   \n\n@c: three   \n")
        );
        // At the end of the file the empty line is dropped.
        let source = format!("{HEAD}@a: one\n\n");
        let at = source.len() - 1;
        let formatted = format_touched(&source, &[(at, at)], false).unwrap();
        assert_eq!(formatted.text, format!("{HEAD}@a: one\n"));
    }
}
