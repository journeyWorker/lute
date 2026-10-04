---
status: Implemented
---

# Phase 3 inventory — evidence

## summary

Inventory complete: static checker verdicts are proof-oriented May/Must analyses; trace/play/test are concrete bounded witnesses; calendar is bounded grid execution; scenario connectivity is tri-state structural proof; beats combines checker diagnostics with ladder heuristics; differential is conformance comparison over concrete cases. The main evidence gap is that D9’s vocabulary is not yet a first-class serialized field across these surfaces.

## files

```json
[
  {
    "path": "docs/design/architecture-direction.md:206-220",
    "description": "D9 evidence vocabulary and author-constraint requirements."
  },
  {
    "path": "docs/proposals/scenario-dsl/0.20.0.md:7-40,78-126,128-170,193-200",
    "description": "May/Must fact verdict semantics and soundness decisions."
  },
  {
    "path": "docs/proposals/scenario-dsl/0.23.0.md:20-50,83-86",
    "description": "beats, calendar, and scenario knowledge command contracts."
  },
  {
    "path": "docs/proposals/scenario-dsl/0.31.0.md:47-62",
    "description": "Objective-window, slot-contention, and advance-cascade rules."
  },
  {
    "path": "crates/lute-check/src/fact_env.rs:23-31,218-220,543-596,1044-1055",
    "description": "May over-approximation, Must under-approximation, and guaranteed derivation."
  },
  {
    "path": "crates/lute-check/src/fact_check.rs:55-59,590-598,673-676",
    "description": "W-FACT-GUARANTEED and relational outcome classification."
  },
  {
    "path": "crates/lute-check/src/clock_positions.rs:1164-1177,1422-1425",
    "description": "Clock-window and cascade diagnostics."
  },
  {
    "path": "crates/lute-check/src/connectivity.rs:1758-1791,1824-1860",
    "description": "Tri-state structural reachability and formula cap."
  },
  {
    "path": "crates/lute-cli/src/cmd_scenario/reach.rs:49-127",
    "description": "Scenario human verdict rendering."
  },
  {
    "path": "crates/lute-cli/src/scenario_fmt.rs:63-76",
    "description": "Scenario JSON reach tokens."
  },
  {
    "path": "crates/lute-cli/src/play/calendar.rs:1-45,900-1035,1703-1829,2170-2265",
    "description": "Calendar bounded-grid model, cell outcomes, text and JSON fields."
  },
  {
    "path": "crates/lute-cli/src/beats_cmd.rs:1-25,37-45,445-475,607-721",
    "description": "Beats verdict attachment and text/JSON rendering."
  },
  {
    "path": "crates/lute-trace/src/report.rs:20-32,454-485",
    "description": "Trace exit/evidence model and JSON report contract."
  },
  {
    "path": "crates/lute-cli/src/play_expect.rs:1-22,66-75",
    "description": "Play expectation semantics and judging boundary."
  },
  {
    "path": "crates/lute-cli/src/testcmd.rs:1-46",
    "description": "Scenario-test witness semantics and incomplete handling."
  },
  {
    "path": "crates/lute-cli/src/differential.rs:1-23,47-64,675-720,1521-1535",
    "description": "Differential concrete-case selection, observations, skips, and output."
  }
]
```

## architecture

D9 currently spans several internal analyses: checker May/Must proofs; connectivity fixed-point tri-state; clock/grid bounded execution; deterministic trace/play witnesses; and runtime differential comparison. They are not unified into one evidence field: most render legacy diagnostic codes/text or tool-specific JSON fields, while only some explicitly say Unknown/bounded scope.

## report

## Evidence vocabulary used here

D9 names **Proven** for sound static proofs, **Witnessed** for a concrete play/trace reproduction, **Bounded** for an explicitly finite scope, **Heuristic** for conservative/informational selection guidance, and **Unknown** where the analysis cannot decide. D9 explicitly requires that “no path found within bounds” never be rendered as “no path exists” [docs/design/architecture-direction.md:206-220]. The implementation generally follows this for calendar and scenario, but several legacy checker messages use absolute wording.

