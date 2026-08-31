# Subquest support — `<objective quest=>` design

Date: 2026-08-31
Status: approved design, pre-implementation

## Problem

Lute quests form implicit hierarchies today only by hand: a parent objective
reads `quest.<child>.state == 'complete'`, a child gates itself with
`after="active('parent')"`, and failure propagation is copy-pasted CEL. Nothing
names the parent–child relationship, so the checker cannot catch orphaned
children, engines cannot derive a journal tree, and the soft-lock where a
failed required child leaves its parent forever `active` is silent.

Games model this constantly (BG3 "Save the Grove" → "Free Halsin"; eevee's
catalog chains quests via `CLEAR_QUEST_ID`/`ACCEPT_QUEST_ID` rows and carries a
never-used `groupId`). Lute should own the vocabulary.

## Decision summary

| Question | Decision |
| --- | --- |
| Completion semantics | Child = big objective: a required child quest's completion is part of the parent's derived completion. |
| Syntax | `<objective id quest="childId"/>` — the parent's objective references the child. No nesting, no `parent=`, no new tag. |
| Upward failure | Derived: a required child reaching `failed` fails the parent. |
| Downward failure | Derived: a parent reaching a terminal state (`failed` or `complete`) fails still-`active` children. |
| Lifecycle enum | Unchanged (`unset` → `active` → `complete` \| `failed`). No `abandoned` state. |
| Namespace | Unchanged. Child ids stay flat, project-unique; the tree is derived from references, not from id structure. |
| Shape | Tree, not DAG: at most one parent per quest; cycles rejected. Depth unbounded. |

## 1. Syntax

```lute
<quest id="saveTheGrove" title="Save the Grove" start="run.act == 1">
  <objective id="halsin" title="Find Halsin" quest="findHalsin"/>
  <objective id="ritual" title="Stop the ritual" quest="stopRitual"/>
  <objective id="scout" quest="scoutPerimeter" optional/>
  <objective id="talkRath" done="run.spokeRath"/>
</quest>

<quest id="findHalsin" title="Find Halsin">
  <objective id="reachCage" done="run.region == 'cage'"/>
</quest>
```

- `quest=` and `done=` are **mutually exclusive** on one `<objective>`
  (`E-OBJECTIVE-QUEST-DONE`). Exactly one of the two is required — an
  objective with neither keeps today's missing-`done` diagnostic.
- `when=`, `optional`, `title=`, and a body remain admitted alongside
  `quest=`. The body still plays exactly once, when the objective first
  becomes `done` — i.e. when the child completes — which is the natural slot
  for a "child resolved" journal line. `when=` still gates visibility only.
- The child is an ordinary `<quest>`, same file or another file. The one-line
  tag rule, id rules, and quest-body grammar are untouched. Regular
  (`done=`) and subquest (`quest=`) objectives mix freely.
- Multi-level trees fall out naturally: a child may itself carry `quest=`
  objectives. The structural checks in §4 (single parent, no cycles) are the
  only boundary.

## 2. Semantics

