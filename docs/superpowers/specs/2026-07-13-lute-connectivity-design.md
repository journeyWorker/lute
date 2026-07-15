# Lute Connectivity Layer — Design Spec

- **Status:** Approved design, ready for implementation.
- **Scope:** scene↔scene and scene↔quest prerequisite/route declarations as a checkable
  authoring contract, plus a per-node available-state envelope analysis. Read-only static
  analysis and a new explain command — no engine/runtime behavior changes beyond an advisory
  IR emission.
- **Relationship to existing specs:** an extension over `scenario-dsl/0.1.0.md` (scene kind,
  shared kernel, state model) and `0.2.0.md` (quest kind, `<quest start/fail>`). Cites section
  numbers from those documents throughout.
- **Revision (2026-07-13, post-implementation amendment):** cycle degradation is now specified
  as **per-node partial recovery** (§4.1) — a cycle no longer blanks reach/envelope for the
  whole project root, only for nodes on or downstream of it — and `compile`/`trace` are now
  **project-aware** (§5), gating on the target document's `check-project` verdict while
  preserving the D1 fact-quarantine. Supersedes the earlier "`trace` — unaffected, no new
  surface" statement.

## 1. Problem

Lute has no vocabulary for **scene↔scene** or **scene↔quest** ordering/prerequisites, and no
way to ask "what state is guaranteed set by the time episode X starts."

Quests already have a declarative-activation predicate (`<quest start=…>`, 0.2.0 §6.3) and
engine-derived, content-readable lifecycle state (`quest.<id>.state`). Scenes have no
equivalent — nothing lets a scene declare its own prerequisite or availability. This asymmetry
is what the connectivity layer closes.

Two established facts bound the design space:

- `scene.*` clears at episode end (one `.lute` doc); `run.*` is the only cross-episode carry
  tier (0.1.0 §9.1). `scene.choices.*` / `scene.visited.<hub>.<choice>` are engine-populated,
  episode-scoped recording namespaces (0.1.0 §9.6, §11.1.3) — no cross-episode analogue exists.
- 0.4.0 §5 reachability (`E-ARM-DEAD`, `E-QUEST-UNREACHABLE`, `E-OBJECTIVE-UNSATISFIABLE`) is
  explicitly local to one construct — "by construction: control flow is forward-only with no
  diverts" (0.4.0 §5.2). Cross-document reachability is a genuinely new analysis, not an
  extension of `decide()`.
- An `<objective done>` gated by a relational fact query (`holds`/`count`/`validAt`) is always
  `Undecided` under the 0.4.0 §5.1 `decide()` fragment (R5) — a genuinely unreachable
  relationally-gated objective passes `check` clean today, and `lute trace` can be fooled by
  mocking a fact that was never producible. This gap is closed as part of §4 below (Analyses,
  Prereq satisfiability), not as a separate feature.

The available-state-envelope ask covers **both** a scene's and a quest's evaluation point, and
has two distinct sides: a **diagnostic** side ("is this read safe?") and a **proactive
inventory** side ("what can I rely on here, before I write anything?"). This spec delivers the
scene half as new graph-positioned machinery (both sides). For quests, the diagnostic side is
already answered by existing, shipped `defassign` machinery; the inventory side is answered by
a new `lute scenario envelope` output available for every quest (defaults-only for quests that
don't declare their own `after`, a full envelope for those that do) — quests are reactive, not
graph-positioned by default, so inferring a graph-envelope from `start=` would be unsound (§4.4
explains why).

`:::route` was removed from the language; scene→scene divert exists only as a reserved,
forward-only, intra-document constraint (`::goto{shot=N}`, 0.1.0 §14), never implemented, and
unrelated to this design — that reservation concerns diverting *within* one document's shot
sequence. This layer is inter-document (episode-to-episode), the same axis `uses:`/`extends:`
already operate on (0.1.0 §9.2, 0.3.0 §4.1).

## 2. Model & vocabulary

### 2.1 The `after` declaration

A scene document declares its prerequisites via a frontmatter key `after:`; a quest declares
its prerequisites via an `after` attribute on its `<quest>` element, sibling to `start`/`fail`
(0.2.0 §6.3):

```yaml
after: 'visited("sofia.ep02") && (completed("sideQuestA") || completed("sideQuestB"))'
```

```lute
<quest id="riverDebt" after="visited('sofia.ep02')" start="…" fail="…">
```

**Placement is asymmetric by design, not by oversight.** A scene document IS exactly one node
(0.1.0: one `.lute` doc = one episode) — frontmatter already carries per-document config
(`uses`/`profile`/`state`), so `after:` fits there directly. A quest document is a pack of ≥1
`<quest>` declarations (0.2.0 §6.2), so a single frontmatter key cannot independently position
multiple quest ids in the same file; an attribute is per-declaration and sits
placement-consistently next to `start`/`fail` on the same element. Both cases share the
**same formula grammar** — only the placement differs, driven by each kind's node-per-file
cardinality.

`after`/`start`/`fail` are three independent attributes on the same `<quest>` tag. This design
does not statically detect a contradiction between a quest's `after` and its `start=` (e.g. an
author writing intent that can't both hold) — doing so would require semantic analysis of
arbitrary `start=` CEL, undecidable territory this design stays out of. The two attributes are
independent; an author-introduced contradiction between them is not caught.

Quest-to-quest and quest-to-episode prereqs already existed before this design —
`<quest start="quest.x.state == 'complete' && …">` (0.2.0 §6.3) already expresses them; quests
are engine-derived and content MUST NOT re-declare completion. The gap this design closes is
**episode nodes**, which had no prerequisite surface at all: `after:` gives scenes what quests
already had via `start`.

### 2.2 The prerequisite profile grammar

`after`'s value is CEL text under a new, maximally-restricted CEL profile — reusing the
existing profile-restriction mechanism `cel_resolve.rs` already implements for the general
Lute-CEL profile (`check_cel_profile`/`is_profile_operator`/`is_profile_fact_query`), not a
bespoke parser and not the general profile (0.1.0 §8.4, which admits full state
reads/comparisons/`holds`/`count`):

```
Formula  ::= "visited(" StringLit ")" | "completed(" StringLit ")"
           | "(" Formula ")" | Formula "&&" Formula | Formula "||" Formula
```

`StringLit` is the existing Lute-CEL string-literal production (§4.4/`STRING_ESCAPE`) — no new
lexer or escaping rules. The grammar admits exactly conjunction and disjunction over two
opaque, monotonic predicates; no negation (§2.5), no arithmetic, no state reads, no arbitrary
CEL. Framing this as a *profile* (an admit-list over the standard CEL parser) rather than a
grammar fork gets `&&`/`||` for free and stays syntax-consistent with the rest of Lute: static
extraction walks the AST for `visited()`/`completed()` call nodes and reads their literal
string args.

**Closure discipline.** A conforming prerequisite-profile checker MUST validate the exact call
shape — func name ∈ {`visited`, `completed`}, exactly one `StringLit` argument for either —
never built as "the general `check_cel_profile` admit-list plus `StringLit` generally
admitted." The grammar is closed by construction: `StringLit` appears in exactly these two
fixed positions, never as a standalone `Formula`, an `&&`/`||` operand, or alongside a
number/bool anywhere. An implementation that instead reuses `check_cel_profile`'s broader "any
literal passes" leaf case wholesale would silently reopen it (e.g. `visited(42)` or a bare
`"x"` slipping through as unintentionally valid) — this needs a narrow, purpose-built
shape-check.

