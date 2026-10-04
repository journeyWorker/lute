---
status: Accepted
---

# Quality audit after 0.36 - idiomatic-rust



## summary

Read-only idiomatic-Rust audit completed. The dominant architecture issues are JSON-as-domain-model in lute-model/lute-cli, stringly-typed error/API boundaries, synchronization-heavy memoization in lute-check, and a very large/low-reviewability patch implementation. Direct production panics are absent; most unwrap/expect matches are test modules or explicitly documented internal invariants.

## architecture

Lute has a mostly typed parser/compiler/checker core, but two boundaries undermine it: semantic diff/patch and CLI runner convert typed state to serde_json::Value and then reason over JSON keys; errors at several checker/benchmark boundaries are String rather than enums. Caching is correct-looking but over-synchronized and clone-heavy: a global Mutex protects maps whose values are Arc<OnceLock<V>>, while callers clone V. The patch module is a god-function/module: wire decoding, path validation, staging/copying, edit application, formatting, commit/rollback, semantic validation, and preserve checks are coupled in one file with many dense one-line expressions. No unsafe blocks or Box<dyn Error> matches were found. No direct panic! remained in non-test library code; panic-like behavior is concentrated in test helpers and invariant expects.

## report

## Counting method and caveat
Counts below are lexical matches in `crates/*/src`, with matches inside `#[cfg(test)]` modules/test functions manually excluded where identifiable. They are not a claim that every `expect` is a bug: several are justified internal invariants. `serde_json::Value` counts include legitimate wire-format handling, but the findings call out cases where it is traversed as domain data. No project-wide build/test was run, per the read-only research assignment.

## Totals per crate

| crate | non-test `.unwrap()`/`.expect()` | direct `panic!` | `Result<_, String>` / string errors | `Box<dyn Error>` | internal `serde_json::Value` matches | sync/cache matches | `#[allow]` |
|---|---:|---:|---:|---:|---:|---:|---:|
| lute-syntax | 5 | 0 | 0 | 0 | 0 | 0 | 0 |
| lute-check | 15 | 0 | 8 | 0 | 7 | 4 | 10 |
| lute-compile | 13 | 0 | 0 | 0 | 7 | 0 | 1 |
| lute-model | 3 | 0 | 4 | 0 | 14 | 0 | 0 |
| lute-resolve | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| lute-trace | 4 | 0 | 0 | 0 | 0 | 0 | 0 |
| lute-manifest | 8 | 0 | 2 | 0 | 2 | 0 | 2 |
| lute-core-span | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| lute-cli | 12 | 0 | 0 | 0 | 25 | 0 | 7 |
| lute-lsp | 0 | 0 | 0 | 0 | 0 | 0 | 2 |
| lute-lint | 1 | 0 | 0 | 0 | 0 | 0 | 0 |
| lute-wasm | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| lute-bench | 0 | 0 | 6 | 0 | 1 | 0 | 0 |
| lute-test-vocab | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| **total** | **61** | **0** | **20** | **0** | **60** | **4** | **22** |

The unwrap/expect total is dominated by inline test code in several large `lib.rs`/module files; the table excludes those. Production examples that remain are listed below. `panic!` search did find test assertions such as `crates/lute-check/src/inject.rs:982`, `crates/lute-syntax/src/datalog.rs:900`, and `crates/lute-model/src/patch.rs` tests, but none in non-test library paths.

## Ranked top 30 concrete sites

Ranking is payoff/cost, not merely match count. Each item includes evidence, consequence, direction, and estimated blast radius.

1. **`crates/lute-model/src/diff.rs:51-61` — JSON fields in the semantic change model.** Evidence: `pub before: Option<Value>`, `pub after: Option<Value>`, followed by custom serialization. This makes semantic meaning dynamically typed, permits invalid shapes until consumers inspect them, and forces clone/recursive traversal. Refactor toward typed `ChangePayload`/field enums with a separate wire serializer; retain `Value` only at the final JSON boundary. Blast radius: 6–10 files (`diff.rs`, `patch.rs`, CLI JSON consumers, model tests).

