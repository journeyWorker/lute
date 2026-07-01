# scenario DSL — state-model design rationale & audit record

> **Normative spec:** the state model is folded into [`0.0.1.md`](0.0.1.md) §6.1 (frontmatter),
> §9 (state tiers / import / definite-assignment / write policy), §11.2 (match coverage), and
> [`../../architecture.md`](../../architecture.md) (system layers + Roadmap #6 persistence backends).
> This doc is the
> **rationale + audit trail** behind that model — *why* it is shaped this way, not the normative
> text. Because `0.0.1` is pre-implementation and unpublished, this is a correction *within* 0.0.1,
> **not** a breaking version bump (no migration: nothing shipped with the old shape).

## Problem with the original two-namespace model

`0.0.1` originally shipped `scene.*` (cleared at scene end) + `save.*` (persistent). Two defects
surfaced on a **roguelike-shaped** game (a *run* = one attempt = ~10 scenes, plus meta-progression
across attempts):

1. **Wrong axis.** `scene` is a *lifetime* (named by when it resets); `save` is a *storage
   mechanism*. They are not the same kind of name, and `save` silently conflated at least three
   lifetimes — per-attempt, per-player, per-install.
2. **No tier for "the previous *episode's* choice."** `scene.*` clears at **episode end** (one
   `.lute` document; it does span shots *within* an episode), so a choice made in one episode could
   not carry into the **next episode** of the same run/attempt. The canonical ask — "if a prior
   episode chose A, this episode opens here" — had no home except the over-coarse `save.*`.

## The four decisions (rationale)

1. **Four lifetime tiers, named by reset boundary** — `scene` / `run` / `user` / `app`. One axis
   (*when does it reset?*), so every name is a boundary. `save.*` removed (it named the mechanism).
   `run.*` is the answer to cross-scene/EP carry within an attempt; `user.*` is meta-progression
   surviving runs; `app.*` is identity-independent device state (language, age rating). All four
   are read by `<match>`/`<when>`. The engine owns each persistence backend and **fires every
   reset** — the language never triggers one.

2. **Path-sensitive definite-assignment.** Non-`scene` tiers may carry in from a prior run / be
   never-set, so they are **maybe-unset at scene entry** unless schema-defaulted. After entry the
   ordinary path-sensitive analysis applies: a dominating `::set{p=…}` write or a guard
   (`has(p)`/`isSet(p)`) proves `p`; compound `+=`/`-=`/`*=` carries an implicit read (so it needs
   a default/guard/prior write — only `=` may be first). This makes "branched into content the
   player never unlocked" fail closed at compile time, not silently at play time.

3. **Declarations as `---` frontmatter, not a `:::meta` fence.** Repo-fit: bard already lives on
   `---` YAML frontmatter (editorials, chunks, `.md`/`.json` sidecars, harp-validated). `:::meta`
   also mis-signalled — `:::` is a body sigil but `meta` is a document header. In this DSL,
   `:::meta` was the *only* `:::` construct (the former `:::route` is the §7.3 `<branch>`/`<match>`
   nesting), so moving meta to frontmatter **removes `:::` from the grammar entirely** — cleaner
   than retaining an open fence. Frontmatter is the container, not a typing escape hatch: the
   schema inside is still checker-validated.

   *Note:* `meta.luteVersion` (frontmatter) is the **DSL language-version** pin (§2) — it is **not** the
   `app.lang` UI-language *state*. The two coexist; this was conflated in an early audit pass and
   is corrected here.

4. **Schema is a single SoT, imported (`uses`).** `run`/`user`/`app` is game/season-global (one
   persisted value cannot have per-scene types), so it lives in one schema document; scenes `uses:`
   it and declare only `scene.*` locals. Import is a **DAG** with normative rules: cycle rejection
   (diagnostic prints the chain), schemas checked before scenes, no scene override of an imported
   tier, `E-UNDECLARED` for unknown paths, duplicate-`defs` error (no silent shadow), path
   canonicalization (two paths to one file = one identity). The state schema is *game content* —
   separate from the engine **capability manifest** (*engine vocabulary*). It rides the planned
   `check()` **provider-snapshot** interface, which carries the *resolved* result; the rules above
   define *how* it resolves.

## Worked shape

Cross-**episode** carry is the headline (one `.lute` doc = one episode; `scene.*` clears at episode
end, so the carrying tier is `run.*`). Two episode documents in one run:

```
# state.schema.lute — SoT; every episode validates against it
---
state:
  run.choseHelp:      { type: bool, default: false }            # carries across episodes in this run
  user.level:         { type: number, default: 1 }
  user.sawTrueEnding: { type: bool, default: false }
  app.rating:         { type: enum, values: [teen, adult], default: teen }   # content-read-only
  app.lang:           { type: enum, values: [ko, en] }
defs:
  warm:  { type: bool, cel: "run.level >= 2" }
  chose: { type: bool, params: { q: choiceRef, opt: choiceId }, cel: "scene.choices[q] == opt" }
---
```

```
# ep02.lute — episode 2: the choice happens here
---
character: sofia
season: 1
episode: 2
uses: ../state.schema.lute
---
## Shot 1.
<branch id="couch">                  # scene.choices.couch auto-recorded; spans shots within ep02
  <choice id="help" label="같이 옮긴다">
    ::set{run.choseHelp = true}      # primitive: promote into run → survives into the NEXT episode
  </choice>
  <!-- sugar equivalent: <choice id="help" label="같이 옮긴다" persist="run" as="choseHelp"> -->
  <choice id="ignore" label="모른 척한다"> ... </choice>
</branch>
## Shot 2.
<match on="scene.choices.couch">     # intra-episode reaction (same .lute doc, across shots)
  <when test="@chose('couch','help')"> ... </when>
  <otherwise> ... </otherwise>
</match>
```

```
# ep03.lute — episode 3: opens based on ep02's choice
---
character: sofia
season: 1
episode: 3
uses: ../state.schema.lute
---
## Shot 1.
<match on="run.choseHelp">           # reads the run-scoped carry from ep02 — "starts from here"
  <when test="$ == true"> :line[sofia]{code="..."}: 저번에 도와줘서… </when>
  <otherwise> ... </otherwise>       # run.choseHelp is schema-defaulted (false), so always assigned
</match>
```

## Newly-specified coverage (closed)

- **Default materialization.** A `default` is materialized into the initial tier state at schema
  load **and** re-materialized when the engine fires that tier's reset boundary (a cleared tier
  reloads its default; checker + engine share the one snapshot). A defaulted path is therefore
  always assigned; an undefaulted non-`scene` path is maybe-unset at scene entry.
