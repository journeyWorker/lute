---
title: Streaming continuation compiler
description: Append ordinary Lute shot-body text to a checked scene template, receive flushed full-artifact snapshots with lute compile-stream, and preserve runtime cursor and state correctly.
---

The checked continuation compiler appends ordinary Lute **shot-body source** to
a host-owned scene template and returns a checked, ordinary IR artifact after
each complete unit—before stdin reaches EOF. Use it when an interactive author
or generator should produce executable, statically checked Lute incrementally.

It is not a streaming-only grammar and it does not emit JSON AST patches.
Dialogue remains an inline Lute content line:

```lute
@guide: The corridor lights wake one by one.
```

The [prospective normative proposal](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.16.1.md)
and [implementation design](https://github.com/journeyWorker/lute/blob/main/docs/superpowers/specs/2026-09-14-streaming-continuation-compiler-design.md)
describe an Unreleased toolchain feature, not a published-version claim.

## Run a complete stream

Create a trusted template. It must already be a valid `kind: scene` document
with at least one shot; continuation text is appended only to the final shot.

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

Now keep stdin open between two valid Lute dialogue lines:

```console
$ { printf '@guide: First checked line.\n'; sleep 1; printf '@guide: Second checked line.\n'; } \
    | lute compile-stream /tmp/live-scene.lute
{"kind":"start","sequence":0,"appendFrom":0,"artifact":{...}}
{"kind":"update","sequence":1,"appendFrom":1,"artifact":{...}}
{"kind":"update","sequence":2,"appendFrom":2,"artifact":{...}}
{"kind":"finish","sequence":2}
```

`{...}` is abbreviated here. Every `start` and `update` actually contains a
complete ordinary Lute artifact, and every record is valid newline-delimited
JSON. The command flushes each accepted update immediately, so the first update
arrives during the one-second pause rather than waiting for EOF.

For a project scene, use the same resolution inputs as ordinary compilation:

```console
$ printf '@guide: Project-resolved line.\n' \
    | lute compile-stream scenes/live.lute --project . --providers snapshots
```

The template is resolved once with the ordinary project/provider/component/
default/identity helpers and is never modified. Exit codes are:

| Exit | Meaning |
| --- | --- |
| `0` | stdin reached EOF and finalization succeeded. |
| `1` | Syntax, semantic, compile, or streaming-service rejection. |
| `2` | Usage, file I/O, broken stdout, or invalid UTF-8. |

stderr is reserved for invocation and I/O errors. If stdout breaks, the command
stops consuming stdin instead of continuing work whose output cannot be read.

## NDJSON records

```json
{"kind":"start","sequence":0,"appendFrom":0,"artifact":{}}
{"kind":"update","sequence":1,"appendFrom":7,"artifact":{}}
{"kind":"finish","sequence":1}
```

- `start` is the checked initial template artifact.
- Each `update` is one accepted complete top-level body unit, in source order.
- `finish` appears only after successful EOF finalization.
- On rejection, the terminal record is
  `{"kind":"error","diagnostics":[...]}` and no `finish` follows.

`appendFrom` is the previous artifact's `commands.length`. It describes where
the newly appended array region starts. It is **not** a runtime program counter
and does not mean “execute every command after this index.” Ordinary
choice/match/hub/jump control flow still selects the path.

## Rust API

The checked API is `lute_compile::streaming`:

```rust
pub struct ContinuationCompiler { /* private */ }

impl ContinuationCompiler {
    pub fn new(
        input: CheckInput,
        identity: IdentityTemplates,
    ) -> Result<Self, Vec<Diagnostic>>;

    pub fn artifact(&self) -> &Artifact;
    pub fn push(&mut self, chunk: &str) -> ContinuationCompilation;
    pub fn finish(&mut self) -> ContinuationCompilation;
}

pub struct CompilationUpdate {
    pub sequence: u64,
    pub append_from: usize,
    pub artifact: Artifact,
}

pub struct ContinuationCompilation {
    pub updates: Vec<CompilationUpdate>,
    pub need_more: Option<NeedMoreInput>,
    pub diagnostics: Vec<Diagnostic>,
    pub finished: bool,
}
```

`CheckInput` includes the complete prefix text and its resolved capability,
provider, import, component, defaults, URI, and analysis inputs.
`IdentityTemplates` is frozen at construction. `new` checks and compiles the
prefix immediately; a non-scene, missing shot, or existing gate failure returns
diagnostics before any continuation is read. `artifact()` exposes the latest
accepted full artifact.

A single `push` can accept several complete units. If a later unit in that same
call fails, earlier accepted updates still appear in `updates`; the failing
unit and everything after it produce no IR. Keep those earlier updates.

## What body text is admitted

The continuation may use the existing legal body grammar:

- inline dialogue and narration, `@speaker{attributes}: text`;
- ordinary directives;
- complete `<branch>`, `<match>`, `<hub>`, and `<timeline>` blocks;
- component use and resolved plugin directives.

It cannot replace frontmatter, add a shot heading or label, or introduce a quest
root. A forward target that ordinary compilation cannot resolve yet fails now;
the compiler does not guess future source or fabricate a closer.

Keep the template and resolved project inputs under host control. Lute validates
the streamed body, but validation does not make generated text trusted. The
compiler is not an LLM client, game engine, permission system, or secure AI
capability sandbox. It performs no remote call, bridge effect, publication, or
player-state persistence.

## Chunks, units, and EOF

Transport chunks are not syntax. A `push(&str)` may stop inside a physical line,
quoted attribute, block comment, or nested element. The compiler emits a leaf
only after a physical newline, and emits an outer block only after its matching
close reaches a physical-line boundary. `need_more` reports the retained suffix
as `Line`, `QuotedAttribute`, `BlockComment`, or
`NestedBlock { open_tags }`; that is framing state, not a diagnostic.

Rust accepts `&str`, so callers receiving raw bytes must decode and buffer split
UTF-8 before `push`. The CLI performs this decoding and rejects malformed input
with exit `2`.

`finish()` treats EOF as the delimiter for one final complete leaf even without
a newline. EOF never invents `</tag>`, `*/`, or a forward target. An incomplete
block is rejected, and success or failure closes the compiler; later calls are
`E-STREAM-CLOSED`.

EOF is not Lute `::end`:

- EOF closes compiler input and permits the `finish` record on success.
- `::end{reason="complete"}` is an ordinary compiled command that can terminate
  runtime execution earlier. It neither closes stdin nor excuses invalid source
  that arrives later.

## Cumulative checking cost

Every complete unit runs the accumulated template and accepted body through the
**existing** parser, checker, normalization, component expansion, stage
injection, lowering, address assignment, and artifact assembly. There is no
second lowering algorithm to drift from `lute compile`.

This improves time to first checked output, not total compiler complexity. Unit
one compiles prefix one, unit two compiles the longer prefix two, and so on. A
long unclosed block emits nothing until its close. Measure large sessions; do
not assume constant work per update or describe this as an asymptotically
incremental compiler.

## Replace snapshots without resetting runtime state

Each update is a complete immutable program snapshot. A consumer must:

1. replace the old artifact with `update.artifact`;
2. rebuild `addr -> command index` lookup;
3. retain its **numeric command cursor**, live state, facts, selected control-flow
   stack, and host-owned effect/idempotency records;
4. initialize only newly declared state slots; and
5. resume the ordinary dispatcher from the retained cursor.

Do not reapply defaults for existing state or seed facts. Do not start executing
at `appendFrom` merely because records are new.

Rebuilding address lookup matters because Lute uses uniform address padding.
More commands can widen every address—`001-0900` may become `001-00900`—without
changing its numeric `(shot, index)` meaning. The compiler accepts that
formatting-only widening. It never normalizes arbitrary payload strings.

If appended source would change an earlier line identity, stage-injected
command, payload, control-flow meaning, or existing state entry, the candidate
is rejected as `E-STREAM-PREFIX-CHANGED`; already observed IR is never amended.

At the current command frontier, wait for an update. Do not mark the scene
complete simply because the latest snapshot currently ends. Completion follows
successful compiler `finish`, or an ordinary authored `end` command. Host side
effects must remain idempotent across snapshot replacement.

## Diagnostics

Ordinary syntax, semantic, resolution, and compile diagnostics remain intact and
refer to cumulative source: template, accepted units, then the failing unit.
The streaming service adds:

| Code | Meaning |
| --- | --- |
| `E-STREAM-TEMPLATE` | The initial input cannot establish the required checked scene prefix and artifact. |
| `E-STREAM-BODY` | Appended text is outside the final-shot body surface. |
| `E-STREAM-PREFIX-CHANGED` | A candidate would retroactively change accepted commands or state. |
| `E-STREAM-CLOSED` | The caller used an instance after success or failure made it terminal. |

## Lower-level parser API

If a tool needs lossless syntax framing but no artifact, use
`lute_syntax::incremental::IncrementalContinuationParser`. Its
`ContinuationUnit` preserves exact source, a stream-relative half-open byte
range, `Vec<Node>`, and syntax diagnostics local to that source. Its
`ContinuationFinalization` separates an unclosed suffix as
`IncompleteContinuation`.

That API intentionally performs no project or provider resolution, semantic
checking, component merge, lowering, addressing, or IR assembly. It is useful
parser documentation, but it is not a substitute for `ContinuationCompiler`
when a runtime will consume the result.

## Source contracts

- [Checked continuation runtime guide](https://github.com/journeyWorker/lute/blob/main/docs/runtime/incremental-continuations.md)
- [Prospective normative proposal](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.16.1.md)
- [`lute-compile` streaming source](https://github.com/journeyWorker/lute/blob/main/crates/lute-compile/src/streaming.rs)
- [`lute-syntax` continuation parser](https://github.com/journeyWorker/lute/blob/main/crates/lute-syntax/src/incremental.rs)
