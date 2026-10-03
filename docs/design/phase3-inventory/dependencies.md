# Phase 3 inventory — dependencies

## summary

Lute already computes several reusable dependency surfaces, but they are split across checker analyses and mostly expose diagnostics/CLI reports rather than a unified node-to-node semantic snapshot. The strongest reusable pieces for impact analysis are: CEL state-path extraction, definite-assignment write/read flow, fact producer/consumer edges, fact must/may environments with provenance, connectivity/prerequisite graphs, project beat records, quest/lore source tables, and compile SourceMap identity/spans. The drowned-crown corpus demonstrates a concrete Regent-death chain through `slew`/`felled` facts, but current analyses do not connect all of that chain to individual lines, nested reward declarations, read disclosures, engine-owned death/survival paths, or runtime event semantics.

## files

```json
[
  {
    "path": "docs/design/architecture-direction.md:100-106",
    "description": "D3 defines the internal project semantic model as holding resolved references, types, reads/writes, domain structure, derived graphs, and diagnostic evidence."
  },
  {
    "path": "docs/design/architecture-direction.md:168-176",
    "description": "D7 requires every construct to be mapped to owning module, reads/writes, evaluation point, and lowering."
  },
  {
    "path": "docs/design/architecture-direction.md:206-213",
    "description": "D9 establishes impossible/guaranteed/possible, trace unknown, calendar grid, Witnessed, and Bounded evidence vocabulary."
  },
  {
    "path": "docs/design/architecture-direction.md:347-358",
    "description": "Phase 3 explicitly calls for one shared snapshot, impact query, constraints, evidence levels, and the NPC-survival exit criterion."
  },
  {
    "path": "docs/design/modules.md:1-42",
    "description": "Normative module map identifies ownership and cross-module dependencies for core, narrative, quest, time, occasions, knowledge, lore, staging, and timeline."
  },
  {
    "path": "crates/lute-check/src/cel_paths.rs:1-17,409-490",
    "description": "Shared AST walk extracts maximal state paths and classifies Read/Guard/WeakGuard uses; successful CEL AST nodes lack precise sub-node offsets and callers fall back to enclosing slot spans."
  },
  {
    "path": "crates/lute-check/src/defassign.rs:180-240,1000-1040",
    "description": "Definite-assignment returns final must-written paths and undefaulted entry-state reads; reads are `(path, Span)` only when they can produce E-MAYBE-UNSET."
  },
  {
    "path": "crates/lute-check/src/fact_must.rs:1-80,179-240,759-840",
    "description": "Fact-must analysis records facts guaranteed at every guard slot, scene-entry guaranteed facts, and provenance for asserted/seed/entry-read facts."
  },
  {
    "path": "crates/lute-check/src/fact_env.rs:1615-1658",
    "description": "FactEnv exposes project-root MaySet, MustMap, optional WIP MaySet, and Impossible/Guaranteed/Excluded/Possible fact verdicts."
  },
  {
    "path": "crates/lute-check/src/cast.rs:739-853",
    "description": "FactProducers indexes assert/effect producer sites by relation, document, unit key, and constant/open arguments; fact_writes additionally retains spans and authored write text."
  },
  {
    "path": "crates/lute-check/src/fact_edges.rs:1-120",
    "description": "FactEdge connects producer unit to reader node, records asserted fact and derived gate fact (`via`), follows positive ground/wildcard holds in beat/entry/quest start/occasion gates to bounded rule depth."
  },
  {
    "path": "crates/lute-check/src/connectivity.rs:680-930,1164-1234",
    "description": "ConnGraph carries typed nodes, prerequisite edges, EdgeKind tags, NodeInfo path/prereq/span, and deterministic topological order."
  },
  {
    "path": "crates/lute-check/src/beats.rs:1455-1515,1650-1715",
    "description": "ProjectBeat carries project/document path, beat identity, occasion/target/kind targets, priority/cadence/after/when/expanded-when, declaration anchor and extent, unit, and folded environment."
  },
  {
    "path": "crates/lute-compile/src/source_map.rs:1-220",
    "description": "Per-document SourceMap maps final addr to source spans/write text/guards/component origin and has quest/objective/entry/beat side tables."
  },
  {
    "path": "crates/lute-compile/src/lib.rs:755-820",
    "description": "Compiler fills source-side quest/objective/handler/entry/bundle-beat tables keyed by stable construct ids."
  },
  {
    "path": "crates/lute-compile/src/address.rs:1-3,253-355",
    "description": "Address pass distinguishes regenerated positional addr from stable lineId/voiceKey identities and assigns line/choice/hub identities."
  },
  {
    "path": "crates/lute-cli/src/cmd_scenario/graph.rs:266-328",
    "description": "`lute scenario --facts` reuses fact_edges and layers may-fact edges over prerequisite graph without re-deriving analyses."
  },
  {
    "path": "crates/lute-cli/src/cmd_scenario/mod.rs:77-125,131-240",
    "description": "RootScenario retains graph, reachability, envelopes, per-scene reads, per-document effects, relational vocabulary, and scene-entry must facts for scenario reporting."
  },
  {
    "path": "crates/lute-cli/src/beats_cmd.rs:1-23",
    "description": "`lute beats` exposes project beat rows and check-project verdicts, including ladder ordering, gates, never-for, shadowing and coverage."
  },
  {
    "path": "crates/lute-cli/src/cmd_scenario/reach.rs:1-18,49-97,180-243",
    "description": "`lute scenario reach` exposes prerequisite structure, referenced nodes and Reachable/Unreachable/Unknown claims."
  },
  {
    "path": "crates/lute-cli/src/cmd_context.rs:17-29,108-116,454-480",
    "description": "`lute context` exposes resolved authoring surface/schema/vocabulary/defs/ids but not per-node dependency edges."
  },
  {
    "path": "docs/examples/games/drowned-crown/scenes/boss/regent.lute:2-20",
    "description": "Regent is a scene/occasion target `foe.regent`; its authored body presents the Regent's fall but does not itself assert a fact."
  },
  {
    "path": "docs/examples/games/drowned-crown/scenes/hub/regent-fell.lute:2-9",
    "description": "Hub beat is gated by `holds('slew',['regent'])`."
  },
  {
    "path": "docs/examples/games/drowned-crown/lore/brann.lute:26-32",
    "description": "Brann disclosure requires `visited('hub.regentFell')` and `holds('felled',['regent'])`, then asserts `told(brann,regent)`."
  },
  {
    "path": "docs/examples/games/drowned-crown/lore/palace.lute:12-21",
    "description": "Codex throne disclosure requires `holds('felled',['regent'])`."
  },
  {
    "path": "docs/examples/games/drowned-crown/lore/quill.lute:26-31",
    "description": "Quill heir disclosure requires `holds('felled',['regent'])`; later witness disclosure consumes `heard(quill,regent)`."
  },
  {
    "path": "docs/examples/games/drowned-crown/quests/dive.lute:9-17,24-28",
    "description": "Regent quest objective requires `holds('slew',['regent'])`, has a run-outcome deadline, and emits a PEARLS reward and conditional quest-complete disclosure; eel quest shows objective-to-reward pattern."
  },
  {
    "path": "docs/examples/games/drowned-crown/quests/library.lute:10-18",
    "description": "Library quest start requires `holds('felled',['regent'])`, with a PEARLS reward and Archivist objective."
  },
  {
    "path": "docs/examples/games/drowned-crown/lore/guardians.lute:12-40",
    "description": "Boss-defeat lore entries consume occasion/target and include first-kill, repeat, and fallback disclosure ladders."
  },
  {
    "path": "docs/examples/games/drowned-crown/README.md:11-23,53-70",
    "description": "Corpus describes engine-reported guardian deaths/kills, quest rewards, components, and existing CLI dogfood commands."
  }
]
```

