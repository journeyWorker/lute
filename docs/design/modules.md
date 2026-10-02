# Domain modules and evaluation contract

**Status: normative for phase 2 (0.33).** This map turns the four phase-2
inventories into the official internal modules. A construct occurs in exactly
one row below; a semicolon-separated list is still one row whose constructs
are owned together. The inventories are research input, not authority: the
normative sources and implementation citations in this document win.

## Module registry

| module | owns | semantic ids (all version 1) |
|---|---|---|
| `core` | execution-IR envelope, baseline state ownership, CEL profile, capability gate, identity/addressing | `lute.core/1` |
| `narrative` | documents, lines, choices, match, hubs, control flow and components | `lute.core/1` |
| `staging` | background/music/sfx/vfx, sprites, camera, cuts and video | `lute.staging/1` |
| `timeline` | timing stamps and barriers | `lute.timeline/1` |
| `quest` | quest/objective lifecycle, handlers, accepts, deadlines and rewards | `lute.quest.lifecycle/1`, `lute.quest.rewards/1` |
| `time` | clock, calendar, cadence, seasons and resets | `lute.time.clock/1`, `lute.time.cadence/1`, `lute.time.seasons/1` |
| `occasions` | gates, terminal state, occasion raises and beat eligibility/selection | `lute.occasions.selection/1`, `lute.occasions.gates/1` |
| `knowledge` | vocabulary, facts, Datalog, CEL fact queries and temporal queries | `lute.knowledge.facts/1`, `lute.knowledge.rules/1`, `lute.knowledge.temporal/1` |
| `lore` | lore entries, disclosure and read state | `lute.lore/1` |

Semantic IDs are behavior IDs, not command-kind IDs. `requiredSemantics` is
compiler-derived, sorted, and cannot be authored (architecture-direction.md:133-153).
Plugins remain declarative vocabulary and typed bridges; `kind: "plugin"` is
capability-version gated and does not add a semantic ID.

### Module dependencies

| module | dependencies (reads/ordering) |
|---|---|
| `core` | none; baseline state/CEL and command walk |
| `narrative` | `core`; source organization lowers to baseline records |
| `staging` | `core`, `timeline`; presentation records consume assets and stamps |
| `timeline` | `core`; stamps and barriers wrap command execution |
| `quest` | `core`, `time`, `occasions`, `knowledge`; lifecycle reads clock/raises/facts and emits grants |
| `time` | `core`; clock/reset observations invoke `quest` and hand raise to `occasions` |
| `occasions` | `core`, `time`, `knowledge`; gates and candidate selection read current state/facts |
| `knowledge` | `core`, `time`; fact validity reads narrative time and state |
| `lore` | `core`, `knowledge`, `occasions`, `time`; disclosure effects use facts, selection, and cadence |

## Construct ownership and lowering

The columns are normative: **reads/writes** describe engine-visible state;
**evaluation** names the instant at which the owner evaluates; **guarantees**
are static guarantees only; **obligation** is what a conforming engine must do.
`IR` names the execution-IR record or the absence of one.

### `core`

| construct | syntax | IR | reads | writes | evaluation point | static guarantees | engine obligation | semantic id |
|---|---|---|---|---|---|---|---|---|
| execution IR/envelope and baseline state/CEL contract | compiled artifact, project/profile declarations | envelope `irVersion`, `celEnv`, `requiredSemantics`, `owner:engine` state | artifact declarations and capability matrix | none | load, before playback | CEL profile, format, required IDs and owned-write records are validated (scenario-dsl/0.32.0.md:135-203) | refuse before playback if any required ID is unsupported; refuse engine-owned writes | `lute.core/1` |
| addressing and stable line identity | `meta.id`, `code`, identity templates; generated `addr` | `addr`, `lineId`, `voiceKey`, source-map metadata | document/component prefixes and speaker/code | tagging may backfill source `code`; playback writes none | compile normalization, before execution | `addr` is positional; line/voice keys are stable joins (docs/runtime/execution-model.md:148-158) | execute array order and treat stable keys as opaque | `lute.core/1` |

