//! YAML source text helpers shared by every YAML surface Lute reads — a
//! document's frontmatter, a schema file, `lute.project.yaml`, a plugin's
//! `plugin.yaml` and its export files.
//!
//! `serde_yaml` keeps no per-node positions and words its errors for the
//! library's own reader. These helpers give a diagnostic the two things it
//! needs instead: where a key sits in the text ([`key_span`]), and a plain
//! sentence with a fix for a file that does not parse ([`yaml_fault`]).

use std::ops::Range;

/// One line of `text`: its byte offset, its indentation (a `- ` list marker
/// counts as indentation, so `- on: x` has the key `on` two columns in), and
/// its content after that indentation.
struct Line<'t> {
    start: usize,
    indent: usize,
    body: &'t str,
}

fn lines(text: &str) -> Vec<Line<'_>> {
    let mut out = Vec::new();
    let mut start = 0;
    for raw in text.split_inclusive('\n') {
        let line = raw.trim_end_matches(['\n', '\r']);
        let mut indent = line.len() - line.trim_start_matches(' ').len();
        let mut body = &line[indent..];
        while let Some(rest) = body.strip_prefix("- ") {
            let pad = rest.len() - rest.trim_start_matches(' ').len();
            indent += 2 + pad;
            body = &rest[pad..];
        }
        out.push(Line {
            start,
            indent,
            body,
        });
        start += raw.len();
    }
    out
}

/// Is `body` a content line (not blank, not a comment)?
fn is_content(body: &str) -> bool {
    let t = body.trim();
    !t.is_empty() && !t.starts_with('#')
}

/// The length of the key `key` at the start of `body` — bare, `"key"` or
/// `'key'` — when it is followed by `:` and then a space or the line end.
fn key_len_at(body: &str, key: &str) -> Option<usize> {
    let written = [key.to_string(), format!("\"{key}\""), format!("'{key}'")]
        .into_iter()
        .find(|w| body.starts_with(w.as_str()))?;
    let rest = body[written.len()..].trim_start_matches(' ');
    let after = rest.strip_prefix(':')?;
    (after.is_empty() || after.starts_with([' ', '\t'])).then_some(written.len())
}

/// The offset of the second `key:` of the block mapping whose first line
/// starts at byte `from` (same indentation, before the mapping ends).
fn second_occurrence(text: &str, from: usize, key: &str) -> Option<usize> {
    let all = lines(text);
    let first = all.iter().position(|l| l.start == from)?;
    let indent = all[first].indent;
    all[first..]
        .iter()
        .take_while(|l| !is_content(l.body) || l.indent >= indent)
        .filter(|l| l.indent == indent && key_len_at(l.body, key).is_some())
        .nth(1)
        .map(|l| l.start + l.indent)
}

/// The key `key` inside a flow mapping `{ … }` on one line, word-bounded and
/// followed by `:` — `defualt` in `run.n: { type: number, defualt: 3 }`.
fn flow_key(line: &str, from: usize, key: &str) -> Option<Range<usize>> {
    let bounded = |i: usize| {
        !line[..i]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_ascii_alphanumeric() || "_.-".contains(c))
    };
    line[from..]
        .match_indices(key)
        .map(|(i, _)| from + i)
        .find(|&i| {
            bounded(i)
                && line[i + key.len()..]
                    .trim_start_matches(' ')
                    .starts_with(':')
        })
        .map(|i| i..i + key.len())
}