2. **`crates/lute-model/src/diff.rs:124-183` — diff index recursively treats `Value` as the internal model.** Evidence: `strip_positional(&mut Value)`, `DocumentCommandIndex { commands: Vec<Value> }`, and `if let Value::Object(map)`. This couples diff correctness to serialized field names and makes schema changes runtime-only failures. Refactor to index typed command/IR projections, with a small explicit `WireValue` adapter. Blast radius: 8–14 files.

3. **`crates/lute-cli/src/runner.rs:304-414` — activation state is a JSON tree built by mutation.** Evidence: `fn insert(node: &mut Json, parts: &[&str], value: Json)` and repeated `get_mut("map").and_then(Json::as_object_mut).unwrap()`. Every path allocates split vectors and JSON maps; malformed shape is handled through panicking unwraps. Refactor to a typed recursive `ActivationNode`/`BTreeMap<String, ActivationNode>`, serialize once. Blast radius: 5–9 files.

4. **`crates/lute-cli/src/runner.rs:438-471` — duplicated JSON-tree insertion implementation.** Evidence: `activation_json_paths` repeats `insert`, `json!({"map": {}})`, and recursive map mutation. Duplication creates semantic drift and doubles allocation-heavy hot code. Refactor both APIs through one typed tree builder with a policy for defaults/selected paths. Blast radius: 3–6 files.

5. **`crates/lute-model/src/patch.rs:301-437` — god-function `apply_patch_to`.** Evidence: one routine validates revisions, stages/copies trees, resolves targets, reads files, formats, detects overlaps, moves files, rebuilds models, diffs, and checks preserve contracts. This is difficult to test and makes changes high-risk. Split into `TargetPlan`, `EditPlan`, `StagedWorkspace`, `CommitPlan`, and `PreserveChecker`; keep one orchestration function. Blast radius: 8–15 files, API can remain stable.

6. **`crates/lute-model/src/patch.rs:391-423` — dense loops and repeated allocation/error conversion.** Evidence: `let mut by_file...; for edit in edits { ... }` and multiple same-line `read_to_string(...).map_err(...)`, `format!`, and `to_string()`. Reviewability and diagnostics suffer; temporary `String`s are created for every filesystem error and several paths. Refactor into named helpers returning `PatchRefusal`, with typed `PatchIoError` conversion and normal multi-line loops. Blast radius: 2–4 files.

7. **`crates/lute-check/src/schema_import.rs:327-350` — over-composed memo cache.** Evidence: `Mutex<HashMap<K, Arc<OnceLock<V>>>>`; `get_or_init` locks the map, clones the cell, then clones `V` from the cell. It combines a global lock, per-key one-time cells, and whole-value cloning. Refactor to `Mutex<HashMap<K, Arc<V>>>` if compute-under-lock is acceptable, or a concurrent map/explicit per-key state if avoiding lock-held initialization is required; return `Arc<V>` rather than `V`. Blast radius: 4–8 files and callers.

8. **`crates/lute-check/src/fact_env.rs:1368-1369` — large fact-set cache uses `Mutex` plus nested `Arc<Vec>`.** Evidence: `seed_derived: Arc<Vec<MustFact>>` and `memo: Mutex<HashMap<ClosureKey, Arc<Vec<MustFact>>>>`. Derived fact vectors are cloned/extended in `SlotFacts::facts` and `MustClosure::derived`. Refactor API to return `Arc<[MustFact]>`/`Arc<Vec<_>>` consistently and use a cache value with immutable ownership; avoid converting shared slices back to owned vectors. Blast radius: 4–7 files.

9. **`crates/lute-check/src/fact_env.rs:1484-1503` — `OnceLock<Vec<MustFact>>` still clones large data on assembly.** Evidence: `full: OnceLock<Vec<MustFact>>` and `all.extend(derived.iter().cloned())`. This is a deliberate lazy cache, but the cached vector is rebuilt per slot and clones each fact. Refactor to shared immutable fact blocks or a merged iterator/materialized `Arc<[MustFact]>` only when a consumer requires ownership. Blast radius: 3–6 files.

