# Anseo drive test, re-run against Lute 0.10.0 — findings

*Measured 2026-08-06 on `feat/lute-0.10.0` at `d8d79e0`, 68 commits ahead of `main`.
Binary: `target/debug/lute`, `lute version` → toolchain `0.10.0`, language `0.10.0`,
IR schema `0.10.0`.*

## What this is

`docs/examples/anseo/` is not a sample. It is an **instrument**. The eleven-scene example
was built in July to drive-test the language; that run produced
[`2026-07-31-anseo-drive-test-findings.md`](2026-07-31-anseo-drive-test-findings.md) —
111 entries, 74 of them verdict-bearing — and the
[38-issue backlog](2026-07-31-lute-0.9.0-improvement-backlog.md) this entire release was
scoped from. Spec [§13](../../proposals/scenario-dsl/0.10.0.md) makes specific, falsifiable
predictions about what `0.10.0` changes.

This document re-runs the instrument and reports which of those predictions hold. It is
the release's acceptance evidence, not a test pass.

**Nothing here was fixed.** Where a probe found a defect it is filed below, with the
command that reproduces it, and the tree was left alone.

## How to read it

Verdict vocabulary is the previous log's, unchanged — `LANGUAGE-GAP`, `ERGONOMIC`,
`DOC-GAP`, `DOC-WRONG`, `AUTHOR-ERROR`, `TOOL-DEFECT`, `SPEC-WRONG` — and its rule holds:
**exactly one verdict applies per entry**. `CLOSED` replaces the verdict where the finding
no longer exists.

Three dispositions for a re-run probe:

- **CLOSED** — the probe no longer reproduces, and the mechanism that closed it is named.
- **STILL REPRODUCES** — `0.10.0` claimed it and did not deliver. This is the finding that
  matters; each one below carries its output verbatim.
- **OUT OF SCOPE** — the issue is one of the twelve deferred `DESIGN` items. Reproduction
  is expected and correct.

**"No output" is never a proof.** A rule that fires on nothing and a rule that does not
exist produce identical corpus output. Every predicted-silent warning below carries a
**positive control** — the failing case constructed in `/tmp`, the rule shown firing — and
only then the demonstration that the corpus does not trigger it. Thirteen distinct rules
were positive-controlled across §13 alone — `E-SET-TYPE`, `E-UNKNOWN-ATTR`, `E-AS-REMOVED`,
D-L's per-position rule, `E-QUEST-UNREACHABLE`, `E-OBJECTIVE-CONTRADICTION`,
`W-PROJECT-INERT`, `E-MOCK-SUBJECT`, `E-TRACE-MOCK-UNDECLARED`, `E-TIME-RESOLUTION`,
`W-EXIT-INERT`, `W-STAGE-ABSENT`, `W-DOMAIN-UNREAD` — and every one fired. Step 3 added
more, including `W-COMPONENT-UNVERIFIED`, `E-TEST-KEY` and `E-DEFAULTS-KEY`.

## Headline

> **Of the 26 issues `0.10.0` claimed, 19 are confirmed closed, 4 landed in part, and 3 did
> not land.** The three that did not land are **#2** (`lute test`'s blind spots — one of five
> probes closed), **#30** (`W-PROJECT-INERT` was wired into `compile` and never into
> `check-project`, so the surface the finding was filed against is still silent), and
> **#34** (`defaults:` is applied by the document pass and by nothing else — using it on the
> Anseo corpus costs the entire scene graph, six compiles and all 31 tests).

Twelve of the 54 re-run probes still reproduce. Eight of those twelve sit on issues the
release claimed outright; the other four are `lute context` defects that spec §2.3
deliberately carved out of #17 and which are therefore *partially* claimed, not out of
scope.

The language axis is in much better shape than the toolchain axis, which is the same split
the July log found and the reverse of what a release themed *"the toolchain says what it
knows"* would predict. Five of the six `LANG` changes are clean; the sixth, `defaults:`,
is the single worst thing in this log.

---

## The 38 backlog issues

`claimed` is the release's own boundary: spec §2.2's thirteen language-axis issues plus
§2.3's thirteen toolchain issues = 26; the remaining 12 are deferred `DESIGN` work.

