//! Total Datalog text grammar shared by every fact/rule consumer (dsl 0.3.0
//! §5, §7.1, Appendix C).
//!
//! [`parse_fact`] parses a ground/wildcard fact pattern (`rel(a, b)` /
//! `rel(a, _)`); [`parse_rule`] parses one Horn clause (`Head :- Body`). Both
//! are TOTAL — malformed input never panics, it produces a typed
//! [`DatalogError`]. All spans are byte offsets RELATIVE to the parsed input
//! string; callers add their own base offset.
//!
//! ```text
//! FactPattern ::= Ident "(" FactArg ("," FactArg)* ")"
//! FactArg     ::= Ident | "true" | "false" | "_"
//! Rule        ::= Atom ":-" Literal ("," Literal)*
//! Literal     ::= "not" WS Atom | "cel(" CelString ")" | Term ("="|"!=") Term | Atom
//! Atom        ::= Ident "(" Term ("," Term)* ")"
//! Term        ::= Ident | "true" | "false"   (* "_" is FACT-pattern-only; in a rule it is Malformed *)
//! Ident       ::= [A-Za-z][A-Za-z0-9_]*       (* CelIdent — no "-" *)
//! CelString   ::= "\"" ([^"\\] | \\.)* "\""
//! ```

/// A fact pattern: `rel(a, b)` / `rel(a, _)` (spec §5 GroundFact/RetractPattern).
#[derive(Clone, Debug, PartialEq)]
pub struct FactPattern {
    pub relation: String,
    /// Byte range of the relation ident, relative to the parsed input.
    pub relation_span: (usize, usize),
    pub args: Vec<FactArg>,
    /// Byte range of the whole pattern, relative to the parsed input.
    pub span: (usize, usize),
}

#[derive(Clone, Debug, PartialEq)]
pub struct FactArg {
    pub term: FactTerm,
    pub span: (usize, usize),
}

#[derive(Clone, Debug, PartialEq)]
pub enum FactTerm {
    Ident(String),
    Bool(bool),
    Wildcard,
}