## architecture

Current dependency information is distributed by phase and scope: per-slot CEL/path and defassign data in `lute-check`; project-root graph, fact envelopes and producer/consumer edges in `lute-check`; compile-time identity/provenance in `lute-compile::SourceMap`; and selected projections through `check-project`, `scenario`, `beats`, `context`, `trace`, and compiled IR/source maps. A future impact query could reuse these structures directly, but today there is no single public read-only snapshot joining state paths, fact edges, prerequisite/occasion eligibility, quest/reward ownership, lore disclosure, component/def expansion provenance, and line identities.

## report

## 1. Existing dependency/read-write information

### A. State-path reads and writes

**CEL path reads.** `crates/lute-check/src/cel_paths.rs:1-17` documents the shared resolver/defassign walk. `collect_path_uses` returns maximal dotted paths such as `scene.player.hp`, not intermediate prefixes, and labels each use `Read`, `Guard`, or `WeakGuard` (`crates/lute-check/src/cel_paths.rs:121-143,409-490`). It recurses through calls, list/map/struct elements and comprehensions (`crates/lute-check/src/cel_paths.rs:409-490`). Granularity is one CEL expression/slot; the returned `PathUse` has the path, role, and local short-circuit proofs, but no precise sub-expression span. Because cel-parser drops successful-AST source positions, the checker generally uses the enclosing CEL slot span (`crates/lute-check/src/cel_paths.rs:15-17`; `crates/lute-check/src/cel_resolve.rs:8-18`). This is per-document during checking; the same walk is reused by project-level analyses. It is not itself a CLI report, but its consequences appear in `check`/`check-project` diagnostics and in `scenario`'s per-scene reads (`crates/lute-cli/src/cmd_scenario/mod.rs:199-229`).

