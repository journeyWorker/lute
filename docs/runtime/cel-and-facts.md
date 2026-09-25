# CEL guards and the fact store

The engine owns **two evaluation surfaces** the compiler deliberately never
touches (design decision D1): CEL guards, and the Datalog fact store. Lute
emits both as data; the engine is the sole evaluator.

## CEL guards at runtime

Every guard slot in the artifact carries a `{raw, expr}` pair
(`ir.rs::CelPair`, and the flattened `when`/`expr` on choice/hub options and
match arms):

- `raw` — the verbatim source CEL, for debug/provenance.
- `expr` — a portable expression AST (IR A7, `crates/lute-compile/src/expr.rs`)
  a plain JSON walker can evaluate with **no CEL parser**. It is **absent**
  whenever the slot was empty or fell outside the closed Lute-CEL profile
  (dsl §8.4); an engine that hits an absent `expr` must fall back to its own
  CEL evaluation of `raw`, or treat the guard as unknown.

`expr` is an untagged union distinguished by its keys (see the schema's
`exprNode`):

| shape | meaning |
| ----- | ------- |
| `{"lit": <number\|bool\|string>}` | scalar literal (all numbers are doubles). |
| `{"path": "a.b.c"}` | a state-path read. |
| `{"op": "!"\|"-", "l": <node>}` | unary negation / minus. |
| `{"op": <sym>, "l": <node>, "r": <node>}` | binary; `<sym>` ∈ `&& \|\| == != < <= > >= + - * / % in`. |
| `{"cond": <node>, "then": <node>, "else": <node>}` | ternary. |
| `{"list": [<node>, …]}` | list literal. |
| `{"isSet": "<path>"}` | the `isSet(path)` extension — true iff an effective value exists. |
| `{"has": "<path>"}` | the `has(path)` macro. |

The profile is closed (dsl §8.4): no `null`/bytes literals, no maps/structs,
no comprehensions, no calls beyond `isSet`/`has`, the fact-query functions,
and `visited` below. Boolean composition follows Kleene short-circuit for `&&`/`||` — an
engine evaluating over partially-unknown state should mirror this so an unknown
operand does not force a spurious verdict.

### Integer `%`

`%` (dsl 0.24.0 §1) is the **integer** remainder. The checker requires both
operands to be integers: a non-number operand or a fractional literal
(`run.day % 2.5`) is `E-CEL-TYPE`. A number path cannot be proven integral
statically, so the engine owns the rest of the rule:

- both operands integral and the divisor non-zero → the **truncated**
  remainder, which takes the sign of the dividend (CEL's `int % int`, Rust's
  `%` on integers): `-7 % 3 == -1`, `7 % -3 == 1`, and `-4 % 2 == 0` (not
  `-0`);
- a fractional operand or a zero divisor → **unknown**, never a float
  remainder. An enclosing guard follows the usual unknown rules.

`lute trace`, `lute test`, `lute play` and the condition decider all apply
this rule (`lute_check::apply_op`, `runner.rs`).

### Fact-query functions in a guard

A guard MAY read the fact store through `holds(...)`, `count(...)`,
`countDistinct(...)`, and `validAt(...)` (dsl §8). These are the *only*
fact-touching functions allowed in a scalar guard, and they read **"now"**
(except `validAt`, a point-in-time query). The checker forbids them inside a
*rule-body* guard (`E-DATALOG-GUARD-FACT`) and forbids `validAt` over a
guard-tainted derived relation (`E-VALIDAT-DERIVED`, `cel_resolve.rs`) — so
any such call that survives into the artifact is well-formed for the engine
to evaluate.

`countDistinct(<pattern>, <Var>)` (dsl 0.24 T3-9) is the number of
**distinct values** at the pattern position the capitalised `<Var>` names,
among the facts matching the pattern with that position read as `_`:
`countDistinct(sawAt(W, _, _, _), W)` counts witnesses, where
`count(sawAt(_, _, _, _))` counts tuples. `<Var>` names exactly one position;
every other position is ground or `_`, as in `count`. A relation cannot be
named after a CEL call, macro or keyword (`has`, `holds`, `count`, …) —
`E-RELATION-RESERVED-NAME` — since a query over it could not be written.

### `visited()` in a guard

