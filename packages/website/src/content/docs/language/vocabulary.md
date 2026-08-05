---
title: Content vocabulary
description: "The compiler declares the content-vocabulary slots and ships no members — how a project declares its own emotions, actions, anchors, moods and VFX types, the three declaration routes and their precedence, and the exits:/default: member semantics."
---

Lute is a general authoring toolchain; your game is a consumer of it. So a concrete list of
emotions does not live in the compiler — **the compiler declares the vocabulary *slots*, and your
project declares the *members*.** Since language `0.9.0` the built-in `lute.core` plugin ships an
empty `enums:`; there is no baseline list to override, repudiate, or work around.

The consequence is the other half of the trade: **using a slot nothing declares is an error.**

<!-- lute-diagnostics -->
```
error [E-DOMAIN-UNKNOWN] `emotion` is not a declared domain — declare its members in an `enums:`
block in this document's own frontmatter, in a project schema reached through `uses:`, or in a
plugin's `enums` export before using `emotion` (dsl 0.9.0 D-C)
```

Before `0.9.0` six of these slots carried closed members no route could extend, and `action` — the
seventh — was skipped by the checker entirely whenever nothing declared it, so a misspelling like
`action="step-foward"` shipped silently. Both special cases are gone.

## The seven slots

A *slot* is a named domain the language binds to an authoring position. The name is the language's;
the members are yours.

| Slot | Bound at |
|---|---|
| `emotion` | content line `emotion=` |
| `action` | content line `action=`, `::auto{action}` |
| `anchor` | `::auto{anchor}` |
| `mood` | `::music{mood}` |
| `volume` | `::music{volume}` |
| `musicAction` | `::music{action}` |
| `vfxType` | `::vfx{type}` |

`::music{action}` fills the `musicAction` slot — the slot name and the attribute name differ, and the
diagnostic names the slot. Two of these bindings are new in `0.9.0`: `::auto{action}` and
`::music{mood}` used to be free `string`s, which is why `mood` had been declared-but-inert since it
shipped and `action` went unchecked.

**Not vocabulary.** `::cut{action}` and `::video{action}` stay `{ enum: [show, hide] }` declared
inline on the directive — a two-member pairing the engine dispatches on, not a shared vocabulary. The
delivery flags `{mono}` / `{os}` / `{vo}`, the reserved `narrator` speaker, and the `::end` tag are
grammar.

## Declaring a vocabulary

A declaration is an `enums:` block. A bare sequence is shorthand for `{ members: [...] }`, so every
pre-`0.9.0` block still parses byte-for-byte.

```yaml
enums:
  emotion: [neutral, surprised, delighted, shy, content, angry, sad]
  anchor:
    members: [left, center, right]
    default: center
  action:
    members: [fade-in-up, sway, lean, idle, fade-out, hide]
    exits: [fade-out, hide]
  mood: [peaceful, tense, romantic, sad, upbeat]
  volume: [silent, down, normal, up, full]
  musicAction: [start, change, stop, resume, fade-out]
  vfxType: [whiteOut, blackOut, rain, snow, leaves, petals, raindrop]
```

There are **three routes** to that block, and all three go through the same parser and the same
validator.

1. **Inline** — in the using document's own frontmatter. `enums:` is a universal frontmatter key, so
   any document (scene, quest, schema) may declare the vocabulary it uses right where it uses it.
   This is the only route open to a single-file author and to the playground, which checks one
   in-memory document and can resolve no import.