**Definite assignment.** `check_definite_assignment` returns diagnostics, a final must-write `Assigned` set, and `(path, Span)` reads that fell back to entry state (`crates/lute-check/src/defassign.rs:213-240`). The `available` lattice includes writes plus guard proofs, while `writes` contains only `::set`/choice-record writes and is reused by the envelope guaranteed-write calculation (`crates/lute-check/src/defassign.rs:180-212`). `check_read` records only declared, undefaulted, non-choice-log reads that could earn E-MAYBE-UNSET (`crates/lute-check/src/defassign.rs:1000-1040`). Granularity is document/node-flow: the final write set is per walked body, while each recorded read is path plus the enclosing slot/node span. It is per-document first; project reconciliation reclassifies reads against project-wide envelopes. `RootScenario` stores `reads_per_scene` and per-document write effects (`crates/lute-cli/src/cmd_scenario/mod.rs:77-125`). CLI exposure is indirect through `check`, `check-project`, and `lute scenario envelope`; there is no standalone read/write dump.

**Writes outside defassign.** The checker has explicit write collectors for state/directive effects and project envelope calculations. `component_effects::splice_component_effects` inserts effectful component `::set`/fact/directive writes into each host at each use (`crates/lute-check/src/component_effects.rs:8-12,405-415`). The project scenario retains `envelope::PerDocEffects`, including write sets and quest-completion writes, specifically because those writers are otherwise dropped (`crates/lute-cli/src/cmd_scenario/mod.rs:102-125,205-219`). This is project-root data at scene/document granularity, with source spans available at the original AST/write sites, but no unified public API or direct CLI writer listing.

### B. Fact producers/consumers and derived dependencies

**Producer sites.** `FactProducers` indexes every `::assert` and declared directive `effects.asserts` by relation, path, unit and constant/open arguments (`crates/lute-check/src/cast.rs:774-853`). Components are intentionally excluded as standalone producers because their writes are bound and spliced at host `::use` sites (`crates/lute-check/src/cast.rs:793-797`). The unit key is `0` for scene shots and the construct's `span.byte_start` for quests, entries and bundle beats (`crates/lute-check/src/cast.rs:804-833`). This is project-root granularity, but the public `FactProducers::sites` data has no span or authored text; it has path and byte-start unit. The internal `fact_writes` collector is richer: it includes relation, constant/open args, assert/retract direction, authored write text, path and `Span` (`crates/lute-check/src/cast.rs:835-853`). It is not exposed as a CLI report.

