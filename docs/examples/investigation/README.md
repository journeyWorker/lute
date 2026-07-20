# Investigation RPG — a worked whodunit

A small, self-contained case that wires together the features you reach for in
an investigation game: **relational facts + Datalog** (clues implicate
suspects), **`after:` scene sequencing** (crime scene → interview →
confrontation), a **fact-guarded `<hub>`** interrogation, a **`<branch>`
accusation** with success/failure endings, and a **quest** whose objectives are
satisfied by the scenes. It also deliberately surfaces one of Lute's
**honest-analysis boundaries** (`W-UNPROVEN-RELATIONAL`) so you can see what the
checker will and will not claim.

Every command below is copy-paste runnable from the **repository root**.

## Layout

| Path | Role |
|---|---|
| `lute.project.yaml` | project root — core-only profile (no plugins) |
| `world.schema.yaml` | shared run/user scalar state **and** the relational world (entities `suspect`/`clue`, relations `foundClue`/`implicates`/`points`, seed `facts:`, and one Datalog `rules:` clause) |
| `scenes/crime-scene.lute` | entry scene (no `after:`) — logs clues with `::assert{ foundClue(...) }`; reads the derived relation `points` in a `when=` guard |
| `scenes/interview.lute` | `after:` the crime scene — a `<hub>` interrogation with **fact-guarded** choices (`when="holds(foundClue(...))"`) and a `<match on="run.suspectFocus">` over run state |
| `scenes/confrontation.lute` | `after:` the interview — a `<branch>` accusation; complementary `when=` verdict lines branch to the success/failure endings |
| `quests/identify-killer.lute` | the goal machine — objectives whose `done=` predicates the scenes satisfy |
| `mocks/accuse-correctly.yaml` | trace mock: accuse the right suspect → success ending |
| `mocks/accuse-wrongly.yaml` | trace mock: accuse the wrong suspect → failure ending |

## 1. Check the whole project

```sh
cargo run -q -p lute-cli -- check-project docs/examples/investigation
```

Exit `0`. Every file checks clean except for **one project-wide warning** on the
quest:

```
docs/examples/investigation/quests/identify-killer.lute:
  26:3: warning [W-UNPROVEN-RELATIONAL] `done="holds(implicates(ledger, blake))"` is gated by a
  relational fact query over producible relation(s) `implicates`; static reachability analysis
  (dsl 0.6.1 §2) neither proves nor refutes it. Verify with `lute trace` seeds or human review
ok: docs/examples/investigation (4 file(s), 1 project-wide warning(s))
```

**This warning is a feature, not a defect.** The `clinchMotive` objective gates on
a relational fact *query* (`holds(implicates(ledger, blake))`). The checker can
prove the `implicates` relation is *producible* (it is seeded in
`world.schema.yaml`), but it will **not** claim the specific ground query is
true — that is a runtime question. Rather than silently assert proof it does not
have, Lute names the exact boundary and points you at `lute trace` seeds or human
review. (`W-UNPROVEN-RELATIONAL` is a warning and never flips the exit code; you
can promote it with `--deny W-UNPROVEN-RELATIONAL` if your project wants it to.)

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

Exit `0`; the artifact is stamped `"lute": "0.6.1"` / `"irVersion": "0.6.1"`.
Every document in the project compiles (`scenes/*.lute` and
`quests/identify-killer.lute`) — swap the path above.
