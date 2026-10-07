; tree-sitter-lute — syntax highlights for the Lute Scenario DSL (dsl §4–7).
;
; The DSL is three visually-distinct LAYERS (architecture.md); this file maps
; each to its own capture family so a real editor colors them apart:
;
;   1. CONTENT (§7.1 `@speaker`)   — dialogue / narration  → @string + @character
;   2. STAGING (§7.2 `::`, §7.4 <timeline>/<track>)        → @function family
;   3. LOGIC   (§7.3 <branch>/<match>, §7.3.4 `::set`, CEL) → @keyword family
;
; Plus distinct captures the arch calls out separately:
;   - CEL expressions  → @embedded         (an embedded expression language)
;   - `@ref`           → @variable.parameter
;   - state paths      → @property

; ---- CONTENT layer (§7.1) -------------------------------------------------
; `@speaker{attrs}: text` — the speaker is a character id; the text is dialogue
; / narration (string-family) that MAY embed `{{…}}` interpolations (§7.6). The
; leading `@` and the `:` before the text are the content-line markers.
(line (speaker) @character)
(line (text) @string)
(line "@" @punctuation.special)
(line ":" @punctuation.special)

; ---- interpolation (§7.6) -------------------------------------------------
; `{{ path | @ref | userName }}` — a render-time state read embedded in content
; text (and, per the checker, `<choice text>`). Delimiters read as special
; punctuation; the interior reuses the property / ref / constant families, and
; `\{{` is an escaped literal `{{`.
(interpolation ["{{" "}}"] @punctuation.special)
(interpolation (path) @property)
(interpolation (reserved) @constant.builtin)
; `:plural(…)` / `:ordinal` display format suffix and `[expr]` subscripts.
(interpolation (format) @function.call)
(interpolation (format_args) @string.special)
(index) @property
(escape) @string.escape

; ---- inline text modifiers (dsl 0.37.0 §3.6) -----------------------------
; `:pause{s=0.5}`, `:speed[text]{rate=1.25}`, `:emphasis[text]` — the `:name`
; head reads as a macro call; the span brackets as special punctuation; the
; span text stays string-family (spans nest); attr keys reuse the attribute
; family below, and numbers read as numbers.
(modifier (modifier_name) @function.macro)
(span ["[" "]"] @punctuation.special)
(span (text) @string)
(modifier (attrs (attr (number) @number)))

; ---- STAGING layer (§7.2, §7.4) -------------------------------------------
; `::`ident staging directives — the directive name reads as a call (@function).
(directive "::" @punctuation.special)
(directive (ident) @function)
; `::jump` / `::label` (dsl 0.37.0 §3.5) are control flow, not staging: the
; forward jump and its document-wide target read in the logic keyword family.
((directive (ident) @keyword.control)
  (#any-of? @keyword.control "jump" "label"))

; `<timeline>` / `<track>` staging blocks — block "macros" that expand into
; scheduled directives; kept in the function family, distinct from logic tags.
(timeline ["<timeline" "</timeline>"] @function.macro)
(track ["<track" "</track>"] @function.macro)

; ---- LOGIC layer (§7.3, §7.3.4, §11.2) ------------------------------------
; `::set` state assignment + its operator (the assignment is a logic keyword).
(set "::set{" @keyword.control)
(set (assign_op) @operator)

; `::assert`/`::retract` relational-fact writes (dsl 0.3.0 §5) — staging
; leaves that mutate the fact store; kept in the logic keyword family beside
; `::set` since they are also state-affecting directives.
(assert "::assert{" @keyword.control)
(retract "::retract{" @keyword.control)
(fact_pattern (ident) @function)
(fact_arg (ident) @variable)
(wildcard) @constant.builtin

; `<branch>` / `<choice>` control-flow branching.
(branch ["<branch" "</branch>"] @keyword.control)
(choice ["<choice" "</choice>"] @keyword.control)

; `<hub>` / hub `<choice>` revisit conversation (§7.3.2) — branching family; a
; distinct node from a branch choice (a hub arm may carry `once`/`exit`).
(hub ["<hub" "</hub>"] @keyword.control)
(hub_choice ["<choice" "</choice>"] @keyword.control)
; `<return>` (dsl 0.28.0 §5) — the hub's revisit text.
(hub_return ["<return" "</return>"] @keyword.control)

; `<match>` / `<when>` / `<otherwise>` first-match-wins conditional.
(match ["<match" "</match>"] @keyword.conditional)
(when ["<when" "</when>"] @keyword.conditional)
(otherwise ["<otherwise" "</otherwise>"] @keyword.conditional)

; `<when is="…">` literal pattern (§7.3.1) — the `is` key is an attribute; its
; `|`-alternation of literals (enum / true / false / number / unset) are consts.
(when_is (when_key) @attribute)
(when_pattern (when_literal) @constant)

; `<quest>` / `<on>` / `<objective>` quest-kind constructs (dsl 0.2.0 §4, §6).
(quest ["<quest" "</quest>"] @keyword.control)
(on ["<on" "</on>"] @keyword.control)
(objective ["<objective" "</objective>" "/>"] @keyword.control)
; `<reward/>` (dsl 0.16.0 §2) — always self-closing; a quest-/objective-scoped
; declaration of a grant. Same control-family capture as its siblings.
(reward ["<reward" "/>"] @keyword.control)
; `<entry>` (dsl 0.19.0 §2) — the lore-kind top-level declaration, a sibling
; of `<quest>`; same control-family capture.
(entry ["<entry" "</entry>"] @keyword.control)
; `<beat>` (dsl 0.23.0 §4) — a lore document's top-level beat bundle, a
; sibling of `<entry>`; same control-family capture.
(beat ["<beat" "</beat>"] @keyword.control)

; ---- distinct arch captures -----------------------------------------------
; CEL expression (the `::set` right-hand side) — an embedded expression lang.
(cel_expr) @embedded
; CEL-valued attribute value (`<match subject>`, `<when test>`, `<choice when>`,
; §7.3/§8) — also embedded CEL, so it colors like `::set` RHS, not a string.
(cel_string) @embedded
; State path (`scene.affect.marina`) — dotted member access. Captured both as a
; `::set` target and wherever it appears inside a CEL value (`<match subject="…">`).
(set (path) @property)
(cel_string (path) @property)
; Bare `@ref` (defs-backed guard / value reference). The bare pattern also
; reaches `@ref`s nested inside a `cel_attr` value / `cel_string` (§8.1).
(ref) @variable.parameter

; ---- attributes (§4.5) ----------------------------------------------------
(attr (key) @attribute)
; CEL-valued attribute key (`subject`/`test`/`when`) — an attribute key like any
; other, but its value is embedded CEL (captured above), not an opaque string.
(cel_attr (cel_key) @attribute)
(string) @string
; Bare unquoted attribute value (`amount=2`).
(attr (value) @constant)

; ---- section headings (dsl 0.37.0 §3.1) -----------------------------------
(section (heading) @markup.heading.2)
(section "##" @punctuation.special)
; `{#stable-id}` suffix — identity metadata, not part of the heading text.
(section_id) @label

; ---- trivia / frontmatter -------------------------------------------------
(comment) @comment
(frontmatter) @string.special

; ---- punctuation ----------------------------------------------------------
[
  "{"
  "}"
  ">"
] @punctuation.bracket

