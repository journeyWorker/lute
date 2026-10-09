//! Top-level CEL condition splitting.

/// `s` split at its top-level `op` (`&&` / `||`, outside parentheses,
/// brackets, braces and quotes).
pub fn top_level_split<'s>(s: &'s str, op: &[u8; 2]) -> Vec<&'s str> {
    let b = s.as_bytes();
    let (mut depth, mut quote, mut start, mut i) = (0i32, None::<u8>, 0, 0);
    let mut out = Vec::new();
    while i < b.len() {
        let c = b[i];
        match quote {
            Some(_) if c == b'\\' => i += 1,
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None => match c {
                b'"' | b'\'' => quote = Some(c),
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => depth -= 1,
                _ if depth == 0 && b[i..].starts_with(op) => {
                    out.push(&s[start..i]);
                    start = i + 2;
                    i += 1;
                }
                _ => {}
            },
        }
        i += 1;
    }
    out.push(&s[start..]);
    out
}

/// The top-level `&&` conjuncts of `s` — `s` whole when a top-level `||`
/// or `?` binds looser than `&&` and splitting would change its meaning.
pub fn top_level_and(s: &str) -> Vec<&str> {
    if top_level_split(s, b"||").len() > 1 || s.contains('?') {
        return vec![s];
    }
    top_level_split(s, b"&&")
}

/// `s` trimmed, without the parentheses that wrap all of it.
pub fn unparen(s: &str) -> &str {
    let mut s = s.trim();
    while let Some(inner) = s.strip_prefix('(').and_then(|r| r.strip_suffix(')')) {
        if !balanced(inner) {
            break;
        }
        s = inner.trim();
    }
    s
}

/// Every `(` in `s` closes before it ends, and no `)` closes early.
fn balanced(s: &str) -> bool {
    let mut depth = 0i32;
    for c in s.bytes() {
        match c {
            b'(' => depth += 1,
            b')' => depth -= 1,
            _ => {}
        }
        if depth < 0 {
            return false;
        }
    }
    depth == 0
}
