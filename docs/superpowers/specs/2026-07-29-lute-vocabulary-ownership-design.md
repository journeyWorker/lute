# Lute — Vocabulary Ownership: the Core Declares Slots, the Project Declares Members (approved design)

- **Date:** 2026-07-29
- **Status:** approved design; spec-first (documents/decisions before implementation)
- **Version:** **language 0.9.0** + **plugin-system 0.0.3** delta. Breaking at the language axis
  (pre-1.0 allowance, dsl 0.1.0 §2): a document using a domain slot must now declare that domain.
  **IR schema unchanged at 0.8.0** by construction (D-B).
- **Drives:** `docs/proposals/scenario-dsl/0.9.0.md` (new), `docs/proposals/plugin-system/0.0.3.md`
  (new delta), `crates/lute-manifest` (`enums.yaml` emptied, `enums:` long form, `staging.yaml`
  retypes), `crates/lute-check` (`content_line.rs` guard removal, `inject.rs` de-hardcoding),
  `crates/lute-compile` (`lower.rs` de-hardcoding), `crates/lute-cli` (`lute init` scaffold),
  `docs/examples/*.schema.yaml`, `docs/plugin-system.md`,
  `docs/adoption/oshiz-assessment.md` (D1 row correction), shipped JSON Schemas.
- **Provenance:** this session's diagnosis dialogue. **This is not a new direction.** It completes
  `2026-07-10-lute-data-catalog-foundation-design.md` **D1**, whose approved classification the
  implementation deviated from, and empties D1's "fixed core enums" bucket entirely.

---

## 0. Framing: Lute is a general authoring tool

Lute ships as an open-source authoring toolchain; OSHiZ is one consuming application. A concrete
list of emotions inside the compiler is therefore a **category error** — not merely inconvenient for
one app, but wrong for a tool whose whole premise is that vocabulary is declared data. Data-catalog
foundation **D1** already said so:

> **Fixed core enums** — the compiler branches on the specific member, so the members ARE language
> semantics. These live in `lute.core`, are closed, and are **NOT author-extensible**.
> **Data vocabularies** — the checker only membership-checks; the value flows through as data
> (`emotion`, `action`, `character`, `costume`, `mood`, `vfxType`). These are **author/plugin
> definable**.
> (Litmus: does the compiler/engine change behavior by *which member* it is?)

`emotion`, `mood`, and `vfxType` were classified **definable**. The implementation put all six
baseline enums into `lute.core` as `open: false`, and every merge path drop-and-reports, so none of
them is definable by anyone:

| Attempt | Result today | Site |
|---|---|---|
| project schema `enums: emotion: […]` | `E-DOMAIN-DUP`, project members dropped | `schema_import.rs:508` |
| own capability plugin exporting `emotion` | `E-PLUGIN-DUP-ACROSS` + `E-DOMAIN-DUP`, **whole project resolution fails** — `lute context` refuses too | `assemble.rs:308` → `merge_map:462` |
| provider named `emotion` | closed domain wins over same-named provider | `directives.rs:390` |
| not activating `lute.core` | impossible — unconditional | `resolve.rs:141` |
| project-schema `extends`' "may only ADD members" relaxation | exists only *within* the schema-file chain; core is not a base of it | `schema_import.rs:399` |

`docs/plugin-system.md:57` independently names `emotion="smug"` as an example of registrable data.
The profile machinery a fix would need **already ships** (`lute.project.yaml` `profiles:`/
`defaultProfile:`/`extends:`, scene-local activation, the six-step order — `project.rs:62`,
`resolve.rs:108`). Nothing was missing except that the vocabulary sat in the one plugin nothing can
influence.

**Measured cost** (`eevee/packages/data-catalog`):

| Domain | core members | measured usage | unrepresentable |
|---|---|---|---|
| `emotion` | 7 | 30,861 values / 17 distinct | **6,377 (20.7%)**; core's `angry` never used — they say `furious` |
| `vfxType` | 7 | `blackOut` ✓, `none`, `plasma-divider` | **78 rows** |
| `action` | **none** | 9,880 values / 53 distinct | receives **zero validation** today (D-C) |
| `mood` | 5 | 10 distinct, 2 overlap | moot: `::music{mood}` is `type: string`, so the domain has no consumer |
| `anchor` | 3 | 3 distinct | none |
| `volume`, `musicAction` | 5, 5 | unused by this app | none |

The 38,090-row `CH.{character}.{costume}.{emotion}.{variant}` asset-id space repeats the `emotion`
split.

## 1. The shape of the fix

