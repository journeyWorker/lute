# Beats and occasions

A narrative game picks its next piece of story at moments of its own: a hub
visit, entering a room, talking to an NPC, a new day, the start of a run. Lute
calls those moments **occasions** and the story pieces that answer them
**beats** (dsl 0.21.0). The engine raises occasions; Lute defines which beats
are eligible and which one wins. The grounding here is `ir.rs::{SceneMeta,
BeatIr, EntryCmd}`, `ProjectIndex.beats`, and the proposals
[`0.21.0.md`](../proposals/scenario-dsl/0.21.0.md) and
[`0.22.0.md`](../proposals/scenario-dsl/0.22.0.md) (entry `once`, target
domains, closing an offered list).

## Occasions

An occasion is **engine vocabulary**: a named moment the engine raises,
optionally *for* a target (`talk` → `npc.vesna`, `roomEnter` → `room.lab`).
Which occasions exist, and when each fires, is the engine's design. A plugin
manifest MAY declare them:

```yaml
occasions:
  hubVisit:   { select: first }
  talk:       { select: first, target: { prefix: npc, entity: person } }
  examine:    { select: first, target: true }
  inbox:      { select: all }
```

- `select: first` (the default) — the engine presents the single winning beat.
- `select: all` — the engine offers every eligible beat, in selection order,
  and the player picks one (or none, below).
- `target: true` — the occasion is raised for a target, and a beat may
  restrict itself to one. Targets are any dotted id (shape only).
- `target: { prefix, entity }` (dsl 0.22.0 §8) — a **target domain**: the
  occasion is raised for `<prefix>.<member>`, where `<member>` is a member of
  the project's entity kind `entity` (`entities:` in the schema; any
  `<prefix>.<id>` when the kind is `open:`, its members engine-populated).
  Raise it with targets of that shape: they are the ids every beat target of
  the occasion was checked against.

