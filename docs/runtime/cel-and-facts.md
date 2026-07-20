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
| `{"op": <sym>, "l": <node>, "r": <node>}` | binary; `<sym>` ∈ `&& \|\| == != < <= > >= + - * / in`. |
| `{"cond": <node>, "then": <node>, "else": <node>}` | ternary. |
| `{"list": [<node>, …]}` | list literal. |
| `{"isSet": "<path>"}` | the `isSet(path)` extension — true iff an effective value exists. |
| `{"has": "<path>"}` | the `has(path)` macro. |

The profile is closed (dsl §8.4): no `null`/bytes literals, no maps/structs,
no comprehensions, no calls beyond `isSet`/`has` and the fact-query functions
below. Boolean composition follows Kleene short-circuit for `&&`/`||` — an
engine evaluating over partially-unknown state should mirror this so an unknown
operand does not force a spurious verdict.

### Fact-query functions in a guard

A guard MAY read the fact store through `holds(...)`, `count(...)`, and
`validAt(...)` (dsl §8). These are the *only* fact-touching functions allowed
in a scalar guard, and they read **"now"** (except `validAt`, a point-in-time
query). The checker forbids them inside a *rule-body* guard
(`E-DATALOG-GUARD-FACT`) and forbids `validAt` over a guard-tainted derived
relation (`E-VALIDAT-DERIVED`, `cel_resolve.rs`) — so any such call that
survives into the artifact is well-formed for the engine to evaluate.

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
`body` of literals — never evaluated by Lute. The engine runs the
**least-fixpoint** over `seedFacts` ∪ asserted facts ∪ `rules`, deriving every
`derive: true` relation (`RelationEntry.derive`). A rule body literal
(`ir.rs::BodyEntry`) is one of:

- `{"kind": "atom", "atom": …, "negated": <bool>}` — a positive or negated
  relation atom;
- `{"kind": "guard", "cel": "…"}` — a CEL guard over ground terms;
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
  unbounded variable reaches the engine.
- **Guard purity.** A rule-body guard may read only scalar state, never
  `holds`/`count`/`validAt`/`now()` — threading a fact query through a guard
  would hide a non-monotonic dependency from the stratification/safety analysis
  (dsl 0.3.0 §7.3, `E-DATALOG-GUARD-FACT`). So a guarded rule stays monotone in
  its facts.

Because negation is stratified and every rule is safe, the least-fixpoint
exists, is unique (the minimal model), and terminates over the finite Herbrand
base. Recomputation policy — full recompute vs. incremental maintenance on each
delta — is the engine's choice; the *result* is fixed by these semantics.

> **Boundary — relational gates are conservatively unproven at compile time.**
> The static reachability/liveness passes reason over rule *structure*
> (`producible()`, `crates/lute-check/src/producible.rs`), never the real
> fixpoint. A fact-query-gated objective or choice yields an **Unknown** verdict
> and rides `W-UNPROVEN-RELATIONAL` / a human-review boundary — the checker will
> not claim a relational gate is satisfiable. The engine's fixpoint is the real
> answer; the compiler only proves the *shape* is well-formed.
