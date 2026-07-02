//! `textDocument/semanticTokens/full` (Task 6.4).
//!
//! A pure function over a parsed [`Document`] (plus the backend's
//! [`lute_core_span::TextIndex`]) that classifies each syntactic span by the
//! architecture's three LAYERS — kept DISTINCT — plus the CEL sub-tokens the
//! logic layer nests:
//!
//! | token type  | what carries it (classified STRUCTURALLY, by node kind)      |
//! |-------------|--------------------------------------------------------------|
//! | `content`   | a `:line` speaker + its text (the §7.1 content node)          |
//! | `staging`   | a `::directive` tag, `<timeline>` / `<track>` open keywords  |
//! | `logic`     | `::set` / `<branch>` / `<match>` open keywords               |
//! | `cel`       | a CEL literal / bare token inside a slot (and the `$` subject)|
//! | `ref`       | an `@ref` inside a CEL slot                                   |
//! | `statePath` | a `::set` target path + a state/choice path inside a slot    |
//!
//! The layer of a construct follows its node kind, never a snapshot lookup: a
//! `::camera` tag is Staging because [`Directive`](lute_syntax::ast::Directive)
//! is a staging construct, regardless of what `camera` resolves to. CEL slots are
//! sub-classified: an `@ref` is `ref`, a state/choice path is `statePath`, and
//! everything else in the slot is `cel`.
//!
//! ## Encoding
//! The result is the LSP DELTA encoding: tokens sorted by `(line, startChar)`,
//! each carrying `deltaLine` / `deltaStartChar` relative to the previous token
//! (the first relative to `0,0`), a UTF-16 `length`, the legend index of its
//! type, and a zero modifier bitset. Positions and lengths come from the shared
//! [`lute_core_span::TextIndex`] — the same UTF-16 accounting diagnostics use —
//! so the semantic surface never drifts a code unit from the headless one.
//!
//! ## Known limitation (shared with `nav`/`references`)
//! CEL sub-tokens are found by a lexical scan of the RAW slot text (never a CEL
//! re-parse — cel-parser drops positions on success). An identifier inside a
//! string literal (`$ == 'gold'`) is therefore tokenized as `cel`; this matches
//! the DSL-level scanning [`crate::features::path_tokens`] / [`lute_cel::scan_refs`]
//! already do for reference collection, and is cosmetic (highlight-only).

use lute_cel::scan_refs;
use lute_core_span::TextIndex;
use lute_syntax::ast::{Arm, ClipNode, Directive, Document, Node, Set};
use tower_lsp_server::ls_types::{SemanticToken, SemanticTokenType, SemanticTokensLegend};

use super::{all_slots, is_state_path, path_tokens};

/// The token types this server emits, in legend order. The index of a variant in
/// this list is its wire `tokenType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokType {
    /// Content layer: `:line` speaker + text.
    Content,
    /// Staging layer: `::directive` tags, `<timeline>`/`<track>` keywords.
    Staging,
    /// Logic layer: `::set` / `<branch>` / `<match>` keywords.
    Logic,
    /// A plain CEL token (literal / operator-adjacent identifier / `$`).
    Cel,
    /// An `@ref` inside a CEL slot.
    Ref,
    /// A state (`scene.`/`run.`/`user.`/`app.`) or `scene.choices.<id>` path.
    StatePath,
}

impl TokType {
    /// The legend index (wire `tokenType`) of this variant.
    pub fn index(self) -> u32 {
        match self {
            TokType::Content => 0,
            TokType::Staging => 1,
            TokType::Logic => 2,
            TokType::Cel => 3,
            TokType::Ref => 4,
            TokType::StatePath => 5,
        }
    }

    /// The legend's `SemanticTokenType` name for this variant.
    fn type_name(self) -> SemanticTokenType {
        match self {
            TokType::Content => SemanticTokenType::new("content"),
            TokType::Staging => SemanticTokenType::new("staging"),
            TokType::Logic => SemanticTokenType::new("logic"),
            TokType::Cel => SemanticTokenType::new("cel"),
            TokType::Ref => SemanticTokenType::new("ref"),
            TokType::StatePath => SemanticTokenType::new("statePath"),
        }
    }
}

