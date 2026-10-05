# Lute

Lute is a scenario-authoring language and toolchain for branching game narrative — visual-novel
episodes, quests, and the conditions and rewards that tie them together. Authored `.lute`
documents compile to flat engine command records plus CEL condition strings: the language is
**total**, not Turing-complete, so what an author writes is statically checkable and reduces to
data an engine replays.

**New to Lute and just want to write a scene?** Skip the spec stack for now — start with
[**Write your first Lute scene**](docs/getting-started-first-scene.md), a linear, hands-on
tutorial that builds one small scene from an empty file step by step, running `lute` at
every step.

## Install

The toolchain ships on npm as [`@lute-lang/lute`](https://www.npmjs.com/package/@lute-lang/lute) —

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
lute init my-project           # scaffold project + starter scene + mock (--template beats: occasions, beats, plays, tests)
lute new quest the-hunt        # scaffold one document (scene | quest | lore | schema; scene --occasion <occasion> writes a beat)
lute check scene.lute          # static validation of one document
lute check-project .           # every document + project-wide passes (quest ids, connectivity, fact guards)
lute lint .                    # advisory L-* findings governed by lute.lint.yaml
lute trace scene.lute --mock m.yaml   # preview SOURCE behaviour, every decision explained
lute compile scene.lute -o out.json   # JSON artifact (--all for a whole project + index)
lute run out.json --mock m.yaml       # reference runtime over the ARTIFACT an engine consumes
lute lore my-project           # world-narrative map: lore entries by target/series, facts revealed
lute play my-project --script p.play.yaml   # raise occasions + engine writes, see every beat's verdict + the winner
lute test my-project/tests --project my-project   # execute declared scenario and play expectations
lute beats my-project          # beat eligibility, ordering, and coverage report
lute tag .                     # stamp stable content-line codes for localization and voice assets
lute impact . fact:felled(regent)   # reverse dependency closure (0.36.1)
lute constraints .                  # manifest invariants (0.36.1)
lute calendar my-project --axis run.day=1..7 --axis run.slot=morning,night   # who answers in every cell
lute loc export . --format csv # localization round trip (+ import, word-count report)
lute scenario .                # read-only graph / reachability / envelope reporting (+ `knowledge`: fact producers)
lute context scene.lute        # project-resolved authoring surface
lute fmt .                     # canonical, lossless source formatting (0.36.1)
lute diff before after         # semantic revision diff (0.36.1)
lute patch . patch.json        # atomic revision-checked edit (0.36.1)
lute doctor                    # diagnose local toolchain + project setup
lute version                   # the three independent version axes
```

## Documents by role

Each document owns one role; read the one that matches what you are doing.

| If you are… | Normative spec (source of truth) | Overview / rationale |
|---|---|---|
| **writing `.lute` scenarios** | the versioned spec stack: base [`0.1.0`](docs/proposals/scenario-dsl/0.1.0.md) plus per-release deltas through the current tip [`0.36.1`](docs/proposals/scenario-dsl/0.36.0.md) (names as written, paths like JavaScript: every name a condition reaches is letters, digits, `_` or `-`, not starting with `-`, and a name that is not an identifier is written in a condition as a quoted index, `quest["zero-coke-001"].state`, or a quoted fact argument, `holds('at', ["lab-b2"])`; only a name read bare as `@name` stays an identifier). [`docs/versioning.md`](docs/versioning.md) lists every release and what each axis earned. | the examples below; [`architecture.md`](docs/architecture.md) |
| **writing scenarios, fast** (one page to keep open: every construct, CLI, diagnostics, gotchas) | the spec stack above | the website's [Cheatsheet](https://lute-lang.vercel.app/reference/cheatsheet/) — every snippet on it is compile-checked in CI |
| **authoring quests** (lifecycle, objectives, subquests, rewards) | [`0.2.0`](docs/proposals/scenario-dsl/0.2.0.md) §6 (quest kind, objectives, lifecycle events) + [`0.14.0`](docs/proposals/scenario-dsl/0.14.0.md) (subquests) + [`0.16.0`](docs/proposals/scenario-dsl/0.16.0.md) (`<reward/>`) + [`0.24.0`](docs/proposals/scenario-dsl/0.24.0.md) §2 (`activate="accept"`, `complete="any"`, `by=` / `until=` deadlines, `failedBy`) + [`0.25.0`](docs/proposals/scenario-dsl/0.25.0.md) §§4–5 (subquest and `start` graph anchors, `accept="external"`) + [`0.27.0`](docs/proposals/scenario-dsl/0.27.0.md) §5 (`rearm=`, `tier="season:<name>"`) + [`0.28.0`](docs/proposals/scenario-dsl/0.28.0.md) §§5–6 (`follows=`, `outcome=`, `visibleWhen=`, `failedBy: subquest`) | [`runtime/quest-lifecycle.md`](docs/runtime/quest-lifecycle.md) |
| **authoring lore** (item descriptions, found notes, codex pages, barks) | [`0.19.0`](docs/proposals/scenario-dsl/0.19.0.md) (`kind: lore`, `<entry>`, `entry.<id>.read`, document bundles) + [`0.23.0`](docs/proposals/scenario-dsl/0.23.0.md) §4 (scene-like `<beat>` bundles) | [`runtime/lore-entries.md`](docs/runtime/lore-entries.md) |
| **writing a plugin** (directives, state, providers, bridge, `stampAttrs`, `rewardKinds`, `occasions`) | [`proposals/plugin-system/0.0.1.md`](docs/proposals/plugin-system/0.0.1.md) — manifest YAML schemas + resolution — plus the [`0.0.2`](docs/proposals/plugin-system/0.0.2.md)–[`0.0.7`](docs/proposals/plugin-system/0.0.7.md) deltas; the `occasions:` export is [`0.21.0`](docs/proposals/scenario-dsl/0.21.0.md) §2 | [`plugin-system.md`](docs/plugin-system.md) |
| **building an engine** (consuming the execution IR) | [`docs/runtime/`](docs/runtime) — execution model, semantic negotiation, quest lifecycle, state lifecycle, timeline semantics, CEL & facts, bridge protocol, lore entries, beats and occasions — plus the execution IR JSON Schema [`schemas/lute-ir-0.36.schema.json`](schemas/lute-ir-0.36.schema.json), `lute.engine.yaml` matrices, and the [`conformance/`](conformance) fixtures | [`architecture.md`](docs/architecture.md) |
| **building the compiler / checker / LSP** | the proposals above | [`architecture.md`](docs/architecture.md) — two-tier AST, auto-injection, the `check()` core, LSP |
| **reasoning about run / user / app state** | [`0.1.0`](docs/proposals/scenario-dsl/0.1.0.md) §9 (scalar tiers) + [`0.3.0`](docs/proposals/scenario-dsl/0.3.0.md) (relational facts + Datalog) + [`0.20.0`](docs/proposals/scenario-dsl/0.20.0.md) (what `check-project` proves about fact guards) | [`state-model-design.md`](docs/proposals/scenario-dsl/state-model-design.md) |
| **authoring characters** (label / costume / `???` reveal / voice) | [`proposals/character-cast/0.0.1.md`](docs/proposals/character-cast/0.0.1.md) — cast contract | [`character-cast/design.md`](docs/proposals/character-cast/design.md) |
| **choosing story beats / running `lute play`** | [`0.21.0`](docs/proposals/scenario-dsl/0.21.0.md) — occasions, scene and entry beats (`on` / `target` / `when` / `priority` / `once`), selection order, and `lute play`'s script and transcript — plus [`0.22.0`](docs/proposals/scenario-dsl/0.22.0.md): `engine:` steps, save seeds, `expect:` assertions, run boundaries, occasion target domains — [`0.23.0`](docs/proposals/scenario-dsl/0.23.0.md): `lute beats` / `lute calendar` overviews, `select: sequence`, `also`, bundle beats — [`0.24.0`](docs/proposals/scenario-dsl/0.24.0.md): a declared clock, `once: day` / `once: slot`, `advance:` / `include:` steps, `judge: before`, `bridges:` answers — [`0.25.0`](docs/proposals/scenario-dsl/0.25.0.md): `share` keys spent together, bundle `<beat after=…>` edges, bridge answers that omit unread fields — [`0.26.0`](docs/proposals/scenario-dsl/0.26.0.md) §§5, 7: `target="kind:<kind>"` beats, `::next` followed by trace/test, `eligible:` for an ineligible beat, `advance: { to: … }` — [`0.27.0`](docs/proposals/scenario-dsl/0.27.0.md) §§3–6, 8: `for="kind:<kind>"`, occasion `payload:`, `raisedWhen:` gates, `terminal:`, a finite clock, `once: week`, `seasons:`, `rearm=`, `spentBy:`, beat templates, the manifest's `sequence:` — [`0.28.0`](docs/proposals/scenario-dsl/0.28.0.md) §§2–6: the manifest's `chapters:`, a latching `spentBy`, `select: sequence` judged at each turn, `expect.end`, `outsideRun:` occasions — and [`0.29.0`](docs/proposals/scenario-dsl/0.29.0.md) §§3–5: `W-CHAPTER-STALL` only for a chain, `lute new scene --occasion` writing `on:`, the `terminal:` long form `{ when, persists: true }` | [`runtime/beats-and-occasions.md`](docs/runtime/beats-and-occasions.md); the website's [Playing a story](https://lute-lang.vercel.app/tooling/play/) |
| **configuring lints** | [`specs/2026-08-26-lute-lint-system-design.md`](docs/superpowers/specs/2026-08-26-lute-lint-system-design.md) — `lute.lint.yaml`, rule levels, project-local `custom:` rules | [`docs/linting.md`](docs/linting.md) |

Worked examples:

- [`docs/examples/games/`](docs/examples/games) — fourteen complete games (a detective mystery,
  a roguelike, an otome dating sim, a creature collector, a live-service gacha and more), each
  with a README saying what it shows of the language and how to run it; CI checks every one
  with `lute check-project --deny-warnings`.
- [`docs/examples/haven/`](docs/examples/haven) — a whole small project: scenes, quests, lore
  entries, a shared world schema, and a component.
- [`docs/examples/marina-s01ep02.lute`](docs/examples/marina-s01ep02.lute) — linear episode faithful
  to a real catalog episode; comments, `::camera`, a multi-track `<timeline>`, and a
  `<branch>`/`<match>`/state callback.
- [`docs/examples/quest-grove.lute`](docs/examples/quest-grove.lute) — quest with objectives,
  lifecycle handlers, and declarative rewards on both completion and failure.
- [`docs/examples/arcia-project/date-minigame.lute`](docs/examples/arcia-project/date-minigame.lute) —
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
`profile` come from the [`arcia-project`](docs/examples/arcia-project) plugin project, not from
the core language):

```lute
---
kind: scene
id: marina.s01ep05
luteVersion: "0.36.1"
profile: date-minigame
extra:
  arc: main
---

## Shot 1.
::minigame{kind="rhythm" id="marina_service_01" resultKey="service01" sync="true"}

<match on="scene.minigame.service01.rank">
  <when is="gold">
    @marina{code="0030" emotion="delighted" variant="1"}: Wonderful! A perfect service!
  </when>
  <otherwise>
    @marina{code="0050" emotion="shy" variant="0"}: Shall we try once more? The rhythm takes practice.
  </otherwise>
</match>
```

A quest — conditions and rewards as data (`inParty`/`ownsItem` are project-declared relations;
`findHalsin` is a sibling quest in the same project):

```lute
<quest id="hunt" title="The Hunt" start="holds('inParty', ['shadowheart'])" fail="run.dawnBroke">
  <reward kind="XP" amount="300"/>
  <reward kind="SHARD" amount="1..5" when="run.bonusMet"/>
  <reward kind="SHARD" amount="2" outcome="failed"/>
  <objective id="track" done="count('ownsItem', ['tracks']) >= 3">
    <reward kind="GOLD" amount="10"/>
  </objective>
  <objective id="freeHalsin" quest="findHalsin"/>
  <on event="questComplete">
    @narrator: The First Druid drew a slow breath. "You have my thanks."
  </on>
</quest>
```

## Play a story

A scene or lore entry becomes a **beat** by naming the **occasion** it answers — an engine
moment such as a hub visit or talking to an NPC — with a `when` condition, a `priority`, and a
repetition policy (`once: run` / `user` / `false`). `lute play` walks a scripted sequence of
raised occasions through the whole project:

```yaml
# plays/tenth-run.play.yaml
state: { user.runs: 10 }
steps:
  - occasion: townVisit
    expect: { winner: town.welcome }
  - occasion: talk
    target: npc.achilles
  - engine:                # what the engine owns: state, facts, reserved relations
      state: { run.outcome: fell, user.runs: { add: 1 } }
  - newRun: true
  - occasion: inbox
    pick: megNote          # a `select: all` occasion: the beat the player takes
choose:
  gift: accept
expect:
  quests: { tenthRun: complete }
```

```sh
lute play my-project --script plays/tenth-run.play.yaml
lute play my-project --script plays/tenth-run.play.yaml --json
lute test my-project       # runs every play that carries an expect:, beside *.test.yaml
```

For each step it lists every candidate beat with its verdict (`once`, `after:`, `when`), presents
the winner — highest priority, then project order — through the same reference runner as
`lute run`, and advances every quest lifecycle, so later `when` conditions over `quest.*` and
`after: completed(…)` see real progress. An `engine:` step writes what the engine owns, so the
harness needs no fake-engine content; a script may start from a save (`visited:`, `presented:`,
`quests:`, `entriesRead:`), and `expect:` turns a play into a test. `run.*` state, `once: run`
spending, and `<quest tier="run">` quests reset at each `newRun`, and `prev.run.*` keeps the
values the last run ended with. To see the whole schedule at once, `lute beats` prints each
occasion's ladder and `lute calendar` shows the winner in every cell of a grid of state values
(a day × slot week, say) from the same save. See
[Playing a story](https://lute-lang.vercel.app/tooling/play/) for the script format, exit
codes, and transcript shapes, and
[`docs/runtime/beats-and-occasions.md`](docs/runtime/beats-and-occasions.md) for the engine
contract. `lute init --template beats` scaffolds a project where all of this passes as
generated.

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

- **Language: implemented at 0.36.1.** The normative surface is the versioned spec stack — the [`0.1.0`](docs/proposals/scenario-dsl/0.1.0.md) base plus every delta through [`0.36.1`](docs/proposals/scenario-dsl/0.36.0.md). This release adds the lossless syntax layer, canonical formatting, source/project revisions, task-scoped context, semantic diff, and atomic source patching; components and templates remain compile-time expansion with no separate engine id.
- **IR: 0.36.1.** The execution IR and project index retain the compiler-derived, sorted `requiredSemantics` envelope alongside the 0.35 source-edit contract. Engines gate the exact major.minor line and compare `requiredSemantics` with the immutable `lute.engine.yaml` matrix.
- **Implementation: shipped.** The checker, compiler, provider/plugin resolver, reference runtime, LSP, and CLI are implemented, tested Rust crates under [`crates/`](crates), with editor clients under [`editors/`](editors) and npm distribution under [`packages/`](packages). Run `lute version` to print all three axes.

The toolchain is MIT-licensed ([`LICENSE`](LICENSE)); releases are tracked in
[`CHANGELOG.md`](CHANGELOG.md).
