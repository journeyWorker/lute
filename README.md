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
lute init my-project           # scaffold project + starter scene + mock (--template beats: occasions, beats, plays, tests)
lute new quest the-hunt        # scaffold one document (scene | quest | lore | schema; scene --on <occasion> writes a beat)
lute check scene.lute          # static validation of one document
lute check-project .           # every document + project-wide passes (quest ids, connectivity, fact guards)
lute lint .                    # advisory L-* findings governed by lute.lint.yaml
lute trace scene.lute --mock m.yaml   # preview SOURCE behaviour, every decision explained
lute compile scene.lute -o out.json   # JSON artifact (--all for a whole project + index)
lute run out.json --mock m.yaml       # reference runtime over the ARTIFACT an engine consumes
lute lore my-project           # world-narrative map: lore entries by target/series, facts revealed
lute play my-project --script p.play.yaml   # raise occasions + engine writes, see every beat's verdict + the winner
lute test .                    # *.test.yaml scenario tests + every *.play.yaml carrying expect:
lute beats my-project          # each occasion's beat ladder: priority, once, when, check-project verdicts
lute calendar my-project --axis run.day=1..7 --axis run.slot=morning,night   # who answers in every cell
lute loc export . --format csv # localization round trip (+ import, word-count report)
lute scenario .                # read-only graph / reachability / envelope reporting (+ `knowledge`: fact producers)
lute tag scenes/               # back-fill stable line codes (localization identity; a file or a directory)
lute context scene.lute        # the project-resolved authoring surface (for AI authoring)
lute doctor                    # diagnose local toolchain + project setup
lute version                   # the three independent version axes
```

## Documents by role

Each document owns one role; read the one that matches what you are doing.

| If you are… | Normative spec (source of truth) | Overview / rationale |
|---|---|---|
| **writing `.lute` scenarios** | the versioned spec stack: base [`0.1.0`](docs/proposals/scenario-dsl/0.1.0.md) plus per-release deltas through the current tip [`0.26.0`](docs/proposals/scenario-dsl/0.26.0.md) (projects at scale and many authors: state declared in several documents, kinds assembled with `add:`, `defaults.uses` globs, component `@@who` / defaults / own results, directive `when=`, kind-wide beat targets, rule aggregates, test/play parity). [`docs/versioning.md`](docs/versioning.md) lists every release and what each axis earned. | the examples below; [`architecture.md`](docs/architecture.md) |
| **writing scenarios, fast** (one page to keep open: every construct, CLI, diagnostics, gotchas) | the spec stack above | the website's [Cheatsheet](https://lute-lang.vercel.app/reference/cheatsheet/) — every snippet on it is compile-checked in CI |
| **authoring quests** (lifecycle, objectives, subquests, rewards) | [`0.2.0`](docs/proposals/scenario-dsl/0.2.0.md) §6 (quest kind, objectives, lifecycle events) + [`0.14.0`](docs/proposals/scenario-dsl/0.14.0.md) (subquests) + [`0.16.0`](docs/proposals/scenario-dsl/0.16.0.md) (`<reward/>`) + [`0.24.0`](docs/proposals/scenario-dsl/0.24.0.md) §2 (`activate="accept"`, `complete="any"`, `by=` / `until=` deadlines, `failedBy`) + [`0.25.0`](docs/proposals/scenario-dsl/0.25.0.md) §§4–5 (subquest and `start` graph anchors, `accept="external"`) | [`runtime/quest-lifecycle.md`](docs/runtime/quest-lifecycle.md) |
| **authoring lore** (item descriptions, found notes, codex pages, barks) | [`0.19.0`](docs/proposals/scenario-dsl/0.19.0.md) (`kind: lore`, `<entry>`, `entry.<id>.read`, document bundles) + [`0.23.0`](docs/proposals/scenario-dsl/0.23.0.md) §4 (scene-like `<beat>` bundles) | [`runtime/lore-entries.md`](docs/runtime/lore-entries.md) |
| **writing a plugin** (directives, state, providers, bridge, `stampAttrs`, `rewardKinds`, `occasions`) | [`proposals/plugin-system/0.0.1.md`](docs/proposals/plugin-system/0.0.1.md) — manifest YAML schemas + resolution — plus the [`0.0.2`](docs/proposals/plugin-system/0.0.2.md)–[`0.0.7`](docs/proposals/plugin-system/0.0.7.md) deltas; the `occasions:` export is [`0.21.0`](docs/proposals/scenario-dsl/0.21.0.md) §2 | [`plugin-system.md`](docs/plugin-system.md) |
| **building an engine** (consuming the artifact) | [`docs/runtime/`](docs/runtime) — execution model, quest lifecycle, state lifecycle, timeline semantics, CEL & facts, bridge protocol, lore entries, beats and occasions — plus the artifact JSON Schema [`schemas/lute-ir-0.26.schema.json`](schemas/lute-ir-0.26.schema.json) and the [`conformance/`](conformance) fixtures | [`architecture.md`](docs/architecture.md) |
| **building the compiler / checker / LSP** | the proposals above | [`architecture.md`](docs/architecture.md) — two-tier AST, auto-injection, the `check()` core, LSP |
| **reasoning about run / user / app state** | [`0.1.0`](docs/proposals/scenario-dsl/0.1.0.md) §9 (scalar tiers) + [`0.3.0`](docs/proposals/scenario-dsl/0.3.0.md) (relational facts + Datalog) + [`0.20.0`](docs/proposals/scenario-dsl/0.20.0.md) (what `check-project` proves about fact guards) | [`state-model-design.md`](docs/proposals/scenario-dsl/state-model-design.md) |
| **authoring characters** (label / costume / `???` reveal / voice) | [`proposals/character-cast/0.0.1.md`](docs/proposals/character-cast/0.0.1.md) — cast contract | [`character-cast/design.md`](docs/proposals/character-cast/design.md) |
| **choosing story beats / running `lute play`** | [`0.21.0`](docs/proposals/scenario-dsl/0.21.0.md) — occasions, scene and entry beats (`on` / `target` / `when` / `priority` / `once`), selection order, and `lute play`'s script and transcript — plus [`0.22.0`](docs/proposals/scenario-dsl/0.22.0.md): `engine:` steps, save seeds, `expect:` assertions, run boundaries, occasion target domains — [`0.23.0`](docs/proposals/scenario-dsl/0.23.0.md): `lute beats` / `lute calendar` overviews, `select: sequence`, `also`, bundle beats — [`0.24.0`](docs/proposals/scenario-dsl/0.24.0.md): a declared clock, `once: day` / `once: slot`, `advance:` / `include:` steps, `judge: before`, `bridges:` answers — and [`0.25.0`](docs/proposals/scenario-dsl/0.25.0.md): `share` keys spent together, bundle `<beat after=…>` edges, bridge answers that omit unread fields — and [`0.26.0`](docs/proposals/scenario-dsl/0.26.0.md) §§5, 7: `target="kind:<kind>"` beats, `::next` followed by trace/test, `eligible:` for an ineligible beat, `advance: { to: … }` | [`runtime/beats-and-occasions.md`](docs/runtime/beats-and-occasions.md); the website's [Playing a story](https://lute-lang.vercel.app/tooling/play/) |
| **configuring lints** | [`specs/2026-08-26-lute-lint-system-design.md`](docs/superpowers/specs/2026-08-26-lute-lint-system-design.md) — `lute.lint.yaml`, rule levels, project-local `custom:` rules | [`docs/linting.md`](docs/linting.md) |

Worked examples:

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
luteVersion: "0.26.0"
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

## Play a story

A scene or lore entry becomes a **beat** by naming the **occasion** it answers — an engine
moment such as a hub visit or talking to an NPC — with a `when` condition, a `priority`, and a
repetition policy (`once: run` / `user` / `false`). `lute play` walks a scripted sequence of
raised occasions through the whole project:

```yaml
# plays/tenth-run.play.yaml
state: { user.runs: 10 }
steps:
  - occasion: hubVisit
    expect: { winner: hub.welcome }
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