/// The full set of token types, legend order. Kept in sync with [`TokType`].
const LAYERS: [TokType; 6] = [
    TokType::Content,
    TokType::Staging,
    TokType::Logic,
    TokType::Cel,
    TokType::Ref,
    TokType::StatePath,
];

/// The legend advertised in `initialize` and used to decode the wire tokens: the
/// closed set of [`TokType`] names, with no modifiers.
pub fn legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: LAYERS.iter().map(|t| t.type_name()).collect(),
        token_modifiers: Vec::new(),
    }
}

/// A classified token as raw document byte offsets, before UTF-16 / delta encoding.
#[derive(Clone, Copy, Debug)]
struct RawTok {
    start: usize,
    end: usize,
    ty: TokType,
}

/// An absolute (pre-delta) token position, in 0-based line + 0-based UTF-16 char.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AbsTok {
    line: u32,
    ch: u32,
    len: u32,
    ty: u32,
}

/// Classify `doc` into DELTA-encoded [`SemanticToken`]s (LSP wire order).
pub fn semantic_tokens(doc: &Document, idx: &TextIndex) -> Vec<SemanticToken> {
    let mut raw = Vec::new();
    for shot in &doc.shots {
        walk_nodes(&shot.body, &mut raw);
    }
    // CEL sub-tokens: every slot's `@ref`s, state paths, and plain tokens.
    for slot in all_slots(doc) {
        slot_tokens(slot.span.byte_start, &slot.raw, &mut raw);
    }
    delta_encode(to_absolute(raw, idx))
}

/// Emit structural tokens for a body's nodes (recursing into nested blocks).
fn walk_nodes(nodes: &[Node], out: &mut Vec<RawTok>) {
    for node in nodes {
        match node {
            Node::Line(l) => {
                // Speaker sits just past the fixed `:line[` prefix (6 bytes).
                let sp_start = l.span.byte_start + ":line[".len();
                push(out, sp_start, sp_start + l.speaker.len(), TokType::Content);
                if !l.text.is_empty() {
                    push(
                        out,
                        l.text_span.byte_start,
                        l.text_span.byte_end,
                        TokType::Content,
                    );
                }
            }
            Node::Directive(d) => directive_tag(d, out),
            Node::Set(s) => set_tokens(s, out),
            Node::Branch(b) => {
                // `<branch` open keyword.
                push(
                    out,
                    b.span.byte_start,
                    b.span.byte_start + "<branch".len(),
                    TokType::Logic,
                );
                for c in &b.choices {
                    // `<choice` open keyword (the arm opener; §7.3 logic layer).
                    push(
                        out,
                        c.span.byte_start,
                        c.span.byte_start + "<choice".len(),
                        TokType::Logic,
                    );
                    walk_nodes(&c.body, out);
                }
            }
            Node::Match(m) => {
                push(
                    out,
                    m.span.byte_start,
                    m.span.byte_start + "<match".len(),
                    TokType::Logic,
                );
                for arm in &m.arms {
                    match arm {
                        Arm::When { body, span, .. } => {
                            // `<when` open keyword.
                            push(
                                out,
                                span.byte_start,
                                span.byte_start + "<when".len(),
                                TokType::Logic,
                            );
                            walk_nodes(body, out);
                        }
                        Arm::Otherwise { body, span } => {
                            // `<otherwise` open keyword.
                            push(
                                out,
                                span.byte_start,
                                span.byte_start + "<otherwise".len(),
                                TokType::Logic,
                            );
                            walk_nodes(body, out);
                        }
                    }
                }
            }
            Node::Timeline(t) => {
                push(
                    out,
                    t.span.byte_start,
                    t.span.byte_start + "<timeline".len(),
                    TokType::Staging,
                );
                for track in &t.tracks {
                    push(
                        out,
                        track.span.byte_start,
                        track.span.byte_start + "<track".len(),
                        TokType::Staging,
                    );
                    for clip in &track.clips {
                        match &clip.node {
                            ClipNode::Directive(d) => directive_tag(d, out),
                            ClipNode::Set(s) => set_tokens(s, out),
                        }
                    }
                }
            }
        }
    }
}