Design rule: synthesize into the existing engine contract wherever the
compiling document has the information; add an engine-derived rule only where
it cannot (single-artifact compilation cannot see a child's parent).

### 2.1 Child completion → parent objective `done` (synthesized)

For `<objective quest="c"/>` the compiler synthesizes the completion
predicate:

```
quest.c.state == 'complete'
```

`ObjectiveEntry.done` stays a required, always-present `CelPair`, so an
engine that predates this feature evaluates subquest objectives correctly
with zero changes. Derived parent completion ("all non-`optional` objectives
`done`") is untouched; `optional` on a subquest objective means the child's
outcome does not gate parent completion.

### 2.2 Upward failure (synthesized)

The parent document knows its children's ids, so the parent's effective
`fail` is synthesized as the disjunction of the authored predicate (if any)
and one `failed` test per **required** child:

```
<authoredFail> || quest.c1.state == 'failed' || quest.c2.state == 'failed'
```

An `optional` child's failure contributes nothing. `QuestCmd.fail` carries
the synthesized `CelPair` (raw = the synthesized text); engine contract
unchanged. Existing precedence holds: `fail` is evaluated before derived
completion.

### 2.3 Downward failure (engine-derived rule)

A child compiled in its own document does not know its parent — the
reference points parent → child — so this direction cannot be synthesized
per-artifact. It becomes a documented engine rule in
`docs/runtime/quest-lifecycle.md`:

> When a quest transitions to a terminal state (`failed` or `complete`),
> every child of that quest still `active` transitions to `failed` and fires
> its `questFailed` handlers. The parent→child edges are the
> `ObjectiveEntry.quest` fields, unioned across artifacts exactly as
> `relations`/`rules`/`prereqEdges` already are.

Notes: a required child cannot be `active` when its parent completes (its
completion is part of the parent's derived completion), so the
parent-`complete` arm of this rule only ever fails `optional` children. The
cascade applies recursively (a cascaded failure is a terminal transition).
Reusing `failed` was an explicit decision: a fifth lifecycle state
(`abandoned`) would ripple through the enum's every consumer (match
exhaustiveness, diagnostics, IR, engine contract) for one nuance of journal
copy.

### 2.4 Activation of referenced children

Being referenced refines a child's activation semantics — the consequence of
"child = big objective" (objectives are only evaluated while their quest is
active):

- **No `start`** → the child activates when its parent activates (instead of
  today's activate-at-walk-start / accept-driven default).
- **With `start`** → the predicate is evaluated only while the parent is
  `active`; the effective gate is the conjunction.

This is an engine rule (same union-derived tree as §2.3), documented in
`quest-lifecycle.md`. An unreferenced quest keeps today's semantics exactly.
`quest.<child>.activatedAt` stamping is unchanged.

## 3. IR (append-only, byte-stability preserved)

- `ObjectiveEntry` gains `quest: Option<String>`
  (`skip_serializing_if = "Option::is_none"`, appended after `body`).
  Serialized only for subquest objectives; artifacts without the feature are
  byte-identical. The journal tree is derivable from this field alone — no
  new edge table, no new command kind.
- `ObjectiveEntry.done` carries the synthesized predicate (§2.1);
  `QuestCmd.fail` carries the synthesized disjunction (§2.2).
- `ProjectIndex` unions nothing new; engines already union quest commands.

## 4. Diagnostics

| Code | Scope | Fires when |
| --- | --- | --- |
| `E-OBJECTIVE-QUEST-DONE` | document | `quest=` and `done=` on one objective. |
| `E-QUEST-REF-UNKNOWN` | document / project | `quest=` names no known quest id. Same split as `after` targets: same-document references resolve at `check`; cross-document resolution is `check-project`'s (a single artifact records the reference unresolved). |
| `E-QUEST-MULTI-PARENT` | project | One quest referenced by `quest=` from two parents (tree, not DAG). |
| `E-QUEST-TREE-CYCLE` | project | The parent→child edges form a cycle. Self-reference is a length-1 cycle; when parent and child share a document, `check` catches it early. |
| `E-OBJECTIVE-UNSATISFIABLE` (extension) | project | A required subquest objective whose child is `E-QUEST-UNREACHABLE` — unreachable-child liveness propagates to the referencing objective. |

## 5. Touched surfaces

- `crates/lute-syntax` — `quest=` attribute on `<objective>` (attr parsing
  only; no grammar change, tree-sitter attrs are generic).
- `crates/lute-check` — mutual-exclusion + same-doc resolution
  (`match_check.rs` neighborhood); project-level tree checks
  (`project_check.rs`); liveness propagation (`reachability.rs` /
  `producible.rs`).
- `crates/lute-compile` — `ObjectiveEntry.quest`, `done`/`fail` synthesis
  (`ir.rs`, `lib.rs`); snapshot updates.
- `docs/runtime/quest-lifecycle.md` — §2.3/§2.4 engine rules.
- `packages/website` … `quests-and-scenes.md`, `docs/examples/`,
  conformance test, CHANGELOG.

## 6. Out of scope

- `abandoned` lifecycle state (decided against, §2.3).
- Journal presentation metadata (ordering, grouping labels) — engine/UI
  concern; the tree itself is the language's contribution.
- The eevee catalog projection pipeline and any rich-catalog (row/field
  lookup) capability — separate tracks.
- Cross-quest scheduling sugar (e.g. auto-`after=` between siblings).
