r5-upgrade-lighthouse-1 — `lute context` (the AI authoring surface) omits 0.26 component param defaults and directive when=

Lute 0.26.0 (native darwin-arm64 binary). Found while upgrading Skerry Rock
(~/Workspace/lute-dogfood/round3/lighthouse-keeper, branch v0.26), whose
lampLighting component now declares `keeper` and `glass` with `default:`.

Commands (from this directory):
  lute check-project .
  lute context scenes/a.lute --project .
  lute context scenes/a.lute --project . --json

The project checks clean. `components/lamp.component.lute` declares
  keeper: { type: string, default: "The inspector" }
  glass:  { type: { enum: [steady, guttering] }, default: steady }
and scenes/a.lute uses it bare (`::use{component="lamp"}`) and with a guard
(`::use{component="lamp" glass="guttering" when="true"}`).

Actual (see actual.txt):
  components (1):
    lamp(keeper: string, glass: enum[steady, guttering])
  JSON: {"name": "lamp", "params": [{"name": "keeper", "type": "string"},
         {"domain": ["steady", "guttering"], "name": "glass", "type": "enum"}]}
  builtinDirectives: `::use{component="<name>" <param>=<value> …}`,
  `::assert{…}`, `::retract{…}`, `::accept{quest="<questId>"}`, `::set{…}` —
  none mentions `when="…"`.

Expected:
  The signature marks optional params and their defaults, e.g.
    lamp(keeper: string = "The inspector", glass: enum[steady, guttering] = steady)
  and JSON `{"name": "keeper", "type": "string", "default": "The inspector"}`
  (a `@def` default as `"default": "@glassTonight"`). The builtin directive
  syntax lines for ::use / ::accept / ::assert / ::retract show the optional
  `when="<condition>"` (dsl 0.26.0 §4), as ::set's already could.

Why it matters: docs/tooling/ai-harness.md says the component signature is
there "so a model writes ::use{component=… …} against the signature". Against
this signature a model must pass every param, so a component whose defaults
come from a host @def (Skerry Rock's `glass` default `@glassTonight`) gets its
dynamic default overridden by a guessed literal, and a model never learns
`when=` exists on directives.
