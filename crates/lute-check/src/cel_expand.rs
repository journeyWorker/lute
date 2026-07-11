//! §5.1: the textual `@def` expander (D2) — "a static, hygienic text
//! substitution the checker performs itself" before `decide()` (Task 2)
//! folds constants. Hosted here (not `lute-compile`) so `decide()` can
//! expand-then-decide with no dependency cycle, and so `DefTable` (which
//! carries `lute-manifest` `Type`s) avoids a new `lute-cel`→`lute-manifest`
//! edge.
//!
//! A def ref is a compile-time macro (dsl §8.1). Each substituted body is
//! PARENTHESIZED; `@fn(args)` binds args positionally (arity/type already
//! gate-proven by the checker); expansion recurses with a cycle guard; `$`
//! substitutes the enclosing `<match>` subject. The output CEL is
//! `@`/`$`-free.
//!
//! Moved verbatim from `lute-compile/src/expand.rs` (0.4.0 T1, behavior-
//! preserving). `lute-compile`'s AST driver
//! (`expand_document`/`expand_nodes`/`expand_attrs`/`expand_slot`) stays put
//! and imports `expand_cel`/`DefTable` from this module.

use std::collections::BTreeMap;

use lute_cel::{cel_string_mask, scan_refs};
use lute_manifest::types::Type;

/// The merged def table (plugin < imported < inline), borrowed from
/// `FoldedEnv { def_bodies, env.def_params }` (`crate::check::FoldedEnv`).
pub struct DefTable<'a> {
    pub bodies: &'a BTreeMap<String, String>,
    pub params: &'a BTreeMap<String, Vec<(String, Type)>>,
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
    // `scan_refs` returns BOTH an `@fn(..)` call AND every `@ref`/`$` nested in
    // its arg list. Splicing a nested ref first (right-to-left) would shift the
    // bytes under the outer call's ORIGINAL span, so re-applying that span to
    // the mutated string panics (shorter replacement) or corrupts the tail
    // (longer). Splice ONLY top-level refs — those whose token does not sit
    // inside another ref's `(...)` group; `expand_ref` recursively expands each
    // call's args (on the ORIGINAL arg text), so nested refs are handled there.
    let call_spans: Vec<(usize, usize)> = refs
        .iter()
        .filter_map(|r| r.call.as_ref())
        .map(|c| (c.span.byte_start, c.span.byte_end))
        .collect();
    let top: Vec<&lute_cel::RefUse> = refs
        .iter()
        .filter(|r| {
            !call_spans
                .iter()
                .any(|&(s, e)| s <= r.span.byte_start && r.span.byte_end <= e)
        })
        .collect();
    let mut out = raw.to_string();
    // Right-to-left so earlier byte offsets stay valid while splicing. Top-level
    // refs never overlap, so each original span is still accurate here.
    for r in top.iter().rev() {
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
    // Thread the caller's `subject`: a match-scoped def body may use `$`, which
    // must resolve to the ENCLOSING match subject. `$` only errors when the
    // ultimate context is truly outside a match (`subject == None`). Nested
    // `@refs` recurse under the cycle guard.
    let expanded = expand_cel(body, defs, subject, stack)?;
    // Bind params HYGIENICALLY: substitute every occurrence simultaneously off
    // the pre-substitution body, so one arg's text can never be re-captured by a
    // later param's name (`outer(b)=@f(b,1)`, `f(a,b)=a+b`: b stays b, not 1).
    let expanded = substitute_params(&expanded, &params, &args);
    stack.pop();
    Ok(format!("({expanded})"))
}

