# Beats and occasions

A narrative game picks its next piece of story at moments of its own: a hub
visit, entering a room, talking to an NPC, a new day, the start of a run. Lute
calls those moments **occasions** and the story pieces that answer them
**beats** (dsl 0.21.0). The engine raises occasions; Lute defines which beats
are eligible and which one wins. The grounding here is `ir.rs::{SceneMeta,
BeatIr, EntryCmd}`, `ProjectIndex.beats`, and the proposal
[`0.21.0.md`](../proposals/scenario-dsl/0.21.0.md).

## Occasions

An occasion is **engine vocabulary**: a named moment the engine raises,
optionally *for* a target (`talk` → `npc.vesna`, `roomEnter` → `room.lab`).
Which occasions exist, and when each fires, is the engine's design. A plugin
manifest MAY declare them:

```yaml
occasions:
  hubVisit:   { select: first }
  talk:       { select: first, target: true }
  inbox:      { select: all }
```

- `select: first` (the default) — the engine presents the single winning beat.
- `select: all` — the engine offers every eligible beat, in selection order,
  and the player picks one.
- `target: true` — the occasion is raised for a target, and a beat may
  restrict itself to one.

Occasion declarations are not in the artifact; they are part of the capability
snapshot (`capabilityVersion`). An engine raising an occasion that no resolved
plugin declares uses `select: first`. The checker guarantees every compiled
beat names a declared occasion once any plugin declares one
(`E-OCCASION-UNKNOWN`), and a target only on an occasion declared with
`target: true` (`E-BEAT-ATTR`).

## Beats in the IR

A **scene beat** is a scene whose envelope carries `meta.beat`:

```ts
type SceneMeta = {
  id: string;
  // … legacy identity, title, extra, plugin …
  beat?: BeatIr;          // absent: the scene is reached by explicit flow only
};

type BeatIr = {
  on: string;             // the occasion this scene answers
  target?: string;        // candidate only when the occasion is raised for this target
  when?: CelPair;         // eligibility, `@def`-expanded
  priority: number;       // resolved; unauthored → 0; higher wins
  once: "run" | "user" | "none"; // unauthored → "run"; "none" is source `once: false`
};
```

An **entry beat** is an `entry` record (`lore-entries.md`) that carries `on`:

```ts
type EntryCmd = {
  // … id, target, category, title, series, order, when, body …
  on?: string;            // the occasion this entry answers
  priority?: number;      // absent → 0
};
```

An entry beat's candidate target is its ordinary `target`, and its
eligibility is its ordinary `when`. Entries have no `once`: re-presentation
is their nature, and an entry meant to be heard once guards on its own
`entry.<id>.read`.

`ProjectIndex.beats` (`lute compile --all`) lists every beat in the project so
an engine can build its `occasion → candidates` table without loading every
artifact:

```ts
type IndexBeat = {
  id: string;             // the scene's meta.id, or the entry id
  kind: "scene" | "entry";
  document: string;       // the owning documents[].path
  on: string;
  target?: string;
  priority: number;       // resolved (unauthored → 0)
  once?: "run" | "user" | "none"; // scene rows only
};
```

Rows are in **document order**: documents in `documents` (path) order, beats
in declaration order within each. That order is the selection tiebreak below.
An engine that does not read the index MUST reproduce the same order.
`beats` is omitted from an index whose project declares no beat.

## Selection

When the engine raises occasion `O`, optionally for target `T`:

1. **Candidates** are the beats with `on == O` whose `target` is absent or
   equal to `T`. An occasion raised without a target has only untargeted
   candidates.
2. A candidate is **eligible** when all of these hold:
   - a scene beat's `after:` holds — the artifact's `prereqEdges` row whose
     `node` is the scene's `meta.id`, evaluated as it is for any scene
     (`quest-lifecycle.md`); an entry has no `after:`;
   - its `when` is absent or evaluates true against live state and facts
     (`evalSlot(when.raw, when.expr, …)`, `execution-model.md`) at the moment
     the occasion is raised;
   - a scene beat's `once` is not spent: `run` — not yet presented this run;
     `user` — never presented; `none` — never spent. Entry beats are never
     spent.
