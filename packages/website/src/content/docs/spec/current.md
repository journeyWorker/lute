---
title: Current specification
description: The consolidated index of what the Lute language enforces today at version 0.7.0 — each language area mapped to the versioned proposal that introduced or last changed it, all pointing back to the normative repository sources.
---

The versioned proposal stack under
[`docs/proposals/scenario-dsl/`](https://github.com/journeyWorker/lute/tree/main/docs/proposals/scenario-dsl)
**remains the normative source of truth**. This page does not replace it — it is
the consolidated **index** of what is *current* at language version **0.7.0**:
for each language area, which proposal revision introduced it, which last changed
it, and where to read the normative text.

:::note
Where this index and a proposal disagree, the proposal in the repo wins. For the
full cumulative history (including the pre-implementation `0.0.1` draft and the
capability proposals), see the [specification index](/spec/).
:::

## What is current at 0.7.0

| Language area | Introduced | Last changed | Normative source |
|---|---|---|---|
| Frontmatter & profiles | 0.1.0 | 0.2.0 (document-kind system — `kind: scene`/`quest` polymorphism) | [0.2.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.2.0.md) |
| Content lines (`@speaker` dialogue) | 0.1.0 | 0.5.1 (delivery-flag authoring-surface honesty) | [0.5.1.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.5.1.md) |
| Branch / match / when / hub | 0.1.0 | 0.4.0 (param-scoped component `<match>` dispatch) | [0.4.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.4.0.md) |
| `into=` records (choice run-record sugar) | 0.1.0 (as `persist=`/`into=`, renamed from `0.0.1` `as`) | 0.6.0 (**breaking** — `persist=` removed, `into=` alone records) | [0.6.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.6.0.md) |
| State tiers (scalar `scene`/`run`/`user`/`app`) | 0.1.0 | 0.2.0 (added the `quest.*` state tier) | [0.2.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.2.0.md) |
| Facts & Datalog (relational layer) | 0.3.0 | 0.3.0 | [0.3.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md) |
| Quests (`<quest>`, `<on>` ECA triggers) | 0.2.0 | 0.2.0 | [0.2.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.2.0.md) |
| Timeline & property tracks | 0.1.0 | 0.1.0 | [0.1.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.1.0.md) |
| Connectivity & `after:` sequencing | 0.2.0 (`after:` scene sequencing) | 0.5.0 (reachability-boundary hardening) | [0.5.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.5.0.md) |
| Coverage warnings (`W-UNPROVEN-RELATIONAL`, `W-LUTE-VERSION-STALE`, `W-TRACE-MOCK-UNPRODUCIBLE`) | 0.6.1 | 0.6.1 | [0.6.1.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.6.1.md) |
| Deny promotion (`--deny` / `--deny-warnings`) | 0.6.1 | 0.6.1 | [0.6.1.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.6.1.md) |
| Version stamp & axis alignment | 0.1.0 | 0.7.0 (version unification — language/IR/toolchain aligned at `0.7.0`; byte-for-byte `0.6.1` semantics) | [0.7.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.7.0.md) |

## Notes on the boundaries

- **`0.6.0` is the one breaking revision in the current stack.** It removed the
  `persist=` attribute so `into=` alone drives the choice run-record sugar, and
  made shot headings free text. Pre-`0.6.0` documents carrying a bare `into=`
  (previously a silent no-op) now record.
- **The `0.6.1` coverage warnings are honesty, not errors.** They name the exact
  edge of what static analysis can prove — a relational fact query it can neither
  prove nor refute, a stale `luteVersion` stamp, an unproducible trace mock — and
  never flip the exit code on their own. Promote any of them to an error with
  `--deny <CODE>` or `--deny-warnings`.
- **Design rationale lives alongside the specs.** The four-tier state model's
  *why* is recorded in
  [`state-model-design.md`](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/state-model-design.md),
  a non-normative companion to `0.0.1` §9.
- **Capability surfaces are specified separately.** Character/cast identity and
  the plugin system are capability proposals, not core scenario-DSL revisions —
  see the [specification index](/spec/) for both.
