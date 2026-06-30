# Lute

Lute is the scenario-authoring language and compiler surface for visual-novel episodes. This repository starts with the language design documents extracted from Bard so the DSL can evolve as its own project instead of being hidden inside the broader Bard workspace.

The current language is a draft. It is intentionally small, line-oriented, and data-reducible: authored `.lute` scenario files compile to flat engine command records plus CEL condition strings. The language is not Turing-complete.

## What is here

- [`docs/scenario-dsl-spec.md`](docs/scenario-dsl-spec.md) — architecture/design draft covering the authoring surface, AST, compiler, checker/LSP, capability profiles, plugin packs, typed bridge calls, and roadmap.
- [`docs/proposals/scenario-dsl/0.0.1.md`](docs/proposals/scenario-dsl/0.0.1.md) — normative language proposal for Lute Scenario DSL v0.0.1.
- [`docs/proposals/scenario-dsl/state-model-design.md`](docs/proposals/scenario-dsl/state-model-design.md) — rationale and audit trail for the scene/run/user/app state model.
- [`docs/examples/bianca-s01ep02.lute`](docs/examples/bianca-s01ep02.lute) — worked example showing comments, camera directives, timeline tracks, branching, matching, and state.

## Core ideas

- **Fixed grammar, typed capabilities.** Plugins add directive vocabulary, state shapes, providers, bridge signatures, and diagnostics. They do not add arbitrary grammar.
- **Profiles select capability sets.** A root-level `profile` selects the active environment for a scene. Reserved `global` profile settings are inherited by all profiles.
- **Plugins are configured by id.** `plugins.<pluginId>` activates a plugin and carries its typed options. There is no `plugins.use` list.
- **Bridge calls are typed directives.** Runtime systems such as minigames or app surfaces are invoked through declared bridge capabilities that write declared state. Story logic observes state, not arbitrary tool-call output.
- **Comments use `/* ... */`.** Body comments may be standalone, inline, trailing, or multi-line; they are stripped before body classification and are ignored inside quoted strings.

## Draft syntax sketch

```lute
---
character: bianca
season: 1
episode: 5
lang: "0.0.1"
profile: date-minigame
---

::minigame{kind="rhythm" id="bianca_service_01" resultKey="service01" wait="true"}

<match on="scene.minigame.service01.rank">
  <when test="$ == 'gold'">
    :line[bianca]{code="0200"}: Wonderful!
  </when>
  <otherwise>
    :line[bianca]{code="0210"}: Shall we try again?
  </otherwise>
</match>
```

## Status

Draft / pre-implementation. The documents define the target language and architecture; compiler/LSP implementation work is not yet in this repository.
