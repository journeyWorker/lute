# Lute runtime conformance fixtures

These fixtures are the **executable acceptance suite for the runtime contract**
(`docs/runtime/*.md` + `schemas/lute-ir-0.38.schema.json`). A third-party engine
that consumes compiled Lute artifacts should **replay every fixture** and check
that its own machine transcript matches the checked-in `expected.json`. They are
small by design — each isolates one contract surface — so a mismatch points
straight at the clause an implementation got wrong.

`lute run` (`crates/lute-cli/src/runner.rs`) is the **reference engine**: it is
the implementation these fixtures were generated from, and the yardstick a new
engine is measured against.

## Fixture layout

Each fixture directory contains:

| file | role |
|---|---|
| `source.lute` | the authored document (for provenance — an engine never reads it) |
| `schema.yaml` | the `uses:`-imported state / relational schema (present only when the source needs one) |
| `artifact.json` | the **compiled artifact** — `lute compile source.lute -o artifact.json`, checked in verbatim; this is the engine's only input |
| `mock.yaml` | the mock playthrough (the same `state:`/`facts:`/`choose:`/`events:`/`accepts:` surfaces `lute trace --mock` reads) |
| `expected.json` | the exact `--json` machine transcript the runtime contract requires |
| `entry.txt` | lore fixtures only (dsl 0.19.0): the one entry id the run presents, passed as `--entry <id>` — a lore artifact has no sequence to play, so `lute run` refuses it without one. The Rust harness (`crates/lute-cli/tests/conformance.rs`) does the same: when a fixture directory holds `entry.txt`, it appends `--entry` and the file's trimmed contents to the `lute run` arguments |
| `beat.txt` | bundle-beat fixtures only (dsl 0.23.0 §4): the one beat id the run presents, passed as `--beat <id>`; the Rust harness appends it the same way |
| `loc/<locale>.json`, `locales.json` | translated fixtures only: authored `lute loc export`-shaped translations, and the bundle `lute loc import` builds from them, merged by `lute compile --locales locales.json` |

## Replaying

From the repository root:

```sh
for d in conformance/*/; do
  [ -f "$d/artifact.json" ] || continue
  entry=(); [ -f "$d/entry.txt" ] && entry=(--entry "$(cat "$d/entry.txt")")
  [ -f "$d/beat.txt" ] && entry=(--beat "$(cat "$d/beat.txt")")
  got=$(cargo run -q -p lute-cli -- run "$d/artifact.json" --mock "$d/mock.yaml" "${entry[@]}" --json)
  diff <(printf '%s\n' "$got") "$d/expected.json" && echo "PASS  $d" || echo "FAIL  $d"
done
```

An engine passes a fixture when its transcript is **byte-identical** to
`expected.json`. The `--json` object has stably-sorted keys, so the comparison is
a plain `diff`.

### The `--json` transcript shape

```json
{
  "kind":       "scene" | "quest" | "lore",
  "irVersion":  "0.22",                // the major.minor line the engine gated on
  "exit":       "complete" | "incomplete",
  "commands":   [ /* executed records, in execution order */ ],
  "state":      { "<path>": <value>, ... },   // final scalar state, key-sorted
  "facts":      [ "rel(a, b)", ... ],         // final fact store (base ∪ derived), sorted
  "quests":     { "<id>": "active"|"complete"|"failed"|"unset", ... }
}
```

### The `grant` transcript event (dsl 0.16.0 §3 D-D)

A `<reward/>` (`RewardEntry` on `QuestCmd.rewards`/`ObjectiveEntry.rewards`)
surfaces one `grant` command per fresh lifecycle transition it fires on. An
objective grant fires at first `done` (once, monotone); a quest grant fires
at fresh `complete` or `failed` (§2.3 cascade included, once per instance).
Order within an owner is declaration order; objective grants precede quest
grants at the SAME settling pass, and both precede the fresh transition's
`questComplete`/`questFailed` handler bodies.

```json
{
  "kind":      "grant",
  "quest":     "<questId>",
  "objective": "<objectiveId>",     // only on an objective-level grant
  "reward": {
    "kind":      "<rewardKind>",
    "target":    "<targetId>",       // present only when authored
    "amount":    5,                  // XOR the range pair
    "amountMin": 1, "amountMax": 5   // XOR the scalar
  },
  "onFailed":  true                  // only when the transition was `failed`
}
```

Range bounds are carried **verbatim** on the wire (spec D-C) — a reference
engine NEVER pre-rolls a value into `amount`; the engine's own dice draw
over `[amountMin, amountMax]` at grant time is engine policy the IR does not
fix. `reward.when` is evaluated at the grant instant and is NEVER surfaced
in the transcript: a grant that fires is unconditionally true; a `false`/
`unknown` gate silently skips its entry.

