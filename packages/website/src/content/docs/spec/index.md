---
title: Specification index
description: The versioned Lute spec stack — every scenario-DSL revision plus the plugin-system and character-cast capability proposals, with links to the normative repository sources.
---

Lute is specified as a **stack of versioned proposals** under [`docs/proposals/`](https://github.com/journeyWorker/lute/tree/main/docs/proposals). Each revision is a compatible refinement or extension of the one before, so the stack reads cumulatively: `0.1.0` is the scene kind + shared kernel, and every later revision cites it.

:::note
The **repository files are the normative source of truth.** This site is the readable companion — where the two differ, the proposal in the repo wins. The current language version is **0.10.1**.
:::

## Scenario DSL (the language)

| Version | Scope | Source |
|---|---|---|
| 0.0.1 | First pre-implementation draft of the authoring language — lexical structure, grammar, and semantics of a `.lute` scenario. | [0.0.1.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.0.1.md) |
| 0.1.0 | The language proper — scene kind + shared kernel: logic layer, Lute-CEL, scalar state model, totality, identity/i18n, and reusable content components. | [0.1.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.1.0.md) |
| 0.2.0 | Document-kind system (making `.lute` polymorphic), the `<on>` ECA trigger, the `quest.*` state tier, and the quest kind in full. | [0.2.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.2.0.md) |
| 0.3.0 | Relational fact layer — a closed SVO/n-ary fact database with valid-time intervals, delta assertion/retraction, and a total Datalog derivation layer. | [0.3.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md) |
| 0.4.0 | Writer-experience layer — `lute trace` preview, provable softlock/dead-content diagnostics, param-scoped component `<match>` dispatch, ceremony sugar, and diagnostic presentation. | [0.4.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.4.0.md) |
| 0.5.0 | Authoring-feedback hardening — diagnostic specificity, trace honesty, the reachability boundary, and `lute context` completeness. | [0.5.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.5.0.md) |
| 0.5.1 | `trace` preview of quest-gated reads plus authoring-surface honesty items (delivery flags, `lute context` reserved paths, event/component diagnostics). | [0.5.1.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.5.1.md) |
| 0.5.2 | A single new `E-UNSET-LITERAL` diagnostic catching the most common misspelling of the *unset* sentinel in a CEL guard. | [0.5.2.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.5.2.md) |
| 0.6.0 | `into=` records without `persist=`; free shot headings — **breaking**. | [0.6.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.6.0.md) |
| 0.6.1 | Three verification-coverage warnings (`W-UNPROVEN-RELATIONAL`, `W-LUTE-VERSION-STALE`, `W-TRACE-MOCK-UNPRODUCIBLE`) and `--deny`/`--deny-warnings` warning promotion; a compatible refinement of 0.6.0. | [0.6.1.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.6.1.md) |
| 0.7.0 | Version unification — language, IR, and toolchain aligned at `0.7.0`; **no grammar, semantic, or IR shape change** (byte-for-byte `0.6.1` semantics). | [0.7.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.7.0.md) |
| 0.8.0 | The **adoption release**: `::end` termination, `after: active()`, the reserved `quest.<id>.activatedAt` anchor, scalar-only author `state:`, `Artifact.shots`, the localization round trip, `identity:` templates, and a uniform-width `addr`; every item traces to a gap found assessing Lute against a real 777-scene / 583-quest game catalog. | [0.8.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.8.0.md) |
| 0.9.0 | **Vocabulary ownership**: the compiler declares the seven content-vocabulary *slots* (`emotion`, `action`, `anchor`, `mood`, `volume`, `musicAction`, `vfxType`) and ships **no members**, so every member is the project's, declared inline, in a schema reached through `uses:`, or by a plugin's `enums` export; the `enums:` long form's `exits:`/`default:` carries the member semantics two hardcoded name heuristics used to guess; using a slot nobody declared is `E-DOMAIN-UNKNOWN` instead of silently unchecked; and five root-only check stages now run over imported component bodies. **Breaking** for documents that use a slot. The IR re-aligned to `0.9.0` and was shape-identical to `0.8.0`. | [0.9.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.9.0.md) |
| 0.10.0 | **the toolchain says what it knows**: thirteen changes, each closing a place where the checker held the answer and did not say it. `::set` types its right-hand side against the path it writes (`E-SET-TYPE`); the six logic tags close their attribute sets (`E-UNKNOWN-ATTR`, `E-AS-REMOVED`); a quest gate that can never open is an error and a contradictory objective pair is `E-OBJECTIVE-CONTRADICTION`; `lute.project.yaml` gains `defaults:`; nested manifests are validated by every command that walks them; `mocks/*.yaml` is checked and requires `file:`; timeline time is integer milliseconds; four new warnings land and `W-INJECT-CONFLICT` is **removed** because its only trigger was agreement. **Breaking** for documents (§§3, 4, 5) and for mocks. The **IR shape changes for the first time since `0.8.0`** — `provenance.reason` becomes `provenance.explanation`, so an engine gated on IR `0.9` must widen to `0.10`. | [0.10.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.10.0.md) |
| **0.10.1** | Current tip — **alignment only**: no grammar production, no static-semantic rule, no IR shape change; `0.10.0` semantics byte for byte. The release's two fixes are toolchain — `lute test` gains `--project` so a document that reaches its schema or its directives through a manifest can be tested at all, and an `assetKind` segment declared with a `Type` outside the closed segment production is rejected at plugin load instead of enforcing nothing. Written down because a language version absent from this stack cannot be told apart from one nobody recorded. | [0.10.1.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.10.1.md) |
| — | State-model design rationale & audit record — *why* the four-tier (`scene`/`run`/`user`/`app`) state model is shaped this way; non-normative companion to `0.0.1` §9. | [state-model-design.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/state-model-design.md) |

For the authoring-side reading of the seven vocabulary slots — the three declaration
routes and their precedence, and the `exits:`/`default:` long form — see
[Content vocabulary](/language/vocabulary/).

## Capability proposals

| Proposal | Scope | Source |
|---|---|---|
| Plugin system 0.0.1 | Normative formats and semantics of the capability/plugin system — plugin packages, YAML manifest schemas, resolution, the capability snapshot, and the data↔code boundary. | [plugin-system/0.0.1.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/plugin-system/0.0.1.md) |
| Plugin system 0.0.2 | Delta on 0.0.1 — plugin option and frontmatter value validation, reserved stamp-attribute rejection, cross-cutting `stampAttrs` admissible on every directive and content line, and the declarative `lower: { record, fields }` form implemented. | [plugin-system/0.0.2.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/plugin-system/0.0.2.md) |
| Plugin system 0.0.3 | Delta on 0.0.1/0.0.2, landing with dsl 0.9.0 — `lute.core` exports an **empty** `enums`, an `enums` entry may use the long form carrying member semantics (`members`/`default`/`exits`), and the closed `semantics` flag vocabulary drops the two flags no consumer read (`isStateful`, `cancelsPrevious`), twelve → ten. | [plugin-system/0.0.3.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/plugin-system/0.0.3.md) |
| Character & cast 0.0.1 | The language contract for character identity, display label, costume, name-reveal, and voice-join — as a capability plugin (registry, `cast:` frontmatter, `scene.cast.*` state, resolution, `seal`/`reveal`/`wear`). | [character-cast/0.0.1.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/character-cast/0.0.1.md) |

The plugin system also carries a human-facing overview at [`docs/plugin-system.md`](https://github.com/journeyWorker/lute/blob/main/docs/plugin-system.md), and the character/cast capability its design rationale at [`character-cast/design.md`](https://github.com/journeyWorker/lute/blob/main/docs/proposals/character-cast/design.md).