### `time`

| construct | syntax | IR | reads | writes | evaluation point | static guarantees | engine obligation | semantic id |
|---|---|---|---|---|---|---|---|---|
| project clock; derived clock paths | schema `clock`; reserved `clock.index`, `weekday`, `weekdayLabel`, `ended` | `clock` envelope; derived paths | day/slot/week and endpoint | engine clock paths only | after each movement, before settle/raise | finite domains and read-only derived paths (`docs/proposals/scenario-dsl/0.33.0.md:31-32`) | derive exact index/weekday/end and never permit content writes | `lute.time.clock/1` |
| clock advance/calendar movement | play `advance`; beat/entry `advances` | `advances` fields; advance is driver input | current clock, bounds, raises | engine day/slot | each intermediate stop and final stop | monotone movement and declared stopping points (`docs/proposals/scenario-dsl/0.33.0.md:31-32`) | settle and raise at every specified stop; nested advances recurse | `lute.time.clock/1` |
| calendar/schedule analysis | `lute calendar` axes and declarations | analysis output, no command | bounded axes, beats, quests, clock | none | analysis only | bounded grid is not proof outside its axes | report bounded evidence, never claim unbounded absence | `lute.time.clock/1` |
| run boundary/prior mirrors/state tiers | `newRun`, `prev.*`, `state:` tiers | tier/state entries; no boundary command | run/user/app/season and prior snapshots | reset scoped tiers; write `prev` snapshots | boundary before first new-run settle | ownership and tier reset checks | reset exact tiers, preserve quest instance counters, expose prior snapshot | `lute.time.seasons/1` |
| season declaration/opening and quest rearm observation | schema `seasons`, quest `tier`, `rearm` | `seasons`, quest tier/rearm fields | live edge, prior value, cadence records | season state/cadence and season-tier scratch | false→true opening; rearm at settle start | typed live/rearm conditions | reset before same-window evaluation; no reset handler | `lute.time.seasons/1` |
| cadence (`once`, `share`, `spentBy`) | beat/entry `once`, `share`, `spentBy` | fields on beat/entry records | spend records, clock window, latch | spend records atomically | admission/presentation | legal cadence domains | preserve shared atomicity and spentBy latch | `lute.time.cadence/1` |

### `occasions`

| construct | syntax | IR | reads | writes | evaluation point | static guarantees | engine obligation | semantic id |
|---|---|---|---|---|---|---|---|---|
| occasion declaration/raise | manifest `occasions`, clock `raise`, host raise | capability/beat `on` fields; no occasion command | target, payload, gate, terminal | raise binding and presentation records | after clock-trigger observation and lifecycle settle | known occasion/target vocabulary | bind target, evaluate gate/terminal, and raise once with one eligibility snapshot | `lute.occasions.selection/1` |
| raisedWhen, terminal/outside-run | manifest gate/terminal declarations | `gates`, `terminal`, `outsideRun` | gate, terminal and persistence state | terminal record | immediately before ordinary raise | gate/terminal type and reachability checks | suppress ordinary raises when terminal; allow only declared outside-run raises | `lute.occasions.gates/1` |
| beat/entry/bundle eligibility and selection | scene/lore `on`, `when`, `priority`, `after`, `also` | beat/entry records | gates, target, facts, cadence, prerequisites | presentation/spend/read state | one snapshot at raise; priority then project order | typed guards and deterministic ties | select once; sequence/also settle between presentations | `lute.occasions.selection/1` |
### `quest`