`visited('<scene id>')` (dsl 0.21.0 §7a.1) is legal in every condition slot —
quest `start` / `fail`, objective `done`, beat and entry `when`, and
content-line, choice, and match guards. It is true once the scene whose
`meta.id` is the argument has been **presented in this save**: the same
visited set a scene's `after:` prerequisite reads (`prereqEdges`,
`quest-lifecycle.md`), which a new run does not clear. The argument is one
string literal; `check-project` rejects an id that names no scene in the
project (`E-CONN-UNKNOWN-NODE`).

Like a fact query, `visited(…)` is outside the portable `expr` profile: a slot
that calls it carries `raw` only, and the engine evaluates it from the raw
text. Nothing new is stored — the engine already keeps the visited set for
`after:`; a guard only reads it.

## The fact store

Facts are ground tuples over the merged relational vocabulary
(`entities` / `enums` / `relations` / `seedFacts` / `rules`). The engine
maintains a fact store; the artifact drives it with:

- **`seedFacts`** — the initial ground tuples (dsl 0.3.0 §4), loaded at start.
- **`assert` commands** — a positive delta: add `relation(args)` to the store
  (`ir.rs::AssertCmd`; args are ground literals, bools as `"true"`/`"false"`,
  never `"_"`).
- **`retract` commands** — a negative delta: remove matches of
  `relation(args)` where `"_"` positions are a bulk wildcard the engine
  resolves (`ir.rs::RetractCmd`, §5 RetractPattern).
- **engine assertions** — a `reserved: true` relation (`RelationEntry.reserved`)
  is the engine's own, the fact-store counterpart of an `owner: engine` state
  path (`state-lifecycle.md`): the checker rejects content's `::assert` /
  `::retract` of it (`E-RELATION-RESERVED-WRITE`), so no `assert` / `retract`
  record in a checked artifact targets one, and its facts are its seeds plus
  whatever the engine establishes.

Deltas are **valid-now** — Lute emits no timestamps. The DSL's temporal model
(dsl 0.3.0 §6) keys each fact to **narrative time**: the engine stamps
`established` / `invalidated` positions from a strictly-monotonic narrative-time
token advanced one tick per command-stream delta, so every assert/retract is
totally ordered. When a relation declares a functional `key` (`RelationEntry.key`,
0-based arg indices), a new tuple agreeing on the key auto-invalidates the prior
one — the superseded and superseding fact never share an open interval (§4/§5).
`validAt(fact, t)` queries this history; `holds`/`count` query the current
store.

## Datalog: the engine computes the minimal model

`rules: RuleEntry[]` are emitted as **structured data** — a `head` atom plus a
`body` of literals — never evaluated by the compiler. The engine runs the
**least-fixpoint** over `seedFacts` ∪ asserted facts ∪ `rules`, deriving every
`derive: true` relation (`RelationEntry.derive`). A rule body literal
(`ir.rs::BodyEntry`) is one of:

- `{"kind": "atom", "atom": …, "negated": <bool>}` — a positive or negated
  relation atom. An atom whose `relation` names an **entity kind** (an
  `entities[]` entry, `K(x)` — kinds and relations share one predicate
  namespace, `E-KIND-NAME-CLASH`) is a **membership test**, never a fact
  lookup: `K(x)` holds iff `x` is one of the kind's `members`, so a positive
  `companion(P)` binds `P` to each member in turn. No fact is ever asserted
  under a kind's name. A sub-kind (`subsetOf:`, dsl 0.24.0 §3) arrives as an
  ordinary closed kind whose `members` are also listed by its parent. An
  `open` kind's members are the engine's registry;
- `{"kind": "guard", "cel": "…"}` — a CEL guard over ground terms. A rule
  authored with an entity-indexed read (`cel("run.approval[P] >= 3")`, dsl
  0.24.0 §3) arrives **grounded**: one `RuleEntry` per member of `P`'s index
  kind, `P` replaced by the member in every atom and the read by the member's
  ordinary path (`run.approval.isolde`), each instance's `raw` suffixed
  `[P = isolde]`. The engine never sees an index. A `@def` in a rule guard
  (`cel("@firstDay")`, dsl 0.24 T1-1) arrives **expanded**: the `cel` text is
  the def's parenthesized body, arguments substituted, exactly as any other
  condition's `raw`; an unexpandable one is `E-RULE-GUARD-DEF` at check;
- `{"kind": "cmp", "lhs": …, "rhs": …, "negated": <bool>}` — a term
  comparison.