**The core declares *slots*; it never declares *members*.** `assets/lute.core/enums.yaml` is
emptied. Seven domain names survive as *types on attributes* with no member list anywhere in the
binary, and every member comes from a project schema or a plugin.

Because the core declares no members, **there is nothing to override**: no `overrides:` protocol, no
"baseline plugin" concept, no override-conflict diagnostics. `E-DOMAIN-DUP` survives untouched for
genuine peer collisions and simply stops firing for these seven names.

### Rejected alternatives

- **Split `lute.core` into a language core plus an overridable `lute.vn` profile plugin.** Its only
  justification was conceptual purity — and since replacing a *directive* is a non-goal (§6), the
  split produces **no observable difference** while touching four crates. Dropped.
- **Keep core members, add an explicit per-domain `overrides:`.** Strictly more machinery than
  emptying the core (an override protocol, a baseline concept, three diagnostics) to reach a weaker
  place: core still ships an arbitrary vocabulary, and every consumer inherits members it must
  explicitly repudiate. Dropped.
- **Move `emotion`/`action`/`variant`/`dialogMotion` into `stampAttrs`.** More uniform, and the
  mechanism already admits it (`content_line.rs:79`), but a `stampAttrs` key lowers into the
  record's `stamp` instead of a named IR field — an IR break for every engine and 30,861 rows,
  bought for purity. Dropped; see D-B.

---

## D-A. The core's seven slots, member-less

| Slot | Bound at | Type change |
|---|---|---|
| `emotion` | content line | none (already domain-typed) |
| `action` | content line, `::auto{action}` | `::auto{action}`: `string` → `{ domain: action }` |
| `anchor` | `::auto{anchor}` | none |
| `mood` | `::music{mood}` | `string` → `{ domain: mood }` |
| `volume` | `::music{volume}` | none |
| `musicAction` | `::music{action}` | none |
| `vfxType` | `::vfx{type}` | none |

The two retypes exist so the slot is actually checkable: today `::auto{action}` and `::music{mood}`
are free strings, which is why the `mood` domain has been declared-but-inert since it shipped.

**Not domains, and untouched:** `::cut{action}` and `::video{action}` are `{ enum: [show, hide] }`
(`staging.yaml:40,46`) — a genuine two-member pairing the engine dispatches on, declared inline on
the directive, not a shared vocabulary. Likewise the delivery flags `mono`/`os`/`vo`, the `narrator`
speaker name (`content_line.rs:110`), and the `::end` tag (`core.rs:24`) are grammar, not vocabulary.

## D-B. The attribute name is the language's; the member set is the project's

`content_line.rs:23 KNOWN_ATTRS` and its 1:1 mapping to IR fields (`lower.rs:43`) **do not move**.
`emotion=` stays a content-line grammar slot lowering to a top-level IR field; only the *members it
resolves against* are project-owned.

```jsonc
// unchanged, guaranteed by this decision
{ "role":"dialogue", "text":"Mr. Fixer!", "emotion":"delighted", "variant":1, … }
```

`irVersion` stays `0.8.0`. This is the one thing the design refuses to trade.

## D-C. Declaring is mandatory — by deleting a special case

`content_line.rs:161` currently *skips* validation when nothing declares `action`:

```rust
"action" if (domains.contains_key("action") || snapshot.providers.contains_key("action")) => { … }
_ => {}   // nobody declared it → silently clean
```

This is why OSHiZ's 9,880 `actionId` values across 53 distinct ids get **zero** checking today: a
typo like `step-foward` ships. **Delete the guard clause.** `action` then falls through to
`check_domain_member` exactly as `emotion` does, and step 4 (`directives.rs:402`) already emits the
right diagnostic:

> `E-DOMAIN-UNKNOWN` — `action` is not a known domain, declared by neither the plugin/core
> vocabulary nor a project schema

**No new diagnostic code; strictness arrives by removing a special case, not by adding a rule.**
Reword the message for the content-line context and to name the fix (declare the domain); keep the
code.

An undeclared slot is an **error, not a warning**: a slot whose members nobody wrote down cannot be
checked, and silently not checking is the failure mode this whole design exists to remove.

## D-D. Member-level semantics are declared with the members

Applying D1's litmus to today's code, `action` and `anchor` are *fixed core* — the compiler branches
on **which member**:

| Hardcoded member semantics | Sites |
|---|---|
| exit detection: `fade-out*` \| `exit*` \| `hide` | `inject.rs:371` **and `lower.rs:480`** — two hand-maintained copies, the second commented *"mirrors `lute-check::inject`'s private helper byte-for-byte"* |
| `DEFAULT_ANCHOR = "center"`, plus a warning for a redundant explicit `center` | `inject.rs:61,229` (single-sourced, re-exported at `lib.rs:63`) |

