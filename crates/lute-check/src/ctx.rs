//! Shared checker context threaded through every `check_*` entrypoint.
//!
//! `Ctx` is deliberately minimal at Task 4.2: directive validation only needs it
//! to exist and be passed through. Later tasks EXTEND it in place — T4.3 (CEL
//! type/scope resolution) reads `in_match`/`match_subject` to type the `$`
//! subject inside a `match`; T4.4 (def-assignment §8.1), T4.5 (app-write
//! read-only §9.5), T4.6, and T4.7 add their own fields here. Keep it small and
//! `Default`-able so those tasks can grow it without touching call sites.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use lute_manifest::snapshot::Domain;
use lute_manifest::types::Type;

use crate::meta::StateSchema;
use crate::rel_schema::RelVocab;

/// Analysis mode. `Author` is the interactive LSP default (lenient about
/// catalog staleness); `Ci` is the batch/build mode that later tasks may treat
/// more strictly. T4.2 does not branch on it, but downstream tasks will.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Author,
    Ci,
}

/// Immutable analysis environment computed once per document: the state schema,
/// def tables, and analysis mode. Borrowed (by reference) through every
/// `check_*` entrypoint so the per-`<match>` lexical scope in [`Ctx`] is cheap
/// to derive instead of deep-cloning these tables on each arm.
///
/// Fields are the state/def tables T4.3 consumes plus the analysis `mode`. Later
/// tasks append fields here; do not remove any without updating their consumers.
#[derive(Clone, Debug, Default)]
pub struct Env {
    /// Author (interactive LSP) vs. Ci (batch) analysis mode.
    pub mode: Mode,
    /// Inline `state:` schema (dsl §9.3): the declared state paths T4.3 resolves
    /// state-path reads against (`E-UNDECLARED`). T4.4–T4.7 read this too.
    pub state: StateSchema,
    /// Names declared under frontmatter `defs:` (dsl §8.1): the `@ref` targets
    /// T4.3 resolves `@name` uses against (`E-UNDECLARED-REF`).
    pub defs: BTreeSet<String>,
    /// def name -> the manifest [`Type`] the def PRODUCES (its declared result
    /// type). Used to flag `E-REF-TYPE` when a whole-slot `@name`/`@name(args)`
    /// def is used in a position whose expected type is incompatible (see
    /// [`ExpectedType`]), and to type nested bare `@ref` arguments of a call.
    /// Populated in `check.rs` from three sources, precedence plugin < imported
    /// < inline:
    ///   * plugin defs — `snapshot.defs` `DefDecl.ty` is already a typed `Type`.
    ///   * imported-schema `defs:` (dsl §9.2) and inline frontmatter `defs:` —
    ///     stored UNTYPED as `serde_yaml::Value`; the `type:` sub-value is
    ///     pulled out and deserialized into `Type` (malformed/absent `type:`
    ///     yields NO entry — never a panic).
    ///
    /// `#[derive(Default)]` stays correct — `BTreeMap::default()` is empty.
    pub def_types: BTreeMap<String, Type>,
    /// def name -> ordered (param name, type), for `@name(args)` arity/arg-type
    /// checks (dsl §8.1). Same sources & precedence as `def_types`.
    pub def_params: BTreeMap<String, Vec<(String, Type)>>,
    /// The document's merged, validated relational vocabulary (dsl 0.3.0
    /// §3/§4, 0.3.0 T7): imports ∪ inline `entities:`/`relations:`/`enums:`/
    /// `facts:`/`rules:`. `Arc`-wrapped — `Env` is `Clone`, the vocab is
    /// shared and never mutated after `fold_env` builds it (Task 9's guard-
    /// taint fill happens on a fresh, non-shared copy before it lands here).
    pub rel_vocab: Arc<RelVocab>,
    /// The FULL merged domain vocabulary (data-catalog foundation A4):
    /// `snapshot.domains` (core/plugin baseline) UNION project-authored
    /// schema-import domains (A3's `merge_domains`) — the SAME value
    /// `fold_env` computes into `FoldedEnv.domains` and threads to
    /// `check_assert`/`check_retract`/`build_rel_vocab`'s membership checks
    /// (0.3.0 T7). Threaded here too (0.3.0 T11 fix) so `check_fact_queries`
    /// (cel_resolve.rs) validates a query pattern's domain-typed arg
    /// (`holds`/`count`/`validAt`) against the SAME merged view a seeded
    /// `facts:`/`::assert`/`::retract` already gets — previously it passed
    /// `check_atom` an empty map, silently skipping `E-FACT-DOMAIN` for any
    /// relation arg typed against a plugin/core/project domain (as opposed
    /// to a RelVocab entity kind or `enums:` name) inside a query.
    pub domains: BTreeMap<String, Domain>,
}

