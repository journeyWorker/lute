# Anseo Prologue Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Author a showcase-scale Lute example — eleven scenes and six quests aboard a generation ship shedding modules — that exercises `::end`, `identity:`, and the relational layer, all of which the current example corpus barely covers.

**Architecture:** A new project root `docs/examples/anseo/` with its own `lute.project.yaml`, a `vocabulary.schema.yaml` carrying the 0.9.0 content vocabulary, a `world.schema.yaml` carrying state plus the entity/relation/rule layer, and scene and quest documents under `scenes/` and `quests/`. Scenes are ordered by `after:` routes; quests gate on relational queries. The root is gated by `lute check-project` like every other example root.

**Tech Stack:** Lute 0.9.0 (`./target/debug/lute`). No Rust changes. Content is `.lute` and YAML only.

## Global Constraints

- **Every syntax form below was verified against the real binary before this plan was written.** Do not substitute forms from memory or from other examples without re-checking.
- `identity:` lives in `lute.project.yaml`, **never** in scene frontmatter — a scene-level `identity:` is `E-META-UNKNOWN-KEY`.
- `IDENTITY_TOKENS` is exactly `{prefix}`, `{speaker}`, `{code}`. Any other token is rejected at project load. `prefix` derives as `{character}.s{season}ep{episode}`.
- `kind: quest` documents take `kind`, `luteVersion`, `uses`, `title` only. `character`/`season`/`episode` are scene-only and are `E-META-UNKNOWN-KEY` in a quest.
- A rule's head relation MUST also be declared, with `derive: true` and no `tier:`. An underived head is `E-RELATION-UNKNOWN`.
- `rules:` is a YAML list of quoted strings. Non-derived relations carry `tier: run`.
- `<timeline>` is choreography with a sub-second local clock. It is **not** a countdown. The shed clock is the declared scalar `run.shedPressure`, moved by `::set` and read by `when=` / `done=`.
- `<on>` is a quest lifecycle hook: `<on event="questComplete">` / `<on event="questFailed">`.
- Tags are line-oriented: an opener, children on their own lines, then the closer. A single-line `<tag>body</tag>` is `E-TAG-INLINE-BODY`.
- Every content-line vocabulary attribute must be declared in `vocabulary.schema.yaml` or checking fails with `E-DOMAIN-UNKNOWN`.
- `lute check-project docs/examples` must exit 0 at the end of every task.
- NEVER run repo-wide `cargo fmt`. No Rust source changes at all in this plan.
- Branch `example/anseo-prologue` is already checked out. Commit there.

---

### Task 1: Project root, vocabulary, and world schema

**Files:**
- Create: `docs/examples/anseo/lute.project.yaml`
- Create: `docs/examples/anseo/vocabulary.schema.yaml`
- Create: `docs/examples/anseo/world.schema.yaml`
- Create: `docs/examples/anseo/scenes/wake.lute`

**Interfaces:**
- Produces: the vocabulary every later scene imports via `uses: [../vocabulary.schema.yaml]`; the state and relations every later quest imports via `uses: ../world.schema.yaml`; the `identity:` templates that fix every `lineId` in the project.

- [ ] **Step 1: Write the project file**

```yaml
defaultProfile: main
profiles:
  main: {}
identity:
  lineId: "{prefix}.{speaker}_{code}"
  voiceKey: "{speaker}-{code}"
```

- [ ] **Step 2: Write the vocabulary**

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

- [ ] **Step 3: Write the world schema**

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

`can_halt` carries `derive: true` and no `tier:`. Omitting the declaration is `E-RELATION-UNKNOWN` at every use site.

- [ ] **Step 4: Write the opening scene**

```lute
---
kind: scene
character: anseo
season: 1
episode: 1
uses: [../vocabulary.schema.yaml]
---

## Cold Wake.
::auto{character="vesna" anchor="port" action="brace"}
@vesna{code="0010" emotion="clipped"}: Cryo's gone. We don't go back under.
@vesna{code="0020" emotion="level"}: So we walk.
```

- [ ] **Step 5: Verify it checks clean**

Run: `./target/debug/lute check-project docs/examples/anseo`
Expected: `ok: docs/examples/anseo (1 file(s), 0 project-wide warning(s))`

