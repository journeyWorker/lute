---
name: Lute
description: The site for the Lute narrative language, printed as an open gamebook.
colors:
  paper: "#f8f8f7"
  paper-2: "#efefec"
  ink: "#0a0a0a"
  ink-2: "#3a3a38"
  ink-3: "#5c5c58"
  bottle-green: "#084828"
  bottle-green-2: "#0b5a33"
  green-tint: "#dbe7df"
  cadmium-red: "#b3200e"
  red-tint: "#f3dcd7"
  ultramarine: "#1c2a96"
  blue-tint: "#dcdff2"
  night-ground: "#111311"
  night-print: "#efefea"
  night-green: "#6cc493"
  night-red: "#ff8f78"
typography:
  display:
    fontFamily: "Anton, Black Han Sans, Arial Narrow, sans-serif"
    fontSize: "calc(122 * var(--u))"
    fontWeight: 400
    lineHeight: 0.9
    letterSpacing: "-0.012em"
  headline:
    fontFamily: "Anton, Black Han Sans, Arial Narrow, sans-serif"
    fontSize: "clamp(2.125rem, 1.5rem + 2.4vw, 3.5rem)"
    fontWeight: 400
    lineHeight: 1.04
    letterSpacing: "-0.005em"
  numeral:
    fontFamily: "Libre Caslon Condensed Variable, Noto Serif KR, Georgia, serif"
    fontSize: "clamp(3.75rem, 2.75rem + 4vw, 6rem)"
    fontWeight: 700
    lineHeight: 0.74
    letterSpacing: "-0.03em"
    fontFeature: "lnum"
  title:
    fontFamily: "Libre Caslon Condensed Variable, Noto Serif KR, Georgia, serif"
    fontSize: "1.375rem"
    fontWeight: 600
    lineHeight: 1.2
    fontFeature: "lnum"
  body:
    fontFamily: "Charis SIL, Noto Serif KR, Georgia, Times New Roman, serif"
    fontSize: "1.0625rem"
    fontWeight: 400
    lineHeight: 1.65
  passage:
    fontFamily: "Charis SIL, Noto Serif KR, Georgia, Times New Roman, serif"
    fontSize: "clamp(1.375rem, calc(36 * var(--u)), 2.25rem)"
    fontWeight: 400
    lineHeight: 1.32
    letterSpacing: "-0.005em"
  label:
    fontFamily: "JetBrains Mono Variable, ui-monospace, SFMono-Regular, Menlo, monospace"
    fontSize: "0.6875rem"
    fontWeight: 500
    lineHeight: 1.4
    letterSpacing: "0.12em"
  code:
    fontFamily: "JetBrains Mono Variable, ui-monospace, SFMono-Regular, Menlo, monospace"
    fontSize: "0.86em"
    fontWeight: 400
rounded:
  none: "0px"
  seal: "50%"
spacing:
  measure: "40rem"
  h2-above: "3.5rem"
  turn-cell: "4.75rem"
  aside-pad: "0.875rem 1.125rem 1rem"
components:
  turn-to:
    backgroundColor: "{colors.paper}"
    textColor: "{colors.ink}"
    rounded: "{rounded.none}"
    height: "4.75rem"
  turn-to-cell:
    backgroundColor: "{colors.bottle-green}"
    textColor: "{colors.paper}"
    typography: "{typography.title}"
    width: "4.75rem"
  turn-to-hover:
    backgroundColor: "{colors.green-tint}"
  install:
    backgroundColor: "{colors.ink}"
    textColor: "{colors.paper}"
    rounded: "{rounded.none}"
  spine:
    backgroundColor: "{colors.bottle-green}"
    textColor: "{colors.paper}"
  aside-title:
    backgroundColor: "{colors.ink}"
    textColor: "{colors.paper}"
    typography: "{typography.label}"
    padding: "0.375rem 0.75rem"
  aside-tip-title:
    backgroundColor: "{colors.bottle-green}"
    textColor: "{colors.paper}"
  aside-danger-title:
    backgroundColor: "{colors.cadmium-red}"
    textColor: "{colors.paper}"
  sheet-box:
    backgroundColor: "{colors.paper}"
    textColor: "{colors.ink}"
    rounded: "{rounded.none}"
  seal:
    textColor: "{colors.bottle-green}"
    rounded: "{rounded.seal}"
  code-frame:
    backgroundColor: "{colors.paper-2}"
    textColor: "{colors.ink}"
    rounded: "{rounded.none}"
---

# Design System: Lute

## Overview

**Creative North Star: "The Open Gamebook"**

The site is a printed choose-your-path book. Interior pages are bright newsprint white printed in one black ink; three cover inks (bottle green, cadmium red, ultramarine) own whole regions — the spine, numeral cells, full-bleed chapter fields — rather than sprinkling accents. Every way forward is a numbered "turn to" line, every list of facts is a ruled adventure-sheet row, and every heading carries its section number.