| # | issue | claimed | re-run | evidence |
|---|---|---|---|---|
| 1 | `::set` writes any expression into any declared path | yes | **CONFIRMED** | T3.2 CLOSED — `E-SET-TYPE` on all three cases, `compile` refuses too |
| 2 | the suite cannot see a gate that opens too wide | yes | **NOT CONFIRMED** | 1 of 5: T9.8 CLOSED; T3.10, T9.9, T9.10, T9.18 still reproduce |
| 3 | component lines dropped from the localization bundle | yes | **CONFIRMED** | T6.10 CLOSED — component null-`lineId` rows 2 → 0; `lute tag` remedy works |
| 4 | `{{@param}}` cannot render a `string` | no — deferred | reproduces (expected) | T6.11, byte-for-byte incl. the `9:39` position |
| 5 | a component has no meaning of its own | no — deferred | reproduces (expected) | T6.2, one body → two vs three commands |
| 6 | `<choice>` accepts and discards any attribute you invent | yes | **CONFIRMED** | T8.2 CLOSED — `E-UNKNOWN-ATTR` on 6/6 constructs, `E-AS-REMOVED` + `lute fix` |
| 7 | no player choice can decide what comes next | no — deferred | reproduces (expected) | T8.3, no project-level `exclusive:` exists |
| 8 | nothing can ask how the player got here | no — deferred | reproduces (expected) | T8.1, T9.3 — `E-CEL-PROFILE` text unchanged |
| 9 | "you got through it without X" is unwritable | no — deferred | reproduces (expected) | T9.4 |
| 10 | the nine false documentation statements | yes | **CONFIRMED** | 9 of 9 rows closed: T7.7 a, T3.13 b, T10.1 c, T8.6 d, T5.5 e, T4.8 f, T6.8 g, T7.10 h, T7.16 i |
| 11 | the three documentation silences | yes | **CONFIRMED** | 3 of 3 broken; both *"also, and cheaply"* additions shipped |
| 12 | a dead gate is quieter than a live one | yes | **CONFIRMED** | T4.5, T9.6 both CLOSED — `E-QUEST-UNREACHABLE`, `E-OBJECTIVE-CONTRADICTION`, `scenario reach` now says `Unreachable` |
| 13 | content proved dead is shipped, translated and billed | no — deferred | reproduces (expected) | T5.7, T7.8 — number for number |
| 14 | nothing says two endings are one story's alternation | no — deferred | reproduces (expected) | T5.4; its *cost 2* was incidentally closed by `E-SET-TYPE` |
| 15 | the envelope reports the declaration layer | yes | **PARTIAL** | T4.7, T9.14 closed; T7.13 + T8.4's central ask open — `envelope --values` does not exist |
| 16 | thirteen undischargeable warnings | no — deferred | reproduces (expected) | T4.4, T9.19 — still exactly the 13 baseline `W-UNPROVEN-RELATIONAL` |
| 17 | `lute context` is not a trustworthy account | in part | **CONFIRMED as scoped** | both claimed halves closed (`W-DOMAIN-UNREAD`, `quest.<id>.state`); the four carved-out `lute context` entries T1.6, T3.7, T5.3, T9.15 all still reproduce |
| 18 | stage state is computed, correct, never asserted on | yes | **CONFIRMED** | T2.1, T2.4 CLOSED — `W-EXIT-INERT`, `W-STAGE-ABSENT`, both fail `--deny-warnings` |
| 19 | `lute test` walks the source | no — deferred | reproduces (expected) | T9.7 |
| 20 | `lute run` plays a choice whose guard is false | yes | **CONFIRMED** | T8.5 CLOSED — `E-TRACE-CHOICE`, exit 2, matched enabling control at exit 0 |
| 21 | a malformed state schema cannot be diagnosed | yes | **CONFIRMED** | T3.9, T10.2 CLOSED — nested diagnostic body, author-vocabulary `E-STATE-DECL` at the declaration's own span |
| 22 | two diagnostics contradict the text they point at | yes | **CONFIRMED** | T3.6, T5.6 CLOSED — new `::set` parse message; `&&` narrowing in four slots |
| 23 | no command is both caller-aware and able to point into a component | yes | **PARTIAL** | T6.7 CLOSED; T6.3 and T9.12 still reproduce — §9 rules 1, 2 and rule 4's first leg shipped; rule 4's no-project leg and rule 3's suppression conjunct did not |
| 24 | coverage keyed on guard text | yes | **CONFIRMED** | T9.13 CLOSED — site-keyed, 19 guarded constructs now 19 rows |
| 25 | a refused test prints four words | yes | **CONFIRMED** | T9.11 CLOSED — diagnostic vector printed, three rot modes → three codes |
| 26 | `E-CLIP-OVERLAP` rejects a legal boundary hand-off | yes | **CONFIRMED** | T7.2 CLOSED — every float boundary in the entry's table passes; `spine-a.lute:37` restored to `0.4` |
| 27 | the track is not in the IR | no — deferred | reproduces (expected) | T7.3 — key union character-for-character identical |
| 28 | conditional content costs a dummy `<match on="true">` | no — deferred | reproduces (expected) | T7.6, T8.7; observed incidentally under T7.7 |
| 29 | two characters cannot speak at once | no — deferred | reproduces (expected) | T7.1 — all four constructs, plus the entry's proposed `{over}` remedy |
| 30 | `check-project` and `compile` disagree about a nested manifest | yes | **NOT CONFIRMED** | T1.10 still reproduces — `W-PROJECT-INERT` never fires under `check-project` |
| 31 | scaffolded artifacts rot in silence | yes | **PARTIAL** | T1.9 half CLOSED (mock validation); T10.4 half still reproduces (`lute init`'s README) |
| 32 | the preview tools drop the one bit that matters | yes | **PARTIAL** | T5.9 CLOSED, T2.5 closed on `trace`/`context`, open on `run`; all three of the backlog's own Verify criteria pass |
| 33 | output names things the author cannot look up | yes | **CONFIRMED** | T1.4, T4.10 CLOSED — `@narrator` not `::narrator`; envelope in author vocabulary |
| 34 | frontmatter is boilerplate and nothing can hoist it | yes | **NOT CONFIRMED** | `defaults:` resolves in the document pass and nowhere else — three defects, below |
| 35 | relation names have no did-you-mean | yes | **CONFIRMED** | `E-RELATION-UNKNOWN … did you mean `can_halt`?` |
| 36 | two unrelated `reason` fields in one JSON document | yes | **CONFIRMED** | `grep -c '"reason"'` on a compiled scene = 1; `provenance.explanation`, `injected: true` |
| 37 | the `anchor` default is the one value you may not write | yes | **CONFIRMED** | `W-INJECT-CONFLICT` removed from the product, `--deny` rejects it at exit 2 |
| 38 | a component param cannot use the long form | yes | **CONFIRMED as scoped** | `{ type: string }` accepted; param *defaults* still rejected, which is the backlog's own Do-not-fix |

**Claimed: 26. Confirmed 17, confirmed-as-scoped 2, partial 4, not confirmed 3.
Deferred: 12, all reproducing as designed.**

---

## Step 1 — the shipped baseline

```
$ ./target/debug/lute check-project docs/examples/anseo
ok: docs/examples/anseo (18 file(s), 13 project-wide warning(s))
rc=0

$ ./target/debug/lute check-project docs/examples
ok: docs/examples (47 file(s), 18 project-wide warning(s))
rc=0

$ ./target/debug/lute test docs/examples
34 passed, 0 failed

$ ./target/debug/lute test docs/examples/anseo
31 passed, 0 failed
```

All 18 project-wide warnings under `docs/examples` are `W-UNPROVEN-RELATIONAL`, and **no
corpus document carries a per-file warning** — the four `W-INJECT-CONFLICT` the July run
measured are gone with the code that emitted them.

The numbers are unchanged from `b03c3af`, which is the correct result: a release that
tightens the language and adds eleven diagnostic codes should not move a corpus that was
already correct. What moved is what the tool *says* when it is not.

---

## Step 2 — the seven §13 predictions

| § | prediction | result | verdict |
|---|---|---|---|
| 13.1 | `E-SET-TYPE` — corpus clean; 37 `::set`, 35/2/0 across the three trees | CONFIRMED | CLOSED |
| 13.2 | closed attribute surfaces — corpus clean; census exact; zero `as=`, zero `goto=` | CONFIRMED | CLOSED |
| 13.3 | dead gates / contradictory objectives — corpus clean; 24 tag-anchored gates | CONFIRMED WITH CORRECTION | CLOSED |
| 13.4 | nested manifests — corpus clean; 7 manifests; 3 of 6 trip D-S | CONFIRMED WITH CORRECTION | CLOSED |
| 13.5 | scaffolded mocks — three deliberate reds, one more behind the first | CONFIRMED WITH CORRECTION | CLOSED |
| 13.6 | timeline resolution — corpus clean; `spine-a.lute:37` restorable to `0.4` | CONFIRMED | CLOSED |
| 13.7 | the `LANG-SOFT` seven — `W-INJECT-CONFLICT` −4, §11.1 and §11.2 add none | CONFIRMED | CLOSED |

**No §13 prediction was refuted.** Every count was re-derived independently rather than
copied; three corrections are recorded below and none is a defect in the shipped toolchain.

### §13.1 — `E-SET-TYPE`

Count re-derived twice by different methods. `37` total: **35** in `docs/examples`, **2** in
`conformance`, **0** in `crates/lute-compile/tests/fixtures` — exactly as predicted. The
right-hand-side census is `25 × += 1`, `5 × = true`, `2 × += 2`, `2 × = "blake"`,
`2 × += 300`, `1 × = "halsinDead"` = 37, which is precisely the six literals §13.1 names,
all bare.

Positive control:

```
$ lute check /tmp/Spec13A/settype/scenes/probe.lute --project /tmp/Spec13A/settype
error [E-SET-TYPE] `::set` writes a `string` into `run.shedPressure`, declared `number` (dsl 0.10.0 §3)
```

Matches §13.1's stated wording verbatim, at the right-hand side's own span. Corpus does not
trigger it (baseline above).

*Recount hazard the spec does not mention:* a naive `grep -c '::set{'` on `docs/examples`
returns **37**, not 35, because two hits are prose inside `/* */` comments
(`choice-persist.lute:16`, `investigation/scenes/crime-scene.lute:16`).

### §13.2 — closed attribute surfaces

Census over 53 `.lute` files with comments stripped and attribute *values* masked reproduces
the spec's permitted sets set-for-set: `branch{id}`, `choice{exit,id,into,label,once,value,when}`,
`match{on}`, `when{is,test}`, `otherwise{}`, `hub{id}`. Zero `as=`, zero `goto=`; the single
`goto` text hit is prose at `anseo/scenes/spine-b.lute:101`, inside the block comment opened
at `:98`. All three `once` and all three `exit` sit on hub choices at the six line numbers
§13.2 names, under the three hubs it names — every number matches.

Four positive controls: `E-UNKNOWN-ATTR` fires on an invented attribute, `E-AS-REMOVED`
fires on `as=` and names `lute fix`, and D-L's per-position rule reddens `once=` on a
**non-hub** `<choice>`.

### §13.3 — dead gates and contradictory objectives

**24** gates, tag-anchored: 6 `<quest start=>`, 2 `<quest fail=>`, 16 `<objective done=>`;
27 counting the three `<objective when=>` §5 does not analyse. Matches the spec exactly.
All four declared relations producible. Both new codes fire on constructed cases, and both
of §5.2's documented false-positive guards stay silent — including the "the number domain
is the reals, not the integers" guard (`> 1` alongside `< 2` on one path is **not** flagged).

The `§13.3` regression guard holds: `start="holds(found(toma))"` stays
`W-UNPROVEN-RELATIONAL` at exit 0 with zero `E-QUEST-UNREACHABLE`, because `found` is
asserted at `scenes/stowaway.lute:19`.

**Correction, citations only.** (a) The `awake(vesna)` seed is at
`docs/examples/anseo/world.schema.yaml:17`, not `:16` — line 16 is the `facts:` key itself.
(b) **The prose-comment trap is four times wider than the spec records.** A naive `start=`
text scan yields **10** hits, of which **four** are false, not one:
`hold-the-spine.lute:26` (the one §13.3 names), plus `false-heading.lute:9`,
`hold-the-spine.lute:18` and `what-vesna-carries.lute:9`. A future recount warned off only
`:26` lands on 9, not 6. This is worth carrying forward: §13.3 already records two
independent recounts hitting this trap from opposite sides, and its own warning is
insufficient.

### §13.4 — nested manifests

Seven manifests, none doubly nested, all seven loading clean. All six capability hashes
reproduce the spec's table exactly when measured the way D-S requires — a profile-less,
plugin-less scratch document under each root: `anseo`/`episodes`/`investigation` at
`0678492f…` (identical to the outer root), `showcase` `2a5eaaeb…`,
`plugindef-project` `1235ef58…`, `idola-project` `c7fa83a9…`. **The counter-example the
spec warns about also reproduced:** measuring a real document under `idola-project/` gives
`d91cefe6…`, the wrong basis, exactly as §13.4 records.

Positive control — `W-PROJECT-INERT` fires when a nested manifest does not govern:

```
$ lute compile --all --project /tmp/Spec13B/inert -o …
/tmp/Spec13B/inert/nested/lute.project.yaml: warning [W-PROJECT-INERT] this manifest does not
govern under `--project /tmp/Spec13B/inert` and would have resolved different `identity:`
templates; its settings are not applied to any document. Invoke this root directly to use
them (0.10.0 §7)
```

The corpus does not trigger it. **But see #30 below: this control is also the proof that
`check-project` never emits the code at all.**

**Corrections.** (i) §13.4's `@warm` `E-UNDECLARED-REF` bullet reads as an observation about
the `compile --all --project docs/examples` run described one sentence earlier. That run
aborts at profile resolution after six `E-PROFILE-UNKNOWN` and never reaches per-document
diagnostics; only a *single-document* `compile --project docs/examples` prints it, at
`plugin-def.lute:13:15`. The claim is true; the command implied is wrong. (ii) Unrecorded by
§13.4: compiling `plugindef-project/plugin-def.lute` from the outer root also raises
`E-CONN-EPISODE-ID-DUP` on `demo.s01ep01` against `docs/examples/affinity-reaction.lute` — a
second, independent reason the outer root is not a viable compile target.

### §13.5 — scaffolded mocks

The prediction describes the **pre-fix** state; the fix shipped, so the probe is inverted.
All three `mocks/*.yaml` now carry `file:` and the corpus is green. Both codes were proven
live by stripping `file:` from `/tmp` copies of all three files (`E-MOCK-SUBJECT` ×3) and by
seeding an undeclared path (`E-TRACE-MOCK-UNDECLARED`). The README consequence landed:
`docs/examples/anseo/README.md:97` now reads *"Two things in this tree are wrong on
purpose"*, with both `W-INJECT-CONFLICT` items and the mock item removed and no residue
anywhere under `docs/examples`.

**Corrections.** (i) **The spec contradicts itself about the fix.** §8:777-781 prescribes
`file: ../scenes/cryobank.lute` with `run.shedPressure: 1`; §13.5:1470 prescribes
`../scenes/wake.lute`. The shipped mock follows §8, and the shipped choice is sound —
`cryobank.lute` exists, `run.shedPressure` is declared `{ type: number }`, the mock's own
header comment already pointed at cryobank, and `lute trace` drives it at rc=0. §13.5's
sentence is the stale half. (ii) `E-TRACE-MOCK-UNDECLARED` does not render the string §8
and §13.5 both quote verbatim; it renders ``​`--state run.greeted=…` names a state path not
declared in the resolved schema … (resolved for `../scenes/wake.lute`)``, reusing the
`lute trace --state` CLI-flag idiom for a key read out of a YAML `state:` block. **D-AB** is
satisfied — right file, offending key named, impossible line/column gone — but the wording
is not what the spec promises. `ERGONOMIC`.

### §13.6 — timeline resolution

Count re-derived: **43** `at`/`duration` values across `docs/examples`, 41 with one
fractional digit and 2 with two (`0.75`, `0.35`); zero non-numeric; zero `delay=` anywhere
in the corpus. Matches.

**The acceptance test passes.** `docs/examples/anseo/scenes/spine-a.lute:37` reads
`duration="0.4"`, the `0.8 + 0.4 → 1.2` hand-off on `:38` is legal, and the file is
`ok … (0 warning(s))` at rc=0. Before this release the same value produced `E-CLIP-OVERLAP`
at `:38:5` and `0.35` was the shipped workaround. The workaround is deleted and the author's
intended value is back.

Positive control across all three time-valued slots — `E-TIME-RESOLUTION` fires on a
sub-millisecond clip `at`, on a cross-cutting `duration`, and on `<timeline duration>`; the
corpus triggers it zero times. §10.3 / **D-T** holds: the artifact keeps seconds as JSON
numbers, with raw literals `['0.0','0.3','0.35','0.4','0.5','0.6','0.8','0.9','1.2','1.6']`
and a cursor-derived `at` of exactly `1.2` — no float noise in the file.

### §13.7 — the `LANG-SOFT` seven

**(a) `W-INJECT-CONFLICT`: 4 → 0, by removal, not by editing the corpus around it.** All
four named sites are byte-unchanged and still write `anchor="center"`, which is the code's
sole emission condition. The constant is gone from every crate — the remaining ten grep hits
are doc comments and regression prose — and
`--deny W-INJECT-CONFLICT` is now a usage error at exit 2:
*"unknown diagnostic code … a typo'd `--deny` must not silently protect nothing"*. That is
the strongest available proof the code no longer exists.

**(b) §11.2 adds none — and the corpus fact is stronger than the spec's claim.** The corpus
has **no content line carrying `action=` at all**, so `W-EXIT-INERT` is unreachable on it by
construction. Its only two declared exits (`anseo/scenes/wake.lute:14`,
`bianca-s01ep02.lute:190`) are each the last staging event for their character — verified by
reading the files to EOF, not by silence. Both new rules fire in `/tmp` on T2.1's and T2.4's
exact shapes, at rc=0, so **D-K** holds: neither can redden a green document.

**(c) §11.1 adds none, and the census re-derives to exactly the predicted seven.**
`action`, `anchor`, `emotion`, `mood`, `musicAction`, `vfxType`, `volume`, with the
per-project split as stated. One nuance the spec glosses: `showcase/` "declares none" is
true of *project-level* `enums:` only — the `showcase.pack` plugin, whose files live
physically under `showcase/`, exports the same seven slots. Same names, so neither the count
nor the verdict moves. Positive control: an eighth, unread `reason:` domain appended to a
`/tmp` copy of `anseo/vocabulary.schema.yaml` takes the project from 13 to 14 warnings with
`W-DOMAIN-UNREAD` naming all three discharge paths.

*Ergonomic observation, filed not fixed:* `W-DOMAIN-UNREAD` is a project-wide diagnostic
(**D-V**) but anchors on whichever document sorts first — here
`components/purser-interject.component.lute:1:1` — and the message has to append *"It may be
declared in a schema this document imports rather than in the document itself"* to apologise
for its own position. Not a violation of any §11.1 requirement (`Diagnostic` has no file
field, the same limitation §8 documents), but the position misleads.

---

## Step 3 — the previous log's own probes, re-run

Every entry in the July log whose verdict was `TOOL-DEFECT`, `DOC-WRONG`, `SPEC-WRONG` or
`LANGUAGE-GAP` — **54 of the 111** — was re-run from its verbatim command.

| disposition | n |
|---|---|
| **CLOSED** | **28** |
| **STILL REPRODUCES** | **12** |
| **OUT OF SCOPE** (deferred `DESIGN`) | **14** |
| total | **54** |

None was unmeasurable. Two probes needed a reconstruction note, recorded with them.

### Closed — 28

| entry | issue | old verdict | what closed it |
|---|---|---|---|
| T1.4 | 33 | `TOOL-DEFECT` | author-facing owner rendering; `@narrator`, and the real directive on the same run still renders `::auto` |
| T2.1 | 18 | `TOOL-DEFECT` | `W-EXIT-INERT` at `13:47`, escalates under `--deny-warnings` |
| T2.4 | 18 | `TOOL-DEFECT` | `W-STAGE-ABSENT` ×2 — the departed speaker and the second exit, separately |
| T3.2 | 1 | `TOOL-DEFECT` | `E-SET-TYPE`; `compile` refuses too, so the runtime half closes by construction; `E-SET-OP-TYPE` precedence intact |
| T3.6 | 22 | `TOOL-DEFECT` | new `::set`-body parse message; the non-compiling `when==` suggestion is gone |
| T3.9 | 21 | `TOOL-DEFECT` | the schema error's body is nested into `E-USES-PARSE`, in text and in `--json` `related`; 32 identical lines became one with `(+22 more callers)` |
| T3.13 | 10 b | `DOC-WRONG` | `directives.md` rewritten to "Content additionally uses … in scenes as well as quests" |
| T4.5 | 12 | `TOOL-DEFECT` | `E-QUEST-UNREACHABLE` with a three-clause body; `scenario reach` now prints `Unreachable` |
| T4.8 | 10 f | `DOC-WRONG` | `runtime-contract.md:22` qualified; the fact-query rule stated at `:28-34` |
| T4.10 | 33 | `TOOL-DEFECT` | `check_quest_guard_defassign` and the internal `T11` label gone; author vocabulary |
| T5.5 | 10 e | `DOC-WRONG` | both false halves removed from `index.mdx:251-256` |
| T5.6 | 22 | `TOOL-DEFECT` | `&&` narrowing in quest `fail=`, `done=`, `start=` and content-line `when=`; removing the guard restores `E-MAYBE-UNSET` |
| T6.7 | 23 | `TOOL-DEFECT` | §9 rule 3's first conjunct — the standalone leg leads with `E-COMPONENT-PARSE` |
| T6.8 | 10 g | `DOC-WRONG` | the whitelist named and quoted in the same block |
| T6.10 | 3 | `TOOL-DEFECT` | `loc export` emits one row per expansion with the caller-derived `lineId`; component null rows 2 → 0; `lute tag` remedy now real |
| T7.2 | 26 | `TOOL-DEFECT` | integer-millisecond internals; every float boundary in the entry's 10-row table now passes; `E-TIMELINE-DURATION` prints `1.2`, not `1.2000000000000002` |
| T7.7 | 10 a | `DOC-WRONG` | qualifying clause at `branch-match-when.md:106` plus a six-line exception paragraph |
| T7.10 | 10 h | `DOC-WRONG` | **the tool was fixed, not the doc** — `trace` now renders the shot heading from the IR; `tracing.md:44` untouched |
| T7.16 | 10 i | `DOC-WRONG` | doc corrected to the entry's own proposed wording; compiler correctly *not* renumbered |
| T8.2 | 6 | `TOOL-DEFECT` | `E-UNKNOWN-ATTR` on 6/6 logic constructs and both `<choice>` positions; `E-AS-REMOVED`; `lute fix` performs the rename |
| T8.5 | 20 | `TOOL-DEFECT` | `lute run` refuses with `E-TRACE-CHOICE` at exit 2; enabling control plays the arm at exit 0 |
| T8.6 | 10 d | `DOC-WRONG` | `evalSlot(o.when, o.expr, …)` / `evalSlot(a.test, a.expr, …)`; old line 190 retracted in place |
| T9.6 | 12 | `TOOL-DEFECT` | `E-OBJECTIVE-CONTRADICTION` naming both objective ids and the path; domain delimited by two negative controls |
| T9.8 | 2 | `TOOL-DEFECT` | `E-TEST-KEY` on unknown `expect:` and top-level keys with did-you-mean; `E-TEST-NO-EXPECT` |
| T9.11 | 25 | `TOOL-DEFECT` | the `TraceExit::Refused(diags)` vector is printed; three rot modes → three codes; messages render YAML key spellings |
| T9.13 | 24 | `TOOL-DEFECT` | site-keyed coverage; 19 guarded constructs render as 19 rows |
| T10.1 | 10 c | `DOC-WRONG` | `state-model.md:26` corrected plus the nesting rule in prose and two companion copies |
| T10.2 | 21 | `TOOL-DEFECT` | `E-STATE-DECL` in author vocabulary at the declaration's own span (`9:3`, was `1:1`) |

### Out of scope — 14

All fourteen belong to the twelve deferred `DESIGN` issues. Each was re-run and each still
reproduces, which is the correct outcome.

| entry | deferred issue | verdict now |
|---|---|---|
| T4.4 | 16 | `TOOL-DEFECT` |
| T5.4 | 14 | `LANGUAGE-GAP` |
| T5.7 | 13 | `SPEC-WRONG` |
| T6.2 | 5 | `SPEC-WRONG` |
| T6.11 | 4 | `SPEC-WRONG` |
| T7.1 | 29 | `LANGUAGE-GAP` |
| T7.3 | 27 | `SPEC-WRONG` |
| T7.8 | 13 | `TOOL-DEFECT` |
| T8.1 | 8 | `LANGUAGE-GAP` |
| T8.3 | 7 | `LANGUAGE-GAP` |
| T9.3 | 8 | `LANGUAGE-GAP` |
| T9.4 | 9 | `LANGUAGE-GAP` |
| T9.7 | 19 | `SPEC-WRONG` |
| T9.19 | 16 | `SPEC-WRONG` |

Two notes worth carrying:

- **T4.4** part (c)'s original `awake` demonstration went stale only because
  `world.schema.yaml` gained a `facts: - "awake(vesna)"` seed. `knows` reproduces it exactly:
  `trace scenes/cryobank.lute --fact "knows(toma, shed_sequence)"` is silent, the same fact
  with the same `--project` on `quests/hold-the-spine.lute` emits
  `W-TRACE-MOCK-UNPRODUCIBLE`, and `check-project` calls `knows` producible at
  `what-vesna-carries.lute:22`. `trace`'s `producible()` is still document-local.
- **T5.4**'s *cost 2* was incidentally closed by `E-SET-TYPE` —
  `::set{run.ending = "shed-with-modle"}` against a declared enum is now an error with a
  did-you-mean where it was `ok` at exit 0 — and its *cost 3* by T5.6's narrowing. The proxy
  now costs three items rather than five. *Cost 1* — nothing joins a `::set` to the adjacent
  `::end`, so a deliberate mismatch is `ok` at exit 0 — is what keeps #14 a `LANGUAGE-GAP`.

---

## Still reproduces — 12

**Eight sit on issues `0.10.0` claimed outright.** Four are `lute context` defects that spec
§2.3 explicitly carved out of #17 while taking its other two halves; those are recorded here
rather than as out-of-scope, because the issue *was* claimed and a reader of the changelog
will not know where the carve-out runs.

### 1. T1.10 — `W-PROJECT-INERT` never fires under `check-project` · issue **30** · `TOOL-DEFECT`

Two of the entry's three complaints closed: `E-IDENTITY-TEMPLATE` now names the offending
manifest's own path, and `compile --all` now reads and refuses over a broken nested manifest
at rc=1, agreeing with `check-project`. **The entry's headline sentence survives intact.**

```
$ lute check-project /tmp/ProbeT12/nest3
ok: /tmp/ProbeT12/nest3/inner/scenes/opening.lute (0 warning(s))
ok: /tmp/ProbeT12/nest3/scenes/opening.lute (0 warning(s))
ok: /tmp/ProbeT12/nest3 (2 file(s), 0 project-wide warning(s))
rc=0
```

— on the exact tree whose `compile` warns that the inner manifest is inert. The corpus
control makes the cost concrete: a `/tmp` copy of `docs/examples` with an `identity:` block
added to the **outer** manifest checks green —

```
$ lute check-project /tmp/ProbeT12/ex | grep -E 'PROJECT-INERT|^ok: /tmp/ProbeT12/ex '
ok: /tmp/ProbeT12/ex (47 file(s), 18 project-wide warning(s))
# zero W-PROJECT-INERT lines; the 18 are the baseline W-UNPROVEN-RELATIONAL
```

— while `compile --all` on the *same tree* emits six:

```
$ lute compile --all --project /tmp/ProbeT12/ex -o … | grep PROJECT-INERT
/tmp/ProbeT12/ex/anseo/lute.project.yaml: warning [W-PROJECT-INERT] this manifest does not
govern under `--project /tmp/ProbeT12/ex` and would have resolved different `identity:`
templates; its settings are not applied to any document. Invoke this root directly to use
them (0.10.0 §7)
… (5 more: episodes, idola-project, investigation, plugindef-project, showcase)
```

Every Anseo artifact built from the outer root silently changes its `lineId`s and
`check-project` stays green through it — verbatim what T1.10 filed. The rule exists and
fires; `check-project` is the one surface it does not reach, and `check-project` is the
surface CI runs and the website's *"what checks clean is exactly what compiles"* guarantee
names. Secondary: single-file `compile --project <outer>` emits no `W-PROJECT-INERT` either;
only `--all` does.

### 2. T3.10 — a mock's unknown key is still silently discarded · issue **2** · `TOOL-DEFECT`

`0.10.0` gave `mocks/*.yaml` a required `file:` and gave `*.test.yaml` a closed key set. The
two never met.

```
$ lute trace scenes/cryobank.lute --mock /tmp/ProbeT3/mocks/bogus.yaml   # bogus.yaml uses `selections:` for `choose:`
trace: scenes/cryobank.lute  (seeds: 1 paths, 0 facts; 0 selections)
  …
  <branch whoWakes>   eligible: wakeToma, wakeIlsabet, wakeNobody   -> wakeToma (auto)
trace complete: 1 decision; choices 1/3 (whoWakes)
rc=0

$ lute run /tmp/ProbeT3/out/cryobank.json --mock /tmp/ProbeT3/mocks/bogus.yaml
run incomplete
rc=3
```

`trace` exits **0** having picked the first arm itself, not the mock's. Placing the same
file at `mocks/bogus.yaml` draws nothing from the checker either: `ok: . (18 file(s), 13
project-wide warning(s))`, byte-identical to baseline.

Positive controls prove both halves of the rule exist and simply do not meet: a mock with no
`file:` draws `E-MOCK-SUBJECT` from `check-project`, and the *same key name* in a
`*.test.yaml` draws
`error [E-TEST-KEY] unknown top-level key `selections` … (legal: accept, accepts, choose, events, expect, facts, file, state)`.
The fix is mechanical — `E-TEST-KEY`'s legal-key list is the model and `E-MOCK-SUBJECT`
proves the mock loader already has a diagnostic channel.

### 3. T6.3 — `lute check <component>` with no project still prints a bare `ok` · issue **23** · `TOOL-DEFECT`

Spec §9 rule 4 is unambiguous: *"With no caller in scope — **no project resolved**, or no
document in the project imports this component — the check MUST NOT report a bare `ok`."*

```
$ lute check components/interject.component.lute        # inside the project root, no --project
ok: components/interject.component.lute (0 warning(s))
rc=0

$ cd /tmp/ProbeT6/iso2 && lute check components/interject.component.lute   # no manifest anywhere on the path
ok: components/interject.component.lute (0 warning(s))
rc=0
```

— on a component that cannot work with five of its six callers. Byte-identical to the
2026-07-31 output. The positive control shows the rule and its diagnostic do exist:

```
$ lute check components/interject.component.lute --project /tmp/ProbeT6/t6/proj-nocaller
components/interject.component.lute:1:1: warning [W-COMPONENT-UNVERIFIED] no document in the
resolved project imports component `interject`, so this verdict covers only its own
frontmatter and body against its OWN `uses:` … (dsl 0.10.0 §9, D-W)
```

**but only when `--project` is passed.** The manifest is never auto-discovered, so the
default invocation an author types is the one still returning the meaningless green. §9's
rules 1, 2 and rule 4's first leg did land, and they close the *localisation* half of the
entry — the caller diagnostic now carries `interject.component.lute:9:30` as a secondary
location and N identical caller reports roll up to `(+4 more callers)`.

Two further §9 gaps filed with it: rule 3's *"and suppresses the `E-UNDECLARED-REF` it
causes"* conjunct is unimplemented (both legs still emit it alongside `E-COMPONENT-PARSE`,
and the caller leg adds a cascaded `E-COMPONENT-ARG` — six errors for two authored
mistakes); and `E-UNDECLARED-REF` on an attribute-position component param reports position
`0:0`, where every other position the binary prints is 1-indexed.