- [ ] **Step 6: Verify the identity template took effect**

Run: `./target/debug/lute compile docs/examples/anseo/scenes/wake.lute -o /tmp/anseo-t1.json && grep -o '"lineId": *"[^"]*"' /tmp/anseo-t1.json | head -2`
Expected: `anseo.s01ep01.vesna_0010` and `anseo.s01ep01.vesna_0020`

- [ ] **Step 7: Verify the whole examples tree still passes**

Run: `./target/debug/lute check-project docs/examples`
Expected: exit 0

- [ ] **Step 8: Commit**

```bash
git add docs/examples/anseo
git commit -m "feat(example): Anseo project root, vocabulary, and world schema"
```

---

### Task 2: The exits proof — a departure that only `exits:` explains

**Files:**
- Modify: `docs/examples/anseo/scenes/wake.lute`

**Interfaces:**
- Consumes: the `action` domain and its `exits:` list from Task 1.
- Produces: the first artifact in the corpus where a declared exit member marks a sprite record `exit: true`.

- [ ] **Step 1: Add a line whose action is a declared exit**

Append inside the shot:

```lute
@vesna{code="0030" emotion="hollowed" action="go-under"}: If the second pod's intact, I'm taking it.
```

- [ ] **Step 2: Compile and inspect the sprite record**

Run: `./target/debug/lute compile docs/examples/anseo/scenes/wake.lute -o /tmp/anseo-t2.json && grep -o '"exit": *true' /tmp/anseo-t2.json | wc -l`
Expected: `1`

- [ ] **Step 3: Prove a non-exit action does not set it**

Temporarily change `action="go-under"` to `action="drift"`, recompile, and confirm the count is `0`. Restore `go-under`.

This is the demonstration the example exists to carry: `go-under` means "returns to cryo", which no naming heuristic recovers. The compiler knows only because `vocabulary.schema.yaml` declares it.

- [ ] **Step 4: Commit**

```bash
git add docs/examples/anseo/scenes/wake.lute
git commit -m "feat(example): a cryo return that only declared exits can explain"
```

---

### Task 3: The shed clock as declared state

**Files:**
- Create: `docs/examples/anseo/scenes/cryobank.lute`

**Interfaces:**
- Consumes: `run.shedPressure` from Task 1's world schema.
- Produces: the `after:` route target that Task 4's scenes chain from.

- [ ] **Step 1: Write the scene, with a choice that costs clock**

```lute
---
kind: scene
character: anseo
season: 1
episode: 2
uses: [../vocabulary.schema.yaml, ../world.schema.yaml]
after: 'visited("anseo.s01ep01")'
---

## The Cryobank.
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

Note the tags: opener, children on their own lines, closer. A single-line `<choice …>…</choice>` is `E-TAG-INLINE-BODY`.

- [ ] **Step 2: Verify**

Run: `./target/debug/lute check-project docs/examples/anseo`
Expected: `ok`, 2 files

- [ ] **Step 3: Confirm the clock is authored state, not a timer**

Run: `./target/debug/lute compile docs/examples/anseo/scenes/cryobank.lute -o /tmp/anseo-t3.json && grep -c 'shedPressure' /tmp/anseo-t3.json`
Expected: a non-zero count — the increments are `setState` records in the artifact, which is exactly the point: the engine executes authored mutations, it does not run a clock.

- [ ] **Step 4: Commit**

```bash
git add docs/examples/anseo/scenes/cryobank.lute
git commit -m "feat(example): the cryobank, where waking crew costs clock"
```

---

### Task 4: The relational quest gate

**Files:**
- Create: `docs/examples/anseo/quests/hold-the-spine.lute`

**Interfaces:**
- Consumes: `can_halt`, `awake`, `knows` from Task 1; `run.shedPressure` from Task 1; the `::assert` facts written in Task 3.
- Produces: the pattern every later quest follows.

- [ ] **Step 1: Write the quest**

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

The frontmatter has no `character`/`season`/`episode` — those are scene-only keys and are `E-META-UNKNOWN-KEY` here.

- [ ] **Step 2: Verify**

Run: `./target/debug/lute check-project docs/examples/anseo`
Expected: `ok`, 3 files

- [ ] **Step 3: Prove the gate is a query, not a flag**

Temporarily change `start="holds(can_halt(toma))"` to `start="holds(can_halt(vesna))"` and re-check: it still passes, because the checker validates the query's shape, not its runtime truth. Then change it to `holds(can_halt(nobody))` and confirm it FAILS — `nobody` is not a `crew` member. Restore `toma`.

This is what a closed entity domain buys: a typo in a gate is a check-time error.

- [ ] **Step 4: Commit**

```bash
git add docs/examples/anseo/quests/hold-the-spine.lute
git commit -m "feat(example): gate a quest on a derived relation, not a flag"
```

---

### Task 5: The terminator

**Files:**
- Create: `docs/examples/anseo/scenes/bridge.lute`
- Create: `docs/examples/anseo/scenes/shed.lute`

**Interfaces:**
- Consumes: the `after:` chain from Tasks 1 and 3.
- Produces: the corpus's first two `::end` uses, with distinct reasons.

- [ ] **Step 1: Write the success terminal**

```lute
---
kind: scene
character: anseo
season: 1
episode: 10
uses: [../vocabulary.schema.yaml, ../world.schema.yaml]
after: 'visited("anseo.s01ep02")'
---

