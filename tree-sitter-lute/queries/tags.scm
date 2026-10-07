; tree-sitter-lute — code-nav tags for the Lute Scenario DSL.
;
; Follows the tree-sitter tags convention: each definition/reference carries a
; `@name` capture (the navigable identifier) plus a `@definition.*` / a
; `@reference.*` capture on the enclosing node.

; ---- definitions ----------------------------------------------------------
; Section heading (dsl 0.37.0 §3.1) — a top-level navigable beat; its heading
; text is the name (the optional `{#id}` suffix is identity metadata, not a
; jump target).
(section (heading) @name) @definition.module

; `::label{name="…"}` (dsl 0.37.0 §3.5) — the document-wide jump label.
(directive
  (ident) @_directive
  (attrs (attr (key) @_key (string) @name))
  (#eq? @_directive "label")
  (#eq? @_key "name")) @definition.constant

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

; `<entry id="…">` (dsl 0.19.0 §3) — the entry id is a project-wide jump
; target, like a quest id.
(entry
  (attr (key) @_key (string) @name)
  (#eq? @_key "id")) @definition.class

; `<beat id="…">` (dsl 0.23.0 §4) — a lore document's beat bundle; the beat
; id is a navigable definition, like an entry id.
(beat
  (attr (key) @_key (string) @name)
  (#eq? @_key "id")) @definition.class

; `<reward id="…">` (dsl 0.37.0 §3.5) — the optional stable reward id, unique
; within its quest.
(reward
  (attr (key) @_key (string) @name)
  (#eq? @_key "id")) @definition.constant

; `<objective id="…">` (§6.4, NEW) — each objective id inside a quest is a
; jump target (self-closing or long form; `attr` is reached either way).
(objective
  (attr (key) @_key (string) @name)
  (#eq? @_key "id")) @definition.function

; ---- references -----------------------------------------------------------
; `::jump{to="…"}` (dsl 0.37.0 §3.5) — a forward jump to a `::label` name.
(directive
  (ident) @_directive
  (attrs (attr (key) @_key (string) @name))
  (#eq? @_directive "jump")
  (#eq? @_key "to")) @reference.call

; Bare `@ref` (§4.5) — a defs-backed guard / value reference; the ref token
; (leading `@` included) is both the reference site and its name. The bare
; pattern also matches `@ref`s nested inside a CEL attribute value (§8.1),
; e.g. `<when test="@fond">`.
(ref) @name @reference.call

; State path inside a CEL-valued attribute (`<match subject="scene.choices.x">`,
; §7.3/§9) — a navigable reference to declared state, mirroring the `@embedded`
; CEL treatment of the `::set` right-hand side.
(cel_string (path) @name) @reference.call

; State path inside a `{{…}}` interpolation (`{{run.coins}}`, §7.6) — a
; navigable read of declared state, like the CEL-attr path above.
(interpolation (path) @name) @reference.call