| construct | syntax | IR | reads | writes | evaluation point | static guarantees | engine obligation | semantic id |
|---|---|---|---|---|---|---|---|---|
| quest document/root and declaration | `kind: quest`; `<quest ...>` | `Command::Quest`/`QuestCmd` | CEL, parent/child and quest state | declaration plus reserved lifecycle state | lifecycle fixpoint | admissible quest body/attributes (phase2-inventory/quest.md:26-28) | instantiate declarations and settle in deterministic project order to fixpoint | `lute.quest.lifecycle/1` |
| objective declaration/completion/visibility | `<objective done visibleWhen optional>` | `ObjectiveEntry` | done/visibility/facts/state | objective done and body-once state | continuous settle; visibility at presentation query | Bool profile; visibility never gates done | monotone done, evaluate visibility without lifecycle mutation | `lute.quest.lifecycle/1` |
| occasion objectives/target and deadlines | objective `on`, `target`, `by`, `until` | fields on `ObjectiveEntry` | raise target, done, deadline | objective done/failed | raise judgment then settle (`done` before failure) | shape and target checks | filter active/target-compatible objectives; defer `by` as specified | `lute.quest.lifecycle/1` |
| subquests | objective `quest=` and child quest | `ObjectiveEntry.quest` | child state, parent mode | parent derived done; child cascade/supersede | child activation before objective/fail/completion fixpoint | tier/reference compatibility | synthesize child-complete done; cascade terminal children | `lute.quest.lifecycle/1` |
| accept/start timing | `start`, `<accept>`, `at=nextRun`, `activate=accept` | `QuestCmd.start/accept`, `Command::Accept` | queue, run boundary, parent state | active state and activation stamp | next settle (queued next-run after reset) | acceptance syntax and references | consume once at defined settle; do not activate immediately | `lute.quest.lifecycle/1` |
| lifecycle state/failure reason | reserved `quest.*.state`, `.failedBy`, objective flags | reserved state entries | all legal CEL reads | engine-only lifecycle state | activation → objective/deadline → fail → completion → cascade | reserved paths and state domain | persist state, reason, activated time, and instance | `lute.quest.lifecycle/1` |
| rewards/grants | nested/direct `<reward>` | `QuestCmd.rewards`, `ObjectiveEntry.rewards`; grant transcript | transition, `when`, instance/index | host settlement and grant ledger | objective grant, then quest grant, before lifecycle handler | owner-relative declaration order and CEL | emit `(quest,instance,objective,index)` and deduplicate | `lute.quest.rewards/1` |
| quest handlers/occasions | `<on event when target>` | `Command::On`/`OnCmd` | one pre-event snapshot | body commands/facts/accepts | default `judge: after`: after candidate selection/presentation, before objective judgment; `judge: before`: deferred until after beats | event/target checks | execute document order; sibling handlers do not see sibling writes | `lute.quest.lifecycle/1` |
| quest graph/chapter/season metadata | `follows`, `chapters`, season metadata | `prereqEdges`, index metadata | graph and selection state | metadata only | candidate selection/indexing, not lifecycle gate | references and graph shape | keep graph selection separate from activation/settlement | — |

### `knowledge`