## 1. Checker `impossible` / `guaranteed` / `possible` (DSL 0.20.0)

**Command/code.** `lute check` performs per-file checking; `lute check-project` adds the project-wide fact envelope and cross-file reconciliation. The fact analysis is in `lute-check`’s `FactEnv`, `MaySet`, `FactMust`, and `fact_check` paths [crates/lute-check/src/fact_env.rs:23-31,543-596; crates/lute-check/src/fact_must.rs:162-169; crates/lute-check/src/fact_check.rs:55-59].

**Claim.** For `holds(P)`: **impossible** means no fact in the project May set matches; **guaranteed** means some fact in the path-sensitive Must set matches; otherwise **possible** means neither proof applies. For `count(P) op n`, the interval `[|Must ∩ P|, |May ∩ P|]` is used analogously [docs/proposals/scenario-dsl/0.20.0.md:37-40,123-131]. `possible` is deliberately silent/normal, not a warning [docs/proposals/scenario-dsl/0.20.0.md:156-159].

**Soundness/scope.** `May` is a flow-insensitive over-approximation, so it may include facts that cannot coexist; this preserves sound impossibility proofs. `Must` is a path-sensitive under-approximation and only monotone facts are carried across uncontrolled time. The docs call impossible and guaranteed sound proofs; the implementation documents negated atoms as satisfiable in May and only proven-true guards/guaranteed facts in the Must closure [docs/proposals/scenario-dsl/0.20.0.md:78-80,193-197; crates/lute-check/src/fact_env.rs:23-31,990-992,1044-1055]. It is not exhaustive state/model checking. An undeclared/wrong-arity/out-of-domain query is owned by its own error and remains undecided, not classified as a fact verdict [docs/proposals/scenario-dsl/0.20.0.md:151-154].

**Rendering.** The verdict is fed into the existing condition decider: impossible relational guards become the existing dead-arm/beat/objective/entry errors; guaranteed relational guards in line/choice/match/entry guards emit `W-FACT-GUARANTEED`; possible emits nothing [docs/proposals/scenario-dsl/0.20.0.md:128-144,156-159; crates/lute-check/src/fact_check.rs:55-59,993-1000]. The warning names the guaranteed fact’s provenance/reason [crates/lute-check/src/fact_env.rs:1014-1017; crates/lute-check/src/fact_check.rs:673-676]. JSON is the normal serialized `CheckResult` diagnostic array rather than a D9 evidence field [crates/lute-cli/src/main.rs:7-11].

**D9 mapping.** `impossible` = **Proven**; `guaranteed` = **Proven**; `possible` = **Unknown** in the D9 sense of undecided (despite being a named checker verdict). `W-FACT-GUARANTEED` itself = **Proven**.

## 2. `trace` three-valued unknown, exit 3

**Command/code.** `lute trace <file>` validates the document and mock, then runs the compiled source through the reference Machine/TraceDriver [crates/lute-trace/src/trace.rs:1-17].

**Claim.** Trace evaluates one concrete seeded/mock walk. A condition can be true, false, or `unknown`; unknown means the supplied inputs do not decide the guard. Unknown halts the walk unless a selection was explicitly forced past it; quest-level unresolved atoms may be recorded while the walk proceeds [crates/lute-trace/src/report.rs:297-310,358-370]. Trace is not a proof over all possible worlds.

**Soundness/scope.** It is a **Witnessed** result for the supplied mock only. The scope is one document/entry/bundle beat and one mock, with explicit state/fact/choice/event/occasion inputs. `TraceExit::Incomplete` is exit 3 for an unknown guard or unknown objective/quest atom; refusal is exit 1 and complete is exit 0 [crates/lute-cli/src/cmd_trace.rs:25-33; crates/lute-trace/src/report.rs:20-32].