/// The byte range of the mapping key at `path` (keys from the document's
/// top level down) in YAML `text`, or `None` when the text holds no such
/// key. Block mappings are followed by indentation; a flow mapping
/// (`{ … }`) on one line is searched for the remaining keys. A list item's
/// keys (`- on: x`) are found at the item's indentation — the first item
/// holding the key wins.
pub fn key_span(text: &str, path: &[&str]) -> Option<Range<usize>> {
    let lines = lines(text);
    // The block the next key is searched in: lines [from, to).
    let (mut from, mut to) = (0usize, lines.len());
    for (depth, key) in path.iter().enumerate() {
        let level = lines[from..to]
            .iter()
            .filter(|l| is_content(l.body))
            .map(|l| l.indent)
            .min()?;
        let hit = (from..to).find(|&i| {
            let l = &lines[i];
            l.indent == level && is_content(l.body) && key_len_at(l.body, key).is_some()
        })?;
        let line = &lines[hit];
        let key_start = line.start + line.indent;
        let key_len = key_len_at(line.body, key)?;
        if depth + 1 == path.len() {
            return Some(key_start..key_start + key_len);
        }
        // A flow mapping on the same line holds the rest of the path.
        let raw_line = &text[line.start..line.start + line.indent + line.body.len()];
        let after_key = line.indent + key_len;
        if let Some(brace) = raw_line[after_key..].find('{') {
            let mut at = after_key + brace;
            let mut found = None;
            for key in &path[depth + 1..] {
                let r = flow_key(raw_line, at, key)?;
                at = r.end;
                found = Some(line.start + r.start..line.start + r.end);
            }
            return found;
        }
        let end = (hit + 1..to)
            .find(|&i| is_content(lines[i].body) && lines[i].indent <= line.indent)
            .unwrap_or(to);
        (from, to) = (hit + 1, end);
    }
    None
}

/// The 1-based line and 1-based character column of byte `offset` in `text`.
pub fn line_col(text: &str, offset: usize) -> (usize, usize) {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    let before = &text[..offset];
    let line = before.matches('\n').count() + 1;
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    (line, text[line_start..offset].chars().count() + 1)
}

/// A YAML text that does not parse: where the fault is (a byte offset into
/// the text) and a plain sentence saying what to write instead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct YamlFault {
    pub offset: usize,
    pub message: String,
}

/// Rewrite a `serde_yaml` syntax error of `text` as a [`YamlFault`]: the
/// common slips get the fix (a value holding `: `, a quote inside a quoted
/// value, a quote never closed, `key:value`, a tab in the indentation, a
/// value starting with a reserved character, a key written twice); anything
/// else is the library's problem sentence without its line/column marks
/// (the offset carries the position).
pub fn yaml_fault(text: &str, e: &serde_yaml::Error) -> YamlFault {
    let problem = strip_marks(&e.to_string());
    let Some(loc) = e.location() else {
        return YamlFault {
            offset: 0,
            message: plain_problem(&problem),
        };
    };
    let line_no = loc.line();
    let bad_line = text.lines().nth(line_no.saturating_sub(1)).unwrap_or("");
    let line_start: usize = text
        .split_inclusive('\n')
        .take(line_no.saturating_sub(1))
        .map(str::len)
        .sum();
    let mut offset = loc.index().min(text.len());
    let indent = &bad_line[..bad_line.len() - bad_line.trim_start().len()];
    let hint = if indent.contains('\t') {
        offset = line_start;
        Some(
            "YAML indents with spaces, not tabs: replace the tab at the start of the line with \
             spaces"
                .to_string(),
        )
    } else if let Some(hint) = nested_quote_hint(bad_line) {
        Some(hint)
    } else if let Some((at, hint)) = second_key(bad_line) {
        offset = line_start + at;
        Some(hint)
    } else if let Some((at, hint)) = earlier_line_fault(text, line_no) {
        // The error surfaced on a later line; the anchor is the slip.
        offset = at;
        Some(hint)
    } else if problem.contains("mapping values are not allowed") {
        let indented = || over_indented(text, line_no).map(|(at, hint)| (at - line_start, hint));
        colon_in_value(bad_line)
            .or_else(indented)
            .map(|(at, hint)| {
                offset = line_start + at;
                hint
            })
    } else if problem.contains("cannot start any token") {
        reserved_start(bad_line).map(|(at, hint)| {
            offset = line_start + at;
            hint
        })
    } else if let Some(key) = duplicate_key(&problem) {
        // The error sits at the mapping; the second `key:` is the one to fix.
        if let Some(at) = second_occurrence(text, line_start, key) {
            offset = at;
        }
        Some(format!(
            "`{key}:` is written twice in one mapping — keep one of them, or rename the second \
             to the key you meant"
        ))
    } else {
        None
    };
    YamlFault {
        offset,
        message: hint.unwrap_or_else(|| plain_problem(&problem)),
    }
}

