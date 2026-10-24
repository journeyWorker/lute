---
status: Accepted
---

# Rust coding rules

- **Inputs:** four external guides, analyzed 2026-10-23: Microsoft *Pragmatic
  Rust Guidelines* (2026.6, ids `M-*`), Google *Rust Style Guide*, *Rust Design
  Patterns* (rust-unofficial), and Ed Page's *Rust Style* (ids `P-*`, `M-*`,
  `F-*`). The per-guide analyses are kept outside the repo, under
  `projects/lute/2026-10-23-rust-guidelines-research/` in the `journey-temp`
  artifact area (S3 and External).
- **Scope:** the Rust code under `crates/`. Lute's own `lute lint`
  (`crates/lute-lint`, `docs/linting.md`) checks `.lute` content and is not
  covered here.
- **Schedule:** see the release order in
  `docs/superpowers/notes/2026-10-23-lute-0.49-dogfood-triage.md`. 0.49
  rewrites the `lute-check` fold and vocabulary and the `lute-model`
  ownership. Once this note is `Accepted`, new 0.49 code follows it, but
  existing code is not swept. 0.49.1 applies the rules to all the code in one
  behaviour-preserving pass (§9). `lute-check` and `lute-model` together hold
  463 of the 2,102 warnings measured in §1, so a sweep before 0.49 would
  polish code that is about to be replaced and conflict with that rewrite.

## 0. Decisions

| Topic | Decision |
|---|---|
| Lints | Lints are declared once in `[workspace.lints]`, and every crate sets `lints.workspace = true`. `clippy.toml` holds the lint configuration. The pedantic group is not enabled as a whole; the lints listed in §1 are. CI keeps `-D warnings`. |
| Module files | `foo/mod.rs` is used for every module that has submodules. `foo.rs` next to `foo/` is not. |
| Visibility | Three levels: private, `pub(crate)`, `pub`. `pub(super)` and `pub(in …)` are not used. |
| Error types | Author mistakes are `Diagnostic`s. Rust error types are typed, per failure domain, and implement `std::error::Error`. A `pub fn` does not return `Result<_, String>`. No `anyhow` or `thiserror`. |
| Panics | A panic means a bug. Code outside tests does not call `unwrap()`. `expect` and `unreachable!` say why the failure cannot happen. No new `catch_unwind`. |
| Newtypes | A newtype is required when values can be confused, when a value carries an invariant, or when a value carries a unit. Wire bytes do not change. |
| HIR / Fold | There is no HIR and no Fold pass in 0.49.1. AST passes keep rewriting the tree in place. A traversal of every slot goes through `lute_syntax::walk`. |

## 1. Lints

### Configuration

- The workspace root `Cargo.toml` gets `[workspace.lints.rust]` and
  `[workspace.lints.clippy]`, and each of the 21 crates gets
  `[lints] workspace = true` (MS M-CARGO-WORKSPACE).
- Every lint is set to `warn` except `unsafe_code`, which is `deny`: there is
  no `unsafe` in the crates today, and a new block needs a reviewed
  `#[expect(unsafe_code, reason = "…")]`. The CI clippy job's `-D warnings`
  turns the warnings into failures. This follows *Design Patterns*
  "`#![deny(warnings)]`": the deny lives in CI, not in source. No
  crate-level `#![deny]` is added.
- The lint names were checked on the pinned toolchain 1.96.1, the same one
  the `clippy` job in `.github/workflows/test.yml` uses. When that pin is
  bumped, these lists are re-checked in the same change.
- A local exception is written as `#[expect(lint, reason = "…")]`, never
  `#[allow]` (MS M-LINT-OVERRIDE-EXPECT). The `clippy` job's comment already
  asks for this; the `allow_attributes*` lints enforce it.

