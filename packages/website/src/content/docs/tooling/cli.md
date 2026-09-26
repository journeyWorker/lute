---
title: CLI reference
description: Every lute subcommand — init, new, check, check-project, compile, compile-stream, run, play, trace, test, lint, scenario, beats, calendar, refs, loc, context, tag, fix, lore, doctor, catalog refresh, version — with its synopsis, key flags, and exit-code contract.
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

Since dsl 0.24.0 `check-project` also warns `W-QUEST-NEVER-ACCEPTED` (an accept-driven quest — no `start`, a root or an `activate="accept"` child — that no `::accept` in the project names), `W-RELATION-UNREAD` (a relation asserted, seeded or derived that no condition, rule body or def reads) and `W-DEF-UNUSED` (a `@def` nothing references), each once per project at its declaration, and its `mocks/*.yaml` pass checks a mock's `bridges:` answers against the plugin calls of the document it names (`E-TRACE-MOCK-UNDECLARED` / `E-TRACE-MOCK-TYPE`).

Since dsl 0.25.0: an `accepts:` mock or test no longer silences `W-QUEST-NEVER-ACCEPTED` (the message names it), a `<quest accept="external">` does, and `accept="external"` on a child that activates with its parent is `E-ACCEPT-TARGET`; [exclusive relations](/state/facts-and-datalog/#exclusive-relations-excludes) make a guard needing both dead, its negation guaranteed (`W-FACT-GUARANTEED`), an assert of one where the other holds on every route `E-FACT-EXCLUSIVE`, and a rule that can only break the exclusion `E-RULE-EXCLUSIVE`; beats of one [`share`](/language/beats/#one-event-several-places-share) key with different `once`s are `E-BEAT-ATTR`; and a bundle beat's `after=` names known nodes (`E-CONN-UNKNOWN-NODE`). A reserved relation's [`changedOn:`](/language/dialogue-and-cast/#presence-after-engine-events-changedon) orders cast presence over the scenario graph, so `assume: true` no longer covers a line after one of its occasions (`W-CAST-ABSENT`). A stale `luteVersion` that every document inherits from the manifest's `defaults:` is one `W-LUTE-VERSION-STALE` at that line of `lute.project.yaml`, ending `— every document inherits it (N documents)`, not one per document; a single-file `lute check` still reports it on the document.

Since dsl 0.26.0, for a project several authors write at once: `E-STATE-DECL-CONFLICT` compares every frontmatter `state:` declaration of one path — inline, or imported from a schema — and names both files and lines when `type`, `default`, `per` or `owner` disagree (`scene.*` paths are scene-local and exempt, and a declaration that refines one it `extends:` is no conflict); `E-REWARD-TARGET` checks a reward kind's `target:` contract — an `{ entity: <kind> }` or `{ provider: <name> }` contract validates the `target=` value, with a did-you-mean, and `required: true` rejects a reward with none; and `E-RULE-AGGREGATE-CYCLE` rejects a rule whose `count(…)` / `countDistinct(…)` reads a relation that depends on the rule's own head. Two advisories join them: `W-DISPLAY-NAME-DUP`, two speakers the dialogue box shows under one name (see [`lint`](#lint) and the [multi-author guide](/guides/multi-author/)), and `W-ENTRY-WRITE-REREAD`, an entry beat without `once` whose body has `::set` or `::retract` — it is presented again on every raise, but its writes apply on its first read in a run only (`::assert` is exempt: the fact holds for the rest of the run either way). Two documents declaring `run.mood` two ways, a misspelt reward target and a missing one, and a repeatable entry that pays out:

<!-- lute-diagnostics -->
```console
./scenes/hub/welcome.lute:10:3: error [E-STATE-DECL-CONFLICT] state path `run.mood` is declared as number, default 0 at `./scenes/hub/morning.lute:10` but as enum(calm|tense), default "calm" at `./scenes/hub/welcome.lute:10`; every declaration of one path must agree on type, default, per and owner — they share one runtime value (declare it once in a schema both documents import) (dsl 0.26.0 §2.1)
./quests/fishing.lute:8:31: error [E-REWARD-TARGET] `goodRodd` is not a member of entity kind `bagItem`, which `<reward kind="ITEM">` targets (dsl 0.26.0 §2.5) — did you mean `goodRod`?
./quests/fishing.lute:9:3: error [E-REWARD-TARGET] `<reward kind="ITEM">` needs a `target=`: the reward kind declares `target: { required: true }` (dsl 0.26.0 §2.5)
./lore/contest.lute:17:3: warning [W-ENTRY-WRITE-REREAD] `<entry id="deskNote">` has no `once`, so it can be presented again in a run, but its `::set` applies on the first read in a run only (dsl 0.19.0 §6); a write meant to repeat belongs in a `<beat once="false">`, and an entry read once per run says so with `once="run"`
```

The same release makes the fact analysis behind these passes cost what the project reads, not beats × facts: the seeds' derived closure is prepared once per project root and extended per slot, so on Monster League (818 beats, 211 static facts) `check-project` fell from 6.1 s to about 1.2 s, with the same output.

`--wip` (dsl 0.23.0) is for a project whose content is still being written. `E-ENTRY-UNREACHABLE`, `E-BEAT-UNREACHABLE`, and `E-OBJECTIVE-UNSATISFIABLE` become warnings when the guard is dead **only** because it reads a relation that nothing produces yet — no seed fact, no `::assert` anywhere, no rule, not `reserved`. Since dsl 0.26.0 §2.6 a relation whose only producer is a component `::assert` with an unbound `@param` — `hasBadge(@badge)` in a badge component no `::use` calls yet — counts as unproduced for specific arguments too, and a required `<objective quest=…>` whose child is dead only for that reason is a warning as well, so a lead can check a spine before the areas that fill it exist. A relation that has producers but can never match stays an error, and so does a downstream `E-CONN-UNREACHABLE` (a quest dead for want of unwritten content still feeds connectivity). An entry guarded on a `rumor` relation no document asserts yet:

<!-- lute-diagnostics unverified="verbatim lute check-project --wip output, but the message is composed at runtime: the E-ENTRY-UNREACHABLE literal plus the `--wip` downgrade suffix appended by lute-check, so no single format! literal spans it" -->
```console
$ lute check-project . --wip
./lore/tomas.lute:22:74: warning [E-ENTRY-UNREACHABLE] entry `noteWanted` is never eligible: its `when` guard `holds(rumor(mara))` is provably false — no seed, assert, rule, or engine relation produces `rumor(mara)` under your declared routes (dsl 0.20.0 §5) — a warning under `--wip`: it is dead only for want of producers not written yet (relations with no seed, assert, rule, or reserved declaration, or written only by a component `::assert` with an unbound `@param`) (dsl 0.23.0 §10, 0.26.0 §2.6)
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
              [--event N]… [--accept Q]… [--occasion O[@T]]… [--mock <FILE>] [--json]
              [--providers <DIR>] [--project <DIR>] [--entry <ID> | --beat <ID>] [--no-derive] [--expand]
```

Preview a document against author-supplied mocks (see the [tracing guide](/tooling/tracing/)). Exit **0** complete, **1** refused (check errors or invalid mocks — the `E-TRACE-*` codes render like check diagnostics — or, dsl 0.25.0, a write or a seed that makes two [exclusive relations](/tooling/tracing/#exclusive-relations) hold together, `E-FACT-EXCLUSIVE`), **2** I/O, **3** incomplete (an `unknown` guard halted the walk). `--occasion <O>` (repeatable, dsl 0.21.0) raises an occasion after a quest walk settles: every active quest's same-named `<on event="O">` handlers run first, then the `<objective on="O">` objectives of every active quest are judged; the mock file's `occasions:` list does the same, and its `visited:` list seeds the scenes `visited('<id>')` reads as presented (unlisted scenes are not visited). A quest walk settles as `lute play` settles it: a grant whose reward kind declares `credits:` adds its scalar amount to that path (the grant line ends `(credits <path> = <value>)`, JSON `credited`), and an objective's body plays once when the objective turns done — see [Rewards and objective bodies](/tooling/tracing/#rewards-and-objective-bodies).

A mock can also start from a save (dsl 0.22.0): `quests: { <id>: unset | active | complete | failed }` seeds `quest.<id>.state`, and `entriesRead: { run: [ids], user: [ids] }` seeds the entry read flags — `run:` both `entry.<id>.read` and `entry.<id>.everRead`, `user:` `entry.<id>.everRead` alone. Each follows the mock rules of the reserved path it spells — the document must read that path, or (for an entry) declare the entry.

**Derivation is on by default** (dsl 0.22.0). Trace loads the project's seed `facts:` and applies its Datalog rules (stratified negation) over the mocked and asserted facts, so a rule-derived fact satisfies a guard, `done`, or `start` without mocking the conclusion, and a derived fact that is false because a negated premise holds can be traced. Mocking a derived atom still works — it is a seed like any other. `--no-derive`, or `derive: false` in the mock (the flag wins), restores the 0.21 model: seeds are not loaded, an unmocked derived atom is unknown, and a note names each derived relation read. Trace, `test`, `run`, and `play` share one Datalog evaluator, so they cannot disagree about what a project's rules conclude.

`--entry <ID>` presents **one** `<entry>` of a [lore document](/language/lore-entries/) (dsl 0.19.0) instead of walking a sequence: its lines, the `<match>` arm taken, and the `::set` / `::assert` / `::retract` a first read applies — or skips, when the mock seeds `entry.<id>.read: true`. A lore document has no sequence to walk, so tracing one without `--entry` is a usage error (exit **2**, naming the declared entry ids); `--entry` on a scene or quest, or naming an id the document does not declare, is `E-TRACE-ENTRY` (exit **1**). For an unknown id the message lists the document's entries and beats, and when the id is a beat's it says to present it with `--beat`.

`--beat <ID>` (dsl 0.23.0) presents **one** [bundle beat](/tooling/play/#bundle-beats) of a lore document, by its local id or its canonical `<document id>.<beat id>`: its body is walked like a scene's (`--choose` decides its branches), every effect applies, and its `when` is noted, not enforced. A lore document takes `--entry` or `--beat`, not both; naming no beat of the document, or a document with no `<beat>`, is `E-TRACE-BEAT` (exit **1**). An occasion can be raised **for a target** as `--occasion <name>@<target>` (or `occasions: [talk@npc.oskar]` in the mock), which also judges the `<objective on="<name>" target="<target>">` objectives; a raise without the target leaves those alone. See [Bundle beats](/tooling/tracing/#bundle-beats) and [Targets, deadlines, and the previous run](/tooling/tracing/#targets-deadlines-and-the-previous-run).

Since dsl 0.24.0: a `<match on="@def">` subject, a guard, and the coverage summary print a def reference as authored (`<match @weekday>`, `-> old (@atLeast(3))`), and `--expand` prints the expansion instead (`--json` keeps the expansion and adds `authoredId` / `authoredGuard` / `authoredLabel`). A mock's `bridges:` answers plugin calls that read a bridge result; an unanswered one leaves its result slots unknown, so a guard over them halts the walk with a `bridges:` hint. `--accept` takes an `activate="accept"` child quest, a `complete="any"` parent's untaken alternative reads `superseded from quest.<parent>`, a raise `E@target` runs the `<on event="E" target="…">` handlers for that target, and with a project trace settles whether a read quest exists. See [Tracing](/tooling/tracing/).

Since dsl 0.26.0: a taken `::next` is followed to its label (`<next -> hall>`, JSON `{"kind": "jump", "to": "hall"}`), so `trace complete` means the walk reached the end of the document; an `::accept` in a quest `<on>` handler activates a quest of the same document, as in play; with a project `--accept` / `accepts:` resolve any quest of the project, and a quest document may seed its own `quest.<id>.*`; `--entry` also takes `<document id>.<entry id>`; and a [kind beat](/tooling/play/#kind-targets) reads its member from `--state occasion.target=<member>`. See [Following a taken `::next`](/tooling/tracing/#following-a-taken-next).

## scenario

```console
$ lute scenario <dir> [--providers <DIR>] [--format text|json|dot] [--facts]
              [reach <nodeId> | envelope <nodeId> | knowledge [--for <node>]]
```

Read-only reporting over the connectivity layer. With no subcommand, prints the assembled node/edge graph. `reach <nodeId>` reports a node's [reachability verdict](/connectivity/reachability/); `envelope <nodeId>` (or `envelope quest:<id>`) prints the [Guaranteed/Possible tables](/connectivity/envelopes/) plus the [guaranteed facts](/connectivity/envelopes/#guaranteed-facts) at the node's entry (dsl 0.20.0; `envelope.guaranteedFacts` in `--format json`, each `{fact, establishedBy}`). `<nodeId>` is a scene's canonical key, a quest id, or a [bundle beat](/tooling/play/#bundle-beats)'s canonical `<document id>.<beat id>`. A bare id is looked up among all three; when it names more than one kind, the command refuses and asks for an explicit `scene:`, `quest:` or `beat:` prefix, which always selects that kind. Exit **0** on success, **2** on I/O or an unresolvable or ambiguous node id.

`--format` selects the output shape of the bare graph view:

- `text` (default) — the topological layers, then one line per edge with the [atom kind(s)](/connectivity/scene-graph/#edge-kinds) that justify it in brackets (or the anchor kind: `[accept]`, `[subquest]`, `[start]`), then — when any exist — the unanchored nodes under ``unanchored (no `after` — available from the start of play; no prerequisites in this graph):``, one per line: each quest on no edge as `quest(<id>)`, and (dsl 0.25.0 §3) each scene or bundle beat without `after` whose `when` has a `visited()` conjunct, with the `after` to write (``beat(keeper.warning) — its `when` reads visited('keeper.greeting'), which gates it but draws no edge; write `after="visited('keeper.greeting')"` to anchor it (dsl 0.25.0 §3)``).
- `json` — `{"roots":[{"root":…,"layers":[[…]],"nodes":[…],"edges":[…]}]}`. Each node is `{id, kind, prereq, reach}` (`kind` is `scene`, `quest`, `beat`, or `entry` — the source of a quest's `[start]` anchor, dsl 0.25.0 §4; `prereq` is the raw declared formula, `null` for an entry node); each edge is `{from, to, kinds}`, where `kinds` is an array because one formula may reference the same node under more than one atom. A root with unanchored nodes also carries `"unanchored": ["quest(<id>)", …]` and, for the scene and beat hints, `"unanchoredHints": { "<node>": "<hint>" }` (each omitted when empty).
- `dot` — one Graphviz `digraph` per root; scenes are boxes, quests ellipses, bundle beats `shape=note`, entries `shape=tab`, and an `active`-only edge is drawn `[style=dashed]`. An unanchored quest is a dashed blue ellipse labelled `quest(<id>) (unanchored)`.

**Fact edges** (dsl 0.26.0 §8). Progress gated by facts — a badge, a key item — draws no `after:` edge, so every gym sits in layer 0. `--facts` adds an edge `producer -> reader [fact]` wherever a scene, beat or quest's gate (`when:` / `start=`) reads `holds(F)` and the producer asserts `F`, or — through the rules — a fact a rule deriving `F` needs, written `[hasItem(goodRod), via canPass(pier)]`; the layers are then drawn over those edges too, wherever one closes no cycle. Text lists them under `fact edges (producer -> reader) [asserted fact]:` and heads the layers `topological layers (after: and fact edges):`; `--format json` adds `factEdges`, each `{from, to, fact, via?, layered}`; `--format dot` draws them dotted, labelled with the fact. See [Story overviews](/tooling/overviews/#fact-edges-lute-scenario---facts).

**Bundle beats are nodes** (dsl 0.23.0 §4). Every `<beat>` of a lore document is drawn as `beat(<document id>.<beat id>)` — an entry node in layer 0 with no edges when it declares no `after=`, selected by its occasion, target and `when` instead, and (dsl 0.25.0 §3) the dependent of the edges its `after=` draws when it does. `reach <beat id>` answers `Reachable`, then an `after:` line (`(none declared) — an entry node: …`, or the beat's `after=` formula), the file that declares it (`declared in:`), and its `on:`, `target:` and `when:` as written; `envelope <beat id>` prints the entry-floor tables, with the seed facts as its guaranteed facts. Since dsl 0.24.0 §2 an `after:` may name a bundle beat (`visited('<doc>.<beat>')`), and connectivity routes through it like a scene. See [The scene graph](/connectivity/scene-graph/#bundle-beats).

An unanchored quest (dsl 0.21.0 §7a.5) sits in no layer and on no edge, but it is not missing from the report: `reach quest:<id>` gives it the verdict ``Unanchored — a quest with no declared `after` prerequisite: available from the start of play; …`` (JSON `"reach": "unanchored"`), and its `after:` line reads `(none declared) — unanchored: this quest is in no prerequisite graph layer and on no edge; it is available from the start of play.` A quest anchored without `after=` — by the `::accept`s that take it up, its parent (a subquest), or its `start` conjuncts (dsl 0.25.0 §4) — lists its anchors instead, `[start] entry(keeperLog)` one per line (`anchors` in JSON, each `{ kind, from }`).

Since 0.23.0 the graph view also ends with a `note:` listing the `completed()`/`active()`/`visited()` references it did not draw — a quest's edges come from its `after`, its subquest tree, its `start` conjuncts and its `::accept`s, so a `completed()` naming a quest on no edge, and a quest's `visited()` read outside its anchoring `start` conjuncts, are listed (`omitted` in `--format json`); see [What the scenario graph leaves out](/tooling/overviews/#what-the-scenario-graph-leaves-out). `knowledge [--for <node>]` traces every fact-guarded condition through the rules to the producers of its facts — asserting documents, seed facts, `reserved`, or `NO PRODUCER`. Since dsl 0.24.0 it covers every guard slot — beat, entry and objective guards, line `when=`, `<choice when>`, `<when>` arm tests, `::next`/`::set` `when`, `<on when>`, reward `when`, quest `start`/`fail` and objective `until` — grouped by document with each guard's source line; a negation reads "holds unless defeated" with one `defeated when …` line per defeater — a derived defeater with every derivation route, `⇐ … / ⇐ …` (dsl 0.25.0 §9) — or "always holds (…) — cannot be defeated", and a derived atom's rules print once (later mentions say `traced above under …`). `knowledge` takes `--format text` or `json` (before the subcommand; JSON elements carry `for` and `line`); `--for` takes a scene (every guard in it), a bundle beat, an entry id, `<quest>.<objective>`, `quest:<id>`, or `<scene>#<branch>.<choice>` for one choice, and an unmatched one is exit **2** with a did-you-mean. A rule's premises are traced too: an entity-kind atom reads as membership (`person(ada) — entity kind `person`; ada is a member`, not an undeclared relation), and a `cel(…)` premise names the state it reads (`cel("run.slot == 'evening'") — state condition on run.slot, decided at run time`); in `--format json` a rule's premises are `{ relation, negated? }`, `{ entityKind }`, or `{ cel }`. `envelope`'s Facts rows use the same producer wording (`seed facts …`, `reserved — the engine asserts it`, `derived by 1 rule`). Both are described, with real output, in [Story overviews](/tooling/overviews/#lute-scenario-knowledge).

Since dsl 0.26.0 §6 a rule body may count (`canPass(earthGymDoor) :- count(hasBadge(_)) >= 7`), and `knowledge` traces such a premise as `count(hasBadge(_)) >= 7 — counts:` with the producers of the counted facts beneath it.

## beats

```console
$ lute beats <dir> [--occasion <O>]… [--target <T>]… [--expand] [--json]
```

Print every beat of the project as one ladder per occasion — and per target of a targeted occasion — in selection order (dsl 0.23.0): priority, beat and title, kind (`scene`, `entry`, `bundle`), `once` (`run`, `user`, `day`, `slot`, or `no` — a bundle beat's `once="day"` / `once="slot"` included — and `also`, and since dsl 0.25.0 a `share` key: `day, share solWarm`; `share` in `--json`), the `check-project` verdicts (`unreachable`, `shadowed`, `tied`, `once-run-user`), `after:` (a bundle beat's `after=` included), and `when` as the author wrote it — `@def` references included (dsl 0.24.0); `--expand` prints each `when` with its defs expanded, and `--json` carries both, `when` (expanded) and `whenAuthored`. `--occasion` and `--target` (both repeatable) filter the ladders; `--json` carries each verdict's full diagnostic. Read-only, and the project need not check clean. Exit **0** on success, **2** on I/O or an unknown `--occasion`. See [Story overviews](/tooling/overviews/#lute-beats).

Since dsl 0.26.0: a fallback that an earlier, never-spent beat whose `when` it implies always beats shows `covered by <id>` in the verdict column (`coveredBy` in `--json`) — informational, not a diagnostic, so a lead can tell a fallback that still plays from one that no longer can; a [kind beat](/tooling/play/#kind-targets) is listed in the ladder of every member some beat names, and in a `kind:<kind>` ladder for the members no beat names on its own; and `--target` accepts any member of a targeted occasion's domain.

## calendar

```console
$ lute calendar <dir> [--axis <path>=<lo>..<hi> | <path>=<a>,<b>,… | clock[=<d1>..<d2>]]…
                [--occasion <O>[@<axis>[=<value>],…]]… [--target <T>]… [--facts <REL>]…
                [--script <FILE> [--until <STEP>]] [--where <CEL>] [--json | --csv]
```

Evaluate `lute play`'s own eligibility at every cell of a grid of state values (dsl 0.23.0). For every cell and every occasion/target column, the grid shows the winner and `+N` shadowed eligible beats, the offered or sequence list, `-` when nothing is eligible, or `?` when an undecidable `when` decides the cell; the beats never eligible in any cell are listed at the end with the reason, and — since dsl 0.24.0 — so are the beats eligible in some cell but presented in none, with what was presented over them (`neverPresented`, each with `beatenBy`, in `--json`; a second table in `--csv`). Each cell starts from the same world, gets its axis values written, and has its quests settled before its occasions are evaluated.

- `--axis <path>=<values>` (optional, repeatable) — an inclusive integer range (`run.day=1..7`) or a list (`run.slot=morning,evening`) over a declared path, checked against its type; the first axis varies slowest, and with no `--axis` the grid is one cell. `quest.<id>.state=unset,active,…` seeds the quest's status as a save's `quests:` does (`unset` and `active` also clear the objective progress a replayed route made), and `quest.<id>.objectives.<oid>.done` is accepted as written; any other `quest.*` path is a usage error. `holds(<fact>)=true,false` asserts or retracts a base fact of a declared relation; a derived relation is refused (`` `trusted(ada)` is derived by rules and cannot be asserted ``). `visited('<id>')=true,false` (dsl 0.24.0) puts a scene or bundle beat in or out of the visited set, so an `after: visited(…)` beat reads both ways. `clock[=<d1>..<d2>]` (dsl 0.24.0 §1) expands to every slot of those days in clock order (bare `clock`: one week). Anything else is a usage error that lists the axis kinds, with a did-you-mean for a mistyped state path.
- `--occasion <O>` (repeatable) — the occasions to evaluate; default every occasion a beat answers. `<O>@<axis>,…` (dsl 0.24.0) varies only the named axes for `O`: it is evaluated where every other axis is at its first value (`dayEnd@run.day,run.slot=night` holds `run.slot` at `night` instead) and left blank elsewhere, so a once-a-day occasion reads once per day (`--json`: the column's `varies` and `heldAt`). With `--axis clock`, name the clock's axes — `dayEnd@clock.day,clock.slot=night`, or the clock's own paths, `dayEnd@run.day,run.slot=night` — to evaluate `O` once per day at one slot (the day's first slot when none is named).
- `--target <T>` (repeatable) — the targets a targeted occasion is raised for. By default a targeted occasion gets one column per target its beats name, not one per member of its domain; when no beat names a target, a single `(any)` column only its untargeted beats answer.
- `--facts <REL>` (repeatable, dsl 0.24.0) — print the relation's facts in every settled cell: a table with a row per first argument and a column per cell (`--facts at`: who is where, when); a per-cell `facts` map in `--json`, a `facts:<rel>` column in `--csv`.
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
  Since dsl 0.24.0 it is a small whodunit built on the `beats` skeleton: a `case.occasions` plugin raising `arrive`, the targeted `examine` (`item.<evidence>`) and `interview` (`npc.<suspect>`), and `accuse`; evidence lore that `::assert`s what is on record; derived rules with stratified negation (a suspect is `cleared` by an alibi unless evidence contradicts it; the `culprit` is implicated and not cleared); an accept-driven quest; an accusation whose choices are guarded by `holds(culprit(…))`; `plays/the-case.play.yaml` with `expect:`, and a scenario test. `check-project`, `test` and `play` pass as scaffolded, and its README shares the `beats` commands.
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

Since dsl 0.24.0 each entry and beat row is followed by its `when`, and the report ends with a **Derived** section: for every derived relation, each conclusion the rules can reach from what the project asserts, the rule instance behind it (`⇐` its premises, each with its source), the evidence it rests on, and the conditions it gates (`--json`: `derived`, each fact with `fact`, `from`, `evidence`, `gates`):

```text
Derived (what the rules can conclude from what the project asserts)
  trusted
    trusted(ada)
      ⇐ met(ada) [scene `inn.ada` (scenes/inn/ada.lute)], not rumor(ada)
      evidence: scene `inn.ada` (scenes/inn/ada.lute)
      gates: objective `ferry.word` (quests/ferry.lute), scene `inn.again` (scenes/inn/again.lute)
```

## refs

```console
$ lute refs <dir> [--attr <DIRECTIVE.ATTR>]… [--reward <KIND>]… [--json]
```

Who uses which engine content id (dsl 0.26.0 §2.5): every value of a directive attribute (`--attr give.item`) or every `target=` of a reward kind (`--reward ITEM`), with the documents and lines using it, so a lead sees who gives what before merging several authors' work — the same rod handed out by three areas, a TM spelt two ways. Both flags repeat and combine; naming neither is a usage error (exit **2**). A value that reaches the attribute through a component — `::use{component="gift" item="goodRod"}` over a body `::give{item=@item}`, through nested `::use`s too — is listed at the `::use` line that binds it, ``(via component `gift`)``; a reward with no `target=` is listed as `(no target)`; an attribute or kind nothing uses reads `no uses`.

```console
$ lute refs . --attr give.item --reward ITEM
::give.item: 2 value(s)
  `goodRod` — 1 use(s) in 1 document(s)
    scenes/wren.lute:15 (via component `gift`)
  `potion` — 1 use(s) in 1 document(s)
    scenes/wren.lute:13
reward ITEM: 2 value(s)
  (no target) — 1 use(s) in 1 document(s)
    quests/fishing.lute:9
  `goodRod` — 1 use(s) in 1 document(s)
    quests/fishing.lute:8
```

`--json` emits `{ "queries": [ { "kind": "attr" | "reward", "name", "values": [ { "value", "uses": [ { "document", "line", "via"? } ] } ] } ] }`, with `value` `null` for `(no target)` and `via` the component name. The report lists, it does not validate: an attribute typed by an entity kind (`item: { type: { entity: bagItem } }`) and a reward kind's `target:` contract are what `check` judges. Read-only; documents need not check clean. Exit **0** on success, **2** on I/O or a usage failure. See the [multi-author guide](/guides/multi-author/).

## new

```console
$ lute new <scene|quest|lore|schema> <name> [--dir <PROJECT>]
$ lute new scene <name> --on <occasion> [--target <target>] [--dir <PROJECT>]
$ lute new quest <name> [--start] [--dir <PROJECT>]
```

Scaffold one new document into an existing project. The first argument is the document kind (`scene`, `quest`, `lore`, or `schema`); `<name>` is the file stem, and a `/` in it nests the file in a subfolder (`lute new scene talk/tomas-evening` writes `scenes/talk/tomas-evening.lute`). Every document gets an `id:` — a scene's is its name (dotted by folder: `talk.tomasEvening`), `lute new quest` writes `quests/<name>.lute` with `id: quest.<ident>`, and `lute new lore` writes `lore/<name>.lute` with `id: lore.<ident>` and one `<entry>` attached to `item.<ident>` — and omits whatever the manifest's `defaults:` already supplies (such as `luteVersion` or `uses`). A `-` camel-cases within a segment, and since dsl 0.24.0 a dotted name keeps its dots as the id: `lute new scene isolde.night` writes `scenes/isolde.night.lute` with `id: isolde.night`, and `lute new quest lamp.oil` a document `id: quest.lamp.oil`. `lute new quest` scaffolds an **accept-driven** stub — no `start`, with a comment naming the `::accept{quest="…"}` that begins it; `--start` writes the auto-starting `start="true"` form instead. `--dir` names the **project** (default: the current directory), and the document lands under the enclosing project's root, the directory holding its `lute.project.yaml`. Outside any project, `lute new` says so on stderr and writes a self-contained document.

A `--dir` inside a project but not at its root is refused (exit **2**, nothing written), with the spelling to use instead — it used to write to `<root>/scenes/<name>.lute` without a word:

```console
$ lute new scene tavi-shell --dir scenes/talk
lute new: `--dir scenes/talk` resolves to `/private/tmp/tp/town/scenes/talk`, which is inside the project at `/private/tmp/tp/town` (its lute.project.yaml) but not its root; `--dir` names the project, not the destination folder — did you mean `lute new scene talk/tavi-shell`? (nothing was written)
```

Running `lute new` without `--dir` from a folder below the project root is refused the same way, and the suggestion names the root:

```console
$ cd scenes && lute new scene tavi-shell
lute new: no `--dir` was given, so `lute new` started from the current directory `/private/tmp/tp/town/scenes`, which is inside the project at `/private/tmp/tp/town` (its lute.project.yaml) but not its root; run from the project root, or pass `--dir <root>` — did you mean `lute new scene tavi-shell --dir /private/tmp/tp/town`? (nothing was written)
```

`--on <occasion>` (dsl 0.22.0) makes the scene a [beat](/language/beats/) answering that occasion, and `--target <target>` names what a targeted occasion is raised for, a `<prefix>.<member>` of its target domain. Both are checked against the project before anything is written — an occasion the project's plugins do not declare, or a target outside the occasion's domain, is exit **2** with a did-you-mean and no file:

```console
$ lute new scene talk/tomas-evening --on talk --target npc.tomass
lute new: target `npc.tomass` is outside occasion `talk`'s domain `npc.<npc>` (`npc.mara`, `npc.tomas`) — did you mean `npc.tomas`? (dsl 0.22.0 §8); nothing was written
```

Outside a project `--on` is refused, since no occasion is declared. Exit **0** on success, **2** on I/O, an invalid kind, a refused `--on`/`--target`, or a `--dir` (or, without one, a current directory) below the project root.

## doctor

```console
$ lute doctor [<dir>] [--json] [--strict]
```

Diagnose the local toolchain and project setup: the version axes, the project manifest, the content documents, play scripts (`*.play.yaml`) and scenario tests (`*.test.yaml`), provider snapshots, the active plugins, every declared occasion with the number of beats answering it, the declared vocabulary slots, and editor integration. `<dir>` is the project directory to inspect (default: the current directory). Provider snapshots are looked for in the manifest's `catalogDir:` (default `catalog/`), the directory `check` reads; with none there the line says `no pinned provider snapshots`. The editor check runs `lute-lsp --version` on the first `lute-lsp` on `PATH` and flags one that reports another version — or none, as a server older than 0.22.0 does — with how to reinstall it. It also compares the `lute-lsp` beside the running `lute` with the one on `PATH` (`lute-lsp beside lute`; `siblingLanguageServer` in `--json`): another build first on `PATH` — same version or not — fails the check, naming both binaries, with the fix: put the `lute` directory first on `PATH`, or point the editor's language server at the `lute-lsp` beside it.

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

`--json` emits the same checks as one object, `{ "dir", "checks": { <key>: { label, ok, detail, hint } } }` (`ok` is `null` for an informational line). By default doctor is a report, never a gate: exit **0** whatever the checks find. `--strict` (dsl 0.26.0) exits **1** when any check fails (`✗`) — a stale running `lute-lsp`, another build beside `lute`, a stale snapshot — so a harness or CI step can refuse to start on a broken setup. Exit **2** when `<dir>` cannot be read. Since dsl 0.26.0 a replaced `lute-lsp` also says so itself in the editor — see [Editors](/tooling/editors/#a-server-older-than-its-binary).

## run

```console
$ lute run <artifact> [--mock <FILE>] [--occasion <O>[@<target>]]… [--json] [--entry <ID> | --beat <ID>]
```

Execute a **compiled artifact** (`lute compile` output) headlessly against a mock playthrough — the reference consumer of the [runtime contract](/tooling/runtime-contract/): command dispatch, CEL guards, the facts + Datalog fixpoint, hubs, and quest lifecycle. Distinct from `lute trace`, which previews *source*; `run` consumes the artifact an engine would. `--mock` is a YAML playthrough (the same surfaces as `lute trace --mock`); `--json` emits the machine-readable transcript. Exit **0** on a complete run, **1** refused, **2** on I/O, **3** incomplete.

Since dsl 0.24.0 the mock's `bridges: { <tag>: [ {<field>: value}, … ] }` answers plugin calls that read a bridge result, one answer per call of the tag in call order: the answered values are written to the result slots and the `plugin` record carries `"answered": [{"field", "value"}, …]` (human: `plugin check (bridge answered: passed=true, margin=3)`). A call with no answer keeps its record's `unresolvedEffects` and the walk goes on. A failing `quest` record carries `failedBy`, and a quest failure prints its reason (`quest X -> failed (by)`, `(until)`, `(fail)`, `(cascade)`, `(superseded)`).

For a quest artifact, `--occasion <O>` (repeatable, dsl 0.21.0) raises an occasion after the walk settles — after the mock's own `occasions:`, in CLI order — judging the `<objective on="O">` objectives of every active quest; each raise is an `{"kind": "occasion", "occasion": "O"}` record (human: `  occasion O`). A quest with no `start` is accept-driven here as in an engine: it stays `unset` until the mock's `accepts:` names it or an `accept` record runs. A scene's `::accept{quest="<id>"}` is an `{"kind": "accept", "quest": "<id>"}` record (human: `<address>  quest <id> accepted`); when the walk already knows the quest to be past `unset`, the record carries `"ignored": "already <state>"` and the line ends ` (already <state> — ignored)`. The mock's `visited:` list seeds the scenes `visited('<id>')` reads as presented.

`--entry <ID>` presents one `entry` record of a **lore artifact** (dsl 0.19.0; [engine contract](https://github.com/journeyWorker/lute/blob/main/docs/runtime/lore-entries.md)): the transcript reports whether it is a first read and whether its `when` holds, runs its body segment, applies first-read effects (or records them as skipped once `entry.<id>.read` is seeded `true`), and then sets `entry.<id>.read`. It is required for a lore artifact and refused on any other kind (both exit **2**).

`--beat <ID>` (dsl 0.23.0) presents one **bundle beat** of a lore artifact instead — the `beat` record named by its canonical `<document id>.<beat id>` (or the bare beat id when unambiguous) and its body segment, run like a scene with every effect applied. A lore artifact needs exactly one of `--entry` / `--beat`; an id the artifact does not declare, or `--beat` on another artifact kind, is exit **2** (`` lute run: `--beat oskar.hnut` names no beat in this artifact (declared: oskar.hunt, oskar.rumor) ``). `--occasion <name>@<target>` raises an occasion for a target, judging a quest's `<objective on="<name>" target="<target>">` objectives (human: `  occasion talk → npc.oskar`); a raise for another target, or none, does not judge them.

## play

```console
$ lute play <PROJECT_DIR> --script <FILE> [--json] [--ir] [--no-derive] [--explain <ATOM>]…
```

Play a story through a WHOLE project as a sequence of raised **occasions** (dsl 0.21.0) — the reference-runtime consumer of [beats and occasions](/tooling/play/). The project is compiled once, in memory, with the same gate and declaration union `compile --all` uses (scene, quest, and lore documents). The required `--script` is a `*.play.yaml` file with a closed key set. Its `steps:` each do one thing — raise an `occasion:` (with `target:`, `pick:`, and a step-local `choose:` that replaces the script's `choose:` key by key for that presentation), start a `newRun:`, write what the engine owns with `engine:` (`state:` literals or `{ add: <n> }`, `facts:`, `retract:`), fire a world `event:`, move a declared clock with `advance: slot | day | <n>` (dsl 0.24.0: writes the clock's paths, settles the quests, raises `dayEnd` / `dayStart` at each midnight it crosses and the `slot` occasion once where it stops, per the clock's `raise:`; it may carry the same moment's `engine:` writes, which land where the clock arrives — after any `dayEnd` / `dayStart` on the way, before the final settle and raise), or end the playthrough with `end: true` — and any step may carry `label:`, `repeat: <n>`, its own `bridges:` answers, and `expect: { winner, offered, notOffered, presented, options, quests, state, facts, notFacts }`; `- include: <file>` splices another file's steps in place. Beside `steps:`, the script takes the `lute trace --mock` grammars for `state:`, `facts:`, `choose:` and `bridges:`, a save to start from (`visited:`, `presented: { run, user }`, `quests:`, `entriesRead: { run, user }`), `derive:`, and a top-level `expect: { exit, quests, state, facts, notFacts, transcriptContains, transcriptLacks }` judging the end of the play (dsl 0.22.0). Each occasion step lists the occasion's candidate beats with their verdicts, presents the winner (or the step's `pick` on a `select: all` occasion; `pick: none`, or no `pick` when nothing is eligible, presents nothing) through `lute run`'s reference evaluator, and advances every quest lifecycle, so later `when` conditions and `after: completed(…)` see real progress. `--json` emits the same transcript as one object.

Since dsl 0.26.0: `advance: { to: <slot> }` / `advance: { to: { weekday, slot } }` moves the clock to the next such position (forward only, never zero steps), and a step `expect:` takes `clock: { weekday, slot, day }`; `engine: { accept: [quest ids] }` accepts an accept-driven quest mid-play (`quest <id> accepted (engine)`); an entry may be named `<document id>.<entry id>` in `pick:`, `entriesRead:` and the selection expectations; a beat targeting `kind:<kind>` answers every member of the kind; a guarded directive that did not run prints `skip ::give{item="potion"} — when: false`; a bridge answer is typed by its result slot or the capability's `result:`, and an untyped one is refused (exit **2**); and a project whose documents declare one state path with two types is refused (exit **1**). The project loads once per play — on Monster League a play load fell from about 12 s to about 1.5 s. See [Playing a story](/tooling/play/).

`--no-derive` (or the script's `derive: false`; the flag wins) stops applying the project's Datalog rules, so an unmocked derived atom is unknown and halts the walk incomplete. `--explain <ATOM>` (repeatable) prints, after the play, the derivation tree of a ground atom — the rule used and each premise's own support (seed fact, asserted, or derived in turn), negated premises shown `(absent)` — or, when it does not hold, every rule that could conclude it with its failing premises; `--json` carries the same tree under `explain`.

Exit **0** complete (every step played, or an `end: true` step ended the playthrough) with every expectation met, **1** an error (the project fails to compile, a vocabulary conflict, a `pick` that is not eligible, a `select: all` step with eligible beats and no `pick`, a step whose own `bridges:` answers went unconsumed, two [exclusive relations](/tooling/play/#exclusive-relations) holding together — `✗ exclusive: …` at the write, dsl 0.25.0 — or an `expect:` miss — each miss names the step, its `label:`, and the actual value), **2** a usage/I/O failure (a malformed script, an unknown occasion or `expect:` key, a step target outside its occasion's target domain, a save id the project does not declare, a `bridges:` answer that fits no call or lacks a field content reads, an `include:` cycle, an `engine:` write that does not fit its declared type or moves the clock backward — or any `quest.*` write, since quest status is the lifecycle's and a save seeds it with top-level `quests:` — or an unreadable project), **3** incomplete (an unscripted choice or hub, a `when` or quest objective the reference runtime cannot decide, `now()`/`validAt()`, or a plugin call with no `bridges:` answer, which halts at the call). A `choose:` decision that is not offered — an ineligible choice, or a `once` hub option already taken (`E-TRACE-CHOICE`) — is exit **1**, like an ineligible `pick`. `lute test` runs every play script that carries an `expect:`. Script format, selection order, and transcript shapes: [Playing a story](/tooling/play/).

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

`lute test` loads and checks each project **once** (dsl 0.26.0 §1) and shares it across every test and play under it: the producer set and quest ids are collected once, a play project is compiled once for all its plays — its compile diagnostics print once, not once per play — and then the tests, and then the plays, run in parallel on every logical core (set `RAYON_NUM_THREADS` to cap the threads). Results are reported in the usual file order, each one's standard error replayed in that order.

`--coverage` also reports branch/arm coverage across the tested documents and lists the **untested** documents — every testable document no `*.test.yaml` names and no play presents — under `--project`, or else under the nearest `lute.project.yaml` above `<dir>`, so `lute test tests --coverage` still measures the whole project. Every document a play presented counts as covered — through an `occasion:` step or through the occasions a clock raises after an `advance:` step — while the branch/hub and arm rows come from traced paths alone, as the header says. Since a lore document is testable, an untested one is listed too:

```console
coverage over 2 traced path(s) and 1 play(s) (plays count toward documents presented only, not branches or arms):
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

Since dsl 0.24.0 a test takes the mock's `bridges:` key too (see [Bridge answers](/tooling/tracing/#bridge-answers)), and three expectations read more: `expect.accepts: [quest ids]` asserts the quests the scene's `::accept`s took, as a set (`accepts: expected [toll], got [parley]`); `expect.offered` of a `<hub>` is every choice eligible at any of its visits, unioned (it was always `[]`); and `transcriptContains` / `transcriptLacks` match only the content lines that played, each in the form `@speaker: text` — the form a play script matches, so `"@narrator: Always shown."` works in both, and a guarded line that never played no longer satisfies `transcriptContains`.

Since dsl 0.26.0 a needle's own line attributes are dropped too, so a line pasted from a play or a trace — `"@mara{emotion=\"content\"}: You're new."` — matches, and a `transcriptContains` miss names the presented line nearest to the needle: `transcriptContains "@mara: Tomas keeps the oil. Ask her.": absent (nearest line: "@mara: Would you? Tomas keeps the oil. Ask him.") (expected present)`.

**An incomplete walk fails.** When an unknown guard halts the trace, the expectations after it were never walked, so the test fails — whatever else it asserts — unless it declares `expect: { exit: incomplete }`. Derivation is on (see [trace](#trace)): a rule-derived fact follows from the test's `facts:` and the project's seed facts, with no need to mock the conclusion. **Migration from 0.21:** a test that relied on an unmocked derived atom being unknown (exit `incomplete`), or on a seeded relation reading empty, now sees the derived or seeded answer; pin `derive: false` to keep the old verdict.

**`exit: complete` means the end of the document** (dsl 0.26.0 §7). The walk follows a taken `::next` to its label, as play does, so `exit: complete` holds only when the walk reached the end — not when it stopped at a jump — and the transcript and state expectations see what play sees (see [Following a taken `::next`](/tooling/tracing/#following-a-taken-next)). The walk also applies an `::accept` in a quest `<on>` handler to a quest of the same document, so `expect: { quests: { second: active } }` holds as it does in play; `accepts:` resolves quests project-wide, a quest another document declares included; and a quest document may seed its own `quest.<id>.*` — `quests: { lampOut: active }` in a test of `quests/lamp.lute` — which starts the quest there. Before 0.26.0 each of these disagreed with `lute play`.

`file:` may name a lore document when the test says what to present: `entry: <id>`, or `entries: [ids]` to present several in order with the read flags set between them, so a repeated id is a re-read that skips first-read effects — or `beat: <id>`, one [bundle beat](/tooling/tracing/#bundle-beats) by its bare or canonical `<document id>.<beat id>`, walked as `lute trace --beat` walks it. A test takes `beat:` or `entry:`/`entries:`, not both (exit **2**), and a lore test that names none is `E-TEST-LORE`, which lists the declared entry ids and beat ids. Two entries, read in order:

```yaml
file: ../lore/tomas.lute
entries: [tomasOil, tomasBusy]
quests: { lampOut: active }            # seeds quest.lampOut.state, which tomasOil's `when` reads
expect:
  transcriptContains: ["Top shelf.", "Busy."]
```

An entry may be named `<document id>.<entry id>` in `entry:`, `entries:` and `expect.eligible` keys (`entries: [lore.tomas.tomasBusy]`, dsl 0.26.0 §8). A bundle beat that targets a [whole kind](/tooling/play/#kind-targets) reads its member from a seed, `state: { occasion.target: r16Gus }`, typed by the kind, as a play step's `target` would bind it.

Trace presents an entry or a beat whether or not its `when` holds — the `when` is the engine's gate, shown, not enforced. `lute test` enforces it (dsl 0.26.0 §7): a test that presents an entry, a bundle beat or a scene beat its mocks make ineligible — its `when` is false, its `after:` does not hold, its `once: user` is spent, or an entry's `once="run"` / `once="user"` is spent — **fails** unless it asserts `expect.eligible`, since the engine would never present it and the walk proves nothing about play. An entry's `once` is spent by its read flags, which the mock's `entriesRead:` seeds (`run:` sets `entry.<id>.read` and `everRead`, `user:` sets `everRead` alone) and an earlier presentation in the same test sets, so the failure names which: ``it is `once="run"` and already spent — the mocked `entriesRead: { run: [<id>] }` read it (`entry.<id>.read`)``. Without the `quests:` seed, `quest.lampOut.state` reads `unset` and Tomas's oil entry is not eligible:

```yaml
# tests/tomas-oil.test.yaml
file: ../lore/tomas.lute
entry: tomasOil
expect:
  transcriptContains: ["Top shelf."]
```

```console
FAIL  ./tests/tomas-oil.test.yaml  (./tests/../lore/tomas.lute)
      eligible tomasOil: not eligible under these mocks (its `when` is false) — the engine would never present it, so the walk proves nothing about play; fix the mocks, or assert `expect: { eligible: { tomasOil: false } }` (the body is then not walked)
```

The failure names the premise that is false and, where one exists, the mock that makes it hold: ``its `when` (user.bond.mara >= 1) is false``, ``its `after: visited('hub.welcome')` is false — mock `visited: [hub.welcome]` ``, or ``it is `once: user` and the mocked `visited:` already lists it``. Before 0.26.0 such a test passed with a note, so a contract test of another author's scene stayed green after that scene could no longer be presented.

`expect.eligible` asserts the verdict: `true` or `false` for every entry and beat the test presents, or a mapping from id to verdict (`eligible: { tomasOil: false }`; a beat may be named by its bare id). With it asserted an ineligible body is not walked: an `eligible: false` test needs no bridge answers for a body the engine never plays, and a transcript expectation on that body fails as absent. Since dsl 0.26.0 it works on a scene beat (`on:`) too, judged by the scene's `when`, `after:` and `once`. A map key may also name an entry or bundle beat of the file that the test did not present (dsl 0.24.0): it is judged alone, under the same mocks, so a lore test may carry a map-form `eligible:` without presenting anything. A `when` the mocks leave undecided matches neither verdict. A miss reads `eligible tomasOil: expected true, got false`.

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

Since dsl 0.26.0 lint also reports `W-DISPLAY-NAME-DUP`, the advisory `check-project` reports too: two speakers the dialogue box would show under one name — two cast entries with the same `name:`, a cast name equal to a `::use{… name="…"}` display string, or two such strings for different speakers (`who=`). A cast entry marked `sharedName: true` is an intended role name several speakers share and is not counted. The code is not a `lute.lint.yaml` rule; `--deny W-DISPLAY-NAME-DUP` makes it an error. See [Linting](/tooling/linting/#display-names).

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