/// The `::name` head of a directive → one Staging token (`::` + tag).
fn directive_tag(d: &Directive, out: &mut Vec<RawTok>) {
    let start = d.span.byte_start;
    push(
        out,
        start,
        start + "::".len() + d.tag.len(),
        TokType::Staging,
    );
}

/// A `::set` → the `::set` keyword (Logic) + its target path (StatePath). The
/// RHS CEL expr is handled by the slot walk.
fn set_tokens(s: &Set, out: &mut Vec<RawTok>) {
    let start = s.span.byte_start;
    push(out, start, start + "::set".len(), TokType::Logic);
    push(
        out,
        s.path_span.byte_start,
        s.path_span.byte_end,
        TokType::StatePath,
    );
}

/// Sub-classify one CEL slot: `@ref`s (Ref), state/choice paths (StatePath), and
/// the remaining bare tokens (Cel). `base` is the slot's document byte offset;
/// `raw` is its verbatim source (scanned, never re-parsed).
fn slot_tokens(base: usize, raw: &str, out: &mut Vec<RawTok>) {
    let mut ref_ranges: Vec<(usize, usize)> = Vec::new();
    for r in scan_refs(raw) {
        let s = base + r.span.byte_start;
        let e = base + r.span.byte_end;
        ref_ranges.push((s, e));
        // The `$` subject is a plain CEL token; a named `@ref` is a ref.
        let ty = if r.is_dollar {
            TokType::Cel
        } else {
            TokType::Ref
        };
        push(out, s, e, ty);
    }
    for (name, (ps, pe)) in path_tokens(raw) {
        let s = base + ps;
        let e = base + pe;
        // Skip the bare name inside an `@ref` (`fond` within `@fond`), already
        // emitted as a Ref above.
        if ref_ranges.iter().any(|&(rs, re)| s < re && rs < e) {
            continue;
        }
        let ty = if is_state_path(&name) {
            TokType::StatePath
        } else {
            TokType::Cel
        };
        push(out, s, e, ty);
    }
}

/// Record a token spanning `[start, end)` bytes, dropping the empty range.
fn push(out: &mut Vec<RawTok>, start: usize, end: usize, ty: TokType) {
    if end > start {
        out.push(RawTok { start, end, ty });
    }
}

/// Map raw byte tokens to 0-based line + UTF-16 positions via `idx`. A token that
/// would span more than one line is dropped: LSP semantic tokens are single-line
/// (the `length` field is a within-line UTF-16 count).
fn to_absolute(raw: Vec<RawTok>, idx: &TextIndex) -> Vec<AbsTok> {
    let mut out = Vec::with_capacity(raw.len());
    for t in raw {
        let sp = idx.position(t.start);
        let ep = idx.position(t.end);
        if sp.line != ep.line {
            continue;
        }
        out.push(AbsTok {
            line: sp.line - 1,
            ch: sp.utf16_col,
            len: ep.utf16_col - sp.utf16_col,
            ty: t.ty.index(),
        });
    }
    out
}

