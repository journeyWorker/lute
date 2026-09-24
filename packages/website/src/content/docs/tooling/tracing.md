---
title: Tracing guide
description: Preview a scene before you ship it — seeding state, facts, choices, events, accepts, the visited set, a save's quest status and entry reads, raised occasions (for a target, too) and the previous run via flags or a mock YAML file, presenting one bundle beat, how the project's rules derive over them, reading the decision transcript, and the E-TRACE-* refusals.
---

`lute trace` walks a document once, deterministically, against **author-supplied mocks**, reporting every decision and why. It is an authoring preview, not a guarantee: it never feeds `check`/`compile`, and is never a static reachability proof. It explores only the mock scenarios you supply — a coverage aid, never a proof. Since 0.22.0 it applies the project's seed facts and Datalog rules over those mocks by default, exactly as `lute run` and `lute play` do — see [Derivation](#derivation).

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
- `--accept <questId>` — simulate accepting a `start`-less (accept-driven) quest.

Two more surfaces feed quest and scene conditions (dsl 0.21.0):

- `--occasion <name>` — raise an occasion after the walk settles, in CLI order (repeatable, after the mock's own `occasions:`). Each raise judges the `<objective on="<name>">` objectives of every active quest; an `on` objective is **never** judged otherwise. `--occasion <name>@<target>` (dsl 0.23.0) raises it for a target — see [Targets, deadlines, and the previous run](#targets-deadlines-and-the-previous-run).
- `visited: [<scene id>…]` (mock file only) — the scenes already presented, read by `visited('<scene id>')` in any condition. The set is closed: a scene you do not list is not visited, so a `visited(…)` read is always `true` or `false`, never unresolved.

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

Two notes point at an occasion mismatch without refusing the walk: an objective whose occasion the walk never raised (``objective `q.o` is judged at occasion `runEnd`, which this walk never raised (supply `--occasion runEnd` or `occasions: [runEnd]`)``), and a raised occasion no `<objective on>` in the document judges (``occasion `dayEnd` is judged by no `<objective on>` in this document``).

### A save's history

Two more mock keys (dsl 0.22.0, mock file only) seed what a player's save already holds — the same keys a [play script starts from](/tooling/play/#starting-from-a-save):

- `quests: { <questId>: unset | active | complete | failed }` — a quest's lifecycle status, exactly a `quest.<id>.state` state seed.
- `entriesRead: { run: [<entry id>…], user: [<entry id>…] }` — `run` seeds `entry.<id>.read: true` (read this run), `user` seeds `entry.<id>.everRead: true` (read in some run). In a mock neither implies the other: a new run clears `read` and keeps `everRead`.

Both are spellings of reserved state paths, so they follow those paths' mock rules: the traced document must read (or declare) the path. Seeding a quest or an entry the document never reads is `E-TRACE-MOCK-UNDECLARED`, and a status outside the four is `E-TRACE-MOCK-PARSE`. In the [`beats` scaffold](/tooling/play/#worked-example), Tomas's oil entry is guarded on the lamp quest being active:

```yaml
# mocks/oil.yaml
quests: { lampOut: active }
```

```console
$ lute trace lore/tomas.lute --project . --entry tomasOil --mock mocks/oil.yaml
trace: lore/tomas.lute  (seeds: 1 paths, 0 facts; 0 selections)
note: quest `lampOut`'s existence is unverified by trace (run `check-project` to confirm it is defined by a project quest, dsl 0.5.1 §1.3/§1.4)
  <entry tomasOil>   (first read)
    @tomas  Oil? Top shelf. Tell Mara it's the wick, not the oil.
    ::assert  knows(lamp)
trace complete: 0 decisions
```

Without the mock the entry reads ``(first read, not eligible (`when` is false))``, and the same `quests:` against a scene that never reads `quest.lampOut.state` is refused with `E-TRACE-MOCK-UNDECLARED`.

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
    grant houndHunt  EMBERS 50
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

A lore document needs `--entry` or `--beat`, and without either the usage error (exit **2**) lists both kinds of id. A `--beat` that names no beat of the document is `E-TRACE-BEAT` (exit **1**), listing the ones it declares:

<!-- lute-diagnostics -->
```console
$ lute trace lore/oskar.lute --project . --beat hnut
lore/oskar.lute:0:0: error [E-TRACE-BEAT] `--beat hnut` names an unknown beat id `hnut`; this document declares: oskar.hunt, oskar.rumor (dsl 0.23.0 §4)
```

`--beat` on a document with no `<beat>` — a scene, say — is `E-TRACE-BEAT` too. [`lute run --beat`](/tooling/cli/#run) presents the same beat from the compiled lore artifact.

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

An `unknown` guard halts the walk at that construct (exit 3) and reports the unresolved atoms — which paths or facts a mock would need. Trace never guesses past unknown eligibility; forcing past an unknown guard via `--choose` is the documented escape hatch. A forced choice still counts: the summary reads `1 unresolved (forced past an unknown guard — the walk continued, exit unchanged)` and names the atoms that would decide it, and `--json` lists it under `forcedUnknown`. The exit code stays what the rest of the walk earned. Reserved quest reads (`quest.<id>.state`, `…objectives.<oid>.done`) resolve to their defaults (`unset` / `false`) unless mocked (`quests:` or `--state`), each carrying an "existence unverified" note (only `check-project` validates a foreign quest id).

**A beat's own `when`.** Tracing a [beat scene](/language/beats/) walks its body whether or not its frontmatter `when:` holds. When the mocks make that `when` false or undecided, the trace opens with a note — ``beat `when` (run.day == 3) is false under these mocks — the `visit` selector would never present this scene; the walk below shows it as if it had been presented`` — and `lute test` shows the same note on the test line.

A scene's [`::accept{quest="<id>"}`](/language/directives/#accept--taking-up-a-quest) renders as its own transcript line, `quest <id> accepted` (JSON step `{"kind": "accept", "quest": "<id>"}`). Trace walks one document, so the accept is recorded, not applied: the quest's own document is where its lifecycle runs (`--accept` / `accepts:` there, or `lute play` across the project).

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

A mocked derived atom is still accepted: it is a seed like any other, so `--fact "culprit(ann)"` holds whatever the rules conclude. [`lute test`](/tooling/cli/#test) walks the same way, and a test whose walk halts fails unless it declares `expect: { exit: incomplete }`.

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

**Migrating from 0.21.** A test that relied on an unmocked derived atom being unknown (exit 3), or on a seeded relation reading empty, now sees the derived or seeded answer and may change verdict. Pin `derive: false` in that test or mock to keep the old one — or, better, assert what the rules conclude. To walk everything the old way at once, run `lute test --no-derive`, which overrides every test's and play script's own `derive:`.

## The `E-TRACE-*` refusals

Before walking, trace resolves the document exactly as `check` does and **refuses** (exit 1) a document with check errors or invalid mocks — run `check` first. The mock refusals: `E-TRACE-MOCK-PARSE` (a malformed mock file — including a `quests:` status outside the four, an `entriesRead:` that is not `{ run: [...], user: [...] }`, or a `derive:` that is not `true`/`false`), `E-TRACE-MOCK-UNDECLARED` (an undeclared `--state` path, or a `quests:`/`entriesRead:` path the document neither reads nor declares), `E-TRACE-MOCK-TYPE` (wrong literal type), `E-TRACE-MOCK-FACT` (unknown relation/arity/foreign arg), `E-TRACE-CHOICE` (unknown or ineligible forced choice), `E-TRACE-BEAT` (a `--beat` naming no bundle beat of the document, dsl 0.23.0), `E-TRACE-EVENT` (a built-in lifecycle event `questActive`/`questComplete`/`questFailed` — engine-derived, never fired by hand), and `E-TRACE-ACCEPT` (an unknown quest id, or one that carries a `start` predicate and needs no accept). An unmatched `--event` is an informational note, not a refusal.

Since 0.6.1, trace also emits a warning (not a refusal) — `W-TRACE-MOCK-UNPRODUCIBLE` — for a `--fact`/mock-YAML fact whose relation no authored producer can ever assert (`producible()` judges it not producible): the supplied answer can never arise in reachable play, so a "complete" walk seeded with it proves nothing. A `reserved: true` or `open: engine`-argument relation is producible by definition and never warns.

The judgement covers the whole project when trace can see one — `--project <dir>`, or a `lute.project.yaml` above the traced file: a relation is producible when a `facts:` seed, or an `::assert` in any document `check-project` does not prove unreachable, can produce it; a derived relation is judged through its rules. With no project the warning judges the traced document alone and says so (`judged against this document only; pass --project <dir> to count the asserts of the project's other documents`), so a clue asserted in a sibling scene is only "not producible" there.