```toml
[workspace.lints.rust]
ambiguous_negative_literals = "warn"
missing_debug_implementations = "warn"
redundant_imports = "warn"
redundant_lifetimes = "warn"
trivial_numeric_casts = "warn"
unsafe_code = "deny"
unsafe_op_in_unsafe_fn = "warn"
unused_lifetimes = "warn"

[workspace.lints.clippy]
# restriction
allow_attributes = "warn"
allow_attributes_without_reason = "warn"
as_pointer_underscore = "warn"
assertions_on_result_states = "warn"
clone_on_ref_ptr = "warn"
deref_by_slicing = "warn"
disallowed_script_idents = "warn"
empty_drop = "warn"
empty_enum_variants_with_brackets = "warn"
empty_structs_with_brackets = "warn"
fn_to_numeric_cast_any = "warn"
if_then_some_else_none = "warn"
map_err_ignore = "warn"
redundant_type_annotations = "warn"
renamed_function_params = "warn"
self_named_module_files = "warn"
semicolon_outside_block = "warn"
undocumented_unsafe_blocks = "warn"
unnecessary_safety_comment = "warn"
unnecessary_safety_doc = "warn"
unneeded_field_pattern = "warn"
unused_result_ok = "warn"
# pedantic, selected
cast_lossless = "warn"
cast_possible_truncation = "warn"
cast_possible_wrap = "warn"
cast_sign_loss = "warn"
cloned_instead_of_copied = "warn"
enum_glob_use = "warn"
explicit_iter_loop = "warn"
implicit_clone = "warn"
manual_let_else = "warn"
map_unwrap_or = "warn"
needless_for_each = "warn"
redundant_closure_for_method_calls = "warn"
semicolon_if_nothing_returned = "warn"
uninlined_format_args = "warn"
unnested_or_patterns = "warn"
wildcard_imports = "warn"
```

`clippy.toml` at the workspace root:

```toml
disallowed-methods = [
  { path = "core::iter::Iterator::for_each", reason = "side effects belong in a `for` loop" },
  { path = "core::iter::Iterator::try_for_each", reason = "side effects belong in a `for` loop" },
]
```

`clippy::unwrap_used` is not in `[workspace.lints]`, because integration-test
helpers under `crates/*/tests/` hold 935 `unwrap()` calls. Clippy's
`allow-unwrap-in-tests` covers only `#[test]` functions and `#[cfg(test)]`
modules, not those helpers. 0.49.1 adds a second command to the `clippy` job in
`.github/workflows/test.yml` (it is not there today) that runs the lint over
production targets only:

```sh
cargo clippy --workspace --lib --bins --locked -- -D warnings -D clippy::unwrap_used
```

### Measured cost (HEAD `b8b7cdba`, toolchain 1.96.1, `--all-targets`)

`src` counts include inline `#[cfg(test)]` modules; `tests` counts only
`crates/*/tests/`. A lint marked autofix offers a machine-applicable
suggestion (`cargo clippy --fix`) at every site.

