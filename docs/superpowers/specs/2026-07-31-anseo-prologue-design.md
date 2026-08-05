# *Anseo* prologue — a showcase-scale Lute example

**Status:** design, approved for scale. Target: `docs/examples/anseo/`.

## Why this exists

Lute's example corpus is 27 `.lute` files that each demonstrate one construct, plus three small
projects (a service-rhythm dating sim, a murder investigation, a plugin showcase). Nothing in it
answers the question a prospective author actually asks: *what does a real opening act look like?*

Measured coverage gaps in that corpus:

| Construct | Examples using it |
|---|---|
| `::end` | **0** |
| `identity:` | **0** |
| `::retract` | 1 |
| `relations:` / `entities:` | 2 |
| `<on>` / `::assert` / `rules:` | 3 |

`::end` having zero coverage is the sharpest: it was dsl 0.8.0's headline addition, and
`terminatesWalk` is the only `SEMANTICS_VOCAB` flag with a real consumer.

This example is sized to Baldur's Gate 3's opening — the Nautiloid through the beach — because
that scale is what forces those constructs to appear naturally rather than as demonstrations.

**Secondary purpose, and historically the more valuable one:** authoring real volume is how the
language's rough edges surface. The OSHiZ adoption assessment drove the whole of 0.9.0. Findings
from writing this are a first-class deliverable, not a byproduct.

## Premise

The generation ship *Anseo*, 300 years into a 900-year voyage, is shedding modules to survive. The
schedule is not negotiable. The sleepers should have passed through it unconscious; instead the
wake system fired early for a handful of them, and their cryo units are damaged. They cannot go
back under. The ship keeps getting smaller.

**The mechanic with teeth:** opening a cryo pod draws power. The allocation intelligence reads
that as load and brings the next shed forward. Every companion you wake costs you time.

## Cast

| | |
|---|---|
| **Vesna Oyelaran** | Hydroponics. Woke alongside you. Will not say why she woke. |
| **Toma Rask** | Structural engineer. Knows how to halt a shed. Most expensive to wake. |
| **Ilsabet Quay** | Navigator. Knows the heading is wrong. Cheap to wake, destabilises the others. |
| **Ottavio** | Stowaway, on no manifest. Optional, and finding him has consequences. |
| **the Purser** | The allocation intelligence. Not a person; speaks through the walls. |

The Purser motivates `mono`/`os` delivery flags on content lines without contrivance.

## Scene graph

```
wake ── cryobank ── spine-a ─┬─ hydroponics ─┐
                             └─ machine-deck ─┴─ stowaway ── spine-b ─┬─ archive ─┐
                                                                       └─ purser ──┴─ bridge
        cryobank ╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌ (failure) ╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌ shed
        spine-b  ╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌ (failure) ╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌ shed
```

Eleven scenes. `bridge` and `shed` both terminate with `::end`, carrying distinct reasons.

Scene ordering is declared with `after:` routes, which is what makes `check-project` able to prove
a scene's state reads are satisfiable — the property the investigation example already exercises
and this one exercises at depth.

## Quests

Six, concurrent. Quests are not "conditionally created": a `<quest>` carries `start=` and `fail=`
expressions, and those may be relational queries.

| Quest | `start=` |
|---|---|
| `unmoored` | `start="true"` — the main line |
| `who-wakes` | `start="true"`, objectives per crew member |
| `hold-the-spine` | `holds(can_halt(toma))` |
| `false-heading` | `holds(awake(ilsabet))` |
| `manifest-gap` | `holds(found(ottavio))` |
| `what-vesna-carries` | `run.vesnaTrust >= 2` |

`found` is a fourth relation, `{ args: [crew], tier: run }`, asserted when a crew member is
located rather than woken — Ottavio is never in a pod. `run.vesnaTrust` is a declared scalar
incremented at three authored beats; the threshold is `>= 2`, so the quest is reachable without
taking every option but not by accident.

## Vocabulary — the 0.9.0 surface

The point of a project-declared vocabulary is that it does not look like the compiler's. This one
reads as a crew under structural strain:

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

`go-under` — returning to cryo — is the argument for `exits:` being **declared** rather than
inferred. No naming heuristic recovers it: `drift` and `go-under` are equally opaque strings,
and only the schema distinguishes them.