**Scoped by parse context, not by name lookup.** `visited`/`completed` are opaque primitives,
not state paths — scoped only to this one formula grammar, exactly as `$` is scoped only to
`<match>` (0.1.0 §4.4) and fact-query functions are barred from Datalog rule bodies (0.3.0
§7.2). Writing `visited(...)` anywhere outside an `after:`/`<quest after>` slot (an ordinary
`::set` expression, a `<match on>` guard, a Datalog rule guard) is parsed under the general
Lute-CEL profile (0.1.0 §8.4), where `visited`/`completed` are simply unrecognized function
names — rejected as an ordinary unknown-call diagnostic, never silently special-cased. This is
the direct answer to `E-CHOICELOG-READ`'s concern (0.1.0 §9.6/§11.1.2): raw engine-populated
cross-episode state must never be freely branchable, and `visited()`/`completed()` are not a
new `run.seen.*` namespace content can branch on elsewhere.

`completed(questId)` needs no new engine bookkeeping — it reads the same already-specified
`quest.<id>.state` (0.2.0 §5.2/§5.4) through a closed predicate instead of raw CEL. `visited`'s
engine bookkeeping requirement is a function of Enforcement posture (§2.6).

### 2.3 Node identifiers: canonical key, exact lookup

`visited()`'s single string argument holds the project's **canonical episode key** —
identical to the `{character}.{episodeId}` join the compiler already computes for `lineId`
(0.1.0 §12, §6.1). `completed()`'s single string argument is the `<quest id>` itself, already
project-unique (0.2.0 §6.3).

**Resolution is exact-lookup, never decomposition.** At `check-project` time, the tooling
walks every scene document's frontmatter and computes each episode's canonical key (the same
join `lineId` derivation performs), assembling a project-wide **set** of known keys.
`visited(K)` resolves by testing whether the literal string `K` the author wrote is a member of
that set — a single string-equality lookup. The checker never parses, splits, or
reverse-engineers a key into its `(character, episodeId)` components. Neither `character` nor
`episodeId` carries any charset constraint (0.1.0 §6.1 defines `episodeId` as "stable opaque";
`character-cast/0.0.1.md` §3/§11 treats `character` as identity only, no format rule), so a
component-splitting design would need to reserve `.` as a separator without any spec grounding
for doing so. Exact-lookup against a project-computed key set sidesteps the question entirely:
matching never needs to know whether `.` is safe inside either component.

`completed(questId)` uses the same discipline: exact string match against the project's set of
declared `<quest id>` values.

**Matching is exact string equality**, on the decoded string value (after ordinary CEL
string-literal unescaping is applied — never compared as raw source text), against the
project-computed key. No canonicalization, no case-folding, no decomposition. An author's
literal string matches only when it equals the project's actual computed key; a typo or
wrong-format guess is an ordinary `E-CONN-UNKNOWN-NODE`, never a partial or fuzzy match. The
canonical key is compared byte-for-byte after ordinary YAML+CEL unescaping — the same
discipline every other embedded CEL string literal in the language already carries (every
`start=`/`test=`/`when=` is CEL text sitting inside a YAML or attribute string already); this
is not a new escaping-risk class.

**Uniqueness.** `E-CONN-EPISODE-ID-DUP` is a new project-wide check, parallel in shape to
`E-QUEST-ID-DUP` (`check_project_quest_ids`): it groups scene documents by their computed
canonical key (not the raw `(character, episodeId)` pair) and flags any group with more than
one member. This subsumes two cases in one check: two documents declaring the identical
`(character, episodeId)` pair, and — since neither `character` nor `episodeId` is charset-
restricted — the narrower case of two *different* pairs whose join happens to collide (only
possible if a component itself embeds the `.` separator). No separate rule is needed for the
collision case; grouping by computed key catches it for free, the same way
`check_project_quest_ids`'s "flag groups with >1 member" logic is agnostic to *why* two entries
share a key.

`E-CONN-EPISODE-ID-DUP` MUST use the same per-resolved-project-root grouping
`check_project_quest_ids` already uses (`main.rs`'s `by_root` grouping, root = nearest ancestor
`lute.project.yaml`), never a flat pooled walk of everything `check-project <dir>` traverses —
otherwise it would false-positive on legitimate cross-subproject id reuse (the shipped
`docs/examples/` corpus already reuses `character: demo, season: 1, episode: 1` across
unrelated standalone files, and `character: bianca, season: 1, episode: 1` across two distinct
subprojects — neither is a live collision today only because both sit in different resolved
project roots).

### 2.4 Two distinct graphs from one formula