| Lint | Source | src | tests | Autofix |
|---|---|---:|---:|---|
| `semicolon_if_nothing_returned` | pedantic | 109 | 2 | yes |
| `wildcard_imports` | Google, MS M-NO-GLOB-REEXPORTS | 107 | 0 | yes |
| `cast_possible_truncation` | §5 panic policy | 97 | 12 | |
| `map_unwrap_or` | pedantic | 84 | 17 | 97 of 101 |
| `missing_debug_implementations` | MS M-PUBLIC-DEBUG | 82 | 0 | |
| `disallowed_methods` (`for_each`) | Google, Ed Page F-COMBINATOR | 80 | 16 | |
| `redundant_closure_for_method_calls` | pedantic | 77 | 30 | yes |
| `clone_on_ref_ptr` | MS M-AVOID-WRAPPERS | 42 | 0 | |
| `allow_attributes_without_reason` | MS M-LINT-OVERRIDE-EXPECT | 34 | 2 | |
| `allow_attributes` | MS M-LINT-OVERRIDE-EXPECT | 33 | 1 | yes |
| `redundant_imports` | MS | 31 | 4 | |
| `cast_lossless` | pedantic | 27 | 4 | yes |
| `cast_possible_wrap` | §5 panic policy | 27 | 1 | |
| `renamed_function_params` | MS | 27 | 0 | |
| `assertions_on_result_states` | MS | 23 | 5 | yes |
| `cast_sign_loss` | §5 panic policy | 21 | 0 | |
| `unnested_or_patterns` | pedantic | 20 | 4 | yes |
| `deref_by_slicing` | MS | 16 | 0 | yes |
| `manual_let_else` | pedantic | 16 | 1 | |
| `unneeded_field_pattern` | MS | 14 | 0 | |
| `map_err_ignore` | MS M-FROM-ERROR | 12 | 3 | |
| `implicit_clone` | pedantic | 10 | 2 | yes |
| `unwrap_used` (production job) | Google, MS M-PANIC-ON-BUG | 9 | — | |
| `explicit_iter_loop` | pedantic | 8 | 0 | yes |
| `self_named_module_files` | Ed Page P-MOD | 8 | 0 | |
| `if_then_some_else_none` | MS | 7 | 0 | yes |
| `uninlined_format_args` | pedantic | 7 | 7 | yes |
| `enum_glob_use` | Google | 3 | 0 | yes |
| `needless_for_each` | Google | 3 | 0 | |
| `redundant_type_annotations` | MS | 2 | 2 | |
| `cloned_instead_of_copied` | pedantic | 1 | 0 | yes |
| `unnecessary_safety_comment` | MS M-UNSAFE | 1 | 0 | |
| `unused_result_ok` | MS | 0 | 16 | |
| `unsafe_code` and the remaining lints in the block | MS M-UNSAFE | 0 | 0 | |

The total is 1,167 sites outside `crates/*/tests/` plus the test sites
listed. About 600 of them are autofix.

### Not enabled, and why

| Lint or group | Sites | Reason |
|---|---:|---|
| `clippy::pedantic` as a group | 4,632 production, all candidate lints | Most of the count is the lints below. The useful pedantic lints are enabled one by one above. |
| `indexing_slicing` | 1,070 production | Compiler code indexes vectors whose bounds the code itself establishes. Replacing those indexes with `get(..).expect(..)` adds noise without finding bugs. |
| `must_use_candidate` | 855 | The crates have no external consumers (§4). The cost does not pay. |
| `too_long_first_doc_paragraph` | 702 | Rustdoc is not published, and the long crate docs are deliberate. |
| `format_push_string` | 196 | The fix, `write!` into a `String`, returns a `Result` that §5 would then require an `expect` for. Hot spots are handled by profiling, not by a lint. |
| `expect_used`, `panic`, `unreachable` | 65 / 3 / 30 production | These are the sanctioned panic forms (§5). |
| `cast_precision_loss` | 50 production | Integer-to-float conversions feed metrics and numeric values. The dangerous directions, float-to-int and narrowing, are already covered by `cast_possible_truncation`, `cast_possible_wrap` and `cast_sign_loss`. |
| `missing_errors_doc`, `missing_panics_doc` | 98 / 13 | The crates are internal. Error and panic conditions are documented where they are not obvious. |
| `clippy::cargo` | 309 | Crates are not published to crates.io (§4); the lints are about crates.io metadata. |
| `mod_module_files` | — | The opposite of §2. |

## 2. Module files

- Decision: a module with submodules lives in `foo/mod.rs` (Ed Page P-MOD).
  `clippy::self_named_module_files` enforces this.
- Today `crates/*/src` has 19 `mod.rs` modules and 8 self-named ones
  (`crates/lute-cli/tests/support/mod.rs` is test support and out of scope):
  - `lute-check`: `cast/presence.rs`, `connectivity.rs`, `project_check.rs`
  - `lute-lsp`: `backend.rs` and `backend/tests.rs`
  - `lute-model`: `patch.rs`
  - `lute-syntax`: `parser.rs`
  - `lute-trace`: `trace.rs`

  The majority form wins, and 0.49.1 moves those 8 files.
