# Anseo Prologue Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Author a showcase-scale Lute example — eleven scenes and six quests aboard a generation ship shedding modules — exercising `::end`, `identity:`, and the relational layer, which the current corpus barely covers.

**Architecture:** A project root at `docs/examples/anseo/`, scaffolded by `lute init`, then filled in. Scenes are ordered by `after:` routes; quests gate on relational queries; scenario tests pin the endings.

**Tech Stack:** Lute 0.9.0 (`./target/debug/lute`). No Rust changes. Content is `.lute` and YAML only.

## Provenance of this plan

Every syntax form below was executed against the real binary before being written down. The first draft of this plan was **wrong in ten places** because it was designed from adjacent-language intuition and only validated afterward. The corrections are in Task 10's findings note. Two consequences for you:

1. **Do not substitute forms from memory**, including from other examples — several corrections came from forms that are correct in one position and wrong in another (see the `exit:` rule below).
2. When something does not check, **read the source or the proposal, not another example**. `docs/proposals/scenario-dsl/*.md` is normative; the examples are not.

## Verified facts this plan depends on

| Fact | Verified how |
|---|---|
| `identity:` is a `lute.project.yaml` key. In scene frontmatter it is `E-META-UNKNOWN-KEY`. | `lute check` |
| Identity tokens are exactly `{prefix}`, `{speaker}`, `{code}`; `prefix` = `{character}.s{season}ep{episode}` | compiled `lineId` = `anseo.s01ep01.vesna_0010` |
| `kind: quest` frontmatter takes `kind`/`luteVersion`/`uses`/`title` only — `character`/`season`/`episode` are `E-META-UNKNOWN-KEY` | `lute check` |
| A rule's head relation must be declared `derive: true`, no `tier:`; otherwise `E-RELATION-UNKNOWN` | `lute check-project` |
| `rules:` is a YAML list of quoted strings; non-derived relations carry `tier:` | `lute check-project` |
| **`exit: true` is emitted on the `::auto` sprite record ONLY.** An `action=` on a content line lowers to `line.action` and never carries `exit`. | `lute compile` + `lower.rs:178-185` |
| A non-exit `action=` on `::auto` emits no `exit` field at all | `lute compile`, control run |
| `::end{reason=…}` emits `{"kind":"end","reason":…}`; content after it is `W-CODE-AFTER-END` | `lute compile`, `lute check` |
| A component body admits a **param-scoped** `<match on="@param">` (dsl 0.4.0 §6.2) | `lute check-project` through `::use` |
| `<branch>`/`<hub>` in a component body is `E-COMPONENT-BODY` on **both** legs — standalone and through a `::use` | `crates/lute-check/tests/component_logic_block.rs`; the standalone leg was a false green until commit `3ff3543` |
| `<timeline>` is `<timeline duration="1.2">` wrapping `<track subject= property=>` — a sub-second choreography unit, NOT a countdown | `docs/examples/property-tracks.lute:27-36` |
| `lute scenario` prints topological layers and a prerequisite→dependent edge list | `lute scenario` |
| `lute init` scaffolds exactly this layout, plus `mocks/playthrough.yaml` and a README | `lute init` |

## Global Constraints

- Tags are line-oriented: opener, children on their own lines, closer. A single-line `<tag>body</tag>` is `E-TAG-INLINE-BODY`.
- Every content-line vocabulary attribute must be a declared member, or `E-DOMAIN-UNKNOWN`.
- `lute check-project docs/examples` must exit 0 at the end of every task.
- No Rust source changes. NEVER run repo-wide `cargo fmt`.
- Branch `example/anseo-prologue` is checked out. Commit there.

---

### Task 1: Scaffold and declare

**Files:**
- Create (via `lute init`): `docs/examples/anseo/{lute.project.yaml,vocabulary.schema.yaml,world.schema.yaml,scenes/opening.lute,mocks/playthrough.yaml,README.md}`
- Modify: the three YAML files and delete the placeholder scene

**Interfaces:**
- Produces: the vocabulary every scene imports; the state/relations every quest imports; the `identity:` templates fixing every `lineId`.