### 4. T9.9 — a never-written path fails against its own rendered value · issue **2** · `TOOL-DEFECT`

```
FAIL  probe/tests/t99.test.yaml  (probe/tests/../../scenes/cryobank.lute)
      state run.shedPressure: expected "<never written>", got "<never written>"

0 passed, 1 failed
rc=1
```

Byte-for-byte the July output. The `<never written>` sentinel is a rendering applied only to
the *actual* side, so `Option<String>::None` can never equal the string it prints as, and
the miss line declares two values different while printing them equal. Three candidate
spellings for the underlying intent were probed and none exists (`unset`, `null`, `~`).
`0.10.0` shipped `E-TEST-KEY` and `E-TEST-NO-EXPECT` for #2 and did not touch the
comparison. This is the protocol's highest-priority category — a diagnostic that says X when
the problem is Y — and the fix is independent of everything #2 did ship.

### 5. T9.10 — `expect:` still has exactly three keys · issue **2** · `TOOL-DEFECT`

All six of the entry's missing expectations are still inexpressible, and `0.10.0`'s own new
diagnostic is the proof — it recites the closed set in every rejection:

```
--- expect: transcriptOmits ---  error [E-TEST-KEY] unknown `expect:` key `transcriptOmits` in a `*.test.yaml` (legal: exit, state, transcriptContains)
--- expect: eligible ---         error [E-TEST-KEY] unknown `expect:` key `eligible` …
--- expect: end ---              error [E-TEST-KEY] unknown `expect:` key `end` …
--- expect: reason ---           error [E-TEST-KEY] unknown `expect:` key `reason` …
--- expect: quest ---            error [E-TEST-KEY] unknown `expect:` key `quest` …
--- expect: questStatus ---      error [E-TEST-KEY] unknown `expect:` key `questStatus` …
--- expect: facts ---            error [E-TEST-KEY] unknown `expect:` key `facts` …
```