- **Per-tier write policy.** `::set` is legal on `scene`/`run`/`user`. `app.*` is
  **content-read-only** — `::set{app.*}` is a compile error (engine/settings owns it).
- **Reset-event ownership.** The engine owns + fires every reset boundary; content only declares
  tier membership and may assume a tier is cleared exactly at — and only at — its boundary.

## Resolved questions (operator + audit)

1. **Schema scope:** start **flat** (one game-global schema); build the resolver as a **DAG** now
   so additive `extends:` lands later. Shadowing stays illegal unless explicitly introduced.
2. **Fifth `episode.*` tier:** **no** — `run` covers cross-EP-within-attempt, `scene` covers local.
3. **`uses:` resolution:** prefer **project-resolved schema ids**; provider snapshot canonicalizes.
   Relative paths are local shorthand, canonicalized to the same identity.
4. **`app.rating` gating:** a **hard release-build gate** (operator-confirmed) — an age-gated
   `<match on="app.rating">` MUST cover `teen` or carry `<otherwise>`; runtime gating alone is not
   enough.
5. **Write policy (operator-confirmed):** `app.*` is content-read-only (no in-story setting writes).

## Cross-episode choice carry — resolved (folded into §7.3 / §9.6 / §11.1)

**Question:** how should a prior episode's choice drive a later episode, given that promoting raw
`<branch id>` into `run.choices.<id>` would force branch ids to be **globally unique across the
run** and couple later episodes to earlier ones' menu-arm naming?

**Resolution (operator-confirmed; codex DSL-critic discussion, `drum swarm`):** never carry raw
choice keys. Cross-episode state is a **named, schema-declared `run.*` fact**, not a remembered
branch key — so the global-uniqueness problem dissolves (`<branch id>` stays **episode-local**,
§11.1). Three-way split:

| path | role |
|---|---|
| `scene.choices.<branchId>` | episode-local control-flow trace (intra-episode reactions only) |
| `run.<declaredFact>` | cross-episode narrative/gameplay state — named, declared in the schema SoT |
| `run.choiceLog.<ep>.<id>` | reserved engine-populated analytics/debug trace — **never** branched on |

- **Primitive (C):** `::set{run.<path> = <value>}` in the choice arm.
- **Sugar (B):** `<choice … persist="run" as="<run.path>" [value="<lit>"]>` desugars to that
  `::set`. `value` defaults to `true` for a `bool` path; **`enum`/`number` require `value`**
  (operator chose to include the typed-value form, not bool-only).
- **No implicit declaration:** `as` MUST resolve to a path already declared in the imported run
  schema; a typo'd/undeclared `as` is `E-UNDECLARED` (state-by-typo fails). The compiler also checks
  writability, `value` type-compatibility, and no conflicting same-arm writes.
- **`run.choiceLog.*` (operator-confirmed: include):** a reserved engine-owned trace (§9.6); content
  MUST NOT read it in a `<match>`/`<when>`/guard (lint error). Raw choice history lives here; authored
  branching reads named `run.*` facts.

Prefer reusable domain names for the facts (`metHelpfully`, `firstKindnessTone`,
`sofaHelpOutcome: enum[ignored, helped, overdid_it]`) over episode-coupled names.

## Audit trail (codex DSL-critic, via `drum swarm`)

- **Round 1:** Decision 2 **BROKEN** (the first rule rejected dominating in-scene writes and
  ignored the compound-assign implicit read) → rewritten path-sensitive. Decision 4 **WEAK** (import
  semantics unspecified) → normative DAG/cycle/order/dup-defs/canonicalization rules added.
  Decision 1 **WEAK** → stated as a within-0.0.1 correction (not a rename/alias). Decision 3
  **SOUND** → grammar tightened (`:::` removed entirely, per this DSL having no other `:::`).
- **Round 2:** both blockers **RESOLVED**; new coverage mutually consistent; one non-blocking nit
  (defaults re-materialize at reset) folded in. Final verdict: **sound enough to fold into the
  spec.**
