# Anseo drive-test — running findings log

**This file is the primary deliverable.** The Anseo example is the instrument; this
log is the measurement. A task that produces a clean example and an empty section
here has not been executed — it has been evaded.

## What is being measured

Whether Lute 0.9.0 is mature enough to author a real work in. Not whether a
determined agent can eventually make `check-project` exit 0 — that is always true
of a Turing-complete-adjacent toolchain and measures nothing.

## The authoring rule (binding on every task that writes content)

> **Write what the beat needs. Then find out whether Lute can express it.**

Never the reverse. Choosing what to write based on what you already know compiles
produces a green example and a false reading. If a scene wants a character to
interrupt another mid-line, write that first and discover the answer — do not
quietly substitute two sequential lines because you know those work.

## Capture protocol

Append an entry the moment friction occurs, not at the end of the task from
memory. Reconstructed logs lose exactly the near-misses that matter.

Every entry carries:

- **Intent** — what the beat needed, in plain prose, written *before* the language
  enters the picture.
- **Attempt** — the form you reached for first, verbatim.
- **Result** — the exact diagnostic, or the silence.
- **Resolution** — what you ended up writing, or `NONE — intent abandoned`.
- **Verdict** — exactly one of the seven below. Never invent a verdict or hyphenate a
  hybrid (`AUTHOR-ERROR-adjacent` is not a verdict); if none fits, say so in the entry
  and raise it with the controller, who owns this table.

### Verdicts

| Verdict | Criterion |
|---|---|
| `LANGUAGE-GAP` | The intent cannot be expressed. **Two sufficient shapes, either one alone qualifies:** (a) you changed the story to fit the tool, or (b) only a lossy proxy exists — the intent is reachable by encoding it as something else, but nothing in the language *means* it, so nothing can check it. Do not withhold this verdict merely because a workaround was found; say which shape applies and what the proxy costs. |
| `ERGONOMIC` | Expressible, but the working form is materially worse than the natural one — more verbose, more indirect, or split across files for no modelling reason. |
| `DOC-GAP` | Expressible and reasonable, but **you had to read Rust source, a proposal, or a test to find it.** The website docs and `lute context` did not get you there. |
| `DOC-WRONG` | The docs are present and **false** — they state a restriction that does not exist, a behaviour that differs, or scope something to the wrong construct. Distinct from `DOC-GAP`, which is silence: silence makes an author search, a false statement makes them stop searching. Rank these above `DOC-GAP` by default; an author who believes a wrong doc never discovers they were lied to. |
| `AUTHOR-ERROR` | The docs said so plainly and you missed it. Not a finding — record it only if the diagnostic pointed somewhere unhelpful. |
| `TOOL-DEFECT` | The language and its docs are fine; a *tool* is wrong, incomplete, or lying about its own contract. A misdirecting diagnostic, a false green, a capability surface that omits something it advertises. Distinct from `DOC-GAP`: the information exists, but the tool that promised to hand it to you did not. |
| `SPEC-WRONG` | Everything works as designed and the design is the defect. Language, docs, and every tool agree; the specified behaviour is itself the wrong call. Use this when you cannot fault any implementation and still believe an author is badly served — a severity chosen wrongly, two equivalent proofs given unequal treatment, a default that surprises. State what the spec says, why it is wrong, and what it should say instead; this verdict is worthless without a proposed alternative. |

**The `DOC-GAP` bar is deliberately harsh.** A working author cannot read
`lower.rs`. If you needed to, the language failed them even though it compiled.

### Also record, always

- **A diagnostic that misdirected.** It said X, the real problem was Y. This
  outranks almost everything else here: a wrong error message costs an author more
  than a missing feature they can see is missing.
- **Silence.** You wrote something plausible, nothing complained, and it did not do
  what you meant. `exit: true` on a content line was exactly this, found while
  planning. Silence is the most expensive failure mode and the hardest to notice —
  when a beat does not appear in the artifact, log it before fixing it.
- **What worked well.** A maturity assessment that only lists complaints is not an
  assessment. If a construct carried real weight cleanly, say so and say why.

---

## Assessment — is Lute 0.9.0 ready to build a real work on?

*Written last, by the one agent who read all ten task sections. Everything below is
recounted from the entries rather than carried forward from their summaries; where a
published tally and my recount disagreed, the disagreement is named. Addressed to
someone deciding whether to commit a production to this toolchain.*

### The verdict

**Yes, for a single-locale work whose structure is simple and whose correctness you
intend to defend by playing it. No, for a localized work, for one that leans on
components, or for one whose branching structure is the point.**

Three conditions, stated so they can be checked against your own project rather than
argued with:

1. **If you localize and you want reuse, stop here.** Components and localization are
   mutually exclusive today (T6.10), with no author-side workaround. This is the only
   item in the log that is a hard blocker rather than a cost.
2. **If `lute test` passing is your definition of correct, adjust the definition.** The
   suite cannot see the class of guard bug that ships (T9.18), and a one-letter typo in
   a test file can turn a false assertion green (T9.8). The harness is a good regression
   net for authored text and state deltas and a poor specification of logic. Budget for
   playing the work.
3. **If the branching *is* the work, Lute models something adjacent to what you mean.**
   `after:` is a monotone availability lattice, not a route graph: no choice a player
   makes decides which scene comes next, alternatives cannot be declared exclusive, and
   "how did you get here" has no spelling (T8.3, T8.1, T5.5). You can ship a branching
   work — this one did — but the structure lives in your engine and Lute cannot check it.

Against that: the declared layers are checked better than I expected at 0.9.0, and the
relational layer in particular is a positive reason to choose this toolchain rather than
a thing you tolerate. Six quests gating on facts asserted in `<choice>` arms seven
episodes upstream, every gate correct on first write, and the checker proving
cross-document whether a gate can *ever* open — that is not table stakes and nothing
string-keyed computes it.

**The one-sentence version.** Lute 0.9.0's *language* is in better shape than its
*tools*, and its tools are in better shape than their *own account of themselves*: the
dominant finding across ten tasks is not a missing capability but a toolchain that
computes more than it will tell you, and occasionally tells you the opposite.

### Ranked by what it costs an author

Ranked by cost to the person writing the work — probability of being hit, times damage
when hit, times how long it takes to notice — not by verdict class. Verdict labels are
given, and they do not drive the order: two `DOC-WRONG`s outrank a `LANGUAGE-GAP` below,
and the log's single most consequential entry class is *silence*, which is not a verdict
at all.

1. **`::set` is not typed against the path it writes (T3.2, `TOOL-DEFECT`).** The single
   most expensive defect measured. `::set{run.shedPressure += "two"}` on a `number` path
   is `ok` under `--deny-warnings`, compiles, and is silently evaluated to `0` by the
   reference runtime; `= true` writes a boolean into it. Ranked first because the cost
   is *shipped wrongness* and the trigger is a typo in the most common write in the
   language, on a mechanic every work has. The asymmetry is what makes it damning:
   `<choice into="run.shedPressure">` without a value gets `E-INTO-VALUE` naming the
   path's type, one construct away, in the same compiler run. The checker knows.
2. **The test suite cannot see an over-permissive gate (T9.18, `TOOL-DEFECT`), and a
   mis-keyed test file passes while running the arm it excludes (T9.8, `TOOL-DEFECT`).**
   Second because these bound everything else: a defect your tests cannot catch has
   unbounded blast radius. Mutation-tested — delete the `when=` from either lever that
   decides which ending the prologue reaches and `check-project` is byte-identical, all
   31 tests pass, and the coverage block does not move. `lute test` enforces
   `chosen ⊆ eligible` and nothing checks `eligible ⊆ intended`; over-restriction is
   loud in play, under-restriction is the direction that reaches players. T9.8 is the
   cheaper and higher-probability half: `chooses:` for `choose:` is dropped in silence,
   `trace` auto-picks the first eligible arm, and the file's assertions — written for
   the arm it names — pass against the arm it did not.
3. **Components and localization are mutually exclusive (T6.10, `TOOL-DEFECT`;
   compounded by T6.11, `SPEC-WRONG`).** Conditional but absolute. Adopting the
   language's only reuse mechanism *removes* a line from the localization pipeline:
   `loc export` emits it with `lineId: null`, `loc import` skips it at exit 0 and tells
   you to run `lute tag`, `lute tag` answers `already tagged` and changes nothing —
   structurally, because a component has no frontmatter to build `{prefix}` from — and
   `compile --locales` then warns about a caller-derived id no export ever carried and
   ships English. Verified with a one-commit before/after. T6.11 removes the other half
   of the case for components: `{{@param}}` cannot render a `string`, the only param
   type that carries text, while the same runtime renders strings through the other two
   interpolation forms. If you localize, this is #1 and there is nothing to do about it.
4. **The branch and the graph are disjoint layers (T8.3, `LANGUAGE-GAP`; T8.2,
   `TOOL-DEFECT`; T8.1, `LANGUAGE-GAP`).** No `goto`/`next`/`route` on `<choice>`, and
   `after:` admits no state reads and no negation. So no player decision routes, and two
   scenes written as alternatives are siblings that both unlock forever — which silently
   invalidates every convergence guard downstream if a player visits both. T8.2 makes it
   worse than a documented limit: `<choice goto="…">` is *accepted and discarded* at
   exit 0, along with the whole logic-tag attribute surface, including `as=`, a real
   attribute whose removal the spec documents and whose sibling `persist` gets a bespoke
   column-exact migration error three lines away. An author reaches for the obvious
   thing and is told nothing.
5. **"You got through it without X" is unwritable, and the form you reach for completes
   instantly (T9.4, `LANGUAGE-GAP`).** Nothing in the language is evaluated at the end of
   a run, so `done="run.corpses == 0"` on a survival quest activates and completes at the
   title card, at exit 0, zero diagnostics. Ranked here rather than lower because it is
   silence attached to an extremely common goal shape, and because the failure looks like
   success.
6. **The documentation errors that stop a search (T7.7, T3.13, T10.1, T8.6 — all
   `DOC-WRONG`).** Ranked as a cluster and above two `LANGUAGE-GAP`s, because a gap you
   can see costs a workaround and a false sentence costs rounds you do not know you are
   spending. `branch-match-when.md` calls the content-line `when=` "exact sugar" for a
   `<match>` that does not compile for the guard class a real scene reaches for — and
   `lute trace` prints the illegal form back at you as if it were source. `directives.md`
   scopes `::assert`/`::retract` to quest documents; they work in scenes, and an author
   who believes it hand-rolls a state flag and loses the Datalog layer. `state-model.md`'s
   only `enum` state example does not parse, and its error arrives as `(1 issue(s))` with
   no body. `execution-model.md`'s reference dispatcher reads two field names the artifact
   does not have, and both are the guard fields, so an engine transcribed from it plays
   the default branch of everything without crashing.
7. **A dead quest is quieter than a live one (T4.5, `TOOL-DEFECT`).** `start=` on an
   unproducible relation emits *nothing*, while the correct gate emits a warning — and
   `scenario reach` prints `Reachable` for a quest that can never activate, having proved
   in the same run that it cannot. The analysis, the slot, the diagnostic class and the
   spec clause all exist; one branch is missing.
8. **`::end` is not an ending (T5.5, `DOC-WRONG`; T5.4, `LANGUAGE-GAP`).** It is `break`
   with a label — exactly equivalent to falling off the end of the command array — so
   "which nodes are terminals", "does every route reach one" and "can a route dead-end"
   are not unanswered but *unaskable*. Ranked eighth, not higher, for a reason worth
   stating: the finding is a false sentence on the homepage, and the condition it names
   costs an author modelling honesty rather than correctness. Note also that T8.10 and
   T8.3 relocate the cause — the structural questions are unaskable because of the
   prerequisite grammar (#4), not because of `::end`. Ranked as its own item because the
   *claim about endings* — a set, a polarity — is separately unstatable and reachable
   only by mirroring each ending into declared state and saying it twice, with nothing
   checking that the two agree.
9. **Nothing tells you what a counter can be, and the hand arithmetic is already wrong in
   this log (T8.4, T7.13, `ERGONOMIC`).** `scenario envelope` is byte-identical at the
   root and at the deepest node of an eleven-scene graph, because every defaulted path is
   safe to read everywhere. The working substitute is reading five documents and adding
   integers, and T7.13 did that and got it wrong, and T8 inherited the error and wrote a
   dead line into a scene before catching it. Two authors, same question, one wrong
   answer, zero diagnostics — the closest thing here to a measured cost for a missing
   feature.
10. **Thirteen undischargeable warnings over the checker's best output (T9.19,
    `SPEC-WRONG`).** `W-UNPROVEN-RELATIONAL` fires on the *presence of the feature*, not
    on any property of its use, so its count scales one-for-one with adoption of the
    thing the language is best at. There is no `--allow`, no seed surface on
    `check-project`, and no site-level acknowledgement. A finished, correct, fully tested
    work triggers it thirteen times forever, in the same project-wide block that carries
    `E-OBJECTIVE-UNSATISFIABLE` — this log's strongest finding. It teaches authors to read
    past the output they most need.

Below the line, and honestly below it: `lute context` omitting parts of the surface it
advertises (T1.6, T3.7, T7.5, T9.15 — four tasks, four different omissions);
`E-CLIP-OVERLAP` rejecting a boundary hand-off its own spec makes legal, because
`0.8 + 0.4` overshoots `1.2` in binary (T7.2 — sharp, but only reachable once a track has
two clips); the retyped frontmatter (T7.12); and the several diagnostics that report a
count where the content was in hand (T3.9, T1.10).

### What worked, and it is not a courtesy paragraph

**26 of the 110 entries are *worked well*** — the single largest disposition after
`TOOL-DEFECT`. Four things earned it repeatedly.

- **The relational layer is the best thing in this language, and it is the reason to
  choose it.** T4.2 is the receipt: a quest in its own file, with its own `uses:`, gated
  on a Datalog head whose base facts are asserted inside a `<choice>` arm of a different
  episode — and `check-project` decides whether that gate can ever open, closes the rule
  set to do it, names the offending relation, and flips warning→error when you delete the
  producer from the other document. Beside it, T3.4: a transposed `knows(shed_sequence,
  toma)` is caught *per argument index*, naming the entity kind of each slot, which no
  string-keyed design can do. And T8.13 is the scale test — nine guard expressions over
  four relations, reading facts asserted in four documents up to seven episodes upstream,
  **every one correct on first write**, producing four materially different scenes from
  one file. A boolean per crew member per fact would have been sixteen paths and no
  `can_halt` at all.
- **Diagnostics that carry their own rationale.** `E-COMPONENT-BODY` does not say no, it
  explains that presenting a menu records a selection and a selection is a state write —
  an author who reads it understands the rule rather than memorising a blacklist.
  `E-COMPONENT-STATE` names the remedy in the message ("bind it through a param").
  `E-BRANCH-ALL-GUARDED` explains that the menu could be empty. `E-CEL-PROFILE` and
  `E-IDENTITY-TEMPLATE` enumerate their entire closed sets, which is exactly what you
  need at the moment you are refused. Did-you-mean is on state paths, `::set` targets and
  scene keys. This is a better diagnostic surface than most shipped 1.0 compilers have,
  and the entries that complain about diagnostics are complaining against a high baseline.
- **`lute init` produces a working baseline, not a repair job.** It checks clean as
  generated, and its `vocabulary.schema.yaml` declares all seven compiler-typed slots with
  comments teaching the two structural rules (`action` needs `exits:`, `anchor` needs
  `default:`) *and why* — "the compiler reads those instead of guessing from names". Anseo's
  vocabulary is a member-for-member substitution into that skeleton. Someone designed this
  deliberately and it paid off.
- **The checker's domain enforcement, demonstrated rather than asserted.** T2.3's negative
  control is the cleanest measurement in the log: two artifacts differing in exactly one
  vocabulary member, one emitting `exit: true` and one not, from one declared list read by
  one function both checker and compiler call. Nothing in Anseo's vocabulary would have
  survived the deleted name-prefix heuristic — `go-under` and `step-out` are both exits and
  neither looks like one. And T6.4 vindicates a *restriction*: `::set` is forbidden in a
  component body, and it is the right call, because the one number this whole prologue is
  about would stop being auditable by reading if eleven files could charge it invisibly.

Also real: `lute scenario` is accurate, fast and readable at eleven nodes and was the
surface that most helped hold the work in one head (T8.10); `after:` disjunction resolved
to two edges first try and an eleven-node rewire cost two lines (T7.15, T8.13); the quest
layer took every shape a real goal machine wants at five instances, with `fail=` as a
genuine independent axis and sequenced objectives falling out of reserved paths the
compiler already declares (T4.1, T9.2, T9.16); and `::end` itself, the least-exercised
construct in the language, lowered, addressed, ran and dead-code-analysed correctly on
first use with no probing (T5.1).

### The count, and why the retractions are the reason to trust it

**110 numbered entries. 73 carry one of the seven verdicts.** Recounted heading by heading
from the entries themselves; every per-task tally in this log reproduced exactly, with one
wording note recorded below.

| verdict | count | entries |
|---|---|---|
| `TOOL-DEFECT` | **32** | T1.4, T1.6, T1.10, T2.1, T2.4, T3.2, T3.6, T3.7, T3.9, T3.10, T4.4, T4.5, T4.10, T5.3, T5.6, T6.3, T6.7, T6.10, T7.2, T7.8, T8.2, T8.5, T9.6, T9.8, T9.9, T9.10, T9.11, T9.12, T9.13, T9.15, T9.18, T10.2 |
| `ERGONOMIC` | **16** | T1.5, T1.9, T2.5, T4.6, T4.7, T5.2, T5.8, T5.9, T6.6, T7.6, T7.12, T7.13, T8.4, T8.7, T9.5, T9.14 |
| `DOC-WRONG` | **9** | T3.13, T4.8, T5.5, T6.8, T7.7, T7.10, T7.16, T8.6, T10.1 |
| `LANGUAGE-GAP` | **6** | T5.4, T7.1, T8.1, T8.3, T9.3, T9.4 |
| `SPEC-WRONG` | **6** | T5.7, T6.2, T6.11, T7.3, T9.7, T9.19 |
| `DOC-GAP` | **3** | T1.7, T2.2, T9.1 |
| `AUTHOR-ERROR` | **1** | T3.8 |

The other 37 entries carry the protocol's non-verdict dispositions: **26 *worked well***,
**3 recurrences** of an earlier verdict, explicitly not re-counted (T7.5, T7.11, T8.9),
**2 declined verdicts** with the reasoning stated (T7.9, T8.8), and **6 notes or
measurements** (T1.11, T7.14, T8.11, T8.12, T9.17, T10.3). 73 + 37 = 110.

**One recount note.** T7's summary claims "three *worked well*"; I count two *entries*
(T7.4, T7.15), because its third is an embedded observation inside T7.13, whose own
verdict line is `ERGONOMIC`. T7's summary says so itself. The two numbers describe
different things and neither is wrong; this table counts entries.

**Read `TOOL-DEFECT` 32 correctly.** It is not 32 broken features. The criterion is "the
language and its docs are fine, and a tool is wrong about its own contract", so a high
count here is a *specific* reading, and it is the log's central one: the analysis
overwhelmingly exists and the reporting layer loses it. `E-BAD-ENUM` renders a speaker as
a `::directive` that does not exist. `check-project` and `compile` disagree about whether
a nested manifest exists. `trace` calls a relation unproducible in project-wide language
using a document-local judgement, contradicting the warning that sent you there.
`E-MAYBE-UNSET` says "no guard" five characters right of one. Most are small individually
and most are cheap to fix; the pattern is what matters.

#### The retractions

**28 of the 110 entries carry an explicit self-correction, and 7 of those changed a
verdict.** That is roughly one entry in four revised against itself, and it is the main
reason this log should be trusted over its own first drafts. Every correction moved in the
direction of a *weaker* claim.

The seven reclassifications: **T1.10** (*worked well* → `TOOL-DEFECT` — its original
conclusion, "nearest manifest wins", was re-probed and **overturned**; the entry now warns
later readers not to carry it forward); **T1.6** and **T2.1** (`DOC-GAP` → `TOOL-DEFECT`,
both because the first pass claimed the docs were silent when the pages in fact carry the
answer — the entries say the `DOC-GAP` claim "inflated the reading"); **T3.8**
(`TOOL-DEFECT` → `AUTHOR-ERROR`, downgraded once it was clear the docs state `{{…}}`
plainly and single braces are legitimate prose); **T5.4** (`ERGONOMIC` → `LANGUAGE-GAP`,
on a controller amendment to the criterion); **T5.7** (filed as fitting no verdict, then
escalated — the `SPEC-WRONG` row exists because of it); and **T9.7** (`TOOL-DEFECT` →
`SPEC-WRONG`, because both evaluators are doing exactly what the spec tells them).

The factual retractions that most matter, because a reader would otherwise have carried
them forward:

- **T9.7** withdrew two claims outright. It had said "the false side of every rule in the
  project is untestable" and that a passing test "documents a world that cannot happen".
  Both false: `lute run` runs the fixpoint, so the negative controls are available today,
  and the over-seeded test documents the game's most-travelled route. The residual finding
  is sharper *and smaller* than the original.
- **T7.2** carries a headed `RETRACTION`. It had called `E-CLIP-OVERLAP` "wrong in both
  directions" and named the permissive half "the worse half". The permissive half does not
  exist — the author had probed zero-duration clips, which are degenerate intervals — and
  the diagnosis of the mechanism was also wrong: the comparison is a correct half-open
  test, and the defect is float accumulation.
- **T8.5** had escalated to "every mock-suite proof in the language is vacuous". One
  `--help` disproved it: `lute test` traces, and a matched pair shows it refusing the
  guard-false selection with a passing control. The defect is one tool wide.
- **T8.4** caught **T7.13**'s hand arithmetic being wrong, which is the only correction
  here that had already cost something: the wrong number produced a dead line written into
  a scene, corrected before commit.
- **T2.2** retracted "`line.action` is a pass-through that nothing reads" — all three of
  its citations were the `::auto` code path. **T2.3** had stated the `Option<bool>` exit
  guarantee exactly backwards. **T8.6** found a retraction its own first draft had missed,
  and it was the load-bearing sentence. **T9.11** and **T9.16** withdrew the unqualified
  "scenario tests do not rot silently". **T8.11**, **T8.8** and **T8.12** each corrected a
  miscount that had been stated in multiple places.

What none of the corrections did was reverse a blocker. T3.2, T6.10, T9.18 and T8.3 were
each probed adversarially, several of them twice by different agents, and each survived.

---

## Findings

<!-- Task agents append below. One `### T<N> — <short title>` section per task,
     with entries inside. Never rewrite another task's section. -->

### T1 — Scaffold and declare

Toolchain 0.9.0 / language 0.9.0 / IR 0.9.0, `./target/debug/lute`.

#### T1.1 — `lute init`'s scaffold checks clean as generated — WORKED WELL

- **Intent** — start a real project from nothing and find out whether the
  scaffolder's output is a working baseline or a thing you must first repair.
- **Attempt** — `lute init docs/examples/anseo`, then immediately, before
  touching a byte: `lute check-project docs/examples/anseo`.
- **Result** — `ok: docs/examples/anseo (1 file(s), 0 project-wide warning(s))`,
  exit 0. Six files, and the four that participate in checking are mutually
  consistent: `opening.lute` imports both schemas, uses only members
  `vocabulary.schema.yaml` declares, and sets only a path `world.schema.yaml`
  declares.
- **Resolution** — n/a.
- **Verdict** — worked well. Worth stating plainly because the alternative is
  common and awful: a scaffolder whose first act is to hand you diagnostics.

#### T1.2 — the generated vocabulary is a real starting point, not a stub — WORKED WELL

- **Intent** — judge whether `init`'s `vocabulary.schema.yaml` survives contact
  with a project that has its own content, or gets deleted wholesale.
- **Attempt** — read it, then replace it with Anseo's vocabulary (brief Step 3).
- **Result** — the generated file declares **all seven** slots the compiler
  types (`emotion`, `action`, `anchor`, `mood`, `volume`, `musicAction`,
  `vfxType`), and its comments teach the two structural rules you would
  otherwise learn from an error: `action` must carry `exits:`, `anchor` must
  carry `default:`, and it says *why* ("the compiler reads those instead of
  guessing from names"). It also states the 0.9.0 ownership model up front —
  "Lute's compiler ships NO members".
- **Resolution** — Anseo's vocabulary is a **member-for-member substitution into
  the generated skeleton**. Seven slots in, seven slots out; both long-form
  slots kept their long form for the same reason the comment gives. I edited
  values and deleted nothing structural.
- **Verdict** — worked well. This is the single most load-bearing thing `init`
  produced. Under 0.9.0 vocabulary ownership, an author who has never heard of
  `exits:` or `default:` is one `E-DOMAIN-UNKNOWN` away from confusion, and the
  scaffold pre-empts it by making the seven slots an edit rather than a
  discovery. Note the phrasing in the file itself — "Declared up front so
  reaching for one is an edit to THIS list rather than an `E-DOMAIN-UNKNOWN`" —
  someone designed this deliberately, and it paid off here.

#### T1.3 — replacing the schemas out from under the placeholder scene: clean, pointed diagnostics — WORKED WELL

- **Intent** — the scaffold's scene is not my story. Replace both schemas and
  find out whether the toolchain tells me the placeholder now references
  vocabulary and state that no longer exist, or whether I get something
  downstream and confusing.
- **Attempt** — replaced `vocabulary.schema.yaml` first, checked; then
  `world.schema.yaml`, checked. Deliberately did *not* delete `opening.lute`
  first, so the dangling references would be live.
- **Result** — exactly the right errors, at the right spans, naming the right
  fix:
  ```
  scenes/opening.lute:15:20: error [E-BAD-ENUM] `delighted` is not a valid value for `emotion` of `::narrator` (expected one of: level, clipped, frayed, hollowed, wry, stricken)
  scenes/opening.lute:16:8: error [E-UNDECLARED] `::set` target `run.greeted` is not declared in the `state:` schema (dsl §7.3.4) (+1 more: 17:17)
  ```
  Both enumerate the legal alternatives or name the schema section to add to.
  The `(+1 more: 17:17)` roll-up is a nice touch — one entry per *problem*, not
  per occurrence.
- **Resolution** — deleted `scenes/opening.lute` per Step 5; both errors cleared.
- **Verdict** — worked well. Schema-edit blast radius is visible and precise.

#### T1.4 — `E-BAD-ENUM` renders a content line's speaker as a `::directive` that does not exist — TOOL-DEFECT

- **Intent** — n/a authorially; first seen in T1.3's output, then probed
  deliberately, because a diagnostic naming a construct the language does not
  have is the protocol's highest-priority category.
- **Attempt** — reproduced from scratch, outside Anseo, so this entry reruns
  without the example tree:
  ```console
  $ ./target/debug/lute init /tmp/t14/proj
  $ cp docs/examples/anseo/vocabulary.schema.yaml /tmp/t14/proj/vocabulary.schema.yaml
  $ ./target/debug/lute check-project /tmp/t14/proj
  ```
  The offending source is the scaffolder's own line 15, untouched:
  `@narrator{emotion="delighted"}: Welcome to your new Lute project.`
- **Result** — exit 1:
  ```
  /tmp/t14/proj/scenes/opening.lute:15:20: error [E-BAD-ENUM] `delighted` is not a valid value for `emotion` of `::narrator` (expected one of: level, clipped, frayed, hollowed, wry, stricken)
  failed: /tmp/t14/proj/scenes/opening.lute (1 error(s), 0 warning(s))
  failed: /tmp/t14/proj (1 file(s), 0 project-wide error(s), 0 project-wide warning(s))
  ```
  There is no `::narrator` directive in the language, in the file, or in the
  nine-directive list `lute context` prints (T1.6). An author who trusts the
  message and searches for `::narrator` finds nothing.
- **Second probe — it is the renderer, not this line.** One scratch file carrying
  a real directive and a content line, each with one bad enum value. Same
  frontmatter as the scaffold's scene, `episode: 2`, saved as
  `/tmp/t14/proj/scenes/probe.lute`:
  ```lute
  ## Probe

  ::auto{character="narrator" anchor="nowhere" action="brace"}
  @narrator{action="jitter"}: A line with a bad action.
  ```
  `./target/debug/lute check /tmp/t14/proj/scenes/probe.lute --project /tmp/t14/proj`:
  ```
  /tmp/t14/proj/scenes/probe.lute:15:37: error [E-BAD-ENUM] `nowhere` is not a valid value for `anchor` of `::auto` (expected one of: port, center, starboard)
  /tmp/t14/proj/scenes/probe.lute:16:19: error [E-BAD-ENUM] `jitter` is not a valid value for `action` of `::narrator` (expected one of: brace, drift, turn-away, seal, unseal, step-out, go-under)
  ```
  `::auto` is right; `::narrator` is fabricated. The `::` is prefixed to the
  owning node's name unconditionally, so this is how *every* content-line enum
  error renders — `action` as much as `emotion` — not a one-off in T1.3's output.
- **Resolution** — none needed. The spans (`15:20`, `16:19`) are correct and land
  on the offending attribute, so the cost is seconds, not minutes.
- **Verdict** — `TOOL-DEFECT`. The language is fine and so are its docs:
  `language/dialogue-and-cast.md` opens by stating the content-line form
  `@speaker{attributes}: the text they say`. A *tool* is describing the author's
  source as a construct that does not exist. Small in cost, but it is a one-word
  fix (`@narrator`) on a shared code path, and it is the cheap kind of wrong — a
  message that invents vocabulary the author will then go looking for.

#### T1.5 — `lute context` cannot answer "what may I write here?" until the file exists — ERGONOMIC

- **Intent** — before authoring `wake.lute`, ask the tool what the authoring
  surface for that scene is. This is the exact moment an author (or an AI) most
  needs the answer: the file is not written yet.
- **Attempt** — `lute context docs/examples/anseo/scenes/wake.lute`
- **Result** — `lute: cannot read …/wake.lute: No such file or directory (os error 2)`, exit 2.
- **Resolution** — ran `lute context` against the *placeholder* scene instead and
  read the surface off that. Works, because the surface is project-resolved, but
  it is an indirection: you must already have a valid document in the project to
  ask what documents in the project may contain. In a project scaffolded by
  `init` there is always one; in a project where you have just deleted the
  placeholder — which Step 5 instructs you to do — there is not.
- **Verdict** — `ERGONOMIC`. The command's own help says it emits the surface
  "an AI needs to WRITE valid Lute against THIS file's project", and it
  "emits regardless of document diagnostics" — it will happily describe a file
  full of errors, but not a file that does not exist yet. A `--project <DIR>`-only
  invocation with no `<FILE>` would close this; the flag already exists.

#### T1.6 — `lute context` gives you the vocabulary but not the grammar it advertises — TOOL-DEFECT

This is the direct measurement of authoring-surface maturity, so it gets the
detail.

- **Intent** — determine, honestly, whether `lute context` alone would have let
  me write `wake.lute` without the brief.
- **Attempt** — `lute context docs/examples/anseo/scenes/opening.lute` (37 lines,
  1363 bytes) and `--json` (14 top-level keys: `assetKinds`, `capabilityVersion`,
  `components`, `deliveryFlags`, `directives`, `entities`, `enums`, `facts`,
  `projectEnums`, `providers`, `relations`, `reservedQuestPaths`, `rules`,
  `stateSchema`).
- **Result — what it gave me, and it is substantial:**
  - all 9 core directives **with their attribute keys** — `auto: character, anchor, action`
    is precisely what I needed for line 10 of `wake.lute`;
  - all 7 project enums with every member, live against the schema I had just
    written (`anchor: port, center, starboard`) — so it is genuinely
    project-resolved, not a static core dump;
  - the state schema, entities, relations, and the derived rule, rendered in a
    compact readable form (`can_halt/1(crew) [derive]`);
  - the three delivery flags **with prose glosses** (`{mono}: interior monologue / thought (not spoken aloud in-scene)`)
    — the one place in the whole output that explains a *form* rather than
    listing a *value*;
  - `capabilityVersion`, which is the right thing to pin a harness on.
- **Result — what it left out, all of which `wake.lute` needs:**
  1. **The content-line form itself.** Nothing in the output says a spoken line
     is `@speaker{attrs}: text`. `emotion` appears under `projectEnums` with its
     six members, but nothing connects it to any construct — the `directives`
     block lists attribute keys per directive, and `emotion` is not among them,
     because it is a *line* attribute. So the output tells you `clipped` is a
     legal `emotion` while never telling you where an `emotion` may be written.
  2. **`code`.** Absent entirely — not in the human outline, not in the JSON
     (`grep '"code"'` over the JSON surface: no match). Yet `code` is the
     author-supplied half of every `lineId` and `voiceKey`, it is the one
     attribute in `wake.lute` with no vocabulary backing it, and the
     zero-padded-by-tens convention (`0010`, `0020`) is nowhere either. An
     author working from `context` alone writes lines with no `code` — which
     checks clean, and silently yields *positional* identity. Verified on a
     scratch project: two bare `@narrator:` lines compile to `…narrator_0010`
     and `…narrator_0020`; insert one line above them, recompile, and those two
     unchanged lines become `_0020` and `_0030`.
     `language/dialogue-and-cast.md` is accurate and careful here — a missing
     `code` "is back-filled deterministically at compile time and can be
     persisted with `lute tag`", i.e. deterministic per compile, not stable
     across edits, which is why `lute tag` exists at all. `context` mentions
     neither `code` nor `lute tag`.
  3. **Frontmatter.** No `kind:`, `character:`, `season:`, `episode:`, and —
     most damaging — no `uses:`. `uses:` is the mechanism that puts the enums
     and state the output is *describing* into scope. The surface describes the
     contents of a room without mentioning the door.
  4. **Section/shot headings.** `## Cold Wake` has no representation.
  5. **`enums (0):` next to `projectEnums (7):`** — two enum sections, the first
     empty and unexplained. Reading top-to-bottom, "enums (0)" is the first
     thing that looks like an answer to "what emotions may I use?" and it is the
     wrong one.
- **Resolution** — wrote `wake.lute` from the brief. From `context` alone I
  could have produced the `::auto` line correctly and every *value* correctly,
  and would have had to guess the frontmatter, the `@speaker{…}:` form, the
  heading, and `code` — i.e. the whole grammar.
- **Verdict** — `TOOL-DEFECT`, not `DOC-GAP`. Every form listed above is
  documented on the shipped website, and I checked each before assigning this:
  - the **content-line form** — `language/dialogue-and-cast.md`, which opens
    "Every content line has the same shape: `@speaker{attributes}: the text they
    say`", then names the line attributes with `code` first;
  - the **frontmatter block** — `language/frontmatter-and-profiles.md`, "Every
    `.lute` document opens with a **YAML frontmatter block** delimited by two
    `---` lines", with `kind`/`character`/`season`/`episode` in its worked example;
  - **`uses:`** — `language/imports.md`, whose title is literally "Imports
    (uses:)" and whose first paragraph names it as the import mechanism;
  - the **`## ` heading** — `getting-started/first-scene.md`, which teaches it
    through `E-CONTENT-OUTSIDE-SHOT` and states the rule as "all content lives
    under a heading".

  So the `DOC-GAP` bar is not met, and claiming it inflated the reading: I did
  not have to open Rust, a proposal, or a test, and a working author would not
  have to either. What failed is the tool's own contract. `lute context --help`
  reads: *"Emit the project-resolved AUTHORING SURFACE for a `.lute` file — the
  directives/attrs/enums/asset-kinds/providers/state-schema/components +
  capabilityVersion an AI needs to WRITE valid Lute against THIS file's
  project."* Read closely, the noun list is honest — every item in it is
  vocabulary, and every item in it is delivered, well. The overclaim is the
  purpose clause. What the output contains is not what an AI needs to write valid
  Lute; it is precisely the half the docs deliberately *delegate* to it.
  `dialogue-and-cast.md` makes that division explicit from the other side —
  "Their *domains* are project vocabulary, not grammar — run `lute context
  <file>` to list the legal `emotion`/`variant` values for your project." The
  docs own the forms and hand off the values; `context` owns the values and
  claims the whole surface.

  That is the `TOOL-DEFECT` criterion exactly: the information exists, and the
  tool that promised to hand it to you did not. It is also worse in practice than
  a documentation hole would be, because the output gives no signal that anything
  is missing — no cross-reference, no form section, not even an explanation of
  the empty `enums (0)`. It reads complete, and a harness pointed at it (which
  the help text invites) has no way to discover otherwise. The cheapest honest
  fix is still not to add grammar to `context`, but to stop claiming it in
  `--help` and to have the output name the pages that carry the forms.

#### T1.7 — the `identity:` block is documented only as an error-code entry — DOC-GAP

- **Intent** — write `identity:` templates that fix `lineId`/`voiceKey` for the
  whole eleven-scene work, and find out how discoverable the closed token set is.
- **Attempt** — the brief supplied the block, so I probed discoverability
  independently, both reactively and proactively.
- **Result — reactive discovery is excellent.** A scratch project with a bogus
  token:
  ```
  lute: E-IDENTITY-TEMPLATE: unknown token `{scene}` in identity template `lineId`; valid tokens are {prefix}, {speaker}, {code}
  ```
  Exit 1. The message names the offending token, the offending template key, and
  **enumerates the entire closed set**. This is a model diagnostic.
- **Result — proactive discovery fails.** Searching the shipped website
  (`packages/website/src/content/docs`) for the token syntax returns exactly one
  hit, `tooling/ai-harness.md:108`, and it is the `E-IDENTITY-TEMPLATE` **error-code
  reference** — i.e. the same information as the diagnostic, filed where you look
  after you have already failed. Specifically:
  - **no worked example of the `identity:` block exists on any authoring page**
    — searching for `voiceKey` across the docs finds it only as a *field in
    compiled JSON output* (`getting-started/first-scene.md`, `plugins/manifests.md`,
    `tooling/runtime-contract.md`), never as a manifest key you may set;
  - **`{prefix}`'s derivation is documented nowhere.** That it expands to
    `{character}.s{season}ep{episode}`, drawn from *scene frontmatter* and
    zero-padded to two digits, appears in no prose. It is inferable only by
    pattern-matching the string `"mira.s01ep01.mira_0010"` in a sample artifact
    against that page's frontmatter. I confirmed the rule empirically instead —
    `character: narrator, season: 1, episode: 1` → `narrator.s01ep01`;
    `character: anseo` → `anseo.s01ep01` — which is the right way to confirm it
    and the wrong way to learn it;
  - `lute init` does **not** scaffold an `identity:` block, and `lute context`
    does not surface identity at all, so neither entry point mentions the
    feature's existence.
- **Resolution** — used the brief's block verbatim; it compiled first try (T1.8).
- **Verdict** — `DOC-GAP`, and a clean instance of the harsh bar. Everything an
  author needs is *technically* present, but reaching it requires knowing the
  feature exists, guessing the YAML shape, and reading an error-code appendix or
  a 0.8.0 proposal. `docs/proposals/scenario-dsl/0.8.0.md` is the only file in
  the repo outside the website that documents it — a proposal, explicitly named
  in the verdict table.
- **Mitigating and worth saying:** the **defaults already are** `{prefix}.{speaker}_{code}`
  and `{speaker}-{code}`. I verified this by compiling the untouched scaffold
  before adding any block: `"lineId": "narrator.s01ep01.narrator_0010"`. So an
  author who never discovers the feature still gets sane, stable identity. The
  gap costs you *control*, not correctness.

#### T1.8 — identity verification: exact, first try — WORKED WELL

- **Attempt** — brief Step 6, `compile` + grep.
- **Result** —
  ```
  "lineId": "anseo.s01ep01.vesna_0010"
  "lineId": "anseo.s01ep01.vesna_0020"
  "voiceKey": "vesna-0010"
  "voiceKey": "vesna-0020"
  ```
  Both `lineId`s match the expected values exactly; `voiceKey` (not asked for,
  checked anyway) matches its template too.
- **Verdict** — worked well. No adjustment to the template was needed or made.

#### T1.9 — a mock left pointing at deleted state rots silently under `check-project` — SILENCE

- **Intent** — none authorial; this is the failure mode the protocol says is
  most expensive, and it fell out of Step 3–5. Task 1 replaces `world.schema.yaml`
  and deletes `opening.lute` while leaving `mocks/playthrough.yaml` as the
  scaffolder wrote it — by instruction.
- **Attempt** — after the schema swap and the deletion, `lute check-project docs/examples/anseo`.
- **Result** — `ok: docs/examples/anseo (1 file(s), 0 project-wide warning(s))`,
  exit 0. The mock still reads:
  ```yaml
  # Trace mock (dsl 0.4.0 §4.3) for scenes/opening.lute. Preview with:
  #   lute trace scenes/opening.lute --mock mocks/playthrough.yaml
  state:
    run.greeted: false
  ```
  Both a state path that no longer exists **and** two references to a scene file
  that no longer exists. `check-project` says nothing — mocks are not in its
  walk, so the project is green with a broken mock in it.
- **The error does exist, one command over.** `lute trace scenes/wake.lute --mock mocks/playthrough.yaml`:
  ```
  docs/examples/anseo/scenes/wake.lute:0:0: error [E-TRACE-MOCK-UNDECLARED] `--state run.greeted=…` names a state path not declared in the resolved schema (state-by-typo MUST fail in mocks exactly as in documents, dsl 0.4.0 §4.3, 0.1 §11.1.1)
  ```
  So the rule is implemented and stated forcefully — "MUST fail in mocks exactly
  as in documents" — but it only fires when a human happens to run `trace` with
  that pairing. Nothing pairs them automatically, and nothing in the green
  `check-project` hints that an unpaired mock is sitting there.
- **Resolution** — none. Left as instructed; recorded rather than fixed.
- **Verdict** — `ERGONOMIC`, shading toward the protocol's *silence* category.
  The scaffolder emits a mock; the checker cannot see it; the mock's only
  validator is a command you must remember to run with the right two arguments.
  In an eleven-scene project this is how a `mocks/` directory quietly becomes
  fiction. A project-wide `check-project` pass over `mocks/*.yaml` (or a
  `W-MOCK-ORPHANED`) would close it.
- **Secondary, and a genuine misdirect:** the diagnostic's position is
  `scenes/wake.lute:0:0`. The defect is in `mocks/playthrough.yaml` at line 4.
  It is rendered "exactly like check diagnostics" — which here means it is
  rendered as a *source* diagnostic against a file that is not at fault, at the
  impossible position `0:0`. The message body names `run.greeted`, so you
  recover, but the filename and span both point away from the problem. Per the
  protocol this outranks most of what is above it.

#### T1.10 — which manifest governs a nested project is decided by the root you invoke, not by proximity — TOOL-DEFECT

Re-run from scratch during the fix pass, because the original entry described its
probes only in prose. **The re-run overturns its conclusion.** The original read
"nearest manifest wins" and was filed *worked well*; nearest manifest does not
win. What follows is what the commands actually print.

- **Intent** — `docs/examples/anseo` is a project *inside* the `docs/examples`
  project, and acceptance requires both `check-project docs/examples/anseo` and
  `check-project docs/examples` to pass. I need to know which manifest governs
  Anseo's scenes when the outer root is the one being walked — otherwise the
  `identity:` block I just wrote is decorative for ten of the eleven scenes.

- **Attempt (a) — is the nested scene walked at all?**
  ```console
  $ ./target/debug/lute check-project docs/examples
  ```
- **Result (a)** — exit 0, closing with
  `ok: docs/examples (30 file(s), 5 project-wide warning(s))`, 31 `ok:` lines, and
  among them:
  ```
  ok: docs/examples/anseo/scenes/wake.lute (0 warning(s))
  ```
  The nested scene is walked, not skipped. This part of the original entry holds.

- **Attempt (b) — is a nested manifest discovered?** A two-manifest scratch tree,
  built entirely by `lute init` so it pastes and runs:
  ```console
  $ lute init /tmp/nest && lute init /tmp/nest/inner
  $ printf '\nidentity:\n  lineId: "{scene}.{speaker}_{code}"\n' >> /tmp/nest/inner/lute.project.yaml
  $ lute check-project /tmp/nest
  ```
  `{scene}` is not a legal token (T1.7), and it is in the **inner** manifest only.
  Baseline before the edit: `lute check-project /tmp/nest` is exit 0 with both
  scenes `ok:`.
- **Result (b)** — exit 1, and this is the entire output:
  ```
  lute: E-IDENTITY-TEMPLATE: unknown token `{scene}` in identity template `lineId`; valid tokens are {prefix}, {speaker}, {code}
  ```
  So nested manifests *are* read and validated by `check-project`: a broken one
  two directories down fails the outer run. Note what the outer run does **not**
  say — which manifest. The line is byte-identical to what
  `lute check-project /tmp/nest/inner` prints, carries no path, and the walk emits
  no `ok:` lines before it. In a tree with two manifests you are told a manifest
  is broken and left to find out which. (Recorded here rather than as its own
  entry: same defect class as T1.9's `0:0` span — a project-level diagnostic with
  no usable location.)

- **Attempt (c) — whose templates land in the artifact?** The original entry's
  probe, done properly: give the two manifests *mutually distinguishable*
  templates instead of testing one against a default.
  ```console
  $ lute init /tmp/nest3 && lute init /tmp/nest3/inner
  # append to /tmp/nest3/lute.project.yaml:
  #   identity:
  #     lineId: "OUTER-{prefix}.{speaker}_{code}"
  # append to /tmp/nest3/inner/lute.project.yaml:
  #   identity:
  #     lineId: "INNER-{prefix}.{speaker}_{code}"
  # and in /tmp/nest3/inner/scenes/opening.lute, so the two scaffolds do not
  # collide on episode key: character: narrator  ->  character: inner
  $ lute compile --all --project /tmp/nest3 -o /tmp/nest3out
  ```
- **Result (c)** — exit 0, `lute compile --all: 2 document(s) -> /tmp/nest3out`,
  and every `lineId` in both artifacts — including the **nested** project's —
  carries the outer template:
  ```
  "lineId": "OUTER-inner.s01ep01.narrator_0010"
  "lineId": "OUTER-inner.s01ep01.narrator_0020"
  "lineId": "OUTER-narrator.s01ep01.narrator_0010"
  "lineId": "OUTER-narrator.s01ep01.narrator_0020"
  ```
  `INNER-` appears nowhere. Compiling at the inner root gives it back, and
  single-file compiles follow `--project`, never proximity:
  ```console
  $ lute compile --all --project /tmp/nest3/inner -o /tmp/nest3in
  "lineId": "INNER-inner.s01ep01.narrator_0010"

  $ lute compile /tmp/nest3/inner/scenes/opening.lute --project /tmp/nest3        -> "OUTER-inner.s01ep01.narrator_0010"
  $ lute compile /tmp/nest3/inner/scenes/opening.lute --project /tmp/nest3/inner  -> "INNER-inner.s01ep01.narrator_0010"
  $ lute compile /tmp/nest3/inner/scenes/opening.lute                             -> "inner.s01ep01.narrator_0010"
  ```
  The last is the default template: with no `--project`, **no** manifest is
  consulted — not even the one sitting in the file's own project directory.

- **Attempt (d) — do the checker and the compiler agree?** The (c) tree with the
  inner manifest's template set to the illegal `"{scene}.{speaker}_{code}"` from
  (b), everything else unchanged.
- **Result (d)** — they do not:
  ```console
  $ lute check-project /tmp/nest4
  lute: E-IDENTITY-TEMPLATE: unknown token `{scene}` in identity template `lineId`; valid tokens are {prefix}, {speaker}, {code}
  # exit 1

  $ lute compile --all --project /tmp/nest4 -o /tmp/nest4out
  lute compile --all: 2 document(s) -> /tmp/nest4out
  # exit 0, every lineId prefixed OUTER-
  ```
  One command refuses to proceed over a manifest the other never reads.

- **Corrected conclusion.** **The invoked root wins; the nearest manifest does
  not.** `--project <DIR>` *is* the manifest selector — a `lute.project.yaml`
  closer to the document is not preferred, and with no `--project` none is used.
  `check-project` additionally walks and validates nested manifests, but
  validating a manifest is not the same as letting it govern.

- **What this means for Anseo, plainly.** Anseo's `identity:` block governs only
  when Anseo is compiled *as its own root*. Compiled from `docs/examples`, its
  scenes take the outer manifest's templates. Today that is invisible:
  ```console
  $ lute compile docs/examples/anseo/scenes/wake.lute --project docs/examples      -o /tmp/a-outer.json
  $ lute compile docs/examples/anseo/scenes/wake.lute --project docs/examples/anseo -o /tmp/a-own.json
  $ cmp -s /tmp/a-outer.json /tmp/a-own.json && echo identical
  identical
  ```
  Both give `"lineId": "anseo.s01ep01.vesna_0010"` and `"voiceKey": "vesna-0010"`.
  But that is **coincidence, not resolution**: `docs/examples/lute.project.yaml`
  declares no `identity:` block, and the defaults are exactly
  `{prefix}.{speaker}_{code}` / `{speaker}-{code}` (T1.7) — which is what Anseo's
  block sets. The day `docs/examples` grows an `identity:` block, every Anseo
  artifact built from the outer root silently changes its `lineId`s, and
  `check-project` stays green through it. (The outer-root invocation also prints
  five `lute: E-PROFILE-UNKNOWN` lines for *other* examples' profiles while still
  exiting 0 and emitting the artifact — noted, not pursued; it belongs to
  whichever task touches profiles.)

- **Verdict** — `TOOL-DEFECT`. Not for "invoked root wins" — that is a defensible
  design and it is what the flag's own help says (`compile --project <DIR>`:
  "Project directory (`lute.project.yaml` + `plugins/`) resolving the document's
  activated capability snapshot"; `--all` "Compile EVERY `*.lute` document under
  `--project <dir>`"). The defect is that `check-project` and `compile` disagree
  about whether a nested manifest exists at all — (d) shows one failing the build
  over a file the other does not open. The website states the opposite guarantee
  (`language/frontmatter-and-profiles.md`: "The checker, LSP, and compiler all
  validate the document against the same resolved capability snapshot, so what
  checks clean is exactly what compiles"), and nothing anywhere warns that a
  nested project's manifest is inert under an outer-root build.

- **Correction of record.** The original probe (c) put a distinctive template on
  the **outer** manifest and left the inner one at its defaults, so an
  un-prefixed `lineId` was consistent with *both* hypotheses — and it was read as
  proof of the wrong one. Later tasks must not carry "nearest manifest wins"
  forward; if a task needs Anseo's identity templates to apply, it must compile
  with `--project docs/examples/anseo`.

#### T1.11 — environment note, not a language finding

The editor LSP in this workstation reported five errors on a `wake.lute` that
the CLI checks clean — including `E-SHOT-HEADING` on `## Cold Wake`,
`E-UNCLASSIFIED` on both valid `@vesna{…}:` lines, and an `anchor` domain of
`left, center, right` (the *scaffold's* members, which I had already replaced).
Cause: `/usr/local/bin/lute-lsp` is an **unrelated product** — `lute --version`
there prints `[deprecated] 'lute' is now 'bard lute'` and `0.1.0`. A name
collision, not a Lute defect. Recorded only because an author who installs
"lute" tooling by name can end up with a language server that confidently
contradicts the compiler, and nothing in either tool says why.

#### T1 summary

Eleven entries: four *worked well*, three `TOOL-DEFECT`, two `ERGONOMIC` (one of
them the silent-mock case), one `DOC-GAP`, one environment note. Nothing in
Task 1 was inexpressible — every construct the brief asked for compiled, and the
identity chain landed exactly on `anseo.s01ep01.vesna_0010` first try.

The friction is almost entirely *informational*, and the fix pass moved where it
sits. Only **one** entry is a genuine hole in the documentation: T1.7, the
`identity:` block, where the tool knows things (the closed token set, the
derivation of `{prefix}`) it will tell you only after you have guessed wrong. The
other three findings are tools misreporting a world the docs describe correctly —
`lute context` promises the write-surface and ships the vocabulary half of it
while the website carries the forms (T1.6); `E-BAD-ENUM` renders every content
line's speaker as a `::directive` that does not exist (T1.4); `check-project` and
`compile` disagree about whether a nested project's manifest exists (T1.10).

That is a better reading of 0.9.0 than the first pass gave — the language and its
docs are in better shape than the tools that describe them — and a worse one for
anyone trusting a tool's own account of itself. One thing must not be carried
forward from the first pass: T1.10's original "nearest manifest wins" is wrong.

### T2 — The exits proof

Toolchain 0.9.0 / language 0.9.0 / IR 0.9.0, `./target/debug/lute`. One beat added
to `scenes/wake.lute`: Vesna decides to take the second pod, and goes back under.

#### T2.1 — an exit written on the line that *is* the departure is accepted, kept, and silently not an exit — TOOL-DEFECT

- **Intent** — Vesna says she is taking the second pod, and returns to cryo. One
  beat: the line, and the character leaving on it.
- **Attempt** — the departure written where the departure happens, on the line
  that *is* the departure:
  ```lute
  @vesna{code="0030" emotion="hollowed" action="go-under"}: If the second pod's intact, I'm taking it.
  ```
  Nothing about this form is speculative. `action` is a documented **line**
  attribute — `language/dialogue-and-cast.md`, "Line attributes": `code`,
  `emotion`, `variant`, **`action`**, `dialogMotion`, `as` — and `go-under` is a
  declared member of the `action` domain, declared in its `exits:`.
- **Result — silence, at every gate I could reach:**
  ```console
  $ lute check docs/examples/anseo/scenes/wake.lute --project docs/examples/anseo
  ok: … (0 warning(s))                                           # exit 0
  $ lute check … --deny-warnings
  ok: … (0 warning(s))                                           # exit 0
  $ lute check-project docs/examples/anseo
  ok: docs/examples/anseo (1 file(s), 0 project-wide warning(s))  # exit 0
  $ lute compile … -o /tmp/t2-probe.json                          # exit 0
  ```
  The artifact keeps the attribute and drops the *exit*:
  ```json
  {"kind":"line","addr":"001-0500","role":"dialogue","speaker":"vesna",
   "text":"If the second pod's intact, I'm taking it.","emotion":"hollowed",
   "action":"go-under","lineId":"anseo.s01ep01.vesna_0030","voiceKey":"vesna-0030"}
  ```
  `[c for c in commands if c.get('exit')]` → `[]`. Vesna never leaves, and she is
  still on stage for the rest of the scene: the only two callers of
  `is_declared_exit` (`inject.rs:193`, `lower.rs:183`) are both on `::auto`'s
  `action`, and the one that removes a character from `StageState.on_stage`
  (`inject.rs:191-197`) is never reached from a content line.

  Scope, stated once so the rest of the entry is precise: it is the **exit**
  reading that is inert in this position, not the attribute. A content line's
  `action` is read, and it emits commands — see T2.2.
- **Is there ANY signal that separates the two positions?** Every surface a
  working author has:
  - `lute check --json` — `"diagnostics": []`, and `resolved.commands_preview`
    renders the whole run as `["::auto", ":vesna", ":vesna", ":vesna"]`. No exit,
    and no way to see one is missing.
  - `lute context` (human) — `auto: character, anchor, action`. Content lines are
    absent from the output entirely (T1.6), so it lists `action` as an `::auto`
    attribute and never mentions the line position. Nothing says either position
    ends a presence.
  - `lute context --json` — **the one place in the toolchain where the fact
    exists.** The `auto` entry carries
    `"semantics": ["reads.onStage","usesAnchor","mayExitCharacter","writes.characterState"]`.
    `mayExitCharacter` is the machine-readable statement that `::auto` is the
    construct that can end a presence. It is dropped from the human rendering of
    the same command, and it appears **nowhere** in
    `packages/website/src/content/docs/` — no hit across the shipped site. The
    only files in the repo that name it are `crates/`, `docs/architecture.md`,
    `docs/plugin-system.md`, and two proposals.
  - `lute trace` and `lute run` — both render every sprite record with no action
    and no exit marker (`<auto>` and `sprite` respectively), so neither preview
    would have shown me the beat was missing.

  Nothing they run tells them.
- **And the checker already warns about this exact shape elsewhere.**
  `check-project docs/examples` emits, twice, over other examples:
  ```
  warning [W-INJECT-CONFLICT] `bianca` is shown with an explicit `anchor="center"` that `auto-anchor-on-show` would otherwise inject
  ```
  A `W-` code whose entire job is "this staging attribute you wrote is not doing
  what you think it is doing". So the precedent exists. So does the information:
  the resolved `action` domain is demonstrably in hand at the content-line check —
  T1.4 has it enumerating all seven members in an `E-BAD-ENUM` on a line's
  `action=` — and `is_declared_exit` is `pub` for exactly this reason. The warning
  is simply not written.
- **Resolution** — staged the departure as its own directive, i.e. the beat
  written as two events instead of one:
  ```lute
  @vesna{code="0030" emotion="hollowed"}: If the second pod's intact, I'm taking it.
  ::auto{character="vesna" action="go-under"}
  ```
- **Verdict** — `TOOL-DEFECT`, and the `DOC-GAP` this was first filed as does not
  survive contact with the pages.

  What the website *does* say, plainly, checked before assigning this:
  `language/directives.md` — "Character staging lives on `::auto` with an action
  id (there is no `::sprite`/`::char`) … a character exit is
  `::auto{action="fade-out-down"}`"; and `language/vocabulary.md` §"Member
  semantics" — the `exits:` members are "the members that end a character's
  presence on stage", and such a member "lowers to a `sprite` record carrying
  `exit: true`". A `sprite` record is what `::auto` lowers to. So the working
  form is one sentence on the shipped site, an author who read `directives.md`
  first would have written the `::auto`, and I did not have to open Rust, a
  proposal, or a test to *find the form*. The `DOC-GAP` bar is not met, and
  claiming it inflated the reading in the same way T1.6's first pass did.

  What fails is a tool, and it fails in the criterion's own words — "a false
  green". The checker holds the resolved `action` domain at the content-line
  check (T1.4 is the proof: `E-BAD-ENUM` enumerates all seven members on a
  line's `action=`), it has `is_declared_exit` exported for the purpose, it has
  a precedent warning of exactly this shape in `W-INJECT-CONFLICT`, and it
  declines to say that a declared-exit member in this position ends nothing.

  It is the protocol's *silence* case in its expensive form. The document is
  green, the string survives into the artifact where a reader will see
  `"action":"go-under"` on the line and assume it means something, and the beat
  is simply absent. One `W-` code closes it — and separately, one sentence in
  `dialogue-and-cast.md` would have kept an author out of the position entirely
  (T2.2).

#### T2.2 — the website never says what a content line's `action` does, and it does something — DOC-GAP

- **Intent** — having found the *exit* reading inert on a line (T2.1), establish
  what `action` in that position actually is. The convenient answer — "nothing" —
  is the one to distrust, so this is checked against the compiler rather than
  assumed from T2.1's silence.
- **Attempt** — read every page that offers the attribute or would carry its
  semantics; then, finding none, read the checker and compile a probe.
- **Result — the documentation, all four surfaces:**
  - `language/dialogue-and-cast.md` offers `action` as one of six line attributes
    and assigns it **no semantics** — "Their *domains* are project vocabulary,
    not grammar". No mention of `::auto`, no cross-reference to `directives.md`.
  - `language/directives.md` attaches "character entrance/exit/pose" to `::auto`.
    So the one plausible meaning of the word is documented on a *different*
    construct, on a page the first one does not point at.
  - `tooling/runtime-contract.md` never lists `action` among a `line` record's
    fields, although the compiler puts it there (`lower.rs:38-49`, `action:
    get("action")`).
  - `posReset` and `auto-pose-reset` — the things a line's `action` actually
    causes — appear **nowhere** in `packages/website/src/content/docs/`. No hit
    across the shipped site.
- **Result — the source. It is read, in two places, and both matter:**
  - `stage_bookkeeping_line` (`crates/lute-check/src/inject.rs:390-397`) writes it
    to the speaker's `SpriteState.pose`;
  - `line_is_stateful` (`inject.rs:405-412`) counts `action` among the four
    sprite-affecting slots, so such a line marks the speaker `dirty`, and a
    *later plain line* from that speaker gets an injected `posReset` under rule
    `auto-pose-reset` (`inject.rs:311-341`).

  This is artifact-visible, and the two scratch scenes differ in exactly one
  attribute. Probe (`/tmp/t2fix/anseo/scenes/pose.lute`):
  ```lute
  ## Cold Wake
  ::auto{character="vesna" anchor="port" action="brace"}
  @vesna{code="0010" action="drift"}: A.
  @vesna{code="0020"}: B.
  ```
  ```console
  $ ./target/debug/lute compile /tmp/t2fix/anseo/scenes/pose.lute \
      --project /tmp/t2fix/anseo -o /tmp/t2fix/pose.json          # exit 0
  ```
  ```json
  {"kind":"sprite","addr":"001-0100","character":"vesna","anchor":"port","action":"brace"}
  {"kind":"line","addr":"001-0200","role":"dialogue","speaker":"vesna","text":"A.","action":"drift", …}
  {"kind":"sprite","addr":"001-0300","character":"vesna","posReset":true,
   "provenance":{"injected":true,"by":"auto-pose-reset",
    "reason":"`vesna` had a dirty pose before a plain line; resetting to neutral"}}
  {"kind":"line","addr":"001-0400","role":"dialogue","speaker":"vesna","text":"B.", …}
  ```
  Control (`ctrl.lute`), identical but for dropping `action="drift"` from line
  `0010`: three records, no `posReset`, nothing injected. So a content line's
  `action` emits a command the author did not write, one address later.
  (`emotion` also trips `line_is_stateful`; the `SpriteState.pose` write at
  `inject.rs:395-397` is `action`'s alone, and the control here carries no
  attributes at all, so `action` is the only variable.)
- **Resolution** — none available to an author: this entry *is* the missing
  documentation. `::auto` remains the way to write an exit (T2.1).
- **Verdict** — `DOC-GAP`, and it stands on its own precisely because it is not
  T2.1. T2.1 is about a tool that stays quiet; this is about a page that does not
  exist. What is **not** claimed here is that the exit rule is unstated — it is
  stated, on `directives.md`, and T2.1 turns on that fact. The hole is narrower
  and it is real: the site hands authors an attribute on one page, assigns its
  one plausible meaning to a different construct on another, lists neither in the
  runtime contract's `line` record, and documents the semantics the attribute
  *actually* has — pose state, statefulness, an injected `posReset` — on no page
  at all. I learned them by compiling the probe above and reading `inject.rs`.
  That is the harsh bar met in its literal form: a working author cannot read
  `inject.rs`, and here there is nowhere else to look. Two sentences in
  `dialogue-and-cast.md` close it — what `action` on a line is for, and that a
  character exit is `::auto`.

  **Correction to this entry's first pass**, recorded rather than quietly edited,
  because the protocol's whole value is that its entries are true. The first pass
  asserted that `line.action` is "a pass-through that nothing reads", citing
  `lower.rs:178-198` and `inject.rs:192,432`. All three citations are the
  `::auto` path — `lower.rs:178-198` is the `"auto"` arm of `lower_directive`,
  `inject.rs:192` is `lower_auto`'s exit branch, `inject.rs:432` is
  `is_declared_exit` itself. None of them is the line path, and the assertion
  they were offered for is false.

#### T2.3 — the proof, and its negative control — WORKED WELL

- **Attempt** — brief Steps 2 and 3, the second run as a real control rather than
  a formality: change nothing but the member, `go-under` → `drift`. Both are
  declared members of the same `action` domain; both are equally opaque strings;
  only one is in `exits:`.
- **Result** — the two artifacts differ in exactly one key, at the same address:
  ```json
  go-under: {"kind":"sprite","addr":"001-0600","character":"vesna","action":"go-under","exit":true}
  drift   : {"kind":"sprite","addr":"001-0600","character":"vesna","action":"drift"}
  ```
  Positive: `[c for c in commands if c.get('exit')]` → exactly one record.
  Negative: `[]`. `check --deny-warnings` is clean in both directions, which is
  the point — `drift` is not an error, it is simply not an exit.
- **Verdict** — worked well, and it is the strongest single thing measured so far.
  `exit` is derived from one declared list, in one file, by one function both the
  checker and the compiler call — `is_declared_exit` (`inject.rs:432`), whose only
  two callers are `inject.rs:193` and `lower.rs:183`. It is `Option<bool>`, set to
  `Some(true)` or `None`, so a non-exit omits the key entirely rather than
  serializing `"exit": false`.

  State that guarantee correctly, because this entry's first pass had it exactly
  backwards. `Option<bool>` does **not** let a consumer tell "not an exit" from
  "unset" — those are the *same* absent field, and the encoding collapses the
  distinction rather than preserving it. What the design actually buys is that
  there is nothing left to distinguish: the compiler writes `exit` for precisely
  the declared-exit members and never writes `false`, so absence is total and the
  consumer's rule is one line — **no `exit` key means not an exit.** The negative
  control above is what makes that rule checkable rather than asserted, and it is
  a real guarantee; it is just not the one first claimed.

  Nothing in this vocabulary would have survived the deleted
  `fade-out*`/`exit*`/`hide` heuristic: `go-under` and `step-out` would both have
  been missed, and `drift` would have been correctly ignored only by accident.
  That is the whole argument for the declaration, demonstrated rather than
  asserted.

#### T2.4 — a character exits, keeps speaking, and exits again: `ok`, zero warnings, two `exit: true` — TOOL-DEFECT

- **Intent** — in the committed scene the exit is last, and reading it back
  (T2.5) position is doing all the work of telling a reader that Vesna is gone.
  Find out whether position is doing the *checker's* work too: is "a character
  who left does not speak" a rule the toolchain enforces, or a property my scene
  happens to have?
- **Attempt** — scratch copy of the example so nothing committed moves —
  `cp -R docs/examples/anseo /tmp/t2fix/anseo` — plus one added scene,
  `/tmp/t2fix/anseo/scenes/stage_state.lute`, verbatim:
  ```lute
  ---
  kind: scene
  character: anseo
  season: 1
  episode: 2
  uses: [../vocabulary.schema.yaml]
  ---

  ## Cold Wake
  ::auto{character="vesna" anchor="port" action="brace"}
  @vesna{code="0010" emotion="clipped"}: Cryo's gone. We don't go back under.
  ::auto{character="vesna" action="go-under"}
  @vesna{code="0020" emotion="level"}: So we walk.
  ::auto{character="vesna" action="go-under"}
  ```
  A declared exit; then an ordinary dialogue line from a character who is no
  longer on stage; then a second exit for a character who already left. Three
  things that cannot all be true of one performance.
- **Result — clean, at the strictest setting the CLI offers:**
  ```console
  $ ./target/debug/lute check /tmp/t2fix/anseo/scenes/stage_state.lute \
      --project /tmp/t2fix/anseo --deny-warnings
  ok: /tmp/t2fix/anseo/scenes/stage_state.lute (0 warning(s))       # exit 0
  $ ./target/debug/lute check-project /tmp/t2fix/anseo
  ok: /tmp/t2fix/anseo/scenes/stage_state.lute (0 warning(s))
  ok: /tmp/t2fix/anseo/scenes/wake.lute (0 warning(s))
  ok: /tmp/t2fix/anseo (2 file(s), 0 project-wide warning(s))       # exit 0
  $ ./target/debug/lute compile /tmp/t2fix/anseo/scenes/stage_state.lute \
      --project /tmp/t2fix/anseo -o /tmp/t2fix/stage_state.json     # exit 0
  ```
  Zero diagnostics of any severity, and the artifact carries the contradiction
  straight through:
  ```json
  {"kind":"sprite","addr":"001-0100","character":"vesna","anchor":"port","action":"brace"}
  {"kind":"sprite","addr":"001-0200","character":"vesna","preload":true,"emotion":"clipped", …}
  {"kind":"line","addr":"001-0300","role":"dialogue","speaker":"vesna","text":"Cryo's gone. We don't go back under.", …}
  {"kind":"sprite","addr":"001-0400","character":"vesna","action":"go-under","exit":true}
  {"kind":"line","addr":"001-0500","role":"dialogue","speaker":"vesna","text":"So we walk.", …}
  {"kind":"sprite","addr":"001-0600","character":"vesna","action":"go-under","exit":true}
  ```
  `[c for c in commands if c.get('exit')]` → **two** records. A runtime is told to
  hide the sprite, then to play a line from it, then to hide it again.
- **This is not a missing analysis. The state exists, it is correct, and it is
  read on the very next node.**
  - The checker removes the character on the first declared exit — `lower_auto`'s
    exit branch calls `state.on_stage.remove(&character)` (`inject.rs:191-197`).
    So at line `0020` the reducer already knows Vesna is off stage.
  - It then *consults* that knowledge, for a different purpose, on that exact
    line. `auto-pose-reset`'s guard is
    `!stateful && state.dirty.contains(speaker) && state.on_stage.contains_key(speaker)`
    (`inject.rs:319`). The third conjunct is false; the only consequence is that
    an injection is skipped. Absence is used as "nothing to reset" and never as
    "the author staged something impossible".
  - The second `::auto` never tests presence at all: the exit branch fires on
    `is_declared_exit` alone, and its `remove` is a no-op on an absent key
    (`inject.rs:191-197`).
- **Resolution** — `NONE — nothing to resolve; the probe is the finding.` The
  committed scene is correct by construction, not by verification, and I have no
  way to make the toolchain confirm the difference.
- **Verdict** — `TOOL-DEFECT`, taking the table in order.
  - Not `LANGUAGE-GAP`: nothing is inexpressible. The correct scene and the
    incoherent one are both writable; the story never had to change.
  - Not `ERGONOMIC`: the working form is not more verbose or more indirect, it is
    *unverified*. The cost lands on a scene that is wrong rather than one that is
    awkward, which is a different kind of cost.
  - Not `DOC-GAP`: no page's absence caused this, and no page could fix it. There
    is nothing for `directives.md` to say — "a character who has exited cannot
    speak" is not knowledge an author lacks.
  - Not `AUTHOR-ERROR`: I did not miss a documented rule; I wrote a scene that
    contradicts itself and the tool called it `ok`.

  That leaves `TOOL-DEFECT`, and the criterion fits word for word: the language
  and its docs are fine, and the tool is "lying about its own contract" — this is
  the "false green" the table names explicitly.

  **Weight.** This is the most serious thing T2 found, and it is more serious than
  T2.1, which is the same silence over a shape the checker has no state for.
  Here the checker has the state, has it correct, and reads it one conjunct away
  from the diagnostic. `--deny-warnings` is the strongest promise the CLI makes;
  an author or a CI harness that trusts it ships staging that no runtime can
  perform, with no diagnostic, no warning, and an artifact that looks deliberate.
  The precedent is already in the codebase — `W-INJECT-CONFLICT` (T2.1) exists to
  say "this staging attribute is not doing what you think it is doing", so the
  severity tier and the reporting path are settled. What is missing is a
  `contains_key` on the line arm and one on the exit branch.

#### T2.5 — the finished source cannot tell you which `::auto` is the exit — ERGONOMIC

- **Intent** — read the shot back as an author who did not write it.
- **Attempt** — the committed scene, in full:
  ```lute
  ## Cold Wake
  ::auto{character="vesna" anchor="port" action="brace"}
  @vesna{code="0010" emotion="clipped"}: Cryo's gone. We don't go back under.
  @vesna{code="0020" emotion="level"}: So we walk.
  @vesna{code="0030" emotion="hollowed"}: If the second pod's intact, I'm taking it.
  ::auto{character="vesna" action="go-under"}
  ```
- **Result** — the entrance and the exit are the same construct with the same
  attribute names, and the entire difference between "Vesna is now on stage" and
  "Vesna is gone" is which of `brace` and `go-under` appears in a list in
  `../vocabulary.schema.yaml`. Position is a hint, not a rule: the exit happens to
  be last here, nothing requires that, and nothing checks it — see **T2.4**, where
  a scene that exits, keeps speaking, and exits again is `ok: … (0 warning(s))`
  under `--deny-warnings`. No author-facing surface annotates the difference in
  place: `lute trace` prints both directives as `<auto>`, `lute run` prints both
  records as `sprite`.
- **Resolution** — none; the source stands as written, and the adjacent
  line/directive pair reads acceptably here only because the line carries
  `emotion=` and the directive carries `action=`. Had the beat wanted both, the
  two adjacent lines would be genuinely ambiguous to a reader. The one command
  that helps is `lute doctor`, which prints the resolved semantics on one line:
  `• vocabulary slots declared: emotion, action (exits: step-out/go-under), anchor (default: center), …`
- **Verdict** — `ERGONOMIC`, and scoped to readability alone now that the
  unchecked-staging half of it is T2.4. This is the deliberate 0.9.0 trade and the
  entry is not an argument against it: a declared list beats a name prefix
  precisely *because* `go-under` is unguessable. But the cost is real and it lands
  on the reader rather than the writer — staging semantics are now non-local, and
  the three tools that render a scene for a human (`trace`, `run`, `context`'s
  human mode) each discard the one bit that says a character left. `trace`
  printing `<auto exit>`, or `context` keeping the `semantics` flags its own
  `--json` already carries, would close it without touching the language.

#### T2 summary

Five entries: two `TOOL-DEFECT`, one `DOC-GAP`, one `ERGONOMIC`, one *worked
well*. The mechanism under test is sound — the negative control is clean, the
field is absent rather than false, and the declaration does exactly the work the
heuristic used to guess at. Everything that went wrong is on the *approach* to it
and on the *verification* of it. On the approach: the position that carries the
exit is stated on one page, the position that does not is offered on another with
no semantics at all even though it has them, and no preview tool shows the
difference in the result. On the verification: the checker accepts a declared-exit
member on a content line without a word (T2.1) and accepts a character speaking
after they have left (T2.4), the second while holding the state that refutes it.
An author gets the exit right by having read the correct page first, or by
compiling and diffing the JSON — and gets no help at all in finding out they got
the staging wrong.

### T3 — The shed clock as declared state

Toolchain 0.9.0 / language 0.9.0 / IR 0.9.0, `./target/debug/lute`. One scene added:
`scenes/cryobank.lute`, `anseo.s01ep02` — the project's first `<branch>`, its first
`::set`, its first `::assert`, and the route ancestor of everything downstream.

The scene carries a design claim: **Lute has no engine clock.** `run.shedPressure` is
the shed schedule, and it advances only where an author wrote `::set`. T3.1 is the
proof; T3.2 is what happens when the author writes one that is quietly wrong.

#### T3.1 — the counter is the natural form, and there is no clock behind it — WORKED WELL

- **Intent** — waking crew costs clock. Cracking a pod draws power the Purser bills
  against the shed schedule; the engineer costs more than the navigator, and leaving
  both under costs nothing. Three choices, two of which move a number.
- **Attempt** — the form I reached for first, unmodified, inside the choice bodies:
  ```lute
  <choice id="wakeToma" label="Wake the engineer">
  ::set{run.shedPressure += 2}
  ```
  Nothing was substituted to make this compile. It is what a counter looks like.
- **Result** — `check-project docs/examples/anseo` → `ok: … (2 file(s), 0 project-wide warning(s))`,
  exit 0, first try. The artifact carries the increments as state-write commands,
  one per arm, at addresses *inside* the arm:
  ```json
  {"kind":"set","addr":"001-0600","path":"run.shedPressure","op":"+=","value":"2","expr":{"lit":2.0}}
  {"kind":"set","addr":"001-1100","path":"run.shedPressure","op":"+=","value":"1","expr":{"lit":1.0}}
  ```
  Both the surface form (`op: "+="`, `value: "2"`) and the parsed form (`expr`) are
  emitted, so a consumer can round-trip the author's text or evaluate the tree.
- **The no-clock claim, with its negative control.** The whole command inventory of
  the compiled scene is `{sprite: 2, line: 5, choice: 1, set: 2, assert: 4, jump: 3}`
  — no tick, no timer, no time-driven kind, and the strings `tick`/`timer`/`clock`/
  `elapsed`/`narrativeTime` appear nowhere in the artifact. Executed against the
  reference runtime with each arm forced:
  ```console
  $ lute run /tmp/t3-cryobank.json --mock ok_wakeToma.yaml      # choose: { whoWakes: wakeToma }
    001-0600  set    run.shedPressure = 2      -> final run.shedPressure = 2
  $ lute run … --mock ok_wakeIlsabet.yaml      -> final run.shedPressure = 1
  $ lute run … --mock ok_wakeNobody.yaml       -> final run.shedPressure = 0
  ```
  The third is the control that matters: the `wakeNobody` arm contains no `::set`, so
  no `set` command exists on that path and the schedule **does not move**. Nothing in
  the engine advances it on its own. The language also refuses to let you invent a
  clock: declaring `run.clock: { type: narrativeTime }` is rejected, and
  `facts-and-datalog.md` states the one narrative-time anchor an author may write is
  `quest.<id>.activatedAt`. So "the schedule advances only because an author wrote
  `::set`" is not a convention this example adopts — it is the only thing available.
- **A rule the scene silently depends on, and it is enforced.** `+=` reads the old
  value first, so a compound assignment needs the path to be already-assigned.
  `run.shedPressure` carries `default: 0`, which is why the bare `+=` is legal.
  Removing the default from `world.schema.yaml` and re-checking:
  ```
  error [E-MAYBE-UNSET] state path `run.shedPressure` may be read before it is set
  (no default, no dominating `::set`, no guard) (dsl §9.4)
  ```
  `state-model.md` states this rule in one sentence and the checker enforces it
  exactly. (Schema restored; the probe was on a scratch copy.)
- **Resolution** — none needed; the first form written is the committed form.
- **Verdict** — worked well. The natural expression *is* the working expression, the
  increment survives to the artifact unmodified, and the design claim the scene
  carries is demonstrable rather than asserted.

#### T3.2 — a `::set` right-hand side is not typed against the path it writes; the runtime then eats it — TOOL-DEFECT

This is T3's most serious finding, and it is the failure mode a counter cannot survive.

- **Intent** — none authorial. It fell out of asking the assignment's question "does
  anything tell you a `::set` target is a state path you declared?" — the answer for
  the *target* is an excellent yes (T3.4). So I asked the same question of the
  *value*, because a number that silently fails to increment is the single worst thing
  that can happen to this scene.
- **Attempt** — three writes to `run.shedPressure`, declared `{ type: number, default: 0 }`:
  ```lute
  ::set{run.shedPressure += "two"}                    # string into a number
  ::set{run.shedPressure = true}                      # bool into a number
  ::set{run.shedPressure += (run.shedPressure > 0) * 3}   # bool arithmetic
  ```
- **Result — all three check clean at the strictest setting:**
  ```console
  $ lute check … --deny-warnings
  ok: /tmp/t3/anseo/scenes/c_strnum.lute  (0 warning(s))    # exit 0
  ok: /tmp/t3/anseo/scenes/c_boolnum.lute (0 warning(s))    # exit 0
  ok: /tmp/t3/anseo/scenes/c_paren.lute   (0 warning(s))    # exit 0
  ```
  All three compile, and the reference runtime — `lute run`, described by its own help
  as "the reference consumer of the runtime contract" — carries them through without a
  word:
  ```console
  $ lute run strnum.json     001-0100  set  run.shedPressure = 0      -> final = 0
  $ lute run boolnum.json    001-0100  set  run.shedPressure = true   -> final = true
  $ lute run paren.json      001-0100  set  run.shedPressure = 0      -> final = 0
  ```
  Exit 0 on all three. `0 += "two"` is silently **0** — the counter does not advance
  and nothing anywhere says so. `= true` is worse: the reference runtime writes a
  boolean into a path the schema declares `number`, and the final-state dump prints
  `run.shedPressure = true`. The `type:` in the schema is not enforced at either end.
- **The asymmetry is the point, and it is inside one construct of each other.** The
  same schema, the same path, the same compiler run:
  - **Relation arguments are typed to the member.** `::assert{awake(nobody)}` →
    `E-FACT-DOMAIN`, naming the entity kind and the argument index (T3.4).
  - **`into=`/`value=` is typed to the path.** `<choice … into="run.shedPressure">`
    without a `value` → `E-INTO-VALUE`: *"`value` is required for `run.shedPressure`
    (only a `bool` path defaults to `true`)"*. That diagnostic can only exist because
    the checker knows this path is a `number` at that moment.
  - **`::set`'s value is typed to nothing.** One construct away, holding the same
    knowledge, it accepts a string.
- **Resolution** — `NONE — nothing to resolve; the committed scene writes integer
  literals and is correct by construction, not by verification.` I have no way to make
  the toolchain confirm the difference.
- **Verdict** — `TOOL-DEFECT`, taking the table in order.
  - Not `LANGUAGE-GAP`: the counter is perfectly expressible, and T3.1 expresses it.
  - Not `ERGONOMIC`: the working form is not more awkward, it is *unverified*.
  - Not `DOC-GAP`: `state-model.md` is unambiguous — state is "a set of **typed** paths
    (`number`, `bool`, `string`, `enum`)", every path "MUST be declared with a `type`".
    No page's absence caused this and no page could fix it.
  - Not `AUTHOR-ERROR`: I did not miss a documented rule; the tool accepted a write the
    documentation says is ill-typed.

  That leaves `TOOL-DEFECT`, in the criterion's own words: the language and its docs
  are fine, and the tool is "lying about its own contract" — a declared type that
  binds nothing. It is also the protocol's *silence* case in the form that costs most.
  A mistyped `emotion` is caught instantly (`E-BAD-ENUM`, T1.4); a mistyped *number*
  reaches the player as a counter that stopped counting, through a green
  `--deny-warnings`, a green compile, and a green reference run. For a work whose
  central mechanic is a schedule that advances, this is the bug you would ship.

#### T3.3 — `::assert` inside a `<choice>` scopes to the arm and reaches a downstream gate — WORKED WELL

Recorded prominently because Task 4's quest gate depends on it, and because a
negative here would have been load-bearing for the next task.

- **Intent** — waking a crew member should be a durable fact about the run, not a
  scene-local flag: Task 4 must be able to ask "is Toma awake, and does he know the
  shed sequence?" from a different document, in a different episode.
- **Attempt** — the assertions written where the waking happens, inside the arm:
  ```lute
  <choice id="wakeToma" label="Wake the engineer">
  ::set{run.shedPressure += 2}
  ::assert{awake(toma)}
  ::assert{knows(toma, shed_sequence)}
  ```
- **Result** — the facts land inside the arm, before its jump to the converge point,
  so they are conditional on the selection rather than unconditional in the scene:
  ```json
  {"kind":"choice","addr":"001-0500","branchId":"whoWakes","recordKey":"scene.choices.whoWakes",
   "options":[{"id":"wakeToma",…,"target":"001-0600"},{"id":"wakeIlsabet",…,"target":"001-1100"},
              {"id":"wakeNobody",…,"target":"001-1600"}],"converge":"001-1800"}
  {"kind":"set",   "addr":"001-0600","path":"run.shedPressure","op":"+=","value":"2"}
  {"kind":"assert","addr":"001-0700","relation":"awake","args":["toma"]}
  {"kind":"assert","addr":"001-0800","relation":"knows","args":["toma","shed_sequence"]}
  {"kind":"line",  "addr":"001-0900",…,"speaker":"toma"}
  {"kind":"jump",  "addr":"001-1000","target":"001-1800"}
  ```
  Four `assert` records total, two per waking arm; the `wakeNobody` arm has none.
- **The full chain to Task 4's gate, verified end to end.** A scratch scene asserts in
  a choice arm, then queries the *derived* relation in a later shot:
  ```lute
  @vesna{code="0030" emotion="level" when="holds(can_halt(toma))"}: Then you can stop the shed.
  @vesna{code="0040" emotion="hollowed" when="!holds(can_halt(toma))"}: Nobody here can stop it.
  ```
  `check --deny-warnings` clean; the guards lower to `match` records carrying the
  compiled predicate:
  ```json
  {"kind":"match","addr":"002-0100","subject":"holds(can_halt(toma))","arms":[{"test":"(holds(can_halt(toma)))",…}]}
  ```
  So `::assert{awake(toma)}` + `::assert{knows(toma, shed_sequence)}` in a choice arm
  → the `world.schema.yaml` rule `can_halt(C) :- awake(C), knows(C, shed_sequence)` →
  a `holds()` guard in a later document. The whole path Task 4 needs is live. Both
  base relations are `tier: run`, so they survive to the next episode, which
  `scene.choices.whoWakes` explicitly does not (`choices-and-hubs.md`: that path
  "clears at episode end").
- **Verdict** — worked well, without qualification. This is the single most important
  thing T3 needed to be true and it was true first try.
- **One documentation wrinkle, filed separately.** The page that enumerates the
  built-in `::`-directives scopes `::assert` / `::retract` to quest documents, which
  is false in exactly the way this entry demonstrates. It is now its own entry —
  **T3.13**, `DOC-WRONG` — because it is a defect in the docs, not in the construct.
  The verdict here is unaffected: the construct itself worked first try.

#### T3.4 — the relational and state-path diagnostics are the best surface measured so far — WORKED WELL

This is the direct answer to "does declaring `entities:` earn its keep?", so it gets
the full transcript. Every probe is one scratch scene, one deliberate mistake.

```
::set{run.shedPresure += 2}
  10:7:  error [E-UNDECLARED] `::set` target `run.shedPresure` is not declared in the
         `state:` schema (dsl §7.3.4) — did you mean `run.shedPressure`?

::assert{knows(toma)}                              # wrong arity
  10:1:  error [E-RELATION-ARITY] relation `knows` expected 2 argument(s), got 1

::assert{awake(nobody)}                            # non-member
  10:1:  error [E-FACT-DOMAIN] `nobody` is not a declared member of entity kind `crew`
         (relation `awake` argument 0, dsl 0.3.0 §3.1)

::assert{awak(toma)}                               # typo'd relation
  10:1:  error [E-RELATION-UNKNOWN] unknown relation `awak` (dsl 0.3.0 §4)

::assert{knows(shed_sequence, toma)}               # arguments transposed
  10:1:  error [E-FACT-DOMAIN] `shed_sequence` is not a declared member of entity kind
         `crew` (relation `knows` argument 0, dsl 0.3.0 §3.1)
  10:1:  error [E-FACT-DOMAIN] `toma` is not a declared member of entity kind `topic`
         (relation `knows` argument 1, dsl 0.3.0 §3.1)

::assert{can_halt(toma)}                           # writing a derived relation
  10:1:  error [E-DERIVED-WRITE] relation `can_halt` is `derive: true`: it is computed
         by `rules:` and MUST NOT be asserted or retracted by content (dsl 0.3.0 §5)

<choice … when="run.vesnaTrst > 0">
  11:32: error [E-UNDECLARED] state path `run.vesnaTrst` is not declared in `state:`
         (dsl §9.4) — did you mean `run.vesnaTrust`?
```

- **Why this is the answer to the maturity question.** Six distinct failure modes, six
  distinct codes, and every one of them names the fix. The transposed-arguments case is
  the standout: a two-argument relation over two different entity kinds catches a swap
  that no amount of naming discipline would, and it reports **per argument index**, so
  the author is told which slot is wrong rather than that the fact "is invalid". This
  is exactly the value that `entities: { crew: …, topic: … }` buys, and it is
  unavailable in any design where `awake` is a string key.
- **Did-you-mean is on every state-path read and write**, including inside a `when=`
  guard and inside a `{{…}}` interpolation, so a typo'd path is a two-second fix
  wherever it appears.
- **The one blemish, small:** relation diagnostics all land at `10:1`, the start of the
  directive, never on the offending argument — including the transposition case, where
  two errors share one span. `::set` and `when=` are column-exact by contrast (`10:7`,
  `11:32`). The message body carries the argument index, so nothing is lost; it is one
  span computation short of perfect.
- **Verdict** — worked well, and it is the strongest counterweight in this log to T3.2.
  The relational layer's arguments are typed to the *member*. The scalar layer's
  `::set` value is typed to *nothing*. Same file, same compiler run, same author.

#### T3.5 — conditional availability works, is guarded against emptiness, and was discoverable — WORKED WELL

- **Intent** — a natural instinct on this beat: "waking the engineer should only be
  offered if Vesna trusts you", and separately "a pod already cracked cannot be cracked
  again". Both are conditional availability of a choice.
- **Attempt** — `when=` on `<choice>`, reached for by analogy with the content-line
  `when=` in `docs/examples/investigation/scenes/confrontation.lute`:
  ```lute
  <choice id="wakeToma" label="Wake the engineer" when="run.vesnaTrust > 0">
  <choice id="a" label="Wake the engineer" when="!holds(awake(toma))">
  ```
- **Result** — both check clean and both reach the artifact with the guard compiled to
  a tree beside its source text:
  ```json
  {"id":"wakeToma","label":"Wake the engineer","when":"run.vesnaTrust > 0",
   "expr":{"op":">","l":{"path":"run.vesnaTrust"},"r":{"lit":0.0}},"target":"001-0300"}
  {"id":"a","label":"Wake the engineer","when":"!holds(awake(toma))","target":"001-0200"}
  ```
  Scalar guards and relational-fact guards both work in choice position, which is what
  the "already cracked" instinct needed.
- **And the obvious way to break it is a hard error.** Guarding every choice:
  ```
  error [E-BRANCH-ALL-GUARDED] `<branch id="bAll">` has no unguarded `<choice>`; every
  choice carries a `when`, so the menu could be empty — a branch must contain at least
  one unguarded choice (dsl §11.1)
  ```
  That rule is why the committed scene's `wakeNobody` is unguarded, and the message
  explains the *reason* rather than just the rule. Neighbouring structural checks are
  equally pointed: `E-CHOICE-DUP` on a repeated choice id within a branch;
  `E-INTO-VALUE` (*"`value` is required for `run.shedPressure` (only a `bool` path
  defaults to `true`)"*) and `E-INTO-TARGET` (*"`into="awake(toma)"` must name a
  `run.<path>` fact"*) when `into=` is misused.
- **`into=`/`value=`: yes, discoverable, and I would have found it.** The assignment
  asks this directly. `language/branch-match-when.md` is the page you reach for when
  you write your first `<branch>` — its title is the construct — and it closes the
  `<branch>` section with: *"Choice mechanics — `when` guards, the `into=` run-record
  sugar, and revisit `<hub>`s — are covered in [Choices & hubs]."* All three of the
  things I wanted, named, in one sentence, with a link, on the page I was already on.
  `choices-and-hubs.md` then gives `into=`/`value=` a worked example and the exact rule
  I would have needed (`value` defaults to `true` only for a `bool` path). This is what
  the T1.6/T1.7 findings were complaining about the *absence* of, and here it is
  present. Worth saying plainly: the language docs did the job the tooling did not.
- **Why the committed scene uses `::assert` and not `into=` anyway** — not a
  workaround, a modelling choice the docs support. `into=` records a *scalar* into a
  `run.*` path; what Anseo needs downstream is a *relation* between a crew member and a
  topic, which `into=` cannot name (`E-INTO-TARGET` says so). The branch already
  records its own selection into `scene.choices.whoWakes` for free — visible in the
  artifact as `recordKey` and in `lute context` as
  `scene.choices.whoWakes: enum [wakeToma, wakeIlsabet, wakeNobody, unset]` — so the
  intra-episode half needs no author action at all.
- **Also checked, and reasonable:** an empty `<choice>` body compiles to a bare jump to
  the converge point while still recording the selection. A "say nothing, but remember
  it" option is expressible without a filler line.
- **Verdict** — worked well. Every conditional-availability idea I had was expressible
  in the form I first reached for.

#### T3.6 — reaching for a guard on `::set` misdirects, and the suggested fix does not parse — TOOL-DEFECT

- **Intent** — "waking the engineer costs more the later you do it": the increment
  should depend on how far the schedule has already advanced.
- **Attempt** — the first form I reached for was a guard on the write itself, by
  analogy with `when=` on lines and on choices, which is the only guard spelling the
  language has shown me:
  ```lute
  ::set{run.shedPressure += 3 when="run.shedPressure > 0"}
  ```
- **Result** — a diagnostic about the wrong thing entirely:
  ```
  10:33: error [E-CEL-PARSE] `=` assigns; comparison is `==` — did you mean
         `3 when=="run.shedPressure > 0"`? (dsl 0.4 §8.1)
  ```
  My `when=` was swallowed into the CEL expression on the right of `+=`, so the parser
  saw a stray `=` and offered the `==` fix it offers for `if (x = 1)`. The real problem
  is that `::set` has no attribute surface at all — it is `::set{path op celExpr}` and
  nothing else. Nothing in the message hints at that.
- **And the suggestion is not merely unhelpful, it is invalid.** Applying it verbatim:
  ```lute
  ::set{run.shedPressure += 3 when=="run.shedPressure > 0"}
  ```
  ```
  10:29: error [E-CEL-PARSE] not a valid condition expression:
         `3 when=="run.shedPressure > 0"` (dsl 0.4 §8.1)
  ```
  The tool proposed a repair and its own next run rejects it. An author who trusts the
  did-you-mean — and T3.4 shows did-you-mean is usually excellent here — is walked one
  step further from the answer.
- **Resolution — the intent is fully expressible, twice over, and neither form is
  worse than what I wanted.** The right-hand side is a complete CEL expression, so the
  scaling cost is a ternary:
  ```lute
  ::set{run.shedPressure += run.shedPressure > 0 ? 3 : 2}     # ok, exit 0
  ```
  and a genuinely guarded write is a `<match>`, which is the construct the language
  designates for state dispatch:
  ```lute
  <match on="run.shedPressure">
  <when test="$ > 0">
  ::set{run.shedPressure += 3}
  </when>
  <otherwise>
  ::set{run.shedPressure += 2}
  </otherwise>
  </match>                                                     # ok, exit 0
  ```
  The committed scene keeps the flat `+= 2` / `+= 1` because the beat wants a fixed
  price per pod, not a rising one — that is an authorial choice made after confirming
  the alternative works, not a substitution made to avoid finding out.
- **Verdict** — `TOOL-DEFECT`, and it is filed for the misdirection, not the missing
  attribute. There is no `LANGUAGE-GAP`: the intent is expressible two ways. There is
  no `DOC-GAP`: `state-model.md` gives the grammar as `::set{path <op> celExpr}` and
  `branch-match-when.md` gives `<match>`; both are on the shipped site. What fails is
  a diagnostic that names the wrong construct and then emits a repair that does not
  compile — the protocol's highest-priority category, "it said X, the real problem was
  Y", with the added cost that following its advice loses you a second round trip.
  A parse failure *inside a `::set` body* has enough context to say the useful thing:
  "`::set` takes no attributes; guard a write with `<match>`/`<when>`."

#### T3.7 — `lute context` says "directives (9)" and omits all four built-in `::`-directives — TOOL-DEFECT

- **Intent** — before writing the first branching scene, ask the tool what may go in
  it. T1.6 already established that `context` ships vocabulary and not grammar, so this
  entry is deliberately *not* that complaint: it is about an item that belongs to the
  vocabulary half, in a list `context` does emit, under a header that counts it.
- **Attempt** — `lute context docs/examples/anseo/scenes/cryobank.lute`, and `--json`.
- **Result** — both renderings list exactly nine directives:
  `auto, bg, camera, cut, end, music, sfx, vfx, video`.
  The four `::`-directives this scene is built on, or that any stateful scene is built
  on, are absent from both: **`::set`, `::assert`, `::retract`, `::use`.**
  `language/directives.md` names them explicitly as directives — its §"Reserved
  directives" opens *"Two `::`-directives are built-in rather than staging
  vocabulary"* — so this is not a category quibble about what the word means. Note
  that `::end` **is** in the list, and `::end` is core control flow, not staging
  vocabulary; so the list is not "staging directives only" either. It is nine of
  thirteen with no rule connecting them and a count that implies completeness.
- **What `context` does get right here, and it is a lot** — recorded so the entry is
  not one-sided. With `world.schema.yaml` in scope it renders the entities, the
  relations *with arity and argument kinds* (`knows/2(crew, topic)`), the derived
  marker (`can_halt/1(crew) [derive]`), the rule text, and the state schema — including
  `scene.choices.whoWakes: enum [wakeToma, wakeIlsabet, wakeNobody, unset]`, the
  reserved path my own `<branch>` had just brought into existence. For the relational
  layer, `context` is genuinely the best surface in the toolchain.
- **Resolution** — wrote the scene from the brief and the website. From `context` alone
  I could not have learned that `::set` or `::assert` exist.
- **Verdict** — `TOOL-DEFECT`, on the same criterion as T1.6 but for a different
  reason, and it is worth keeping the two apart. T1.6's missing items (the content-line
  form, frontmatter, headings, `code`) are *grammar*, which the docs deliberately own —
  `dialogue-and-cast.md` says so from the other side. A directive name is not grammar;
  it is the exact kind of project-resolved vocabulary this output exists to enumerate,
  it sits in a section headed `directives (N)`, and the parenthesised count asserts
  the list is whole. An AI harness pointed at `--json` — which the `--help` text
  invites — will never emit a `::set`, and nothing in the output signals an omission.

#### T3.8 — a single-brace `{run.shedPressure}` in a choice label is silently literal text — AUTHOR-ERROR

- **Intent** — make the price visible on the button: "Wake the engineer (schedule 4)".
  `choices-and-hubs.md` says a label "may interpolate" and gives no syntax on that page,
  so I guessed.
- **Attempt** — the three spellings a working author would try, in the order I tried
  them. Single braces first, because `{…}` is the attribute-block delimiter everywhere
  else in Lute (`@vesna{…}`, `::set{…}`), so it is the most Lute-shaped guess:
  ```lute
  <choice id="a" label="Wake the engineer (schedule {run.shedPressure})">
  <choice id="a" label="Wake the engineer (schedule ${run.shedPressure})">
  <choice id="a" label="Wake the engineer (schedule {{run.shedPressure}})">
  ```
- **Result — all three `ok`, exit 0 under `--deny-warnings`, and only one of them
  means anything:**
  ```json
  {run.shedPressure}    -> "label":"Wake the engineer (schedule {run.shedPressure})"
  ${run.shedPressure}   -> "label":"Wake the engineer (schedule ${run.shedPressure})"
  {{run.shedPressure}}  -> "label":"Wake the engineer (schedule {{run.shedPressure}})",
                           "placeholders":[{"kind":"path","path":"run.shedPressure"}]
  ```
  Two of the three reach the artifact as literal text a player will read off a button.
  Three mutually exclusive syntaxes, one diagnostic between them: none.
- **The checker is not blind here — it is looking one character too narrowly.** Inside
  the *correct* delimiter, a typo is caught with a suggestion:
  ```
  label="… {{run.shedPresure}}"
  11:1: error [E-UNDECLARED] state path `run.shedPresure` is not declared in `state:`
        (dsl §9.4) — did you mean `run.shedPressure`?
  ```
  Inside single braces, the identical typo is silent, because the whole span is text.
  So the resolver, the path table, and the did-you-mean machinery are all present and
  correct at that exact position; they are simply never asked.
- **Resolution** — the committed scene's labels are plain text, which is what the beat
  wanted anyway. The finding is the near-miss, not the label.
- **Verdict** — `AUTHOR-ERROR`. The docs say so plainly and I did not read them before
  guessing: `dialogue-and-cast.md` states the form — "Content `Text` (and a `<choice>`
  label) may embed **`{{…}}`** interpolations" — on the shipped site, not in Rust. Given
  that, `check` treating `{run.shedPressure}` as literal text is the *correct*
  behaviour, not a violated contract: single braces are ordinary prose, Lute never
  claimed them as an interpolation delimiter, and a tool that faithfully reproduces
  characters the language does not reserve is doing its job. The `W-` code I argued for
  below would be a **new lint heuristic**, i.e. a feature request — and a feature that
  does not exist cannot be a tool lying about its own contract. Downgraded from
  `TOOL-DEFECT` on that reasoning.
- **Why it is kept rather than deleted, stated plainly.** The `AUTHOR-ERROR` criterion
  admits an entry only "if the diagnostic pointed somewhere unhelpful", and **that
  clause does not apply here — there was no diagnostic at all.** It is kept under the
  other standing rule instead, *Also record, always → Silence*: I wrote something
  plausible, nothing complained, and it did not do what I meant. All three spellings
  are `ok` under `--deny-warnings` and two of them ship a state path to a player's
  button. That is the entry's whole value, and it is an observation about silence, not
  a claim of defect.
- **The wish, recorded as a wish.** The low-noise rule would be *single braces wrapping
  a string that resolves to a declared state path* — the checker already has the path
  table open at that span (it fires `E-UNDECLARED` with did-you-mean one character
  over), so a `W-` code there would hit essentially nothing else, with
  `W-INJECT-CONFLICT` (T2.1) as precedent. Separately, one sentence on
  `choices-and-hubs.md` linking to the interpolation section would have prevented the
  guess entirely; today that page says "may interpolate" and stops. Neither is a
  finding against 0.9.0 as shipped.

#### T3.9 — a broken state schema is reported as a count with no message, and the obvious way to look closer misparses it as a scene — TOOL-DEFECT

Found while probing whether an author may declare their own clock (T3.1). This scene
is the first in the project to `uses:` `world.schema.yaml`, so every later task's
schema edit lands on this diagnostic.

- **Intent** — declare `run.clock: { type: narrativeTime }` and find out whether the
  language lets an author invent a time axis.
- **Attempt** — one line added to a scratch `world.schema.yaml`, then `lute check` on a
  scene that imports it.
- **Result** — rejected, correctly, and unusably:
  ```
  scenes/g_clock.lute:1:1: error [E-USES-PARSE] schema import
  `/private/tmp/t3/anseo/world.schema.yaml` has parse/frontmatter errors (1 issue(s))
  ```
  A count. Not the issue. The message names the file and the number of problems in it
  and never the problem, and `--json` carries exactly the same single diagnostic with
  nothing nested inside it. A *hard YAML syntax error* in the same file produces the
  byte-identical shape — `(1 issue(s))` — so the author cannot even tell a typo'd type
  name from unbalanced brackets.
- **Every other way to look closer, and what each does:**
  - `lute check-project` — repeats the same count-only line once per importing
    document. In this project that is 32 identical lines saying nothing.
  - `lute doctor <project>` — **exit 0**, and prints
    `✓ content documents: 32 .lute file(s)` plus the vocabulary summary. It reports the
    project healthy while the state schema is broken.
  - `lute context <schema>` — exit 0, emits a surface with `stateSchema (0):`. Zero
    declared paths is exactly what an *empty* schema looks like, so the one output that
    could have shown the damage renders it as absence.
  - `lute check world.schema.yaml` — the natural next move, and the worst outcome. It
    parses the YAML schema **as a scene document**:
    ```
    world.schema.yaml:1:1: error [E-KIND-MISSING] required frontmatter key `kind` is
      missing; every root document must declare `kind: scene` or `kind: quest`
    world.schema.yaml:1:1: error [E-META-MISSING] required meta key `character` is missing
    world.schema.yaml:1:1: error [E-META-MISSING] required meta key `season` is missing
    world.schema.yaml:1:1: error [E-META-MISSING] required meta key `episode` is missing
    world.schema.yaml:2:3: error [E-UNCLASSIFIED] unrecognized line
    … one E-UNCLASSIFIED per line of the schema
    ```
    This is not a bad error, it is a *wrong* one: it is the same flood for a perfectly
    valid schema file, it never mentions the actual defect, and an author who follows
    its advice adds `kind: scene` to their state schema and destroys it.
  - There is no `lute explain`; `lute --help` lists no subcommand that opens a schema.
- **Resolution** — I recovered only because I had just typed the offending line and
  could bisect it out. An author who edits a schema and comes back an hour later has
  a count and a file.
- **Verdict** — `TOOL-DEFECT`, and the strongest sub-case is the misdirect, which the
  protocol ranks above almost everything: the one command whose name promises to check
  a file tells you your state schema is missing `kind:`, `character:`, `season:` and
  `episode:`. The information plainly exists — something produced the count `1`, so the
  underlying issue list is in hand and is discarded at the reporting boundary. That is
  the `TOOL-DEFECT` criterion word for word: "the information exists, but the tool that
  promised to hand it to you did not." Related to T1.9 and T1.10 in kind — this
  toolchain's project-level diagnostics repeatedly lose either their location (T1.9's
  `0:0`, T1.10's manifest with no path) or, here, their body.
- **Confirmed in passing:** `type: narrativeTime` is not author-declarable, which is
  the T3.1 claim's other half. The rule is stated on `facts-and-datalog.md`
  (`E-TEMPORAL-ARG`); the diagnostic that enforces it is the one above.

#### T3.10 — an unknown key in a mock file is silently ignored by both `trace` and `run` — TOOL-DEFECT

- **Intent** — drive each arm of the branch to prove the counter moves (T3.1's
  verification). Writing the mock from memory rather than the page, I guessed the key.
- **Attempt** —
  ```yaml
  state:
    run.shedPressure: 0
  selections:
    whoWakes: wakeToma
  ```
- **Result** — `lute run artifact.json --mock that.yaml` accepts the file, makes no
  selection, and stops:
  ```
  001-0500  choice [whoWakes] -> (none)
  -- final state --
    run.shedPressure = 0
    scene.choices.whoWakes = unset
  run incomplete
  ```
  Exit 3. Nothing says the mock contained a key it did not understand; "incomplete" is
  also what you get from a mock that is simply missing the selection, so the two are
  indistinguishable. I initially read this as "`--mock` does not carry selections to
  `run` at all" — which would have been a much bigger and entirely false finding — and
  only caught it by reading `tooling/tracing.md`, where the key is `choose:`.
  With `choose: { whoWakes: wakeToma }` it works perfectly and exits 0 (T3.1).
  `lute trace --mock` with the same bogus keys (`selections:`, `chose:`) is exit **0**
  and walks the scene as if no mock had been supplied.
- **Resolution** — used the documented key. The mock format is on the page, correctly,
  with all five surfaces in one YAML block.
- **Verdict** — `TOOL-DEFECT`. Not `AUTHOR-ERROR` — or rather, it began as one, and it
  is recorded because the diagnostic pointed nowhere, which the table says is exactly
  when to record one. The contract being broken is explicit: `trace --help` says it
  "refuses (exit 1) a document with check errors OR **invalid mocks**", and
  `tracing.md` enumerates six `E-TRACE-MOCK-*`/`E-TRACE-*` refusals — every one of them
  for a bad *value* (undeclared path, wrong literal type, unknown relation, ineligible
  choice). An unrecognised *key* is not among them and is not refused. A mock whose
  selection key is misspelled is an invalid mock; both tools read it, silently discard
  half of it, and report a result. `trace`'s header does whisper the truth —
  `(seeds: 1 paths, 0 facts; 0 selections)` — which is the only tell in either tool,
  and it is a count you have to already suspect. This compounds T1.9: mocks are the one
  part of a Lute project with no checker over them, and now with no key validation
  inside the tools that do read them.

#### T3.11 — the route-ancestor chain is verified project-wide, with did-you-mean — WORKED WELL

- **Intent** — this scene is the ancestor of everything downstream, so `after:` is
  load-bearing in a way it was not for `wake.lute`. Find out whether a wrong scene key
  is caught, or whether the eleven-scene graph can quietly come apart.
- **Attempt** — `after: 'visited("anseo.s01ep01")'` as committed, plus two deliberate
  breakages: a wrong episode (`anseo.s01ep99`) and a misspelled character
  (`anso.s01ep01`).
- **Result** — both caught at project level, both with a suggestion:
  ```
  scenes/a_typo.lute:7:1: error [E-CONN-UNKNOWN-NODE] unknown node: no scene resolves to
    key `anseo.s01ep99` (`visited`, dsl §2.3/§4.1) — did you mean `anseo.s01ep01`?
  scenes/a_bad.lute:7:1:  error [E-CONN-UNKNOWN-NODE] unknown node: no scene resolves to
    key `anso.s01ep01`  (`visited`, dsl §2.3/§4.1) — did you mean `anseo.s01ep01`?
  ```
  And `lute scenario` reports the resulting graph directly, which is the readback the
  eleven-scene structure will need:
  ```console
  $ lute scenario docs/examples/anseo
    topological layers:
      layer 0: scene(anseo.s01ep01)
      layer 1: scene(anseo.s01ep02)
    edges (prerequisite -> dependent) [atom kind(s)]:
      scene(anseo.s01ep01) -> scene(anseo.s01ep02) [visited]
  ```
- **Verdict** — worked well. `lute scenario` is the tool T2.5 wished for and did not
  have: a rendering that shows the structural fact rather than making you infer it.
- **One caveat later tasks must carry**, not a defect — the subcommand help states it:
  the per-file `lute check` on a scene whose `after:` names a nonexistent scene prints
  `ok: … (0 warning(s))`, exit 0. `E-CONN-UNKNOWN-NODE` is a *project-wide* diagnostic
  and only `check-project` computes it. Checking one file is not enough to know the
  route is intact.

#### T3.12 — `trace` and `run` render branching honestly — WORKED WELL

- **Attempt** — read the committed scene back through both preview tools.
- **Result** — `lute trace` shows the construct, the eligible set, the winning arm, and
  every effect inside it:
  ```
  <branch whoWakes>   eligible: wakeToma, wakeIlsabet, wakeNobody   -> wakeToma (auto)
    ::set  run.shedPressure = 2
    ::assert  awake(toma)
    ::assert  knows(toma, shed_sequence)
    @toma  How long have I been under?
  trace complete: 1 decision; choices 1/3 (whoWakes)
  ```
  `lute run` does the same over the artifact, with addresses and a final-state dump.
- **Verdict** — worked well, and it is the direct contrast to T2.5, where the same two
  tools discarded the one bit that said a character had left. For branching, state, and
  facts they discard nothing: eligibility, selection, both effect kinds, and the
  coverage summary are all there.
- **One readback nuance, noted rather than complained about:** trace renders `+= 2` as
  `::set run.shedPressure = 2`, i.e. the resolved post-value, not the delta. Seeded
  with `--state run.shedPressure=7` the same line prints `= 9`. That is arguably the
  more useful number — it is the state after — but for a scene whose subject is *how
  much each choice costs*, the price the author wrote is the thing that is no longer on
  screen. `= 9 (+= 2)` would carry both.

#### T3.13 — `directives.md` scopes `::assert`/`::retract` to "Quest documents"; they work in scenes — DOC-WRONG

Split out of T3.3, where it was found. T3.3 records that the construct worked; this
records that the page telling you whether you may reach for it is false.

- **Intent** — before writing per-arm facts, ask the docs the prior question: may a
  *scene* assert a fact at all, or is fact mutation reserved to quest documents? The
  natural place to look is the canonical enumeration of built-in `::`-directives.
- **Attempt** — read `packages/website/src/content/docs/language/directives.md`
  §"Reserved directives". Lines 124–127, verbatim:
  > Two `::`-directives are built-in rather than staging vocabulary: `::set` writes
  > declared state (see [State model](/state/state-model/)) and `::use` expands a
  > reusable content component (see
  > [Components & extends](/language/components-and-extends/)). **Quest documents
  > additionally use `::assert` / `::retract` to mutate facts** (see
  > [Facts & Datalog](/state/facts-and-datalog/)).

  The false clause is the third sentence, `directives.md:126–127`.
- **Result — false as written, and this task depends on it being false.**
  `docs/examples/anseo/scenes/cryobank.lute` is `kind: scene` (line 2), and four
  `::assert` directives sit inside `<choice>` bodies (lines 18, 19, 24, 25). The
  checker accepts them without qualification —
  `ok: docs/examples/anseo/scenes/cryobank.lute (0 warning(s))`, and
  `lute check-project docs/examples` exits 0 — they lower to real `assert` records
  (`{"kind":"assert","addr":"001-0700","relation":"awake","args":["toma"]}`, T3.3),
  and the facts they write reach a `holds()` guard in a later document. Read plainly,
  the page says the construct is not for the document kind in which it demonstrably
  works. No restriction of the stated shape exists.
- **The docs contradict each other, and that is worse, not better.**
  `packages/website/src/content/docs/state/facts-and-datalog.md:25` states the
  unscoped truth: "Content writes **deltas** with the leaf directives `::assert` and
  `::retract`". So the right answer *is* on the shipped site — which is precisely why
  this is not `DOC-GAP`: nothing is silent, I read no Rust, no proposal, no test. But
  an author asking "which `::`-directives exist and where may I use them?" lands on
  `directives.md` first, because that is the page named after the question. Being told
  the construct belongs to another document kind is a *terminating* answer: they stop
  looking, and never reach the facts page that would have corrected them. A second
  page holding the truth only helps the author who keeps searching, and a false
  statement is exactly the thing that stops them searching.
- **Resolution** — the asserts were written in the scene anyway, and worked (T3.3).
  Resolution for the *doc*: the clause should read that content documents generally —
  scenes included — use `::assert` / `::retract`, or simply drop "Quest documents" and
  say "Content additionally uses", matching `facts-and-datalog.md`.
- **Verdict** — `DOC-WRONG`. Present and false: it scopes a construct to the wrong
  document kind. Ranked above a `DOC-GAP` per the table — an author who believes it
  never discovers they were lied to, and in this project's case would have hand-rolled
  a state flag for something the language already does, losing the Datalog derivation
  (`can_halt(C) :- awake(C), knows(C, shed_sequence)`) that Task 4's gate depends on.

#### T3 summary

Thirteen entries: six *worked well*, five `TOOL-DEFECT`, one `DOC-WRONG`, one
`AUTHOR-ERROR`, no `LANGUAGE-GAP`, no `DOC-GAP`. Nothing this scene wanted was
inexpressible, and — the part that matters for the authoring rule — nothing was
substituted. The counter, the branch, the
per-arm facts, the conditional availability, and the scaling cost were each written in
the form I first reached for, and each of them worked.

**The design claim holds and is now demonstrated, not asserted.** The compiled scene
contains `{sprite: 2, line: 5, choice: 1, set: 2, assert: 4, jump: 3}` and no
time-driven command of any kind; the reference runtime moves `run.shedPressure` to 2,
to 1, or not at all, strictly according to which `::set` an author placed in which arm;
and the language refuses to let an author declare a clock of their own. There is no
engine clock.

**The split in the findings is sharp and it is not about the language.** Everything
*declared* is checked superbly: relation arity, per-argument entity domains, derived-
relation writes, undeclared and misspelled state paths in every position including
inside `{{…}}`, `E-BRANCH-ALL-GUARDED`, `E-INTO-VALUE`/`E-INTO-TARGET`,
`E-MAYBE-UNSET`, `E-CONN-UNKNOWN-NODE`. T3.4 is the answer to whether `entities:` earns
its keep — a transposed `knows(shed_sequence, toma)` is caught per argument index, and
no string-keyed design could do that.

Set against it, **T3.2 is the finding to act on**: the declared `type:` of a state path
constrains `into=`/`value=` and constrains nothing about `::set`. `::set{run.shedPressure += "two"}`
on a `number` path is `ok` under `--deny-warnings`, compiles, and is silently evaluated
to `0` by the reference runtime; `::set{run.shedPressure = true}` writes a boolean into
it. For a work whose central mechanic is a schedule that advances, that is a counter
that stops counting with every gate green.

The remaining four defects are the same shape T1 and T2 found, in new places: tools
that lose information they hold. `context` omits four directives from a list that
counts itself (T3.7); `E-CEL-PARSE` names the wrong construct and proposes a repair
that does not parse (T3.6); a broken state schema is reported as an integer while the
command you would run next misparses the schema as a scene (T3.9); a mock key typo is
discarded by both tools that read mocks (T3.10). Two of the five report a count where
the content was in hand — *say what you found, not how much of it there was* (T3.7,
T3.9) — and two more discard information silently (T3.2's untyped `::set` right-hand
side, T3.10's mock key). T3.9 is the one to fix first, because it is the only one
where the tool's advice actively damages the file.

Outside that shape sit the two reclassified entries. **T3.13 is the finding a reader
of these logs is most likely to hit themselves**: `directives.md` tells authors that
`::assert` / `::retract` are for quest documents, this scene uses them in a `<choice>`
body, and the checker is perfectly happy. One clause, one page, and it is the page
named after the question. T3.8, by contrast, is now an `AUTHOR-ERROR` kept only for
its silence: the shipped docs specify `{{…}}` plainly and single braces are legitimate
prose, so `check` reading them as text is correct behaviour, not a defect.

### T4 — The relational quest gate

Toolchain 0.9.0 / language 0.9.0 / IR 0.9.0, `./target/debug/lute`. One quest added:
`quests/hold-the-spine.lute`, the project's first `kind: quest` document, gating on the
derived relation `can_halt` that T3.3 proved reachable from a `<choice>` arm two
documents away.

This is the first task to exercise the layer Lute's static-analysis claims are
strongest about, and the reading splits cleanly. **Everything the checker computes
about a relational gate is excellent, and it computes it project-wide.** Everything
that *reports* that computation to an author — one attribute slot, one preview tool,
one reachability verdict, one line of the runtime contract — is wrong, absent, or
contradicts a sibling.

#### T4.1 — every shape a real quest wants was in the language, in the first form reached for — WORKED WELL

- **Intent** — written before the brief's skeleton was typed. The shed is walking down
  the spine toward the infirmary. Vesna wants it stopped, and stopping it needs a hand
  at the coupling belonging to someone awake who knows the sequence. That beat wants
  four things, and only the first is in the brief:
  1. reach the coupling;
  2. **cut** it — an objective that is not offered until the first one is done;
  3. **optionally** pull the manifest from the coupling locker on the way — it does not
     gate the halt, but it matters later;
  4. a way to **fail that is not the inverse of succeeding** — the shed arrives at the
     infirmary first, whatever the crew were doing at the time.
- **Attempt** — all four, written at once, no hedging, against a scratch copy
  (`/tmp/t4/anseo`) with one extra scalar (`run.couplingCut`) so objective 2 had
  something real to read:
  ```lute
  <quest id="holdTheSpine" title="Hold the Spine" start="holds(can_halt(toma))" fail="run.shedPressure >= 5">
  <objective id="reachToma" title="Reach the spine coupling" done="run.shedPressure >= 1"/>
  <objective id="cutCoupling" title="Cut the coupling" done="run.couplingCut" when="quest.holdTheSpine.objectives.reachToma.done"/>
  <objective id="pullManifest" title="Pull the manifest from the locker" done="holds(found(toma))" optional/>
  <on event="questComplete">
  ::set{run.vesnaTrust += 1}
  @narrator: The shed halted, one module short of the infirmary.
  </on>
  <on event="questFailed">
  @narrator: The shed reached the infirmary bulkhead and kept walking.
  </on>
  </quest>
  ```
- **Result** — the *grammar* took all four without a murmur. The only diagnostic in the
  run was semantic, about the story rather than the shape (T4.2). Specifically:
  - **Sequencing is a reserved-path read**, and it composes: an objective may gate its
    own visibility on another objective's completion by reading
    `quest.<id>.objectives.<oid>.done`, a path the compiler declares for you (it is in
    the artifact's `state` table with `"provenance": "quest:holdTheSpine"`). No new
    construct, no author-declared mirror flag.
  - **`optional`** is a bare attribute and excludes the objective from derived
    completion, exactly as `quests-and-scenes.md` says.
  - **`fail=` is a sibling of `start=`, over anything CEL can say** — so an independent
    failure clock is one attribute, and `<on event="questFailed">` reacts to it. The
    failure condition genuinely does not have to mention the success condition.
- **The one semantic caveat, and it is documented and correct.** `when=` on an
  `<objective>` gates "visibility/tracking, not the completion obligation"
  (`quests-and-scenes.md`). So `cutCoupling` is *hidden* until `reachToma` is done but
  still *required* for the quest to complete — which is what "becomes available after"
  should mean. Had I wanted "and skippable if never offered", that is `optional`, and
  the two compose.
- **Resolution** — the committed file is the brief's single-objective form. That is an
  authorial choice made *after* confirming the richer form works, in the same sense as
  T3.6's flat `+= 2`: this quest is the prologue's one-line goal machine, and Task 9's
  five siblings are where the sequencing and the optional arm belong. Nothing was
  substituted to avoid finding out.
- **Verdict** — worked well. Four independent quest-design instincts, four constructs,
  zero workarounds, and the sequencing one did not even need a new idea — it falls out
  of the reserved state the quest already declares.

#### T4.2 — the checker proves a fact gate dead *across documents*, and says which relation — WORKED WELL

This is the strongest single thing T4 measured and it deserves the transcript.

- **Intent** — none authorial; it fell out of T4.1. `pullManifest` gated on
  `holds(found(toma))`, and `found` is declared in `world.schema.yaml` but asserted by
  no document in the project.
- **Result** — a hard, project-wide error naming the relation and the reason:
  ```
  quests/hold-the-spine.lute:11:1: error [E-OBJECTIVE-UNSATISFIABLE] `done` predicate
  `holds(found(toma))` queries relation(s) `found`, which is unreachable under your
  declared routes: no `facts:` seed, no `reserved` tier, and no rule closure over
  already-producible relations can ever populate it, so the objective can never
  complete on any run (dsl 0.4.0 §4.2/§5.3)
  ```
  Change one relation to one the story *does* produce — `holds(knows(toma, manifest))`,
  where `knows` is asserted in `scenes/cryobank.lute`'s choice arms — and the error
  becomes `W-UNPROVEN-RELATIONAL`, exit 0.
- **And the difference really is the other document.** The negative control, run in the
  scratch copy: delete the two `::assert{knows(…)}` lines from `cryobank.lute` and
  re-check, changing nothing in the quest.
  ```
  quests/hold-the-spine.lute:11:1: error [E-OBJECTIVE-UNSATISFIABLE] `done` predicate
  `holds(knows(toma, manifest))` queries relation(s) `knows`, which is unreachable …
  ```
  Warning → error, from an edit in a file the quest does not name and cannot see. The
  producibility analysis is genuinely project-wide, and it is closed over the rule set:
  `can_halt` is never asserted anywhere either, and it is judged producible because
  `can_halt(C) :- awake(C), knows(C, shed_sequence)` closes over two relations that are.
  When the objective is required, the message even carries the consequence up a level —
  "the objective — and, being required, the quest — can never complete".
- **Verdict** — worked well, without qualification. A goal machine in its own file,
  gated on a Datalog head derived from base facts asserted inside a `<choice>` arm of a
  different episode, and the checker still knows whether the gate can ever open. This is
  the payoff the declared relational layer is *for*, and no string-keyed flag design
  could compute it.

#### T4.3 — Step 3: the gate is typed, and `vesna` passing is the interesting half — WORKED WELL

- **Intent** — the assignment's central proof: a typo in a quest gate is a check-time
  error, and that is what a closed entity domain buys.
- **Attempt and result** — three runs against the committed file, exit codes exact:
  ```console
  # A — a name that is not a crew member
  $ lute check-project docs/examples/anseo      # start="holds(can_halt(nobody))"
  quests/hold-the-spine.lute:8:56: error [E-FACT-DOMAIN] `nobody` is not a declared
    member of entity kind `crew` (relation `can_halt` argument 0, dsl 0.3.0 §3.1)
  failed: docs/examples/anseo (3 file(s), …)                                # exit 1

  # B — a crew member who cannot, in this story, ever halt the shed
  $ lute check-project docs/examples/anseo      # start="holds(can_halt(vesna))"
  ok: docs/examples/anseo (3 file(s), 1 project-wide warning(s))            # exit 0

  # C — restored
  $ lute check-project docs/examples/anseo      # start="holds(can_halt(toma))"
  ok: docs/examples/anseo (3 file(s), 1 project-wide warning(s))            # exit 0
  ```
  The error is column-exact on the attribute value (`8:56`), names the entity kind, and
  reports the **argument index** — the T3.4 shape, now confirmed in quest-attribute
  position and not just in `::assert`.
- **B is the entry, not A.** `awake(vesna)` and `knows(vesna, shed_sequence)` are
  asserted **nowhere** in the project — the `wakeToma` arm wakes Toma, the
  `wakeIlsabet` arm wakes Ilsabet, and no arm wakes Vesna. So `can_halt(vesna)` cannot
  hold on any run, and the checker accepts it. That is the correct behaviour and it is
  the whole point: the checker validates the query's **shape** and its arguments'
  **domain membership**, and declines to claim anything about runtime truth.
- **How far "declines to claim" goes, measured rather than assumed.** Diff the two
  green runs with the argument name normalised away:
  ```console
  $ diff <(sed 's/vesna/ARG/' out-vesna.txt) <(sed 's/toma/ARG/' out-toma.txt)
  # (no output — identical)
  ```
  A gate the story can open and a gate the story can never open produce **byte-identical
  diagnostics**. The analysis is relation-level, not ground-fact-level: it proved
  `can_halt` producible (T4.2) and stops there. That is a real and stated boundary, not
  a bug — but it is the precise size of the guarantee, and it is worth knowing that
  `E-FACT-DOMAIN` catches `nobody` and nothing catches `vesna`.
- **Verdict** — worked well. Both halves of the brief's claim hold, and the second half
  is sharper than the brief puts it: what the closed domain buys is not "the gate is
  right", it is "the gate is *askable*". Every misspelling, every wrong entity kind,
  every wrong arity is a build break (T4.6); every well-formed question is accepted and
  honestly labelled unproven.

#### T4.4 — `W-UNPROVEN-RELATIONAL` names two verification routes, and the tool one cannot do the job — TOOL-DEFECT

The assignment asks whether this warning is actionable or a shrug the author learns to
ignore. It is neither: it is a referral, and it names two routes — `lute trace` seeds
**or human review**. The tool route is the one this entry measures, and it does not work.

- **The warning, in full:**
  ```
  warning [W-UNPROVEN-RELATIONAL] `start="holds(can_halt(toma))"` is gated by a
  relational fact query over producible relation(s) `can_halt`; static reachability
  analysis (dsl 0.6.1 §2) neither proves nor refutes it. Verify with `lute trace`
  seeds or human review
  ```
  As prose this is close to a model diagnostic: it quotes the offending attribute, names
  the relation, cites the clause, states the limit precisely ("neither proves nor
  refutes"), and — unusually for a `W-` code — **names remedies**, two of them. It is not
  a shrug. It is a referral.
- **Following the referral.** `lute trace` on the quest is genuinely good at the first
  step — it stops at the gate and hands you the exact flag:
  ```console
  $ lute trace quests/hold-the-spine.lute --project docs/examples/anseo
  trace incomplete: 1 unresolved atom (exit 3)
    unresolved: quest `holds(can_halt(toma))` (holdTheSpine quest) — supply --fact "can_halt(toma)" as a mock
  ```
  Then it comes apart, twice.
- **(a) The rules do not fire on seeds, so the chain under test cannot be exercised.**
  Seeding the two base facts the story actually asserts changes nothing:
  ```console
  $ lute trace … --fact "awake(toma)" --fact "knows(toma, shed_sequence)"
  trace incomplete: 1 unresolved atom (exit 3)
    unresolved: quest `holds(can_halt(toma))` … — supply --fact "can_halt(toma)" as a mock
  ```
  This is documented, in a parenthetical — `tracing.md`: a `--fact` is "a *supplied
  answer*, so it may name a `derive:`/`reserved:` relation" — so it is design, not
  defect. But the consequence is that the only **tool-assisted** route the warning offers
  requires you to **assert the conclusion**, and the rule
  `can_halt(C) :- awake(C), knows(C, shed_sequence)` — the thing the whole quest rests
  on, and the only part of the chain a human could plausibly get wrong — is never
  evaluated by any command an author can run.
- **(b) And when you do supply the conclusion, trace tells you it proves nothing.**
  ```console
  $ lute trace quests/hold-the-spine.lute --project docs/examples/anseo --fact "can_halt(toma)"
  trace: … (seeds: 0 paths, 1 facts; 0 selections)
  note: W-TRACE-MOCK-UNPRODUCIBLE — mock fact over relation `can_halt` is not producible
  (no `facts:` seed, no reachable `::assert`, not `reserved`) — the supplied answer can
  never arise from authored producers, so a complete walk seeded with it proves nothing
  about reachable play (§4)
    <quest holdTheSpine>   -> active (holds(can_halt(toma)))
    <objective reachToma>   -> pending (run.shedPressure >= 1)
  trace complete: 2 decisions                                              # exit 0
  ```
  The referral's tool-assisted half closes the loop back onto itself: `check-project` says
  "verify with `lute trace` seeds", and `lute trace` says the seed proves nothing.
  *(Correction of record. The first pass logged `trace complete: 4 decisions` against this
  command. The committed one-objective quest emits **2** — one quest decision, one
  objective — and the transcript above is the re-run, verbatim. The 4 is T4.1's richer
  three-objective scratch form, re-confirmed on a rebuilt scratch copy: quest +
  `reachToma` + `cutCoupling` + `pullManifest`. Everything else in this entry reproduced.)*
- **(c) The two tools contradict each other about the same word, and trace is the one
  that is wrong.** `W-TRACE-MOCK-UNPRODUCIBLE` asserts `can_halt` "is not producible
  (no reachable `::assert`)". `check-project`, in the same project, with the same
  `--project` root, calls `can_halt` a "producible relation" *in the very warning that
  sent me here* — and T4.2 proves that judgement is real, cross-document, and rule-closed.
  The disagreement is scope, and it is isolable in two commands:
  ```console
  # the document that CONTAINS ::assert{awake(toma)} — no warning
  $ lute trace scenes/cryobank.lute --project docs/examples/anseo --fact "awake(toma)"
  trace: … (seeds: 0 paths, 1 facts; 0 selections)
  ## Shot 1. …

  # a different document in the SAME project, same fact — warning
  $ lute trace quests/hold-the-spine.lute --project docs/examples/anseo --fact "awake(toma)"
  note: W-TRACE-MOCK-UNPRODUCIBLE — mock fact over relation `awake` is not producible …
  ```
  `trace`'s `producible()` is **document-local**; `check-project`'s is **project-wide**.
  Both print the same three-clause justification, so nothing in either output hints that
  they are answering different questions.
- **The other named route is human review, and nothing here shows it is impossible.** The
  warning says "Verify with `lute trace` seeds **or human review**", and only the first of
  the two is a tool. Human review is in fact what discharged this gate: T3.3 compiled the
  artifact, read the `assert` records out of it, and checked the rule by hand. So the
  claim this entry proves is the narrow one — **the tool-assisted route is unusable** —
  and the offered fallback is unassisted manual work over a compiled artifact, on the one
  link in the chain (`can_halt(C) :- awake(C), knows(C, shed_sequence)`) that the
  toolchain has already reasoned about, correctly and at relation level, and renders
  nowhere (T4.7).
- **Resolution** — `NONE — nothing to resolve; the committed gate is correct by T3.3's
  end-to-end verification, which was done by reading the compiled artifact, not by any
  command that claims to verify gates.`
- **Verdict** — `TOOL-DEFECT`, taking the table in order.
  - Not `LANGUAGE-GAP`: the gate is expressible and expressed.
  - Not `ERGONOMIC`: the working form is not more awkward; the *verification* of it is
    circular.
  - Not `DOC-GAP`: `tracing.md` documents the supplied-answer semantics and documents
    `W-TRACE-MOCK-UNPRODUCIBLE`. I read no Rust to establish any of this.
  - Not `DOC-WRONG` — although it is close, and worth saying why not. `tracing.md:58`
    glosses the warning as firing when "no authored producer can ever assert" the
    relation, which is the *project-wide* meaning and is false of `awake` here. But the
    page is describing what the code is plainly meant to do; the code is what deviates.
  - Not `AUTHOR-ERROR`: I followed the diagnostic's own instruction.

  That leaves `TOOL-DEFECT` on the criterion's own words — a tool "lying about its own
  contract". The false claim is (c): `trace` reports a **document-local** `producible()`
  verdict in project-wide language, contradicting `check-project` on the same word, in the
  same project, under the same `--project` root. (b) is not a second, independent lie —
  it is that same one closing the loop, the referral's tool half answered by a false claim
  about your project. **On the assignment's question:** the warning is *not* a noise floor
  that trains people to ignore warnings — it fires on exactly the correct usages, but it
  fires with a specific, honest, quotable statement of an analysis boundary, which is the
  right thing for a checker to do when it cannot decide. Five such warnings already sit on
  other examples and `check-project docs/examples` still exits 0 with all six. What erodes
  it is narrower than "undischargeable": of the two routes the warning names, the one an
  author reaches for first — the tool it names by command — cannot be completed, so every
  firing falls back on unassisted review of a compiled artifact.

#### T4.5 — a `start=` gate on an unproducible relation is silent, and `scenario reach` calls the quest Reachable — TOOL-DEFECT

T4's most serious finding, and a direct consequence of T4.2 having been done so well one
attribute over.

- **Intent** — none authorial. T4.2 established that `done="holds(found(toma))"` is a
  build-breaking error because nothing in the project can ever assert `found`. The
  obvious next question is whether the same predicate is caught in the slot that decides
  whether the quest ever *starts*.
- **Attempt** — one attribute changed on the otherwise-committed quest, scratch copy:
  ```lute
  <quest id="holdTheSpine" title="Hold the Spine" start="holds(found(toma))">
  ```
- **Result — total silence, and it is quieter than the correct version:**
  ```console
  $ lute check-project /tmp/t4/anseo
  ok: /tmp/t4/anseo/quests/hold-the-spine.lute (0 warning(s))
  ok: /tmp/t4/anseo (3 file(s), 0 project-wide warning(s))                  # exit 0
  ```
  Zero diagnostics of any severity. Note the count: the **correct** gate
  (`holds(can_halt(toma))`) yields one project-wide warning; the gate that can never open
  yields none. The louder signal is the working code.
- **The machinery exists, is wired to this exact slot, and has the diagnostic class
  already.** Three facts from the same command:
  1. `start=` *does* run producibility — that is what emits `W-UNPROVEN-RELATIONAL` when
     the relation is producible. There is simply no "not producible" branch.
  2. `E-QUEST-UNREACHABLE` exists and fires on this attribute:
     ```
     8:1: error [E-QUEST-UNREACHABLE] quest can never complete: `start` decides false —
          the quest never activates (dsl 0.4 §5.3)
     ```
     on both `start="false"` and `start="1 > 2"`.
  3. `E-OBJECTIVE-UNSATISFIABLE` and `E-QUEST-UNREACHABLE` cite **the same spec clause**,
     `dsl 0.4 §5.3`, and the objective one already escalates to the quest ("the
     objective — and, being required, the quest — can never complete").
  So the analysis, the slot, the diagnostic class, and the spec clause are all present.
  One wire is missing.
- **And it is not merely a missing diagnostic — a tool positively asserts the wrong
  answer.** `lute scenario … reach` consults the quest lifecycle, provably:
  ```console
  $ lute scenario /tmp/t4/anseo reach quest:holdTheSpine       # start="false"
    verdict: Unreachable — quest lifecycle proves this quest can never complete
             (E-QUEST-UNREACHABLE), under your declared routes.

  $ lute scenario /tmp/t4/anseo reach quest:holdTheSpine       # start="holds(found(toma))"
    verdict: Reachable — a plain quest with no declared `after` prerequisite,
             reachable by default quest lifecycle under your declared routes.
  ```
  Same tool, same question, same *kind* of dead quest — and for the relational one it
  prints **Reachable**. This is not `scenario` being honestly graph-only: it reaches into
  the lifecycle verdict for the scalar case and gets a right answer, then reports a wrong
  one for the relational case because the verdict it is reading was never computed.
- **Resolution** — `NONE — nothing to resolve; the probe is the finding. The committed
  quest gates on a producible relation, which is correct by T4.2's analysis, not by
  anything the `start=` slot checked.`
- **Verdict** — `TOOL-DEFECT`, and it is the "false green" the table names, in its
  compound form. Not `LANGUAGE-GAP` (nothing inexpressible), not `ERGONOMIC` (the form is
  fine, the verification is absent), not `DOC-GAP` (no page's absence causes it and none
  could fix it — `scene-graph.md` and `quests-and-scenes.md` both describe the intended
  behaviour correctly), not `AUTHOR-ERROR` (I broke no documented rule; the tool called a
  dead quest live). A quest whose `start` predicate can never become true is a quest that
  is never playable, and the toolchain will tell you so if you write `false` and will
  tell you the opposite if you write a fact query — while proving, in the same run, that
  it knows the fact query is dead.

#### T4.6 — relation names are the one identifier class with no did-you-mean — ERGONOMIC

- **Intent** — the assignment's typo probes: the checks that decide whether a declared
  relational layer pays for itself.
- **Result — everything is caught, at the right severity, with the right body:**
  ```
  start="holds(can_hlat(toma))"
    8:56: error [E-RELATION-UNKNOWN] unknown relation `can_hlat` (dsl 0.3.0 §4)

  start="holds(can_halt(toma, extra))"
    8:56: error [E-RELATION-ARITY] relation `can_halt` expected 1 argument(s), got 2 (dsl 0.3.0 §4/§5)

  start="holds(can_halt())"
    8:56: error [E-RELATION-ARITY] relation `can_halt` expected 1 argument(s), got 0 (dsl 0.3.0 §4/§5)

  start="holds(can_halt(shed_sequence))"          # right arity, wrong entity kind
    8:56: error [E-FACT-DOMAIN] `shed_sequence` is not a declared member of entity kind
          `crew` (relation `can_halt` argument 0, dsl 0.3.0 §3.1)

  start="can_halt(toma)"                          # forgot the holds()
    8:56: error [E-CEL-PROFILE] `can_halt(…)` is outside the Lute-CEL profile — only
          operators, literals, lists, `?:`, `in`, `has()`, `isSet()`, `holds()`,
          `count()`, `validAt()`, and `now()` are permitted (dsl §8.4, 0.3.0 §8)
  ```
  All exit 1. Five failure modes, five codes, and the `E-CEL-PROFILE` one enumerates the
  entire permitted set, which is how I confirmed `count()` and `validAt()` are available
  here without opening a page. Unlike T3.4's `::assert` probes these are
  **column-exact** — `8:56` lands on the attribute value, not the start of the element.
- **The gap, and it is visible in a single run of a single file.** A misspelled state
  path in `done=` gets a suggestion; a misspelled relation in `start=` does not:
  ```
  9:66: error [E-UNDECLARED] state path `run.shedPresure` is not declared in `state:`
        (dsl §9.4) — did you mean `run.shedPressure`?
  8:56: error [E-RELATION-UNKNOWN] unknown relation `can_hlat` (dsl 0.3.0 §4)
  ```
  Same document, same check, same closed declared set to compare against — `relations:`
  has four members in `world.schema.yaml`. T3.4 recorded `E-RELATION-UNKNOWN` on `awak`
  and noted only its span; the missing suggestion is the more useful half, and it
  generalises: state paths, `after:` scene keys (`E-CONN-UNKNOWN-NODE`, T3.11) and
  `::set` targets all suggest; relation names alone do not.
- **Secondary, small: the warning fires over a query that does not typecheck.** On
  `start="holds(can_halt(toma, extra))"` the run emits both the `E-RELATION-ARITY` error
  *and* `W-UNPROVEN-RELATIONAL` at the same span, i.e. it reports that a malformed query
  is neither proved nor refuted. (`can_hlat` correctly emits no warning — the relation
  never resolves.) Cosmetic, but it is one more instance of the project-wide pass not
  knowing what the document pass already decided.
- **Verdict** — `ERGONOMIC`. Nothing is unexpressible and nothing is misreported; the
  cost is one extra round trip on the identifier class where a closed declared set makes
  the suggestion cheapest to compute. Recorded because the assignment asks directly
  whether these checks make the declared relational layer worth its cost, and the answer
  is an emphatic yes with one uneven edge.

#### T4.7 — nothing an author can run answers "is this quest reachable?" — ERGONOMIC

- **Intent** — the assignment's structural question. The quest lives in its own file with
  its own `uses:`, gating on a fact a scene two documents away asserts inside a
  `<choice>` arm. Nothing links them syntactically. So: how would an author know this
  quest is reachable at all?
- **Attempt** — every read-only surface the CLI offers, against the committed project.
- **Result — three tools, three partial answers, and the union still misses:**
  - **`lute scenario <dir>`** — the quest is not in the graph. Not a node, not a layer,
    not an edge; `--format json` has no `quest(holdTheSpine)` entry at all. This is
    *documented and deliberate* — `scene-graph.md`: "A quest becomes a graph node by
    declaring `after` (even `after=""`); a quest that never opts into a graph position is
    still addressable by `lute scenario <dir> envelope quest:<id>`, but contributes no
    edges." Confirmed by adding `after="visited('anseo.s01ep02')"`, which puts
    `quest(holdTheSpine)` in layer 2 with an edge from the scene. So the answer is
    available — but only to an author who already knew the answer and hand-declared it.
  - **`lute scenario <dir> reach quest:holdTheSpine`** — `verdict: Reachable`, with no
    mention of the `start` gate. Correct in the tool's own narrow sense (`--help`:
    "Evaluates no CEL, runs no Datalog"), and for a *scalar-dead* quest it does better
    than that (T4.5). For a fact-gated quest the word "Reachable" is the answer to a
    different question than the author asked.
  - **`lute scenario <dir> envelope quest:holdTheSpine`** — Guaranteed/Possible tables
    over `run.shedPressure` and `run.vesnaTrust`. **No fact section of any kind**, so the
    one gate that decides whether this quest ever activates is absent from the surface
    designed to say what holds when control reaches it. It does close with a genuinely
    useful nudge — "this quest declares no `after` attribute, so this is the defaults-only
    `D` table … declaring `after` … would enrich this table" — which is the clearest
    pointer in the toolchain toward the `after=` opt-in above.
  - **`lute trace`** — the best of the four at *locating* the question and, per T4.4,
    unable to answer it.
  - **`lute doctor`** — file counts and vocabulary slots; nothing relational.
- **The information exists.** `check-project` computed, in the same run, that `can_halt`
  is producible *because* `cryobank.lute` asserts `awake` and `knows` and the rule closes
  over them (T4.2's negative control proves the dependency is real and cross-document).
  That is precisely a producer → consumer edge, it is exactly what the author's question
  asks for, and no output renders it. Adding `after=` does not render it either — it
  records a second, hand-written claim that happens to run parallel to it.
- **Resolution** — I know the gate is live because T3.3 compiled the artifact and read
  the `assert` records out of it, then checked the rule by hand. That is the right way to
  confirm it and the wrong way to learn it.
- **Verdict** — `ERGONOMIC`. Not `TOOL-DEFECT`: every tool here is accurate within its
  documented scope, `scenario --help` and `scene-graph.md` both state their limits
  plainly, and the `envelope` note actively points at the fix. Not `DOC-GAP`: the pages
  say what the tools do. The cost is that quest/scene separation is real — separate file,
  separate `uses:`, no syntactic link — and the toolchain has the join in hand and
  renders it nowhere, so the author's obvious question has a four-command answer that
  still requires reading an artifact. In an eleven-scene work with six quests (Task 9),
  that is the surface that decides whether the quest layer is trustworthy.
- **Recommendation carried forward to Task 9**, recorded here rather than acted on
  because the brief specifies the committed file: giving each quest an `after=` costs one
  attribute and buys a graph node, a real edge, the full envelope table instead of the
  defaults-only one, and a `scenario` rendering an author can read. `hold-the-spine.lute`
  as committed has no `after=`, matching the brief. **Adopted** — see *T4 controller
  decision* below for the ruling and for why T4 stays the no-`after=` control.

#### T4.8 — the quest's relational gate reaches the artifact as an unparsed string — DOC-WRONG

- **Intent** — read the compiled quest back and confirm the gate survives to the runtime
  in a form a consumer can act on. The static layer is superb (T4.2, T4.3); the artifact
  is the other half of "does declaring `entities:` earn its keep".
- **Attempt** — `lute compile docs/examples/anseo/quests/hold-the-spine.lute --project docs/examples/anseo`.
- **Result** — the quest lowers to one `quest` command with its objectives inline, and
  the two predicate slots are **not** treated alike:
  ```json
  {"kind":"quest","addr":"001-0100","id":"holdTheSpine","title":"Hold the Spine",
   "titleLineId":"holdTheSpine.title",
   "start":{"raw":"holds(can_halt(toma))"},
   "objectives":[{"id":"reachToma","title":"Reach the spine coupling",
     "titleLineId":"holdTheSpine.reachToma",
     "done":{"raw":"run.shedPressure >= 1",
             "expr":{"op":">=","l":{"path":"run.shedPressure"},"r":{"lit":1.0}}},
     "optional":false,"body":null}]}
  ```
  The scalar `done` carries a parsed `expr` tree beside its `raw`; the relational `start`
  carries `raw` only. A consumer written against the `expr` AST gets `undefined` on every
  fact gate and must parse `holds(can_halt(toma))` itself. (Consistent with T3.5, where a
  `<choice when="!holds(awake(toma))">` also reached the artifact `expr`-less while the
  scalar guard beside it did not.)
- **What the docs say, and they disagree with each other.**
  - `tooling/runtime-contract.md:22`, the Lute-vs-engine responsibility table:
    **"Lower every CEL guard to a portable `expr` AST."** `holds(can_halt(toma))` is a
    CEL guard — it sits in a CEL slot and `E-CEL-PROFILE` lists `holds()` among the
    permitted forms — and it is not lowered.
  - `schemas/lute-ir-0.9.schema.json`, `$defs.exprNode`: `expr` is "**Absent** whenever
    the CEL slot was empty or fell outside the closed Lute-CEL profile (dsl §8.4)". So
    the machine-checkable schema correctly permits the absence — but its stated *reason*
    does not apply either, because `holds()` is squarely **inside** the §8.4 profile, by
    the profile error's own enumeration.
  So the artifact's behaviour is licensed by the schema and contradicted by the prose,
  and neither states the actual rule: *relational fact queries lower to `raw` only.*
- **Resolution** — none needed authorially; the committed quest is unaffected and the
  reference runtime handles it (`lute trace` resolves the gate from seeds). The finding
  is for whoever writes the second engine.
- **Verdict** — `DOC-WRONG`, ranked per the table above `DOC-GAP`. The runtime contract
  is the page an engine implementer reads to know what they must handle, its statement is
  present and universally quantified ("every CEL guard"), and it is false for the exact
  construct this task exists to demonstrate. An implementer who believes it writes
  `evalExpr(cmd.start.expr)` — the page's own §"the engine loop" pseudocode does exactly
  this for `set`/`choice`/`match` — and never discovers they were lied to until a fact
  gate silently evaluates undefined. One clause on line 22 ("every CEL guard *except*
  relational fact queries, which carry `raw` only") closes it.

#### T4.9 — a quest document's frontmatter and its identity chain both behave — WORKED WELL

Two observations, one disposition, one verdict.

- **Intent** — get a `kind: quest` document's header and its localisable strings right in
  a project whose only prior documents are scenes, and find out what a quest's identity is
  built from when the scene keys that build `{prefix}` are unavailable.
- **Attempt** — (i) the scene header pasted verbatim into the quest, `character`/`season`/
  `episode` and all; (ii) the committed quest compiled, and its identity fields read back
  out of the artifact.
- **Result** — both behaved:
  - **Scene-only frontmatter keys are rejected per key, exactly as the brief predicted:**
    ```
    1:1: error [E-META-UNKNOWN-KEY] unknown top-level meta key `character` (not a core key
         and not owned by an active plugin)
    ```
    Three errors for three keys, not one roll-up (exit 1), so pasting a scene header into
    a quest is a single-pass fix. The message's "not owned by an active plugin" clause is
    the useful half — it says *why* the key is unknown rather than just that it is.
  - **Quest identity is derived from the quest id, and titles are addressable.** With no
    `character`/`season`/`episode` to build `{prefix}` from, the `<on>` arm's narration
    lands on `"lineId": "holdTheSpine.narrator_0010"`, and both the quest title and each
    objective title carry a `titleLineId` (`holdTheSpine.title`, `holdTheSpine.reachToma`)
    — so quest-log strings are localisable on the same footing as dialogue.
- **Resolution** — the committed frontmatter carries `kind`/`luteVersion`/`uses`/`title`
  only. Nothing was worked around.
- **Verdict** — worked well. One caveat, deliberately **not** scored here: neither the
  quest `{prefix}` derivation nor `titleLineId` is mentioned by `lute context` or on the
  identity docs. That is T1.7's existing `DOC-GAP` extended to quests — cross-referenced,
  not counted a second time.

#### T4.10 — `scenario envelope` describes the author's project in compiler-internal vocabulary — TOOL-DEFECT

- **Intent** — read the envelope as an author would, to learn what state is safe to read
  when the quest activates. Part of the T4.7 sweep, split out because it is a defect in
  the output rather than a limit of the surface.
- **Attempt** — `lute scenario docs/examples/anseo envelope quest:holdTheSpine`.
- **Result** — the `Possible \ Guaranteed` table is annotated:
  ```
  Possible \ Guaranteed -- inventory only (paths set on SOME but not every declared route
  reaching this quest, dsl §4.4). This is NOT the T11 warning-grade read-site class --
  quest read diagnostics are `check_quest_guard_defassign`'s separate territory (that
  class is scene-only, see the scene envelope's own section)
  ```
  `check_quest_guard_defassign` is a Rust function name and "T11" is an internal task
  label; neither appears anywhere on the website. The sentence is addressed to a reader
  with the compiler's source and its task tracker open, and it is printed to an author.
- **Resolution** — `NONE — nothing to resolve; the table itself is correct and the
  committed project is unaffected.`
- **Verdict** — `TOOL-DEFECT`, and the smallest of T4's three by a wide margin. Not
  `LANGUAGE-GAP` or `ERGONOMIC` — nothing about the authored form is at issue. Not
  `DOC-GAP` or `DOC-WRONG`: no page is silent or false, and no page *could* fix this, since
  the two terms are absent from the docs precisely because they are not public API. Not
  `AUTHOR-ERROR`. That leaves the criterion's own words — a tool wrong about its own
  contract, where the contract of author-facing output is that an author can resolve it.
  Same habit as T1.4's fabricated `::narrator`; cosmetic in cost, recorded because the
  habit is now four tasks old.

#### T4 controller decision — Task 9's quests carry `after=`; this one deliberately does not

Recorded in the durable log rather than left in scratch, because a future reader comparing
`hold-the-spine.lute` with Task 9's five quests will otherwise read the difference as an
oversight. The implementer asked whether quests should carry `after=`. The decision, taken
on the reviewer's recommendation and independently verified by them:

> **The five Task 9 quests carry explicit `after=` prerequisites. `hold-the-spine.lute` is
> not retrofitted.**

The reasoning is T4.7's measurement, and both halves reproduce on the committed project:

- Without `after=`, a quest is absent from the `lute scenario` graph *entirely* — no node,
  no layer, no edge — and `envelope quest:holdTheSpine` returns the **defaults-only `D`
  table**, closing with its own note that declaring `after` "would enrich this table".
- With `after="visited('anseo.s01ep02')"`, `quest(holdTheSpine)` appears at **layer 2**
  with a real `scene(anseo.s01ep02) -> quest(holdTheSpine) [visited]` edge, and the
  defaults-only note disappears — the envelope is now the project-resolved one.

So `after=` costs one attribute and buys the reachability surface that Task 9's
eleven-scene, six-quest shape needs, on all five of its new quests.

Keeping T4 as the **no-`after=` control is deliberate**, not an inconsistency. It leaves
the blind spot visible in a shipped example: a quest that is genuinely reachable,
genuinely checked project-wide (T4.2), and invisible to the one tool an author would ask
about reachability (T4.7) — while `scenario reach` on the committed tree still answers
*"Reachable — a plain quest with no declared `after` prerequisite"*. That visibility is
worth more than uniformity across the two tasks, and it keeps T4.5's `reach` probe one
attribute away from the committed tree rather than requiring the `after=` line be stripped
again first.

#### T4 summary

Ten entries: four *worked well* (T4.1, T4.2, T4.3, T4.9), three `TOOL-DEFECT` (T4.4, T4.5,
T4.10), two `ERGONOMIC` (T4.6, T4.7), one `DOC-WRONG` (T4.8) — every entry carrying
exactly one verdict. No `LANGUAGE-GAP`, no `DOC-GAP`, no `AUTHOR-ERROR` scored here; the
one `DOC-GAP`-shaped observation T4 turned up (quest identity is undocumented) extends
T1.7 and is counted there. Nothing this quest wanted was inexpressible and nothing was
substituted — the sequenced objective, the optional objective, the independent failure
condition and the derived-relation gate were each written in the form first reached for,
and each worked (T4.1).

**The declared relational layer pays for itself, and the receipt is T4.2.** A quest in
its own file, with its own `uses:`, gated on a Datalog head whose base facts are asserted
inside a `<choice>` arm of a different episode — and `check-project` still decides
whether that gate can ever open, closes the rule set to do it, names the offending
relation, and flips warning→error when you delete the producer from the other document.
Set beside T4.3's `E-FACT-DOMAIN` on `nobody` and T4.6's five distinct codes for five
distinct malformations, this is the strongest analysis surface measured in four tasks.
No string-keyed flag design computes any of it.

**And it is reported to the author through four surfaces, three of which are wrong.**
The pattern is identical to T1–T3's and it is now unmistakable: *this toolchain computes
more than it will tell you, and where it tells you, it sometimes tells you the opposite.*
T4.5 is the one to fix first and it is the worst thing in this log after T3.2 — the same
producibility judgement that makes `done="holds(found(toma))"` a build-breaking error
makes `start="holds(found(toma))"` emit nothing at all, and makes `scenario reach` print
**Reachable** for a quest that can never activate, while `start="false"` correctly prints
`Unreachable` citing `E-QUEST-UNREACHABLE`. The analysis, the slot, the diagnostic class,
and the spec clause (`dsl 0.4 §5.3`) are all already there; one branch is missing, and
its absence makes a dead quest quieter than a live one.

T4.4 is second and it is the more demoralising, because it is what an author hits *doing
everything right*. `W-UNPROVEN-RELATIONAL` is a well-written warning that states a real
boundary and names two remedies — `lute trace` seeds **or human review** — and the tool
one cannot be performed: `lute trace` will not run the rule the gate depends on
(documented), and when you seed the conclusion instead it declares that seed unproducible
on a document-local judgement that contradicts the project-wide one in the warning that
sent you there. The human-review fallback does stand, and it is what actually discharged
this gate (T3.3) — but discharging it means reading a compiled artifact by hand, every
time the warning fires, for a producer→consumer join the checker already computed and
renders nowhere (T4.7). On the assignment's question, then: the warning is not a shrug and
it does not train people to ignore warnings by being noisy. The route an author reaches
for first is simply closed. Six of them now sit on `check-project docs/examples`, and
every one marks a correct, deliberate gate.

The remaining four are cheaper: no did-you-mean on relation names, alone among the
identifier classes that have a closed declared set (T4.6); a quest that is invisible to
`lute scenario` until an author hand-writes an `after=` the checker has already inferred
the substance of (T4.7, and see the controller decision above); a runtime-contract table
promising an `expr` AST for "every CEL guard" while relational gates ship as `raw` strings
(T4.8); and `scenario envelope` annotating an author-facing table with a Rust function
name and an internal task label (T4.10).

One thing later tasks must carry forward: **`lute check <file>` is not enough for a
quest.** `E-OBJECTIVE-UNSATISFIABLE`, `W-UNPROVEN-RELATIONAL` and `W-QUEST-REF-UNKNOWN`
are all project-wide, so a quest with a typo'd cross-quest reference —
`quest.holdTheSpine.objectives.reachTomaTYPO.done` — is `ok: … (0 warning(s))`, exit 0,
under per-file `check --deny-warnings`, and only `check-project` reports it (it does, and
well: distinct message bodies for an unknown quest id and for a known quest that does not
declare that objective). This generalises T3.11's caveat from `after:` to the whole quest
layer.

### T5 — The terminators

Toolchain 0.9.0 / language 0.9.0 / IR 0.9.0, `./target/debug/lute`. Two scenes added:
`scenes/bridge.lute` (`anseo.s01ep10`, the success terminal) and `scenes/shed.lute`
(`anseo.s01ep11`, the failure terminal) — the corpus's **first two `::end`s**. Nothing
under `docs/examples/` had ever terminated a walk before, so every entry below is a
first-use measurement. Both `after:` routes are provisional (Task 8 repoints them).

#### T5.1 — `::end` works, first try, on every surface that carries it — WORKED WELL

- **Intent** — two endings. Vesna reaches the bridge and the ship still steers; or the
  Purser gets its module and the allocation is satisfied. Each beat stops there.
- **Attempt** — the brief's two files verbatim, `::end{reason="bridge-reached"}` and
  `::end{reason="shed-with-module"}` as the last node of each document.
- **Result** — `ok: docs/examples/anseo (5 file(s), …)`, exit 0. Every surface that is
  supposed to carry the terminator does:
  - **the artifact** — `{"kind":"end","addr":"001-0400","reason":"bridge-reached"}`, an
    ordinary addressed record at the normal +100 gap, no `wait`/`duration` stamp;
  - **`lute run`** — `001-0400  end    reason=bridge-reached`, then `run complete`;
  - **`W-CODE-AFTER-END`** — fires exactly as `directives.md` describes, once, anchored
    at the first dead node rather than at the `::end`, with the spec's message verbatim;
  - **`--deny W-CODE-AFTER-END`** — promotes it to `error [W-CODE-AFTER-END] [denied]`,
    exit 1.
  - **`E-UNKNOWN-ATTR`** — `::end{ending="…"}` is `` `::end` has no attribute `ending` ``.
    The attribute *key* is closed even though its value is not (T5.3).
- **Resolution** — n/a; nothing was substituted.
- **Verdict** — worked well. This is the least-exercised construct in the language and
  it behaved like the best-exercised one. Worth saying plainly before the four entries
  that follow, all of which are about what `::end` *does not* mean rather than about
  anything it got wrong.

#### T5.2 — the same JSON document carries two unrelated `reason` fields, and the obvious verification matches the wrong one — ERGONOMIC

- **Intent** — confirm the authored reason survives to the artifact.
- **Attempt** — the first thing anyone types:
  ```console
  $ lute compile docs/examples/anseo/scenes/bridge.lute -o /tmp/t5.json
  $ grep -n '"reason"' /tmp/t5.json
  217:        "reason": "pre-loading `vesna`'s first emotion `level` seen ahead of the entrance"
  233:      "reason": "bridge-reached"
  ```
- **Result** — two `reason` keys, and the **first** one is not mine. `bridge.lute`'s
  `::auto` triggers `entry-emotion-lookahead`, whose injected preload sprite carries
  `provenance: { injected, by, reason }` — and that `reason` is a *human-readable English
  justification for a compiler decision*. Mine is an *opaque author token for a host to
  dispatch on*. Same key, same document, one nested under `provenance` and one not,
  contracts with nothing in common. `grep -m1`, `jq '..|.reason?|select(.)' | head -1`,
  and any harness that greps the file all read the injector's prose and report success.
- **Resolution** — match the record, not the key:
  `jq -c '.commands[] | select(.kind=="end")'` → `{"kind":"end","addr":"001-0400","reason":"bridge-reached"}`.
- **Verdict** — `ERGONOMIC`. Not `TOOL-DEFECT`: both shapes are documented, both are
  where they should be, and a *correct* consumer walking `commands[]` by `kind` never
  sees the collision. The cost is entirely on ad-hoc verification, which is what an
  author and an AI harness actually do — and the evidence that the cost is real is that
  this task's own brief had to carry a warning sentence about it. `provenance.reason` is
  the field with the weaker claim on the name (it is a `why`, not a `what`); calling it
  `note` or `justification` would end the collision for free.

#### T5.3 — `::end`'s `reason` is unconstrained, and the one thing an author reaches for to constrain it is accepted, advertised as live vocabulary, and inert — TOOL-DEFECT

This is T5's most serious finding.

- **Intent** — a work with a closed set of endings wants its ending ids closed too. Every
  other value in Anseo's content is checked against a declared domain (T1.2, T1.3), so the
  natural instinct is to declare one for the ending ids and get the same protection.
- **Attempt, in order.** All probes on a scratch copy of the project.
  1. **Is `reason` required?** `::end` bare → `ok`, 0 warnings. Documented (`reason` is
     optional), so not a finding — but it means a terminator can carry no identity at all
     and the artifact simply omits the field.
  2. **Duplicate reason across both documents.** Set `bridge-reached` on the shed
     terminator too, so two different endings of one project answer to one id →
     `check-project` clean, 0 project-wide diagnostics. No cross-document notion of an
     ending id exists to collide.
  3. **Misspelled reason.** `::end{reason="bridge-reachd"}` → `ok`, exit 0. Nothing to
     spell it against.
  4. **Empty reason.** `::end{reason=""}` → `ok`, and it reaches the artifact as
     `{"kind":"end","addr":"001-0200","reason":""}`. A host that distinguishes "no reason
     given" (field absent) from "reason given" now has a silent third state: field
     present, empty.
  5. **Declare a domain for it** — the actual attempt, and the one that matters:
     ```yaml
     # world.schema.yaml
     enums:
       reason: [bridge-reached, shed-with-module]
     ```
- **Result** — the declaration is accepted with no diagnostic (no `E-DOMAIN-DUP`, no
  `E-DOMAIN-UNKNOWN`), it **constrains nothing** — `::end{reason="not-a-declared-member"}`
  still checks `ok`, exit 0 — and it is then *advertised as live vocabulary by two
  surfaces*:
  ```console
  $ lute context scenes/p1.lute
  projectEnums (8):
    action: brace, drift, turn-away, seal, unseal, step-out, go-under
    anchor: port, center, starboard
    emotion: level, clipped, frayed, hollowed, wry, stricken
    mood: quiet, pressurized, failing, weightless
    musicAction: start, swell, cut, resume, fade-out
    reason: bridge-reached, shed-with-module      <-- enforces nothing
    vfxType: shed, klaxon, pressure-drop, frost
    volume: silent, muted, normal, raised, alarm
  ```
  ```console
  $ jq -c '.enums[] | select(.name=="reason")' artifact.json
  {"name":"reason","members":["bridge-reached","shed-with-module"]}
  ```
  `reason` sits in `projectEnums` beside the seven slots that *are* enforced, with no mark
  distinguishing it, and ships into the compiled artifact's `enums` array — the array
  `vocabulary.md` describes as making "the artifact self-describing about the vocabulary it
  was compiled against". The artifact asserts it was compiled against a two-member `reason`
  domain. It was not compiled against it at all.
- **Generality** — not specific to the name. `enums: sausage: [a, b]` behaves identically:
  accepted, listed in `projectEnums`, shipped in `enums`, read by nothing. Any project may
  declare arbitrary dead vocabulary and both surfaces will vouch for it.
- **Resolution** — `NONE — intent abandoned.` The shipped corpus leaves `reason` as a free
  string. The mirrored-state proxy in T5.4 is the closest thing to typing an ending, and it
  does not type `reason`.
- **Verdict** — `TOOL-DEFECT`. The language is honest: `directives.md` says `reason` is
  "optional and free-form … Lute assigns it no meaning", and 0.8.0 §3 agrees. So the
  *absence* of typed end reasons is a documented design choice and not a finding. What is a
  finding is that the two surfaces an author and a harness use to learn what is enforced —
  `lute context`, whose `--help` calls itself the surface "an AI needs to WRITE valid Lute",
  and the artifact's self-describing `enums` array — both report an enforced domain that is
  not enforced. This is T1.6's defect with the sign flipped: there `context` omitted things
  the project really had; here it invents one the project really does not. An author who
  declares the domain, sees it in `context`, sees it in the artifact, and concludes their
  ending ids are now typed has been told so by two tools and is wrong. A `W-DOMAIN-UNREAD`
  on a declared domain no active construct reads would close it, and the checker already
  knows the reading set — it computes `E-DOMAIN-UNKNOWN` from exactly that.

#### T5.4 — nothing says two endings are one story's alternation, and nothing says one of them is the bad one; both are reachable only by mirroring the ending into declared state and saying it twice — LANGUAGE-GAP

The assignment's two hardest questions, and they turn out to be one question. Both probes
were run to a working end before this verdict was assigned.

- **Intent** — two things a work with endings wants to state. (a) *These are the endings of
  this story* — a set, a category, an exhaustiveness claim, something that makes adding a
  third ending a change to a declared list rather than a new string in a new file. (b)
  *This one is the failure* — `::end{reason="shed-with-module"}` says the shed happened; it
  does not say it went badly.
- **Attempt (a), the ending set.** Four routes, in the order I reached for them:
  1. **Frontmatter.** `ending: true`, `terminal: true`, `outcome: failure`, and
     `endings: [bridge-reached, shed-with-module]` on the scene →
     `error [E-META-UNKNOWN-KEY] unknown top-level meta key `ending` (not a core key and
     not owned by an active plugin)` for each. A closed key set is the right design and the
     diagnostic even names the escape hatch — but the escape hatch is *ship a plugin*.
  2. **A `reason` enum domain.** T5.3: accepted, advertised, inert.
  3. **The graph.** T5.5: no notion of termination anywhere in it.
  4. **Mirror it into declared state** — the one that works:
     ```yaml
     run.ending: { type: { enum: [unspecified, bridge-reached, shed-with-module] }, default: unspecified }
     ```
     ```lute
     ::set{run.ending = "bridge-reached"}
     ::end{reason="bridge-reached"}
     ```
     and then the claim becomes checkable, because `<match>` exhaustiveness is real: a
     `<match on="run.ending">` covering one arm is
     `error [E-NONEXHAUSTIVE] non-exhaustive `<match>`: the subject's domain is not fully
     covered and there is no `<otherwise>`` plus `E-UNSET-UNCOVERED`. Add a third ending to
     the enum and every reader of it breaks until it is handled. That is exactly the
     property (a) wanted.
- **Attempt (b), the failure ending.** `::end` declares one attribute, so there is nothing
  to write on it (`E-UNKNOWN-ATTR`, T5.1). The language's actual failure vocabulary is the
  quest lifecycle, so: a quest whose `fail=` reads the mirrored enum.
  ```lute
  <quest id="theWalk" title="The Walk" start="run.shedPressure >= 0" fail="run.ending == 'shed-with-module'">
  <objective id="reachBridge" title="Reach the bridge" done="run.ending == 'bridge-reached'"/>
  <on event="questComplete">
  @narrator: The ship still had a helm.
  </on>
  <on event="questFailed">
  @narrator: The allocation was satisfied. That was all.
  </on>
  </quest>
  ```
  Driven end to end (`compile --all`, then `run` the quest artifact with
  `state: {run.ending: shed-with-module}`):
  ```
  quest theWalk -> active
  quest theWalk -> failed
    001-0500  narrator: The allocation was satisfied. That was all.
  -- quests --
    theWalk: failed
  ```
  So (b) **is** expressible. Lute has genuine, typed, engine-observable failure semantics
  with a lifecycle event and a reserved readable path. They attach to a quest, not to an
  ending.
- **Resolution** — the shipped corpus keeps the two bare `::end{reason}`s. The proxy above
  is not in `docs/examples/anseo/` because paying its price for two endings in an
  eleven-scene prologue is not a call this task should make for Task 8, and recording what
  it costs is the deliverable. What it costs:
  1. **Every ending is stated twice, in two syntaxes, and nothing checks that they agree.**
     `::set{run.ending = "bridge-reached"}` and `::end{reason="bridge-reached"}` are
     unrelated strings on adjacent lines. Swap one and both check clean.
  2. **The half that is supposed to be typed is not.** T3.2's hole applies verbatim to enum
     paths — `::set{run.ending = "shed-with-modle"}` against a declared
     `{ enum: [bridge-reached, shed-with-module] }` is `ok`, exit 0. Verified here, not
     assumed. So the *entire* protection the proxy buys is `<match>` exhaustiveness at the
     read sites; the write sites are as unchecked as the `reason` strings they mirror.
  3. **A sentinel enum member exists only to satisfy the checker.** `run.ending` has no
     honest default, and without one every quest predicate reading it is `E-MAYBE-UNSET`
     (T5.6). A two-ending story therefore declares a three-member domain, and every
     exhaustive `<match>` over it carries an arm for a value that is not an ending.
  4. **Polarity lands on the quest, not the ending.** The `end` record in the shed artifact
     is still `{"kind":"end","reason":"shed-with-module"}`. A host reading the terminator —
     the record whose entire purpose is to tell the host how the walk ended — learns nothing
     about whether that was good. It must separately be running the quest layer and reading
     `quest.theWalk.state`.
  5. **Nothing observes the join.** The quest lives in its own document; `lute run` takes
     one artifact, so no shipped tool plays the scene and the quest together. This is
     T4.7's shape exactly and is counted there, not re-filed.
- **Verdict** — `LANGUAGE-GAP`, **shape (b)**, for both halves. Nothing in the language
  *means* either claim, so nothing can check either claim; each is reachable only by
  encoding it as something else.
  - **The proxy, named.** Ending identity becomes a declared enum state path —
    `run.ending: { type: { enum: [unspecified, bridge-reached, shed-with-module] } }` — written
    as a `::set` on the line above the `::end`. Ending polarity becomes a quest lifecycle
    transition, `fail="run.ending == 'shed-with-module'"` reading that same mirrored path.
    Both were driven to a working end before this verdict was assigned, and both work:
    `<match>` exhaustiveness genuinely breaks when a third ending is added
    (`E-NONEXHAUSTIVE` + `E-UNSET-UNCOVERED`), and `quest theWalk -> failed` is a genuine,
    typed, engine-observable failure. The evidence stands as recorded; none of it is
    softened by this reclassification.
  - **What the proxy costs.** Itemised above, and the first item is the one that makes this
    shape (b) rather than a verbose spelling: **no check connects either proxy to the
    adjacent `::end`.** `::set{run.ending = "bridge-reached"}` and
    `::end{reason="bridge-reached"}` are unrelated strings on consecutive lines; an
    intentional mismatch between them checks clean, verified here and independently
    reproduced in review. Then: the mirrored write site is itself untyped, so a misspelt
    enum member is `ok` at exit 0 (cost 2, T3.2 re-verified for enum paths); a two-ending
    story must declare a three-member domain because a sentinel exists only to satisfy the
    checker (cost 3, T5.6); the `end` record — the one record whose entire purpose is to
    tell a host how the walk ended — still says nothing about whether it ended well (cost
    4); and no shipped tool plays the scene and the quest together, so nothing observes the
    join (cost 5, T4.7). The corpus ships neither proxy: `docs/examples/anseo/` keeps two
    bare `::end{reason}`s, and both claims therefore go unstated in the delivered work.
  - **Why not `ERGONOMIC`.** `ERGONOMIC` is for a working form materially worse than the
    natural one, which presumes the language can say the thing at all. It cannot. `::end`
    declares one attribute (`E-UNKNOWN-ATTR`, T5.1); no frontmatter key admits the claim
    (`E-META-UNKNOWN-KEY` on `ending`, `terminal`, `outcome`, `endings`); a declared
    `reason` domain is accepted, advertised and inert (T5.3); and the scenario graph has no
    notion of termination to hang it on (T5.5). What the proxy produces is not the claim
    said awkwardly — it is a *different* claim, about a state path and a quest, which a
    reader has to trust corresponds to the terminator beside it.
  - **The amendment is what moved it.** This entry was first filed `ERGONOMIC`, with a note
    to the controller rather than a forced verdict, because **the story itself is fully
    expressible** — both endings are written, both play, both stop, nothing was substituted
    and no beat was dropped — and the then-current criterion's second sentence ("You
    changed the story to fit the tool") read as a precondition. The controller amended
    `LANGUAGE-GAP` so that either shape alone qualifies. Shape (b) is precisely this case:
    the work is intact, the claim *about* the work is not expressible, and only a lossy
    proxy reaches it. One optional attribute on `::end` — a declared-domain `reason`, or an
    `outcome` — would make both claims mean something, and would let something check them.

#### T5.5 — `::end` is not an ending, and no tool will tell you whether a route reaches one — DOC-WRONG

- **Intent** — the structural question a branching work lives or dies on. Two terminals now
  exist; ask the tooling (i) which nodes are terminals, (ii) whether every route reaches
  one, (iii) whether a route can dead-end without terminating.
- **Attempt** — `lute scenario docs/examples/anseo`, `scenario reach`, `--format json`, plus
  a probe scene declaring itself downstream of a terminal.
- **Result** —
  ```
  project root: docs/examples/anseo
    topological layers:
      layer 0: scene(anseo.s01ep01)
      layer 1: scene(anseo.s01ep02)
      layer 2: scene(anseo.s01ep10), scene(anseo.s01ep11)
    edges (prerequisite -> dependent) [atom kind(s)]:
      scene(anseo.s01ep01) -> scene(anseo.s01ep02) [visited]
      scene(anseo.s01ep02) -> scene(anseo.s01ep10) [visited]
      scene(anseo.s01ep02) -> scene(anseo.s01ep11) [visited]
  ```
  ep10 and ep11 are leaves — but that is an `after:`-derived property and coincidence. The
  JSON node record is `{"id","kind","prereq","reach"}` and has no terminal field.
  `scenario reach anseo.s01ep10` reports `Reachable` and its prerequisites, nothing about
  what happens when you get there. Nothing distinguishes ep10 (terminates) from ep01 (does
  not); nothing flags a leaf that never terminates; the project checked clean through T1–T4
  with **zero** `::end` in it and no surface remarked on that either.
- **The probe that explains why.** A third scene, `after: 'visited("anseo.s01ep10")'` —
  declaring itself downstream of the scene whose only route ends in `::end`:
  ```
  ok: /tmp/t5probe/scenes/after-the-end.lute (0 warning(s))
      layer 3: scene(anseo.s01ep12)
      scene(anseo.s01ep10) -> scene(anseo.s01ep12) [visited]
  ```
  Clean, layered, `Reachable`. My first read was that this is a missing analysis. It is
  not — **it is correct, and it is correct because `::end` does not mean what its name says.**
  `directives.md` is precise about this: `::end` "is exactly equivalent to falling off the
  end of the command array, except that it carries a reason", and `lute-cli`'s own test is
  named `ending_matches_falling_off_the_end_except_for_the_reason` (identical `exit`,
  identical `state`). Every document falls off the end of its command array. So `::end` is
  a `break` with a label attached: it ends *this document's walk*, which ending the document
  does anyway, and it means nothing whatsoever about the run. `visited("anseo.s01ep10")`
  is satisfiable *because the scene was visited* — the walk stopped, the engine routes on.
  There is therefore no "does every route reach an ending" property to compute, because
  Lute has no ending to reach. `bridge.lute` with `::end` and `wake.lute` without it are,
  at the run level, the same kind of document.
- **What `::end` actually buys, precisely.** Two things, both real and both local: the
  free-form `reason` on one artifact record, and `W-CODE-AFTER-END` dead-code analysis
  within one straight-line body. Nothing else. It is well named for the first and
  mis-named for what an author reads into it.
- **Resolution** — `NONE — intent abandoned.` (i), (ii) and (iii) are unanswerable by any
  shipped tool, and (ii)/(iii) are not well-formed questions in the language's model.
- **Verdict** — `DOC-WRONG`, and located on one specific sentence rather than on the
  reference pages, which are accurate. The homepage's "Built for scenarios you can trust"
  card (`packages/website/src/content/docs/index.mdx:251-255`) reads:

  > Every scenario provably terminates — no infinite loops, no unbounded recursion — and
  > `::end` makes an ending explicit, so anything written after it is reported as dead
  > rather than quietly shipped.

  Both halves of the clause after the dash are false. `::end` does not make *an ending*
  explicit — it makes a document's early exit explicit, and `directives.md` says so two
  clicks away. And "anything written after it" is *not* "anything": it is the immediately
  enclosing straight-line body only, which `directives.md` also states correctly and this
  card contradicts. The load-bearing falsehood is the last four words — see T5.7, where the
  dead line is reported *and* quietly shipped, to the artifact, to the localization export,
  and to the production word count, at exit 0. This is the table's own argument for ranking
  `DOC-WRONG` above `DOC-GAP`: silence makes an author search, and the reference pages would
  have answered them. This sentence makes them stop searching, on the front page, in the
  section titled "scenarios you can trust". An author who reads it believes the language has
  endings and that dead content cannot reach the artifact; both beliefs are wrong and
  neither will be corrected by anything that fails.

#### T5.6 — a guard the checker honours in a scene is ignored in a quest predicate, and the diagnostic blames its absence — TOOL-DEFECT

Found reaching for T5.4(b)'s quest gate, on the un-defaulted ending enum.

- **Intent** — `fail="run.ending == 'shed-with-module'"` on a quest, where `run.ending` is a
  declared enum with no default (it has no honest default — before an ending, there is no
  ending). `E-MAYBE-UNSET`, correctly. `state-model.md` names the remedy: "a dominating
  `::set{p = …}` write **or a guard (`has(p)` / `isSet(p)`)** proves it".
- **Attempt** — apply the documented remedy in the only place a quest predicate has:
  ```lute
  <quest id="theWalk" … fail="isSet(run.ending) && run.ending == 'shed-with-module'">
  <objective id="reachBridge" … done="isSet(run.ending) && run.ending == 'bridge-reached'"/>
  ```
- **Result** — unchanged, and anchored on the guarded read:
  ```
  quests/the-walk.lute:8:74: error [E-MAYBE-UNSET] state path `run.ending` may be read before it is set (no default, no dominating `::set`, no guard) (dsl §9.4)
  quests/the-walk.lute:9:60: error [E-MAYBE-UNSET] state path `run.ending` may be read before it is set (no default, no dominating `::set`, no guard) (dsl §9.4)
  ```
  *"no guard"* — with `isSet(run.ending) &&` five characters to its left.
- **The narrowing exists; it is one construct away.** Three probes, same project, same path,
  same expression:
  | where | expression | result |
  |---|---|---|
  | scene content line `when=` | `isSet(run.ending) && run.ending == 'bridge-reached'` | `ok`, exit 0 |
  | quest `<objective done=>` | `isSet(run.ending)` alone | `ok`, exit 0 |
  | quest `<objective done=>` | `isSet(run.ending) && run.ending == 'bridge-reached'` | `E-MAYBE-UNSET` at col 35 |
  | quest `<quest fail=>` | `has(run.ending) && run.ending == '…'` | `E-MAYBE-UNSET` |
  So `isSet`/`has` are admitted in a quest predicate, and intra-expression `&&`
  short-circuit narrowing is implemented — for a scene line guard. The quest predicate slot
  does not run it.
- **Resolution** — added a sentinel `unspecified` member and `default: unspecified` to the
  enum. That works, and it is T5.4's cost item 3: a two-ending story declaring a
  three-member domain, and every exhaustive `<match>` over it carrying an arm for a
  non-ending, because the only other route to a quest gate on an optional path is closed.
- **Verdict** — `TOOL-DEFECT`, and it is the misdirecting-diagnostic case the protocol
  ranks near the top. The message does not say "a guard here must dominate the read" or
  "quest predicates are evaluated without flow context"; it says **"no guard"**, which is
  false about the text it is pointing at. An author who has just read `state-model.md`,
  applied the documented remedy, and been told the remedy is absent has no next move —
  the working fix (distort the domain with a sentinel) is not hinted at anywhere in the
  message, and the reason the fix is needed is invisible. Either arm closes it: run the
  same narrowing in the predicate slot, or say what is actually true in the message.

#### T5.7 — content after `::end` is reported *and* shipped: to the artifact, to `loc export`, and to the production word count — SPEC-WRONG

The assignment asks whether warning is the right severity for authored content that will
never play. Here is what the severity buys and what it costs, then my answer.

- **Attempt** — the required Step 3 probe. One content line after the shed terminator:
  ```lute
  @purser{code="0010" emotion="level" os}: Module released. Allocation is satisfied.
  ::end{reason="shed-with-module"}
  @vesna{code="0020" emotion="hollowed"}: Then we're the allocation.
  ```
- **Result** — the diagnostic is exemplary: one per body, at the first dead node, spec
  message verbatim, `--deny`-promotable (T5.1). And then, at exit 0:
  ```console
  $ lute check-project docs/examples/anseo          # ok, 5 file(s).  EXIT=0
  $ lute compile …/shed.lute -o /tmp/t5-dead.json
  {"kind":"line","addr":"001-0100","text":"Module released. Allocation is satisfied."}
  {"kind":"end","addr":"001-0200","reason":"shed-with-module"}
  {"kind":"line","addr":"001-0300","text":"Then we're the allocation."}   <-- proven dead

  $ lute loc export docs/examples/anseo -o /tmp/t5-loc.json
  1 lines untagged — run lute tag
  $ jq -r '..|objects|select(has("text"))|"\(.code)  \(.text)"' /tmp/t5-loc.json | grep -i allocation
  0010  Module released. Allocation is satisfied.
  0020  Then we're the allocation.                                        <-- for translation

  $ lute loc report docs/examples/anseo | grep shed
  docs/examples/anseo/scenes/shed.lute      2      9      …               <-- billed
  #  and with the probe line removed, the same row reads:  1      5
  ```
  The line the checker has *proven* unreachable becomes a command record with a real
  address, an entry in the localization export, and — by the difference between those two
  report rows — exactly 1 line / 4 words of the production budget. `loc export --help`
  calls itself "Extract **every translatable content line**".
  Money is spent translating and recording a line that cannot play.
- **The asymmetry.** This is one reachability pass with two severities. Its sibling verdict
  on provably-dead gated content is `E-ARM-DEAD` — an **error** — so that content never
  reaches an artifact, never reaches a translator, and never reaches a budget. Verified on
  a scratch scene, both forms, outside Anseo:
  ```console
  $ lute compile t5arm/b.lute -o /tmp/t5arm-b.json     # <choice … when="false">
  t5arm/b.lute:17:1: error [E-ARM-DEAD] choice can never fire: guard `false` is provably false (dsl 0.4 §5.2)
  1 error(s); no artifact emitted
  $ lute compile t5arm/a.lute -o /tmp/t5arm.json       # @narrator{when="false"}
  t5arm/a.lute:13:45: error [E-ARM-DEAD] this gated line can never be shown: its `when` guard is provably false (dsl 0.4 §7.2, §5.2)
  1 error(s); no artifact emitted
  ```
  Post-`::end` content is the same proof of the same property in the same pass, and it
  ships. Nothing about the `::end` case is less certain: `W-CODE-AFTER-END` fires only on
  the provable straight-line case, which is why its scope is so carefully bounded (a
  sibling `<choice>`'s `::end` says nothing, and correctly does not warn).
- **My answer, with reasons.** Warning is the wrong severity; `E-CODE-AFTER-END` is right,
  and I would ship it as an error even though it is the more disruptive change.
  1. *The proof is total, not heuristic.* Every case this fires on is unreachable in the
     same sense `E-ARM-DEAD`'s is. Two severities for one proof needs a justification and
     0.8.0 §3 offers none.
  2. *Warning severity is load-bearing on the thing it fails to prevent.* A warning's
     contract is "this may be fine". Here the tool knows it is not fine, and the
     consequence is not stylistic: it is bytes in a shipped artifact and invoices in a
     localization pipeline.
  3. *`--deny` is not a mitigation.* Denial is per-project CI policy, chosen by whoever set
     up the build, and it promotes on a code the author of the dead line may never see. The
     default is what most projects get.
  4. *The counter-argument, and why it loses.* Dead content after a terminator is plausibly
     work-in-progress an author wants to keep while iterating — real, and the reason a
     warning was chosen. But that is also true of a dead `<branch>` arm, which is an error;
     comment it out, or move it above the `::end`. Iteration convenience does not outweigh
     shipping proven-dead content to paid downstream consumers, and the language already
     made that trade the other way one code over.
  5. *If it must stay a warning*, then the artifact and `loc` are the wrong place to pay
     for it: `compile` should drop provably-dead records, or `loc export` should skip them.
     Reporting *and* shipping is the one combination with no defensible reading.
- **Resolution** — probe line removed. `check-project docs/examples/anseo` back to
  `ok (5 file(s))`.
- **Verdict** — `SPEC-WRONG`. No implementation is at fault and the agreed design is the
  defect. 0.8.0 §3 specifies a warning; `directives.md` documents that warning and how to
  promote it; the checker emits exactly it, once per body, at the first dead node; and
  `compile` and `loc export` faithfully retain a record the language told them to keep.
  Language, docs, checker, compiler and localization all agree — which is why `DOC-GAP`,
  `DOC-WRONG`, `AUTHOR-ERROR` and `TOOL-DEFECT` are all false, and why
  `ERGONOMIC`/`LANGUAGE-GAP` do not apply (nothing here is about expressing anything). What
  the spec says is that provably-dead content after a terminator is a warning. What it
  should say is `E-CODE-AFTER-END`, an **error**, for the four reasons argued above (items
  1–4). This entry was filed as fitting no verdict and escalated; the seventh row exists
  for it.
  - **The strongest single fact, and it is in the checker's own source.**
    `W-CODE-AFTER-END` and `E-ARM-DEAD` are not two analyses that happen to agree about
    reachability — they are reached through the **same recursive reachability walk**.
    `crates/lute-check/src/reachability.rs` is one "§5.2/§5.3 whole-document reachability
    pass", and its `walk_reach` calls `check_code_after_end(nodes, diags)` on entry to
    every body it descends into, because "`nodes` is by construction exactly ONE
    straight-line body at every call site … so the `W-CODE-AFTER-END` scan rides this
    recursion instead of duplicating it". One walk, one PROVABLE-ONLY boundary, two
    severities — and the permissive branch is the one that ships bytes. Confirmed
    independently in review.
  - **Fallback, if compatibility forbids the error immediately.** Then `compile` and `loc`
    must at least **prune** the proven-dead content rather than shipping it: no addressed
    command record, no `loc export` entry, no `loc report` words. This is the reviewer's
    fallback position and item 5 above, reached separately. Reporting *and* shipping is the
    one combination with no defensible reading — and it is, word for word, what the front
    page promises cannot happen (T5.5).

  (T5.5 is `DOC-WRONG` rather than `SPEC-WRONG` because there the falsehood is in prose the
  reference pages already contradict — a wrong sentence, not a wrong decision. This entry
  has no wrong sentence anywhere.)

#### T5.8 — the `anchor` domain's declared `default:` cannot be written on purpose — ERGONOMIC

- **Intent** — Vesna at the helm, dead centre. The bridge is the one scene in the prologue
  where where she stands is the point, so the staging says so:
  `::auto{character="vesna" anchor="center" action="brace"}`. Anseo's `anchor` slot is
  `{ members: [port, center, starboard], default: center }`, declared in T1 long before this
  scene existed.
- **Result** — a permanent warning on the finished scene:
  ```
  docs/examples/anseo/scenes/bridge.lute:11:34: warning [W-INJECT-CONFLICT] `vesna` is shown with an explicit `anchor="center"` that `auto-anchor-on-show` would otherwise inject
  ```
  The message is accurate and the mechanism is right (no double injection, the author's
  anchor wins). But the *only* authored shape it fires on is agreement: writing a
  **different** anchor is honoured silently, and writing **none** is silent. So the one
  value an author cannot state explicitly is the one the schema calls the default — and
  `port`, which every other Anseo scene writes explicitly, is fine.
- **Resolution** — kept as written, warning and all. The three alternatives are all worse:
  delete a true statement about the staging; change `world`'s `anchor` `default:` to a
  member the project never uses, distorting the schema to silence a diagnostic; or omit the
  attribute and rely on an injection rule, which reads as an oversight to the next author.
  This is the first diagnostic in five tasks the corpus carries deliberately, so, to be
  unambiguous for Task 8 and for review: **`bridge.lute`'s warning is intentional and is
  the evidence for this entry, not an unfinished edit.**
- **Verdict** — `ERGONOMIC`, and slightly worse than it looks because there is **no
  suppression**. `lute check` has `--deny <CODE>` and `--deny-warnings` and no `--allow`,
  and there is no in-source acknowledgement — so a project on `--deny-warnings` in CI (which
  the toolchain's own docs encourage) cannot express "centre, on purpose" at all: it must
  either not say it or edit its vocabulary. `W-INJECT-CONFLICT` earns its keep in its other
  role — T2.1 cites it as the precedent for "this staging attribute is not doing what you
  think" — but redundancy and conflict are different claims, and this is the redundant case
  wearing the conflicting case's name and severity. A note-level severity, or an `--allow`,
  or simply not warning when the explicit value equals the injected one (nothing is lost,
  nothing is ambiguous, nothing is overridden) would each close it.

#### T5.9 — `lute trace` records the terminator and drops its only payload — ERGONOMIC

- **Intent** — preview the two endings in the author's preview tool, which is where the
  reasons should be most visible.
- **Result** — the walk stops in the right place and the terminator is recorded, but
  reasonlessly, in both renderings:
  ```console
  $ lute trace docs/examples/anseo/scenes/bridge.lute
    ## Shot 1.
      <auto>
      @vesna  Whatever's left of the ship, it's steering.
      <end>
  trace complete: 0 decisions
  $ lute trace …/bridge.lute --json | jq -c '.steps[]'
  {"kind":"directive","tag":"end","component_boundary":null}
  ```
  `<end>` is rendered exactly like `<auto>`, and the JSON `TraceReport` has no exit or
  disposition field at all (`["coverage","decisions","file","notes","seeds","steps","unresolved"]`),
  so a harness reading `trace --json` cannot tell a walk that was terminated from one that
  ran out of nodes, nor recover which ending it just previewed. `lute run` prints
  `end    reason=bridge-reached` from the same information.
- **Resolution** — used `lute run` on the compiled artifact to read the reasons back.
- **Verdict** — `ERGONOMIC`, deliberately not `TOOL-DEFECT`: `trace` records directive
  *tags* and not attributes, uniformly — `<auto>`'s `anchor`/`action` are dropped the same
  way — so this is a consistent terseness rather than a broken contract, and T3.12 records
  that `trace` renders branching honestly. The cost lands disproportionately on `::end`
  because `reason` is not one attribute among several: it is the terminator's *entire*
  payload, the only thing distinguishing it from falling off the end of the document
  (T5.5). A project with several endings previews them all as an identical `<end>`.

#### T5 summary

Nine entries: one *worked well* (T5.1), two `TOOL-DEFECT` — the vocabulary surfaces (T5.3)
and a misdirecting diagnostic (T5.6) — one `DOC-WRONG` (T5.5), three `ERGONOMIC` (T5.2,
T5.8, T5.9), one `LANGUAGE-GAP` (T5.4), and one `SPEC-WRONG` (T5.7). Every entry carries
exactly one of the seven verdicts; no `DOC-GAP` and no `AUTHOR-ERROR` scored here. T5 is
the first task to score either of the table's two newest readings, and both come from the
same place — what `::end` is and is not. The two claims a work with endings most wants to
make are not expressible, only proxyable (T5.4, shape (b)); and the one guarantee the
language does make about a terminator is specified at a severity that lets it be broken at
exit 0 (T5.7).

**`::end` works. It is also not what its name, or the front page, says it is.** The
construct itself is the cleanest first-use in this log: nine directives in `lute.core`, the
ninth exercised for the first time by this task, and it lowered, addressed, ran, and
dead-code-analysed correctly on the first attempt with no probing required (T5.1). What
five entries then measure is a gap between the construct and the concept an author brings
to it. `::end` is `break` plus a label: `directives.md` says it is "exactly equivalent to
falling off the end of the command array, except that it carries a reason", the CLI's own
test is named after that equivalence, and my after-the-terminator probe confirms it — a
scene may declare itself `after:` a scene that terminates, and that is *correct*, because
terminating a document's walk is what every document does. There is no ending in Lute. So
the three questions a branching work actually asks — which nodes are terminals, does every
route reach one, can a route dead-end without terminating — are not unanswered by the
tooling (T5.5); they are unaskable, and `lute scenario`'s node record has no field for
them because there is no property to put there.

**Two endings, one story is expressible only as two spellings of the same word, in
different languages, with nothing checking them against each other.** T5.4 is the entry to
read. The set claim and the polarity claim both resolve to the same workaround — mirror
each ending into a declared enum state path, `::set` it beside the `::end`, and let
`<match>` exhaustiveness (real, and good) or a quest `fail=` (real, typed, and observable
as `quest.…state = failed`) carry the structure. It works; it was driven end to end. It
costs a schema path the story does not need, a sentinel enum member that exists only
because a quest predicate ignores its own documented guard (T5.6), each ending said twice
with no cross-check, a `::set` half that is as untyped as the `reason` it mirrors (T3.2,
re-verified here for enum paths), a whole quest document to carry polarity, and an `end`
record that still tells a host nothing about whether the walk ended well. One optional
attribute on `::end` — a declared-domain `reason`, or an `outcome` — would collapse most
of that. That mirroring is a lossy proxy, not a wordier spelling — nothing in the language
means "these are the endings" or "this one is the failure", so nothing checks that the
proxy and the terminator beside it agree. That is why T5.4 is `LANGUAGE-GAP` shape (b).

**Two findings are about tools vouching for things that are not true, which is now the
dominant pattern of this log.** T5.3: declare `enums: reason: [...]`, and `lute context`
lists it in `projectEnums (8)` beside the seven enforced slots while the compiled artifact
ships it in the `enums` array documented as describing "the vocabulary it was compiled
against". It enforces nothing; any domain name behaves this way. That is T1.6 with the sign
flipped — `context` omitting what the project has, now inventing what it does not. T5.6:
`isSet(p) && p == x` narrows in a scene line guard, does not narrow in a quest predicate,
and the resulting `E-MAYBE-UNSET` says **"no guard"** while pointing five characters right
of one. Both are cheap fixes on information the checker already has.

**The one thing this task would change first is neither.** It is T5.7, the task's one
`SPEC-WRONG`: `W-CODE-AFTER-END` is a warning, so at exit 0 a line the checker has *proven*
unreachable becomes an addressed command record, an entry in `lute loc export` ("every
translatable content line"), and 4 words of `lute loc report`'s production budget — while
the same reachability pass's verdict on a dead `<branch>` arm is `E-ARM-DEAD`, an error,
which ships nothing. And it is not merely the same *kind* of proof: it is the same
recursive walk, one function call apart — `reachability.rs`'s `walk_reach` runs the
dead-code scan on entry to every body it recurses into rather than duplicating the
recursion. One walk, two severities, and the permissive one is the one that reaches a
translator's invoice. Reported *and* quietly shipped — which is, word for word, what the
homepage promises cannot happen (T5.5). `E-CODE-AFTER-END` is the fix; failing that,
`compile` and `loc` must prune what the checker has already proven dead.

Two housekeeping notes for whoever reads the corpus next. `bridge.lute` carries a
deliberate `W-INJECT-CONFLICT` (T5.8) — the `anchor` domain's declared `default:` is the
one member an author may not write on purpose, there is no `--allow` and no in-source
suppression, and the scene keeps the true statement rather than the clean output. And both
`after:` routes here are Task 8's to repoint; the graph in T5.5 is provisional by design.

One finding raised while probing T5.4's schema is deliberately **not** filed here:
`state-model.md`'s only `enum` state declaration example does not parse. It is outside
T5's remit (`::end`) and is **held for Task 10**, which owns the documentation gates; the
full reproduction is in `.superpowers/sdd/anseo/task-5-report.md`.

### T6 — The Purser component

Toolchain 0.9.0 / language 0.9.0 / IR 0.9.0, `./target/debug/lute`. One component added:
`components/purser-interject.component.lute`, the project's first `component:` document, and
`scenes/cryobank.lute`'s inline Purser line replaced by a `::use`. Components are the
language's only content-reuse mechanism, so this task is the first measurement of whether
Lute scales past one episode.

**Process note, recorded because the brief predicted it and it was true.** The binary at
`target/debug/lute` had mtime `14:05`; commit `3ff3543`, which closed the standalone leg of
the component-body contract, landed at `14:18`. The binary was stale. Rebuilding
`lute-cli` took 12s and every probe below was run against the rebuilt binary. A drive test
that had trusted the stale binary would have recorded the false green `3ff3543` fixed as a
live finding.

#### T6.1 — the component-body contract is enforced on both legs, and each diagnostic names its own rationale — WORKED WELL

- **Intent** — the Purser interjects in every module of the prologue at a different
  pressure. One block of words, one param, invoked eleven times. Then: establish that the
  presentational contract is real on both routes, because the whole value of a
  once-authored block is that the checker actually guards it.
- **Attempt** — the brief's Step 3 probe. A `<branch>` temporarily added after the
  component's `</match>`, then three checks: the component file alone, the importing
  scene, and `check-project`.
- **Result** — `E-COMPONENT-BODY` on every leg, exit 1:
  ```
  $ lute check docs/examples/anseo/components/purser-interject.component.lute
  …component.lute:28:1: error [E-COMPONENT-BODY] a component body must be presentational
    (dsl 0.4 §6.2): the `<branch probeOnly>` logic block is not allowed — presenting a menu
    records the selection, a state write; only a param-scoped `<match>` is admitted

  $ lute check docs/examples/anseo/scenes/cryobank.lute --project docs/examples/anseo
  …cryobank.lute:1:1: error [E-COMPONENT-BODY] component `purserInterject`
    (/…/purser-interject.component.lute): a component body must be presentational … (same tail)
  ```
  The same contract holds for state reads, with its own code and its own remedy, again on
  both legs — a `{{run.shedPressure}}` in a component body:
  ```
  error [E-COMPONENT-STATE] `run.shedPressure` reads ambient state — a component body may
    not depend on it; bind it through a param (dsl 0.4 §6.2)
  ```
  And the admitted exception works as specified: the param-scoped `<match on="@pressure">`
  with `<when is="rising">` / `<otherwise>` checks clean on both legs, and the `os` delivery
  flag survives the expansion (`"role": "offscreen"` in the artifact).
- **Resolution** — probe removed; the corpus checks clean at 6 files.
- **Verdict** — worked well, and worth stating in this register: each of the three
  messages says *why*, not just *no*. `E-COMPONENT-BODY` explains that a menu records a
  selection and that a selection is a state write — an author who reads it understands the
  rule rather than memorising a blacklist. `E-COMPONENT-STATE` names the remedy ("bind it
  through a param") in the message. This is the best-explained restriction measured in six
  tasks. It is also the restriction that turns out to matter least, for reasons T6.2 gives.

#### T6.2 — a component has no fixed meaning: one body, two callers, two different command streams, zero diagnostics — SPEC-WRONG

T6's structural finding. The vocabulary-scope limitation is documented as a *scoping*
inconvenience; it is a *semantic* one, and the difference is what a production pays.

- **Intent** — the Purser says the same thing, the same way, in every module. That is the
  entire reason to write a component instead of eleven lines. So: establish what "the same"
  is guaranteed to mean when two scenes with different vocabularies invoke the same
  component.
- **Attempt** — constructed in `/tmp/t6fix/proj`. Two vocabularies that agree on every member
  an author can see and on **every** other declaration, and disagree on exactly one thing an
  author cannot see — `action.exits`:
  ```yaml
  # vocabA.schema.yaml            # vocabB.schema.yaml
  emotion: [level, clipped]       emotion: [level, clipped]
  action:                         action:
    members: [brace, go-under]      members: [brace, go-under]
    exits: [go-under]               exits: [brace]
  anchor: {members: [port,center],  anchor: {members: [port,center],
           default: port}                    default: port}
  ```
  `diff vocabA.schema.yaml vocabB.schema.yaml` is one line, the `exits:` line. That control
  matters and cost a re-run: the first version of this probe also gave the two vocabularies
  different `anchor.default`s, which makes any causal claim about `exits` unsupported. It also
  hides a second caller-dependent divergence — with both defaults set to `center`, so that the
  component's explicit `anchor="center"` coincides with the injected default, caller B alone
  earns `W-INJECT-CONFLICT` ("`purser` is shown with an explicit `anchor="center"` that
  `auto-anchor-on-show` would otherwise inject") and caller A does not, because in A the sprite
  is an exit and nothing is *shown*. The transcript below sets both defaults to `port` so that
  interaction is out of the way and `exits` is the only live variable.

  One component, `uses: ../vocabA.schema.yaml`, body:
  ```lute
  ::auto{character="purser" anchor="center" action="go-under"}
  @purser{code="0010" emotion="clipped"}: The schedule advances.
  ```
  Two scenes, identical but for their `uses:`, each `::use{component="interject" pressure="rising"}`.
- **Result** — `ok: . (3 file(s), 0 project-wide warning(s))`, exit 0, and **no warning on any
  file** — not the component, not either caller. Nothing mentions that two callers disagree.
  And the two compiled artifacts are not the same artifact:
  ```console
  $ jq -c '.commands[]' outA.json        # caller A — TWO commands
  {"kind":"sprite","addr":"001-0100","character":"purser","anchor":"center","action":"go-under",
   "exit":true,"source":{"component":"interject"}}
  {"kind":"line","addr":"001-0200","role":"dialogue","speaker":"purser","text":"The schedule advances.",
   "emotion":"clipped","lineId":"probe.s01ep01.purser_0010","voiceKey":"purser-0010",
   "source":{"component":"interject"}}

  $ jq -c '.commands[]' outB.json        # caller B — THREE commands
  {"kind":"sprite","addr":"001-0100","character":"purser","anchor":"center","action":"go-under",
   "source":{"component":"interject"}}
  {"kind":"sprite","addr":"001-0200","character":"purser","preload":true,"emotion":"clipped",
   "provenance":{"injected":true,"by":"entry-emotion-lookahead",
     "reason":"pre-loading `purser`'s first emotion `clipped` seen ahead of the entrance"},
   "source":{"component":"interject"}}
  {"kind":"line","addr":"001-0300","role":"dialogue","speaker":"purser","text":"The schedule advances.",
   "emotion":"clipped","lineId":"probe.s01ep02.purser_0010","voiceKey":"purser-0010",
   "source":{"component":"interject"}}
  ```
  Both compiles exit 0. In caller A the Purser **leaves the scene** on that sprite
  (`exit: true`) and then speaks a line after having left (T2.4's defect, arriving here through
  a component whose author wrote no exit at all). In caller B the same line is an **entrance**,
  so the `entry-emotion-lookahead` rule fires and injects a whole extra command that exists in
  neither the component nor the scene. Two commands versus three, different addresses for
  the same authored line, opposite staging semantics. One body, one differing enum attribute
  that no author can see from the callsite. No diagnostic.
- **Resolution** — for Anseo, the mitigation the docs prescribe: `purser-interject.component.lute`
  declares `uses: ../vocabulary.schema.yaml` and every caller reaches the *same* project-root
  schema, so divergence is impossible by construction. That is a discipline, not a guarantee —
  nothing checks it, and the next author to give one scene its own `action` domain gets the
  transcript above at exit 0.
- **Verdict** — `SPEC-WRONG`. No implementation is at fault and the docs are not silent:
  `vocabulary.md` has a section headed "Known limitation: a component body resolves against
  the importing document", it opens "State it plainly, because it will bite someone", it
  says a component's own `uses:` and inline `enums:` "are both discarded at parse", and it
  names the future direction ("A *component schema* surface … is a named future direction
  filed separately"). Everything works exactly as specified. **The specification is the
  defect**, and specifically the sentence that frames it: *"This is a scoping limit, not a
  checking divergence."* That is true about *checking* and false about *meaning*. The
  discarded `uses:` does not merely fail to bring vocabulary along — it means the language's
  only reuse construct has **no denotation of its own**. `interject` is not a block of
  content; it is a block of content *per caller*, and the callers can disagree about whether
  it removes a character from the stage.

  Two alternatives, either sufficient, both cheap. (i) The named future direction: `::use`
  carries the component's declared domains into the expansion, so the component means what
  its author wrote. (ii) If that is too large for a point release, the *detection* is nearly
  free, because `check-project` already resolves every component and every caller in one
  run: for each slot a component body touches, compare the component's own declared domain
  against each caller's and emit `W-COMPONENT-VOCAB-DIVERGENT` when they differ. That would
  have fired twice on the transcript above. As shipped, the only reuse mechanism in the
  language is the one construct whose behaviour nothing in the project can pin down, and the
  doc that warns you about it under-describes what it costs.

#### T6.3 — no check is both caller-aware and able to point into the component: the fault is reported N times, in the N files that are correct, while the one file to edit reports `ok` — TOOL-DEFECT

- **Intent** — a component is authored once and validated per caller. Ask the practical
  question directly: if it is correct against caller A and broken against caller B, when and
  where do I find out?
- **Attempt** — same `/tmp/t6/proj`, now with the component pointed at `vocabB` and using a
  member only `vocabB` declares (`emotion="molten"`), with seven callers of various depths
  in the project.
- **Result** — the fault **is** detected, and `check-project` **is** the command that detects
  it: exit 1, `failed: .`. State that first, because it bounds the finding — this is not a
  hole in coverage, it is a hole in *localisation*. What no command gives you is a check that
  is simultaneously caller-context-aware and able to point at the component's own source span.
  The two legs split those properties between them and neither has both.

  The component's own check **passes**, with and without `--project`:
  ```console
  $ lute check components/interject.component.lute                  # ok, exit 0
  $ lute check components/interject.component.lute --project /tmp/t6/proj   # ok, exit 0
  ```
  `check-project` catches it, and reports it once per caller, at line 1 of the wrong file:
  ```console
  $ lute check-project .                                            # exit 1
  scenes/sceneA.lute:1:1:  error [E-BAD-ENUM] component `interject` (/…/interject.component.lute):
    `molten` is not a valid value for `emotion` of `::purser` (expected one of: level, clipped)
  scenes/nest.lute:1:1:    error [E-BAD-ENUM] component `interject` (…) : (identical)
  scenes/callsite.lute:1:1: error [E-BAD-ENUM] component `interject` (…) : (identical)
  components/outer.component.lute:1:1: error [E-BAD-ENUM] component `interject` (…) : (identical)
  ok: components/interject.component.lute (0 warning(s))
  failed: . (… 0 project-wide error(s), 0 project-wide warning(s))
  ```
  So the file the author must edit is the one file reporting `ok`, and the N files reporting
  errors are the N files that are correct. With eleven modules that is eleven identical
  messages, all at `1:1`, none of them in the component. The caller-context-awareness is real
  and worth crediting — `sceneB.lute`, whose vocabulary *does* declare `molten`, correctly
  reports `ok` in the same run. It is the position that is thrown away.

  **And the position exists.** The standalone leg printed `28:1` for T6.1's `<branch>`, and in
  the same `check-project` run above it prints `reader.component.lute:9:54` for an
  `E-COMPONENT-STATE` inside a component body — a component-internal span, on the standalone
  leg, in the caller-aware command. The caller leg prints `1:1` for a fault it has located
  precisely enough to quote the offending value and enumerate the legal alternatives. The
  checker knows the span inside the component body and does not pass it through on the one
  fault class that needs it. There is also no way to ask the question the author actually has
  — "check this component as caller A would" — because `--project` does not change the
  component's resolution root (proven above): a standalone check resolves against the
  component's own `uses:`, which is the one vocabulary that is *never* the one that applies at
  runtime.
- **Resolution** — none available. The working procedure is: never trust `lute check` on a
  component file, always `check-project`, and read the component path out of the message
  prefix rather than the file position.
- **Verdict** — `TOOL-DEFECT`, on the protocol's highest-priority ground — a diagnostic that
  misdirects. Not "a component cannot be validated": it can, by `check-project`, which returns
  failure. It is a *double misdirection about where*, and the pairing is what makes it
  expensive: `lute check <component>` reports `ok` on a component that cannot work (not a
  false green — it is true against the component's declared vocabulary — but a *meaningless*
  green, since that vocabulary governs nothing), while the caller-side error points at the
  caller's frontmatter for a fault that is N files away. The fix is two small ones on
  information the checker already holds: forward the component-internal span into the prefixed
  diagnostic (`…/interject.component.lute:10:34`, reported at the caller), and roll the N
  identical caller reports into one per *problem* the way T1.3 praises `E-UNDECLARED` for doing
  (`(+6 more callers)`).

#### T6.4 — `::set` is forbidden in a component body, and the rule is right — WORKED WELL

Recorded as a vindication, because a maturity report that never vindicates a restriction is
not an assessment.

- **Intent** — a Purser interjection that *costs power*. Reading the crew's draw is the
  Purser's whole function in this story, and the beat is "it notices, and the schedule
  advances" — one command, one price. So the natural first form puts the price in the
  component, beside the words:
  ```lute
  ## Costly
  ::set{run.shedPressure += 1}
  @purser{code="0010" emotion="level"}: Allocation notes the draw.
  ```
- **Result** — refused, on both legs, with the rationale in the message:
  ```
  …costly.component.lute:9:1: error [E-COMPONENT-BODY] a component body must be presentational
    (dsl 0.4 §6.2): `::set` of `run.shedPressure` writes state — only a param-scoped `<match>`
    is admitted for logic, not a state write
  …costly.component.lute:9:7: error [E-UNDECLARED] state path `run.shedPressure` is not declared
    in `state:` (dsl §9.4)
  ```
- **Resolution** — the price moved to the callsite, which checks clean and is what the
  committed corpus does in spirit (cryobank's arms carry their own `::set`s):
  ```lute
  ::set{run.shedPressure += 1}
  ::use{component="interject" pressure="rising"}
  ```
- **Verdict** — worked well; the rule serves the author here, and I would not change it. The
  reason is not purity, it is legibility under reuse. A component that writes state is an
  invisible `+= 1` fired eleven times from eleven files that do not say so; the one number
  the whole Anseo prologue is about would become impossible to audit by reading. Putting the
  cost at the callsite makes the price legible exactly where the beat is priced, and it lets
  different modules charge different amounts for the same words — which is what a real
  production wants anyway. The restriction is also *consistent*: T6.1's `E-COMPONENT-STATE`
  blocks the read for the same reason, and "bind it through a param" is genuinely the right
  answer.

  Two honest caveats, neither of which changes the verdict. **The pairing is unenforced.**
  Nothing in the language says `purserInterject` must be accompanied by a pressure
  increment; the component guarantees the words and guarantees nothing about the cost, so
  the tenth module that forgets the `::set` is silent. **And the second diagnostic is
  noise** — `E-UNDECLARED` on `run.shedPressure` fires because a component has no state
  schema in scope and never can, so it is a guaranteed companion error on every occurrence
  of the first, telling the author to declare a path they are forbidden to write.
  Suppressing state-path resolution once `E-COMPONENT-BODY` has fired on the same directive
  would leave a clean single message.

#### T6.5 — reuse with variation: nesting, param threading, and params inside `<match>` arms all work — WORKED WELL

- **Intent** — the things a real production reaches for second. The Purser speaks in every
  module at a different pressure; the mechanism is the param-scoped `<match>`. Push it:
  a component invoking another component, a param threaded through that invocation, and arm
  content that is itself parameterised.
- **Result** — all three work, first form reached for.
  - **Component invoking a component.** A component file accepts `components:` in its own
    frontmatter and `::use` in its body, and a param threads through the inner invocation:
    ```lute
    ---
    component: outer
    params: { pressure: string }
    components: [interject.component.lute]
    ---
    ## Outer
    ::use{component="interject" pressure=@pressure}
    ```
    `ok` standalone and through a scene, and the expansion compiles correctly. `lute trace`
    renders the nesting honestly — two `-- component begin --` markers, correctly nested.
  - **Parameterised arm content.** A `@param` in an attribute position inside a `<when>` arm
    is accepted: `::auto{character=@who anchor="port" action="brace"}` inside
    `<when is="rising">` checks clean. So arms are not second-class; the whole param surface
    is available inside them.
  - **The `<match>` fold is the well-behaved case of T5.7's machinery.** cryobank passes the
    literal `pressure="rising"`, so the `<otherwise>` arm is statically unreachable *for this
    caller*. All three tools do the right thing and agree: the artifact **prunes** it
    (`grep -c "Allocation is nominal"` over `commands[]` → `0`), `check-project` emits **no**
    dead-arm diagnostic (correct — another caller may pass something else), and `loc export`
    **does** carry it (correct — another caller may need it translated). That is precisely the
    combination T5.7 faults `W-CODE-AFTER-END` for getting backwards, reached here by the
    same reachability pass. Worth saying plainly: the fold is right.
- **Verdict** — worked well. Nothing this beat wanted from the variation surface was missing,
  and the nesting in particular is the thing most likely to be a stub in a young language.

#### T6.6 — a component param cannot have a default, and the long form every other schema surface uses is rejected — ERGONOMIC

- **Intent** — the Purser is nominal in most modules and rising in a few. So the natural
  declaration gives the common case a default and lets nine of eleven callers write
  `::use{component="purserInterject"}`.
- **Attempt** — `params: pressure: { type: string, default: "steady" }`, then a bare `::use`.
- **Result** — the accepted param grammar is exactly two forms, established by narrowing all
  five candidates through a caller:
  | form | result |
  |---|---|
  | `pressure: string` | accepted |
  | `pressure: { enum: [steady, rising] }` | accepted |
  | `pressure: { type: string }` | `E-COMPONENT-PARSE` |
  | `pressure: { type: string, default: "steady" }` | `E-COMPONENT-PARSE` |
  | `pressure: { enum: [steady, rising], default: steady }` | `E-COMPONENT-PARSE` |
  ```
  error [E-COMPONENT-PARSE] component file `/…/pf.component.lute` has a malformed `params:`
    — each entry must be `name: <type>` (dsl §13)
  ```
  Omitting an arg is clean and well-aimed: `error [E-COMPONENT-ARG] component `pf` requires
  argument `pressure` (dsl §13)`, at the `::use` line and column.
- **Resolution** — every caller spells every argument. For Anseo that is one arg on one
  callsite; for the eleven-module version of this component it is eleven copies of
  `pressure="nominal"`, which is the verbosity a component exists to remove.
- **Verdict** — `ERGONOMIC`, with two distinct costs. **No defaults** is the smaller one and
  arguably defensible — an explicit arg at every callsite is legible, and `E-COMPONENT-ARG`
  makes the omission a check-time error rather than a silent fallback. **The rejected
  `{ type: … }` is the sharper one**, because it is inconsistent rather than restrictive: the
  long form is how `state:` entries are written, how `defs:` entries are written
  (`warm: { type: bool, cel: … }`), and how the `anchor` domain is written in Anseo's own
  vocabulary — and `components-and-extends.md` says component params are "typed exactly like
  a [def param]". An author who has read the rest of the YAML surface writes `{ type: string }`
  and is told their `params:` is malformed. Accepting `{ type: X }` as a synonym for `X`
  would cost nothing and close it.

#### T6.7 — the component file's own check blames the reference for a fault in its own frontmatter, while the caller's check names the cause — TOOL-DEFECT

Split out of T6.6, where it was found, because it is a misdirection and the protocol ranks
those above the ergonomics of the thing that produced it.

- **Attempt** — the same malformed `params:` from T6.6, checked from both legs.
- **Result** — the two legs disagree about what is wrong, and the leg that owns the file
  gets it wrong:
  ```console
  $ lute check components/defaulted.component.lute --project /tmp/t6/proj
  …defaulted.component.lute:9:51: error [E-UNDECLARED-REF] `@pressure` is not a declared def (dsl §8.1)

  $ lute check scenes/probes.lute --project /tmp/t6/proj
  …probes.lute:1:1: error [E-COMPONENT-PARSE] component file `/…/defaulted.component.lute` has a
    malformed `params:` — each entry must be `name: <type>` (dsl §13)
  …probes.lute:1:1: error [E-UNDECLARED-REF] component `defaulted` (…): `@pressure` is not a
    declared def (dsl §8.1)
  ```
  The component's `params:` failed to parse, so the param was never registered, so the body's
  `@pressure` resolves against nothing. The caller reports **both** the cause and the
  consequence, in that order. The component's own check reports **only the consequence** — and
  reports it as `E-UNDECLARED-REF … not a declared def`, which sends the author to
  `defs:`/§8.1 for a param they declared four lines up and one character wrong.
- **Control, added during review at `AnseoT6Rev`'s request.** The body above puts the param in
  a `{{@pressure}}` interpolation, which T6.8 shows is independently illegal for a `string`,
  so the split could have been an artefact of that position. It is not. Same malformed
  `params:` with the param in an **attribute** position and no interpolation anywhere —
  `params: who: { type: string, default: "purser" }`, body `::auto{character=@who …}` —
  reproduces it exactly: standalone gives only
  `attrpos.component.lute:9:18: error [E-UNDECLARED-REF] `@who` is not a declared def`, and the
  caller gives `E-COMPONENT-PARSE` **and** the prefixed `E-UNDECLARED-REF`. The misdirection is
  a property of the leg, not of the ref position. Minimal single-component isolate:
  `/tmp/t6/t67`.
- **Resolution** — read the error from the caller, not from the file that contains it.
- **Verdict** — `TOOL-DEFECT`. The information exists — the same binary prints
  `E-COMPONENT-PARSE` for this exact file, from the other leg, one command later — and the
  tool nearest the fault does not hand it over. This is the T6.3 pattern in miniature and it
  compounds with it: the standalone leg is the only one that can point *into* a component
  file, and on the two faults measured here it either says `ok` (T6.3) or blames the wrong
  construct (this entry). `E-COMPONENT-PARSE` should fire on the standalone leg too, and
  should suppress the downstream `E-UNDECLARED-REF` it causes.

#### T6.8 — `{{@param}}` cannot render a `string`, the doc says it can, and the one built-in interpolation *is* a string — DOC-WRONG

- **Intent** — reuse with variation, in the place variation is most wanted: the words. The
  Purser names the module it is billing — "Draw exceeds projection in {{@module}}" — so one
  component carries eleven interjections instead of eleven near-duplicate blocks.
- **Attempt** — the form the components page documents, verbatim from its own worked example's
  param type:
  ```lute
  params:
    who: string
  ---
  @purser{code="0010" emotion="level"}: {{@who}}, the schedule advances.
  ```
- **Result** —
  ```
  …interp.component.lute:9:39: error [E-REF-TYPE] `@who` produces a non-renderable type;
    a `{{…}}` interpolation renders only number/bool/enum (dsl §7.6)
  ```
  Narrowed across all four param types, one component each: `number` `ok`, `bool` `ok`,
  `{ enum: [low, high] }` `ok`, **`string` errors**. So the only param type that carries
  arbitrary text is the only one that cannot be interpolated into text — and it is the type
  both shipped component examples declare (`greet`'s `who: string`, and this task's
  `pressure: string`).
- **Resolution** — `NONE — intent abandoned`. The component varies its words by
  `<match>`-ing on the param and writing each variant out in full, which is what
  `purser-interject.component.lute` ships. That works for two variants and does not scale to
  eleven module names; for those the words would go back to being authored per scene, i.e.
  reuse abandoned for exactly the beat that wanted it.
- **Verdict** — `DOC-WRONG`. `components-and-extends.md` states it flat and unqualified:
  *"A parameter is referenced as `@<param>` in ref and attribute positions, and inside content
  text via `{{@param}}` interpolation."* One sentence later it explains that `@who` is legal
  in the `character` position "only because that attribute is `string`-typed" — so the page
  is careful about type restrictions in the attribute position and silent about the one in
  the interpolation position, immediately above an example whose only param is a `string`.
  Nothing on the shipped website states the renderable-type rule anywhere: `grep` for
  "renderable" / "E-REF-TYPE" / "number/bool/enum" across `packages/website/src/content/docs`
  returns one hit, `params.md:77`, which says only that a whole-slot `@ref` must "produce the
  position's required type" and never names the interpolation whitelist.
  `dialogue-and-cast.md`'s interpolation section says an interpolation "must name a
  **declared** state path" and says nothing about type. So the author is told it works, at
  the type the example uses, and finds out from a diagnostic.

  **The doc fix is not the whole answer, and the rest is now filed separately.** The
  restriction this page fails to document is itself defective — the runtime renders strings,
  and the language forbids the only param type that carries one. That is a different verdict
  against a different artifact (the spec, not the page), so it is **T6.11**, filed as
  `SPEC-WRONG` at the controller's direction rather than hyphenated onto this entry. This
  entry's verdict stands alone: whatever §7.6 ought to say, `components-and-extends.md` states
  the opposite of what §7.6 says *today*, one line above an example that cannot compile.

#### T6.9 — provenance is carried on every surface a consumer reads, and the human renderings drop the name — WORKED WELL

- **Intent** — read cryobank back as a consumer, and as a translator. Can either tell which
  lines came from a component and which were authored inline?
- **Result** — yes, on all four surfaces, which is better than this log's usual finding about
  artifact fidelity (T4.8, T5.9):
  - **compiled artifact** — every expanded command carries `"source": {"component": "purserInterject"}`,
    and inline commands carry no `source` at all. Unambiguous, per command, machine-readable.
  - **`lute trace`** — `-- component begin --` / `-- component end --` around the expansion,
    correctly nested for a component invoking a component.
  - **`lute trace --json`** — `{"kind":"directive","tag":"__component-begin","component_boundary":"begin"}`,
    so a harness can bracket the region.
  - **`lute context --json`** — `components: [{"name":"purserInterject","params":[{"name":"pressure","type":"string"}]}]`.
    Name *and* param names *and* types. After T1.6 and T3.7 this is worth crediting
    explicitly: on components, `context` ships the grammar an author needs to write the
    `::use`, not just the identifier.
- **Verdict** — worked well. Three narrow gaps, none of which changes that:
  1. **The human renderings drop the name.** `lute context`'s outline prints
     `components (1): purserInterject` with no params — a harness reading the human form
     cannot write the `::use`; the `--json` form is complete. And `trace`'s
     `-- component begin --` carries no name, so a scene invoking three components previews
     three identical markers.
  2. **Nesting collapses to the innermost name.** `outer` → `interject` produces
     `"source":{"component":"interject"}` on every command, byte-identical to a direct
     invocation of `interject`. The chain is visible in `trace` (as depth) and lost in the
     artifact (as identity).
  3. **`__component-begin` is a leaked internal.** A double-underscore synthetic tag in the
     surface T5.9 establishes harnesses read; `component_boundary` beside it already carries
     the meaning.

#### T6.10 — every component line is silently dropped from the localization bundle, and the remedy the tool names is a no-op — TOOL-DEFECT

T6's most serious finding, and the one that would stop a real production from using
components at all. Adopting the language's only reuse mechanism *removed a line from the
localization pipeline*, and the before/after is one command.

- **Intent** — the translator question. The Purser says this in eleven modules; find out what
  `lute loc` hands a translator, and specifically whether they see the line once or eleven
  times.
- **Attempt** — the full round trip on the committed corpus: `loc export` → translate every
  row → `loc import` → `compile --locales`.
- **Result — the good half, and it is genuinely good.** `loc export` emits the component's
  lines **once**, keyed to the component file and its real line number, not once per caller:
  ```json
  { "code": "0020", "file": "docs/examples/anseo/components/purser-interject.component.lute",
    "kind": "line", "line": 21, "lineId": null, "speaker": "purser",
    "text": "Draw exceeds projection. The schedule advances." }
  ```
  `loc report` agrees and counts it once, as its own document — 2 lines / 9 words, `tagged 2`.
  A translator does *not* see the same line eleven times, and a producer does not pay for it
  eleven times. That is exactly right, and it is the strongest single argument for components
  in this whole task.
- **Result — `lineId` is `null`, and everything downstream is keyed on `lineId`.**
  ```console
  $ lute loc import /tmp/t6/ja-JP.json -o bundle.json
  3 rows skipped (no lineId) — run lute tag, then re-export
  exit=0

  $ lute tag docs/examples/anseo/components/purser-interject.component.lute
  lute: already tagged                        # exit 0, file unchanged (diff: no change)

  $ lute compile …/cryobank.lute --project docs/examples/anseo --locales bundle.json
  …cryobank.lute:1:1: warning [W-L10N-MISSING] no `ja-JP` text for `anseo.s01ep02.purser_0020`
  exit=0
  $ jq -r '.commands[]|select(.kind=="line")|"\(.lineId)\t\(.text)"' cryo-ja.json
  anseo.s01ep02.vesna_0010   [ja] Every pod you crack, the Purser reads as load.
  anseo.s01ep02.purser_0020  Draw exceeds projection. The schedule advances.   ← SOURCE LANGUAGE
  ```
  The chain: the row exports with no `lineId`; `import` skips it and exits **0**; `compile
  --locales` demands `anseo.s01ep02.purser_0020` — a **caller-derived** id that no export row
  ever carried — and ships the untranslated string at exit 0.

  **The named remedy cannot work.** `loc import --help` documents the bundle as keyed on
  `lineId` ("a duplicate `lineId` within one locale" is its error case), and `lute tag`
  "back-fills a stable `code` into every untagged `:line`". These lines are not untagged —
  they carry `code="0020"` / `code="0010"`, `loc report` counts them as `tagged 2`, and `lute
  tag` answers `already tagged` and changes nothing. Their `lineId` is null because
  `{prefix}` derives from `{character}.s{season}ep{episode}` in the *importing* document's
  frontmatter, which a component does not have and structurally cannot have. No amount of
  `lute tag` will ever produce one. The message sends the author to a command that is a
  guaranteed no-op, at exit 0.
- **Result — the before/after, which is the measurement.** The same Purser beat, one commit
  apart, moved from inline to `::use`:
  ```console
  $ lute loc export <HEAD, inline>  | jq -r '.[]|select(.speaker=="purser")|…'
  anseo.s01ep02.purser_0020   cryobank.lute      Allocation notes the draw. The schedule advances.
  anseo.s01ep11.purser_0010   shed.lute          Module released. Allocation is satisfied.

  $ lute loc export <after, ::use> | jq -r '.[]|select(.speaker=="purser")|…'
  NULL                        purser-interject.component.lute   Draw exceeds projection. …
  NULL                        purser-interject.component.lute   Allocation is nominal.
  anseo.s01ep11.purser_0010   shed.lute          Module released. Allocation is satisfied.
  ```
  Inline, the line was localizable. Through the language's reuse mechanism, it is not.
  Anseo's null-`lineId` count went 1 → 3 on this task, and the one pre-existing null is a
  genuinely untagged quest line that `lute tag` *can* fix — so the message is correct for one
  of three rows and impossible for the other two.
- **Resolution** — none available, and the corpus ships the defect. `check-project` is `ok`
  at 6 files; `compile --locales` is the only command that says anything, at warning severity,
  naming an id the author will not find in any export they were given. **Kept as written**,
  because the alternative is to abandon components for any translated line, which is the
  finding.
- **Verdict** — `TOOL-DEFECT`, and deliberately not `LANGUAGE-GAP`. The *language* is right:
  a component line's identity is caller-derived, which is the correct semantics — one source
  of words, eleven addressable lines. Every failure is in a tool, and the information all
  three need is present in a single `check-project`/`compile` run: `loc export` knows the
  callers (it is a project-wide walk), `compile` knows all eleven `lineId`s, and `loc import`
  knows the row's `file` is a component. Three concrete fixes, any one of which unblocks a
  translated production: (i) `loc export` emits one row **per expansion**, carrying the
  caller-derived `lineId`, with the component file+line retained as a `source` field so a TMS
  dedupes on identical source text and a translator still sees the string once — this is the
  right fix, because it also makes `W-L10N-MISSING` unreachable; (ii) failing that, the bundle
  accepts a component-scoped key (`purserInterject#0020`) that `compile --locales` resolves
  before falling back to `lineId`; (iii) at minimum, `loc import` must not name `lute tag`
  for a row whose file declares `component:`, and skipping a translated row should not be
  exit 0 — `loc export`'s own "1 lines untagged — run lute tag" precedent shows the surface
  for saying so honestly.

#### T6.11 — the interpolation whitelist forbids the one param type that carries text, while the runtime renders text on the other two interpolation forms — SPEC-WRONG

Escalated out of T6.8 at the controller's direction. T6.8 is the *page* being wrong about
what §7.6 says; this is §7.6 itself being the wrong rule. Filed against the language because
the drive test's remit **is** the language.

- **Intent** — the same beat T6.8 wanted: one component whose words vary by an author-supplied
  string — "Draw exceeds projection in {{@module}}" — so eleven modules share one block
  instead of eleven near-duplicates. Then, separately: establish whether the restriction that
  blocks it is a coherent rule or an accident of which grammar alternative you land in.
- **Attempt** — three probes in `/tmp/t6fix/proj`, all against the freshly built binary.
  (a) One component per param type, each body a single line reading `{{@who}}, the schedule
  advances.`; (b) the same line with the reserved token `{{userName}}` instead; (c) the same
  line reading a `string`-typed **declared state path**, `{{run.label}}`, declared
  `run.label: { type: string, default: "nominal" }`.
- **Result** — the whitelist binds exactly one of the three interpolation forms, and it is the
  one a component param uses.
  ```console
  $ lute check components/ip-number.component.lute        # ok         (params: who: number)
  $ lute check components/ip-bool.component.lute          # ok         (params: who: bool)
  $ lute check components/ip-enumlowhigh.component.lute   # ok         (params: who: { enum: [low, high] })
  $ lute check components/ip-string.component.lute        # exit 1     (params: who: string)
  components/ip-string.component.lute:9:39: error [E-REF-TYPE] `@who` produces a
    non-renderable type; a `{{…}}` interpolation renders only number/bool/enum (dsl §7.6)
  ```
  The reserved token — a string — checks clean and compiles to a placeholder the runtime
  substitutes:
  ```console
  $ lute check components/ip-user.component.lute          # ok, exit 0
  $ jq -c '.commands[]' outUser.json
  {"kind":"line",…,"text":"{{userName}}, the schedule advances.",…,
   "placeholders":[{"kind":"reserved","token":"userName"}],"source":{"component":"ipuser"}}
  ```
  And so does a `string`-typed declared state path, which is the probe that settles it:
  ```console
  $ lute check scenes/strpath.lute --project .            # ok, exit 0
  $ jq -c '.commands[]' outStr.json
  {"kind":"line",…,"text":"Draw stands at {{run.label}}.",…,
   "placeholders":[{"kind":"path","path":"run.label"}]}
  ```
  So: `{{userName}}` renders a string. `{{run.label}}` renders a string. `{{@who}}` is a
  static error *because* it is a string. Three forms of one construct, one shared runtime
  substitution mechanism (`docs/runtime/state-lifecycle.md`: the `placeholders` list names each
  referent — "a state `path`, an `@`-`ref`, or a `reserved` token" — and "the engine substitutes
  these against live state"), and the type rule applies to exactly one of them.
- **Resolution** — `NONE — intent abandoned`, same as T6.8: the shipped component varies its
  words by writing each variant out in full under a `<match>`, which does not scale past a
  handful and does not reach eleven module names at all.
- **Verdict** — `SPEC-WRONG`. Nothing here is misimplemented and, unlike T6.8, nothing here is
  a contradiction on the page either — that distinction matters and I had it wrong first time.

  **What the spec says.** `docs/proposals/scenario-dsl/0.1.0.md` §7.6 gives three grammar
  alternatives — `Interp ::= "{{" ( Path | Ref | ReservedToken ) "}}"` — and attaches the type
  rule to precisely one: *"An interpolated `Ref` MUST resolve to a renderable type (number /
  bool / enum, per the rendering rule below); a `@def` of any other type is a static error."*
  `ReservedToken` gets its own bullet with no type rule at all (*"`userName` renders the
  runtime player name"*), and the normative **Rendering** paragraph enumerates only
  number/bool/enum. So `{{userName}}` is not a violation of the whitelist; it is *outside* it,
  by explicit construction. **This is not a literal grammar contradiction** and should not be
  reported as one. The `Path` case is looser still — §7.6's rendering paragraph never defines
  how a string path renders, and the checker admits it silently.

  **Why that is the wrong call.** The defect is a **capability mismatch**, not an
  inconsistency. The rendering pipeline demonstrably renders arbitrary strings: it does not
  interpolate at compile time at all, it emits a `placeholders` list and keeps the raw text,
  and the engine substitutes at present time. Nothing about a `string` is unrenderable — two
  of the three interpolation forms ship strings today. What §7.6 actually does is forbid the
  **only param type that can carry arbitrary author text** from reaching the one position that
  displays text, in the language's **only** content-reuse construct. The cost is not a
  workaround; it is that varying a component's words by an argument is unreachable, which
  removes the main reason to write a component with a param at all — and the two shipped
  component examples both declare `string` params (`greet`'s `who`, this task's `pressure`),
  so the spec forbids the shape its own documentation models. If the rule's motive is
  localization safety — splicing untranslated author text into a `lineId`-keyed line is a real
  hazard, and §7.6's `E-L10N-PLACEHOLDER` placeholder-set contract is the surface that would
  police it — then the rule as written does not achieve it, because `{{run.label}}` splices an
  arbitrary string into a translatable line at exit 0 and `{{userName}}` is taught on
  getting-started page one.

  **What it should say instead.** Replace the produced-type whitelist with a
  substitution-mechanism rule, which is what the implementation already is:

  1. **Admit `string` as a renderable type** and extend the normative Rendering paragraph to
     cover it (*"a **string** → its text verbatim"*), making the sentence true of all three
     forms rather than one. `E-REF-TYPE` then keeps its real job — a `@def` producing a
     *structural* type (map/list) is still a static error, which is the case the rule was
     presumably written for.
  2. **Keep the safety story explicit** by scoping it where it belongs: every interpolation,
     of any form, is already a placeholder in the line's translatable text, and
     `E-L10N-PLACEHOLDER` already enforces placeholder-set equality across translations. If a
     project wants to forbid interpolating unbounded text into translatable lines, that is a
     lint over *all* placeholder kinds (`W-INTERP-FREE-TEXT`, say, which would fire on
     `{{run.label}}` too), not a type ban that one grammar alternative happens to escape.
  3. **Failing both**, if the ban is deliberate and permanent, then say so *as a rule about
     component params* and make the diagnostic say it — `E-REF-TYPE`'s current text
     ("a `{{…}}` interpolation renders only number/bool/enum") is a claim about interpolation
     that the same binary falsifies twice in the transcript above.

#### T6 summary

Eleven entries: four *worked well* (T6.1, T6.4, T6.5, T6.9), three `TOOL-DEFECT` (T6.3, T6.7,
T6.10), two `SPEC-WRONG` (T6.2, T6.11), one `DOC-WRONG` (T6.8), one `ERGONOMIC` (T6.6). Every
entry carries exactly one verdict and no hybrids; the four *worked well* entries are the
protocol's "what worked well" register rather than a verdict, as in T1–T5. No `LANGUAGE-GAP`
and no `DOC-GAP`: everything this component wanted to *be* was expressible, and nothing
required opening Rust, a proposal, or a test — T6.2's limitation has its own headed section on
the shipped website, and T6.8's is the doc being wrong rather than silent. T6.11 was
originally deferred inside T6.8 as an escalation; the controller ruled the language is in
remit, and it is now filed as its own entry.

**The construct is good and the surrounding toolchain is not ready for it.** That split is
sharper here than anywhere else in this log. The body contract is the best-explained
restriction in six tasks — three distinct codes, each carrying its rationale, each enforced
on both legs (T6.1) — and it was *repaired two hours before this task started*, which is
the only reason the brief's Step 3 probe is a pass rather than a false green. The variation
surface is complete on first reach: nesting, param threading, params inside `<match>` arms,
and a `<match>` fold whose three tools agree (T6.5). Provenance is carried on all four
consumer surfaces (T6.9). And the one restriction that blocked a natural beat — no `::set` in
a body — is, on inspection, the right call for a reason the rationale does not even claim:
not purity, but that the number this whole prologue is about would stop being auditable by
reading if eleven files could charge it invisibly (T6.4).

**Then the findings that decide the maturity question.** T6.2: a component's own `uses:`
is discarded, so it has no denotation of its own — two callers took one body and produced
two-command and three-command streams with opposite staging semantics, at exit 0, with no
diagnostic. The docs describe this as "a scoping limit, not a checking divergence"; that
sentence is true about checking and false about meaning, and it is why the verdict is
`SPEC-WRONG` rather than a doc finding. T6.10: adopting the reuse mechanism took a line
*out* of the localization pipeline — `loc export` correctly emits it once, with
`lineId: null`; `loc import` skips it at exit 0 and prescribes `lute tag`, which answers
`already tagged`; `compile --locales` then warns about a caller-derived id that appears in no
export the translator ever saw, and ships English. Verified end to end, with a one-commit
before/after.

**The pattern this log has been accumulating shows up here as a matched pair, and that is the
finding worth carrying forward.** T6.3: the standalone leg can point *into* a component file
and cannot detect a caller-relative fault (it reports `ok`); the caller leg detects every
such fault and cannot point into the file (it reports `1:1`, N times, in the N files that are
correct, while the one file to edit reports `ok`). T6.7 is the same pair on a different fault
— the component's own check blames `@pressure` for a malformed `params:` that the caller's
check names outright. Neither is a missing capability; both are information the same binary
prints from the other leg, one command apart. To be exact about the cost, because the first
draft of this section overstated it: a component **can** be validated — `check-project`
detects the caller-relative fault and exits 1. **What no command offers is a check that is
both caller-context-aware and able to point at the component's own source span**, so the
author is handed N identical `1:1` reports in the N files that are correct while the one file
to edit reports `ok`. Until `lute check <component>` either forwards the caller-side span or
refuses to claim `ok`, the honest instruction to an author is: never check a component file,
only ever `check-project`, and read the path out of the message prefix.

**What a production would need before shipping a translated work with components.** In
order: T6.10 (i) — `loc export` per expansion with caller-derived `lineId`s, without which
components and localization are mutually exclusive; T6.2 (ii) —
`W-COMPONENT-VOCAB-DIVERGENT`, which is nearly free given what `check-project` already
resolves; T6.11 — admit `string` as a renderable type, without which a param cannot vary a
component's words and the construct's headline feature is decorative; T6.3 — forward the
component-internal span and roll up the N caller reports. The first is a blocker. The rest are
the difference between a construct you can use and one you can trust.

---

### T7 — The branch scenes

Toolchain 0.9.0 / language 0.9.0 / IR 0.9.0, `./target/debug/lute`, rebuilt before the
first probe. Four scenes added: `scenes/spine-a.lute` (`anseo.s01ep03`, the first shed on
screen and the corpus's second `<timeline>` ever), `scenes/hydroponics.lute`
(`anseo.s01ep04`), `scenes/machine-deck.lute` (`anseo.s01ep05`), `scenes/stowaway.lute`
(`anseo.s01ep06`).

This is the first task whose deliverable is **dialogue** rather than a construct. T1–T6 each
drove one mechanism to its edge with as little prose as the mechanism needed. T7 writes four
scenes of a real story and measures what four scenes cost — which is a different question,
and it surfaces a different class of finding: the substitution you make three times without
noticing, and the line you retype in every file.

#### T7.1 — the Purser cannot interrupt anyone; a `<track>` may not hold a line, and a line may not hold a time — LANGUAGE-GAP

- **Intent** — written before any Lute was typed. The Purser is a ship voice reading a
  schedule aloud. Its first appearance on the spine should be cut off *by the thing it is
  announcing*: it begins "Module four is scheduled for release in—" and the module goes
  before the number does. The joke and the horror are both in the overlap. Separately, and
  more ordinarily: Vesna talking over the klaxon rather than after it.
- **Attempt** — the beat is choreography, so the first form reached for put the line inside
  the timeline that carries the choreography, at an offset:
  ```lute
  <track subject="purser" property="voice">
    @purser{code="0050" emotion="level" os at="0.2"}: Released.
  </track>
  ```
- **Result** —
  ```
  spine-a.lute:36:5: error [E-TIMELINE-CONTENT] a <track> body may contain only staging
  directives and ::set
  ```
  Correct, documented, and reasoned: `language/timeline-and-property-tracks.md` says tracks
  are "staging-only and non-interactive … No dialogue, prose, `<choice>`, `<branch>`, or
  `<match>` may appear inside — those would make the beat reader-paced rather than
  clock-paced." That rationale is sound and I would not remove the restriction.
- **Second attempt — put the time on the line instead of the line on the clock.** If a
  content line carried any of the §7.5 timing attrs, two lines could be made to overlap
  without a timeline at all. All three, probed one at a time in the project:
  ```
  10:36: error [E-UNKNOWN-ATTR] unknown content-line attribute `at` (dsl 0.1.0 §7.1)
  10:36: error [E-UNKNOWN-ATTR] unknown content-line attribute `delay` (dsl 0.1.0 §7.1)
  10:36: error [E-UNKNOWN-ATTR] unknown content-line attribute `wait` (dsl 0.1.0 §7.1)
  ```
  A content line has no temporal surface of any kind. `@vesna,toma{…}: Together.` — one line,
  two speakers, the cheapest possible spelling of simultaneity — is
  `E-UNCLASSIFIED: content line needs a second ':' before its text`, i.e. the comma is read as
  part of the speaker name.
- **Resolution — the story changed.** The interruption is now **punctuation and sequence**:
  the Purser's line ends in an em-dash, the timeline follows it, and a reader supplies the
  overlap. Vesna-over-the-klaxon became Vesna-before-the-klaxon. Both work as prose. Neither
  is what I wrote down first, and nothing in the source, the artifact, or `lute trace` records
  that an overlap was ever intended — the em-dash is invisible to every tool.
- **Verdict** — `LANGUAGE-GAP`, shape (a): I changed the story to fit the tool. Recorded
  without a complaint attached to the timeline restriction, which is right, and without one
  attached to `E-UNKNOWN-ATTR`, which is accurate and cites its clause. The gap is that
  between "reader-paced dialogue" and "clock-paced staging" the language has no third thing,
  and overlapping speech is the most common beat in any recorded medium that is neither. Two
  characters speaking at once is not an exotic want; it is how arguments, alarms and ship
  voices work. A minimal fix that does not disturb the timeline's rationale: admit content
  lines in a `<track>` but require them to carry an explicit `at`, so the beat stays
  clock-paced and the "reader-paced" objection does not arise. Failing that, a `{over}`
  delivery flag beside `{mono}`/`{os}`/`{vo}` — "this line begins before the previous one
  ends" — would at least let the source say what the em-dash is doing.

#### T7.2 — two clips that touch are rejected as an overlap, because the track cursor's float sum overshoots the boundary the author wrote — TOOL-DEFECT

- **Intent** — the shed effect plays, and the pressure drop begins the moment it finishes.
  One channel, two clips, hand-off at the boundary. `::vfx{type="shed" at="0.8" duration="0.4"}`
  ends at `1.2`, so the next clip goes `at="1.2"`.
- **Attempt** —
  ```lute
  <track channel="vfx">
    ::vfx{type="shed" label="module four" at="0.8" duration="0.4"}
    ::vfx{type="pressure-drop" transition="cut" at="1.2"}
  </track>
  ```
- **Result** —
  ```
  spine-a.lute:38:5: error [E-CLIP-OVERLAP] clip at 1.2 overlaps another clip in track `#vfx`
  ```
  Both normative sources make this hand-off legal, and they agree.
  `docs/proposals/scenario-dsl/0.1.0.md` §11.4:

  > - a clip's **resolved end** = its start + `duration` (or its start, if it carries no
  >   `duration`).
  > - clips within one track MUST NOT overlap: a clip whose start precedes the previous clip's
  >   resolved end is **`E-CLIP-OVERLAP`**.

  `1.2` does not *precede* `1.2`. `docs/runtime/timeline-semantics.md` states the same rule in
  interval form — "two clips in the **same** track whose `[at, at+duration)` **half-open**
  intervals overlap" — and `[0.8, 1.2)` and `[1.2, …)` are disjoint. The clip is legal under
  both spellings of the rule and is rejected.
- **Probed until the rule was exact.** One clip pair per run, each checked on its own, and the
  last four rows added on the re-check that produced the retraction below:

  | clips in one track | result | matches §11.4? |
  |---|---|---|
  | `at=0.8 duration=0.4` + `at=1.2 duration=0.1` | **E-CLIP-OVERLAP** | **no — this is the defect** |
  | `at=0.8 duration=0.4` + `at=1.19 duration=0.1` | **E-CLIP-OVERLAP** | yes |
  | `at=0.8 duration=0.4` + `at=1.2000001 duration=0.1` | `ok` | yes |
  | `at=0.8 duration=0.4` + `at=1.3 duration=0.1` | `ok` | yes |
  | `at=0.8 duration=0.4` + `at=0.8` *(no duration)* | `ok` | **yes** |
  | `at=0.8` + `at=0.8` *(neither has a duration)* | `ok` | **yes** |
  | `at=0.8 duration=0.4` + `at=0.8 duration=0.1` | **E-CLIP-OVERLAP** | yes |
  | `at=0.8 duration=0.4` + `at=0.8 duration=0.4` | **E-CLIP-OVERLAP** | yes |
  | `at=0.8` *(no duration)* + `at=0.8 duration=0.1` | `ok` | yes |
  | `at=0.8 duration=0.4` + `at=0.9` *(no duration)* | **E-CLIP-OVERLAP** | yes |

- **RETRACTION — the second half of this entry, as first written, was wrong.** The original entry
  claimed `E-CLIP-OVERLAP` is a "wrong answer in both directions": restrictive at the boundary
  **and** permissive for "two clips at the same instant on one track", which it called "the worse
  half" and "the only thing the invariant exists to prevent". It also claimed "a clip with no
  resolvable `duration` is exempt from the analysis entirely, in both directions". Neither claim
  survives, and the error was mine, not the checker's: **both accepted same-instant probes omit
  `duration`.** §11.4 defines a clip's resolved end as "its start, if it carries no `duration`",
  so each of those clips resolves to the degenerate interval `[0.8, 0.8)` — a span that contains
  no instant and therefore cannot be simultaneous with anything, including itself. The rule
  raises `E-CLIP-OVERLAP` only when a start *precedes* a previous resolved end, and `0.8` does
  not precede `0.8`. `timeline-semantics.md` also specifies what happens to genuinely
  same-instant records — clips are emitted in "`(at, track index)` order … stable on ties so
  same-`(at, track)` clips keep document order" — so a same-instant pair is ordered, not raced.
  Accepting those two probes is **correct behaviour**, and the zero-duration probes did not test
  what I read them as testing: they measured the rule's treatment of empty intervals, not its
  treatment of simultaneity.

  Re-probed with `duration` present on both clips (rows 7–8 above), the checker **rejects**
  same-instant clips exactly as it should. The exemption claim is refuted too: row 10 is a
  no-`duration` clip *inside* a preceding clip's span and it is correctly rejected, so a clip
  with no `duration` participates fully in the analysis as a zero-width point. **The checker is
  right in the permissive direction. The defect is not bidirectional; there is one defect here,
  at the boundary.**
- **The mechanism is float accumulation, not the comparison operator** — which is the other
  thing the original entry got wrong. It concluded "the check requires
  `next.at > prev.at + prev.duration` **strictly**". It does not. `timeline.rs` tests
  `if at < o_end && o_at < end` — a textbook half-open intersection, symmetric, against every
  earlier clip in the track. Written out, no-error requires `at >= prev.at + prev.duration`,
  which is *non*-strict and is precisely what §11.4 asks for. The comparison is already correct.
  What is wrong is the right-hand side: `0.8 + 0.4` in IEEE-754 binary is
  `1.2000000000000002`, so `1.2 < 1.2000000000000002` holds and a hand-off the author spelled
  exactly on the boundary lands one ULP inside the previous clip.

  Confirmed by choosing boundaries that are exactly representable in binary, where the same
  hand-off shape passes:

  | hand-off | `at + duration` computes to | result |
  |---|---|---|
  | `at=0.5 duration=0.25` + `at=0.75` | `0.75` (exact) | `ok` |
  | `at=1.0 duration=0.5` + `at=1.5` | `1.5` (exact) | `ok` |
  | `at=0.25 duration=0.5` + `at=0.75` | `0.75` (exact) | `ok` |
  | `at=0.8 duration=0.4` + `at=1.2` | `1.2000000000000002` | **E-CLIP-OVERLAP** |
  | `at=0.1 duration=0.2` + `at=0.3` | `0.30000000000000004` | **E-CLIP-OVERLAP** |
  | `at=0.2 duration=0.4` + `at=0.6` | `0.6000000000000001` | **E-CLIP-OVERLAP** |

  So boundary hand-off is not broken in general — it is broken for the boundaries authors
  actually type. `0.75` and `1.5` work; every tenth that is not a negative power of two does
  not. That is worse than a uniformly broken rule, because the failure looks arbitrary from the
  author's chair: the same construct, same shape, works at `0.5 + 0.25` and fails at
  `0.8 + 0.4`, with a diagnostic that prints `clip at 1.2` and names no discrepancy.
- **The float leak is also visible in an author-facing message, and it is the same bug.**
  `E-TIMELINE-DURATION` (correctly raised when I set `<timeline duration="0.5">` over a clip
  ending at 1.2) renders the bound as:
  ```
  error [E-TIMELINE-DURATION] timeline duration 0.5 is below the max resolved clip end
  1.2000000000000002; a timeline may not truncate its own content (dsl §11.4)
  ```
  The original entry filed this "in passing" as a cosmetic complaint about float accumulation
  leaking into a message. It is not cosmetic: it is the same `0.8 + 0.4` the overlap check
  compares against, printed. The evidence for the root cause was in the entry from the start
  and I read it as a separate, smaller finding.
- **Resolution** — the shed's `duration` is `0.35` in the committed scene. I did not want
  `0.35`; I wanted `0.4` and a hand-off. The number in the file is an artifact of the
  diagnostic, and this is the small silent kind of substitution T7 exists to count: nobody
  reading `spine-a.lute` will ever know why the shed is 350ms.
- **Verdict** — `TOOL-DEFECT`, and it stands on the boundary case alone. The language is fine,
  the docs are fine and *specify the correct rule* in two places that agree, the diagnostic's
  span and wording are good, and the interval logic is right. One float comparison against an
  accumulated sum contradicts the written contract in the restrictive direction, for the case
  authors write most: boundary hand-off is the normal way to sequence two effects on one
  channel. An author who hits it learns to sprinkle epsilons — `1.2000001` passes — and carries
  that superstition into every timeline afterwards.

  The fix is **not** a comparison operator, which is what this entry originally proposed. It is
  either an epsilon tolerance on the interval test (compare `at + EPS < o_end`, with `EPS` well
  below any authorable resolution) or, better, quantising timeline time to integer milliseconds
  at parse and doing all cursor arithmetic in that domain — which removes the class rather than
  the instance, and incidentally stops `E-TIMELINE-DURATION` printing
  `1.2000000000000002` at an author.

#### T7.3 — a timeline's tracks do not exist in the artifact, and one engine option in the runtime contract cannot be implemented — SPEC-WRONG

- **Intent** — verify, after the fact, that the six-track beat I wrote actually reached the
  compiled artifact as a six-track beat.
- **Attempt** — `lute compile scenes/spine-a.lute --project docs/examples/anseo`, then the
  union of keys over every record carrying a `timeline` stamp.
- **Result** — the scheduling is exactly right and the tracks are gone:
  ```
  action, addr, anchor, at, character, duration, focus, kind, label, mood, shake, sound,
  timeline, transition, vfxType, volume, wait, zoom
  ```
  Eight clips, `(at, track index)`-sorted, barrier at the explicit `duration`:
  ```
  sfx    001-0700  at 0.0   dur 0.6
  camera 001-0800  at 0.0   dur 0.3
  camera 001-0900  at 0.35  dur 0.5
  sprite 001-1000  at 0.4
  music  001-1100  at 0.5
  vfx    001-1200  at 0.8   dur 0.35
  sprite 001-1300  at 0.9
  vfx    001-1400  at 1.2
  barrier 001-1500 at 1.6
  ```
  There is no `track`, `subject`, `channel`, or `property` field on any of them. The two
  clips I deliberately split across two property tracks on one subject —
  `subject="vesna" property="pos"` and `subject="vesna" property="grip"` — arrive as:
  ```json
  {"kind":"sprite","addr":"001-1000","character":"vesna","anchor":"port","action":"brace","at":0.4,"timeline":1}
  {"kind":"sprite","addr":"001-1300","character":"vesna","action":"seal","at":0.9,"timeline":1}
  ```
  distinguishable only by the `action` an author happened to write.
- **Why this is a defect and not just terseness.** `docs/runtime/timeline-semantics.md`
  closes with an **Engine contract** giving "two sound options", and says the write-conflict
  guarantee "makes both equivalent in observable state":
  1. replay the pre-scheduled `(at, track)` order;
  2. **"Run tracks concurrently.** Drive each track's clips on the local clock in parallel."

  Option 2 is not implementable from the artifact. An engine cannot drive per-track clips in
  parallel when nothing in the stream says which clips are a track. Nor can it do the natural
  implementation of a track — one tween slot per track key, each new clip superseding the
  previous one on that key — which is the entire reason `language/timeline-and-property-tracks.md`
  gives for property tracks existing: "Two `subject="camera"` tracks would silently fight."
  The checker proves they do not fight, then deletes the evidence of which writer is which,
  so an engine receiving two `sprite` records for `vesna` must either re-derive the split or
  treat them as one writer — the exact fight the feature prevents.
- **The spec is internally consistent about it, which is why the verdict is `SPEC-WRONG`.**
  The same page's "What the IR carries" section lists the `Stamp` fields — `timeline`, `at`,
  `duration`, `delay` — and `track` is correctly absent from that list. *On the question of the
  missing `track` field* nothing is undocumented and no tool is lying: the compiler emits
  precisely what the IR schema specifies. Two clauses of one document disagree, and the one
  that governs the wire format won.

  Scoping that sentence tightly, because as first written it was broader than the evidence and
  false: I cited that four-item list approvingly without checking the content of its items, and
  the `timeline` bullet in the very list I was praising *is* wrong about the field it describes.
  See T7.16.
- **Proposed alternative** — add `track` (the resolved track key string, exactly the one
  `E-DUP-TRACK` and `E-CLIP-OVERLAP` already compute and print — `#vfx`, `vesna.pos`,
  `camera`) to `Stamp`, beside `timeline`. It is one field, it is already in hand at
  schedule time, and without it the second engine option and the property-track feature are
  both decorative.

#### T7.4 — the timeline is choreography, and the instinct to make it the clock is worth recording — WORKED WELL, with a note

- **What worked, and it is the cleanest first-use since T5.1.** Six tracks written in one
  pass, three keyed by `channel=`, one by `subject=`, two as property tracks on a single
  subject, with deliberately overlapping cross-track offsets (`camera` 0.35–0.85 straddles
  `vesna.pos` at 0.4). It checked clean on the second attempt; of the two errors on the first
  attempt one was real (`E-TIMELINE-CONTENT`, T7.1) and one was the false positive filed as
  T7.2 — corrected here, because this bullet originally called both of them real and T7.15's
  own accounting says otherwise. The cursor math is right *as specified* (an omitted `at` on the
  track's first clip starts at 0.0; `::sfx` with `duration="0.6"` and no `at` lands at 0.0) —
  T7.2 is a defect in how that arithmetic is *carried*, in binary floating point, not in the
  rule it implements. The sort is right,
  the barrier lands on the explicit `duration` (1.6) rather than the content maximum (1.2),
  and `E-AT-CONTEXT` — *"`at` is valid only on a `<track>` clip; `::vfx` here is not a
  timeline clip (dsl §7.5)"* — is the most precisely-scoped diagnostic I hit in this task.
- **`property=` is free-form and nothing checks it.** `property="grip"` is accepted with no
  diagnostic, exactly as `property="pos"` is. That is defensible — the property namespace is
  engine policy, not language vocabulary — but it is worth stating beside T7.3, because the
  string an author invents to separate two writers is neither validated nor transmitted.
- **The note: I wanted the timeline to be the countdown, and it must not be.** The shed
  schedule is the spine of this whole prologue, and the first thing a `<timeline duration>`
  looks like is a countdown to it. It is not, and the design is right that it is not: the
  timeline's clock is local, sub-second, and non-interactive, while the shed clock is
  `run.shedPressure`, advanced by `::set` in choice arms across four documents and readable by
  a guard. Fusing them would put a player-paced quantity on a frame-paced clock. Recorded
  because the instinct was strong enough that I typed `<timeline duration="1.2">` around the
  wrong beat once before catching it, and because the brief predicted it — an author without
  that warning has one construct spelled `duration` and one spelled `+= 1`, and only the docs
  distinguish them. `timeline-and-property-tracks.md` does, in its first sentence
  ("bounded, non-interactive choreography unit"). It is doing real work there.

#### T7.5 — `lute context`'s per-directive attribute lists omit the timing attrs, and the checker accepts them — recurrence of T1.6/T3.7

- **Intent** — write `::sfx{sound="klaxon-two-tone" duration="0.6"}` and check the attribute
  is legal before compiling.
- **Attempt** — `lute context docs/examples/anseo/scenes/spine-a.lute`, which prints
  `sfx: sound, assetId, name`. No `duration`, no `at`, no `delay`, no `wait`. `::vfx` and
  `::music` likewise list only their own attrs. (`camera` and `video` *do* list some — `camera`
  shows `duration, easing, delay, wait`, `video` shows `wait` — which makes the omission look
  like a real per-directive difference rather than a uniform one.)
- **Result** — the checker accepts `duration` on `::sfx`, `::vfx` and `::music`, inside a
  timeline and outside one (`::sfx{sound="hum" duration="0.6"}` alone in a shot: `ok`). The
  §7.5 timing attrs are cross-cutting and apply to every staging directive; `context`
  presents them as belonging to two directives out of nine.
- **Resolution** — none needed; I wrote them anyway and they work.
- **Verdict** — folded into T1.6/T3.7 rather than filed as a new one, but recorded as a
  **recurrence**, because it is the same failure a third time and in the same list: the
  `directives (9)` block asserts a per-directive attribute surface, and it is neither complete
  per directive (here) nor complete in its directive set (T3.7). Two of the three
  `duration`-bearing clips in `spine-a`'s timeline are attributes `context` says do not exist.

#### T7.6 — a relation may guard a line but may not open a block; the block form needs a subject the guard never reads — ERGONOMIC

- **Intent** — `machine-deck`'s whole scene turns on one condition: is Toma awake. If he is, he
  talks you through seating the coupling and the schedule holds; if he is not, you fail and
  `run.shedPressure += 1`. That is a two-arm conditional over a *block* of eight lines and two
  staging directives, not over one line.
- **Attempt** — the obvious form, and the one the language's own state-dispatch construct
  suggests:
  ```lute
  <match on="holds(awake(toma))">
  <when test="$">
  ```
- **Result** —
  ```
  machine-deck.lute:20:12: error [E-MATCH-RELATION-SUBJECT] relations are guard-only; a
  `<match on>` subject must stay enum/bool/scalar so exhaustiveness stays decidable (dsl 0.3.0 §8)
  ```
  A clear diagnostic with a stated rationale — and the rationale does not survive inspection.
  `holds(...)` is bool-valued; bool is the canonical finite domain the exhaustiveness section of
  `branch-match-when.md` uses as its example ("a bool covered by `is="true"`/`is="false"` needs
  none either"). Exhaustiveness over a relational subject is *more* decidable than over the
  `number` subject the same construct accepts without comment. Whatever the real reason is —
  presumably that a Datalog query is not a scalar read and the `$`-substitution machinery does
  not want it — decidability is not it.
- **Probed for the form that does work.** Four variants:

  | form | result |
  |---|---|
  | `<when test="holds(awake(toma))">` with no enclosing `<match>` | `E-UNCLASSIFIED: unexpected block here` |
  | `<match on="holds(awake(toma))"><when test="$">` | `E-MATCH-RELATION-SUBJECT` |
  | `<match on="run.shedPressure"><when test="holds(awake(toma))">` | `ok` |
  | `<match on="true"><when test="holds(awake(toma))">` | `ok` |

  So a relation is admitted in `test=` and refused in `on=`. There is no `<if>`, and a bare
  `<when>` is not a block. The only way to gate a block on a relation is to open a `<match>` on
  a subject the guard never uses.
- **Resolution** — the committed scene carries `<match on="true">` and a six-line comment
  explaining why, because without one the next reader will "fix" it. `<otherwise>` is then
  mandatory (`E-NONEXHAUSTIVE` if omitted), which here is harmless — I wanted the else arm — but
  means a *one*-armed relational block costs a dummy subject and an empty `<otherwise>` too.
- **Verdict** — `ERGONOMIC`. The intent is fully expressible and nothing is mirrored, lost, or
  unchecked; `lute trace` and `lute run` both drive the two arms correctly (transcripts below in
  T7.15). What an author writes is `on="true"` — a subject that is a lie about what the block
  dispatches on — in a language whose other constructs are unusually honest about their own
  meaning. Admitting a bool-valued relational query as a `<match on>` subject, with
  `<otherwise>` required exactly as it is for a `test`-guarded arm today, would close this with
  no loss of decidability.

#### T7.7 — the content-line `when=` is documented as "exact sugar" for a `<match>` that does not compile, and `lute trace` prints the illegal form back at you — DOC-WRONG

- **Intent** — n/a authorially; found while writing T7.6's entry, because the page that refused
  my `<match>` is the page that told me the two forms were the same thing.
- **Attempt** — `language/branch-match-when.md`, §"The `when=` content-line sugar":

  > This is **exact sugar for a one-arm match**, and lowers to that record identically… Written
  > out it is the explicit twin below

  followed by a worked `@sofia{when="run.metHelpfully"}` / `<match on="run.metHelpfully">` pair,
  and the assurance that the example file "keeps both forms, one shot each, so they stay visibly
  interchangeable."
- **Result** — they are not interchangeable. In the same project, in the same document:
  ```lute
  @toma{code="0030" emotion="frayed" os when="holds(awake(toma))"}: …    # ok
  <match on="holds(awake(toma))">                                        # E-MATCH-RELATION-SUBJECT
  ```
  The sugar admits a guard class its stated desugaring rejects outright. An author who believes
  the page — and this page is otherwise one of the best in the docs — writes the "explicit twin"
  of a line that already compiles and gets a hard error, with a diagnostic that never mentions
  that the sugared form is legal.
- **And the tooling states the illegal form as fact.** `lute trace` renders a guarded content
  line as a match decision, using the guard as the subject:
  ```
  <match holds(awake(toma))>   -> arm 1 ((holds(awake(toma))))
      @toma  Don't put a hand on that ring. It'll take the arm and thank you for it.
  <match !holds(awake(toma))>   -> otherwise
  ```
  `<match holds(awake(toma))>` is, character for character, the subject the checker refuses. The
  preview tool desugars exactly the way the doc says — internally, where the restriction does not
  apply — and prints the result to the author as if it were source.
- **Resolution** — none available; T7.6 records what the scene ships.
- **Verdict** — `DOC-WRONG`, and it is the flavour the table ranks above `DOC-GAP` for a reason:
  the sentence is emphatic ("exact", "identically", "visibly interchangeable"), so an author does
  not go looking. The claim is true for scalar guards, which is every example on the page. One
  qualifying clause fixes it — *"exact sugar wherever the guard is a legal `<match>` subject; a
  relational guard is legal on a line and not as a subject (§8)"* — and it would have saved the
  round trip that produced T7.6.

#### T7.8 — a line with no words gets a `lineId`, a `voiceKey`, a command record and a translation row, and nothing says a thing — TOOL-DEFECT (silence)

- **Intent** — `hydroponics`'s third answer is to say nothing. The beat wants a held silence and
  then Vesna's reaction to it. Reaching for the cheapest spelling of "this character is present
  and produces no words":
  ```lute
  @vesna{code="0160" emotion="hollowed"}:
  ```
- **Result** — `ok`, 0 warnings. And downstream:
  ```json
  {"kind":"line","addr":"001-0100","role":"dialogue","speaker":"vesna","text":"",
   "emotion":"hollowed","lineId":"anseo.s01ep90.vesna_0010","voiceKey":"vesna-0010"}
  ```
  `lute loc export --format json` emits it as a translatable unit:
  ```json
  {"code":"0010","kind":"line","lineId":"anseo.s01ep90.vesna_0010","speaker":"vesna","text":""}
  ```
  So an empty line reaches a translator as a row with nothing in it, reaches a voice pipeline as a
  `voiceKey` implying a recording exists, and reaches `lute loc report` as a line with zero words.
- **Resolution** — the committed scene does not use it. The silence is
  `::camera{focus="vesna" zoom="1.15" duration="1.4" wait="true"}` — a documented blocking hold
  (`directives.md`, "Timing & the `wait` model") — followed by Vesna's line. See T7.9 for why
  this substitution is recorded as a *good* one.
- **Verdict** — `TOOL-DEFECT`, filed under the protocol's **silence** category. Nothing here is a
  language question: an empty content line is a plausible thing to type, the parser accepts it,
  and every consumer downstream treats it as a real line. The checker knows the text is empty at
  the moment it assigns the `lineId`. `W-CODE-AFTER-END` exists for a line the checker proved
  will never play; there is no counterpart for a line that will play and say nothing. This is
  T5.7's shape one step earlier in the pipeline — the localization and production surfaces
  accepting content the checker could have flagged for free — and the fix is the same size:
  `W-EMPTY-LINE`, or exclude empty text from `loc export`.

#### T7.9 — a silence has to be attached to a staging subject, and the one form that isn't is undocumented — recorded, no verdict

- **Intent** — as T7.8: a beat of nothing, held, then a reaction.
- **Attempt / result** — there is no beat primitive. `::pause{duration="1.2"}` and
  `::beat{duration="1.2"}` are both `E-UNKNOWN-DIRECTIVE`. The documented way to hold the script
  is `wait="true"` with a `duration` on a staging directive (`directives.md` §"Timing & the
  `wait` model"), so a silence is spelled as a camera instruction, a background change, or a
  video.
- **The undocumented form, found by reading the runtime contract and then probing.** `barrier_at`
  is "the timeline's explicit `<timeline duration>` when present, otherwise the maximum clip end
  across all tracks (`0.0` for an empty timeline)". So a timeline with **no tracks at all** and an
  explicit duration is a pure hold. It checks clean and compiles to exactly one record:
  ```lute
  <timeline duration="1.2">
  </timeline>
  ```
  ```json
  {"kind":"barrier","addr":"001-0200","timeline":1,"at":1.2}
  ```
  A timeline containing one empty `<track>` also checks clean. Neither shape appears anywhere on
  the website; "pause" does not occur in `packages/website/src/content/docs` at all, and the only
  mention of an empty timeline in the repo is the parenthetical above, which is about the
  fallback case and reads as though an empty timeline were a no-op.
- **Resolution** — I used the camera. `::camera{focus="vesna" zoom="1.15" duration="1.4" wait="true"}`
  during the silence is *better* writing than a bare hold: the camera pushing in while the player
  says nothing is the shot I want anyway.
- **No verdict, deliberately.** I considered `ERGONOMIC` and rejected it. The criterion is "the
  working form is materially worse than the natural one", and here the working form is better. It
  is recorded because the protocol asks for every substitution including small ones, and because
  the *next* author who wants a beat with no camera in it will find nothing — one sentence in
  `directives.md` pointing at the `wait` model, and one line in the timeline page noting that an
  empty timeline with a `duration` is a legal hold, would close it.

#### T7.10 — `lute trace` prints `## Shot 1.` where its own documented transcript prints the author's heading — DOC-WRONG

- **Intent** — four scenes into a story, use `trace` to check I had not misplaced a beat.
- **Attempt** — `lute trace docs/examples/anseo/scenes/hydroponics.lute --project docs/examples/anseo --mock …`
- **Result** — the transcript opens `## Shot 1.` My heading is `## Hydroponics`, and the compiled
  artifact keeps it: `"shots":[{"shot":1,"heading":"Hydroponics"}]`. The tool holds the title and
  prints an ordinal.
- **The docs say otherwise, and I ran their exact command to be sure.**
  `tooling/tracing.md` §"Reading the transcript" gives:
  ```console
  $ lute trace docs/examples/choice-persist.lute --choose sofaHelp=help
  trace: choice-persist.lute  (seeds: 0 paths, 0 facts; 1 selection)
    ## Recording the Choice
  ```
  Run against the shipped binary and the shipped file:
  ```console
  $ ./target/debug/lute trace docs/examples/choice-persist.lute --project docs/examples --choose sofaHelp=help
  trace: docs/examples/choice-persist.lute  (seeds: 0 paths, 0 facts; 1 selection)
    ## Shot 1.
  ```
  The documented output and the actual output differ on the one line that tells you where you are.
- **Resolution** — none needed; I read shot ordinals instead.
- **Verdict** — `DOC-WRONG`. Cheap on its own — but this is the tool an author reaches for
  precisely when they are holding four scenes in their head, and the shipped example promises the
  affordance that would help. It is also the log's dominant pattern once more, in miniature: the
  information is in hand (`heading` is in the IR, and the doc proves the transcript once printed
  it) and the surface drops it. Compare T5.9 — `trace` recording `::end` and dropping its
  `reason` — and T4.10.

#### T7.11 — a mock YAML key spelled `choices:` instead of `choose:` is silently discarded — recurrence of T3.10

- **Attempt** — the first mock file I wrote for `hydroponics`:
  ```yaml
  state: { run.shedPressure: 2, run.vesnaTrust: 0 }
  choices: { thePlainAnswer: ownIt }
  ```
- **Result** — `trace: … (seeds: 2 paths, 0 facts; 0 selections)`, and the branch line reads
  `-> ownIt (auto)`. With the key corrected to `choose:`, `1 selection` and `-> ownIt`. No
  diagnostic either way. The walk still *looked* right — first-eligible auto-selection happened to
  pick the arm I had asked for — so the only two signals that my mock did nothing were a count in
  the header and the parenthetical `(auto)`.
- **Recurrence of T3.10** (`TOOL-DEFECT`), not re-counted in T7's tally. Recorded because it is a
  *different* key from the one T3.10 found, on the same file format, and because trace has a
  refusal code for every other mock malformation — `E-TRACE-MOCK-UNDECLARED`,
  `E-TRACE-MOCK-TYPE`, `E-TRACE-MOCK-FACT`, `E-TRACE-CHOICE`, `E-TRACE-EVENT`, `E-TRACE-ACCEPT`
  — so an author has every reason to believe a bad mock is reported. The five top-level keys are a
  closed set; an unknown one should be `E-TRACE-MOCK-KEY` with a did-you-mean, which the codebase
  clearly knows how to write.

#### T7.12 — four more scenes, thirty retyped frontmatter lines, and no mechanism to hoist any of them — ERGONOMIC

- **Intent** — write a fourth, fifth, sixth scene of one work. Each one opens by restating that it
  is a scene, that it belongs to Anseo, that it is season 1, and which two schemas it imports.
- **Measurement**, over the eight scenes now in `scenes/` (frontmatter = lines between the `---`
  fences):

  | line | files carrying it verbatim |
  |---|---|
  | `kind: scene` | 8 / 8 |
  | `character: anseo` | 8 / 8 |
  | `season: 1` | 8 / 8 |
  | `uses: [../vocabulary.schema.yaml, ../world.schema.yaml]` | 7 / 8 |

  48 frontmatter key lines across the eight scenes and **18 distinct** ones — so 30 are
  byte-identical duplicates of a line already in another file — and only two keys, `episode:`
  and `after:`, carry per-file information. Frontmatter is 18% of the corpus's scene lines
  (48 of 260) and its majority is boilerplate.
- **Checked before filing, because the docs describe two composition mechanisms.** Neither
  applies: `uses:` unions *peer schemas* and `extends:` composes *schema* documents with override
  precedence (`components-and-extends.md` §"Schema `extends:`") — both operate on schema files,
  neither on document meta. `frontmatter-and-profiles.md` documents `profile:` falling back to the
  project's `defaultProfile`, so the *idea* of a project-level default for a frontmatter key
  already exists in the language and is applied to exactly one key. Collapsing the two `uses:`
  into one `extends:`-composed schema would save one import per file and touch none of the other
  three lines — and T6.2's rule (component and caller must *both* declare the vocabulary import)
  means the imports cannot be centralised anyway.
- **Verdict** — `ERGONOMIC`. Nothing is inexpressible and nothing is at risk; `lute new scene`
  scaffolds the block, so the typing cost is near zero. The cost is on the *reading* side and it
  compounds with the corpus: three of the six lines at the top of every file are noise that the
  eye must skip to find the two that matter, and `character:`/`season:` are already known to the
  project — `lute.project.yaml` **consumes** them, in `identity: lineId: "{prefix}.{speaker}_{code}"`
  where `{prefix}` is `{character}.s{season}ep{episode}`. The manifest that reads those two values
  cannot supply them. A `defaults:` block in `lute.project.yaml` mirroring the `defaultProfile`
  precedent, overridable per document, would take the eight scenes' frontmatter from 48 lines to
  17 — the 24 universal-key lines and the seven identical `uses:` lines gone, `wake.lute`'s
  divergent single import still stated where it diverges.

#### T7.13 — the envelope answers read-safety precisely and there is no answer to "what could this be by now" — ERGONOMIC

- **Intent** — the honest one, written while typing `hydroponics`'s first `<match on="run.shedPressure">`.
  I needed two facts I could not hold in my head across six documents: (1) **which values can
  `run.shedPressure` have when control reaches episode 4**, so I know how many arms to write and
  whether any is dead; and (2) **is Vesna on stage when this scene opens**, so I know whether to
  stage her or assume her.
- **Attempt** — `lute scenario docs/examples/anseo envelope anseo.s01ep04`, then the same for
  `anseo.s01ep01` and `anseo.s01ep06` as controls.
- **Result** — byte-identical output for all three:
  ```
  Guaranteed: run.shedPressure, run.vesnaTrust
  Possible:   run.shedPressure, run.vesnaTrust
  Possible \ Guaranteed — warning-grade reads: (none)
  ```
  `anseo.s01ep01` is the root, with no predecessor and no upstream write of any kind;
  `anseo.s01ep06` sits behind five scenes and four `::set`s. The tables cannot tell them apart,
  because both declared paths carry a `default:` and a defaulted path is always safe to read. For
  a schema written the way `state/schemas.md` encourages, these tables are constant across the
  entire graph.
- **And the feature is not broken — I checked, and nearly filed it as one.** In a scratch copy of
  the project I added `run.probeNoDefault: { type: number }` (no default), wrote it on the
  `ownIt` arm of episode 4 only, and read it from a guard in episode 6:
  ```
  Possible \ Guaranteed -- warning-grade reads:
    - ./scenes/stowaway.lute:32:44: state path `run.probeNoDefault` is set under your declared
      routes on SOME routes reaching this node, but not every one — not yet guaranteed (dsl §4.3)
  ```
  Exactly right, filtered to reads that actually occur, with a span, three documents from the
  write. `check-project` reports that same file `ok (0 warnings)`, which the tool's own header
  says it will. **This is a WORKED WELL and it is recorded as one.**
- **What is still unanswered.** Neither question I had. On (1): the connectivity layer holds every
  `::set` on every route — it must, to compute `Possible` at all — and reports set-membership
  rather than value. `run.shedPressure` is `0`, `1` or `2` at episode 4 and `0`–`3` at episode 6;
  I derived that by opening `cryobank.lute` and `machine-deck.lute` and adding integers, and if I
  had got it wrong the third `<match>` arm would simply be dead with nothing to say so. On (2):
  nothing anywhere models stage presence across documents — `trace` walks one document, `scenario`
  is pure graph math over `after:`, `context` describes vocabulary. Whether Vesna is standing in
  hydroponics when it opens is a question the toolchain has no representation for, which is the
  same hole T2.4 measured from the other side (a character exits, keeps speaking, and nothing
  objects).
- **Verdict** — `ERGONOMIC`, on T4.7's precedent ("nothing an author can run answers *is this
  quest reachable?*"). The working form is reading four documents and doing arithmetic; it is
  correct, it does not scale, and nothing checks it. A `--values` mode on `envelope`, reporting the
  reachable value set per numeric/enum path from the `::set`s already collected, would answer (1)
  with data the tool has in hand. (2) needs a model that does not exist and is a larger question
  than this entry.

#### T7.14 — environment note: the `lute-lsp` on `PATH` reports 17 errors on a file `check-project` calls `ok`

Not a language finding, and filed as T1.11's kind of note — but it cost real time and the next
drive-test agent will hit it.

`/usr/local/bin/lute-lsp` → `~/.cargo/bin/lute-lsp`, **dated 10 July**. The workspace's own
`target/debug/lute-lsp` is dated 3 August. Every editor-surface diagnostic in this session came
from the July binary, and on `scenes/cryobank.lute` — committed, unmodified, `ok (0 warning(s))`
under `check-project` — it reports **17 errors**, including `E-UNKNOWN-DIRECTIVE` on `::assert`,
`E-UNCLASSIFIED` on every `@speaker:` content line, `E-META-UNKNOWN-KEY` on `after`,
`E-SHOT-HEADING` on `## The Cryobank`, `anchor` values from a vocabulary this project does not
declare (`left, center, right`), and `E-USES-PARSE … has parse/frontmatter errors (10 issue(s))`
on `vocabulary.schema.yaml` — where 10 is the file's line count.

Two things make this worth the paragraph. First, the failure mode is *confident wrongness at
volume*: 17 red squiggles on a clean file, with plausible-looking codes, on the surface an author
looks at most. Second, nothing detects it. `backend.rs` resolves the project root correctly
(nearest ancestor `lute.project.yaml`) and reuses the CLI's own `resolve_document_snapshot`, so
the two surfaces genuinely cannot diverge *at the same version* — the whole divergence is the
three-week gap, and there is no handshake that would notice. `lute doctor` prints the three
version axes and then, under editor integration, only `VS Code extension: not detectable from the
CLI`; it never looks for a `lute-lsp` on `PATH` or compares its version to its own. That is a
one-line addition to a command whose entire purpose is "diagnose the local toolchain + project
setup", and it is the exact class of stale-binary trap T6's process note recorded for `lute`
itself. Every finding above was taken from `./target/debug/lute`, rebuilt first.

#### T7.15 — four documents, one story, and the parts that carried it — WORKED WELL

A maturity assessment that only lists friction is not an assessment, and this task wrote 49
content lines and 617 words across four scenes and four routes without the language getting in
the way once, outside the entries above.

- **`after:` takes a disjunction and the graph is right.** `stowaway` is reachable from either
  branch of the fork, and the natural spelling worked first try:
  ```yaml
  after: 'visited("anseo.s01ep04") || visited("anseo.s01ep05")'
  ```
  `lute scenario` resolves it to **two** edges, not one, and lays all eight scenes out
  in five topological layers with the ep03 fork and the ep06 join both correct:
  ```
  layer 2: scene(anseo.s01ep03), scene(anseo.s01ep10), scene(anseo.s01ep11)
  layer 3: scene(anseo.s01ep04), scene(anseo.s01ep05)
  layer 4: scene(anseo.s01ep06)
  scene(anseo.s01ep04) -> scene(anseo.s01ep06) [visited]
  scene(anseo.s01ep05) -> scene(anseo.s01ep06) [visited]
  ```
  This is the surface that most helped me hold four scenes at once, and it is the one that
  needed no probing.
- **Guarded content lines are load-bearing and cheap.** Five of them across two scenes, three
  guard classes — a scalar (`when="run.shedPressure >= 2"`), a relation
  (`when="holds(awake(ilsabet))"`) and a negated relation (`when="!holds(awake(toma))"`) — all
  correct on first write, all visible in `trace`. The `holds(awake(ilsabet))` line in
  `stowaway` is a payoff four episodes downstream of the `<choice>` arm in `cryobank` that
  asserts it, in a different file, and it required nothing but writing the guard.
- **`trace` and `run` agree, and both are readable.** Every branch of every scene was driven
  before commit: `machine-deck` on both arms via `--fact "awake(toma)"`; `hydroponics` on the
  honest arm at each of the three `run.shedPressure` values; `stowaway` with
  `--fact "awake(ilsabet)" --state run.shedPressure=2`. Then the same routes through the
  compiled artifact with `lute run`, which is the check that matters — the honest arm ends
  `run.vesnaTrust = 1`, the failed coupling ends `run.shedPressure = 2` from a seed of 1, and
  the facts block carries `knows(vesna, manifest)`. Task 9's `what-vesna-carries` is reachable,
  demonstrated rather than asserted.
- **The diagnostics I hit were, with one exception, excellent.** `E-AT-CONTEXT` names the
  construct, the directive and the clause. `E-MATCH-RELATION-SUBJECT` states its rationale
  (T7.6 disputes the rationale, not the message). `E-NONEXHAUSTIVE`, `E-TIMELINE-CONTENT`,
  `E-CLIP-OVERLAP`, `E-UNKNOWN-ATTR` and `E-TIMELINE-DURATION` all landed on the right span with
  the right words — bar the raw `1.2000000000000002` in `E-TIMELINE-DURATION`'s bound, which
  T7.2 traces to the same float accumulation. The four scenes' first drafts drew **six**
  diagnostics in total —
  `E-CLIP-OVERLAP` twice, `E-TIMELINE-CONTENT`, `E-MATCH-RELATION-SUBJECT`, and an
  `E-UNCLOSED-TAG`/`E-UNCLASSIFIED` pair from a stray line my editor left in the file rather
  than from anything I authored. Five were true; the sixth is T7.2.
- **Volume is cheap once the shapes are known.** 10 documents, 61 lines, 695 words, 6 choices
  (`lute loc report`). The fourth scene took a fraction of the first, and none of the four
  needed a construct that had to be discovered.

#### T7.16 — the `timeline` stamp is documented as a 0-based ordinal and is emitted 1-based — DOC-WRONG

Noticed while writing T7.3, which read the artifact's `timeline` stamps closely, and *not
classified there* — the observation sat in the entry as an unexamined detail while the same entry
asserted that nothing on the page was undocumented and no tool was lying. Filing it now.

- **Intent** — n/a authorially. T7.3 needed to know which records belonged to `spine-a`'s one
  timeline, so it read the stamp.
- **What the doc says.** `docs/runtime/timeline-semantics.md`, "What the IR carries", first
  bullet of the four:

  > - `timeline` — the 0-based **timeline ordinal** (`u32`) this record belongs to;

- **What is emitted.** `spine-a.lute` has exactly one `<timeline>` — the corpus's second ever —
  and every stamped record in it, plus the barrier, carries `timeline: 1`. The union of ordinals
  over the whole artifact is `{1}`; there is no `0`.
- **Confirmed stable, not a one-off.** Three ways:
  1. `lute compile docs/examples/property-tracks.lute --project docs/examples` — a second
     document, unrelated to Anseo, whose first and only timeline also emits ordinal `1` and no
     `0`.
  2. A two-timeline probe document emits `[('vfx', 1), ('barrier', 1), ('sfx', 2), ('barrier', 2)]`
     — consecutive, document-order, and 1-based. So it is an off-by-one in the base, not noise.
  3. `crates/lute-compile/src/stage.rs` increments before assigning:
     ```rust
     cx.timelines += 1;
     let ordinal = cx.timelines;
     ```
     A counter starting at `0` and pre-incremented can never hand out `0`. The doc's "0-based"
     is unreachable by construction.
- **Which side is wrong — the doc, and it is not a close call.** The temptation is to call this a
  `TOOL-DEFECT` and renumber the compiler, since the doc is the normative page for this field.
  Two things argue the other way.

  First, `0` is not reserved. `ir.rs::Stamp` declares
  `pub timeline: Option<u32>` with `skip_serializing_if = "Option::is_none"`, so "this record is
  not in a timeline" is already spelled by *omitting the key*, not by `0`. The implementation is
  therefore not 1-based to protect a sentinel — it is 1-based incidentally, because the counter
  is pre-incremented. Neither base is load-bearing.

  Second, and decisively: the ordinal is only ever an **opaque correlation key**. Its whole job
  is to let a consumer group a barrier with the clips it joins, which the runtime contract
  describes as "every clip scheduled before `barrier_at` on every track of that `timeline`
  ordinal". Equality is the only operation the contract asks for. Nothing indexes an array by it
  and nothing does arithmetic on it. So the base carries no meaning — which is exactly what makes
  the doc's claim the defect: it is *gratuitous* precision, spent stating a fact the contract does
  not need, and it happens to be false. Meanwhile changing the emitted value would silently shift
  every artifact already produced by one, with no diagnostic and no version signal, breaking
  precisely the consumers who *did* read the doc and hard-code a base. A wire-format change to fix
  an adjective is the wrong trade.

  **I would change the doc:** strike "0-based" and write *"`timeline` — an opaque per-document
  timeline ordinal (`u32`), assigned in document order starting at `1`; treat it as a correlation
  key, not an index."* That states the truth, and the added clause tells an engine author the one
  thing they actually need — that equality is the only supported operation — which the current
  sentence, by naming a base, actively invites them to get wrong.
- **Resolution** — none needed authorially; T7.3 read the stamps correctly by observation.
- **Verdict** — `DOC-WRONG`. It is the table's canonical shape: the docs are present and state a
  behaviour that differs. Cheap, and it is the *quiet* kind — an off-by-one in a base has no
  symptom at the surface. An engine author who trusts the page allocates or indexes one slot
  short, and the artifact never tells them, because `1` is a perfectly plausible ordinal under
  either reading. Ranked as `DOC-WRONG` rather than `DOC-GAP` on the table's own rule: silence
  would have made me check the artifact, and this sentence is why I did not check it for three
  entries.

#### T7 summary

Sixteen entries, audited heading-by-heading against each entry's own verdict line rather than
carried forward from the running count: three *worked well* (T7.4, T7.15, and the
verified-correct envelope inside T7.13), three `ERGONOMIC` (T7.6, T7.12, T7.13), three
`DOC-WRONG` (T7.7, T7.10, T7.16), two `TOOL-DEFECT` (T7.2, T7.8), one `SPEC-WRONG` (T7.3), one
`LANGUAGE-GAP` (T7.1), two recurrences carrying no new verdict (T7.5 → T1.6/T3.7,
T7.11 → T3.10), one entry deliberately left without a verdict (T7.9), and one environment note
(T7.14). No `DOC-GAP` and no `AUTHOR-ERROR`.

Ten entries carry one of the seven verdicts (T7.1, T7.2, T7.3, T7.6, T7.7, T7.8, T7.10, T7.12,
T7.13, T7.16); the other six carry exactly one non-verdict disposition the protocol asks for —
*worked well* (T7.4, T7.15), *recurrence* of an earlier verdict (T7.5, T7.11), a declined verdict
with its reasoning stated (T7.9), and an environment note (T7.14). No entry carries two of the
seven and none carries a hyphenated hybrid. T7.8's `TOOL-DEFECT (silence)` is the protocol's
silence *category*, not a second verdict, and T7.13's embedded *worked well* is a
sub-observation inside an entry whose verdict line is singular.

**Two corrections this section made to itself, recorded because a log that inflates its own
findings is worth less than a shorter honest one.** T7.2 originally claimed `E-CLIP-OVERLAP` was
wrong in *both* directions and called the permissive half "the worse half"; the permissive half
does not exist, the checker is right there, and the entry now carries the retraction and the
re-probe that refuted it. And T7.3's assertion that "nothing is undocumented and no tool is
lying" was broader than its evidence — it praised a four-item doc list without checking the
items, and one of them is T7.16. Both corrections moved the count of *real* defects down and the
count of doc errors up.

**The authoring rule held, and it cost the story exactly one thing.** Every beat in the brief
was written before the Lute was, and one of them did not survive: the Purser cannot be
interrupted (T7.1). That is the whole `LANGUAGE-GAP` in four scenes, and it is a real one —
overlapping speech is not exotic, and the language's two temporal modes (reader-paced dialogue,
clock-paced staging) have nothing between them. Everything else was expressible; three things
were expressible only sideways (T7.6's dummy subject, T7.12's retyped frontmatter, T7.13's
arithmetic-by-hand), and each is recorded with the shape of the thing I actually typed.

**Volume surfaces a different class of finding than depth, which was this task's premise and it
held.** T1–T6 each drove one construct to its edge. T7 wrote four scenes and the findings that
fell out are: a boundary condition you only meet when a track has two clips (T7.2), a doc
sentence that is true for every example on its page and false for the guard class a fourth scene
reaches for (T7.7), a mock key you only mistype once you are writing mocks routinely (T7.11), and
30 duplicated lines that are invisible in one file and 18% of the corpus in eight (T7.12). None
of these is discoverable from a showcase scene. The single most valuable measurement in this
section is the plainest: **four scenes of real dialogue drew six diagnostics, five of them
true, and two of those five were an editor accident rather than an authoring mistake** — a
better first-draft hit rate than the log's tone to this point would predict.

**`<timeline>` is the headline construct and the reading is split down the middle.** As an
authoring surface it is the cleanest first-use since T5.1 — six tracks, three keying styles,
overlapping cross-track offsets, a correct barrier, and a diagnostic (`E-AT-CONTEXT`) that knows
exactly where it is (T7.4). As a *contract* it has one implementation defect and two documentation
defects, and they point the same way. `E-CLIP-OVERLAP` rejects the boundary hand-off its own
written spec makes legal — not because the rule or the comparison is wrong, both are right, but
because the per-track cursor accumulates `at + duration` in binary floating point and
`0.8 + 0.4` overshoots the `1.2` the author typed (T7.2). That is the one place T7.4's praise
for "correct cursor math" needs qualifying: the *math* is correct and the *arithmetic* is lossy,
which is why a hand-off works at `0.5 + 0.25` and fails at `0.8 + 0.4`. Then the track — the unit
all three timeline invariants are *about* — is not in the IR at all, so the second of the two
engine options the runtime contract offers cannot be implemented, and property tracks, whose
stated purpose is telling an engine that one subject has two independent writers, tell it nothing
(T7.3). And the one `Stamp` field that *is* documented for the engine's benefit is documented with
the wrong base (T7.16). The checker computes the guarantee and deletes the evidence; that is
T1–T6's dominant pattern arriving in the staging layer.

**Two recurrences worth naming because they are now three-and four-time offenders.** `lute
context`'s `directives (N)` block is incomplete for the third time and in a third way (T7.5: the
§7.5 timing attrs are documented as valid on *any* staging directive and listed on two of nine).
And nothing gates stage presence: `@toma` speaks before any `::auto` stages him, `@ilsabet` speaks
in a scene that never stages her at all, both at exit 0 — correct in my source because both carry
`{os}`, and unchecked either way, which is T2.4 from the other end.

**What T7 would fix first.** T7.2, because a correctness check gives a wrong answer for the
boundary authors write most, and because the repair is not the comparison operator this entry
first proposed — the comparison is already the correct half-open test — but an epsilon tolerance,
or quantising timeline time to integer milliseconds and removing the class. Then T7.3, one field
on `Stamp`. Then T7.7, one clause on one sentence — the cheapest item in this whole log and the
one that cost this
task the most, because a page that says "exact" and "identically" and "visibly interchangeable" is
a page you stop questioning. T7.1 is the only entry here asking for language design rather than
repair, and the minimal version — content lines admitted in a `<track>` when they carry an
explicit `at` — does not disturb the rationale that currently excludes them.

### T8 — The convergence scenes

Toolchain 0.9.0 / language 0.9.0 / IR 0.9.0, `./target/debug/lute`, rebuilt before the first
probe. Three scenes added — `scenes/spine-b.lute` (`anseo.s01ep07`, the second shed, where the
ep04/ep05 routes are reconciled), `scenes/archive.lute` (`anseo.s01ep08`), `scenes/purser.lute`
(`anseo.s01ep09`, the confrontation) — both terminals repointed, and a two-line `facts:` block
(one ground fact) added to `world.schema.yaml`. Thirteen documents; the graph is complete for
the first time.

T7 measured what four scenes cost. T8 measures what the *eleventh* scene costs, which is a
different question again: nothing here is about a construct's first use. Every finding below
is about holding a whole work in one head, and most of them are only visible once there is a
whole work to hold.

#### T8.1 — nothing in the language can ask how the player arrived; the only route marker in this project is a content fact that means something else — LANGUAGE-GAP

This is the entry T8 exists for, and it was the first thing typed.

- **Intent** — written before any Lute. `spine-b` is where episodes 4 and 5 rejoin, and the
  release of module five has to read differently depending on which way you came. Three
  readings of one event: *you seated the coupling with Toma and it lets go clean*; *you were
  down there and failed and it goes with margin*; *nobody was ever down there, and it tears for
  a reason nobody in the scene can name*. The third is the one the beat is for — the difference
  between a failure and an absence — and it needs the scene to know you never went.
- **Attempt** — the obvious form, and the same predicate the document's own frontmatter is
  already using two lines above the shot heading:
  ```lute
  @narrator{code="0055" when="visited('anseo.s01ep05')"}: You had both hands on that ring an hour ago.
  <match on="visited('anseo.s01ep05')">
  <when test="$ && holds(awake(toma))">
  ```
- **Result** — twice, once per site:
  ```
  spine-b.lute:23:29: error [E-CEL-PROFILE] `visited(…)` is outside the Lute-CEL profile — only
  operators, literals, lists, `?:`, `in`, `has()`, `isSet()`, `holds()`, `count()`, `validAt()`,
  and `now()` are permitted (dsl §8.4, 0.3.0 §8)
  ```
  The diagnostic is good — it enumerates the entire admissible surface, which is exactly what an
  author needs at that moment — and the restriction is **documented, precisely, in the one place
  you would look**. `connectivity/scene-graph.md`: *"These predicates are scoped to this one
  slot; writing `visited(...)` in any ordinary CEL guard is just an unknown-function error."*
  So no `DOC-GAP` and no misdirection. I did not have to read Rust; I had to read one sentence,
  and the sentence was there.
- **What the language offers instead, enumerated rather than assumed.** The admissible guard
  surface is scalar state (`run.shedPressure`, `run.vesnaTrust`), the fact database
  (`holds`/`count`/`validAt`), and nothing else. So "came via ep05" has to be encoded as a state
  write or a fact assertion that only that route performs. Anseo has neither by design — and
  **has one by accident**:

  | candidate | what it actually means | usable as "came via ep05"? |
  |---|---|---|
  | `run.shedPressure >= 3` | the machine-deck arm failed *and* you woke Toma in ep02 | no — 3 is reachable only on one of four ep05 sub-routes |
  | `run.vesnaTrust >= 1` | you answered Vesna honestly in ep04 | no — the `deflect`/`saidNothing` arms leave it 0 |
  | `holds(awake(toma))` | you woke the engineer in ep02 | no — orthogonal to which room you walked into |
  | `holds(knows(vesna, manifest))` | Vesna has read the loading manifest | **yes, by accident** |

  `hydroponics.lute` line 20 asserts `knows(vesna, manifest)` unconditionally, at the head of the
  scene, before its first branch. `machine-deck.lute` asserts nothing. So on every route through
  ep04 the fact holds at ep07 and on every route through ep05 it does not, and
  `holds(knows(vesna, manifest))` is a total, correct discriminator between the two arrivals.
- **Resolution** — the committed `spine-b` carries two three-arm `<match on="true">` blocks
  keyed on exactly that proxy, with a thirteen-line comment above the first explaining what the
  guard is standing in for, because without one the next author reads it as a claim about
  Vesna's knowledge and edits it accordingly. The arms are ordered so the proxy is tested first
  and `<otherwise>` carries "machine deck, alone":
  ```lute
  <match on="true">
  <when test="holds(knows(vesna, manifest))">   <!-- came via hydroponics: nobody was at the coupling -->
  <when test="holds(awake(toma))">              <!-- came via the machine deck, with Toma -->
  <otherwise>                                   <!-- came via the machine deck, alone -->
  ```
  The beat survives intact. Every line I wrote first is in the file.
- **What the proxy costs, which is the whole entry.** It is not a wordier spelling of the intent;
  it is a different proposition that happens to be coextensive with it *in this corpus, today*.
  1. **It is one `::assert` away from silently inverting.** Anything that teaches Vesna the
     manifest on the other route makes both arms fire wrong, at exit 0, with no diagnostic. This
     is not hypothetical: **`archive.lute`, written later in this same task, asserts
     `knows(vesna, manifest)` — the brief requires it.** The only thing keeping `spine-b`'s
     guards honest is that ep08 is downstream of ep07. A convergence guard whose correctness
     depends on the topological position of an unrelated scene's fact write is a trap with a
     three-week fuse.
  2. **Nothing can check it.** There is no construct that means "arrived via", so no analysis
     can confirm the proxy still discriminates. `check-project`, `scenario`, and `envelope` all
     read it as an ordinary fact query and are right to.
  3. **The negation is worse than the positive.** "Came via the machine deck" is
     `!holds(knows(vesna, manifest))` — the absence of a fact about a third character standing
     in for the presence of a room you walked through.
  4. **The asymmetry is total.** "This line on either route" is free: write it unguarded, which
     is what convergence means. "This line on one route" has no spelling at all. A language that
     makes the join free and the disjoin impossible has picked the easy half.
- **Verdict** — `LANGUAGE-GAP`, **shape (b)**. The story is intact — nothing was cut, and the
  three readings of the module-five release are the three I wrote down before opening the editor
  — so shape (a) does not apply. What applies is the second clause exactly: the intent is
  reachable only by encoding it as something else, and **nothing in the language means it, so
  nothing can check it**.

  Stated as a design claim, because the assignment asked for one either way: **Lute distinguishes
  *what is true now* and has no representation of *how you got here*.** That is a coherent
  position — it is the same position a pure state machine takes, and it has the real virtue that
  a scene's behaviour is a function of its state and not of its history, which is what makes
  `envelope` computable at all. But a branching work's convergence points are precisely where an
  author needs the other thing, and the language already computes the answer: `check-project`
  builds the `visited(K)` key set and resolves every prerequisite formula against it, so
  arrival-by-key is a first-class notion *one slot over*. The minimal fix is to admit the same
  three predicates in a content guard — `visited("anseo.s01ep05")` as a read-only query over the
  visited set the engine already maintains for `after:`. It introduces no new state, no new
  vocabulary, and no new analysis; it makes the language able to say the thing its own graph
  layer is already made of. Failing that, an author must mint a marker per route, and the honest
  version of that is a declared `run.arrivedVia` enum written by hand in every scene — which is
  a second, parallel, unchecked copy of the scene graph.

#### T8.2 — a `<choice>` accepts any attribute you invent, discards it, and reports `ok`; one of the attributes it silently eats is one the spec says was removed, whose sibling gets a bespoke error — TOOL-DEFECT (silence)

Found in the first draft of `spine-b`, reaching for the single most obvious thing at a routing
branch. It is the sharpest finding in this task and the cheapest to fix.

- **Intent** — `spine-b`'s branch is a routing choice: the archive, the Purser, or stay with
  module nine. Those are ep08, ep09 and ep11. The choice should name where it goes.
- **Attempt** — verbatim, in the first draft:
  ```lute
  <choice id="readTheRecord" label="Go to the archive and find out what it is counting" goto="anseo.s01ep08">
  <choice id="goAtItDirect" label="Go forward and say stop" goto="anseo.s01ep09">
  <choice id="stayWithNine" label="Stay in the module" goto="anseo.s01ep11">
  ```
- **Result** — `ok: docs/examples/anseo/scenes/spine-b.lute (0 warning(s))`, exit 0. The same
  run reported the two `E-CEL-PROFILE` errors from T8.1 on the lines above, so the file *was*
  being checked. And the artifact:
  ```console
  $ lute compile …/spine-b.lute --project docs/examples/anseo -o /tmp/spineb.json
  $ jq -c '.commands[]|select(.kind=="choice")' /tmp/spineb.json
  {"kind":"choice","addr":"001-2300","branchId":"whatsLeft","recordKey":"scene.choices.whatsLeft",
   "options":[{"id":"readTheRecord","label":"Go to the archive…","lineId":"…","target":"001-2400"}, …]}
  $ grep -c goto /tmp/spineb.json
  0
  ```
  Three routing declarations, gone, at exit 0, with no warning.
- **Generality — it is the whole logic-tag family, and the one exception proves it is an
  omission.** One probe file, one unknown attribute on each tag:

  | construct | probe | result |
  |---|---|---|
  | `<branch id="p" nonsenseOnBranch="zzz">` | unknown attr | **`ok`, silently discarded** |
  | `<choice … goto= nonsenseOnChoice= next=>` | three unknown attrs | **`ok`, silently discarded** |
  | `<match on="true" nonsenseOnMatch="zzz">` | unknown attr | **`ok`, silently discarded** |
  | `<when test="true" nonsenseOnWhen="zzz">` | unknown attr | **`ok`, silently discarded** |
  | `<hub id="h" nonsenseOnHub="zzz">` | unknown attr | **`ok`, silently discarded** |
  | `<choice … nonsenseOnHubChoice="zzz">`, *inside a `<hub>`* | unknown attr | **`ok`, silently discarded** |
  | `<otherwise nonsenseOnOtherwise="zzz">` | unknown attr | `error [E-LOGIC-CONTENT] <otherwise> takes no attributes (dsl §7.3)` |

  Six constructs, **five of six open, one enforced.** `<otherwise>` is the one logic tag whose
  permitted set is *empty*, and it is the one that checks. The five with a permitted set do not
  enforce it — and `<choice>` is open in *both* of its positions, under `<branch>` and under
  `<hub>`, which rules out "hub choices go down a different path". `dsl 0.1.0 §7.3` specifies
  them in one list — *"Required / permitted attributes per tag"* — and closes it with
  *"Missing/unknown required attributes are a static error."*
- **And the neighbouring constructs get this right, which is what makes it an inconsistency
  rather than a policy.** Same project, same run:
  ```
  probe.lute:12:55: error [E-UNKNOWN-ATTR] `::auto` has no attribute `nonsense`
  probe.lute:13:36: error [E-UNKNOWN-ATTR] unknown content-line attribute `nonsense` (dsl 0.1.0 §7.1)
  ```
  Directives: closed and enforced, with a per-construct message. Content lines: closed and
  enforced, with its own message and a clause reference. Logic tags: open. Three attribute
  surfaces in one language, two enforced.
- **The documented-removal case, and it is not a hypothetical author typing gibberish.**
  `dsl 0.1.0 §7.3`, on `<choice>`:

  > optional run-promotion sugar `persist`, `into`, `value` (§11.1). The persist target attribute
  > is `into` (**renamed from 0.0.1 `as`**); `as` on a `<choice>` is **no longer accepted** (it
  > survives only on content lines as the display-label override, §7.1).

  Two attributes removed from `<choice>` by two different releases. One file, one choice each,
  side by side:
  ```lute
  <choice id="a" label="removed in 0.1.0" as="run.vesnaTrust" value="1">
  <choice id="b" label="removed in 0.6.0" persist into="run.vesnaTrust" value="1">
  ```
  ```
  probe-removed.lute:16:41: error [E-PERSIST-REMOVED] the `persist` attribute was removed in
  0.6.0 — `into=` alone now records the run fact (dsl 0.6.0 §2.2)
  failed: probe-removed.lute (1 error(s), 0 warning(s))
  ```
  One diagnostic. The 0.6.0 removal has a **dedicated error code, a column-exact span, and a
  migration instruction**. The 0.1.0 removal, three lines above it, is eaten. The exit code is 1
  entirely because of `persist`.
- **What the silence costs, measured.** `into=`/`value=` lower to a real write inside the arm:
  ```json
  {"kind":"set","addr":"001-0300","path":"run.vesnaTrust","op":"=","value":"1","expr":{"lit":1.0}}
  ```
  The same file with `as=` in place of `into=` compiles to **no `set` record at all**, `ok`, exit
  0. The severity rests on that silence and not on a migration story, because **`lute fix`
  migrates the rename.** Its `--help` says so — *"`<choice>`/`<hub>` choice `as="…"` →
  `into="…"` (dsl §7.3)"* — and it does it, on a probe carrying two such choices:
  ```console
  $ lute check probe-fix.lute --project docs/examples/anseo
  ok: probe-fix.lute (0 warning(s))
  $ lute fix probe-fix.lute
  lute: migrated 2 edit(s) to 0.2.2
  $ grep -c 'into="run\.' probe-fix.lute
  2
  ```
  So an author migrating a 0.0.1 project who *runs `lute fix`* is safe, and the first draft of
  this entry rested the severity on a migration narrative — hits `E-PERSIST-REMOVED`, deletes
  `persist`, keeps `as`, ships a green build with every choice-driven state write gone — that
  the toolchain in fact mitigates. The narrative is not the finding. What survives it, exactly,
  is the silence: `lute fix` is **opt-in**, `lute check` reports `ok` on the unmigrated file,
  and nothing in the toolchain tells an author the file needs fixing. The migration hazard is
  the sharpest *illustration* of the silence; the silence is the defect, and it eats a state
  write on any file nobody thought to run `fix` over.
  And `as` is not a made-up word: it is a **live content-line attribute** in the same language
  (`dialogue-and-cast.md`: *"`as` (a one-off speaker-label override)"*). The author is typing a
  real attribute one construct to the left of where it works.
- **Resolution** — the committed `spine-b` drops `goto=` and carries a comment saying the arms
  are intentions rather than routes (T8.3). Nothing in the shipped corpus depends on this
  behaviour.
- **Verdict** — `TOOL-DEFECT`, filed under the protocol's **silence** category, and it is the
  worst-shaped instance of it in this log. The language is not at fault: §7.3 enumerates the
  permitted set per tag and says unknown attributes are a static error. The docs are not at
  fault: they state the `as` → `into` rename explicitly and say `as` is "no longer accepted". The
  checker simply does not implement the closure for five of the six logic constructs — and it *has*
  the list, because `<otherwise>`'s empty set is enforced from it and `E-PERSIST-REMOVED` is a
  bespoke check written against it. The fix is `E-UNKNOWN-ATTR` on logic tags, the same code the
  directive and content-line paths already raise, against the §7.3 table that already exists. It
  would have caught all three of my `goto=`s, both spellings of the migration hazard, and every
  future attribute that gets renamed.

#### T8.3 — the branch and the graph are disjoint layers, and no choice a player makes can decide which scene comes next — LANGUAGE-GAP

T8.2 is the diagnostic hole. This is the modelling question underneath it, and the answer does
not change when the hole is fixed.

- **Intent** — the ordinary shape of a convergence point. The player leaves `spine-b` one of
  three ways, and picking one closes the other two. Three successors exist and are already
  written into the graph: ep08 (archive), ep09 (Purser), ep11 (the shed ending).
- **Attempt and result** — three routes, in the order I reached for them.
  1. **Name the successor on the choice.** `goto=` → accepted, discarded, `ok` (T8.2). There is
     no `goto`, `next`, `route` or `then` on `<choice>`; §7.3's permitted set is
     `id`, `label`, `when`, `into`, `value`, and that is all.
  2. **Have the successor read what the choice wrote.** The natural inversion: `::set` a path in
     each arm and let each downstream scene's `after:` test it. The `after:` grammar admits
     **no state reads at all**:
     ```
     Formula ::= "visited(" StringLit ")" | "completed(" StringLit ")" | "active(" StringLit ")"
               | "(" Formula ")" | Formula "&&" Formula | Formula "||" Formula
     ```
  3. **Exclude the routes not taken.** `after: 'visited("anseo.s01ep03") && !visited("anseo.s01ep04")'`:
     ```
     machine-deck.lute:7:1: error [E-CONN-PROFILE] `!_(…)` is outside the `after` prerequisite
     profile — only `visited("id")`, `completed("id")`, `active("id")`, `&&`, and `||` are
     permitted (no negation, arithmetic, comparisons, or other calls)
     ```
     Exact, documented, and it names the exclusion explicitly. The grammar is **monotone**: a
     prerequisite formula can only ever become *more* true as a run proceeds, so once a scene is
     available it is available forever.
- **The consequence, which is larger than the branch I was writing.** Anseo has been described
  through six tasks as forking at ep03 — T7.15 calls ep04/ep05 "the ep03 fork and the ep06 join".
  **It is not a fork.** Both scenes declare `after: 'visited("anseo.s01ep03")'`, both become
  available at the same instant, and nothing anywhere says they are alternatives. A conformant
  engine may play ep04, then ep05, then ep06, and every guard I wrote in T8.1 to discriminate
  the two arrivals is then simply wrong — `knows(vesna, manifest)` holds, so `spine-b` narrates
  "nobody had been down to five since the wake" to a player who spent the last scene down at
  five. That is not a bug in my guard. There is no guard that survives it, because the exclusion
  the story depends on is not stated anywhere and cannot be.
- **The one route that does work, and what it costs.** Choice-driven *unlocking* is expressible,
  laundered through the quest layer: a choice arm writes `::set{run.wentToArchive = 1}`, a quest
  document carries `<objective done="run.wentToArchive == 1">`, and the successor declares
  `after: 'completed("theArchiveRun")'`. State reaches the graph, through a construct whose
  predicates *can* read state. That is a real mechanism and it is T5.4's shape at the graph layer
  — mirror the claim into a second syntax in a third file. It costs one quest document per branch
  point, and it still cannot express exclusion, because the *other* successors' formulas remain
  unable to say "and not this one". So even the working proxy buys availability and never
  alternation.
- **Resolution** — the shipped `whatsLeft` branch has three arms that are **intentions rather
  than routes**, with a six-line comment saying so, and one of them `::set{run.vesnaTrust += 1}`
  so the choice is at least observable downstream. The player's declaration of where they are
  going does not affect where they go. That is the substitution, and it is shape (a): I changed
  what the branch means to fit what the branch can do.
- **Verdict** — `LANGUAGE-GAP`, **both shapes**, and the honest framing takes a paragraph because
  the design is deliberate and says so. `connectivity/scene-graph.md` opens: *"Scenes and quests
  declare their **prerequisites** — what must have happened before this node is available."*
  Availability, explicitly; routing is the engine's. The split is coherent and it is what makes
  the graph analysable — a monotone formula set is acyclic and decidable, and negation would
  cost that.

  What follows from it is the thing an author needs told plainly and is told nowhere: **`lute
  scenario`'s graph is not the story's graph.** It is the availability lattice — the partial
  order in which content unlocks. The story's actual shape, the one with alternatives and
  consequences and roads not taken, lives in an engine Lute does not ship and cannot check. Every
  question T5.5 found unaskable is unaskable for this reason and not for `::end`'s: "can a route
  strand the player" presumes routes, and Lute has none.

  A minimal fix that keeps the monotone core: an `exclusive:` group declared once at project
  level — `exclusive: [[anseo.s01ep04, anseo.s01ep05]]` — naming node sets of which at most one
  may be visited. It adds no negation to the formula grammar, it is checkable by exactly the
  key-set machinery `check-project` already runs, and it would let `scenario` report the two
  things an eleven-scene work most wants: which nodes are genuine alternatives, and how many
  distinct routes the graph actually has. Without it, the corpus's central structural claim —
  that ep04 and ep05 are two ways through the same act — is a comment in a brief.

#### T8.4 — the envelope at the deepest node in the graph is byte-identical to the envelope at the root, and the arithmetic it declines to do is wrong in this very log — ERGONOMIC

T7.13 filed this shape and closed with "a `--values` mode … would answer (1) with data the tool
has in hand". T8 is the first task in a position to say what the missing mode costs, because T8
is the first task with eight scenes of upstream writes and a threshold guard that depends on
them. It cost a dead line — written, then caught and corrected to `>= 2` before the commit, so
nothing dead shipped — and the reason it cost one is that **T7.13's own hand arithmetic — the
"working form" it recommends — is wrong.**

- **Intent** — writing `spine-b`'s reaction to the shed clock, I needed one number: the largest
  value `run.shedPressure` can hold when control reaches episode 7. That decides whether
  `when="run.shedPressure >= 3"` is a line or a corpse.
- **Attempt** — `lute scenario docs/examples/anseo envelope anseo.s01ep09`, and the same for
  `anseo.s01ep01`, `anseo.s01ep07` and `anseo.s01ep10` as controls. ep09 sits behind **eight**
  scenes (ep01–ep08; ep11 is a sibling leaf, not a predecessor), **six** `::set`s and **four**
  `<branch>`es — recounted from source rather than carried forward: `cryobank` 2 sets / 1 branch,
  `hydroponics` 1/1, `machine-deck` 1/0, `spine-b` 1/1, `archive` 1/1, and nothing in `wake`,
  `spine-a` or `stowaway` — plus a relational world. ep01 is the root and has no predecessor of
  any kind.
- **Result** — all four are byte-identical:
  ```
  Guaranteed (safe to read under your declared routes):
    - run.shedPressure
    - run.vesnaTrust
  Possible (set on at least one declared route reaching this node):
    - run.shedPressure
    - run.vesnaTrust
  Possible \ Guaranteed -- warning-grade reads … : (none)
  ```
  T7.13 measured this across three nodes of an eight-scene graph. It holds across the complete
  eleven-scene graph, at the deepest node, with more than twice the content behind it. The
  tables are constant over the entire work, and they are constant *by construction*: both
  declared paths carry a `default:`, and a defaulted path is safe to read everywhere.
- **And the relational layer is absent from the envelope entirely.** Four declared relations (one
  of them the derived `can_halt`), a Datalog rule, a `facts:` seed, **eleven** `::assert`s across
  **five** documents (`cryobank` 4, `stowaway` 3, `archive` 2, `hydroponics` 1, `purser` 1) — and
  `envelope` reports two scalar paths and nothing else. There is no `Guaranteed`/`Possible`
  notion for facts. So at
  `purser.lute`, the scene whose every line is gated on who is awake, the tool that exists to
  say what is true when control arrives says nothing about the only thing the scene reads.
  That is not a defect against its spec — `envelopes.md` is about state paths — but the
  assignment asked whether the envelope is precise enough to write `purser.lute` against, and
  the answer is that it does not mention the subject.
- **What I actually had to do, and what it produced.** I read every `::set{run.shedPressure …}`
  in the corpus and worked the routes by hand:

  | cryobank arm | writes | `awake(toma)`? | machine-deck `otherwise` (`+1`, guarded `!holds(awake(toma))`) | total at ep07 |
  |---|---|---|---|---|
  | `wakeToma` | `+= 2` | yes | **cannot fire** | 2 |
  | `wakeIlsabet` | `+= 1` | no | fires if ep05 visited | 1 or 2 |
  | `wakeNobody` | — | no | fires if ep05 visited | 0 or 1 |

  **The maximum at episode 7 is 2. `run.shedPressure >= 3` is unreachable**, because the only
  `+1` outside the cryobank is gated on Toma being asleep, and Toma being asleep caps the
  cryobank contribution at 1.
- **T7.13 got this wrong, and I inherited the error.** Its text reads: *"`run.shedPressure` is
  `0`, `1` or `2` at episode 4 and `0`–`3` at episode 6; I derived that by opening
  `cryobank.lute` and `machine-deck.lute` and adding integers."* Adding the integers is exactly
  what produces `3` — `2` from `wakeToma` plus `1` from the machine deck — and it is wrong,
  because those two writes are mutually exclusive by a guard in a third place. The entry even
  names the failure mode one sentence later: *"if I had got it wrong the third `<match>` arm
  would simply be dead with nothing to say so."* It did get it wrong. The dead arm it predicted
  is the line I then wrote in `spine-b`:
  ```lute
  @vesna{code="0280" emotion="frayed" when="run.shedPressure >= 3"}: And it's ahead of its own schedule, because we kept handing it reasons.
  ```
  `check-project`: `ok`, 0 warnings. **And the checker does implement dead-gated-line analysis on
  exactly this construct**, with a message written for it, one attribute away from the guard it
  declines to judge. Four content lines, one probe file, same project:
  ```console
  $ lute check probe-dead.lute --project docs/examples/anseo
  probe-dead.lute:11:42: error [E-ARM-DEAD] this gated line can never be shown: its `when` guard
    is provably false (dsl 0.4 §7.2, §5.2)                      # when="false"
  probe-dead.lute:13:42: error [E-ARM-DEAD] … (same message)     # when="1 == 2"
  probe-dead.lute:14:42: error [E-ARM-DEAD] … (same message)     # when="run.shedPressure >= 0 && false"
  failed: probe-dead.lute (3 error(s), 0 warning(s))
  ```
  Line 12 — `when="run.shedPressure >= 99"`, a threshold no route can reach — draws **nothing**.
  So the analysis is not "syntactically `when="false"`", which is what T5.7's evidence suggested:
  it is `decide()`, the checker's constant folder, and it folds comparisons and conjunctions
  happily. What it does not do is read the write set. The author-visible consequence is that one
  error code, one message and one column serve two different proofs, and the diagnostic never
  says which one it just performed: `1 == 2` is caught, `>= 99` is not, and the second is the
  one an eleven-scene work actually produces.
- **Resolution** — the committed line reads `>= 2`, and it is exercised: driven through
  `lute run` on the `wakeToma` route (`run.shedPressure = 2`) it fires, and on the two `= 1`
  routes it does not. The finding is not that the guard was wrong. **The finding is that the
  only way I found out was by redoing, and correcting, a prior task's hand arithmetic.**
- **One more thing the question needs, which is why `--values` is harder than T7.13 implies.**
  "What can `run.shedPressure` be at ep07" is not well-posed under Lute's own graph model,
  because `after:` is monotone (T8.3): every scene stays available forever, so a player may
  re-enter `cryobank` and take `wakeToma` twice. Under revisits the reachable set is unbounded.
  My table is the no-revisit answer, which is the answer an author means and which nothing in
  the project declares. A `--values` mode would have to state its revisit policy, and the graph
  currently has nowhere to declare one.
- **Which verdict this half belongs to, argued rather than assumed.** The log's `TOOL-DEFECT`
  (silence) shape requires a promise the tool breaks — that is what T8.2 turns on. There is no
  such promise here, and I looked for one: 0.4.0 §5.2 states the reachability analysis is "LOCAL
  to one construct … no cross-construct graph"; the connectivity design restates it (*"0.4.0 §5
  reachability … is explicitly local to one construct"*, and *"cross-document reachability is a
  genuinely new analysis, not an extension of `decide()`"*); and the pass's own test suite pins
  the boundary by name — `undecided_guard_is_never_flagged`, `test="run.n > 1"` → clean. A
  value-range threshold is undecidable *by that construct alone*: it needs the write set of eight
  upstream documents plus a revisit policy the graph has nowhere to declare. So the dead-guard
  half is a missing feature behind a documented boundary, not a check lying about its own
  contract — which is why the verdict below is the one it always was.
- **Verdict** — `ERGONOMIC`, on T7.13's precedent, and the evidence for the severity is now
  much stronger than T7.13 could make it. The intent is fully reachable: I got the right number.
  The working form is reading five documents, building a three-by-three table, and noticing that
  two writes in different files are mutually exclusive because of a guard in a third. It is
  correct, it does not scale, **nothing checks it, and the one previous attempt at it in this
  log is wrong.** That is as close to a measurement of a missing feature as this drive test has
  produced: two authors, same question, one wrong answer, one dead line, zero diagnostics.
  `envelope --values`, reporting the reachable value set per numeric and enum path from the
  `::set` set the connectivity layer already walks — with guard-implied exclusions honoured, or
  failing that with an over-approximation clearly labelled as one — would have answered it in
  one command. Failing even that, `E-ARM-DEAD` extended to constant thresholds outside the
  over-approximated range would have caught the dead line for free.

#### T8.5 — `lute run` plays a `<choice>` whose guard is false; `lute trace` refuses the identical selection, and the runtime contract says to check it — TOOL-DEFECT

Found verifying `purser.lute`'s empty-room route, which is the route where the levers are
*supposed* to be closed.

- **Intent** — prove that the relational guards actually gate. `purser.lute`'s
  `invalidateTheVoyage` arm is `when="holds(awake(ilsabet)) && holds(knows(ilsabet, true_heading))"`.
  On the `wakeNobody` route Ilsabet is in a pod. The arm must not be selectable.
- **Attempt** — the compiled artifact, the reference runner, and a mock that asks for the arm
  anyway:
  ```yaml
  facts: ["awake(ottavio)", "found(ottavio)", "knows(ottavio, manifest)"]
  choose: { theCorrection: invalidateTheVoyage }
  ```
  ```console
  $ lute run /tmp/purser.json --mock /tmp/m-bad.yaml
  ```
- **Result** — it plays, in full, at exit 0, with no diagnostic. Re-run for this correction;
  `[…]` marks four elided middle lines of the arm and nothing else is trimmed:
  ```
  001-4000  choice [theCorrection] -> invalidateTheVoyage
  001-5400  set    run.vesnaTrust = 1
  001-5500  ilsabet: Filed voyage. Eleven years, four months, destination as filed. Read me the date.
  001-5600  purser: Filed arrival is four months from the yard. Elapsed is eleven years, four months.
  […]
  001-6200  vesna: Eleven years. Eleven years, and it just needed to be told it had got there.
  001-8100  purser: Crew departing. Allocation continues.
  -- final state --
    run.shedPressure = 0
    run.vesnaTrust = 1
    scene.choices.theCorrection = invalidateTheVoyage
  ```
  Ilsabet delivers **three** lines from inside a cryopod — the arm holds nine records: one
  `::set`, three `@ilsabet`, four `@purser`, one `@vesna` — the arm's state write runs, and the
  run records the selection as a fact about the playthrough.
- **`lute trace` refuses the same selection, on the same document, in the same project.** With
  the upstream `<match>` resolved so the branch is reached:
  ```console
  $ lute trace …/purser.lute --project docs/examples/anseo \
      --fact "awake(ottavio)" --fact "found(ottavio)" --fact "knows(ottavio, manifest)" \
      --fact "can_halt(toma)" --fact "can_halt(vesna)" \
      --choose theCorrection=invalidateTheVoyage
  purser.lute:70:1: error [E-TRACE-CHOICE] `--choose theCorrection=invalidateTheVoyage` is
  ineligible at its presentation point: its guard decided false at this presentation point
  (dsl 0.4.0 §4.4)
  trace refused: … — invalid mock input
  ```
  Line-exact on the `<choice>` it refused (`70:1` is the tag's start rather than the `when=`
  span), correctly reasoned, and it names the clause. The author's preview tool enforces
  the guard. The **artifact runner does not**, and the artifact runner is the one whose
  `--help` reads: *"the reference consumer of the runtime contract (docs/runtime/): command
  dispatch, CEL guards, facts + Datalog fixpoint, hubs, and quest lifecycle."* CEL guards is
  the second item on its own list.
- **And `lute test` refuses it too, because `lute test` is `trace`-based — which is the
  correction this entry most needed.** An earlier draft escalated from "`run` skips the check"
  to "a mock suite is how you prove a branching work's gates hold, and every such proof run
  through `run` is vacuous". **That is false, and one `--help` disproves it:** `lute test`'s
  reads *"every `*.test.yaml` under `dir` **traces** its scene against the declared mocks and
  asserts the declared expectations (transcript, state, quest status)"*. Traces. Verified with a
  matched pair on `purser.lute` — identical five-fact seeds, one asking for the guard-false
  option, one for the guard-true option:
  ```console
  $ lute test /tmp/t85
  FAIL  /tmp/t85/tests/guard-false-choice.test.yaml  (…/anseo/scenes/purser.lute)
        trace refused: invalid mock input
  PASS  /tmp/t85/tests/guard-true-choice.test.yaml   (…/anseo/scenes/purser.lute)

  1 passed, 1 failed
  ```
  Exit 1, and `--json` records the failing case as `"exit": "refused"`,
  `"refusal": "trace refused: invalid mock input"`, `"expectations": []` — the walk never began.
  The positive control passes on the same seeds, so the refusal is the guard and not the
  harness. **The hole is therefore exactly one tool wide.** `check` proves the guard is emitted,
  `trace` enforces it, `test` inherits that enforcement, and only `run` — hand-driven, on a
  compiled artifact — does not. The correct claim: **hand-rolled `lute run` verification of an
  artifact is not guard-enforcing; `lute test` and `lute trace` are.**
- **It is not missing information.** The guard is in the artifact, on the option, as a raw CEL
  string:
  ```json
  {"id":"invalidateTheVoyage","label":"Tell it the voyage is already over",
   "lineId":"anseo.s01ep09.theCorrection.invalidateTheVoyage",
   "when":"holds(awake(ilsabet)) && holds(knows(ilsabet, true_heading))","target":"001-5400"}
  ```
  And `run` demonstrably evaluates CEL and runs the Datalog fixpoint elsewhere in the same
  walk — every `<match>` arm in the transcript above resolved correctly against the fact
  database, and `can_halt(vesna)` was *derived* from the `facts:` seed plus an `::assert`
  (T8.8). The one guard it does not evaluate is the one attached to a mocked selection.
- **And the contract it names is explicit.** `docs/runtime/execution-model.md`'s dispatcher:
  ```ts
  case "choice":
  case "hub": {
    const opt = pickOption(cmd, state); // eligibility via evalExpr(opt.expr)
  ```
  Eligibility is the whole of what `pickOption` is specified to do.
- **Task 9's scenario tests are not undermined.** They are `*.test.yaml` under `lute test`, and
  the matched pair above is the proof that that path refuses an ineligible selection rather than
  playing it. Nothing in this entry asks Task 9 to re-verify anything.
- **Resolution** — none needed authorially; I verified the guards by driving each route with
  its own honest fact set instead, and all four behave correctly (transcripts in T8.13). But
  the verification I *wanted* — "prove this arm is closed on this route" — is the one `run`
  cannot perform, because asking for a closed arm gets you the arm. The way to perform it is
  `lute test` or `lute trace`, and both do it correctly.
- **Verdict** — `TOOL-DEFECT`, with the severity **narrower than the first draft of this entry
  claimed.** What is defective is real and unambiguous. `run --help` advertises "CEL guards" as
  the second item on its own list; `run` evaluates CEL everywhere else in the same walk — every
  `<match>` arm resolved correctly, content-line `when=` honoured, `can_halt(vesna)` derived
  through the Datalog fixpoint; the guard is in the artifact as `option.when`; and the one guard
  it skips is the one attached to a mocked selection. The reference consumer of the runtime
  contract does not implement one clause of the contract it names. `E-TRACE-CHOICE` already
  exists and already has the right words; `run` needs the same refusal, or — if replaying an
  ineligible selection is deliberately permitted for debugging — a loud
  `W-RUN-CHOICE-INELIGIBLE` and a flag to demand it.
  What this is **not** is a hole under every mock suite in the language. `lute test` traces, and
  the matched pair above shows it refusing the identical selection with a passing control. The
  cost is confined to hand-driven `run` verification of an artifact — which is what this task
  reached for, which is why the defect surfaced here, and which is the only place the claim can
  honestly be made.
  **The commit body (`7dbd8a2`) cannot be edited.** On this entry it is accurate — it says
  *"`lute run` plays a choice whose guard is false where `lute trace` refuses it"*, with no
  mock-suite escalation. What it does carry pre-correction is T8.11/T8.13's superseded count,
  "twelve relational guards over awake/knows/found and the derived can_halt": read **nine** guard
  expressions over **four** relations, one of which is the derived head — see T8.11's counting
  rule.

#### T8.6 — the runtime contract's reference dispatcher reads two fields the artifact does not have, and both are guard fields — DOC-WRONG

Found while establishing T8.5's contract citation, by checking the pseudocode against the
artifact it claims to dispatch.

- **What the doc says.** `docs/runtime/execution-model.md` introduces its dispatcher as
  authoritative about field names — *"The kinds below are exactly the `Command` variants
  (`ir.rs`)"* — and it is, almost everywhere. Verified field-by-field against
  `purser.lute`'s artifact:

  | dispatcher line | artifact | verdict |
  |---|---|---|
  | `writeState(state, cmd.path, cmd.op, evalExpr(cmd.expr, state))` | `{"kind":"set","path":"run.vesnaTrust","op":"+=","value":"1","expr":{"lit":1.0}}` | correct — `expr` is a lowered AST |
  | `facts.assert(cmd.relation, cmd.args)` | `{"kind":"assert","relation":"knows","args":["vesna","manifest"]}` | correct |
  | `next = cmd.target` (jump) | `{"kind":"jump","target":"001-1900"}` | correct |
  | `finish(cmd.reason)` (end) | `{"kind":"end","reason":"bridge-reached"}` | correct |
  | `pickOption(cmd, state); // eligibility via evalExpr(opt.expr)` | option carries **`when`**, a raw CEL string; there is no `opt.expr` | **wrong** |
  | `cmd.arms.find(a => truthy(evalExpr(a.expr, state)))` | arm keys are exactly `["target","test"]`; `test` is a raw CEL string | **wrong** |

- **Why it matters more than a typo.** Both wrong fields are the *guard* fields, and both fail
  the same silent way. An engine author transcribing the dispatcher gets `undefined` from
  `opt.expr` and `a.expr`; `truthy(undefined)` is false, so **every `<match>` takes
  `<otherwise>` and every guarded `<choice>` is permanently ineligible**, with no crash, no
  type error, and no artifact that looks malformed. The work plays, and it plays the default
  branch of everything.
  There is a second trap layered on it: `set`'s `expr` really is a lowered AST, so
  `evalExpr` is a sensible name there — but `when` and `test` are **raw CEL text**, which is
  T4's finding that guards are not lowered (the host must parse `holds(can_halt(toma))`
  itself). So even after fixing the field names, `evalExpr` means two different things in one
  switch statement and the doc never says so.
- **Resolution** — none needed authorially.
- **Verdict** — `DOC-WRONG`. Present and false, in a document that opens by promising fidelity
  to `ir.rs`, on the two lines an engine author cannot get wrong and survive. Rank it above the
  other doc errors in this log on the table's own rule: silence would make an author open the
  artifact, and this pseudocode is precisely why they would not. The fix is four characters —
  `opt.when`, `a.test` — **and one retraction the first draft of this entry missed.**
  `execution-model.md:190`, the prose immediately under the dispatcher, reads: *"`evalExpr` walks
  the portable `expr` AST (IR A7) carried alongside every guard"*. That sentence is also false,
  and it is the load-bearing one — it is what makes a reader believe `opt.expr` and `a.expr` are
  real field names rather than typos, because it asserts as a *general rule* the thing those two
  lines assume. No guard carries an `expr` AST. Verified again on `purser.lute`'s artifact:
  `arms[0]`'s keys are exactly `["target","test"]`, and the option carries
  `"when":"holds(awake(ilsabet)) && holds(knows(ilsabet, true_heading))"` — raw CEL text in both
  slots. Only `set` carries `expr` (`{"path":"run.vesnaTrust","op":"+=","expr":{"lit":1.0}}`),
  and `set` is not a guard. So the fix is three things: the two field names, the **retraction of
  line 190**, and one sentence saying `evalExpr` means two different operations in one switch —
  a lowered-AST walk for `set.expr`, a host CEL parse for `option.when` and `arm.test`.

#### T8.7 — a staging directive cannot be conditional, so gating one `::auto` costs a five-line block — ERGONOMIC

- **Intent** — `archive.lute`. If Ilsabet is awake she comes to her own drawer, so she has to be
  put on stage; if she is not, nobody does. One directive, one condition.
- **Attempt** — the attribute that gates the line directly underneath it:
  ```lute
  ::auto{character="ilsabet" anchor="center" action="unseal" when="holds(awake(ilsabet))"}
  @ilsabet{code="0100" emotion="clipped"}: Move. That drawer is mine and it has been for eleven years.
  ```
- **Result** — `archive.lute:29:60: error [E-UNKNOWN-ATTR] `::auto` has no attribute `when``.
  Correct and correctly documented: `when=` is a content-line and `<choice>` attribute, and
  `cel.md` lists its slots exactly. The content line under it takes the same guard happily.
- **Resolution** — the whole passage became a block, which needs `<match on="true">` because a
  relation may not be a `<match on>` subject (T7.6), plus a mandatory `<otherwise>`:
  ```lute
  <match on="true">
  <when test="holds(awake(ilsabet))">
  ::auto{character="ilsabet" action="unseal"}
  …four lines…
  </when>
  <otherwise>
  …three lines…
  </otherwise>
  </match>
  ```
  This is **better writing** — the two worlds diverge for more than one directive, and forcing
  me to write the else arm produced Ottavio's best line in the scene. So the resolution is not a
  complaint. But the cost is real and it is not always paid this well: `purser.lute` needs two
  more `<match on="true">` blocks, one of them wrapping a single `::auto` and a single line per
  arm, and `spine-b` needs two. **Five dummy-subject blocks in three scenes**, where T7 needed
  one in four.
- **Verdict** — `ERGONOMIC`, and filed separately from T7.6 rather than as a recurrence because
  the shape is different: T7.6 is *a relation cannot be a `<match on>` subject*, which is about
  the guard's type. This is *staging cannot be gated at all*, which is about the guard's
  position, and it stands even if T7.6 were fixed tomorrow. The two compose badly — a
  conditional entrance, which is the most ordinary thing in staged drama, costs a dummy subject
  it does not read and an `<otherwise>` it may not want. Admitting `when=` on staging directives
  would close it; it is the same guard slot, the same CEL profile, and the same evaluation the
  line one row down already performs.

#### T8.8 — the protagonist's companion was never `awake`, a derived relation could not fire for six scenes, and nothing anywhere said a word — Task 1 schema defect, `facts:` is the fix and it works

The latency is the finding. This was true from the moment `world.schema.yaml` was written and it
took until the eighth task and the eleventh scene to surface, because the only thing that would
ever surface it is a scene that needs the derived relation to fire for Vesna.

- **Intent** — `archive.lute`'s payoff. Vesna reads the shed sequence off the yard's own page,
  and therefore she can halt the shed. That is the rule, verbatim, in the project's own schema:
  `can_halt(C) :- awake(C), knows(C, shed_sequence)`. So: assert the second premise in the
  `readItAloud` arm and guard the payoff line on the head.
- **Attempt** —
  ```lute
  ::assert{knows(vesna, shed_sequence)}
  …
  @vesna{code="0230" emotion="level" when="holds(can_halt(vesna))"}: I can stop it. That is not the same as knowing how to make it want to.
  ```
- **Result** — `ok`, 0 warnings, and the line can never play. **`awake(vesna)` is asserted
  nowhere in the corpus.** Vesna wakes you in episode 1; she is the only character continuously
  present in all eleven scenes; and the relational world does not contain the fact that she is
  conscious. `cryobank.lute`'s three arms assert `awake(toma)` or `awake(ilsabet)` or nothing;
  `stowaway.lute` asserts `awake(ottavio)`. Nobody ever wakes Vesna because in the story nobody
  needs to.
- **What knew, and what said nothing.** T4.3 recorded the boundary honestly — the producibility
  analysis is relation-level, so `can_halt(vesna)` and `can_halt(toma)` produce byte-identical
  diagnostics — and that is the correct design. But T4.3 measured it on a *quest gate someone
  deliberately mistyped*. Here it is a live authoring consequence, and the list of surfaces that
  had the information and did not use it is longer than T4.3's:
  - `check-project`: `ok`. The relation is producible, the argument is a declared `crew` member,
    the query is well-formed.
  - `lute scenario` / `envelope`: silent — the whole fact layer is outside the envelope (T8.4).
  - `lute context`: prints `can_halt/1(crew) [derive]` and the rule, with no indication that one
    of the four `crew` members can never satisfy it.
  - `lute doctor`: not its job, and it does not.
  - And **`count(awake(_))` was short by one on every route in the project**, silently, which is
    a wrong answer rather than a missing one. Anything written against that count before today
    would have been off.
- **Resolution** — a two-line `facts:` block in `world.schema.yaml` carrying one ground fact, and
  it is the right construct rather than a workaround. `facts-and-datalog.md` documents a
  schema-level `facts:` block of ground facts:
  ```yaml
  facts:
    - "awake(vesna)"
  ```
  This is the corpus's **first use of `facts:`** in eight tasks. It works, end to end, and it
  works better than I expected:
  - `check-project` accepts it; the project's diagnostics are byte-identical before and after,
    exit 0, and no existing guard changes meaning (`machine-deck`'s `holds(awake(toma))`,
    `stowaway`'s `holds(awake(ilsabet))`, and the quest's `holds(can_halt(toma))` are all
    untouched).
  - **`lute run` auto-loads the seed and runs the Datalog fixpoint over it.** Driven on
    `archive.lute` with `facts: ["awake(ottavio)", "found(ottavio)", "knows(ottavio, manifest)"]`
    and `choose: { theSequence: readItAloud }`, the final fact block reads — verbatim, re-run:
    ```
    -- facts --
      awake(ottavio)
      awake(vesna)
      can_halt(vesna)
      found(ottavio)
      knows(ottavio, manifest)
      knows(vesna, manifest)
      knows(vesna, shed_sequence)
    ```
    `can_halt(vesna)` is *derived* — nothing asserts it, and `::assert{can_halt(…)}` would be
    `E-DERIVED-WRITE` (T4). Seed plus one authored assertion plus one Horn clause, and the
    payoff line fires. On the `pocketIt` arm it does not. This is the relational layer doing
    exactly what it is for.
    **Seven facts — and an earlier draft of this entry printed six.** It dropped
    `knows(vesna, manifest)`, which `archive.lute:21` asserts **unconditionally**, above the
    `<match>` and above the branch, so it is on every walk of this document whatever the arm or
    the seed. The block as printed was therefore not reproducible. The conclusion is unaffected:
    the missing fact is an authored `::assert`, and the claim being made is about the derived
    head.
  - `purser.lute`'s `haltTheSequence` lever consequently opens on **two** independent routes —
    `holds(can_halt(toma)) || holds(can_halt(vesna))` — one earned **seven** episodes upstream in
    the cryobank (ep02) and one earned **one** episode upstream in the archive (ep08), and each
    is voiced by whoever earned it (`when="holds(can_halt(toma))"` on Toma's line,
    `when="holds(can_halt(vesna))"` on Vesna's). Both drive correctly.
- **No verdict on `facts:` — it worked.** The verdict-bearing half is the omission, and it is
  attributable to T1's schema rather than to any tool. Recorded here because the *measurement* is
  the six-scene latency: a declared relation, a declared rule, a declared entity, and one of the
  four members of that entity silently outside the rule's reach for the entire authoring of the
  work — while the same toolchain rejects `awake(vensa)` with an `E-FACT-DOMAIN` naming the
  entity kind and the argument index (T8.11 — it is *not* column-exact; it reports the
  directive's start column). Lute is emphatic about names that do
  not exist and silent about facts that cannot happen. A `W-FACT-UNREACHABLE` — *"no route
  asserts or seeds `awake(vesna)`, so `can_halt(vesna)` cannot hold"* — is computable from
  exactly the producibility walk T4.2 proved is already project-wide and already closed over the
  rule set; it would need ground-fact granularity rather than relation granularity, which is the
  one thing T4.3 identified as the boundary. This entry is the argument for moving that boundary.

#### T8.9 — `run` derives the Datalog layer and `trace` does not, so the author's preview tool cannot walk the scene the fact layer exists for — recurrence of T4.4, with the divergence now demonstrable

T4.4 established that `lute trace` cannot verify a derived gate and must be handed the head fact
as a mock, which `W-TRACE-MOCK-UNPRODUCIBLE` then correctly calls meaningless. T8 is the first
task able to put the two walkers side by side on the same document, because it is the first task
with a `facts:` seed and a derived relation that actually fires.

- **The same document, the same project, the same rule.** `archive.lute` asserts
  `knows(vesna, shed_sequence)`; the schema seeds `awake(vesna)`; the rule is
  `can_halt(C) :- awake(C), knows(C, shed_sequence)`.
  - `lute run` on the compiled artifact: `can_halt(vesna)` appears in the final fact block and
    the guarded line plays.
  - `lute trace` on the source: `trace incomplete: 1 unresolved atom (exit 3) — unresolved:
    match (holds(can_halt(vesna))) … supply --fact "can_halt(vesna)" as a mock`.
  Base relations are closed-world in `trace` once anything produces them — seeding only
  `awake(ottavio)` correctly decides `holds(awake(toma))` false and `count(awake(_)) >= 3` false
  — so the three-valued behaviour is confined precisely to the derived layer, which `trace` does
  not evaluate at all.
- **And the schema seed is the other half of the divergence.** `trace` prints, honestly and
  unconditionally:
  ```
  note: the schema declares seed facts (e.g. `awake`) but trace does not auto-load them
  (§3.1, the explicit-world model) — supply seeded relations explicitly via --fact
  ```
  `run` auto-loads them. Both behaviours are defensible in isolation and both are documented;
  together they mean the preview tool and the reference runtime disagree about the contents of
  the world before either has executed a line.
- **The concrete cost, and it lands on the hardest scene.** `purser.lute`'s empty-room route —
  `wakeNobody`, no archive — is the route where *nothing* is derivable, and it is therefore the
  route `trace` cannot walk: the guard
  `!holds(can_halt(toma)) && !holds(can_halt(vesna)) && !holds(awake(ilsabet))` halts the walk
  on two unresolved atoms, and there is **no way to seed a negative** — `--fact "!can_halt(toma)"`
  is `E-TRACE-MOCK-FACT: does not parse as a ground fact pattern`, and none of the five mock
  surfaces (`state`/`facts`/`choose`/`events`/`accepts`) admits absence. So the branch with the
  fewest facts is the one the author's preview tool is least able to show, and the only way to
  see it is to compile and use `run` — which is the walker that skips choice guards (T8.5).
- **Recurrence of T4.4**, not re-counted in T8's tally. New evidence: the divergence is now
  *demonstrated* rather than inferred, in both directions (derivation, seed loading), and its
  cost is located — it is the negative-guard route, which is exactly the route a branching work
  most needs to preview, because it is the one with the least content and the most ways to be
  accidentally empty.

#### T8.10 — `lute scenario` on the complete eleven-scene graph: correct, useful, and unable to answer three of the four questions an author brings to it — WORKED WELL, with the boundary stated

The assignment asked whether `scenario` tells an author anything they need, now that a real graph
exists, and whether T5's claim that the structural questions are *unaskable in principle* survives
contact with one. It survives, and the reason is T8.3 rather than `::end`.

- **What it printed**, on 13 documents, first run, no probing:
  ```
  topological layers:
    layer 0: scene(anseo.s01ep01)
    layer 1: scene(anseo.s01ep02)
    layer 2: scene(anseo.s01ep03)
    layer 3: scene(anseo.s01ep04), scene(anseo.s01ep05)
    layer 4: scene(anseo.s01ep06)
    layer 5: scene(anseo.s01ep07)
    layer 6: scene(anseo.s01ep08), scene(anseo.s01ep11)
    layer 7: scene(anseo.s01ep09)
    layer 8: scene(anseo.s01ep10)
  edges (prerequisite -> dependent) [atom kind(s)]:
    scene(anseo.s01ep01) -> scene(anseo.s01ep02) [visited]
    scene(anseo.s01ep02) -> scene(anseo.s01ep03) [visited]
    scene(anseo.s01ep03) -> scene(anseo.s01ep04) [visited]
    scene(anseo.s01ep03) -> scene(anseo.s01ep05) [visited]
    scene(anseo.s01ep04) -> scene(anseo.s01ep06) [visited]
    scene(anseo.s01ep05) -> scene(anseo.s01ep06) [visited]
    scene(anseo.s01ep06) -> scene(anseo.s01ep07) [visited]
    scene(anseo.s01ep07) -> scene(anseo.s01ep08) [visited]
    scene(anseo.s01ep07) -> scene(anseo.s01ep09) [visited]
    scene(anseo.s01ep07) -> scene(anseo.s01ep11) [visited]
    scene(anseo.s01ep08) -> scene(anseo.s01ep09) [visited]
    scene(anseo.s01ep09) -> scene(anseo.s01ep10) [visited]
  ```
  Eleven scenes, nine layers, twelve edges, `wake` alone in layer 0, both terminals present as
  dependents. Every `after:` I wrote resolved the way I meant it, including the two disjunctions
  (ep06 from either of ep04/ep05, ep09 from either of ep07/ep08), each correctly yielding **two**
  edges rather than one. The two repointings took effect immediately and moved both terminals to
  the far end of the graph. Nothing needed a second attempt. **This is the surface that most
  helped me, and it is the one I trusted.**
- **Question 1 — is every scene reachable?** *Answerable, and answered.* `scenario reach` on all
  eleven: `Reachable — a satisfiable route exists under your declared routes`, each with its
  `after` structure and the reachability of every node it names, under a header that explicitly
  warns the referenced-node list is *not* a flat requirement list. That caveat is a nice piece of
  writing: it is the exact misreading a disjunction invites.
  The honest qualification is that in a connected DAG rooted at one entry point this is true by
  construction — the only way to be `Unreachable` is to name a prerequisite that is itself
  unsatisfiable. It is a real check and it caught nothing, because there was nothing to catch.
- **Question 2 — can any route strand the player?** *Not answerable, and not well-formed.*
  Stranding presumes routes. `after:` is a monotone availability lattice (T8.3): once a scene
  unlocks it never re-locks, so from every node in this graph every downstream node stays
  available forever. There is no state in which a player has nowhere to go, and equally no state
  in which the graph constrains where they go. T5.5 reached this conclusion from `::end`'s
  semantics; with a real graph in front of me the deeper reason is visible, and it is the
  prerequisite grammar rather than the terminator.
- **Question 3 — do both endings remain reachable?** *Answerable only as "are both nodes
  reachable", which is a weaker question and is yes.* `reach anseo.s01ep10` and
  `reach anseo.s01ep11` both report `Reachable`. But the graph has no notion of an ending, so
  this says exactly what it says for ep04. The JSON node record is still
  `{"id","kind","prereq","reach"}` — verified on this complete graph, ep10 and ep11 included —
  with no terminal field, exactly as T5.5 found on the provisional two-scene version. The fact
  that ep10 and ep11 are leaves is an artefact of nobody declaring `after:` them.
- **Question 4 — how many distinct routes does this work have?** *Not asked by any tool, and the
  one I most wanted.* With eleven scenes, three choice points feeding relational facts, and two
  disjunctive joins, the number of materially different playthroughs is the thing an author plans
  a production budget against. `scenario` reports layers and edges; the route count is not
  derivable from them, because — again T8.3 — the graph does not know that ep04 and ep05 are
  alternatives rather than two things you do in sequence.
- **A small correct behaviour worth recording.** `quests/hold-the-spine.lute` declares no `after`,
  and it is correctly absent from the graph while remaining addressable:
  `scenario reach quest:holdTheSpine` → *"a plain quest with no declared `after` prerequisite,
  reachable by default quest lifecycle"*. `scene-graph.md` documents exactly this, and the tool
  does exactly that. It is the kind of edge case that is usually wrong.
- **Verdict** — worked well, as far as its stated remit goes, and the remit is narrower than the
  name. `lute scenario` is a **prerequisite-graph viewer**, and as one it is accurate, fast,
  deterministic and readable at eleven nodes. It is not a scenario viewer in the sense an author
  means, and the gap between those two readings is T8.3.

#### T8.11 — what eleven scenes cost to hold in one head, itemised

The assignment asks for the single most transferable measurement in the drive test, so this entry
is a list rather than an argument: everything I had to track by hand while writing three scenes
into an eight-scene work, and everything I did not.

**Checked for me, and it carried real weight:**

| thing | what caught it |
|---|---|
| every `emotion`, `action`, `anchor`, `mood`, `volume`, `musicAction`, `vfxType` value | `E-BAD-ENUM`, with the legal set enumerated |
| every relation's arity and argument entity kind | `E-RELATION-ARITY`, `E-FACT-DOMAIN` — the latter naming the entity kind *and* the argument index. **Neither is column-exact:** both report the directive's start (probed at `12:1` unindented and `13:6` on the same directive indented five spaces) |
| every state path I read or wrote | `E-UNDECLARED` |
| every guard's function surface | `E-CEL-PROFILE`, which lists the whole admissible set |
| whether a `<match>` covered its subject | `E-NONEXHAUSTIVE` |
| whether the scene graph was acyclic and its ids resolved | `check-project` + `scenario` |
| `lineId`/`voiceKey` uniqueness | identity templates, and `loc report`'s untagged column |

That is a substantial list and it is why three scenes and 1,494 words drew only the diagnostics
recorded above. Nothing in this entry should be read as saying the toolchain is unhelpful.

**Held in my head, with no tool that could have held it:**

1. **Who is awake on which route.** Four crew, one `<choice>` in ep02 that wakes at most one of
   them, an unconditional `::assert{awake(ottavio)}` in ep06, and now a `facts:` seed. All
   **nine** of `purser.lute`'s guard expressions depend on knowing this, and the only
   representation of it anywhere is the six documents themselves. I built the table by hand and
   checked it by driving four `lute run` routes.

   **The counting rule, stated once here and used everywhere in T8, because an earlier draft
   said "twelve" in three places and twelve is not any of these numbers.** Counted from
   `purser.lute` source:
   - **9 guard expressions** — 3 `<when test=>` (lines 36, 45, 49) plus 6 `when=` attributes
     (59 on a content line, 62/70/81 on `<choice>`es, 64/65 on content lines inside an arm).
   - **11 guarded constructs** — the 9 above plus the 2 `<match on="true">` dummy subjects,
     which occupy a guard position the scene does not read (T7.6).
   - **14 relational reads** — 13 `holds(…)` plus 1 `count(awake(_))`, because one expression can
     read three facts (line 59 reads `can_halt(toma)`, `can_halt(vesna)` and `awake(ilsabet)`).

   Reproduce: `grep -c 'when test=' purser.lute` → 3; `grep -o 'when="[^"]*"' … | wc -l` → 6;
   `grep -c '<match on="true">' …` → 2; `grep -o 'holds(' … | wc -l` → 13. (`grep -o 'count('`
   returns 2 — one of them is inside a comment.) **"Nine" is the number T8 uses**, because the
   guard expression is the unit an author writes and the unit a diagnostic points at.
2. **The reachable range of two counters** — five documents, one cross-file guard exclusion, and
   a wrong answer already in this log (T8.4).
3. **Which facts are asserted where, and therefore which guards mean what.** `spine-b`'s
   convergence turns on `knows(vesna, manifest)` being asserted in exactly one upstream place
   (T8.1). Nothing lists the assertion sites of a relation. `grep` did.
4. **That ep04 and ep05 are alternatives.** Stated in no file, checkable by no tool, and false
   under the graph's actual semantics (T8.3).
5. **Which anchor is free in each scene.** Three characters, three anchors, and the middle one
   warns (T8.12). Bookkeeping, per scene, by hand.
6. **Speaker names.** This is the one that surprised me, so it gets its own paragraph.

**Speaker identity is unchecked, and it sits one line away from the strictest check in the
language.** Two lines in one file:
```lute
@vensa{code="0010" emotion="level"}: A misspelling of a declared crew member.
::assert{awake(vensa)}
```
```
probe-speaker.lute:14:1: error [E-FACT-DOMAIN] `vensa` is not a declared member of entity kind
`crew` (relation `awake` argument 0, dsl 0.3.0 §3.1)
```
One diagnostic, and it is for the `::assert`. Delete that line and the file is `ok`, 0 warnings —
and `vensa` becomes a speaker with a real identity in every downstream surface:
```json
{"speaker":"vensa","lineId":"anseo.s01ep96.vensa_0010"}
```
```
speaker    lines   words
…
vensa          1       7
vesna         27     242
```
Alphabetically adjacent in `loc report`'s own table, which is the one place it is visible and the
last place anyone looks. In an eleven-scene work with 170 lines this is the continuity error I
would actually make, and the only reason I have not is that I typed six names a few hundred
times. A project that declares an entity kind whose members are its cast has told the toolchain
the cast list; nothing joins the speaker namespace to it. This is not a new verdict — it is
T7.15's *"nothing gates stage presence"* one level lower down, at identity rather than presence —
but it is worth stating in its own words because the remedy is different and smaller: an optional
`cast:` binding in the schema (*"speaker ids are members of entity kind `crew`, plus these
non-diegetic ids"*) would make `@vensa` an `E-SPEAKER-DOMAIN` for the same reason `awake(vensa)`
is an `E-FACT-DOMAIN`, and the checker already owns both halves.

**The summary measurement.** Of six things I had to track manually, **five are relationships
between documents** — who is awake, what a counter can be, where a fact is asserted, which scenes
are alternatives, who is on stage — and the sixth is a namespace the project has already declared
elsewhere. Lute checks *within* a document superbly and checks *declarations* across documents
superbly. What it does not model is the accumulated state of a fiction at a point in its graph,
which is the entire content of an author's working memory on scene eleven.

#### T8.12 — recurrences, each with what the eleventh scene added

No new verdicts here; these are prior entries re-hit by a different author on different content,
recorded because "does this bite twice" is a maturity question and four of these bit again.

- **T5.8 (`W-INJECT-CONFLICT` on the declared default anchor) — hit three times in three scenes,
  and the shape is now clear.** Anseo's `anchor` domain is
  `{ members: [port, center, starboard], default: center }`, and every scene that stages a
  **third** character has to put them in the middle. `spine-b` (Vesna port, Ottavio starboard,
  Toma arriving between them), `archive` (Ilsabet at her drawer), `purser` (whoever is awake,
  entering). Three scenes, three collisions with a domain decision made in Task 1.
  I applied T5.8's own criterion — *is the position the point?* — and got two different answers,
  which is why the corpus gains exactly one new warning rather than three: in `spine-b` Toma's
  place between the two of them at the coupling he saved is the staging, so the attribute stays
  and the warning with it; in `archive` and `purser` the entrance is ordinary, so the attribute
  is omitted and `auto-anchor-on-show` injects the same value silently. **That is the measurement
  T5 could not make with two scenes: the tax is not per-scene, it is per-scene-where-the-middle-
  is-meaningful, and here that was one in three.** The corpus now carries two deliberate
  `W-INJECT-CONFLICT`s — `bridge.lute:11` (T5.8's original evidence, left untouched by
  instruction) and `spine-b.lute:44` — and **both are intentional, not unfinished edits.**
- **T7.2 (`E-CLIP-OVERLAP` at a boundary hand-off) — still live, and I missed it by luck.**
  `spine-b` carries the corpus's **second** `<timeline>` — `spine-a` has the other, and
  `grep -rn '<timeline'` over the project finds no third — **seven** clips over four tracks with
  **three** boundary hand-offs: `sfx` `0.0 + 0.5 → 0.5`, `camera` `0.0 + 0.5 → 0.5`, and `vfx`
  `0.5 + 0.5 → 1.0`. All three pass. I chose those numbers because they are round, not because
  they are exactly representable in binary, and that is the whole of why the timeline checked
  clean on the first attempt. Substituting the
  values T7.2 documents — `at="0.8" duration="0.4"` then `at="1.2"` — into the same track of the
  same file reproduces it exactly:
  ```
  spine-b.lute:65:5: error [E-CLIP-OVERLAP] clip at 1.2 overlaps another clip in track `#vfx`
  ```
  So the defect is unchanged, and the second author to write a timeline in this project escaped
  it by picking halves. That is a worse property than a uniform failure, and it is T7.2's own
  argument arriving as evidence: the trap is invisible until you happen to type a tenth.
- **T7.6 (a relation cannot be a `<match on>` subject) — five more dummy subjects.** `spine-b`
  two, `archive` one, `purser` two. Every relational block in this task opens
  `<match on="true">`. T7 shipped one and called it a curiosity; at five in three scenes it is
  the standard idiom of the corpus, which is a different thing to be. Each carries or shares a
  comment, because without one it reads as a mistake.
- **T7.12 (retyped frontmatter) — 18 more boilerplate lines.** Three new scenes, each restating
  `kind: scene`, `character: anseo`, `season: 1`, and the same two-schema `uses:`. The corpus's
  eleven scenes now carry **66** frontmatter key lines of which **41 are byte-identical
  duplicates**. T7.12's own counting rule — total lines minus *distinct* lines — and it is
  restated here because the ratio was previously quoted without it. 66 total, **25 distinct**:
  `kind: scene`, `character: anseo`, `season: 1`, one `components:`, two `uses:` spellings
  (`wake` imports one schema, the other ten import two), 11 distinct `episode:` lines, and 8
  distinct `after:` formulas — `visited("anseo.s01ep03")` is shared by `hydroponics` and
  `machine-deck`, `visited("anseo.s01ep07")` by `archive` and `shed`. 66 − 25 = 41. The same rule
  reproduces T7.12's 48 − 18 = 30 exactly. **So it did not scale linearly:** 41/66 (62.1%)
  against 30/48 (62.5%), and the shortfall is the four extra distinct `after:` formulas an
  eleven-node graph needs. Boilerplate share is asymptotically flat rather than growing — a
  weaker claim than "44/66, exactly linear", and T7.12's complaint survives it unchanged: three
  of the six lines at the top of every file are still noise.
- **T7.14 (the stale `lute-lsp` on `PATH`) — unchanged and still loud.** Every editor diagnostic
  in this session came from the July binary. On `purser.lute` — `ok (0 warnings)` under
  `check-project` — it reports **58 errors**, including `E-CEL-PROFILE` on every `holds(…)` and
  `count(…)` guard (its profile list stops at `isSet()`), `E-BAD-ENUM` on `port`/`starboard`
  against an anchor domain of `left, center, right` this project does not declare,
  `E-UNKNOWN-DIRECTIVE` on `::assert`, and `E-UNCLASSIFIED` on most content lines. It is worth
  restating T7.14's point with this number attached: the surface an author looks at most reported
  58 confident errors on a file the checker calls clean, and nothing in the toolchain notices.

#### T8.13 — three scenes, four routes, and the parts that carried the weight — WORKED WELL

2,189 words and 170 lines across 13 documents now, of which this task wrote 1,494 words, 109
lines and 9 choices — more than doubling the corpus. (`lute loc report docs/examples/anseo`,
re-run after the correction pass, which added two lines to `archive.lute` to close a dropped
thread; as first committed the figures were 2,136/168 and 1,441/107.) The friction is above;
this is what did not
get in the way.

- **The relational layer is the best thing in this language, and `purser.lute` is the proof.**
  **Nine** guard expressions — T8.11's counting rule; 11 guarded constructs, 14 relational reads
  — over **all four** declared relations: `awake`, `knows`, `found`, and the derived `can_halt`,
  which **is one of the four** rather than a fifth head beside them. They read facts asserted in
  four different documents — `cryobank` (ep02), `stowaway` (ep06), `archive` (ep08) and
  `world.schema.yaml`'s seed — up to seven episodes upstream, and **every one of them was correct
  on first write.** No probing, no diagnostics, no reshaping. Driven through `lute run` on four
  routes — Toma woken, Ilsabet woken, nobody woken, and nobody-woken-plus-archive — the scene
  produces four materially different confrontations:
  - *Toma*: `can_halt(toma)` derives from the ep02 choice; the halt lever opens and Toma voices
    it; `count(awake(_)) >= 3` fires.
  - *Ilsabet*: the heading lever opens; the Purser recomputes and withdraws the release.
  - *Nobody*: both levers closed, the long negated conjunction
    `!holds(can_halt(toma)) && !holds(can_halt(vesna)) && !holds(awake(ilsabet))` fires, the
    two-person arms play, and the only remaining lever is Ottavio's — which makes the shed
    *worse*, because being counted costs a module. The empty-room route is the best scene of the
    four, and it exists because the guards made me write it.
  - *Archive*: `can_halt(vesna)` derives from a `facts:` seed plus one `::assert` in the previous
    episode, and Vesna voices the same lever in her own words.
  Nothing about this is expressible with flags. A boolean per crew member per fact would be
  sixteen paths and no `can_halt` at all.
- **`after:` disjunction, again, and the repointing was genuinely one line each.** `purser.lute`
  landed `after: 'visited("anseo.s01ep07") || visited("anseo.s01ep08")'` first try, two edges,
  correct layer. Repointing the two terminals was exactly the two frontmatter lines the brief
  said it would be, and `scenario` reflected both immediately with no other edit anywhere. For a
  graph with no centralised manifest, an eleven-node rewire costing two lines is the design
  paying off.
- **`facts:` (T8.8) — first use in the corpus, worked immediately, and `run` derives over it.**
- **The diagnostics I hit were, again, mostly excellent.** `E-CEL-PROFILE` enumerates the entire
  admissible surface, which is the one thing you want when a guard is rejected.
  `E-CONN-PROFILE` names the exclusions explicitly ("no negation, arithmetic, comparisons").
  `E-TRACE-CHOICE` names the branch, the choice, the reason and the clause. `E-FACT-DOMAIN`
  gives the entity kind *and* the argument index. `E-UNKNOWN-ATTR` on `::auto{when=}` is
  instant and unambiguous.
  **Three scenes' first drafts drew six diagnostics in total**: two `E-CEL-PROFILE` (T8.1, the
  real finding), one `E-UNKNOWN-ATTR` (T8.7), one `E-LOGIC-CONTENT` from a probe, and two
  `W-INJECT-CONFLICT`. All six were true. The three `goto=`s that should have been a seventh,
  eighth and ninth were the silence in T8.2.
- **`lute run` as a verification surface, its one hole aside.** Four routes through `purser`,
  three through `spine-b`, two through `archive` — nine walks, each printing the state, the
  derived fact set and every decision, all nine matching what I intended before I ran them. The
  `-- facts --` block at the end of a run is the single most useful continuity instrument in the
  toolchain, and it is the closest thing that exists to answering T8.11's question. It answers it
  one route at a time, after the fact, on a compiled artifact — but it answers it.

#### T8 summary

Thirteen entries. Audited heading-by-heading against each entry's own verdict line rather than
carried forward from a running count, because a miscounted tally is how T7 nearly shipped two
wrong claims:

| entry | disposition |
|---|---|
| T8.1 — no way to ask how the player arrived | `LANGUAGE-GAP` (shape b) |
| T8.2 — unknown attributes on logic tags silently discarded | `TOOL-DEFECT` (silence) |
| T8.3 — branch and graph are disjoint; no choice routes | `LANGUAGE-GAP` (both shapes) |
| T8.4 — envelope constant across the graph; hand arithmetic wrong in this log | `ERGONOMIC` |
| T8.5 — `run` plays a guard-false choice; `trace` refuses it | `TOOL-DEFECT` |
| T8.6 — runtime dispatcher reads two fields the artifact lacks | `DOC-WRONG` |
| T8.7 — staging cannot be gated; one `::auto` costs a block | `ERGONOMIC` |
| T8.8 — `awake(vesna)` never asserted; `facts:` is the fix | Task 1 schema defect; no tool verdict |
| T8.9 — `run` derives, `trace` does not | recurrence of T4.4 |
| T8.10 — `scenario` on the complete graph | worked well, boundary stated |
| T8.11 — what eleven scenes cost to hold in one head | measurement |
| T8.12 — recurrences (T5.8 ×3, T7.2, T7.6 ×5, T7.12, T7.14) | recurrences |
| T8.13 — what carried the weight | worked well |

Seven of the thirteen entries are verdict-bearing, and each carries **exactly one** of the seven:
T8.1 and T8.3 `LANGUAGE-GAP`, T8.2 and T8.5 `TOOL-DEFECT`, T8.4 and T8.7 `ERGONOMIC`, T8.6
`DOC-WRONG`. The other six carry none by the protocol's own provisions rather than by omission —
T8.10 and T8.13 are *what worked well*, T8.11 is a measurement, T8.9 is a recurrence of T4.4 and
T8.12 a recurrence bundle (both explicitly not re-counted, on T7.11's precedent), and T8.8's
defect is attributable to Task 1's schema while `facts:`, the fix, carries no verdict because it
worked. Re-audited heading by heading after the T8.5 rescope, and **no verdict class moved**: the
rescope narrows T8.5's blast radius, not its category, because `run --help` still advertises a
clause `run` does not implement. No `DOC-GAP` and no `AUTHOR-ERROR` — which now holds for
four consecutive tasks, and is worth saying plainly: **the website docs answered every question I
took to them, and I did not open a Rust file to author anything in this task.**

**The convergence answer, since the assignment asked for a design claim either way.** Lute
distinguishes *what is true now* and has no representation whatsoever of *how you got here*
(T8.1). "This line on either route" is free — it is what convergence means, and you write it by
writing nothing. "This line on one route" has no spelling: `visited()` is scoped to the `after:`
slot and is `E-CEL-PROFILE` in a guard, and the only discriminator available in this corpus is
`holds(knows(vesna, manifest))`, a content fact that distinguishes the two arrivals by accident
of where a different scene happened to assert it. That proxy is one `::assert` from inverting,
and the scene that would invert it is the one I wrote next. The design is coherent — a scene
whose behaviour is a function of state and not of history is exactly what makes `envelope`
computable — but it means the language's answer to the single most common structural moment in
branching fiction is *mint a marker and check it yourself*.

**And the graph is not the story's graph, which is the larger version of the same thing.** `after:`
is a monotone availability lattice: no negation, no state reads, no exclusion (T8.3). So ep04 and
ep05, described as a fork through three tasks of this log, are not a fork — nothing prevents
visiting both, and if a player does, every convergence guard in `spine-b` reports the wrong
arrival. A `<choice>` cannot name a successor (`goto=` is eaten silently, T8.2), and the
successors cannot read what a choice wrote, so **no decision a player makes can determine which
scene comes next** except by laundering state through a quest document — which buys availability
and still cannot buy alternation. Everything T5.5 found unaskable is unaskable for this reason
rather than for `::end`'s: "can a route strand the player" presumes routes, and Lute models
availability. That is a defensible design and it is nowhere stated as one; an author reading
`lute scenario`'s output sees something that looks exactly like a story graph.

**The two cheapest fixes in this section are also the two most valuable, and neither is a language
change.** T8.2: raise `E-UNKNOWN-ATTR` on `<branch>`/`<choice>`/`<match>`/`<when>`/`<hub>` against the
§7.3 permitted-attribute table that already exists and that `<otherwise>` and `E-PERSIST-REMOVED`
are already enforced from. It costs nothing, it would have caught three routing declarations and
both spellings of a documented migration hazard, and the current behaviour means the checker
silently eats `as="run.vesnaTrust" value="1"` — a state write — while emitting a bespoke,
helpful, column-exact error for its sibling `persist` three lines away. T8.5: make `lute run`
evaluate an option's `when` before honouring a mocked selection, as its own runtime contract
specifies and as `trace` already does with a purpose-built diagnostic. **Scope, corrected:**
this is not a hole under every mock suite in the language. `lute test` is `trace`-based — its
`--help` says it *"traces"* each `*.test.yaml` against the declared mocks — and a matched pair on
`purser.lute` confirms it refuses the guard-false selection (`FAIL … trace refused: invalid mock
input`, exit 1) while passing the guard-true control on the identical seeds. The vacuous
verification is specifically hand-driven `lute run` against a compiled artifact; `lute test` and
`lute trace` enforce the guard, and **Task 9's scenario tests are `lute test`, and are therefore
sound.**

**The most transferable measurement is T8.11, and it is one sentence.** Of the six things I had
to hold in my head across eleven scenes, five are relationships *between* documents — who is
awake, what a counter can be, where a fact is asserted, which scenes are alternatives, who is on
stage — and the sixth, the cast list, is a namespace the project has already declared and that
nothing joins to the speaker slot. Lute checks within a document superbly, checks declarations
across documents superbly, and models the accumulated state of a fiction at a point in its graph
not at all. The `-- facts --` block `lute run` prints at the end of a walk is the closest thing
that exists, and it works one route at a time, after compilation, for a single document.

**What T8 would fix first.** T8.2, because it is free and because silence is the failure mode
this log has spent eight tasks establishing is the expensive one. Then T8.5, for the same reason
one layer down. Then T8.4's `envelope --values`, because the alternative is hand arithmetic and
the hand arithmetic in this log is already wrong once, which cost a dead line *written into a
scene* — caught and corrected to `>= 2` before the commit, as T8.4's own body records, so nothing
dead shipped — and it was caught only because the eleventh scene happened to need the number.
T8.1 and T8.3 are the two entries asking for design rather than repair, and of the two, T8.1's
minimal form is nearly free: admit `visited("id")` as a read-only query in a content guard, over
the visited set the engine already maintains for `after:`. It adds no state, no vocabulary and no
analysis, and it would let a convergence scene say the one thing convergence scenes exist to say.

### T9 — The quests and the tests

Toolchain 0.9.0 / language 0.9.0 / IR 0.9.0, `./target/debug/lute`. Five quest
documents added (`unmoored`, `who-wakes`, `false-heading`, `manifest-gap`,
`what-vesna-carries`) and **31** `*.test.yaml` scenario tests, the project's first.
Final state: 18 `.lute` documents, `check-project docs/examples/anseo` ok with 13
project-wide warnings, `lute test docs/examples/anseo` 31 passed / 0 failed.

This is the first task to write tests, so most of what follows measures `lute test`
rather than the language. That is the right place to spend a last task: a branching
work with two endings is exactly the thing whose correctness cannot be read off the
source, and a verification story either exists or it does not.

#### T9.1 — a quest's `after` is an attribute, not a frontmatter key, and the page that owns `after:` says the opposite — DOC-GAP

- **Intent** — give each of the five quests the `after=` prerequisite the T4
  controller decision requires, so each lands as a node in `lute scenario`.
- **Attempt** — the form the decision's own prose and the scene documents both
  suggest, written into the quest frontmatter beside `uses:` and `title:`:
  ```lute
  ---
  kind: quest
  luteVersion: "0.9.0"
  uses: ../world.schema.yaml
  title: Probe
  after: 'visited("anseo.s01ep03")'
  ---
  ```
- **Result** — exit 1:
  ```
  quests/probe.lute:1:1: error [E-META-UNKNOWN-KEY] unknown top-level meta key `after` (not a core key and not owned by an active plugin)
  ```
  No did-you-mean, and there are two available: `after` **is** a core frontmatter
  key on the sibling document kind, and it **is** a legal attribute two lines below
  in the same file. The span is the whole meta block (`1:1`), not the offending key.
- **Resolution** — `after=` on the `<quest>` element:
  ```lute
  <quest id="whoWakes" title="Who Wakes" start="true" after="visited('anseo.s01ep01')">
  ```
  Checks clean, and produces the node and edge (T9.2). Found by experiment in a
  scratch copy, not from documentation.
- **Why the docs did not get me there.** `language/quests-and-scenes.md` is the page
  that owns both constructs, and its heading is *"Scenes and `after:`"*: "Scenes are
  *sequenced* with the frontmatter key **`after:`**, which declares the routes the
  scene may be entered from." Its quest half never mentions `after` at all. So the
  one page covering both document kinds states the scene spelling as *the*
  spelling, and an author who reads it carefully — as I did — writes the wrong
  thing. The tool that recommends the attribute does not say where it goes either:
  `scenario envelope` closes a defaults-only quest table with "declaring `after`
  would enrich this table", naming the key and not the slot.
- **Verdict** — `DOC-GAP`. I did not have to open Rust, but I did have to discover a
  construct by probing, which is the same failure the bar exists to catch: the
  website and `lute context` between them do not tell you that a quest can carry
  `after`, or how. The fix is two sentences on an existing page and one did-you-mean
  on an existing diagnostic.

#### T9.2 — five quests, five layers, and two quest-to-quest edges — WORKED WELL

- **Intent** — the controller decision's stated payoff: `after=` buys the
  reachability surface. Test whether it holds at five, and whether the graph can
  express "this quest only matters once that one has happened".
- **Attempt** — each quest's prerequisite chosen where its subject matter begins,
  not uniformly: the cryobank question is posed in the last line of the cold wake,
  the navigator names the false heading in episode 2, the ship is measurably
  shorter after episode 3's release, Vesna first prices an answer in episode 4, and
  Ottavio does not exist until episode 6. Two carry a second conjunct naming
  another quest.
- **Result** — every one lands, on five distinct layers, each with a real edge:
  ```
    layer 1: scene(anseo.s01ep02), quest(whoWakes)
    layer 2: scene(anseo.s01ep03), quest(falseHeading)
    layer 3: scene(anseo.s01ep04), scene(anseo.s01ep05), quest(unmoored)
    layer 4: scene(anseo.s01ep06), quest(whatVesnaCarries)
    layer 5: scene(anseo.s01ep07), quest(manifestGap)
    …
    quest(whoWakes) -> quest(falseHeading) [active]
    quest(whoWakes) -> quest(manifestGap) [completed]
  ```
  The edge kind is carried through to the label, so the two quest-to-quest edges
  read differently and correctly: `falseHeading` needs `whoWakes` *live*,
  `manifestGap` needs it *finished*. That answers the assignment's question
  directly — **yes, one quest's completion can gate another's start**, and it is
  visible in the graph rather than buried in a predicate.
- **Verdict** — worked well. This is the single cheapest thing in the whole
  exercise: one attribute per file, and the six-quest, eleven-scene project has a
  reachability graph an author can read. T4's decision to leave `hold-the-spine`
  without one is the right control and the contrast is stark — that quest is a
  full participant in `check-project` and simply absent from the picture above.

#### T9.3 — `after=` speaks the route graph; `done=` cannot say a word of it — LANGUAGE-GAP

- **Intent** — `unmoored` is the frame quest. The obvious objective is the one the
  whole prologue is about: **get to the bridge.** Write it.
- **Attempt** —
  ```lute
  <objective id="arrive" title="Arrive at the bridge" done="visited('anseo.s01ep10')"/>
  ```
- **Result** —
  ```
  quests/probe.lute:10:59: error [E-CEL-PROFILE] `visited(…)` is outside the Lute-CEL profile — only operators, literals, lists, `?:`, `in`, `has()`, `isSet()`, `holds()`, `count()`, `validAt()`, and `now()` are permitted (dsl §8.4, 0.3.0 §8)
  ```
  A good diagnostic — it enumerates the whole permitted set — attached to a
  restriction that is very hard to defend **in this file**. Nine lines above the
  error, the same document says `after="visited('anseo.s01ep10')"` and it compiles.
  One parser, one document, two slots, and the predicate vocabulary is disjoint:
  `after=` admits `visited`/`completed`/`active` and nothing else (no negation, no
  arithmetic, no state); `done=` admits everything else and none of those three.
- **Resolution** — every one of the 15 objectives across the five quests is a
  proxy over declared state or the fact database. `unmoored`'s arrival objective
  became `count(can_halt(_)) >= 1` — "somebody aboard can stop the shed" — which is
  a *good* objective and is not the one I wrote first.
- **Verdict** — `LANGUAGE-GAP`, shape (b). The intent is reachable only by encoding
  it as something else, and nothing in the language *means* "the player got here",
  so nothing can check it. This is T8.1's gap one layer up, and the quest layer
  makes it much harder to read as a design choice: T8.1 could be explained by
  "content guards evaluate inside a scene, where arrival is trivially true". A
  quest is a whole-run object whose *other* predicate slot already reads the
  visited set the engine maintains. The minimal fix is the same one T8.1 asks for
  and it is smaller here: admit `visited("id")` in `done=`, read-only, over a set
  the same file's `after=` is already querying.

#### T9.4 — nothing is evaluated at the end of a run, so a hold-the-line objective completes before the player moves — LANGUAGE-GAP

- **Intent** — `unmoored`'s second objective, written before checking anything: *the
  ship is coming apart, and the win condition is that you did not hand the schedule
  new reasons.* In the corpus's own terms: `run.shedPressure` never got past one.
- **Attempt** —
  ```lute
  <objective id="noNewReasons" title="Give the schedule no new reasons" done="run.shedPressure < 2"/>
  ```
- **Result** — **silence.** `ok`, zero diagnostics, and completely wrong.
  `run.shedPressure` is declared `default: 0`, objectives are evaluated
  continuously, and derived completion fires as soon as every non-`optional`
  objective is `done` — so the quest activates and completes in the same tick, at
  episode 3, before the player has made a single choice that could violate it. The
  trace shows it plainly: `<quest unmoored> -> active`, three objectives `-> done`,
  `<quest unmoored> -> complete`, all before any content runs.
- **What I looked for and did not find.** A terminal evaluation point of any kind:
  an `at="end"`/`evaluateAt=` on `<objective>`, an `<on event="runComplete">`, a
  `::end`-scoped hook, a "never became true" negative objective. `::end` exists and
  is the only end-of-run construct in the language, and T5.5 already establishes it
  is a *scene* terminator with no story-level meaning; nothing binds a quest to it.
  `fail=` is the nearest thing and it is the inverse: it fires the moment the bad
  condition becomes true, which is correct behaviour and not the same statement.
- **Resolution** — the objective was rewritten in its positive form
  (`aHandAtTheCoupling`, `somebodyToWalkWith`) and the losing side moved to
  `fail="run.shedPressure >= 4"`. The story survives; the sentence I set out to
  write does not exist in the shipped file. The reasoning is recorded in the
  document's own comment so the next author does not re-derive it.
- **Verdict** — `LANGUAGE-GAP`, shape (a): I changed the story to fit the tool. The
  cost is not the one objective. It is that **every "you got through it without X"
  goal in any work is silently unwritable** — the form you reach for compiles,
  passes `check-project --deny-warnings`, and completes instantly, which is the
  most expensive failure mode this log has (T2.1, T8.2, T7.8). An author who writes
  `done="run.corpses == 0"` on a survival quest ships a quest that completes at the
  title card.

#### T9.5 — five quests at once: what the repetition actually measured — ERGONOMIC

T7.12 measured frontmatter duplication over four new scenes. The same measurement
over five new quests, recounted from the committed files.

- **Frontmatter.** **20** key lines across the five documents, **8** distinct — so
  **12** are byte-identical duplicates of a line in another file.

  | line | files carrying it verbatim |
  |---|---|
  | `kind: quest` | 5 / 5 |
  | `luteVersion: "0.9.0"` | 5 / 5 |
  | `uses: ../world.schema.yaml` | 5 / 5 |
  | `title: <the quest's title>` | 5 / 5, all different |

  Worse than T7.12's scenes proportionally: there, two of six keys carried
  per-file information; here **one of four** does. And that one is duplicated
  *within* each file — every quest states its title twice, once in frontmatter and
  once as `<quest title="…">`. Verified: deleting the frontmatter `title:` leaves
  the document `ok`, and setting the two to different strings is also `ok`. So the
  frontmatter copy is optional, unchecked against the attribute, and present in all
  five files because the scaffolded shape has it.

- **Objective predicates.** **15** objectives, **12** distinct `done=` strings. The
  repeat is `holds(knows(vesna, manifest))`, written **four** times across four
  different quests — `falseHeading.findThePaper`, `unmoored.theOrderOnPaper`,
  `manifestGap.readItYourself`, `whatVesnaCarries.sheHasThePaper`. Four quests
  reaching for the same fact is not itself a smell; four quests naming it in four
  places, with four different titles, and no way to say "the manifest has been
  read" once, is.
- **Predicate shapes.** All 15 are one of three: `holds(…)` ground query (8),
  `<scalar> >= n` (5), `count(…) >= n` (2). A world with two scalars and four
  relations does not give five goal machines much room, and the language offers
  nothing to factor the shared part: no named predicate, no macro, no shared
  objective library, no `extends:` for documents (it composes *schemas* only —
  checked, T7.12 has the same finding from the other side).
- **What the language could have offered instead.** Two things, both with existing
  precedent in this project. (1) A **named derived predicate** — the `rules:` block
  already gives exactly this for facts (`can_halt(C) :- awake(C), knows(C, shed_sequence)`),
  and `holds(can_halt(vesna))` is used as an objective in `whatVesnaCarries`. There
  is no scalar-side equivalent, so `run.vesnaTrust >= 2` cannot become
  `trusted()`. (2) A **`defaults:` block in `lute.project.yaml`**, T7.12's proposal,
  which would take the five quests' frontmatter from 20 lines to 5.
- **Verdict** — `ERGONOMIC`. Nothing is at risk and nothing is inexpressible; the
  cost is that the fifth quest reads like the first with the nouns changed, and the
  one thing a reader most needs to see — how these five goal machines differ — is
  buried under three identical lines and four identical predicates.

#### T9.6 — an objective on a scalar gets no satisfiability analysis at all, while the same slot on a relation gets a project-wide fixpoint — TOOL-DEFECT

- **Intent** — none authorial. T4.2 is this log's strongest single finding: a
  relational `done=` gate is proved dead *across documents*, naming the relation.
  With five new quests and 15 objectives I wanted to know whether the same care
  covers the scalar half, because that is the half authors write most.
- **Attempt** — one scratch quest in a copy of the committed project, three
  objectives:
  ```lute
  <quest id="deadQuest" title="Dead" start="true" after="visited('anseo.s01ep01')">
  <objective id="tooHigh" title="Beyond the corpus maximum" done="run.shedPressure >= 99"/>
  <objective id="alsoLow" title="Contradicts the one above" done="run.shedPressure <= 0"/>
  <objective id="never"   title="Literally false"           done="false"/>
  </quest>
  ```
- **Result** — exactly one diagnostic, for the literal:
  ```
  quests/dead.lute:11:1: error [E-OBJECTIVE-UNSATISFIABLE] `done` predicate `false` is provably false: the objective can never complete on any run; the objective — and, being required, the quest — can never complete (dsl 0.4 §5.3)
  ```
  `tooHigh` and `alsoLow` draw **nothing**. And they are two separate failures:
  - `run.shedPressure >= 99` is unreachable in this corpus — the maximum any route
    can produce is **4** (cryobank `wakeToma` +2, machine-deck `<otherwise>` +1,
    purser `listTheMass` +1), and every write is a literal-delta `::set` the
    checker already reads. This needs range analysis and I would not call its
    absence a defect on its own.
  - `tooHigh` **and** `alsoLow` together is a different thing entirely. Two
    required objectives on one path with disjoint solution sets: the quest can
    never complete, and deciding it needs no corpus knowledge, no route walk and no
    range propagation — just the two literals. Nothing reports it.
- **And `scenario reach` still says Reachable.** With the file failing `check-project`
  on `E-OBJECTIVE-UNSATISFIABLE`:
  ```
  reach quest(deadQuest):
    verdict: Reachable — a satisfiable route exists under your declared routes.
  ```
  Same shape as T4.5, now on a quest whose own document is a hard error rather than
  a silent one.
- **Verdict** — `TOOL-DEFECT`. The check exists, is named, cites its spec clause,
  and states its conclusion in absolute terms ("can never complete on any run") —
  and it covers constant-folded literals and a cross-document relational fixpoint
  while dropping the one case an author produces by accident. `E-OBJECTIVE-UNSATISFIABLE`'s
  own existence is the advertisement; incompleteness against it is the defect. The
  contradictory-pair case in particular is a handful of lines beside machinery that
  already does far harder work.

#### T9.7 — two evaluators disagree about derived relations, and `--coverage` is computed by the half that cannot see them — SPEC-WRONG

This is the largest single obstacle the test suite hit, and it shaped six of the
31 test files. **It is also the entry this task got most wrong, and the corrections
are recorded below rather than quietly folded in, because the retracted claims are
the sort a reader would have carried forward.**

- **Intent** — the assignment's question, and the right one for a relational work:
  *does a relational gate open only after the fact that licenses it?* Two tests per
  gate, one with the licensing fact and one without.
- **Attempt** — the negative control for `purser.lute`'s `theCorrection` branch:
  nobody awake but Vesna, nobody who can halt the shed, so `sayNothing` should be
  the only eligible arm.
  ```yaml
  file: ../scenes/purser.lute
  facts: ["awake(vesna)"]
  choose: { theCorrection: sayNothing }
  expect: { exit: complete }
  ```
- **Result** — `exit: incomplete`. The walk stops dead:
  ```
  trace incomplete: 1 unresolved atom (exit 3)
    unresolved: match `(!holds(can_halt(toma)) && !holds(can_halt(vesna)) && !holds(awake(ilsabet)))` — supply --fact "can_halt(toma)", --fact "can_halt(vesna)" as a mock
  ```
  Base relations are closed-world in the **trace** evaluator — absent means false —
  but `derive: true` relations are UNKNOWN when absent, and unknown halts the walk.
  Inside `lute trace`, and therefore inside `lute test`, `can_halt` has exactly one
  expressible value, `true`, and the hint the tool prints is the only move
  available: assert the conclusion of the project's one rule.

**The claim this entry originally made, and why it is false.** The first draft
titled this *"the false side of every rule in the project is untestable"* and closed
*"any work whose interesting guards are derived can test the yes and not the no"*.
Both are wrong, and the counter-example is one command away. `lute run` — the
reference consumer of the runtime contract — **does** run the Datalog fixpoint, so
every derived relation has a proper closed-world false there. Compile
`scenes/purser.lute` and run it with only the world seed:

```
$ lute compile --project . scenes/purser.lute -o purser.artifact.json
$ cat m1.yaml
facts: ["awake(vesna)"]
choose: { theCorrection: sayNothing }
$ lute run purser.artifact.json --mock m1.yaml
  …
  001-3600  match  -> arm 1
  001-3700  vesna: We came up here with a stowaway and a grievance. Let's find out what that buys.
  …
run complete
```

Line 0170 — the one whose guard is three negations, the exact atom `trace` could not
resolve — **executes**, because `can_halt(toma)` and `can_halt(vesna)` are absent
from the least fixpoint and therefore false. And the positive direction discriminates
too. Seed only the rule *body* for Toma:

```
$ cat m2.yaml
facts: ["awake(toma)", "knows(toma, shed_sequence)"]
choose: { theCorrection: haltTheSequence }
$ lute run purser.artifact.json --mock m2.yaml
  001-4000  choice [theCorrection] -> haltTheSequence
  001-4200  match  -> otherwise          ← 0180, `holds(can_halt(vesna))`: correctly suppressed
  001-4600  match  -> arm 1              ← 0190, `holds(can_halt(toma))`
  001-4700  toma: Maintenance hold. Two hands, both of them steady, and you have to mean it.
  …
-- facts --
  awake(toma) / awake(vesna) / can_halt(toma) / knows(toma, shed_sequence)
```

`can_halt(toma)` was **derived**, not seeded; the lever opened on it; 0190 played and
0180 was correctly suppressed. That is the whole negative control the entry claimed
was inexpressible, and it is expressible today, in a shipped tool, with no new
feature. The general claim is retracted.

**The true claim, which is sharper.** Two evaluators in one toolchain disagree about
what a derived relation is worth, and the assertion machinery is bolted to the half
that cannot see them.

| | derived relation absent from seeds | 0170 (`!can_halt(toma) && !can_halt(vesna) && !awake(ilsabet)`) |
|---|---|---|
| `lute run` (artifact, runtime contract) | **false** — least fixpoint over seeds ∪ asserts ∪ rules | executes |
| `lute trace` / `lute test` (source preview) | **unknown** — halts the walk | unresolved atom, exit 3 |

Same document, same seed, opposite outcomes. Neither tool is misimplemented: dsl
0.4.0 §4.2 rule 3 is explicit — *"`trace` MUST NOT execute engine machinery. It runs
no Datalog fixpoint (a `derive: true` relation is never computed)"* — and
`docs/runtime/cel-and-facts.md` is equally explicit that the engine, which `lute run`
stands in for, *"runs the least-fixpoint over `seedFacts` ∪ asserted facts ∪ `rules`"*.
Both do exactly what they are told.

**And the consequence is that `--coverage` is computed by the blind half.** `lute test`
walks `trace`, so every coverage number this project can produce is a statement about
an evaluator that cannot compute the one rule the work turns on. That is not a
rounding error in the report; it is three permanently red rows and one arm that can
never be exercised, all of them artefacts of the evaluator choice rather than of the
content:

1. **`purser.lute:61` is dead to the harness.** Its guard is true only when
   `can_halt(toma)`, `can_halt(vesna)` and `awake(ilsabet)` are all *false* — and
   two of those three have no false inside `trace`. `lute test --coverage` reports
   it and it cannot be satisfied:
   `match !holds(can_halt(toma)) && …: 1/2 arm(s) executed; 1 unexecuted`,
   permanently, and the same for `holds(can_halt(toma))` and `holds(can_halt(vesna))`.
   Recounted from the committed tree: `--coverage` renders **15** rows (5 branch/hub,
   10 match) and **exactly 3** of them are structurally incomplete. All three go green
   under `lute run`, which walks 0170 on the first mock above.
2. **A test must over-seed to trace one arm.** `haltTheSequence` carries two
   alternative lines — 0180 guarded on `can_halt(vesna)`, 0190 on `can_halt(toma)` —
   and leaving either unseeded halts the walk, so the only traceable version of that
   arm is one where both hold. **The original entry called that "a world state no
   route in the story produces" and said the passing test "documents a world that
   cannot happen". That is false and it is retracted.** It is the ordinary maximal
   route: `wake` → `cryobank:wakeToma` (asserts `awake(toma)`,
   `knows(toma, shed_sequence)`) → `spine-a` → `hydroponics`/`machine-deck` →
   `stowaway` → `spine-b` → `archive:readItAloud` (asserts
   `knows(vesna, shed_sequence)`) → `purser`, with `awake(vesna)` from the world seed.
   `archive.lute`'s `after:` is `visited("anseo.s01ep07")` and carries no dependency
   on which pod the cryobank opened, so nothing forces a choice between the two
   halters. `lute run` on the accumulated route facts derives **both** `can_halt(toma)`
   and `can_halt(vesna)` and plays both 0180 and 0190. The committed test documents
   the game's most-travelled route, which is the right thing for it to document.
   What is genuinely lost is narrower: the *alternation* — a world where exactly one
   of the two can halt — is real, is walked correctly by `lute run` (m2 above), and
   cannot be traced, because the unseeded half is unknown rather than false. The
   shipped test's comment carried the false sentence verbatim and has been corrected.
3. **The seed is not time-varying.** `archive-read-it-aloud.test.yaml` seeds
   `can_halt(vesna)` from line 1, including across the branch that *earns* it;
   `archive-with-navigator.test.yaml` has to seed it on a route that pockets the
   page and never learns the sequence, making line 0230 play when the story says
   it must not. Both files say so in a comment. This one is unaffected by the
   correction: `run`'s fixpoint is recomputed per delta, so it would get both right.
- **And there is no negative mock surface, which is the part that stands.** All three
  spellings an author reaches for are rejected identically:
  `--fact "!can_halt(toma)"`, `--fact "not can_halt(toma)"` and
  `--fact "can_halt(toma)=false"` each draw `E-TRACE-MOCK-FACT`. Inside `trace` a
  derived relation has exactly one expressible value.
- **Resolution** — six tests seed a `can_halt(…)` they should have derived, each
  with the reason written into the file. No content was changed: the gates are
  right, and the harness the project's `lute test` suite runs on cannot derive them.
- **Verdict** — `SPEC-WRONG`, corrected from this entry's first-draft `TOOL-DEFECT`.
  Nothing here is misimplemented and nothing here is undocumented. `trace`'s refusal
  to run rules is *normative* (dsl 0.4.0 §4.2, the D1 quarantine, restated in 0.5.0
  §3 and cited by this log twice already at T4.4 and T8.9); `run`'s fixpoint is
  normative; `lute test --help` says plainly that it "traces". Language, docs and
  both tools agree with each other. The design is the defect, and the criterion in
  the table fits exactly: *two equivalent proofs given unequal treatment.*

  **What the spec says.** D1 quarantines all runtime evaluation behind the artifact,
  so the source-preview tool is forbidden the fixpoint. That is a defensible
  boundary for `trace`, whose job is to preview *one document* before it compiles.

  **Why it is the wrong call for `lute test`.** 0.4.0 §4.2 was written when `trace`
  was the only evaluator; `lute test` inherited the quarantine by being built on
  `trace`, not by anyone deciding a *test harness* should be forbidden the engine's
  answer. A scenario test is not a preview. It has a compiled artifact available, it
  is run in CI, and the single question it exists to answer for a relational work —
  *does this gate open only after the fact that licenses it?* — is exactly the
  question the quarantine removes. The result is that the project's verification
  story is strictly weaker than a tool already in the box.

  **What it should say instead**, in preference order:
  1. **`lute test` should walk the artifact, not the source** — i.e. be a `lute run`
     harness. D1 is untouched (the fixpoint stays engine-side; `run` *is* the
     reference engine consumer), the three permanently red coverage rows go green,
     the six over-seeded tests drop their seeds, and the alternation in item 2
     becomes testable. This is not a feature request: `lute run --mock` already
     accepts the same `facts:`/`choose:`/`state:` surface these test files declare.
     T9.17 and this task's summary both listed "run the fixpoint `lute run` already
     runs" as a *fix to be made*, which understated it — for the negative controls
     themselves it is an **available workflow today**, just not one `lute test`,
     `--coverage`, or any assertion in `expect:` can reach. What is missing is
     wiring, not capability.
  2. Failing that, a **negative mock** (`--no-fact` / `notFacts:`) so a derived
     relation can be pinned false inside the quarantine. Cheap, and it makes
     `trace` honest about the closed world its base relations already live in.
  3. At minimum, `trace`'s unresolved-atom hint should stop printing only
     `--fact "can_halt(toma)"`. It names the one move that biases every suite built
     on it toward the positive case, and it should name `lute run` beside it.

#### T9.8 — the silence is not confined to `expect:`, and a mis-keyed `choose:` passes while exercising the opposite arm — TOOL-DEFECT (silence)

- **Intent** — none authorial; asked the moment the third test file was written,
  because a test harness that fails open is worse than no harness.
- **Attempt** — four probe files in a scratch copy of the project, run through
  `lute test`:
  ```yaml
  # a-no-expect.test.yaml — no `expect:` block at all
  file: ../scenes/cryobank.lute
  choose: { whoWakes: wakeToma }
  ```
  ```yaml
  # b-empty-expect.test.yaml
  file: ../scenes/cryobank.lute
  choose: { whoWakes: wakeToma }
  expect: {}
  ```
  ```yaml
  # c-typo-key.test.yaml — four wrong keys, every assertion false
  file: ../scenes/cryobank.lute
  choose: { whoWakes: wakeToma }
  expect:
    transcriptContain: ["THIS TEXT IS NOWHERE IN THE CORPUS"]
    states: { run.shedPressure: 999 }
    exits: incomplete
    questStatus: { whoWakes: complete }
  ```
- **Result** — all three **PASS**.
  ```
  PASS  /tmp/t9probe/tests/a-no-expect.test.yaml
  PASS  /tmp/t9probe/tests/b-empty-expect.test.yaml
  PASS  /tmp/t9probe/tests/c-typo-key.test.yaml
  ```
  The verdict is `expectations.iter().all(|e| e.passed)` over a vector that is empty
  when no key is recognised, and `all()` on empty is `true`. `expect:` knows exactly
  three keys — `exit`, `transcriptContains`, `state` — and every other key, however
  plausibly misspelled, is dropped without a word.
- **Two aggravations.** `transcriptContain` (singular) is the natural typo and the
  most common assertion. And `questStatus` is not an invention: `lute test --help`
  reads *"…asserts the declared expectations (transcript, state, **quest status**)"*.
  An author following the tool's own help writes a `questStatus:` block, gets a
  green test, and believes a quest was asserted.

**The worse half, found on re-examination: the silence is not confined to `expect:`.**
A **top-level** key typo is dropped in exactly the same way, and for `choose:` the
result is qualitatively worse than a test that asserts nothing — it is a test that
asserts the *wrong arm's* outcome and passes.

```yaml
# z-typo-choose.test.yaml — `chooses:`, one letter off `choose:`
file: ../scenes/cryobank.lute
chooses:
  whoWakes: wakeNobody
expect:
  state:
    run.shedPressure: 2
  transcriptContains:
    - "How long have I been under?"
```
```
PASS  ./tests/z-typo-choose.test.yaml

1 passed, 0 failed
```

Read what that test believes and what it did. It names `wakeNobody`, the one arm in
the corpus that writes nothing. The selection is dropped, so no selection survives —
and `lute trace` then **auto-picks the first eligible arm**:
```
<branch whoWakes>   eligible: wakeToma, wakeIlsabet, wakeNobody   -> wakeToma (auto)
```
so the walk runs `wakeToma`, which sets `run.shedPressure += 2` and speaks Toma's
line. The expectations in the file are `wakeToma`'s outcome, asserted verbatim
against a file that says `wakeNobody`. Green.

The control settles it. Fix the one letter and change nothing else:
```
FAIL  ./tests/z2-correct-choose.test.yaml
      transcriptContains "How long have I been under?": absent (expected present)
      state run.shedPressure: expected "2", got "<never written>"
```
The typo is the only reason the test is green. Two silences compose here — an
ignored top-level key and an auto-picked default — and between them they turn a
false assertion into a passing one. Not every top-level typo lands this way: a
`factss:` typo is dropped just as silently but happens to go red, because the walk
then hits an unresolved atom. Whether a dropped key is caught is luck, and `choose:`
is the unluckiest one, because the harness has a default for it.

- **This punctures T9.11 / T9.16.5's "scenario tests do not rot silently".** That
  claim stands for the three rot modes it was tested against and fails for a fourth,
  measured side by side in one run:

  | rot mode | result |
  |---|---|
  | `choose:` names a deleted choice id (`wakeTomaTYPO`) | FAIL |
  | `choose:` names a deleted branch id (`whoWakesTYPO`) | FAIL |
  | `state:` names a path deleted from the schema | FAIL |
  | **`choose:` itself is mis-keyed (`chooses:`)** | **PASS** |

  The distinction is exact and worth keeping: a stale *value inside* a recognised
  key fails closed, because it is validated against the document. A stale or
  mistyped *key* fails open, because it is never looked at. T9.11's opening and
  T9.16.5 are corrected in place to say so.
- **Verdict** — `TOOL-DEFECT`, and the worst thing T9 found — more so with this half
  than without it. This is T3.10 and T7.11's "unknown mock key silently discarded"
  recurring one layer up, where it is categorically more expensive: a discarded
  *mock* key makes a test behave oddly and usually fail confusingly; a discarded
  *expectation* key makes it pass; a discarded `choose:` key makes it pass while
  running the branch the author was trying to rule out. The suite that exists to
  catch regressions is itself the thing with no check on it, and the cheapest fix is
  the one T3.10 already asked for — reject unknown keys, at *both* levels of the
  test file, not just inside `expect:` — plus a refusal to green a test with zero
  expectations. A fourth, independent of unknown-key rejection: `lute test` should
  not accept an auto-picked branch silently. `trace` prints `(auto)`; the harness
  should either report it or require the test to opt in.

#### T9.9 — a never-written state path fails against its own rendered value — TOOL-DEFECT (misdirecting diagnostic)

- **Intent** — the honest question behind `cryobank-wake-nobody.test.yaml`: the
  `wakeNobody` arm is the only branch arm in the corpus that writes nothing. Assert
  that `run.shedPressure` was not written.
- **Attempt** — `expect:` compares against the *final written* state, and an
  unwritten path renders as `<never written>` in the miss line, so:
  ```yaml
  expect:
    state:
      run.shedPressure: "<never written>"
  ```
- **Result** — FAIL, with:
  ```
  state run.shedPressure: expected "<never written>", got "<never written>"
  ```
  A mismatch line whose two sides are byte-identical. The comparison is
  `Option<String>` against `Some(want)` and the sentinel is a *rendering* applied
  only to the actual side, so `None` never equals the string it prints as.
- **Resolution** — the intent is abandoned; the test asserts the transcript only,
  and says so in a comment.
- **Verdict** — `TOOL-DEFECT`. Small, but it is the protocol's highest-priority
  category — a diagnostic that says X when the problem is Y — in its purest
  available form: it says the two values differ and then prints them equal. An
  author sees this and looks for whitespace, for quoting, for YAML coercion. The
  underlying gap (there is no way to assert that a path was not written) is T9.10's;
  this entry is only about the message.

#### T9.10 — what `expect:` cannot say — TOOL-DEFECT

The assignment asked what a test cannot express. The full list, each verified
against the committed suite. `expect:` has three keys, and the missing ones are not
exotic:

1. **That a line did NOT play.** There is no `transcriptOmits`/`not:`. Every
   negative in the suite is implied by a positive: `hydroponics-pressure-none`
   asserts the `<otherwise>` arm's line is present and *infers* that the `$ >= 2`
   arm's is not. A refactor that made both play would keep the test green.
2. **That a state path was untouched.** T9.9. `expect: state:` reads only the last
   `::set` per path across the walk, so seeds are unassertable too — the
   preconditions a test declares can never be checked as postconditions.
3. **Which options the player was offered.** The report *has* this. Every
   `<branch>` decision carries an `eligible[]` list, `lute trace` prints it
   (`<branch theCorrection>   eligible: invalidateTheVoyage, sayNothing`), and
   `--coverage` aggregates it across runs. There is no `expect:` key for it. For a
   branching work this is the single most valuable assertion available and it is
   computed, rendered, and not offered. What saves the suite is indirect and worth
   stating: `lute test` **enforces** eligibility, so a `choose:` naming a
   guard-false option is refused outright — the gate is proved by the test
   *running*, not by anything it says.
4. **That a particular ending was reached.** `::end{reason="shed-with-module"}`
   renders in the transcript as a bare `<end>` with no payload (T5.9), and `expect:`
   has no key for it. `exit: complete` means "the walk finished", nothing more —
   both terminals and all nine non-terminal scenes report it. Which ending a test
   pins is carried entirely by `file:`, and if the two terminal scenes ever shared a
   line of dialogue the two ending tests would be indistinguishable.
5. **Anything about a quest.** `lute test` traces a quest document fully — the
   activation predicate, every objective's `done`/`pending`, derived completion, the
   `<on>` handler — and reports all of it in `decisions[]`:
   ```
     <quest whoWakes>   -> active (true)
     <objective bringSomebodyUp>   -> done (count(awake(_)) >= 2)
     <objective theFifthBody>   -> pending (holds(found(ottavio)))
     <quest whoWakes>   -> complete
   ```
   None of it is assertable, and `--help` says it is. The **only** channel is a side
   effect: `quest-who-wakes-completes.test.yaml` proves completion by asserting the
   `<on event="questComplete">` narrator line, and `quest-manifest-gap-completes.test.yaml`
   proves it in state because that quest's handler happens to carry a `::set`. To
   make a quest testable at all, an author must give it a side effect it did not
   otherwise need.
6. **That a fact holds.** `facts:` is an input surface only; there is no
   `expect: facts:`. The suite asserts `::assert` sites by their downstream visible
   effects, never directly, even though the trace's `steps[]` records each one.

- **Verdict** — `TOOL-DEFECT`. Items 3, 4, 5 and 6 are all *computed and rendered by
  the same tool in the same run* and simply have no expectation slot; item 5 is
  additionally promised in `--help`. That is the criterion exactly: the information
  exists, and the tool that promised to hand it to you did not. Items 1 and 2 are
  the design half — a `transcriptOmits` and a `state: { path: unset }` would close
  both — and I would rank 3 first, because "was this choice offered" is the question
  a branching work exists to make you ask.

#### T9.11 — a refused test prints four words where `trace` prints the code, the key and the fix — TOOL-DEFECT

- **What works, and it is worth saying first — with one correction.** A scenario
  test does not rot silently *when the rot is inside a key the harness reads*,
  unlike a mock file (T1.9, T3.10). Three separate rot modes were probed against a
  copy of the project and all three fail the suite: a `choose:` naming a choice id
  that no longer exists, a `choose:` naming a branch id that no longer exists, and a
  `state:` naming a path deleted from the schema. `lute test` treats a refused trace
  as a test failure, so a stale suite goes red. **The unqualified form of that claim
  — "a scenario test does not rot silently" — is wrong and is withdrawn:** a
  mis-keyed `choose:` (`chooses:`) is dropped without a word, `trace` auto-picks the
  first eligible arm, and the test passes while running the branch it was written to
  exclude (T9.8). Stale *values* fail closed; stale or mistyped *keys* fail open.
- **Attempt** — read the failure and fix it.
- **Result** — all three produce the identical line:
  ```
  FAIL  tests/stale.test.yaml   trace refused: invalid mock input
  FAIL  tests/stale2.test.yaml  trace refused: invalid mock input
  FAIL  tests/stale3.test.yaml  trace refused: invalid mock input
  ```
  `lute trace`, on the same three inputs, prints:
  ```
  error [E-TRACE-CHOICE] `--choose whoWakes=wakeTomaTYPO` names an unknown choice id `wakeTomaTYPO` for `<branch/hub id="whoWakes">` (dsl 0.4.0 §4.3)
  error [E-TRACE-CHOICE] `--choose whoWakesTYPO=…` names an unknown branch/hub id `whoWakesTYPO` (dsl 0.4.0 §4.3)
  error [E-TRACE-MOCK-UNDECLARED] `--state run.deletedPath=…` names a state path not declared in the resolved schema (state-by-typo MUST fail in mocks exactly as in documents, dsl 0.4.0 §4.3, 0.1 §11.1.1)
  ```
  The harness **has** those diagnostics — it holds them in `TraceExit::Refused(diags)`
  and inspects their codes to choose between two canned strings — and then discards
  the vector.
- **Verdict** — `TOOL-DEFECT`. On a 31-file suite this is the difference between a
  one-second fix and a bisect: the message names neither the key, nor the value, nor
  the code, and is identical for faults in three different mock surfaces. The fix is
  to print the vector already in hand. (Minor, noted in passing: those messages
  render CLI flag spellings — `--choose`, `--state` — for input that came from YAML
  keys, so even printed they would point at the wrong syntax.)

#### T9.12 — the one document kind that cannot be tested is the one built for reuse — TOOL-DEFECT

- **Intent** — cover all 18 documents. The last one is
  `components/purser-interject.component.lute`.
- **Attempt** — `lute trace docs/examples/anseo/components/purser-interject.component.lute`.
- **Result** —
  ```
  components/purser-interject.component.lute:19:12: error [E-COMPILE-EXPAND] `@pressure` names no known def body (gate should have caught this)
  trace refused: … has check error(s) — run `lute check` first
  ```
  The advice is impossible to follow: `lute check` on that exact file, with
  `--project`, reports **`ok` (0 warning(s))**, and so does `check-project`. Two
  tools disagree about whether the same document has check errors, and the message
  telling you to consult the one that says `ok` is the one that refuses. Note also
  the parenthetical — *"(gate should have caught this)"* — an internal invariant
  assertion shipped in an author-facing diagnostic.
- **The coverage consequence.** The component's body is a `<match on="@pressure">`
  with two arms. It is invoked from exactly one site in the project
  (`cryobank.lute:14`, `pressure="rising"`), so its `<otherwise>` arm
  (`Allocation is nominal.`) is dead in this work — and **nothing says so**.
  `check-project` is clean, and `lute test --coverage` has no row for it at all:
  component-internal matches never appear in a traced report, so the three cryobank
  tests that expand the component contribute zero coverage information about it.
- **Resolution** — the suite covers **17 of 18** documents. The eighteenth is
  recorded in `quest-hold-the-spine-completes.test.yaml`'s comment.
- **Verdict** — `TOOL-DEFECT`. Two tools, opposite verdicts, same file — the same
  shape as T6.3 and T6.7, now on the testing surface. A component is the one
  construct in the language explicitly for reuse across callers, which makes it the
  construct where an arm most easily goes dead, and it is the only one with no test
  and no coverage.

#### T9.13 — coverage, honestly: what 31 tests actually exercise and what no tool will tell you — TOOL-DEFECT

`lute test --coverage` is real and is the best answer to "what is untested" that
exists in this toolchain. It is also keyed on the wrong thing. Every number below is
counted from the committed tree.

**What the suite covers, and this part is good.**

- **Choices: 15 / 15.** Five `<branch>`es — `whoWakes` (3), `thePlainAnswer` (3),
  `theSequence` (2), `whatsLeft` (3), `theCorrection` (4) — every arm of every one
  forced by at least one test.
- **Documents: 17 / 18.** All 11 scenes, all 6 quests; the component cannot be
  named (T9.12).
- **Arms: 12 of 15 reported rows complete.** The three that are not are
  `!holds(can_halt(toma)) && …`, `holds(can_halt(toma))` and `holds(can_halt(vesna))`
  — all three structurally unreachable for T9.7's reason, not for want of a test.
- The header wording is honest and worth crediting: *"coverage over 31 traced
  path(s)"*, never a whole-space claim. Its own module comment says so
  ("never a whole-space coverage claim"), and it holds to it.

**Where the number stops meaning what it says.** The coverage key is the guard's
**text**, not the construct's identity.

- The scenes hold **8** `<match on=…>` blocks and **11** guarded content lines —
  **19** distinct guarded constructs. `--coverage` renders them as **10** rows.
- **Six** of the eight blocks open `<match on="true">` — T7.6's dummy subject, the
  workaround the language *forces* because a relation may not be a `<match on>`
  subject. They live in `archive.lute:35` (2 arms), `machine-deck.lute:26` (2),
  `purser.lute:37` (2), `purser.lute:46` (3), `spine-b.lute:38` (3),
  `spine-b.lute:72` (3) — **15 arms across four files** — and they collapse into one
  row that reads:
  ```
  match `true`: 3/3 arm(s) executed [arm 1, arm 2, otherwise]
  ```
  A full-green row, over a bucket whose members it cannot name, whose largest member
  has three arms and whose total is fifteen. The two `<match on="run.shedPressure">`
  blocks in `hydroponics.lute` (3 arms each) collapse the same way into a `3/3` row.
- The same happens to line guards across documents: `holds(can_halt(vesna))` is one
  row covering `archive.lute:70` and `purser.lute:66`, in different scenes.
- So the tool's only false statement is also its most reassuring one. **A row reading
  `3/3` is the output an author scans for and stops at**, and here it certifies a set
  of six blocks that no single traced path ever visited together.

**What nothing answers at all.**

- **Untested documents are invisible.** Coverage is accumulated only from reports
  that ran, so a scene with no test contributes no row and generates no complaint.
  Delete `wake-cold-open.test.yaml` and nothing anywhere says `wake.lute` is
  untested. The 17/18 figure above is one I computed by hand from `file:` keys; no
  tool produces it.
- **No line coverage.** 167 content lines across the eleven scenes, and nothing
  reports which ever played. `loc export` knows every line; `trace` knows which
  played; nothing joins them.
- **No route coverage.** The suite traces **one scene at a time** by construction
  (`file:` is singular), so "does every ending remain reachable?" is not a question
  the harness can be asked. `reach-bridge.test.yaml` proves episode 10 *runs*, not
  that any route arrives at it; `lute scenario` proves the graph reaches it, but
  knows nothing about guards. The two halves of that question live in two tools and
  are never joined.
- **`<choice when=>` guards get no row.** Three of them (`purser.lute:64`, `:72`,
  `:83`) fold into their branch's eligibility and are never reported as covered or
  not.
- **Verdict** — `TOOL-DEFECT`. The keying is a one-line change (key on document +
  span, which every `Decision` already carries; render the guard text as a label),
  and until it is made the tool reports full coverage of things it has not covered.
  The missing document-level report is the bigger design hole and the honest summary
  of this entry: **an author shipping this work still has no way to ask what is
  untested — only what, among the things they happened to test, they tested
  incompletely.**

#### T9.14 — five quests over one world, and the only coupling the toolchain can see is the one you declared by hand — ERGONOMIC

- **Intent** — the assignment's question. Five quests now share two scalars and four
  relations. What does anything notice?
- **What is seen, and it is exactly the declared part.** `after=` produces
  `quest(whoWakes) -> quest(falseHeading) [active]` and
  `quest(whoWakes) -> quest(manifestGap) [completed]` (T9.2), and
  `quest.<id>.objectives.<oid>.done` cross-references are checked project-wide —
  T4's `W-QUEST-REF-UNKNOWN` fires with distinct messages for an unknown quest and
  for a known quest missing that objective, verified again here.
- **What is not seen, and it is live in the committed project.**
  `manifest-gap.lute`'s `<on event="questComplete">` runs `::set{run.vesnaTrust += 1}`.
  `what-vesna-carries.lute` activates on `run.vesnaTrust >= 2` and completes on
  `>= 3`. **One quest's completion advances another quest's activation threshold**,
  across two files, and nothing reports the edge: `lute scenario` shows only
  declared `after=` edges, and `scenario envelope quest:whatVesnaCarries` lists
  ```
    Guaranteed (safe to read under your declared routes):
      - run.shedPressure
      - run.vesnaTrust
  ```
  with no provenance for either — no writer list, no note that one of the writers is
  a sibling quest's completion handler. The one real inter-quest coupling in the
  project is the one no tool draws.
- **Two quests that can never both complete** — probed and not detected; that is
  T9.6, and it holds a fortiori across documents, since the intra-document case
  already draws nothing.
- **A quest satisfied by another quest's side effect** — not detected, and the
  example above is exactly it: `whatVesnaCarries.sheWillWorkWithYours`
  (`run.vesnaTrust >= 3`) is satisfiable by `manifestGap` completing plus two
  choices, and an author reading `what-vesna-carries.lute` alone cannot know that.
- **Verdict** — `ERGONOMIC`. Nothing is wrong and nothing is at risk; the analysis
  simply stops at the declaration layer. The envelope already computes which paths
  are written on which routes — it is one join away from naming the writers, and at
  six quests over two scalars the writer list is the thing an author needs.

#### T9.15 — a quest's own lifecycle state is the one reserved path you cannot read, and the surface that would have said so is empty — TOOL-DEFECT

- **Intent** — gate one quest on another's completion in the predicate, not just in
  the graph: `start="quest.holdTheSpine.state == 'complete'"`.
- **Result** —
  ```
  quests/probe.lute:8:45: error [E-MAYBE-UNSET] state path `quest.holdTheSpine.state` may be read before it is set (no default, no dominating `::set`, no guard) (dsl §9.4)
  ```
  The path is *declared* — `lute context` on a quest document prints it —
  ```
    quest.probeQuest.activatedAt: narrativeTime
    quest.probeQuest.objectives.arrive.done: bool
    quest.probeQuest.state: enum [active, complete, failed]
  ```
  — and its sibling reads fine: `start="quest.probeQuest.objectives.arrive.done"`
  checks clean, because `bool` carries a default and the `enum` does not. So the
  compiler-declared, three-valued lifecycle state of a quest is the one reserved
  path an author may not consult, while the derived per-objective flag beside it is
  free. An author wanting "after that quest finished" must either use `after=`
  (graph only) or read an objective flag and hope it means completion.
- **And the authoring surface hides all of it.**
  - `lute context --json` has a top-level key **`reservedQuestPaths`** and it is
    `[]` — on a quest document that prints three of them in its own human output,
    and on the shipped `docs/examples/investigation` quest too. The JSON
    `stateSchema` has no `quest.*` key either. Human and machine renderings of the
    same surface disagree.
  - `lute context` on a **scene** lists **zero** quest paths, in a project declaring
    six quests — although a scene may legally guard on one. Verified: adding
    `when="quest.probeQuest.objectives.arrive.done"` to a `purser.lute` content line
    checks clean project-wide, and nothing in that scene's authoring surface would
    have told you the path exists.
- **Verdict** — `TOOL-DEFECT`, and a direct recurrence of T1.6's shape: the tool
  whose stated job is to emit the authoring surface omits part of it, with no signal
  that anything is missing. New here is the *empty declared key* — `reservedQuestPaths`
  is not an omission an author can miss, it is a promise the output makes and then
  answers with `[]`. The `E-MAYBE-UNSET` on `quest.<id>.state` is the smaller half
  and may well be deliberate; if so it needs a message saying "read
  `quest.<id>.objectives.<oid>.done`, or use `after="completed(…)"`", because the
  current one describes a definite-assignment problem the author cannot fix.

#### T9.16 — what carried real weight — WORKED WELL

Five things, each load-bearing in the committed suite.

1. **`lute test` enforces choice eligibility — in one direction.** A `choose:`
   naming a guard-false option is refused, not silently played.
   `purser-invalidate-the-voyage.test.yaml` passes only because seeding
   `awake(ilsabet)` and `knows(ilsabet, true_heading)` genuinely opens the lever,
   and removing either turns the test red rather than wrong. Set against T8.5 —
   `lute run` plays a guard-false choice — the *test* harness is the one that got
   this right, which is the right way round. **The first draft called this "the
   single reason the relational gate tests mean anything"; that is half true and the
   half it omits is the dangerous one.** Enforcement proves the test's chosen option
   was *permitted*; it says nothing about what else was. T9.18 measures the
   consequence by mutation: delete the `when=` from that same lever and all 31 tests
   still pass. What eligibility enforcement catches is a gate that is too *narrow*.
   A gate that is too *wide* is invisible to it.
2. **`fail=` is a real independent axis, and the failure path is walkable.**
   `quest-false-heading-fails.test.yaml` completes all three objectives and still
   ends `failed`, firing `<on event="questFailed">`. T4.1 established the grammar
   accepts this; this is the first time it was executed.
3. **The three-value objective report.** `<objective theFifthBody> -> pending` is
   exactly the right rendering — done / pending / unresolved, each with the
   predicate that decided it, in both the human transcript and `decisions[]`. The
   quest walk is fully computed and well presented; only the assertion slot is
   missing (T9.10).
4. **The sequenced objective, in shipped content.** T4.1 verified
   `when="quest.<id>.objectives.<oid>.done"` and deferred it to this task; three of
   the five quests use it (`whoWakes.knowTheExchange`, `falseHeading.findThePaper`,
   `manifestGap.listHim`) and it composes with `optional` on the third. No mirror
   flag, no new construct.
5. **Scenario tests do not rot silently — when the rot is in a value.** Three
   separate rot modes all go red (T9.11): a stale choice id, a stale branch id, a
   deleted state path. After eight tasks of mocks and attributes being discarded
   without a word, a surface that fails closed is worth naming. **Corrected:** the
   unqualified claim does not hold. A mistyped *key* — `chooses:` for `choose:` —
   is dropped in silence and the test passes on an auto-picked arm (T9.8). What
   fails closed is validation of values inside keys the harness recognises.

#### T9.17 — recurrences, not re-counted

- **T4.4 / T8.9, from inside the harness.** Every quest test seeds facts, and every
  seed of a relation the quest document does not itself `::assert` draws
  `W-TRACE-MOCK-UNPRODUCIBLE — … the supplied answer can never arise from authored
  producers, so a complete walk seeded with it proves nothing about reachable play`.
  Producibility is judged document-locally, so on a quest file — which contains no
  `::assert` at all — *every* seeded relation is "unproducible", including `knows`,
  which the project-wide `W-UNPROVEN-RELATIONAL` on the very same line calls
  "producible relation(s) `knows`". Two warnings, one project, opposite adjectives,
  same relation. And `lute test` **prints neither**: the notes are dropped, so a
  green suite silently carries the "proves nothing" warning on six of its files.
- **T4.5.** `scenario reach` reports `Reachable` for a quest whose own document
  fails `check-project` on `E-OBJECTIVE-UNSATISFIABLE` (T9.6).
- **T5.4 / T5.9.** The ending is not assertable; `<end>` renders without its
  `reason` (T9.10, item 4). `manifestGap.listHim` wanted to read "the Purser
  recomputed with his name in it" and there is nothing to read — the `listTheMass`
  arm's only writes are a `run.shedPressure += 1` three other things also produce
  and an `::assert{knows(vesna, manifest)}` three other places also make. It ships
  as a shed-pressure proxy, marked `optional`, with the reason in the file.
- **T7.7.** A content-line `when=` is reported by `trace` and counted by
  `--coverage` as a `<match>` arm — the sugar the docs describe as "exact sugar for
  a `<match>` that does not compile" is visible in the coverage numbers, where 11
  guarded lines occupy 8 of the 15 rows.
- **T3.10 / T7.11.** Unknown-key silence, now on the assertion surface *and* on the
  test file's top level, where it is worse than either earlier instance: a mistyped
  `choose:` greens a test that runs the opposite arm (T9.8).

#### T9.18 — mutation-tested: 31 passing tests and a clean `check-project` cannot see a gate that opens when it should not — TOOL-DEFECT

T9.13 counted what the suite *touches*. This entry measures what it would *catch*,
because those are different questions and only the second is what a test suite is
for. Method: mutate one guard in a copy of the committed tree, re-run
`check-project docs/examples` and `lute test docs/examples/anseo --coverage`, and
diff all three outputs — verdict, warning set, and every coverage row — against the
unmutated baseline (47 files, 18 project-wide warnings; 31 passed, 0 failed;
15 coverage rows).

| # | mutation | `check-project` | `lute test` | `--coverage` |
|---|---|---|---|---|
| M1 | delete `when=` from `haltTheSequence`, the **ending-deciding** lever | ok, warnings byte-identical | **31 passed** | **identical** |
| M2 | weaken `invalidateTheVoyage` from `awake(ilsabet) && knows(ilsabet, true_heading)` to `awake(ilsabet)` | ok, identical | **31 passed** | **identical** |
| M3 | widen a content-line guard, `run.shedPressure >= 2` → `>= 0` (`stowaway.lute:26`; same result at `spine-b.lute:95`) | ok, identical | **31 passed** | one row changes |
| C1 | *control* — flip `holds(awake(toma))` to `holds(awake(vesna))` | ok, identical | 28 passed, **3 failed** | identical |
| C2 | *control* — `<match>` arm `$ >= 2` → `$ >= 1` in `hydroponics.lute` | ok, identical | 30 passed, **1 failed** | one row changes |
| C3 | *control* — narrow `haltTheSequence` to `can_halt(toma) && awake(ilsabet)` | ok, identical | 30 passed, **1 failed** (`trace refused`) | two rows change |

**M1 is the finding.** `haltTheSequence` is one of the two levers that decide which
ending the prologue reaches. Deleting its guard entirely — so the lever is offered
to a crew with nobody who can halt anything — changes *nothing observable anywhere
in the toolchain*: the same `ok`, the same eighteen warnings, the same 31 green
tests, and a byte-identical coverage block including
`branch/hub theCorrection: 4/4 chosen`. M2 is the same shape on the other lever:
drop half a conjunction from the guard on the *other* ending's lever and the suite
does not move.

**M3 needs a qualification the first framing of this finding did not have.** The
run is still 31/31 and `check-project` is still ok, so the mutation ships. But the
coverage block is not quite identical: a new row appears reading
`match run.shedPressure >= 0: 1/2 arm(s) executed; 1 unexecuted`, because an
always-true guard starves its own `<otherwise>`. So there *is* a signal, in the one
output nothing gates on, keyed on the mutated text (T9.13) so it appears as a new
row rather than as a changed one. Calling M3 "completely invisible" would be
overstating it; calling 31/31 a pass is not.

**The structural rule, and it is exact.** `lute test`'s one piece of non-authored
enforcement is that a `choose:` must name an *eligible* option. That checks
`chosen ⊆ eligible`. Nothing anywhere checks `eligible ⊆ intended`. So:

- **Over-restriction is caught** (C3): narrowing a guard makes some test's `choose:`
  ineligible, the trace is refused, the suite goes red. The mechanism is the
  eligibility enforcement T9.16.1 credits.
- **Under-restriction is not** (M1, M2): widening a guard makes *more* options
  eligible, every existing `choose:` stays legal, every transcript is unchanged, and
  there is nothing to fail.

The two controls that do fire, fire for the same reason as each other and neither is
eligibility. C1 changes *which lines play*, so positive `transcriptContains`
assertions go absent. C2 is the interesting one: `$ >= 2` → `$ >= 1` widens arm 1,
but arms in a `<match>` are exclusive, so widening one **starves** the next, and it
is the starved arm's positive assertion going absent that fails the test. That is
worth stating precisely, because it bounds exactly how much a `<match>` protects
you: a widened guard is caught only where widening it takes something away from a
sibling that a test asserts. A bare content-line `when=` (M3) and a `<choice when=>`
(M1, M2) starve nothing. They are the two guard forms with no sibling to rob, and
they are the two the suite cannot see.

**Which direction actually ships a broken work.** Over-restriction is loud in play:
the lever the designer wanted is missing, and the first person to walk the route
notices. Under-restriction is a lever that opens for a crew that has not earned it —
the halt available with nobody who can halt, the navigator's argument available from
a navigator who does not know the true heading. That is the bug that reaches
players, and it is the one nothing in this toolchain can see.

**What the suite actually is, stated plainly.** It is a good regression harness for
**authored text and state deltas**: change a line, change a `::set`, delete a
`<choice>` id, rename a state path, and it goes red immediately and reliably (C1,
C2, C3, and the three rot modes in T9.11). It is a **poor specification of the
work's logic**: the guards that decide which of two endings a run reaches can be
deleted outright and it stays green. Anyone reading `31 passed` as *the branching is
correct* is misled. The honest reading of `31 passed` is *the eleven scenes still
say what they said, and every arm we force is still reachable.*

- **Verdict** — `TOOL-DEFECT`. This is the measurement behind T9.10 item 3, which
  ranked "which options the player was offered" as the most valuable missing
  assertion; this entry is why. The eligibility set is **computed** — every
  `<branch>` decision in a trace report carries `eligible[]`, `lute trace` prints it
  (`<branch theCorrection>   eligible: invalidateTheVoyage, sayNothing`), and
  `--coverage` already aggregates it well enough to say *"1 never seen eligible in
  any traced path"* under C3. It is rendered, it is in `decisions[]`, and `expect:`
  has no key for it. That is the criterion exactly: the information exists and the
  tool that computed it will not let you assert it. One key closes M1 and M2 both:
  ```yaml
  expect:
    eligible: { theCorrection: [haltTheSequence, sayNothing] }   # exact set
  ```
  With that one line in the four `purser-*.test.yaml` files, M1 and M2 both go red.
  Nothing new is computed; a vector already in the report gets an assertion slot.

#### T9.19 — a warning class a finished, correct, fully-tested work triggers thirteen times, with no discharge path — SPEC-WRONG

Recorded in this task's report as an observation with no verdict; it needs one.
`W-UNPROVEN-RELATIONAL` went from **1** to **13** on this project — the pre-existing
one is `hold-the-spine.lute:8`, the twelve new ones are one per relational predicate
across the five new quests. Four things were checked before filing, because three of
the four could have made this an author problem instead.

1. **Every one marks a correct gate.** The thirteen are `awake`×2, `knows`×5,
   `found`×3, `can_halt`×3 across six quest documents. Each is a deliberate
   relational predicate on a quest `start=` or objective `done=` — precisely the
   construct T4 established as the language's strength.
2. **Every relation is genuinely producible, and the checker really does
   distinguish.** Negative control: delete the one rule from `world.schema.yaml`
   (`rules: []`, nothing else changed). The count drops 13 → 10 and **exactly two**
   of the three `can_halt` sites become hard errors:
   ```
   quests/unmoored.lute:21:1: error [E-OBJECTIVE-UNSATISFIABLE] `done` predicate
     `count(can_halt(_)) >= 1` queries relation(s) `can_halt`, which is unreachable
     under your declared routes …
   quests/what-vesna-carries.lute:21:1: error [E-OBJECTIVE-UNSATISFIABLE] `done`
     predicate `holds(can_halt(vesna))` …
   failed: (18 file(s), 2 project-wide error(s), 10 project-wide warning(s))
   ```
   So the warning is not a blanket "relations are hard"; it is the *residue* after a
   real analysis, and the analysis has teeth. (The third `can_halt` site,
   `hold-the-spine.lute:8`, is a quest `start=` and simply goes silent — there is no
   error-grade counterpart for that slot, which is T4.4's "no not-producible branch"
   observed from the other side.)
3. **Twelve of the thirteen are demonstrated satisfiable by a passing test.** The
   six quest tests between them seed every warned relation and show the quest
   reaching `complete` (or, for `false-heading`, `failed`). The exception is
   `who-wakes.lute:19`, `theFifthBody`, `done="holds(found(ottavio))"`: it is marked
   `optional`, its test leaves it `-> pending`, and the quest completes anyway. One
   warned site out of thirteen has no test showing it satisfied — worth knowing, and
   not a defence of the other twelve.
4. **None of them is dischargeable.** `scan_objective_liveness`
   (`crates/lute-check/src/producible.rs:190-216`) emits on every quest `start`/`fail`
   slot and walks every objective, with no gate but "is the relation producible".
   `check-project` takes `--json`, `--providers`, `--deny` and `--deny-warnings` and
   nothing else — **no seed surface, no mock surface**, so the "verify with `lute
   trace` seeds" half of the warning's own remedy cannot be fed back to the tool that
   raised it. There is no per-site suppression construct in the language, and there
   is no `--allow`: dsl 0.6.1 §6 lists *"No `--allow` demotion"* as an explicit
   non-goal. An author who has done everything right — correct gates, producible
   relations, a passing test per predicate — has exactly one lever, `--deny`, and it
   points the wrong way.

  So the question the observation left open answers itself. This is not misuse: the
  gates are right, the relations are real, and the work is tested. Nor is it a bug:
  every component behaves as specified.

- **Verdict** — `SPEC-WRONG`. Nothing is misimplemented and nothing is
  undocumented. dsl 0.6.1 §2 specifies the warning, `producible.rs` emits exactly
  it, the message is one of the better-written in the toolchain (it names the
  relation, cites its clause, and proposes two remedies), and §6's refusal of
  `--allow` is a deliberate, reasoned decision.

  **What the spec says.** A relational gate is a region static reachability "neither
  proves nor refutes", so the checker declines to claim satisfiability and refers the
  author to `lute trace` seeds or human review. **Why it is the wrong call.** The
  severity is chosen for the wrong population. As specified, the warning fires on the
  *presence of the feature*, not on any property of the author's use of it: `awake`
  and `knows` are producible in this project because scenes assert them, which is the
  language working. So the class scales one-for-one with adoption of relational
  quests — six quests give thirteen, a twelve-quest work gives twenty-six — and it is
  unclearable at every size. A warning class whose count is a function of how much of
  the language you use, with no discharge path, is not a signal; it teaches authors to
  read past the project-wide block, which is where `W-QUEST-REF-UNKNOWN` and
  `E-OBJECTIVE-UNSATISFIABLE`'s relational cause also live (T4.2, this log's strongest
  finding). That is the cost: it is a noise floor over the checker's best output.

  **What it should say instead.** Any one of these fixes it; the first is the real
  one.
  1. **Let evidence discharge it.** The two remedies the message names are both real
     work an author can do, and neither is reportable back. If a `*.test.yaml` in the
     project traces the document and shows the predicate satisfied, that predicate's
     warning should not fire — `lute test` already computes exactly this, and T9.7's
     proposal (walk the artifact under `lute run`, which *does* run the fixpoint)
     would make it a proof rather than a hint. A warning that names a remedy should
     accept its completion.
  2. **Site-level acknowledgement.** An attribute — `unproven="reviewed"`, or the
     existing comment convention promoted to a recognised marker — so "human review"
     is a state the file can record. This is the cheap version.
  3. **Report it once, not thirteen times.** Demote the per-site warning to a
     single project-wide note with a count and a `--verbose` list. Cheapest of all
     and it keeps the signal while removing the flood.

#### T9 summary

Sixteen entries carrying a verdict: **one `DOC-GAP`** (T9.1), **two
`LANGUAGE-GAP`** (T9.3, T9.4), **two `ERGONOMIC`** (T9.5, T9.14), **nine
`TOOL-DEFECT`** (T9.6, T9.8, T9.9, T9.10, T9.11, T9.12, T9.13, T9.15, T9.18), and
**two `SPEC-WRONG`** (T9.7, T9.19) — 1 + 2 + 2 + 9 + 2 = 16, one verdict per entry,
no hybrids. Three further entries carry no verdict: T9.2 and T9.16 record what
worked, T9.17 rolls up recurrences already counted in earlier tasks. Nineteen
entries in all. **Nine of the sixteen are `TOOL-DEFECT`** — the highest
concentration in this log, which is what you would expect from the first task to
point a testing tool at the work rather than a checker.

*This tally is post-correction. As first written the section had fourteen
verdict-bearing entries and nine `TOOL-DEFECT`; T9.7 was reclassified `SPEC-WRONG`
(the two evaluators are each doing what the spec tells them — see the entry, which
also retracts two false factual claims), and T9.18 (`TOOL-DEFECT`) and T9.19
(`SPEC-WRONG`) were added.*

**The quest layer of the language is in good shape and the quest layer of the tools
is not.** Everything a real goal machine wants was there and worked at five
instances: `after=` nodes and quest-to-quest edges (T9.2), `fail=` as an independent
axis executed end to end (T9.16.2), `optional`, sequenced objectives via the
compiler's own reserved paths (T9.16.4), derived relations as objective predicates,
and a per-objective done/pending/unresolved report (T9.16.3). Two real language
holes were found and both are about *time*: a quest cannot say "the player got here"
(T9.3) and nothing in the language is evaluated at the end of a run (T9.4). The
second is the one to fix — the form an author reaches for compiles clean and
completes instantly, which is silence, which this log has spent nine tasks
establishing is the expensive failure.

**The testing story exists, runs, and fails open — in both directions.** 31 tests,
17 of 18 documents, 15 of 15 choices — that suite is buildable in an afternoon and
it caught real things while being written. But `expect:` recognises three keys and
silently ignores every other, a *top-level* key is dropped just as silently, a test
with no expectations is green, and the one key `--help` advertises that does not
exist (`questStatus`) is the one an author would most want (T9.8). And what the
suite does assert is narrower than it looks: deleting the guard from an
ending-deciding lever leaves all 31 green and `check-project` byte-identical
(T9.18). Ranked by what it costs an author:

1. **T9.8** — a verification tool that greens a typo'd assertion is worse than none,
   and a mis-keyed `choose:` is worse still: it greens a test that runs the arm the
   author was excluding. The fix is the same unknown-key rejection T3.10 asked for
   two hundred entries ago, applied at both levels of the file.
2. **T9.18** — the suite cannot see an over-permissive gate, which is the failure
   mode that ships a broken branching work. One `expect: eligible:` key over a
   vector the trace report already carries closes it.
3. **T9.7** — the harness `lute test` walks cannot compute the project's one rule,
   so three coverage rows are permanently red and six tests seed a conclusion they
   should have derived. Correction to this entry's first draft, which matters for
   how it is prioritised: the negative controls it called impossible are **already
   available**, in `lute run`, which runs the fixpoint. So the work is wiring
   `lute test` to the artifact — not inventing a capability.
4. **T9.13** — keying coverage on guard text: six distinct blocks reporting one
   `3/3` row is a false green in the one output an author reads to decide they are
   done.
5. **T9.11** — because it is printing a vector the code already holds.

**The measurement the assignment asked for, stated plainly.** After 31 passing tests
covering every choice in the work, *no tool in this toolchain can tell an author what
is untested.* Coverage is accumulated only over documents that were traced, so an
untested scene is not a gap in the report — it is absent from it. There is no line
coverage, no route coverage, and no document-level report; the 17/18 figure in T9.13
was counted by hand out of the `file:` keys. And because `file:` is singular, the
question a two-ending work most needs answered — *is every ending still reachable?* —
cannot be put to the test harness at all. `lute scenario` answers the graph half
knowing nothing of guards, `lute test` answers the guard half one scene at a time,
and nothing joins them. That gap is not a missing assertion or a wrong key; it is the
shape of the tool.

**And one measurement the assignment did not ask for, added on review, because it
reframes the rest.** *What is untested* turns out to be the smaller question. The
larger one is what the tested part would catch, and T9.18 answers it by mutation:
`lute test` is a good regression harness for **authored text and state deltas** and
a poor specification of the **work's logic**. Change a line, a `::set`, a choice id
or a state path and it goes red at once. Delete the `when=` from either of the two
levers that decide which ending the prologue reaches and it stays green, 31 of 31,
with `check-project` byte-identical and every coverage row unchanged. The
enforcement the suite rests on — a `choose:` must name an eligible option — proves
only that a gate is not too narrow; nothing anywhere proves it is not too wide, and
too wide is the direction that ships. That is the last thing this drive test found,
and it is the one an author is most likely to read backwards.

### T10 — The gates, the README, and the two deferred findings

Toolchain 0.9.0 / language 0.9.0 / IR 0.9.0, `./target/debug/lute`, rebuilt before the
first probe. No content added: this task closes the two items earlier tasks deferred
here, replaces `docs/examples/anseo/README.md`, and registers the one CI surface that
turned out not to exist. Both deferred items are re-derived from scratch below rather
than transcribed from the reports that raised them.

#### T10.1 — the page that owns state declaration carries one `enum` example, and it does not parse — DOC-WRONG

Held for this task by T5 (see the note closing the T5 summary), which found it while
probing T5.4's mirrored-ending enum and correctly ruled it outside `::end`'s remit.
Reproduced here from a fresh `lute init` tree, not from T5's transcript.

- **Intent** — declare an `enum`-typed state path. T5.4's proxy needs one
  (`run.ending`), and it is the ordinary thing any work with a closed set of outcomes
  reaches for. Go to the page named after the question.
- **Attempt** — `packages/website/src/content/docs/state/state-model.md`, §"Declaration",
  lines 21–27. The block is four declarations and exactly one of them is an `enum`:
  ```yaml
  state:
    scene.affect.sofia: { type: number, default: 0 }
    run.choseHelp:      { type: bool,   default: false }
    user.level:         { type: number, default: 1 }
    app.rating:         { type: enum, values: [teen, adult], default: teen }
  ```
  Line 26, copied verbatim into a scratch project's `world.schema.yaml`, imported by a
  scene through `uses:`.
- **Result** — exit 1, and the diagnostic never names the line:
  ```console
  $ lute check /tmp/t10b/proj/scenes/probe.lute --project /tmp/t10b/proj
  /tmp/t10b/proj/scenes/probe.lute:1:1: error [E-USES-PARSE] schema import
  `/private/tmp/t10b/proj/world.schema.yaml` has parse/frontmatter errors (1 issue(s))
  failed: /tmp/t10b/proj/scenes/probe.lute (1 error(s), 0 warning(s))     # exit 1
  ```
  That is T3.9's count-with-no-body arriving on top of this one, and it is the shape an
  author actually meets, because a state schema is a file you `uses:`. Declared inline
  in a scene's own frontmatter instead — the one position that bypasses `E-USES-PARSE` —
  the real message appears:
  ```console
  $ lute check /tmp/t10b/proj/scenes/inline.lute --project /tmp/t10b/proj
  /tmp/t10b/proj/scenes/inline.lute:1:1: error [E-STATE-DECL] invalid state declaration
  for `scene.rating`: invalid type: unit variant, expected newtype variant   # exit 1
  ```
- **The working form, and it is not obscure.** `{ type: { enum: [...] } }` — the enum
  members nested *inside* `type:` rather than beside it in a `values:` key:
  ```console
  $ cat world.schema.yaml
  state:
    app.rating: { type: { enum: [teen, adult] }, default: teen }
  $ lute check /tmp/t10b/proj/scenes/probe.lute --project /tmp/t10b/proj
  ok: /tmp/t10b/proj/scenes/probe.lute (0 warning(s))                      # exit 0
  $ lute context /tmp/t10b/proj/scenes/probe.lute
  stateSchema (1):
    app.rating: enum [teen, adult]
  ```
- **The counts, both directions, from the source tree only (no `dist/`, no
  `node_modules/`).** The working spelling is used in **six shipped example files** that
  `check-project docs/examples` walks green — `affinity-reaction.lute`,
  `choice-persist.lute`, `investigation/world.schema.yaml`, `showcase/when-is-demo.lute`,
  `showcase/schema/base.schema.yaml`, and the `idola-project`/`showcase` plugin manifests
  — and on the website in `getting-started/build-an-investigation.mdx:46–48`. The broken
  spelling appears **four times, and none of them parses**:
  `state/state-model.md:26`; `packages/website/public/llms-full.txt:1879`, which is the
  same line in the bundle the repo ships *for machine consumption*; and
  `docs/proposals/scenario-dsl/state-model-design.md:75–76`, twice.
- **And the two spellings collide on the same declaration.**
  `docs/examples/showcase/schema/base.schema.yaml:9` reads:
  ```yaml
  app.rating:       { type: { enum: [teen, adult] }, default: teen }
  ```
  Same path, same two members, same default as `state-model.md:26`. One is a shipped
  example that checks clean; the other is the reference page's only enum example and it
  is rejected. The page and the example were plainly written from each other and one of
  them was not run.
- **Resolution** — none authorially; Anseo declares no `enum` state path (T5.4's proxy
  was measured and deliberately not shipped), so nothing in the corpus changes. The
  entry *is* the finding. The doc fix is one line, and the same edit applies to
  `llms-full.txt` and to the proposal.
- **Verdict** — `DOC-WRONG`. Present and false, and the table's own ranking argument
  applies without adjustment: silence would send an author to a working example, and
  this sentence stops them looking. It is T3.13's exact shape — a reference page stating
  something the rest of the corpus contradicts — with one aggravation T3.13 did not
  have. There, the truth was on a sibling page (`facts-and-datalog.md`) and an author who
  kept reading recovered. Here the page in question is `state-model.md`, under the
  heading **Declaration**, which is *the* page for how to declare a state path; the
  working form is not stated in prose anywhere on the site, only shown in a tutorial's
  code block three pages away. An author who copies the reference page's block gets
  `E-USES-PARSE`, a count, and no message (T3.9), on a file the checker will not open
  for them (`lute check world.schema.yaml` misparses it as a scene). The recovery path
  is to notice that a different page's YAML nests one key differently.
  Two sentences fix it: correct line 26, and add "an `enum` path nests its members
  inside `type:` — `{ type: { enum: [a, b] } }`" to the paragraph below the block, which
  currently names `enum` as a legal scalar type and never shows its shape.

#### T10.2 — `E-STATE-DECL`'s body is serde's vocabulary, not the author's — TOOL-DEFECT

Split from T10.1 at the controller's direction, because the page being wrong and the
diagnostic being unreadable are different defects against different artifacts, and the
second survives the first being fixed.

- **Intent** — n/a authorially. Once T10.1's real message was unhidden, read it as the
  author it is addressed to.
- **Result** — the message, in full:
  ```
  error [E-STATE-DECL] invalid state declaration for `scene.rating`:
  invalid type: unit variant, expected newtype variant
  ```
  The prefix is excellent — the code is specific, and it names the offending path. The
  colon then hands over to **serde's internal data model**. "Unit variant" and "newtype
  variant" are Rust enum-representation terms; neither occurs anywhere in
  `packages/website/src/content/docs`, in the language, or in YAML. Nothing in the
  sentence names the key that is wrong (`values:`), the key that is missing, the legal
  shape, or the fact that an author wrote `enum` where the parser wanted `{ enum: [...] }`.
  The author's mistake is one nesting level; the message describes a Rust deserialiser's
  state.
- **It is a general defect, not one bad string.** The same wording is already pinned as a
  scraped literal by the repo's own snippet gate, against a *different* path and a
  different mistake — `scripts/check-doc-snippets.py` lists, among the twelve messages it
  resolves to a literal in `crates/*/src`:
  ```
  · [E-STATE-DECL] invalid state declaration for `run.inventory`: invalid type: unit
    variant, expected newtype vari…
  ```
  So the serde tail is what `E-STATE-DECL` says for any malformed `type:`, and it is
  verified-in-CI as the thing it says.
- **The checker is one line from the useful message.** The neighbouring rule on the same
  page is enforced with a message written for a human:
  `E-STATE-COLLECTION` — *"state path `run.inventory` cannot declare a collection type
  (`list`/`record`/`map`); author state is scalar-only"* — which names the offending
  input, the rule, and the closed legal set. `E-STATE-DECL` has the same path in hand and
  the same closed set (`number`, `bool`, `string`, `enum`) and forwards a library error
  instead.
- **Resolution** — `NONE — nothing to resolve; the probe is the finding.`
- **Verdict** — `TOOL-DEFECT`, on the criterion's own words and on this log's own
  precedent. Not `DOC-GAP` or `DOC-WRONG`: no page's absence or falsehood causes it and
  none could fix it — T10.1 fixing line 26 leaves this message exactly as it is for the
  next author who mistypes a `type:`. Not `AUTHOR-ERROR`: the diagnostic is the thing at
  issue, not the mistake. Not `SPEC-WRONG`: nothing is specified about this wording, so
  there is no design to fault. That leaves a tool wrong about its own contract, where the
  contract of author-facing output is that an author can act on it — the identical ground
  on which T4.10 files `scenario envelope` printing `check_quest_guard_defassign` and an
  internal task label at an author, and T9.12 files `E-COMPILE-EXPAND` shipping
  *"(gate should have caught this)"*. That makes this the **third** distinct instance of
  compiler internals reaching an author-facing surface, which is why it is filed rather
  than folded into T10.1: one is a slip, three is a habit, and this one is load-bearing
  because it is the *only* message an author gets for a malformed state declaration.

#### T10.3 — the repo triggers CI on 34 scenario tests and never ran them — closed by this task, no verdict

Not a language finding and not a defect in Lute; a gap in this repository's own gating,
found by doing what the assignment asked — running the gates rather than assuming their
coverage — and closed in the same commit. Recorded because the drive test's own
conclusions are only as good as the surfaces that hold them in place.

- **What was asked** — whether `docs/examples/anseo` is covered by the existing
  `docs/examples` root, and to register it if not.
- **What the gates print.** `scripts/check-docs-consistency.py` ends by emitting the
  example-check manifest, and it is one root:
  ```console
  $ python3 scripts/check-docs-consistency.py
  check-docs-consistency: example roots for CI check-project:
    - docs/examples
  ```
  `scripts/check-doc-snippets.py` takes `EXAMPLES_ROOT = docs/examples` and pins
  capability hashes over the project roots beneath it.
- **The check surface is genuinely covered, and nesting is not the problem.** T1.10
  established that `check-project` walks nested projects; verified again on the finished
  corpus, which is the state that matters:
  ```console
  $ ./target/debug/lute check-project docs/examples | grep -c anseo
  33
  ```
  All 18 Anseo documents are walked by the outer root, and both deliberate
  `W-INJECT-CONFLICT`s and all 13 `W-UNPROVEN-RELATIONAL`s appear in that run. So
  **`docs/examples/anseo` needed no registration for `check-project`** — the answer the
  assignment asked to be given either way.
- **The test surface was covered by nothing.** `lute test` does not appear in
  `.github/`, in `scripts/`, or anywhere else that runs:
  ```console
  $ grep -rn 'lute test' .github scripts
  # (no matches)
  ```
  There are **34** `*.test.yaml` files in the repository — 31 under
  `docs/examples/anseo/tests/` and 3 under `docs/examples/investigation/tests/` — and no
  job read one. `check-project` walks `.lute` documents and never opens a test file, so
  the two gates that *do* run were both honestly reporting on a surface these files are
  not part of.
- **Why that is worse than an ordinary omission.** `docs.yml` lists `docs/**` in its
  trigger paths, so every one of those 34 files *starts* a CI run that then reads none of
  them. The workflow's own header names this exact anti-pattern, three lines above the
  job: *"`docs/**` is in the trigger paths below, so it has to be a root here too — a
  trigger that fires on files nothing reads advertises coverage it does not provide."*
  The principle was written down and the test suites were added afterwards.
- **Resolution — registered.** One step in the `examples` job of
  `.github/workflows/docs.yml`, on the same root and the same already-built binary as the
  `check-project` step: `cargo run -q -p lute-cli -- test docs/examples`. `lute test`
  recurses, so one root covers both suites — verified at `34 passed, 0 failed`, exit 0.
  The step's comment states what a green run does and does not prove, citing T9.18,
  because a gate that is read as stronger than it is would be a worse outcome than no
  gate.
- **No verdict.** Nothing here is a property of Lute 0.9.0. It is filed under the
  protocol's *silence* register all the same: the most expensive failure in this log is
  consistently a surface that reports success over something it never looked at, and for
  eight tasks that description fit this repository's own CI as squarely as it fits
  `lute test`'s `expect:` block (T9.8).

#### T10 summary

Three entries: one `DOC-WRONG` (T10.1), one `TOOL-DEFECT` (T10.2), and one closed
infrastructure gap carrying no verdict (T10.3). No content was written, so there is no
`LANGUAGE-GAP` or `ERGONOMIC` here by construction — this task authored a README and a CI
step, not a scene.

Both deferred findings landed where the tasks that deferred them predicted, and both are
instances of patterns this log had already named rather than new classes. T10.1 is T3.13
— a reference page contradicting the corpus — with the recovery path narrower, because
the correct spelling exists in six example files and no prose. T10.2 is T4.10 and T9.12 a
third time: compiler internals in author-facing output. The one genuinely new observation
is the compounding in T10.1, and it is worth stating on its own because it is what an
author actually experiences. Three defects this log filed separately, in three different
tasks, stack on one four-word mistake: the page is wrong (T10.1), the error it produces is
reported as an integer with no body because the declaration is in an imported schema
(T3.9), and the command an author would run next misparses the schema as a scene and tells
them to add `kind:` to it (T3.9 again). The author's total information is
`(1 issue(s))`. Individually each is small; in series they are the difference between a
typo and an afternoon.

T10.3 is not a Lute finding and is recorded anyway, because the shape is the log's own:
34 test files that fire CI and are read by nothing, in a repository whose workflow header
had already written down the rule they violate. It is closed.
