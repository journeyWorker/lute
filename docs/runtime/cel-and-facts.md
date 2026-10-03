# CEL and facts

The compiler emits standard CEL slots and a separate Datalog fact store. The
engine evaluates both; the compiler never evaluates runtime state.

## Standard CEL, closed profile

Every condition and value slot is standard CEL. `@name` and `$` are expanded
at compile time; `<when is="…">` and `{{…}}` are authoring forms, not CEL.
Rule-body syntax remains separate under **Datalog** below.

Admitted: typed literals (`int`, `double`, `bool`, `string`), lists, state
selects/indexes, `!`, unary `-`, `&&`, `||`, comparisons, arithmetic, `?:`,
`in`, `has(select)`, `int(x)`, `double(x)`, and §Host functions. `uint`, bytes,
`null`, map/struct literals, comprehensions, string methods, `size`, free
identifiers, and every other function or macro are `E-CEL-PROFILE`.

Numeric declarations use only `int` or `double`; `number` is removed.
Arithmetic requires matching numeric types; mixed numeric comparison/equality
is `E-CEL-TYPE`. `int / int` truncates toward zero and `%` is integer-only.
Integer overflow and integer division/modulo by zero are evaluation errors;
IEEE `double` division by zero yields infinity. Defaults must have the declared
type (`0` for int, `0.0` for double).

CEL accepts either quote style. Inside a double-quoted `.lute` attribute, YAML
double-quoted scalar, or `cel("…")` string, CEL strings MUST use single quotes:
`when="holds('inParty', ['elena', '_'])"`.

## Host functions

| Signature | Meaning |
|---|---|
| `holds(string, list(dyn)) -> bool` | current fact matches |
| `count(string, list(dyn)) -> int` | number of matching facts |
| `countDistinct(string, list(dyn), int) -> int` | distinct values at column |
| `validAt(string, list(dyn), int) -> bool` | fact valid at narrative tick |
| `now() -> int` | current narrative-time tick |
| `visited(string) -> bool` | scene presented in this save |

Lists have one element per relation argument: names, booleans, `"_"`, or
`occasion.target`; `countDistinct` requires `"_"` at its column. Relation names
are string literals. Arity/domain checks, `E-DATALOG-GUARD-FACT`, and
`E-VALIDAT-DERIVED` remain in force.

## Environment, activation, and errors

Every execution IR carries `celEnv`, listing exactly the roots and host
functions used. Roots use CEL types such as `map(string, dyn)`; operators,
`has`, `int`, and `double` are implicit. Activation contains nested maps for
declared roots; intermediate maps exist, and slots exist only with an effective
write/seed/default, values retain declared types. Reserved engine paths are
always present with their reserved defaults: for every quest the IR declares,
`quest.<id>.state` and `quest.<id>.failedBy` are `"unset"`; each objective's
`quest.<id>.objectives.<oid>.done` and `.failed` are `false`; and every lore
entry's `entry.<id>.read` and `.everRead` are `false`. `"key" in map` handles
non-identifier keys.

An evaluation error in a condition means not satisfied. A `::set` error halts
without a partial write. CEL short-circuiting applies normally. For a
`<match>` with an unset bare state-path subject, an arm carrying `is` is not
satisfied unless its value is exactly `"unset"`; `is: "unset"` is satisfied.
Other arms evaluate normally. `lute trace` retains its three-valued preview for
unknown state/time; that is a tool view, not engine semantics.

## Datalog: separate rule-body syntax

Rule bodies retain their structured atom/guard/comparison/count syntax.
`cel("…")` guards are CEL strings, but rule-body fact-query restrictions remain.
The engine computes the least fixpoint over seed, asserted, and derived facts;
it does not reinterpret rule syntax as ordinary CEL.
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
  ordinary path (`run.approval.isolde`), each instance's `cel` suffixed
  `[P = isolde]`. The engine never sees an index. A `@def` in a rule guard
  (`cel("@firstDay")`, dsl 0.24 T1-1) arrives **expanded**: the `cel` text is
  the def's parenthesized body, arguments substituted, exactly as any other
  condition's `cel`; an unexpandable one is `E-RULE-GUARD-DEF` at check;
- `{"kind": "cmp", "lhs": …, "rhs": …, "negated": <bool>}` — a term
  comparison;
- `{"kind": "count", "atom": …, "distinct": ["T", …], "op": ">=", "n": 5}`
  (dsl 0.26.0 §6) — authored `count(hasBadge(_)) >= 5` or
  `countDistinct(toured(P, T), T) >= 2`: the number of facts matching
  `atom` (or, with `distinct`, of distinct values of those variables among
  them) compared by `op` (`==`, `!=`, `<`, `<=`, `>`, `>=`) to the whole
  number `n`. A variable of `atom` bound by another literal of the rule is
  read (`P` above: the count is grouped per person); any other ranges over
  the facts. A count binds nothing. `distinct` is omitted for a plain
  `count`.

The compiler's static analyses let the engine trust that this fixpoint is
well-defined:

- **Stratified negation.** The predicate-dependency graph has **no cycle
  through a `not` edge** — the checker runs Tarjan SCC over it and rejects a
  negation cycle as `E-DATALOG-UNSTRATIFIED`
  (`crates/lute-check/src/datalog_check.rs::check_stratification`). A purely
  positive cycle (e.g. `canReach`'s self-recursion) is allowed. The engine
  therefore evaluates **stratum by stratum**, with each negated body literal
  resolved against a strictly lower stratum's completed relation — standard
  stratified-Datalog semantics. A `count` literal is an **aggregate edge**:
  its relation MUST sit in a strictly lower stratum than the rule's head, so
  the count is final before the head is derived. A count whose relation
  depends on the head — directly (`open(D) :- door(D), count(open(_)) >= 1`)
  or through other rules — is `E-RULE-AGGREGATE-CYCLE`.
- **Safety.** Every variable in a rule head or a negated body atom is bound by
  some positive body atom or equality chain — the checker's safety fixpoint
  over variable names guarantees it (`E-DATALOG-UNSAFE`, same file). No
  unbounded variable reaches the engine, with one deliberate exception: an
  authored `_` (dsl 0.24 T3-9) arrives as a variable named `_0`, `_1`, … (a
  name no author can write). In a positive atom it binds like any variable
  and is read nowhere else; in a **negated** atom it is the only variable left
  unbound, and it is **existential** — `not seen(W, _)` holds iff no
  `seen(W, …)` tuple exists at all. `_` never appears in a head or a
  comparison. A `countDistinct` variable MUST be an argument of the counted
  atom and bound nowhere else in the rule (otherwise it has one value) —
  `E-DATALOG-UNSAFE`.
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
>
> `check-project --wip` (dsl 0.23.0 §10, 0.26.0 §2.6) re-decides a dead
> guard against a wider *may* set in which every relation content not yet
> written may still produce is unbounded — one with no seed, assert, rule or
> reserved declaration, or one only a component `::assert` with an unbound
> `@param` writes (`hasBadge(@badge)` before any area `::use`s the
> component). A guard dead only under the narrow set is reported as a
> warning, as is a required `<objective quest=…>` whose child quest is dead
> only for that reason. The engine's semantics are unchanged.
>
> A rule's `count(…)` / `countDistinct(…)` literal is read as satisfiable
> in *may* (the head may hold whenever its other premises may) and never
> guarantees a head in *must*: the compile-time sets over-approximate it.
