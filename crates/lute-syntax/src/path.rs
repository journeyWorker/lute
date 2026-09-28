//! State paths as an author writes them outside a CEL parse: a `::set`
//! target, a `{{…}}` interpolation, a `state:` key, a `--state` input.
//!
//! A path is a root name, then segments: `.name` or an index with a quoted
//! name, `["lab-b2"]` / `['lab-b2']` — the two spellings of one segment, as
//! in JavaScript. Every reader returns the segments with the quotes removed,
//! so `run.visits["lab-b2"]` and `run.visits.labB2` differ only in the name.

/// A quoted name at the start of `text` — `"…"` or `'…'`, `\` escaping the
/// next character — and the bytes it spans, quotes included. `None` when
/// `text` does not open with a quote or the quote is never closed.
pub fn read_quoted(text: &str) -> Option<(String, usize)> {
    let b = text.as_bytes();
    let quote = *b.first().filter(|&&q| q == b'"' || q == b'\'')?;
    let mut out = String::new();
    let mut chars = text[1..].char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '\\' => out.push(chars.next()?.1),
            c if c as u32 == quote as u32 => return Some((out, i + 2)),
            c => out.push(c),
        }
    }
    None
}

/// A `["name"]` / `['name']` index at the start of `text`: the name and the
/// bytes the index spans, brackets included. Whitespace inside the brackets
/// is allowed, as CEL allows it.
pub fn read_index(text: &str) -> Option<(String, usize)> {
    let rest = text.strip_prefix('[')?;
    let lead = rest.len() - rest.trim_start().len();
    let (name, len) = read_quoted(&rest[lead..])?;
    let after = &rest[lead + len..];
    let trail = after.len() - after.trim_start().len();
    after[trail..]
        .starts_with(']')
        .then(|| (name, 1 + lead + len + trail + 1))
}

/// `true` for a byte a bare name segment may carry: a letter, digit, `_` or
/// `-`. Whether the segment may be written bare is the caller's to judge.
fn is_name_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}

/// The static path at the start of `text` — a root name, then `.name` or
/// quoted-index segments — and the bytes it spans. Stops at the first byte
/// that continues no segment; an empty root yields no segments.
pub fn scan_path(text: &str) -> (Vec<String>, usize) {
    let b = text.as_bytes();
    let bare = |from: usize| from + b[from..].iter().take_while(|&&c| is_name_byte(c)).count();
    let root_end = bare(0);
    if root_end == 0 {
        return (Vec::new(), 0);
    }
    let mut segs = vec![text[..root_end].to_string()];
    let mut i = root_end;
    loop {
        if b.get(i) == Some(&b'.') {
            let end = bare(i + 1);
            if end == i + 1 {
                break;
            }
            segs.push(text[i + 1..end].to_string());
            i = end;
        } else if let Some((name, len)) = read_index(&text[i..]) {
            segs.push(name);
            i += len;
        } else {
            break;
        }
    }
    (segs, i)
}

/// The segments of `text` when the whole of it (surrounding whitespace
/// aside) is one static path; `None` otherwise.
pub fn parse_path(text: &str) -> Option<Vec<String>> {
    let text = text.trim();
    let (segs, len) = scan_path(text);
    (!segs.is_empty() && len == text.len()).then_some(segs)
}

/// The canonical dotted form of a path: its segments joined by `.`.
pub fn render_path<S: AsRef<str>>(segs: &[S]) -> String {
    let mut out = String::new();
    for (i, seg) in segs.iter().enumerate() {
        if i > 0 {
            out.push('.');
        }
        out.push_str(seg.as_ref());
    }
    out
}

/// How an author writes the path in a condition: the root bare, then each
/// segment after a `.` when it is an identifier, else as a double-quoted
/// index (`quest["zero-coke-001"].state`).
pub fn bracket_spelling<S: AsRef<str>>(segs: &[S]) -> String {
    let mut out = String::new();
    for (i, seg) in segs.iter().enumerate() {
        let seg = seg.as_ref();
        if i == 0 {
            out.push_str(seg);
        } else if lute_manifest::ident::is_ident(seg) {
            out.push('.');
            out.push_str(seg);
        } else {
            out.push_str("[\"");
            out.push_str(seg);
            out.push_str("\"]");
        }
    }
    out
}

/// [`bracket_spelling`] of a canonical dotted path.
pub fn bracket_spelling_of(path: &str) -> String {
    bracket_spelling(&path.split('.').collect::<Vec<_>>())
}

/// The one message for a name that is not an identifier written after a
/// `.`: `written` as the author spelled it, `name` the first such segment,
/// `segs` the path meant.
pub fn glued_message<S: AsRef<str>>(written: &str, name: &str, segs: &[S]) -> String {
    let why = if name.contains('-') {
        "CEL reads its `-` as subtraction"
    } else {
        "CEL cannot read it after a `.`"
    };
    format!(
        "`{written}`: `{name}` is not an identifier, so it cannot follow a `.` — {why}; write \
         `{}`",
        bracket_spelling(segs)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_and_quoted_segments_read_the_same_names() {
        for (text, want) in [
            ("run.visits.labB2", Some(vec!["run", "visits", "labB2"])),
            (
                "run.visits[\"lab-b2\"]",
                Some(vec!["run", "visits", "lab-b2"]),
            ),
            (
                "run.visits['lab-b2']",
                Some(vec!["run", "visits", "lab-b2"]),
            ),
            (
                "quest[\"zero-coke-001\"].state",
                Some(vec!["quest", "zero-coke-001", "state"]),
            ),
            ("run.a[ 'x' ].b", Some(vec!["run", "a", "x", "b"])),
            ("run.a-b", Some(vec!["run", "a-b"])),
            ("run.a[b]", None),
            ("run.a[\"b\"", None),
            ("run.a + 1", None),
            ("", None),
        ] {
            let got = parse_path(text);
            let want = want.map(|w| w.into_iter().map(String::from).collect::<Vec<_>>());
            assert_eq!(got, want, "{text}");
        }
    }

    #[test]
    fn scan_stops_at_the_first_non_path_byte() {
        let (segs, len) = scan_path("run.x[\"a\"] += 1");
        assert_eq!(segs, ["run", "x", "a"]);
        assert_eq!(len, "run.x[\"a\"]".len());
    }
}