The checker's provable claims stay graph-structural — never an evaluation of the formula's
runtime truth. Two things rule out reusing CEL/Datalog evaluation for this purpose: (1) 0.3.0
`derive: true` relations are computed only by the engine's fixpoint, and D1 forbids evaluating
them anywhere in the toolchain (0.4.0 §4.2 rules 1–3) — if prereq edges were themselves derived
Datalog facts, `check` could never decide them; (2) R1–R5 (0.4.0 §5.1) only ever prove an
expression false or trivially decided, with no mechanism to prove positive reachability or
reverse-engineer a graph out of arbitrary boolean CEL. Graph algorithms need an actual graph.

Two distinct graphs are derived from the same formula, kept explicitly separate:

1. **Topological-precedence DAG** — flattened, over-approximating: an edge `p → n` for every
   node `p` referenced anywhere in `n`'s formula, regardless of `&&`/`||` position. Used for
   cycle detection (any possible route creating a cycle is a real authoring error, so
   over-approximating is the safe direction) and as the traversal order (topological sort) for
   envelope propagation.
2. **Formula AST — structural recursion, not DNF materialization.** Expanding a formula to
   disjunctive normal form (`(A||B) && (C||D) && …` → 2ⁿ routes) blows up exponentially on
   realistic branchy graphs. Reachability (§4.1/§4.2) and the envelope (§4.3) are instead
   computed as a structural recursion directly over the formula tree, one memoized pass per
   node — linear in formula size, no route enumeration ever happens. This computation is
   provably equal to a hypothetical per-route ∩/∪ computation, by the set-distributivity
   identity `⋂ᵢⱼ(aᵢ∪bⱼ) = (⋂ᵢaᵢ) ∪ (⋂ⱼbⱼ)` applied over `&&`/`||` nesting — same semantics,
   never materializing the exponential route set.

**The formula's truth value is a genuine runtime question the engine evaluates** — exactly as
it already evaluates `<quest start>` (0.2.0 §6.3); Lute itself never runs it. What the checker
proves is entirely graph-structural: is the declared edge set acyclic, do referenced ids exist,
is there at least one structurally satisfiable route, what does the declared graph guarantee
about state. The checker never claims to know whether a formula is true at a given play
session — only whether the graph it implies is well-formed, and what follows if the graph
accurately models play (the same trust `E-QUEST-UNREACHABLE` already extends to the schema,
applied one level up to the narrative graph).

### 2.5 Negation is out of scope for v1

`!` is excluded from the prerequisite-profile grammar entirely. Negation's natural semantics is
**mutual exclusion** — a legitimate pattern (node `A` declares `after: "!visited(B)"`, node `B`
declares `after: "!visited(A)"`; both selectable at the project's start, picking one
narratively excludes the other) — but it is a genuinely different analysis shape than positive
precedence, not a variant of it:

- Folding a negated reference into the same over-approximating edge set as positive references
  is unsound: an exclusion constraint is not a precedence one, and doing so fabricates cycles
  on valid, mutually-exclusive content.
- Under Enforcement posture A/A-hybrid, `!completed(X)` would have zero static meaning and zero
  runtime enforcement — an author writing it expecting a secret-route gate would get a silently
  inert no-op, a footgun rather than a neutral omission.

Negation's proper treatment needs its own state-space/route-combination analysis (e.g. modeling
`A`/`B` above as alternative branches of one exclusive choice-point, not as DAG edges) plus real
runtime enforcement so the operator isn't inert — its own dedicated design pass, tracked as
future work (§8).

### 2.6 Enforcement posture: A-hybrid

The graph is emitted to compiled IR as **advisory data** — the same posture `relations:`/
`rules:` already have. An engine MAY consult it to build real content-gating UI, but is under
no spec obligation to honor it. This is not a normative new engine surface.

**This is a locked decision**, not an open choice per document or per project. The
grammar/checker surface is identical regardless of enforcement posture: the static analyses
(§4.1–§4.4) derive their value entirely from the *declared structure*, never from evaluating
the formula, so nothing about them depends on whether an engine acts on the emitted graph.

**Normative diagnostic-wording requirement.** Because the engine is not bound to honor `after`
under A-hybrid, every §4.2/§4.3/§4.4 diagnostic MUST be worded as "unreachable **under your
declared `after` routes**" / "guaranteed **under your declared routes**" — never as an
unconditional runtime claim. If an engine opens a node early via its own separate unlock
mechanism, a path the envelope calls `Guaranteed` may actually be unset at runtime; omitting
the hedge would misrepresent a declared-route assumption as a runtime guarantee. This wording
requirement is mandatory, not optional phrasing. `E-CONN-UNREACHABLE` (§4.1) is the one
exception that needs no hedge — it is a pure fact about the *authored* graph's
self-consistency (no route exists in what you declared), never a claim about runtime behavior.

**Future promotion path (not part of this design's scope).** A future normative-enforcement
posture — the engine MUST only ever unlock a node once its formula holds, direct precedent:
`<quest start>` already works exactly this way (0.2.0 §6.3) — is a larger instance of the same
D1 pattern `<quest start>`/`fail`/`done` already establish (Lute declares, engine evaluates +
enforces), not imperative goto and not a philosophy violation. For **scenes**, this promotion
is free: the grammar/checker surface never changes, so no existing `after:` formula is
rewritten and no diagnostic code is renamed. For **quests that declare their own `after`**,
promotion additionally requires resolving how the two independent predicates (`after` and
`start`) combine once enforcement is normative — either (i) activation becomes at least
`after_formula && start_formula`, requiring a combination contract for when the two conditions
become true at different times, or (ii) `after` stays a declared-route assumption for quests
even under enforcement, keeping the scene/quest asymmetry permanently. This decision is
deferred; it does not block or affect anything in this design's current scope. Negation is out
of scope even under a future enforcement posture — it needs its own dedicated mutual-exclusion
design pass (§2.5, §8).

## 3. Placement

`after:` lives in each scene's own frontmatter — colocated with `uses`/`profile`/`plugins`/
`state`, the existing per-file config precedent. `check-project` assembles the project-wide
graph by walking every document's frontmatter, the same shape it already uses for global
`<quest id>` uniqueness (`check_project_quest_ids`/`check_project_quest_refs`) — a flat
per-file collector extended to validate declared prereq edges, a bounded, in-character
extension rather than a new command class.

A centralized edge-manifest document (sibling to the state schema) was considered and rejected:
its only advantage — a single whole-graph view — is delivered by `lute scenario` tooling
instead (§5), without paying a second-source-of-truth staleness/duplication cost against
frontmatter's `episodeId`.

## 4. Analyses

### 4.1 §A. Graph well-formedness

Project-wide pass, `check-project`-scoped (new module, modeled on the `uses:` DAG checker,
0.1.0 §9.2), operating on the topological-precedence DAG (§2.4):

- **`E-CONN-UNKNOWN-NODE`** — an `after` formula's `visited(K)`/`completed(K)` string argument
  is not a member of the project's computed key set (mirrors `E-COMPONENT-UNDECLARED`'s
  nearest-match suggestion). Node resolution walks every scene document's frontmatter,
  computes each one's canonical `{character}.{episodeId}` key, and tests `K` for set
  membership by exact string equality.