- Reasons for this form: a module and its children move and rename as one
  directory, and a plain `foo.rs` is always a leaf.
- A `mod.rs` or `lib.rs` that has submodules holds module declarations,
  re-exports and the module's namesake item, plus the small amount of code
  that ties the submodules together (Ed Page P-DIR-MOD, Google "non-leaf
  modules"). 0.49.1 does not move existing logic out of `lib.rs` files. New
  modules follow the rule.
- No `#[path]` attribute, except for files generated by `build.rs` (Ed Page
  P-PATH-MOD).

## 3. Visibility

- Decision: an item is private, `pub(crate)` or `pub` (Ed Page
  P-VISIBILITY). `pub(super)` and `pub(in path)` are not used.
- Today there are 698 such sites: `lute-check` 286, `lute-cli` 188,
  `lute-trace` 146, `lute-runtime` 29, `lute-syntax` 27 and `lute-lsp` 22.
  0.49.1 rewrites them to `pub(crate)`. Runtime behaviour cannot change, since
  this only widens visibility inside one crate. Compile-time results can:
  an item that becomes visible to the whole crate may change `dead_code`
  reachability, or expose a private type in its signature
  (`private_interfaces`). The step is done when the clippy job is clean
  again; a type newly reachable through a signature is made `pub(crate)` too,
  never re-hidden with `pub(super)`.
- Reason: the crate is the unit of encapsulation that matters (cargo, the API
  gate, the crate-split decisions). Visibility levels finer than the crate add
  edits to every refactor and protect nothing a reviewer relies on. If a
  module needs to be walled off from the rest of its crate, that is a reason
  to make it a crate, as was done for `lute-semantic` and `lute-load`.
- No clippy lint forbids `pub(super)`. 0.49.1 adds a check under `scripts/` that
  rejects `pub(super)` and `pub(in` in `crates/*/src`, and the `clippy` job
  runs it. `clippy::redundant_pub_crate` (nursery) contradicts this rule and
  stays off.
- `pub` items that are not re-exported from the crate root are not an error
  in themselves. The `pub` surface stays subject to the existing API gate.

## 4. Error types

The crates have no external consumers: they ship only inside the npm binaries
(lute-model-crate-split, owner decision 1), and D6 applies. Rules written to
keep a library stable under semver (MS M-ERRORS-CANONICAL-STRUCTS, Google
"error types `Copy`") therefore do not apply.

- **Author mistakes are `Diagnostic`s.** Problems in `.lute` source, project
  declarations, plugin manifests and bridge YAML are reported as
  `lute_core_span::Diagnostic` values with stable codes (D9, `lute
  --explain`). A Rust error type never carries an author mistake to the user.
- **Rust error types are for failures that are not author mistakes:** I/O,
  malformed artifacts or host messages, and internal failures between crates.
  - One type per failure domain, named `<Domain>Error`, for example
    `ProjectLoadError`, `RevisionError` or `IndexError`. 25 such types exist
    today.
  - A `pub enum` is allowed and stays the default, because callers match
    variants to choose a diagnostic code or an exit code.
  - Every error type implements `Debug`, `Display` and `std::error::Error`.
    `source()` returns the wrapped cause. Today 16 of the 25 types lack the
    `Error` impl, and 11 of those also lack `Display`; 0.49.1 adds them:
    `CelParseError`, `IndexError`, `EvalError`, `DatalogError`, both
    `ResolveError`s (`lute-manifest`, `lute-resolve`), `OptionError`,
    `DecomposeError`, `LoadError`, `ManifestError`, `AssembleError`,
    `SeedError`, `RenameError`, `FormatError`, `IsLiteralError`,
    `CommentError`.
  - Conversions that `?` needs are `From` impls (MS M-FROM-ERROR), not
    repeated `map_err` calls. `clippy::map_err_ignore` rejects a `map_err`
    that discards the cause.
- **No `Result<_, String>` from a `pub fn`.** There are 20 such functions
  today and 89 `Result<_, String>` signatures in total. 0.49.1 gives the 20
  public ones typed errors. A private helper may return a `String` when that
  string becomes the diagnostic message itself.
- **No new error crates.** `anyhow`, `eyre` and `thiserror` are not
  dependencies today:
  - The CLI returns an `ExitCode` and reports through diagnostics, so it does
    not need `anyhow` (MS M-APP-ERROR would allow it, but nothing calls for
    it).
  - The 16 hand-written `Display` impls are short.
  - Revisit if the number of error types grows past what hand-written impls
    can keep consistent.
- **No `Backtrace` field.** A panic carries the backtrace for bugs (§5), and
  every other failure carries its cause through `source()`.

## 5. Panics, `expect` and `unwrap`

| Situation | Response |
|---|---|
| Author input: source, project declarations, plugin manifests, bridge YAML | A `Diagnostic`. Never a panic. D11's "parser never panics on arbitrary input" property test stays the guard. |
| Input from outside the toolchain: an IR artifact read by `lute-runtime`, a host or bridge message, a file system error | An `Err` of a typed error (§4). |
| A broken internal invariant, that is, a bug | A panic with a message that states the invariant. |
| Tests | `unwrap()` is fine. |

- **`unwrap()` is not used outside tests.** The production clippy job (§1)
  enforces this. Today there are 9 sites: `lute-model` 5, `lute-cli` 3 and
  `lute-lint` 1.
- **`expect` says why the failure cannot happen,** not what went wrong:
  `expect("slot ids are assigned by lute_cel::fill before lowering")`, not
  `expect("missing slot id")` (Google "prefer `expect`", MS M-PANIC-MESSAGE).
  `unreachable!` and `panic!` carry the same kind of message.
- **Narrowing `as` casts are checked.** `u32::try_from(n).expect("…")`, or
  `#[expect(clippy::cast_possible_truncation, reason = "…")]` where the
  range is proven nearby. A silently truncated address or id would change
  the IR, which is a public contract (D3).
- **`catch_unwind`:** no new use in production code (`crates/*/src` outside
  `#[cfg(test)]`). The inventory today:
  - `lute_cel::parse_prepared` (`crates/lute-cel/src/lib.rs`), the only
    production use. It contains a third-party panic (cel-parser hits
    `unreachable!()` on malformed input) so the LSP survives every
    keystroke. It stays and keeps its comment.
  - Tests that assert "does not panic": the formatter fuzz test in
    `crates/lute-syntax/src/format.rs`, `crates/lute-check/tests/parser.rs`
    and three sites in `crates/lute-compile/tests/e2e.rs`. Tests may keep
    using it for that purpose.
  - A future production use needs the same kind of justification: containing
    a dependency's panic, never recovering from our own bug (MS
    M-PANIC-CONTINUATION, Google).