| construct | syntax | IR | reads | writes | evaluation point | static guarantees | engine obligation | semantic id |
|---|---|---|---|---|---|---|---|---|
| entity domains/enums | schema `entities`, `enums` | `entities[]`, `enums[]` | schema vocabulary | engine open-member ids | project resolution | closed-domain, type and membership checks | expose stable open IDs | `lute.knowledge.facts/1` |
| relations/tiers/seeds/reserved/exclusive | schema `relations`, `facts`, `reserved`, `excludes`, `tier` | `relations[]`, `seedFacts[]` | domains and tier policy | seed/engine/content fact stores | load/reset before commands | arity/domain/ownership/exclusion checks | enforce ownership, keyed replacement, tier reset and seed materialization | `lute.knowledge.facts/1` |
| assert/retract and validity intervals | `::assert`, `::retract`; implicit history | `AssertCmd`, `RetractCmd`; fact history | live facts and `validAt` time | current facts/tombstones/history | sequential command order; one token per delta | ground assert, wildcard retract, half-open intervals | apply in order, preserve history, recompute closure | `lute.knowledge.facts/1` |
| CEL fact queries | `holds`, `count`, `countDistinct` | CEL/`expr` calls | current fact store | none | each condition against current snapshot | arity and domain checks | exact matching/cardinality | `lute.knowledge.facts/1` |
| temporal queries | `validAt`, `now` | CEL/`expr` calls | fact history and clock | none | each condition against current snapshot | temporal argument checks | exact narrative ticks and half-open intervals | `lute.knowledge.temporal/1` |
| derived relations/rules/recursion/negation/guards | schema `derive`, `rules`, `not`, `cel(...)` | `RelationEntry`, `RuleEntry`, `BodyEntry` | lower strata and scalar state | derived tuples only | stratified least-fixpoint after deltas | safety, finite terms, stratification, aggregate-cycle rejection | deterministic minimal model; unknown guards derive nothing | `lute.knowledge.rules/1` |
| fact verdicts | checker analysis of guards/scenario | diagnostics, no runtime field | May/Must flow and paths | none | check/analysis | impossible/guaranteed/possible are sound and bounded | runtime remains authoritative | `lute.knowledge.facts/1` |
| cast presence/epistemics | schema `cast`; ordinary truth/belief relations/rules | line/context plus ordinary relation/rule records | presence, reserved facts, truth/testimony/belief | observations/testimony facts; derived beliefs | checker assumptions and normal fixpoint | presence implication and separated truth/knowledge | preserve holder-first tuples; enforce engine presence facts | `lute.knowledge.facts/1` |

### `lore`

| construct | syntax | IR | reads | writes | evaluation point | static guarantees | engine obligation | semantic id |
|---|---|---|---|---|---|---|---|---|
| lore disclosure/read state | `kind: lore`, `<entry>`, lore beat, body asserts | lore artifact, `EntryCmd`/`BeatCmd`, ordinary assert commands | eligibility, facts, `entry.*.read` | facts and engine read flags | eligibility, then body, then read flags | first-read effects only; entry body admission | maintain read/reread flags and apply disclosures in command order | `lute.lore/1` |
### `narrative`

| construct | syntax | IR | reads | writes | evaluation point | static guarantees | engine obligation | semantic id |
|---|---|---|---|---|---|---|---|---|
| scene document/beat and shot | scene frontmatter, `##` headings | `meta.beat`, `ShotEntry`, addressed commands | occasion, target, state, after/visited | scene scratch and body effects | eligibility at raise; commands at PC | beat attributes, shot admission | reset/present scene and walk command order | `lute.core/1` |
| line/speaker and lineId/voiceKey | `@speaker{code}: text`; identity templates | `kind: line` fields | state/defs/interpolation | none at runtime | PC presentation | speaker, uniqueness and placeholder checks | present opaque stable IDs and text | `lute.core/1` |
| branch/choice and hub | `<branch>`, `<choice>`, `<hub>`, `<return>` | `kind: choice`/`hub` | option guards, selection, visits | `scene.choices`, `scene.visited` | record admission and each hub revisit | IDs, guards, target/convergence checks | offer eligible options; route and persist visits | `lute.core/1` |
| match/when/otherwise | `<match>`, `<when>`, `<otherwise>` | `kind: match` | subject and arm tests | selected body effects | PC, top-to-bottom first match | CEL, exhaustiveness and unset semantics | choose one arm and route converge | `lute.core/1` |
| interpolation/localization | `{{...}}`, locale bundles | placeholders; `texts`, option labels | live state/target and stable IDs | locale maps only | locale selection then interpolation at render | ref/type/locale key checks | deterministic fallback policy and current-value rendering | `lute.core/1` |

### `staging`

| construct | syntax | IR | reads | writes | evaluation point | static guarantees | engine obligation | semantic id |
|---|---|---|---|---|---|---|---|---|
| components/templates and `::use` | component declarations and `::use` | compile-time expansion/source map; no runtime use | params/caller state | effect-enabled component writes | expansion before lowering | cycle, arity/type and atomic guard checks | never expose `::use`; execute expanded records | — (compile-time) |
| background/music/sfx/vfx | `::bg`, `::music`, `::sfx`, `::vfx` | corresponding command kinds | assets/timing | presentation state | command order/timeline local time | closed attrs/assets | render and honor blocking/timing | `lute.staging/1` |
| sprite/stage lifetime | `::auto` and compiler injections | `kind: sprite` | cast/stage model | stage membership/pose | authored order plus deterministic injections | cast/action checks | execute show/reposition/reset/auto-hide | `lute.staging/1` |
| camera/cut/video | directives | command kinds | transforms/assets | camera/media state | sequential or timeline time | attr/media domains | execute and honor wait | `lute.staging/1` |
### `timeline`