### Exit codes

| code | meaning |
|---|---|
| `0` | a complete walk |
| `2` | I/O or usage failure: unreadable/malformed artifact or mock, an artifact carrying a removed 0.36 field or a command without `position` (`E-IR-REMOVED-FIELD`), an `irVersion` outside the engine's major.minor line, or an unknown command `kind` |
| `3` | an incomplete walk — a `choice`/`hub` was reached with no mock decision (mirrors `lute trace`'s §4.5 incomplete convention) |

## The fixtures

| fixture | contract surface exercised |
|---|---|
| `choice-basic` | `choice` control flow — the mock forces one branch option; the chosen id lands in the `selectionKey` slot and the option body runs to the converge |
| `match-otherwise` | `match` precedence — the seeded subject matches no arm, so the `otherwise` target is taken (execution-model.md `arm ?? otherwise ?? converge`) |
| `match-range` | `match` over a numeric subject with range arms (dsl 0.18.0) — `..0`, `1..3`, `4..` lower to `>=`/`<=`/`&&` comparisons in `expr`; the seeded score `3` sits on the inclusive upper bound of `1..3`, so arm 2 is taken |
| `hub-once-exit` | `hub` re-presentation — the mock forces `[probe, probe, leave]`; `probe` is `once` (the second force is refused) and `leave` is the `exit` option that leaves the hub |
| `facts-datalog-rule` | the **Datalog least-fixpoint** — an `assert` delta plus a seeded fact drive the derived relation `suspected` (cel-and-facts.md); a `holds(...)` guard over the derived relation returns a definite answer |
| `quest-complete` | the **quest lifecycle** — `start=true` activation, monotone objective completion, derived quest completion, and the `questComplete` `<on>` handler body (quest-lifecycle.md) |
| `quest-subquest` | the **subquest fail cascade** — the seeded `run.giveUp` fires `parentQ`'s authored `fail`, and its `quest=`-linked child `findKey` fails with it; each failing `quest` record carries `failedBy` (`fail` for the parent, `cascade` for the child, dsl 0.24.0 §2) and the final state holds the read-only `quest.<id>.failedBy` paths beside each `outcome="failed"` grant |
| `end-reason` | the **`::end` walk terminator** (dsl 0.8.0) — the forced arm's `end` record stops the walk with its `reason` surfaced; the shared converge one record later is never reached, and the run is still `complete` |
| `lore-entry` | a **lore entry, first read** (dsl 0.19.0, lore-entries.md) — `entry.txt` names `scientistLog1`, so the run presents that one `entry` record: the `entry` event carries `firstRead: true` and `eligible`, the body segment runs to the next `entry` record (its `match` picks arm 1 from the seeded `run.labBurned: true`), the first-read `assert`/`set` apply, and the engine then sets `entry.scientistLog1.read` — the sibling `scientistLog2` is never presented and its `read` path stays `false` |
| `lore-entry-reread` | the same entry **re-read** — the mock seeds `entry.scientistLog1.read: true`, so the text presents (the `otherwise` arm) and the `assert`/`set` records are recorded as `skipped` events (`effect` + the record's `path`/`fact`/`pattern`) without changing state or facts |
| `command-staging` | staging/timeline command records: `bg` (`::bg`), `music` (`playback`), `sfx`, `vfx` (`type`), `actor` (`::actor`), `camera` (`focus`/`framing`/`move`/`transition`), `cg` (`display`), `video` (`display`), plus the timeline `barrier`; all are compiled and replayed in one linear scene |
| `command-lifecycle` | lore beat presentation (`beat`), quest acceptance (`accept`), and fact retraction (`retract`) in a beat selected by `beat.txt` |
| `command-plugin` | a local capability plugin's mock bridge (`plugin`) with an unanswered bridge result; the runner records the external call without host invocation |
| `cel-numbers` | typed `int`/`double` CEL literals and arithmetic, including truncating division, modulo, conversions, and numeric rendering |
| `cel-facts` | list-form `holds`, `count`, `countDistinct`, `validAt`, `now`, and `visited`, including wildcards and `occasion.target` |
| `cel-presence` | `has()` and quoted-key `in` presence checks plus indexed state reads |
| `cel-errors` | condition-error-as-not-satisfied and `::set` evaluation errors that halt without a partial write |
| `grant-instance` | a single-walk `lute run` fixture pinning `instance` and `index` field presence on objective and quest rewards; multi-run identity is pinned by `crates/lute-cli/tests/grant_identity.rs` |
| `grant-replay` | a single-walk `lute run` fixture pinning stable grant identity fields and objective/index coordinates; replay and multi-run identity are pinned by `crates/lute-cli/tests/grant_identity.rs` |
| `identity/explicit-component` | explicit `instance="use-001"`/`use-002` component scopes; nested component joins remain stable under insertion |
| `identity/insertion-before-repeat` | inserting before a repeated component preserves authored instance identities |
| `identity/rename-ledger` | a project manifest's quest rename expands to objective/state pairs in `identityRenames`; `requiredSemantics` includes `lute.identity.renames/1` |
| `identity/save-collision` | the real rename ledger is paired with `save-before.json` and `migration-report.json`; engines MUST reproduce the report's refusal/coalescing outcomes before mutating saves |
| `section-ids` | dsl 0.37.0 §3.1 sections: `## Heading {#id}` lands in `sections[]` as `{section, heading, id}`; the middle section has no id and keeps only its positional fallback; each command `position` starts with its one-based section number |
| `staging-sequence-camera` | every 0.37 staging kind with `family: "staging"`: `bg`, `music` (`playback`), `sfx`, `vfx` (`type`), `sequence` (`wait` defaults to `true`; `wait="false"` emits `timing.wait: false`), authored `actor` with `emotion`/`costume`, an `actor` emotion update with no line, the injected anchor and pose-reset `actor` records (`provenance`), `camera` domain members `focus`/`framing`/`move`/`transition`, `cg` with its resolved default `display: "show"` and a `layout`, `video` with its resolved default `display`, an actor exit from `action.exits`, and a `<timeline>` whose clips carry nested `timing` (`at`, zero-based `timeline` ordinal) joined by a `barrier` with direct `timeline`/`at` |
| `line-roles-voicekey` | every line `role` — `narration`, `dialogue`, `mono` (the POV speaker and a `monoSpeakers` speaker), `os`, `vo` — and a `voiceKey` on every line, narration and mono included |
| `inline-modifiers` | dsl 0.37.0 §3.6 inline modifiers: `pause`, `speed`, nested `textStyle` spans, escaped punctuation, and `{{…}}` interpolation inside a span; `text` is the plain derivation and `segments` the coalesced runs (innermost rate wins). `loc/ja-JP.json` is imported to `locales.json` and merged with `lute compile --locales`, so each line carries `texts["ja-JP"]` and modified lines `localeSegments["ja-JP"]` |
| `jump-label` | `::label{name}` emits no record; a guarded forward `::jump{to}` is taken from the seeded `run.hasKey`; a branch option carries `text`; `<match subject=…>` reads the seeded subject |
| `reward-id` | `<reward id=…>` reaches the IR `rewards[].id` on quest and objective rewards beside an untagged reward; the grants fire in declaration order |
| `invalid/owned-write` | a hand-built artifact whose `set` writes an engine-owned path; `lute run` refuses it with `E-RUN-OWNED-WRITE`, exit 1 |
| `invalid/removed-field-addr`, `invalid/removed-field-timing`, `invalid/removed-field-envelope`, `invalid/removed-field-injected`, `invalid/removed-field-kind` | the compiled source with one removed 0.36 field put back (`addr`, flattened `wait`/`duration`, envelope `capabilityVersion`, `provenance.injected`, a per-kind `cg.action`) by `invalid/derive.py`; `lute run` refuses each with `E-IR-REMOVED-FIELD`, exit 2 |
| `invalid/removed-kind-sprite` | the compiled source with the `actor` kind renamed to the removed `sprite`; `lute run` refuses the unknown kind, exit 2 |

The component-key uniqueness rule is intentionally strict: duplicate keys in
mutually exclusive branches still conflict when they share one immediate
owner (scenario DSL 0.36.0 §2.1). Checker tests cover that diagnostic; it is
not a runtime conformance fixture.

The save format is engine-owned. `save-collision/migration-report.json` is the
engine contract data: an engine MUST apply the artifact's `identityRenames`
atomically, refuse unequal destination values while leaving the save unchanged,
and MAY coalesce equal values.

The command inventory is intentionally explicit: every currently supported runtime
command kind is covered by at least one fixture above or by the pre-existing
fixtures. `command-staging` covers `bg`, `music`, `sfx`, `vfx`, `actor`,
`camera`, `cg`, `video`, and `barrier`; `staging-sequence-camera` adds
`sequence`; `jump-label` covers `jump`; `command-lifecycle` covers `accept`,
`beat`, and `retract`; `command-plugin` covers `plugin`. Every recorded command
carries its `family` (`content`, `staging`, `state`, `control`, `declaration`,
`plugin`) and `position`. No requested kind is unsupported by the reference
runner. The staging fixture uses a live `luteVersion: "0.38.0"` header, as do
the lifecycle and plugin sources.

### Diagnostic fixtures

`diagnostics/<name>/` pins the checker diagnostics of a deliberately invalid
source (dsl 0.37.0 §8): `expected.json` is the CLI's `--json` diagnostics
output, run from the fixture directory, with the directory's absolute prefix
removed. A fixture with a `lute.project.yaml` runs `lute check-project .
--json`; one with `loc/` translations runs `lute compile source.lute --locales
locales.json --json`; any other runs `lute check source.lute --json`. Each exits
1. `crates/lute-cli/tests/conformance.rs` replays them byte for byte. These
sources are not formatter input (several do not parse).

| fixture | diagnostics |
|---|---|
| `diagnostics/section-dup` | `E-SECTION-DUP` — a second section reuses `{#dock}` |
| `diagnostics/inert-title` | `E-INERT-TITLE` — a body `# ` heading |
| `diagnostics/removed-scene-tag` | `E-REMOVED-TAG` — the removed `<scene>` tag |
| `diagnostics/removed-project-key` | `E-REMOVED-PROJECT-KEY` — project `sequence:` |
| `diagnostics/old-surface` | `E-RENAMED-DIRECTIVE` (`::auto`, `::cut`, `::mark`, `::next`), `E-RENAMED-ATTR` (music/video `action`), `E-RENAMED-TAG-ATTR` (choice `label`, match `on`), `E-REMOVED-ATTR` (music `track`, sfx `name`), `E-CG-LAYOUT` (`full`), `E-CAMERA-REMOVED` (`zoom`), `E-CAMERA-EMPTY` |
| `diagnostics/mono-pov` | `E-MONO-POV` — a `mono` speaker that is neither the POV nor in `monoSpeakers` |
| `diagnostics/mono-no-pov` | `E-MONO-NO-POV` — a `mono` line with no resolved POV |
| `diagnostics/mono-component-caller` | `E-MONO-POV` judged at the `::use` site with the caller's POV, naming the component line and carrying it as `related` |
| `diagnostics/text-modifier` | `E-TEXT-MODIFIER` — an unterminated span, a non-positive `speed` rate, attrs on a text style |
| `diagnostics/text-escape` | `E-TEXT-ESCAPE` — an unknown escape inside a span |
| `diagnostics/l10n-modifiers` | `E-L10N-MODIFIERS` — a translation whose modifier multiset differs from the source |
| `diagnostics/dup-voicekey` | `E-DUP-VOICEKEY` across two documents under a `{speaker}-{code}` template; the pair differing only in modifier markup is not a collision |
| `diagnostics/reward-dup` | `E-REWARD-DUP` — a quest and its objective reuse a reward id |

## Boundaries — what the reference runner deliberately does NOT implement

These are host/engine policy the runtime contract leaves unspecified. The
reference runner records them honestly rather than faking them; a conforming
engine MAY implement them, but no fixture asserts their behavior:

- **No real timeline clock.** `<timeline>` clips are already flattened and
  pre-scheduled by the compiler (timeline-semantics.md). The runner replays the
  stamped records in stream order and treats a `barrier` as a transcript note —
  it honors no `at`/`duration`/`delay` wall-clock timing and models no track
  concurrency or frame pacing.
- **No real bridges.** A `plugin` command (bridge-protocol.md) is recorded as an
  external call; its `op`/literal effects ARE applied, but a `bridgeResult`
  effect has no mock surface to read from and is recorded unresolved. The runner
  invokes no host service and ignores the `wait` attr.
- **No narrative-time history.** `now()` / `validAt(...)` have no mock surface
  and read unknown; the fact store is valid-now (`holds`/`count` over the
  current least-fixpoint).

## Regenerating

Every generated file is re-recorded by one script, which runs the CLI commands
below over the whole corpus (replay, invalid, and diagnostic fixtures):

```sh
cargo build -p lute-cli && conformance/regenerate.sh
```

For one replay fixture the commands are:

```sh
d=conformance/choice-basic
cargo run -q -p lute-cli -- compile "$d/source.lute" -o "$d/artifact.json"
cargo run -q -p lute-cli -- run "$d/artifact.json" --mock "$d/mock.yaml" --json > "$d/expected.json"
```

(a lore fixture adds `--entry "$(cat "$d/entry.txt")"` and a bundle-beat
fixture `--beat "$(cat "$d/beat.txt")"` to the `run` line; a fixture with
`loc/*.json` translations first runs `lute loc import "$d"/loc/*.json -o
"$d/locales.json"` and adds `--locales "$d/locales.json"` to the `compile`
line). `invalid/stale-minor` and `invalid/owned-write` are hand-built
artifacts the compiler cannot produce and are not regenerated.

`artifact.json` is checked in (not regenerated on demand) precisely so a
third-party engine can conform against a **frozen** compiler output — an engine
must not need the Lute compiler to run the suite.

## 0.33 semantic fixture inventory

Record (or re-record) the semantic-registry fixtures with:

```sh
for d in conformance/{staging-all-directives,staging-video-blocking,timeline-stamps-barrier,quest-handler-snapshot,clock-advance-settle,cadence-share-spentBy,season-opening-reset,rearm-instance-preserved,occasion-priority-targets,occasion-also-for-kind,beat-after-index-order,occasion-raisedWhen-terminal,occasion-outside-run,knowledge-keyed-retract,knowledge-negation-aggregate-recursion,knowledge-validity-half-open,knowledge-validAt-now,lore-first-read-disclosure,lore-reread-state,lore-bundle-beat}; do
  cargo run -q -p lute-cli -- compile "$d/source.lute" -o "$d/artifact.json"
  cargo run -q -p lute-cli -- run "$d/artifact.json" --mock "$d/mock.yaml" --json > "$d/expected.json"
done
```

| fixture | collected semantic ids (compiled source) |
|---|---|
| staging-all-directives; staging-video-blocking | `lute.core/1`, `lute.staging/1` |
| timeline-stamps-barrier | `lute.core/1`, `lute.staging/1`, `lute.timeline/1` |
| quest-handler-snapshot | `lute.core/1`, `lute.quest.lifecycle/1`, `lute.quest.rewards/1` |
| clock-advance-settle | `lute.core/1`, `lute.lore/1`, `lute.occasions.selection/1`, `lute.time.cadence/1`, `lute.time.clock/1` |
| cadence-share-spentBy | `lute.core/1`, `lute.knowledge.facts/1`, `lute.lore/1`, `lute.occasions.selection/1`, `lute.time.cadence/1` |
| season-opening-reset; rearm-instance-preserved | `lute.core/1`, `lute.quest.lifecycle/1`, `lute.time.seasons/1` |
| occasion-priority-targets; occasion-also-for-kind; beat-after-index-order | `lute.core/1`, `lute.knowledge.facts/1`, `lute.lore/1`, `lute.occasions.selection/1`, `lute.quest.lifecycle/1`, `lute.time.cadence/1` |
| occasion-raisedWhen-terminal; occasion-outside-run | `lute.core/1`, `lute.knowledge.facts/1`, `lute.lore/1`, `lute.occasions.gates/1`, `lute.occasions.selection/1`, `lute.quest.lifecycle/1`, `lute.time.cadence/1` |
| knowledge-keyed-retract | `lute.core/1`, `lute.knowledge.facts/1` |
| knowledge-negation-aggregate-recursion | `lute.core/1`, `lute.knowledge.facts/1`, `lute.knowledge.rules/1` |
| knowledge-validity-half-open; knowledge-validAt-now | `lute.core/1`, `lute.knowledge.facts/1`, `lute.knowledge.temporal/1` |
| lore-first-read-disclosure; lore-reread-state; lore-bundle-beat | `lute.core/1`, `lute.knowledge.facts/1`, `lute.lore/1` |

Invalid engine fixtures use `engine.yaml`, an artifact, and `expected-stderr`; they must exit 2 with `E-ENGINE-SEMANTICS`, `E-ENGINE-IR-VERSION`, and `E-ENGINE-MATRIX` respectively.

## Edit-task suite

The 0.38.0 AI edit-loop conformance suite is in
[`edit-tasks/`](edit-tasks/). It contains 12 inspect/plan/patch/check cases;
`REPORT.json` records the expected outcomes.

Each numbered task directory contains `task.json`, which names the canonical
game directory used as its base:

```json
{"base":"docs/examples/games/summer-station"}
```

The base is never copied into the task fixture. The runner builds one model
for each distinct base game, then applies the reference patch and every
`trap-*.json` as a dry-run against that real directory. Dry-runs stage all
edits and rebuild the staged model, but never write the game tree. The
12 task tests run independently and may execute in parallel. The standalone
job-restriction fixture is at
`edit-tasks/_games/job-restriction/` and is selected by its `task.json`.