10. **`crates/lute-model/src/patch.rs:145-148` — `Value` clone during preserve decoding.** Evidence: `let (key, value) = object.iter().next().unwrap();` followed by `serde_json::from_value::<Vec<NodeKey>>(value.clone())` and `Vec<String>(value.clone())`. Large preserve arrays are copied before deserialization. Refactor to deserialize from borrowed `serde_json::value::RawValue`/typed tagged structs, or use one `PreserveWire` enum. Blast radius: 2–4 files.

11. **`crates/lute-model/src/patch.rs:113-125` — stringly node-kind parser returns `Result<NodeKey, String>`.** Evidence: `match kind { "project" => ... }` and `_ => return Err(format!(...))`. Callers cannot distinguish malformed syntax, unknown kind, or empty key. Refactor to `NodeKeyParseError { MissingSeparator, UnknownKind, EmptyKey }` with `Display`/`Error`; map it to serde errors only at the wire boundary. Blast radius: 3–5 files.

12. **`crates/lute-check/src/cel_expand.rs:38-39,87-88` — `Result<String, String>` expansion APIs.** Evidence: `pub fn ... -> Result<String, String>` and errors such as `"$"...to_string()`/`format!(...)`. This prevents structured handling of unknown refs, arity, cycles, and subject errors. Introduce `ExpandError` variants carrying name/expected/actual/span and implement `Error`. Blast radius: 4–8 files.

13. **`crates/lute-check/src/component_effects.rs:213-218` — public conversion uses `Result<FactTerm, String>`.** Evidence: `pub fn fact_arg_constant(arg: &AttrValue) -> Result<FactTerm, String>`. The match is a closed semantic set, so string errors are an avoidable loss of information. Use `FactArgError::{Reference, InvalidLiteral}` (or `Option` if callers only need rejection). Blast radius: 2–4 files.

14. **`crates/lute-check/src/beats.rs:989-1077` — validation helpers return stringly errors.** Evidence: `) -> Result<(), String>` and `) -> Result<(String, Vec<String>), String>`. These are checker paths with stable diagnostic categories, but callers must parse prose. Return a typed internal error carrying code/message data, converting to `Diagnostic` once. Blast radius: 4–7 files.

15. **`crates/lute-bench/src/main.rs:159-206,410-526` — benchmark CLI uses `Result<_, String>` end-to-end.** Evidence: `parse_args() -> Result<Config, String>`, `loop_until... Result<Duration, String>`, and `run_tests... Result<(), String>`. This conflates invalid CLI input, filesystem failures, compile failures, and test assertion failures. Introduce `BenchError` with `thiserror`-style variants or local enums and preserve source errors. Blast radius: 2–4 files.

16. **`crates/lute-trace/src/report.rs:607-608` — public serialization uses `expect`.** Evidence: `pub fn render_json(&self) -> String { serde_json::to_string_pretty(self).expect(...) }`. A future non-serializable field turns a reporting API into a process abort. Return `Result<String, serde_json::Error>` or make the infallibility guarantee a compile-time/wire-type property. Blast radius: 3–5 files.

17. **`crates/lute-compile/src/normalize.rs:705-721,738-739` — production `expect` on optional guards.** Evidence: `line.when.take().expect("caller guarantees...")` repeated for directive/set. The invariant is supplied by callers but not represented in types. Refactor callers to destructure `Some` and pass a `Guarded<T>` helper, or return `Option<Node>` if malformed state is reachable. Blast radius: 3–6 files.

18. **`crates/lute-compile/src/streaming.rs:102-117` — production parser ownership assertions.** Evidence: `.as_mut().expect("non-terminal compiler owns its parser")` and `.take().expect(...)`. These are likely valid state-machine invariants but panic on misuse. Encode terminal/nonterminal states as enum variants (`StreamingState::Open(Parser)`/`Finished`) and make transitions exhaustive. Blast radius: 3–5 files.

