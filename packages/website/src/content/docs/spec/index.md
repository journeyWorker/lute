---
title: Specification index
description: The versioned Lute spec stack — every scenario-DSL revision plus the plugin-system and character-cast capability proposals, with links to the normative repository sources.
---

Lute is specified as a **stack of versioned proposals** under [`docs/proposals/`](https://github.com/KantoRegion/lute/tree/main/docs/proposals). Each revision is a compatible refinement or extension of the one before, so the stack reads cumulatively: `0.1.0` is the scene kind + shared kernel, and every later revision cites it.

:::note
The **repository files are the normative source of truth.** This site is the readable companion — where the two differ, the proposal in the repo wins. The current language version is **0.5.2**.
:::

## Scenario DSL (the language)

| Version | Scope | Source |
|---|---|---|
| 0.0.1 | First pre-implementation draft of the authoring language — lexical structure, grammar, and semantics of a `.lute` scenario. | [0.0.1.md](https://github.com/KantoRegion/lute/blob/main/docs/proposals/scenario-dsl/0.0.1.md) |
| 0.1.0 | The language proper — scene kind + shared kernel: logic layer, Lute-CEL, scalar state model, totality, identity/i18n, and reusable content components. | [0.1.0.md](https://github.com/KantoRegion/lute/blob/main/docs/proposals/scenario-dsl/0.1.0.md) |
| 0.2.0 | Document-kind system (making `.lute` polymorphic), the `<on>` ECA trigger, the `quest.*` state tier, and the quest kind in full. | [0.2.0.md](https://github.com/KantoRegion/lute/blob/main/docs/proposals/scenario-dsl/0.2.0.md) |
| 0.3.0 | Relational fact layer — a closed SVO/n-ary fact database with valid-time intervals, delta assertion/retraction, and a total Datalog derivation layer. | [0.3.0.md](https://github.com/KantoRegion/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md) |
| 0.4.0 | Writer-experience layer — `lute trace` preview, provable softlock/dead-content diagnostics, param-scoped component `<match>` dispatch, ceremony sugar, and diagnostic presentation. | [0.4.0.md](https://github.com/KantoRegion/lute/blob/main/docs/proposals/scenario-dsl/0.4.0.md) |
| 0.5.0 | Authoring-feedback hardening — diagnostic specificity, trace honesty, the reachability boundary, and `lute context` completeness. | [0.5.0.md](https://github.com/KantoRegion/lute/blob/main/docs/proposals/scenario-dsl/0.5.0.md) |
| 0.5.1 | `trace` preview of quest-gated reads plus authoring-surface honesty items (delivery flags, `lute context` reserved paths, event/component diagnostics). | [0.5.1.md](https://github.com/KantoRegion/lute/blob/main/docs/proposals/scenario-dsl/0.5.1.md) |
| **0.5.2** | Current tip — a single new `E-UNSET-LITERAL` diagnostic catching the most common misspelling of the *unset* sentinel in a CEL guard. | [0.5.2.md](https://github.com/KantoRegion/lute/blob/main/docs/proposals/scenario-dsl/0.5.2.md) |
| — | State-model design rationale & audit record — *why* the four-tier (`scene`/`run`/`user`/`app`) state model is shaped this way; non-normative companion to `0.0.1` §9. | [state-model-design.md](https://github.com/KantoRegion/lute/blob/main/docs/proposals/scenario-dsl/state-model-design.md) |

## Capability proposals

| Proposal | Scope | Source |
|---|---|---|
| Plugin system 0.0.1 | Normative formats and semantics of the capability/plugin system — plugin packages, YAML manifest schemas, resolution, the capability snapshot, and the data↔code boundary. | [plugin-system/0.0.1.md](https://github.com/KantoRegion/lute/blob/main/docs/proposals/plugin-system/0.0.1.md) |
| Character & cast 0.0.1 | The language contract for character identity, display label, costume, name-reveal, and voice-join — as a capability plugin (registry, `cast:` frontmatter, `scene.cast.*` state, resolution, `seal`/`reveal`/`wear`). | [character-cast/0.0.1.md](https://github.com/KantoRegion/lute/blob/main/docs/proposals/character-cast/0.0.1.md) |

The plugin system also carries a human-facing overview at [`docs/plugin-system.md`](https://github.com/KantoRegion/lute/blob/main/docs/plugin-system.md), and the character/cast capability its design rationale at [`character-cast/design.md`](https://github.com/KantoRegion/lute/blob/main/docs/proposals/character-cast/design.md).
