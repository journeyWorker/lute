; tree-sitter-lute — syntax highlights for the Lute Scenario DSL (dsl §4–7).
;
; The DSL is three visually-distinct LAYERS (architecture.md); this file maps
; each to its own capture family so a real editor colors them apart:
;
;   1. CONTENT (§7.1 `:speaker`)   — dialogue / narration  → @string + @character
;   2. STAGING (§7.2 `::`, §7.4 <timeline>/<track>)        → @function family
;   3. LOGIC   (§7.3 <branch>/<match>, §7.3.4 `::set`, CEL) → @keyword family
;
; Plus distinct captures the arch calls out separately:
;   - CEL expressions  → @embedded         (an embedded expression language)
;   - `@ref`           → @variable.parameter
;   - state paths      → @property

; ---- CONTENT layer (§7.1) -------------------------------------------------
; `:speaker{attrs}: text` — the speaker is a character id; the text is dialogue
; / narration (string-family) that MAY embed `{{…}}` interpolations (§7.6). The
; leading and second `:` are the content-line markers.
(line (speaker) @character)
(line (text) @string)
(line ":" @punctuation.special)

; ---- interpolation (§7.6) -------------------------------------------------
; `{{ path | @ref | userName }}` — a render-time state read embedded in content
; text (and, per the checker, `<choice label>`). Delimiters read as special
; punctuation; the interior reuses the property / ref / constant families, and
; `\{{` is an escaped literal `{{`.
(interpolation ["{{" "}}"] @punctuation.special)
(interpolation (path) @property)
(interpolation (reserved) @constant.builtin)
(escape) @string.escape

; ---- STAGING layer (§7.2, §7.4) -------------------------------------------
; `::`ident staging directives — the directive name reads as a call (@function).
(directive "::" @punctuation.special)
(directive (ident) @function)

; `<timeline>` / `<track>` staging blocks — block "macros" that expand into
; scheduled directives; kept in the function family, distinct from logic tags.
(timeline ["<timeline" "</timeline>"] @function.macro)
(track ["<track" "</track>"] @function.macro)

; ---- LOGIC layer (§7.3, §7.3.4, §11.2) ------------------------------------
; `::set` state assignment + its operator (the assignment is a logic keyword).
(set "::set{" @keyword.control)
(set (assign_op) @operator)

; `<branch>` / `<choice>` control-flow branching.
(branch ["<branch" "</branch>"] @keyword.control)
(choice ["<choice" "</choice>"] @keyword.control)

; `<hub>` / hub `<choice>` revisit conversation (§7.3.2) — branching family; a
; distinct node from a branch choice (a hub arm may carry `once`/`exit`).
(hub ["<hub" "</hub>"] @keyword.control)
(hub_choice ["<choice" "</choice>"] @keyword.control)

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

; ---- distinct arch captures -----------------------------------------------
; CEL expression (the `::set` right-hand side) — an embedded expression lang.
(cel_expr) @embedded
; CEL-valued attribute value (`<match on>`, `<when test>`, `<choice when>`,
; §7.3/§8) — also embedded CEL, so it colors like `::set` RHS, not a string.
(cel_string) @embedded
; State path (`scene.affect.bianca`) — dotted member access. Captured both as a
; `::set` target and wherever it appears inside a CEL value (`<match on="…">`).
(set (path) @property)
(cel_string (path) @property)
; Bare `@ref` (defs-backed guard / value reference). The bare pattern also
; reaches `@ref`s nested inside a `cel_attr` value / `cel_string` (§8.1).
(ref) @variable.parameter

; ---- attributes (§4.5) ----------------------------------------------------
(attr (key) @attribute)
; CEL-valued attribute key (`on`/`test`/`when`) — an attribute key like any
; other, but its value is embedded CEL (captured above), not an opaque string.
(cel_attr (cel_key) @attribute)
(string) @string

; ---- headings (§6.2, §6.3) ------------------------------------------------
(title (text) @markup.heading.1)
(title "#" @punctuation.special)
(shot (text) @markup.heading.2)
(shot "##" @punctuation.special)

; ---- trivia / frontmatter -------------------------------------------------
(comment) @comment
(frontmatter) @string.special

; ---- punctuation ----------------------------------------------------------
[
  "{"
  "}"
  ">"
] @punctuation.bracket