/// The library's problem sentence without any ` at line N column M` mark.
fn strip_marks(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find(" at line ") {
        out.push_str(&rest[..i]);
        let tail = &rest[i + " at line ".len()..];
        let skip = tail
            .find(|c: char| !(c.is_ascii_digit() || c == ' ' || "column".contains(c)))
            .unwrap_or(tail.len());
        rest = &tail[skip..];
    }
    out.push_str(rest);
    out.trim().trim_end_matches(',').to_string()
}

/// The library's problem, in words an author can act on where the cause is
/// known; otherwise its own sentence, marked as YAML's.
fn plain_problem(problem: &str) -> String {
    let known: &[(&str, &str)] = &[
        (
            "did not find expected key",
            "a line here is indented differently from the keys around it — line it up with the \
             key it belongs beside",
        ),
        (
            "block sequence entries are not allowed",
            "a `- ` list item stands where a value was expected — put the list on the lines \
             below its key, indented",
        ),
        (
            "did not find expected ',' or ']'",
            "a `[…]` list is not closed, or two of its items are not separated by `,`",
        ),
        (
            "did not find expected ',' or '}'",
            "a `{…}` mapping is not closed, or two of its entries are not separated by `,`",
        ),
        (
            "found unexpected end of stream",
            "the text ends inside an unclosed quote, `[` or `{`",
        ),
        (
            "did not find expected node content",
            "a value is missing here — a `[` or `{` never closed, or a `,` with no item after it",
        ),
        (
            "could not find expected ':'",
            "a key is missing its `:` (write `key: value`)",
        ),
        (
            "mapping values are not allowed",
            "a value holding `: ` must be quoted — `key: \"a: b\"`",
        ),
    ];
    known
        .iter()
        .find(|(needle, _)| problem.contains(needle))
        .map_or_else(
            || format!("the YAML does not parse: {problem}"),
            |(_, m)| m.to_string(),
        )
}

/// `duplicate entry with key "x"` → `x`.
fn duplicate_key(problem: &str) -> Option<&str> {
    let rest = problem.split("duplicate entry with key ").nth(1)?;
    Some(rest.trim().trim_matches('"'))
}

/// `key: "value" other: x` — a second key written on the line of a quoted
/// value, meant as the next line. The offset of the second key and the fix.
fn second_key(line: &str) -> Option<(usize, String)> {
    let colon = line.find(": ")?;
    let key = line[..colon].trim().trim_start_matches("- ");
    let after = &line[colon + 2..];
    let at = colon + 2 + (after.len() - after.trim_start().len());
    let value = &line[at..];
    let quote = value.chars().next().filter(|c| matches!(c, '"' | '\''))?;
    let b = value.as_bytes();
    let close =
        (1..b.len()).find(|&i| b[i] == quote as u8 && (quote == '\'' || b[i - 1] != b'\\'))?;
    let rest = &value[close + 1..];
    let next = at + close + 1 + (rest.len() - rest.trim_start().len());
    let tail = line[next..].trim_end();
    let name_end = tail
        .find(|c: char| !(c.is_ascii_alphanumeric() || "_.-".contains(c)))
        .unwrap_or(tail.len());
    let second = &tail[..name_end];
    let after_name = &tail[name_end..];
    (!second.is_empty() && (after_name == ":" || after_name.starts_with(": "))).then(|| {
        (
            next,
            format!("`{second}:` starts a new key on `{key}:`'s line — put `{tail}` on a line of its own"),
        )
    })
}

/// A key indented under a key that already has a value (`title: D` then
/// `  on: evening`): YAML reads it as part of that value. The 1-based line
/// `line_no` of `text`; the offset of the key and the fix.
fn over_indented(text: &str, line_no: usize) -> Option<(usize, String)> {
    let mut offset = 0;
    let mut above: Option<&str> = None;
    for (i, line) in text.split_inclusive('\n').enumerate() {
        let body = line.trim_end_matches(['\n', '\r']);
        if i + 1 == line_no {
            let indent = body.len() - body.trim_start().len();
            let key = body
                .trim_start()
                .split(": ")
                .next()
                .filter(|k| !k.is_empty() && !k.contains(' '))?;
            let parent = above?;
            let parent_indent = parent.len() - parent.trim_start().len();
            let (pkey, pvalue) = parent.trim_start().split_once(": ")?;
            return (indent > parent_indent && !pvalue.trim().is_empty()).then(|| {
                (
                    offset + indent,
                    format!(
                        "`{key}:` is indented under `{pkey}:`, which already has a value — start \
                         `{key}:` in the same column as `{pkey}:`"
                    ),
                )
            });
        }
        if is_content(body) {
            above = Some(body);
        }
        offset += line.len();
    }
    None
}