- [ ] **Step 1: Scaffold with the tool, not by hand**

```bash
./target/debug/lute init docs/examples/anseo
```

Dogfooding the scaffolder in the flagship example is the point; it also gets the `mocks/` layout right for free.

- [ ] **Step 2: Set the identity templates in `lute.project.yaml`**

```yaml
identity:
  lineId: "{prefix}.{speaker}_{code}"
  voiceKey: "{speaker}-{code}"
```

Only `{prefix}`/`{speaker}`/`{code}` exist. Any other token is rejected at project load.

- [ ] **Step 3: Replace `vocabulary.schema.yaml`**

```yaml
enums:
  emotion: [level, clipped, frayed, hollowed, wry, stricken]
  action:
    members: [brace, drift, turn-away, seal, unseal, step-out, go-under]
    exits: [step-out, go-under]
  anchor: { members: [port, center, starboard], default: center }
  mood: [quiet, pressurized, failing, weightless]
  volume: [silent, muted, normal, raised, alarm]
  musicAction: [start, swell, cut, resume, fade-out]
  vfxType: [shed, klaxon, pressure-drop, frost]
```

- [ ] **Step 4: Replace `world.schema.yaml`**

```yaml
state:
  run.vesnaTrust:    { type: number, default: 0 }
  run.shedPressure:  { type: number, default: 0 }
entities:
  crew:  { members: [vesna, toma, ilsabet, ottavio] }
  topic: { members: [shed_sequence, true_heading, manifest] }
relations:
  awake:    { args: [crew], tier: run }
  knows:    { args: [crew, topic], tier: run }
  found:    { args: [crew], tier: run }
  can_halt: { args: [crew], derive: true }
rules:
  - "can_halt(C) :- awake(C), knows(C, shed_sequence)"
```

- [ ] **Step 5: Replace the placeholder scene with `scenes/wake.lute`**

```lute
---
kind: scene
character: anseo
season: 1
episode: 1
uses: [../vocabulary.schema.yaml]
---

## Cold Wake
::auto{character="vesna" anchor="port" action="brace"}
@vesna{code="0010" emotion="clipped"}: Cryo's gone. We don't go back under.
@vesna{code="0020" emotion="level"}: So we walk.
```

Delete `scenes/opening.lute`.

- [ ] **Step 6: Verify**

```bash
./target/debug/lute check-project docs/examples/anseo   # ok, 1 file
./target/debug/lute compile docs/examples/anseo/scenes/wake.lute -o /tmp/t1.json
grep -o '"lineId": *"[^"]*"' /tmp/t1.json | head -2
```
Expected: `anseo.s01ep01.vesna_0010`, `anseo.s01ep01.vesna_0020`

- [ ] **Step 7: Verify the whole tree, then commit**

```bash
./target/debug/lute check-project docs/examples
git add docs/examples/anseo
git commit -m "feat(example): Anseo project root, vocabulary, and world schema"
```

---

### Task 2: The exits proof

**Files:**
- Modify: `docs/examples/anseo/scenes/wake.lute`

**Interfaces:**
- Consumes: the `action` domain and its `exits:` list from Task 1.
- Produces: the corpus's demonstration that a declared exit member sets `exit: true`.

**Read this before writing anything.** `exit: true` is emitted by the `"auto"` lowering arm (`lower.rs:178-185`) onto the **sprite** record. Putting `action="go-under"` on a `@vesna{…}` content line produces `{"kind":"line", …, "action":"go-under"}` with **no `exit` field**. The first draft of this plan got this backwards and its verification step would have failed. The exit must be staged with `::auto`.

- [ ] **Step 1: Stage the departure**

Append to the shot:

```lute
@vesna{code="0030" emotion="hollowed"}: If the second pod's intact, I'm taking it.
::auto{character="vesna" action="go-under"}
```

- [ ] **Step 2: Prove it**

```bash
./target/debug/lute compile docs/examples/anseo/scenes/wake.lute -o /tmp/t2.json
python3 -c "
import json;d=json.load(open('/tmp/t2.json'))
print([c for c in d['commands'] if c.get('exit')])"
```
Expected: exactly one sprite record with `'exit': True`.

