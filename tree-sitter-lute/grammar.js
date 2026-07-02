/**
 * tree-sitter-lute — grammar for the fixed Lute Scenario DSL (dsl §4–7).
 *
 * EDITOR-SIDE ONLY. This grammar is the syntax-highlighting / folding host for
 * `.lute` files; it is NOT the authoritative AST (that is `lute-syntax`'s
 * hand-written classifier). It only recognizes the grammar's *shapes* well
 * enough for editor features, mirroring the §4.3 line classification:
 *
 *   1. `## ` shot heading / `# ` document title      (§6.2, §6.3)
 *   2. `::set{ … }` assignment directive             (§7.3.4)  — tried before `::`
 *   3. `::`ident`{ … }` staging directive (leaf)      (§7.2)
 *   4. `:line[speaker]{attrs}: text` content line     (§7.1)   — text OPAQUE to EOL
 *   5. `<tag …> … </tag>` logic / timeline BLOCKS     (§7.3, §7.4) — these NEST
 *   6. `/* … *​/` comments are trivia                  (§4.2)   — `extras`
 *
 * Frontmatter (`---` YAML `---`, §6.1) is an opaque leaf recognized by the
 * external scanner (its delimiter-to-delimiter body can't be matched by a
 * tree-sitter regex because a body line may itself look like a delimiter).
 * Quoted `String`/`CelString` values are opaque tokens, so a `<`/`{`/`:` inside
 * them is content, not structure (§4.4).
 */