- **`E-CONN-EPISODE-ID-DUP`** — two scene documents compute the same canonical key (§2.3).
  Parallel construction to `E-QUEST-ID-DUP` (`check_project_quest_ids`), same
  `group_by_id`-style walk, keyed on the computed join string, same per-resolved-project-root
  scoping. Prerequisite of `E-CONN-UNKNOWN-NODE` being sound: without this, `visited()` could
  silently resolve to an ambiguous node.
- **`E-CONN-CYCLE`** — the flattened edge set contains a cycle; the diagnostic prints the chain
  (`uses:` precedent).
- **`E-CONN-UNREACHABLE`** — a node has no satisfiable route from the project's entry set,
  computed by a memoized structural evaluation of the formula AST (not route enumeration):
  `reachable(visited(Y)) = ¬E-CONN-UNREACHABLE(Y)`, `reachable(completed(Q)) =
  ¬E-QUEST-UNREACHABLE(Q)` (existing 0.4.0 §5.3 signal), `reachable(X && Y) = reachable(X) ∧
  reachable(Y)`, `reachable(X || Y) = reachable(X) ∨ reachable(Y)` — each node's reachability
  computed once, memoized, over the topologically-ordered graph: linear, no blowup, no route
  enumeration, no BDD needed (the grammar excludes negation, §2.5, so every formula is
  monotone). Entry-set definition: any node with an absent or empty `after` is, by definition,
  a graph entry point (`reachable = true` trivially) — no separate "declare the start"
  convention needed.
- **`E-CONN-FORMULA-TOO-COMPLEX`** — a defensive cap on a formula's atom count, flagged if some
  pathological input exceeds it. A pragmatic guard, not the primary soundness mechanism — the
  structural recursion above is what actually prevents blowup.

**Cycle degradation is per-node (partial recovery).** `E-CONN-CYCLE` marks a malformed
ordering, but it does NOT blank the reach/envelope analyses for the whole project root.
Reachability and the envelope are computed over the graph's natural topological order (above):
a node enters that order once every prerequisite edge is resolved, which recursively fails for
exactly the cycle members and every node structurally downstream of them. So a node that is
topologically INDEPENDENT of the cycle still receives its full, sound `reach`/`envelope`
verdict; only nodes ON or DOWNSTREAM of a cycle are excluded and reported degraded (`reach` →
`Unknown`; `envelope` → the defaults-only `D` floor plus an explicit cycle note). `lute
scenario`, `reach`, `envelope`, and the `check-project` reconciliation all scope this exclusion
per-node, never per-root.

**Accepted conservative gap.** The `&&`/`||` edge model over-approximates position, so a node
reachable ONLY via an `||` branch that passes through a cyclic node (e.g. `visited(Independent)
|| visited(Cyclic)`) is conservatively reported degraded even though its independent disjunct
could prove reachability. Recovering that requires SCC-condensation-aware, per-disjunct
analysis (§8); until then the checker stays provable-only — a false `Unknown` is always sound,
a false `Reachable`/`Guaranteed` never is.

### 4.2 §B. Prereq satisfiability / node reachability

`E-CONN-UNREACHABLE` (§4.1) is graph-reachability. The relational-objective-liveness gap named
in §1 — an `<objective done>` gated by a relational fact query is always `Undecided` under
`decide()`, so a genuinely unreachable relationally-gated objective passes `check` clean today
— is subsumed as a second-order consequence.

A naive approach (tracing `::assert` sites for the gating relation directly) is unsound: a
`derive: true` relation can never be `::assert`ed at all (`E-DERIVED-WRITE`), so a direct
assert-site search finds an empty set for every derived relation by construction, wrongly
flagging any derived-relation-gated objective as dead. (Concrete counterexample from the
shipped corpus: `docs/examples/quest-rescue-halsin.lute:31` gates
`done="holds(canReach(player,grove))"`; `canReach` is `derive: true` in
`docs/examples/act1.schema.yaml:14`, derived from `atLocation`/`connected`, both seeded
unconditionally via `facts:` — always producible from load, independent of any episode. A
naive assert-site search would falsely kill this shipped, correct example.)

**Algorithm — walk the rule-dependency graph to base relations**, reusing
`datalog_check.rs`'s existing `predicate_edges` extraction (already built for
`E-DATALOG-UNSTRATIFIED`'s Tarjan-SCC pass, 0.3.0 §7.2):

1. Define `producible(R)` recursively over the rule DAG (positive recursion, e.g. a
   self-referencing relation, needs a small monotone least-fixpoint iteration over this boolean
   domain — finite and terminating by the same finite-Herbrand-base argument the real Datalog
   fixpoint relies on, just cheaper: boolean, not fact-set):
   - a **base** relation `R`: `producible(R)` iff `R` has a schema `facts:` seed
     (unconditional), or `R.reserved == true` (engine-populated out-of-band — no author-side
     producer is not a sound impossibility signal), or `R` has an `::assert{R(…)}` site in a
     node that is `E-CONN-UNREACHABLE`-clean.
   - a **derived** relation `R`: `producible(R)` iff any rule clause `R(...) :- B1,…,Bn` has
     every *positive* atom `Bi` producible. Negated atoms and rule-body `cel(…)` guards (0.3.0
     §7.3) are conservatively treated as always-satisfiable — provable-only, never guess,
     mirroring §4's own discipline; never causes a false-positive unreachable claim.