`exit, state, transcriptContains` — the same three the July log recorded. **This is the
sharpest illustration of what #2 actually bought:** closed-key validation landed, the
missing expectation kinds did not. `eligible:` is the one the entry ranked first, and it is
the one T9.18 turns on. One thing did improve, second-order: an author reaching for a
missing key now gets a hard error naming the closed set instead of a green test.

### 6. T9.12 — `lute trace` and `lute check` still disagree on a component · issue **23** · `TOOL-DEFECT`

§9 states its own purpose outright: *"Rule 4 is what makes T9.12's advice followable:
`lute trace` on a component and `lute check` on the same file stop disagreeing."* They still
disagree, in the same direction, on the same file, with the same two messages:

```
$ lute trace docs/examples/anseo/components/purser-interject.component.lute
…component.lute:19:12: error [E-COMPILE-EXPAND] `@pressure` names no known def body (gate should have caught this)
trace refused: …component.lute has check error(s) — run `lute check` first
rc=1

$ cd docs/examples/anseo && lute check --project . components/purser-interject.component.lute
ok: components/purser-interject.component.lute (0 warning(s))
rc=0
```

The advice *"run `lute check` first"* still points at the tool that says `ok`, and the
internal invariant assertion *"(gate should have caught this)"* is still shipped in an
author-facing diagnostic. §9 delivered the caller-less half; Anseo's component **is**
imported at `cryobank.lute:14`, so rule 4 legitimately reports `ok` and the fault lives in
the untouched `trace` path. The suite still covers 17 of 18 documents with zero coverage
rows for the component. Worse than unchanged: T9.13's new untested-document report reads
*"every testable document under this root is named by at least one test"* — **green, because
the component is excluded from "testable"** rather than reported as unreachable, which
normalises exactly the gap this entry filed.