module.exports = grammar({
  name: "lute",

  // Trivia (§4.1–4.2): blank lines/whitespace and `/* … */` comments are not
  // nodes of the grammar; comments are a named extra so highlighters can color
  // them, but they float outside the structural tree.
  extras: ($) => [/[ \t\r\n]/, $.comment],

  externals: ($) => [$.frontmatter],

  rules: {
    // Document ::= Meta? DocItem*  (§6). The corpus permits bare nodes/title at
    // the top level (a directive with no enclosing shot), then shot blocks that
    // greedily absorb the rest. Splitting "pre-shot items" from "shots" removes
    // the shift/reduce ambiguity of a node that could attach to either a shot
    // body or the top level.
    source_file: ($) =>
      seq(optional($.frontmatter), repeat($._pre_item), repeat($.shot)),

    // Items legal before the first shot heading: the document title and bare
    // nodes (the corpus' top-level directive lives here).
    _pre_item: ($) => choice($.title, $._node),

    // ---- headings ----------------------------------------------------------
    // Title ::= "# " Text (§6.2). Text opaque to EOL.
    title: ($) => seq("#", $.text),

    // ShotBlock ::= ShotHeading Node* (§6.3). Heading text opaque to EOL; the
    // body greedily absorbs nodes until the next `## ` heading or EOF.
    shot: ($) => seq("##", $.text, repeat($._node)),

    // A body Node (§7). NB: no `title` here — a `# ` inside a shot ends it.
    _node: ($) =>
      choice(
        $.set,
        $.directive,
        $.line,
        $.branch,
        $.match,
        $.timeline,
      ),

    // ---- staging (leaf) ----------------------------------------------------
    // Set ::= "::set{" Path WS AssignOp WS CelExpr "}" (§7.3.4).
    set: ($) =>
      seq(
        alias("::set{", "::set{"),
        $.path,
        $.assign_op,
        optional($.cel_expr),
        "}",
      ),

    // Directive ::= "::" Ident Attrs? (§7.2). Leaf — does NOT nest.
    directive: ($) => seq("::", $.ident, optional($.attrs)),

    // ---- content -----------------------------------------------------------
    // Line ::= ":line[" Speaker "]" Attrs? ":" WS Text (§7.1). Text opaque.
    line: ($) =>
      seq(
        ":line[",
        $.speaker,
        "]",
        optional($.attrs),
        ":",
        optional($.text),
      ),

    // ---- logic blocks (nest) ----------------------------------------------
    // Branch ::= "<branch" Attrs ">" Choice+ "</branch>" (§7.3).
    branch: ($) =>
      seq(
        "<branch",
        repeat($._tag_attr),
        ">",
        repeat($.choice),
        "</branch>",
      ),

    // Choice ::= "<choice" Attrs ">" Node* "</choice>" (§7.3).
    choice: ($) =>
      seq(
        "<choice",
        repeat($._tag_attr),
        ">",
        repeat($._node),
        "</choice>",
      ),

    // Match ::= "<match" Attrs ">" When+ Otherwise? "</match>" (§7.3, §11.2).
    match: ($) =>
      seq(
        "<match",
        repeat($._tag_attr),
        ">",
        repeat($.when),
        optional($.otherwise),
        "</match>",
      ),

    // When ::= "<when" Attrs ">" Node* "</when>" (§7.3).
    when: ($) =>
      seq("<when", repeat($._tag_attr), ">", repeat($._node), "</when>"),

    // Otherwise ::= "<otherwise>" Node* "</otherwise>" (§7.3).
    otherwise: ($) =>
      seq("<otherwise", repeat($._tag_attr), ">", repeat($._node), "</otherwise>"),

    // ---- timeline (nest, restricted body) ---------------------------------
    // Timeline ::= "<timeline" Attrs? ">" Track+ "</timeline>" (§7.4).
    timeline: ($) =>
      seq(
        "<timeline",
        repeat($._tag_attr),
        ">",
        repeat($.track),
        "</timeline>",
      ),

    // Track ::= "<track" Attrs ">" Clip+ "</track>" (§7.4). Clip = Directive|Set.
    track: ($) =>
      seq(
        "<track",
        repeat($._tag_attr),
        ">",
        repeat(choice($.directive, $.set)),
        "</track>",
      ),

    // ---- attributes (§4.5) -------------------------------------------------
    // Attrs ::= "{" ( Attr ( WS Attr )* )? "}"  — the brace-delimited form used
    // by `:line` and `::` directives. Tag attributes reuse `_tag_attr` directly.
    attrs: ($) => seq("{", repeat($._tag_attr), "}"),

    // An attribute in any position (brace-form or bare tag attribute). Splitting
    // the CEL-valued keys (`on`/`test`/`when`, §7.3/§8) into their own node lets
    // editor queries reach the CEL sub-tokens (@ref, state-path) inside their
    // value; every other key is a plain String/Ref attribute (§4.5).
    _tag_attr: ($) => choice($.attr, $.cel_attr),

    // Attr ::= Ident "=" String | Ident "=" Ref | Ident  (bare ⇒ true).
    attr: ($) =>
      seq(
        $.key,
        optional(seq("=", choice($.string, $.ref))),
      ),

    // CelAttr ::= CelKey "=" ( CelString | Ref )  — the CEL-valued attributes
    // `<match on>`, `<when test>`, `<choice when>` (§7.3, §11.1–11.2). The value
    // is a CEL expression (§8): a double-quoted `CelString` (§4.4) or a bare
    // `@ref` macro (§8.1). Distinct from `attr` so highlight/tag queries can
    // capture the CEL innards (@ref, state-path) rather than an opaque string.
    cel_attr: ($) => seq($.cel_key, "=", choice($.cel_string, $.ref)),

    // CelKey — the reserved attribute keys whose value is CEL (§7.3): `on` is a
    // `<match>` subject, `test` a `<when>` guard, `when` a `<choice>` guard. A
    // named node (lexes ahead of the generic `key` on a tie) so editors treat
    // these keys distinctly and know their value is embedded CEL.
    cel_key: ($) => choice("on", "test", "when"),

    // CelString (§4.4) — a double-quoted CEL expression used as an attribute
    // value. Unlike the opaque `string` token, its interior is *structured* so
    // editor queries can capture the embedded CEL sub-tokens: `@ref` macros
    // (§8.1) and dotted state-`path`s (§9). CEL's own single-quoted string
    // literals (`'blunt'`) are opaque runs (§4.4 quote boundaries respected), so
    // a `@`, letter, or `}` inside `'…'` is content, not a ref/path/terminator.
    // Every interior piece is `token.immediate`, so the value can neither skip
    // whitespace/comments (an `extra`) nor span a newline: a missing closing `"`
    // fails locally instead of swallowing following lines.
    cel_string: ($) =>
      seq(
        '"',
        repeat(
          choice(
            // `@name` / `@name(args)` ref macro (§8.1) — outside CEL strings.
            alias(token.immediate(/@[A-Za-z][A-Za-z0-9_-]*(\([^)\n]*\))?/), $.ref),
            // Dotted state path (§9), e.g. `scene.choices.number`.
            alias(
              token.immediate(/[A-Za-z][A-Za-z0-9_]*(\.[A-Za-z][A-Za-z0-9_]*)+/),
              $.path,
            ),
            // CEL single-quoted string literal — opaque (with `\` escapes).
            $._cel_squote,
            // Bare CEL identifier / keyword (no dot ⇒ not a path), e.g. `in`.
            $._cel_word,
            // Everything else: operators, spaces, digits, brackets, escapes.
            $._cel_sym,
          ),
        ),
        token.immediate('"'),
      ),

    // A CEL single-quoted string literal, consumed whole so its interior is
    // content (§4.4). Backslash escapes; no raw newline.
    _cel_squote: ($) => token.immediate(/'([^'\\\n]|\\[^\n])*'/),

    // A bare CEL identifier/keyword inside a `cel_string` (no `.` ⇒ not a path).
    _cel_word: ($) => token.immediate(/[A-Za-z_][A-Za-z0-9_]*/),

    // Filler inside a `cel_string`: any run that is not the start of a ref,
    // path, word, single-quote literal, or the closing `"` — and never a raw
    // newline (so the value stays single-line). Backslash escapes stay attached.
    _cel_sym: ($) => token.immediate(/([^"'@A-Za-z\r\n\\]|\\[^\n])+/),

    // ---- terminals (§4.4) --------------------------------------------------
    // Ident ::= [A-Za-z][A-Za-z0-9_-]*  (directive/tag name).
    ident: ($) => /[A-Za-z][A-Za-z0-9_-]*/,

    // Attribute key — lexically an Ident; a distinct node name so editors can
    // treat attribute keys and directive names differently.
    key: ($) => /[A-Za-z][A-Za-z0-9_-]*/,

    // Speaker ::= Ident (§7.1) — a character id (incl. reserved narrator/pov).
    speaker: ($) => /[A-Za-z][A-Za-z0-9_-]*/,

    // String / CelString (§4.4): double-quoted, backslash escapes, no raw
    // newline. CEL strings use single quotes internally, so a `'x'` inside is
    // content; a `<`/`{`/`:` inside is content too (quote boundaries respected).
    string: ($) => token(/"([^"\\\n]|\\[^\n])*"/),

    // Ref ::= "@" Ident ( "(" CelArgs ")" )?  — bare (unquoted) attribute ref.
    ref: ($) => token(/@[A-Za-z][A-Za-z0-9_-]*(\([^)\n]*\))?/),

    // Path ::= ("scene"|"run"|"user"|"app") ("." Ident)+  (§9). Editor-side we
    // accept any dotted ident path; the checker validates the root + declares.
    path: ($) => token(/[A-Za-z][A-Za-z0-9_]*(\.[A-Za-z][A-Za-z0-9_]*)+/),

    // AssignOp ::= "=" | "+=" | "-=" | "*="  (§7.3.4). A token, not a value.
    assign_op: ($) => choice("=", "+=", "-=", "*="),

    // CelExpr — the `::set` right-hand side, opaque to the closing `}` of the
    // set. Quoted-string boundaries are respected before structural scanning
    // (§4.4): a `}` inside a double-quoted `CelString` OR inside a CEL
    // single-quoted literal (`'a}b'`) is content, not the terminator. Both quote
    // forms carry backslash escapes and MUST NOT span a raw newline.
    cel_expr: ($) =>
      token(
        /([^"'}\n]|"([^"\\\n]|\\[^\n])*"|'([^'\\\n]|\\[^\n])*')+/,
      ),

    // Text ::= rest of line, verbatim, to EOL (§4.4). OPAQUE: `(`,`?`,`<`,`:`,
    // quotes are not special. `token.immediate` keeps `extras` (the newline)
    // from being skipped into the next line, and the leading space after `: `
    // is consumed as part of the opaque run.
    text: ($) => token.immediate(/[ \t]*[^ \t\r\n][^\r\n]*/),

    // Comment ::= "/*" … "*/"  (§4.2). Trivia (an `extra`); does not nest.
    comment: ($) => token(seq("/*", /[^*]*\*+([^/*][^*]*\*+)*/, "/")),
  },
});
