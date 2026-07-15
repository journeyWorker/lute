---
title: Plugin concepts
description: How Lute plugins extend vocabulary and capability surfaces without ever touching the fixed grammar or compiler behavior.
---

Lute has a **fixed grammar** and a **total, non-Turing** core. A plugin never changes either. It adds only *vocabulary and capability surfaces* that the grammar already refers to — directive names, attributes, enum domains, state shapes, providers, bridge signatures, definitions, frontmatter keys, asset kinds, and diagnostics. Everything a plugin declares is **data**, validated against the manifest schemas and resolved, together with a selected profile, into a single deterministic **capability snapshot** that the checker, LSP, and compiler all consume.

## The one rule

Plugins add vocabulary; the core owns grammar and behavior. Control flow, lowering algorithms, exhaustiveness, and auto-injection all live in the compiler — never in a manifest.

## What a plugin MAY add

- **Directive vocabulary** — new `::name` staging or bridge directives, with typed `attrs`, `semantics` flags, and declared state effects (see [Manifests](/plugins/manifests/)).
- **State shapes** — reusable typed records and structured path templates that open typed slots.
- **Providers** — snapshot-first id registries (character ids, asset ids, minigame ids) resolved against a pinned catalog, never a live network call.
- **Bridge signatures** — typed runtime calls whose declared result fields are the only values a directive may write back into story state (see [Bridge](/plugins/bridge/)).
- **Enums, defs, frontmatter keys, asset kinds, and world events** — closed value sets, shared typed-CEL `@refs`, plugin-owned `---` meta keys, structured asset-id templates, and `<on event>` triggers.
- **Diagnostics** — per-attribute messages, deprecations, aliases, and fix-its.

## What a plugin MUST NOT add

Grammar productions, new bracket forms, new block kinds, new control flow, or any behavior. A capability is registrable as plugin data **only if** all hold: fixed syntax; validation local to its attrs plus catalog lookup; lowering is a finite attrs→records mapping; no new control flow; no cross-sibling or global reasoning beyond declared resource conflicts; no AST-shape change; no ordering-sensitive interpretation beyond the existing timeline/`wait` model. If any condition fails, the capability is **code** — a compiler-core change — and does not belong in a plugin.

The trap to name: authors most want things that *feel* like vocabulary but are compiler behavior — "have her leave naturally", "keep the same pose unless mood changes", "hit this SFX exactly as the line appears". Those are served, if at all, by a **named builtin lowering hook** added to the core, gated by a golden test per directive. **If a directive can't get a clean golden test, it isn't just data — it's code.**

The [character/cast](https://github.com/journeyWorker/lute/blob/main/docs/proposals/character-cast/0.0.1.md) capability is the canonical example: it adds the `cast:` frontmatter key, a `character` provider, `scene.cast.<id>.*` state, the `CH` asset kind, and the `::seal` / `::reveal` / `::wear` directives — all pure desugaring to `::set`, no new semantics.