2. **Imported** — in a project schema reached through `uses:` or `extends:` (see
   [State schemas](/state/schemas/) and [Imports](/language/imports/)). The route a multi-document
   project should prefer: declare once at the root and let every document reach the same members.
   [`docs/examples/base.schema.yaml`](https://github.com/journeyWorker/lute/blob/main/docs/examples/base.schema.yaml)
   is one.
3. **Plugin** — in a capability plugin's `enums` export (see [Manifests](/plugins/manifests/)). The
   route an engine or genre pack uses to ship a vocabulary to every project that activates it.
   [`showcase.pack`](https://github.com/journeyWorker/lute/blob/main/docs/examples/showcase/plugins/showcase.pack/enums/vocabulary.yaml)
   is one. Unlike the other two it surfaces on the capability snapshot — it lands under
   `lute context --json`'s `enums` and so moves `capabilityVersion`, where the two project routes
   land under the separate `projectEnums` key and move neither.

Declaration is **per project root** — a directory with a `lute.project.yaml`. A sibling root's
declaration never reaches into yours.

:::note
A plugin's members only reach a single-file `lute check` when the run can see the project:
`lute check scenes/opening.lute --project .`, or `lute check-project .`, which resolves the root
itself. Without one of those the run is core-only, and a document relying on the plugin route
reports `E-DOMAIN-UNKNOWN`.
:::

## Precedence when two routes declare one slot

The two clashes behave differently, and the difference is worth reading twice.

**A project route against a plugin is `E-DOMAIN-DUP`, and the plugin wins.** This holds for *both*
project routes — an inline `enums:` block and an imported schema's are treated the same way. The
project entry is dropped and reported, so subsequent values are judged against the plugin's members:

<!-- lute-diagnostics -->
```
error [E-DOMAIN-DUP] domain `emotion` is declared by this project — in a document's own `enums:`
frontmatter or in a project schema reached through `uses:`/`extends:` — but already exists in the
plugin/core vocabulary; a domain name must be declared by exactly one source, so drop the project
declaration or the plugin's `enums` export (the plugin's wins)
error [E-BAD-ENUM] `furious` is not a valid value for `emotion` of `::narrator`
(expected one of: neutral, delighted, pensive)
```

Pick one route per slot. Since the core ships no members, this only ever fires against a plugin you
activated.

**Inline against imported is not a clash — the inline declaration wins, and it must be a
superset.** A document may re-declare a slot its schema already declares in order to *add* members,
and the inline list then governs. Dropping a member the base declared is
`E-EXTENDS-RELATION-SIG`:

<!-- lute-diagnostics -->
```
error [E-EXTENDS-RELATION-SIG] enum `emotion` is missing base member(s) ["delighted"]; an inline
re-declaration must re-declare a superset of the imported base's members (dsl 0.3.0 §4.1)
```

This is deliberately **not** `E-DOMAIN-DUP`. That code is reserved for clashes involving a plugin.

## Member semantics: `exits:` and `default:`

Two slots have members the compiler *branches on*, and it will not guess which.

- **A declaration of `action` MUST supply `exits:`** — the members that end a character's presence on
  stage. A member listed there lowers to a `sprite` record carrying `exit: true`; a member not listed
  does not.
- **A declaration of `anchor` MUST supply `default:`** — the member used when a character is shown
  without an explicit anchor. It is injected as a `sprite` record with
  `provenance.by: "auto-anchor-on-show"`.

Both used to be hardcoded: exits were detected by name (`fade-out*` / `exit*` / `hide`) in two
hand-synced copies, and the default anchor was a `DEFAULT_ANCHOR = "center"` constant in the checker.
Omission is now an error rather than a fallback, because a silent fallback to a name prefix is
exactly the hidden coupling `0.9.0` removes. The language owns the knowledge that these two slots
need member semantics; it does not know which members satisfy them.

For the other five slots `exits:` and `default:` are meaningless and **rejected**, so a typo cannot
hide in an ignored key. `default:` and every `exits:` entry must itself be a declared member.

| Code | Fires when |
|---|---|
| `E-ENUM-MISSING-SEMANTICS` | `action` declared without `exits:`, or `anchor` without `default:` |
| `E-ENUM-UNEXPECTED-SEMANTICS` | `exits:` or `default:` on a slot that has no such semantics |
| `E-ENUM-DEFAULT-NOT-MEMBER` | `default:` is not one of `members:` |
| `E-ENUM-EXITS-NOT-MEMBER` | an `exits:` entry is not one of `members:` |

Neither key is serialized into the artifact. The compiler has already resolved them into
`sprite.exit` and the emitted anchor, so an engine needs no member semantics at runtime.

## Tooling

`lute init` scaffolds a starter vocabulary covering all seven slots into a
`vocabulary.schema.yaml` the generated scene imports — an opinionated template, not a rule, in a file
you own and edit.

`lute doctor <dir>` reports which slots a root has declared, with the member semantics inline:

```
$ lute init demo && lute doctor demo
lute doctor — demo
  • toolchain version: 0.10.0
  • language version: 0.10.0
  • IR schema version: 0.10.0
  ✓ lute.project.yaml: found at demo/lute.project.yaml
  ✓ content documents: 1 `.lute` file(s) under demo
  • provider snapshots: no providers/ directory (core-only project)
  • vocabulary slots declared: emotion, action (exits: fade-out/hide), anchor (default: center), mood, volume, musicAction, vfxType
  • VS Code extension: not detectable from the CLI
```

A root that declares nothing reports `vocabulary slots declared: none`.

For a project that declares inline or through `uses:`/`extends:`, the compiled artifact's `enums`
array becomes populated — the artifact is self-describing about the vocabulary it was compiled
against. A plugin-supplied vocabulary does **not** appear there; it is capability surface, and shows
up in `capabilityVersion` and `lute context --json`'s `enums` instead. Either way this is an
artifact-*content* change only: the vocabulary work added, renamed, and moved no IR field.
(`irVersion` reads `"0.10.0"`; the shape change that number carries is a single
provenance-field rename, unrelated to vocabulary.)

## Known limitation: a component body resolves against the importing document

State it plainly, because it will bite someone. **A component's own `uses:` and its own inline
`enums:` are both discarded at parse.** The resolved component carries only `params`, `body`, and
`src`, so once its body is folded into an importing document the vocabulary it checks against is the
**importing document's**.

So a component naming a slot only *it* declares passes standalone and fails through a `::use`:

<!-- lute-diagnostics unverified="deliberately abridged — both quotes elide the component's path (`…/c.component.lute`) and the tail of the shared E-DOMAIN-UNKNOWN sentence (`— …`) because the block exists to show the `component ... (...):` prefix, not the message it prefixes; the unabridged sentence is pinned at vocabulary.md:14" -->
```
$ lute check c.component.lute      # its own uses: declares emotion, its own inline enums: declares vfxType
ok: c.component.lute (0 warning(s))

$ lute check s.lute                # components: [c.component.lute], scene declares no vocabulary
s.lute:1:1: error [E-DOMAIN-UNKNOWN] component `reaction` (…/c.component.lute): `emotion` is not a declared domain — …
s.lute:1:1: error [E-DOMAIN-UNKNOWN] component `reaction` (…/c.component.lute): `vfxType` is not a declared domain — …
failed: s.lute (2 error(s), 0 warning(s))
```

The diagnostic anchors at the `::use` site — the importing document's frontmatter — with a prefix
naming the component and its file.

**Consumers own the vocabulary imports.** The practical rule: keep the declaration at the project
root and let both the scene and the component reach the same one, which is what `docs/examples` does.
A *component schema* surface — a component declaring its own imports and `::use` carrying them into
the expansion — is a named future direction filed separately, not part of this release.

This is a *scoping* limit, not a checking divergence. Every content-line rule now runs on both
routes: `0.9.0` closed five stages of `check()` that used to be root-only, so a component body no
longer checks clean through a `::use` while the same lines report at scene level. See
[Components & extends](/language/components-and-extends/).