/// One Horn clause: `Head :- Body` (spec §7.1).
#[derive(Clone, Debug, PartialEq)]
pub struct Rule {
    pub head: RuleAtom,
    pub body: Vec<BodyLiteral>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RuleAtom {
    pub relation: String,
    pub relation_span: (usize, usize),
    pub terms: Vec<RuleTerm>,
    pub span: (usize, usize),
}

/// Var = leading ASCII uppercase ident; `true`/`false` = Bool; other idents = Const (§7.1).
#[derive(Clone, Debug, PartialEq)]
pub enum RuleTerm {
    Var(String),
    Const(String),
    Bool(bool),
}

#[derive(Clone, Debug, PartialEq)]
pub enum BodyLiteral {
    Pos(RuleAtom),
    Neg(RuleAtom),
    /// `cel("…")` — the raw CEL string, unescaped (§7.3). Span covers the whole literal.
    Guard { cel: String, span: (usize, usize) },
    /// `Term = Term` (negated: false) / `Term != Term` (negated: true) (§7.1).
    Cmp { lhs: RuleTerm, rhs: RuleTerm, negated: bool, span: (usize, usize) },
}

#[derive(Clone, Debug, PartialEq)]
pub enum DatalogError {
    /// Anything that violates the Appendix C grammar other than a function term.
    Malformed { at: usize, msg: String },
    /// A compound/function term `f(g(x))`, or arithmetic between terms (§7.1, E-DATALOG-FUNCTION).
    FunctionTerm { at: usize, name: String },
}

/// Parses a fact pattern: `rel(a, b)` / `rel(a, _)`. Total — never panics.
pub fn parse_fact(input: &str) -> Result<FactPattern, DatalogError> {
    let mut c = Cur { b: input.as_bytes(), i: 0 };
    c.ws();
    let pattern_start = c.i;
    let (relation, relation_span) = c.ident().ok_or_else(|| DatalogError::Malformed {
        at: c.i,
        msg: "expected relation name".to_string(),
    })?;
    let args = parse_arg_list(&mut c, |c| {
        let start = c.i;
        let term = parse_fact_term(c)?;
        Ok(FactArg { term, span: (start, c.i) })
    })?;
    let end = c.i;
    c.ws();
    if c.i != c.b.len() {
        return Err(DatalogError::Malformed {
            at: c.i,
            msg: "unexpected trailing input after fact pattern".to_string(),
        });
    }
    Ok(FactPattern { relation, relation_span, args, span: (pattern_start, end) })
}

/// Parses one Horn-clause rule: `Head :- Body`. Total — never panics.
pub fn parse_rule(input: &str) -> Result<Rule, DatalogError> {
    let mut c = Cur { b: input.as_bytes(), i: 0 };
    c.ws();
    let head = parse_rule_atom(&mut c)?;
    c.ws();
    if !eat_str(&mut c, ":-") {
        return Err(DatalogError::Malformed {
            at: c.i,
            msg: "expected `:-` after rule head".to_string(),
        });
    }
    let mut body = Vec::new();
    loop {
        c.ws();
        body.push(parse_body_literal(&mut c)?);
        c.ws();
        if c.eat(b',') {
            continue;
        }
        break;
    }
    c.ws();
    if c.i != c.b.len() {
        return Err(DatalogError::Malformed {
            at: c.i,
            msg: "unexpected trailing input after rule body".to_string(),
        });
    }
    Ok(Rule { head, body })
}

struct Cur<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Cur<'a> {
    fn ws(&mut self) {
        while self.i < self.b.len() && (self.b[self.i] == b' ' || self.b[self.i] == b'\t') {
            self.i += 1;
        }
    }
    fn ident(&mut self) -> Option<(String, (usize, usize))> {
        let s = self.i;
        if self.i >= self.b.len() || !self.b[self.i].is_ascii_alphabetic() {
            return None;
        }
        self.i += 1;
        while self.i < self.b.len()
            && (self.b[self.i].is_ascii_alphanumeric() || self.b[self.i] == b'_')
        {
            self.i += 1;
        }
        Some((String::from_utf8_lossy(&self.b[s..self.i]).into_owned(), (s, self.i)))
    }
    fn eat(&mut self, c: u8) -> bool {
        if self.i < self.b.len() && self.b[self.i] == c {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }
}

/// Consumes the literal byte string `s` if the cursor is positioned at it.
fn eat_str(c: &mut Cur, s: &str) -> bool {
    let bytes = s.as_bytes();
    if c.b[c.i..].starts_with(bytes) {
        c.i += bytes.len();
        true
    } else {
        false
    }
}

/// `"(" item ("," item)* ")"`, requiring at least one item.
fn parse_arg_list<T>(
    c: &mut Cur,
    mut parse_item: impl FnMut(&mut Cur) -> Result<T, DatalogError>,
) -> Result<Vec<T>, DatalogError> {
    c.ws();
    if !c.eat(b'(') {
        return Err(DatalogError::Malformed {
            at: c.i,
            msg: "expected `(` after relation name".to_string(),
        });
    }
    c.ws();
    if c.eat(b')') {
        return Err(DatalogError::Malformed {
            at: c.i,
            msg: "a fact pattern needs at least one argument".to_string(),
        });
    }
    let mut items = Vec::new();
    loop {
        c.ws();
        items.push(parse_item(c)?);
        c.ws();
        if c.eat(b',') {
            continue;
        }
        if c.eat(b')') {
            break;
        }
        return Err(DatalogError::Malformed {
            at: c.i,
            msg: "expected `,` or `)`".to_string(),
        });
    }
    Ok(items)
}

/// After a term ident, flags a nested call `f(` or an adjacent arithmetic
/// operator `+ - * /` as the distinct `FunctionTerm` error (§7.1).
fn check_function_or_op(c: &mut Cur, at: usize, name: &str) -> Option<DatalogError> {
    c.ws();
    if c.peek() == Some(b'(') {
        return Some(DatalogError::FunctionTerm { at, name: name.to_string() });
    }
    if let Some(op) = c.peek() {
        if matches!(op, b'+' | b'-' | b'*' | b'/') {
            return Some(DatalogError::FunctionTerm { at, name: (op as char).to_string() });
        }
    }
    None
}

fn parse_fact_term(c: &mut Cur) -> Result<FactTerm, DatalogError> {
    if c.peek() == Some(b'_') {
        c.i += 1;
        return Ok(FactTerm::Wildcard);
    }
    let at = c.i;
    let (name, _) = c.ident().ok_or_else(|| DatalogError::Malformed {
        at,
        msg: "expected an argument (identifier, `true`, `false`, or `_`)".to_string(),
    })?;
    if let Some(err) = check_function_or_op(c, at, &name) {
        return Err(err);
    }
    Ok(match name.as_str() {
        "true" => FactTerm::Bool(true),
        "false" => FactTerm::Bool(false),
        _ => FactTerm::Ident(name),
    })
}

fn classify_rule_term(name: String) -> RuleTerm {
    match name.as_str() {
        "true" => RuleTerm::Bool(true),
        "false" => RuleTerm::Bool(false),
        _ if name.as_bytes()[0].is_ascii_uppercase() => RuleTerm::Var(name),
        _ => RuleTerm::Const(name),
    }
}

fn parse_rule_term(c: &mut Cur) -> Result<RuleTerm, DatalogError> {
    if c.peek() == Some(b'_') {
        return Err(DatalogError::Malformed {
            at: c.i,
            msg: "`_` is retract-pattern-only; rule terms are Var or Const (dsl 0.3.0 §7.1)"
                .to_string(),
        });
    }
    let at = c.i;
    let (name, _) = c.ident().ok_or_else(|| DatalogError::Malformed {
        at,
        msg: "expected a term (identifier, `true`, or `false`)".to_string(),
    })?;
    if let Some(err) = check_function_or_op(c, at, &name) {
        return Err(err);
    }
    Ok(classify_rule_term(name))
}

fn parse_rule_atom(c: &mut Cur) -> Result<RuleAtom, DatalogError> {
    let atom_start = c.i;
    let (relation, relation_span) = c.ident().ok_or_else(|| DatalogError::Malformed {
        at: c.i,
        msg: "expected relation name".to_string(),
    })?;
    let terms = parse_arg_list(c, |c| parse_rule_term(c))?;
    Ok(RuleAtom { relation, relation_span, terms, span: (atom_start, c.i) })
}

/// `"cel(" ...` already consumed through the opening `(`; parses the quoted
/// CEL string (with `\`-escapes unescaped) and the closing `)`.
fn parse_cel_guard(c: &mut Cur, lit_start: usize) -> Result<BodyLiteral, DatalogError> {
    c.ws();
    if !c.eat(b'"') {
        return Err(DatalogError::Malformed {
            at: c.i,
            msg: "expected a quoted CEL string after `cel(`".to_string(),
        });
    }
    let mut buf: Vec<u8> = Vec::new();
    loop {
        match c.peek() {
            None => {
                return Err(DatalogError::Malformed {
                    at: c.i,
                    msg: "unterminated CEL string in `cel(...)` guard".to_string(),
                })
            }
            Some(b'"') => {
                c.i += 1;
                break;
            }
            Some(b'\\') => {
                c.i += 1;
                match c.peek() {
                    Some(esc) => {
                        buf.push(esc);
                        c.i += 1;
                    }
                    None => {
                        return Err(DatalogError::Malformed {
                            at: c.i,
                            msg: "unterminated escape in CEL string".to_string(),
                        })
                    }
                }
            }
            Some(byte) => {
                buf.push(byte);
                c.i += 1;
            }
        }
    }
    c.ws();
    if !c.eat(b')') {
        return Err(DatalogError::Malformed {
            at: c.i,
            msg: "expected `)` to close `cel(...)`".to_string(),
        });
    }
    Ok(BodyLiteral::Guard { cel: String::from_utf8_lossy(&buf).into_owned(), span: (lit_start, c.i) })
}

/// `Literal ::= "not" WS Atom | "cel(" CelString ")" | Term ("="|"!=") Term | Atom`.
fn parse_body_literal(c: &mut Cur) -> Result<BodyLiteral, DatalogError> {
    c.ws();
    let lit_start = c.i;
    let at = c.i;
    let (name, _) = c.ident().ok_or_else(|| DatalogError::Malformed {
        at,
        msg: "expected a body literal (atom, `not` atom, `cel(...)`, or a comparison)"
            .to_string(),
    })?;

    if name == "not" {
        c.ws();
        return parse_rule_atom(c).map(BodyLiteral::Neg).map_err(|e| match e {
            DatalogError::FunctionTerm { .. } => e,
            DatalogError::Malformed { .. } => DatalogError::Malformed {
                at: lit_start,
                msg: "`not` must be followed by an atom".to_string(),
            },
        });
    }

    if name == "cel" {
        let save = c.i;
        c.ws();
        if c.eat(b'(') {
            return parse_cel_guard(c, lit_start);
        }
        c.i = save;
    }

    c.ws();
    if c.peek() == Some(b'(') {
        let terms = parse_arg_list(c, |c| parse_rule_term(c))?;
        let relation_span = (at, at + name.len());
        return Ok(BodyLiteral::Pos(RuleAtom {
            relation: name,
            relation_span,
            terms,
            span: (lit_start, c.i),
        }));
    }

    if let Some(err) = check_function_or_op(c, at, &name) {
        return Err(err);
    }
    let lhs = classify_rule_term(name);
    c.ws();
    let negated = if eat_str(c, "!=") {
        true
    } else if c.eat(b'=') {
        false
    } else {
        return Err(DatalogError::Malformed {
            at: c.i,
            msg: "expected `(` (atom) or `=`/`!=` (comparison)".to_string(),
        });
    };
    c.ws();
    let rhs = parse_rule_term(c)?;
    Ok(BodyLiteral::Cmp { lhs, rhs, negated, span: (lit_start, c.i) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ground_binary_fact() {
        let f = parse_fact("atLocation(shadowheart, grove)").unwrap();
        assert_eq!(f.relation, "atLocation");
        assert_eq!(f.relation_span, (0, "atLocation".len()));
        assert_eq!(
            f.args.iter().map(|a| a.term.clone()).collect::<Vec<_>>(),
            vec![FactTerm::Ident("shadowheart".into()), FactTerm::Ident("grove".into())]
        );
    }

    #[test]
    fn parses_wildcard_and_bool_args() {
        let f = parse_fact("knows(_, true)").unwrap();
        assert_eq!(f.args[0].term, FactTerm::Wildcard);
        assert_eq!(f.args[1].term, FactTerm::Bool(true));
    }

    #[test]
    fn fact_function_term_is_function_error() {
        assert!(matches!(
            parse_fact("rel(f(x))"),
            Err(DatalogError::FunctionTerm { name, .. }) if name == "f"
        ));
    }

    #[test]
    fn fact_malformed_shapes() {
        for bad in ["", "rel", "rel(", "rel()", "rel(a", "rel(a,)", "rel(a) x", "re-l(a)"] {
            assert!(matches!(parse_fact(bad), Err(DatalogError::Malformed { .. })), "{bad}");
        }
    }

    #[test]
    fn parses_recursive_rule() {
        let r = parse_rule("canReach(C, L2) :- canReach(C, L1), connected(L1, L2)").unwrap();
        assert_eq!(r.head.relation, "canReach");
        assert_eq!(r.head.terms, vec![RuleTerm::Var("C".into()), RuleTerm::Var("L2".into())]);
        assert_eq!(r.body.len(), 2);
        assert!(matches!(&r.body[0], BodyLiteral::Pos(a) if a.relation == "canReach"));
    }

    #[test]
    fn parses_negation_inequality_and_const_head() {
        let r = parse_rule(
            "ally(A, B) :- faction(A), faction(B), not hostile(A, B), not hostile(B, A), A != B",
        )
        .unwrap();
        assert!(matches!(&r.body[2], BodyLiteral::Neg(a) if a.relation == "hostile"));
        assert!(matches!(
            &r.body[4],
            BodyLiteral::Cmp { lhs: RuleTerm::Var(a), rhs: RuleTerm::Var(b), negated: true, .. }
                if a == "A" && b == "B"
        ));
        let c = parse_rule("alerted(absolute, F) :- hostile(F, absolute)").unwrap();
        assert_eq!(c.head.terms[0], RuleTerm::Const("absolute".into()));
    }

    #[test]
    fn parses_cel_guard() {
        let r = parse_rule("act1Site(L) :- location(L), site(L), cel(\"run.act == 1\")").unwrap();
        assert!(matches!(&r.body[2], BodyLiteral::Guard { cel, .. } if cel == "run.act == 1"));
    }

    #[test]
    fn rule_function_term_and_wildcard_are_errors() {
        assert!(matches!(
            parse_rule("d(X) :- b(f(X))"),
            Err(DatalogError::FunctionTerm { .. })
        ));
        assert!(matches!(
            parse_rule("d(X) :- b(X), c(_)"),
            Err(DatalogError::Malformed { .. })
        ));
        assert!(matches!(
            parse_rule("d(X) :- b(X + 1)"),
            Err(DatalogError::FunctionTerm { .. })
        ));
    }

    #[test]
    fn equality_binds_shape_parses() {
        let r = parse_rule("d(X, Y) :- b(X), Y = X").unwrap();
        assert!(matches!(&r.body[1], BodyLiteral::Cmp { negated: false, .. }));
    }
}
