//! Lute scenario-DSL front end: lexing, parsing, and the syntax-layer AST for
//! `.lute` documents.
//!
//! Implements the syntactic layers of the **Lute Scenario DSL 0.1.0**. The
//! normative language of record — the source of truth for grammar, semantics,
//! and the diagnostics registry — is `docs/proposals/scenario-dsl/0.1.0.md`
//! (it supersedes 0.0.1 wholesale). This crate owns the lexical structure
//! (§4), document structure (§6), and node grammar (§7); the static-semantic
//! checks (§8–§13) live in `lute-check`.
//!
//! Module map:
//! - [`lex`] — frontmatter peel (§6.1), block/line comment stripping (§4.2),
//!   and line/attribute scanning terminals (§4.4–§4.5).
//! - [`parser`] — §4.3 line-classification precedence + recursive block
//!   assembly (§7.3–§7.4), producing an [`ast::Document`] plus parse
//!   diagnostics.
//! - [`ast`] — the syntax-layer AST node types.
//! - [`cel_ast`] — the embedded Lute-CEL expression AST (§8.4).
//! - [`walk`] — a read-only AST visitor.
//!
//! **Diagnostics are spec-owned.** Each `pub const E_*` code in [`parser`]
//! cites the § that defines it; the normative catalogue (per-section, plus the
//! registry delta in Appendix D) lives in the spec, not in this crate.

pub mod ast;
pub mod cel_ast;
pub mod lex;
pub mod parser;
pub mod walk;

pub use ast::scan_label_interps;
pub use parser::parse;
