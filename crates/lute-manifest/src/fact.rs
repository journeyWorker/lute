//! The Datalog fact-pattern grammar (dsl 0.3.0 §5, Appendix C), and the
//! cursor and term readers the rule grammar (`lute_syntax::datalog`) shares.
//!
//! [`parse_fact`] parses a ground/wildcard fact pattern (`rel(a, b)` /
//! `rel(a, _)`). It is TOTAL — malformed input never panics, it produces a
//! typed [`DatalogError`]. All spans are byte offsets RELATIVE to the parsed
//! input string; callers add their own base offset.
//!
//! ```text
//! FactPattern ::= Ident "(" FactArg ("," FactArg)* ")"
//! FactArg     ::= Ident | Quoted | "true" | "false" | "_" | "@" Ident | "occasion.target"
//!                 (* "@" Ident: a component param; "occasion.target": the member a kind
//!                    or `for=` beat runs for *)
//! CountOp     ::= ">=" | ">" | "<=" | "<" | "==" | "=" | "!="
//! Ident       ::= [A-Za-z][A-Za-z0-9_]*
//! Quoted      ::= "\"" Name "\"" | "'" Name "'"   (* any name, `lab-b2`; the same name as
//!                                                     its bare spelling; always a constant *)
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
    /// `@name` (dsl 0.24.0 §4): an `effects: true` component's param, bound
    /// to its `::use` argument — a constant — when the body is expanded. A
    /// param still present in a host document is an error.
    Param(String),
    /// `occasion.target` (dsl 0.28.0 §3): the member the enclosing kind or
    /// `for=` beat runs for — ground per member, bound when the write
    /// executes. The checker judges it once per member of the beat.
    Target,
}

/// `true` for the fresh variable a body-atom `_` parses to (dsl 0.24 T3-9).
/// It is bound by its positive atom like any variable but never read
/// elsewhere; in a negated atom it is existential (`not seen(W, _)`: no
/// `seen(W, …)` tuple at all).
pub fn is_anonymous_var(name: &str) -> bool {
    name.starts_with('_')
}

/// The comparison of a [`BodyLiteral::Count`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CountOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl CountOp {
    /// Whether `count <op> n` holds.
    pub fn holds(self, count: u64, n: u64) -> bool {
        match self {
            CountOp::Eq => count == n,
            CountOp::Ne => count != n,
            CountOp::Lt => count < n,
            CountOp::Le => count <= n,
            CountOp::Gt => count > n,
            CountOp::Ge => count >= n,
        }
    }

    /// The canonical spelling (`=` is read as `==`).
    pub fn as_str(self) -> &'static str {
        match self {
            CountOp::Eq => "==",
            CountOp::Ne => "!=",
            CountOp::Lt => "<",
            CountOp::Le => "<=",
            CountOp::Gt => ">",
            CountOp::Ge => ">=",
        }
    }

    /// Parse a canonical spelling ([`CountOp::as_str`]).
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "==" | "=" => CountOp::Eq,
            "!=" => CountOp::Ne,
            "<" => CountOp::Lt,
            "<=" => CountOp::Le,
            ">" => CountOp::Gt,
            ">=" => CountOp::Ge,
            _ => return None,
        })
    }
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
    let mut c = Cur {
        b: input.as_bytes(),
        i: 0,
        anon: 0,
    };
    c.ws();
    let pattern_start = c.i;
    let (relation, relation_span) = c.ident().ok_or_else(|| DatalogError::Malformed {
        at: c.i,
        msg: "expected relation name".to_string(),
    })?;
    let args = parse_arg_list(&mut c, |c| {
        let start = c.i;
        let term = parse_fact_term(c)?;
        Ok(FactArg {
            term,
            span: (start, c.i),
        })
    })?;
    let end = c.i;
    c.ws();
    if c.i != c.b.len() {
        return Err(DatalogError::Malformed {
            at: c.i,
            msg: "unexpected trailing input after fact pattern".to_string(),
        });
    }
    Ok(FactPattern {
        relation,
        relation_span,
        args,
        span: (pattern_start, end),
    })
}

/// A byte cursor over the parsed input, shared by the fact and rule grammars.
pub struct Cur<'a> {
    pub b: &'a [u8],
    pub i: usize,
    /// The next anonymous-variable index of the rule being parsed.
    pub anon: usize,
}

