# Streaming continuation compiler

Date: 2026-09-14
Status: implementation contract; not a released-version claim

Companion documents: the
[prospective normative proposal](../../proposals/scenario-dsl/0.16.1.md), the
[runtime and user guide](../../runtime/incremental-continuations.md), and the
[implementation architecture](../../architecture.md#checked-streaming-continuation-compiler-prospective-0161-tooling).

## User outcome

A host keeps a checked scene prefix (frontmatter, project configuration and authored body), feeds append-only Lute body text, and receives checked ordinary Lute IR after each complete top-level line/directive/block, before transport EOF. No JSON AST patches and no new dialogue grammar. The CLI surface exposes the same API and flushes each accepted update to stdout.

## Scope and ownership

This feature is a compiler service, not an LLM client or game engine. Existing checker, normalization, component expansion, stage injection and addressing remain authoritative. Product-specific vocabulary, monetary authority, prompt calls and per-user persistence do not enter Lute core. Existing plugin/profile declarations still apply. This feature does not claim to add a secure AI capability sandbox.

## API contract

`lute_compile::streaming::ContinuationCompiler` owns a resolved `CheckInput`, frozen `IdentityTemplates`, the incremental body framer and the latest accepted artifact.

- `new(input: CheckInput, identity: IdentityTemplates) -> Result<Self, Vec<Diagnostic>>`: checks/compiles a complete scene prefix. Requires scene kind and at least one shot. Establishes the initial artifact; appends body only to the last existing shot. Prefix source is immutable.
- `artifact(&self) -> &Artifact`: latest accepted ordinary artifact.
- `push(&mut self, chunk: &str) -> ContinuationCompilation`: consumes valid UTF-8 text, returns zero or more accepted `CompilationUpdate`s followed by optional terminal errors. Complete units are processed in source order regardless of input chunk partition.
- `finish(&mut self) -> ContinuationCompilation`: treats EOF as delimiter for a final complete leaf, rejects incomplete syntax, and closes the compiler. After success no source can be appended. After failure the compiler is terminal.
- `CompilationUpdate { sequence: u64, append_from: usize, artifact: Artifact }`: sequence starts at 1 for the first body update. The snapshot is a normal IR artifact. append_from is the prior snapshot's command count, not a new runtime PC.
- `ContinuationCompilation { updates: Vec<CompilationUpdate>, need_more: Option<NeedMoreInput>, diagnostics: Vec<Diagnostic>, finished: bool }`.

Diagnostics from invalid appended source refer to the cumulative source (prefix + accepted units + failing unit); no synthetic shot shifts. A failing unit and all subsequent units produce no IR. Earlier accepted units in the same push remain returned; the host must not discard already accepted output because a later unit failed.

## Correctness before optimization

At each completed unit, compile the accumulated prefix using the EXISTING compiler, never an independent lowering algorithm. This is incremental output with cumulative re-analysis, not an asymptotically incremental compiler. It reduces first-output latency, not total compiler cost. One long unfinished block still waits for its close. Large sessions should be measured before optimization is promised.

## Irreversible output and compatibility

A newly compiled artifact may widen decimal address padding. Compare old/new commands after canonicalizing only typed address and control-target strings to unpadded numeric shot/index pairs. Never normalize arbitrary payload strings. Every prior command must be semantically identical and old state table entries must remain identical. If appended source retroactively changes a prior line identity, stage injection or other command, reject with E-STREAM-PREFIX-CHANGED; never amend emitted IR.

Consumers receive full ordinary artifact snapshots, not patch instructions. On each update they replace the immutable program snapshot, rebuild its address lookup and retain the numerical command cursor, state, facts and selected control-flow stack. They initialize only newly declared state slots. Existing state defaults and facts must NOT be reapplied.

`appendFrom` defines the newly added command-array region; it is NOT a request to execute every command in that slice. The ordinary choice/match/jump dispatcher selects execution paths. At the current stream frontier the host waits for another update; it does not mark the scene complete until successful `finish`, or an explicit ordinary `end` command. Host side effects use host-owned idempotency.

## Input admission and diagnostics

Body input cannot replace frontmatter, add shots/labels, or introduce quest roots. Existing legal shot-body grammar remains available, including closed branch, match, hub and timeline units, component use and plugin directives. A unit that requires a forward target not yet present fails the existing check/compile gate; the compiler does not guess future source or fabricate a closer. A physical newline is syntax, not an arbitrary network chunk boundary. Rust accepts `&str`; callers must decode split UTF-8 bytes before pushing. CLI performs valid UTF-8 decoding and rejects malformed input as I/O/input error.

New service diagnostics: E-STREAM-TEMPLATE, E-STREAM-BODY, E-STREAM-PREFIX-CHANGED, E-STREAM-CLOSED. Existing syntax/semantic diagnostics remain unchanged and unsuppressed. E-STREAM-* are service diagnostics, not new Lute grammar rules.

## CLI contract

`lute compile-stream <scene.lute> [--project DIR] [--providers DIR]`

Template is resolved once using the ordinary CLI project/provider/component/default/identity helpers. Body is read from stdin; template is never modified. stdout is newline-delimited JSON and MUST be flushed per update:

- `{"kind":"start","sequence":0,"appendFrom":0,"artifact":<ordinary IR>}`
- `{"kind":"update","sequence":N,"appendFrom":K,"artifact":<ordinary IR>}`
- `{"kind":"finish","sequence":N}` only on success
- `{"kind":"error","diagnostics":[...]}` on compiler rejection; no success marker

Exit 0 only successful EOF finalization; 1 syntax/semantic/service error; 2 invocation/I/O/invalid UTF-8. Broken stdout must stop reading rather than continue consuming input. stderr is for I/O errors. No remote calls, publishing or user effects.

## Acceptance and verification

1. Pipe a delayed multi-turn body: first update arrives while stdin remains open.
2. Reassemble final snapshot; existing `lute run` executes selected branch and final state correctly.
3. Same body under single-byte ASCII chunks, Unicode-character chunks and whole-buffer chunks yields identical accepted snapshots and final artifact.
4. State written in an earlier unit can be read in a later unit; stage state and component lowering retain ordinary compiler semantics.
5. Missing close/comment, unknown directive/provider, undeclared state and forbidden root/heading fail without exposing failing unit IR.
6. Address-width growth preserves prefix; later authored code that renumbers an earlier implicit line is rejected.
7. Prefix errors fail before body read; no calls after finish/failure accepted.
8. Project identity/defaults/providers/components are honored by CLI.
9. Document English and Korean website/API/CLI examples, normative proposal, runtime consumer rules and Unreleased note. No version bump or published-release claim until an explicit release is cut.