- **Poisoned locks propagate the panic.** Call `lock().expect("… lock
  poisoned")`. Never recover the guard with `into_inner` (Google).

## 6. Newtypes

A distinct type is required when one of the following holds (Google
"newtypes vs aliases", MS M-STRONG-TYPES and M-STRONG-TYPES-GUARD):

1. **Confusable values.** Two values with the same primitive type appear in
   one signature or one struct and mean different things: ids, offsets,
   addresses, indexes into different tables. Existing examples are
   `StableId`, `cfg::Label` and `CelAstHandle`.
2. **A unit or coordinate system.** Byte offset versus UTF-16 column versus
   line number, or milliseconds versus beats. `lute-resolve` already treats
   byte offsets as authoritative and converts at the boundary. That boundary
   is the first place to apply this rule when the code is next touched.
3. **An invariant.** Not every value of the underlying type is valid, for
   example a non-empty id or a normalized path.
   - The field is private.
   - Construction goes through `TryFrom` or `FromStr`. An infallible `From`
     from the underlying type is not allowed.
   - A panicking `const fn new` may exist in addition, for constants.

Constraints:

- **Wire bytes and schemas do not change.** A newtype on a serialized field
  uses `#[serde(transparent)]`. The schema check in CI (`git diff
  --exit-code schemas/`) and the byte-identity matrix must stay clean.
