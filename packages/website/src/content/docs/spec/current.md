---
title: Current specification
description: The consolidated index of what the Lute language enforces today at version 0.25.1 — each language area mapped to the versioned proposal that introduced or last changed it, all pointing back to the normative repository sources.
---

The versioned proposal stack under
[`docs/proposals/scenario-dsl/`](https://github.com/journeyWorker/lute/tree/main/docs/proposals/scenario-dsl)
**remains the normative source of truth**. This page does not replace it — it is
the consolidated **index** of what is *current* at language version **0.25.1**:
for each language area, which proposal revision introduced it, which last changed
it, and where to read the normative text.

:::note
Where this index and a proposal disagree, the proposal in the repo wins. For the
full cumulative history (including the pre-implementation `0.0.1` draft and the
capability proposals), see the [specification index](/spec/).
:::

## What is current at 0.25.1

| Language area | Introduced | Last changed | Normative source |
|---|---|---|---|
| Frontmatter & profiles | 0.1.0 | 0.25.0 (a scene beat may name a `share:` key beside a written `once`, spent together with every beat of the key; `0.24.0`: scene and bundle beats take `once: day` / `once: slot` when the project declares a clock — without one, `E-BEAT-ATTR`; `0.21.0`: a scene becomes a beat with the scene-only keys `on:` / `target:` / `when:` / `priority:` / `once:`; `when`, `target`, `priority`, or `once` without `on` is `E-BEAT-ATTR`) | [0.25.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.25.0.md); [0.24.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md); [0.21.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.21.0.md) |
| Content lines (`@speaker` dialogue) | 0.1.0 | 0.25.0 (a reserved relation's `changedOn:` names the occasions on which the engine changes it, and cast `assume: true` then no longer covers it in a unit presented on one of them, in that unit's after-descendants, or under a guard that needs a fact of it; the `{{x:ordinalWord}}` format hint — `lute play` renders `first` … `twentieth`, the engine localizes; `0.24.0`: a cast entry may declare `present:` — a line whose guards do not imply it is `W-CAST-ABSENT`, `assume: true` reads a negated engine-reserved fact as true — and `emotions:`, outside which `emotion=` is `E-BAD-ENUM`; the `{{x:ordinal}}` format hint, and `{{path}}` of an enum-typed path renders the member's display `labels:`; `0.23.0`: a declared cast — a plugin `cast` export or a schema document's `cast:` — makes a speaker outside it `E-CAST-UNKNOWN` with a did-you-mean; without a cast speakers stay shape-only; `0.5.1`: delivery-flag authoring-surface honesty) | [0.25.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.25.0.md); [0.24.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md); [0.23.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.23.0.md); [0.5.1.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.5.1.md) |
| Core directives (the closed `lute.core` vocabulary) | 0.1.0 | 0.24.0 (`::clear` — every character on stage exits, background and music stay — moves the `lute.core` stamp for every document; `::set{… when="…"}` writes only when its guard holds; `::auto{character}` / `::camera{focus}` are held to a declared cast; `0.9.0`: `::auto{action}` and `::music{mood}` retyped from free `string` to `{ domain: … }`, so both slots are checkable at last; the core's own member lists emptied) | [0.24.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md); [0.9.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.9.0.md) |
| Content vocabulary (`emotion`, `action`, `anchor`, `mood`, `volume`, `musicAction`, `vfxType`) | 0.1.0 (closed member lists shipped inside `lute.core`) | 0.9.0 (**the project owns the members** — the compiler declares slots and ships none; three declaration routes; the `exits:`/`default:` long form; using an undeclared slot is `E-DOMAIN-UNKNOWN`) | [0.9.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.9.0.md) |
| Branch / match / when / hub | 0.1.0 | 0.24.0 (a beat's `when:` narrows the subject for its whole body, so an arm it rules out is `E-ARM-DEAD` and `E-NONEXHAUSTIVE` reads the same narrowed domain; a `<match on="@def">` takes the def's path or declared type as its domain; `0.23.1`: an arm narrows its subject: inside `<when is="x">` with no `unset` alternative the subject is set, and once an arm takes every unset value later arms and `<otherwise>` read it as set — no `E-MAYBE-UNSET` there; `0.23.0`: `<hub prompt="…">` — the question shown with a hub's options, IR `HubCmd.prompt`, an empty prompt is `E-BRANCH-PROMPT`; `0.18.0`: inclusive numeric ranges `N..M` / `N..` / `..M` in `is=`; a `number` subject gets interval coverage over the reals, so `E-NONEXHAUSTIVE` names the first uncovered gap and `E-ARM-DEAD` / `W-OVERLAP-ARMS` / `W-OTHERWISE-DEAD` are interval-aware; `E-WHEN-RANGE` for a malformed or empty range) | [0.24.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md); [0.23.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.23.0.md); [0.18.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.18.0.md); [CHANGELOG `0.23.1`](https://github.com/journeyWorker/lute/blob/main/CHANGELOG.md) |
| Reusable content components (`::use` expansion) | 0.1.0 | 0.24.0 (`effects: true` — a component may `::set` / `::assert` / `::retract`, each write checked at every `::use` against the host's schema and compiled where the `::use` sits; `speaker` params take a cast id; `0.23.0`: a component `string` param may be interpolated as `{{@p}}` — a literal `::use` argument is substituted per call site under that call's own `lineId`, and binding the param to a `@def` is `E-REF-TYPE`; `0.22.0`: a component line gets its own identity scope — `{prefix}.{component}#{n}.{speaker}_{code}`, `n` the host's 1-based `::use` ordinal of that component — so two uses never share a `lineId`; `0.9.0` made five root-only check stages now run over an imported component body — content-line attrs, `E-DUP-LINE-CODE`, reachability, unwalked-content admission, injection folding) | [0.24.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md); [0.23.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.23.0.md); [0.22.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.22.0.md); [0.9.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.9.0.md) |
| `into=` records (choice run-record sugar) | 0.1.0 (as `persist=`/`into=`, renamed from `0.0.1` `as`) | 0.6.0 (**breaking** — `persist=` removed, `into=` alone records) | [0.6.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.6.0.md) |
| State tiers (scalar `scene`/`run`/`user`/`app`) | 0.1.0 | 0.24.0 (entity-indexed state — `{ type: number, default: 0, per: <kind> }` declares one path per member, with per-member defaults `{ _: 0, <member>: 1 }`; with a declared clock the reserved read-only `clock.index` / `clock.weekday` / `clock.weekdayLabel`; integer `%` joins the CEL profile; `0.23.0`: the reserved read-only `prev.run.<path>` mirrors every declared `run.<path>` with the value it had when the previous run ended — maybe-unset, so a read needs `isSet` or an `unset` arm, a write is `E-QUEST-RESERVED-WRITE`, and declaring `prev.*` is `E-STATE-NAMESPACE`; `0.22.0`: a `state:` declaration may carry `owner: engine` — content that `::set`s it is `E-ENGINE-OWNED-WRITE`, reads stay unrestricted; the reserved user-tier `entry.<id>.everRead` joins the run-tier `entry.<id>.read`; `0.8.0` enforced scalar-only author `state:` — `E-STATE-COLLECTION`) | [0.24.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md); [0.23.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.23.0.md); [0.22.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.22.0.md); [0.8.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.8.0.md) |
| Facts & Datalog (relational layer) | 0.3.0 | 0.25.0 (a relation may declare `excludes:` — symmetric, partners with the same argument kinds or `E-RELATION-DECL`; `check-project` reads a guard needing both as dead and a negated partner as following, an `::assert` where the excluded fact holds on every route is `E-FACT-EXCLUSIVE`, a rule deriving one side from the other on the head's own arguments is `E-RULE-EXCLUSIVE`, and `lute play` / `lute trace` / `lute test` halt at a write that makes both hold; a reserved relation may declare `changedOn:` — on a non-reserved relation or naming an undeclared occasion it is `E-RELATION-DECL`; `0.24.0`: `_` in a rule body is a fresh anonymous variable, existential under `not`; `countDistinct(<pattern>, <Var>)`; entity sub-kinds (`subsetOf:`) and an entity kind atom in a rule body as a membership test; a rule `cel()` guard may read `F[P]` of a `per:` family, compiled grounded, one IR rule per member; `E-RELATION-RESERVED-NAME`) | [0.25.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.25.0.md); [0.24.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md); [0.3.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md) |
| Quests (`<quest>`, `<on>` ECA triggers) | 0.2.0 | 0.25.0 (`<quest accept="external">` — accepted outside the script — is the only acceptance outside content that silences `W-QUEST-NEVER-ACCEPTED`, a test or trace `accepts:` mock no longer does, and beside `start` it is `E-ATTR-TYPE`; `lute scenario` draws `[subquest]` edges and anchors a quest by the top-level `start` conjuncts that read `visited()`, `entry.X.everRead` or `quest.Y.state` (`[start]` edges); `0.24.0`: `<quest activate="accept">` subquests wait for an `::accept` while the parent is active; `<quest complete="any">` alternatives fail the rest as `superseded`; `by=` is judged at every settle and the place-bound rule is the new `until=`, with `W-DEADLINE-BEFORE-DONE` for a `done` that implies its `by`; the read-only `quest.<id>.failedBy` / `objectives.<o>.failed`; `<on event target>`; `::accept{at="nextRun"}`; `W-QUEST-NEVER-ACCEPTED`; `0.23.1`: `E-QUEST-TIER-MIX` — a subquest whose `tier` differs from its parent's, since a mixed tree locks for good; an `on=` objective's `by` is judged only when its occasion is raised, right after its `done`; a raised occasion first fires a same-named declared world event, so every active quest's `<on event>` handlers run before its `on=` objectives are judged; `0.23.0`: `<objective by="…">` deadlines — the first time `by` holds while the objective is not done, it fails, and a failed required objective fails the quest; `<objective on target>` is judged only when the occasion is raised for that target, IR `ObjectiveEntry.by` / `target`; `0.22.0`: `<quest tier="run">` — status and objectives reset to `unset` when a run starts, default `tier="user"` persists across runs, IR `QuestCmd.tier`; `check-project` warns `W-QUEST-HANDLER-DEAD` on a `questFailed` handler of a quest that can never fail; `0.14.0` added subquests — `<objective quest="childId"/>` makes a child quest's completion a parent objective; synthesized `done`/upward `fail`, engine-rule downward cascade and referenced-child activation; tree-not-DAG project shape with four new diagnostics) | [0.25.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.25.0.md); [0.24.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md); [0.23.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.23.0.md); [0.22.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.22.0.md); [0.14.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.14.0.md); [CHANGELOG `0.23.1`](https://github.com/journeyWorker/lute/blob/main/CHANGELOG.md) |
| Timeline & property tracks | 0.1.0 | 0.1.0 | [0.1.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.1.0.md) |
| Connectivity & `after:` sequencing | 0.2.0 (`after:` scene sequencing) | 0.25.0 (a bundle `<beat after=…>` is a scenario edge like a scene's `after:`; nested quests draw `[subquest]` edges and a quest's top-level `start` reads draw `[start]` anchors, so a `start`-driven quest needs no copied `after=`; `lute scenario` lists a unit whose `when` reads `visited()` without an `after` as unanchored, with the `after` to write; `0.8.0`: `active("questId")` — the third prerequisite primitive) | [0.25.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.25.0.md); [0.8.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.8.0.md) |
| Identity & localization (`lineId` / `voiceKey`, locale texts) | 0.1.0 | 0.23.1 (an untagged component line takes the code `lute tag` would write into the component file before a param-scoped `<match>` folds, so a line in a non-first arm gets a new `lineId` / `voiceKey` once — the one `lute tag` persists; `0.22.0`: **breaking for compiled ids** — the default `voiceKey` becomes `{prefix}.{speaker}-{code}`, pin `identity.voiceKey: "{speaker}-{code}"` to keep audio recorded against the old keys; a component line is addressed `{prefix}.{component}#{n}.{speaker}_{code}`; `0.21.1` added `E-DUP-VOICEKEY` — `check-project` and `compile --all` refuse a `voiceKey` carried by lines with different text; `0.8.0` introduced `identity:` templates and the `loc import` → `compile --locales` round trip) | [0.22.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.22.0.md); [0.8.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.8.0.md); [CHANGELOG `0.21.1` / `0.23.1`](https://github.com/journeyWorker/lute/blob/main/CHANGELOG.md) |
| Compiled artifact shape (`addr` addressing, IR carriers) | 0.1.0 | 0.25.0 (additive: optional `RelationEntry.excludes`, `share` on `BeatIr` / `EntryCmd` / `BeatCmd` / index beat rows, a bundle `BeatCmd.after` with its `prereqEdges` row, `QuestCmd.accept` (`"external"`), and the placeholder format `"ordinalWord"`; `0.24.0`: additive: an optional `clock` on the artifact and `ProjectIndex`, beat and entry `once` `day` / `slot`, `ObjectiveEntry.until`, `QuestCmd.activate` / `complete`, `OnCmd.target`, `AcceptCmd.applies`, placeholder `format`, `StateEntry.labels`, and the CEL binary op `%`; a rule reading entity-indexed state compiles to one grounded rule per member; `0.23.0`: additive: the `beat` command heads each bundle beat's addressing unit in a lore artifact; optional `BeatIr.also`, `ObjectiveEntry.by` / `target`, `HubCmd.prompt`, `RewardEntry.credits`; `ProjectIndex.beats` rows gain kind `bundle`, `when`, and `title`; `0.22.0`: additive: optional `QuestCmd.tier` — `"run"`, omitted for the default `user` — and `EntryCmd.once`, also on the `ProjectIndex.beats` entry rows; content moves for documents on the default `voiceKey` or with `::use` components; `0.21.1` gave a `ref` placeholder optional `expr`, the referenced def body inlined as a `{raw, expr}` CEL pair so an engine renders `{{@def}}` without a defs table; `0.21.0` added optional `SceneMeta.beat` — `on`, `target?`, `when?`, the resolved `priority`, and `once` as `"run"` / `"user"` / `"none"` — optional `EntryCmd.on` / `priority`, and `ProjectIndex.beats` in selection-tiebreak order; optional `ObjectiveEntry.on` and the new `accept` record for `::accept{quest}`; a `visited()` condition carries `raw` only, like `holds()`; artifacts using none of it are byte-identical apart from the version strings) | [0.25.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.25.0.md); [0.24.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md); [0.23.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.23.0.md); [0.22.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.22.0.md); [0.21.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.21.0.md); [CHANGELOG `0.21.1`](https://github.com/journeyWorker/lute/blob/main/CHANGELOG.md) |
| Warning-severity diagnostics (`W-LUTE-VERSION-STALE`, `W-TRACE-MOCK-UNPRODUCIBLE`, `W-CODE-AFTER-END`, `W-L10N-MISSING`, `W-FACT-GUARANTEED`, `W-BEAT-SHADOWED`, `W-QUEST-STATE-ISSET`, `W-TEXT-LOOKS-LIKE-REF`, `W-QUEST-HANDLER-DEAD`, `W-BEAT-PRIORITY-TIE`, `W-BEAT-ONCE-RUN-USER`, `W-REWARD-DOUBLE-CREDIT`, `W-CAST-ABSENT`, `W-DEADLINE-BEFORE-DONE`, `W-QUEST-NEVER-ACCEPTED`, `W-RELATION-UNREAD`, `W-DEF-UNUSED`) | 0.6.1 | 0.25.0 (`W-LUTE-VERSION-STALE` for a stamp inherited from the manifest's `defaults:` is reported once, at the manifest; `W-QUEST-NEVER-ACCEPTED` no longer counts a mock's `accepts:`; a negation that a declared exclusion implies is `W-FACT-GUARANTEED`; `0.24.0`: `W-CAST-ABSENT` — a line whose guards do not imply its speaker's cast `present:`; `W-DEADLINE-BEFORE-DONE` — an `on=` objective whose `done` implies its `by`; `W-QUEST-NEVER-ACCEPTED` — an accept-driven quest nothing accepts; `W-RELATION-UNREAD` / `W-DEF-UNUSED` — a relation nothing queries, a def nothing references; `0.23.0`: `W-REWARD-DOUBLE-CREDIT` — a reward whose kind `credits` a state path beside a handler `::set` of that same path pays twice; `0.22.0`: `W-QUEST-HANDLER-DEAD` — a `questFailed` handler on a quest with no way to fail; `W-BEAT-PRIORITY-TIE` — equal-priority beats on one `select: first` occasion whose `when`s are not provably exclusive, so file order picks; `W-BEAT-ONCE-RUN-USER` — a `once: run` beat gated only on user-tier state replays every run; `W-STAGE-ABSENT` follows paths, forking at each arm and joining at convergence; `0.21.1` added `W-QUEST-STATE-ISSET` — `isSet(quest.<id>.state)` is always true, compare with `== 'unset'`; `W-TEXT-LOOKS-LIKE-REF` — a line whose whole text is `@name` for a def or param ships that literal text; `0.21.0` added `W-BEAT-SHADOWED` — `check-project` warns when a `select: first` beat can never win because an earlier-ordered beat on the same occasion and target is always eligible and never spent; `0.20.0` added `W-FACT-GUARANTEED` and removed `W-UNPROVEN-RELATIONAL`, one of the three original 0.6.1 coverage warnings — naming it in `--deny` is a usage error) | [0.25.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.25.0.md); [0.24.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md); [0.23.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.23.0.md); [0.22.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.22.0.md); [0.21.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.21.0.md) |
| Deny promotion (`--deny` / `--deny-warnings`) | 0.6.1 | 0.6.1 | [0.6.1.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.6.1.md) |
| Version stamp & axis alignment | 0.1.0 | 0.13.0 (the runtime version-negotiation gate relaxes to **MAJOR-only** — minor/patch are compatible-by-default, fields append-only within a major line — so `0.21.0`'s additive IR move — beat fields an engine without beat support ignores — `0.21.1`'s optional `placeholder.expr`, `0.22.0`'s optional `QuestCmd.tier` / `EntryCmd.once`, `0.23.0`'s optional beat, objective, hub and reward fields, `0.24.0`'s optional `clock`, `until`, `activate` / `complete`, handler `target`, accept `applies`, placeholder `format` and state `labels`, and `0.25.0`'s optional relation `excludes`, beat and entry `share`, bundle beat `after`, quest `accept` and the `ordinalWord` format cost a consuming engine nothing — only `0.23.0`'s new `beat` record, which appears only in a lore artifact that bundles beats, is refused by an engine that predates it; `0.23.1` and `0.25.1` move the version strings and no shape; the schema file renames per release line and keeps its name within one, published today as `lute-ir-0.25.schema.json`) | [0.13.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.13.0.md) |
| Scene & document identity (`id:` frontmatter) | 0.15.0 (authored canonical scene key; new `id:` frontmatter is the lineId prefix, `visited()`/connectivity node, and `prereqEdges[].node`, superseding the derived `{character}.{episodeId}` join wherever it was consumed; `character`/`season`/`episode`/`episodeId` demote to optional when `id:` is present) | 0.19.0 (quest and lore documents may declare an optional document `id:` — the bundle name, the artifact's `meta.id`, and its `ProjectIndex` key; document ids share one project-wide namespace with scene ids, so `E-META-ID` and `E-CONN-EPISODE-ID-DUP` extend to them) | [0.19.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.19.0.md) |
| Descriptive `extra:` block | 0.15.0 (open mapping of scalars or flat scalar-lists on scene and quest roots; carried verbatim into `meta.extra` and read by no language rule — `E-META-VALUE` on a nested mapping or non-scalar list entry) | 0.15.0 | [0.15.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.15.0.md) |
| Legacy identity keys (`character` / `season` / `episode` / `episodeId`) | 0.1.0 | 0.15.0 (deprecated in prose only — `W-META-LEGACY` warns per legacy key when a document also authors `id:`; the four keys are no longer required when `id:` is present, removal deferred to a future major) | [0.15.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.15.0.md) |
| Declarative rewards (quest / objective `<reward/>`, range amounts) | 0.16.0 (self-closing `<reward kind= target= amount= when= on=/>` as a direct child of `<quest>` or `<objective>` — pure data on the owning records, `QuestCmd.rewards` / `ObjectiveEntry.rewards`; `amount` admits an integer scalar or the range literal `N..M`, negatives real; two new diagnostics `E-REWARD-ATTR` shape / `E-REWARD-KIND` vocabulary; `reward.when` joins the CEL-slot registry; `lute run` / `play` / `trace` emit deterministic `grant` transcript events at each fresh transition — the engine grants, the reference runtime never rolls a range) | 0.16.0 | [0.16.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.16.0.md) |
| Reward-kind plugin vocabulary (`rewardKinds:` manifest export) | 0.16.0 (optional plugin-manifest map of kind id → contract — optional `target` provider domain and optional extra attr schema — that makes `<reward kind=>` / `target=` statically checkable when declared; folded into the capability snapshot as a guarded, sorted section so `capabilityVersion` moves only for projects that install a `rewardKinds:`-declaring plugin; the empty core section hashes byte-identically) | 0.23.0 (a reward kind may declare `credits: <state path>`, stamped on each reward as `RewardEntry.credits`; `lute run` / `play` add a scalar amount there on grant) | [0.23.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.23.0.md); [0.16.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.16.0.md) |
| Plugin passthrough ownership | 0.17.2 (optional `plugin` owner id on `kind: "plugin"` records; hosts dispatch by `(plugin, tag)`; older artifacts may omit the field) | 0.17.2 | [0.17.2.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.17.2.md) |
| Lore entries (queried content) | 0.19.0 (`kind: lore` documents of top-level `<entry>` declarations — content the engine looks up rather than plays; `id` / `target` / `category` / `title` / `series` / `order` / `when` attributes, `target` and `category` shape-only; bodies admit content lines, `<match>`, `::set`, `::assert`, `::retract`; knowledge revealed by plain `::assert`; the reserved engine-written `entry.<id>.read` path; first-read-only effects; a document-level `series:` ordering entries by file position; new `E-ENTRY-ATTR`, `E-ENTRY-ID-DUP`, `E-ENTRY-SERIES-ORDER`) | 0.25.0 (`<entry share=…>` beside a written `once` joins a shared spend — reading the entry spends every beat of the key; `0.24.0`: an entry's `target=` on an untargeted occasion is metadata; entries take `once="day"` / `once="slot"` with a declared clock; under `entry.X.read` the facts X's body asserts on every route join the must set; `0.23.0`: a lore document may bundle scene-like `<beat id on target title when priority once also>` blocks beside its entries, canonical id `<document id>.<beat id>`; an entry's or beat's body segment runs to the next `entry` or `beat` record; `0.22.0`: an entry beat takes `once="run" \| "user"` — not eligible once `entry.<id>.read` / the new user-tier `entry.<id>.everRead` is set, absent = repeatable; `0.21.0` let an entry answer an engine occasion with `on=` / `priority=` beside its `when` / `target`, making it an entry beat) | [0.25.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.25.0.md); [0.24.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md); [0.23.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.23.0.md); [0.22.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.22.0.md); [0.21.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.21.0.md) |
| Relational guard analysis (fact envelopes) | 0.20.0 (`check-project` decides every `holds(…)` / `count(…)` in a guard slot as **impossible**, **guaranteed**, or **possible** from a project-wide, argument-level *may* set and a path-sensitive *must* set of monotone facts propagated across the `after:` graph; guards are assumptions inside their regions and `count` is decided over an interval; a dead relational guard reuses its slot's code — `E-ARM-DEAD`, `E-OBJECTIVE-UNSATISFIABLE`, `E-QUEST-UNREACHABLE`, `W-OBJECTIVE-HIDDEN` — plus the new `E-ENTRY-UNREACHABLE` for a lore entry `when`, and a guaranteed one is `W-FACT-GUARANTEED`; project-level only — single-file `check` leaves relational queries undecided; `lute scenario … envelope` lists the guaranteed facts at a node) | 0.25.0 (a declared `excludes:` pair decides a guard needing both false and a negated partner true — in the same guard, an enclosing one, or on every route; `0.23.0`: the decider reasons per path across `&&` / `||` — a conjunction whose operands on one path cannot all hold decides false, a disjunction covering the path's domain decides true, `unset` counted as a value so every verdict is the expression's real value; `check-project --wip` downgrades a dead guard caused only by a relation with no producer to a warning) | [0.25.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.25.0.md); [0.23.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.23.0.md); [0.20.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.20.0.md) |
| Beats and occasions (story selection) | 0.21.0 (a scene or lore entry becomes a **beat** by naming the engine **occasion** it answers — scene frontmatter `on:` / `target:` / `when:` / `priority:` / `once: run \| user \| false`, entry attributes `on=` / `priority=` beside `when` / `target`; eligible when `after:` and `when` hold and `once` is unspent, ordered by priority then `ProjectIndex.beats` order; an optional plugin `occasions:` export — `select: first \| all`, `target: true` — folded into the capability snapshot as a guarded section, occasion names shape-only until some plugin declares them; new `E-BEAT-ATTR`, `E-OCCASION-UNKNOWN`, `E-BEAT-UNREACHABLE`, `W-BEAT-SHADOWED`; `when` joins the CEL-slot registry; `schedule.yaml` and every `E-SCHED-*` / `W-SCHED-*` code **removed**, `lute play` rebuilt on raised occasions and driving quest lifecycles — see [Playing a story](/tooling/play/)) | 0.25.0 (a scene `share:`, bundle `<beat share=…>` or `<entry share=…>` beside a written `once` names a project-wide key whose beats are spent together — a malformed key, a `share` without a spending `once`, or beats of one key with different `once`s are `E-BEAT-ATTR`; a bundle `<beat after=…>` is an eligibility conjunct; `0.24.0`: occasion `judge: before` judges its `on=` objectives and settles the quests before its beats are decided; `once: day` / `once: slot`; a bundle beat may be an `after:` predecessor and an `::accept` anchors an accept-driven quest; `0.23.1`: an occasion `target:` domain may narrow to a member subset, `{ prefix, entity, members: [...] }` — a member outside a closed kind is `E-BEAT-ATTR`, and the list restamps `capabilityVersion` only where declared; raising an occasion also fires a same-named declared world event; `W-BEAT-ONCE-RUN-USER` fires only on a defaulted `once`; `0.23.0`: occasion `select: sequence` presents every eligible beat in order; a scene or bundle beat's `also: true` rides along after a `select: first` winner and is ignored by `W-BEAT-SHADOWED` / `W-BEAT-PRIORITY-TIE`; bundle beats in lore documents; index beat rows carry `when` and `title` for menus; `0.22.0`: an occasion's `target:` may name a domain, `{ prefix, entity }`, so a beat target must be `<prefix>.<member>` of that `entities:` kind — `E-BEAT-ATTR` with a did-you-mean; entry beats take `once`; `pick: none` passes on a `select: all` list; new `W-BEAT-PRIORITY-TIE` and `W-BEAT-ONCE-RUN-USER`) | [0.25.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.25.0.md); [0.24.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md); [0.23.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.23.0.md); [0.22.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.22.0.md); [0.21.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.21.0.md); [CHANGELOG `0.23.1`](https://github.com/journeyWorker/lute/blob/main/CHANGELOG.md) |
| Play / test harness (`lute play` scripts, derivation in trace and test) | 0.22.0 (`engine:` play steps write engine-owned state and facts, reserved relations included; a script starts from a save — `visited:`, `presented:`, `quests:`, `entriesRead:` — varies `choose:` per step, fires world events with `event:`, and asserts with step and top-level `expect:`; `lute test` runs every play carrying an `expect:`; `lute trace` / `test` / `play` apply seed facts and Datalog rules by default, `derive: false` / `--no-derive` restores the `0.21` answer) | 0.25.0 (a write that makes two exclusive facts hold halts `lute play` at the step (exit 1) and refuses the `lute trace` / `lute test` walk with `E-FACT-EXCLUSIVE`, and seeded facts that already break an exclusion refuse the run; a `bridges:` answer may omit result fields no content reads, and `lute play` no longer halts at a call none of whose results are read; `0.24.0`: `advance:` and `include:` play steps move a declared clock and raise its occasions; a plugin call is answered by `bridges:` in play, test and trace — a `scene.*` seed no longer answers it; `lute calendar` gains `clock`, `visited()` and per-occasion axes; `0.23.1`: a `::end` in `lute play` ends only the presentation it runs in and a step `end: true` ends the playthrough; step `expect:` gains `quests` / `state` / `facts` / `notFacts`; a `*.test.yaml` gains `expect.facts` / `notFacts` / `eligible` and `beat:`; trace and test apply reward `credits:` and walk objective bodies as play does; `lute calendar --script` replays the script's steps; `0.23.0`: `lute beats`, `lute calendar` and `lute scenario knowledge` overviews; play presents `sequence` lists and `also` beats, gains the `presented:` expectation, snapshots `prev.run` at `newRun`, and credits rewards; trace and run raise `<occasion>@<target>` and present one bundle beat with `--beat`) | [0.25.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.25.0.md); [0.24.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md); [0.23.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.23.0.md); [0.22.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.22.0.md); [CHANGELOG `0.23.1`](https://github.com/journeyWorker/lute/blob/main/CHANGELOG.md) |
| Clock (`clock:` schema declaration) | 0.24.0 (a schema may declare one `clock: { day, slot, slots, raise, week }` over two `owner: engine` paths; content reads the reserved read-only `clock.index` / `clock.weekday` / `clock.weekdayLabel`; beats spend `once: day` / `once: slot`; `raise:` names the occasion an advance raises, or a map `{ slot, dayStart, dayEnd }`; a clock may omit `slot` / `slots` and count whole days; a malformed clock is `E-CLOCK-DECL`) | 0.24.0 | [0.24.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md) |


## Notes on the boundaries

- **`0.25.1` is a performance patch on the `0.25` line: no language change,
  no IR shape change.** Every `0.25.0` document checks and compiles as before;
  artifacts differ only in the version strings, and
  `schemas/lute-ir-0.25.schema.json` keeps its name and `$id`. The release
  record is the
  [changelog](https://github.com/journeyWorker/lute/blob/main/CHANGELOG.md) and
  the [versioning guide](https://github.com/journeyWorker/lute/blob/main/docs/versioning.md).
- **`0.25.0` is additive in syntax, but a few answers move.** Every `0.24.0`
  document parses as before and the new syntax — `excludes:`, `changedOn:`,
  `share`, a bundle `<beat after=…>`, `accept="external"`,
  `{{x:ordinalWord}}` — is opt-in. What moves: a quest only a test or trace
  mock `accepts:` warns again (`W-QUEST-NEVER-ACCEPTED`; declare
  `accept="external"`); a write that breaks an exclusion halts `lute play`
  and `lute trace`; a project whose rules contradict a declared exclusion is
  `E-RULE-EXCLUSIVE`; `W-LUTE-VERSION-STALE` for a stamp inherited from the
  manifest is reported once, at the manifest; and a bridge answer may omit
  fields no content reads, so `lute play` no longer halts at a call whose
  results nobody reads. `capabilityVersion` does not move. The release record
  is the
  [changelog](https://github.com/journeyWorker/lute/blob/main/CHANGELOG.md).
- **`0.24.0` is additive in syntax, but a few answers move.** Every `0.23.1`
  document parses as before and the new syntax — `clock:`, `once: day|slot`,
  `activate=`, `complete=`, `until=`, `<on target>`, `per:`, `subsetOf:`,
  cast `present:` / `emotions:`, `effects: true`, `::clear`, `%`, a guarded
  `::set`, `{{x:ordinal}}` — is opt-in. What moves: `by=` is judged at every
  settle (the old place-bound rule is `until=`); a beat's `when:` narrows its
  body, so a match arm kept only to satisfy the checker is now `E-ARM-DEAD`;
  a plugin call's result is answered by `bridges:`, not a `scene.*` seed; a
  cast that declares `present:` checks every line (`W-CAST-ABSENT`); and
  `capabilityVersion` moves for every document because `lute.core` gains
  `::clear`. The release record is the
  [changelog](https://github.com/journeyWorker/lute/blob/main/CHANGELOG.md).
- **`0.23.1` is a patch on the `0.23` line: no new syntax, no IR shape
  change.** A subquest whose `tier` differs from its parent's is now
  `E-QUEST-TIER-MIX`; an occasion target domain may narrow to `members:`; a
  raised occasion first fires a same-named declared world event (every active
  quest's `<on event>` handlers) and then judges its `on=` objectives, whose
  `by` deadline is judged only there, after `done`. An untagged component line
  takes the code `lute tag` would write, so its `lineId` may change once. The
  spec text is still the proposal that introduced each construct; the release
  record is the
  [changelog](https://github.com/journeyWorker/lute/blob/main/CHANGELOG.md) and
  the [versioning guide](https://github.com/journeyWorker/lute/blob/main/docs/versioning.md).
- **`0.23.0` is additive, but a sharper checker can redden a clean
  project.** Every `0.22.0` document parses and compiles as before (only the
  version strings move); the new syntax — `by=`, objective `target=`,
  `also:`, `select: sequence`, `<beat>` in lore, `<hub prompt>`,
  `{{@p}}` string params, `prev.run.*`, `cast:`, `credits:` — is opt-in. What
  can change is `check-project`'s answer: the condition decider now proves
  same-path contradictions (`run.n > 5 && run.n < 3`, `x && !x`) dead, so a
  guard that was always false is now `E-BEAT-UNREACHABLE`,
  `E-ENTRY-UNREACHABLE`, `E-ARM-DEAD` or `E-OBJECTIVE-UNSATISFIABLE`, and a
  project that declares a cast checks its speakers.
- **`0.22.0` is breaking for compiled identity, not for grammar.** Every
  `0.21.1` document still parses and checks as before apart from the new
  diagnostics; what moves is the ids `lute compile` writes. The default
  `voiceKey` template gains `{prefix}` (the `0.21` default collided across
  documents), and a line expanded from a component gets its own
  `{prefix}.{component}#{n}.{speaker}_{code}` address. A project that recorded
  audio against the old keys pins `identity.voiceKey: "{speaker}-{code}"`;
  documents that `::use` components re-export their localization and voice
  manifests. The toolchain side changes answers too: trace and test now apply
  the project's Datalog rules by default. `<quest tier>` defaults to `user`, so
  no existing quest changes its persistence.
- **`0.21.1` is a patch on the `0.21` line: no new syntax, tighter static
  semantics.** Checks that accepted a defect and shipped the wrong thing now
  report it: `quest.<id>.state` reads as an always-assigned lifecycle enum;
  a def body passes the CEL profile gate; a def in a directive attribute folds
  to its literal or is `E-ATTR-DEF-DYNAMIC`; a `{{@def}}` that cannot be
  inlined is `E-INTERP-DEF`; `<quest>` / `<objective>` / `<on>` close their
  attributes (`E-UNKNOWN-ATTR`); a single-quoted attribute value is
  `E-ATTR-QUOTE`; an `is=` + `test=` arm covers only what both prove;
  `E-DUP-LINE-CODE` runs over expanded components; and `check-project` runs
  the compile, the single-snapshot gate (`E-CAPABILITY-MISMATCH`), and
  `E-DUP-VOICEKEY`. The spec text for these rules is still the proposal that
  introduced each construct; the release record is the
  [changelog](https://github.com/journeyWorker/lute/blob/main/CHANGELOG.md) and
  the [versioning guide](https://github.com/journeyWorker/lute/blob/main/docs/versioning.md).
- **`0.11.0` moves the IR's `major.minor` with nothing behind it — the first
  time this stack records that shape.** `0.10.1` stayed inside `0.10` and cost
  nothing; `0.10.2` also stayed inside `0.10` but moved real content
  (`meta.plugin`); `0.11.0` moves the *number itself* out of `0.10` into
  `0.11` while carrying **zero** content or shape change — the artifact
  `0.11.0` produces is byte-identical to `0.10.2`'s. An engine gated on IR
  `0.10` still has to widen its gate to `0.11` to keep accepting artifacts
  (the runtime contract gates on `major.minor`, not on whether anything
  inside actually changed), and `schemas/lute-ir-0.10.schema.json` is renamed
  to `schemas/lute-ir-0.25.schema.json` under the `0.7.0` precedent — a
  `major.minor` move renames the schema file regardless of why it moved. The
  release itself is entirely toolchain: a new `schedule.yaml` project-file
  layer and `lute play` command (`0.21.0` removed the layer again and rebuilt
  `lute play` on occasions — [Playing a story](/tooling/play/)), plus two fixes in the
  shared reference runner (a compiled `<when is=…>` match arm now reads its
  structured `expr` instead of always falling through to `<otherwise>`, and a
  hub whose scripted decisions run out with an eligible option remaining now
  halts incomplete instead of silently converging).
- **`0.6.0` is the one breaking *grammar* revision in the current stack.** It
  removed the `persist=` attribute so `into=` alone drives the choice run-record
  sugar, and made shot headings free text. Pre-`0.6.0` documents carrying a bare
  `into=` (previously a silent no-op) now record.
- **`0.8.0` — the adoption release — is backward compatible in the grammar, but
  it is not a pure addition.** Every item in it traces to a gap found assessing
  Lute against a real 777-scene / 583-quest game catalog, and two of those items
  have edges worth naming. The IR **gains a command kind**, `end`; an unknown
  `kind` is a hard error under the execution-model version policy, so a `0.7`
  engine MUST refuse an artifact carrying one — which is the intended signal,
  since termination is capability an older engine cannot fake. And
  `E-STATE-COLLECTION` **enforces a rule that was always normative but never
  checked**: a `state:` declaration typed `list`/`record`/`map` used to pass the
  shape validator and now fails. Everything else is optional and append-only —
  apart from that one declaration, a `0.7.0`-clean document needs nothing but a
  restamped `luteVersion:` to check clean under `0.8.0`.
- **`addr` widths are uniform per artifact, and that is now a guarantee.** The
  index segment used to be a fixed four digits, so a shot with 100 or more
  records emitted `001-11500` beside `001-1400` and string comparison reported
  `"001-11500" < "001-1400"` — lexicographic order silently diverged from
  execution order. Both segments are now padded to a width computed from the
  document and held uniform across the artifact, so *lexicographic order over
  `addr` equals execution order* is something an engine may rely on. A document
  whose every shot emits fewer than 100 addresses compiles byte-identically to
  `0.7.0`. `addr` is still a position regenerated on every compile, never an
  identity — the stable joins remain `lineId` / `voiceKey`.
- **`0.9.0` — vocabulary ownership — is breaking in validation, not in
  grammar.** The compiler declares the seven content-vocabulary *slots* and
  ships **no members**, so a document writing `emotion="delighted"` needs its
  project to declare `emotion`; using a slot nobody declared is
  `E-DOMAIN-UNKNOWN`, an **error**, where `action` in particular used to be
  skipped outright. Every pre-`0.9.0` `enums:` block still parses byte-for-byte
  — a bare sequence is shorthand for `{ members: [...] }` — but a declaration of
  `action` MUST now supply `exits:` and one of `anchor` MUST supply `default:`,
  because those are the two places the compiler branches on *which* member, and
  it no longer infers them from a name prefix. **The IR shape did not move in
  that release**: no field was added, renamed, or moved, and `irVersion` read
  `"0.9.0"` only because a release re-aligns every axis — an engine gated on IR
  `0.8` had only to widen its gate to `0.9`. The
  artifact's *content* does move — `enums` becomes populated for a project that
  declares inline or through `uses:`, and `capabilityVersion` shifts. The
  authoring side is written up at [Content vocabulary](/language/vocabulary/).
- **`0.9.0` also made an imported component body check like the content it
  is.** Five of `check()`'s eighteen diagnostic stages were root-only and never
  ran over a component body, so the same lines checked *clean* through a `::use`
  and *dirty* at scene level — content-line attribute rules, `E-DUP-LINE-CODE`,
  reachability, admission of content the walker does not process, and injection
  folding. All five run now, anchored at the `::use` site with a prefix naming
  the component and its file. A component body that used to pass may report, and
  every such report was already reaching the artifact or already being silently
  dropped. What a component body still does **not** get is its own vocabulary
  scope: its `uses:` and its own inline `enums:` are both discarded at parse, so
  the body resolves vocabulary against the **importing** document.
- **`0.10.0` — the toolchain says what it knows — reddens documents, mocks, and
  the IR shape.** Thirteen language changes, and the through-line is that every
  one of them is a place the checker already held the answer. Three of them can
  redden a document that checks clean today: `::set` now types its right-hand
  side against the path it writes (`E-SET-TYPE`), the six logic tags close their
  attribute sets (`E-UNKNOWN-ATTR`, and `E-AS-REMOVED` for `as=` on a
  `<choice>`), and a `<quest start>` gate that can never open is
  `E-QUEST-UNREACHABLE` where it used to be silent. `mocks/*.yaml` is validated
  for the first time and its `file:` key is **required** — a mock without one is
  `E-MOCK-SUBJECT`. And the IR **shape** moves for the first time since
  `0.8.0`: `provenance.reason` becomes `provenance.explanation`, so an engine
  gated on IR `0.9` must widen to `0.10` and rename the one field it reads.
  `W-INJECT-CONFLICT` is **removed** rather than narrowed, and the information
  it carried is dropped, not migrated — agreement with the declared default was
  its only trigger, so there was nothing left to warn about.
- **The `0.6.1` coverage warnings are honesty, not errors.** They name the exact
  edge of what static analysis can prove — a stale `luteVersion` stamp, an
  unproducible trace mock — and never flip the exit code on their own. The third,
  `W-UNPROVEN-RELATIONAL` (a relational fact query the checker could neither prove
  nor refute), is gone since `0.20.0`: `check-project` now decides relational
  guards, reporting the dead ones through their slots' errors and the redundant
  ones as `W-FACT-GUARANTEED`, and a merely *possible* guard is the normal case
  and silent. The two `0.8.0` additions are ordinary findings rather than
  coverage claims — unreachable content after an `::end`, and a translatable
  record missing a locale the bundle declares — but they carry the same
  severity. Promote any of them to an error with `--deny <CODE>` or
  `--deny-warnings`.
- **Design rationale lives alongside the specs.** The four-tier state model's
  *why* is recorded in
  [`state-model-design.md`](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/state-model-design.md),
  a non-normative companion to `0.0.1` §9.
- **Capability surfaces are specified separately.** Character/cast identity and
  the plugin system are capability proposals, not core scenario-DSL revisions.
  The plugin system's current revision is `0.0.4`, landing alongside `0.10.2`:
  a plugin-owned, checker-validated frontmatter key now reaches the compiled
  artifact (`meta.plugin`) instead of being discarded at compile time. `0.0.3`
  — `lute.core` exports an empty `enums`, an `enums` entry may carry the long
  form's member semantics, and the closed `semantics` flag vocabulary drops
  the two flags no consumer read — and `0.0.2` — option and frontmatter value
  validation, reserved stamp-attribute rejection, cross-cutting `stampAttrs`,
  and the declarative `lower: { record, fields }` form made real — remain its
  base as amended. See the [specification index](/spec/) for all four.
