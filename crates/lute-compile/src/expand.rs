//! D4: compile-time `@ref`/`@fn(args)`/`$` → inline-CEL expansion.
//!
//! A def ref is a compile-time macro (dsl §8.1). Each substituted body is
//! PARENTHESIZED; `@fn(args)` binds args positionally (arity/type already
//! gate-proven by the checker); expansion recurses with a cycle guard; `$`
//! substitutes the enclosing `<match>` subject. The artifact carries no defs
//! table — output CEL is `@`/`$`-free.

use std::collections::BTreeMap;

use lute_cel::{cel_string_mask, scan_refs};
use lute_core_span::{Diagnostic, Layer, Severity};
use lute_manifest::types::Type;
use lute_syntax::ast::{Arm, Attr, AttrValue, CelSlot, ClipNode, Document, Node};

/// The merged def table (plugin < imported < inline), borrowed from
/// `lute_check::FoldedEnv { def_bodies, env.def_params }`.
pub struct DefTable<'a> {
    pub bodies: &'a BTreeMap<String, String>,
    pub params: &'a BTreeMap<String, Vec<(String, Type)>>,
}

/// Expand every CEL slot in the document in place. Returns diagnostics for
/// expander failures (`E-COMPILE-EXPAND`: cycle / unknown def / arity — the
/// latter two gate-proven unreachable, kept total). Never panics.
pub fn expand_document(doc: &mut Document, defs: &DefTable<'_>) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for shot in &mut doc.shots {
        expand_nodes(&mut shot.body, defs, None, &mut diags);
    }
    diags
}

fn expand_nodes(
    nodes: &mut [Node],
    defs: &DefTable<'_>,
    subject: Option<&str>,
    diags: &mut Vec<Diagnostic>,
) {
    for node in nodes {
        match node {
            Node::Line(l) => expand_attrs(&mut l.attrs, defs, subject, diags),
            Node::Directive(d) => expand_attrs(&mut d.attrs, defs, subject, diags),
            Node::Set(s) => expand_slot(&mut s.expr, defs, subject, diags),
            Node::Branch(b) => {
                expand_attrs(&mut b.attrs, defs, subject, diags);
                for c in &mut b.choices {
                    if let Some(w) = &mut c.when {
                        expand_slot(w, defs, subject, diags);
                    }
                    expand_attrs(&mut c.attrs, defs, subject, diags);
                    expand_nodes(&mut c.body, defs, subject, diags);
                }
            }
            Node::Match(m) => {
                // The subject itself expands in the OUTER scope (a nested
                // match's `$` refers to its own subject only after this).
                expand_slot(&mut m.subject, defs, subject, diags);
                let inner = m.subject.raw.clone();
                for arm in &mut m.arms {
                    match arm {
                        Arm::When { test, body, .. } => {
                            expand_slot(test, defs, Some(&inner), diags);
                            expand_nodes(body, defs, Some(&inner), diags);
                        }
                        Arm::Otherwise { body, .. } => {
                            expand_nodes(body, defs, Some(&inner), diags)
                        }
                    }
                }
            }
            Node::Timeline(t) => {
                if let Some(d) = &mut t.duration {
                    expand_slot(d, defs, subject, diags);
                }
                for track in &mut t.tracks {
                    for clip in &mut track.clips {
                        match &mut clip.node {
                            ClipNode::Directive(d) => {
                                expand_attrs(&mut d.attrs, defs, subject, diags)
                            }
                            ClipNode::Set(s) => expand_slot(&mut s.expr, defs, subject, diags),
                        }
                    }
                }
            }
        }
    }
}

fn expand_attrs(
    attrs: &mut [Attr],
    defs: &DefTable<'_>,
    subject: Option<&str>,
    diags: &mut Vec<Diagnostic>,
) {
    for a in attrs {
        if let AttrValue::Ref(slot) = &mut a.value {
            expand_slot(slot, defs, subject, diags);
        }
    }
}

fn expand_slot(
    slot: &mut CelSlot,
    defs: &DefTable<'_>,
    subject: Option<&str>,
    diags: &mut Vec<Diagnostic>,
) {
    match expand_cel(&slot.raw, defs, subject, &mut Vec::new()) {
        Ok(s) => slot.raw = s,
        Err(message) => diags.push(Diagnostic {
            code: "E-COMPILE-EXPAND".to_string(),
            severity: Severity::Error,
            message,
            span: slot.span,
            layer: Layer::Cel,
            fixits: Vec::new(),
            provenance: None,
        }),
    }
}