- A newtype without an invariant may keep a `pub` field, as `StableId(pub
  u64)` does.
- A plain type alias (`type X = Y`) is not a substitute for any of the three
  cases above.
- 0.49.1 does not introduce newtypes across the codebase; type changes are not
  a mechanical sweep. The rule binds new code and code being changed.

## 7. HIR, Fold and Visitor

- **No HIR.** D3 names three representations: source with its lossless AST,
  the project semantic model, and the execution IR. A desugared HIR between
  the AST and the IR would be a fourth one.
  - Today `lute-compile` rewrites the AST in place:
    - `normalize::normalize_document(&mut Document, …)` binds components and
      desugars `when=` and `into=`.
    - `expand::expand_document` and `expand::fold_attr_refs` expand
      definitions and attribute references.
    - `lower::*` then lowers the result into IR commands.
  - `lute-trace` calls the same `normalize_document` before
    `expand_document`, so it sees the tree that compile sees, without a
    second copy of the logic.
  - That sharing already covers the case a HIR would serve, and the AST must
    stay lossless for D2 (edits apply to source text) and for the formatter.
- **No Fold pattern.** A Fold consumes one tree and builds a new one, which
  allocates a second tree per document per pass. The in-place `&mut` passes
  give the same result without that cost. The `fold_*` functions in
  `lute-check` (`fold_env`, `fold_seasons`, …) compute environments; they are
  not the Fold pattern and are not renamed.
- **One shared traversal (Visitor).** Code that must reach every CEL slot uses
  `lute_syntax::walk::for_each_cel_slot` or `for_each_cel_slot_mut`, which
  are the single source of truth for slot order (`lute_cel::fill` assigns
  `StableId`s in that order). It does not add a new recursive matcher. A new
  traversal over another node set is added to `walk.rs` in the same style:
  free `walk`-style functions, with no `accept` methods on nodes.
- **Revisit when:**
  - the 0.49 `lute-check` rewrite produces a resolved tree that the checker,
    compile and trace all consume; or
  - a pass must drop source-only information that the LSP or formatter still
    needs; or
  - the ordering contract "`normalize_document` before `expand_document`"
    gains a third step. At that point, encode the phase in the type
    (typestate, *Design Patterns* "Generics as type classes") before
    introducing a separate tree.

Any of these amends D3 first.

## 8. Not decided here

- Edition 2024 (MS M-LATEST-EDITION).
- `imports_granularity`: nightly-only in rustfmt.
- Moving unit tests to `tests/` (MS M-INTEGRATION-TESTS).
- A faster hasher for internal maps (MS M-FAST-HASHER, 41 `HashMap` uses).
  Change only with a benchmark from the D11 corpus.

Each of these needs its own measurement or decision and is not part of the
0.49.1 sweep.

## 9. 0.49.1 sweep

The sweep is behaviour-preserving and runs with no other branch open. Each
step lands as its own commit and, on its own, passes the full suite, the
clippy job and the byte-identity matrix. Formatting-only commits go into
`.git-blame-ignore-revs`.

1. Add the lint configuration (§1), `[lints] workspace = true` in every
   crate, `clippy.toml`, and the production `unwrap_used` command in
   `.github/workflows/test.yml`. Apply `cargo clippy --fix` for the autofix
   lints.
2. Fix the remaining lint sites by hand: casts, `for_each`, `Debug` impls,
   `allow` → `expect`, `Arc::clone`, `wildcard_imports` and the production
   `unwrap()` calls.
3. Move the 8 self-named module files to `mod.rs` (§2).
4. Rewrite `pub(super)` and `pub(in …)` to `pub(crate)`, resolve what that
   surfaces in `dead_code` and `private_interfaces` (§3), and add the
   `scripts/` check.
