# Investigation RPG — a worked whodunit

A small, self-contained case that wires together the features you reach for in
an investigation game: **relational facts + Datalog** (clues implicate
suspects), **`after:` scene sequencing** (crime scene → interview →
confrontation), a **`<hub>`** interrogation, a **`<branch>` accusation** with
success/failure endings, and a **quest** whose objectives are satisfied by the
scenes. It also shows what `check-project` proves about relational fact
queries across scenes — which is why two guards you might expect to see here
are not.

Every command below is copy-paste runnable from the **repository root**.

## Layout

| Path | Role |
|---|---|
| `lute.project.yaml` | project root — core-only profile (no plugins) |
| `world.schema.yaml` | shared run/user scalar state **and** the relational world (entities `suspect`/`clue`, relations `foundClue`/`implicates`/`points`, seed `facts:`, and one Datalog `rules:` clause) |
| `scenes/crime-scene.lute` | entry scene (no `after:`) — logs clues with `::assert{ foundClue(...) }`, which the derived relation `points` reads |
| `scenes/interview.lute` | `after:` the crime scene — a `<hub>` interrogation pressing the logged clues, and a `<match on="run.suspectFocus">` over run state |
| `scenes/confrontation.lute` | `after:` the interview — a `<branch>` accusation; complementary `when=` verdict lines branch to the success/failure endings |
| `quests/identify-killer.lute` | the goal machine — objectives whose `done=` predicates the scenes satisfy |
| `mocks/accuse-correctly.yaml` | trace mock: accuse the right suspect → success ending |
| `mocks/accuse-wrongly.yaml` | trace mock: accuse the wrong suspect → failure ending |

## 1. Check the whole project

```sh
cargo run -q -p lute-cli -- check-project docs/examples/investigation
```

Exit `0`, with no warnings:

```
ok: docs/examples/investigation/quests/identify-killer.lute (0 warning(s))
ok: docs/examples/investigation/scenes/confrontation.lute (0 warning(s))
ok: docs/examples/investigation/scenes/crime-scene.lute (0 warning(s))
ok: docs/examples/investigation/scenes/interview.lute (0 warning(s))
ok: docs/examples/investigation (4 file(s), 0 project-wide warning(s))
```

**What the checker proved.** `check-project` decides every relational fact
query (`holds(…)`, `count(…)`) in every guard: *impossible* (nothing can ever
produce the fact), *guaranteed* (it holds on every declared route to the
guard), or *possible*. The crime scene asserts `foundClue(ledger)` and
`foundClue(letter)` on its only route, and the interview is sequenced `after:`
it, so both facts hold on entry to the interview. Give `pressLedger` the guard
`when="holds(foundClue(ledger))"` and `pressLetter` a guard on a clue nobody
logs, `when="holds(foundClue(knife))"`, and `check-project` reports the first
as redundant (it can never close) and the second as dead (it can never open):

<!-- lute-diagnostics -->
```
docs/examples/investigation/scenes/interview.lute:31:67: warning [W-FACT-GUARANTEED] guard `holds(foundClue(ledger))` is redundant: `foundClue(ledger)` is asserted on every route to here (docs/examples/investigation/scenes/crime-scene.lute:28) (dsl 0.20.0 §5)
```

<!-- lute-diagnostics unverified="the relational E-ARM-DEAD message is composed in crates/lute-check/src/fact_check.rs, which names the code through the reachability::E_ARM_DEAD constant rather than a string literal, so the scraper cannot pair quote and code; copied verbatim from check-project output" -->
```
docs/examples/investigation/scenes/interview.lute:36:3: error [E-ARM-DEAD] choice can never fire: guard `holds(foundClue(knife))` is provably false — no seed, assert, rule, or engine relation produces `foundClue(knife)` under your declared routes (dsl 0.20.0 §5)
```

That is why `pressLedger` / `pressLetter` carry no `when=`, and why the crime
scene's last line reads the derived `points(blake)` without one. The quest's
optional `clinchMotive` objective, `done="holds(implicates(ledger, blake))"`,
is decided the same way: the fact is a schema seed nothing retracts, so the
objective is satisfiable; had no seed, assert, or rule produced it, the
objective would be `E-OBJECTIVE-UNSATISFIABLE`. (A `done` is a predicate, not a
guard, so an always-true one is never reported as redundant.) The analysis
needs every document at once, so it runs in `check-project` only; a
single-file `lute check` leaves relational queries undecided.

## 2. Reachability & the scene graph

`lute scenario` reports pure graph structure over the declared `after:` routes —
no CEL is evaluated, no Datalog is run.

```sh
cargo run -q -p lute-cli -- scenario docs/examples/investigation
```

shows the reachability chain as topological layers:

```
    layer 0: scene(detective.s01ep01)   # crime scene (root)
    layer 1: scene(detective.s01ep02)   # interview  (after crime scene)
    layer 2: scene(detective.s01ep03)   # confrontation (after interview)
```

Ask about one node's reachability and its declared prerequisite structure:

```sh
cargo run -q -p lute-cli -- scenario docs/examples/investigation reach detective.s01ep03
```

```
  verdict: Reachable — a satisfiable route exists under your declared routes.
  after: visited("detective.s01ep02")
```

Note the hedge — *"under your declared routes."* Reachability is conservative:
it reasons about the `after:` graph you declared, not about whether any given
playthrough actually walks it.

## 3. Trace both endings

`lute trace` walks **one** document along **one** deterministic, mock-driven
path. It is a preview, **not** a proof of all paths. Both mocks drive the
confrontation scene; they seed the truth (`run.trueKiller`) and force the
`accuse` branch to a different choice.

Accuse the **right** suspect → the success ending:

```sh
cargo run -q -p lute-cli -- trace docs/examples/investigation/scenes/confrontation.lute \
  --mock docs/examples/investigation/mocks/accuse-correctly.yaml
```

```
  <branch accuse>   ... -> accuseBlake
    ::set  run.accused = blake  (into sugar)
  <match run.accused == run.trueKiller>   -> arm 1
    @narrator  The cuffs close on the right wrists. Case closed.
    @detective  Booked. The file can finally rest.
trace complete: ...
```

Accuse the **wrong** suspect → the failure ending:

```sh
cargo run -q -p lute-cli -- trace docs/examples/investigation/scenes/confrontation.lute \
  --mock docs/examples/investigation/mocks/accuse-wrongly.yaml
```

```
  <branch accuse>   ... -> accuseCass
    ::set  run.accused = cass  (into sugar)
  <match run.accused != run.trueKiller>   -> arm 1
    @narrator  The wrong suspect walks free. Somewhere, the real one exhales.
    @detective  I got it wrong. The file stays open.
trace complete: ...
```

Both traces exit `0` (a complete walk) and reach visibly **different** endings —
the same document, two forced choices.

> Trace prints an informational note that it does **not** auto-load the schema's
> seed `facts:` (the explicit-world model, §3.1). These endings turn only on
> scalar run state, so no `--fact` seeds are needed here; a trace that gated on a
> fact query would supply it with `--fact "implicates(ledger, blake)"`.

## 4. Compile

Once a document checks clean it compiles to its JSON command-record artifact:

```sh
cargo run -q -p lute-cli -- compile docs/examples/investigation/scenes/crime-scene.lute \
  --project docs/examples/investigation -o /tmp/crime-scene.json
```

Exit `0`; the artifact is stamped `"lute": "0.22.0"` / `"irVersion": "0.22.0"`.
Every document in the project compiles (`scenes/*.lute` and
`quests/identify-killer.lute`) — swap the path above.

## 5. Scenario tests (`lute test`)

`lute trace` is a manual preview; `lute test` turns those mock-driven
playthroughs into repeatable assertions. Each `*.test.yaml` under `tests/`
names a document (`file:`, resolved relative to the test file), carries the
same mock surfaces as `lute trace --mock`
(`state:`/`facts:`/`choose:`/`events:`/`accepts:`, and since dsl 0.21.0
`visited:`/`occasions:`), and declares an `expect:` block — `exit:`,
`transcriptContains:`, `state:`, and, for a quest document, `quests:`
(`{questId: unset | active | complete | failed}`):

```yaml
file: ../scenes/confrontation.lute
state:                       # mock seed — identical to `lute trace --mock`
  run.trueKiller: blake
choose:
  accuse: accuseBlake
expect:
  exit: complete             # complete | incomplete
  transcriptContains:        # substrings that MUST appear in the transcript
    - "The cuffs close on the right wrists. Case closed."
  state:                     # the trace's FINAL written state
    run.accused: blake
```

Run the whole suite from the repository root:

```sh
cargo run -q -p lute-cli -- test docs/examples/investigation
```

```
PASS  .../tests/accuse-correctly.test.yaml  (.../scenes/confrontation.lute)
PASS  .../tests/accuse-wrongly.test.yaml  (.../scenes/confrontation.lute)
PASS  .../tests/interview-press-ledger.test.yaml  (.../scenes/interview.lute)

3 passed, 0 failed
```

Exit `0` when every test passes, `1` when any expectation fails (the miss is
reported as `expected … got …`), `2` on an I/O error or a malformed test
file. Add `--json` for a machine report, and `--coverage` for an honest
chosen-vs-never-chosen / executed-vs-unexecuted roll-up **over the traced
paths only** — never a whole-space coverage claim (D1: trace explains, it
never proves):

```sh
cargo run -q -p lute-cli -- test docs/examples/investigation --coverage
```

```
coverage over 3 traced path(s):
  branch/hub accuse: 2/3 chosen [accuseBlake, accuseCass]; never chosen [accuseDana]
  branch/hub interrogate: 2/4 chosen [leave, pressLedger]; 2 never seen eligible in any traced path
  match `run.suspectFocus`: 1/3 arm(s) executed [arm 1]; 2 unexecuted
```