/// Sort by `(line, char)` and DELTA-encode: each token's line/char are stored
/// relative to the previous emitted token (the first relative to `0,0`).
fn delta_encode(mut toks: Vec<AbsTok>) -> Vec<SemanticToken> {
    toks.sort_by_key(|t| (t.line, t.ch));
    let mut out = Vec::with_capacity(toks.len());
    let (mut prev_line, mut prev_char) = (0u32, 0u32);
    for t in toks {
        let delta_line = t.line - prev_line;
        let delta_start = if delta_line == 0 {
            t.ch - prev_char
        } else {
            t.ch
        };
        out.push(SemanticToken {
            delta_line,
            delta_start,
            length: t.len,
            token_type: t.ty,
            token_modifiers_bitset: 0,
        });
        prev_line = t.line;
        prev_char = t.ch;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use lute_syntax::parse;

    const BIANCA: &str = include_str!("../../../../docs/examples/bianca-s01ep02.lute");

    fn tokens(text: &str) -> Vec<SemanticToken> {
        let (doc, _) = parse(text);
        semantic_tokens(&doc, &TextIndex::new(text))
    }

    /// Decode the delta stream back to absolute `(line, char, len, ty)` tuples,
    /// so tests can look a token up by position.
    fn decode(toks: &[SemanticToken]) -> Vec<(u32, u32, u32, u32)> {
        let mut out = Vec::with_capacity(toks.len());
        let (mut line, mut ch) = (0u32, 0u32);
        for t in toks {
            if t.delta_line == 0 {
                ch += t.delta_start;
            } else {
                line += t.delta_line;
                ch = t.delta_start;
            }
            out.push((line, ch, t.length, t.token_type));
        }
        out
    }

    /// The legend index of a named token type, for readable assertions.
    fn ty(name: &str) -> u32 {
        legend()
            .token_types
            .iter()
            .position(|t| t.as_str() == name)
            .unwrap() as u32
    }

    /// ACCEPTANCE: a `::camera` directive tag carries the STAGING layer.
    #[test]
    fn camera_directive_tag_is_staging() {
        let idx = TextIndex::new(BIANCA);
        // `::camera{` (with brace) matches only real directives; a bare `::camera`
        // also appears inside the file's block comment, which the parser blanks.
        let cam = BIANCA.find("::camera{").unwrap();
        let p = idx.position(cam);
        let (want_line, want_ch) = (p.line - 1, p.utf16_col);
        let decoded = decode(&tokens(BIANCA));
        let tok = decoded
            .iter()
            .find(|&&(l, c, _, _)| l == want_line && c == want_ch)
            .expect("a token anchored on the ::camera tag");
        assert_eq!(
            tok.3,
            ty("staging"),
            "::camera tag must be the staging layer"
        );
        assert_eq!(tok.2, "::camera".len() as u32, "token covers `::camera`");
    }

    /// ACCEPTANCE (added): an `@ref` gets the distinct `ref` type and a state path
    /// the distinct `statePath` type — the two CEL sub-token classes.
    #[test]
    fn ref_and_state_path_get_distinct_types() {
        let text = "---\ncharacter: bianca\nseason: 1\nepisode: 2\nstate:\n  scene.affect.bianca: { type: number, default: 0 }\ndefs:\n  fond: { type: bool, cel: \"scene.affect.bianca >= 1\" }\n---\n## Shot 1.\n<match on=\"scene.choices.number\">\n  <when test=\"@fond\">\n    :line[f]: a.\n  </when>\n  <otherwise>\n    :line[f]: b.\n  </otherwise>\n</match>\n";
        let idx = TextIndex::new(text);
        let decoded = decode(&tokens(text));

        // `@fond` → ref.
        let at = idx.position(text.find("@fond").unwrap());
        let ref_tok = decoded
            .iter()
            .find(|&&(l, c, _, _)| l == at.line - 1 && c == at.utf16_col)
            .expect("token on @fond");
        assert_eq!(ref_tok.3, ty("ref"), "@fond is a ref");
        assert_eq!(ref_tok.2, "@fond".len() as u32);

        // `scene.choices.number` (match subject path) → statePath.
        let sp = idx.position(text.find("scene.choices.number").unwrap());
        let path_tok = decoded
            .iter()
            .find(|&&(l, c, _, _)| l == sp.line - 1 && c == sp.utf16_col)
            .expect("token on the match subject path");
        assert_eq!(
            path_tok.3,
            ty("statePath"),
            "scene.choices.number is a state path"
        );
        assert_eq!(path_tok.2, "scene.choices.number".len() as u32);
    }

    /// S2: the logic layer must tokenize the `<choice>`/`<when>`/`<otherwise>`
    /// opening keywords, not only the enclosing `<branch>`/`<match>`. Otherwise
    /// the arm openers go untokenized and the logic layer is under-highlighted.
    #[test]
    fn choice_and_arm_openers_are_logic_tokens() {
        let text = "## Shot 1.\n<branch id=\"b\">\n<choice id=\"c\" label=\"L\">\n:line[f]: a.\n</choice>\n</branch>\n<match on=\"scene.x\">\n<when test=\"$ == 1\">\n:line[f]: b.\n</when>\n<otherwise>\n:line[f]: c.\n</otherwise>\n</match>\n";
        let idx = TextIndex::new(text);
        let decoded = decode(&tokens(text));
        for (kw, len) in [("<choice", 7u32), ("<when", 5), ("<otherwise", 10)] {
            let p = idx.position(text.find(kw).unwrap());
            let tok = decoded
                .iter()
                .find(|&&(l, c, _, _)| l == p.line - 1 && c == p.utf16_col)
                .unwrap_or_else(|| panic!("no token on `{kw}` opener"));
            assert_eq!(tok.3, ty("logic"), "`{kw}` opener must be a logic token");
            assert_eq!(tok.2, len, "`{kw}` token covers the keyword");
        }
    }

    /// The legend is the closed six-type set, indices matching [`TokType::index`].
    #[test]
    fn legend_is_the_six_layer_set() {
        let l = legend();
        assert_eq!(
            l.token_types
                .iter()
                .map(SemanticTokenType::as_str)
                .collect::<Vec<_>>(),
            ["content", "staging", "logic", "cel", "ref", "statePath"],
        );
        assert!(l.token_modifiers.is_empty());
        assert_eq!(TokType::StatePath.index(), 5);
    }

    /// Delta math on a hand-built two-token case: same-line tokens delta the
    /// char; a later-line token resets the char to absolute.
    #[test]
    fn delta_encoding_math() {
        let toks = vec![
            AbsTok {
                line: 0,
                ch: 5,
                len: 3,
                ty: 1,
            },
            AbsTok {
                line: 0,
                ch: 12,
                len: 2,
                ty: 4,
            }, // same line: ds = 12 - 5
            AbsTok {
                line: 3,
                ch: 4,
                len: 6,
                ty: 2,
            }, // new line: dl = 3, ds = 4
        ];
        let out = delta_encode(toks);
        assert_eq!(
            out[0],
            SemanticToken {
                delta_line: 0,
                delta_start: 5,
                length: 3,
                token_type: 1,
                token_modifiers_bitset: 0
            }
        );
        assert_eq!(
            out[1],
            SemanticToken {
                delta_line: 0,
                delta_start: 7,
                length: 2,
                token_type: 4,
                token_modifiers_bitset: 0
            }
        );
        assert_eq!(
            out[2],
            SemanticToken {
                delta_line: 3,
                delta_start: 4,
                length: 6,
                token_type: 2,
                token_modifiers_bitset: 0
            }
        );
    }

    /// Delta encoding sorts by position first, so out-of-order input is fine.
    #[test]
    fn delta_encoding_sorts_unordered_input() {
        let toks = vec![
            AbsTok {
                line: 2,
                ch: 0,
                len: 1,
                ty: 0,
            },
            AbsTok {
                line: 0,
                ch: 0,
                len: 1,
                ty: 0,
            },
        ];
        let out = delta_encode(toks);
        assert_eq!(out[0].delta_line, 0, "line 0 token comes first");
        assert_eq!(out[1].delta_line, 2);
    }

    /// A UTF-16 length is emitted, not a byte length. `:line` text of `café`
    /// is 5 bytes (`é` = 2) but 4 UTF-16 units — so the content token's `length`
    /// must be 4, and a preceding multibyte char must shift the `startChar` in
    /// UTF-16 units too.
    #[test]
    fn lengths_are_utf16_not_bytes() {
        let text = "## Shot 1.\n:line[π]: café\n";
        let idx = TextIndex::new(text);
        let decoded = decode(&tokens(text));

        // Speaker `π`: 2 bytes, 1 UTF-16 unit, at the `[`+1 column.
        let sp = idx.position(text.find('π').unwrap());
        let speaker = decoded
            .iter()
            .find(|&&(l, c, _, _)| l == sp.line - 1 && c == sp.utf16_col)
            .expect("token on the π speaker");
        assert_eq!(speaker.2, 1, "π is one UTF-16 unit");

        // Text `café`: 5 bytes but 4 UTF-16 units. Its startChar is measured past
        // the `π` speaker in UTF-16, not bytes.
        let tx = idx.position(text.find("café").unwrap());
        let content = decoded
            .iter()
            .find(|&&(l, c, _, _)| l == tx.line - 1 && c == tx.utf16_col)
            .expect("token on the café text");
        assert_eq!(content.2, 4, "café is four UTF-16 units");
    }
}