/// Checker context threaded through the directive/CEL/state validators.
///
/// Borrows the immutable [`Env`] and carries only the lexical `match` scope, so
/// entering a `<match>` arm is a cheap re-borrow (copying a reference plus two
/// small fields) rather than a deep clone of the schema/def tables. Read env
/// fields via `ctx.env.<field>`; `in_match`/`match_subject` are the scope.
#[derive(Clone)]
pub struct Ctx<'a> {
    /// The immutable analysis environment (schema, def tables, mode).
    pub env: &'a Env,
    /// True while validating nodes nested inside a `match` block.
    pub in_match: bool,
    /// The raw CEL subject expression of the enclosing `match`, if any (the `$`
    /// binding T4.3 resolves).
    pub match_subject: Option<String>,
}

/// The statically-known expected type of a CEL slot's value, when derivable.
///
/// Contexts where no expected type can be derived are represented by simply not
/// setting it: B2.2 passes `Option<&ExpectedType>` to `check_cel_slot`, and
/// `None` means "no constraint, never flag". `E-REF-TYPE` is only ever emitted
/// when BOTH an expected type is known AND the slot's `@name` resolves to a
/// known def type in [`Env::def_types`].
///
/// # Purpose — B2.2 `E-REF-TYPE` (dsl §8)
///
/// A CEL slot may reference a frontmatter/plugin def via `@name`. Each def
/// declares the [`Type`] it PRODUCES. When the slot sits in a position with a
/// statically-known expected type, B2.2 compares the def's produced type against
/// the expected type (see the compatibility relation below) and emits
/// `E-REF-TYPE` on a CLEAR mismatch. This design task adds the model only; no
/// diagnostic is emitted yet.
///
/// # Per-`CelKind` derivation (`lute-syntax/src/ast.rs:147-153`)
///
/// `CelKind` has exactly four variants. For each, whether an expected type is
/// statically known, and how B2.2 derives it:
///
/// * **`Condition` ⇒ `ExpectedType::Bool` — ALWAYS (statically known).**
///   The kind alone fixes the type: `<when test=…>` arm guards
///   (`Arm::When.test`, ast.rs:86-91; checked at `check.rs:342`),
///   `<choice when=…>` guards (`Choice.when`, ast.rs:72; `check.rs:309`), and any
///   other boolean guard all expect `bool`. Highest confidence — no schema
///   lookup, no threading. `Arm::Otherwise` (ast.rs:92-95) carries no test, so
///   there is no slot to type there.
///
/// * **`SetExpr` ⇒ `ExpectedType::Ty(target_path_type)` when resolvable, else
///   unknown (statically known iff the target path is in the schema).**
///   The `::set` node is `Set { path: String, expr: CelSlot, .. }`
///   (ast.rs:51-58; checked at `check.rs:300` / `check.rs:370`). B2.2 looks the
///   RHS's expected type up via `set_op::resolve_type(&set.path, &ctx.state)`
///   (`set_op.rs:102`) — the target path's declared [`Type`], resolved by exact
///   `state:` key OR by descending `Record` fields / `Map` values from the
///   nearest declared ancestor (so a nested target like `scene.player.hp` under
///   a declared `scene.player` record resolves), NOT a flat `ctx.state.decls.get`
///   exact-key lookup. A compound or
///   derived RHS (`a + b`, a ternary, …) still expects the SINGLE declared type
///   of the target path — the whole expression must evaluate to that type. If
///   `set.path` is absent from the schema, the expected type is unknown (no flag;
///   the missing path itself is a separate `E-UNDECLARED`-class concern).
///
/// * **`AttrValue` (a `@ref` used as a directive attribute value) ⇒
///   `ExpectedType::Ty(attr_declared_type)` when the owning directive + attr are
///   known, else unknown.** The attr's type is
///   `snapshot.directive(tag)` (snapshot.rs:37-39) → the matching
///   `AttrDecl { name, ty, .. }` (`DirectiveDecl.attrs`, schema.rs:50-54;
///   `AttrDecl.ty`, schema.rs:66-72).
///   **THREADING COST (explicit):** a `CelSlot` does NOT carry its owning
///   directive tag or attr name (ast.rs:141-162), so the expected type CANNOT be
///   derived inside `check_cel_slot` (`cel_resolve.rs:30`). B2.2 must compute it
///   at the CALL SITE `check_attr_refs` (`check.rs:381-386`), where the owning
///   `Directive.tag` and `Attr.key` are in scope, and pass it in. This is why
///   B2.2 gives `check_cel_slot` an `expected: Option<&ExpectedType>` parameter
///   rather than having it derive the type internally — the slot is context-free.
///
/// * **`MatchSubject` ⇒ `ExpectedType::Ty(subject_path_type)` when the subject is
///   a single state path with a known type, else unknown.** The subject is
///   `Match.subject: CelSlot` (ast.rs:78-83; checked at `check.rs:327`). B2.2
///   resolves `subject.raw` via `set_op::resolve_type` exactly as `SetExpr` does
///   (exact key OR descend `Record`/`Map` from the nearest declared ancestor); a
///   compound subject expression (not a single resolvable path) has no single
///   expected type ⇒ unknown.
///
/// So `Condition` is unconditionally statically known; `SetExpr`, `AttrValue`,
/// and `MatchSubject` are statically known only when their respective lookup
/// (state schema / directive-attr decl) resolves, and unknown otherwise.
///
/// # Def-type sources (populating [`Env::def_types`] in B2.2)
///
/// * **Plugin defs — already typed, direct.** `snapshot.defs: BTreeMap<String,
///   DefDecl>` (snapshot.rs:19); `DefDecl.ty: Type` (schema.rs:167-170) is a
///   ready `Type`. Insert `(name, ty.clone())` directly.
/// * **Inline frontmatter `defs:` — stored UNTYPED.** `parse_meta` keeps them as
///   `TypedMeta.defs: BTreeMap<String, serde_yaml::Value>` (`crate::meta`,
///   meta.rs:44; populated meta.rs:152). Each value is a mapping like
///   `{ type: bool, cel: "…" }` (see `docs/examples/bianca-s01ep02.lute:14-15`:
///   `fond: { type: bool, cel: "scene.affect.bianca >= 1" }`). B2.2 pulls the
///   `type:` sub-value and deserializes it into `Type` via the SAME serde path
///   `Type` uses (the `TypeDef` singleton-map shadow, `types.rs:149-270`), e.g.
///   `serde_yaml::from_value::<Type>(v.get("type")?.clone()).ok()`. A
///   malformed/absent `type:` yields NO entry — never a panic.
///
/// # `ctx.defs` vs `def_types` (two tables, one union source)
///
/// [`Env::defs`] (the `E-UNDECLARED-REF` set) is the UNION of inline frontmatter
/// def names (`typed.defs.keys()`), plugin-exported def names
/// (`snapshot.defs.keys()`), and imported-schema def names
/// (`imports.defs.keys()`) — see `check.rs:207-209`. `def_types` is a parallel
/// table mapping the subset of those names with a known produced [`Type`]. A
/// name may be in `defs` without a `def_types` entry (untyped/malformed
/// `type:`), so the two are consulted independently: `E-REF-TYPE` fires ONLY
/// when a name is present in `def_types` AND an expected type is known.
///
/// # Compatibility relation — `compatible(produced: &Type, expected: &ExpectedType) -> bool` (B2.2)
///
/// CONSERVATIVE by construction: return `true` (no flag) for everything not
/// PROVABLY incompatible. The decided relation:
///
/// * `expected == ExpectedType::Bool` ⇒ compatible iff `produced == Type::Bool`.
///   This is the canonical positive case to flag: a def producing `Number` used
///   in `<when test="@count">` where `count: number` — `produced == Number !=
///   Bool` ⇒ INCOMPATIBLE ⇒ `E-REF-TYPE`.
/// * `expected == ExpectedType::Ty(t)`:
///   1. **Id types are always compatible (never flag).** If EITHER `t` or
///      `produced` is `ProviderRef(_)`, `SlotId { .. }`, or `AssetKind(_)`
///      (types.rs:19-22, all attribute-only / namespaced id types), treat as
///      unknown and return `true`. DECISION + JUSTIFICATION: these carry a
///      namespaced/provider identity that a def's produced `Type` and def-CEL
///      string production cannot be shown to satisfy here; their value-level form
///      is a string, but membership/namespace validity is a separate concern
///      E-REF-TYPE deliberately does not attempt. Folding them into the string
///      family (the other option) would risk false positives (e.g. `Str`-vs-
///      `SlotId`) for no real gain, since defs almost never declare an id type —
///      so "always compatible" is the strictly-safer call.
///   2. **String family is mutually compatible.** Else if BOTH `produced` and `t`
///      are in `{ Str, Enum(_), EnumFromOption(_) }`, return `true`. An enum value
///      IS a string at the value level and def CEL produces string-ish values;
///      E-REF-TYPE does not do enum-membership checking (a value-level concern).
///   3. **Otherwise structural equality.** Compatible iff `produced == t`
///      (`Type` derives `PartialEq`, types.rs:10 — structural over the whole
///      shape, incl. `List`/`Record`/`Map`). This flags clear mismatches like
///      `Number` vs `Bool`, `Number` vs `List(_)`, `Bool` vs `Str`.
/// * Anything not resolving to a clear incompatibility ⇒ compatible (no flag).
///
/// # Scope guidance for the design gate
///
/// If the reviewer/user find the full model too broad, B2.2 can ship a CLEAN
/// SUBSET: `Condition ⇒ Bool` + `SetExpr ⇒ target-path type` ONLY, deferring
/// `AttrValue` and `MatchSubject`. This is a strict prefix — SAME `ExpectedType`,
/// SAME `compatible`, SAME `def_types` — differing only in how many call sites
/// populate `expected`. `Condition`/`SetExpr` need no extra threading (the slot
/// kind and the `Set.path` are already at the call site), whereas `AttrValue`
/// needs the owning directive/attr threaded to `check_attr_refs` and
/// `MatchSubject` needs single-path subject resolution. Narrowing therefore costs
/// zero rework and is a clean subset of this design.
#[derive(Clone, Debug, PartialEq)]
pub enum ExpectedType {
    /// A boolean guard/condition (`<when test>`, `<match>`-arm test,
    /// `<choice when>`): expects `bool`.
    Bool,
    /// A concrete manifest type: a `::set` RHS = the target path's declared type;
    /// a directive attr `@ref` = the attr's declared type; a `<match on>` subject
    /// = the subject path's declared type.
    Ty(Type),
}