19. **`crates/lute-compile/src/locale.rs:97-99` — serialization `expect` in a public-ish wire renderer.** Evidence: `serde_json::to_string(&wire).expect("... infallible")`. The comment explains current field types, but future additions can invalidate the assumption. Return `Result` or use a dedicated infallible serializer boundary with a test that enforces the wire type. Blast radius: 2–3 files.

20. **`crates/lute-manifest/src/core.rs:67-71` — fixed initializer uses three `expect`s.** Evidence: embedded `MANIFEST`, `STAGING`, and `ENUMS` are parsed with `expect`. This is acceptable only if startup abort is intentional; otherwise use a `LazyLock<Result<CoreSnapshot, CoreLoadError>>` and surface a controlled initialization error. Because data is fixed, `LazyLock` is preferable to an instance `OnceLock`. Blast radius: 2–4 files.

21. **`crates/lute-manifest/src/project.rs:1127-1129` — `expect` after shape validation.** Evidence: `.as_str().expect("shape already checked by defaults_shape_ok")`. The check and extraction are separated, so refactoring either can reintroduce a panic. Return a typed validated shape or destructure the value in the same function. Blast radius: 2–4 files.

22. **`crates/lute-cli/src/cli.rs:457-460` — finite document kind is `String`.** Evidence: clap field `kind: String` with docs enumerating `scene`, `quest`, `lore`, `schema`. Typos reach later branching and repeated literal comparisons. Use `DocumentKind` implementing `ValueEnum`/`FromStr`; keep serialization/display separate. Blast radius: 4–8 files.

23. **`crates/lute-check/src/meta.rs:2493-2496` — state declaration kind is `String`.** Evidence: `pub kind: String`; related code carries kind names through maps and compares string literals. Use a closed enum where the vocabulary is finite, or a newtype plus validated constructor where plugins can extend it. Blast radius: 6–12 files.

24. **`crates/lute-compile/src/ir.rs:476-519` — `TargetKind` and `ForKind` carry `kind: String`.** Evidence: both structs expose `pub kind: String` while representing the same finite semantic concept. This permits inconsistent target/for values and repeated lookup/string allocation. Introduce a shared `EntityKindId`/validated newtype and convert to string only for wire serialization. Blast radius: 8–15 files.

25. **`crates/lute-lint/src/metrics.rs:53-65` — metric rows repeat authored kind as String.** Evidence: `LineRow.kind`, `SceneRow.kind`, and later `kind` fields are all `String`, while engine logic compares `== "scene"`. Use `DocKind`/`ProjectBeatKind` where the row is internal; serialize as string at the report boundary. Blast radius: 5–9 files.

26. **`crates/lute-check/src/cast.rs:322-325` — `&Vec<Node>` return type.** Evidence: `fn doc_bodies(doc: &Document) -> impl Iterator<Item = &Vec<Node>>`. This unnecessarily exposes vector representation and blocks slices/other sequence types. Return `impl Iterator<Item = &[Node]>` or use a small body accessor. Blast radius: 1–3 files.

27. **`crates/lute-check/src/fix.rs:94-99` — tuple soup for edits.** Evidence: `edits.push((te.span.byte_start, te.span.byte_end, te.new_text.clone()))`. Positional triples obscure meaning and encourage accidental field swaps. Introduce `PendingEdit { start, end, new_text }`; this also provides a natural place for overlap validation. Blast radius: 2–4 files.

28. **`crates/lute-check/src/chapters.rs:789-800` — too-many-arguments suppression hides a missing object.** Evidence: `#[allow(clippy::too_many_arguments)] fn stall(index, chain, ...)`. The suppression is on a core reachability/checking path. Bundle stable context into `ChapterCheckCtx` and pass a small `StallInput`; retain only domain-specific values as parameters. Blast radius: 3–6 files.