5. Give the 20 public `Result<_, String>` functions typed errors, and add the
   missing `Display` and `Error` impls (§4).

Done means the `clippy` job, including the production `unwrap_used`
command, passes with the configuration above, and the counts in §1, §2, §3
and §4 are zero.

The owner renumbered the sweep from 0.50 to 0.49.1 on 2026-10-25. It ships
as a patch on the 0.49 line. The counts in §1–§5 above were measured at
`b8b7cdba` and are kept as the record of this note's analysis. The sweep
starts from the re-measure at `f34f141f` (v0.49.0) in the header of
`docs/superpowers/plans/2026-10-25-lute-0.49.1-coding-rules.md`.

## Amendments (2026-10-25)

These were found while planning the sweep on top of 0.49. They override the
text above where the two differ.

1. **Lints are enabled in stages.** §9 step 1 adds the whole §1 block before
   step 2 fixes the hand sites. That cannot pass the `clippy` job's
   `-D warnings` on every commit, because about 608 sites have no
   machine-applicable fix. Each step instead adds to `[workspace.lints]`
   exactly the lints it has brought to zero. `clippy.toml` lands with the
   `for_each` fixes, because creating it turns the 96 `for_each` sites into
   warnings (`disallowed_methods` is warn by default). The full §1 block is
   in place after the last lint step.
2. **`allow` → `expect` by hand.** The `allow_attributes` autofix rewrites
   `#[allow(x)]` to `#[expect(x)]` without a reason. An `#[expect]` whose lint
   does not fire is itself a warning (`unfulfilled_lint_expectations`). That
   happens when the lint fires only under `cfg(test)`, only natively or only
   on wasm32. Each site is converted by hand, together with its reason, and
   checked by every clippy form (native `--all-targets`, wasm32,
   production).
3. **Casts on input keep `as`.** §5 says to check narrowing casts with
   `try_from(..).expect(..)`. That is right only when the range is an
   internal invariant. When the value comes from source text, a manifest, an
   IR artifact or a host message (for example `f64 as i64` in `lute-runtime`
   `value.rs`, `expr.rs` and `clock.rs`, or `lute-manifest` `text.rs` and
   `clock.rs`), `expect` would add a panic on input, which the §5 table
   forbids. Such a cast keeps `as` and gets `#[expect(clippy::cast_…, reason
   = "…")]`, with a reason that states the defined semantics (for example
   float-to-int saturation).
4. **`Result<_, String>`: an explicit list, not counts.** The §4 counts
   ("20 such functions", "89 signatures") cannot be reproduced. Parsing every
   `fn` signature gives 114 signatures and 30 `pub fn` at `b8b7cdba`;
   single-line greps give 130 and 15. At `f34f141f` there are 116 signatures,
   31 of them `pub fn`. The plan header lists those 31, and that list is the
   target. A `pub fn` in a private module whose `String` is itself the
   diagnostic or runtime message may become `pub(crate)` instead of getting
   a typed error.
5. **`pub(crate)` error types are in scope.** "Every error type" includes
   `lute-cli`'s `CliError` and `ExpectError`. §4's list omits them, and both
   lack `Display` and `Error`.
6. **0.49 did not fully follow this note.** Two gaps:
   - `lute-model` `IncludeError` was added without `Display` or `Error`. That
     makes 26 `pub` error types, 17 of them without `Error`.
   - `redundant_imports` grew from 35 to 81 sites while the files that carry
     it were unchanged. These are explicit `use` items that a parent
     module's `use super::*` glob now also provides, after 0.49 added imports
     to parent modules.

   `redundant_imports` moves with `wildcard_imports`, so it is re-measured
   after the autofix step.
7. **CI trigger paths.** `.github/workflows/test.yml` runs only for the
   files in its `pull_request.paths`. The sweep adds the root `clippy.toml`
   and `scripts/check-visibility.py` to that list. Otherwise a change to
   either file alone would run no `clippy` job.