/// `key: a plain value: with a colon` — YAML reads the second `: ` as a
/// nested mapping. The offset of the value and the quoted fix. A
/// `kind: crew` value names a kind ref, whose spelling has no space.
fn colon_in_value(line: &str) -> Option<(usize, String)> {
    let colon = line.find(": ")?;
    let key = line[..colon].trim().trim_start_matches("- ");
    let after = &line[colon + 2..];
    let at = colon + 2 + (after.len() - after.trim_start().len());
    let value = line[at..].trim_end();
    if !value.contains(": ") || value.starts_with(['"', '\'', '{', '[']) {
        return None;
    }
    if let Some(kind) = value.strip_prefix("kind: ") {
        let kind = kind.trim();
        return Some((
            at,
            format!(
                "a `kind:` reference is one word with no space after its colon, and quoted — \
                 `{key}: \"kind:{kind}\"`"
            ),
        ));
    }
    Some((
        at,
        format!(
            "a value containing `: ` must be quoted — `{key}: \"{}\"`",
            value.replace('"', "'")
        ),
    ))
}

/// A plain value starting with a character YAML reserves (`` ` ``, `@`,
/// `%`): the offset of the value and the quoted fix.
fn reserved_start(line: &str) -> Option<(usize, String)> {
    let colon = line.find(": ")?;
    let key = line[..colon].trim().trim_start_matches("- ");
    let after = &line[colon + 2..];
    let at = colon + 2 + (after.len() - after.trim_start().len());
    let value = line[at..].trim_end();
    let first = value
        .chars()
        .next()
        .filter(|c| matches!(c, '`' | '@' | '%'))?;
    Some((
        at,
        format!(
            "a value starting with `{first}` must be quoted — `{key}: \"{}\"`",
            value.replace('"', "'")
        ),
    ))
}

/// The fix for a quoted value that holds its own quote character, which YAML
/// reads as the end of the value. `key: "a "b" c"` → use single quotes
/// inside (`key: "a 'b' c"`); `key: 'a 'b' c'` → quote the value with `"`
/// instead (`key: "a 'b' c"`). `None` when the line's value is not such a
/// quoted scalar.
fn nested_quote_hint(line: &str) -> Option<String> {
    let colon = line.find(": ")?;
    let key = line[..colon].trim();
    let value = line[colon + 1..].trim();
    if let Some(inner) = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')) {
        let b = inner.as_bytes();
        let nested = (0..b.len()).any(|i| b[i] == b'"' && (i == 0 || b[i - 1] != b'\\'));
        return nested.then(|| {
            format!(
                "a `\"` inside a double-quoted value ends the value early: use single quotes \
                 inside a double-quoted value — `{key}: \"{}\"`",
                inner.replace('"', "'")
            )
        });
    }
    let inner = value.strip_prefix('\'')?.strip_suffix('\'')?;
    (inner.contains('\'') && !inner.contains('"')).then(|| {
        format!(
            "a `'` inside a single-quoted value ends the value early: quote the value with \
             double quotes instead — `{key}: \"{inner}\"`"
        )
    })
}

/// FS-F15: the two slips whose YAML error surfaces on a LATER line — a
/// quoted value not closed on its own line (the quote swallows the next
/// lines), and `key:value` with no space after the colon (a plain scalar,
/// so the mapping breaks on the next line). The nearest such line at or
/// before the error's 1-based line `upto`: the offset of the opening quote
/// / the colon, and the fix.
fn earlier_line_fault(text: &str, upto: usize) -> Option<(usize, String)> {
    let mut offset = 0;
    let mut found = None;
    for line in text.split_inclusive('\n').take(upto) {
        let body = line.trim_end_matches(['\n', '\r']);
        if let Some((at, hint)) = unclosed_quote(body).or_else(|| missing_colon_space(body)) {
            found = Some((offset + at, hint));
        }
        offset += line.len();
    }
    found
}