### 7. T9.18 — the mutation test still passes · issue **2** · `TOOL-DEFECT`

The most expensive reproduction in this log. Three mutations on a `/tmp` copy of the corpus:

```
############ M1: delete when= from haltTheSequence ############
<choice id="haltTheSequence" label="Halt the sequence">
ok: . (18 file(s), 13 project-wide warning(s))     ← BYTE-IDENTICAL to baseline
31 passed, 0 failed
coverage diff vs baseline: IDENTICAL (byte-for-byte), incl. `branch/hub theCorrection: 4/4 chosen`

############ M2: weaken invalidateTheVoyage ############
<choice id="invalidateTheVoyage" label="…" when="holds(awake(ilsabet))">
ok: . (18 file(s), 13 project-wide warning(s))     ← BYTE-IDENTICAL
31 passed, 0 failed                                 ← coverage BYTE-IDENTICAL
```

M1 deletes the guard from `haltTheSequence`, one of the two levers deciding which ending the
prologue reaches, so it is offered to a crew with nobody who can halt anything — and nothing
observable anywhere in the toolchain changes. All three of the entry's controls fire
(C1, C2, C3 each give `30 passed, 1 failed`), so the blind spot is real and not a broken
harness. M3 improved only in that its coverage row now changes in place at a stable span
rather than appearing as a new row; it still ships at 31/31.