29. **`crates/lute-check/src/check/component_body.rs:682-697` — another large context walker suppressed rather than modeled.** Evidence: `#[allow(clippy::too_many_arguments)] pub(super) fn walk_component_body(nodes, snapshot, ...)`. This is a god-function boundary and likely drives future parameter growth. Introduce a context struct containing immutable environment and diagnostic sink, plus a narrow recursive walker. Blast radius: 4–8 files.

30. **`crates/lute-model/src/patch.rs:145,229-245,301-672` — extremely long one-line expressions.** Evidence includes `if object.len() != 1 { return Err(...) }`, one-line recursive copy/error paths, and a one-line `PatchRefusal` `Display` implementation. This is not formatting taste: line-level control flow hides ownership, error boundaries, and cleanup semantics, making correctness review and profiling difficult. Refactor into named helpers and ordinary blocks before optimizing. Blast radius: 2–5 files.

## Additional requested checks

- **Large clones / hot paths:** strongest confirmed sites are `diff.rs:51-61` (`Value` payloads), `diff.rs:124-183` (JSON tree/index cloning), `patch.rs:145-148` (`value.clone()` before deserialization), `fact_env.rs:1489-1501` (`Vec<MustFact>` assembly and `iter().cloned()`), `schema_import.rs:345` (whole cached `V` clone), and `lute-cli/src/cmd_context.rs:205-217` (`item.clone()` into multiple JSON result buckets). Refactor ownership/shared immutable payloads before micro-optimizing string formatting.
- **`to_string()`/`format!()` in loops:** `lute-model/src/patch.rs:391-423,493-499,505-535` allocates error strings and temporary paths inside filesystem/edit loops; `lute-cli/src/runner.rs:331-365,447-469` allocates path strings, split vectors, JSON nodes, and `format!("prev.{path}")` while building every activation response. These are measurable candidates for a benchmark after typed-tree refactoring.
- **`Rc<RefCell<_>>` / `Arc<Mutex<_>>`:** no `Rc<RefCell<_>>` matches. Production `Arc<Mutex<_>>`-like structures are localized to `lute-check/src/fact_env.rs:1368-1369` and `schema_import.rs:327-350`; the latter is the higher-payoff redesign because it clones whole cached values.
- **Stringly enums:** confirmed in `lute-cli/src/cli.rs:458`, `lute-check/src/meta.rs:2494`, `lute-compile/src/ir.rs:477,517`, `lute-core-span/src/lib.rs:168` (wire-facing quickfix kind is less urgent), and `lute-lint/src/metrics.rs:54,64`. Literal comparisons also occur in `lute-cli/src/cmd_constraints.rs:151`, `cmd_context.rs:181,206-217`, and `endings.rs:760-762`.
- **Bool flags / tuple APIs:** `lute-cli/src/cmd_compile.rs:31-39` and `cmd_trace.rs:34-42` have too-many-argument APIs including `json: bool`/mode flags; prefer command option structs. `lute-check/src/fix.rs:97` is the clearest tuple-soup case. Several `Vec<(String, String, bool)>`-style exact matches were not found, but adjacent tuple-heavy APIs exist and should be audited during refactors.
- **Manual loops:** no blanket conversion is recommended. The loops in graph traversal, parser state machines, patch application, and recursive JSON/tree construction are stateful and should remain loops. Allocation-heavy collection loops in `runner.rs` and patch filesystem/edit code are the worthwhile iterator/helper targets.
- **One-line `match self` methods:** `PatchRefusal::code` and `PatchRefusal::message` at `lute-model/src/patch.rs:191-202` are data-only projections and could be derived/generated or expanded into normal match blocks; this is low payoff compared with typed errors. `ChangeKind::rank/text` at `lute-model/src/diff.rs:32-42` is similar but currently supplies ordering/wire text, so retaining an impl is reasonable.
- **`#[allow(...)]`:** 22 source attributes were found. The meaningful architectural suppressions are the `too_many_arguments`/`type_complexity` attributes in `lute-check` and CLI dispatch/project collection, plus `lute-compile/src/lib.rs:881`. `#[allow(dead_code)]` in `lute-cli/src/runner.rs:305` should be removed if the function is genuinely unused or promoted to a tested API; `#[allow(deprecated)]` in LSP symbols is an external compatibility necessity; the test-only `non_snake_case` is harmless.
- **Unsafe:** no `unsafe {}`, `unsafe fn`, or `unsafe impl` matches in the crates.
- **`std::sync::Mutex` + `.lock().unwrap()`:** no direct `.lock().unwrap()` match. `schema_import.rs` uses `lock().unwrap_or_else(|e| e.into_inner())`, which is deliberate poison recovery. The architectural issue is lock granularity/value cloning, not poisoning behavior.
- **`OnceLock` vs `LazyLock`:** `lute-check/src/fact_env.rs:1484` and `lute-check/src/schema_import.rs:328` are per-instance/per-key lazy cells and should remain `OnceLock` unless ownership is redesigned. `lute-model/src/project.rs:94` is also per-model lazy state. Fixed embedded core data in `lute-manifest/src/core.rs:67-71` is the clear `LazyLock` candidate.
- **Manual `Default`:** several manual defaults are semantically meaningful (`Dnf`, `DomainUse`, `ScriptSource`) or needed to construct fields. The `Memo` default at `schema_import.rs:331-336` is boilerplate and can be `#[derive(Default)]` once bounds/field type permit; this is low payoff.
- **Public error enums:** `lute-model` already has `DiffError` and `PatchRefusal`, so the recommended direction is to extend that pattern rather than introduce a dependency-wide error abstraction. `Box<dyn Error>` was not found in public or private source APIs.