/// `key: "value` with no closing quote on the line.
fn unclosed_quote(line: &str) -> Option<(usize, String)> {
    let colon = line.find(": ")?;
    let key = line[..colon].trim().trim_start_matches("- ");
    let after = &line[colon + 2..];
    let at = colon + 2 + (after.len() - after.trim_start().len());
    let value = &line[at..];
    let quote = value.chars().next().filter(|c| matches!(c, '"' | '\''))?;
    let rest = value[1..].as_bytes();
    let closed = if quote == '"' {
        (0..rest.len()).any(|i| rest[i] == b'"' && (i == 0 || rest[i - 1] != b'\\'))
    } else {
        value[1..].replace("''", "").contains('\'')
    };
    (!closed).then(|| {
        (
            at,
            format!(
                "the `{quote}` that opens `{key}:`'s value is never closed — end the value with \
                 `{quote}` on the same line"
            ),
        )
    })
}

/// `key:value` — a mapping key with no space after its colon.
fn missing_colon_space(line: &str) -> Option<(usize, String)> {
    let t = line.trim_start();
    if t.starts_with(['#', '-']) || line.contains(": ") {
        return None;
    }
    let colon = t.find(':')?;
    let key = &t[..colon];
    let value = t[colon + 1..].trim_end();
    let is_key = !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c));
    (is_key && !value.is_empty() && !value.starts_with(char::is_whitespace)).then(|| {
        (
            line.len() - t.len() + colon,
            format!("a key needs a space after its colon — `{key}: {value}`"),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at<'t>(text: &'t str, path: &[&str]) -> Option<(&'t str, (usize, usize))> {
        key_span(text, path).map(|r| (&text[r.clone()], line_col(text, r.start)))
    }

    #[test]
    fn key_span_follows_indentation_and_flow_mappings() {
        let text = "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n\
                    state:\n  run.n: { type: number, defualt: 3 }\n  run.m:\n    type: bool\n    ownr: engine\n";
        assert_eq!(
            at(text, &["profiles", "core", "plugins"]),
            Some(("plugins", (4, 5)))
        );
        assert_eq!(
            at(text, &["state", "run.n", "defualt"]),
            Some(("defualt", (6, 26)))
        );
        assert_eq!(
            at(text, &["state", "run.m", "ownr"]),
            Some(("ownr", (9, 5)))
        );
        // A key of a deeper block is not a top-level key.
        assert_eq!(at(text, &["plugins"]), None);
        assert_eq!(at(text, &["state", "run.q"]), None);
    }

    #[test]
    fn colon_in_a_plain_value_gets_the_quoted_fix() {
        for (text, fix) in [
            (
                "kind: scene\ntitle: Tea at Hollin Street: Arrival\n",
                "`title: \"Tea at Hollin Street: Arrival\"`",
            ),
            ("on: talk\ntarget: kind: crew\n", "`target: \"kind:crew\"`"),
        ] {
            let e = serde_yaml::from_str::<serde_yaml::Value>(text).unwrap_err();
            let f = yaml_fault(text, &e);
            assert!(f.message.contains(fix), "{}", f.message);
            assert!(!f.message.contains("mapping values"), "{}", f.message);
            assert_eq!(line_col(text, f.offset).0, 2, "{f:?}");
        }
    }

    // Each slip gets its own fix at its own place, not a neighbour's hint:
    // two keys on one line are no indentation problem, an over-indented key
    // holds no `: ` to quote, and a duplicate is fixed at its second copy.
    #[test]
    fn each_slip_is_named_where_it_is() {
        for (text, fix, pos) in [
            (
                "id: c\nwhen: \"run.n == 1\" priority: 3\n",
                "put `priority: 3` on a line of its own",
                (2, 20),
            ),
            (
                "id: d\ntitle: D\n  on: evening\n",
                "`on:` is indented under `title:`",
                (3, 3),
            ),
            (
                "clock:\n  day: run.day\n  slots: [a]\n  day: 1\n",
                "`day:` is written twice",
                (4, 3),
            ),
        ] {
            let e = serde_yaml::from_str::<serde_yaml::Value>(text).unwrap_err();
            let f = yaml_fault(text, &e);
            assert!(f.message.contains(fix), "{}", f.message);
            assert_eq!(line_col(text, f.offset), pos, "{f:?}");
        }
    }
}