- [ ] **Step 3: Prove the negative**

Change `action="go-under"` to `action="drift"` on that `::auto`, recompile, and confirm **no** command carries `exit`. Restore `go-under`.

This is the demonstration the example exists to carry: `go-under` means "returns to cryo", which no naming heuristic recovers — `drift` and `go-under` are equally opaque strings. The compiler knows only because `vocabulary.schema.yaml` declares one of them in `exits:`.

- [ ] **Step 4: Commit**

```bash
git commit -am "feat(example): a cryo return that only declared exits can explain"
```

---

### Task 3: The shed clock as declared state

**Files:**
- Create: `docs/examples/anseo/scenes/cryobank.lute`

**Interfaces:**
- Consumes: `run.shedPressure`, `awake`, `knows` from Task 1.
- Produces: `anseo.s01ep02`, the route ancestor of everything downstream.

- [ ] **Step 1: Write the scene**

```lute
---
kind: scene
character: anseo
season: 1
episode: 2
uses: [../vocabulary.schema.yaml, ../world.schema.yaml]
after: 'visited("anseo.s01ep01")'
---

## The Cryobank
::auto{character="vesna" anchor="port" action="drift"}
@vesna{code="0010" emotion="clipped"}: Every pod you crack, the Purser reads as load.
@purser{code="0020" emotion="level" os}: Allocation notes the draw. The schedule advances.

<branch id="whoWakes">
<choice id="wakeToma" label="Wake the engineer">
::set{run.shedPressure += 2}
::assert{awake(toma)}
::assert{knows(toma, shed_sequence)}
@toma{code="0030" emotion="frayed"}: How long have I been under?
</choice>
<choice id="wakeIlsabet" label="Wake the navigator">
::set{run.shedPressure += 1}
::assert{awake(ilsabet)}
::assert{knows(ilsabet, true_heading)}
@ilsabet{code="0040" emotion="stricken"}: We're not going where you think we are.
</choice>
<choice id="wakeNobody" label="Leave them under">
@vesna{code="0050" emotion="wry"}: Cheapest crew is the one still asleep.
</choice>
</branch>
```

- [ ] **Step 2: Verify**

```bash
./target/debug/lute check-project docs/examples/anseo   # ok, 2 files
```

- [ ] **Step 3: Confirm the clock is authored, not a timer**

Compile and confirm the increments appear as state-write commands. There is no engine clock: the schedule advances only because an author wrote `::set`. That is the design claim this scene carries.

- [ ] **Step 4: Commit**

```bash
git add docs/examples/anseo/scenes/cryobank.lute
git commit -m "feat(example): the cryobank, where waking crew costs clock"
```

---

### Task 4: The relational quest gate

**Files:**
- Create: `docs/examples/anseo/quests/hold-the-spine.lute`

- [ ] **Step 1: Write it**

```lute
---
kind: quest
luteVersion: "0.9.0"
uses: ../world.schema.yaml
title: Hold the Spine
---

<quest id="holdTheSpine" title="Hold the Spine" start="holds(can_halt(toma))">
<objective id="reachToma" title="Reach the spine coupling" done="run.shedPressure >= 1"/>
<on event="questComplete">
::set{run.vesnaTrust += 1}
@narrator: The shed halted, one module short of the infirmary.
</on>
</quest>
```

No `character`/`season`/`episode` — scene-only keys, `E-META-UNKNOWN-KEY` here.

- [ ] **Step 2: Verify**

```bash
./target/debug/lute check-project docs/examples/anseo   # ok, 3 files
```

- [ ] **Step 3: Prove the gate is typed**

Change `holds(can_halt(toma))` to `holds(can_halt(nobody))` and confirm it FAILS — `nobody` is not a `crew` member. Then try `holds(can_halt(vesna))` and observe it PASSES: the checker validates the query's shape and its argument's domain membership, not its runtime truth. Restore `toma`.

A typo in a gate is a check-time error. That is what a closed entity domain buys, and it is the reason to declare `crew` rather than use bare strings.