/// Bind every parameter of a def body SIMULTANEOUSLY: scan `body` once for
/// whole-identifier occurrences of any param name (outside CEL string literals),
/// then splice each occurrence's `(arg)` replacement RIGHT-TO-LEFT. Because the
/// scan reads the ORIGINAL body and a spliced replacement is never re-scanned,
/// an argument's text can never be captured by a later param's name (hygiene).
///
/// An identifier preceded by `.` (member access: `scene.n`) is a different name
/// and is left alone; maximal-run matching means `none` never binds param `n`.
fn substitute_params(body: &str, params: &[(String, Type)], args: &[String]) -> String {
    if params.is_empty() {
        return body.to_string();
    }
    let binding: BTreeMap<&str, &str> = params
        .iter()
        .zip(args)
        .map(|((p, _ty), a)| (p.as_str(), a.as_str()))
        .collect();
    let mask = cel_string_mask(body);
    let bytes = body.as_bytes();
    let is_ident = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    // (start, end, replacement) per param occurrence, collected in source order.
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if is_ident(bytes[i]) {
            let start = i;
            i += 1;
            while i < bytes.len() && is_ident(bytes[i]) {
                i += 1;
            }
            // A member-access tail (`scene.n`) or a string-literal byte is not a
            // free parameter reference.
            let prev_ok = start == 0 || bytes[start - 1] != b'.';
            if prev_ok && !mask[start] {
                if let Some(arg) = binding.get(&body[start..i]) {
                    edits.push((start, i, format!("({arg})")));
                }
            }
        } else {
            i += 1;
        }
    }
    let mut out = body.to_string();
    for (start, end, replacement) in edits.into_iter().rev() {
        out.replace_range(start..end, &replacement);
    }
    out
}

/// `$` substitution text (§4.5): a bare dotted path goes in verbatim; anything
/// compound is parenthesized for precedence safety.
pub fn subject_text(subject: &str) -> String {
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

    // Finding 1 (Critical): `scan_refs` returns the outer call AND the refs
    // nested in its args. Only the top-level ref is spliced; nested refs are
    // expanded recursively by `expand_ref`, so a length-changing inner
    // replacement can no longer corrupt the outer call's byte range or panic.
    #[test]
    fn nested_fn_refs_expand_without_double_splice_or_panic() {
        let t = tables(
            &[("f", "a * 2"), ("g", "b + 1")],
            &[("f", &["a"]), ("g", &["b"])],
        );
        // g(1) = (1)+1, f(that) = that*2 — fully parenthesized, correct.
        assert_eq!(expand("@f(@g(1))", &t, None).unwrap(), "((((1) + 1)) * 2)");

        // A deeper nest must not panic (exercises shorter/longer replacements).
        let deep = tables(&[("id", "x"), ("h", "y")], &[("id", &["x"]), ("h", &["y"])]);
        assert!(expand("@id(@id(@h(1)))", &deep, None).is_ok());

        // Unbalanced nesting degrades to a diagnostic, never a panic.
        assert!(expand("@f(@g(1)", &t, None).is_err());
    }

    // Finding 2 (Critical): a match-scoped def body may use `$`; it must resolve
    // to the ENCLOSING match subject, not be rejected as `$`-outside-match.
    #[test]
    fn def_body_dollar_resolves_to_enclosing_match_subject() {
        let t = tables(&[("is_selected", "$ == 'blunt'")], &[]);
        assert_eq!(
            expand("@is_selected", &t, Some("scene.choices.number")).unwrap(),
            "(scene.choices.number == 'blunt')"
        );
        // The same ref outside any <match> still errors (no subject to bind).
        assert!(expand("@is_selected", &t, None).is_err());
    }

    // Finding 3 (Important): params bind SIMULTANEOUSLY. Non-hygienic one-at-a-
    // time binding let f's later param `b` rewrite the arg text `b` (outer's
    // param), collapsing `@outer(2)` to `1 + 1`; it must stay `(2) + (1)`.
    #[test]
    fn param_binding_is_hygienic_no_arg_recapture() {
        let t = tables(
            &[("outer", "@f(b, 1)"), ("f", "a + b")],
            &[("outer", &["b"]), ("f", &["a", "b"])],
        );
        assert_eq!(expand("@outer(2)", &t, None).unwrap(), "((((2)) + (1)))");
    }

    // 0.4.0 T1: proves the moved expander works standalone from lute-check,
    // with no lute-compile AST/Document scaffolding involved.
    #[test]
    fn expands_zero_arity_def_through_check_crate() {
        let mut bodies = std::collections::BTreeMap::new();
        bodies.insert("never".to_string(), "1 > 2".to_string());
        let params = std::collections::BTreeMap::new();
        let defs = DefTable {
            bodies: &bodies,
            params: &params,
        };
        let out = expand_cel("@never || run.x", &defs, None, &mut Vec::new()).unwrap();
        assert_eq!(out, "(1 > 2) || run.x"); // bodies parenthesize (D4 doc)
    }
}
