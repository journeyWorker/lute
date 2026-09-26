---
title: Components & extends
description: Two reuse mechanisms — reusable content components invoked with ::use, including string params that carry a sentence into each call site, speaker params a body speaks as with @@who, param defaults, a body that reads its own plugin directive results, guarded ::use, and effects components that write state — and extends schema composition with base-layer override precedence.
---

Lute has three reuse mechanisms, each for a different thing: `defs` reuse typed CEL *values*,
`uses`/`extends` reuse *schema*, and **components** reuse *content*. This page covers content
components and schema `extends:` composition.

## Reusable content components

A **component** is a named, parameterized block of lines and staging that is expanded inline
wherever it is invoked. It lives in its own **component file** — a `.lute` document whose
frontmatter declares `component: <name>` and, optionally, `params:` (typed exactly like a
[def param](/language/params/), plus, since 0.24.0, the [`speaker`](#speaker-params) type). The body is a **presentational template**, unless the file
declares [`effects: true`](#components-that-write-state-effects-true).

```lute check="docs/examples/components/greet.component.lute"
---
component: greet
params:
  who: string
uses: ../base.schema.yaml
---

## A Familiar Face

::auto{character=@who action="fade-in-up"}
@narrator: A familiar face steps into the light.
```

*(From [`docs/examples/components/greet.component.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/components/greet.component.lute).)*

A parameter is referenced as `@<param>` in ref and attribute positions, and inside content text via
`{{@param}}` interpolation. `@who` binds to the invocation argument at expansion time — it is legal
in the `character` position because that attribute is `string`-typed.

A `{{…}}` interpolation in a component line renders a **number, bool, or enum** param (§7.6) and,
since dsl 0.23.0, a **`string`** param whose argument is a literal. So `who: string` above could
also appear as `{{@who}}` in a line. See [Sentences in string params](#sentences-in-string-params).

The `uses:` line is the component's own [content vocabulary](/language/vocabulary/) import — since
`0.9.0` `action="fade-in-up"` resolves against a declared `action` domain, and a component file has
to reach one to check on its own. Through `::use` it is the **importing** document's vocabulary that
applies (see [the known limitation](/language/vocabulary/#known-limitation-a-component-body-resolves-against-the-importing-document)), so both sides declare it.

A scene imports components via a `components:` frontmatter key (canonicalized, cycle-checked, and
diamond-deduped like `uses:` — see [Imports](/language/imports/)), then invokes one with the
reserved built-in directive **`::use`**:

```lute check="docs/examples/components/scene.lute"
---
kind: scene
character: demo
season: 1
episode: 2
uses: ../base.schema.yaml
components: [greet.component.lute]
---

## Greeting by Component

::use{component="greet" who="marina"}
@narrator: And the scene carries on.
```

*(From [`docs/examples/components/scene.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/components/scene.lute).)*

`::use` expands the named component's body inline, binding each `@param` to the matching named arg;
argument count and type are checked (`E-COMPONENT-ARG`), and naming a component from no imported
file is `E-COMPONENT-UNDECLARED`. A literal argument is substituted into the expansion, so
`{{@weather}}` in a component line compiles to `Outside: grey.` for `weather="grey"`.

An argument may also be a **def of the calling document** — `::use{component="greet" tier=@mood}`.
The component still reads no state itself; the caller's def is where the state read lives, and it
is checked where it lands:

- for an enum param, every value the def body can produce must be a member, else `E-COMPONENT-ARG`
  (`@mood` = `"run.n > 2 ? 'warm' : 'hot'"` against `{ enum: [cold, warm] }` fails on `hot`);
- in a `<match on="@tier">`, it dispatches at run time like any subject;
- in content text, `{{@n}}` stays a placeholder naming the caller's def, and the engine evaluates
  it (a number, bool, or enum param; a `string` param that a line interpolates takes only a
  literal, [below](#sentences-in-string-params));
- in a directive attribute, it must fold to a constant, and a state-dependent def is
  `E-ATTR-DEF-DYNAMIC`.

### Sentences in string params

A component line may interpolate a `string` param (dsl 0.23.0). A literal `::use` argument is
substituted into the line when the component expands, like any other literal argument, so each
call site ships its own finished sentence:

```lute check
---
component: cheers
params:
  to: string
---

## A Toast

@mira: To {{@to}}!
@oskar: To {{@to}}.
```

A scene with `id: harbor.wake` imports it and uses it twice:

```lute
::use{component="cheers" to="the harbor"}
@oskar: Again?
::use{component="cheers" to="absent friends"}
```

Each expansion's lines carry their own text under their own [component-scoped](#line-identity)
`lineId`:

| `lineId` | Text |
|---|---|
| `harbor.wake.cheers#1.mira_0010` | To the harbor! |
| `harbor.wake.cheers#1.oskar_0010` | To the harbor. |
| `harbor.wake.cheers#2.mira_0010` | To absent friends! |
| `harbor.wake.cheers#2.oskar_0010` | To absent friends. |

A translator or a voice actor therefore gets `To absent friends!` as a line of its own, not a
template with a hole in it. The argument has to be a literal. A string `@def` of the caller is only
known when the engine evaluates it, and a `{{…}}` interpolation renders only number, bool, and enum
values at run time, so `::use{component="cheers" to=@place}` is `E-REF-TYPE` at the argument. The
check follows the param through nested components, so a component that passes its own `@to` on to
`cheers` needs a literal from its caller too.

A literal argument may itself hold `{{…}}` interpolations (dsl 0.26.0 §3.1). They keep their
placeholder records after expansion, exactly as in a line written in the host:
`::use{component="cheers" to="you, {{userName}}"}` compiles to the text `To you, {{userName}}!` with
`"placeholders": [{"kind": "reserved", "token": "userName"}]`, so the engine renders the player's
name. Before 0.26.0 the placeholder record was dropped and the braces shipped as literal text.

### Component body rules

A component body is **presentational**: lines, staging directives, and `@param` refs only. It may
**not** read scene/run state, may **not** write it unless the file declares
[`effects: true`](#components-that-write-state-effects-true), and may **not** contain logic blocks (`E-COMPONENT-BODY`) —
pass values in through params instead (a caller's def is a legal argument, above). One notable
exception: a `<match>` that dispatches on the component's own param is admitted, because dispatch on
a param is a pure read of an invocation argument, not of ambient state:

```lute check="docs/examples/components/reaction.component.lute"
---
component: reaction
params:
  tier: { enum: [cold, warm, fond] }
uses: ../base.schema.yaml
---

## The Tiered Greeting

<match on="@tier">
  <when is="fond">
    @marina{emotion="delighted"}: You remembered!
  </when>
  <when is="warm">
    @marina{emotion="content"}: Not bad at all, Mr. Fixer.
  </when>
  <when is="cold">
    @marina{emotion="neutral"}: ...Shall we begin?
  </when>
</match>
```

*(From [`docs/examples/components/reaction.component.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/components/reaction.component.lute).)* The three arms cover the declared
enum and a param is never `unset`, so no `<otherwise>` is needed.

The second exception (dsl 0.26.0 §3.3) is the result of a plugin directive in the body itself: a
component that calls `::battle{… resultKey="fight"}` may read `scene.battle.fight.won`. See
[A component's own directive results](#a-components-own-directive-results).

### Components that write state: `effects: true`

Some reusable content has a consequence: a companion's nod that also raises their approval, a
search that asserts what was found. A component file that declares **`effects: true`** (dsl 0.24.0
§4) may `::set`, `::assert` and `::retract`, use a plugin directive that writes state, and `::use`
another effects component:

```lute check
---
component: nod
effects: true
params:
  who: speaker
  delta: number
state:
  run.approval: { type: number, default: 0, per: companion }
entities:
  companion: { members: [isolde, corvin] }
---

## A Nod

@narrator: {{@who}} nods.
<match on="@who">
  <when is="isolde">
    ::set{run.approval.isolde += @delta}
  </when>
  <when is="corvin">
    ::set{run.approval.corvin += @delta}
  </when>
  <otherwise>
    @narrator: Nobody else minds.
  </otherwise>
</match>
```

(The `state:` and `entities:` above let the file check on its own. In a project the component
`uses:` the shared schema, and the declaration that counts is the host's.)

The writes belong to the **host**. Each one is checked at every `::use` against the host document's
schema — an undeclared path, a `::set` of the wrong type, an `owner: engine` path, a relation's
vocabulary and domain, permissions — and the error is reported at the `::use`. A host whose schema
declares no `run.approval` gets:

<!-- lute-diagnostics -->
```
./scenes/market.lute:12:1: error [E-UNDECLARED] `::set` target `run.approval.isolde` is not declared in the `state:` schema (dsl §7.3.4)
```

The writes compile into the host's commands where the `::use` sits, and a `<match>` on a param keeps
only the arm the argument selects. In a scene `camp.fire` whose cast names `corvin` "Corvin Hale",
`::use{component="nod" who="corvin" delta="2"}` compiles to:

```json
{"kind": "line", "addr": "001-0200", "role": "narration", "speaker": "narrator", "text": "Corvin Hale nods.", "lineId": "camp.fire.nod#1.narrator_0010"}
{"kind": "set", "addr": "001-0300", "path": "run.approval.corvin", "op": "+=", "value": "2", "expr": {"lit": 2.0}}
```

A fact atom in the body may take params as arguments, `::assert{gifted(@who, @item)}` or the same
in a `::retract`. Each `::use` binds the params to its arguments, and the host checks the bound atom
there, like any atom it wrote itself. With `item: string` and an `item` kind of `[locket, map]`,
`::use{component="gift" who="isolde" item="locket"}` compiles to an `assert` of
`gifted(isolde, locket)`, and `item="lockt"` is `E-FACT-DOMAIN` at the `::use`. `check-project`'s
fact analyses see the bound atom too, so a later `holds(gifted(isolde, locket))` is a guaranteed
guard, and a query no `::use` can produce is dead (`E-ARM-DEAD`). A fact's arguments are ground, so
the argument must be a constant: an entity or enum member id, `true` or `false`. A def argument,
`item=@best`, is `E-COMPONENT-ARG`, and the message names the atom. Outside a component body a
`@param` in a fact is `E-FACT-DOMAIN`.

Writing is the only thing `effects: true` unlocks. A guard or match subject in the body still may not
read state (`E-COMPONENT-STATE`), apart from the results of its own plugin directives
([below](#a-components-own-directive-results)). A presentational component that `::use`s an effects
component is `E-COMPONENT-BODY`, and the message asks for `effects: true` on it as well. Without
`effects: true`, a write is `E-COMPONENT-BODY`, as before.

### Speaker params

A param typed **`speaker`** (dsl 0.24.0 §4) takes a cast id, as `who` does in `nod` above:

- `{{@who}}` in a line renders the member's display `name` from the [cast](/language/dialogue-and-cast/#the-cast),
  or the id when the member has none. So `who="corvin"` renders `Corvin Hale nods.`
- The line attribute `as=@who` renders the display name too, and so does `{{@who}}` inside a quoted
  line attribute (`as="{{@who}}, again"` compiles to `"as": "Corvin Hale, again"`) (dsl 0.26.0
  §3.1). Before 0.26.0 `as=@who` shipped the id and the braces in an attribute string stayed
  literal.
- Other attributes (`::auto{character=@who}`, a plugin directive's `foe=@who`) and
  `<match on="@who">` see the id. The match ranges over the declared cast plus
  `narrator`, so arms named after cast members check against it: a typo arm is
  `E-WHEN-LITERAL-DOMAIN`, and a match with neither every member nor `<otherwise>` is
  `E-NONEXHAUSTIVE` (`narrator` counts as a member).
- An argument outside the cast is `E-CAST-UNKNOWN` at the `::use`, with a did-you-mean
  (`who="corvn"` suggests `corvin`). Without a declared cast any identifier is accepted.
- The argument must be a literal. A def argument, `who=@lead`, is `E-COMPONENT-ARG`, because the
  name is chosen when the component expands, not at run time.

### Speaking as a param: `@@who:`

A component body may speak **as** the member a `speaker` param names (dsl 0.26.0 §3.2). `@@who:`
is a content line whose speaker is the argument of `who` at each `::use`:

```lute check
---
component: greeter
params:
  who: speaker
  line: string
  times: { type: number, default: 1 }
---

## Greeter

@@who: {{@line}}
@narrator{as=@who}: ({{@times}} times today.)
```

`::use{component="greeter" who="mira" line="Morning."}` in a scene `harbor.gate` compiles the first
line with `"speaker": "mira"` and `"lineId": "harbor.gate.greeter#1.mira_0010"`: the member's own
portrait, voice and line identity, as if the host had written `@mira: Morning.`. The member's cast
checks are judged at the `::use`: an `emotion=` outside its `emotions:` is `E-BAD-ENUM`, a line its
`present:` does not cover is `W-CAST-ABSENT` (once per member and guard), and a line after the member
left the stage is `W-STAGE-ABSENT`.

`@@x` for a param that is not a `speaker` param, for a name that is no param, or in a document that
is no component is `E-COMPONENT-ARG`:

<!-- lute-diagnostics -->
```
./components/challenge.component.lute:11:1: error [E-COMPONENT-ARG] `@@taunt:` speaks as the cast member a `speaker` param names, and `taunt` is not a `speaker` param — declare `taunt: speaker` (dsl 0.26.0 §3.2)
./scenes/pier.lute:9:1: error [E-COMPONENT-ARG] `@@mira:` speaks as a component's `speaker` param `mira`, and this document is no component — write the cast id (`@mira:`) (dsl 0.26.0 §3.2)
```

### Param defaults

A param may declare a **`default:`** (dsl 0.26.0 §3.3) in the long form
`name: { type: <type>, default: <value> }`, as `times` does in `greeter` above. A `::use` that omits
the argument takes the default, and the default is judged at the `::use` like the argument it
stands for. It is a literal or a `@def` the **host** resolves; quote a def, since YAML does not start
a plain value with `@`:

```yaml
params:
  kind: { type: { enum: [rival, champion] }, default: rival }
  won:  { type: bool, default: "@wonFight" }
```

A `params:` entry that is neither `name: <type>` nor the long form is `E-COMPONENT-PARSE`, and the
message spells both shapes.

### A component's own directive results

A plugin directive that declares result slots, such as a bridge `::battle` that writes
`scene.battle.<resultKey>.*`, may sit in a component body, and the body may read the slots it
declares (dsl 0.26.0 §3.3). This is the trainer battle of a monster-collecting game, where every
route trainer is one `::use`:

```lute unverified="needs the engine plugin that declares the ::battle bridge directive and the cast; checked with lute check-project on a scratch project that declares both"
---
component: trainerBattle
effects: true
params:
  who: speaker
  intro: string
  win: string
  lose: string
  prize: { type: number, default: 100 }
---

## Trainer battle

@@who: {{@intro}}
::battle{foe=@who resultKey="fight"}
<match on="scene.battle.fight.won">
  <when is="true">
    @@who: {{@win}}
    ::assert{defeated(@who)}
    ::set{run.money += @prize}
    @narrator: {{userName}} got {{@prize}} coins for winning!
  </when>
  <otherwise>
    @@who: {{@lose}}
  </otherwise>
</match>
```

`scene.battle.fight.won` is not ambient state: it is the slot this body's own `::battle` declares,
so a `<match>`, a guard or a `{{scene.battle.fight.turns}}` interpolation may read it. Any other
`scene.*` or `run.*` read is still `E-COMPONENT-STATE`.

Each `::use` also **declares** those slots in its host, bound to the use's arguments (dsl 0.26.0
§3.1). The host's check, its compiled `state` table (`scene.battle.fight.won`,
`scene.battle.fight.turns`), trace mocks and `lute play` all see them, and the host declares nothing
of its own. `lute play` types the bridge's answer by that slot, or else by the capability's
`result:` shape, and refuses an answer it cannot type instead of storing it as a string. Before
0.26.0 a host had to declare the slot in its schema, and the component took the outcome as a
param (`won=@wonFight`) instead of reading it.

### Guarding a `::use`: `when=`

`::use` takes `when="<condition>"` like [`::set`](/language/directives/#guarding-a-directive-when)
(dsl 0.26.0 §4). The whole expansion runs, or none of it: the lines, the `::battle` and the writes
alike. The use's argument reads are judged under its guard. A dungeon floor skips a trainer who is
already beaten in one line:

```lute
::use{component="trainerBattle" who="lassMina" intro="You look strong, {{userName}}." win="Oh no!" lose="Hehe." prize="200" when="!holds(defeated(lassMina))"}
```

`lute play` prints a skipped use with its arguments, `skip ::use{component="trainerBattle"
who="lassMina" …} — when: false`, and trace and test skip it the same way.

### Engine ids through a param

A plugin may type a directive attribute by a project entity kind, `::give{item}` by
`{ entity: bagItem }` (see [Manifests](/plugins/manifests/#engine-ids-typed-by-an-entity-kind)).
The check reaches through components (dsl 0.26.0 §2.5). When a body passes a param whole to such an
attribute (`::give{item=@item}`, through nested `::use`s as well), or the param is itself typed
`{ entity: bagItem }` or `{ domain: … }`, each `::use` argument, or the param's `default:`, is
checked against the kind, with a did-you-mean:

<!-- lute-diagnostics -->
```
./scenes/route2.lute:9:30: error [E-BAD-ENUM] `superPotoin` is not a member of entity kind `bagItem` (attribute `item` of `::give` in component `gift`) — did you mean `superPotion`?
```

[`lute refs <dir> --attr give.item`](/tooling/cli/) lists such a value at the `::use` line that
binds it, marked ``(via component `gift`)``.

### Line identity

Each `::use` expansion is its own identity scope (0.22.0). A line expanded from a component is
addressed `{prefix}.{component}#{n}.{speaker}_{code}`, where `{prefix}` is the host document's
key and `n` counts the host's `::use`s of that component — 1-based, in document order. The scene
above compiles the component's narrator line to `demo.s01ep02.greet#1.narrator_0010` and its own
`@narrator: And the scene carries on.` to `demo.s01ep02.narrator_0010`. A voiced line's default
`voiceKey` takes the same scope (see [Frontmatter & profiles](/language/frontmatter-and-profiles/)
for the `identity:` templates).

Take a component with one tagged and one untagged line:

```lute check
---
component: toast
---

## A Toast

@mira{code="0010"}: To the harbor.
@oskar: To the harbor.
```

A scene with `id: harbor.dinner` imports it (`components: [toast.component.lute]`) and uses it
twice, between lines of its own:

```lute
@mira{code="0010"}: Sit, everyone.
::use{component="toast"}
@oskar: Again?
::use{component="toast"}
@mira: Last one.
```

Every line gets its own `lineId`:

| Line | `lineId` |
|---|---|
| `@mira{code="0010"}: Sit, everyone.` | `harbor.dinner.mira_0010` |
| first `toast` | `harbor.dinner.toast#1.mira_0010`, `harbor.dinner.toast#1.oskar_0010` |
| `@oskar: Again?` | `harbor.dinner.oskar_0010` |
| second `toast` | `harbor.dinner.toast#2.mira_0010`, `harbor.dinner.toast#2.oskar_0010` |
| `@mira: Last one.` | `harbor.dinner.mira_0020` |

- **Each expansion back-fills its own untagged codes.** The component's `@oskar` is `0010` at every
  use, however many host lines precede it, and the host's own untagged lines keep the codes
  `lute tag` writes for them (`@oskar: Again?` → `0010`, `@mira: Last one.` → `0020`).
- **Codes no longer collide across the boundary.** Two uses of a tagged component, or a component
  line sharing a code with a host line (both `@mira` lines above are `0010`), compile clean with
  distinct ids. Before 0.22.0 both were `E-DUP-LINE-CODE`.
- **A nested `::use` adds a segment.** It counts within its enclosing expansion: a `feast`
  component that uses `toast`, itself used twice, gives `…feast#1.toast#1.mira_0010` and
  `…feast#2.toast#1.mira_0010`.
- **`lute loc export` emits the same ids.**

Migrating from 0.21: every line a component contributes has a new `lineId` and `voiceKey`, so
re-export the localization and voice manifests of each document that `::use`s a component.

## Schema `extends:`

Where `uses:` unions **peer** schemas (a name declared by two peers is an error), **`extends:`**
names one or more **base** schemas that a document *refines*. A base is a lower-precedence layer.

```yaml
# base.schema.yaml
state:
  run.blessed: { type: bool, default: false }
defs:
  wealthy: { type: bool, cel: "run.blessed" }
```

```yaml
# child.schema.yaml
extends: base.schema.yaml
state:
  run.blessed: { type: bool, default: true }   # overrides the base default
```

*(From [`docs/examples/child.schema.yaml`](https://github.com/journeyWorker/lute/blob/main/docs/examples/child.schema.yaml) and [`base.schema.yaml`](https://github.com/journeyWorker/lute/blob/main/docs/examples/base.schema.yaml).)*

Precedence, low → high: a document's `extends` bases (recursively) < its `uses` peers < its own
inline `state:`/`defs:`. When the extending layer redeclares a base name, it **overrides** it — no
duplicate error. A `defs` entry is replaced wholesale. A `state` entry is overridden too, but
because persisted state must keep a stable type, an override that changes the declared **type** is
`E-EXTENDS-STATE-TYPE`; a `default`-only refinement (same type) is allowed silently. `extends` edges
share the same DAG discipline as `uses:` — cycles, missing files, and parse errors reuse the
`E-USES-*` diagnostics.