- [ ] **Step 4: Commit**

```bash
git add docs/examples/anseo/quests
git commit -m "feat(example): gate a quest on a derived relation, not a flag"
```

---

### Task 5: The terminators

**Files:**
- Create: `docs/examples/anseo/scenes/bridge.lute` (episode 10)
- Create: `docs/examples/anseo/scenes/shed.lute` (episode 11)

**Interfaces:**
- Produces: the corpus's first two `::end` uses, with distinct reasons. Both `after:` routes are provisional and get repointed in Task 8.

- [ ] **Step 1: The success terminal**

```lute
---
kind: scene
character: anseo
season: 1
episode: 10
uses: [../vocabulary.schema.yaml, ../world.schema.yaml]
after: 'visited("anseo.s01ep02")'
---

## The Bridge
::auto{character="vesna" anchor="center" action="brace"}
@vesna{code="0010" emotion="level"}: Whatever's left of the ship, it's steering.
::end{reason="bridge-reached"}
```

- [ ] **Step 2: The failure terminal**

```lute
---
kind: scene
character: anseo
season: 1
episode: 11
uses: [../vocabulary.schema.yaml, ../world.schema.yaml]
after: 'visited("anseo.s01ep02")'
---

## Shed
@purser{code="0010" emotion="level" os}: Module released. Allocation is satisfied.
::end{reason="shed-with-module"}
```

- [ ] **Step 3: Verify and probe**

```bash
./target/debug/lute check-project docs/examples/anseo   # ok, 5 files
```
Add a content line after `::end` in `shed.lute`, re-check, confirm `W-CODE-AFTER-END`, remove it.

```bash
./target/debug/lute compile docs/examples/anseo/scenes/bridge.lute -o /tmp/t5.json
grep -o '"reason": *"bridge-reached"' /tmp/t5.json
```
Note: the artifact also carries an injected `reason` on the auto-preload sprite (`entry-emotion-lookahead` provenance). Match the `end` record specifically, not the first `reason` in the file.

- [ ] **Step 4: Commit**

```bash
git add docs/examples/anseo/scenes
git commit -m "feat(example): two terminals, two end reasons"
```

---

### Task 6: The Purser component

**Files:**
- Create: `docs/examples/anseo/components/purser-interject.component.lute`
- Modify: `docs/examples/anseo/scenes/cryobank.lute`

**Interfaces:**
- Produces: a reusable interjection later scenes invoke with `::use`.

A component body is presentational: content lines, staging directives, `@param` refs, `::use`, and — per dsl 0.4.0 §6.2 — a **param-scoped** `<match on="@param">`. `<branch>`/`<hub>` are forbidden on principle (presenting a menu writes `scene.choices.*`), and `::set`/`::assert`/`::retract`/`<timeline>`/`<on>`/`<objective>` are `E-COMPONENT-BODY`.

- [ ] **Step 1: Write the component**

```lute
---
component: purserInterject
params:
  pressure: string
uses: ../vocabulary.schema.yaml
---

## Interjection
<match on="@pressure">
<when is="rising">
@purser{code="0020" emotion="level" os}: Draw exceeds projection. The schedule advances.
</when>
<otherwise>
@purser{code="0010" emotion="level" os}: Allocation is nominal.
</otherwise>
</match>
```

The `uses:` makes a standalone check resolve `emotion=`; through `::use` the **importing** document's vocabulary is what applies (0.9.0 §5, known limitation), so both sides declare it.

- [ ] **Step 2: Invoke it**

Add `components: [../components/purser-interject.component.lute]` to `cryobank.lute`'s frontmatter and replace its inline Purser line with:

```lute
::use{component="purserInterject" pressure="rising"}
```

- [ ] **Step 3: Verify**

```bash
./target/debug/lute check-project docs/examples/anseo   # ok, 6 files
```

Then confirm the contract both ways: temporarily add a `<branch>` to the component body and check the component file **alone** — it must report `E-COMPONENT-BODY`, and so must `cryobank.lute`. Remove the branch. Until `3ff3543` the standalone leg reported `ok` here; if you see `ok`, you are on a stale binary — rebuild before trusting anything else in this plan.

