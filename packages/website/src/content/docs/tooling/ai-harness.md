---
title: AI harness guide
description: Wiring Lute into an AI authoring pipeline — lute context as prompt context, the lute check --json feedback loop with --deny promotion, the exit-code contract, the capability-resolution errors that bypass JSON, the diagnostics a generator actually trips, and the proof-vs-review verification boundary.
---

Lute is built to be driven by a model, not just a person. An AI harness reads exit codes and JSON, never prose, so the whole authoring surface and every verification gap is exposed on the tool surface. A working loop is: **context in → generate → check → promote → tag**.

## Prompt context: `lute context --json`

Seed the model from the project's *authoring surface*, never from guesswork:

```sh
lute context scene.lute --json --project .
```

It emits the project-resolved directives, attrs, enums, asset kinds, providers, state schema, relational vocabulary, imported components, and a `capabilityVersion`. It is a capability **query**, not validation: it emits regardless of the document's own diagnostics (exit `0`), and — the key property — **works on an empty file**, because the surface comes from the resolved project and plugins, not the document body. Use `capabilityVersion` as a prompt-cache key: the vocabulary only changes when it does.

## Feedback loop: `lute check --json`

After each generation, check and feed the serialized diagnostics back:

```sh
lute check scene.lute --json --project .
```

A pipeline judges by exit code. To make a warning block the loop, promote it with the rustc/clippy-style flags (0.6.1 §5), also on `check-project`:

```sh
lute check scene.lute --json --deny W-UNPROVEN-RELATIONAL --deny-warnings
```

`--deny <CODE>` (repeatable) treats exactly that code as an error for the verdict and exit code; `--deny-warnings` promotes every warning. A promoted diagnostic reports severity `error` and carries `"denied": true` in JSON, distinguishing it from a native error. An unknown code is a usage error (exit `2`). Errors are never demotable.

## Capability-resolution errors gate the exit code

Some errors describe the **project**, not a span in a document: a plugin option that does not exist or fails its declared type, an `identity:` template naming an unknown token, a profile activating a plugin that is not installed. These print on the `lute:` channel — stderr, `lute: <CODE>: <message>` — and are **not** in the `--json` diagnostic list, because they have no document position to attach to.

Since 0.8.0 they set the exit code. Previously they printed and the build passed, so a typo'd plugin option shipped silently:

<!-- lute-diagnostics -->
```console
$ lute check scene.lute --project . --json
lute: E-PLUGIN-OPTION-TYPE: option `showcase.pack.resultScope` expects enum(scene|run), got "galaxy"
$ echo $?
1
```

Verified on `check`, `check-project`, `compile` (including `--all`), `context`, and `trace`. The one deliberate exemption is the forced-single-root reconciliation scan behind `compile --project`: a sibling document belonging to a nested subproject legitimately mis-resolves under a forced root, and that is not the target document's fault.

**The harness consequence is the important part.** Exit `1` with an **empty stdout** is now a reachable state under `--json`, and it means a project-level error. A loop that parses stdout and treats "no diagnostics" as "clean" will report success on a broken project. Branch on the exit code first, parse JSON second, and surface stderr when the two disagree.

The codes on this channel: `E-PLUGIN-OPTION-UNKNOWN`, `E-PLUGIN-OPTION-TYPE`, `E-PLUGIN-MISSING-ACTIVE`, `E-IDENTITY-TEMPLATE`, and the plugin load/assembly family (`E-PLUGIN-MANIFEST`, `E-PLUGIN-PARSE`, `E-PLUGIN-DUP-ID`, …).

## Exit-code contract

| Command | 0 | 1 | 2 | 3 |
|---|---|---|---|---|
| `check` / `check-project` | clean | error present | I/O | — |
| `compile` (incl. `--all`) | success | failed gate | I/O / serialization | — |
| `trace` | complete | refused | I/O | incomplete |
| `test` | every test passed | a test failed | I/O | — |
| `loc import` | bundle written | `E-LOCALE-BUNDLE` | I/O | — |

For `trace`, distinguish the two failure modes: **1 = refused** (check errors or invalid mocks — fix the document, then retry) versus **3 = incomplete** (an `unknown` guard halted the walk — supply more mock seeds, then retry). They demand different retry strategies.

`compile --all` is all-or-nothing: exit **1** means nothing was written, so a partially-updated output directory is never a state the harness has to reason about.

## Diagnostics a generator will meet

These are the diagnostics a model-driven loop actually trips, grouped by what to do about them.

**`E-DOMAIN-UNKNOWN` — the one that will bite first.** Since `0.9.0` the compiler declares the seven content-vocabulary *slots* and ships **no members**, so `@mira{emotion="delighted"}` fails until the project declares `emotion`. A model has no way to guess a project's palette, and guessing is exactly the wrong move: the members are in the capability snapshot, so **read them, never invent them.** `lute context --json` splits them by route — `projectEnums` for the members an `enums:` block declares (inline or through `uses:`/`extends:`), `enums` for the ones an active plugin exports:

```console
$ lute context docs/examples/property-tracks.lute --project docs/examples --json
{ …, "enums": {}, "projectEnums": { "action": ["fade-in-up", …, "hide"], "anchor": ["left", "center", "right"],
  "emotion": ["neutral", "surprised", "delighted", "shy", "content", "angry", "sad"], … } }
```

