---
title: CLI reference
description: Every lute subcommand — init, new, check, check-project, compile, compile-stream, run, play, trace, test, lint, scenario, loc, context, tag, fix, lore, doctor, catalog refresh, version — with its synopsis, key flags, and exit-code contract.
---

`lute` is the headless checker and compiler for `.lute` documents. The core `check()` is the contract; the CLI adds argument parsing, file I/O, and output formatting, and owns no validation logic. Two resolution flags recur: `--providers <DIR>` pins a directory of provider snapshots to resolve ids against, and `--project <DIR>` loads a `lute.project.yaml` + `plugins/` to resolve the document's activated capability snapshot. Without it, `check` applies the nearest `lute.project.yaml` above the file; the other single-file commands (`compile`, `trace`, `context`, `test`) resolve core-only (`lute.core`). On the permission-aware authoring commands below, `--permission-profile <NAME>` requires project resolution and applies that trusted profile's [permissions](/tooling/capability-permissions/) as an additional ceiling without activating its plugins or changing the source profile.

## check

```console
$ lute check <file> [--json] [--providers <DIR>] [--project <DIR>]
              [--permission-profile <NAME>] [--deny <CODE>]… [--deny-warnings]
```

Statically validate one `.lute` document. Without `--project`, the nearest `lute.project.yaml` above the file supplies the project (its `defaults:`, profiles, and plugins), and stderr says so: `lute: note: using project <dir> (nearest lute.project.yaml); pass --project to choose another`. With no manifest above the file, the check is core-only. Exit **0** clean, **1** when any `Error`-severity diagnostic is present, **2** on an I/O failure. `--permission-profile <NAME>` applies a trusted additional ceiling; denied authored effects are non-suppressible `E-PERMISSION-*` errors, and a missing project/profile or invalid policy is an explicit resolver error rather than an unrestricted fallback. `--json` prints the serialized `CheckResult`; otherwise a human line per diagnostic. `--deny <CODE>` (repeatable, rustc/clippy `-D` precedent, 0.6.1 §5) promotes every diagnostic with exactly that code to an error for the verdict and exit code, and `--deny-warnings` promotes every warning — a pipeline denies `W-FACT-GUARANTEED` (under `check-project`) to reject redundant relational guards, `W-LUTE-VERSION-STALE` to reject a stale `luteVersion` stamp. A promoted diagnostic reports severity `error` with a `"denied": true` marker in `--json`; an unknown code in `--deny` — including a removed one such as `W-UNPROVEN-RELATIONAL` (0.20.0) — is a usage error (exit **2**), and errors are never demotable.

## check-project

```console
$ lute check-project <dir> [--json] [--providers <DIR>] [--deny <CODE>]… [--deny-warnings]
```