**Rendering.** Human output includes unresolved/forced lines; JSON’s normative report fields are `file`, `seeds`, `steps`, `decisions`, `unresolved`, `coverage`, with additive `disposition` and `endReason` fields [crates/lute-trace/src/report.rs:454-485]. Decision records include construct/id/span/outcome/guard, and unresolved atoms name what mock input would decide them [crates/lute-trace/src/report.rs:339-370]. Entry/beat JSON uses `eligible: true|false|null`; null is the unknown case, and trace shows entry eligibility rather than enforcing it [crates/lute-trace/src/report.rs:180-183].

**D9 mapping.** Decided behavior under the mock = **Witnessed**. `unknown`/exit 3 = **Unknown**. A forced pick past unknown remains **Witnessed selection plus Unknown guard evidence**, not a proof.

## 3. `calendar` grid bounds

**Command/code.** `lute calendar <dir> --axis ... [--json|--csv]` is implemented in `crates/lute-cli/src/play/calendar.rs`; it is explicitly a tool over play, not a schedule file [crates/lute-cli/src/play/calendar.rs:1-8].

**Claim.** It evaluates every cell of the Cartesian product of declared axes, starting from one declared-default world or a save/play script replayed up to `--until`; each cell judges candidates, eligibility, selection, presentation, clock raise and gate state [crates/lute-cli/src/play/calendar.rs:6-8,31-45,1022-1035,1703-1797]. Axis values are declared state values, clock positions, quest/objective states, facts, visited values, and related finite domains [crates/lute-cli/src/play/calendar.rs:75-87].

**Soundness/scope.** This is **Bounded**, not unbounded proof. The scope is exactly the supplied axis values/product, save seed, replay prefix, occasions/targets, and optional `--where`; more than 10,000 cells is refused by the DSL contract [docs/proposals/scenario-dsl/0.23.0.md:30-40]. Unknown `when` is retained as unknown; `--where` unknown is an error rather than a guessed filter [crates/lute-cli/src/play/calendar.rs:1163-1165,1415-1418]. A beat “never eligible in any cell” means no eligibility in the enumerated grid, not globally impossible. The implementation tracks candidate-but-never-eligible and eligible-but-never-presented separately [crates/lute-cli/src/play/calendar.rs:1022-1035,1553-1555].

**Rendering.** Text cells use `not raised`, `?`, `gate false`, `-`, or presented IDs with `+N` shadowed count [crates/lute-cli/src/play/calendar.rs:1809-1829]. Text lists `shadowed`, `undecided`, `never eligible in any cell`, and `eligible but never presented in any cell` [crates/lute-cli/src/play/calendar.rs:2119-2138]. JSON has `axes`, `cells[].results[].winner`, `presented`, `shadowed`, optional `unknown:[{id,reason}]`, `undecided`, `gated`, `notRaised`, plus `neverEligible`, `neverPresented`, and `beatenBy` [crates/lute-cli/src/play/calendar.rs:2170-2265]. CSV carries winner/presented/shadowed/unknown and notes [crates/lute-cli/src/play/calendar.rs:2294-2345].

**D9 mapping.** Every cell’s concrete result = **Bounded + Witnessed** (a replayed execution in a finite cell); unknown cells = **Unknown**; aggregate “never in any cell” = **Bounded**, not Proven.

## 4. `scenario` reachability and envelopes

**Command/code.** `lute scenario reach <node>` and graph/envelope views reuse `check-project`’s connectivity/envelope passes rather than re-running a separate semantic evaluator [crates/lute-cli/src/cmd_scenario/mod.rs:1-8,71-76,128-130].

**Claim.** Connectivity computes whether a scene/quest/beat has a satisfiable route under declared `after`/`follows` structure. The algorithm is memoized structural recursion over prerequisite formulas, linear in total formula size and never route enumeration [crates/lute-check/src/connectivity.rs:1758-1767,1824-1826]. `Reachable` means a satisfiable declared route exists; `Unreachable` means no satisfiable route exists under those declared routes; `Unknown` means the analysis proves neither [crates/lute-cli/src/cmd_scenario/reach.rs:65-75].

