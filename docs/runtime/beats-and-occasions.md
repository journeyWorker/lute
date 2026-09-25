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
  evening:    { select: sequence }
```

- `select: first` (the default) — the engine presents the single winning beat,
  then any eligible `also` beat (below).
- `select: all` — the engine offers every eligible beat, in selection order,
  and the player picks one (or none, below).
- `select: sequence` (dsl 0.23.0 §3) — the engine presents **every** eligible
  beat, one after another, in selection order: a routine followed by the
  day's event.
- `target: true` — the occasion is raised for a target, and a beat may
  restrict itself to one. Targets are any dotted id (shape only).
- `target: { prefix, entity }` (dsl 0.22.0 §8) — a **target domain**: the
  occasion is raised for `<prefix>.<member>`, where `<member>` is a member of
  the project's entity kind `entity` (`entities:` in the schema; any
  `<prefix>.<id>` when the kind is `open:`, its members engine-populated).
  Raise it with targets of that shape: they are the ids every beat target of
  the occasion was checked against.
- `target: { prefix, entity, members: [ids] }` (0.23.1) — a target domain
  narrowed to a **member subset**: the occasion is raised only for
  `<prefix>.<member>` of a listed member. Every listed member must belong to
  `entity` when that kind is closed (`E-BEAT-ATTR` names the stray member,
  with a did-you-mean over the kind's members); under an `open:` kind the list
  itself is the domain. An empty list, or a member listed twice, is a load
  error of the declaration file. The subset narrows every consumer alike:
  beat and objective targets, `lute play` step targets, `lute calendar`
  columns.

Occasion declarations are not in the artifact; they are part of the capability
snapshot (`capabilityVersion`), where an occasion's `target` serializes as
`false`, `true`, `{ "prefix", "entity" }`, or `{ "prefix", "entity",
"members" }`. A domain changes the stamp, and so does its member list; a
domain without `members` keeps its 0.22 stamp, and an occasion declared only
with `target: true` / `false` keeps its 0.21 stamp. An
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
  also?: true;            // dsl 0.23.0 §3; present only when authored true
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

A **bundle beat** (dsl 0.23.0 §4) is a `beat` record of a lore artifact: a
scene-like beat written as `<beat>` in a lore document, beside other beats and
entries. Like an `entry` record, it heads its own addressing unit, and its
body segment follows it:

```ts
type BeatCmd = {
  kind: "beat";
  addr: Addr;
  id: string;             // canonical: `<document id>.<beat id>`
  on: string;
  target?: string;
  title?: string;
  titleLineId?: string;   // `<id>.title`
  when?: CelPair;         // eligibility, `@def`-expanded
  priority: number;       // resolved; unauthored → 0
  once: "run" | "user" | "none"; // unauthored → "run"
  also?: true;
  body: Addr;             // first record of the body segment
};
```

The canonical `id` is the beat's `ProjectIndex.beats` row id, its key in the
presentation record, and its `visited()` key. The body segment runs from
`body` to the next `entry` or `beat` record, or the end of the artifact, and
holds what a scene shot holds (lines, choices, hubs, matches, staging,
writes). A bundle beat is otherwise a **scene beat**: its eligibility is its
`when` and its `once` against the presentation record (it has no `after:`).

`ProjectIndex.beats` (`lute compile --all`) lists every beat in the project so
an engine can build its `occasion → candidates` table without loading every
artifact:

```ts
type IndexBeat = {
  id: string;             // the scene's meta.id, the entry id, or a bundle beat's canonical id
  kind: "scene" | "entry" | "bundle";
  document: string;       // the owning documents[].path
  on: string;
  target?: string;
  priority: number;       // resolved (unauthored → 0)
  once?: "run" | "user" | "none"; // scene rows: always; entry rows: the authored `once`, absent = repeatable
  when?: string;          // dsl 0.23.0 §1: the beat's condition, `@def`-expanded
  title?: string;         // dsl 0.23.0 §11: scene `title:`, entry or bundle beat `title=`
};
```

Rows also carry `when` (the raw condition after `@def` expansion) and `title`
(a scene's `title:`, an entry's or bundle beat's `title=`), each omitted when
absent (dsl 0.23.0 §1, §11), so a tool can list the beats and an engine can
label a `select: all` menu without loading every artifact.

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
4. `select: first` presents the first eligible beat that is not `also` (the
   **winner**), then every eligible `also` beat in selection order (dsl 0.23.0
   §3). `select: sequence` presents every eligible beat in selection order.
   `select: all` offers the ordered list and presents the one the player
   picks. Beats offered but not picked are not presented and spend nothing.
   The player may also close the list without picking (dsl 0.22.0 §10): then
   nothing is presented or spent, and the occasion still judges its
   objectives (below).
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
    if (select === "all") {
      const chosen = playerPicks(eligible); // undefined: the player closed the list
      if (chosen) present(chosen, state, facts, presented);
    } else {
      for (const beat of presentationOrder(select, eligible)) {
        present(beat, state, facts, presented);
        settleQuests(state, facts); // every presentation is an evaluation instant, quest-lifecycle.md
      }
    }
  } // else the occasion passes with no story: engine default
  judgeObjectives(occasion, state, facts); // objectives with `on`, quest-lifecycle.md
}

// Eligibility is decided once, when the occasion is raised: presenting one
// beat of the list never makes a later one eligible or ineligible.
function presentationOrder(select: "first" | "sequence", eligible: IndexBeat[]) {
  if (select === "sequence") return eligible;
  // alsoOf: a scene's meta.beat.also, a bundle beat's `also`; an entry never
  const winner = eligible.find((beat) => !alsoOf(beat));
  return [...(winner ? [winner] : []), ...eligible.filter(alsoOf)];
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

Presenting a **bundle beat** is presenting a scene beat: the engine adds its
canonical id to the presentation record (so `visited('<document id>.<beat
id>')` holds from then on), then runs its body segment — from `body` to the
next `entry` or `beat` record — exactly as a scene's command stream runs
(`execution-model.md`): choices, hubs, and matches select, staging plays, and
every `set` / `assert` / `retract` applies (there is no first-read rule).
`scene.*` state is fresh for each presentation, as for a scene.

Every beat an occasion presents is presented in full, spends its own `once`,
and is followed by a quest settle before the next one begins: a `select:
sequence` routine and the day's event, or a `select: first` winner and the
`also` beats riding along, are each an evaluation instant for quest
lifecycles (`quest-lifecycle.md`). A `::end` ends the presentation it runs in
(its settle still runs); the occasion's other presentations and judging go on.

An `also` beat (`also: true` in a scene's frontmatter, or a bundle beat's
`also`, dsl 0.23.0 §3) is a side remark: it never competes for the win and
never replaces the main beat. It is presented after the winner when it is
eligible — and when no main beat is eligible at all. `also` means something
only on a `select: first` occasion; on `select: all` or `sequence` every
eligible beat is already offered or presented, so there it is `E-BEAT-ATTR`.
An entry beat cannot ride along (`<entry also>` is `E-BEAT-ATTR`).

## Occasions judge objectives

An occasion also judges quest objectives that name it (dsl 0.21.0 §7a.2): an
`ObjectiveEntry` with `on` is evaluated only when its occasion is raised
while its quest is `active` (`quest-lifecycle.md`). Raising an occasion
therefore does two things, in this order: select and present a beat as
above, then answer the raise in every active quest and settle those quests.
Answering the raise is itself two steps, in this order (0.23.1): when a world
event of the same name is declared, the engine fires it — every active
quest's `<on event>` handler for that name runs, once, and the event carries
no target — and then every active quest's objectives with that `on` are
evaluated, so an objective can read what the handler wrote. An occasion
referenced only by objectives or same-named handlers is still raised — with
no beat to present, only the second step runs, and so does a `select: all`
list the player closed without picking. An occasion never fires a quest
lifecycle event (`questActive`, `questComplete`, `questFailed`). An objective's
occasion is checked against the vocabulary exactly as a beat's `on`
(`E-OCCASION-UNKNOWN`, `E-BEAT-ATTR`). An objective without `target` is
judged whenever its occasion is raised, whatever the target. An objective
with `target=` (dsl 0.23.0 §2) follows the beat target rule: it is judged only
when the occasion is raised for that target. Its target is checked like a
beat's: `E-BEAT-ATTR` when it is not a quoted dotted id, has no `on`, sits on
an occasion that takes no target, or lies outside the occasion's domain.

## Static guarantees

An artifact that compiled cleanly carries these guarantees:

- `on` is an identifier, `target` a dotted id, `priority` an integer, a scene
  beat's `once` one of `run` / `user` / `none` and an entry beat's `once`
  absent or one of `run` / `user`; beat keys never appear without `on`
  (`E-BEAT-ATTR`).
- `also` is `true` or `false`, and appears only on a scene or bundle beat of a
  `select: first` occasion (`E-BEAT-ATTR`).
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
falls to `ProjectIndex.beats` order. Both ignore `also` beats, which never
compete for the win: an `also` beat is never shadowed, never shadows, and
never ties. `W-BEAT-ONCE-RUN-USER` names a beat whose `once` is defaulted to
`run` and whose `when` reads only user-tier state, so once it holds it
replays every run; an authored `once: run` (an entry's `once="run"`)
acknowledges that and silences it, and a `prev.run.*` read counts as run
history, not user state.

## Reference tooling

- `lute play <dir> --script <play.yaml>` walks a playthrough as a sequence of
  raised occasions: for each step it computes the candidates and their
  verdicts, presents the winner and its eligible `also` beats (every eligible
  beat on a `select: sequence` occasion, or the step's `pick` on a `select:
  all` occasion; `pick: none` closes the list), runs each with the reference
  runner, and advances every quest lifecycle as `lute run` does after each
  presentation, so `when` conditions over `quest.*` and `after: completed(…)`
  see real progress; a step's occasion also judges the objectives that name
  it, for the step's `target`. A step target outside the occasion's target
  domain is a usage error with a did-you-mean, and so is `pick:` on a
  `select: sequence` occasion. A step's `expect: { presented: [ids] }` matches
  the presented beats exactly and in order. `--json` emits the same
  transcript: `presented` is the first presentation and `then` lists the rest
  in order; a candidate or presentation that rides along carries `also: true`.
- In `lute play`, a `::end` ends only the presentation (or quest handler) it
  runs in; the playthrough goes on with the next step. A step `end: true`
  ends the playthrough: exit 0, and every later step is listed as skipped
  (`skipped: [{ step, label }]` in `--json`). The transcript prints staging as
  authored (`::bg{…}`, `::auto{…}`, a plugin directive by its own name); `--ir`
  prints the lowered records instead, compiler-injected ones included.
- `lute calendar <dir> --script <play.yaml>` starts every cell from the
  script's save with its steps replayed as `lute play` plays them; `--until
  <step number | label>` stops before that step. An axis
  `quest.<id>.state=…` seeds the quest's status (an `unset`/`active` seed
  drops the route's objective progress), `holds(<fact>)=true,false` asserts or
  retracts a base fact, and an axis the calendar cannot apply (another
  `quest.*` path, a derived fact) is a usage error. `--where <cel>` keeps only
  the cells where the condition holds after the axes are applied and quests
  settle — independent axes otherwise combine into saves no run reaches. A
  targeted occasion gets one column per target its beats name (or per
  `--target`), never the unanswered rest of its declared domain.
- `lute scenario <dir>` draws every bundle beat as an edgeless entry node
  `beat(<doc>.<beat>)`; `scenario reach|envelope` accept its canonical id,
  bare or `beat:`-prefixed.
- `lute trace` / `lute run` raise occasions for a quest walk with the mock
  key `occasions: [runEnd]` or `--occasion runEnd` (repeatable), applied in
  order after the walk settles. A raise for a target is written
  `<occasion>@<target>` (`occasions: [talk@npc.maud]`, `--occasion
  talk@npc.maud`); it judges the objectives on that occasion whose `target`
  is absent or equal to it.
- `lute trace <lore.lute> --beat <id>` and `lute run <lore artifact> --beat
  <id>` present one bundle beat by its local or canonical id, outside any
  selection: `when` is evaluated and shown on the beat's head, not enforced,
  and the body runs as a scene's (a mock's `choose:` picks). An unknown id is
  `E-TRACE-BEAT` (`trace`, exit 1) or a usage error (`run`, exit 2).
