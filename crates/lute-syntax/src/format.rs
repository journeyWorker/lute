use lute_core_span::{Diagnostic, Severity, Span};

/// Options for canonical formatting. An empty region list formats the entire document.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FormatOptions {
    pub regions: Vec<Span>,
}

/// The formatted document and whether its bytes differ from the input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormatResult {
    pub text: String,
    pub changed: bool,
}

/// Formatting can fail before producing a document when the source is not a
/// parseable Lute document, or when a preservation invariant cannot be met.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormatError {
    InvalidSource(Vec<Diagnostic>),
    Preservation { span: Span, reason: String },
}

fn trim_horizontal(s: &str) -> &str {
    s.trim_end_matches([' ', '\t'])
}

fn frontmatter_end(s: &str) -> Option<usize> {
    if !(s.starts_with("---\n") || s == "---") {
        return None;
    }
    let mut at = 4usize;
    while at <= s.len() {
        let end = s[at..].find('\n').map_or(s.len(), |n| at + n);
        if s[at..end].trim_end_matches('\r') == "---" {
            return Some(if end < s.len() { end + 1 } else { end });
        }
        if end == s.len() { break; }
        at = end + 1;
    }
    None
}

fn likely_yaml(s: &str) -> bool {
    let mut saw_mapping = false;
    for raw in s.lines().filter(|line| !line.trim().is_empty()) {
        let line = raw.trim_end_matches([' ', '\t']);
        let t = line.trim_start();
        if t.starts_with("##") || t.starts_with('@') || t.starts_with("::") || t.starts_with('<') {
            return false;
        }
        if t.contains(':') { saw_mapping = true; }
    }
    saw_mapping
}

