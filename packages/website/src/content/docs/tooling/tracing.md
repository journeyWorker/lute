---
title: Tracing guide
description: "Preview a scene before you ship it — seeding state, facts, choices, events, accepts, the visited set, a save's quest status and entry reads, raised occasions (for a target, too), the previous run and plugin bridge answers via flags or a mock YAML file, credited rewards and objective bodies, quest structure, presenting one bundle beat or a kind beat's member, how the project's rules derive over them, reading the decision transcript (def references as authored, or `--expand`ed; a taken `::next` followed to its label), and the E-TRACE-* refusals."
---

`lute trace` walks a document once, deterministically, against **author-supplied mocks**, reporting every decision and why. It is an authoring preview, not a guarantee: it never feeds `check`/`compile`, and is never a static reachability proof. It explores only the mock scenarios you supply — a coverage aid, never a proof. Since 0.22.0 it applies the project's seed facts and Datalog rules over those mocks by default, exactly as `lute run` and `lute play` do — see [Derivation](#derivation). Since dsl 0.26.0 it also follows a taken `::next` to its label and applies a quest `<on>` handler's `::accept`, as play does, and `lute test` fails a test that presents a beat its mocks make ineligible — see [Following a taken `::next`](#following-a-taken-next) and [`lute test`](/tooling/cli/#test).

## Seeding the world

Trace walks the world you supply. A state path you do not seed reads its declared default (unknown when it has none), and the facts are your mocks, the schema's seed `facts:`, and everything the project's rules derive from them. Five surfaces feed the walk, as repeatable flags:

```console
$ lute trace scene.lute \
    --state run.metMira=true \
    --fact "inParty(shadowheart)" \
    --choose sofaHelp=help \
    --event npcSpoke \
    --accept sideQuest
```

- `--state <path>=<literal>` — a scalar seed on a declared path.
- `--fact "<rel>(<arg>…)"` — a ground fact, valid-now, over the declared vocabulary (a *supplied answer*, so it may name a `derive:`/`reserved:` relation).
- `--choose <id>=<choiceId>[,<choiceId>…]` — a menu selection at a `<branch>`/`<hub>` id; a hub may force a whole ordered visit sequence via one flag's comma list.
- `--event <name>` — fire a capability/world event, in CLI order.
- `--accept <questId>` — simulate accepting a `start`-less (accept-driven) quest. With a project, since dsl 0.26.0, it may name any quest of the project, not only one the traced document declares; an id no quest declares is `E-TRACE-ACCEPT`.

Two more surfaces feed quest and scene conditions (dsl 0.21.0):

- `--occasion <name>` — raise an occasion after the walk settles, in CLI order (repeatable, after the mock's own `occasions:`). Each raise first runs every active quest's `<on event="<name>">` handlers — an occasion also fires the same-named world event, as it does in `lute play` and `lute run` — and then judges the `<objective on="<name>">` objectives of every active quest, so an objective can read what a handler wrote; an `on` objective is **never** judged otherwise. `--occasion <name>@<target>` (dsl 0.23.0) raises it for a target — see [Targets, deadlines, and the previous run](#targets-deadlines-and-the-previous-run).
- `visited: [<scene id>…]` (mock file only) — the scenes already presented, read by `visited('<scene id>')` in any condition. The set is closed: a scene you do not list is not visited, so a `visited(…)` read is always `true` or `false`, never unresolved.
- `bridges: { <tag>: [ {<field>: value}, … ] }` (mock file or test only, dsl 0.24.0 §5) — answers for plugin calls that read a bridge result, one per call of that tag, in call order. See [Bridge answers](#bridge-answers).

The same surfaces live in a `--mock <file.yaml>` document; CLI flags compose with it, the flag winning on a conflict.

```yaml
state:  { run.metMira: true }
facts:  ["inParty(shadowheart)"]
choose: { sofaHelp: help }
events: [npcSpoke]
accepts: [sideQuest]
visited: [haven.shed]
occasions: [runEnd]
```

Two notes point at an occasion mismatch without refusing the walk: an objective whose occasion the walk never raised (``objective `q.o` is judged at occasion `runEnd`, which this walk never raised (supply `--occasion runEnd` or `occasions: [runEnd]`)``), and a raised occasion nothing in the document answers (``occasion `dayEnd` is judged by no `<objective on>` and fires no `<on event>` handler in this document``).

### A save's history

Two more mock keys (dsl 0.22.0, mock file only) seed what a player's save already holds — the same keys a [play script starts from](/tooling/play/#starting-from-a-save):

- `quests: { <questId>: unset | active | complete | failed }` — a quest's lifecycle status, exactly a `quest.<id>.state` state seed.
- `entriesRead: { run: [<entry id>…], user: [<entry id>…] }` — `run` seeds `entry.<id>.read: true` and `entry.<id>.everRead: true` (read this run is read ever), `user` seeds `entry.<id>.everRead: true` alone (read in an earlier run: a new run clears `read` and keeps `everRead`). A spent `once="run"` / `once="user"` entry is ineligible: trace heads it ``not eligible (`once="run"` is already spent)``, and a test that presents it fails unless it asserts `expect.eligible`.

Both are spellings of reserved state paths, so they follow those paths' mock rules: the traced document must read (or declare) the path. Seeding a quest the document never reads, or an entry it neither reads nor declares, is `E-TRACE-MOCK-UNDECLARED`, and a status outside the four is `E-TRACE-MOCK-PARSE`. Since dsl 0.26.0 a quest document counts as reading the `quest.<id>.*` paths of every quest it declares, so its own test may seed `quests: { lampOut: active }` and the walk starts the quest there. In the [`beats` scaffold](/tooling/play/#worked-example), Tomas's oil entry is guarded on the lamp quest being active:

```yaml
# mocks/oil.yaml
quests: { lampOut: active }
```

```console
$ lute trace lore/tomas.lute --project . --entry tomasOil --mock mocks/oil.yaml
trace: lore/tomas.lute  (seeds: 1 paths, 0 facts; 0 selections)
  <entry tomasOil>   (first read)
    @tomas  Oil? Top shelf. Tell Mara it's the wick, not the oil.
    ::assert  knows(lamp)
trace complete: 0 decisions
```

Without the mock the entry reads ``(first read, not eligible (`when` is false))``, and the same `quests:` against a scene that never reads `quest.lampOut.state` is refused with `E-TRACE-MOCK-UNDECLARED`. With `--project` (or a manifest above the file) trace settles whether a quest the document reads exists (dsl 0.24.0): a quest the project declares draws no note, and one it does not says so with a did-you-mean — ``quest `lampOot` is declared by no quest document of the project — did you mean `lampOut`? (every read of `quest.lampOot.*` takes its reserved default)``. Only a trace with no project still notes that existence is unverified.

## Targets, deadlines, and the previous run

Three 0.23.0 additions reach trace through the same surfaces.

**Raising for a target.** An objective with `target=` (`<objective on="talk" target="npc.oskar">`) is judged only by a raise for that target, written `<occasion>@<target>` — `--occasion talk@npc.oskar`, or `occasions: [talk@npc.oskar]` in the mock. A bare `talk`, or `talk@npc.maud`, leaves it alone, and the walk says which raise it was waiting for. The tower's hound quest from [Playing a story](/tooling/play/#deadlines-and-targeted-objectives), with the kill mocked:

```console
$ lute trace quests/hound.lute --project . --accept houndHunt --fact "slew(hound)" --occasion talk@npc.maud
trace: quests/hound.lute  (seeds: 0 paths, 1 facts; 0 selections)
note: objective `houndHunt.report` is judged at occasion `talk` for `npc.oskar`, which this walk never raised (supply `--occasion talk@npc.oskar` or `occasions: [talk@npc.oskar]`)
  <quest houndHunt>   -> active (forced)
  <objective collar>   -> done (holds(slew(hound)))
trace complete: 2 decisions
$ lute trace quests/hound.lute --project . --accept houndHunt --fact "slew(hound)" --occasion talk@npc.oskar
trace: quests/hound.lute  (seeds: 0 paths, 1 facts; 0 selections)
  <quest houndHunt>   -> active (forced)
  <objective collar>   -> done (holds(slew(hound)))
  <objective report>   -> done (holds(slew(hound)))
  <quest houndHunt>   -> complete
    grant houndHunt  EMBERS 50 (credits user.embers = 50)
trace complete: 4 decisions
```

**Deadlines.** An objective's `by=` is judged in every settle after its `done`; the first time it holds while the objective is not done, the objective is `failed` — naming the deadline — and never judged again, and a required one fails its quest. Without the kill, on floor four:

```console
$ lute trace quests/hound.lute --project . --accept houndHunt --state run.floor=4
trace: quests/hound.lute  (seeds: 1 paths, 0 facts; 0 selections)
  <quest houndHunt>   -> active (forced)
  <objective collar>   -> pending (holds(slew(hound)))
  <objective collar>   -> failed (run.floor >= 4)
  <quest houndHunt>   -> failed (run.floor >= 4)
  <on questFailed>   -> fires
    @oskar  Floor four already? Then it's gone to ground.
trace complete: 5 decisions
```

**The previous run.** [`prev.run.<path>`](/state/state-model/#the-previous-run) is a mockable state path like any other — `--state prev.run.floor=3`, or `state: { prev.run.floor: 3 }` in the mock — typed like its run path. Unmocked it is `unset`, as before the first run ends, so a scene that guards it takes its `unset` route:

```console
$ lute trace scenes/start-recap.lute --project . --state prev.run.floor=3
trace: scenes/start-recap.lute  (seeds: 1 paths, 0 facts; 0 selections)
  ## The foot of the stair
  <match isSet(prev.run.floor)>   -> arm 1 ((isSet(prev.run.floor)))
    @maud  Floor 3 last time. Beat it.
trace complete: 1 decision; arms 1/2 (isSet(prev.run.floor) @11:1)
```

## Rewards and objective bodies

A quest walk settles the way [`lute play`](/tooling/play/) settles it, so a trace, a test, and a play agree on what a completion leaves behind.

**Credited rewards.** When a `<reward>`'s kind declares [`credits: <path>`](/plugins/manifests/#rewards-that-credit-state), a grant with a scalar amount adds it to that path, by the same rule as `::set <path> += <n>`. The grant line ends with the credit — `grant houndHunt  EMBERS 50 (credits user.embers = 50)` above, since the tower's `EMBERS` kind credits `user.embers` — and in `--json` the `grant` step carries `"credited": { "path": "user.embers", "value": "50" }`. The walk ends with the credited value, so a test's `expect.state` can assert `user.embers: 50`. A range amount is the engine's roll and credits nothing, and a path with no value to add to stays `unknown`.

**Objective bodies.** An `<objective>` with a body plays it once, when the objective first turns done: its lines, `::set`, `::assert` and `::retract` follow the objective's `done` decision and its own grants in the transcript, and `--choose` decides a `<branch>` inside it.

**Quest structure** (dsl 0.24.0 §2). `--accept` (and `accepts:`) takes an `activate="accept"` child quest, which activates only while its parent is active. When a `complete="any"` parent completes, its other active children fail, and the decision names why — `superseded from quest.<parent>` (a child of a failed parent still reads `cascade from quest.<parent>`). A river crossing whose `road` completes on either `parley` or `toll`:

```console
$ lute trace quests/road.lute --project . --accept parley --accept toll --state run.talked=true
trace: quests/road.lute  (seeds: 1 paths, 0 facts; 0 selections)
  <quest road>   -> active (true)
  <quest parley>   -> active (forced)
  <quest toll>   -> active (forced)
  <objective words>   -> pending (quest.parley.state == 'complete')
  <objective silver>   -> pending (quest.toll.state == 'complete')
  <objective terms>   -> done (run.talked)
  <quest parley>   -> complete
  <objective pay>   -> pending (run.paid)
  <objective words>   -> done (quest.parley.state == 'complete')
  <objective silver>   -> pending (quest.toll.state == 'complete')
  <quest road>   -> complete
  <quest toll>   -> failed (superseded from quest.road)
trace complete: 12 decisions
```

A raise for a target also runs the `<on event="E" target="…">` handlers for that target: `--occasion talk@npc.maud` (or `occasions: [talk@npc.maud]`) fires `<on event="talk" target="npc.maud">`, and a bare `talk` or another target does not. An objective's `until=` (dsl 0.24.0 §2.1) is judged only at such a raise, while `by=` is judged in every settle. A scene's `::accept{quest="…" at="nextRun"}` renders `quest relic accepted (queued: applies after the next run start)`, and its JSON step is `{"kind": "accept", "quest": "relic", "nextRun": true}` — the acceptance applies after the next `newRun`, which only [`lute play`](/tooling/play/#quest-structure) models.

**A handler's accept** (dsl 0.26.0 §7). An `::accept{quest="…"}` in a quest's `<on>` handler takes up a quest of the same document exactly as `lute play` does: the handler's `quest second accepted` line is followed by `<quest second> -> active`, and the accepted quest's objectives are judged in the same walk, so a test can assert `expect: { quests: { second: active } }`. Before 0.26.0 trace printed the accept and left the quest `unset`, and a test and a play of the same document disagreed.

## Bundle beats

A lore document's [bundle beats](/tooling/play/#bundle-beats) (dsl 0.23.0) have no sequence to walk, so trace presents one at a time: `--beat <id>`, by its local id or its canonical `<document id>.<beat id>`. Its body is walked like a scene's — `--choose` decides its branches and hubs — every effect applies, and its `when` is shown, not enforced (JSON: a first step `{"kind": "beat", "id": …, "eligible": …}`). Oskar's hunt:

```console
$ lute trace lore/oskar.lute --project . --beat hunt
trace: lore/oskar.lute  (seeds: 0 paths, 0 facts; 0 selections)
  <beat oskar.hunt>
    @oskar  The hound took my dog's collar. Bring it back before you pass floor four.
    quest houndHunt accepted
trace complete: 0 decisions
```

A beat's own `after="…"` (dsl 0.25.0 §3) is shown the same way, over the mock's `visited:` and quest states, and never enforced: ``<beat keeper.greeting>   (not eligible: `after` prerequisite not satisfied)`` heads the walk when it does not hold.

A beat or entry that targets a whole kind (`target="kind:trainer"`, dsl 0.26.0 §5; see [Kind targets](/tooling/play/#kind-targets)) reads the raised member as `occasion.target`. Seed it like any state path — `--state occasion.target=r16Gus`, or `state: { occasion.target: r16Gus }` in a mock or test — typed by the kind, so a non-member is `E-TRACE-MOCK-TYPE`. Where the text interpolates it, trace and test print the member the way `lute play` does: its cast `name:` when the member is a cast id (`@narrator  Hiker Brom squares up.`), else the id. Without the seed the text keeps `{{occasion.target}}`. `--entry` also takes an entry's `<document id>.<entry id>` (`--entry lore.tomas.tomasOil`).

A lore document needs `--entry` or `--beat`, and without either the usage error (exit **2**) lists both kinds of id. A `--beat` that names no beat of the document is `E-TRACE-BEAT` (exit **1**), listing the ones it declares:

<!-- lute-diagnostics -->
```console
$ lute trace lore/oskar.lute --project . --beat hnut
lore/oskar.lute:0:0: error [E-TRACE-BEAT] `--beat hnut` names an unknown beat id `hnut`; this document declares: oskar.hunt, oskar.rumor (dsl 0.23.0 §4)
```

`--beat` on a document with no `<beat>` — a scene, say — is `E-TRACE-BEAT` too. [`lute run --beat`](/tooling/cli/#run) presents the same beat from the compiled lore artifact.

A scenario test presents the same beat with `beat: <id>` (bare or canonical) instead of `entry:`. Trace shows a beat's `when` without enforcing it, but since dsl 0.26.0 `lute test` enforces it: a test that presents an entry or beat its mocks make ineligible fails, naming the premise that is false, unless it asserts `expect.eligible` — see [`lute test`](/tooling/cli/#test). `--entry` with a beat's id is refused as `E-TRACE-ENTRY`; after the document's entries its message adds ``— `hunt` is a `<beat>`: present it with `--beat hunt` ``, and in a test it names the `beat:` key instead. For an id that is neither, the message lists the beats (`--beat`) after the entries.

An `eligible:` expectation answers that failure, and with it asserted an ineligible body is not walked, so an `eligible: false` test needs no bridge answers for a body that never plays. A map key may name an entry or bundle beat of the file that the test did not present: it is judged alone, under the same mocks, so a lore test may carry a map-form `eligible:` without presenting anything. Two more test expectations read a walk's structure: `expect.accepts: [quest ids]` asserts the quests the scene's `::accept`s took, as a set (`accepts: expected [toll], got [parley]`), and `expect.offered` of a `<hub>` is every choice eligible at any of its visits, unioned, as for a branch (it was always `[]`). See [`lute test`](/tooling/cli/#test).

## Reading the transcript

The human form is an indented, ordered transcript: emitted content lines (interpolations substituted where decided, kept verbatim `{{…}}` where unknown), staging directives, state writes, and one line per **decision** — the construct, the winning arm/choice, and the guard with its read values. A trailing summary reports decisions taken, arm/choice coverage, and any unresolved atoms.

```console
$ lute trace docs/examples/choice-persist.lute --choose sofaHelp=help
trace: docs/examples/choice-persist.lute  (seeds: 0 paths, 0 facts; 1 selection)
  ## Recording the Choice
    @narrator  Elena had slipped on the wet step. You could help — or walk on.
  <branch sofaHelp>   eligible: help, warmly, tip   -> help
    @elena  Thank you. I won't forget this.
    ::set  run.metHelpfully = true  (into sugar)
  ## Reading It Back
  <match run.metHelpfully>   -> arm 1 (run.metHelpfully == true)
    @elena  You helped me back then. I've been meaning to thank you again.
trace complete: 2 decisions; choices 1/3 (sofaHelp), arms 1/2 (run.metHelpfully @45:1)
```

An `unknown` guard halts the walk at that construct (exit 3) and reports the unresolved atoms — which paths or facts a mock would need. Trace never guesses past unknown eligibility; forcing past an unknown guard via `--choose` is the documented escape hatch. A forced choice still counts: the summary reads `1 unresolved (forced past an unknown guard — the walk continued, exit unchanged)` and names the atoms that would decide it, and `--json` lists it under `forcedUnknown`. The exit code stays what the rest of the walk earned. Reserved quest reads (`quest.<id>.state`, `…objectives.<oid>.done`, and since 0.24.0 `quest.<id>.failedBy` and `…objectives.<oid>.failed`) resolve to their defaults (`unset` / `false`) unless mocked (`quests:` or `--state`); without a project each carries an "existence unverified" note, and with one trace checks the id against the project's quests.

**Def references read as authored** (dsl 0.24.0). A `<match on="@def">` header, an arm or choice guard, and the coverage summary print a def reference the way the author wrote it; `--expand` prints the expansion the walk evaluated:

```console
$ lute trace week.lute --state user.runs=3 --choose ask=old
trace: week.lute  (seeds: 1 paths, 0 facts; 1 selection)
  ## Morning
  <match @weekday>   -> arm 1 (is="mon")
    @narrator  Monday again.
  <branch ask>   eligible: old, new   -> old (@atLeast(3))
    @narrator  You remember.
trace complete: 2 decisions; choices 1/2 (ask), arms 1/2 (@weekday @14:1)
$ lute trace week.lute --state user.runs=3 --choose ask=old --expand
trace: week.lute  (seeds: 1 paths, 0 facts; 1 selection)
  ## Morning
  <match (run.day == 1 ? 'mon' : run.day == 2 ? 'tue' : 'other')>   -> arm 1 (is="mon")
    @narrator  Monday again.
  <branch ask>   eligible: old, new   -> old ((user.runs >= 3))
    @narrator  You remember.
trace complete: 2 decisions; choices 1/2 (ask), arms 1/2 ((run.day == 1 ? 'mon' : run.day == 2 ? 'tue' : 'other') @14:1)
```

`--json` always carries the expansion in `id`, `guard` and a coverage entry's `label`, and adds the author's text — only where an expansion changed it — as `authoredId`, `authoredGuard` and `authoredLabel`.

**A beat's own `when`.** Tracing a [beat scene](/language/beats/) walks its body whether or not its frontmatter `when:` holds — trace is a preview. When the mocks make that `when` false or undecided, the trace opens with a note — ``beat `when` (run.day == 3) is false under these mocks — the `visit` selector would never present this scene; the walk below shows it as if it had been presented``. `lute test` does not pass such a walk: since dsl 0.26.0 a test that presents a beat its mocks make ineligible — by its `when`, its `after:`, or a spent `once: user` — fails unless it asserts `expect.eligible` (see [`lute test`](/tooling/cli/#test)).

A scene's [`::accept{quest="<id>"}`](/language/directives/#accept--taking-up-a-quest) renders as its own transcript line, `quest <id> accepted` (JSON step `{"kind": "accept", "quest": "<id>"}`). Trace walks one document, so the accept is recorded, not applied: the quest's own document is where its lifecycle runs (`--accept` / `accepts:` there, or `lute play` across the project). An `::accept` in a quest's own `<on>` handler is applied — see [A handler's accept](#rewards-and-objective-bodies).

```console
$ lute trace scenes/shed.lute --project . --choose offer=take
trace: scenes/shed.lute  (seeds: 0 paths, 0 facts; 1 selection)
  ## Shed
    @vesna  Somebody has to mind the shed.
  <branch offer>   eligible: take, leave   -> take
    quest sideJob accepted
    @vesna  Good. It's yours.
trace complete: 1 decision; choices 1/2 (offer)
```

### Following a taken `::next`

A `::next{to="<label>"}` the walk takes jumps to its `::mark`, exactly as `lute run` and `lute play` jump (dsl 0.26.0 §7): the transcript shows `<next -> <label>>` (JSON step `{"kind": "jump", "to": "<label>"}`) and the walk goes on from the mark, so `trace complete` — and a test's `exit: complete` — means the walk reached the end of the document. Before 0.26.0 the walk ended at a taken jump and still reported complete, so a test could pass on a transcript that play never shows. A vault whose door choice jumps past the guard:

```console
$ lute trace scenes/vault.lute --project . --choose enter=yes
trace: scenes/vault.lute  (seeds: 0 paths, 0 facts; 1 selection)
  ## The door
    @narrator  The vault door stands open.
  <branch enter>   eligible: yes, no   -> yes
    <next -> hall>
  ## The hall
    <mark>
    @narrator  The treasure hall.
    ::set  run.gold = 10
trace complete: 1 decision; choices 1/2 (enter)
```

A test of the same walk asserts what play sees — `transcriptContains: ["@narrator: The treasure hall."]`, `transcriptLacks: ["@narrator: A guard waves you back."]`, `state: { run.gold: 10 }` — and passes.

## Bridge answers

A plugin directive that calls a host service writes its result into `scene.*` slots through `bridgeResult` effects. Trace calls no service: since 0.24.0 an unanswered call leaves those slots **unknown** — never the state shape's default — so a guard over one halts the walk incomplete with a hint naming the answer to give. In the town gate from [Playing a story](/tooling/play/#answering-bridge-calls), `::check` writes `scene.check.<key>.passed` and `.margin`:

```console
$ lute trace scenes/gate/guards.lute --project .
trace: scenes/gate/guards.lute  (seeds: 0 paths, 0 facts; 0 selections)
  ## The gate
    <check>
      (bridge unanswered: no `bridges.check` answer — its results read unknown)
trace incomplete: 1 unresolved atom (exit 3)
  unresolved: match `is="true"` (scene.check.guards.passed match) — supply bridges: { check: [ { passed: <bool>, margin: <number> } ] } (plugin `check` call unanswered; `scene.check.guards.passed` reads its `passed` result) as a mock; arms 0/2 (scene.check.guards.passed @12:1)
```

The hint is an answer the loader accepts once its placeholders are filled: it lists every field of the call's result that content reads, each with its type — `<bool>`, `<number>`, `<string>`, `<one of: a|b>` for an enum, `<value>` otherwise. A mock's (or a `*.test.yaml`'s) `bridges:` answers the calls in order, one answer per call of the tag, each giving the result fields content reads:

```yaml
# mocks/gate.yaml
file: ../scenes/gate/guards.lute
bridges:
  check:
    - { passed: true, margin: 3 }
    - { passed: false, margin: -2 }
```

```console
$ lute trace scenes/gate/guards.lute --project . --mock mocks/gate.yaml
trace: scenes/gate/guards.lute  (seeds: 0 paths, 0 facts; 0 selections)
  ## The gate
    <check>
      (bridge answered: passed=true, margin=3)
  <match scene.check.guards.passed>   -> arm 1 (is="true")
    @narrator  The guards wave you through.
  <match scene.check.guards.margin > 5>   -> otherwise
    <check>
      (bridge answered: passed=false, margin=-2)
  <match scene.check.sneak.passed>   -> arm 2 (is="false")
    @narrator  A stallholder shouts after you.
trace complete: 3 decisions; arms 1/2 (scene.check.guards.passed @12:1), arms 1/2 (scene.check.guards.margin > 5 @15:5), arms 1/2 (scene.check.sneak.passed @22:1)
```

**A field nothing reads may be left out** (dsl 0.25.0 §7). A result field that no guard, line or write in the project reads decides nothing, so an answer may omit it: its result slot simply stays unresolved. Here a line reads `scene.check.guards.margin`, so `margin` is required. Without that line, `- { passed: true }` is a complete answer, and the hint on an unanswered call lists only what content reads: `supply bridges: { check: [ { passed: <bool> } ] }`. An answer may still give an unread field the call's effects write.

A tag no plugin call of the document reads a bridge result through, or a field no effect reads, is `E-TRACE-MOCK-UNDECLARED`; an answer that lacks a field content reads, or a value that does not fit a result slot, is `E-TRACE-MOCK-TYPE` — a missing field's message spells the typed answer, and a bad value is checked against every slot the tag's calls write. Each is anchored at the offending tag key, answer or field key **in the mock file** (a `*.test.yaml`, a `--mock` file, or `mocks/*.yaml`), not at the document, and the JSON diagnostic carries `"provenance": "mock"`:

<!-- lute-diagnostics -->
```console
$ lute trace scenes/gate/guards.lute --project . --mock lack.yaml
lack.yaml:4:7: error [E-TRACE-MOCK-TYPE] `bridges.check` answer 1 lacks `margin`, which content reads — an answer gives every bridge result `::check` content reads: `{ passed: <bool>, margin: <number> }` (dsl 0.25.0 §7)
trace refused: scenes/gate/guards.lute — invalid mock input
```

<!-- lute-diagnostics -->
```console
$ lute trace scenes/gate/guards.lute --project . --mock bad.yaml
bad.yaml:4:9: error [E-TRACE-MOCK-TYPE] `bridges.check` answer 1: `passed: yes` is not compatible with `scene.check.guards.passed`'s declared type (dsl 0.24.0 §5)
bad.yaml:4:9: error [E-TRACE-MOCK-TYPE] `bridges.check` answer 1: `passed: yes` is not compatible with `scene.check.sneak.passed`'s declared type (dsl 0.24.0 §5)
trace refused: scenes/gate/guards.lute — invalid mock input
```

`lack.yaml` gives `- { passed: true }` and `bad.yaml` `- { passed: yes, margin: 3 }`, each the fourth line of the file. Before 0.24 a missing field was `E-TRACE-MOCK-UNDECLARED`, and every one of these errors was reported at `<document>:0:0`. In a scenario test the same error lands on the test's own line — `./tests/t.test.yaml:6:9: error [E-TRACE-MOCK-TYPE] …` under its `FAIL` — and `check-project` reports a bad `mocks/*.yaml` at `./mocks/lack.yaml:4:7`.

**Migrating a 0.23.1 mock.** A mock or test that answered a call by seeding its result slot — `state: { scene.check.guards.passed: true }` — no longer decides the guard: the call's answer is read from `bridges:` only, so the walk stops unresolved with the hint above. Replace the seed with `bridges: { check: [ { passed: true, margin: 3 } ] }`, one answer per call, in call order, with every field content reads.

`check-project` runs the same checks over `mocks/*.yaml`. A scenario test without the answers fails as incomplete, its hint ending `in this test`; `lute run --mock` answers calls from the same key, and [`lute play`](/tooling/play/#answering-bridge-calls) takes it at the top level and per step.

## Derivation

**Facts are closed; rules derive** (dsl 0.22.0 §6). Trace loads the schema's seed `facts:`, adds your `--fact` / `facts:` mocks and every `::assert` / `::retract` the walk makes, and applies the project's Datalog rules — with stratified negation — over them. It is the same evaluator `lute run` and `lute play` use, so trace, test, and play cannot disagree about what a project's rules conclude. A base fact that is neither seeded, mocked, nor asserted is false; a derived fact holds exactly when a rule concludes it. So a guard, `done`, or `start` on a derived relation decides without mocking the conclusion, and "derived false because a negated premise holds" is testable:

```lute check
---
kind: scene
id: case.accuse
entities:
  person: { members: [ann, bob] }
relations:
  suspect: { args: [person] }
  alibi: { args: [person] }
  culprit: { args: [person], derive: true }
facts:
  - "suspect(ann)"
rules:
  - "culprit(P) :- suspect(P), not alibi(P)"
---

## The parlour

@inspector: Everyone stays in this room.

<branch id="ask">
  <choice id="maid" label="Ask the maid first">
    @maid: Miss Ann? She was at the harbour all evening.
    ::assert{ alibi(ann) }
  </choice>
  <choice id="skip" label="Ask no one">
    @inspector: No more questions.
  </choice>
</branch>

<branch id="verdict">
  <choice id="ann" label="Name Ann" when="holds(culprit(ann))">
    @inspector: It was Ann. No one can place her anywhere else.
  </choice>
  <choice id="wait" label="Keep looking">
    @inspector: Then we start again.
  </choice>
</branch>
```

With no mock at all, the seeded `suspect(ann)` and the absent `alibi(ann)` derive `culprit(ann)`:

```console
$ lute trace accuse.lute --choose ask=skip --choose verdict=ann
trace: accuse.lute  (seeds: 0 paths, 0 facts; 2 selections)
  ## The parlour
    @inspector  Everyone stays in this room.
  <branch ask>   eligible: maid, skip   -> skip
    @inspector  No more questions.
  <branch verdict>   eligible: ann, wait   -> ann (holds(culprit(ann)))
    @inspector  It was Ann. No one can place her anywhere else.
trace complete: 2 decisions; choices 1/2 (ask), choices 1/2 (verdict)
```

Once the maid's `::assert{ alibi(ann) }` runs, the negated premise holds and `culprit(ann)` is definitely false — `verdict` offers only `wait`, and forcing `--choose verdict=ann` there is refused (`E-TRACE-CHOICE`, exit 1), not forced past an unknown:

```console
$ lute trace accuse.lute --choose ask=maid --choose verdict=wait
trace: accuse.lute  (seeds: 0 paths, 0 facts; 2 selections)
  ## The parlour
    @inspector  Everyone stays in this room.
  <branch ask>   eligible: maid, skip   -> maid
    @maid  Miss Ann? She was at the harbour all evening.
    ::assert  alibi(ann)
  <branch verdict>   eligible: wait   -> wait
    @inspector  Then we start again.
trace complete: 2 decisions; choices 1/2 (ask), choices 1/2 (verdict)
```

A mocked derived atom is still accepted: it is a seed like any other, so `--fact "culprit(ann)"` holds whatever the rules conclude. [`lute test`](/tooling/cli/#test) walks the same way, and a test whose walk halts fails unless it declares `expect: { exit: incomplete }`. A test asserts what the rules conclude with `expect.facts` and `expect.notFacts` — atoms that must hold, or must not, after derivation when the walk ends:

```yaml
file: accuse.lute
choose: { ask: maid, verdict: wait }
expect:
  facts: ["alibi(ann)"]
  notFacts: ["culprit(ann)"]
```

A miss names both sides — `notFacts culprit(ann): expected does not hold, got holds` — and an atom whose derivation read undecided state is `unknown`, which satisfies neither list.

**A rule guard over undecided state.** A rule may test state (`cel("…")`). When the mocks leave that state undecided, the rule decides nothing: its conclusion is unknown, and the report names the state path that would decide it rather than the derived atom:

```lute check
---
kind: scene
id: case.arrest
state:
  run.day: { type: number }
entities:
  person: { members: [ann] }
relations:
  suspect: { args: [person] }
  warrant: { args: [person], derive: true }
facts:
  - "suspect(ann)"
rules:
  - 'warrant(P) :- suspect(P), cel("run.day >= 3")'
---

## The station

<branch id="arrest">
  <choice id="now" label="Arrest Ann" when="holds(warrant(ann))">
    @inspector: The warrant came through. Bring her in.
  </choice>
  <choice id="later" label="Wait for the warrant">
    @inspector: Not yet.
  </choice>
</branch>
```

```console
$ lute trace arrest.lute --choose arrest=now
trace: arrest.lute  (seeds: 0 paths, 0 facts; 1 selection)
  ## The station
  <branch arrest>   eligible: later   -> now (holds(warrant(ann))) (forced)
    @inspector  The warrant came through. Bring her in.
trace complete: 1 decision; 1 unresolved (forced past an unknown guard — the walk continued, exit unchanged); choices 1/2 (arrest)
  unresolved (forced): branch `arrest -> now` choice guard `holds(warrant(ann))` was unknown — supply --state run.day=<value> as a mock to decide it
```

With `--state run.day=3` the rule concludes `warrant(ann)` and `now` is eligible outright.

**`derive: false` restores 0.21.** The mock key `derive: false` (also accepted in a `*.test.yaml`), or the `--no-derive` flag on `lute trace` and `lute test` — the flag wins over the key — brings back the lookup-only model: the seed facts are not loaded, the rules are not applied, and a fact of a derived relation that you did not mock is **unknown**, so a guard on it halts the walk (exit 3) unless forced. A note says so when the schema declares seed facts you did not supply, and another names each derived relation the walk read. (`lute play` takes the same key and flag; there the seed facts still load and only the rules stop — see [Playing a story](/tooling/play/#derivation-and---explain).)

```console
$ lute trace accuse.lute --choose ask=skip --choose verdict=ann --no-derive
trace: accuse.lute  (seeds: 0 paths, 0 facts; 2 selections)
note: the schema declares seed facts (e.g. `suspect`) but under `derive: false` trace does not auto-load them (§3.1, the explicit-world model) — supply seeded relations explicitly via --fact
note: derived relation `culprit` read under `derive: false`: its rules were not applied, so an unmocked `culprit(…)` is unknown — supply it via --fact, or drop `derive: false`
  ## The parlour
    @inspector  Everyone stays in this room.
  <branch ask>   eligible: maid, skip   -> skip
    @inspector  No more questions.
  <branch verdict>   eligible: wait   -> ann (holds(culprit(ann))) (forced)
    @inspector  It was Ann. No one can place her anywhere else.
trace complete: 2 decisions; 1 unresolved (forced past an unknown guard — the walk continued, exit unchanged); choices 1/2 (ask), choices 1/2 (verdict)
  unresolved (forced): branch `verdict -> ann` choice guard `holds(culprit(ann))` was unknown — supply --fact "culprit(ann)" as a mock to decide it
```

**Migrating from 0.21.** A test that relied on an unmocked derived atom being unknown (exit 3), or on a seeded relation reading empty, now sees the derived or seeded answer and may change verdict. Pin `derive: false` in that test or mock to keep the old one — or, better, assert what the rules conclude with `expect.facts` / `expect.notFacts`. To walk everything the old way at once, run `lute test --no-derive`, which overrides every test's and play script's own `derive:`.

## Exclusive relations

Relations declared [`excludes:`](/state/facts-and-datalog/#exclusive-relations-excludes) (dsl 0.25.0 §1) never hold together on the same arguments. Trace checks the facts — mocked, seeded, asserted and derived — after every write, and a write that makes two exclusive facts hold stops the walk there. Seed `panicked(maren)` and walk the dawn scene, which asserts `calm(maren)`:

<!-- lute-diagnostics unverified="verbatim lute trace output; E-FACT-EXCLUSIVE is named through a constant in the trace crate, so the scraper cannot pair quote and code" -->
```console
$ lute trace scenes/dawn.lute --project . --fact "panicked(maren)" --choose look=nothing
trace: scenes/dawn.lute  (seeds: 0 paths, 1 facts; 1 selection)
  ## Shot 1.
  <branch look>   eligible: saw, nothing   -> nothing
    @narrator  Nobody answers.
  <match holds(seenAfter(elias)) && !holds(fell(elias))>   -> otherwise
    ::assert  calm(maren)
    ✗ exclusive: calm(maren) and panicked(maren) both hold
trace stopped at the `✗ exclusive` line above (exit 1); choices 1/2 (look), arms 1/2 (holds(seenAfter(elias)) && !holds(fell(elias)) @21:1)
scenes/dawn.lute:22:1: error [E-FACT-EXCLUSIVE] this write makes exclusive relations hold together: calm(maren) and panicked(maren) both hold (dsl 0.25.0 §1)
trace refused: scenes/dawn.lute — exclusive relations hold together (dsl 0.25.0 §1)
```

The exit is **1**, and nothing after the write is walked. A scenario test that reaches such a write fails the same way. When the seeded facts themselves — the mock's `facts:` or `--fact`, the project's `facts:` seeds, and what the rules derive from them — already break an exclusion, trace refuses before the walk starts, with an `E-FACT-EXCLUSIVE` that says so. `check-project` reports the same code statically when the other fact holds on every route to the `::assert`; trace catches the cases that are only possible, such as a fact asserted down one branch of an earlier scene, or supplied by a mock.

## The `E-TRACE-*` refusals

Before walking, trace resolves the document exactly as `check` does and **refuses** (exit 1) a document with check errors or invalid mocks — run `check` first. The mock refusals: `E-TRACE-MOCK-PARSE` (a malformed mock file — including a `quests:` status outside the four, an `entriesRead:` that is not `{ run: [...], user: [...] }`, a `derive:` that is not `true`/`false`, or a `bridges:` that is not a map of tag → answers), `E-TRACE-MOCK-UNDECLARED` (an undeclared `--state` path, a `quests:`/`entriesRead:` path the document neither reads nor declares, or a `bridges:` tag or field no plugin call reads), `E-TRACE-MOCK-TYPE` (wrong literal type, in a seed or a bridge answer, or a bridge answer lacking a field its call reads), `E-TRACE-MOCK-FACT` (unknown relation/arity/foreign arg), `E-TRACE-CHOICE` (unknown or ineligible forced choice), `E-TRACE-ENTRY` (an `--entry` on a document that is not lore, or naming none of its entries; the message lists the document's entries, then its beats — or, for a beat's id, says to present it with `--beat`), `E-TRACE-BEAT` (a `--beat` naming no bundle beat of the document, dsl 0.23.0), `E-TRACE-EVENT` (a built-in lifecycle event `questActive`/`questComplete`/`questFailed` — engine-derived, never fired by hand), and `E-TRACE-ACCEPT` (an unknown quest id, or one that carries a `start` predicate and needs no accept). An unmatched `--event` is an informational note, not a refusal.

Since 0.6.1, trace also emits a warning (not a refusal) — `W-TRACE-MOCK-UNPRODUCIBLE` — for a `--fact`/mock-YAML fact whose relation no authored producer can ever assert (`producible()` judges it not producible): the supplied answer can never arise in reachable play, so a "complete" walk seeded with it proves nothing. A `reserved: true` or `open: engine`-argument relation is producible by definition and never warns.

The judgement covers the whole project when trace can see one — `--project <dir>`, or a `lute.project.yaml` above the traced file: a relation is producible when a `facts:` seed, or an `::assert` in any document `check-project` does not prove unreachable, can produce it; a derived relation is judged through its rules. With no project the warning judges the traced document alone and says so (`judged against this document only; pass --project <dir> to count the asserts of the project's other documents`), so a clue asserted in a sibling scene is only "not producible" there.
