//! `pair` rules (dsl 0.51.0 §6): begin/end balance over the `node` rows.
//!
//! A rule has three lint-CEL predicates over a row — `open`, `close` and the
//! optional `inside` — and a [`PairScope`]. It is evaluated in the node walk
//! ([`crate::nodes::walk_nodes_with`]), at each row while the row's view is
//! valid, with one open slot per frame: per body for scope `body` (the rows
//! are that body's direct children), per section or top-level declaration
//! for scope `section` (every row in it, at every depth, in document order),
//! and one for the whole document for scope `document` (every row of every
//! section and declaration, in document order).
//! A slot holds where its pair was opened and the message rendered there;
//! no row is stored.
//!
//! At a row, with a pair open: `close` closes it; otherwise `open` is a
//! nested open (the outer pair stays open); otherwise a false `inside` is an
//! inside fault. With no pair open: `open` opens one; otherwise `close` is
//! an unopened close. A row satisfying both predicates is therefore a close
//! while a pair is open and an open otherwise, and a row costs at most three
//! predicate invocations. A frame that ends with its slot open reports the
//! unclosed open. A finding carries the rule's code and level, anchors at the
//! offending row and appends the [`PairFault`] phrase to the rendered
//! message; a nested or inside fault relates the open row.

use cel_parser::ast::IdedExpr;
use lute_core_span::{Diagnostic, Layer, RelatedDiagnostic, Severity, Span};

use crate::eval::{eval, render_message, Env, Value};
use crate::model::{diagnostic_code, PairScope, PairSpec};
use crate::nodes::NodeRow;
use crate::rules::{expr_error, parse_predicate, Finding, ResolvedRule, Tally};

/// What a `pair` finding reports, appended to the rule's rendered message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PairFault {
    /// An open whose frame ended before a close.
    Unclosed(PairScope),
    /// A close with no open pair.
    Unopened(PairScope),
    /// An open while the pair opened at `line` is still open.
    Nested { line: u32 },
    /// A row between the open at `line` and its close for which `inside`
    /// is false.
    Inside { line: u32 },
}

impl std::fmt::Display for PairFault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let unit = |scope: &PairScope| match scope {
            PairScope::Body => "body",
            PairScope::Section => "section",
            PairScope::Document => "document",
        };
        match self {
            PairFault::Unclosed(scope) => {
                write!(f, "— opened here, with no close in this {}", unit(scope))
            }
            PairFault::Unopened(scope) => {
                write!(f, "— a close with no open pair in this {}", unit(scope))
            }
            PairFault::Nested { line } => {
                write!(
                    f,
                    "— opened again while the pair from line {line} is still open"
                )
            }
            PairFault::Inside { line } => {
                write!(f, "— not allowed inside the pair opened at line {line}")
            }
        }
    }
}

/// A frame of the node walk a slot lives in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Frame {
    /// A body: its direct children.
    Body,
    /// A section or top-level declaration.
    Scope,
    /// The whole document: every section and top-level declaration.
    Document,
}

/// An open pair: where it was opened and the message rendered at that row.
#[derive(Debug)]
struct Open {
    span: Span,
    message: String,
}

/// One active `pair` rule over one document.
#[derive(Debug)]
pub(crate) struct PairRule<'r> {
    id: &'r str,
    code: String,
    severity: Severity,
    message: &'r str,
    options: Value,
    open: IdedExpr,
    close: IdedExpr,
    inside: Option<IdedExpr>,
    frame: Frame,
    scope: PairScope,
    /// One slot per frame of this rule's kind, innermost last.
    slots: Vec<Option<Open>>,
    /// Set by the rule's first `E-LINT-EXPR`: the rule stops for the
    /// document.
    failed: bool,
}

impl<'r> PairRule<'r> {
    /// Parse `pair`'s predicates, or the `E-LINT-EXPR` of the first that
    /// does not parse.
    pub(crate) fn parse(
        rule: &'r ResolvedRule,
        pair: &PairSpec,
        message: &'r str,
        severity: Severity,
        options: Value,
        decl_span: Span,
    ) -> Result<Self, Finding> {
        let parse = |key, text: &str| parse_predicate(&rule.id, key, text, decl_span);
        Ok(Self {
            id: &rule.id,
            code: diagnostic_code(&rule.id),
            severity,
            message,
            options,
            open: parse("open", &pair.open)?,
            close: parse("close", &pair.close)?,
            inside: pair
                .inside
                .as_deref()
                .map(|text| parse("inside", text))
                .transpose()?,
            frame: match pair.scope {
                PairScope::Body => Frame::Body,
                PairScope::Section => Frame::Scope,
                PairScope::Document => Frame::Document,
            },
            scope: pair.scope,
            slots: Vec::new(),
            failed: false,
        })
    }