The landing is one open spread measured off a 1536 × 1024 comp: a green spine on the left, the page in the middle, an adventure sheet on the right with ruled boxes and a code-drawn checker's seal. The docs are the book's interior: the same spine becomes Starlight's numbered sidebar, titles and h2s are numbered, asides are sheet notes, pagination is turn-to rows. Light is the printed book; dark docs are night reading on a near-black ground. The landing always prints light.

Content is real: only genuine `.lute` source and genuine CLI output appear, and version strings are read at build time from the repository's constants (`src/lib/versions.ts`), never typed into copy.

**Key Characteristics:**
- One ink on paper; cover inks fill regions, never decorate.
- Square everything: zero radius, 1px (comp 1.5px) ink rules; the seal is the only circle.
- No shadows; depth comes from ink fields and ruled boxes.
- Numbers are the navigation: condensed lining numerals on spine, titles, h2s, turn-to cells.
- Hangul is first-class: keep-all breaking, no faux italic, tight tracking.

## Colors

Newsprint white and one black ink, with three opaque cover inks that each own a field.

### Primary
- **Bottle Green** (spine, sidebar, turn-to numeral cells, seal, selection, written-in sheet values, link underlines in docs, h2 numerals in light). `bottle-green-2` is Starlight's light accent; `green-tint` is the turn-to hover wash and the light accent-low.

### Secondary
- **Cadmium Red**: the red chapter spot and full red field section on the landing; danger asides; strings in code. `red-tint` is its wash.

### Tertiary
- **Ultramarine**: the full blue field section on the landing; data in code. `blue-tint` is its wash.

### Neutral
- **Paper** — page ground and ink-on-cover text. **Paper 2** — code frames and docs inline code ground in light.
- **Ink** — all text, rules, the install bar. **Ink 2 / Ink 3** — secondary text; Ink 3 prints the struck-through previous value and the declared default on sheet boxes.
- **Night Ground / Night Print** — dark docs ground and text; **Night Green** and **Night Red** are the cover inks lifted to read on it.

### Named Rules
**The Cover Ink Rule.** Green, red and blue fill a region (spine, numeral cell, field section) or nothing. On a field, the field's ink becomes the text's ground and paper becomes the spot.

**The Night Reading Rule.** Only the docs switch to dark, and only through Starlight's theme switch (`data-theme`), never the OS preference alone. The landing forces `color-scheme: light`; dark code inks apply only on docs.

## Typography

**Display Font:** Anton (Black Han Sans for Hangul)
**Condensed voice:** Libre Caslon Condensed Variable, weights 400–700 (numerals, wordmark, navigation, h3, sheet boxes, turn-to titles)
**Body Font:** Charis SIL (Noto Serif KR for Hangul)
**Label/Mono Font:** JetBrains Mono Variable (code, sheet data, tracked uppercase labels)

**Character:** A poster grotesque shouting chapter titles over a bookish serif, with a condensed old-style serif doing the counting.

### Hierarchy
- **Display** (Anton 400, comp 122px, 0.9): landing hero and chapter titles.
- **Headline** (Anton 400, clamp 2.125–3.5rem, 1.04): docs h1; h2 at clamp 1.75–2.25rem, line-height 1.1.
- **Numeral** (Caslon Condensed 700, 0.74 line-height): landing hero numeral (comp 458px), chapter numerals (comp 300px), docs title numeral (clamp 3.75–6rem).
- **Title** (Caslon Condensed 560–700): nav, h3, ledger terms, turn-to titles (1.375rem / 600).
- **Body** (Charis 400, 1.0625rem, 1.65; Hangul 1.8): docs measure 40rem (~70ch). Landing body scales with `--u` (clamp 1.0625–1.375rem, 1.55); passages run larger (1.32).
- **Label** (JetBrains Mono 500, 0.6875rem, 0.12em, uppercase): aside titles, turn-to cues, code frame UI; landing sheet/spine labels track 0.18–0.55em.

### Named Rules
**The Lining Figures Rule.** Caslon numerals are `lining-nums` only. Tabular figures widen `. , :` to figure width, so `tabular-nums` is never applied to anything that carries punctuation (it is used only on bare numeral columns: spine nav, book index).

**The Case Rule.** File names and commands keep their case: code frame titles, terminal headers and CLI output on the sheet are never uppercased.

**The Hangul Rule.** `word-break: keep-all` everywhere Korean appears; Hangul has no italic, so emphasis is upright (emphasis dots in docs) and the declared-default italic drops; wide label tracking collapses to ~0.04em.

## Layout

**Landing.** A container-query unit `--u = min(100cqw / 1536, 100svh / 1024)` is one comp pixel; desktop geometry is written as `calc(N * var(--u))`. Grid: spine `minmax(280u, 1fr)` | page `minmax(0, 970u)` | sheet `286u`. Page and sheet keep the comp's widths; spare width goes to the spine. Spine and sheet columns are sticky 1024u tall. Chapters pad `120u 25u 140u 56u`; text blocks cap at 34em. Below 64rem the book collapses to one column.