**Fact edges.** `FactEdge` is the closest existing impact-like relation: `from` producer node → `to` reader node, with asserted fact and optional `via` gate fact (`crates/lute-check/src/fact_edges.rs:39-55`). It scans positive top-level ground/wildcard `holds` requirements in scene beat `when`, bundle beat/lore-entry `when`, quest `start`, and occasion `raisedWhen`; it follows positive rule dependencies to a bounded depth of four (`crates/lute-check/src/fact_edges.rs:1-32,59-107`). Producer nodes include scene, quest, lore entry and bundle beat units; directive effect asserts and component uses are included through the host (`crates/lute-check/src/fact_edges.rs:1-32`). Granularity is construct/node, not line. `FactEdge` carries no source `Span`; its nodes resolve through `ConnGraph::NodeInfo`, whose declaration path and anchor span are available (`crates/lute-check/src/connectivity.rs:901-924`). It is project-root/per-resolved-root, not cross-project. CLI exposure is `lute scenario --facts`; the CLI explicitly reuses `fact_edges` and adds may edges to the prerequisite graph (`crates/lute-cli/src/cmd_scenario/graph.rs:266-324`), and JSON calls them `factEdges` (`crates/lute-cli/src/scenario_fmt.rs:226-305`).

**Fact may/must flow.** `FactMust` stores guaranteed facts at every guard slot and guaranteed facts at scene entry, with `MustFact.provenance` (`crates/lute-check/src/fact_must.rs:179-240`). The transfer records the fact set before each guard, then assumes positive `holds` and `entry.*.read/everRead`; entry-read facts preserve provenance from the entry body (`crates/lute-check/src/fact_must.rs:759-840`). `MustMap::at` looks up facts by document path and byte span (`crates/lute-check/src/fact_env.rs:739-760`). `FactEnv` combines `MaySet`, `MustMap` and optional WIP MaySet; `HoldsVerdict` yields Impossible, Guaranteed with a provenance fact, Excluded with a conflicting provenance fact, or Possible (`crates/lute-check/src/fact_env.rs:1615-1658`). Granularity is project-root environment with slot-level lookup and source span keys. CLI exposure is through check diagnostics, `scenario envelope`, scenario fact sections, and trace mock/producibility warnings; there is no general fact-environment export.

**Relation-level producers and derived closure.** `producible()` determines whether a relation can ever exist from facts seeds, reserved engine ownership, live assert sites, or rule bodies; it deliberately does not run runtime Datalog (`crates/lute-check/src/producible.rs:1-10,33-55`). It is relation-level, project-root, generally without a source span; `scenario envelope` prints relation producibility and writer summaries (`crates/lute-cli/src/cmd_scenario/node_envelope.rs:151-184`). This can explain “why a guarded objective is impossible,” but not enumerate every concrete affected line.

### C. Quest objective → reward/grant links

The compile IR keeps rewards directly on their owner: `QuestCmd.rewards` and `ObjectiveEntry.rewards`; objective records also retain `done`, `visibleWhen`, body, optional/subquest linkage and source identity (`crates/lute-compile/src/ir.rs:1404-1418,1506-1530`). The normalized runtime walk lowers objectives and rewards together (`crates/lute-trace/src/exec/machine/quest.rs:580-720`, as cited by `docs/design/modules.md:quest table`). SourceMap side tables retain quest span, objective span, expanded `done`/`by`/`until`, and handler spans (`crates/lute-compile/src/source_map.rs:175-199`; `crates/lute-compile/src/lib.rs:755-803`). Granularity is quest/objective/reward declaration; rewards are not separate graph nodes, and the shown source-side objective table does not add reward-specific spans/indexes. The project-wide connectivity pass does synthesize quest edges for subquest, start and accept anchors (`crates/lute-check/src/connectivity.rs:901-924,972-1212`), but it does not make an objective-to-reward dependency edge. CLI exposure: `scenario reach` exposes quest prerequisite structure and lifecycle reach verdicts (`crates/lute-cli/src/cmd_scenario/reach.rs:49-97,180-243`); `scenario envelope` can show quest writes/producibility; compile JSON exposes nested reward/objective data, but no CLI query says “this fact affects this reward.”

### D. Beat/entry eligibility dependencies

