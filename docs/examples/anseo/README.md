# Anseo — a prologue in eleven scenes

A generation ship eleven years into a four-month voyage. The Purser is releasing
modules on a schedule, the crew is in cryo, and Vesna is awake. The prologue ends
either on the bridge or in the shed.

Anseo exists to be a **whole small work** rather than a feature demo. Every other
example under `docs/examples/` isolates one construct; this one carries a story
across eighteen documents and asks whether the constructs still hold when they
have to coexist. It was written as the instrument of a drive test, and the
measurement — 111 entries, 74 of them carrying a verdict — is
[`docs/superpowers/notes/2026-07-31-anseo-drive-test-findings.md`](../../superpowers/notes/2026-07-31-anseo-drive-test-findings.md).
Read that file if you want to know what 0.9.0 costs an author. Read this one if
you want to know what is in the tree.

## What is here

| | count | |
|---|---|---|
| `scenes/` | 11 | episodes 1–11, one connected graph |
| `quests/` | 6 | five with `after=`, one deliberately without |
| `components/` | 1 | the Purser's interjection, the only reuse construct in the language |
| `tests/` | 31 | `*.test.yaml` scenario tests, run by `lute test` |
| `world.schema.yaml` | | scalar state, four relations, one Datalog rule, one seed fact |
| `vocabulary.schema.yaml` | | all seven compiler-typed content-vocabulary slots |
| `lute.project.yaml` | | the manifest, including the `identity:` block |

177 content lines, 2,302 words, 15 choices (`lute loc report`).

## What it is the corpus's first coverage of

Three of these are genuinely first; the fourth is a matter of degree, and the
distinction is kept because a README that oversells is worse than no README.

- **`::end` — first, and still the only.** `grep -rl '::end' docs/examples`
  returns `bridge.lute` and `shed.lute` and nothing else. Before Anseo, no
  document under `docs/examples/` had ever terminated a walk. Read
  [T5.1 and T5.5](../../superpowers/notes/2026-07-31-anseo-drive-test-findings.md)
  before you read `::end` as an *ending*: it is `break` with a label, exactly
  equivalent to falling off the end of the command array, and the two terminals
  here are leaves of the graph by coincidence of `after:` rather than by any
  property the toolchain can see.
- **`identity:` — first, and still the only.** Anseo's is the one
  `lute.project.yaml` in the repo that sets `lineId`/`voiceKey` templates, so it
  is the only place the `{prefix}`/`{speaker}`/`{code}` token set is exercised on
  real content. Note that the templates govern only when Anseo is compiled as its
  own root; built from `docs/examples`, its scenes take the outer manifest's
  templates. They agree today only because the outer manifest declares none.
- **Scenario tests at scale — 31 of the corpus's 34.** `investigation/` has the
  other three. This is the first suite large enough to say what `lute test` is
  and is not: a good regression harness for authored text and state deltas, and a
  poor specification of the work's logic. Deleting the guard from either lever
  that decides which ending the prologue reaches leaves all 31 green.
- **Derived relations — not first, but first *across documents*.**
  `investigation/world.schema.yaml` and `act1.schema.yaml` both declare
  `derive: true` relations, so the construct is not new here. What is new is the
  distance: `can_halt(C) :- awake(C), knows(C, shed_sequence)` has its premises
  asserted inside `<choice>` arms of episodes 2 and 8 and in a `facts:` seed, and
  its head is gated in six quest documents that assert none of them — up to seven
  episodes upstream. In `investigation`, the one document that reads a derived
  relation is the same document that asserts its base fact.

## Running it

```sh
# The checker. Exits 0. Prints 15 warnings: 13 project-wide (which is the
# number in its own summary line) plus 2 per-file. See "Deliberate
# imperfections" below — every one of the 15 is accounted for there.
lute check-project docs/examples/anseo

# The 31 scenario tests. Exits 0, 31 passed.
lute test docs/examples/anseo
lute test docs/examples/anseo --coverage    # 15 rows; three are permanently red

# The prerequisite graph: 11 scenes over 9 topological layers, and 19 edges —
# 12 scene-to-scene, 5 from a scene to one of the five quests that declare
# `after=`, and 2 quest-to-quest (`whoWakes` gates two siblings, one on
# `active` and one on `completed`).
lute scenario docs/examples/anseo

# Preview one scene. `trace` runs no Datalog fixpoint, so a guard over the
# derived `can_halt` must be seeded — or driven through `lute run` instead.
lute trace docs/examples/anseo/scenes/cryobank.lute \
  --project docs/examples/anseo --choose whoWakes=wakeToma
```

