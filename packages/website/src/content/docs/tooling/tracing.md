---
title: Tracing guide
description: Preview a scene before you ship it — seeding state, facts, choices, events, and accepts via flags or a mock YAML file, reading the decision transcript, and the E-TRACE-* refusals.
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

The same five surfaces live in a `--mock <file.yaml>` document; CLI flags compose with it, the flag winning on a conflict.

```yaml
state:  { run.metMira: true }
facts:  ["inParty(shadowheart)"]
choose: { sofaHelp: help }
events: [npcSpoke]
accepts: [sideQuest]
```

## Reading the transcript

The human form is an indented, ordered transcript: emitted content lines (interpolations substituted where decided, kept verbatim `{{…}}` where unknown), staging directives, state writes, and one line per **decision** — the construct, the winning arm/choice, and the guard with its read values. A trailing summary reports decisions taken, arm/choice coverage, and any unresolved atoms.

```console
$ lute trace docs/examples/choice-persist.lute --choose sofaHelp=help
trace: choice-persist.lute  (seeds: 0 paths, 0 facts; 1 selection)
  ## Recording the Choice
  <branch sofaHelp>   eligible: help, warmly, tip   → help (--choose)
    ::set     run.metHelpfully = true          (into sugar)
  ## Reading It Back
  <match run.metHelpfully>   = true → arm 1 ($ == true)
trace complete: 2 decisions; choices 1/3, arms 1/2
```

An `unknown` guard halts the walk at that construct (exit 3) and reports the unresolved atoms — which paths or facts a mock would need. Trace never guesses past unknown eligibility; forcing past an unknown guard via `--choose` is the documented escape hatch. Reserved quest reads (`quest.<id>.state`, `…objectives.<oid>.done`) resolve to their defaults (`unset` / `false`) unless mocked, each carrying an "existence unverified" note (only `check-project` validates a foreign quest id).

## The `E-TRACE-*` refusals

Before walking, trace resolves the document exactly as `check` does and **refuses** (exit 1) a document with check errors or invalid mocks — run `check` first. The mock refusals: `E-TRACE-MOCK-UNDECLARED` (undeclared `--state` path), `E-TRACE-MOCK-TYPE` (wrong literal type), `E-TRACE-MOCK-FACT` (unknown relation/arity/foreign arg), `E-TRACE-CHOICE` (unknown or ineligible forced choice), `E-TRACE-EVENT` (a built-in lifecycle event `questActive`/`questComplete`/`questFailed` — engine-derived, never fired by hand), and `E-TRACE-ACCEPT` (an unknown quest id, or one that carries a `start` predicate and needs no accept). An unmatched `--event` is an informational note, not a refusal.

Since 0.6.1, trace also emits a warning (not a refusal) — `W-TRACE-MOCK-UNPRODUCIBLE` — for a `--fact`/mock-YAML fact whose relation no authored producer can ever assert (`producible()` judges it not producible): the supplied answer can never arise in reachable play, so a "complete" walk seeded with it proves nothing. A `reserved: true` or `open: engine`-argument relation is producible by definition and never warns.