**And the one-key fix the entry named is the one T9.10 shows was not shipped:**

```
error [E-TEST-KEY] unknown `expect:` key `eligible` in a `*.test.yaml` (legal: exit, state, transcriptContains)
```

#2's shipped work cannot reach M1 or M2 even in principle.

### 8. T10.4 — `lute init`'s README still hard-codes the placeholder · issue **31** · `TOOL-DEFECT`

Issue 31's other half genuinely closed — `check-project` now validates `mocks/*.yaml`,
catching both the dangling subject and the orphaned state path, each positioned on the mock
file itself. T10.4's half did not:

```
$ grep -n 'opening.lute' README.md          # the generated one
13:lute check scenes/opening.lute
16:lute trace scenes/opening.lute --mock mocks/playthrough.yaml

$ lute check scenes/opening.lute            # after replacing the placeholder, as T1.3 did
lute: cannot read scenes/opening.lute: No such file or directory (os error 2)
# exit 2
$ lute trace scenes/opening.lute --mock mocks/playthrough.yaml
lute: cannot read scenes/opening.lute: No such file or directory (os error 2)
# exit 2
```

Both commands are hard-coded to the one file the scaffold's own last content line invites
you to replace. The backlog's fix was explicit and binary — *"write them as templates
(`lute check scenes/<your-scene>.lute`), or, strictly better and free, do not write the
README's 'Next steps' section at all"* — and neither was done. The entry's closing sentence
still holds verbatim: *"The class is not closed: the next `lute init` writes the same two
lines."* Mitigation that changes the severity and not the verdict: the sibling mock now
errors in the same tree (`E-MOCK-SUBJECT`), so a project that has replaced the placeholder
is no longer entirely green.