A directive-level `semantics` flag cannot express either — both are properties of *individual
members*. So the domain declaration carries them:

```yaml
enums:
  emotion: [neutral, content, delighted, shy, surprised, sad, affection]   # flat list = { members: […] }
  anchor:
    members: [left, center, right]
    default: center
  action:
    members: [fade-in-up, sway, lean, nod, hop, step-forward, fade-out, fade-out-down, hide]
    exits:   [fade-out, fade-out-down, hide]
```

- A flat list is shorthand for `{ members: […] }` — every existing `enums.yaml` and project schema
  keeps parsing.
- `default:` must be a member → `E-ENUM-DEFAULT-NOT-MEMBER`.
- Every `exits:` entry must be a member → `E-ENUM-EXITS-NOT-MEMBER`.
- **A declaration of `action` MUST supply `exits:`; a declaration of `anchor` MUST supply
  `default:`** → `E-ENUM-MISSING-SEMANTICS`. The core knows these two slots need member semantics
  (that is the same "the language owns the slot" claim as D-B); it does not know which members
  satisfy them. Omission is an error rather than a fallback, because a silent fallback to
  `fade-out*` is precisely the hidden coupling being removed.
- For the other five slots `exits:`/`default:` are meaningless and rejected →
  `E-ENUM-UNEXPECTED-SEMANTICS`, so a typo cannot hide in an ignored key.

## D-E. Delete the hardcoding

- **Both copies** of the exit heuristic: `lute-check`'s at `inject.rs:370-372` and `lute-compile`'s
  twin at `lower.rs:477-481`. Replaced by the resolved domain's `exits:` set, read from the one
  snapshot both crates already hold — which also removes the hand-maintained duplication the
  plugin-system doc cites as the very reason the manifest exists.
- `DEFAULT_ANCHOR` (`inject.rs:61`) and its `lib.rs:63` re-export. Replaced by the domain's
  `default:`.
- **Dead `pose` reads.** `inject.rs:350` reads a `pose` attr and `:364` lists it as
  sprite-affecting, but `pose` is absent from `KNOWN_ATTRS`, so `@x{pose="…"}` is `E-UNKNOWN-ATTR`
  and neither line is reachable. The stateful set becomes `emotion | variant | action |
  dialogMotion`.
- **Two fictional `semantics` flags.** `SEMANTICS_VOCAB` (`validate.rs:4`) declares 12 flags and
  **zero are consumed** by the checker or compiler; `isStateful` and `cancelsPrevious` are attached
  to no directive and have no honest consumer even after this change. Remove them (plugin 0.0.3
  delta; no shipped plugin declares either). The remaining nine keep their current declarative
  roles — wiring them to drive dispatch is explicitly **not** in this design (§6).

**Audit backing the central claim.** A repo-wide search for member-literal branches
(`== "center"|"left"|"neutral"|"show"|"hide"|"start"|…`, `starts_with("fade"|"exit"|"pose")`) in
non-test compiler/checker code returns **exactly the two `is_exit_action` copies**. After D-E, the
core branches on **zero** domain members, and D1's "fixed core enums" bucket is empty.

## D-F. The opinionated default lives in the scaffold, not the compiler

Emptying the core must not make first contact hostile. `lute init` already scaffolds
`lute.project.yaml`, a state schema, a starter scene, and a trace mock (`main.rs:277`); it gains a
**starter vocabulary declaration** carrying a reasonable VN-ish `emotion`/`action`/`anchor` set with
`exits:`/`default:` filled in.

This is the whole answer to "every project uses something different": the **template** is
opinionated, the **tool** is not. An author edits a file they own instead of repudiating members
compiled into the binary. `lute check <file>` on a bare file that uses `emotion=` errors and names
the fix; `lute doctor` reports which slots are declared and which are not.

---

## 2. Diagnostics

| Code | Severity | Meaning | New? |
|---|---|---|---|
| `E-DOMAIN-UNKNOWN` | error | a domain slot is used but no source declares the domain (D-C) | reused; message reworded |
| `E-ENUM-DEFAULT-NOT-MEMBER` | error | `default:` is not in `members:` | ✔ |
| `E-ENUM-EXITS-NOT-MEMBER` | error | an `exits:` entry is not in `members:` | ✔ |
| `E-ENUM-MISSING-SEMANTICS` | error | `action` declared without `exits:`, or `anchor` without `default:` | ✔ |
| `E-ENUM-UNEXPECTED-SEMANTICS` | error | `exits:`/`default:` on a slot that has no such semantics | ✔ |