/// Expand one raw CEL fragment. `stack` is the def-name expansion path (cycle
/// guard). On `Err` the stack may be left dirty — the caller aborts the whole
/// compile, never resumes.
pub fn expand_cel(
    raw: &str,
    defs: &DefTable<'_>,
    subject: Option<&str>,
    stack: &mut Vec<String>,
) -> Result<String, String> {
    let refs = scan_refs(raw);
    if refs.is_empty() {
        return Ok(raw.to_string());
    }
    let mut out = raw.to_string();
    // Right-to-left so earlier byte offsets stay valid while splicing.
    for r in refs.iter().rev() {
        let end = r.call.as_ref().map_or(r.span.byte_end, |c| c.span.byte_end);
        let replacement = if r.is_dollar {
            let Some(s) = subject else {
                return Err("`$` used outside a <match> arm".to_string());
            };
            subject_text(s)
        } else {
            expand_ref(r, raw, defs, subject, stack)?
        };
        out.replace_range(r.span.byte_start..end, &replacement);
    }
    Ok(out)
}

fn expand_ref(
    r: &lute_cel::RefUse,
    raw: &str,
    defs: &DefTable<'_>,
    subject: Option<&str>,
    stack: &mut Vec<String>,
) -> Result<String, String> {
    let name = &r.name;
    let Some(body) = defs.bodies.get(name) else {
        return Err(format!(
            "`@{name}` names no known def body (gate should have caught this)"
        ));
    };
    // Args expand in the CALLER's scope, BEFORE the cycle push — `@f(@f(1))`
    // is nesting, not a cycle.
    let params = defs.params.get(name).cloned().unwrap_or_default();
    let args: Vec<String> = match &r.call {
        Some(call) => {
            let mut v = Vec::with_capacity(call.args.len());
            for sp in &call.args {
                v.push(expand_cel(
                    &raw[sp.byte_start..sp.byte_end],
                    defs,
                    subject,
                    stack,
                )?);
            }
            v
        }
        None => Vec::new(),
    };
    if args.len() != params.len() {
        return Err(format!(
            "`@{name}` takes {} arg(s), got {} (gate should have caught this)",
            params.len(),
            args.len()
        ));
    }
    if stack.iter().any(|n| n == name) {
        return Err(format!(
            "def expansion cycle: {} -> {name}",
            stack.join(" -> ")
        ));
    }
    stack.push(name.clone());
    // A def body is subject-independent (no `$`); nested `@refs` recurse.
    let mut expanded = expand_cel(body, defs, None, stack)?;
    for ((pname, _ty), arg) in params.iter().zip(&args) {
        expanded = substitute_ident(&expanded, pname, &format!("({arg})"));
    }
    stack.pop();
    Ok(format!("({expanded})"))
}

/// Replace whole-identifier occurrences of `name` outside CEL string literals.
/// An occurrence preceded by `.`/ident-byte or followed by an ident-byte is a
/// different identifier (`scene.n`, `none`) and is left alone.
fn substitute_ident(body: &str, name: &str, replacement: &str) -> String {
    let mask = cel_string_mask(body);
    let bytes = body.as_bytes();
    let mut out = String::with_capacity(body.len());
    let mut i = 0;
    while i < bytes.len() {
        if !mask[i] && body[i..].starts_with(name) {
            let prev_ok = i == 0 || {
                let p = bytes[i - 1];
                !(p.is_ascii_alphanumeric() || p == b'_' || p == b'.')
            };
            let end = i + name.len();
            let next_ok = end >= bytes.len() || {
                let n = bytes[end];
                !(n.is_ascii_alphanumeric() || n == b'_')
            };
            if prev_ok && next_ok {
                out.push_str(replacement);
                i = end;
                continue;
            }
        }
        let ch_len = body[i..].chars().next().map_or(1, |c| c.len_utf8());
        out.push_str(&body[i..i + ch_len]);
        i += ch_len;
    }
    out
}