impl<'a> Cur<'a> {
    pub fn ws(&mut self) {
        while self.i < self.b.len() && (self.b[self.i] == b' ' || self.b[self.i] == b'\t') {
            self.i += 1;
        }
    }
    pub fn ident(&mut self) -> Option<(String, (usize, usize))> {
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
        Some((
            String::from_utf8_lossy(&self.b[s..self.i]).into_owned(),
            (s, self.i),
        ))
    }
    pub fn eat(&mut self, c: u8) -> bool {
        if self.i < self.b.len() && self.b[self.i] == c {
            self.i += 1;
            true
        } else {
            false
        }
    }
    pub fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }
}

/// Consumes the literal byte string `s` if the cursor is positioned at it.
pub fn eat_str(c: &mut Cur, s: &str) -> bool {
    let bytes = s.as_bytes();
    if c.b[c.i..].starts_with(bytes) {
        c.i += bytes.len();
        true
    } else {
        false
    }
}

/// `"(" item ("," item)* ")"`, requiring at least one item.
pub fn parse_arg_list<T>(
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
pub fn check_function_or_op(c: &mut Cur, at: usize, name: &str) -> Option<DatalogError> {
    // `lab-b2` written bare: one name only when quoted.
    if !name.starts_with(|c: char| c.is_ascii_uppercase())
        && c.peek() == Some(b'-')
        && c.b
            .get(c.i + 1)
            .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
    {
        let end = c.i
            + c.b[c.i..]
                .iter()
                .take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
                .count();
        let glued = String::from_utf8_lossy(&c.b[at..end]);
        return Some(DatalogError::Malformed {
            at,
            msg: format!(
                "`{glued}` is a name that is not an identifier — write it quoted: \"{glued}\""
            ),
        });
    }
    c.ws();
    if c.peek() == Some(b'(') {
        return Some(DatalogError::FunctionTerm {
            at,
            name: name.to_string(),
        });
    }
    if let Some(op) = c.peek() {
        if matches!(op, b'+' | b'-' | b'*' | b'/') {
            return Some(DatalogError::FunctionTerm {
                at,
                name: (op as char).to_string(),
            });
        }
    }
    None
}

/// A quoted name at the cursor (`"lab-b2"` / `'lab-b2'`): the name, cursor
/// past the closing quote. The quoted and bare spellings of a name are the
/// same name. `Ok(None)` when the cursor is not at a quote.
pub fn quoted_term(c: &mut Cur) -> Result<Option<String>, DatalogError> {
    if !matches!(c.peek(), Some(b'"' | b'\'')) {
        return Ok(None);
    }
    let at = c.i;
    let text = std::str::from_utf8(&c.b[at..]).unwrap_or_default();
    let (name, len) = crate::text::read_quoted(text).ok_or_else(|| DatalogError::Malformed {
        at,
        msg: "unclosed quote in an argument".to_string(),
    })?;
    c.i += len;
    Ok(Some(name))
}

fn parse_fact_term(c: &mut Cur) -> Result<FactTerm, DatalogError> {
    if c.peek() == Some(b'_') {
        c.i += 1;
        return Ok(FactTerm::Wildcard);
    }
    if let Some(name) = quoted_term(c)? {
        return Ok(FactTerm::Ident(name));
    }
    let at = c.i;
    if c.peek() == Some(b'@') {
        c.i += 1;
        let (name, _) = c.ident().ok_or_else(|| DatalogError::Malformed {
            at: c.i,
            msg: "expected a component param name after `@`".to_string(),
        })?;
        return Ok(FactTerm::Param(name));
    }
    let (name, _) = c.ident().ok_or_else(|| DatalogError::Malformed {
        at,
        msg: "expected an argument (identifier, a quoted name, `true`, `false`, `_`, a \
              component `@param`, or `occasion.target`)"
            .to_string(),
    })?;
    if name == "occasion" && eat_str(c, ".target") {
        if c.peek()
            .is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.')
        {
            return Err(DatalogError::Malformed {
                at,
                msg: "a fact argument reads the beat's member as `occasion.target`".to_string(),
            });
        }
        return Ok(FactTerm::Target);
    }
    if let Some(err) = check_function_or_op(c, at, &name) {
        return Err(err);
    }
    Ok(match name.as_str() {
        "true" => FactTerm::Bool(true),
        "false" => FactTerm::Bool(false),
        _ => FactTerm::Ident(name),
    })
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
            vec![
                FactTerm::Ident("shadowheart".into()),
                FactTerm::Ident("grove".into())
            ]
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
        for bad in [
            "", "rel", "rel(", "rel()", "rel(a", "rel(a,)", "rel(a) x", "re-l(a)",
        ] {
            assert!(
                matches!(parse_fact(bad), Err(DatalogError::Malformed { .. })),
                "{bad}"
            );
        }
    }
}