`E-DOMAIN-DUP`, `E-PLUGIN-DUP-ACROSS`, and `E-BAD-ENUM` are unchanged; the first two simply stop
firing for the seven names.

## 3. Compatibility & migration

| Axis | Effect |
|---|---|
| **IR schema** | **unchanged (0.8.0)** by D-B |
| **Artifact content** | changes, deliberately: a project's declared vocabulary now reaches the compiled artifact's `enums` array. `build_rel_vocab` copies `SchemaImports.rel.enums` and `lute-compile`'s `rel_entries` serializes each entry, whereas a core-shipped vocabulary lived in the capability snapshot and was never serialized per artifact. Not an IR-schema change (no field added, renamed, or moved; `irVersion` stays `0.8.0`) — the artifact simply becomes self-describing about the vocabulary it was compiled against, which is the honest consequence of that vocabulary becoming project-declared data. Engines that ignore `enums` are unaffected |
| **capabilityVersion** | changes — core's `enums`/`domains` empty and two attr types change. Stamped and expected (plugin §13) |
| **Documents** | breaking: a document using any of the seven slots needs its project to declare that domain. `lute init` scaffolds one |
| **Plugins** | break only if one declares `isStateful`/`cancelsPrevious` (none shipped) |
| **`lute.project.yaml`** | no new keys |
| **Versions** | language 0.9.0; plugin-system 0.0.3; toolchain per release |

Measured migration surface:

- **`conformance/`: zero fixtures.** No conformance source uses `emotion`/`anchor`/`vfxType`/
  `volume`/`mood`, so every single-document contract test is untouched.
