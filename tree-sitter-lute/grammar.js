/**
 * tree-sitter-lute — grammar for the fixed Lute Scenario DSL (dsl §4–7).
 *
 * EDITOR-SIDE ONLY. This grammar is the syntax-highlighting / folding host for
 * `.lute` files; it is NOT the authoritative AST (that is `lute-syntax`'s
 * hand-written classifier). It only recognizes the grammar's *shapes* well
 * enough for editor features, mirroring the §4.3 line classification:
 *
 *   1. `## ` section heading (+ optional `{#id}`)    (dsl 0.37.0 §3.1)
 *   2. `::set{ … }` assignment directive             (§7.3.4)  — tried before `::`
 *   3. `::`ident`{ … }` staging directive (leaf)      (§7.2)
 *   4. `@speaker{attrs}: text` content line          (§7.1)   — text, may carry
 *      `{{…}}` interpolations and inline modifiers  (dsl 0.37.0 §3.6)
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

  // `frontmatter` is the leading YAML envelope; `modifier_name` is the `:name`
  // head of an inline text modifier, recognized only when the next char is `[`
  // or `{` (dsl 0.37.0 §3.6) — a lookahead a DFA token cannot express.
  externals: ($) => [$.frontmatter, $.modifier_name],

  rules: {
    // Document ::= Meta? DocItem*  (§6). The corpus permits bare nodes at the
    // top level (a directive with no enclosing section — the checker owns
    // E-CONTENT-OUTSIDE-SECTION), then section blocks that greedily absorb the
    // rest. Splitting "pre-section items" from "sections" removes the
    // shift/reduce ambiguity of a node that could attach to either a section
    // body or the top level. There is no body `# title` node (dsl 0.37.0 §3.1:
    // the frontmatter `title:` is the only document title).
    source_file: ($) =>
      seq(
        optional($.frontmatter),
        repeat($._node),
        repeat(choice($.section, $.quest, $.entry, $.beat)),
      ),

    // ---- sections (dsl 0.37.0 §3.1) ----------------------------------------
    // Section ::= "## " Heading SectionId? Node*. Heading text is opaque to EOL
    // (no inline modifiers — those are content-line only, §3.6); the body
    // greedily absorbs nodes until the next `## ` heading or EOF.
    section: ($) =>
      seq("##", $.heading, optional($.section_id), repeat($._node)),

    // Heading text. Runs of literal text plus `{{…}}` interpolations (a heading
    // interpolation is a harmless editor over-recognition; the checker owns it).
    heading: ($) =>
      repeat1(choice($._heading_chunk, $._heading_special, $.escape, $.interpolation)),

    // A run of heading text: anything but a brace, a backslash, or a newline.
    // `prec(1)` beats the `comment` extra, as for content `_text_chunk`.
    _heading_chunk: ($) => token.immediate(prec(1, /[^{\\\r\n]+/)),

    // A lone `{` or `\` in a heading that opens neither `{{`, `{#`, nor an escape.
    _heading_special: ($) => token.immediate(/[{\\]/),

    // SectionId ::= "{#" Token "}" — the optional final heading suffix
    // (§3.1). One token; the 64-char bound and uniqueness are the checker's.
    section_id: ($) => token.immediate(/\{#[A-Za-z][A-Za-z0-9_-]*\}/),

    // A body Node (§7). A `## ` inside a section body starts the next section.
    _node: ($) =>
      choice(
        $.assert,
        $.retract,
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
        repeat($.index),
        $.assign_op,
        optional($.cel_expr),
        "}",
      ),

    // Assert ::= "::assert{" FactPattern "}" (dsl 0.3.0 §5). Leaf; ground args.
    assert: ($) =>
      seq(alias("::assert{", "::assert{"), $.fact_pattern, repeat($._tag_attr), "}"),

    // Retract ::= "::retract{" FactPattern "}" (dsl 0.3.0 §5). `_` retract-only —
    // grammar admits it in both; the CHECKER owns E-RETRACT-WILDCARD-ASSERT.
    retract: ($) =>
      seq(alias("::retract{", "::retract{"), $.fact_pattern, repeat($._tag_attr), "}"),

    // FactPattern ::= Ident "(" FactArg ("," FactArg)* ")" (dsl 0.3.0 Appendix C).
    fact_pattern: ($) =>
      seq($.ident, "(", $.fact_arg, repeat(seq(",", $.fact_arg)), ")"),
    // A component body may pass a `@param` ref as a fact argument.
    fact_arg: ($) => choice($.ident, $.ref, $.wildcard),
    wildcard: ($) => "_",

    // Directive ::= "::" Ident Attrs? (§7.2). Leaf — does NOT nest.
    directive: ($) => seq("::", $.ident, optional($.attrs)),

    // ---- content -----------------------------------------------------------
    // Line ::= "@" Speaker Attrs? ":" WS Text (§7.1). Text MAY interpolate (§7.6).
    // The leading marker is a single `@` (0.2.2, foundation C1 — was `:` in
    // 0.1.0/0.2.0); `::set{` and `::` are longer tokens on the SAME `:` prefix
    // family and are unaffected. A line only ever starts on `@` + a speaker
    // ident, which is positionally distinct from the inline `@ref` macro
    // (`ref` only ever appears as an attr VALUE, inside a `cel_string`, or
    // inside `{{…}}` — never at `_node` position), so no grammar conflict
    // arises between line-start `@` and expression-context `@ref`.
    line: ($) =>
      seq(
        "@",
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
    // Hub ::= "<hub" Attrs ">" (HubChoice | HubReturn)+ "</hub>" (§7.3.2,
    // dsl 0.28.0 §5). A revisit conversation that re-presents eligible
    // choices. `id` required; the `once`/`exit` flags and
    // `into`/`persist`/`value`/`when` sugar all ride the generic `_tag_attr`
    // machinery (bare-bool `once`/`exit`; string/ref values) — no new attr
    // vocabulary needed. At most one `<return>` is the checker's rule.
    hub: ($) =>
      seq(
        "<hub",
        repeat($._tag_attr),
        ">",
        repeat(choice($.hub_choice, $.hub_return)),
        "</hub>",
      ),

    // HubReturn ::= "<return>" Node* "</return>" (dsl 0.28.0 §5): the text
    // that runs each time an option hands control back to the hub.
    hub_return: ($) =>
      seq(
        "<return",
        repeat($._tag_attr),
        ">",
        repeat($._node),
        "</return>",
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
        repeat(choice($._tag_attr, alias($._match_subject, $.cel_attr))),
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

    // Literal ::= EnumMember | "true" | "false" | Number | NumRange | "unset"
    // (§7.3.1; ranges dsl 0.18.0 §2). NumRange ::= Number ".." Number
    // | Number ".." | ".." Number — inclusive bounds, each a Number (§4.4)
    // that may carry a leading "-" and a decimal part (`-3..-1`, `0.5..1.5`,
    // `..0`, `2..`). The range alternative is listed first so a bound that
    // starts with "-" or ".." lexes as ONE literal; the second captures
    // signed/decimal numerals; the third keeps bare enum-member / true / false
    // / unset identifiers (and the `a|b` shape). Maximal munch picks the
    // longest, so `gold`, `-1` and `1..3` lex exactly as before. Malformed
    // ranges (`..`, `1..2..3`) are the checker's `E-WHEN-RANGE`, not a parse
    // error — this grammar is editor-side only.
    when_literal: ($) =>
      token.immediate(
        /(-?[0-9]+(\.[0-9]+)?)?\.\.(-?[0-9]+(\.[0-9]+)?)?|-?[0-9]+(\.[0-9]+)?|[A-Za-z0-9_][A-Za-z0-9_.-]*/,
      ),

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
    // TOP-LEVEL declaration (mirrors `section`, not a `_node` alternative) — the
    // quest kind admits `<quest>` only at the top level.
    quest: ($) =>
      seq(
        "<quest",
        repeat($._tag_attr),
        ">",
        repeat(choice($._node, $.reward)),
        "</quest>",
      ),

    // ---- lore entries (nest; dsl 0.19.0 §2–§4) ------------------------------
    // Entry ::= "<entry" Attrs ">" Node* "</entry>". A DOCUMENT TOP-LEVEL
    // declaration of a `kind: lore` document, exactly like `quest` (never a
    // `_node` alternative). The body is the ordinary node stream; which nodes
    // an entry admits (lines, `<match>`, `::set`/`::assert`/`::retract`) is
    // the checker's `E-GRAMMAR-NOT-ADMITTED`, not a parse error. Attributes
    // (`id`/`target`/`category`/`title`/`series`/`order`, CEL `when`) ride the
    // generic `_tag_attr` machinery.
    entry: ($) =>
      seq(
        "<entry",
        repeat($._tag_attr),
        ">",
        repeat($._node),
        "</entry>",
      ),

    // BeatDecl ::= "<beat" Attrs ">" SceneBody "</beat>" (dsl 0.23.0 §4, beat
    // bundles). A DOCUMENT TOP-LEVEL declaration of a `kind: lore` document,
    // exactly like `entry`. The body is the ordinary node stream; its scene
    // section body admission is the checker's. Attributes (`id`/`on`/`target`/
    // `title`/`priority`/`once`, bare `also`, CEL `when`) ride the generic
    // `_tag_attr` machinery. dsl 0.27.0 §6: a template use (`use="…"` plus
    // its param attrs) may be self-closing — `<beat use="trainer" id="r3"/>`
    // — tried first so `/>` vs `>` stays LR(1)-clean, like `objective`.
    beat: ($) =>
      choice(
        seq("<beat", repeat($._tag_attr), "/>"),
        seq("<beat", repeat($._tag_attr), ">", repeat($._node), "</beat>"),
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
        seq(
          "<objective",
          repeat($._tag_attr),
          ">",
          repeat(choice($._node, $.reward)),
          "</objective>",
        ),
      ),

    // Reward ::= "<reward" Attrs "/>" (dsl 0.16.0 §2). ALWAYS self-closing —
    // a body-form `<reward> … </reward>` is a parse error (an authoring bug
    // any editor surfaces immediately). Reachable ONLY as a direct child of
    // `<quest>` or `<objective>` (owner-scoped), never in `_node` — so an
    // exhaustive body walk elsewhere in the tooling can never see a reward
    // out of place (spec D-A). Attribute set (optional stable `id`, dsl
    // 0.37.0 §3.5; `kind`/`target`/`amount`/`when`/`on`) rides the generic
    // `_tag_attr` machinery; the checker owns shape/vocabulary via
    // `E-REWARD-ATTR`/`E-REWARD-KIND` and duplicate ids via `E-REWARD-DUP`.
    reward: ($) =>
      seq("<reward", repeat($._tag_attr), "/>"),

    // ---- attributes (§4.5) -------------------------------------------------
    // Attrs ::= "{" ( Attr ( WS Attr )* )? "}"  — the brace-delimited form used
    // by `:line` and `::` directives. Tag attributes reuse `_tag_attr` directly.
    // A `,` between attributes is tolerated (the core attribute scanner skips
    // non-ident separators).
    attrs: ($) => seq("{", repeat(choice($._tag_attr, ",")), "}"),

    // An attribute in any position (brace-form or bare tag attribute). Splitting
    // the CEL-valued keys (`on`/`test`/`when`, §7.3/§8) into their own node lets
    // editor queries reach the CEL sub-tokens (@ref, state-path) inside their
    // value; every other key is a plain String/Ref attribute (§4.5).
    _tag_attr: ($) => choice($.attr, $.cel_attr),

    // Attr ::= Ident "=" String | Ident "=" Ref | Ident  (bare ⇒ true).
    attr: ($) =>
      seq(
        $.key,
        optional(seq("=", choice($.string, $.ref, $.value))),
      ),

    // CelAttr ::= CelKey "=" ( CelString | Ref )  — the CEL-valued attributes
    // `<match subject>`, `<when test>`, `<choice when>` (§7.3, §11.1–11.2). The value
    // is a CEL expression (§8): a double-quoted `CelString` (§4.4) or a bare
    // `@ref` macro (§8.1). Distinct from `attr` so highlight/tag queries can
    // capture the CEL innards (@ref, state-path) rather than an opaque string.
    cel_attr: ($) => seq($.cel_key, "=", choice($.cel_string, $.ref)),

    // CelKey — the reserved attribute keys whose value is CEL (§7.3): `test`
    // is a `<when>` guard, `when` a `<choice>` guard,
    // `visibleWhen` an objective's visibility condition, `rearm` a quest's
    // re-arm condition and `spentBy` a beat's spend condition (dsl 0.27.0 §5).
    // A named node (lexes ahead of the generic `key` on a tie) so editors treat
    // these keys distinctly and know their value is embedded CEL.
    cel_key: ($) =>
      choice("test", "when", "visibleWhen", "done", "start", "fail", "rearm", "spentBy"),

    // `<match subject="…">` (dsl 0.37.0 §3.5) — the match subject is CEL. A
    // `cel_attr` scoped to `<match>` (like `when_is` to `<when>`): `subject` is
    // a keyword only there, so `<track subject="camera">` stays a plain attr.
    _match_subject: ($) =>
      seq(alias("subject", $.cel_key), "=", choice($.cel_string, $.ref)),

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
    // A component's speaker parameter is `@@name` (the `@` marker + `@name`).
    speaker: ($) => /@?[A-Za-z][A-Za-z0-9_-]*/,

    // Bare (unquoted) attribute value — `n=2`, `amount=5`, `rate=1.25`: the
    // core scanner reads `key=` + token to whitespace/terminator as a string.
    value: ($) => /[^ \t\r\n"'@}>,\/][^ \t\r\n"}>,\/]*/,

    // Subscript `[expr]` on a state path (`user.bond[occasion.target]`), in
    // `::set` targets and `{{…}}` interpolations. Opaque, single-line.
    index: ($) => token.immediate(/\[[^\]\r\n]*\]/),

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

    // Text (§4.4/§7.1, dsl 0.37.0 §3.6): the rest of a content line to EOL — a
    // run of opaque text chunks, escapes, `{{…}}` interpolations (§7.6) and
    // inline modifiers. Every piece is `token.immediate`, so `extras`
    // (whitespace, comments, the newline) are never skipped mid-text: a `//` or
    // `/*` INSIDE text stays literal text (Text is opaque, §4.2), text never
    // spills onto the next line, and the leading space after `: ` is kept.
    // Content-line only: section headings use `heading` (no modifiers).
    text: ($) =>
      repeat1(
        choice($._text_chunk, $._text_special, $.escape, $.interpolation, $.modifier),
      ),

    // A run of literal text: anything but a brace, bracket, colon, backslash,
    // or newline. Stops at those so the longer `{{`/escape/modifier tokens win.
    // `prec(1)` beats the `comment` extra: a `//` or `/*` after an inline `:` /
    // `[` / `{` stays literal text instead of lexing as a trailing comment.
    _text_chunk: ($) => token.immediate(prec(1, /[^{}\[\]:\\\r\n]+/)),

    // A lone punctuation char that opens no interpolation, escape or modifier —
    // literal text (`:` not followed by `name[`/`name{` is ordinary, §3.6).
    _text_special: ($) => token.immediate(/[{}\[\]:\\]/),

    // Escape ::= "\{{" | "\:" | "\[" | "\]" | "\{" | "\}" | "\\" (§7.6, dsl
    // 0.37.0 §3.6). `\{{` is consumed whole (3 chars) so its `{{` never opens
    // an interpolation; `\:` never starts a modifier. An unknown escape is the
    // checker's `E-TEXT-ESCAPE` (here a lone `\` is literal text).
    escape: ($) => token.immediate(/\\\{\{|\\[:\[\]{}\\]/),

    // Modifier ::= ":" Name Span? Attrs? (dsl 0.37.0 §3.6) — `:pause{s=0.5}`,
    // `:speed[text]{rate=1.25}`, `:emphasis[text]`. `modifier_name` (external)
    // is the `:name` head and only matches when `[` or `{` follows, so `Note:`
    // or `12:30` stay ordinary text. Name vocabulary (`pause`/`speed`/the
    // project `textStyle` domain) and attr shape are the checker's
    // `E-TEXT-MODIFIER`.
    modifier: ($) =>
      seq(
        $.modifier_name,
        choice(
          seq($.span, optional(alias($._modifier_attrs, $.attrs))),
          alias($._modifier_attrs, $.attrs),
        ),
      ),

    // Span ::= "[" Text "]" — modifier spans nest; interpolations inside a span
    // stay interpolations. A `]` closes the span (escape it as `\]`).
    span: ($) =>
      seq(
        token.immediate("["),
        optional(alias($._span_text, $.text)),
        token.immediate("]"),
      ),

    _span_text: ($) =>
      repeat1(
        choice($._text_chunk, $._span_special, $.escape, $.interpolation, $.modifier),
      ),

    // Span-body literal punctuation: like `_text_special` minus the closing `]`.
    _span_special: ($) => token.immediate(/[{}\[:\\]/),

    // Attrs ::= "{" Attr (hws Attr)* "}" on a modifier — aliased to the shared
    // `attrs`/`attr` nodes so attribute queries apply. Immediate throughout,
    // so a modifier never spans a newline.
    _modifier_attrs: ($) =>
      seq(
        token.immediate("{"),
        repeat(choice(token.immediate(/[ \t]+/), alias($._modifier_attr, $.attr))),
        token.immediate("}"),
      ),

    // Attr ::= Name "=" Value | Name; Value ::= Quoted | Ref | Number | Bare.
    _modifier_attr: ($) =>
      seq(
        alias(token.immediate(/[A-Za-z][A-Za-z0-9_-]*/), $.key),
        optional(
          seq(
            token.immediate("="),
            choice(
              alias(token.immediate(/"([^"\\\n]|\\[^\n])*"/), $.string),
              alias(token.immediate(/@[A-Za-z][A-Za-z0-9_-]*/), $.ref),
              alias(token.immediate(/-?[0-9]+(\.[0-9]+)?/), $.number),
              alias(token.immediate(/[A-Za-z][A-Za-z0-9_-]*/), $.value),
            ),
          ),
        ),
      ),

    // Interp ::= "{{" WS? ( Path | Ref | ReservedToken ) Index* Format? WS? "}}"
    // (§7.6). Only the three legal bases are admitted (a bare CEL expr is not,
    // §7.6); `Index` is a `[expr]` subscript and `Format` a `:name(args)?`
    // display suffix (`:plural(one|# many)`, `:ordinal`). The
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
        repeat($.index),
        // Display format suffix: `:plural(one|# many)`, `:ordinal`.
        optional(
          seq(
            token.immediate(":"),
            alias(token.immediate(/[A-Za-z][A-Za-z0-9_]*/), $.format),
            optional(alias(token.immediate(/\([^)\r\n]*\)/), $.format_args)),
          ),
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