**Soundness/scope.** `Reachable`/`Unreachable` are **Proven**, but only relative to declared routes and the restricted prerequisite profile. `Unknown` is **Unknown**. A formula atom cap of 256 is a defensive complexity bound, not a search bound [crates/lute-check/src/connectivity.rs:1784-1794]. Invalid formulas, ambiguous IDs, undeclared nodes, and cycle/downstream nodes stay Unknown/cycle-degraded rather than being guessed [crates/lute-check/src/connectivity.rs:1857-1860; crates/lute-cli/src/cmd_scenario/reach.rs:79-127]. Unanchored quests are not “reachable by graph proof”; they are a distinct “available from start; lifecycle decides” state [crates/lute-cli/src/cmd_scenario/reach.rs:105-114]. Objective-dead and lifecycle-unreachable quests render as Unreachable with `E-OBJECTIVE-UNSATISFIABLE` or `E-QUEST-UNREACHABLE` [crates/lute-cli/src/cmd_scenario/reach.rs:84-92].

**Rendering.** Human text says `Reachable — a satisfiable route exists under your declared routes`, `Unreachable — no satisfiable route exists ... (E-CONN-UNREACHABLE)`, or `Unknown ...` [crates/lute-cli/src/cmd_scenario/reach.rs:65-75]. JSON/DOT use stable tokens including `reachable`, `unreachable`, `unknown`, `unanchored`, and `cycle-degraded`; JSON is intentionally derived from the same text verdict logic [crates/lute-cli/src/scenario_fmt.rs:63-76]. Envelopes render pre-entry guaranteed/possible state writers and relation producers, but relation producer output is descriptive, not a verdict [crates/lute-cli/src/cmd_scenario/node_envelope.rs:28-31,151-155].

**D9 mapping.** Reachable/unreachable under declared route formulas = **Proven**; unknown/ambiguous/cycle-degraded = **Unknown**; envelope “written but not provably before” = **Unknown/non-proof context**.

## 5. `beats` analysis

**Command/code.** `lute beats <dir>` consumes the same project beat rows and reconciled `check-project` diagnostics; it does not re-derive semantics [crates/lute-cli/src/beats_cmd.rs:1-25,235-241].

**Claims.** It reports ladder order (priority descending then project order), `E-BEAT-UNREACHABLE`/entry equivalent, `W-BEAT-SHADOWED`, `W-BEAT-PRIORITY-TIE`, `W-BEAT-ONCE-RUN-USER`, plus informational `covered by`, per-ladder `shadowed by`, `never for <target>`, and occasion `raisedWhen` gate failures [crates/lute-cli/src/beats_cmd.rs:37-45,67-82,445-475].

- **Unreachable**: checker proves a beat `when` never holds; registry says “provably always false” [crates/lute-cli/src/codes.rs:267-269]. D9 **Proven**.
- **Shadowed**: on `select:first`, an earlier always-eligible, never-spent beat always wins first [crates/lute-cli/src/codes.rs:1532-1534]. This is a static proof under the ladder’s eligibility assumptions: **Proven** relative to that ladder. The informational `shadowed by`/`covered by` rows are selection-analysis results and should be treated as **Heuristic** when they depend on conservative ladder implication rather than full state proof [crates/lute-cli/src/beats_cmd.rs:7-11,445-475].
- **Priority tie**: same priority and simultaneous eligibility; file order decides [crates/lute-cli/src/codes.rs:1527-1529]. The tie condition is a static structural/guard-overlap analysis, not a witnessed play: **Proven** as “tie can occur” when emitted, with the possibility of false silence where overlap cannot be proved.
- **W-BEAT-ONCE-RUN-USER**: default run spending plus only user-tier reads means replay each run unless `once` is explicit [crates/lute-cli/src/codes.rs:1521-1524]. This is a static policy warning: **Heuristic/advisory**, not a claim that a particular play will replay.
- **Gate never holds / never-for**: computed with `gate_never_holds` under the root fact envelope and target members [crates/lute-cli/src/beats_cmd.rs:78-82,93-121]. **Proven** where the fact envelope proves false; otherwise absence of a mark is not proof of reachability.

