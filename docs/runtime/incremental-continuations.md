# Checked streaming continuations

A continuation compiler lets a host append ordinary Lute shot-body text to a
trusted, already complete scene template and receive checked ordinary IR after
each complete unit, before stdin or another transport reaches EOF. It is useful
for interactive authoring and generated continuations where first checked output
matters.

This feature is **not** a streaming-only language and does not emit AST patches.
Dialogue still uses Lute's inline content-line syntax:

```lute
@guide: The corridor lights wake one by one.
```

The normative contract is
[`scenario-dsl/0.17.0`](../proposals/scenario-dsl/0.17.0.md), based on the
[implementation design](../superpowers/specs/2026-09-14-streaming-continuation-compiler-design.md).
It shipped with Lute `0.17.0`.

## Trust and source boundary

The host owns and trusts the template, project manifest, provider snapshots,
components, defaults, and identity templates. Resolve them before constructing
the compiler and keep them frozen for the stream. The template must be a valid
`kind: scene` document with at least one `## ` shot. It may already contain body
content.

Only the **body of the template's last shot** is appendable. Streamed text may
contain the same legal constructs as any shot body—dialogue, directives, closed
`<branch>`, `<match>`, `<hub>`, and `<timeline>` units, component use, and plugin
directives—but may not replace frontmatter, create a shot/label, or introduce a
quest root.

Compilation proves the source satisfies Lute's existing rules. It does not make
untrusted generated text trusted, call a model, grant filesystem or network
permission, execute a plugin bridge, persist player data, or provide a secure AI
sandbox. The host owns those policy and effect boundaries.

## CLI: runnable end-to-end example

Create a complete scene template whose final shot is ready for more body text:

```console
$ cat > /tmp/live-scene.lute <<'LUTE'
---
kind: scene
id: live-demo
---

## Live
@narrator: The connection opens.
LUTE
```

Stream two ordinary dialogue lines with a delay. Each accepted unit produces and
flushes one `update` record while the pipe is still open; EOF then produces
`finish`:

```console
$ { printf '@guide: First checked line.\n'; sleep 1; printf '@guide: Second checked line.\n'; } \
    | lute compile-stream /tmp/live-scene.lute
{"kind":"start","sequence":0,"appendFrom":0,"artifact":{...}}
{"kind":"update","sequence":1,"appendFrom":1,"artifact":{...}}
{"kind":"update","sequence":2,"appendFrom":2,"artifact":{...}}
{"kind":"finish","sequence":2}
```

`{...}` abbreviates each complete ordinary artifact in this guide; actual output
is valid one-record-per-line JSON. `--project DIR` and `--providers DIR` use the
same resolution behavior as `lute compile`, including project identity templates,
defaults, schemas, components, plugins, and provider references:

```console
$ printf '@guide: Project-resolved line.\n' \
    | lute compile-stream scenes/live.lute --project . --providers snapshots
```

The command never modifies the template. It reads the appended body from stdin,
flushes stdout after every record, and exits:

- `0` after successful EOF finalization;
- `1` after a syntax, semantic, compilation, or `E-STREAM-*` rejection; or
- `2` for usage, file I/O, broken stdout, or invalid UTF-8 input.

stderr is for invocation and I/O errors. A broken stdout stops stdin consumption
instead of silently continuing work whose results cannot be delivered.

## Rust API

The checked API lives in `lute_compile::streaming`:

```rust
use lute_compile::streaming::{
    CompilationUpdate, ContinuationCompilation, ContinuationCompiler,
};

// `input: lute_check::CheckInput` and
// `identity: lute_manifest::project::IdentityTemplates` are resolved once.
let mut compiler = ContinuationCompiler::new(input, identity)?;
let initial: &lute_compile::Artifact = compiler.artifact();

let batch: ContinuationCompilation =
    compiler.push("@guide: A complete physical line.\n");
for CompilationUpdate { sequence, append_from, artifact } in batch.updates {
    consume_snapshot(sequence, append_from, artifact);
}

let done = compiler.finish();
assert!(done.finished && done.diagnostics.is_empty());
```

`new` checks and compiles the complete template immediately. If the prefix is not
a valid scene with a shot, it returns `Err(Vec<Diagnostic>)` before any body is
accepted. `artifact()` is the latest accepted ordinary artifact.

`push(&str)` accepts decoded UTF-8 text and can return zero, one, or many updates.
A network caller receiving bytes must buffer an incomplete UTF-8 sequence before
calling it; `&str` is already valid UTF-8. `finish(&mut self)` declares EOF and
makes the compiler terminal whether finalization succeeds or fails.

## Units, chunks, and `NeedMoreInput`

A transport chunk is not syntax. It may end inside a line, quoted attribute,
block comment, or nested element. The framer emits only complete top-level body
units:

- a dialogue/directive leaf after its physical newline;
- a closed outer block after the physical line containing its matching close.