2. **Relation-level, not argument-level** (sound, deliberately incomplete, the same tradeoff
   `W-OVERLAP-ARMS` already makes, 0.4.0 §5.2): if `producible(R) == false`, every ground
   instance of `R` is dead, so any `<objective done>` gated by `holds`/`count`/`validAt` on `R`
   is provably dead too — rides as a third named cause on the existing
   `E-OBJECTIVE-UNSATISFIABLE`/`E-QUEST-UNREACHABLE` family (0.4.0 §5.3, established precedent
   for naming whichever standalone cause holds, not a new shape). If `producible(R) == true`
   because some other argument tuple is reachable, the checker correctly stays silent even when
   the specific ground atom the objective needs is actually dead — sound under-approximation,
   never a false positive.
3. **Declared-route hedge applies here too.** "Dead" above means dead given the declared
   `after` graph — under Enforcement posture A-hybrid, the engine isn't bound to honor that
   graph, so this diagnostic MUST be worded "unreachable under your declared routes," never an
   unconditional "can never happen in play" claim.

This computes entirely without ever running the Datalog fixpoint: `producible()` is a boolean
satisfiability walk over declared rule *structure*, never an evaluation of facts against
runtime state — fully outside the D1 quarantine.

### 4.3 §C. Available-state envelope

Two distinct questions; the envelope answers only the second. (a) DECLARED/legal-to-read —
governed by schema import (`uses:`), already project-global and uninteresting per-node (every
scene importing the schema sees the full vocabulary). (b) **Actually SET** by the time control
reaches node X — this needs a per-node effect summary, not just the prereq graph (the graph
gives order, not writes).

**Scope: `run.*`/`user.*` only, not `quest.<id>.*`.** The envelope algebra assumes writes are
monotonic ("once set, stays set," so union/intersect over predecessors is sound). That holds
for `run.*`/`user.*` (only a full run/profile reset clears them, well outside one run's DAG
walk) but not `quest.<id>.*` scratch fields: 0.2.0 §5.1 lets the engine clear them mid-run on
re-instantiation of a repeatable quest, a clearing point this lattice doesn't model. Quest
lifecycle state doesn't need envelope machinery anyway — "was it reachable at X" is already
answered directly by `completed(<questId>)` membership in the route structure (§4.1/§4.2), a
stronger, simpler signal.

**Effect summary is a graph-lift of `defassign`'s own lattice operations.**
`defassign.rs::walk_nodes` already threads an `Assigned` set forward through one document,
joining `<branch>`/`<match>` arms via `intersect_all` ("assigned only if assigned on every
arm" — a must/guaranteed lattice) and letting `<hub>`/`<on>`/`<objective>` arms fork-and-discard
(their writes are may-only, never folded back). The final `Assigned` set at end-of-document,
filtered to `run.*`/`user.*` paths, is exactly `G(episode)` — already computed by the
whole-document driver (the pass that already drives cross-shot analysis over the
whole-document ordered node stream, since `scene.*` persists across shots), currently thrown
away after producing diagnostics. Exposing it is a small, additive change: a new return value
on the existing pass, no new algorithm.

`P(episode)` (possible-writes, `run.*`/`user.*` only) has no existing analog but needs none: a
flat, path-insensitive scan for "does any `::set`/persist-sugar target this path anywhere in
the document" — cheaper than `defassign` itself (no fork/join tracking at all). `::assert` is
dropped from this scan — it targets relational facts, out of this section's scalar-tier scope.

**Canonical propagation — structural recursion over the formula AST**, one memoized pass per
node, over the topologically-ordered graph:

```
Atom visited(Y):    G = Guaranteed(Y) ∪ G(Y)          P = Possible(Y) ∪ P(Y)
Atom completed(Q):  G = P = writesOnComplete(Q)
X && Y:  G(X && Y) = G(X) ∪ G(Y)      P(X && Y) = P(X) ∪ P(Y)
X || Y:  G(X || Y) = G(X) ∩ G(Y)      P(X || Y) = P(X) ∪ P(Y)
absent/empty after: (entry node)  G = P = D
```

`writesOnComplete(Q)` is the union, across `Q`'s required objectives' bodies plus its
`questComplete` `<on>` handler body, of each body's own guaranteed-write set — each body's set
computed by running the same `defassign` `intersect_all` walk on that body's node stream alone
(a body MAY itself contain `<match>`/`<branch>`, 0.2.0 §6.7's admitted-nodes list, so "the body
ran" does not make every `::set` inside it guaranteed — only writes on every internal path
through that body are). Composition is then: intersect within a body (must), union across
bodies (each fires independently and unconditionally-once on completion, 0.2.0 §6.3/§6.4, so
nothing narrows between them) — the same intersect/union split used throughout this section, at
one finer grain. Optional objectives' bodies are excluded — they need not fire for completion.

This is provably identical to a hypothetical per-route ∩/∪ definition (never fewer/more
diagnostics than route-enumerated computation would produce), by a set-lattice distributivity
identity: for route families `{aᵢ}` of `X` and `{bⱼ}` of `Y`, `X && Y`'s routes are the cross
product `{aᵢ∪bⱼ}` and `X || Y`'s are the plain union `{aᵢ}∪{bⱼ}`; then
`⋂ᵢⱼ(writesG(aᵢ)∪writesG(bⱼ)) = (⋂ᵢwritesG(aᵢ)) ∪ (⋂ⱼwritesG(bⱼ))` — exactly `G(X)∪G(Y)`, by two
applications of `x ∪ (⋂ⱼbⱼ) = ⋂ⱼ(x∪bⱼ)`. The `||`/`Possible` cases follow the same way. This
keeps route-based semantics — including "a path guaranteed on every route, even ones joined by
`||`, ends up in `Guaranteed`" — while never enumerating a route. One topological-order pass
computes both sets for every node; complexity is linear in formula size × graph size.

**Entry base case `D`.** The entry base case is the project-resolved set of `run.*`/`user.*`
paths carrying a schema `default` — reused verbatim from the schema import/merge layer
(`uses:`/`extends:`), never re-derived (cross-schema default conflicts are already that layer's
job). A `run.*`/`user.*` path with a schema default is already seeded/assigned at scene entry
by `defassign`'s own existing rule (`has_default` in `defassign.rs`: "a schema-defaulted read
is seeded at scene entry, no error"), so `D` (not `∅`) at entry nodes is required for §C to
never newly error a file that checks clean standalone. `D ⊆ Guaranteed(n)` at every node `n`,
not just entries: `D` seeds every entry (base case), `∪` never drops a member of a set it's
applied to, and `∩` of two sets that both already contain `D` still contains `D` — so `D`'s
membership survives every operation in the recursion by construction, exactly matching
`defassign`'s own "a defaulted path is always assigned" invariant, now lifted to the whole
graph. This induction depends on one grammar property: every route synthesized from a
non-empty formula has ≥1 real `visited()`/`completed()` atom, because the grammar has no
literal `true`/`false` constant — there is no way to write a vacuously-true atom whose route
would contain zero real members and could fail to carry `D` forward.

**Diagnostic use — reading path `P` at node X:**
- `P ∈ Guaranteed(X)` → no diagnostic. This means "safe assuming your declared `after` routes
  are what actually gets played" (§2.6).
- `P ∉ Possible(X)` → under your declared `after` routes, no route ever sets `P` before `X` —
  **error grade**, ships in `check-project` by default (`E-STATE-MAYBE-UNAVAILABLE`). The
  message MUST carry the "under your declared routes" qualifier verbatim (§2.6).
- `P ∈ Possible(X) \ Guaranteed(X)` → set on some declared route, not all — **warning grade**,
  default-suppressed to `lute scenario envelope`'s explicit output only (not emitted by default
  `check-project`), de-risking noise on a first release without losing the error-grade
  soundness guarantee.