**Docs.** Starlight frame; measure `--sl-content-width: 40rem`. The green spine runs up through the header from 50rem; below 50rem the whole header is the green band. h2s get 3.5rem above, a 1px ink rule and 0.875rem below it. Pagination is an auto-fit grid of turn-to rows (min 18rem), next always in the last column.

## Elevation & Depth

Flat. No shadows anywhere: Starlight's search dialog, mobile TOC dropdown and expressive-code frames all have shadows removed. Depth is printed: an opaque ink field, a ruled box, a numeral cell that overlaps its box by 1px so its edges are the box's edges.

### Named Rules
**The Printed Depth Rule.** If something must stand forward, give it an ink field or a rule, never a shadow or blur.

## Shapes

Square corners throughout (0px, enforced on search, selects, code, kbd, asides, TOC toggle, code frames). Rules are 1px (landing: `max(1px, 1.5u)`); short 2u spine/sheet rules mark divisions. The single circle is the checker's seal — a code-drawn double ring with text on arcs, rotated −6°. Arrows are drawn SVG (`TurnArrow`), not glyph icons.

## Components

### Turn-to row (signature)
- **Shape:** square ruled row, comp 94u tall (docs min 4.75rem).
- **Cell:** numeral cell in the spot ink with paper numeral, Caslon Condensed.
- **Hover:** row washes `green-tint` (landing) / cell and title shift (docs); arrow nudges 0.12em, 200ms `cubic-bezier(0.16, 1, 0.3, 1)`.
- **Docs pagination:** mono uppercase cue ("turn to") over a Caslon title.

### Install control
- Ink bar (comp 76u): mono command button (`cursor: copy`, `user-select: all`) | 1.5u paper divider | Caslon "go" link. Copied state tints the command `#9fd5b3` and announces via a visually hidden live region. Hover underlines 1px.

### Navigation (spine)
- Green band; Caslon wordmark (640, comp 132.6u); numbered Caslon links (560) with a 2u underline that draws from the left on hover/current (260ms). Mono imprint and meta with locale links.
- **Docs sidebar:** CSS counters `gb-chapter` / `gb-page` print `n` and `n.m`, matching `sectionNumber()` in `src/lib/section.ts`; the current page inverts onto paper.

### Adventure-sheet boxes
- Ruled square boxes, Caslon term over a hairline, value below in lining figures. A declared default shows in Ink 3 italic; a change strikes the previous value and writes the new one in green 700 with the write-in tick.

### Ledger rows
- Two-column ruled rows (13em term / text) with 1px rules top and bottom — used instead of cards. Tables likewise: ruled, never boxed.

### Asides (docs)
- Ruled box; title is a filled mono uppercase cell overlapping the corner. Note = ink cell, tip = green, caution = red outline (red-ink text), danger = red fill. Icons hidden.

### Field sections
- Landing chapters can take a full cover-ink field (blue or red): paper becomes the spot, the field ink the ground, rules turn paper.

### Book index
- Two-column ruled index: Caslon numeral, Caslon label, mono tracked page range.

### Code
- Expressive Code, 0 radius, 1px border, no shadow, mono UI at 0.6875rem. Light theme (`gamebook-light`, paper-2 ground: one black ink, green keywords, red strings, blue data) is the default; `gamebook-dark` applies only on `:root[data-theme="dark"]` docs, never on the landing.

### Motion
- **Page turn:** the `gb-page` view transition wipes the old page away left-to-right (`clip-path`, 300ms, `cubic-bezier(0.16, 1, 0.3, 1)`).
- **Write-in tick:** sheet values reveal by clip (520ms). **Re-stamp:** seal settles from 1.08 scale (360ms).
- **Reduced motion:** all landing animation and transition off.

### Browser surfaces
- Selection: green ground, paper text (spine selection inverts). Focus: 2px outline, 3px offset in the accent (landing: currentColor, 4px offset). Scrollbar: gray-4 thumb on transparent.

## Do's and Don'ts

### Do:
- **Do** fill regions with a cover ink (spine, numeral cell, field section) and print everything else in one ink on paper.
- **Do** number things: section numerals from `sectionNumber()`/counters, turn-to cells, h2 `n.m` prefixes.
- **Do** express sets of facts as ruled ledger rows or ruled tables.
- **Do** use only real `.lute` code and real CLI output; read versions from `src/lib/versions.ts`.
- **Do** set Caslon numerals with `lining-nums`; keep Hangul `keep-all`, upright, and tightly tracked.
- **Do** write landing geometry in comp pixels (`calc(N * var(--u))`) and let spare width go to the spine.

### Don't:
- **Don't** round corners or add shadows; the seal is the only circle.
- **Don't** use cards; use ledger rows.
- **Don't** apply `tabular-nums` to numerals that carry punctuation.
- **Don't** uppercase file names, commands or CLI output.
- **Don't** let the landing go dark or follow the OS color preference.
- **Don't** fake italics on Hangul.
- **Don't** use glyph icons; arrows are drawn SVG.