Both keys are folded into `capabilityVersion`, so the prompt-cache key already moves when a vocabulary does. If the union is empty for a slot the document needs, the fix is a *declaration*, not a different attribute value — see [Content vocabulary](/language/vocabulary/) for the three routes. Two follow-on codes catch a model writing the declaration itself: `E-ENUM-MISSING-SEMANTICS` (`action` declared without `exits:`, or `anchor` without `default:` — the compiler branches on those members and no longer infers them from a name prefix) and `E-DOMAIN-DUP` (one slot declared by both a plugin and a project route in the same root; pick one route per slot).

**`E-STATE-COLLECTION`.** Author `state:` is scalar: `bool | number | string | enum`. A `list`, `record`, or `map` declaration is rejected. Models reach for `type: { list: string }` constantly, and this was the single case where a document clean under 0.7.0 newly failed at 0.8.0. The fix is never to coerce the type — it is to model the collection as [`relations:`](/state/facts-and-datalog/), which is what the relational layer exists for. Feed the diagnostic back verbatim; it names the remedy.

```lute expect="E-STATE-COLLECTION"
---
kind: scene
character: mira
season: 1
episode: 1
state:
  run.inventory: { type: { list: string }, default: [] }
---

## The Counter

@mira: What have you got in there?
```

<!-- lute-diagnostics -->
```console
$ lute check scene.lute
scene.lute:7:3: error [E-STATE-COLLECTION] state path `run.inventory` cannot declare a collection type (`list`/`record`/`map`); author state is scalar (number|bool|string|enum) — model collections as `relations:` (dsl 0.3.0 §3) or a plugin `state_shapes` slot
failed: scene.lute (1 error(s), 0 warning(s))
```

**`W-CODE-AFTER-END`** — content after `::end` in the same straight-line body (shot body, `<choice>` body, `<when>` arm, objective body, `<on>` body) is unreachable. Reported once per body, at the first such node. A generator that emits a terminator and then keeps writing produces exactly this. Promote it with `--deny W-CODE-AFTER-END` if dead content should block the loop.

**`E-IDENTITY-TEMPLATE`** — a `lute.project.yaml` `identity:` template using a token other than `{prefix}`, `{speaker}`, `{code}`. Project-level, so it arrives on the `lute:` channel (see above), not in JSON.

**`W-L10N-MISSING` / `E-LOCALE-BUNDLE`** — the localization round trip. The warning is one per `(lineId, locale)` pair a `compile --locales` bundle fails to cover; the error is a malformed bundle at `loc import` (unparseable input, a `lineId` appearing twice within one locale, an empty locale tag). A harness that owns translation should run `--deny W-L10N-MISSING` so an incomplete bundle cannot reach an artifact.

**The plugin family** — these fire when the model authors a *plugin*, not a document:

| Code | Cause | Channel |
|---|---|---|
| `E-PLUGIN-OPTION-UNKNOWN` | an activated option name the plugin manifest never declares | `lute:` |
| `E-PLUGIN-OPTION-TYPE` | a merged option value that fails its declared type | `lute:` |
| `E-PLUGIN-RESERVED-STAMP-ATTR` | an attr named `at`/`duration`/`delay`/`wait`/`timeline`/`provenance`/`source` — reserved stamp keys, on both the `stampAttrs` and per-directive surfaces | `lute:` |
| `E-LOWER-RECORD-UNKNOWN` | `lower: { record }` naming something that is not a core staging kind | `lute:` |
| `E-LOWER-RECORD-FIELD` | `lower: { fields }` naming a field the record kind lacks, a `fromAttr` the directive never declares, or a literal that cannot fill the field | `lute:` |
| `E-FRONTMATTER-SCHEMA` | a plugin-owned frontmatter key whose value violates the declared schema | JSON, with a span |

Only `E-FRONTMATTER-SCHEMA` has a document position and therefore a JSON diagnostic; the rest are project-level and reach you on stderr with an empty JSON body. Every code on this page is a valid `--deny` argument — an unknown code is a usage error (exit `2`), so the flag cannot silently protect nothing.

## Fixits

Diagnostics carry fixits with a `kind`. `kind: "migrate"` is machine-applicable — apply it unprompted (this is what `lute fix` does). `kind: "refactor"` is an author choice — surface it as an LSP code action, never auto-apply.

## The verification boundary

`check` is sound but deliberately incomplete over the relational layer, so know which regions are proof-covered and which are review-covered:

- **Scalar gates are proof-covered** — reachability and Guaranteed/Possible envelopes (§5) statically decide them.
- **Relational fact gates are review-covered** — a fact query over a producible relation is always `Undecided`. `W-UNPROVEN-RELATIONAL` marks each such `<objective done>`/`<quest start|fail>` predicate; deny it to force human routing of those regions.
- `W-TRACE-MOCK-UNPRODUCIBLE` warns when a `lute trace` mock seeds a fact over a relation no authored producer can ever assert — the walk proves nothing about reachable play.
- `W-LUTE-VERSION-STALE` catches a model reproducing a stale `luteVersion` stamp copied from an old example.

## Close with `lute tag`

Run `lute tag scene.lute` at the pipeline end to back-fill a stable `code` into every untagged line. Never let the model hand-write `code=` values — line identity is the tool's job, not the model's.