## The Bridge.
::auto{character="vesna" anchor="center" action="brace"}
@vesna{code="0010" emotion="level"}: Whatever's left of the ship, it's steering.
::end{reason="bridge-reached"}
```

- [ ] **Step 2: Write the failure terminal**

```lute
---
kind: scene
character: anseo
season: 1
episode: 11
uses: [../vocabulary.schema.yaml, ../world.schema.yaml]
after: 'visited("anseo.s01ep02")'
---

## Shed.
@purser{code="0010" emotion="level" os}: Module released. Allocation is satisfied.
::end{reason="shed-with-module"}
```

- [ ] **Step 3: Verify both check clean**

Run: `./target/debug/lute check-project docs/examples/anseo`
Expected: `ok`, 5 files

- [ ] **Step 4: Prove `::end` terminates the walk**

Add a line after `::end` in `shed.lute`, re-check, and confirm `W-CODE-AFTER-END`. Remove it.

- [ ] **Step 5: Confirm both reasons reach the artifact**

Run: `./target/debug/lute compile docs/examples/anseo/scenes/bridge.lute -o /tmp/anseo-b.json && grep -o '"reason": *"[^"]*"' /tmp/anseo-b.json`
Expected: `"reason": "bridge-reached"`

- [ ] **Step 6: Commit**

```bash
git add docs/examples/anseo/scenes/bridge.lute docs/examples/anseo/scenes/shed.lute
git commit -m "feat(example): two terminals, two end reasons"
```

---

### Task 6: The Purser component

**Files:**
- Create: `docs/examples/anseo/components/purser-interject.component.lute`
- Modify: `docs/examples/anseo/scenes/cryobank.lute` (add `components:` and one `::use`)

**Interfaces:**
- Produces: a reusable interjection every later scene invokes with `::use{component="purserInterject" …}`, so the Purser's voice is authored once.

The Purser speaks in every module. Authoring that inline eleven times is the duplication components exist to remove.

- [ ] **Step 1: Write the component**

```lute
---
component: purserInterject
params:
  pressure: { enum: [low, rising, critical] }
---

