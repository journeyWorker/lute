---
title: Current specification
description: The consolidated index of what the Lute language enforces today at version 0.8.0 — each language area mapped to the versioned proposal that introduced or last changed it, all pointing back to the normative repository sources.
---

The versioned proposal stack under
[`docs/proposals/scenario-dsl/`](https://github.com/journeyWorker/lute/tree/main/docs/proposals/scenario-dsl)
**remains the normative source of truth**. This page does not replace it — it is
the consolidated **index** of what is *current* at language version **0.8.0**:
for each language area, which proposal revision introduced it, which last changed
it, and where to read the normative text.

:::note
Where this index and a proposal disagree, the proposal in the repo wins. For the
full cumulative history (including the pre-implementation `0.0.1` draft and the
capability proposals), see the [specification index](/spec/).
:::

## What is current at 0.8.0

| Language area | Introduced | Last changed | Normative source |
|---|---|---|---|
| Frontmatter & profiles | 0.1.0 | 0.2.0 (document-kind system — `kind: scene`/`quest` polymorphism) | [0.2.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.2.0.md) |
| Content lines (`@speaker` dialogue) | 0.1.0 | 0.5.1 (delivery-flag authoring-surface honesty) | [0.5.1.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.5.1.md) |
| Core directives (the closed `lute.core` vocabulary) | 0.1.0 | 0.8.0 (`::end{reason?}` — the ninth core directive, terminating the walk) | [0.8.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.8.0.md) |
| Branch / match / when / hub | 0.1.0 | 0.4.0 (param-scoped component `<match>` dispatch) | [0.4.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.4.0.md) |
| `into=` records (choice run-record sugar) | 0.1.0 (as `persist=`/`into=`, renamed from `0.0.1` `as`) | 0.6.0 (**breaking** — `persist=` removed, `into=` alone records) | [0.6.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.6.0.md) |
| State tiers (scalar `scene`/`run`/`user`/`app`) | 0.1.0 | 0.8.0 (author `state:` is scalar-only, now enforced — `E-STATE-COLLECTION`) | [0.8.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.8.0.md) |
| Facts & Datalog (relational layer) | 0.3.0 | 0.3.0 | [0.3.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md) |
| Quests (`<quest>`, `<on>` ECA triggers) | 0.2.0 | 0.8.0 (`quest.<id>.activatedAt` — a reserved `narrativeTime` anchor for `validAt`) | [0.8.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.8.0.md) |
| Timeline & property tracks | 0.1.0 | 0.1.0 | [0.1.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.1.0.md) |
| Connectivity & `after:` sequencing | 0.2.0 (`after:` scene sequencing) | 0.8.0 (`active("questId")` — the third prerequisite primitive) | [0.8.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.8.0.md) |
| Identity & localization (`lineId` / `voiceKey`, locale texts) | 0.1.0 | 0.8.0 (`identity:` templates; the `loc import` → `compile --locales` round trip) | [0.8.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.8.0.md) |
| Compiled artifact shape (`addr` addressing, IR carriers) | 0.1.0 | 0.8.0 (uniform `addr` width per artifact; the `end` command kind; `shots`, `texts`, `labels`) | [0.8.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.8.0.md) |
| Warning-severity diagnostics (`W-UNPROVEN-RELATIONAL`, `W-LUTE-VERSION-STALE`, `W-TRACE-MOCK-UNPRODUCIBLE`, `W-CODE-AFTER-END`, `W-L10N-MISSING`) | 0.6.1 | 0.8.0 (`W-CODE-AFTER-END`, `W-L10N-MISSING`) | [0.8.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.8.0.md) |
| Deny promotion (`--deny` / `--deny-warnings`) | 0.6.1 | 0.6.1 | [0.6.1.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.6.1.md) |
| Version stamp & axis alignment | 0.1.0 | 0.8.0 (language, IR, and toolchain at `0.8.0`; the IR schema becomes `lute-ir-0.8.schema.json`) | [0.8.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.8.0.md) |

## Notes on the boundaries

- **`0.6.0` is the one breaking grammar revision in the current stack.** It
  removed the `persist=` attribute so `into=` alone drives the choice run-record
  sugar, and made shot headings free text. Pre-`0.6.0` documents carrying a bare
  `into=` (previously a silent no-op) now record.
- **`0.8.0` — the adoption release — is backward compatible in the grammar, but
  it is not a pure addition.** Every item in it traces to a gap found assessing
  Lute against a real 777-scene / 583-quest game catalog, and two of those items
  have edges worth naming. The IR **gains a command kind**, `end`; an unknown
  `kind` is a hard error under the execution-model version policy, so a `0.7`
  engine MUST refuse an artifact carrying one — which is the intended signal,
  since termination is capability an older engine cannot fake. And
  `E-STATE-COLLECTION` **enforces a rule that was always normative but never
  checked**: a `state:` declaration typed `list`/`record`/`map` used to pass the
  shape validator and now fails. Everything else is optional and append-only —
  apart from that one declaration, a `0.7.0`-clean document needs nothing but a
  restamped `luteVersion:` to check clean under `0.8.0`.
- **`addr` widths are uniform per artifact, and that is now a guarantee.** The
  index segment used to be a fixed four digits, so a shot with 100 or more
  records emitted `001-11500` beside `001-1400` and string comparison reported
  `"001-11500" < "001-1400"` — lexicographic order silently diverged from
  execution order. Both segments are now padded to a width computed from the
  document and held uniform across the artifact, so *lexicographic order over
  `addr` equals execution order* is something an engine may rely on. A document
  whose every shot emits fewer than 100 addresses compiles byte-identically to
  `0.7.0`. `addr` is still a position regenerated on every compile, never an
  identity — the stable joins remain `lineId` / `voiceKey`.
- **The `0.6.1` coverage warnings are honesty, not errors.** They name the exact
  edge of what static analysis can prove — a relational fact query it can neither
  prove nor refute, a stale `luteVersion` stamp, an unproducible trace mock — and
  never flip the exit code on their own. The two `0.8.0` additions are ordinary
  findings rather than coverage claims — unreachable content after an `::end`,
  and a translatable record missing a locale the bundle declares — but they carry
  the same severity. Promote any of them to an error with `--deny <CODE>` or
  `--deny-warnings`.
- **Design rationale lives alongside the specs.** The four-tier state model's
  *why* is recorded in
  [`state-model-design.md`](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/state-model-design.md),
  a non-normative companion to `0.0.1` §9.
- **Capability surfaces are specified separately.** Character/cast identity and
  the plugin system are capability proposals, not core scenario-DSL revisions.
  The plugin system's current revision is `0.0.2`, which lands alongside `0.8.0`:
  option and frontmatter value validation, reserved stamp-attribute rejection,
  cross-cutting `stampAttrs`, and the declarative `lower: { record, fields }`
  form made real. See the [specification index](/spec/) for both.