### 9–12. The #17 `lute context` carve-out — T1.6, T3.7, T5.3, T9.15 · all `TOOL-DEFECT`

Spec §2.3 is explicit: *"**#17**'s `lute context` defects are excluded while its
`W-DOMAIN-UNREAD` and its `E-MAYBE-UNSET` message are §11.1 and §12.2."* **Both claimed
halves closed.** All four `lute context` probes still reproduce. They are recorded as
reproductions rather than as out-of-scope because #17 appears in the claimed 26 and the
carve-out is one clause deep in the spec.

- **T1.6** — all five gaps intact: no statement of the content-line form
  `@speaker{attrs}: text`; `grep -c '"code"'` over the JSON surface is `0` and `lute tag` is
  unmentioned; no frontmatter and no `uses:`; no `## ` heading representation; `enums (0):`
  still sits unexplained above `projectEnums (7):`. Same 14 top-level JSON keys, same
  `--help` purpose clause.
- **T3.7** — `directives (9):` = `auto, bg, camera, cut, end, music, sfx, vfx, video`. The
  four built-in `::`-directives (`set`, `assert`, `retract`, `use`) are still absent, and
  `::end` being listed rules out "staging only" as the explanation. The same run emits
  `stateSchema (3)`, `relations (4)`, `facts (1)`, `rules (1)`, `projectEnums (7)`,
  `components (1)`, so the surface is live and the omission is specific.
- **T5.3** — `W-DOMAIN-UNREAD` fires and closes the `check-project` half by the exact remedy
  this entry proposed (13 → 14 warnings when an unread `reason:` domain is declared). But
  `lute context scenes/bridge.lute` still prints `reason: bridge-reached, shed-with-module`
  inside `projectEnums (8)` with no mark, the artifact still ships
  `{"name":"reason","members":[…]}` in its self-describing `enums` array, and
  `::end{reason="not-a-declared-member"}` is still `ok` at exit 0. Two surfaces still assert
  an enforced domain that enforces nothing.
- **T9.15** — two of three parts closed: `E-MAYBE-UNSET` now says the path is
  engine-populated and names both remedies, and `--json`'s `stateSchema` carries the quest's
  own `quest.*` rows. Still open: `lute context` on a **scene** reports
  `reservedQuestPaths = []` and zero `quest.*` rows in a project declaring six quests, while
  `when="quest.probeQuest.objectives.arrive.done"` on a scene content line checks clean
  project-wide. The key reports paths the document already *references* and so cannot be used
  to discover them.

---

## New this run: issue #34, `defaults:`, is broken outside the document pass

This did not come from the 54. Issue #34's source entries (T7.12, T9.5) are `ERGONOMIC`, so
the issue had no probe in the re-run set; it was probed separately because the 38-issue table
needs a row for it. **It is the worst finding in this log.**

`defaults:` is one of the six `LANG` changes. The document pass implements §6 correctly on
every rule reachable through it — the closed set with did-you-mean (§6.1), whole-value
override and present-but-empty (§6.2), per-kind legality for quests (§6.3), `{prefix}`
participation (§6.4), manifest-relative default paths against document-relative authored
ones (**D-Y**), and no absolute-path leak into `capabilityVersion` or the artifact
(**D-Z**). A scene whose entire frontmatter is `episode: 1` resolves `kind`, `character`,
`season` and both schema imports from the manifest and compiles to
`anseo.s01ep01.vesna_0010`.

**And then nothing else in the toolchain reads it.**

**(a) `defaults: kind:` reaches component files, contradicting §6.3 verbatim.** §6.3: *"a
default of `kind:` or `character:` never reaches one."* A/B/C isolation on a `/tmp` copy of
Anseo:

```
# A — defaults WITHOUT `kind: scene`
ok: …/components/purser-interject.component.lute (0 warning(s))
# B — same corpus, `kind: scene` added back to defaults
…component.lute:1:1: error [E-META-MISSING] required meta key `episode` is missing
…component.lute:1:1: error [E-META-UNKNOWN-KEY] unknown top-level meta key `component` … did you mean `components`?
…component.lute:1:1: error [E-META-UNKNOWN-KEY] unknown top-level meta key `params` …
…component.lute:19:12: error [E-UNDECLARED-REF] `@pressure` is not a declared def (dsl §8.1)
failed: … (4 error(s), 0 warning(s))
# C — control, HEAD corpus, no defaults: block
ok: … (0 warning(s))
```

One defaulted `kind: scene` turns every component file in the root into a four-error
document.

**(b) The project-wide connectivity walk ignores `defaults:` — the scene graph goes empty,
silently.** Two scenes and one `after:`:

```
$ lute check-project /tmp/Complement/conn
ok: /tmp/Complement/conn (2 file(s), 0 project-wide warning(s))
rc=0
$ lute scenario /tmp/Complement/conn
project root: /tmp/Complement/conn
  (no scene/quest nodes)
```

The byte-identical control with the three keys written per document produces the expected
two-layer graph with its `visited` edge. A single-variable matrix isolates it: `defaults.kind`
alone, `defaults.character` alone and `defaults.season` alone each empty the graph. A
`defaults:` block per se is innocent — `defaults: contentLang: en` leaves the graph intact.
It is specifically the keys that determine a node's kind and canonical key. **On a small
project this fails silently at `ok`.** On the real corpus it fails loudly, which is the
luckier outcome.

**(c) `lute test` does not apply `defaults:` at all.**

```
$ lute check-project /tmp/Complement/testdef | tail -1
ok: /tmp/Complement/testdef (1 file(s), 0 project-wide warning(s))
$ lute test /tmp/Complement/testdef
FAIL  …/tests/one.test.yaml
      trace refused:
        …/scenes/one.lute:1:1: error [E-KIND-MISSING] required frontmatter key `kind` is missing …
        …/scenes/one.lute:1:1: error [E-META-MISSING] required meta key `character` is missing
        …/scenes/one.lute:1:1: error [E-META-MISSING] required meta key `season` is missing
0 passed, 1 failed
```

Every test fails on documents `check-project` calls `ok`. (`trace`, `compile`, `context`,
`doctor` and `check --project` all honour `defaults:`; `scenario` and `test` do not.)

**The backlog's own Verify, run in full.** A copy of the Anseo corpus with `defaults:`
declaring `kind`, `character`, `season`, `uses` and `luteVersion`, and those lines deleted
from every document — exactly the hoist #34 exists to enable:

```
HEAD  scenes frontmatter key lines: 66   quests: 24
with defaults:                     23          12          ← the ergonomic win is real

$ lute check-project /tmp/Complement/anseoD | tail -1
failed: /tmp/Complement/anseoD (18 file(s), 16 project-wide error(s), 2 project-wide warning(s))
$ lute scenario /tmp/Complement/anseoD | head -4
project root: /tmp/Complement/anseoD
  topological layers:
    layer 0: quest(unmoored), quest(whatVesnaCarries), quest(whoWakes)
    layer 1: quest(falseHeading), quest(manifestGap)          ← not one scene node
$ lute test /tmp/Complement/anseoD | tail -1
0 passed, 31 failed
```