| construct | syntax | IR | reads | writes | evaluation point | static guarantees | engine obligation | semantic id |
|---|---|---|---|---|---|---|---|---|
| timeline/track/clip/barrier | `<timeline>`, `<track>`, `at`; `::set` | stamped flattened commands and `kind: barrier` | `Stamp.timeline`, `Stamp.at`, barrier timeline/at | clip placement and barrier join | sort clips by timeline placement; barrier joins before walk continuation | timeline placement/track overlap and barrier shape checks | preserve ordering and joins independently of payload; per-command `wait`/`duration`/`delay` stay with owning command | `lute.timeline/1` |

### `core` (plugin wire)

| construct | syntax | IR | reads | writes | evaluation point | static guarantees | engine obligation | semantic id |
|---|---|---|---|---|---|---|---|---|
| plugin directive/capability snapshot | plugin manifest, `::pluginTag`, bridge YAML | `kind: plugin`, `capabilityVersion` | typed fields and snapshot | declared effects only | command position; capability gate before playback | schema, permission and exact snapshot checks | dispatch only declared plugin; reject incompatible snapshot | `lute.core/1` |
| bridge replay/effects | `replay: recorded|deterministic|none` | plugin record effects metadata | bridge result and replay ledger | listed effects after return, declaration order | after bridge return; `wait` may block | typed result/effect paths and replay declaration | apply effects, record/replay deterministically, define host errors | `lute.core/1` |

## Global evaluation order

The following is the single cross-module order. “Instant” means one engine
observation point: load, a clock stop, a raised occasion, a presentation, or a
lifecycle re-settle. Edges are normative only where both citations exist;
otherwise the gap rule below applies.

1. **Capability snapshot/load → IR gate → execution: GAP.** The normative
   requirement is architecture-direction.md:133-153 and
   scenario-dsl/0.32.0.md:195-203, but no reference-executor capability
   loader path exists in `crates/lute-trace/src/exec/`. **Proposed:** add a
   load-phase gate before any walk and refuse unsupported required IDs.
2. **Clock advance → derived clock refresh → clock-trigger observation →
   quest settle → occasions raise.** The normative advance stops and
   settle-before-raise rule is `docs/proposals/scenario-dsl/0.33.0.md:31-32`;
   implementation is `crates/lute-trace/src/exec/session/advance.rs:145-150`
   and `crates/lute-trace/src/exec/session/step.rs:164-180`.
3. **Season/run reset → prior snapshot/queued accepts → lifecycle settle.**
   Normative reset order is runtime/quest-lifecycle.md:173-180 and
   `docs/proposals/scenario-dsl/0.33.0.md:31-32`; implementation is
   `crates/lute-trace/src/exec/session/step.rs:378-385` and
   `crates/lute-trace/src/exec/session/lifecycle.rs:21-44`.
4. **Terminal/gate check → one raise eligibility snapshot → selection.**
   `docs/proposals/scenario-dsl/0.33.0.md:32-34`;
   selection is runtime/beats-and-occasions.md:249-298. Implementation is
   `crates/lute-trace/src/exec/session/step.rs:164-180` and
   `crates/lute-trace/src/exec/machine/quest.rs:720-783`.