/// `$` substitution text (§4.5): a bare dotted path goes in verbatim; anything
/// compound is parenthesized for precedence safety.
fn subject_text(subject: &str) -> String {
    let bare = !subject.is_empty()
        && subject
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.');
    if bare {
        subject.to_string()
    } else {
        format!("({subject})")
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use lute_manifest::types::Type;

    use super::*;

    type Tables = (
        BTreeMap<String, String>,
        BTreeMap<String, Vec<(String, Type)>>,
    );

    fn tables(bodies: &[(&str, &str)], params: &[(&str, &[&str])]) -> Tables {
        let b = bodies
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let p = params
            .iter()
            .map(|(k, ps)| {
                (
                    k.to_string(),
                    ps.iter().map(|n| (n.to_string(), Type::Number)).collect(),
                )
            })
            .collect();
        (b, p)
    }

    fn expand(raw: &str, t: &Tables, subject: Option<&str>) -> Result<String, String> {
        let defs = DefTable {
            bodies: &t.0,
            params: &t.1,
        };
        expand_cel(raw, &defs, subject, &mut Vec::new())
    }

    #[test]
    fn bare_ref_expands_parenthesized() {
        let t = tables(&[("fond", "scene.affect.bianca >= 1")], &[]);
        assert_eq!(
            expand("@fond", &t, None).unwrap(),
            "(scene.affect.bianca >= 1)"
        );
    }

    #[test]
    fn fn_ref_binds_args_positionally_and_parenthesized() {
        let t = tables(
            &[("atLeast", "scene.affect.bianca >= n")],
            &[("atLeast", &["n"])],
        );
        assert_eq!(
            expand("@atLeast(2)", &t, None).unwrap(),
            "(scene.affect.bianca >= (2))"
        );
        // Param ident boundaries: `n` inside `scene.n`/`none` must NOT bind.
        let t = tables(&[("f", "scene.n + none + n")], &[("f", &["n"])]);
        assert_eq!(expand("@f(9)", &t, None).unwrap(), "(scene.n + none + (9))");
    }

    #[test]
    fn refs_expand_recursively() {
        let t = tables(&[("a", "@b + 1"), ("b", "2")], &[]);
        assert_eq!(expand("@a", &t, None).unwrap(), "((2) + 1)");
    }

    #[test]
    fn cycle_is_an_error_not_a_hang() {
        let t = tables(&[("a", "@b"), ("b", "@a")], &[]);
        let err = expand("@a", &t, None).unwrap_err();
        assert!(err.contains("cycle"), "{err}");
    }

    #[test]
    fn dollar_substitutes_bare_subject_verbatim() {
        let t = tables(&[], &[]);
        assert_eq!(
            expand("$ == 'blunt'", &t, Some("scene.choices.number")).unwrap(),
            "scene.choices.number == 'blunt'"
        );
        // Compound subject gets parenthesized (plan spec-gap note 11).
        assert_eq!(expand("$ == 3", &t, Some("a + b")).unwrap(), "(a + b) == 3");
        // `$` with no enclosing match is a gate-proven-unreachable error.
        assert!(expand("$ == 1", &t, None).is_err());
    }

    #[test]
    fn string_literal_tokens_are_untouched() {
        let t = tables(&[], &[]);
        assert_eq!(expand("x == '@gold'", &t, None).unwrap(), "x == '@gold'");
    }

    #[test]
    fn unknown_ref_is_an_error() {
        let t = tables(&[], &[]);
        assert!(expand("@nope", &t, None).is_err());
    }

    #[test]
    fn expand_document_rewrites_slots_with_match_subject_scope() {
        let src = "---\ncharacter: bianca\nseason: 1\nepisode: 2\nstate:\n  scene.affect.bianca: { type: number, default: 0 }\ndefs:\n  fond: { type: bool, cel: \"scene.affect.bianca >= 1\" }\n---\n\n## Shot 1.\n\n<match on=\"scene.choices.number\">\n  <when test=\"@fond\">\n    :line[fixer]{delivery=\"thought\"}: a\n  </when>\n  <when test=\"$ == 'blunt'\">\n    :line[fixer]{delivery=\"thought\"}: b\n  </when>\n  <otherwise>\n    :line[fixer]{delivery=\"thought\"}: c\n  </otherwise>\n</match>\n";
        let (mut doc, diags) = lute_syntax::parse(src);
        assert!(diags
            .iter()
            .all(|d| d.severity != lute_core_span::Severity::Error));
        let t = tables(&[("fond", "scene.affect.bianca >= 1")], &[]);
        let defs = DefTable {
            bodies: &t.0,
            params: &t.1,
        };
        let ediags = expand_document(&mut doc, &defs);
        assert!(ediags.is_empty(), "{ediags:#?}");
        let lute_syntax::ast::Node::Match(m) = &doc.shots[0].body[0] else {
            panic!("first node is the match");
        };
        let tests: Vec<&str> = m
            .arms
            .iter()
            .filter_map(|a| match a {
                lute_syntax::ast::Arm::When { test, .. } => Some(test.raw.as_str()),
                lute_syntax::ast::Arm::Otherwise { .. } => None,
            })
            .collect();
        assert_eq!(
            tests,
            vec![
                "(scene.affect.bianca >= 1)",
                "scene.choices.number == 'blunt'"
            ]
        );
    }
}
