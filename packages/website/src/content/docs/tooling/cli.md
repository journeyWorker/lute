---
title: CLI reference
description: Every lute subcommand — init, new, check, check-project, compile, compile-stream, run, play, trace, test, lint, scenario, beats, calendar, loc, context, tag, fix, lore, doctor, catalog refresh, version — with its synopsis, key flags, and exit-code contract.
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
$ lute check-project <dir> [--json] [--providers <DIR>] [--deny <CODE>]… [--deny-warnings] [--wip]
```

Recursively `check` every `*.lute` file under `<dir>` in deterministic sorted order, each against its own nearest-ancestor `lute.project.yaml` root, **plus** project-wide `<quest id>` uniqueness, the connectivity passes (`E-CONN-*`, `W-QUEST-REF-UNKNOWN`, `E-STATE-MAYBE-UNAVAILABLE`), and the [relational guard analysis](/state/facts-and-datalog/#how-check-project-analyzes-relational-guards) (dsl 0.20.0): every `holds(…)`/`count(…)` in a guard is decided impossible, guaranteed, or possible over the whole project, so a guard that can never hold is reported through its slot's dead-code error (`E-ARM-DEAD`, `E-ENTRY-UNREACHABLE`, `E-OBJECTIVE-UNSATISFIABLE`, `E-QUEST-UNREACHABLE`) and a redundant one as `W-FACT-GUARANTEED`. The project-wide beat and quest advisories run here too (dsl 0.22.0): `W-BEAT-PRIORITY-TIE` (beats on one `select: first` occasion, for the same or no target, with equal priority and `when`s not provably exclusive, so file order picks the winner), `W-BEAT-ONCE-RUN-USER` (a `once: run` beat whose `when` reads only user-tier state, so it replays every run), and `W-QUEST-HANDLER-DEAD` (an `<on event="questFailed">` on a quest that has no `fail`, no required subquest objective, and no parent quest, so it can never fail). It then compiles every document that checked clean under its project's `identity:`, so compile-stage errors fail here rather than only at `compile`: `E-DUP-VOICEKEY` (lines with different text on one `voiceKey` — since 0.22.0 the default `{prefix}.{speaker}-{code}` keeps scenes apart, so this is what a project pinning the older unprefixed `identity.voiceKey: "{speaker}-{code}"` risks) and `E-CAPABILITY-MISMATCH` (documents resolving two capability snapshots, which `compile --all` and `play` refuse). Exit **0** clean, **1** when any file has an error or a project-wide collision, **2** on I/O. The same `--deny <CODE>`/`--deny-warnings` promotion (see `check`) applies project-wide.

`--wip` (dsl 0.23.0) is for a project whose content is still being written. `E-ENTRY-UNREACHABLE`, `E-BEAT-UNREACHABLE`, and `E-OBJECTIVE-UNSATISFIABLE` become warnings when the guard is dead **only** because it reads a relation that nothing produces yet — no seed fact, no `::assert` anywhere, no rule, not `reserved`. A relation that has producers but can never match stays an error, and so does a downstream `E-CONN-UNREACHABLE` (a quest dead for want of unwritten content still feeds connectivity). An entry guarded on a `rumor` relation no document asserts yet:

<!-- lute-diagnostics unverified="verbatim lute check-project --wip output, but the message is composed at runtime: the E-ENTRY-UNREACHABLE literal plus the `--wip` downgrade suffix appended by lute-check, so no single format! literal spans it" -->
```console
$ lute check-project . --wip
./lore/places.lute:35:72: warning [E-ENTRY-UNREACHABLE] entry `noteWanted` is never eligible: its `when` guard `holds(rumor(bo))` is provably false — no seed, assert, rule, or engine relation produces `rumor(bo)` under your declared routes (dsl 0.20.0 §5) — a warning under `--wip`: only relations that nothing produces yet (no seed, assert, rule, or reserved declaration) make it so (dsl 0.23.0 §10)
```

Without `--wip` the same line is an `error` and the command exits **1**; with it the project passes. [`lute scenario knowledge`](/tooling/overviews/#lute-scenario-knowledge) lists every such relation as `NO PRODUCER`. Since 0.23.0 the decider behind these three codes, `E-ARM-DEAD`, and `W-BEAT-PRIORITY-TIE` also reasons per path across `&&`/`||`, so a contradiction such as `run.slot == 'morning' && run.slot == 'evening'` is now reported where 0.22 missed it — see [how a `when` is decided](/language/beats/#how-a-when-is-decided).

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
              [--providers <DIR>] [--project <DIR>] [--entry <ID> | --beat <ID>] [--no-derive]
```