5. **Raise phase differs by `judge`.** For default `judge: after`, selection
   and presentation → world handlers → objective judgment → grants and
   lifecycle handlers. For `judge: before`, objective judgment → beat
   selection/presentation → deferred world handlers → grants and lifecycle
   handlers. The normative phase table is
   `docs/proposals/scenario-dsl/0.33.0.md:36-41`; implementation is
   `crates/lute-trace/src/exec/session/step.rs:191-195,330-334`,
   `crates/lute-trace/src/exec/session/lifecycle.rs:101-111,245-257`, and
   `crates/lute-trace/src/exec/machine/quest.rs:293-295`.
6. **Presentation → knowledge delta/closure → quest re-settle → next
   presentation.** Normative phase and fact-delta order are
   `docs/proposals/scenario-dsl/0.33.0.md:36-41` and
   `docs/proposals/scenario-dsl/0.32.0.md:110-133`; implementation is
   `crates/lute-trace/src/exec/session/lifecycle.rs:21-44` and
   `crates/lute-trace/src/exec/machine/quest.rs:430-610`.
7. **Quest objective grant → quest grant → lifecycle handler → cascade.**
   Normative grant order is runtime/quest-lifecycle.md:560-645 and
   implementation is `crates/lute-trace/src/exec/machine/quest.rs:580-720`.
8. **Beat presentation → beat `advances` → recursive clock movement.**
   Normative rule is `docs/proposals/scenario-dsl/0.33.0.md:31-32`; implementation is
   `crates/lute-trace/src/exec/session/advance.rs:145-150`.
9. **Timeline expansion/flattening → stamped ordering → barrier → next
   command: GAP.** The normative flattening rule is
   `docs/proposals/scenario-dsl/0.33.0.md:29-30`; the reference executor explicitly
   records a non-executing barrier at
   `crates/lute-trace/src/exec/machine/commands.rs:333-340` and says no real
   timeline clock exists in `crates/lute-trace/src/exec/machine/mod.rs:56-60`.
   **Proposed:** implement local-clock sorting and make the barrier wait for
   all clips before the next command (including the documented failure policy).

### Explicit gaps and proposed rules

The following orderings are not currently specified; the proposal is explicit
so an engine cannot silently choose a different meaning.

- **Target binding → `raisedWhen`** is implied but not universal
  (`docs/proposals/scenario-dsl/0.33.0.md:32-34`). **Proposed:** bind target/payload first,
  then evaluate gate, terminal, and beat guards against that binding.
- **Intermediate clock engine writes → settle** is ambiguous
  (`docs/proposals/scenario-dsl/0.33.0.md:31-32`). **Proposed:** apply writes at each stop,
  refresh derived paths, settle, then raise.
- **Terminal re-check between nested/intermediate raises** is unspecified
  (`docs/proposals/scenario-dsl/0.33.0.md:32-34`). **Proposed:** re-check terminal before
  every raise, including each intermediate and nested `advances` stop.
- **Beat body/`::end` → `advances`** is unspecified
  (`docs/proposals/scenario-dsl/0.33.0.md:31-32`). **Proposed:** execute the complete presented
  body and its end segment, settle, then perform `advances`.
 - **Same-time timeline clips** lack observation semantics
   (`docs/proposals/scenario-dsl/0.33.0.md:29-30`). **Proposed:** sort by
   `(at, track index, source index)` and apply state writes in that order; each
   later clip observes earlier writes.
- **Plugin nonblocking completion → barrier** lacks failure/timeout semantics
  (runtime/bridge-protocol.md:69-99). **Proposed:** a barrier waits for all
  clips; timeout/error aborts the barrier and emits no later clip effects.
 - **Fact delta batching → condition snapshot** lacks one universal boundary
   (`docs/proposals/scenario-dsl/0.33.0.md:33-34`). **Proposed:** each
   command/event is one token; apply all writes in command order, close
   derived facts, then expose the resulting snapshot to the next condition.
- **Overlapping season openings** have no order
  (`docs/proposals/scenario-dsl/0.33.0.md:31-32`). **Proposed:** sort season IDs by canonical
  project order and open/reset in that order; all opens complete before settle.

Every proposed rule above MUST become a cited normative spec rule and a
reference-executor conformance case before it is treated as settled.
