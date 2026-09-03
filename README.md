# Lute

Lute is a scenario-authoring language and toolchain for branching game narrative — visual-novel
episodes, quests, and the conditions and rewards that tie them together. Authored `.lute`
documents compile to flat engine command records plus CEL condition strings: the language is
**total**, not Turing-complete, so what an author writes is statically checkable and reduces to
data an engine replays.

**New to Lute and just want to write a scene?** Skip the spec stack for now — start with
[**Write your first Lute scene**](docs/getting-started-first-scene.md), a linear, hands-on
tutorial that builds one small scene from an empty file.

## Install

The toolchain ships on npm as [`@lute-lang/lute`](https://www.npmjs.com/package/@lute-lang/lute) —
a launcher that resolves prebuilt native binaries for your platform (darwin-arm64, linux-x64,
win32-x64) and installs two bins: `lute` (CLI) and `lute-lsp` (language server):

```sh
bun add -g @lute-lang/lute     # or: npm i -g @lute-lang/lute
lute check scene.lute
bunx @lute-lang/lute check scene.lute   # no install
```

Building from source instead: `cargo install --path crates/lute-cli --path crates/lute-lsp`.

The website — landing, guides, language reference, CLI docs (en + 한국어) — lives in
[`packages/website`](packages/website) (Astro Starlight, deployed via Vercel).

## The CLI surface

```sh
lute init my-project           # scaffold project + starter scene + mock
lute new quest the-hunt        # scaffold one document (scene | quest | schema)
lute check scene.lute          # static validation of one document
lute check-project .           # every document + project-wide passes (quest ids, connectivity)
lute lint .                    # advisory L-* findings governed by lute.lint.yaml
lute trace scene.lute --mock m.yaml   # preview SOURCE behaviour, every decision explained
lute compile scene.lute -o out.json   # JSON artifact (--all for a whole project + index)
lute run out.json --mock m.yaml       # reference runtime over the ARTIFACT an engine consumes
lute play my-project --auto first     # a whole scheduled route as one chained transcript
lute test .                    # *.test.yaml scenario tests (transcript/state/quest assertions)
lute loc export . --format csv # localization round trip (+ import, word-count report)
lute scenario .                # read-only graph / reachability / envelope reporting
lute tag scene.lute            # back-fill stable line codes (localization identity)
lute context scene.lute        # the project-resolved authoring surface (for AI authoring)
lute doctor                    # diagnose local toolchain + project setup
lute version                   # the three independent version axes
```

## Documents by role

Each document owns one role; read the one that matches what you are doing.

| If you are… | Normative spec (source of truth) | Overview / rationale |
|---|---|---|
| **writing `.lute` scenarios** | the versioned spec stack: base [`0.1.0`](docs/proposals/scenario-dsl/0.1.0.md) plus per-release deltas through the current tip [`0.16.0`](docs/proposals/scenario-dsl/0.16.0.md) (declarative rewards). [`docs/versioning.md`](docs/versioning.md) lists every release and what each axis earned. | the examples below; [`architecture.md`](docs/architecture.md) |
| **authoring quests** (lifecycle, objectives, subquests, rewards) | [`0.2.0`](docs/proposals/scenario-dsl/0.2.0.md) §6 (quest kind, objectives, lifecycle events) + [`0.14.0`](docs/proposals/scenario-dsl/0.14.0.md) (subquests) + [`0.16.0`](docs/proposals/scenario-dsl/0.16.0.md) (`<reward/>`) | [`runtime/quest-lifecycle.md`](docs/runtime/quest-lifecycle.md) |
| **writing a plugin** (directives, state, providers, bridge, `stampAttrs`, `rewardKinds`) | [`proposals/plugin-system/0.0.1.md`](docs/proposals/plugin-system/0.0.1.md) — manifest YAML schemas + resolution — plus the [`0.0.2`](docs/proposals/plugin-system/0.0.2.md)–[`0.0.5`](docs/proposals/plugin-system/0.0.5.md) deltas | [`plugin-system.md`](docs/plugin-system.md) |
| **building an engine** (consuming the artifact) | [`docs/runtime/`](docs/runtime) — execution model, quest lifecycle, state lifecycle, timeline semantics, CEL & facts, bridge protocol — plus the artifact JSON Schema [`schemas/lute-ir-0.16.schema.json`](schemas/lute-ir-0.16.schema.json) and the [`conformance/`](conformance) fixtures | [`architecture.md`](docs/architecture.md) |
| **building the compiler / checker / LSP** | the proposals above | [`architecture.md`](docs/architecture.md) — two-tier AST, auto-injection, the `check()` core, LSP |
| **reasoning about run / user / app state** | [`0.1.0`](docs/proposals/scenario-dsl/0.1.0.md) §9 (scalar tiers) + [`0.3.0`](docs/proposals/scenario-dsl/0.3.0.md) (relational facts + Datalog) | [`state-model-design.md`](docs/proposals/scenario-dsl/state-model-design.md) |
| **authoring characters** (label / costume / `???` reveal / voice) | [`proposals/character-cast/0.0.1.md`](docs/proposals/character-cast/0.0.1.md) — cast contract | [`character-cast/design.md`](docs/proposals/character-cast/design.md) |
| **scheduling routes / running `lute play`** | [`specs/2026-08-14-lute-schedule-and-play-design.md`](docs/superpowers/specs/2026-08-14-lute-schedule-and-play-design.md) — `schedule.yaml`'s tick clock, lanes, guarded placements; `lute play`'s transcript and coverage semantics | [`docs/schedule-and-play.md`](docs/schedule-and-play.md) |
| **configuring lints** | [`specs/2026-08-26-lute-lint-system-design.md`](docs/superpowers/specs/2026-08-26-lute-lint-system-design.md) — `lute.lint.yaml`, rule levels, project-local `custom:` rules | [`docs/linting.md`](docs/linting.md) |

Worked examples:

- [`docs/examples/anseo/`](docs/examples/anseo) — a whole small project: scenes, quests, a shared
  world schema, a component, and a `schedule.yaml`.
- [`docs/examples/bianca-s01ep02.lute`](docs/examples/bianca-s01ep02.lute) — linear episode faithful
  to a real catalog episode; comments, `::camera`, a multi-track `<timeline>`, and a
  `<branch>`/`<match>`/state callback.
- [`docs/examples/quest-grove.lute`](docs/examples/quest-grove.lute) — quest with objectives,
  lifecycle handlers, and declarative rewards on both completion and failure.
- [`docs/examples/idola-project/date-minigame.lute`](docs/examples/idola-project/date-minigame.lute) —
  plugin-system demo: a `profile`, scene-local plugin options, a bridge `::minigame`, and a
  `<match>` on its declared result slot.

**Normative specs** (the strict contract) live under [`docs/proposals/`](docs/proposals) and
[`docs/runtime/`](docs/runtime); the **architecture & rationale** docs
([`docs/architecture.md`](docs/architecture.md), [`docs/plugin-system.md`](docs/plugin-system.md),
and the state-model rationale) are the human-facing companions that explain how it is built and why.

## Core ideas

- **Fixed grammar, typed capabilities.** Plugins add directive vocabulary, state shapes, providers,
  bridge signatures, reward kinds, and diagnostics — never arbitrary grammar (see
  [`docs/plugin-system.md`](docs/plugin-system.md)).
- **Conditions and rewards are declarations, not code.** A quest's `start`/`fail`, an objective's
  `done`, and a `<reward/>` are checked data: reachability, satisfiability, and vocabulary are
  verified at build time, and the compiled artifact carries them for journals and balancing.
- **Profiles select capability sets.** A root-level `profile` selects the active environment for a
  document; the reserved `global` profile is inherited by every other profile.
- **Plugins are configured by id.** `plugins.<pluginId>` activates a plugin and carries its typed
  options. There is no `plugins.use` list.
- **Bridge calls are typed directives.** Runtime systems such as minigames or app surfaces are
  invoked through declared bridge capabilities that write declared state. Story logic observes
  state, not arbitrary tool-call output.
- **Comments use `/* ... */`** in the body (frontmatter uses YAML `#`). Body comments may be
  standalone, inline, trailing, or multi-line; they are stripped before classification and ignored
  inside quoted strings.

## Syntax sketch

A scene — identity, staging, dialogue, and a guarded branch (the `::minigame` bridge and its
`profile` come from the [`idola-project`](docs/examples/idola-project) plugin project, not from
the core language):

```lute
---
kind: scene
id: bianca.s01ep05
luteVersion: "0.16.0"
profile: date-minigame
extra:
  arc: main
---

## Shot 1.
::minigame{kind="rhythm" id="bianca_service_01" resultKey="service01" sync="true"}

<match on="scene.minigame.service01.rank">
  <when test="$ == 'gold'">
    @bianca{code="0030" emotion="delighted" variant="1"}: Wonderful! A perfect service!
  </when>
  <otherwise>
    @bianca{code="0050" emotion="shy" variant="0"}: Shall we try once more? The rhythm takes practice.
  </otherwise>
</match>
```

A quest — conditions and rewards as data (`inParty`/`ownsItem` are project-declared relations;
`findHalsin` is a sibling quest in the same project):

```lute
<quest id="hunt" title="The Hunt" start="holds(inParty(shadowheart))" fail="run.dawnBroke">
  <reward kind="XP" amount="300"/>
  <reward kind="SHARD" amount="1..5" when="run.bonusMet"/>
  <reward kind="SHARD" amount="2" on="failed"/>
  <objective id="track" done="count(ownsItem(tracks)) >= 3">
    <reward kind="GOLD" amount="10"/>
  </objective>
  <objective id="freeHalsin" quest="findHalsin"/>
  <on event="questComplete">
    @narrator: The First Druid drew a slow breath. "You have my thanks."
  </on>
</quest>
```

## Play a scheduled route

A project with a `schedule.yaml` (a tick clock + `user`/`world` lanes + route-guarded
placements beside `lute.project.yaml`) can be replayed end to end as one reviewer-facing
transcript instead of read scene by scene:

```sh
lute play my-project --state run.route=iroha --auto first
lute play my-project --script routes/iroha-a.play.yaml --lanes all --json
lute play my-project --coverage routes/*.play.yaml   # review-gap report across a whole corpus
```

`lute play` compiles the whole project once, walks the schedule's placements in presentation
order (never file order), re-evaluates each event's route-guarded variant against live state,
and threads `run.*`/`user.*`/`app.*`/`quest.*` state and facts across scene boundaries — so a
reviewer sees exactly what one route's player sees, in the order they see it. See
[`docs/schedule-and-play.md`](docs/schedule-and-play.md) for the full `schedule.yaml` key
reference, CLI flags, exit codes, and diagnostic table.

## Editor support

Language support for `.lute` files — diagnostics, hover, completion, go-to-definition,
references, folding, symbols, and highlighting — is provided by the `lute-lsp` stdio
language server plus a thin client per editor. Installing `@lute-lang/lute` installs
`lute-lsp` alongside `lute`; clients for **VS Code**, **Neovim**, and the **Oh My Pi**
harness live under [`editors/`](editors) (see [`editors/README.md`](editors/README.md)).

- **VS Code** — [`editors/vscode/`](editors/vscode) (extension + TextMate grammar).
- **Neovim** — [`editors/nvim/`](editors/nvim) (filetype + LSP autostart + tree-sitter).
- **Oh My Pi** — [`.omp/lsp.json`](.omp/lsp.json) auto-detects `lute-lsp` for `.lute`.

## Status

Lute's status splits along three independent axes, held aligned at one visible number per
release (see [`docs/versioning.md`](docs/versioning.md) for the full policy and per-release
history):

- **Language: draft, at 0.16.0.** The normative surface is the versioned spec stack — the
  [`0.1.0`](docs/proposals/scenario-dsl/0.1.0.md) base plus every delta up to
  [`0.16.0`](docs/proposals/scenario-dsl/0.16.0.md). Recent tips: `0.13.0` code locking and the
  lint layer, `0.14.0` subquests, `0.15.0` authored scene identity (`id:` + the descriptive
  `extra:` block), `0.16.0` declarative rewards. Being draft means the grammar may still break
  before 1.0; each breaking change ships a `lute fix` migration where the rewrite is mechanical.
- **IR: 0.16.0.** The compiled artifact is specified by
  [`schemas/lute-ir-0.16.schema.json`](schemas/lute-ir-0.16.schema.json) and the
  [`docs/runtime/`](docs/runtime) contract, with executable
  [`conformance/`](conformance) fixtures. Engines gate on `irVersion` by **MAJOR** only
  (since `0.13.0`): fields are append-only within a major line, so a minor move costs a
  consumer nothing.
- **Implementation: shipped.** The checker, compiler, provider/plugin resolver, reference
  runtime, LSP, and CLI are implemented, tested Rust crates under [`crates/`](crates)
  (including `lute-syntax`, `lute-manifest`, `lute-check`, `lute-compile`, `lute-trace`, `lute-lint`,
  `lute-cli`, `lute-lsp`), with editor clients under [`editors/`](editors) and npm
  distribution under [`packages/`](packages) (`@lute-lang/lute` + platform binary packages).
  Run `lute version` to print all three axes.
- **Production stability: not yet guaranteed.** Because the grammar and the artifact schema may
  still move before 1.0, pin the toolchain version and validate artifacts against the
  `irVersion` you target.

The toolchain is MIT-licensed ([`LICENSE`](LICENSE)); releases are tracked in
[`CHANGELOG.md`](CHANGELOG.md).