`check-project docs/examples` (the outer root, which is what CI runs) walks all
eighteen of these documents. `lute test docs/examples` picks up all 31 of these
tests plus `investigation`'s three.

## Deliberate imperfections

Four things in this tree are wrong on purpose. They are evidence for named
findings, and a reader who "fixes" one deletes the evidence.

1. **`scenes/bridge.lute:11` — `W-INJECT-CONFLICT`.** Vesna is staged
   `anchor="center"`, which is the `anchor` domain's declared `default:`, and
   that is the one value an author may not write on purpose: agreement warns,
   disagreement is silent, omission is silent. There is no `--allow` and no
   in-source suppression, so the scene keeps the true statement about the staging
   rather than the clean output (**T5.8**).
2. **`scenes/spine-b.lute:44` — `W-INJECT-CONFLICT`.** The same collision, kept
   for the same reason: Toma's place between the other two at the coupling he
   saved *is* the staging. The other two scenes that stage a third character omit
   the attribute instead, because there the entrance is ordinary — the tax is not
   per-scene, it is per-scene-where-the-middle-is-meaningful, and here that was
   one in three (**T8.12**).
3. **`components/purser-interject.component.lute` — a dead `<otherwise>` arm.**
   The component's body is a param-scoped `<match on="@pressure">` with two arms,
   and it is invoked from exactly one site (`cryobank.lute:14`, `pressure="rising"`).
   `Allocation is nominal.` therefore never plays in this work. Nothing says so:
   `check-project` is clean, the artifact correctly prunes the arm, `loc export`
   correctly keeps it (another caller may need it translated), and
   `lute test --coverage` has no row for it at all, because component-internal
   matches never appear in a traced report. The arm stays because a one-armed
   component is not a component, and because it is the corpus's only example of
   the one document kind that cannot be tested (**T9.12**).
4. **`mocks/playthrough.yaml` — a rotting scaffold mock.** It is still exactly
   what `lute init` wrote: it names `scenes/opening.lute`, deleted in the first
   task, and seeds `run.greeted`, a path no longer in the schema. `check-project`
   is green through it, because mocks are not in its walk. The error does exist
   one command over — `lute trace … --mock mocks/playthrough.yaml` reports
   `E-TRACE-MOCK-UNDECLARED` — but nothing pairs them automatically. This is how
   a `mocks/` directory quietly becomes fiction (**T1.9**).

Separately, the 13 project-wide `W-UNPROVEN-RELATIONAL` warnings are neither
deliberate nor removable. Each marks a correct relational gate on a producible
relation; the checker declines to claim the ground query is true, which is honest.
The warning has no discharge path — no `--allow`, no seed surface on
`check-project`, no site-level acknowledgement — so a finished, correct, fully
tested work triggers it thirteen times and there is nothing an author can do
about it (**T9.19**).

## Reading order

`scenes/wake.lute` (ep01) is the entry point and has no `after:`. From there the
graph is: cryobank (02) → spine-a (03) → {hydroponics (04) | machine-deck (05)} →
stowaway (06) → spine-b (07) → {archive (08), shed (11)} → purser (09) →
bridge (10).

Two cautions about that sentence. The `{a | b}` is a lie the language cannot
correct: `after:` is a **monotone availability lattice**, so ep04 and ep05 both
unlock at the same instant and nothing prevents visiting both. They are
alternatives in the story and siblings in the graph (**T8.3**). And no choice a
player makes decides which scene comes next — `spine-b`'s three-arm branch is
labelled with intentions, not routes, and says so in a comment (**T8.2**).
`lute scenario`'s output is the availability lattice, not the story's graph.