- **Language: draft, at 0.26.0.** The normative surface is the versioned spec stack — the
  [`0.1.0`](docs/proposals/scenario-dsl/0.1.0.md) base plus every delta up to
  [`0.26.0`](docs/proposals/scenario-dsl/0.26.0.md). Recent tips: `0.23.0` author overviews
  and time (`lute beats`, `lute calendar`, `lute scenario knowledge`; `by=` deadlines;
  `select: sequence`; lore `<beat>` bundles; a sharper condition decider), `0.23.1` a patch
  on which `lute trace`, `lute test` and `lute play` agree, `0.24.0` clocks, quest structure
  and parties (a declared `clock:`, `once: day` / `once: slot`, `activate="accept"`,
  `complete="any"`, `until=`, `failedBy`, entity sub-kinds and `per:` state, cast
  `present:`, `effects: true` components, `bridges:` answers), `0.25.0` exclusion, shared
  spends and graph edges (relation `excludes:`, `share` keys, bundle `<beat after=…>`,
  `[subquest]` / `[start]` anchors, `accept="external"`, `changedOn:`,
  `{{x:ordinalWord}}`), `0.25.1` a performance patch, and `0.26.0` scale and many authors:
  the fact closure is computed once per project and `lute test` loads a project once and
  runs in parallel; state declared in several documents must agree
  (`E-STATE-DECL-CONFLICT`), a duplicate member is an error, a schema may `add:` members to
  a kind another declares, and `defaults.uses` takes globs; directive attributes may be
  typed by an entity kind and a reward kind's `target:` is checked (`E-REWARD-TARGET`);
  components gain `@@who:`, param `default:`s and their own result slots; directives take
  `when=`; a beat may `target="kind:<kind>"` and read `occasion.target`; a rule body may
  `count(…)` a lower stratum (`E-RULE-AGGREGATE-CYCLE`); `lute trace` / `lute test` follow
  `::next`, apply a handler `::accept` and fail a walk of an ineligible beat unless it
  asserts `eligible:`; and `W-DISPLAY-NAME-DUP` / `W-ENTRY-WRITE-REREAD` join the advisories.
  Being draft means the grammar may still break before 1.0; each breaking change ships a
  `lute fix` migration or a pin where the rewrite is mechanical.
- **IR: 0.26.0.** The compiled artifact is specified by
  [`schemas/lute-ir-0.26.schema.json`](schemas/lute-ir-0.26.schema.json) and the
  [`docs/runtime/`](docs/runtime) contract, with executable
  [`conformance/`](conformance) fixtures. Engines gate on `irVersion` by **MAJOR** only
  (since `0.13.0`): fields are append-only within a major line, so a minor move costs a
  consumer nothing. `0.26.0` is additive: an optional `targetKind` on beats, entries,
  bundle beats and index beat rows, the placeholder kind `occasionTarget`
  (`{{occasion.target}}`, carrying its `entityKind`), and the rule-body literal `count`; a
  directive's `when=` lowers to a one-arm match and a param `default:` to its value, so
  neither adds a field.
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