**Position matters, and it is the sharpest edge in the language.** `lower.rs:178-185` is the
single reader of `exits:` for both crates, and it lives in the `"auto"` arm: `exit: true` is
emitted onto the **sprite** record produced by `::auto{action="go-under"}`. The same attribute
on a content line — `@vesna{action="go-under"}` — lowers to `line.action` and carries no
`exit` field at all. Verified by compiling both forms. The example must stage the departure
with `::auto`; an author who writes it on the dialogue line gets silence, not a diagnostic.

## The countdown, stated accurately

The shed clock is **authored bookkeeping, not an engine timer.** Lute has no simulated clock.

```yaml
state:
  run.shedPressure: { type: number, default: 0 }
```

`::set{run.shedPressure += 1}` at each cost, `when=` guards and quest `done=`/`fail=` read it.
The pacing is enforced by authored state and by `after:` route analysis, not by wall time.

`<timeline>` is **not** this mechanism. A timeline is a bounded non-interactive choreography unit
with a local clock in fractions of a second, running concurrent subject/property tracks and
blocking the following content until it completes. Its correct role here is the shed itself: a
~1.5s beat with camera shake, a `brace` action, and a `shed` vfx on separate tracks.

`<on>` is likewise not a reactive trigger — it is a quest lifecycle hook, `<on event="questComplete">`
and `<on event="questFailed">`.

## Relational layer

The state the game asks about is genuinely relational: *is anyone awake who can stop this?*

```yaml
entities:
  crew:   { members: [vesna, toma, ilsabet, ottavio] }
  module: { members: [infirmary, cryobank, spine, hydroponics, machinedeck, archive, bridge] }
  topic:  { members: [shed_sequence, true_heading, manifest] }
relations:
  awake:    { args: [crew], tier: run }
  knows:    { args: [crew, topic], tier: run }
  attached: { args: [module], tier: run }
  found:    { args: [crew], tier: run }
rules:
  - "can_halt(C) :- awake(C), knows(C, shed_sequence)"
```

Shapes verified against `docs/examples/investigation/world.schema.yaml`: `rules:` is a YAML list
of quoted strings, and each relation carries a `tier:`. Getting this wrong in the spec would have
propagated into every quest gate.

`hold-the-spine` gates on `holds(can_halt(toma))` rather than a boolean flag. This is the corpus's
thinnest area (`relations:` in 2 examples, `rules:` in 3), and the premise demands it rather than
decorating with it.

## Identity

`IDENTITY_TOKENS` is exactly three: `{prefix}`, `{speaker}`, `{code}`. Any other token is rejected
at project load, so a `{ship}.{module}.…` template is **invalid**.

`prefix` derives from frontmatter as `{character}.s{season}ep{episode}` — verified empirically:
a scene with `character: anseo, season: 1, episode: 3` and `@vesna{code="0040"}` compiles to
`lineId: anseo.s01ep03.vesna_0040`, `voiceKey: vesna-0040`.

So the ship name rides in `character:` and the module sequence in `episode:`. The example declares
an explicit `identity:` block anyway — the corpus has zero — pinning the default line-id shape and
a voice-key convention suited to a fully voiced cast.

## Layout

```
docs/examples/anseo/
  lute.project.yaml
  vocabulary.schema.yaml      # the 0.9.0 surface
  world.schema.yaml           # state + entities/relations/rules
  scenes/       11 files
  quests/       6 files
  components/   shared Purser interjections
  tests/        2 scenario tests, one per ending
  mocks/        playthrough.yaml
```

`lute init` scaffolds this layout exactly — project file, both schemas, `scenes/`, `mocks/`,
README. The example uses the scaffolder rather than hand-writing it, which dogfoods the tool
the docs tell every new author to run first.

## Scope and risk

The bulk of this work is **writing, not engineering** — eleven scenes of real dialogue. That is
also where its value is: volume is what surfaces rough edges.

At this size it is not a small teaching example, so it sits as its own project root under
`docs/examples/` and is gated by `check-project` like the other roots. The tutorial keeps using
the two-file `episodes/` project; this one is the "what does a real act look like" reference.

Both terminal scenes must be reachable and both `::end` reasons distinct, or the example fails to
demonstrate the construct it exists partly to cover.