Occasion declarations are not in the artifact; they are part of the capability
snapshot (`capabilityVersion`), where an occasion's `target` serializes as
`false`, `true`, or `{ "prefix", "entity" }`. A domain changes the stamp; an
occasion declared only with `target: true` / `false` keeps its 0.21 stamp. An
engine raising an occasion that no resolved plugin declares uses
`select: first`. The checker guarantees every compiled beat names a declared
occasion once any plugin declares one (`E-OCCASION-UNKNOWN`), a target only on
an occasion declared with `target: true` or a domain (`E-BEAT-ATTR`), and, on
a domain occasion, only a target inside the domain (`E-BEAT-ATTR`).

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
  once?: "run" | "user";  // dsl 0.22.0 §7; absent: repeatable
};
```

An entry beat's candidate target is its ordinary `target`, and its
eligibility is its ordinary `when` plus its `once`. Re-presentation is an
entry's nature, so without `once` an entry beat is never spent, as in 0.21.
With `once` (dsl 0.22.0 §7) it is spent by its own read flags
(`lore-entries.md`), not by a presentation record:

- `once: "run"` — not eligible while `entry.<id>.read` is true (read this
  run; a run start resets it);
- `once: "user"` — not eligible once `entry.<id>.everRead` is true (read in
  any run; never reset).

The flags are set by any first read, so an entry the engine presented by
looking it up (its `target`, `category`) spends its `once` too.

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
  once?: "run" | "user" | "none"; // scene rows: always; entry rows: the authored `once`, absent = repeatable
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
   - a beat's `once` is not spent. A scene beat: `run` — not yet presented
     this run; `user` — never presented; `none` — never spent. An entry beat:
     `run` — `entry.<id>.read` is false; `user` — `entry.<id>.everRead` is
     false; absent — never spent.
3. Eligible beats are **ordered by `priority` descending, then
   `ProjectIndex.beats` order**. Scene and entry beats on the same occasion
   compete in one list.
4. `select: first` presents the first eligible beat; `select: all` offers the
   ordered list and presents the one the player picks. Beats offered but not
   picked are not presented and spend nothing. The player may also close the
   list without picking (dsl 0.22.0 §10): then nothing is presented or spent,
   and the occasion still judges its objectives (below).
5. **No eligible beat** — the occasion passes with no story, and the engine's
   default behavior for that moment applies.

The engine keeps the presentation record a scene beat's `once` reads. The run
record resets with the **run** tier and the user record persists with the
**user** tier (`state-lifecycle.md`); both are keyed by the beat's `id`.
Content never reads or writes them. An entry beat's `once` reads the entry's
read flags instead, which the engine already keeps (`lore-entries.md`).

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
  if (eligible.length > 0) {
    const chosen = select === "first" ? eligible[0] : playerPicks(eligible);
    if (chosen) present(chosen, state, facts, presented); // undefined: the player closed the list
  } // else the occasion passes with no story: engine default
  judgeObjectives(occasion, state, facts); // objectives with `on`, quest-lifecycle.md
}

function isEligible(beat: IndexBeat, state, facts, presented: Presented) {
  if (beat.kind === "entry") {
    if (beat.once === "run" && state.get(`entry.${beat.id}.read`)) return false;
    if (beat.once === "user" && state.get(`entry.${beat.id}.everRead`)) return false;
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
    presentEntry(entryRecord(beat.id), state, facts); // lore-entries.md: sets read / everRead
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
apply only on the first read in a run, after which the engine sets
`entry.<id>.read = true` and `entry.<id>.everRead = true`.

## Occasions judge objectives

An occasion also judges quest objectives that name it (dsl 0.21.0 §7a.2): an
`ObjectiveEntry` with `on` is evaluated only when its occasion is raised
while its quest is `active` (`quest-lifecycle.md`). Raising an occasion
therefore does two things, in this order: select and present a beat as
above, then evaluate every active quest's objectives with that `on` and
settle those quests. An occasion referenced only by objectives is still
raised — with no beat to present, only the second step runs, and so does a
`select: all` list the player closed without picking. An objective's
occasion is checked against the vocabulary exactly as a beat's `on`
(`E-OCCASION-UNKNOWN`, `E-BEAT-ATTR`). An objective has no target (dsl
0.22.0 §8 defers objective targets to 0.23): it is judged whenever its
occasion is raised, whatever the target.

## Static guarantees

An artifact that compiled cleanly carries these guarantees:

- `on` is an identifier, `target` a dotted id, `priority` an integer, a scene
  beat's `once` one of `run` / `user` / `none` and an entry beat's `once`
  absent or one of `run` / `user`; beat keys never appear without `on`
  (`E-BEAT-ATTR`).
- `on` names a declared occasion whenever any resolved plugin declares
  occasions (`E-OCCASION-UNKNOWN`), and a `target` of a domain occasion is
  `<prefix>.<member>` of that domain (`E-BEAT-ATTR`).
- A beat's `when` is not provably always false (`E-BEAT-UNREACHABLE`), and it
  never reads the scene's own `scene.*` state, which does not exist until the
  scene runs.

Three `check-project` warnings are advisory. `W-BEAT-SHADOWED` names a
`select: first` beat that an earlier-ordered, always-eligible, never-spent
beat (a scene with `once: false`, an entry without `once`) on the same
occasion and target beats every time. `W-BEAT-PRIORITY-TIE` names beats on
one `select: first` occasion, either untargeted or for the same target, with
equal priority and `when`s not provably exclusive, whose winner therefore
falls to `ProjectIndex.beats` order. `W-BEAT-ONCE-RUN-USER` names a beat
spent once per run whose `when` reads only user-tier state, so once it holds
it replays every run.

## Reference tooling

- `lute play <dir> --script <play.yaml>` walks a playthrough as a sequence of
  raised occasions: for each step it computes the candidates and their
  verdicts, presents the winner (or the step's `pick` on a `select: all`
  occasion; `pick: none` closes the list), runs it with the reference runner,
  and advances every quest lifecycle as `lute run` does, so `when` conditions
  over `quest.*` and `after: completed(…)` see real progress; a step's
  occasion also judges the objectives that name it. A step target outside the
  occasion's target domain is a usage error with a did-you-mean. `--json`
  emits the same transcript.
- `lute trace` / `lute run` raise occasions for a quest walk with the mock
  key `occasions: [runEnd]` or `--occasion runEnd` (repeatable), applied in
  order after the walk settles.
