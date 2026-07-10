; tree-sitter-lute — code-nav tags for the Lute Scenario DSL.
;
; Follows the tree-sitter tags convention: each definition/reference carries a
; `@name` capture (the navigable identifier) plus a `@definition.*` / a
; `@reference.*` capture on the enclosing node.

; ---- definitions ----------------------------------------------------------
; Shot heading (§6.3) — a top-level navigable beat; its text is the name.
(shot (text) @name) @definition.module

; `<branch id="…">` (§7.3) — the branch id is a jump target.
(branch
  (attr (key) @_key (string) @name)
  (#eq? @_key "id")) @definition.class

; `<choice id="…">` (§7.3) — each choice id inside a branch is a jump target.
(choice
  (attr (key) @_key (string) @name)
  (#eq? @_key "id")) @definition.function

; `<hub id="…">` (§7.3.2) — a revisit-conversation entry; the hub id is a jump
; target, like a branch id.
(hub
  (attr (key) @_key (string) @name)
  (#eq? @_key "id")) @definition.class

; hub `<choice id="…">` (§7.3.2) — each hub arm id is a jump target.
(hub_choice
  (attr (key) @_key (string) @name)
  (#eq? @_key "id")) @definition.function

; `<quest id="…">` (§6.3, NEW) — the quest id is a project-wide jump target.
(quest
  (attr (key) @_key (string) @name)
  (#eq? @_key "id")) @definition.class

; `<objective id="…">` (§6.4, NEW) — each objective id inside a quest is a
; jump target (self-closing or long form; `attr` is reached either way).
(objective
  (attr (key) @_key (string) @name)
  (#eq? @_key "id")) @definition.function

; ---- references -----------------------------------------------------------
; Bare `@ref` (§4.5) — a defs-backed guard / value reference; the ref token
; (leading `@` included) is both the reference site and its name. The bare
; pattern also matches `@ref`s nested inside a CEL attribute value (§8.1),
; e.g. `<when test="@fond">`.
(ref) @name @reference.call

; State path inside a CEL-valued attribute (`<match on="scene.choices.x">`,
; §7.3/§9) — a navigable reference to declared state, mirroring the `@embedded`
; CEL treatment of the `::set` right-hand side.
(cel_string (path) @name) @reference.call

; State path inside a `{{…}}` interpolation (`{{run.coins}}`, §7.6) — a
; navigable read of declared state, like the CEL-attr path above.
(interpolation (path) @name) @reference.call
