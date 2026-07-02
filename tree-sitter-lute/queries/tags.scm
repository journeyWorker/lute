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

; ---- references -----------------------------------------------------------
; Bare `@ref` (§4.5) — a defs-backed guard / value reference; the ref token
; (leading `@` included) is both the reference site and its name.
(ref) @name @reference.call
