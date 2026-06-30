# Lute — Plugin System (overview & rationale)

**Status:** draft / forward-looking. Not yet implemented.

> **Normative spec:** the plugin formats and semantics — the YAML manifest schemas, the capability
> resolution algorithm, the capability snapshot, and the data↔code boundary — are specified with
> RFC 2119 rigor in [`proposals/plugin-system/0.0.1.md`](proposals/plugin-system/0.0.1.md).
> **That proposal is the source of truth.** This document is the human-facing **overview +
> rationale** (the *why* and the author's mental model); where the two differ, the proposal wins.

**Audience:** plugin authors, and the compiler/checker/LSP implementers who consume what a plugin
declares.

**See also:** [`proposals/scenario-dsl/0.0.1.md`](proposals/scenario-dsl/0.0.1.md) (the language),
[`architecture.md`](architecture.md) (compiler/AST/validation/LSP),
[`examples/date-minigame.lute`](examples/date-minigame.lute) (a worked plugin scene).

## The one rule

Plugins add *vocabulary* (directive names, attrs, enums, state shapes, providers, bridge
signatures, definitions, diagnostics) and *capability surfaces* — **never grammar and never
behavior**. The fixed grammar lives in the language spec; behavior (control flow, lowering
algorithms, exhaustiveness, auto-injection) lives in the compiler core. The line that decides which
side a new capability falls on is the [data↔code boundary](#the-datacode-boundary-read-this-first)
(normative: proposal §3.2, §12).

## Why one capability manifest

Today the vocabulary is hardcoded across four places (`parser.ts` enums + inline attr checks,
`generator.ts` lowering, the future LSP, the future tree-sitter grammar) — drift-prone (this design
repeatedly hit that class: invented `::scene`, an unused `track` attr, a redundant `<parallel>`).
The fix is a **single declarative capability manifest** that all four consumers derive from: add a
capability once → parser, compiler, LSP, and grammar all update. Activation resolves the installed
plugins + selected profile into one immutable **capability snapshot** (the exact field list is
proposal §13) that the checker, LSP, and compiler all validate against.

**Semantic tags in the manifest, algorithms in code.** A provider returning ids is too weak: the
compiler also needs to know what a kind *means* (`bg` scene-persistent, `sfx` fire-and-forget,
`music` channel/action, `character` binds pose/anchor/layer, `cut` pairs show/hide). Encode those
as reusable **semantic flags** from a closed vocabulary (`semantics: ["writes.characterState",
"mayExitCharacter", "usesAnchor"]`, proposal §8.1) so parser/compiler/LSP agree on meaning — but
keep the bespoke algorithm in code.

## The data↔code boundary (read this first)

Before adding anything, decide which side of the line it falls on. A capability is **registrable
data** (it belongs in a plugin manifest) iff *all* hold: (1) fixed syntax; (2) validation local to
its attrs + catalog lookup; (3) lowering is a finite attrs→records mapping; (4) no new control
flow; (5) no cross-sibling/global reasoning beyond declared resource conflicts; (6) no AST-shape
change; (7) no ordering-sensitive interpretation beyond the existing timeline/`wait` model. Any
false → **code** (a compiler-core change, not a plugin).

- *Data:* a new `::shake`, `emotion="smug"`, `::vfx type="rain"`, `musicAction="duck"`, a new
  `::bg transition="wipe"` attr.
- *Code:* a new branching construct, a new timeline-resolver behavior, a new exhaustiveness rule,
  "run until interrupted", "bind this action to future dialogue state", "auto-place characters".

**The trap to name:** authors/LLMs most want things that *feel* like vocabulary but are compiler
behavior — "have her leave naturally", "keep the same pose unless mood changes", "hit this SFX
exactly as the line appears", "hide whoever is no longer speaking". Treating these as registrable
data is precisely where you'd accidentally need a scripting language. They are code — served, if at
all, by a **named builtin lowering hook** added to the core (proposal §8.2), gated by a
golden-test-per-directive (proposal §12).

## How a plugin is built (orientation)

A plugin is a directory of declarative YAML behind one `plugin.yaml` entry; consumers reference the
**plugin id** only. The loader reads only the directories named in `exports`.

```text
plugins/<id>/
  plugin.yaml          # id, version, depends, exports, options   → proposal §5
  directives/*.yaml     # ::name directive declarations            → proposal §8
  state/
    shapes.yaml         # reusable typed record shapes             → proposal §6.2
    templates.yaml      # structured path templates                → proposal §6.3
  providers/*.yaml      # id registries (snapshot-first)           → proposal §6.4, §10
  bridge/*.yaml         # typed runtime bridge capabilities        → proposal §6.5
  defs/*.yaml           # shared typed-CEL @refs                   → proposal §6.6
  docs/*.md             # hover docs (non-normative)
```

Everything a plugin declares is **typed** by one small manifest type system (`enum`, `list`,
`bool`/`number`/`string`, `enumFromOption`, `providerRef`, `slotId`, shape refs — proposal §7), and
all variable state paths use **structured segments**, never `$name` string interpolation (proposal
§6.3, §7.4). A directive declares its `attrs`, optional `semantics` flags, the state slots it
`declares`, the state it `writes`, an optional `bridge` binding, and how it lowers (`record` for a
finite mapping, or a named `builtin` hook) — full schema in proposal §8.

Bridge calls are typed directives that write declared state, not arbitrary tool calls: the DSL
emits data, the engine executes the bridge, and story control-flow observes only the declared state
effects (proposal §9). With `wait="false"` a bridge result is read **before it is produced** — and
because result slots are shape-defaulted, that reads the **default**, not the outcome (a
stale-default diagnostic, distinct from `E-MAYBE-UNSET`; proposal §9).

## Profiles & activation (orientation)

A root-level `profile` selects the active capability set for a scene; the reserved `global` profile
is inherited by every other profile, and profiles compose via `extends`. A scene picks one with
frontmatter `profile:` and MAY layer scene-local `plugins:` on top. Activation is by **presence of
the plugin id** in a `plugins` map (its value is the typed option object); there is **no**
`plugins.use` list, and 0.0.1 has no scene-local *deactivation*. Resolution is deterministic
(`lute.core` → `global` → `extends` chain → selected profile → scene-local → dependency closure),
with scalar options overriding, maps deep-merging, and lists replacing — the normative algorithm
and merge rules are proposal §11. A reference to a directive from an installed-but-inactive plugin
is a diagnostic with fix-its, never silently accepted.

## Providers are snapshot-first

Compiler/checker correctness must never depend on a live or remote catalog. Providers resolve
against a pinned **snapshot artifact**; the compiler fails if required data is missing (never blocks
on the network), and the LSP keeps a stale snapshot + emits a *catalog-stale* diagnostic when
offline rather than false *unknown-id* errors. The parser never calls providers — only the checker
does (proposal §10).

## Declarative lowering vs named hooks

Trivial one-record directives lower as data (`lower: { record: "camera.set", fields: {…} }`).
Lowering that reads prior commands, pairs show/hide, allocates timeline tracks, expands to a
variable number of records, or inspects siblings needs an **imperative hook** — but a *narrow,
named* one from a closed core registry (`lower: { kind: builtin, name: autoCharacterAction }`). No
inline code in a manifest; each hook declares input/output record schemas + unit tests; the
directive still declares attrs/validation/writes/semantics as data. **Adding a hook is a core code
change, not content registration** (proposal §8.2). This is what stops the manifest from becoming a
hidden programming language.

## MVP order (don't build a framework first)

1. enums → manifest. 2. directive attr schemas → manifest. 3. generate parser/checker validation
tables. 4. generate LSP completion/diagnostics. 5. keep compiler lowering handwritten at first.
6. declarative lowering for trivial one-record directives only. 7. manifest version/hash checks on
every generated artifact. **Start with the easiest consumers (validation + completion); generate
tree-sitter grammar last.**

## Highest risk — semantic drift disguised as extensibility

The manifest says a directive is valid, the LSP completes it, the parser accepts it, but compiler
behavior still depends on hidden `generator.ts` rules the manifest didn't model → *statically valid
scripts that lower incorrectly.* Mitigation: every directive declares
`attrs / reads / writes / semantics / loweringKind / recordSchema / examples`, plus a **golden test
per directive** (proposal §12). **If a directive can't get a clean golden test, it isn't just data —
it's code.**

Stated the other way (the single biggest whole-design risk): **the manifest becoming a "semantic
god object."** Vocabulary, namespaces, directive names, asset kinds, and completion data belong in
it; the moment real *behavior* leaks in, you get a brittle generated system where parser, compiler,
LSP, and tree-sitter all *appear* unified but nobody knows where behavior actually lives. **Keep the
manifest declarative and boring — it describes vocabulary and capability surfaces; the core owns
meaning** (named, tested checker/lowering/injection rules with provenance).