`G` is a near-free lift (expose an existing return value, filtered to two tiers); `P` is
cheaper than `defassign` itself (no path-sensitivity needed). The prereq graph construction
(§4.1: frontmatter grammar + parser, project-wide id/edge assembly, cycle/reachability pass,
the §4.2 producibility walk) is the larger new surface — `check-project` today does flat
per-file collection (quest-id uniqueness), not general graph assembly. Build order: §4.1
(graph assembly + well-formedness) first, then §4.2 (reuses `datalog_check.rs`'s existing
dependency-edge extraction), then §4.3 last (a thin analytical layer on top of the same
topological walk).

### 4.4 §D. Quest-time availability

§4.1–§4.3 are entirely scene-node machinery; quests appear in them only as the `completed(Q)`
atom (a scene reading quest lifecycle state) — there is no graph-positioned envelope for quest
activation or objective-evaluation, and forcing one in would be unsound.

**Why quests can't be forced into the scene graph-envelope model.** A scene is reached via
`after`-declared routes — bounded, positioned, with a well-defined entry in the graph. A quest
is reactive: `<quest start="COND">` activates the first instant `COND` holds (0.2.0 §6.3,
declarative activation), and `COND` is arbitrary Lute-CEL, not the restricted prerequisite
profile. There is no single graph position "where" a quest starts; it could activate as early
as the first evaluated state or arbitrarily deep into a run, depending entirely on what `COND`
reads. Retrofitting `start=` into a graph-positioned model would mean re-expressing arbitrary
quest CEL in the closed `after` profile — a separate, much larger redesign that would also
break every existing 0.2.0 quest document.

**Two separate questions need different tooling:** (a) the **diagnostic** question — "is this
specific guard's reads safe?" — and (b) the **inventory** question — "what can I rely on here,
before I write anything?"

**(a) Diagnostic — already answered, unmodified, by `check_quest_guard_defassign`
(`defassign.rs`).** The shipped implementation already analyzes a quest's `start`/`fail` guard
(and, via the same `check_definite_assignment` entry point, every objective/`<on>`-handler body
inside a quest) starting from an empty assigned set — nothing dominates them; the assigned set
starts empty, exactly like a fresh `check_definite_assignment` call. This is sound for a
reactive/unpositioned construct: since a quest might activate at the earliest possible instant,
nothing beyond schema defaults can be soundly guaranteed at quest-entry. This is reactive — it
flags an unsafe read only after an author has already written a guard; it does not print a
proactive list of what's safe.

**(b) Inventory — answered by extending `lute scenario envelope` to every node, including
quests with no `after`.** The explain command MUST accept a quest id
(`lute scenario envelope quest:<id>`) and print a real, useful table for every quest, not just
ones that declared `after`:
- A quest **with** an `after` attribute → the full `Guaranteed`/`Possible` tables computed
  exactly like a scene's (§4.3, reused unchanged).
- A quest **without** an `after` attribute → `Guaranteed = Possible = D` (the schema-default
  `run.*`/`user.*` set, §4.3's entry base case) — a real, useful "here's what's guaranteed at
  this reactive quest" answer, never an empty or error output — plus a one-line note that
  declaring `after` on the quest would enrich the table beyond defaults-only.

This is a proactive inventory (what a writer consults before authoring a guard), distinct from
and complementary to (a)'s reactive diagnostic (which still fires if a guard reads something
outside whatever the inventory showed was safe).

**Opt-in enrichment via the quest `after` attribute (§2.1, §2.2):** a quest MAY additionally
declare its own `after` — same restricted-CEL formula grammar as scene `after:`, fully
decoupled from `start=`, to get a real graph position and a richer §4.1–§4.3 envelope than
defaults-only. This does not contradict "can't force it" above, because it never infers a
position from `start=`'s arbitrary CEL; the author states the position directly, in a separate,
independently-checkable declaration. A quest with no `after` (the default, especially for
genuinely early/reactive quests) keeps the conservative defaults-only answer above — sound
either way, richer only when the author opts in.

