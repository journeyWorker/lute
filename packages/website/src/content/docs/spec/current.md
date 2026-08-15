---
title: Current specification
description: The consolidated index of what the Lute language enforces today at version 0.11.0 — each language area mapped to the versioned proposal that introduced or last changed it, all pointing back to the normative repository sources.
---

The versioned proposal stack under
[`docs/proposals/scenario-dsl/`](https://github.com/journeyWorker/lute/tree/main/docs/proposals/scenario-dsl)
**remains the normative source of truth**. This page does not replace it — it is
the consolidated **index** of what is *current* at language version **0.11.0**:
for each language area, which proposal revision introduced it, which last changed
it, and where to read the normative text.

:::note
Where this index and a proposal disagree, the proposal in the repo wins. For the
full cumulative history (including the pre-implementation `0.0.1` draft and the
capability proposals), see the [specification index](/spec/).
:::

## What is current at 0.11.0

| Language area | Introduced | Last changed | Normative source |
|---|---|---|---|
| Frontmatter & profiles | 0.1.0 | 0.2.0 (document-kind system — `kind: scene`/`quest` polymorphism) | [0.2.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.2.0.md) |
| Content lines (`@speaker` dialogue) | 0.1.0 | 0.5.1 (delivery-flag authoring-surface honesty) | [0.5.1.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.5.1.md) |
| Core directives (the closed `lute.core` vocabulary) | 0.1.0 | 0.9.0 (`::auto{action}` and `::music{mood}` retyped from free `string` to `{ domain: … }`, so both slots are checkable at last; the core's own member lists emptied) | [0.9.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.9.0.md) |
| Content vocabulary (`emotion`, `action`, `anchor`, `mood`, `volume`, `musicAction`, `vfxType`) | 0.1.0 (closed member lists shipped inside `lute.core`) | 0.9.0 (**the project owns the members** — the compiler declares slots and ships none; three declaration routes; the `exits:`/`default:` long form; using an undeclared slot is `E-DOMAIN-UNKNOWN`) | [0.9.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.9.0.md) |
| Branch / match / when / hub | 0.1.0 | 0.4.0 (param-scoped component `<match>` dispatch) | [0.4.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.4.0.md) |
| Reusable content components (`::use` expansion) | 0.1.0 | 0.9.0 (five root-only check stages now run over an imported component body — content-line attrs, `E-DUP-LINE-CODE`, reachability, unwalked-content admission, injection folding) | [0.9.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.9.0.md) |
| `into=` records (choice run-record sugar) | 0.1.0 (as `persist=`/`into=`, renamed from `0.0.1` `as`) | 0.6.0 (**breaking** — `persist=` removed, `into=` alone records) | [0.6.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.6.0.md) |
| State tiers (scalar `scene`/`run`/`user`/`app`) | 0.1.0 | 0.8.0 (author `state:` is scalar-only, now enforced — `E-STATE-COLLECTION`) | [0.8.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.8.0.md) |
| Facts & Datalog (relational layer) | 0.3.0 | 0.3.0 | [0.3.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md) |
| Quests (`<quest>`, `<on>` ECA triggers) | 0.2.0 | 0.8.0 (`quest.<id>.activatedAt` — a reserved `narrativeTime` anchor for `validAt`) | [0.8.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.8.0.md) |
| Timeline & property tracks | 0.1.0 | 0.1.0 | [0.1.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.1.0.md) |
| Connectivity & `after:` sequencing | 0.2.0 (`after:` scene sequencing) | 0.8.0 (`active("questId")` — the third prerequisite primitive) | [0.8.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.8.0.md) |
| Identity & localization (`lineId` / `voiceKey`, locale texts) | 0.1.0 | 0.8.0 (`identity:` templates; the `loc import` → `compile --locales` round trip) | [0.8.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.8.0.md) |
| Compiled artifact shape (`addr` addressing, IR carriers) | 0.1.0 | 0.10.2 (`meta.plugin` — a plugin-owned, checker-validated frontmatter key now reaches the compiled artifact instead of being discarded at compile time; plugin-system `0.0.4`, additive, no engine gate widens since `0.10.2` shares major.minor `0.10`) | [0.10.2.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.10.2.md) |
| Warning-severity diagnostics (`W-UNPROVEN-RELATIONAL`, `W-LUTE-VERSION-STALE`, `W-TRACE-MOCK-UNPRODUCIBLE`, `W-CODE-AFTER-END`, `W-L10N-MISSING`) | 0.6.1 | 0.10.0 (four new warnings — `W-COMPONENT-UNVERIFIED`, `W-DOMAIN-UNREAD`, `W-EXIT-INERT`, `W-STAGE-ABSENT` — plus `W-PROJECT-INERT`; and `W-INJECT-CONFLICT` is **removed**, the first removal in the series, because equality with the declared default was its only trigger) | [0.10.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.10.0.md) |
| Deny promotion (`--deny` / `--deny-warnings`) | 0.6.1 | 0.6.1 | [0.6.1.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.6.1.md) |
| Version stamp & axis alignment | 0.1.0 | 0.11.0 (all three axes read `0.11.0` — toolchain, language, and IR — because a release re-aligns every visible number; this time the **toolchain** earns it, alone: language `0.11.0` is byte-for-byte `0.10.2`/`0.10.1`/`0.10.0` semantics, and the IR carries no content change either — its `major.minor` moves anyway, `0.10` → `0.11`, so `schemas/lute-ir-0.10.schema.json` is renamed the same way `0.7.0`'s was) | [0.11.0.md](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.11.0.md) |

## Notes on the boundaries

- **`0.11.0` moves the IR's `major.minor` with nothing behind it — the first
  time this stack records that shape.** `0.10.1` stayed inside `0.10` and cost
  nothing; `0.10.2` also stayed inside `0.10` but moved real content
  (`meta.plugin`); `0.11.0` moves the *number itself* out of `0.10` into
  `0.11` while carrying **zero** content or shape change — the artifact
  `0.11.0` produces is byte-identical to `0.10.2`'s. An engine gated on IR
  `0.10` still has to widen its gate to `0.11` to keep accepting artifacts
  (the runtime contract gates on `major.minor`, not on whether anything
  inside actually changed), and `schemas/lute-ir-0.10.schema.json` is renamed
  to `schemas/lute-ir-0.11.schema.json` under the `0.7.0` precedent — a
  `major.minor` move renames the schema file regardless of why it moved. The
  release itself is entirely toolchain: a new `schedule.yaml` project-file
  layer and `lute play` command
  ([Schedule & play](/tooling/schedule-and-play/)), plus two fixes in the
  shared reference runner (a compiled `<when is=…>` match arm now reads its
  structured `expr` instead of always falling through to `<otherwise>`, and a
  hub whose scripted decisions run out with an eligible option remaining now
  halts incomplete instead of silently converging).
- **`0.6.0` is the one breaking *grammar* revision in the current stack.** It
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
- **`0.9.0` — vocabulary ownership — is breaking in validation, not in
  grammar.** The compiler declares the seven content-vocabulary *slots* and
  ships **no members**, so a document writing `emotion="delighted"` needs its
  project to declare `emotion`; using a slot nobody declared is
  `E-DOMAIN-UNKNOWN`, an **error**, where `action` in particular used to be
  skipped outright. Every pre-`0.9.0` `enums:` block still parses byte-for-byte
  — a bare sequence is shorthand for `{ members: [...] }` — but a declaration of
  `action` MUST now supply `exits:` and one of `anchor` MUST supply `default:`,
  because those are the two places the compiler branches on *which* member, and
  it no longer infers them from a name prefix. **The IR shape did not move in
  that release**: no field was added, renamed, or moved, and `irVersion` read
  `"0.9.0"` only because a release re-aligns every axis — an engine gated on IR
  `0.8` had only to widen its gate to `0.9`. The
  artifact's *content* does move — `enums` becomes populated for a project that
  declares inline or through `uses:`, and `capabilityVersion` shifts. The
  authoring side is written up at [Content vocabulary](/language/vocabulary/).
- **`0.9.0` also made an imported component body check like the content it
  is.** Five of `check()`'s eighteen diagnostic stages were root-only and never
  ran over a component body, so the same lines checked *clean* through a `::use`
  and *dirty* at scene level — content-line attribute rules, `E-DUP-LINE-CODE`,
  reachability, admission of content the walker does not process, and injection
  folding. All five run now, anchored at the `::use` site with a prefix naming
  the component and its file. A component body that used to pass may report, and
  every such report was already reaching the artifact or already being silently
  dropped. What a component body still does **not** get is its own vocabulary
  scope: its `uses:` and its own inline `enums:` are both discarded at parse, so
  the body resolves vocabulary against the **importing** document.
- **`0.10.0` — the toolchain says what it knows — reddens documents, mocks, and
  the IR shape.** Thirteen language changes, and the through-line is that every
  one of them is a place the checker already held the answer. Three of them can
  redden a document that checks clean today: `::set` now types its right-hand
  side against the path it writes (`E-SET-TYPE`), the six logic tags close their
  attribute sets (`E-UNKNOWN-ATTR`, and `E-AS-REMOVED` for `as=` on a
  `<choice>`), and a `<quest start>` gate that can never open is
  `E-QUEST-UNREACHABLE` where it used to be silent. `mocks/*.yaml` is validated
  for the first time and its `file:` key is **required** — a mock without one is
  `E-MOCK-SUBJECT`. And the IR **shape** moves for the first time since
  `0.8.0`: `provenance.reason` becomes `provenance.explanation`, so an engine
  gated on IR `0.9` must widen to `0.10` and rename the one field it reads.
  `W-INJECT-CONFLICT` is **removed** rather than narrowed, and the information
  it carried is dropped, not migrated — agreement with the declared default was
  its only trigger, so there was nothing left to warn about.
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
  The plugin system's current revision is `0.0.4`, landing alongside `0.10.2`:
  a plugin-owned, checker-validated frontmatter key now reaches the compiled
  artifact (`meta.plugin`) instead of being discarded at compile time. `0.0.3`
  — `lute.core` exports an empty `enums`, an `enums` entry may carry the long
  form's member semantics, and the closed `semantics` flag vocabulary drops
  the two flags no consumer read — and `0.0.2` — option and frontmatter value
  validation, reserved stamp-attribute rejection, cross-cutting `stampAttrs`,
  and the declarative `lower: { record, fields }` form made real — remain its
  base as amended. See the [specification index](/spec/) for all four.
