# Declarative rewards — `<reward/>` design

Date: 2026-09-01
Status: approved design, pre-implementation

## Problem

Quest conditions are first-class, statically checked language surface
(`start=`/`fail=`/`done=`/`when=`/`after=`, CEL + fact queries, reachability
and satisfiability diagnostics). Rewards are not: the only idiom is
operational — `<on event="questComplete">` bodies carrying plugin `::grant`
directives and `::set` writes. That executes correctly (exactly-once,
quest-scoped since 0.14.0) but the artifact carries rewards only as commands
inside a handler, so nothing can read "this quest's rewards" as data:

- no journal-UI reward preview at accept time;
- no balancing extraction across a project;
- no reward-shaped lints;
- conditional rewards run but are equally invisible.

0.2.0 §Out-of-scope explicitly deferred reward kinds to a follow-on that
never shipped. The adoption assessment mapped 1,759 `quest_rewards` rows
through the handler idiom and rated the fit high — the *data* half is the
missing piece, not execution.

## Decision summary

| Question | Decision |
| --- | --- |
| Surface | New self-closing element `<reward … />`. An element, not a `::`-directive: `::`-forms are commands in flow, elements are declarations — `<reward/>` sits beside `<objective/>`. One-physical-line rule applies. |
| Attachment | Direct child of `<quest>` (granted at the `→ complete` transition; `on="failed"` selects the `→ failed` transition instead) and of `<objective>` (granted when that objective becomes done). `on=` on an objective-level reward is an error. |
| Semantics | Pure data. Lowered to `QuestEntry.rewards` / `ObjectiveEntry.rewards`; the ENGINE grants at the transition (engine rule in quest-lifecycle.md), exactly once per instance, mirroring objective-body monotonicity. NOT synthesized into lifecycle handlers — `::grant` is plugin vocabulary the language must not depend on. The handler idiom stays for narrative staging. |
| Attributes | `kind` (required, non-empty string) · `target` (optional id string) · `amount` (optional; integer scalar, negative allowed, or range literal `N..M` with `N <= M`; default 1) · `when` (optional CelString, evaluated at the grant instant, full existing CEL checking) · `on` (`complete` \| `failed`, quest-level only, default `complete`). |
| Random quantity | The range literal IS the declaration ("1..5 shards" is journal/balancing data); the roll is the engine's, per the 0.0.1 dice contract. IR emits `amount` for a scalar, `amountMin`/`amountMax` for a range — exactly one form present, no polymorphic parsing. |
| Vocabulary | Language owns the shape only. A plugin manifest MAY declare `rewardKinds:` (kind → optional target provider domain + optional extra attr schema); when a resolved plugin declares them, `kind`/`target` are statically validated (`E-REWARD-KIND`, provider snapshot id check — the same class of validation `::grant` already gets from directive attr schemas). No declaration → shape checks only. |
| Reference runtime | `lute run`/`play`/`trace` emit a reward-grant transcript event at the transition (kind/target/resolved-or-range amount, `when` verdict), so `lute test` can assert grants. State/inventory mutation stays with real engines. |
| Version | New element + static semantics + IR shape → **0.16.0** (language & IR minor). Parser/attr-closure move; tree-sitter/capabilityVersion impact verified at implementation (tag enumeration TBD by survey). |

## Diagnostics

| Code | Grade | Meaning |
| --- | --- | --- |
| `E-REWARD-ATTR` | error | shape violation: empty `kind`, non-integer/malformed `amount` (bad range, `N > M`), `on=` on an objective-level reward, unknown `on` value |
| `E-REWARD-KIND` | error | a resolved plugin declares `rewardKinds:` and the reward's `kind` (or its `target` against the kind's provider domain) does not validate |
| existing CEL set | — | `when=` inherits `E-CEL-PROFILE`, `E-MAYBE-UNSET`, unset-comparison guards — same registry as every other CEL slot |
| existing attr closure | — | unknown attributes on `<reward>` are `E-UNKNOWN-ATTR` via the per-tag table |

## Non-goals (named honestly)

- **Drop chance** (`30%` grant probability): display/expected-value contracts
  vary per game; a game declares it as a plugin attr (e.g. `chance=`) and its
  engine interprets it. The language never reads it.
- Double-grant detection (declarative + handler `::grant` of the same kind):
  plugin semantics are opaque to the checker.
- "Quest without rewards" lint: belongs to the lint layer as a configurable
  rule, later, for free.
- Reward/achievement/daily document *kinds* (0.2.0's deferred list stays
  deferred; this closes only the reward-data gap on the existing quest kind).

## Grounding

- oshiz catalog: `quest_rewards` 1,759 rows, unified `{rewardKind, target,
  amount, min, max}`, 16 kinds in use, negative amounts real (ITEM −5
  deduction) — `amount`/`amountMin`/`amountMax` cover the shape verbatim.
- `*_AFTER`-style conditional grants ride on `when=` with `activatedAt`
  (0.8.0) unchanged.