Recursively `check` every `*.lute` file under `<dir>` in deterministic sorted order, each against its own nearest-ancestor `lute.project.yaml` root, **plus** project-wide `<quest id>` uniqueness, the connectivity passes (`E-CONN-*`, `W-QUEST-REF-UNKNOWN`, `E-STATE-MAYBE-UNAVAILABLE`), and the [relational guard analysis](/state/facts-and-datalog/#how-check-project-analyzes-relational-guards) (dsl 0.20.0): every `holds(…)`/`count(…)` in a guard is decided impossible, guaranteed, or possible over the whole project, so a guard that can never hold is reported through its slot's dead-code error (`E-ARM-DEAD`, `E-ENTRY-UNREACHABLE`, `E-OBJECTIVE-UNSATISFIABLE`, `E-QUEST-UNREACHABLE`) and a redundant one as `W-FACT-GUARANTEED`. The project-wide beat and quest advisories run here too (dsl 0.22.0): `W-BEAT-PRIORITY-TIE` (beats on one `select: first` occasion, for the same or no target, with equal priority and `when`s not provably exclusive, so file order picks the winner), `W-BEAT-ONCE-RUN-USER` (a `once: run` beat whose `when` reads only user-tier state, so it replays every run), and `W-QUEST-HANDLER-DEAD` (an `<on event="questFailed">` on a quest that has no `fail`, no required subquest objective, and no parent quest, so it can never fail). It then compiles every document that checked clean under its project's `identity:`, so compile-stage errors fail here rather than only at `compile`: `E-DUP-VOICEKEY` (lines with different text on one `voiceKey` — since 0.22.0 the default `{prefix}.{speaker}-{code}` keeps scenes apart, so this is what a project pinning the older unprefixed `identity.voiceKey: "{speaker}-{code}"` risks) and `E-CAPABILITY-MISMATCH` (documents resolving two capability snapshots, which `compile --all` and `play` refuse). Exit **0** clean, **1** when any file has an error or a project-wide collision, **2** on I/O. The same `--deny <CODE>`/`--deny-warnings` promotion (see `check`) applies project-wide.

## compile

```console
$ lute compile <file> [--json] [--providers <DIR>] [--project <DIR>] [-o <FILE>]
                      [--permission-profile <NAME>] [--locales <FILE>]
                      [--deny <CODE>]… [--deny-warnings]
$ lute compile --all --project <DIR> -o <DIR> [--providers <DIR>] [--locales <FILE>]
                      [--permission-profile <NAME>] [--json]
                      [--deny <CODE>]… [--deny-warnings]
```

Compile a document to its JSON command-record artifact (gated on a clean check and a compiler-side recheck of the effective permission policy). Exit **0** on success, **1** on a failed gate, **2** on I/O or serialization failure. The artifact is always JSON; `-o`/`--out` writes it to a file instead of stdout. With `--project`, the gate is the target's reconciled `check-project` verdict. A `--permission-profile` ceiling is checked again immediately before lowering, so a check result created under another policy cannot authorize forbidden IR.


### `--all` — project-wide compile and index

`--all` compiles **every** `*.lute` document under `--project <DIR>` into `-o <DIR>`, mirroring the project's own layout (`quests/a.lute` → `<outdir>/quests/a.lute.json`), and writes a `<outdir>/project.index.json`. Under `--all`, `-o` is an output **directory**, not a file; it is created if absent. `*.component.lute` fragments are skipped — a component is inlined into its importers and has no artifact of its own. When `--permission-profile` is present, its ceiling applies independently to every source-selected profile; it never activates the named profile's plugins.

`--all` requires **both** `--project` and `-o` and takes no `<file>`. Each of those three is checked independently and every violation is reported, then the command exits **2** without reading a document:

```console
$ lute compile --all
error: --all requires --project <DIR> (the document set and capability snapshot both resolve per project)
error: --all requires -o <DIR>, an output DIRECTORY (there is no single artifact to write to stdout)

Usage: lute compile --all --project <DIR> -o <DIR>
```

The index carries the document table plus the **union** of every artifact's `entities`, `enums`, `relations`, `seedFacts`, `rules`, and `prereqEdges` — the union [an engine must compute anyway](/tooling/runtime-contract/) before it can evaluate anything:

```json
{
  "irVersion": "0.10.0",
  "capabilityVersion": "…",
  "documents": [
    { "path": "quests/findKai.lute", "artifact": "quests/findKai.lute.json",
      "kind": "quest", "key": "findkai" },
    { "path": "scenes/opening.lute", "artifact": "scenes/opening.lute.json",
      "kind": "scene", "key": "narrator.s01ep01" }
  ],
  "entities": [], "enums": [], "relations": [], "seedFacts": [], "rules": [], "prereqEdges": []
}
```

`path` is the source, relative to the project root; `artifact` is its compiled output, relative to the output directory; `key` is the document's canonical node id — a scene's `{character}.{episodeId}`, or a quest document's **first** declared `<quest id>` (a quest pack's remaining ids stay recoverable from its own artifact's `quest` records). All paths are forward-slash relative, never absolute, so an index survives being copied between machines or packed into a game archive.

`documents` is sorted by `path` and every vocabulary array is deduplicated and totally ordered, so the index is byte-stable across runs. Unlike an artifact, which omits an empty vocabulary array, the index always emits all six — an engine unions them unconditionally, and an absent key would force it to distinguish "no relations" from "index too old to carry them".

### `--all` is all-or-nothing

Every document is compiled in memory and the index is built before anything touches the filesystem. Three things stop the write, each exiting **1** with nothing emitted:

- **A failed ordinary or permission gate.** One document's diagnostics print, followed by `N of M document(s) failed; no output written`. A host permission denial never leaves a denied artifact or partial index behind.
- **A `--deny`-promoted warning** (`--deny W-L10N-MISSING`, `--deny-warnings`): `--deny promoted N diagnostic(s); no output written`.
- **A vocabulary conflict.** Two documents declaring the same entity kind / enum / relation / prerequisite node with **different** signatures, or resolving different capability snapshots — never a silent pick. `check-project` reports the snapshot case as `E-CAPABILITY-MISMATCH`. A signature conflict is the one class it cannot see, because it validates each document against its own resolved vocabulary and never unions across independent documents:

```console
$ lute check-project .
ok: . (2 file(s), 0 project-wide warning(s))
$ lute compile --all --project . -o out
lute compile --all: relation `knows` is declared with conflicting signatures by two documents (`scenes/a.lute` and `scenes/b.lute`)
lute compile --all: 1 vocabulary conflict(s); no output written
```

An `E-`-severity capability-resolution diagnostic (a bad plugin option, identity template, permission shape, or host permission-profile lookup) also exits **1** — see [the AI harness guide](/tooling/ai-harness/#capability-resolution-errors-gate-the-exit-code).

`--all` writes, but never prunes: an artifact whose source document was deleted stays in the output directory. Build into a directory you own and clear.

### `--locales` — merge a translation bundle

`--locales <bundle.json>` merges a locale bundle (see [`loc import`](#loc-import)) into the artifact: `texts` on every line record and `labels` on every choice/hub option, both keyed by `lineId`. The source-language `text`/`label` is never overwritten, and both maps are omitted when empty — so a document compiled without `--locales` is byte-identical to before. A bundle entry matching nothing in this document is ignored; a bundle legitimately spans a whole project. It composes with `--all`, merging the one bundle into every artifact.

A translatable record missing a locale the bundle declares is `W-L10N-MISSING`, one per `(lineId, locale)` pair, written to stderr. It is a warning: the artifact still emits, carrying the source-language string. `--deny W-L10N-MISSING` (or `--deny-warnings`) promotes it, so CI can require a complete translation before anything ships:

<!-- lute-diagnostics -->
```console
$ lute compile scenes/opening.lute --project . --locales bundle.json --deny W-L10N-MISSING -o out.json
scenes/opening.lute:1:1: error [W-L10N-MISSING] [denied] no `ja-JP` text for `narrator.s01ep01.narrator_0020`
--deny promoted 1 diagnostic(s); no artifact emitted
```

## compile-stream

```console
$ lute compile-stream <scene.lute> [--project <DIR>] [--providers <DIR>]
                      [--permission-profile <NAME>]
```

Resolve and check a complete scene template once, then read append-only ordinary
Lute shot-body text from stdin. Each complete accepted line/directive/block is
compiled through the existing cumulative whole-document pipeline and flushed to
stdout as NDJSON: `start` with the initial full artifact, one `update` with a
full artifact per unit, then `finish` on successful EOF. A rejection writes an
`error` record with diagnostics and no `finish`.

`--permission-profile <NAME>` freezes one trusted additional ceiling with the
project snapshot. It applies to both the fixed template and every appended body
unit; there is no mid-stream switch. A forbidden unit terminates with its
`E-PERMISSION-*` diagnostic before any update containing the denied IR.

Exit **0** only after successful EOF finalization, **1** on a
syntax/semantic/permission/compile/streaming rejection, and **2** on usage, I/O,
broken stdout, or invalid UTF-8. Authored `::end` is an ordinary runtime command;
it does not replace EOF or close stdin. The command never modifies the template
or performs remote/runtime effects. See the
[streaming continuation compiler guide](/tooling/continuation-compiler/) for
the record schema, cumulative cost, admitted body surface, permission freezing,
and consumer cursor/state rules.

## trace

```console
$ lute trace <file> [--state P=L]… [--fact "R(A…)"]… [--choose ID=C[,C]]…
              [--event N]… [--accept Q]… [--occasion O]… [--mock <FILE>] [--json]
              [--providers <DIR>] [--project <DIR>] [--entry <ID>] [--no-derive]
```

Preview a document against author-supplied mocks (see the [tracing guide](/tooling/tracing/)). Exit **0** complete, **1** refused (check errors or invalid mocks — the `E-TRACE-*` codes render like check diagnostics), **2** I/O, **3** incomplete (an `unknown` guard halted the walk). `--occasion <O>` (repeatable, dsl 0.21.0) raises an occasion after a quest walk settles, judging the `<objective on="O">` objectives of every active quest; the mock file's `occasions:` list does the same, and its `visited:` list seeds the scenes `visited('<id>')` reads as presented (unlisted scenes are not visited).

A mock can also start from a save (dsl 0.22.0): `quests: { <id>: unset | active | complete | failed }` seeds `quest.<id>.state`, and `entriesRead: { run: [ids], user: [ids] }` seeds `entry.<id>.read` and `entry.<id>.everRead`. Each follows the mock rules of the reserved path it spells — the document must read that path.

**Derivation is on by default** (dsl 0.22.0). Trace loads the project's seed `facts:` and applies its Datalog rules (stratified negation) over the mocked and asserted facts, so a rule-derived fact satisfies a guard, `done`, or `start` without mocking the conclusion, and a derived fact that is false because a negated premise holds can be traced. Mocking a derived atom still works — it is a seed like any other. `--no-derive`, or `derive: false` in the mock (the flag wins), restores the 0.21 model: seeds are not loaded, an unmocked derived atom is unknown, and a note names each derived relation read. Trace, `test`, `run`, and `play` share one Datalog evaluator, so they cannot disagree about what a project's rules conclude.

`--entry <ID>` presents **one** `<entry>` of a [lore document](/language/lore-entries/) (dsl 0.19.0) instead of walking a sequence: its lines, the `<match>` arm taken, and the `::set` / `::assert` / `::retract` a first read applies — or skips, when the mock seeds `entry.<id>.read: true`. A lore document has no sequence to walk, so tracing one without `--entry` is a usage error (exit **2**, naming the declared entry ids); `--entry` on a scene or quest, or naming an id the document does not declare, is `E-TRACE-ENTRY` (exit **1**).

## scenario

```console
$ lute scenario <dir> [--providers <DIR>] [--format text|json|dot]
              [reach <nodeId> | envelope <nodeId>]
```

Read-only reporting over the connectivity layer. With no subcommand, prints the assembled node/edge graph. `reach <nodeId>` reports a node's [reachability verdict](/connectivity/reachability/); `envelope <nodeId>` (or `envelope quest:<id>`) prints the [Guaranteed/Possible tables](/connectivity/envelopes/) plus the [guaranteed facts](/connectivity/envelopes/#guaranteed-facts) at the node's entry (dsl 0.20.0; `envelope.guaranteedFacts` in `--format json`, each `{fact, establishedBy}`). `<nodeId>` is a scene's canonical key or `quest:<id>`. Exit **0** on success, **2** on I/O or an unresolvable node id.

`--format` selects the output shape of the bare graph view:

- `text` (default) — the topological layers, then one line per edge with the [atom kind(s)](/connectivity/scene-graph/#edge-kinds) that justify it in brackets, then — when any exist — the quests with no `after=` under ``unanchored (no `after` — available from the start of play; no prerequisites in this graph):``, one `quest(<id>)` per line.
- `json` — `{"roots":[{"root":…,"layers":[[…]],"nodes":[…],"edges":[…]}]}`. Each node is `{id, kind, prereq, reach}` (`prereq` is the raw declared formula, `null` for an entry node); each edge is `{from, to, kinds}`, where `kinds` is an array because one formula may reference the same node under more than one atom. A root with unanchored quests also carries `"unanchored": ["quest(<id>)", …]` (omitted when there are none).
- `dot` — one Graphviz `digraph` per root; scenes are boxes, quests ellipses, and an `active`-only edge is drawn `[style=dashed]`. An unanchored quest is a dashed blue ellipse labelled `quest(<id>) (unanchored)`.

An unanchored quest (dsl 0.21.0 §7a.5) sits in no layer and on no edge, but it is not missing from the report: `reach quest:<id>` gives it the verdict ``Unanchored — a quest with no declared `after` prerequisite: available from the start of play; …`` (JSON `"reach": "unanchored"`), and its `after:` line reads `(none declared) — unanchored: this quest is in no prerequisite graph layer and on no edge; it is available from the start of play.`

## context

```console
$ lute context <file> [--json] [--providers <DIR>] [--project <DIR>]
                      [--permission-profile <NAME>]
```

Emit the project-resolved **authoring surface** an AI or human needs to write valid Lute against this file's project — directives, attrs, enums, asset kinds, providers, state schema, relational vocabulary, delivery flags, referenced reserved quest paths, effective permission layers, and `capabilityVersion`. A capability query, not validation — it emits regardless of document diagnostics. With `--permission-profile`, JSON `permissions` is `{ "layers": [...] }`, `bridges` contains only allowed bridge capability objects, `rewardKinds` is the allowed name-keyed object (empty when rewards are denied), and `questsAllowed` is a boolean. `directives` excludes both directive-denied entries and bridge directives whose `service/operation` is denied. External read-only state remains visible. Text output describes a compile-time authoring restriction and explicitly does not claim runtime sandboxing. Exit **0** on success, **2** on I/O; project/profile resolution errors are surfaced rather than treated as unrestricted.

Since 0.22.0 the surface also carries `defs` (each named condition's `name`, `type`, `params`, and `body`), the language's built-in directives under `builtinDirectives` (`::set`, `::assert`, `::retract`, `::accept`, `::use`, each with its `syntax` and `meaning`), and `ids` — every scene, quest, and lore entry id in the `--project` (`{ scenes, quests, entries }`; without `--project`, the document's own). A relation reports its `tier` and whether it is `reserved`, a state path declared `owner: engine` says so, an occasion carries its `description` and its `target` (`false`, `true`, or a `{ prefix, entity }` domain — human: `talk (select: first, target: npc.<npc>)`), and the human outline prints each imported component's parameters with their types. See the [AI harness guide](/tooling/ai-harness/#prompt-context-lute-context---json).

## tag

```console
$ lute tag <path> [--force]
```

Back-fill a stable `code` into every untagged `:line`, rewriting the file in place; a document already fully tagged is left byte-identical. `--force` renumbers every line's `code` in clean document order instead (`0010`/`0020`/… per speaker per scope) — a drafting tool, refused for a document whose frontmatter declares `codesLocked:` (published codes are `lineId`/`voiceKey` identity) or that has structural errors. Exit **0** on success, **1** when a document was refused, **2** on I/O.

`<path>` may be a directory (dsl 0.22.0): every `.lute` file under it is rewritten, recursively, in the same sorted order `check-project` walks. Each changed file gets a line naming it, a file with nothing to do stays silent, and a summary closes the run; a refused or unreadable file is reported and the walk goes on, and the exit code is the worst outcome across the files:

```console
$ lute tag scenes
lute: scenes/hub/day-end.lute: tagged 1 line(s)
lute: scenes/hub/morning.lute: tagged 1 line(s)
lute: scenes/hub/welcome.lute: tagged 1 line(s)
lute: scenes/talk/mara-first.lute: tagged 3 line(s)
lute: scenes/talk/mara-idle.lute: tagged 2 line(s)
lute: tagged 8 line(s) in 5 of 5 file(s)
```

## fix

```console
$ lute fix <path>
```

Apply the mechanical, meaning-preserving migrations in place — `:line[speaker]{…}: text` → `@speaker{…}: text`, leading `:` sigil → `@`, choice `as="…"` → `into="…"`, and a literal-comparison `<when test="$ == 'gold'">` → `<when is="gold">` (`W-WHEN-TEST-LITERAL`, dsl 0.18.0). Byte-exact and comment-preserving; writes back only when something changed. Like [`tag`](#tag), `<path>` may be a directory: every `.lute` file under it, recursively and in sorted order, each changed file on its own line, then a summary (`lute: applied N fix(es) in M of K file(s)`). Exit **0** on success, **2** on I/O — for a directory, the worst outcome across its files.

## catalog refresh

```console
$ lute catalog refresh <dir> [--project <DIR>]
```

Re-stamp every pinned provider snapshot in `<dir>` against the current `capabilityVersion` and clear its `stale` flag (see [providers & catalog](/tooling/providers-and-catalog/)). Exit **0** on success, **2** on I/O.

## init

```console
$ lute init <dir> [--template minimal|investigation|beats]
```

Scaffold a new Lute project directory, ready for `lute check-project`. `<dir>` must not already contain a `lute.project.yaml`. `--template` selects the starter content:

- `minimal` (default) — a core-only `lute.project.yaml`, a state schema, a vocabulary schema, a starter scene, and a trace mock.
- `investigation` — the worked whodunit.
- `beats` (dsl 0.22.0) — a game driven by [beats and occasions](/tooling/play/): a project-local occasions plugin (one occasion with a `{ prefix, entity }` target domain), a manifest whose `defaults:` supply `luteVersion` and `uses`, a `world.schema.yaml` with an `owner: engine` clock and shorthand `defs`, scene beats with `id:`, a quest, lore entry beats, a play script with an `engine:` step and `expect:`s, and scenario tests. `check-project`, `test`, and `play` all pass as scaffolded.

None of the templates pins `identity:` — the default `voiceKey` already carries the scene prefix. The command prints every file it created and the next commands to run. Exit **0** on success, **2** on I/O, an unknown template, or a refused overwrite.

## lore

```console
$ lute lore <dir> [--json]
```

Print the project's **world-narrative map** (dsl 0.19.0): every [lore entry](/language/lore-entries/) under `<dir>` grouped by `target` and by `series` (in `order`), then, for every relation asserted anywhere in the project, each ground fact and whether lore entries, scenes/quests, or both reveal it — with the entries and documents that do. `--json` emits the same report as an object with `targets`, `series`, and `relations` arrays. Read-only; documents need not check clean. Exit **0** on success, **2** on I/O.

## new

```console
$ lute new <scene|quest|lore|schema> <name> [--dir <DIR>]
$ lute new scene <name> --on <occasion> [--target <target>] [--dir <DIR>]
```

Scaffold one new document into an existing project. The first argument is the document kind (`scene`, `quest`, `lore`, or `schema`); `<name>` is the file stem, and a `/` in it nests the file in a subfolder (`lute new scene talk/tomas-evening` writes `scenes/talk/tomas-evening.lute`). Every document gets an `id:` — a scene's is its name (dotted by folder: `talk.tomasEvening`), `lute new quest` writes `quests/<name>.lute` with `id: quest.<ident>`, and `lute new lore` writes `lore/<name>.lute` with `id: lore.<ident>` and one `<entry>` attached to `item.<ident>` — and omits whatever the manifest's `defaults:` already supplies (such as `luteVersion` or `uses`). `--dir` is where to start looking for the project (default: the current directory); the document lands under the enclosing project's root, the directory holding its `lute.project.yaml`. Outside any project, `lute new` says so on stderr and writes a self-contained document.

`--on <occasion>` (dsl 0.22.0) makes the scene a [beat](/language/beats/) answering that occasion, and `--target <target>` names what a targeted occasion is raised for, a `<prefix>.<member>` of its target domain. Both are checked against the project before anything is written — an occasion the project's plugins do not declare, or a target outside the occasion's domain, is exit **2** with a did-you-mean and no file:

```console
$ lute new scene talk/tomas-evening --on talk --target npc.tomass
lute new: target `npc.tomass` is outside occasion `talk`'s domain `npc.<npc>` (`npc.mara`, `npc.tomas`) — did you mean `npc.tomas`? (dsl 0.22.0 §8); nothing was written
```

Outside a project `--on` is refused, since no occasion is declared. Exit **0** on success, **2** on I/O, an invalid kind, or a refused `--on`/`--target`.

## doctor

```console
$ lute doctor [<dir>] [--json]
```

Diagnose the local toolchain and project setup: the version axes, the project manifest, the content documents, play scripts (`*.play.yaml`) and scenario tests (`*.test.yaml`), provider snapshots, the active plugins, every declared occasion with the number of beats answering it, the declared vocabulary slots, and editor integration. `<dir>` is the project directory to inspect (default: the current directory). Provider snapshots are looked for in the manifest's `catalogDir:` (default `catalog/`), the directory `check` reads; with none there the line says `no pinned provider snapshots`. The editor check runs `lute-lsp --version` on the first `lute-lsp` on `PATH` and flags one that reports another version — or none, as a server older than 0.22.0 does — with how to reinstall it.

```console
$ lute doctor .
lute doctor — .
  • toolchain version: 0.22.0
  • language version: 0.22.0
  • IR schema version: 0.22.0
  ✓ lute.project.yaml: found at ./lute.project.yaml
  ✓ content documents: 7 `.lute` file(s) under .
  • play scripts: 1 `*.play.yaml`
  • scenario tests: 2 `*.test.yaml`
  • provider snapshots: no pinned provider snapshots
  • active plugins: game.occasions 0.1.0
  • occasions (beats answering): 3 declared, 7 beat(s) — dayEnd (1), hubVisit (2), talk (4)
  • vocabulary slots declared: emotion, action (exits: fade-out/hide), anchor (default: center), mood, volume, musicAction, vfxType
  • VS Code extension: not detectable from the CLI
  ✗ lute-lsp on PATH: /usr/local/bin/lute-lsp reports no version (older than 0.22.0) — differs from lute 0.22.0
      → reinstall the language server from this toolchain (`cargo install --path crates/lute-lsp`) and restart the editor
```

`--json` emits the same checks as one object, `{ "dir", "checks": { <key>: { label, ok, detail, hint } } }` (`ok` is `null` for an informational line). A report, never a gate: exit **0** whatever the checks find, **2** when `<dir>` cannot be read.

## run

```console
$ lute run <artifact> [--mock <FILE>] [--occasion <O>]… [--json] [--entry <ID>]
```

Execute a **compiled artifact** (`lute compile` output) headlessly against a mock playthrough — the reference consumer of the [runtime contract](/tooling/runtime-contract/): command dispatch, CEL guards, the facts + Datalog fixpoint, hubs, and quest lifecycle. Distinct from `lute trace`, which previews *source*; `run` consumes the artifact an engine would. `--mock` is a YAML playthrough (the same surfaces as `lute trace --mock`); `--json` emits the machine-readable transcript. Exit **0** on a complete run, **1** refused, **2** on I/O, **3** incomplete.

For a quest artifact, `--occasion <O>` (repeatable, dsl 0.21.0) raises an occasion after the walk settles — after the mock's own `occasions:`, in CLI order — judging the `<objective on="O">` objectives of every active quest; each raise is an `{"kind": "occasion", "occasion": "O"}` record (human: `  occasion O`). A quest with no `start` is accept-driven here as in an engine: it stays `unset` until the mock's `accepts:` names it or an `accept` record runs. A scene's `::accept{quest="<id>"}` is an `{"kind": "accept", "quest": "<id>"}` record (human: `<address>  quest <id> accepted`); when the walk already knows the quest to be past `unset`, the record carries `"ignored": "already <state>"` and the line ends ` (already <state> — ignored)`. The mock's `visited:` list seeds the scenes `visited('<id>')` reads as presented.

`--entry <ID>` presents one `entry` record of a **lore artifact** (dsl 0.19.0; [engine contract](https://github.com/journeyWorker/lute/blob/main/docs/runtime/lore-entries.md)): the transcript reports whether it is a first read and whether its `when` holds, runs its body segment, applies first-read effects (or records them as skipped once `entry.<id>.read` is seeded `true`), and then sets `entry.<id>.read`. It is required for a lore artifact and refused on any other kind (both exit **2**).

## play

```console
$ lute play <PROJECT_DIR> --script <FILE> [--json] [--no-derive] [--explain <ATOM>]…
```

Play a story through a WHOLE project as a sequence of raised **occasions** (dsl 0.21.0) — the reference-runtime consumer of [beats and occasions](/tooling/play/). The project is compiled once, in memory, with the same gate and declaration union `compile --all` uses (scene, quest, and lore documents). The required `--script` is a `*.play.yaml` file with a closed key set. Its `steps:` each do one thing — raise an `occasion:` (with `target:`, `pick:`, and a step-local `choose:` that replaces the script's `choose:` key by key for that presentation), start a `newRun:`, write what the engine owns with `engine:` (`state:` literals or `{ add: <n> }`, `facts:`, `retract:`), or fire a world `event:` — and any step may carry `label:`, `repeat: <n>`, and `expect: { winner, offered, notOffered }`. Beside `steps:`, the script takes the `lute trace --mock` grammars for `state:`, `facts:`, and `choose:`, a save to start from (`visited:`, `presented: { run, user }`, `quests:`, `entriesRead: { run, user }`), `derive:`, and a top-level `expect: { exit, quests, state, facts, notFacts, transcriptContains, transcriptLacks }` judging the end of the play (dsl 0.22.0). Each occasion step lists the occasion's candidate beats with their verdicts, presents the winner (or the step's `pick` on a `select: all` occasion; `pick: none` presents nothing) through `lute run`'s reference evaluator, and advances every quest lifecycle, so later `when` conditions and `after: completed(…)` see real progress. `--json` emits the same transcript as one object.

`--no-derive` (or the script's `derive: false`; the flag wins) stops applying the project's Datalog rules, so an unmocked derived atom is unknown and halts the walk incomplete. `--explain <ATOM>` (repeatable) prints, after the play, the derivation tree of a ground atom — the rule used and each premise's own support (seed fact, asserted, or derived in turn), negated premises shown `(absent)` — or, when it does not hold, every rule that could conclude it with its failing premises; `--json` carries the same tree under `explain`.

Exit **0** complete (every step played, or a scene's `::end`) with every expectation met, **1** an error (the project fails to compile, a vocabulary conflict, a `pick` that is not eligible, or an `expect:` miss — each miss names the step, its `label:`, and the actual value), **2** a usage/I/O failure (a malformed script, an unknown occasion or `expect:` key, a step target outside its occasion's target domain, a save id the project does not declare, an `engine:` write that does not fit its declared type — or any `quest.*` write, since quest status is the lifecycle's and a save seeds it with top-level `quests:` — or an unreadable project), **3** incomplete (an unscripted choice or hub, or a `when`, quest objective, `now()`/`validAt()`, or plugin `bridgeResult` the reference runtime cannot decide). A `choose:` decision that is not offered — an ineligible choice, or a `once` hub option already taken (`E-TRACE-CHOICE`) — is exit **1**, like an ineligible `pick`. `lute test` runs every play script that carries an `expect:`. Script format, selection order, and transcript shapes: [Playing a story](/tooling/play/).

## test

```console
$ lute test [<dir>] [--json] [--providers <DIR>] [--project <DIR>] [--coverage] [--no-derive]
```

Run the project's scenario tests: every `*.test.yaml` under `<dir>` (default: the current directory) traces its scene or quest — or presents a lore document's entries — against the declared mocks and asserts the declared expectations, and every `*.play.yaml` under `<dir>` that carries an `expect:` (on a step or at the top level) is played exactly as [`lute play`](#play) plays it and judged by its own expectations (dsl 0.22.0). `--json` emits the machine-readable report; `--providers` pins a snapshot directory; `--project` resolves each traced document against the project (its `defaults:` and plugins) exactly as `lute trace --project` does — without it the trace is core-only. A play runs against `--project`, else the nearest `lute.project.yaml` above the script. `--no-derive` turns derivation off for every test and play, overriding their own `derive:` keys (see [trace](#trace)). Exit **0** when every test and play passes, **1** on a failure, **2** on I/O or a malformed test file.

Each test and play prints one `PASS`/`FAIL` line naming its file and what it ran, a failure followed by its misses, then a summary:

```console
$ lute test . --project .
PASS  ./tests/lamp-quest.test.yaml  (./tests/../quests/lamp.lute)
PASS  ./tests/mara-first.test.yaml  (./tests/../scenes/talk/mara-first.lute)
FAIL  ./plays/first-day.play.yaml  (play of .)
      step 6 at hubVisit: expect winner: expected hub.welcome, actual hub.morning

2 passed, 1 failed
```

A play passes when every step and top-level expectation holds and the play completed. **A play that halts fails** — an unscripted choice, or a `when` the reference runtime cannot decide — unless its top-level `expect:` declares the exit (`expect: { exit: incomplete }`); the failure names why it stopped. In `--json`, each `tests` entry carries `"kind": "test"` or `"kind": "play"`; a play's misses are in `misses`, each `{ step, label, repetition, occasion, key, expected, actual }` (`step` is `null` for a top-level expectation).

A failing test says why its walk stopped: the unresolved guards and the `state:`/`facts:` entries that would decide them (`--json`: `unresolved`, each with `atoms` and `supply`).

`--coverage` also reports branch/arm coverage across the tested documents and lists the **untested** documents — every testable document no `*.test.yaml` names and no play presents — under `--project`, or else under the nearest `lute.project.yaml` above `<dir>`, so `lute test tests --coverage` still measures the whole project. Every document a play presented counts as covered, and since a lore document is now testable, an untested one is listed too:

```console
coverage over 2 traced path(s) and 1 play(s):
  branch/hub maraAsk (./tests/../scenes/talk/mara-first.lute:maraAsk): 1/2 chosen [lamp]; never chosen [leave]
  1 untested document(s) under . — no *.test.yaml names them and no play presents them:
    ./scenes/talk/mara-idle.lute
```

`--json` carries the same under `coverage` (`tracedPaths`, `plays`, `choices`, `arms`, `untested`).

A `*.test.yaml` file declares:

```yaml
file: scenes/confrontation.lute   # path to the .lute under test, relative to this file
# optional mock surfaces — identical to `lute trace --mock`:
state:   { run.trueKiller: blake }
facts:   ["implicates(ledger, blake)"]
choose:  { accuse: accuseBlake }
events:  [npcSpoke]
accepts: [identifyKiller]
expect:
  transcriptContains: ["Case closed."]   # substrings that must appear in the transcript
  transcriptLacks: ["You let her go."]   # substrings that must NOT appear
  offered: { accuse: [accuseBlake, accuseMoss] }  # the exact options offered at a branch/hub
  state: { run.accused: blake }          # path: literal assertions after the walk
  exit: complete                         # complete | incomplete
```

`file:` is required; every mock surface and every `expect:` key is optional. The mock surfaces also include `visited:` and `occasions:` (dsl 0.21.0), the save seeds `quests:` and `entriesRead:` (dsl 0.22.0), and `derive:`, exactly as in a [trace mock](#trace). `expect.transcriptContains` lists substrings that must appear in the transcript and `expect.transcriptLacks` substrings that must not. `expect.offered` maps a `<branch>`/`<hub>` id to the exact set of options the walk offered there, order-insensitive and across all its presentations; a mismatch names both sets, and a branch the walk never presented fails as such. `expect.state` maps a state path to the literal it must hold after the walk — compared against the value the walk ends with, read the way trace reads it: the walk's last write, else the test's `state:` seed, else the declared `default:` — and `expect.exit` asserts the terminal verdict (`complete` or `incomplete`).

**An incomplete walk fails.** When an unknown guard halts the trace, the expectations after it were never walked, so the test fails — whatever else it asserts — unless it declares `expect: { exit: incomplete }`. Derivation is on (see [trace](#trace)): a rule-derived fact follows from the test's `facts:` and the project's seed facts, with no need to mock the conclusion. **Migration from 0.21:** a test that relied on an unmocked derived atom being unknown (exit `incomplete`), or on a seeded relation reading empty, now sees the derived or seeded answer; pin `derive: false` to keep the old verdict.

`file:` may name a lore document when the test says which entries to present: `entry: <id>`, or `entries: [ids]` to present several in order with the read flags set between them, so a repeated id is a re-read that skips first-read effects. A lore test that names neither is `E-TEST-LORE`, listing the declared entry ids:

```yaml
file: ../lore/tomas.lute
entries: [tomasOil, tomasBusy]
quests: { lampOut: active }            # seeds quest.lampOut.state, which tomasOil's `when` reads
expect:
  transcriptContains: ["Top shelf.", "Busy."]
```

`expect.quests` (dsl 0.21.0 §7a.4) asserts a quest document's lifecycle outcome directly — the state each quest ended the trace in, one of `unset`, `active`, `complete`, `failed`:

```yaml
file: quests/hold.lute
visited: [haven.shed]
occasions: [runEnd]
expect:
  quests: { holdLine: complete, sideJob: unset }
```

A mismatch fails as `quests holdLine: expected "complete", got "active"`; a value outside the four states, or a quest id the traced document does not declare, fails the test with a message naming it. (The top-level `quests:` key is the seed the walk *starts* from; `expect.quests` is the outcome it must *end* in.)

## lint

```console
$ lute lint [<path>] [--json] [--config <FILE>] [--deny <CODE>]… [--deny-warnings]
```

Run the advisory content lints — line length, dialogue ratio, emotion streaks, missing assets, and project-local rules — over a file or a directory tree (default: the current directory). The linear-VN norms (`L-SHOT-STARTS-WITH-BACKGROUND`, `L-DIALOGUE-RATIO`, `L-SCENE-LENGTH-SPREAD`) judge only linear scenes, never beats, components, quests, or lore. Documents are grouped by their nearest `lute.project.yaml`, and each project's `lute.lint.yaml` (or `--config <FILE>`) sets rule levels, thresholds, ignore globs, and `custom:` rules. Findings are `L-*` codes, separate from `lute check`: lints never enter the capability snapshot or change an artifact. `--deny`/`--deny-warnings` promote findings as in `check`. Exit **0** clean or only sub-error findings, **1** any error-severity finding (including `E-LINT-CONFIG`/`E-LINT-EXPR`), **2** on I/O, malformed YAML, or usage. Rules, metrics, and the config format: [Linting](/tooling/linting/).

## loc export

```console
$ lute loc export <dir> [--format json|csv] [-o <FILE>]
```

Extract every translatable content line — the stable `code`, speaker, text, and choice labels — across a project to a localization export. `--format` is `json` (default) or `csv`; `-o`/`--out` writes to a file instead of stdout. Exit **0** on success, **2** on I/O.

Each row also carries the `lineId` the compiler will stamp on that record — the join `loc import` and `compile --locales` key on. It is `null` (JSON) or empty (CSV) for a line with no authored `code`, whose id the compiler back-fills from the post-expansion command stream and which no source-only walk can reproduce: run `lute tag` first, and the advisory `N lines untagged — run lute tag` on stderr goes away with it.

## loc import

```console
$ lute loc import <file>… [-o <FILE>]
```

Canonicalize translated `loc export` files into one **locale bundle** — the reverse direction, consumed by `lute compile --locales`. Exit **0** on success, **1** on `E-LOCALE-BUNDLE`, **2** on I/O.

Input is exactly what `export` writes, in either format (`.csv` → CSV, anything else → JSON). `export` carries no locale, because it extracts the *source* language — so the normal workflow is **one file per locale**: copy the export to `ja-JP.json`, translate the `text`/`label` values, and the file **stem** is the locale tag. A row carrying its own non-empty `locale` field (JSON) or `locale` column (CSV) overrides that, so a single merged file spanning every locale also works.

```json
{
  "schemaVersion": 1,
  "locales": ["en-US", "ja-JP"],
  "entries": {
    "marina.s01ep02.marina_0010": { "en-US": "Hello there.", "ja-JP": "こんにちは。" }
  }
}
```

`locales` and `entries` are both sorted, so the bundle is byte-stable: importing the same inputs twice produces identical bytes. An unparseable input, a `lineId` appearing twice within one locale, or an empty locale tag is `E-LOCALE-BUNDLE`, reported with the offending file and row. A row with **no** `lineId` is skipped rather than rejected — an untagged line simply has no stable identity yet — and a single stderr summary counts them.

## loc report

```console
$ lute loc report <dir> [--json]
```

Word-count and line-count report per document and per speaker — a production-planning view over the same content lines. `--json` emits the report as JSON instead of human table lines. Exit **0** on success, **2** on I/O.

## version

```console
$ lute version [--json]
```

Print the three independent version axes ([versioning](https://github.com/journeyWorker/lute/blob/main/docs/versioning.md)): the **toolchain** version (this CLI and the workspace crates), the **language** version (the grammar/semantics the checker enforces), and the **IR** schema version (stamped as `irVersion` in every compiled artifact). Distinct from clap's built-in `--version`, which prints only the toolchain version; the language server answers the same flag, `lute-lsp --version` printing `lute-lsp <version>` (which [`doctor`](#doctor) compares against this CLI). `--json` prints one object `{"toolchain":…,"language":…,"ir":…}`; human mode prints one labeled line each. Always exits **0**.