- [ ] **Step 4: Commit**

```bash
git add docs/examples/anseo/components docs/examples/anseo/scenes/cryobank.lute
git commit -m "feat(example): the Purser speaks through a component"
```

---

### Task 7: The branch scenes

**Files:**
- Create: `scenes/spine-a.lute` (ep3), `scenes/hydroponics.lute` (ep4), `scenes/machine-deck.lute` (ep5), `scenes/stowaway.lute` (ep6)

This is a writing task: the plan fixes each scene's structural contract; the dialogue is the deliverable produced while executing it.

- **spine-a** (after ep02) — the first shed, on screen. The example's only `<timeline>`. Real shape:

```lute
<timeline duration="1.2">
  <track subject="camera">
    ::camera{shake="0.4" duration="0.3" at="0.7"}
  </track>
  <track subject="vesna" property="pos">
    ::auto{character="vesna" anchor="port" action="brace"}
  </track>
</timeline>
```
  Sub-second offsets. It is choreography, not the countdown.
- **hydroponics** (after ep03) — Vesna. `::set{run.vesnaTrust += 1}` on the honest branch, so Task 9's `what-vesna-carries` is reachable.
- **machine-deck** (after ep03) — if `holds(awake(toma))` the coupling is saved; else `::set{run.shedPressure += 1}`.
- **stowaway** (after ep04 or ep05) — Ottavio. `::assert{found(ottavio)}`.

- [ ] **Step 1: Write the four scenes**
- [ ] **Step 2: Verify** — `./target/debug/lute check-project docs/examples/anseo` → ok, 10 files
- [ ] **Step 3: Commit** — `git commit -m "feat(example): the branch scenes and the first shed"`

---

### Task 8: The convergence scenes

**Files:**
- Create: `scenes/spine-b.lute` (ep7), `scenes/archive.lute` (ep8), `scenes/purser.lute` (ep9)
- Modify: `scenes/bridge.lute`, `scenes/shed.lute` (repoint `after:`)

- **spine-b** (after ep06) — the second shed; branches converge.
- **archive** (after ep07) — `::assert{knows(vesna, manifest)}`.
- **purser** (after ep07 or ep08) — what can be said depends on who is awake, via `when=` guards over the relational facts.

- [ ] **Step 1: Write the three scenes**

- [ ] **Step 2: Repoint the terminals**

`bridge.lute` → `after: 'visited("anseo.s01ep09")'`; `shed.lute` → `after: 'visited("anseo.s01ep07")'`.

- [ ] **Step 3: Verify the graph**

```bash
./target/debug/lute check-project docs/examples/anseo   # ok, 13 files
./target/debug/lute scenario docs/examples/anseo
```
`scenario` prints topological layers and a prerequisite→dependent edge list. Expected: eleven scenes across multiple layers with `wake` alone in layer 0, and both terminals present as dependents. **If every scene sits in layer 0, the `after:` routes are not wired** — that was the shape of the empty two-scene run during planning. Fix the routes, not the expectation.

- [ ] **Step 4: Commit** — record the `scenario` output in the commit body.

---

### Task 9: The remaining five quests, and the tests

**Files:**
- Create: `quests/{unmoored,who-wakes,false-heading,manifest-gap,what-vesna-carries}.lute`
- Create: `tests/reach-bridge.test.yaml`, `tests/shed-with-module.test.yaml`

Gates, all verified forms:

| Quest | `start=` |
|---|---|
| `unmoored` | `"true"` |
| `who-wakes` | `"true"` |
| `false-heading` | `holds(awake(ilsabet))` |
| `manifest-gap` | `holds(found(ottavio))` |
| `what-vesna-carries` | `run.vesnaTrust >= 2` |

The first draft of this plan **omitted tests entirely**, though `lute test` exists and `docs/examples/investigation/tests/` carries three. An example with two endings and no test pinning either is not a finished example.

- [ ] **Step 1: Write the five quests** — Task 4's frontmatter shape.

- [ ] **Step 2: Write two scenario tests**, one per ending. Verified shape:

