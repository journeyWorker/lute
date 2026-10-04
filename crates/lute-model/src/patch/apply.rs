use lute_core_span::Span;

pub(crate) fn span_valid(span: Span, length: usize) -> bool {
    span.byte_start <= span.byte_end && span.byte_end <= length
}

pub(crate) fn line_bounds(text: &str, regions: &[Span]) -> Option<(usize, usize)> {
    if regions.is_empty() {
        return None;
    }
    let start = regions.iter().map(|span| span.byte_start).min()?;
    let end = regions.iter().map(|span| span.byte_end).max()?;
    let line_start = text[..start.min(text.len())]
        .rfind('\n')
        .map(|index| index + 1)
        .unwrap_or(0);
    let line_end = text[end.min(text.len())..]
        .find('\n')
        .map(|index| end.min(text.len()) + index + 1)
        .unwrap_or(text.len());
    Some((line_start, line_end))
}

pub(crate) fn format_touched(source: &str, regions: &[Span], yaml: bool) -> Result<String, String> {
    if yaml {
        return Ok(lute_syntax::format_yaml_source(source).text);
    }
    let full = lute_syntax::format_source(
        source,
        &lute_syntax::FormatOptions {
            regions: regions.to_vec(),
        },
    )
    .map_err(|error| format!("{error:?}"))?
    .text;
    let Some((start, end)) = line_bounds(source, regions) else {
        return Ok(full);
    };
    if start == 0 && end == source.len() {
        return Ok(full);
    }
    let start_line = source[..start]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count();
    let count = source[start..end]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        .max(1);
    let mut formatted_lines = full.split_inclusive('\n').collect::<Vec<_>>();
    if !full.ends_with('\n') {
        formatted_lines.push(&full[full.len()..]);
    }
    let end_line = (start_line + count).min(formatted_lines.len());
    let mut output = String::new();
    output.push_str(&source[..start]);
    if start_line < formatted_lines.len() {
        output.extend(formatted_lines[start_line..end_line].iter().copied());
    }
    output.push_str(&source[end..]);
    Ok(output)
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
