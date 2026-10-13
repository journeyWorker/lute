---
version: 1
slug: "src-content-docs-index-mdx"
primary_target: "src/content/docs/index.mdx"
related_targets: ["src/content/docs/ko/index.mdx"]
---

Scope: the landing page (`src/content/docs/index.mdx`, EN and `/ko/`). Mode: Persuade. The visual world it establishes also carries the documentation pages (Read, code-led), recorded later in DESIGN.md.

Audience and job: scenario writers first, then game engineers and AI agents. Action: copy the install command / start the first-scene tutorial, or open the playground. Proof: a real Lute scene and real checker results; no testimonials, adopters or numbers exist.

Seed: e11bad04 (direction roll, re-rolled once; the owner pinned the gamebook direction, which beats the roll).

Future: the landing's first section becomes a playable intro game written in Lute and run by `@lute-lang/player-web` + `@lute-lang/runtime` (lazy-loaded on Play). Not in this build; the first section is designed as that game's section 1 so the swap needs no redesign.

## Direction contract

THESIS: Lute is a gamebook you can prove. The page is a numbered-section adventure book: the scene is section 1, choices print as "turn to 2", the adventure sheet carries the state, and the checker's seal certifies every section is reachable and none is a dead end. Refuses the dev-tool default (dark hero, code window, six feature cards) and the cream editorial page with a serif.

OWN-WORLD: Pulp gamebook: saturated cover inks (deep cadmium red, bottle green, ultramarine) own whole regions; interior pages are cheap bright newsprint white, not cream, printed in one black ink with a single spot colour per section. Section numbers set huge in a condensed bold face; body in a sturdy text face; Hangul set properly. Adventure-sheet boxes ruled in hairline, filled in by hand. The checker's seal is a round rubber stamp.

STORY: The visitor reads section 1 (a real Lute scene), sees that each choice is a numbered jump and each condition a box on the adventure sheet, sees the seal (reachable, total, no dead ends), understands Lute writes the gamebook and proves it, and copies the install command.

FIRST VIEWPORT: A cover-ink field (red) carrying the book open to section 1 at large scale: the big section number, the scene set as gamebook prose with its choices printed as "turn to" lines; beside it the adventure sheet with the scene's real state rows (`scene.affect.marina` etc.); the checker seal stamped across the page corner. Headline: "Write the gamebook. Lute proves it." Primary action: the install command + Get started on a single solid control; Playground beside it. The next section (how the engine runs it) peeks below as section 2's number.

FORM: Gamebook. Navigation is numbered sections; docs sections carry their numbers; state lives on adventure-sheet boxes. Signature interaction: choosing a "turn to" line jumps to that section and ticks the adventure sheet (real state from the scene); the seal shows what the checker proved for the section in view. Motion: page-turn cut, never a slide; ticks write in. Reduced motion: no transitions.

RISK: Retro pulp or children's-book reading; newsprint drifting toward cream. Keep cover inks saturated and dominant, interiors stark white, type confident.

Approved comp: `.impeccable/mocks/comp-2-page.webp` (owner-approved 2026-10-12). Do not literalize: the invented margin taglines ("NARRATIVE YOU CAN TRUST", "BRANCH BOLDER STORIES", "A MORE PLAYABLE TOMORROW", "SAME STORY. MORE PATHS. FEWER BUGS.", "LUTE FOR BRIGHTER WORLDS.", the footer line) and the literal "pencilled 0 → 1" label; the adventure sheet shows the real state rows of the real scene.