    /// A frame of `frame`'s kind begins: this rule's next slot, when it is
    /// the kind the rule balances in.
    pub(crate) fn frame_start(&mut self, frame: Frame, tally: &mut Tally<'_>) {
        if self.failed || frame != self.frame {
            return;
        }
        self.slots.push(None);
        tally.counts.pair_slots += 1;
    }

    /// A frame of `frame`'s kind ends: an open slot is an unclosed open.
    pub(crate) fn frame_end(&mut self, frame: Frame, tally: &mut Tally<'_>) {
        if self.failed || frame != self.frame {
            return;
        }
        let slot = self
            .slots
            .pop()
            .expect("the walk ends only the frames it began");
        if let Some(open) = slot {
            let fault = PairFault::Unclosed(self.scope);
            tally
                .out
                .push(self.finding(format!("{} {fault}", open.message), open.span, None));
        }
    }

    /// Balance `row` against the innermost slot.
    pub(crate) fn row(&mut self, row: &NodeRow<'_>, scene: Option<&Value>, tally: &mut Tally<'_>) {
        if self.failed {
            return;
        }
        let mut env = Env::new()
            .with_record("node", row)
            .with_ref("options", &self.options);
        if let Some(scene) = scene {
            env = env.with_ref("scene", scene);
        }
        let step = self.step(row, &env, tally);
        tally.counts.eval_chars += env.chars_scanned();
        let slot = self
            .slots
            .last_mut()
            .expect("every row is inside a frame of each kind");
        match step {
            Ok(Step::Pass) => {}
            Ok(Step::Open(open)) => {
                *slot = Some(open);
                tally.counts.pair_slots += 1;
            }
            Ok(Step::Close) => {
                *slot = None;
                tally.counts.pair_slots += 1;
            }
            Ok(Step::Fault(finding)) => tally.out.push(finding),
            Err(failure) => {
                self.failed = true;
                tally
                    .out
                    .push(expr_error(self.id, failure, tally.decl_span));
            }
        }
    }

    /// What `row` does to the innermost slot.
    fn step(
        &self,
        row: &NodeRow<'_>,
        env: &Env<'_>,
        tally: &mut Tally<'_>,
    ) -> Result<Step, String> {
        let opened = self
            .slots
            .last()
            .expect("every row is inside a frame of each kind")
            .as_ref()
            .map(|open| open.span);
        let fault = match opened {
            Some(opened) => {
                if self.holds("close", &self.close, env, tally)? {
                    return Ok(Step::Close);
                }
                if self.holds("open", &self.open, env, tally)? {
                    PairFault::Nested { line: opened.line }
                } else if let Some(inside) = &self.inside {
                    if self.holds("inside", inside, env, tally)? {
                        return Ok(Step::Pass);
                    }
                    PairFault::Inside { line: opened.line }
                } else {
                    return Ok(Step::Pass);
                }
            }
            None => {
                if self.holds("open", &self.open, env, tally)? {
                    return Ok(Step::Open(Open {
                        span: row.span(),
                        message: render_message(self.message, env),
                    }));
                }
                if !self.holds("close", &self.close, env, tally)? {
                    return Ok(Step::Pass);
                }
                PairFault::Unopened(self.scope)
            }
        };
        let message = format!("{} {fault}", render_message(self.message, env));
        let related = opened.map(|opened| RelatedDiagnostic {
            file: tally.file.to_string(),
            diagnostic: Diagnostic {
                code: self.code.clone(),
                severity: self.severity,
                message: "the pair opened here".to_string(),
                evidence: None,
                span: opened,
                layer: Layer::Content,
                fixits: Vec::new(),
                provenance: None,
                covered: Vec::new(),
                related: Vec::new(),
            },
        });
        Ok(Step::Fault(self.finding(message, row.span(), related)))
    }

    /// Evaluate one predicate: its value, or the `E-LINT-EXPR` reason.
    fn holds(
        &self,
        key: &str,
        predicate: &IdedExpr,
        env: &Env<'_>,
        tally: &mut Tally<'_>,
    ) -> Result<bool, String> {
        tally.counts.pair_evals += 1;
        match eval(&predicate.expr, env) {
            Ok(Value::Bool(b)) => Ok(b),
            Ok(other) => Err(format!("`{key}` returned non-bool: {other:?}")),
            Err(e) => Err(format!("`{key}`: {}", e.message)),
        }
    }

    fn finding(&self, message: String, span: Span, related: Option<RelatedDiagnostic>) -> Finding {
        Finding {
            code: self.code.clone(),
            severity: self.severity,
            message,
            span,
            related: related.into_iter().collect(),
            layer: None,
        }
    }
}

/// What a row does to its slot.
#[derive(Debug)]
enum Step {
    /// Nothing.
    Pass,
    /// Opens a pair.
    Open(Open),
    /// Closes the open pair.
    Close,
    /// A nested open, an inside fault or an unopened close.
    Fault(Finding),
}
