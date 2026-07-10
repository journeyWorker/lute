# Lute compile IR — 0.2.0 addendum: kind envelope + quest/objective/on records

- **Date:** 2026-07-09
- **Status:** design addendum to `docs/superpowers/specs/2026-07-04-lute-compile-json-ir-design.md` (the 0.1.0 JSON-IR design). Drives Plan D (`docs/superpowers/plans/…lute-compile-0.2.0-quest…`).
- **Scope:** the compiler artifact for the 0.2.0 additions — the `kind` envelope discriminator, the kind-polymorphic envelope `meta`, and the three quest-kind record types (`quest`, `objective`, `on`). The scene artifact is UNCHANGED except for the new leading `kind` field. Everything else (§4 record set, §5 addressing, §5.6 addr scheme, CEL `ExprNode`, `{{…}}` placeholders) is reused verbatim.
- **Reduces-to-data invariant (§3):** a quest document still compiles to a finite, ordered, flat array of command records + CEL `ExprNode`s. `<on>`/`<objective>` are forward-only; no backward jumps.

## 1. Envelope

`Artifact` gains a **leading** `kind` field (a `DocKind` enum serialized lowercase, mirroring `Role`), and `meta` becomes kind-polymorphic:

```json
{
  "kind": "quest",
  "lute": "0.2.0",
  "irVersion": "0.2.0",
  "capabilityVersion": "…",
  "meta": { … },
  "state": [ … ],
  "commands": [ … ]
}
```

- `kind` — `"scene"` | `"quest"`, from the resolved `DocKind` (lute-check). FIRST field (most fundamental discriminator). Adding it re-records every scene golden (expected one-time migration).
- `lute` / `irVersion` both bump to **`"0.2.0"`** (`LUTE_LANG_VERSION` / `LUTE_IR_VERSION`).
- `meta` — an **untagged** `ArtifactMeta` enum: `Scene(SceneMeta)` = the existing `{character, season, episode, episodeId, title?}` (BYTE-IDENTICAL to 0.1.0), `Quest(QuestMeta)` = `{title?, contentLang?}` (both `skip_serializing_if=None`; MAY be `{}`). The consumer reads `kind` to know which shape `meta` is. `serde(untagged)` picks the shape on serialize; scene bytes are unchanged.
- `state` — the folded state table (§4.1 of the 0.1.0 design), now including the checker-folded quest reserved decls: `quest.<id>.state` (`{path, type:"enum", domain:["active","complete","failed","unset"], provenance:"quest:<id>"}`, no default) and, per objective, `quest.<id>.objectives.<oid>.done` (`{path, type:"bool", default:false, provenance:"quest:<id>"}`). Ordinary declared `quest.<id>.*` scratch fields appear as their own entries. BTreeMap order (sorted by path), as today.

## 2. `state` provenance for quest reserved paths

Mirror the scene `branch:<id>` provenance convention (0.1.0 §4.1): reserved quest paths carry `provenance: "quest:<id>"`. The `unset` domain member is appended to `quest.<id>.state` exactly as `scene.choices.*` appends `unset` (§11.1) — it is engine-populated and maybe-unset before the quest is known.

## 3. Records

A quest document's `commands` array is produced per `<quest>` declaration (the addressing unit; §5). Each quest contributes, in document order:

> **Direct quest-body content (normative).** A `<quest>` body admits, besides `<objective>`/`<on>`, bare `content-line`/`::directive`/`::set`/`<branch>`/`<match>` directly (dsl 0.2.0 §6.3, §6.7 — the checker admits them and folds their `<branch>` decls). The compiler LOWERS these as ordinary records (`line`/`set`/plugin/`choice`/`match`, the existing 0.1.0 record types) into the same per-quest addressing unit, in document order alongside the `quest` head record and the `on` records — NEVER dropped. The engine assigns their execution meaning (Lute emits a faithful record stream; §8 of the DSL — execution is the engine's). Their `lineId` uses the `{questId}` prefix (§4).

### 3.1 `quest` record (declaration head)

```json
{
  "kind": "quest",
  "addr": "001-0100",
  "id": "rescueHalsin",
  "title": "Rescue the First Druid",
  "titleLineId": "rescueHalsin.title",
  "start": { "raw": "run.act == 1", "expr": { … ExprNode … } },
  "fail":  { "raw": "run.npc.halsin.dead", "expr": { … } },
  "objectives": [
    {
      "id": "reachGrove",
      "title": "Reach the Emerald Grove",
      "titleLineId": "rescueHalsin.reachGrove",
      "done": { "raw": "run.region == 'grove'", "expr": { … } },
      "when": null,
      "optional": false,
      "body": null
    }
  ]
}
```

- `start`/`fail` — optional; each a `{raw, expr}` pair (the `expr` lowered via `crate::expr::lower_expr`, `@ref`/`$` already expanded; `null`/omitted when the attr is absent). `skip_serializing_if=None` for absent.
- `title` / `titleLineId` — present only when authored; `titleLineId` = `{questId}.title` (§4, identity).
- `objectives[]` — the objective TABLE inlined in the quest record (analogous to `HubCmd.options`): declaration data the engine needs to derive the lifecycle (§6.3 of the DSL). Fields: `id`, `title?`/`titleLineId?` (`{questId}.{objectiveId}`), `done` (`{raw, expr}`), `when?` (`{raw, expr}` or null), `optional` (bool, always present), `body` (a target addr into the objective's completion-body records, or `null` when the body is empty). Completion is derived by the engine (all non-`optional` objectives `done` ⇒ `complete`; `fail` wins on tie) — the compiler does NOT emit control flow for it.
- The `quest` record is a declaration head, like `HubCmd`: it carries no executable body itself; the executable pieces (objective completion bodies, `<on>` arms) follow as their own addressed records, referenced by `body`/target.

### 3.2 objective completion body

An objective with a non-empty body: its `Node*` body lowers to ordinary records (line/set/directive/match/branch — the existing lowering), emitted as an addressed segment; the objective entry's `body` field targets the segment's entry addr. Because completion is monotonic (§6.3), the engine plays the segment once when `done` first holds. The segment is forward-only (ends by falling through / a forward converge — NO backward jump). An empty-body objective has `body: null` and emits no segment.

### 3.3 `on` record

```json
{
  "kind": "on",
  "addr": "001-0400",
  "event": "questComplete",
  "when": { "raw": "run.npc.halsin.dead", "expr": { … } },
  "body": "001-0500"
}
```

- `event` — the resolved event name (String; a built-in lifecycle or capability world event — the checker proved it declared).
- `when` — optional `{raw, expr}` guard (or `null`).
- `body` — target addr of the on-arm's first emitted record (the arm's `Node*` lowered as ordinary records). An empty arm targets its own one-past-end converge.
- Firing is engine policy (all-match, pre-event snapshot, document order — DSL §4.2); the compiler emits only the declaration + the forward body segment.

## 4. Addressing & identity (§5, §5.6)

- **Addressing unit = the `<quest>` declaration**, indexed 1-based in document order — reuses `ShotRecords { shot: i64, … }` with `shot` = the quest index, and the unchanged `"{unit:03}-{idx:04}"` +100-gapped scheme. `assign_addresses` is UNCHANGED (it is agnostic to what a "unit" means). Labels/converge targets stay per-unit symbolic `@n`, resolved as today.
- **Identity (`lineId`) is PER-QUEST** (DSL §7, D7): the prefix is `{questId}`, not `{character}.{episodeId}`. A content line in an `<on>`/`<objective>` arm → `{questId}.{speaker}_{code}`; objective title → `{questId}.{objectiveId}`; quest title → `{questId}.title`. This requires the identity pass to be PER-DECLARATION-scoped for quest (each quest is its own identity scope: code back-fill counters reset per quest), whereas scene is ONE document-wide scope (all shots, prefix `{character}.{episodeId}`). Plan D generalizes `assign_identity`/`IdCx` to take a per-unit prefix + per-scope code counters; scene passes one scope, quest passes one per quest.
- `voiceKey` on voiced lines in quest arms follows the same per-quest `{speaker}-{code}` scheme.

## 5. `Command` enum + impl seams

New `Command` variants `Quest(QuestCmd)`, `Objective(ObjectiveCmd)` (only if objective bodies are modeled as standalone records rather than inline `body` targets — SEE NOTE), `On(OnCmd)`. Each new `*Cmd` struct: first field `pub addr: String`, `#[serde(flatten)] pub stamp: Stamp` where stamped. The three `impl Command` match sites (`addr_mut` exhaustive, `for_each_target` has `_` wildcard, `stamp_mut` exhaustive) gain arms: `addr_mut`/`stamp_mut` REQUIRE arms (compiler-forced); `for_each_target` needs explicit arms for `quest`/`on` because they carry symbolic `body`/objective-`body` targets that MUST be rewritten to concrete addrs (do NOT let them fall through the `_` wildcard).

> NOTE (objective modeling choice, Plan D Task decides + tests): objectives are INLINED in the `quest` record's `objectives[]` array (§3.1) with a `body` target — there is NO standalone `objective` record. This keeps the objective TABLE in one place for the engine (mirroring `HubCmd.options`) and avoids a third record type. The `on` handlers ARE standalone `on` records (they are independent event rules, not part of the quest's declaration table). So Plan D adds `Command::{Quest, On}` (2 variants), not 3. `ObjectiveEntry` is a plain serde struct inside `QuestCmd`, not a `Command`.

## 6. Compile flow (§5 pipeline)

`compile()` gates on the clean check (D6, unchanged), re-parses/fills/folds, then dispatches on `folded.doc_kind`:
- **scene** — the existing shot loop + `assign_addresses` + `SceneMeta` envelope (byte-identical to 0.1.0 aside from the new `kind:"scene"`).
- **quest** — for each `<quest>` (unit index `i`): a fresh `Emitter`; emit the `quest` declaration record (with inlined objective table + symbolic `body` targets via `em.fresh()`); then walk each objective's completion body and each `<on>` arm via the existing `stage::walk_seq` (binding their entry labels); collect into `ShotRecords { shot: i, … }`. Then `assign_addresses` with per-quest identity prefixes. Envelope `kind:"quest"`, `QuestMeta`.

The `<on>`/`<objective>` no-op arms in `stage::walk_seq` (Plan A transitional) are REPLACED by real `walk_on`/objective-body handling driven from the quest loop (or by matching `Node::On`/`Node::Objective` inside a quest-body `walk_seq` — Plan D decides; since quests are top-level and their bodies are walked explicitly, driving from the quest loop is cleaner and keeps `walk_seq`'s On/Objective arms unreachable/`unreachable!()`-guarded for scene bodies).

## 7. Non-goals (0.2.0 IR)

- No relational/fact envelope sections (0.3.0).
- No engine-side execution semantics — the artifact is declarative data; firing/derivation/reset are the engine's (DSL §4.2, §5.1, §8).