**What this design does not deliver, and why that's sound rather than a gap:** a maximally
*precise* per-quest envelope (e.g. "if this quest's `start` reads
`quest.otherQuest.state == 'complete'`, then `otherQuest`'s own guaranteed writes are also
guaranteed here") would require statically recognizing specific patterns inside arbitrary
`start=` CEL — a genuinely different, larger analysis outside the closed `after`-profile
machinery this spec defines. The existing conservative (empty/defaults-only) answer is sound,
just not maximally precise; enriching it later is future work, not a defect in what ships.

**Scope, precisely:** this design delivers (1) scene-node prerequisite/route graph + envelope —
new; (2) `completed(Q)` as a readable atom inside scene `after` formulas — new, small; (3)
quest→quest / quest→episode prereqs where already expressible via existing `<quest start>`
CEL — not new, already shipped; (4) quest-*time* available-state — answered by the existing
`check_quest_guard_defassign` by default, not a new graph problem; (5) an opt-in richer quest
envelope when a quest declares its own `after` — decoupled from `start=`, reusing §4.1–§4.3
unchanged. It does not deliver an envelope for quest activation inferred from `start=`'s CEL.

## 5. Tooling surface

- **`check` (single-file)** — unchanged in verdict semantics; MAY validate an `after` formula's
  local syntax (well-formed grammar, known predicate shapes) but cannot resolve node existence
  without project context — mirrors how `uses:` resolution already needs project/provider
  context.
- **`check-project` (extended)** — new home for graph well-formedness (§4.1) and node/objective
  reachability (§4.2) diagnostics, including the relational-objective-liveness closure. The
  envelope (§4.3) also feeds a new diagnostic here: error-grade `E-STATE-MAYBE-UNAVAILABLE` by
  default, warning-grade suppressed to `lute scenario envelope`. This follows an existing
  precedent, not a new one: `check-project` already suppresses a per-file diagnostic it can
  prove redundant (`main.rs:611-615` retains a per-file `E-QUEST-ID-DUP` only when the
  project-level pass does not already cover it) — `check-project` already reports fewer
  diagnostics than raw per-file `check` when it has stronger project-wide proof. This
  suppress-only-when-the-project-proves-it-safe pattern applies to the envelope diagnostic
  (`E-STATE-MAYBE-UNAVAILABLE`): suppress-only direction, never newly error a file that checks
  clean standalone. The §4.1/§4.2 project-only errors are a distinct class — they detect
  cross-document faults invisible to single-file `check` and are meant to fire there (§7).
- **`compile` and `trace` — project-aware gate (revises the prior `trace` position).** Both
  commands gate emission on a `check` verdict of the target document. When the document is
  resolved against a project (via `--project <dir>`, see the selection rule below), that gate is the document's
  **`check-project` verdict**: a `run.*`/`user.*` read the connectivity envelope proves
  `Guaranteed` no longer blocks with a standalone `E-MAYBE-UNSET`, and a read absent from
  `Possible` (no declared route sets it) is blocked with the project's error-grade
  `E-STATE-MAYBE-UNAVAILABLE`; a `Possible \ Guaranteed` read (set on some but not all routes)
  stays a default-suppressed warning and does NOT block. This makes a
  connectivity-dependent scene compilable/traceable exactly when the project soundly accepts
  it, closing the gap where a project-valid scene could not preview. The gate is the target's
  reconciled per-document verdict PLUS one graph refusal that can be stricter than a bare
  clean per-document result: a target absent from the sound topological order (on or downstream
  of a cycle, §4.1) is refused even if it declares no read of its own, because its prerequisite
  ordering is unresolvable. The reconciliation is **pure
  graph math over declared structure** — it evaluates no CEL and runs no Datalog,
  `visited()`/`completed()` never enter `trace`'s evaluated subset, and `trace` still takes its
  `--fact`/`--state` mocks unchanged — so the **D1 fact-quarantine is preserved**: graph
  reconciliation is admitted, fact evaluation is not. (This deliberately revises §5's earlier
  "`trace` — unaffected, no new surface" statement: the graph gate is new; the quarantine is
  intact.)
  - **Project-context selection (normative, single-root).** The connectivity root is the SAME
    project that resolves the document's capability snapshot — there is never a second,
    independently discovered root, so one invocation can never interpret the document under two
    different projects. WITH `--project <dir>` the document is project-aware: capabilities AND
    connectivity both resolve against exactly that `<dir>` (`load_project(dir)`, no nested
    nearest-root search — that directory-walk discovery is `check-project`'s alone). WITHOUT
    `--project` the document resolves standalone — core-only capabilities and single-file
    `check`, unchanged from today; there is no separate auto-discovery, so the no-flag path
    keeps its current behaviour. The gate blocks on the TARGET document's reconciled
    diagnostics only: a project-only fault in a SIBLING document (e.g. another scene's
    `E-CONN-UNKNOWN-NODE`) does not block compiling this one — that stays `check-project`'s
    surface — but a fault on the target's own `after`/reads does: an unknown node, an
    unavailable read, OR the target being absent from the sound topological order because it is
    **on or downstream of a cycle** (§4.1). This last case is decided by topological-order
    membership, not by which single file the `E-CONN-CYCLE` diagnostic happens to anchor to, so
    every cycle-involved target is refused (a scene whose prerequisite chain transitively hits a
    cycle has no sound envelope to compile against).
    **Out-of-tree target (normative).** If `--project <dir>` is given but the canonicalized
    target document is NOT within `<dir>`'s recursively-collected `.lute` set, the command
    errors EXPLICITLY (the connectivity gate needs the target to be part of the project) rather
    than silently falling back to a standalone `check` — a silent fallback would mask a
    mistyped path or wrong `--project`. (Capability-only resolution of an out-of-tree document
    is not combined with the project-aware connectivity gate.)
- **New command — `lute scenario`, subcommands `reach` and `envelope`.** A proactive
  availability inventory, not merely a reactive diagnostic — this is the tool that answers
  "what can I rely on here," where `check-project`'s surface is error-only. Project-wide,
  read-only reporting surface for everything §4 computes: `lute scenario` prints the assembled
  node/edge graph (topological layers); `lute scenario reach <nodeId>` reports whether a node
  is reachable and via which route(s); `lute scenario envelope <nodeId>` (or
  `envelope quest:<id>`, §4.4) prints the `Guaranteed`/`Possible` tables — for a scene or an
  `after`-opted-in quest, the full tables (§4.3); for a quest with no `after` attribute, the
  defaults-only `D` table (never empty, never an error) plus a one-line note that adding
  `after` would enrich it. This command needs no D1 quarantine — it evaluates no CEL, runs no
  Datalog, takes no mocks; it is pure graph math over declared structure, closer in spirit to
  `check`'s soundness bar than to `trace`'s "coverage, never proof" posture. Diagnostics
  themselves live in `check-project` (pass/fail); `lute scenario` is the explain/inventory
  companion.

## 6. Diagnostics table

| Code | Surface | Grade | Description |
|---|---|---|---|
| `E-CONN-UNKNOWN-NODE` | `check-project` | error | `after` formula references a canonical key/quest id not in the project's computed key set (§4.1). |
| `E-CONN-EPISODE-ID-DUP` | `check-project` | error | Two scene documents (in the same resolved project root) compute the same canonical episode key (§2.3, §4.1). |
| `E-CONN-CYCLE` | `check-project` | error | The topological-precedence DAG contains a cycle (§4.1). |
| `E-CONN-UNREACHABLE` | `check-project` | error | A node has no satisfiable route from the project's entry set, under the declared `after` graph (§4.1). |
| `E-OBJECTIVE-UNSATISFIABLE` / `E-QUEST-UNREACHABLE` (extended) | `check-project` | error | An objective's relational-fact gate is provably dead — a third named cause added to the existing family (§4.2). |
| `E-CONN-FORMULA-TOO-COMPLEX` | `check-project` | error | Defensive cap on a formula's atom count (§4.1). |
| `E-STATE-MAYBE-UNAVAILABLE` | `check-project` | error (default) | A read at node X is not in `Possible(X)` under the declared `after` graph (§4.3). |
| (`Possible \ Guaranteed`) | `lute scenario envelope` | warning (suppressed by default) | A read at node X is set on some but not all declared routes (§4.3). |

## 7. Testing approach

- **Corpus grounding.** Every new diagnostic MUST be validated against the shipped
  `docs/examples/` corpus before shipping: `E-CONN-EPISODE-ID-DUP`'s project-root scoping in
  particular must not false-positive on the corpus's existing cross-subproject id reuse
  (`character: demo`/`bianca` recurring across unrelated standalone files and separate
  subprojects).
- **Soundness invariant tests (envelope class only).** The `E-STATE-MAYBE-UNAVAILABLE`
  envelope diagnostic (§4.3) MUST never newly error a file that single-file `check` reports
  clean standalone — a suppress-only-when-the-project-proves-it-safe invariant, asserted by a
  regression suite, mirroring the existing `E-QUEST-ID-DUP` suppression precedent
  (`main.rs:611-615`). This invariant applies ONLY to the envelope/`defassign`-derived class.
  The §4.1/§4.2 **project-only** diagnostics (`E-CONN-UNKNOWN-NODE`, `E-CONN-CYCLE`,
  `E-CONN-EPISODE-ID-DUP`, `E-CONN-UNREACHABLE`, and the relational-objective-liveness cause)
  are explicitly exempt: they legitimately fire on a file that is clean under single-file
  `check`, because they detect genuine cross-document errors per-file `check` structurally
  cannot see (§5) — that is precisely their purpose, not a soundness violation.
- **Algebraic identity tests.** The §4.3 canonical propagation's equivalence to a hypothetical
  per-route ∩/∪ computation is a provable identity (§4.3) — cover it with property-based /
  randomized tests generating formula ASTs of varying `&&`/`||` nesting and comparing the
  structural-recursion result against a brute-force per-route enumeration on small graphs,
  alongside the closed-form induction proof already given.
- **Unit coverage per diagnostic**, mirroring the existing `check_project_quest_ids`/
  `check_project_quest_refs` test patterns: unknown-node references, cycles (2-node and
  N-node), unreachable nodes behind an unsatisfiable formula, canonical-key collisions (both
  identical-pair and cross-pair), and the relational-objective-liveness closure (must not
  false-positive on `derive: true` relations seeded via `facts:`, the
  `quest-rescue-halsin.lute`/`act1.schema.yaml` shipped example being the canonical
  regression case).
- **Enforcement-posture wording tests.** Every §4.2/§4.3 diagnostic message MUST be asserted to
  carry the "under your declared routes" qualifier verbatim (§2.6) — a lint-level test over
  diagnostic message templates, not just behavior.
- **Quest-envelope defaults-only path.** `lute scenario envelope quest:<id>` for a quest with
  no `after` attribute MUST be tested to return the non-empty `D` table plus the enrichment
  note, never an empty or error result (§4.4).

## 8. Future work

- **Negation / mutual exclusion (§2.5).** A dedicated design pass for expressing mutually
  exclusive branch-and-lock authoring patterns, with its own state-space/route-combination
  analysis (not a DAG-edge extension) and real runtime enforcement so the operator is not
  inert. Out of scope for this design; not part of the Enforcement-posture decision.
- **Enforcement posture promotion to normative (§2.6).** Promoting from A-hybrid to a
  normatively-enforced posture is free for scenes (zero grammar/checker churn) but requires
  resolving the quest `after`/`start` combination contract first for quests that opted into
  their own `after` — deferred, tracked as a named follow-up when enforcement is actually
  adopted.
- **Precise per-quest envelope from `start=` pattern recognition (§4.4).** Statically
  recognizing specific patterns inside arbitrary `start=` CEL (e.g. a quest-completion
  dependency) to enrich the conservative defaults-only quest envelope — a genuinely larger
  analysis outside the closed `after`-profile machinery, explicitly deferred.
- **Warning-grade envelope diagnostic promotion (§4.3).** Revisit promoting
  `Possible \ Guaranteed` from a default-suppressed `lute scenario envelope` warning to a
  default `check-project` warning once real corpus signal exists.
- **SCC-condensation-aware reachability through cyclic disjuncts (§4.1).** The per-node cycle
  recovery conservatively excludes any node reachable only via an `||` branch that passes
  through a cyclic node, even when an independent disjunct could prove it reachable. A
  per-disjunct, SCC-condensation analysis would recover these; deferred as an accepted
  conservative gap — a false `Unknown` is sound, so the current under-approximation ships
  safely.
