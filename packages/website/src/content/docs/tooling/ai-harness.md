---
title: AI harness guide
description: Wiring Lute into an AI authoring pipeline — lute context as prompt context, the lute check --json feedback loop with --deny promotion, the exit-code contract, machine-applicable fixits, and the proof-vs-review verification boundary.
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

## Exit-code contract

| Command | 0 | 1 | 2 | 3 |
|---|---|---|---|---|
| `check` / `check-project` | clean | error present | I/O | — |
| `compile` | success | failed gate | I/O / serialization | — |
| `trace` | complete | refused | I/O | incomplete |

For `trace`, distinguish the two failure modes: **1 = refused** (check errors or invalid mocks — fix the document, then retry) versus **3 = incomplete** (an `unknown` guard halted the walk — supply more mock seeds, then retry). They demand different retry strategies.

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
