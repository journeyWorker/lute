; tree-sitter-lute — foldable regions for the Lute Scenario DSL.
;
; Fold the multi-line structural blocks: shot bodies and every nesting logic /
; timeline block (§6.3, §7.3, §7.4). Leaf nodes (`@speaker` line, `::`directive,
; `::set`) are single-line and never folded.

; Shot body (§6.3) — `## heading` … up to the next shot / EOF.
(shot) @fold

; Logic blocks (§7.3, §11.2) — nest, so each level folds independently.
(branch) @fold
(choice) @fold
(match) @fold
(when) @fold
(otherwise) @fold
(hub) @fold
(hub_choice) @fold

; Timeline blocks (§7.4) — the timeline and each of its tracks.
(timeline) @fold
(track) @fold

; Quest blocks (§4, §6.3, §6.4, NEW) — the quest itself and each nested
; `<on>` arm / `<objective>` body (a self-closing `<objective/>` is single-line
; and never actually spans a fold range, so no filtering is needed here).
(quest) @fold
(on) @fold
(objective) @fold
