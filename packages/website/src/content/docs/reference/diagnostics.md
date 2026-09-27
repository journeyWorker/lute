---
title: Diagnostics reference
description: "Every diagnostic code Lute reports: what raises it, and the spec sections behind it."
---

<!-- Generated from crates/lute-cli/src/codes.rs. Edit the registry, then run
     LUTE_BLESS_DIAGNOSTICS=1 cargo test -p lute-cli --bins codes -->

Every diagnostic Lute prints carries a code. `E-` codes are errors: the document fails the check and `lute check` exits 1. `W-` codes are warnings: the document passes, unless `--deny <CODE>` or `--deny-warnings` promotes them. An `E-` code is never printed as a warning: `check-project --wip` reports the dead guards it spares as `W-WIP`, and the message names the error code the same guard has without the flag. `lute --explain <CODE>` prints a code's entry below in the terminal, and an editor links each code to its section here.

A message says what is wrong in plain words. The spec sections behind a code are listed under it, each linked to its proposal, and `--json` output carries them in each diagnostic's `spec` field.

A position `file:line:column` counts lines and columns from 1, and the column counts characters, not bytes: a Korean syllable or an emoji before the error is one column. `--json` `span.column` and `lute scenario … reach` `causes[].column` are the same number. The language server reports UTF-16 positions, as LSP requires.

## Errors

### E-ACCEPT-TARGET

An `::accept` directive names no quest, gives `quest` a value that is not a quoted quest id, gives `at` a value other than `"nextRun"`, or targets a quest declared `accept="external"` that already activates with its parent (making the acceptance a no-op).

Spec: [dsl 0.21.0 §7a.3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.21.0.md), [dsl 0.24.0 §2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md), [dsl 0.25.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.25.0.md)

### E-AGE-GATE

An age-gated `<match on="app.rating">` covers neither a `teen` arm nor an `<otherwise>`, so a release build could hit no matching case.

Spec: [dsl §11.2](/spec/)

### E-APP-READONLY

A `::set` directive writes to the `app.*` namespace, which content may never write because the engine or settings layer owns it exclusively.

Spec: [dsl §9.5](/spec/)

### E-ARM-DEAD

A gated content line, `<match>` arm, `<choice when>`, or `::next{when}` carries a `when` guard that is provably always false, so it can never be shown or taken.

Spec: [dsl 0.4.0 §5.2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.4.0.md), [dsl 0.4.0 §7.2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.4.0.md), [dsl 0.20.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.20.0.md)

### E-AS-REMOVED

A choice uses the removed `as` attribute; `into` names the run fact a choice records.

Spec: [dsl 0.10.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.10.0.md)

### E-ASSET-DECOMPOSE

An asset id string has the wrong number of `/`- or `.`-separated segments for its declared kind, or a fixed (`const`) segment does not match the value the kind requires.

Spec: [dsl §7.2](/spec/)

### E-ASSET-SEGMENT

A segment of an asset id is not a member of its declared enum, is not a number where one is required, or is not a known id in its provider's catalog.

Spec: [dsl §7.2](/spec/)

### E-ASSET-UNKNOWN-ID

A pure-query asset kind's id string is not a known id in its declared provider's catalog.

Spec: [dsl §7.2](/spec/)

### E-AT-CONTEXT

A directive (plain `::directive` or `::use`) carries the timeline-position attribute `at` outside a `<track>` clip, where `at` is not valid.

Spec: [dsl §7.5](/spec/)

### E-ATTR-DEF-DYNAMIC

A `@def` used as a directive attribute value, or as a `::use` argument landing in a component-body attribute, does not fold to a compile-time constant because it depends on state.

Spec: [dsl §5.1](/spec/)

### E-ATTR-QUOTE

An attribute value is delimited with a single quote or a curly (word-processor) quote instead of the required straight double quote `"`.

Spec: [dsl §4.4](/spec/), [dsl §4.5](/spec/)

### E-ATTR-TYPE

An attribute's value does not match its declared type — for example a non-numeric `duration`/`zoom`/`shake`, a non-boolean flag, a bare identifier where a quoted string is required, or a value naming no member of its provider, domain, or entity kind.

### E-BAD-ENUM

A value is not a member of the closed enum, domain, or entity kind it must belong to — for example a content line's `emotion=` outside the speaker's declared `emotions:`, or an entity id outside its kind.

Spec: [dsl 0.24.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md)

### E-BEAT-ATTR

A beat's `on`, `target`, `priority`, or `once` attribute is malformed — a non-identifier `on`, a `target` on an occasion not declared `target: true`, a non-integer `priority`, a `once` outside `run`/`user`/`false`, a beat key with no `on`, a `when` reading the scene's own not-yet-existing `scene.*` state, or a `spentBy` beside `once: false` or a `share` key.

