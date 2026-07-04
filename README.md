# Lute

Lute is the scenario-authoring language and compiler surface for visual-novel episodes. This
repository holds the language design documents extracted from Bard so the DSL can evolve as its own
project instead of being hidden inside the broader Bard workspace.

The language is a draft: intentionally small, line-oriented, and data-reducible. Authored `.lute`
scenario files compile to flat engine command records plus CEL condition strings. The language is
**total**, not Turing-complete.

## Documents by role

Each document owns one role; read the one that matches what you are doing.

| If you are… | Normative spec (source of truth) | Overview / rationale |
|---|---|---|
| **writing `.lute` scenarios** | [`proposals/scenario-dsl/0.0.1.md`](docs/proposals/scenario-dsl/0.0.1.md) — language grammar + semantics | the examples below; [`architecture.md`](docs/architecture.md) *New additions* |
| **writing a plugin** (directives, state, providers, bridge) | [`proposals/plugin-system/0.0.1.md`](docs/proposals/plugin-system/0.0.1.md) — manifest YAML schemas + resolution | [`plugin-system.md`](docs/plugin-system.md) |
| **building the compiler / checker / LSP** | both proposals above | [`architecture.md`](docs/architecture.md) — two-tier AST, auto-injection, the `check()` core, LSP |
| **reasoning about run / user / app state** | [`proposals/scenario-dsl/0.0.1.md`](docs/proposals/scenario-dsl/0.0.1.md) §9 | [`state-model-design.md`](docs/proposals/scenario-dsl/state-model-design.md) |
| **authoring characters** (label / costume / `???` reveal / voice) | [`proposals/character-cast/0.0.1.md`](docs/proposals/character-cast/0.0.1.md) — cast contract | [`character-cast/design.md`](docs/proposals/character-cast/design.md) |

Worked examples:

- [`docs/examples/bianca-s01ep02.lute`](docs/examples/bianca-s01ep02.lute) — linear episode faithful
  to the real catalog S01EP02; shows comments, `::camera`, a multi-track `<timeline>`, and a
  `<branch>`/`<match>`/state callback.
- [`docs/examples/date-minigame.lute`](docs/examples/date-minigame.lute) — illustrative plugin-system
  demo: a `profile`, scene-local plugin options, a bridge `::minigame`, and a `<match>` on its
  declared result slot.

**Normative specs** (the strict contract) live under [`docs/proposals/`](docs/proposals); the
**architecture & rationale** docs ([`docs/architecture.md`](docs/architecture.md),
[`docs/plugin-system.md`](docs/plugin-system.md), and the state-model rationale) are the
human-facing companions that explain how it is built and why.

## Core ideas

- **Fixed grammar, typed capabilities.** Plugins add directive vocabulary, state shapes, providers,
  bridge signatures, and diagnostics — never arbitrary grammar (see
  [`docs/plugin-system.md`](docs/plugin-system.md)).
- **Profiles select capability sets.** A root-level `profile` selects the active environment for a
  scene; the reserved `global` profile is inherited by every other profile.
- **Plugins are configured by id.** `plugins.<pluginId>` activates a plugin and carries its typed
  options. There is no `plugins.use` list.
- **Bridge calls are typed directives.** Runtime systems such as minigames or app surfaces are
  invoked through declared bridge capabilities that write declared state. Story logic observes
  state, not arbitrary tool-call output.
- **Comments use `/* ... */`** in the body (frontmatter uses YAML `#`). Body comments may be
  standalone, inline, trailing, or multi-line; they are stripped before classification and ignored
  inside quoted strings.

## Draft syntax sketch

An excerpt of [`docs/examples/date-minigame.lute`](docs/examples/date-minigame.lute):

```lute
---
character: bianca
season: 1
episode: 5
luteVersion: "0.0.1"
profile: date-minigame
---

::minigame{kind="rhythm" id="bianca_service_01" resultKey="service01" wait="true"}

<match on="scene.minigame.service01.rank">
  <when test="$ == 'gold'">
    :line[bianca]{code="0030" emotion="delighted" variant="1"}: Wonderful! A perfect service!
  </when>
  <otherwise>
    :line[bianca]{code="0050" emotion="shy" variant="0"}: Shall we try once more? The rhythm takes practice.
  </otherwise>
</match>
```

## Editor support

Language support for `.lute` files — diagnostics, hover, completion, go-to-definition,
references, folding, symbols, and highlighting — is provided by the `lute-lsp` stdio
language server plus a thin client per editor. Clients for **VS Code**, **Neovim**, and
the **Oh My Pi** harness live under [`editors/`](editors); see
[`editors/README.md`](editors/README.md) for setup.

Install the server once (`cargo install --path crates/lute-lsp`), then:

- **VS Code** — [`editors/vscode/`](editors/vscode) (extension + TextMate grammar).
- **Neovim** — [`editors/nvim/`](editors/nvim) (filetype + LSP autostart + tree-sitter).
- **Oh My Pi** — [`.omp/lsp.json`](.omp/lsp.json) auto-detects `lute-lsp` for `.lute`.

## Status

Draft / pre-implementation. The documents define the target language and architecture; compiler/LSP
implementation work is not yet in this repository.