## Recommended order of work
1. Type the semantic diff/patch payloads and runner activation tree; this removes the most correctness risk and the largest repeated JSON traversal/allocation cost.
2. Split `apply_patch_to` and replace String errors with `PatchError`/`NodeKeyParseError` while preserving `PatchRefusal` at the external API.
3. Redesign `schema_import::Memo` and fact closure ownership to return shared immutable values instead of cloning large vectors.
4. Replace finite `kind: String` fields with validated enums/newtypes at internal boundaries.
5. Remove production invariant `expect`s by making state transitions/types explicit; retain only fixed-initializer failures where a controlled startup error is impossible and document that policy.

## files

- `crates/lute-model/src/patch.rs`: Patch wire deserialization, staged application, repeated JSON conversion, many long one-line expressions, and String-based internal errors.
- `crates/lute-model/src/diff.rs`: Semantic diff stores before/after as serde_json::Value and recursively indexes JSON objects as the semantic model.
- `crates/lute-cli/src/runner.rs`: Execution state and condition scope are represented and traversed as untyped JSON trees; duplicated recursive insertion code.
- `crates/lute-check/src/schema_import.rs`: Generic Memo uses Mutex<HashMap<K, Arc<OnceLock<V>>>> and clones every cached V on reads.
- `crates/lute-check/src/fact_env.rs`: MustClosure uses Arc<Vec>, Mutex<HashMap<..., Arc<Vec<...>>>>, and OnceLock<Vec<...>> around large fact sets.
- `crates/lute-compile/src/normalize.rs`: Production invariant assertions use expect(), although callers can encode the invariant in the type/control flow.
- `crates/lute-trace/src/report.rs`: Public render_json() uses expect() for serialization.
- `crates/lute-manifest/src/core.rs`: Fixed embedded YAML initializers use expect() during core snapshot construction.
- `crates/lute-check/src/cel_expand.rs`: Public-ish expansion functions expose Result<String, String>, losing typed failure information.
- `crates/lute-check/src/component_effects.rs`: fact_arg_constant() exposes Result<FactTerm, String> for a closed set of parse/type failures.
- `crates/lute-bench/src/main.rs`: Benchmark CLI and callbacks use Result<_, String> throughout.
- `crates/lute-cli/src/cli.rs`: CLI document kind is an unconstrained String despite a finite documented set.