Preview a document against author-supplied mocks (see the [tracing guide](/tooling/tracing/)). Exit **0** complete, **1** refused (check errors or invalid mocks — the `E-TRACE-*` codes render like check diagnostics), **2** I/O, **3** incomplete (an `unknown` guard halted the walk). `--occasion <O>` (repeatable, dsl 0.21.0) raises an occasion after a quest walk settles: every active quest's same-named `<on event="O">` handlers run first, then the `<objective on="O">` objectives of every active quest are judged; the mock file's `occasions:` list does the same, and its `visited:` list seeds the scenes `visited('<id>')` reads as presented (unlisted scenes are not visited). A quest walk settles as `lute play` settles it: a grant whose reward kind declares `credits:` adds its scalar amount to that path (the grant line ends `(credits <path> = <value>)`, JSON `credited`), and an objective's body plays once when the objective turns done — see [Rewards and objective bodies](/tooling/tracing/#rewards-and-objective-bodies).

A mock can also start from a save (dsl 0.22.0): `quests: { <id>: unset | active | complete | failed }` seeds `quest.<id>.state`, and `entriesRead: { run: [ids], user: [ids] }` seeds `entry.<id>.read` and `entry.<id>.everRead`. Each follows the mock rules of the reserved path it spells — the document must read that path.

**Derivation is on by default** (dsl 0.22.0). Trace loads the project's seed `facts:` and applies its Datalog rules (stratified negation) over the mocked and asserted facts, so a rule-derived fact satisfies a guard, `done`, or `start` without mocking the conclusion, and a derived fact that is false because a negated premise holds can be traced. Mocking a derived atom still works — it is a seed like any other. `--no-derive`, or `derive: false` in the mock (the flag wins), restores the 0.21 model: seeds are not loaded, an unmocked derived atom is unknown, and a note names each derived relation read. Trace, `test`, `run`, and `play` share one Datalog evaluator, so they cannot disagree about what a project's rules conclude.

`--entry <ID>` presents **one** `<entry>` of a [lore document](/language/lore-entries/) (dsl 0.19.0) instead of walking a sequence: its lines, the `<match>` arm taken, and the `::set` / `::assert` / `::retract` a first read applies — or skips, when the mock seeds `entry.<id>.read: true`. A lore document has no sequence to walk, so tracing one without `--entry` is a usage error (exit **2**, naming the declared entry ids); `--entry` on a scene or quest, or naming an id the document does not declare, is `E-TRACE-ENTRY` (exit **1**). For an unknown id the message lists the document's entries and beats, and when the id is a beat's it says to present it with `--beat`.