What holds: `wake.lute`'s divergent single import still overrides correctly, and all 168
scene `lineId`s are byte-identical before and after
(`md5 696cdededa58f2e242ce4cc9f1639114` both sides) — so the *identity* semantics §6.4
specifies are right. Everything downstream of the document pass is not.

Verdict `TOOL-DEFECT` rather than `SPEC-WRONG`: in all three cases the spec is unambiguous
and the implementation does not match it.

*Cosmetic, same issue:* `E-DEFAULTS-KEY` has no sentence break before *"The defaultable set
is closed"* when no suggestion is emitted, and `defaults.mode` suggests `pov`, which is not
a plausible confusion and buries §6.1's real reason for excluding `mode`.

---

## Other defects filed, not fixed

Each was found while re-running a probe, is outside that probe's verdict, and is recorded so
it is not lost.

1. **`loc export` is now expansion-scoped, so an unreachable `<match>` arm leaves the
   localization bundle with no warning.** Anseo's component `<otherwise>` line
   *"Allocation is nominal."* appears in **zero** export rows, because the corpus's single
   `::use` passes a literal `pressure="rising"`. Confirmed as reachability-scoping and not
   loss: a two-caller fixture exports both arms. This is a side effect of #3's fix and it
   interacts with the deliberate imperfection `docs/examples/anseo/README.md` item 1
   documents. `ERGONOMIC`.
2. **`E-UNDECLARED-REF` on an attribute-position component param reports position `0:0`**,
   both standalone and as §9 rule 1's secondary location. Every other position the binary
   prints is 1-indexed. `TOOL-DEFECT`.
3. **`lute trace … --project docs/examples` emits five `E-PROFILE-UNKNOWN` lines before the
   transcript**; the same run without `--project` is silent. `TOOL-DEFECT`.
4. **`compile` prints neither `W-EXIT-INERT` nor `W-STAGE-ABSENT`**, so a compile-only
   pipeline gets no signal for either of §11.2's new rules. Same shape as #30's residual.
   `TOOL-DEFECT`.
5. **`lute run` still prints an `"exit":true` sprite record as a bare `sprite`** — T2.5's
   third renderer, outside #32's Fix. `ERGONOMIC`.
6. **`envelope --values` does not exist** (`error: unexpected argument '--values'`;
   `envelope --help` shows no options), so #15's headline acceptance test cannot be run at
   all. Related: `when="run.shedPressure >= 3"` in `spine-b` checks `ok` with 0 warnings
   while `when="1 == 2"` at the same column draws `E-ARM-DEAD`. `ERGONOMIC`.
7. **Spec citation, §13.3:** `world.schema.yaml:16` should be `:17`; and the prose-comment
   trap has four instances, not one. `SPEC-WRONG` (citations only; the substantive
   prediction holds).
8. **Spec contradiction, §8 vs §13.5:** the prescribed mock subject differs
   (`cryobank.lute` vs `wake.lute`). The shipped mock follows §8 and is sound; §13.5's
   sentence is stale. `SPEC-WRONG`.
9. **Spec omission, §13.4:** the `@warm` bullet implies a command that never emits it, and
   `E-CONN-EPISODE-ID-DUP` on `demo.s01ep01` is unrecorded. `SPEC-WRONG`.
10. **`E-TRACE-MOCK-UNDECLARED` does not render the string §8 and §13.5 both quote**,
    reusing the `--state` CLI-flag idiom for a YAML key. `ERGONOMIC`.
11. **`W-DOMAIN-UNREAD` anchors on an arbitrary first-sorted document** and its message
    disclaims its own position. `ERGONOMIC`.
12. **`E-CEL-PARSE` on an unprompted `when==`** still gives the generic *"not a valid
    condition expression"* with a stale `(dsl 0.4 §8.1)` tag and no `::set` hint. Not a
    reopening of T3.6 — nothing walks an author there any more. `ERGONOMIC`.

---

## What I could not measure

Stated explicitly, because a log that does not name its gaps invites the next reader to
assume there are none.

1. **`envelope --values`** — #15's central ask. The flag does not exist, so the acceptance
   test the backlog wrote for it cannot be run either way. Recorded as unshipped rather than
   as failing.
2. **The 19 `ERGONOMIC` and `DOC-GAP` entries outside the 54, plus the one `AUTHOR-ERROR`
   the backlog put on no-action.** Step 3's scope is the four verdicts the assignment names
   (74 verdict-bearing entries − 54 = 20). The eight claimed issues carried only by
   `ERGONOMIC`/`DOC-GAP` entries were probed separately at issue level (rows 11, 15, 32, 34,
   35, 36, 37, 38 of the table); the individual entries behind them were not each re-run.
3. **`docs/examples/anseo/scenes/opening.lute`** does not exist — Anseo is now eleven named
   scenes — so T1.6's probe was re-run on `bridge.lute`, the same project with the same
   both-schema `uses:` shape. T5.3's `scenes/p1.lute` was likewise a scratch name; the
   delivered file is `scenes/bridge.lute`.
4. **T2.4's project-level run needed `episode: 99`** rather than the log's `episode: 2`,
   because the corpus grew and `cryobank.lute` now owns `anseo.s01ep02`
   (`E-CONN-EPISODE-ID-DUP`). Single-file runs used the log's value unchanged.
5. **T4.4 part (c)'s original `awake` demonstration is stale** because `world.schema.yaml`
   gained a `facts:` seed. `knows` reproduces the same divergence exactly and was used
   instead.
6. **Nothing was measured under `--release`, under CI, or on a second platform.** All
   figures are a debug build on darwin/arm64.
7. **The 12 deferred issues were confirmed to reproduce, not analysed.** Whether any has
   *changed shape* since July — beyond the two incidental closures noted under T5.4 — was
   not assessed.

---

## Counts

| measurement | value |
|---|---|
| previous-log probes re-run | **54** (33 `TOOL-DEFECT`, 9 `DOC-WRONG`, 6 `SPEC-WRONG`, 6 `LANGUAGE-GAP`) |
| closed | **28** |
| still reproduces | **12** — 8 on fully-claimed issues, 4 on #17's carve-out |
| out of scope (deferred) | **14** |
| unmeasurable | **0** |
| §13 predictions checked | **7** — 7 confirmed, 0 refuted, 3 with citation corrections |
| positive controls constructed | **13** across §13, plus further controls in Step 3; all fired |
| backlog issues claimed by `0.10.0` | **26** — 17 confirmed, 2 confirmed as scoped, 4 partial, 3 not confirmed |
| backlog issues deferred | **12** — all reproduce, as designed |
| corpus documents changed by this run | **0** |

The July log's central reading was *"the analysis overwhelmingly exists and the reporting
layer loses it."* `0.10.0` was scoped against exactly that and largely delivered it: eleven
new codes, nine corrected documentation statements, three broken silences, and five of six
`LANG` changes clean on first measurement. The three misses share one shape and it is a new
one — **a rule that was implemented on one surface and not wired into the others.**
`W-PROJECT-INERT` reaches `compile` and not `check-project`. `E-TEST-KEY` reaches
`*.test.yaml` and not `mocks/*.yaml`. `defaults:` reaches the document pass and not the
connectivity walk or `lute test`. §9's rule 4 reaches `--project` and not the bare
invocation. That is a different failure mode from `0.9.0`'s and it is the one to watch in
`0.11.0`.