## Interjection.
<match on="@pressure">
<when is="low">
@purser{code="0010" emotion="level" os}: Allocation is nominal.
</when>
<when is="rising">
@purser{code="0020" emotion="level" os}: Draw exceeds projection. The schedule advances.
</when>
<when is="critical">
@purser{code="0030" emotion="clipped" os}: Release is imminent. Clear the module.
</when>
</match>
```

A component body resolves vocabulary against the IMPORTING document, so the scene that uses it must import `vocabulary.schema.yaml` — the component's own `uses:` is discarded at parse. This is the documented limitation from the 0.9.0 release notes, and the example is a good place to exercise it.

- [ ] **Step 2: Invoke it from the cryobank**

Add `components: [../components/purser-interject.component.lute]` to `cryobank.lute`'s frontmatter and replace its inline Purser line with:

```lute
::use{component="purserInterject" pressure="rising"}
```

- [ ] **Step 3: Verify**

Run: `./target/debug/lute check-project docs/examples/anseo`
Expected: `ok`, 5 files

- [ ] **Step 4: Commit**

```bash
git add docs/examples/anseo/components docs/examples/anseo/scenes/cryobank.lute
git commit -m "feat(example): the Purser speaks through a component"
```

---

### Task 7: The branch scenes

**Files:**
- Create: `docs/examples/anseo/scenes/spine-a.lute` (episode 3)
- Create: `docs/examples/anseo/scenes/hydroponics.lute` (episode 4)
- Create: `docs/examples/anseo/scenes/machine-deck.lute` (episode 5)
- Create: `docs/examples/anseo/scenes/stowaway.lute` (episode 6)

**Interfaces:**
- Consumes: everything from Tasks 1-6.
- Produces: `anseo.s01ep06` as the join point Task 8's `spine-b` routes from.

This is a writing task: the plan fixes each scene's structural contract, and the dialogue is the deliverable produced while executing it. Every scene takes the frontmatter shape from Task 3 — `character: anseo`, its episode number, `uses:` both schemas, `components:` where it invokes the Purser, and an `after:` naming its predecessor.

- **spine-a** (after ep02) — the first shed, on screen. This is where `<timeline>` belongs and the only place in the example it appears: a bounded choreography beat with a camera track and a `brace` track and a `shed` vfx, durations in fractions of a second. It is NOT the countdown.
- **hydroponics** (after ep03) — Vesna's scene. `::set{run.vesnaTrust += 1}` on the honest branch, so Task 9's `what-vesna-carries` becomes reachable.
- **machine-deck** (after ep03) — the alternative route. If `holds(awake(toma))` the coupling is saved; otherwise `::set{run.shedPressure += 1}`.
- **stowaway** (after ep04 or ep05) — Ottavio. `::assert{found(ottavio)}`.

- [ ] **Step 1: Write the four scenes**

Every vocabulary attribute must be a declared member of `vocabulary.schema.yaml`; anything else is `E-DOMAIN-UNKNOWN`. Tags are line-oriented — opener, children on their own lines, closer.

- [ ] **Step 2: Verify**

Run: `./target/debug/lute check-project docs/examples/anseo`
Expected: `ok`, 9 files

- [ ] **Step 3: Confirm the timeline is choreography-shaped**

Run: `./target/debug/lute compile docs/examples/anseo/scenes/spine-a.lute -o /tmp/anseo-t7.json`
Inspect the timeline's records: durations must be sub-second and the block must carry concurrent tracks. A `duration` of tens of seconds means it is being used as a countdown, which is the misreading this example exists partly to correct.

- [ ] **Step 4: Commit**

```bash
git add docs/examples/anseo/scenes
git commit -m "feat(example): the branch scenes and the first shed"
```

---

### Task 8: The convergence scenes

**Files:**
- Create: `docs/examples/anseo/scenes/spine-b.lute` (episode 7)
- Create: `docs/examples/anseo/scenes/archive.lute` (episode 8)
- Create: `docs/examples/anseo/scenes/purser.lute` (episode 9)
- Modify: `docs/examples/anseo/scenes/bridge.lute` (repoint `after:`)
- Modify: `docs/examples/anseo/scenes/shed.lute` (repoint `after:`)

**Interfaces:**
- Consumes: Task 7's branch scenes.
- Produces: the complete eleven-scene graph with both terminals reachable.

- **spine-b** (after ep06) — the second shed. The branches converge.
- **archive** (after ep07) — the manifest. `::assert{knows(vesna, manifest)}`.
- **purser** (after ep07 or ep08) — the confrontation. What can be said depends on who is awake, via `when=` guards reading the relational facts.

- [ ] **Step 1: Write the three scenes**

- [ ] **Step 2: Repoint the terminals**

`bridge.lute`'s `after:` becomes `'visited("anseo.s01ep09")'`; `shed.lute`'s becomes `'visited("anseo.s01ep07")'`.

- [ ] **Step 3: Verify the whole project**

Run: `./target/debug/lute check-project docs/examples/anseo`
Expected: `ok`, 12 files (11 scenes + 1 quest)

- [ ] **Step 4: Verify both terminals are reachable**

Run: `./target/debug/lute scenario docs/examples/anseo`
Expected: both `bridge` and `shed` reachable. If either is not, the `after:` graph is wrong — fix the routes, not the expectation. Record the output in the commit body.

- [ ] **Step 5: Commit**

```bash
git add docs/examples/anseo/scenes
git commit -m "feat(example): the convergence scenes and both terminals"
```

---

### Task 9: The remaining five quests

**Files:**
- Create: `docs/examples/anseo/quests/unmoored.lute`
- Create: `docs/examples/anseo/quests/who-wakes.lute`
- Create: `docs/examples/anseo/quests/false-heading.lute`
- Create: `docs/examples/anseo/quests/manifest-gap.lute`
- Create: `docs/examples/anseo/quests/what-vesna-carries.lute`

**Interfaces:**
- Consumes: the relations and state from Task 1; the pattern from Task 4.

Gates, all verified forms:

| Quest | `start=` |
|---|---|
| `unmoored` | `"true"` |
| `who-wakes` | `"true"` |
| `false-heading` | `holds(awake(ilsabet))` |
| `manifest-gap` | `holds(found(ottavio))` |
| `what-vesna-carries` | `run.vesnaTrust >= 2` |

- [ ] **Step 1: Write all five**

Each uses the Task 4 frontmatter shape — `kind`, `luteVersion`, `uses`, `title` only.

- [ ] **Step 2: Verify**

Run: `./target/debug/lute check-project docs/examples/anseo`
Expected: `ok`, 17 files

- [ ] **Step 3: Commit**

```bash
git add docs/examples/anseo/quests
git commit -m "feat(example): five more quests, three gated on relations"
```

---

### Task 10: Wire into the gates and write the findings

**Files:**
- Create: `docs/examples/anseo/README.md`
- Modify: `scripts/check-docs-consistency.py` (the example-roots list, if the new root needs registering — check first)
- Create: `docs/superpowers/notes/2026-07-31-anseo-authoring-findings.md`

- [ ] **Step 1: Check whether the new root needs registering**

Run: `python3 scripts/check-docs-consistency.py`
It prints the example roots CI checks. If `docs/examples/anseo` is covered by the existing `docs/examples` root, nothing to add — confirm rather than assume.

- [ ] **Step 2: Write the README**

Explain what the example demonstrates and, specifically, which constructs it is the corpus's first or only coverage of: `::end`, `identity:`, and derived relations.

- [ ] **Step 3: Write the findings note**

This is a first-class deliverable, not a postscript. Record every rough edge hit while authoring eleven scenes: diagnostics that misdirected, syntax that had to be looked up rather than guessed, anything that needed a source read. The 0.9.0 release came out of exactly this kind of note.

Seed it with the eight errors found while validating this plan — all real, all caught before a line of content was written:

1. `<timeline>` is choreography, not a countdown
2. `<on>` is a quest lifecycle hook, not a reactive trigger
3. `identity:` is a project key, not scene frontmatter
4. `IDENTITY_TOKENS` is only `{prefix}`/`{speaker}`/`{code}`
5. Quests do not take `character`/`season`/`episode`
6. A rule head must be declared with `derive: true`
7. `rules:` is a list of quoted strings; relations need `tier:`
8. Quests use `start=`/`fail=` expressions rather than conditional existence

That eight of eight design assumptions were wrong before checking is itself the finding: the language's shapes are not guessable from adjacent knowledge, which is worth saying out loud in the note.

- [ ] **Step 4: Full verification**

```bash
./target/debug/lute check-project docs/examples
cargo test --workspace --no-fail-fast
python3 scripts/check-docs-consistency.py
python3 scripts/check-doc-snippets.py
```
Expected: all clean, 0 test failures.

- [ ] **Step 5: Commit**

```bash
git add docs/examples/anseo/README.md docs/superpowers/notes
git commit -m "docs(example): Anseo README and authoring findings"
```
