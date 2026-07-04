# Proposal (IMPLEMENTED — dsl §13): Reusable Content Components / Macros

**Status:** IMPLEMENTED as **Option C** (directive-form, file-based, NO grammar change) — see DSL spec §13 and the FEAT-5 plan (`docs/superpowers/plans/2026-07-03-feat5-components.md`). Options A/B below are retained as design history; the shipped decision, its checker codes, and its v0.0.1 scope are recorded in the **Option C** section and resolve every open question.

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

### Option C — directive-form, file-based (IMPLEMENTED, dsl §13)
The shipped decision: a hybrid that takes Option A's file-based, `params`-typed component model but invokes it through the **existing directive surface** instead of new element grammar, so it required **no tree-sitter/parser change and no `capabilityVersion` re-stamp**.
```
# greet.component.lute  — a component FILE: frontmatter + presentational body
---
component: greet
params: { who: string }
---
## Greeting.
::auto{character=@who action="fade-in-up"}
:line[narrator]: A familiar face steps into the light.
```
```
# scene.lute — imports via `components:` and invokes with the reserved `::use`
---
character: demo
season: 1
episode: 1
components: [greet.component.lute]
---
## Shot 1.
::use{component="greet" who="bianca"}
```
- **Definition:** one component per file (a `component:` + optional `params:` frontmatter, presentational body); imported by a scene via a `components: [<path>]` frontmatter key resolved through the same canonicalized, cycle-checked, diamond-deduped import DAG as `uses:` (§9.2).
- **Invocation:** the reserved built-in directive `::use{ component="<name>" <arg>=<value> … }` (`use` is reserved, §10) — named args bind to params by name, checked in count + type; expansion is a compile-time macro over content, engine-side (§11.5). The checker validates structurally (declared / args / acyclic / body) without a real expander.
- **v0.0.1 scope (spec §13.4):** a component body is **presentational** — lines + staging + `@param` refs only; NO scene/run state read/write and NO `<branch>`/`<match>`/`<timeline>` logic blocks (pass values via params). One component per file; a body MAY `::use` other components (acyclic) but MAY NOT define one; text interpolation into `:line` prose is deferred.
- **Future work:** the Option A `<component>`/`<use>` **element form** (grammar-level syntax), stateful/logic-bearing bodies, and `:line` text interpolation are all deferred — the directive form was chosen precisely so it reuses the existing directive + frontmatter + import-DAG surface with zero grammar change, leaving the element form as a possible later addition.

## Static-checker surface (either option)
- Implemented codes (spec §13.3): `E-COMPONENT-UNDECLARED` (unknown component), `E-COMPONENT-ARG` (args vs params — count/name/type), `E-COMPONENT-CYCLE` (recursive expansion / import cycle), `E-COMPONENT-DUP` (duplicate component name across imports), `E-COMPONENT-PARSE` (malformed component file / `params:`), `E-COMPONENT-BODY` (non-presentational body node in v1).
- The FND-1 `for_each_cel_slot` traversal seam already generalizes so an expanded body's slots are validated uniformly (the audit tied this feature to that seam — now in place).

## Open questions (RESOLVED — see Option C)
1. **Syntax:** directive form (`::use`) — no grammar change; the `<component>`/`<use>` element form is deferred future work.
2. **Definition location:** a dedicated **component file** imported via a `components:` frontmatter key (not `uses:`, not the manifest).
3. **Param interpolation into `:line` text:** deferred; params are usable only in ref/attr positions in v0.0.1.
4. **Expansion visibility:** validate structurally without expanding (like `@name(args)`); expansion is engine-side (§11.5).
5. **Nesting/recursion + logic blocks:** a body MAY `::use` other components (acyclic, `E-COMPONENT-CYCLE`) but is presentational only — `<branch>`/`<match>`/`<timeline>` in a body is `E-COMPONENT-BODY` in v1.

## Cost estimate
Option A: tree-sitter grammar addition (+ capabilityVersion re-stamp if a hashed field is touched — likely NOT, grammar-only) + lute-syntax parser + new AST node(s) + checker validation pass + LSP features (hover/completion/nav for components) + fixtures + spec section. Large (multi-task), and the grammar/semantics decisions above are load-bearing.

**Outcome:** shipped as **Option C** (directive-form, file-based) rather than the recommended Option A, avoiding the load-bearing grammar/semantics work while delivering content reuse now; the element form remains available as future work. See DSL spec §13, fixtures under `docs/examples/components/`, and the showcase `::use` in `docs/examples/showcase/`.
