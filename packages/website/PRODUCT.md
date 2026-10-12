# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

- **Scenario writers and narrative designers** (primary). They write dialogue, choices,
  quests and cinematic beats in `.lute` source and need the language to read like a script,
  with clear rules for what is allowed and fast answers when the checker refuses something.
- **Game engineers** evaluating or integrating Lute: they read the runtime contract, the IR
  and event schemas, the CLI, and the runtime/player packages to decide whether and how their
  engine consumes Lute output.
- **AI agents** (and the people running them) that read the language reference through
  `llms.txt` / `llms-full.txt` and the AI-harness pages to write and fix `.lute` documents
  under the checker's guidance.

All three are confirmed audiences; none outranks the others on the documentation pages.

## Product Purpose

Lute is a scenario-authoring language and toolchain for branching game narrative —
visual-novel episodes, story-driven RPG quests, branching cinematic scenes, and the
conditions and rewards that tie them together. The website introduces Lute to newcomers and
is the reference everyone returns to.

Success has two halves, both required:

1. **Landing (persuade):** a first-time visitor understands what Lute is, why it differs
   from other narrative tools, and tries it (install command, playground, first-scene
   tutorial).
2. **Documentation (read):** someone already using Lute finds the syntax rule, runtime
   contract, or CLI flag they need quickly and can read long reference pages comfortably.

## Positioning

Lute is **total, not Turing-complete**: what an author writes is statically checkable and
compiles to data an engine replays — versioned JSON IR plus CEL conditions. The checker
proves reachability and simulates outcomes before anything ships ("what ships is what you
analyzed"). Engines either embed the official runtime (`@lute-lang/runtime`, pure step
function, wasm) or interpret the IR themselves against a conformance corpus. The player
(`@lute-lang/player`, `@lute-lang/player-web`) renders the story; the game owns the world.

## Operating Context

- Site: Astro Starlight in `packages/website`, deployed via Vercel; English at `/` and
  Korean at `/ko/`.
- Sections: getting started, language, state, connectivity, plugins, tooling, player,
  reference (cheatsheet, diagnostics, reserved names), guides, examples, spec/history.
- Interactive playground at `/playground/` (browser check/compile/trace of a scene).
- Generated pages (diagnostics reference, reserved names) and hand-maintained LLM mirrors
  (`public/llms.txt`, `public/llms-full.txt`) must stay consistent with page content.
- Code samples are checked in CI against the real `lute` binary (`scripts/check-doc-snippets.py`).

## Capabilities and Constraints

- **Astro (Starlight) stays.** Everything else — theme, components, landing composition,
  typography, color, imagery, logo — may change.
- English and Korean must both read well; Korean body text is long-form and needs a
  typeface that sets Hangul properly.
- Documentation pages are long and code-heavy (`.lute`, YAML, JSON, shell); code blocks are
  first-class content.
- The landing hero currently uses an illustration (`src/assets/hero-lute.png`) and a
  serif wordmark; neither is binding.

## Evidence on Hand

- Real example projects under `docs/examples/` (games such as monster-league, seven-days,
  lighthouse-keeper, drowned-crown) and the scene used on the landing page.
- Real CLI output can be generated with the `lute` binary for any shown command.
- No testimonials, adopters, benchmarks or customer logos exist; do not fabricate any.

## Product Principles

1. Show real `.lute` and real tool output; never mock syntax or results.
2. Authors first in tone and examples; contracts and schemas precise enough for engineers
   and agents.
3. The checker's guarantees (reachability, totality, versioned contract) are the argument;
   state them plainly instead of marketing them.
4. English and Korean are equal citizens.