**Rendering.** Human rows contain verdict words (`unreachable`, `shadowed`, `tied`, `once-run-user`), `covered by`, `shadowed by`, and `never for`; gate text appears in ladder headers [crates/lute-cli/src/beats_cmd.rs:450-475,501-518]. JSON beat rows contain `verdicts:[{code,severity,message}]`, `coveredBy`, `shadowedBy`, `neverFor`; ladder objects carry `raisedWhen`, `gateNeverHolds`, `gateNeverHoldsFor` [crates/lute-cli/src/beats_cmd.rs:607-721].

## 6. Play/test witnesses and author-declared constraints

### `play`

`lute play` executes the whole project through the reference runner and produces a concrete presentation transcript, candidates, choices, state, facts, quests and end state. It is a **Witnessed** run for the supplied script/save/engine writes, not a proof of all routes. The play expectation module explicitly judges only what the walk recorded; it does not build a second semantic model [crates/lute-cli/src/play_expect.rs:1-22].

Step expectations (`winner`, `offered`, `notOffered`, `presented`, `quests`, `state`, `facts`, `notFacts`, `clock`) judge the post-step/last-raise result; top-level expectations judge end-of-play [crates/lute-cli/src/play_expect.rs:1-12,49-75]. A miss exits 1; malformed/unknown expectation keys are usage errors before execution [crates/lute-cli/src/play_expect.rs:13-18]. `expect.presented` is therefore an author-declared witness assertion: it constrains the expected observed list, including every raise caused by an `advance` step [crates/lute-cli/src/play_expect.rs:3-10; crates/lute-cli/src/play/outcome.rs:73-101].

Human output renders the transcript and expectation misses with actual-vs-expected; JSON is the play transcript/outcome structure, with step records, presented/winner/offered fields, world snapshots/deltas, quests, notes, and end. The source confirms `PlayOutcome` is populated from runner records [crates/lute-cli/src/play/outcome.rs:15-18].

### `test`

`lute test` runs `*.test.yaml` scenario tests and any `*.play.yaml` carrying expectations. Each test traces once and checks all declared expectations; an incomplete trace fails unless `expect: {end: incomplete}` explicitly accepts that end state [crates/lute-cli/src/testcmd.rs:1-40]. This makes tests **Witnessed** assertions over one concrete mock, not universal invariants. `--coverage` reports only executed-vs-unexecuted constructs over N traced paths and explicitly avoids whole-space claims [crates/lute-cli/src/testcmd.rs:40-46].

Author constraints available in test/play artifacts:

- `expect:` is a declared assertion over transcript, options, presented IDs, end state, quests, state, facts, and absence facts [crates/lute-cli/src/testcmd.rs:15-26; crates/lute-cli/src/play_expect.rs:1-12].
- `expect.presented` constrains exact presentation lists for a raise/advance; it is a witness requirement, not a static proof [crates/lute-cli/src/play_expect.rs:3-10].
- `assume` on cast presence is an author-declared checker assumption: `cast.present` conditions normally require the enclosing guards to imply presence, while `assume: true` treats a negated `holds` of an engine-reserved relation in `present` as true [crates/lute-check/src/cast.rs:407-437]. This makes `assume` a constraint/assumption affecting `W-CAST-ABSENT`, not a runtime witness.
- Cast `present:` itself is a declared presence invariant. `W-CAST-ABSENT` warns when line guards do not prove it; the proof is per conjunct using `guards && !present` [crates/lute-check/src/cast.rs:193-196,408-411]. D9 mapping: emitted warning’s underlying non-absence proof is **Proven** if emitted; no warning is not a witnessed presence guarantee.
- `optional` objective is an author-declared lifecycle constraint: envelope completion/write analysis skips optional objectives because they need not fire, while required objectives participate [crates/lute-check/src/envelope.rs:177-185,203-207]. This changes the constraint scope; it is not evidence itself.
- `presented:` save seeds in play scripts constrain the starting witnessed world (the save’s visited/presentation memory), not a proof that those presentations are reachable in the current script [crates/lute-cli/src/play/mod.rs:25-29; README.md:191-198].
- `assume`/`present` and `optional` are source/harness constraints; they should not be conflated with D9 Proven/Witnessed unless a run/test actually exercises them.

