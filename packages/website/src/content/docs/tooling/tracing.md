---
title: Tracing guide
description: Preview a scene before you ship it — seeding state, facts, choices, events, accepts, the visited set, and raised occasions via flags or a mock YAML file, reading the decision transcript, and the E-TRACE-* refusals.
---

`lute trace` walks a document once, deterministically, against **author-supplied mocks**, reporting every decision and why. It is an authoring preview, not a guarantee: it never runs the Datalog fixpoint, never feeds `check`/`compile`, and is never a static reachability proof. It explores only the mock scenarios you supply — a coverage aid, never a proof.

## Seeding the world

Trace operates on an **explicit** world — the effective set is exactly what you supply, never the schema's own seed block. Five surfaces feed the walk, as repeatable flags:

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

- `--occasion <name>` — raise an occasion after the walk settles, in CLI order (repeatable, after the mock's own `occasions:`). Each raise judges the `<objective on="<name>">` objectives of every active quest; an `on` objective is **never** judged otherwise.
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

An `unknown` guard halts the walk at that construct (exit 3) and reports the unresolved atoms — which paths or facts a mock would need. Trace never guesses past unknown eligibility; forcing past an unknown guard via `--choose` is the documented escape hatch. A forced choice still counts: the summary reads `1 unresolved (forced past an unknown guard — the walk continued, exit unchanged)` and names the atoms that would decide it, and `--json` lists it under `forcedUnknown`. The exit code stays what the rest of the walk earned. Reserved quest reads (`quest.<id>.state`, `…objectives.<oid>.done`) resolve to their defaults (`unset` / `false`) unless mocked, each carrying an "existence unverified" note (only `check-project` validates a foreign quest id).

**Facts are closed; derived facts are unknown.** Trace does not run the Datalog rules. A base fact you did not supply is simply false. A fact of a `derive: true` relation that you did not supply is **unknown**, even when you supplied every base fact its rule needs, so a guard on it halts the walk (exit 3):

```console
$ lute trace d.lute --fact "clue(ann)"
…
  <match holds(clue(ann))>   -> arm 1 ((holds(clue(ann))))
    @ann  base.
trace incomplete: 1 unresolved atom (exit 3)
  unresolved: match `(holds(guilty(ann)))` (holds(guilty(ann)) match) — supply --fact "guilty(ann)" as a mock; …
```

To trace what follows from a conclusion, mock the conclusion itself (`--fact "guilty(ann)"`). There is no mock for "this derived fact is false": leave the guard unresolved, or exercise the rule through `lute run` or `lute play`, which apply the seeds and the rules. [`lute test`](/tooling/cli/#test) walks the same way, and a test whose walk halts fails unless it declares `expect: { exit: incomplete }`.

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

## The `E-TRACE-*` refusals

Before walking, trace resolves the document exactly as `check` does and **refuses** (exit 1) a document with check errors or invalid mocks — run `check` first. The mock refusals: `E-TRACE-MOCK-UNDECLARED` (undeclared `--state` path), `E-TRACE-MOCK-TYPE` (wrong literal type), `E-TRACE-MOCK-FACT` (unknown relation/arity/foreign arg), `E-TRACE-CHOICE` (unknown or ineligible forced choice), `E-TRACE-EVENT` (a built-in lifecycle event `questActive`/`questComplete`/`questFailed` — engine-derived, never fired by hand), and `E-TRACE-ACCEPT` (an unknown quest id, or one that carries a `start` predicate and needs no accept). An unmatched `--event` is an informational note, not a refusal.

Since 0.6.1, trace also emits a warning (not a refusal) — `W-TRACE-MOCK-UNPRODUCIBLE` — for a `--fact`/mock-YAML fact whose relation no authored producer can ever assert (`producible()` judges it not producible): the supplied answer can never arise in reachable play, so a "complete" walk seeded with it proves nothing. A `reserved: true` or `open: engine`-argument relation is producible by definition and never warns.

The judgement covers the whole project when trace can see one — `--project <dir>`, or a `lute.project.yaml` above the traced file: a relation is producible when a `facts:` seed, or an `::assert` in any document `check-project` does not prove unreachable, can produce it; a derived relation is judged through its rules. With no project the warning judges the traced document alone and says so (`judged against this document only; pass --project <dir> to count the asserts of the project's other documents`), so a clue asserted in a sibling scene is only "not producible" there.
