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
 *   4. `:speaker{attrs}: text` content line          (§7.1)   — text, may {{…}}
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
      seq(
        optional($.frontmatter),
        repeat($._pre_item),
        repeat(choice($.shot, $.quest)),
      ),

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
        $.hub,
        $.on,
        $.objective,
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
    // Line ::= ":" Speaker Attrs? ":" WS Text (§7.1). Text MAY interpolate (§7.6).
    // The leading marker is a single `:`; `::set{` and `::` are longer tokens, so
    // maximal-munch picks the directive/set forms at a `::` boundary and a line
    // only ever starts on `:` + a non-`:` speaker.
    line: ($) =>
      seq(
        ":",
        $.speaker,
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

    // ---- hub (nest; §7.3.2) -----------------------------------------------
    // Hub ::= "<hub" Attrs ">" HubChoice+ "</hub>" (§7.3.2). A revisit
    // conversation that re-presents eligible choices. `id` required; the
    // `once`/`exit` flags and `into`/`persist`/`value`/`when` sugar all ride the
    // generic `_tag_attr` machinery (bare-bool `once`/`exit`; string/ref values)
    // — no new attr vocabulary needed.
    hub: ($) =>
      seq(
        "<hub",
        repeat($._tag_attr),
        ">",
        repeat($.hub_choice),
        "</hub>",
      ),

    // HubChoice ::= "<choice" Attrs ">" Node* "</choice>" (§7.3.2). Same surface
    // as a branch `choice`, but a distinct node so editors can tell a hub arm
    // (may carry `once`/`exit`) from a branch arm.
    hub_choice: ($) =>
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
      seq(
        "<when",
        repeat(choice($._tag_attr, $.when_is)),
        ">",
        repeat($._node),
        "</when>",
      ),

    // Otherwise ::= "<otherwise>" Node* "</otherwise>" (§7.3). NO attributes —
    // any attribute on `<otherwise>` is a parse error (§7.3, S10).
    otherwise: ($) =>
      seq("<otherwise", ">", repeat($._node), "</otherwise>"),

    // ---- <when> literal pattern (`is`, §7.3.1) -----------------------------
    // WhenIs ::= "is" "=" '"' WhenPattern '"' — the `<when is="…">` literal
    // pattern. Unlike `test` (a CEL guard ⇒ `cel_attr`), `is` is a plain String
    // whose *content* is a `|`-alternation of literals (enum member / true /
    // false / Number / `unset`, §7.3.1). Given its own node (not the generic
    // `attr` fallthrough) so editors can color each literal. `is` is a keyword
    // only inside `<when …>` — tree-sitter's per-state lexer leaves a speaker /
    // key named `is` elsewhere intact; it is NOT a `cel_key`.
    when_is: ($) => seq($.when_key, "=", $.when_pattern),

    when_key: ($) => "is",

    // WhenPattern ::= Literal ( WS? "|" WS? Literal )* (§7.3.1). The interior is
    // tokenized immediately (like `cel_string`) so a missing close `"` fails on
    // the line rather than swallowing the next.
    when_pattern: ($) =>
      seq(
        '"',
        $.when_literal,
        repeat(seq(token.immediate(/[ \t]*\|[ \t]*/), $.when_literal)),
        token.immediate('"'),
      ),

    // Literal ::= EnumMember | "true" | "false" | Number | "unset" (§7.3.1).
    // Number (§4.4) may carry a leading "-" (e.g. `-1`, `-2.5`); the first
    // alternative captures signed/decimal numerals, the second keeps bare
    // enum-member / true / false / unset identifiers (and the `a|b` shape).
    when_literal: ($) =>
      token.immediate(/-?[0-9]+(\.[0-9]+)?|[A-Za-z0-9_][A-Za-z0-9_.-]*/),

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

    // ---- quest blocks (nest; dsl 0.2.0 §6) ---------------------------------
    // Quest ::= "<quest" Attrs ">" QuestBody "</quest>" (§6.3). A DOCUMENT
    // TOP-LEVEL declaration (mirrors `shot`, not a `_node` alternative) — the
    // quest kind admits `<quest>` only at the top level.
    quest: ($) =>
      seq(
        "<quest",
        repeat($._tag_attr),
        ">",
        repeat($._node),
        "</quest>",
      ),

    // On ::= "<on" Attrs ">" Node* "</on>" (§4.1). The Event-Condition-Action
    // trigger; `event` is a plain String key (NOT CEL), `when` is the optional
    // CEL guard (routed through `cel_key`/`cel_attr` below).
    on: ($) =>
      seq(
        "<on",
        repeat($._tag_attr),
        ">",
        repeat($._node),
        "</on>",
      ),

    // Objective ::= "<objective" Attrs ">" Node* "</objective>"
    //            |  "<objective" Attrs "/>"  (§6.4) — self-closing when the
    // body is empty (the common case: an objective with no completion body).
    // The FIRST alternative to try is the self-close so the `/>` vs `>` choice
    // is LR(1)-clean (`/` never opens `_tag_attr`).
    objective: ($) =>
      choice(
        seq("<objective", repeat($._tag_attr), "/>"),
        seq("<objective", repeat($._tag_attr), ">", repeat($._node), "</objective>"),
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
    cel_key: ($) => choice("on", "test", "when", "done", "start", "fail"),

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

    // Text (§4.4/§7.1): the rest of a content line to EOL. Was one opaque token;
    // now a run of opaque text chunks and `{{…}}` interpolations (§7.6). Chunks
    // and openers are `token.immediate`, so `extras` (whitespace, comments, the
    // newline) are never skipped mid-text: a `//` or `/*` INSIDE text stays
    // literal text (Text is opaque, §4.2), text never spills onto the next line,
    // and the leading space after `: ` is kept. NB: shared by `title`/`shot`
    // headings — a heading with no `{{` yields a bare `(text)` exactly as before;
    // a heading `{{…}}` is a harmless editor over-recognition (the spec restricts
    // interpolation to content + `<choice label>`, enforced by the checker/LSP).
    text: ($) =>
      repeat1(choice($._text_chunk, $._text_special, $.escape, $.interpolation)),

    // A run of literal text: anything but a brace, a backslash, or a newline.
    // Stops at `{`/`\` so the longer `{{`/`\{{` tokens win by maximal munch.
    _text_chunk: ($) => token.immediate(/[^{\\\r\n]+/),

    // A lone `{` or `\` that does NOT open `{{`/`\{{` — literal text.
    _text_special: ($) => token.immediate(/[{\\]/),

    // Escape ::= "\{{" — a literal `{{` (renders one `{{`, §7.6). Consumed whole
    // (3 chars) so its `{{` never opens an interpolation.
    escape: ($) => token.immediate(/\\\{\{/),

    // Interp ::= "{{" WS? ( Path | Ref | ReservedToken ) WS? "}}"  (§7.6). Only
    // the three legal forms are admitted (a bare CEL expr is not, §7.6). The
    // interior is immediate (no `extras` ⇒ no newline), so an unterminated `{{`
    // fails on its line instead of swallowing the next. `ReservedToken` is
    // `userName` (the runtime player name, §7.6).
    interpolation: ($) =>
      seq(
        token.immediate("{{"),
        optional(token.immediate(/[ \t]+/)),
        choice(
          alias(
            token.immediate(/[A-Za-z][A-Za-z0-9_]*(\.[A-Za-z][A-Za-z0-9_]*)+/),
            $.path,
          ),
          alias(token.immediate(/@[A-Za-z][A-Za-z0-9_-]*(\([^)\n]*\))?/), $.ref),
          alias(token.immediate("userName"), $.reserved),
        ),
        optional(token.immediate(/[ \t]+/)),
        token.immediate("}}"),
      ),

    // Comment (§4.2). Trivia (an `extra`); neither form nests. Block `/* … */`
    // may span lines; line `//` runs to EOL. Per §4.2 a `//` is a comment only
    // line-leading — this `extra` may also match a trailing `//` after structure
    // (a harmless editor over-recognition), but a `//` INSIDE a content line's
    // opaque Text stays text (text is immediate, so `extras` are never scanned
    // there), and a `//` inside a quoted String is content (String is one token).
    comment: ($) =>
      token(
        choice(
          seq("/*", /[^*]*\*+([^/*][^*]*\*+)*/, "/"),
          seq("//", /[^\n]*/),
        ),
      ),
  },
});
