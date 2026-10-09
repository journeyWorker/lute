//! dsl 0.27.0 §3: `occasion.target` / `occasion.payload` occurrences.

use crate::semantics::beats::OCCASION_TARGET;

/// The calls whose first argument is a fact pattern (dsl 0.3.0 §5).
const FACT_QUERIES: &[&str] = &["holds", "count", "countDistinct", "validAt"];

/// Where one `occasion.target` occurrence sits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Position {
    /// A fact-query pattern argument: `holds('owned', [occasion.target])`.
    PatternArg,
    /// A family index: `user.bond[occasion.target]`; `range` spans the
    /// brackets.
    Index,
    /// Anything else: an ordinary read of the bound value.
    Value,
}

/// One `occasion.target` occurrence in a CEL text: the byte range it
/// replaces (brackets included for [`Position::Index`]) and its position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Occurrence {
    pub range: (usize, usize),
    pub position: Position,
}

fn ident(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// Every `occasion.target` in `cel` outside string literals, in order.
pub fn occurrences(cel: &str) -> Vec<Occurrence> {
    let mask = crate::text::cel_string_mask(cel);
    let in_string = |i: usize| mask.get(i).copied().unwrap_or(false);
    let b = cel.as_bytes();
    let mut stack: Vec<&str> = Vec::new();
    let mut list_depth = 0usize;
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if in_string(i) {
            i += 1;
            continue;
        }
        match b[i] {
            b'[' => {
                list_depth += 1;
                i += 1;
                continue;
            }
            b']' => {
                list_depth = list_depth.saturating_sub(1);
                i += 1;
                continue;
            }
            b'(' => {
                let mut s = i;
                while s > 0 && b[s - 1].is_ascii_whitespace() {
                    s -= 1;
                }
                let end = s;
                while s > 0 && ident(b[s - 1]) {
                    s -= 1;
                }
                let dotted = s > 0 && b[s - 1] == b'.';
                stack.push(if dotted { "" } else { &cel[s..end] });
                i += 1;
                continue;
            }
            b')' => {
                stack.pop();
                i += 1;
                continue;
            }
            _ => {}
        }
        // Compare bytes, not a `str` slice: `i` walks bytes, and a
        // multi-byte char outside a literal (`≥`, `‘`, Hangul) puts it
        // inside a char (FS-F1).
        if b[i..].starts_with(OCCASION_TARGET.as_bytes())
            && (i == 0 || !(ident(b[i - 1]) || b[i - 1] == b'.' || b[i - 1] == b'@'))
            && b.get(i + OCCASION_TARGET.len())
                .is_none_or(|&c| !(ident(c) || c == b'.'))
        {
            let end = i + OCCASION_TARGET.len();
            let mut before = i;
            while before > 0 && b[before - 1].is_ascii_whitespace() {
                before -= 1;
            }
            let mut after = end;
            while after < b.len() && b[after].is_ascii_whitespace() {
                after += 1;
            }
            let bracketed = before > 0 && b[before - 1] == b'[' && b.get(after) == Some(&b']');
            // `x[occasion.target]` after a path (not a list literal).
            let indexed =
                bracketed && before >= 2 && (ident(b[before - 2]) || b[before - 2] == b']');
            let occurrence = if indexed {
                Occurrence {
                    range: (before - 1, after + 1),
                    position: Position::Index,
                }
            } else {
                let arg_of = stack.last().copied().unwrap_or("");
                let outer = stack.len().checked_sub(2).map_or("", |k| stack[k]);
                let pattern = (list_depth > 0 && FACT_QUERIES.contains(&arg_of))
                    || (!arg_of.is_empty()
                        && !FACT_QUERIES.contains(&arg_of)
                        && FACT_QUERIES.contains(&outer));
                Occurrence {
                    range: (i, end),
                    position: if pattern {
                        Position::PatternArg
                    } else {
                        Position::Value
                    },
                }
            };
            i = occurrence.range.1.max(end);
            out.push(occurrence);
            continue;
        }
        i += 1;
    }
    out
}

/// Whether `cel` reads `occasion.target` at all.
pub fn mentions_target(cel: &str) -> bool {
    !occurrences(cel).is_empty()
}

/// The payload path prefix (dsl 0.27.0 §3): `occasion.payload.<field>`.
pub const OCCASION_PAYLOAD: &str = "occasion.payload";