`ProjectBeat` is the existing normalized eligibility record. It carries path, scene/entry/bundle-beat identity, occasion, target and resolved kind/for targets, priority, cadence, `after`, `when_slot`, expanded `when`, `spentBy`, title, anchor span, source extent, unit and folded environment (`crates/lute-check/src/beats.rs:1455-1515`). `project_beats` constructs those records and expands defs for `when` (`crates/lute-check/src/beats.rs:1650-1715`). Granularity is beat/entry/document, with declaration anchor and extent spans; it does not expose individual line dependencies.

Connectivity handles `after`, scene/quest prerequisite formulas, visited/completed/active atoms, and synthesized accept/start/subquest anchors. `ConnGraph` has typed node ids, node declaration path/prerequisite/span, edge sets and edge-kind tags (`crates/lute-check/src/connectivity.rs:680-930`). `EdgeKind` distinguishes visited, completed, active, accept, start and subquest (`crates/lute-check/src/connectivity.rs:827-900`). It is project-root and source-anchored at the node declaration, not at every atom occurrence. `lute scenario reach` exposes the raw parenthesized prerequisite formula and referenced-node verdicts (`crates/lute-cli/src/cmd_scenario/reach.rs:1-18,180-243`).

Occasion gate dependencies are included in `FactEdge`: `raisedWhen` is treated as a reader, with member-target grounding, and fact producers can be scenes/quests/lore entries/bundle beats (`crates/lute-check/src/fact_edges.rs:1-32`). `lute beats` exposes the selection ladder, priority, target cells, gate impossibility, never-for, coverage and shadowing verdicts (`crates/lute-cli/src/beats_cmd.rs:1-23`). It does not emit a generic “all state/fact paths read by this beat” record.

### E. Lore disclosure: entry reads → facts

The fact-must analysis explicitly models disclosure flow. A guard `entry.X.read` assumes crossing facts guaranteed by entry X's first-read body; `entry.X.everRead` only transfers user/app-tier facts because run-tier facts reset (`crates/lute-check/src/fact_must.rs:1-18`). `entry_outcomes` and `FactMust::assumptions` attach the originating entry body provenance to those facts (`crates/lute-check/src/fact_must.rs:759-840`). Lore entries lower to `EntryCmd` records with `when`, body, read-state behavior; SourceMap indexes each entry by id and span (`crates/lute-compile/src/ir.rs:1599-1608`; `crates/lute-compile/src/source_map.rs:19-31`). Granularity is entry/body and guard slot, not individual disclosure lines. The source map has entry declaration spans but no explicit “entry read → every affected line/quest” edge. CLI exposure is `lute trace --entry/--beat`, `lute test`, `lute beats`, `lute scenario` and lore reports, but none exports the disclosure dependency graph (`crates/lute-cli/src/cmd_trace.rs:73-108`; `crates/lute-cli/src/beats_cmd.rs:1-23`).

### F. `@def` expansion dependencies

The checker/compiler expands defs hygienically and recursively. `expand_cel` scans top-level refs, recursively expands call arguments, detects cycles, threads match `$` subjects, and substitutes parameters hygienically (`crates/lute-check/src/cel_expand.rs:1-90,94-183`). Def expansion is used when building normalized beat `when` values (`crates/lute-check/src/beats.rs:1650-1675`) and in fact edge gate extraction (`crates/lute-check/src/fact_edges.rs:75-107`). Defassign tracks a `via` chain for state reads routed through defs, using `def_chain_where`/`def_chain_label` (`crates/lute-check/src/defassign.rs:1200-1240`). SourceMap stores expanded guard text and authored guard text for match/choice arms (`crates/lute-compile/src/source_map.rs:106-130`), and SourceMap records `authored_id`/component origin. Granularity is expression/use/guard; dependency names are available internally, and some diagnostics carry the authored ref span, but there is no exported def-to-use graph or source-span edge list. `lute context` exposes def declarations/bodies as authoring surface, not reverse consumers (`crates/lute-cli/src/cmd_context.rs:17-29,108-116`).

### G. Components/templates