Spec: [dsl 0.21.0 §3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.21.0.md), [dsl 0.21.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.21.0.md), [dsl 0.28.0 §6](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### E-BEAT-ID-DUP

Two declarations of one lore document share an id: a bundle `<beat>` id repeated, or an `<entry>` whose id is a `<beat>`'s — the beat's canonical id `<document id>.<id>` is also the entry's alias, so `visited()` or a play's `expect.winner` would name both.

Spec: [dsl 0.28.0 §7](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### E-BEAT-UNREACHABLE

A scene beat's `when` guard is provably always false, so the beat can never be selected.

Spec: [dsl 0.21.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.21.0.md)

### E-BRANCH-ALL-GUARDED

A non-empty `<branch>` has every `<choice>` guarded by a `when`, so every guard could be false at once and present an empty menu; at least one unguarded choice is required.

Spec: [dsl §11.1](/spec/)

### E-BRANCH-EMPTY

A `<branch>` or `<hub>` contains no `<choice>` at all, which would flatten to an unroutable choice.

Spec: [dsl §7.3](/spec/)

### E-BRANCH-PROMPT

A `<branch prompt>` or `<hub prompt>` value is missing or an empty string, but the prompt must be the non-empty sentence the UI shows verbatim.

Spec: [dsl 0.11.1 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.11.1.md), [dsl 0.23.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.23.0.md)

### E-BRANCH-TIMEOUT

A `<branch timeout>` value does not parse as a positive whole number of seconds.

Spec: [dsl 0.11.1 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.11.1.md)

### E-CAPABILITY-MISMATCH

Two documents in the same project resolve different capability snapshots, so the project has no single `capabilityVersion` to index.

Spec: [dsl §13](/spec/)

### E-CAST-UNKNOWN

A content line's speaker, or a `::auto{character}`/`::camera{focus}` literal, names an id outside the project's declared cast.

Spec: [dsl 0.23.0 §7](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.23.0.md), [dsl 0.24.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md)

### E-CEL-PARSE

A CEL expression in a condition slot, def body, or `present:`/beat `when` does not parse as valid CEL syntax.

Spec: [dsl 0.4.0 §8.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.4.0.md)

### E-CEL-PROFILE

A CEL expression uses a construct outside the restricted Lute-CEL profile — a disallowed function call, a comprehension macro, a map/struct literal, a bare identifier that is not a state path or def, a reserved internal token, or `visited(…)` used outside a condition slot.

Spec: [dsl §8.4](/spec/), [dsl 0.21.0 §7a.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.21.0.md)

### E-CEL-TYPE

A condition's types cannot mean what is written: a comparison between a bool, a number and a string (`visited('x') > 2`, `run.oil == true`, `run.day == 'monday'`), an ordering of anything but numbers (`run.hour >= 'h03'`), a non-bool operand of `&&` / `||` / `!` / `?:` or condition, arithmetic that cannot be computed, or an operand of the integer modulo operator `%` that is not an integer.

Spec: [dsl 0.24.0 §1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md), [dsl 0.28.0 §1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### E-CHAPTERS

The project's `chapters:` is malformed — not a list of `{ on, scenes }` chains, a key that is neither (a chain names its occasion with `on:`, not `occasion:`), an entry that is no scene id, a scene listed twice, two chains on one occasion — or the manifest still uses the retired `sequence:` key; or a chain names an occasion no plugin declares (or, shape-only, a near-miss of one other beats answer), lists an id no scene declares (a bundle beat, lore entry or document is named as such), lists a scene whose own `on:` answers another occasion, or, on an occasion raised for a target, lists a scene with no `target:` (it would play for every target). A malformed chain is not applied; the other chains are. Reported at the manifest line — a missing `target:` at the scene's `id:` — and the documents are still checked.

Spec: [dsl 0.28.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### E-CHOICE-DUP

A `<branch>` or `<hub>` declares two `<choice>` elements with the same `id`, but choice ids must be unique within their branch or hub.

Spec: [dsl §11.1](/spec/)

### E-CHOICELOG-READ

A guard or condition reads a reserved choice-log path, which may not be read from a guard or condition.

Spec: [dsl §9.6](/spec/)

### E-CLIP-OVERLAP

Two clips in the same `<track>` have overlapping `[at, at+duration)` intervals.

Spec: [dsl §11.4](/spec/)

### E-CLIP-TIMING

A single `<track>` clip carries both `at` (an absolute timeline position) and `delay` (a relative nudge), which are mutually exclusive on one clip.

Spec: [dsl §7.5](/spec/), [dsl §11.4](/spec/)

### E-CLOCK-DECL

A `clock:` declaration is malformed (a missing field, a `last.slot` not in `slots`, `days: 0`, both `last` and `days`), or a project declares more than one `clock:`.

Spec: [dsl 0.24.0 §1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md)

### E-CLOCK-END

A `lute play`/`lute test` `advance:` step runs while a finite clock has ended, or starts past its last position — the clock already raised its last `dayEnd` and stopped, so play a `newRun` or end the script.

Spec: [dsl 0.27.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.27.0.md)

### E-COMMENT-UNTERMINATED

A `/* … */` block comment ran to the end of the file without a closing `*/`.

Spec: [dsl §4.2](/spec/)

### E-COMPILE-COMPONENT

A `::use` invocation reaches compilation still unresolved — used inside a `<timeline>` clip (not allowed), naming no resolvable component, or with args that do not match the component's params — a case the check gate should already have caught.

### E-COMPILE-EXPAND

A CEL slot or beat `when` fails to expand at compile time — an expansion cycle, an unknown def, or an arity mismatch.

### E-COMPILE-INTERNAL

The compiler reached a state it cannot recover from; this is a Lute bug, so please report it with the document that triggers it.

### E-COMPONENT-ARG

A `::use` invocation's arguments do not match the named component's declared params — an unknown argument, a missing required param, a value of the wrong type, an incompatible default, or (for a `speaker` param) an argument that is not a literal cast id.

Spec: [dsl §13](/spec/), [dsl 0.24.0 §3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md), [dsl 0.26.0 §3.3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.26.0.md)

### E-COMPONENT-BODY

A component body contains a construct that is not presentational — a `::set`, `<branch>`, `<hub>`, `<timeline>`, `<objective>`, `<on>`, `::assert`, or `::retract` that writes or affects state, or a `::use` of an effectful inner component without declaring `effects: true` itself.

Spec: [dsl 0.4.0 §6.2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.4.0.md), [dsl 0.24.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md)

### E-COMPONENT-CYCLE

A chain of `::use` invocations expands into a cycle across components.

Spec: [dsl §13](/spec/)

### E-COMPONENT-DUP

Two different component files declare the same `component:` name.

### E-COMPONENT-PARSE

A component file cannot be read, resolved, or parsed cleanly — an unresolvable `components:` import path, a missing `component:` name, or a malformed `params:` entry.

### E-COMPONENT-STATE

A component body reads or writes ambient state directly — a CEL reference to a state path, a fact query, or a directive that declares state/bridge-result writes — instead of binding it through a param.

Spec: [dsl 0.4.0 §6.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.4.0.md), [dsl 0.4.0 §6.2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.4.0.md), [dsl 0.24.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md)

### E-COMPONENT-UNDECLARED

A `::use`'s `component` attribute names a component absent from the resolved `components:` table.

Spec: [dsl §13](/spec/)

### E-CONN-CYCLE

The project's `after` prerequisite graph contains a cycle, so no evaluation order can satisfy every `after` clause simultaneously.

Spec: [dsl §4.1](/spec/)

### E-CONN-EPISODE-ID-DUP

Two documents (or a document and a derived key) share the same document id in the project's shared id namespace.

Spec: [dsl 0.15.0 §2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.15.0.md), [dsl 0.19.0 §2.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.19.0.md)

### E-CONN-FORMULA-TOO-COMPLEX

An `after` prerequisite formula's atom count exceeds the defensive cap, a pattern typical of a pathological or machine-generated formula rather than one an author types by hand.

Spec: [dsl §4.1](/spec/)

### E-CONN-PROFILE

An `after` prerequisite formula uses a construct outside the restricted prerequisite profile — anything other than `visited(…)`/`completed(…)`/`active(…)` atoms combined with `&&`/`||` and parentheses.

Spec: [dsl §4.1](/spec/)

### E-CONN-UNKNOWN-NODE

An `after` prerequisite formula's `visited(K)`/`completed(Q)`/`active(Q)` atom names a node that does not exist anywhere in the project.

Spec: [dsl §2.3](/spec/), [dsl §4.1](/spec/)

### E-CONN-UNREACHABLE

A scene, quest, or beat is provably unreachable — no evaluation order can ever satisfy its `after` prerequisite.

Spec: [dsl §4.1](/spec/), [dsl §4.2](/spec/)

### E-CONTENT-LINE-BRACKET

A content line's attributes are written with `[…]` instead of the required `{…}` (the same delimiter `::directive{…}` uses).

Spec: [dsl §2.1](/spec/)

### E-CONTENT-OUTSIDE-SHOT

A content-shaped line (`@speaker…`, `::directive`, a `<tag>`) appears before the document's first `## ` shot heading, since content only belongs inside a shot body.

Spec: [dsl 0.5.0 §2.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.5.0.md), [dsl 0.6.0 §3.3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.6.0.md)

### E-DATALOG-FUNCTION

An `::assert`/`::retract` payload, a `facts:` entry, or a `rules:` term uses a compound/function term such as `f(g(x))`, but facts and rule terms admit only ground identifiers/booleans.

Spec: [dsl 0.3.0 §7.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md)

### E-DATALOG-GUARD-FACT

A rule-body CEL guard reads the fact store or narrative time via `holds`, `count`, `validAt`, or `now`, which a guard is not allowed to depend on.

Spec: [dsl 0.3.0 §7.2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md), [dsl 0.3.0 §7.3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md)

### E-DATALOG-PARSE

A `facts:`/`rules:` entry is not a quoted string, or a quoted `facts:`/`rules:` string fails to parse as a well-formed ground fact or rule.

Spec: [dsl 0.3.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md), [dsl 0.3.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md)

### E-DATALOG-UNSAFE

A rule's negated atom, `!=` comparison, or CEL guard reads a variable that no positive body atom binds first, so the rule cannot be safely evaluated.

Spec: [dsl 0.27.0 §3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.27.0.md), [dsl 0.24.0 §3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md)

### E-DATALOG-UNSTRATIFIED

A rule's negation edge closes a cycle in the predicate-dependency graph (including `p :- not p`), so the ruleset cannot be stratified.

Spec: [dsl §7.2](/spec/)

### E-DEF-DECL

A `defs:` entry is malformed — not a CEL string or a `{ type?, params?, cel }` mapping, an unknown key, a missing `cel:`, or a `type:` that is neither inferable from the body nor consistent with it.

Spec: [dsl 0.21.0 §7b](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.21.0.md)

### E-DEFAULTS-KEY

A manifest's `defaults:` block names a key outside the closed defaultable-frontmatter set, or gives a defaultable key a value of the wrong shape.

Spec: [dsl 0.10.0 §6.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.10.0.md)

### E-DELIVERY-CONFLICT

A content line sets more than one of the mutually exclusive delivery flags `mono`, `os`, `vo`.

Spec: [dsl 0.2.2 D7](/spec/)

### E-DELIVERY-FLAG-VALUE

A delivery flag (`mono`/`os`/`vo`) is given a value, but a delivery flag is bare and takes none.

Spec: [dsl 0.2.2 D7](/spec/)

### E-DELIVERY-NARRATOR

A `narrator` content line carries a delivery attribute, but narration takes no delivery.

Spec: [dsl 0.1.0 §12.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.1.0.md)

### E-DEPENDS-CYCLE

Two or more plugins' `depends` declarations form a cycle, so their activation order cannot be resolved.

### E-DEPENDS-UNRESOLVED

A plugin's `depends` names another plugin id that is not installed in the project.

### E-DEPENDS-VERSION

A plugin's `depends` names an installed plugin whose version does not satisfy the declared version range.

### E-DERIVE-TIER

A relation is declared `derive: true` but also declares a write `tier:`, though a derived relation has no write tier of its own.

Spec: [dsl 0.3.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md), [dsl 0.3.0 §7.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md)

### E-DERIVE-UNDECLARED

A `rules:` entry's head names a relation that is not declared `derive: true` (a base relation, a reserved relation, or an entity-kind name), though only a derived relation may be a rule head.

Spec: [dsl 0.3.0 §7.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md), [dsl 0.3.0 §3.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md)

### E-DERIVED-WRITE

Content asserts or retracts a relation declared `derive: true`, though a derived relation is computed by `rules:` and must not be written directly.

Spec: [dsl 0.3.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md)

### E-DOLLAR-OUTSIDE-MATCH

`$` (the match subject) is used outside a `<match>` block, where it is not valid.

Spec: [dsl §8.2](/spec/)

### E-DOMAIN-DUP

A project's own `enums:`/`entities:` declaration (inline or reached through `uses:`/`extends:`) names a domain that a plugin or the core already declares, though a domain name must have exactly one source.

Spec: [dsl 0.3.0 §3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md), [dsl 0.9.0 D-D](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.9.0.md)

### E-DOMAIN-NAME-CLASH

One name is declared both as an `enums:` domain and as an entity kind — in one document or across the schemas a document merges. Enums and entity kinds share one domain namespace, so the kind's members would silently replace the enum's wherever the name types a value.

### E-DOMAIN-UNKNOWN

A content-line `emotion`/`action` slot, an entity attribute, or an implicit `anchor` read names a domain that no `enums:`/`entities:` declaration defines.

Spec: [dsl 0.9.0 D-C](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.9.0.md), [dsl 0.9.0 D-D](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.9.0.md)

### E-DUP-BRANCH

A `<branch id>` or `<hub id>` repeats an id already used by another branch or hub in the same episode, since branch and hub ids share one uniqueness domain.

Spec: [dsl §11.1](/spec/), [dsl §7.3.2](/spec/)

### E-DUP-LINE-CODE

Two content lines from the same speaker share the same `:line` `code=`, though a (speaker, code) pair must be unique — it is the voice-key/i18n identity join key.

Spec: [dsl §12](/spec/)

### E-DUP-TRACK

Two `<track>`s in a `<timeline>` share the same track key.

### E-DUP-VOICEKEY

Two voiced lines with different text compile to the same `voiceKey`, so one recording would voice both — usually because the `voiceKey` template lacks `{prefix}` and collides across documents.

### E-ENGINE-OWNED-WRITE

An `::set` writes a state path declared `owner: engine`, though the engine writes such a path and content may only read it.

Spec: [dsl 0.22.0 §1.2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.22.0.md)

### E-ENTITY-KIND-CLASH

An id is listed as a member of two different entity kinds' `members:`, though an id must belong to exactly one kind unless one kind is declared a `subsetOf:` the other.

Spec: [dsl 0.3.0 §3.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md), [dsl 0.24.0 §3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md)

### E-ENTITY-KIND-SHAPE

An entity kind declaration is malformed — it declares neither or both of `members:`/`open:`, has an unknown key, lists a member more than once, or a `labels:`/`add:` entry names a member the kind doesn't have or targets an `open:`/unknown kind.

Spec: [dsl 0.3.0 §3.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md), [dsl 0.26.0 §2.3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.26.0.md), [dsl 0.27.0 §2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.27.0.md)

### E-ENTRY-ATTR

A `<entry>` attribute has a malformed shape — a non-quoted-string value, a missing or invalid `id`, or a `series=`/`order=` authored under a document-level `series:`.

Spec: [dsl 0.19.0 §3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.19.0.md), [dsl 0.19.0 §2.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.19.0.md)

### E-ENTRY-ID-DUP

Two `<entry>` elements share the same `id`, though entry ids must be unique across the document (and project-wide under `check-project`).

Spec: [dsl 0.19.0 §3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.19.0.md)

### E-ENTRY-SERIES-ORDER

Two `<entry>` elements resolve to the same `(series, order)` position, though each position in a series must name exactly one entry.

Spec: [dsl 0.19.0 §2.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.19.0.md), [dsl 0.19.0 §3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.19.0.md)

### E-ENTRY-UNREACHABLE

A lore entry's `when` eligibility guard provably never holds, so the entry can never be presented.

Spec: [dsl 0.20.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.20.0.md)

### E-ENUM-DEFAULT-NOT-MEMBER

A domain's `default:` value is not one of its own declared members.

Spec: [dsl 0.9.0 D-D](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.9.0.md)

### E-ENUM-EXITS-NOT-MEMBER

A domain's `exits:` list names a value that is not one of its own declared members.

Spec: [dsl 0.9.0 D-D](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.9.0.md)

### E-ENUM-LABEL-NOT-MEMBER

A domain's `labels:` key names a value that is not one of its own declared members.

Spec: [dsl 0.24.0 §1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md)

### E-ENUM-MISSING-SEMANTICS

A domain occupies a slot (such as `anchor`) that requires `default:` or `exits:` semantics, but the domain (or an `entities:` kind standing in for it) declares neither.

Spec: [dsl 0.9.0 D-D](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.9.0.md)

### E-ENUM-UNEXPECTED-SEMANTICS

A domain declares `default:` or `exits:`, but the slot it fills has no meaning for that semantics.

Spec: [dsl 0.9.0 D-D](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.9.0.md)

### E-EXTENDS-RELATION-SIG

An `extends`-base entity kind, relation, or enum is re-declared by a child schema with a members/open shape flip, a missing base member, or a decl that otherwise differs from the base, though a re-declaration must be a legal superset refinement.

Spec: [dsl 0.3.0 §4.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md)

### E-EXTENDS-STATE-TYPE

An imported schema's state path is re-declared by a deeper `extends` base or the scene's inline `state:` with a different `type`, though persisted state must keep a stable type.

Spec: [dsl §9.2](/spec/)

### E-FACT-DOMAIN

A rule guard, indexed query, or content query compares/binds a relation argument against a variable that ranges over no closed kind, or over a kind that shares no members with the argument's declared domain.

Spec: [dsl 0.27.0 §3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.27.0.md), [dsl 0.3.0 §3.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md)

### E-FACT-EXCLUSIVE

An `::assert{A(x)}` targets arguments where a fact of a mutually exclusive relation already holds, though the two relations are declared exclusive on those arguments.

Spec: [dsl 0.25.0 §1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.25.0.md)

### E-FACT-TIER-WRITE

Content asserts or retracts an `app`-tier base relation, though it is engine-owned/read-only to content, exactly like `app.*` scalar state.

Spec: [dsl 0.3.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md), [dsl 0.3.0 §9.5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md)

### E-FLAG-VALUE

A flag attribute (`<choice once>`/`exit`, `<objective optional>`, `<beat also>`) is given a value other than `true`/`false`; a flag is written bare, and a beat/entry `once` period on a choice is refused.

Spec: [dsl 0.28.0 §1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### E-FRONTMATTER-SCHEMA

A document's frontmatter key, declared by an active plugin, holds a value whose shape does not match the plugin's declared type for that key.

### E-GRAMMAR-NOT-ADMITTED

A document uses a construct its document kind's grammar forbids in that context — such as a `<quest>` in a scene, `<hub>`/`<timeline>` in a quest body, or staging/choices in a lore `<entry>`.

Spec: [dsl 0.2.0 §3.3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.2.0.md), [dsl 0.2.0 §6.7](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.2.0.md), [dsl 0.19.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.19.0.md)

### E-HUB-NO-EXIT

A `<hub>` can never exit: it lacks both an unguarded (`when`-less) `exit` choice and the guarantee that every choice is `once` so the eligible set provably empties.

Spec: [dsl §7.3.2](/spec/), [dsl §11.1.3](/spec/)

### E-IDENTITY-TEMPLATE

A project's `identity.lineId`/`identity.voiceKey` template names an unknown `{token}`, or resolves to an empty string.

Spec: [dsl 0.8.0 §9](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.8.0.md)

### E-INTERP-DEF

A `{{@def}}` interpolation cannot inline into one standalone expression — its body has an expansion cycle, reads `$`, or fails to parse.

### E-INTERP-UNTERMINATED

A `{{…}}` interpolation is not closed before the end of the line.

Spec: [dsl §7.6](/spec/)

### E-INTO-TARGET

A `<choice>`'s `into=` attribute is missing, is not a `run.<path>` string literal, or names a bare `run` or non-`run.*` path.

Spec: [dsl 0.6.0 §2.2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.6.0.md)

### E-INTO-UNDECLARED

A `<choice>`'s `into="run.<path>"` names a path that is not declared in the run schema, so it cannot silently create a field.

Spec: [dsl 0.6.0 §2.2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.6.0.md)

### E-INTO-VALUE

A `<choice>`'s `into=` run-record sugar has a `value` attribute that is missing, not a `true`/`false` literal for a bool path, or incompatible with the target path's declared type.

Spec: [dsl 0.6.0 §2.2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.6.0.md)

### E-KIND-MISSING

A root document declares no `kind:` frontmatter key and the manifest's `defaults:` also names none, so the document's kind cannot be resolved.

Spec: [dsl 0.2.0 §3.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.2.0.md), [dsl 0.19.0 §2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.19.0.md), [dsl 0.10.0 §6.3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.10.0.md)

### E-KIND-NAME-CLASH

An `entities:` block declares the same entity kind name twice, two schemas declare one kind differently, or a name is declared as both an entity kind and a relation.

Spec: [dsl 0.3.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md)

### E-LEGACY-CONTENT-SIGIL

A content line uses the old `:` speaker sigil, which `@` replaced — write `@speaker{…}: text` instead.

Spec: [dsl §7.1](/spec/)

### E-LINT-CONFIG

`lute.lint.yaml` is malformed — a wrong-typed or unknown top-level key, a bad rule override, or a custom rule id that collides with a core rule id.

### E-LINT-EXPR

A lint rule's `when` CEL expression fails to resolve or is mistyped, so that rule is skipped for the document.

### E-LINT-RULE

A plugin or custom lint rule declaration is malformed, such as reusing a core rule's id.

### E-LOCALE-BUNDLE

`lute compile --locales` failed to merge a locale import file — bad syntax, a row with an empty `locale`, or a duplicate `lineId` for the same locale.

### E-LOGIC-CONTENT

A logic block holds a child it does not admit (a `<branch>` or `<hub>` takes only `<choice>`s, a `<match>` only `<when>` and `<otherwise>`), a `<choice>`, `<when>`, `<otherwise>`, `<track>` or `<reward>` stands outside the block it belongs in, or a `<reward>` is not self-closing.

Spec: [dsl §7.3](/spec/), [dsl §7.3.2](/spec/), [dsl 0.16.0 §2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.16.0.md)

### E-LOWER-RECORD-FIELD

A directive's declarative `lower: { record, fields }` mapping names a target field the record does not have, or maps a field it cannot legally write.

### E-LOWER-RECORD-UNKNOWN

A directive's `lower: { record: … }` names something outside the closed set of staging record kinds declarative lowering supports.

### E-MANIFEST

`lute.project.yaml` cannot be read as a manifest: it does not parse, is not a mapping, lacks `defaultProfile:`, or a value has the wrong shape.

Spec: [dsl 0.28.0 §1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### E-MANIFEST-KEY

`lute.project.yaml` has a key it does not define — at the top level, in a profile, or in `identity:` — or a key another layer owns (a schema key such as `terminal:`, a document key that belongs under `defaults:`).

Spec: [dsl 0.28.0 §1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### E-MARK-DUP

A mark id — a `::mark{id}` or a content line's `id=` — is declared more than once anywhere in the document; both share one namespace.

Spec: [dsl 0.12.0](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.12.0.md)

### E-MATCH-DUP-OTHERWISE

A `<match>` contains more than one `<otherwise>` arm, though at most one is allowed.

Spec: [dsl §11.2](/spec/)

### E-MATCH-RELATION-SUBJECT

A `<match on>` subject, directly or via an `@def` it expands to, is a fact query (`holds`/`count`/`validAt`), which match subjects may not be.

Spec: [dsl 0.27.0 §2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.27.0.md), [dsl 0.3.0 §8](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md)

### E-MAYBE-UNSET

A state path is read where it may not yet be set — no default, no dominating `::set`, and no guard proves it is set.

Spec: [dsl §9.4](/spec/)

### E-META-ID

A document's `id:` frontmatter value is empty or contains characters outside `[A-Za-z0-9_.-]+`.

### E-META-MISSING

A required frontmatter key — such as `character`/`season`/`episode`, or an `id:` — is missing from a document that must declare scene identity.

Spec: [dsl 0.15.0 §2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.15.0.md), [dsl 0.15.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.15.0.md)

### E-META-PARSE

A document's frontmatter is not valid YAML, or parses as YAML but is not a mapping.

### E-META-UNKNOWN-KEY

A document declares a top-level meta key that is neither a core key nor owned by an active plugin, or one reserved for another document kind.

### E-META-VALUE

A frontmatter value has the wrong shape — a malformed `extra:` mapping or key, a non-identifier `series:`, a malformed `cast:`/`enums:`/`terminal:` entry, or a bad `effects:` flag.

Spec: [dsl 0.15.0 §3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.15.0.md), [dsl 0.19.0 §2.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.19.0.md), [dsl 0.23.0 §7](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.23.0.md)

### E-MISSING-ATTR

A directive is missing an attribute its declared schema requires.

### E-MOCK-SUBJECT

A `--mock`/`mocks/*.yaml` entry declares no `file:`, names a `file:` path that does not exist or is not a `.lute` document, or disagrees with the document named on the command line.

Spec: [dsl 0.10.0 §8](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.10.0.md)

### E-NEXT-BACKWARD

A `::next{to}` names a mark that is not forward of the `::next` in document order — jumps must go forward only.

Spec: [dsl 0.12.0](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.12.0.md)

### E-NEXT-UNDEFINED

A `::next{to}` names a mark that no `::mark` (or content line `id=`) anywhere in the document declares.

Spec: [dsl 0.12.0](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.12.0.md)

### E-NONEXHAUSTIVE

A `<match>` has no `<otherwise>` and its subject's domain is not fully covered by the `<when>` arms — the message names the missing values or the first uncovered gap.

Spec: [dsl 0.18.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.18.0.md), [dsl §11.2](/spec/)

### E-OBJECTIVE-CONTRADICTION

Two required `<objective>`s of one `<quest>` name `done` predicates over the same state path whose solution sets can never both hold.

Spec: [dsl 0.10.0 §5.2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.10.0.md)

### E-OBJECTIVE-ID-DUP

An `<objective id>` is declared more than once within the same `<quest>`.

### E-OBJECTIVE-ID-MISSING

An `<objective>` within a `<quest>` has no `id`.

### E-OBJECTIVE-MISSING-DONE

An `<objective>` has an empty `done` completion predicate and no `quest=` reference to delegate its completion.

### E-OBJECTIVE-QUEST-DONE

An `<objective>` carries both a `quest=` subquest reference and a non-empty `done=` predicate, which are mutually exclusive.

### E-OBJECTIVE-UNSATISFIABLE

A required `<objective>`'s `done` predicate can never decide true, or its `<objective quest="…">` references a subquest that is itself unreachable.

Spec: [dsl 0.4.0 §5.3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.4.0.md)

### E-OCCASION-GATE

A `lute play` step raises an occasion while its `raisedWhen` gate is false, or after the schema's `terminal:` condition already holds, so the engine would not actually raise it.

Spec: [dsl 0.27.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.27.0.md)

### E-OCCASION-UNKNOWN

A beat's or entry's `on=` names an occasion that no resolved plugin declares.

Spec: [dsl 0.21.0 §2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.21.0.md)

### E-ON-NO-EVENT

An `<on>` has no `event` attribute — every `<on>` must be anchored to a discrete event.

Spec: [dsl 0.2.0 §4.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.2.0.md)

### E-PATH-IDENT

A dotted state path has a segment after its leading tier that contains a hyphen, which is not a valid identifier character there.

Spec: [dsl §8.4](/spec/)

### E-PERMISSION-BRIDGE

A directive invokes a bridge service/operation the effective `bridges` permission ceiling forbids.

### E-PERMISSION-DIRECTIVE

A `::set`/`::assert`/`::retract` or plugin directive is forbidden by the effective `directives` permission ceiling.

### E-PERMISSION-FACT

A fact write — `::assert`/`::retract`, a plugin effect, or a seed fact — targets a relation the effective `factWrites` permission ceiling forbids.

### E-PERMISSION-PROFILE

`--permission-profile <name>` was passed without a loaded `lute.project.yaml` from `--project <DIR>` to resolve the profile against.

### E-PERMISSION-QUEST

A `<quest>` is declared where the effective `quests` permission ceiling forbids quest declarations.

### E-PERMISSION-REWARD

A `<reward>` is declared where the effective `rewards` permission ceiling forbids reward declarations.

### E-PERMISSION-STATE

A state write — a `::set`, a choice `into=`, a state/plugin default initialization, or an unresolved plugin write path — targets a path the effective `stateWrites` permission ceiling forbids.

### E-PERSIST-REMOVED

A directive uses the removed `persist` attribute — `into=` alone records the run fact.

Spec: [dsl 0.6.0 §2.2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.6.0.md)

### E-PLUGIN-ASSET-SEGMENT-TYPE

A plugin's `assetKinds` export declares a segment type outside the four a segment position admits — `enum`, `number`, `string`, or `providerRef`.

### E-PLUGIN-DUP-ACROSS

Two active plugins declare the same directive, event, reward kind, occasion, cast id, or bridge operation; the first plugin's declaration is used.

### E-PLUGIN-DUP-ID

A plugin package declares the same id more than once within one export kind; the first declaration is used and the rest of the project is still checked.

### E-PLUGIN-INVALID-DIRECTIVE

A plugin directive declaration uses a `semantics:` flag outside the closed vocabulary, or declares the same attribute name more than once.

### E-PLUGIN-IO

A plugin export file or directory could not be read due to an I/O or encoding failure.

### E-PLUGIN-KEY

A key in a plugin's `plugin.yaml` or one of its export files is not one that file takes (with the key meant, e.g. `dependencies` → `depends`), or `plugin.yaml` declares a `kind:` other than `capability`.

Spec: [dsl 0.28.0 §1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### E-PLUGIN-MANIFEST

A plugin package's `plugin.yaml` manifest is missing, is not valid YAML, or names no `id`/`version`/`kind`/`exports`.

### E-PLUGIN-MISSING-ACTIVE

A profile activates a plugin `id` that is not installed under the plugins directory, or whose package failed to load.

Spec: [dsl §11](/spec/)

### E-PLUGIN-MISSING-EXPORT

A plugin manifest's `exports:` entry names a path that does not exist on disk.

Spec: [dsl §4](/spec/), [dsl §11](/spec/)

### E-PLUGIN-OPTION-TYPE

A plugin activation's option value does not match the type its manifest declares for that option.

Spec: [plugin Appendix C1](/spec/)

### E-PLUGIN-OPTION-UNKNOWN

A plugin activation sets an option name the plugin's manifest never declares.

Spec: [plugin Appendix C1](/spec/)

### E-PLUGIN-PARSE

A plugin export file is not valid YAML or holds a value of the wrong shape (named with its line and the shape the key takes), or a directive's `effects.writes`/`effects.asserts`/`effects.retracts` entry names an attr the directive never declares (or an assert uses `_`).

Spec: [dsl 0.27.0 §2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.27.0.md), [dsl 0.27.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.27.0.md), [dsl 0.28.0 §7](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### E-PLUGIN-RESERVED-NAME

A plugin declares a name the core language owns: a directive named like a core statement (`set`, `assert`, `retract`, `accept`, `use`, `body`, `cut`), a core block tag (`scene`, `on`, `quest`, `objective`, `match`, `branch`, `hub`, `choice`, `when`, `otherwise`, `entry`, `beat`, `timeline`, `track`, `reward`, `return`) or a `lute.core` directive (`end`, `mark`, `bg`, …); an event or occasion named like an engine lifecycle event (`questComplete`, …) or a play step key; a cast id `narrator`. Reported at the declaration's file and line.

Every reserved name, where it is refused and what to write instead: [Reserved names](/reference/reserved-names/).

Spec: [dsl §10](/spec/), [dsl 0.28.0 §1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### E-PLUGIN-RESERVED-STAMP-ATTR

A non-core plugin's `stampAttrs` export or a directive's `attrs` declares an attribute name (`at`/`duration`/`delay`/`wait`/`timeline`/`provenance`/`source`) that the core stamp already owns.

Spec: [plugin §14](/spec/)

### E-PLUGIN-UNKNOWN-ASSETKIND

A directive binds an attribute to an asset kind that no active plugin declares.

Spec: [plugin §7](/spec/)

### E-PLUGIN-UNKNOWN-EXPORT

A plugin manifest's `exports:` key is not one of the export kinds (with a did-you-mean); the old spellings `rewardkinds`/`assetkinds`/`stampattrs` name `rewardKinds`/`assetKinds`/`stampAttrs`.

Spec: [plugin §4](/spec/), [dsl 0.28.0 §6](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### E-PLUGIN-UNKNOWN-REWARD-TARGET

A `rewardKinds:` entry pins `target: { provider: <name> }` to a provider no active plugin declares.

Spec: [dsl 0.16.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.16.0.md)

### E-PLURAL-FORM

A `{{n:plural(…)}}` hint whose forms are not a bare singular and a bare plural separated by `|` — quoted forms, a `,` separator, a missing or empty form.

Spec: [dsl 0.27.0 §7](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.27.0.md), [dsl 0.28.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### E-PROFILE-EXTENDS-CYCLE

A profile's `extends` chain loops back to itself.

Spec: [dsl §11](/spec/)

### E-PROFILE-UNKNOWN

The project selects a profile name that `lute.project.yaml` never declares.

Spec: [dsl §11](/spec/)

### E-PROJECT-CONFIG

The editor could not load the document's `lute.project.yaml` (a malformed or unreadable project manifest).

### E-QUEST-ID-DUP

A `<quest id=…>` repeats an id already used in this document, across its import graph, or project-wide.

Spec: [dsl 0.2.0 §6.3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.2.0.md)

### E-QUEST-ID-MISSING

A `<quest>` has no `id` attribute.

Spec: [dsl 0.2.0 §6.3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.2.0.md)

### E-QUEST-MULTI-PARENT

A subquest is referenced as a child by `<objective quest=…>` from two different parent quests, but a quest may have at most one parent.

### E-QUEST-REF-UNKNOWN

An `<objective quest=…>` names a child quest id that no quest in the project defines.

### E-QUEST-RESERVED-DECL

A `state:` declaration's path collides with an implicitly-declared reserved quest field (`quest.<id>.*` / `objectives.<oid>.done`).

Spec: [dsl 0.2.0 §5.2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.2.0.md)

### E-QUEST-RESERVED-WRITE

An `::set` writes a reserved, engine-populated path — `quest.<id>.state`, `objectives.<oid>.done`, an `entry.*` path, `prev.run.*`, `prev.season.*`, or `clock.*`.

Spec: [dsl 0.2.0 §5.4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.2.0.md), [dsl 0.19.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.19.0.md)

### E-QUEST-TIER-MIX

A subquest's effective `tier` differs from its parent quest's tier.

Spec: [dsl 0.23.0 §6](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.23.0.md)

### E-QUEST-TREE-CYCLE

The parent-child edges induced by `<objective quest=…>` close a cycle, including a quest naming itself as its own child.

### E-QUEST-UNREACHABLE

A `<quest>` can provably never complete because its `start` guard always decides false or its `fail` guard always decides true.

Spec: [dsl 0.4.0 §5.3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.4.0.md)

### E-REF-ARG-TYPE

A `@name(args)` call to a def passes an argument whose static type does not match the def's declared parameter type.

Spec: [dsl §8.1](/spec/)

### E-REF-ARITY

A `@name(args)` call supplies a different number of arguments than the def declares parameters.

Spec: [dsl §8.1](/spec/)

### E-REF-TYPE

A `@ref` produces a type incompatible with the CEL slot, component-arg, or `{{…}}` interpolation position it fills — including a non-renderable produced type or a `:number` format hint on a non-number.

Spec: [dsl §8](/spec/), [dsl §7.6](/spec/), [dsl 0.24.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md)

### E-RELATION-ARITY

A fact atom (a seed `facts:` entry, a rule body/head atom, an `::assert`/`::retract`, or a CEL fact query) supplies a different number of arguments than the relation declares.

Spec: [dsl 0.3.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md), [dsl 0.3.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md)

### E-RELATION-DECL

A relation declares `changedOn:` while not `reserved: true`, `changedOn:` names an occasion no schema declares, or an `excludes:` entry names a relation of incompatible argument kinds or fails its symmetry contract.

Spec: [dsl 0.25.0 §1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.25.0.md), [dsl 0.25.0 §6](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.25.0.md)

### E-RELATION-DOMAIN

A relation declares a field the schema does not recognize, an unknown `tier`, an out-of-range/duplicate `key:` index, or an argument domain that names no declared entity kind, enum, or domain.

Spec: [dsl 0.3.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md)

### E-RELATION-DUP

A relation name is declared more than once in a `relations:` block.

Spec: [dsl 0.3.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md)

### E-RELATION-EMPTY

A relation declares no `args:`.

Spec: [dsl 0.3.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md)

### E-RELATION-RESERVED-WRITE

A relation is declared both `derive: true` and `reserved: true`, giving it two conflicting write owners.

Spec: [dsl 0.3.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md), [dsl 0.3.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md)

### E-RELATION-UNKNOWN

A fact atom (a seed, rule, `::assert`/`::retract`, or CEL fact query) names a relation no schema declares.

Spec: [dsl 0.3.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md)

### E-RESERVED-NAME

A declared name is one the language keeps for itself — a state root naming an entity member, def or season, `unset`/`true`/`false`/`null`/`_` naming a member, `none` or a CEL literal naming an id, a CEL keyword in a state path or id that becomes one, a CEL call or rule word naming a relation, `narrator` in `cast:`, or a number in a member list — so the name would be read as that word where it is used. The message names a replacement; `lute --explain E-RESERVED-NAME` lists every reserved name.

Every reserved name, where it is refused and what to write instead: [Reserved names](/reference/reserved-names/).

Spec: [dsl 0.28.0 §1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### E-RETRACT-WILDCARD-ASSERT

A relation argument is `_` in a context other than a `::retract` pattern, which alone may contain wildcards.

Spec: [dsl 0.3.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md)

### E-REWARD-ATTR

A `<reward>` element is malformed: an empty/missing `kind`, an `amount=` that is not a signed integer or a valid `N..M` range, or an `outcome=` used on an objective-level reward or with a value other than `"failed"`.

Spec: [dsl 0.16.0 §2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.16.0.md), [dsl 0.16.0 §6](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.16.0.md)

### E-REWARD-KIND

A `<reward kind=…>` value names no reward kind declared in the resolved capability snapshot's `rewardKinds` vocabulary.

Spec: [dsl 0.16.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.16.0.md), [dsl 0.16.0 §6](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.16.0.md)

### E-REWARD-TARGET

A `<reward>`'s `target=` violates its reward kind's target contract: required but missing, or naming neither a declared entity-kind member nor a provider catalog id.

Spec: [dsl 0.26.0 §2.5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.26.0.md)

### E-RULE-AGGREGATE-CYCLE

A rule's `count(...)`/`countDistinct(...)` aggregate reads a relation that depends on the rule's own head, but an aggregate may only read a relation outside its head's own cycle.

Spec: [dsl §9](/spec/)

### E-RULE-EXCLUSIVE

A rule derives its head relation only where a positive body relation holds on the same arguments, but the two relations are declared mutually exclusive, so every derivation would violate that exclusion.

Spec: [dsl 0.25.0 §1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.25.0.md)

### E-RULE-GUARD-DEF

A rule's `cel("...")` guard cannot expand its `@def`/`@def(args)` references — the def is undeclared, its arity is wrong, or it is otherwise not usable in a guard.

### E-SEASON-DECL

A `seasons:` declaration is malformed (an entry that is not a map, a missing or empty `live`, an unknown key, a bad season name), two schemas declare one season differently, or a `season.<name>.*` path, `once: season:<name>` or `tier="season:<name>"` names an undeclared season, or a scene's legacy `season:` key (the episode number) holds a declared season's name; a write to `prev.season.*` is `E-QUEST-RESERVED-WRITE` instead.

Spec: [dsl 0.27.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.27.0.md)

### E-SET-OP-TYPE

An `::set`'s compound operator (`+=`/`-=`/`*=`) targets a path whose declared type is not `number`.

Spec: [dsl §7.3.4](/spec/)

### E-SET-SHAPE

An `::set` is malformed: it has no valid assignment operator (`=`/`+=`/`-=`/`*=`) after the path, uses `==` where `=` was meant, or indexes a state-family path with a param instead of a concrete key.

Spec: [dsl 0.27.0 §2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.27.0.md)

### E-SET-TYPE

An `::set`'s right-hand expression's decidable type does not match the type declared for the path it writes.

Spec: [dsl 0.10.0 §3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.10.0.md)

### E-STATE-COLLECTION

A `state:` declaration gives a path a collection type (`list`/`record`/`map`), but author state must be scalar.

Spec: [dsl 0.3.0 §3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md)

### E-STATE-DECL

A `state:` declaration is malformed: a non-string key, an unknown or incomplete `type:` (including an `enum` not nested under `type:`), a bad `default:`/`per:` shape, or `state:` itself is not a map.

Spec: [dsl 0.8.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.8.0.md)

### E-STATE-DECL-CONFLICT

Two `state:` declarations of the same path disagree on `type`, `default`, `per`, or `owner`, and neither refines the other via `extends:`.

Spec: [dsl §2](/spec/)

### E-STATE-MAYBE-UNAVAILABLE

A read of a state path is not guaranteed set by any declared `after:` route reaching this node (error grade), or is set on only some of those routes (warning grade).

Spec: [dsl §4.3](/spec/)

### E-STATE-NAMESPACE

A `state:` path does not begin with one of the recognized namespace roots `scene.`, `run.`, `user.`, `app.`, or `season.`.

### E-STATE-REDECLARE

A scene's inline `state:` declares or overrides a state path that an imported (`uses:`) schema already declares, which a scene must never redeclare.

Spec: [dsl §9.2](/spec/)

### E-STATE-SHAPE-CYCLE

A `state:` shape refers to itself, directly or through another shape, forming a cycle.

### E-STREAM-BODY

A streaming continuation ends mid-construct at the end of input, or its body carries frontmatter or a heading, which a continuation body cannot contain.

### E-STREAM-CLOSED

A streaming continuation is submitted after the continuation compiler has already closed.

### E-STREAM-PREFIX-CHANGED

Appended source in a streaming continuation would change commands or state that were already emitted for an earlier prefix of the input.

### E-STREAM-TEMPLATE

A streaming continuation's template is not a scene with at least one shot.

### E-STRING-ESCAPE

A quoted attribute value uses a backslash escape other than the four defined ones (`\"`, `\\`, `\n`, `\t`).

Spec: [dsl §4.4](/spec/)

### E-SUBQUEST-REARM

A quest that an `<objective quest=…>` names as a subquest declares `rearm=`; a subquest activates with its parent, so once the parent has ended a rearmed child stays `unset`.

Spec: [dsl 0.27.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.27.0.md), [dsl 0.28.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### E-TAG-INLINE-BODY

A block's body, and often its close, is written on the opener's own line; the opener, each body line and the `</tag>` close each need a line of their own.

Spec: [dsl §2.3](/spec/)

### E-TAG-NOT-ONE-LINE

A `<tag …>` opener's attributes wrap past its own physical line instead of staying on one line as the grammar requires.

Spec: [dsl §2.3](/spec/)

### E-TEMPLATE

A beat template is misused: `<beat use=>` names no component or one without a `beat:` header, a template `beat:` header is malformed or gives a header param a value it cannot take, a component declares a param named like a key of its use (`component` or `when`, or for a beat template a `<beat>` header key such as `title`, `once` or `id`) that no use could ever pass, or `::body` appears outside a template's top level.

Spec: [dsl 0.27.0 §6](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.27.0.md), [dsl 0.28.0 §1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### E-TEMPORAL-ARG

A narrative-time value (`now()` etc.) is used somewhere other than an ordering comparison against another narrative-time value or `validAt`'s second argument — as a bare value, in arithmetic, indexing, field access, a list literal, or with `!=`.

Spec: [dsl 0.3.0 §6](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md)

### E-TEST-FILE

A `*.test.yaml`'s `file:` names a document that does not exist.

### E-TEST-KEY

A `*.test.yaml` has an unrecognized top-level or `expect:`-level key, or a key that is not a string.

### E-TEST-LORE

A test's `file:` names a lore document, which is looked up rather than played, so the test must instead name what to present (`entry:`/`entries:`, `beat:`) or judge it with `expect:`.

Spec: [dsl 0.22.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.22.0.md)

### E-TEST-NEEDLE

A `*.test.yaml`'s `transcriptContains`/`transcriptLacks` needle names a speaker outside the project's cast, an attribute no transcript line shows, or a value outside its domain, so it could never match a presented line.

### E-TEST-NO-EXPECT

A `*.test.yaml` declares no recognized `expect:` key, so the test asserts nothing and cannot pass.

### E-TIME-RESOLUTION

An authored time value (a clip `at`, `duration`, `delay`, or `<timeline duration>`) carries more fractional precision than a millisecond.

Spec: [dsl 0.10.0 §10.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.10.0.md)

### E-TIMELINE-CONTENT

A `<timeline>` or `<track>` body contains non-staging content instead of only clip/staging elements.

Spec: [dsl §7.4](/spec/)

### E-TIMELINE-DURATION

A `<timeline duration>` is explicitly set below the maximum resolved end of its clips, which would truncate the timeline's own content.

Spec: [dsl §11.4](/spec/)

### E-TITLE-PLACEMENT

A document's `# ` title appears more than once, or appears after the first shot instead of before it.

Spec: [dsl §6.2](/spec/)

### E-TRACE-ACCEPT

A `--accept`/`accept:` entry names an unknown quest id, a quest that carries a `start` predicate (which activates declaratively, needing no accept), or a quest referenced by a parent's `<objective quest=…>` (a no-start child that activates via its parent).

Spec: [dsl 0.4.0 §4.3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.4.0.md), [dsl 0.4.0 §4.4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.4.0.md)

### E-TRACE-BEAT

`lute trace --beat <id>` targets a document that is not `kind: lore`, or names an id no `<beat>` in the document declares.

Spec: [dsl 0.23.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.23.0.md)

### E-TRACE-CHOICE

A `--choose` entry names an unknown branch/hub id or an unknown choice id for that branch/hub, either before the walk starts or because the choice's guard decides false when reached.

Spec: [dsl 0.4.0 §4.3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.4.0.md), [dsl 0.4.0 §4.4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.4.0.md)

### E-TRACE-ENTRY

`lute trace --entry <id>` targets a document that is not `kind: lore`, or names an id no `<entry>` in the document declares.

Spec: [dsl 0.19.0 §8](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.19.0.md)

### E-TRACE-EVENT

A `--event`/`events:` entry names a built-in lifecycle event (`questActive`, `questComplete`, `questFailed`), which is engine-derived and can never be user-fired.

Spec: [dsl 0.4.0 §4.3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.4.0.md), [dsl 0.4.0 §4.4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.4.0.md)

### E-TRACE-MOCK-FACT

A `--fact`/`facts:` entry, or a test's `expect.facts`/`expect.notFacts` atom, does not parse as a ground fact pattern, or names an unknown relation, wrong arity, or a foreign argument.

Spec: [dsl 0.4.0 §4.3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.4.0.md)

### E-TRACE-MOCK-PARSE

A `--mock`/`mocks/*.yaml` file is malformed — invalid YAML, not a mapping, an unrecognized top-level key, or a `state:`/`facts:`/`choose:`/`events:`/`quests:` section with the wrong shape.

Spec: [dsl 0.4.0 §4.3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.4.0.md), [dsl 0.10.0 §8](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.10.0.md)

### E-TRACE-MOCK-TYPE

A mock or test seed's literal (`--state <path>=<literal>`, `state:`, `quests:`) or a test's `expect.state` value is not compatible with the path's reserved domain or declared type — for a `{ domain: K }`, `{ entity: K }` or enum path, not one of its members — or an answered bridge result lacks a field that content reads.

Spec: [dsl 0.4.0 §4.3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.4.0.md), [dsl 0.24.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md), [dsl 0.27.0 §2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.27.0.md)

### E-TRACE-MOCK-UNDECLARED

A `--state <path>=…` seed names a path the clock derives (not seedable), a path not declared in the resolved schema, a path nothing in the document reads, or a bridge answer naming a call/field no directive reads or writes.

Spec: [dsl 0.4.0 §4.3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.4.0.md), [dsl 0.24.0 §1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md), [dsl 0.24.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md)

### E-TRACK-KEY

A `<track>` declares neither `subject`, `channel`, nor a `subject`+`property` pair, so it has no identifying key.

Spec: [dsl §7.4](/spec/)

### E-UNCLASSIFIED

A body line is no Lute construct — not a content line, directive, `::set` or known block — or a block stands where it cannot, such as a `<quest>` inside a shot.

Spec: [dsl 0.5.0 §2.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.5.0.md)

### E-UNCLOSED-TAG

A block is never closed — its body reaches the end of the file, a `## ` heading, or the close of an enclosing block before its own `</tag>` — or a `</tag>` closes no open block.

Spec: [dsl §5](/spec/), [dsl §7.3](/spec/)

### E-UNDECLARED

A CEL slot, `::set` target, or rule guard reads or writes a state path that no schema declares.

Spec: [dsl §7.3.4](/spec/), [dsl §9.4](/spec/)

### E-UNDECLARED-REF

A `@name` interpolation or guard reference names a `def` that no schema declares.

Spec: [dsl §8.1](/spec/)

### E-UNKNOWN-ATTR

A content line or directive carries an attribute key that the content-line grammar or the directive's own declaration does not recognize.

Spec: [dsl 0.1.0 §7.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.1.0.md)

### E-UNKNOWN-DIRECTIVE

A `::directive` names a tag no core or active plugin declares.

### E-UNKNOWN-EVENT

An `<on event="…">` names an event that resolves to neither a built-in lifecycle event nor a capability-declared world event.

Spec: [dsl 0.2.0 §4.5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.2.0.md)

### E-UNKNOWN-ID

An attribute referencing a `providerRef` id names an id absent from the pinned provider catalog.

### E-UNKNOWN-KIND

A document's `kind:` frontmatter key has a value other than `scene`, `quest`, or `lore`.

Spec: [dsl 0.2.0 §3.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.2.0.md), [dsl 0.19.0 §2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.19.0.md)

### E-UNSET-LITERAL

A CEL guard slot compares a maybe-unset finite-domain subject to the foreign string literal `'unset'`, the most common misspelling of the DSL's actual unset sentinel.

Spec: [dsl 0.5.2 §2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.5.2.md)

### E-UNSET-UNCOVERED

A `<match>` subject that may be unset (a `run.`/`user.`/`app.` path with no schema `default`; a `scene.*` path, including a branch's `scene.choices.*` record, is judged per path as `E-MAYBE-UNSET`) is not covered by an `unset`-matching arm or an `<otherwise>`.

Spec: [dsl §11.2](/spec/)

### E-USES-CYCLE

A document's `uses:`/`extends:` imports form a directed cycle.

### E-USES-DUP-DEF

Two peer imports at the same import depth declare the same `def` name differently.

### E-USES-DUP-RELATION

Two peer imports at the same import depth declare the same relation or enum name differently.

### E-USES-DUP-STATE

Two peer imports at the same import depth declare the same state path differently.

### E-USES-NOT-FOUND

A `uses:`/`extends:` import names a path that cannot be resolved or read.

### E-USES-PARSE

A `uses:`/`extends:` import's target document has parse or frontmatter errors of its own.

### E-VALIDAT-DERIVED

`validAt` is used over a derived relation, whose rule closure carries a CEL guard and so keeps no single well-defined timestamp.

Spec: [dsl §8](/spec/)

### E-WHEN-LITERAL-DOMAIN

A `<when is="…">` literal falls outside the subject's decided finite domain — a foreign enum member (a typo), a number/bool literal against a mismatched domain, or `unset` on a subject that is never unset.

Spec: [dsl 0.4.0 §5.2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.4.0.md), [dsl §6.3](/spec/)

### E-WHEN-PATTERN

A `<when>` arm carries neither an `is` literal pattern nor a `test` guard, but one of the two is required.

Spec: [dsl §7.3.1](/spec/)

### E-WHEN-RANGE

A `<when is="…">` alternative contains `..` but is a malformed or empty range literal (for example `..`, `a..b`, `1...2`, or `3..1`).

Spec: [dsl 0.18.0 §2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.18.0.md)

### E-WHEN-UNSET-SUBJECT

A `<when is="unset">` arm needs its `<match>` subject to be a plain state path, and this subject is an expression.

Spec: [dsl §7.3.1](/spec/)

### E-WRITE-CONFLICT

Two `<clip>`s on different `<track>`s of a `<timeline>` write overlapping state targets at overlapping times.

Spec: [dsl §11.4](/spec/)

## Warnings

### W-ASSET-PLACEHOLDER

An asset id looks like a placeholder that should be resolved before release.

### W-BEAT-ONCE-RUN-USER

A scene beat's `once` defaults to `run`, but its `when` reads only user-tier state, so it replays every run unless `once` is authored explicitly.

Spec: [dsl 0.22.0 §13](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.22.0.md), [dsl 0.23.1](/spec/)

### W-BEAT-PRIORITY-TIE

Two or more beats on one `select: first` occasion share the same `priority` and can be eligible at the same time, so which one wins falls to file order.

Spec: [dsl 0.27.0 §11](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.27.0.md), [dsl 0.22.0 §13](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.22.0.md)

### W-BEAT-SHADOWED

A `select: first` beat can never win its occasion because an earlier-ordered, always-eligible, never-spent beat with the same or absent target always wins first.

Spec: [dsl 0.21.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.21.0.md)

### W-BEAT-SPENT-AT-START

A beat's `spentBy` already holds at the start of play (every state path at its default, only the seed facts) — often `spentBy` read as "repeat while", or an inverted `!holds(…)` copied from an old `when` — so the beat is spent before it can play: a `spentBy` beat stays spent once its condition has held.

Spec: [dsl 0.27.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.27.0.md), [dsl 0.28.0 §6](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### W-BRANCH-ID-SHARED

Two documents of one project each declare a `<branch>` or `<hub>` with the same id. Ids need only be unique within a document, but a play's or test's `choose:` names a menu by its id alone, so one key answers both menus (and a list of decisions is consumed across both).

Spec: [dsl 0.28.0 §7](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### W-CAST-ABSENT

A content line's speaker has a cast entry declaring a `present:` condition, but the line's enclosing guards do not imply that condition holds.

Spec: [dsl 0.24.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md)

### W-CATALOG-STALE

A `providerRef` id is not found in the pinned provider catalog, which may mean the snapshot is stale or offline rather than the id being wrong.

Spec: [dsl §7.2](/spec/)

### W-CHAPTER-ORDER

On a `select: sequence` occasion, where a chain of the project's `chapters:` is the order within one raise, a listed scene writes its own `priority:`, which places it out of the order the chain lists. Remove the scene's `priority:`, or move it in the chain's `scenes:`.

Spec: [dsl 0.28.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### W-CHAPTER-STALL

A scene listed in a chain of the project's `chapters:` has its own `when:` that can stay false for good — it reads state the story may never set, not only the clock — and the next listed scene's `after:` (the one the chain writes, or one it wrote itself) waits on it, so the chapters can stop there. A condition over the clock alone only delays the chain and is not reported. If the scene may be skipped, let the next one follow the scene before it (`after: visited("<previous>")`; the skipped one still plays first while eligible, as it ranks higher); if it must play, make sure the story makes its condition true.

Spec: [dsl 0.28.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### W-CODE-AFTER-END

Content follows an `::end` directive in the same straight-line body, but the walk already terminated there so nothing after it can run.

Spec: [dsl 0.8.0](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.8.0.md)

### W-CODE-AFTER-NEXT

Content follows an unguarded `::next` directive in the same straight-line body, but the jump leaves that body so nothing after it can run.

Spec: [dsl 0.12.0](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.12.0.md)

### W-COMPONENT-UNVERIFIED

A standalone component check has no caller in scope — either no project was resolved, or the resolved project has no document that `::use`s the component — so the verdict covers only the component's own frontmatter and body.

Spec: [dsl 0.10.0 §9](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.10.0.md)

### W-DEADLINE-BEFORE-DONE

An `on=` objective's `by=` deadline (with no `until=`) provably implies before its `done` predicate can ever be judged, so the deadline fails the objective before it can complete.

Spec: [dsl 0.24.0 §2.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md)

### W-DEADLINE-BEFORE-WINDOW

An objective's `done` can only hold at clock positions where its `by=` deadline already holds — typically a `visited` beat whose `when` opens after the deadline — so the deadline fails the objective before it can be done.

Spec: [dsl 0.28.0](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### W-DEADLINE-NEVER

An objective's `by=` deadline can never hold — typically a moment past the end of a clock that ends — so it never fails the objective.

Spec: [dsl 0.24.0 §2.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md)

### W-DEF-UNUSED

A declared `@def` is never referenced by an `@name` use anywhere in the project's content, other defs, or rule guards.

Spec: [dsl 0.24.0](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md)

### W-DERIVE-NO-RULES

A relation declared `derive: true` has no rules that produce it, so it is legal but permanently empty — almost always a typo'd rule head.

Spec: [dsl 0.3.0 §7.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md)

### W-DISPLAY-NAME-DUP

Two different speakers show the same dialogue display name (from a cast entry's `name:` or a component `::use{name=}`), so the player cannot tell them apart.

Spec: [dsl 0.26.0 §2.8](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.26.0.md)

### W-DOMAIN-UNREAD

A declared domain is not read by any active construct — no directive attribute, content-line slot, state path, `relations:` argument, `per:`/`subsetOf:` family, or rule/condition query names it — so it enforces nothing.

Spec: [dsl 0.10.0 §11.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.10.0.md)

### W-ENTRY-REF-UNKNOWN

An `entry.<id>.read` reference names an entry id no lore document in the project declares.

Spec: [dsl 0.19.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.19.0.md)

### W-ENTRY-WRITE-REREAD

An entry that can be read again in a run (a lookup entry, an entry beat without `once`, a `once` shorter than the run, a `spentBy` entry, a `for=` entry without `once: run|user`) writes state, but an entry's writes apply on its first read in a run only; the message names the remedy for its shape (a `<beat>` with the same attributes, `once="run"`, or a `when="!entry.<id>.read"` guard, which also silences it).

Spec: [dsl 0.26.0 §8](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.26.0.md), [dsl 0.19.0 §6](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.19.0.md)

### W-EXIT-INERT

A content line's `action` names a declared exit member of the `action` domain, but on a content line it has no staging effect — the character stays on stage.

Spec: [dsl 0.10.0 §11.2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.10.0.md)

### W-FACT-GUARANTEED

A guard's relational query (`holds`/`count`) is guaranteed true on every route that reaches it, making the condition redundant.

Spec: [dsl 0.20.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.20.0.md)

### W-INTO-SET-DUP

A `<choice>` arm both `::set`s a path and records the same path via `into=`, recording it twice.

Spec: [dsl 0.6.0 §2.2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.6.0.md)

### W-L10N-MISSING

A compiled line record is missing text for a locale its localization bundle declares.

Spec: [dsl 0.8.0 §7](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.8.0.md)

### W-LUTE-VERSION-STALE

A document's `luteVersion` frontmatter stamp is present but differs from the toolchain's current DSL version, suggesting it was copied from an older example.

Spec: [dsl 0.6.1 §3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.6.1.md)

### W-META-LEGACY

A frontmatter document authors a legacy scene-identity key (such as `character`, `season`, or `episode`) alongside `id:`, but `id:` now carries scene identity so the legacy key should be removed.

Spec: [dsl 0.15.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.15.0.md)

### W-OBJECTIVE-HIDDEN

A required (`!optional`) objective's `visibleWhen` visibility gate provably never holds, so it can never be visible or tracked even though it still gates quest completion.

Spec: [dsl 0.4.0 §5.3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.4.0.md)

### W-OTHERWISE-DEAD

A `<match>`'s `<otherwise>` arm is provably unreachable because earlier unguarded `is` arms already cover the subject's whole domain.

Spec: [dsl 0.4.0 §5.2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.4.0.md)

### W-OVERLAP-ARMS

Two `<when>` arms provably match the same value, so the later arm is unreachable under first-match-wins ordering.

Spec: [dsl §11.2](/spec/), [dsl 0.18.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.18.0.md)

### W-PROJECT-INERT

A manifest does not govern under the forced `--project` root and would have resolved differently, so its settings are not applied to any document.

### W-QUEST-HANDLER-DEAD

A quest's `<on event="questFailed">` handler never runs because the quest can never fail — no `fail` condition, no required objective with a `by=` deadline, no failing required subquest, and no parent quest that could cascade-fail it.

Spec: [dsl 0.22.0 §7](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.22.0.md)

### W-QUEST-NEVER-ACCEPTED

An accept-driven quest is never named by any `::accept{quest=…}` and is not `accept="external"`, so nothing in the project ever accepts it.

Spec: [dsl 0.24.0 §2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md), [dsl 0.25.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.25.0.md)

### W-QUEST-REARM-CONSTANT

A quest's `rearm=` condition is constant (`"true"`, `"false"`, or a def or comparison that folds to one), so it never turns from false to true and the quest never rearms.

Spec: [dsl 0.27.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.27.0.md), [dsl 0.28.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### W-QUEST-REF-UNKNOWN

A reserved `quest.<id>.state` / `quest.<id>.objectives.<oid>.done` reference (or similar) names a quest id or objective id no quest document in the project defines.

Spec: [dsl 0.5.1 §1.4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.5.1.md)

### W-QUEST-STATE-ISSET

An `isSet(quest.<id>.state)` guard is always true, since a quest's state is always assigned — `unset` until activation, then a real state — so the check tests nothing.

### W-QUEST-TIER-IMPLICIT

A quest with no `tier=` (so it defaults to user-tier, persisting across runs) reads only run-tier state in its conditions — `run.*`, `clock.*` over a `run.*` day, run-tier facts, or subquests that are run-tier or flagged too, but not `visited()`, which a new run keeps — suggesting it (and its quest tree) was meant to reset each run.

### W-RELATION-TIER-IMPLICIT

A stored (not `derive: true`) relation declares no `tier:`, so it is run-tier: its facts — the engine's too, on a `reserved: true` relation — are cleared at every new run. Write `tier: run` to keep that, or `user`, `app` or `season:<name>` for facts that outlive the run.

Spec: [dsl 0.28.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### W-RELATION-UNREAD

A declared, non-reserved relation is written (asserted, seeded, or derived) but never read by any condition, rule body, or def — the facts it records change nothing.

Spec: [dsl 0.24.0](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md)

### W-REWARD-DOUBLE-CREDIT

A quest handler's `::set` writes the same path a `<reward kind="…" credits=…>` already credits when granted, so the reward pays twice.

Spec: [dsl 0.23.0 §8](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.23.0.md)

### W-SEASON-UNGATED

A beat with `once: season:<name>` (or a `tier="season:<name>"` quest with a `start`) whose `when` (or `start`) does not imply the season's `live` condition: `once` only sets how long the beat stays spent, so it plays even while the season has never opened. Add the season's `live` condition (or a def that reads it) to the `when`.

Spec: [dsl 0.28.0 §7](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### W-STAGE-ABSENT

A content line or `::auto` targets a character who already left the stage (via a declared exit, a `::bg` scene change, or `::clear`) and was never re-shown, so the staging is impossible.

Spec: [dsl 0.22.0 §12](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.22.0.md)

### W-TEMPLATE-DOT-PARAM

A beat template's `when:` or `spentBy:` header reads a member as a path segment spelled with a param (`user.bond.@who`). It works in a header, but a component body refuses that spelling; write `user.bond[@who]`, which both accept.

Spec: [dsl 0.28.0 §3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### W-TEMPLATE-OVERRIDE

A `<beat use=…>` writes its own `when=`, which replaces the template's `when:` whole, so the template's condition no longer gates the beat (and an argument only that condition read is unused). Write both conditions in the use's `when=`, or give the template a param to conjoin (`when: "<condition> && (@only)"`) and pass it instead.

Spec: [dsl 0.28.0 §3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### W-TERMINAL-PERSISTENT

The schema's `terminal:` reads state a new run keeps (`visited(…)`, `user.*`, `app.*`, `entry.<id>.everRead`, a user-tier quest or relation), so once it holds no new run can play on.

Spec: [dsl 0.28.0 §2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### W-TEXT-BRACKET-LABEL

A `<choice>` label is wrapped in `[…]`, Ink's bracket suppression. Lute shows a label exactly as written, so the brackets appear on the button; write the label without them.

Spec: [dsl 0.28.0 §2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### W-TEXT-COMMENT-LIKE

Line text or a choice label holds a ` // …` comment or ends in an Ink `#tag`. Text after `: ` is literal, so the player sees it; a comment is `// …` on a line of its own, and Lute has no line tags.

Spec: [dsl 0.28.0 §2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### W-TEXT-LOOKS-LIKE-REF

A content line's whole text is exactly `@<name>` for a declared def or component param, which ships as the literal string `"@<name>"` instead of being resolved.

Spec: [dsl §7.6](/spec/)

### W-TEXT-SINGLE-BRACE

Line text or a choice label holds a single-brace group that reads as another language's markup: a state path or def (`{run.oil}`), a Yarn `{$var}`, Ink conditional text (`{cond: text}`) or alternatives (`{~a|b}`). Single braces are literal; interpolation is `{{run.oil}}`, and conditional text is a guarded line or a `<match>`.

Spec: [dsl 0.28.0 §2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### W-TIMELINE-CLIPS

A `<timeline>` track has more than 12 clips, which the checker suggests splitting.

Spec: [dsl §11.4](/spec/)

### W-TIMELINE-TOTAL

A `<timeline>` has more than 40 clips across all its tracks combined, which the checker suggests splitting.

Spec: [dsl §11.4](/spec/)

### W-TIMELINE-TRACKS

A `<timeline>` has more than 8 tracks, which the checker suggests splitting.

Spec: [dsl §11.4](/spec/)

### W-TRACE-MOCK-UNPRODUCIBLE

A supplied `--fact`/mock-YAML fact's relation is judged not producible by any authored producer, so a walk seeded with it proves nothing about reachable play.

Spec: [dsl 0.6.1 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.6.1.md)

### W-WHEN-TEST-LITERAL

A `<when test="…">` arm is written as a CEL literal comparison that the `is=` pattern form would say more clearly and that the checker can reason about directly.

Spec: [dsl 0.18.0 §3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.18.0.md), [dsl §7.3.1](/spec/)

### W-WIP

Under `check-project --wip`, a guard or objective is dead only because a relation it needs has no producer written yet (no seed, `::assert`, rule, or reserved declaration, or only a component `::assert` with an unbound `@param`); the message names the error code it is without `--wip`: `E-ARM-DEAD`, `E-BEAT-UNREACHABLE`, `E-ENTRY-UNREACHABLE`, or `E-OBJECTIVE-UNSATISFIABLE`.

Spec: [dsl 0.23.0 §10](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.23.0.md), [dsl 0.26.0 §2.6](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.26.0.md)