## 7. W-OBJECTIVE-STRANDED

**Code/claim.** `check-project` emits this warning when a required objective’s only known completion beats have finite clock windows that can all close, while the objective has neither `until=` nor `by=` deadline/failure handling [crates/lute-check/src/clock_positions.rs:1164-1168; crates/lute-cli/src/codes.rs:1666-1669].

**Soundness/scope.** The 0.31 spec calls it a static calendar check and says it does not perform path search; an unclocked `when` leaves an open path conservatively [docs/proposals/scenario-dsl/0.31.0.md:47-50; crates/lute-check/src/clock_positions.rs:1165-1168]. Thus it is **Bounded** to declared clock windows plus static candidate/completion structure, with conservative static reasoning; not proof that every actual route strands the objective. Best D9 classification: **Bounded** (with conservative/heuristic incompleteness), not Proven universal.

**Rendering.** Standard diagnostic text is warning `W-OBJECTIVE-STRANDED`; `check`/`check-project` human output prints path:line:column, severity, code, message and JSON carries the diagnostic object [crates/lute-cli/src/codes.rs:1666-1669; crates/lute-cli/src/cmd_check_project.rs:339-341; crates/lute-cli/src/main.rs:7-11].

## 8. W-SLOT-CONTENTION

**Code/claim.** It warns that two required objectives in one run can only be completed by advancing beats at the same single clock position, so one presentation consumes the other objective’s only slot [crates/lute-check/src/clock_positions.rs:1170-1172; crates/lute-cli/src/codes.rs:1736-1739].

**Soundness/scope.** This is explicitly a static calendar check, not path search [docs/proposals/scenario-dsl/0.31.0.md:54-55]. It is **Bounded** to the declared clock/advance slot model and objective candidate sets; it does not prove all dynamic worlds. The word “can only” is a static structural conclusion, while actual contention depends on selection/order and state.

**Rendering.** Warning diagnostic `W-SLOT-CONTENTION` in normal check/check-project text or JSON diagnostic arrays [crates/lute-cli/src/codes.rs:1736-1739; crates/lute-cli/src/cmd_check_project.rs:339-341].

## 9. E-ADVANCE-CASCADE

**Code/claim.** It errors on an unguarded repeatable beat that answers the same clock raise it advances into, creating a recursive nested `advances` cascade [crates/lute-check/src/clock_positions.rs:1173-1177; crates/lute-cli/src/codes.rs:191-194].

**Soundness/scope.** The authoring diagnosis is static; runtime safety still bounds nested declared advances at 64 presentations and reports the looping beat rather than hanging [docs/proposals/scenario-dsl/0.31.0.md:56-62]. Static error = **Proven** as a structural self-cascade pattern. Runtime bound behavior = **Bounded** (64 nested presentations), not proof of absence of all loops.

**Rendering.** `E-ADVANCE-CASCADE` is a normal error diagnostic with source span/message; runtime play rendering reports the bounded cascade/looping beat through the play halt/error path [crates/lute-cli/src/codes.rs:191-194; docs/proposals/scenario-dsl/0.31.0.md:60-62].

## 10. Tie/exclusivity

**Tie.** `W-BEAT-PRIORITY-TIE` says beats on one `select:first` occasion share priority and can be eligible simultaneously; file order then breaks the tie [crates/lute-cli/src/codes.rs:1527-1529]. This is a static possible-overlap warning: **Bounded/Heuristic** (it establishes tie potential under analyzed guard overlap, not that a supplied play will tie). `beats` renders `tied` and JSON includes the underlying code/severity/message [crates/lute-cli/src/beats_cmd.rs:37-45,678-695].

