//! State paths in CEL: one name, two spellings.
//!
//! A name that is an identifier may follow a `.`; any name may be written as
//! a quoted index, as in JavaScript: `quest["zero-coke-001"].state`,
//! `run.visits['lab-b2']`. Both spellings are the same path. Every consumer
//! that reads a path out of a condition goes through [`static_path`], and
//! every place that shows one to an author spells it with
//! [`bracket_spelling`]; the canonical dotted form ([`render_path`]) is what
//! schemas, the IR and the runtime key on — a segment may carry `-` but
//! never `.`, so `split('.')` recovers it.

use cel_parser::ast::{operators, Expr};
use cel_parser::reference::Val;

pub use lute_syntax::path::{
    bracket_spelling, bracket_spelling_of, glued_message, parse_path as parse_path_text,
    read_index, read_quoted, render_path, scan_path,
};

/// The segments of a static path expression: an `Ident`, then `.field`
/// selections and string-literal indexes (`a.b["c-d"].e` → `[a, b, c-d,
/// e]`). A `has()` test select counts like any other. `None` for anything
/// else — a call, a computed index, a literal operand.
pub fn static_path(expr: &Expr) -> Option<Vec<String>> {
    match expr {
        Expr::Ident(name) => Some(vec![name.clone()]),
        Expr::Select(sel) => {
            let mut segs = static_path(&sel.operand.expr)?;
            segs.push(sel.field.clone());
            Some(segs)
        }
        Expr::Call(call)
            if call.func_name == operators::INDEX
                && call.target.is_none()
                && call.args.len() == 2 =>
        {
            let Expr::Literal(Val::String(key)) = &call.args[1].expr else {
                return None;
            };
            let mut segs = static_path(&call.args[0].expr)?;
            segs.push(key.clone());
            Some(segs)
        }
        _ => None,
    }
}

/// [`static_path`] rendered canonically ([`render_path`]).
pub fn static_path_string(expr: &Expr) -> Option<String> {
    static_path(expr).map(|segs| render_path(&segs))
}

/// A name written as a fact argument or a match arm: a bare identifier
/// (`labB2`) or a string literal (`"lab-b2"`) — the same name either way.
/// `None` for anything else.
pub fn atom_arg(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Ident(name) => Some(name.clone()),
        Expr::Literal(Val::String(s)) => Some(s.clone()),
        _ => None,
    }
}

/// A path written with a name that is not an identifier after a `.` — which
/// CEL reads as a subtraction (`quest.zero-coke-001.state`) or refuses
/// (`run.visits.001`). `start..end` is its byte range in the scanned text;
/// `name` the first such name; `segs` the path the author meant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GluedPath {
    pub start: usize,
    pub end: usize,
    pub name: String,
    pub segs: Vec<String>,
}

impl GluedPath {
    /// The spelling a condition takes: [`bracket_spelling`] of the path.
    pub fn bracket(&self) -> String {
        bracket_spelling(&self.segs)
    }

    /// The [`glued_message`] for this path in the text it was found in.
    pub fn message(&self, raw: &str) -> String {
        glued_message(&raw[self.start..self.end], &self.name, &self.segs)
    }
}

/// Every [`GluedPath`] in `raw` whose root `is_root` accepts, outside string
/// literals. A `-` between two parts of a segment glues them into one name
/// unless every part after the first is a number or itself a root
/// (`run.hp-1`, `run.a-scene.b` are subtractions); a segment after a `.`
/// that opens with a digit is always a glued name.
pub fn glued_paths(raw: &str, is_root: impl Fn(&str) -> bool) -> Vec<GluedPath> {
    let mask = lute_manifest::text::cel_string_mask(raw);
    let b = raw.as_bytes();
    let continues =
        |c: u8| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'.' | b'@' | b'$' | b'-');
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let at_start = !mask[i]
            && (b[i].is_ascii_alphabetic() || b[i] == b'_')
            && (i == 0 || !continues(b[i - 1]));
        if !at_start {
            i += 1;
            continue;
        }
        let root_len = b[i..]
            .iter()
            .take_while(|c| c.is_ascii_alphanumeric() || **c == b'_')
            .count();
        let root = &raw[i..i + root_len];
        if !is_root(root) {
            i += root_len;
            continue;
        }
        let (glued, end) = glue_after_root(raw, i, root_len, &is_root);
        if let Some((name, segs)) = glued {
            out.push(GluedPath {
                start: i,
                end,
                name,
                segs,
            });
        }
        i = end.max(i + root_len);
    }
    out
}

