# Proposal (NEEDS USER REVIEW — not yet implemented): Reusable Content Components / Macros

**Status:** DESIGN PROPOSAL for review. NOT implemented. Building this blind would invent load-bearing language grammar + semantics with no spec; it needs your design sign-off first (it is the one FEAT-4 item that is a genuine language-design decision, unlike the other shipped features which had spec backing).

## Why
The language already has three reuse mechanisms, all shipped:
- **`defs` / `@name(args)`** (§8.1) — reusable, typed, parameterized CEL *values* (guards/expressions).
- **`uses:`** (§9.2) — peer schema imports.
- **`extends:`** (§9.2) — base-schema composition with override.

What is missing is reuse of **content/staging** — repeated dialogue+staging patterns (a "greet" beat, a "chapter card", a recurring stinger) that today must be copy-pasted per scene.

## Proposed construct (two options — pick one)

### Option A — `<component>` definition + `<use>` invocation (RECOMMENDED)
Define once (in a component file imported via `uses:`, or a `components:` frontmatter block):
```
<component name="greet" params="{ who: providerRef(characterId), warmly: bool }">
  ::auto{character=@who action="wave"}
  :line[@who]: Hey there.
</component>
```
Invoke:
```
<use component="greet" who="bianca" warmly=true/>
```
- Expansion is a **compile-time macro over content** (mirrors `@name(args)` for values): the `<use>` is replaced by the component body with params substituted, *before* reduction. Expansion itself is engine-side (like §11.5 reduction); the **checker** validates: component name declared, args match declared params (count + type — reuse the `E-REF-ARITY`/`E-REF-ARG-TYPE` machinery), body is well-formed in the invocation context, no recursive component cycles.
- Param substitution into content: params usable in `@who` (ref position), attrs, and possibly `:line` speaker/text interpolation (needs a decision — see open questions).

### Option B — user-defined content directives
Extend the plugin/manifest directive system so a *project* (not just a plugin) can declare a content macro directive that lowers to a fixed body. Reuses the existing directive-declaration + validation path, but couples content reuse to the capability-manifest layer (arguably the wrong owner — content is game data, not engine vocabulary, per §9.2's data↔code boundary).

## Static-checker surface (either option)
- New codes: `E-COMPONENT-UNDECLARED` (unknown component), `E-COMPONENT-ARITY`/`E-COMPONENT-ARG-TYPE` (args vs params), `E-COMPONENT-CYCLE` (recursive expansion), `E-COMPONENT-BODY` (body invalid).
- The FND-1 `for_each_cel_slot` traversal seam already generalizes so an expanded body's slots are validated uniformly (the audit tied this feature to that seam — now in place).

## Open questions (need your call before implementation)
1. **Syntax:** `<component>`/`<use>` element form (Option A) vs directive form (Option B) vs a `@macro`-style form?
2. **Definition location:** inline `components:` frontmatter, a dedicated component file imported via `uses:`, or the project manifest?
3. **Param interpolation into `:line` text** (e.g. `:line[@who]: Hi, @greetingTarget`) — allowed? This touches §4.4 opaque-text and localization (§12 textUnitId) semantics.
4. **Expansion visibility:** does the checker expand-then-check (needs a real expander), or validate structurally without expanding (like the current `@name(args)` approach)?
5. **Nesting/recursion depth**, and whether a component may contain `<branch>`/`<match>`/`<timeline>`.

## Cost estimate
Option A: tree-sitter grammar addition (+ capabilityVersion re-stamp if a hashed field is touched — likely NOT, grammar-only) + lute-syntax parser + new AST node(s) + checker validation pass + LSP features (hover/completion/nav for components) + fixtures + spec section. Large (multi-task), and the grammar/semantics decisions above are load-bearing.

**Recommendation:** approve Option A with answers to the open questions, then it becomes a normal spec → plan → subagent-driven build like the shipped features.