**Exclusivity.** `E-FACT-EXCLUSIVE` is a runtime/check contract around mutually excluding relation facts: trace records an `Exclusive` step immediately after a write that makes both excluded facts hold and refuses with exit 1 [crates/lute-trace/src/report.rs:239-246]. Static fact write/exclusion checks are checker errors where the checker can prove the conflicting write; the runtime report is a concrete **Witnessed** violation. The trace JSON step kind is `exclusive`; human transcript explains the conflicting facts [crates/lute-trace/src/report.rs:239-246].

## 11. W-FACT-GUARANTEED (separate emphasis)

This warning is a **Proven** redundancy proof, not a witness: the queried relation is in Must at every route to the guard. It applies only to guard locations (line `when`, choice `when`, match test, entry/beat `when`), not quest `start`/`fail` or objective `done` predicates [docs/proposals/scenario-dsl/0.20.0.md:140-145; crates/lute-check/src/fact_check.rs:55-59]. It renders as diagnostic warning text/JSON with provenance reason [crates/lute-check/src/fact_env.rs:1014-1017].

## 12. E-BEAT-UNREACHABLE

**Code/claim.** A scene/bundle beat `when` guard is provably always false, so the beat can never be selected [crates/lute-check/src/beats.rs:43-47; crates/lute-cli/src/codes.rs:267-269]. This comes from scalar/fact proof, including impossible relational queries, not path search. D9 = **Proven** relative to the checker’s sound analysis scope. `W-WIP` may downgrade dead-guard errors to warnings when the only cause is an as-yet unproduced relation, but it still names the underlying error [crates/lute-check/src/fact_check.rs:712-715; crates/lute-cli/src/codes.rs:1817-1819].

**Rendering.** `check-project` diagnostic; `lute beats` attaches it as `unreachable` in text and JSON verdicts [crates/lute-cli/src/beats_cmd.rs:37-45,445-475,678-695].

## 13. Connectivity and related “no path” wording

`E-CONN-UNREACHABLE` is not a bounded search failure. It is emitted only when memoized structural reachability proves no satisfiable route under declared prerequisite formulas; the implementation expressly says only `Unreachable` earns the diagnostic and `Unknown` never collapses to it [crates/lute-check/src/connectivity.rs:1758-1767]. The human text nevertheless says “no satisfiable route exists under your declared routes” [crates/lute-cli/src/cmd_scenario/reach.rs:70-71], which is acceptable because this is a proof over the declared route model, not “no path found within bounds.”

Other connectivity errors are structural proofs/errors: `E-CONN-CYCLE` means no evaluation order satisfies all `after` clauses; `E-CONN-PROFILE` rejects constructs outside the restricted prerequisite grammar; `E-CONN-FORMULA-TOO-COMPLEX` is a defensive formula-size refusal; `E-CONN-UNKNOWN-NODE` identifies undeclared references [crates/lute-cli/src/codes.rs:411-439; crates/lute-check/src/connectivity.rs:1784-1794]. Their evidence is **Proven** for the structural defect, with formula cap/invalid/cycle fallout rendered Unknown where reachability itself cannot be trusted.

## 14. Differential

**Command/code.** The differential test compares trace’s source/AST walk with run’s compiled-IR reference Machine, over transcript lines, final state, facts, quest states and exit [crates/lute-cli/src/differential.rs:1-6,47-64].

**Soundness/scope.** It is a conformance comparison, not a semantic proof of either runtime: **Witnessed** agreement/disagreement over concrete inputs. The harness skips check-refused cases, incomplete/unknown traces, refused mocks, unoffered scripted picks, unanswered bridges, and forced unknown picks; only concrete cases are compared [crates/lute-cli/src/differential.rs:675-720]. Play presentations are replayed from the world play actually presented [crates/lute-cli/src/differential.rs:15-19]. A compared divergence is a **Witnessed** incompatibility. The corpus floor requires at least 90% of the recorded 201 in-repo comparison floor unless filtering is active [crates/lute-cli/src/differential.rs:43-45,1521-1535].