```yaml
file: ../scenes/bridge.lute
state:
  run.shedPressure: 1
choose:
  whoWakes: wakeToma
expect:
  exit: complete
  transcriptContains:
    - "Whatever's left of the ship, it's steering."
  state:
    run.vesnaTrust: 1
```

- [ ] **Step 3: Verify**

```bash
./target/debug/lute check-project docs/examples/anseo   # ok, 18 files
./target/debug/lute test docs/examples/anseo
```
Both tests must pass. A test that cannot be made to pass means the route graph or a gate is wrong — fix the content.

- [ ] **Step 4: Commit**

---

### Task 10: README, findings, and the gates

**Files:**
- Modify: `docs/examples/anseo/README.md` (replace the `lute init` stub)
- Create: `docs/superpowers/notes/2026-07-31-anseo-authoring-findings.md`

- [ ] **Step 1: Confirm CI coverage rather than assuming it**

```bash
python3 scripts/check-docs-consistency.py
python3 scripts/check-doc-snippets.py
```
Both print the roots they cover. If `docs/examples/anseo` is not covered by the existing `docs/examples` root, register it. Confirm; do not assume.

- [ ] **Step 2: Write the README** — what the example demonstrates, and specifically that it is the corpus's first coverage of `::end`, `identity:`, and derived relations.

- [ ] **Step 3: Write the findings note**

A first-class deliverable. Seed it with the ten errors found while validating this plan, all real, all caught before content was written:

1. `<timeline>` is sub-second choreography, not a countdown
2. `<on>` is a quest lifecycle hook, not a reactive trigger
3. `identity:` is a project key, not scene frontmatter
4. Identity tokens are only `{prefix}`/`{speaker}`/`{code}`
5. Quests reject `character`/`season`/`episode`
6. A rule head must be declared `derive: true`
7. `rules:` is a list of quoted strings; relations need `tier:`
8. Quests use `start=`/`fail=` expressions, not conditional existence
9. A param-scoped `<match>` in a component **is** legal (0.4.0 §6.2 relaxed the 0.1 ban)
10. **`exit: true` is emitted on the `::auto` sprite record only**, never from a content line's `action=`

Three tooling defects surfaced while validating this plan. **All three were fixed before authoring began** (commit `3ff3543`) — a broken checker would have shaped the example around itself:

- **`lute check <component>.lute` was a false green.** `E-COMPONENT-BODY` is enforced in `walk_component_body`, reached only from `validate_components` over an importing document. A component file carries no `kind:`, so it degraded to `DocKind::Scene` and walked through `Walker`, where `<branch>`/`<hub>`/`::set`/… are all legal. A component containing `<branch>` checked `ok` standalone and failed at every call site. The component root now routes through the same `walk_component_body` the `::use` leg uses.
- **`greet.component.lute` and `stinger.component.lute` documented a dropped rule.** Both headers stated dsl §13.4's blanket ban on logic blocks; 0.4.0 §6.2 admits a param-scoped `<match>`, which `reaction.component.lute` relies on. Reading the examples taught the wrong rule.
- **`stinger.component.lute`'s stale standalone claim.** Its header said a standalone check reports `E-META-MISSING`; it reports `E-DOMAIN-UNKNOWN`, because that file declares no `uses:` of its own and so has no vocabulary to resolve against on the standalone leg (0.9.0 §5).

The note should still record all three, with the fix commit — the value is the *class*: two of the three were documentation asserting a contract the code had already stopped enforcing or had never enforced. Re-derive from the source, not from a neighbouring example.

State the meta-finding plainly: ten of ten design assumptions were wrong before checking. The language's shapes are not guessable from adjacent knowledge, and the two forms that differ only by *position* (`action=` on `::auto` versus on a content line) are the sharpest edge found.

- [ ] **Step 4: Full verification**

```bash
./target/debug/lute check-project docs/examples
./target/debug/lute test docs/examples/anseo
cargo test --workspace --no-fail-fast
python3 scripts/check-docs-consistency.py
python3 scripts/check-doc-snippets.py
```

- [ ] **Step 5: Commit**