/// Scan the segments after a root at `start`: the path the author meant and
/// its end when a segment is glued, else `None` and where scanning stopped.
fn glue_after_root(
    raw: &str,
    start: usize,
    root_len: usize,
    is_root: &impl Fn(&str) -> bool,
) -> (Option<(String, Vec<String>)>, usize) {
    let b = raw.as_bytes();
    let mut segs = vec![raw[start..start + root_len].to_string()];
    let mut i = start + root_len;
    let mut glued: Option<String> = None;
    loop {
        if b.get(i) == Some(&b'.') {
            let len = b[i + 1..]
                .iter()
                .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-'))
                .count();
            if len == 0 {
                break;
            }
            let seg = raw[i + 1..i + 1 + len].trim_end_matches('-');
            let parts: Vec<&str> = seg.split('-').collect();
            let later_named = parts[1..]
                .iter()
                .any(|p| !p.is_empty() && !p.bytes().all(|c| c.is_ascii_digit()) && !is_root(p));
            if seg.starts_with(|c: char| c.is_ascii_digit()) || (parts.len() > 1 && later_named) {
                glued.get_or_insert_with(|| seg.to_string());
                segs.push(seg.to_string());
                i += 1 + seg.len();
            } else {
                // A subtraction (`run.hp-1`) ends the path at its first part.
                segs.push(parts[0].to_string());
                i += 1 + parts[0].len();
                if parts.len() > 1 {
                    break;
                }
            }
        } else if let Some((name, len)) = read_index(&raw[i..]) {
            segs.push(name);
            i += len;
        } else {
            break;
        }
    }
    (glued.map(|name| (name, segs)), i)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(raw: &str) -> Expr {
        let mut arena = crate::CelArena::default();
        let h = crate::parse_slot(&mut arena, raw, 0).expect(raw);
        arena.get(h).unwrap().expr.clone()
    }

    #[test]
    fn bare_and_quoted_spellings_are_one_path() {
        for (raw, want) in [
            ("quest.zeroCoke001.state", "quest.zeroCoke001.state"),
            (
                "quest[\"zero-coke-001\"].state",
                "quest.zero-coke-001.state",
            ),
            ("run.visits['lab-b2']", "run.visits.lab-b2"),
            ("run.visits[\"labB2\"]", "run.visits.labB2"),
            ("scene.choices[\"go-left\"]", "scene.choices.go-left"),
        ] {
            assert_eq!(
                static_path_string(&parse(raw)).as_deref(),
                Some(want),
                "{raw}"
            );
        }
        for raw in ["run.visits[run.k]", "run.xs[0]", "f(x).y", "'a'"] {
            assert_eq!(static_path(&parse(raw)), None, "{raw}");
        }
    }

    #[test]
    fn bracket_spelling_quotes_only_non_identifiers() {
        for (path, want) in [
            (
                "quest.zero-coke-001.state",
                "quest[\"zero-coke-001\"].state",
            ),
            ("run.visits.labB2", "run.visits.labB2"),
            ("run.visits.001", "run.visits[\"001\"]"),
            ("run._x", "run._x"),
        ] {
            assert_eq!(bracket_spelling_of(path), want);
        }
    }

    #[test]
    fn glued_names_are_found_and_subtractions_are_not() {
        let roots = |r: &str| matches!(r, "quest" | "run" | "scene");
        for (raw, want) in [
            (
                "quest.zero-coke-001.state == 'active'",
                Some("quest[\"zero-coke-001\"].state"),
            ),
            ("run.visits.lab-b2 > 0", Some("run.visits[\"lab-b2\"]")),
            ("run.visits.001 > 0", Some("run.visits[\"001\"]")),
            ("run.hp-1 > 0", None),
            ("run.a-scene.b > 0", None),
            ("run.visits[\"lab-b2\"] > 0", None),
            ("'run.a-b' == x", None),
            ("npc.a-b > 0", None),
        ] {
            let got = glued_paths(raw, roots);
            assert_eq!(
                got.first().map(GluedPath::bracket).as_deref(),
                want,
                "{raw}"
            );
            assert!(got.len() <= 1, "{raw}: {got:?}");
        }
    }
}