Component bodies are checked in a separate restricted environment. `component_def_reads` reports component-body references to ambient defs/state and recommends passing them as typed params, with precise component-local token spans rebased into diagnostics (`crates/lute-check/src/check/component_body.rs:1219-1293`). Effectful component bodies are spliced into host documents at each `::use`, and component writes/guards are judged against the host schema at the use site (`crates/lute-check/src/component_effects.rs:8-12,405-415`; `crates/lute-check/src/check/component_body.rs:619-683`). Compile normalization records component scope; SourceMap records component use name/scope and component-file spans (`crates/lute-compile/src/source_map.rs:39-40,145-168`). Granularity is component definition/use/expanded node. It carries source spans, including component-local spans for expanded records, but no reusable semantic edge graph from template parameter to each expanded line. CLI exposure is diagnostics, `context` imported component surface, compile IR and trace source reporting; no component dependency query.

### H. Line ids and source identity

`addr` is regenerated positional addressing; stable content joins are `lineId` and `voiceKey`, derived from speaker/code and identity templates (`crates/lute-compile/src/address.rs:1-3,253-355`). SourceMap is keyed by final `addr` but each `SourceInfo` carries the authored span, write text, guard/arm source, source-only markers and component origin (`crates/lute-compile/src/source_map.rs:1-20,80-168`). SourceMap also has construct-id side tables for quests/objectives/entries/bundle beats (`crates/lute-compile/src/lib.rs:755-803`). Line IDs are serialized in execution IR and exposed by compile/loc, while SourceMap is an in-memory `compile_mapped` result and is not serialized (`crates/lute-compile/src/lib.rs:417-427`; `crates/lute-compile/src/source_map.rs:1-7`). Therefore the line identity/provenance substrate exists, but checker dependency records generally key on source slot spans or construct byte starts, not lineId; joining a dependency to a line requires a compile/source-map pass.

## 2. Concrete drowned-crown chain

The corpus has no literal NPC `survival` state or `run.killedBy` field. The closest exact dogfood is a killable guardian (the Tide Regent) and its death fact/disclosures. The README says the engine reports which guardian fell or killed Ilo (`docs/examples/games/drowned-crown/README.md:11-15`), while the Regent scene is the `bossDefeated` target `foe.regent` and narrates its body unwinding (`docs/examples/games/drowned-crown/scenes/boss/regent.lute:2-20`).

A change to the Regent-death condition/fact currently has this visible chain:

1. **Event/producer boundary.** The engine raises `bossDefeated` with a typed foe target; the scene is eligible on `bossDefeated`, target `foe.regent`, once user (`docs/examples/games/drowned-crown/scenes/boss/regent.lute:6-10`). The content scene itself contains no `::assert{slew(...)}` or `::assert{felled(...)}`; the engine/plugin event surface and/or another host path must provide the fact. The plugin README/schema establishes that engine-owned run state and event surface are external (`docs/examples/games/drowned-crown/README.md:11-15,27-29`; `docs/examples/games/drowned-crown/plugins/game.occasions/occasions/stair.yaml:8-10` is the analogous explicit event contract in Ashen Stair).
2. **Immediate beat gate.** `hub.regentFell` is selected on `hubVisit` only when `holds('slew',['regent'])` (`docs/examples/games/drowned-crown/scenes/hub/regent-fell.lute:2-9`). This is a fact consumer. `fact_edges` can see a positive ground `holds` gate, but only if it sees an authored producer; an engine-owned/reserved fact has no content producer and is treated as reserved/producible rather than linked to a concrete engine event (`crates/lute-check/src/fact_edges.rs:1-32`; `crates/lute-check/src/producible.rs:33-55`).
3. **Quest objective/reward.** `regentHunt.slay` requires `holds('slew',['regent'])`, with `by="run.outcome != 'diving'"`; the quest owns a 20-PEARLS reward (`docs/examples/games/drowned-crown/quests/dive.lute:9-17`). The dependency checker can identify the objective guard and relation-level producibility, and SourceMap can identify quest/objective spans, but no existing edge connects the specific `slew(regent)` fact to this objective's reward declaration. Rewards remain nested owner data in the IR (`crates/lute-compile/src/ir.rs:1404-1418,1506-1530`).
4. **Quest start and downstream quest.** `libraryKey` starts when `holds('felled',['regent'])`, owns a 25-PEARLS reward, and has the Archivist objective (`docs/examples/games/drowned-crown/quests/library.lute:10-18`). This creates a fact-gated quest start that `fact_edges` can represent as a producer/reader edge if a content producer exists; if `felled(regent)` is engine/reserved, current `FactEdge` has no source event link.
5. **Brann disclosure.** `brann.regentTold` requires both `visited('hub.regentFell')` and `holds('felled',['regent'])`; its body presents four lines, asserts `told(brann,regent)`, and writes bond state (`docs/examples/games/drowned-crown/lore/brann.lute:26-32`). Connectivity can represent the `visited` prerequisite as a node edge, while fact analysis can recognize the `felled` guard. The four individual lines have stable line IDs after compile and source spans in SourceMap, but no current impact result fans the guard dependency down to all four line records.
6. **Quill disclosure.** `quillHeir` reads `holds('felled',['regent'])` and presents a disclosure line; `quillWitness` later reads `heard(quill,regent)` (`docs/examples/games/drowned-crown/lore/quill.lute:26-31`). The first relation is a direct fact consumer; the second requires a producer not shown in the cited body and is not automatically inferred from seeing the Regent fall. Current fact edges follow authored assert/effect producers and bounded positive rule dependencies, not semantic/event narrative implication (`crates/lute-check/src/fact_edges.rs:1-32,59-107`).
7. **Codex disclosure.** `codexThrone` is an entry whose `when` is `holds('felled',['regent'])` and presents the throne disclosure (`docs/examples/games/drowned-crown/lore/palace.lute:16-21`). Lore read-state can then become a prerequisite for later quest/objective guards through `entry.<id>.everRead`, but the existing fact-must provenance is primarily fact-flow provenance at guard slots, not a project-wide disclosure-to-line/quest/reward impact relation (`crates/lute-check/src/fact_must.rs:1-18,759-840`).
8. **Guardian disclosure ladder.** The `bossDefeated` lore entries include first-kill entries, a run-repeat kind-target entry with a Regent arm, and a fallback entry (`docs/examples/games/drowned-crown/lore/guardians.lute:12-40`). These depend on occasion and target/cadence/priority selection. `ProjectBeat` and `lute beats` can expose the ladder and static shadowing/coverage/gate verdicts (`crates/lute-check/src/beats.rs:1455-1515`; `crates/lute-cli/src/beats_cmd.rs:1-23`), but not an impact explanation tying each selected line to the changed Regent survival/death condition.

### What the current checker can see in that chain

- It can see `slew(regent)`/`felled(regent)` as literal fact-query consumers in beat/entry/quest `when`, `start`, and objective guards, subject to the fact-edge extractor's positive top-level/ground limitations (`crates/lute-check/src/fact_edges.rs:1-32,59-107`).
- It can see authored `::assert` and declared directive effect producers, including host-spliced component effects, and can classify relation-level producibility (`crates/lute-check/src/cast.rs:774-853`; `crates/lute-check/src/producible.rs:33-55`).
- It can compute guaranteed facts and provenance at guard slots, including facts implied by entry read/everRead and direct guard assumptions (`crates/lute-check/src/fact_must.rs:1-18,759-840`).
- It can see `visited('hub.regentFell')` and other `after`/start/subquest/accept edges as typed connectivity nodes/edges (`crates/lute-check/src/connectivity.rs:827-930,1164-1234`).
- It can see beat eligibility records, target expansion, cadence, after, expanded when, source anchor and extent (`crates/lute-check/src/beats.rs:1455-1515`).
- It can map compiled records to source spans, component origin, authored guards/writes, quest/objective/entry/beat declarations, and stable line IDs (`crates/lute-compile/src/source_map.rs:1-20,80-199`; `crates/lute-compile/src/address.rs:253-355`).

### What it cannot currently see or join