- **Tests: 19 sites**, plus the `golden.rs` harness. `lute-check`: `line_when`, `content_line`
  (one test deliberately stays on the bare core — see below), `component_match`, `examples`,
  `fact_query`, `fragment_kind`, three tests in `domains.rs`, and the `#[cfg(test)]`
  `vocab_domains()` helper in `src/directives.rs`; `lute-compile`: `inject`, `component_fold`,
  `timeline`, `compile`, `address`, `flatten`, `stamp_attrs`; `lute-lsp`: the `completion.rs` and
  `hover.rs` anchor-domain tests. All 12 `golden/*.lute` fixtures route through one harness
  (`golden.rs`), so they are covered by the shared test vocabulary. 49 Rust files mention
  `load_core_snapshot`; only these touch vocabulary.
  **Two counts of this surface were wrong before this one.** The first said 11 files: it came from
  grepping attribute literals (`emotion=`, `anchor=`, …), which cannot see a domain referenced
  programmatically (`fact_query.rs` resolves `emotion` via `snap.domains["emotion"]`;
  `directives.rs`'s inline tests went through a `core_domains()` helper) or an attribute that only
  becomes domain-typed under D-A (`fragment_kind.rs` authors `::auto{action="fade-in-up"}`). The
  second said 14 and still missed the three `domains.rs` tests and both `lute-lsp` sites. What
  finally worked was a **probe**: temporarily write `enums: {}` over the core, run the suites, and
  read the failure list. Any future re-measurement of a vocabulary surface should probe, not grep.
  Three of the found sites needed a semantic decision rather than a mechanical switch, because they
  assert *about* the core baseline: `merge_domains_unions_project_with_core` needs a snapshot that
  has `emotion` but lacks `action`, so it builds a local one; the clash test indexed
  `snapshot.domains["emotion"]` and would have panicked, not failed; and `content_line.rs`'s
  `action_is_open_by_default` must keep asserting against a snapshot with NO `action` domain or it
  passes for the wrong reason.
- **`docs/examples/`: 5 project roots**, each already carrying `lute.project.yaml`, and
  `base.schema.yaml`/`state.schema.yaml`/`act1.schema.yaml`/`child.schema.yaml` already holding
  `state:`/`defs:` blocks — `enums:` joins them. CI runs `check-project docs/examples`
  (`.github/workflows/docs.yml:70`), so this is gated.
- **`docs/architecture.md`** uses content-line `action="sway"`/`"lean"` (`:162,165`) and is **not**
  covered by any gate: the `examples` job checks only `docs/examples`, and the `website` job checks
  syntax highlighting, not semantics. Fix it in the docs task and record the gate gap.

That the test harness must declare its vocabulary is a feature, not a tax: the harness then performs
exactly the act a consuming project performs.

## 4. Verification plan

Evidence, not assertions.

1. **`exits:` reproduces the deleted heuristic.** Before removing either copy, a table test asserts
   `exits:`-membership equals `is_exit_action`'s verdict over the scaffold vocabulary **plus** the
   values every repo fixture uses. Proven equivalent, not assumed.
2. **The two copies agreed.** A test asserting `lute-check`'s and `lute-compile`'s exit verdicts are
   identical over the same input set, written *before* deletion — if they had already drifted, the
   replacement must be told which one was right.
3. **Goldens hold.** After the shared vocabulary helper lands, `cargo test -p lute-check
   -p lute-compile` passes with **no** `snapshots/*.snap` re-recording. Any snapshot delta means a
   member-semantics regression, not a fixture that needs blessing.
4. **Conformance untouched.** `conformance/` replays green with zero fixture edits — the claim that
   no fixture uses these vocabularies, re-checked by CI rather than by grep.
5. **Strict bites.** In a project declaring no `action` domain, `@x{action="wave"}` reports
   `E-DOMAIN-UNKNOWN` naming the fix; declaring it makes `wave` clean and `zzz` `E-BAD-ENUM`.
6. **The core ships no members.** A test asserting `load_core_snapshot().enums.is_empty()` and
   `.domains.is_empty()`, plus the member-literal audit of §D-E re-run as a grep test, so a future
   commit cannot quietly reintroduce a baked-in vocabulary.
7. **Semantics guards.** One fixture per code in §2.
8. **Real-data smoke.** Generate a vocabulary from `eevee/packages/data-catalog` (emotion 17,
   action 53 with its 4 exits, vfxType 3) and `lute check-project` a scene converted from
   `idola_script_commands/`; every previously-unrepresentable value resolves, including the 78
   `none`/`plasma-divider` vfx rows and the 6,377 emotion occurrences.
9. **Zero-config path.** `lute init tmp && lute check-project tmp` is green out of the box, proving
   D-F's scaffold is complete rather than a stub.

## 5. Sequencing

Each step ends green.

1. `enums:` long form (`members`/`default`/`exits`) + its four validation codes. Flat-list shorthand
   keeps every fixture parsing; nothing else changes yet.
2. Shared test vocabulary helper; switch the 11 test files and the `golden.rs` harness to it. Still
   green against a core that *still* has members — this step is purely additive.
3. Read `exits:`/`default:` in `lute-check` and `lute-compile`; verification 1 and 2 gate the
   deletion of both `is_exit_action` copies and `DEFAULT_ANCHOR`.
4. Empty `assets/lute.core/enums.yaml`; retype `::auto{action}` and `::music{mood}`. Verification 3,
   4 and 6 gate it.
5. Delete the `content_line.rs:161` guard; reword `E-DOMAIN-UNKNOWN`. Verification 5 gates it.
6. `lute init` scaffold vocabulary + `lute doctor` slot report. Verification 9 gates it.
7. Remove the dead `pose` reads and the two fictional flags.
8. Docs: `scenario-dsl/0.9.0.md`, `plugin-system/0.0.3.md`, `plugin-system.md`,
   `docs/examples/*.schema.yaml`, `docs/architecture.md`, `oshiz-assessment.md` D1 row (its current
   `emotion` claim is wrong), CHANGELOG.

Steps 1–2 are independent; 3 depends on 1–2; 4 on 3; 5 on 4. Steps 6–8 follow 5.

## 6. Non-goals

- **Splitting `lute.core`** into a language core plus a genre profile plugin. No observable effect
  once directive override is out of scope; see §1.
- **An `overrides:` protocol.** Unnecessary once the core declares no members.
- **Directive override.** Adding directives already works via a project's own capability plugin;
  OSHiZ's measured need is purely additive (`foreground` 430, `external` 366, `split-screen-*` 18,
  `ui` 13, `love-lockdown-*` 60 alongside 7 of core's 8 directives), and none of those rows carries
  `character`/`anchor`, so none needs stage-rule participation.
- **Driving reducer dispatch from `semantics` flags** (`clearsStage`, a `sceneSlot:` declaration,
  wiring the ten surviving flags). `inject.rs` keeps matching `d.tag == "auto"|"bg"|"music"`. The
  measured need is zero: no OSHiZ-side directive wants stage rules, and `mutatesScene` cannot drive
  it anyway (declared on both `::bg` and `::music`, `staging.yaml:7,16`, so branching on it would
  make `::music` clear the stage). Left as a separate, evidence-gated change.
- **Member patterns** (`pose-*`). The language has no matching facility and adding one to every
  domain declaration for a handful of known values is unwarranted; enumerate instead.