If the current suffix is incomplete, `ContinuationCompilation::need_more`
contains `Line`, `QuotedAttribute`, `BlockComment`, or
`NestedBlock { open_tags }`. This is a request for more source, not a diagnostic.
A long unfinished block waits for its close even if it contains complete-looking
inner lines.

EOF is allowed to delimit one final complete leaf without `\n`. It never invents
`</branch>`, `*/`, or a target that might appear later. Incomplete syntax at EOF
is an error.

The lower-level
`lute_syntax::incremental::IncrementalContinuationParser` remains public for
tools that need only lossless syntax framing. Its `ContinuationUnit` carries
exact source, a stream-relative half-open byte range, parsed `Vec<Node>`, and
syntax diagnostics whose spans are local to that source. Its `finish(self)`
returns an unclosed suffix separately as `IncompleteContinuation`. It performs
no manifest resolution, semantic check, merge, lowering, addressing, or artifact
assembly; use `ContinuationCompiler` when output will reach a runtime.

## What happens for every complete unit

For each framed unit, the compiler appends the unit to the accumulated source and
runs the **existing** whole-document pipeline: parser, checker, normalization,
component expansion, stage injection, lowering, address assignment, and artifact
assembly. It accepts the candidate only if the already emitted prefix is still
semantically identical.

This is incremental **delivery**, not asymptotically incremental compilation.
With many units, each unit pays for compiling the cumulative prefix again. It
reduces first-output latency but can cost more total work than one final compile.
Measure long sessions; do not assume per-unit work is constant.

A single `push` can contain several units. If unit three fails, updates for units
one and two are still returned, but unit three and everything after it produce no
artifact. Do not discard accepted updates merely because the same call also
returned diagnostics.

## Reading an update

```rust
pub struct CompilationUpdate {
    pub sequence: u64,
    pub append_from: usize,
    pub artifact: Artifact,
}
```

`sequence` begins at `1` for the first accepted body unit. `artifact` is a full
immutable ordinary snapshot, not a patch. `append_from` is the **previous
snapshot's command count**, identifying where the newly appended array region
begins. It is not a program counter and does not instruct a runtime to execute
every record after that index.

### Address padding and prefix rejection

Lute computes uniform address widths for a complete artifact. Adding enough
commands can widen every address, such as `001-0900` to `001-00900`. The service
accepts this formatting-only change by comparing typed addresses and control
flow targets as numeric `(shot, index)` pairs.

It does not normalize arbitrary strings. If an append retroactively changes an
earlier line identity, stage-injected command, payload, control-flow meaning, or
existing state-table entry, the candidate is rejected with
`E-STREAM-PREFIX-CHANGED`. Previously emitted IR is never amended.

## Consumer state and cursor rules

Treat an update as a new immutable program image:

1. replace the previous artifact snapshot;
2. rebuild the `addr -> command index` lookup, because padding may have changed;
3. retain the numerical command cursor, live state values, facts, selected
   control-flow stack, and host effect/idempotency records;
4. initialize only state slots that are newly declared; and
5. resume the ordinary choice/match/hub/jump dispatcher from the retained cursor.

Never reapply defaults for existing state or re-seed existing facts. Never use
`append_from` as “execute this entire suffix”: branches and jumps still select
which commands run.

When execution reaches the current stream frontier, wait. Reaching the end of the
latest snapshot does **not** complete the scene while the compiler remains open.
A scene becomes complete only after successful `finish`, unless an authored
ordinary `::end{reason="..."}` command terminates runtime execution earlier.
`::end` does not close stdin; EOF does not synthesize `::end`.

Any host side effect must remain idempotent across snapshot replacement. The
compiler neither records nor retries those effects.

## Errors and lifecycle

Appended-source diagnostics point into cumulative source: immutable template,
accepted units, then the failing unit. The compiler keeps ordinary parser,
checker, resolution, and compile diagnostics intact. The service adds:

| Code | Meaning |
| --- | --- |
| `E-STREAM-TEMPLATE` | The initial prefix cannot establish the required checked scene and artifact. |
| `E-STREAM-BODY` | Streamed source is not admitted as final-shot body content. |
| `E-STREAM-PREFIX-CHANGED` | A candidate would retroactively alter accepted commands or state. |
| `E-STREAM-CLOSED` | `push` or `finish` was called after the compiler became terminal. |

A forward reference that ordinary compilation cannot resolve yet is an error,
not a promise to retry after future input. On any rejection, the failing unit and
later input are not compiled and the instance is terminal. Construct a new
compiler from a corrected complete prefix to continue.

## Source references

- [Implementation contract](../superpowers/specs/2026-09-14-streaming-continuation-compiler-design.md)
- [Normative 0.17.0 proposal](../proposals/scenario-dsl/0.17.0.md)
- [`lute-compile` streaming implementation](../../crates/lute-compile/src/streaming.rs)
- [`lute-syntax` continuation framer](../../crates/lute-syntax/src/incremental.rs)
- [Ordinary runtime execution model](./execution-model.md)