- No literal NPC survival/killedBy semantic state is represented in drowned-crown; death/fall is an engine/plugin event boundary, and the current content dependency graph does not link an engine event payload/target to its later `slew`/`felled` facts (`docs/examples/games/drowned-crown/README.md:11-15`; `docs/examples/games/drowned-crown/scenes/boss/regent.lute:6-20`).
- `FactEdge` has no source spans and only construct nodes; it cannot directly identify the exact assert/guard token or every affected line (`crates/lute-check/src/fact_edges.rs:39-55`).
- `FactProducers` uses unit byte-start keys and open arguments; it does not expose authored spans/text, though the separate internal `fact_writes` does (`crates/lute-check/src/cast.rs:774-853`).
- Fact edges are limited to positive top-level holds and bounded positive rule dependency; they do not model arbitrary CEL state paths, negated/compound fact conditions, reward `when`, body-level line guards, event dispatch semantics, or implicit engine writes (`crates/lute-check/src/fact_edges.rs:1-32,59-107`).
- Objective-to-reward ownership exists in IR but is not represented as an impact edge, and reward declarations do not have a dedicated source-span/index record (`crates/lute-compile/src/ir.rs:1404-1418,1506-1530`; `crates/lute-compile/src/source_map.rs:175-199`).
- Lore entry read/everRead propagation exists in must-fact analysis, but there is no reverse graph from an entry read to all lines, quest objectives, rewards, and disclosures that depend on it (`crates/lute-check/src/fact_must.rs:1-18`; `crates/lute-compile/src/source_map.rs:19-31`).
- `@def` expansion is performed and some diagnostics retain authored/expanded forms, but no def-to-use dependency index is exposed (`crates/lute-check/src/cel_expand.rs:1-90`; `crates/lute-compile/src/source_map.rs:106-130`).
- Component expansion preserves source origin and component scopes in SourceMap, but no parameter-to-expanded-node dependency graph is emitted (`crates/lute-check/src/check/component_body.rs:1219-1293`; `crates/lute-compile/src/source_map.rs:145-168`).
- Stable line IDs are in the compiled IR while most checker facts are keyed by slot spans, construct IDs, document path or byte-start unit; joining them requires compile-mapped output and is not currently part of `check`/`scenario` JSON (`crates/lute-compile/src/address.rs:253-355`; `crates/lute-compile/src/source_map.rs:1-20`).
- Existing CLI commands expose projections (`check`, `check-project`, `context`, `scenario`, `beats`, `trace`, `compile`, `loc`), but none accepts a changed node/path/fact and returns a complete impact closure with reasons and evidence levels (`crates/lute-cli/src/cmd_scenario/graph.rs:266-324`; `crates/lute-cli/src/beats_cmd.rs:1-23`; `crates/lute-cli/src/cmd_context.rs:17-29`).

## Gaps / risks observed

- The semantic evidence vocabulary requested by D9 is not yet attached uniformly to dependency records; current outputs mix diagnostics, static verdicts, must/may facts, reachability, and trace unknown (`docs/design/architecture-direction.md:206-213`).
- Engine-owned state/event producers are intentionally outside authored `FactProducers`, so impact completeness for engine-reported survival/death changes is limited (`crates/lute-check/src/cast.rs:774-853`; `docs/examples/games/drowned-crown/README.md:11-15`).
- Fact edges are construct-level and spanless, while line identity/provenance is compile-side; a complete line-level result must join separate representations (`crates/lute-check/src/fact_edges.rs:39-55`; `crates/lute-compile/src/source_map.rs:1-20`).
- Rewards, lore disclosure state, `@def`, component/template expansion, and event/occasion eligibility each have partial dependency data but no single reverse-indexed graph (`crates/lute-compile/src/ir.rs:1404-1418`; `crates/lute-check/src/fact_must.rs:1-18`; `crates/lute-check/src/cel_expand.rs:1-90`; `crates/lute-compile/src/source_map.rs:145-199`).
- Fact-edge rule traversal is bounded at depth four, so a longer derived relation chain is not represented as a complete impact path (`crates/lute-check/src/fact_edges.rs:59-64`).