**Rendering.** Verbose mode prints both observations; the summary prints compared/skipped/refused counts and skip reasons; failures report divergence fields such as transcript/state/facts/quests/exit/IR [crates/lute-cli/src/differential.rs:1422-1436,1521-1528]. There is no D9 evidence label in the output.

## 15. Explicit “no path found within bounds” audit

- **Calendar:** does **not** claim global absence. “never eligible in any cell” and “eligible but never presented in any cell” are explicitly grid-scoped; JSON uses `neverEligible`/`neverPresented`, not “no path exists” [crates/lute-cli/src/play/calendar.rs:1553-1555,2252-2256].
- **Trace:** exit 3 says incomplete/unknown for the supplied walk; it does not say no path exists [crates/lute-trace/src/report.rs:20-32,454-485].
- **Differential:** skipped “not concrete” is not reported as no path; the summary says skipped as not concrete [crates/lute-cli/src/differential.rs:1521-1528].
- **Scenario connectivity:** `Unknown` is preserved; `Unreachable` is only a proof under declared routes, not a bounded-search miss [crates/lute-check/src/connectivity.rs:1758-1767; crates/lute-cli/src/cmd_scenario/reach.rs:70-75].
- **Checker E-BEAT-UNREACHABLE / objective dead diagnostics:** these use absolute “can never/provably” wording, but are based on sound static proof, not bounded search [crates/lute-cli/src/codes.rs:267-269; docs/proposals/scenario-dsl/0.20.0.md:193-197].
- **Potentially risky wording:** `W-OBJECTIVE-STRANDED` and `W-SLOT-CONTENTION` are static bounded calendar checks, yet their diagnostic summaries say “can only”/“only known”; the spec explicitly says no path search, so they should be read as bounded static conclusions, not universal execution proofs [docs/proposals/scenario-dsl/0.31.0.md:47-55; crates/lute-cli/src/codes.rs:1666-1669,1736-1739].

## Gaps/risks observed

1. D9’s Proven/Witnessed/Bounded/Heuristic/Unknown labels are not consistently serialized: diagnostics retain legacy code/severity/message, trace uses `null`/`unresolved`/exit 3, calendar uses cell flags, scenario uses tokens, and differential uses compared/skipped counts [crates/lute-trace/src/report.rs:454-485; crates/lute-cli/src/play/calendar.rs:2218-2237; crates/lute-cli/src/scenario_fmt.rs:63-76].
2. Calendar’s aggregate `neverEligible`/`neverPresented` is correctly grid-scoped internally, but the field names alone omit the bound unless consumers also inspect `axes`/`from` [crates/lute-cli/src/play/calendar.rs:2192-2200,2248-2256].
3. `W-OBJECTIVE-STRANDED` and `W-SLOT-CONTENTION` are explicitly non-path-search static checks, but their normal warning prose can be read more strongly than the stated bounded scope [docs/proposals/scenario-dsl/0.31.0.md:47-55; crates/lute-cli/src/codes.rs:1666-1669,1736-1739].
4. Play/test expectations are strong author constraints over concrete witnesses, but there is no indication in the expectation output that a passing assertion is Witnessed rather than Proven [crates/lute-cli/src/play_expect.rs:1-22].
5. `assume: true` changes presence-analysis assumptions around engine-reserved relations; the source distinguishes this from ordinary presence proof, but output does not expose an evidence-level distinction [crates/lute-check/src/cast.rs:407-437].
6. Differential intentionally excludes unknown/incomplete/unanswered-bridge cases, so agreement coverage is only over concrete cases; the summary reports skips, but the result is not a universal runtime-equivalence claim [crates/lute-cli/src/differential.rs:675-720,1521-1528].