`--beat <ID>` (dsl 0.23.0) presents **one** [bundle beat](/tooling/play/#bundle-beats) of a lore document, by its local id or its canonical `<document id>.<beat id>`: its body is walked like a scene's (`--choose` decides its branches), every effect applies, and its `when` is noted, not enforced. A lore document takes `--entry` or `--beat`, not both; naming no beat of the document, or a document with no `<beat>`, is `E-TRACE-BEAT` (exit **1**). An occasion can be raised **for a target** as `--occasion <name>@<target>` (or `occasions: [talk@npc.oskar]` in the mock), which also judges the `<objective on="<name>" target="<target>">` objectives; a raise without the target leaves those alone. See [Bundle beats](/tooling/tracing/#bundle-beats) and [Targets, deadlines, and the previous run](/tooling/tracing/#targets-deadlines-and-the-previous-run).

## scenario

```console
$ lute scenario <dir> [--providers <DIR>] [--format text|json|dot]
              [reach <nodeId> | envelope <nodeId> | knowledge [--for <node>]]
```

Read-only reporting over the connectivity layer. With no subcommand, prints the assembled node/edge graph. `reach <nodeId>` reports a node's [reachability verdict](/connectivity/reachability/); `envelope <nodeId>` (or `envelope quest:<id>`) prints the [Guaranteed/Possible tables](/connectivity/envelopes/) plus the [guaranteed facts](/connectivity/envelopes/#guaranteed-facts) at the node's entry (dsl 0.20.0; `envelope.guaranteedFacts` in `--format json`, each `{fact, establishedBy}`). `<nodeId>` is a scene's canonical key, a quest id, or a [bundle beat](/tooling/play/#bundle-beats)'s canonical `<document id>.<beat id>`. A bare id is looked up among all three; when it names more than one kind, the command refuses and asks for an explicit `scene:`, `quest:` or `beat:` prefix, which always selects that kind. Exit **0** on success, **2** on I/O or an unresolvable or ambiguous node id.

`--format` selects the output shape of the bare graph view:

- `text` (default) — the topological layers, then one line per edge with the [atom kind(s)](/connectivity/scene-graph/#edge-kinds) that justify it in brackets, then — when any exist — the quests with no `after=` under ``unanchored (no `after` — available from the start of play; no prerequisites in this graph):``, one `quest(<id>)` per line.
- `json` — `{"roots":[{"root":…,"layers":[[…]],"nodes":[…],"edges":[…]}]}`. Each node is `{id, kind, prereq, reach}` (`kind` is `scene`, `quest`, or `beat`; `prereq` is the raw declared formula, `null` for an entry node); each edge is `{from, to, kinds}`, where `kinds` is an array because one formula may reference the same node under more than one atom. A root with unanchored quests also carries `"unanchored": ["quest(<id>)", …]` (omitted when there are none).
- `dot` — one Graphviz `digraph` per root; scenes are boxes, quests ellipses, bundle beats `shape=note`, and an `active`-only edge is drawn `[style=dashed]`. An unanchored quest is a dashed blue ellipse labelled `quest(<id>) (unanchored)`.

**Bundle beats are nodes** (dsl 0.23.0 §4). Every `<beat>` of a lore document is drawn as `beat(<document id>.<beat id>)` — an entry node in layer 0 with no edges, since a bundle beat declares no `after` and is selected by its occasion, target and `when` instead. `reach <beat id>` answers `Reachable`, then an `after:` line saying a bundle beat declares none, the file that declares it (`declared in:`), and its `on:`, `target:` and `when:` as written; `envelope <beat id>` prints the entry-floor tables, with the seed facts as its guaranteed facts. An `after:` cannot name a bundle beat: `visited('<doc>.<beat>')` there is still `E-CONN-UNKNOWN-NODE`, whose message suggests the `visited()` read in a `when` instead. See [The scene graph](/connectivity/scene-graph/#bundle-beats).

An unanchored quest (dsl 0.21.0 §7a.5) sits in no layer and on no edge, but it is not missing from the report: `reach quest:<id>` gives it the verdict ``Unanchored — a quest with no declared `after` prerequisite: available from the start of play; …`` (JSON `"reach": "unanchored"`), and its `after:` line reads `(none declared) — unanchored: this quest is in no prerequisite graph layer and on no edge; it is available from the start of play.`

Since 0.23.0 the graph view also ends with a `note:` listing the `completed()`/`active()`/`visited()` references it did not draw because the quest involved declares no `after=` (`omitted` in `--format json`), and `knowledge [--for <node>]` traces every fact-guarded beat, entry, and objective through the rules to the producers of its facts — asserting documents, seed facts, `reserved`, or `NO PRODUCER`. `knowledge` takes `--format text` or `json` (before the subcommand); `--for` takes a scene key, an entry id, `<quest>.<objective>`, or `quest:<id>`, and an unmatched one is exit **2** with a did-you-mean. Both are described, with real output, in [Story overviews](/tooling/overviews/#lute-scenario-knowledge).

## beats

```console
$ lute beats <dir> [--occasion <O>]… [--target <T>]… [--json]
```

Print every beat of the project as one ladder per occasion — and per target of a targeted occasion — in selection order (dsl 0.23.0): priority, beat and title, kind (`scene`, `entry`, `bundle`), `once` (and `also`), the `check-project` verdicts (`unreachable`, `shadowed`, `tied`, `once-run-user`), `after:`, and `when` with every `@def` expanded. `--occasion` and `--target` (both repeatable) filter the ladders; `--json` carries each verdict's full diagnostic. Read-only, and the project need not check clean. Exit **0** on success, **2** on I/O or an unknown `--occasion`. See [Story overviews](/tooling/overviews/#lute-beats).

## calendar

```console
$ lute calendar <dir> [--axis <path>=<lo>..<hi> | <path>=<a>,<b>,…]…
                [--occasion <O>]… [--target <T>]… [--script <FILE> [--until <STEP>]]
                [--where <CEL>] [--json | --csv]
```

Evaluate `lute play`'s own eligibility at every cell of a grid of state values (dsl 0.23.0). For every cell and every occasion/target column, the grid shows the winner and `+N` shadowed eligible beats, the offered or sequence list, `-` when nothing is eligible, or `?` when an undecidable `when` decides the cell; the beats never eligible in any cell are listed at the end with the reason. Each cell starts from the same world, gets its axis values written, and has its quests settled before its occasions are evaluated.

- `--axis <path>=<values>` (optional, repeatable) — an inclusive integer range (`run.day=1..7`) or a list (`run.slot=morning,evening`) over a declared path, checked against its type; the first axis varies slowest, and with no `--axis` the grid is one cell. `quest.<id>.state=unset,active,…` seeds the quest's status as a save's `quests:` does (`unset` and `active` also clear the objective progress a replayed route made), and `quest.<id>.objectives.<oid>.done` is accepted as written; any other `quest.*` path is a usage error. `holds(<fact>)=true,false` asserts or retracts a base fact of a declared relation; a derived relation is refused.
- `--occasion <O>` (repeatable) — the occasions to evaluate; default every occasion a beat answers.
- `--target <T>` (repeatable) — the targets a targeted occasion is raised for. By default a targeted occasion gets one column per target its beats name, not one per member of its domain; when no beat names a target, a single `(any)` column only its untargeted beats answer.
- `--script <FILE>` — a play script: every cell starts from its save, with its `steps:` replayed exactly as `lute play` plays them.
- `--until <STEP>` — with `--script`, replay only the steps before this one — a 1-based step number or a step `label:` (an unknown label gets a did-you-mean); the step itself is not played.
- `--where <CEL>` — keep only the cells where this condition holds after the axes are written and the quests settle, to prune combinations of independent axes no run reaches. It is plain CEL — an `@def` call is not expanded there — and a cell where it evaluates unknown is a usage error.
- `--json` / `--csv` — machine-readable output; `--json` carries `from` (where the cells start), `pruned` (cells `--where` dropped), and per cell `notes`.

More than 10,000 cells is refused, and a replay that halts is a usage error. Exit **0** on success, **1** when the project does not compile, **2** on I/O or a usage error. See [Story overviews](/tooling/overviews/#lute-calendar).

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

Print the project's **world-narrative map** (dsl 0.19.0): every [lore entry](/language/lore-entries/) under `<dir>` grouped by `target` and by `series` (in `order`), then, for every relation asserted anywhere in the project, each ground fact and who reveals it — lore entries, bundle beats, scenes/quests, or `both` (more than one of those) — with the entries, beats, and documents that do. Beats are listed under their target beside the entries, labelled `beat` and followed by their occasion: a [bundle beat](/tooling/play/#bundle-beats) under its canonical `<document id>.<beat id>`, a scene beat under its scene key.

```text
  item.rusty_key
    beat  lore.dock.talk  "The key"  lore/dock.lute  (on talk)
    rustyKey  [item]  lore/ship.lute
```

A fact a bundle beat asserts is credited to the beat (a `beats:` line), not to the entries; a scene beat's asserts stay its scene's. `--json` emits the same report as an object with `targets`, `series`, and `relations` arrays: a beat row carries `"kind": "beat"` and `on`, and each fact carries `entries`, `beats`, and `documents`, with `revealedBy` one of `entries`, `beats`, `scenes`, or `both`. Read-only; documents need not check clean. Exit **0** on success, **2** on I/O.

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

An editor keeps running the `lute-lsp` it started, so reinstalling the toolchain does not reach an open editor. On macOS and Linux doctor therefore also lists every running `lute-lsp` server (`runningLanguageServers` in `--json`) and flags one whose binary was replaced after the server started, or whose binary reports another version, with the fix — restart the editor or its language server:

```console
  ✗ running lute-lsp: pid 12431 (/usr/local/bin/lute-lsp) started before its binary was replaced, so it runs an older build
      → restart the editor (or its language server) so it launches this toolchain's lute-lsp
```

`--json` emits the same checks as one object, `{ "dir", "checks": { <key>: { label, ok, detail, hint } } }` (`ok` is `null` for an informational line). A report, never a gate: exit **0** whatever the checks find, **2** when `<dir>` cannot be read.

## run

```console
$ lute run <artifact> [--mock <FILE>] [--occasion <O>[@<target>]]… [--json] [--entry <ID> | --beat <ID>]
```

Execute a **compiled artifact** (`lute compile` output) headlessly against a mock playthrough — the reference consumer of the [runtime contract](/tooling/runtime-contract/): command dispatch, CEL guards, the facts + Datalog fixpoint, hubs, and quest lifecycle. Distinct from `lute trace`, which previews *source*; `run` consumes the artifact an engine would. `--mock` is a YAML playthrough (the same surfaces as `lute trace --mock`); `--json` emits the machine-readable transcript. Exit **0** on a complete run, **1** refused, **2** on I/O, **3** incomplete.

For a quest artifact, `--occasion <O>` (repeatable, dsl 0.21.0) raises an occasion after the walk settles — after the mock's own `occasions:`, in CLI order — judging the `<objective on="O">` objectives of every active quest; each raise is an `{"kind": "occasion", "occasion": "O"}` record (human: `  occasion O`). A quest with no `start` is accept-driven here as in an engine: it stays `unset` until the mock's `accepts:` names it or an `accept` record runs. A scene's `::accept{quest="<id>"}` is an `{"kind": "accept", "quest": "<id>"}` record (human: `<address>  quest <id> accepted`); when the walk already knows the quest to be past `unset`, the record carries `"ignored": "already <state>"` and the line ends ` (already <state> — ignored)`. The mock's `visited:` list seeds the scenes `visited('<id>')` reads as presented.

`--entry <ID>` presents one `entry` record of a **lore artifact** (dsl 0.19.0; [engine contract](https://github.com/journeyWorker/lute/blob/main/docs/runtime/lore-entries.md)): the transcript reports whether it is a first read and whether its `when` holds, runs its body segment, applies first-read effects (or records them as skipped once `entry.<id>.read` is seeded `true`), and then sets `entry.<id>.read`. It is required for a lore artifact and refused on any other kind (both exit **2**).

`--beat <ID>` (dsl 0.23.0) presents one **bundle beat** of a lore artifact instead — the `beat` record named by its canonical `<document id>.<beat id>` (or the bare beat id when unambiguous) and its body segment, run like a scene with every effect applied. A lore artifact needs exactly one of `--entry` / `--beat`; an id the artifact does not declare, or `--beat` on another artifact kind, is exit **2** (`` lute run: `--beat oskar.hnut` names no beat in this artifact (declared: oskar.hunt, oskar.rumor) ``). `--occasion <name>@<target>` raises an occasion for a target, judging a quest's `<objective on="<name>" target="<target>">` objectives (human: `  occasion talk → npc.oskar`); a raise for another target, or none, does not judge them.

## play

```console
$ lute play <PROJECT_DIR> --script <FILE> [--json] [--no-derive] [--explain <ATOM>]…
```

Play a story through a WHOLE project as a sequence of raised **occasions** (dsl 0.21.0) — the reference-runtime consumer of [beats and occasions](/tooling/play/). The project is compiled once, in memory, with the same gate and declaration union `compile --all` uses (scene, quest, and lore documents). The required `--script` is a `*.play.yaml` file with a closed key set. Its `steps:` each do one thing — raise an `occasion:` (with `target:`, `pick:`, and a step-local `choose:` that replaces the script's `choose:` key by key for that presentation), start a `newRun:`, write what the engine owns with `engine:` (`state:` literals or `{ add: <n> }`, `facts:`, `retract:`), or fire a world `event:` — and any step may carry `label:`, `repeat: <n>`, and `expect: { winner, offered, notOffered }`. Beside `steps:`, the script takes the `lute trace --mock` grammars for `state:`, `facts:`, and `choose:`, a save to start from (`visited:`, `presented: { run, user }`, `quests:`, `entriesRead: { run, user }`), `derive:`, and a top-level `expect: { exit, quests, state, facts, notFacts, transcriptContains, transcriptLacks }` judging the end of the play (dsl 0.22.0). Each occasion step lists the occasion's candidate beats with their verdicts, presents the winner (or the step's `pick` on a `select: all` occasion; `pick: none` presents nothing) through `lute run`'s reference evaluator, and advances every quest lifecycle, so later `when` conditions and `after: completed(…)` see real progress. `--json` emits the same transcript as one object.

`--no-derive` (or the script's `derive: false`; the flag wins) stops applying the project's Datalog rules, so an unmocked derived atom is unknown and halts the walk incomplete. `--explain <ATOM>` (repeatable) prints, after the play, the derivation tree of a ground atom — the rule used and each premise's own support (seed fact, asserted, or derived in turn), negated premises shown `(absent)` — or, when it does not hold, every rule that could conclude it with its failing premises; `--json` carries the same tree under `explain`.

Exit **0** complete (every step played, or a scene's `::end`) with every expectation met, **1** an error (the project fails to compile, a vocabulary conflict, a `pick` that is not eligible, or an `expect:` miss — each miss names the step, its `label:`, and the actual value), **2** a usage/I/O failure (a malformed script, an unknown occasion or `expect:` key, a step target outside its occasion's target domain, a save id the project does not declare, an `engine:` write that does not fit its declared type — or any `quest.*` write, since quest status is the lifecycle's and a save seeds it with top-level `quests:` — or an unreadable project), **3** incomplete (an unscripted choice or hub, or a `when`, quest objective, `now()`/`validAt()`, or plugin `bridgeResult` the reference runtime cannot decide). A `choose:` decision that is not offered — an ineligible choice, or a `once` hub option already taken (`E-TRACE-CHOICE`) — is exit **1**, like an ineligible `pick`. `lute test` runs every play script that carries an `expect:`. Script format, selection order, and transcript shapes: [Playing a story](/tooling/play/).

## test

```console
$ lute test [<dir> | <file>] [--json] [--providers <DIR>] [--project <DIR>] [--coverage] [--no-derive]
```

Run the project's scenario tests: every `*.test.yaml` under `<dir>` (default: the current directory) traces its scene or quest — or presents a lore document's entries or one of its bundle beats — against the declared mocks and asserts the declared expectations, and every `*.play.yaml` under `<dir>` that carries an `expect:` (on a step or at the top level) is played exactly as [`lute play`](#play) plays it and judged by its own expectations (dsl 0.22.0). The argument may also be one `*.test.yaml` or `*.play.yaml` file, which runs alone; any other file is exit **2**. `--json` emits the machine-readable report; `--providers` pins a snapshot directory; `--project` resolves each traced document against the project (its `defaults:` and plugins) exactly as `lute trace --project` does. Without `--project`, a test resolves its document against the nearest `lute.project.yaml` above it, as `lute check <file>` does, and standard error says so once per project — `lute: note: scenario tests use project . (nearest lute.project.yaml); pass --project to choose another`; with no manifest above the document the trace is core-only. A play runs against `--project`, else the nearest `lute.project.yaml` above the script. `--no-derive` turns derivation off for every test and play, overriding their own `derive:` keys (see [trace](#trace)). Exit **0** when every test and play passes, **1** on a failure, **2** on I/O or a malformed test file.

Each test and play prints one `PASS`/`FAIL` line naming its file and what it ran, a failure followed by its misses, then a summary:

```console
$ lute test . --project .
PASS  ./tests/lamp-quest.test.yaml  (./tests/../quests/lamp.lute)
PASS  ./tests/mara-first.test.yaml  (./tests/../scenes/talk/mara-first.lute)
FAIL  ./plays/first-day.play.yaml  (play of .)
      step 6 at hubVisit: expect winner: expected hub.welcome, actual hub.morning

2 passed, 1 failed
```

A test whose `file:` names a document that does not exist fails on its own as `E-TEST-FILE`, and the rest of the suite still runs:

<!-- lute-diagnostics -->
```console
FAIL  ./tests/old-shed.test.yaml  (./tests/../scenes/old-shed.lute)
        error [E-TEST-FILE] `file: ../scenes/old-shed.lute` names no document (./tests/../scenes/old-shed.lute does not exist)
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
  facts: ["points(blake)"]               # atoms that hold when the walk ends, after derivation
  notFacts: ["points(cass)"]             # atoms that do not hold then
  exit: complete                         # complete | incomplete
```

`file:` is required; every mock surface and every `expect:` key is optional. The mock surfaces also include `visited:` and `occasions:` (dsl 0.21.0), the save seeds `quests:` and `entriesRead:` (dsl 0.22.0), and `derive:`, exactly as in a [trace mock](#trace). `expect.transcriptContains` lists substrings that must appear in the transcript and `expect.transcriptLacks` substrings that must not. `expect.offered` maps a `<branch>`/`<hub>` id to the exact set of options the walk offered there, order-insensitive and across all its presentations; a mismatch names both sets, and a branch the walk never presented fails as such. `expect.state` maps a state path to the literal it must hold after the walk — compared against the value the walk ends with, read the way trace reads it: the walk's last write (a [credited reward](/tooling/tracing/#rewards-and-objective-bodies) included), else the test's `state:` seed, else the declared `default:` — and `expect.exit` asserts the terminal verdict (`complete` or `incomplete`). `expect.facts` and `expect.notFacts` list ground atoms that must hold, or must not, when the walk ends — after derivation, so a rule's conclusion is asserted directly. A miss names both sides (`facts points(blake): expected holds, got does not hold`), and an atom whose derivation read undecided state is `unknown`, which satisfies neither list.

**An incomplete walk fails.** When an unknown guard halts the trace, the expectations after it were never walked, so the test fails — whatever else it asserts — unless it declares `expect: { exit: incomplete }`. Derivation is on (see [trace](#trace)): a rule-derived fact follows from the test's `facts:` and the project's seed facts, with no need to mock the conclusion. **Migration from 0.21:** a test that relied on an unmocked derived atom being unknown (exit `incomplete`), or on a seeded relation reading empty, now sees the derived or seeded answer; pin `derive: false` to keep the old verdict.

`file:` may name a lore document when the test says what to present: `entry: <id>`, or `entries: [ids]` to present several in order with the read flags set between them, so a repeated id is a re-read that skips first-read effects — or `beat: <id>`, one [bundle beat](/tooling/tracing/#bundle-beats) by its bare or canonical `<document id>.<beat id>`, walked as `lute trace --beat` walks it. A test takes `beat:` or `entry:`/`entries:`, not both (exit **2**), and a lore test that names none is `E-TEST-LORE`, which lists the declared entry ids and beat ids. Two entries, read in order:

```yaml
file: ../lore/tomas.lute
entries: [tomasOil, tomasBusy]
quests: { lampOut: active }            # seeds quest.lampOut.state, which tomasOil's `when` reads
expect:
  transcriptContains: ["Top shelf.", "Busy."]
```

Trace presents an entry or a beat whether or not its `when` holds — the `when` is the engine's gate, shown, not enforced — so a test that presents one its mocks make ineligible says so, even when it passes. Without the `quests:` seed, `quest.lampOut.state` reads `unset` and Tomas's oil entry is not eligible:

```yaml
# tests/tomas-oil.test.yaml
file: ../lore/tomas.lute
entry: tomasOil
expect:
  transcriptContains: ["Top shelf."]
```

```console
PASS  ./tests/tomas-oil.test.yaml  (./tests/../lore/tomas.lute)
      note: `tomasOil` is not eligible under these mocks (its `when` is false); the test presents it anyway — assert it with `expect: { eligible: false }`
```

`expect.eligible` asserts that verdict: `true` or `false` for every entry and beat the test presents, or a mapping from id to verdict for the ones it names (`eligible: { tomasOil: false }`; a beat may be named by its bare id). A `when` the mocks leave undecided matches neither. A miss reads `eligible tomasOil: expected true, got false`, and an id the test did not present fails as `… but the test presented no such entry or beat`.

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