fn quote_aware_attr_spacing(inner: &str) -> String {
    if !inner.is_ascii() {
        return inner.to_string();
    }
    let bytes = inner.as_bytes();
    let mut out = String::with_capacity(inner.len());
    let mut i = 0;
    let mut quote = None;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if let Some(q) = quote {
            out.push(c);
            if c == '\\' && i + 1 < bytes.len() {
                i += 1;
                out.push(bytes[i] as char);
            } else if c == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        if c == '"' || c == '\'' {
            quote = Some(c);
            out.push(c);
            i += 1;
            continue;
        }
        if c.is_ascii_whitespace() {
            let start = i;
            while i < bytes.len() && (bytes[i] as char).is_ascii_whitespace() { i += 1; }
            let next = inner[i..].as_bytes();
            let is_attr_boundary = next.first().is_some_and(|b| b.is_ascii_alphabetic() || *b == b'_')
                && inner[i..].find('=').is_some_and(|eq| eq == 1 || inner[i..].as_bytes().get(1).is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_'));
            if is_attr_boundary {
                out.push(' ');
            } else {
                out.push_str(&inner[start..i]);
            }
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

/// Normalize only layout around an attribute/delimiter pair; quoted payloads
/// and CEL/prose text remain untouched.
fn normalize_delimiters(line: &str) -> String {
    if !line.is_ascii() {
        return line.trim_end_matches([' ', '\t']).to_string();
    }
    let mut out = String::with_capacity(line.len());
    let bytes = line.as_bytes();
    let mut i = 0;
    let mut quote = None;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if let Some(q) = quote {
            out.push(c);
            if c == '\\' && i + 1 < bytes.len() {
                i += 1;
                out.push(bytes[i] as char);
            } else if c == q { quote = None; }
            i += 1;
            continue;
        }
        if c == '"' || c == '\'' { quote = Some(c); out.push(c); i += 1; continue; }
        if c == '{' {
            out.push(c);
            let start = i + 1;
            let mut j = start;
            let mut q = None;
            while j < bytes.len() {
                let d = bytes[j] as char;
                if let Some(qq) = q {
                    if d == '\\' { j += 2; continue; }
                    if d == qq { q = None; }
                } else if d == '"' || d == '\'' { q = Some(d); }
                else if d == '}' { break; }
                j += 1;
            }
            if j < bytes.len() {
                let inner = line[start..j].trim_matches([' ', '\t', '\r']);
                out.push_str(&quote_aware_attr_spacing(inner));
                out.push('}');
                i = j + 1;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out.trim_end_matches([' ', '\t']).to_string()
}
fn canonical_line(line: &str) -> String {
    let trimmed = line.trim_start_matches([' ', '\t']);
    if trimmed.starts_with('@') {
        let bytes = trimmed.as_bytes();
        let mut braces = 0usize;
        let mut quote = None;
        for (i, b) in bytes.iter().enumerate() {
            let c = *b as char;
            if let Some(q) = quote {
                if c == q && (i == 0 || bytes[i - 1] != b'\\') { quote = None; }
                continue;
            }
            if c == '"' || c == '\'' { quote = Some(c); }
            else if c == '{' { braces += 1; }
            else if c == '}' { braces = braces.saturating_sub(1); }
            else if c == ':' && braces == 0 {
                return format!("{}{}", normalize_delimiters(&trimmed[..=i]), &trimmed[i + 1..]);
            }
        }
        trimmed.to_string()
    } else if trimmed.starts_with("::") || trimmed.starts_with('<') {
        normalize_delimiters(trimmed)
    } else {
        trimmed.to_string()
    }
}


fn is_closing(line: &str) -> bool {
    line.trim_start().starts_with("</")
}

fn opens_block(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with('<') && !t.starts_with("</") && !t.ends_with("/>") && t.ends_with('>')
}

fn format_yaml_like(source: &str) -> String {
    let normalized = source.replace("\r\n", "\n").replace('\r', "\n");
    let mut out = String::with_capacity(normalized.len() + 1);
    for (n, line) in normalized.split('\n').enumerate() {
        if n > 0 { out.push('\n'); }
        out.push_str(trim_horizontal(line));
    }
    while out.ends_with("\n\n") { out.pop(); }
    if !out.is_empty() && !out.ends_with('\n') { out.push('\n'); }
    out
}

/// Canonicalize one Lute source document while retaining opaque payload bytes.
/// The parser is consulted only as a validity gate; it is never used as a
/// printer and no AST values are serialized back into source.
pub fn format_source(source: &str, _options: &FormatOptions) -> Result<FormatResult, FormatError> {
    let normalized = source.replace("\r\n", "\n").replace('\r', "\n");
    let has_bom = normalized.starts_with('\u{feff}');
    let parse_source = normalized.strip_prefix('\u{feff}').unwrap_or(&normalized);
    let (_doc, diagnostics) = crate::parse(parse_source);
    let errors: Vec<Diagnostic> = diagnostics.into_iter().filter(|d| d.severity == Severity::Error).collect();
    if !errors.is_empty() { return Err(FormatError::InvalidSource(errors)); }
    let yaml_end = frontmatter_end(parse_source).unwrap_or(0);
    let yaml_only = yaml_end == 0 && likely_yaml(parse_source);
    let mut result = if yaml_only {
        format_yaml_like(parse_source)
    } else {
        let (yaml, body) = parse_source.split_at(yaml_end);
        let mut out = String::with_capacity(parse_source.len() + 1);
        if !yaml.is_empty() {
            // Frontmatter is opaque except for line endings/trailing space and
            // final-newline normalization; blank runs and key order stay put.
            out.push_str(&format_yaml_like(yaml));
            if !out.ends_with('\n') { out.push('\n'); }
        }
        let mut depth = 0usize;
        let mut in_block_comment = false;
        let mut blank_pending = false;
        for raw in body.split('\n') {
            let line = trim_horizontal(raw);
            let trimmed = line.trim_start_matches([' ', '\t']);
            if in_block_comment || trimmed.starts_with("/*") {
                out.push_str(&"  ".repeat(depth));
                out.push_str(trimmed);
                out.push('\n');
                in_block_comment = !trimmed.contains("*/");
                blank_pending = false;
                continue;
            }
            if trimmed.is_empty() {
                blank_pending = true;
                continue;
            }
            if blank_pending {
                if out.is_empty() {
                    out.push('\n');
                } else if !out.ends_with("\n\n") {
                    out.push('\n');
                }
            }
            blank_pending = false;
            if is_closing(trimmed) { depth = depth.saturating_sub(1); }
            let canonical = canonical_line(trimmed);
            out.push_str(&"  ".repeat(depth));
            out.push_str(&canonical);
            out.push('\n');
            if opens_block(trimmed) { depth += 1; }
        }
        while out.ends_with("\n\n") { out.pop(); }
        if !out.is_empty() && !out.ends_with('\n') { out.push('\n'); }
        out
    };

    if has_bom {
        result.insert(0, '\u{feff}');
    }
    Ok(FormatResult { changed: result != source, text: result })
}

/// Format a project/schema/plugin YAML file using the opaque YAML rules.
pub fn format_yaml_source(source: &str) -> FormatResult {
    let text = format_yaml_like(source);
    FormatResult { changed: text != source, text }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source_stream;

    fn concat(stream: &crate::SourceStream<'_>) -> String {
        stream.tokens.iter().map(|t| &stream.source[t.span.byte_start..t.span.byte_end]).collect()
    }

    #[test]
    fn stream_round_trips() {
        for source in ["", "## S\n@a: hello\n", "---\n# c\n---\n\n<match on=\"x\">\n</match>\n", "/* unterminated"] {
            assert_eq!(concat(&source_stream(source)), source);
        }
    }

    #[test]
    fn canonical_rules_and_idempotence() {
        let source = "## S\r\n\r\n\r\n<match on=\"x\">\r\n <when is=\"true\">\r\n @a{ x=\"y\"   z=\"q\" }: hi  \r\n </when>\r\n</match>\r\n";
        let first = format_source(source, &FormatOptions::default()).unwrap().text;
        let second = format_source(&first, &FormatOptions::default()).unwrap().text;
        assert_eq!(first, second);
        assert!(first.ends_with('\n'));
        assert!(!first.contains("\r"));
    }

    #[test]
    fn arbitrary_editor_buffers_never_panic() {
        for seed in 0u32..256 {
            let mut state = seed.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
            let mut source = String::new();
            for _ in 0..64 {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                source.push(match state % 11 {
                    0 => '\n',
                    1 => '\r',
                    2 => '{',
                    3 => '}',
                    4 => '<',
                    5 => '>',
                    6 => '/',
                    7 => '"',
                    8 => ' ',
                    9 => '\t',
                    _ => char::from_u32(0x20 + (state % 0x5f)).unwrap(),
                });
            }
            let stream_ok = std::panic::catch_unwind(|| source_stream(&source)).is_ok();
            let format_ok = std::panic::catch_unwind(|| {
                let _ = format_source(&source, &FormatOptions::default());
            }).is_ok();
            assert!(stream_ok && format_ok, "seed {seed}");
            assert_eq!(concat(&source_stream(&source)), source);
        }
    }

    #[test]
    fn opaque_payloads_and_comments_survive() {
        let source = "## S\n/* keep  spaces\n   and lines */\n@a{when=\"x == 'a'\"}: prose  {opaque}\n::set{run.x = foo + bar}\n";
        let formatted = format_source(source, &FormatOptions::default()).unwrap().text;
        assert!(formatted.contains("/* keep  spaces"));
        assert!(formatted.contains("and lines */"));
        assert!(formatted.contains(": prose  {opaque}"));
        assert!(formatted.contains("when=\"x == 'a'\""));
        assert!(formatted.contains("foo + bar"));
    }

    #[test]
    fn yaml_is_opaque_beyond_layout() {
        let source = "---\r\nkind: scene\r\nflow: {a: 1, b: \"x\"}  \r\n# keep this\r\n---\r\n## S\r\n";
        let formatted = format_source(source, &FormatOptions::default()).unwrap().text;
        assert!(formatted.starts_with("---\nkind: scene\nflow: {a: 1, b: \"x\"}\n# keep this\n---\n"));
        assert_eq!(format_yaml_source("a: {b: 1}  \n").text, "a: {b: 1}\n");
    }
    #[test]
    fn bom_is_preserved_while_parse_view_is_stripped() {
        let source = "\u{feff}## S\r\n@narrator: hi  \r\n";
        let result = format_source(source, &FormatOptions::default()).unwrap();
        assert!(result.text.starts_with('\u{feff}'));
        assert_eq!(result.text, "\u{feff}## S\n@narrator: hi\n");
        assert_eq!(format_source(&result.text, &FormatOptions::default()).unwrap().text, result.text);
    }
    #[test]
    fn corpus_idempotence_and_losslessness() {
        fn walk(dir: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
            let Ok(entries) = std::fs::read_dir(dir) else { return };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() { walk(&path, files); }
                else if path.extension().and_then(|e| e.to_str()) == Some("lute") { files.push(path); }
            }
        }
        let mut files = Vec::new();
        walk(std::path::Path::new("../../docs/examples"), &mut files);
        files.sort();
        for path in files {
            let source = std::fs::read_to_string(&path).expect("example corpus UTF-8");
            assert_eq!(concat(&source_stream(&source)), source, "lossless {}", path.display());
            let first = format_source(&source, &FormatOptions::default())
                .unwrap_or_else(|err| panic!("{}: {err:?}", path.display())).text;
            let second = format_source(&first, &FormatOptions::default())
                .unwrap_or_else(|err| panic!("{} second pass: {err:?}", path.display())).text;
            assert_eq!(first, second, "not idempotent {}", path.display());
        }
    }
}