3. Eligible beats are **ordered by `priority` descending, then
   `ProjectIndex.beats` order**. Scene and entry beats on the same occasion
   compete in one list.
4. `select: first` presents the first eligible beat; `select: all` offers the
   ordered list and presents the one the player picks. Beats offered but not
   picked are not presented and spend nothing.
5. **No eligible beat** — the occasion passes with no story, and the engine's
   default behavior for that moment applies.

The engine keeps the presentation record `once` reads. The run record resets
with the **run** tier and the user record persists with the **user** tier
(`state-lifecycle.md`); both are keyed by the beat's `id`. Content never reads
or writes them.

Selection is deterministic: for the same state, facts, and presentation
record, every engine picks the same beat. An engine MAY layer its own policy
on top — weighted randomness, cooldowns — but that policy is the engine's, and
the reference tooling implements exactly the order above.

```ts
type Presented = { run: Set<string>; user: Set<string> };

function raise(index: ProjectIndex, occasion: string, target: string | undefined,
               state, facts, presented: Presented) {
  const select = declaredOccasion(occasion)?.select ?? "first";
  const eligible = index.beats
    .map((beat, order) => ({ beat, order }))
    .filter(({ beat }) => beat.on === occasion &&
                          (beat.target === undefined || beat.target === target))
    .filter(({ beat }) => isEligible(beat, state, facts, presented))
    .sort((a, b) => b.beat.priority - a.beat.priority || a.order - b.order)
    .map(({ beat }) => beat);
  if (eligible.length === 0) return; // the occasion passes: engine default
  const chosen = select === "first" ? eligible[0] : playerPicks(eligible);
  present(chosen, state, facts, presented);
}

function isEligible(beat: IndexBeat, state, facts, presented: Presented) {
  if (beat.kind === "entry") {
    const when = entryRecord(beat.id).when;
    return !when || truthy(evalSlot(when.raw, when.expr, state, facts));
  }
  const artifact = sceneArtifact(beat.id);
  const { when, once } = artifact.meta.beat;
  if (!afterHolds(artifact, state, facts)) return false; // prereqEdges row for meta.id
  if (when && !truthy(evalSlot(when.raw, when.expr, state, facts))) return false;
  if (once === "run" && presented.run.has(beat.id)) return false;
  if (once === "user" && presented.user.has(beat.id)) return false;
  return true;
}

function present(beat: IndexBeat, state, facts, presented: Presented) {
  if (beat.kind === "entry") {
    presentEntry(entryRecord(beat.id), state, facts); // lore-entries.md
    return;
  }
  presented.run.add(beat.id);
  presented.user.add(beat.id);
  run(sceneArtifact(beat.id), state, facts);         // execution-model.md
}
```

## Presentation

Presenting a **scene beat** runs the scene: its artifact's command stream from
the first record, exactly as any other scene (`execution-model.md`).

Presenting an **entry beat** follows the entry rules (`lore-entries.md`): its
body runs against live state, and its `set` / `assert` / `retract` records
apply only on the first read, after which the engine sets
`entry.<id>.read = true`.

## Static guarantees

An artifact that compiled cleanly carries these guarantees:

- `on` is an identifier, `target` a dotted id, `priority` an integer, and
  `once` one of `run` / `user` / `none`; beat keys never appear without `on`
  (`E-BEAT-ATTR`).
- `on` names a declared occasion whenever any resolved plugin declares
  occasions (`E-OCCASION-UNKNOWN`).
- A beat's `when` is not provably always false (`E-BEAT-UNREACHABLE`), and it
  never reads the scene's own `scene.*` state, which does not exist until the
  scene runs.

`W-BEAT-SHADOWED` (`check-project`) is advisory: it names a `select: first`
beat that an earlier-ordered, always-eligible, never-spent beat on the same
occasion and target beats every time.

## Reference tooling

- `lute play <dir> --script <play.yaml>` walks a playthrough as a sequence of
  raised occasions: for each step it computes the candidates and their
  verdicts, presents the winner (or the step's `pick` on a `select: all`
  occasion), runs it with the reference runner, and advances every quest
  lifecycle as `lute run` does, so `when` conditions over `quest.*` and
  `after: completed(…)` see real progress. `--json` emits the same transcript.