The compiler's static analyses let the engine trust that this fixpoint is
well-defined:

- **Stratified negation.** The predicate-dependency graph has **no cycle
  through a `not` edge** — the checker runs Tarjan SCC over it and rejects a
  negation cycle as `E-DATALOG-UNSTRATIFIED`
  (`crates/lute-check/src/datalog_check.rs::check_stratification`). A purely
  positive cycle (e.g. `canReach`'s self-recursion) is allowed. The engine
  therefore evaluates **stratum by stratum**, with each negated body literal
  resolved against a strictly lower stratum's completed relation — standard
  stratified-Datalog semantics.
- **Safety.** Every variable in a rule head or a negated body atom is bound by
  some positive body atom or equality chain — the checker's safety fixpoint
  over variable names guarantees it (`E-DATALOG-UNSAFE`, same file). No
  unbounded variable reaches the engine, with one deliberate exception: an
  authored `_` (dsl 0.24 T3-9) arrives as a variable named `_0`, `_1`, … (a
  name no author can write). In a positive atom it binds like any variable
  and is read nowhere else; in a **negated** atom it is the only variable left
  unbound, and it is **existential** — `not seen(W, _)` holds iff no
  `seen(W, …)` tuple exists at all. `_` never appears in a head or a
  comparison.
- **Guard purity.** A rule-body guard may read only scalar state, never
  `holds`/`count`/`countDistinct`/`validAt`/`now()` — threading a fact query
  through a guard would hide a non-monotonic dependency from the
  stratification/safety analysis (dsl 0.3.0 §7.3, `E-DATALOG-GUARD-FACT`,
  checked on the def-expanded text). So a guarded rule stays monotone in its
  facts.
- **An undecided guard is not "false".** When a rule guard evaluates to
  neither true nor false (it reads a path with no value), the rule instance
  derives nothing and its head relation — with every relation fed by it — is
  **undecided**, not empty. The reference runner treats a query over an
  undecided relation as unknown: `lute play` halts at the guard that asked,
  naming the state that would decide it, and `lute trace`/`lute test` report
  it unresolved. An engine SHOULD do likewise rather than read the relation
  as holding no facts.

Because negation is stratified and every rule is safe, the least-fixpoint
exists, is unique (the minimal model), and terminates over the finite Herbrand
base. Recomputation policy — full recompute vs. incremental maintenance on each
delta — is the engine's choice; the *result* is fixed by these semantics.

The toolchain implements these semantics once, in `lute_trace::datalog`: the
reference runner (`lute run` / `lute play`) and, since dsl 0.22.0 §6,
`lute trace` / `lute test` compute this fixpoint over the seed facts, the
mocked and asserted facts, and the rules, so none of them can disagree with the
others about a rule's conclusion. That makes the runner a reference to check an
engine's evaluator against: `lute play --explain <atom>` prints the derivation
it found for a ground atom at the end of a playthrough — the rule and each
premise's support — or, when the atom does not hold, every rule that could
conclude it with its failing premises.

> **Boundary — relational gates are decided conservatively at compile time.**
> Since dsl 0.20.0, `check-project` decides every `holds(…)`/`count(…)` in a
> guard from two sets (`crates/lute-check/src/fact_env.rs`,
> `fact_must.rs`): the project-wide **may** set — every ground fact any seed,
> assert, rule, or reserved relation can produce — and the path-sensitive
> **must** set — the monotone facts that hold on every declared route to the
> guard. A query outside *may* is **impossible** (the guard is reported through
> its slot's dead-code error: `E-ARM-DEAD`, `E-ENTRY-UNREACHABLE`,
> `E-OBJECTIVE-UNSATISFIABLE`, `E-QUEST-UNREACHABLE`); a query inside *must* is
> **guaranteed** (`W-FACT-GUARANTEED` on a redundant guard); everything else is
> **possible** and silent. Both proofs are sound, but *possible* is not a
> promise either way: the analysis never enumerates runs, so a guard it cannot
> separate stays undecided, and single-file `lute check` leaves every
> relational query undecided (a sibling document's asserts are invisible to
> it). `W-UNPROVEN-RELATIONAL`, which marked every relational gate "not
> proven", was removed with that release. The engine's fixpoint over the live
> fact store remains the only real answer at run time; the compiler proves the
> *shape* is well-formed and reports the gates it can prove dead or redundant.
